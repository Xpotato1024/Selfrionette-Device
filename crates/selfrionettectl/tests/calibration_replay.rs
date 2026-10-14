use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Temp(PathBuf);
impl Temp {
    fn new(bytes: &[u8]) -> Self {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "selfrionette-replay-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("recording.json");
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        Self(path)
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_dir(self.0.parent().unwrap());
    }
}
fn invoke(value: &Value) -> (Output, Temp) {
    let temp = Temp::new(&serde_json::to_vec(value).unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
        .arg("calibrate-replay")
        .arg(&temp.0)
        .output()
        .unwrap();
    (output, temp)
}
fn sample(t: &mut u32, values: [f64; 7]) -> Value {
    *t += 10;
    json!({"timestamp_ms":*t,"epoch":42,"values":values,"quality":"usable"})
}
fn window(t: &mut u32, values: [f64; 7]) -> Vec<Value> {
    (0..8)
        .map(|i| sample(t, values.map(|v| v + if i % 2 == 0 { -0.1 } else { 0.1 })))
        .collect()
}
fn mix(target: [f64; 7]) -> [f64; 7] {
    let mut result = target.map(|v| v * 1000.0);
    result[0] += target[1] * 300.0;
    result[1] += target[0] * 200.0;
    result.map(|v| v + 10.0)
}
fn input(reverse: bool) -> Value {
    let mut t = 0;
    let neutral = window(&mut t, [10.0; 7]);
    let mut trials = vec![];
    for axis in 0..7 {
        for sign in if reverse { vec![1.0, -1.0] } else { vec![1.0] } {
            for repeat in 0..2 {
                let mut target = [0.0; 7];
                target[axis] = sign;
                trials.push(json!({"id":format!("axis-{axis}-{sign}-{repeat}"),"axis":axis,"level":sign,"instruction":"synthetic requested direction",
            "neutral":window(&mut t,[10.0;7]),"loaded":window(&mut t,mix(target)),"released":window(&mut t,[10.0;7])}));
            }
        }
    }
    let mut replay = vec![];
    for intent in [[0.0; 7], [0.4, -0.7, 0.2, 0.5, 0.1, -0.2, 0.6], [0.0; 7]] {
        replay.push(json!({"sample":sample(&mut t,mix(intent)),"intent":intent}));
    }
    json!({"schema_version":1,"recording_id":"test-synthetic-recording","sample_domain":"firmware_v2_vector","origin":"synthetic",
        "context":{"device_id":"srn-0123456789abcdef0123456789abcdef","firmware":"synthetic-test","assembly":"fixture-1",
            "epoch":42,"user":"test-user","posture":"test-posture"},
        "axes":(0..7).map(|channel|json!({"channel":channel,"positive_direction":format!("synthetic direction {channel}")})).collect::<Vec<_>>(),
        "limits":{"noise_floor_counts":([0.2;7]),"max_rest_range_counts":([1.0;7]),"max_drift_counts":([2.0;7]),"min_signal_to_noise":5.0,
            "repeat_tolerance":0.05,"direction_tolerance":0.05,"max_condition_number":20.0,"max_noise_gain":10.0,"max_sample_gap_ms":100},
        "signal":{"time_constant_ms":0.0,"noise_multiplier":3.0,"release_ratio":0.5,"max_deadzone_fraction":0.1},
        "neutral":neutral,"trials":trials,"replay":replay})
}

#[test]
fn compares_methods_without_installing_settings_or_creating_files() {
    let (output, temp) = invoke(&input(true));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["hardware_verified"], false);
    assert_eq!(report["control_ready"], false);
    assert_eq!(report["fit_limits"]["repeat_tolerance"], 0.05);
    assert_eq!(report["trial_summary"].as_array().unwrap().len(), 28);
    let a = report["comparison"]["diagonal"]["metrics"]["command_rmse"]
        .as_f64()
        .unwrap();
    let b = report["comparison"]["coupled"]["metrics"]["command_rmse"]
        .as_f64()
        .unwrap();
    assert!(a > 0.02 && b < 0.001, "a={a}, b={b}");
    assert_eq!(
        report["comparison"]["coupled"]["multi_axis_sample_count"],
        1
    );
    assert_eq!(fs::read_dir(temp.0.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn file_only_command_rejects_serial_options_before_any_work() {
    for args in [
        vec![],
        vec!["--port", "COM_SHOULD_NOT_OPEN"],
        vec!["--port"],
        vec!["a.json", "b.json"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
            .arg("calibrate-replay")
            .args(args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn rejects_unknown_schema_domain_fields_missing_quality_and_wrong_channel_count() {
    for kind in 0..7 {
        let mut data = input(true);
        match kind {
            0 => data["schema_version"] = json!(2),
            1 => data["sample_domain"] = json!("raw_adc"),
            2 => data["hardware_verified"] = json!(true),
            3 => {
                data["neutral"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("quality");
            }
            4 => data["neutral"][0]["values"] = json!([1, 2, 3]),
            5 => data["neutral"][0]["quality"] = json!("unknown"),
            _ => data["context"]["epoch"] = json!(99),
        }
        let (output, _) = invoke(&data);
        assert!(!output.status.success(), "kind {kind}");
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn replay_reuse_abnormal_quality_and_out_of_range_labels_produce_no_partial_report() {
    for kind in 0..3 {
        let mut data = input(true);
        match kind {
            0 => data["replay"][0]["sample"] = data["neutral"][0].clone(),
            1 => data["replay"][1]["sample"]["quality"] = json!("timeout"),
            _ => data["replay"][1]["intent"][0] = json!(1.5),
        }
        let (output, _) = invoke(&data);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn unsupported_directions_are_null_and_never_removed_to_improve_rmse() {
    let (output, _) = invoke(&input(false));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let coupled = &report["comparison"]["coupled"];
    assert_eq!(coupled["unsupported_sample_count"], 1);
    assert!(coupled["metrics"]["command_rmse"].is_null());
    assert!(coupled["trace"][1]["command"][1].is_null());
}

#[test]
fn unavailable_compensation_is_reported_without_substituting_diagonal_results() {
    let mut data = input(true);
    data["limits"]["max_condition_number"] = json!(1.0);
    let (output, _) = invoke(&data);
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["comparison"]["coupled"]["available"], false);
    assert_eq!(report["comparison"]["diagonal"]["available"], true);
    assert!(report["comparison"]["coupled"]["metrics"].is_null());
}

#[test]
fn bounded_regular_file_input_rejects_directories_and_oversized_records() {
    let temp = Temp::new(&vec![b' '; 8 * 1024 * 1024 + 1]);
    for path in [temp.0.as_path(), temp.0.parent().unwrap()] {
        let output = Command::new(env!("CARGO_BIN_EXE_selfrionettectl"))
            .arg("calibrate-replay")
            .arg(path)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}
