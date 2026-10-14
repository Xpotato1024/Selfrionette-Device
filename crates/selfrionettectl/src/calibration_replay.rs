//! File-only calibration/evaluation. This module never opens a serial transport.
use crate::CliError;
use selfrionette_core::calibration::{Context, Source};
use selfrionette_core::input_calibration::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum Origin {
    Synthetic,
    Recorded,
}
#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Quality {
    Usable,
    Unknown,
    Reset,
    Timeout,
    Spike,
    Clipped,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextInput {
    device_id: String,
    firmware: String,
    assembly: String,
    epoch: u64,
    user: String,
    posture: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AxisInput {
    channel: usize,
    positive_direction: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleInput {
    timestamp_ms: u32,
    epoch: u64,
    values: Vector,
    quality: Quality,
}
impl SampleInput {
    fn core(&self) -> FirmwareVectorSample {
        let quality = match self.quality {
            Quality::Usable => InputQuality::Usable,
            Quality::Unknown => InputQuality::Unknown,
            Quality::Reset => InputQuality::Reset,
            Quality::Timeout => InputQuality::Timeout,
            Quality::Spike => InputQuality::Spike,
            Quality::Clipped => InputQuality::Clipped,
        };
        FirmwareVectorSample {
            timestamp_ms: self.timestamp_ms,
            epoch: self.epoch,
            values: self.values,
            quality,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrialInput {
    id: String,
    axis: usize,
    level: f64,
    instruction: String,
    neutral: Vec<SampleInput>,
    loaded: Vec<SampleInput>,
    released: Vec<SampleInput>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LimitsInput {
    noise_floor_counts: Vector,
    max_rest_range_counts: Vector,
    max_drift_counts: Vector,
    min_signal_to_noise: f64,
    repeat_tolerance: f64,
    direction_tolerance: f64,
    max_condition_number: f64,
    max_noise_gain: f64,
    max_sample_gap_ms: u32,
}
impl LimitsInput {
    fn core(&self) -> FitLimits {
        FitLimits {
            noise_floor_counts: self.noise_floor_counts,
            max_rest_range_counts: self.max_rest_range_counts,
            max_drift_counts: self.max_drift_counts,
            min_signal_to_noise: self.min_signal_to_noise,
            repeat_tolerance: self.repeat_tolerance,
            direction_tolerance: self.direction_tolerance,
            max_condition_number: self.max_condition_number,
            max_noise_gain: self.max_noise_gain,
            max_sample_gap_ms: self.max_sample_gap_ms,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SignalInput {
    time_constant_ms: f64,
    noise_multiplier: f64,
    release_ratio: f64,
    max_deadzone_fraction: f64,
}
impl SignalInput {
    fn core(&self) -> SignalSettings {
        SignalSettings {
            time_constant_ms: self.time_constant_ms,
            noise_multiplier: self.noise_multiplier,
            release_ratio: self.release_ratio,
            max_deadzone_fraction: self.max_deadzone_fraction,
        }
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LabeledSample {
    sample: SampleInput,
    intent: Vector,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    schema_version: u32,
    recording_id: String,
    sample_domain: String,
    origin: Origin,
    context: ContextInput,
    axes: [AxisInput; N],
    limits: LimitsInput,
    signal: SignalInput,
    neutral: Vec<SampleInput>,
    trials: Vec<TrialInput>,
    replay: Vec<LabeledSample>,
}

pub(crate) fn run(args: &[String]) -> Result<(), CliError> {
    let [file] = args else {
        return Err(CliError::Usage(
            "calibrate-replay requires one recording JSON file (no serial options)".into(),
        ));
    };
    if file.starts_with('-') {
        return Err(CliError::Usage(
            "calibrate-replay accepts a file, not a port or option".into(),
        ));
    }
    let path = Path::new(file);
    let metadata = std::fs::symlink_metadata(path).map_err(failure)?;
    if !metadata.is_file() || metadata.len() > MAX_INPUT_BYTES {
        return Err(CliError::Failure(
            "recording must be a regular file no larger than 8 MiB".into(),
        ));
    }
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(failure)?
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(failure)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err(CliError::Failure("recording grew beyond 8 MiB".into()));
    }
    let input: Input = serde_json::from_slice(&bytes).map_err(failure)?;
    let report = build_report(input)?;
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, &report).map_err(failure)?;
    out.write_all(b"\n").map_err(failure)?;
    Ok(())
}
fn failure(error: impl std::fmt::Display) -> CliError {
    CliError::Failure(format!("calibration replay: {error}"))
}

fn build_report(input: Input) -> Result<Value, CliError> {
    if input.schema_version != 1
        || input.sample_domain != "firmware_v2_vector"
        || input.recording_id.trim().is_empty()
    {
        return Err(CliError::Failure(
            "unsupported schema/domain or missing recording_id".into(),
        ));
    }
    if input.replay.is_empty()
        || input.replay.len() > MAX_REPLAY_SAMPLES
        || input
            .replay
            .iter()
            .any(|s| s.intent.iter().any(|v| !v.is_finite() || v.abs() > 1.0))
    {
        return Err(CliError::Failure(
            "replay requires 1..8192 samples and finite intent in [-1,1]".into(),
        ));
    }
    let context = Context {
        source: Source {
            id: input.context.device_id.parse().map_err(failure)?,
            firmware: input.context.firmware.clone(),
            assembly: input.context.assembly.clone(),
            epoch: input.context.epoch,
        },
        user: input.context.user.clone(),
        posture: input.context.posture.clone(),
    };
    let recording = CalibrationRecording {
        context,
        axes: std::array::from_fn(|i| AxisDefinition {
            channel: input.axes[i].channel,
            positive_direction: input.axes[i].positive_direction.clone(),
        }),
        neutral: input.neutral.iter().map(SampleInput::core).collect(),
        trials: input
            .trials
            .iter()
            .map(|trial| GuidedTrial {
                id: trial.id.clone(),
                axis: trial.axis,
                level: trial.level,
                instruction: trial.instruction.clone(),
                neutral: trial.neutral.iter().map(SampleInput::core).collect(),
                loaded: trial.loaded.iter().map(SampleInput::core).collect(),
                released: trial.released.iter().map(SampleInput::core).collect(),
            })
            .collect(),
    };
    let fitted = fit(&recording, &input.limits.core()).map_err(failure)?;
    let summary = fitted.summary();
    let diagonal = evaluate(
        &fitted,
        InputMethod::Diagonal,
        input.signal.core(),
        &input.replay,
    )?;
    let coupled = evaluate(
        &fitted,
        InputMethod::Coupled,
        input.signal.core(),
        &input.replay,
    )?;
    let axes: Vec<_> = input
        .axes
        .iter()
        .map(|a| json!({"channel":a.channel,"positive_direction":a.positive_direction}))
        .collect();
    Ok(json!({
        "schema_version":1,"recording_id":input.recording_id,"origin":input.origin,
        "sample_domain":"firmware_v2_vector","result_kind":"offline_operator_adaptation",
        "hardware_verified":fitted.hardware_verified(),"control_ready":fitted.control_ready(),
        "context":{"device_id":input.context.device_id,"firmware":input.context.firmware,"assembly":input.context.assembly,
            "epoch":input.context.epoch,"user":input.context.user,"posture":input.context.posture},
        "axes":axes,
        "fit_limits":input.limits,
        "trial_summary":input.trials.iter().map(|t|json!({"id":t.id,"axis":t.axis,"level":t.level,
            "instruction":t.instruction,"neutral_samples":t.neutral.len(),"loaded_samples":t.loaded.len(),
            "released_samples":t.released.len()})).collect::<Vec<_>>(),
        "calibration":{"baseline_counts":summary.baseline_counts,"noise_envelope_counts":summary.noise_envelope_counts,
            "polarity":summary.polarity,"positive_span_counts":summary.positive_span_counts,"negative_span_counts":summary.negative_span_counts,
            "positive_repeats":summary.positive_repeats,"negative_repeats":summary.negative_repeats,"response_matrix":summary.response_matrix,
            "cross_axis_ratio":summary.cross_axis_ratio,"condition_number":summary.condition_number,"inverse_noise_gain":summary.inverse_noise_gain,
            "coupled_rejection":summary.coupled_rejection.as_ref().map(ToString::to_string)},
        "signal_settings":{"time_constant_ms":input.signal.time_constant_ms,"noise_multiplier":input.signal.noise_multiplier,
            "release_ratio":input.signal.release_ratio,"max_deadzone_fraction":input.signal.max_deadzone_fraction},
        "comparison":{"diagonal":diagonal,"coupled":coupled},
        "notice":"Offline evidence only. Instructions label intended effort, not physical force. No profile is installed, no preferred method is selected, and no hardware is accessed."
    }))
}

fn evaluate(
    fitted: &FittedInput,
    method: InputMethod,
    settings: SignalSettings,
    replay: &[LabeledSample],
) -> Result<Value, CliError> {
    let mut processor = match InputProcessor::new(fitted, method, settings) {
        Ok(processor) => processor,
        Err(error) => {
            return Ok(
                json!({"available":false,"reason":error.to_string(),"metrics":null,"trace":[]}),
            );
        }
    };
    let on = processor.activation_threshold_counts();
    let off = processor.release_threshold_counts();
    let mut trace = Vec::with_capacity(replay.len());
    let mut squared_error = 0.0;
    let mut max_abs_error: f64 = 0.0;
    let mut complete_samples = 0;
    let mut neutral_samples = 0;
    let mut multi_axis_samples = 0;
    let mut neutral_peak: f64 = 0.0;
    let mut exceeded_samples = 0;
    for (index, labeled) in replay.iter().enumerate() {
        let output = processor
            .process(fitted.context(), &labeled.sample.core())
            .map_err(|error| failure(format!("{method:?} replay sample {index}: {error}")))?;
        let complete = output.command.iter().all(Option::is_some);
        if complete {
            complete_samples += 1;
            for (actual, expected) in output.command.iter().flatten().zip(labeled.intent) {
                let error = *actual - expected;
                squared_error += error * error;
                max_abs_error = max_abs_error.max(error.abs());
            }
        }
        let intended_axes = labeled.intent.iter().filter(|v| **v != 0.0).count();
        if intended_axes == 0 {
            neutral_samples += 1;
            for value in output.command.iter().flatten() {
                neutral_peak = neutral_peak.max(value.abs());
            }
        }
        if intended_axes > 1 {
            multi_axis_samples += 1;
        }
        if output.span_exceeded.iter().any(|v| *v) {
            exceeded_samples += 1;
        }
        trace.push(json!({"timestamp_ms":output.timestamp_ms,"intent":labeled.intent,"firmware_values":output.firmware_values,
            "baseline_delta":output.baseline_delta,"estimated_axis_counts":output.estimated_axis_counts,
            "filtered_axis_counts":output.filtered_axis_counts,"normalized":output.normalized,
            "unbounded_command":output.unbounded_command,"command":output.command,"active":output.active,
            "span_exceeded":output.span_exceeded,"unsupported_direction":output.unsupported_direction}));
    }
    let all_complete = complete_samples == replay.len();
    // Never report a favourable RMSE by silently excluding unsupported directions.
    let rmse = all_complete.then(|| (squared_error / (replay.len() * N) as f64).sqrt());
    Ok(
        json!({"available":true,"sample_count":replay.len(),"complete_sample_count":complete_samples,
        "unsupported_sample_count":replay.len()-complete_samples,"neutral_sample_count":neutral_samples,
        "multi_axis_sample_count":multi_axis_samples,"span_exceeded_sample_count":exceeded_samples,
        "activation_threshold_counts":on,"release_threshold_counts":off,
        "metrics":{"command_rmse":rmse,"max_abs_command_error":all_complete.then_some(max_abs_error),
            "neutral_peak_command":(all_complete && neutral_samples>0).then_some(neutral_peak)},"trace":trace}),
    )
}
