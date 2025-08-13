// Jackson Coxson
// idevice Rust implementation of libimobiledevice's idevicediagnostics

use clap::{Arg, Command, ArgMatches};
use idevice::{services::diagnostics_relay::DiagnosticsRelayClient, IdeviceService};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("idevicediagnostics")
        .about("Interact with the diagnostics interface of a device")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("IP address of the device"),
        )
        .arg(
            Arg::new("pairing_file")
                .long("pairing-file")
                .value_name("PATH")
                .help("Path to the pairing file"),
        )
        .arg(
            Arg::new("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)")
                .index(1),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(
            Command::new("ioregistry")
                .about("Print IORegistry information")
                .arg(
                    Arg::new("plane")
                        .long("plane")
                        .value_name("PLANE")
                        .help("IORegistry plane to query (e.g., IODeviceTree, IOService)")
                )
                .arg(
                    Arg::new("name")
                        .long("name")
                        .value_name("NAME")
                        .help("Entry name to filter by")
                )
                .arg(
                    Arg::new("class")
                        .long("class")
                        .value_name("CLASS")
                        .help("Entry class to filter by")
                )
        )
        .subcommand(
            Command::new("mobilegestalt")
                .about("Print MobileGestalt information")
                .arg(
                    Arg::new("keys")
                        .long("keys")
                        .value_name("KEYS")
                        .help("Comma-separated list of keys to query")
                        .value_delimiter(',')
                        .num_args(1..)
                )
        )
        .subcommand(
            Command::new("gasguage")
                .about("Print gas gauge (battery) information")
        )
        .subcommand(
            Command::new("nand")
                .about("Print NAND flash information")
        )
        .subcommand(
            Command::new("all")
                .about("Print all available diagnostics information")
        )
        .subcommand(
            Command::new("wifi")
                .about("Print WiFi diagnostics information")
        )
        .subcommand(
            Command::new("goodbye")
                .about("Send Goodbye to diagnostics relay")
        )
        .subcommand(
            Command::new("restart")
                .about("Restart the device")
        )
        .subcommand(
            Command::new("shutdown")
                .about("Shutdown the device")
        )
        .subcommand(
            Command::new("sleep")
                .about("Put the device to sleep")
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("idevicediagnostics - interact with the diagnostics interface of a device. Reimplementation of libimobiledevice's binary.");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "idevicediagnostics-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let mut diagnostics_client = match DiagnosticsRelayClient::connect(&*provider).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Unable to connect to diagnostics relay: {e:?}");
            return;
        }
    };

    match matches.subcommand() {
        Some(("ioregistry", sub_matches)) => {
            handle_ioregistry(&mut diagnostics_client, sub_matches).await;
        }
        Some(("mobilegestalt", sub_matches)) => {
            handle_mobilegestalt(&mut diagnostics_client, sub_matches).await;
        }
        Some(("gasguage", _)) => {
            handle_gasguage(&mut diagnostics_client).await;
        }
        Some(("nand", _)) => {
            handle_nand(&mut diagnostics_client).await;
        }
        Some(("all", _)) => {
            handle_all(&mut diagnostics_client).await;
        }
        Some(("wifi", _)) => {
            handle_wifi(&mut diagnostics_client).await;
        }
        Some(("restart", _)) => {
            handle_restart(&mut diagnostics_client).await;
        }
        Some(("shutdown", _)) => {
            handle_shutdown(&mut diagnostics_client).await;
        }
        Some(("sleep", _)) => {
            handle_sleep(&mut diagnostics_client).await;
        }
        Some(("goodbye", _)) => {
            handle_goodbye(&mut diagnostics_client).await;
        }
        _ => {
            eprintln!("No subcommand specified. Use --help for usage information.");
        }
    }
}

async fn handle_ioregistry(client: &mut DiagnosticsRelayClient, matches: &ArgMatches) {
    let plane = matches.get_one::<String>("plane").map(|s| s.as_str());
    let name = matches.get_one::<String>("name").map(|s| s.as_str());
    let class = matches.get_one::<String>("class").map(|s| s.as_str());

    match client.ioregistry(plane, name, class).await {
        Ok(Some(data)) => {
            println!("{:#?}", data);
        }
        Ok(None) => {
            println!("No IORegistry data returned");
        }
        Err(e) => {
            eprintln!("Failed to get IORegistry data: {e:?}");
        }
    }
}

async fn handle_mobilegestalt(client: &mut DiagnosticsRelayClient, matches: &ArgMatches) {
    let keys = matches.get_many::<String>("keys")
        .map(|values| values.map(|s| s.to_string()).collect::<Vec<_>>());

    match client.mobilegestalt(keys).await {
        Ok(Some(data)) => {
            println!("{:#?}", data);
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
            println!("{:#?}", data);
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
            println!("{:#?}", data);
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
            println!("{:#?}", data);
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
            println!("{:#?}", data);
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