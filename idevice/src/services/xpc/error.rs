// DebianArch

use super::http2::error::Http2Error;
use std::{
    array::TryFromSliceError, error::Error, ffi::FromVecWithNulError, io, num::TryFromIntError,
    str::Utf8Error,
};

#[derive(Debug)]
pub enum XPCError {
    Io(io::Error),
    Http2Error(Http2Error),
    ParseError(ParseError),
    Custom(String),
}

#[derive(Debug)]
pub enum ParseError {
    TryFromSliceError(TryFromSliceError),
    TryFromIntError(TryFromIntError),
    FromVecWithNulError(FromVecWithNulError),
    Utf8Error(Utf8Error),
}

impl From<TryFromSliceError> for XPCError {
    fn from(value: TryFromSliceError) -> Self {
        Self::ParseError(ParseError::TryFromSliceError(value))
    }
}

impl From<TryFromIntError> for XPCError {
    fn from(value: TryFromIntError) -> Self {
        Self::ParseError(ParseError::TryFromIntError(value))
    }
}

impl From<ParseError> for XPCError {
    fn from(value: ParseError) -> Self {
        Self::ParseError(value)
    }
}

impl From<FromVecWithNulError> for XPCError {
    fn from(value: FromVecWithNulError) -> Self {
        Self::ParseError(ParseError::FromVecWithNulError(value))
    }
}

impl From<Utf8Error> for XPCError {
    fn from(value: Utf8Error) -> Self {
        Self::ParseError(ParseError::Utf8Error(value))
    }
}

impl From<io::Error> for XPCError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<&str> for XPCError {
    fn from(value: &str) -> Self {
        Self::Custom(value.to_string())
    }
}

impl From<Http2Error> for XPCError {
    fn from(value: Http2Error) -> Self {
        Self::Http2Error(value)
    }
}

impl std::fmt::Display for XPCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "XPCError({})",
            match self {
                Self::Io(io) => io.to_string(),
                Self::Http2Error(http2) => http2.to_string(),
                Self::ParseError(e) => e.to_string(),
                Self::Custom(s) => s.clone(),
            }
        )
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ParseError({})",
            match self {
                Self::TryFromSliceError(e) => e.to_string(),
                Self::TryFromIntError(e) => e.to_string(),
                Self::FromVecWithNulError(e) => e.to_string(),
                Self::Utf8Error(e) => e.to_string(),
            }
        )
    }
}

impl Error for XPCError {}
