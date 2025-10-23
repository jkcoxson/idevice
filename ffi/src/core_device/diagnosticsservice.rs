// Jackson Coxson

use std::ffi::{CString, c_char};
use std::pin::Pin;
use std::ptr::null_mut;

use futures::{Stream, StreamExt};
use idevice::core_device::DiagnostisServiceClient;
use idevice::{IdeviceError, ReadWrite, RsdService};
use tracing::debug;

use crate::core_device_proxy::AdapterHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::{IdeviceFfiError, ReadWriteOpaque, ffi_err, run_sync, run_sync_local};

/// Opaque handle to an AppServiceClient
pub struct DiagnosticsServiceHandle(pub DiagnostisServiceClient<Box<dyn ReadWrite>>);
pub struct SysdiagnoseStreamHandle<'a>(
    pub Pin<Box<dyn Stream<Item = Result<Vec<u8>, IdeviceError>> + 'a>>,
);

/// Creates a new DiagnosticsServiceClient using RSD connection
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
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut DiagnosticsServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<DiagnostisServiceClient<Box<dyn ReadWrite>>, IdeviceError> =
        run_sync_local(async move {
            let provider_ref = unsafe { &mut (*provider).0 };
            let handshake_ref = unsafe { &mut (*handshake).0 };
            debug!(
                "Connecting to DiagnosticsService: provider {provider_ref:?}, handshake: {:?}",
                handshake_ref.uuid
            );

            DiagnostisServiceClient::connect_rsd(provider_ref, handshake_ref).await
        });

    match res {
        Ok(client) => {
            debug!("Connected to DiagnosticsService");
            let boxed = Box::new(DiagnosticsServiceHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new DiagnostisServiceClient from a socket
///
/// # Arguments
/// * [`socket`] - The socket to use for communication
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `socket` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_service_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut DiagnosticsServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let res =
        run_sync(async move { DiagnostisServiceClient::from_stream(socket.inner.unwrap()).await });

    match res {
        Ok(client) => {
            let new_handle = DiagnosticsServiceHandle(client);
            unsafe { *handle = Box::into_raw(Box::new(new_handle)) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Captures a sysdiagnose from the device.
/// Note that this will take a LONG time to return while the device collects enough information to
/// return to the service. This function returns a stream that can be called on to get the next
/// chunk of data. A typical sysdiagnose is roughly 1-2 GB.
///
/// # Arguments
/// * [`handle`] - The handle to the client
/// * [`dry_run`] - Whether or not to do a dry run with a simple .txt file from the device
/// * [`preferred_filename`] - The name the device wants to save the sysdaignose as
/// * [`expected_length`] - The size in bytes of the sysdiagnose
/// * [`stream_handle`] - The handle that will be set to capture bytes for
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// Pointers must be all valid. Handle must be allocated by this library. Preferred filename must
/// be freed `idevice_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_service_capture_sysdiagnose(
    handle: *mut DiagnosticsServiceHandle,
    dry_run: bool,
    preferred_filename: *mut *mut c_char,
    expected_length: *mut usize,
    stream_handle: *mut *mut SysdiagnoseStreamHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null()
        || preferred_filename.is_null()
        || expected_length.is_null()
        || stream_handle.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let handle = unsafe { &mut *handle };
    let res = run_sync_local(async move { handle.0.capture_sysdiagnose(dry_run).await });
    match res {
        Ok(res) => {
            let filename = CString::new(res.preferred_filename).unwrap();
            unsafe {
                *preferred_filename = filename.into_raw();
                *expected_length = res.expected_length;
                *stream_handle = Box::into_raw(Box::new(SysdiagnoseStreamHandle(res.stream)));
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Gets the next packet from the stream.
/// Data will be set to 0 when there is no more data to get from the stream.
///
/// # Arguments
/// * [`handle`] - The handle to the stream
/// * [`data`] - A pointer to the bytes
/// * [`len`] - The length of the bytes written
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// Pass valid pointers. The handle must be allocated by this library.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysdiagnose_stream_next(
    handle: *mut SysdiagnoseStreamHandle,
    data: *mut *mut u8,
    len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || data.is_null() || len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let handle = unsafe { &mut *handle };
    let res = run_sync_local(async move { handle.0.next().await });
    match res {
        Some(Ok(res)) => {
            let mut res = res.into_boxed_slice();
            unsafe {
                *len = res.len();
                *data = res.as_mut_ptr();
            }
            std::mem::forget(res);
            null_mut()
        }
        Some(Err(e)) => ffi_err!(e),
        None => {
            // we're empty
            unsafe { *data = null_mut() };
            null_mut()
        }
    }
}

/// Frees a DiagnostisServiceClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn diagnostics_service_free(handle: *mut DiagnosticsServiceHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Frees a SysdiagnoseStreamHandle handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysdiagnose_stream_free(handle: *mut SysdiagnoseStreamHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
