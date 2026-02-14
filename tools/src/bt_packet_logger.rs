// Jackson Coxson

use futures_util::StreamExt;
use idevice::{IdeviceService, bt_packet_logger::BtPacketLoggerClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand};
use tokio::io::AsyncWrite;

use crate::pcap::{write_pcap_header, write_pcap_record};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Writes Bluetooth pcap data")
        .with_argument(JkArgument::new().with_help("Write PCAP to this file (use '-' for stdout)"))
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let out: Option<String> = arguments.clone().next_argument();

    let logger_client = BtPacketLoggerClient::connect(&*provider)
        .await
        .expect("Failed to connect to amfi");

    let mut s = logger_client.into_stream();

    // Open output (default to stdout if --out omitted)
    let mut out_writer: Box<dyn AsyncWrite + Unpin + Send> = match out.as_deref() {
        Some("-") | None => Box::new(tokio::io::stdout()),
        Some(path) => Box::new(tokio::fs::File::create(path).await.expect("open pcap")),
    };

    // Write global header
    write_pcap_header(&mut out_writer)
        .await
        .expect("pcap header");

    // Drain stream to PCAP
    while let Some(res) = s.next().await {
        match res {
            Ok(frame) => {
                write_pcap_record(
                    &mut out_writer,
                    frame.hdr.ts_secs,
                    frame.hdr.ts_usecs,
                    frame.kind,
                    &frame.h4,
                )
                .await
                .unwrap_or_else(|e| eprintln!("pcap write error: {e}"));
            }
            Err(e) => eprintln!("Failed to get next packet: {e:?}"),
        }
    }
}
