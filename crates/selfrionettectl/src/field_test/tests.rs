use super::*;
use std::cell::Cell;
use std::collections::VecDeque;
use std::rc::Rc;
fn configured() -> Plan {
    let mut p = Plan::template();
    p.device_label = "fixture-A".into();
    p.operator = "synthetic".into();
    p.posture = "fixture".into();
    p.board = "fixture".into();
    p.combined_instruction = "fixture axis0+axis1".into();
    for (i, a) in p.axes.iter_mut().enumerate() {
        a.positive = format!("fixture direction {i}");
    }
    p
}
#[test]
fn durations_are_bounded_and_reverse_is_opt_in() {
    let mut p = configured();
    let a = stages(&p);
    assert_eq!(a.last().unwrap().end_ms, 589_000);
    assert_eq!(
        a.iter()
            .filter(|s| s.label.starts_with("train/") && s.label.ends_with("/hold"))
            .count(),
        14
    );
    for axis in &mut p.axes {
        axis.negative = Some("safe reverse".into());
    }
    let b = stages(&p);
    assert_eq!(b.last().unwrap().end_ms, 813_000);
    assert_eq!(
        b.iter()
            .filter(|s| s.label.starts_with("train/") && s.label.ends_with("/hold"))
            .count(),
        28
    );
    assert!(b.windows(2).all(|w| w[0].end_ms == w[1].start_ms));
}
#[test]
fn template_previews_but_cannot_be_used_on_hardware() {
    Plan::template().validate(false).unwrap();
    assert!(Plan::template().validate(true).is_err());
    let mut p = configured();
    p.validate(true).unwrap();
    p.axes[0].negative = Some(String::new());
    assert!(p.validate(true).is_err());
    p = configured();
    p.operator = "hidden\nline".into();
    assert!(p.validate(false).is_err());
    p = configured();
    p.axes[1].name = p.axes[0].name.clone();
    assert!(p.validate(true).is_err());
}
#[test]
fn stats_empty_is_not_zero_measurement() {
    let mut s = Stats::default();
    assert!(s.json()["mean"].is_null());
    s.add([1.0; 7], 10).unwrap();
    s.add([3.0; 7], 20).unwrap();
    assert_eq!(s.json()["mean"], json!(([2.0; 7])));
    assert_eq!(s.json()["observed_line_rate_hz"], 100.0);
    assert!((s.json()["stddev"][0].as_f64().unwrap() - 2_f64.sqrt()).abs() < 1e-9);
}
#[test]
fn quality_remains_unknown_and_identity_is_not_forged() {
    let mut o = Observation::new(1);
    let v = o.observe("vector,1,1,2,3,4,5,6,7", 1, 0, true).unwrap();
    assert_eq!(v["quality"], "unknown");
    assert!(o.info.is_none());
    o.observe("warn,spike,0", 2, 0, true).unwrap();
    assert_eq!(o.warnings, 1);
    assert!(o.observe("vector,2,NaN,2,3,4,5,6,7", 3, 0, true).is_err());
}
#[test]
fn duplicate_wrap_reset_and_metadata_are_distinguished() {
    let mut o = Observation::new(1);
    o.observe("vector,4294967290,0,0,0,0,0,0,0", 1, 0, true)
        .unwrap();
    o.observe("vector,3,0,0,0,0,0,0,0", 2, 0, true).unwrap();
    o.observe("vector,3,0,0,0,0,0,0,0", 3, 0, true).unwrap();
    assert_eq!(o.duplicates, 1);
    assert!(o.observe("vector,1,0,0,0,0,0,0,0", 4, 0, true).is_err());
    assert!(o.observe("status,calibration_start", 5, 0, false).is_err());
    let mut o = Observation::new(1);
    o.observe("device,2,v1,unprovisioned,7", 1, 0, true)
        .unwrap();
    assert!(
        o.observe("device,2,v2,unprovisioned,7", 2, 0, true)
            .is_err()
    );
}
struct Fake {
    clock: Rc<Cell<u64>>,
    rows: VecDeque<Result<String, TransportError>>,
}
impl LineTransport for Fake {
    fn discard_input(&mut self) -> Result<(), TransportError> {
        panic!("must not discard")
    }
    fn write_line(&mut self, _: &str) -> Result<(), TransportError> {
        panic!("must NEVER transmit")
    }
    fn read_line(&mut self, _: Duration) -> Result<String, TransportError> {
        self.clock.set(self.clock.get() + 10);
        self.rows
            .pop_front()
            .unwrap_or(Err(TransportError::Timeout))
    }
}
fn short() -> Vec<Stage> {
    vec![Stage {
        label: "test".into(),
        instruction: "test".into(),
        start_ms: 0,
        end_ms: 30,
    }]
}
#[test]
fn passive_capture_preserves_receive_times_and_never_sends() {
    let clock = Rc::new(Cell::new(0));
    let mut f = Fake {
        clock: clock.clone(),
        rows: (1..=3)
            .map(|t| Ok(format!("vector,{t},1,2,3,4,5,6,7")))
            .collect(),
    };
    let mut log = Log::new(Vec::new());
    let mut o = Observation::new(1);
    let mut cues = 0;
    collect(
        &mut f,
        &short(),
        &mut log,
        &mut o,
        || clock.get(),
        || false,
        |_| {
            cues += 1;
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(o.vectors, 3);
    assert_eq!(cues, 1);
    let lines: Vec<Value> = String::from_utf8(log.out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[1]["event"], "wire");
    assert_eq!(lines[1]["host_ms"], 10);
}
#[test]
fn malformed_line_is_preserved_before_failure() {
    let clock = Rc::new(Cell::new(0));
    let mut f = Fake {
        clock: clock.clone(),
        rows: VecDeque::from([Ok("vector,1,NaN,0,0,0,0,0,0".into())]),
    };
    let mut log = Log::new(Vec::new());
    let mut o = Observation::new(1);
    assert!(
        collect(
            &mut f,
            &short(),
            &mut log,
            &mut o,
            || clock.get(),
            || false,
            |_| Ok(())
        )
        .is_err()
    );
    assert!(String::from_utf8(log.out).unwrap().contains("NaN"));
}
#[test]
fn interruption_and_timeout_do_not_create_fake_samples() {
    let clock = Rc::new(Cell::new(0));
    let mut f = Fake {
        clock: clock.clone(),
        rows: VecDeque::from([Ok("vector,1,0,0,0,0,0,0,0".into())]),
    };
    let mut log = Log::new(Vec::new());
    let mut o = Observation::new(1);
    assert!(
        collect(
            &mut f,
            &short(),
            &mut log,
            &mut o,
            || clock.get(),
            || true,
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(clock.get(), 0);
    assert!(
        collect(
            &mut f,
            &short(),
            &mut log,
            &mut o,
            || clock.get(),
            || false,
            |_| Ok(())
        )
        .is_err()
    );
    assert_eq!(o.vectors, 1);
}
#[test]
fn budget_and_write_errors_are_not_silent() {
    let mut log = Log::new(Vec::new());
    log.bytes = MAX_LOG_BYTES;
    assert!(log.event(json!({"x":1})).is_err());
    struct Bad;
    impl Write for Bad {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(Log::new(Bad).event(json!({"x":1})).is_err());
}
#[test]
fn invalid_hardware_arguments_fail_before_io() {
    for args in [
        vec![],
        vec![
            "capture",
            "unopened.json",
            "--port",
            "COM_BAD",
            "--out",
            "nope",
        ],
        vec![
            "capture",
            "unopened.json",
            "--port",
            "",
            "--out",
            "nope",
            "--yes-open",
        ],
    ] {
        assert!(matches!(
            run(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
            Err(CliError::Usage(_))
        ));
    }
}
#[test]
fn unobserved_stages_are_not_certified() {
    let s = stages(&configured());
    let o = Observation::new(s.len());
    let r = o.report(&s, "aborted", Some("test"), "synthetic");
    assert_eq!(r["hardware_verified"], false);
    assert_eq!(r["control_ready"], false);
    assert!(o.review(&s, "aborted").contains("未取得"));
}

#[test]
fn silent_startup_reports_no_vectors_instead_of_generic_timeout() {
    let clock = Rc::new(Cell::new(0));
    let mut f = Fake {
        clock: clock.clone(),
        rows: VecDeque::new(),
    };
    let stages = [Stage {
        label: "startup".into(),
        instruction: "wait".into(),
        start_ms: 0,
        end_ms: 40_000,
    }];
    let mut log = Log::new(Vec::new());
    let mut o = Observation::new(1);
    let err = collect(
        &mut f,
        &stages,
        &mut log,
        &mut o,
        || clock.get(),
        || false,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(err.contains("no 7ch vector in 30 s"), "{err}");
    assert!(err.contains("startup status: 0"), "{err}");
    assert_eq!(o.vectors, 0);
    assert_eq!(o.warnings, 0);
}

#[test]
fn startup_status_without_vector_is_reported_separately() {
    let clock = Rc::new(Cell::new(0));
    let mut f = Fake {
        clock: clock.clone(),
        rows: VecDeque::from([
            Ok("status,setup_start".into()),
            Ok("status,sensor_init_start".into()),
        ]),
    };
    let stages = [Stage {
        label: "startup".into(),
        instruction: "wait".into(),
        start_ms: 0,
        end_ms: 40_000,
    }];
    let mut log = Log::new(Vec::new());
    let mut o = Observation::new(1);
    let err = collect(
        &mut f,
        &stages,
        &mut log,
        &mut o,
        || clock.get(),
        || false,
        |_| Ok(()),
    )
    .unwrap_err();
    assert!(err.contains("startup status: 2"), "{err}");
    assert_eq!(o.statuses, 2);
    assert_eq!(o.vectors, 0);
    assert_eq!(
        o.report(&stages, "aborted", Some(&err), "synthetic")["statuses"],
        2
    );
}

fn short_guided_stages() -> Vec<Stage> {
    [
        ("train/axis0/positive/r0/prepare", 20),
        ("train/axis0/positive/r0/neutral", 40),
        ("train/axis0/positive/r0/ramp", 60),
        ("train/axis0/positive/r0/hold", 80),
        ("train/axis0/positive/r0/release", 100),
        ("final-neutral", 120),
    ]
    .into_iter()
    .scan(0, |begin, (label, end)| {
        let next = Stage {
            label: label.into(),
            instruction: format!("physical instruction for {label}"),
            start_ms: *begin,
            end_ms: end,
        };
        *begin = end;
        Some(next)
    })
    .collect()
}

fn guided_fixture() -> (Fake, Rc<Cell<u64>>) {
    let clock = Rc::new(Cell::new(0));
    let rows = (1..=25)
        .map(|ms| Ok(format!("vector,{ms},1,2,3,4,5,6,7")))
        .collect();
    (
        Fake {
            clock: clock.clone(),
            rows,
        },
        clock,
    )
}

#[test]
fn guided_trial_pauses_until_ready_and_excludes_preparation_data() {
    let (mut fake, clock) = guided_fixture();
    let stages = short_guided_stages();
    let mut log = Log::new(Vec::new());
    let mut obs = Observation::new(stages.len() + 1);
    let mut polls = 0;
    let mut cues = Vec::new();
    guided::collect(
        &mut fake,
        &stages,
        &mut log,
        &mut obs,
        || clock.get(),
        || false,
        || {
            polls += 1;
            (polls == 3).then_some(guided::Action::Ready)
        },
        |stage, waiting| cues.push((stage.label.clone(), waiting)),
    )
    .unwrap();
    assert!(obs.stats.last().unwrap().count >= 2);
    assert!(obs.stats[0].count > 0);
    assert!(obs.stats[3].count > 0);
    assert!(obs.stats[5].count > 0);
    assert_eq!(
        obs.report(&stages, "complete", None, "synthetic")["unscored_vectors"],
        obs.stats.last().unwrap().count
    );
    assert!(cues[0].1);
    assert!(!cues[1].1);
    let events = String::from_utf8(log.out).unwrap();
    assert!(events.contains("\"event\":\"operator_wait\""));
    assert!(events.contains("\"event\":\"operator_ready\""));
    assert!(events.contains("\"stage_index\":null"));
}

#[test]
fn guided_skip_leaves_unperformed_trial_unscored_and_continues() {
    let (mut fake, clock) = guided_fixture();
    let stages = short_guided_stages();
    let mut log = Log::new(Vec::new());
    let mut obs = Observation::new(stages.len() + 1);
    guided::collect(
        &mut fake,
        &stages,
        &mut log,
        &mut obs,
        || clock.get(),
        || false,
        || Some(guided::Action::Skip),
        |_, _| {},
    )
    .unwrap();
    assert!(obs.stats[..5].iter().all(|s| s.count == 0));
    assert!(obs.stats[5].count > 0);
    assert!(
        String::from_utf8(log.out)
            .unwrap()
            .contains("\"event\":\"operator_skip\"")
    );
}

#[test]
fn guided_quit_never_generates_unrequested_measurements() {
    let (mut fake, clock) = guided_fixture();
    let stages = short_guided_stages();
    let mut log = Log::new(Vec::new());
    let mut obs = Observation::new(stages.len() + 1);
    let err = guided::collect(
        &mut fake,
        &stages,
        &mut log,
        &mut obs,
        || clock.get(),
        || false,
        || Some(guided::Action::Stop),
        |_, _| {},
    )
    .unwrap_err();
    assert!(err.contains("operator stopped"));
    assert_eq!(obs.vectors, 0);
}

#[test]
fn legacy_quick_plan_is_upgraded_only_when_exactly_provisional() {
    let mut plan = configured();
    plan.device_label = "PROVISIONAL-DEVICE-ID-UNKNOWN".into();
    for (i, axis) in plan.axes.iter_mut().enumerate() {
        axis.name = format!("PROVISIONAL group {i}");
        axis.positive =
            format!("Gently move {i} in ONE SAFE direction. Physical direction NOT YET DEFINED.");
    }
    assert!(upgrade_provisional_plan(&mut plan));
    assert!(plan.axes[1].positive.contains("右向き"));
    assert!(plan.axes[6].positive.contains("上向き"));
    assert_eq!(plan.axes[1].negative, None);
    assert!(!upgrade_provisional_plan(&mut plan));

    let mut manual = configured();
    let previous = manual.axes[1].positive.clone();
    assert!(!upgrade_provisional_plan(&mut manual));
    assert_eq!(manual.axes[1].positive, previous);
}
