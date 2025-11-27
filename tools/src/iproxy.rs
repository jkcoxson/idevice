// iproxy - Proxy tool to forward local TCP ports to specified ports on iOS devices
// Based on libusbmuxd/tools/iproxy.c implementation

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use clap::{Arg, Command};
use idevice::{
    ReadWrite,
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdDevice},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

const BUFFER_SIZE: usize = 32768;

/// Port pair configuration
#[derive(Debug, Clone)]
struct PortPair {
    /// Local listening port
    local_port: u16,
    /// Device port
    device_port: u16,
}

/// Lookup options
#[derive(Debug, Clone, Copy)]
struct LookupOptions {
    /// Whether to lookup USB devices
    usb: bool,
    /// Whether to lookup network devices
    network: bool,
}

impl LookupOptions {
    fn new(usb: bool, network: bool) -> Self {
        if !usb && !network {
            // Default to USB
            Self {
                usb: true,
                network: false,
            }
        } else {
            Self { usb, network }
        }
    }
}

/// Client connection data
struct ClientData {
    /// Device UDID (optional)
    udid: Option<String>,
    /// Device port
    device_port: u16,
    /// Lookup options
    lookup_opts: LookupOptions,
    /// usbmuxd address
    usbmuxd_addr: UsbmuxdAddr,
}

/// Handle a single client connection
async fn handle_client(
    mut client_stream: TcpStream,
    client_data: Arc<ClientData>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client_addr = client_stream.peer_addr()?;
    info!("Accepted new connection from {}", client_addr);

    // Get device
    let device = get_device(&client_data).await?;

    // Connect to device
    let device_stream =
        connect_to_device(&device, client_data.device_port, &client_data.usbmuxd_addr).await?;

    // Bidirectional data forwarding
    let (mut client_read, mut client_write) = client_stream.split();
    let (mut device_read, mut device_write) = tokio::io::split(device_stream);

    let client_to_device = async {
        let mut buffer = vec![0u8; BUFFER_SIZE];
        loop {
            match client_read.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Client connection closed");
                    break;
                }
                Ok(n) => {
                    if let Err(e) = device_write.write_all(&buffer[..n]).await {
                        error!("Failed to write to device: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from client: {}", e);
                    break;
                }
            }
        }
    };

    let device_to_client = async {
        let mut buffer = vec![0u8; BUFFER_SIZE];
        loop {
            match device_read.read(&mut buffer).await {
                Ok(0) => {
                    debug!("Device connection closed");
                    break;
                }
                Ok(n) => {
                    if let Err(e) = client_write.write_all(&buffer[..n]).await {
                        error!("Failed to write to client: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to read from device: {}", e);
                    break;
                }
            }
        }
    };

    // Wait for either direction to finish
    tokio::select! {
        _ = client_to_device => {},
        _ = device_to_client => {},
    }

    info!("Connection {} closed", client_addr);
    Ok(())
}

/// Get device
async fn get_device(
    client_data: &ClientData,
) -> Result<UsbmuxdDevice, Box<dyn std::error::Error + Send + Sync>> {
    let mut usbmuxd = client_data.usbmuxd_addr.connect(1).await?;

    if let Some(udid) = &client_data.udid {
        // Find device by UDID
        let device = usbmuxd.get_device(udid).await?;
        Ok(device)
    } else {
        // Get all devices and filter by lookup options
        let devices = usbmuxd.get_devices().await?;

        if devices.is_empty() {
            return Err("No connected devices found".into());
        }

        // Prefer USB devices (if allowed), otherwise select network devices
        for device in &devices {
            if client_data.lookup_opts.usb && device.connection_type == Connection::Usb {
                return Ok(device.clone());
            }
        }

        for device in &devices {
            if client_data.lookup_opts.network
                && let Connection::Network(_) = device.connection_type
            {
                return Ok(device.clone());
            }
        }

        // If no matching device found, return first device
        Ok(devices[0].clone())
    }
}

/// Connect to device
async fn connect_to_device(
    device: &UsbmuxdDevice,
    port: u16,
    usbmuxd_addr: &UsbmuxdAddr,
) -> Result<Box<dyn ReadWrite>, Box<dyn std::error::Error + Send + Sync>> {
    match &device.connection_type {
        Connection::Network(ip_addr) => {
            info!(
                "Requesting connection to NETWORK device {} (serial: {}), port {}",
                ip_addr, device.udid, port
            );
            let socket_addr = SocketAddr::new(*ip_addr, port);
            let stream = TcpStream::connect(socket_addr).await?;
            Ok(Box::new(stream) as Box<dyn ReadWrite>)
        }
        Connection::Usb => {
            info!(
                "Requesting connection to USB device handle {} (serial: {}), port {}",
                device.device_id, device.udid, port
            );
            let conn = usbmuxd_addr.connect(device.device_id).await?;
            let idevice = conn
                .connect_to_device(device.device_id, port, "iproxy")
                .await?;

            // Extract underlying socket from Idevice
            match idevice.get_socket() {
                Some(socket) => Ok(socket),
                None => Err("Unable to get device socket".into()),
            }
        }
        Connection::Unknown(desc) => Err(format!("Unsupported connection type: {}", desc).into()),
    }
}

/// Parse port pair
fn parse_port_pair(arg: &str) -> Result<PortPair, String> {
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid port pair format: {}", arg));
    }

    let local_port = parts[0]
        .parse::<u16>()
        .map_err(|_| format!("Invalid local port: {}", parts[0]))?;
    let device_port = parts[1]
        .parse::<u16>()
        .map_err(|_| format!("Invalid device port: {}", parts[1]))?;

    if local_port == 0 {
        return Err("Local port cannot be 0".into());
    }
    if device_port == 0 {
        return Err("Device port cannot be 0".into());
    }

    Ok(PortPair {
        local_port,
        device_port,
    })
}

/// Start listener
async fn start_listener(
    port_pair: PortPair,
    source_addr: Option<IpAddr>,
    client_data: Arc<ClientData>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind_addr = SocketAddr::new(
        source_addr.unwrap_or_else(|| "127.0.0.1".parse().unwrap()),
        port_pair.local_port,
    );

    let listener = TcpListener::bind(bind_addr).await?;
    info!(
        "Creating listening port {} for device port {}",
        port_pair.local_port, port_pair.device_port
    );

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                debug!(
                    "New connection: {} -> {}",
                    port_pair.local_port, port_pair.device_port
                );
                let client_data = Arc::clone(&client_data);

                tokio::spawn(async move {
                    if let Err(e) = handle_client(stream, client_data).await {
                        error!("Failed to handle client connection: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let matches = Command::new("iproxy")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Proxy that binds local TCP ports to be forwarded to the specified ports on a usbmux device")
        .arg(
            Arg::new("udid")
                .short('u')
                .long("udid")
                .value_name("UDID")
                .help("Target specific device by UDID"),
        )
        .arg(
            Arg::new("network")
                .short('n')
                .long("network")
                .action(clap::ArgAction::SetTrue)
                .help("Connect to network device"),
        )
        .arg(
            Arg::new("local")
                .short('l')
                .long("local")
                .action(clap::ArgAction::SetTrue)
                .help("Connect to USB device (default)"),
        )
        .arg(
            Arg::new("source")
                .short('s')
                .long("source")
                .value_name("ADDR")
                .help("Source address for listening socket (default 127.0.0.1)"),
        )
        .arg(
            Arg::new("port_pairs")
                .value_name("LOCAL_PORT:DEVICE_PORT")
                .help("Port pairs in LOCAL_PORT:DEVICE_PORT format")
                .required(true)
                .num_args(1..),
        )
        .get_matches();

    // Parse lookup options
    let usb = matches.get_flag("local");
    let network = matches.get_flag("network");
    let lookup_opts = LookupOptions::new(usb, network);

    // Parse UDID
    let udid = matches.get_one::<String>("udid").cloned();

    // Parse source address
    let source_addr = matches.get_one::<String>("source").map(|addr_str| {
        addr_str
            .parse::<IpAddr>()
            .unwrap_or_else(|_| panic!("Invalid source address: {}", addr_str))
    });

    // Parse port pairs
    let port_pairs_args: Vec<&String> = matches.get_many::<String>("port_pairs").unwrap().collect();

    let mut port_pairs = Vec::new();

    // Support old format: two separate arguments for port pair
    if port_pairs_args.len() == 2
        && !port_pairs_args[0].contains(':')
        && !port_pairs_args[1].contains(':')
    {
        let local_port = port_pairs_args[0]
            .parse::<u16>()
            .unwrap_or_else(|_| panic!("Invalid local port: {}", port_pairs_args[0]));
        let device_port = port_pairs_args[1]
            .parse::<u16>()
            .unwrap_or_else(|_| panic!("Invalid device port: {}", port_pairs_args[1]));

        if local_port == 0 {
            eprintln!("ERROR: Local port cannot be 0");
            std::process::exit(1);
        }
        if device_port == 0 {
            eprintln!("ERROR: Device port cannot be 0");
            std::process::exit(1);
        }

        port_pairs.push(PortPair {
            local_port,
            device_port,
        });
    } else {
        // New format: colon-separated port pairs
        for arg in port_pairs_args {
            match parse_port_pair(arg) {
                Ok(pair) => port_pairs.push(pair),
                Err(e) => {
                    eprintln!("ERROR: {}", e);
                    std::process::exit(1);
                }
            }
        }
    }

    if port_pairs.len() > 16 {
        eprintln!("ERROR: Too many port pairs, maximum is 16");
        std::process::exit(1);
    }

    // Get usbmuxd address
    let usbmuxd_addr = UsbmuxdAddr::from_env_var().unwrap_or_default();

    // Start listener for each port pair
    let mut tasks = Vec::new();

    for port_pair in port_pairs {
        let client_data = Arc::new(ClientData {
            udid: udid.clone(),
            device_port: port_pair.device_port,
            lookup_opts,
            usbmuxd_addr: usbmuxd_addr.clone(),
        });

        let task = tokio::spawn(start_listener(port_pair, source_addr, client_data));
        tasks.push(task);
    }

    info!("Waiting for connection...");

    // Wait for all tasks to complete (they will run indefinitely)
    for task in tasks {
        if let Err(e) = task.await {
            error!("Task failed: {}", e);
        }
    }
}
