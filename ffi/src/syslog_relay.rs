use std::os::raw::c_char;

use idevice::{syslog_relay::SyslogRelayClient, IdeviceError, IdeviceService};

use crate::{
    provider::TcpProviderHandle, IdeviceErrorCode, RUNTIME
};

pub struct SyslogRelayClientHandle(pub SyslogRelayClient);

/// Automatically creates and connects to syslog relay, returning a client handle
///
/// # Arguments
/// * [`provider`] - A TcpProvider
/// * [`client`] - On success, will be set to point to a newly allocated SyslogRelayClient handle
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub extern "C" fn syslog_relay_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut SyslogRelayClientHandle
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<SyslogRelayClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_box = unsafe { Box::from_raw(provider) };
        
        let provider_ref = &provider_box.0;

        let result = SyslogRelayClient::connect(provider_ref).await;

        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(c) => {
            let boxed = Box::new(SyslogRelayClientHandle(c));

            unsafe { *client = Box::into_raw(boxed) };

            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            let _ = unsafe { Box::from_raw(provider) };
            e.into()
        }
    }
}

/// Frees a handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub extern "C" fn syslog_relay_client_free(
    handle: *mut SyslogRelayClientHandle
) {
    if !handle.is_null() {
        log::debug!("Freeing syslog relay client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Gets the next log message from the relay
///
/// # Arguments
/// * [`client`] - The SyslogRelayClient handle
/// * [`log_message`] - On success a newly allocated cstring will be set to point to the log message
/// 
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `log_message` must be a valid, non-null pointer to a location where the log message will be stored
#[unsafe(no_mangle)]
pub extern "C" fn syslog_relay_next(
    client: *mut SyslogRelayClientHandle,
    log_message: *mut *mut c_char,
) -> IdeviceErrorCode {
    if client.is_null() || log_message.is_null() {
        return IdeviceErrorCode::InvalidArg;
    }

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .next()
            .await
    });

    match res {
        Ok(log) => {
            use std::ffi::CString;
            
            // null bytes are a curse in C, so just use spaces
            let safe_log = log.replace('\0', " ");
            
            match CString::new(safe_log) {
                Ok(c_string) => {
                    unsafe { *log_message = c_string.into_raw() };
                    IdeviceErrorCode::IdeviceSuccess
                }
                Err(_) => {
                    log::error!("Failed to convert log message to C string");
                    IdeviceErrorCode::InvalidString
                }
            }
        }
        Err(e) => e.into(),
    }
}