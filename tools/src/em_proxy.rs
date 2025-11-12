// Jackson Coxson & SternXD
// Emotional Mangling Proxy - A transparent interface level proxy for TCP packets
// Based on the original implementation by jkcoxson (https://github.com/jkcoxson/em_proxy)

use clap::{Arg, Command};
use idevice::{IdeviceService, core_device_proxy::CoreDeviceProxy};

mod common;

#[derive(Debug, Clone)]
enum TunnelMode {
    WireGuard,
    CoreDeviceProxy,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let matches = Command::new("em_proxy")
		.about("Emotional Mangling Proxy - Transparent TCP packet proxy")
		.arg(
			Arg::new("host")
				.long("host")
				.value_name("HOST")
				.help("IP address of the device"),
		)
		.arg(
			Arg::new("pairing_file")
				.long("pairing-file")
				.value_name("PATH")
				.help("Path to the pairing file"),
		)
		.arg(
			Arg::new("udid")
				.value_name("UDID")
				.help("UDID of the device (overrides host/pairing file)")
				.index(1),
		)
		.arg(
			Arg::new("wireguard")
				.long("wireguard")
				.help("Use WireGuard tunnel (default)")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("core-device")
				.long("core-device")
				.help("Use CoreDeviceProxy tunnel")
				.action(clap::ArgAction::SetTrue),
		)
		.arg(
			Arg::new("wg-interface")
				.long("wg-interface")
				.value_name("INTERFACE")
				.help("WireGuard interface name (default: wg0)"),
		)
		.arg(
			Arg::new("wg-server-key")
				.long("wg-server-key")
				.value_name("KEY_OR_PATH")
				.help("WireGuard server private key (base64 string or path to key file)"),
		)
		.arg(
			Arg::new("wg-client-key")
				.long("wg-client-key")
				.value_name("KEY_OR_PATH")
				.help("WireGuard client public key (base64 string or path to key file)"),
		)
		.arg(
			Arg::new("wg-keys-dir")
				.long("wg-keys-dir")
				.value_name("DIR")
				.help("Directory containing server_privatekey and client_publickey files (default: ./keys)"),
		)
		.arg(
			Arg::new("wg-bind")
				.long("wg-bind")
				.value_name("ADDRESS")
				.help("WireGuard UDP bind address (default: 127.0.0.1:51820)"),
		)
		.arg(
			Arg::new("about")
				.long("about")
				.help("Show about information")
				.action(clap::ArgAction::SetTrue),
		)
		.get_matches();

    if matches.get_flag("about") {
        println!("em_proxy - Emotional Mangling Proxy");
        println!("A transparent interface-level proxy for TCP packets");
        return;
    }

    // Determine tunnel mode
    let tunnel_mode = if matches.get_flag("core-device") {
        TunnelMode::CoreDeviceProxy
    } else {
        TunnelMode::WireGuard
    };

    let wg_interface = matches
        .get_one::<String>("wg-interface")
        .map(|s| s.as_str())
        .unwrap_or("wg0");

    let wg_server_key = matches.get_one::<String>("wg-server-key");
    let wg_client_key = matches.get_one::<String>("wg-client-key");
    let wg_keys_dir = matches.get_one::<String>("wg-keys-dir");
    let wg_bind = matches.get_one::<String>("wg-bind");

    match tunnel_mode {
        TunnelMode::WireGuard => {
            println!("Using WireGuard tunnel (interface: {})", wg_interface);
            run_wireguard_proxy(
                wg_interface,
                wg_server_key,
                wg_client_key,
                wg_keys_dir,
                wg_bind,
            )
            .await;
        }
        TunnelMode::CoreDeviceProxy => {
            #[cfg(feature = "core-device")]
            {
                println!("Using CoreDeviceProxy tunnel");
                let udid = matches.get_one::<String>("udid");
                let host = matches.get_one::<String>("host");
                let pairing_file = matches.get_one::<String>("pairing_file");

                let provider =
                    match common::get_provider(udid, host, pairing_file, "em_proxy-jkcoxson").await
                    {
                        Ok(p) => p,
                        Err(e) => {
                            eprintln!("{e}");
                            return;
                        }
                    };

                run_core_device_proxy(provider).await;
            }
            #[cfg(not(feature = "core-device"))]
            {
                eprintln!(
                    "CoreDeviceProxy support requires the 'core-device' feature to be enabled"
                );
                eprintln!("Build with: cargo build --features core-device");
                eprintln!();
                eprintln!("Alternatively, use --wireguard for WireGuard tunnel");
            }
        }
    }
}

/// Loads a key from either a file path or uses the string directly as base64
#[cfg(feature = "wireguard")]
fn load_key(
    key_or_path: Option<&String>,
    default_file: &str,
    keys_dir: Option<&String>,
) -> Option<String> {
    if let Some(key) = key_or_path {
        // Check if it's a file path (contains / or starts with .)
        if key.contains('/') || key.starts_with('.') || std::path::Path::new(key).exists() {
            // It's a file path
            match std::fs::read_to_string(key) {
                Ok(content) => {
                    // Trim whitespace and newlines
                    Some(content.trim().to_string())
                }
                Err(e) => {
                    eprintln!("Failed to read key file '{}': {e:?}", key);
                    None
                }
            }
        } else {
            // It's a direct key string
            Some(key.clone())
        }
    } else if let Some(dir) = keys_dir {
        // Try to load from keys directory
        let path = std::path::Path::new(dir).join(default_file);
        match std::fs::read_to_string(&path) {
            Ok(content) => Some(content.trim().to_string()),
            Err(_) => None,
        }
    } else {
        // First try current dir
        let current_dir_path = std::path::Path::new("keys").join(default_file);
        if let Ok(content) = std::fs::read_to_string(&current_dir_path) {
            return Some(content.trim().to_string());
        }

        // Second try user config dir
        if let Some(home) = std::env::var_os("HOME") {
            let config_path = std::path::Path::new(&home)
                .join(".config")
                .join("em_proxy")
                .join("keys")
                .join(default_file);
            if let Ok(content) = std::fs::read_to_string(&config_path) {
                return Some(content.trim().to_string());
            }
        }

        None
    }
}

#[cfg(feature = "wireguard")]
async fn run_wireguard_proxy(
    wg_interface: &str,
    wg_server_key: Option<&String>,
    wg_client_key: Option<&String>,
    wg_keys_dir: Option<&String>,
    wg_bind: Option<&String>,
) {
    use boringtun::noise::Tunn;
    use std::net::SocketAddrV4;
    use std::str::FromStr;
    use x25519_dalek::{PublicKey, StaticSecret};

    // Default WireGuard UDP port
    let bind_addr_str = wg_bind.map(|s| s.as_str()).unwrap_or("127.0.0.1:51820");
    let bind_addr = match SocketAddrV4::from_str(bind_addr_str) {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!("Invalid bind address '{}': {e:?}", bind_addr_str);
            eprintln!("Expected format: IP:PORT (e.g., 127.0.0.1:51820)");
            return;
        }
    };

    // Get keys from CLI, files, or use placeholders
    let server_private_str = load_key(wg_server_key, "server_privatekey", wg_keys_dir)
        .unwrap_or_else(|| {
            eprintln!("Warning: No server private key provided");
            eprintln!("Provide key via:");
            eprintln!("  --wg-server-key <base64_key>");
            eprintln!("  --wg-server-key /path/to/key/file");
            eprintln!("  --wg-keys-dir /path/to/keys (looks for server_privatekey)");
            eprintln!("  Or place server_privatekey in one of:");
            eprintln!("    - ./keys/ (current directory)");
            eprintln!("    - ~/.config/em_proxy/keys/ (user config)");
            "00000000000000000000000000000000000000000000000000".to_string() // 44 chars base64
        });

    let client_public_str = load_key(wg_client_key, "client_publickey", wg_keys_dir)
        .unwrap_or_else(|| {
            eprintln!("Warning: No client public key provided");
            eprintln!("Provide key via:");
            eprintln!("  --wg-client-key <base64_key>");
            eprintln!("  --wg-client-key /path/to/key/file");
            eprintln!("  --wg-keys-dir /path/to/keys (looks for client_publickey)");
            eprintln!("  Or place client_publickey in one of:");
            eprintln!("    - ./keys/ (current directory)");
            eprintln!("    - ~/.config/em_proxy/keys/ (user config)");
            "00000000000000000000000000000000000000000000000000".to_string() // 44 chars base64
        });

    // Parse base64 keys to bytes, then to StaticSecret/PublicKey
    use base64::{Engine as _, engine::general_purpose};

    let server_private_bytes = match general_purpose::STANDARD.decode(server_private_str.trim()) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        Ok(_) => {
            eprintln!("Server private key must be 32 bytes when decoded");
            return;
        }
        Err(e) => {
            eprintln!("Failed to decode server private key (must be base64): {e:?}");
            return;
        }
    };

    let client_public_bytes = match general_purpose::STANDARD.decode(client_public_str.trim()) {
        Ok(bytes) if bytes.len() == 32 => bytes,
        Ok(_) => {
            eprintln!("Client public key must be 32 bytes when decoded");
            return;
        }
        Err(e) => {
            eprintln!("Failed to decode client public key (must be base64): {e:?}");
            return;
        }
    };

    let server_private =
        StaticSecret::from(<[u8; 32]>::try_from(server_private_bytes.as_slice()).unwrap());
    let client_public =
        PublicKey::from(<[u8; 32]>::try_from(client_public_bytes.as_slice()).unwrap());

    let tun = match Tunn::new(server_private, client_public, None, None, 0, None) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to create WireGuard tunnel: {e:?}");
            return;
        }
    };

    println!("-----------------------------");
    println!("WireGuard Emotional Mangling Proxy");
    println!("Listening on: {}", bind_addr);
    println!("Interface: {}", wg_interface);
    println!("-----------------------------");
    println!("em_proxy is now intercepting and modifying packets");
    println!("Packets will have source/destination IPs swapped (bytes 12-15 <-> 16-19)");

    // Bind to UDP socket for WireGuard
    let socket = match tokio::net::UdpSocket::bind(bind_addr).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to bind to {}: {e:?}", bind_addr);
            eprintln!("Note: The address may be in use. Try a different port.");
            return;
        }
    };

    let mut buf = [0_u8; 2048];
    let mut unencrypted_buf = [0_u8; 2176];
    let tun = std::sync::Arc::new(tokio::sync::Mutex::new(tun));

    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, endpoint)) => {
                let tun_clone = tun.clone();
                let mut tun_guard = tun_clone.lock().await;

                // Decapsulate WireGuard packet
                let result =
                    tun_guard.decapsulate(Some(endpoint.ip()), &buf[..size], &mut unencrypted_buf);

                match result {
                    boringtun::noise::TunnResult::WriteToTunnelV4(packet, _addr) => {
                        // This is the "emotional mangling" - swap source and destination IP
                        // Bytes 12-15 (source IP) <-> bytes 16-19 (destination IP)
                        let mut mangled_packet = packet.to_vec();
                        if mangled_packet.len() >= 20 {
                            emotional_mangle_ip_packet(&mut mangled_packet);
                        }

                        // Re-encapsulate and send back
                        let mut send_buf = [0_u8; 2048];
                        match tun_guard.encapsulate(&mangled_packet, &mut send_buf) {
                            boringtun::noise::TunnResult::WriteToNetwork(data) => {
                                if let Err(e) = socket.send_to(data, endpoint).await {
                                    eprintln!("Failed to send packet: {e:?}");
                                }
                            }
                            _ => {
                                eprintln!("Unexpected result from encapsulate");
                            }
                        }
                    }
                    boringtun::noise::TunnResult::WriteToTunnelV6(_, _) => {
                        eprintln!("IPv6 not supported");
                    }
                    boringtun::noise::TunnResult::WriteToNetwork(data) => {
                        // Forward WireGuard control packets
                        if let Err(e) = socket.send_to(data, endpoint).await {
                            eprintln!("Failed to send control packet: {e:?}");
                        }
                    }
                    boringtun::noise::TunnResult::Done => {
                        // Handshake complete or keep alive
                    }
                    boringtun::noise::TunnResult::Err(e) => {
                        eprintln!("WireGuard error: {e:?}");
                    }
                }
            }
            Err(e) => {
                eprintln!("Error receiving from socket: {e:?}");
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }
    }
}

#[cfg(not(feature = "wireguard"))]
async fn run_wireguard_proxy(
    _wg_interface: &str,
    _wg_server_key: Option<&String>,
    _wg_client_key: Option<&String>,
    _wg_keys_dir: Option<&String>,
    _wg_bind: Option<&String>,
) {
    eprintln!("WireGuard support requires the 'wireguard' feature to be enabled");
    eprintln!("Build with: cargo build --features wireguard");
    eprintln!();
    eprintln!("Alternatively, use --core-device for CoreDeviceProxy tunnel");
}

#[cfg(feature = "core-device")]
async fn run_core_device_proxy(provider: Box<dyn idevice::provider::IdeviceProvider>) {
    let mut tun_proxy = match CoreDeviceProxy::connect(&*provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Unable to connect to CoreDeviceProxy: {e:?}");
            return;
        }
    };

    // Create TUN interface
    use tun_rs::DeviceBuilder;
    let dev = match DeviceBuilder::new()
        .mtu(tun_proxy.handshake.client_parameters.mtu)
        .build_sync()
    {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to create TUN interface: {e:?}");
            eprintln!("Note: You may need to run with sudo/root privileges");
            return;
        }
    };

    // Configure TUN interface with addresses from handshake
    let client_ip: std::net::Ipv6Addr = match tun_proxy.handshake.client_parameters.address.parse()
    {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Failed to parse client IP (must be IPv6): {e:?}");
            return;
        }
    };

    let server_ip: std::net::Ipv6Addr = match tun_proxy.handshake.server_address.parse() {
        Ok(ip) => ip,
        Err(e) => {
            eprintln!("Failed to parse server IP (must be IPv6): {e:?}");
            return;
        }
    };

    // Set MTU
    if let Err(e) = dev.set_mtu(tun_proxy.handshake.client_parameters.mtu) {
        eprintln!("Failed to set MTU: {e:?}");
        return;
    }

    // convert netmask to prefix length
    let netmask_str = &tun_proxy.handshake.client_parameters.netmask;
    let prefix_len = if let Ok(netmask_ipv6) = netmask_str.parse::<std::net::Ipv6Addr>() {
        // Count leading 1s in the netmask to get prefix length
        let octets = netmask_ipv6.octets();
        let mut prefix = 0;
        for &byte in &octets {
            if byte == 0xFF {
                prefix += 8;
            } else {
                // Count bits in partial byte
                prefix += byte.leading_ones();
                break;
            }
        }
        prefix as u8
    } else {
        // Default to /64 for IPv6 if parsing fails
        64
    };

    if let Err(e) = dev.add_address_v6(client_ip, prefix_len) {
        eprintln!("Failed to add IPv6 address: {e:?}");
        return;
    }

    let async_dev = match tun_rs::AsyncDevice::new(dev) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Failed to create async TUN device: {e:?}");
            return;
        }
    };

    if let Err(e) = async_dev.enabled(true) {
        eprintln!("Failed to enable TUN device: {e:?}");
        return;
    }

    println!("-----------------------------");
    println!("TUN device created: {:?}", async_dev.name());
    println!(
        "Client address: {}",
        tun_proxy.handshake.client_parameters.address
    );
    println!("Server address: {}", tun_proxy.handshake.server_address);
    println!("RSD port: {}", tun_proxy.handshake.server_rsd_port);
    println!("-----------------------------");
    println!("em_proxy is now intercepting and modifying TCP packets");
    println!("Packets are being retransmitted through CoreDeviceProxy tunnel");

    // intercept packets, modify them, and retransmit
    let mut buf = vec![0; 20_000]; // Large buffer for XPC packets
    loop {
        tokio::select! {
            Ok(len) = async_dev.recv(&mut buf) => {
                // Packet received from TUN interface
                // Translate localhost (127.0.0.1) to tunnel IPs
                let modified_packet = modify_packet_for_loopback(&buf[..len], &client_ip, &server_ip);

                // Send modified packet through CoreDeviceProxy tunnel
                if let Err(e) = tun_proxy.send(&modified_packet).await {
                    eprintln!("Failed to send packet through tunnel: {e:?}");
                }
            }
            Ok(res) = tun_proxy.recv() => {
                // Packet received from device through tunnel
                // Translate tunnel IPs back to localhost
                let modified_packet = modify_packet_from_device(&res, &client_ip, &server_ip);

                if let Err(e) = async_dev.send(&modified_packet).await {
                    eprintln!("Failed to send packet to TUN: {e:?}");
                }
            }
        }
    }
}

/// Emotional mangling: swaps source and destination IP addresses
/// Used for WireGuard mode - swaps bytes 12-15 with 16-19 in the IP header
#[cfg(feature = "wireguard")]
fn emotional_mangle_ip_packet(packet: &mut [u8]) {
    // bytes 12-15 are source IP, 16-19 are destination IP
    // Swap them to route through tunnel instead of localhost
    if packet.len() >= 20 {
        packet.swap(12, 16);
        packet.swap(13, 17);
        packet.swap(14, 18);
        packet.swap(15, 19);
    }
}

/// Modifies a packet to work around loopback limitations for CoreDeviceProxy
/// Translates localhost (::1) addresses to tunnel IPs
fn modify_packet_for_loopback(
    packet: &[u8],
    client_ip: &std::net::Ipv6Addr,
    server_ip: &std::net::Ipv6Addr,
) -> Vec<u8> {
    let mut modified = packet.to_vec();
    translate_localhost_to_tunnel(&mut modified, client_ip, server_ip);
    modified
}

/// Translates localhost addresses to tunnel IPs
/// Replaces ::1 (IPv6 localhost) with tunnel IPs so CoreDeviceProxy can route them
fn translate_localhost_to_tunnel(
    packet: &mut [u8],
    client_ip: &std::net::Ipv6Addr,
    server_ip: &std::net::Ipv6Addr,
) {
    // IPv6 header is at least 40 bytes, IP addresses start at offset 8 (source) and 24 (destination)
    if packet.len() < 40 {
        return; // Not a valid IPv6 packet
    }

    // Check IPv6 version (first 4 bits should be 6)
    if (packet[0] >> 4) != 6 {
        return; // Not an IPv6 packet
    }

    // Check if source IP is ::1 (IPv6 localhost: all zeros except last byte is 1)
    let is_localhost_src = packet[8..15] == [0, 0, 0, 0, 0, 0, 0] && packet[15] == 1;
    // Check if destination IP is ::1
    let is_localhost_dst = packet[24..31] == [0, 0, 0, 0, 0, 0, 0] && packet[31] == 1;

    if is_localhost_src {
        // Replace source IP with client tunnel IP
        let ip_bytes = client_ip.octets();
        packet[8..16].copy_from_slice(&ip_bytes);
    }
    if is_localhost_dst {
        // Replace destination IP with server tunnel IP
        let ip_bytes = server_ip.octets();
        packet[24..32].copy_from_slice(&ip_bytes);
    }
}

/// Modifies a packet received from the device to route back to localhost
/// Translates tunnel IPs back to ::1 (IPv6 localhost)
fn modify_packet_from_device(
    packet: &[u8],
    client_ip: &std::net::Ipv6Addr,
    server_ip: &std::net::Ipv6Addr,
) -> Vec<u8> {
    let mut modified = packet.to_vec();
    translate_tunnel_to_localhost(&mut modified, client_ip, server_ip);
    modified
}

/// Translates tunnel IPs back to localhost addresses (::1)
fn translate_tunnel_to_localhost(
    packet: &mut [u8],
    client_ip: &std::net::Ipv6Addr,
    server_ip: &std::net::Ipv6Addr,
) {
    // IPv6 header is at least 40 bytes, IP addresses start at offset 8 (source) and 24 (destination)
    if packet.len() < 40 {
        return; // Not a valid IPv6 packet
    }

    // Check IPv6 version (first 4 bits should be 6)
    if (packet[0] >> 4) != 6 {
        return; // Not an IPv6 packet
    }

    let client_bytes = client_ip.octets();
    let server_bytes = server_ip.octets();

    // Check if source IP matches server tunnel IP (packet coming from device)
    let is_server_src = packet[8..16] == server_bytes;

    // Check if destination IP matches client tunnel IP (packet going to Mac)
    let is_client_dst = packet[24..32] == client_bytes;

    if is_server_src {
        // Replace server IP with ::1 (IPv6 localhost)
        packet[8..16].fill(0);
        packet[15] = 1;
    }
    if is_client_dst {
        // Replace client IP with ::1 (IPv6 localhost)
        packet[24..32].fill(0);
        packet[31] = 1;
    }
}
