// Jackson Coxson
// Print out all the RemoteXPC services

use clap::{Arg, Command};
use idevice::{core_device_proxy::CoreDeviceProxy, xpc::RemoteXpcClient, IdeviceService};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("remotexpc")
        .about("Get services from RemoteXPC")
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
        println!("remotexpc - get info from RemoteXPC");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let provider = match common::get_provider(udid, host, pairing_file, "remotexpc-jkcoxson").await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let mut adapter = proxy.create_software_tunnel().expect("no software tunnel");
    adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut client = RemoteXpcClient::new(Box::new(adapter)).await.unwrap();

    println!("{:#?}", client.do_handshake().await);
}
