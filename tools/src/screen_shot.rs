use clap::{Arg, Command};
use idevice::{IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake};
use std::fs;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();
    let matches = Command::new("screen_shot")
        .about("take screenshot")
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
            Arg::new("output")
                .short('o')
                .long("output")
                .value_name("FILE")
                .help("Output file path for the screenshot (default: ./screenshot.png)")
                .default_value("screenshot.png"),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        print!("screen_shot - take screenshot from ios device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let output_path = matches.get_one::<String>("output").unwrap();

    let provider =
        match common::get_provider(udid, host, pairing_file, "take_screenshot-jkcoxson").await {
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

    let mut ts_client = idevice::dvt::screen_shot::ScreenShotClient::new(&mut ts_client)
        .await
        .expect("Unable to get channel for take screenshot");
    let res = ts_client
        .take_screenshot()
        .await
        .expect("Failed to take screenshot");

    match fs::write(output_path, &res) {
        Ok(_) => println!("Screenshot saved to: {}", output_path),
        Err(e) => eprintln!("Failed to write screenshot to file: {}", e),
    }
}
