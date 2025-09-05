// Jackson Coxson

use clap::{Arg, Command};
use idevice::{IdeviceService, os_trace_relay::OsTraceRelayClient};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("os_trace_relay")
        .about("Relay system logs")
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
                .help("UDID of the device (overrides host/pairing file)"),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("Relay logs on the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "misagent-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    let log_client = OsTraceRelayClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    let mut relay = log_client.start_trace(None).await.expect("Start failed");

    loop {
        println!(
            "{:#?}",
            relay.next().await.expect("Failed to read next log")
        );
    }
}
