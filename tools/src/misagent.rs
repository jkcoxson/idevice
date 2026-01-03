// Jackson Coxson

use std::path::PathBuf;

use idevice::{IdeviceService, misagent::MisagentClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage provisioning profiles on the device")
        .with_subcommand(
            "list",
            JkCommand::new()
                .help("List profiles installed on the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to save profiles from the device")
                        .required(false),
                ),
        )
        .with_subcommand(
            "remove",
            JkCommand::new()
                .help("Remove a profile installed on the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("ID of the profile to remove")
                        .required(true),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    tracing_subscriber::fmt::init();

    let mut misagent_client = MisagentClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand passed");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "list" => {
            let images = misagent_client
                .copy_all()
                .await
                .expect("Unable to get images");
            if let Some(path) = sub_args.next_argument::<PathBuf>() {
                tokio::fs::create_dir_all(&path)
                    .await
                    .expect("Unable to create save DIR");

                for (index, image) in images.iter().enumerate() {
                    let f = path.join(format!("{index}.pem"));
                    tokio::fs::write(f, image)
                        .await
                        .expect("Failed to write image");
                }
            }
        }
        "remove" => {
            let id = sub_args.next_argument::<String>().expect("No ID passed");
            misagent_client
                .remove(id.as_str())
                .await
                .expect("Failed to remove");
        }
        _ => unreachable!(),
    }
}
