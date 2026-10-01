use std::fmt;
use std::io::{Read, Write};
use std::time::{Duration, Instant};

use selfrionette_core::{
    CompatibilityError, DeviceId, DeviceIdentity, DeviceInfo, HostCommand, ProtocolFrame,
    ProtocolParseError, parse_line,
};
use serialport::{ClearBuffer, SerialPort};

pub const SERIAL_BAUD_RATE: u32 = 115_200;
pub const MAX_LINE_BYTES: usize = 1024;
pub const DEFAULT_MAX_LINES: usize = 64;
pub const DEFAULT_QUERY_DEADLINE: Duration = Duration::from_secs(5);
const READ_SLICE_TIMEOUT: Duration = Duration::from_millis(20);

pub trait LineTransport {
    fn discard_input(&mut self) -> Result<(), TransportError>;
    fn write_line(&mut self, line: &str) -> Result<(), TransportError>;
    fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError>;
}

pub struct SerialPortTransport {
    port_name: String,
    port: Box<dyn SerialPort>,
}

impl SerialPortTransport {
    pub fn open(port_name: &str) -> Result<Self, TransportError> {
        let port = serialport::new(port_name, SERIAL_BAUD_RATE)
            .timeout(READ_SLICE_TIMEOUT)
            .open()
            .map_err(|error| TransportError::Open {
                port_name: port_name.to_owned(),
                message: error.to_string(),
            })?;

        Ok(Self {
            port_name: port_name.to_owned(),
            port,
        })
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }
}

impl LineTransport for SerialPortTransport {
    fn discard_input(&mut self) -> Result<(), TransportError> {
        self.port
            .clear(ClearBuffer::Input)
            .map_err(|error| TransportError::Serial(error.to_string()))
    }

    fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
        if !line.ends_with('\n') {
            return Err(TransportError::CommandNotTerminated);
        }

        self.port
            .write_all(line.as_bytes())
            .map_err(|error| TransportError::Io(error.to_string()))?;
        self.port
            .flush()
            .map_err(|error| TransportError::Io(error.to_string()))
    }

    fn read_line(&mut self, timeout: Duration) -> Result<String, TransportError> {
        let started = Instant::now();
        let mut bytes = Vec::with_capacity(128);

        loop {
            let remaining = timeout
                .checked_sub(started.elapsed())
                .ok_or(TransportError::Timeout)?;
            if remaining.is_zero() {
                return Err(TransportError::Timeout);
            }

            self.port
                .set_timeout(remaining.min(READ_SLICE_TIMEOUT))
                .map_err(|error| TransportError::Serial(error.to_string()))?;

            let mut byte = [0_u8; 1];
            match self.port.read(&mut byte) {
                Ok(0) => continue,
                Ok(_) => match byte[0] {
                    b'\r' => continue,
                    b'\n' => {
                        return String::from_utf8(bytes).map_err(|_| TransportError::InvalidUtf8);
                    }
                    value => {
                        if bytes.len() >= MAX_LINE_BYTES {
                            return Err(TransportError::LineTooLong {
                                max_bytes: MAX_LINE_BYTES,
                            });
                        }
                        bytes.push(value);
                    }
                },
                Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
                Err(error) => return Err(TransportError::Io(error.to_string())),
            }
        }
    }
}

pub fn available_port_names() -> Result<Vec<String>, TransportError> {
    let mut names = serialport::available_ports()
        .map_err(|error| TransportError::Serial(error.to_string()))?
        .into_iter()
        .map(|port| port.port_name)
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    Ok(names)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionLimits {
    pub deadline: Duration,
    pub max_lines: usize,
}

impl Default for SessionLimits {
    fn default() -> Self {
        Self {
            deadline: DEFAULT_QUERY_DEADLINE,
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

pub struct DeviceSession<T> {
    transport: T,
    limits: SessionLimits,
}

impl<T: LineTransport> DeviceSession<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            limits: SessionLimits::default(),
        }
    }

    pub fn with_limits(transport: T, limits: SessionLimits) -> Self {
        Self { transport, limits }
    }

    pub fn into_inner(self) -> T {
        self.transport
    }

    pub fn query_info(&mut self) -> Result<DeviceInfo, SessionError> {
        self.transport.discard_input()?;
        self.transport
            .write_line(&HostCommand::Info.encode_line())?;

        let started = Instant::now();
        for _ in 0..self.limits.max_lines {
            let line = self.read_with_deadline(started)?;
            match parse_line(&line)? {
                ProtocolFrame::Device(info) => {
                    info.validate_compatibility()?;
                    return Ok(info);
                }
                ProtocolFrame::Vector(_)
                | ProtocolFrame::Status(_)
                | ProtocolFrame::Warning(_)
                | ProtocolFrame::Unknown(_) => {}
            }
        }

        Err(SessionError::LineBudgetExceeded {
            max_lines: self.limits.max_lines,
        })
    }

    pub fn provision(&mut self, device_id: DeviceId) -> Result<DeviceInfo, SessionError> {
        let before = self.query_info()?;
        if let DeviceIdentity::Provisioned(existing) = before.identity {
            return Err(SessionError::AlreadyProvisioned(existing));
        }

        self.transport.discard_input()?;
        self.transport
            .write_line(&HostCommand::Provision(device_id).encode_line())?;
        self.wait_for_provision_result()?;

        let after = self.query_info()?;
        match after.identity {
            DeviceIdentity::Provisioned(actual) if actual == device_id => Ok(after),
            actual => Err(SessionError::ProvisionVerification {
                expected: device_id,
                actual,
            }),
        }
    }

    fn wait_for_provision_result(&mut self) -> Result<(), SessionError> {
        let started = Instant::now();
        for _ in 0..self.limits.max_lines {
            let line = self.read_with_deadline(started)?;
            match parse_line(&line)? {
                ProtocolFrame::Status(status) if status.token == "provision_ok" => return Ok(()),
                ProtocolFrame::Warning(warning)
                    if matches!(
                        warning.token.as_str(),
                        "already_provisioned"
                            | "invalid_device_id"
                            | "provision_verify_failed"
                            | "command_too_long"
                            | "unknown_command"
                    ) =>
                {
                    return Err(SessionError::ProvisionRejected(warning.token));
                }
                ProtocolFrame::Device(_)
                | ProtocolFrame::Vector(_)
                | ProtocolFrame::Status(_)
                | ProtocolFrame::Warning(_)
                | ProtocolFrame::Unknown(_) => {}
            }
        }

        Err(SessionError::LineBudgetExceeded {
            max_lines: self.limits.max_lines,
        })
    }

    fn read_with_deadline(&mut self, started: Instant) -> Result<String, SessionError> {
        let remaining = self
            .limits
            .deadline
            .checked_sub(started.elapsed())
            .ok_or(SessionError::DeadlineExceeded)?;
        if remaining.is_zero() {
            return Err(SessionError::DeadlineExceeded);
        }

        self.transport.read_line(remaining).map_err(Into::into)
    }
}

#[derive(Debug)]
pub enum TransportError {
    Open { port_name: String, message: String },
    Serial(String),
    Io(String),
    Timeout,
    LineTooLong { max_bytes: usize },
    InvalidUtf8,
    CommandNotTerminated,
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open { port_name, message } => {
                write!(
                    formatter,
                    "failed to open serial port {port_name:?}: {message}"
                )
            }
            Self::Serial(message) => write!(formatter, "serial operation failed: {message}"),
            Self::Io(message) => write!(formatter, "serial I/O failed: {message}"),
            Self::Timeout => formatter.write_str("serial read timed out"),
            Self::LineTooLong { max_bytes } => {
                write!(formatter, "serial line exceeded {max_bytes} bytes")
            }
            Self::InvalidUtf8 => formatter.write_str("serial line is not valid UTF-8"),
            Self::CommandNotTerminated => formatter.write_str("host command must end with newline"),
        }
    }
}

impl std::error::Error for TransportError {}

#[derive(Debug)]
pub enum SessionError {
    Transport(TransportError),
    Protocol(ProtocolParseError),
    Compatibility(CompatibilityError),
    DeadlineExceeded,
    LineBudgetExceeded {
        max_lines: usize,
    },
    AlreadyProvisioned(DeviceId),
    ProvisionRejected(String),
    ProvisionVerification {
        expected: DeviceId,
        actual: DeviceIdentity,
    },
}

impl From<TransportError> for SessionError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

impl From<ProtocolParseError> for SessionError {
    fn from(error: ProtocolParseError) -> Self {
        Self::Protocol(error)
    }
}

impl From<CompatibilityError> for SessionError {
    fn from(error: CompatibilityError) -> Self {
        Self::Compatibility(error)
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Transport(error) => error.fmt(formatter),
            Self::Protocol(error) => write!(formatter, "protocol parse failed: {error}"),
            Self::Compatibility(error) => write!(formatter, "device is incompatible: {error}"),
            Self::DeadlineExceeded => formatter.write_str("management query deadline exceeded"),
            Self::LineBudgetExceeded { max_lines } => {
                write!(formatter, "management query exceeded {max_lines} lines")
            }
            Self::AlreadyProvisioned(device_id) => {
                write!(formatter, "device is already provisioned as {device_id}")
            }
            Self::ProvisionRejected(token) => {
                write!(formatter, "device rejected provisioning: {token}")
            }
            Self::ProvisionVerification { expected, actual } => {
                write!(
                    formatter,
                    "provision verification mismatch: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for SessionError {}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    const TEST_ID: &str = "srn-0123456789abcdef0123456789abcdef";
    const OTHER_ID: &str = "srn-fedcba9876543210fedcba9876543210";

    struct FakeTransport {
        queued: VecDeque<String>,
        responses: VecDeque<Vec<String>>,
        writes: Vec<String>,
        discard_count: usize,
    }

    impl FakeTransport {
        fn with_responses(responses: Vec<Vec<&str>>) -> Self {
            Self {
                queued: VecDeque::new(),
                responses: responses
                    .into_iter()
                    .map(|batch| batch.into_iter().map(str::to_owned).collect())
                    .collect(),
                writes: Vec::new(),
                discard_count: 0,
            }
        }
    }

    impl LineTransport for FakeTransport {
        fn discard_input(&mut self) -> Result<(), TransportError> {
            self.queued.clear();
            self.discard_count += 1;
            Ok(())
        }

        fn write_line(&mut self, line: &str) -> Result<(), TransportError> {
            self.writes.push(line.to_owned());
            if let Some(batch) = self.responses.pop_front() {
                self.queued.extend(batch);
            }
            Ok(())
        }

        fn read_line(&mut self, _timeout: Duration) -> Result<String, TransportError> {
            self.queued.pop_front().ok_or(TransportError::Timeout)
        }
    }

    #[test]
    fn query_info_ignores_interleaved_non_device_frames() {
        let transport = FakeTransport::with_responses(vec![vec![
            "vector,1,1,2,3,4,5,6,7",
            "status,setup_end",
            "device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,7",
        ]]);
        let mut session = DeviceSession::new(transport);

        let info = session.query_info().expect("device info");
        assert_eq!(info.identity.to_string(), TEST_ID);

        let transport = session.into_inner();
        assert_eq!(transport.writes, vec!["info\n"]);
        assert_eq!(transport.discard_count, 1);
    }

    #[test]
    fn query_info_rejects_incompatible_protocol() {
        let transport = FakeTransport::with_responses(vec![vec!["device,3,0.1.0,unprovisioned,7"]]);
        let mut session = DeviceSession::new(transport);

        assert!(matches!(
            session.query_info(),
            Err(SessionError::Compatibility(
                CompatibilityError::ProtocolMajor { actual: 3, .. }
            ))
        ));
    }

    #[test]
    fn provision_verifies_identity_after_write() {
        let transport = FakeTransport::with_responses(vec![
            vec!["device,2,0.1.0,unprovisioned,7"],
            vec!["vector,1,1,2,3,4,5,6,7", "status,provision_ok"],
            vec!["device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,7"],
        ]);
        let mut session = DeviceSession::new(transport);
        let id: DeviceId = TEST_ID.parse().expect("test id");

        let info = session.provision(id).expect("provision succeeds");
        assert_eq!(info.identity, DeviceIdentity::Provisioned(id));

        let transport = session.into_inner();
        assert_eq!(
            transport.writes,
            vec![
                "info\n",
                "provision,srn-0123456789abcdef0123456789abcdef\n",
                "info\n",
            ]
        );
    }

    #[test]
    fn provision_refuses_already_provisioned_device() {
        let transport = FakeTransport::with_responses(vec![vec![
            "device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,7",
        ]]);
        let mut session = DeviceSession::new(transport);
        let requested: DeviceId = OTHER_ID.parse().expect("requested id");

        assert!(matches!(
            session.provision(requested),
            Err(SessionError::AlreadyProvisioned(_))
        ));

        let transport = session.into_inner();
        assert_eq!(transport.writes, vec!["info\n"]);
    }

    #[test]
    fn provision_rejects_management_warning() {
        let transport = FakeTransport::with_responses(vec![
            vec!["device,2,0.1.0,unprovisioned,7"],
            vec!["warn,provision_verify_failed"],
        ]);
        let mut session = DeviceSession::new(transport);
        let id: DeviceId = TEST_ID.parse().expect("test id");

        assert!(matches!(
            session.provision(id),
            Err(SessionError::ProvisionRejected(token))
                if token == "provision_verify_failed"
        ));
    }

    #[test]
    fn provision_rejects_mismatched_readback() {
        let transport = FakeTransport::with_responses(vec![
            vec!["device,2,0.1.0,unprovisioned,7"],
            vec!["status,provision_ok"],
            vec!["device,2,0.1.0,srn-fedcba9876543210fedcba9876543210,7"],
        ]);
        let mut session = DeviceSession::new(transport);
        let id: DeviceId = TEST_ID.parse().expect("test id");

        assert!(matches!(
            session.provision(id),
            Err(SessionError::ProvisionVerification { expected, .. })
                if expected == id
        ));
    }
}
