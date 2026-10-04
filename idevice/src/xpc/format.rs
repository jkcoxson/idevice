use plist_macro::plist;
use std::{
    ffi::CString,
    ops::{BitOr, BitOrAssign},
};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::errors::XpcError;
use crate::{CdTunnelError, IdeviceError};

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum XPCFlag {
    AlwaysSet,
    DataFlag,
    Reply,
    WantingReply,
    InitHandshake,

    FileTxStreamRequest,
    FileTxStreamResponse,

    Custom(u32),
}

impl From<XPCFlag> for u32 {
    fn from(value: XPCFlag) -> Self {
        match value {
            XPCFlag::AlwaysSet => 0x00000001,
            XPCFlag::DataFlag => 0x00000100,
            XPCFlag::Reply => 0x00020000,
            XPCFlag::WantingReply => 0x00010000,
            XPCFlag::InitHandshake => 0x00400000,
            XPCFlag::FileTxStreamRequest => 0x00100000,
            XPCFlag::FileTxStreamResponse => 0x00200000,
            XPCFlag::Custom(inner) => inner,
        }
    }
}

impl BitOr for XPCFlag {
    fn bitor(self, rhs: Self) -> Self::Output {
        XPCFlag::Custom(u32::from(self) | u32::from(rhs))
    }

    type Output = XPCFlag;
}

impl BitOrAssign for XPCFlag {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.bitor(rhs);
    }
}

impl PartialEq for XPCFlag {
    fn eq(&self, other: &Self) -> bool {
        u32::from(*self) == u32::from(*other)
    }
}

#[repr(u32)]
pub enum XPCType {
    Null = 0x00001000,
    Bool = 0x00002000,
    Dictionary = 0x0000f000,
    Array = 0x0000e000,

    Int64 = 0x00003000,
    UInt64 = 0x00004000,
    Double = 0x00005000,

    Date = 0x00007000,

    String = 0x00009000,
    Data = 0x00008000,
    Uuid = 0x0000a000,
    FileTransfer = 0x0001a000,
}

impl TryFrom<u32> for XPCType {
    type Error = IdeviceError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x00001000 => Ok(Self::Null),
            0x00002000 => Ok(Self::Bool),
            0x0000f000 => Ok(Self::Dictionary),
            0x0000e000 => Ok(Self::Array),
            0x00003000 => Ok(Self::Int64),
            0x00005000 => Ok(Self::Double),
            0x00004000 => Ok(Self::UInt64),
            0x00007000 => Ok(Self::Date),
            0x00009000 => Ok(Self::String),
            0x00008000 => Ok(Self::Data),
            0x0000a000 => Ok(Self::Uuid),
            0x0001a000 => Ok(Self::FileTransfer),
            _ => Err(XpcError::UnknownXpcType(value))?,
        }
    }
}

pub type Dictionary = IndexMap<String, XPCObject>;

/// Default cap on a whole XPC wrapper (24-byte header plus body), in bytes.
pub const DEFAULT_MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Maximum nesting of dictionaries, arrays and file transfers in a decoded
/// object; the root container is depth 1.
pub const MAX_NESTING_DEPTH: usize = 64;

/// Fixed XPC message-wrapper header: magic + flags + body length + message id.
pub(crate) const XPC_WRAPPER_LEN: usize = 24;

const WRAPPER_MAGIC: u32 = 0x29b00b92;

/// Bounds-checked cursor over a peer-supplied byte slice. Every read checks
/// the requested length against the bytes remaining before touching or
/// allocating anything, so a hostile length can never over-allocate or panic.
struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// The next `n` bytes, or `LengthOutOfBounds` if fewer remain.
    fn take(&mut self, n: usize) -> Result<&'a [u8], IdeviceError> {
        let remaining = self.remaining();
        if n > remaining {
            return Err(XpcError::LengthOutOfBounds {
                declared: n as u64,
                remaining,
            }
            .into());
        }
        let out = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], IdeviceError> {
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    fn u32(&mut self) -> Result<u32, IdeviceError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, IdeviceError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// A length field checked against the remaining bytes and `max`.
    fn len(&mut self, max: usize) -> Result<usize, IdeviceError> {
        let declared = self.u32()?;
        let remaining = self.remaining();
        match usize::try_from(declared) {
            Ok(l) if l <= remaining && l <= max => Ok(l),
            _ => Err(XpcError::LengthOutOfBounds {
                declared: declared as u64,
                remaining: remaining.min(max),
            }
            .into()),
        }
    }

    /// Skips alignment padding. Lenient as before: a message whose final
    /// padding is missing still decodes.
    fn skip_padding(&mut self, n: usize) {
        self.pos += n.min(self.remaining());
    }

    /// A NUL-terminated UTF-8 key, terminator consumed.
    fn cstr_until_nul(&mut self) -> Result<String, IdeviceError> {
        let rest = &self.buf[self.pos..];
        let Some(nul) = rest.iter().position(|&b| b == 0) else {
            return Err(XpcError::InvalidCString.into());
        };
        let bytes = self.take(nul + 1)?;
        cstring_to_string(bytes.to_vec())
    }
}

fn cstring_to_string(bytes: Vec<u8>) -> Result<String, IdeviceError> {
    CString::from_vec_with_nul(bytes)
        .ok()
        .and_then(|x| x.into_string().ok())
        .ok_or_else(|| XpcError::InvalidCString.into())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum XPCObject {
    Null,
    Bool(bool),
    Dictionary(Dictionary),
    Array(Vec<XPCObject>),

    Double(f64),
    Int64(i64),
    UInt64(u64),

    Date(std::time::SystemTime),

    String(String),
    Data(Vec<u8>),
    Uuid(uuid::Uuid),

    FileTransfer { msg_id: u64, data: Box<XPCObject> },
}

impl From<plist::Value> for XPCObject {
    fn from(value: plist::Value) -> Self {
        match value {
            plist::Value::Array(v) => {
                XPCObject::Array(v.iter().map(|item| XPCObject::from(item.clone())).collect())
            }
            plist::Value::Dictionary(v) => {
                let mut dict = Dictionary::new();
                for (k, v) in v.into_iter() {
                    dict.insert(k.clone(), XPCObject::from(v));
                }
                XPCObject::Dictionary(dict)
            }
            plist::Value::Boolean(v) => XPCObject::Bool(v),
            plist::Value::Data(v) => XPCObject::Data(v),
            plist::Value::Date(_) => todo!(),
            plist::Value::Real(f) => XPCObject::Double(f),
            plist::Value::Integer(v) => XPCObject::Int64(v.as_signed().unwrap()),
            plist::Value::String(v) => XPCObject::String(v),
            plist::Value::Uid(_) => todo!(),
            _ => todo!(),
        }
    }
}

impl XPCObject {
    pub fn to_plist(&self) -> plist::Value {
        match self {
            Self::Null => plist::Value::String("".into()),
            Self::Bool(v) => plist::Value::Boolean(*v),
            Self::Uuid(uuid) => plist::Value::String(uuid.to_string()),
            Self::Double(f) => plist::Value::Real(*f),
            Self::UInt64(v) => plist::Value::Integer({ *v }.into()),
            Self::Int64(v) => plist::Value::Integer({ *v }.into()),
            Self::Date(d) => plist::Value::Date(plist::Date::from(*d)),
            Self::String(v) => plist::Value::String(v.clone()),
            Self::Data(v) => plist::Value::Data(v.clone()),
            Self::Array(v) => plist::Value::Array(v.iter().map(|item| item.to_plist()).collect()),
            Self::Dictionary(v) => {
                let mut dict = plist::Dictionary::new();
                for (k, v) in v.into_iter() {
                    dict.insert(k.clone(), v.to_plist());
                }
                plist::Value::Dictionary(dict)
            }
            Self::FileTransfer { msg_id, data } => {
                plist!({
                    "msg_id": *msg_id,
                    "data": data.to_plist(),
                })
            }
        }
    }

    /// Serializes the object with the XPC object header (magic, version 5).
    ///
    /// Fails with [`XpcError::DateOutOfRange`] for a `Date` before the Unix
    /// epoch or more than `u64::MAX` nanoseconds after it; no other variant
    /// fails.
    pub fn encode(&self) -> Result<Vec<u8>, IdeviceError> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&0x42133742_u32.to_le_bytes());
        buf.extend_from_slice(&0x00000005_u32.to_le_bytes());
        self.encode_object(&mut buf)?;
        Ok(buf)
    }

    fn encode_object(&self, buf: &mut Vec<u8>) -> Result<(), IdeviceError> {
        match self {
            XPCObject::Null => buf.extend_from_slice(&(XPCType::Null as u32).to_le_bytes()),
            XPCObject::Bool(val) => {
                buf.extend_from_slice(&(XPCType::Bool as u32).to_le_bytes());
                buf.push(if *val { 1 } else { 0 });
                buf.extend_from_slice(&[0].repeat(3));
            }
            XPCObject::Dictionary(dict) => {
                buf.extend_from_slice(&(XPCType::Dictionary as u32).to_le_bytes());
                let mut content_buf = Vec::new();
                content_buf.extend_from_slice(&(dict.len() as u32).to_le_bytes());
                for (k, v) in dict {
                    let padding = Self::calculate_padding(k.len() + 1);
                    content_buf.extend_from_slice(k.as_bytes());
                    content_buf.push(0);
                    content_buf.extend_from_slice(&[0].repeat(padding));
                    v.encode_object(&mut content_buf)?;
                }
                buf.extend_from_slice(&(content_buf.len() as u32).to_le_bytes());
                buf.extend_from_slice(&content_buf);
            }
            XPCObject::Array(items) => {
                buf.extend_from_slice(&(XPCType::Array as u32).to_le_bytes());
                let mut content_buf = Vec::new();
                content_buf.extend_from_slice(&(items.len() as u32).to_le_bytes());
                for item in items {
                    item.encode_object(&mut content_buf)?;
                }
                buf.extend_from_slice(&(content_buf.len() as u32).to_le_bytes());
                buf.extend_from_slice(&content_buf);
            }

            XPCObject::Double(f) => {
                buf.extend_from_slice(&(XPCType::Double as u32).to_le_bytes());
                buf.extend_from_slice(&f.to_le_bytes());
            }
            XPCObject::Int64(num) => {
                buf.extend_from_slice(&(XPCType::Int64 as u32).to_le_bytes());
                buf.extend_from_slice(&num.to_le_bytes());
            }
            XPCObject::UInt64(num) => {
                buf.extend_from_slice(&(XPCType::UInt64 as u32).to_le_bytes());
                buf.extend_from_slice(&num.to_le_bytes());
            }
            XPCObject::Date(date) => {
                let nanos = date
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|d| u64::try_from(d.as_nanos()).ok())
                    .ok_or(XpcError::DateOutOfRange)?;
                buf.extend_from_slice(&(XPCType::Date as u32).to_le_bytes());
                buf.extend_from_slice(&nanos.to_le_bytes());
            }
            XPCObject::String(item) => {
                let l = item.len() + 1;
                let padding = Self::calculate_padding(l);
                buf.extend_from_slice(&(XPCType::String as u32).to_le_bytes());
                buf.extend_from_slice(&(l as u32).to_le_bytes());
                buf.extend_from_slice(item.as_bytes());
                buf.push(0);
                buf.extend_from_slice(&[0].repeat(padding));
            }
            XPCObject::Data(data) => {
                let l = data.len();
                let padding = Self::calculate_padding(l);
                buf.extend_from_slice(&(XPCType::Data as u32).to_le_bytes());
                buf.extend_from_slice(&(l as u32).to_le_bytes());
                buf.extend_from_slice(data);
                buf.extend_from_slice(&[0].repeat(padding));
            }
            XPCObject::Uuid(uuid) => {
                buf.extend_from_slice(&(XPCType::Uuid as u32).to_le_bytes());
                buf.extend_from_slice(uuid.as_bytes());
            }
            XPCObject::FileTransfer { msg_id, data } => {
                buf.extend_from_slice(&(XPCType::FileTransfer as u32).to_le_bytes());
                buf.extend_from_slice(&msg_id.to_le_bytes());
                data.encode_object(buf)?;
            }
        }
        Ok(())
    }

    /// Decodes an XPC object (magic, version, one object) from `buf`, with
    /// [`DEFAULT_MAX_MESSAGE_SIZE`] as the cap on any nested length.
    ///
    /// Errors: `NotEnoughBytes` under 8 bytes, `InvalidXpcMagic`,
    /// `UnexpectedXpcVersion`, `LengthOutOfBounds` for a field running past the
    /// buffer, `NestingTooDeep` past [`MAX_NESTING_DEPTH`], `UnknownXpcType`,
    /// `InvalidCString`. O(len) time; see [`XPCMessage::decode_with_limit`].
    pub fn decode(buf: &[u8]) -> Result<Self, IdeviceError> {
        Self::decode_with_limit(buf, DEFAULT_MAX_MESSAGE_SIZE)
    }

    fn decode_with_limit(buf: &[u8], max: usize) -> Result<Self, IdeviceError> {
        if buf.len() < 8 {
            return Err(IdeviceError::NotEnoughBytes(buf.len(), 8));
        }
        let mut r = Reader { buf, pos: 0 };
        if r.u32()? != 0x42133742 {
            warn!("Invalid magic for XPCObject");
            return Err(XpcError::InvalidXpcMagic.into());
        }
        if r.u32()? != 0x00000005 {
            warn!("Unexpected version for XPCObject");
            return Err(XpcError::UnexpectedXpcVersion.into());
        }
        Self::decode_object(&mut r, max, 0)
    }

    /// Decodes one object at nesting `depth` (number of enclosing containers).
    /// Recursion is bounded by [`MAX_NESTING_DEPTH`]; every allocation is sized
    /// from bytes already proven present.
    fn decode_object(r: &mut Reader<'_>, max: usize, depth: usize) -> Result<Self, IdeviceError> {
        let xpc_type: XPCType = r.u32()?.try_into()?;
        let nested = |depth: usize| {
            if depth >= MAX_NESTING_DEPTH {
                Err(IdeviceError::from(XpcError::NestingTooDeep {
                    max: MAX_NESTING_DEPTH,
                }))
            } else {
                Ok(depth + 1)
            }
        };
        match xpc_type {
            XPCType::Null => Ok(XPCObject::Null),
            XPCType::Dictionary => {
                let depth = nested(depth)?;
                // Byte length of count + entries; checked, not otherwise used.
                r.len(max)?;
                let num_entries = r.u32()?;
                // Each entry needs at least 5 bytes (NUL key terminator and a
                // type; padding is skipped leniently, so not counted).
                check_count(num_entries, 5, r.remaining())?;
                let mut ret = IndexMap::new();
                for _ in 0..num_entries {
                    let key = r.cstr_until_nul()?;
                    r.skip_padding(Self::calculate_padding(key.len() + 1));
                    let value = Self::decode_object(r, max, depth)?;
                    ret.insert(key, value);
                }
                Ok(XPCObject::Dictionary(ret))
            }
            XPCType::Array => {
                let depth = nested(depth)?;
                r.len(max)?;
                let num_entries = r.u32()?;
                // Each element needs at least its 4-byte type.
                check_count(num_entries, 4, r.remaining())?;
                let mut ret = Vec::new();
                for _ in 0..num_entries {
                    ret.push(Self::decode_object(r, max, depth)?);
                }
                Ok(XPCObject::Array(ret))
            }
            XPCType::Double => Ok(XPCObject::Double(f64::from_le_bytes(r.array()?))),
            XPCType::Int64 => Ok(XPCObject::Int64(i64::from_le_bytes(r.array()?))),
            XPCType::UInt64 => Ok(XPCObject::UInt64(r.u64()?)),
            XPCType::Date => Ok(XPCObject::Date(
                std::time::UNIX_EPOCH + std::time::Duration::from_nanos(r.u64()?),
            )),
            XPCType::String => {
                // 'l' includes the NUL terminator.
                let l = r.len(max)?;
                let s = cstring_to_string(r.take(l)?.to_vec())?;
                r.skip_padding(Self::calculate_padding(l));
                Ok(XPCObject::String(s))
            }
            XPCType::Bool => {
                let b: [u8; 4] = r.array()?;
                Ok(XPCObject::Bool(b[0] != 0))
            }
            XPCType::Data => {
                let l = r.len(max)?;
                let data = r.take(l)?.to_vec();
                r.skip_padding(Self::calculate_padding(l));
                Ok(XPCObject::Data(data))
            }
            XPCType::Uuid => Ok(XPCObject::Uuid(
                uuid::Builder::from_bytes(r.array()?).into_uuid(),
            )),
            XPCType::FileTransfer => {
                let depth = nested(depth)?;
                let msg_id = r.u64()?;
                // The next thing in the stream is a full XPC object
                let inner = Self::decode_object(r, max, depth)?;
                Ok(XPCObject::FileTransfer {
                    msg_id,
                    data: Box::new(inner),
                })
            }
        }
    }

    pub fn as_dictionary(&self) -> Option<&Dictionary> {
        match self {
            XPCObject::Dictionary(dict) => Some(dict),
            _ => None,
        }
    }

    pub fn to_dictionary(self) -> Option<Dictionary> {
        match self {
            XPCObject::Dictionary(dict) => Some(dict),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Self>> {
        match self {
            XPCObject::Array(array) => Some(array),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            XPCObject::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<&bool> {
        match self {
            XPCObject::Bool(b) => Some(b),
            _ => None,
        }
    }

    pub fn as_signed_integer(&self) -> Option<i64> {
        match self {
            XPCObject::String(s) => s.parse().ok(),
            XPCObject::Int64(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_unsigned_integer(&self) -> Option<u64> {
        match self {
            XPCObject::String(s) => s.parse().ok(),
            XPCObject::UInt64(v) => Some(*v),
            _ => None,
        }
    }

    fn calculate_padding(len: usize) -> usize {
        (4 - len % 4) % 4
    }
}

/// Rejects an entry count that cannot fit in `remaining` bytes at
/// `min_entry` bytes each, before any per-entry work.
fn check_count(count: u32, min_entry: u64, remaining: usize) -> Result<(), IdeviceError> {
    let need = u64::from(count) * min_entry;
    if need > remaining as u64 {
        return Err(XpcError::LengthOutOfBounds {
            declared: need,
            remaining,
        }
        .into());
    }
    Ok(())
}

impl From<Dictionary> for XPCObject {
    fn from(value: Dictionary) -> Self {
        XPCObject::Dictionary(value)
    }
}

pub struct XPCMessage {
    pub flags: u32,
    pub message: Option<XPCObject>,
    pub message_id: Option<u64>,
}

impl XPCMessage {
    pub fn new(
        flags: Option<XPCFlag>,
        message: Option<XPCObject>,
        message_id: Option<u64>,
    ) -> XPCMessage {
        XPCMessage {
            flags: flags.unwrap_or(XPCFlag::AlwaysSet).into(),
            message,
            message_id,
        }
    }

    /// Decodes one whole wrapper from the front of `data` with the default
    /// [`DEFAULT_MAX_MESSAGE_SIZE`] cap. Equivalent to
    /// [`Self::decode_with_limit`]`(data, DEFAULT_MAX_MESSAGE_SIZE)`.
    pub fn decode(data: &[u8]) -> Result<XPCMessage, IdeviceError> {
        Self::decode_with_limit(data, DEFAULT_MAX_MESSAGE_SIZE)
    }

    /// Decodes one whole wrapper from the front of `data`; trailing bytes are
    /// ignored. `max_message_size` caps header plus declared body length.
    ///
    /// Errors, in check order: `NotEnoughBytes` under 24 bytes; `MalformedXpc`
    /// for a bad magic; `MessageTooLarge` as soon as the header declares more
    /// than `max_message_size` bytes (before waiting for or allocating the
    /// body); `CdTunnel(SizeMismatch)` while the body is incomplete; then any
    /// object error from [`XPCObject::decode`].
    ///
    /// Bounds: O(n) time in the body length n (each byte is read once; entry
    /// counts are checked against the remaining bytes up front); recursion
    /// depth at most [`MAX_NESTING_DEPTH`]; transient space is the decoded
    /// object, at most one node per 4 body bytes plus copied string and data
    /// bytes, so O(n).
    pub fn decode_with_limit(
        data: &[u8],
        max_message_size: usize,
    ) -> Result<XPCMessage, IdeviceError> {
        match Self::decode_prefix(data, max_message_size)? {
            Some((msg, _)) => Ok(msg),
            None if data.len() < XPC_WRAPPER_LEN => {
                Err(IdeviceError::NotEnoughBytes(data.len(), XPC_WRAPPER_LEN))
            }
            None => Err(CdTunnelError::SizeMismatch.into()),
        }
    }

    /// Like [`Self::decode_with_limit`] but `Ok(None)` while `data` does not
    /// yet hold the whole wrapper, and on success also the bytes it occupied.
    pub(crate) fn decode_prefix(
        data: &[u8],
        max_message_size: usize,
    ) -> Result<Option<(XPCMessage, usize)>, IdeviceError> {
        let Some(header) = data.get(..XPC_WRAPPER_LEN) else {
            return Ok(None);
        };
        let mut r = Reader {
            buf: header,
            pos: 0,
        };
        if r.u32()? != WRAPPER_MAGIC {
            warn!("XPCMessage magic is invalid.");
            Err(XpcError::MalformedXpc)?
        }
        let flags = r.u32()?;
        let body_len = r.u64()?;
        let message_id = r.u64()?;
        let total = body_len
            .checked_add(XPC_WRAPPER_LEN as u64)
            .filter(|&t| t <= max_message_size as u64)
            .ok_or(XpcError::MessageTooLarge {
                declared: body_len.saturating_add(XPC_WRAPPER_LEN as u64),
                max: max_message_size,
            })? as usize;
        let Some(body) = data.get(XPC_WRAPPER_LEN..total) else {
            debug!(
                "Body length is {body_len}, but received bytes is {}",
                data.len()
            );
            return Ok(None);
        };
        let res = XPCMessage {
            flags,
            message: if body.is_empty() {
                None
            } else {
                Some(XPCObject::decode_with_limit(body, max_message_size)?)
            },
            message_id: Some(message_id),
        };
        debug!("Decoded {res:#?}");
        Ok(Some((res, total)))
    }

    pub fn encode(self, message_id: u64) -> Result<Vec<u8>, IdeviceError> {
        let mut out = 0x29b00b92_u32.to_le_bytes().to_vec();
        out.extend_from_slice(&self.flags.to_le_bytes());
        match self.message {
            Some(message) => {
                let body = message.encode()?;
                out.extend_from_slice(&(body.len() as u64).to_le_bytes()); // body length
                out.extend_from_slice(&message_id.to_le_bytes()); // messageId
                out.extend_from_slice(&body);
            }
            _ => {
                out.extend_from_slice(&0_u64.to_le_bytes());
                out.extend_from_slice(&message_id.to_le_bytes());
            }
        }
        Ok(out)
    }
}

impl std::fmt::Debug for XPCMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();

        if self.flags & 0x00000001 != 0 {
            parts.push("AlwaysSet".to_string());
        }
        if self.flags & 0x00000100 != 0 {
            parts.push("DataFlag".to_string());
        }
        if self.flags & 0x00010000 != 0 {
            parts.push("WantingReply".to_string());
        }
        if self.flags & 0x00020000 != 0 {
            parts.push("Reply".to_string());
        }
        if self.flags & 0x00400000 != 0 {
            parts.push("InitHandshake".to_string());
        }

        // Check for any unknown bits (not covered by known flags)
        let known_mask = 0x00000001 | 0x00000100 | 0x00010000 | 0x00020000 | 0x00400000;
        let custom_bits = self.flags & !known_mask;
        if custom_bits != 0 {
            parts.push(format!("Custom(0x{custom_bits:08X})"));
        }

        write!(
            f,
            "XPCMessage {{ flags: [{}], message_id: {:?}, message: {:?} }}",
            parts.join(" | "),
            self.message_id,
            self.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    fn err(r: Result<impl std::fmt::Debug, IdeviceError>) -> XpcError {
        match r {
            Err(IdeviceError::Xpc(e)) => e,
            other => panic!("expected an XpcError, got {other:?}"),
        }
    }

    fn nested_arrays(depth: usize) -> XPCObject {
        (0..depth).fold(XPCObject::Null, |inner, _| XPCObject::Array(vec![inner]))
    }

    #[test]
    fn round_trip_every_variant() {
        let mut d = Dictionary::new();
        d.insert("n".into(), XPCObject::Null);
        d.insert("b".into(), XPCObject::Bool(true));
        d.insert("f".into(), XPCObject::Double(-0.5));
        d.insert("i".into(), XPCObject::Int64(-3));
        d.insert("u".into(), XPCObject::UInt64(u64::MAX));
        d.insert(
            "t".into(),
            XPCObject::Date(UNIX_EPOCH + Duration::from_nanos(123)),
        );
        d.insert("s".into(), XPCObject::String("abc".into()));
        d.insert("d".into(), XPCObject::Data(vec![1, 2, 3, 4, 5]));
        d.insert("id".into(), XPCObject::Uuid(uuid::Uuid::from_u128(7)));
        d.insert(
            "ft".into(),
            XPCObject::FileTransfer {
                msg_id: 9,
                data: Box::new(XPCObject::Array(vec![XPCObject::Int64(1)])),
            },
        );
        let o = XPCObject::Dictionary(d);
        assert_eq!(XPCObject::decode(&o.encode().unwrap()).unwrap(), o);
    }

    #[test]
    fn date_out_of_range_rejected() {
        let before = XPCObject::Date(UNIX_EPOCH - Duration::from_nanos(1));
        assert!(matches!(err(before.encode()), XpcError::DateOutOfRange));
        let after =
            XPCObject::Date(UNIX_EPOCH + Duration::from_nanos(u64::MAX) + Duration::from_nanos(1));
        assert!(matches!(err(after.encode()), XpcError::DateOutOfRange));
        let max = XPCObject::Date(UNIX_EPOCH + Duration::from_nanos(u64::MAX));
        assert_eq!(XPCObject::decode(&max.encode().unwrap()).unwrap(), max);
    }

    #[test]
    fn depth_64_accepted_65_rejected() {
        let ok = nested_arrays(MAX_NESTING_DEPTH);
        assert_eq!(XPCObject::decode(&ok.encode().unwrap()).unwrap(), ok);
        let deep = nested_arrays(MAX_NESTING_DEPTH + 1).encode().unwrap();
        assert!(matches!(
            err(XPCObject::decode(&deep)),
            XpcError::NestingTooDeep { max: 64 }
        ));
    }

    #[test]
    fn nested_length_past_remaining_rejected() {
        let mut b = XPCObject::String("abc".into()).encode().unwrap();
        // Declared string length (bytes 12..16) raised far past the buffer.
        b[12..16].copy_from_slice(&0xffff_fff0u32.to_le_bytes());
        assert!(matches!(
            err(XPCObject::decode(&b)),
            XpcError::LengthOutOfBounds {
                declared: 0xffff_fff0,
                ..
            }
        ));

        let mut b = XPCObject::Data(vec![0; 8]).encode().unwrap();
        b.truncate(b.len() - 1);
        assert!(matches!(
            err(XPCObject::decode(&b)),
            XpcError::LengthOutOfBounds { .. }
        ));

        // An entry count that cannot fit fails before any entry is read.
        let mut b = XPCObject::Array(vec![]).encode().unwrap();
        b[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            err(XPCObject::decode(&b)),
            XpcError::LengthOutOfBounds { .. }
        ));
    }

    #[test]
    fn nested_length_over_cap_rejected() {
        let body = XPCObject::Data(vec![0; 4096]);
        let w = XPCMessage::new(None, Some(body), Some(1))
            .encode(1)
            .unwrap();
        // Wrapper fits a 5000-byte cap; nested length checked against it too.
        assert!(XPCMessage::decode_with_limit(&w, 5000).is_ok());
        assert!(matches!(
            err(XPCMessage::decode_with_limit(&w, 1000)),
            XpcError::MessageTooLarge { max: 1000, .. }
        ));
        let mut r = Reader {
            buf: &w[24 + 12..],
            pos: 0,
        };
        assert!(matches!(
            err(r.len(100)),
            XpcError::LengthOutOfBounds { .. }
        ));
    }

    #[test]
    fn wrapper_length_overflow_rejected() {
        let mut h = 0x29b00b92_u32.to_le_bytes().to_vec();
        h.extend(1u32.to_le_bytes());
        h.extend(u64::MAX.to_le_bytes());
        h.extend(0u64.to_le_bytes());
        assert!(matches!(
            err(XPCMessage::decode(&h)),
            XpcError::MessageTooLarge {
                declared: u64::MAX,
                ..
            }
        ));
    }

    #[test]
    fn incomplete_wrapper_keeps_legacy_errors() {
        let w = XPCMessage::new(None, Some(XPCObject::Bool(true)), Some(1))
            .encode(1)
            .unwrap();
        assert!(matches!(
            XPCMessage::decode(&w[..10]),
            Err(IdeviceError::NotEnoughBytes(10, 24))
        ));
        assert!(matches!(
            XPCMessage::decode(&w[..w.len() - 1]),
            Err(IdeviceError::CdTunnel(CdTunnelError::SizeMismatch))
        ));
    }
}
