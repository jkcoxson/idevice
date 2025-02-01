// Jackson Coxson

use std::{
    future::Future,
    net::{IpAddr, SocketAddr},
    pin::Pin,
};

#[cfg(feature = "tcp")]
use tokio::net::TcpStream;

use crate::{pairing_file::PairingFile, Idevice, IdeviceError};

#[cfg(feature = "usbmuxd")]
use crate::usbmuxd::UsbmuxdAddr;

/// A provider for connecting to the iOS device
/// This is an ugly trait until async traits are stabilized
pub trait IdeviceProvider: Unpin + Send + Sync + std::fmt::Debug {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>>;

    fn label(&self) -> &str;

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>>;
}

#[cfg(feature = "tcp")]
#[derive(Debug)]
pub struct TcpProvider {
    pub addr: IpAddr,
    pub pairing_file: PairingFile,
    pub label: String,
}

#[cfg(feature = "tcp")]
impl IdeviceProvider for TcpProvider {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        let addr = self.addr;
        let label = self.label.clone();
        Box::pin(async move {
            let socket_addr = SocketAddr::new(addr, port);
            let stream = TcpStream::connect(socket_addr).await?;
            Ok(Idevice::new(Box::new(stream), label))
        })
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        let pairing_file = self.pairing_file.clone();
        Box::pin(async move { Ok(pairing_file) })
    }
}

#[cfg(feature = "usbmuxd")]
#[derive(Debug)]
pub struct UsbmuxdProvider {
    pub addr: UsbmuxdAddr,
    pub tag: u32,
    pub udid: String,
    pub device_id: u32,
    pub label: String,
}

#[cfg(feature = "usbmuxd")]
impl IdeviceProvider for UsbmuxdProvider {
    fn connect(
        &self,
        port: u16,
    ) -> Pin<Box<dyn Future<Output = Result<Idevice, IdeviceError>> + Send>> {
        let addr = self.addr.clone();
        let tag = self.tag;
        let device_id = self.device_id;
        let label = self.label.clone();

        Box::pin(async move {
            let usbmuxd = addr.connect(tag).await?;
            usbmuxd.connect_to_device(device_id, port, &label).await
        })
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn get_pairing_file(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<PairingFile, IdeviceError>> + Send>> {
        let addr = self.addr.clone();
        let tag = self.tag;
        let udid = self.udid.clone();

        Box::pin(async move {
            let mut usbmuxd = addr.connect(tag).await?;
            usbmuxd.get_pair_record(&udid).await
        })
    }
}
