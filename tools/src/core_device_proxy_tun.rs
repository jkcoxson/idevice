// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    core_device_proxy::{self},
    IdeviceService,
};

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let matches = Command::new("core_device_proxy_tun")
        .about("Start a tunnel")
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
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("core_device_proxy - Start a lockdown tunnel on the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "core_device_proxy-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let mut tun_proxy = core_device_proxy::CoreDeviceProxy::connect(&*provider)
        .await
        .expect("Unable to connect");

    // Create TUN interface
    use tun_rs::DeviceBuilder;
    let dev = DeviceBuilder::new()
        .mtu(tun_proxy.handshake.client_parameters.mtu)
        .build_sync()
        .expect("Failed to create TUN interface");

    // Make TUN interface with addresses from handshake
    let client_ip: std::net::Ipv6Addr = tun_proxy
        .handshake
        .client_parameters
        .address
        .parse()
        .expect("Failed to parse client IP (must be IPv6)");

    // Set MTU
    dev.set_mtu(tun_proxy.handshake.client_parameters.mtu)
        .expect("Failed to set MTU");

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

    dev.add_address_v6(client_ip, prefix_len)
        .expect("Failed to add IPv6 address");

    let async_dev = tun_rs::AsyncDevice::new(dev).unwrap();
    async_dev.enabled(true).unwrap();
    println!("-----------------------------");
    println!("tun device created: {:?}", async_dev.name());
    println!("server address: {}", tun_proxy.handshake.server_address);
    println!("rsd port: {}", tun_proxy.handshake.server_rsd_port);
    println!("-----------------------------");

    let mut buf = vec![0; 20_000]; // XPC is big lol
    loop {
        tokio::select! {
            Ok(len) = async_dev.recv(&mut buf) => {
                println!("tun pkt: {:?}", &buf[..len]);
                tun_proxy.send(&buf[..len]).await.unwrap();
            }
            Ok(res) = tun_proxy.recv() => {
                println!("dev pkt: {:?}", &res);
                async_dev.send(&res).await.unwrap();
            }
        }
    }
}
