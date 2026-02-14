// Jackson Coxson

use std::path::PathBuf;

use idevice::{
    IdeviceService,
    afc::{AfcClient, opcode::AfcFopenMode},
    house_arrest::HouseArrestClient,
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

const DOCS_HELP: &str = "Read the documents from a bundle. Note that when vending documents, you can only access files in /Documents";

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage files in the AFC jail of a device")
        .with_flag(
            JkFlag::new("documents")
                .with_help(DOCS_HELP)
                .with_argument(JkArgument::new().required(true)),
        )
        .with_flag(
            JkFlag::new("container")
                .with_help("Read the container contents of a bundle")
                .with_argument(JkArgument::new().required(true)),
        )
        .with_subcommand(
            "list",
            JkCommand::new()
                .help("Lists the items in the directory")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("The directory to list in"),
                ),
        )
        .with_subcommand(
            "download",
            JkCommand::new()
                .help("Download a file")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path in the AFC jail"),
                )
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to save file to"),
                ),
        )
        .with_subcommand(
            "upload",
            JkCommand::new()
                .help("Upload a file")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to the file to upload"),
                )
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to save file to in the AFC jail"),
                ),
        )
        .with_subcommand(
            "mkdir",
            JkCommand::new().help("Create a folder").with_argument(
                JkArgument::new()
                    .required(true)
                    .with_help("Path to the folder to create in the AFC jail"),
            ),
        )
        .with_subcommand(
            "remove",
            JkCommand::new().help("Remove a file").with_argument(
                JkArgument::new()
                    .required(true)
                    .with_help("Path to the file to remove"),
            ),
        )
        .with_subcommand(
            "remove_all",
            JkCommand::new().help("Remove a folder").with_argument(
                JkArgument::new()
                    .required(true)
                    .with_help("Path to the folder to remove"),
            ),
        )
        .with_subcommand(
            "info",
            JkCommand::new()
                .help("Get info about a file")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path to the file to get info for"),
                ),
        )
        .with_subcommand(
            "device_info",
            JkCommand::new().help("Get info about the device"),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut afc_client = if let Some(bundle_id) = arguments.get_flag::<String>("container") {
        let h = HouseArrestClient::connect(&*provider)
            .await
            .expect("Failed to connect to house arrest");
        h.vend_container(bundle_id)
            .await
            .expect("Failed to vend container")
    } else if let Some(bundle_id) = arguments.get_flag::<String>("documents") {
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

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();
    match sub_name.as_str() {
        "list" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            let res = afc_client
                .list_dir(&path)
                .await
                .expect("Failed to read dir");
            println!("{path}\n{res:#?}");
        }
        "mkdir" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            afc_client.mk_dir(path).await.expect("Failed to mkdir");
        }
        "download" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            let save = sub_args.next_argument::<String>().expect("No path passed");

            let mut file = afc_client
                .open(path, AfcFopenMode::RdOnly)
                .await
                .expect("Failed to open");

            let res = file.read_entire().await.expect("Failed to read");
            tokio::fs::write(save, res)
                .await
                .expect("Failed to write to file");
        }
        "upload" => {
            let file = sub_args.next_argument::<PathBuf>().expect("No path passed");
            let path = sub_args.next_argument::<String>().expect("No path passed");

            let bytes = tokio::fs::read(file).await.expect("Failed to read file");
            let mut file = afc_client
                .open(path, AfcFopenMode::WrOnly)
                .await
                .expect("Failed to open");

            file.write_entire(&bytes)
                .await
                .expect("Failed to upload bytes");
        }
        "remove" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            afc_client.remove(path).await.expect("Failed to remove");
        }
        "remove_all" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            afc_client.remove_all(path).await.expect("Failed to remove");
        }
        "info" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            let res = afc_client
                .get_file_info(path)
                .await
                .expect("Failed to get file info");
            println!("{res:#?}");
        }
        "device_info" => {
            let res = afc_client
                .get_device_info()
                .await
                .expect("Failed to get file info");
            println!("{res:#?}");
        }
        _ => unreachable!(),
    }
}
