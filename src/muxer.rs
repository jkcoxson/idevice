// Manages the connection to the muxer

use log::warn;
use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;
use std::{env, io::Cursor};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UnixStream},
    sync::Mutex,
};

const CLIENT_VERSION: &str = "idevice-rs 0.1.0";
const USBMUX_VERSION: u8 = 3;

lazy_static::lazy_static! {
    static ref TAG: Mutex<u32> = Mutex::new(1);
}

pub struct MuxerConnection {
    pub unix_stream: Option<UnixStream>,
    pub tcp_stream: Option<TcpStream>,
}

impl MuxerConnection {
    pub async fn read(&mut self) -> Result<Vec<u8>, std::io::Error> {
        match self.unix_stream {
            Some(ref mut unix_stream) => {
                let mut buf = [0; 4];
                unix_stream.read_exact(&mut buf).await?;
                let len = u32::from_le_bytes(buf);

                let mut buf = vec![0; len as usize];
                let size = unix_stream.read(&mut buf).await?;
                return Ok(buf[..size].to_vec());
            }
            None => {
                let mut buf = [0; 4];
                self.tcp_stream
                    .as_mut()
                    .unwrap()
                    .read_exact(&mut buf)
                    .await?;
                let len = u32::from_le_bytes(buf);

                let mut buf = vec![0; len as usize];
                let size = self.tcp_stream.as_mut().unwrap().read(&mut buf).await?;
                return Ok(buf[..size].to_vec());
            }
        };
    }

    pub async fn write(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        match self.unix_stream {
            Some(ref mut unix_stream) => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&((data.len() + 4) as u32).to_le_bytes());
                buf.extend_from_slice(data);
                unix_stream.write_all(&buf).await?;
                return Ok(());
            }
            None => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&((data.len() + 4) as u32).to_le_bytes());
                buf.extend_from_slice(data);
                self.tcp_stream.as_mut().unwrap().write_all(&buf).await?;
                self.tcp_stream.as_mut().unwrap().flush().await?;
                return Ok(());
            }
        };
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PacketBase {
    client_version_string: String,
    message_type: String,
    prog_name: String,
    #[serde(rename = "kLibUSBMuxVersion")]
    k_lib_usbmux_version: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProperties {
    pub connection_speed: Option<u32>,
    pub connection_type: String,
    #[serde(alias = "DeviceID")]
    pub device_id: u16,
    pub location_id: Option<u32>,
    pub escaped_full_service_name: Option<String>,
    pub interface_index: Option<u16>,
    pub network_address: Option<ByteBuf>,
    pub serial_number: String,
}

impl DeviceProperties {
    pub fn get_ip(&self) -> Option<String> {
        if self.connection_type != "Network" {
            return None;
        }

        let data = self.network_address.clone().unwrap();
        // Determine if the data is IPv4 or IPv6
        match data[1] {
            0x02 => {
                // IPv4
                let mut ip_addr = [0u8; 4];
                ip_addr.copy_from_slice(&data[4..8]);
                let ip_addr = std::net::Ipv4Addr::from(ip_addr);
                Some(ip_addr.to_string())
            }
            0x1E => {
                // IPv6
                let mut ip_addr = [0u8; 16];
                ip_addr.copy_from_slice(&data[8..24]);
                let ip_addr = std::net::Ipv6Addr::from(ip_addr);
                Some(ip_addr.to_string())
            }
            _ => {
                warn!("Unknown IP address type");
                None
            }
        }
    }
}

/// Creates a connection to the system's muxer
pub async fn connect() -> Result<MuxerConnection, std::io::Error> {
    // Get the USBMUXD_SOCKET_ADDRESS environment variable
    let address = match env::var("USBMUXD_SOCKET_ADDRESS") {
        Ok(address) => address,
        Err(_) => match std::env::consts::OS {
            "linux" => "/var/run/usbmuxd".to_string(),
            "macos" => "/var/run/usbmuxd".to_string(),
            "windows" => "127.0.0.1:27015".to_string(),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Specify the address of the muxer using the USBMUXD_SOCKET_ADDRESS environment variable",
                ))
            }
        },
    };

    // Determine if the address is a path or a socket
    if address.starts_with("/") {
        return Ok(MuxerConnection {
            unix_stream: Some(UnixStream::connect(address).await?),
            tcp_stream: None,
        });
    }
    return Ok(MuxerConnection {
        unix_stream: None,
        tcp_stream: Some(TcpStream::connect(address).await?),
    });
}

pub async fn get_devices(
    program_name: impl Into<String>,
) -> Result<Vec<DeviceProperties>, std::io::Error> {
    let mut connection = connect().await?;
    let packet = PacketBase {
        client_version_string: CLIENT_VERSION.to_string(),
        message_type: "ListDevices".to_string(),
        prog_name: program_name.into(),
        k_lib_usbmux_version: USBMUX_VERSION,
    };

    // Serialize the packet to a plist
    let mut to_send = Vec::new();
    let _ = match plist::to_writer_xml(&mut to_send, &packet) {
        Ok(_) => (),
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to serialize packet: {}", e),
            ));
        }
    };

    // Append the packet header to the beginning of the packet
    let version = (1 as u32).to_le_bytes();
    let message = (8 as u32).to_le_bytes();

    let tag = *TAG.lock().await;
    *TAG.lock().await += 1;
    let tag = tag.to_le_bytes();

    let mut buf = Vec::new();
    buf.extend_from_slice(&version);
    buf.extend_from_slice(&message);
    buf.extend_from_slice(&tag);
    buf.extend_from_slice(&to_send);

    // Send the packet to the muxer
    connection.write(&buf).await?;

    // Read the response from the muxer
    let buf = connection.read().await?;
    let buf = buf[12..].to_vec();

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct ListEntry {
        properties: DeviceProperties,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Response {
        device_list: Vec<ListEntry>,
    }

    let mut cursor = Cursor::new(buf);
    let response: Response = match plist::from_reader(&mut cursor) {
        Ok(device_list) => device_list,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unable to deserialize packet",
            ))
        }
    };

    Ok(response
        .device_list
        .into_iter()
        .map(|x| x.properties)
        .collect())
}
