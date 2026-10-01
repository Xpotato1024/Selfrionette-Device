use core::fmt;

use crate::identity::{DeviceId, DeviceIdParseError};

pub const PROTOCOL_MAJOR_VERSION: u8 = 2;
pub const CHANNEL_COUNT: usize = 7;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceIdentity {
    Provisioned(DeviceId),
    Unprovisioned,
}

impl fmt::Display for DeviceIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Provisioned(device_id) => device_id.fmt(formatter),
            Self::Unprovisioned => formatter.write_str("unprovisioned"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub protocol_major: u8,
    pub firmware_version: String,
    pub identity: DeviceIdentity,
    pub channel_count: u8,
}

impl DeviceInfo {
    pub fn validate_compatibility(&self) -> Result<(), CompatibilityError> {
        if self.protocol_major != PROTOCOL_MAJOR_VERSION {
            return Err(CompatibilityError::ProtocolMajor {
                expected: PROTOCOL_MAJOR_VERSION,
                actual: self.protocol_major,
            });
        }

        if self.channel_count as usize != CHANNEL_COUNT {
            return Err(CompatibilityError::ChannelCount {
                expected: CHANNEL_COUNT as u8,
                actual: self.channel_count,
            });
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VectorFrame {
    pub timestamp_ms: u32,
    pub channels: [f64; CHANNEL_COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticFrame {
    pub token: String,
    pub args: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownFrame {
    pub prefix: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProtocolFrame {
    Device(DeviceInfo),
    Vector(VectorFrame),
    Status(DiagnosticFrame),
    Warning(DiagnosticFrame),
    Unknown(UnknownFrame),
}

pub fn parse_line(line: &str) -> Result<ProtocolFrame, ProtocolParseError> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Err(ProtocolParseError::EmptyLine);
    }

    let fields: Vec<&str> = line.split(',').collect();
    let prefix = fields[0];
    match prefix {
        "device" => parse_device_frame(&fields),
        "vector" => parse_vector_frame(&fields),
        "status" => parse_diagnostic_frame(&fields, true),
        "warn" => parse_diagnostic_frame(&fields, false),
        "" => Err(ProtocolParseError::EmptyPrefix),
        _ => Ok(ProtocolFrame::Unknown(UnknownFrame {
            prefix: prefix.to_owned(),
            fields: fields[1..]
                .iter()
                .map(|field| (*field).to_owned())
                .collect(),
        })),
    }
}

fn parse_device_frame(fields: &[&str]) -> Result<ProtocolFrame, ProtocolParseError> {
    if fields.len() != 5 {
        return Err(ProtocolParseError::WrongFieldCount {
            frame: "device",
            expected: 5,
            actual: fields.len(),
        });
    }

    let protocol_major =
        fields[1]
            .parse::<u8>()
            .map_err(|_| ProtocolParseError::InvalidInteger {
                field: "protocol_major",
                value: fields[1].to_owned(),
            })?;

    if fields[2].is_empty() {
        return Err(ProtocolParseError::EmptyField {
            field: "firmware_version",
        });
    }

    let identity = if fields[3] == "unprovisioned" {
        DeviceIdentity::Unprovisioned
    } else {
        DeviceIdentity::Provisioned(
            fields[3]
                .parse::<DeviceId>()
                .map_err(ProtocolParseError::InvalidDeviceId)?,
        )
    };

    let channel_count =
        fields[4]
            .parse::<u8>()
            .map_err(|_| ProtocolParseError::InvalidInteger {
                field: "channel_count",
                value: fields[4].to_owned(),
            })?;

    Ok(ProtocolFrame::Device(DeviceInfo {
        protocol_major,
        firmware_version: fields[2].to_owned(),
        identity,
        channel_count,
    }))
}

fn parse_vector_frame(fields: &[&str]) -> Result<ProtocolFrame, ProtocolParseError> {
    if fields.len() != 2 + CHANNEL_COUNT {
        return Err(ProtocolParseError::WrongFieldCount {
            frame: "vector",
            expected: 2 + CHANNEL_COUNT,
            actual: fields.len(),
        });
    }

    let timestamp_ms =
        fields[1]
            .parse::<u32>()
            .map_err(|_| ProtocolParseError::InvalidInteger {
                field: "timestamp_ms",
                value: fields[1].to_owned(),
            })?;

    let mut channels = [0.0_f64; CHANNEL_COUNT];
    for (index, target) in channels.iter_mut().enumerate() {
        let field_index = index + 2;
        let value =
            fields[field_index]
                .parse::<f64>()
                .map_err(|_| ProtocolParseError::InvalidFloat {
                    channel: index,
                    value: fields[field_index].to_owned(),
                })?;
        if !value.is_finite() {
            return Err(ProtocolParseError::NonFiniteFloat {
                channel: index,
                value: fields[field_index].to_owned(),
            });
        }
        *target = value;
    }

    Ok(ProtocolFrame::Vector(VectorFrame {
        timestamp_ms,
        channels,
    }))
}

fn parse_diagnostic_frame(
    fields: &[&str],
    is_status: bool,
) -> Result<ProtocolFrame, ProtocolParseError> {
    if fields.len() < 2 || fields[1].is_empty() {
        return Err(ProtocolParseError::MissingDiagnosticToken);
    }

    let frame = DiagnosticFrame {
        token: fields[1].to_owned(),
        args: fields[2..]
            .iter()
            .map(|field| (*field).to_owned())
            .collect(),
    };

    Ok(if is_status {
        ProtocolFrame::Status(frame)
    } else {
        ProtocolFrame::Warning(frame)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostCommand {
    Info,
    Tare,
    Provision(DeviceId),
}

impl HostCommand {
    pub fn encode_line(&self) -> String {
        match self {
            Self::Info => "info\n".to_owned(),
            Self::Tare => "tare\n".to_owned(),
            Self::Provision(device_id) => format!("provision,{device_id}\n"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompatibilityError {
    ProtocolMajor { expected: u8, actual: u8 },
    ChannelCount { expected: u8, actual: u8 },
}

impl fmt::Display for CompatibilityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProtocolMajor { expected, actual } => {
                write!(
                    formatter,
                    "unsupported protocol major: expected {expected}, got {actual}"
                )
            }
            Self::ChannelCount { expected, actual } => {
                write!(
                    formatter,
                    "incompatible channel count: expected {expected}, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for CompatibilityError {}

#[derive(Clone, Debug, PartialEq)]
pub enum ProtocolParseError {
    EmptyLine,
    EmptyPrefix,
    WrongFieldCount {
        frame: &'static str,
        expected: usize,
        actual: usize,
    },
    EmptyField {
        field: &'static str,
    },
    InvalidInteger {
        field: &'static str,
        value: String,
    },
    InvalidFloat {
        channel: usize,
        value: String,
    },
    NonFiniteFloat {
        channel: usize,
        value: String,
    },
    MissingDiagnosticToken,
    InvalidDeviceId(DeviceIdParseError),
}

impl fmt::Display for ProtocolParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyLine => formatter.write_str("protocol line is empty"),
            Self::EmptyPrefix => formatter.write_str("protocol prefix is empty"),
            Self::WrongFieldCount {
                frame,
                expected,
                actual,
            } => write!(
                formatter,
                "{frame} frame must contain exactly {expected} fields, got {actual}"
            ),
            Self::EmptyField { field } => write!(formatter, "{field} must not be empty"),
            Self::InvalidInteger { field, value } => {
                write!(formatter, "invalid integer for {field}: {value:?}")
            }
            Self::InvalidFloat { channel, value } => {
                write!(formatter, "invalid float for channel {channel}: {value:?}")
            }
            Self::NonFiniteFloat { channel, value } => {
                write!(
                    formatter,
                    "non-finite float for channel {channel}: {value:?}"
                )
            }
            Self::MissingDiagnosticToken => {
                formatter.write_str("diagnostic frame must contain a non-empty token")
            }
            Self::InvalidDeviceId(error) => write!(formatter, "invalid device id: {error}"),
        }
    }
}

impl std::error::Error for ProtocolParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_provisioned_device_frame() {
        let frame = parse_line("device,2,0.1.0,srn-0123456789abcdef0123456789abcdef,7\r\n")
            .expect("valid device frame");

        let ProtocolFrame::Device(info) = frame else {
            panic!("expected device frame");
        };
        assert_eq!(info.protocol_major, 2);
        assert_eq!(info.firmware_version, "0.1.0");
        assert_eq!(info.channel_count, 7);
        assert!(matches!(info.identity, DeviceIdentity::Provisioned(_)));
        assert_eq!(info.validate_compatibility(), Ok(()));
    }

    #[test]
    fn parses_unprovisioned_device_frame() {
        let frame = parse_line("device,2,0.1.0,unprovisioned,7").expect("valid device frame");
        let ProtocolFrame::Device(info) = frame else {
            panic!("expected device frame");
        };
        assert_eq!(info.identity, DeviceIdentity::Unprovisioned);
    }

    #[test]
    fn compatibility_rejects_wrong_protocol() {
        let info = DeviceInfo {
            protocol_major: 3,
            firmware_version: "0.1.0".to_owned(),
            identity: DeviceIdentity::Unprovisioned,
            channel_count: 7,
        };
        assert_eq!(
            info.validate_compatibility(),
            Err(CompatibilityError::ProtocolMajor {
                expected: 2,
                actual: 3,
            })
        );
    }

    #[test]
    fn compatibility_rejects_wrong_channel_count() {
        let info = DeviceInfo {
            protocol_major: 2,
            firmware_version: "0.1.0".to_owned(),
            identity: DeviceIdentity::Unprovisioned,
            channel_count: 6,
        };
        assert_eq!(
            info.validate_compatibility(),
            Err(CompatibilityError::ChannelCount {
                expected: 7,
                actual: 6,
            })
        );
    }

    #[test]
    fn parses_vector_frame() {
        let frame = parse_line("vector,1234,1,2,3,4,5,6,7").expect("valid vector");
        let ProtocolFrame::Vector(vector) = frame else {
            panic!("expected vector frame");
        };
        assert_eq!(vector.timestamp_ms, 1234);
        assert_eq!(vector.channels, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn rejects_non_finite_vector_value() {
        let error = parse_line("vector,1234,1,2,3,4,NaN,6,7").expect_err("NaN must fail");
        assert!(matches!(
            error,
            ProtocolParseError::NonFiniteFloat { channel: 4, .. }
        ));
    }

    #[test]
    fn rejects_wrong_vector_field_count() {
        let error = parse_line("vector,1234,1,2,3").expect_err("short vector must fail");
        assert_eq!(
            error,
            ProtocolParseError::WrongFieldCount {
                frame: "vector",
                expected: 9,
                actual: 5,
            }
        );
    }

    #[test]
    fn parses_status_and_warning_frames() {
        assert_eq!(
            parse_line("status,provision_ok").expect("status"),
            ProtocolFrame::Status(DiagnosticFrame {
                token: "provision_ok".to_owned(),
                args: vec![],
            })
        );
        assert_eq!(
            parse_line("warn,ready_timeout,3").expect("warn"),
            ProtocolFrame::Warning(DiagnosticFrame {
                token: "ready_timeout".to_owned(),
                args: vec!["3".to_owned()],
            })
        );
    }

    #[test]
    fn unknown_prefix_is_not_a_sample() {
        assert_eq!(
            parse_line("future,one,two").expect("unknown diagnostic"),
            ProtocolFrame::Unknown(UnknownFrame {
                prefix: "future".to_owned(),
                fields: vec!["one".to_owned(), "two".to_owned()],
            })
        );
    }

    #[test]
    fn encodes_canonical_host_commands() {
        let id: DeviceId = "srn-0123456789abcdef0123456789abcdef"
            .parse()
            .expect("device id");
        assert_eq!(HostCommand::Info.encode_line(), "info\n");
        assert_eq!(HostCommand::Tare.encode_line(), "tare\n");
        assert_eq!(
            HostCommand::Provision(id).encode_line(),
            "provision,srn-0123456789abcdef0123456789abcdef\n"
        );
    }
}
