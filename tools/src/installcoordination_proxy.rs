// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    installcoordination_proxy::InstallcoordinationProxy, rsd::RsdHandshake,
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("installationcoordination_proxy")
        .about("")
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
        .subcommand(
            Command::new("info")
                .about("Get info about an app on the device")
                .arg(
                    Arg::new("bundle_id")
                        .required(true)
                        .help("The bundle ID to query"),
                ),
        )
        .subcommand(
            Command::new("uninstall")
                .about("Get info about an app on the device")
                .arg(
                    Arg::new("bundle_id")
                        .required(true)
                        .help("The bundle ID to query"),
                ),
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
        match common::get_provider(udid, host, pairing_file, "app_service-jkcoxson").await {
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
    adapter
        .pcap("/Users/jacksoncoxson/Desktop/hmmmm.pcap")
        .await
        .unwrap();

    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut icp = InstallcoordinationProxy::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    if let Some(matches) = matches.subcommand_matches("info") {
        let bundle_id: &String = match matches.get_one("bundle_id") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };

        let res = icp.query_app_path(bundle_id).await.expect("no info");
        println!("Path: {res}");
    } else if let Some(matches) = matches.subcommand_matches("uninstall") {
        let bundle_id: &String = match matches.get_one("bundle_id") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };

        icp.uninstall_app(bundle_id)
            .await
            .expect("uninstall failed");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
