// Jackson Coxson
// Just lists apps for now

use idevice::{
    IdeviceService, installation_proxy::InstallationProxyClient, provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage files in the AFC jail of a device")
        .with_subcommand(
            "lookup",
            JkCommand::new().help("Gets the apps on the device"),
        )
        .with_subcommand(
            "browse",
            JkCommand::new().help("Browses the apps on the device"),
        )
        .with_subcommand(
            "check_capabilities",
            JkCommand::new().help("Check the capabilities"),
        )
        .with_subcommand(
            "install",
            JkCommand::new()
                .help("Install an app in the AFC jail")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Path in the AFC jail"),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut instproxy_client = InstallationProxyClient::connect(&*provider)
        .await
        .expect("Unable to connect to instproxy");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("no sub arg");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "lookup" => {
            let apps = instproxy_client.get_apps(Some("User"), None).await.unwrap();
            for app in apps.keys() {
                println!("{app}");
            }
        }
        "browse" => {
            instproxy_client.browse(None).await.expect("browse failed");
        }
        "check_capabilities" => {
            instproxy_client
                .check_capabilities_match(Vec::new(), None)
                .await
                .expect("check failed");
        }
        "install" => {
            let path: String = match sub_args.next_argument() {
                Some(p) => p,
                None => {
                    eprintln!("No path passed, pass -h for help");
                    return;
                }
            };

            instproxy_client
                .install_with_callback(
                    path,
                    None,
                    async |(percentage, _)| {
                        println!("Installing: {percentage}");
                    },
                    (),
                )
                .await
                .expect("Failed to install")
        }
        _ => unreachable!(),
    }
}
