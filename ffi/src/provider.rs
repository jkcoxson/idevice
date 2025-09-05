// Jackson Coxson

use idevice::provider::{IdeviceProvider, TcpProvider, UsbmuxdProvider};
use std::net::IpAddr;
use std::os::raw::c_char;
use std::{ffi::CStr, ptr::null_mut};

use crate::util::{SockAddr, idevice_sockaddr};
use crate::{IdeviceFfiError, ffi_err, usbmuxd::UsbmuxdAddrHandle, util};
use crate::{IdevicePairingFile, RUNTIME};

pub struct IdeviceProviderHandle(pub Box<dyn IdeviceProvider>);

/// Creates a TCP provider for idevice
///
/// # Arguments
/// * [`ip`] - The sockaddr IP to connect to
/// * [`pairing_file`] - The pairing file handle to use
/// * [`label`] - The label to use with the connection
/// * [`provider`] - A pointer to a newly allocated provider
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `ip` must be a valid sockaddr
/// `pairing_file` is consumed must never be used again
/// `label` must be a valid Cstr
/// `provider` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_tcp_provider_new(
    ip: *const idevice_sockaddr,
    pairing_file: *mut crate::pairing_file::IdevicePairingFile,
    label: *const c_char,
    provider: *mut *mut IdeviceProviderHandle,
) -> *mut IdeviceFfiError {
    let ip = ip as *const SockAddr;
    let addr: IpAddr = match util::c_addr_to_rust(ip) {
        Ok(i) => i,
        Err(e) => return ffi_err!(e),
    };

    let label = match unsafe { CStr::from_ptr(label).to_str() } {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    // consume the pairing file on success
    let pairing_file = unsafe { Box::from_raw(pairing_file) };

    let t = TcpProvider {
        addr,
        pairing_file: pairing_file.0,
        label,
    };

    let boxed = Box::new(IdeviceProviderHandle(Box::new(t)));
    unsafe { *provider = Box::into_raw(boxed) };
    std::ptr::null_mut()
}

/// Frees an IdeviceProvider handle
///
/// # Arguments
/// * [`provider`] - The provider handle to free
///
/// # Safety
/// `provider` must be a valid pointer to a IdeviceProvider handle that was allocated this library
///  or NULL (in which case this function does nothing)
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_provider_free(provider: *mut IdeviceProviderHandle) {
    if !provider.is_null() {
        log::debug!("Freeing provider");
        unsafe { drop(Box::from_raw(provider)) };
    }
}

/// Creates a usbmuxd provider for idevice
///
/// # Arguments
/// * [`addr`] - The UsbmuxdAddr handle to connect to
/// * [`tag`] - The tag returned in usbmuxd responses
/// * [`udid`] - The UDID of the device to connect to
/// * [`device_id`] - The muxer ID of the device to connect to
/// * [`label`] - The label to use with the connection
/// * [`provider`] - A pointer to a newly allocated provider
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `addr` must be a valid pointer to UsbmuxdAddrHandle created by this library, and never used again
/// `udid` must be a valid CStr
/// `label` must be a valid Cstr
/// `provider` must be a valid, non-null pointer to a location where the handle will be stored
#[unsafe(no_mangle)]
pub unsafe extern "C" fn usbmuxd_provider_new(
    addr: *mut UsbmuxdAddrHandle,
    tag: u32,
    udid: *const c_char,
    device_id: u32,
    label: *const c_char,
    provider: *mut *mut IdeviceProviderHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || udid.is_null() || label.is_null() || provider.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let udid = match unsafe { CStr::from_ptr(udid) }.to_str() {
        Ok(u) => u.to_string(),
        Err(e) => {
            log::error!("Invalid UDID string: {e:?}");
            return ffi_err!(IdeviceError::FfiInvalidString);
        }
    };

    let label = match unsafe { CStr::from_ptr(label) }.to_str() {
        Ok(l) => l.to_string(),
        Err(e) => {
            log::error!("Invalid label string: {e:?}");
            return ffi_err!(IdeviceError::FfiInvalidArg);
        }
    };

    let addr = unsafe { Box::from_raw(addr) }.0;

    let p = UsbmuxdProvider {
        addr,
        tag,
        udid,
        device_id,
        label,
    };

    let boxed = Box::new(IdeviceProviderHandle(Box::new(p)));
    unsafe { *provider = Box::into_raw(boxed) };

    null_mut()
}

/// Gets the pairing file for the device
///
/// # Arguments
/// * [`provider`] - A pointer to the provider
/// * [`pairing_file`] - A pointer to the newly allocated pairing file
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// `provider` must be a valid, non-null pointer to the provider
#[unsafe(no_mangle)]
pub unsafe extern "C" fn idevice_provider_get_pairing_file(
    provider: *mut IdeviceProviderHandle,
    pairing_file: *mut *mut IdevicePairingFile,
) -> *mut IdeviceFfiError {
    let provider = unsafe { &mut *provider };

    let res = RUNTIME.block_on(async move { provider.0.get_pairing_file().await });
    match res {
        Ok(pf) => {
            let pf = Box::new(IdevicePairingFile(pf));
            unsafe { *pairing_file = Box::into_raw(pf) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
