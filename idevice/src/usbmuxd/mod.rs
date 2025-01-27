// Jackson Coxson

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

#[cfg(target_os = "windows")]
use std::net::{Ipv4Addr, SocketAddrV4};

use log::debug;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{pairing_file::PairingFile, IdeviceError, ReadWrite};

mod raw_packet;

#[derive(Debug, Clone)]
pub enum Connection {
    Usb,
    Network(IpAddr),
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct UsbmuxdDevice {
    pub connection_type: Connection,
    pub udid: String,
    pub device_id: u32,
}

pub struct UsbmuxdConnection {
    socket: Box<dyn ReadWrite>,
    tag: u32,
}

#[derive(Deserialize)]
struct ListDevicesResponse {
    #[serde(rename = "DeviceList")]
    device_list: Vec<DeviceListResponse>,
}

#[derive(Deserialize)]
struct DeviceListResponse {
    #[serde(rename = "DeviceID")]
    device_id: u32,
    #[serde(rename = "Properties")]
    properties: DevicePropertiesResponse,
}

#[derive(Deserialize)]
struct DevicePropertiesResponse {
    #[serde(rename = "ConnectionType")]
    connection_type: String,
    #[serde(rename = "NetworkAddress")]
    network_address: Option<plist::Data>,
    #[serde(rename = "SerialNumber")]
    serial_number: String,
}

impl UsbmuxdConnection {
    pub const DEFAULT_PORT: u16 = 27015;
    pub const SOCKET_FILE: &str = "/var/run/usbmuxd";

    pub const BINARY_PLIST_VERSION: u32 = 0;
    pub const XML_PLIST_VERSION: u32 = 1;

    pub const RESULT_MESSAGE_TYPE: u32 = 1;
    pub const PLIST_MESSAGE_TYPE: u32 = 8;

    pub async fn default() -> Result<Self, IdeviceError> {
        #[cfg(target_os = "windows")]
        let socket = tokio::net::TcpStream::connect(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            Self::DEFAULT_PORT,
        ))
        .await?;

        #[cfg(any(target_os = "macos", target_os = "linux"))]
        let socket = tokio::net::UnixStream::connect(Self::SOCKET_FILE).await?;

        Ok(Self {
            socket: Box::new(socket),
            tag: 0,
        })
    }

    pub async fn new(socket: Box<dyn ReadWrite>, tag: u32) -> Self {
        Self { socket, tag }
    }

    pub async fn get_devices(&mut self) -> Result<Vec<UsbmuxdDevice>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("MessageType".into(), "ListDevices".into());
        req.insert("ClientVersionString".into(), "idevice-rs".into());
        req.insert("kLibUSBMuxVersion".into(), 3.into());
        self.write_plist(req).await?;
        let res = self.read_plist().await?;
        let res = plist::to_value(&res)?;
        let res = plist::from_value::<ListDevicesResponse>(&res)?;

        let mut devs = Vec::new();
        for dev in res.device_list {
            let connection_type = match dev.properties.connection_type.as_str() {
                "Network" => {
                    if let Some(addr) = dev.properties.network_address {
                        let addr = &Into::<Vec<u8>>::into(addr);
                        if addr.len() < 8 {
                            return Err(IdeviceError::UnexpectedResponse);
                        }

                        let addr = match addr[0] {
                            0x02 => {
                                // ipv4
                                IpAddr::V4(Ipv4Addr::new(addr[4], addr[5], addr[6], addr[7]))
                            }
                            0x1E => {
                                // ipv6
                                if addr.len() < 24 {
                                    return Err(IdeviceError::UnexpectedResponse);
                                }

                                IpAddr::V6(Ipv6Addr::new(
                                    u16::from_le_bytes([addr[8], addr[9]]),
                                    u16::from_le_bytes([addr[10], addr[11]]),
                                    u16::from_le_bytes([addr[12], addr[13]]),
                                    u16::from_le_bytes([addr[14], addr[15]]),
                                    u16::from_le_bytes([addr[16], addr[17]]),
                                    u16::from_le_bytes([addr[18], addr[19]]),
                                    u16::from_le_bytes([addr[20], addr[21]]),
                                    u16::from_le_bytes([addr[22], addr[23]]),
                                ))
                            }
                            _ => {
                                return Err(IdeviceError::UnexpectedResponse);
                            }
                        };
                        Connection::Network(addr)
                    } else {
                        return Err(IdeviceError::UnexpectedResponse);
                    }
                }
                "USB" => Connection::Usb,
                _ => Connection::Unknown(dev.properties.connection_type),
            };
            devs.push(UsbmuxdDevice {
                connection_type,
                udid: dev.properties.serial_number,
                device_id: dev.device_id,
            })
        }

        Ok(devs)
    }

    pub async fn get_pair_record(&mut self, udid: &str) -> Result<PairingFile, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("MessageType".into(), "ReadPairRecord".into());
        req.insert("PairRecordID".into(), udid.into());
        self.write_plist(req).await?;
        let res = self.read_plist().await?;

        match res.get("PairRecordData") {
            Some(plist::Value::Data(d)) => PairingFile::from_bytes(d),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn get_buid(&mut self) -> Result<String, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("MessageType".into(), "ReadBUID".into());
        self.write_plist(req).await?;
        let mut res = self.read_plist().await?;

        match res.remove("BUID") {
            Some(plist::Value::String(s)) => Ok(s),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn connect_to_device(
        mut self,
        device_id: u32,
        port: u16,
    ) -> Result<Box<dyn ReadWrite>, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("MessageType".into(), "Connect".into());
        req.insert("DeviceID".into(), device_id.into());
        req.insert("PortNumber".into(), port.into());
        self.write_plist(req).await?;
        match self.read_plist().await?.get("Number") {
            Some(plist::Value::Integer(i)) => match i.as_unsigned() {
                Some(0) => Ok(self.socket),
                _ => Err(IdeviceError::UnexpectedResponse),
            },
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }

    async fn write_plist(&mut self, req: plist::Dictionary) -> Result<(), IdeviceError> {
        let raw = raw_packet::RawPacket::new(
            req,
            Self::XML_PLIST_VERSION,
            Self::PLIST_MESSAGE_TYPE,
            self.tag,
        );

        let raw: Vec<u8> = raw.into();
        self.socket.write_all(&raw).await?;

        Ok(())
    }

    async fn read_plist(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        let mut header_buffer = [0; 16];
        self.socket.read_exact(&mut header_buffer).await?;

        // We are safe to unwrap as it only panics if the buffer isn't 4
        let packet_size = u32::from_le_bytes(header_buffer[..4].try_into().unwrap()) - 16;
        debug!("Reading {packet_size} bytes from muxer");

        let mut body_buffer = vec![0; packet_size as usize];
        self.socket.read_exact(&mut body_buffer).await?;

        let res = plist::from_bytes(&body_buffer)?;

        Ok(res)
    }
}
