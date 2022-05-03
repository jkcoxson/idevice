// jkcoxson

use crate::muxer::DeviceProperties;
use log::info;
use serde::{de::DeserializeOwned, Serialize};
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
}

impl Connection {
    /// Creates a new connection to the device
    /// # Arguments
    /// * `properties` - The properties of the device
    /// * `label` - The label to give the connection internally
    /// * `port` - The port to connect to
    /// # Returns
    /// A new connection to the device
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

                info!("Connecting to {}", socket_addr);

                // Create a new TcpStream to the device
                let stream = tokio::net::TcpStream::connect(socket_addr).await?;

                return Ok(Connection {
                    unix_stream: None,
                    tcp_stream: Some(stream),
                    properties: properties.clone(),
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

    /// Reads a packet from the device
    /// # Returns
    /// A vector of bytes representing the packet
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

    /// Writes a packet to the connection
    /// # Arguments
    /// * `packet` - The packet to write in bytes
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

    /// Writes a plist to the connection
    /// # Arguments
    /// * `plist` - The plist to write. Must have the `Serialize` trait
    pub(crate) async fn write_plist(
        &mut self,
        plist: &impl Serialize,
    ) -> Result<(), std::io::Error> {
        let to_send = plist_to_binary(plist)?;
        self.write(&to_send).await?;
        Ok(())
    }

    /// Reads a plist from the connection
    /// # Returns
    /// The plist read from the connection
    pub(crate) async fn read_plist<T: DeserializeOwned>(&mut self) -> Result<T, std::io::Error> {
        let bytes = self.read().await?;
        let plist: T = binary_to_plist(&bytes)?;
        return Ok(plist);
    }
}

/// Converts a plist to binary form in XML
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

/// Converts a binary plist in XML to a plist
/// # Arguments
/// * `bytes` - The bytes to convert
/// # Returns
/// The plist read from the bytes
///
/// # Example
/// ```
/// #[derive(Deserialize)]
/// #[serde(rename_all = "PascalCase")]
/// struct Packet {
///     foo: String,
/// }
/// let packet: Packet = binary_to_plist(&bytes).unwrap();
/// ```
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
