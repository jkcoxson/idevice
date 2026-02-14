// Jackson Coxson

use idevice::{
    IdeviceService, RsdService, companion_proxy::CompanionProxy,
    core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};
use plist_macro::{pretty_print_dictionary, pretty_print_plist};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Apple Watch proxy")
        .with_subcommand(
            "list",
            JkCommand::new().help("List the companions on the device"),
        )
        .with_subcommand("listen", JkCommand::new().help("Listen for devices"))
        .with_subcommand(
            "get",
            JkCommand::new()
                .help("Gets a value from an AW")
                .with_argument(
                    JkArgument::new()
                        .with_help("The AW UDID to get from")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("The value to get")
                        .required(true),
                ),
        )
        .with_subcommand(
            "start",
            JkCommand::new()
                .help("Starts a service on the Apple Watch")
                .with_argument(
                    JkArgument::new()
                        .with_help("The port to listen on")
                        .required(true),
                )
                .with_argument(JkArgument::new().with_help("The service name")),
        )
        .with_subcommand(
            "stop",
            JkCommand::new()
                .help("Stops a service on the Apple Watch")
                .with_argument(
                    JkArgument::new()
                        .with_help("The port to stop")
                        .required(true),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core_device_proxy");
    let rsd_port = proxy.handshake.server_rsd_port;
    let mut provider = proxy
        .create_software_tunnel()
        .expect("no tunnel")
        .to_async_handle();
    let mut handshake = RsdHandshake::new(provider.connect(rsd_port).await.unwrap())
        .await
        .unwrap();
    let mut proxy = CompanionProxy::connect_rsd(&mut provider, &mut handshake)
        .await
        .expect("no companion proxy connect");

    // let mut proxy = CompanionProxy::connect(&*provider)
    //     .await
    //     .expect("Failed to connect to companion proxy");

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "list" => {
            proxy.get_device_registry().await.expect("Failed to show");
        }
        "listen" => {
            let mut stream = proxy.listen_for_devices().await.expect("Failed to show");
            while let Ok(v) = stream.next().await {
                println!("{}", pretty_print_dictionary(&v));
            }
        }
        "get" => {
            let key: String = sub_args.next_argument::<String>().expect("no value passed");
            let udid = sub_args
                .next_argument::<String>()
                .expect("no AW udid passed");

            match proxy.get_value(udid, key).await {
                Ok(value) => {
                    println!("{}", pretty_print_plist(&value));
                }
                Err(e) => {
                    eprintln!("Error getting value: {e}");
                }
            }
        }
        "start" => {
            let port: u16 = sub_args
                .next_argument::<String>()
                .expect("no port passed")
                .parse()
                .expect("not a number");
            let name = sub_args.next_argument::<String>();

            match proxy
                .start_forwarding_service_port(
                    port,
                    match &name {
                        Some(n) => Some(n.as_str()),
                        None => None,
                    },
                    None,
                )
                .await
            {
                Ok(value) => {
                    println!("started on port {value}");
                }
                Err(e) => {
                    eprintln!("Error starting: {e}");
                }
            }
        }
        "stop" => {
            let port: u16 = sub_args
                .next_argument::<String>()
                .expect("no port passed")
                .parse()
                .expect("not a number");

            if let Err(e) = proxy.stop_forwarding_service_port(port).await {
                eprintln!("Error starting: {e}");
            }
        }
        _ => unreachable!(),
    }
}
