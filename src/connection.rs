// jkcoxson

use crate::muxer::DeviceProperties;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
};

pub struct Connection {
    unix_stream: Option<UnixStream>,
    tcp_stream: Option<tokio::net::TcpStream>,
    properties: DeviceProperties,
}

impl Connection {
    pub async fn new(
        properties: &DeviceProperties,
        port: u16,
    ) -> Result<Connection, std::io::Error> {
        match properties.connection_type.as_str() {
            "Network" => {
                let ip = properties.get_ip().unwrap();
                let ip = ip.parse::<std::net::IpAddr>().unwrap();

                println!("{:?}", ip);

                // Create a new TcpStream to the device (idk about usb devices rn)
                let stream = tokio::net::TcpStream::connect((ip, port)).await?;

                Ok(Connection {
                    unix_stream: None,
                    tcp_stream: Some(stream),
                    properties: properties.clone(),
                })
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
                let len = u32::from_le_bytes(buf);

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
                let len = u32::from_le_bytes(buf);

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
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
                unix_stream.write_all(&buf).await?;
                unix_stream.flush().await?;
                return Ok(());
            }
            None => {
                let mut buf = Vec::new();
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(data);
                self.tcp_stream.as_mut().unwrap().write_all(&buf).await?;
                self.tcp_stream.as_mut().unwrap().flush().await?;
                return Ok(());
            }
        };
    }
}
