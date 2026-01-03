// Jackson Coxson
// idevice Rust implementation of libimobiledevice's idevicediagnostics

use idevice::{
    IdeviceService, provider::IdeviceProvider, services::diagnostics_relay::DiagnosticsRelayClient,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the diagnostics interface of a device")
        .with_subcommand(
            "ioregistry",
            JkCommand::new()
                .help("Print IORegistry information")
                .with_flag(
                    JkFlag::new("plane")
                        .with_help("IORegistry plane to query (e.g., IODeviceTree, IOService)")
                        .with_argument(JkArgument::new().required(true)),
                )
                .with_flag(
                    JkFlag::new("name")
                        .with_help("Entry name to filter by")
                        .with_argument(JkArgument::new().required(true)),
                )
                .with_flag(
                    JkFlag::new("class")
                        .with_help("Entry class to filter by")
                        .with_argument(JkArgument::new().required(true)),
                ),
        )
        .with_subcommand(
            "mobilegestalt",
            JkCommand::new()
                .help("Print MobileGestalt information")
                .with_argument(
                    JkArgument::new()
                        .with_help("Comma-separated list of keys to query")
                        .required(true),
                ),
        )
        .with_subcommand(
            "gasguage",
            JkCommand::new().help("Print gas gauge (battery) information"),
        )
        .with_subcommand(
            "nand",
            JkCommand::new().help("Print NAND flash information"),
        )
        .with_subcommand(
            "all",
            JkCommand::new().help("Print all available diagnostics information"),
        )
        .with_subcommand(
            "wifi",
            JkCommand::new().help("Print WiFi diagnostics information"),
        )
        .with_subcommand(
            "goodbye",
            JkCommand::new().help("Send Goodbye to diagnostics relay"),
        )
        .with_subcommand("restart", JkCommand::new().help("Restart the device"))
        .with_subcommand("shutdown", JkCommand::new().help("Shutdown the device"))
        .with_subcommand("sleep", JkCommand::new().help("Put the device to sleep"))
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut diagnostics_client = match DiagnosticsRelayClient::connect(&*provider).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Unable to connect to diagnostics relay: {e:?}");
            return;
        }
    };

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_matches = sub_args.clone();

    match sub_name.as_str() {
        "ioregistry" => {
            handle_ioregistry(&mut diagnostics_client, &sub_matches).await;
        }
        "mobilegestalt" => {
            handle_mobilegestalt(&mut diagnostics_client, &mut sub_matches).await;
        }
        "gasguage" => {
            handle_gasguage(&mut diagnostics_client).await;
        }
        "nand" => {
            handle_nand(&mut diagnostics_client).await;
        }
        "all" => {
            handle_all(&mut diagnostics_client).await;
        }
        "wifi" => {
            handle_wifi(&mut diagnostics_client).await;
        }
        "restart" => {
            handle_restart(&mut diagnostics_client).await;
        }
        "shutdown" => {
            handle_shutdown(&mut diagnostics_client).await;
        }
        "sleep" => {
            handle_sleep(&mut diagnostics_client).await;
        }
        "goodbye" => {
            handle_goodbye(&mut diagnostics_client).await;
        }
        _ => unreachable!(),
    }
}

async fn handle_ioregistry(client: &mut DiagnosticsRelayClient, matches: &CollectedArguments) {
    let plane = matches.get_flag::<String>("plane");
    let name = matches.get_flag::<String>("name");
    let class = matches.get_flag::<String>("class");

    let plane = plane.as_deref();
    let name = name.as_deref();
    let class = class.as_deref();

    match client.ioregistry(plane, name, class).await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No IORegistry data returned");
        }
        Err(e) => {
            eprintln!("Failed to get IORegistry data: {e:?}");
        }
    }
}

async fn handle_mobilegestalt(
    client: &mut DiagnosticsRelayClient,
    matches: &mut CollectedArguments,
) {
    let keys = matches.next_argument::<String>().unwrap();
    let keys = keys.split(',').map(|x| x.to_string()).collect();

    match client.mobilegestalt(Some(keys)).await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No MobileGestalt data returned");
        }
        Err(e) => {
            eprintln!("Failed to get MobileGestalt data: {e:?}");
        }
    }
}

async fn handle_gasguage(client: &mut DiagnosticsRelayClient) {
    match client.gasguage().await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No gas gauge data returned");
        }
        Err(e) => {
            eprintln!("Failed to get gas gauge data: {e:?}");
        }
    }
}

async fn handle_nand(client: &mut DiagnosticsRelayClient) {
    match client.nand().await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No NAND data returned");
        }
        Err(e) => {
            eprintln!("Failed to get NAND data: {e:?}");
        }
    }
}

async fn handle_all(client: &mut DiagnosticsRelayClient) {
    match client.all().await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No diagnostics data returned");
        }
        Err(e) => {
            eprintln!("Failed to get all diagnostics data: {e:?}");
        }
    }
}

async fn handle_wifi(client: &mut DiagnosticsRelayClient) {
    match client.wifi().await {
        Ok(Some(data)) => {
            println!("{data:#?}");
        }
        Ok(None) => {
            println!("No WiFi diagnostics returned");
        }
        Err(e) => {
            eprintln!("Failed to get WiFi diagnostics: {e:?}");
        }
    }
}

async fn handle_restart(client: &mut DiagnosticsRelayClient) {
    match client.restart().await {
        Ok(()) => {
            println!("Device restart command sent successfully");
        }
        Err(e) => {
            eprintln!("Failed to restart device: {e:?}");
        }
    }
}

async fn handle_shutdown(client: &mut DiagnosticsRelayClient) {
    match client.shutdown().await {
        Ok(()) => {
            println!("Device shutdown command sent successfully");
        }
        Err(e) => {
            eprintln!("Failed to shutdown device: {e:?}");
        }
    }
}

async fn handle_sleep(client: &mut DiagnosticsRelayClient) {
    match client.sleep().await {
        Ok(()) => {
            println!("Device sleep command sent successfully");
        }
        Err(e) => {
            eprintln!("Failed to put device to sleep: {e:?}");
        }
    }
}

async fn handle_goodbye(client: &mut DiagnosticsRelayClient) {
    match client.goodbye().await {
        Ok(()) => println!("Goodbye acknowledged by device"),
        Err(e) => eprintln!("Goodbye failed: {e:?}"),
    }
}
