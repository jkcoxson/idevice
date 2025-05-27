// Jackson Coxson
// Just lists apps for now

use clap::{Arg, Command};
use idevice::{installation_proxy::InstallationProxyClient, IdeviceService};

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
        .subcommand(Command::new("lookup").about("Gets the apps on the device"))
        .subcommand(Command::new("browse").about("Browses the apps on the device"))
        .subcommand(Command::new("check_capabilities").about("Check the capabilities"))
        .subcommand(
            Command::new("install")
                .about("Install an app in the AFC jail")
                .arg(Arg::new("path")),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("instproxy - query and manage apps installed on a device. Reimplementation of libimobiledevice's binary.");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "instproxy-jkcoxson").await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let mut instproxy_client = InstallationProxyClient::connect(&*provider)
        .await
        .expect("Unable to connect to instproxy");
    if matches.subcommand_matches("lookup").is_some() {
        let apps = instproxy_client
            .get_apps(Some("User".to_string()), None)
            .await
            .unwrap();
        for app in apps.keys() {
            println!("{app}");
        }
    } else if matches.subcommand_matches("browse").is_some() {
        instproxy_client.browse(None).await.expect("browse failed");
    } else if matches.subcommand_matches("check_capabilities").is_some() {
        instproxy_client
            .check_capabilities_match(Vec::new(), None)
            .await
            .expect("check failed");
    } else if let Some(matches) = matches.subcommand_matches("install") {
        let path: &String = match matches.get_one("path") {
            Some(p) => p,
            None => {
                eprintln!("No path passed, pass -h for help");
                return;
            }
        };

        instproxy_client
            .install_with_callback(
                path,
                None,
                async |(percentage, _)| {
                    println!("Installing: {percentage}");
                },
                (),
            )
            .await
            .expect("Failed to install")
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
