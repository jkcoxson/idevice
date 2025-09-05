//! Remote Server Client implementation for iOS instruments protocol.
//!
//! This module provides a client for communicating with iOS devices through the
//! remote server protocol used by instruments. It handles channel management and
//! message passing between the host and device.
//!
//! Remote Server communicates via NSKeyedArchives. These archives are binary plists
//! formatted specifically for naive recreation at the target.
//! Requests are sent as method calls to objective C objects on the device.
//!
//! # Overview
//! The client manages multiple communication channels and provides methods for:
//! - Creating new channels
//! - Sending method calls
//! - Reading responses
//!
//! # Example
//! ```rust,no_run
//! use std::sync::Arc;
//! use tokio::net::TcpStream;
//! use your_crate::{ReadWrite, IdeviceError};
//! use your_crate::instruments::RemoteServerClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), IdeviceError> {
//!     // Establish connection to device over the tunnel (see XPC docs)
//!     let transport = TcpStream::connect("1.2.3.4:1234").await?;
//!
//!     // Create client
//!     let mut client = RemoteServerClient::new(transport);
//!
//!     // Read the first message
//!     client.read_message(0).await?;
//!
//!     // Call a method on root channel
//!     client.call_method(
//!         0,
//!         Some("someMethod"),
//!         Some(vec![AuxValue::String("param".into())]),
//!         true
//!     ).await?;
//!
//!     // Read response
//!     let response = client.read_message(0).await?;
//!     println!("Got response: {:?}", response);
//!
//!
//!     Ok(())
//! }
//! ```

use std::collections::{HashMap, VecDeque};

use log::{debug, warn};
use tokio::io::AsyncWriteExt;

use crate::{
    IdeviceError, ReadWrite,
    dvt::message::{Aux, AuxValue, Message, MessageHeader, PayloadHeader},
};

/// Message type identifier for instruments protocol
pub const INSTRUMENTS_MESSAGE_TYPE: u32 = 2;

/// Client for communicating with iOS remote server protocol
///
/// Manages multiple communication channels and handles message serialization/deserialization.
/// Each channel operates independently and maintains its own message queue.
pub struct RemoteServerClient<R: ReadWrite> {
    /// The underlying device connection
    idevice: R,
    /// Counter for message identifiers
    current_message: u32,
    /// Next available channel number
    new_channel: u32,
    /// Map of channel numbers to their message queues
    channels: HashMap<u32, VecDeque<Message>>,
}

/// Handle to a specific communication channel
///
/// Provides channel-specific operations for use on the remote server client.
pub struct Channel<'a, R: ReadWrite> {
    /// Reference to parent client
    client: &'a mut RemoteServerClient<R>,
    /// Channel number this handle operates on
    channel: u32,
}

impl<R: ReadWrite> RemoteServerClient<R> {
    /// Creates a new RemoteServerClient with the given transport
    ///
    /// # Arguments
    /// * `idevice` - The underlying transport implementing ReadWrite
    ///
    /// # Returns
    /// A new client instance with root channel (0) initialized
    pub fn new(idevice: R) -> Self {
        let mut channels = HashMap::new();
        channels.insert(0, VecDeque::new());
        Self {
            idevice,
            current_message: 0,
            new_channel: 1,
            channels,
        }
    }

    /// Consumes the client and returns the underlying transport
    pub fn into_inner(self) -> R {
        self.idevice
    }

    /// Returns a handle to the root channel (channel 0)
    pub fn root_channel<'c>(&'c mut self) -> Channel<'c, R> {
        Channel {
            client: self,
            channel: 0,
        }
    }

    /// Creates a new channel with the given identifier
    ///
    /// # Arguments
    /// * `identifier` - String identifier for the new channel
    ///
    /// # Returns
    /// * `Ok(Channel)` - Handle to the new channel
    /// * `Err(IdeviceError)` - If channel creation fails
    ///
    /// # Errors
    /// * `IdeviceError::UnexpectedResponse` if server responds with unexpected data
    /// * Other IO or serialization errors
    pub async fn make_channel<'c>(
        &'c mut self,
        identifier: impl Into<String>,
    ) -> Result<Channel<'c, R>, IdeviceError> {
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

    fn build_channel<'c>(&'c mut self, code: u32) -> Result<Channel<'c, R>, IdeviceError> {
        Ok(Channel {
            client: self,
            channel: code,
        })
    }

    /// Calls a method on the specified channel
    ///
    /// # Arguments
    /// * `channel` - Channel number to call method on
    /// * `data` - Optional method data (plist value)
    /// * `args` - Optional arguments for the method
    /// * `expect_reply` - Whether to expect a response
    ///
    /// # Returns
    /// * `Ok(())` - If method was successfully called
    /// * `Err(IdeviceError)` - If call failed
    ///
    /// # Errors
    /// IO or serialization errors
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
        self.idevice.flush().await?;

        Ok(())
    }

    /// Reads the next message from the specified channel
    ///
    /// Checks cached messages first, then reads from transport if needed.
    ///
    /// # Arguments
    /// * `channel` - Channel number to read from
    ///
    /// # Returns
    /// * `Ok(Message)` - The received message
    /// * `Err(IdeviceError)` - If read failed
    ///
    /// # Errors
    /// * `IdeviceError::UnknownChannel` if channel doesn't exist
    /// * Other IO or deserialization errors
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
    /// Reads the next message from the remote server on this channel
    ///
    /// # Returns
    /// * `Ok(Message)` - The received message
    /// * `Err(IdeviceError)` - If read failed
    ///
    /// # Errors
    /// * `IdeviceError::UnknownChannel` if channel doesn't exist
    /// * Other IO or deserialization errors
    pub async fn read_message(&mut self) -> Result<Message, IdeviceError> {
        self.client.read_message(self.channel).await
    }

    /// Calls a method on the specified channel
    ///
    /// # Arguments
    /// * `method` - Optional method data (plist value)
    /// * `args` - Optional arguments for the method
    /// * `expect_reply` - Whether to expect a response
    ///
    /// # Returns
    /// * `Ok(())` - If method was successfully called
    /// * `Err(IdeviceError)` - If call failed
    ///
    /// # Errors
    /// IO or serialization errors
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
