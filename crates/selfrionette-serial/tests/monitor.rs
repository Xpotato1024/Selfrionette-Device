use selfrionette_core::{DeviceId, DeviceIdentity, ProtocolFrame};
use selfrionette_serial::{
    DeviceSession, LineTransport, MonitorError, SessionError, SessionLimits, TransportError,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;
use std::rc::Rc;
use std::time::Duration;

const ID: &str = "srn-0123456789abcdef0123456789abcdef";
const INFO: &str = "device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,7";
const VECTOR: &str = "vector,1,1,2,3,4,5,6,7";

#[derive(Default)]
struct Trace {
    writes: Vec<String>,
    discards: usize,
    reads: usize,
    timeouts: Vec<Duration>,
}
struct FakeTransport {
    queued: VecDeque<String>,
    response: VecDeque<String>,
    trace: Rc<RefCell<Trace>>,
}
impl LineTransport for FakeTransport {
    fn discard_input(&mut self) -> Result<(), TransportError> {
        self.queued.clear();
        self.trace.borrow_mut().discards += 1;
        Ok(())
    }
    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        self.trace.borrow_mut().writes.push(line.to_owned());
        self.queued.append(&mut self.response);
        Ok(())
    }
    fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError> {
        let mut trace = self.trace.borrow_mut();
        trace.reads += 1;
        trace.timeouts.push(timeout);
        self.queued.pop_front().ok_or(TransportError::Timeout)
    }
}
fn session(
    lines: &[&str],
    limits: SessionLimits,
) -> (DeviceSession<FakeTransport>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let transport = FakeTransport {
        queued: VecDeque::from([VECTOR.to_owned()]),
        response: lines.iter().map(|line| (*line).to_owned()).collect(),
        trace: trace.clone(),
    };
    (DeviceSession::with_limits(transport, limits), trace)
}
fn id() -> DeviceId {
    ID.parse().unwrap()
}

#[test]
fn golden_capture_binds_metadata_preserves_wrap_and_separates_diagnostics() {
    let fixture = include_str!("../../../tests/fixtures/protocol-v2-monitor.txt");
    let lines: Vec<_> = fixture.lines().collect();
    let (session, trace) = session(&lines, SessionLimits::default());
    let mut events = Vec::new();
    let info = session
        .monitor(id(), 2, |metadata, frame| {
            assert_eq!(metadata.identity, DeviceIdentity::Provisioned(id()));
            events.push(frame.clone());
            Ok(())
        })
        .unwrap();
    assert_eq!(info.identity, DeviceIdentity::Provisioned(id()));
    assert_eq!(events.len(), 7);
    assert!(matches!(&events[0], ProtocolFrame::Device(actual) if actual == &info));
    assert!(matches!(&events[1], ProtocolFrame::Status(frame) if frame.token == "calibration_end"));
    assert!(matches!(&events[2], ProtocolFrame::Warning(frame) if frame.token == "ready_timeout"));
    assert!(matches!(&events[3], ProtocolFrame::Unknown(frame) if frame.prefix == "future"));
    assert!(matches!(&events[4], ProtocolFrame::Vector(frame)
        if frame.timestamp_ms == u32::MAX && frame.channels[0] == -1.0 && frame.channels[1] == 2.5));
    assert!(matches!(&events[6], ProtocolFrame::Vector(frame) if frame.timestamp_ms == 0));
    let trace = trace.borrow();
    assert_eq!(trace.writes, ["info\n"]);
    assert_eq!(trace.discards, 1);
    assert!(
        trace
            .timeouts
            .iter()
            .all(|timeout| *timeout <= Duration::from_secs(5))
    );
}

#[test]
fn mismatched_or_unprovisioned_identity_emits_nothing() {
    for info in [
        "device,2,0.1.0,srn-fedcba9876543210fedcba9876543210,7",
        "device,2,0.1.0,unprovisioned,7",
    ] {
        let (session, trace) = session(&[info, VECTOR], SessionLimits::default());
        assert!(matches!(
            session.monitor(id(), 1, |_, _| panic!("identity mismatch")),
            Err(MonitorError::IdentityMismatch { .. })
        ));
        assert_eq!(trace.borrow().reads, 1);
        assert_eq!(trace.borrow().writes, ["info\n"]);
    }
}

#[test]
fn incompatible_handshake_emits_nothing() {
    for info in [
        "device,3,0.1.0,srn-0123456789abcdef0123456789abcdef,7",
        "device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,6",
    ] {
        let (session, _) = session(&[info, VECTOR], SessionLimits::default());
        assert!(matches!(
            session.monitor(id(), 1, |_, _| panic!("incompatible")),
            Err(MonitorError::Session(SessionError::Compatibility(_)))
        ));
    }
}

#[test]
fn metadata_change_stops_before_another_sample() {
    for changed in [
        "device,2,0.1.0,srn-fedcba9876543210fedcba9876543210,7",
        "device,2,0.1.0,unprovisioned,7",
        "device,2,0.2.0,srn-0123456789abcdef0123456789abcdef,7",
    ] {
        let (session, _) = session(&[INFO, VECTOR, changed, VECTOR], SessionLimits::default());
        let mut samples = 0;
        assert!(matches!(
            session.monitor(id(), 2, |_, frame| {
                samples += usize::from(matches!(frame, ProtocolFrame::Vector(_)));
                Ok(())
            }),
            Err(MonitorError::MetadataChanged { .. })
        ));
        assert_eq!(samples, 1);
    }
}

#[test]
fn incompatible_repeated_device_stops() {
    let (session, _) = session(
        &[
            INFO,
            "device,3,0.1.0,srn-0123456789abcdef0123456789abcdef,7",
            VECTOR,
        ],
        SessionLimits::default(),
    );
    assert!(matches!(
        session.monitor(id(), 1, |_, _| Ok(())),
        Err(MonitorError::Session(SessionError::Compatibility(_)))
    ));
}

#[test]
fn malformed_frame_stops_without_fake_sample() {
    for malformed in [
        "vector,1,1,2,3",
        "vector,1,1,2,3,4,NaN,6,7",
        "vector,4294967296,1,2,3,4,5,6,7",
        "warn,",
        "device,2,0.1.0,bad-id,7",
    ] {
        let (session, _) = session(&[INFO, malformed, VECTOR], SessionLimits::default());
        let mut samples = 0;
        assert!(matches!(
            session.monitor(id(), 1, |_, frame| {
                samples += usize::from(matches!(frame, ProtocolFrame::Vector(_)));
                Ok(())
            }),
            Err(MonitorError::Session(SessionError::Protocol(_)))
        ));
        assert_eq!(samples, 0);
    }
}

#[test]
fn diagnostics_exhaust_line_budget_without_counting_as_samples() {
    let limits = SessionLimits {
        max_lines: 2,
        ..SessionLimits::default()
    };
    let (session, trace) = session(
        &[
            INFO,
            "warn,ready_timeout,3",
            "status,calibration_end",
            VECTOR,
        ],
        limits,
    );
    assert!(matches!(
        session.monitor(id(), 1, |_, _| Ok(())),
        Err(MonitorError::Session(SessionError::LineBudgetExceeded {
            max_lines: 2
        }))
    ));
    assert_eq!(trace.borrow().reads, 3);
}

#[test]
fn handshake_has_an_independent_line_budget() {
    let limits = SessionLimits {
        max_lines: 2,
        ..SessionLimits::default()
    };
    let (session, _) = session(&[VECTOR, "status,setup_end", INFO, VECTOR], limits);
    assert!(matches!(
        session.monitor(id(), 1, |_, _| panic!("no verified metadata")),
        Err(MonitorError::Session(SessionError::LineBudgetExceeded {
            max_lines: 2
        }))
    ));
}

#[test]
fn timeout_never_reuses_a_previous_sample() {
    let (session, _) = session(&[INFO, VECTOR], SessionLimits::default());
    let mut samples = 0;
    assert!(matches!(
        session.monitor(id(), 2, |_, frame| {
            samples += usize::from(matches!(frame, ProtocolFrame::Vector(_)));
            Ok(())
        }),
        Err(MonitorError::Session(SessionError::Transport(
            TransportError::Timeout
        )))
    ));
    assert_eq!(samples, 1);
}

#[test]
fn zero_deadline_fails_before_read() {
    let limits = SessionLimits {
        deadline: Duration::ZERO,
        ..SessionLimits::default()
    };
    let (session, trace) = session(&[INFO, VECTOR], limits);
    assert!(matches!(
        session.monitor(id(), 1, |_, _| panic!("deadline")),
        Err(MonitorError::Session(SessionError::DeadlineExceeded))
    ));
    assert_eq!(trace.borrow().reads, 0);
}

#[test]
fn capture_deadline_is_not_reset_by_frames() {
    let limits = SessionLimits {
        deadline: Duration::from_millis(100),
        ..SessionLimits::default()
    };
    let (session, trace) = session(&[INFO, "status,setup_end", VECTOR], limits);
    assert!(matches!(
        session.monitor(id(), 1, |_, frame| {
            if matches!(frame, ProtocolFrame::Status(_)) {
                std::thread::sleep(Duration::from_millis(150));
            }
            Ok(())
        }),
        Err(MonitorError::Session(SessionError::DeadlineExceeded))
    ));
    assert_eq!(trace.borrow().reads, 2);
}

#[test]
fn invalid_sample_limit_performs_no_io() {
    for count in [0, 65, usize::MAX] {
        let (session, trace) = session(&[INFO, VECTOR], SessionLimits::default());
        assert!(matches!(
            session.monitor(id(), count, |_, _| panic!("invalid limit")),
            Err(MonitorError::InvalidSampleCount { .. })
        ));
        let trace = trace.borrow();
        assert!(trace.writes.is_empty());
        assert_eq!(trace.discards, 0);
        assert_eq!(trace.reads, 0);
    }
}

#[test]
fn output_failure_stops_immediately() {
    let (session, trace) = session(&[INFO, VECTOR], SessionLimits::default());
    assert!(matches!(
        session.monitor(id(), 1, |_, _| {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "consumer closed"))
        }),
        Err(MonitorError::Output(_))
    ));
    assert_eq!(trace.borrow().reads, 1);
}

#[test]
fn requested_count_stops_without_reading_extra_input() {
    let (session, trace) = session(
        &[INFO, VECTOR, "malformed ignored after completion"],
        SessionLimits::default(),
    );
    session.monitor(id(), 1, |_, _| Ok(())).unwrap();
    assert_eq!(trace.borrow().reads, 2);
}
