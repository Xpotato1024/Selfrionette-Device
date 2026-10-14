//! Offline, evidence-bound operator adaptation of firmware-v2 vectors.
//!
//! This is NOT an ADC/Newton calibration, a persistent device setting, or a
//! robot mapping. The firmware has already negated/tared/filtered these values.
//! Usable evidence is supplied by the caller; ordinary v2 traffic does not prove it.
mod matrix;
mod signal;

use crate::CHANNEL_COUNT;
use crate::calibration::Context;
pub use signal::{ConditionedFrame, InputProcessor, SignalSettings};
use std::fmt;

pub const N: usize = CHANNEL_COUNT;
pub type Vector = [f64; N];
pub type Matrix = [[f64; N]; N];
pub const MAX_PHASE_SAMPLES: usize = 1024;
pub const MAX_CALIBRATION_SAMPLES: usize = 16384;
pub const MAX_REPLAY_SAMPLES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputQuality {
    Usable,
    Unknown,
    Reset,
    Timeout,
    Spike,
    Clipped,
}

#[derive(Clone, Debug)]
pub struct FirmwareVectorSample {
    pub timestamp_ms: u32,
    pub epoch: u64,
    pub values: Vector,
    pub quality: InputQuality,
}

#[derive(Clone, Debug)]
pub struct AxisDefinition {
    /// Explicit wire channel association, not inferred from response magnitude.
    pub channel: usize,
    pub positive_direction: String,
}

#[derive(Clone, Debug)]
pub struct GuidedTrial {
    pub id: String,
    pub axis: usize,
    /// Signed, instructed comfortable effort; not a known physical load.
    pub level: f64,
    pub instruction: String,
    pub neutral: Vec<FirmwareVectorSample>,
    pub loaded: Vec<FirmwareVectorSample>,
    pub released: Vec<FirmwareVectorSample>,
}

#[derive(Clone, Debug)]
pub struct CalibrationRecording {
    pub context: Context,
    pub axes: [AxisDefinition; N],
    /// Natural operating posture with no intended command, NOT unloaded ADC tare.
    pub neutral: Vec<FirmwareVectorSample>,
    pub trials: Vec<GuidedTrial>,
}

/// No universal measured thresholds or operating spans are provided as defaults.
#[derive(Clone, Debug)]
pub struct FitLimits {
    pub noise_floor_counts: Vector,
    pub max_rest_range_counts: Vector,
    pub max_drift_counts: Vector,
    pub min_signal_to_noise: f64,
    pub repeat_tolerance: f64,
    pub direction_tolerance: f64,
    pub max_condition_number: f64,
    pub max_noise_gain: f64,
    pub max_sample_gap_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMethod {
    Diagonal,
    Coupled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputError {
    InvalidContext,
    InvalidAxes,
    Bounds,
    InvalidLimits,
    NonFinite,
    QualityUnusable,
    EpochMismatch,
    TimestampOrder,
    SampleGap,
    UnstableRest,
    BaselineDrift,
    MissingInstruction,
    IncompleteRepeats(usize),
    WeakSignal(usize),
    PolarityDisagreement(usize),
    RepeatDisagreement(usize),
    CoupledUnavailable(&'static str),
    InvalidSignalSettings,
    DeadzoneTooLarge(usize),
    NeutralRequired,
    Invalidated,
}
impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for InputError {}

#[derive(Clone, Debug)]
pub struct CalibrationSummary {
    pub baseline_counts: Vector,
    pub noise_envelope_counts: Vector,
    pub polarity: [i8; N],
    pub positive_span_counts: Vector,
    pub negative_span_counts: [Option<f64>; N],
    pub positive_repeats: [usize; N],
    pub negative_repeats: [usize; N],
    /// Rows = original channels; columns = instructed axes in primary-count units.
    pub response_matrix: Matrix,
    pub cross_axis_ratio: Vector,
    pub condition_number: Option<f64>,
    pub inverse_noise_gain: Option<f64>,
    pub coupled_rejection: Option<InputError>,
}

#[derive(Clone, Debug)]
pub struct FittedInput {
    context: Context,
    axes: [AxisDefinition; N],
    summary: CalibrationSummary,
    coupled_inverse: Option<Matrix>,
    last_calibration_timestamp_ms: u32,
    max_sample_gap_ms: u32,
}
impl FittedInput {
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn axes(&self) -> &[AxisDefinition; N] {
        &self.axes
    }
    pub fn summary(&self) -> &CalibrationSummary {
        &self.summary
    }
    pub const fn hardware_verified(&self) -> bool {
        false
    }
    pub const fn control_ready(&self) -> bool {
        false
    }
    fn transform(&self, method: InputMethod) -> Result<Matrix, InputError> {
        match method {
            InputMethod::Diagonal => {
                let mut result = [[0.0; N]; N];
                for (axis, row) in result.iter_mut().enumerate() {
                    row[self.axes[axis].channel] = f64::from(self.summary.polarity[axis]);
                }
                Ok(result)
            }
            InputMethod::Coupled => self.coupled_inverse.ok_or_else(|| {
                self.summary
                    .coupled_rejection
                    .clone()
                    .unwrap_or(InputError::CoupledUnavailable("not fitted"))
            }),
        }
    }
}

pub(super) fn elapsed(previous: u32, current: u32) -> Result<u32, InputError> {
    let dt = current.wrapping_sub(previous);
    if dt == 0 || dt >= (1_u32 << 31) {
        Err(InputError::TimestampOrder)
    } else {
        Ok(dt)
    }
}

fn validate_sample(sample: &FirmwareVectorSample, epoch: u64) -> Result<(), InputError> {
    if sample.epoch != epoch {
        return Err(InputError::EpochMismatch);
    }
    if sample.quality != InputQuality::Usable {
        return Err(InputError::QualityUnusable);
    }
    if sample.values.iter().any(|v| !v.is_finite()) {
        return Err(InputError::NonFinite);
    }
    Ok(())
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        values[mid - 1] / 2.0 + values[mid] / 2.0
    }
}

fn center(samples: &[FirmwareVectorSample]) -> Vector {
    std::array::from_fn(|ch| median(samples.iter().map(|s| s.values[ch]).collect()))
}

fn check_window(
    samples: &[FirmwareVectorSample],
    context: &Context,
    limits: &FitLimits,
    last: &mut Option<u32>,
    rest: bool,
) -> Result<Vector, InputError> {
    if !(8..=MAX_PHASE_SAMPLES).contains(&samples.len()) {
        return Err(InputError::Bounds);
    }
    for (index, sample) in samples.iter().enumerate() {
        validate_sample(sample, context.source.epoch)?;
        if let Some(previous) = *last {
            let dt = elapsed(previous, sample.timestamp_ms)?;
            // An instruction can take time between windows, but not within a capture.
            if index > 0 && dt > limits.max_sample_gap_ms {
                return Err(InputError::SampleGap);
            }
        }
        *last = Some(sample.timestamp_ms);
    }
    if rest {
        for ch in 0..N {
            let lo = samples
                .iter()
                .map(|s| s.values[ch])
                .fold(f64::INFINITY, f64::min);
            let hi = samples
                .iter()
                .map(|s| s.values[ch])
                .fold(f64::NEG_INFINITY, f64::max);
            if hi - lo > limits.max_rest_range_counts[ch] {
                return Err(InputError::UnstableRest);
            }
        }
    }
    Ok(center(samples))
}

fn validate_limits(l: &FitLimits) -> Result<(), InputError> {
    if [
        &l.noise_floor_counts,
        &l.max_rest_range_counts,
        &l.max_drift_counts,
    ]
    .iter()
    .flat_map(|a| a.iter())
    .any(|v| !v.is_finite() || *v <= 0.0)
        || !l.min_signal_to_noise.is_finite()
        || l.min_signal_to_noise <= 1.0
        || !l.repeat_tolerance.is_finite()
        || !(0.0..1.0).contains(&l.repeat_tolerance)
        || l.repeat_tolerance == 0.0
        || !l.direction_tolerance.is_finite()
        || !(0.0..1.0).contains(&l.direction_tolerance)
        || l.direction_tolerance == 0.0
        || !l.max_condition_number.is_finite()
        || l.max_condition_number < 1.0
        || !l.max_noise_gain.is_finite()
        || l.max_noise_gain < 1.0
        || l.max_sample_gap_ms == 0
        || l.max_sample_gap_ms >= (1_u32 << 31)
    {
        return Err(InputError::InvalidLimits);
    }
    Ok(())
}

struct Response {
    axis: usize,
    level: f64,
    slope: Vector,
}

fn combine(
    responses: &[&Response],
    axis: usize,
    channel: usize,
    tolerance: f64,
) -> Result<(Vector, f64, i8), InputError> {
    if !(2..=8).contains(&responses.len()) {
        return Err(InputError::IncompleteRepeats(axis));
    }
    let combined: Vector =
        std::array::from_fn(|ch| median(responses.iter().map(|r| r.slope[ch]).collect()));
    let span = combined[channel].abs();
    let sign = if combined[channel] > 0.0 { 1 } else { -1 };
    if !span.is_finite() || span == 0.0 {
        return Err(InputError::WeakSignal(axis));
    }
    for response in responses {
        if (response.slope[channel] > 0.0) != (sign > 0) {
            return Err(InputError::PolarityDisagreement(axis));
        }
        if (0..N).any(|ch| (response.slope[ch] - combined[ch]).abs() / span > tolerance) {
            return Err(InputError::RepeatDisagreement(axis));
        }
    }
    let direction = combined.map(|v| v / span);
    if direction.iter().any(|v| !v.is_finite()) {
        return Err(InputError::NonFinite);
    }
    Ok((direction, span, sign))
}

pub fn fit(
    recording: &CalibrationRecording,
    limits: &FitLimits,
) -> Result<FittedInput, InputError> {
    crate::calibration::validate_context(&recording.context)
        .map_err(|_| InputError::InvalidContext)?;
    validate_limits(limits)?;
    let mut seen_channels = [false; N];
    for axis in &recording.axes {
        if axis.channel >= N
            || seen_channels[axis.channel]
            || axis.positive_direction.trim().is_empty()
            || axis.positive_direction.len() > 256
        {
            return Err(InputError::InvalidAxes);
        }
        seen_channels[axis.channel] = true;
    }
    if recording.trials.len() > 112 {
        return Err(InputError::Bounds);
    }
    let total = recording
        .trials
        .iter()
        .try_fold(recording.neutral.len(), |n, t| {
            n.checked_add(t.neutral.len())?
                .checked_add(t.loaded.len())?
                .checked_add(t.released.len())
        })
        .ok_or(InputError::Bounds)?;
    if total > MAX_CALIBRATION_SAMPLES {
        return Err(InputError::Bounds);
    }
    let mut last = None;
    let baseline = check_window(
        &recording.neutral,
        &recording.context,
        limits,
        &mut last,
        true,
    )?;
    let mut noise = limits.noise_floor_counts;
    for sample in &recording.neutral {
        for ch in 0..N {
            noise[ch] = noise[ch].max((sample.values[ch] - baseline[ch]).abs());
        }
    }
    let mut responses = Vec::with_capacity(recording.trials.len());
    let mut ids = std::collections::HashSet::new();
    for trial in &recording.trials {
        if trial.axis >= N
            || !trial.level.is_finite()
            || trial.level == 0.0
            || trial.level.abs() > 1.0
        {
            return Err(InputError::Bounds);
        }
        if trial.instruction.trim().is_empty()
            || trial.id.trim().is_empty()
            || !ids.insert(&trial.id)
        {
            return Err(InputError::MissingInstruction);
        }
        let before = check_window(&trial.neutral, &recording.context, limits, &mut last, true)?;
        let loaded = check_window(&trial.loaded, &recording.context, limits, &mut last, false)?;
        let released = check_window(&trial.released, &recording.context, limits, &mut last, true)?;
        for ch in 0..N {
            if (before[ch] - baseline[ch]).abs() > limits.max_drift_counts[ch]
                || (released[ch] - baseline[ch]).abs() > limits.max_drift_counts[ch]
                || (released[ch] - before[ch]).abs() > limits.max_drift_counts[ch]
            {
                return Err(InputError::BaselineDrift);
            }
            for sample in trial.neutral.iter().chain(&trial.released) {
                noise[ch] = noise[ch].max((sample.values[ch] - baseline[ch]).abs());
            }
        }
        let delta: Vector = std::array::from_fn(|ch| loaded[ch] - before[ch]);
        let primary = delta[recording.axes[trial.axis].channel].abs();
        for ch in 0..N {
            let lo = trial
                .loaded
                .iter()
                .map(|s| s.values[ch])
                .fold(f64::INFINITY, f64::min);
            let hi = trial
                .loaded
                .iter()
                .map(|s| s.values[ch])
                .fold(f64::NEG_INFINITY, f64::max);
            if hi - lo > limits.max_rest_range_counts[ch] + limits.repeat_tolerance * primary {
                return Err(InputError::RepeatDisagreement(trial.axis));
            }
        }
        let slope = delta.map(|v| v / trial.level);
        if slope.iter().any(|v| !v.is_finite()) {
            return Err(InputError::NonFinite);
        }
        responses.push(Response {
            axis: trial.axis,
            level: trial.level,
            slope,
        });
    }
    for response in &responses {
        let ch = recording.axes[response.axis].channel;
        if (response.slope[ch] * response.level).abs() <= noise[ch] * limits.min_signal_to_noise {
            return Err(InputError::WeakSignal(response.axis));
        }
    }
    let mut summary = CalibrationSummary {
        baseline_counts: baseline,
        noise_envelope_counts: noise,
        polarity: [0; N],
        positive_span_counts: [0.0; N],
        negative_span_counts: [None; N],
        positive_repeats: [0; N],
        negative_repeats: [0; N],
        response_matrix: [[0.0; N]; N],
        cross_axis_ratio: [0.0; N],
        condition_number: None,
        inverse_noise_gain: None,
        coupled_rejection: None,
    };
    for axis in 0..N {
        let positive: Vec<_> = responses
            .iter()
            .filter(|r| r.axis == axis && r.level > 0.0)
            .collect();
        let negative: Vec<_> = responses
            .iter()
            .filter(|r| r.axis == axis && r.level < 0.0)
            .collect();
        let channel = recording.axes[axis].channel;
        let (direction, span, sign) = combine(&positive, axis, channel, limits.repeat_tolerance)?;
        summary.polarity[axis] = sign;
        summary.positive_span_counts[axis] = span;
        summary.positive_repeats[axis] = positive.len();
        summary.negative_repeats[axis] = negative.len();
        for (ch, value) in direction.iter().enumerate() {
            summary.response_matrix[ch][axis] = *value;
        }
        summary.cross_axis_ratio[axis] = direction
            .iter()
            .enumerate()
            .filter(|(ch, _)| *ch != channel)
            .map(|(_, v)| v.abs())
            .fold(0.0, f64::max);
        if !negative.is_empty() {
            let (reverse_direction, reverse_span, reverse_sign) =
                combine(&negative, axis, channel, limits.repeat_tolerance)?;
            if reverse_sign != sign {
                return Err(InputError::PolarityDisagreement(axis));
            }
            summary.negative_span_counts[axis] = Some(reverse_span);
            if (0..N).any(|ch| {
                (reverse_direction[ch] - direction[ch]).abs() > limits.direction_tolerance
            }) {
                summary.coupled_rejection = Some(InputError::CoupledUnavailable(
                    "direction-dependent response",
                ));
            }
        }
    }
    let mut coupled_inverse = None;
    match matrix::inverse(&summary.response_matrix) {
        Err(error) => {
            summary.coupled_rejection.get_or_insert(error);
        }
        Ok(inverse) => {
            let gain = matrix::norm(&inverse);
            let condition = matrix::norm(&summary.response_matrix) * gain;
            if !condition.is_finite() || !gain.is_finite() {
                summary.coupled_rejection = Some(InputError::NonFinite);
            } else {
                summary.condition_number = Some(condition);
                summary.inverse_noise_gain = Some(gain);
                if condition > limits.max_condition_number {
                    summary.coupled_rejection =
                        Some(InputError::CoupledUnavailable("condition-number limit"));
                } else if gain > limits.max_noise_gain {
                    summary.coupled_rejection =
                        Some(InputError::CoupledUnavailable("noise-gain limit"));
                }
                if summary.coupled_rejection.is_none() {
                    coupled_inverse = Some(inverse);
                }
            }
        }
    }
    Ok(FittedInput {
        context: recording.context.clone(),
        axes: recording.axes.clone(),
        summary,
        coupled_inverse,
        last_calibration_timestamp_ms: last.ok_or(InputError::Bounds)?,
        max_sample_gap_ms: limits.max_sample_gap_ms,
    })
}
