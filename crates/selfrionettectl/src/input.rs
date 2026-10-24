//! A finite direct consumer of the same v3 service used by Device Manager.
use crate::CliError;
use selfrionette_serial::{
    LineTransport, SerialPortTransport,
    app::{CapturePhase, Command, LiveService, MethodDto, Reply},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Debug)]
struct Arguments {
    port: String,
    profile: PathBuf,
    operator: String,
    posture: String,
    grip: String,
    method: MethodDto,
    frames: usize,
    tare: bool,
}
fn parse(args: &[String]) -> Result<Arguments, CliError> {
    let usage = |message: &str| CliError::Usage(message.into());
    let mut values = BTreeMap::new();
    let mut neutral = false;
    let mut tare = false;
    let mut i = 0;
    while i < args.len() {
        let key = args[i].as_str();
        if key == "--neutral-ready" {
            if neutral {
                return Err(usage("duplicate --neutral-ready"));
            }
            neutral = true;
            i += 1;
            continue;
        }
        if key == "--tare-confirmed" {
            if tare {
                return Err(usage("duplicate --tare-confirmed"));
            }
            tare = true;
            i += 1;
            continue;
        }
        if ![
            "--port",
            "--profile",
            "--operator",
            "--posture",
            "--grip",
            "--method",
            "--frames",
        ]
        .contains(&key)
        {
            return Err(usage("unknown input argument"));
        }
        let value = args
            .get(i + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| usage("input argument requires a value"))?;
        if value.is_empty()
            || value.chars().any(char::is_control)
            || values.insert(key, value.clone()).is_some()
        {
            return Err(usage(
                "empty, control-character, or duplicate input argument",
            ));
        }
        i += 2;
    }
    let required = |key| {
        values.get(key).cloned().ok_or_else(||usage("input requires --port --profile --operator --posture --grip --method --frames and --neutral-ready"))
    };
    let port = required("--port")?;
    if port.trim() != port || port.len() > 256 {
        return Err(usage("invalid explicit port"));
    }
    let profile = PathBuf::from(required("--profile")?);
    if !profile.is_absolute()
        || profile.extension().and_then(|s| s.to_str()) != Some("srncal")
        || profile.file_name().is_none()
    {
        return Err(usage(
            "--profile must be an absolute saved .srncal profile path",
        ));
    }
    let operator = required("--operator")?;
    let posture = required("--posture")?;
    let grip = required("--grip")?;
    if [&operator, &posture, &grip]
        .iter()
        .any(|v| v.len() > 128 || v.trim() != v.as_str())
    {
        return Err(usage(
            "operator/posture/grip must be nonempty trimmed text within 128 UTF-8 bytes",
        ));
    }
    let method = match required("--method")?.as_str() {
        "diagonal" => MethodDto::Diagonal,
        "coupled" => MethodDto::Coupled,
        _ => {
            return Err(usage(
                "--method must be diagonal or coupled; no automatic fallback",
            ));
        }
    };
    let frames = required("--frames")?
        .parse::<usize>()
        .map_err(|_| usage("--frames must be 1..=1000000"))?;
    if !(1..=1_000_000).contains(&frames) {
        return Err(usage("--frames must be 1..=1000000"));
    }
    if !neutral {
        return Err(usage(
            "--neutral-ready explicitly confirms readiness for a new personal neutral; no capture was started",
        ));
    }
    Ok(Arguments {
        port,
        profile,
        operator,
        posture,
        grip,
        method,
        frames,
        tare,
    })
}
pub fn run(args: &[String]) -> Result<(), CliError> {
    let args = parse(args)?;
    if args.tare {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            return Err(CliError::Usage("--tare-confirmed requires an interactive neutral-ready confirmation after tare; use the shared library to provide your own user-ready step".into()));
        }
    }
    let metadata = std::fs::symlink_metadata(&args.profile)
        .map_err(|e| CliError::Failure(format!("profile file unavailable: {e}")))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 8 * 1024 * 1024
    {
        return Err(CliError::Failure(
            "profile must be a bounded regular file".into(),
        ));
    }
    let interrupted = Arc::new(AtomicBool::new(false));
    let flag = interrupted.clone();
    ctrlc::set_handler(move || flag.store(true, Ordering::SeqCst))
        .map_err(|e| CliError::Failure(e.to_string()))?;
    // This is the only live access point, after all CLI/path checks and explicit selection.
    let transport = SerialPortTransport::open_with_dtr(&args.port)
        .map_err(|e| CliError::Failure(e.to_string()))?;
    consume(Box::new(transport), &args, &interrupted)
}
fn execute(service: &mut LiveService, command: Command, root: &Path) -> Result<Reply, CliError> {
    let reply = service
        .execute(command, root)
        .map_err(|e| CliError::Failure(e.to_string()))?;
    if let Some(error) = &reply.error {
        return Err(CliError::Failure(error.clone()));
    }
    Ok(reply)
}
fn consume(
    transport: Box<dyn LineTransport + Send>,
    args: &Arguments,
    interrupted: &AtomicBool,
) -> Result<(), CliError> {
    let directory = args
        .profile
        .parent()
        .ok_or_else(|| CliError::Usage("profile parent missing".into()))?;
    let root = directory.parent().ok_or_else(|| {
        CliError::Usage("profile must reside in the per-device application store".into())
    })?;
    let name = args
        .profile
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| CliError::Usage("profile basename is not UTF-8".into()))?
        .to_owned();
    let mut service = LiveService::new(transport);
    let result = (|| {
        let connected = execute(&mut service, Command::Connect, root)?;
        let info = connected.view.info.ok_or_else(|| {
            CliError::Failure("compatible v3 device information is required".into())
        })?;
        if directory.file_name().and_then(|s| s.to_str()) != Some(info.device_id.as_str()) {
            return Err(CliError::Failure(
                "profile directory identity differs from the selected device".into(),
            ));
        }
        execute(&mut service, Command::StreamStart, root)?;
        if args.tare {
            eprintln!("明示確認された無荷重tareを実行します。個人neutralとは別です。");
            execute(&mut service, Command::Tare { confirmed: true }, root)?;
            // Stop at the producer while the operator changes posture. Draining
            // periodically would still admit samples acquired before user-ready.
            pause_for_user_ready(&mut service, root, || wait_for_neutral_ready(interrupted))?;
        }
        let latest = execute(&mut service, Command::Info, root)?;
        if !latest.view.info.is_some_and(|i| i.tare_valid) {
            return Err(CliError::Failure("this connection requires an explicit successful device tare; confirm unloaded conditions and use --tare-confirmed".into()));
        }
        execute(
            &mut service,
            Command::SetOperator {
                operator: args.operator.clone(),
                posture: args.posture.clone(),
                grip: args.grip.clone(),
            },
            root,
        )?;
        execute(
            &mut service,
            Command::LoadCalibration { record_path: name },
            root,
        )?;
        eprintln!(
            "今回の個人neutralを取得します。指定した姿勢・握りで意図的な入力をせず保持してください。"
        );
        execute(
            &mut service,
            Command::CaptureStart {
                phase: CapturePhase::SessionNeutral,
                axis: None,
                direction: 1,
                repeat: 0,
                level: 0.0,
                instruction: "CLI explicit user-ready personal neutral".into(),
                intent: None,
                sample_count: Some(16),
            },
            root,
        )?;
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if interrupted.load(Ordering::SeqCst) {
                return Err(CliError::Interrupted);
            }
            if Instant::now() > deadline {
                return Err(CliError::Failure(
                    "personal neutral capture exceeded 12 seconds".into(),
                ));
            }
            let reply = execute(&mut service, Command::CaptureTick, root)?;
            if reply
                .view
                .capture
                .as_ref()
                .is_some_and(|c| !c.active && c.complete)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let applied = execute(
            &mut service,
            Command::ApplyCalibration {
                confirmed: true,
                method: args.method,
            },
            root,
        )?;
        let mut emitted = 0;
        let mut sequence = applied.view.sample.map(|s| s.sequence);
        let mut last_output = Instant::now();
        while emitted < args.frames {
            if interrupted.load(Ordering::SeqCst) {
                return Err(CliError::Interrupted);
            }
            let reply = execute(&mut service, Command::StreamTick, root)?;
            for event in reply.sample_events {
                let sample = event.sample;
                if Some(sample.sequence) == sequence {
                    return Err(CliError::Failure(
                        "sample delivery repeated a source sequence".into(),
                    ));
                }
                sequence = Some(sample.sequence);
                let absent = [None::<f64>; 7];
                let age_ms = service
                    .host_elapsed_us()
                    .saturating_sub(event.host_received_us)
                    .div_ceil(1000)
                    .saturating_add(event.relative_backlog_ms);
                let output = event
                    .output
                    .as_ref()
                    .filter(|_| event.ready && age_ms <= 250);
                println!(
                    "{}",
                    serde_json::json!({"schema":1,"domain":"normalized_input","preprocessing_domain":"raw_adc","postprocessing_domain":"device_oriented_counts","source":{"device_id":info.device_id,"firmware":info.firmware_version,"source_sha":info.source_sha,"port":args.port},"sequence":sample.sequence,"device_ms":sample.timestamp_ms,"tare_epoch":sample.tare_epoch,"settings_generation":sample.generation,"quality_flags":sample.flags,"normalized":output.map(|o|o.normalized).unwrap_or(absent),"command":output.map(|o|o.command).unwrap_or(absent),"missing":output.is_none(),"span_exceeded":output.map(|o|o.span_exceeded),"unsupported_direction":output.map(|o|o.unsupported_direction),"host_clock":"consumer_process_elapsed_us","host_received_us":event.host_received_us,"host_emitted_us":service.host_elapsed_us(),"sample_age_ms":age_ms,"hardware_acceptance":false})
                );
                emitted += 1;
                last_output = Instant::now();
                if !event.ready || age_ms > 250 {
                    return Err(CliError::Failure("input invalidated; current sample missing, acquire new neutral before reuse".into()));
                }
                if emitted >= args.frames {
                    break;
                }
            }
            if last_output.elapsed() > Duration::from_secs(3) {
                return Err(CliError::Failure(
                    "no fresh normalized input within three seconds".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    })();
    let cleanup = execute(&mut service, Command::Disconnect, root);
    result.and(cleanup.map(|_| ()))
}

fn pause_for_user_ready(
    service: &mut LiveService,
    root: &Path,
    ready: impl FnOnce() -> Result<(), CliError>,
) -> Result<(), CliError> {
    execute(service, Command::StreamStop, root)?;
    ready()?;
    // The service retains this connection's challenge, so successful device
    // tare survives the pause. A different connection requires a new tare.
    execute(service, Command::StreamStart, root)?;
    Ok(())
}

fn wait_for_neutral_ready(interrupted: &AtomicBool) -> Result<(), CliError> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    eprintln!(
        "tareが完了しました。自然な使用姿勢・握りに戻り、準備ができたらEnterを押してください。Esc/Ctrl+Cで中止します。"
    );
    let deadline = Instant::now() + Duration::from_secs(900);
    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Err(CliError::Interrupted);
        }
        if Instant::now() > deadline {
            return Err(CliError::Failure(
                "neutral-ready confirmation timed out after fifteen minutes".into(),
            ));
        }
        if event::poll(Duration::from_millis(100)).map_err(|e| CliError::Failure(e.to_string()))? {
            if let Event::Key(key) = event::read().map_err(|e| CliError::Failure(e.to_string()))? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Enter => return Ok(()),
                    KeyCode::Esc => return Err(CliError::Interrupted),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Err(CliError::Interrupted);
                    }
                    _ => {}
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use selfrionette_serial::TransportError;
    use std::{collections::VecDeque, sync::Mutex};

    struct PausedTransport {
        replies: VecDeque<String>,
        streaming: Arc<AtomicBool>,
        sessions: Arc<Mutex<Vec<String>>>,
    }
    impl LineTransport for PausedTransport {
        fn discard_input(&mut self) -> Result<(), TransportError> {
            self.replies.clear();
            Ok(())
        }
        fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
            let fields: Vec<_> = line.trim().split(',').collect();
            match fields[0] {
                "info3" => self.replies.push_back(format!("info3,{},0.2.0-preview.1,9994bbbc927bcd87ff33a312df6167c72da07a6b,sparkfun_promicro16,atmega32u4,16000000,srn-11111111111111111111111111111111,0,0,0,0,0,0", fields[1])),
                "stream3" => {
                    self.streaming.store(fields[3] == "1", Ordering::SeqCst);
                    self.sessions.lock().unwrap().push(fields[2].to_owned());
                    self.replies.push_back(format!("ok3,{},stream,{}", fields[1], fields[2]));
                }
                _ => panic!("unexpected management command: {line}"),
            }
            Ok(())
        }
        fn read_line(&mut self, _: Duration) -> Result<String, TransportError> {
            self.replies.pop_front().ok_or(TransportError::Timeout)
        }
    }
    #[test]
    fn user_ready_pauses_producer_and_resumes_the_same_connection_only_after_confirmation() {
        for accepted in [true, false] {
            let streaming = Arc::new(AtomicBool::new(false));
            let sessions = Arc::new(Mutex::new(Vec::new()));
            let mut service = LiveService::new(Box::new(PausedTransport {
                replies: VecDeque::new(),
                streaming: streaming.clone(),
                sessions: sessions.clone(),
            }));
            let root = std::env::temp_dir();
            execute(&mut service, Command::Connect, &root).unwrap();
            execute(&mut service, Command::StreamStart, &root).unwrap();
            let result = pause_for_user_ready(&mut service, &root, || {
                assert!(
                    !streaming.load(Ordering::SeqCst),
                    "no pre-ready samples may accumulate"
                );
                if accepted {
                    Ok(())
                } else {
                    Err(CliError::Interrupted)
                }
            });
            assert_eq!(result.is_ok(), accepted);
            assert_eq!(streaming.load(Ordering::SeqCst), accepted);
            let challenges = sessions.lock().unwrap();
            assert!(challenges.iter().all(|value| value == &challenges[0]));
            assert_eq!(challenges.len(), if accepted { 3 } else { 2 });
        }
    }

    fn args() -> Vec<String> {
        let mut args = vec![
            "--port",
            "COM9",
            "--profile",
            "placeholder",
            "--operator",
            "op",
            "--posture",
            "seated",
            "--grip",
            "supported",
            "--method",
            "diagonal",
            "--frames",
            "10",
            "--neutral-ready",
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>();
        args[3] = std::env::temp_dir()
            .join("profiles/srn-11111111111111111111111111111111/profile-1.srncal")
            .to_str()
            .unwrap()
            .to_owned();
        args
    }
    #[test]
    fn explicit_port_profile_context_and_neutral_are_required_before_open() {
        let good = args();
        assert!(parse(&good).is_ok());
        for key in [
            "--port",
            "--profile",
            "--operator",
            "--posture",
            "--grip",
            "--method",
            "--frames",
        ] {
            let mut bad = good.clone();
            let at = bad.iter().position(|v| v == key).unwrap();
            bad.drain(at..=at + 1);
            assert!(parse(&bad).is_err(), "{key}");
        }
        let mut bad = good;
        bad.pop();
        assert!(parse(&bad).is_err());
    }
    #[test]
    fn duplicate_flags_relative_profile_and_invalid_method_fail_closed() {
        let mut bad = args();
        bad.push("--neutral-ready".into());
        assert!(parse(&bad).is_err());
        let mut bad = args();
        bad[3] = "relative.srncal".into();
        assert!(parse(&bad).is_err());
        let mut bad = args();
        bad[11] = "auto".into();
        assert!(parse(&bad).is_err());
    }
}
