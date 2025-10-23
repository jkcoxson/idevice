// Jackson Coxson

use std::ffi::{CStr, c_char};
use std::ptr::null_mut;

use idevice::tcp::handle::StreamHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::core_device_proxy::AdapterHandle;
use crate::{IdeviceFfiError, RUNTIME, ReadWriteOpaque, ffi_err};

pub struct AdapterStreamHandle(pub StreamHandle);

/// Connects the adapter to a specific port
///
/// # Arguments
/// * [`adapter_handle`] - The adapter handle
/// * [`port`] - The port to connect to
/// * [`stream_handle`] - A pointer to allocate the new stream to
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library.
/// Any stream allocated must be used in the same thread as the adapter. The handles are NOT thread
/// safe.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_connect(
    adapter_handle: *mut AdapterHandle,
    port: u16,
    stream_handle: *mut *mut ReadWriteOpaque,
) -> *mut IdeviceFfiError {
    if adapter_handle.is_null() || stream_handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*adapter_handle).0 };
    let res = RUNTIME.block_on(async move { adapter.connect(port).await });

    match res {
        Ok(r) => {
            let boxed = Box::new(ReadWriteOpaque {
                inner: Some(Box::new(r)),
            });
            unsafe { *stream_handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => {
            tracing::error!("Adapter connect failed: {e}");
            ffi_err!(e)
        }
    }
}

/// Enables PCAP logging for the adapter
///
/// # Arguments
/// * [`handle`] - The adapter handle
/// * [`path`] - The path to save the PCAP file (null-terminated string)
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `path` must be a valid null-terminated string
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_pcap(
    handle: *mut AdapterHandle,
    path: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*handle).0 };
    let c_str = unsafe { CStr::from_ptr(path) };
    let path_str = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res = RUNTIME.block_on(async move { adapter.pcap(path_str).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => {
            tracing::error!("Adapter pcap failed: {e}");
            ffi_err!(e)
        }
    }
}

/// Closes the adapter stream connection
///
/// # Arguments
/// * [`handle`] - The adapter stream handle
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_stream_close(
    handle: *mut AdapterStreamHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*handle).0 };
    RUNTIME.block_on(async move { adapter.close() });

    null_mut()
}

/// Stops the entire adapter TCP stack
///
/// # Arguments
/// * [`handle`] - The adapter handle
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_close(handle: *mut AdapterHandle) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*handle).0 };
    RUNTIME.block_on(async move { adapter.close().await.ok() });

    null_mut()
}

/// Sends data through the adapter stream
///
/// # Arguments
/// * [`handle`] - The adapter stream handle
/// * [`data`] - The data to send
/// * [`length`] - The length of the data
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `length` bytes
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_send(
    handle: *mut AdapterStreamHandle,
    data: *const u8,
    length: usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*handle).0 };
    let data_slice = unsafe { std::slice::from_raw_parts(data, length) };

    let res = RUNTIME.block_on(async move { adapter.write_all(data_slice).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => {
            tracing::error!("Adapter send failed: {e}");
            ffi_err!(e)
        }
    }
}

/// Receives data from the adapter stream
///
/// # Arguments
/// * [`handle`] - The adapter stream handle
/// * [`data`] - Pointer to a buffer where the received data will be stored
/// * [`length`] - Pointer to store the actual length of received data
/// * [`max_length`] - Maximum number of bytes that can be stored in `data`
///
/// # Returns
/// Null on success, an IdeviceFfiError otherwise
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
/// `data` must be a valid pointer to at least `max_length` bytes
/// `length` must be a valid pointer to a usize
#[unsafe(no_mangle)]
pub unsafe extern "C" fn adapter_recv(
    handle: *mut AdapterStreamHandle,
    data: *mut u8,
    length: *mut usize,
    max_length: usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || length.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let adapter = unsafe { &mut (*handle).0 };
    let res: Result<Vec<u8>, std::io::Error> = RUNTIME.block_on(async move {
        let mut buf = [0; 2048];
        let res = adapter.read(&mut buf).await?;
        Ok(buf[..res].to_vec())
    });

    match res {
        Ok(received_data) => {
            let received_len = received_data.len();
            if received_len > max_length {
                return ffi_err!(IdeviceError::FfiBufferTooSmall(received_len, max_length));
            }

            unsafe {
                std::ptr::copy_nonoverlapping(received_data.as_ptr(), data, received_len);
                *length = received_len;
            }

            null_mut()
        }
        Err(e) => {
            tracing::error!("Adapter recv failed: {e}");
            ffi_err!(e)
        }
    }
}
