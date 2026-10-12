//! 旧Sim CLIのraw commandとline stream。v2管理sessionとは別経路。
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use crate::{MAX_LINE_BYTES, TransportError};
use selfrionette_core::legacy::{
    ChannelMeans, ChannelSummary, MeasureConfig, MonitorConfig, MonitorEffect, MonitorState,
    legacy_vector, strongest, summarize,
};
use serialport::SerialPort;

pub const POLL_SLICE: Duration = Duration::from_millis(20);

pub trait LegacyIo {
    fn read(&mut self, timeout: Duration) -> Result<Option<String>, String>;
    fn write(&mut self, text: &str) -> Result<(), String>;
}
pub trait Clock {
    fn now(&self) -> Duration;
    fn pause(&mut self, duration: Duration);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Character(char),
    Enter,
    Interrupted,
    EndOfInput,
}
pub trait Operator {
    fn poll_key(&mut self) -> Result<Option<Key>, String>;
    fn interrupted(&self) -> bool;
    fn poll_interrupt(&mut self) -> Result<bool, String> {
        Ok(self.interrupted())
    }
    fn output(&mut self, text: &str) -> Result<(), String>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    Interrupted,
    Failure(String),
}
impl From<String> for RunError {
    fn from(value: String) -> Self {
        Self::Failure(value)
    }
}

pub struct LegacySerialTransport {
    port: Box<dyn SerialPort>,
    pending: Vec<u8>,
}
impl LegacySerialTransport {
    pub fn open(name: &str, baud: u32) -> Result<Self, TransportError> {
        let mut port = serialport::new(name, baud)
            .timeout(POLL_SLICE)
            .open()
            .map_err(|e| TransportError::Open {
                port_name: name.into(),
                message: e.to_string(),
            })?;
        port.write_data_terminal_ready(true)
            .map_err(|e| TransportError::Serial(e.to_string()))?;
        port.write_request_to_send(true)
            .map_err(|e| TransportError::Serial(e.to_string()))?;
        Ok(Self {
            port,
            pending: Vec::with_capacity(128),
        })
    }
}
impl LegacyIo for LegacySerialTransport {
    fn write(&mut self, text: &str) -> Result<(), String> {
        self.port
            .write_all(text.as_bytes())
            .and_then(|()| self.port.flush())
            .map_err(|e| e.to_string())
    }
    fn read(&mut self, timeout: Duration) -> Result<Option<String>, String> {
        let start = Instant::now();
        while let Some(remaining) = timeout.checked_sub(start.elapsed()) {
            if remaining.is_zero() {
                break;
            }
            self.port
                .set_timeout(remaining.min(POLL_SLICE))
                .map_err(|e| e.to_string())?;
            let mut byte = [0];
            match self.port.read(&mut byte) {
                Ok(0) => {}
                Ok(_) if byte[0] == b'\r' => {}
                Ok(_) if byte[0] == b'\n' => {
                    let line = std::mem::take(&mut self.pending);
                    return String::from_utf8(line)
                        .map(Some)
                        .map_err(|_| TransportError::InvalidUtf8.to_string());
                }
                Ok(_) => {
                    if self.pending.len() >= MAX_LINE_BYTES {
                        return Err(TransportError::LineTooLong {
                            max_bytes: MAX_LINE_BYTES,
                        }
                        .to_string());
                    }
                    self.pending.push(byte[0]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(None)
    }
}

fn check_stop(operator: &impl Operator) -> Result<(), RunError> {
    if operator.interrupted() {
        Err(RunError::Interrupted)
    } else {
        Ok(())
    }
}
fn settle(clock: &mut impl Clock, operator: &mut impl Operator) -> Result<(), RunError> {
    let until = clock.now() + Duration::from_secs(1);
    while clock.now() < until {
        check_stop(operator)?;
        if operator.poll_interrupt()? {
            return Err(RunError::Interrupted);
        }
        clock.pause(POLL_SLICE.min(until.saturating_sub(clock.now())));
    }
    // 最後のpause中に到着したraw Ctrl+Cも、startup writeより先に回収する。
    check_stop(operator)?;
    if operator.poll_interrupt()? {
        return Err(RunError::Interrupted);
    }
    Ok(())
}
fn effect(
    io: &mut impl LegacyIo,
    operator: &mut impl Operator,
    effect: MonitorEffect,
) -> Result<bool, RunError> {
    if let Some(text) = effect.write {
        io.write(&text)?;
    }
    for text in effect.output {
        operator.output(&text)?;
    }
    Ok(effect.quit)
}
/// ioを所有するため、正常・error・interruptの全returnでserial handleをdropする。
pub fn monitor(
    mut io: impl LegacyIo,
    clock: &mut impl Clock,
    operator: &mut impl Operator,
    config: MonitorConfig,
) -> Result<(), RunError> {
    settle(clock, operator)?;
    let deadline = (config.duration_seconds > 0)
        .then(|| clock.now() + Duration::from_secs(config.duration_seconds as u64));
    operator.output("Keys: p=pause, r=resume, c=send calibration, q=quit.")?;
    operator.output(&format!(
        "Display levels: normal<={}, paused<={}",
        config.display_level.name(),
        config.paused_display_level.name()
    ))?;
    let mut state = MonitorState::new(config);
    effect(&mut io, operator, state.start())?;
    loop {
        check_stop(operator)?;
        if deadline.is_some_and(|until| clock.now() >= until) {
            return Ok(());
        }
        while let Some(key) = operator.poll_key()? {
            match key {
                Key::Interrupted => return Err(RunError::Interrupted),
                Key::Character(key) => {
                    if effect(&mut io, operator, state.key(key))? {
                        return Ok(());
                    }
                }
                Key::Enter | Key::EndOfInput => {}
            }
        }
        let timeout = deadline.map_or(POLL_SLICE, |until| {
            POLL_SLICE.min(until.saturating_sub(clock.now()))
        });
        if let Some(line) = io.read(timeout)? {
            effect(&mut io, operator, state.line(&line))?;
        }
    }
}

fn wait_enter(
    clock: &mut impl Clock,
    operator: &mut impl Operator,
    prompt: &str,
) -> Result<(), RunError> {
    operator.output(prompt)?;
    loop {
        check_stop(operator)?;
        match operator.poll_key()? {
            Some(Key::Enter) => return Ok(()),
            Some(Key::Interrupted) => return Err(RunError::Interrupted),
            Some(Key::EndOfInput) => {
                return Err(RunError::Failure(
                    "stdin ended before Enter confirmation".into(),
                ));
            }
            _ => clock.pause(POLL_SLICE),
        }
    }
}
fn phase(
    io: &mut impl LegacyIo,
    clock: &mut impl Clock,
    operator: &mut impl Operator,
    label: &str,
    seconds: i32,
) -> Result<ChannelMeans, RunError> {
    operator.output(&format!("\n[{label}] {seconds} seconds"))?;
    let deadline = clock.now() + Duration::from_secs(seconds.max(0) as u64);
    let mut means = ChannelMeans::default();
    while clock.now() < deadline {
        check_stop(operator)?;
        // raw terminalのCtrl+Cはsignalではなくkey eventになる。
        if operator.poll_interrupt()? {
            return Err(RunError::Interrupted);
        }
        if let Some(line) = io.read(POLL_SLICE.min(deadline.saturating_sub(clock.now())))? {
            let line = line.trim();
            if let Some(values) = legacy_vector(line) {
                means.add(values).map_err(|e| RunError::Failure(e.into()))?;
            } else if line.starts_with("status,") || line.starts_with("warn,") {
                operator.output(line)?;
            } else if line.starts_with("vector,") {
                operator.output("[ignored malformed legacy vector]")?;
            }
        }
    }
    if means.count() == 0 {
        operator.output("[no samples: legacy summary zero is not a measured sensor value]")?;
    }
    Ok(means)
}
fn show_rows(
    operator: &mut impl Operator,
    rows: &[ChannelSummary],
    title: &str,
) -> Result<(), RunError> {
    operator.output(title)?;
    operator.output("Channel BaselineMean PressMean Delta AbsDelta")?;
    for r in rows {
        operator.output(&format!(
            "{} {:.2} {:.2} {:.2} {:.2}",
            r.channel, r.baseline_mean, r.press_mean, r.delta, r.abs_delta
        ))?;
    }
    Ok(())
}
pub fn measure(
    mut io: impl LegacyIo,
    clock: &mut impl Clock,
    operator: &mut impl Operator,
    config: MeasureConfig,
) -> Result<(), RunError> {
    settle(clock, operator)?;
    wait_enter(
        clock,
        operator,
        "Step 1: keep all load cells untouched, then press Enter.",
    )?;
    let baseline = phase(
        &mut io,
        clock,
        operator,
        "baseline",
        config.baseline_seconds,
    )?;
    let mut results = Vec::new();
    for (index, sensor) in config.sensors().enumerate() {
        if !config.all_sensors && config.repeats > 1 {
            operator.output(&format!("Repeat {}/{}", index + 1, config.repeats))?;
        }
        wait_enter(
            clock,
            operator,
            &format!("Step: press load cell #{sensor}, then press Enter."),
        )?;
        let press = phase(
            &mut io,
            clock,
            operator,
            &format!("press#{sensor}"),
            config.press_seconds,
        )?;
        let rows = summarize(&baseline, &press);
        let ranked = strongest(&rows);
        show_rows(operator, &rows, "=== Channel summary ===")?;
        show_rows(operator, &ranked[..3], "=== Strongest responses ===")?;
        results.push((sensor, ranked[0].clone()));
    }
    if config.all_sensors || config.repeats > 1 {
        operator.output(if config.all_sensors {
            "=== Final mapping summary ==="
        } else {
            "=== Repeated mapping summary ==="
        })?;
        operator.output("Sensor Channel AbsDelta Delta")?;
        for (sensor, row) in results {
            operator.output(&format!(
                "{sensor} {} {:.2} {:.2}",
                row.channel, row.abs_delta, row.delta
            ))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, collections::VecDeque, rc::Rc};
    struct FakeClock(Rc<Cell<Duration>>);
    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.0.get()
        }
        fn pause(&mut self, d: Duration) {
            self.0.set(self.now() + d);
        }
    }
    struct FakeIo {
        time: Rc<Cell<Duration>>,
        lines: VecDeque<Result<Option<String>, String>>,
        writes: Rc<std::cell::RefCell<Vec<String>>>,
        dropped: Rc<Cell<bool>>,
    }
    impl LegacyIo for FakeIo {
        fn read(&mut self, d: Duration) -> Result<Option<String>, String> {
            self.time.set(self.time.get() + d);
            self.lines.pop_front().unwrap_or(Ok(None))
        }
        fn write(&mut self, s: &str) -> Result<(), String> {
            self.writes.borrow_mut().push(s.into());
            Ok(())
        }
    }
    impl Drop for FakeIo {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    #[derive(Default)]
    struct FakeOperator {
        keys: VecDeque<Option<Key>>,
        output: Vec<String>,
        stopped: bool,
        interrupt_at: Option<(Rc<Cell<Duration>>, Duration)>,
    }
    impl Operator for FakeOperator {
        fn poll_key(&mut self) -> Result<Option<Key>, String> {
            Ok(self.keys.pop_front().flatten())
        }
        fn interrupted(&self) -> bool {
            self.stopped
        }
        fn poll_interrupt(&mut self) -> Result<bool, String> {
            if self
                .interrupt_at
                .as_ref()
                .is_some_and(|(time, at)| time.get() >= *at)
            {
                return Ok(true);
            }
            if self.keys.front() == Some(&Some(Key::Interrupted)) {
                self.keys.pop_front();
                return Ok(true);
            }
            Ok(self.stopped)
        }

        fn output(&mut self, s: &str) -> Result<(), String> {
            self.output.push(s.into());
            Ok(())
        }
    }
    fn io(clock: &FakeClock) -> FakeIo {
        FakeIo {
            time: clock.0.clone(),
            lines: VecDeque::new(),
            writes: Rc::default(),
            dropped: Rc::default(),
        }
    }
    #[test]
    fn monitor_times_out_and_releases_without_newline_or_v2_query() {
        let mut clock = FakeClock(Rc::default());
        let io = io(&clock);
        let drop = io.dropped.clone();
        let writes = io.writes.clone();
        let mut op = FakeOperator::default();
        monitor(
            io,
            &mut clock,
            &mut op,
            MonitorConfig {
                duration_seconds: 1,
                calibrate: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(drop.get());
        assert_eq!(*writes.borrow(), ["c"]);
        assert_eq!(clock.now(), Duration::from_secs(2));
    }
    #[test]
    fn quit_error_and_interrupt_all_release_transport() {
        for mode in [0, 1, 2, 3] {
            let mut clock = FakeClock(Rc::default());
            let mut io = io(&clock);
            let drop = io.dropped.clone();
            let mut op = FakeOperator::default();
            match mode {
                0 => op.keys.push_back(Some(Key::Character('q'))),
                1 => io.lines.push_back(Err("disconnected".into())),
                2 => op.keys.push_back(Some(Key::Interrupted)),
                _ => op.stopped = true,
            }
            let result = monitor(io, &mut clock, &mut op, MonitorConfig::default());
            assert!(drop.get());
            assert_eq!(result.is_ok(), mode == 0);
            if mode >= 2 {
                assert_eq!(result, Err(RunError::Interrupted));
            }
        }
    }
    #[test]
    fn measure_repeats_and_sweep_keep_summaries_with_fake_clock_and_enter() {
        for all in [false, true] {
            let mut clock = FakeClock(Rc::default());
            let io = io(&clock);
            let drop = io.dropped.clone();
            let mut op = FakeOperator::default();
            let count = if all { 7 } else { 3 };
            op.keys = vec![Some(Key::Enter); count + 1].into();
            measure(
                io,
                &mut clock,
                &mut op,
                MeasureConfig {
                    baseline_seconds: 0,
                    press_seconds: 0,
                    sensor: 4,
                    repeats: 3,
                    all_sensors: all,
                },
            )
            .unwrap();
            assert!(drop.get());
            assert_eq!(
                op.output
                    .iter()
                    .filter(|s| s.as_str() == "=== Channel summary ===")
                    .count(),
                count
            );
            assert!(op.output.iter().any(|s| s.contains("no samples")));
            assert!(op.output.iter().any(|s| s.contains(if all {
                "Final mapping"
            } else {
                "Repeated mapping"
            })));
        }
    }
    #[test]
    fn measurement_interrupt_and_eof_during_confirmation_release() {
        for key in [Key::Interrupted, Key::EndOfInput] {
            let mut clock = FakeClock(Rc::default());
            let io = io(&clock);
            let drop = io.dropped.clone();
            let mut op = FakeOperator::default();
            op.keys.push_back(Some(key));
            assert!(measure(io, &mut clock, &mut op, MeasureConfig::default()).is_err());
            assert!(drop.get());
        }
    }
    #[test]
    fn measure_nonzero_windows_preserve_7ch_means_signed_response_and_enter_queue() {
        let mut clock = FakeClock(Rc::default());
        let mut io = io(&clock);
        let dropped = io.dropped.clone();
        io.lines
            .push_back(Ok(Some("vector,1,2,2,2,2,2,2,2".into())));
        io.lines.extend((0..49).map(|_| Ok(None)));
        io.lines
            .push_back(Ok(Some("vector,2,12,-8,2,2.125,2.375,2,2".into())));
        io.lines.extend((0..49).map(|_| Ok(None)));
        let mut op = FakeOperator {
            keys: vec![Some(Key::Enter), Some(Key::Enter)].into(),
            ..Default::default()
        };
        measure(
            io,
            &mut clock,
            &mut op,
            MeasureConfig {
                baseline_seconds: 1,
                press_seconds: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(dropped.get());
        assert_eq!(clock.now(), Duration::from_secs(3));
        assert!(op.output.contains(&"0 2.00 12.00 10.00 10.00".into()));
        assert!(op.output.contains(&"1 2.00 -8.00 -10.00 10.00".into()));
        assert!(op.output.contains(&"3 2.00 2.12 0.12 0.12".into()));
        assert!(!op.output.iter().any(|s| s.contains("no samples")));
    }
    #[test]
    fn startup_raw_interrupt_prevents_any_calibration_or_send_text_write() {
        for calibrate in [false, true] {
            let mut clock = FakeClock(Rc::default());
            let io = io(&clock);
            let dropped = io.dropped.clone();
            let writes = io.writes.clone();
            let mut op = FakeOperator {
                keys: vec![Some(Key::Interrupted)].into(),
                ..Default::default()
            };
            assert_eq!(
                monitor(
                    io,
                    &mut clock,
                    &mut op,
                    MonitorConfig {
                        calibrate,
                        send_text: "explicit text".into(),
                        ..Default::default()
                    }
                ),
                Err(RunError::Interrupted)
            );
            assert!(dropped.get());
            assert!(writes.borrow().is_empty());
            assert!(clock.now() < Duration::from_secs(1));
        }
    }
    #[test]
    fn final_settle_slice_interrupt_prevents_startup_write() {
        for calibrate in [false, true] {
            let mut clock = FakeClock(Rc::default());
            let io = io(&clock);
            let dropped = io.dropped.clone();
            let writes = io.writes.clone();
            let mut op = FakeOperator {
                interrupt_at: Some((clock.0.clone(), Duration::from_secs(1))),
                ..Default::default()
            };
            assert_eq!(
                monitor(
                    io,
                    &mut clock,
                    &mut op,
                    MonitorConfig {
                        calibrate,
                        send_text: "explicit text".into(),
                        ..Default::default()
                    }
                ),
                Err(RunError::Interrupted)
            );
            assert_eq!(clock.now(), Duration::from_secs(1));
            assert!(dropped.get());
            assert!(writes.borrow().is_empty());
        }
    }
}
