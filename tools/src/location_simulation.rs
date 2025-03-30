// Jackson Coxson
// Just lists apps for now

use clap::{Arg, Command};
use idevice::{core_device_proxy::CoreDeviceProxy, xpc::XPCDevice, IdeviceService};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

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
        println!("simulate_location - Sets the simlulated location on an iOS device");
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
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let mut adapter = proxy.create_software_tunnel().expect("no software tunnel");
    adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let client = XPCDevice::new(Box::new(adapter)).await.unwrap();

    // Get the debug proxy
    let service = client
        .services
        .get(idevice::dvt::SERVICE_NAME)
        .expect("Client did not contain DVT service")
        .to_owned();

    let mut adapter = client.into_inner();
    adapter.connect(service.port).await.unwrap();

    let mut rs_client = idevice::dvt::remote_server::RemoteServerClient::new(Box::new(adapter));
    rs_client.read_message(0).await.expect("no read??");

    let mut ls_client =
        idevice::dvt::location_simulation::LocationSimulationClient::new(&mut rs_client)
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
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
