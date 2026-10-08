// Jackson Coxson

/// Errors specific to the DVT (Developer Tools) protocol
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum DvtError {
    #[error("NSKeyedArchive error")]
    NsKeyedArchiveError(#[from] ns_keyed_archive::ConverterError),
    #[error("Unknown aux value type: {0}")]
    UnknownAuxValueType(u32),
    #[error("unknown channel: {0}")]
    UnknownChannel(u32),
    #[error("disable memory limit failed")]
    DisableMemoryLimitFailed,
    #[error("invalid XCTest runner environment: {0}")]
    InvalidXCTestRunnerEnvironment(String),
    /// A DTX message header did not start with magic `0x1F3D5B79`.
    #[error("bad DTX magic: {0:#010x}")]
    BadMagic(u32),
    /// A DTX message header declared a header length other than 32.
    #[error("bad DTX header length: {0}")]
    BadHeaderLength(u32),
    /// Fragments arrived out of order, with a zero count, or with a changing
    /// identifier or count. `identifier` is the message being reassembled.
    #[error(
        "bad DTX fragment sequence for message {identifier}: expected fragment {expected_id}/{expected_count}, got {got_id}/{got_count} of message {got_identifier}"
    )]
    FragmentSequence {
        identifier: u32,
        expected_id: u16,
        expected_count: u16,
        got_identifier: u32,
        got_id: u16,
        got_count: u16,
    },
    /// The reassembled DTX message would exceed the configured cap.
    #[error("DTX message of at least {size} bytes exceeds the {max}-byte cap")]
    MessageTooLarge { size: usize, max: usize },
    /// The payload header's lengths do not fit the received bytes:
    /// `aux_length > total_length`, or `16 + total_length > available`.
    #[error(
        "bad DTX payload lengths: aux {aux_length}, total {total_length}, {available} bytes after the 16-byte payload header"
    )]
    PayloadLength {
        aux_length: u32,
        total_length: u32,
        available: usize,
    },
    /// The message body is shorter than the 16-byte payload header.
    #[error("DTX message body of {0} bytes is shorter than the 16-byte payload header")]
    ShortPayloadHeader(usize),
    /// An aux field declared more bytes than remain in the aux buffer.
    #[error("truncated aux: field needs {needed} bytes, {available} remain")]
    AuxTruncated { needed: u64, available: u64 },
}

impl DvtError {
    pub fn sub_code(&self) -> i32 {
        match self {
            Self::NsKeyedArchiveError(_) => 1,
            Self::UnknownAuxValueType(_) => 2,
            Self::UnknownChannel(_) => 3,
            Self::DisableMemoryLimitFailed => 4,
            Self::InvalidXCTestRunnerEnvironment(_) => 5,
            Self::BadMagic(_) => 6,
            Self::BadHeaderLength(_) => 7,
            Self::FragmentSequence { .. } => 8,
            Self::MessageTooLarge { .. } => 9,
            Self::PayloadLength { .. } => 10,
            Self::ShortPayloadHeader(_) => 11,
            Self::AuxTruncated { .. } => 12,
        }
    }
}
