// let ip = Ipv6Addr::new(0xfe80, 0, 0, 0, 0x282a, 0x9aff, 0xfedb, 0x8cbb);
// let addr = SocketAddrV6::new(ip, 60461, 0, 28);
// let conn = tokio::net::TcpStream::connect(addr).await.unwrap();

// Jackson Coxson

use std::{
    any::Any,
    net::{IpAddr, SocketAddr, SocketAddrV6},
    sync::Arc,
    time::Duration,
};
#[cfg(target_os = "linux")]
use std::{fs, process::Command as OsCommand};

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

            let host_name = result.host_name().to_string();
            let service_address = result.address().to_string();
            let scope_id = link_local_scope_id_from_avahi(&host_name, &service_address);

            let stream = match connect_to_service_port(
                &host_name,
                &service_address,
                scope_id,
                *result.port(),
            )
            .await
            {
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
            let stream = connect_to_service_port(&host_name, &service_address, scope_id, ts.port)
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

async fn connect_to_service_port(
    host_name: &str,
    service_address: &str,
    scope_id: Option<u32>,
    port: u16,
) -> Option<TcpStream> {
    if let Some(stream) = lookup_host_and_connect(host_name, port).await {
        return Some(stream);
    }

    let addr: IpAddr = match service_address.parse() {
        Ok(addr) => addr,
        Err(e) => {
            println!("failed to parse resolved service address {service_address}: {e}");
            return None;
        }
    };

    let socket = match addr {
        IpAddr::V6(v6) if v6.is_unicast_link_local() => {
            SocketAddr::V6(SocketAddrV6::new(v6, port, 0, scope_id.unwrap_or(0)))
        }
        _ => SocketAddr::new(addr, port),
    };

    println!("using resolved service address fallback: {socket}");

    match TcpStream::connect(socket).await {
        Ok(s) => {
            println!("connected with local addr {:?}", s.local_addr());
            Some(s)
        }
        Err(e) => {
            println!("failed to connect with service address fallback: {e:?}");
            None
        }
    }
}

async fn lookup_host_and_connect(host: &str, port: u16) -> Option<TcpStream> {
    let looked_up = match tokio::net::lookup_host((host, port)).await {
        Ok(addrs) => addrs,
        Err(e) => {
            println!("hostname lookup failed for {host}:{port}: {e}");
            return None;
        }
    };

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

#[cfg(target_os = "linux")]
fn link_local_scope_id_from_avahi(host_name: &str, service_address: &str) -> Option<u32> {
    let output = OsCommand::new("avahi-browse")
        .args(["-rpt", "_remoted._tcp"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if !line.starts_with("=;") {
            continue;
        }

        let parts: Vec<&str> = line.split(';').collect();
        if parts.len() < 9 {
            continue;
        }

        let ifname = parts[1];
        let resolved_host = parts[6];
        let resolved_addr = parts[7];
        if resolved_host == host_name && resolved_addr == service_address {
            let ifindex_path = format!("/sys/class/net/{ifname}/ifindex");
            return fs::read_to_string(ifindex_path).ok()?.trim().parse().ok();
        }
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn link_local_scope_id_from_avahi(_host_name: &str, _service_address: &str) -> Option<u32> {
    None
}
