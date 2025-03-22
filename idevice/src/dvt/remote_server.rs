// Jackson Coxson

use std::collections::{HashMap, VecDeque};

use log::{debug, warn};
use tokio::io::AsyncWriteExt;

use crate::{
    dvt::message::{Aux, Message, MessageHeader, PayloadHeader},
    IdeviceError, ReadWrite,
};

use super::message::AuxValue;

pub const INSTRUMENTS_MESSAGE_TYPE: u32 = 2;

pub struct RemoteServerClient<R: ReadWrite> {
    idevice: R,
    current_message: u32,
    new_channel: u32,
    channels: HashMap<u32, VecDeque<Message>>,
}

pub struct Channel<'a, R: ReadWrite> {
    client: &'a mut RemoteServerClient<R>,
    channel: u32,
}

impl<R: ReadWrite> RemoteServerClient<R> {
    pub fn new(idevice: R) -> Result<Self, IdeviceError> {
        let mut channels = HashMap::new();
        channels.insert(0, VecDeque::new());
        Ok(Self {
            idevice,
            current_message: 0,
            new_channel: 1,
            channels,
        })
    }

    pub fn root_channel(&mut self) -> Channel<R> {
        Channel {
            client: self,
            channel: 0,
        }
    }

    pub async fn make_channel(
        &mut self,
        identifier: impl Into<String>,
    ) -> Result<Channel<R>, IdeviceError> {
        let code = self.new_channel;
        self.new_channel += 1;

        let args = vec![
            AuxValue::U32(code),
            AuxValue::Array(
                ns_keyed_archive::encode::encode_to_bytes(plist::Value::String(identifier.into()))
                    .expect("Failed to encode"),
            ),
        ];

        let mut root = self.root_channel();
        root.call_method(
            Some("_requestChannelWithCode:identifier:"),
            Some(args),
            true,
        )
        .await?;

        let res = root.read_message().await?;
        if res.data.is_some() {
            return Err(IdeviceError::UnexpectedResponse);
        }

        self.channels.insert(code, VecDeque::new());

        self.build_channel(code)
    }

    fn build_channel(&mut self, code: u32) -> Result<Channel<R>, IdeviceError> {
        Ok(Channel {
            client: self,
            channel: code,
        })
    }

    pub async fn call_method(
        &mut self,
        channel: u32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        self.current_message += 1;

        let mheader = MessageHeader::new(0, 1, self.current_message, 0, channel, expect_reply);
        let pheader = PayloadHeader::method_invocation();
        let aux = args.map(Aux::from_values);
        let data: Option<plist::Value> = data.map(Into::into);

        let message = Message::new(mheader, pheader, aux, data);
        debug!("Sending message: {message:#?}");
        self.idevice.write_all(&message.serialize()).await?;

        Ok(())
    }

    pub async fn read_message(&mut self, channel: u32) -> Result<Message, IdeviceError> {
        // Determine if we already have a message cached
        let cache = match self.channels.get_mut(&channel) {
            Some(c) => c,
            None => return Err(IdeviceError::UnknownChannel(channel)),
        };

        if let Some(msg) = cache.pop_front() {
            return Ok(msg);
        }

        loop {
            let msg = Message::from_reader(&mut self.idevice).await?;
            debug!("Read message: {msg:#?}");

            if msg.message_header.channel == channel {
                return Ok(msg);
            } else if let Some(cache) = self.channels.get_mut(&msg.message_header.channel) {
                cache.push_back(msg);
            } else {
                warn!(
                    "Received message for unknown channel: {}",
                    msg.message_header.channel
                );
            }
        }
    }
}

impl<R: ReadWrite> Channel<'_, R> {
    pub async fn read_message(&mut self) -> Result<Message, IdeviceError> {
        self.client.read_message(self.channel).await
    }

    pub async fn call_method(
        &mut self,
        method: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        self.client
            .call_method(self.channel, method, args, expect_reply)
            .await
    }
}
