// Jackson Coxson

use idevice::{IdeviceService, preboard_service::PreboardServiceClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the preboard service")
        .with_subcommand("create", JkCommand::new().help("Create a stashbag??"))
        .with_subcommand("commit", JkCommand::new().help("Commit a stashbag??"))
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut pc = PreboardServiceClient::connect(&*provider)
        .await
        .expect("Failed to connect to Preboard");

    let (sub_name, _) = arguments.first_subcommand().unwrap();

    match sub_name.as_str() {
        "create" => {
            pc.create_stashbag(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0])
                .await
                .expect("Failed to create");
        }
        "commit" => {
            pc.commit_stashbag(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 0])
                .await
                .expect("Failed to create");
        }
        _ => unreachable!(),
    }
}
