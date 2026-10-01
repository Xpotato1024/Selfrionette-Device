pub mod identity;
pub mod protocol;

pub use identity::{DeviceId, DeviceIdParseError};
pub use protocol::{
    CHANNEL_COUNT, CompatibilityError, DeviceIdentity, DeviceInfo, DiagnosticFrame, HostCommand,
    PROTOCOL_MAJOR_VERSION, ProtocolFrame, ProtocolParseError, UnknownFrame, VectorFrame,
    parse_line,
};
