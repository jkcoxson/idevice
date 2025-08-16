// Jackson Coxson

use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use log::trace;
use tokio::io::AsyncWriteExt;

use crate::{ReadWrite, provider::RsdProvider};

pub mod adapter;
pub mod handle;
pub mod packets;
pub mod stream;

pub(crate) fn log_packet(file: &Arc<tokio::sync::Mutex<tokio::fs::File>>, packet: &[u8]) {
    trace!("Logging {} byte packet", packet.len());
    let packet = packet.to_vec();
    let file = file.to_owned();
    let now = SystemTime::now();
    tokio::task::spawn(async move {
        let mut file = file.lock().await;
        file.write_all(&(now.duration_since(UNIX_EPOCH).unwrap().as_secs() as u32).to_le_bytes())
            .await
            .unwrap();
        let micros = now.duration_since(UNIX_EPOCH).unwrap().as_micros() % 1_000_000_000;
        file.write_all(&(micros as u32).to_le_bytes())
            .await
            .unwrap();
        file.write_all(&(packet.len() as u32).to_le_bytes())
            .await
            .unwrap();
        file.write_all(&(packet.len() as u32).to_le_bytes())
            .await
            .unwrap();
        file.write_all(&packet).await.unwrap();
    });
}

impl RsdProvider for handle::AdapterHandle {
    async fn connect_to_service_port(
        &mut self,
        port: u16,
    ) -> Result<Box<dyn ReadWrite>, crate::IdeviceError> {
        let s = self.connect(port).await?;
        Ok(Box::new(s))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{IpAddr, Ipv6Addr},
        str::FromStr,
    };

    use super::*;

    use adapter::Adapter;
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };
    use stream::AdapterStream;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tun_rs::DeviceBuilder;

    use bytes::BytesMut;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tun_rs::AsyncDevice;

    pub struct AsyncDeviceWrapper {
        device: AsyncDevice,
        // Buffer to store unread data
        buffer: BytesMut,
    }

    impl std::fmt::Debug for AsyncDeviceWrapper {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AsyncDeviceWrapper")
                .field("buffer", &self.buffer)
                .finish()
        }
    }

    impl AsyncRead for AsyncDeviceWrapper {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            // First, check if we have data in our buffer
            let this = self.get_mut();

            if !this.buffer.is_empty() {
                // We have buffered data, copy as much as possible to the output buffer
                let bytes_to_copy = std::cmp::min(this.buffer.len(), buf.remaining());
                let data_to_copy = this.buffer.split_to(bytes_to_copy);
                buf.put_slice(&data_to_copy);

                return Poll::Ready(Ok(()));
            }

            // If our buffer is empty, try to read more data
            let mut temp_buf = vec![0u8; 4096]; // Temporary buffer with reasonable size

            match this.device.poll_recv(cx, &mut temp_buf) {
                Poll::Ready(Ok(n)) => {
                    if n > 0 {
                        // Got some data, first fill the output buffer
                        let bytes_to_copy = std::cmp::min(n, buf.remaining());
                        buf.put_slice(&temp_buf[..bytes_to_copy]);

                        // If we have more data than fits in the output buffer, store in our internal buffer
                        if n > bytes_to_copy {
                            this.buffer.extend_from_slice(&temp_buf[bytes_to_copy..n]);
                        }

                        Poll::Ready(Ok(()))
                    } else {
                        // Zero bytes read
                        Poll::Ready(Ok(()))
                    }
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            }
        }
    }

    impl AsyncWrite for AsyncDeviceWrapper {
        fn poll_write(
            self: std::pin::Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
            buf: &[u8],
        ) -> std::task::Poll<Result<usize, std::io::Error>> {
            self.device.poll_send(cx, buf)
        }

        fn poll_flush(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), std::io::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: std::pin::Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Result<(), std::io::Error>> {
            std::task::Poll::Ready(Ok(()))
        }
    }

    const SERVER_PORT: u16 = 5555;

    #[tokio::test]
    async fn local_tcp() {
        env_logger::init();

        let our_ip = Ipv6Addr::from_str("fd12:3456:789a::1").unwrap();
        let their_ip = Ipv6Addr::from_str("fd12:3456:789a::2").unwrap();
        let dev = DeviceBuilder::new()
            .ipv6(their_ip, "ffff:ffff:ffff:ffff::")
            .mtu(1420)
            .build_async()
            .expect("Failed to create tunnel. Are you root?");

        println!("Created tunnel [{:?}] {}", dev.name(), their_ip);

        let mut adapter = Adapter::new(
            Box::new(AsyncDeviceWrapper {
                device: dev,
                buffer: BytesMut::new(),
            }),
            IpAddr::V6(our_ip),
            IpAddr::V6(their_ip),
        );
        adapter.pcap("./local_tcp.pcap").await.expect("no pcap");

        tokio::task::spawn(async move {
            let listener = tokio::net::TcpListener::bind(format!("[::0]:{SERVER_PORT}"))
                .await
                .unwrap();
            while let Ok((mut stream, addr)) = listener.accept().await {
                println!("Accepted connection from {addr:?}");

                tokio::task::spawn(async move {
                    loop {
                        let mut buf = [0; 1024];
                        let read_len = stream.read(&mut buf).await.unwrap();
                        stream.write_all(&buf[..read_len]).await.unwrap();
                    }
                });
            }
        });

        println!("Attach Wireshark, press enter to continue\n");
        let mut buf = Vec::new();
        let _ = tokio::io::stdin().read(&mut buf).await.unwrap();

        let mut stream = match AdapterStream::connect(&mut adapter, SERVER_PORT).await {
            Ok(s) => s,
            Err(e) => {
                println!("no connect: {e:?}");
                return;
            }
        };

        if let Err(e) = stream.write_all(&[1, 2, 3, 4, 5]).await {
            println!("no send: {e:?}");
        } else {
            let mut buf = [0u8; 4];
            match stream.read_exact(&mut buf).await {
                Ok(_) => println!("recv'd {buf:?}"),
                Err(e) => println!("no recv: {e:?}"),
            }
        }

        if let Err(e) = stream.write_all(&[69, 69, 42, 0, 1]).await {
            println!("no send: {e:?}");
        } else {
            let mut buf = [0u8; 6];
            match stream.read_exact(&mut buf).await {
                Ok(_) => println!("recv'd {buf:?}"),
                Err(e) => println!("no recv: {e:?}"),
            }
        }

        if let Err(e) = stream.close().await {
            println!("no close: {e:?}");
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        println!("\n\npress enter");
        let mut buf = Vec::new();
        let _ = tokio::io::stdin().read(&mut buf).await.unwrap();
    }
}
