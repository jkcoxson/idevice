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

use idevice::IdeviceError;
use idevice::remote_pairing::{
    PAIRABLE_HOST_SERVICE_TYPE, PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket,
};
use mdns_sd::{ServiceDaemon, ServiceInfo};

use crate::rp_pairing_file::RpPairingFileHandle;
use crate::{IdeviceFfiError, ffi_err, run_sync_local};

/// Wrapper so the raw PIN-callback context pointer can cross the async boundary.
struct PinCtx(*mut c_void);
unsafe impl Send for PinCtx {}
unsafe impl Sync for PinCtx {}

/// Advertises this computer as a pairable host and accepts a single device-initiated
/// pairing.
///
/// This blocks the calling thread until a device discovers the advertised
/// `_remotepairing-pairable-host._tcp` service, connects, and the pairing either
/// completes or fails. While the pairing is in progress `pin_callback` is invoked
/// once with the 6-digit setup code that the user must type into the device.
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
/// * `out_host_alt_irk` - optional. If non-NULL, must point to a 16-byte buffer that
///   receives the host's generated `altIRK` (needed to re-advertise this host so an
///   already-paired device recognizes it). May be `NULL`.
/// * `out_pairing_file` - receives the resulting pairing file on success.
///
/// # Safety
/// `name` must be a valid null-terminated C string. `model` must be NULL or a valid
/// null-terminated C string. `out_host_alt_irk` must be NULL or point to at least 16
/// writable bytes. `out_pairing_file` must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pairable_host_accept(
    name: *const c_char,
    model: *const c_char,
    port: u16,
    pin_callback: Option<extern "C" fn(pin: *const c_char, context: *mut c_void)>,
    pin_context: *mut c_void,
    out_host_alt_irk: *mut u8,
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

    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async move {
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

        // Wait for a device to connect and start pairing.
        let (stream, _peer) = listener
            .accept()
            .await
            .map_err(|e| IdeviceError::InternalError(format!("accept: {e}")))?;

        let socket = RpPairingSocket::new_device(stream);
        let mut host = PairableHost::new(socket, host_info);

        host.accept(&mut pairing_file, |pin| async move {
            if let Some(cb) = pin_callback
                && let Ok(cpin) = CString::new(pin)
            {
                cb(cpin.as_ptr(), ctx.0);
            }
        })
        .await?;

        Ok::<_, IdeviceError>((pairing_file, host_alt_irk))
    });

    match res {
        Ok((rpf, host_alt_irk)) => {
            if !out_host_alt_irk.is_null() {
                unsafe {
                    std::ptr::copy_nonoverlapping(host_alt_irk.as_ptr(), out_host_alt_irk, 16);
                }
            }
            unsafe { *out_pairing_file = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
