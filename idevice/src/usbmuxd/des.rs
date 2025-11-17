// Jackson Coxson

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use serde::Deserialize;
use tracing::{debug, warn};

use crate::{
    IdeviceError,
    usbmuxd::{Connection, UsbmuxdDevice},
};

#[derive(Deserialize)]
pub struct ListDevicesResponse {
    #[serde(rename = "DeviceList")]
    pub device_list: Vec<DeviceListResponse>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DeviceListResponse {
    #[serde(rename = "DeviceID")]
    pub device_id: u32,
    #[serde(rename = "Properties")]
    pub properties: DevicePropertiesResponse,
}

#[derive(Clone, Debug, Deserialize)]
pub struct DevicePropertiesResponse {
    #[serde(rename = "ConnectionType")]
    pub connection_type: String,
    #[serde(rename = "NetworkAddress")]
    pub network_address: Option<plist::Data>,
    #[serde(rename = "SerialNumber")]
    pub serial_number: String,
}

impl DeviceListResponse {
    pub fn into_usbmuxd_dev(self) -> Result<UsbmuxdDevice, IdeviceError> {
        self.try_into()
    }
}

impl TryFrom<DeviceListResponse> for UsbmuxdDevice {
    type Error = IdeviceError;

    fn try_from(dev: DeviceListResponse) -> Result<Self, Self::Error> {
        let connection_type = match dev.properties.connection_type.as_str() {
            "Network" => {
                if let Some(addr) = dev.properties.network_address {
                    let addr = &Into::<Vec<u8>>::into(addr);
                    if addr.len() < 8 {
                        warn!("Device address bytes len < 8");
                        return Err(IdeviceError::UnexpectedResponse);
                    }

                    match addr[0] {
                        0x02 => {
                            // IPv4
                            Connection::Network(IpAddr::V4(Ipv4Addr::new(
                                addr[4], addr[5], addr[6], addr[7],
                            )))
                        }
                        0x1E => {
                            // IPv6
                            if addr.len() < 24 {
                                warn!("IPv6 address is less than 24 bytes");
                                return Err(IdeviceError::UnexpectedResponse);
                            }

                            Connection::Network(IpAddr::V6(Ipv6Addr::new(
                                u16::from_be_bytes([addr[8], addr[9]]),
                                u16::from_be_bytes([addr[10], addr[11]]),
                                u16::from_be_bytes([addr[12], addr[13]]),
                                u16::from_be_bytes([addr[14], addr[15]]),
                                u16::from_be_bytes([addr[16], addr[17]]),
                                u16::from_be_bytes([addr[18], addr[19]]),
                                u16::from_be_bytes([addr[20], addr[21]]),
                                u16::from_be_bytes([addr[22], addr[23]]),
                            )))
                        }
                        0x1C => {
                            if addr.len() < 28 {
                                warn!("IPv6 sockaddr_in6 data too short (len {})", addr.len());
                                return Err(IdeviceError::UnexpectedResponse);
                            }
                            if addr[1] == 0x1E {
                                // IPv6 address starts at offset 8 in sockaddr_in6
                                Connection::Network(IpAddr::V6(Ipv6Addr::new(
                                    u16::from_be_bytes([addr[8], addr[9]]),
                                    u16::from_be_bytes([addr[10], addr[11]]),
                                    u16::from_be_bytes([addr[12], addr[13]]),
                                    u16::from_be_bytes([addr[14], addr[15]]),
                                    u16::from_be_bytes([addr[16], addr[17]]),
                                    u16::from_be_bytes([addr[18], addr[19]]),
                                    u16::from_be_bytes([addr[20], addr[21]]),
                                    u16::from_be_bytes([addr[22], addr[23]]),
                                )))
                            } else {
                                warn!(
                                    "Expected IPv6 family (0x1E) but got {:02X} for length 0x1C",
                                    addr[1]
                                );
                                Connection::Unknown(format!("Network {:02X}", addr[1]))
                            }
                        }
                        _ => {
                            warn!("Unknown IP address protocol: {:02X}", addr[0]);
                            Connection::Unknown(format!("Network {:02X}", addr[0]))
                        }
                    }
                } else {
                    warn!("Device is network attached, but has no network info");
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            "USB" => Connection::Usb,
            _ => Connection::Unknown(dev.properties.connection_type),
        };
        debug!("Connection type: {connection_type:?}");
        Ok(UsbmuxdDevice {
            connection_type,
            udid: dev.properties.serial_number,
            device_id: dev.device_id,
        })
    }
}
