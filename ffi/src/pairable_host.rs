//! FFI for device-initiated remote pairing (the "pairable host" / responder side).
//!
//! Starting with iOS 27 a device can initiate pairing to a computer instead of the
//! computer initiating pairing to the device. The computer advertises an
//! `_remotepairing-pairable-host._tcp` mDNS service; the device connects to the
//! advertised port and drives the rppairing conversation while this side acts as
//! the SRP server/accessory. We generate a setup PIN and hand it to the caller via
//! a callback; the user types it into the device.
//!
//! This mirrors the host-initiated FFI in [`crate::tunnel_provider`] and the
//! `pair_host` tool in `tools/src/pair_host.rs`.

use std::ffi::{CStr, CString, c_char, c_void};
use std::net::Ipv4Addr;
use std::ptr::null_mut;
use std::sync::Arc;
use std::time::Duration;

use idevice::IdeviceError;
use idevice::remote_pairing::{
    PAIRABLE_HOST_SERVICE_TYPE, PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket,
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tokio::sync::Semaphore;

use crate::rp_pairing_file::{RpPairingFileHandle, RpPairingPeerDeviceC, peer_device_to_c};
use crate::{IdeviceFfiError, ffi_err, run_sync_local};

/// Wrapper so the raw PIN-callback context pointer can cross the async boundary.
struct PinCtx(*mut c_void);
unsafe impl Send for PinCtx {}
unsafe impl Sync for PinCtx {}

/// Cancellation state shared between the blocking `pairable_host_accept` call and
/// whichever thread signals it.
///
/// A zero-permit semaphore is the whole mechanism: `acquire` never completes on its
/// own, and `close` wakes every waiter. Because acquiring a *closed* semaphore fails
/// immediately, a cancel signalled before the wait even starts is not lost.
struct CancelState {
    sem: Semaphore,
}

impl CancelState {
    fn new() -> Self {
        Self {
            sem: Semaphore::new(0),
        }
    }

    fn signal(&self) {
        self.sem.close();
    }

    fn is_canceled(&self) -> bool {
        self.sem.is_closed()
    }

    /// Resolves once (and only once) the token has been signalled.
    async fn canceled(&self) {
        let _ = self.sem.acquire().await;
    }
}

/// Opaque cancellation token for [`pairable_host_accept`].
///
/// Create one with `pairable_host_cancel_new`, hand it to `pairable_host_accept`,
/// and call `pairable_host_cancel_signal` from any other thread to abort the wait.
/// Free it with `pairable_host_cancel_free` once the accept has returned.
pub struct PairableHostCancel(Arc<CancelState>);

/// Creates a cancellation token for `pairable_host_accept`.
///
/// Returns NULL only if allocation fails. Free with `pairable_host_cancel_free`.
#[unsafe(no_mangle)]
pub extern "C" fn pairable_host_cancel_new() -> *mut PairableHostCancel {
    Box::into_raw(Box::new(PairableHostCancel(Arc::new(CancelState::new()))))
}

/// Signals a cancellation token, unblocking the `pairable_host_accept` it was passed
/// to. That call returns the `CanceledByUser` error.
///
/// Safe to call from any thread, before or during the accept, and safe to call more
/// than once. Cancelling a token that was never passed to an accept, or one whose
/// accept already returned, does nothing.
///
/// # Safety
/// `cancel` must be a pointer returned by `pairable_host_cancel_new` that has not yet
/// been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_cancel_signal(cancel: *const PairableHostCancel) {
    if cancel.is_null() {
        return;
    }
    unsafe { &*cancel }.0.signal();
}

/// Frees a cancellation token.
///
/// The in-flight accept holds its own reference to the shared state, so freeing the
/// token while an accept is still running is safe — it just means nothing can cancel
/// that accept any more.
///
/// # Safety
/// `cancel` must be a pointer returned by `pairable_host_cancel_new` or NULL, and must
/// not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_cancel_free(cancel: *mut PairableHostCancel) {
    if cancel.is_null() {
        return;
    }
    let _ = unsafe { Box::from_raw(cancel) };
}

/// Advertises this computer as a pairable host and accepts a single device-initiated
/// pairing.
///
/// This blocks the calling thread until a device discovers the advertised
/// `_remotepairing-pairable-host._tcp` service, connects, and the pairing either
/// completes or fails — or until `cancel` is signalled from another thread. While the
/// pairing is in progress `pin_callback` is invoked once with the 6-digit setup code
/// that the user must type into the device.
///
/// On success a freshly generated [`RpPairingFileHandle`] is written to
/// `out_pairing_file`; it carries this host's long-term keys plus the paired
/// device's `altIRK`. Persist it (and `out_host_alt_irk`, see below) so the device
/// keeps recognizing this host on future connections.
///
/// # Arguments
/// * `name` - human-readable name shown on the device (e.g. "Jackson's MacBook Pro").
/// * `model` - hardware model identifier shown on the device. `NULL` defaults to
///   `"Mac17,7"`. iOS treats the host as a computer, so keep this a Mac identifier.
/// * `port` - TCP port to listen on. `0` picks a free port.
/// * `pin_callback` - invoked with the setup PIN to display. May be `NULL`.
/// * `pin_context` - opaque pointer passed back to `pin_callback`.
/// * `cancel` - optional cancellation token from `pairable_host_cancel_new`. Signal it
///   from another thread to abort the wait (e.g. the user dismissed the pairing UI).
///   `NULL` means the call can only be ended by a device connecting. Without one there
///   is no way to stop advertising short of exiting the process.
/// * `out_host_alt_irk` - optional. If non-NULL, must point to a 16-byte buffer that
///   receives the host's generated `altIRK` (needed to re-advertise this host so an
///   already-paired device recognizes it). May be `NULL`.
/// * `out_peer_device` - optional. If non-NULL, receives the paired device's identity
///   (name, model, UDID, `altIRK`), which the caller must free with
///   `rppairing_peer_device_free`. May be `NULL`.
/// * `out_pairing_file` - receives the resulting pairing file on success.
///
/// # Safety
/// `name` must be a valid null-terminated C string. `model` must be NULL or a valid
/// null-terminated C string. `cancel` must be NULL or a live token from
/// `pairable_host_cancel_new`. `out_host_alt_irk` must be NULL or point to at least 16
/// writable bytes. `out_peer_device` must be NULL or a valid writable pointer.
/// `out_pairing_file` must be valid and non-null.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn pairable_host_accept(
    name: *const c_char,
    model: *const c_char,
    port: u16,
    pin_callback: Option<extern "C" fn(pin: *const c_char, context: *mut c_void)>,
    pin_context: *mut c_void,
    cancel: *const PairableHostCancel,
    out_host_alt_irk: *mut u8,
    out_peer_device: *mut *mut RpPairingPeerDeviceC,
    out_pairing_file: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if name.is_null() || out_pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let name = match unsafe { CStr::from_ptr(name) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let model = if model.is_null() {
        "Mac17,7".to_string()
    } else {
        match unsafe { CStr::from_ptr(model) }.to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
        }
    };

    let cancel = if cancel.is_null() {
        None
    } else {
        Some(unsafe { &*cancel }.0.clone())
    };

    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async move {
        if cancel.as_ref().is_some_and(|c| c.is_canceled()) {
            return Err(IdeviceError::CanceledByUser);
        }

        // Bind first so we can advertise the real port.
        let listener = tokio::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
            .await
            .map_err(|e| IdeviceError::InternalError(format!("bind: {e}")))?;
        let port = listener
            .local_addr()
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?
            .port();

        let mut pairing_file = RpPairingFile::generate(&name);
        let host_info = PairableHostInfo::generate(&name, &model);
        let host_alt_irk = host_info.alt_irk;
        let service_identifier = pairing_file.identifier.clone();

        // Advertise the pairable-host mDNS service so the device can find us.
        let mdns = ServiceDaemon::new()
            .map_err(|e| IdeviceError::InternalError(format!("mDNS daemon: {e}")))?;
        // Apple's instance names exceed the default cap.
        let _ = mdns.set_service_name_len_max(30);
        let hostname = format!("idevice-{}.local.", &service_identifier[..8]);
        let txt = host_info.mdns_txt_records(&service_identifier);
        let properties: Vec<(&str, &str)> =
            txt.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let service_info = ServiceInfo::new(
            PAIRABLE_HOST_SERVICE_TYPE,
            &service_identifier,
            &hostname,
            "",
            port,
            &properties[..],
        )
        .map_err(|e| IdeviceError::InternalError(format!("mDNS service info: {e}")))?
        .enable_addr_auto();
        mdns.register(service_info)
            .map_err(|e| IdeviceError::InternalError(format!("mDNS register: {e}")))?;

        let pair = async {
            // Wait for a device to connect and start pairing.
            let (stream, _peer) = listener
                .accept()
                .await
                .map_err(|e| IdeviceError::InternalError(format!("accept: {e}")))?;

            let socket = RpPairingSocket::new_device(stream);
            let mut host = PairableHost::new(socket, host_info);

            let peer_device = host
                .accept(&mut pairing_file, |pin| async move {
                    if let Some(cb) = pin_callback
                        && let Ok(cpin) = CString::new(pin)
                    {
                        cb(cpin.as_ptr(), ctx.0);
                    }
                })
                .await?;

            Ok::<_, IdeviceError>(peer_device)
        };

        let result = match cancel {
            Some(c) => {
                tokio::select! {
                    biased;
                    _ = c.canceled() => Err(IdeviceError::CanceledByUser),
                    r = pair => r,
                }
            }
            None => pair.await,
        };

        if let Ok(rx) = mdns.shutdown() {
            let _ =
                tokio::task::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(2))).await;
        }

        result.map(|peer_device| (pairing_file, host_alt_irk, peer_device))
    });

    match res {
        Ok((rpf, host_alt_irk, peer_device)) => {
            if !out_host_alt_irk.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(host_alt_irk.as_ptr(), out_host_alt_irk, 16);
                }
            }
            if !out_peer_device.is_null() {
                unsafe {
                    *out_peer_device = Box::into_raw(Box::new(peer_device_to_c(&peer_device)))
                };
            }
            unsafe { *out_pairing_file = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
