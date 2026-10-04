//! Instruments protocol message format implementation
//!
//! This module handles the serialization and deserialization of messages used in
//! the iOS instruments protocol. The message format consists of:
//! - 32-byte message header
//! - 16-byte payload header
//! - Optional auxiliary data section
//! - Payload data (typically NSKeyedArchive format)
//!
//! # Message Structure
//! ```text
//! +---------------------+
//! |   MessageHeader     | 32 bytes
//! +---------------------+
//! |   PayloadHeader     | 16 bytes
//! +---------------------+
//! |   AuxHeader         | 16 bytes (if aux present)
//! |   Aux data          | variable length
//! +---------------------+
//! |   Payload data      | variable length (NSKeyedArchive)
//! +---------------------+
//! ```
//!
//! # Example
//! ```rust,no_run
//! use plist::Value;
//! use your_crate::IdeviceError;
//! use your_crate::dvt::message::{Message, MessageHeader, PayloadHeader, AuxValue};
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), IdeviceError> {
//! // Create a new message
//! let header = MessageHeader::new(
//!     1,      // fragment_id
//!     1,      // fragment_count  
//!     123,    // identifier
//!     0,      // conversation_index
//!     42,     // channel
//!     true    // expects_reply
//! );
//!
//! let message = Message::new(
//!     header,
//!     PayloadHeader::method_invocation(),
//!     Some(AuxValue::from_values(vec![
//!         AuxValue::String("param".into()),
//!         AuxValue::U32(123),
//!     ])),
//!     Some(Value::String("data".into()))
//! );
//!
//! // Serialize message
//! let bytes = message.serialize();
//!
//! // Deserialize message (from async reader)
//! # let mut reader = &bytes[..];
//! let deserialized = Message::from_reader(&mut reader).await?;
//! # Ok(())
//! # }

use plist::Value;
use std::io::{Cursor, Read};
use tokio::io::{AsyncRead, AsyncReadExt};

use super::errors::DvtError;
use crate::{IdeviceError, pretty_print_plist};

/// Message header containing metadata about the message
///
/// 32-byte structure that appears at the start of every message
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MessageHeader {
    /// Magic number identifying the protocol (0x1F3D5B79)
    magic: u32,
    /// Length of this header (always 32)
    header_len: u32,
    /// Fragment identifier for multipart messages
    fragment_id: u16,
    /// Total number of fragments
    fragment_count: u16,
    /// Total length of payload (headers + aux + data)
    length: u32,
    /// Unique message identifier
    identifier: u32,
    /// Conversation tracking index
    conversation_index: u32,
    /// Channel number this message belongs to
    pub channel: i32,
    /// Whether a reply is expected
    expects_reply: bool,
}

/// Payload header containing information about the message contents
///
/// 16-byte structure following the message header
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct PayloadHeader {
    /// DTX message type (DISPATCH/OBJECT/OK/ERROR/DATA)
    msg_type: u8,
    /// Reserved bytes in the wire format
    flags_a: u8,
    /// Reserved bytes in the wire format
    flags_b: u8,
    /// Reserved byte in the wire format
    reserved: u8,
    /// Length of auxiliary data section
    aux_length: u32,
    /// Total length of payload (aux + data)
    total_length: u32,
    /// Additional payload flags
    flags: u32,
}

/// Header for auxiliary data section
///
/// 16-byte structure preceding auxiliary data
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct AuxHeader {
    /// Buffer size hint (often 496)
    buffer_size: u32,
    /// Unknown field (typically 0)
    unknown: u32,
    /// Actual size of auxiliary data
    aux_size: u32,
    /// Unknown field (typically 0)
    unknown2: u32,
}

/// Auxiliary data container
///
/// Contains a header and a collection of typed values
#[derive(Debug, Clone, PartialEq)]
pub struct Aux {
    /// Auxiliary data header
    pub header: AuxHeader,
    /// Collection of auxiliary values
    pub values: Vec<AuxValue>,
}

/// Typed auxiliary value that can be included in messages
#[derive(Clone, PartialEq)]
pub enum AuxValue {
    /// NULL value (type 0x0a) - no payload bytes
    Null,
    /// UTF-8 string value (type 0x01)
    String(String),
    /// Raw byte array (type 0x02)
    Array(Vec<u8>),
    /// 32-bit unsigned integer (type 0x03)
    U32(u32),
    /// 64-bit signed integer (type 0x06)
    I64(i64),
    /// 64-bit floating point (double) (type 0x09)
    Double(f64),
    /// Primitive dictionary (type 0xF0) - value is a list of primitives to match pymobiledevice3 format
    PrimitiveDictionary(Vec<(AuxValue, Vec<AuxValue>)>),
}

/// Default cap on a reassembled DTX message (payload header + aux + data), 16 MiB.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

const DTX_MAGIC: u32 = 0x1F3D5B79;

/// Complete protocol message
///
/// For a received message, aux and payload decode independently: a decode
/// failure in one sets its `*_error` field and leaves the decoded field `None`,
/// while the raw bytes stay available in `raw_aux` / `raw_data`.
#[derive(Clone, PartialEq)]
pub struct Message {
    /// Message metadata header
    pub message_header: MessageHeader,
    /// Payload description header
    pub payload_header: PayloadHeader,
    /// Decoded auxiliary data; `None` when absent or when decoding failed (see `aux_error`)
    pub aux: Option<Aux>,
    /// Decoded payload (NSKeyedArchive); `None` when absent or when decoding failed (see `data_error`)
    pub data: Option<Value>,
    /// Raw bytes of the data section before NSKeyedArchive decoding; `Some` whenever
    /// a received message carried a non-empty data section
    pub raw_data: Option<Vec<u8>>,
    /// Raw bytes of the aux section; `Some` whenever a received message carried a
    /// non-empty aux section
    pub raw_aux: Option<Vec<u8>>,
    /// Why `raw_aux` failed to decode, when it did
    pub aux_error: Option<String>,
    /// Why `raw_data` failed to decode as an NSKeyedArchive, when it did
    pub data_error: Option<String>,
}

impl Aux {
    /// Parses the legacy aux wire format used on iOS 16 and earlier
    ///
    /// Layout: `[AuxHeader (16 B)][type (4 B)][data...][type (4 B)][data...]...`
    ///
    /// Type 0xF0 entries are `PrimitiveDictionary` blocks embedded inside
    /// the legacy envelope; their bodies are skipped since the useful values
    /// are the surrounding flat entries.
    ///
    /// O(n) time in `bytes.len()`; the values allocate at most `n` bytes in
    /// total because every length is checked against the remaining bytes first.
    fn parse_legacy_bytes(bytes: &[u8]) -> Result<Self, IdeviceError> {
        if bytes.len() < 16 {
            return Err(IdeviceError::NotEnoughBytes(bytes.len(), 16));
        }

        let mut cursor = Cursor::new(bytes);
        let header = AuxHeader {
            buffer_size: Self::read_u32(&mut cursor)?,
            unknown: Self::read_u32(&mut cursor)?,
            aux_size: Self::read_u32(&mut cursor)?,
            unknown2: Self::read_u32(&mut cursor)?,
        };

        let mut values = Vec::new();
        while cursor.position() + 4 <= bytes.len() as u64 {
            let aux_type = Self::read_u32(&mut cursor)?;
            match aux_type {
                0x0a => {
                    // PNULL separator — used as dictionary keys; not a user value.
                }
                0x0f0 => {
                    // PrimitiveDictionary block embedded in a legacy envelope.
                    // Layout after the type: u32 flags, u64 body_length, [body].
                    // Skip the entire block; positional args appear as flat entries.
                    let _flags = Self::read_u32(&mut cursor)?;
                    let body_len = Self::read_u64(&mut cursor)?;
                    Self::ensure_remaining(&cursor, body_len)?;
                    cursor.set_position(cursor.position() + body_len);
                }
                _ => {
                    // All other types share the same encoding as parse_primitive,
                    // but the type word is already consumed above so we reconstruct
                    // a cursor over [type || remaining] to reuse parse_primitive.
                    let pos = cursor.position() - 4;
                    let rest = bytes.get(pos as usize..).unwrap_or_default();
                    let mut sub = Cursor::new(rest);
                    values.push(Self::parse_primitive(&mut sub)?);
                    cursor.set_position(pos + sub.position());
                }
            }
        }

        Ok(Self { header, values })
    }

    /// Errors with [`DvtError::AuxTruncated`] unless `needed` bytes remain after the cursor.
    fn ensure_remaining(cursor: &Cursor<&[u8]>, needed: u64) -> Result<(), IdeviceError> {
        let available = (cursor.get_ref().len() as u64).saturating_sub(cursor.position());
        if needed > available {
            return Err(DvtError::AuxTruncated { needed, available }.into());
        }
        Ok(())
    }

    fn read_array<const N: usize>(cursor: &mut Cursor<&[u8]>) -> Result<[u8; N], IdeviceError> {
        Self::ensure_remaining(cursor, N as u64)?;
        let mut buf = [0u8; N];
        Read::read_exact(cursor, &mut buf)?;
        Ok(buf)
    }

    fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, IdeviceError> {
        Ok(u32::from_le_bytes(Self::read_array(cursor)?))
    }

    fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, IdeviceError> {
        Ok(u64::from_le_bytes(Self::read_array(cursor)?))
    }

    fn read_f64(cursor: &mut Cursor<&[u8]>) -> Result<f64, IdeviceError> {
        Ok(f64::from_le_bytes(Self::read_array(cursor)?))
    }

    /// Reads `len` bytes, allocating only after `len` is checked against the remaining bytes.
    fn read_exact_vec(cursor: &mut Cursor<&[u8]>, len: usize) -> Result<Vec<u8>, IdeviceError> {
        Self::ensure_remaining(cursor, len as u64)?;
        let mut buf = vec![0u8; len];
        Read::read_exact(cursor, &mut buf)?;
        Ok(buf)
    }

    fn parse_primitive(cursor: &mut Cursor<&[u8]>) -> Result<AuxValue, IdeviceError> {
        let raw_type = Self::read_u32(cursor)?;
        let type_code = raw_type & 0xFF;
        match type_code {
            0x01 => {
                let len = Self::read_u32(cursor)? as usize;
                Ok(AuxValue::String(String::from_utf8(Self::read_exact_vec(
                    cursor, len,
                )?)?))
            }
            0x02 => {
                let len = Self::read_u32(cursor)? as usize;
                Ok(AuxValue::Array(Self::read_exact_vec(cursor, len)?))
            }
            0x03 => Ok(AuxValue::U32(Self::read_u32(cursor)?)),
            0x06 => Ok(AuxValue::I64(Self::read_u64(cursor)? as i64)),
            0x09 => Ok(AuxValue::Double(Self::read_f64(cursor)?)),
            0x0A => Ok(AuxValue::Null),
            _ => Err(DvtError::UnknownAuxValueType(raw_type).into()),
        }
    }

    /// Parses auxiliary data from bytes, selecting the correct wire format
    /// based on the leading magic byte.
    ///
    /// # Wire formats
    ///
    /// **Legacy**: the first byte is NOT `0xF0`.
    /// The buffer begins with a 16-byte `AuxHeader` followed by flat
    /// type-tagged value entries.
    ///
    /// **Modern** (iOS 17+, RSD/testmanagerd path): the first byte IS `0xF0`,
    /// indicating the entire buffer is a single `PrimitiveDictionary` block
    /// (`[flags(4B)][unknown(4B)][body_len(8B)][key-value pairs...]`).
    /// Keys are positional-null sentinels; only the values are collected.
    ///
    /// # Errors
    /// Never panics or reads past `bytes`. Yields `NotEnoughBytes` for a buffer
    /// shorter than its 16-byte header, [`DvtError::AuxTruncated`] when a declared
    /// length runs past the buffer, [`DvtError::UnknownAuxValueType`] for an
    /// unknown type tag, and `Utf8` for a non-UTF-8 string value.
    /// O(n) time and at most n bytes of value storage for an n-byte buffer.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, IdeviceError> {
        Self::parse(&bytes)
    }

    /// Slice form of [`Aux::from_bytes`] with the same validation.
    pub fn parse(bytes: &[u8]) -> Result<Self, IdeviceError> {
        let Some(&first) = bytes.first() else {
            return Ok(Self::from_values(Vec::new()));
        };

        if first != 0xF0 {
            return Self::parse_legacy_bytes(bytes);
        }

        if bytes.len() < 16 {
            return Err(IdeviceError::NotEnoughBytes(bytes.len(), 16));
        }

        let mut cursor = Cursor::new(bytes);
        let _type_and_flags = Self::read_u32(&mut cursor)?;
        let _unknown_flags = Self::read_u32(&mut cursor)?;
        let body_len = Self::read_u64(&mut cursor)?;
        Self::ensure_remaining(&cursor, body_len)?;
        let body_end = 16 + body_len;

        let mut values = Vec::new();
        while cursor.position() < body_end {
            let _key = Self::parse_primitive(&mut cursor)?;
            let value = Self::parse_primitive(&mut cursor)?;
            values.push(value);
        }

        Ok(Self {
            header: AuxHeader::default(),
            values,
        })
    }

    /// Creates new auxiliary data from values
    ///
    /// Note: Header fields are populated during serialization
    ///
    /// # Arguments
    /// * `values` - Collection of auxiliary values to include
    pub fn from_values(values: Vec<AuxValue>) -> Self {
        Self {
            header: AuxHeader::default(),
            values,
        }
    }

    /// Serializes auxiliary data to bytes
    ///
    /// Includes properly formatted header with updated size fields
    pub fn serialize(&self) -> Vec<u8> {
        let mut values_payload = Vec::new();
        for v in self.values.iter() {
            values_payload.extend_from_slice(&0x0a_u32.to_le_bytes());
            match v {
                AuxValue::Null => {
                    // PNULL - type 0x0a with no payload bytes
                }
                AuxValue::String(s) => {
                    values_payload.extend_from_slice(&0x01_u32.to_le_bytes());
                    values_payload.extend_from_slice(&(s.len() as u32).to_le_bytes());
                    values_payload.extend_from_slice(s.as_bytes());
                }
                AuxValue::Array(v) => {
                    values_payload.extend_from_slice(&0x02_u32.to_le_bytes());
                    values_payload.extend_from_slice(&(v.len() as u32).to_le_bytes());
                    values_payload.extend_from_slice(v);
                }
                AuxValue::U32(u) => {
                    values_payload.extend_from_slice(&0x03_u32.to_le_bytes());
                    values_payload.extend_from_slice(&u.to_le_bytes());
                }
                AuxValue::I64(i) => {
                    values_payload.extend_from_slice(&0x06_u32.to_le_bytes());
                    values_payload.extend_from_slice(&i.to_le_bytes());
                }
                AuxValue::Double(d) => {
                    values_payload.extend_from_slice(&0x09_u32.to_le_bytes());
                    values_payload.extend_from_slice(&d.to_le_bytes());
                }
                AuxValue::PrimitiveDictionary(entries) => {
                    // PrimitiveDictionary: type=0xF0, entries are (key, [values]) pairs
                    // Header: u32 magic (0x1F0), u32 unknown (0), u64 body_length
                    // pymobiledevice3 format: {PNULL: [arg1, arg2, ...]}
                    let mut body_payload = Vec::new();
                    for (key, values) in entries {
                        // Write the key primitive once (typically NULL 0x0a)
                        body_payload.extend_from_slice(&0x0a_u32.to_le_bytes());
                        write_primitive_value(key, &mut body_payload);
                        // Write each value in the list
                        for value in values {
                            write_primitive_value(value, &mut body_payload);
                        }
                    }
                    let body_len = body_payload.len() as u64;
                    values_payload.extend_from_slice(&0xf0_u32.to_le_bytes());
                    values_payload.extend_from_slice(&0_u32.to_le_bytes()); // unknown flags
                    values_payload.extend_from_slice(&body_len.to_le_bytes());
                    values_payload.extend_from_slice(&body_payload);
                }
            }
        }

        let mut res = Vec::new();
        let buffer_size = 496_u32;
        res.extend_from_slice(&buffer_size.to_le_bytes());
        res.extend_from_slice(&0_u32.to_le_bytes());
        res.extend_from_slice(&(values_payload.len() as u32).to_le_bytes());
        res.extend_from_slice(&0_u32.to_le_bytes());
        res.extend_from_slice(&values_payload);
        res
    }
}

/// Helper to write a primitive value to the payload
fn write_primitive_value(v: &AuxValue, payload: &mut Vec<u8>) {
    match v {
        AuxValue::Null => {
            // PNULL - type 0x0a with no payload bytes
        }
        AuxValue::String(s) => {
            payload.extend_from_slice(&0x01_u32.to_le_bytes());
            payload.extend_from_slice(&(s.len() as u32).to_le_bytes());
            payload.extend_from_slice(s.as_bytes());
        }
        AuxValue::Array(bytes) => {
            payload.extend_from_slice(&0x02_u32.to_le_bytes());
            payload.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            payload.extend_from_slice(bytes);
        }
        AuxValue::U32(val) => {
            payload.extend_from_slice(&0x03_u32.to_le_bytes());
            payload.extend_from_slice(&val.to_le_bytes());
        }
        AuxValue::I64(val) => {
            payload.extend_from_slice(&0x06_u32.to_le_bytes());
            payload.extend_from_slice(&val.to_le_bytes());
        }
        AuxValue::Double(val) => {
            payload.extend_from_slice(&0x09_u32.to_le_bytes());
            payload.extend_from_slice(&val.to_le_bytes());
        }
        AuxValue::PrimitiveDictionary(_) => {
            // Nested dictionaries not typically used as primitive values
            // Write as empty dict
            payload.extend_from_slice(&0xf0_u32.to_le_bytes());
            payload.extend_from_slice(&0_u32.to_le_bytes());
            payload.extend_from_slice(&0u64.to_le_bytes());
        }
    }
}

impl AuxValue {
    /// Creates an auxiliary value containing NSKeyedArchived data
    ///
    /// # Arguments
    /// * `v` - Plist value to archive
    pub fn archived_value(v: impl Into<plist::Value>) -> Self {
        Self::Array(ns_keyed_archive::encode::encode_to_bytes(v.into()).expect("Failed to encode"))
    }

    /// Creates a primitive buffer (immutable buffer) containing raw bytes
    /// This is used for passing primitive arrays as DTX arguments (wire type 0x02)
    ///
    /// # Arguments
    /// * `bytes` - Raw bytes to include in the buffer
    pub fn primitive_buffer(bytes: Vec<u8>) -> Self {
        Self::Array(bytes)
    }

    /// Creates a PrimitiveDictionary auxiliary value (wire type 0xF0)
    ///
    /// # Arguments
    /// * `entries` - List of (key, [values]) pairs
    pub fn primitive_dictionary(entries: Vec<(AuxValue, Vec<AuxValue>)>) -> Self {
        Self::PrimitiveDictionary(entries)
    }

    /// Creates a DTX method call auxiliary argument format matching pymobiledevice3
    ///
    /// This wraps all arguments in a single PrimitiveDictionary with a NULL key,
    /// producing the structure `{PNULL: [arg1, arg2, ...]}`.
    ///
    /// This matches pymobiledevice3's `MessageAux.build()` which creates `PDict({PNULL: converted_list})`.
    ///
    /// # Arguments
    /// * `args` - List of arguments to wrap
    pub fn dtx_method_args(args: Vec<Self>) -> Self {
        // Create a PrimitiveDictionary with a single entry: (PNULL, [arg1, arg2, ...])
        Self::PrimitiveDictionary(vec![(Self::Null, args)])
    }
}

impl MessageHeader {
    /// Creates a new message header
    ///
    /// Note: Length field is updated during message serialization
    ///
    /// # Arguments
    /// * `fragment_id` - Identifier for message fragments
    /// * `fragment_count` - Total fragments in message
    /// * `identifier` - Unique message ID
    /// * `conversation_index` - Conversation tracking number
    /// * `channel` - Channel number
    /// * `expects_reply` - Whether response is expected
    pub fn new(
        fragment_id: u16,
        fragment_count: u16,
        identifier: u32,
        conversation_index: u32,
        channel: i32,
        expects_reply: bool,
    ) -> Self {
        Self {
            magic: 0x1F3D5B79,
            header_len: 32,
            fragment_id,
            fragment_count,
            length: 0,
            identifier,
            conversation_index,
            channel,
            expects_reply,
        }
    }

    /// Returns the message identifier. A reply carries the identifier of the
    /// message it answers.
    pub fn identifier(&self) -> u32 {
        self.identifier
    }

    /// Returns the conversation index: 0 for a message that starts an
    /// exchange, incremented by each reply in it.
    pub fn conversation_index(&self) -> u32 {
        self.conversation_index
    }

    /// Returns the channel code. For a received message this is normalized by
    /// conversation-index parity: the wire value is negated when the index is
    /// even, so a reply and its request share one code.
    pub fn channel(&self) -> i32 {
        self.channel
    }

    /// Returns whether the sender asked for a reply.
    pub fn expects_reply(&self) -> bool {
        self.expects_reply
    }

    /// Serializes header to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut res = Vec::new();
        res.extend_from_slice(&self.magic.to_le_bytes());
        res.extend_from_slice(&self.header_len.to_le_bytes());
        res.extend_from_slice(&self.fragment_id.to_le_bytes());
        res.extend_from_slice(&self.fragment_count.to_le_bytes());
        res.extend_from_slice(&self.length.to_le_bytes());
        res.extend_from_slice(&self.identifier.to_le_bytes());
        res.extend_from_slice(&self.conversation_index.to_le_bytes());
        res.extend_from_slice(&self.channel.to_le_bytes());
        res.extend_from_slice(&if self.expects_reply { 1_u32 } else { 0 }.to_le_bytes());

        res
    }
}

impl PayloadHeader {
    /// Creates a new payload header
    pub fn new() -> Self {
        Self::default()
    }

    /// Serializes header to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut res = vec![self.msg_type, self.flags_a, self.flags_b, self.reserved];
        res.extend_from_slice(&self.aux_length.to_le_bytes());
        res.extend_from_slice(&self.total_length.to_le_bytes());
        res.extend_from_slice(&self.flags.to_le_bytes());

        res
    }

    /// Returns the DTX message type byte (2 = method invocation, 3 = object
    /// reply, 4 = error reply; other values pass through unvalidated).
    pub fn message_type(&self) -> u8 {
        self.msg_type
    }

    /// Returns the payload header's trailing 32-bit flags word.
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// Creates header for method invocation messages
    pub fn method_invocation() -> Self {
        Self {
            msg_type: 2,
            ..Default::default()
        }
    }
}

impl Message {
    /// Reads and parses a message from an async reader, with the
    /// [`DEFAULT_MAX_MESSAGE_SIZE`] cap. See [`Message::from_reader_limited`].
    pub async fn from_reader<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self, IdeviceError> {
        Self::from_reader_limited(reader, DEFAULT_MAX_MESSAGE_SIZE).await
    }

    /// Reads one DTX message, reassembling fragments, with the reassembled
    /// body capped at `max_size` bytes.
    ///
    /// Framing is validated and any violation is a fatal error (the stream
    /// position is undefined afterwards): magic ([`DvtError::BadMagic`]),
    /// header length ([`DvtError::BadHeaderLength`]), fragments numbered
    /// 0..count in order with one identifier and count
    /// ([`DvtError::FragmentSequence`]; a multi-fragment message starts with a
    /// header-only fragment 0), the running reassembled size checked before each
    /// fragment is read ([`DvtError::MessageTooLarge`]), the 16-byte payload
    /// header ([`DvtError::ShortPayloadHeader`]) and `aux_length <= total_length
    /// <= remaining bytes` ([`DvtError::PayloadLength`]). I/O errors pass through.
    ///
    /// Aux and payload then decode independently and never fail a well-framed
    /// message: on failure the decoded field is `None`, `aux_error` /
    /// `data_error` holds the reason, and `raw_aux` / `raw_data` keep the bytes.
    ///
    /// The returned `channel` is normalized by conversation-index parity (wire
    /// value negated when the index is even).
    ///
    /// Bounds: O(L + 32·F) time for L reassembled bytes in F fragments, with
    /// L <= `max_size`; peak extra space about 2·L (the reassembly buffer, then
    /// the aux/data split) plus the decoded values.
    pub async fn from_reader_limited<R: AsyncRead + Unpin>(
        reader: &mut R,
        max_size: usize,
    ) -> Result<Self, IdeviceError> {
        let mut packet_data: Vec<u8> = Vec::new();
        let mut expected: Option<(u32, u16)> = None; // (identifier, fragment_count)
        let mut next_fragment: u16 = 0;
        let mheader = loop {
            let mut buf = [0u8; 32];
            reader.read_exact(&mut buf).await?;
            let header = Self::parse_header(&buf)?;

            let (identifier, count) =
                *expected.get_or_insert((header.identifier, header.fragment_count));
            if header.fragment_count == 0
                || header.identifier != identifier
                || header.fragment_count != count
                || header.fragment_id != next_fragment
            {
                return Err(DvtError::FragmentSequence {
                    identifier,
                    expected_id: next_fragment,
                    expected_count: count,
                    got_identifier: header.identifier,
                    got_id: header.fragment_id,
                    got_count: header.fragment_count,
                }
                .into());
            }
            next_fragment += 1;

            if header.fragment_count > 1 && header.fragment_id == 0 {
                // when reading multiple message fragments, the first fragment contains only a message header.
                continue;
            }

            let len = header.length as usize;
            let start = packet_data.len();
            let end = start
                .checked_add(len)
                .filter(|end| *end <= max_size)
                .ok_or(DvtError::MessageTooLarge {
                    size: start.saturating_add(len),
                    max: max_size,
                })?;
            packet_data.resize(end, 0);
            reader.read_exact(&mut packet_data[start..end]).await?;
            if header.fragment_id == header.fragment_count - 1 {
                break header;
            }
        };

        // read the payload header
        let Some(buf) = packet_data.get(0..16) else {
            return Err(DvtError::ShortPayloadHeader(packet_data.len()).into());
        };
        let pheader = PayloadHeader {
            msg_type: buf[0],
            flags_a: buf[1],
            flags_b: buf[2],
            reserved: buf[3],
            aux_length: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            total_length: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            flags: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
        };
        let available = packet_data.len() - 16;
        if pheader.aux_length > pheader.total_length || pheader.total_length as usize > available {
            return Err(DvtError::PayloadLength {
                aux_length: pheader.aux_length,
                total_length: pheader.total_length,
                available,
            }
            .into());
        }

        // Split [16..16+aux) and [16+aux..16+total) without further copies of the prefix.
        let mut body = packet_data.split_off(16);
        body.truncate(pheader.total_length as usize);
        let data_bytes = body.split_off(pheader.aux_length as usize);
        let aux_bytes = body;

        let (aux, raw_aux, aux_error) = if aux_bytes.is_empty() {
            (None, None, None)
        } else {
            match Aux::parse(&aux_bytes) {
                Ok(aux) => (Some(aux), Some(aux_bytes), None),
                Err(e) => (None, Some(aux_bytes), Some(e.to_string())),
            }
        };
        let (data, raw_data, data_error) = if data_bytes.is_empty() {
            (None, None, None)
        } else {
            match ns_keyed_archive::decode::from_bytes(&data_bytes) {
                Ok(v) => (Some(v), Some(data_bytes), None),
                Err(e) => (
                    None,
                    Some(data_bytes),
                    Some(format!("NSKeyedArchive decode failed: {e}")),
                ),
            }
        };

        Ok(Message {
            message_header: mheader,
            payload_header: pheader,
            aux,
            data,
            raw_data,
            raw_aux,
            aux_error,
            data_error,
        })
    }

    fn parse_header(buf: &[u8; 32]) -> Result<MessageHeader, IdeviceError> {
        let u32_at = |i: usize| u32::from_le_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]);
        let magic = u32_at(0);
        if magic != DTX_MAGIC {
            return Err(DvtError::BadMagic(magic).into());
        }
        let header_len = u32_at(4);
        if header_len != 32 {
            return Err(DvtError::BadHeaderLength(header_len).into());
        }
        let conversation_index = u32_at(20);
        let wire_channel = u32_at(24) as i32;
        Ok(MessageHeader {
            magic,
            header_len,
            fragment_id: u16::from_le_bytes([buf[8], buf[9]]),
            fragment_count: u16::from_le_bytes([buf[10], buf[11]]),
            length: u32_at(12),
            identifier: u32_at(16),
            conversation_index,
            channel: if conversation_index.is_multiple_of(2) {
                wire_channel.wrapping_neg()
            } else {
                wire_channel
            },
            expects_reply: u32_at(28) == 1,
        })
    }

    /// Returns the message identifier (see [`MessageHeader::identifier`]).
    pub fn identifier(&self) -> u32 {
        self.message_header.identifier
    }

    /// Returns the conversation index (see [`MessageHeader::conversation_index`]).
    pub fn conversation_index(&self) -> u32 {
        self.message_header.conversation_index
    }

    /// Returns the parity-normalized channel code (see [`MessageHeader::channel`]).
    pub fn channel(&self) -> i32 {
        self.message_header.channel
    }

    /// Returns whether the sender asked for a reply.
    pub fn expects_reply(&self) -> bool {
        self.message_header.expects_reply
    }

    /// Returns the DTX message type byte (see [`PayloadHeader::message_type`]).
    pub fn message_type(&self) -> u8 {
        self.payload_header.msg_type
    }

    /// Returns the payload header flags word.
    pub fn flags(&self) -> u32 {
        self.payload_header.flags
    }

    /// Creates a new message
    ///
    /// # Arguments
    /// * `message_header` - Message metadata
    /// * `payload_header` - Payload description  
    /// * `aux` - Optional auxiliary data
    /// * `data` - Optional payload data
    pub fn new(
        message_header: MessageHeader,
        payload_header: PayloadHeader,
        aux: Option<Aux>,
        data: Option<Value>,
    ) -> Self {
        Self {
            message_header,
            payload_header,
            aux,
            data,
            raw_data: None,
            raw_aux: None,
            aux_error: None,
            data_error: None,
        }
    }

    /// Serializes message to bytes
    ///
    /// Updates length fields in headers automatically
    pub fn serialize(&self) -> Vec<u8> {
        let aux = match &self.aux {
            Some(a) => a.serialize(),
            None => Vec::new(),
        };
        let data = match &self.data {
            Some(d) => ns_keyed_archive::encode::encode_to_bytes(d.to_owned())
                .expect("Failed to encode value"),
            None => Vec::new(),
        };

        // Update the payload header
        let mut payload_header = self.payload_header.to_owned();
        payload_header.aux_length = aux.len() as u32;
        payload_header.total_length = (aux.len() + data.len()) as u32;
        let payload_header = payload_header.serialize();

        // Update the message header
        let mut message_header = self.message_header.to_owned();
        message_header.length = (payload_header.len() + aux.len() + data.len()) as u32;

        let mut res = Vec::new();
        res.extend_from_slice(&message_header.serialize());
        res.extend_from_slice(&payload_header);
        res.extend_from_slice(&aux);
        res.extend_from_slice(&data);

        res
    }

    /// Builds a raw reply frame for an incoming message, sending `data_bytes`
    /// verbatim as the payload without additional NSKeyedArchive encoding.
    ///
    /// This is used for replies where the payload is already a serialised
    /// NSKeyedArchive (e.g. `XCTestConfiguration`).  Pass an empty slice to
    /// send an acknowledgement with no payload.
    pub(crate) fn build_raw_reply(
        channel: i32,
        incoming_msg_id: u32,
        incoming_conversation_index: u32,
        data_bytes: &[u8],
    ) -> Vec<u8> {
        // Payload header (16 bytes): flags=0, aux_len=0, total_len
        let msg_type: u8 = if data_bytes.is_empty() { 0 } else { 3 };
        let flags_a: u8 = 0;
        let flags_b: u8 = 0;
        let reserved: u8 = 0;
        let aux_len: u32 = 0;
        let total_len: u32 = data_bytes.len() as u32;

        let payload_total = 16usize + data_bytes.len(); // payload_hdr + data

        // Message header (32 bytes)
        let magic: u32 = 0x1F3D5B79;
        let header_len: u32 = 32;
        let fragment_id: u16 = 0;
        let fragment_count: u16 = 1;
        let length: u32 = payload_total as u32;
        let conversation_index = incoming_conversation_index + 1;
        let expects_reply: u32 = 0;
        let wire_channel = if conversation_index.is_multiple_of(2) {
            channel
        } else {
            -channel
        };

        let mut buf = Vec::with_capacity(32 + 16 + data_bytes.len());
        buf.extend_from_slice(&magic.to_le_bytes());
        buf.extend_from_slice(&header_len.to_le_bytes());
        buf.extend_from_slice(&fragment_id.to_le_bytes());
        buf.extend_from_slice(&fragment_count.to_le_bytes());
        buf.extend_from_slice(&length.to_le_bytes());
        buf.extend_from_slice(&incoming_msg_id.to_le_bytes());
        buf.extend_from_slice(&conversation_index.to_le_bytes());
        buf.extend_from_slice(&wire_channel.to_le_bytes());
        buf.extend_from_slice(&expects_reply.to_le_bytes());
        // Payload header
        buf.push(msg_type);
        buf.push(flags_a);
        buf.push(flags_b);
        buf.push(reserved);
        buf.extend_from_slice(&aux_len.to_le_bytes());
        buf.extend_from_slice(&total_len.to_le_bytes());
        buf.extend_from_slice(&0_u32.to_le_bytes());
        // Data
        buf.extend_from_slice(data_bytes);
        buf
    }
}

impl std::fmt::Debug for AuxValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuxValue::Null => write!(f, "Null"),
            AuxValue::String(s) => write!(f, "String({s:?})"),
            AuxValue::Array(arr) => write!(
                f,
                "Array(len={}, first_bytes={:?})",
                arr.len(),
                &arr[..arr.len().min(10)]
            ),
            AuxValue::U32(n) => write!(f, "U32({n})"),
            AuxValue::I64(n) => write!(f, "I64({n})"),
            AuxValue::Double(d) => write!(f, "Double({d})"),
            AuxValue::PrimitiveDictionary(_) => write!(f, "PrimitiveDictionary"),
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("message_header", &self.message_header)
            .field("payload_header", &self.payload_header)
            .field("aux", &self.aux)
            .field("data", &self.data.as_ref().map(pretty_print_plist))
            .field("raw_aux_len", &self.raw_aux.as_ref().map(Vec::len))
            .field("raw_data_len", &self.raw_data.as_ref().map(Vec::len))
            .field("aux_error", &self.aux_error)
            .field("data_error", &self.data_error)
            .finish()
    }
}

/// Synthetic DTX frame builders shared by the parser and client tests.
#[cfg(test)]
pub(crate) mod test_frames {
    /// 16-byte payload header + aux + data, with lengths taken from the slices.
    pub(crate) fn body(msg_type: u8, aux: &[u8], data: &[u8]) -> Vec<u8> {
        body_with_lengths(
            msg_type,
            aux.len() as u32,
            (aux.len() + data.len()) as u32,
            aux,
            data,
        )
    }

    /// Payload with explicit (possibly lying) `aux_length` / `total_length`.
    pub(crate) fn body_with_lengths(
        msg_type: u8,
        aux_length: u32,
        total_length: u32,
        aux: &[u8],
        data: &[u8],
    ) -> Vec<u8> {
        let mut b = vec![msg_type, 0, 0, 0];
        b.extend_from_slice(&aux_length.to_le_bytes());
        b.extend_from_slice(&total_length.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(aux);
        b.extend_from_slice(data);
        b
    }

    /// 32-byte message header declaring `length`, followed by `payload`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame(
        identifier: u32,
        conversation_index: u32,
        wire_channel: i32,
        expects_reply: bool,
        fragment_id: u16,
        fragment_count: u16,
        length: u32,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&0x1F3D5B79u32.to_le_bytes());
        f.extend_from_slice(&32u32.to_le_bytes());
        f.extend_from_slice(&fragment_id.to_le_bytes());
        f.extend_from_slice(&fragment_count.to_le_bytes());
        f.extend_from_slice(&length.to_le_bytes());
        f.extend_from_slice(&identifier.to_le_bytes());
        f.extend_from_slice(&conversation_index.to_le_bytes());
        f.extend_from_slice(&wire_channel.to_le_bytes());
        f.extend_from_slice(&(expects_reply as u32).to_le_bytes());
        f.extend_from_slice(payload);
        f
    }

    /// One unfragmented frame carrying `payload`.
    pub(crate) fn single(
        identifier: u32,
        conversation_index: u32,
        wire_channel: i32,
        payload: &[u8],
    ) -> Vec<u8> {
        frame(
            identifier,
            conversation_index,
            wire_channel,
            false,
            0,
            1,
            payload.len() as u32,
            payload,
        )
    }

    pub(crate) fn archive(v: impl Into<plist::Value>) -> Vec<u8> {
        ns_keyed_archive::encode::encode_to_bytes(v.into()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::test_frames::*;
    use super::*;

    async fn read(bytes: &[u8], max: usize) -> Result<Message, IdeviceError> {
        let mut r = bytes;
        Message::from_reader_limited(&mut r, max).await
    }

    fn dvt_err(r: Result<Message, IdeviceError>) -> DvtError {
        match r {
            Err(IdeviceError::Dvt(e)) => e,
            other => panic!("expected DvtError, got {other:?}"),
        }
    }

    /// Legacy aux holding one String value.
    fn legacy_aux_string(s: &str) -> Vec<u8> {
        Aux::from_values(vec![AuxValue::String(s.into())]).serialize()
    }

    /// Splits `payload` into a header-only fragment 0 plus two body fragments.
    fn three_fragments(
        identifier: u32,
        payload: &[u8],
        split: usize,
    ) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (a, b) = payload.split_at(split);
        (
            frame(identifier, 0, 1, false, 0, 3, payload.len() as u32, &[]),
            frame(identifier, 0, 1, false, 1, 3, a.len() as u32, a),
            frame(identifier, 0, 1, false, 2, 3, b.len() as u32, b),
        )
    }

    #[tokio::test]
    async fn three_fragment_message_is_reassembled() {
        let aux = legacy_aux_string("arg");
        let payload = body(2, &aux, &archive("selector:"));
        let (f0, f1, f2) = three_fragments(7, &payload, 20);
        let msg = read(&[f0, f1, f2].concat(), DEFAULT_MAX_MESSAGE_SIZE)
            .await
            .unwrap();
        assert_eq!(msg.identifier(), 7);
        assert_eq!(msg.message_type(), 2);
        assert_eq!(
            msg.channel(),
            -1,
            "even conversation index negates the wire channel"
        );
        assert_eq!(msg.data, Some(plist::Value::String("selector:".into())));
        assert_eq!(
            msg.aux.unwrap().values,
            vec![AuxValue::String("arg".into())]
        );
        assert_eq!(msg.raw_aux, Some(aux));
        assert!(msg.aux_error.is_none() && msg.data_error.is_none());
    }

    #[tokio::test]
    async fn out_of_order_fragment_is_rejected() {
        let payload = body(2, &[], &archive("x"));
        let (f0, _f1, f2) = three_fragments(7, &payload, 20);
        let e = dvt_err(read(&[f0, f2].concat(), DEFAULT_MAX_MESSAGE_SIZE).await);
        assert!(
            matches!(
                e,
                DvtError::FragmentSequence {
                    expected_id: 1,
                    got_id: 2,
                    ..
                }
            ),
            "{e:?}"
        );
    }

    #[tokio::test]
    async fn fragment_with_other_identifier_is_rejected() {
        let payload = body(2, &[], &archive("x"));
        let (f0, _, _) = three_fragments(7, &payload, 20);
        let (_, g1, _) = three_fragments(8, &payload, 20);
        let e = dvt_err(read(&[f0, g1].concat(), DEFAULT_MAX_MESSAGE_SIZE).await);
        assert!(
            matches!(
                e,
                DvtError::FragmentSequence {
                    identifier: 7,
                    got_identifier: 8,
                    ..
                }
            ),
            "{e:?}"
        );
    }

    #[tokio::test]
    async fn reassembled_size_over_cap_is_rejected_before_last_fragment_is_read() {
        let payload = body(2, &[], &archive("a longer selector string:"));
        let (f0, f1, f2) = three_fragments(7, &payload, 20);
        // Only the last fragment's header is present; reading its body would be EOF,
        // so a MessageTooLarge result proves the cap was checked first.
        let stream = [f0, f1, f2[..32].to_vec()].concat();
        let e = dvt_err(read(&stream, payload.len() - 1).await);
        assert!(
            matches!(e, DvtError::MessageTooLarge { size, max } if size == payload.len() && max == payload.len() - 1),
            "{e:?}"
        );
        // Each fragment alone is under the cap: the cap is on the reassembled total.
        assert!(20 < payload.len() - 1 && payload.len() - 20 < payload.len() - 1);
    }

    #[tokio::test]
    async fn aux_length_over_total_length_is_rejected() {
        let payload = body_with_lengths(2, 40, 8, &[0; 40], &[]);
        let e = dvt_err(read(&single(1, 0, 0, &payload), DEFAULT_MAX_MESSAGE_SIZE).await);
        assert!(
            matches!(
                e,
                DvtError::PayloadLength {
                    aux_length: 40,
                    total_length: 8,
                    ..
                }
            ),
            "{e:?}"
        );
    }

    #[tokio::test]
    async fn total_length_past_body_is_rejected() {
        let payload = body_with_lengths(2, 0, 1000, &[], &[1, 2, 3]);
        let e = dvt_err(read(&single(1, 0, 0, &payload), DEFAULT_MAX_MESSAGE_SIZE).await);
        assert!(
            matches!(
                e,
                DvtError::PayloadLength {
                    total_length: 1000,
                    available: 3,
                    ..
                }
            ),
            "{e:?}"
        );
    }

    #[tokio::test]
    async fn body_shorter_than_payload_header_is_rejected() {
        let e = dvt_err(read(&single(1, 0, 0, &[2, 0, 0]), DEFAULT_MAX_MESSAGE_SIZE).await);
        assert!(matches!(e, DvtError::ShortPayloadHeader(3)), "{e:?}");
    }

    #[tokio::test]
    async fn bad_magic_is_rejected() {
        let mut f = single(1, 0, 0, &body(2, &[], &[]));
        f[0] ^= 0xFF;
        assert!(matches!(
            dvt_err(read(&f, DEFAULT_MAX_MESSAGE_SIZE).await),
            DvtError::BadMagic(_)
        ));
    }

    #[test]
    fn truncated_aux_string_is_an_error_not_a_panic() {
        let mut aux = legacy_aux_string("abc");
        aux.truncate(aux.len() - 2);
        // `Aux::serialize` writes buffer size 496 (first byte 0xF0), so this parses
        // as the modern PrimitiveDictionary form and the body length is what overruns.
        let e = Aux::parse(&aux).unwrap_err();
        assert!(
            matches!(
                e,
                IdeviceError::Dvt(DvtError::AuxTruncated {
                    needed: 15,
                    available: 13
                })
            ),
            "{e:?}"
        );
        // Legacy form (first byte not 0xF0): the string length overruns instead.
        let mut legacy = vec![0u8; 16];
        legacy.extend_from_slice(&1u32.to_le_bytes());
        legacy.extend_from_slice(&3u32.to_le_bytes());
        legacy.push(b'a');
        let e = Aux::parse(&legacy).unwrap_err();
        assert!(
            matches!(
                e,
                IdeviceError::Dvt(DvtError::AuxTruncated {
                    needed: 3,
                    available: 1
                })
            ),
            "{e:?}"
        );
    }

    #[test]
    fn huge_declared_aux_lengths_do_not_allocate_or_panic() {
        // Modern PrimitiveDictionary whose body_len would overflow 16 + body_len.
        let mut modern = vec![0xF0, 0, 0, 0, 0, 0, 0, 0];
        modern.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            Aux::parse(&modern),
            Err(IdeviceError::Dvt(DvtError::AuxTruncated { .. }))
        ));
        // Legacy Array value declaring 4 GiB.
        let mut legacy = vec![0u8; 16];
        legacy.extend_from_slice(&2u32.to_le_bytes());
        legacy.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            Aux::parse(&legacy),
            Err(IdeviceError::Dvt(DvtError::AuxTruncated { .. }))
        ));
        // Legacy embedded 0xF0 block skipping past the end.
        let mut skip = vec![0u8; 16];
        skip.extend_from_slice(&0xF0u32.to_le_bytes());
        skip.extend_from_slice(&0u32.to_le_bytes());
        skip.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(matches!(
            Aux::parse(&skip),
            Err(IdeviceError::Dvt(DvtError::AuxTruncated { .. }))
        ));
    }

    #[tokio::test]
    async fn truncated_aux_inside_a_message_is_captured() {
        let mut aux = legacy_aux_string("abc");
        aux.truncate(aux.len() - 2);
        let data = archive("ok");
        let msg = read(
            &single(1, 1, 0, &body(3, &aux, &data)),
            DEFAULT_MAX_MESSAGE_SIZE,
        )
        .await
        .unwrap();
        assert!(msg.aux.is_none());
        assert_eq!(msg.raw_aux, Some(aux));
        assert!(msg.aux_error.unwrap().contains("truncated aux"));
        assert_eq!(
            msg.data,
            Some(plist::Value::String("ok".into())),
            "data decodes independently"
        );
    }

    #[tokio::test]
    async fn unknown_aux_type_yields_aux_error_and_raw_aux() {
        let mut aux = vec![0u8; 16];
        aux.extend_from_slice(&0x77u32.to_le_bytes());
        let msg = read(
            &single(1, 1, 0, &body(3, &aux, &[])),
            DEFAULT_MAX_MESSAGE_SIZE,
        )
        .await
        .unwrap();
        assert!(msg.aux.is_none());
        assert_eq!(msg.raw_aux, Some(aux));
        assert!(
            msg.aux_error
                .unwrap()
                .contains("Unknown aux value type: 119")
        );
        assert!(msg.data.is_none() && msg.raw_data.is_none() && msg.data_error.is_none());
    }

    #[tokio::test]
    async fn invalid_archive_yields_data_error_and_raw_data() {
        let aux = legacy_aux_string("still decoded");
        let junk = b"not an archive".to_vec();
        let msg = read(
            &single(1, 1, 0, &body(3, &aux, &junk)),
            DEFAULT_MAX_MESSAGE_SIZE,
        )
        .await
        .unwrap();
        assert!(msg.data.is_none());
        assert_eq!(msg.raw_data, Some(junk));
        assert!(
            msg.data_error
                .unwrap()
                .starts_with("NSKeyedArchive decode failed")
        );
        assert_eq!(
            msg.aux.unwrap().values,
            vec![AuxValue::String("still decoded".into())]
        );
    }
}
