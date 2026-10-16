use super::{
    Context, FirmwareVectorSample, FittedInput, InputError, InputMethod, Matrix, N, Vector,
    elapsed, matrix,
};

#[derive(Clone, Copy, Debug)]
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
pub struct InputProcessor<'a> {
    calibration: &'a FittedInput,
    transform: Matrix,
    settings: SignalSettings,
    on: Vector,
    off: Vector,
    previous_timestamp: Option<u32>,
    filtered: Vector,
    active: [bool; N],
    invalidated: bool,
}
impl<'a> InputProcessor<'a> {
    pub fn new(
        calibration: &'a FittedInput,
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
            calibration,
            transform,
            settings,
            on,
            off,
            previous_timestamp: None,
            filtered: [0.0; N],
            active: [false; N],
            invalidated: false,
        })
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
        }
        result
    }

    fn process_checked(
        &mut self,
        context: &Context,
        sample: &FirmwareVectorSample,
    ) -> Result<ConditionedFrame, InputError> {
        if context != &self.calibration.context {
            return Err(InputError::InvalidContext);
        }
        self.calibration
            .validate_replay_sample(self.previous_timestamp, sample)?;
        let delta: Vector = std::array::from_fn(|ch| {
            sample.values[ch] - self.calibration.summary.baseline_counts[ch]
        });
        if delta.iter().any(|v| !v.is_finite()) {
            return Err(InputError::NonFinite);
        }
        let estimated = matrix::apply(&self.transform, &delta)?;
        let filtered = match self.previous_timestamp {
            None => {
                // Calibration observations cannot be replayed as held-out validation.
                if (0..N).any(|axis| estimated[axis].abs() > self.on[axis]) {
                    return Err(InputError::NeutralRequired);
                }
                estimated
            }
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
