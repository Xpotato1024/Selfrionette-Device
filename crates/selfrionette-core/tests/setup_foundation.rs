use selfrionette_core::setup::N;
use selfrionette_core::{input_calibration::*, setup::*, setup_persistence::*};

fn evidence(origin: Origin) -> Evidence {
    Evidence {
        origin,
        quality: InputQuality::Usable,
        reference: "test-fixture-only".into(),
    }
}
fn context() -> CurrentContext {
    CurrentContext {
        setup: DeviceSetup {
            device_id: "srn-0123456789abcdef0123456789abcdef".parse().unwrap(),
            assembly_revision: 1,
            config_revision: 2,
            settings_generation: 3,
            channels: std::array::from_fn(|wire| ChannelOrientation {
                logical_axis: Some(wire),
                location: Some(format!("fixture-position-{wire}")),
                positive_direction: Some(format!("fixture-direction-{wire}")),
                polarity: Some(1),
            }),
            mapping_validated: true,
            evidence: evidence(Origin::Recorded),
        },
        firmware: FirmwareBinding {
            version: "test-v2".into(),
            source_revision: "1".repeat(40),
            sample_domain: SampleDomain::LegacyFirmwareV2,
        },
        operator: OperatorContext {
            operator: "test-person".into(),
            posture: "test-posture".into(),
            grip: "test-grip".into(),
        },
    }
}
fn profile(c: &CurrentContext) -> ProfileBinding {
    ProfileBinding {
        profile_revision: 1,
        parameter_record: "fixture-parameters-1".into(),
        setup: c.setup.clone(),
        firmware: c.firmware.clone(),
        operator: c.operator.clone(),
        evidence: c.setup.evidence.clone(),
    }
}
fn raw() -> DomainSample {
    DomainSample {
        domain: SampleDomain::RawAdc,
        values: [10., 20., 30., 40., 50., 60., 70.],
        quality: InputQuality::Usable,
    }
}

#[test]
fn reversed_and_swapped_sensors_are_projected_once_without_changing_wire_evidence() {
    let mut c = context();
    c.setup.channels[0].polarity = Some(-1);
    c.setup.channels[0].logical_axis = Some(2);
    c.setup.channels[2].logical_axis = Some(0);
    let sample = raw();
    let projected = project_raw(&c.setup, &sample).unwrap();
    assert_eq!(projected.values(), &[30., 20., -10., 40., 50., 60., 70.]);
    assert_eq!(sample.values[0], 10.);
    for domain in [
        SampleDomain::LegacyFirmwareV2,
        SampleDomain::DeviceOrientedCounts,
        SampleDomain::Newtons,
        SampleDomain::NormalizedInput,
    ] {
        assert_eq!(
            project_raw(
                &c.setup,
                &DomainSample {
                    domain,
                    values: *projected.values(),
                    quality: InputQuality::Usable
                }
            )
            .unwrap_err(),
            SetupError::DomainMismatch
        );
    }
}

#[test]
fn missingness_and_unvalidated_mounting_never_become_positive_polarity() {
    let mut c = context();
    c.setup.mapping_validated = false;
    c.setup.channels[0].polarity = None;
    c.setup.channels[2].logical_axis = None;
    c.setup.channels[3].location = None;
    let loaded = decode_context(&encode_context(&c).unwrap()).unwrap();
    assert_eq!(loaded, c);
    assert!(!loaded.setup.is_complete());
    assert_eq!(
        project_raw(&loaded.setup, &raw()).unwrap_err(),
        SetupError::IncompleteSetup
    );
    assert_eq!(
        profile(&loaded).validate(),
        Err(SetupError::IncompleteSetup)
    );
    c.setup.mapping_validated = true;
    assert_eq!(c.setup.validate(), Err(SetupError::IncompleteSetup));
}

#[test]
fn rejects_duplicate_invalid_and_aliased_channels_or_polarities() {
    for axis in [Some(0), Some(7), Some(usize::MAX)] {
        let mut c = context();
        c.setup.channels[1].logical_axis = axis;
        assert_eq!(c.setup.validate(), Err(SetupError::InvalidMapping));
    }
    for polarity in [0, 2, -2, i8::MIN] {
        let mut c = context();
        c.setup.channels[0].polarity = Some(polarity);
        assert_eq!(c.setup.validate(), Err(SetupError::InvalidPolarity));
    }
    let mut c = context();
    c.setup.channels[1].location = c.setup.channels[0].location.clone();
    c.setup.channels[1].positive_direction = c.setup.channels[0].positive_direction.clone();
    assert_eq!(c.setup.validate(), Err(SetupError::InvalidMapping));
}

#[test]
fn exact_device_assembly_config_generation_and_mapping_binding_is_required() {
    let c = context();
    let p = profile(&c);
    p.check_compatibility(&c).unwrap();
    for revision in 0..7 {
        let mut changed = c.clone();
        match revision {
            0 => changed.setup.device_id = "srn-1123456789abcdef0123456789abcdef".parse().unwrap(),
            1 => changed.setup.assembly_revision += 1,
            2 => changed.setup.config_revision += 1,
            3 => changed.setup.settings_generation += 1,
            4 => changed.setup.channels[0].polarity = Some(-1),
            5 => {
                changed.setup.channels[0].logical_axis = Some(1);
                changed.setup.channels[1].logical_axis = Some(0);
            }
            _ => changed.setup.channels[0].positive_direction = Some("new-mount-direction".into()),
        }
        assert_eq!(
            p.check_compatibility(&changed),
            Err(SetupError::RevalidationRequired)
        );
    }
    for field in 0..3 {
        let mut changed = c.clone();
        match field {
            0 => changed.operator.operator.push('2'),
            1 => changed.operator.posture.push('2'),
            _ => changed.operator.grip.push('2'),
        }
        assert_eq!(
            p.check_compatibility(&changed),
            Err(SetupError::OperatorMismatch)
        );
    }
    let mut changed = c.clone();
    changed.firmware.version.push('2');
    assert_eq!(
        p.check_compatibility(&changed),
        Err(SetupError::FirmwareMismatch)
    );
    changed = c.clone();
    changed.firmware.source_revision = "2".repeat(40);
    assert_eq!(
        p.check_compatibility(&changed),
        Err(SetupError::FirmwareMismatch)
    );
    changed = c.clone();
    changed.firmware.sample_domain = SampleDomain::RawAdc;
    assert_eq!(
        p.check_compatibility(&changed),
        Err(SetupError::DomainMismatch)
    );
}

#[test]
fn tare_reconnect_and_parameter_changes_require_fresh_neutral_but_keep_binding() {
    let c = context();
    let p = profile(&c);
    let epoch = SessionEpoch {
        connection: 10,
        tare: 20,
    };
    let neutral = SessionNeutral::new(&p, c.clone(), epoch, [5.; N], InputQuality::Usable).unwrap();
    assert_eq!(neutral.check(&p, &c, epoch).unwrap(), &[5.; N]);
    for new_epoch in [
        SessionEpoch {
            connection: 11,
            ..epoch
        },
        SessionEpoch { tare: 21, ..epoch },
    ] {
        assert_eq!(
            neutral.check(&p, &c, new_epoch).unwrap_err(),
            SetupError::FreshNeutralRequired
        );
        p.check_compatibility(&c).unwrap();
        SessionNeutral::new(&p, c.clone(), new_epoch, [6.; N], InputQuality::Usable).unwrap();
    }
    let mut new_profile = p.clone();
    new_profile.profile_revision += 1;
    assert_eq!(
        neutral.check(&new_profile, &c, epoch).unwrap_err(),
        SetupError::FreshNeutralRequired
    );
    let mut changed = c.clone();
    changed.setup.settings_generation += 1;
    assert_eq!(
        neutral.check(&p, &changed, epoch).unwrap_err(),
        SetupError::FreshNeutralRequired
    );
}

#[test]
fn synthetic_unknown_or_nonfinite_data_cannot_supply_live_neutral() {
    let mut c = context();
    c.setup.evidence.origin = Origin::Synthetic;
    let p = profile(&c);
    let epoch = SessionEpoch {
        connection: 1,
        tare: 1,
    };
    // Synthetic manifests remain synthetic after a roundtrip and compare only as metadata.
    let loaded = decode_binding(&encode_binding(&p).unwrap()).unwrap();
    assert_eq!(loaded.evidence.origin, Origin::Synthetic);
    assert!(matches!(
        SessionNeutral::new(&loaded, c, epoch, [0.; N], InputQuality::Usable),
        Err(SetupError::SyntheticOnly)
    ));
    let c = context();
    let mut p = profile(&c);
    for quality in [
        InputQuality::Unknown,
        InputQuality::Reset,
        InputQuality::Timeout,
        InputQuality::Spike,
        InputQuality::Clipped,
    ] {
        assert!(matches!(
            SessionNeutral::new(&p, c.clone(), epoch, [0.; N], quality),
            Err(SetupError::EvidenceUnusable)
        ));
        p.evidence.quality = quality;
        assert_eq!(p.validate(), Err(SetupError::EvidenceUnusable));
    }
    p = profile(&c);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(matches!(
            SessionNeutral::new(&p, c.clone(), epoch, [value; N], InputQuality::Usable),
            Err(SetupError::InvalidSample)
        ));
    }
}

#[test]
fn raw_domain_rejects_nonfinite_fractional_out_of_range_and_unknown_quality() {
    let c = context();
    for value in [f64::NAN, f64::INFINITY, 0.5, -8388609., 8388608.] {
        let mut sample = raw();
        sample.values[0] = value;
        assert_eq!(
            project_raw(&c.setup, &sample).unwrap_err(),
            SetupError::InvalidSample
        );
    }
    let mut sample = raw();
    sample.quality = InputQuality::Unknown;
    assert_eq!(
        project_raw(&c.setup, &sample).unwrap_err(),
        SetupError::EvidenceUnusable
    );
}

#[test]
fn binding_codec_revalidates_schema_revisions_ids_mapping_and_provenance() {
    let c = context();
    let p = profile(&c);
    let text = encode_binding(&p).unwrap();
    assert_eq!(decode_binding(&text).unwrap(), p);
    assert!(!text.contains("neutral"));
    let first_wire = text
        .lines()
        .find(|line| line.starts_with("wire,0,"))
        .unwrap();
    let second_wire = text
        .lines()
        .find(|line| line.starts_with("wire,1,"))
        .unwrap();
    for invalid in [
        text.replace("srn-profile-binding,1", "srn-profile-binding,2"),
        text.replace(
            "srn-0123456789abcdef0123456789abcdef",
            "srn-00000000000000000000000000000000",
        ),
        text.replace(",1,2,3,1\n", ",0,2,3,1\n"),
        text.replace(",1,2,3,1\n", ",1,0,3,1\n"),
        text.replace(",1,2,3,1\n", ",1,2,0,1\n"),
        text.replace("profile,1,", "profile,0,"),
        text.replace(first_wire, &first_wire.replace("wire,0,0,", "wire,0,1,")),
        text.replace(second_wire, first_wire),
        text.replace("evidence,recorded,usable,", "evidence,recorded,unknown,"),
        text.replace("firmware_v2_vector", "rawish"),
        text.replace(&"1".repeat(40), &"0".repeat(40)),
        format!("{text}unknown,1\n"),
        "x".repeat(32769),
        text[..text.len() / 2].into(),
    ] {
        assert!(decode_binding(&invalid).is_err(), "accepted {invalid}");
    }
    for metadata in [
        "".into(),
        "x".repeat(129),
        "line\nbreak".into(),
        " padded".into(),
    ] {
        let mut c = c.clone();
        c.operator.operator = metadata;
        assert!(encode_context(&c).is_err());
    }
}

#[test]
fn bound_fitting_uses_existing_engine_and_keeps_processed_domain_separate() {
    let c = context();
    let p = profile(&c);
    let mut t = 0;
    let mut window = |values: [f64; N]| -> Vec<FirmwareVectorSample> {
        (0..8)
            .map(|i| {
                t += 10;
                FirmwareVectorSample {
                    timestamp_ms: t,
                    epoch: 1,
                    values: values.map(|v| v + if i % 2 == 0 { -0.1 } else { 0.1 }),
                    quality: InputQuality::Usable,
                }
            })
            .collect()
    };
    let neutral = window([10.; N]);
    let mut trials = Vec::new();
    for axis in 0..N {
        for sign in [1., -1.] {
            for repeat in 0..2 {
                let level = sign * if repeat == 0 { 1. } else { 0.5 };
                let mut values = [10.; N];
                values[axis] += level * if axis == 1 { -1000. } else { 1000. };
                trials.push(GuidedTrial {
                    id: format!("{axis}-{sign}-{repeat}"),
                    axis,
                    level,
                    instruction: format!("fixture instruction {axis} {sign}"),
                    neutral: window([10.; N]),
                    loaded: window(values),
                    released: window([10.; N]),
                });
            }
        }
    }
    let mut r = CalibrationRecording {
        context: selfrionette_core::calibration::Context {
            source: selfrionette_core::calibration::Source {
                id: c.setup.device_id,
                firmware: c.firmware.version.clone(),
                assembly: "1".into(),
                epoch: 1,
            },
            user: c.operator.operator.clone(),
            posture: c.operator.posture.clone(),
        },
        axes: std::array::from_fn(|axis| AxisDefinition {
            channel: axis,
            positive_direction: c.setup.channels[axis].positive_direction.clone().unwrap(),
        }),
        neutral,
        trials,
    };
    let limits = FitLimits {
        noise_floor_counts: [0.2; N],
        max_rest_range_counts: [1.; N],
        max_drift_counts: [2.; N],
        min_signal_to_noise: 5.,
        repeat_tolerance: 0.05,
        direction_tolerance: 0.05,
        max_condition_number: 20.,
        max_noise_gain: 10.,
        max_sample_gap_ms: 100,
    };
    let fitted = p.fit_v2(&c, &r, &limits).unwrap();
    assert_eq!(fitted.summary().polarity, [1, -1, 1, 1, 1, 1, 1]);
    assert_eq!(
        fitted.summary().positive_span_counts,
        fit(&r, &limits).unwrap().summary().positive_span_counts
    );
    assert!(!fitted.control_ready() && !fitted.hardware_verified());
    r.neutral[0].quality = InputQuality::Unknown;
    assert!(matches!(
        p.fit_v2(&c, &r, &limits),
        Err(SetupError::Fit(InputError::QualityUnusable))
    ));
    let mut c = c;
    c.firmware.sample_domain = SampleDomain::RawAdc;
    let p = profile(&c);
    assert!(matches!(
        p.fit_v2(&c, &r, &limits),
        Err(SetupError::UnsupportedFitDomain)
    ));
}
