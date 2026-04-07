// Jackson Coxson
//! Sysmontap tool - System monitoring tap for process and system stats

use idevice::{
    IdeviceService, RsdService,
    core_device_proxy::CoreDeviceProxy,
    dvt::device_info::DeviceInfoClient,
    dvt::remote_server::RemoteServerClient,
    dvt::sysmontap::{SysmontapClient, SysmontapConfig},
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Monitor system and process stats via sysmontap")
        .with_argument(
            JkArgument::new()
                .with_help("Sampling interval in milliseconds (default: 500)")
                .required(false),
        )
}

pub async fn main(args: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut args = args.clone();
    let interval_ms: u32 = args
        .next_argument::<String>()
        .and_then(|s| s.parse().ok())
        .unwrap_or(500);

    eprintln!("Connecting to DVT service...");

    let mut remote_client = match connect(&*provider).await {
        Some(c) => c,
        None => return,
    };

    // Get attribute lists from DeviceInfo first
    eprintln!("Fetching attribute lists...");
    let (proc_attrs, sys_attrs) = {
        let mut device_info = match DeviceInfoClient::new(&mut remote_client).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to create DeviceInfoClient: {e}");
                return;
            }
        };
        let proc_attrs = match device_info.sysmon_process_attributes().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Failed to get process attributes: {e}");
                return;
            }
        };
        let sys_attrs = match device_info.sysmon_system_attributes().await {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Failed to get system attributes: {e}");
                return;
            }
        };
        (proc_attrs, sys_attrs)
    };

    eprintln!(
        "Got {} process attrs and {} system attrs",
        proc_attrs.len(),
        sys_attrs.len()
    );

    let config = SysmontapConfig {
        interval_ms,
        process_attributes: proc_attrs.clone(),
        system_attributes: sys_attrs.clone(),
    };

    let mut sysmontap = match SysmontapClient::new(&mut remote_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create SysmontapClient: {e}");
            return;
        }
    };

    if let Err(e) = sysmontap.set_config(&config).await {
        eprintln!("Failed to set config: {e}");
        return;
    }

    if let Err(e) = sysmontap.start().await {
        eprintln!("Failed to start: {e}");
        return;
    }

    eprintln!("Sampling every {interval_ms}ms... Press Ctrl+C to stop.\n");

    // Print system attribute header
    if !sys_attrs.is_empty() {
        print!("System: ");
        println!("{}", sys_attrs.join(", "));
    }

    loop {
        match sysmontap.next_sample().await {
            Ok(sample) => {
                if let Some(sys) = sample.system {
                    print!("  sys=[");
                    let strs: Vec<String> = sys.iter().map(|v| format!("{v:?}")).collect();
                    print!("{}", strs.join(", "));
                    println!("]");
                }
                if let Some(cpu) = sample.system_cpu_usage
                    && let Some(idle) = cpu.get("CPU_TotalLoad")
                {
                    println!("  cpu_load={idle:?}");
                }
                if let Some(procs) = sample.processes {
                    println!("  {} processes:", procs.len());
                    for (pid, info) in procs.iter().take(5) {
                        println!("    pid={pid}: {info:?}");
                    }
                    if procs.len() > 5 {
                        println!("    ... and {} more", procs.len() - 5);
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading sample: {e}");
                break;
            }
        }
    }

    if let Err(e) = sysmontap.stop().await {
        eprintln!("Failed to stop: {e}");
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
            None
        }
    }
}
