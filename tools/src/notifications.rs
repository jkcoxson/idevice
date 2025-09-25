//  Monitor memory and app notifications

use clap::{Arg, Command};
use idevice::{IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake};
mod common;

#[tokio::main]
async fn main() {
    env_logger::init();
    let matches = Command::new("notifications")
        .about("start notifications")
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
        .get_matches();

    if matches.get_flag("about") {
        print!("notifications - start notifications to ios device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "notifications-jkcoxson").await {
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

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");

    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();
    let mut ts_client =
        idevice::dvt::remote_server::RemoteServerClient::connect_rsd(&mut adapter, &mut handshake)
            .await
            .expect("Failed to connect");
    ts_client.read_message(0).await.expect("no read??");
    let mut notification_client =
        idevice::dvt::notifications::NotificationsClient::new(&mut ts_client)
            .await
            .expect("Unable to get channel for notifications");
    notification_client
        .start_notifications()
        .await
        .expect("Failed to start notifications");

    // Handle Ctrl+C gracefully
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutdown signal received, exiting.");
                break;
            }

            // Branch 2: Wait for the next batch of notifications.
            result = notification_client.get_notifications() => {
                if let Err(e) = result {
                    eprintln!("Failed to get notifications: {}", e);
                } else {
                    println!("Received notifications: {:#?}", result.unwrap());
                }
            }
        }
    }
}
