// let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x282a, 0x9aff, 0xfedb, 0x8cbb);
// let addr = SocketAddrV6::new(ip, 60461, 0, 28);
// let conn = tokio::net::TcpStream::connect(addr).await.unwrap();

// Jackson Coxson

use std::{any::Any, sync::Arc, time::Duration};

use clap::{Arg, Command};
use idevice::{
    RemoteXpcClient,
    remote_pairing::{RemotePairingClient, RpPairingFile},
    rsd::RsdHandshake,
};
use tokio::net::TcpStream;
use zeroconf::{
    BrowserEvent, MdnsBrowser, ServiceType,
    prelude::{TEventLoop, TMdnsBrowser},
};

const SERVICE_NAME: &str = "remoted";
const SERVICE_PROTOCOL: &str = "tcp";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

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

    let mut browser = MdnsBrowser::new(
        ServiceType::new(SERVICE_NAME, SERVICE_PROTOCOL).expect("Unable to start mDNS browse"),
    );
    browser.set_service_callback(Box::new(on_service_discovered));

    let event_loop = browser.browse_services().unwrap();

    loop {
        // calling `poll()` will keep this browser alive
        event_loop.poll(Duration::from_secs(0)).unwrap();
    }
}

fn on_service_discovered(
    result: zeroconf::Result<BrowserEvent>,
    _context: Option<Arc<dyn Any + Send + Sync>>,
) {
    if let Ok(BrowserEvent::Add(result)) = result {
        tokio::task::spawn(async move {
            println!("Found iOS device to pair with!! - {result:?}");

            let stream = match lookup_host_and_connect(result.host_name(), 58783).await {
                Some(s) => s,
                None => {
                    println!("Couldn't open TCP port on device");
                    return;
                }
            };

            let handshake = RsdHandshake::new(stream).await.expect("no rsd");

            println!("handshake: {handshake:#?}");

            let ts = handshake
                .services
                .get("com.apple.internal.dt.coredevice.untrusted.tunnelservice")
                .unwrap();

            println!("connecting to tunnel service");
            let stream = lookup_host_and_connect(result.host_name(), ts.port)
                .await
                .expect("failed to connect to tunnselservice");
            let mut conn = RemoteXpcClient::new(stream).await.unwrap();

            println!("doing tunnel service handshake");
            conn.do_handshake().await.unwrap();

            let msg = conn.recv_root().await.unwrap();
            println!("{msg:#?}");

            let host = "idevice-rs-jkcoxson";
            let mut rpf = RpPairingFile::generate(host);
            let mut rpc = RemotePairingClient::new(conn, host, &mut rpf);
            rpc.connect(
                async |_| "000000".to_string(),
                0u8, // we need no state, so pass a single byte that will hopefully get optimized out
            )
            .await
            .expect("no pair");

            rpf.write_to_file("ios_pairing_file.plist").await.unwrap();
            println!(
                "congrats you're paired now, the rppairing record has been saved. Have a nice day."
            );
        });
    }
}

async fn lookup_host_and_connect(host: &str, port: u16) -> Option<TcpStream> {
    let looked_up = tokio::net::lookup_host(format!("{}:{}", host, port))
        .await
        .unwrap();

    let mut stream = None;
    for l in looked_up {
        if l.is_ipv4() {
            continue;
        }

        println!("Found IP: {l:?}");

        match tokio::net::TcpStream::connect(l).await {
            Ok(s) => {
                println!("connected with local addr {:?}", s.local_addr());
                stream = Some(s);
                break;
            }
            Err(e) => println!("failed to connect: {e:?}"),
        }
    }

    stream
}
