// Jackson Coxson
// RPPairing tool - pair and create tunnels to iOS devices over network

use idevice::{
    IdeviceService, RemoteXpcClient,
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    remote_pairing::{RemotePairingClient, RpPairingFile},
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Remote pairing and tunnel operations for iOS 17+ devices")
        .with_subcommand(
            "pair",
            JkCommand::new()
                .help("Create an RPPairing file via USB (no SIGSTOP needed)")
                .with_argument(
                    JkArgument::new()
                        .with_help("Hostname to identify this computer")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to save the pairing file")
                        .required(true),
                ),
        )
        .with_subcommand(
            "tunnel",
            JkCommand::new()
                .help("Create a tunnel and list services (USB)")
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to RPPairing file (optional, for wireless)")
                        .required(false),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "pair" => {
            let hostname = sub_args
                .next_argument::<String>()
                .expect("hostname required");
            let output_path = sub_args
                .next_argument::<String>()
                .expect("output path required");

            pair_via_usb(&*provider, &hostname, &output_path).await;
        }
        "tunnel" => {
            let _pairing_file_path = sub_args.next_argument::<String>();
            tunnel_usb(&*provider).await;
        }
        _ => unreachable!(),
    }
}

async fn pair_via_usb(provider: &dyn IdeviceProvider, hostname: &str, output_path: &str) {
    println!("Connecting to CoreDeviceProxy...");
    let proxy = match CoreDeviceProxy::connect(provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("CoreDeviceProxy connect failed: {e}");
            return;
        }
    };

    let rsd_port = proxy.tunnel_info().server_rsd_port;
    println!("CDTunnel established, RSD port {rsd_port}");

    println!("Starting TCP stack...");
    let adapter = match proxy.create_software_tunnel() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Software tunnel failed: {e}");
            return;
        }
    };
    let mut adapter = adapter.to_async_handle();

    println!("Performing RSD handshake...");
    let rsd_stream = match adapter.connect(rsd_port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("RSD connect failed: {e}");
            return;
        }
    };
    let handshake = match RsdHandshake::new(rsd_stream).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("RSD handshake failed: {e}");
            return;
        }
    };
    println!("RSD: {} services", handshake.services.len());

    let ts = match handshake
        .services
        .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
    {
        Some(s) => s,
        None => {
            eprintln!("Untrusted tunnel service not found");
            return;
        }
    };

    println!("Connecting to untrusted tunnel service...");
    let ts_stream = match adapter.connect(ts.port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Tunnel service connect failed: {e}");
            return;
        }
    };
    let mut conn = match RemoteXpcClient::new(ts_stream).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("RemoteXPC init failed: {e}");
            return;
        }
    };
    if let Err(e) = conn.do_handshake().await {
        eprintln!("XPC handshake failed: {e}");
        return;
    }
    let _ = conn.recv_root().await;

    println!("Starting RPPairing...");
    println!("(You may need to tap Trust on the device)");

    let mut rpf = RpPairingFile::generate(hostname);
    let mut rpc = RemotePairingClient::new(conn, hostname, &mut rpf);

    match rpc.connect(async |_| "000000".to_string(), 0u8).await {
        Ok(()) => match rpf.write_to_file(output_path).await {
            Ok(()) => println!("Paired! Saved to {output_path}"),
            Err(e) => eprintln!("Failed to save pairing file: {e}"),
        },
        Err(e) => eprintln!("RPPairing failed: {e:?}"),
    }
}

async fn tunnel_usb(provider: &dyn IdeviceProvider) {
    println!("Connecting to CoreDeviceProxy...");
    let proxy = match CoreDeviceProxy::connect(provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("CoreDeviceProxy connect failed: {e}");
            return;
        }
    };

    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let client_addr = proxy.tunnel_info().client_address.clone();
    let server_addr = proxy.tunnel_info().server_address.clone();
    println!("CDTunnel: {client_addr} → {server_addr}, RSD port {rsd_port}");

    println!("Starting TCP stack...");
    let adapter = match proxy.create_software_tunnel() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Software tunnel failed: {e}");
            return;
        }
    };
    let mut adapter = adapter.to_async_handle();

    println!("Performing RSD handshake...");
    let rsd_stream = match adapter.connect(rsd_port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("RSD connect failed: {e}");
            return;
        }
    };
    let handshake = match RsdHandshake::new(rsd_stream).await {
        Ok(h) => h,
        Err(e) => {
            eprintln!("RSD handshake failed: {e}");
            return;
        }
    };

    println!(
        "\nTunneled RSD Services ({} total):",
        handshake.services.len()
    );
    for (name, svc) in &handshake.services {
        println!(
            "  {name}: port={} xpc={} ver={:?}",
            svc.port, svc.uses_remote_xpc, svc.service_version
        );
    }
    println!("\nDevice UUID: {}", handshake.uuid);
}
