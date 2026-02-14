// Jackson Coxson

use futures_util::StreamExt;
use idevice::{
    IdeviceService, RsdService, core_device::DiagnostisServiceClient,
    core_device_proxy::CoreDeviceProxy, provider::IdeviceProvider, rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkCommand};
use tokio::io::AsyncWriteExt;

pub fn register() -> JkCommand {
    JkCommand::new().help("Retrieve a sysdiagnose")
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

    let mut dsc = DiagnostisServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    println!("Getting sysdiagnose, this takes a while! iOS is slow...");
    let mut res = dsc
        .capture_sysdiagnose(false)
        .await
        .expect("no sysdiagnose");
    println!("Got sysdaignose! Saving to file");

    let mut written = 0usize;
    let mut out = tokio::fs::File::create(&res.preferred_filename)
        .await
        .expect("no file?");
    while let Some(chunk) = res.stream.next().await {
        let buf = chunk.expect("stream stopped?");
        if !buf.is_empty() {
            out.write_all(&buf).await.expect("no write all?");
            written += buf.len();
        }
        println!("wrote {written}/{} bytes", res.expected_length);
    }
    println!("Done! Saved to {}", res.preferred_filename);
}
