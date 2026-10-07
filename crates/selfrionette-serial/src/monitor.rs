use std::fmt;
use std::io;
use std::time::Instant;

use selfrionette_core::{DeviceId, DeviceIdentity, DeviceInfo, ProtocolFrame, parse_line};

use crate::{DeviceSession, LineTransport, SessionError};

impl<T: LineTransport> DeviceSession<T> {
    /// Consume one session, binding every emitted frame to a freshly verified identity.
    ///
    /// This is a bounded diagnostic capture, not a calibration or sensor-health check.
    /// Callbacks run synchronously and should return promptly. Reconnect requires a
    /// new transport and handshake; this session cannot be reused after failure.
    pub fn monitor(
        mut self,
        expected_id: DeviceId,
        sample_count: usize,
        mut on_frame: impl FnMut(&DeviceInfo, &ProtocolFrame) -> io::Result<()>,
    ) -> Result<DeviceInfo, MonitorError> {
        if sample_count == 0 || sample_count > self.limits.max_lines {
            return Err(MonitorError::InvalidSampleCount {
                requested: sample_count,
                max_samples: self.limits.max_lines,
            });
        }

        let info = self.query_info()?;
        if info.identity != DeviceIdentity::Provisioned(expected_id) {
            return Err(MonitorError::IdentityMismatch {
                expected: expected_id,
                actual: info.identity,
            });
        }
        on_frame(&info, &ProtocolFrame::Device(info.clone())).map_err(MonitorError::Output)?;

        // Do not clear the queue again: samples following the handshake belong
        // to this session. The existing line/deadline bounds also cover diagnostics.
        let started = Instant::now();
        let mut samples = 0;
        for _ in 0..self.limits.max_lines {
            let line = self.read_with_deadline(started)?;
            let frame = parse_line(&line).map_err(SessionError::from)?;
            if let ProtocolFrame::Device(actual) = &frame {
                actual
                    .validate_compatibility()
                    .map_err(SessionError::from)?;
                if actual != &info {
                    return Err(MonitorError::MetadataChanged {
                        expected: info,
                        actual: actual.clone(),
                    });
                }
            }
            on_frame(&info, &frame).map_err(MonitorError::Output)?;
            if matches!(frame, ProtocolFrame::Vector(_)) {
                samples += 1;
                if samples == sample_count {
                    return Ok(info);
                }
            }
        }
        Err(SessionError::LineBudgetExceeded {
            max_lines: self.limits.max_lines,
        }
        .into())
    }
}

#[derive(Debug)]
pub enum MonitorError {
    Session(SessionError),
    InvalidSampleCount {
        requested: usize,
        max_samples: usize,
    },
    IdentityMismatch {
        expected: DeviceId,
        actual: DeviceIdentity,
    },
    MetadataChanged {
        expected: DeviceInfo,
        actual: DeviceInfo,
    },
    Output(io::Error),
}

impl From<SessionError> for MonitorError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

impl fmt::Display for MonitorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => error.fmt(formatter),
            Self::InvalidSampleCount {
                requested,
                max_samples,
            } => {
                write!(
                    formatter,
                    "sample count must be 1..={max_samples}, got {requested}"
                )
            }
            Self::IdentityMismatch { expected, actual } => {
                write!(
                    formatter,
                    "monitor identity mismatch: expected {expected}, got {actual}"
                )
            }
            Self::MetadataChanged { expected, actual } => {
                write!(
                    formatter,
                    "device metadata changed during monitor: expected {expected:?}, got {actual:?}"
                )
            }
            Self::Output(error) => write!(formatter, "monitor output failed: {error}"),
        }
    }
}

impl std::error::Error for MonitorError {}
