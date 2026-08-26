// Jackson Coxson
//! FFI bindings for RPPairing files (Ed25519-based remote pairing credentials).

use std::{
    ffi::{CStr, CString, c_char},
    ptr::null_mut,
};

use idevice::remote_pairing::{PeerDevice, RpPairingFile};

use crate::{IdeviceFfiError, ffi_err, run_sync_local};

/// Opaque handle to an RPPairing file
pub struct RpPairingFileHandle(pub RpPairingFile);

/// Generates a new RPPairing file with fresh Ed25519 keys.
///
/// # Safety
/// `hostname` must be a valid null-terminated C string.
/// `out` must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_generate(
    hostname: *const c_char,
    out: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if hostname.is_null() || out.is_null() {
        return ffi_err!(idevice::IdeviceError::FfiInvalidArg);
    }

    let host = match unsafe { CStr::from_ptr(hostname) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(idevice::IdeviceError::FfiInvalidString),
    };

    let rpf = RpPairingFile::generate(host);
    unsafe { *out = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
    null_mut()
}

/// Reads an RPPairing file from a path.
///
/// # Safety
/// `path` must be a valid null-terminated C string.
/// `out` must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_read(
    path: *const c_char,
    out: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if path.is_null() || out.is_null() {
        return ffi_err!(idevice::IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(idevice::IdeviceError::FfiInvalidString),
    };

    let res = run_sync_local(async { RpPairingFile::read_from_file(&path).await });

    match res {
        Ok(rpf) => {
            unsafe { *out = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Parses an RPPairing file from plist bytes (XML or binary).
///
/// # Safety
/// `data` must point to `len` valid bytes.
/// `out` must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_from_bytes(
    data: *const u8,
    len: usize,
    out: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if data.is_null() || out.is_null() {
        return ffi_err!(idevice::IdeviceError::FfiInvalidArg);
    }

    let bytes = unsafe { std::slice::from_raw_parts(data, len) };

    match RpPairingFile::from_bytes(bytes) {
        Ok(rpf) => {
            unsafe { *out = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Serializes an RPPairing file to XML plist bytes.
///
/// The caller must free the returned bytes with `idevice_data_free(data, len)`.
///
/// # Safety
/// `handle`, `out_data`, and `out_len` must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_to_bytes(
    handle: *mut RpPairingFileHandle,
    out_data: *mut *mut u8,
    out_len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || out_data.is_null() || out_len.is_null() {
        return ffi_err!(idevice::IdeviceError::FfiInvalidArg);
    }

    let rpf = unsafe { &(*handle).0 };
    let xml = rpf.to_bytes();

    let len = xml.len();
    let ptr = xml.as_ptr();
    std::mem::forget(xml);

    unsafe {
        *out_data = ptr as *mut u8;
        *out_len = len;
    }

    null_mut()
}

/// Writes an RPPairing file to a path.
///
/// # Safety
/// `handle` and `path` must be valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_write(
    handle: *mut RpPairingFileHandle,
    path: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || path.is_null() {
        return ffi_err!(idevice::IdeviceError::FfiInvalidArg);
    }

    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(idevice::IdeviceError::FfiInvalidString),
    };

    let rpf = unsafe { &(*handle).0 };
    let res = run_sync_local(async { rpf.write_to_file(&path).await });

    match res {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Frees an RPPairing file handle.
///
/// # Safety
/// `handle` must be valid or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rp_pairing_file_free(handle: *mut RpPairingFileHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}

/// The peer device identity learned during a successful pair-setup.
///
/// Free with `rppairing_peer_device_free`.
#[repr(C)]
pub struct RpPairingPeerDeviceC {
    /// Peer identifier, the same identifier a later `verifyManualPairing` returns.
    pub account_id: *mut c_char,
    /// The device's 16-byte `altIRK`, used to match its mDNS `authTag` records.
    pub alt_irk: [u8; 16],
    /// Hardware model identifier, e.g. "AppleTV14,1".
    pub model: *mut c_char,
    /// User-visible device name, e.g. "Living Room".
    pub name: *mut c_char,
    /// The device's UDID.
    pub udid: *mut c_char,
}

pub(crate) fn peer_device_to_c(p: &PeerDevice) -> RpPairingPeerDeviceC {
    let s = |v: &str| CString::new(v).unwrap_or_default().into_raw();
    let mut alt_irk = [0u8; 16];
    let n = p.alt_irk.len().min(16);
    alt_irk[..n].copy_from_slice(&p.alt_irk[..n]);
    RpPairingPeerDeviceC {
        account_id: s(&p.account_id),
        alt_irk,
        model: s(&p.model),
        name: s(&p.name),
        udid: s(&p.remotepairing_udid),
    }
}

/// Frees a peer device struct and its heap-allocated string fields.
///
/// # Safety
/// `peer_device` must be a pointer returned by `rppairing_pair_network` or
/// `pairable_host_accept`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rppairing_peer_device_free(peer_device: *mut RpPairingPeerDeviceC) {
    if peer_device.is_null() {
        return;
    }
    let p = unsafe { Box::from_raw(peer_device) };
    for field in [p.account_id, p.model, p.name, p.udid] {
        if !field.is_null() {
            let _ = unsafe { CString::from_raw(field) };
        }
    }
}
