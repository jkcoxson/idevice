// Jackson Coxson

use std::ptr::null_mut;

use idevice::{IdeviceError, IdeviceService, services::screenshotr::ScreenshotService, provider::IdeviceProvider};

use crate::{IdeviceFfiError, ffi_err, provider::IdeviceProviderHandle, run_sync_local};

pub struct ScreenshotrClientHandle(pub ScreenshotService);

/// Represents a screenshot data buffer
#[repr(C)]
pub struct ScreenshotData {
    pub data: *mut u8,
    pub length: usize,
}

/// Connects to screenshotr service using provider
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`client`] - On success, will be set to point to a newly allocated ScreenshotrClient handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid pointer to a handle allocated by this library
/// `client` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshotr_connect(
    provider: *mut IdeviceProviderHandle,
    client: *mut *mut ScreenshotrClientHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || client.is_null() {
        tracing::error!("Null pointer provided");
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<ScreenshotService, IdeviceError> = run_sync_local(async move {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        ScreenshotService::connect(provider_ref).await
    });

    match res {
        Ok(r) => {
            let boxed = Box::new(ScreenshotrClientHandle(r));
            unsafe { *client = Box::into_raw(boxed) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Takes a screenshot from the device
///
/// # Arguments
/// * `client` - A valid ScreenshotrClient handle
/// * `screenshot` - Pointer to store the screenshot data
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `client` must be a valid pointer to a handle allocated by this library
/// `screenshot` must be a valid pointer to store the screenshot data
/// The caller is responsible for freeing the screenshot data using screenshotr_screenshot_free
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshotr_take_screenshot(
    client: *mut ScreenshotrClientHandle,
    screenshot: *mut ScreenshotData,
) -> *mut IdeviceFfiError {
    if client.is_null() || screenshot.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<Vec<u8>, IdeviceError> = run_sync_local(async move {
        let client_ref = unsafe { &mut (*client).0 };
        client_ref.take_screenshot().await
    });

    match res {
        Ok(data) => {
            let len = data.len();
            let boxed = data.into_boxed_slice();
            let ptr = Box::into_raw(boxed) as *mut u8;
            
            unsafe {
                (*screenshot).data = ptr;
                (*screenshot).length = len;
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Frees screenshot data
///
/// # Arguments
/// * `screenshot` - The screenshot data to free
///
/// # Safety
/// `screenshot` must be a valid ScreenshotData that was allocated by screenshotr_take_screenshot
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshotr_screenshot_free(screenshot: ScreenshotData) {
    if !screenshot.data.is_null() && screenshot.length > 0 {
        tracing::debug!("Freeing screenshot data");
        let _ = unsafe {
            Vec::from_raw_parts(screenshot.data, screenshot.length, screenshot.length)
        };
    }
}

/// Frees a ScreenshotrClient handle
///
/// # Arguments
/// * [`handle`] - The handle to free
///
/// # Safety
/// `handle` must be a valid pointer to the handle that was allocated by this library,
/// or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn screenshotr_client_free(handle: *mut ScreenshotrClientHandle) {
    if !handle.is_null() {
        tracing::debug!("Freeing screenshotr_client");
        let _ = unsafe { Box::from_raw(handle) };
    }
}
