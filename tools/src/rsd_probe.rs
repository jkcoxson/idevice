// Probe an already-established tunnel (e.g. utun17)
// for the device's RemoteServiceDiscovery port. Connects real TCP sockets (no
// CoreDeviceProxy software tunnel) so we can reach the device the same way
// Device Hub does.
//
//   cargo run -p idevice-tools --bin rsd_probe -- fd68:fb1f:92ef::1 49000 50000

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use idevice::rsd::RsdHandshake;
use tokio::net::TcpStream;

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let ip: IpAddr = args
        .next()
        .expect("usage: rsd_probe <device-ip> [start] [end]")
        .parse()
        .expect("bad ip");
    let start: u16 = args.next().map(|s| s.parse().unwrap()).unwrap_or(49000);
    let end: u16 = args.next().map(|s| s.parse().unwrap()).unwrap_or(50000);

    println!("probing {ip} ports {start}..={end} for RSD ...");
    for port in start..=end {
        let addr = SocketAddr::new(ip, port);
        let stream = match tokio::time::timeout(
            Duration::from_millis(300),
            TcpStream::connect(addr),
        )
        .await
        {
            Ok(Ok(s)) => s,
            _ => continue,
        };
        match tokio::time::timeout(Duration::from_secs(3), RsdHandshake::new(stream)).await {
            Ok(Ok(hs)) => {
                println!(
                    "\n*** RSD on port {port} - {} services ***",
                    hs.services.len()
                );
                let mut names: Vec<_> = hs.services.keys().cloned().collect();
                names.sort();
                for n in &names {
                    println!("  >> {n} -> port {:?}", hs.services.get(n).map(|s| &s.port));
                }
                println!("  (total {} services)", names.len());
                return;
            }
            Ok(Err(e)) => println!("port {port}: open but not RSD ({e})"),
            Err(_) => println!("port {port}: open but RSD handshake timed out"),
        }
    }
    println!("no RSD found in range");
}
