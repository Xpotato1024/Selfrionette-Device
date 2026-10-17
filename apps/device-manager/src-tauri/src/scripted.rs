//! Explicit developer-only transport. The production service and parsers are reused.
//! No OS port, device reset, EEPROM, or programmer is touched by this transport.
use selfrionette_serial::{LineTransport, TransportError};
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
    next: Instant,
}
impl ScriptedTransport {
    pub fn new(state: Shared) -> Self {
        Self {
            state,
            queue: VecDeque::new(),
            session: 0,
            streaming: false,
            sequence: 0,
            next: Instant::now(),
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
        if !self.streaming || self.session == 0 || Instant::now() < self.next {
            return Err(TransportError::Timeout);
        }
        self.next = Instant::now() + Duration::from_millis(50);
        self.sequence += 1;
        let mut fields = vec![
            "sample3".into(),
            self.session.to_string(),
            self.sequence.to_string(),
            (self.sequence * 50).to_string(),
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
