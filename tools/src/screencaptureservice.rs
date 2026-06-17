// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{ImageFormat, ScreenCaptureServiceClient},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Take a screenshot over com.apple.coredevice.screencaptureservice")
        .with_argument(
            JkArgument::new()
                .with_help("Output PNG path (default coredevice_screenshot.png)")
                .required(false),
        )
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let out_path = arguments
        .clone()
        .next_argument::<String>()
        .unwrap_or_else(|| "coredevice_screenshot.png".to_string());

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut client = ScreenCaptureServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no screencaptureservice");

    let image = client
        .take_screenshot(None, ImageFormat::Png)
        .await
        .expect("screenshot failed");

    match std::fs::write(&out_path, &image) {
        Ok(_) => println!("Screenshot ({} bytes) saved to {out_path}", image.len()),
        Err(e) => eprintln!("failed to write screenshot to {out_path}: {e}"),
    }
}
