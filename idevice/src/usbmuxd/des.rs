// Jackson Coxson

use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListDevicesResponse {
    #[serde(rename = "DeviceList")]
    pub device_list: Vec<DeviceListResponse>,
}

#[derive(Deserialize)]
pub struct DeviceListResponse {
    #[serde(rename = "DeviceID")]
    pub device_id: u32,
    #[serde(rename = "Properties")]
    pub properties: DevicePropertiesResponse,
}

#[derive(Deserialize)]
pub struct DevicePropertiesResponse {
    #[serde(rename = "ConnectionType")]
    pub connection_type: String,
    #[serde(rename = "NetworkAddress")]
    pub network_address: Option<plist::Data>,
    #[serde(rename = "SerialNumber")]
    pub serial_number: String,
}
