// Jackson Coxson

use std::ptr::null_mut;

use idevice::{ReadWrite, dvt::location_simulation::LocationSimulationClient};

use crate::{IdeviceFfiError, RUNTIME, dvt::remote_server::RemoteServerHandle, ffi_err};

/// Opaque handle to a ProcessControlClient
pub struct LocationSimulationHandle<'a>(pub LocationSimulationClient<'a, Box<dyn ReadWrite>>);

/// Creates a new ProcessControlClient from a RemoteServerClient
///
/// # Arguments
/// * [`server`] - The RemoteServerClient to use
/// * [`handle`] - Pointer to store the newly created ProcessControlClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `server` must be a valid pointer to a handle allocated by this library
/// `handle` must be a valid pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_new(
    server: *mut RemoteServerHandle,
    handle: *mut *mut LocationSimulationHandle<'static>,
) -> *mut IdeviceFfiError {
    if server.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let server = unsafe { &mut (*server).0 };
    let res = RUNTIME.block_on(async move { LocationSimulationClient::new(server).await });

    match res {
        Ok(client) => {
            let boxed = Box::new(LocationSimulationHandle(client));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees a ProcessControlClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_free(handle: *mut LocationSimulationHandle<'static>) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// Clears the location set
///
/// # Arguments
/// * [`handle`] - The LocationSimulation handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid or NULL where appropriate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_clear(
    handle: *mut LocationSimulationHandle<'static>,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.clear().await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Sets the location
///
/// # Arguments
/// * [`handle`] - The LocationSimulation handle
/// * [`latitude`] - The latitude to set
/// * [`longitude`] - The longitude to set
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointers must be valid or NULL where appropriate
#[unsafe(no_mangle)]
pub unsafe extern "C" fn location_simulation_set(
    handle: *mut LocationSimulationHandle<'static>,
    latitude: f64,
    longitude: f64,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let res = RUNTIME.block_on(async move { client.set(latitude, longitude).await });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}
