// Jackson Coxson
//! Condition inducer tool - Simulate network/thermal conditions

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    dvt::condition_inducer::ConditionInducerClient, dvt::remote_server::RemoteServerClient,
    provider::IdeviceProvider, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Simulate network or thermal conditions on the device")
        .with_subcommand("list", JkCommand::new().help("List available conditions"))
        .with_subcommand(
            "enable",
            JkCommand::new()
                .help("Enable a condition profile")
                .with_argument(
                    JkArgument::new()
                        .with_help("Profile identifier to enable")
                        .required(true),
                ),
        )
        .with_subcommand(
            "disable",
            JkCommand::new().help("Disable the active condition"),
        )
        .subcommand_required(true)
}

pub async fn main(args: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut remote_client = match connect(&*provider).await {
        Some(c) => c,
        None => return,
    };

    let mut client = match ConditionInducerClient::new(&mut remote_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create ConditionInducerClient: {e}");
            return;
        }
    };

    let (subcommand, sub_args) = match args.first_subcommand() {
        Some(s) => s,
        None => {
            eprintln!("No subcommand provided");
            return;
        }
    };

    match subcommand.as_str() {
        "list" => match client.available_conditions().await {
            Ok(groups) => {
                for group in groups {
                    println!("{}:", group.identifier);
                    for profile in group.profiles {
                        println!("  {} - {}", profile.identifier, profile.description);
                    }
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        "enable" => {
            let mut sub_args = sub_args.clone();
            let profile_id = sub_args.next_argument::<String>().unwrap_or_default();

            // Find the group identifier for this profile
            let groups = match client.available_conditions().await {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("Failed to list conditions: {e}");
                    return;
                }
            };

            let mut found = false;
            for group in &groups {
                for profile in &group.profiles {
                    if profile.identifier == profile_id {
                        eprintln!("Enabling: {} ({})", profile.description, profile.identifier);
                        if let Err(e) = client
                            .enable_condition(&group.identifier, &profile.identifier)
                            .await
                        {
                            eprintln!("Error: {e}");
                        } else {
                            println!("Condition enabled.");
                        }
                        found = true;
                        break;
                    }
                }
                if found {
                    break;
                }
            }

            if !found {
                eprintln!(
                    "Profile '{profile_id}' not found. Use 'list' to see available profiles."
                );
            }
        }
        "disable" => match client.disable_condition().await {
            Ok(()) => println!("Condition disabled."),
            Err(e) => eprintln!("Error: {e}"),
        },
        _ => unreachable!(),
    }
}

async fn connect(
    provider: &dyn IdeviceProvider,
) -> Option<RemoteServerClient<Box<dyn idevice::ReadWrite>>> {
    match CoreDeviceProxy::connect(provider).await {
        Ok(proxy) => {
            let rsd_port = proxy.tunnel_info().server_rsd_port;
            let adapter = proxy.create_software_tunnel().expect("no software tunnel");
            let mut adapter = adapter.to_async_handle();
            let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
            let mut handshake = RsdHandshake::new(stream).await.unwrap();
            match RemoteServerClient::connect_rsd(&mut adapter, &mut handshake).await {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("Failed to connect via RSD: {e}");
                    None
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to connect to CoreDeviceProxy: {e}");
            eprintln!("Falling back to Lockdown-based connection...");
            match RemoteServerClient::connect(provider).await {
                Ok(c) => Some(c),
                Err(e2) => {
                    eprintln!("Failed to connect via Lockdown: {e2}");
                    None
                }
            }
        }
    }
}
