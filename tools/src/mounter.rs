// Jackson Coxson
// Just lists apps for now

use clap::{Arg, Command};
use idevice::{mounter::ImageMounter, IdeviceService};

use sha2::{Digest, Sha384};

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
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .help("Show this help message")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("mounter - query and manage images mounted on a device. Reimplementation of libimobiledevice's binary.");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "ideviceinfo-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let mut mounter_client = ImageMounter::connect(&*provider)
        .await
        .expect("Unable to connect to image mounter");

    let images = mounter_client.copy_devices().await.unwrap();
    println!("Images: {images:#?}");

    let image = std::fs::read("Image.dmg").unwrap();
    let mut hasher = Sha384::new();
    hasher.update(image);
    let hash = hasher.finalize();

    let manifest = mounter_client
        .query_personalization_manifest("DeveloperDiskImage", hash.to_vec())
        .await
        .unwrap();
    println!("len: {}", manifest.len());
}
