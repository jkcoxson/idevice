// Jackson Coxson
// Just lists apps for now

use std::{io::Write, path::PathBuf};

use clap::{Arg, Command, arg, value_parser};
use idevice::{
    IdeviceService, lockdown::LockdownClient, mobile_image_mounter::ImageMounter,
    pretty_print_plist,
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
        .subcommand(Command::new("unmount").about("Unmounts the developer disk image"))
        .subcommand(
            Command::new("mount")
                .about("Mounts the developer disk image")
                .arg(
                    arg!(-i --image <FILE> "the developer disk image to mount")
                        .value_parser(value_parser!(PathBuf))
                        .required(true),
                )
                .arg(
                    arg!(-b --manifest <FILE> "the build manifest (iOS 17+)")
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    arg!(-t --trustcache <FILE> "the trust cache (iOS 17+)")
                        .value_parser(value_parser!(PathBuf)),
                )
                .arg(
                    arg!(-s --signature <FILE> "the image signature (iOS < 17.0")
                        .value_parser(value_parser!(PathBuf)),
                ),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!(
            "mounter - query and manage images mounted on a device. Reimplementation of libimobiledevice's binary."
        );
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

    let mut lockdown_client = LockdownClient::connect(&*provider)
        .await
        .expect("Unable to connect to lockdown");

    let product_version = match lockdown_client
        .get_value(Some("ProductVersion"), None)
        .await
    {
        Ok(p) => p,
        Err(_) => {
            lockdown_client
                .start_session(&provider.get_pairing_file().await.unwrap())
                .await
                .unwrap();
            lockdown_client
                .get_value(Some("ProductVersion"), None)
                .await
                .unwrap()
        }
    };
    let product_version = product_version
        .as_string()
        .unwrap()
        .split('.')
        .collect::<Vec<&str>>()[0]
        .parse::<u8>()
        .unwrap();

    let mut mounter_client = ImageMounter::connect(&*provider)
        .await
        .expect("Unable to connect to image mounter");

    if matches.subcommand_matches("list").is_some() {
        let images = mounter_client
            .copy_devices()
            .await
            .expect("Unable to get images");
        for i in images {
            println!("{}", pretty_print_plist(&i));
        }
    } else if matches.subcommand_matches("unmount").is_some() {
        if product_version < 17 {
            mounter_client
                .unmount_image("/Developer")
                .await
                .expect("Failed to unmount");
        } else {
            mounter_client
                .unmount_image("/System/Developer")
                .await
                .expect("Failed to unmount");
        }
    } else if let Some(matches) = matches.subcommand_matches("mount") {
        let image: &PathBuf = match matches.get_one("image") {
            Some(i) => i,
            None => {
                eprintln!("No image was passed! Pass -h for help");
                return;
            }
        };
        let image = tokio::fs::read(image).await.expect("Unable to read image");
        if product_version < 17 {
            let signature: &PathBuf = match matches.get_one("signature") {
                Some(s) => s,
                None => {
                    eprintln!("No signature was passed! Pass -h for help");
                    return;
                }
            };
            let signature = tokio::fs::read(signature)
                .await
                .expect("Unable to read signature");

            mounter_client
                .mount_developer(&image, signature)
                .await
                .expect("Unable to mount");
        } else {
            let manifest: &PathBuf = match matches.get_one("manifest") {
                Some(s) => s,
                None => {
                    eprintln!("No build manifest was passed! Pass -h for help");
                    return;
                }
            };
            let build_manifest = &tokio::fs::read(manifest)
                .await
                .expect("Unable to read signature");

            let trust_cache: &PathBuf = match matches.get_one("trustcache") {
                Some(s) => s,
                None => {
                    eprintln!("No trust cache was passed! Pass -h for help");
                    return;
                }
            };
            let trust_cache = tokio::fs::read(trust_cache)
                .await
                .expect("Unable to read signature");

            let unique_chip_id =
                match lockdown_client.get_value(Some("UniqueChipID"), None).await {
                    Ok(u) => u,
                    Err(_) => {
                        lockdown_client
                            .start_session(&provider.get_pairing_file().await.unwrap())
                            .await
                            .expect("Unable to start session");
                        lockdown_client
                            .get_value(Some("UniqueChipID"), None)
                            .await
                            .expect("Unable to get UniqueChipID")
                    }
                }
                .as_unsigned_integer()
                .expect("Unexpected value for chip IP");

            mounter_client
                .mount_personalized_with_callback(
                    &*provider,
                    image,
                    trust_cache,
                    build_manifest,
                    None,
                    unique_chip_id,
                    async |((n, d), _)| {
                        let percent = (n as f64 / d as f64) * 100.0;
                        print!("\rProgress: {percent:.2}%");
                        std::io::stdout().flush().unwrap(); // Make sure it prints immediately
                        if n == d {
                            println!();
                        }
                    },
                    (),
                )
                .await
                .expect("Unable to mount");
        }
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
