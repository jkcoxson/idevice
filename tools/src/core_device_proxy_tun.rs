// Jackson Coxson

use idevice::{
    core_device_proxy::{self},
    lockdownd::{self, LockdowndClient},
    pairing_file::PairingFile,
    Idevice,
};
use tun_rs::AbstractDevice;

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    str::FromStr,
};

#[tokio::main]
async fn main() {
    env_logger::init();
    let mut host = None;
    let mut pairing_file = None;

    // Loop through args
    let mut i = 0;
    while i < std::env::args().len() {
        match std::env::args().nth(i).unwrap().as_str() {
            "--host" => {
                host = Some(std::env::args().nth(i + 1).unwrap().to_string());
                i += 2;
            }
            "--pairing-file" => {
                pairing_file = Some(std::env::args().nth(i + 1).unwrap().to_string());
                i += 2;
            }
            "-h" | "--help" => {
                println!("core_device_proxy_tun - start a tunnel");
                println!("Usage:");
                println!("  core_device_proxy_tun [options]");
                println!("Options:");
                println!("  --host <host>");
                println!("  --pairing_file <path>");
                println!("  -h, --help");
                println!("  --about");
                println!("\n\nSet RUST_LOG to info, debug, warn, error, or trace to see more logs. Default is error.");
                std::process::exit(0);
            }
            "--about" => {
                println!("ideviceinfo - get information from the idevice. Reimplementation of libimobiledevice's binary.");
                println!("Copyright (c) 2025 Jackson Coxson");
            }
            _ => {
                i += 1;
            }
        }
    }
    if host.is_none() {
        println!("Invalid arguments! Pass the IP of the device with --host");
        return;
    }
    if pairing_file.is_none() {
        println!("Invalid arguments! Pass the path the the pairing file with --pairing-file");
        return;
    }
    let ip = Ipv4Addr::from_str(host.unwrap().as_str()).unwrap();
    let socket = SocketAddrV4::new(ip, lockdownd::LOCKDOWND_PORT);

    let socket = tokio::net::TcpStream::connect(socket).await.unwrap();
    let socket = Box::new(socket);
    let idevice = Idevice::new(socket, "heartbeat_client");

    let p = PairingFile::read_from_file(pairing_file.as_ref().unwrap()).unwrap();

    let mut lockdown_client = LockdowndClient { idevice };
    lockdown_client.start_session(&p).await.unwrap();

    let (port, _) = lockdown_client
        .start_service(core_device_proxy::SERVCE_NAME)
        .await
        .unwrap();

    let socket = SocketAddrV4::new(ip, port);
    let socket = tokio::net::TcpStream::connect(socket).await.unwrap();
    let socket = Box::new(socket);
    let mut idevice = Idevice::new(socket, "core_device_proxy_tun");

    let p = PairingFile::read_from_file(pairing_file.unwrap()).unwrap();

    idevice.start_session(&p).await.unwrap();

    let mut tun_proxy = core_device_proxy::CoreDeviceProxy::new(idevice);
    let response = tun_proxy.establish_tunnel().await.unwrap();

    let dev = tun_rs::create(&tun_rs::Configuration::default()).unwrap();
    dev.add_address_v6(response.client_parameters.address.parse().unwrap(), 32)
        .unwrap();
    dev.set_mtu(response.client_parameters.mtu).unwrap();
    dev.set_network_address(
        response.client_parameters.address,
        response.client_parameters.netmask.parse().unwrap(),
        Some(response.server_address.parse().unwrap()),
    )
    .unwrap();

    let async_dev = tun_rs::AsyncDevice::new(dev).unwrap();
    async_dev.enabled(true).unwrap();
    println!("-----------------------------");
    println!("tun device created: {:?}", async_dev.name());
    println!("server address: {}", response.server_address);
    println!("rsd port: {}", response.server_rsd_port);
    println!("-----------------------------");

    let mut buf = vec![0; 1500];
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
