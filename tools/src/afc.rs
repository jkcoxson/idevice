// Jackson Coxson

use std::path::PathBuf;

use clap::{Arg, Command, value_parser};
use idevice::{
    IdeviceService,
    afc::{AfcClient, opcode::AfcFopenMode},
    house_arrest::HouseArrestClient,
};

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("afc")
        .about("Manage files on the device")
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
                .long("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)"),
        )
        .arg(
            Arg::new("documents")
                .long("documents")
                .value_name("BUNDLE_ID")
                .help("Read the documents from a bundle. Note that when vending documents, you can only access files in /Documents")
                .global(true),
        )
        .arg(
            Arg::new("container")
                .long("container")
                .value_name("BUNDLE_ID")
                .help("Read the container contents of a bundle")
                .global(true),
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
            Command::new("download")
                .about("Downloads a file")
                .arg(Arg::new("path").required(true).index(1))
                .arg(Arg::new("save").required(true).index(2)),
        )
        .subcommand(
            Command::new("upload")
                .about("Creates a directory")
                .arg(
                    Arg::new("file")
                        .required(true)
                        .index(1)
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(Arg::new("path").required(true).index(2)),
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

    let mut afc_client = if let Some(bundle_id) = matches.get_one::<String>("container") {
        let h = HouseArrestClient::connect(&*provider)
            .await
            .expect("Failed to connect to house arrest");
        h.vend_container(bundle_id)
            .await
            .expect("Failed to vend container")
    } else if let Some(bundle_id) = matches.get_one::<String>("documents") {
        let h = HouseArrestClient::connect(&*provider)
            .await
            .expect("Failed to connect to house arrest");
        h.vend_documents(bundle_id)
            .await
            .expect("Failed to vend documents")
    } else {
        AfcClient::connect(&*provider)
            .await
            .expect("Unable to connect to misagent")
    };

    if let Some(matches) = matches.subcommand_matches("list") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        let res = afc_client.list_dir(path).await.expect("Failed to read dir");
        println!("{path}\n{res:#?}");
    } else if let Some(matches) = matches.subcommand_matches("mkdir") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        afc_client.mk_dir(path).await.expect("Failed to mkdir");
    } else if let Some(matches) = matches.subcommand_matches("download") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        let save = matches.get_one::<String>("save").expect("No path passed");

        let mut file = afc_client
            .open(path, AfcFopenMode::RdOnly)
            .await
            .expect("Failed to open");

        let res = file.read().await.expect("Failed to read");
        tokio::fs::write(save, res)
            .await
            .expect("Failed to write to file");
    } else if let Some(matches) = matches.subcommand_matches("upload") {
        let file = matches.get_one::<PathBuf>("file").expect("No path passed");
        let path = matches.get_one::<String>("path").expect("No path passed");

        let bytes = tokio::fs::read(file).await.expect("Failed to read file");
        let mut file = afc_client
            .open(path, AfcFopenMode::WrOnly)
            .await
            .expect("Failed to open");

        file.write(&bytes).await.expect("Failed to upload bytes");
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
