// Jackson Coxson

use std::io::Write;

use clap::{Arg, Command};
use idevice::{
    core_device::AppServiceClient, core_device_proxy::CoreDeviceProxy,
    debug_proxy::DebugProxyClient, rsd::RsdHandshake, tcp::stream::AdapterStream, IdeviceService,
    RsdService,
};

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
            Arg::new("tunneld")
                .long("tunneld")
                .help("Use tunneld")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(Command::new("list").about("Lists the images mounted on the device"))
        .get_matches();

    if matches.get_flag("about") {
        println!("debug_proxy - connect to the debug proxy and run commands");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let provider =
        match common::get_provider(udid, host, pairing_file, "debug-proxy-jkcoxson").await {
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
    adapter
        .pcap("/Users/jacksoncoxson/Desktop/rs_xpc.pcap")
        .await
        .unwrap();

    let stream = AdapterStream::connect(&mut adapter, rsd_port)
        .await
        .expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();
    println!("{:?}", handshake.services);

    let mut asc = AppServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    if matches.subcommand_matches("list").is_some() {
        let apps = asc
            .list_apps(true, true, true, true, true)
            .await
            .expect("Failed to get apps");
        println!("{apps:#?}");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
