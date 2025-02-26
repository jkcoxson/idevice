// Thanks DebianArch

use std::collections::HashMap;

use crate::{
    http2::{
        self,
        h2::{SettingsFrame, WindowUpdateFrame},
    },
    IdeviceError,
};
use error::XPCError;
use format::{XPCFlag, XPCMessage, XPCObject};
use log::{debug, warn};
use serde::Deserialize;

pub mod cdtunnel;
pub mod error;
pub mod format;

pub struct XPCDevice {
    pub connection: XPCConnection,
    pub services: HashMap<String, XPCService>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct XPCService {
    pub entitlement: String,
    pub port: u16,
    pub uses_remote_xpc: bool,
    pub features: Option<Vec<String>>,
    pub service_version: Option<i64>,
}

pub struct XPCConnection {
    inner: http2::Connection,
    root_message_id: u64,
    reply_message_id: u64,
}

impl XPCDevice {
    pub async fn new(stream: crate::IdeviceSocket) -> Result<Self, IdeviceError> {
        let mut connection = XPCConnection::new(stream).await?;

        let data = connection
            .read_message(http2::Connection::ROOT_CHANNEL)
            .await?;

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
}

impl XPCConnection {
    pub const ROOT_CHANNEL: u32 = http2::Connection::ROOT_CHANNEL;
    pub const REPLY_CHANNEL: u32 = http2::Connection::REPLY_CHANNEL;
    const INIT_STREAM: u32 = http2::Connection::INIT_STREAM;

    pub async fn new(stream: crate::IdeviceSocket) -> Result<Self, XPCError> {
        let mut client = http2::Connection::new(stream).await?;
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
        client
            .send_frame(WindowUpdateFrame::new(Self::INIT_STREAM, 983041))
            .await?;
        let mut xpc_client = Self {
            inner: client,
            root_message_id: 1,
            reply_message_id: 1,
        };
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

        // we are here. we send data to stream_id 3 yet we get data from stream 1 ???
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

    pub async fn send_recv_message(
        &mut self,
        stream_id: u32,
        message: XPCMessage,
    ) -> Result<XPCMessage, XPCError> {
        self.send_message(stream_id, message).await?;
        self.read_message(stream_id).await
    }

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
                    log::error!("Error decoding message: {:?}", err);
                    buf.extend_from_slice(&self.inner.read_streamid(stream_id).await?);
                }
            }
        }
    }
}
