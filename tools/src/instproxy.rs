// Jackson Coxson
// Just lists apps for now

use idevice::{
    installation_proxy::InstallationProxyClient,
    lockdownd::{self, LockdowndClient},
    pairing_file::PairingFile,
    Idevice,
};

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
                println!("ideviceinfo - get information from the idevice");
                println!("Usage:");
                println!("  ideviceinfo [options]");
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
        .start_service("com.apple.mobile.installation_proxy")
        .await
        .unwrap();

    let socket = SocketAddrV4::new(ip, port);
    let socket = tokio::net::TcpStream::connect(socket).await.unwrap();
    let socket = Box::new(socket);
    let mut idevice = Idevice::new(socket, "instproxy-client");

    let p = PairingFile::read_from_file(pairing_file.unwrap()).unwrap();

    idevice.start_session(&p).await.unwrap();

    let mut instproxy_client = InstallationProxyClient::new(idevice);
    let apps = instproxy_client.get_apps(None, None).await.unwrap();
    for app in apps.keys() {
        println!("{app}");
    }
}
