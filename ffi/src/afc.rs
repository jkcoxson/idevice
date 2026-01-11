// Jackson Coxson

use std::{io::SeekFrom, ptr::null_mut};

use idevice::{
    IdeviceError, IdeviceService,
    afc::{AfcClient, DeviceInfo, FileInfo, file::FileDescriptor},
    provider::IdeviceProvider,
};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::{
    IdeviceFfiError, IdeviceHandle, LOCAL_RUNTIME, ffi_err, provider::IdeviceProviderHandle,
    run_sync, run_sync_local,
};

pub struct AfcClientHandle(pub AfcClient);

/// Connects to the AFC service using a TCP provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated AfcClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_client_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };

        AfcClient::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(AfcClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Connects to the AFC2 service using a TCP provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated AfcClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc2_client_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };

        AfcClient::new_afc2(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(AfcClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new AfcClient from an existing Idevice connection
///
/// # Arguments
/// * [`socket`] - An IdeviceSocket handle
/// * [`client`] - On success, will be set to point to a newly allocated AfcClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_client_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut AfcClientHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = AfcClient::new(socket);
    let boxed = Box::new(AfcClientHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Frees an AfcClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_client_free(handle: *mut AfcClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing afc_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Lists the contents of a directory on the device
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path to the directory to list (UTF-8 null-terminated)
/// * [`entries`] - Will be set to point to an array of directory entries
/// * [`count`] - Will be set to the number of entries
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_list_directory(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
    entries: *mut *mut *mut libc::c_char,
    count: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if path.is_null() || entries.is_null() || count.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    // Use to_string_lossy to handle non-UTF8 paths
    let path = path_cstr.to_string_lossy();

    let res: Result<Vec<String>, IdeviceError> = run_sync_local(async move {
        // SAFETY: We're assuming client is a valid pointer here
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.list_dir(&path.to_string()).await
    });

    match res {
        Ok(items) => {
            // Create a heap-allocated array of C strings that will be freed by the caller
            let c_strings = items
                .into_iter()
                .filter_map(|s| std::ffi::CString::new(s).ok())
                .collect::<Vec<_>>();

            // Get the count before we modify anything
            let string_count = c_strings.len();

            // Create memory for array of char pointers (including NULL terminator)
            let layout = std::alloc::Layout::array::<*mut libc::c_char>(string_count + 1).unwrap();
            let ptr = unsafe { std::alloc::alloc(layout) as *mut *mut libc::c_char };
            if ptr.is_null() {
                return ffi_err!(IdeviceError::FfiInvalidArg);
            }

            // Fill the array with pointers to the strings, then leak each CString
            for (i, cstring) in c_strings.into_iter().enumerate() {
                let string_ptr = cstring.into_raw();
                unsafe { *ptr.add(i) = string_ptr };
            }

            // Set NULL terminator
            unsafe { *ptr.add(string_count) = std::ptr::null_mut() };

            // Store the result and count
            unsafe {
                *entries = ptr;
                *count = string_count;
            }

            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new directory on the device
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path of the directory to create (UTF-8 null-terminated)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_make_directory(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    let path = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.mk_dir(path).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// File information structure for C bindings
#[repr(C)]
pub struct AfcFileInfo {
    pub size: libc::size_t,
    pub blocks: libc::size_t,
    pub creation: i64,
    pub modified: i64,
    pub st_nlink: *mut libc::c_char,
    pub st_ifmt: *mut libc::c_char,
    pub st_link_target: *mut libc::c_char,
}

/// Retrieves information about a file or directory
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path to the file or directory (UTF-8 null-terminated)
/// * [`info`] - Will be populated with file information
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` and `path` must be valid pointers
/// `info` must be a valid pointer to an AfcFileInfo struct
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_get_file_info(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
    info: *mut AfcFileInfo,
) -> *mut IdeviceFfiError {
    if client.is_null() || path.is_null() || info.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    let path = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<FileInfo, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_file_info(path).await
    });

    match res {
        Ok(file_info) => {
            unsafe {
                (*info).size = file_info.size;
                (*info).blocks = file_info.blocks;
                (*info).creation = file_info.creation.and_utc().timestamp();
                (*info).modified = file_info.modified.and_utc().timestamp();

                (*info).st_nlink = std::ffi::CString::new(file_info.st_nlink)
                    .unwrap()
                    .into_raw();

                (*info).st_ifmt = std::ffi::CString::new(file_info.st_ifmt)
                    .unwrap()
                    .into_raw();

                (*info).st_link_target = match file_info.st_link_target {
                    Some(target) => std::ffi::CString::new(target).unwrap().into_raw(),
                    None => std::ptr::null_mut(),
                };
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees memory allocated by afc_get_file_info
///
/// # Arguments
/// * [`info`] - Pointer to AfcFileInfo struct to free
///
/// # Safety
/// `info` must be a valid pointer to an AfcFileInfo struct previously returned by afc_get_file_info
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_info_free(info: *mut AfcFileInfo) {
    if !info.is_null() {
        unsafe {
            if !(*info).st_nlink.is_null() {
                let _ = std::ffi::CString::from_raw((*info).st_nlink);
            }
            if !(*info).st_ifmt.is_null() {
                let _ = std::ffi::CString::from_raw((*info).st_ifmt);
            }
            if !(*info).st_link_target.is_null() {
                let _ = std::ffi::CString::from_raw((*info).st_link_target);
            }
        }
    }
}

/// Device information structure for C bindings
#[repr(C)]
pub struct AfcDeviceInfo {
    pub model: *mut libc::c_char,
    pub total_bytes: libc::size_t,
    pub free_bytes: libc::size_t,
    pub block_size: libc::size_t,
}

/// Retrieves information about the device's filesystem
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`info`] - Will be populated with device information
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` and `info` must be valid pointers
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_get_device_info(
    client: *mut AfcClientHandle,
    info: *mut AfcDeviceInfo,
) -> *mut IdeviceFfiError {
    if client.is_null() || info.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<DeviceInfo, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.get_device_info().await
    });

    match res {
        Ok(device_info) => {
            unsafe {
                (*info).model = std::ffi::CString::new(device_info.model)
                    .unwrap()
                    .into_raw();
                (*info).total_bytes = device_info.total_bytes;
                (*info).free_bytes = device_info.free_bytes;
                (*info).block_size = device_info.block_size;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees memory allocated by afc_get_device_info
///
/// # Arguments
/// * [`info`] - Pointer to AfcDeviceInfo struct to free
///
/// # Safety
/// `info` must be a valid pointer to an AfcDeviceInfo struct previously returned by afc_get_device_info
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_device_info_free(info: *mut AfcDeviceInfo) {
    if !info.is_null() && unsafe { !(*info).model.is_null() } {
        unsafe {
            let _ = std::ffi::CString::from_raw((*info).model);
        }
    }
}

/// Removes a file or directory
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path to the file or directory to remove (UTF-8 null-terminated)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_remove_path(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    let path = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.remove(path).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Recursively removes a directory and all its contents
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path to the directory to remove (UTF-8 null-terminated)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `path` must be a valid null-terminated C string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_remove_path_and_contents(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    let path = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.remove_all(path).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

#[repr(C)]
pub enum AfcFopenMode {
    AfcRdOnly = 0x00000001,   // r   O_RDONLY
    AfcRw = 0x00000002,       // r+  O_RDWR   | O_CREAT
    AfcWrOnly = 0x00000003,   // w   O_WRONLY | O_CREAT  | O_TRUNC
    AfcWr = 0x00000004,       // w+  O_RDWR   | O_CREAT  | O_TRUNC
    AfcAppend = 0x00000005,   // a   O_WRONLY | O_APPEND | O_CREAT
    AfcRdAppend = 0x00000006, // a+  O_RDWR   | O_APPEND | O_CREAT
}

impl From<AfcFopenMode> for idevice::afc::opcode::AfcFopenMode {
    fn from(value: AfcFopenMode) -> Self {
        match value {
            AfcFopenMode::AfcRdOnly => idevice::afc::opcode::AfcFopenMode::RdOnly,
            AfcFopenMode::AfcRw => idevice::afc::opcode::AfcFopenMode::Rw,
            AfcFopenMode::AfcWrOnly => idevice::afc::opcode::AfcFopenMode::WrOnly,
            AfcFopenMode::AfcWr => idevice::afc::opcode::AfcFopenMode::Wr,
            AfcFopenMode::AfcAppend => idevice::afc::opcode::AfcFopenMode::Append,
            AfcFopenMode::AfcRdAppend => idevice::afc::opcode::AfcFopenMode::RdAppend,
        }
    }
}

/// Handle for an open file on the device
#[allow(dead_code)]
pub struct AfcFileHandle<'a>(Box<idevice::afc::file::FileDescriptor<'a>>); // Opaque pointer

/// Opens a file on the device
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`path`] - Path to the file to open (UTF-8 null-terminated)
/// * [`mode`] - File open mode
/// * [`handle`] - Will be set to a new file handle on success
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `path` must be a valid null-terminated C string.
/// The file handle MAY NOT be used from another thread, and is
/// dependant upon the client it was created by.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_open(
    client: *mut AfcClientHandle,
    path: *const libc::c_char,
    mode: AfcFopenMode,
    handle: *mut *mut AfcFileHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() || path.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let path_cstr = unsafe { std::ffi::CStr::from_ptr(path) };
    let path = match path_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let mode = mode.into();

    let res: Result<*mut AfcFileHandle, IdeviceError> = LOCAL_RUNTIME.block_on(async move {
        let client_ref = unsafe { &mut (*client).0 };
        let result = client_ref.open(path, mode).await;
        match result {
            Ok(f) => {
                let boxed = Box::new(f);
                Ok(Box::into_raw(boxed) as *mut AfcFileHandle)
            }
            Err(e) => Err(e),
        }
    });

    match res {
        Ok(f) => {
            unsafe { *handle = f }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Closes a file handle
///
/// # Arguments
/// * [`handle`] - File handle to close
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_close(handle: *mut AfcFileHandle) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { Box::from_raw(handle as *mut idevice::afc::file::FileDescriptor) };
    let res: Result<(), IdeviceError> = run_sync(async move { fd.close().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads data from an open file. This advances the cursor of the file.
///
/// # Arguments
/// * [`handle`] - File handle to read from
/// * [`data`] - Will be set to point to the read data
/// * [`len`] - Number of bytes to read from the file
/// * [`bytes_read`] - The number of bytes read from the file
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_read(
    handle: *mut AfcFileHandle,
    data: *mut *mut u8,
    len: usize,
    bytes_read: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || bytes_read.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { &mut *(handle as *mut FileDescriptor) };
    let res: Result<Vec<u8>, IdeviceError> = run_sync({
        let mut buf = vec![0u8; len];
        async move {
            let r = fd.read(&mut buf).await?;
            buf.truncate(r);
            Ok(buf)
        }
    });

    match res {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            unsafe {
                *data = boxed.as_mut_ptr();
                *bytes_read = boxed.len();
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads all data from an open file.
///
/// # Arguments
/// * [`handle`] - File handle to read from
/// * [`data`] - Will be set to point to the read data
/// * [`length`] - The number of bytes read from the file
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_read_entire(
    handle: *mut AfcFileHandle,
    data: *mut *mut u8,
    length: *mut libc::size_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || length.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { &mut *(handle as *mut idevice::afc::file::FileDescriptor) };
    let res: Result<Vec<u8>, IdeviceError> = run_sync(async move { fd.read_entire().await });

    match res {
        Ok(bytes) => {
            let mut boxed = bytes.into_boxed_slice();
            unsafe {
                *data = boxed.as_mut_ptr();
                *length = boxed.len();
            }
            std::mem::forget(boxed);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Moves the read/write cursor in an open file.
///
/// # Arguments
/// * [`handle`] - File handle whose cursor should be moved
/// * [`offset`] - Distance to move the cursor, interpreted based on `whence`
/// * [`whence`] - Origin used for the seek operation:
///     * `0` — Seek from the start of the file (`SeekFrom::Start`)
///     * `1` — Seek from the current cursor position (`SeekFrom::Current`)
///     * `2` — Seek from the end of the file (`SeekFrom::End`)
/// * [`new_pos`] - Output parameter; will be set to the new absolute cursor position
///
/// # Returns
/// An [`IdeviceFfiError`] on error, or null on success.
///
/// # Safety
/// All pointers must be valid and non-null.
///
/// # Notes
/// * If `whence` is invalid, this function returns `FfiInvalidArg`.
/// * The AFC protocol may restrict seeking beyond certain bounds; such errors
///   are reported through the returned [`IdeviceFfiError`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_seek(
    handle: *mut AfcFileHandle,
    offset: i64,
    whence: libc::c_int,
    new_pos: *mut i64,
) -> *mut IdeviceFfiError {
    if handle.is_null() || new_pos.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { &mut *(handle as *mut FileDescriptor) };

    let seek_from = match whence {
        0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<u64, IdeviceError> = run_sync(async move { Ok(fd.seek(seek_from).await?) });

    match res {
        Ok(pos) => {
            unsafe {
                *new_pos = pos as i64;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Returns the current read/write cursor position of an open file.
///
/// # Arguments
/// * [`handle`] - File handle whose cursor should be queried
/// * [`pos`] - Output parameter; will be set to the current absolute cursor position
///
/// # Returns
/// An [`IdeviceFfiError`] on error, or null on success.
///
/// # Safety
/// All pointers must be valid and non-null.
///
/// # Notes
/// This function is equivalent to performing a seek operation with
/// `SeekFrom::Current(0)` internally.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_tell(
    handle: *mut AfcFileHandle,
    pos: *mut i64,
) -> *mut IdeviceFfiError {
    if handle.is_null() || pos.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { &mut *(handle as *mut FileDescriptor) };

    let res: Result<u64, IdeviceError> =
        run_sync(async { Ok(fd.seek(SeekFrom::Current(0)).await?) });

    match res {
        Ok(cur) => {
            unsafe {
                *pos = cur as i64;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Writes data to an open file
///
/// # Arguments
/// * [`handle`] - File handle to write to
/// * [`data`] - Data to write
/// * [`length`] - Length of data to write
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `data` must point to at least `length` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_write(
    handle: *mut AfcFileHandle,
    data: *const u8,
    length: libc::size_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let fd = unsafe { &mut *(handle as *mut idevice::afc::file::FileDescriptor) };
    let data_slice = unsafe { std::slice::from_raw_parts(data, length) };

    let res: Result<(), IdeviceError> = run_sync(async move { fd.write_entire(data_slice).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Link type for creating hard or symbolic links
#[repr(C)]
pub enum AfcLinkType {
    Hard = 1,
    Symbolic = 2,
}

/// Creates a hard or symbolic link
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`target`] - Target path of the link (UTF-8 null-terminated)
/// * [`source`] - Path where the link should be created (UTF-8 null-terminated)
/// * [`link_type`] - Type of link to create
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `target` and `source` must be valid null-terminated C strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_make_link(
    client: *mut AfcClientHandle,
    target: *const libc::c_char,
    source: *const libc::c_char,
    link_type: AfcLinkType,
) -> *mut IdeviceFfiError {
    if client.is_null() || target.is_null() || source.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let target_cstr = unsafe { std::ffi::CStr::from_ptr(target) };
    let target = match target_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let source_cstr = unsafe { std::ffi::CStr::from_ptr(source) };
    let source = match source_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let link_type = match link_type {
        AfcLinkType::Hard => idevice::afc::opcode::LinkType::Hardlink,
        AfcLinkType::Symbolic => idevice::afc::opcode::LinkType::Symlink,
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.link(target, source, link_type).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Renames a file or directory
///
/// # Arguments
/// * [`client`] - A valid AfcClient handle
/// * [`source`] - Current path of the file/directory (UTF-8 null-terminated)
/// * [`target`] - New path for the file/directory (UTF-8 null-terminated)
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid and non-null
/// `source` and `target` must be valid null-terminated C strings
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_rename_path(
    client: *mut AfcClientHandle,
    source: *const libc::c_char,
    target: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || source.is_null() || target.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let source_cstr = unsafe { std::ffi::CStr::from_ptr(source) };
    let source = match source_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let target_cstr = unsafe { std::ffi::CStr::from_ptr(target) };
    let target = match target_cstr.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidArg),
    };

    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.rename(source, target).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees memory allocated by a file read function allocated by this library
///
/// # Arguments
/// * [`info`] - Pointer to AfcDeviceInfo struct to free
///
/// # Safety
/// `info` must be a valid pointer to an AfcDeviceInfo struct previously returned by afc_get_device_info
#[unsafe(no_mangle)]
pub unsafe extern "C" fn afc_file_read_data_free(data: *mut u8, length: libc::size_t) {
    if !data.is_null() {
        let boxed = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, length)) };
        drop(boxed);
    }
}
