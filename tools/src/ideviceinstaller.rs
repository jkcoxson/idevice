// A minimal ideviceinstaller-like CLI to install/upgrade apps

use clap::{Arg, ArgAction, Command};
use idevice::utils::installation;

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("ideviceinstaller")
        .about("Install/upgrade apps on an iOS device (AFC + InstallationProxy)")
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
                .action(ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("install")
                .about("Install a local .ipa or directory")
                .arg(Arg::new("path").required(true).value_name("PATH")),
        )
        .subcommand(
            Command::new("upgrade")
                .about("Upgrade from a local .ipa or directory")
                .arg(Arg::new("path").required(true).value_name("PATH")),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("ideviceinstaller - install/upgrade apps using AFC + InstallationProxy (Rust)");
        println!("Copyright (c) 2025");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "ideviceinstaller").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    if let Some(matches) = matches.subcommand_matches("install") {
        let path: &String = matches.get_one("path").expect("required");
        match installation::install_package_with_callback(
            &*provider,
            path,
            None,
            |(percentage, _)| async move {
                println!("Installing: {percentage}%");
            },
            (),
        )
        .await
        {
            Ok(()) => println!("install success"),
            Err(e) => eprintln!("Install failed: {e}"),
        }
    } else if let Some(matches) = matches.subcommand_matches("upgrade") {
        let path: &String = matches.get_one("path").expect("required");
        match installation::upgrade_package_with_callback(
            &*provider,
            path,
            None,
            |(percentage, _)| async move {
                println!("Upgrading: {percentage}%");
            },
            (),
        )
        .await
        {
            Ok(()) => println!("upgrade success"),
            Err(e) => eprintln!("Upgrade failed: {e}"),
        }
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
