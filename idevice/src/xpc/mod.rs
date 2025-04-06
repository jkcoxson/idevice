//! XPC (Cross-Process Communication) Implementation
//!
//! Provides functionality for interacting with Apple's XPC protocol over HTTP/2,
//! which is used for inter-process communication between iOS/macOS components.

use std::collections::HashMap;

use crate::{
    http2::{
        self,
        h2::{SettingsFrame, WindowUpdateFrame},
    },
    IdeviceError, ReadWrite,
};
use error::XPCError;
use format::{XPCFlag, XPCMessage, XPCObject};
use log::{debug, warn};
use serde::Deserialize;

pub mod error;
mod format;

/// Represents an XPC connection to a device with available services
pub struct XPCDevice<R: ReadWrite> {
    /// The underlying XPC connection
    pub connection: XPCConnection<R>,
    /// Map of available XPC services by name
    pub services: HashMap<String, XPCService>,
}

/// Describes an available XPC service
#[derive(Debug, Clone, Deserialize)]
pub struct XPCService {
    /// Required entitlement to access this service
    pub entitlement: String,
    /// Port number where the service is available
    pub port: u16,
    /// Whether the service uses remote XPC
    pub uses_remote_xpc: bool,
    /// Optional list of supported features
    pub features: Option<Vec<String>>,
    /// Optional service version number
    pub service_version: Option<i64>,
}

/// Manages an active XPC connection over HTTP/2
pub struct XPCConnection<R: ReadWrite> {
    pub(crate) inner: http2::Connection<R>,
    root_message_id: u64,
    reply_message_id: u64,
}

impl<R: ReadWrite> XPCDevice<R> {
    /// Creates a new XPC device connection
    ///
    /// # Arguments
    /// * `stream` - The underlying transport stream
    ///
    /// # Returns
    /// A connected XPCDevice instance with discovered services
    ///
    /// # Errors
    /// Returns `IdeviceError` if:
    /// - The connection fails
    /// - The service discovery response is malformed
    pub async fn new(stream: R) -> Result<Self, IdeviceError> {
        let mut connection = XPCConnection::new(stream).await?;

        // Read initial services message
        let data = connection.read_message(http2::ROOT_CHANNEL).await?;

        let data = match data.message {
            Some(d) => match d
                .as_dictionary()
                .and_then(|x| x.get("Services"))
                .and_then(|x| x.as_dictionary())
            {
                Some(d) => d.to_owned(),
                None => return Err(IdeviceError::UnexpectedResponse),
            },
            None => return Err(IdeviceError::UnexpectedResponse),
        };

        // Parse available services
        let mut services = HashMap::new();
        for (name, service) in data.into_iter() {
            match service.as_dictionary() {
                Some(service) => {
                    let entitlement = match service.get("Entitlement").and_then(|x| x.as_string()) {
                        Some(e) => e.to_string(),
                        None => {
                            warn!("Service did not contain entitlement string");
                            continue;
                        }
                    };
                    let port = match service
                        .get("Port")
                        .and_then(|x| x.as_string())
                        .and_then(|x| x.parse::<u16>().ok())
                    {
                        Some(e) => e,
                        None => {
                            warn!("Service did not contain port string");
                            continue;
                        }
                    };
                    let uses_remote_xpc = match service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("UsesRemoteXPC"))
                        .and_then(|x| x.as_bool())
                    {
                        Some(e) => e.to_owned(),
                        None => false, // default is false
                    };

                    let features = service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("Features"))
                        .and_then(|x| x.as_array())
                        .map(|f| {
                            f.iter()
                                .filter_map(|x| x.as_string())
                                .map(|x| x.to_string())
                                .collect::<Vec<String>>()
                        });

                    let service_version = service
                        .get("Properties")
                        .and_then(|x| x.as_dictionary())
                        .and_then(|x| x.get("ServiceVersion"))
                        .and_then(|x| x.as_signed_integer())
                        .map(|e| e.to_owned());

                    services.insert(
                        name,
                        XPCService {
                            entitlement,
                            port,
                            uses_remote_xpc,
                            features,
                            service_version,
                        },
                    );
                }
                None => {
                    warn!("Service is not a dictionary!");
                    continue;
                }
            }
        }

        Ok(Self {
            connection,
            services,
        })
    }

    /// Consumes the device and returns the underlying transport stream
    pub fn into_inner(self) -> R {
        self.connection.inner.stream
    }
}

impl<R: ReadWrite> XPCConnection<R> {
    /// Channel ID for root messages
    pub const ROOT_CHANNEL: u32 = http2::ROOT_CHANNEL;
    /// Channel ID for reply messages
    pub const REPLY_CHANNEL: u32 = http2::REPLY_CHANNEL;
    /// Initial stream ID for HTTP/2 connection
    const INIT_STREAM: u32 = http2::INIT_STREAM;

    /// Establishes a new XPC connection
    ///
    /// # Arguments
    /// * `stream` - The underlying transport stream
    ///
    /// # Returns
    /// A connected XPCConnection instance
    ///
    /// # Errors
    /// Returns `XPCError` if the connection handshake fails
    pub async fn new(stream: R) -> Result<Self, XPCError> {
        let mut client = http2::Connection::new(stream).await?;

        // Configure HTTP/2 settings
        client
            .send_frame(SettingsFrame::new(
                [
                    (SettingsFrame::MAX_CONCURRENT_STREAMS, 100),
                    (SettingsFrame::INITIAL_WINDOW_SIZE, 1048576),
                ]
                .into_iter()
                .collect(),
                Default::default(),
            ))
            .await?;

        // Update window size
        client
            .send_frame(WindowUpdateFrame::new(Self::INIT_STREAM, 983041))
            .await?;

        let mut xpc_client = Self {
            inner: client,
            root_message_id: 1,
            reply_message_id: 1,
        };

        // Perform XPC handshake
        xpc_client
            .send_recv_message(
                Self::ROOT_CHANNEL,
                XPCMessage::new(
                    Some(XPCFlag::AlwaysSet),
                    Some(XPCObject::Dictionary(Default::default())),
                    None,
                ),
            )
            .await?;

        xpc_client
            .send_recv_message(
                Self::REPLY_CHANNEL,
                XPCMessage::new(
                    Some(XPCFlag::InitHandshake | XPCFlag::AlwaysSet),
                    None,
                    None,
                ),
            )
            .await?;

        xpc_client
            .send_recv_message(
                Self::ROOT_CHANNEL,
                XPCMessage::new(Some(XPCFlag::Custom(0x201)), None, None),
            )
            .await?;

        Ok(xpc_client)
    }

    /// Sends a message and waits for the response
    ///
    /// # Arguments
    /// * `stream_id` - The channel/stream to use
    /// * `message` - The XPC message to send
    ///
    /// # Returns
    /// The response message
    pub async fn send_recv_message(
        &mut self,
        stream_id: u32,
        message: XPCMessage,
    ) -> Result<XPCMessage, XPCError> {
        self.send_message(stream_id, message).await?;
        self.read_message(stream_id).await
    }

    /// Sends an XPC message without waiting for a response
    pub async fn send_message(
        &mut self,
        stream_id: u32,
        message: XPCMessage,
    ) -> Result<(), XPCError> {
        self.inner
            .write_streamid(stream_id, message.encode(self.root_message_id)?)
            .await?;
        Ok(())
    }

    /// Reads an XPC message from the specified stream
    pub async fn read_message(&mut self, stream_id: u32) -> Result<XPCMessage, XPCError> {
        let mut buf = self.inner.read_streamid(stream_id).await?;
        loop {
            match XPCMessage::decode(&buf) {
                Ok(decoded) => {
                    debug!("Decoded message: {:?}", decoded);
                    match stream_id {
                        1 => self.root_message_id += 1,
                        3 => self.reply_message_id += 1,
                        _ => {}
                    }
                    return Ok(decoded);
                }
                Err(err) => {
                    log::warn!("Error decoding message: {:?}", err);
                    buf.extend_from_slice(&self.inner.read_streamid(stream_id).await?);
                }
            }
        }
    }
}

