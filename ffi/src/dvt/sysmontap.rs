// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    ptr::null_mut,
};

use idevice::{
    ReadWrite,
    dvt::sysmontap::{SysmontapClient, SysmontapConfig},
};
use plist_ffi::PlistWrapper;

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, plist_t, run_sync};

/// Opaque handle to a SysmontapClient
pub struct SysmontapHandle<'a>(pub SysmontapClient<'a, Box<dyn ReadWrite>>);

/// Configuration for sysmontap sampling passed over FFI
#[repr(C)]
pub struct IdeviceSysmontapConfig {
    /// Sampling interval in milliseconds
    pub interval_ms: u32,
    /// Array of process attribute name strings (null-terminated C strings)
    pub process_attributes: *const *const c_char,
    pub process_attributes_count: usize,
    /// Array of system attribute name strings (null-terminated C strings)
    pub system_attributes: *const *const c_char,
    pub system_attributes_count: usize,
}

/// Creates a new SysmontapClient from a RemoteServerClient
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut SysmontapHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { SysmontapClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(SysmontapHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a SysmontapClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_free(handle: *mut SysmontapHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Sends configuration to the device
///
/// # Arguments
/// * [`handle`] - The SysmontapClient handle
/// * [`config`] - Pointer to an IdeviceSysmontapConfig struct
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null. String arrays must contain valid C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_set_config(
    handle: *mut SysmontapHandle<'static>,
    config: *const IdeviceSysmontapConfig,
) -> *mut IdeviceFfiError {
    if handle.is_null() || config.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let cfg = unsafe { &*config };

    let process_attributes = if !cfg.process_attributes.is_null() {
        let slice = unsafe {
            std::slice::from_raw_parts(cfg.process_attributes, cfg.process_attributes_count)
        };
        let mut v = Vec::with_capacity(slice.len());
        for &ptr in slice {
            if ptr.is_null() {
                continue;
            }
            match unsafe { CStr::from_ptr(ptr).to_str() } {
                Ok(s) => v.push(s.to_string()),
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            }
        }
        v
    } else {
        Vec::new()
    };

    let system_attributes = if !cfg.system_attributes.is_null() {
        let slice = unsafe {
            std::slice::from_raw_parts(cfg.system_attributes, cfg.system_attributes_count)
        };
        let mut v = Vec::with_capacity(slice.len());
        for &ptr in slice {
            if ptr.is_null() {
                continue;
            }
            match unsafe { CStr::from_ptr(ptr).to_str() } {
                Ok(s) => v.push(s.to_string()),
                Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
            }
        }
        v
    } else {
        Vec::new()
    };

    let rust_config = SysmontapConfig {
        interval_ms: cfg.interval_ms,
        process_attributes,
        system_attributes,
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.set_config(&rust_config).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Starts sampling. Consumes the device's initial ack message internally.
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_start(
    handle: *mut SysmontapHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.start().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Stops sampling.
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_stop(
    handle: *mut SysmontapHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.stop().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads the next sysmontap sample. Blocks until data arrives.
///
/// Each output plist is a dictionary (or NULL if that field was not present in the sample):
/// - `processes_out`: dict of PID → per-process attribute array
/// - `system_out`:    plist array of system attribute values
/// - `cpu_usage_out`: dict of CPU usage keys
///
/// The caller is responsible for freeing non-NULL plists with `plist_free`.
///
/// # Safety
/// `handle` must be valid and non-null. Output pointers may be null to ignore that field.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysmontap_next_sample(
    handle: *mut SysmontapHandle<'static>,
    processes_out: *mut plist_t,
    system_out: *mut plist_t,
    cpu_usage_out: *mut plist_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.next_sample().await });

    match res {
        Ok(sample) => {
            if !processes_out.is_null() {
                match sample.processes {
                    Some(dict) => {
                        let p = PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
                        unsafe { *processes_out = p as plist_t };
                    }
                    None => unsafe { *processes_out = null_mut() },
                }
            }
            if !system_out.is_null() {
                match sample.system {
                    Some(arr) => {
                        let p = PlistWrapper::new_node(plist::Value::Array(arr)).into_ptr();
                        unsafe { *system_out = p as plist_t };
                    }
                    None => unsafe { *system_out = null_mut() },
                }
            }
            if !cpu_usage_out.is_null() {
                match sample.system_cpu_usage {
                    Some(dict) => {
                        let p = PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
                        unsafe { *cpu_usage_out = p as plist_t };
                    }
                    None => unsafe { *cpu_usage_out = null_mut() },
                }
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
