// Jackson Coxson

use clap::{Arg, Command};
use idevice::{IdeviceService, preboard_service::PreboardServiceClient};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("preboard")
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
        .subcommand(Command::new("create").about("Create a stashbag??"))
        .subcommand(Command::new("commit").about("Commit a stashbag??"))
        .get_matches();

    if matches.get_flag("about") {
        println!("preboard - no idea what this does");
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

    let mut pc = PreboardServiceClient::connect(&*provider)
        .await
        .expect("Failed to connect to Preboard");

    if matches.subcommand_matches("create").is_some() {
        pc.create_stashbag(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0])
            .await
            .expect("Failed to create");
    } else if matches.subcommand_matches("commit").is_some() {
        pc.commit_stashbag(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0])
            .await
            .expect("Failed to create");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
