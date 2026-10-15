//! Operator-gated observation. Serial capture always runs while waiting for
//! the operator; no buffered vector is mislabeled as an instructed trial.
use super::*;
use std::sync::mpsc;
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Ready,
    Skip,
    Stop,
}
const MAX_GATE_WAIT_MS: u64 = 120_000;
const MAX_TOTAL_WAIT_MS: u64 = 600_000;

/// Reads keyboard lines on a separate thread without ever touching USB.
/// Enter = start, s+Enter = skip this trial, q+Enter = abort.
/// Dropping the receiver when capture ends lets the process exit normally.
pub(super) fn console_actions() -> mpsc::Receiver<Action> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        loop {
            let mut line = String::new();
            match stdin.read_line(&mut line) {
                Ok(0) | Err(_) => {
                    let _ = sender.send(Action::Stop);
                    break;
                }
                Ok(_) => {
                    let action = match line.trim().to_ascii_lowercase().as_str() {
                        "" => Some(Action::Ready),
                        "s" | "skip" => Some(Action::Skip),
                        "q" | "quit" => Some(Action::Stop),
                        _ => None,
                    };
                    if let Some(action) = action {
                        if sender.send(action).is_err() {
                            break;
                        }
                    } else {
                        println!("Enter=開始 / s+Enter=この試行をスキップ / q+Enter=中止");
                    }
                }
            }
        }
    });
    receiver
}

/// Advance the printed schedule only while not waiting for the operator.
/// All wire data is still drained, bounded, and recorded during a pause, but
/// not entered into neutral/trial statistics.
#[allow(clippy::too_many_arguments)]
pub(super) fn collect<T: LineTransport, W: Write>(
    transport: &mut T,
    stages: &[Stage],
    log: &mut Log<W>,
    observation: &mut Observation,
    mut now: impl FnMut() -> u64,
    mut cancelled: impl FnMut() -> bool,
    mut action: impl FnMut() -> Option<Action>,
    mut cue: impl FnMut(&Stage, bool),
) -> Result<(), String> {
    let end = stages.last().ok_or("empty schedule")?.end_ms;
    if observation.stats.len() != stages.len() + 1 {
        return Err("guided session requires an unscored idle bucket".into());
    }
    let idle_index = stages.len();
    let mut total_pause_ms = 0_u64;
    let mut skipped_schedule_ms = 0_u64;
    let mut current = usize::MAX;
    let mut waiting: Option<(usize, u64)> = None;
    for _ in 0..MAX_LINES {
        if cancelled() {
            return Err("operator interrupted".into());
        }
        let wall = now();
        let ongoing_pause = waiting
            .map(|(_, start)| wall.saturating_sub(start))
            .unwrap_or(0);
        let virtual_ms = wall
            .saturating_sub(total_pause_ms.saturating_add(ongoing_pause))
            .saturating_add(skipped_schedule_ms);
        if virtual_ms >= end {
            return Ok(());
        }
        let stage = stages
            .iter()
            .position(|s| virtual_ms < s.end_ms)
            .ok_or("invalid guided schedule")?;
        if waiting.is_none() && stage != current {
            if stages[stage].label.ends_with("/prepare") {
                waiting = Some((stage, wall));
                cue(&stages[stage], true);
                log.event(json!({
                    "event":"operator_wait", "host_ms":wall,
                    "stage_index":stage,
                    "instruction":stages[stage].instruction,
                    "next":"Enter=start, s+Enter=skip, q+Enter=abort",
                    "data_during_wait":"unscored"
                }))?;
                log.flush()?;
            } else {
                current = stage;
                cue(&stages[stage], false);
                log.event(json!({
                    "event":"cue", "host_ms":wall,
                    "stage_index":stage, "stage":stages[stage]
                }))?;
                log.flush()?;
            }
        }

        if let Some((gate_stage, start)) = waiting {
            match action() {
                Some(Action::Ready) => {
                    let waited = wall.saturating_sub(start);
                    if total_pause_ms.saturating_add(waited) > MAX_TOTAL_WAIT_MS {
                        return Err("total operator preparation exceeded 10 minutes".into());
                    }
                    total_pause_ms += waited;
                    waiting = None;
                    current = gate_stage;
                    cue(&stages[gate_stage], false);
                    log.event(json!({
                        "event":"operator_ready", "host_ms":wall,
                        "stage_index":gate_stage, "waited_ms":waited,
                        "instruction":stages[gate_stage].instruction
                    }))?;
                    log.flush()?;
                }
                Some(Action::Skip) => {
                    let waited = wall.saturating_sub(start);
                    if total_pause_ms.saturating_add(waited) > MAX_TOTAL_WAIT_MS {
                        return Err("total operator preparation exceeded 10 minutes".into());
                    }
                    total_pause_ms += waited;
                    let label = &stages[gate_stage].label;
                    let root = label
                        .strip_suffix("/prepare")
                        .ok_or("invalid trial stage name")?;
                    let release_label = format!("{root}/release");
                    let end_index = (gate_stage..stages.len())
                        .find(|&i| stages[i].label == release_label)
                        .ok_or("incomplete trial schedule")?;
                    let skip_amount = stages[end_index].end_ms.saturating_sub(virtual_ms);
                    skipped_schedule_ms = skipped_schedule_ms.saturating_add(skip_amount);
                    waiting = None;
                    current = usize::MAX;
                    log.event(json!({
                        "event":"operator_skip", "host_ms":wall,
                        "stage_index":gate_stage, "trial":root,
                        "reason":"operator did not approve physical direction"
                    }))?;
                    log.flush()?;
                }
                Some(Action::Stop) => {
                    return Err("operator stopped using q+Enter".into());
                }
                None => {
                    if wall.saturating_sub(start) > MAX_GATE_WAIT_MS {
                        return Err(format!(
                            "preparation gate {} exceeded 120 s without approval",
                            stages[gate_stage].label
                        ));
                    }
                    if total_pause_ms.saturating_add(wall.saturating_sub(start)) > MAX_TOTAL_WAIT_MS
                    {
                        return Err("total operator preparation exceeded 10 minutes".into());
                    }
                }
            }
        } else {
            // Typed ahead during active capture cannot approve a later trial.
            let _ = action();
        }

        let line = match transport.read_line(Duration::from_millis(1000)) {
            Ok(line) => line,
            Err(TransportError::Timeout) if observation.vectors == 0 && wall < STARTUP_MS => {
                continue;
            }
            Err(TransportError::Timeout) if observation.vectors == 0 => {
                return Err(format!(
                    "no 7ch vector in 30 s (startup status {}, warnings {}, unknown {})",
                    observation.statuses, observation.warnings, observation.unknown
                ));
            }
            Err(e) => return Err(format!("read stopped: {e}")),
        };
        let received = now();
        let latest_pause = waiting
            .map(|(_, start)| received.saturating_sub(start))
            .unwrap_or(0);
        let recorded_virtual_ms = received
            .saturating_sub(total_pause_ms.saturating_add(latest_pause))
            .saturating_add(skipped_schedule_ms);
        let index = if waiting.is_some() {
            None
        } else {
            stages.iter().position(|s| recorded_virtual_ms < s.end_ms)
        };
        let effective_stage = index.unwrap_or(idle_index);
        // Log literal wire bytes before parsing; never turn malformed input into a sample.
        log.event(json!({
            "event":"wire", "host_ms":received,
            "schedule_ms":recorded_virtual_ms,
            "stage_index":index, "line":line
        }))?;
        let parsed = observation.observe(&line, received, effective_stage, effective_stage == 0)?;
        log.event(json!({
            "event":"parsed", "host_ms":received,
            "stage_index":index, "data":parsed
        }))?;
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
