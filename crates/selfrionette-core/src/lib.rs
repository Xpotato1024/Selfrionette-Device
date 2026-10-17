pub mod calibration;
pub mod identity;
pub mod input_calibration;
pub mod legacy;
pub mod persistence;
pub mod protocol;
pub mod protocol_v3;
pub mod setup;
pub mod setup_persistence;

pub use identity::{DeviceId, DeviceIdParseError};
pub use protocol::{
    CHANNEL_COUNT, CompatibilityError, DeviceIdentity, DeviceInfo, DiagnosticFrame, HostCommand,
    PROTOCOL_MAJOR_VERSION, ProtocolFrame, ProtocolParseError, UnknownFrame, VectorFrame,
    parse_line,
};
