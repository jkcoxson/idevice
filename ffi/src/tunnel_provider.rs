//! FFI functions for creating tunnels to iOS/tvOS/visionOS devices.
//!
//! These produce an `AdapterHandle` + `RsdHandshakeHandle` — the same types
//! used by every `_connect_rsd` function (e.g. `debug_proxy_connect_rsd`).
//!
//! Three paths:
//! - **USB via CoreDeviceProxy**: `tunnel_create_usb` / `tunnel_pair_usb`
//! - **Network via RemoteXPC** (NCM/USB Ethernet): `tunnel_create_remotexpc`
//! - **Network via raw RPPairing** (Wi-Fi/LAN): `tunnel_create_rppairing`

use std::ffi::{CStr, CString, c_char, c_void};
use std::ptr::null_mut;

use idevice::RemoteXpcClient;
use idevice::remote_pairing::{PeerDevice, RemotePairingClient, RpPairingSocket};
use idevice::{
    IdeviceError, IdeviceService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    rsd::RsdHandshake,
};

use crate::core_device_proxy::AdapterHandle;
use crate::rp_pairing_file::RpPairingFileHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::run_global_timeout;
use crate::util::{SockAddr, idevice_sockaddr, idevice_socklen_t};
use crate::{IdeviceFfiError, ffi_err, provider::IdeviceProviderHandle, run_sync_local};

struct PinCtx(*mut c_void);
unsafe impl Send for PinCtx {}
unsafe impl Sync for PinCtx {}

/// Shared logic: given a connected & paired `RemotePairingClient`, create
/// the TLS-PSK tunnel and return adapter + handshake.
async fn finish_tunnel(
    rpc: &mut idevice::remote_pairing::RemotePairingClient<
        impl idevice::remote_pairing::RpPairingSocketProvider,
    >,
    connect_addr: std::net::SocketAddr,
) -> Result<(idevice::tcp::handle::AdapterHandle, RsdHandshake), IdeviceError> {
    use idevice::remote_pairing::connect_tls_psk_tunnel_native;

    let tunnel_port = rpc.create_tcp_listener().await?;
    let mut tunnel_addr = connect_addr;
    tunnel_addr.set_port(tunnel_port);
    let tunnel_stream = run_global_timeout(|| tokio::net::TcpStream::connect(tunnel_addr))
        .await
        .map_err(|e| IdeviceError::InternalError(format!("TLS tunnel: {e}")))?;
    let tunnel = connect_tls_psk_tunnel_native(tunnel_stream, rpc.encryption_key()).await?;

    let client_ip: std::net::IpAddr = tunnel
        .info
        .client_address
        .parse()
        .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
    let server_ip: std::net::IpAddr = tunnel
        .info
        .server_address
        .parse()
        .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
    let mtu = tunnel.info.mtu as usize;
    let rsd_port = tunnel.info.server_rsd_port;

    let raw = tunnel.into_inner();
    let mut adapter = idevice::tcp::adapter::Adapter::new(Box::new(raw), client_ip, server_ip);
    adapter.set_mss(mtu.saturating_sub(60));
    let mut adapter = adapter.to_async_handle();

    let rsd_stream = adapter
        .connect(rsd_port)
        .await
        .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
    let handshake = RsdHandshake::new(rsd_stream).await?;

    Ok((adapter, handshake))
}

fn write_result(
    adapter: idevice::tcp::handle::AdapterHandle,
    handshake: RsdHandshake,
    out_adapter: *mut *mut AdapterHandle,
    out_handshake: *mut *mut RsdHandshakeHandle,
) {
    unsafe {
        *out_adapter = Box::into_raw(Box::new(AdapterHandle(adapter)));
        *out_handshake = Box::into_raw(Box::new(RsdHandshakeHandle(handshake)));
    }
}

/// Creates a tunnel over USB via CoreDeviceProxy.
/// No need to stop remoted.
///
/// # Safety
/// All pointer arguments must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tunnel_create_usb(
    lockdown_provider: *mut IdeviceProviderHandle,
    out_adapter: *mut *mut AdapterHandle,
    out_handshake: *mut *mut RsdHandshakeHandle,
) -> *mut IdeviceFfiError {
    if lockdown_provider.is_null() || out_adapter.is_null() || out_handshake.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let res = run_sync_local(async {
        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*lockdown_provider).0 };
        let proxy = CoreDeviceProxy::connect(provider_ref).await?;
        let rsd_port = proxy.tunnel_info().server_rsd_port;
        let adapter = proxy
            .create_software_tunnel()
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let mut adapter = adapter.to_async_handle();
        let rsd_stream = adapter
            .connect(rsd_port)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let handshake = RsdHandshake::new(rsd_stream).await?;
        Ok::<_, IdeviceError>((adapter, handshake))
    });

    match res {
        Ok((adapter, handshake)) => {
            write_result(adapter, handshake, out_adapter, out_handshake);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Pairs via USB CoreDeviceProxy tunnel (no SIGSTOP needed).
///
/// For iOS, `pin_callback` can be NULL (defaults to "000000").
/// For Apple TV / Vision Pro, provide a callback returning the on-screen PIN.
///
/// # Safety
/// All pointer arguments must be valid and non-null (except `pin_callback`/`pin_context`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tunnel_pair_usb(
    lockdown_provider: *mut IdeviceProviderHandle,
    hostname: *const c_char,
    pin_callback: Option<extern "C" fn(context: *mut c_void) -> *const c_char>,
    pin_context: *mut c_void,
    out_pairing_file: *mut *mut RpPairingFileHandle,
) -> *mut IdeviceFfiError {
    if lockdown_provider.is_null() || hostname.is_null() || out_pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let host = match unsafe { CStr::from_ptr(hostname) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async {
        use idevice::RemoteXpcClient;
        use idevice::remote_pairing::{RemotePairingClient, RpPairingFile};

        let provider_ref: &dyn IdeviceProvider = unsafe { &*(*lockdown_provider).0 };

        let proxy = CoreDeviceProxy::connect(provider_ref).await?;
        let rsd_port = proxy.tunnel_info().server_rsd_port;
        let adapter = proxy
            .create_software_tunnel()
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let mut adapter = adapter.to_async_handle();

        let rsd_stream = adapter
            .connect(rsd_port)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let handshake = RsdHandshake::new(rsd_stream).await?;

        let ts = handshake
            .services
            .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
            .ok_or(IdeviceError::ServiceNotFound)?;

        let ts_stream = adapter
            .connect(ts.port)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let mut conn = RemoteXpcClient::new(ts_stream).await?;
        conn.do_handshake().await?;
        let _ = conn.recv_root().await?;

        let mut rpf = RpPairingFile::generate(&host);
        let mut rpc = RemotePairingClient::new(conn, &host);
        rpc.connect(&mut rpf, async || get_pin(pin_callback, &ctx))
            .await?;

        Ok::<_, IdeviceError>(rpf)
    });

    match res {
        Ok(rpf) => {
            unsafe { *out_pairing_file = Box::into_raw(Box::new(RpPairingFileHandle(rpf))) };
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a tunnel over the network via RemoteXPC.
///
/// Use this when connecting to a device discovered via `_remoted._tcp` (RSD port).
/// The connection goes: RSD → find tunnel service → RemoteXPC → RPPairing → tunnel.
///
/// # Safety
/// All pointer arguments must be valid and non-null (except `pin_callback`/`pin_context`).
/// `pairing_file` is borrowed, not consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tunnel_create_remotexpc(
    addr: *const idevice_sockaddr,
    addr_len: idevice_socklen_t,
    hostname: *const c_char,
    pairing_file: *mut RpPairingFileHandle,
    pin_callback: Option<extern "C" fn(context: *mut c_void) -> *const c_char>,
    pin_context: *mut c_void,
    out_adapter: *mut *mut AdapterHandle,
    out_handshake: *mut *mut RsdHandshakeHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null()
        || hostname.is_null()
        || pairing_file.is_null()
        || out_adapter.is_null()
        || out_handshake.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket_addr = match crate::util::c_socket_to_rust(addr as *const SockAddr, addr_len) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };
    let host = match unsafe { CStr::from_ptr(hostname) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let rpf = unsafe { &mut (*pairing_file).0 };
    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async {
        // RSD handshake to discover tunnel service
        let rsd_stream = run_global_timeout(|| tokio::net::TcpStream::connect(socket_addr))
            .await
            .map_err(|e| IdeviceError::InternalError(format!("RSD connect: {e}")))?;
        let rsd_handshake = RsdHandshake::new(rsd_stream).await?;

        let ts = rsd_handshake
            .services
            .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
            .ok_or(IdeviceError::ServiceNotFound)?;

        // Connect to tunnel service via RemoteXPC
        let ts_addr = std::net::SocketAddr::new(socket_addr.ip(), ts.port);
        let ts_stream = run_global_timeout(|| tokio::net::TcpStream::connect(ts_addr))
            .await
            .map_err(|e| IdeviceError::InternalError(format!("tunnel service: {e}")))?;
        let mut conn = RemoteXpcClient::new(ts_stream).await?;
        conn.do_handshake().await?;
        let _ = conn.recv_root().await?;

        // RPPairing over RemoteXPC
        let mut rpc = RemotePairingClient::new(conn, &host);
        rpc.connect(rpf, async || get_pin(pin_callback, &ctx))
            .await?;

        finish_tunnel(&mut rpc, socket_addr).await
    });

    match res {
        Ok((adapter, handshake)) => {
            write_result(adapter, handshake, out_adapter, out_handshake);
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Creates a tunnel over the network via raw RPPairing protocol.
///
/// Use this when connecting to a device discovered via `_remotepairing._tcp`.
/// The connection goes: direct TCP → RPPairing (JSON) → tunnel.
///
/// `pairing_file` is used for pair-verify. If verification fails (typically
/// because the device has never been paired with this host) a full pair-setup
/// runs on the same connection and `pairing_file` is updated in place, so the
/// caller should persist it afterwards regardless of whether it was freshly
/// generated.
///
///
/// # Safety
/// All pointer arguments must be valid and non-null (except `pin_callback`/`pin_context`).
/// `pairing_file` is borrowed, not consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tunnel_create_rppairing(
    addr: *const idevice_sockaddr,
    addr_len: idevice_socklen_t,
    hostname: *const c_char,
    pairing_file: *mut RpPairingFileHandle,
    pin_callback: Option<extern "C" fn(context: *mut c_void) -> *const c_char>,
    pin_context: *mut c_void,
    out_adapter: *mut *mut AdapterHandle,
    out_handshake: *mut *mut RsdHandshakeHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null()
        || hostname.is_null()
        || pairing_file.is_null()
        || out_adapter.is_null()
        || out_handshake.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket_addr = match crate::util::c_socket_to_rust(addr as *const SockAddr, addr_len) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };
    let host = match unsafe { CStr::from_ptr(hostname) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let rpf = unsafe { &mut (*pairing_file).0 };
    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async {
        // Connect directly and use raw RPPairing protocol
        let stream = run_global_timeout(|| tokio::net::TcpStream::connect(socket_addr))
            .await
            .map_err(|e| IdeviceError::InternalError(format!("connect: {e}")))?;
        let conn = RpPairingSocket::new(stream);

        let mut rpc = RemotePairingClient::new(conn, &host);
        rpc.connect(rpf, async || get_pin(pin_callback, &ctx))
            .await?;

        finish_tunnel(&mut rpc, socket_addr).await
    });

    match res {
        Ok((adapter, handshake)) => {
            write_result(adapter, handshake, out_adapter, out_handshake);
            null_mut()
        }
        Err(e) => ffi_err!(e),
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

fn peer_device_to_c(p: &PeerDevice) -> RpPairingPeerDeviceC {
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
/// `peer_device` must be a pointer returned by `rppairing_pair_network` or NULL.
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

/// Pairs with a device over the network via raw RPPairing, without creating a tunnel.
///
/// This is for tvOS.
///
/// On iOS `tunnel_create_rppairing` handles both halves on its own; this function
/// is only needed there if you want to pair and connect as separate steps.
///
/// # Arguments
/// * `addr` / `addr_len` - address of the pairing service to connect to.
/// * `hostname` - name this host presents to the device.
/// * `pairing_file` - borrowed, not consumed. Updated in place on success. Pass a
///   freshly generated file (`rp_pairing_file_generate`) for a first-time pairing.
/// * `pin_callback` / `pin_context` - invoked to obtain the PIN shown on the
///   device. May be `NULL`.
/// * `out_peer_device` - optional. If non-NULL, receives the paired device's
///   identity, which the caller must free with `rppairing_peer_device_free`. Only
///   written when a pair-setup actually ran; a successful pair-verify leaves it
///   NULL.
///
/// # Safety
/// All pointer arguments must be valid and non-null except `pin_callback`,
/// `pin_context`, and `out_peer_device`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rppairing_pair_network(
    addr: *const idevice_sockaddr,
    addr_len: idevice_socklen_t,
    hostname: *const c_char,
    pairing_file: *mut RpPairingFileHandle,
    pin_callback: Option<extern "C" fn(context: *mut c_void) -> *const c_char>,
    pin_context: *mut c_void,
    out_peer_device: *mut *mut RpPairingPeerDeviceC,
) -> *mut IdeviceFfiError {
    if addr.is_null() || hostname.is_null() || pairing_file.is_null() {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket_addr = match crate::util::c_socket_to_rust(addr as *const SockAddr, addr_len) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };
    let host = match unsafe { CStr::from_ptr(hostname) }.to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return ffi_err!(IdeviceError::FfiInvalidString),
    };
    let rpf = unsafe { &mut (*pairing_file).0 };
    let ctx = PinCtx(pin_context);

    let res = run_sync_local(async {
        let stream = run_global_timeout(|| tokio::net::TcpStream::connect(socket_addr))
            .await
            .map_err(|e| IdeviceError::InternalError(format!("connect: {e}")))?;
        let conn = RpPairingSocket::new(stream);

        let mut rpc = RemotePairingClient::new(conn, &host);
        rpc.connect(rpf, async || get_pin(pin_callback, &ctx))
            .await?;

        // Only present when a pair-setup ran; a successful pair-verify has none.
        Ok::<_, IdeviceError>(rpc.paired_peer_device().ok().map(peer_device_to_c))
    });

    match res {
        Ok(peer_device) => {
            if !out_peer_device.is_null() {
                unsafe {
                    *out_peer_device = match peer_device {
                        Some(p) => Box::into_raw(Box::new(p)),
                        None => null_mut(),
                    }
                };
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

fn get_pin(cb: Option<extern "C" fn(*mut c_void) -> *const c_char>, ctx: &PinCtx) -> String {
    if let Some(cb) = cb {
        let ptr = cb(ctx.0);
        if !ptr.is_null()
            && let Ok(s) = unsafe { CStr::from_ptr(ptr) }.to_str()
        {
            return s.to_string();
        }
    }
    "000000".to_string()
}
