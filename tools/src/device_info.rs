// Jackson Coxson
//! Device info tool - Query running processes and device information

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    dvt::device_info::DeviceInfoClient, dvt::remote_server::RemoteServerClient,
    provider::IdeviceProvider, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Query device information via DVT")
        .with_subcommand("processes", JkCommand::new().help("List running processes"))
        .with_subcommand(
            "hardware",
            JkCommand::new().help("Show hardware information"),
        )
        .with_subcommand("network", JkCommand::new().help("Show network information"))
        .with_subcommand("kernel", JkCommand::new().help("Show mach kernel name"))
        .with_subcommand(
            "ls",
            JkCommand::new()
                .help("List directory contents")
                .with_argument(JkArgument::new().with_help("Path to list").required(true)),
        )
        .with_subcommand(
            "execname",
            JkCommand::new()
                .help("Get executable path for PID")
                .with_argument(JkArgument::new().with_help("PID to query").required(true)),
        )
        .subcommand_required(true)
}

pub async fn main(args: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut remote_client = match connect(&*provider).await {
        Some(c) => c,
        None => return,
    };

    let (subcommand, sub_args) = match args.first_subcommand() {
        Some(s) => s,
        None => {
            eprintln!("No subcommand provided");
            return;
        }
    };

    let mut client = match DeviceInfoClient::new(&mut remote_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create DeviceInfoClient: {e}");
            return;
        }
    };

    match subcommand.as_str() {
        "processes" => match client.running_processes().await {
            Ok(procs) => {
                println!("{:>6}  {:<40}  app", "PID", "name");
                for p in procs {
                    println!("{:>6}  {:<40}  {}", p.pid, p.name, p.is_application);
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        "hardware" => match client.hardware_information().await {
            Ok(dict) => {
                for (k, v) in &dict {
                    println!("{k}: {v:?}");
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        "network" => match client.network_information().await {
            Ok(dict) => {
                for (k, v) in &dict {
                    println!("{k}: {v:?}");
                }
            }
            Err(e) => eprintln!("Error: {e}"),
        },
        "kernel" => match client.mach_kernel_name().await {
            Ok(name) => println!("{name}"),
            Err(e) => eprintln!("Error: {e}"),
        },
        "ls" => {
            let mut sub_args = sub_args.clone();
            let path = sub_args
                .next_argument::<String>()
                .unwrap_or_else(|| "/".into());
            match client.directory_listing(&path).await {
                Ok(entries) => {
                    for e in entries {
                        println!("{e}");
                    }
                }
                Err(e) => eprintln!("Error: {e}"),
            }
        }
        "execname" => {
            let mut sub_args = sub_args.clone();
            let pid: u32 = sub_args
                .next_argument::<String>()
                .unwrap_or_default()
                .parse()
                .unwrap_or(0);
            match client.execname_for_pid(pid).await {
                Ok(name) => println!("{name}"),
                Err(e) => eprintln!("Error: {e}"),
            }
        }
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
