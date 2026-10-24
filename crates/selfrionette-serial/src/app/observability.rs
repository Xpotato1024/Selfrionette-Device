//! Bounded observations of the host receive path, independent of GUI polling.
use selfrionette_core::protocol_v3::{FLAG_FRESH, Sample};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> Instant;
}

pub struct SystemClock;
impl MonotonicClock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

pub(super) fn system_clock() -> Arc<dyn MonotonicClock> {
    Arc::new(SystemClock)
}

const RATE_WINDOW: Duration = Duration::from_secs(2);
const MAX_RATE_OBSERVATIONS: usize = 256;

#[derive(Clone, Debug, Default, Serialize)]
pub struct StreamDiagnostics {
    pub stream_ticks: u64,
    pub received_frames: u64,
    pub normalized_frames: u64,
    pub bad_quality_frames: u64,
    pub sequence_gaps: u64,
    pub stale_invalidations: u64,
    pub drain_budget_exhaustions: u64,
    pub last_tick_lines: usize,
    pub last_tick_samples: usize,
    pub receive_frame_hz: Option<f64>,
    pub observed_source_frame_hz: Option<f64>,
    pub fresh_frame_hz: [Option<f64>; 7],
    pub normalized_frame_hz: Option<f64>,
    pub usable_ratio: Option<f64>,
    /// Extra delay relative to the smallest observed device/host clock offset.
    /// Fixed transit delay present at the first observation remains unknown.
    pub relative_backlog_ms: Option<u64>,
    pub transport_latency_ms: Option<u64>,
    pub rate_window_samples: usize,
}

struct Observation {
    received: Instant,
    device_elapsed_ms: u64,
    sequence: u32,
    fresh: [bool; 7],
    usable: bool,
    normalized: bool,
}

pub(super) struct StreamMonitor {
    origin: Instant,
    previous_device_ms: Option<u32>,
    previous_sequence: Option<u32>,
    device_elapsed_ms: u64,
    minimum_offset_us: Option<i128>,
    observations: VecDeque<Observation>,
    counters: StreamDiagnostics,
}

impl StreamMonitor {
    pub fn new(origin: Instant) -> Self {
        Self {
            origin,
            previous_device_ms: None,
            previous_sequence: None,
            device_elapsed_ms: 0,
            minimum_offset_us: None,
            observations: VecDeque::with_capacity(MAX_RATE_OBSERVATIONS),
            counters: StreamDiagnostics::default(),
        }
    }

    pub fn elapsed_us(&self, now: Instant) -> u64 {
        now.saturating_duration_since(self.origin)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64
    }

    /// A pause creates a new continuous-rate interval, but keeps the host origin.
    pub fn reset(&mut self) {
        *self = Self::new(self.origin);
    }

    pub fn tick(&mut self, lines: usize, samples: usize, exhausted: bool) {
        self.counters.stream_ticks = self.counters.stream_ticks.saturating_add(1);
        self.counters.last_tick_lines = lines;
        self.counters.last_tick_samples = samples;
        if exhausted {
            self.counters.drain_budget_exhaustions =
                self.counters.drain_budget_exhaustions.saturating_add(1);
        }
    }

    pub fn stale(&mut self) {
        self.counters.stale_invalidations = self.counters.stale_invalidations.saturating_add(1);
    }

    pub fn observe(
        &mut self,
        sample: &Sample,
        received: Instant,
        usable: bool,
    ) -> Result<u64, &'static str> {
        if let Some(previous) = self.previous_device_ms {
            let delta = sample.device_ms.wrapping_sub(previous);
            if delta == 0 || delta >= (1_u32 << 31) {
                return Err("sample device timestamp repeated or moved backwards");
            }
            self.device_elapsed_ms = self.device_elapsed_ms.saturating_add(u64::from(delta));
        }
        if let Some(previous) = self.previous_sequence {
            let delta = sample.sequence.wrapping_sub(previous);
            self.counters.sequence_gaps = self
                .counters
                .sequence_gaps
                .saturating_add(u64::from(delta.saturating_sub(1)));
        }
        self.previous_device_ms = Some(sample.device_ms);
        self.previous_sequence = Some(sample.sequence);
        let host_us = self.elapsed_us(received);
        let offset = i128::from(host_us) - i128::from(self.device_elapsed_ms) * 1000;
        let minimum = self
            .minimum_offset_us
            .map_or(offset, |prior| prior.min(offset));
        self.minimum_offset_us = Some(minimum);
        let backlog_ms = ((offset - minimum) / 1000).min(i128::from(u64::MAX)) as u64;
        self.counters.relative_backlog_ms = Some(backlog_ms);
        self.counters.received_frames = self.counters.received_frames.saturating_add(1);
        if !usable {
            self.counters.bad_quality_frames = self.counters.bad_quality_frames.saturating_add(1);
        }
        while self.observations.len() >= MAX_RATE_OBSERVATIONS
            || self
                .observations
                .front()
                .is_some_and(|old| received.saturating_duration_since(old.received) > RATE_WINDOW)
        {
            self.observations.pop_front();
        }
        self.observations.push_back(Observation {
            received,
            device_elapsed_ms: self.device_elapsed_ms,
            sequence: sample.sequence,
            fresh: sample.flags.map(|flag| flag == FLAG_FRESH),
            usable,
            normalized: false,
        });
        Ok(backlog_ms)
    }

    pub fn normalized(&mut self) {
        self.counters.normalized_frames = self.counters.normalized_frames.saturating_add(1);
        if let Some(last) = self.observations.back_mut() {
            last.normalized = true;
        }
    }

    pub fn snapshot(&self, now: Instant) -> StreamDiagnostics {
        let mut result = self.counters.clone();
        result.rate_window_samples = self.observations.len();
        let Some(first) = self.observations.front() else {
            return result;
        };
        let Some(last) = self.observations.back() else {
            return result;
        };
        if now.saturating_duration_since(last.received) > RATE_WINDOW {
            return result;
        }
        result.usable_ratio = Some(
            self.observations.iter().filter(|item| item.usable).count() as f64
                / self.observations.len() as f64,
        );
        let seconds = last
            .received
            .saturating_duration_since(first.received)
            .as_secs_f64();
        if self.observations.len() >= 2 && seconds > 0.0 {
            result.receive_frame_hz = Some((self.observations.len() - 1) as f64 / seconds);
            result.fresh_frame_hz = std::array::from_fn(|axis| {
                Some(
                    self.observations
                        .iter()
                        .skip(1)
                        .filter(|item| item.fresh[axis])
                        .count() as f64
                        / seconds,
                )
            });
            result.normalized_frame_hz = Some(
                self.observations
                    .iter()
                    .skip(1)
                    .filter(|item| item.normalized)
                    .count() as f64
                    / seconds,
            );
        }
        let device_ms = last
            .device_elapsed_ms
            .saturating_sub(first.device_elapsed_ms);
        if device_ms > 0 {
            result.observed_source_frame_hz = Some(
                f64::from(last.sequence.wrapping_sub(first.sequence)) * 1000.0 / device_ms as f64,
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(sequence: u32, device_ms: u32) -> Sample {
        Sample {
            session: 1,
            sequence,
            device_ms,
            tare_epoch: 1,
            generation: 1,
            tare_valid: true,
            raw: [Some(0); 7],
            oriented: [Some(0); 7],
            flags: [FLAG_FRESH; 7],
        }
    }

    #[test]
    fn rates_are_measured_without_counting_gui_ticks_or_claiming_adc_conversions() {
        for hz in [20_u64, 40, 80] {
            let origin = Instant::now();
            let mut monitor = StreamMonitor::new(origin);
            for n in 0..=hz * 4 {
                let us = n * 1_000_000 / hz;
                monitor
                    .observe(
                        &sample(n as u32, (us / 1000) as u32),
                        origin + Duration::from_micros(us),
                        true,
                    )
                    .unwrap();
                monitor.normalized();
            }
            let observed = monitor.snapshot(origin + Duration::from_secs(4));
            assert!((observed.receive_frame_hz.unwrap() - hz as f64).abs() < 1e-9);
            assert!((observed.normalized_frame_hz.unwrap() - hz as f64).abs() < 1e-9);
            assert_eq!(observed.fresh_frame_hz, [Some(hz as f64); 7]);
            assert_eq!(observed.received_frames, hz * 4 + 1);
            assert_eq!(observed.sequence_gaps, 0);
            assert_eq!(observed.transport_latency_ms, None);
            assert!(observed.rate_window_samples <= MAX_RATE_OBSERVATIONS);
        }
    }

    #[test]
    fn gaps_backlog_rollover_and_bad_channels_remain_distinct() {
        let origin = Instant::now();
        let mut monitor = StreamMonitor::new(origin);
        monitor
            .observe(&sample(u32::MAX - 1, u32::MAX - 10), origin, true)
            .unwrap();
        let mut next = sample(1, 9);
        next.flags[2] = 8;
        assert_eq!(
            monitor
                .observe(&next, origin + Duration::from_millis(120), false)
                .unwrap(),
            100
        );
        let view = monitor.snapshot(origin + Duration::from_millis(120));
        assert_eq!(view.sequence_gaps, 2);
        assert_eq!(view.bad_quality_frames, 1);
        assert_eq!(view.fresh_frame_hz[2], Some(0.0));
        assert_eq!(view.usable_ratio, Some(0.5));
        assert!(monitor.observe(&sample(2, 9), origin, true).is_err());
        monitor.reset();
        assert_eq!(monitor.snapshot(origin).received_frames, 0);
    }
}
