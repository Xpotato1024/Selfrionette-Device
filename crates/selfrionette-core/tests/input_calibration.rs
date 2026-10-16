use selfrionette_core::calibration::{Context, Source};
use selfrionette_core::input_calibration::*;

fn limits() -> FitLimits {
    FitLimits {
        noise_floor_counts: [0.2; N],
        max_rest_range_counts: [1.0; N],
        max_drift_counts: [2.0; N],
        min_signal_to_noise: 5.0,
        repeat_tolerance: 0.05,
        direction_tolerance: 0.05,
        max_condition_number: 20.0,
        max_noise_gain: 10.0,
        max_sample_gap_ms: 100,
    }
}
fn settings() -> SignalSettings {
    SignalSettings {
        time_constant_ms: 0.0,
        noise_multiplier: 3.0,
        release_ratio: 0.5,
        max_deadzone_fraction: 0.1,
    }
}
fn identity() -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 1.0 } else { 0.0 }))
}
fn mixing() -> Matrix {
    let mut a = identity();
    a[1][1] = -1.0;
    a[0][1] = 0.3;
    a[1][0] = -0.2;
    a[5][4] = 0.15;
    a
}
fn context() -> Context {
    Context {
        source: Source {
            id: "srn-0123456789abcdef0123456789abcdef".parse().unwrap(),
            firmware: "synthetic-input-test".into(),
            assembly: "fixture-1".into(),
            epoch: 42,
        },
        user: "test-operator".into(),
        posture: "test-posture".into(),
    }
}
fn sample(t: u32, values: Vector) -> FirmwareVectorSample {
    FirmwareVectorSample {
        timestamp_ms: t,
        epoch: 42,
        values,
        quality: InputQuality::Usable,
    }
}
fn window(t: &mut u32, values: Vector) -> Vec<FirmwareVectorSample> {
    (0..8)
        .map(|i| {
            *t = t.wrapping_add(10);
            sample(*t, values.map(|v| v + if i % 2 == 0 { -0.1 } else { 0.1 }))
        })
        .collect()
}
fn values(a: &Matrix, intent: Vector) -> Vector {
    std::array::from_fn(|ch| {
        10.0 + (0..N)
            .map(|axis| {
                let span = if intent[axis] >= 0.0 {
                    1000.0 + axis as f64 * 100.0
                } else {
                    600.0 + axis as f64 * 50.0
                };
                a[ch][axis] * span * intent[axis]
            })
            .sum::<f64>()
    })
}
fn recording(a: Matrix, reverse: bool, start: u32) -> (CalibrationRecording, u32) {
    let mut t = start;
    let neutral = window(&mut t, [10.0; N]);
    let mut trials = Vec::new();
    for axis in 0..N {
        for sign in if reverse { vec![1.0, -1.0] } else { vec![1.0] } {
            for repeat in 0..2 {
                let level = sign * if repeat == 0 { 1.0 } else { 0.5 };
                let mut intent = [0.0; N];
                intent[axis] = level;
                trials.push(GuidedTrial {
                    id: format!("axis-{axis}-{sign}-{repeat}"),
                    axis,
                    level,
                    instruction: format!("instructed axis {axis} effort {level}"),
                    neutral: window(&mut t, [10.0; N]),
                    loaded: window(&mut t, values(&a, intent)),
                    released: window(&mut t, [10.0; N]),
                });
            }
        }
    }
    (
        CalibrationRecording {
            context: context(),
            axes: std::array::from_fn(|channel| AxisDefinition {
                channel,
                positive_direction: format!("defined positive direction {channel}"),
            }),
            neutral,
            trials,
        },
        t,
    )
}
fn approx(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn fits_signed_spans_and_never_claims_hardware_or_control_readiness() {
    let (r, _) = recording(mixing(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    assert_eq!(fit.summary().polarity, [1, -1, 1, 1, 1, 1, 1]);
    assert_eq!(fit.summary().positive_repeats, [2; N]);
    assert_eq!(fit.summary().negative_repeats, [2; N]);
    for axis in 0..N {
        approx(
            fit.summary().positive_span_counts[axis],
            1000.0 + axis as f64 * 100.0,
        );
        approx(
            fit.summary().negative_span_counts[axis].unwrap(),
            600.0 + axis as f64 * 50.0,
        );
    }
    assert!(!fit.hardware_verified() && !fit.control_ready());
    assert!(fit.summary().condition_number.unwrap() > 1.0);
}

#[test]
fn matrix_recovers_held_out_simultaneous_input_not_only_training_directions() {
    let a = mixing();
    let (r, t) = recording(a, true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let mut coupled = InputProcessor::new(&fit, InputMethod::Coupled, settings()).unwrap();
    let mut diagonal = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    for p in [&mut coupled, &mut diagonal] {
        p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
    }
    let target = [0.4, -0.7, 0.25, -0.3, 0.55, 0.2, -0.15];
    let raw = sample(t + 20, values(&a, target));
    let c = coupled.process(&r.context, &raw).unwrap();
    let d = diagonal.process(&r.context, &raw).unwrap();
    for (axis, expected) in target.iter().enumerate() {
        approx(c.normalized[axis].unwrap(), *expected);
    }
    assert!((d.normalized[0].unwrap() - target[0]).abs() > 0.1);
    assert_eq!(c.firmware_values, raw.values);
}

#[test]
fn noise_stays_neutral_and_does_not_recalibrate_the_profile() {
    let (r, t) = recording(mixing(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let baseline = fit.summary().baseline_counts;
    let mut p = InputProcessor::new(&fit, InputMethod::Coupled, settings()).unwrap();
    for i in 1..50 {
        let frame = p
            .process(
                &r.context,
                &sample(t + i * 10, [10.0 + if i % 2 == 0 { 0.1 } else { -0.1 }; N]),
            )
            .unwrap();
        assert_eq!(frame.command, [Some(0.0); N]);
    }
    assert_eq!(fit.summary().baseline_counts, baseline);
}

#[test]
fn negative_gain_is_not_invented_for_a_one_sided_axis() {
    let a = identity();
    let (r, t) = recording(a, false, 0);
    let fit = fit(&r, &limits()).unwrap();
    assert_eq!(fit.summary().negative_span_counts, [None; N]);
    let mut p = InputProcessor::new(&fit, InputMethod::Coupled, settings()).unwrap();
    p.process(&r.context, &sample(t + 10, [9.9; N])).unwrap();
    let mut target = [0.0; N];
    target[0] = -0.5;
    let output = p
        .process(&r.context, &sample(t + 20, values(&a, target)))
        .unwrap();
    assert_eq!(output.normalized[0], None);
    assert_eq!(output.command[0], None);
    assert!(output.unsupported_direction[0]);
    assert_eq!(output.command[1], Some(0.0));
}

#[test]
fn operation_span_exceedance_is_logged_before_output_clamping() {
    let a = identity();
    let (r, t) = recording(a, true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
    let output = p
        .process(&r.context, &sample(t + 20, values(&a, [2.0; N])))
        .unwrap();
    assert_eq!(output.command, [Some(1.0); N]);
    assert!(output.unbounded_command[0].unwrap() > 1.0);
    assert_eq!(output.span_exceeded, [true; N]);
    approx(output.normalized[0].unwrap(), 2.0);
}

#[test]
fn all_bad_quality_kinds_latch_invalid_without_reusing_old_output() {
    let (r, t) = recording(identity(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    for quality in [
        InputQuality::Unknown,
        InputQuality::Timeout,
        InputQuality::Spike,
        InputQuality::Clipped,
        InputQuality::Reset,
    ] {
        let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
        p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
        let mut bad = sample(t + 20, [10.0; N]);
        bad.quality = quality;
        assert_eq!(
            p.process(&r.context, &bad).unwrap_err(),
            InputError::QualityUnusable
        );
        assert!(p.is_invalidated());
        assert_eq!(
            p.process(&r.context, &sample(t + 30, [10.0; N]))
                .unwrap_err(),
            InputError::Invalidated
        );
    }
}

#[test]
fn context_epoch_timestamp_and_nan_fail_closed() {
    let (r, t) = recording(identity(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    for kind in 0..5 {
        let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
        p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
        let mut s = sample(t + 20, [10.0; N]);
        let mut c = r.context.clone();
        match kind {
            0 => s.epoch += 1,
            1 => s.timestamp_ms = t + 10,
            2 => s.timestamp_ms = t + 500,
            3 => s.values[0] = f64::NAN,
            _ => c.user = "another-user".into(),
        };
        assert!(p.process(&c, &s).is_err());
        assert!(p.is_invalidated());
    }
}

#[test]
fn replay_requires_a_new_neutral_observation_and_rejects_training_reuse() {
    let (r, t) = recording(identity(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    assert_eq!(
        p.process(&r.context, &sample(t, [10.0; N])).unwrap_err(),
        InputError::TimestampOrder
    );
    let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    assert_eq!(
        p.process(&r.context, &sample(t + 10, [500.0; N]))
            .unwrap_err(),
        InputError::NeutralRequired
    );
}

#[test]
fn low_pass_uses_elapsed_time_and_releases_to_zero() {
    let a = identity();
    let (r, t) = recording(a, true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let s = SignalSettings {
        time_constant_ms: 100.0,
        ..settings()
    };
    let mut one = InputProcessor::new(&fit, InputMethod::Diagonal, s).unwrap();
    let mut two = InputProcessor::new(&fit, InputMethod::Diagonal, s).unwrap();
    for p in [&mut one, &mut two] {
        p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
    }
    let full = values(&a, [1.0; N]);
    let once = one.process(&r.context, &sample(t + 40, full)).unwrap();
    two.process(&r.context, &sample(t + 20, full)).unwrap();
    let twice = two.process(&r.context, &sample(t + 40, full)).unwrap();
    approx(
        once.filtered_axis_counts[0],
        1000.0 * (1.0 - (-0.3_f64).exp()),
    );
    approx(once.filtered_axis_counts[0], twice.filtered_axis_counts[0]);
    let mut output = twice;
    for i in 1..100 {
        output = two
            .process(&r.context, &sample(t + 40 + i * 10, [10.0; N]))
            .unwrap();
    }
    assert_eq!(output.command, [Some(0.0); N]);
    assert_eq!(output.active, [false; N]);
}

#[test]
fn hysteresis_has_separate_activation_and_release_thresholds() {
    let (r, t) = recording(identity(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    let on = p.activation_threshold_counts()[0];
    let off = p.release_threshold_counts()[0];
    assert!(off < on);
    p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
    for (index, (factor, expected)) in [(2.0, true), (0.75, true), (0.4, false)]
        .into_iter()
        .enumerate()
    {
        let mut value = [10.0; N];
        value[0] += on * factor;
        assert_eq!(
            p.process(&r.context, &sample(t + 20 + index as u32 * 10, value))
                .unwrap()
                .active[0],
            expected
        );
    }
}

#[test]
fn firmware_u32_wrap_is_forward_progress_not_a_reset() {
    let (r, t) = recording(identity(), true, u32::MAX - 100);
    let fit = fit(&r, &limits()).unwrap();
    let mut p = InputProcessor::new(&fit, InputMethod::Diagonal, settings()).unwrap();
    p.process(&r.context, &sample(t.wrapping_add(10), [10.0; N]))
        .unwrap();
}

#[test]
fn singular_or_ill_conditioned_response_never_falls_back_when_coupled_requested() {
    for coupling in [1.0, 0.999] {
        let mut a = identity();
        a[0][1] = coupling;
        a[1][0] = coupling;
        let (r, _) = recording(a, true, 0);
        let fit = fit(&r, &limits()).unwrap();
        assert!(fit.summary().coupled_rejection.is_some());
        assert!(InputProcessor::new(&fit, InputMethod::Coupled, settings()).is_err());
        assert!(InputProcessor::new(&fit, InputMethod::Diagonal, settings()).is_ok());
    }
}

#[test]
fn inverse_noise_gain_has_its_own_gate() {
    let mut a = identity();
    a[0][1] = 0.9;
    a[1][0] = 0.9;
    let (r, _) = recording(a, true, 0);
    let fit = fit(
        &r,
        &FitLimits {
            max_condition_number: 1000.0,
            max_noise_gain: 5.0,
            ..limits()
        },
    )
    .unwrap();
    assert_eq!(
        fit.summary().coupled_rejection,
        Some(InputError::CoupledUnavailable("noise-gain limit"))
    );
}

#[test]
fn reverse_direction_shape_mismatch_rejects_only_coupled_candidate() {
    let (mut r, _) = recording(identity(), true, 0);
    for trial in &mut r.trials {
        if trial.axis == 0 && trial.level < 0.0 {
            for frame in &mut trial.loaded {
                frame.values[1] += 200.0 * trial.level;
            }
        }
    }
    let fit = fit(&r, &limits()).unwrap();
    assert_eq!(
        fit.summary().coupled_rejection,
        Some(InputError::CoupledUnavailable(
            "direction-dependent response"
        ))
    );
    assert!(InputProcessor::new(&fit, InputMethod::Diagonal, settings()).is_ok());
}

#[test]
fn incomplete_direction_quality_drift_and_inconsistent_repeats_are_not_approved() {
    for kind in 0..8 {
        let (mut r, _) = recording(identity(), true, 0);
        match kind {
            0 => r.trials.retain(|t| t.axis != 0 || t.level < 0.0),
            1 => r.trials[0].instruction.clear(),
            2 => r.neutral[0].quality = InputQuality::Unknown,
            3 => {
                for f in &mut r.trials[0].released {
                    f.values[0] += 10.0;
                }
            }
            4 => {
                for f in &mut r.trials[0].loaded {
                    f.values[0] += 500.0;
                }
            }
            5 => r.axes[1].channel = 0,
            6 => r.neutral[0].values[0] = f64::INFINITY,
            _ => r.trials[1].id = r.trials[0].id.clone(),
        }
        assert!(fit(&r, &limits()).is_err(), "kind {kind}");
    }
}

#[test]
fn unmeasured_or_overlarge_filter_settings_are_not_silently_repaired() {
    let (r, _) = recording(identity(), true, 0);
    let fit = fit(&r, &limits()).unwrap();
    for time_constant_ms in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(
            InputProcessor::new(
                &fit,
                InputMethod::Diagonal,
                SignalSettings {
                    time_constant_ms,
                    ..settings()
                }
            )
            .is_err()
        );
    }
    assert!(
        InputProcessor::new(
            &fit,
            InputMethod::Diagonal,
            SignalSettings {
                noise_multiplier: 1e9,
                ..settings()
            }
        )
        .is_err()
    );
}

#[test]
fn explicit_axis_channel_permutation_is_preserved() {
    let a = mixing();
    let (mut r, t) = recording(a, true, 0);
    r.axes.swap(0, 1);
    for trial in &mut r.trials {
        if trial.axis == 0 {
            trial.axis = 1;
        } else if trial.axis == 1 {
            trial.axis = 0;
        }
    }
    let fit = fit(&r, &limits()).unwrap();
    let mut p = InputProcessor::new(&fit, InputMethod::Coupled, settings()).unwrap();
    p.process(&r.context, &sample(t + 10, [10.0; N])).unwrap();
    let output = p
        .process(
            &r.context,
            &sample(t + 20, values(&a, [0.3, -0.4, 0.0, 0.0, 0.0, 0.0, 0.0])),
        )
        .unwrap();
    approx(output.normalized[0].unwrap(), -0.4);
    approx(output.normalized[1].unwrap(), 0.3);
}
