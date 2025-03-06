// Jackson Coxson

use std::path::PathBuf;

use clap::{arg, value_parser, Arg, Command};
use idevice::{
    lockdownd::LockdowndClient, misagent::MisagentClient, mounter::ImageMounter,
    pretty_print_plist, IdeviceService,
};

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
        .subcommand(Command::new("list").about("Lists the images mounted on the device"))
        .get_matches();

    if matches.get_flag("about") {
        println!("mounter - query and manage images mounted on a device. Reimplementation of libimobiledevice's binary.");
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
    let mut misagent_client = MisagentClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    if matches.subcommand_matches("list").is_some() {
        let images = misagent_client
            .copy_all()
            .await
            .expect("Unable to get images");
        for i in images {
            println!("{}", pretty_print_plist(&i));
        }
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
