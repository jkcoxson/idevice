// Jackson Coxson
// Test: pair RPPairing via USB CoreDeviceProxy tunnel

use idevice::{
    IdeviceService, RemoteXpcClient,
    core_device_proxy::CoreDeviceProxy,
    remote_pairing::{RemotePairingClient, RpPairingFile},
    rsd::RsdHandshake,
    usbmuxd::{UsbmuxdAddr, UsbmuxdConnection},
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // 1. Connect to first USB device
    println!("Connecting to device via usbmuxd...");
    let mut usbmuxd = UsbmuxdConnection::default()
        .await
        .expect("Failed to connect to usbmuxd");
    let devices = usbmuxd.get_devices().await.expect("Failed to list devices");
    let dev = devices
        .into_iter()
        .find(|d| matches!(d.connection_type, idevice::usbmuxd::Connection::Usb))
        .expect("No USB device found");
    println!("Found device: {}", dev.udid);
    let provider = dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), "pair-via-tunnel");

    // 2. Connect to CoreDeviceProxy (USB, no SIGSTOP needed)
    println!("Connecting to CoreDeviceProxy...");
    let proxy = CoreDeviceProxy::connect(&provider)
        .await
        .expect("CoreDeviceProxy connect failed");

    let rsd_port = proxy.tunnel_info().server_rsd_port;
    println!(
        "CDTunnel: {} → {}, RSD port {}",
        proxy.tunnel_info().client_address,
        proxy.tunnel_info().server_address,
        rsd_port
    );

    // 3. Create jktcp adapter
    println!("Starting TCP stack...");
    let adapter = proxy
        .create_software_tunnel()
        .expect("create_software_tunnel failed");
    let mut adapter = adapter.to_async_handle();

    // 4. RSD handshake through tunnel
    println!("Connecting to RSD through tunnel...");
    let rsd_stream = adapter.connect(rsd_port).await.expect("RSD connect failed");
    let handshake = RsdHandshake::new(rsd_stream)
        .await
        .expect("RSD handshake failed");
    println!("RSD: {} services", handshake.services.len());

    // 5. Find the untrusted tunnel service
    let ts = handshake
        .services
        .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
        .expect("untrusted tunnel service not found in RSD");
    println!(
        "Found untrusted tunnel service: port={} xpc={}",
        ts.port, ts.uses_remote_xpc
    );

    // 6. Connect to it via RemoteXPC
    println!("Connecting to untrusted tunnel service via RemoteXPC...");
    let ts_stream = adapter
        .connect(ts.port)
        .await
        .expect("tunnel service connect failed");
    let mut conn = RemoteXpcClient::new(ts_stream)
        .await
        .expect("RemoteXPC new failed");
    conn.do_handshake().await.expect("XPC handshake failed");
    let msg = conn.recv_root().await.expect("XPC recv failed");
    println!("Tunnel service info: {msg:#?}");

    // 7. RPPairing through the tunnel service
    let host = "idevice-rs-jkcoxson";
    let mut rpf = RpPairingFile::generate(host);
    let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);

    println!("Attempting RPPairing through USB tunnel...");
    println!("(You may need to tap Trust on the device)");

    match rpc
        .connect(
            async |_| {
                println!("Enter PIN (or press enter for 000000):");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).ok();
                let pin = input.trim().to_string();
                if pin.is_empty() {
                    "000000".to_string()
                } else {
                    pin
                }
            },
            0u8,
        )
        .await
    {
        Ok(()) => {
            println!("RPPairing succeeded!");
            rpf.write_to_file("ios_pairing_file.plist")
                .await
                .expect("Failed to save pairing file");
            println!("Saved to ios_pairing_file.plist");
        }
        Err(e) => {
            eprintln!("RPPairing failed: {e:?}");
        }
    }
}
