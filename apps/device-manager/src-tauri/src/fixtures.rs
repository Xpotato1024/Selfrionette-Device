use selfrionette_core::calibration::*;
pub(crate) fn draft_json(d: &Draft) -> serde_json::Value {
    serde_json::json!({
        "schema_version": 1, "provenance": "synthetic fixture", "hardware_verified": false, "control_ready": false,
        "physical": {"id": d.context.source.id.to_string(), "assembly_revision": d.context.source.assembly, "firmware": d.context.source.firmware,
            "revision": d.revision, "signs": d.signs, "evidence": d.evidence, "manual_override": d.manual},
        "operation": {"user": d.context.user, "posture": d.context.posture, "epoch": d.context.source.epoch,
            "positive_spans": d.positive_spans, "reverse_spans": d.reverse_spans, "active_baseline_restored": false},
        "historical_observation": {"timestamp_ms": d.observation.timestamp_ms, "firmware_sample": d.observation.firmware_sample,
            "historical_baseline": d.observation.baseline, "delta": d.observation.delta, "oriented": d.observation.oriented, "normalized": d.observation.normalized}
    })
}
pub(crate) fn trials(target: usize, scenario: &str) -> Vec<Trial> {
    let delta = if target.is_multiple_of(2) {
        20.0 + target as f64 * 5.0
    } else {
        -40.0 - target as f64 * 5.0
    };
    let mut result = vec![];
    for repeat in 0..2 {
        let baseline = [10.0; N];
        let mut loaded = baseline;
        loaded[target] += delta;
        let mut trial = Trial {
            baseline: vec![baseline; 3],
            loaded: vec![loaded; 3],
            released: vec![baseline; 3],
            quality: Quality::KnownGoodSynthetic,
            known_direction: Some("fixture known positive input".into()),
            crosstalk_known: true,
        };
        if target == 0 {
            match scenario {
                "noise" => trial.loaded = vec![[10.01; N]; 3],
                "disagreement" if repeat == 1 => trial.loaded[0][0] = -100.0,
                "unstable" => trial.baseline[0][0] = 20.0,
                "release" => trial.released[0][0] = 20.0,
                "crosstalk" => trial.crosstalk_known = false,
                "timeout" => trial.quality = Quality::Timeout,
                "direction" => trial.known_direction = None,
                _ => {}
            }
        }
        result.push(trial);
    }
    result
}
