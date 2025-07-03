// Jackson Coxson

use clap::{arg, Arg, Command};
use idevice::{lockdown::LockdownClient, pretty_print_plist, IdeviceService};
use plist::Value;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("lockdown")
        .about("Start a tunnel")
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
        .subcommand(
            Command::new("get")
                .about("Gets a value")
                .arg(arg!(-v --value <STRING> "the value to get").required(true))
                .arg(arg!(-d --domain <STRING> "the domain to get in").required(false)),
        )
        .subcommand(
            Command::new("get_all")
                .about("Gets all")
                .arg(arg!(-d --domain <STRING> "the domain to get in").required(false)),
        )
        .subcommand(
            Command::new("set")
                .about("Sets a lockdown value")
                .arg(arg!(-k --key <STRING> "the key to set").required(true))
                .arg(arg!(-v --value <STRING> "the value to set the key to").required(true))
                .arg(arg!(-d --domain <STRING> "the domain to get in").required(false)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("lockdown - query and manage values on a device. Reimplementation of libimobiledevice's binary.");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "ideviceinfo-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let mut lockdown_client = LockdownClient::connect(&*provider)
        .await
        .expect("Unable to connect to lockdown");

    lockdown_client
        .start_session(&provider.get_pairing_file().await.expect("no pairing file"))
        .await
        .expect("no session");

    match matches.subcommand() {
        Some(("get", sub_m)) => {
            let key = sub_m.get_one::<String>("value").unwrap();
            let domain = sub_m.get_one::<String>("domain").cloned();

            match lockdown_client.get_value(key, domain).await {
                Ok(value) => {
                    println!("{}", pretty_print_plist(&value));
                }
                Err(e) => {
                    eprintln!("Error getting value: {e}");
                }
            }
        }
        Some(("get_all", sub_m)) => {
            let domain = sub_m.get_one::<String>("domain").cloned();

            match lockdown_client.get_all_values(domain).await {
                Ok(value) => {
                    println!("{}", pretty_print_plist(&plist::Value::Dictionary(value)));
                }
                Err(e) => {
                    eprintln!("Error getting value: {e}");
                }
            }
        }

        Some(("set", sub_m)) => {
            let key = sub_m.get_one::<String>("key").unwrap();
            let value_str = sub_m.get_one::<String>("value").unwrap();
            let domain = sub_m.get_one::<String>("domain").cloned();

            let value = Value::String(value_str.clone());

            match lockdown_client.set_value(key, value, domain).await {
                Ok(()) => println!("Successfully set"),
                Err(e) => eprintln!("Error setting value: {e}"),
            }
        }

        _ => {
            eprintln!("No subcommand provided. Try `--help` for usage.");
        }
    }
}
