// Manages the connection to the muxer

use async_trait::async_trait;
use plist::Value;
use serde::{Deserialize, Serialize};
use std::{env, io::Cursor};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, UnixStream},
    sync::Mutex,
};

const CLIENT_VERSION: &str = "idevice-rs 0.1.0";
const USBMUX_VERSION: u8 = 3;

lazy_static::lazy_static! {
    static ref TAG: Mutex<u32> = Mutex::new(0);
}

#[async_trait]
pub trait MuxerConnection {
    async fn read(&mut self) -> Result<Vec<u8>, std::io::Error>;
    async fn write(&mut self, buf: &[u8]) -> Result<(), std::io::Error>;
}

#[async_trait]
impl MuxerConnection for TcpStream {
    async fn read(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = [0; 1024];
        let size = AsyncReadExt::read(&mut self, &mut buf).await.unwrap();
        Ok(buf[..size].to_vec())
    }
    async fn write(&mut self, buf: &[u8]) -> Result<(), std::io::Error> {
        self.write_all(buf).await?;
        Ok(())
    }
}
#[async_trait]
impl MuxerConnection for UnixStream {
    async fn read(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let mut buf = [0; 1024];
        let size = AsyncReadExt::read(&mut self, &mut buf).await.unwrap();
        Ok(buf[..size].to_vec())
    }
    async fn write(&mut self, buf: &[u8]) -> Result<(), std::io::Error> {
        self.write_all(buf).await?;
        Ok(())
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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProperties {
    pub connection_type: String,
    pub device_id: u16,
    pub escaped_full_service_name: String,
    pub interface_index: u16,
    pub network_address: String,
    pub serial_number: String,
}

/// Creates a connection to the system's muxer
pub async fn connect() -> Result<Box<dyn MuxerConnection>, std::io::Error> {
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
        return Ok(Box::new(UnixStream::connect(address).await?));
    }
    return Ok(Box::new(TcpStream::connect(address).await?));
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
    let _ = plist::to_writer_xml(&mut to_send, &packet).unwrap();

    // Append the packet header to the beginning of the packet
    let version = (1 as u32).to_le_bytes();
    let message = (8 as u32).to_le_bytes();

    let tag = *TAG.lock().await;
    *TAG.lock().await += 1;
    let tag = tag.to_le_bytes();

    let size = (16 + to_send.len() as u32).to_le_bytes();

    let mut buf = Vec::new();
    buf.extend_from_slice(&size);
    buf.extend_from_slice(&version);
    buf.extend_from_slice(&message);
    buf.extend_from_slice(&tag);
    buf.extend_from_slice(&to_send);

    // Send the packet to the muxer
    connection.write(&buf).await?;

    // Read the response from the muxer
    let buf = connection.read().await?;
    let buf = buf[16..].to_vec();

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct ListEntry {
        device_id: u16,
        message_type: String,
        properties: DeviceProperties,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Response {
        device_list: Vec<ListEntry>,
    }

    let mut cursor = Cursor::new(buf);
    let response: Response = plist::from_reader(&mut cursor).unwrap();

    Ok(response
        .device_list
        .into_iter()
        .map(|x| x.properties)
        .collect())
}
