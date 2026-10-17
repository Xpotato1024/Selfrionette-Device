//! One explicitly selected serial monitor. No calibration or firmware writes.
use crate::discovery::{PortKey, PortView};
use selfrionette_core::{DeviceInfo, ProtocolFrame, VectorFrame, parse_line};
use selfrionette_serial::legacy::{LegacyIo, LegacySerialTransport};
use selfrionette_serial::{PortSummary, available_port_summaries};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

const FRESH_FOR: Duration = Duration::from_millis(500);
const POLL_BUDGET: Duration = Duration::from_millis(40);
const MAX_LINES: usize = 128;
const MAX_DIAGNOSTICS: usize = 8;

pub trait Backend: Send {
    fn ports(&mut self) -> Result<Vec<PortSummary>, String>;
    fn open(&mut self, name: &str) -> Result<Box<dyn LegacyIo + Send>, String>;
}
pub struct SystemBackend;
impl Backend for SystemBackend {
    fn ports(&mut self) -> Result<Vec<PortSummary>, String> {
        available_port_summaries().map_err(|e| e.to_string())
    }
    fn open(&mut self, name: &str) -> Result<Box<dyn LegacyIo + Send>, String> {
        LegacySerialTransport::open(name, 115_200)
            .map(|p| Box::new(p) as Box<dyn LegacyIo + Send>)
            .map_err(|e| e.to_string())
    }
}
#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Snapshot {},
    Scan {},
    Connect {
        target: PortKey,
        #[serde(default)]
        query_info: bool,
        #[serde(default)]
        metadata_supported: bool,
    },
    Read {},
    Info {
        #[serde(default)]
        metadata_supported: bool,
    },
    Disconnect {},
    Pause {},
    Reconnect {
        confirmed: bool,
        #[serde(default)]
        metadata_supported: bool,
    },
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub expected: Option<u64>,
    pub command: Command,
}
#[derive(Debug, Serialize)]
pub struct Sample {
    pub timestamp_ms: u32,
    pub values: [f64; 7],
    pub received_age_ms: u64,
}
#[derive(Debug, Serialize)]
pub struct View {
    pub expected_id: Option<String>,
    pub candidate: Option<PortKey>,
    pub revision: u64,
    pub connection_id: u64,
    pub identity_state: &'static str,
    pub ports: Vec<PortView>,
    pub connected: bool,
    pub port: Option<String>,
    pub phase: &'static str,
    pub received_frames: u64,
    pub sample: Option<Sample>,
    pub reported_id: Option<String>,
    pub firmware: Option<String>,
    pub protocol: Option<u8>,
    pub info_pending: bool,
    pub diagnostics: Vec<String>,
    pub failure: Option<String>,
    pub hardware_verified: bool,
    pub control_ready: bool,
}
#[derive(Debug, Serialize)]
pub struct Reply {
    pub view: View,
    pub error: Option<String>,
}
pub struct Monitor {
    backend: Box<dyn Backend>,
    io: Option<Box<dyn LegacyIo + Send>>,
    ports: Vec<PortView>,
    port: Option<String>,
    revision: u64,
    connection_id: u64,
    info_attempted: bool,
    frame: Option<VectorFrame>,
    received_at: Option<Instant>,
    frames: u64,
    metadata: Option<DeviceInfo>,
    info_started: Option<Instant>,
    diagnostics: VecDeque<String>,
    failure: Option<String>,
    candidate: Option<PortKey>,
    known_id: Option<String>,
    checking_identity: bool,
    paused: bool,
}
impl Default for Monitor {
    fn default() -> Self {
        Self::new(Box::new(SystemBackend))
    }
}
impl Monitor {
    pub fn new(backend: Box<dyn Backend>) -> Self {
        Self {
            backend,
            io: None,
            ports: Vec::new(),
            port: None,
            revision: 0,
            connection_id: 0,
            info_attempted: false,
            frame: None,
            received_at: None,
            frames: 0,
            metadata: None,
            info_started: None,
            diagnostics: VecDeque::new(),
            failure: None,
            candidate: None,
            known_id: None,
            checking_identity: false,
            paused: false,
        }
    }
    fn view_at(&self, now: Instant) -> View {
        let sample = self.frame.as_ref().and_then(|f| {
            let age = now.saturating_duration_since(self.received_at?);
            (self.io.is_some() && !self.checking_identity && age <= FRESH_FOR).then_some(Sample {
                timestamp_ms: f.timestamp_ms,
                values: f.channels,
                received_age_ms: age.as_millis() as u64,
            })
        });
        let phase = if self.failure.is_some() {
            "error"
        } else if self.paused {
            "paused"
        } else if self.io.is_none() {
            "disconnected"
        } else if sample.is_some() {
            "receiving"
        } else if self.frames > 0 {
            "stale"
        } else {
            "waiting"
        };
        View {
            expected_id: self.known_id.clone(),
            candidate: self.candidate.clone(),
            revision: self.revision,
            connection_id: self.connection_id,
            identity_state: if let Some(metadata) = &self.metadata {
                if metadata.identity.to_string() == "unprovisioned" {
                    "unprovisioned"
                } else {
                    "identified"
                }
            } else if self.info_started.is_some() {
                "querying"
            } else if self.info_attempted {
                "no_response"
            } else {
                "unqueried"
            },
            ports: self.ports.clone(),
            connected: self.io.is_some(),
            port: self.port.clone(),
            phase,
            received_frames: self.frames,
            sample,
            reported_id: self.metadata.as_ref().map(|m| m.identity.to_string()),
            firmware: self.metadata.as_ref().map(|m| m.firmware_version.clone()),
            protocol: self.metadata.as_ref().map(|m| m.protocol_major),
            info_pending: self.info_started.is_some(),
            diagnostics: self.diagnostics.iter().cloned().collect(),
            failure: self.failure.clone(),
            hardware_verified: false,
            control_ready: false,
        }
    }
    pub fn execute(&mut self, request: Request) -> Reply {
        if matches!(request.command, Command::Snapshot {}) {
            return Reply {
                view: self.view_at(Instant::now()),
                error: None,
            };
        }
        if !matches!(request.command, Command::Scan {}) && request.expected != Some(self.revision) {
            return Reply {
                view: self.view_at(Instant::now()),
                error: Some(
                    "対象または接続状態が変わりました。現在の状態で操作し直してください。".into(),
                ),
            };
        }
        let Some(revision) = self.revision.checked_add(1) else {
            self.stop("接続の更新番号が上限に達しました。アプリを再起動してください。".into());
            return Reply {
                view: self.view_at(Instant::now()),
                error: self.failure.clone(),
            };
        };
        self.revision = revision;
        let error = self.apply(request.command).err();
        Reply {
            view: self.view_at(Instant::now()),
            error,
        }
    }
    fn scan(&mut self) -> Result<(), String> {
        let ports = self.backend.ports()?;
        if ports.len() > 128 {
            return Err("ポート数が上限128を超えています。".into());
        }
        self.ports = ports.into_iter().map(PortView::from).collect();
        Ok(())
    }
    pub fn connected(&self) -> bool {
        self.io.is_some()
    }
    fn clear_measurements(&mut self) {
        self.info_attempted = false;
        self.frame = None;
        self.received_at = None;
        self.metadata = None;
        self.info_started = None;
    }
    fn stop(&mut self, reason: String) {
        self.io = None;
        self.clear_measurements();
        self.failure = Some(reason);
    }
    fn note(&mut self, text: &str) {
        if self.diagnostics.len() == MAX_DIAGNOSTICS {
            self.diagnostics.pop_front();
        }
        self.diagnostics.push_back(text.chars().take(256).collect());
    }
    fn apply(&mut self, command: Command) -> Result<(), String> {
        match command {
            Command::Snapshot {} => Ok(()),
            Command::Scan {} => self.scan(),
            Command::Connect {
                target,
                query_info,
                metadata_supported,
            } => {
                if query_info && !metadata_supported {
                    return Err("情報照会を選択してください。通常の入力確認は受信専用です。".into());
                }
                let port = target.name.clone();
                if self.io.is_some() {
                    return Err("先に現在の接続を切断してください。".into());
                }
                self.clear_measurements();
                self.frames = 0;
                self.diagnostics.clear();
                self.failure = None;
                self.scan()?;
                crate::discovery::validate_candidate_topology(&self.ports, &target)?;
                let selected = self.ports.iter().find(|p| p.target == target);
                if selected.is_some_and(|p| p.category == "bootloader") {
                    return Err("書込み待機用ポートには入力監視として接続しません。".into());
                }
                if selected.is_none() {
                    return Err(
                        "選択時からポートの情報が変わったか、切断されました。再検索して選び直してください。".into(),
                    );
                }
                self.port = Some(port.clone());
                if self.candidate.as_ref() != Some(&target) {
                    self.known_id = None;
                }
                self.candidate = Some(target);
                self.paused = false;
                self.checking_identity = self.known_id.is_some();
                match self.backend.open(&port) {
                    Ok(io) => {
                        self.io = Some(io);
                        self.connection_id = self.revision;
                        if query_info || self.checking_identity {
                            self.apply(Command::Info {
                                metadata_supported: true,
                            })
                        } else {
                            Ok(())
                        }
                    }
                    Err(e) => {
                        let message = format!(
                            "接続できません。他のアプリによる占有や切断を確認してください。{e}"
                        );
                        self.stop(message.clone());
                        Err(message)
                    }
                }
            }
            Command::Reconnect {
                confirmed,
                metadata_supported,
            } => {
                let target = self.candidate.clone().ok_or("接続先を選択してください。")?;
                if self.known_id.is_none() && !confirmed {
                    return Err(
                        "個体IDを確認できません。同じ実物を再接続したことを確認してください。"
                            .into(),
                    );
                }
                self.io = None;
                self.apply(Command::Connect {
                    target,
                    query_info: false,
                    metadata_supported,
                })
            }
            Command::Pause {} | Command::Disconnect {} => {
                self.io = None;
                self.clear_measurements();
                self.paused = true;
                self.failure = None;
                self.frames = 0;
                Ok(())
            }
            Command::Info { metadata_supported } => {
                if !metadata_supported {
                    return Err(
                        "情報照会は明示操作でのみ行います。通常の入力確認は受信専用です。".into(),
                    );
                }
                if self.info_started.is_some() {
                    return Err("個体情報の応答を待っています。".into());
                }
                let io = self.io.as_mut().ok_or("先に対象を接続してください。")?;
                if let Err(e) = io.write("?2\n") {
                    self.stop(e.clone());
                    return Err(e);
                }
                self.info_attempted = true;
                self.info_started = Some(Instant::now());
                Ok(())
            }
            Command::Read {} => self.read(),
        }
    }
    fn read(&mut self) -> Result<(), String> {
        if self.io.is_none() {
            return Ok(());
        }
        let start = Instant::now();
        for _ in 0..MAX_LINES {
            let remaining = POLL_BUDGET.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                break;
            }
            let result = self
                .io
                .as_mut()
                .expect("connected")
                .read(remaining.min(Duration::from_millis(20)));
            match result {
                Ok(Some(line)) => {
                    if let Err(e) = self.accept(&line, Instant::now()) {
                        self.stop(e.clone());
                        return Err(e);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    let message = format!("受信が停止しました。接続を確認してください。{e}");
                    self.stop(message.clone());
                    return Err(message);
                }
            }
        }
        if self
            .info_started
            .is_some_and(|t| t.elapsed() > Duration::from_secs(2))
        {
            self.info_started = None;
            if self.checking_identity {
                self.stop("既知の個体IDを再確認できません。接続先を確認してください。".into());
                return Err(self.failure.clone().unwrap());
            }
            self.note("個体情報の応答なし。旧FirmwareまたはProtocol v2非対応の可能性があります。受信値から個体IDは推定しません。");
        }
        Ok(())
    }
    fn accept(&mut self, line: &str, now: Instant) -> Result<(), String> {
        if line.is_empty() {
            return Ok(());
        }
        if line.len() > selfrionette_serial::MAX_LINE_BYTES {
            return Err("受信行が長すぎます。".into());
        }
        match parse_line(line).map_err(|e| format!("受信形式が不正です。{e}"))? {
            ProtocolFrame::Vector(frame) => {
                if let Some(previous) = self.frame.as_ref() {
                    if frame.timestamp_ms < previous.timestamp_ms {
                        return Err("機器時刻が後退しました。再起動や時刻の周回を確認して接続し直してください。".into());
                    }
                    if frame.timestamp_ms == previous.timestamp_ms {
                        return Ok(());
                    }
                }
                self.frame = Some(frame);
                self.received_at = Some(now);
                self.frames = self.frames.saturating_add(1);
            }
            ProtocolFrame::Device(info) => {
                info.validate_compatibility().map_err(|e| e.to_string())?;
                let id = info.identity.to_string();
                if self.known_id.as_ref().is_some_and(|known| known != &id) {
                    return Err("同じ接続先で別の個体IDを検出しました。接続を停止しました。".into());
                }
                if id != "unprovisioned" {
                    self.known_id = Some(id);
                }
                self.checking_identity = false;
                if self.metadata.as_ref().is_some_and(|old| old != &info) {
                    return Err("接続中に個体情報が変わりました。接続し直してください。".into());
                }
                self.metadata = Some(info);
                self.info_started = None;
            }
            ProtocolFrame::Status(s) => {
                if self.frames > 0
                    && matches!(
                        s.token.as_str(),
                        "setup_start"
                            | "sensor_init_start"
                            | "calibration_start"
                            | "tare_command_received"
                    )
                {
                    return Err(
                        "機器の初期化・基準変更を検出しました。接続し直してください。".into(),
                    );
                }
                self.note(line);
            }
            ProtocolFrame::Warning(_) | ProtocolFrame::Unknown(_) => self.note(line),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[derive(Default)]
    struct Evidence {
        calibrations: usize,
        opens: Vec<String>,
        writes: Vec<String>,
        drops: usize,
        lines: VecDeque<Result<Option<String>, String>>,
        ports: Option<Vec<PortSummary>>,
    }
    struct FakeBackend(Arc<Mutex<Evidence>>);
    struct FakeIo(Arc<Mutex<Evidence>>);
    impl Backend for FakeBackend {
        fn ports(&mut self) -> Result<Vec<PortSummary>, String> {
            if let Some(ports) = self.0.lock().unwrap().ports.clone() {
                return Ok(ports);
            }
            Ok(vec![PortSummary {
                name: "COM_TEST".into(),
                kind: selfrionette_serial::PortKind::Usb,
                serial_number: Some("fixture-serial".into()),
                product: Some("fixture".into()),
                manufacturer: None,
                vid: Some(0),
                pid: Some(0),
            }])
        }
        fn open(&mut self, name: &str) -> Result<Box<dyn LegacyIo + Send>, String> {
            self.0.lock().unwrap().opens.push(name.into());
            Ok(Box::new(FakeIo(self.0.clone())))
        }
    }
    impl LegacyIo for FakeIo {
        fn read(&mut self, _: Duration) -> Result<Option<String>, String> {
            self.0.lock().unwrap().lines.pop_front().unwrap_or(Ok(None))
        }
        fn write(&mut self, text: &str) -> Result<(), String> {
            let mut evidence = self.0.lock().unwrap();
            evidence.calibrations += text.bytes().filter(|b| *b == b'i').count();
            evidence.writes.push(text.into());
            Ok(())
        }
    }
    impl Drop for FakeIo {
        fn drop(&mut self) {
            self.0.lock().unwrap().drops += 1;
        }
    }
    fn setup() -> (Monitor, Arc<Mutex<Evidence>>) {
        let e = Arc::new(Mutex::new(Evidence::default()));
        (Monitor::new(Box::new(FakeBackend(e.clone()))), e)
    }
    fn run(m: &mut Monitor, command: Command) -> Reply {
        m.execute(Request {
            expected: Some(m.revision),
            command,
        })
    }
    fn key(name: &str) -> PortKey {
        PortKey {
            name: name.into(),
            kind: "usb".into(),
            serial_number: Some("fixture-serial".into()),
            product: Some("fixture".into()),
            manufacturer: None,
            vid: Some(0),
            pid: Some(0),
        }
    }
    fn connect(m: &mut Monitor) {
        assert!(
            run(
                m,
                Command::Connect {
                    target: key("COM_TEST"),
                    query_info: false,
                    metadata_supported: true
                }
            )
            .error
            .is_none()
        );
    }
    fn line(e: &Arc<Mutex<Evidence>>, s: &str) {
        e.lock().unwrap().lines.push_back(Ok(Some(s.into())));
    }
    const VECTOR: &str = "vector,100,1,-2,3,4,5,6,7";
    const INFO: &str = "device,2,0.1.0,srn-11111111111111111111111111111111,7";
    #[test]
    fn original_legacy_monitor_and_reconnect_never_send_implicit_i_calibration() {
        let (mut m, e) = setup();
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target: key("COM_TEST"),
                    query_info: false,
                    metadata_supported: false
                }
            )
            .error
            .is_none()
        );
        line(&e, VECTOR);
        assert!(run(&mut m, Command::Read {}).view.sample.is_some());
        assert!(
            run(
                &mut m,
                Command::Info {
                    metadata_supported: false
                }
            )
            .error
            .is_some()
        );
        run(&mut m, Command::Pause {});
        assert!(
            run(
                &mut m,
                Command::Reconnect {
                    confirmed: true,
                    metadata_supported: false
                }
            )
            .error
            .is_none()
        );
        let e = e.lock().unwrap();
        assert!(e.writes.is_empty());
        assert_eq!(e.calibrations, 0);
    }
    #[test]
    fn pause_and_failed_read_reopen_the_retained_candidate_without_stale_samples() {
        let (mut m, e) = setup();
        connect(&mut m);
        line(&e, VECTOR);
        let first = run(&mut m, Command::Read {}).view;
        assert!(first.sample.is_some());
        let paused = run(&mut m, Command::Pause {}).view;
        assert!(!paused.connected);
        assert!(paused.candidate.is_some());
        assert!(paused.sample.is_none());
        assert_eq!(e.lock().unwrap().drops, 1);
        assert!(
            run(
                &mut m,
                Command::Reconnect {
                    confirmed: false,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        let reopened = run(
            &mut m,
            Command::Reconnect {
                confirmed: true,
                metadata_supported: true,
            },
        )
        .view;
        assert!(reopened.connected);
        assert_ne!(first.connection_id, reopened.connection_id);
        assert!(reopened.sample.is_none());
        e.lock().unwrap().lines.push_back(Err("removed".into()));
        assert!(!run(&mut m, Command::Read {}).view.connected);
        assert!(
            run(
                &mut m,
                Command::Reconnect {
                    confirmed: true,
                    metadata_supported: true
                }
            )
            .view
            .connected
        );
        assert_eq!(e.lock().unwrap().opens.len(), 3);
    }
    #[test]
    fn reconnect_waits_for_known_identity_and_rejects_same_com_swap() {
        let (mut m, e) = setup();
        connect(&mut m);
        line(&e, INFO);
        run(&mut m, Command::Read {});
        run(&mut m, Command::Pause {});
        run(
            &mut m,
            Command::Reconnect {
                confirmed: false,
                metadata_supported: true,
            },
        );
        line(&e, VECTOR);
        let pending = run(&mut m, Command::Read {});
        assert!(pending.view.sample.is_none());
        line(
            &e,
            &INFO.replace(
                "11111111111111111111111111111111",
                "22222222222222222222222222222222",
            ),
        );
        let swapped = run(&mut m, Command::Read {});
        assert!(swapped.error.is_some());
        assert!(!swapped.view.connected);
        assert!(swapped.view.sample.is_none());
    }
    #[test]
    fn reconnect_descriptor_change_is_rejected_before_open() {
        let (mut m, e) = setup();
        connect(&mut m);
        run(&mut m, Command::Pause {});
        let mut p = PortSummary {
            name: "COM_TEST".into(),
            kind: selfrionette_serial::PortKind::Usb,
            serial_number: Some("changed".into()),
            product: Some("fixture".into()),
            manufacturer: None,
            vid: Some(0),
            pid: Some(0),
        };
        e.lock().unwrap().ports = Some(vec![p.clone()]);
        assert!(
            run(
                &mut m,
                Command::Reconnect {
                    confirmed: true,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert_eq!(e.lock().unwrap().opens.len(), 1);
        p.serial_number = Some("fixture-serial".into());
        e.lock().unwrap().ports = Some(vec![p.clone(), p]);
        assert!(
            run(
                &mut m,
                Command::Reconnect {
                    confirmed: true,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert_eq!(e.lock().unwrap().opens.len(), 1);
    }
    #[test]
    fn enumeration_and_snapshot_never_open_or_write() {
        let (mut m, e) = setup();
        assert_eq!(run(&mut m, Command::Scan {}).view.ports.len(), 1);
        let v = run(&mut m, Command::Snapshot {}).view;
        assert!(!v.connected);
        assert!(v.sample.is_none());
        let e = e.lock().unwrap();
        assert!(e.opens.is_empty());
        assert!(e.writes.is_empty());
    }
    #[test]
    fn only_present_selected_port_can_be_opened() {
        let (mut m, e) = setup();
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target: key("COM_OTHER"),
                    query_info: false,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert!(e.lock().unwrap().opens.is_empty());
        connect(&mut m);
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target: key("COM_TEST"),
                    query_info: false,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert_eq!(e.lock().unwrap().opens.len(), 1);
    }
    #[test]
    fn legacy_receive_preserves_values_without_any_serial_command() {
        let (mut m, e) = setup();
        connect(&mut m);
        line(&e, VECTOR);
        let v = run(&mut m, Command::Read {}).view;
        assert_eq!(v.phase, "receiving");
        assert_eq!(v.sample.unwrap().values, [1., -2., 3., 4., 5., 6., 7.]);
        assert!(v.reported_id.is_none());
        assert!(!v.hardware_verified);
        assert!(!v.control_ready);
        assert!(e.lock().unwrap().writes.is_empty());
    }
    #[test]
    fn explicit_info_is_the_only_available_output_command() {
        let (mut m, e) = setup();
        connect(&mut m);
        assert!(
            run(
                &mut m,
                Command::Info {
                    metadata_supported: true
                }
            )
            .error
            .is_none()
        );
        assert!(
            run(
                &mut m,
                Command::Info {
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        line(&e, INFO);
        let v = run(&mut m, Command::Read {}).view;
        assert_eq!(
            v.reported_id.as_deref(),
            Some("srn-11111111111111111111111111111111")
        );
        assert!(!v.info_pending);
        assert_eq!(e.lock().unwrap().writes, ["?2\n"]);
    }
    #[test]
    fn stale_mutation_cannot_change_another_connection_state() {
        let (mut m, _) = setup();
        let old = m.revision;
        connect(&mut m);
        let r = m.execute(Request {
            expected: Some(old),
            command: Command::Disconnect {},
        });
        assert!(r.error.is_some());
        assert!(r.view.connected);
    }
    #[test]
    fn invalid_vectors_clear_values_and_release_the_port() {
        for invalid in [
            "vector,101,NaN,2,3,4,5,6,7",
            "vector,101,1,2",
            "vector,bad,1,2,3,4,5,6,7",
        ] {
            let (mut m, e) = setup();
            connect(&mut m);
            line(&e, VECTOR);
            run(&mut m, Command::Read {});
            line(&e, invalid);
            let r = run(&mut m, Command::Read {});
            assert!(r.error.is_some());
            assert!(!r.view.connected);
            assert!(r.view.sample.is_none());
            assert_eq!(e.lock().unwrap().drops, 1);
        }
    }
    #[test]
    fn io_failure_does_not_display_a_stale_value_as_a_new_sample() {
        let (mut m, e) = setup();
        connect(&mut m);
        line(&e, VECTOR);
        run(&mut m, Command::Read {});
        e.lock().unwrap().lines.push_back(Err("unplugged".into()));
        let r = run(&mut m, Command::Read {});
        assert!(r.error.is_some());
        assert!(r.view.sample.is_none());
        assert_eq!(e.lock().unwrap().drops, 1);
    }
    #[test]
    fn changing_identity_or_protocol_stops_the_stream() {
        for changed in [
            "device,2,0.1.0,srn-22222222222222222222222222222222,7",
            "device,3,0.2.0,srn-11111111111111111111111111111111,7",
        ] {
            let (mut m, e) = setup();
            connect(&mut m);
            line(&e, INFO);
            line(&e, VECTOR);
            run(&mut m, Command::Read {});
            line(&e, changed);
            let r = run(&mut m, Command::Read {});
            assert!(r.error.is_some());
            assert!(!r.view.connected);
            assert!(r.view.sample.is_none());
        }
    }
    #[test]
    fn backwards_time_and_reinitialization_require_explicit_reconnect() {
        for event in [
            "vector,1,1,2,3,4,5,6,7",
            "status,setup_start",
            "status,calibration_start",
        ] {
            let (mut m, e) = setup();
            connect(&mut m);
            line(&e, VECTOR);
            run(&mut m, Command::Read {});
            line(&e, event);
            assert!(run(&mut m, Command::Read {}).error.is_some());
            assert!(!m.view_at(Instant::now()).connected);
        }
    }
    #[test]
    fn repeated_timestamps_do_not_refresh_frozen_data() {
        let (mut m, _) = setup();
        connect(&mut m);
        let start = Instant::now();
        m.accept(VECTOR, start).unwrap();
        m.accept(VECTOR, start + Duration::from_millis(600))
            .unwrap();
        let v = m.view_at(start + Duration::from_millis(600));
        assert_eq!(v.phase, "stale");
        assert!(v.sample.is_none());
        assert_eq!(v.received_frames, 1);
    }
    #[test]
    fn diagnostics_and_per_poll_work_remain_bounded() {
        let (mut m, e) = setup();
        connect(&mut m);
        for _ in 0..200 {
            line(&e, "warn,ready_timeout,0");
        }
        let v = run(&mut m, Command::Read {}).view;
        assert_eq!(v.diagnostics.len(), 8);
        assert_eq!(e.lock().unwrap().lines.len(), 72);
        assert!(v.sample.is_none());
        assert!(!v.control_ready);
    }
    #[test]
    fn disconnect_and_drop_release_without_calibration_or_provisioning() {
        let (mut m, e) = setup();
        connect(&mut m);
        run(&mut m, Command::Disconnect {});
        assert_eq!(e.lock().unwrap().drops, 1);
        connect(&mut m);
        drop(m);
        let e = e.lock().unwrap();
        assert_eq!(e.drops, 2);
        assert!(e.writes.is_empty());
    }
    #[test]
    fn strict_request_shape_rejects_hardware_write_operations() {
        for action in ["tare", "provision", "flash", "reset"] {
            assert!(
                serde_json::from_str::<Request>(&format!(
                    r#"{{"expected":0,"command":{{"action":"{action}"}}}}"#
                ))
                .is_err()
            );
        }
    }
    #[test]
    fn descriptor_change_rejects_before_open_even_when_com_number_matches() {
        let (mut m, e) = setup();
        let mut target = key("COM_TEST");
        target.serial_number = Some("replaced".into());
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target,
                    query_info: false,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert!(e.lock().unwrap().opens.is_empty());
    }
    #[test]
    fn bootloader_port_is_not_opened_and_explicit_info_is_opt_in() {
        let (mut m, e) = setup();
        e.lock().unwrap().ports = Some(vec![PortSummary {
            name: "COM8".into(),
            kind: selfrionette_serial::PortKind::Usb,
            serial_number: None,
            product: None,
            manufacturer: None,
            vid: Some(0x1b4f),
            pid: Some(0x9205),
        }]);
        let target = run(&mut m, Command::Scan {}).view.ports[0].target.clone();
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target,
                    query_info: true,
                    metadata_supported: true
                }
            )
            .error
            .is_some()
        );
        assert!(e.lock().unwrap().opens.is_empty());
        e.lock().unwrap().ports = None;
        assert!(
            run(
                &mut m,
                Command::Connect {
                    target: key("COM_TEST"),
                    query_info: true,
                    metadata_supported: true
                }
            )
            .error
            .is_none()
        );
        assert_eq!(e.lock().unwrap().writes, vec!["?2\n"]);
        assert_eq!(m.view_at(Instant::now()).identity_state, "querying");
    }
    #[test]
    fn timeout_keeps_legacy_input_and_a_new_connection_gets_a_new_session() {
        let (mut m, e) = setup();
        connect(&mut m);
        let previous = m.connection_id;
        run(
            &mut m,
            Command::Info {
                metadata_supported: true,
            },
        );
        m.info_started = Some(Instant::now() - Duration::from_secs(3));
        line(&e, VECTOR);
        let v = run(&mut m, Command::Read {}).view;
        assert!(v.connected);
        assert!(v.sample.is_some());
        assert_eq!(v.identity_state, "no_response");
        run(&mut m, Command::Disconnect {});
        connect(&mut m);
        assert_ne!(m.connection_id, previous);
    }
}
