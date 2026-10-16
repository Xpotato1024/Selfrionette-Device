use super::{
    Context, FirmwareVectorSample, FittedInput, InputError, InputMethod, Matrix, N, Vector,
    elapsed, matrix,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignalSettings {
    /// Zero explicitly disables the causal low-pass filter.
    pub time_constant_ms: f64,
    pub noise_multiplier: f64,
    /// Hysteresis release threshold / activation threshold, strictly between 0 and 1.
    pub release_ratio: f64,
    /// Refuse an unusably large deadzone instead of hiding it with a large threshold.
    pub max_deadzone_fraction: f64,
}

#[derive(Clone, Debug)]
pub struct ConditionedFrame {
    pub timestamp_ms: u32,
    pub firmware_values: Vector,
    pub baseline_delta: Vector,
    pub estimated_axis_counts: Vector,
    pub filtered_axis_counts: Vector,
    /// Before the deadzone; None means that direction's span was not measured.
    pub normalized: [Option<f64>; N],
    pub unbounded_command: [Option<f64>; N],
    pub command: [Option<f64>; N],
    /// Operation-span exceedance, NOT proof of physical ADC clipping.
    pub span_exceeded: [bool; N],
    pub unsupported_direction: [bool; N],
    pub active: [bool; N],
}

/// Explicitly session-bound and method-selected. Any sample error latches invalid.
/// Rebuilding requires a deliberate caller action; normal input never recalibrates.
pub struct InputProcessor {
    calibration: FittedInput,
    context: Context,
    session_epoch: u64,
    baseline: Vector,
    transform: Matrix,
    settings: SignalSettings,
    on: Vector,
    off: Vector,
    previous_timestamp: Option<u32>,
    filtered: Vector,
    active: [bool; N],
    has_processed_sample: bool,
    require_neutral_on_first_sample: bool,
    invalidated: bool,
}
impl InputProcessor {
    pub fn new(
        calibration: &FittedInput,
        method: InputMethod,
        settings: SignalSettings,
    ) -> Result<Self, InputError> {
        if !settings.time_constant_ms.is_finite()
            || settings.time_constant_ms < 0.0
            || !settings.noise_multiplier.is_finite()
            || settings.noise_multiplier <= 1.0
            || !settings.release_ratio.is_finite()
            || settings.release_ratio <= 0.0
            || settings.release_ratio >= 1.0
            || !settings.max_deadzone_fraction.is_finite()
            || settings.max_deadzone_fraction <= 0.0
            || settings.max_deadzone_fraction >= 1.0
        {
            return Err(InputError::InvalidSignalSettings);
        }
        let transform = calibration.transform(method)?;
        // L1 propagation gives a conservative envelope even for correlated noise.
        let on: Vector = std::array::from_fn(|axis| {
            transform[axis]
                .iter()
                .zip(calibration.summary.noise_envelope_counts)
                .map(|(coefficient, noise)| coefficient.abs() * noise)
                .sum::<f64>()
                * settings.noise_multiplier
        });
        let off = on.map(|value| value * settings.release_ratio);
        for (axis, threshold) in on.iter().enumerate() {
            let positive = calibration.summary.positive_span_counts[axis];
            let smallest = calibration.summary.negative_span_counts[axis]
                .map_or(positive, |negative| negative.min(positive));
            if !threshold.is_finite()
                || *threshold <= 0.0
                || *threshold / smallest > settings.max_deadzone_fraction
            {
                return Err(InputError::DeadzoneTooLarge(axis));
            }
        }
        Ok(Self {
            calibration: calibration.clone(),
            context: calibration.context.clone(),
            session_epoch: calibration.context.source.epoch,
            baseline: calibration.summary.baseline_counts,
            transform,
            settings,
            on,
            off,
            // Direct replay/runtime construction is still ordered after all
            // calibration observations. The app's fresh-session constructor
            // deliberately clears this after rebinding a live epoch.
            previous_timestamp: Some(calibration.last_calibration_timestamp_ms),
            filtered: [0.0; N],
            active: [false; N],
            has_processed_sample: false,
            require_neutral_on_first_sample: true,
            invalidated: false,
        })
    }

    /// Start a fresh volatile session from a compatible permanent fit and a
    /// freshly captured neutral. A reconnect/tare/settings change uses a new
    /// `current_context.source.epoch`; the fitted parameters remain reusable.
    pub fn new_for_session(
        calibration: &FittedInput,
        current_context: Context,
        neutral: &[f64; N],
        method: InputMethod,
        settings: SignalSettings,
    ) -> Result<Self, InputError> {
        let fitted = calibration.context();
        if current_context.source.id != fitted.source.id
            || current_context.source.firmware != fitted.source.firmware
            || current_context.source.assembly != fitted.source.assembly
            || current_context.user != fitted.user
            || current_context.posture != fitted.posture
            || current_context.source.epoch == 0
            || neutral.iter().any(|value| !value.is_finite())
        {
            return Err(InputError::InvalidContext);
        }
        let mut processor = Self::new(calibration, method, settings)?;
        processor.session_epoch = current_context.source.epoch;
        processor.context = current_context;
        processor.baseline = *neutral;
        processor.previous_timestamp = None;
        processor.filtered = [0.0; N];
        processor.active = [false; N];
        processor.has_processed_sample = false;
        processor.require_neutral_on_first_sample = true;
        processor.invalidated = false;
        Ok(processor)
    }

    /// Begin a new explicitly captured verification phase after the initial
    /// neutral window has been processed. The fitted transform and fresh
    /// session baseline stay fixed; only filter/timebase state is reset so
    /// operator pauses between bounded evidence windows do not fabricate a
    /// sample-gap failure. Live stream processing should never call this.
    pub fn begin_captured_window(&mut self) -> Result<(), InputError> {
        if self.invalidated {
            return Err(InputError::Invalidated);
        }
        if !self.has_processed_sample {
            return Err(InputError::NeutralRequired);
        }
        self.previous_timestamp = None;
        self.filtered = [0.0; N];
        self.active = [false; N];
        self.require_neutral_on_first_sample = false;
        Ok(())
    }

    pub fn activation_threshold_counts(&self) -> Vector {
        self.on
    }
    pub fn release_threshold_counts(&self) -> Vector {
        self.off
    }
    pub fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    pub fn process(
        &mut self,
        context: &Context,
        sample: &FirmwareVectorSample,
    ) -> Result<ConditionedFrame, InputError> {
        if self.invalidated {
            return Err(InputError::Invalidated);
        }
        let result = self.process_checked(context, sample);
        if result.is_err() {
            self.invalidated = true;
            self.previous_timestamp = None;
            self.filtered = [0.0; N];
            self.active = [false; N];
            self.has_processed_sample = false;
        }
        result
    }

    fn process_checked(
        &mut self,
        context: &Context,
        sample: &FirmwareVectorSample,
    ) -> Result<ConditionedFrame, InputError> {
        if context != &self.context {
            return Err(InputError::InvalidContext);
        }
        if sample.epoch != self.session_epoch {
            return Err(InputError::EpochMismatch);
        }
        if sample.quality != super::InputQuality::Usable {
            return Err(InputError::QualityUnusable);
        }
        if sample.values.iter().any(|value| !value.is_finite()) {
            return Err(InputError::NonFinite);
        }
        if let Some(previous) = self.previous_timestamp {
            let dt = elapsed(previous, sample.timestamp_ms)?;
            if dt > self.calibration.max_sample_gap_ms {
                return Err(InputError::SampleGap);
            }
        }
        let delta: Vector = std::array::from_fn(|ch| sample.values[ch] - self.baseline[ch]);
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(InputError::NonFinite);
        }
        let estimated = matrix::apply(&self.transform, &delta)?;
        if !self.has_processed_sample
            && self.require_neutral_on_first_sample
            && (0..N).any(|axis| estimated[axis].abs() > self.on[axis])
        {
            return Err(InputError::NeutralRequired);
        }
        let filtered = match self.previous_timestamp {
            None => estimated,
            Some(previous) => {
                let dt = elapsed(previous, sample.timestamp_ms)?;
                let alpha = if self.settings.time_constant_ms == 0.0 {
                    1.0
                } else {
                    -(-f64::from(dt) / self.settings.time_constant_ms).exp_m1()
                };
                std::array::from_fn(|axis| {
                    alpha * estimated[axis] + (1.0 - alpha) * self.filtered[axis]
                })
            }
        };
        if filtered.iter().any(|v| !v.is_finite()) {
            return Err(InputError::NonFinite);
        }
        let mut normalized = [None; N];
        let mut unbounded = [None; N];
        let mut active = self.active;
        let mut unsupported = [false; N];
        let mut exceeded = [false; N];
        for axis in 0..N {
            let value = filtered[axis];
            let span = if value >= 0.0 {
                Some(self.calibration.summary.positive_span_counts[axis])
            } else {
                self.calibration.summary.negative_span_counts[axis]
            };
            normalized[axis] = span.map(|s| value / s);
            if normalized[axis].is_some_and(|v| !v.is_finite()) {
                return Err(InputError::NonFinite);
            }
            exceeded[axis] = normalized[axis].is_some_and(|v| v.abs() > 1.0);
            let magnitude = value.abs();
            if active[axis] {
                if magnitude <= self.off[axis] {
                    active[axis] = false;
                }
            } else if magnitude > self.on[axis] {
                active[axis] = true;
            }
            if !active[axis] {
                // Neutral classification is not an invented reverse gain.
                unbounded[axis] = Some(0.0);
            } else if let Some(span) = span {
                let command = value.signum() * (magnitude - self.off[axis]).max(0.0)
                    / (span - self.off[axis]);
                if !command.is_finite() {
                    return Err(InputError::NonFinite);
                }
                unbounded[axis] = Some(command);
            } else {
                unsupported[axis] = true;
            }
        }
        self.filtered = filtered;
        self.previous_timestamp = Some(sample.timestamp_ms);
        self.has_processed_sample = true;
        self.require_neutral_on_first_sample = false;
        self.active = active;
        Ok(ConditionedFrame {
            timestamp_ms: sample.timestamp_ms,
            firmware_values: sample.values,
            baseline_delta: delta,
            estimated_axis_counts: estimated,
            filtered_axis_counts: filtered,
            normalized,
            unbounded_command: unbounded,
            command: unbounded.map(|v| v.map(|v| v.clamp(-1.0, 1.0))),
            span_exceeded: exceeded,
            unsupported_direction: unsupported,
            active,
        })
    }
}
