// Jackson Coxson

use clap::{Arg, Command};
use idevice::{afc::AfcClient, IdeviceService};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("afc")
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
                .help("UDID of the device (overrides host/pairing file)"),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("list")
                .about("Lists the items in the directory")
                .arg(Arg::new("path").required(true).index(1)),
        )
        .subcommand(
            Command::new("mkdir")
                .about("Creates a directory")
                .arg(Arg::new("path").required(true).index(1)),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a provisioning profile")
                .arg(Arg::new("path").required(true).index(1)),
        )
        .subcommand(
            Command::new("remove_all")
                .about("Remove a provisioning profile")
                .arg(Arg::new("path").required(true).index(1)),
        )
        .subcommand(
            Command::new("info")
                .about("Get info about a file")
                .arg(Arg::new("path").required(true).index(1)),
        )
        .subcommand(Command::new("device_info").about("Get info about the device"))
        .get_matches();

    if matches.get_flag("about") {
        println!("afc");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "afc-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    let mut afc_client = AfcClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    if let Some(matches) = matches.subcommand_matches("list") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        let res = afc_client.list_dir(path).await.expect("Failed to read dir");
        println!("{path}\n{res:#?}");
    } else if let Some(matches) = matches.subcommand_matches("mkdir") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        afc_client.mk_dir(path).await.expect("Failed to mkdir");
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        afc_client.remove(path).await.expect("Failed to remove");
    } else if let Some(matches) = matches.subcommand_matches("remove_all") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        afc_client.remove_all(path).await.expect("Failed to remove");
    } else if let Some(matches) = matches.subcommand_matches("info") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        let res = afc_client
            .get_file_info(path)
            .await
            .expect("Failed to get file info");
        println!("{res:#?}");
    } else if matches.subcommand_matches("device_info").is_some() {
        let res = afc_client
            .get_device_info()
            .await
            .expect("Failed to get file info");
        println!("{res:#?}");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
