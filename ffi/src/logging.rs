// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    fs::File,
    sync::Once,
};

use tracing::Level;
use tracing_subscriber::{EnvFilter, Layer, fmt::writer::BoxMakeWriter};
use tracing_subscriber::{Registry, fmt, layer::SubscriberExt, util::SubscriberInitExt};

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

impl From<IdeviceLogLevel> for Level {
    fn from(value: IdeviceLogLevel) -> Self {
        match value {
            IdeviceLogLevel::Disabled => Level::ERROR, // won't matter, filter will disable
            IdeviceLogLevel::ErrorLevel => Level::ERROR,
            IdeviceLogLevel::Warn => Level::WARN,
            IdeviceLogLevel::Info => Level::INFO,
            IdeviceLogLevel::Debug => Level::DEBUG,
            IdeviceLogLevel::Trace => Level::TRACE,
        }
    }
}

impl IdeviceLogLevel {
    fn as_filter(&self) -> String {
        match self {
            IdeviceLogLevel::Disabled => "off",
            IdeviceLogLevel::ErrorLevel => "error",
            IdeviceLogLevel::Warn => "warn",
            IdeviceLogLevel::Info => "info",
            IdeviceLogLevel::Debug => "debug",
            IdeviceLogLevel::Trace => "trace",
        }
        .to_string()
    }
}

// ensures we only init once
static INIT: Once = Once::new();

/// Initializes the global logger
///
/// # Safety
/// Pass a valid file path string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_init_logger(
    console_level: IdeviceLogLevel,
    file_level: IdeviceLogLevel,
    file_path: *mut c_char,
) -> IdeviceLoggerError {
    let mut init_result = IdeviceLoggerError::Success;

    INIT.call_once(|| {
        let console_filter = console_level.as_filter();
        let file_filter = file_level.as_filter();

        let mut layers = Vec::new();

        // Console layer
        if console_level != IdeviceLogLevel::Disabled {
            let console_layer = fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(true)
                .with_target(false)
                .with_filter(EnvFilter::new(console_filter));
            layers.push(console_layer.boxed());
        }

        // File layer
        if !file_path.is_null() && file_level != IdeviceLogLevel::Disabled {
            let path = match unsafe { CStr::from_ptr(file_path).to_str() } {
                Ok(p) => p,
                Err(_) => {
                    init_result = IdeviceLoggerError::InvalidPathString;
                    return;
                }
            };

            let file = match File::create(path) {
                Ok(f) => f,
                Err(_) => {
                    init_result = IdeviceLoggerError::FileError;
                    return;
                }
            };

            let file_layer = fmt::layer()
                .with_writer(BoxMakeWriter::new(file))
                .with_ansi(false)
                .with_target(false)
                .with_filter(EnvFilter::new(file_filter));
            layers.push(file_layer.boxed());
        }

        // Compose and set as global subscriber
        let subscriber = Registry::default().with(layers);
        subscriber.init();
    });

    // If it was already initialized, Once won't run again
    if !INIT.is_completed() {
        IdeviceLoggerError::AlreadyInitialized
    } else {
        init_result
    }
}
