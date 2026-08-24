// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{AppIconTarget, IconServiceClient},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Fetch an app icon as a PNG over the CoreDevice icon service")
        .with_argument(
            JkArgument::new()
                .with_help("Bundle ID of the app (or an on-device app path with --app-path)")
                .required(true),
        )
        .with_argument(
            JkArgument::new()
                .with_help("Path to write the PNG to")
                .required(true),
        )
        .with_argument(JkArgument::new().with_help("Size in points (default: 60)"))
        .with_argument(JkArgument::new().with_help("Scale factor (default: 2)"))
        .with_flag(
            JkFlag::new("app-path")
                .with_help("Treat the first argument as an on-device app bundle path"),
        )
        .with_flag(
            JkFlag::new("no-placeholder")
                .with_help("Fail instead of returning a generic placeholder icon"),
        )
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut arguments = arguments.clone();
    let identifier: String = arguments
        .next_argument()
        .expect("bundle ID (or app path) required");
    let out_path: String = arguments.next_argument().expect("output path required");
    let size: f32 = arguments.next_argument().unwrap_or(60.0);
    let scale: f32 = arguments.next_argument().unwrap_or(2.0);

    let target = if arguments.has_flag("app-path") {
        AppIconTarget::AppPath(identifier)
    } else {
        AppIconTarget::BundleIdentifier(identifier)
    };
    let allow_placeholder = !arguments.has_flag("no-placeholder");

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut client = IconServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no icon service");

    let icon = client
        .fetch_icon(target, size, size, scale, allow_placeholder)
        .await
        .expect("failed to fetch the icon");

    let png: Vec<u8> = icon.png_data.clone().into();
    println!(
        "{}x{} points at {}x ({}x{} pixels), {} bytes{}",
        icon.size.0,
        icon.size.1,
        icon.scale,
        icon.pixel_size.0,
        icon.pixel_size.1,
        png.len(),
        if icon.is_placeholder {
            " [placeholder]"
        } else {
            ""
        }
    );
    tokio::fs::write(&out_path, &png)
        .await
        .expect("failed to write the PNG");
    println!("wrote {out_path}");
}
