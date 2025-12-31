// let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x282a, 0x9aff, 0xfedb, 0x8cbb);
// let addr = SocketAddrV6::new(ip, 60461, 0, 28);
// let conn = tokio::net::TcpStream::connect(addr).await.unwrap();

// Jackson Coxson

use std::{any::Any, io::Write, net::IpAddr, str::FromStr, sync::Arc, time::Duration};

use clap::{Arg, Command};
use futures_util::{StreamExt, pin_mut};
use idevice::remote_pairing::{RemotePairingClient, RpPairingFile};
use zeroconf::{
    BrowserEvent, MdnsBrowser, ServiceDiscovery, ServiceType,
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
            println!("{result:?}");
        });
    }
}
