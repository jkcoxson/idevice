// Jackson Coxson

use idevice::{IdeviceService, amfi::AmfiClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Mess with devleoper mode")
        .with_subcommand(
            "show",
            JkCommand::new().help("Shows the developer mode option in settings"),
        )
        .with_subcommand("enable", JkCommand::new().help("Enables developer mode"))
        .with_subcommand(
            "accept",
            JkCommand::new().help("Shows the accept dialogue for developer mode"),
        )
        .with_subcommand(
            "status",
            JkCommand::new().help("Gets the developer mode status"),
        )
        .with_subcommand(
            "trust",
            JkCommand::new()
                .help("Trusts an app signer")
                .with_argument(JkArgument::new().with_help("UUID").required(true)),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut amfi_client = AmfiClient::connect(&*provider)
        .await
        .expect("Failed to connect to amfi");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand passed");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "show" => {
            amfi_client
                .reveal_developer_mode_option_in_ui()
                .await
                .expect("Failed to show");
        }
        "enable" => {
            amfi_client
                .enable_developer_mode()
                .await
                .expect("Failed to show");
        }
        "accept" => {
            amfi_client
                .accept_developer_mode()
                .await
                .expect("Failed to show");
        }
        "status" => {
            let status = amfi_client
                .get_developer_mode_status()
                .await
                .expect("Failed to get status");
            println!("Enabled: {status}");
        }
        "trust" => {
            let uuid: String = match sub_args.next_argument() {
                Some(u) => u,
                None => {
                    eprintln!("No UUID passed. Invalid usage, pass -h for help");
                    return;
                }
            };
            let status = amfi_client
                .trust_app_signer(uuid)
                .await
                .expect("Failed to get state");
            println!("Enabled: {status}");
        }
        _ => unreachable!(),
    }
}
