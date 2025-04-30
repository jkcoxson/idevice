// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    lockdown::LockdownClient,
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection},
    IdeviceService,
};
use mac_address::get_mac_address;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("pair")
        .about("Pair with the device")
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
        println!("pair - pair with the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");

    let mut u = UsbmuxdConnection::default()
        .await
        .expect("Failed to connect to usbmuxd");
    let dev = match udid {
        Some(udid) => u
            .get_device(udid)
            .await
            .expect("Failed to get device with specific udid"),
        None => u
            .get_devices()
            .await
            .expect("Failed to get devices")
            .into_iter()
            .find(|x| x.connection_type == Connection::Usb)
            .expect("No devices connected via USB"),
    };
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-jkcoxson");

    let mut lockdown_client = match LockdownClient::connect(&provider).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Unable to connect to lockdown: {e:?}");
            return;
        }
    };
    let mac_address = get_mac_address()
        .expect("Failed to get MAC")
        .expect("No MAC??")
        .to_string();
    let id = uuid::Uuid::new_v4().to_string().to_uppercase();

    let mut pairing_file = lockdown_client
        .pair(id, mac_address, u.get_buid().await.unwrap())
        .await
        .expect("Failed to pair");

    // Test the pairing file
    lockdown_client
        .start_session(&pairing_file)
        .await
        .expect("Pairing file test failed");

    // Add the UDID (jitterbug spec)
    pairing_file.udid = Some(dev.udid);

    println!(
        "{}",
        String::from_utf8(pairing_file.serialize().unwrap()).unwrap()
    );
}
