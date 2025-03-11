// Jackson Coxson

use std::collections::HashMap;

use tokio::io::AsyncWriteExt;

use crate::{
    dvt::message::{Aux, Message, MessageHeader, PayloadHeader},
    IdeviceError, ReadWrite,
};

use super::message::AuxValue;

pub const INSTRUMENTS_MESSAGE_TYPE: u32 = 2;

pub struct RemoteServerClient {
    idevice: Box<dyn ReadWrite>,
    current_message: u32,
    new_channel: u32,
    channels: HashMap<u8, Vec<Message>>,
}

impl RemoteServerClient {
    pub fn new(idevice: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self {
            idevice,
            current_message: 0,
            new_channel: 1,
            channels: HashMap::new(),
        })
    }

    pub async fn make_channel(
        &mut self,
        identifier: impl Into<String>,
    ) -> Result<(), IdeviceError> {
        let code = self.new_channel;
        let args = vec![AuxValue::U32(code), AuxValue::String(identifier.into())];
        self.send_message(
            0,
            Some("_requestChannelWithCode:identifier:"),
            Some(args),
            true,
        )
        .await?;

        let res = self.read_message().await?;
        if res.data.is_some() {
            return Err(IdeviceError::UnexpectedResponse);
        }

        Ok(())
    }

    pub async fn send_message(
        &mut self,
        channel: u32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        self.current_message += 1;

        let mheader = MessageHeader::new(0, 1, self.current_message, 0, channel, expect_reply);
        let mut pheader = PayloadHeader::instruments_message_type();
        if expect_reply {
            pheader.apply_expects_reply_map();
        }
        let aux = args.map(Aux::from_values);
        let data: Option<plist::Value> = data.map(Into::into);

        let message = Message::new(mheader, pheader, aux, data);
        self.idevice.write_all(&message.serialize()).await?;

        Ok(())
    }

    pub async fn read_message(&mut self) -> Result<Message, IdeviceError> {
        Message::from_reader(&mut self.idevice).await
    }
}

pub struct Channel {}

#[cfg(test)]
mod tests {
    use crate::util::plist_to_archived_bytes;

    #[test]
    fn t1() {
        let selector: plist::Value = "asdf".into();
        let selector = plist_to_archived_bytes(selector);

        std::fs::write("/Users/jacksoncoxson/code/test/test-rs.plist", selector).unwrap();
    }
}
