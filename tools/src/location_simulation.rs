// Jackson Coxson
// Just lists apps for now

use clap::{Arg, Command};
use idevice::{IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake};

use idevice::dvt::location_simulation::LocationSimulationClient;
use idevice::services::simulate_location::LocationSimulationService;
mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("simulate_location")
        .about("Simulate device location")
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
        .subcommand(Command::new("clear").about("Clears the location set on the device"))
        .subcommand(
            Command::new("set")
                .about("Set the location on the device")
                .arg(Arg::new("latitude").required(true))
                .arg(Arg::new("longitude").required(true)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("simulate_location - Sets the simulated location on an iOS device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "simulate_location-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    if let Ok(proxy) = CoreDeviceProxy::connect(&*provider).await {
        let rsd_port = proxy.handshake.server_rsd_port;

        let adapter = proxy.create_software_tunnel().expect("no software tunnel");
        let mut adapter = adapter.to_async_handle();
        let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

        // Make the connection to RemoteXPC
        let mut handshake = RsdHandshake::new(stream).await.unwrap();

        let mut ls_client = idevice::dvt::remote_server::RemoteServerClient::connect_rsd(
            &mut adapter,
            &mut handshake,
        )
        .await
        .expect("Failed to connect");
        ls_client.read_message(0).await.expect("no read??");
        let mut ls_client = LocationSimulationClient::new(&mut ls_client)
            .await
            .expect("Unable to get channel for location simulation");
        if matches.subcommand_matches("clear").is_some() {
            ls_client.clear().await.expect("Unable to clear");
            println!("Location cleared!");
        } else if let Some(matches) = matches.subcommand_matches("set") {
            let latitude: &String = match matches.get_one("latitude") {
                Some(l) => l,
                None => {
                    eprintln!("No latitude passed! Pass -h for help");
                    return;
                }
            };
            let latitude: f64 = latitude.parse().expect("Failed to parse as float");
            let longitude: &String = match matches.get_one("longitude") {
                Some(l) => l,
                None => {
                    eprintln!("No longitude passed! Pass -h for help");
                    return;
                }
            };
            let longitude: f64 = longitude.parse().expect("Failed to parse as float");
            ls_client
                .set(latitude, longitude)
                .await
                .expect("Failed to set location");

            println!("Location set!");
            println!("Press ctrl-c to stop");
            loop {
                ls_client
                    .set(latitude, longitude)
                    .await
                    .expect("Failed to set location");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        } else {
            eprintln!("Invalid usage, pass -h for help");
        }
    } else {
        let mut location_client = match LocationSimulationService::connect(&*provider).await {
            Ok(client) => client,
            Err(e) => {
                eprintln!(
                    "Unable to connect to simulate_location service: {e} Ensure Developer Disk Image is mounted."
                );
                return;
            }
        };
        if matches.subcommand_matches("clear").is_some() {
            location_client.clear().await.expect("Unable to clear");
            println!("Location cleared!");
        } else if let Some(matches) = matches.subcommand_matches("set") {
            let latitude: &String = match matches.get_one("latitude") {
                Some(l) => l,
                None => {
                    eprintln!("No latitude passed! Pass -h for help");
                    return;
                }
            };

            let longitude: &String = match matches.get_one("longitude") {
                Some(l) => l,
                None => {
                    eprintln!("No longitude passed! Pass -h for help");
                    return;
                }
            };
            location_client
                .set(latitude, longitude)
                .await
                .expect("Failed to set location");

            println!("Location set!");
        } else {
            eprintln!("Invalid usage, pass -h for help");
        }
    };

    return;
}
