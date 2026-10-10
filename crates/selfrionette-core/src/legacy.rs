//! 旧Sim 7ch producerの表示・測定。v2 identity / tareとは別の互換処理。
use crate::CHANNEL_COUNT;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DisplayLevel {
    Status,
    Warn,
    Vector,
}

impl DisplayLevel {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "status" => Some(Self::Status),
            "warn" => Some(Self::Warn),
            "vector" => Some(Self::Vector),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Warn => "warn",
            Self::Vector => "vector",
        }
    }
    fn for_line(line: &str) -> Self {
        if line.starts_with("vector,") {
            Self::Vector
        } else if line.starts_with("warn,") {
            Self::Warn
        } else {
            Self::Status
        }
    }
}

#[derive(Clone, Debug)]
pub struct MonitorConfig {
    pub duration_seconds: i32,
    pub send_text: String,
    pub calibrate: bool,
    pub display_level: DisplayLevel,
    pub paused_display_level: DisplayLevel,
}
impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            duration_seconds: 0,
            send_text: String::new(),
            calibrate: false,
            display_level: DisplayLevel::Vector,
            paused_display_level: DisplayLevel::Status,
        }
    }
}

#[derive(Default, Debug, PartialEq)]
pub struct MonitorEffect {
    pub write: Option<String>,
    pub output: Vec<String>,
    pub quit: bool,
}

pub struct MonitorState {
    config: MonitorConfig,
    paused: bool,
    pending_calibration: bool,
}
impl MonitorState {
    pub fn new(config: MonitorConfig) -> Self {
        Self {
            paused: config.calibrate,
            pending_calibration: config.calibrate,
            config,
        }
    }
    pub fn start(&self) -> MonitorEffect {
        let text = if self.config.calibrate {
            "c"
        } else {
            &self.config.send_text
        };
        if text.trim().is_empty() {
            MonitorEffect::default()
        } else {
            MonitorEffect {
                write: Some(text.to_owned()),
                output: vec![format!("Sent: {text}")],
                quit: false,
            }
        }
    }
    pub fn key(&mut self, key: char) -> MonitorEffect {
        let mut effect = MonitorEffect::default();
        match key.to_ascii_lowercase() {
            'p' => {
                self.paused = true;
                effect.output.push("[paused]".into());
            }
            'r' => {
                self.paused = false;
                effect.output.push("[resumed]".into());
            }
            'c' => {
                effect.write = Some("c".into());
                effect.output.push("[sent c]".into());
            }
            'q' => effect.quit = true,
            _ => {}
        }
        effect
    }
    pub fn line(&mut self, line: &str) -> MonitorEffect {
        let line = line.trim();
        let mut effect = MonitorEffect::default();
        if line.is_empty() {
            return effect;
        }
        let limit = if self.paused {
            self.config.paused_display_level
        } else {
            self.config.display_level
        };
        if DisplayLevel::for_line(line) <= limit {
            effect.output.push(line.into());
        }
        if line == "status,calibration_end" {
            effect.output.push("[calibration complete]".into());
            self.pending_calibration = false;
            if self.paused {
                self.paused = false;
                effect.output.push("[resumed]".into());
            }
        }
        if self.pending_calibration && line == "status,calibration_start" {
            effect.output.push("[calibration running]".into());
        }
        effect
    }
}

#[derive(Clone, Debug)]
pub struct MeasureConfig {
    pub baseline_seconds: i32,
    pub press_seconds: i32,
    pub sensor: u8,
    pub repeats: i32,
    pub all_sensors: bool,
}
impl Default for MeasureConfig {
    fn default() -> Self {
        Self {
            baseline_seconds: 3,
            press_seconds: 4,
            sensor: 1,
            repeats: 1,
            all_sensors: false,
        }
    }
}
impl MeasureConfig {
    pub fn sensors(&self) -> impl Iterator<Item = u8> {
        let all = self.all_sensors;
        let sensor = self.sensor;
        let count = if all { 7 } else { self.repeats.max(1) as usize };
        (0..count).map(move |index| if all { (index + 1) as u8 } else { sensor })
    }
}

/// Legacy timestampは元consumerのsigned int64を保持する。v2 parserを流用しない。
pub fn legacy_vector(line: &str) -> Option<[f64; CHANNEL_COUNT]> {
    let fields: Vec<_> = line.trim().split(',').collect();
    if fields.len() != 9 || fields[0] != "vector" || fields[1].parse::<i64>().is_err() {
        return None;
    }
    let mut values: [f64; CHANNEL_COUNT] = [0.0; CHANNEL_COUNT];
    for (value, field) in values.iter_mut().zip(&fields[2..]) {
        *value = field.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
    }
    Some(values)
}

#[derive(Clone, Debug, Default)]
pub struct ChannelMeans {
    sum: [f64; CHANNEL_COUNT],
    count: u64,
}
impl ChannelMeans {
    pub fn add(&mut self, values: [f64; CHANNEL_COUNT]) -> Result<(), &'static str> {
        let mut next = self.sum;
        for (sum, value) in next.iter_mut().zip(values) {
            *sum += value;
            if !sum.is_finite() {
                return Err("non-finite channel accumulator");
            }
        }
        self.sum = next;
        self.count = self.count.checked_add(1).ok_or("sample count overflow")?;
        Ok(())
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn means(&self) -> [f64; CHANNEL_COUNT] {
        if self.count == 0 {
            [0.0; CHANNEL_COUNT]
        } else {
            self.sum.map(|sum| sum / self.count as f64)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSummary {
    pub channel: usize,
    pub baseline_mean: f64,
    pub press_mean: f64,
    pub delta: f64,
    pub abs_delta: f64,
}
fn round_two(value: f64) -> f64 {
    if value.abs() >= 1e16 {
        value
    } else {
        (value * 100.0).round_ties_even() / 100.0
    }
}
pub fn summarize(baseline: &ChannelMeans, press: &ChannelMeans) -> Vec<ChannelSummary> {
    baseline
        .means()
        .into_iter()
        .zip(press.means())
        .enumerate()
        .map(|(channel, (base, press))| {
            let delta = round_two(press - base);
            ChannelSummary {
                channel,
                baseline_mean: round_two(base),
                press_mean: round_two(press),
                delta,
                abs_delta: delta.abs(),
            }
        })
        .collect()
}
pub fn strongest(summary: &[ChannelSummary]) -> Vec<ChannelSummary> {
    let mut sorted = summary.to_vec();
    sorted.sort_by(|a, b| b.abs_delta.total_cmp(&a.abs_delta));
    sorted
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_preserves_filters_commands_and_calibration_notifications() {
        let mut state = MonitorState::new(MonitorConfig {
            calibrate: true,
            send_text: "ignored".into(),
            ..Default::default()
        });
        assert_eq!(state.start().write.as_deref(), Some("c"));
        assert!(state.line("vector,1,1,2,3,4,5,6,7").output.is_empty());
        assert!(state.line("warn,spike,0,3").output.is_empty());
        assert_eq!(
            state.line("status,calibration_start").output,
            ["status,calibration_start", "[calibration running]"]
        );
        assert_eq!(
            state.line("status,calibration_end").output,
            [
                "status,calibration_end",
                "[calibration complete]",
                "[resumed]"
            ]
        );
        assert_eq!(state.key('c').write.as_deref(), Some("c"));
        state.key('p');
        assert!(state.line("vector,x").output.is_empty());
        state.key('r');
        assert_eq!(state.line("warn,x").output, ["warn,x"]);
        assert!(state.key('q').quit);
        assert_eq!(state.line("unknown,text").output, ["unknown,text"]);
    }
    #[test]
    fn display_level_is_inclusive_and_key_c_does_not_implicitly_pause() {
        let mut state = MonitorState::new(MonitorConfig {
            display_level: DisplayLevel::Warn,
            ..Default::default()
        });
        state.key('c');
        assert_eq!(state.line("warn,x").output, ["warn,x"]);
        assert!(state.line("vector,x").output.is_empty());
        assert_eq!(state.line("status,x").output, ["status,x"]);
    }
    #[test]
    fn measurement_golden_means_rounding_signed_delta_and_stable_ties() {
        let mut base = ChannelMeans::default();
        let mut press = ChannelMeans::default();
        base.add([1.0; 7]).unwrap();
        base.add([3.0; 7]).unwrap();
        press
            .add([12.0, -8.0, 2.0, 2.125, 2.375, 2.0, 2.0])
            .unwrap();
        let rows = summarize(&base, &press);
        assert_eq!(rows[0].baseline_mean, 2.0);
        assert_eq!(rows[1].delta, -10.0);
        assert_eq!(rows[3].delta, 0.12);
        assert_eq!(rows[4].delta, 0.38);
        assert_eq!(
            strongest(&rows)[..3]
                .iter()
                .map(|r| r.channel)
                .collect::<Vec<_>>(),
            [0, 1, 4]
        );
        assert_eq!(base.count(), 2);
    }
    #[test]
    fn legacy_rows_and_empty_phase_do_not_create_samples() {
        assert!(legacy_vector("vector,1,1,2,3,4,5,6,7\r\n").is_some());
        for line in [
            "status,calibration_end",
            "device,2,0.1,unprovisioned,7",
            "vector,1,NaN,2,3,4,5,6,7",
            "vector,1,1,2",
            "vector,x,1,2,3,4,5,6,7",
        ] {
            assert!(legacy_vector(line).is_none());
        }
        let means = ChannelMeans::default();
        assert_eq!(means.count(), 0);
        assert_eq!(means.means(), [0.0; 7]);
    }
    #[test]
    fn sweep_precedes_repeat_and_defaults_match_legacy() {
        let default = MeasureConfig::default();
        assert_eq!(default.sensors().collect::<Vec<_>>(), [1]);
        assert_eq!(
            MeasureConfig {
                sensor: 4,
                repeats: 3,
                ..default.clone()
            }
            .sensors()
            .collect::<Vec<_>>(),
            [4, 4, 4]
        );
        assert_eq!(
            MeasureConfig {
                all_sensors: true,
                repeats: 3,
                ..default
            }
            .sensors()
            .collect::<Vec<_>>(),
            [1, 2, 3, 4, 5, 6, 7]
        );
    }
    #[test]
    fn legacy_keys_are_case_insensitive() {
        let mut state = MonitorState::new(MonitorConfig::default());
        state.key('P');
        assert!(state.line("vector,x").output.is_empty());
        state.key('R');
        assert_eq!(state.line("vector,x").output, ["vector,x"]);
        assert_eq!(state.key('C').write.as_deref(), Some("c"));
        assert!(state.key('Q').quit);
    }
}
