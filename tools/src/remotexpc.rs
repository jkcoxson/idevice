// Jackson Coxson
// Print out all the RemoteXPC services

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use clap::{Arg, Command};
use idevice::{tunneld::get_tunneld_devices, xpc::XPCDevice};
use tokio::net::TcpStream;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("remotexpc")
        .about("Get services from RemoteXPC")
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
        println!("remotexpc - get info from RemoteXPC");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");

    let socket = SocketAddr::new(
        IpAddr::from_str("127.0.0.1").unwrap(),
        idevice::tunneld::DEFAULT_PORT,
    );
    let mut devices = get_tunneld_devices(socket)
        .await
        .expect("Failed to get tunneld devices");

    let (_udid, device) = match udid {
        Some(u) => (
            u.to_owned(),
            devices.remove(u).expect("Device not in tunneld"),
        ),
        None => devices.into_iter().next().expect("No devices"),
    };

    // Make the connection to RemoteXPC
    let client = XPCDevice::new(Box::new(
        TcpStream::connect((device.tunnel_address.as_str(), device.tunnel_port))
            .await
            .unwrap(),
    ))
    .await
    .unwrap();

    println!("{:#?}", client.services);
}
