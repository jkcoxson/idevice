// Jackson Coxson

use idevice::{
    IdeviceService, provider::IdeviceProvider, springboardservices::SpringBoardServicesClient,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist_macro::{plist_value_to_xml_bytes, pretty_print_plist};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage the springboard service")
        .with_subcommand(
            "get_icon_state",
            JkCommand::new()
                .help("Gets the icon state from the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("Version to query by")
                        .required(false),
                )
                .with_flag(
                    JkFlag::new("save")
                        .with_help("Path to save to")
                        .with_argument(JkArgument::new().required(true)),
                ),
        )
        .with_subcommand(
            "set_icon_state",
            JkCommand::new().help("Sets the icon state").with_argument(
                JkArgument::new()
                    .with_help("plist to set based on")
                    .required(true),
            ),
        )
        .with_subcommand(
            "get_wallpaper_preview",
            JkCommand::new()
                .help("Gets wallpaper preview")
                .with_subcommand("homescreen", JkCommand::new())
                .with_subcommand("lockscreen", JkCommand::new())
                .subcommand_required(true)
                .with_flag(
                    JkFlag::new("save")
                        .with_help("Path to save the wallpaper PNG file, or preview.png by default")
                        .with_argument(JkArgument::new().required(true)),
                ),
        )
        .with_subcommand(
            "get_interface_orientation",
            JkCommand::new().help("Gets the device's current screen orientation"),
        )
        .with_subcommand(
            "get_homescreen_icon_metrics",
            JkCommand::new().help("Gets home screen icon layout metrics"),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut sbc = SpringBoardServicesClient::connect(&*provider)
        .await
        .expect("Failed to connect to springboardservices");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand passed");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "get_icon_state" => {
            let version: Option<String> = sub_args.next_argument();
            let version = version.as_deref();
            let state = sbc
                .get_icon_state(version)
                .await
                .expect("Failed to get icon state");
            println!("{}", pretty_print_plist(&state));

            if let Some(path) = sub_args.get_flag::<String>("save") {
                tokio::fs::write(path, plist_value_to_xml_bytes(&state))
                    .await
                    .expect("Failed to save to path");
            }
        }
        "set_icon_state" => {
            let load_path = sub_args.next_argument::<String>().unwrap();
            let load = tokio::fs::read(load_path)
                .await
                .expect("Failed to read plist");
            let load: plist::Value =
                plist::from_bytes(&load).expect("Failed to parse bytes as plist");

            sbc.set_icon_state(load)
                .await
                .expect("Failed to set icon state");
        }
        "get_wallpaper_preview" => {
            let (wallpaper_type, _) = sub_args.first_subcommand().unwrap();

            let wallpaper = match wallpaper_type.as_str() {
                "homescreen" => sbc.get_home_screen_wallpaper_preview_pngdata().await,
                "lockscreen" => sbc.get_lock_screen_wallpaper_preview_pngdata().await,
                _ => panic!("Invalid wallpaper type. Use 'homescreen' or 'lockscreen'"),
            }
            .expect("Failed to get wallpaper preview");

            let save_path = sub_args
                .get_flag::<String>("save")
                .unwrap_or("preview.png".to_string());

            tokio::fs::write(&save_path, wallpaper)
                .await
                .expect("Failed to save wallpaper");
        }
        "get_interface_orientation" => {
            let orientation = sbc
                .get_interface_orientation()
                .await
                .expect("Failed to get interface orientation");
            println!("{:?}", orientation);
        }
        "get_homescreen_icon_metrics" => {
            let metrics = sbc
                .get_homescreen_icon_metrics()
                .await
                .expect("Failed to get homescreen icon metrics");
            let metrics_value = plist::Value::Dictionary(metrics);
            println!("{}", pretty_print_plist(&metrics_value));
        }
        _ => unreachable!(),
    }
}
