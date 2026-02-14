// Jackson Coxson
// Just lists apps for now

use idevice::provider::IdeviceProvider;
use idevice::{IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake};

use idevice::dvt::location_simulation::LocationSimulationClient;
use idevice::services::simulate_location::LocationSimulationService;
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Simulate device location")
        .with_subcommand(
            "clear",
            JkCommand::new().help("Clears the location set on the device"),
        )
        .with_subcommand(
            "set",
            JkCommand::new()
                .help("Set the location on the device")
                .with_argument(JkArgument::new().with_help("latitude").required(true))
                .with_argument(JkArgument::new().with_help("longitutde").required(true)),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments.first_subcommand().expect("No sub arg passed");
    let mut sub_args = sub_args.clone();

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
        match sub_name.as_str() {
            "clear" => {
                ls_client.clear().await.expect("Unable to clear");
                println!("Location cleared!");
            }
            "set" => {
                let latitude: String = match sub_args.next_argument() {
                    Some(l) => l,
                    None => {
                        eprintln!("No latitude passed! Pass -h for help");
                        return;
                    }
                };
                let latitude: f64 = latitude.parse().expect("Failed to parse as float");
                let longitude: String = match sub_args.next_argument() {
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
            }
            _ => unreachable!(),
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

        match sub_name.as_str() {
            "clear" => {
                location_client.clear().await.expect("Unable to clear");
                println!("Location cleared!");
            }
            "set" => {
                let latitude: String = match sub_args.next_argument() {
                    Some(l) => l,
                    None => {
                        eprintln!("No latitude passed! Pass -h for help");
                        return;
                    }
                };

                let longitude: String = match sub_args.next_argument() {
                    Some(l) => l,
                    None => {
                        eprintln!("No longitude passed! Pass -h for help");
                        return;
                    }
                };
                location_client
                    .set(latitude.as_str(), longitude.as_str())
                    .await
                    .expect("Failed to set location");

                println!("Location set!");
            }
            _ => unreachable!(),
        }
    };
}
