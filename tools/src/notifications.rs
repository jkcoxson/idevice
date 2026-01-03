//  Monitor memory and app notifications

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("Notification proxy")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");

    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();
    let mut ts_client =
        idevice::dvt::remote_server::RemoteServerClient::connect_rsd(&mut adapter, &mut handshake)
            .await
            .expect("Failed to connect");
    ts_client.read_message(0).await.expect("no read??");
    let mut notification_client =
        idevice::dvt::notifications::NotificationsClient::new(&mut ts_client)
            .await
            .expect("Unable to get channel for notifications");
    notification_client
        .start_notifications()
        .await
        .expect("Failed to start notifications");

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("\nShutdown signal received, exiting.");
                break;
            }

            result = notification_client.get_notification() => {
                if let Err(e) = result {
                    eprintln!("Failed to get notifications: {}", e);
                } else {
                    println!("Received notifications: {:#?}", result.unwrap());
                }
            }
        }
    }
}
