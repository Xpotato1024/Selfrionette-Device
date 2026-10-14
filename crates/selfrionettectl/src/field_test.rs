//! Passive timed observation only. Never a calibration or control-readiness gate.
use crate::CliError;
use selfrionette_core::{DeviceInfo, ProtocolFrame, parse_line};
use selfrionette_serial::{
    LineTransport, SerialPortTransport, TransportError, available_port_names,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
const MAX_LOG_BYTES: usize = 64 * 1024 * 1024;
const MAX_LINES: usize = 100_000;
const STARTUP_MS: u64 = 30_000;
const SILENCE_MS: u64 = 1_000;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Axis {
    name: String,
    positive: String,
    negative: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema_version: u32,
    device_label: String,
    operator: String,
    posture: String,
    board: String,
    firmware_source: String,
    axes: [Axis; 7],
    combined_instruction: String,
}
impl Plan {
    fn template() -> Self {
        Self {
            schema_version: 1,
            device_label: String::new(),
            operator: String::new(),
            posture: String::new(),
            board: String::new(),
            firmware_source: "unknown".into(),
            axes: std::array::from_fn(|i| Axis {
                name: format!("操作軸{i}（ch番号ではない）"),
                positive: String::new(),
                negative: None,
            }),
            combined_instruction: String::new(),
        }
    }
    fn validate(&self, configured: bool) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported field-plan schema".into());
        }
        let mut fields = vec![
            &self.device_label,
            &self.operator,
            &self.posture,
            &self.board,
            &self.firmware_source,
            &self.combined_instruction,
        ];
        for axis in &self.axes {
            fields.extend([&axis.name, &axis.positive]);
            if let Some(reverse) = &axis.negative {
                fields.push(reverse);
            }
        }
        if fields.iter().any(|v| {
            v.len() > 512 || v.chars().any(char::is_control) || (configured && v.trim().is_empty())
        }) {
            return Err("fill plan fields with nonempty single-line text (512 UTF-8 bytes max) before capture".into());
        }
        for i in 0..7 {
            if self.axes[i + 1..]
                .iter()
                .any(|a| a.name == self.axes[i].name)
            {
                return Err("axis names must be unique".into());
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
struct Stage {
    label: String,
    instruction: String,
    start_ms: u64,
    end_ms: u64,
}
fn stages(plan: &Plan) -> Vec<Stage> {
    let mut out = Vec::new();
    fn add(out: &mut Vec<Stage>, label: &str, text: &str, seconds: u64) {
        let start_ms = out.last().map_or(0, |s| s.end_ms);
        out.push(Stage {
            label: label.into(),
            instruction: text.into(),
            start_ms,
            end_ms: start_ms + seconds * 1000,
        });
    }
    fn trial(out: &mut Vec<Stage>, label: &str, text: &str, strength: &str) {
        add(
            out,
            &format!("{label}/prepare"),
            &format!("次: {text} / {strength}。まだ入力しない"),
            3,
        );
        add(
            out,
            &format!("{label}/neutral"),
            "自然な操作姿勢で脱力。入力しない",
            3,
        );
        add(
            out,
            &format!("{label}/ramp"),
            &format!("開始: {text} / {strength}。穏やかに力を増やす"),
            2,
        );
        add(
            out,
            &format!("{label}/hold"),
            &format!("保持: {text} / {strength}"),
            4,
        );
        add(
            out,
            &format!("{label}/release"),
            "力を抜く。自然な操作姿勢を保つ",
            4,
        );
    }
    add(
        &mut out,
        "startup",
        "指を離して無荷重。接続時の再起動・ゼロ点処理を観測",
        45,
    );
    add(&mut out, "unloaded", "指を離して無荷重のまま", 30);
    add(
        &mut out,
        "operating-neutral",
        "いつもの操作姿勢で指を添え、操作するつもりの力を加えない",
        30,
    );
    for (i, axis) in plan.axes.iter().enumerate() {
        for repeat in 0..2 {
            trial(
                &mut out,
                &format!("train/axis{i}/positive/r{repeat}"),
                &axis.positive,
                "無理なく再現できる強度（最大筋力ではない）",
            );
            if let Some(reverse) = &axis.negative {
                trial(
                    &mut out,
                    &format!("train/axis{i}/negative/r{repeat}"),
                    reverse,
                    "無理なく再現できる強度（最大筋力ではない）",
                );
            }
        }
        add(
            &mut out,
            &format!("rest/axis{i}"),
            "休憩。指と装置の位置はなるべく変えない",
            10,
        );
    }
    for (i, axis) in plan.axes.iter().enumerate().rev() {
        trial(
            &mut out,
            &format!("validation/axis{i}/positive"),
            &axis.positive,
            "先ほどより弱い、おおむね半分のつもりの力",
        );
    }
    for repeat in 0..3 {
        trial(
            &mut out,
            &format!("validation/combined/r{repeat}"),
            &plan.combined_instruction,
            "無理のない同時入力",
        );
    }
    add(
        &mut out,
        "final-neutral",
        "同じ操作姿勢で脱力。最初の中立との差を観測",
        30,
    );
    out
}
#[derive(Clone)]
struct Stats {
    count: u64,
    mean: [f64; 7],
    m2: [f64; 7],
    min: [f64; 7],
    max: [f64; 7],
    first_ms: u64,
    last_ms: u64,
}
impl Default for Stats {
    fn default() -> Self {
        Self {
            count: 0,
            mean: [0.0; 7],
            m2: [0.0; 7],
            min: [0.0; 7],
            max: [0.0; 7],
            first_ms: 0,
            last_ms: 0,
        }
    }
}
impl Stats {
    fn add(&mut self, values: [f64; 7], ms: u64) -> Result<(), String> {
        if self.count == 0 {
            self.first_ms = ms;
            self.min = values;
            self.max = values;
        }
        self.last_ms = ms;
        self.count += 1;
        for (ch, value) in values.into_iter().enumerate() {
            self.min[ch] = self.min[ch].min(value);
            self.max[ch] = self.max[ch].max(value);
            let delta = value - self.mean[ch];
            self.mean[ch] += delta / self.count as f64;
            self.m2[ch] += delta * (value - self.mean[ch]);
        }
        if self.mean.iter().chain(&self.m2).any(|v| !v.is_finite()) {
            return Err("statistics overflow".into());
        }
        Ok(())
    }
    fn json(&self) -> Value {
        let stddev = (self.count > 1).then(|| {
            self.m2
                .map(|v| (v.max(0.0) / (self.count - 1) as f64).sqrt())
        });
        let rate = (self.count > 1 && self.last_ms > self.first_ms)
            .then(|| (self.count - 1) as f64 * 1000.0 / (self.last_ms - self.first_ms) as f64);
        json!({"samples":self.count,"mean":(self.count>0).then_some(self.mean),"stddev":stddev,
            "min":(self.count>0).then_some(self.min),"max":(self.count>0).then_some(self.max),"observed_line_rate_hz":rate})
    }
}
struct Log<W: Write> {
    out: W,
    bytes: usize,
}
impl<W: Write> Log<W> {
    fn new(out: W) -> Self {
        Self { out, bytes: 0 }
    }
    fn event(&mut self, value: Value) -> Result<(), String> {
        let mut bytes = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        bytes.push(b'\n');
        if bytes.len() > MAX_LOG_BYTES.saturating_sub(self.bytes) {
            return Err("64 MiB log budget reached".into());
        }
        self.out.write_all(&bytes).map_err(|e| e.to_string())?;
        self.bytes += bytes.len();
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        self.out.flush().map_err(|e| e.to_string())
    }
}
struct Observation {
    stats: Vec<Stats>,
    vectors: u64,
    warnings: u64,
    unknown: u64,
    duplicates: u64,
    gaps_over_200ms: u64,
    last_device_ms: Option<u32>,
    last_advance_host_ms: Option<u64>,
    info: Option<DeviceInfo>,
}
impl Observation {
    fn new(count: usize) -> Self {
        Self {
            stats: vec![Stats::default(); count],
            vectors: 0,
            warnings: 0,
            unknown: 0,
            duplicates: 0,
            gaps_over_200ms: 0,
            last_device_ms: None,
            last_advance_host_ms: None,
            info: None,
        }
    }
    fn observe(
        &mut self,
        line: &str,
        host_ms: u64,
        stage: usize,
        startup: bool,
    ) -> Result<Value, String> {
        let frame = parse_line(line).map_err(|e| format!("malformed frame: {e}"))?;
        match frame {
            ProtocolFrame::Vector(v) => {
                if let Some(previous) = self.last_device_ms {
                    let dt = v.timestamp_ms.wrapping_sub(previous);
                    if dt >= (1u32 << 31) {
                        return Err("device clock regressed/reset; start a NEW session".into());
                    }
                    if dt == 0 {
                        self.duplicates += 1;
                    } else {
                        if dt > 200 {
                            self.gaps_over_200ms += 1;
                        }
                        self.last_advance_host_ms = Some(host_ms);
                    }
                } else {
                    self.last_advance_host_ms = Some(host_ms);
                }
                self.last_device_ms = Some(v.timestamp_ms);
                self.vectors += 1;
                self.stats[stage].add(v.channels, host_ms)?;
                Ok(
                    json!({"frame":"vector","device_timestamp_ms":v.timestamp_ms,"values":v.channels,
                    "quality":"unknown","quality_reason":"ordinary vector does not establish per-sample sensor health"}),
                )
            }
            ProtocolFrame::Warning(w) => {
                self.warnings += 1;
                Ok(json!({"frame":"warning","token":w.token,"args":w.args}))
            }
            ProtocolFrame::Status(s) => {
                if !startup
                    && (s.token.starts_with("setup_")
                        || s.token == "calibration_start"
                        || s.token == "sensor_init_start")
                {
                    return Err(format!("measurement basis changed: {}", s.token));
                }
                Ok(json!({"frame":"status","token":s.token,"args":s.args}))
            }
            ProtocolFrame::Device(info) => {
                info.validate_compatibility().map_err(|e| e.to_string())?;
                if self.info.as_ref().is_some_and(|old| old != &info) {
                    return Err("device metadata changed".into());
                }
                let value = json!({"frame":"device","identity":info.identity.to_string(),"firmware":info.firmware_version});
                self.info = Some(info);
                Ok(value)
            }
            ProtocolFrame::Unknown(_) => {
                self.unknown += 1;
                Ok(json!({"frame":"unknown"}))
            }
        }
    }
    fn report(&self, stages: &[Stage], outcome: &str, error: Option<&str>, origin: &str) -> Value {
        let rows: Vec<_> = stages
            .iter()
            .zip(&self.stats)
            .map(|(s, v)| json!({"stage":s,"statistics":v.json()}))
            .collect();
        json!({"schema_version":1,"result_kind":"field_observation_not_calibration","origin":origin,"outcome":outcome,"error":error,
            "hardware_verified":false,"control_ready":false,"vectors":self.vectors,"warnings":self.warnings,"unknown_lines":self.unknown,
            "duplicate_device_timestamps":self.duplicates,"device_gaps_over_200ms":self.gaps_over_200ms,
            "device_identity":self.info.as_ref().map(|i|i.identity.to_string()),"device_firmware":self.info.as_ref().map(|i|i.firmware_version.clone()),
            "stages":rows,"notice":"Firmware-processed counts, not raw ADC/Newtons. Host receive/cue times, not measured intent or latency. No usable-quality conversion."})
    }
}

fn collect<T: LineTransport, W: Write>(
    transport: &mut T,
    stages: &[Stage],
    log: &mut Log<W>,
    observation: &mut Observation,
    mut now: impl FnMut() -> u64,
    mut cancelled: impl FnMut() -> bool,
    mut prompt: impl FnMut(&Stage) -> Result<(), String>,
) -> Result<(), String> {
    let end = stages.last().ok_or("empty schedule")?.end_ms;
    let mut current = usize::MAX;
    for _ in 0..MAX_LINES {
        if cancelled() {
            return Err("operator interrupted".into());
        }
        let ms = now();
        if ms >= end {
            return Ok(());
        }
        let stage = stages
            .iter()
            .position(|s| ms < s.end_ms)
            .ok_or("invalid schedule")?;
        if stage != current {
            current = stage;
            prompt(&stages[stage])?;
            log.event(
                json!({"event":"cue","host_ms":ms,"stage_index":stage,"stage":stages[stage]}),
            )?;
            log.flush()?;
        }
        let line = match transport.read_line(Duration::from_millis(1000)) {
            Ok(line) => line,
            // Initial alignment/startup only. Existing transport does not retain
            // a partial line across timeouts, so never retry after a valid vector.
            Err(TransportError::Timeout) if observation.vectors == 0 && ms < STARTUP_MS => continue,
            Err(e) => return Err(format!("read stopped: {e}")),
        };
        let received = now();
        let actual_stage = stages
            .iter()
            .position(|s| received < s.end_ms)
            .unwrap_or(stages.len() - 1);
        log.event(
            json!({"event":"wire","host_ms":received,"stage_index":actual_stage,"line":line}),
        )?;
        let result = observation.observe(&line, received, actual_stage, actual_stage == 0)?;
        log.event(
            json!({"event":"parsed","host_ms":received,"stage_index":actual_stage,"data":result}),
        )?;
        if observation.vectors == 0 && received >= STARTUP_MS {
            return Err("no vector within 30 seconds".into());
        }
        if observation
            .last_advance_host_ms
            .is_some_and(|t| received.saturating_sub(t) > SILENCE_MS)
        {
            return Err("no advancing device timestamp for one second".into());
        }
    }
    Err("100000-line capture budget reached".into())
}
fn load_plan(path: &str, configured: bool) -> Result<Plan, String> {
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() || metadata.len() > 32768 {
        return Err("plan must be a regular file <=32 KiB".into());
    }
    let mut data = Vec::new();
    File::open(path)
        .map_err(|e| e.to_string())?
        .take(32769)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() > 32768 {
        return Err("plan too large".into());
    }
    let plan: Plan = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    plan.validate(configured)?;
    Ok(plan)
}
fn write_text_new(path: &Path, text: &str) -> Result<(), String> {
    let mut f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    f.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())
}
fn write_new(path: &Path, value: &Value) -> Result<(), String> {
    write_text_new(
        path,
        &(serde_json::to_string_pretty(value).map_err(|e| e.to_string())? + "\n"),
    )
}
impl Observation {
    fn review(&self, stages: &[Stage], outcome: &str) -> String {
        let mut text = format!(
            "# 実機観測サマリー\n\n結果: {outcome}。校正済み・利用許可の判定ではありません。\n\nvector {} / warn {} / unknown {} / 重複timestamp {} / device時刻差200 ms超 {}\n\n",
            self.vectors, self.warnings, self.unknown, self.duplicates, self.gaps_over_200ms
        );
        let find = |name: &str| {
            stages
                .iter()
                .position(|s| s.label == name)
                .map(|i| &self.stats[i])
        };
        let first = find("operating-neutral").unwrap();
        let last = find("final-neutral").unwrap();
        text.push_str("## 操作姿勢での中立\n\n|ch|初期平均|標準偏差|最終平均|最終−初期|\n|---|---:|---:|---:|---:|\n");
        for ch in 0..7 {
            if first.count > 1 && last.count > 0 {
                text.push_str(&format!(
                    "|{ch}|{:.2}|{:.2}|{:.2}|{:.2}|\n",
                    first.mean[ch],
                    (first.m2[ch] / (first.count - 1) as f64).max(0.0).sqrt(),
                    last.mean[ch],
                    last.mean[ch] - first.mean[ch]
                ));
            } else {
                text.push_str(&format!("|{ch}|未取得|未取得|未取得|未取得|\n"));
            }
        }
        text.push_str("\n## 保持区間の応答（同じtrialの直前中立平均との差）\n\nFirmware処理後のcount値です。感度未校正なので物理的な力の比ではありません。指示通りに入力できなかった区間をnotes.txtへ記載してください。\n\n|操作区間|ch0|ch1|ch2|ch3|ch4|ch5|ch6|\n|---|---:|---:|---:|---:|---:|---:|---:|\n");
        for (i, stage) in stages.iter().enumerate() {
            if let Some(base) = stage.label.strip_suffix("/hold") {
                let neutral = find(&format!("{base}/neutral")).unwrap();
                let hold = &self.stats[i];
                text.push_str(&format!("|{base}|"));
                for ch in 0..7 {
                    if neutral.count > 0 && hold.count > 0 {
                        text.push_str(&format!("{:.2}|", hold.mean[ch] - neutral.mean[ch]));
                    } else {
                        text.push_str("未取得|");
                    }
                }
                text.push('\n');
            }
        }
        text.push_str("\n詳細はevents.jsonlとsummary.json。品質はunknownのまま。指示時刻は人の入力開始・反応時間の測定ではありません。\n");
        text
    }
}
fn usage() -> CliError {
    CliError::Usage("field-test template <new-plan.json> | preview <plan.json> | capture <plan.json> --port <port> --out <NEW-directory> --yes-open".into())
}
pub(crate) fn run(args: &[String]) -> Result<(), CliError> {
    let fail = |e: String| CliError::Failure(format!("field-test: {e}"));
    match args {
        [cmd, path] if cmd == "template" => {
            write_new(Path::new(path), &json!(Plan::template())).map_err(fail)
        }
        [cmd, path] if cmd == "preview" => {
            let plan = load_plan(path, false).map_err(fail)?;
            let schedule = stages(&plan);
            println!(
                "OFFLINE preview: {} seconds. No port opened. Plan configured: {}",
                schedule.last().unwrap().end_ms / 1000,
                plan.validate(true).is_ok()
            );
            for s in schedule {
                println!(
                    "{:4}-{:4}s | {} | {}",
                    s.start_ms / 1000,
                    s.end_ms / 1000,
                    s.label,
                    s.instruction
                );
            }
            Ok(())
        }
        [cmd, path, pf, port, of, out, yes]
            if cmd == "capture"
                && pf == "--port"
                && of == "--out"
                && yes == "--yes-open"
                && !port.is_empty()
                && !port.starts_with('-') =>
        {
            let plan = load_plan(path, true).map_err(fail)?;
            if Path::new(out).exists() {
                return Err(fail(
                    "output directory exists; choose a NEW session name".into(),
                ));
            }
            let ports = available_port_names().map_err(|e| fail(e.to_string()))?;
            if !ports.iter().any(|p| p == port) {
                return Err(fail("selected port is not in current OS list".into()));
            }
            let stopped = Arc::new(AtomicBool::new(false));
            let signal = Arc::clone(&stopped);
            ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
                .map_err(|e| fail(e.to_string()))?;
            fs::create_dir(out).map_err(|e| fail(e.to_string()))?;
            let dir = Path::new(out);
            let schedule = stages(&plan);
            let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(dir.join("events.jsonl"))
                .map_err(|e| fail(e.to_string()))?;
            let mut log = Log::new(BufWriter::new(file));
            log.event(json!({"event":"header","schema_version":1,"origin":"recorded","plan":plan,"port":port,"baud":115200,
                "unix_start_s":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|fail(e.to_string()))?.as_secs(),
                "host_version":env!("CARGO_PKG_VERSION"),"stages":schedule,"quality":"unknown",
                "note":"Passive read only; opening a port may reset hardware. Physical label is not device ID. Retain executable SHA-256."})).map_err(fail)?;
            log.flush().map_err(fail)?;
            println!(
                "対象: {} / {}。ポートを開きます。指を離してください。",
                plan.device_label, port
            );
            println!("送信・tare・書込みは行いません。Ctrl+Cで中止。出力: {out}");
            let mut observation = Observation::new(schedule.len());
            let result = (|| {
                if stopped.load(Ordering::SeqCst) {
                    return Err("interrupted before open".into());
                }
                let mut transport = SerialPortTransport::open(port).map_err(|e| e.to_string())?;
                let start = Instant::now();
                collect(
                    &mut transport,
                    &schedule,
                    &mut log,
                    &mut observation,
                    || start.elapsed().as_millis() as u64,
                    || stopped.load(Ordering::SeqCst),
                    |s| {
                        println!(
                            "\x07[{:02}:{:02}] {} ({}秒)",
                            s.start_ms / 60000,
                            (s.start_ms / 1000) % 60,
                            s.instruction,
                            (s.end_ms - s.start_ms) / 1000
                        );
                        Ok(())
                    },
                )
            })();
            let outcome = if result.is_err() {
                "aborted"
            } else {
                "completed_observation"
            };
            let report = observation.report(
                &schedule,
                outcome,
                result.as_ref().err().map(String::as_str),
                "recorded",
            );
            log.event(json!({"event":"end","outcome":outcome,"error":result.as_ref().err()}))
                .map_err(fail)?;
            log.flush().map_err(fail)?;
            write_new(&dir.join("summary.json"), &report).map_err(fail)?;
            write_text_new(
                &dir.join("review.md"),
                &observation.review(&schedule, outcome),
            )
            .map_err(fail)?;
            write_text_new(&dir.join("notes.txt"),"指・方向と操作軸0～6の対応:\n写真ファイル名:\n指示と違う入力になった区間:\n再試行する区間:\n痛み・疲労・姿勢変化:\n").map_err(fail)?;
            println!(
                "{outcome}: {} vectors / {} warnings。校正済みとは判定しません。",
                observation.vectors, observation.warnings
            );
            result.map_err(fail)
        }
        _ => Err(usage()),
    }
}
#[cfg(test)]
mod tests;
