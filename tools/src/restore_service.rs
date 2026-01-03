// Jackson Coxson

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    restore_service::RestoreServiceClient, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};
use plist_macro::pretty_print_dictionary;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the Restore Service service")
        .with_subcommand("delay", JkCommand::new().help("Delay recovery image"))
        .with_subcommand("recovery", JkCommand::new().help("Enter recovery mode"))
        .with_subcommand("reboot", JkCommand::new().help("Reboots the device"))
        .with_subcommand(
            "preflightinfo",
            JkCommand::new().help("Gets the preflight info"),
        )
        .with_subcommand("nonces", JkCommand::new().help("Gets the nonces"))
        .with_subcommand(
            "app_parameters",
            JkCommand::new().help("Gets the app parameters"),
        )
        .with_subcommand(
            "restore_lang",
            JkCommand::new()
                .help("Restores the language")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("Language to restore"),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();
    println!("{:?}", handshake.services);

    let mut restore_client = RestoreServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("Unable to connect to service");

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "recovery" => {
            restore_client
                .enter_recovery()
                .await
                .expect("command failed");
        }
        "reboot" => {
            restore_client.reboot().await.expect("command failed");
        }
        "preflightinfo" => {
            let info = restore_client
                .get_preflightinfo()
                .await
                .expect("command failed");
            println!("{}", pretty_print_dictionary(&info));
        }
        "nonces" => {
            let nonces = restore_client.get_nonces().await.expect("command failed");
            println!("{}", pretty_print_dictionary(&nonces));
        }
        "app_parameters" => {
            let params = restore_client
                .get_app_parameters()
                .await
                .expect("command failed");
            println!("{}", pretty_print_dictionary(&params));
        }
        "restore_lang" => {
            let lang: String = sub_args
                .next_argument::<String>()
                .expect("No language passed");
            restore_client
                .restore_lang(lang)
                .await
                .expect("failed to restore lang");
        }
        _ => unreachable!(),
    }
}
