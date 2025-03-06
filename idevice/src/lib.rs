// Jackson Coxson

#[cfg(feature = "core_device_proxy")]
pub mod core_device_proxy;
#[cfg(feature = "debug_proxy")]
pub mod debug_proxy;
#[cfg(feature = "heartbeat")]
pub mod heartbeat;
#[cfg(feature = "xpc")]
pub mod http2;
#[cfg(feature = "installation_proxy")]
pub mod installation_proxy;
pub mod lockdownd;
#[cfg(feature = "misagent")]
pub mod misagent;
#[cfg(feature = "mounter")]
pub mod mounter;
pub mod pairing_file;
pub mod provider;
#[cfg(feature = "tss")]
pub mod tss;
#[cfg(feature = "tunneld")]
pub mod tunneld;
#[cfg(feature = "usbmuxd")]
pub mod usbmuxd;
mod util;
#[cfg(feature = "xpc")]
pub mod xpc;

use log::{debug, error};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use provider::IdeviceProvider;
use std::io::{self, BufWriter};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub use util::{pretty_print_dictionary, pretty_print_plist};

pub trait ReadWrite: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug> ReadWrite for T {}

pub trait IdeviceService: Sized {
    fn service_name() -> &'static str;
    fn connect(
        provider: &dyn IdeviceProvider,
    ) -> impl std::future::Future<Output = Result<Self, IdeviceError>> + Send;
}

pub type IdeviceSocket = Box<dyn ReadWrite>;

pub struct Idevice {
    socket: Option<Box<dyn ReadWrite>>, // in a box for now to use the ReadWrite trait for further uses
    label: String,
}

impl Idevice {
    pub fn new(socket: Box<dyn ReadWrite>, label: impl Into<String>) -> Self {
        Self {
            socket: Some(socket),
            label: label.into(),
        }
    }

    pub async fn get_type(&mut self) -> Result<String, IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Label".into(), self.label.clone().into());
        req.insert("Request".into(), "QueryType".into());
        let message = plist::to_value(&req)?;
        self.send_plist(message).await?;
        let message: plist::Dictionary = self.read_plist().await?;
        match message.get("Type") {
            Some(m) => Ok(plist::from_value(m)?),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    pub async fn rsd_checkin(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Label".into(), self.label.clone().into());
        req.insert("ProtocolVersion".into(), "2".into());
        req.insert("Request".into(), "RSDCheckin".into());
        self.send_plist(plist::to_value(&req).unwrap()).await?;
        let res = self.read_plist().await?;
        match res.get("Request").and_then(|x| x.as_string()) {
            Some(r) => {
                if r != "RSDCheckin" {
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            None => return Err(IdeviceError::UnexpectedResponse),
        }

        let res = self.read_plist().await?;
        match res.get("Request").and_then(|x| x.as_string()) {
            Some(r) => {
                if r != "StartService" {
                    return Err(IdeviceError::UnexpectedResponse);
                }
            }
            None => return Err(IdeviceError::UnexpectedResponse),
        }

        Ok(())
    }

    /// Sends a plist to the socket
    async fn send_plist(&mut self, message: plist::Value) -> Result<(), IdeviceError> {
        if let Some(socket) = &mut self.socket {
            debug!("Sending plist: {}", pretty_print_plist(&message));

            let buf = Vec::new();
            let mut writer = BufWriter::new(buf);
            message.to_writer_xml(&mut writer)?;
            let message = writer.into_inner().unwrap();
            let message = String::from_utf8(message)?;
            let len = message.len() as u32;
            socket.write_all(&len.to_be_bytes()).await?;
            socket.write_all(message.as_bytes()).await?;
            Ok(())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Sends raw bytes to the socket
    async fn send_raw(&mut self, message: &[u8]) -> Result<(), IdeviceError> {
        self.send_raw_with_progress(message, |_| async {}, ()).await
    }

    async fn send_raw_with_progress<Fut, S>(
        &mut self,
        message: &[u8],
        callback: impl Fn(((usize, usize), S)) -> Fut,
        state: S,
    ) -> Result<(), IdeviceError>
    where
        Fut: std::future::Future<Output = ()>,
        S: Clone,
    {
        if let Some(socket) = &mut self.socket {
            let message_parts = message.chunks(1024 * 64);
            let part_len = message_parts.len() - 1;

            for (i, part) in message_parts.enumerate() {
                debug!("Writing {i}/{part_len}");
                socket.write_all(part).await?;
                callback(((i, part_len), state.clone())).await;
            }
            Ok(())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Reads raw bytes from the socket
    async fn read_raw(&mut self, len: usize) -> Result<Vec<u8>, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let mut buf = vec![0; len];
            socket.read_exact(&mut buf).await?;
            Ok(buf)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Reads bytes from the socket until it doesn't
    async fn read_any(&mut self, max_size: u32) -> Result<Vec<u8>, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let mut buf = vec![0; max_size as usize];
            let len = socket.read(&mut buf).await?;
            Ok(buf[..len].to_vec())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Read a plist from the socket
    async fn read_plist(&mut self) -> Result<plist::Dictionary, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            debug!("Reading response size");
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await?;
            let len = u32::from_be_bytes(buf);
            let mut buf = vec![0; len as usize];
            socket.read_exact(&mut buf).await?;
            let res: plist::Dictionary = plist::from_bytes(&buf)?;
            debug!("Received plist: {}", pretty_print_dictionary(&res));

            if let Some(e) = res.get("Error") {
                let e: String = plist::from_value(e)?;
                if let Some(e) = IdeviceError::from_device_error_type(e.as_str(), &res) {
                    return Err(e);
                } else {
                    return Err(IdeviceError::UnknownErrorType(e));
                }
            }
            Ok(res)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Wraps current connection in TLS
    pub async fn start_session(
        &mut self,
        pairing_file: &pairing_file::PairingFile,
    ) -> Result<(), IdeviceError> {
        let connector = SslConnector::builder(SslMethod::tls()).unwrap();

        let mut connector = connector
            .build()
            .configure()
            .unwrap()
            .into_ssl("ur mom")
            .unwrap();

        connector.set_certificate(&pairing_file.host_certificate)?;
        connector.set_private_key(&pairing_file.host_private_key)?;
        connector.set_verify(SslVerifyMode::empty());

        let socket = self.socket.take().unwrap();

        let mut ssl_stream = tokio_openssl::SslStream::new(connector, socket)?;
        std::pin::Pin::new(&mut ssl_stream).connect().await?;
        self.socket = Some(Box::new(ssl_stream));

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum IdeviceError {
    #[error("device socket io failed")]
    Socket(#[from] io::Error),
    #[error("ssl io failed")]
    Ssl(#[from] openssl::ssl::Error),
    #[error("ssl failed to setup")]
    SslSetup(#[from] openssl::error::ErrorStack),
    #[error("io on plist")]
    Plist(#[from] plist::Error),
    #[error("can't convert bytes to utf8")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("unexpected response from device")]
    UnexpectedResponse,
    #[error("this request was prohibited")]
    GetProhibited,
    #[error("no SSL session is active")]
    SessionInactive,
    #[error("device does not have pairing file")]
    InvalidHostID,
    #[error("no established connection")]
    NoEstablishedConnection,
    #[error("device went to sleep")]
    HeartbeatSleepyTime,
    #[error("heartbeat timeout")]
    HeartbeatTimeout,
    #[error("not found")]
    NotFound,
    #[error("CDTunnel packet too short")]
    CdtunnelPacketTooShort,
    #[error("CDTunnel packet invalid magic")]
    CdtunnelPacketInvalidMagic,
    #[error("Proclaimed packet size does not match actual size")]
    PacketSizeMismatch,

    #[cfg(feature = "core_device_proxy")]
    #[error("JSON serialization failed")]
    Json(#[from] serde_json::Error),

    #[error("device not found")]
    DeviceNotFound,

    #[error("device lockded")]
    DeviceLocked,

    #[error("device refused connection")]
    UsbConnectionRefused,
    #[error("bad command")]
    UsbBadCommand,
    #[error("bad device")]
    UsbBadDevice,
    #[error("usb bad version")]
    UsbBadVersion,

    #[error("bad build manifest")]
    BadBuildManifest,
    #[error("image not mounted")]
    ImageNotMounted,

    #[cfg(any(feature = "tss", feature = "tunneld"))]
    #[error("http reqwest error")]
    Reqwest(#[from] reqwest::Error),

    #[error("internal error")]
    InternalError(String),

    #[cfg(feature = "xpc")]
    #[error("xpc message failed")]
    Xpc(#[from] xpc::error::XPCError),

    #[cfg(feature = "debug_proxy")]
    #[error("invalid argument passed")]
    InvalidArgument,

    #[error("unknown error `{0}` returned from device")]
    UnknownErrorType(String),
}

impl IdeviceError {
    fn from_device_error_type(e: &str, context: &plist::Dictionary) -> Option<Self> {
        match e {
            "GetProhibited" => Some(Self::GetProhibited),
            "InvalidHostID" => Some(Self::InvalidHostID),
            "SessionInactive" => Some(Self::SessionInactive),
            "DeviceLocked" => Some(Self::DeviceLocked),
            "InternalError" => {
                let detailed_error = context
                    .get("DetailedError")
                    .and_then(|d| d.as_string())
                    .unwrap_or("No context")
                    .to_string();

                if detailed_error.contains("There is no matching entry in the device map for") {
                    Some(Self::ImageNotMounted)
                } else {
                    Some(Self::InternalError(detailed_error))
                }
            }
            _ => None,
        }
    }
}
