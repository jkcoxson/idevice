// Jackson Coxson
//! Graphics monitoring - Monitor GPU/graphics performance

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, dvt::graphics::GraphicsClient,
    dvt::remote_server::RemoteServerClient, provider::IdeviceProvider, rsd::RsdHandshake,
};

pub fn register() -> jkcli::JkCommand {
    jkcli::JkCommand::new().help("Monitor GPU/graphics performance")
}

pub async fn main(_args: &jkcli::CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    eprintln!("Connecting to graphics monitoring service...");

    let proxy = match CoreDeviceProxy::connect(&*provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to connect to CoreDeviceProxy: {e}");
            eprintln!("Falling back to Lockdown-based connection...");
            match RemoteServerClient::connect(&*provider).await {
                Ok(mut remote_client) => {
                    run_graphics_monitor(&mut remote_client).await;
                    return;
                }
                Err(e2) => {
                    eprintln!("Failed to connect via Lockdown: {e2}");
                    return;
                }
            }
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

    run_graphics_monitor(&mut remote_client).await;
}

async fn run_graphics_monitor(remote_client: &mut RemoteServerClient<Box<dyn idevice::ReadWrite>>) {
    let mut graphics = match GraphicsClient::new(remote_client).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to create graphics client: {e}");
            return;
        }
    };

    eprintln!("Starting graphics monitoring...");
    eprintln!("Press Ctrl+C to stop...\n");

    if let Err(e) = graphics.start_sampling(0.0).await {
        eprintln!("Failed to start sampling: {e}");
        return;
    }

    println!(
        "{:<14} {:>7} {:>14} {:>14}  gpu",
        "timestamp(µs)", "fps", "alloc_mem", "used_mem"
    );

    loop {
        match graphics.sample().await {
            Ok(sample) => println!("{sample}"),
            Err(e) => {
                eprintln!("Failed to read graphics sample: {e}");
                break;
            }
        }
    }

    if let Err(e) = graphics.stop_sampling().await {
        eprintln!("Failed to stop sampling: {e}");
    }
}
