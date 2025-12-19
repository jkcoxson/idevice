// Jackson Coxson
// A PoC to pair by IP
// Ideally you'd browse by mDNS in production

use std::{io::Write, net::IpAddr, str::FromStr};

use clap::{Arg, Command};
use idevice::remote_pairing::{RemotePairingClient, RpPairingFile};

#[tokio::main]
async fn main() {
    // tracing_subscriber::fmt::init();

    let matches = Command::new("pair")
        .about("Pair with the device")
        .arg(
            Arg::new("ip")
                .value_name("IP")
                .help("The IP of the Apple TV")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("port")
                .value_name("port")
                .help("The port of the Apple TV")
                .required(true)
                .index(2),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("pair - pair with the Apple TV");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let ip = matches.get_one::<String>("ip").expect("no IP passed");
    let port = matches.get_one::<String>("port").expect("no port passed");
    let port = port.parse::<u16>().unwrap();

    let conn =
        tokio::net::TcpStream::connect((IpAddr::from_str(ip).expect("failed to parse IP"), port))
            .await
            .expect("Failed to connect");

    let host = "idevice-rs-jkcoxson";
    let mut rpf = RpPairingFile::generate(host);
    let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);
    rpc.connect(
        async |_| {
            let mut buf = String::new();
            print!("Enter the Apple TV pin: ");
            std::io::stdout().flush().unwrap();
            std::io::stdin()
                .read_line(&mut buf)
                .expect("Failed to read line");
            buf.trim_end().to_string()
        },
        0u8, // we need no state, so pass a single byte that will hopefully get optimized out
    )
    .await
    .expect("no pair");

    // now that we are paired, we should be good
    println!("Reconnecting...");
    let conn =
        tokio::net::TcpStream::connect((IpAddr::from_str(ip).expect("failed to parse IP"), port))
            .await
            .expect("Failed to connect");
    let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);
    rpc.connect(
        async |_| {
            panic!("we tried to pair again :(");
        },
        0u8,
    )
    .await
    .expect("no reconnect");

    rpf.write_to_file("atv_pairing_file.plist").await.unwrap();
    println!("Pairing file validated and written to disk. Have a nice day.");
}
