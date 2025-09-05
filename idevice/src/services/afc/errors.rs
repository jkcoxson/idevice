// Jackson Coxson

#[derive(thiserror::Error, Debug, PartialEq)]
#[non_exhaustive]
#[repr(C)]
pub enum AfcError {
    Success = 0,
    UnknownError = 1,
    OpHeaderInvalid = 2,
    NoResources = 3,
    ReadError = 4,
    WriteError = 5,
    UnknownPacketType = 6,
    InvalidArg = 7,
    ObjectNotFound = 8,
    ObjectIsDir = 9,
    PermDenied = 10,
    ServiceNotConnected = 11,
    OpTimeout = 12,
    TooMuchData = 13,
    EndOfData = 14,
    OpNotSupported = 15,
    ObjectExists = 16,
    ObjectBusy = 17,
    NoSpaceLeft = 18,
    OpWouldBlock = 19,
    IoError = 20,
    OpInterrupted = 21,
    OpInProgress = 22,
    InternalError = 23,
    MuxError = 30,
    NoMem = 31,
    NotEnoughData = 32,
    DirNotEmpty = 33,
}

impl std::fmt::Display for AfcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let description = match self {
            AfcError::Success => "Success",
            AfcError::UnknownError => "Unknown error",
            AfcError::OpHeaderInvalid => "Operation header invalid",
            AfcError::NoResources => "No resources available",
            AfcError::ReadError => "Read error",
            AfcError::WriteError => "Write error",
            AfcError::UnknownPacketType => "Unknown packet type",
            AfcError::InvalidArg => "Invalid argument",
            AfcError::ObjectNotFound => "Object not found",
            AfcError::ObjectIsDir => "Object is a directory",
            AfcError::PermDenied => "Permission denied",
            AfcError::ServiceNotConnected => "Service not connected",
            AfcError::OpTimeout => "Operation timed out",
            AfcError::TooMuchData => "Too much data",
            AfcError::EndOfData => "End of data",
            AfcError::OpNotSupported => "Operation not supported",
            AfcError::ObjectExists => "Object already exists",
            AfcError::ObjectBusy => "Object is busy",
            AfcError::NoSpaceLeft => "No space left",
            AfcError::OpWouldBlock => "Operation would block",
            AfcError::IoError => "I/O error",
            AfcError::OpInterrupted => "Operation interrupted",
            AfcError::OpInProgress => "Operation in progress",
            AfcError::InternalError => "Internal error",
            AfcError::MuxError => "Multiplexer error",
            AfcError::NoMem => "Out of memory",
            AfcError::NotEnoughData => "Not enough data",
            AfcError::DirNotEmpty => "Directory not empty",
        };
        write!(f, "{description}")
    }
}

impl From<u64> for AfcError {
    fn from(value: u64) -> Self {
        match value {
            0 => Self::Success,
            1 => Self::UnknownError,
            2 => Self::OpHeaderInvalid,
            3 => Self::NoResources,
            4 => Self::ReadError,
            5 => Self::WriteError,
            6 => Self::UnknownPacketType,
            7 => Self::InvalidArg,
            8 => Self::ObjectNotFound,
            9 => Self::ObjectIsDir,
            10 => Self::PermDenied,
            11 => Self::ServiceNotConnected,
            12 => Self::OpTimeout,
            13 => Self::TooMuchData,
            14 => Self::EndOfData,
            15 => Self::OpNotSupported,
            16 => Self::ObjectExists,
            17 => Self::ObjectBusy,
            18 => Self::NoSpaceLeft,
            19 => Self::OpWouldBlock,
            20 => Self::IoError,
            21 => Self::OpInterrupted,
            22 => Self::OpInProgress,
            23 => Self::InternalError,
            30 => Self::MuxError,
            31 => Self::NoMem,
            32 => Self::NotEnoughData,
            33 => Self::DirNotEmpty,
            _ => Self::UnknownError, // fallback for unknown codes
        }
    }
}
