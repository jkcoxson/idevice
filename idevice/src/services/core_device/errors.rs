//! Errors specific to the CoreDevice services.

use thiserror::Error;

/// Failures specific to talking to the device's CoreDevice services.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum CoreDeviceError {
    /// The device returned an error envelope instead of the expected output
    /// (typically a populated `CoreDevice.error` in the response). The string is
    /// the device's own error detail.
    #[error("device returned an error: {0}")]
    DeviceError(String),

    /// A field the response was required to contain was absent.
    #[error("device response missing required field `{0}`")]
    MissingField(&'static str),

    /// A field was present but had a type or shape we couldn't interpret.
    #[error("device response field `{0}` had an unexpected type or shape")]
    MalformedField(&'static str),

    /// An AVConference media negotiation blob (offer/answer) failed to encode or
    /// decode.
    #[error("media negotiation blob error: {0}")]
    Negotiation(String),

    /// A touchscreen report contained more contacts than the device supports.
    #[error("touchscreen report has {0} contacts; at most 5 are supported")]
    TooManyTouchscreenContacts(usize),

    /// A touchscreen contact identity was outside the supported range.
    #[error("touchscreen contact identity {0} is outside 0..5")]
    InvalidTouchscreenContactIdentity(u8),

    /// A touchscreen report used the same contact identity more than once.
    #[error("touchscreen contact identity {0} is duplicated")]
    DuplicateTouchscreenContactIdentity(u8),

    /// A multi-touch tap was requested without any contact positions.
    #[error("multi-touch tap requires at least one contact")]
    NoTouchscreenContacts,
}

impl CoreDeviceError {
    pub fn sub_code(&self) -> i32 {
        match self {
            Self::DeviceError(_) => 1,
            Self::MissingField(_) => 2,
            Self::MalformedField(_) => 3,
            Self::Negotiation(_) => 4,
            Self::TooManyTouchscreenContacts(_) => 5,
            Self::InvalidTouchscreenContactIdentity(_) => 6,
            Self::DuplicateTouchscreenContactIdentity(_) => 7,
            Self::NoTouchscreenContacts => 8,
        }
    }
}
