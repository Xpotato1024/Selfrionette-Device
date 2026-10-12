use selfrionette_core::calibration::*;
use selfrionette_core::{DeviceId, VectorFrame};
use serde::Serialize;
use std::sync::Mutex;
#[derive(Default)]
struct State {
    session: Option<CalibrationSession>,
    context: Option<Context>,
    preview: Option<Preview>,
    stage: u8,
    role: String,
}
#[derive(Serialize)]
struct Row {
    wire: usize,
    physical: usize,
    firmware: f64,
    baseline: f64,
    oriented: f64,
    normalized: Option<f64>,
    displayed: Option<f64>,
    saturated: bool,
    delta: f64,
    sign: i8,
    positive_span: f64,
    evidence: String,
}
#[derive(Serialize)]
struct View {
    stage: u8,
    id: String,
    role: String,
    epoch: u64,
    provenance: &'static str,
    hardware_verified: bool,
    control_ready: bool,
    rows: Vec<Row>,
    export: Option<String>,
    message: String,
}
fn view(state: &State, message: &str) -> View {
    let context = state.context.as_ref();
    let rows = state
        .preview
        .as_ref()
        .map(|p| {
            (0..N)
                .map(|i| Row {
                    wire: i,
                    physical: i + 1,
                    firmware: p.firmware_sample[i],
                    baseline: p.baseline[i],
                    oriented: p.oriented[i],
                    normalized: p.normalized[i],
                    displayed: p.displayed[i],
                    saturated: p.saturated[i],
                    delta: p.delta[i],
                    sign: p.signs[i],
                    positive_span: p.positive_spans[i],
                    evidence: p.evidence[i].clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    View {
        stage: state.stage,
        id: context.map(|c| c.source.id.to_string()).unwrap_or_default(),
        role: state.role.clone(),
        epoch: context.map(|c| c.source.epoch).unwrap_or(0),
        provenance: "synthetic fixture — no hardware evidence",
        hardware_verified: false,
        control_ready: false,
        rows,
        export: None,
        message: message.into(),
    }
}
fn trials(target: usize, scenario: &str) -> Vec<Trial> {
    let delta = if target % 2 == 0 {
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
#[tauri::command]
fn fixture_action(
    action: String,
    argument: String,
    state: tauri::State<Mutex<State>>,
) -> Result<View, String> {
    let mut state = state.lock().map_err(|_| "state unavailable")?;
    match action.as_str() {
        "begin" => {
            if !["left", "right"].contains(&argument.as_str()) {
                return Err("explicit role required".into());
            }
            state.role = argument.clone();
            let id: DeviceId = if argument == "right" {
                "srn-22222222222222222222222222222222"
            } else {
                "srn-11111111111111111111111111111111"
            }
            .parse()
            .map_err(|e| format!("{e}"))?;
            let epoch = state
                .context
                .as_ref()
                .map(|c| c.source.epoch + 1)
                .unwrap_or(1);
            let context = Context {
                source: Source {
                    id,
                    firmware: "synthetic-v2".into(),
                    assembly: "fixture-assembly-1".into(),
                    epoch,
                },
                user: "fixture-user".into(),
                posture: "fixture-neutral".into(),
            };
            validate_bindings(&[id], [1, 2, 3, 4, 5, 6, 7])?;
            state.session = Some(CalibrationSession::new(context.clone())?);
            state.context = Some(context);
            state.preview = None;
            state.stage = 1;
        }
        "baseline" => {
            state
                .session
                .as_mut()
                .ok_or("start fixture first")?
                .set_baseline([10.0; N])?;
            state.preview = None;
            state.stage = 2;
        }
        "assess" => {
            if state.stage < 2 {
                return Err("fresh baseline required".into());
            }
            state.preview = None;
            state.stage = 2;
            let candidates = std::array::from_fn(|i| {
                assess(
                    i,
                    &trials(i, &argument),
                    Thresholds {
                        noise_floor: 0.1,
                        stability: 0.5,
                        signal_to_noise: 5.0,
                        cross_ratio: 0.2,
                    },
                )
            });
            state
                .session
                .as_mut()
                .ok_or("fixture missing")?
                .set_candidates(
                    candidates,
                    std::array::from_fn(|i| {
                        format!(
                            "synthetic trial wirech{i}; known direction; repeat 2; return checked"
                        )
                    }),
                )?;
            state.stage = 3;
        }
        "flip" => {
            let (ch, reason) = argument.split_once(':').ok_or("channel:reason required")?;
            let ch = ch.parse().map_err(|_| "invalid wire channel")?;
            state
                .session
                .as_mut()
                .ok_or("fixture missing")?
                .manual_flip(ch, reason)?;
            state.preview = None;
            state.stage = 3;
        }
        "preview" => {
            if state.stage < 3 {
                return Err("qualified candidates required".into());
            }
            let level: f64 = argument.parse().map_err(|_| "invalid fixture level")?;
            if !level.is_finite() || !(-2.0..=2.0).contains(&level) {
                return Err("fixture level out of bounds".into());
            }
            let channels = std::array::from_fn(|i| {
                10.0 + level
                    * if i % 2 == 0 {
                        20.0 + i as f64 * 5.0
                    } else {
                        -40.0 - i as f64 * 5.0
                    }
            });
            let sample = FirmwareSample::from_vector(
                state.context.clone().ok_or("context missing")?,
                &VectorFrame {
                    timestamp_ms: 0,
                    channels,
                },
                Quality::KnownGoodSynthetic,
            )?;
            state.preview = Some(
                state
                    .session
                    .as_mut()
                    .ok_or("fixture missing")?
                    .preview(&sample)?,
            );
            state.stage = 4;
        }
        "confirm" => {
            let preview = state.preview.clone().ok_or("preview required")?;
            state
                .session
                .as_mut()
                .ok_or("fixture missing")?
                .approve(&preview)?;
            state.stage = 5;
        }
        "export" => {
            let draft = state.session.as_ref().ok_or("fixture missing")?.draft()?;
            let mut result = view(
                &state,
                "未検証draftを書き出せます。再読込では基準・readinessを復元しません。",
            );
            result.export = Some(serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1, "provenance": "synthetic fixture", "hardware_verified": draft.hardware_verified(), "control_ready": draft.control_ready(),
                "physical": {"id": draft.context.source.id.to_string(), "assembly_revision": draft.context.source.assembly, "firmware": draft.context.source.firmware,
                    "revision": draft.revision, "signs": draft.signs, "evidence": draft.evidence, "manual_override": draft.manual},
                "operation": {"user": draft.context.user, "posture": draft.context.posture, "epoch": draft.context.source.epoch,
                    "positive_spans": draft.positive_spans, "reverse_spans": draft.reverse_spans, "baseline_persisted": false}
            })).map_err(|e| e.to_string())?);
            return Ok(result);
        }
        "invalidate" => {
            if let Some(session) = state.session.as_mut() {
                session.invalidate();
            }
            state.preview = None;
            state.stage = 1;
        }
        _ => return Err("unknown offline action".into()),
    }
    Ok(view(
        &state,
        "合成fixtureのみ。実機校正済み・control readyにはなりません。",
    ))
}
pub fn run() {
    tauri::Builder::default()
        .manage(Mutex::new(State::default()))
        .invoke_handler(tauri::generate_handler![fixture_action])
        .run(tauri::generate_context!())
        .expect("Device Manager startup failed");
}
