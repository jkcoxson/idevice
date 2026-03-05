use crate::{
    IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync_local,
};
use idevice::{
    IdeviceError, IdeviceService, provider::IdeviceProvider,
    services::simulate_location::LocationSimulationService,
};
use std::ptr::null_mut;

// Opaque handle wrapping the Rust service client
pub struct LocationSimulationServiceHandle(pub LocationSimulationService);

/// Connects to the Location Simulation service using a provider  
/// This is the location_simulation api for iOS 16 and below
/// You must have a developer disk image mounted to use this API
///
/// # Safety  
/// `provider` must be valid; `client` must be a non-null pointer to store the handle.  
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_connect(
    provider: *mut IdeviceProviderHandle,
    handle: *mut *mut LocationSimulationServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handle.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<LocationSimulationService, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        LocationSimulationService::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(LocationSimulationServiceHandle(r));
            unsafe { *handle = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new Location Simulation service client directly from an existing `IdeviceHandle` (socket).
///
/// # Safety
/// - `socket` must be a valid, unowned pointer to an `IdeviceHandle` that has been properly
///   initialized and represents an open connection to the Location Simulation service.
///   Ownership of the `IdeviceHandle` is transferred to this function.
/// - `client` must be a non-null pointer to a location where the newly created
///   `*mut LocationSimulationServiceHandle` will be stored.
///
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_new(
    socket: *mut IdeviceHandle,
    client: *mut *mut LocationSimulationServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let socket = unsafe { Box::from_raw(socket) }.0;
    let r = LocationSimulationService::new(socket);
    let boxed = Box::new(LocationSimulationServiceHandle(r));
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Sets the device's simulated location.
/// This is the location_simulation api for iOS 16 and below.
///
/// # Safety
/// `handle` must be a valid pointer to a `LocationSimulationServiceHandle` returned by `lockdown_location_simulation_connect`.
/// `latitude` and `longitude` must be valid, null-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_set(
    handle: *mut LocationSimulationServiceHandle,
    latitude: *const libc::c_char,
    longitude: *const libc::c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || latitude.is_null() || longitude.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let latitude = unsafe { std::ffi::CStr::from_ptr(latitude) }
        .to_string_lossy()
        .into_owned();
    let longitude = unsafe { std::ffi::CStr::from_ptr(longitude) }
        .to_string_lossy()
        .into_owned();

    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (*handle).0 };

        client_ref.set(&latitude, &longitude).await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Clears the device's simulated location, returning it to the actual location.
/// This is the location_simulation api for iOS 16 and below.
///
/// # Safety
/// `handle` must be a valid pointer to a `LocationSimulationServiceHandle` returned by `lockdown_location_simulation_connect`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_clear(
    handle: *mut LocationSimulationServiceHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async move {
        let client_ref = unsafe { &mut (*handle).0 };

        client_ref.clear().await
    });

    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a LocationSimulationService handle  
///  
/// # Safety  
/// `handle` must be a pointer returned by `lockdown_location_simulation_connect`.  
#[unsafe(no_mangle)]
pub unsafe extern "C" fn lockdown_location_simulation_free(
    handle: *mut LocationSimulationServiceHandle,
) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
