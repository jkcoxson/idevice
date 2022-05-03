// jkcoxson

use crate::muxer::DeviceProperties;
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub struct Connection {
    unix_stream: Option<UnixStream>,
    tcp_stream: Option<tokio::net::TcpStream>,
    pub properties: DeviceProperties,
    pub service_type: String,
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
                let mut stream = tokio::net::TcpStream::connect(socket_addr).await?;

                // Query the device for the connection type
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

                // Get the size of the packet and append it to the front
                let size = to_send.len() as u32;
                let mut size = size.to_be_bytes().to_vec();
                size.extend_from_slice(&to_send);

                // Send the packet to the device
                stream.write_all(&size).await?;

                // Read the len from the device
                let mut buf = [0; 4];
                let size = stream.read(&mut buf).await?;
                let size_buf = &buf[..size];
                let size = u32::from_be_bytes(size_buf.try_into().unwrap());

                // Read the packet from the device
                let mut buf = vec![0; size as usize];
                let size = stream.read(&mut buf).await?;
                let buf = &buf[..size];

                // Deserialize the response
                #[derive(Deserialize)]
                #[serde(rename_all = "PascalCase")]
                struct Res {
                    type_: String,
                }
                let response: Res = match plist::from_bytes(buf) {
                    Ok(v) => v,
                    Err(e) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Unable to deserialize packet: {}", e),
                        ));
                    }
                };

                return Ok(Connection {
                    unix_stream: None,
                    tcp_stream: Some(stream),
                    properties: properties.clone(),
                    service_type: response.type_,
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
