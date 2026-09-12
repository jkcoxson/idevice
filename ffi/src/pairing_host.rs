//! FFI helpers for callers that own pairable-host networking.
//!
//! This exposes the lower-level pieces of device-initiated remote pairing without
//! forcing the Rust library to bind sockets or publish mDNS. A caller can generate
//! the host identity and TXT records, publish `_remotepairing-pairable-host._tcp`
//! with any platform Bonjour API, accept a device TCP connection, and hand that
//! connected socket to `pairable_host_accept_fd`.
//!
//! Based on AltStore's pairable-host FFI shape from
//! altstoreio/idevice@54cfbc7088c711a8b5b6fa7cc666b03, with the policy choices
//! made explicit for library consumers.

use std::ffi::{CStr, CString, c_char, c_void};
#[cfg(unix)]
use std::os::fd::FromRawFd;
use std::ptr::null_mut;

use idevice::IdeviceError;
use idevice::remote_pairing::{PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket};

use crate::rp_pairing_file::{RpPairingFileHandle, RpPairingPeerDeviceC, peer_device_to_c};
use crate::{IdeviceFfiError, ffi_err, run_sync_local};

/// Called when the device issues a setup PIN, so the caller can surface it to the
/// user. May be NULL.
pub type PairableHostPinCb = Option<extern "C" fn(pin: *const c_char, context: *mut c_void)>;

/// Opaque handle holding a generated host identity between
/// `pairable_host_prepare` and `pairable_host_accept_fd`.
pub struct PairableHostHandle {
    pairing_file: RpPairingFile,
    host_info: PairableHostInfo,
}

/// Wrapper so the raw PIN-callback context pointer can cross the async boundary.
/// The caller keeps it valid for the blocking duration of the handshake.
struct PinCtx(*mut c_void);
unsafe impl Send for PinCtx {}

/// Generates a fresh host identity and returns the data a caller needs to publish
/// its own `_remotepairing-pairable-host._tcp` Bonjour service.
///
/// # Arguments
/// * `name` - human-readable name shown on the device.
/// * `model` - hardware model shown on the device. `NULL` defaults to `"Mac17,7"`.
/// * `allows_pinless_pairing` - if true, advertise pinless pairing and use the
///   all-zero setup code expected by that flow; if false, generate a random PIN.
/// * `out_handle` - receives the host handle; pass it to `pairable_host_accept_fd`
///   and free it with `pairable_host_free`.
/// * `out_service_id` - receives the Bonjour service instance name. Free with
///   `idevice_string_free`.
/// * `out_txt_data`/`out_txt_len` - receive an XML plist dictionary of the TXT
///   records to publish. Free with `idevice_data_free`.
/// * `out_host_alt_irk` - optional. If non-NULL, must point to a 16-byte buffer
///   that receives the generated host `altIRK`; persist it with the pairing file.
///
/// A fresh identity is generated on every call.
///
/// # Safety
/// `name` must be a valid null-terminated C string. `model` must be NULL or a
/// valid null-terminated C string. All required out-pointers must be valid and
/// non-null. `out_host_alt_irk` must be NULL or point to at least 16 writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_prepare(
    name: *const c_char,
    model: *const c_char,
    allows_pinless_pairing: bool,
    out_handle: *mut *mut PairableHostHandle,
    out_service_id: *mut *mut c_char,
    out_txt_data: *mut *mut u8,
    out_txt_len: *mut usize,
    out_host_alt_irk: *mut u8,
) -> *mut IdeviceFfiError {
    if name.is_null()
        || out_handle.is_null()
        || out_service_id.is_null()
        || out_txt_data.is_null()
        || out_txt_len.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let model = if model.is_null() {
        "Mac17,7"
    } else {
        match unsafe { CStr::from_ptr(model) }.to_str() {
            Ok(s) => s,
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let pairing_file = RpPairingFile::generate(name);
    let mut host_info = PairableHostInfo::generate(name, model);
    host_info.allows_pinless_pairing = allows_pinless_pairing;
    let service_id = pairing_file.identifier.clone();

    let mut dict = plist::Dictionary::new();
    for (key, value) in host_info.mdns_txt_records(&service_id) {
        dict.insert(key, plist::Value::String(value));
    }
    let mut txt = Vec::new();
    if let Err(e) = plist::to_writer_xml(&mut txt, &plist::Value::Dictionary(dict)) {
        return ffi_err!(IdeviceError::InternalError(format!(
            "failed to encode TXT records: {e}"
        )));
    }

    let service_id = match CString::new(service_id) {
        Ok(s) => s,
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };

    let handle = Box::new(PairableHostHandle {
        pairing_file,
        host_info,
    });

    let len = txt.len();
    let ptr = txt.as_mut_ptr();
    std::mem::forget(txt);

    if !out_host_alt_irk.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(handle.host_info.alt_irk.as_ptr(), out_host_alt_irk, 16);
        }
    }

    unsafe {
        *out_handle = Box::into_raw(handle);
        *out_service_id = service_id.into_raw();
        *out_txt_data = ptr;
        *out_txt_len = len;
    }
    null_mut()
}

/// Backwards-compatible alias for AltStore's original function name.
/// Prefer `pairable_host_prepare` for new callers.
///
/// # Safety
/// Same requirements as `pairable_host_prepare`, except `model` is required and
/// pinless pairing is disabled.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_new(
    name: *const c_char,
    model: *const c_char,
    out_handle: *mut *mut PairableHostHandle,
    out_service_id: *mut *mut c_char,
    out_txt_data: *mut *mut u8,
    out_txt_len: *mut usize,
) -> *mut IdeviceFfiError {
    if model.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }
    unsafe {
        pairable_host_prepare(
            name,
            model,
            false,
            out_handle,
            out_service_id,
            out_txt_data,
            out_txt_len,
            null_mut(),
        )
    }
}

/// Runs pair-setup against a device that has already connected to `fd`.
///
/// Blocks the calling thread until pairing succeeds or fails. The fd is duplicated
/// before use, so the caller keeps ownership of the original socket.
///
/// # Safety
/// `handle` must be a valid handle from `pairable_host_prepare` or
/// `pairable_host_new`. `fd` must be a valid connected TCP socket.
/// `out_pairing_file` must be valid and non-null. `out_peer_device` must be NULL
/// or a valid writable pointer. `pin_cb`/`ctx` must stay valid until this call returns.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_accept_fd(
    handle: *mut PairableHostHandle,
    fd: i32,
    pin_cb: PairableHostPinCb,
    ctx: *mut c_void,
    out_peer_device: *mut *mut RpPairingPeerDeviceC,
    out_pairing_file: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if handle.is_null() || out_pairing_file.is_null() || fd < 0 {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let handle = unsafe { &*handle };
    let host_info = handle.host_info.clone();
    let mut pairing_file = handle.pairing_file.clone();
    let pin_ctx = PinCtx(ctx);

    let res = run_sync_local(async move {
        let dup_fd = unsafe { libc::dup(fd) };
        if dup_fd < 0 {
            return Err(IdeviceError::Socket(std::io::Error::last_os_error()));
        }
        let std_stream = unsafe { std::net::TcpStream::from_raw_fd(dup_fd) };
        std_stream
            .set_nonblocking(true)
            .map_err(IdeviceError::Socket)?;
        let stream = tokio::net::TcpStream::from_std(std_stream).map_err(IdeviceError::Socket)?;

        let socket = RpPairingSocket::new_device(stream);
        let mut host = PairableHost::new(socket, host_info);
        let peer_device = host
            .accept(&mut pairing_file, move |pin| async move {
                if let (Some(cb), Ok(pin)) = (pin_cb, CString::new(pin)) {
                    cb(pin.as_ptr(), pin_ctx.0);
                }
            })
            .await?;

        Ok::<_, IdeviceError>((pairing_file, peer_device))
    });

    match res {
        Ok((pairing_file, peer_device)) => {
            if !out_peer_device.is_null() {
                unsafe {
                    *out_peer_device = Box::into_raw(Box::new(peer_device_to_c(&peer_device)))
                };
            }
            unsafe {
                *out_pairing_file = Box::into_raw(Box::new(RpPairingFileHandle(pairing_file)))
            };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Backwards-compatible alias for AltStore's original function name.
/// Prefer `pairable_host_accept_fd` for new callers.
///
/// # Safety
/// Same requirements as `pairable_host_accept_fd`.
#[cfg(unix)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_handshake(
    handle: *mut PairableHostHandle,
    fd: i32,
    pin_cb: PairableHostPinCb,
    ctx: *mut c_void,
    out_pairing_file: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    unsafe { pairable_host_accept_fd(handle, fd, pin_cb, ctx, null_mut(), out_pairing_file) }
}

/// Frees a `PairableHostHandle`.
///
/// # Safety
/// `handle` must be a handle from `pairable_host_prepare`/`pairable_host_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_free(handle: *mut PairableHostHandle) {
    if !handle.is_null() {
        let _ = unsafe { Box::from_raw(handle) };
    }
}
