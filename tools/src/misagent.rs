// Jackson Coxson & Aleksei Borodin

use std::path::PathBuf;

use clap::{arg, value_parser, Arg, Command};
use idevice::{
    misagent::{MisagentClient, MisagentRsdClient}, 
    core_device_proxy::CoreDeviceProxy,
    rsd::RsdHandshake,
    tcp::stream::AdapterStream,
    IdeviceService, RsdService
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("misagent")
        .about("Manage provisioning profiles on iOS devices")
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
        .subcommand(
            Command::new("list")
                .about("Lists the provisioning profiles on the device")
                .arg(
                    arg!(-s --save <FOLDER> "the folder to save the profiles to")
                        .value_parser(value_parser!(PathBuf)),
                ),
        )
        .subcommand(
            Command::new("add")
                .about("Install a provisioning profile on the device")
                .arg(
                    Arg::new("provisioning_profile")
                        .long("provisioning-profile")
                        .value_name("FILE")
                        .help("Path to the .mobileprovision file to install")
                        .required(true)
                        .value_parser(value_parser!(PathBuf)),
                ),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a provisioning profile")
                .arg(Arg::new("id").required(true).index(1)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("misagent - manage provisioning profiles on iOS devices. Reimplementation of libimobiledevice's binary.");
        println!("Copyright (c) 2025 Jackson Coxson & Aleksei Borodin");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "misagent-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    // If host is specified, use RSD-based connection
    if host.is_some() {
        println!("Using RSD connection for network device");
        
        // Establish core device proxy for RSD access
        let proxy = match CoreDeviceProxy::connect(&*provider).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Failed to connect to core device proxy: {}", e);
                return;
            }
        };
        
        let rsd_port = proxy.handshake.server_rsd_port;
        let mut adapter = match proxy.create_software_tunnel() {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Failed to create software tunnel: {}", e);
                return;
            }
        };
        
        let stream = match AdapterStream::connect(&mut adapter, rsd_port).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to connect to RSD port: {}", e);
                return;
            }
        };
        
        // Make the connection to RemoteXPC
        let mut handshake = match RsdHandshake::new(stream).await {
            Ok(h) => h,
            Err(e) => {
                eprintln!("Failed to create RSD handshake: {}", e);
                return;
            }
        };
        
        let mut misagent_client = match MisagentRsdClient::connect_rsd(&mut adapter, &mut handshake).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to misagent service: {}", e);
                return;
            }
        };

    if let Some(matches) = matches.subcommand_matches("list") {
            let profiles = match misagent_client.copy_all().await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to get provisioning profiles: {}", e);
                    return;
                }
            };
            
            println!("Found {} provisioning profiles", profiles.len());
            
        if let Some(path) = matches.get_one::<PathBuf>("save") {
                if let Err(e) = tokio::fs::create_dir_all(path).await {
                    eprintln!("Unable to create save directory: {}", e);
                    return;
                }

                for (index, profile) in profiles.iter().enumerate() {
                    let f = path.join(format!("{index}.mobileprovision"));
                    if let Err(e) = tokio::fs::write(f, profile).await {
                        eprintln!("Failed to write profile {}: {}", index, e);
                        return;
                    }
                }
                println!("Saved profiles to {}", path.display());
            }
        } else if let Some(matches) = matches.subcommand_matches("add") {
            let profile_path = matches.get_one::<PathBuf>("provisioning_profile").expect("Profile path is required");
            
            let profile_data = match tokio::fs::read(profile_path).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to read provisioning profile file: {}", e);
                    return;
                }
            };
            
            match misagent_client.install(profile_data).await {
                Ok(()) => println!("Successfully installed provisioning profile from {}", profile_path.display()),
                Err(e) => {
                    eprintln!("Failed to install provisioning profile: {}", e);
                    return;
            }
        }
    } else if let Some(matches) = matches.subcommand_matches("remove") {
        let id = matches.get_one::<String>("id").expect("No ID passed");
            if let Err(e) = misagent_client.remove(id).await {
                eprintln!("Failed to remove profile: {}", e);
                return;
            }
            println!("Successfully removed profile {}", id);
        } else {
            eprintln!("Invalid usage, pass -h for help");
        }
    } else {
        // Use traditional lockdown-based connection
        println!("Using lockdown connection");
        
        let mut misagent_client = match MisagentClient::connect(&*provider).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Unable to connect to misagent: {}", e);
                return;
            }
        };

        if let Some(matches) = matches.subcommand_matches("list") {
            let profiles = match misagent_client.copy_all().await {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("Failed to get provisioning profiles: {}", e);
                    return;
                }
            };
            
            println!("Found {} provisioning profiles", profiles.len());
            
            if let Some(path) = matches.get_one::<PathBuf>("save") {
                if let Err(e) = tokio::fs::create_dir_all(path).await {
                    eprintln!("Unable to create save directory: {}", e);
                    return;
                }

                for (index, profile) in profiles.iter().enumerate() {
                    let f = path.join(format!("{index}.mobileprovision"));
                    if let Err(e) = tokio::fs::write(f, profile).await {
                        eprintln!("Failed to write profile {}: {}", index, e);
                        return;
                    }
                }
                println!("Saved profiles to {}", path.display());
            }
        } else if let Some(matches) = matches.subcommand_matches("add") {
            let profile_path = matches.get_one::<PathBuf>("provisioning_profile").expect("Profile path is required");
            
            let profile_data = match tokio::fs::read(profile_path).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!("Failed to read provisioning profile file: {}", e);
                    return;
                }
            };
            
            match misagent_client.install(profile_data).await {
                Ok(()) => println!("Successfully installed provisioning profile from {}", profile_path.display()),
                Err(e) => {
                    eprintln!("Failed to install provisioning profile: {}", e);
                    return;
                }
            }
        } else if let Some(matches) = matches.subcommand_matches("remove") {
            let id = matches.get_one::<String>("id").expect("No ID passed");
            if let Err(e) = misagent_client.remove(id).await {
                eprintln!("Failed to remove profile: {}", e);
                return;
            }
            println!("Successfully removed profile {}", id);
    } else {
        eprintln!("Invalid usage, pass -h for help");
        }
    }
}
