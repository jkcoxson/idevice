// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{OrientationServiceClient, RotationDirection},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Rotate the device 90 degrees over CoreDevice (left = CCW, right = CW)")
        .with_argument(JkArgument::new().with_help("direction: left or right (default: left)"))
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut arguments = arguments.clone();
    let direction = match arguments.next_argument::<String>().as_deref() {
        Some("left") | None => RotationDirection::Left,
        Some("right") => RotationDirection::Right,
        Some(other) => {
            eprintln!("direction must be 'left' or 'right', got {other:?}");
            return;
        }
    };

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut client = OrientationServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no devicecontrol service");
    let state = client.rotate(direction).await.expect("rotate failed");
    println!("{state:#?}");
}
