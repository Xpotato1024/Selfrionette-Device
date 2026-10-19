//! Bounded stdio transport for the shared, single-device bridge session.
use crate::CliError;
use selfrionette_serial::{
    LineTransport, SerialPortTransport,
    app::{MonotonicClock, SystemClock},
    bridge::{
        BridgeEvent, BridgeRequest, BridgeSession, MAX_COMMAND_QUEUE, MAX_EVENT_QUEUE,
        MAX_IPC_LINE_BYTES, OpenTransport,
    },
};
use std::{
    io::{self, BufRead, BufReader, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

const BUILD_REVISION: &str = match option_env!("SELFRIONETTE_BRIDGE_REVISION") {
    Some(revision) => revision,
    None => "unknown",
};
const COMMAND_WAIT: Duration = Duration::from_millis(5);
const WRITER_FINISH_TIMEOUT: Duration = Duration::from_millis(200);

enum InputMessage {
    Request(Box<BridgeRequest>),
    Failed(String),
    Eof,
}

/// `read_until`/`lines` can allocate without limit before seeing a newline.
fn read_bounded_line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::with_capacity(1024);
    loop {
        let available = reader
            .fill_buf()
            .map_err(|error| format!("stdin read failed: {error}"))?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("stdin closed inside a JSON line".into())
            };
        }
        let take = available
            .iter()
            .position(|&b| b == b'\n')
            .map_or(available.len(), |i| i + 1);
        if line.len().saturating_add(take) > MAX_IPC_LINE_BYTES {
            return Err("command line exceeds 64 KiB including newline".into());
        }
        let ended = available[take - 1] == b'\n';
        line.extend_from_slice(&available[..take]);
        reader.consume(take);
        if ended {
            return Ok(Some(line));
        }
    }
}

fn read_commands(
    reader: &mut impl BufRead,
    commands: SyncSender<InputMessage>,
    overflow: &AtomicBool,
) {
    loop {
        let message = match read_bounded_line(reader) {
            Ok(Some(line)) => match serde_json::from_slice::<BridgeRequest>(&line) {
                Ok(request) => InputMessage::Request(Box::new(request)),
                Err(error) => InputMessage::Failed(format!("invalid bridge command: {error}")),
            },
            Ok(None) => InputMessage::Eof,
            Err(error) => InputMessage::Failed(error),
        };
        let terminal = !matches!(&message, InputMessage::Request(_));
        match commands.try_send(message) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                overflow.store(true, Ordering::Release);
                return;
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
        if terminal {
            return;
        }
    }
}

fn write_events(
    writer: &mut impl Write,
    events: Receiver<BridgeEvent>,
    clock: &dyn MonotonicClock,
    origin: Instant,
) -> Result<(), String> {
    for mut event in events {
        let emitted_us = clock
            .now()
            .saturating_duration_since(origin)
            .as_micros()
            .min(u128::from(u64::MAX)) as u64;
        event.stamp_emitted(emitted_us);
        let mut line = serde_json::to_vec(&event)
            .map_err(|error| format!("event serialization failed: {error}"))?;
        if line.len() >= MAX_IPC_LINE_BYTES {
            return Err("event exceeds 64 KiB including newline".into());
        }
        line.push(b'\n');
        writer
            .write_all(&line)
            .and_then(|_| writer.flush())
            .map_err(|error| format!("stdout write failed: {error}"))?;
    }
    Ok(())
}

fn queue_event(events: &SyncSender<BridgeEvent>, event: BridgeEvent) -> Result<(), String> {
    events.try_send(event).map_err(|error| match error {
        TrySendError::Full(_) => "stdout event queue exhausted; input invalidated".into(),
        TrySendError::Disconnected(_) => {
            "stdout event consumer disconnected; input invalidated".into()
        }
    })
}

fn drive(
    session: &mut BridgeSession,
    commands: Receiver<InputMessage>,
    events: &SyncSender<BridgeEvent>,
    input_overflow: &AtomicBool,
    output_failed: &AtomicBool,
    interrupted: &AtomicBool,
    open: &mut OpenTransport<'_>,
) -> Result<(), String> {
    queue_event(events, session.status())?;
    loop {
        if input_overflow.load(Ordering::Acquire) {
            return Err("stdin command queue exhausted".into());
        }
        if output_failed.load(Ordering::Acquire) {
            return Err("stdout writer failed".into());
        }
        if interrupted.load(Ordering::Acquire) {
            return Err("bridge interrupted".into());
        }
        match commands.recv_timeout(COMMAND_WAIT) {
            Ok(InputMessage::Request(request)) => {
                queue_event(events, session.handle(*request, open)?)?
            }
            Ok(InputMessage::Failed(error)) => return Err(error),
            Ok(InputMessage::Eof) | Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if session.is_polling() {
            for event in session.tick()? {
                queue_event(events, event)?;
            }
        }
    }
}

pub fn run(args: &[String]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::Usage(
            "bridge accepts commands only through versioned stdin JSON Lines".into(),
        ));
    }
    let origin = Instant::now();
    let clock: Arc<dyn MonotonicClock> = Arc::new(SystemClock);
    let interrupted = Arc::new(AtomicBool::new(false));
    let signal = interrupted.clone();
    ctrlc::set_handler(move || signal.store(true, Ordering::Release))
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let input_overflow = Arc::new(AtomicBool::new(false));
    let output_failed = Arc::new(AtomicBool::new(false));
    let (command_tx, command_rx) = mpsc::sync_channel(MAX_COMMAND_QUEUE);
    let (event_tx, event_rx) = mpsc::sync_channel(MAX_EVENT_QUEUE);
    let (writer_done_tx, writer_done_rx) = mpsc::sync_channel(1);
    let reader_overflow = input_overflow.clone();
    // Only these pipe workers can block on the parent. Serial ownership remains
    // on this thread, with bounded queues and fail-closed backpressure.
    thread::spawn(move || {
        read_commands(
            &mut BufReader::with_capacity(8192, io::stdin().lock()),
            command_tx,
            &reader_overflow,
        )
    });
    let writer_clock = clock.clone();
    let writer_failed = output_failed.clone();
    thread::spawn(move || {
        let result = write_events(
            &mut io::stdout().lock(),
            event_rx,
            writer_clock.as_ref(),
            origin,
        );
        if result.is_err() {
            writer_failed.store(true, Ordering::Release);
        }
        let _ = writer_done_tx.try_send(result);
    });
    let mut session = BridgeSession::new(BUILD_REVISION, clock, origin);
    let mut open = |port: &str| -> Result<Box<dyn LineTransport + Send>, String> {
        SerialPortTransport::open_with_dtr(port)
            .map(|transport| Box::new(transport) as Box<dyn LineTransport + Send>)
            .map_err(|error| error.to_string())
    };
    let result = drive(
        &mut session,
        command_rx,
        &event_tx,
        &input_overflow,
        &output_failed,
        &interrupted,
        &mut open,
    );
    let result = match result {
        Err(error) => {
            let _ = queue_event(&event_tx, session.fail(&error));
            Err(error)
        }
        Ok(()) => session.shutdown(),
    };
    drop(event_tx);
    // A parent that stopped reading must not retain an exclusively owned port.
    // Never join a blocked pipe worker; process exit terminates those workers.
    let writer_result = writer_done_rx
        .recv_timeout(WRITER_FINISH_TIMEOUT)
        .map_err(|_| "stdout did not finish within the bounded shutdown interval".to_owned())
        .and_then(|result| result);
    if interrupted.load(Ordering::Acquire) {
        return Err(CliError::Interrupted);
    }
    result.and(writer_result).map_err(CliError::Failure)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn bounded_reader_rejects_overlong_incomplete_and_invalid_commands() {
        let mut input = BufReader::with_capacity(7, Cursor::new(b"{}\n{}\r\n"));
        assert_eq!(read_bounded_line(&mut input).unwrap().unwrap(), b"{}\n");
        assert_eq!(read_bounded_line(&mut input).unwrap().unwrap(), b"{}\r\n");
        assert!(read_bounded_line(&mut input).unwrap().is_none());
        for bytes in [
            vec![b'x'; MAX_IPC_LINE_BYTES + 1],
            b"{\"action\":\"status\"}".to_vec(),
        ] {
            assert!(read_bounded_line(&mut Cursor::new(bytes)).is_err());
        }
        let mut exact = vec![b' '; MAX_IPC_LINE_BYTES];
        exact[MAX_IPC_LINE_BYTES - 1] = b'\n';
        assert_eq!(
            read_bounded_line(&mut Cursor::new(exact))
                .unwrap()
                .unwrap()
                .len(),
            MAX_IPC_LINE_BYTES
        );
        let (tx, rx) = mpsc::sync_channel(2);
        read_commands(
            &mut Cursor::new(vec![0xff, b'\n']),
            tx,
            &AtomicBool::new(false),
        );
        assert!(matches!(rx.recv().unwrap(), InputMessage::Failed(_)));
    }

    #[test]
    fn queue_backpressure_fails_without_blocking_or_opening_a_device() {
        let line = "{\"schema\":\"selfrionette-bridge-command/v1\",\"request_id\":1,\"action\":\"status\"}\n";
        let (tx, _rx) = mpsc::sync_channel(1);
        let overflow = AtomicBool::new(false);
        read_commands(&mut Cursor::new(line.repeat(2)), tx, &overflow);
        assert!(overflow.load(Ordering::Acquire));
        let origin = Instant::now();
        let mut session = BridgeSession::new("unknown", Arc::new(SystemClock), origin);
        let (tx, _rx) = mpsc::sync_channel(1);
        queue_event(&tx, session.status()).unwrap();
        assert!(
            queue_event(&tx, session.status())
                .unwrap_err()
                .contains("queue exhausted")
        );
    }

    #[test]
    fn eof_and_writer_failure_finish_the_owner_without_serial_access() {
        let origin = Instant::now();
        let mut session = BridgeSession::new("unknown", Arc::new(SystemClock), origin);
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(InputMessage::Eof).unwrap();
        let (events, _reader) = mpsc::sync_channel(2);
        let mut open = |_port: &str| -> Result<Box<dyn LineTransport + Send>, String> {
            panic!("read-only startup must not open serial")
        };
        drive(
            &mut session,
            rx,
            &events,
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            &AtomicBool::new(false),
            &mut open,
        )
        .unwrap();
        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(session.status()).unwrap();
        drop(tx);
        assert!(
            write_events(&mut BrokenWriter, rx, &SystemClock, origin)
                .unwrap_err()
                .contains("stdout write failed")
        );
    }
}
