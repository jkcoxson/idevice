// Jackson Coxson

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

    let matches = Command::new("process_control")
        .about("Query process control")
        .arg(
            Arg::new("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)")
                .index(2),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("bundle_id")
                .value_name("Bundle ID")
                .help("Bundle ID of the app to launch")
                .index(1),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("process_control - launch and manage processes on the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let bundle_id = matches
        .get_one::<String>("bundle_id")
        .expect("No bundle ID specified");

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

    // Get the debug proxy
    let service = client
        .services
        .get(idevice::dvt::SERVICE_NAME)
        .expect("Client did not contain DVT service");

    let stream = TcpStream::connect(SocketAddr::new(
        IpAddr::from_str(&device.tunnel_address).unwrap(),
        service.port,
    ))
    .await
    .expect("Failed to connect");

    let mut rs_client =
        idevice::dvt::remote_server::RemoteServerClient::new(Box::new(stream)).unwrap();
    rs_client.read_message(0).await.expect("no read??");
    let mut pc_client = idevice::dvt::process_control::ProcessControlClient::new(&mut rs_client)
        .await
        .unwrap();

    let pid = pc_client
        .launch_app(bundle_id, None, None, true, false)
        .await
        .expect("no launch??");
    pc_client
        .disable_memory_limit(pid)
        .await
        .expect("no disable??");
    println!("PID: {pid}");
}
