// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    IdeviceService,
    pcapd::{PcapFileWriter, PcapdClient},
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("pcapd")
        .about("Capture IP packets")
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
    let out = matches.get_one::<String>("out").map(String::to_owned);

    let provider = match common::get_provider(udid, None, None, "pcapd-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let mut logger_client = PcapdClient::connect(&*provider)
        .await
        .expect("Failed to connect to pcapd! This service is only available over USB!");

    logger_client.next_packet().await.unwrap();

    // Open output (default to stdout if --out omitted)
    let mut out_writer = match out.as_deref() {
        Some(path) => Some(
            PcapFileWriter::new(tokio::fs::File::create(path).await.expect("open pcap"))
                .await
                .expect("write header"),
        ),
        _ => None,
    };

    println!("Starting packet stream");
    loop {
        let packet = logger_client
            .next_packet()
            .await
            .expect("failed to read next packet");
        if let Some(writer) = &mut out_writer {
            writer.write_packet(&packet).await.expect("write packet");
        } else {
            println!("{packet:?}");
        }
    }
}
