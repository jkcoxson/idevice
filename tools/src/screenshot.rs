use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};
use std::fs;

use idevice::screenshotr::ScreenshotService;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Take a screenshot")
        .with_argument(JkArgument::new().with_help("Output path").required(true))
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let output_path = arguments.clone().next_argument::<String>().unwrap();

    let res = if let Ok(proxy) = CoreDeviceProxy::connect(&*provider).await {
        println!("Using DVT over CoreDeviceProxy");
        let rsd_port = proxy.handshake.server_rsd_port;

        let adapter = proxy.create_software_tunnel().expect("no software tunnel");
        let mut adapter = adapter.to_async_handle();
        let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

        // Make the connection to RemoteXPC
        let mut handshake = RsdHandshake::new(stream).await.unwrap();
        let mut ts_client = idevice::dvt::remote_server::RemoteServerClient::connect_rsd(
            &mut adapter,
            &mut handshake,
        )
        .await
        .expect("Failed to connect");
        ts_client.read_message(0).await.expect("no read??");

        let mut ts_client = idevice::dvt::screenshot::ScreenshotClient::new(&mut ts_client)
            .await
            .expect("Unable to get channel for take screenshot");
        ts_client
            .take_screenshot()
            .await
            .expect("Failed to take screenshot")
    } else {
        println!("Using screenshotr");
        let mut screenshot_client = match ScreenshotService::connect(&*provider).await {
            Ok(client) => client,
            Err(e) => {
                eprintln!(
                    "Unable to connect to screenshotr service: {e} Ensure Developer Disk Image is mounted."
                );
                return;
            }
        };
        screenshot_client.take_screenshot().await.unwrap()
    };

    match fs::write(&output_path, res) {
        Ok(_) => println!("Screenshot saved to: {}", output_path),
        Err(e) => eprintln!("Failed to write screenshot to file: {}", e),
    }
}
