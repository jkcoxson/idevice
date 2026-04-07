// Jackson Coxson
//! Network monitor tool - Monitor network connections on the device

use idevice::{
    IdeviceService, RsdService,
    core_device_proxy::CoreDeviceProxy,
    dvt::network_monitor::{NetworkEvent, NetworkMonitorClient},
    dvt::remote_server::RemoteServerClient,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};

pub fn register() -> jkcli::JkCommand {
    jkcli::JkCommand::new().help("Monitor network connections on the device")
}

pub async fn main(_args: &jkcli::CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    eprintln!("Connecting to network monitor service...");

    let mut remote_client = match connect(&*provider).await {
        Some(c) => c,
        None => return,
    };

    let mut client = match NetworkMonitorClient::new(&mut remote_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create NetworkMonitorClient: {e}");
            return;
        }
    };

    if let Err(e) = client.start_monitoring().await {
        eprintln!("Failed to start monitoring: {e}");
        return;
    }

    eprintln!("Monitoring network activity... Press Ctrl+C to stop.\n");

    loop {
        match client.next_event().await {
            Ok(event) => match event {
                NetworkEvent::InterfaceDetection(e) => {
                    println!("[INTERFACE] index={} name={}", e.interface_index, e.name);
                }
                NetworkEvent::ConnectionDetection(e) => {
                    let local = e
                        .local_address
                        .map(|a| format!("{}:{}", a.addr, a.port))
                        .unwrap_or_else(|| "?".into());
                    let remote = e
                        .remote_address
                        .map(|a| format!("{}:{}", a.addr, a.port))
                        .unwrap_or_else(|| "?".into());
                    println!(
                        "[CONNECT]   pid={:5} {} -> {} if={} sn={}",
                        e.pid, local, remote, e.interface_index, e.serial_number
                    );
                }
                NetworkEvent::ConnectionUpdate(e) => {
                    println!(
                        "[UPDATE]    sn={} rx_bytes={} tx_bytes={} rx_pkts={} tx_pkts={}",
                        e.connection_serial, e.rx_bytes, e.tx_bytes, e.rx_packets, e.tx_packets
                    );
                }
                NetworkEvent::Unknown(t) => {
                    eprintln!("[UNKNOWN]   event type={t}");
                }
            },
            Err(e) => {
                eprintln!("Error reading event: {e}");
                break;
            }
        }
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
