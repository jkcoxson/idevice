// Shim code for using pymobiledevice3's tunneld

use std::{collections::HashMap, net::SocketAddr};

use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::IdeviceError;

pub const DEFAULT_PORT: u16 = 49151;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunneldDevice {
    pub interface: String,
    #[serde(rename = "tunnel-address")]
    pub tunnel_address: String,
    #[serde(rename = "tunnel-port")]
    pub tunnel_port: u16,
}

pub async fn get_tunneld_devices(
    socket: SocketAddr,
) -> Result<HashMap<String, TunneldDevice>, IdeviceError> {
    let res: Value = reqwest::get(format!("http://{socket}"))
        .await?
        .json()
        .await?;

    let res = match res.as_object() {
        Some(r) => r,
        None => {
            warn!("tunneld return type wasn't a dictionary");
            return Err(IdeviceError::UnexpectedResponse);
        }
    };

    let mut to_return = HashMap::new();
    for (udid, v) in res.into_iter() {
        let mut v: Vec<TunneldDevice> = match serde_json::from_value(v.clone()) {
            Ok(v) => v,
            Err(e) => {
                warn!("Failed to parse tunneld results as vector of struct: {e:?}");
                continue;
            }
        };

        if v.is_empty() {
            warn!("Device had no entries");
            continue;
        }

        to_return.insert(udid.clone(), v.remove(0));
    }

    Ok(to_return)
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, str::FromStr};

    use super::*;

    #[tokio::test]
    async fn t1() {
        let host = SocketAddr::new(IpAddr::from_str("127.0.0.1").unwrap(), DEFAULT_PORT);
        println!("{:#?}", get_tunneld_devices(host).await);
    }
}
