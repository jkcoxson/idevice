// Jackson Coxson

use std::{
    ffi::{CStr, c_char},
    ptr::null_mut,
};

use idevice::{
    IdeviceError, IdeviceService,
    provider::IdeviceProvider,
    services::crashreportcopymobile::{CrashReportCopyMobileClient, flush_reports},
};

use crate::{
    IdeviceFfiError, IdeviceHandle, afc::AfcClientHandle, ffi_err, provider::IdeviceProviderHandle,
    run_sync_local,
};

pub struct CrashReportCopyMobileHandle(pub CrashReportCopyMobileClient);

/// Automatically creates and connects to the crash report copy mobile service,
/// returning a client handle
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut CrashReportCopyMobileHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<CrashReportCopyMobileClient, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        CrashReportCopyMobileClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(CrashReportCopyMobileHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new CrashReportCopyMobile client from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut CrashReportCopyMobileHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = CrashReportCopyMobileClient::new(socket);
    let boxed = Box::new(CrashReportCopyMobileHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Lists crash report files in the specified directory
///
/// # Arguments
/// * [`client`] - A valid CrashReportCopyMobile handle
/// * [`dir_path`] - Optional directory path (NULL for root "/")
/// * [`entries`] - Will be set to point to an array of C strings
/// * [`count`] - Will be set to the number of entries
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `dir_path` may be NULL (defaults to root)
/// Caller must free the returned array with `afc_free_directory_entries`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_ls(
    client: *mut CrashReportCopyMobileHandle,
    dir_path: *const c_char,
    entries: *mut *mut *mut c_char,
    count: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || entries.is_null() || count.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = if dir_path.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(dir_path) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let res: Result<Vec<String>, IdeviceError> = run_sync_local(async {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.ls(path).await
    });

    match res {
        Ok(items) => {
            let c_strings = items
                .into_iter()
                .filter_map(|s| std::ffi::CString::new(s).ok())
                .collect::<Vec<_>>();

            let string_count = c_strings.len();

            // Allocate array for char pointers (with NULL terminator)
            let layout = std::alloc::Layout::array::<*mut c_char>(string_count + 1).unwrap();
            let ptr = unsafe { std::alloc::alloc(layout) as *mut *mut c_char };
            if ptr.is_null() {
                return ffi_err!(IdeviceError::FfiInvalidArg);
            }

            for (i, cstring) in c_strings.into_iter().enumerate() {
                let string_ptr = cstring.into_raw();
                unsafe { *ptr.add(i) = string_ptr };
            }

            // NULL terminator
            unsafe { *ptr.add(string_count) = std::ptr::null_mut() };

            unsafe {
                *entries = ptr;
                *count = string_count;
            }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Downloads a crash report file from the device
///
/// # Arguments
/// * [`client`] - A valid CrashReportCopyMobile handle
/// * [`log_name`] - Name of the log file to download (C string)
/// * [`data`] - Will be set to point to the file contents
/// * [`length`] - Will be set to the size of the data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `log_name` must be a valid C string
/// Caller must free the returned data with `idevice_data_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_pull(
    client: *mut CrashReportCopyMobileHandle,
    log_name: *const c_char,
    data: *mut *mut u8,
    length: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if client.is_null() || log_name.is_null() || data.is_null() || length.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name = match unsafe { CStr::from_ptr(log_name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res: Result<Vec<u8>, IdeviceError> = run_sync_local(async {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.pull(name).await
    });

    match res {
        Ok(file_data) => {
            let len = file_data.len();
            let mut boxed = file_data.into_boxed_slice();
            unsafe {
                *data = boxed.as_mut_ptr();
                *length = len;
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Removes a crash report file from the device
///
/// # Arguments
/// * [`client`] - A valid CrashReportCopyMobile handle
/// * [`log_name`] - Name of the log file to remove (C string)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `log_name` must be a valid C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_remove(
    client: *mut CrashReportCopyMobileHandle,
    log_name: *const c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || log_name.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name = match unsafe { CStr::from_ptr(log_name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res = run_sync_local(async {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.remove(name).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Converts this client to an AFC client for advanced file operations
///
/// # Arguments
/// * [`client`] - A valid CrashReportCopyMobile handle (will be consumed)
/// * [`afc_client`] - On success, will be set to an AFC client handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer (will be freed after this call)
/// `afc_client` must be a valid, non-null pointer where the new AFC client will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_to_afc(
    client: *mut CrashReportCopyMobileHandle,
    afc_client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || afc_client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let crash_client = unsafe { Box::from_raw(client) }.0;
    let afc = crash_client.to_afc_client();

    let a = Box::into_raw(Box::new(AfcClientHandle(afc)));
    unsafe { *afc_client = a };

    null_mut()
}

/// Triggers a flush of crash logs from system storage
///
/// This connects to the crashreportmover service to move crash logs
/// into the AFC-accessible directory. Should be called before listing logs.
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_flush(
    provider: *mut IdeviceProviderHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        flush_reports(provider_ref).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a CrashReportCopyMobile client handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crash_report_client_free(handle: *mut CrashReportCopyMobileHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing crash_report_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
