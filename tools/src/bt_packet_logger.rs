// Jackson Coxson

use clap::{Arg, Command};
use futures_util::StreamExt;
use idevice::{IdeviceService, bt_packet_logger::BtPacketLoggerClient};
use tokio::io::AsyncWrite;

use crate::pcap::{write_pcap_header, write_pcap_record};

mod common;
mod pcap;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("amfi")
        .about("Capture Bluetooth packets")
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
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("out")
                .long("out")
                .value_name("PCAP")
                .help("Write PCAP to this file (use '-' for stdout)"),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("bt_packet_logger - capture bluetooth packets");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let out = matches.get_one::<String>("out").map(String::to_owned);

    let provider = match common::get_provider(udid, host, pairing_file, "amfi-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

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
