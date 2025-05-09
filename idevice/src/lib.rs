#![doc = include_str!("../README.md")]
// Jackson Coxson

#[cfg(feature = "afc")]
pub mod afc;
#[cfg(feature = "amfi")]
pub mod amfi;
#[cfg(feature = "pair")]
mod ca;
#[cfg(feature = "core_device_proxy")]
pub mod core_device_proxy;
#[cfg(feature = "crashreportcopymobile")]
pub mod crashreportcopymobile;
#[cfg(feature = "debug_proxy")]
pub mod debug_proxy;
#[cfg(feature = "dvt")]
pub mod dvt;
#[cfg(feature = "heartbeat")]
pub mod heartbeat;
#[cfg(feature = "xpc")]
mod http2;
#[cfg(feature = "installation_proxy")]
pub mod installation_proxy;
pub mod lockdown;
#[cfg(feature = "misagent")]
pub mod misagent;
#[cfg(feature = "mobile_image_mounter")]
pub mod mobile_image_mounter;
pub mod pairing_file;
pub mod provider;
mod sni;
#[cfg(feature = "springboardservices")]
pub mod springboardservices;
#[cfg(feature = "tunnel_tcp_stack")]
pub mod tcp;
#[cfg(feature = "tss")]
pub mod tss;
#[cfg(feature = "tunneld")]
pub mod tunneld;
#[cfg(feature = "usbmuxd")]
pub mod usbmuxd;
mod util;
#[cfg(feature = "xpc")]
pub mod xpc;

use log::{debug, error, trace};
use provider::IdeviceProvider;
use rustls::{crypto::CryptoProvider, pki_types::ServerName};
use std::{
    io::{self, BufWriter},
    sync::Arc,
};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub use util::{pretty_print_dictionary, pretty_print_plist};

/// A trait combining all required characteristics for a device communication socket
///
/// This serves as a convenience trait for any type that can be used as an asynchronous
/// read/write socket for device communication. Combines common async I/O traits with
/// thread safety and debugging requirements.
///
/// Tokio's TcpStream and UnixStream implement this trait.
pub trait ReadWrite: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug {}

// Blanket implementation for any compatible type
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync + std::fmt::Debug> ReadWrite for T {}

/// Interface for services that can be connected to on an iOS device
///
/// Implement this trait to define new services that can be accessed through the
/// device connection protocol.
pub trait IdeviceService: Sized {
    /// Returns the service name as advertised by the device
    fn service_name() -> &'static str;

    /// Establishes a connection to this service
    ///
    /// # Arguments
    /// * `provider` - The device provider that can supply connections
    fn connect(
        provider: &dyn IdeviceProvider,
    ) -> impl std::future::Future<Output = Result<Self, IdeviceError>> + Send;
}

/// Type alias for boxed device connection sockets
///
/// Used to enable dynamic dispatch of different connection types while maintaining
/// the required ReadWrite characteristics.
pub type IdeviceSocket = Box<dyn ReadWrite>;

/// Main handle for communicating with an iOS device
///
/// Manages the connection socket and provides methods for common device operations
/// and message exchange.
pub struct Idevice {
    /// The underlying connection socket, boxed for dynamic dispatch
    socket: Option<Box<dyn ReadWrite>>,
    /// Unique label identifying this connection
    label: String,
}

impl Idevice {
    /// Creates a new device connection handle
    ///
    /// # Arguments
    /// * `socket` - The established connection socket
    /// * `label` - Unique identifier for this connection
    pub fn new(socket: Box<dyn ReadWrite>, label: impl Into<String>) -> Self {
        Self {
            socket: Some(socket),
            label: label.into(),
        }
    }

    /// Queries the device type
    ///
    /// Sends a QueryType request and parses the response
    ///
    /// # Returns
    /// The device type string on success
    ///
    /// # Errors
    /// Returns `IdeviceError` if communication fails or response is invalid
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

    /// Performs RSD (Remote Service Discovery) check-in procedure
    ///
    /// Establishes the basic service connection protocol
    ///
    /// # Errors
    /// Returns `IdeviceError` if the protocol sequence isn't followed correctly
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

    /// Sends a plist-formatted message to the device
    ///
    /// # Arguments
    /// * `message` - The plist value to send
    ///
    /// # Errors
    /// Returns `IdeviceError` if serialization or transmission fails
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
            socket.flush().await?;
            Ok(())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Sends raw binary data to the device
    ///
    /// # Arguments
    /// * `message` - The bytes to send
    ///
    /// # Errors
    /// Returns `IdeviceError` if transmission fails
    async fn send_raw(&mut self, message: &[u8]) -> Result<(), IdeviceError> {
        self.send_raw_with_progress(message, |_| async {}, ()).await
    }

    /// Sends raw binary data with progress callbacks
    ///
    /// # Arguments
    /// * `message` - The bytes to send
    /// * `callback` - Progress callback invoked after each chunk
    /// * `state` - Arbitrary state passed to callback
    ///
    /// # Type Parameters
    /// * `Fut` - Future type returned by callback
    /// * `S` - Type of state passed to callback
    ///
    /// # Errors
    /// Returns `IdeviceError` if transmission fails
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
                trace!("Writing {i}/{part_len}");
                socket.write_all(part).await?;
                callback(((i, part_len), state.clone())).await;
            }
            socket.flush().await?;
            Ok(())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Reads exactly `len` bytes from the device
    ///
    /// # Arguments
    /// * `len` - Exact number of bytes to read
    ///
    /// # Returns
    /// The received bytes
    ///
    /// # Errors
    /// Returns `IdeviceError` if reading fails or connection is closed prematurely
    async fn read_raw(&mut self, len: usize) -> Result<Vec<u8>, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let mut buf = vec![0; len];
            socket.read_exact(&mut buf).await?;
            Ok(buf)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Reads up to `max_size` bytes from the device
    ///
    /// # Arguments
    /// * `max_size` - Maximum number of bytes to read
    ///
    /// # Returns
    /// The received bytes (may be shorter than max_size)
    ///
    /// # Errors
    /// Returns `IdeviceError` if reading fails
    async fn read_any(&mut self, max_size: u32) -> Result<Vec<u8>, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let mut buf = vec![0; max_size as usize];
            let len = socket.read(&mut buf).await?;
            Ok(buf[..len].to_vec())
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Reads a plist-formatted message from the device
    ///
    /// # Returns
    /// The parsed plist dictionary
    ///
    /// # Errors
    /// Returns `IdeviceError` if reading, parsing fails, or device reports an error
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

    /// Upgrades the connection to TLS using device pairing credentials
    ///
    /// # Arguments
    /// * `pairing_file` - Contains the device's identity and certificates
    ///
    /// # Errors
    /// Returns `IdeviceError` if TLS handshake fails or credentials are invalid
    pub async fn start_session(
        &mut self,
        pairing_file: &pairing_file::PairingFile,
    ) -> Result<(), IdeviceError> {
        if CryptoProvider::get_default().is_none() {
            if let Err(e) =
                CryptoProvider::install_default(rustls::crypto::aws_lc_rs::default_provider())
            {
                // For whatever reason, getting the default provider will return None on iOS at
                // random. Installing the default provider a second time will return an error, so
                // we will log it but not propogate it. An issue should be opened with rustls.
                log::error!("Failed to set crypto provider: {e:?}");
            }
        }
        let config = sni::create_client_config(pairing_file)?;
        let connector = tokio_rustls::TlsConnector::from(Arc::new(config));

        let socket = self.socket.take().unwrap();
        let socket = connector
            .connect(ServerName::try_from("Device").unwrap(), socket)
            .await?;

        self.socket = Some(Box::new(socket));

        Ok(())
    }
}

/// Comprehensive error type for all device communication failures
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum IdeviceError {
    #[error("device socket io failed")]
    Socket(#[from] io::Error),
    #[error("PEM parse failed")]
    PemParseFailed(#[from] rustls::pki_types::pem::Error),
    #[error("TLS error")]
    Rustls(#[from] rustls::Error),
    #[error("TLS verifiction build failed")]
    TlsBuilderFailed(#[from] rustls::server::VerifierBuilderError),
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

    #[cfg(feature = "pair")]
    #[error("pairing trust dialog pending")]
    PairingDialogResponsePending,

    #[cfg(feature = "pair")]
    #[error("user denied pairing trust")]
    UserDeniedPairing,

    #[cfg(feature = "pair")]
    #[error("device is locked")]
    PasswordProtected,

    #[cfg(feature = "misagent")]
    #[error("misagent operation failed")]
    MisagentFailure,

    #[cfg(feature = "installation_proxy")]
    #[error("installation proxy operation failed")]
    InstallationProxyOperationFailed(String),

    #[cfg(feature = "afc")]
    #[error("afc error")]
    Afc(#[from] afc::errors::AfcError),

    #[cfg(feature = "afc")]
    #[error("unknown afc opcode")]
    UnknownAfcOpcode,

    #[cfg(feature = "afc")]
    #[error("invalid afc magic")]
    InvalidAfcMagic,

    #[cfg(feature = "afc")]
    #[error("missing file attribute")]
    AfcMissingAttribute,

    #[cfg(feature = "crashreportcopymobile")]
    #[error("crash report mover sent the wrong response")]
    CrashReportMoverBadResponse(Vec<u8>),

    #[cfg(any(feature = "tss", feature = "tunneld"))]
    #[error("http reqwest error")]
    Reqwest(#[from] reqwest::Error),

    #[error("internal error")]
    InternalError(String),

    #[cfg(feature = "xpc")]
    #[error("xpc message failed")]
    Xpc(#[from] xpc::error::XPCError),

    #[cfg(feature = "dvt")]
    #[error("NSKeyedArchive error")]
    NsKeyedArchiveError(#[from] ns_keyed_archive::ConverterError),

    #[cfg(feature = "dvt")]
    #[error("Unknown aux value type")]
    UnknownAuxValueType(u32),

    #[cfg(feature = "dvt")]
    #[error("unknown channel")]
    UnknownChannel(u32),

    #[error("cannot parse string as IpAddr")]
    AddrParseError(#[from] std::net::AddrParseError),

    #[cfg(feature = "dvt")]
    #[error("disable memory limit failed")]
    DisableMemoryLimitFailed,

    #[error("not enough bytes, expected {1}, got {0}")]
    NotEnoughBytes(usize, usize),

    #[error("failed to parse bytes as valid utf8")]
    Utf8Error,

    #[cfg(feature = "debug_proxy")]
    #[error("invalid argument passed")]
    InvalidArgument,

    #[error("unknown error `{0}` returned from device")]
    UnknownErrorType(String),
}

impl IdeviceError {
    /// Converts a device-reported error string to a typed error
    ///
    /// # Arguments
    /// * `e` - The error string from device
    /// * `context` - Full plist context containing additional error details
    ///
    /// # Returns
    /// Some(IdeviceError) if the string maps to a known error type, None otherwise
    fn from_device_error_type(e: &str, context: &plist::Dictionary) -> Option<Self> {
        match e {
            "GetProhibited" => Some(Self::GetProhibited),
            "InvalidHostID" => Some(Self::InvalidHostID),
            "SessionInactive" => Some(Self::SessionInactive),
            "DeviceLocked" => Some(Self::DeviceLocked),
            #[cfg(feature = "pair")]
            "PairingDialogResponsePending" => Some(Self::PairingDialogResponsePending),
            #[cfg(feature = "pair")]
            "UserDeniedPairing" => Some(Self::UserDeniedPairing),
            #[cfg(feature = "pair")]
            "PasswordProtected" => Some(Self::PasswordProtected),
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
