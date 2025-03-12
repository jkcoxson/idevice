// Jackson Coxson
// Messages contain:
// - 32 byte header
// - 16 byte payload header
// - Optional auxiliary
//   - 16 byte aux header, useless
//   - Aux data
// - Payload (NSKeyedArchive)

use plist::Value;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::IdeviceError;

#[derive(Debug, Clone, PartialEq)]
pub struct MessageHeader {
    magic: u32,      // 0x795b3d1f
    header_len: u32, // will always be 32 bytes
    fragment_id: u16,
    fragment_count: u16,
    length: u32, // Length of of the payload
    identifier: u32,
    conversation_index: u32,
    pub channel: u32,
    expects_reply: bool,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PayloadHeader {
    flags: u32,
    aux_length: u32,
    total_length: u64,
}

#[derive(Debug, Default, PartialEq)]
pub struct AuxHeader {
    buffer_size: u32,
    unknown: u32,
    aux_size: u32,
    unknown2: u32,
}

#[derive(Debug, PartialEq)]
pub struct Aux {
    pub header: AuxHeader,
    pub values: Vec<AuxValue>,
}

#[derive(PartialEq)]
pub enum AuxValue {
    String(String), // 0x01
    Array(Vec<u8>), // 0x02
    U32(u32),       // 0x03
    I64(i64),       // 0x06
}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub message_header: MessageHeader,
    pub payload_header: PayloadHeader,
    pub aux: Option<Aux>,
    pub data: Option<Value>,
}

impl Aux {
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, IdeviceError> {
        if bytes.len() < 16 {
            return Err(IdeviceError::NotEnoughBytes(bytes.len(), 24));
        }

        let header = AuxHeader {
            buffer_size: u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            unknown: u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            aux_size: u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
            unknown2: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        };

        let mut bytes = &bytes[16..];
        let mut values = Vec::new();
        loop {
            if bytes.len() < 8 {
                break;
            }
            let aux_type = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            bytes = &bytes[4..];
            match aux_type {
                0x0a => {
                    // null, skip
                    // seems to be in between every value
                }
                0x01 => {
                    let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                    bytes = &bytes[4..];
                    if bytes.len() < len {
                        return Err(IdeviceError::NotEnoughBytes(bytes.len(), len));
                    }
                    values.push(AuxValue::String(String::from_utf8(bytes[..len].to_vec())?));
                    bytes = &bytes[len..];
                }
                0x02 => {
                    let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
                    bytes = &bytes[4..];
                    if bytes.len() < len {
                        return Err(IdeviceError::NotEnoughBytes(bytes.len(), len));
                    }
                    values.push(AuxValue::Array(bytes[..len].to_vec()));
                    bytes = &bytes[len..];
                }
                0x03 => {
                    values.push(AuxValue::U32(u32::from_le_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3],
                    ])));
                    bytes = &bytes[4..];
                }
                0x06 => {
                    if bytes.len() < 8 {
                        return Err(IdeviceError::NotEnoughBytes(8, bytes.len()));
                    }
                    values.push(AuxValue::I64(i64::from_le_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6],
                        bytes[7],
                    ])));
                    bytes = &bytes[8..];
                }
                _ => return Err(IdeviceError::UnknownAuxValueType(aux_type)),
            }
        }

        Ok(Self { header, values })
    }

    // Creates the default struct
    // Note that the header isn't updated until serialization
    pub fn from_values(values: Vec<AuxValue>) -> Self {
        Self {
            header: AuxHeader::default(),
            values,
        }
    }

    /// Serializes the values with the correctly sized header
    pub fn serialize(&self) -> Vec<u8> {
        let mut values_payload = Vec::new();
        for v in self.values.iter() {
            values_payload.extend_from_slice(&0x0a_u32.to_le_bytes());
            match v {
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
            }
        }

        let mut res = Vec::new();
        let buffer_size = 496_u32;
        res.extend_from_slice(&buffer_size.to_le_bytes()); // TODO: find what
                                                           // this means and how to actually serialize it
                                                           // go-ios just uses 496
                                                           // pymobiledevice3 doesn't seem to parse the header at all
        res.extend_from_slice(&0_u32.to_le_bytes());
        res.extend_from_slice(&(values_payload.len() as u32).to_le_bytes());
        res.extend_from_slice(&0_u32.to_le_bytes());
        res.extend_from_slice(&values_payload);
        res
    }
}

impl AuxValue {
    // Returns an array AuxType
    pub fn archived_value(v: impl Into<plist::Value>) -> Self {
        Self::Array(ns_keyed_archive::encode::encode_to_bytes(v.into()).expect("Failed to encode"))
    }
}

impl MessageHeader {
    /// Creates a new header. Note that during serialization, the length will be updated
    pub fn new(
        fragment_id: u16,
        fragment_count: u16,
        identifier: u32,
        conversation_index: u32,
        channel: u32,
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
    pub fn new() -> Self {
        Self::default()
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut res = Vec::new();
        res.extend_from_slice(&self.flags.to_le_bytes());
        res.extend_from_slice(&self.aux_length.to_le_bytes());
        res.extend_from_slice(&self.total_length.to_le_bytes());

        res
    }

    pub fn method_invocation() -> Self {
        Self {
            flags: 2,
            ..Default::default()
        }
    }

    pub fn apply_expects_reply_map(&mut self) {
        self.flags |= 0x1000
    }
}

impl Message {
    pub async fn from_reader<R: AsyncRead + Unpin>(reader: &mut R) -> Result<Self, IdeviceError> {
        let mut buf = [0u8; 32];
        reader.read_exact(&mut buf).await?;

        let mheader = MessageHeader {
            magic: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            header_len: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            fragment_id: u16::from_le_bytes([buf[8], buf[9]]),
            fragment_count: u16::from_le_bytes([buf[10], buf[11]]),
            length: u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            identifier: u32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            conversation_index: u32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
            channel: u32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]),
            expects_reply: u32::from_le_bytes([buf[28], buf[29], buf[30], buf[31]]) == 1,
        };

        let mut buf = [0u8; 16];
        reader.read_exact(&mut buf).await?;

        let pheader = PayloadHeader {
            flags: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            aux_length: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            total_length: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        };

        let aux = if pheader.aux_length > 0 {
            let mut buf = vec![0u8; pheader.aux_length as usize];
            reader.read_exact(&mut buf).await?;
            Some(Aux::from_bytes(buf)?)
        } else {
            None
        };

        let mut buf = vec![0u8; (pheader.total_length - pheader.aux_length as u64) as usize];
        reader.read_exact(&mut buf).await?;

        let data = if buf.is_empty() {
            None
        } else {
            Some(ns_keyed_archive::decode::from_bytes(&buf)?)
        };

        Ok(Message {
            message_header: mheader,
            payload_header: pheader,
            aux,
            data,
        })
    }

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
        }
    }

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
        payload_header.total_length = (aux.len() + data.len()) as u64;
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
}

impl std::fmt::Debug for AuxValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuxValue::String(s) => write!(f, "String({:?})", s),
            AuxValue::Array(arr) => write!(
                f,
                "Array(len={}, first_bytes={:?})",
                arr.len(),
                &arr[..arr.len().min(10)]
            ), // Show only first 10 bytes
            AuxValue::U32(n) => write!(f, "U32({})", n),
            AuxValue::I64(n) => write!(f, "I64({})", n),
        }
    }
}
