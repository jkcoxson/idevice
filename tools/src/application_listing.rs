// Jackson Coxson
//! Application listing tool - List installed applications on the device

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy,
    dvt::application_listing::ApplicationListingClient, dvt::remote_server::RemoteServerClient,
    provider::IdeviceProvider, rsd::RsdHandshake,
};

pub fn register() -> jkcli::JkCommand {
    jkcli::JkCommand::new().help("List installed applications on the device")
}

pub async fn main(_args: &jkcli::CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    eprintln!("Connecting to application listing service...");

    let mut remote_client = match connect(&*provider).await {
        Some(c) => c,
        None => return,
    };

    let mut client = match ApplicationListingClient::new(&mut remote_client).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to create ApplicationListingClient: {e}");
            return;
        }
    };

    match client.installed_applications().await {
        Ok(apps) => {
            println!("Found {} applications:", apps.len());
            for app in apps {
                let bundle_id = app
                    .get("CFBundleIdentifier")
                    .and_then(|v| v.as_string())
                    .unwrap_or("?");
                let name = app
                    .get("DisplayName")
                    .or_else(|| app.get("CFBundleDisplayName"))
                    .or_else(|| app.get("CFBundleName"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("?");
                let version = app
                    .get("Version")
                    .or_else(|| app.get("CFBundleShortVersionString"))
                    .and_then(|v| v.as_string())
                    .unwrap_or("?");
                println!("  {bundle_id:<55}  {name:<30}  {version}");
            }
        }
        Err(e) => eprintln!("Error: {e}"),
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
