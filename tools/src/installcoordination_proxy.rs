// Jackson Coxson

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    installcoordination_proxy::InstallcoordinationProxy, provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the RemoteXPC installation coordination proxy")
        .with_subcommand(
            "info",
            JkCommand::new()
                .help("Get info about an app on the device")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("The bundle ID to query"),
                ),
        )
        .with_subcommand(
            "uninstall",
            JkCommand::new()
                .help("Uninstalls an app on the device")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("The bundle ID to delete"),
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
    adapter
        .pcap("/Users/jacksoncoxson/Desktop/hmmmm.pcap")
        .await
        .unwrap();

    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut icp = InstallcoordinationProxy::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "info" => {
            let bundle_id: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };

            let res = icp
                .query_app_path(bundle_id.as_str())
                .await
                .expect("no info");
            println!("Path: {res}");
        }
        "uninstall" => {
            let bundle_id: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };

            icp.uninstall_app(bundle_id.as_str())
                .await
                .expect("uninstall failed");
        }
        _ => unreachable!(),
    }
}
