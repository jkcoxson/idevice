// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    restore_service::RestoreServiceClient, rsd::RsdHandshake,
};
use plist_macro::pretty_print_dictionary;

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("restore_service")
        .about("Interact with the Restore Service service")
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
                .help("UDID of the device (overrides host/pairing file)"),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(Command::new("delay").about("Delay recovery image"))
        .subcommand(Command::new("recovery").about("Enter recovery mode"))
        .subcommand(Command::new("reboot").about("Reboots the device"))
        .subcommand(Command::new("preflightinfo").about("Gets the preflight info"))
        .subcommand(Command::new("nonces").about("Gets the nonces"))
        .subcommand(Command::new("app_parameters").about("Gets the app parameters"))
        .subcommand(
            Command::new("restore_lang")
                .about("Restores the language")
                .arg(Arg::new("language").required(true).index(1)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!(
            "mounter - query and manage images mounted on a device. Reimplementation of libimobiledevice's binary."
        );
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "restore_service-jkcoxson").await {
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
    println!("{:?}", handshake.services);

    let mut restore_client = RestoreServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("Unable to connect to service");

    if matches.subcommand_matches("recovery").is_some() {
        restore_client
            .enter_recovery()
            .await
            .expect("command failed");
    } else if matches.subcommand_matches("reboot").is_some() {
        restore_client.reboot().await.expect("command failed");
    } else if matches.subcommand_matches("preflightinfo").is_some() {
        let info = restore_client
            .get_preflightinfo()
            .await
            .expect("command failed");
        pretty_print_dictionary(&info);
    } else if matches.subcommand_matches("nonces").is_some() {
        let nonces = restore_client.get_nonces().await.expect("command failed");
        pretty_print_dictionary(&nonces);
    } else if matches.subcommand_matches("app_parameters").is_some() {
        let params = restore_client
            .get_app_parameters()
            .await
            .expect("command failed");
        pretty_print_dictionary(&params);
    } else if let Some(matches) = matches.subcommand_matches("restore_lang") {
        let lang = matches
            .get_one::<String>("language")
            .expect("No language passed");
        restore_client
            .restore_lang(lang)
            .await
            .expect("failed to restore lang");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
