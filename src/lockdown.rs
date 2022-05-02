// jkcoxson

use serde::Serialize;

use crate::{connection::Connection, muxer::DeviceProperties};

const LOCKDOWND_PORT: u16 = 62078;

pub struct LockdowndClient {
    //
}

impl LockdowndClient {
    pub async fn new(
        properties: &DeviceProperties,
        label: impl Into<String>,
    ) -> Result<LockdowndClient, std::io::Error> {
        let mut connection = Connection::new(properties, LOCKDOWND_PORT).await?;

        #[derive(Serialize)]
        #[serde(rename_all = "PascalCase")]
        struct Query {
            label: String,
            request: String,
        }

        let query = Query {
            label: label.into(),
            request: "QueryType".to_string(),
        };

        // Serialize the query to a plist
        let mut to_send = Vec::new();
        let _ = match plist::to_writer_xml(&mut to_send, &query) {
            Ok(_) => (),
            Err(e) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unable to serialize packet: {}", e),
                ));
            }
        };

        // Append the packet size to the beginning of the packet
        let size = (4 + to_send.len() as u32).to_le_bytes();
        let mut buf = Vec::new();
        buf.extend_from_slice(&size);
        buf.extend_from_slice(&to_send);

        // Send the packet to the device
        connection.write(&buf).await?;

        // Read the response from the device
        let res = connection.read().await?;
        println!("{:?}", res);

        todo!()
    }
}
