// Jackson Coxson

use std::ffi::CString;
use std::ffi::c_char;
use std::ptr::null_mut;

use idevice::IdeviceError;
use idevice::mobileactivationd::MobileActivationdClient;
use idevice::provider::IdeviceProvider;

use crate::{IdeviceFfiError, ffi_err, provider::IdeviceProviderHandle, run_sync_local};

/// Opaque handle wrapping a provider pointer for MobileActivationd.
/// The client is recreated per call since each request requires a new connection.
pub struct MobileActivationdClientHandle {
    provider: *mut IdeviceProviderHandle,
}

/// Creates a new MobileActivationd client handle from a provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider (not consumed, must remain valid for the lifetime of the handle)
/// * [`client`] - On success, will be set to point to a newly allocated handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library.
/// The provider must remain valid for the lifetime of the returned handle.
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobileactivationd_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut MobileActivationdClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let boxed = Box::new(MobileActivationdClientHandle { provider });
    unsafe { *client = Box::into_raw(boxed) };
    null_mut()
}

/// Gets the activation state of the device
///
/// # Arguments
/// * `client` - A valid MobileActivationd handle
/// * `state` - On success, will be set to a newly allocated C string with the activation state
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// The returned string must be freed with `idevice_string_free`
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobileactivationd_get_state(
    client: *mut MobileActivationdClientHandle,
    state: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if client.is_null() || state.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let provider_ptr = unsafe { (*client).provider };
    let res: Result<String, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider_ptr).0 };
        let ma_client = MobileActivationdClient::new(provider_ref);
        ma_client.state().await
    });
    match res {
        Ok(s) => match CString::new(s) {
            Ok(c_string) => {
                unsafe { *state = c_string.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Checks if the device is activated
///
/// # Arguments
/// * `client` - A valid MobileActivationd handle
/// * `activated` - On success, will be set to true if the device is activated
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobileactivationd_is_activated(
    client: *mut MobileActivationdClientHandle,
    activated: *mut bool,
) -> *mut IdeviceFfiError {
    if client.is_null() || activated.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let provider_ptr = unsafe { (*client).provider };
    let res: Result<bool, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider_ptr).0 };
        let ma_client = MobileActivationdClient::new(provider_ref);
        ma_client.activated().await
    });
    match res {
        Ok(a) => {
            unsafe { *activated = a };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Deactivates the device
///
/// # Arguments
/// * `client` - A valid MobileActivationd handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobileactivationd_deactivate(
    client: *mut MobileActivationdClientHandle,
) -> *mut IdeviceFfiError {
    if client.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    let provider_ptr = unsafe { (*client).provider };
    let res: Result<(), IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider_ptr).0 };
        let ma_client = MobileActivationdClient::new(provider_ref);
        ma_client.deactivate().await
    });
    match res {
        Ok(_) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a MobileActivationd client handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mobileactivationd_client_free(handle: *mut MobileActivationdClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing MobileActivationdClientHandle");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
