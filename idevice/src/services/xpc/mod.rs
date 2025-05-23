// Jackson Coxson

use http2::Setting;
use log::debug;

use crate::{IdeviceError, ReadWrite};

mod format;
mod http2;

pub use format::XPCMessage;
use format::{XPCFlag, XPCObject};

const ROOT_CHANNEL: u32 = 1;
const REPLY_CHANNEL: u32 = 3;

pub struct RemoteXpcClient<R: ReadWrite> {
    h2_client: http2::Http2Client<R>,
    root_id: u64,
    reply_id: u64,
}

impl<R: ReadWrite> RemoteXpcClient<R> {
    pub async fn new(socket: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            h2_client: http2::Http2Client::new(socket).await?,
            root_id: 1,
            reply_id: 1,
        })
    }

    pub async fn do_handshake(&mut self) -> Result<plist::Value, IdeviceError> {
        self.h2_client
            .set_settings(
                vec![
                    Setting::MaxConcurrentStreams(10),
                    Setting::InitialWindowSize(1048576),
                ],
                0,
            )
            .await?;
        self.h2_client.window_update(983041, 0).await?;
        self.h2_client.open_stream(1).await?; // root channel

        debug!("Sending empty dictionary");
        self.send_root(XPCMessage::new(
            Some(XPCFlag::AlwaysSet),
            Some(XPCObject::Dictionary(Default::default())),
            None,
        ))
        .await?;

        self.recv_root().await?;
        self.recv_root().await?;

        debug!("Sending weird flags");
        self.send_root(XPCMessage::new(Some(XPCFlag::Custom(0x201)), None, None))
            .await?;

        debug!("Opening reply stream");
        self.h2_client.open_stream(REPLY_CHANNEL).await?;
        self.send_reply(XPCMessage::new(
            Some(XPCFlag::InitHandshake | XPCFlag::AlwaysSet),
            None,
            None,
        ))
        .await?;

        let mut total_msg = Vec::new();
        loop {
            // We receive from the root channel for this message
            total_msg.extend(self.h2_client.read(ROOT_CHANNEL).await?);
            let msg = match XPCMessage::decode(&total_msg) {
                Ok(m) => m,
                Err(IdeviceError::PacketSizeMismatch) => {
                    continue;
                }
                Err(e) => {
                    return Err(e);
                }
            };

            match msg.message {
                Some(msg) => {
                    return Ok(msg.to_plist());
                }
                None => {
                    return Err(IdeviceError::UnexpectedResponse);
                }
            };
        }
    }

    pub async fn recv(&mut self) -> Result<plist::Value, IdeviceError> {
        loop {
            let msg = self.h2_client.read(REPLY_CHANNEL).await?;

            let msg = XPCMessage::decode(&msg)?;
            if let Some(msg) = msg.message {
                return Ok(msg.to_plist());
            }
            self.reply_id += 1;
        }
    }

    pub async fn recv_root(&mut self) -> Result<Option<plist::Value>, IdeviceError> {
        let msg = self.h2_client.read(ROOT_CHANNEL).await?;
        let msg = XPCMessage::decode(&msg)?;

        if let Some(msg) = msg.message {
            Ok(Some(msg.to_plist()))
        } else {
            Ok(None)
        }
    }

    async fn send_root(&mut self, msg: XPCMessage) -> Result<(), IdeviceError> {
        self.h2_client
            .send(msg.encode(self.root_id)?, ROOT_CHANNEL)
            .await?;
        Ok(())
    }

    async fn send_reply(&mut self, msg: XPCMessage) -> Result<(), IdeviceError> {
        self.h2_client
            .send(msg.encode(self.root_id)?, REPLY_CHANNEL)
            .await?;
        Ok(())
    }
}
