// Jackson Coxson

use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use plist::Value;
use std::io::{Cursor, Read, Write};

const MESSAGE_AUX_MAGIC: u64 = 0x1f0;
const DTX_MESSAGE_MAGIC: u32 = 0x1F3D5B79;
const EMPTY_DICTIONARY: u32 = 0xa;

#[derive(Debug, Clone)]
pub enum AuxValue {
    Object(Vec<u8>),
    Int(u32),
    Long(u64),
    Bytes(Vec<u8>),
    PlistObject(Value),
}

#[derive(Debug, Clone)]
pub struct AuxItem {
    aux_type: u32,
    value: AuxValue,
}

#[derive(Debug, Clone)]
pub struct MessageAux {
    magic: u64,
    aux: Vec<AuxItem>,
}

#[derive(Debug, Clone)]
pub struct DtxMessageHeader {
    magic: u32,
    cb: u32,
    fragment_id: u16,
    fragment_count: u16,
    length: u32,
    identifier: u32,
    conversation_index: u32,
    channel_code: i32,
    expects_reply: u32,
}

#[derive(Debug, Clone)]
pub struct DtxMessagePayloadHeader {
    flags: u32,
    auxiliary_length: u32,
    total_length: u64,
}

impl MessageAux {
    pub fn new() -> Self {
        MessageAux {
            magic: MESSAGE_AUX_MAGIC,
            aux: Vec::new(),
        }
    }

    pub fn append_int(&mut self, value: u32) -> &mut Self {
        self.aux.push(AuxItem {
            aux_type: 3,
            value: AuxValue::Int(value),
        });
        self
    }

    pub fn append_long(&mut self, value: u64) -> &mut Self {
        self.aux.push(AuxItem {
            aux_type: 6,
            value: AuxValue::Long(value),
        });
        self
    }

    pub fn append_obj(&mut self, value: Value) -> &mut Self {
        // Serialize the plist value to binary format
        let mut buf = Vec::new();
        value.to_writer_binary(&mut buf).unwrap();

        self.aux.push(AuxItem {
            aux_type: 2,
            value: AuxValue::Object(buf),
        });
        self
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::new();

        // Write magic number
        result.write_u64::<LittleEndian>(self.magic).unwrap();

        // Calculate and write the total size of aux data
        let mut aux_data = Vec::new();
        for item in &self.aux {
            // Write empty dictionary marker
            aux_data
                .write_u32::<LittleEndian>(EMPTY_DICTIONARY)
                .unwrap();

            // Write type
            aux_data.write_u32::<LittleEndian>(item.aux_type).unwrap();

            // Write value based on type
            match &item.value {
                AuxValue::Object(data) => {
                    aux_data
                        .write_u32::<LittleEndian>(data.len() as u32)
                        .unwrap();
                    aux_data.write_all(data).unwrap();
                }
                AuxValue::Int(value) => {
                    aux_data.write_u32::<LittleEndian>(*value).unwrap();
                }
                AuxValue::Long(value) => {
                    aux_data.write_u64::<LittleEndian>(*value).unwrap();
                }
                AuxValue::Bytes(data) => {
                    aux_data.write_all(data).unwrap();
                }
                AuxValue::PlistObject(obj) => {
                    let mut buf = Vec::new();
                    obj.to_writer_binary(&mut buf).unwrap();
                    aux_data.write_all(&buf).unwrap();
                }
            }
        }

        // Write the length of aux data
        result
            .write_u64::<LittleEndian>(aux_data.len() as u64)
            .unwrap();

        // Write aux data
        result.write_all(&aux_data).unwrap();

        result
    }

    pub fn deserialize(mut data: &[u8]) -> Result<Self, std::io::Error> {
        let magic = data.read_u64::<LittleEndian>()?;
        let aux_length = data.read_u64::<LittleEndian>()?;

        let mut aux_items = Vec::new();
        let mut aux_data = data.take(aux_length);

        while let Ok(empty_dict) = aux_data.read_u32::<LittleEndian>() {
            if empty_dict != EMPTY_DICTIONARY {
                // Handle non-standard format
                continue;
            }

            let aux_type = aux_data.read_u32::<LittleEndian>()?;

            let value = match aux_type {
                2 => {
                    // Object (Binary Plist)
                    let length = aux_data.read_u32::<LittleEndian>()?;
                    let mut buffer = vec![0u8; length as usize];
                    aux_data.read_exact(&mut buffer)?;

                    // You could optionally parse the binary plist here
                    let cursor = Cursor::new(buffer);
                    let plist_value: Value = Value::from_reader(cursor).expect("bad plist");
                    AuxValue::PlistObject(plist_value)
                }
                3 => {
                    // Int
                    let value = aux_data.read_u32::<LittleEndian>()?;
                    AuxValue::Int(value)
                }
                6 => {
                    // Long
                    let value = aux_data.read_u64::<LittleEndian>()?;
                    AuxValue::Long(value)
                }
                _ => {
                    // Default: raw bytes (remaining)
                    let mut buffer = Vec::new();
                    aux_data.read_to_end(&mut buffer)?;
                    AuxValue::Bytes(buffer)
                }
            };

            aux_items.push(AuxItem { aux_type, value });
        }

        Ok(MessageAux {
            magic,
            aux: aux_items,
        })
    }
}

impl Default for MessageAux {
    fn default() -> Self {
        Self::new()
    }
}

impl DtxMessageHeader {
    pub fn new() -> Self {
        DtxMessageHeader {
            magic: DTX_MESSAGE_MAGIC,
            cb: 0,
            fragment_id: 0,
            fragment_count: 0,
            length: 0,
            identifier: 0,
            conversation_index: 0,
            channel_code: 0,
            expects_reply: 0,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.write_u32::<LittleEndian>(self.magic).unwrap();
        result.write_u32::<LittleEndian>(self.cb).unwrap();
        result.write_u16::<LittleEndian>(self.fragment_id).unwrap();
        result
            .write_u16::<LittleEndian>(self.fragment_count)
            .unwrap();
        result.write_u32::<LittleEndian>(self.length).unwrap();
        result.write_u32::<LittleEndian>(self.identifier).unwrap();
        result
            .write_u32::<LittleEndian>(self.conversation_index)
            .unwrap();
        result.write_i32::<LittleEndian>(self.channel_code).unwrap();
        result
            .write_u32::<LittleEndian>(self.expects_reply)
            .unwrap();
        result
    }

    pub fn deserialize(mut data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(DtxMessageHeader {
            magic: data.read_u32::<LittleEndian>()?,
            cb: data.read_u32::<LittleEndian>()?,
            fragment_id: data.read_u16::<LittleEndian>()?,
            fragment_count: data.read_u16::<LittleEndian>()?,
            length: data.read_u32::<LittleEndian>()?,
            identifier: data.read_u32::<LittleEndian>()?,
            conversation_index: data.read_u32::<LittleEndian>()?,
            channel_code: data.read_i32::<LittleEndian>()?,
            expects_reply: data.read_u32::<LittleEndian>()?,
        })
    }
}

impl Default for DtxMessageHeader {
    fn default() -> Self {
        Self::new()
    }
}

impl DtxMessagePayloadHeader {
    pub fn new() -> Self {
        DtxMessagePayloadHeader {
            flags: 0,
            auxiliary_length: 0,
            total_length: 0,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::new();
        result.write_u32::<LittleEndian>(self.flags).unwrap();
        result
            .write_u32::<LittleEndian>(self.auxiliary_length)
            .unwrap();
        result.write_u64::<LittleEndian>(self.total_length).unwrap();
        result
    }

    pub fn deserialize(mut data: &[u8]) -> Result<Self, std::io::Error> {
        Ok(DtxMessagePayloadHeader {
            flags: data.read_u32::<LittleEndian>()?,
            auxiliary_length: data.read_u32::<LittleEndian>()?,
            total_length: data.read_u64::<LittleEndian>()?,
        })
    }
}

impl Default for DtxMessagePayloadHeader {
    fn default() -> Self {
        Self::new()
    }
}
