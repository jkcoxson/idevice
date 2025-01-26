// DebianArch

use std::{array::TryFromSliceError, error::Error, io, num::TryFromIntError};

use tokio::sync::mpsc::error::SendError;

#[derive(Debug)]
pub enum Http2Error {
    Io(io::Error),
    SendError,
    TryFromIntError(TryFromIntError),
    TryFromSliceError(TryFromSliceError),
    Custom(String),
}

impl From<io::Error> for Http2Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl<T> From<SendError<T>> for Http2Error {
    fn from(_: SendError<T>) -> Self {
        Self::SendError
    }
}

impl From<&str> for Http2Error {
    fn from(value: &str) -> Self {
        Self::Custom(value.to_string())
    }
}

impl From<TryFromIntError> for Http2Error {
    fn from(value: TryFromIntError) -> Self {
        Self::TryFromIntError(value)
    }
}

impl From<TryFromSliceError> for Http2Error {
    fn from(value: TryFromSliceError) -> Self {
        Self::TryFromSliceError(value)
    }
}

impl std::fmt::Display for Http2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Http2Error({})",
            match self {
                Self::Io(io) => io.to_string(),
                Self::SendError => "SendError".to_string(),
                Self::TryFromIntError(e) => e.to_string(),
                Self::TryFromSliceError(e) => e.to_string(),
                Self::Custom(s) => s.clone(),
            }
        )
    }
}

impl Error for Http2Error {}
