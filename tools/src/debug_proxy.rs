// Jackson Coxson

use std::io::Write;

use idevice::{
    IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, debug_proxy::DebugProxyClient,
    provider::IdeviceProvider, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("Start a debug proxy shell")
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
    println!("{:?}", handshake.services);

    let mut dp = DebugProxyClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    println!("Shell connected!");
    loop {
        print!("> ");
        std::io::stdout().flush().unwrap();

        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).unwrap();

        let buf = buf.trim();

        if buf == "exit" {
            break;
        }

        let res = dp.send_command(buf.into()).await.expect("Failed to send");
        if let Some(res) = res {
            println!("{res}");
        }
    }
}
