// Jackson Coxson

/// Errors specific to the XPC/HTTP2 protocol layer
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum XpcError {
    #[error("unknown http frame type: {0}")]
    UnknownFrame(u8),
    #[error("unknown http setting type: {0}")]
    UnknownHttpSetting(u16),
    #[error("uninitialized stream ID")]
    UninitializedStreamId,
    #[error("unknown XPC type: {0}")]
    UnknownXpcType(u32),
    #[error("malformed XPC message")]
    MalformedXpc,
    #[error("invalid XPC magic")]
    InvalidXpcMagic,
    #[error("unexpected XPC version")]
    UnexpectedXpcVersion,
    #[error("invalid C string")]
    InvalidCString,
    /// The peer sent RST_STREAM for `stream_id`. Every later read of that
    /// stream yields this error again.
    #[error("HTTP/2 stream {stream_id} reset by peer (error code {error_code})")]
    Reset { stream_id: u32, error_code: u32 },
    /// The peer sent GOAWAY; the connection accepts no further streams.
    #[error("HTTP/2 GOAWAY (last stream {last_stream_id}, error code {error_code}): {debug}")]
    GoAway {
        last_stream_id: u32,
        error_code: u32,
        debug: String,
    },
    /// The peer ended `stream_id` (END_STREAM on DATA or HEADERS) and every
    /// payload it sent on that stream has already been returned. Other streams
    /// stay readable; stop reading this one.
    #[error("HTTP/2 stream {stream_id} ended by peer")]
    StreamEnded { stream_id: u32 },
    /// The connection or stream ended while `buffered` bytes of an incomplete
    /// HTTP/2 frame (`stream_id: None`) or XPC wrapper (`Some(channel)`) were
    /// pending.
    #[error("connection ended mid-message ({buffered} bytes pending on {stream_id:?})")]
    Truncated {
        stream_id: Option<u32>,
        buffered: usize,
    },
    /// TCP EOF with no partial frame or wrapper pending.
    #[error("HTTP/2 connection closed by peer")]
    ConnectionClosed,
    /// Strict-streams mode only: DATA arrived for a stream this client never
    /// opened (typically a file-transfer side channel).
    #[error("DATA on unopened HTTP/2 stream {stream_id}")]
    UnexpectedStream { stream_id: u32 },
    /// A wrapper's declared length (header plus body) exceeds the configured
    /// maximum message size; rejected before the body is buffered or allocated.
    #[error("XPC message declares {declared} bytes, limit is {max}")]
    MessageTooLarge { declared: u64, max: usize },
    /// A nested length or fixed-size field runs past the bytes that remain in
    /// the enclosing message.
    #[error("XPC field declares {declared} bytes but only {remaining} remain")]
    LengthOutOfBounds { declared: u64, remaining: usize },
    /// Dictionaries, arrays and file transfers are nested deeper than `max`.
    #[error("XPC nesting deeper than {max}")]
    NestingTooDeep { max: usize },
    /// A `Date` before the Unix epoch, or more than `u64::MAX` nanoseconds after
    /// it, cannot be encoded.
    #[error("XPC date outside 1970-01-01 ..= 1970 + u64::MAX ns")]
    DateOutOfRange,
}

impl XpcError {
    pub fn sub_code(&self) -> i32 {
        match self {
            Self::UnknownFrame(_) => 1,
            Self::UnknownHttpSetting(_) => 2,
            Self::UninitializedStreamId => 3,
            Self::UnknownXpcType(_) => 4,
            Self::MalformedXpc => 5,
            Self::InvalidXpcMagic => 6,
            Self::UnexpectedXpcVersion => 7,
            Self::InvalidCString => 8,
            Self::Reset { .. } => 9,
            Self::GoAway { .. } => 10,
            Self::StreamEnded { .. } => 11,
            Self::Truncated { .. } => 12,
            Self::ConnectionClosed => 13,
            Self::UnexpectedStream { .. } => 14,
            Self::MessageTooLarge { .. } => 15,
            Self::LengthOutOfBounds { .. } => 16,
            Self::NestingTooDeep { .. } => 17,
            Self::DateOutOfRange => 18,
        }
    }
}
