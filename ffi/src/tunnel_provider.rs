//! FFI functions for creating RSD tunnel connections.
//!
//! These produce an `AdapterHandle` + `RsdHandshakeHandle` — the same types
//! used by every `_connect_rsd` function (e.g. `debug_proxy_connect_rsd`).
//!
//! Two paths:
//! - **USB**: `rsd_tunnel_create_usb` — goes through CoreDeviceProxy, no SIGSTOP needed
//! - **Network**: `rsd_tunnel_create_network` - uses RPPairing over TLS-PSK
//!
//! Both also support creating an RPPairing file through the USB tunnel for
//! future wireless use.

use std::ffi::{CStr, c_char, c_void};
use std::ptr::null_mut;

use idevice::{
    IdeviceError, IdeviceService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    rsd::RsdHandshake,
};

use crate::core_device_proxy::AdapterHandle;
use crate::rp_pairing_file::RpPairingFileHandle;
use crate::rsd::RsdHandshakeHandle;
use crate::util::{SockAddr, idevice_sockaddr, idevice_socklen_t};
use crate::{IdeviceFfiError, ffi_err, provider::IdeviceProviderHandle, run_sync_local};

/// Creates an RSD tunnel over USB via CoreDeviceProxy.
///
/// Returns an `AdapterHandle` and `RsdHandshakeHandle` that can be passed
/// to any service's `_connect_rsd` function.
///
/// No need to stop remoted.
///
/// # Safety
/// All pointer arguments must be valid and non-null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_tunnel_create_usb(
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
            .map_err(|e| IdeviceError::InternalError(format!("software tunnel: {e}")))?;
        let mut adapter = adapter.to_async_handle();

        let rsd_stream = adapter
            .connect(rsd_port)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("RSD connect: {e}")))?;
        let handshake = RsdHandshake::new(rsd_stream).await?;

        Ok::<_, IdeviceError>((adapter, handshake))
    });

    match res {
        Ok((adapter, handshake)) => {
            unsafe {
                *out_adapter = Box::into_raw(Box::new(AdapterHandle(adapter)));
                *out_handshake = Box::into_raw(Box::new(RsdHandshakeHandle(handshake)));
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}

/// Pairs with a device via USB CoreDeviceProxy tunnel and saves an RPPairing file.
///
/// This goes through the USB tunnel to the untrusted tunnel service and performs
/// RPPairing so no SIGSTOP on remoted needed. The resulting pairing file can be
/// used for future wireless connections.
///
/// The user will need to tap "Trust" on the device.
///
/// For iOS devices, `pin_callback` can be NULL (defaults to "000000").
/// For Apple TV / Vision Pro, provide a callback that returns the PIN shown on
/// the device screen. The returned string must be null-terminated and remain
/// valid until the next call or until pairing completes.
///
/// # Safety
/// All pointer arguments must be valid and non-null (except `pin_callback`/`pin_context`).
/// `out_pairing_file` receives a newly allocated handle that must be freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_tunnel_pair_usb(
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

    // Wrap context for Send safety
    struct Ctx(*mut c_void);
    unsafe impl Send for Ctx {}
    unsafe impl Sync for Ctx {}
    let ctx = Ctx(pin_context);

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
        let mut rpc = RemotePairingClient::new(conn, &host, &mut rpf);
        rpc.connect(
            async |_| {
                if let Some(cb) = pin_callback {
                    let ptr = cb(ctx.0);
                    if !ptr.is_null() {
                        if let Ok(s) = unsafe { CStr::from_ptr(ptr) }.to_str() {
                            return s.to_string();
                        }
                    }
                }
                "000000".to_string()
            },
            0u8,
        )
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

/// Creates an RSD tunnel over the network using an existing RPPairing file.
///
/// Returns an `AdapterHandle` and `RsdHandshakeHandle` that can be passed
/// to any service's `_connect_rsd` function.
///
/// # Arguments
/// * `addr` - Socket address (IP + port) of the device's RSD service
/// * `addr_len` - Length of the socket address
/// * `pairing_file` - Borrowed RPPairing file handle
/// * `out_adapter` - Receives the adapter handle
/// * `out_handshake` - Receives the RSD handshake handle
///
/// # Safety
/// All pointer arguments must be valid and non-null (except `pin_callback`/`pin_context`).
/// `pairing_file` is borrowed, not consumed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn rsd_tunnel_create_network(
    addr: *const idevice_sockaddr,
    addr_len: idevice_socklen_t,
    pairing_file: *mut RpPairingFileHandle,
    pin_callback: Option<extern "C" fn(context: *mut c_void) -> *const c_char>,
    pin_context: *mut c_void,
    out_adapter: *mut *mut AdapterHandle,
    out_handshake: *mut *mut RsdHandshakeHandle,
) -> *mut IdeviceFfiError {
    if addr.is_null() || pairing_file.is_null() || out_adapter.is_null() || out_handshake.is_null()
    {
        return ffi_err!(IdeviceError::FfiInvalidArg);
    }

    let socket_addr = match crate::util::c_socket_to_rust(addr as *const SockAddr, addr_len) {
        Ok(a) => a,
        Err(e) => return ffi_err!(e),
    };

    let rpf = unsafe { &mut (*pairing_file).0 };

    struct Ctx(*mut c_void);
    unsafe impl Send for Ctx {}
    unsafe impl Sync for Ctx {}
    let ctx = Ctx(pin_context);

    let res = run_sync_local(async {
        use idevice::RemoteXpcClient;
        use idevice::remote_pairing::{RemotePairingClient, connect_tls_psk_tunnel_native};

        let rsd_stream = tokio::net::TcpStream::connect(socket_addr)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("RSD connect: {e}")))?;
        let rsd_handshake = RsdHandshake::new(rsd_stream).await?;

        let ts = rsd_handshake
            .services
            .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
            .ok_or(IdeviceError::ServiceNotFound)?;

        let ts_addr = std::net::SocketAddr::new(socket_addr.ip(), ts.port);
        let ts_stream = tokio::net::TcpStream::connect(ts_addr)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("tunnel connect: {e}")))?;
        let mut conn = RemoteXpcClient::new(ts_stream).await?;
        conn.do_handshake().await?;
        let _ = conn.recv_root().await?;

        let host = "idevice-ffi";
        let mut rpc = RemotePairingClient::new(conn, host, rpf);
        rpc.connect(
            async |_| {
                if let Some(cb) = pin_callback {
                    let ptr = cb(ctx.0);
                    if !ptr.is_null() {
                        if let Ok(s) = unsafe { CStr::from_ptr(ptr) }.to_str() {
                            return s.to_string();
                        }
                    }
                }
                "000000".to_string()
            },
            0u8,
        )
        .await?;

        // Create tunnel
        let tunnel_port = rpc.create_tcp_listener().await?;
        let tunnel_addr = std::net::SocketAddr::new(socket_addr.ip(), tunnel_port);
        let tunnel_stream = tokio::net::TcpStream::connect(tunnel_addr)
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
        let inner_rsd_port = tunnel.info.server_rsd_port;

        // jktcp
        let raw = tunnel.into_inner();
        let adapter = idevice::tcp::adapter::Adapter::new(Box::new(raw), client_ip, server_ip);
        let mut adapter = adapter.to_async_handle();

        // RSD through tunnel
        let rsd_stream = adapter
            .connect(inner_rsd_port)
            .await
            .map_err(|e| IdeviceError::InternalError(format!("{e}")))?;
        let handshake = RsdHandshake::new(rsd_stream).await?;

        Ok::<_, IdeviceError>((adapter, handshake))
    });

    match res {
        Ok((adapter, handshake)) => {
            unsafe {
                *out_adapter = Box::into_raw(Box::new(AdapterHandle(adapter)));
                *out_handshake = Box::into_raw(Box::new(RsdHandshakeHandle(handshake)));
            }
            null_mut()
        }
        Err(e) => ffi_err!(e),
    }
}
