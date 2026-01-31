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
                .with_flag(
                    JkFlag::new("type")
                        .with_help("Wallpaper type: 'homescreen' or 'lockscreen'")
                        .with_argument(JkArgument::new().required(true)),
                )
                .with_flag(
                    JkFlag::new("save")
                        .with_help("Path to save the wallpaper PNG file")
                        .with_argument(JkArgument::new().required(true)),
                ),
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
            let wallpaper_type = sub_args
                .get_flag::<String>("type")
                .expect("Wallpaper type required");

            let wallpaper = match wallpaper_type.as_str() {
                "homescreen" => sbc.get_home_screen_wallpaper_preview_pngdata().await,
                "lockscreen" => sbc.get_lock_screen_wallpaper_preview_pngdata().await,
                _ => panic!("Invalid wallpaper type. Use 'homescreen' or 'lockscreen'"),
            }
            .expect("Failed to get wallpaper preview");

            if let Some(path) = sub_args.get_flag::<String>("save") {
                tokio::fs::write(&path, wallpaper)
                    .await
                    .expect("Failed to save wallpaper");
            }
        }
        _ => unreachable!(),
    }
}
