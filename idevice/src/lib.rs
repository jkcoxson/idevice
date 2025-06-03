#![doc = include_str!("../README.md")]
// Jackson Coxson

#[cfg(feature = "pair")]
mod ca;
pub mod pairing_file;
pub mod provider;
mod sni;
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

pub mod services;
pub use services::*;

#[cfg(feature = "xpc")]
pub use xpc::RemoteXpcClient;

use log::{debug, error, trace};
use provider::{IdeviceProvider, RsdProvider};
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

pub trait RsdService: Sized {
    fn rsd_service_name() -> &'static str;
    fn from_stream(
        stream: Self::Stream,
    ) -> impl std::future::Future<Output = Result<Self, IdeviceError>> + Send;
    fn connect_rsd<'a, S>(
        provider: &'a mut impl RsdProvider<'a, Stream = S>,
        handshake: &mut rsd::RsdHandshake,
    ) -> impl std::future::Future<Output = Result<Self, IdeviceError>>
    where
        Self: crate::RsdService<Stream = S>,
        S: ReadWrite,
    {
        handshake.connect(provider)
    }

    type Stream: ReadWrite;
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

    /// Sends a binary plist-formatted message to the device
    ///
    /// # Arguments
    /// * `message` - The plist value to send
    ///
    /// # Errors
    /// Returns `IdeviceError` if serialization or transmission fails
    async fn send_bplist(&mut self, message: plist::Value) -> Result<(), IdeviceError> {
        if let Some(socket) = &mut self.socket {
            debug!("Sending plist: {}", pretty_print_plist(&message));

            let buf = Vec::new();
            let mut writer = BufWriter::new(buf);
            message.to_writer_binary(&mut writer)?;
            let message = writer.into_inner().unwrap();
            let len = message.len() as u32;
            socket.write_all(&len.to_be_bytes()).await?;
            socket.write_all(&message).await?;
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
    pub async fn send_raw(&mut self, message: &[u8]) -> Result<(), IdeviceError> {
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
    pub async fn send_raw_with_progress<Fut, S>(
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
    pub async fn read_raw(&mut self, len: usize) -> Result<Vec<u8>, IdeviceError> {
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
    pub async fn read_any(&mut self, max_size: u32) -> Result<Vec<u8>, IdeviceError> {
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

    #[cfg(feature = "syslog_relay")]
    async fn read_until_delim(
        &mut self,
        delimiter: &[u8],
    ) -> Result<Option<bytes::BytesMut>, IdeviceError> {
        if let Some(socket) = &mut self.socket {
            let mut buffer = bytes::BytesMut::with_capacity(1024);
            let mut temp = [0u8; 1024];

            loop {
                let n = socket.read(&mut temp).await?;
                if n == 0 {
                    if buffer.is_empty() {
                        return Ok(None); // EOF and no data
                    } else {
                        return Ok(Some(buffer)); // EOF but return partial data
                    }
                }

                buffer.extend_from_slice(&temp[..n]);

                if let Some(pos) = buffer.windows(delimiter.len()).position(|w| w == delimiter) {
                    let mut line = buffer.split_to(pos + delimiter.len());
                    line.truncate(line.len() - delimiter.len()); // remove delimiter
                    return Ok(Some(line));
                }
            }
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
#[repr(i32)]
#[non_exhaustive]
pub enum IdeviceError {
    #[error("device socket io failed")]
    Socket(#[from] io::Error) = -1,
    #[error("PEM parse failed")]
    PemParseFailed(#[from] rustls::pki_types::pem::Error) = -2,
    #[error("TLS error")]
    Rustls(#[from] rustls::Error) = -3,
    #[error("TLS verifiction build failed")]
    TlsBuilderFailed(#[from] rustls::server::VerifierBuilderError) = -4,
    #[error("io on plist")]
    Plist(#[from] plist::Error) = -5,
    #[error("can't convert bytes to utf8")]
    Utf8(#[from] std::string::FromUtf8Error) = -6,
    #[error("unexpected response from device")]
    UnexpectedResponse = -7,
    #[error("this request was prohibited")]
    GetProhibited = -8,
    #[error("no SSL session is active")]
    SessionInactive = -9,
    #[error("device does not have pairing file")]
    InvalidHostID = -10,
    #[error("no established connection")]
    NoEstablishedConnection = -11,
    #[error("device went to sleep")]
    HeartbeatSleepyTime = -12,
    #[error("heartbeat timeout")]
    HeartbeatTimeout = -13,
    #[error("not found")]
    NotFound = -14,
    #[error("service not found")]
    ServiceNotFound = -15,
    #[error("CDTunnel packet too short")]
    CdtunnelPacketTooShort = -16,
    #[error("CDTunnel packet invalid magic")]
    CdtunnelPacketInvalidMagic = -17,
    #[error("Proclaimed packet size does not match actual size")]
    PacketSizeMismatch = -18,

    #[cfg(feature = "core_device_proxy")]
    #[error("JSON serialization failed")]
    Json(#[from] serde_json::Error) = -19,

    #[error("device not found")]
    DeviceNotFound = -20,

    #[error("device lockded")]
    DeviceLocked = -21,

    #[error("device refused connection")]
    UsbConnectionRefused = -22,
    #[error("bad command")]
    UsbBadCommand = -23,
    #[error("bad device")]
    UsbBadDevice = -24,
    #[error("usb bad version")]
    UsbBadVersion = -25,

    #[error("bad build manifest")]
    BadBuildManifest = -26,
    #[error("image not mounted")]
    ImageNotMounted = -27,

    #[cfg(feature = "pair")]
    #[error("pairing trust dialog pending")]
    PairingDialogResponsePending = -28,

    #[cfg(feature = "pair")]
    #[error("user denied pairing trust")]
    UserDeniedPairing = -29,

    #[cfg(feature = "pair")]
    #[error("device is locked")]
    PasswordProtected = -30,

    #[cfg(feature = "misagent")]
    #[error("misagent operation failed")]
    MisagentFailure = -31,

    #[cfg(feature = "installation_proxy")]
    #[error("installation proxy operation failed")]
    InstallationProxyOperationFailed(String) = -32,

    #[cfg(feature = "afc")]
    #[error("afc error: {0}")]
    Afc(#[from] afc::errors::AfcError) = -33,

    #[cfg(feature = "afc")]
    #[error("unknown afc opcode")]
    UnknownAfcOpcode = -34,

    #[cfg(feature = "afc")]
    #[error("invalid afc magic")]
    InvalidAfcMagic = -35,

    #[cfg(feature = "afc")]
    #[error("missing file attribute")]
    AfcMissingAttribute = -36,

    #[cfg(feature = "crashreportcopymobile")]
    #[error("crash report mover sent the wrong response")]
    CrashReportMoverBadResponse(Vec<u8>) = -37,

    #[cfg(any(feature = "tss", feature = "tunneld"))]
    #[error("http reqwest error")]
    Reqwest(#[from] reqwest::Error) = -38,

    #[error("internal error")]
    InternalError(String) = -39,

    #[cfg(feature = "xpc")]
    #[error("unknown http frame type")]
    UnknownFrame(u8) = -40,

    #[cfg(feature = "xpc")]
    #[error("unknown http setting type")]
    UnknownHttpSetting(u16) = -41,

    #[cfg(feature = "xpc")]
    #[error("Unintialized stream ID")]
    UninitializedStreamId = -42,

    #[cfg(feature = "xpc")]
    #[error("unknown XPC type")]
    UnknownXpcType(u32) = -43,

    #[cfg(feature = "xpc")]
    #[error("malformed XPC message")]
    MalformedXpc = -44,

    #[cfg(feature = "xpc")]
    #[error("invalid XPC magic")]
    InvalidXpcMagic = -45,

    #[cfg(feature = "xpc")]
    #[error("unexpected XPC version")]
    UnexpectedXpcVersion = -46,

    #[cfg(feature = "xpc")]
    #[error("invalid C string")]
    InvalidCString = -47,

    #[cfg(feature = "xpc")]
    #[error("stream reset")]
    HttpStreamReset = -48,

    #[cfg(feature = "xpc")]
    #[error("go away packet received")]
    HttpGoAway(String) = -49,

    #[cfg(feature = "dvt")]
    #[error("NSKeyedArchive error")]
    NsKeyedArchiveError(#[from] ns_keyed_archive::ConverterError) = -50,

    #[cfg(feature = "dvt")]
    #[error("Unknown aux value type")]
    UnknownAuxValueType(u32) = -51,

    #[cfg(feature = "dvt")]
    #[error("unknown channel")]
    UnknownChannel(u32) = -52,

    #[error("cannot parse string as IpAddr")]
    AddrParseError(#[from] std::net::AddrParseError) = -53,

    #[cfg(feature = "dvt")]
    #[error("disable memory limit failed")]
    DisableMemoryLimitFailed = -54,

    #[error("not enough bytes, expected {1}, got {0}")]
    NotEnoughBytes(usize, usize) = -55,

    #[error("failed to parse bytes as valid utf8")]
    Utf8Error = -56,

    #[cfg(feature = "debug_proxy")]
    #[error("invalid argument passed")]
    InvalidArgument = -57,

    #[error("unknown error `{0}` returned from device")]
    UnknownErrorType(String) = -59,

    #[error("invalid arguments were passed")]
    FfiInvalidArg = -60,
    #[error("invalid string was passed")]
    FfiInvalidString = -61,
    #[error("buffer passed is too small - needs {0}, got {1}")]
    FfiBufferTooSmall(usize, usize) = -62,
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

    pub fn code(&self) -> i32 {
        match self {
            IdeviceError::Socket(_) => -1,
            IdeviceError::PemParseFailed(_) => -2,
            IdeviceError::Rustls(_) => -3,
            IdeviceError::TlsBuilderFailed(_) => -4,
            IdeviceError::Plist(_) => -5,
            IdeviceError::Utf8(_) => -6,
            IdeviceError::UnexpectedResponse => -7,
            IdeviceError::GetProhibited => -8,
            IdeviceError::SessionInactive => -9,
            IdeviceError::InvalidHostID => -10,
            IdeviceError::NoEstablishedConnection => -11,
            IdeviceError::HeartbeatSleepyTime => -12,
            IdeviceError::HeartbeatTimeout => -13,
            IdeviceError::NotFound => -14,
            IdeviceError::ServiceNotFound => -15,
            IdeviceError::CdtunnelPacketTooShort => -16,
            IdeviceError::CdtunnelPacketInvalidMagic => -17,
            IdeviceError::PacketSizeMismatch => -18,

            #[cfg(feature = "core_device_proxy")]
            IdeviceError::Json(_) => -19,

            IdeviceError::DeviceNotFound => -20,
            IdeviceError::DeviceLocked => -21,
            IdeviceError::UsbConnectionRefused => -22,
            IdeviceError::UsbBadCommand => -23,
            IdeviceError::UsbBadDevice => -24,
            IdeviceError::UsbBadVersion => -25,
            IdeviceError::BadBuildManifest => -26,
            IdeviceError::ImageNotMounted => -27,

            #[cfg(feature = "pair")]
            IdeviceError::PairingDialogResponsePending => -28,
            #[cfg(feature = "pair")]
            IdeviceError::UserDeniedPairing => -29,
            #[cfg(feature = "pair")]
            IdeviceError::PasswordProtected => -30,

            #[cfg(feature = "misagent")]
            IdeviceError::MisagentFailure => -31,

            #[cfg(feature = "installation_proxy")]
            IdeviceError::InstallationProxyOperationFailed(_) => -32,

            #[cfg(feature = "afc")]
            IdeviceError::Afc(_) => -33,
            #[cfg(feature = "afc")]
            IdeviceError::UnknownAfcOpcode => -34,
            #[cfg(feature = "afc")]
            IdeviceError::InvalidAfcMagic => -35,
            #[cfg(feature = "afc")]
            IdeviceError::AfcMissingAttribute => -36,

            #[cfg(feature = "crashreportcopymobile")]
            IdeviceError::CrashReportMoverBadResponse(_) => -37,

            #[cfg(any(feature = "tss", feature = "tunneld"))]
            IdeviceError::Reqwest(_) => -38,

            IdeviceError::InternalError(_) => -39,

            #[cfg(feature = "xpc")]
            IdeviceError::UnknownFrame(_) => -40,
            #[cfg(feature = "xpc")]
            IdeviceError::UnknownHttpSetting(_) => -41,
            #[cfg(feature = "xpc")]
            IdeviceError::UninitializedStreamId => -42,
            #[cfg(feature = "xpc")]
            IdeviceError::UnknownXpcType(_) => -43,
            #[cfg(feature = "xpc")]
            IdeviceError::MalformedXpc => -44,
            #[cfg(feature = "xpc")]
            IdeviceError::InvalidXpcMagic => -45,
            #[cfg(feature = "xpc")]
            IdeviceError::UnexpectedXpcVersion => -46,
            #[cfg(feature = "xpc")]
            IdeviceError::InvalidCString => -47,
            #[cfg(feature = "xpc")]
            IdeviceError::HttpStreamReset => -48,
            #[cfg(feature = "xpc")]
            IdeviceError::HttpGoAway(_) => -49,

            #[cfg(feature = "dvt")]
            IdeviceError::NsKeyedArchiveError(_) => -50,
            #[cfg(feature = "dvt")]
            IdeviceError::UnknownAuxValueType(_) => -51,
            #[cfg(feature = "dvt")]
            IdeviceError::UnknownChannel(_) => -52,

            IdeviceError::AddrParseError(_) => -53,

            #[cfg(feature = "dvt")]
            IdeviceError::DisableMemoryLimitFailed => -54,

            IdeviceError::NotEnoughBytes(_, _) => -55,
            IdeviceError::Utf8Error => -56,

            #[cfg(feature = "debug_proxy")]
            IdeviceError::InvalidArgument => -57,

            IdeviceError::UnknownErrorType(_) => -59,
            IdeviceError::FfiInvalidArg => -60,
            IdeviceError::FfiInvalidString => -61,
            IdeviceError::FfiBufferTooSmall(_, _) => -62,
        }
    }
}
