// Jackson Coxson
//
// iOS 27 let a device initiate pairing to a computer instead of
// the other way around. The computer advertises an
// `_remotepairing-pairable-host._tcp` mDNS service. The device connects to the
// advertised port and drives the rppairing conversation, with this side acting
// as the SRP server/accessory. We generate and display a PIN that the user
// types into the device.

use std::io::Write;
use std::net::Ipv4Addr;

use clap::{Arg, Command};
use idevice::remote_pairing::{
    PAIRABLE_HOST_SERVICE_TYPE, PairableHost, PairableHostInfo, RpPairingFile, RpPairingSocket,
};
use mdns_sd::{ServiceDaemon, ServiceInfo};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("pair_host")
        .about("Advertise as a pairable host and accept a device-initiated pairing")
        .arg(
            Arg::new("name")
                .long("name")
                .value_name("NAME")
                .help("Name shown on the device")
                .default_value("idevice-rs"),
        )
        .arg(
            Arg::new("model")
                .long("model")
                .value_name("MODEL")
                .help("Hardware model identifier shown on the device")
                .default_value("Mac17,7"),
        )
        .arg(
            Arg::new("port")
                .long("port")
                .value_name("PORT")
                .help("TCP port to listen on (0 = pick a free port)")
                .default_value("0"),
        )
        .arg(
            Arg::new("out")
                .long("out")
                .value_name("PATH")
                .help("Where to write the resulting pairing file")
                .default_value("host_pairing_file.plist"),
        )
        .get_matches();

    let name = matches.get_one::<String>("name").unwrap().clone();
    let model = matches.get_one::<String>("model").unwrap().clone();
    let port: u16 = matches
        .get_one::<String>("port")
        .unwrap()
        .parse()
        .expect("invalid port");
    let out = matches.get_one::<String>("out").unwrap().clone();

    // Bind first so we can advertise the real port.
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port))
        .await
        .expect("failed to bind TCP listener");
    let port = listener.local_addr().expect("no local addr").port();

    // Our persistent-ish identity. A real consumer should persist both the
    // pairing file and `host_info.alt_irk` across runs so already-paired devices
    // keep recognizing this host; this PoC regenerates them each run.
    let mut pairing_file = RpPairingFile::generate(&name);
    let host_info = PairableHostInfo::generate(&name, &model);
    let service_identifier = pairing_file.identifier.clone();

    let mdns = ServiceDaemon::new().expect("failed to create mDNS daemon");
    mdns.set_service_name_len_max(30)
        .expect("failed to raise service name length limit");
    let hostname = format!("idevice-{}.local.", &service_identifier[..8]);
    let txt = host_info.mdns_txt_records(&service_identifier);
    let properties: Vec<(&str, &str)> = txt.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let service_info = ServiceInfo::new(
        PAIRABLE_HOST_SERVICE_TYPE,
        &service_identifier,
        &hostname,
        "",
        port,
        &properties[..],
    )
    .expect("invalid service info")
    .enable_addr_auto();
    mdns.register(service_info)
        .expect("failed to register mDNS service");

    println!("Advertising {PAIRABLE_HOST_SERVICE_TYPE} as \"{name}\" ({model})");
    println!("  identifier: {service_identifier}");
    println!("  port:       {port}");
    println!("\nWaiting for a device to connect and start pairing...");

    let (stream, peer_addr) = listener.accept().await.expect("accept failed");
    println!("Device connected from {peer_addr}");

    let socket = RpPairingSocket::new_device(stream);
    let mut host = PairableHost::new(socket, host_info);

    let peer_device = host
        .accept(&mut pairing_file, |pin| async move {
            println!("\n========================================");
            println!("  Enter this code on your device: {pin}");
            println!("========================================\n");
            std::io::stdout().flush().ok();
        })
        .await
        .expect("pairing failed");

    println!("Paired with device:");
    println!("  name:  {}", peer_device.name);
    println!("  model: {}", peer_device.model);
    println!("  udid:  {}", peer_device.remotepairing_udid);

    pairing_file
        .write_to_file(&out)
        .await
        .expect("failed to write pairing file");
    println!("\nPairing file written to {out}. Have a nice day.");
}
