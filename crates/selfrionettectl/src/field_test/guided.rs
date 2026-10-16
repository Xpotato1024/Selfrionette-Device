//! Operator-gated observation. Serial capture always runs while waiting for
//! the operator; waiting/boundary reads are retained without trial attribution.
use super::*;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    terminal,
};
use std::io::{ErrorKind, IsTerminal};
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
    generation: u64,
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
            state.generation += 1;
            state.pending = None;
        }
        let action = state.pending.take();
        if action.is_some() {
            // Close this gate immediately, including while the next read blocks.
            state.gate = None;
            state.generation += 1;
        }
        action
    }

    fn snapshot(&self) -> (Option<usize>, u64) {
        let state = self.0.lock().unwrap();
        (state.gate, state.generation)
    }

    fn submit_scoped(&self, scope: (Option<usize>, u64), action: Action) -> bool {
        let mut state = self.0.lock().unwrap();
        if action == Action::Stop {
            state.stopped = true;
            state.pending = None;
            return true;
        }
        if !state.stopped
            && scope.0.is_some()
            && (state.gate, state.generation) == scope
            && state.pending.is_none()
        {
            state.pending = Some(action);
            return true;
        }
        false
    }

    #[cfg(test)]
    fn submit(&self, gate: Option<usize>, action: Action) {
        self.submit_scoped((gate, self.snapshot().1), action);
    }

    /// The input must return WouldBlock only when its pending queue is empty.
    /// Feedback reports whether a Ready/Skip was accepted, never for Stop.
    fn read(&self, mut input: impl Read, mut feedback: impl FnMut(bool)) {
        let mut line = [0_u8; 16];
        let mut len: usize = 0;
        let mut overflow = false;
        let mut scope = (None, 0);
        let mut armed = None;
        loop {
            let before = self.snapshot();
            // Bind the entire line BEFORE its first blocking read. A byte that
            // arrives during capture must not acquire a subsequently opened gate.
            if len == 0 && !overflow {
                scope = if armed == Some(before) {
                    before
                } else {
                    (None, before.1)
                };
            }
            let mut byte = [0];
            match input.read(&mut byte) {
                Ok(1) => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    // A new gate cannot adopt any backlog, even if the reader
                    // was descheduled between already-buffered input lines.
                    if self.snapshot() == before {
                        armed = Some(before);
                    }
                    continue;
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                _ => {
                    self.submit_scoped((None, 0), Action::Stop);
                    return;
                }
            }
            if byte[0] == 3 {
                self.submit_scoped((None, 0), Action::Stop);
                return;
            }
            if byte[0] == 8 {
                len = len.saturating_sub(1);
                continue;
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
                    // Check the generation under the same lock as submission,
                    // including close/reopen of the same stage during read().
                    let accepted = self.submit_scoped(scope, action);
                    if action == Action::Stop {
                        return;
                    }
                    feedback(accepted);
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

struct KeyboardBytes {
    finished: Arc<AtomicBool>,
    enter_down: bool,
}
impl KeyboardBytes {
    fn new(finished: Arc<AtomicBool>) -> Self {
        Self {
            finished,
            enter_down: false,
        }
    }
    fn key_byte(&mut self, key: KeyEvent) -> Option<u8> {
        if key.code == KeyCode::Enter {
            if key.kind == KeyEventKind::Release {
                self.enter_down = false;
                return None;
            }
            // Windows reports typematic key-down as Press, not Repeat.
            if self.enter_down {
                return None;
            }
            if key.kind == KeyEventKind::Press {
                self.enter_down = true;
            }
        }
        if key.kind != KeyEventKind::Press {
            return None;
        }
        Some(match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => 3,
            KeyCode::Enter => b'\n',
            KeyCode::Backspace => 8,
            KeyCode::Char(c) if c.is_ascii() => c as u8,
            KeyCode::Char(_) => 0xff,
            _ => return None,
        })
    }
}
impl Read for KeyboardBytes {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.finished.load(Ordering::SeqCst) {
                return Ok(0);
            }
            if !event::poll(Duration::from_millis(25))? {
                return Err(ErrorKind::WouldBlock.into());
            }
            if let Event::Key(key) = event::read()? {
                if let Some(byte) = self.key_byte(key) {
                    out[0] = byte;
                    return Ok(1);
                }
            }
        }
    }
}

/// Own the keyboard thread and terminal mode for this capture only.
pub(super) struct ConsoleInput {
    actions: ConsoleActions,
    finished: Arc<AtomicBool>,
    reader: Option<thread::JoinHandle<()>>,
}
impl ConsoleInput {
    pub(super) fn poll(&self, gate: Option<usize>) -> Option<Action> {
        self.actions.poll(gate)
    }
}
impl Drop for ConsoleInput {
    fn drop(&mut self) {
        self.finished.store(true, Ordering::SeqCst);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        restore_input_mode();
    }
}

fn restore_input_mode() {
    #[cfg(not(windows))]
    let _ = crossterm::execute!(std::io::stdout(), event::PopKeyboardEnhancementFlags);
    let _ = terminal::disable_raw_mode();
}

/// Bounded lines, queue-empty rearming, one gate-scoped approval, sticky Stop.
/// This keyboard thread never touches USB; redirected approval input is refused.
pub(super) fn console_actions() -> Result<ConsoleInput, String> {
    if !std::io::stdin().is_terminal() {
        return Err("operator gates require an interactive terminal".into());
    }
    // Unix legacy key events have no releases; fail closed rather than confuse
    // Enter auto-repeat with a new physical approval on unsupported terminals.
    #[cfg(not(windows))]
    if !terminal::supports_keyboard_enhancement()
        .map_err(|e| format!("keyboard capability: {e}"))?
    {
        return Err("operator gates require a terminal with key-release reporting".into());
    }
    terminal::enable_raw_mode().map_err(|e| format!("keyboard input: {e}"))?;
    #[cfg(not(windows))]
    if let Err(e) = crossterm::execute!(
        std::io::stdout(),
        event::PushKeyboardEnhancementFlags(
            event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
        )
    ) {
        restore_input_mode();
        return Err(format!("keyboard event mode: {e}"));
    }
    let actions = ConsoleActions::default();
    let reader = actions.clone();
    let finished = Arc::new(AtomicBool::new(false));
    let bytes = KeyboardBytes::new(finished.clone());
    let handle = thread::Builder::new().spawn(move || {
        reader.read(bytes, |accepted| {
            if !accepted {
                eprintln!("入力は受け付けられませんでした（準備待ちへの切替中、待機外、または受付済み）。画面の準備待ちを確認し、開始はEnterをもう一度、スキップはs+Enterをもう一度押してください。\r");
            }
        });
    });
    match handle {
        Ok(reader) => Ok(ConsoleInput {
            actions,
            finished,
            reader: Some(reader),
        }),
        Err(e) => {
            restore_input_mode();
            Err(format!("keyboard reader: {e}"))
        }
    }
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

    struct SwitchingRead<F> {
        bytes: &'static [u8],
        offset: usize,
        empty_at: &'static [usize],
        emptied: usize,
        before_return: F,
    }
    impl<F: FnMut(usize)> Read for SwitchingRead<F> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.empty_at.get(self.emptied) == Some(&self.offset) {
                self.emptied += 1;
                return Err(ErrorKind::WouldBlock.into());
            }
            (self.before_return)(self.offset);
            let Some(&byte) = self.bytes.get(self.offset) else {
                return Ok(0);
            };
            out[0] = byte;
            self.offset += 1;
            Ok(1)
        }
    }

    #[test]
    fn actual_reader_never_rebinds_inflight_or_partial_lines_to_a_new_gate() {
        for bytes in [b"\n".as_slice(), b"s\n", b"skip\r\n", b"q\n"] {
            for old in [None, Some(1)] {
                for switch_at in 0..bytes.len() {
                    let input = ConsoleActions::default();
                    input.poll(old);
                    let mut notices = 0;
                    input.read(
                        SwitchingRead {
                            bytes,
                            offset: 0,
                            empty_at: &[0],
                            emptied: 0,
                            before_return: |offset| {
                                if offset == switch_at {
                                    input.poll(Some(2));
                                }
                                if offset == bytes.len() {
                                    let state = input.0.lock().unwrap();
                                    assert_eq!(
                                        state.pending, None,
                                        "{bytes:?}, {old:?}, {switch_at}"
                                    );
                                    assert_eq!(state.stopped, bytes == b"q\n");
                                }
                            },
                        },
                        |accepted| {
                            assert!(!accepted);
                            notices += 1;
                        },
                    );
                    assert_eq!(notices, usize::from(bytes != b"q\n"));
                    assert_eq!(input.poll(Some(3)), Some(Action::Stop));
                    assert_eq!(input.poll(None), Some(Action::Stop));
                }
            }
        }
    }

    #[test]
    fn actual_reader_rejects_a_closed_and_reopened_gate_generation() {
        let input = ConsoleActions::default();
        input.poll(Some(1));
        input.read(
            SwitchingRead {
                bytes: b"\n",
                offset: 0,
                empty_at: &[0],
                emptied: 0,
                before_return: |offset| {
                    if offset == 0 {
                        input.poll(None);
                        input.poll(Some(1));
                    } else {
                        assert_eq!(input.0.lock().unwrap().pending, None);
                    }
                },
            },
            |_| {},
        );
    }

    #[test]
    fn actual_reader_requests_retry_and_accepts_only_a_fresh_line() {
        let input = ConsoleActions::default();
        let mut notices = 0;
        input.read(
            SwitchingRead {
                bytes: b"\n\ns\nq\n",
                offset: 0,
                empty_at: &[0, 1],
                emptied: 0,
                before_return: |offset| match offset {
                    0 => {
                        input.poll(Some(2));
                    }
                    1 => assert_eq!(input.0.lock().unwrap().pending, None),
                    2 => {
                        assert_eq!(input.poll(Some(2)), Some(Action::Ready));
                        input.poll(Some(3));
                    }
                    4 => assert_eq!(input.0.lock().unwrap().pending, None),
                    _ => {}
                },
            },
            |accepted| {
                if !accepted {
                    notices += 1;
                }
            },
        );
        assert_eq!(notices, 2);
        assert_eq!(input.poll(Some(4)), Some(Action::Stop));
        assert_eq!(input.poll(None), Some(Action::Stop));
    }

    #[test]
    fn actual_reader_drains_multiline_backlog_after_acceptance_opens_next_gate() {
        for (bytes, first) in [
            (b"\n\ns\nskip\nq\n".as_slice(), Action::Ready),
            (b"s\n\ns\nskip\nq\n".as_slice(), Action::Skip),
        ] {
            let input = ConsoleActions::default();
            input.poll(Some(1));
            let mut accepted_count = 0;
            let mut rejected_count = 0;
            input.read(
                SwitchingRead {
                    bytes,
                    offset: 0,
                    empty_at: &[0],
                    emptied: 0,
                    before_return: |_| {},
                },
                |accepted| {
                    if accepted {
                        accepted_count += 1;
                        assert_eq!(input.poll(Some(1)), Some(first));
                        // Deterministically deschedule between accepted lines,
                        // BEFORE the next snapshot, with more bytes already queued.
                        input.poll(Some(2));
                    } else {
                        rejected_count += 1;
                        assert_eq!(input.poll(Some(2)), None);
                    }
                },
            );
            assert_eq!(accepted_count, 1);
            assert_eq!(rejected_count, 3);
            assert_eq!(input.poll(Some(3)), Some(Action::Stop));
        }
    }

    #[test]
    fn actual_reader_keeps_partial_line_stale_across_empty_queue_rearming() {
        let input = ConsoleActions::default();
        input.poll(Some(1));
        let mut rejected = 0;
        input.read(
            SwitchingRead {
                bytes: b"s\n\nq\n",
                offset: 0,
                empty_at: &[0, 1, 2],
                emptied: 0,
                before_return: |offset| {
                    if offset == 0 {
                        input.poll(Some(2));
                    }
                },
            },
            |accepted| {
                if accepted {
                    assert_eq!(input.poll(Some(2)), Some(Action::Ready));
                } else {
                    rejected += 1;
                }
            },
        );
        assert_eq!(rejected, 1);
        assert_eq!(input.poll(Some(3)), Some(Action::Stop));
    }

    #[test]
    fn actual_reader_control_c_stops_even_with_partial_unapproved_input() {
        let input = ConsoleActions::default();
        input.read(std::io::Cursor::new(b"skip\x03"), |_| panic!("no approval"));
        assert_eq!(input.poll(None), Some(Action::Stop));
        assert_eq!(input.poll(Some(2)), Some(Action::Stop));
    }

    #[test]
    fn keyboard_enter_repeat_after_queue_empty_cannot_approve_next_gate() {
        struct Keys {
            keyboard: KeyboardBytes,
            events: std::collections::VecDeque<Option<KeyEvent>>,
        }
        impl Read for Keys {
            fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
                while let Some(event) = self.events.pop_front() {
                    let Some(key) = event else {
                        return Err(ErrorKind::WouldBlock.into());
                    };
                    if let Some(byte) = self.keyboard.key_byte(key) {
                        out[0] = byte;
                        return Ok(1);
                    }
                }
                Ok(0)
            }
        }
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let mut release = enter;
        release.kind = KeyEventKind::Release;
        let input = ConsoleActions::default();
        input.poll(Some(1));
        let mut approvals = 0;
        input.read(
            Keys {
                keyboard: KeyboardBytes::new(Arc::new(AtomicBool::new(false))),
                events: [
                    None,
                    Some(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)),
                    Some(enter),
                    None,
                    Some(enter), // Windows reports typematic key-down as Press.
                    None,
                    Some(enter),
                    Some(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)),
                    Some(release),
                    Some(enter),
                ]
                .into(),
            },
            |accepted| {
                assert!(accepted);
                approvals += 1;
                assert_eq!(input.poll(Some(1)), Some(Action::Skip));
                input.poll(Some(2));
            },
        );
        assert_eq!(approvals, 1);
        assert_eq!(input.poll(Some(2)), Some(Action::Stop));
    }

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
        input.read(
            std::io::Cursor::new([bytes.as_slice(), b"\nq\n"].concat()),
            |_| {},
        );
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
