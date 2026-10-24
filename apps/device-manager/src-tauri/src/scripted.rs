//! Explicit developer-only transport. The production service and parsers are reused.
//! No OS port, device reset, EEPROM, or programmer is touched by this transport.
use selfrionette_serial::{
    LineTransport, TransportError,
    app::{MonotonicClock, SystemClock},
};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

pub struct State {
    pub raw: [i32; 7],
    pub flags: [u8; 7],
    pub connected: bool,
    pub(crate) identity: String,
    generation: u32,
    assembly: u32,
    known: u8,
    negative: u8,
    tare: u32,
    zero: [i32; 7],
    valid: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            raw: [1_000_000; 7],
            flags: [1; 7],
            connected: true,
            identity: "unprovisioned".into(),
            generation: 0,
            assembly: 0,
            known: 0,
            negative: 0,
            tare: 0,
            zero: [0; 7],
            valid: false,
        }
    }
}
pub type Shared = Arc<Mutex<State>>;
pub struct ScriptedTransport {
    pub state: Shared,
    queue: VecDeque<String>,
    session: u32,
    streaming: bool,
    sequence: u32,
    clock: Arc<dyn MonotonicClock>,
    origin: Instant,
    next: Instant,
}
impl ScriptedTransport {
    pub fn new(state: Shared) -> Self {
        Self::with_clock(state, Arc::new(SystemClock))
    }

    fn with_clock(state: Shared, clock: Arc<dyn MonotonicClock>) -> Self {
        let origin = clock.now();
        Self {
            state,
            queue: VecDeque::new(),
            session: 0,
            streaming: false,
            sequence: 0,
            clock,
            origin,
            next: origin,
        }
    }
}
impl LineTransport for ScriptedTransport {
    fn discard_input(&mut self) -> Result<(), TransportError> {
        self.queue.clear();
        Ok(())
    }
    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        let f = line.trim().split(',').collect::<Vec<_>>();
        let mut s = self
            .state
            .lock()
            .map_err(|_| TransportError::Io("fixture mutex".into()))?;
        let req = f.get(1).copied().unwrap_or("0");
        match f.first().copied(){
            Some("info3" | "?3")=>self.queue.push_back(format!("info3,{req},0.2.0-preview.2,9994bbbc927bcd87ff33a312df6167c72da07a6b,sparkfun_promicro16,atmega32u4,16000000,{},{},{},{},{},{},{}",s.identity,s.generation,s.assembly,s.known,s.negative,s.tare,u8::from(s.valid))),
            Some("stream3") if f.len()==4=>{let requested=f[2].parse().unwrap_or(0);if f[3]=="1"&&self.session!=requested{self.sequence=0;s.valid=false;}self.session=requested;self.streaming=f[3]=="1";self.queue.push_back(format!("ok3,{req},stream,{}",f[2]));},
            Some("settings3") if f.len()==7=>{
                if f[2]!=s.identity||f[3].parse::<u32>().ok()!=Some(s.generation){self.queue.push_back(format!("error3,{req},stale_generation"));}
                else{s.generation+=1;s.assembly=f[4].parse().unwrap_or(0);s.known=f[5].parse().unwrap_or(0);s.negative=f[6].parse().unwrap_or(0);s.valid=false;self.queue.push_back(format!("ok3,{req},settings,{}",s.generation));}
            },
            Some("tare3") if f.len()==4=>{
                s.valid=false;if f[2]!=s.identity||f[3].parse::<u32>().ok()!=Some(s.generation)||s.flags!=[1;7]{self.queue.push_back(format!("error3,{req},tare_quality"));}
                else{s.tare+=1;s.zero=s.raw;s.valid=true;self.queue.push_back(format!("ok3,{req},tare,{}",s.tare));}
            },
            Some("info")=>self.queue.push_back(format!("device,2,0.2.0-preview.2,{},7",s.identity)),
            Some("provision") if f.len()==2=>{if s.identity=="unprovisioned"{s.identity=f[1].into();self.queue.push_back("status,provision_ok".into());}else{self.queue.push_back("warn,already_provisioned".into());}},
            _=>self.queue.push_back(format!("error3,{req},unknown_command")),
        }
        Ok(())
    }
    fn read_line(&mut self, _timeout: Duration) -> Result<String, TransportError> {
        let s = self
            .state
            .lock()
            .map_err(|_| TransportError::Io("fixture mutex".into()))?;
        if !s.connected {
            return Err(TransportError::Io("scripted disconnect".into()));
        }
        if let Some(line) = self.queue.pop_front() {
            return Ok(line);
        }
        let now = self.clock.now();
        if !self.streaming || self.session == 0 || now < self.next {
            return Err(TransportError::Timeout);
        }
        self.next = now + Duration::from_millis(50);
        self.sequence += 1;
        // Samples are created here, not queued at an ideal 20 Hz. Include GUI/IPC
        // waits in producer time; retain the firmware's wrapping millisecond clock.
        let device_ms = now.saturating_duration_since(self.origin).as_millis() as u32;
        let mut fields = vec![
            "sample3".into(),
            self.session.to_string(),
            self.sequence.to_string(),
            device_ms.to_string(),
            s.tare.to_string(),
            s.generation.to_string(),
            u8::from(s.valid).to_string(),
        ];
        fields.extend(s.raw.iter().map(ToString::to_string));
        fields.extend((0..7).map(|ch| {
            if s.valid && s.known == 127 && s.flags[ch] == 1 {
                ((s.raw[ch] - s.zero[ch]) * if s.negative & (1 << ch) != 0 { -1 } else { 1 })
                    .to_string()
            } else {
                "?".into()
            }
        }));
        fields.extend(s.flags.iter().map(ToString::to_string));
        Ok(fields.join(","))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use selfrionette_core::protocol_v3::{Frame, Sample, parse_line};
    use selfrionette_serial::app::{Command, LiveService};
    use std::path::Path;

    struct ManualClock {
        origin: Instant,
        elapsed: Mutex<Duration>,
    }
    impl ManualClock {
        fn new() -> Self {
            Self {
                origin: Instant::now(),
                elapsed: Mutex::new(Duration::ZERO),
            }
        }
        fn set_ms(&self, milliseconds: u64) {
            *self.elapsed.lock().unwrap() = Duration::from_millis(milliseconds);
        }
    }
    impl MonotonicClock for ManualClock {
        fn now(&self) -> Instant {
            self.origin + *self.elapsed.lock().unwrap()
        }
    }

    #[test]
    fn delayed_gui_reads_do_not_invent_backlog_in_the_shared_service() {
        let clock = Arc::new(ManualClock::new());
        let transport =
            ScriptedTransport::with_clock(Arc::new(Mutex::new(State::default())), clock.clone());
        let mut service = LiveService::with_clock(Box::new(transport), clock.clone(), clock.origin);
        let store = Path::new("unused-scripted-clock-test");
        service.execute(Command::Connect, store).unwrap();
        service.execute(Command::StreamStart, store).unwrap();

        // Model rendering/IPC overhead and pauses longer than the real 250 ms
        // backlog limit. The transport creates one fresh sample after each wait.
        let times = [0, 85, 170, 255, 600, 685, 1000];
        let mut observed = Vec::new();
        for milliseconds in times {
            clock.set_ms(milliseconds);
            let reply = service.execute(Command::StreamTick, store).unwrap();
            assert!(reply.view.connected);
            assert_eq!(reply.sample_events.len(), 1, "no catch-up or replay frames");
            let sample = &reply.sample_events[0].sample;
            observed.push((sample.sequence, sample.timestamp_ms));
            assert_eq!(reply.view.diagnostics.relative_backlog_ms, Some(0));
            assert_eq!(reply.view.diagnostics.stale_invalidations, 0);
        }
        assert_eq!(
            observed,
            times
                .into_iter()
                .enumerate()
                .map(|(index, milliseconds)| (index as u32 + 1, milliseconds as u32))
                .collect::<Vec<_>>()
        );
    }

    fn read_sample(transport: &mut ScriptedTransport) -> Sample {
        match parse_line(&transport.read_line(Duration::ZERO).unwrap()).unwrap() {
            Frame::Sample(sample) => sample,
            other => panic!("expected a quality sample, got {other:?}"),
        }
    }

    #[test]
    fn producer_clock_keeps_cadence_and_wraps_without_resetting_for_a_new_session() {
        let clock = Arc::new(ManualClock::new());
        let mut transport =
            ScriptedTransport::with_clock(Arc::new(Mutex::new(State::default())), clock.clone());
        let before_wrap = u64::from(u32::MAX) - 25;
        clock.set_ms(before_wrap);
        transport.write_line("stream3,1,7,1").unwrap();
        assert_eq!(
            transport.read_line(Duration::ZERO).unwrap(),
            "ok3,1,stream,7"
        );
        let first = read_sample(&mut transport);
        assert_eq!((first.sequence, first.device_ms), (1, u32::MAX - 25));

        clock.set_ms(before_wrap + 49);
        assert!(matches!(
            transport.read_line(Duration::ZERO),
            Err(TransportError::Timeout)
        ));
        clock.set_ms(before_wrap + 50);
        let wrapped = read_sample(&mut transport);
        assert_eq!((wrapped.sequence, wrapped.device_ms), (2, 24));

        transport.write_line("stream3,2,8,1").unwrap();
        assert_eq!(
            transport.read_line(Duration::ZERO).unwrap(),
            "ok3,2,stream,8"
        );
        clock.set_ms(before_wrap + 150);
        let next_session = read_sample(&mut transport);
        assert_eq!(
            (
                next_session.session,
                next_session.sequence,
                next_session.device_ms
            ),
            (8, 1, 124)
        );
    }
}
