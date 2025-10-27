// Jackson Coxson

use clap::{Arg, Command};
use futures_util::StreamExt;
use idevice::{
    IdeviceService, RsdService, core_device::DiagnostisServiceClient,
    core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake,
};
use tokio::io::AsyncWriteExt;

mod common;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("remotexpc")
        .about("Gets a sysdiagnose")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("IP address of the device"),
        )
        .arg(
            Arg::new("pairing_file")
                .long("pairing-file")
                .value_name("PATH")
                .help("Path to the pairing file"),
        )
        .arg(
            Arg::new("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)")
                .index(1),
        )
        .arg(
            Arg::new("tunneld")
                .long("tunneld")
                .help("Use tunneld")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("debug_proxy - connect to the debug proxy and run commands");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let provider =
        match common::get_provider(udid, host, pairing_file, "diagnosticsservice-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };
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
