// Jackson Coxson

use std::ffi::{CStr, CString, c_char};
use std::os::raw::{c_double, c_float, c_int};
use std::ptr::null_mut;

use idevice::core_device::{ConfigurationServiceClient, UserInterfaceStyle};
use idevice::{IdeviceError, ReadWrite};

use crate::{IdeviceFfiError, ReadWriteOpaque, ffi_err, run_sync, run_sync_local};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use crate::{core_device_proxy::AdapterHandle, rsd::RsdHandshakeHandle};
#[cfg(all(feature = "core_device_proxy", feature = "rsd"))]
use idevice::RsdService as _;

/// Opaque handle to a ConfigurationServiceClient
pub struct ConfigurationServiceHandle(pub ConfigurationServiceClient<Box<dyn ReadWrite>>);

/// The system's light/dark appearance
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum IdeviceUserInterfaceStyle {
    IdeviceUserInterfaceStyleLight = 0,
    IdeviceUserInterfaceStyleDark = 1,
}

impl From<UserInterfaceStyle> for IdeviceUserInterfaceStyle {
    fn from(value: UserInterfaceStyle) -> Self {
        match value {
            UserInterfaceStyle::Light => Self::IdeviceUserInterfaceStyleLight,
            UserInterfaceStyle::Dark => Self::IdeviceUserInterfaceStyleDark,
        }
    }
}

impl From<IdeviceUserInterfaceStyle> for UserInterfaceStyle {
    fn from(value: IdeviceUserInterfaceStyle) -> Self {
        match value {
            IdeviceUserInterfaceStyle::IdeviceUserInterfaceStyleLight => UserInterfaceStyle::Light,
            IdeviceUserInterfaceStyle::IdeviceUserInterfaceStyleDark => UserInterfaceStyle::Dark,
        }
    }
}

/// The accessibility color filter's state
#[repr(C)]
pub struct ColorFilterC {
    pub enabled: c_int,
    /// The filter preset's name, or NULL if the device didn't report one.
    /// Free with `idevice_string_free`.
    pub filter_type: *mut c_char,
    /// Filter strength, 0.0 to 1.0. Only meaningful when `has_intensity` is 1.
    pub intensity: c_double,
    pub has_intensity: c_int,
}

/// Creates a new ConfigurationServiceClient using RSD connection
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
pub unsafe extern "C" fn configuration_service_connect_rsd(
    provider: *mut AdapterHandle,
    handshake: *mut RsdHandshakeHandle,
    handle: *mut *mut ConfigurationServiceHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || handshake.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res: Result<ConfigurationServiceClient<Box<dyn ReadWrite>>, IdeviceError> =
        run_sync_local(async {
            let provider_ref = unsafe { &mut (*provider).0 };
            let handshake_ref = unsafe { &mut (*handshake).0 };

            ConfigurationServiceClient::connect_rsd(provider_ref, handshake_ref).await
        });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(ConfigurationServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a new ConfigurationServiceClient from a socket
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
pub unsafe extern "C" fn configuration_service_new(
    socket: *mut ReadWriteOpaque,
    handle: *mut *mut ConfigurationServiceHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket = unsafe { Box::from_raw(socket) };
    let res = run_sync(async move { ConfigurationServiceClient::new(socket.inner.unwrap()).await });

    match res {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(ConfigurationServiceHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Reads the device's light/dark appearance
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`style`] - Pointer to store the appearance
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_user_interface_style(
    handle: *mut ConfigurationServiceHandle,
    style: *mut IdeviceUserInterfaceStyle,
) -> *mut IdeviceFfiError {
    if handle.is_null() || style.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_user_interface_style().await }) {
        Ok(s) => {
            unsafe { *style = s.into() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Switches the device between light and dark appearance
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`style`] - The appearance to set
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_user_interface_style(
    handle: *mut ConfigurationServiceHandle,
    style: IdeviceUserInterfaceStyle,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_user_interface_style(style.into()).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Sets the system liquid-glass opacity
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`opacity`] - The opacity, 0.0 to 1.0
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_liquid_glass_opacity(
    handle: *mut ConfigurationServiceHandle,
    opacity: c_float,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_liquid_glass_opacity(opacity).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads the accessibility color filter's state
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`filter`] - Pointer to store the state. Free its `filter_type` with
///   `idevice_string_free`.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_color_filter(
    handle: *mut ConfigurationServiceHandle,
    filter: *mut ColorFilterC,
) -> *mut IdeviceFfiError {
    if handle.is_null() || filter.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_color_filter().await }) {
        Ok(f) => {
            let filter_type = match f.filter_type.and_then(|t| CString::new(t).ok()) {
                Some(t) => t.into_raw(),
                None => null_mut(),
            };
            unsafe {
                *filter = ColorFilterC {
                    enabled: f.enabled as c_int,
                    filter_type,
                    intensity: f.intensity.unwrap_or_default(),
                    has_intensity: f.intensity.is_some() as c_int,
                }
            };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Enables or disables the accessibility color filter
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Whether the filter is on
/// * [`filter_type`] - The preset to use, e.g. `Protanopia`. Required when enabling,
///   ignored otherwise, and may be NULL when disabling.
/// * [`intensity`] - Filter strength, 0.0 to 1.0. Ignored unless `has_intensity` is set.
/// * [`has_intensity`] - Whether to send `intensity`
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All non-NULL pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_color_filter(
    handle: *mut ConfigurationServiceHandle,
    enabled: c_int,
    filter_type: *const c_char,
    intensity: c_float,
    has_intensity: c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let filter_type = if filter_type.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(filter_type) }.to_str() {
            Ok(s) => Some(s),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };
    let intensity = if has_intensity == 0 {
        None
    } else {
        Some(intensity)
    };

    let client = unsafe { &mut (*handle).0 };
    match run_sync_local(async {
        client
            .set_color_filter(enabled != 0, filter_type, intensity)
            .await
    }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads the dynamic-type size's name, e.g. `medium` or `large`
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`size`] - Pointer to store the name. Free with `idevice_string_free`.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_device_text_size(
    handle: *mut ConfigurationServiceHandle,
    size: *mut *mut c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || size.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_device_text_size().await }) {
        Ok(s) => match CString::new(s) {
            Ok(s) => {
                unsafe { *size = s.into_raw() };
                null_mut()
            }
            Err(_) => ffi_err!(IdeviceError::FfiInvalidString),
        },
        Err(e) => ffi_err!(e),
    }
}

/// Sets the dynamic-type size by name, e.g. `medium` or `large`
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`size`] - The size's name
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_device_text_size(
    handle: *mut ConfigurationServiceHandle,
    size: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || size.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let size = match unsafe { CStr::from_ptr(size) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let client = unsafe { &mut (*handle).0 };
    match run_sync_local(async { client.set_device_text_size(size).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads whether Reduce Motion is on
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Pointer to store the state
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_reduce_motion(
    handle: *mut ConfigurationServiceHandle,
    enabled: *mut c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() || enabled.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_reduce_motion().await }) {
        Ok(e) => {
            unsafe { *enabled = e as c_int };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Toggles Reduce Motion
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Whether to turn it on
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_reduce_motion(
    handle: *mut ConfigurationServiceHandle,
    enabled: c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_reduce_motion(enabled != 0).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads whether Reduce Transparency is on
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Pointer to store the state
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_reduce_transparency(
    handle: *mut ConfigurationServiceHandle,
    enabled: *mut c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() || enabled.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_reduce_transparency().await }) {
        Ok(e) => {
            unsafe { *enabled = e as c_int };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Toggles Reduce Transparency
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Whether to turn it on
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_reduce_transparency(
    handle: *mut ConfigurationServiceHandle,
    enabled: c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_reduce_transparency(enabled != 0).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Reads whether the layout-debug borders overlay is on
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Pointer to store the state
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_get_show_borders(
    handle: *mut ConfigurationServiceHandle,
    enabled: *mut c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() || enabled.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.get_show_borders().await }) {
        Ok(e) => {
            unsafe { *enabled = e as c_int };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Toggles the layout-debug borders overlay
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Whether to turn it on
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_show_borders(
    handle: *mut ConfigurationServiceHandle,
    enabled: c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_show_borders(enabled != 0).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Toggles Increase Contrast
///
/// The device offers no getter for this one.
///
/// # Arguments
/// * [`handle`] - The ConfigurationServiceClient handle
/// * [`enabled`] - Whether to turn it on
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_set_increase_contrast(
    handle: *mut ConfigurationServiceHandle,
    enabled: c_int,
) -> *mut IdeviceFfiError {
    if handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.set_increase_contrast(enabled != 0).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees a ConfigurationServiceClient handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn configuration_service_free(handle: *mut ConfigurationServiceHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
