// Jackson Coxson

use std::net::{IpAddr, SocketAddr};

use tokio::net::TcpStream;

use crate::{usbmuxd::UsbmuxdAddr, Idevice, IdeviceError};

pub trait IdeviceProvider {
    // https://blog.rust-lang.org/2023/12/21/async-fn-rpit-in-traits.html#is-it-okay-to-use-async-fn-in-traits-what-are-the-limitations
    fn connect(
        &self,
        port: u16,
    ) -> impl std::future::Future<Output = Result<Idevice, IdeviceError>> + Send;
    fn label(&self) -> &str;
}

pub struct TcpProvider {
    addr: IpAddr,
    label: String,
}

impl IdeviceProvider for TcpProvider {
    async fn connect(&self, port: u16) -> Result<Idevice, IdeviceError> {
        let socket_addr = SocketAddr::new(self.addr, port);
        let stream = TcpStream::connect(socket_addr).await?;
        Ok(Idevice::new(Box::new(stream), self.label.to_owned()))
    }
    fn label(&self) -> &str {
        self.label.as_str()
    }
}

#[cfg(feature = "usbmuxd")]
pub struct UsbmuxdProvider {
    addr: UsbmuxdAddr,
    tag: u32,
    device_id: u32,
    label: String,
}

#[cfg(feature = "usbmuxd")]
impl IdeviceProvider for UsbmuxdProvider {
    async fn connect(&self, port: u16) -> Result<Idevice, IdeviceError> {
        let usbmuxd = self.addr.connect(self.tag).await?;
        usbmuxd
            .connect_to_device(self.device_id, port, &self.label)
            .await
    }

    fn label(&self) -> &str {
        self.label.as_str()
    }
}
