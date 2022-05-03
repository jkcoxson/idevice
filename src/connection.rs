// jkcoxson

use crate::muxer::DeviceProperties;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub struct Connection {
    // Streams for writing and reading
    unix_stream: Option<UnixStream>,
    tcp_stream: Option<tokio::net::TcpStream>,

    // Public properties
    pub properties: DeviceProperties,
    pub label: String,

    // Private property caches
    service_type: Option<String>,
    product_version: Option<String>,
}

impl Connection {
    pub async fn new(
        properties: &DeviceProperties,
        port: u16,
        label: impl Into<String>,
    ) -> Result<Connection, std::io::Error> {
        match properties.connection_type.as_str() {
            "Network" => {
                let ip = properties.get_ip().unwrap();
                let ip = ip.parse::<std::net::IpAddr>().unwrap();
                let socket_addr = std::net::SocketAddr::new(ip, port);

                let socket_addr = match socket_addr {
                    std::net::SocketAddr::V4(_) => socket_addr,
                    std::net::SocketAddr::V6(mut addr) => {
                        addr.set_scope_id(14); // This is not always 14, somebody who knows IPv6 better should fix this
                        addr.into()
                    }
                };

                // Create a new TcpStream to the device
                let stream = tokio::net::TcpStream::connect(socket_addr).await?;

                return Ok(Connection {
                    unix_stream: None,
                    tcp_stream: Some(stream),
                    properties: properties.clone(),
                    service_type: None,
                    product_version: None,
                    label: label.into(),
                });
            }
            "USB" => {
                todo!()
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Unknown connection type",
            )),
        }
    }

    pub async fn get_service_type(&mut self) -> Result<String, std::io::Error> {
        if self.service_type.is_some() {
            return Ok(self.service_type.clone().unwrap());
        }
        // Query the device for the connection type
        let query = Query {
            label: self.label.clone(),
            request: "QueryType".to_string(),
        };

        self.write_plist(&query).await?;

        let res: QueryRes = self.read_plist().await?;

        self.service_type = Some(res.type_.clone());

        Ok(res.type_)
    }

    pub(crate) async fn read(&mut self) -> Result<Vec<u8>, std::io::Error> {
        match self.unix_stream {
            Some(ref mut unix_stream) => {
                let mut buf = [0; 4];
                unix_stream.read_exact(&mut buf).await?;
                let len = u32::from_be_bytes(buf);

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
                let len = u32::from_be_bytes(buf);

                let mut buf = vec![0; len as usize];
                let size = self.tcp_stream.as_mut().unwrap().read(&mut buf).await?;
                return Ok(buf[..size].to_vec());
            }
        };
    }

    pub(crate) async fn write_plist(
        &mut self,
        plist: &impl Serialize,
    ) -> Result<(), std::io::Error> {
        let to_send = plist_to_binary(plist)?;
        self.write(&to_send).await?;
        Ok(())
    }

    pub(crate) async fn read_plist<T: DeserializeOwned>(&mut self) -> Result<T, std::io::Error> {
        let bytes = self.read().await?;
        let plist: T = binary_to_plist(&bytes)?;
        return Ok(plist);
    }

    pub(crate) async fn write(&mut self, data: &[u8]) -> Result<(), std::io::Error> {
        match self.unix_stream {
            Some(ref mut unix_stream) => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
                buf.extend_from_slice(data);
                unix_stream.write_all(&buf).await?;
                return Ok(());
            }
            None => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&(data.len() as u32).to_be_bytes());
                buf.extend_from_slice(data);
                self.tcp_stream.as_mut().unwrap().write_all(&buf).await?;
                return Ok(());
            }
        };
    }
}

pub(crate) fn plist_to_binary(plist: impl Serialize) -> Result<Vec<u8>, std::io::Error> {
    let mut buf = Vec::new();
    let _ = match plist::to_writer_xml(&mut buf, &plist) {
        Ok(_) => (),
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to serialize packet: {}", e),
            ));
        }
    };
    Ok(buf)
}

pub(crate) fn binary_to_plist<'a, T: DeserializeOwned>(data: &[u8]) -> Result<T, std::io::Error> {
    let response: T = match plist::from_bytes(data) {
        Ok(v) => v,
        Err(e) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to deserialize packet: {}", e),
            ));
        }
    };

    Ok(response)
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
