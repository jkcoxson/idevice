// Jackson Coxson
// Just lists apps for now

use std::{io::Write, path::PathBuf};

use idevice::{
    IdeviceService, lockdown::LockdownClient, mobile_image_mounter::ImageMounter,
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist_macro::pretty_print_plist;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage mounts on an iOS device")
        .with_subcommand(
            "list",
            JkCommand::new().help("Lists the images mounted on the device"),
        )
        .with_subcommand(
            "lookup",
            JkCommand::new().help("Lookup the image signature on the device"),
        )
        .with_subcommand(
            "unmount",
            JkCommand::new().help("Unmounts the developer disk image"),
        )
        .with_subcommand(
            "mount",
            JkCommand::new()
                .help("Mounts the developer disk image")
                .with_flag(
                    JkFlag::new("image")
                        .with_short("i")
                        .with_argument(JkArgument::new().required(true))
                        .with_help("A path to the image to mount")
                        .required(true),
                )
                .with_flag(
                    JkFlag::new("manifest")
                        .with_short("b")
                        .with_argument(JkArgument::new())
                        .with_help("the build manifest (iOS 17+)"),
                )
                .with_flag(
                    JkFlag::new("trustcache")
                        .with_short("t")
                        .with_argument(JkArgument::new())
                        .with_help("the trust cache (iOS 17+)"),
                )
                .with_flag(
                    JkFlag::new("signature")
                        .with_short("s")
                        .with_argument(JkArgument::new())
                        .with_help("the image signature (iOS < 17.0"),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
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

    let (subcommand, sub_args) = arguments
        .first_subcommand()
        .expect("No subcommand passed! Pass -h for help");

    match subcommand.as_str() {
        "list" => {
            let images = mounter_client
                .copy_devices()
                .await
                .expect("Unable to get images");
            for i in images {
                println!("{}", pretty_print_plist(&i));
            }
        }
        "lookup" => {
            let sig = mounter_client
                .lookup_image(if product_version < 17 {
                    "Developer"
                } else {
                    "Personalized"
                })
                .await
                .expect("Failed to lookup images");
            println!("Image signature: {sig:02X?}");
        }
        "unmount" => {
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
        }
        "mount" => {
            let image: PathBuf = match sub_args.get_flag("image") {
                Some(i) => i,
                None => {
                    eprintln!("No image was passed! Pass -h for help");
                    return;
                }
            };
            let image = tokio::fs::read(image).await.expect("Unable to read image");
            if product_version < 17 {
                let signature: PathBuf = match sub_args.get_flag("signature") {
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
                let manifest: PathBuf = match sub_args.get_flag("manifest") {
                    Some(s) => s,
                    None => {
                        eprintln!("No build manifest was passed! Pass -h for help");
                        return;
                    }
                };
                let build_manifest = &tokio::fs::read(manifest)
                    .await
                    .expect("Unable to read signature");

                let trust_cache: PathBuf = match sub_args.get_flag("trustcache") {
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
        }
        _ => unreachable!(),
    }
}
