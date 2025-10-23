// Jackson Coxson

use clap::{Arg, Command};
use idevice::{IdeviceService, amfi::AmfiClient};

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("amfi")
        .about("Mess with developer mode")
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
        .subcommand(Command::new("show").about("Shows the developer mode option in settings"))
        .subcommand(Command::new("enable").about("Enables developer mode"))
        .subcommand(Command::new("accept").about("Shows the accept dialogue for developer mode"))
        .subcommand(Command::new("status").about("Gets the developer mode status"))
        .subcommand(
            Command::new("trust")
                .about("Trusts an app signer")
                .arg(Arg::new("uuid").required(true)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("amfi - manage developer mode");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "amfi-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let mut amfi_client = AmfiClient::connect(&*provider)
        .await
        .expect("Failed to connect to amfi");

    if matches.subcommand_matches("show").is_some() {
        amfi_client
            .reveal_developer_mode_option_in_ui()
            .await
            .expect("Failed to show");
    } else if matches.subcommand_matches("enable").is_some() {
        amfi_client
            .enable_developer_mode()
            .await
            .expect("Failed to show");
    } else if matches.subcommand_matches("accept").is_some() {
        amfi_client
            .accept_developer_mode()
            .await
            .expect("Failed to show");
    } else if matches.subcommand_matches("status").is_some() {
        let status = amfi_client
            .get_developer_mode_status()
            .await
            .expect("Failed to get status");
        println!("Enabled: {status}");
    } else if let Some(matches) = matches.subcommand_matches("state") {
        let uuid: &String = match matches.get_one("uuid") {
            Some(u) => u,
            None => {
                eprintln!("No UUID passed. Invalid usage, pass -h for help");
                return;
            }
        };
        let status = amfi_client
            .trust_app_signer(uuid)
            .await
            .expect("Failed to get state");
        println!("Enabled: {status}");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
