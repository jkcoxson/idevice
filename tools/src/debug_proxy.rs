// Jackson Coxson

use std::io::Write;

use clap::{Arg, Command};
use idevice::{
    core_device_proxy::CoreDeviceProxy, debug_proxy::DebugProxyClient, rsd::RsdClient,
    tcp::stream::AdapterStream, IdeviceService,
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("remotexpc")
        .about("Get services from RemoteXPC")
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
            Arg::new("tunneld")
                .long("tunneld")
                .help("Use tunneld")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("debug_proxy - connect to the debug proxy and run commands");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let provider =
        match common::get_provider(udid, host, pairing_file, "debug-proxy-jkcoxson").await {
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
    let stream = AdapterStream::connect(&mut adapter, rsd_port)
        .await
        .expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut client = RsdClient::new(stream).await.unwrap();

    // Get the debug proxy
    let service = client
        .get_services()
        .await
        .unwrap()
        .get(idevice::debug_proxy::SERVICE_NAME)
        .expect("Client did not contain debug proxy service")
        .to_owned();

    let stream = AdapterStream::connect(&mut adapter, service.port)
        .await
        .unwrap();

    let mut dp = DebugProxyClient::new(stream);

    println!("Shell connected!");
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();

        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).unwrap();

        let buf = buf.trim();

        if buf == "exit" {
            break;
        }

        let res = dp.send_command(buf.into()).await.expect("Failed to send");
        if let Some(res) = res {
            println!("{res}");
        }
    }
}
