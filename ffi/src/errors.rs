// Jackson Coxson

use idevice::IdeviceError;

#[repr(C)]
pub enum IdeviceErrorCode {
    IdeviceSuccess = 0,
    // Main library
    Socket = -1,
    Ssl = -2,
    SslSetup = -3,
    Plist = -4,
    Utf8 = -5,
    UnexpectedResponse = -6,
    GetProhibited = -7,
    SessionInactive = -8,
    InvalidHostID = -9,
    NoEstablishedConnection = -10,
    HeartbeatSleepyTime = -11,
    HeartbeatTimeout = -12,
    NotFound = -13,
    CdtunnelPacketTooShort = -14,
    CdtunnelPacketInvalidMagic = -15,
    PacketSizeMismatch = -16,
    Json = -17,
    DeviceNotFound = -18,
    DeviceLocked = -19,
    UsbConnectionRefused = -20,
    UsbBadCommand = -21,
    UsbBadDevice = -22,
    UsbBadVersion = -23,
    BadBuildManifest = -24,
    ImageNotMounted = -25,
    Reqwest = -26,
    InternalError = -27,
    Xpc = -28,
    NsKeyedArchiveError = -29,
    UnknownAuxValueType = -30,
    UnknownChannel = -31,
    AddrParseError = -32,
    DisableMemoryLimitFailed = -33,
    NotEnoughBytes = -34,
    Utf8Error = -35,
    InvalidArgument = -36,
    UnknownErrorType = -37,
    // FFI specific bindings
    AdapterIOFailed = -996,
    ServiceNotFound = -997,
    BufferTooSmall = -998,
    InvalidString = -999,
    InvalidArg = -1000,
}

impl From<IdeviceError> for IdeviceErrorCode {
    fn from(err: IdeviceError) -> Self {
        match err {
            IdeviceError::Socket(_) => IdeviceErrorCode::Socket,
            IdeviceError::Ssl(_) => IdeviceErrorCode::Ssl,
            IdeviceError::SslSetup(_) => IdeviceErrorCode::SslSetup,
            IdeviceError::Plist(_) => IdeviceErrorCode::Plist,
            IdeviceError::Utf8(_) => IdeviceErrorCode::Utf8,
            IdeviceError::UnexpectedResponse => IdeviceErrorCode::UnexpectedResponse,
            IdeviceError::GetProhibited => IdeviceErrorCode::GetProhibited,
            IdeviceError::SessionInactive => IdeviceErrorCode::SessionInactive,
            IdeviceError::InvalidHostID => IdeviceErrorCode::InvalidHostID,
            IdeviceError::NoEstablishedConnection => IdeviceErrorCode::NoEstablishedConnection,
            IdeviceError::HeartbeatSleepyTime => IdeviceErrorCode::HeartbeatSleepyTime,
            IdeviceError::HeartbeatTimeout => IdeviceErrorCode::HeartbeatTimeout,
            IdeviceError::NotFound => IdeviceErrorCode::NotFound,
            IdeviceError::CdtunnelPacketTooShort => IdeviceErrorCode::CdtunnelPacketTooShort,
            IdeviceError::CdtunnelPacketInvalidMagic => {
                IdeviceErrorCode::CdtunnelPacketInvalidMagic
            }
            IdeviceError::PacketSizeMismatch => IdeviceErrorCode::PacketSizeMismatch,
            IdeviceError::Json(_) => IdeviceErrorCode::Json,
            IdeviceError::DeviceNotFound => IdeviceErrorCode::DeviceNotFound,
            IdeviceError::DeviceLocked => IdeviceErrorCode::DeviceLocked,
            IdeviceError::UsbConnectionRefused => IdeviceErrorCode::UsbConnectionRefused,
            IdeviceError::UsbBadCommand => IdeviceErrorCode::UsbBadCommand,
            IdeviceError::UsbBadDevice => IdeviceErrorCode::UsbBadDevice,
            IdeviceError::UsbBadVersion => IdeviceErrorCode::UsbBadVersion,
            IdeviceError::BadBuildManifest => IdeviceErrorCode::BadBuildManifest,
            IdeviceError::ImageNotMounted => IdeviceErrorCode::ImageNotMounted,
            IdeviceError::Reqwest(_) => IdeviceErrorCode::Reqwest,
            IdeviceError::InternalError(_) => IdeviceErrorCode::InternalError,
            IdeviceError::Xpc(_) => IdeviceErrorCode::Xpc,
            IdeviceError::NsKeyedArchiveError(_) => IdeviceErrorCode::NsKeyedArchiveError,
            IdeviceError::UnknownAuxValueType(_) => IdeviceErrorCode::UnknownAuxValueType,
            IdeviceError::UnknownChannel(_) => IdeviceErrorCode::UnknownChannel,
            IdeviceError::AddrParseError(_) => IdeviceErrorCode::AddrParseError,
            IdeviceError::DisableMemoryLimitFailed => IdeviceErrorCode::DisableMemoryLimitFailed,
            IdeviceError::NotEnoughBytes(_, _) => IdeviceErrorCode::NotEnoughBytes,
            IdeviceError::Utf8Error => IdeviceErrorCode::Utf8Error,
            IdeviceError::InvalidArgument => IdeviceErrorCode::InvalidArgument,
            IdeviceError::UnknownErrorType(_) => IdeviceErrorCode::UnknownErrorType,
            _ => IdeviceErrorCode::InternalError,
        }
    }
}
