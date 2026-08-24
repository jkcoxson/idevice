// Jackson Coxson
//! FFI bindings for the `remotepairingdeviced` control channel, reached over
//! USB through the `com.apple.dt.remotepairingdeviced.lockdown` lockdown
//! service.

use std::ffi::{CStr, c_char};
use std::ptr::null_mut;

use idevice::provider::IdeviceProvider;
use idevice::remote_pairing::{RemotePairingClient, RemotePairingLockdownService, RpPairingSocket};
use idevice::{IdeviceService, ReadWrite};
use plist_ffi::{PlistWrapper, plist_t};

use crate::rp_pairing_file::RpPairingFileHandle;
use crate::{IdeviceFfiError, IdeviceHandle, ffi_err, provider::IdeviceProviderHandle, run_sync};

/// The PIN used when the caller passes none. The USB transport is already
/// trusted, so the device pairs promptlessly and never asks for one.
const DEFAULT_PIN: &str = "000000";

/// Opaque handle to a remote pairing client speaking `RPPairing` over lockdown
pub struct RemotePairingLockdownHandle(
    pub RemotePairingClient<RpPairingSocket<Box<dyn ReadWrite>>>,
);

/// Connects to `remotepairingdeviced` over lockdown
///
/// # Arguments
/// * [`provider`] - An IdeviceProvider
/// * [`sending_host`] - The name this computer identifies itself by, the same
///   value the wireless flow uses
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_connect(
    provider: *mut IdeviceProviderHandle,
    sending_host: *const c_char,
    handle: *mut *mut RemotePairingLockdownHandle,
) -> *mut IdeviceFfiError {
    if provider.is_null() || sending_host.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let sending_host = match unsafe { CStr::from_ptr(sending_host) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let res = crate::run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*provider).0 };
        RemotePairingLockdownService::connect(provider_ref).await
    });

    let service = match res {
        Ok(service) => service,
        Err(e) => return ffi_err!(e),
    };
    match service.into_client(sending_host) {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(RemotePairingLockdownHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Wraps an existing lockdown connection to `remotepairingdeviced`
///
/// # Arguments
/// * [`socket`] - A connection to `com.apple.dt.remotepairingdeviced.lockdown`.
///   Consumed regardless of the result.
/// * [`sending_host`] - The name this computer identifies itself by
/// * [`handle`] - Pointer to store the newly created handle
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_new(
    socket: *mut IdeviceHandle,
    sending_host: *const c_char,
    handle: *mut *mut RemotePairingLockdownHandle,
) -> *mut IdeviceFfiError {
    if socket.is_null() || sending_host.is_null() || handle.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let sending_host = match unsafe { CStr::from_ptr(sending_host) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let socket = unsafe { Box::from_raw(socket) }.0;
    match RemotePairingLockdownService::new(socket).into_client(sending_host) {
        Ok(client) => {
            unsafe { *handle = Box::into_raw(Box::new(RemotePairingLockdownHandle(client))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Runs the control channel's handshake and returns what the device reports
/// about itself
///
/// # Arguments
/// * [`handle`] - The client handle
/// * [`handshake`] - Pointer to store the device's response
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_attempt_pair_verify(
    handle: *mut RemotePairingLockdownHandle,
    handshake: *mut plist_t,
) -> *mut IdeviceFfiError {
    if handle.is_null() || handshake.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    match run_sync(async move { client.attempt_pair_verify().await }) {
        Ok(res) => {
            unsafe { *handshake = PlistWrapper::new_node(res).into_ptr() };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Checks whether the device still recognizes a pairing record
///
/// The handshake must have run first, i.e.
/// `remote_pairing_lockdown_attempt_pair_verify`.
///
/// # Arguments
/// * [`handle`] - The client handle
/// * [`pairing_file`] - The RPPairing file to validate
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_validate_pairing(
    handle: *mut RemotePairingLockdownHandle,
    pairing_file: *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() || pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let client = unsafe { &mut (*handle).0 };
    let pairing_file = unsafe { &mut (*pairing_file).0 };
    match run_sync(async move { client.validate_pairing(pairing_file).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Pairs with the device, saving the record into `pairing_file`
///
/// # Arguments
/// * [`handle`] - The client handle
/// * [`pairing_file`] - The RPPairing file to pair with, e.g. a fresh one from
///   `rp_pairing_file_generate`. Updated in place on success, so write it out
///   afterwards to keep the pairing.
/// * [`pin`] - The PIN to answer a Trust prompt with, or NULL for `000000`.
///   Pairing over USB is promptless, so the device should never ask.
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All non-NULL pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_pair(
    handle: *mut RemotePairingLockdownHandle,
    pairing_file: *mut RpPairingFileHandle,
    pin: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let pin = if pin.is_null() {
        DEFAULT_PIN.to_string()
    } else {
        match unsafe { CStr::from_ptr(pin) }.to_str() {
            Ok(p) => p.to_string(),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let client = unsafe { &mut (*handle).0 };
    let pairing_file = unsafe { &mut (*pairing_file).0 };
    match run_sync(async move { client.pair(pairing_file, || async { pin.clone() }).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// Pairs only if the device doesn't already recognize the pairing record
///
/// Runs the handshake, validates `pairing_file`, and pairs when that fails.
///
/// # Arguments
/// * [`handle`] - The client handle
/// * [`pairing_file`] - The RPPairing file to validate or pair with. Updated in
///   place when pairing happens, so write it out afterwards.
/// * [`pin`] - The PIN to answer a Trust prompt with, or NULL for `000000`
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All non-NULL pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_connect_pairing(
    handle: *mut RemotePairingLockdownHandle,
    pairing_file: *mut RpPairingFileHandle,
    pin: *const c_char,
) -> *mut IdeviceFfiError {
    if handle.is_null() || pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let pin = if pin.is_null() {
        DEFAULT_PIN.to_string()
    } else {
        match unsafe { CStr::from_ptr(pin) }.to_str() {
            Ok(p) => p.to_string(),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let client = unsafe { &mut (*handle).0 };
    let pairing_file = unsafe { &mut (*pairing_file).0 };
    match run_sync(async move { client.connect(pairing_file, || async { pin.clone() }).await }) {
        Ok(()) => null_mut(),
        Err(e) => ffi_err!(e),
    }
}

/// The encryption key established during pairing, used as the TLS-PSK for
/// tunnel connections
///
/// # Arguments
/// * [`handle`] - The client handle
/// * [`key`] - Pointer to store the key, freed with `idevice_data_free`
/// * [`key_len`] - Pointer to store the number of bytes
///
/// # Returns
/// An IdeviceFfiError on error, null on success
///
/// # Safety
/// All pointer parameters must be valid
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_encryption_key(
    handle: *mut RemotePairingLockdownHandle,
    key: *mut *mut u8,
    key_len: *mut usize,
) -> *mut IdeviceFfiError {
    if handle.is_null() || key.is_null() || key_len.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let mut data = unsafe { &(*handle).0 }
        .encryption_key()
        .to_vec()
        .into_boxed_slice();
    unsafe {
        *key_len = data.len();
        *key = data.as_mut_ptr();
    }
    std::mem::forget(data);
    null_mut()
}

/// Frees a remote pairing lockdown handle
///
/// # Safety
/// `handle` must be a valid pointer to a handle allocated by this library or NULL
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remote_pairing_lockdown_free(handle: *mut RemotePairingLockdownHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
