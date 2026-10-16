//! Operator-gated observation. Serial capture always runs while waiting for
//! the operator; waiting/boundary reads are retained without trial attribution.
use super::*;
use std::sync::Mutex;
use std::thread;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Ready,
    Skip,
    Stop,
}
const MAX_GATE_WAIT_MS: u64 = 120_000;
const MAX_TOTAL_WAIT_MS: u64 = 600_000;

#[derive(Default)]
struct InputState {
    gate: Option<usize>,
    pending: Option<Action>,
    stopped: bool,
}

#[derive(Clone, Default)]
pub(super) struct ConsoleActions(Arc<Mutex<InputState>>);

impl ConsoleActions {
    pub(super) fn poll(&self, gate: Option<usize>) -> Option<Action> {
        let mut state = self.0.lock().unwrap();
        if state.stopped {
            return Some(Action::Stop);
        }
        if state.gate != gate {
            state.gate = gate;
            state.pending = None;
        }
        let action = state.pending.take();
        if action.is_some() {
            // Close this gate immediately, including while the next read blocks.
            state.gate = None;
        }
        action
    }

    fn submit(&self, gate: Option<usize>, action: Action) {
        let mut state = self.0.lock().unwrap();
        if action == Action::Stop {
            state.stopped = true;
        } else if gate.is_some() && state.gate == gate && state.pending.is_none() {
            state.pending = Some(action);
        }
    }

    fn read(&self, mut input: impl Read) {
        let mut line = [0_u8; 16];
        let mut len = 0;
        let mut overflow = false;
        let mut gate = None;
        loop {
            let mut byte = [0];
            if !matches!(input.read(&mut byte), Ok(1)) {
                self.submit(None, Action::Stop);
                return;
            }
            if len == 0 && !overflow {
                gate = self.0.lock().unwrap().gate;
            }
            if byte[0] == b'\n' {
                let text = std::str::from_utf8(&line[..len])
                    .unwrap_or("invalid")
                    .trim();
                let action = if overflow {
                    None
                } else if text.is_empty() {
                    Some(Action::Ready)
                } else if text.eq_ignore_ascii_case("s") || text.eq_ignore_ascii_case("skip") {
                    Some(Action::Skip)
                } else if text.eq_ignore_ascii_case("q") || text.eq_ignore_ascii_case("quit") {
                    Some(Action::Stop)
                } else {
                    None
                };
                if let Some(action) = action {
                    self.submit(gate, action);
                    if action == Action::Stop {
                        return;
                    }
                }
                len = 0;
                overflow = false;
            } else if len < line.len() {
                line[len] = byte[0];
                len += 1;
            } else {
                overflow = true;
            }
        }
    }
}

/// Bounded lines and one gate-scoped approval; Stop is sticky in every stage.
/// This keyboard thread never touches USB.
pub(super) fn console_actions() -> ConsoleActions {
    let actions = ConsoleActions::default();
    let reader = actions.clone();
    thread::spawn(move || {
        let stdin = std::io::stdin();
        reader.read(stdin.lock());
    });
    actions
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
    mut action: impl FnMut(Option<usize>) -> Option<Action>,
    mut cue: impl FnMut(&Stage, bool),
) -> Result<(), String> {
    if stages.is_empty() {
        return Err("empty schedule".into());
    }
    if observation.stats.len() != stages.len() + 1 {
        return Err("guided session requires an unscored idle bucket".into());
    }
    let idle_index = stages.len();
    let mut total_pause_ms = 0_u64;
    let mut current = 0;
    let mut active_since: Option<u64> = None;
    let mut waiting: Option<u64> = None;
    for _ in 0..MAX_LINES {
        if cancelled() {
            return Err("operator interrupted".into());
        }
        if active_since.is_some_and(|start| {
            now().saturating_sub(start) >= stages[current].end_ms - stages[current].start_ms
        }) {
            current += 1;
            active_since = None;
        }
        if current >= stages.len() {
            if action(None) == Some(Action::Stop) {
                return Err("operator stopped using q+Enter".into());
            }
            return Ok(());
        }
        let stage = current;
        if waiting.is_none() && active_since.is_none() {
            if stages[stage].label.ends_with("/prepare") {
                cue(&stages[stage], true);
                let wall = now();
                waiting = Some(wall);
                log.event(json!({
                    "event":"operator_wait", "host_ms":wall,
                    "stage_index":stage,
                    "instruction":stages[stage].instruction,
                    "next":"Enter=start, s+Enter=skip, q+Enter=abort",
                    "data_during_wait":"unscored"
                }))?;
                log.flush()?;
            } else {
                cue(&stages[stage], false);
                let wall = now();
                active_since = Some(wall);
                log.event(json!({
                    "event":"cue", "host_ms":wall,
                    "stage_index":stage, "stage":stages[stage]
                }))?;
                log.flush()?;
            }
        }

        let input = action(waiting.map(|_| stage));
        let wall = now();
        if input == Some(Action::Stop) {
            return Err("operator stopped using q+Enter".into());
        }
        if let Some(start) = waiting {
            let gate_stage = stage;
            let waited = wall.saturating_sub(start);
            if waited > MAX_GATE_WAIT_MS {
                return Err(format!(
                    "preparation gate {} exceeded 120 s without approval",
                    stages[gate_stage].label
                ));
            }
            if total_pause_ms.saturating_add(waited) > MAX_TOTAL_WAIT_MS {
                return Err("total operator preparation exceeded 10 minutes".into());
            }
            match input {
                Some(Action::Ready) => {
                    total_pause_ms += waited;
                    waiting = None;
                    cue(&stages[gate_stage], false);
                    active_since = Some(now());
                    log.event(json!({
                        "event":"operator_ready", "host_ms":active_since,
                        "stage_index":gate_stage, "waited_ms":waited,
                        "instruction":stages[gate_stage].instruction
                    }))?;
                    log.flush()?;
                }
                Some(Action::Skip) => {
                    total_pause_ms += waited;
                    let label = &stages[gate_stage].label;
                    let root = label
                        .strip_suffix("/prepare")
                        .ok_or("invalid trial stage name")?;
                    let release_label = format!("{root}/release");
                    let end_index = (gate_stage..stages.len())
                        .find(|&i| stages[i].label == release_label)
                        .ok_or("incomplete trial schedule")?;
                    waiting = None;
                    current = end_index + 1;
                    log.event(json!({
                        "event":"operator_skip", "host_ms":wall,
                        "stage_index":gate_stage, "trial":root,
                        "reason":"operator did not approve physical direction"
                    }))?;
                    log.flush()?;
                    // Establish the next cue/gate before attempting another read.
                    continue;
                }
                Some(Action::Stop) => {
                    return Err("operator stopped using q+Enter".into());
                }
                None => {}
            }
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
        let elapsed = active_since.map(|start| received.saturating_sub(start));
        let duration = stages[stage].end_ms - stages[stage].start_ms;
        // A blocking read cannot establish a cue or acknowledge readiness.
        // Keep crossing and waiting lines, but never score an unannounced stage.
        let index = elapsed.filter(|&ms| ms < duration).map(|_| stage);
        let recorded_virtual_ms = stages[stage].start_ms + elapsed.unwrap_or(0).min(duration);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_has_one_gate_scoped_approval_and_sticky_stop() {
        let input = ConsoleActions::default();
        for _ in 0..1000 {
            input.submit(None, Action::Ready);
            input.submit(None, Action::Skip);
        }
        assert_eq!(input.poll(Some(0)), None);
        input.submit(Some(0), Action::Ready);
        for _ in 0..1000 {
            input.submit(Some(0), Action::Skip);
        }
        assert_eq!(input.poll(Some(0)), Some(Action::Ready));
        assert_eq!(input.poll(None), None);
        input.submit(Some(0), Action::Ready);
        assert_eq!(input.poll(Some(5)), None);
        // A line begun at the old gate cannot authorize the new one.
        input.submit(Some(0), Action::Skip);
        assert_eq!(input.poll(Some(5)), None);
        input.submit(Some(5), Action::Skip);
        assert_eq!(input.poll(Some(10)), None);
        input.submit(None, Action::Stop);
        assert_eq!(input.poll(None), Some(Action::Stop));
        assert_eq!(input.poll(Some(10)), Some(Action::Stop));
    }

    #[test]
    fn console_bounds_overlong_lines_and_retains_subsequent_stop() {
        let input = ConsoleActions::default();
        input.poll(Some(0));
        let bytes = vec![b' '; 100_000];
        input.read(std::io::Cursor::new([bytes.as_slice(), b"\nq\n"].concat()));
        let state = input.0.lock().unwrap();
        assert_eq!(state.pending, None);
        assert!(state.stopped);
    }

    #[test]
    fn queued_console_input_cannot_auto_advance_a_future_trial() {
        use std::{cell::Cell, rc::Rc};
        struct Capture {
            clock: Rc<Cell<u64>>,
            input: ConsoleActions,
        }
        impl LineTransport for Capture {
            fn discard_input(&mut self) -> Result<(), TransportError> {
                panic!("no discard")
            }
            fn write_line(&mut self, _: &str) -> Result<(), TransportError> {
                panic!("no write")
            }
            fn read_line(&mut self, _: Duration) -> Result<String, TransportError> {
                let ms = self.clock.get() + 10;
                self.clock.set(ms);
                for _ in 0..1000 {
                    // Only the first trial may be approved. Excess approvals
                    // during its wait/read/capture must not reach trial two.
                    self.input.submit(Some(1), Action::Ready);
                    self.input.submit(Some(1), Action::Skip);
                }
                if ms >= 80 {
                    self.input.submit(None, Action::Stop);
                }
                Ok(format!("vector,{ms},1,2,3,4,5,6,7"))
            }
        }
        let clock = Rc::new(Cell::new(0));
        let input = ConsoleActions::default();
        let mut transport = Capture {
            clock: clock.clone(),
            input: input.clone(),
        };
        let stages: Vec<_> = [
            "startup",
            "first/prepare",
            "first/release",
            "second/prepare",
        ]
        .into_iter()
        .enumerate()
        .map(|(i, label)| Stage {
            label: label.into(),
            instruction: "test".into(),
            start_ms: i as u64 * 20,
            end_ms: (i as u64 + 1) * 20,
        })
        .collect();
        let mut obs = Observation::new(stages.len() + 1);
        let mut log = Log::new(Vec::new());
        let err = collect(
            &mut transport,
            &stages,
            &mut log,
            &mut obs,
            || clock.get(),
            || false,
            |gate| input.poll(gate),
            |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("operator stopped"));
        assert_eq!(obs.stats[3].count, 0);
        let events: Vec<Value> = String::from_utf8(log.out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["event"] == "operator_ready")
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e["event"] == "operator_skip")
                .count(),
            0
        );
    }
}
