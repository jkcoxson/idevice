// Jackson Coxson
// Heartbeat client

use clap::{Arg, Command};
use idevice::{heartbeat::HeartbeatClient, IdeviceService};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();
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
        println!("heartbeat_client - heartbeat a device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "heartbeat_client-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };
    let mut heartbeat_client = HeartbeatClient::connect(&*provider)
        .await
        .expect("Unable to connect to heartbeat");

    let mut interval = 15;
    loop {
        interval = heartbeat_client.get_marco(interval).await.unwrap();
        heartbeat_client.send_polo().await.unwrap();
    }
}
