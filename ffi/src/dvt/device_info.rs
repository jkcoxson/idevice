// Jackson Coxson

use std::{
    ffi::{CStr, CString, c_char},
    ptr::null_mut,
};

use idevice::{ReadWrite, dvt::device_info::DeviceInfoClient};
use plist_ffi::PlistWrapper;

use crate::{IdeviceFfiError, dvt::remote_server::RemoteServerHandle, ffi_err, plist_t, run_sync};

/// Opaque handle to a DeviceInfoClient
pub struct DeviceInfoHandle<'a>(pub DeviceInfoClient<'a, Box<dyn ReadWrite>>);

/// A running process on the device
#[repr(C)]
pub struct IdeviceRunningProcess {
    pub pid: u32,
    pub name: *mut c_char,
    pub real_app_name: *mut c_char,
    pub is_application: bool,
    pub start_page_count: u64,
}

/// Creates a new DeviceInfoClient from a RemoteServerClient
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut DeviceInfoHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = run_sync(async move { DeviceInfoClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(DeviceInfoHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a DeviceInfoClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_free(handle: *mut DeviceInfoHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Frees a single IdeviceRunningProcess struct and its heap-allocated strings
///
/// # Safety
/// `process` must be a valid pointer allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_running_process_free(process: *mut IdeviceRunningProcess) {
    if process.is_null() {
        return;
    }
    let p = unsafe { Box::from_raw(process) };
    if !p.name.is_null() {
        let _ = unsafe { CString::from_raw(p.name) };
    }
    if !p.real_app_name.is_null() {
        let _ = unsafe { CString::from_raw(p.real_app_name) };
    }
}

/// Frees an array of IdeviceRunningProcess pointers
///
/// # Safety
/// `processes` must be a valid pointer to an array of length `count` allocated by this library,
/// or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_running_processes_free(
    processes: *mut *mut IdeviceRunningProcess,
    count: usize,
) {
    if processes.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(processes, count) };
    for &p in slice {
        unsafe { device_info_running_process_free(p) };
    }
    let _ = unsafe { Vec::from_raw_parts(processes, count, count) };
}

/// Returns the list of running processes on the device
///
/// # Arguments
/// * [`handle`] - The DeviceInfoClient handle
/// * [`processes`] - On success, set to a heap-allocated array of process pointers
/// * [`count`] - On success, set to the number of processes returned
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_running_processes(
    handle: *mut DeviceInfoHandle<'static>,
    processes: *mut *mut *mut IdeviceRunningProcess,
    count: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || processes.is_null() || count.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.running_processes().await });

    match res {
        Ok(procs) => {
            let mut ptrs: Vec<*mut IdeviceRunningProcess> = procs
                .into_iter()
                .map(|p| {
                    let name = CString::new(p.name).unwrap_or_default().into_raw();
                    let real_app_name =
                        CString::new(p.real_app_name).unwrap_or_default().into_raw();
                    Box::into_raw(Box::new(IdeviceRunningProcess {
                        pid: p.pid,
                        name,
                        real_app_name,
                        is_application: p.is_application,
                        start_page_count: p.start_page_count,
                    }))
                })
                .collect();
            ptrs.shrink_to_fit();
            unsafe { *count = ptrs.len() };
            let ptr = ptrs.as_mut_ptr();
            std::mem::forget(ptrs);
            unsafe { *processes = ptr };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns the executable name for the given PID
///
/// # Safety
/// All pointers must be valid and non-null. Free the returned string with `idevice_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_execname_for_pid(
    handle: *mut DeviceInfoHandle<'static>,
    pid: u32,
    name_out: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || name_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.execname_for_pid(pid).await });

    match res {
        Ok(name) => match CString::new(name) {
            Ok(s) => {
                unsafe { *name_out = s.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Returns whether the given PID is currently running
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_is_running_pid(
    handle: *mut DeviceInfoHandle<'static>,
    pid: u32,
    result: *mut bool,
) -> *mut IdeviceFfiError {
    if handle.is_null() || result.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.is_running_pid(pid).await });

    match res {
        Ok(b) => {
            unsafe { *result = b };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns hardware information as a plist dictionary
///
/// # Safety
/// All pointers must be valid and non-null. Free the returned plist with `plist_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_hardware_information(
    handle: *mut DeviceInfoHandle<'static>,
    plist_out: *mut plist_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || plist_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.hardware_information().await });

    match res {
        Ok(dict) => {
            let p = PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            unsafe { *plist_out = p as plist_t };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns network information as a plist dictionary
///
/// # Safety
/// All pointers must be valid and non-null. Free the returned plist with `plist_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_network_information(
    handle: *mut DeviceInfoHandle<'static>,
    plist_out: *mut plist_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || plist_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.network_information().await });

    match res {
        Ok(dict) => {
            let p = PlistWrapper::new_node(plist::Value::Dictionary(dict)).into_ptr();
            unsafe { *plist_out = p as plist_t };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns the mach kernel name
///
/// # Safety
/// All pointers must be valid and non-null. Free the returned string with `idevice_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_mach_kernel_name(
    handle: *mut DeviceInfoHandle<'static>,
    name_out: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || name_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.mach_kernel_name().await });

    match res {
        Ok(name) => match CString::new(name) {
            Ok(s) => {
                unsafe { *name_out = s.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Frees a null-terminated string array allocated by this library
///
/// # Safety
/// `strings` must be a valid pointer to an array of `count` C strings allocated by this library,
/// or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_string_array_free(strings: *mut *mut c_char, count: usize) {
    if strings.is_null() {
        return;
    }
    let slice = unsafe { std::slice::from_raw_parts(strings, count) };
    for &s in slice {
        if !s.is_null() {
            let _ = unsafe { CString::from_raw(s) };
        }
    }
    let _ = unsafe { Vec::from_raw_parts(strings, count, count) };
}

fn strings_to_c(strings: Vec<String>) -> (*mut *mut c_char, usize) {
    let mut v: Vec<*mut c_char> = strings
        .into_iter()
        .map(|s| CString::new(s).unwrap_or_default().into_raw())
        .collect();
    v.shrink_to_fit();
    let count = v.len();
    let ptr = v.as_mut_ptr();
    std::mem::forget(v);
    (ptr, count)
}

/// Returns the list of sysmon process attribute names
///
/// # Safety
/// All pointers must be valid and non-null. Free with `device_info_string_array_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_sysmon_process_attributes(
    handle: *mut DeviceInfoHandle<'static>,
    attrs_out: *mut *mut *mut c_char,
    count_out: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || attrs_out.is_null() || count_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.sysmon_process_attributes().await });

    match res {
        Ok(attrs) => {
            let (ptr, count) = strings_to_c(attrs);
            unsafe {
                *attrs_out = ptr;
                *count_out = count;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns the list of sysmon system attribute names
///
/// # Safety
/// All pointers must be valid and non-null. Free with `device_info_string_array_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_sysmon_system_attributes(
    handle: *mut DeviceInfoHandle<'static>,
    attrs_out: *mut *mut *mut c_char,
    count_out: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || attrs_out.is_null() || count_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.sysmon_system_attributes().await });

    match res {
        Ok(attrs) => {
            let (ptr, count) = strings_to_c(attrs);
            unsafe {
                *attrs_out = ptr;
                *count_out = count;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns directory listing for the given path
///
/// # Safety
/// All pointers must be valid and non-null. Free with `device_info_string_array_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn device_info_directory_listing(
    handle: *mut DeviceInfoHandle<'static>,
    path: *const c_char,
    entries_out: *mut *mut *mut c_char,
    count_out: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() || entries_out.is_null() || count_out.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync(async move { client.directory_listing(&path).await });

    match res {
        Ok(entries) => {
            let (ptr, count) = strings_to_c(entries);
            unsafe {
                *entries_out = ptr;
                *count_out = count;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
