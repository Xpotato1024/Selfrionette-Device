//! Strict parser and encoder for the opt-in quality-bearing v3 wire protocol.
use crate::{CHANNEL_COUNT, DeviceId, DeviceIdentity};
use std::fmt;

pub const MAX_FRAME_BYTES: usize = 511;
pub const MAX_COMMAND_BYTES: usize = 87;
pub const ADC_MIN: i32 = -8_388_608;
pub const ADC_MAX: i32 = 8_388_607;
pub const ORIENTED_MIN: i32 = -16_777_215;
pub const ORIENTED_MAX: i32 = 16_777_215;
pub const FLAG_FRESH: u8 = 1;
pub const FLAG_TIMEOUT: u8 = 2;
pub const FLAG_SATURATED: u8 = 4;
pub const FLAG_HELD: u8 = 8;
pub const FLAG_REJECTED: u8 = 16;
const VALID_FLAGS: u8 = FLAG_FRESH | FLAG_TIMEOUT | FLAG_SATURATED | FLAG_HELD | FLAG_REJECTED;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub firmware_version: String,
    pub source_revision: String,
    pub board: String,
    pub mcu: String,
    pub clock_hz: u32,
    pub identity: DeviceIdentity,
    pub generation: u32,
    pub assembly_revision: u32,
    pub known_mask: u8,
    pub negative_mask: u8,
    pub tare_epoch: u32,
    pub tare_valid: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub session: u32,
    pub sequence: u32,
    pub device_ms: u32,
    pub tare_epoch: u32,
    pub generation: u32,
    pub tare_valid: bool,
    /// Original sensor wire order, diagnostic-only when flags are not fresh.
    pub raw: [Option<i32>; CHANNEL_COUNT],
    /// Firmware-oriented wire order. Host mapping may permute it, never negate it again.
    pub oriented: [Option<i32>; CHANNEL_COUNT],
    pub flags: [u8; CHANNEL_COUNT],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Info {
        request: u32,
        info: DeviceInfo,
    },
    Sample(Sample),
    Ok {
        request: u32,
        operation: String,
        value: u32,
    },
    Error {
        request: u32,
        token: String,
    },
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Empty,
    TooLong,
    FieldCount {
        frame: &'static str,
        expected: usize,
        actual: usize,
    },
    InvalidInteger {
        field: &'static str,
    },
    InvalidBoolean {
        field: &'static str,
    },
    InvalidIdentity,
    InvalidSourceRevision,
    InvalidMetadata {
        field: &'static str,
    },
    InvalidMask,
    InvalidCount {
        field: &'static str,
        channel: usize,
    },
    InvalidFlags {
        channel: usize,
    },
    InvalidRequest,
    EmptyToken,
    InvalidAscii,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for ParseError {}

fn uint<T: std::str::FromStr>(value: &str, field: &'static str) -> Result<T, ParseError> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ParseError::InvalidInteger { field });
    }
    value
        .parse()
        .map_err(|_| ParseError::InvalidInteger { field })
}

fn bool01(value: &str, field: &'static str) -> Result<bool, ParseError> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(ParseError::InvalidBoolean { field }),
    }
}

fn token(value: &str, field: &'static str, max: usize) -> Result<String, ParseError> {
    if value.is_empty()
        || value.len() > max
        || !value.is_ascii()
        || value.bytes().any(|b| b.is_ascii_control() || b == b',')
    {
        return Err(ParseError::InvalidMetadata { field });
    }
    Ok(value.to_owned())
}

pub fn parse_line(line: &str) -> Result<Frame, ParseError> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        return Err(ParseError::Empty);
    }
    if line.len() > MAX_FRAME_BYTES {
        return Err(ParseError::TooLong);
    }
    if !line.is_ascii() {
        return Err(ParseError::InvalidAscii);
    }
    let fields: Vec<_> = line.split(',').collect();
    match fields.first().copied().unwrap_or_default() {
        "info3" => parse_info(&fields),
        "sample3" => parse_sample(&fields),
        "ok3" => parse_ok(&fields),
        "error3" => parse_error(&fields),
        _ => Ok(Frame::Other),
    }
}

fn parse_info(fields: &[&str]) -> Result<Frame, ParseError> {
    const COUNT: usize = 14;
    if fields.len() != COUNT {
        return Err(ParseError::FieldCount {
            frame: "info3",
            expected: COUNT,
            actual: fields.len(),
        });
    }
    let request = uint(fields[1], "request")?;
    if request == 0 {
        return Err(ParseError::InvalidRequest);
    }
    let firmware_version = token(fields[2], "firmware_version", 64)?;
    let source_revision = fields[3];
    if source_revision.len() != 40
        || source_revision.bytes().all(|b| b == b'0')
        || !source_revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(ParseError::InvalidSourceRevision);
    }
    let board = token(fields[4], "board", 64)?;
    let mcu = token(fields[5], "mcu", 64)?;
    let clock_hz = uint(fields[6], "clock_hz")?;
    if board != "sparkfun_promicro16" || mcu != "atmega32u4" || clock_hz != 16_000_000 {
        return Err(ParseError::InvalidMetadata { field: "target" });
    }
    let identity = if fields[7] == "unprovisioned" {
        DeviceIdentity::Unprovisioned
    } else {
        DeviceIdentity::Provisioned(
            fields[7]
                .parse::<DeviceId>()
                .map_err(|_| ParseError::InvalidIdentity)?,
        )
    };
    let generation = uint(fields[8], "generation")?;
    let assembly_revision = uint(fields[9], "assembly")?;
    let known_mask: u8 = uint(fields[10], "known")?;
    let negative_mask: u8 = uint(fields[11], "negative")?;
    let tare_epoch = uint(fields[12], "tare_epoch")?;
    let tare_valid = bool01(fields[13], "tare_valid")?;
    if known_mask & !0x7f != 0 || negative_mask & !0x7f != 0 || negative_mask & !known_mask != 0 {
        return Err(ParseError::InvalidMask);
    }
    if generation == 0 {
        if assembly_revision != 0 || known_mask != 0 || negative_mask != 0 {
            return Err(ParseError::InvalidMask);
        }
    } else if assembly_revision == 0 {
        return Err(ParseError::InvalidMask);
    }
    Ok(Frame::Info {
        request,
        info: DeviceInfo {
            firmware_version,
            source_revision: source_revision.to_owned(),
            board,
            mcu,
            clock_hz,
            identity,
            generation,
            assembly_revision,
            known_mask,
            negative_mask,
            tare_epoch,
            tare_valid,
        },
    })
}

fn optional_count(
    value: &str,
    min: i32,
    max: i32,
    field: &'static str,
    channel: usize,
) -> Result<Option<i32>, ParseError> {
    if value == "?" {
        return Ok(None);
    }
    let parsed = value
        .parse::<i32>()
        .map_err(|_| ParseError::InvalidCount { field, channel })?;
    if !(min..=max).contains(&parsed) {
        return Err(ParseError::InvalidCount { field, channel });
    }
    Ok(Some(parsed))
}

fn parse_sample(fields: &[&str]) -> Result<Frame, ParseError> {
    const COUNT: usize = 7 + CHANNEL_COUNT * 3;
    if fields.len() != COUNT {
        return Err(ParseError::FieldCount {
            frame: "sample3",
            expected: COUNT,
            actual: fields.len(),
        });
    }
    let session = uint(fields[1], "session")?;
    let sequence = uint(fields[2], "sequence")?;
    if session == 0 || sequence == 0 {
        return Err(ParseError::InvalidRequest);
    }
    let device_ms = uint(fields[3], "device_ms")?;
    let tare_epoch = uint(fields[4], "tare_epoch")?;
    let generation = uint(fields[5], "generation")?;
    let tare_valid = bool01(fields[6], "tare_valid")?;
    let mut raw = [None; CHANNEL_COUNT];
    let mut oriented = [None; CHANNEL_COUNT];
    let mut flags = [0; CHANNEL_COUNT];
    for channel in 0..CHANNEL_COUNT {
        raw[channel] = optional_count(fields[7 + channel], ADC_MIN, ADC_MAX, "raw", channel)?;
        oriented[channel] = optional_count(
            fields[7 + CHANNEL_COUNT + channel],
            ORIENTED_MIN,
            ORIENTED_MAX,
            "oriented",
            channel,
        )?;
        let parsed = uint::<u16>(fields[7 + 2 * CHANNEL_COUNT + channel], "flags")?;
        if parsed > u16::from(VALID_FLAGS) {
            return Err(ParseError::InvalidFlags { channel });
        }
        flags[channel] = parsed as u8;
        if oriented[channel].is_some() && raw[channel].is_none() {
            return Err(ParseError::InvalidCount {
                field: "oriented_without_raw",
                channel,
            });
        }
        if flags[channel] & FLAG_FRESH != 0 && flags[channel] & (FLAG_TIMEOUT | FLAG_HELD) != 0 {
            return Err(ParseError::InvalidFlags { channel });
        }
    }
    Ok(Frame::Sample(Sample {
        session,
        sequence,
        device_ms,
        tare_epoch,
        generation,
        tare_valid,
        raw,
        oriented,
        flags,
    }))
}

fn parse_ok(fields: &[&str]) -> Result<Frame, ParseError> {
    if fields.len() != 4 {
        return Err(ParseError::FieldCount {
            frame: "ok3",
            expected: 4,
            actual: fields.len(),
        });
    }
    let request = uint(fields[1], "request")?;
    if request == 0 {
        return Err(ParseError::InvalidRequest);
    }
    let operation = token(fields[2], "operation", 16)?;
    if !matches!(operation.as_str(), "stream" | "settings" | "tare") {
        return Err(ParseError::InvalidMetadata { field: "operation" });
    }
    let value = uint(fields[3], "value")?;
    Ok(Frame::Ok {
        request,
        operation,
        value,
    })
}

fn parse_error(fields: &[&str]) -> Result<Frame, ParseError> {
    if fields.len() != 3 {
        return Err(ParseError::FieldCount {
            frame: "error3",
            expected: 3,
            actual: fields.len(),
        });
    }
    let request = uint(fields[1], "request")?;
    if request == 0 {
        return Err(ParseError::InvalidRequest);
    }
    let token = token(fields[2], "error", 64).map_err(|_| ParseError::EmptyToken)?;
    Ok(Frame::Error { request, token })
}

pub fn info_command(request: u32) -> Result<String, ParseError> {
    command(format!("info3,{request}\n"), request)
}
pub fn stream_command(request: u32, session: u32, enabled: bool) -> Result<String, ParseError> {
    if session == 0 {
        return Err(ParseError::InvalidRequest);
    }
    command(
        format!("stream3,{request},{session},{}\n", u8::from(enabled)),
        request,
    )
}
pub fn settings_command(
    request: u32,
    id: DeviceId,
    expected_generation: u32,
    assembly: u32,
    known: u8,
    negative: u8,
) -> Result<String, ParseError> {
    if assembly == 0 || known & !0x7f != 0 || known != 0x7f || negative & !known != 0 {
        return Err(ParseError::InvalidMask);
    }
    command(
        format!("settings3,{request},{id},{expected_generation},{assembly},{known},{negative}\n"),
        request,
    )
}
pub fn tare_command(
    request: u32,
    id: DeviceId,
    expected_generation: u32,
) -> Result<String, ParseError> {
    command(
        format!("tare3,{request},{id},{expected_generation}\n"),
        request,
    )
}
fn command(line: String, request: u32) -> Result<String, ParseError> {
    if request == 0 {
        return Err(ParseError::InvalidRequest);
    }
    if line.len() > MAX_COMMAND_BYTES {
        return Err(ParseError::TooLong);
    }
    Ok(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";
    const ID: &str = "srn-0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_info3_with_complete_identity_and_settings() {
        let line = format!(
            "info3,9,1.2.3,{SHA},sparkfun_promicro16,atmega32u4,16000000,{ID},2,3,127,8,4,1"
        );
        let Frame::Info { request, info } = parse_line(&line).unwrap() else {
            panic!("info")
        };
        assert_eq!(request, 9);
        assert_eq!(
            info.identity,
            DeviceIdentity::Provisioned(ID.parse().unwrap())
        );
        assert_eq!(info.known_mask, 127);
        assert_eq!(info.negative_mask, 8);
    }

    #[test]
    fn parses_bounded_quality_sample_and_preserves_missing_counts() {
        let line = "sample3,7,1,90,3,2,1,1,2,3,4,5,6,7,11,12,13,14,15,16,17,1,1,1,1,1,1,1";
        let Frame::Sample(sample) = parse_line(line).unwrap() else {
            panic!("sample")
        };
        assert_eq!(sample.raw[0], Some(1));
        assert_eq!(sample.oriented[0], Some(11));
        assert_eq!(sample.flags, [1; CHANNEL_COUNT]);
        let missing = line.replace(",11,", ",?,");
        let Frame::Sample(sample) = parse_line(&missing).unwrap() else {
            panic!("sample")
        };
        assert_eq!(sample.oriented[0], None);
    }

    #[test]
    fn rejects_malformed_masks_flags_ranges_and_lengths() {
        let mut info =
            format!("info3,1,1,{SHA},sparkfun_promicro16,atmega32u4,16000000,{ID},2,1,1,2,1,1");
        assert_eq!(parse_line(&info), Err(ParseError::InvalidMask));
        info = format!(
            "info3,1,1,{SHA},sparkfun_promicro16,atmega32u4,16000000,{ID},2,1,127,0,1,1,extra"
        );
        assert!(matches!(
            parse_line(&info),
            Err(ParseError::FieldCount { .. })
        ));
        assert!(matches!(
            parse_line(
                "sample3,1,1,0,0,1,1,8388608,2,3,4,5,6,7,11,12,13,14,15,16,17,1,1,1,1,1,1,1"
            ),
            Err(ParseError::InvalidCount { field: "raw", .. })
        ));
        assert!(matches!(
            parse_line("sample3,1,1,0,0,1,1,1,2,3,4,5,6,7,11,12,13,14,15,16,17,3,1,1,1,1,1,1"),
            Err(ParseError::InvalidFlags { channel: 0 })
        ));
    }

    #[test]
    fn retains_firmware_fresh_saturation_and_rejection_as_unusable_diagnostics() {
        for flags in [
            FLAG_FRESH | FLAG_SATURATED,
            FLAG_FRESH | FLAG_REJECTED,
            FLAG_FRESH | FLAG_SATURATED | FLAG_REJECTED,
        ] {
            let wire = format!(
                "sample3,7,1,90,3,2,1,1,2,3,4,5,6,7,?,12,13,14,15,16,17,{flags},1,1,1,1,1,1"
            );
            let Frame::Sample(sample) = parse_line(&wire).unwrap() else {
                panic!("sample")
            };
            assert_eq!(sample.raw[0], Some(1));
            assert_eq!(sample.oriented[0], None);
            assert_eq!(sample.flags[0], flags);
            assert_ne!(sample.flags[0], FLAG_FRESH);
        }
    }

    #[test]
    fn encodes_bounded_commands_with_nonzero_requests() {
        let id: DeviceId = ID.parse().unwrap();
        assert_eq!(info_command(4).unwrap(), "info3,4\n");
        assert_eq!(stream_command(4, 5, true).unwrap(), "stream3,4,5,1\n");
        assert_eq!(
            settings_command(1, id, 3, 9, 127, 4).unwrap(),
            format!("settings3,1,{ID},3,9,127,4\n")
        );
        assert!(info_command(0).is_err());
        assert!(settings_command(1, id, 3, 9, 15, 0).is_err());
    }
}
