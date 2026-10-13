//! Offline calibration preview. Firmware vectors are already negated/tared/filtered.
use crate::{CHANNEL_COUNT, DeviceId, VectorFrame};
pub const N: usize = CHANNEL_COUNT;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub id: DeviceId,
    pub firmware: String,
    pub assembly: String,
    pub epoch: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub source: Source,
    pub user: String,
    pub posture: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quality {
    KnownGoodSynthetic,
    Unknown,
    Reset,
    Spike,
    Timeout,
    Clipped,
}
#[derive(Clone, Debug)]
pub struct FirmwareSample {
    context: Context,
    values: [f64; N],
    timestamp_ms: u64,
    quality: Quality,
}
impl FirmwareSample {
    pub fn from_vector(
        context: Context,
        vector: &VectorFrame,
        quality: Quality,
    ) -> Result<Self, &'static str> {
        validate_context(&context)?;
        if vector.channels.iter().any(|v| !v.is_finite()) {
            return Err("non-finite firmware sample");
        }
        Ok(Self {
            context,
            values: vector.channels,
            timestamp_ms: u64::from(vector.timestamp_ms),
            quality,
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Assessment {
    Candidate { sign: i8, positive_span: f64 },
    Inconclusive(&'static str),
}
#[derive(Clone, Debug)]
pub struct Trial {
    pub baseline: Vec<[f64; N]>,
    pub loaded: Vec<[f64; N]>,
    pub released: Vec<[f64; N]>,
    pub quality: Quality,
    pub known_direction: Option<String>,
    pub crosstalk_known: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Thresholds {
    pub noise_floor: f64,
    pub stability: f64,
    pub signal_to_noise: f64,
    pub cross_ratio: f64,
}
pub fn assess(target: usize, trials: &[Trial], limits: Thresholds) -> Assessment {
    use Assessment::*;
    if target >= N || trials.len() < 2 || trials.len() > 8 {
        return Inconclusive("repeat/target bounds");
    }
    if ![
        limits.noise_floor,
        limits.stability,
        limits.signal_to_noise,
        limits.cross_ratio,
    ]
    .iter()
    .all(|v| v.is_finite() && *v > 0.0)
        || limits.signal_to_noise <= 1.0
        || limits.cross_ratio >= 1.0
    {
        return Inconclusive("invalid thresholds");
    }
    let mut sign = None;
    let mut span = f64::INFINITY;
    for trial in trials {
        if trial.quality != Quality::KnownGoodSynthetic {
            return Inconclusive("quality unverified or excluded");
        }
        if !trial.crosstalk_known {
            return Inconclusive("crosstalk unknown");
        }
        if trial
            .known_direction
            .as_ref()
            .is_none_or(|d| d.trim().is_empty())
        {
            return Inconclusive("known direction missing");
        }
        let phases = [&trial.baseline, &trial.loaded, &trial.released];
        if phases
            .iter()
            .any(|p| p.len() < 3 || p.len() > 64 || p.iter().flatten().any(|v| !v.is_finite()))
        {
            return Inconclusive("invalid/bounded phase samples");
        }
        let mean =
            |p: &Vec<[f64; N]>, ch: usize| p.iter().map(|s| s[ch]).sum::<f64>() / p.len() as f64;
        let range = |p: &Vec<[f64; N]>, ch: usize| {
            let min = p.iter().map(|s| s[ch]).fold(f64::INFINITY, f64::min);
            let max = p.iter().map(|s| s[ch]).fold(f64::NEG_INFINITY, f64::max);
            max - min
        };
        for ch in 0..N {
            let baseline_mean = mean(&trial.baseline, ch);
            let loaded_mean = mean(&trial.loaded, ch);
            let released_mean = mean(&trial.released, ch);
            if ![
                baseline_mean,
                loaded_mean,
                released_mean,
                loaded_mean - baseline_mean,
                released_mean - baseline_mean,
            ]
            .iter()
            .all(|v| v.is_finite())
            {
                return Inconclusive("phase mean/difference overflow");
            }
            if range(&trial.baseline, ch) > limits.stability
                || range(&trial.released, ch) > limits.stability
            {
                return Inconclusive("baseline/release unstable");
            }
            if (mean(&trial.baseline, ch) - mean(&trial.released, ch)).abs() > limits.stability {
                return Inconclusive("release did not return");
            }
        }
        let delta = mean(&trial.loaded, target) - mean(&trial.baseline, target);
        let noise = range(&trial.baseline, target).max(limits.noise_floor);
        if !delta.is_finite() || delta.abs() <= noise * limits.signal_to_noise {
            return Inconclusive("signal near noise");
        }
        for ch in 0..N {
            if ch != target
                && (mean(&trial.loaded, ch) - mean(&trial.baseline, ch)).abs()
                    > delta.abs() * limits.cross_ratio
            {
                return Inconclusive("coupled response requires mapping review");
            }
        }
        let candidate = if delta > 0.0 { 1 } else { -1 };
        if sign.is_some_and(|previous| previous != candidate) {
            return Inconclusive("repeat polarity disagreement");
        }
        sign = Some(candidate);
        span = span.min(delta.abs());
    }
    Candidate {
        sign: sign.unwrap(),
        positive_span: span,
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub timestamp_ms: u64,
    pub firmware_sample: [f64; N],
    pub baseline: [f64; N],
    pub delta: [f64; N],
    pub oriented: [f64; N],
    pub normalized: [Option<f64>; N],
    pub displayed: [Option<f64>; N],
    pub saturated: [bool; N],
    pub revision: u64,
    pub signs: [i8; N],
    pub positive_spans: [f64; N],
    pub evidence: [String; N],
    context: Context,
}
#[derive(Clone, Debug)]
pub struct CalibrationSession {
    context: Context,
    baseline: Option<[f64; N]>,
    signs: Option<[i8; N]>,
    spans: Option<[f64; N]>,
    reverse: [Option<f64>; N],
    evidence: [String; N],
    manual: [bool; N],
    revision: u64,
    approved: Option<Preview>,
    current_preview: Option<Preview>,
}
impl CalibrationSession {
    pub fn new(context: Context) -> Result<Self, &'static str> {
        validate_context(&context)?;
        Ok(Self {
            context,
            baseline: None,
            signs: None,
            spans: None,
            reverse: [None; N],
            evidence: std::array::from_fn(|_| String::new()),
            manual: [false; N],
            revision: 1,
            approved: None,
            current_preview: None,
        })
    }
    fn changed(&mut self) {
        self.revision += 1;
        self.approved = None;
        self.current_preview = None;
    }
    pub fn set_baseline(&mut self, baseline: [f64; N]) -> Result<(), &'static str> {
        if baseline.iter().any(|v| !v.is_finite()) {
            return Err("invalid baseline");
        }
        self.baseline = Some(baseline);
        self.signs = None;
        self.spans = None;
        self.manual = [false; N];
        self.changed();
        Ok(())
    }
    pub fn set_candidates(
        &mut self,
        candidates: [Assessment; N],
        evidence: [String; N],
    ) -> Result<(), &'static str> {
        self.signs = None;
        self.spans = None;
        self.manual = [false; N];
        self.changed();
        let mut signs = [1; N];
        let mut spans = [0.0; N];
        for ch in 0..N {
            let Assessment::Candidate {
                sign,
                positive_span,
            } = candidates[ch]
            else {
                return Err("inconclusive channel");
            };
            if ![-1, 1].contains(&sign)
                || !positive_span.is_finite()
                || positive_span <= 0.0
                || evidence[ch].trim().is_empty()
            {
                return Err("invalid sign/span/evidence");
            }
            signs[ch] = sign;
            spans[ch] = positive_span;
        }
        self.signs = Some(signs);
        self.spans = Some(spans);
        self.evidence = evidence;
        self.changed();
        Ok(())
    }
    pub fn manual_flip(&mut self, channel: usize, reason: &str) -> Result<(), &'static str> {
        if channel >= N || reason.trim().is_empty() {
            return Err("manual evidence required");
        }
        let signs = self
            .signs
            .as_mut()
            .ok_or("quality-qualified candidates required")?;
        signs[channel] *= -1;
        self.manual[channel] = true;
        self.evidence[channel].push_str(&format!("; manual override: {reason}"));
        self.changed();
        Ok(())
    }
    pub fn preview(&mut self, sample: &FirmwareSample) -> Result<Preview, &'static str> {
        self.approved = None;
        self.current_preview = None;
        if sample.context != self.context {
            return Err("source/context/epoch mismatch");
        }
        if sample.quality != Quality::KnownGoodSynthetic {
            return Err("quality unknown or excluded");
        }
        let baseline = self.baseline.ok_or("fresh session baseline required")?;
        let signs = self.signs.ok_or("unknown polarity profile")?;
        let spans = self.spans.ok_or("positive span required")?;
        let delta = std::array::from_fn(|i| sample.values[i] - baseline[i]);
        let oriented: [f64; N] = std::array::from_fn(|i| f64::from(signs[i]) * delta[i]);
        if oriented.iter().any(|v| !v.is_finite()) {
            return Err("correction overflow");
        }
        let normalized = std::array::from_fn(|i| {
            if oriented[i] >= 0.0 {
                Some(oriented[i] / spans[i])
            } else {
                self.reverse[i].map(|span| oriented[i] / span)
            }
        });
        if normalized.iter().flatten().any(|v| !v.is_finite()) {
            return Err("normalization overflow");
        }
        let preview = Preview {
            timestamp_ms: sample.timestamp_ms,
            firmware_sample: sample.values,
            baseline,
            delta,
            oriented,
            normalized,
            displayed: normalized.map(|v| v.map(|n| n.clamp(-1.0, 1.0))),
            saturated: normalized.map(|v| v.is_some_and(|n| n.abs() > 1.0)),
            revision: self.revision,
            signs,
            positive_spans: spans,
            evidence: self.evidence.clone(),
            context: self.context.clone(),
        };
        self.current_preview = Some(preview.clone());
        Ok(preview)
    }
    pub fn approve(&mut self, preview: &Preview) -> Result<(), &'static str> {
        if preview.revision != self.revision
            || preview.context != self.context
            || self.current_preview.as_ref() != Some(preview)
        {
            return Err("stale preview");
        }
        if preview.saturated.iter().any(|v| *v) {
            return Err("saturated preview");
        }
        self.approved = Some(preview.clone());
        Ok(())
    }
    pub fn revoke_preview(&mut self) {
        self.approved = None;
        self.current_preview = None;
    }
    pub fn invalidate(&mut self) {
        self.baseline = None;
        self.spans = None;
        self.manual = [false; N];
        self.changed();
    }
    pub fn draft(&self) -> Result<Draft, &'static str> {
        if self.approved.is_none() {
            return Err("preview confirmation required");
        }
        Ok(Draft {
            context: self.context.clone(),
            signs: self.signs.unwrap(),
            positive_spans: self.spans.unwrap(),
            reverse_spans: self.reverse,
            evidence: self.evidence.clone(),
            manual: self.manual,
            revision: self.revision,
            observation: Observation::from(self.approved.as_ref().unwrap()),
        })
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Observation {
    pub timestamp_ms: u64,
    pub firmware_sample: [f64; N],
    pub baseline: [f64; N],
    pub delta: [f64; N],
    pub oriented: [f64; N],
    pub normalized: [Option<f64>; N],
}
impl From<&Preview> for Observation {
    fn from(p: &Preview) -> Self {
        Self {
            timestamp_ms: p.timestamp_ms,
            firmware_sample: p.firmware_sample,
            baseline: p.baseline,
            delta: p.delta,
            oriented: p.oriented,
            normalized: p.normalized,
        }
    }
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub context: Context,
    pub signs: [i8; N],
    pub positive_spans: [f64; N],
    pub reverse_spans: [Option<f64>; N],
    pub evidence: [String; N],
    pub manual: [bool; N],
    pub revision: u64,
    pub observation: Observation,
}
impl Draft {
    pub const fn hardware_verified(&self) -> bool {
        false
    }
    pub const fn control_ready(&self) -> bool {
        false
    }
    pub fn reload(&self, fresh_context: Context) -> Result<CalibrationSession, &'static str> {
        // No baseline/readiness is recovered from persistence.
        CalibrationSession::new(fresh_context)
    }
}
fn validate_context(context: &Context) -> Result<(), &'static str> {
    if context.source.firmware.trim().is_empty()
        || context.source.assembly.trim().is_empty()
        || context.source.epoch == 0
        || context.user.trim().is_empty()
        || context.posture.trim().is_empty()
    {
        return Err("incomplete source/context");
    }
    Ok(())
}
pub fn validate_bindings(ids: &[DeviceId], physical_numbers: [u8; N]) -> Result<(), &'static str> {
    if ids.is_empty() || ids.len() > 2 || (ids.len() == 2 && ids[0] == ids[1]) {
        return Err("missing/duplicate ID");
    }
    for i in 0..N {
        if physical_numbers[i] == 0
            || physical_numbers[i..]
                .iter()
                .filter(|n| **n == physical_numbers[i])
                .count()
                != 1
        {
            return Err("unknown/duplicate physical number");
        }
    }
    Ok(())
}
