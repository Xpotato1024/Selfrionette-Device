//! File/offline foundation. These checks never certify physical quality or enable control.
//! Wire indices are preserved; no finger layout or unknown polarity has a default.
use crate::input_calibration::{self, CalibrationRecording, FitLimits, FittedInput, InputQuality};
use crate::{CHANNEL_COUNT, DeviceId};

pub const N: usize = CHANNEL_COUNT;
pub const MAX_LABEL_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SampleDomain {
    RawAdc,
    LegacyFirmwareV2,
    DeviceOrientedCounts,
    Newtons,
    NormalizedInput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Synthetic,
    Recorded,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub origin: Origin,
    /// Caller-supplied acquisition annotation, never inferred from ordinary v2 vectors.
    pub quality: InputQuality,
    pub reference: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelOrientation {
    pub logical_axis: Option<usize>,
    pub location: Option<String>,
    pub positive_direction: Option<String>,
    /// Relative to raw ADC, NOT the already negated firmware-v2 vector.
    pub polarity: Option<i8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceSetup {
    pub device_id: DeviceId,
    pub assembly_revision: u32,
    pub config_revision: u32,
    pub settings_generation: u32,
    /// Indexed by original wire channel. Values describe the explicitly observed mapping.
    pub channels: [ChannelOrientation; N],
    /// Assertion about this entire mapping at these revisions; not a physical certificate.
    pub mapping_validated: bool,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FirmwareBinding {
    pub version: String,
    /// Exact source commit, not a mutable branch name. File assertions are not readback.
    pub source_revision: String,
    pub sample_domain: SampleDomain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorContext {
    pub operator: String,
    pub posture: String,
    pub grip: String,
}

/// Metadata for an existing/future parameter record, NOT a usable parameter record itself.
/// No session neutral is persisted here. The full setup snapshot detects edits even if a
/// caller accidentally reuses a revision number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileBinding {
    pub profile_revision: u32,
    pub parameter_record: String,
    pub setup: DeviceSetup,
    pub firmware: FirmwareBinding,
    pub operator: OperatorContext,
    pub evidence: Evidence,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentContext {
    pub setup: DeviceSetup,
    pub firmware: FirmwareBinding,
    pub operator: OperatorContext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupError {
    InvalidMetadata,
    InvalidRevision,
    InvalidMapping,
    InvalidPolarity,
    IncompleteSetup,
    RevalidationRequired,
    FirmwareMismatch,
    DomainMismatch,
    OperatorMismatch,
    EvidenceUnusable,
    SyntheticOnly,
    InvalidSample,
    FreshNeutralRequired,
    UnsupportedFitDomain,
    Fit(input_calibration::InputError),
}
impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SetupError {}

fn label(s: &str) -> Result<(), SetupError> {
    if s.is_empty() || s.trim() != s || s.len() > MAX_LABEL_BYTES || s.chars().any(char::is_control)
    {
        return Err(SetupError::InvalidMetadata);
    }
    Ok(())
}
impl Evidence {
    pub fn validate(&self) -> Result<(), SetupError> {
        label(&self.reference)
    }
    pub fn require_recorded_usable(&self) -> Result<(), SetupError> {
        self.validate()?;
        if self.origin != Origin::Recorded {
            return Err(SetupError::SyntheticOnly);
        }
        if self.quality != InputQuality::Usable {
            return Err(SetupError::EvidenceUnusable);
        }
        Ok(())
    }
}
impl DeviceSetup {
    /// Partial observations remain inspectable, but cannot be projected or profile-bound.
    pub fn validate(&self) -> Result<(), SetupError> {
        if self.assembly_revision == 0 || self.config_revision == 0 || self.settings_generation == 0
        {
            return Err(SetupError::InvalidRevision);
        }
        self.evidence.validate()?;
        let mut seen = [false; N];
        for (wire, ch) in self.channels.iter().enumerate() {
            if let Some(axis) = ch.logical_axis {
                if axis >= N || seen[axis] {
                    return Err(SetupError::InvalidMapping);
                }
                seen[axis] = true;
            }
            if ch.polarity.is_some_and(|p| p != -1 && p != 1) {
                return Err(SetupError::InvalidPolarity);
            }
            for value in [&ch.location, &ch.positive_direction].into_iter().flatten() {
                label(value)?;
            }
            if let (Some(location), Some(direction)) = (&ch.location, &ch.positive_direction) {
                if self.channels[..wire].iter().any(|previous| {
                    previous.location.as_ref() == Some(location)
                        && previous.positive_direction.as_ref() == Some(direction)
                }) {
                    return Err(SetupError::InvalidMapping);
                }
            }
        }
        if self.mapping_validated && !self.is_complete() {
            return Err(SetupError::IncompleteSetup);
        }
        Ok(())
    }
    pub fn is_complete(&self) -> bool {
        self.channels.iter().all(|ch| {
            ch.logical_axis.is_some()
                && ch.location.is_some()
                && ch.positive_direction.is_some()
                && ch.polarity.is_some()
        })
    }
    pub fn require_validated_mapping(&self) -> Result<(), SetupError> {
        self.validate()?;
        if !self.is_complete() || !self.mapping_validated {
            return Err(SetupError::IncompleteSetup);
        }
        if self.evidence.quality != InputQuality::Usable {
            return Err(SetupError::EvidenceUnusable);
        }
        Ok(())
    }
}
impl FirmwareBinding {
    pub fn validate(&self) -> Result<(), SetupError> {
        label(&self.version)?;
        if self.source_revision.len() != 40
            || self.source_revision.bytes().all(|b| b == b'0')
            || !self
                .source_revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(SetupError::InvalidMetadata);
        }
        Ok(())
    }
}
impl OperatorContext {
    pub fn validate(&self) -> Result<(), SetupError> {
        label(&self.operator)?;
        label(&self.posture)?;
        label(&self.grip)
    }
}
impl CurrentContext {
    pub fn validate(&self) -> Result<(), SetupError> {
        self.setup.validate()?;
        self.firmware.validate()?;
        self.operator.validate()
    }
}
impl ProfileBinding {
    pub fn validate(&self) -> Result<(), SetupError> {
        if self.profile_revision == 0 {
            return Err(SetupError::InvalidRevision);
        }
        label(&self.parameter_record)?;
        self.setup.require_validated_mapping()?;
        self.firmware.validate()?;
        self.operator.validate()?;
        self.evidence.validate()?;
        if self.evidence.origin != self.setup.evidence.origin
            || self.evidence.quality != InputQuality::Usable
        {
            return Err(SetupError::EvidenceUnusable);
        }
        Ok(())
    }
    /// Structural compatibility only. Does not load parameters or restore readiness.
    pub fn check_compatibility(&self, current: &CurrentContext) -> Result<(), SetupError> {
        self.validate()?;
        current.validate()?;
        if self.setup != current.setup {
            return Err(SetupError::RevalidationRequired);
        }
        if self.firmware.sample_domain != current.firmware.sample_domain {
            return Err(SetupError::DomainMismatch);
        }
        if self.firmware != current.firmware {
            return Err(SetupError::FirmwareMismatch);
        }
        if self.operator != current.operator {
            return Err(SetupError::OperatorMismatch);
        }
        Ok(())
    }
    /// Reuses the existing v2 fitter. Raw/oriented fitting requires a future domain-aware
    /// engine contract; relabelling those counts as v2 is explicitly unsupported.
    pub fn fit_v2(
        &self,
        current: &CurrentContext,
        recording: &CalibrationRecording,
        limits: &FitLimits,
    ) -> Result<FittedInput, SetupError> {
        self.check_compatibility(current)?;
        if self.firmware.sample_domain != SampleDomain::LegacyFirmwareV2 {
            return Err(SetupError::UnsupportedFitDomain);
        }
        let context = &recording.context;
        if context.source.id != self.setup.device_id
            || context.source.assembly != self.setup.assembly_revision.to_string()
        {
            return Err(SetupError::RevalidationRequired);
        }
        if context.source.firmware != self.firmware.version {
            return Err(SetupError::FirmwareMismatch);
        }
        if context.user != self.operator.operator || context.posture != self.operator.posture {
            return Err(SetupError::OperatorMismatch);
        }
        for (wire, ch) in self.setup.channels.iter().enumerate() {
            let axis = ch.logical_axis.ok_or(SetupError::IncompleteSetup)?;
            if recording.axes[axis].channel != wire
                || Some(&recording.axes[axis].positive_direction) != ch.positive_direction.as_ref()
            {
                return Err(SetupError::InvalidMapping);
            }
        }
        input_calibration::fit(recording, limits).map_err(SetupError::Fit)
    }
}

#[derive(Clone, Debug)]
pub struct DomainSample {
    pub domain: SampleDomain,
    pub values: [f64; N],
    pub quality: InputQuality,
}
#[derive(Clone, Debug)]
pub struct OrientedCounts {
    values: [f64; N],
}
impl OrientedCounts {
    pub fn values(&self) -> &[f64; N] {
        &self.values
    }
}

/// Raw 24-bit ADC -> explicitly mapped logical device counts, once. No tare, N
/// coefficient, filtering or operator normalization is manufactured here.
pub fn project_raw(
    setup: &DeviceSetup,
    sample: &DomainSample,
) -> Result<OrientedCounts, SetupError> {
    setup.require_validated_mapping()?;
    if sample.domain != SampleDomain::RawAdc {
        return Err(SetupError::DomainMismatch);
    }
    if sample.quality != InputQuality::Usable {
        return Err(SetupError::EvidenceUnusable);
    }
    if sample
        .values
        .iter()
        .any(|v| !v.is_finite() || v.fract() != 0.0 || !(-8388608.0..=8388607.0).contains(v))
    {
        return Err(SetupError::InvalidSample);
    }
    let mut values = [0.0; N];
    for (wire, ch) in setup.channels.iter().enumerate() {
        values[ch.logical_axis.ok_or(SetupError::IncompleteSetup)?] =
            sample.values[wire] * f64::from(ch.polarity.ok_or(SetupError::IncompleteSetup)?);
    }
    Ok(OrientedCounts { values })
}

/// Volatile session identity. Both counters must be freshly tracked by the future
/// acquisition owner; reconnect/reset changes connection, explicit tare changes tare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionEpoch {
    pub connection: u64,
    pub tare: u64,
}
impl SessionEpoch {
    fn validate(self) -> Result<(), SetupError> {
        if self.connection == 0 || self.tare == 0 {
            return Err(SetupError::InvalidRevision);
        }
        Ok(())
    }
}
pub struct SessionNeutral {
    profile: ProfileBinding,
    context: CurrentContext,
    epoch: SessionEpoch,
    values: [f64; N],
}
impl SessionNeutral {
    /// This stores a caller-qualified observation, not a neutral estimation algorithm.
    pub fn new(
        profile: &ProfileBinding,
        context: CurrentContext,
        epoch: SessionEpoch,
        values: [f64; N],
        quality: InputQuality,
    ) -> Result<Self, SetupError> {
        profile.check_compatibility(&context)?;
        epoch.validate()?;
        profile.evidence.require_recorded_usable()?;
        profile.setup.evidence.require_recorded_usable()?;
        if quality != InputQuality::Usable {
            return Err(SetupError::EvidenceUnusable);
        }
        if values.iter().any(|v| !v.is_finite()) {
            return Err(SetupError::InvalidSample);
        }
        if !matches!(
            context.firmware.sample_domain,
            SampleDomain::LegacyFirmwareV2 | SampleDomain::DeviceOrientedCounts
        ) {
            return Err(SetupError::DomainMismatch);
        }
        Ok(Self {
            profile: profile.clone(),
            context,
            epoch,
            values,
        })
    }
    /// A changed context or epoch cannot reuse this neutral; permanent profile
    /// parameters remain untouched and may be retained if compatibility still passes.
    pub fn check(
        &self,
        profile: &ProfileBinding,
        current: &CurrentContext,
        epoch: SessionEpoch,
    ) -> Result<&[f64; N], SetupError> {
        current.validate()?;
        epoch.validate()?;
        if profile != &self.profile || current != &self.context || epoch != self.epoch {
            return Err(SetupError::FreshNeutralRequired);
        }
        Ok(&self.values)
    }
}
