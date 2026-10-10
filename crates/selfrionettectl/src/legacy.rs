use crate::CliError;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    terminal,
};
use selfrionette_core::legacy::{DisplayLevel, MeasureConfig, MonitorConfig};
use selfrionette_serial::{
    SERIAL_BAUD_RATE,
    legacy::{self, Clock, Key, LegacySerialTransport, Operator, RunError},
};
use std::{
    collections::{HashSet, VecDeque},
    io::{self, BufRead, IsTerminal, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
struct Options {
    port: String,
    baud: u32,
    monitor: MonitorConfig,
    measure: MeasureConfig,
    help: bool,
}

fn help(command: &str) -> String {
    let m = MonitorConfig::default();
    let measure = MeasureConfig::default();
    let options = if command == "legacy-monitor" {
        format!(
            "--duration-seconds <i32> (default {})\n  --send-text <text>\n  --calibrate\n  --display-level <status|warn|vector> (default {})\n  --paused-display-level <status|warn|vector> (default {})",
            m.duration_seconds,
            m.display_level.name(),
            m.paused_display_level.name()
        )
    } else {
        format!(
            "--baseline-seconds <i32> (default {})\n  --press-seconds <i32> (default {})\n  --sensor <1..7> (default {})\n  --repeats <i32> (default {})\n  --all-sensors",
            measure.baseline_seconds, measure.press_seconds, measure.sensor, measure.repeats
        )
    };
    format!(
        "usage: selfrionettectl {command} --port <explicit-port> [options]\n  --baud-rate <positive-i32> (default {SERIAL_BAUD_RATE})\n  {options}\n  --help\n  --powershell-args: accept legacy PowerShell parameter names and switches\nLegacy Sim 7ch only. No v2 identity/tare/provision. No implicit COM5 or auto-discovery."
    )
}

fn parameter(command: &str, spelling: &str, ps: bool) -> Result<&'static str, CliError> {
    let common = [
        ("port", "--port"),
        ("baudrate", "--baud-rate"),
        ("help", "--help"),
    ];
    let monitor = [
        ("durationseconds", "--duration-seconds"),
        ("sendtext", "--send-text"),
        ("calibrate", "--calibrate"),
        ("displaylevel", "--display-level"),
        ("pauseddisplaylevel", "--paused-display-level"),
    ];
    let measure = [
        ("baselineseconds", "--baseline-seconds"),
        ("pressseconds", "--press-seconds"),
        ("sensor", "--sensor"),
        ("repeats", "--repeats"),
        ("allsensors", "--all-sensors"),
    ];
    let specific = if command == "legacy-monitor" {
        &monitor[..]
    } else {
        &measure[..]
    };
    let spelling = spelling.to_ascii_lowercase();
    let matches = common
        .iter()
        .chain(specific)
        .filter(|(old, new)| {
            if ps {
                old.starts_with(spelling.trim_start_matches('-')) || *new == spelling
            } else {
                *new == spelling
            }
        })
        .map(|(_, new)| *new)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [one] => Ok(one),
        _ => Err(CliError::Usage(format!(
            "unknown or ambiguous option: {spelling}"
        ))),
    }
}
fn integer(text: &str, name: &str) -> Result<i32, CliError> {
    text.parse()
        .map_err(|_| CliError::Usage(format!("{name} requires an int32 value")))
}
fn switch(text: Option<&str>) -> Result<bool, CliError> {
    match text.map(|s| s.trim_start_matches('$').to_ascii_lowercase()) {
        None => Ok(true),
        Some(s) if s == "true" => Ok(true),
        Some(s) if s == "false" => Ok(false),
        _ => Err(CliError::Usage("switch value must be true or false".into())),
    }
}
fn parse(command: &str, args: &[String]) -> Result<Options, CliError> {
    let ps = args.first().is_some_and(|s| s == "--powershell-args");
    if ps {
        return parse(command, &normalize_powershell(command, &args[1..])?);
    }
    let mut result = Options {
        port: String::new(),
        baud: SERIAL_BAUD_RATE,
        monitor: MonitorConfig::default(),
        measure: MeasureConfig::default(),
        help: false,
    };
    let mut seen = HashSet::new();
    let mut i = 0;
    while i < args.len() {
        let key = parameter(command, &args[i], false)?;
        if !seen.insert(key) {
            return Err(CliError::Usage(format!("duplicate option: {key}")));
        }
        let is_switch = matches!(key, "--calibrate" | "--all-sensors" | "--help");
        let value = if is_switch {
            None
        } else {
            i += 1;
            Some(
                args.get(i)
                    .ok_or_else(|| CliError::Usage(format!("{key} requires a value")))?
                    .as_str(),
            )
        };
        match key {
            "--help" => result.help = switch(value)?,
            "--calibrate" => result.monitor.calibrate = switch(value)?,
            "--all-sensors" => result.measure.all_sensors = switch(value)?,
            "--port" => result.port = value.unwrap().into(),
            "--baud-rate" => {
                let baud = integer(value.unwrap(), key)?;
                result.baud = u32::try_from(baud)
                    .ok()
                    .filter(|b| *b > 0)
                    .ok_or_else(|| CliError::Usage("baud rate must be positive".into()))?;
            }
            "--duration-seconds" => result.monitor.duration_seconds = integer(value.unwrap(), key)?,
            "--send-text" => result.monitor.send_text = value.unwrap().into(),
            "--display-level" | "--paused-display-level" => {
                let level = DisplayLevel::parse(value.unwrap()).ok_or_else(|| {
                    CliError::Usage(format!("{key} requires status, warn or vector"))
                })?;
                if key == "--display-level" {
                    result.monitor.display_level = level;
                } else {
                    result.monitor.paused_display_level = level;
                }
            }
            "--baseline-seconds" => result.measure.baseline_seconds = integer(value.unwrap(), key)?,
            "--press-seconds" => result.measure.press_seconds = integer(value.unwrap(), key)?,
            "--repeats" => result.measure.repeats = integer(value.unwrap(), key)?,
            "--sensor" => {
                let sensor = integer(value.unwrap(), key)?;
                result.measure.sensor =
                    u8::try_from(sensor)
                        .ok()
                        .filter(|s| (1..=7).contains(s))
                        .ok_or_else(|| CliError::Usage("sensor must be 1..7".into()))?;
            }
            _ => unreachable!(),
        }
        i += 1;
    }
    if !result.help && result.port.trim().is_empty() {
        return Err(CliError::Usage(
            "--port must be explicit; implicit COM5 was removed".into(),
        ));
    }
    Ok(result)
}

/// PowerShellはnamed parameterを先にbindし、switchを除く未指定parameterへpositionをbindする。
fn normalize_powershell(command: &str, args: &[String]) -> Result<Vec<String>, CliError> {
    let positions = if command == "legacy-monitor" {
        &[
            "--port",
            "--baud-rate",
            "--duration-seconds",
            "--send-text",
            "--display-level",
            "--paused-display-level",
        ][..]
    } else {
        &[
            "--port",
            "--baud-rate",
            "--baseline-seconds",
            "--press-seconds",
            "--sensor",
            "--repeats",
        ][..]
    };
    let mut seen = HashSet::new();
    let mut named = Vec::new();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if !args[i].starts_with('-') || args[i].parse::<i32>().is_ok() {
            positional.push(args[i].clone());
            i += 1;
            continue;
        }
        let (spelling, mut inline) = args[i]
            .split_once(':')
            .map_or((args[i].as_str(), None), |(a, b)| (a, Some(b)));
        let key = parameter(command, spelling, true)?;
        if !seen.insert(key) {
            return Err(CliError::Usage(format!("duplicate option: {key}")));
        }
        if inline == Some("") && i + 1 < args.len() {
            i += 1;
            inline = Some(args[i].as_str());
        }
        if matches!(key, "--calibrate" | "--all-sensors" | "--help") {
            if switch(inline)? {
                named.push(key.to_owned());
            }
        } else {
            let value = match inline {
                Some(value) => value,
                None => {
                    i += 1;
                    args.get(i)
                        .ok_or_else(|| CliError::Usage(format!("{key} requires a value")))?
                }
            };
            named.extend([key.to_owned(), value.to_owned()]);
        }
        i += 1;
    }
    let mut available = positions.iter().filter(|key| !seen.contains(**key));
    for value in positional {
        let key = available
            .next()
            .ok_or_else(|| CliError::Usage("too many positional arguments".into()))?;
        named.extend([(*key).to_owned(), value]);
    }
    Ok(named)
}

pub(super) fn run(command: &str, args: &[String]) -> Result<(), CliError> {
    let options = parse(command, args)?;
    if options.help {
        println!("{}", help(command));
        return Ok(());
    }
    let stopped = Arc::new(AtomicBool::new(false));
    let signal = stopped.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::SeqCst))
        .map_err(|e| CliError::Failure(e.to_string()))?;
    // OS adapterを先に準備。invalid argv / console failureではportをopenしない。
    let mut operator =
        Console::new(stopped, command == "legacy-measure").map_err(CliError::Failure)?;
    let mut clock = RealClock(Instant::now());
    operator
        .output(&format!(
            "Opening {} at {} baud. Legacy Sim 7ch protocol.",
            options.port, options.baud
        ))
        .map_err(CliError::Failure)?;
    if command == "legacy-measure" {
        operator
            .output("Close any existing monitor on the same port before continuing.")
            .map_err(CliError::Failure)?;
    }
    let io = LegacySerialTransport::open(&options.port, options.baud)
        .map_err(|e| CliError::Failure(e.to_string()))?;
    let result = if command == "legacy-monitor" {
        legacy::monitor(io, &mut clock, &mut operator, options.monitor)
    } else {
        legacy::measure(io, &mut clock, &mut operator, options.measure)
    };
    match result {
        Ok(()) => Ok(()),
        Err(RunError::Interrupted) => Err(CliError::Interrupted),
        Err(RunError::Failure(s)) => Err(CliError::Failure(s)),
    }
}
struct RealClock(Instant);
impl Clock for RealClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
    fn pause(&mut self, d: Duration) {
        std::thread::sleep(d);
    }
}
struct Console {
    stopped: Arc<AtomicBool>,
    raw: bool,
    lines: Option<Receiver<Key>>,
    pending: VecDeque<Key>,
}
impl Console {
    fn new(stopped: Arc<AtomicBool>, measure: bool) -> Result<Self, String> {
        let raw = io::stdin().is_terminal();
        if raw {
            terminal::enable_raw_mode().map_err(|e| e.to_string())?;
        }
        let lines = if measure && !raw {
            let (send, receive) = mpsc::sync_channel(16);
            std::thread::spawn(move || {
                for line in io::stdin().lock().lines() {
                    if line.is_err() || send.send(Key::Enter).is_err() {
                        break;
                    }
                }
                let _ = send.send(Key::EndOfInput);
            });
            Some(receive)
        } else {
            None
        };
        Ok(Self {
            stopped,
            raw,
            lines,
            pending: VecDeque::new(),
        })
    }
    fn pump(&mut self) -> Result<bool, String> {
        if !self.raw {
            return Ok(self.interrupted());
        }
        while event::poll(Duration::ZERO).map_err(|e| e.to_string())? {
            if let Event::Key(event) = event::read().map_err(|e| e.to_string())? {
                if event.kind == KeyEventKind::Release {
                    continue;
                }
                let key = match event.code {
                    KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(true);
                    }
                    KeyCode::Char(c) => Key::Character(c),
                    KeyCode::Enter => Key::Enter,
                    _ => continue,
                };
                if self.pending.len() >= 64 {
                    return Err("operator key queue exceeded 64 events".into());
                }
                self.pending.push_back(key);
            }
        }
        Ok(self.interrupted())
    }
}
impl Operator for Console {
    fn poll_key(&mut self) -> Result<Option<Key>, String> {
        if self.pump()? {
            return Ok(Some(Key::Interrupted));
        }
        if let Some(key) = self.pending.pop_front() {
            return Ok(Some(key));
        }
        match &self.lines {
            Some(receiver) => match receiver.try_recv() {
                Ok(key) => Ok(Some(key)),
                Err(mpsc::TryRecvError::Empty) => Ok(None),
                Err(mpsc::TryRecvError::Disconnected) => Ok(Some(Key::EndOfInput)),
            },
            None => Ok(None),
        }
    }
    fn interrupted(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
    fn poll_interrupt(&mut self) -> Result<bool, String> {
        self.pump()
    }
    fn output(&mut self, text: &str) -> Result<(), String> {
        let mut out = io::stdout().lock();
        // raw terminalはLFだけではcolumnを戻さない。全platformで同じ行を出す。
        for line in text.split('\n') {
            write!(out, "{line}\r\n").map_err(|e| e.to_string())?;
        }
        out.flush().map_err(|e| e.to_string())
    }
}
impl Drop for Console {
    fn drop(&mut self) {
        if self.raw {
            let _ = terminal::disable_raw_mode();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn missing_port_help_and_invalid_options_are_checked_before_live_open() {
        for command in ["legacy-monitor", "legacy-measure"] {
            assert!(matches!(parse(command, &[]), Err(CliError::Usage(_))));
            assert!(parse(command, &args(&["--help"])).unwrap().help);
            for a in [
                vec!["--port", " ", "--help:false"],
                vec!["--port", "COM9", "--wrong"],
                vec!["--port", "COM9", "--baud-rate", "0"],
                vec!["--port", "COM9", "--port", "COM8"],
            ] {
                assert!(parse(command, &args(&a)).is_err());
            }
        }
    }
    #[test]
    fn aliases_switch_false_case_abbreviation_and_literal_values() {
        let o = parse(
            "legacy-monitor",
            &args(&[
                "--powershell-args",
                "-pOrT",
                "日本語 space;&",
                "-Calibrate:$false",
                "-SendText",
                "",
                "-DURATIONSECONDS",
                "-1",
                "-Displ",
                "WARN",
            ]),
        )
        .unwrap();
        assert_eq!(o.port, "日本語 space;&");
        assert!(!o.monitor.calibrate);
        assert_eq!(o.monitor.send_text, "");
        assert_eq!(o.monitor.duration_seconds, -1);
        assert_eq!(o.monitor.display_level, DisplayLevel::Warn);
        assert!(
            parse(
                "legacy-monitor",
                &args(&["--powershell-args", "-p", "COM9"])
            )
            .is_err()
        );
    }
    #[test]
    fn measurement_range_int32_and_defaults() {
        let o = parse("legacy-measure", &args(&["--port", "COM9"])).unwrap();
        assert_eq!(o.baud, SERIAL_BAUD_RATE);
        assert_eq!(o.measure.baseline_seconds, 3);
        assert_eq!(o.measure.press_seconds, 4);
        for pair in [
            ["--sensor", "0"],
            ["--sensor", "8"],
            ["--repeats", "2147483648"],
            ["--press-seconds", "x"],
        ] {
            assert!(
                parse(
                    "legacy-measure",
                    &args(&["--port", "COM9", pair[0], pair[1]])
                )
                .is_err()
            );
        }
        let o = parse(
            "legacy-measure",
            &args(&[
                "--powershell-args",
                "-Port:COM9",
                "-AllSensors:True",
                "-Repeats",
                "3",
                "-Sensor",
                "4",
            ]),
        )
        .unwrap();
        assert!(o.measure.all_sensors);
        assert_eq!(o.measure.sensor, 4);
        assert_eq!(o.measure.repeats, 3);
    }
    #[test]
    fn powershell_colon_separate_value_and_help_false_preserve_binding() {
        let o = parse(
            "legacy-monitor",
            &args(&[
                "--powershell-args",
                "-Port:",
                "COM9",
                "-Calibrate:",
                "False",
                "-Help:$false",
                "-SendText",
                "-Help",
            ]),
        )
        .unwrap();
        assert!(!o.help);
        assert!(!o.monitor.calibrate);
        assert_eq!(o.port, "COM9");
        assert_eq!(o.monitor.send_text, "-Help");
    }
    #[test]
    fn powershell_positions_skip_switches_and_named_parameters() {
        let o = parse(
            "legacy-monitor",
            &args(&[
                "--powershell-args",
                "COM9",
                "9600",
                "5",
                "text",
                "warn",
                "vector",
            ]),
        )
        .unwrap();
        assert_eq!(o.port, "COM9");
        assert_eq!(o.baud, 9600);
        assert_eq!(o.monitor.duration_seconds, 5);
        assert_eq!(o.monitor.send_text, "text");
        assert_eq!(o.monitor.display_level, DisplayLevel::Warn);
        assert_eq!(o.monitor.paused_display_level, DisplayLevel::Vector);
        let o = parse(
            "legacy-monitor",
            &args(&[
                "--powershell-args",
                "COM9",
                "9600",
                "text",
                "-DurationSeconds",
                "5",
            ]),
        )
        .unwrap();
        assert_eq!(o.monitor.send_text, "text");
        let o = parse(
            "legacy-measure",
            &args(&[
                "--powershell-args",
                "COM9",
                "9600",
                "2",
                "3",
                "4",
                "2",
                "-AllSensors",
            ]),
        )
        .unwrap();
        assert_eq!(o.measure.sensor, 4);
        assert_eq!(o.measure.repeats, 2);
        assert!(o.measure.all_sensors);
    }
}
