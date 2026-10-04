// Jackson Coxson

use async_stream::try_stream;
use futures::Stream;
use http2::Setting;
use tracing::debug;

use crate::{IdeviceError, ReadWrite, xpc};

pub mod errors;
mod format;
mod http2;
pub mod xpc_macro;

use errors::XpcError;
use format::XPCFlag;
pub use format::{DEFAULT_MAX_MESSAGE_SIZE, Dictionary, MAX_NESTING_DEPTH, XPCMessage, XPCObject};

const ROOT_CHANNEL: u32 = 1;
const REPLY_CHANNEL: u32 = 3;
/// First stream ID available for an outbound file transfer. Client-initiated
/// streams must be odd, and 1/3 are taken by the root and reply channels.
const FIRST_OUTBOUND_FILE_STREAM: u32 = 5;

/// One XPC wrapper as received, unfiltered.
#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    /// HTTP/2 stream it arrived on: 1 is the root channel, 3 the reply channel.
    pub channel: u32,
    /// The wrapper's message id.
    pub message_id: u64,
    /// Raw wrapper flags (0x1 always set, 0x100 data, 0x10000 wants reply,
    /// 0x20000 reply, 0x400000 init handshake).
    pub flags: u32,
    pub body: Body,
}

/// An envelope's payload, as needed to tell control wrappers from messages.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// Any object other than an empty dictionary.
    Object(XPCObject),
    /// An empty dictionary (keepalive / handshake shape).
    Empty,
    /// A wrapper with no body at all.
    None,
}

#[derive(Debug)]
pub struct RemoteXpcClient<R: ReadWrite> {
    h2_client: http2::Http2Client<R>,
    root_id: u64,
    // reply_id: u64 // maybe not used?
    /// Per-channel bytes accumulated toward the next whole XPC message. Persisted
    /// across `recv_from_channel` calls so a partially-received message survives a
    /// cancelled read.
    partial: std::collections::HashMap<u32, Vec<u8>>,
    /// Stream ID for the next outbound file transfer.
    next_outbound_file_stream: u32,
    /// Cap on one wrapper (header plus declared body), in bytes.
    max_message_size: usize,
}

impl<R: ReadWrite> RemoteXpcClient<R> {
    pub async fn new(socket: R) -> Result<Self, IdeviceError> {
        Ok(Self {
            h2_client: http2::Http2Client::new(socket).await?,
            root_id: 1,
            partial: std::collections::HashMap::new(),
            next_outbound_file_stream: FIRST_OUTBOUND_FILE_STREAM,
            max_message_size: DEFAULT_MAX_MESSAGE_SIZE,
        })
    }

    /// Sets the largest wrapper (24-byte header plus declared body) this client
    /// accepts, in bytes; default [`DEFAULT_MAX_MESSAGE_SIZE`] (16 MiB). A
    /// wrapper declaring more fails with [`XpcError::MessageTooLarge`] as soon
    /// as its header is buffered, before its body is waited for or allocated;
    /// nested lengths are checked against the same cap.
    pub fn set_max_message_size(&mut self, bytes: usize) {
        self.max_message_size = bytes;
    }

    /// When `strict` is true, DATA on an HTTP/2 stream this client never opened
    /// fails the read with [`XpcError::UnexpectedStream`] instead of being
    /// cached unboundedly. That is how a file-transfer side channel the peer
    /// starts on its own becomes visible before its bytes accumulate. The root
    /// and reply streams (1 and 3) and streams opened by
    /// [`Self::open_file_stream_for_response`] or [`Self::send_file_transfer`]
    /// are always admitted. The offending frame is consumed without a window
    /// update, so the connection should be dropped afterwards. Default false.
    pub fn set_strict_streams(&mut self, strict: bool) {
        self.h2_client.set_strict_streams(strict);
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

        debug!("Opening reply stream");
        self.h2_client.open_stream(REPLY_CHANNEL).await?;
        self.send_reply(XPCMessage::new(
            Some(XPCFlag::InitHandshake | XPCFlag::AlwaysSet),
            None,
            None,
        ))
        .await?;

        debug!("Sending weird flags");
        self.send_root(XPCMessage::new(Some(XPCFlag::Custom(0x201)), None, None))
            .await?;

        Ok(())
    }

    /// Announce ourselves to the device's `remoted` as a modern (non-legacy)
    /// RemoteXPC peer.
    ///
    /// Send this only on the RSD/remoted control connection
    pub async fn send_device_handshake(&mut self) -> Result<(), IdeviceError> {
        const REMOTE_XPC_VERSION_FLAGS: u64 = 0x0100_0000_0000_0006;

        let msg = xpc!({
            "MessageType": "Handshake",
            "MessagingProtocolVersion": 7u64,
            "UUID": uuid::Uuid::new_v4(),
            "Properties": {
                "RemoteXPCVersionFlags": REMOTE_XPC_VERSION_FLAGS,
                "SensitivePropertiesVisible": true,
            },
            "Services": XPCObject::Dictionary(Dictionary::new())
        });

        self.send_object(msg, false).await
    }

    /// Next application message on the reply channel; skips bodyless wrappers
    /// and empty dictionaries. Errors as [`Self::recv_envelope`].
    pub async fn recv(&mut self) -> Result<plist::Value, IdeviceError> {
        self.recv_message(&[REPLY_CHANNEL]).await
    }

    /// Next application message on the root channel; filtering as [`Self::recv`].
    pub async fn recv_root(&mut self) -> Result<plist::Value, IdeviceError> {
        self.recv_message(&[ROOT_CHANNEL]).await
    }

    /// Next application message on either channel (root checked first);
    /// filtering as [`Self::recv`].
    pub async fn recv_any(&mut self) -> Result<plist::Value, IdeviceError> {
        self.recv_message(&[ROOT_CHANNEL, REPLY_CHANNEL]).await
    }

    async fn recv_message(&mut self, channels: &[u32]) -> Result<plist::Value, IdeviceError> {
        loop {
            if let Body::Object(o) = self.recv_envelope(channels).await?.body {
                return Ok(o.to_plist());
            }
        }
    }

    /// Returns the next whole wrapper on any of `channels` (1 = root,
    /// 3 = reply), with no filtering: control wrappers come back as
    /// [`Body::Empty`] / [`Body::None`].
    ///
    /// Every whole wrapper already buffered on the listed channels is returned
    /// before any socket read, so several wrappers coalesced into one read come
    /// back from consecutive calls without waiting. Wrappers on channels not
    /// listed stay buffered for a later call.
    ///
    /// Errors: [`XpcError::StreamEnded`] once a listed channel's peer half is
    /// ended and drained (returned on every call until the caller stops listing
    /// it; the other channel stays readable); [`XpcError::Truncated`] when the
    /// stream ends or TCP closes with part of a wrapper (`Some(channel)`) or an
    /// HTTP/2 frame (`None`) pending; [`XpcError::ConnectionClosed`] on a clean
    /// TCP EOF; [`XpcError::Reset`] / [`XpcError::GoAway`];
    /// [`XpcError::MessageTooLarge`] and the decode errors of
    /// [`XPCMessage::decode_with_limit`]; [`XpcError::UnexpectedStream`] in
    /// strict-streams mode.
    ///
    /// Cancel-safe: partial bytes persist in the client. Per call O(c + n)
    /// for c channels and n bytes decoded; buffered bytes per channel are at
    /// most one wrapper (≤ the max message size) plus one HTTP/2 frame.
    pub async fn recv_envelope(&mut self, channels: &[u32]) -> Result<Envelope, IdeviceError> {
        loop {
            for &channel in channels {
                if let Some(msg) = self.take_buffered(channel)? {
                    return Ok(Envelope {
                        channel,
                        message_id: msg.message_id.unwrap_or_default(),
                        flags: msg.flags,
                        body: match msg.message {
                            None => Body::None,
                            Some(XPCObject::Dictionary(d)) if d.is_empty() => Body::Empty,
                            Some(o) => Body::Object(o),
                        },
                    });
                }
            }
            match self.h2_client.read_any(channels).await {
                Ok((channel, chunk)) => self.partial.entry(channel).or_default().extend(chunk),
                Err(e) => return Err(self.classify_end(e, channels)),
            }
        }
    }

    /// Promotes a clean end into `Truncated` when the affected channel still
    /// holds part of a wrapper.
    fn classify_end(&self, e: IdeviceError, channels: &[u32]) -> IdeviceError {
        let pending = |ch: u32| self.partial.get(&ch).map_or(0, |b| b.len());
        let truncated = |ch: u32| {
            XpcError::Truncated {
                stream_id: Some(ch),
                buffered: pending(ch),
            }
            .into()
        };
        match e {
            IdeviceError::Xpc(XpcError::StreamEnded { stream_id }) if pending(stream_id) > 0 => {
                truncated(stream_id)
            }
            IdeviceError::Xpc(XpcError::ConnectionClosed) => {
                match channels.iter().copied().find(|&c| pending(c) > 0) {
                    Some(c) => truncated(c),
                    None => e,
                }
            }
            e => e,
        }
    }

    /// Decodes one whole message out of `channel`'s buffered bytes, if there is
    /// one, consuming exactly the bytes it occupies so the next message in the
    /// buffer survives. Returns `None` when the buffer doesn't hold a whole
    /// message yet; an over-cap header fails here, before the body arrives.
    fn take_buffered(&mut self, channel: u32) -> Result<Option<XPCMessage>, IdeviceError> {
        let buf = self.partial.entry(channel).or_default();
        match XPCMessage::decode_prefix(buf, self.max_message_size)? {
            Some((msg, consumed)) => {
                buf.drain(..consumed);
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Sends `msg` on the root channel; see [`Self::send_object_with_id`].
    pub async fn send_object(
        &mut self,
        msg: impl Into<XPCObject>,
        expect_reply: bool,
    ) -> Result<(), IdeviceError> {
        self.send_object_with_id(msg, expect_reply)
            .await
            .map(|_| ())
    }

    /// Sends `msg` on the root channel with the data flag (plus wants-reply
    /// when `expect_reply`) and returns the message id it carried. Ids start at
    /// 1 and advance by one per message sent through this method, matching
    /// pymobiledevice3. Fails with [`XpcError::DateOutOfRange`] for an
    /// unencodable `Date`, before anything is written, and with transport
    /// errors from the write; the id advances only after a successful write.
    pub async fn send_object_with_id(
        &mut self,
        msg: impl Into<XPCObject>,
        expect_reply: bool,
    ) -> Result<u64, IdeviceError> {
        let mut flag = XPCFlag::DataFlag | XPCFlag::AlwaysSet;
        if expect_reply {
            flag |= XPCFlag::WantingReply;
        }
        let id = self.root_id;
        let msg = XPCMessage::new(Some(flag), Some(msg.into()), Some(id));
        self.send_root(msg).await?;
        self.root_id += 1;
        Ok(id)
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

    pub fn iter_file_chunks<'a>(
        &'a mut self,
        total_size: usize,
        file_idx: u32,
    ) -> impl Stream<Item = Result<Vec<u8>, IdeviceError>> + 'a {
        let stream_id = (file_idx + 1) * 2;

        try_stream! {
            fn strip_xpc_wrapper_prefix(buf: &[u8]) -> (&[u8], bool) {
                // Returns (data_after_wrapper, stripped_anything)
                const MAGIC: u32 = 0x29b00b92;

                if buf.len() < 24 {
                    return (buf, false);
                }

                let magic = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
                if magic != MAGIC {
                    return (buf, false);
                }

                // flags at [4..8] – not needed to compute size
                let body_len = u64::from_le_bytes([
                    buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
                ]) as usize;

                let wrapper_len = 24 + body_len;
                if buf.len() < wrapper_len {
                    // Incomplete wrapper (shouldn’t happen with your read API), keep as-is.
                    return (buf, false);
                }

                (&buf[wrapper_len..], true)
            }
            self.open_file_stream_for_response(stream_id).await?;

            let mut got = 0usize;
            while got < total_size {
                let bytes = self.h2_client.read(stream_id).await?;
                let (after, stripped) = strip_xpc_wrapper_prefix(&bytes);
                if stripped && after.is_empty() {
                    continue; // pure control wrapper, don't count
                }

                let data = if stripped { after.to_vec() } else { bytes };

                if data.is_empty() {
                    continue;
                }

                got += data.len();
                yield data;
            }
        }
    }

    /// Pushes the payload of a file transfer we announced in an earlier request.
    pub async fn send_file_transfer(
        &mut self,
        transfer_id: u64,
        data: &[u8],
    ) -> Result<(), IdeviceError> {
        let stream_id = self.next_outbound_file_stream;
        self.next_outbound_file_stream += 2;

        self.h2_client.open_stream(stream_id).await?;

        // The preamble is a DATA frame like any other, so it has to go through
        // the flow-controlled send path: sending it unaccounted overruns the
        // connection window once an earlier transfer has drained it, and the
        // device answers with GOAWAY (FLOW_CONTROL_ERROR).
        let preamble = XPCMessage::new(
            Some(XPCFlag::FileTxStreamRequest | XPCFlag::AlwaysSet),
            None,
            Some(transfer_id),
        )
        .encode(transfer_id)?;
        self.h2_client.send(preamble, stream_id).await?;
        self.h2_client.send(data.to_vec(), stream_id).await?;
        self.h2_client
            .send_end_stream(Vec::new(), stream_id)
            .await?;
        Ok(())
    }

    pub async fn open_file_stream_for_response(
        &mut self,
        stream_id: u32,
    ) -> Result<(), IdeviceError> {
        // 1) Open the HTTP/2 stream
        self.h2_client.open_stream(stream_id).await?;

        // 2) Send an empty XPC wrapper on that same stream with FILE_TX_STREAM_RESPONSE
        let flags = XPCFlag::AlwaysSet | XPCFlag::FileTxStreamResponse;

        let msg = XPCMessage::new(Some(flags), None, Some(0));

        // IMPORTANT: send on `stream_id`, not ROOT/REPLY
        let bytes = msg.encode(0)?;
        self.h2_client.send(bytes, stream_id).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Synthetic-peer tests: the peer half of an in-memory duplex writes raw
    //! HTTP/2 frames; no device or network.
    use super::*;
    use http2::frame::{DataFrame, Frame, HttpFrame};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    async fn client() -> (RemoteXpcClient<DuplexStream>, DuplexStream) {
        let (ours, peer) = tokio::io::duplex(1 << 20);
        (RemoteXpcClient::new(ours).await.unwrap(), peer)
    }

    fn wrapper(flags: u32, id: u64, body: Option<XPCObject>) -> Vec<u8> {
        XPCMessage {
            flags,
            message: body,
            message_id: Some(id),
        }
        .encode(id)
        .unwrap()
    }

    fn data(stream_id: u32, payload: Vec<u8>, end_stream: bool) -> Vec<u8> {
        DataFrame {
            stream_id,
            payload,
            end_stream,
        }
        .serialize()
    }

    fn obj(k: &str) -> XPCObject {
        crate::xpc!({ "k": k })
    }

    fn keepalive() -> Vec<u8> {
        wrapper(1, 0, Some(XPCObject::Dictionary(Dictionary::new())))
    }

    /// Fails the test instead of hanging when a call waits for bytes that the
    /// peer will never send.
    async fn soon<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(2), f)
            .await
            .expect("call waited for input that was already buffered")
    }

    fn xpc_err(r: Result<impl std::fmt::Debug, IdeviceError>) -> XpcError {
        match r {
            Err(IdeviceError::Xpc(e)) => e,
            other => panic!("expected an XpcError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn coalesced_keepalive_and_data_drain_without_waiting() {
        let (mut c, mut peer) = client().await;
        let mut payload = keepalive();
        payload.extend(wrapper(0x101, 7, Some(obj("a"))));
        peer.write_all(&data(3, payload.clone(), false))
            .await
            .unwrap();

        let first = soon(c.recv_envelope(&[REPLY_CHANNEL])).await.unwrap();
        assert_eq!(first.body, Body::Empty);
        let second = soon(c.recv_envelope(&[REPLY_CHANNEL])).await.unwrap();
        assert_eq!((second.message_id, second.flags), (7, 0x101));
        assert_eq!(second.body, Body::Object(obj("a")));

        // The filtered wrappers skip the keepalive and also must not stall.
        peer.write_all(&data(3, payload, false)).await.unwrap();
        let v = soon(c.recv_any()).await.unwrap();
        assert_eq!(v, obj("a").to_plist());
    }

    #[tokio::test]
    async fn bodyless_wrapper_is_body_none() {
        let (mut c, mut peer) = client().await;
        peer.write_all(&data(1, wrapper(0x201, 0, None), false))
            .await
            .unwrap();
        let e = soon(c.recv_envelope(&[ROOT_CHANNEL])).await.unwrap();
        assert_eq!(
            (e.channel, e.flags, e.body),
            (ROOT_CHANNEL, 0x201, Body::None)
        );
    }

    #[tokio::test]
    async fn unrequested_channel_stays_buffered() {
        let (mut c, mut peer) = client().await;
        peer.write_all(&data(3, wrapper(0x20101, 1, Some(obj("reply"))), false))
            .await
            .unwrap();
        peer.write_all(&data(1, wrapper(0x101, 2, Some(obj("root"))), false))
            .await
            .unwrap();
        let root = soon(c.recv_envelope(&[ROOT_CHANNEL])).await.unwrap();
        assert_eq!(root.body, Body::Object(obj("root")));
        let reply = soon(c.recv_envelope(&[REPLY_CHANNEL])).await.unwrap();
        assert_eq!(
            (reply.channel, reply.body),
            (REPLY_CHANNEL, Body::Object(obj("reply")))
        );
    }

    #[tokio::test]
    async fn end_stream_after_drain_leaves_other_stream_readable() {
        let (mut c, mut peer) = client().await;
        peer.write_all(&data(3, wrapper(0x101, 1, Some(obj("last"))), true))
            .await
            .unwrap();
        peer.write_all(&data(1, wrapper(0x101, 2, Some(obj("root"))), false))
            .await
            .unwrap();
        let both = [ROOT_CHANNEL, REPLY_CHANNEL];
        // The payload queued before END_STREAM comes first, then the end.
        assert_eq!(
            soon(c.recv_envelope(&both)).await.unwrap().body,
            Body::Object(obj("last"))
        );
        assert!(matches!(
            xpc_err(soon(c.recv_envelope(&both)).await),
            XpcError::StreamEnded { stream_id: 3 }
        ));
        // The end is sticky while the channel is listed.
        assert!(matches!(
            xpc_err(soon(c.recv_envelope(&both)).await),
            XpcError::StreamEnded { stream_id: 3 }
        ));
        // Root still delivers once the ended channel is dropped.
        assert_eq!(
            soon(c.recv_envelope(&[ROOT_CHANNEL])).await.unwrap().body,
            Body::Object(obj("root"))
        );
        peer.write_all(&data(1, wrapper(0x101, 3, Some(obj("more"))), false))
            .await
            .unwrap();
        assert_eq!(
            soon(c.recv_envelope(&[ROOT_CHANNEL])).await.unwrap().body,
            Body::Object(obj("more"))
        );
    }

    #[tokio::test]
    async fn end_stream_on_headers_is_recorded() {
        let (mut c, mut peer) = client().await;
        // HEADERS, empty block, flags END_STREAM | END_HEADERS, stream 1.
        peer.write_all(&[0, 0, 0, 0x01, 0x05, 0, 0, 0, 1])
            .await
            .unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv_root()).await),
            XpcError::StreamEnded { stream_id: 1 }
        ));
    }

    #[tokio::test]
    async fn end_stream_mid_wrapper_is_truncated() {
        let (mut c, mut peer) = client().await;
        let w = wrapper(0x101, 1, Some(obj("x")));
        peer.write_all(&data(3, w[..30].to_vec(), true))
            .await
            .unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Truncated {
                stream_id: Some(3),
                buffered: 30
            }
        ));
    }

    #[tokio::test]
    async fn tcp_eof_classification() {
        // Partial XPC wrapper buffered on the channel.
        let (mut c, mut peer) = client().await;
        let w = wrapper(0x101, 1, Some(obj("x")));
        peer.write_all(&data(3, w[..30].to_vec(), false))
            .await
            .unwrap();
        peer.shutdown().await.unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Truncated {
                stream_id: Some(3),
                buffered: 30
            }
        ));

        // Partial HTTP/2 frame buffered.
        let (mut c, mut peer) = client().await;
        let f = data(3, w.clone(), false);
        peer.write_all(&f[..12]).await.unwrap();
        peer.shutdown().await.unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Truncated {
                stream_id: None,
                buffered: 12
            }
        ));

        // Nothing pending.
        let (mut c, mut peer) = client().await;
        peer.shutdown().await.unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::ConnectionClosed
        ));
    }

    #[tokio::test]
    async fn rst_stream_and_goaway_are_typed() {
        let (mut c, mut peer) = client().await;
        peer.write_all(&[0, 0, 4, 0x03, 0, 0, 0, 0, 3, 0, 0, 0, 8])
            .await
            .unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Reset {
                stream_id: 3,
                error_code: 8
            }
        ));
        // Sticky for that stream.
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Reset { stream_id: 3, .. }
        ));

        let (mut c, mut peer) = client().await;
        let mut goaway = vec![0, 0, 10, 0x07, 0, 0, 0, 0, 0];
        goaway.extend([0, 0, 0, 3, 0, 0, 0, 2, b'h', b'i']);
        peer.write_all(&goaway).await.unwrap();
        match xpc_err(soon(c.recv_root()).await) {
            XpcError::GoAway {
                last_stream_id,
                error_code,
                debug,
            } => assert_eq!((last_stream_id, error_code, debug.as_str()), (3, 2, "hi")),
            e => panic!("{e:?}"),
        }
    }

    #[tokio::test]
    async fn rst_on_one_stream_leaves_other_readable() {
        let (mut c, mut peer) = client().await;
        peer.write_all(&[0, 0, 4, 0x03, 0, 0, 0, 0, 3, 0, 0, 0, 8])
            .await
            .unwrap();
        peer.write_all(&data(1, wrapper(0x101, 1, Some(obj("root"))), false))
            .await
            .unwrap();
        assert_eq!(soon(c.recv_root()).await.unwrap(), obj("root").to_plist());
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::Reset { stream_id: 3, .. }
        ));
    }

    #[tokio::test]
    async fn over_cap_header_rejected_before_body() {
        let (mut c, mut peer) = client().await;
        c.set_max_message_size(1024);
        // Header only, declaring a 1 GiB body; none of the body is ever sent.
        let mut header = 0x29b00b92_u32.to_le_bytes().to_vec();
        header.extend(1u32.to_le_bytes());
        header.extend((1u64 << 30).to_le_bytes());
        header.extend(0u64.to_le_bytes());
        peer.write_all(&data(3, header, false)).await.unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv()).await),
            XpcError::MessageTooLarge { declared, max: 1024 } if declared == (1 << 30) + 24
        ));
    }

    #[tokio::test]
    async fn send_object_ids_are_sequential() {
        let (mut c, mut peer) = client().await;
        assert_eq!(c.send_object_with_id(obj("a"), true).await.unwrap(), 1);
        c.send_object(obj("b"), false).await.unwrap();
        assert_eq!(c.send_object_with_id(obj("c"), false).await.unwrap(), 3);

        // An unencodable message fails before writing and consumes no id.
        let bad = XPCObject::Date(std::time::UNIX_EPOCH - Duration::from_secs(1));
        assert!(matches!(
            xpc_err(c.send_object_with_id(bad, false).await),
            XpcError::DateOutOfRange
        ));
        assert_eq!(c.send_object_with_id(obj("d"), false).await.unwrap(), 4);

        drop(c);
        let mut wire = Vec::new();
        peer.read_to_end(&mut wire).await.unwrap();
        let mut rest = &wire[b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".len()..];
        let mut ids = Vec::new();
        while let Some((frame, used)) = Frame::parse(rest).unwrap() {
            if let Frame::Data(d) = frame {
                ids.push(XPCMessage::decode(&d.payload).unwrap().message_id.unwrap());
            }
            rest = &rest[used..];
        }
        assert_eq!(ids, [1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn strict_streams_reject_unopened_stream() {
        let (mut c, mut peer) = client().await;
        c.set_strict_streams(true);
        peer.write_all(&data(8, vec![1, 2, 3], false))
            .await
            .unwrap();
        assert!(matches!(
            xpc_err(soon(c.recv_any()).await),
            XpcError::UnexpectedStream { stream_id: 8 }
        ));
        // Root/reply data before they are opened is still admitted.
        peer.write_all(&data(3, wrapper(0x101, 1, Some(obj("ok"))), false))
            .await
            .unwrap();
        assert_eq!(soon(c.recv()).await.unwrap(), obj("ok").to_plist());

        // Non-strict (default) caches it instead.
        let (mut c, mut peer) = client().await;
        peer.write_all(&data(8, vec![1, 2, 3], false))
            .await
            .unwrap();
        peer.write_all(&data(3, wrapper(0x101, 1, Some(obj("ok"))), false))
            .await
            .unwrap();
        assert_eq!(soon(c.recv()).await.unwrap(), obj("ok").to_plist());
    }
}
