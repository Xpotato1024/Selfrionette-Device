use selfrionette_core::calibration::*;
use selfrionette_core::{DeviceId, VectorFrame};
fn id() -> DeviceId {
    "srn-0123456789abcdef0123456789abcdef".parse().unwrap()
}
fn context() -> Context {
    Context {
        source: Source {
            id: id(),
            firmware: "fixture-v2".into(),
            assembly: "synthetic-1".into(),
            epoch: 1,
        },
        user: "fixture-user".into(),
        posture: "fixture-neutral".into(),
    }
}
fn limits() -> Thresholds {
    Thresholds {
        noise_floor: 0.1,
        stability: 0.5,
        signal_to_noise: 5.0,
        cross_ratio: 0.2,
    }
}
fn trial(ch: usize, delta: f64) -> Trial {
    let baseline = [10.0; N];
    let mut loaded = baseline;
    loaded[ch] += delta;
    Trial {
        baseline: vec![baseline; 3],
        loaded: vec![loaded; 3],
        released: vec![baseline; 3],
        quality: Quality::KnownGoodSynthetic,
        known_direction: Some("fixture known positive input".into()),
        crosstalk_known: true,
    }
}
fn session() -> CalibrationSession {
    let mut s = CalibrationSession::new(context()).unwrap();
    s.set_baseline([10.0; N]).unwrap();
    s.set_candidates(
        std::array::from_fn(|i| {
            assess(
                i,
                &[
                    trial(i, if i % 2 == 0 { 20.0 } else { -40.0 }),
                    trial(i, if i % 2 == 0 { 20.0 } else { -40.0 }),
                ],
                limits(),
            )
        }),
        std::array::from_fn(|i| format!("synthetic repeat evidence wirech{i}")),
    )
    .unwrap();
    s
}
fn sample(values: [f64; N]) -> FirmwareSample {
    FirmwareSample::from_vector(
        context(),
        &VectorFrame {
            timestamp_ms: 0,
            channels: values,
        },
        Quality::KnownGoodSynthetic,
    )
    .unwrap()
}
#[test]
fn different_spans_and_signs_preserve_original_order_and_values() {
    let mut s = session();
    let original = [20.0, -10.0, 20.0, -10.0, 20.0, -10.0, 20.0];
    let preview = s.preview(&sample(original)).unwrap();
    assert_eq!(preview.firmware_sample, original);
    assert_eq!(preview.normalized, [Some(0.5); N]);
    assert_eq!(preview.oriented, [10.0, 20.0, 10.0, 20.0, 10.0, 20.0, 10.0]);
}
#[test]
fn noise_invalid_numbers_sign_missing_channels_and_binding_rejected() {
    assert!(matches!(
        assess(0, &[trial(0, 0.1), trial(0, 0.1)], limits()),
        Assessment::Inconclusive(_)
    ));
    let mut bad = [10.0; N];
    bad[0] = f64::NAN;
    assert!(
        FirmwareSample::from_vector(
            context(),
            &VectorFrame {
                timestamp_ms: 0,
                channels: bad
            },
            Quality::KnownGoodSynthetic
        )
        .is_err()
    );
    bad[0] = f64::INFINITY;
    assert!(
        FirmwareSample::from_vector(
            context(),
            &VectorFrame {
                timestamp_ms: 0,
                channels: bad
            },
            Quality::KnownGoodSynthetic
        )
        .is_err()
    );
    let mut s = session();
    assert!(
        s.set_candidates(
            std::array::from_fn(|_| Assessment::Candidate {
                sign: 0,
                positive_span: 2.0
            }),
            std::array::from_fn(|_| "fixture".into())
        )
        .is_err()
    );
    assert!(selfrionette_core::parse_line("vector,0,1,2,3,4,5,6").is_err());
    assert!(validate_bindings(&[id(), id()], [1, 2, 3, 4, 5, 6, 7]).is_err());
    assert!(validate_bindings(&[id()], [1, 1, 3, 4, 5, 6, 7]).is_err());
    let mut wrong = context();
    wrong.source.epoch = 2;
    let wrong = FirmwareSample::from_vector(
        wrong,
        &VectorFrame {
            timestamp_ms: 0,
            channels: [20.0; N],
        },
        Quality::KnownGoodSynthetic,
    )
    .unwrap();
    assert!(s.preview(&wrong).is_err());
}
#[test]
fn disagreement_unstable_release_crosstalk_and_missing_direction_inconclusive() {
    assert!(matches!(
        assess(0, &[trial(0, 20.0), trial(0, -20.0)], limits()),
        Assessment::Inconclusive(_)
    ));
    for issue in 0..5 {
        let mut bad = trial(0, 20.0);
        match issue {
            0 => bad.baseline[0][0] = 15.0,
            1 => bad.released[0][0] = 15.0,
            2 => bad.crosstalk_known = false,
            3 => bad.known_direction = None,
            _ => bad.loaded[0][1] = 50.0,
        }
        assert!(matches!(
            assess(0, &[trial(0, 20.0), bad], limits()),
            Assessment::Inconclusive(_)
        ));
    }
}
#[test]
fn one_direction_never_invents_reverse_span() {
    let mut s = session();
    let p = s
        .preview(&sample([0.0, 30.0, 0.0, 30.0, 0.0, 30.0, 0.0]))
        .unwrap();
    assert_eq!(p.normalized, [None; N]);
}
#[test]
fn manual_override_invalidates_confirmation_and_records_reason() {
    let mut s = session();
    let p = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&p).unwrap();
    s.manual_flip(0, "fixture operator direction confirmation")
        .unwrap();
    assert!(s.draft().is_err());
    assert!(s.approve(&p).is_err());
    let p = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&p).unwrap();
    let d = s.draft().unwrap();
    assert!(d.manual[0]);
    assert!(d.evidence[0].contains("manual override"));
    assert!(!d.hardware_verified());
    assert!(!d.control_ready());
}
#[test]
fn reset_tare_reconnect_and_reload_require_fresh_baseline() {
    let mut s = session();
    let p = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&p).unwrap();
    let d = s.draft().unwrap();
    let mut loaded = d.reload(context()).unwrap();
    assert!(loaded.preview(&sample([10.0; N])).is_err());
    s.invalidate();
    assert!(s.preview(&sample([10.0; N])).is_err());
    assert!(s.draft().is_err());
    for quality in [
        Quality::Unknown,
        Quality::Reset,
        Quality::Spike,
        Quality::Timeout,
        Quality::Clipped,
    ] {
        let mut t = trial(0, 20.0);
        t.quality = quality;
        assert!(matches!(
            assess(0, &[t.clone(), t], limits()),
            Assessment::Inconclusive(_)
        ));
    }
}
#[test]
fn runtime_samples_do_not_recalibrate_and_saturation_is_separate() {
    let mut s = session();
    let normal = s.preview(&sample([10.0; N])).unwrap();
    let high = s
        .preview(&sample([
            110.0, -190.0, 110.0, -190.0, 110.0, -190.0, 110.0,
        ]))
        .unwrap();
    assert_eq!(high.revision, normal.revision);
    assert_eq!(high.normalized, [Some(5.0); N]);
    assert_eq!(high.displayed, [Some(1.0); N]);
    assert_eq!(high.saturated, [true; N]);
    assert!(s.approve(&high).is_err());
    // Preview cannot be passed back to preview(): it requires FirmwareSample.
}

#[test]
fn preview_change_and_tampering_invalidate_confirmation() {
    let mut s = session();
    let old = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&old).unwrap();
    let fresh = s.preview(&sample([10.0; N])).unwrap();
    assert!(s.draft().is_err());
    let mut changed = fresh.clone();
    changed.firmware_sample[0] = 999.0;
    assert!(s.approve(&changed).is_err());
    s.approve(&fresh).unwrap();
    let bad = std::array::from_fn(|_| Assessment::Inconclusive("noise"));
    assert!(
        s.set_candidates(bad, std::array::from_fn(|_| "fixture".into()))
            .is_err()
    );
    assert!(s.draft().is_err());
    assert!(s.preview(&sample([10.0; N])).is_err());
}

#[test]
fn repeated_auto_candidates_clear_manual_origin() {
    let mut s = session();
    s.manual_flip(0, "synthetic manual confirmation").unwrap();
    s.set_baseline([10.0; N]).unwrap();
    s.set_candidates(
        std::array::from_fn(|_| Assessment::Candidate {
            sign: 1,
            positive_span: 20.0,
        }),
        std::array::from_fn(|_| "new synthetic auto evidence".into()),
    )
    .unwrap();
    let p = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&p).unwrap();
    let d = s.draft().unwrap();
    assert_eq!(d.manual, [false; N]);
    assert!(!d.evidence[0].contains("manual override"));
}
#[test]
fn finite_phase_values_cannot_hide_mean_overflow() {
    let mut t = trial(0, 20.0);
    for p in [&mut t.baseline, &mut t.loaded, &mut t.released] {
        for frame in p {
            frame[1] = 1.0e308;
        }
    }
    assert!(matches!(
        assess(0, &[t.clone(), t], limits()),
        Assessment::Inconclusive("phase mean/difference overflow")
    ));
}

#[test]
fn rejected_preview_request_revokes_previous_confirmation() {
    let mut s = session();
    let old = s.preview(&sample([10.0; N])).unwrap();
    s.approve(&old).unwrap();
    s.revoke_preview();
    assert!(s.draft().is_err());
    assert!(s.approve(&old).is_err());
    let fresh = s.preview(&sample([10.0; N])).unwrap();
    assert_eq!(fresh.revision, old.revision);
    s.approve(&fresh).unwrap();
    assert!(!s.draft().unwrap().control_ready());
}
