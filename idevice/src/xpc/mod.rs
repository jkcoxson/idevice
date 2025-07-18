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

    pub async fn do_handshake(&mut self) -> Result<(), IdeviceError> {
        self.h2_client
            .set_settings(
                vec![
                    Setting::MaxConcurrentStreams(100),
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

        Ok(())
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

    pub async fn recv_root(&mut self) -> Result<plist::Value, IdeviceError> {
        let mut msg_buffer = Vec::new();
        loop {
            msg_buffer.extend(self.h2_client.read(ROOT_CHANNEL).await?);
            let msg = match XPCMessage::decode(&msg_buffer) {
                Ok(m) => m,
                Err(IdeviceError::PacketSizeMismatch) => continue,
                Err(e) => break Err(e),
            };

            match msg.message {
                Some(msg) => {
                    if let Some(d) = msg.as_dictionary() {
                        if d.is_empty() {
                            msg_buffer.clear();
                            continue;
                        }
                    }
                    break Ok(msg.to_plist());
                }
                None => {
                    // don't care didn't ask
                    msg_buffer.clear();
                    continue;
                }
            }
        }
    }

    pub async fn send_object(
        &mut self,
        msg: plist::Value,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        let msg: XPCObject = msg.into();

        let mut flag = XPCFlag::DataFlag | XPCFlag::AlwaysSet;
        if expect_reply {
            flag |= XPCFlag::WantingReply;
        }

        let msg = XPCMessage::new(Some(flag), Some(msg), Some(self.root_id));
        self.send_root(msg).await?;

        Ok(())
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
