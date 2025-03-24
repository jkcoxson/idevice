// Jackson Coxson

use std::{
    ffi::{CString, c_char},
    fs::File,
};

use log::LevelFilter;
use simplelog::{
    ColorChoice, CombinedLogger, Config, SharedLogger, TermLogger, TerminalMode, WriteLogger,
};

/// Initializes the logger
///
/// # Arguments
/// * [`console_level`] - The level to log to the file
/// * [`file_level`] - The level to log to the file
/// * [`file_path`] - If not null, the file to write logs to
///
/// ## Log Level
/// 0. Disabled
/// 1. Error
/// 2. Warn
/// 3. Info
/// 4. Debug
/// 5. Trace
///
/// # Returns
/// 0 for success, -1 if the file couldn't be created, -2 if a logger has been initialized, -3 for invalid path string
///
/// # Safety
/// Pass a valid CString for file_path. Pass valid log levels according to the enum
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_init_logger(
    console_level: IdeviceLogLevel,
    file_level: IdeviceLogLevel,
    file_path: *mut c_char,
) -> IdeviceLoggerError {
    let mut loggers: Vec<Box<dyn SharedLogger>> = Vec::new();
    let level: LevelFilter = console_level.into();
    loggers.push(TermLogger::new(
        level,
        Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Auto,
    ));

    if !file_path.is_null() {
        let file_path = match unsafe { CString::from_raw(file_path) }.to_str() {
            Ok(f) => f.to_string(),
            Err(_) => {
                return IdeviceLoggerError::InvalidPathString;
            }
        };
        let level: LevelFilter = file_level.into();
        loggers.push(WriteLogger::new(
            level,
            Config::default(),
            match File::create(file_path) {
                Ok(f) => f,
                Err(e) => {
                    println!("Failed to create path: {e:?}");
                    return IdeviceLoggerError::FileError;
                }
            },
        ));
    }

    if CombinedLogger::init(loggers).is_err() {
        IdeviceLoggerError::AlreadyInitialized
    } else {
        IdeviceLoggerError::Success
    }
}

#[repr(C)]
pub enum IdeviceLoggerError {
    Success = 0,
    FileError = -1,
    AlreadyInitialized = -2,
    InvalidPathString = -3,
}

#[repr(C)]
#[derive(PartialEq)]
pub enum IdeviceLogLevel {
    Disabled = 0,
    ErrorLevel = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
    Trace = 5,
}

impl TryFrom<u8> for IdeviceLogLevel {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Disabled,
            1 => Self::ErrorLevel,
            2 => Self::Warn,
            3 => Self::Info,
            4 => Self::Debug,
            5 => Self::Trace,
            _ => {
                return Err(());
            }
        })
    }
}

impl From<IdeviceLogLevel> for LevelFilter {
    fn from(value: IdeviceLogLevel) -> Self {
        match value {
            IdeviceLogLevel::Disabled => LevelFilter::Off,
            IdeviceLogLevel::ErrorLevel => LevelFilter::Error,
            IdeviceLogLevel::Warn => LevelFilter::Warn,
            IdeviceLogLevel::Info => LevelFilter::Info,
            IdeviceLogLevel::Debug => LevelFilter::Debug,
            IdeviceLogLevel::Trace => LevelFilter::Trace,
        }
    }
}
