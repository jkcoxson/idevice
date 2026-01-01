// Jackson Coxson

use clap::{Arg, Command, arg};
use idevice::{
    IdeviceService, RsdService, companion_proxy::CompanionProxy,
    core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake,
};
use plist_macro::{pretty_print_dictionary, pretty_print_plist};

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("companion_proxy")
        .about("Apple Watch things")
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
        .subcommand(Command::new("list").about("List the companions on the device"))
        .subcommand(Command::new("listen").about("Listen for devices"))
        .subcommand(
            Command::new("get")
                .about("Gets a value")
                .arg(arg!(-d --device_udid <STRING> "the device udid to get from").required(true))
                .arg(arg!(-v --value <STRING> "the value to get").required(true)),
        )
        .subcommand(
            Command::new("start")
                .about("Starts a service")
                .arg(arg!(-p --port <PORT> "the port").required(true))
                .arg(arg!(-n --name <STRING> "the optional service name").required(false)),
        )
        .subcommand(
            Command::new("stop")
                .about("Starts a service")
                .arg(arg!(-p --port <PORT> "the port").required(true)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("companion_proxy");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "amfi-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core_device_proxy");
    let rsd_port = proxy.handshake.server_rsd_port;
    let mut provider = proxy
        .create_software_tunnel()
        .expect("no tunnel")
        .to_async_handle();
    let mut handshake = RsdHandshake::new(provider.connect(rsd_port).await.unwrap())
        .await
        .unwrap();
    let mut proxy = CompanionProxy::connect_rsd(&mut provider, &mut handshake)
        .await
        .expect("no companion proxy connect");

    // let mut proxy = CompanionProxy::connect(&*provider)
    //     .await
    //     .expect("Failed to connect to companion proxy");

    if matches.subcommand_matches("list").is_some() {
        proxy.get_device_registry().await.expect("Failed to show");
    } else if matches.subcommand_matches("listen").is_some() {
        let mut stream = proxy.listen_for_devices().await.expect("Failed to show");
        while let Ok(v) = stream.next().await {
            println!("{}", pretty_print_dictionary(&v));
        }
    } else if let Some(matches) = matches.subcommand_matches("get") {
        let key = matches.get_one::<String>("value").expect("no value passed");
        let udid = matches
            .get_one::<String>("device_udid")
            .expect("no AW udid passed");

        match proxy.get_value(udid, key).await {
            Ok(value) => {
                println!("{}", pretty_print_plist(&value));
            }
            Err(e) => {
                eprintln!("Error getting value: {e}");
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("start") {
        let port: u16 = matches
            .get_one::<String>("port")
            .expect("no port passed")
            .parse()
            .expect("not a number");
        let name = matches.get_one::<String>("name").map(|x| x.as_str());

        match proxy.start_forwarding_service_port(port, name, None).await {
            Ok(value) => {
                println!("started on port {value}");
            }
            Err(e) => {
                eprintln!("Error starting: {e}");
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("stop") {
        let port: u16 = matches
            .get_one::<String>("port")
            .expect("no port passed")
            .parse()
            .expect("not a number");

        if let Err(e) = proxy.stop_forwarding_service_port(port).await {
            eprintln!("Error starting: {e}");
        }
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
