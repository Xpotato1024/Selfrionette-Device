use core::fmt;
use core::str::FromStr;

pub const DEVICE_ID_PREFIX: &str = "srn-";
pub const DEVICE_ID_BYTES: usize = 16;
pub const DEVICE_ID_TEXT_LEN: usize = DEVICE_ID_PREFIX.len() + DEVICE_ID_BYTES * 2;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId([u8; DEVICE_ID_BYTES]);

impl DeviceId {
    pub const fn from_bytes(bytes: [u8; DEVICE_ID_BYTES]) -> Result<Self, DeviceIdParseError> {
        let mut index = 0;
        let mut any_nonzero = false;
        while index < DEVICE_ID_BYTES {
            if bytes[index] != 0 {
                any_nonzero = true;
                break;
            }
            index += 1;
        }

        if !any_nonzero {
            return Err(DeviceIdParseError::AllZero);
        }

        Ok(Self(bytes))
    }

    pub const fn as_bytes(&self) -> &[u8; DEVICE_ID_BYTES] {
        &self.0
    }
}

impl FromStr for DeviceId {
    type Err = DeviceIdParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != DEVICE_ID_TEXT_LEN {
            return Err(DeviceIdParseError::WrongLength {
                expected: DEVICE_ID_TEXT_LEN,
                actual: value.len(),
            });
        }

        if !value.starts_with(DEVICE_ID_PREFIX) {
            return Err(DeviceIdParseError::WrongPrefix);
        }

        let hex = value.as_bytes();
        let mut bytes = [0_u8; DEVICE_ID_BYTES];
        let mut index = 0;
        while index < DEVICE_ID_BYTES {
            let high_index = DEVICE_ID_PREFIX.len() + index * 2;
            let low_index = high_index + 1;
            let high = parse_lower_hex(hex[high_index]).ok_or(DeviceIdParseError::InvalidHex {
                index: high_index,
                value: hex[high_index] as char,
            })?;
            let low = parse_lower_hex(hex[low_index]).ok_or(DeviceIdParseError::InvalidHex {
                index: low_index,
                value: hex[low_index] as char,
            })?;
            bytes[index] = (high << 4) | low;
            index += 1;
        }

        Self::from_bytes(bytes)
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(DEVICE_ID_PREFIX)?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

fn parse_lower_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceIdParseError {
    WrongLength { expected: usize, actual: usize },
    WrongPrefix,
    InvalidHex { index: usize, value: char },
    AllZero,
}

impl fmt::Display for DeviceIdParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongLength { expected, actual } => {
                write!(
                    formatter,
                    "device id length must be {expected} bytes, got {actual}"
                )
            }
            Self::WrongPrefix => formatter.write_str("device id must start with srn-"),
            Self::InvalidHex { index, value } => {
                write!(
                    formatter,
                    "invalid lowercase hex at byte {index}: {value:?}"
                )
            }
            Self::AllZero => formatter.write_str("all-zero device id is reserved"),
        }
    }
}

impl std::error::Error for DeviceIdParseError {}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = "srn-0123456789abcdef0123456789abcdef";

    #[test]
    fn parses_and_formats_canonical_device_id() {
        let id: DeviceId = VALID.parse().expect("valid device id");
        assert_eq!(id.to_string(), VALID);
        assert_eq!(
            id.as_bytes(),
            &[
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef,
            ]
        );
    }

    #[test]
    fn rejects_uppercase_hex() {
        let error = "srn-0123456789ABCDEF0123456789abcdef"
            .parse::<DeviceId>()
            .expect_err("uppercase must fail");
        assert!(matches!(error, DeviceIdParseError::InvalidHex { .. }));
    }

    #[test]
    fn rejects_all_zero() {
        let error = "srn-00000000000000000000000000000000"
            .parse::<DeviceId>()
            .expect_err("all-zero must fail");
        assert_eq!(error, DeviceIdParseError::AllZero);
    }

    #[test]
    fn rejects_wrong_prefix() {
        let error = "dev-0123456789abcdef0123456789abcdef"
            .parse::<DeviceId>()
            .expect_err("wrong prefix must fail");
        assert_eq!(error, DeviceIdParseError::WrongPrefix);
    }
}
