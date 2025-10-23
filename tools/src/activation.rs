// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    IdeviceService, lockdown::LockdownClient, mobileactivationd::MobileActivationdClient,
};

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("activation")
        .about("mobileactivationd")
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
        .subcommand(Command::new("state").about("Gets the activation state"))
        .subcommand(Command::new("deactivate").about("Deactivates the device"))
        .get_matches();

    if matches.get_flag("about") {
        println!("activation - activate the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "activation-jkcoxson").await
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let activation_client = MobileActivationdClient::new(&*provider);
    let mut lc = LockdownClient::connect(&*provider)
        .await
        .expect("no lockdown");
    lc.start_session(&provider.get_pairing_file().await.unwrap())
        .await
        .expect("no TLS");
    let udid = lc
        .get_value(Some("UniqueDeviceID"), None)
        .await
        .expect("no udid")
        .into_string()
        .unwrap();

    if matches.subcommand_matches("state").is_some() {
        let s = activation_client.state().await.expect("no state");
        println!("Activation State: {s}");
    } else if matches.subcommand_matches("deactivate").is_some() {
        println!("CAUTION: You are deactivating {udid}, press enter to continue.");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        activation_client.deactivate().await.expect("no deactivate");
    // } else if matches.subcommand_matches("accept").is_some() {
    //     amfi_client
    //         .accept_developer_mode()
    //         .await
    //         .expect("Failed to show");
    // } else if matches.subcommand_matches("status").is_some() {
    //     let status = amfi_client
    //         .get_developer_mode_status()
    //         .await
    //         .expect("Failed to get status");
    //     println!("Enabled: {status}");
    // } else if let Some(matches) = matches.subcommand_matches("state") {
    //     let uuid: &String = match matches.get_one("uuid") {
    //         Some(u) => u,
    //         None => {
    //             eprintln!("No UUID passed. Invalid usage, pass -h for help");
    //             return;
    //         }
    //     };
    //     let status = amfi_client
    //         .trust_app_signer(uuid)
    //         .await
    //         .expect("Failed to get state");
    //     println!("Enabled: {status}");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
    return;
}
