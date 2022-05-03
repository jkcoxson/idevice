// jkcoxson

use crate::{connection::Connection, muxer::DeviceProperties};

const LOCKDOWND_PORT: u16 = 62078;

pub struct LockdowndClient {
    connection: Connection,
}

impl LockdowndClient {
    pub async fn new(
        properties: &DeviceProperties,
        label: impl Into<String>,
    ) -> Result<LockdowndClient, std::io::Error> {
        let connection = Connection::new(properties, LOCKDOWND_PORT, label).await?;

        if connection.service_type == "com.apple.mobile.lockdown" {
            Ok(LockdowndClient { connection })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unexpected service type",
            ))
        }
    }
}
