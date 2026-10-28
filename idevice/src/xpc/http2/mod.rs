// Jackson Coxson

use frame::HttpFrame;
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, warn};

use crate::xpc::errors::XpcError;
use crate::{IdeviceError, ReadWrite};

pub mod frame;
pub use frame::Setting;

const HTTP2_MAGIC: &[u8] = "PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n".as_bytes();

/// HTTP/2 default initial flow-control window for both the connection and each
/// stream (RFC 7540 §6.9.2). The peer can raise the per-stream default via a
/// SETTINGS `InitialWindowSize`.
const DEFAULT_WINDOW: i64 = 65535;

/// Root and reply channels. The peer may send DATA on these before we open
/// them, so strict-streams mode always admits them.
const PRE_OPEN_STREAMS: [u32; 2] = [1, 3];

#[derive(Debug)]
pub struct Http2Client<R: ReadWrite> {
    inner: R,
    cache: HashMap<u32, VecDeque<Vec<u8>>>,
    /// How many payload octets we may still send on the connection as a whole
    /// before the peer must replenish it with a connection-level WINDOW_UPDATE.
    conn_send_window: i64,
    /// Per-stream remaining send window. Lazily seeded to `peer_initial_window`.
    stream_send_windows: HashMap<u32, i64>,
    /// The peer's current SETTINGS `InitialWindowSize` — the window each *new*
    /// stream starts with.
    peer_initial_window: i64,
    /// Raw inbound bytes not yet parsed into a whole frame. Persisting this
    /// across reads keeps [`Self::pump`] cancellation-safe: a partially-received
    /// frame survives a dropped read.
    recv_buf: Vec<u8>,
    /// Streams we finished pushing an outbound file transfer on. The device
    /// resets them once it has the payload, so a RST_STREAM for one of these is
    /// normal completion rather than an error.
    finished_file_transfer_streams: HashSet<u32>,
    /// Streams the peer ended with END_STREAM. Reads return their cached
    /// payloads first, then [`XpcError::StreamEnded`].
    ended: HashSet<u32>,
    /// Streams the peer reset, with the RST_STREAM error code.
    reset: HashMap<u32, u32>,
    /// Streams this client opened with HEADERS.
    opened: HashSet<u32>,
    /// When set, DATA on a stream outside `opened` and [`PRE_OPEN_STREAMS`] is
    /// [`XpcError::UnexpectedStream`] instead of being cached.
    strict_streams: bool,
}

impl<R: ReadWrite> Http2Client<R> {
    /// Writes the magic and inits the caches
    pub async fn new(mut inner: R) -> Result<Self, IdeviceError> {
        inner.write_all(HTTP2_MAGIC).await?;
        inner.flush().await?;
        Ok(Self {
            inner,
            cache: HashMap::new(),
            conn_send_window: DEFAULT_WINDOW,
            stream_send_windows: HashMap::new(),
            peer_initial_window: DEFAULT_WINDOW,
            recv_buf: Vec::new(),
            finished_file_transfer_streams: HashSet::new(),
            ended: HashSet::new(),
            reset: HashMap::new(),
            opened: HashSet::new(),
            strict_streams: false,
        })
    }

    /// See `RemoteXpcClient::set_strict_streams`.
    pub fn set_strict_streams(&mut self, strict: bool) {
        self.strict_streams = strict;
    }

    /// Read the next whole frame, buffering raw bytes in `recv_buf` until one is
    /// complete. Cancellation-safe: the single `read` is cancel-safe (no bytes
    /// lost if the future is dropped on `Pending`), and any bytes already
    /// buffered persist in `self` for the next call.
    ///
    /// TCP EOF is [`XpcError::Truncated`] when part of a frame is buffered and
    /// [`XpcError::ConnectionClosed`] otherwise.
    async fn next_frame(&mut self) -> Result<frame::Frame, IdeviceError> {
        loop {
            if let Some((frame, consumed)) = frame::Frame::parse(&self.recv_buf)? {
                self.recv_buf.drain(..consumed);
                return Ok(frame);
            }
            let mut tmp = [0u8; 16384];
            let n = self.inner.read(&mut tmp).await?;
            if n == 0 {
                return Err(if self.recv_buf.is_empty() {
                    XpcError::ConnectionClosed
                } else {
                    XpcError::Truncated {
                        stream_id: None,
                        buffered: self.recv_buf.len(),
                    }
                }
                .into());
            }
            self.recv_buf.extend_from_slice(&tmp[..n]);
        }
    }

    pub async fn set_settings(
        &mut self,
        settings: Vec<frame::Setting>,
        stream_id: u32,
    ) -> Result<(), IdeviceError> {
        let frame = frame::SettingsFrame {
            settings,
            stream_id,
            flags: 0,
        }
        .serialize();
        self.inner.write_all(&frame).await?;
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn window_update(
        &mut self,
        increment_size: u32,
        stream_id: u32,
    ) -> Result<(), IdeviceError> {
        let frame = frame::WindowUpdateFrame {
            increment_size,
            stream_id,
        }
        .serialize();
        self.inner.write_all(&frame).await?;
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn open_stream(&mut self, stream_id: u32) -> Result<(), IdeviceError> {
        // Sometimes Apple is silly and sends data to a stream that isn't open
        self.cache.entry(stream_id).or_default();
        self.opened.insert(stream_id);
        let frame = frame::HeadersFrame {
            stream_id,
            end_stream: false,
        }
        .serialize();
        self.inner.write_all(&frame).await?;
        self.inner.flush().await?;
        Ok(())
    }

    pub async fn send(&mut self, payload: Vec<u8>, stream_id: u32) -> Result<(), IdeviceError> {
        self.send_inner(payload, stream_id, false).await
    }

    /// Sends `payload` and closes our half of the stream with END_STREAM.
    ///
    /// Used to finish an outbound file transfer: the device resets the stream
    /// once it has the payload, and [`Self::pump`] treats that reset as normal
    /// completion rather than an error.
    pub async fn send_end_stream(
        &mut self,
        payload: Vec<u8>,
        stream_id: u32,
    ) -> Result<(), IdeviceError> {
        self.finished_file_transfer_streams.insert(stream_id);
        self.send_inner(payload, stream_id, true).await
    }

    async fn send_inner(
        &mut self,
        payload: Vec<u8>,
        stream_id: u32,
        end_stream: bool,
    ) -> Result<(), IdeviceError> {
        const MAX_FRAME_SIZE: usize = 16384;
        let mut chunks = payload.chunks(MAX_FRAME_SIZE).peekable();
        // Always send at least one frame, even for an empty payload. An empty
        // DATA frame costs no flow-control window, so send it directly.
        if chunks.peek().is_none() {
            let frame = frame::DataFrame {
                stream_id,
                payload: Vec::new(),
                end_stream,
            }
            .serialize();
            self.inner.write_all(&frame).await?;
            self.inner.flush().await?;
            return Ok(());
        }
        while let Some(chunk) = chunks.next() {
            let need = chunk.len() as i64;
            // Respect the peer's flow-control window: a DATA frame must not exceed
            // either the connection-level or the stream-level send window, or the
            // peer aborts the connection with a GOAWAY (FLOW_CONTROL_ERROR). When
            // either window is exhausted, pump inbound frames until the peer grants
            // more room with a WINDOW_UPDATE. (Matters for large payloads like
            // pasteboard images; small ones fit in the initial 64 KiB window.)
            while self.conn_send_window < need || self.stream_send_window(stream_id) < need {
                self.pump().await?;
            }
            let frame = frame::DataFrame {
                stream_id,
                payload: chunk.to_vec(),
                // Only the last chunk carries END_STREAM.
                end_stream: end_stream && chunks.peek().is_none(),
            }
            .serialize();
            self.inner.write_all(&frame).await?;
            self.conn_send_window -= need;
            *self
                .stream_send_windows
                .get_mut(&stream_id)
                .expect("seeded by stream_send_window above") -= need;
        }
        self.inner.flush().await?;
        Ok(())
    }

    /// The remaining send window for `stream_id`, seeding it to the peer's current
    /// initial window size the first time we touch the stream.
    fn stream_send_window(&mut self, stream_id: u32) -> i64 {
        *self
            .stream_send_windows
            .entry(stream_id)
            .or_insert(self.peer_initial_window)
    }

    /// Reads the next buffered payload from whichever of `stream_ids` produces
    /// one first, returning it with the stream it came from.
    ///
    /// Cached payloads on every listed stream are returned before any end or
    /// reset is reported. Then a listed stream the peer reset yields
    /// [`XpcError::Reset`] and one it ended yields [`XpcError::StreamEnded`],
    /// on every call, so the caller drops that id and keeps reading the others.
    /// O(len(stream_ids)) per pass plus one frame per pump.
    pub async fn read_any(&mut self, stream_ids: &[u32]) -> Result<(u32, Vec<u8>), IdeviceError> {
        for id in stream_ids {
            self.cache.entry(*id).or_default();
        }
        loop {
            for id in stream_ids {
                if let Some(d) = self.cache.get_mut(id).and_then(|c| c.pop_front()) {
                    return Ok((*id, d));
                }
            }
            for id in stream_ids {
                self.check_stream_open(*id)?;
            }
            self.pump().await?;
        }
    }

    /// Reads the next payload on `stream_id`; see [`Self::read_any`] for end
    /// and reset reporting.
    pub async fn read(&mut self, stream_id: u32) -> Result<Vec<u8>, IdeviceError> {
        self.read_any(&[stream_id]).await.map(|(_, d)| d)
    }

    /// `Err` when the peer reset or ended `stream_id`; called only once its
    /// cache is empty.
    fn check_stream_open(&self, stream_id: u32) -> Result<(), IdeviceError> {
        if let Some(&error_code) = self.reset.get(&stream_id) {
            return Err(XpcError::Reset {
                stream_id,
                error_code,
            }
            .into());
        }
        if self.ended.contains(&stream_id) {
            return Err(XpcError::StreamEnded { stream_id }.into());
        }
        Ok(())
    }

    /// Read and handle a single inbound frame: ack SETTINGS (applying any
    /// `InitialWindowSize` change), apply WINDOW_UPDATEs to our send windows,
    /// replenish the peer's receive window for inbound DATA and buffer that DATA
    /// by stream, recording END_STREAM from DATA and HEADERS. GOAWAY surfaces as
    /// an error from [`frame::Frame::parse`]; RST_STREAM (other than for a
    /// finished outbound transfer) is recorded, and reads of that stream
    /// report [`XpcError::Reset`] after its cache drains. O(frame length).
    async fn pump(&mut self) -> Result<(), IdeviceError> {
        let frame = self.next_frame().await?;
        match frame {
            frame::Frame::Settings(settings_frame) if settings_frame.flags != 1 => {
                // Adjust every existing stream's send window by the delta in the
                // new InitialWindowSize (RFC 7540 §6.9.2), then ack.
                for setting in &settings_frame.settings {
                    if let frame::Setting::InitialWindowSize(new) = setting {
                        let delta = *new as i64 - self.peer_initial_window;
                        self.peer_initial_window = *new as i64;
                        for w in self.stream_send_windows.values_mut() {
                            *w += delta;
                        }
                    }
                }
                let ack = frame::SettingsFrame {
                    settings: Vec::new(),
                    stream_id: settings_frame.stream_id,
                    flags: 1,
                }
                .serialize();
                self.inner.write_all(&ack).await?;
                self.inner.flush().await?;
            }
            frame::Frame::WindowUpdate(w) => {
                if w.stream_id == 0 {
                    self.conn_send_window += w.increment_size as i64;
                } else {
                    let initial = self.peer_initial_window;
                    *self
                        .stream_send_windows
                        .entry(w.stream_id)
                        .or_insert(initial) += w.increment_size as i64;
                }
            }
            frame::Frame::RstStream(rst) => {
                if self.finished_file_transfer_streams.remove(&rst.stream_id) {
                    debug!(
                        "Device reset finished file transfer stream {}",
                        rst.stream_id
                    );
                } else {
                    // Recorded, not returned: a reset of one stream must not
                    // fail a read of another. Readers of this stream get
                    // `Reset` once its cache is drained.
                    self.reset.insert(rst.stream_id, rst.error_code);
                }
            }
            frame::Frame::Headers(h) => {
                if h.end_stream {
                    self.ended.insert(h.stream_id);
                }
            }
            frame::Frame::Data(data_frame) => {
                debug!(
                    "Got data frame for {} with {} bytes",
                    data_frame.stream_id,
                    data_frame.payload.len()
                );

                let len = data_frame.payload.len() as u32;
                let stream_id = data_frame.stream_id;
                if self.strict_streams
                    && !self.opened.contains(&stream_id)
                    && !PRE_OPEN_STREAMS.contains(&stream_id)
                {
                    return Err(XpcError::UnexpectedStream { stream_id }.into());
                }
                if data_frame.end_stream {
                    self.ended.insert(stream_id);
                }
                // Cache the payload BEFORE any await so a cancelled pump (the poll
                // tick interrupting `recv_push`) can never drop it.
                self.cache
                    .entry(stream_id)
                    .or_insert_with(|| {
                        // Apple sometimes sends data before the stream is "open".
                        warn!("Received message for stream ID {stream_id} not in cache");
                        VecDeque::new()
                    })
                    .push_back(data_frame.payload);
                if len > 0 {
                    // Replenish the peer's view of our receive window so it keeps
                    // sending (e.g. the rest of a large pasteboard image). Queue
                    // both WINDOW_UPDATE frames, then flush once: write_all on the
                    // tunnel stream queues a whole frame without suspending, so a
                    // cancellation can only land on the flush — by which point both
                    // frames are already queued (never a torn or dropped update).
                    let conn = frame::WindowUpdateFrame {
                        increment_size: len,
                        stream_id: 0,
                    }
                    .serialize();
                    let stream = frame::WindowUpdateFrame {
                        increment_size: len,
                        stream_id,
                    }
                    .serialize();
                    self.inner.write_all(&conn).await?;
                    self.inner.write_all(&stream).await?;
                    self.inner.flush().await?;
                }
            }
            _ => {
                // SETTINGS ack — nothing to do.
            }
        }
        Ok(())
    }
}
