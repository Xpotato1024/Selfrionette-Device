use super::*;
use crate::TransportError;
use std::{
    collections::VecDeque,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
const DEVICE_ID: &str = "srn-0123456789abcdef0123456789abcdef";

#[test]
fn commands_are_strictly_typed_and_bounded_by_the_io_owner() {
    let valid = r#"{"schema":"selfrionette-bridge-command/v1","request_id":1,"action":"status"}"#;
    assert!(serde_json::from_str::<BridgeRequest>(valid).is_ok());
    for invalid in [
        valid.replace(
            "\"action\":\"status\"",
            "\"action\":\"status\",\"unexpected\":true",
        ),
        valid.replace(
            "\"action\":\"status\"",
            "\"action\":\"tare\",\"confirmed\":\"true\"",
        ),
        valid.replace(
            "\"action\":\"status\"",
            "\"action\":\"tare\",\"confirmed\":true,\"unexpected\":true",
        ),
        valid.replace(
            "\"action\":\"status\"",
            "\"action\":\"tare\",\"confirmed\":null",
        ),
        valid.replace("\"request_id\":1", "\"request_id\":1,\"request_id\":2"),
        valid.replace("\"request_id\":1", "\"request_id\":1.5"),
    ] {
        assert!(
            serde_json::from_str::<BridgeRequest>(&invalid).is_err(),
            "{invalid}"
        );
    }
}

struct Clock {
    origin: Instant,
    micros: AtomicU64,
}
impl Clock {
    fn new() -> Self {
        Self {
            origin: Instant::now(),
            micros: AtomicU64::new(0),
        }
    }
    fn elapsed(&self) -> u64 {
        self.micros.load(Ordering::SeqCst)
    }
    fn set(&self, micros: u64) {
        self.micros.store(micros, Ordering::SeqCst);
    }
}
impl MonotonicClock for Clock {
    fn now(&self) -> Instant {
        self.origin + Duration::from_micros(self.elapsed())
    }
}

struct DeviceState {
    device_id: String,
    replies: VecDeque<String>,
    writes: Vec<String>,
    streaming: bool,
    session: u32,
    sequence: u32,
    tare_epoch: u32,
    tare_valid: bool,
    next_us: u64,
    values: [i32; 7],
    flags: [u8; 7],
    dropped: Arc<AtomicBool>,
}
impl DeviceState {
    fn new() -> Self {
        Self {
            device_id: DEVICE_ID.into(),
            replies: VecDeque::new(),
            writes: Vec::new(),
            streaming: false,
            session: 0,
            sequence: 0,
            tare_epoch: 1,
            tare_valid: false,
            next_us: 0,
            values: [100; 7],
            flags: [1; 7],
            dropped: Arc::new(AtomicBool::new(false)),
        }
    }
}
struct Transport {
    state: Arc<Mutex<DeviceState>>,
    clock: Arc<Clock>,
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.state
            .lock()
            .unwrap()
            .dropped
            .store(true, Ordering::SeqCst);
    }
}
impl LineTransport for Transport {
    fn discard_input(&mut self) -> Result<(), TransportError> {
        self.state.lock().unwrap().replies.clear();
        Ok(())
    }
    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        let mut state = self.state.lock().unwrap();
        state.writes.push(line.into());
        let fields: Vec<_> = line.trim().split(',').collect();
        let request = fields[1];
        let reply = match fields[0] {
            "?3" => format!(
                "info3,{request},1.2.0,{REVISION},sparkfun_promicro16,atmega32u4,16000000,{},{},2,127,1,{},{}",
                state.device_id,
                2,
                state.tare_epoch,
                u8::from(state.tare_valid)
            ),
            "stream3" => {
                let session: u32 = fields[2].parse().unwrap();
                if state.session != session {
                    state.session = session;
                    state.tare_valid = false;
                    state.sequence = 0;
                }
                state.streaming = fields[3] == "1";
                state.next_us = self.clock.elapsed() + 12_500;
                format!("ok3,{request},stream,{session}")
            }
            "tare3" => {
                state.tare_epoch += 1;
                state.tare_valid = true;
                format!("ok3,{request},tare,{}", state.tare_epoch)
            }
            other => panic!("bridge sent an unapproved command: {other}"),
        };
        state.replies.push_back(reply);
        Ok(())
    }
    fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError> {
        let mut state = self.state.lock().unwrap();
        if let Some(reply) = state.replies.pop_front() {
            return Ok(reply);
        }
        let deadline = self.clock.elapsed() + timeout.as_micros() as u64;
        if !state.streaming || state.next_us > deadline {
            self.clock.set(deadline);
            return Err(TransportError::Timeout);
        }
        let at = state.next_us.max(self.clock.elapsed());
        self.clock.set(at);
        let timestamp_ms = state.next_us / 1000;
        state.next_us += 12_500;
        state.sequence += 1;
        let mut fields = vec![
            "sample3".into(),
            state.session.to_string(),
            state.sequence.to_string(),
            timestamp_ms.to_string(),
            state.tare_epoch.to_string(),
            "2".into(),
            u8::from(state.tare_valid).to_string(),
        ];
        fields.extend([1000; 7].map(|value| value.to_string()));
        fields.extend(state.values.map(|value| value.to_string()));
        fields.extend(state.flags.map(|value| value.to_string()));
        Ok(fields.join(","))
    }
}
fn config(path: PathBuf) -> BridgeConfig {
    BridgeConfig {
        port: "fixture-only".into(),
        expected_device_id: DEVICE_ID.into(),
        profile_path: path,
        operator: "operator-1".into(),
        posture: "neutral posture".into(),
        grip: "comfortable grip".into(),
        method: MethodDto::Diagonal,
    }
}
fn request(id: u64, command: BridgeCommand) -> BridgeRequest {
    BridgeRequest {
        schema: COMMAND_SCHEMA.into(),
        request_id: id,
        command,
    }
}
fn status(event: BridgeEvent) -> BridgeStatus {
    match event {
        BridgeEvent::Status(status) => status,
        _ => panic!("status expected"),
    }
}

#[test]
fn explicit_confirmations_revision_and_identity_gate_live_access() {
    let profile = crate::app::tests::saved_profile_fixture();
    let clock = Arc::new(Clock::new());
    let mut session = BridgeSession::new("unknown", clock.clone(), clock.origin);
    let mut no_open = |_port: &str| -> Result<Box<dyn LineTransport + Send>, String> {
        panic!("must validate before serial access")
    };
    session
        .handle(
            request(
                1,
                BridgeCommand::Configure {
                    config: config(profile.clone()),
                },
            ),
            &mut no_open,
        )
        .unwrap();
    assert!(
        session
            .handle(
                request(2, BridgeCommand::Connect { confirmed: false }),
                &mut no_open
            )
            .unwrap_err()
            .contains("confirmation")
    );
    assert!(
        session
            .handle(
                request(3, BridgeCommand::Connect { confirmed: true }),
                &mut no_open
            )
            .unwrap_err()
            .contains("revision is unknown")
    );
    assert!(
        session
            .handle(request(3, BridgeCommand::Status), &mut no_open)
            .is_err()
    );
    assert!(
        session
            .handle(request(4, BridgeCommand::Start), &mut no_open)
            .is_err()
    );
    assert!(session.tick().unwrap().is_empty());
    assert!(!valid_revision(&"0".repeat(40)));
    assert!(!valid_revision(&REVISION.to_uppercase()));

    let device = Arc::new(Mutex::new(DeviceState::new()));
    device.lock().unwrap().device_id = "srn-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    let mut session = BridgeSession::new(REVISION, clock.clone(), clock.origin);
    let mut open = |_port: &str| -> Result<Box<dyn LineTransport + Send>, String> {
        Ok(Box::new(Transport {
            state: device.clone(),
            clock: clock.clone(),
        }))
    };
    session
        .handle(
            request(
                1,
                BridgeCommand::Configure {
                    config: config(profile.clone()),
                },
            ),
            &mut open,
        )
        .unwrap();
    let error = session
        .handle(
            request(2, BridgeCommand::Connect { confirmed: true }),
            &mut open,
        )
        .unwrap_err();
    assert!(error.contains("identity"));
    let failure = session.fail(error);
    assert!(matches!(
        failure,
        BridgeEvent::Error(BridgeFailure {
            ready: false,
            fatal: true,
            ..
        })
    ));
    assert!(device.lock().unwrap().dropped.load(Ordering::SeqCst));
    assert_eq!(
        device.lock().unwrap().writes.len(),
        1,
        "identity mismatch must not tare or stream"
    );
    std::fs::remove_dir_all(profile.parent().unwrap().parent().unwrap()).unwrap();
}

#[test]
fn saved_profile_drives_explicit_neutral_then_each_80_hz_frame_without_replay() {
    let profile = crate::app::tests::saved_profile_fixture();
    let clock = Arc::new(Clock::new());
    let device = Arc::new(Mutex::new(DeviceState::new()));
    let mut open = |_port: &str| -> Result<Box<dyn LineTransport + Send>, String> {
        Ok(Box::new(Transport {
            state: device.clone(),
            clock: clock.clone(),
        }))
    };
    let mut session = BridgeSession::new(REVISION, clock.clone(), clock.origin);
    let configured = status(
        session
            .handle(
                request(
                    1,
                    BridgeCommand::Configure {
                        config: config(profile.clone()),
                    },
                ),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(configured.phase, BridgePhase::Configured);
    assert_eq!(configured.request_id, 1);
    assert!(configured.source.is_none());
    let connected = status(
        session
            .handle(
                request(2, BridgeCommand::Connect { confirmed: true }),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(connected.source.unwrap().device_id, DEVICE_ID);
    assert!(!connected.ready);
    let tared = status(
        session
            .handle(
                request(3, BridgeCommand::Tare { confirmed: true }),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(tared.phase, BridgePhase::AwaitingNeutral);
    assert!(!tared.session_neutral_valid);
    assert!(
        !device.lock().unwrap().streaming,
        "wait for separate posture confirmation at producer"
    );
    assert!(session.tick().unwrap().is_empty());
    let neutral = status(
        session
            .handle(
                request(4, BridgeCommand::Neutral { confirmed: true }),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(neutral.phase, BridgePhase::CapturingNeutral);
    let mut completed = false;
    for _ in 0..100 {
        for event in session.tick().unwrap() {
            let view = status(event);
            assert_eq!(view.request_id, 4);
            assert_eq!(view.phase, BridgePhase::NeutralReady);
            assert!(view.session_neutral_valid);
            assert!(!view.ready);
            completed = true;
        }
        if completed {
            break;
        }
    }
    assert!(completed);
    let applied = status(
        session
            .handle(
                request(5, BridgeCommand::Apply { confirmed: true }),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(applied.phase, BridgePhase::Applied);
    assert!(
        !applied.ready,
        "application waits for first fresh neutral sample"
    );
    for _ in 0..4 {
        assert!(session.tick().unwrap().is_empty());
    }
    let prepared = status(
        session
            .handle(request(6, BridgeCommand::Status), &mut open)
            .unwrap(),
    );
    assert!(prepared.ready);
    assert_eq!(
        prepared.profile.as_ref().unwrap().method,
        MethodDto::Diagonal
    );
    assert_eq!(prepared.profile.as_ref().unwrap().assembly_revision, 2);
    session
        .handle(request(7, BridgeCommand::Start), &mut open)
        .unwrap();
    device.lock().unwrap().values[0] = 110;
    let expected_start_sequence = device.lock().unwrap().sequence + 1;
    let mut events = Vec::new();
    while events.len() < 80 {
        events.extend(session.tick().unwrap());
    }
    assert_eq!(events.len(), 80);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../examples/host-bridge/sample-event-v1.json"
    ))
    .unwrap();
    for (index, event) in events.iter().enumerate() {
        let wire = serde_json::to_value(event).unwrap();
        assert_eq!(
            wire.as_object().unwrap().keys().collect::<Vec<_>>(),
            fixture.as_object().unwrap().keys().collect::<Vec<_>>()
        );
        let BridgeEvent::Sample(sample) = event else {
            panic!("sample expected")
        };
        assert_eq!(sample.sequence, expected_start_sequence + index as u32);
        assert!(sample.ready && !sample.missing);
        assert_eq!(sample.normalized[0], Some(0.5));
        assert!((sample.command[0].unwrap() - (10.0 - 0.3) / (20.0 - 0.3)).abs() < 1e-12);
        assert_eq!(sample.settings_generation, 2);
        assert_eq!(sample.tare_epoch, 2);
        assert_eq!(sample.quality_flags, [1; 7]);
        assert_eq!(sample.source.source_sha, REVISION);
        assert!(!sample.hardware_acceptance);
        assert!(sample.header.host_emitted_us >= sample.host_received_us);
    }
    let previous_sequence = device.lock().unwrap().sequence;
    let diagnostic = status(
        session
            .handle(request(8, BridgeCommand::Status), &mut open)
            .unwrap(),
    );
    assert_eq!(
        device.lock().unwrap().sequence,
        previous_sequence,
        "status must not fabricate or read input"
    );
    assert!(diagnostic.diagnostics.unwrap().normalized_frames >= 80);
    let mut stale = events[0].clone();
    let BridgeEvent::Sample(sample) = &stale else {
        unreachable!()
    };
    stale.stamp_emitted(sample.host_received_us + 251_000);
    let BridgeEvent::Sample(stale) = stale else {
        unreachable!()
    };
    assert!(!stale.ready && stale.missing);
    assert_eq!(stale.command, [None; 7]);
    assert_eq!(stale.active, None);
    device.lock().unwrap().flags[2] = 2;
    let error = (0..3)
        .find_map(|_| session.tick().err())
        .expect("bad quality must invalidate");
    session.fail(error);
    assert!(device.lock().unwrap().dropped.load(Ordering::SeqCst));
    assert_eq!(session.phase, BridgePhase::Failed);
    assert!(!status(session.status()).ready);
    assert!(!device.lock().unwrap().streaming);
    let stopped = status(
        session
            .handle(request(9, BridgeCommand::Stop), &mut open)
            .unwrap(),
    );
    assert_eq!(stopped.phase, BridgePhase::Stopped);
    assert!(stopped.source.is_none());
    let reconnected = status(
        session
            .handle(
                request(10, BridgeCommand::Connect { confirmed: true }),
                &mut open,
            )
            .unwrap(),
    );
    assert_eq!(reconnected.phase, BridgePhase::Connected);
    assert!(!reconnected.ready && !reconnected.session_neutral_valid);
    assert!(reconnected.profile.is_none());
    assert!(
        session
            .handle(request(11, BridgeCommand::Start), &mut open)
            .is_err()
    );
    session.shutdown().unwrap();
    std::fs::remove_dir_all(profile.parent().unwrap().parent().unwrap()).unwrap();
}
