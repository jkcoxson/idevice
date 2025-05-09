// DebianArch

use std::collections::HashMap;

use super::error::Http2Error;

pub const HTTP2_MAGIC: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

#[derive(Debug)]
pub struct Frame {
    pub stream_id: u32,
    pub flags: u8,
    pub frame_type: FrameType,

    pub body: Vec<u8>,
}

impl Frame {
    pub fn new(stream_id: u32, flags: u8, frame_type: FrameType) -> Self {
        Self {
            stream_id,
            flags,
            frame_type,
            body: Vec::new(),
        }
    }

    pub fn set_body(&mut self, body: Vec<u8>) {
        self.body = body;
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self, Http2Error> {
        let mut len_buf = buf[0..3].to_vec();
        len_buf.insert(0, 0);

        let body_len = u32::from_be_bytes(len_buf.try_into().unwrap()) as usize;
        let frame_type = buf[3];
        let flags = buf[4];
        let stream_id = u32::from_be_bytes(buf[5..9].try_into()?);
        let body = buf[9..9 + body_len].to_vec();
        Ok(Self {
            stream_id,
            flags,
            frame_type: frame_type.into(),
            body,
        })
    }
}

impl Framable for Frame {
    fn serialize(&self) -> Vec<u8> {
        let mut res = Vec::new();

        let body_len = (self.body.len() as u32).to_be_bytes();
        res.extend_from_slice(&[body_len[1], body_len[2], body_len[3]]); // [0..3]
        res.push(self.frame_type.into()); // [3]
        res.push(self.flags); // flag mask [4]
        res.extend_from_slice(&self.stream_id.to_be_bytes()); // [4..8]
        res.extend_from_slice(&self.body); // [9..9+len]
        res
    }
}

pub trait Framable: From<Frame> {
    fn serialize(&self) -> Vec<u8>;
}

// Frame implementations:
pub struct SettingsFrame {
    frame: Frame,
    pub settings: HashMap<u16, u32>,
}

impl SettingsFrame {
    pub const HEADER_TABLE_SIZE: u16 = 0x01;
    pub const ENABLE_PUSH: u16 = 0x02;
    pub const MAX_CONCURRENT_STREAMS: u16 = 0x03;
    pub const INITIAL_WINDOW_SIZE: u16 = 0x04;
    pub const MAX_FRAME_SIZE: u16 = 0x05;
    pub const MAX_HEADER_LIST_SIZE: u16 = 0x06;
    pub const ENABLE_CONNECT_PROTOCOL: u16 = 0x08;

    pub const ACK: u8 = 0x01;
    pub fn new(/*stream_id: u32, */ settings: HashMap<u16, u32>, flags: u8) -> Self {
        let mut body = Vec::new();
        for setting in settings.clone() {
            body.extend_from_slice(&setting.0.to_be_bytes());
            body.extend_from_slice(&setting.1.to_be_bytes());
        }
        Self {
            frame: Frame {
                /*stream_id*/ stream_id: 0,
                flags,
                frame_type: FrameType::Settings,
                body,
            },
            settings,
        }
    }

    pub fn ack(/*stream_id: u32*/) -> Self {
        Self {
            frame: Frame {
                /*stream_id*/ stream_id: 0,
                flags: Self::ACK,
                frame_type: FrameType::Settings,
                body: Vec::new(),
            },
            settings: HashMap::new(),
        }
    }
}

impl Framable for SettingsFrame {
    fn serialize(&self) -> Vec<u8> {
        self.frame.serialize()
    }
}

impl From<Frame> for SettingsFrame {
    fn from(value: Frame) -> Self {
        let mut idx = 0;
        let mut settings = HashMap::new();
        while idx < value.body.len() {
            let key = u16::from_be_bytes(value.body[idx..idx + 2].try_into().unwrap());
            let value = u32::from_be_bytes(value.body[idx + 2..idx + 2 + 4].try_into().unwrap());
            settings.insert(key, value);
            idx += 2 + 4;
        }
        Self {
            frame: value,
            settings,
        }
    }
}

pub struct WindowUpdateFrame {
    frame: Frame,
    pub window_increment: u32,
}

impl WindowUpdateFrame {
    // the frame's stream identifier indicates the affected stream; in the latter, the value "0" indicates that the entire connection is the subject of the frame.
    pub fn new(stream_id: u32, window_increment: u32) -> Self {
        if window_increment == 0 {
            panic!("PROTOCOL_ERROR");
        }
        Self {
            frame: Frame {
                stream_id,
                flags: Default::default(),
                frame_type: FrameType::WindowUpdate,
                body: window_increment.to_be_bytes().to_vec(),
            },
            window_increment,
        }
    }
}

impl Framable for WindowUpdateFrame {
    fn serialize(&self) -> Vec<u8> {
        self.frame.serialize()
    }
}

impl From<Frame> for WindowUpdateFrame {
    fn from(value: Frame) -> Self {
        let body = value.body.clone();
        Self {
            frame: value,
            window_increment: u32::from_be_bytes(body.try_into().unwrap()),
        }
    }
}

pub struct HeadersFrame {
    frame: Frame,
}

impl HeadersFrame {
    pub const END_STREAM: u8 = 0x01;
    pub const END_HEADERS: u8 = 0x04;
    pub const PADDED: u8 = 0x08;
    pub const PRIORITY: u8 = 0x20;
    pub fn new(stream_id: u32, flags: u8) -> Self {
        Self {
            frame: Frame {
                stream_id,
                flags,
                frame_type: FrameType::Headers,
                body: Vec::new(),
            },
        }
    }
}

impl Framable for HeadersFrame {
    fn serialize(&self) -> Vec<u8> {
        if self.frame.flags & Self::PADDED == Self::PADDED {
            unimplemented!("haven't added padding support !")
        };

        if self.frame.flags & Self::PRIORITY == Self::PRIORITY {
            unimplemented!("haven't added priority support !")
        };

        //   let padding = 0; for 'PADDED' flag
        // let priority_data = b""; // for PRIORITY flag
        self.frame.serialize()
    }
}

impl From<Frame> for HeadersFrame {
    fn from(value: Frame) -> Self {
        if value.flags & Self::PADDED == Self::PADDED {
            unimplemented!("haven't added padding support !")
        };

        if value.flags & Self::PRIORITY == Self::PRIORITY {
            unimplemented!("haven't added priority support !")
        };
        Self { frame: value }
    }
}

pub struct DataFrame {
    frame: Frame,
}

impl DataFrame {
    // TODO: Handle padding flag.
    pub fn new(stream_id: u32, data: Vec<u8>, flags: u8) -> Self {
        Self {
            frame: Frame {
                stream_id,
                flags,
                frame_type: FrameType::Data,
                body: data,
            },
        }
    }
}

impl Framable for DataFrame {
    fn serialize(&self) -> Vec<u8> {
        self.frame.serialize()
    }
}

impl From<Frame> for DataFrame {
    fn from(value: Frame) -> Self {
        Self { frame: value }
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum FrameType {
    Data = 0,
    Headers = 1,
    Priority = 2,
    RstStream = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
}

impl From<FrameType> for u8 {
    fn from(value: FrameType) -> Self {
        value as u8
    }
}

impl From<u8> for FrameType {
    fn from(value: u8) -> Self {
        unsafe { std::mem::transmute::<_, FrameType>(value) }
    }
}

// impl Drop for Connection {
//     fn drop(&mut self) {

//     }
// }
