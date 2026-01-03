// Jackson Coxson
// Print out all the RemoteXPC services

use idevice::{
    IdeviceService, core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider,
    rsd::RsdHandshake, tcp::stream::AdapterStream,
};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("Get services from RemoteXPC")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let mut adapter = proxy.create_software_tunnel().expect("no software tunnel");

    let stream = AdapterStream::connect(&mut adapter, rsd_port)
        .await
        .expect("no RSD connect");

    // Make the connection to RemoteXPC
    let handshake = RsdHandshake::new(stream).await.unwrap();
    println!("{:#?}", handshake.services);
}
