// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::os::raw::c_int;
use std::ptr::null_mut;

use idevice::core_device::{Domain, FileServiceClient};
use idevice::{IdeviceError, ReadWrite};

use crate::{IdeviceFfiError, ReadWriteOpaque, ffi_err, run_sync, run_sync_local};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use crate::{core_device_proxy::AdapterHandle, rsd::RsdHandshakeHandle};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use idevice::RsdService as _;

/// Opaque handle to a FileServiceClient
pub struct FileServiceHandle(pub FileServiceClient<Box<dyn ReadWrite>>);

/// Which of the device's filesystem domains a session is scoped to
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IdeviceFileServiceDomain {
    /// An app's own data container. The identifier is the bundle ID.
    IdeviceFileServiceDomainAppDataContainer = 1,
    /// A shared app-group container. The identifier is the group ID.
    IdeviceFileServiceDomainAppGroupDataContainer = 2,
    /// The temporary directory.
    IdeviceFileServiceDomainTemporary = 3,
    /// The system crash-log store.
    IdeviceFileServiceDomainSystemCrashLogs = 5,
}

impl From<IdeviceFileServiceDomain> for Domain {
    fn from(value: IdeviceFileServiceDomain) -> Self {
        match value {
            IdeviceFileServiceDomain::IdeviceFileServiceDomainAppDataContainer => {
                Domain::AppDataContainer
            }
            IdeviceFileServiceDomain::IdeviceFileServiceDomainAppGroupDataContainer => {
                Domain::AppGroupDataContainer
            }
            IdeviceFileServiceDomain::IdeviceFileServiceDomainTemporary => Domain::Temporary,
            IdeviceFileServiceDomain::IdeviceFileServiceDomainSystemCrashLogs => {
                Domain::SystemCrashLogs
            }
        }
    }
}

/// Creates a new FileServiceClient using RSD connection
///
/// This connects the service's control channel, i.e.
/// `com.apple.coredevice.fileservice.control`. Downloads additionally need the
/// data channel, `com.apple.coredevice.fileservice.data`.
///
/// # Arguments
/// * [`provider`] - An adapter created by this library
/// * [`handshake`] - An RSD handshake from the same provider
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` and `handshake` must be valid pointers to handles allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut FileServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<FileServiceClient<Box<dyn ReadWrite>>, IdeviceError> = run_sync_local(async {
        let provider_ref = unsafe { &mut (*provider).0 };
        let handshake_ref = unsafe { &mut (*handshake).0 };

        FileServiceClient::connect_rsd(provider_ref, handshake_ref).await
    });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(FileServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new FileServiceClient from a socket
///
/// # Arguments
/// * [`socket`] - The socket to use for communication. Consumed regardless of the result.
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut FileServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let res = run_sync(async move { FileServiceClient::new(socket.inner.unwrap()).await });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(FileServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Opens a session on a domain, which every later command is scoped to
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`domain`] - The domain to scope the session to
/// * [`identifier`] - The container's identifier, i.e. a bundle ID or an app-group ID.
///   The domains that don't take one ignore it, and it may be NULL for them.
/// * [`session_id`] - Pointer to store the new session's ID, or NULL to ignore it.
///   Free with `idevice_string_free`.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All non-NULL pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_create_session(
    handle: *mut FileServiceHandle,
    domain: IdeviceFileServiceDomain,
    identifier: *const c_char,
    session_id: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let identifier = if identifier.is_null() {
        ""
    } else {
        match unsafe { CStr::from_ptr(identifier) }.to_str() {
            Ok(s) => s,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let client = unsafe { &mut (*handle).0 };
    match run_sync_local(async { client.create_session(domain.into(), identifier).await }) {
        Ok(session) => {
            if !session_id.is_null() {
                match CString::new(session) {
                    Ok(s) => unsafe { *session_id = s.into_raw() },
                    Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
                }
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// The session ID from the last `file_service_create_session`
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`session_id`] - Pointer to store the ID, set to NULL when there is no
///   session. Free with `idevice_string_free`.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_session_id(
    handle: *mut FileServiceHandle,
    session_id: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || session_id.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &(*handle).0 };
    match client.session_id() {
        Some(session) => match CString::new(session) {
            Ok(s) => {
                unsafe { *session_id = s.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        None => {
            unsafe { *session_id = null_mut() };
            null_mut()
        }
    }
}

/// Lists a directory, relative to the session's domain root
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`path`] - The directory to list
/// * [`entries`] - Pointer to store the entry names, freed with
///   `file_service_free_directory_list`
/// * [`len`] - Pointer to store the number of entries
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_retrieve_directory_list(
    handle: *mut FileServiceHandle,
    path: *const c_char,
    entries: *mut *mut *mut c_char,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() || entries.is_null() || len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    match run_sync_local(async { client.retrieve_directory_list(path).await }) {
        Ok(list) => {
            let mut c_list = list
                .into_iter()
                .map(|e| match CString::new(e) {
                    Ok(e) => e.into_raw(),
                    Err(_) => null_mut(),
                })
                .collect::<Vec<*mut c_char>>()
                .into_boxed_slice();
            unsafe {
                *len = c_list.len();
                *entries = c_list.as_mut_ptr();
            }
            std::mem::forget(c_list);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees the list from `file_service_retrieve_directory_list`
///
/// # Safety
/// `entries` must be a pointer returned by `file_service_retrieve_directory_list`
/// with its reported length, or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_free_directory_list(entries: *mut *mut c_char, len: usize) {
    if entries.is_null() {
        return;
    }
    let entries = unsafe { Vec::from_raw_parts(entries, len, len) };
    for entry in entries {
        if !entry.is_null() {
            let _ = unsafe { CString::from_raw(entry) };
        }
    }
}

/// Downloads a file, relative to the session's domain root
///
/// The transfer itself runs on the service's data channel, which the caller
/// opens by connecting the adapter to the port the RSD handshake reports for
/// `com.apple.coredevice.fileservice.data`.
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`path`] - The file to download
/// * [`adapter`] - The adapter the control channel was connected over
/// * [`data_port`] - The port of `com.apple.coredevice.fileservice.data`
/// * [`data`] - Pointer to store the contents, freed with `idevice_data_free`
/// * [`len`] - Pointer to store the number of bytes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_retrieve_file(
    handle: *mut FileServiceHandle,
    path: *const c_char,
    adapter: *mut AdapterHandle,
    data_port: u16,
    data: *mut *mut u8,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() || adapter.is_null() || data.is_null() || len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync_local(async {
        client
            .retrieve_file(path, async || {
                let adapter = unsafe { &mut (*adapter).0 };
                adapter
                    .connect(data_port)
                    .await
                    .map(|s| Box::new(s) as Box<dyn ReadWrite>)
                    .map_err(IdeviceError::from)
            })
            .await
    });

    match res {
        Ok(contents) => {
            let mut contents = contents.into_boxed_slice();
            unsafe {
                *len = contents.len();
                *data = contents.as_mut_ptr();
            }
            std::mem::forget(contents);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Downloads a file over a data channel the caller already opened
///
/// Like `file_service_retrieve_file`, but takes the data channel itself instead
/// of opening one. Note that the device only accepts the connection once the
/// control channel has announced the transfer, so a stream opened well in
/// advance may have been dropped.
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`path`] - The file to download
/// * [`data_stream`] - The data channel. Consumed regardless of the result.
/// * [`data`] - Pointer to store the contents, freed with `idevice_data_free`
/// * [`len`] - Pointer to store the number of bytes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_retrieve_file_with_stream(
    handle: *mut FileServiceHandle,
    path: *const c_char,
    data_stream: *mut ReadWriteOpaque,
    data: *mut *mut u8,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null()
        || path.is_null()
        || data_stream.is_null()
        || data.is_null()
        || len.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let Some(data_stream) = unsafe { Box::from_raw(data_stream) }.inner else {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    };

    let client = unsafe { &mut (*handle).0 };
    let res = run_sync_local(async { client.retrieve_file(path, async || Ok(data_stream)).await });

    match res {
        Ok(contents) => {
            let mut contents = contents.into_boxed_slice();
            unsafe {
                *len = contents.len();
                *data = contents.as_mut_ptr();
            }
            std::mem::forget(contents);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates an empty file, relative to the session's domain root
///
/// # Arguments
/// * [`handle`] - The FileServiceClient handle
/// * [`path`] - The file to create
/// * [`file_permissions`] - The file's mode, e.g. 0644
/// * [`uid`] - The owning user's ID, e.g. 501
/// * [`gid`] - The owning group's ID, e.g. 501
/// * [`creation_time`] - The creation time to set
/// * [`last_modification_time`] - The modification time to set
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_propose_empty_file(
    handle: *mut FileServiceHandle,
    path: *const c_char,
    file_permissions: u32,
    uid: u32,
    gid: u32,
    creation_time: i64,
    last_modification_time: i64,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    match run_sync_local(async {
        client
            .propose_empty_file(
                path,
                file_permissions,
                uid,
                gid,
                creation_time,
                last_modification_time,
            )
            .await
    }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Looks a domain up by the name the device uses, e.g. `appDataContainer`
///
/// # Arguments
/// * [`name`] - The domain's name
/// * [`domain`] - Pointer to store the domain
///
/// # Returns
/// 1 when the name is known, 0 otherwise
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_domain_from_name(
    name: *const c_char,
    domain: *mut IdeviceFileServiceDomain,
) -> c_int {
    if name.is_null() || domain.is_null() {
        return 0;
    }
    let Ok(name) = unsafe { CStr::from_ptr(name) }.to_str() else {
        return 0;
    };

    let found = match Domain::from_name(name) {
        Some(Domain::AppDataContainer) => {
            IdeviceFileServiceDomain::IdeviceFileServiceDomainAppDataContainer
        }
        Some(Domain::AppGroupDataContainer) => {
            IdeviceFileServiceDomain::IdeviceFileServiceDomainAppGroupDataContainer
        }
        Some(Domain::Temporary) => IdeviceFileServiceDomain::IdeviceFileServiceDomainTemporary,
        Some(Domain::SystemCrashLogs) => {
            IdeviceFileServiceDomain::IdeviceFileServiceDomainSystemCrashLogs
        }
        None => return 0,
    };
    unsafe { *domain = found };
    1
}

/// Frees a FileServiceClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn file_service_free(handle: *mut FileServiceHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
