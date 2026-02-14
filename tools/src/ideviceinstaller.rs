// A minimal ideviceinstaller-like CLI to install/upgrade apps

use idevice::{provider::IdeviceProvider, utils::installation};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage files in the AFC jail of a device")
        .with_subcommand(
            "install",
            JkCommand::new()
                .help("Install a local .ipa or directory")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to the .ipa or directory containing the app"),
                ),
        )
        .with_subcommand(
            "upgrade",
            JkCommand::new()
                .help("Install a local .ipa or directory")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to the .ipa or directory containing the app"),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments.first_subcommand().expect("no sub arg");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "install" => {
            let path: String = sub_args.next_argument().expect("required");
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
        }
        "upgrade" => {
            let path: String = sub_args.next_argument().expect("required");
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
        }
        _ => unreachable!(),
    }
}
