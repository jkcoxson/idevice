// DebianArch

use json::JsonValue;
pub struct CDTunnel {}

impl CDTunnel {
    const MAGIC: &'static [u8; 8] = b"CDTunnel";
    pub fn decode(data: &[u8]) -> Result<JsonValue, Box<dyn std::error::Error>> {
        let magic_len = CDTunnel::MAGIC.len();
        if &data[0..magic_len] != CDTunnel::MAGIC {
            Err("Invalid Magic")?;
        }

        let size = u16::from_be_bytes(data[magic_len..magic_len + 2].try_into()?) as usize;
        let content = &data[magic_len + 2..magic_len + 2 + size];
        Ok(json::parse(&String::from_utf8(content.to_vec())?)?)
    }

    pub fn encode(value: JsonValue) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        let mut buf = Vec::new();
        let json_str = value.dump();

        buf.extend_from_slice(CDTunnel::MAGIC);
        buf.extend_from_slice(&u16::to_be_bytes(json_str.len().try_into()?));
        buf.extend_from_slice(json_str.as_bytes());
        Ok(buf)
    }
}
