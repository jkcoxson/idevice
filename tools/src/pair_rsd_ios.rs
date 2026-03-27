// let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x282a, 0x9aff, 0xfedb, 0x8cbb);
// let addr = SocketAddrV6::new(ip, 60461, 0, 28);
// let conn = tokio::net::TcpStream::connect(addr).await.unwrap();

// Jackson Coxson

use std::{
    any::Any,
    net::{IpAddr, SocketAddr, SocketAddrV6},
    sync::Arc,
    time::Duration,
};
#[cfg(target_os = "linux")]
use std::{fs, process::Command as OsCommand};

use clap::{Arg, Command};
use idevice::{
    RemoteXpcClient, RsdService,
    core_device::AppServiceClient,
    remote_pairing::{
        RemotePairingClient, RpPairingFile, RpPairingSocket,
        connect_tls_psk_tunnel_native,
    },
    rsd::RsdHandshake,
    tcp,
};
use tokio::net::TcpStream;
use zeroconf::{
    BrowserEvent, MdnsBrowser, ServiceType,
    prelude::{TEventLoop, TMdnsBrowser},
};

const SERVICE_PROTOCOL: &str = "tcp";

static TUNNEL_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static WIFI_PAIR_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("pair")
        .about("Pair with the device")
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tunnel")
                .long("tunnel")
                .help("Test tunnel with existing pairing file")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("wifi-pair")
                .long("wifi-pair")
                .help("Pair wirelessly via _remotepairing._tcp (no USB needed)")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("pair - pair with the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    // Store flags for the callback
    TUNNEL_MODE.store(
        matches.get_flag("tunnel"),
        std::sync::atomic::Ordering::Relaxed,
    );
    let wifi_pair = matches.get_flag("wifi-pair");
    WIFI_PAIR_MODE.store(wifi_pair, std::sync::atomic::Ordering::Relaxed);

    let service_name = if wifi_pair { "remotepairing" } else { "remoted" };
    println!("Browsing for _{service_name}._tcp ...");

    let mut browser = MdnsBrowser::new(
        ServiceType::new(service_name, SERVICE_PROTOCOL).expect("Unable to start mDNS browse"),
    );
    browser.set_service_callback(Box::new(on_service_discovered));

    let event_loop = browser.browse_services().unwrap();

    loop {
        // calling `poll()` will keep this browser alive
        event_loop.poll(Duration::from_secs(0)).unwrap();
    }
}

fn on_service_discovered(
    result: zeroconf::Result<BrowserEvent>,
    _context: Option<Arc<dyn Any + Send + Sync>>,
) {
    if let Ok(BrowserEvent::Add(result)) = result {
        tokio::task::spawn(async move {
            let wifi_pair = WIFI_PAIR_MODE.load(std::sync::atomic::Ordering::Relaxed);
            println!("Found device: {result:?}");

            let host_name = result.host_name().to_string();
            let service_address = result.address().to_string();
            let scope_id = link_local_scope_id_from_avahi(&host_name, &service_address);

            if wifi_pair {
                // Wi-Fi pairing: connect directly, use RPPairing socket protocol
                wifi_pair_flow(&host_name, &service_address, scope_id, *result.port()).await;
                return;
            }

            // USB/remoted flow: RSD → tunnel service → RemoteXPC → RPPairing
            let stream = match connect_to_service_port(
                &host_name,
                &service_address,
                scope_id,
                *result.port(),
            )
            .await
            {
                Some(s) => s,
                None => {
                    println!("Couldn't open TCP port on device");
                    return;
                }
            };

            let handshake = RsdHandshake::new(stream).await.expect("no rsd");

            println!("handshake: {handshake:#?}");

            let ts = handshake
                .services
                .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
                .unwrap();

            println!("connecting to tunnel service");
            let stream = connect_to_service_port(&host_name, &service_address, scope_id, ts.port)
                .await
                .expect("failed to connect to tunnselservice");
            let mut conn = RemoteXpcClient::new(stream).await.unwrap();

            println!("doing tunnel service handshake");
            conn.do_handshake().await.unwrap();

            let msg = conn.recv_root().await.unwrap();
            println!("{msg:#?}");

            let tunnel_mode = TUNNEL_MODE.load(std::sync::atomic::Ordering::Relaxed);
            let host = "idevice-rs-jkcoxson";

            let mut rpf = if tunnel_mode {
                match RpPairingFile::read_from_file("ios_pairing_file.plist").await {
                    Ok(f) => {
                        println!("Loaded existing pairing file");
                        f
                    }
                    Err(e) => {
                        eprintln!("Failed to load pairing file: {e}");
                        eprintln!("Run without --tunnel first to pair");
                        return;
                    }
                }
            } else {
                RpPairingFile::generate(host)
            };

            let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);
            rpc.connect(async |_| "000000".to_string(), 0u8)
                .await
                .expect("pairing/verification failed");

            if !tunnel_mode {
                rpf.write_to_file("ios_pairing_file.plist").await.unwrap();
                println!("Paired! Pairing file saved. Run with --tunnel to test tunnel.");
                return;
            }

            // === Tunnel test ===
            println!("Requesting TCP tunnel listener...");
            let port = match rpc.create_tcp_listener().await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("create_tcp_listener failed: {e}");
                    return;
                }
            };
            println!("Device listening on port {port}");

            // Connect to the tunnel port
            println!("Connecting to tunnel...");
            let tunnel_stream =
                match connect_to_service_port(&host_name, &service_address, scope_id, port).await {
                    Some(s) => s,
                    None => {
                        eprintln!("Failed to connect to tunnel port");
                        return;
                    }
                };

            {
                println!("Performing TLS-PSK + CDTunnel handshake...");
                match connect_tls_psk_tunnel_native(tunnel_stream, rpc.encryption_key()).await {
                    Ok(tunnel) => {
                        println!("Tunnel established!");
                        println!("  Client address: {}", tunnel.info.client_address);
                        println!("  Server address: {}", tunnel.info.server_address);
                        println!("  MTU: {}", tunnel.info.mtu);
                        println!("  RSD port: {}", tunnel.info.server_rsd_port);

                        let client_ip: std::net::IpAddr =
                            tunnel.info.client_address.parse().expect("bad client IP");
                        let server_ip: std::net::IpAddr =
                            tunnel.info.server_address.parse().expect("bad server IP");
                        let rsd_port = tunnel.info.server_rsd_port;

                        // Feed the tunnel into jktcp
                        println!("Starting userspace TCP stack...");
                        let raw_stream = tunnel.into_inner();
                        let adapter =
                            tcp::adapter::Adapter::new(Box::new(raw_stream), client_ip, server_ip);
                        let mut handle = adapter.to_async_handle();

                        // Connect to the RSD port through the tunnel
                        println!("Connecting to RSD through tunnel on port {rsd_port}...");
                        let rsd_stream = match handle.connect(rsd_port).await {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Failed to connect to RSD through tunnel: {e}");
                                return;
                            }
                        };

                        println!("Performing RSD handshake through tunnel...");
                        let handshake = match RsdHandshake::new(rsd_stream).await {
                            Ok(hs) => {
                                println!("RSD: {} services, UUID: {}", hs.services.len(), hs.uuid);
                                hs
                            }
                            Err(e) => {
                                eprintln!("RSD handshake through tunnel failed: {e:?}");
                                return;
                            }
                        };

                        // Connect to AppService through the tunnel
                        let app_port = match handshake
                            .services
                            .get("com.apple.coredevice.appservice")
                            .map(|s| s.port)
                        {
                            Some(p) => p,
                            None => {
                                eprintln!("AppService not found in RSD services");
                                return;
                            }
                        };
                        println!("Connecting to AppService on port {app_port}...");
                        let app_stream = match handle.connect(app_port).await {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("AppService connect failed: {e}");
                                return;
                            }
                        };
                        let mut asc = match AppServiceClient::from_stream(
                            Box::new(app_stream) as Box<dyn idevice::ReadWrite>
                        )
                        .await
                        {
                            Ok(c) => c,
                            Err(e) => {
                                eprintln!("AppService handshake failed: {e:?}");
                                return;
                            }
                        };

                        // List all apps
                        println!("Listing apps on device...\n");
                        match asc.list_apps(true, true, true, true, true).await {
                            Ok(apps) => {
                                for app in &apps {
                                    let version = app.version.as_deref().unwrap_or("?");
                                    println!(
                                        "  {} ({}) v{version}",
                                        app.name, app.bundle_identifier
                                    );
                                }
                                println!("\nTotal: {} apps", apps.len());
                            }
                            Err(e) => {
                                eprintln!("list_apps failed: {e:?}");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("TLS-PSK tunnel failed: {e:?}");
                    }
                }
            }
        });
    }
}

async fn wifi_pair_flow(
    host_name: &str,
    service_address: &str,
    scope_id: Option<u32>,
    port: u16,
) {
    println!("Wi-Fi pairing: connecting to {host_name} port {port}...");

    let stream = match connect_to_service_port(host_name, service_address, scope_id, port).await {
        Some(s) => s,
        None => {
            eprintln!("Couldn't connect to remotepairing service");
            return;
        }
    };

    println!("Connected! Starting RPPairing protocol...");

    let conn = RpPairingSocket::new(stream);
    let host = "idevice-rs-jkcoxson";
    let mut rpf = RpPairingFile::generate(host);

    let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);

    println!("Attempting pair verify / pair setup...");
    println!("(You may need to tap Trust on the device)");

    match rpc
        .connect(
            async |_| {
                println!("Enter the PIN shown on the device (or press enter for 000000):");
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
            rpf.write_to_file("ios_pairing_file.plist")
                .await
                .unwrap();
            println!("Paired! Pairing file saved to ios_pairing_file.plist");
        }
        Err(e) => {
            eprintln!("Pairing failed: {e:?}");
        }
    }
}

async fn connect_to_service_port(
    host_name: &str,
    service_address: &str,
    scope_id: Option<u32>,
    port: u16,
) -> Option<TcpStream> {
    if let Some(stream) = lookup_host_and_connect(host_name, port).await {
        return Some(stream);
    }

    let addr: IpAddr = match service_address.parse() {
        Ok(addr) => addr,
        Err(e) => {
            println!("failed to parse resolved service address {service_address}: {e}");
            return None;
        }
    };

    let socket = match addr {
        IpAddr::V6(v6) if v6.is_unicast_link_local() => {
            SocketAddr::V6(SocketAddrV6::new(v6, port, 0, scope_id.unwrap_or(0)))
        }
        _ => SocketAddr::new(addr, port),
    };

    println!("using resolved service address fallback: {socket}");

    match TcpStream::connect(socket).await {
        Ok(s) => {
            println!("connected with local addr {:?}", s.local_addr());
            Some(s)
        }
        Err(e) => {
            println!("failed to connect with service address fallback: {e:?}");
            None
        }
    }
}

async fn lookup_host_and_connect(host: &str, port: u16) -> Option<TcpStream> {
    let looked_up = match tokio::net::lookup_host((host, port)).await {
        Ok(addrs) => addrs,
        Err(e) => {
            println!("hostname lookup failed for {host}:{port}: {e}");
            return None;
        }
    };

    let mut stream = None;
    for l in looked_up {
        if l.is_ipv4() {
            continue;
        }

        println!("Found IP: {l:?}");

        match tokio::net::TcpStream::connect(l).await {
            Ok(s) => {
                println!("connected with local addr {:?}", s.local_addr());
                stream = Some(s);
                break;
            }
            Err(e) => println!("failed to connect: {e:?}"),
        }
    }

    stream
}

#[cfg(target_os = "linux")]
fn link_local_scope_id_from_avahi(host_name: &str, service_address: &str) -> Option<u32> {
    let output = OsCommand::new("avahi-browse")
        .args(["-rpt", "_remoted._tcp"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.starts_with("=;") {
            continue;
        }

        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 9 {
            continue;
        }

        let ifname = parts[1];
        let resolved_host = parts[6];
        let resolved_addr = parts[7];
        if resolved_host == host_name && resolved_addr == service_address {
            let ifindex_path = format!("/sys/class/net/{ifname}/ifindex");
            return fs::read_to_string(ifindex_path).ok()?.trim().parse().ok();
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn link_local_scope_id_from_avahi(_host_name: &str, _service_address: &str) -> Option<u32> {
    None
}
