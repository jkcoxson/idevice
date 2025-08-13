// Jackson Coxson & Aleksei Borodin
// Rewritten based on SideStore's libimobiledevice implementation

use std::path::PathBuf;
use std::fs;

use clap::{arg, value_parser, Arg, Command};
use idevice::{
    misagent::MisagentClient, 
    IdeviceService
};
use log::{debug, warn};
use plist::Value;

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
                )
                .arg(
                    arg!(-a --all "list all profiles including system profiles")
                        .action(clap::ArgAction::SetTrue),
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
                .arg(Arg::new("id").required(true).index(1).help("Profile UUID to remove")),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("misagent - manage provisioning profiles on iOS devices");
        println!("Based on SideStore's libimobiledevice implementation");
        println!("Copyright (c) 2025 Jackson Coxson & Aleksei Borodin");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "misagent-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to get provider: {e}");
            return;
        }
    };

    // Connect to misagent service using traditional lockdown approach
    debug!("Connecting to misagent service...");
    let mut client = match MisagentClient::connect(&*provider).await {
        Ok(c) => {
            debug!("Successfully connected to misagent service");
            c
        }
        Err(e) => {
            eprintln!("Failed to connect to misagent service: {:?}", e);
            return;
        }
    };

    // Handle subcommands
    match matches.subcommand() {
        Some(("list", list_matches)) => {
            handle_list_command(&mut client, list_matches).await;
        }
        Some(("add", add_matches)) => {
            handle_add_command(&mut client, add_matches).await;
        }
        Some(("remove", remove_matches)) => {
            handle_remove_command(&mut client, remove_matches).await;
        }
        _ => {
            eprintln!("No subcommand specified. Use --help for available commands.");
        }
    }
}

async fn handle_list_command(client: &mut MisagentClient, matches: &clap::ArgMatches) {
    let list_all = matches.get_flag("all");
    let save_folder = matches.get_one::<PathBuf>("save");
    
    debug!("Listing provisioning profiles (all: {})", list_all);
    
    let profiles = if list_all {
        match client.list_all_profiles().await {
            Ok(profiles) => profiles,
            Err(e) => {
                eprintln!("Failed to list all profiles: {:?}", e);
                eprintln!("Error code: {}", client.get_last_error());
                return;
            }
        }
    } else {
        match client.list_profiles().await {
            Ok(profiles) => profiles,
            Err(e) => {
                eprintln!("Failed to list profiles: {:?}", e);
                eprintln!("Error code: {}", client.get_last_error());
                return;
            }
        }
    };

    if profiles.is_empty() {
        println!("No provisioning profiles found.");
        return;
    }

    println!("Found {} provisioning profile(s):", profiles.len());
    
    for (i, profile) in profiles.iter().enumerate() {
        if let Value::Data(profile_data) = profile {
            // Parse the provisioning profile to extract useful information
            match parse_provisioning_profile(profile_data) {
                Ok(info) => {
                    println!("Profile #{}: {}", i + 1, info.name);
                    println!("  UUID: {}", info.uuid);
                    println!("  App ID: {}", info.app_id);
                    println!("  Team ID: {}", info.team_id);
                    println!("  Expiration: {}", info.expiration_date);
                    println!("  Device Count: {}", info.device_count);
                    
                    // Save profile if requested
                    if let Some(folder) = save_folder {
                        let filename = format!("{}.mobileprovision", info.name.replace(" ", "_"));
                        let filepath = folder.join(filename);
                        
                        if let Err(e) = fs::create_dir_all(folder) {
                            warn!("Failed to create directory {}: {}", folder.display(), e);
                        } else if let Err(e) = fs::write(&filepath, profile_data) {
                            warn!("Failed to save profile to {}: {}", filepath.display(), e);
                        } else {
                            println!("  Saved to: {}", filepath.display());
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to parse profile #{}: {}", i + 1, e);
                    println!("Profile #{}: <parsing failed>", i + 1);
                }
            }
            println!();
        } else {
            warn!("Profile #{} is not binary data", i + 1);
        }
    }
}

async fn handle_add_command(client: &mut MisagentClient, matches: &clap::ArgMatches) {
    let profile_path = matches.get_one::<PathBuf>("provisioning_profile").unwrap();
    
    debug!("Installing provisioning profile: {}", profile_path.display());
    
    // Read the provisioning profile file
    let profile_data = match fs::read(profile_path) {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Failed to read provisioning profile file {}: {}", profile_path.display(), e);
            return;
        }
    };
    
    // Validate it's a valid provisioning profile
    match parse_provisioning_profile(&profile_data) {
        Ok(info) => {
            println!("Installing provisioning profile: {}", info.name);
            println!("  UUID: {}", info.uuid);
            println!("  App ID: {}", info.app_id);
            println!("  Team ID: {}", info.team_id);
        }
        Err(e) => {
            eprintln!("Invalid provisioning profile: {}", e);
            return;
        }
    }
    
    // Install the profile
    match client.install_profile(&profile_data).await {
        Ok(()) => {
            println!("✅ Provisioning profile installed successfully!");
        }
        Err(e) => {
            eprintln!("❌ Failed to install provisioning profile: {:?}", e);
            eprintln!("Error code: {}", client.get_last_error());
        }
    }
}

async fn handle_remove_command(client: &mut MisagentClient, matches: &clap::ArgMatches) {
    let profile_id = matches.get_one::<String>("id").unwrap();
    
    debug!("Removing provisioning profile: {}", profile_id);
    
    match client.remove_profile(profile_id).await {
        Ok(()) => {
            println!("✅ Provisioning profile removed successfully!");
        }
        Err(e) => {
            eprintln!("❌ Failed to remove provisioning profile: {:?}", e);
            eprintln!("Error code: {}", client.get_last_error());
        }
    }
}

/// Information extracted from a provisioning profile
#[derive(Debug)]
struct ProvisioningProfileInfo {
    name: String,
    uuid: String,
    app_id: String,
    team_id: String,
    expiration_date: String,
    device_count: usize,
}

/// Parse a provisioning profile and extract key information
fn parse_provisioning_profile(data: &[u8]) -> Result<ProvisioningProfileInfo, Box<dyn std::error::Error>> {
    // Provisioning profiles are signed plist files, but we can extract the plist part
    // Look for the start of the plist data
    let data_str = String::from_utf8_lossy(data);
    
    // Find the start and end of the plist
    let start_marker = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
    let end_marker = "</plist>";
    
    let start_pos = data_str.find(start_marker)
        .ok_or("Could not find plist start marker")?;
    let end_pos = data_str.find(end_marker)
        .ok_or("Could not find plist end marker")?;
    
    let plist_data = &data_str[start_pos..end_pos + end_marker.len()];
    
    // Parse the plist
    let plist: Value = plist::from_bytes(plist_data.as_bytes())?;
    
    if let Value::Dictionary(dict) = plist {
        let name = dict.get("Name")
            .and_then(|v| v.as_string())
            .unwrap_or("Unknown")
            .to_string();
            
        let uuid = dict.get("UUID")
            .and_then(|v| v.as_string())
            .unwrap_or("Unknown")
            .to_string();
            
        let app_id = dict.get("Entitlements")
            .and_then(|v| v.as_dictionary())
            .and_then(|d| d.get("application-identifier"))
            .and_then(|v| v.as_string())
            .unwrap_or("Unknown")
            .to_string();
            
        let team_id = dict.get("TeamIdentifier")
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_string())
            .unwrap_or("Unknown")
            .to_string();
            
        let expiration_date = dict.get("ExpirationDate")
            .and_then(|v| v.as_date())
            .map(|d| format!("{:?}", d))
            .unwrap_or("Unknown".to_string());
            
        let device_count = dict.get("ProvisionedDevices")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        
        Ok(ProvisioningProfileInfo {
            name,
            uuid,
            app_id,
            team_id,
            expiration_date,
            device_count,
        })
    } else {
        Err("Provisioning profile does not contain a dictionary".into())
    }
}