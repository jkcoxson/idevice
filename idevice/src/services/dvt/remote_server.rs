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

use std::{
    collections::{HashMap, VecDeque},
    future::Future,
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
};

#[cfg(not(feature = "xctest"))]
use std::io;

use plist::Dictionary;
use tokio::{
    io::{AsyncWriteExt, ReadHalf, WriteHalf},
    sync::{Mutex, Notify, oneshot},
};
use tracing::{debug, warn};

/// Pins `$event` and the close notifier as `$ev` and `$closed`, both armed
/// (`Notified::enable`) before the caller inspects the state it waits on, so a
/// `notify_waiters` that fires between the inspection and the wait is not
/// lost. The caller loops back to the inspection after either wakes, so a
/// message queued just before the connection closed is still delivered.
macro_rules! arm_wait {
    ($ev:ident, $closed:ident, $event:expr, $close:expr) => {
        let $ev = $event;
        let $closed = $close;
        tokio::pin!($ev, $closed);
        $ev.as_mut().enable();
        $closed.as_mut().enable();
    };
}

use super::errors::DvtError;

/// Wraps the spawn handle returned from `spawn_reader`. On native we hold a
/// real `tokio::task::JoinHandle` so `Drop` can `.abort()` the reader; on
/// wasm32 there's no join/abort primitive available. The reader exits on
/// transport EOF via the existing read loop, so this is a unit type.
#[cfg(not(target_arch = "wasm32"))]
type ReaderTask = tokio::task::JoinHandle<()>;
#[cfg(target_arch = "wasm32")]
type ReaderTask = ();

#[cfg(feature = "xctest")]
fn remote_timeout_error(timeout: std::time::Duration) -> IdeviceError {
    IdeviceError::XcTestTimeout(timeout.as_secs_f64())
}

#[cfg(not(feature = "xctest"))]
fn remote_timeout_error(timeout: std::time::Duration) -> IdeviceError {
    IdeviceError::Socket(io::Error::new(
        io::ErrorKind::TimedOut,
        format!(
            "remote server operation timed out after {:.1}s",
            timeout.as_secs_f64()
        ),
    ))
}

use crate::{
    IdeviceError, ReadWrite,
    dvt::message::{
        Aux, AuxValue, DEFAULT_MAX_MESSAGE_SIZE, Message, MessageHeader, PayloadHeader,
    },
};

/// Reply slots keyed by the identifier of the message awaiting a reply.
type PendingSlots = Mutex<HashMap<u32, oneshot::Sender<Message>>>;

/// Awaitable reply to one message sent by [`Channel::send_call`] or
/// [`RemoteServerClient::send_call`].
///
/// Resolves to the first incoming message whose identifier matches the sent
/// one and whose conversation index is non-zero (a reply). It holds no borrow
/// of the client, so it can be awaited inside `tokio::select!` next to
/// `read_message` on the same channel.
///
/// Resolves to `Err(IdeviceError::Socket(BrokenPipe))` when the connection
/// closes (reader error, peer EOF, or the client being dropped) before the
/// reply arrives. It never times out on its own; wrap it in a timeout.
///
/// Dropping it unresolved removes its reply slot; a reply that arrives later
/// is then treated as an ordinary incoming message and queued on its channel.
#[derive(Debug)]
pub struct PendingReply {
    identifier: u32,
    receiver: oneshot::Receiver<Message>,
    slots: Weak<PendingSlots>,
}

impl PendingReply {
    /// Returns the identifier of the sent message this reply answers.
    pub fn identifier(&self) -> u32 {
        self.identifier
    }
}

impl Future for PendingReply {
    type Output = Result<Message, IdeviceError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.receiver)
            .poll(cx)
            .map(|r| r.map_err(|_| closed_error()))
    }
}

impl Drop for PendingReply {
    fn drop(&mut self) {
        // Best effort: if the map is momentarily locked, the slot is pruned by
        // the next send (closed senders are dropped there) or on disconnect.
        if let Some(slots) = self.slots.upgrade()
            && let Ok(mut slots) = slots.try_lock()
        {
            slots.remove(&self.identifier);
        }
    }
}

fn closed_error() -> IdeviceError {
    IdeviceError::Socket(std::io::Error::new(
        std::io::ErrorKind::BrokenPipe,
        "remote server connection closed",
    ))
}

/// Why a connection stopped delivering messages. Every waiter sees the same
/// generic closed error; this tells a peer hangup from a protocol failure.
#[derive(Debug, Clone)]
pub enum CloseReason {
    /// The transport reached EOF: the peer hung up.
    Eof,
    /// The reader stopped on this frame or I/O error.
    Error(Arc<IdeviceError>),
    /// The client was closed or dropped on this side.
    Local,
}

/// Message type identifier for instruments protocol
pub const INSTRUMENTS_MESSAGE_TYPE: u32 = 2;

/// Client for communicating with iOS remote server protocol
///
/// Manages multiple communication channels and handles message serialization/deserialization.
/// Each channel operates independently and maintains its own message queue.
pub struct RemoteServerClient<R: ReadWrite> {
    label: Arc<str>,
    shared: Arc<RemoteServerShared<WriteHalf<R>>>,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    reader_task: ReaderTask,
}

/// Handle to a specific communication channel
///
/// Provides channel-specific operations for use on the remote server client.
#[derive(Debug)]
pub struct Channel<'a, R: ReadWrite> {
    /// Reference to parent client
    client: &'a mut RemoteServerClient<R>,
    /// Channel number this handle operates on
    channel: i32,
}

/// Owned handle to a specific communication channel.
///
/// This mirrors pymobiledevice3's `DTXChannel` lifetime model more closely
/// than the borrowed [`Channel`]: it keeps only the shared transport state and
/// the channel code, so service/proxy wrappers can outlive a temporary
/// `&mut RemoteServerClient` borrow.
#[derive(Debug)]
pub struct OwnedChannel<R: ReadWrite> {
    label: Arc<str>,
    shared: Arc<RemoteServerShared<WriteHalf<R>>>,
    channel: i32,
}

impl<R: ReadWrite> Clone for OwnedChannel<R> {
    fn clone(&self) -> Self {
        Self {
            label: self.label.clone(),
            shared: self.shared.clone(),
            channel: self.channel,
        }
    }
}

type IncomingMessageHandler = Arc<
    dyn Fn(
            Message,
        )
            -> Pin<Box<dyn Future<Output = Result<IncomingHandlerOutcome, IdeviceError>> + Send>>
        + Send
        + Sync,
>;

type IncomingChannelInitializer<W> = Arc<
    dyn Fn(
            Arc<str>,
            Arc<RemoteServerShared<W>>,
            i32,
            String,
        ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send>>
        + Send
        + Sync,
>;

pub(crate) enum IncomingHandlerOutcome {
    Unhandled,
    HandledNoReply,
    Reply(Vec<u8>),
}

#[derive(Debug, Default)]
struct ChannelQueue {
    messages: Mutex<VecDeque<Message>>,
    notify: Notify,
}

#[derive(Debug, Clone)]
struct ChannelMetadata {
    code: i32,
    identifier: String,
    remote: bool,
}

struct IncomingChannelRegistration<W> {
    identifiers: Vec<String>,
    initializer: IncomingChannelInitializer<W>,
}

#[derive(Debug, Clone)]
enum CapabilityHandshakeState {
    Pending,
    Skipped,
    Received(Dictionary),
}

struct RemoteServerShared<W> {
    label: Arc<str>,
    writer: Mutex<W>,
    current_message: AtomicU32,
    new_channel: AtomicU32,
    channels: Mutex<HashMap<i32, Arc<ChannelQueue>>>,
    channel_metadata: Mutex<HashMap<i32, ChannelMetadata>>,
    pending_replies: Arc<PendingSlots>,
    max_message_size: AtomicUsize,
    handlers: Mutex<HashMap<i32, IncomingMessageHandler>>,
    incoming_channel_registrations: Mutex<Vec<IncomingChannelRegistration<W>>>,
    registry_notify: Notify,
    supported_identifiers: Mutex<CapabilityHandshakeState>,
    handshake_notify: Notify,
    closed: AtomicBool,
    closed_notify: Notify,
    close_reason: std::sync::Mutex<Option<CloseReason>>,
}

impl<W> std::fmt::Debug for RemoteServerShared<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteServerShared")
            .field(
                "current_message",
                &self.current_message.load(Ordering::Relaxed),
            )
            .field("new_channel", &self.new_channel.load(Ordering::Relaxed))
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl<W> RemoteServerShared<W> {
    /// Keeps the first reason only: a local close after a reader failure
    /// does not overwrite why the reader stopped.
    fn record_close_reason(&self, reason: CloseReason) {
        let mut slot = self.close_reason.lock().unwrap_or_else(|p| p.into_inner());
        slot.get_or_insert(reason);
    }

    fn new(label: Arc<str>, writer: W) -> Self {
        let mut channels = HashMap::new();
        channels.insert(0, Arc::new(ChannelQueue::default()));
        let mut channel_metadata = HashMap::new();
        channel_metadata.insert(
            0,
            ChannelMetadata {
                code: 0,
                identifier: "ctrl".into(),
                remote: false,
            },
        );
        Self {
            label,
            writer: Mutex::new(writer),
            current_message: AtomicU32::new(0),
            new_channel: AtomicU32::new(1),
            channels: Mutex::new(channels),
            channel_metadata: Mutex::new(channel_metadata),
            pending_replies: Arc::new(Mutex::new(HashMap::new())),
            max_message_size: AtomicUsize::new(DEFAULT_MAX_MESSAGE_SIZE),
            handlers: Mutex::new(HashMap::new()),
            incoming_channel_registrations: Mutex::new(Vec::new()),
            registry_notify: Notify::new(),
            supported_identifiers: Mutex::new(CapabilityHandshakeState::Pending),
            handshake_notify: Notify::new(),
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
            close_reason: std::sync::Mutex::new(None),
        }
    }
}

impl<R: ReadWrite> std::fmt::Debug for RemoteServerClient<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteServerClient")
            .field("shared", &"<remote-server-shared>")
            .finish()
    }
}

impl<R: ReadWrite> RemoteServerClient<R> {
    /// Creates a new client with a debug label used in tracing output.
    fn with_label_typed(idevice: R, label: impl Into<String>) -> Self
    where
        R: 'static,
    {
        let (reader, writer) = tokio::io::split(idevice);
        let label: Arc<str> = label.into().into();
        let shared = Arc::new(RemoteServerShared::new(label.clone(), writer));
        let reader_task = Self::spawn_reader(label.clone(), shared.clone(), reader);
        Self {
            label,
            shared,
            reader_task,
        }
    }

    /// Sets the cap on one reassembled incoming DTX message (payload header,
    /// aux and data; default [`DEFAULT_MAX_MESSAGE_SIZE`], 16 MiB).
    ///
    /// Applies to the next message the reader starts reading. A message over the
    /// cap is a fatal framing error ([`DvtError::MessageTooLarge`]): the reader
    /// stops, the connection is marked closed, and every waiter gets a closed
    /// error. This bounds wire/reassembly storage per message only; queued
    /// messages are not bounded.
    pub fn set_max_message_size(&self, bytes: usize) {
        self.shared.max_message_size.store(bytes, Ordering::Relaxed);
    }

    /// Returns a handle to the root channel (channel 0)
    pub fn root_channel<'c>(&'c mut self) -> Channel<'c, R> {
        Channel {
            client: self,
            channel: 0,
        }
    }

    /// Returns a future that resolves when this DTX connection disconnects.
    ///
    /// This captures the shared state by clone so callers can await it
    /// alongside operations that hold a mutable borrow of the client.
    pub(crate) fn disconnect_waiter(&self) -> impl Future<Output = ()> + Send + 'static
    where
        R: 'static,
    {
        let shared = self.shared.clone();
        async move {
            if shared.closed.load(Ordering::Relaxed) {
                return;
            }
            shared.closed_notify.notified().await;
        }
    }

    /// Returns the peer capabilities received during `_notifyOfPublishedCapabilities:`.
    pub(crate) async fn supported_identifiers(&self) -> Option<Dictionary> {
        match &*self.shared.supported_identifiers.lock().await {
            CapabilityHandshakeState::Received(dict) => Some(dict.clone()),
            CapabilityHandshakeState::Pending | CapabilityHandshakeState::Skipped => None,
        }
    }

    /// Waits for `_notifyOfPublishedCapabilities:` from the remote side.
    pub(crate) async fn wait_for_capabilities(
        &self,
        timeout: std::time::Duration,
    ) -> Result<Dictionary, IdeviceError> {
        crate::time::timeout(timeout, async {
            loop {
                arm_wait!(
                    event,
                    closed,
                    self.shared.handshake_notify.notified(),
                    self.shared.closed_notify.notified()
                );
                match &*self.shared.supported_identifiers.lock().await {
                    CapabilityHandshakeState::Received(dict) => return Ok(dict.clone()),
                    CapabilityHandshakeState::Skipped => {
                        return Err(IdeviceError::UnexpectedResponse(
                            "unexpected response".into(),
                        ));
                    }
                    CapabilityHandshakeState::Pending => {}
                }

                if self.shared.closed.load(Ordering::Relaxed) {
                    return Err(Self::closed_error());
                }

                tokio::select! {
                    _ = &mut event => {}
                    _ = &mut closed => {}
                }
            }
        })
        .await
        .map_err(|_| remote_timeout_error(timeout))?
    }

    /// Performs the DTX capability handshake, mirroring pymobiledevice3's
    /// `DTXConnection._perform_handshake()`.
    pub(crate) async fn perform_handshake(
        &mut self,
        capabilities: Option<Dictionary>,
        timeout: std::time::Duration,
    ) -> Result<Option<Dictionary>, IdeviceError> {
        let already_received = self.supported_identifiers().await;

        {
            let mut state = self.shared.supported_identifiers.lock().await;
            *state = match (capabilities.is_some(), already_received.as_ref()) {
                (false, _) => CapabilityHandshakeState::Skipped,
                (true, Some(dict)) => CapabilityHandshakeState::Received(dict.clone()),
                (true, None) => CapabilityHandshakeState::Pending,
            };
        }

        if let Some(capabilities) = capabilities {
            self.root_channel()
                .call_method(
                    Some("_notifyOfPublishedCapabilities:"),
                    Some(vec![AuxValue::archived_value(plist::Value::Dictionary(
                        capabilities,
                    ))]),
                    false,
                )
                .await?;
        } else {
            return Ok(None);
        }

        if let Some(capabilities) = already_received {
            return Ok(Some(capabilities));
        }

        crate::time::timeout(timeout, async {
            loop {
                arm_wait!(
                    event,
                    closed,
                    self.shared.handshake_notify.notified(),
                    self.shared.closed_notify.notified()
                );
                match &*self.shared.supported_identifiers.lock().await {
                    CapabilityHandshakeState::Received(dict) => return Ok(Some(dict.clone())),
                    CapabilityHandshakeState::Skipped => return Ok(None),
                    CapabilityHandshakeState::Pending => {}
                }

                if self.shared.closed.load(Ordering::Relaxed) {
                    return Err(Self::closed_error());
                }

                tokio::select! {
                    _ = &mut event => {}
                    _ = &mut closed => {}
                }
            }
        })
        .await
        .map_err(|_| remote_timeout_error(timeout))?
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
    /// * `IdeviceError::UnexpectedResponse("unexpected response".into())` unless the
    ///   reply is structurally empty: no payload bytes (decoded or not) and no
    ///   undecodable aux
    /// * a closed-connection `Socket` error if the connection drops first
    /// * Other IO or serialization errors
    #[allow(unreachable_code)]
    pub async fn make_channel<'c>(
        &'c mut self,
        identifier: impl Into<String>,
    ) -> Result<Channel<'c, R>, IdeviceError> {
        let code = self.shared.new_channel.fetch_add(1, Ordering::Relaxed) as i32;
        let identifier = identifier.into();
        self.register_channel_metadata(code, identifier.clone(), false)
            .await;
        self.ensure_channel_registered(code).await;

        let args = vec![
            AuxValue::U32(
                code.try_into()
                    .expect("locally opened channels are positive"),
            ),
            AuxValue::Array(
                ns_keyed_archive::encode::encode_to_bytes(plist::Value::String(identifier))
                    .expect("Failed to encode"),
            ),
        ];

        let reply = self
            .call_method_with_reply(0, Some("_requestChannelWithCode:identifier:"), Some(args))
            .await?;

        if !Self::is_empty_control_reply(&reply) {
            warn!("make_channel: unexpected reply: {:?}", reply);
            return Err(IdeviceError::UnexpectedResponse(
                "unexpected response".into(),
            ));
        }

        self.build_channel(code)
    }

    /// Opens a named service channel.
    ///
    /// This is a service-level alias for `make_channel()` that mirrors the
    /// terminology used by pymobiledevice3's `DTXConnection.open_channel()`.
    pub(crate) async fn open_service_channel<'c>(
        &'c mut self,
        identifier: &str,
    ) -> Result<Channel<'c, R>, IdeviceError> {
        self.make_channel(identifier).await
    }

    /// Opens a `dtxproxy:` channel assembled from local/remote service names.
    ///
    /// Mirrors pymobiledevice3's proxy-channel naming model, where the caller
    /// reasons about the two sub-services and the transport constructs the
    /// wire identifier.
    pub(crate) async fn make_proxy_channel<'c>(
        &'c mut self,
        local_service: &str,
        remote_service: &str,
    ) -> Result<Channel<'c, R>, IdeviceError> {
        self.make_channel(format!("dtxproxy:{local_service}:{remote_service}"))
            .await
    }

    /// Opens a proxied service channel assembled from local/remote service names.
    ///
    /// This is a service-level alias for `make_proxy_channel()` that matches
    /// the "proxy service" terminology used in pymobiledevice3.
    pub(crate) async fn open_proxied_service_channel<'c>(
        &'c mut self,
        local_service: &str,
        remote_service: &str,
    ) -> Result<Channel<'c, R>, IdeviceError> {
        self.make_proxy_channel(local_service, remote_service).await
    }

    fn build_channel<'c>(&'c mut self, code: i32) -> Result<Channel<'c, R>, IdeviceError> {
        Ok(Channel {
            client: self,
            channel: code,
        })
    }

    /// Returns an owned handle for an existing registered channel.
    pub(crate) fn accept_owned_channel(&self, code: i32) -> OwnedChannel<R> {
        OwnedChannel {
            label: self.label.clone(),
            shared: self.shared.clone(),
            channel: code,
        }
    }

    /// Registers an initializer that runs as soon as the remote opens a
    /// matching incoming channel via `_requestChannelWithCode:identifier:`.
    ///
    /// This mirrors pymobiledevice3's service instantiation timing more
    /// closely: the handler is installed before we acknowledge the channel
    /// request, so the channel can start handling inbound invokes
    /// immediately after the peer receives the OK reply.
    pub(crate) async fn register_incoming_channel_initializer<F, Fut>(
        &mut self,
        identifiers: &[&str],
        initializer: F,
    ) where
        F: Fn(OwnedChannel<R>, String) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), IdeviceError>> + Send + 'static,
    {
        let identifiers = identifiers
            .iter()
            .map(|identifier| (*identifier).to_owned())
            .collect();
        let initializer: IncomingChannelInitializer<WriteHalf<R>> =
            Arc::new(move |label, shared, channel, identifier| {
                let owned = OwnedChannel {
                    label,
                    shared,
                    channel,
                };
                Box::pin(initializer(owned, identifier))
            });
        self.shared
            .incoming_channel_registrations
            .lock()
            .await
            .push(IncomingChannelRegistration {
                identifiers,
                initializer,
            });
    }

    async fn register_channel_metadata(&self, code: i32, identifier: String, remote: bool) {
        self.shared.channel_metadata.lock().await.insert(
            code,
            ChannelMetadata {
                code,
                identifier,
                remote,
            },
        );
        self.shared.registry_notify.notify_waiters();
    }

    pub(crate) async fn wait_for_registered_channel_code(
        &self,
        identifiers: &[&str],
        remote: Option<bool>,
        timeout: Option<std::time::Duration>,
    ) -> Result<i32, IdeviceError> {
        let wait_future = async {
            loop {
                arm_wait!(
                    event,
                    closed,
                    self.shared.registry_notify.notified(),
                    self.shared.closed_notify.notified()
                );
                if let Some(code) = self.find_registered_channel_code(identifiers, remote).await {
                    return Ok(code);
                }

                if self.shared.closed.load(Ordering::Relaxed) {
                    return Err(Self::closed_error());
                }

                tokio::select! {
                    _ = &mut event => {}
                    _ = &mut closed => {}
                }
            }
        };

        match timeout {
            Some(timeout) => crate::time::timeout(timeout, wait_future)
                .await
                .map_err(|_| remote_timeout_error(timeout))?,
            None => wait_future.await,
        }
    }

    /// Waits for the code of a service channel matching one of the given identifiers.
    pub(crate) async fn wait_for_service_channel_code(
        &self,
        identifiers: &[&str],
        remote: Option<bool>,
        timeout: Option<std::time::Duration>,
    ) -> Result<i32, IdeviceError> {
        self.wait_for_registered_channel_code(identifiers, remote, timeout)
            .await
    }

    pub(crate) async fn wait_for_proxied_channel_code(
        &self,
        identifiers: &[&str],
        remote_service: bool,
        remote_channel: Option<bool>,
        timeout: Option<std::time::Duration>,
    ) -> Result<i32, IdeviceError> {
        let wait_future = async {
            loop {
                arm_wait!(
                    event,
                    closed,
                    self.shared.registry_notify.notified(),
                    self.shared.closed_notify.notified()
                );
                if let Some(code) = self
                    .find_registered_proxied_channel_code(
                        identifiers,
                        remote_service,
                        remote_channel,
                    )
                    .await
                {
                    return Ok(code);
                }

                if self.shared.closed.load(Ordering::Relaxed) {
                    return Err(Self::closed_error());
                }

                tokio::select! {
                    _ = &mut event => {}
                    _ = &mut closed => {}
                }
            }
        };

        match timeout {
            Some(timeout) => crate::time::timeout(timeout, wait_future)
                .await
                .map_err(|_| remote_timeout_error(timeout))?,
            None => wait_future.await,
        }
    }

    /// Waits for the code of a proxied service channel whose local or remote
    /// sub-service matches one of `identifiers`.
    pub(crate) async fn wait_for_proxied_service_channel_code(
        &self,
        identifiers: &[&str],
        remote_service: bool,
        remote_channel: Option<bool>,
        timeout: Option<std::time::Duration>,
    ) -> Result<i32, IdeviceError> {
        self.wait_for_proxied_channel_code(identifiers, remote_service, remote_channel, timeout)
            .await
    }

    async fn find_registered_channel_code(
        &self,
        identifiers: &[&str],
        remote: Option<bool>,
    ) -> Option<i32> {
        let metadata = self.shared.channel_metadata.lock().await;
        metadata.values().find_map(|entry| {
            let matches_identifier = identifiers.contains(&entry.identifier.as_str());
            let matches_remote = remote.is_none_or(|remote_flag| remote_flag == entry.remote);
            (matches_identifier && matches_remote).then_some(entry.code)
        })
    }

    async fn find_registered_proxied_channel_code(
        &self,
        identifiers: &[&str],
        remote_service: bool,
        remote_channel: Option<bool>,
    ) -> Option<i32> {
        let metadata = self.shared.channel_metadata.lock().await;
        metadata.values().find_map(|entry| {
            let matches_remote_channel =
                remote_channel.is_none_or(|remote_flag| remote_flag == entry.remote);
            if !matches_remote_channel {
                return None;
            }

            let (local_service, remote_service_name) =
                Self::parse_dtxproxy_identifier(&entry.identifier, entry.remote)?;
            let candidate = if remote_service {
                remote_service_name
            } else {
                local_service
            };

            identifiers.contains(&candidate).then_some(entry.code)
        })
    }

    fn parse_dtxproxy_identifier(identifier: &str, remote_channel: bool) -> Option<(&str, &str)> {
        let mut parts = identifier.split(':');
        let prefix = parts.next()?;
        let first = parts.next()?;
        let second = parts.next()?;
        if prefix != "dtxproxy" || parts.next().is_some() {
            return None;
        }

        if remote_channel {
            Some((second, first))
        } else {
            Some((first, second))
        }
    }

    async fn send_method(
        &self,
        channel: i32,
        identifier: u32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
        correlate_reply: bool,
    ) -> Result<Option<oneshot::Receiver<Message>>, IdeviceError> {
        let mheader = MessageHeader::new(0, 1, identifier, 0, channel, expect_reply);
        let pheader = PayloadHeader::method_invocation();
        let aux = args.map(Aux::from_values);
        let data: Option<plist::Value> = data.map(Into::into);

        let message = Message::new(mheader, pheader, aux, data);
        debug!("[{}] Sending message: {message:#?}", self.label);

        let receiver = if correlate_reply {
            let (sender, receiver) = oneshot::channel();
            let mut slots = self.shared.pending_replies.lock().await;
            slots.retain(|_, s| !s.is_closed());
            slots.insert(identifier, sender);
            Some(receiver)
        } else {
            None
        };

        let write_result = self.shared.write_all(&message.serialize()).await;
        if write_result.is_err() {
            self.shared.pending_replies.lock().await.remove(&identifier);
        }
        write_result?;

        Ok(receiver)
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
        channel: i32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        let identifier = self.shared.current_message.fetch_add(1, Ordering::Relaxed) + 1;
        self.send_method(channel, identifier, data, args, expect_reply, false)
            .await?;
        Ok(())
    }

    /// Sends one method invocation on `channel` with `expects_reply` set and
    /// returns its identifier and a [`PendingReply`] for the correlated reply.
    ///
    /// The reply slot is registered before the frame is written, and the frame
    /// is written exactly once; a failed write removes the slot and returns the
    /// I/O error. `selector` is NSKeyedArchive-encoded as the payload; `args`
    /// become the aux section.
    ///
    /// Delivery rules for everything else on the connection: only channels
    /// registered by [`make_channel`](Self::make_channel) (and channel 0) have
    /// queues; messages for an unknown channel are logged and dropped. The
    /// reader normalizes the wire channel by conversation-index parity (negated
    /// when the index is even), so replies land on the request's channel. Calls
    /// the peer makes on the reverse (negative) channel of a locally opened
    /// channel have no queue and are dropped without an acknowledgement.
    ///
    /// # Errors
    /// I/O errors from the write. Panics only if `selector` cannot be
    /// NSKeyedArchive-encoded (the same `expect` as [`call_method`](Self::call_method)).
    pub async fn send_call(
        &self,
        channel: i32,
        selector: impl Into<plist::Value>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<(u32, PendingReply), IdeviceError> {
        self.send_call_inner(channel, Some(selector), args).await
    }

    async fn send_call_inner(
        &self,
        channel: i32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<(u32, PendingReply), IdeviceError> {
        let identifier = self.shared.current_message.fetch_add(1, Ordering::Relaxed) + 1;
        let receiver = self
            .send_method(channel, identifier, data, args, true, true)
            .await?
            .ok_or(IdeviceError::UnexpectedResponse(
                "unexpected response".into(),
            ))?;
        Ok((
            identifier,
            PendingReply {
                identifier,
                receiver,
                slots: Arc::downgrade(&self.shared.pending_replies),
            },
        ))
    }

    /// Calls a method and waits for the reply correlated by message identifier.
    ///
    /// Equivalent to [`send_call`](Self::send_call) followed by awaiting the
    /// [`PendingReply`], except that `data` may be `None` (no payload). Holds
    /// `&mut self` until the reply, so pushes cannot be read meanwhile; use
    /// `send_call` for that. Returns the reply message as received, including
    /// any `aux_error` / `data_error`; errors are I/O or a closed connection.
    pub async fn call_method_with_reply(
        &mut self,
        channel: i32,
        data: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<Message, IdeviceError> {
        let (_, pending) = self.send_call_inner(channel, data, args).await?;
        pending.await
    }

    /// Reads the next message from the specified channel
    ///
    /// Checks cached messages first, then reads from transport if needed.
    ///
    /// Only messages that no reply slot, control handler or incoming handler
    /// consumed reach the queue, and only channel 0 and channels registered via
    /// [`make_channel`](Self::make_channel) have queues; traffic for any other
    /// channel is logged and dropped. The channel is normalized by
    /// conversation-index parity (see [`Message::channel`]). Peer calls on the
    /// reverse (negative) channel of a locally opened channel are dropped
    /// unacknowledged. A message whose aux or payload failed to decode is still
    /// delivered, with `aux_error` / `data_error` set.
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
    pub async fn read_message(&mut self, channel: i32) -> Result<Message, IdeviceError> {
        loop {
            let queue = self
                .get_channel_queue(channel)
                .await
                .ok_or_else(|| DvtError::UnknownChannel(channel.unsigned_abs()))?;

            arm_wait!(
                event,
                closed,
                queue.notify.notified(),
                self.shared.closed_notify.notified()
            );

            {
                let mut messages = queue.messages.lock().await;
                if let Some(msg) = messages.pop_front() {
                    return Ok(msg);
                }
            }

            if self.shared.closed.load(Ordering::Relaxed) {
                return Err(Self::closed_error());
            }

            tokio::select! {
                _ = &mut event => {}
                _ = &mut closed => {}
            }
        }
    }

    fn spawn_reader(
        label: Arc<str>,
        shared: Arc<RemoteServerShared<WriteHalf<R>>>,
        mut reader: ReadHalf<R>,
    ) -> ReaderTask
    where
        R: 'static,
    {
        let fut = async move {
            loop {
                let max = shared.max_message_size.load(Ordering::Relaxed);
                match Message::from_reader_limited(&mut reader, max).await {
                    Ok(msg) => {
                        debug!("[{}] Read message: {msg:#?}", label);
                        if Self::dispatch_pending_reply(&shared, msg.clone()).await {
                            continue;
                        }
                        if Self::handle_control_message(&shared, &msg).await {
                            continue;
                        }
                        if Self::dispatch_to_handler(&shared, msg.clone()).await {
                            continue;
                        }
                        Self::enqueue_message(&shared, msg).await;
                    }
                    Err(e) => {
                        warn!("[{}] RemoteServer reader exiting: {} ({:?})", label, e, e);
                        let eof = matches!(&e, IdeviceError::Socket(io)
                            if io.kind() == std::io::ErrorKind::UnexpectedEof);
                        let reason = if eof {
                            CloseReason::Eof
                        } else {
                            CloseReason::Error(Arc::new(e))
                        };
                        shared.record_close_reason(reason);
                        // Mark closed before failing waiters so a failed waiter
                        // always observes the closed state.
                        shared.closed.store(true, Ordering::Relaxed);
                        Self::fail_pending_replies(&shared).await;
                        shared.closed_notify.notify_waiters();
                        break;
                    }
                }
            }
        };

        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::spawn(fut)
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(fut);
        }
    }

    async fn handle_control_message(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        msg: &Message,
    ) -> bool {
        if msg.message_header.channel != 0 {
            return false;
        }

        match msg.data.as_ref() {
            Some(plist::Value::String(selector))
                if selector == "_notifyOfPublishedCapabilities:" =>
            {
                let aux = match msg.aux.as_ref() {
                    Some(aux) => aux.values.as_slice(),
                    None => {
                        warn!(
                            "Capabilities notification without decodable aux payload: {:?}",
                            msg.aux_error
                        );
                        return true;
                    }
                };

                let Some(first) = aux.first() else {
                    warn!("Capabilities notification missing payload");
                    return true;
                };

                match Self::decode_capabilities(first) {
                    Ok(capabilities) => {
                        debug!("Received remote capabilities: {:?}", capabilities);
                        *shared.supported_identifiers.lock().await =
                            CapabilityHandshakeState::Received(capabilities);
                        shared.handshake_notify.notify_waiters();
                        // Preserve pre-XCTest behavior: older DVT callers expect the
                        // initial capabilities hello to remain observable via
                        // `read_message(0)` on the root channel.
                        Self::enqueue_message(shared, msg.clone()).await;
                    }
                    Err(e) => warn!("Failed to decode remote capabilities: {}", e),
                }
                return true;
            }
            Some(plist::Value::String(selector)) if selector == "_channelCanceled:" => {
                let aux = match msg.aux.as_ref() {
                    Some(aux) => aux.values.as_slice(),
                    None => {
                        warn!("Incoming channel cancellation without aux payload");
                        return true;
                    }
                };

                let Some(first) = aux.first() else {
                    warn!("Incoming channel cancellation missing channel code");
                    return true;
                };

                match Self::decode_channel_code(first) {
                    Ok(channel_code) => {
                        debug!("Remote cancelled channel {}", channel_code);
                        Self::remove_channel(shared, channel_code).await;
                    }
                    Err(e) => warn!("Failed to decode incoming channel cancellation: {}", e),
                }
                return true;
            }
            Some(plist::Value::String(selector))
                if selector == "_requestChannelWithCode:identifier:" => {}
            _ => return false,
        }

        let aux = match msg.aux.as_ref() {
            Some(aux) => aux.values.as_slice(),
            None => {
                warn!("Incoming channel request without aux payload");
                return false;
            }
        };

        if aux.len() < 2 {
            warn!("Incoming channel request missing aux values");
            return false;
        }

        let code = match aux[0] {
            AuxValue::U32(code) => -(code as i32),
            _ => {
                warn!("Incoming channel request aux[0] is not U32");
                return false;
            }
        };

        let identifier = match Self::decode_identifier(&aux[1]) {
            Ok(identifier) => identifier,
            Err(e) => {
                warn!("Failed to decode incoming channel identifier: {}", e);
                return false;
            }
        };

        debug!(
            "Remote requested channel {} with identifier '{}'",
            code, identifier
        );

        shared.channel_metadata.lock().await.insert(
            code,
            ChannelMetadata {
                code,
                identifier: identifier.clone(),
                remote: true,
            },
        );
        shared.registry_notify.notify_waiters();
        Self::ensure_channel_registered_shared(shared, code).await;

        if let Err(error) =
            Self::run_incoming_channel_initializers(shared, code, identifier.clone()).await
        {
            warn!(
                "Failed to initialize incoming channel {} ('{}'): {}",
                code, identifier, error
            );
        }

        if let Err(e) = shared
            .send_raw_reply(
                0,
                msg.message_header.identifier(),
                msg.message_header.conversation_index(),
                &[],
            )
            .await
        {
            warn!("Failed to acknowledge incoming channel request: {}", e);
            shared.record_close_reason(CloseReason::Error(Arc::new(e)));
            shared.closed.store(true, Ordering::Relaxed);
            shared.closed_notify.notify_waiters();
        }

        true
    }

    async fn run_incoming_channel_initializers(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        channel: i32,
        identifier: String,
    ) -> Result<(), IdeviceError> {
        let initializer = {
            let registrations = shared.incoming_channel_registrations.lock().await;
            registrations
                .iter()
                .find(|registration| {
                    registration
                        .identifiers
                        .iter()
                        .any(|candidate| candidate == &identifier)
                })
                .map(|registration| registration.initializer.clone())
        };

        let Some(initializer) = initializer else {
            return Ok(());
        };

        initializer(shared.label.clone(), shared.clone(), channel, identifier).await
    }

    async fn enqueue_message(shared: &Arc<RemoteServerShared<WriteHalf<R>>>, msg: Message) {
        if msg.message_header.conversation_index() == 0 {
            debug!(
                "Queueing unhandled incoming message on channel {} expects_reply={} data={:?}",
                msg.message_header.channel,
                msg.message_header.expects_reply(),
                msg.data
            );
        }
        if let Some(queue) =
            Self::get_channel_queue_shared(shared, msg.message_header.channel).await
        {
            let notify = &queue.notify;
            {
                let mut messages = queue.messages.lock().await;
                messages.push_back(msg);
            }
            notify.notify_waiters();
        } else {
            warn!(
                "Received message for unknown channel: {}",
                msg.message_header.channel
            );
        }
    }

    async fn dispatch_to_handler(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        msg: Message,
    ) -> bool {
        if msg.message_header.conversation_index() != 0 {
            return false;
        }

        let handler = {
            let handlers = shared.handlers.lock().await;
            handlers.get(&msg.message_header.channel).cloned()
        };

        let Some(handler) = handler else {
            return false;
        };

        let expects_reply = msg.message_header.expects_reply();
        let msg_id = msg.message_header.identifier();
        let conversation_index = msg.message_header.conversation_index();
        let channel = msg.message_header.channel;

        match handler(msg).await {
            Ok(IncomingHandlerOutcome::Unhandled) => false,
            Ok(IncomingHandlerOutcome::HandledNoReply) => {
                if expects_reply
                    && let Err(e) = shared
                        .send_raw_reply(channel, msg_id, conversation_index, &[])
                        .await
                {
                    warn!("Failed to auto-ack handled incoming message: {}", e);
                }
                true
            }
            Ok(IncomingHandlerOutcome::Reply(reply_bytes)) => {
                if let Err(e) = shared
                    .send_raw_reply(channel, msg_id, conversation_index, &reply_bytes)
                    .await
                {
                    warn!("Failed to reply from incoming handler: {}", e);
                }
                true
            }
            Err(e) => {
                warn!("Incoming message handler failed: {}", e);
                false
            }
        }
    }

    async fn dispatch_pending_reply(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        msg: Message,
    ) -> bool {
        if msg.message_header.conversation_index() == 0 {
            return false;
        }

        let pending = shared
            .pending_replies
            .lock()
            .await
            .remove(&msg.message_header.identifier());

        let Some(sender) = pending else {
            return false;
        };

        // A waiter dropped while its slot was locked leaves a closed sender;
        // the reply then falls through to the channel queue like any message.
        if sender.send(msg).is_err() {
            warn!("Reply waiter dropped before correlated reply was delivered");
            return false;
        }

        true
    }

    async fn ensure_channel_registered(&self, code: i32) {
        Self::ensure_channel_registered_shared(&self.shared, code).await;
    }

    async fn ensure_channel_registered_shared(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        code: i32,
    ) {
        let mut channels = shared.channels.lock().await;
        channels
            .entry(code)
            .or_insert_with(|| Arc::new(ChannelQueue::default()));
    }

    async fn get_channel_queue(&self, code: i32) -> Option<Arc<ChannelQueue>> {
        Self::get_channel_queue_shared(&self.shared, code).await
    }

    async fn get_channel_queue_shared(
        shared: &Arc<RemoteServerShared<WriteHalf<R>>>,
        code: i32,
    ) -> Option<Arc<ChannelQueue>> {
        let channels = shared.channels.lock().await;
        channels.get(&code).cloned()
    }

    fn decode_identifier(aux: &AuxValue) -> Result<String, IdeviceError> {
        match aux {
            AuxValue::String(s) => Ok(s.clone()),
            AuxValue::Array(bytes) => {
                match ns_keyed_archive::decode::from_bytes(bytes).map_err(DvtError::from)? {
                    plist::Value::String(s) => Ok(s),
                    _ => Err(IdeviceError::UnexpectedResponse(
                        "unexpected response".into(),
                    )),
                }
            }
            _ => Err(IdeviceError::UnexpectedResponse(
                "unexpected response".into(),
            )),
        }
    }

    fn decode_capabilities(aux: &AuxValue) -> Result<Dictionary, IdeviceError> {
        match aux {
            AuxValue::Array(bytes) => {
                match ns_keyed_archive::decode::from_bytes(bytes).map_err(DvtError::from)? {
                    plist::Value::Dictionary(dict) => Ok(dict),
                    _ => Err(IdeviceError::UnexpectedResponse(
                        "unexpected response".into(),
                    )),
                }
            }
            _ => Err(IdeviceError::UnexpectedResponse(
                "unexpected response".into(),
            )),
        }
    }

    fn decode_channel_code(aux: &AuxValue) -> Result<i32, IdeviceError> {
        match aux {
            AuxValue::U32(code) => i32::try_from(*code)
                .map_err(|_| IdeviceError::UnexpectedResponse("unexpected response".into())),
            AuxValue::I64(code) => i32::try_from(*code)
                .map_err(|_| IdeviceError::UnexpectedResponse("unexpected response".into())),
            _ => Err(IdeviceError::UnexpectedResponse(
                "unexpected response".into(),
            )),
        }
    }

    async fn remove_channel(shared: &Arc<RemoteServerShared<WriteHalf<R>>>, channel_code: i32) {
        shared.handlers.lock().await.remove(&channel_code);
        shared.channels.lock().await.remove(&channel_code);
        shared.channel_metadata.lock().await.remove(&channel_code);
        shared.registry_notify.notify_waiters();
    }

    async fn fail_pending_replies(shared: &Arc<RemoteServerShared<WriteHalf<R>>>) {
        shared.pending_replies.lock().await.clear();
    }

    fn closed_error() -> IdeviceError {
        closed_error()
    }

    /// Why the connection closed, once it has; `None` while the reader runs.
    pub fn close_reason(&self) -> Option<CloseReason> {
        self.shared
            .close_reason
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// A control reply is admitted only when it carries no payload bytes
    /// (decoded or not) and no undecodable aux.
    fn is_empty_control_reply(reply: &Message) -> bool {
        reply.data.is_none()
            && reply.raw_data.is_none()
            && reply.data_error.is_none()
            && reply.aux_error.is_none()
    }
}

impl RemoteServerClient<Box<dyn ReadWrite>> {
    /// Creates a new RemoteServerClient with the given transport.
    pub fn new(idevice: impl ReadWrite + 'static) -> Self {
        Self::with_label(idevice, "remote-server")
    }

    /// Creates a new client with a debug label used in tracing output.
    pub fn with_label(idevice: impl ReadWrite + 'static, label: impl Into<String>) -> Self {
        Self::with_label_typed(Box::new(idevice), label)
    }
}

impl<R: ReadWrite> Drop for RemoteServerClient<R> {
    fn drop(&mut self) {
        // No JoinHandle::abort on wasm32
        #[cfg(not(target_arch = "wasm32"))]
        self.reader_task.abort();
        // The reader no longer runs, so nothing would ever complete a pending
        // reply or wake a queue reader; fail them now. Owned channels and
        // `PendingReply` handles may outlive the client.
        self.shared.record_close_reason(CloseReason::Local);
        self.shared.closed.store(true, Ordering::Relaxed);
        if let Ok(mut slots) = self.shared.pending_replies.try_lock() {
            slots.clear();
        }
        self.shared.closed_notify.notify_waiters();
    }
}

impl<W: tokio::io::AsyncWrite + Unpin> RemoteServerShared<W> {
    async fn write_all(&self, bytes: &[u8]) -> Result<(), IdeviceError> {
        let mut writer = self.writer.lock().await;
        writer.write_all(bytes).await?;
        writer.flush().await?;
        Ok(())
    }

    async fn send_raw_reply(
        &self,
        channel: i32,
        incoming_msg_id: u32,
        incoming_conversation_index: u32,
        data_bytes: &[u8],
    ) -> Result<(), IdeviceError> {
        let buf = Message::build_raw_reply(
            channel,
            incoming_msg_id,
            incoming_conversation_index,
            data_bytes,
        );
        self.write_all(&buf).await
    }
}

impl<R: ReadWrite> Channel<'_, R> {
    /// Converts this borrowed channel handle into an owned/shared one.
    pub(crate) fn detach(&self) -> OwnedChannel<R> {
        OwnedChannel {
            label: self.client.label.clone(),
            shared: self.client.shared.clone(),
            channel: self.channel,
        }
    }

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

    /// Calls a method on this channel and waits for the correlated reply.
    ///
    /// See [`RemoteServerClient::call_method_with_reply`].
    pub async fn call_method_with_reply(
        &mut self,
        method: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<Message, IdeviceError> {
        self.client
            .call_method_with_reply(self.channel, method, args)
            .await
    }

    /// Sends `selector` with `args` once on this channel, expecting a reply,
    /// and returns the message identifier and an awaitable [`PendingReply`]
    /// that does not borrow the channel, so [`read_message`](Self::read_message)
    /// can run concurrently for pushes.
    ///
    /// See [`RemoteServerClient::send_call`] for delivery rules and errors.
    pub async fn send_call(
        &mut self,
        selector: impl Into<plist::Value>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<(u32, PendingReply), IdeviceError> {
        self.client.send_call(self.channel, selector, args).await
    }

    /// Returns this channel's code.
    pub fn code(&self) -> i32 {
        self.channel
    }
}

impl<R: ReadWrite + 'static> OwnedChannel<R> {
    /// Reads the next queued message from this channel.
    pub async fn read_message(&mut self) -> Result<Message, IdeviceError> {
        loop {
            let queue =
                RemoteServerClient::<R>::get_channel_queue_shared(&self.shared, self.channel)
                    .await
                    .ok_or_else(|| DvtError::UnknownChannel(self.channel.unsigned_abs()))?;

            arm_wait!(
                event,
                closed,
                queue.notify.notified(),
                self.shared.closed_notify.notified()
            );

            {
                let mut messages = queue.messages.lock().await;
                if let Some(msg) = messages.pop_front() {
                    return Ok(msg);
                }
            }

            if self.shared.closed.load(Ordering::Relaxed) {
                return Err(RemoteServerClient::<R>::closed_error());
            }

            tokio::select! {
                _ = &mut event => {}
                _ = &mut closed => {}
            }
        }
    }

    /// Reads the next queued message with a timeout.
    pub(crate) async fn read_message_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<Message, IdeviceError> {
        crate::time::timeout(timeout, self.read_message())
            .await
            .map_err(|_| remote_timeout_error(timeout))?
    }

    /// Calls a method on this channel.
    pub async fn call_method(
        &mut self,
        method: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        let identifier = self.shared.current_message.fetch_add(1, Ordering::Relaxed) + 1;
        let mheader = MessageHeader::new(0, 1, identifier, 0, self.channel, expect_reply);
        let pheader = PayloadHeader::method_invocation();
        let aux = args.map(Aux::from_values);
        let data: Option<plist::Value> = method.map(Into::into);
        let message = Message::new(mheader, pheader, aux, data);
        debug!("[{}] Sending message: {message:#?}", self.label);

        self.shared.write_all(&message.serialize()).await?;

        Ok(())
    }

    /// Calls a method on this channel and waits for the correlated reply.
    pub(crate) async fn call_method_with_reply(
        &mut self,
        method: Option<impl Into<plist::Value>>,
        args: Option<Vec<AuxValue>>,
    ) -> Result<Message, IdeviceError> {
        let identifier = self.shared.current_message.fetch_add(1, Ordering::Relaxed) + 1;
        let mheader = MessageHeader::new(0, 1, identifier, 0, self.channel, true);
        let pheader = PayloadHeader::method_invocation();
        let aux = args.map(Aux::from_values);
        let data: Option<plist::Value> = method.map(Into::into);
        let message = Message::new(mheader, pheader, aux, data);
        debug!("[{}] Sending message: {message:#?}", self.label);

        let (sender, receiver) = oneshot::channel::<Message>();
        self.shared
            .pending_replies
            .lock()
            .await
            .insert(identifier, sender);

        let write_result = self.shared.write_all(&message.serialize()).await;
        if write_result.is_err() {
            self.shared.pending_replies.lock().await.remove(&identifier);
        }
        write_result?;

        match receiver.await {
            Ok(message) => Ok(message),
            Err(_) => {
                self.shared.pending_replies.lock().await.remove(&identifier);
                if self.shared.closed.load(Ordering::Relaxed) {
                    Err(RemoteServerClient::<R>::closed_error())
                } else {
                    Err(IdeviceError::UnexpectedResponse(
                        "unexpected response".into(),
                    ))
                }
            }
        }
    }

    /// Registers an incoming handler for this channel.
    pub(crate) async fn set_incoming_handler<F, Fut>(&mut self, handler: F)
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<IncomingHandlerOutcome, IdeviceError>> + Send + 'static,
    {
        let handler: IncomingMessageHandler = Arc::new(move |msg| Box::pin(handler(msg)));
        self.shared
            .handlers
            .lock()
            .await
            .insert(self.channel, handler);
    }

    /// Removes the incoming handler for this channel.
    pub(crate) async fn clear_incoming_handler(&mut self) {
        self.shared.handlers.lock().await.remove(&self.channel);
    }

    /// Sends a raw reply for an incoming message on this channel.
    pub(crate) async fn send_raw_reply_for(
        &mut self,
        incoming_msg_id: u32,
        incoming_conversation_index: u32,
        data_bytes: &[u8],
    ) -> Result<(), IdeviceError> {
        self.shared
            .send_raw_reply(
                self.channel,
                incoming_msg_id,
                incoming_conversation_index,
                data_bytes,
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dvt::message::test_frames::*;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    fn client() -> (RemoteServerClient<Box<dyn ReadWrite>>, DuplexStream) {
        let (ours, peer) = tokio::io::duplex(1 << 20);
        (RemoteServerClient::new(ours), peer)
    }

    /// Reads one frame from the peer side and answers it with `reply_body`
    /// (conversation index 1, same identifier and channel).
    async fn answer(peer: &mut DuplexStream, reply_body: Vec<u8>) -> Message {
        let req = Message::from_reader(peer).await.unwrap();
        let reply = single(req.identifier(), 1, req.channel(), &reply_body);
        peer.write_all(&reply).await.unwrap();
        req
    }

    #[tokio::test]
    async fn make_channel_accepts_an_empty_reply() {
        let (mut client, mut peer) = client();
        let peer_task = tokio::spawn(async move {
            let req = answer(&mut peer, body(0, &[], &[])).await;
            (req, peer)
        });
        let ch = client.make_channel("com.example.svc").await.unwrap();
        assert_eq!(ch.code(), 1);
        let (req, _peer) = peer_task.await.unwrap();
        assert_eq!(
            req.data,
            Some(plist::Value::String(
                "_requestChannelWithCode:identifier:".into()
            ))
        );
    }

    #[tokio::test]
    async fn make_channel_rejects_a_reply_with_undecodable_payload() {
        let (mut client, mut peer) = client();
        let peer_task = tokio::spawn(async move {
            answer(&mut peer, body(3, &[], b"not an archive")).await;
            peer
        });
        let err = client.make_channel("com.example.svc").await.unwrap_err();
        assert!(
            matches!(err, IdeviceError::UnexpectedResponse(_)),
            "{err:?}"
        );
        let _peer = peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn make_channel_rejects_a_reply_with_undecodable_aux() {
        let (mut client, mut peer) = client();
        let mut aux = vec![0u8; 16];
        aux.extend_from_slice(&0x77u32.to_le_bytes());
        let peer_task = tokio::spawn(async move {
            answer(&mut peer, body(0, &aux, &[])).await;
            peer
        });
        let err = client.make_channel("com.example.svc").await.unwrap_err();
        assert!(
            matches!(err, IdeviceError::UnexpectedResponse(_)),
            "{err:?}"
        );
        let _peer = peer_task.await.unwrap();
    }

    #[tokio::test]
    async fn send_call_writes_one_frame_and_reply_resolves_beside_a_push() {
        let (mut client, mut peer) = client();
        let mut ch = client.root_channel();
        let (id, mut pending) = ch.send_call("doThing:", None).await.unwrap();
        assert_eq!(pending.identifier(), id);

        let req = Message::from_reader(&mut peer).await.unwrap();
        assert_eq!(req.identifier(), id);
        assert!(req.expects_reply());
        assert_eq!(req.data, Some(plist::Value::String("doThing:".into())));
        // Exactly one frame: nothing else is readable.
        let mut more = [0u8; 1];
        assert!(
            tokio::time::timeout(Duration::from_millis(100), peer.read_exact(&mut more))
                .await
                .is_err()
        );

        // An unsolicited push on the same channel, then the reply.
        peer.write_all(&single(900, 0, 0, &body(2, &[], &archive("push"))))
            .await
            .unwrap();
        peer.write_all(&single(id, 1, 0, &body(3, &[], &archive("result"))))
            .await
            .unwrap();

        let (mut reply, mut push) = (None, None);
        while reply.is_none() || push.is_none() {
            tokio::select! {
                r = &mut pending, if reply.is_none() => reply = Some(r.unwrap()),
                m = ch.read_message(), if push.is_none() => push = Some(m.unwrap()),
            }
        }
        let (reply, push) = (reply.unwrap(), push.unwrap());
        assert_eq!(reply.identifier(), id);
        assert_eq!(reply.data, Some(plist::Value::String("result".into())));
        assert_eq!(push.identifier(), 900);
        assert_eq!(push.data, Some(plist::Value::String("push".into())));
    }

    /// A message queued just before the peer hangs up is delivered before the
    /// closed error, whichever notifier wakes the waiter first.
    #[tokio::test]
    async fn message_queued_before_close_is_delivered() {
        for _ in 0..64 {
            let (mut client, mut peer) = client();
            let write = async {
                // Let read_message park first, so both wakeups race.
                tokio::task::yield_now().await;
                peer.write_all(&single(7, 0, 0, &body(2, &[], &archive("last"))))
                    .await
                    .unwrap();
                drop(peer);
            };
            let (msg, ()) = tokio::join!(client.read_message(0), write);
            assert_eq!(msg.unwrap().data, Some(plist::Value::String("last".into())));
            assert!(client.read_message(0).await.is_err());
            assert!(matches!(client.close_reason(), Some(CloseReason::Eof)));
        }
    }

    #[tokio::test]
    async fn framing_error_is_the_close_reason() {
        let (mut client, mut peer) = client();
        peer.write_all(&[0u8; 32]).await.unwrap();
        assert!(client.read_message(0).await.is_err());
        assert!(matches!(client.close_reason(), Some(CloseReason::Error(_))));
        drop(client);
    }

    #[tokio::test]
    async fn dropped_pending_reply_frees_its_slot_and_late_reply_is_queued() {
        let (mut client, mut peer) = client();
        let (id, pending) = client.send_call(0, "doThing:", None).await.unwrap();
        assert_eq!(client.shared.pending_replies.lock().await.len(), 1);
        drop(pending);
        assert!(client.shared.pending_replies.lock().await.is_empty());

        Message::from_reader(&mut peer).await.unwrap();
        peer.write_all(&single(id, 1, 0, &body(3, &[], &archive("late"))))
            .await
            .unwrap();
        let late = client.read_message(0).await.unwrap();
        assert_eq!(late.identifier(), id);
    }

    #[tokio::test]
    async fn oversized_message_closes_the_connection_and_fails_waiters() {
        let (client, mut peer) = client();
        client.set_max_message_size(64);
        let (_, pending) = client.send_call(0, "doThing:", None).await.unwrap();
        Message::from_reader(&mut peer).await.unwrap();
        peer.write_all(&single(1, 1, 0, &body(3, &[], &[0u8; 100])))
            .await
            .unwrap();
        let err = tokio::time::timeout(Duration::from_secs(5), pending)
            .await
            .expect("waiter woken")
            .unwrap_err();
        assert!(
            matches!(err, IdeviceError::Socket(ref e) if e.kind() == std::io::ErrorKind::BrokenPipe),
            "{err:?}"
        );
        assert!(client.shared.closed.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn undecodable_user_reply_is_delivered_not_fatal() {
        let (mut client, mut peer) = client();
        let peer_task = tokio::spawn(async move {
            answer(&mut peer, body(3, &[], b"junk")).await;
            peer
        });
        let reply = client
            .call_method_with_reply(0, Some("doThing:"), None)
            .await
            .unwrap();
        assert!(reply.data.is_none() && reply.data_error.is_some());
        assert_eq!(reply.raw_data.as_deref(), Some(&b"junk"[..]));
        assert!(!client.shared.closed.load(Ordering::Relaxed));
        let _peer = peer_task.await.unwrap();
    }
}
