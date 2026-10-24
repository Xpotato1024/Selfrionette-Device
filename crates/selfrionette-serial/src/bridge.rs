//! One explicitly selected device, driven by versioned local commands.
//! Calibration and all device operations remain in LiveService.
use crate::{
    LineTransport,
    app::{
        ActiveProfile, CapturePhase, Command, LiveInfo, LiveService, LiveSnapshot, MethodDto,
        MonotonicClock, SampleEvent, StreamDiagnostics,
    },
};
use selfrionette_core::{DeviceId, setup::OperatorContext};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

pub const COMMAND_SCHEMA: &str = "selfrionette-bridge-command/v1";
pub const EVENT_SCHEMA: &str = "selfrionette-bridge-event/v1";
pub const HOST_CLOCK: &str = "bridge_process_elapsed_us";
pub const MAX_IPC_LINE_BYTES: usize = 64 * 1024;
pub const MAX_COMMAND_QUEUE: usize = 8;
pub const MAX_EVENT_QUEUE: usize = 32;
const MAX_INPUT_AGE_MS: u64 = 250;
pub type OpenTransport<'a> = dyn FnMut(&str) -> Result<Box<dyn LineTransport + Send>, String> + 'a;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BridgeConfig {
    pub port: String,
    pub expected_device_id: String,
    pub profile_path: PathBuf,
    pub operator: String,
    pub posture: String,
    pub grip: String,
    pub method: MethodDto,
}

#[derive(Debug, Deserialize)]
#[serde(from = "WireRequest")]
pub struct BridgeRequest {
    pub schema: String,
    pub request_id: u64,
    pub command: BridgeCommand,
}

// Keep each command envelope strict, including unit actions. A flattened tagged
// enum does not reliably reject unknown fields on every serde enum shape.
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum WireRequest {
    Configure {
        schema: String,
        request_id: u64,
        config: BridgeConfig,
    },
    Connect {
        schema: String,
        request_id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    Tare {
        schema: String,
        request_id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    Neutral {
        schema: String,
        request_id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    Apply {
        schema: String,
        request_id: u64,
        #[serde(default)]
        confirmed: bool,
    },
    Start {
        schema: String,
        request_id: u64,
    },
    Stop {
        schema: String,
        request_id: u64,
    },
    Disconnect {
        schema: String,
        request_id: u64,
    },
    Status {
        schema: String,
        request_id: u64,
    },
}
impl From<WireRequest> for BridgeRequest {
    fn from(request: WireRequest) -> Self {
        let (schema, request_id, command) = match request {
            WireRequest::Configure {
                schema,
                request_id,
                config,
            } => (schema, request_id, BridgeCommand::Configure { config }),
            WireRequest::Connect {
                schema,
                request_id,
                confirmed,
            } => (schema, request_id, BridgeCommand::Connect { confirmed }),
            WireRequest::Tare {
                schema,
                request_id,
                confirmed,
            } => (schema, request_id, BridgeCommand::Tare { confirmed }),
            WireRequest::Neutral {
                schema,
                request_id,
                confirmed,
            } => (schema, request_id, BridgeCommand::Neutral { confirmed }),
            WireRequest::Apply {
                schema,
                request_id,
                confirmed,
            } => (schema, request_id, BridgeCommand::Apply { confirmed }),
            WireRequest::Start { schema, request_id } => (schema, request_id, BridgeCommand::Start),
            WireRequest::Stop { schema, request_id } => (schema, request_id, BridgeCommand::Stop),
            WireRequest::Disconnect { schema, request_id } => {
                (schema, request_id, BridgeCommand::Disconnect)
            }
            WireRequest::Status { schema, request_id } => {
                (schema, request_id, BridgeCommand::Status)
            }
        };
        Self {
            schema,
            request_id,
            command,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum BridgeCommand {
    Configure {
        config: BridgeConfig,
    },
    Connect {
        #[serde(default)]
        confirmed: bool,
    },
    Tare {
        #[serde(default)]
        confirmed: bool,
    },
    Neutral {
        #[serde(default)]
        confirmed: bool,
    },
    Apply {
        #[serde(default)]
        confirmed: bool,
    },
    Start,
    Stop,
    Disconnect,
    Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgePhase {
    Unconfigured,
    Configured,
    Connected,
    AwaitingNeutral,
    CapturingNeutral,
    NeutralReady,
    Applied,
    Streaming,
    Stopped,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventHeader {
    pub schema: &'static str,
    pub bridge_revision: String,
    pub host_clock: &'static str,
    pub host_emitted_us: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BridgeSource {
    pub device_id: String,
    pub firmware_version: String,
    pub source_sha: String,
    pub protocol: u8,
}
impl From<&LiveInfo> for BridgeSource {
    fn from(info: &LiveInfo) -> Self {
        Self {
            device_id: info.device_id.clone(),
            firmware_version: info.firmware_version.clone(),
            source_sha: info.source_sha.clone(),
            protocol: 3,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct BridgeStatus {
    #[serde(flatten)]
    pub header: EventHeader,
    pub request_id: u64,
    pub phase: BridgePhase,
    pub ready: bool,
    pub session_neutral_valid: bool,
    pub source: Option<BridgeSource>,
    pub profile: Option<ActiveProfile>,
    pub operator: Option<OperatorContext>,
    pub diagnostics: Option<StreamDiagnostics>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BridgeSample {
    #[serde(flatten)]
    pub header: EventHeader,
    pub host_received_us: u64,
    pub sample_age_ms: u64,
    pub relative_backlog_ms: u64,
    pub source: BridgeSource,
    pub profile: ActiveProfile,
    pub operator: OperatorContext,
    pub domain: &'static str,
    pub session: u32,
    pub sequence: u32,
    pub device_ms: u32,
    pub settings_generation: u32,
    pub tare_epoch: u32,
    pub quality_flags: [u8; 7],
    pub normalized: [Option<f64>; 7],
    pub command: [Option<f64>; 7],
    pub span_exceeded: Option<[bool; 7]>,
    pub unsupported_direction: Option<[bool; 7]>,
    pub active: Option<[bool; 7]>,
    pub ready: bool,
    pub missing: bool,
    pub hardware_acceptance: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct BridgeFailure {
    #[serde(flatten)]
    pub header: EventHeader,
    pub request_id: u64,
    pub reason: String,
    pub ready: bool,
    pub fatal: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum BridgeEvent {
    Status(BridgeStatus),
    Sample(BridgeSample),
    Error(BridgeFailure),
}

impl BridgeEvent {
    /// Called immediately before writing, so the finite IPC queue contributes age.
    pub fn stamp_emitted(&mut self, emitted_us: u64) {
        match self {
            Self::Status(status) => status.header.host_emitted_us = emitted_us,
            Self::Error(error) => error.header.host_emitted_us = emitted_us,
            Self::Sample(sample) => {
                sample.header.host_emitted_us = emitted_us;
                sample.sample_age_ms = emitted_us
                    .saturating_sub(sample.host_received_us)
                    .div_ceil(1000)
                    .saturating_add(sample.relative_backlog_ms);
                if sample.sample_age_ms > MAX_INPUT_AGE_MS {
                    sample.ready = false;
                    sample.missing = true;
                    sample.normalized = [None; 7];
                    sample.command = [None; 7];
                    sample.span_exceeded = None;
                    sample.unsupported_direction = None;
                    sample.active = None;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;

pub fn valid_revision(revision: &str) -> bool {
    revision.len() == 40
        && revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && revision.bytes().any(|b| b != b'0')
}

fn bounded_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.trim() == value
        && value.len() <= 128
        && !value.chars().any(char::is_control)
}

impl BridgeConfig {
    fn validate(&self) -> Result<(), String> {
        if !bounded_text(&self.port)
            || !bounded_text(&self.operator)
            || !bounded_text(&self.posture)
            || !bounded_text(&self.grip)
        {
            return Err("port/operator/posture/grip must be explicit bounded text".into());
        }
        self.expected_device_id
            .parse::<DeviceId>()
            .map_err(|e| e.to_string())?;
        if !self.profile_path.is_absolute() || self.profile_path.as_os_str().len() > 4096 {
            return Err("profile_path must be an absolute bounded path".into());
        }
        if self
            .profile_path
            .extension()
            .and_then(|value| value.to_str())
            != Some("srncal")
        {
            return Err("profile_path must name a saved .srncal profile".into());
        }
        self.profile_name()?;
        let parent = self
            .profile_path
            .parent()
            .ok_or("profile directory missing")?;
        if parent.file_name().and_then(|v| v.to_str()) != Some(self.expected_device_id.as_str())
            || parent.parent().is_none()
        {
            return Err("profile must remain in the expected per-device store directory".into());
        }
        let directory = fs::symlink_metadata(parent)
            .map_err(|error| format!("profile directory is unavailable: {error}"))?;
        if !directory.is_dir() || directory.file_type().is_symlink() {
            return Err("profile directory must be a regular per-device store directory".into());
        }
        let metadata = fs::symlink_metadata(&self.profile_path)
            .map_err(|e| format!("profile is unavailable: {e}"))?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() > 8 * 1024 * 1024
        {
            return Err("profile must be a bounded regular file".into());
        }
        Ok(())
    }
    fn root(&self) -> &Path {
        self.profile_path.parent().unwrap().parent().unwrap()
    }
    fn profile_name(&self) -> Result<String, String> {
        self.profile_path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or_else(|| "profile basename is not UTF-8".into())
    }
}

pub struct BridgeSession {
    revision: String,
    clock: Arc<dyn MonotonicClock>,
    origin: Instant,
    config: Option<BridgeConfig>,
    service: Option<LiveService>,
    phase: BridgePhase,
    last_request_id: u64,
}

impl BridgeSession {
    pub fn new(revision: &str, clock: Arc<dyn MonotonicClock>, origin: Instant) -> Self {
        Self {
            revision: revision.into(),
            clock,
            origin,
            config: None,
            service: None,
            phase: BridgePhase::Unconfigured,
            last_request_id: 0,
        }
    }
    fn elapsed_us(&self) -> u64 {
        self.clock
            .now()
            .saturating_duration_since(self.origin)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64
    }
    fn header(&self) -> EventHeader {
        EventHeader {
            schema: EVENT_SCHEMA,
            bridge_revision: self.revision.clone(),
            host_clock: HOST_CLOCK,
            host_emitted_us: self.elapsed_us(),
        }
    }
    pub fn is_polling(&self) -> bool {
        matches!(
            self.phase,
            BridgePhase::CapturingNeutral
                | BridgePhase::NeutralReady
                | BridgePhase::Applied
                | BridgePhase::Streaming
        )
    }
    fn execute(&mut self, command: Command) -> Result<crate::app::Reply, String> {
        let root = self.config.as_ref().ok_or("bridge is unconfigured")?.root();
        self.service
            .as_mut()
            .ok_or("bridge is disconnected")?
            .execute(command, root)
            .map_err(|error| error.to_string())
    }
    pub fn status(&mut self) -> BridgeEvent {
        let view = if self.service.is_some() {
            self.execute(Command::Snapshot).ok().map(|reply| reply.view)
        } else {
            None
        };
        self.status_from_view(view.as_ref())
    }
    fn status_from_view(&self, view: Option<&LiveSnapshot>) -> BridgeEvent {
        BridgeEvent::Status(BridgeStatus {
            header: self.header(),
            request_id: self.last_request_id,
            phase: self.phase,
            ready: matches!(self.phase, BridgePhase::Applied | BridgePhase::Streaming)
                && view.is_some_and(|v| v.ready && v.output.is_some()),
            session_neutral_valid: view.is_some_and(|v| v.session_neutral_valid),
            source: view.and_then(|v| v.info.as_ref()).map(BridgeSource::from),
            profile: view.and_then(|v| v.profile.clone()),
            operator: view.and_then(|v| v.operator.clone()),
            diagnostics: view.map(|v| v.diagnostics.clone()),
        })
    }
    fn require_phase(&self, allowed: &[BridgePhase]) -> Result<(), String> {
        if allowed.contains(&self.phase) {
            Ok(())
        } else {
            Err(format!("command is invalid in phase {:?}", self.phase))
        }
    }
    fn confirmed(confirmed: bool, operation: &str) -> Result<(), String> {
        if confirmed {
            Ok(())
        } else {
            Err(format!(
                "{operation} requires a separate explicit operator-ready confirmation"
            ))
        }
    }
    pub fn handle(
        &mut self,
        request: BridgeRequest,
        open: &mut OpenTransport<'_>,
    ) -> Result<BridgeEvent, String> {
        if request.schema != COMMAND_SCHEMA
            || request.request_id == 0
            || request.request_id > 9_007_199_254_740_991
            || request.request_id <= self.last_request_id
        {
            return Err("unsupported command schema or non-increasing request_id".into());
        }
        self.last_request_id = request.request_id;
        match request.command {
            BridgeCommand::Configure { config } => {
                if self.service.is_some() {
                    return Err("disconnect before changing bridge configuration".into());
                }
                config.validate()?;
                self.config = Some(config);
                self.phase = BridgePhase::Configured;
            }
            BridgeCommand::Connect { confirmed } => {
                self.require_phase(&[BridgePhase::Configured, BridgePhase::Stopped])?;
                Self::confirmed(confirmed, "connect/port handoff")?;
                if !valid_revision(&self.revision) {
                    return Err("bridge build revision is unknown; rebuild with SELFRIONETTE_BRIDGE_REVISION before live access".into());
                }
                let config = self.config.as_ref().ok_or("configure first")?;
                config.validate()?;
                let transport = open(&config.port)?;
                self.service = Some(LiveService::with_safe_metadata_probe_and_clock(
                    transport,
                    self.clock.clone(),
                    self.origin,
                ));
                let reply = self.execute(Command::Connect)?;
                let info = reply
                    .view
                    .info
                    .as_ref()
                    .ok_or("compatible Protocol 3 information is required")?;
                if info.device_id != self.config.as_ref().unwrap().expected_device_id {
                    return Err(
                        "connected stable device identity does not match the selected profile"
                            .into(),
                    );
                }
                self.phase = BridgePhase::Connected;
            }
            BridgeCommand::Tare { confirmed } => {
                self.require_phase(&[BridgePhase::Connected, BridgePhase::AwaitingNeutral])?;
                Self::confirmed(confirmed, "unloaded device tare")?;
                // Establish the session before tare: a new stream challenge revokes tare.
                self.execute(Command::StreamStart)?;
                self.execute(Command::Tare { confirmed: true })?;
                self.execute(Command::StreamStop)?;
                let config = self.config.as_ref().unwrap().clone();
                self.execute(Command::SetOperator {
                    operator: config.operator,
                    posture: config.posture,
                    grip: config.grip,
                })?;
                self.execute(Command::LoadCalibration {
                    record_path: self.config.as_ref().unwrap().profile_name()?,
                })?;
                self.phase = BridgePhase::AwaitingNeutral;
            }
            BridgeCommand::Neutral { confirmed } => {
                self.require_phase(&[BridgePhase::AwaitingNeutral])?;
                Self::confirmed(confirmed, "natural-posture personal neutral")?;
                self.execute(Command::StreamStart)?;
                self.execute(Command::CaptureStart {
                    phase: CapturePhase::SessionNeutral,
                    axis: None,
                    direction: 1,
                    repeat: 0,
                    level: 0.0,
                    instruction: "Bridge explicit operator-ready session neutral".into(),
                    intent: None,
                    sample_count: Some(16),
                })?;
                self.phase = BridgePhase::CapturingNeutral;
            }
            BridgeCommand::Apply { confirmed } => {
                self.require_phase(&[BridgePhase::NeutralReady])?;
                Self::confirmed(confirmed, "profile application")?;
                self.execute(Command::ApplyCalibration {
                    confirmed: true,
                    method: self.config.as_ref().unwrap().method,
                })?;
                self.phase = BridgePhase::Applied;
            }
            BridgeCommand::Start => {
                self.require_phase(&[BridgePhase::Applied])?;
                let view = self.execute(Command::Snapshot)?.view;
                if !view.ready || view.output.is_none() {
                    return Err("fresh applied input is required before start".into());
                }
                self.phase = BridgePhase::Streaming;
            }
            BridgeCommand::Stop | BridgeCommand::Disconnect => {
                self.disconnect()?;
                self.phase = BridgePhase::Stopped;
            }
            BridgeCommand::Status => {}
        }
        Ok(self.status())
    }

    pub fn tick(&mut self) -> Result<Vec<BridgeEvent>, String> {
        if !self.is_polling() {
            return Ok(Vec::new());
        }
        let command = if self.phase == BridgePhase::CapturingNeutral {
            Command::CaptureTick
        } else {
            Command::StreamTick
        };
        let reply = self.execute(command)?;
        if self.phase == BridgePhase::CapturingNeutral {
            let capture = reply
                .view
                .capture
                .as_ref()
                .ok_or("neutral capture disappeared")?;
            if !capture.active {
                if !capture.completed || !reply.view.session_neutral_valid {
                    return Err(capture
                        .reason
                        .clone()
                        .unwrap_or_else(|| "neutral capture did not complete".into()));
                }
                self.phase = BridgePhase::NeutralReady;
                return Ok(vec![self.status_from_view(Some(&reply.view))]);
            }
            return Ok(Vec::new());
        }
        if matches!(self.phase, BridgePhase::NeutralReady | BridgePhase::Applied) {
            if !reply.view.session_neutral_valid {
                return Err("session neutral invalidated before input start".into());
            }
            return Ok(Vec::new());
        }
        let mut events = Vec::with_capacity(reply.sample_events.len());
        for event in reply.sample_events {
            events.push(self.sample_event(event, &reply.view)?);
        }
        if !reply.view.ready {
            return Err(
                "normalized input invalidated; stop and reconnect with fresh operator preparation"
                    .into(),
            );
        }
        Ok(events)
    }
    fn sample_event(&self, event: SampleEvent, view: &LiveSnapshot) -> Result<BridgeEvent, String> {
        let info = view
            .info
            .as_ref()
            .ok_or("sample device provenance is unavailable")?;
        let source = BridgeSource::from(info);
        let profile = view
            .profile
            .clone()
            .ok_or("sample profile provenance is unavailable")?;
        let operator = view
            .operator
            .clone()
            .ok_or("sample operator provenance is unavailable")?;
        let output = event.output.as_ref().filter(|_| event.ready);
        let mut sample = BridgeEvent::Sample(BridgeSample {
            header: self.header(),
            host_received_us: event.host_received_us,
            sample_age_ms: 0,
            relative_backlog_ms: event.relative_backlog_ms,
            source,
            profile,
            operator,
            domain: "normalized_input",
            session: event.sample.session,
            sequence: event.sample.sequence,
            device_ms: event.sample.timestamp_ms,
            settings_generation: event.sample.generation,
            tare_epoch: event.sample.tare_epoch,
            quality_flags: event.sample.flags,
            normalized: output.map(|o| o.normalized).unwrap_or([None; 7]),
            command: output.map(|o| o.command).unwrap_or([None; 7]),
            span_exceeded: output.map(|o| o.span_exceeded),
            unsupported_direction: output.map(|o| o.unsupported_direction),
            active: output.map(|o| o.active),
            ready: event.ready && output.is_some(),
            missing: output.is_none(),
            hardware_acceptance: false,
        });
        sample.stamp_emitted(self.elapsed_us());
        Ok(sample)
    }
    fn disconnect(&mut self) -> Result<(), String> {
        let result = if self.service.is_some() {
            self.execute(Command::Disconnect).map(|_| ())
        } else {
            Ok(())
        };
        // Release the exclusively owned port even when acknowledgement fails.
        self.service = None;
        result
    }
    pub fn shutdown(&mut self) -> Result<(), String> {
        let result = self.disconnect();
        self.phase = BridgePhase::Stopped;
        result
    }
    pub fn fail(&mut self, reason: impl Into<String>) -> BridgeEvent {
        let reason = reason.into();
        let cleanup = self.disconnect().err();
        self.phase = BridgePhase::Failed;
        let mut detail = reason;
        if let Some(cleanup) = cleanup {
            detail.push_str(&format!("; disconnect: {cleanup}"));
        }
        if detail.len() > 2048 {
            let mut end = 2048;
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
        }
        BridgeEvent::Error(BridgeFailure {
            header: self.header(),
            request_id: self.last_request_id,
            reason: detail,
            ready: false,
            fatal: true,
        })
    }
}
