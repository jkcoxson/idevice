// let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x282a, 0x9aff, 0xfedb, 0x8cbb);
// let addr = SocketAddrV6::new(ip, 60461, 0, 28);
// let conn = tokio::net::TcpStream::connect(addr).await.unwrap();

// Jackson Coxson

use std::{io::Write, net::IpAddr, str::FromStr, time::Duration};

use clap::{Arg, Command};
use futures_util::{StreamExt, pin_mut};
use idevice::remote_pairing::{RemotePairingClient, RpPairingFile};
use mdns::{Record, RecordKind};

const SERVICE_NAME: &'static str = "ncm._remoted._tcp.local.";

#[tokio::main]
async fn main() {
    // tracing_subscriber::fmt::init();

    let matches = Command::new("pair")
        .about("Pair with the device")
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("pair - pair with the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let stream = mdns::discover::all(SERVICE_NAME, Duration::from_secs(1))
        .unwrap()
        .listen();
    pin_mut!(stream);

    while let Some(Ok(response)) = stream.next().await {
        let addr = response.records().filter_map(self::to_ip_addr).next();

        if let Some(addr) = addr {
            println!("found cast device at {}", addr);
        } else {
            println!("cast device does not advertise address");
        }
    }
}

fn to_ip_addr(record: &Record) -> Option<IpAddr> {
    match record.kind {
        RecordKind::A(addr) => Some(addr.into()),
        RecordKind::AAAA(addr) => Some(addr.into()),
        _ => None,
    }
}
