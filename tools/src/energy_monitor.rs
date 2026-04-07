// Jackson Coxson
// Energy monitor - Monitor energy consumption for iOS processes

use idevice::{
    IdeviceService, RsdService,
    core_device_proxy::CoreDeviceProxy,
    dvt::energy_monitor::{EnergyMonitorClient, EnergySample},
    dvt::remote_server::RemoteServerClient,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Monitor energy consumption for iOS processes")
        .with_argument(
            JkArgument::new()
                .with_help("PIDs to monitor (comma-separated, required)")
                .required(true),
        )
}

pub async fn main(args: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    eprintln!("Connecting to energy monitor service...");

    let mut args = args.clone();
    let pids: Vec<u32> = args
        .next_argument::<String>()
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| s.trim().parse::<u32>().ok())
        .collect();

    if pids.is_empty() {
        eprintln!("Error: at least one PID is required");
        return;
    }

    let proxy = match CoreDeviceProxy::connect(&*provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to connect to CoreDeviceProxy: {e}");
            return;
        }
    };

    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut remote_client =
        match RemoteServerClient::connect_rsd(&mut adapter, &mut handshake).await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to DVT service via RSD: {e}");
                return;
            }
        };

    let mut energy_monitor = match EnergyMonitorClient::new(&mut remote_client).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to create energy monitor client: {e}");
            return;
        }
    };

    eprintln!("Monitoring energy for PIDs: {pids:?}");
    eprintln!("Press Ctrl+C to stop...\n");

    if let Err(e) = energy_monitor.stop_sampling(&pids).await {
        eprintln!("Warning: stop_sampling failed: {e}");
    }
    if let Err(e) = energy_monitor.start_sampling(&pids).await {
        eprintln!("Failed to start sampling: {e}");
        return;
    }

    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        match energy_monitor.sample_attributes(&pids).await {
            Ok(raw_bytes) => match EnergySample::from_bytes(&raw_bytes) {
                Ok(samples) if samples.is_empty() => {
                    eprintln!("(no data yet)");
                }
                Ok(samples) => {
                    for s in &samples {
                        println!(
                            "PID {:5} | t={:4}s | total={:8.3} cpu={:8.3} gpu={:8.3} net={:8.3} display={:8.3}",
                            s.pid,
                            s.timestamp,
                            s.total_energy,
                            s.cpu_energy,
                            s.gpu_energy,
                            s.networking_energy,
                            s.display_energy
                        );
                    }
                }
                Err(e) => eprintln!("Failed to parse energy sample: {e}"),
            },
            Err(e) => {
                eprintln!("Failed to sample energy: {e}");
                break;
            }
        }
    }

    if let Err(e) = energy_monitor.stop_sampling(&pids).await {
        eprintln!("Failed to stop sampling: {e}");
    }
}
