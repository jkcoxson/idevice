use std::os::raw::c_char;
use std::ffi::CString;

use idevice::{os_trace_relay::OsTraceRelayClient, IdeviceError, IdeviceService};

use crate::{
    provider::TcpProviderHandle, IdeviceErrorCode, RUNTIME
};

pub struct OsTraceRelayClientHandle(pub OsTraceRelayClient);
pub struct OsTraceRelayReceiverHandle(pub idevice::os_trace_relay::OsTraceRelayReceiver);

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OsTraceLog {
    pub pid: u32,
    pub timestamp: i64,
    pub level: u8,
    pub image_name: *const c_char,
    pub filename: *const c_char,
    pub message: *const c_char,
    pub label: *const SyslogLabel,
}

#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyslogLabel {
    pub subsystem: *const c_char,
    pub category: *const c_char,
}

#[unsafe(no_mangle)]
pub extern "C" fn os_trace_relay_connect_tcp(
    provider: *mut TcpProviderHandle,
    client: *mut *mut OsTraceRelayClientHandle
) -> IdeviceErrorCode {
    if provider.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res: Result<OsTraceRelayClient, IdeviceError> = RUNTIME.block_on(async move {
        let provider_box = unsafe { Box::from_raw(provider) };
        
        let provider_ref = &provider_box.0;

        let result = OsTraceRelayClient::connect(provider_ref).await;

        std::mem::forget(provider_box);
        result
    });

    match res {
        Ok(c) => {
            let boxed = Box::new(OsTraceRelayClientHandle(c));
            unsafe { *client = Box::into_raw(boxed) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => {
            let _ = unsafe { Box::from_raw(provider) };
            e.into()
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn os_trace_relay_free(
    handle: *mut OsTraceRelayClientHandle
)  {
     if !handle.is_null() {
        log::debug!("Freeing os trace relay client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn os_trace_relay_start_trace(
    client: *mut OsTraceRelayClientHandle,
    receiver: *mut *mut OsTraceRelayReceiverHandle,
    pid: *const u32,
) -> IdeviceErrorCode {
    if receiver.is_null() || client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let pid_option = if pid.is_null() {
        None
    } else {
        Some(unsafe { *pid })
    };

    let client_owned = unsafe { Box::from_raw(client) };
    
    let res= RUNTIME.block_on(async {
        client_owned
            .0
            .start_trace(pid_option)
            .await
    });

    match res {
        Ok(relay) => {
            let boxed = Box::new(OsTraceRelayReceiverHandle(relay));
            unsafe { *receiver = Box::into_raw(boxed) };

            IdeviceErrorCode::IdeviceSuccess
        },
        Err(e) => e.into(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn os_trace_relay_receiver_free(
    handle: *mut OsTraceRelayReceiverHandle
) {
    if !handle.is_null() {
        log::debug!("Freeing syslog relay client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn os_trace_relay_get_pid_list(
    client: *mut OsTraceRelayClientHandle,
    list: *mut *mut Vec<u64>,
) -> IdeviceErrorCode {
    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .get_pid_list()
            .await
    });

    match res {
        Ok(r) => {
            unsafe { *list = Box::into_raw(Box::new(r)) };
            IdeviceErrorCode::IdeviceSuccess
        },
        Err(e) => e.into(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn os_trace_relay_next(
    client: *mut OsTraceRelayReceiverHandle,
    log: *mut *mut OsTraceLog
) -> IdeviceErrorCode {
    if client.is_null() {
        log::error!("Null pointer provided");
        return IdeviceErrorCode::InvalidArg;
    }

    let res = RUNTIME.block_on(async {
        unsafe { &mut *client }
            .0
            .next()
            .await
    });

    match res {
        Ok(r) => {
            let log_entry = Box::new(OsTraceLog {
                pid: r.pid,
                timestamp: r.timestamp.and_utc().timestamp(),
                level: r.level as u8,
                image_name: CString::new(r.image_name).unwrap().into_raw(),
                filename: CString::new(r.filename).unwrap().into_raw(),
                message: CString::new(r.message).unwrap().into_raw(),
                label: if let Some(label) = r.label {
                    Box::into_raw(Box::new(SyslogLabel {
                        subsystem: CString::new(label.subsystem).unwrap().into_raw(),
                        category: CString::new(label.category).unwrap().into_raw(),
                    }))
                } else {
                    std::ptr::null()
                },
            });
            
            unsafe { *log = Box::into_raw(log_entry) };
            IdeviceErrorCode::IdeviceSuccess
        }
        Err(e) => e.into(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn os_trace_relay_free_log(log: *mut OsTraceLog) {
    if !log.is_null() {
        unsafe {
            if !(*log).image_name.is_null() {
                let _ = CString::from_raw((*log).image_name as *mut c_char);
            }
            if !(*log).filename.is_null() {
                let _ = CString::from_raw((*log).filename as *mut c_char);
            }
            if !(*log).message.is_null() {
                let _ = CString::from_raw((*log).message as *mut c_char);
            }
            if !(*log).label.is_null() {
                let label = &*(*log).label;
                
                if !label.subsystem.is_null() {
                    let _ = CString::from_raw(label.subsystem as *mut c_char);
                }
                
                if !label.category.is_null() {
                    let _ = CString::from_raw(label.category as *mut c_char);
                }

                let _ = Box::from_raw((*log).label as *mut SyslogLabel);
            }
            
            let _ = Box::from_raw(log);
        }
    }
}