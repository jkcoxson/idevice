// jkcoxson

use log::info;
use serde::{Deserialize, Serialize};

use crate::{connection::Connection, muxer::DeviceProperties, pairing_file::PairingFile};

const LOCKDOWND_PORT: u16 = 62078;

pub struct LockdowndClient {
    pub connection: Connection,

    // Private property caches
    service_type: Option<String>,
    product_version: Option<String>,
}

impl LockdowndClient {
    pub async fn new(
        properties: &DeviceProperties,
        label: impl Into<String>,
    ) -> Result<LockdowndClient, std::io::Error> {
        let mut client = LockdowndClient {
            connection: Connection::new(properties, LOCKDOWND_PORT, label).await?,
            service_type: None,
            product_version: None,
        };

        if client.get_service_type().await? == "com.apple.mobile.lockdown" {
            Ok(client)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unexpected service type",
            ))
        }
    }

    /// Gets the service type of the connection
    /// This is cached after the first call
    /// # Returns
    /// The service type of the connection as a string
    pub async fn get_service_type(&mut self) -> Result<String, std::io::Error> {
        if self.service_type.is_some() {
            info!("Returning cached service type");
            return Ok(self.service_type.clone().unwrap());
        }
        // Query the device for the connection type
        let query = Query {
            label: self.connection.label.clone(),
            request: "QueryType".to_string(),
        };

        self.connection.write_plist(&query).await?;

        let res: QueryRes = self.connection.read_plist().await?;

        self.service_type = Some(res.type_.clone());

        Ok(res.type_)
    }

    /// Gets the iOS version of the device
    pub async fn get_product_version(&mut self) -> Result<String, std::io::Error> {
        if self.product_version.is_some() {
            info!("Returning cached product version");
            return Ok(self.product_version.clone().unwrap());
        }
        // Query the device for the connection type
        let query = RequestKey {
            label: self.connection.label.clone(),
            request: "GetValue".to_string(),
            key: "ProductVersion".to_string(),
        };

        self.connection.write_plist(&query).await?;

        let res: RequestKeyRes = self.connection.read_plist().await?;

        self.product_version = Some(res.value.clone());

        Ok(res.value)
    }

    pub async fn start_session(
        &mut self,
        pairing_file: PairingFile,
        buid: String,
    ) -> Result<(), std::io::Error> {
        let start = StartSession {
            label: self.connection.label.clone(),
            request: "StartSession".to_string(),
            host_id: pairing_file.host_id.clone(),
            system_buid: buid,
        };

        self.connection.write_plist(&start).await?;

        let res: StartSessionRes = self.connection.read_plist().await?;

        if res.enable_session_ssl {
            self.connection.pairing_file = Some(pairing_file);
        }

        Ok(())
    }
}

/// The initial packet sent to the device after connection
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Query {
    label: String,
    request: String,
}

/// The response to the initial packet sent to the device after connection
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct QueryRes {
    type_: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RequestKey {
    label: String,
    key: String,
    request: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct RequestKeyRes {
    key: String,
    request: String,
    value: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct StartSession {
    label: String,
    request: String,
    #[serde(rename = "HostID")]
    host_id: String,
    #[serde(rename = "SystemBUID")]
    system_buid: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct StartSessionRes {
    #[serde(rename = "EnableSessionSSL")]
    enable_session_ssl: bool,
    request: String,
    #[serde(rename = "SessionID")]
    session_id: String,
}
