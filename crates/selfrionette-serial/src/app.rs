//! Typed, bounded live management and personal calibration service for v3 devices.
//!
//! The owner of the serial handle and application storage root stays in the native
//! application. This module owns protocol matching, session invalidation, measured
//! capture, fitting, held-out replay, and immutable profile persistence.

use crate::{LineTransport, TransportError};
pub mod observability;
use observability::StreamMonitor;
pub use observability::{MonotonicClock, StreamDiagnostics, SystemClock};
use selfrionette_core::{
    DeviceId, DeviceIdentity,
    calibration::Context,
    input_calibration::{
        self, AxisDefinition, CalibrationRecording, FirmwareVectorSample, FitLimits, FittedInput,
        GuidedTrial, InputMethod, InputProcessor, InputQuality, MAX_CALIBRATION_SAMPLES,
        SignalSettings,
    },
    protocol_v3::{self, DeviceInfo, FLAG_FRESH, Frame, Sample},
    setup::{
        ChannelOrientation, CurrentContext, DeviceSetup, DomainSample, Evidence, FirmwareBinding,
        OperatorContext, Origin, ProfileBinding, SampleDomain, SetupError, project_device_oriented,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fmt,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const INFO_TIMEOUT: Duration = Duration::from_secs(3);
const READ_BUDGET: usize = 64;
const SAMPLE_TIMEOUT_TICKS: u8 = 20;
const MAX_CAPTURE_SAMPLES: usize = 64;
const DEFAULT_CAPTURE_SAMPLES: usize = 16;
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_CAPTURE_ARCHIVE_COUNT: usize = 256;
const MAX_CAPTURE_ARCHIVE_BYTES: u64 =
    (MAX_CAPTURE_ARCHIVE_COUNT as u64) * (MAX_CAPTURE_ARCHIVE_RECORD_BYTES as u64);
const MAX_CAPTURE_ARCHIVE_RECORD_BYTES: usize = 1024 * 1024;
const STREAM_DRAIN_BUDGET: usize = 16;
const STREAM_DRAIN_TIME: Duration = Duration::from_millis(20);
const STREAM_READ_SLICE: Duration = Duration::from_millis(10);
const MAX_SAMPLE_AGE: Duration = Duration::from_millis(250);
const MAX_PROFILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROFILE_COUNT: usize = 128;
const MAX_DEVICE_STORE_ENTRIES: usize = 512;
const PROFILE_SCHEMA: u16 = 1;
const SETUP_SCHEMA: u16 = 1;
const VERIFICATION_POLICY_VERSION: &str = "captured-intent-direction-tolerance-coverage-v2";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn random_session_id() -> Result<u32, ServiceError> {
    for _ in 0..4 {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|error| {
            ServiceError::Fatal(format!(
                "OS random source unavailable for stream session: {error}"
            ))
        })?;
        let session_id = u32::from_le_bytes(bytes);
        if session_id != 0 {
            return Ok(session_id);
        }
    }
    Err(ServiceError::Fatal(
        "OS random source returned reserved stream session zero repeatedly".into(),
    ))
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapturePhase {
    Neutral,
    TrialNeutral,
    Ramp,
    Hold,
    Release,
    HeldOutNeutral,
    HeldOutRamp,
    HeldOutHold,
    HeldOutRelease,
    SessionNeutral,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MethodDto {
    Diagonal,
    Coupled,
}
impl From<MethodDto> for InputMethod {
    fn from(value: MethodDto) -> Self {
        match value {
            MethodDto::Diagonal => Self::Diagonal,
            MethodDto::Coupled => Self::Coupled,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupChannelDto {
    pub wire: usize,
    pub logical_axis: Option<usize>,
    pub location: Option<String>,
    pub direction: Option<String>,
    pub polarity: Option<i8>,
    pub confirmed: bool,
    #[serde(default)]
    pub evidence: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SetupDto {
    pub assembly_revision: u32,
    pub config_revision: u32,
    pub channels: [SetupChannelDto; 7],
}

/// Explicit per-session thresholds; there is intentionally no Rust-side default.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FitLimitsDto {
    pub noise_floor_counts: [f64; 7],
    pub max_rest_range_counts: [f64; 7],
    pub max_drift_counts: [f64; 7],
    pub min_signal_to_noise: f64,
    pub repeat_tolerance: f64,
    pub direction_tolerance: f64,
    pub max_condition_number: f64,
    pub max_noise_gain: f64,
    pub max_sample_gap_ms: u32,
}
impl From<FitLimitsDto> for FitLimits {
    fn from(value: FitLimitsDto) -> Self {
        Self {
            noise_floor_counts: value.noise_floor_counts,
            max_rest_range_counts: value.max_rest_range_counts,
            max_drift_counts: value.max_drift_counts,
            min_signal_to_noise: value.min_signal_to_noise,
            repeat_tolerance: value.repeat_tolerance,
            direction_tolerance: value.direction_tolerance,
            max_condition_number: value.max_condition_number,
            max_noise_gain: value.max_noise_gain,
            max_sample_gap_ms: value.max_sample_gap_ms,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignalSettingsDto {
    pub time_constant_ms: f64,
    pub noise_multiplier: f64,
    pub release_ratio: f64,
    pub max_deadzone_fraction: f64,
}
impl From<SignalSettingsDto> for SignalSettings {
    fn from(value: SignalSettingsDto) -> Self {
        Self {
            time_constant_ms: value.time_constant_ms,
            noise_multiplier: value.noise_multiplier,
            release_ratio: value.release_ratio,
            max_deadzone_fraction: value.max_deadzone_fraction,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Snapshot,
    Connect,
    Disconnect,
    Info,
    StreamStart,
    StreamTick,
    StreamStop,
    Tare {
        confirmed: bool,
    },
    Provision {
        confirmed: bool,
        device_id: String,
    },
    Setup {
        setup: Box<SetupDto>,
    },
    SaveDeviceSettings {
        expected_generation: u32,
        known_mask: u8,
        negative_mask: u8,
    },
    SetOperator {
        operator: String,
        posture: String,
        grip: String,
    },
    SkipDirection {
        axis: usize,
        direction: i8,
    },
    CaptureStart {
        phase: CapturePhase,
        axis: Option<usize>,
        direction: i8,
        repeat: u8,
        level: f64,
        instruction: String,
        #[serde(default)]
        intent: Option<[f64; 7]>,
        #[serde(default)]
        sample_count: Option<usize>,
    },
    CaptureTick,
    CaptureAbort,
    Fit {
        limits: FitLimitsDto,
        signal_settings: SignalSettingsDto,
        #[serde(default)]
        method: Option<MethodDto>,
    },
    VerifyHeldOut {
        method: MethodDto,
    },
    SaveCalibration {
        profile_revision: u32,
        method: MethodDto,
        confirmed: bool,
    },
    ListCalibrations,
    LoadCalibration {
        record_path: String,
    },
    ApplyCalibration {
        confirmed: bool,
        method: MethodDto,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Reply {
    pub view: LiveSnapshot,
    /// Each accepted frame from this operation, in source order (at most 16).
    /// An empty vector never republishes the previous sample.
    pub sample_events: Vec<SampleEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveSnapshot {
    pub connected: bool,
    pub ready: bool,
    pub session_neutral_valid: bool,
    pub sample_fresh: bool,
    pub sample_age_ms: Option<u64>,
    pub diagnostics: StreamDiagnostics,
    pub profile: Option<ActiveProfile>,
    pub message: String,
    pub phase: String,
    pub info: Option<LiveInfo>,
    pub sample: Option<LiveSample>,
    pub output: Option<NormalizedOutput>,
    pub setup: Option<SetupDto>,
    pub operator: Option<OperatorContext>,
    pub capture: Option<CaptureView>,
    pub fit: Option<FitView>,
    pub heldout: HeldOutView,
    pub profiles: Vec<ProfileView>,
    pub profile_message: Option<String>,
    pub saved_path: Option<String>,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveInfo {
    pub firmware_version: String,
    pub source_sha: String,
    pub board: String,
    pub mcu: String,
    pub clock_hz: u32,
    pub device_id: String,
    pub generation: u32,
    pub assembly_revision: u32,
    pub known_mask: u8,
    pub negative_mask: u8,
    pub tare_epoch: u32,
    pub tare_valid: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveSample {
    pub session: u32,
    pub sequence: u32,
    pub timestamp_ms: u32,
    pub tare_epoch: u32,
    pub generation: u32,
    pub raw: [Option<i32>; 7],
    pub oriented: [Option<i32>; 7],
    pub flags: [u8; 7],
}

#[derive(Clone, Debug, Serialize)]
pub struct NormalizedOutput {
    pub normalized: [Option<f64>; 7],
    pub command: [Option<f64>; 7],
    pub span_exceeded: [bool; 7],
    pub unsupported_direction: [bool; 7],
    pub active: [bool; 7],
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveProfile {
    pub record: String,
    pub revision: u32,
    pub assembly_revision: u32,
    pub config_revision: u32,
    pub method: MethodDto,
}

#[derive(Clone, Debug, Serialize)]
pub struct SampleEvent {
    pub sample: LiveSample,
    /// Microseconds from this service's host monotonic origin.
    pub host_received_us: u64,
    pub relative_backlog_ms: u64,
    pub ready: bool,
    pub output: Option<NormalizedOutput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct CaptureView {
    pub phase: String,
    pub active: bool,
    pub samples: usize,
    pub count: usize,
    pub done: usize,
    pub required: usize,
    pub complete: bool,
    pub completed: bool,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FitView {
    pub diagonal_available: bool,
    pub diagonal_reason: Option<String>,
    pub coupled_available: bool,
    pub coupled_rejection: Option<String>,
    pub positive_span_counts: [f64; 7],
    pub negative_span_counts: [Option<f64>; 7],
    pub noise_envelope_counts: [f64; 7],
    pub cross_axis_ratio: [f64; 7],
    pub condition_number: Option<f64>,
    pub inverse_noise_gain: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct HeldOutView {
    pub passed: bool,
    pub reason: Option<String>,
    pub trials: usize,
    pub method: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProfileView {
    pub path: String,
    pub label: String,
    pub compatible: bool,
    pub reason: Option<String>,
}

#[derive(Debug)]
pub enum ServiceError {
    Transport(TransportError),
    Protocol(protocol_v3::ParseError),
    Input(input_calibration::InputError),
    Setup(SetupError),
    Invalid(String),
    Persistence(String),
    Fatal(String),
}
impl ServiceError {
    fn fatal(&self) -> bool {
        matches!(
            self,
            Self::Transport(_) | Self::Protocol(_) | Self::Fatal(_)
        )
    }
}
impl fmt::Display for ServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(e) => e.fmt(f),
            Self::Protocol(e) => write!(f, "v3 protocol error: {e}"),
            Self::Input(e) => write!(f, "calibration error: {e}"),
            Self::Setup(e) => write!(f, "setup error: {e}"),
            Self::Invalid(s) => f.write_str(s),
            Self::Persistence(s) => write!(f, "profile storage error: {s}"),
            Self::Fatal(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for ServiceError {}
impl From<TransportError> for ServiceError {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}
impl From<protocol_v3::ParseError> for ServiceError {
    fn from(value: protocol_v3::ParseError) -> Self {
        Self::Protocol(value)
    }
}
impl From<input_calibration::InputError> for ServiceError {
    fn from(value: input_calibration::InputError) -> Self {
        Self::Input(value)
    }
}
impl From<SetupError> for ServiceError {
    fn from(value: SetupError) -> Self {
        Self::Setup(value)
    }
}

#[derive(Clone, Debug)]
struct CapturedWindow {
    phase: CapturePhase,
    axis: Option<usize>,
    direction: i8,
    repeat: u8,
    level: f64,
    instruction: String,
    intent: Option<[f64; 7]>,
    samples: Vec<FirmwareVectorSample>,
    wire_samples: Vec<Sample>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct TrialKey {
    axis: usize,
    direction: i8,
    repeat: u8,
}

#[derive(Clone, Debug, Default)]
struct TrialParts {
    level: Option<f64>,
    neutral: Option<Vec<FirmwareVectorSample>>,
    ramp: Option<Vec<FirmwareVectorSample>>,
    hold: Option<Vec<FirmwareVectorSample>>,
    release: Option<Vec<FirmwareVectorSample>>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct HeldOutKey {
    axis: usize,
    instruction: String,
}

#[derive(Clone, Debug, Default)]
struct HeldOutParts {
    neutral: Option<Vec<FirmwareVectorSample>>,
    ramp: Option<Vec<FirmwareVectorSample>>,
    hold: Option<Vec<FirmwareVectorSample>>,
    release: Option<Vec<FirmwareVectorSample>>,
    intent: Option<[f64; 7]>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StoredSetup {
    schema: u16,
    setup: DeviceSetup,
    before_settings: DeviceSettingsSnapshot,
    after_settings: DeviceSettingsSnapshot,
    known_mask: u8,
    negative_mask: u8,
    channel_evidence: [String; 7],
    observation: RawSetupObservation,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DeviceSettingsSnapshot {
    device_id: DeviceId,
    generation: u32,
    assembly_revision: u32,
    known_mask: u8,
    negative_mask: u8,
}

fn validate_settings_transition(
    before: &DeviceSettingsSnapshot,
    after: &DeviceSettingsSnapshot,
) -> Result<(), ServiceError> {
    let valid = before.device_id == after.device_id
        && ((before.generation == after.generation && before.same_payload(after))
            || (before.generation.checked_add(1) == Some(after.generation)
                && !before.same_payload(after)));
    if valid {
        Ok(())
    } else {
        Err(ServiceError::Fatal(
            "settings readback is not a same-payload no-op or one-generation verified update"
                .into(),
        ))
    }
}

fn valid_setup_proof(record: &StoredSetup) -> bool {
    let expected_negative =
        record
            .setup
            .channels
            .iter()
            .enumerate()
            .fold(0u8, |mask, (wire, channel)| {
                if channel.polarity == Some(-1) {
                    mask | (1 << wire)
                } else {
                    mask
                }
            });
    let observation = &record.observation;
    let before = &record.before_settings;
    let after = &record.after_settings;
    let valid_transition = before.device_id == after.device_id
        && ((before.generation == after.generation && before.same_payload(after))
            || (before.generation.checked_add(1) == Some(after.generation)
                && !before.same_payload(after)));
    record.schema == SETUP_SCHEMA
        && record.setup.device_id == before.device_id
        && record.setup.device_id == after.device_id
        && record.setup.settings_generation == after.generation
        && record.setup.assembly_revision == after.assembly_revision
        && record.known_mask == 0x7f
        && record.negative_mask == expected_negative
        && record.known_mask == after.known_mask
        && record.negative_mask == after.negative_mask
        && before.known_mask & !0x7f == 0
        && before.negative_mask & !before.known_mask == 0
        && after.known_mask & !0x7f == 0
        && after.negative_mask & !after.known_mask == 0
        && observation.session != 0
        && observation.sequence != 0
        && observation.generation == before.generation
        && observation.flags.iter().all(|flag| *flag == FLAG_FRESH)
        && observation.raw.iter().all(Option::is_some)
        && record.channel_evidence.iter().all(|value| {
            !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
        })
        && valid_transition
        && record.setup.require_validated_mapping().is_ok()
}

fn validate_setup_revision(
    previous_records: &[StoredSetup],
    proposed: &DeviceSetup,
) -> Result<(), ServiceError> {
    for previous in previous_records.iter().filter(|record| {
        record.setup.device_id == proposed.device_id
            && record.setup.assembly_revision == proposed.assembly_revision
    }) {
        if proposed.config_revision < previous.setup.config_revision {
            return Err(ServiceError::Persistence(
                "host config_revision cannot move backwards within an assembly revision".into(),
            ));
        }
        if proposed.config_revision == previous.setup.config_revision
            && proposed.channels != previous.setup.channels
        {
            return Err(ServiceError::Persistence(
                "an existing config_revision cannot be rebound to a different host mapping".into(),
            ));
        }
    }
    Ok(())
}

fn read_valid_setup_records(directory: &Path) -> Result<Vec<StoredSetup>, ServiceError> {
    match fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ServiceError::Persistence(error.to_string())),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(ServiceError::Persistence(
                "device store path is not a regular directory".into(),
            ));
        }
        Ok(_) => {}
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| ServiceError::Persistence(error.to_string()))?
        .take(MAX_DEVICE_STORE_ENTRIES)
    {
        let entry = entry.map_err(|error| ServiceError::Persistence(error.to_string()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("setup-") || !name.ends_with(".srnsetup") {
            continue;
        }
        let path = entry.path();
        reject_symlink(&path)?;
        let metadata =
            fs::metadata(&path).map_err(|error| ServiceError::Persistence(error.to_string()))?;
        if !metadata.is_file() || metadata.len() > 64 * 1024 {
            continue;
        }
        let bytes =
            fs::read(&path).map_err(|error| ServiceError::Persistence(error.to_string()))?;
        let Ok(record) = serde_json::from_slice::<StoredSetup>(&bytes) else {
            continue;
        };
        if valid_setup_proof(&record) && record.setup.require_validated_mapping().is_ok() {
            records.push(record);
        }
    }
    Ok(records)
}

fn setup_matches_info(record: &StoredSetup, id: DeviceId, info: &DeviceInfo) -> bool {
    record.setup.device_id == id
        && record.setup.assembly_revision == info.assembly_revision
        && record.setup.settings_generation == info.generation
        && record.known_mask == info.known_mask
        && record.negative_mask == info.negative_mask
}

impl DeviceSettingsSnapshot {
    fn from_info(info: &DeviceInfo) -> Result<Self, ServiceError> {
        Ok(Self {
            device_id: provisioned_id(info)?,
            generation: info.generation,
            assembly_revision: info.assembly_revision,
            known_mask: info.known_mask,
            negative_mask: info.negative_mask,
        })
    }

    fn same_payload(&self, other: &Self) -> bool {
        self.device_id == other.device_id
            && self.assembly_revision == other.assembly_revision
            && self.known_mask == other.known_mask
            && self.negative_mask == other.negative_mask
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RawSetupObservation {
    session: u32,
    sequence: u32,
    generation: u32,
    tare_epoch: u32,
    flags: [u8; 7],
    raw: [Option<i32>; 7],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredHeldOut {
    key: String,
    axis: usize,
    instruction: String,
    intent: [f64; 7],
    neutral: Vec<FirmwareVectorSample>,
    ramp: Vec<FirmwareVectorSample>,
    hold: Vec<FirmwareVectorSample>,
    release: Vec<FirmwareVectorSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredWireSample {
    session: u32,
    sequence: u32,
    device_ms: u32,
    tare_epoch: u32,
    generation: u32,
    tare_valid: bool,
    raw: [Option<i32>; 7],
    oriented: [Option<i32>; 7],
    flags: [u8; 7],
}
impl From<&Sample> for StoredWireSample {
    fn from(sample: &Sample) -> Self {
        Self {
            session: sample.session,
            sequence: sample.sequence,
            device_ms: sample.device_ms,
            tare_epoch: sample.tare_epoch,
            generation: sample.generation,
            tare_valid: sample.tare_valid,
            raw: sample.raw,
            oriented: sample.oriented,
            flags: sample.flags,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredCapture {
    schema: u16,
    status: String,
    committed_for_current_session: bool,
    archive_is_current_runtime_evidence: bool,
    reason: Option<String>,
    phase: CapturePhase,
    axis: Option<usize>,
    direction: i8,
    repeat: u8,
    level: f64,
    instruction: String,
    intent: Option<[f64; 7]>,
    requested_samples: usize,
    first_sequence: Option<u32>,
    samples: Vec<FirmwareVectorSample>,
    wire_samples: Vec<StoredWireSample>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct FitSummaryRecord {
    baseline_counts: [f64; 7],
    noise_envelope_counts: [f64; 7],
    polarity: [i8; 7],
    positive_span_counts: [f64; 7],
    negative_span_counts: [Option<f64>; 7],
    positive_repeats: [usize; 7],
    negative_repeats: [usize; 7],
    response_matrix: [[f64; 7]; 7],
    cross_axis_ratio: [f64; 7],
    condition_number: Option<f64>,
    inverse_noise_gain: Option<f64>,
    coupled_rejection: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRecord {
    schema: u16,
    binding: ProfileBinding,
    recording: CalibrationRecording,
    limits: FitLimits,
    signal_settings: SignalSettings,
    fit_summary: FitSummaryRecord,
    setup_proof: StoredSetup,
    heldout: Vec<StoredHeldOut>,
    heldout_passed: bool,
    heldout_result: String,
    heldout_method: MethodDto,
    verification_policy_version: String,
}

#[derive(Clone)]
struct ActiveCapture {
    window: CapturedWindow,
    requested: usize,
    first_sequence: Option<u32>,
    started: Instant,
    pending_archive: Option<PendingArchive>,
}

#[derive(Clone)]
struct PendingArchive {
    committed: bool,
    status: String,
    reason: Option<String>,
}

pub struct LiveService {
    transport: Box<dyn LineTransport + Send>,
    clock: Arc<dyn MonotonicClock>,
    stream_monitor: StreamMonitor,
    sample_events: Vec<SampleEvent>,
    safe_metadata_probe: bool,
    info: Option<DeviceInfo>,
    setup: Option<DeviceSetup>,
    pending_setup: Option<SetupDto>,
    operator: Option<OperatorContext>,
    connection_epoch: u64,
    request_id: u32,
    session_id: u32,
    streaming: bool,
    resume_after_tare: bool,
    last_sequence: Option<u32>,
    timeouts: u8,
    latest_sample: Option<Sample>,
    latest_received: Option<Instant>,
    latest_sample_usable: bool,
    setup_evidence: Option<[String; 7]>,
    setup_observation: Option<RawSetupObservation>,
    setup_before_settings: Option<DeviceSettingsSnapshot>,
    setup_after_settings: Option<DeviceSettingsSnapshot>,
    latest_output: Option<NormalizedOutput>,
    active_capture: Option<ActiveCapture>,
    global_neutral: Option<Vec<FirmwareVectorSample>>,
    trials: BTreeMap<TrialKey, TrialParts>,
    heldout: BTreeMap<HeldOutKey, HeldOutParts>,
    session_neutral: Option<[f64; 7]>,
    fit: Option<FittedInput>,
    fit_limits: Option<FitLimits>,
    signal_settings: Option<SignalSettings>,
    fit_view: Option<FitView>,
    fit_summary: Option<FitSummaryRecord>,
    fit_method: InputMethod,
    heldout_view: HeldOutView,
    heldout_method: Option<InputMethod>,
    loaded_binding: Option<ProfileBinding>,
    applied_context: Option<Context>,
    processor: Option<InputProcessor>,
    profiles: Vec<ProfileView>,
    profile_message: Option<String>,
    saved_path: Option<String>,
    messages: Vec<String>,
    fatal: bool,
    last_error: Option<String>,
    last_capture: Option<CaptureView>,
}

impl LiveService {
    /// GUI probing uses aliases ignored by both archived firmware sources.
    /// The explicit CLI constructor retains its existing wire contract.
    pub fn with_safe_metadata_probe(transport: Box<dyn LineTransport + Send>) -> Self {
        let mut service = Self::new(transport);
        service.safe_metadata_probe = true;
        service
    }
    pub fn with_safe_metadata_probe_and_clock(
        transport: Box<dyn LineTransport + Send>,
        clock: Arc<dyn MonotonicClock>,
        origin: Instant,
    ) -> Self {
        let mut service = Self::with_clock(transport, clock, origin);
        service.safe_metadata_probe = true;
        service
    }
    pub fn new(transport: Box<dyn LineTransport + Send>) -> Self {
        Self::with_clock(transport, observability::system_clock(), Instant::now())
    }

    /// The caller supplies one monotonic origin for service and IPC timestamps.
    /// Production callers use SystemClock; tests can advance a shared fake clock.
    pub fn with_clock(
        transport: Box<dyn LineTransport + Send>,
        clock: Arc<dyn MonotonicClock>,
        origin: Instant,
    ) -> Self {
        Self {
            transport,
            clock,
            stream_monitor: StreamMonitor::new(origin),
            sample_events: Vec::with_capacity(STREAM_DRAIN_BUDGET),
            safe_metadata_probe: false,
            info: None,
            setup: None,
            pending_setup: None,
            operator: None,
            connection_epoch: 0,
            request_id: 0,
            session_id: 0,
            streaming: false,
            resume_after_tare: false,
            last_sequence: None,
            timeouts: 0,
            latest_sample: None,
            latest_received: None,
            latest_sample_usable: false,
            setup_evidence: None,
            setup_observation: None,
            setup_before_settings: None,
            setup_after_settings: None,
            latest_output: None,
            active_capture: None,
            global_neutral: None,
            trials: BTreeMap::new(),
            heldout: BTreeMap::new(),
            session_neutral: None,
            fit: None,
            fit_limits: None,
            signal_settings: None,
            fit_view: None,
            fit_summary: None,
            fit_method: InputMethod::Diagonal,
            heldout_view: HeldOutView::default(),
            heldout_method: None,
            loaded_binding: None,
            applied_context: None,
            processor: None,
            profiles: Vec::new(),
            profile_message: None,
            saved_path: None,
            messages: Vec::new(),
            fatal: false,
            last_error: None,
            last_capture: None,
        }
    }

    #[cfg(test)]
    fn with_test_session(transport: Box<dyn LineTransport + Send>, session_id: u32) -> Self {
        let mut service = Self::new(transport);
        service.session_id = session_id;
        service
    }

    pub fn execute(&mut self, command: Command, store_root: &Path) -> Result<Reply, ServiceError> {
        self.sample_events.clear();
        let snapshot = matches!(&command, Command::Snapshot);
        if self.fatal && !matches!(&command, Command::Snapshot | Command::Disconnect) {
            return Err(ServiceError::Fatal(
                "live session is invalid; disconnect and reconnect".into(),
            ));
        }
        let result = self.execute_inner(command, store_root);
        if let Err(error) = &result {
            self.sample_events.clear();
            self.last_error = Some(error.to_string());
            if error.fatal() {
                let _ = self.persist_active_capture(store_root, "protocol or transport failure");
                self.fatal = true;
                self.streaming = false;
                self.invalidate_calibration("device/protocol state invalidated");
            }
        }
        match result {
            Ok(()) => Ok(self.reply(None)),
            Err(error) if snapshot => Ok(self.reply(Some(error.to_string()))),
            Err(error) => Err(error),
        }
    }

    fn execute_inner(&mut self, command: Command, store_root: &Path) -> Result<(), ServiceError> {
        match command {
            Command::Snapshot => Ok(()),
            Command::Connect => self.connect(store_root),
            Command::Disconnect => self.disconnect(store_root),
            Command::Info => self.refresh_info(store_root),
            Command::StreamStart => self.stream_start(),
            Command::StreamTick => self.stream_tick(store_root),
            Command::StreamStop => {
                self.resume_after_tare = false;
                self.stream_stop(store_root, "capture interrupted by stream stop")
            }
            Command::Tare { confirmed } => self.tare(confirmed, store_root),
            Command::Provision {
                confirmed,
                device_id,
            } => self.provision(confirmed, &device_id, store_root),
            Command::Setup { setup } => self.stage_setup(store_root, *setup),
            Command::SaveDeviceSettings {
                expected_generation,
                known_mask,
                negative_mask,
            } => self.save_device_settings(
                store_root,
                expected_generation,
                known_mask,
                negative_mask,
            ),
            Command::SetOperator {
                operator,
                posture,
                grip,
            } => self.set_operator(store_root, operator, posture, grip),
            Command::SkipDirection { axis, direction } => {
                self.skip_direction(store_root, axis, direction)
            }
            Command::CaptureStart {
                phase,
                axis,
                direction,
                repeat,
                level,
                instruction,
                intent,
                sample_count,
            } => self.capture_start(
                store_root,
                phase,
                axis,
                direction,
                repeat,
                level,
                instruction,
                intent,
                sample_count,
            ),
            Command::CaptureTick => self.capture_tick(store_root),
            Command::CaptureAbort => self.abort_capture(store_root, "operator aborted capture"),
            Command::Fit {
                limits,
                signal_settings,
                method,
            } => self.fit(
                store_root,
                limits.into(),
                signal_settings.into(),
                method.map(Into::into),
            ),
            Command::VerifyHeldOut { method } => self.verify_heldout(method.into()),
            Command::SaveCalibration {
                profile_revision,
                method,
                confirmed,
            } => self.save_calibration(store_root, profile_revision, method, confirmed),
            Command::ListCalibrations => self.list_calibrations(store_root),
            Command::LoadCalibration { record_path } => {
                self.load_calibration(store_root, &record_path)
            }
            Command::ApplyCalibration { confirmed, method } => {
                self.apply_calibration(confirmed, method.into())
            }
        }
    }

    fn reply(&self, error: Option<String>) -> Reply {
        Reply {
            view: self.snapshot_view(),
            sample_events: self.sample_events.clone(),
            error,
        }
    }

    fn snapshot_view(&self) -> LiveSnapshot {
        let diagnostics = self.stream_monitor.snapshot(self.clock.now());
        let sample_age_ms = self.latest_received.map(|received| {
            (self
                .clock
                .now()
                .saturating_duration_since(received)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64)
                .saturating_add(diagnostics.relative_backlog_ms.unwrap_or(0))
        });
        let sample_fresh = self.latest_sample_usable
            && sample_age_ms.is_some_and(|age| age <= MAX_SAMPLE_AGE.as_millis() as u64);
        LiveSnapshot {
            connected: self.info.is_some() && !self.fatal,
            ready: self.processor.is_some() && sample_fresh && !self.fatal,
            session_neutral_valid: self.session_neutral.is_some() && !self.fatal,
            sample_fresh,
            sample_age_ms,
            diagnostics,
            profile: self.loaded_binding.as_ref().map(|binding| ActiveProfile {
                record: binding.parameter_record.clone(),
                revision: binding.profile_revision,
                assembly_revision: binding.setup.assembly_revision,
                config_revision: binding.setup.config_revision,
                method: match self.heldout_method.unwrap_or(self.fit_method) {
                    InputMethod::Diagonal => MethodDto::Diagonal,
                    InputMethod::Coupled => MethodDto::Coupled,
                },
            }),
            message: self.last_error.clone().unwrap_or_else(|| {
                if self.processor.is_some() {
                    "current profile applied".into()
                } else if self.setup.is_none() {
                    "confirm setup and device settings".into()
                } else if self.fit.is_none() {
                    "fit or load a compatible calibration profile".into()
                } else if !self.heldout_view.passed {
                    "complete and verify independent held-out trials".into()
                } else if self.session_neutral.is_none() {
                    "capture a fresh session neutral".into()
                } else {
                    "calibration is not applied".into()
                }
            }),
            phase: if self.streaming {
                "streaming"
            } else {
                "connected"
            }
            .into(),
            info: self.info.as_ref().map(info_view),
            sample: self.latest_sample.as_ref().map(sample_view),
            output: sample_fresh.then(|| self.latest_output.clone()).flatten(),
            setup: self
                .setup
                .as_ref()
                .map(|setup| setup_view(setup, self.setup_evidence.as_ref())),
            operator: self.operator.clone(),
            capture: self
                .active_capture
                .as_ref()
                .map(|c| CaptureView {
                    phase: format!("{:?}", c.window.phase).to_ascii_lowercase(),
                    active: c.pending_archive.is_none(),
                    samples: c.window.samples.len(),
                    count: c.requested,
                    complete: c.pending_archive.is_some(),
                    completed: c
                        .pending_archive
                        .as_ref()
                        .is_some_and(|archive| archive.committed),
                    done: c.window.samples.len(),
                    required: c.requested,
                    reason: c
                        .pending_archive
                        .as_ref()
                        .and_then(|archive| archive.reason.clone()),
                })
                .or_else(|| self.latest_capture_view()),
            fit: self.fit_view.clone(),
            heldout: self.heldout_view.clone(),
            profiles: self.profiles.clone(),
            profile_message: self.profile_message.clone(),
            saved_path: self.saved_path.clone(),
            messages: self.messages.clone(),
        }
    }

    fn latest_capture_view(&self) -> Option<CaptureView> {
        self.last_capture.clone()
    }

    fn push_message(&mut self, message: impl Into<String>) {
        self.messages.push(message.into());
        if self.messages.len() > 16 {
            self.messages.remove(0);
        }
    }

    fn invalidate_calibration(&mut self, message: &str) {
        if self
            .active_capture
            .as_ref()
            .is_some_and(|capture| capture.pending_archive.is_none())
        {
            self.active_capture = None;
        }
        self.global_neutral = None;
        self.trials.clear();
        self.heldout.clear();
        self.session_neutral = None;
        self.fit = None;
        self.fit_limits = None;
        self.signal_settings = None;
        self.fit_view = None;
        self.fit_summary = None;
        self.fit_method = InputMethod::Diagonal;
        self.heldout_view = HeldOutView::default();
        self.heldout_method = None;
        self.loaded_binding = None;
        self.applied_context = None;
        self.processor = None;
        self.latest_output = None;
        self.push_message(message);
    }

    fn invalidate_fit(&mut self, message: &str) {
        self.session_neutral = None;
        self.fit = None;
        self.fit_limits = None;
        self.signal_settings = None;
        self.fit_view = None;
        self.fit_summary = None;
        self.fit_method = InputMethod::Diagonal;
        self.heldout.clear();
        self.heldout_view = HeldOutView::default();
        self.heldout_method = None;
        self.loaded_binding = None;
        self.applied_context = None;
        self.processor = None;
        self.latest_output = None;
        self.push_message(message);
    }

    fn invalidate_session(&mut self, message: &str) {
        self.stream_monitor.reset();
        self.streaming = false;
        self.resume_after_tare = false;
        self.last_sequence = None;
        self.timeouts = 0;
        self.latest_sample = None;
        self.latest_received = None;
        self.latest_sample_usable = false;
        self.pending_setup = None;
        self.invalidate_calibration(message);
    }

    fn connect(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        if self.info.is_some() {
            return Err(ServiceError::Invalid("already connected".into()));
        }
        self.transport.discard_input()?;
        let info = self.query_info()?;
        if self.session_id == 0 {
            self.session_id = random_session_id()?;
        }
        self.connection_epoch = self.connection_epoch.wrapping_add(1).max(1);
        self.info = Some(info);
        self.setup = None;
        self.setup_evidence = None;
        self.setup_observation = None;
        self.setup_before_settings = None;
        self.setup_after_settings = None;
        self.operator = None;
        self.invalidate_calibration("new connection requires fresh session state");
        self.load_setup_for_current(store_root)?;
        Ok(())
    }

    fn disconnect(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        if self.active_capture.is_some() {
            self.persist_active_capture(store_root, "capture interrupted by disconnect")?;
        }
        if self.streaming && !self.fatal {
            self.stream_stop(store_root, "capture interrupted by disconnect")?;
        }
        self.invalidate_session("disconnected; capture and session readiness invalidated");
        self.session_id = 0;
        self.info = None;
        self.setup = None;
        self.setup_evidence = None;
        self.setup_observation = None;
        self.setup_before_settings = None;
        self.setup_after_settings = None;
        self.operator = None;
        self.fatal = false;
        Ok(())
    }

    fn query_info(&mut self) -> Result<DeviceInfo, ServiceError> {
        let request = self.next_request();
        let command = if self.safe_metadata_probe {
            format!("?3,{request}\n")
        } else {
            protocol_v3::info_command(request)?
        };
        self.transport.write_line(&command)?;
        let started = Instant::now();
        for _ in 0..READ_BUDGET {
            let remaining = INFO_TIMEOUT
                .checked_sub(started.elapsed())
                .ok_or_else(|| ServiceError::Fatal("info request timed out".into()))?;
            let line = self.transport.read_line(remaining)?;
            match protocol_v3::parse_line(&line)? {
                Frame::Info {
                    request: actual,
                    info,
                } if actual == request => return Ok(info),
                Frame::Error {
                    request: actual,
                    token,
                } if actual == request => {
                    return Err(ServiceError::Fatal(format!(
                        "device rejected info request: {token}"
                    )));
                }
                Frame::Info { .. }
                | Frame::Ok { .. }
                | Frame::Error { .. }
                | Frame::Sample(_)
                | Frame::Other => {}
            }
        }
        Err(ServiceError::Fatal(
            "info request exceeded bounded line budget".into(),
        ))
    }

    fn refresh_info(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        self.require_connected()?;
        let old = self.info.clone().unwrap();
        let new = self.query_info()?;
        if info_context_changed(&old, &new) {
            self.persist_active_capture(store_root, "device context changed during capture")?;
            self.info = Some(new);
            self.invalidate_session(
                "identity, firmware, settings, or tare changed; capture new evidence",
            );
            return Err(ServiceError::Fatal(
                "device context changed while connected".into(),
            ));
        }
        self.info = Some(new);
        Ok(())
    }

    fn stream_start(&mut self) -> Result<(), ServiceError> {
        self.require_connected()?;
        if self.streaming {
            self.resume_after_tare = false;
            return Ok(());
        }
        let request = self.next_request();
        if self.session_id == 0 {
            return Err(ServiceError::Fatal(
                "stream session challenge was not initialized by Connect".into(),
            ));
        }
        self.transport.discard_input()?;
        let command = protocol_v3::stream_command(request, self.session_id, true)?;
        self.transport.write_line(&command)?;
        let acknowledged = self.wait_ok(request, "stream")?;
        if acknowledged != self.session_id {
            return Err(ServiceError::Fatal(
                "stream acknowledgement session mismatch".into(),
            ));
        }
        self.streaming = true;
        self.stream_monitor.reset();
        self.resume_after_tare = false;
        self.last_sequence = None;
        self.timeouts = 0;
        self.latest_sample = None;
        self.latest_received = None;
        self.latest_sample_usable = false;
        Ok(())
    }

    fn stream_stop(&mut self, store_root: &Path, reason: &str) -> Result<(), ServiceError> {
        self.require_connected()?;
        if self.active_capture.is_some() {
            self.persist_active_capture(store_root, reason)?;
        }
        if !self.streaming {
            return Ok(());
        }
        self.processor = None;
        self.latest_output = None;
        let request = self.next_request();
        let command = protocol_v3::stream_command(request, self.session_id, false)?;
        self.transport.write_line(&command)?;
        let acknowledged = self.wait_ok(request, "stream")?;
        if acknowledged != self.session_id {
            return Err(ServiceError::Fatal(
                "stream stop acknowledgement session mismatch".into(),
            ));
        }
        self.streaming = false;
        self.last_sequence = None;
        self.latest_received = None;
        self.latest_sample_usable = false;
        self.session_neutral = None;
        Ok(())
    }

    fn wait_ok(&mut self, request: u32, operation: &str) -> Result<u32, ServiceError> {
        let started = Instant::now();
        for _ in 0..READ_BUDGET {
            let remaining = INFO_TIMEOUT.checked_sub(started.elapsed()).ok_or_else(|| {
                ServiceError::Fatal(format!("{operation} acknowledgement timed out"))
            })?;
            let line = self.transport.read_line(remaining)?;
            match protocol_v3::parse_line(&line)? {
                Frame::Ok {
                    request: actual,
                    operation: actual_op,
                    value,
                } if actual == request && actual_op == operation => return Ok(value),
                Frame::Error {
                    request: actual,
                    token,
                } if actual == request => {
                    return Err(ServiceError::Invalid(format!(
                        "device rejected {operation}: {token}"
                    )));
                }
                Frame::Info { .. }
                | Frame::Ok { .. }
                | Frame::Error { .. }
                | Frame::Sample(_)
                | Frame::Other => {}
            }
        }
        Err(ServiceError::Fatal(format!(
            "{operation} acknowledgement exceeded bounded line budget"
        )))
    }

    fn capture_acquisition_barrier(&mut self) -> Result<(), ServiceError> {
        self.latest_sample = None;
        self.latest_received = None;
        self.latest_sample_usable = false;
        self.latest_output = None;
        self.last_capture = None;
        self.streaming = false;

        // Ordered stop/resume acknowledgements drain pre-window samples. Reuse
        // the challenge so firmware preserves tare and sequence, while resume
        // clears its fresh mask. Do not use stream_stop's calibration teardown.
        for enabled in [false, true] {
            let request = self.next_request();
            let result = (|| {
                let command = protocol_v3::stream_command(request, self.session_id, enabled)?;
                self.transport.write_line(&command)?;
                let acknowledged = self.wait_ok(request, "stream")?;
                if acknowledged != self.session_id {
                    return Err(ServiceError::Fatal(
                        "capture barrier acknowledgement session mismatch".into(),
                    ));
                }
                Ok(())
            })();
            result.map_err(|error: ServiceError| {
                ServiceError::Fatal(format!("capture acquisition barrier failed: {error}"))
            })?;
        }
        self.streaming = true;
        self.timeouts = 0;
        self.stream_monitor.reset();
        // Keep last_sequence: same-challenge resume never resets sequence.
        Ok(())
    }

    fn stream_tick(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        self.require_connected()?;
        if !self.streaming {
            return Err(ServiceError::Invalid("stream is not active".into()));
        }
        if self
            .active_capture
            .as_ref()
            .is_some_and(|capture| capture.pending_archive.is_some())
        {
            self.persist_active_capture(store_root, "capture archive retry")?;
        }
        let started = self.clock.now();
        let mut lines = 0usize;
        let mut samples = 0usize;
        while lines < STREAM_DRAIN_BUDGET {
            let Some(remaining) =
                STREAM_DRAIN_TIME.checked_sub(self.clock.now().saturating_duration_since(started))
            else {
                break;
            };
            if remaining.is_zero() {
                break;
            }
            let line = match self.transport.read_line(remaining.min(STREAM_READ_SLICE)) {
                Ok(line) => line,
                Err(TransportError::Timeout) => break,
                Err(error) => return Err(ServiceError::Transport(error)),
            };
            lines += 1;
            match protocol_v3::parse_line(&line)? {
                Frame::Sample(sample) => {
                    self.accept_sample(sample, store_root)?;
                    samples += 1;
                }
                Frame::Error { request, token } => {
                    return Err(ServiceError::Fatal(format!(
                        "device stream error request={request}: {token}"
                    )));
                }
                Frame::Info { .. } | Frame::Ok { .. } | Frame::Other => {}
            }
        }
        self.stream_monitor.tick(
            lines,
            samples,
            lines >= STREAM_DRAIN_BUDGET
                || self.clock.now().saturating_duration_since(started) >= STREAM_DRAIN_TIME,
        );
        if samples > 0 {
            self.timeouts = 0;
        } else {
            self.timeouts = self.timeouts.saturating_add(1);
            if self.latest_received.is_some_and(|received| {
                self.clock.now().saturating_duration_since(received) > MAX_SAMPLE_AGE
            }) {
                if self.latest_sample_usable {
                    self.stream_monitor.stale();
                }
                self.processor = None;
                self.applied_context = None;
                self.latest_output = None;
                self.session_neutral = None;
                self.latest_sample_usable = false;
                self.push_message("sample stream is stale; capture a fresh session neutral and reapply the profile");
            }
            if self.timeouts >= SAMPLE_TIMEOUT_TICKS {
                return Err(ServiceError::Fatal(
                    "quality stream timeout; reconnect before reuse".into(),
                ));
            }
        }
        Ok(())
    }

    fn accept_sample(&mut self, sample: Sample, store_root: &Path) -> Result<(), ServiceError> {
        if self
            .active_capture
            .as_ref()
            .is_some_and(|capture| capture.pending_archive.is_some())
        {
            return Err(ServiceError::Persistence(
                "capture archive is pending; stream samples are held until it is stored".into(),
            ));
        }
        if sample.session != self.session_id {
            return Err(ServiceError::Fatal(
                "sample belongs to a different stream session".into(),
            ));
        }
        if let Some(previous) = self.last_sequence {
            let delta = sample.sequence.wrapping_sub(previous);
            if delta == 0 || delta >= (1_u32 << 31) {
                return Err(ServiceError::Fatal(
                    "sample sequence repeated or moved backwards".into(),
                ));
            }
        }
        let info = self
            .info
            .as_ref()
            .ok_or_else(|| ServiceError::Fatal("missing connected device context".into()))?;
        if sample.generation != info.generation
            || sample.tare_epoch != info.tare_epoch
            || sample.tare_valid != info.tare_valid
        {
            return Err(ServiceError::Fatal(
                "sample identity generation or tare epoch changed".into(),
            ));
        }
        let sample_epoch = self.epoch_token(sample.tare_epoch);
        let received = self.clock.now();
        self.last_sequence = Some(sample.sequence);
        self.latest_sample = Some(sample.clone());
        self.latest_received = Some(received);
        self.latest_output = None;

        let usable = sample.tare_valid
            && sample.flags.iter().all(|flag| *flag == FLAG_FRESH)
            && sample.raw.iter().all(Option::is_some)
            && sample.oriented.iter().all(Option::is_some);
        let backlog_ms = self
            .stream_monitor
            .observe(&sample, received, usable)
            .map_err(|message| ServiceError::Fatal(message.into()))?;
        if backlog_ms > MAX_SAMPLE_AGE.as_millis() as u64 {
            self.stream_monitor.stale();
            return Err(ServiceError::Fatal(
                "quality stream accumulated more than 250 ms of relative host backlog".into(),
            ));
        }
        self.latest_sample_usable = usable;
        if !usable {
            // Runtime validity is volatile. Completed independent calibration
            // evidence belongs to the unchanged context and is retained.
            self.processor = None;
            self.applied_context = None;
            self.session_neutral = None;
            self.latest_output = None;
            if self.active_capture.is_some() {
                self.persist_active_capture_with_sample(
                    store_root,
                    "sample quality was not fresh on all channels",
                    Some(&sample),
                )?;
            }
            self.push_message("non-fresh sample revoked runtime and session neutral; complete any interrupted evidence, then capture fresh neutral and explicitly apply");
            self.record_sample_event(&sample, received, backlog_ms)?;
            self.latest_sample = Some(sample);
            return Ok(());
        }

        let projected = if let Some(setup) = &self.setup {
            let wire_values = sample.oriented.map(|value| f64::from(value.unwrap()));
            let domain = DomainSample {
                domain: SampleDomain::DeviceOrientedCounts,
                values: wire_values,
                quality: InputQuality::Usable,
            };
            Some(project_device_oriented(setup, &domain)?.values().to_owned())
        } else {
            None
        };
        if let (Some(capture), Some(values)) = (&mut self.active_capture, projected) {
            let logical = values;
            let fw = FirmwareVectorSample {
                timestamp_ms: sample.device_ms,
                epoch: sample_epoch,
                values: logical,
                quality: InputQuality::Usable,
            };
            capture.first_sequence.get_or_insert(sample.sequence);
            capture.window.samples.push(fw);
            capture.window.wire_samples.push(sample.clone());
            if capture.window.samples.len() >= capture.requested {
                self.finish_capture(store_root)?;
            }
        }
        if let (Some(processor), Some(context), Some(setup)) = (
            &mut self.processor,
            self.applied_context.as_ref(),
            self.setup.as_ref(),
        ) {
            if let Some(values) = projected {
                let sample_for_core = FirmwareVectorSample {
                    timestamp_ms: sample.device_ms,
                    epoch: sample_epoch,
                    values,
                    quality: InputQuality::Usable,
                };
                match processor.process(context, &sample_for_core) {
                    Ok(out) => {
                        self.latest_output = Some(NormalizedOutput {
                            normalized: out.normalized,
                            command: out.command,
                            span_exceeded: out.span_exceeded,
                            unsupported_direction: out.unsupported_direction,
                            active: out.active,
                        })
                    }
                    Err(error) => {
                        self.processor = None;
                        self.applied_context = None;
                        self.session_neutral = None;
                        self.latest_output = None;
                        self.push_message(format!("runtime invalidated: {error}"));
                    }
                }
            } else {
                let _ = setup;
            }
        }
        if self.latest_output.is_some() {
            self.stream_monitor.normalized();
        }
        self.record_sample_event(&sample, received, backlog_ms)?;
        self.latest_sample = Some(sample);
        Ok(())
    }

    fn record_sample_event(
        &mut self,
        sample: &Sample,
        received: Instant,
        relative_backlog_ms: u64,
    ) -> Result<(), ServiceError> {
        if self.sample_events.len() >= STREAM_DRAIN_BUDGET {
            return Err(ServiceError::Fatal(
                "bounded sample delivery capacity exceeded".into(),
            ));
        }
        self.sample_events.push(SampleEvent {
            sample: sample_view(sample),
            host_received_us: self.stream_monitor.elapsed_us(received),
            relative_backlog_ms,
            ready: self.processor.is_some() && self.latest_sample_usable && !self.fatal,
            output: self.latest_output.clone(),
        });
        Ok(())
    }

    pub fn host_elapsed_us(&self) -> u64 {
        self.stream_monitor.elapsed_us(self.clock.now())
    }

    fn finish_capture(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        let Some(active) = self.active_capture.as_ref().cloned() else {
            return Ok(());
        };
        if active.pending_archive.is_some() {
            return Ok(());
        }
        let window = active.window.clone();
        let samples = window.samples.clone();
        let committed: Result<(), &str> = match window.phase {
            CapturePhase::Neutral => {
                self.global_neutral = Some(samples.clone());
                Ok(())
            }
            CapturePhase::SessionNeutral => {
                if samples.is_empty() {
                    Err("session neutral capture empty")
                } else if let Some(limits) = self.fit_limits.as_ref() {
                    match stable_neutral(&samples, limits) {
                        Ok(neutral) => {
                            self.session_neutral = Some(neutral);
                            Ok(())
                        }
                        Err(_) => Err(
                            "session neutral window exceeded the configured rest stability limits",
                        ),
                    }
                } else {
                    Err("fit limits are unavailable for session-neutral validation")
                }
            }
            CapturePhase::TrialNeutral
            | CapturePhase::Ramp
            | CapturePhase::Hold
            | CapturePhase::Release => {
                if let Some(axis) = window.axis {
                    let key = TrialKey {
                        axis,
                        direction: window.direction,
                        repeat: window.repeat,
                    };
                    let parts = self.trials.entry(key).or_default();
                    if parts.level.is_some_and(|level| level != window.level) {
                        Err(
                            "all phases of a training repeat must use the same positive effort level",
                        )
                    } else {
                        parts.level = Some(window.level);
                        let target = match window.phase {
                            CapturePhase::TrialNeutral => &mut parts.neutral,
                            CapturePhase::Ramp => &mut parts.ramp,
                            CapturePhase::Hold => &mut parts.hold,
                            CapturePhase::Release => &mut parts.release,
                            _ => unreachable!(),
                        };
                        *target = Some(samples.clone());
                        Ok(())
                    }
                } else {
                    Err("trial axis required")
                }
            }
            CapturePhase::HeldOutNeutral
            | CapturePhase::HeldOutRamp
            | CapturePhase::HeldOutHold
            | CapturePhase::HeldOutRelease => {
                if let Some(axis) = window.axis {
                    let key = HeldOutKey {
                        axis,
                        instruction: window.instruction.clone(),
                    };
                    let result = {
                        let parts = self.heldout.entry(key).or_default();
                        let intent_result = if matches!(
                            window.phase,
                            CapturePhase::HeldOutRamp | CapturePhase::HeldOutHold
                        ) {
                            match window.intent {
                                Some(intent)
                                    if parts.intent.is_some_and(|existing| existing != intent) =>
                                {
                                    Err("held-out ramp and hold must use the same intent")
                                }
                                Some(intent) => {
                                    parts.intent = Some(intent);
                                    Ok(())
                                }
                                None => Err("held-out ramp/hold intent is required"),
                            }
                        } else {
                            Ok(())
                        };
                        if intent_result.is_ok() {
                            let target = match window.phase {
                                CapturePhase::HeldOutNeutral => &mut parts.neutral,
                                CapturePhase::HeldOutRamp => &mut parts.ramp,
                                CapturePhase::HeldOutHold => &mut parts.hold,
                                CapturePhase::HeldOutRelease => &mut parts.release,
                                _ => unreachable!(),
                            };
                            *target = Some(samples.clone());
                        }
                        intent_result
                    };
                    if result.is_ok() {
                        self.heldout_view = HeldOutView {
                            passed: false,
                            reason: Some("new held-out evidence requires verification".into()),
                            trials: self.heldout.len(),
                            method: None,
                        };
                    }
                    result
                } else {
                    Err("held-out trial axis required")
                }
            }
        };
        let (committed_for_current_session, status, reason) = match committed {
            Ok(()) => {
                self.push_message(format!(
                    "capture: {:?} captured {} samples",
                    window.phase, active.requested
                ));
                (true, "completed", None)
            }
            Err(reason) => {
                self.push_message(format!("capture rejected: {reason}"));
                (false, "rejected", Some(reason))
            }
        };
        self.last_capture = Some(CaptureView {
            phase: format!("{:?}", window.phase).to_ascii_lowercase(),
            active: false,
            samples: samples.len(),
            count: active.requested,
            done: samples.len(),
            required: active.requested,
            complete: true,
            completed: committed_for_current_session,
            reason: reason.map(str::to_owned),
        });
        if let Some(capture) = self.active_capture.as_mut() {
            capture.pending_archive = Some(PendingArchive {
                committed: committed_for_current_session,
                status: status.into(),
                reason: reason.map(str::to_owned),
            });
        }
        self.persist_active_capture(store_root, "capture archive retry")
    }

    fn capture_tick(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        if self.active_capture.is_none() {
            return Ok(());
        }
        if self
            .active_capture
            .as_ref()
            .is_some_and(|capture| capture.pending_archive.is_some())
        {
            return self.persist_active_capture(store_root, "capture archive retry");
        }
        self.stream_tick(store_root)?;
        if self
            .active_capture
            .as_ref()
            .is_some_and(|capture| capture.started.elapsed() >= CAPTURE_TIMEOUT)
        {
            self.persist_active_capture(store_root, "capture exceeded the 8-second deadline")?;
        }
        Ok(())
    }

    fn abort_capture(&mut self, root: &Path, reason: &str) -> Result<(), ServiceError> {
        self.persist_active_capture(root, reason)
    }

    fn persist_active_capture(&mut self, root: &Path, reason: &str) -> Result<(), ServiceError> {
        self.persist_active_capture_with_sample(root, reason, None)
    }

    fn persist_active_capture_with_sample(
        &mut self,
        root: &Path,
        reason: &str,
        extra: Option<&Sample>,
    ) -> Result<(), ServiceError> {
        let Some(mut active) = self.active_capture.as_ref().cloned() else {
            return Ok(());
        };
        if active.pending_archive.is_none() {
            active.pending_archive = Some(PendingArchive {
                committed: false,
                status: "interrupted".into(),
                reason: Some(reason.into()),
            });
        }
        if let Some(sample) = extra {
            active.window.wire_samples.push(sample.clone());
        }
        let pending = active
            .pending_archive
            .as_ref()
            .expect("archive metadata is set")
            .clone();
        self.active_capture = Some(active.clone());
        self.last_capture = Some(CaptureView {
            phase: format!("{:?}", active.window.phase).to_ascii_lowercase(),
            active: false,
            samples: active.window.samples.len(),
            count: active.requested,
            done: active.window.samples.len(),
            required: active.requested,
            complete: true,
            completed: pending.committed,
            reason: pending.reason.clone(),
        });
        self.persist_capture_archive(
            root,
            &active,
            pending.committed,
            &pending.status,
            pending.reason.as_deref(),
        )?;
        self.active_capture = None;
        self.push_message(format!(
            "capture archived: {}",
            pending.reason.unwrap_or(pending.status)
        ));
        Ok(())
    }

    fn persist_capture_archive(
        &self,
        root: &Path,
        active: &ActiveCapture,
        committed: bool,
        status: &str,
        reason: Option<&str>,
    ) -> Result<(), ServiceError> {
        self.persist_capture_archive_with_wire(
            root,
            active,
            &active.window.wire_samples,
            committed,
            status,
            reason,
        )
    }

    fn ensure_capture_capacity(&self, root: &Path) -> Result<(), ServiceError> {
        let info = self.info.as_ref().ok_or_else(|| {
            ServiceError::Invalid("cannot capture without connected identity".into())
        })?;
        let directory = device_directory(root, provisioned_id(info)?)?.join("captures");
        let (count, bytes) = capture_archive_usage(&directory)?;
        if count >= MAX_CAPTURE_ARCHIVE_COUNT || bytes >= MAX_CAPTURE_ARCHIVE_BYTES {
            return Err(ServiceError::Persistence(
                "capture archive capacity is exhausted; no new capture was started".into(),
            ));
        }
        Ok(())
    }

    fn persist_capture_archive_with_wire(
        &self,
        root: &Path,
        active: &ActiveCapture,
        wires: &[Sample],
        committed: bool,
        status: &str,
        reason: Option<&str>,
    ) -> Result<(), ServiceError> {
        let info = self.info.as_ref().ok_or_else(|| {
            ServiceError::Persistence("cannot archive capture without connected identity".into())
        })?;
        let device_dir = device_directory(root, provisioned_id(info)?)?;
        fs::create_dir_all(&device_dir).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&device_dir)?;
        let dir = device_dir.join("captures");
        fs::create_dir_all(&dir).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&dir)?;
        let (count, archive_bytes) = capture_archive_usage(&dir)?;
        if count >= MAX_CAPTURE_ARCHIVE_COUNT {
            return Err(ServiceError::Persistence(
                "capture archive reached its 256-record bound".into(),
            ));
        }
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let record = StoredCapture {
            schema: 1,
            status: status.into(),
            committed_for_current_session: committed,
            archive_is_current_runtime_evidence: false,
            reason: reason.map(str::to_owned),
            phase: active.window.phase,
            axis: active.window.axis,
            direction: active.window.direction,
            repeat: active.window.repeat,
            level: active.window.level,
            instruction: active.window.instruction.clone(),
            intent: active.window.intent,
            requested_samples: active.requested,
            first_sequence: active.first_sequence,
            samples: active.window.samples.clone(),
            wire_samples: wires.iter().map(StoredWireSample::from).collect(),
        };
        let bytes =
            serde_json::to_vec(&record).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        if bytes.len() > MAX_CAPTURE_ARCHIVE_RECORD_BYTES {
            return Err(ServiceError::Persistence(
                "capture archive exceeds 1 MiB bound".into(),
            ));
        }
        if archive_bytes.saturating_add(bytes.len() as u64) > MAX_CAPTURE_ARCHIVE_BYTES {
            return Err(ServiceError::Persistence(
                "capture archive reached its total byte bound".into(),
            ));
        }
        let name = format!("capture-{}-{sequence:010}.srncap", std::process::id());
        atomic_create_immutable(&dir, &dir.join(name), &bytes)
    }

    // Mirrors the independent fields of the typed CaptureStart command.
    #[allow(clippy::too_many_arguments)]
    fn capture_start(
        &mut self,
        store_root: &Path,
        phase: CapturePhase,
        axis: Option<usize>,
        direction: i8,
        repeat: u8,
        level: f64,
        instruction: String,
        intent: Option<[f64; 7]>,
        sample_count: Option<usize>,
    ) -> Result<(), ServiceError> {
        self.require_stream_and_setup()?;
        if self.active_capture.is_some() {
            return Err(ServiceError::Invalid("another capture is active".into()));
        }
        let count = sample_count.unwrap_or(DEFAULT_CAPTURE_SAMPLES);
        if !(8..=MAX_CAPTURE_SAMPLES).contains(&count) {
            return Err(ServiceError::Invalid("sample_count must be 8..=64".into()));
        }
        self.ensure_capture_capacity(store_root)?;
        if ![-1, 1].contains(&direction)
            || repeat > 7
            || !level.is_finite()
            || level.abs() > 1.0
            || instruction.trim().is_empty()
            || instruction.len() > 1024
            || instruction.chars().any(char::is_control)
        {
            return Err(ServiceError::Invalid(
                "invalid capture instruction, direction, repeat or level".into(),
            ));
        }
        if axis.is_some_and(|axis| axis >= 7) {
            return Err(ServiceError::Invalid("axis must be 0..=6".into()));
        }
        if matches!(
            phase,
            CapturePhase::TrialNeutral
                | CapturePhase::Ramp
                | CapturePhase::Hold
                | CapturePhase::Release
        ) && (axis.is_none() || level <= 0.0)
        {
            return Err(ServiceError::Invalid(
                "training trial requires an axis and positive effort magnitude".into(),
            ));
        }
        if phase == CapturePhase::SessionNeutral {
            self.session_neutral = None;
            self.processor = None;
            self.applied_context = None;
        }
        if matches!(
            phase,
            CapturePhase::HeldOutNeutral
                | CapturePhase::HeldOutRamp
                | CapturePhase::HeldOutHold
                | CapturePhase::HeldOutRelease
        ) {
            if self.fit.is_none() {
                return Err(ServiceError::Invalid(
                    "fit must be computed before held-out capture".into(),
                ));
            }
            if axis.is_none() {
                return Err(ServiceError::Invalid("held-out trial axis required".into()));
            }
            let expected = intent.ok_or_else(|| {
                ServiceError::Invalid(
                    "held-out capture requires an explicit seven-axis intent".into(),
                )
            })?;
            if expected.iter().any(|v| !v.is_finite() || v.abs() > 1.0) {
                return Err(ServiceError::Invalid(
                    "held-out intent must be finite and within -1..=1".into(),
                ));
            }
            if matches!(
                phase,
                CapturePhase::HeldOutNeutral | CapturePhase::HeldOutRelease
            ) && expected.iter().any(|v| *v != 0.0)
            {
                return Err(ServiceError::Invalid(
                    "neutral/release intent must be all zero".into(),
                ));
            }
            if matches!(phase, CapturePhase::HeldOutRamp | CapturePhase::HeldOutHold)
                && expected.iter().all(|v| *v == 0.0)
            {
                return Err(ServiceError::Invalid(
                    "held-out ramp/hold requires a nonzero intent".into(),
                ));
            }
            if matches!(phase, CapturePhase::HeldOutRamp | CapturePhase::HeldOutHold) {
                let main_axis = axis.ok_or_else(|| {
                    ServiceError::Invalid("held-out ramp/hold requires its primary axis".into())
                })?;
                if expected[main_axis] == 0.0
                    || expected[main_axis].signum() != f64::from(direction)
                {
                    return Err(ServiceError::Invalid(
                        "held-out primary intent direction must match the instructed direction"
                            .into(),
                    ));
                }
            }
        }
        if phase == CapturePhase::SessionNeutral
            && (self.fit.is_none() || !self.heldout_view.passed)
        {
            return Err(ServiceError::Invalid(
                "verify held-out behavior before capturing the apply-session neutral".into(),
            ));
        }
        if matches!(
            phase,
            CapturePhase::TrialNeutral
                | CapturePhase::Ramp
                | CapturePhase::Hold
                | CapturePhase::Release
        ) {
            self.validate_training_capture_predecessor(phase, axis, direction, repeat, level)?;
        }
        self.capture_acquisition_barrier()?;
        if phase == CapturePhase::Neutral {
            self.global_neutral = None;
            self.trials.clear();
        } else if matches!(
            phase,
            CapturePhase::TrialNeutral
                | CapturePhase::Ramp
                | CapturePhase::Hold
                | CapturePhase::Release
        ) {
            self.prepare_training_retry(phase, axis.unwrap(), direction, repeat);
        }
        if !matches!(
            phase,
            CapturePhase::HeldOutNeutral
                | CapturePhase::HeldOutRamp
                | CapturePhase::HeldOutHold
                | CapturePhase::HeldOutRelease
                | CapturePhase::SessionNeutral
        ) && self.fit.is_some()
        {
            self.invalidate_fit("new training capture invalidated prior fit; other completed training trials remain available");
        }
        if matches!(
            phase,
            CapturePhase::HeldOutNeutral
                | CapturePhase::HeldOutRamp
                | CapturePhase::HeldOutHold
                | CapturePhase::HeldOutRelease
        ) {
            // Replacing a phase also invalidates its successors immediately,
            // so an aborted/in-progress retry cannot certify old evidence.
            let key = HeldOutKey {
                axis: axis.unwrap(),
                instruction: instruction.clone(),
            };
            if phase == CapturePhase::HeldOutNeutral {
                self.heldout.insert(key, HeldOutParts::default());
            } else {
                let parts = self.heldout.entry(key).or_default();
                match phase {
                    CapturePhase::HeldOutRamp => {
                        parts.ramp = None;
                        parts.hold = None;
                        parts.release = None;
                        parts.intent = None;
                    }
                    CapturePhase::HeldOutHold => {
                        parts.hold = None;
                        parts.release = None;
                    }
                    CapturePhase::HeldOutRelease => {
                        parts.release = None;
                    }
                    _ => unreachable!(),
                }
            }
            self.heldout_view = HeldOutView {
                passed: false,
                reason: Some(
                    "held-out capture started; complete all phases and verify again".into(),
                ),
                trials: self.heldout.len(),
                method: None,
            };
            self.heldout_method = None;
            self.session_neutral = None;
            self.processor = None;
            self.applied_context = None;
            self.latest_output = None;
        }
        self.active_capture = Some(ActiveCapture {
            window: CapturedWindow {
                phase,
                axis,
                direction,
                repeat,
                level,
                instruction,
                intent,
                samples: Vec::with_capacity(count),
                wire_samples: Vec::with_capacity(count),
            },
            requested: count,
            first_sequence: None,
            started: Instant::now(),
            pending_archive: None,
        });
        Ok(())
    }

    fn validate_training_capture_predecessor(
        &self,
        phase: CapturePhase,
        axis: Option<usize>,
        direction: i8,
        repeat: u8,
        level: f64,
    ) -> Result<(), ServiceError> {
        if self.global_neutral.is_none() {
            return Err(ServiceError::Invalid(
                "capture a new global natural-neutral baseline before training trials".into(),
            ));
        }
        let key = TrialKey {
            axis: axis.ok_or_else(|| ServiceError::Invalid("trial axis required".into()))?,
            direction,
            repeat,
        };
        let parts = self.trials.get(&key);
        let has = |select: fn(&TrialParts) -> &Option<Vec<FirmwareVectorSample>>| {
            parts.is_some_and(|value| select(value).is_some())
        };
        let predecessor_captured = match phase {
            CapturePhase::TrialNeutral => true,
            CapturePhase::Ramp => has(|parts| &parts.neutral),
            CapturePhase::Hold => has(|parts| &parts.neutral) && has(|parts| &parts.ramp),
            CapturePhase::Release => has(|parts| &parts.hold),
            _ => true,
        };
        if !predecessor_captured {
            return Err(ServiceError::Invalid("training retry requires the prior completed phases for this exact axis, direction, and repeat".into()));
        }
        if phase != CapturePhase::TrialNeutral
            && parts.is_some_and(|value| value.level.is_some_and(|captured| captured != level))
        {
            return Err(ServiceError::Invalid(
                "retry with a different effort level requires recapturing the trial neutral first"
                    .into(),
            ));
        }
        Ok(())
    }

    fn prepare_training_retry(
        &mut self,
        phase: CapturePhase,
        axis: usize,
        direction: i8,
        repeat: u8,
    ) {
        let key = TrialKey {
            axis,
            direction,
            repeat,
        };
        if phase == CapturePhase::TrialNeutral {
            self.trials.remove(&key);
            return;
        }
        if let Some(parts) = self.trials.get_mut(&key) {
            match phase {
                CapturePhase::Ramp => {
                    parts.ramp = None;
                    parts.hold = None;
                    parts.release = None;
                }
                CapturePhase::Hold => {
                    parts.hold = None;
                    parts.release = None;
                }
                CapturePhase::Release => {
                    parts.release = None;
                }
                _ => {}
            }
        }
    }

    fn fit(
        &mut self,
        store_root: &Path,
        limits: FitLimits,
        signal_settings: SignalSettings,
        method: Option<InputMethod>,
    ) -> Result<(), ServiceError> {
        self.require_stream_and_setup()?;
        self.persist_active_capture(store_root, "fit started while another capture was active")?;
        self.invalidate_fit("new fit requires independent held-out verification and fresh neutral");
        let current = self.current_context()?;
        let binding = self.make_binding("pending-profile.srncal", 1)?;
        let recording = self.build_recording()?;
        let fitted = binding.fit_device_oriented(&current, &recording, &limits)?;
        let summary = summary_record(fitted.summary());
        let view = fit_view(&fitted);
        self.fit = Some(fitted);
        self.fit_method = method.unwrap_or(InputMethod::Diagonal);
        self.fit_limits = Some(limits);
        self.signal_settings = Some(signal_settings);
        self.fit_summary = Some(summary);
        self.fit_view = Some(view);
        self.loaded_binding = Some(binding);
        self.heldout_view = HeldOutView {
            passed: false,
            reason: Some("fit complete; capture independent held-out trials".into()),
            trials: 0,
            method: None,
        };
        self.heldout_method = None;
        self.heldout.clear();
        self.session_neutral = None;
        self.profile_message = None;
        Ok(())
    }

    fn build_recording(&self) -> Result<CalibrationRecording, ServiceError> {
        let info = self
            .info
            .as_ref()
            .ok_or_else(|| ServiceError::Invalid("device info is unavailable".into()))?;
        let setup = self
            .setup
            .as_ref()
            .ok_or_else(|| ServiceError::Invalid("validated device setup is required".into()))?;
        let operator = self.operator.as_ref().ok_or_else(|| {
            ServiceError::Invalid("operator/posture/grip context is required".into())
        })?;
        let neutral = self.global_neutral.clone().ok_or_else(|| {
            ServiceError::Invalid("capture global natural-neutral evidence first".into())
        })?;
        let neutral_start = neutral
            .first()
            .ok_or_else(|| {
                ServiceError::Invalid("global natural-neutral evidence is empty".into())
            })?
            .timestamp_ms;
        let mut trials = Vec::new();
        let mut total = neutral.len();
        let mut ordered_trials = Vec::new();
        for (key, parts) in &self.trials {
            let (Some(before), Some(_ramp), Some(_loaded), Some(_released)) =
                (&parts.neutral, &parts.ramp, &parts.hold, &parts.release)
            else {
                continue;
            };
            if key.direction < 0 && parts.hold.is_none() {
                continue;
            }
            let first = before
                .first()
                .ok_or_else(|| ServiceError::Invalid("trial neutral evidence is empty".into()))?;
            let offset = first.timestamp_ms.wrapping_sub(neutral_start);
            if offset >= (1_u32 << 31) {
                return Err(ServiceError::Invalid(
                    "trial capture time is outside the global-neutral session ordering window"
                        .into(),
                ));
            }
            ordered_trials.push((offset, key.clone(), parts));
        }
        ordered_trials.sort_by_key(|(offset, key, _)| (*offset, key.clone()));
        for (_, key, parts) in ordered_trials {
            let (Some(before), Some(_ramp), Some(loaded), Some(released)) =
                (&parts.neutral, &parts.ramp, &parts.hold, &parts.release)
            else {
                continue;
            };
            total = total
                .checked_add(before.len())
                .and_then(|v| v.checked_add(loaded.len()))
                .and_then(|v| v.checked_add(released.len()))
                .ok_or_else(|| ServiceError::Invalid("capture sample bound overflow".into()))?;
            if total > MAX_CALIBRATION_SAMPLES {
                return Err(ServiceError::Invalid(
                    "calibration evidence exceeds bounded sample capacity".into(),
                ));
            }
            let axis_direction = setup
                .channels
                .iter()
                .find(|c| c.logical_axis == Some(key.axis))
                .and_then(|c| c.positive_direction.clone())
                .ok_or_else(|| ServiceError::Invalid("setup direction is incomplete".into()))?;
            let instruction = if key.direction > 0 {
                axis_direction
            } else {
                format!("reverse: {axis_direction}")
            };
            let id = format!(
                "axis{}-{}-repeat{}",
                key.axis,
                if key.direction > 0 {
                    "positive"
                } else {
                    "negative"
                },
                key.repeat
            );
            let measured_level = parts
                .level
                .ok_or_else(|| ServiceError::Invalid("training effort level is missing".into()))?;
            trials.push(GuidedTrial {
                id,
                axis: key.axis,
                level: f64::from(key.direction) * measured_level,
                instruction,
                neutral: before.clone(),
                loaded: loaded.clone(),
                released: released.clone(),
            });
        }
        if trials.iter().filter(|t| t.level > 0.0).count() < 14 {
            return Err(ServiceError::Invalid(
                "two complete positive hold/release trials per axis are required".into(),
            ));
        }
        let mut axes = std::array::from_fn(|axis| AxisDefinition {
            channel: axis,
            positive_direction: String::new(),
        });
        for channel in &setup.channels {
            let axis = channel
                .logical_axis
                .ok_or_else(|| ServiceError::Invalid("setup mapping incomplete".into()))?;
            axes[axis] = AxisDefinition {
                channel: axis,
                positive_direction: channel.positive_direction.clone().unwrap_or_default(),
            };
        }
        let context = Context {
            source: self.context_source(info),
            user: operator.operator.clone(),
            posture: operator.posture.clone(),
        };
        Ok(CalibrationRecording {
            context,
            axes,
            neutral,
            trials,
        })
    }

    fn verify_heldout(&mut self, method: InputMethod) -> Result<(), ServiceError> {
        self.processor = None;
        self.applied_context = None;
        self.latest_output = None;
        self.heldout_view = HeldOutView {
            passed: false,
            reason: Some("held-out verification did not complete".into()),
            trials: 0,
            method: None,
        };
        self.heldout_method = None;
        self.session_neutral = None;
        if self.active_capture.is_some() {
            return Err(ServiceError::Invalid(
                "finish or abort the active capture before held-out verification".into(),
            ));
        }
        let fit = self.fit.as_ref().ok_or_else(|| {
            ServiceError::Invalid("fit calibration before held-out verification".into())
        })?;
        let settings = self
            .signal_settings
            .ok_or_else(|| ServiceError::Invalid("signal settings are unavailable".into()))?;
        let limits = self
            .fit_limits
            .as_ref()
            .ok_or_else(|| ServiceError::Invalid("fit limits are unavailable".into()))?;
        let current_context = self.current_context()?;
        let context = Context {
            source: self.context_source(self.info.as_ref().unwrap()),
            user: current_context.operator.operator.clone(),
            posture: current_context.operator.posture.clone(),
        };
        let mut checked = 0;
        let mut failures = Vec::new();
        let mut positive_axes = [false; 7];
        let mut negative_axes = [false; 7];
        let mut weak_single_axis = false;
        let mut combined_axes = false;
        for (key, parts) in &self.heldout {
            let (Some(neutral), Some(ramp), Some(hold), Some(release), Some(intent)) = (
                &parts.neutral,
                &parts.ramp,
                &parts.hold,
                &parts.release,
                parts.intent,
            ) else {
                failures.push(format!(
                    "{}: neutral/ramp/hold/release evidence is incomplete",
                    key.instruction
                ));
                continue;
            };
            if ramp.is_empty() || hold.is_empty() || release.is_empty() || neutral.is_empty() {
                failures.push(format!("{}: held-out phase is empty", key.instruction));
                continue;
            }
            if neutral
                .iter()
                .chain(ramp)
                .chain(hold)
                .chain(release)
                .any(|s| s.epoch != self.epoch_token(self.info.as_ref().unwrap().tare_epoch))
            {
                failures.push(format!("{}: held-out epoch is stale", key.instruction));
                continue;
            }
            let active_intent = intent
                .iter()
                .enumerate()
                .filter(|(_, value)| **value != 0.0)
                .collect::<Vec<_>>();
            if active_intent.len() == 1 {
                let (axis, value) = active_intent[0];
                if axis == key.axis && *value == 1.0 {
                    positive_axes[axis] = true;
                }
                if axis == key.axis && *value == -1.0 {
                    negative_axes[axis] = true;
                }
                if axis == key.axis && value.abs() < 1.0 {
                    weak_single_axis = true;
                }
            } else if active_intent.len() >= 2 {
                combined_axes = true;
            }
            let stable_baseline = match stable_neutral(neutral, limits) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "{}: unstable held-out neutral: {error}",
                        key.instruction
                    ));
                    continue;
                }
            };
            let mut processor = match InputProcessor::new_for_session(
                fit,
                context.clone(),
                &stable_baseline,
                method,
                settings,
            ) {
                Ok(value) => value,
                Err(error) => {
                    failures.push(format!(
                        "{}: selected method is unavailable: {error}",
                        key.instruction
                    ));
                    continue;
                }
            };
            let mut phase_failed = None;
            for sample in neutral {
                if let Err(e) = processor.process(&context, sample) {
                    phase_failed = Some(e.to_string());
                    break;
                }
            }
            if phase_failed.is_none() {
                if let Err(error) = processor.begin_captured_window() {
                    phase_failed = Some(error.to_string());
                } else {
                    for sample in ramp {
                        match processor.process(&context, sample) {
                            Ok(frame)
                                if command_matches_intent(
                                    &frame.command,
                                    &intent,
                                    limits.direction_tolerance,
                                    false,
                                ) => {}
                            Ok(_) => {
                                phase_failed = Some("ramp direction or off-axis response exceeded the explicit intent tolerance".into());
                                break;
                            }
                            Err(e) => {
                                phase_failed = Some(e.to_string());
                                break;
                            }
                        }
                    }
                }
            }
            let mut hold_activated = false;
            if phase_failed.is_none() {
                if let Err(error) = processor.begin_captured_window() {
                    phase_failed = Some(error.to_string());
                } else {
                    for sample in hold {
                        match processor.process(&context, sample) {
                            Ok(frame)
                                if command_matches_intent(
                                    &frame.command,
                                    &intent,
                                    limits.direction_tolerance,
                                    true,
                                ) =>
                            {
                                hold_activated |= frame
                                    .active
                                    .iter()
                                    .enumerate()
                                    .any(|(axis, active)| intent[axis] != 0.0 && *active)
                            }
                            Ok(_) => {
                                phase_failed = Some("held-out hold was weak, wrong-direction, or outside the explicit intent tolerance".into());
                                break;
                            }
                            Err(e) => {
                                phase_failed = Some(e.to_string());
                                break;
                            }
                        }
                    }
                }
                if phase_failed.is_none() && !hold_activated {
                    phase_failed = Some("held-out hold never activated an intended axis".into());
                }
            }
            if phase_failed.is_none() {
                if let Err(error) = processor.begin_captured_window() {
                    phase_failed = Some(error.to_string());
                } else {
                    for sample in release {
                        match processor.process(&context, sample) {
                            Ok(frame) if frame.active.iter().all(|active| !active) => {}
                            Ok(_) => {
                                phase_failed = Some(
                                    "release did not return every axis below its configured gate"
                                        .into(),
                                );
                                break;
                            }
                            Err(e) => {
                                phase_failed = Some(e.to_string());
                                break;
                            }
                        }
                    }
                }
            }
            if let Some(error) = phase_failed {
                failures.push(format!("{}: {error}", key.instruction));
            } else {
                checked += 1;
            }
        }
        for axis in 0..7 {
            if !positive_axes[axis] {
                failures.push(format!(
                    "positive single-axis held-out evidence is missing for axis {axis}"
                ));
            }
            if fit.summary().negative_span_counts[axis].is_some() && !negative_axes[axis] {
                failures.push(format!(
                    "negative held-out evidence is required for measured reverse axis {axis}"
                ));
            }
        }
        if !weak_single_axis {
            failures.push("a lower-effort single-axis held-out trial is required".into());
        }
        if !combined_axes {
            failures.push("a combined two-or-more-axis held-out trial is required".into());
        }
        if checked == 0 || !failures.is_empty() {
            self.heldout_view = HeldOutView {
                passed: false,
                reason: Some(if failures.is_empty() {
                    "no independent held-out trials were captured".into()
                } else {
                    failures.join("; ")
                }),
                trials: checked,
                method: None,
            };
            self.session_neutral = None;
            return Err(ServiceError::Invalid(
                self.heldout_view.reason.clone().unwrap(),
            ));
        }
        self.heldout_method = Some(method);
        self.heldout_view = HeldOutView { passed: true, reason: Some("independent positive/required reverse axes, lower-effort, combined response, and release captures passed".into()), trials: checked, method: Some(format!("{method:?}").to_ascii_lowercase()) };
        Ok(())
    }

    fn set_operator(
        &mut self,
        store_root: &Path,
        operator: String,
        posture: String,
        grip: String,
    ) -> Result<(), ServiceError> {
        self.require_connected()?;
        let next = OperatorContext {
            operator,
            posture,
            grip,
        };
        next.validate()?;
        if self.operator.as_ref() != Some(&next) {
            self.persist_active_capture(store_root, "operator/posture/grip context changed")?;
            self.operator = Some(next);
            self.invalidate_calibration(
                "operator/posture/grip changed; previous evidence and neutral were invalidated",
            );
        }
        Ok(())
    }

    fn skip_direction(
        &mut self,
        store_root: &Path,
        axis: usize,
        direction: i8,
    ) -> Result<(), ServiceError> {
        self.require_stream_and_setup()?;
        if axis >= 7 || direction != -1 {
            return Err(ServiceError::Invalid(
                "only an explicitly skipped negative direction for axis 0..=6 is supported".into(),
            ));
        }
        if !self.latest_sample_usable
            || self
                .latest_received
                .is_none_or(|received| received.elapsed() > MAX_SAMPLE_AGE)
        {
            return Err(ServiceError::Invalid("negative direction skip requires a recent all-channel fresh sample and current setup".into()));
        }
        self.persist_active_capture(
            store_root,
            "negative direction skipped after partial capture",
        )?;
        self.trials
            .retain(|key, _| key.axis != axis || key.direction != direction);
        self.invalidate_fit(
            "negative direction explicitly skipped; that axis will have no measured reverse span",
        );
        self.persist_skip_marker(store_root, axis)?;
        Ok(())
    }

    fn persist_skip_marker(&self, root: &Path, axis: usize) -> Result<(), ServiceError> {
        let info = self.info.as_ref().ok_or_else(|| {
            ServiceError::Persistence("cannot archive skip without connected identity".into())
        })?;
        let directory = device_directory(root, provisioned_id(info)?)?;
        fs::create_dir_all(&directory).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&directory)?;
        let captures = directory.join("captures");
        fs::create_dir_all(&captures).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&captures)?;
        let (count, archive_bytes) = capture_archive_usage(&captures)?;
        if count >= MAX_CAPTURE_ARCHIVE_COUNT {
            return Err(ServiceError::Persistence(
                "capture archive reached its 256-record bound".into(),
            ));
        }
        let record = StoredCapture {
            schema: 1,
            status: "negative-direction-skipped".into(),
            committed_for_current_session: false,
            archive_is_current_runtime_evidence: false,
            reason: Some("operator explicitly skipped reverse-direction measurement; no negative span may be inferred".into()),
            phase: CapturePhase::TrialNeutral,
            axis: Some(axis),
            direction: -1,
            repeat: 0,
            level: 0.0,
            instruction: "explicit negative-direction skip".into(),
            intent: None,
            requested_samples: 0,
            first_sequence: None,
            samples: Vec::new(),
            wire_samples: Vec::new(),
        };
        let bytes =
            serde_json::to_vec(&record).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        if archive_bytes.saturating_add(bytes.len() as u64) > MAX_CAPTURE_ARCHIVE_BYTES {
            return Err(ServiceError::Persistence(
                "capture archive reached its total byte bound".into(),
            ));
        }
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!("capture-{}-{sequence:010}.srncap", std::process::id());
        atomic_create_immutable(&captures, &captures.join(name), &bytes)
    }

    fn stage_setup(&mut self, store_root: &Path, setup: SetupDto) -> Result<(), ServiceError> {
        self.require_connected()?;
        self.persist_active_capture(store_root, "setup changed during capture")?;
        let info = self.info.as_ref().unwrap();
        if setup.assembly_revision == 0 || setup.config_revision == 0 {
            return Err(ServiceError::Invalid(
                "assembly_revision and config_revision must be nonzero".into(),
            ));
        }
        let mut seen = [false; 7];
        for (wire, channel) in setup.channels.iter().enumerate() {
            if channel.wire != wire
                || !channel.confirmed
                || channel.logical_axis.is_none()
                || channel
                    .location
                    .as_ref()
                    .is_none_or(|v| v.trim().is_empty())
                || channel
                    .direction
                    .as_ref()
                    .is_none_or(|v| v.trim().is_empty())
                || !channel.polarity.is_some_and(|p| p == -1 || p == 1)
                || channel
                    .evidence
                    .as_ref()
                    .is_none_or(|v| v.trim().is_empty() || v.len() > 512)
            {
                return Err(ServiceError::Invalid(format!(
                    "wire channel {wire} needs confirmed mapping, direction, location, polarity, and a live observation reference"
                )));
            }
            let axis = channel.logical_axis.unwrap();
            if axis >= 7 || seen[axis] {
                return Err(ServiceError::Invalid(
                    "logical axes must be a permutation of 0..=6".into(),
                ));
            }
            seen[axis] = true;
        }
        if info.identity == DeviceIdentity::Unprovisioned {
            return Err(ServiceError::Invalid(
                "provision the device before saving setup".into(),
            ));
        }
        let sample = self.latest_sample.as_ref().ok_or_else(|| {
            ServiceError::Invalid(
                "receive a fresh v3 raw sample before saving setup evidence".into(),
            )
        })?;
        if sample.flags.iter().any(|flag| *flag != FLAG_FRESH)
            || sample.raw.iter().any(Option::is_none)
            || sample.generation != info.generation
            || sample.tare_epoch != info.tare_epoch
            || self
                .latest_received
                .is_none_or(|received| received.elapsed() > MAX_SAMPLE_AGE)
        {
            return Err(ServiceError::Invalid("setup observation requires a recent all-channel fresh raw v3 sample at the current settings generation".into()));
        }
        self.setup_observation = Some(RawSetupObservation {
            session: sample.session,
            sequence: sample.sequence,
            generation: sample.generation,
            tare_epoch: sample.tare_epoch,
            flags: sample.flags,
            raw: sample.raw,
        });
        self.setup_evidence = Some(std::array::from_fn(|wire| {
            setup.channels[wire].evidence.clone().unwrap()
        }));
        self.setup_before_settings = Some(DeviceSettingsSnapshot::from_info(info)?);
        self.setup_after_settings = None;
        self.pending_setup = Some(setup);
        self.invalidate_calibration(
            "setup changed; previous calibration, held-out result, and neutral were invalidated",
        );
        Ok(())
    }

    fn save_device_settings(
        &mut self,
        store_root: &Path,
        expected_generation: u32,
        known: u8,
        negative: u8,
    ) -> Result<(), ServiceError> {
        self.require_connected()?;
        let info = self.info.clone().unwrap();
        let pending = self.pending_setup.clone().ok_or_else(|| {
            ServiceError::Invalid("explicitly confirmed setup is required".into())
        })?;
        let before_settings = DeviceSettingsSnapshot::from_info(&info)?;
        let observation = self
            .setup_observation
            .clone()
            .ok_or_else(|| ServiceError::Invalid("setup observation is unavailable".into()))?;
        if info.generation != expected_generation || known != 0x7f || negative & !known != 0 {
            return Err(ServiceError::Invalid(
                "settings generation or masks are invalid".into(),
            ));
        }
        if self.setup_before_settings.as_ref() != Some(&before_settings)
            || observation.generation != expected_generation
        {
            return Err(ServiceError::Invalid(
                "setup observation is not bound to the current pre-write settings snapshot".into(),
            ));
        }
        let expected_negative = pending.channels.iter().fold(0u8, |mask, channel| {
            if channel.polarity == Some(-1) {
                mask | (1 << channel.wire)
            } else {
                mask
            }
        });
        if expected_negative != negative {
            return Err(ServiceError::Invalid(
                "negative polarity mask does not match confirmed setup".into(),
            ));
        }
        let was_streaming = self.streaming;
        if was_streaming {
            self.stream_stop(store_root, "device settings changed during capture")?;
        }
        self.invalidate_calibration(
            "device settings are changing; calibration and session neutral invalidated",
        );
        if let Some(cached) = self.info.as_mut() {
            cached.tare_valid = false;
        }
        let request = self.next_request();
        let id = provisioned_id(&info)?;
        let command = protocol_v3::settings_command(
            request,
            id,
            expected_generation,
            pending.assembly_revision,
            known,
            negative,
        )?;
        self.transport.write_line(&command)?;
        let ack_generation = self.wait_ok(request, "settings")?;
        let readback = self.query_info()?;
        if readback.identity != info.identity
            || readback.generation != ack_generation
            || readback.assembly_revision != pending.assembly_revision
            || readback.known_mask != known
            || readback.negative_mask != negative
        {
            return Err(ServiceError::Fatal("settings readback did not match the requested identity, generation, assembly, or masks".into()));
        }
        self.info = Some(readback.clone());
        let after_settings = DeviceSettingsSnapshot::from_info(&readback)?;
        validate_settings_transition(&before_settings, &after_settings)?;
        let channel_evidence = self.setup_evidence.clone().ok_or_else(|| {
            ServiceError::Invalid("setup observation references are unavailable".into())
        })?;
        let setup = self.device_setup_from_dto(pending, ack_generation)?;
        setup.require_validated_mapping()?;
        self.persist_setup(
            store_root,
            &setup,
            before_settings.clone(),
            after_settings.clone(),
            known,
            negative,
            channel_evidence,
            observation,
        )?;
        self.setup = Some(setup);
        self.setup_before_settings = Some(before_settings);
        self.setup_after_settings = Some(after_settings);
        self.pending_setup = None;
        self.operator = None;
        self.connection_epoch = self.connection_epoch.wrapping_add(1).max(1);
        if was_streaming {
            self.stream_start()?;
        }
        Ok(())
    }

    fn device_setup_from_dto(
        &self,
        dto: SetupDto,
        generation: u32,
    ) -> Result<DeviceSetup, ServiceError> {
        let id = provisioned_id(
            self.info
                .as_ref()
                .ok_or_else(|| ServiceError::Invalid("device info missing".into()))?,
        )?;
        let channels = dto.channels.map(|channel| ChannelOrientation {
            logical_axis: channel.logical_axis,
            location: channel.location,
            positive_direction: channel.direction,
            polarity: channel.polarity,
        });
        let setup = DeviceSetup {
            device_id: id,
            assembly_revision: dto.assembly_revision,
            config_revision: dto.config_revision,
            settings_generation: generation,
            channels,
            mapping_validated: true,
            evidence: Evidence {
                origin: Origin::Recorded,
                quality: InputQuality::Usable,
                reference: "operator-confirmed-v3-settings-readback".into(),
            },
        };
        setup.require_validated_mapping()?;
        Ok(setup)
    }

    fn tare(&mut self, confirmed: bool, store_root: &Path) -> Result<(), ServiceError> {
        if !confirmed {
            return Err(ServiceError::Invalid(
                "tare requires explicit confirmation".into(),
            ));
        }
        self.require_connected()?;
        let info = self.info.clone().unwrap();
        let was_streaming = self.streaming || self.resume_after_tare;
        if was_streaming && self.streaming {
            self.stream_stop(store_root, "tare requested during capture")?;
            self.resume_after_tare = true;
        }
        self.persist_active_capture(store_root, "tare requested during capture")?;
        self.latest_sample = None;
        self.last_sequence = None;
        self.invalidate_calibration(
            "tare requested; all calibration and neutral evidence invalidated",
        );
        if let Some(cached) = self.info.as_mut() {
            cached.tare_valid = false;
        }
        let request = self.next_request();
        let command = protocol_v3::tare_command(request, provisioned_id(&info)?, info.generation)?;
        self.transport.write_line(&command)?;
        let ack_epoch = self.wait_ok(request, "tare")?;
        let readback = self.query_info()?;
        if readback.identity != info.identity
            || readback.generation != info.generation
            || readback.tare_epoch != ack_epoch
            || !readback.tare_valid
        {
            return Err(ServiceError::Fatal(
                "tare readback did not match acknowledgement and prior identity/generation".into(),
            ));
        }
        self.info = Some(readback);
        self.connection_epoch = self.connection_epoch.wrapping_add(1).max(1);
        if was_streaming {
            self.stream_start()?;
        }
        Ok(())
    }

    fn provision(
        &mut self,
        confirmed: bool,
        device_id: &str,
        store_root: &Path,
    ) -> Result<(), ServiceError> {
        if !confirmed {
            return Err(ServiceError::Invalid(
                "provisioning requires explicit confirmation".into(),
            ));
        }
        self.require_connected()?;
        let before = self.info.clone().unwrap();
        if before.identity != DeviceIdentity::Unprovisioned {
            return Err(ServiceError::Invalid(
                "already provisioned devices cannot be rewritten".into(),
            ));
        }
        let id: DeviceId = device_id
            .parse()
            .map_err(|e| ServiceError::Invalid(format!("invalid OS-generated device id: {e}")))?;
        if id.as_bytes() == &[0; 16] {
            return Err(ServiceError::Invalid(
                "all-zero device id is reserved".into(),
            ));
        }
        let was_streaming = self.streaming;
        if was_streaming {
            self.stream_stop(store_root, "provisioning during capture")?;
        }
        self.invalidate_session("provisioning changes device identity; prior state invalidated");
        self.transport.discard_input()?;
        self.transport
            .write_line(&selfrionette_core::HostCommand::Provision(id).encode_line())?;
        let started = Instant::now();
        let mut accepted = false;
        for _ in 0..READ_BUDGET {
            let remaining = INFO_TIMEOUT
                .checked_sub(started.elapsed())
                .ok_or_else(|| ServiceError::Fatal("provision result timed out".into()))?;
            let line = self.transport.read_line(remaining)?;
            match selfrionette_core::parse_line(&line) {
                Ok(selfrionette_core::ProtocolFrame::Status(status))
                    if status.token == "provision_ok" =>
                {
                    accepted = true;
                    break;
                }
                Ok(selfrionette_core::ProtocolFrame::Warning(warning))
                    if matches!(
                        warning.token.as_str(),
                        "already_provisioned" | "invalid_device_id" | "provision_verify_failed"
                    ) =>
                {
                    return Err(ServiceError::Invalid(format!(
                        "provisioning rejected: {}",
                        warning.token
                    )));
                }
                Ok(_) => {}
                Err(error) => {
                    return Err(ServiceError::Fatal(format!(
                        "legacy provision response parse failed: {error}"
                    )));
                }
            }
        }
        if !accepted {
            return Err(ServiceError::Fatal(
                "provision result exceeded bounded line budget".into(),
            ));
        }
        let readback = self.query_info()?;
        if readback.identity != DeviceIdentity::Provisioned(id) {
            return Err(ServiceError::Fatal(
                "provisioned identity readback mismatch".into(),
            ));
        }
        self.info = Some(readback);
        self.connection_epoch = self.connection_epoch.wrapping_add(1).max(1);
        if was_streaming {
            self.stream_start()?;
        }
        Ok(())
    }

    fn apply_calibration(
        &mut self,
        confirmed: bool,
        method: InputMethod,
    ) -> Result<(), ServiceError> {
        if !confirmed {
            return Err(ServiceError::Invalid(
                "profile application requires explicit confirmation".into(),
            ));
        }
        self.require_connected()?;
        if !self.heldout_view.passed || self.heldout_method != Some(method) {
            return Err(ServiceError::Invalid("a fresh held-out verification using the exact selected method is required before apply".into()));
        }
        let fit = self.fit.as_ref().ok_or_else(|| {
            ServiceError::Invalid("load or fit a validated calibration profile first".into())
        })?;
        let binding = self.loaded_binding.as_ref().ok_or_else(|| {
            ServiceError::Invalid("compatible profile binding is required".into())
        })?;
        let current = self.current_context()?;
        binding.check_compatibility(&current)?;
        let neutral = self.session_neutral.ok_or_else(|| {
            ServiceError::Invalid(
                "capture a fresh session-neutral window after verification".into(),
            )
        })?;
        let settings = self
            .signal_settings
            .ok_or_else(|| ServiceError::Invalid("signal settings are unavailable".into()))?;
        let context = Context {
            source: self.context_source(self.info.as_ref().unwrap()),
            user: current.operator.operator.clone(),
            posture: current.operator.posture.clone(),
        };
        self.processor = Some(InputProcessor::new_for_session(
            fit,
            context.clone(),
            &neutral,
            method,
            settings,
        )?);
        self.applied_context = Some(context);
        self.latest_output = None;
        Ok(())
    }

    fn save_calibration(
        &mut self,
        store_root: &Path,
        revision: u32,
        method: MethodDto,
        confirmed: bool,
    ) -> Result<(), ServiceError> {
        if !confirmed {
            return Err(ServiceError::Invalid(
                "saving a profile requires explicit confirmation".into(),
            ));
        }
        if revision == 0 {
            return Err(ServiceError::Invalid(
                "profile_revision must be nonzero".into(),
            ));
        }
        if !self.heldout_view.passed || self.heldout_method != Some(method.into()) {
            return Err(ServiceError::Invalid("independent held-out verification must pass using the same method selected for this profile".into()));
        }
        let current = self.current_context()?;
        let recording = self.build_recording()?;
        let limits = self
            .fit_limits
            .clone()
            .ok_or_else(|| ServiceError::Invalid("fit limits are unavailable".into()))?;
        let settings = self
            .signal_settings
            .ok_or_else(|| ServiceError::Invalid("signal settings are unavailable".into()))?;
        let summary = self
            .fit_summary
            .clone()
            .ok_or_else(|| ServiceError::Invalid("fit summary is unavailable".into()))?;
        let id = provisioned_id(self.info.as_ref().unwrap())?;
        let directory = device_directory(store_root, id)?;
        fs::create_dir_all(&directory).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&directory)?;
        let file_name = unique_profile_name(&directory, revision)?;
        let binding = ProfileBinding {
            profile_revision: revision,
            parameter_record: file_name.clone(),
            setup: current.setup.clone(),
            firmware: current.firmware.clone(),
            operator: current.operator.clone(),
            evidence: Evidence {
                origin: Origin::Recorded,
                quality: InputQuality::Usable,
                reference: file_name.clone(),
            },
        };
        binding.validate()?;
        binding.check_compatibility(&current)?;
        let record = ProfileRecord {
            schema: PROFILE_SCHEMA,
            binding: binding.clone(),
            recording,
            limits,
            signal_settings: settings,
            fit_summary: summary,
            heldout: self.stored_heldout()?,
            heldout_passed: true,
            heldout_result: self
                .heldout_view
                .reason
                .clone()
                .unwrap_or_else(|| "passed".into()),
            setup_proof: StoredSetup {
                schema: SETUP_SCHEMA,
                setup: current.setup.clone(),
                before_settings: self.setup_before_settings.clone().ok_or_else(|| {
                    ServiceError::Invalid("pre-write setup settings snapshot is unavailable".into())
                })?,
                after_settings: self.setup_after_settings.clone().ok_or_else(|| {
                    ServiceError::Invalid(
                        "post-write setup settings snapshot is unavailable".into(),
                    )
                })?,
                known_mask: self.info.as_ref().unwrap().known_mask,
                negative_mask: self.info.as_ref().unwrap().negative_mask,
                channel_evidence: self
                    .setup_evidence
                    .clone()
                    .ok_or_else(|| ServiceError::Invalid("setup evidence is unavailable".into()))?,
                observation: self.setup_observation.clone().ok_or_else(|| {
                    ServiceError::Invalid("setup raw observation is unavailable".into())
                })?,
            },
            heldout_method: method,
            verification_policy_version: VERIFICATION_POLICY_VERSION.into(),
        };
        let bytes =
            serde_json::to_vec(&record).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        if bytes.len() as u64 > MAX_PROFILE_BYTES {
            return Err(ServiceError::Persistence(
                "profile exceeds bounded storage size".into(),
            ));
        }
        let destination = directory.join(&file_name);
        atomic_create_immutable(&directory, &destination, &bytes)?;
        let loaded = read_profile(&destination)?;
        self.validate_profile(&loaded, &current, &file_name)?;
        self.loaded_binding = Some(binding);
        self.profile_message = Some(format!(
            "saved and reloaded verified profile ({:?})",
            method
        ));
        self.saved_path = Some(file_name);
        Ok(())
    }

    fn list_calibrations(&mut self, store_root: &Path) -> Result<(), ServiceError> {
        self.require_connected()?;
        let id = provisioned_id(self.info.as_ref().unwrap())?;
        let directory = device_directory(store_root, id)?;
        self.profiles.clear();
        if !directory.exists() {
            return Ok(());
        }
        reject_symlink(&directory)?;
        let current = self.current_context().ok();
        let entries =
            fs::read_dir(&directory).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        for (count, item) in entries.take(MAX_PROFILE_COUNT).enumerate() {
            let item = item.map_err(|e| ServiceError::Persistence(e.to_string()))?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".srncal") {
                continue;
            }
            let path = item.path();
            let result = read_profile(&path).and_then(|record| {
                self.validate_profile(&record, &record.binding_current_context(), &name)
                    .map(|_| record)
            });
            match result {
                Ok(record) => {
                    let compatible = current
                        .as_ref()
                        .is_some_and(|ctx| record.binding.check_compatibility(ctx).is_ok());
                    let reason = if compatible {
                        None
                    } else {
                        Some("setup, firmware, or operator context differs".into())
                    };
                    self.profiles.push(ProfileView {
                        path: name.clone(),
                        label: format!("r{} · {}", record.binding.profile_revision, name),
                        compatible,
                        reason,
                    });
                }
                Err(error) => self.profiles.push(ProfileView {
                    path: name.clone(),
                    label: format!("invalid · {name}"),
                    compatible: false,
                    reason: Some(error.to_string()),
                }),
            }
            if count + 1 >= MAX_PROFILE_COUNT {
                break;
            }
        }
        Ok(())
    }

    fn load_calibration(&mut self, store_root: &Path, name: &str) -> Result<(), ServiceError> {
        self.require_connected()?;
        if !valid_profile_name(name) {
            return Err(ServiceError::Invalid(
                "record_path must be a profile basename returned by list_calibrations".into(),
            ));
        }
        let id = provisioned_id(self.info.as_ref().unwrap())?;
        let directory = device_directory(store_root, id)?;
        reject_symlink(&directory)?;
        let path = directory.join(name);
        reject_symlink(&path)?;
        let record = read_profile(&path)?;
        let current = self.current_context()?;
        let fit = self.validate_profile(&record, &current, name)?;
        let heldout = record.heldout.clone();
        let heldout_method: InputMethod = record.heldout_method.into();
        self.invalidate_calibration("profile loaded; stored held-out captures are being revalidated for this exact setup and method");
        self.fit = Some(fit);
        self.fit_limits = Some(record.limits.clone());
        self.signal_settings = Some(record.signal_settings);
        self.fit_summary = Some(record.fit_summary.clone());
        self.fit_view = Some(fit_view(self.fit.as_ref().unwrap()));
        self.loaded_binding = Some(record.binding);
        self.load_stored_heldout(heldout)?;
        self.verify_heldout(heldout_method)?;
        self.profile_message = Some(format!(
            "loaded compatible profile {name}; stored holdout was recalculated and a fresh session neutral is still required"
        ));
        Ok(())
    }

    fn load_stored_heldout(&mut self, records: Vec<StoredHeldOut>) -> Result<(), ServiceError> {
        let epoch = self.epoch_token(
            self.info
                .as_ref()
                .ok_or_else(|| ServiceError::Invalid("device info is unavailable".into()))?
                .tare_epoch,
        );
        self.heldout.clear();
        for record in records {
            if record.axis >= 7
                || record.key != record.instruction
                || record.instruction.trim().is_empty()
                || record
                    .intent
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 1.0)
            {
                return Err(ServiceError::Persistence(
                    "stored held-out capture metadata is invalid".into(),
                ));
            }
            let rebind = |samples: Vec<FirmwareVectorSample>| {
                samples
                    .into_iter()
                    .map(|mut sample| {
                        sample.epoch = epoch;
                        sample
                    })
                    .collect::<Vec<_>>()
            };
            let key = HeldOutKey {
                axis: record.axis,
                instruction: record.instruction.clone(),
            };
            if self
                .heldout
                .insert(
                    key,
                    HeldOutParts {
                        neutral: Some(rebind(record.neutral)),
                        ramp: Some(rebind(record.ramp)),
                        hold: Some(rebind(record.hold)),
                        release: Some(rebind(record.release)),
                        intent: Some(record.intent),
                    },
                )
                .is_some()
            {
                return Err(ServiceError::Persistence(
                    "profile contains duplicate held-out trial keys".into(),
                ));
            }
        }
        Ok(())
    }

    fn validate_profile(
        &self,
        record: &ProfileRecord,
        current: &CurrentContext,
        name: &str,
    ) -> Result<FittedInput, ServiceError> {
        let setup_proof_valid = record.setup_proof.setup == record.binding.setup
            && valid_setup_proof(&record.setup_proof);
        if record.schema != PROFILE_SCHEMA
            || record.binding.parameter_record != name
            || !valid_profile_name(name)
            || !record.heldout_passed
            || record.heldout.is_empty()
            || record.verification_policy_version != VERIFICATION_POLICY_VERSION
            || !setup_proof_valid
        {
            return Err(ServiceError::Persistence(
                "profile schema, binding, or held-out evidence is invalid".into(),
            ));
        }
        record.binding.check_compatibility(current)?;
        let fit = record
            .binding
            .fit_device_oriented(current, &record.recording, &record.limits)?;
        if summary_record(fit.summary()) != record.fit_summary {
            return Err(ServiceError::Persistence(
                "stored fitted summary does not match recalculated capture evidence".into(),
            ));
        }
        Ok(fit)
    }

    fn stored_heldout(&self) -> Result<Vec<StoredHeldOut>, ServiceError> {
        let mut result = Vec::new();
        for (key, parts) in &self.heldout {
            let (Some(neutral), Some(ramp), Some(hold), Some(release), Some(intent)) = (
                &parts.neutral,
                &parts.ramp,
                &parts.hold,
                &parts.release,
                parts.intent,
            ) else {
                return Err(ServiceError::Invalid(
                    "every held-out trial must have neutral/ramp/hold/release samples".into(),
                ));
            };
            result.push(StoredHeldOut {
                key: key.instruction.clone(),
                axis: key.axis,
                instruction: key.instruction.clone(),
                intent,
                neutral: neutral.clone(),
                ramp: ramp.clone(),
                hold: hold.clone(),
                release: release.clone(),
            });
        }
        if result.is_empty() {
            return Err(ServiceError::Invalid("held-out evidence is missing".into()));
        }
        Ok(result)
    }

    fn current_context(&self) -> Result<CurrentContext, ServiceError> {
        let setup = self.setup.clone().ok_or_else(|| {
            ServiceError::Invalid("validated persistent device setup is required".into())
        })?;
        let info = self
            .info
            .as_ref()
            .ok_or_else(|| ServiceError::Invalid("device info is unavailable".into()))?;
        let operator = self.operator.clone().ok_or_else(|| {
            ServiceError::Invalid("operator/posture/grip context is required".into())
        })?;
        let firmware = FirmwareBinding {
            version: info.firmware_version.clone(),
            source_revision: info.source_revision.clone(),
            sample_domain: SampleDomain::DeviceOrientedCounts,
        };
        let current = CurrentContext {
            setup,
            firmware,
            operator,
        };
        current.validate()?;
        Ok(current)
    }

    fn make_binding(&self, name: &str, revision: u32) -> Result<ProfileBinding, ServiceError> {
        let current = self.current_context()?;
        let evidence = Evidence {
            origin: Origin::Recorded,
            quality: InputQuality::Usable,
            reference: "service-owned-v3-capture".into(),
        };
        let binding = ProfileBinding {
            profile_revision: revision,
            parameter_record: name.into(),
            setup: current.setup,
            firmware: current.firmware,
            operator: current.operator,
            evidence,
        };
        binding.validate()?;
        Ok(binding)
    }

    fn context_source(&self, info: &DeviceInfo) -> selfrionette_core::calibration::Source {
        let id = match info.identity {
            DeviceIdentity::Provisioned(id) => id,
            DeviceIdentity::Unprovisioned => {
                DeviceId::from_bytes([1; 16]).expect("nonzero fixed fallback")
            }
        };
        selfrionette_core::calibration::Source {
            id,
            firmware: info.firmware_version.clone(),
            assembly: info.assembly_revision.to_string(),
            epoch: self.epoch_token(info.tare_epoch),
        }
    }

    fn epoch_token(&self, tare_epoch: u32) -> u64 {
        self.connection_epoch.wrapping_shl(32) | u64::from(tare_epoch)
    }

    fn next_request(&mut self) -> u32 {
        self.request_id = self.request_id.wrapping_add(1).max(1);
        self.request_id
    }

    fn require_connected(&self) -> Result<(), ServiceError> {
        if self.info.is_none() || self.fatal {
            return Err(ServiceError::Invalid(
                "connect to a valid v3 device first".into(),
            ));
        }
        Ok(())
    }
    fn require_stream_and_setup(&self) -> Result<(), ServiceError> {
        self.require_connected()?;
        if !self.streaming {
            return Err(ServiceError::Invalid(
                "start the v3 quality stream first".into(),
            ));
        }
        if self.setup.is_none() {
            return Err(ServiceError::Invalid(
                "save and read back a complete device setup first".into(),
            ));
        }
        if self
            .info
            .as_ref()
            .is_some_and(|i| i.identity == DeviceIdentity::Unprovisioned || !i.tare_valid)
        {
            return Err(ServiceError::Invalid(
                "provisioned identity and valid firmware tare are required".into(),
            ));
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_setup(
        &self,
        root: &Path,
        setup: &DeviceSetup,
        before_settings: DeviceSettingsSnapshot,
        after_settings: DeviceSettingsSnapshot,
        known: u8,
        negative: u8,
        channel_evidence: [String; 7],
        observation: RawSetupObservation,
    ) -> Result<(), ServiceError> {
        let dir = device_directory(root, setup.device_id)?;
        fs::create_dir_all(&dir).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        reject_symlink(&dir)?;
        let record = StoredSetup {
            schema: SETUP_SCHEMA,
            setup: setup.clone(),
            before_settings,
            after_settings,
            known_mask: known,
            negative_mask: negative,
            channel_evidence,
            observation,
        };
        if !valid_setup_proof(&record) {
            return Err(ServiceError::Persistence(
                "setup evidence does not bind a valid before/after settings transaction".into(),
            ));
        }
        validate_setup_revision(&read_valid_setup_records(&dir)?, setup)?;
        let bytes =
            serde_json::to_vec(&record).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        let name = format!(
            "setup-a{}-c{}-g{}.srnsetup",
            setup.assembly_revision, setup.config_revision, setup.settings_generation
        );
        let path = dir.join(name);
        atomic_create_immutable(&dir, &path, &bytes)
    }

    fn load_setup_for_current(&mut self, root: &Path) -> Result<(), ServiceError> {
        let Some(info) = self.info.as_ref() else {
            return Ok(());
        };
        let DeviceIdentity::Provisioned(id) = info.identity else {
            return Ok(());
        };
        let dir = device_directory(root, id)?;
        if !dir.exists() {
            return Ok(());
        }
        let mut selected: Option<StoredSetup> = None;
        for record in read_valid_setup_records(&dir)? {
            if !setup_matches_info(&record, id, info) {
                continue;
            }
            match selected.as_ref() {
                None => selected = Some(record),
                Some(current) if record.setup.config_revision > current.setup.config_revision => {
                    selected = Some(record)
                }
                Some(current) if record.setup.config_revision == current.setup.config_revision => {
                    if current != &record {
                        return Err(ServiceError::Persistence(
                            "multiple distinct host setup snapshots share the highest config_revision"
                                .into(),
                        ));
                    }
                }
                Some(_) => {}
            }
        }
        if let Some(record) = selected {
            self.setup = Some(record.setup);
            self.setup_evidence = Some(record.channel_evidence);
            self.setup_observation = Some(record.observation);
            self.setup_before_settings = Some(record.before_settings);
            self.setup_after_settings = Some(record.after_settings);
            self.push_message(
                "loaded highest validated host config_revision after firmware readback",
            );
        }
        Ok(())
    }
}

fn command_matches_intent(
    command: &[Option<f64>; 7],
    intent: &[f64; 7],
    direction_tolerance: f64,
    require_active: bool,
) -> bool {
    if !direction_tolerance.is_finite() || !(0.0..=1.0).contains(&direction_tolerance) {
        return false;
    }
    let intent_scale = intent
        .iter()
        .map(|value| value.abs())
        .fold(0.0_f64, f64::max);
    if intent_scale == 0.0 {
        return false;
    }
    let allowed_leakage = direction_tolerance * intent_scale;
    (0..7).all(|axis| {
        let value = command[axis].unwrap_or(0.0);
        if !value.is_finite() {
            return false;
        }
        if intent[axis] == 0.0 {
            value.abs() <= allowed_leakage
        } else if value == 0.0 {
            !require_active
        } else if value.signum() == intent[axis].signum() {
            true
        } else if require_active {
            false
        } else {
            value.abs() <= allowed_leakage
        }
    })
}

fn capture_archive_usage(directory: &Path) -> Result<(usize, u64), ServiceError> {
    match fs::symlink_metadata(directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(error) => return Err(ServiceError::Persistence(error.to_string())),
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(ServiceError::Persistence(
                "capture archive path is not a regular directory".into(),
            ));
        }
        Ok(_) => {}
    }
    let mut count = 0usize;
    let mut bytes = 0u64;
    for (entries_seen, entry) in fs::read_dir(directory)
        .map_err(|error| ServiceError::Persistence(error.to_string()))?
        .enumerate()
    {
        if entries_seen >= MAX_CAPTURE_ARCHIVE_COUNT {
            return Err(ServiceError::Persistence(
                "capture archive directory exceeds its 256-entry scan bound".into(),
            ));
        }
        let entry = entry.map_err(|error| ServiceError::Persistence(error.to_string()))?;
        if !entry.file_name().to_string_lossy().ends_with(".srncap") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| ServiceError::Persistence(error.to_string()))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ServiceError::Persistence(
                "capture archive contains a symlink or non-file entry".into(),
            ));
        }
        count += 1;
        bytes = bytes.checked_add(metadata.len()).ok_or_else(|| {
            ServiceError::Persistence("capture archive byte count overflow".into())
        })?;
    }
    Ok((count, bytes))
}

fn stable_neutral(
    samples: &[FirmwareVectorSample],
    limits: &FitLimits,
) -> Result<[f64; 7], ServiceError> {
    if samples.len() < 8 || samples.len() > MAX_CAPTURE_SAMPLES {
        return Err(ServiceError::Invalid(
            "neutral evidence must contain 8..=1024 samples".into(),
        ));
    }
    if samples
        .iter()
        .any(|sample| sample.values.iter().any(|value| !value.is_finite()))
    {
        return Err(ServiceError::Invalid(
            "neutral evidence contains non-finite counts".into(),
        ));
    }
    let midpoint = samples.len() / 2;
    let mut neutral = [0.0; 7];
    for (axis, output) in neutral.iter_mut().enumerate() {
        let values = samples
            .iter()
            .map(|sample| sample.values[axis])
            .collect::<Vec<_>>();
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !limits.max_rest_range_counts[axis].is_finite()
            || max - min > limits.max_rest_range_counts[axis]
        {
            return Err(ServiceError::Invalid(format!(
                "neutral range on axis {axis} exceeded configured max_rest_range_counts"
            )));
        }
        let first = median(&values[..midpoint]);
        let second = median(&values[midpoint..]);
        if !limits.max_drift_counts[axis].is_finite()
            || (second - first).abs() > limits.max_drift_counts[axis]
        {
            return Err(ServiceError::Invalid(format!(
                "neutral drift on axis {axis} exceeded configured max_drift_counts"
            )));
        }
        *output = median(&values);
    }
    Ok(neutral)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    }
}

fn info_context_changed(old: &DeviceInfo, new: &DeviceInfo) -> bool {
    old.identity != new.identity
        || old.firmware_version != new.firmware_version
        || old.source_revision != new.source_revision
        || old.assembly_revision != new.assembly_revision
        || old.generation != new.generation
        || old.known_mask != new.known_mask
        || old.negative_mask != new.negative_mask
        || old.tare_epoch != new.tare_epoch
        || old.tare_valid != new.tare_valid
}

fn info_view(info: &DeviceInfo) -> LiveInfo {
    LiveInfo {
        firmware_version: info.firmware_version.clone(),
        source_sha: info.source_revision.clone(),
        board: info.board.clone(),
        mcu: info.mcu.clone(),
        clock_hz: info.clock_hz,
        device_id: match info.identity {
            DeviceIdentity::Provisioned(id) => id.to_string(),
            DeviceIdentity::Unprovisioned => "unprovisioned".into(),
        },
        generation: info.generation,
        assembly_revision: info.assembly_revision,
        known_mask: info.known_mask,
        negative_mask: info.negative_mask,
        tare_epoch: info.tare_epoch,
        tare_valid: info.tare_valid,
    }
}

fn sample_view(sample: &Sample) -> LiveSample {
    LiveSample {
        session: sample.session,
        sequence: sample.sequence,
        timestamp_ms: sample.device_ms,
        tare_epoch: sample.tare_epoch,
        generation: sample.generation,
        raw: sample.raw,
        oriented: sample.oriented,
        flags: sample.flags,
    }
}

fn setup_view(setup: &DeviceSetup, evidence: Option<&[String; 7]>) -> SetupDto {
    SetupDto {
        assembly_revision: setup.assembly_revision,
        config_revision: setup.config_revision,
        channels: std::array::from_fn(|wire| {
            let c = &setup.channels[wire];
            SetupChannelDto {
                wire,
                logical_axis: c.logical_axis,
                location: c.location.clone(),
                direction: c.positive_direction.clone(),
                polarity: c.polarity,
                confirmed: setup.mapping_validated,
                evidence: evidence.map(|rows| rows[wire].clone()),
            }
        }),
    }
}

fn fit_view(fit: &FittedInput) -> FitView {
    let summary = fit.summary();
    FitView {
        diagonal_available: true,
        diagonal_reason: None,
        coupled_available: summary.condition_number.is_some(),
        coupled_rejection: summary.coupled_rejection.as_ref().map(ToString::to_string),
        positive_span_counts: summary.positive_span_counts,
        negative_span_counts: summary.negative_span_counts,
        noise_envelope_counts: summary.noise_envelope_counts,
        cross_axis_ratio: summary.cross_axis_ratio,
        condition_number: summary.condition_number,
        inverse_noise_gain: summary.inverse_noise_gain,
    }
}

fn summary_record(summary: &input_calibration::CalibrationSummary) -> FitSummaryRecord {
    FitSummaryRecord {
        baseline_counts: summary.baseline_counts,
        noise_envelope_counts: summary.noise_envelope_counts,
        polarity: summary.polarity,
        positive_span_counts: summary.positive_span_counts,
        negative_span_counts: summary.negative_span_counts,
        positive_repeats: summary.positive_repeats,
        negative_repeats: summary.negative_repeats,
        response_matrix: summary.response_matrix,
        cross_axis_ratio: summary.cross_axis_ratio,
        condition_number: summary.condition_number,
        inverse_noise_gain: summary.inverse_noise_gain,
        coupled_rejection: summary.coupled_rejection.as_ref().map(ToString::to_string),
    }
}

fn provisioned_id(info: &DeviceInfo) -> Result<DeviceId, ServiceError> {
    match info.identity {
        DeviceIdentity::Provisioned(id) => Ok(id),
        DeviceIdentity::Unprovisioned => Err(ServiceError::Invalid(
            "device must be provisioned first".into(),
        )),
    }
}

fn device_directory(root: &Path, id: DeviceId) -> Result<PathBuf, ServiceError> {
    if !root.is_absolute() {
        return Err(ServiceError::Persistence(
            "application storage root must be absolute".into(),
        ));
    }
    reject_symlink(root)?;
    Ok(root.join(id.to_string()))
}

fn reject_symlink(path: &Path) -> Result<(), ServiceError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(ServiceError::Persistence(format!(
            "refusing symlink/reparse path {}",
            path.display()
        ))),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ServiceError::Persistence(e.to_string())),
    }
}

fn valid_profile_name(name: &str) -> bool {
    name.ends_with(".srncal")
        && name.len() <= 96
        && !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

fn unique_profile_name(directory: &Path, revision: u32) -> Result<String, ServiceError> {
    for sequence in 1..=MAX_PROFILE_COUNT as u32 {
        let name = format!("profile-{revision:010}-{sequence:03}.srncal");
        if !directory.join(&name).exists() {
            return Ok(name);
        }
    }
    Err(ServiceError::Persistence(
        "profile directory reached the bounded immutable record limit".into(),
    ))
}

fn atomic_create_immutable(
    directory: &Path,
    destination: &Path,
    bytes: &[u8],
) -> Result<(), ServiceError> {
    reject_symlink(directory)?;
    reject_symlink(destination)?;
    if destination.exists() {
        return Err(ServiceError::Persistence(format!(
            "immutable record already exists: {}",
            destination
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        )));
    }
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp = directory.join(format!("pending-{}-{sequence}.tmp", std::process::id()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| ServiceError::Persistence(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| ServiceError::Persistence(e.to_string()))?;
        file.sync_all()
            .map_err(|e| ServiceError::Persistence(e.to_string()))?;
        drop(file);
        let readback = fs::read(&temp).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        if readback != bytes {
            return Err(ServiceError::Persistence(
                "temporary write readback mismatch".into(),
            ));
        }
        fs::hard_link(&temp, destination).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        fs::remove_file(&temp).map_err(|e| ServiceError::Persistence(e.to_string()))?;
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn read_profile(path: &Path) -> Result<ProfileRecord, ServiceError> {
    reject_symlink(path)?;
    let meta = fs::metadata(path).map_err(|e| ServiceError::Persistence(e.to_string()))?;
    if !meta.is_file() || meta.len() > MAX_PROFILE_BYTES {
        return Err(ServiceError::Persistence(
            "profile is not a bounded regular file".into(),
        ));
    }
    let file = File::open(path).map_err(|e| ServiceError::Persistence(e.to_string()))?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_PROFILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ServiceError::Persistence(e.to_string()))?;
    if bytes.len() as u64 > MAX_PROFILE_BYTES {
        return Err(ServiceError::Persistence(
            "profile exceeds size limit".into(),
        ));
    }
    let record: ProfileRecord = serde_json::from_slice(&bytes)
        .map_err(|e| ServiceError::Persistence(format!("profile JSON is invalid: {e}")))?;
    let canonical =
        serde_json::to_vec(&record).map_err(|e| ServiceError::Persistence(e.to_string()))?;
    if canonical != bytes {
        return Err(ServiceError::Persistence(
            "profile JSON is not canonical".into(),
        ));
    }
    Ok(record)
}

impl ProfileRecord {
    fn binding_current_context(&self) -> CurrentContext {
        CurrentContext {
            setup: self.binding.setup.clone(),
            firmware: self.binding.firmware.clone(),
            operator: self.binding.operator.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
    };

    fn task_test_root() -> PathBuf {
        std::env::var_os("CODEX_TASK_TEMP_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    }

    type SharedReads = Arc<Mutex<VecDeque<String>>>;

    struct QueueTransport {
        reads: SharedReads,
        writes: Arc<Mutex<Vec<String>>>,
    }
    impl QueueTransport {
        fn new() -> (Self, SharedReads) {
            let reads = Arc::new(Mutex::new(VecDeque::new()));
            (
                Self {
                    reads: reads.clone(),
                    writes: Arc::new(Mutex::new(Vec::new())),
                },
                reads,
            )
        }
    }
    impl LineTransport for QueueTransport {
        fn discard_input(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
        fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
            self.writes.lock().unwrap().push(line.into());
            Ok(())
        }
        fn read_line(&mut self, _timeout: Duration) -> Result<String, TransportError> {
            self.reads
                .lock()
                .unwrap()
                .pop_front()
                .ok_or(TransportError::Timeout)
        }
    }

    fn queue(reads: &SharedReads, line: impl Into<String>) {
        reads.lock().unwrap().push_back(line.into());
    }

    #[test]
    fn safe_gui_probe_of_legacy_or_no_reply_never_falls_back_to_i() {
        for line in [
            None,
            Some("vector,100,1,2,3,4,5,6,7"),
            Some("warn,unknown_command"),
        ] {
            let (transport, reads) = QueueTransport::new();
            let writes = transport.writes.clone();
            if let Some(line) = line {
                queue(&reads, line);
            }
            let mut service = LiveService::with_safe_metadata_probe(Box::new(transport));
            assert!(service.query_info().is_err());
            let writes = writes.lock().unwrap();
            assert_eq!(*writes, ["?3,1\n"]);
            assert!(!writes.iter().any(|line| line.contains('i')));
        }
    }

    fn queue_capture_barrier(service: &LiveService, reads: &SharedReads) {
        let stop = service.request_id.wrapping_add(1).max(1);
        let resume = stop.wrapping_add(1).max(1);
        queue(reads, format!("ok3,{stop},stream,{}", service.session_id));
        queue(reads, format!("ok3,{resume},stream,{}", service.session_id));
    }

    fn info_line(
        request: u32,
        generation: u32,
        assembly: u32,
        known: u8,
        negative: u8,
        tare_epoch: u32,
    ) -> String {
        format!(
            "info3,{request},1.2.0,0123456789abcdef0123456789abcdef01234567,sparkfun_promicro16,atmega32u4,16000000,srn-0123456789abcdef0123456789abcdef,{generation},{assembly},{known},{negative},{tare_epoch},1"
        )
    }

    fn queue_live_window(
        reads: &SharedReads,
        sequence: &mut u32,
        device_ms: &mut u32,
        values: [i32; 7],
        epoch: u32,
        generation: u32,
    ) {
        for _ in 0..8 {
            *sequence += 1;
            *device_ms += 10;
            let mut fields = vec![
                "sample3".to_owned(),
                "1".into(),
                sequence.to_string(),
                device_ms.to_string(),
                epoch.to_string(),
                generation.to_string(),
                "1".into(),
            ];
            fields.extend([1000; 7].into_iter().map(|value| value.to_string()));
            fields.extend(values.into_iter().map(|value| value.to_string()));
            fields.extend([1; 7].into_iter().map(|value| value.to_string()));
            queue(reads, fields.join(","));
        }
    }

    // Fixture exposes every independently varied capture condition explicitly.
    #[allow(clippy::too_many_arguments)]
    fn capture_window(
        service: &mut LiveService,
        reads: &SharedReads,
        store: &Path,
        sequence: &mut u32,
        device_ms: &mut u32,
        phase: CapturePhase,
        axis: Option<usize>,
        direction: i8,
        repeat: u8,
        level: f64,
        instruction: &str,
        intent: Option<[f64; 7]>,
        values: [i32; 7],
    ) -> CaptureView {
        let prior_info = service.info.clone();
        let prior_setup = format!("{:?}", service.setup);
        let prior_epoch = service.connection_epoch;
        let prior_fit = service
            .fit_summary
            .as_ref()
            .map(|summary| serde_json::to_vec(summary).unwrap());
        let prior_trials = format!("{:?}", service.trials);
        queue_capture_barrier(service, reads);
        service
            .execute(
                Command::CaptureStart {
                    phase,
                    axis,
                    direction,
                    repeat,
                    level,
                    instruction: instruction.into(),
                    intent,
                    sample_count: Some(8),
                },
                store,
            )
            .unwrap();
        assert_eq!(service.info, prior_info);
        assert_eq!(format!("{:?}", service.setup), prior_setup);
        assert_eq!(service.connection_epoch, prior_epoch);
        if matches!(
            phase,
            CapturePhase::HeldOutNeutral
                | CapturePhase::HeldOutRamp
                | CapturePhase::HeldOutHold
                | CapturePhase::HeldOutRelease
                | CapturePhase::SessionNeutral
        ) {
            assert_eq!(
                service
                    .fit_summary
                    .as_ref()
                    .map(|summary| serde_json::to_vec(summary).unwrap()),
                prior_fit
            );
            assert_eq!(format!("{:?}", service.trials), prior_trials);
        }
        queue_live_window(reads, sequence, device_ms, values, 2, 2);
        let reply = service.execute(Command::CaptureTick, store).unwrap();
        let capture = reply
            .view
            .capture
            .expect("capture progress is included in the service reply");
        assert!(
            capture.complete && capture.completed,
            "{phase:?} failed: {:?}",
            capture.reason
        );
        assert_eq!(capture.done, 8);
        capture
    }

    fn test_info() -> DeviceInfo {
        DeviceInfo {
            firmware_version: "1.2.0".into(),
            source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            board: "sparkfun_promicro16".into(),
            mcu: "atmega32u4".into(),
            clock_hz: 16_000_000,
            identity: DeviceIdentity::Provisioned(
                "srn-0123456789abcdef0123456789abcdef".parse().unwrap(),
            ),
            generation: 1,
            assembly_revision: 1,
            known_mask: 0x7f,
            negative_mask: 0,
            tare_epoch: 1,
            tare_valid: true,
        }
    }

    fn test_setup(device_id: DeviceId) -> DeviceSetup {
        DeviceSetup {
            device_id,
            assembly_revision: 1,
            config_revision: 1,
            settings_generation: 1,
            channels: std::array::from_fn(|wire| ChannelOrientation {
                logical_axis: Some((wire + 1) % 7),
                location: Some(format!("location-{wire}")),
                positive_direction: Some(format!("direction-{wire}")),
                polarity: Some(if wire == 0 { -1 } else { 1 }),
            }),
            mapping_validated: true,
            evidence: Evidence {
                origin: Origin::Recorded,
                quality: InputQuality::Usable,
                reference: "test-setup".into(),
            },
        }
    }

    fn test_stored_setup(config_revision: u32, changed_location: bool) -> StoredSetup {
        let id: DeviceId = "srn-0123456789abcdef0123456789abcdef".parse().unwrap();
        let mut setup = test_setup(id);
        setup.config_revision = config_revision;
        if changed_location {
            setup.channels[0].location = Some("different-location".into());
        }
        let settings = DeviceSettingsSnapshot {
            device_id: setup.device_id,
            generation: 1,
            assembly_revision: 1,
            known_mask: 0x7f,
            negative_mask: 1,
        };
        StoredSetup {
            schema: SETUP_SCHEMA,
            setup,
            before_settings: settings.clone(),
            after_settings: settings,
            known_mask: 0x7f,
            negative_mask: 1,
            channel_evidence: std::array::from_fn(|wire| format!("observed-wire-{wire}")),
            observation: RawSetupObservation {
                session: 1,
                sequence: 1,
                generation: 1,
                tare_epoch: 1,
                flags: [FLAG_FRESH; 7],
                raw: [Some(100); 7],
            },
        }
    }

    fn confirmed_setup_dto() -> SetupDto {
        SetupDto {
            assembly_revision: 2,
            config_revision: 1,
            channels: std::array::from_fn(|wire| SetupChannelDto {
                wire,
                logical_axis: Some(wire),
                location: Some(format!("location-{wire}")),
                direction: Some(format!("direction-{wire}")),
                polarity: Some(if wire == 0 { -1 } else { 1 }),
                confirmed: true,
                evidence: Some(format!("raw-observation-wire-{wire}")),
            }),
        }
    }

    fn sample_line(sequence: u32, oriented_start: i32) -> String {
        let mut fields = vec![
            "sample3".to_owned(),
            "1".into(),
            sequence.to_string(),
            (sequence * 10).to_string(),
            "1".into(),
            "1".into(),
            "1".into(),
        ];
        fields.extend((0..7).map(|wire| (100 + { wire }).to_string()));
        fields.extend((0..7).map(|wire| (oriented_start + { wire }).to_string()));
        fields.extend((0..7).map(|_| "1".to_owned()));
        fields.join(",")
    }

    struct FakeTransport {
        reads: VecDeque<Result<String, TransportError>>,
        writes: Vec<String>,
    }
    impl FakeTransport {
        fn new(lines: &[&str]) -> Self {
            Self {
                reads: lines.iter().map(|s| Ok((*s).into())).collect(),
                writes: Vec::new(),
            }
        }
    }
    impl LineTransport for FakeTransport {
        fn discard_input(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
        fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
            self.writes.push(line.into());
            Ok(())
        }
        fn read_line(&mut self, _timeout: Duration) -> Result<String, TransportError> {
            self.reads
                .pop_front()
                .unwrap_or(Err(TransportError::Timeout))
        }
    }

    #[test]
    fn command_dto_is_strict_and_uses_ui_action_names() {
        let parsed: Command = serde_json::from_str(r#"{"action":"capture_start","phase":"held_out_hold","axis":0,"direction":1,"repeat":0,"level":1.0,"instruction":"positive","intent":[1,0,0,0,0,0,0],"sample_count":16}"#).unwrap();
        assert!(matches!(
            parsed,
            Command::CaptureStart {
                phase: CapturePhase::HeldOutHold,
                sample_count: Some(16),
                ..
            }
        ));
        assert!(serde_json::from_str::<Command>(r#"{"action":"capture_start","phase":"hold","axis":0,"direction":1,"repeat":0,"level":1,"instruction":"x","unknown":1}"#).is_err());
    }

    #[test]
    fn connect_matches_info_request_and_bounds_unknown_lines() {
        let source = "0123456789abcdef0123456789abcdef01234567";
        let id = "srn-0123456789abcdef0123456789abcdef";
        let old = format!(
            "info3,1,1.2,{source},sparkfun_promicro16,atmega32u4,16000000,{id},1,1,127,0,1,1"
        );
        let current = format!(
            "info3,2,1.2,{source},sparkfun_promicro16,atmega32u4,16000000,{id},1,1,127,0,1,1"
        );
        let fake = FakeTransport::new(&[&old, &current]);
        let mut service = LiveService::with_test_session(Box::new(fake), 1);
        let temp = task_test_root().join("connect-test-storage");
        let result = service.execute(Command::Connect, &temp);
        assert!(result.is_ok());
    }

    #[test]
    fn setup_reload_selects_highest_revision_and_rejects_ambiguous_latest_snapshot() {
        let old = test_stored_setup(1, false);
        let latest = test_stored_setup(2, false);
        assert!(valid_setup_proof(&old) && valid_setup_proof(&latest));
        assert!(validate_setup_revision(std::slice::from_ref(&latest), &old.setup).is_err());
        let mut rebound = latest.setup.clone();
        rebound.channels[0].location = Some("different-location".into());
        assert!(validate_setup_revision(std::slice::from_ref(&latest), &rebound).is_err());
        let mut newer = latest.setup.clone();
        newer.config_revision = 3;
        assert!(validate_setup_revision(std::slice::from_ref(&latest), &newer).is_ok());

        let root = task_test_root().join(format!(
            "setup-selection-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let directory = device_directory(&root, latest.setup.device_id).unwrap();
        fs::create_dir_all(&directory).unwrap();
        for record in [&old, &latest] {
            let name = format!(
                "setup-a{}-c{}-g{}.srnsetup",
                record.setup.assembly_revision,
                record.setup.config_revision,
                record.setup.settings_generation
            );
            fs::write(directory.join(name), serde_json::to_vec(record).unwrap()).unwrap();
        }
        let mut info = test_info();
        info.negative_mask = 1;
        let mut service = LiveService::with_test_session(Box::new(FakeTransport::new(&[])), 1);
        service.info = Some(info.clone());
        service.load_setup_for_current(&root).unwrap();
        assert_eq!(service.setup.as_ref().unwrap().config_revision, 2);

        let ambiguous = test_stored_setup(2, true);
        fs::write(
            directory.join("setup-duplicate.srnsetup"),
            serde_json::to_vec(&ambiguous).unwrap(),
        )
        .unwrap();
        let mut service = LiveService::with_test_session(Box::new(FakeTransport::new(&[])), 1);
        service.info = Some(info);
        assert!(service.load_setup_for_current(&root).is_err());
        assert!(service.setup.is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capture_barrier_drains_prebuffered_samples_and_preserves_session_and_prior_windows() {
        let root = task_test_root();
        let (transport, reads) = QueueTransport::new();
        let writes = transport.writes.clone();
        let mut service = LiveService::with_test_session(Box::new(transport), 1);
        let info = test_info();
        let DeviceIdentity::Provisioned(id) = info.identity else {
            unreachable!()
        };
        service.info = Some(info.clone());
        service.setup = Some(test_setup(id));
        service.streaming = true;
        service.session_id = 1;
        service.connection_epoch = 17;
        let Frame::Sample(previous) = protocol_v3::parse_line(&sample_line(1, 10)).unwrap() else {
            unreachable!()
        };
        service.accept_sample(previous, &root).unwrap();
        let prior_window = vec![FirmwareVectorSample {
            timestamp_ms: 10,
            epoch: service.epoch_token(1),
            values: [10.0; 7],
            quality: InputQuality::Usable,
        }];
        service.global_neutral = Some(prior_window.clone());
        service.trials.insert(
            TrialKey {
                axis: 0,
                direction: 1,
                repeat: 0,
            },
            TrialParts {
                level: Some(1.0),
                neutral: Some(prior_window.clone()),
                ramp: Some(prior_window.clone()),
                hold: Some(prior_window.clone()),
                release: Some(prior_window),
            },
        );
        let prior_trials = format!("{:?}", service.trials);
        let prior_neutral = serde_json::to_vec(&service.global_neutral).unwrap();
        let prior_setup = format!("{:?}", service.setup);
        queue(&reads, sample_line(2, 999));
        queue(&reads, "ok3,1,stream,1");
        queue(&reads, sample_line(3, 999));
        queue(&reads, "ok3,2,stream,1");
        queue(&reads, sample_line(4, 20));

        let reply = service
            .execute(
                Command::CaptureStart {
                    phase: CapturePhase::TrialNeutral,
                    axis: Some(1),
                    direction: 1,
                    repeat: 0,
                    level: 1.0,
                    instruction: "next trial".into(),
                    intent: None,
                    sample_count: Some(8),
                },
                &root,
            )
            .unwrap();
        assert!(!reply.view.ready && !reply.view.sample_fresh);
        assert!(reply.view.sample.is_none() && reply.view.output.is_none());
        assert!(
            service
                .active_capture
                .as_ref()
                .unwrap()
                .window
                .samples
                .is_empty()
        );
        assert_eq!(service.last_sequence, Some(1));
        assert_eq!(service.info, Some(info));
        assert_eq!(format!("{:?}", service.setup), prior_setup);
        assert_eq!(service.connection_epoch, 17);
        assert_eq!(
            serde_json::to_vec(&service.global_neutral).unwrap(),
            prior_neutral
        );
        assert_eq!(format!("{:?}", service.trials), prior_trials);
        assert_eq!(
            *writes.lock().unwrap(),
            vec!["stream3,1,1,0\n", "stream3,2,1,1\n"]
        );

        service.execute(Command::CaptureTick, &root).unwrap();
        let active = service.active_capture.as_ref().unwrap();
        assert_eq!(active.window.samples.len(), 1);
        assert_eq!(active.window.samples[0].timestamp_ms, 40);
        assert_eq!(active.window.samples[0].values[1], 20.0);
        assert_eq!(active.window.wire_samples[0].sequence, 4);
        assert!(reads.lock().unwrap().is_empty());

        let Frame::Sample(old) = protocol_v3::parse_line(&sample_line(3, 999)).unwrap() else {
            unreachable!()
        };
        assert!(matches!(
            service.accept_sample(old, &root),
            Err(ServiceError::Fatal(_))
        ));
        assert_eq!(
            service
                .active_capture
                .as_ref()
                .unwrap()
                .window
                .samples
                .len(),
            1
        );
    }

    #[test]
    fn capture_barrier_mismatch_timeout_and_budget_fail_closed() {
        let root = task_test_root();
        let cases = [
            vec![],
            vec!["ok3,1,stream,2".to_owned()],
            vec!["ok3,1,stream,1".to_owned()],
            vec!["ok3,1,stream,1".to_owned(), "ok3,2,stream,2".to_owned()],
            vec!["ok3,99,stream,1".to_owned(), "ok3,1,tare,1".to_owned()],
            vec!["error3,1,stale_session".to_owned()],
            vec![
                "ok3,1,stream,1".to_owned(),
                "error3,2,stale_session".to_owned(),
            ],
            (0..READ_BUDGET)
                .map(|_| sample_line(2, 999))
                .chain(["ok3,1,stream,1".into(), "ok3,2,stream,1".into()])
                .collect(),
            std::iter::once("ok3,1,stream,1".into())
                .chain((0..READ_BUDGET).map(|_| sample_line(2, 999)))
                .chain(["ok3,2,stream,1".into()])
                .collect(),
        ];
        for lines in cases {
            let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
            let mut service =
                LiveService::with_test_session(Box::new(FakeTransport::new(&refs)), 1);
            let info = test_info();
            let DeviceIdentity::Provisioned(id) = info.identity else {
                unreachable!()
            };
            service.info = Some(info);
            service.setup = Some(test_setup(id));
            service.streaming = true;
            service.session_id = 1;
            let Frame::Sample(previous) = protocol_v3::parse_line(&sample_line(1, 10)).unwrap()
            else {
                unreachable!()
            };
            service.accept_sample(previous, &root).unwrap();
            assert!(
                matches!(
                    service.execute(
                        Command::CaptureStart {
                            phase: CapturePhase::Neutral,
                            axis: None,
                            direction: 1,
                            repeat: 0,
                            level: 0.0,
                            instruction: "neutral".into(),
                            intent: None,
                            sample_count: Some(8),
                        },
                        &root
                    ),
                    Err(ServiceError::Fatal(_))
                ),
                "{lines:?}"
            );
            assert!(service.fatal && !service.streaming);
            assert!(service.active_capture.is_none());
            let snapshot = service.execute(Command::Snapshot, &root).unwrap().view;
            assert!(!snapshot.ready && !snapshot.sample_fresh);
            assert!(snapshot.sample.is_none() && snapshot.output.is_none());
        }
    }

    #[test]
    fn capture_barrier_transport_read_and_write_failures_are_fatal() {
        struct FailingTransport {
            fail_write: usize,
            writes: usize,
        }
        impl LineTransport for FailingTransport {
            fn discard_input(&mut self) -> Result<(), TransportError> {
                panic!("barrier must use acknowledgements")
            }
            fn write_line(&mut self, _line: &str) -> Result<(), TransportError> {
                self.writes += 1;
                if self.writes == self.fail_write {
                    Err(TransportError::Io("ambiguous write".into()))
                } else {
                    Ok(())
                }
            }
            fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError> {
                assert!(timeout <= INFO_TIMEOUT);
                if self.fail_write == 0 || self.fail_write == 3 && self.writes == 2 {
                    Err(TransportError::Io("ambiguous read".into()))
                } else {
                    Ok("ok3,1,stream,1".into())
                }
            }
        }
        for fail_write in [0, 1, 2, 3] {
            let mut service = LiveService::with_test_session(
                Box::new(FailingTransport {
                    fail_write,
                    writes: 0,
                }),
                1,
            );
            service.streaming = true;
            service.session_id = 1;
            assert!(matches!(
                service.capture_acquisition_barrier(),
                Err(ServiceError::Fatal(_))
            ));
            assert!(!service.streaming && service.active_capture.is_none());
        }
    }

    #[test]
    fn capture_tick_consumes_stream_and_projects_device_oriented_counts_once() {
        let info = test_info();
        let DeviceIdentity::Provisioned(id) = &info.identity else {
            unreachable!()
        };
        let id = *id;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store =
            task_test_root().join(format!("capture-e2e-{}-{timestamp}", std::process::id()));
        let lines: Vec<String> = ["ok3,1,stream,1".into(), "ok3,2,stream,1".into()]
            .into_iter()
            .chain((1..=8).map(|sequence| sample_line(sequence, 10)))
            .collect();
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let mut service = LiveService::with_test_session(Box::new(FakeTransport::new(&refs)), 1);
        service.info = Some(info);
        service.setup = Some(test_setup(id));
        service.streaming = true;
        service.session_id = 1;

        service
            .execute(
                Command::CaptureStart {
                    phase: CapturePhase::Neutral,
                    axis: None,
                    direction: 1,
                    repeat: 0,
                    level: 0.0,
                    instruction: "natural neutral".into(),
                    intent: None,
                    sample_count: Some(8),
                },
                &store,
            )
            .unwrap();
        let reply = service.execute(Command::CaptureTick, &store).unwrap();

        let capture = reply.view.capture.unwrap();
        assert!(capture.complete && capture.completed);
        assert_eq!(capture.done, 8);
        assert_eq!(service.global_neutral.as_ref().unwrap()[0].values[1], 10.0);
        assert_eq!(
            capture_archive_usage(&store.join(id.to_string()).join("captures"))
                .unwrap()
                .0,
            1
        );
        std::fs::remove_dir_all(&store).unwrap();
    }

    fn verified_live_service() -> (LiveService, SharedReads, PathBuf, u32, u32) {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store =
            task_test_root().join(format!("service-e2e-{}-{timestamp}", std::process::id()));
        let (transport, reads) = QueueTransport::new();
        let mut service = LiveService::with_test_session(Box::new(transport), 1);

        queue(&reads, info_line(1, 1, 1, 0, 0, 1));
        service.execute(Command::Connect, &store).unwrap();
        queue(&reads, "ok3,2,stream,1");
        service.execute(Command::StreamStart, &store).unwrap();
        queue(&reads, sample_line(1, 100));
        service.execute(Command::StreamTick, &store).unwrap();
        service
            .execute(
                Command::Setup {
                    setup: Box::new(confirmed_setup_dto()),
                },
                &store,
            )
            .unwrap();

        queue(&reads, "ok3,3,stream,1");
        queue(&reads, "ok3,4,settings,2");
        queue(&reads, info_line(5, 2, 2, 0x7f, 1, 1));
        queue(&reads, "ok3,6,stream,1");
        service
            .execute(
                Command::SaveDeviceSettings {
                    expected_generation: 1,
                    known_mask: 0x7f,
                    negative_mask: 1,
                },
                &store,
            )
            .unwrap();

        queue(&reads, "ok3,7,stream,1");
        queue(&reads, "ok3,8,tare,2");
        queue(&reads, info_line(9, 2, 2, 0x7f, 1, 2));
        queue(&reads, "ok3,10,stream,1");
        service
            .execute(Command::Tare { confirmed: true }, &store)
            .unwrap();
        service
            .execute(
                Command::SetOperator {
                    operator: "operator-1".into(),
                    posture: "neutral posture".into(),
                    grip: "comfortable grip".into(),
                },
                &store,
            )
            .unwrap();

        let (mut sequence, mut device_ms) = (1u32, 10u32);
        let neutral = [100; 7];
        capture_window(
            &mut service,
            &reads,
            &store,
            &mut sequence,
            &mut device_ms,
            CapturePhase::Neutral,
            None,
            1,
            0,
            0.0,
            "natural neutral",
            None,
            neutral,
        );

        for axis in 0..7 {
            for (repeat, level) in [(0, 1.0), (1, 0.5)] {
                capture_window(
                    &mut service,
                    &reads,
                    &store,
                    &mut sequence,
                    &mut device_ms,
                    CapturePhase::TrialNeutral,
                    Some(axis),
                    1,
                    repeat,
                    level,
                    "before",
                    None,
                    neutral,
                );
                let mut ramp = neutral;
                ramp[axis] += (10.0 * level) as i32;
                capture_window(
                    &mut service,
                    &reads,
                    &store,
                    &mut sequence,
                    &mut device_ms,
                    CapturePhase::Ramp,
                    Some(axis),
                    1,
                    repeat,
                    level,
                    "ramp",
                    None,
                    ramp,
                );
                let mut hold = neutral;
                hold[axis] += (20.0 * level) as i32;
                capture_window(
                    &mut service,
                    &reads,
                    &store,
                    &mut sequence,
                    &mut device_ms,
                    CapturePhase::Hold,
                    Some(axis),
                    1,
                    repeat,
                    level,
                    "hold",
                    None,
                    hold,
                );
                capture_window(
                    &mut service,
                    &reads,
                    &store,
                    &mut sequence,
                    &mut device_ms,
                    CapturePhase::Release,
                    Some(axis),
                    1,
                    repeat,
                    level,
                    "release",
                    None,
                    neutral,
                );
            }
        }
        for (repeat, level) in [(0, 1.0), (1, 0.5)] {
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::TrialNeutral,
                Some(0),
                -1,
                repeat,
                level,
                "reverse before",
                None,
                neutral,
            );
            let mut ramp = neutral;
            ramp[0] -= (10.0 * level) as i32;
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::Ramp,
                Some(0),
                -1,
                repeat,
                level,
                "reverse ramp",
                None,
                ramp,
            );
            let mut hold = neutral;
            hold[0] -= (20.0 * level) as i32;
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::Hold,
                Some(0),
                -1,
                repeat,
                level,
                "reverse hold",
                None,
                hold,
            );
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::Release,
                Some(0),
                -1,
                repeat,
                level,
                "reverse release",
                None,
                neutral,
            );
        }

        let limits = FitLimitsDto {
            noise_floor_counts: [0.2; 7],
            max_rest_range_counts: [1.0; 7],
            max_drift_counts: [2.0; 7],
            min_signal_to_noise: 5.0,
            repeat_tolerance: 0.05,
            direction_tolerance: 0.1,
            max_condition_number: 20.0,
            max_noise_gain: 10.0,
            max_sample_gap_ms: 100,
        };
        let signal_settings = SignalSettingsDto {
            time_constant_ms: 0.0,
            noise_multiplier: 3.0,
            release_ratio: 0.5,
            max_deadzone_fraction: 0.1,
        };
        service
            .execute(
                Command::Fit {
                    limits,
                    signal_settings,
                    method: Some(MethodDto::Diagonal),
                },
                &store,
            )
            .unwrap();

        let mut heldout_trials = Vec::new();
        let mut intent = [0.0; 7];
        intent[0] = 1.0;
        heldout_trials.push((0, 1, 1.0, "axis0-full".to_owned(), intent));
        let mut intent = [0.0; 7];
        intent[0] = 0.25;
        heldout_trials.push((0, 1, 0.25, "axis0-weak".to_owned(), intent));
        for axis in 1..7 {
            let mut intent = [0.0; 7];
            intent[axis] = 1.0;
            heldout_trials.push((axis, 1, 1.0, format!("axis{axis}-full"), intent));
        }
        let mut intent = [0.0; 7];
        intent[0] = -1.0;
        heldout_trials.push((0, -1, 1.0, "axis0-negative".to_owned(), intent));
        let mut intent = [0.0; 7];
        intent[0] = 0.5;
        intent[1] = 0.5;
        heldout_trials.push((0, 1, 0.5, "axes0-1-combined".to_owned(), intent));
        for (axis, direction, level, instruction, intent) in heldout_trials {
            let zero = [0.0; 7];
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::HeldOutNeutral,
                Some(axis),
                direction,
                0,
                0.0,
                &instruction,
                Some(zero),
                neutral,
            );
            let mut ramp = neutral;
            let mut hold = neutral;
            for index in 0..7 {
                ramp[index] += (10.0 * intent[index]) as i32;
                hold[index] += (20.0 * intent[index]) as i32;
            }
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::HeldOutRamp,
                Some(axis),
                direction,
                0,
                level,
                &instruction,
                Some(intent),
                ramp,
            );
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::HeldOutHold,
                Some(axis),
                direction,
                0,
                level,
                &instruction,
                Some(intent),
                hold,
            );
            capture_window(
                &mut service,
                &reads,
                &store,
                &mut sequence,
                &mut device_ms,
                CapturePhase::HeldOutRelease,
                Some(axis),
                direction,
                0,
                0.0,
                &instruction,
                Some(zero),
                neutral,
            );
        }
        service
            .execute(
                Command::VerifyHeldOut {
                    method: MethodDto::Diagonal,
                },
                &store,
            )
            .unwrap();
        (service, reads, store, sequence, device_ms)
    }

    pub(crate) fn saved_profile_fixture() -> PathBuf {
        let (mut service, _, store, _, _) = verified_live_service();
        let save = service
            .execute(
                Command::SaveCalibration {
                    profile_revision: 1,
                    method: MethodDto::Diagonal,
                    confirmed: true,
                },
                &store,
            )
            .unwrap();
        store
            .join("srn-0123456789abcdef0123456789abcdef")
            .join(save.view.saved_path.unwrap())
    }

    #[test]
    fn loaded_profile_metadata_reports_the_verified_method() {
        let (mut service, _, store, _, _) = verified_live_service();
        service
            .execute(
                Command::VerifyHeldOut {
                    method: MethodDto::Coupled,
                },
                &store,
            )
            .unwrap();
        let save = service
            .execute(
                Command::SaveCalibration {
                    profile_revision: 1,
                    method: MethodDto::Coupled,
                    confirmed: true,
                },
                &store,
            )
            .unwrap();
        let loaded = service
            .execute(
                Command::LoadCalibration {
                    record_path: save.view.saved_path.unwrap(),
                },
                &store,
            )
            .unwrap();
        assert_eq!(loaded.view.profile.unwrap().method, MethodDto::Coupled);
        std::fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn live_service_flow_binds_captures_persists_reload_revalidates_and_requires_fresh_neutral() {
        let (mut service, reads, store, mut sequence, mut device_ms) = verified_live_service();
        let neutral = [100; 7];
        let save = service
            .execute(
                Command::SaveCalibration {
                    profile_revision: 1,
                    method: MethodDto::Diagonal,
                    confirmed: true,
                },
                &store,
            )
            .unwrap();
        let profile_name = save.view.saved_path.expect("saved profile basename");
        let profile_path = store
            .join("srn-0123456789abcdef0123456789abcdef")
            .join(&profile_name);
        let saved_bytes = std::fs::read(&profile_path).unwrap();
        assert!(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&profile_path)
                .is_err(),
            "profile paths are immutable"
        );
        assert_eq!(std::fs::read(&profile_path).unwrap(), saved_bytes);

        service
            .execute(
                Command::LoadCalibration {
                    record_path: profile_name,
                },
                &store,
            )
            .unwrap();
        assert!(service.heldout_view.passed);
        assert_eq!(service.heldout_method, Some(InputMethod::Diagonal));
        assert!(service.session_neutral.is_none());
        capture_window(
            &mut service,
            &reads,
            &store,
            &mut sequence,
            &mut device_ms,
            CapturePhase::SessionNeutral,
            None,
            1,
            0,
            0.0,
            "fresh session neutral",
            None,
            neutral,
        );
        let applied = service
            .execute(
                Command::ApplyCalibration {
                    confirmed: true,
                    method: MethodDto::Diagonal,
                },
                &store,
            )
            .unwrap();
        assert!(applied.view.ready);

        let request = service.request_id;
        queue(&reads, format!("ok3,{},stream,1", request + 1));
        queue(&reads, format!("ok3,{},tare,3", request + 2));
        queue(&reads, info_line(request + 3, 2, 2, 0x7f, 1, 3));
        queue(&reads, format!("ok3,{},stream,1", request + 4));
        let after_tare = service
            .execute(Command::Tare { confirmed: true }, &store)
            .unwrap();
        assert!(!after_tare.view.ready);
        assert!(service.session_neutral.is_none() && service.processor.is_none());
        assert!(
            service.fit.is_none() && service.heldout.is_empty() && !service.heldout_view.passed
        );
        queue(&reads, format!("ok3,{},stream,1", service.request_id + 1));
        let disconnected = service.execute(Command::Disconnect, &store).unwrap();
        assert!(!disconnected.view.connected && !disconnected.view.ready);
        assert!(service.heldout.is_empty() && !service.heldout_view.passed);
        std::fs::remove_dir_all(&store).unwrap();
    }

    fn start_recovery_capture(
        service: &mut LiveService,
        reads: &SharedReads,
        store: &Path,
        phase: CapturePhase,
    ) {
        let mut intent = [0.0; 7];
        if matches!(phase, CapturePhase::HeldOutRamp | CapturePhase::HeldOutHold) {
            intent[0] = 1.0;
        }
        queue_capture_barrier(service, reads);
        service
            .execute(
                Command::CaptureStart {
                    phase,
                    axis: Some(0),
                    direction: 1,
                    repeat: 0,
                    level: intent[0],
                    instruction: "axis0-full".into(),
                    intent: Some(intent),
                    sample_count: Some(8),
                },
                store,
            )
            .unwrap();
    }

    fn runtime_sample(sequence: &mut u32, device_ms: &mut u32, fresh: bool) -> String {
        *sequence += 1;
        *device_ms += 10;
        let mut fields = vec![
            "sample3".into(),
            "1".into(),
            sequence.to_string(),
            device_ms.to_string(),
            "2".into(),
            "2".into(),
            "1".into(),
        ];
        fields.extend([1000; 7].map(|value| value.to_string()));
        fields.extend([100; 7].map(|value| value.to_string()));
        fields.extend([if fresh { 1 } else { 2 }; 7].map(|value| value.to_string()));
        fields.join(",")
    }

    struct ManualClock {
        origin: Instant,
        micros: AtomicU64,
    }
    impl ManualClock {
        fn new() -> Self {
            Self {
                origin: Instant::now(),
                micros: AtomicU64::new(0),
            }
        }
        fn advance(&self, us: u64) {
            self.micros.fetch_add(us, Ordering::SeqCst);
        }
    }
    impl MonotonicClock for ManualClock {
        fn now(&self) -> Instant {
            self.origin + Duration::from_micros(self.micros.load(Ordering::SeqCst))
        }
    }
    struct TimedTransport {
        clock: Arc<ManualClock>,
        lines: VecDeque<(u64, String)>,
    }
    impl LineTransport for TimedTransport {
        fn discard_input(&mut self) -> Result<(), TransportError> {
            self.lines.clear();
            Ok(())
        }
        fn write_line(&mut self, _line: &str) -> Result<(), TransportError> {
            Ok(())
        }
        fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError> {
            let now = self.clock.micros.load(Ordering::SeqCst);
            let until = now + timeout.as_micros() as u64;
            if let Some((at, _)) = self.lines.front().filter(|(at, _)| *at <= until) {
                self.clock.advance(at.saturating_sub(now));
                return Ok(self.lines.pop_front().unwrap().1);
            }
            self.clock.advance(timeout.as_micros() as u64);
            Err(TransportError::Timeout)
        }
    }

    #[test]
    fn rated_transport_delivers_every_normalized_frame_at_twenty_forty_and_eighty_hz() {
        for hz in [20_u32, 40, 80] {
            let (mut service, reads, store, mut sequence, mut device_ms) = verified_live_service();
            start_recovery_capture(&mut service, &reads, &store, CapturePhase::SessionNeutral);
            queue_live_window(&reads, &mut sequence, &mut device_ms, [100; 7], 2, 2);
            service.execute(Command::CaptureTick, &store).unwrap();
            service
                .execute(
                    Command::ApplyCalibration {
                        confirmed: true,
                        method: MethodDto::Diagonal,
                    },
                    &store,
                )
                .unwrap();
            let context = service.applied_context.clone().unwrap();
            let mut reference = InputProcessor::new_for_session(
                service.fit.as_ref().unwrap(),
                context.clone(),
                &[100.0; 7],
                InputMethod::Diagonal,
                service.signal_settings.unwrap(),
            )
            .unwrap();
            let clock = Arc::new(ManualClock::new());
            service.clock = clock.clone();
            service.stream_monitor = StreamMonitor::new(clock.origin);
            let count = hz * 2 + 1;
            let mut lines = VecDeque::new();
            let mut expected = Vec::new();
            for n in 0..count {
                let us = u64::from(n) * 1_000_000 / u64::from(hz);
                let timestamp_ms = device_ms + 1 + (us / 1000) as u32;
                let mut values = [100_i32; 7];
                if n > 0 {
                    values[0] += (n % 20) as i32;
                }
                let sample = Sample {
                    session: 1,
                    sequence: sequence + n + 1,
                    device_ms: timestamp_ms,
                    tare_epoch: 2,
                    generation: 2,
                    tare_valid: true,
                    raw: [Some(1000); 7],
                    oriented: values.map(Some),
                    flags: [FLAG_FRESH; 7],
                };
                let mut fields = vec![
                    "sample3".into(),
                    "1".into(),
                    sample.sequence.to_string(),
                    timestamp_ms.to_string(),
                    "2".into(),
                    "2".into(),
                    "1".into(),
                ];
                fields.extend([1000; 7].map(|value| value.to_string()));
                fields.extend(values.map(|value| value.to_string()));
                fields.extend([1; 7].map(|value| value.to_string()));
                lines.push_back((us, fields.join(",")));
                let out = reference
                    .process(
                        &context,
                        &FirmwareVectorSample {
                            timestamp_ms,
                            epoch: service.epoch_token(2),
                            values: values.map(f64::from),
                            quality: InputQuality::Usable,
                        },
                    )
                    .unwrap();
                expected.push((sample.sequence, out.command));
            }
            service.transport = Box::new(TimedTransport {
                clock: clock.clone(),
                lines,
            });
            let mut observed = Vec::new();
            while observed.len() < count as usize {
                // A GUI-style 50 ms pause still lets the shared service deliver
                // multiple producer frames, without dropping all but the latest.
                clock.advance(50_000);
                let reply = service.execute(Command::StreamTick, &store).unwrap();
                assert!(reply.sample_events.len() <= STREAM_DRAIN_BUDGET);
                for event in reply.sample_events {
                    assert!(event.ready);
                    observed.push((event.sample.sequence, event.output.unwrap().command));
                }
            }
            assert_eq!(
                observed, expected,
                "{hz} Hz changed or lost conditioned input"
            );
            let empty = service.execute(Command::Snapshot, &store).unwrap();
            assert!(
                empty.sample_events.is_empty(),
                "snapshot must not replay old frames"
            );
            assert_eq!(empty.view.diagnostics.received_frames, u64::from(count));
            assert_eq!(empty.view.diagnostics.normalized_frames, u64::from(count));
            assert_eq!(empty.view.diagnostics.sequence_gaps, 0);
            assert!(empty.view.diagnostics.rate_window_samples <= 256);
            std::fs::remove_dir_all(store).unwrap();
        }
    }

    #[test]
    fn stream_batch_is_bounded_and_invalid_frames_cannot_leave_partial_success() {
        let mut service = LiveService::with_test_session(Box::new(FakeTransport::new(&[])), 1);
        service.info = Some(test_info());
        service.streaming = true;
        let (transport, reads) = QueueTransport::new();
        service.transport = Box::new(transport);
        for n in 1..=32 {
            queue(&reads, sample_line(n, 10));
        }
        let first = service
            .execute(Command::StreamTick, Path::new("unused-fixture-store"))
            .unwrap();
        assert_eq!(first.sample_events.len(), 16);
        assert_eq!(first.view.diagnostics.drain_budget_exhaustions, 1);
        let second = service
            .execute(Command::StreamTick, Path::new("unused-fixture-store"))
            .unwrap();
        assert_eq!(second.sample_events[0].sample.sequence, 17);
        queue(&reads, sample_line(33, 10));
        queue(&reads, sample_line(33, 10));
        assert!(
            service
                .execute(Command::StreamTick, Path::new("unused-fixture-store"))
                .is_err()
        );
        let failed = service
            .execute(Command::Snapshot, Path::new("unused-fixture-store"))
            .unwrap();
        assert!(!failed.view.ready && failed.sample_events.is_empty());
    }

    #[test]
    fn host_backlog_cannot_refresh_old_samples_by_reading_them_late() {
        let clock = Arc::new(ManualClock::new());
        let (transport, reads) = QueueTransport::new();
        let mut service = LiveService::with_clock(Box::new(transport), clock.clone(), clock.origin);
        service.session_id = 1;
        service.info = Some(test_info());
        service.streaming = true;
        queue(&reads, sample_line(1, 10));
        service
            .execute(Command::StreamTick, Path::new("unused-fixture-store"))
            .unwrap();
        clock.advance(300_000);
        queue(&reads, sample_line(2, 10));
        assert!(
            service
                .execute(Command::StreamTick, Path::new("unused-fixture-store"))
                .is_err()
        );
        let view = service
            .execute(Command::Snapshot, Path::new("unused-fixture-store"))
            .unwrap()
            .view;
        assert!(!view.ready && view.output.is_none());
        assert_eq!(view.diagnostics.relative_backlog_ms, Some(290));
        assert_eq!(view.diagnostics.stale_invalidations, 1);
    }

    #[test]
    fn verified_runtime_quality_failure_preserves_proof_but_requires_neutral_and_explicit_apply() {
        let (mut service, reads, store, mut sequence, mut device_ms) = verified_live_service();
        let proof = serde_json::to_vec(&service.stored_heldout().unwrap()).unwrap();
        for _ in 0..2 {
            start_recovery_capture(&mut service, &reads, &store, CapturePhase::SessionNeutral);
            queue_live_window(&reads, &mut sequence, &mut device_ms, [100; 7], 2, 2);
            service.execute(Command::CaptureTick, &store).unwrap();
            assert!(!service.snapshot_view().ready);
            service
                .execute(
                    Command::ApplyCalibration {
                        confirmed: true,
                        method: MethodDto::Diagonal,
                    },
                    &store,
                )
                .unwrap();
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, true));
            assert!(
                service
                    .execute(Command::StreamTick, &store)
                    .unwrap()
                    .view
                    .ready
            );
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, false));
            let bad = service.execute(Command::StreamTick, &store).unwrap().view;
            assert!(!bad.ready && bad.output.is_none());
            assert!(
                bad.heldout.passed,
                "completed independent proof must survive an ordinary runtime quality failure"
            );
            assert!(
                service.processor.is_none()
                    && service.applied_context.is_none()
                    && service.session_neutral.is_none()
            );
            assert_eq!(
                serde_json::to_vec(&service.stored_heldout().unwrap()).unwrap(),
                proof
            );
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, true));
            let good = service.execute(Command::StreamTick, &store).unwrap().view;
            assert!(!good.ready && good.output.is_none());
            assert!(
                service
                    .execute(
                        Command::ApplyCalibration {
                            confirmed: true,
                            method: MethodDto::Diagonal
                        },
                        &store
                    )
                    .is_err()
            );
        }
        std::fs::remove_dir_all(&store).unwrap();
    }

    #[test]
    fn bad_samples_abort_neutral_and_heldout_retries_without_certifying_partial_evidence() {
        let (mut service, reads, store, mut sequence, mut device_ms) = verified_live_service();
        let proof = serde_json::to_vec(&service.stored_heldout().unwrap()).unwrap();
        for _ in 0..2 {
            start_recovery_capture(&mut service, &reads, &store, CapturePhase::SessionNeutral);
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, true));
            service.execute(Command::CaptureTick, &store).unwrap();
            assert_eq!(
                service
                    .active_capture
                    .as_ref()
                    .unwrap()
                    .window
                    .samples
                    .len(),
                1
            );
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, false));
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, false));
            queue_live_window(&reads, &mut sequence, &mut device_ms, [100; 7], 2, 2);
            service.execute(Command::CaptureTick, &store).unwrap();
            assert!(service.session_neutral.is_none());
            assert!(!service.snapshot_view().capture.unwrap().completed);
            assert!(service.heldout_view.passed);
            assert_eq!(
                serde_json::to_vec(&service.stored_heldout().unwrap()).unwrap(),
                proof
            );
        }
        for phase in [
            CapturePhase::HeldOutRelease,
            CapturePhase::HeldOutHold,
            CapturePhase::HeldOutRamp,
            CapturePhase::HeldOutNeutral,
        ] {
            start_recovery_capture(&mut service, &reads, &store, phase);
            assert!(
                !service.heldout_view.passed,
                "starting a retry revokes verification before any new sample"
            );
            assert!(
                service
                    .execute(
                        Command::VerifyHeldOut {
                            method: MethodDto::Diagonal
                        },
                        &store
                    )
                    .is_err()
            );
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, true));
            queue(&reads, runtime_sample(&mut sequence, &mut device_ms, false));
            service.execute(Command::CaptureTick, &store).unwrap();
            let capture = service.snapshot_view().capture.unwrap();
            assert!(!capture.completed && capture.samples == 1);
            assert!(
                service.stored_heldout().is_err(),
                "a failed retry cannot reuse its prior completed phase"
            );
            assert!(
                service
                    .execute(
                        Command::VerifyHeldOut {
                            method: MethodDto::Diagonal
                        },
                        &store
                    )
                    .is_err()
            );
            assert!(!service.heldout_view.passed && service.session_neutral.is_none());
        }
        let archives = store.join("srn-0123456789abcdef0123456789abcdef/captures");
        let interrupted = std::fs::read_dir(archives)
            .unwrap()
            .filter_map(|entry| {
                let bytes = std::fs::read(entry.unwrap().path()).unwrap();
                let record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                (record["status"] == "interrupted").then_some(record)
            })
            .collect::<Vec<_>>();
        assert_eq!(interrupted.len(), 6);
        assert!(
            interrupted
                .iter()
                .all(|record| record["committed_for_current_session"] == false
                    && record["wire_samples"].as_array().unwrap().len() == 2)
        );
        std::fs::remove_dir_all(&store).unwrap();
    }

    #[test]
    fn verified_proof_is_invalidated_by_fatal_generation_tare_or_stream_session_change() {
        for field in [1, 4, 5] {
            let (mut service, reads, store, mut sequence, mut device_ms) = verified_live_service();
            let line = runtime_sample(&mut sequence, &mut device_ms, true);
            let mut fields = line.split(',').map(str::to_owned).collect::<Vec<_>>();
            fields[field] = "99".into();
            queue(&reads, fields.join(","));
            assert!(service.execute(Command::StreamTick, &store).is_err());
            assert!(service.fatal && service.fit.is_none() && service.heldout.is_empty());
            assert!(!service.heldout_view.passed && service.session_neutral.is_none());
            assert!(
                service
                    .execute(
                        Command::ApplyCalibration {
                            confirmed: true,
                            method: MethodDto::Diagonal
                        },
                        &store
                    )
                    .is_err()
            );
            std::fs::remove_dir_all(&store).unwrap();
        }
    }

    #[test]
    fn calibration_changes_require_revalidation_instead_of_runtime_neutral_recovery() {
        for change in [
            "operator", "setup", "firmware", "identity", "training", "fit",
        ] {
            let (mut service, reads, store, _, _) = verified_live_service();
            match change {
                "operator" => {
                    service
                        .execute(
                            Command::SetOperator {
                                operator: "another-operator".into(),
                                posture: "neutral posture".into(),
                                grip: "comfortable grip".into(),
                            },
                            &store,
                        )
                        .unwrap();
                }
                "setup" => {
                    let mut setup = confirmed_setup_dto();
                    setup.config_revision += 1;
                    service
                        .execute(
                            Command::Setup {
                                setup: Box::new(setup),
                            },
                            &store,
                        )
                        .unwrap();
                }
                "firmware" | "identity" => {
                    let line = info_line(service.request_id + 1, 2, 2, 0x7f, 1, 2);
                    let mut fields = line.split(',').map(str::to_owned).collect::<Vec<_>>();
                    if change == "firmware" {
                        fields[2] = "1.2.1".into();
                    } else {
                        fields[7] = "srn-11111111111111111111111111111111".into();
                    }
                    queue(&reads, fields.join(","));
                    assert!(service.execute(Command::Info, &store).is_err());
                    assert!(service.fatal);
                }
                "training" => {
                    start_recovery_capture(&mut service, &reads, &store, CapturePhase::Neutral);
                    assert!(service.global_neutral.is_none());
                }
                "fit" => {
                    let limits = service.fit_limits.clone().unwrap();
                    let signal_settings = service.signal_settings.unwrap();
                    service
                        .fit(&store, limits, signal_settings, Some(InputMethod::Diagonal))
                        .unwrap();
                }
                _ => unreachable!(),
            }
            assert!(
                !service.heldout_view.passed && service.heldout.is_empty(),
                "{change}"
            );
            assert!(
                service.session_neutral.is_none()
                    && service.processor.is_none()
                    && service.applied_context.is_none()
            );
            assert!(
                service
                    .execute(
                        Command::ApplyCalibration {
                            confirmed: true,
                            method: MethodDto::Diagonal
                        },
                        &store
                    )
                    .is_err()
            );
            std::fs::remove_dir_all(&store).unwrap();
        }
    }

    #[test]
    fn rejected_tare_keeps_cached_info_tare_invalid() {
        let id: DeviceId = "srn-0123456789abcdef0123456789abcdef".parse().unwrap();
        let readback = info_line(4, 1, 1, 0x7f, 0, 3);
        let responses = [
            "ok3,1,stream,1",
            "error3,2,tare_rejected",
            "ok3,3,tare,3",
            readback.as_str(),
            "ok3,5,stream,1",
        ];
        let mut service =
            LiveService::with_test_session(Box::new(FakeTransport::new(&responses)), 1);
        service.info = Some(DeviceInfo {
            firmware_version: "1.2.0".into(),
            source_revision: "0123456789abcdef0123456789abcdef01234567".into(),
            board: "sparkfun_promicro16".into(),
            mcu: "atmega32u4".into(),
            clock_hz: 16_000_000,
            identity: DeviceIdentity::Provisioned(id),
            generation: 1,
            assembly_revision: 1,
            known_mask: 0x7f,
            negative_mask: 0,
            tare_epoch: 1,
            tare_valid: true,
        });
        service.streaming = true;
        let root = task_test_root();
        assert!(
            service
                .execute(Command::Tare { confirmed: true }, &root)
                .is_err()
        );
        assert!(!service.streaming && service.resume_after_tare);
        let snapshot = service.execute(Command::Snapshot, &root).unwrap();
        assert!(!snapshot.view.info.unwrap().tare_valid);
        assert!(!snapshot.view.ready);
        let retried = service
            .execute(Command::Tare { confirmed: true }, &root)
            .unwrap();
        assert!(retried.view.connected && service.streaming);
        assert!(!service.resume_after_tare);
        assert_eq!(service.session_id, 1);
    }

    #[test]
    fn weak_and_coupled_heldout_intent_is_explicit() {
        let mut output = [None; 7];
        output[0] = Some(0.25);
        output[2] = Some(-0.5);
        let mut intent = [0.0; 7];
        intent[0] = 0.25;
        intent[2] = -0.5;
        output[1] = Some(0.04);
        assert!(command_matches_intent(&output, &intent, 0.1, true));
        output[1] = Some(0.06);
        assert!(!command_matches_intent(&output, &intent, 0.1, true));
        output[1] = None;
        output[2] = Some(0.5);
        assert!(!command_matches_intent(&output, &intent, 0.1, true));
    }

    #[test]
    fn persisted_profile_names_reject_paths_and_noncanonical_characters() {
        assert!(valid_profile_name("profile-0000000001-001.srncal"));
        assert!(!valid_profile_name("..\\profile.srncal"));
        assert!(!valid_profile_name("C:\\profile.srncal"));
    }
}
