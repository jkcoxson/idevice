// Jackson Coxson
// https://sourceware.org/gdb/current/onlinedocs/gdb.html/Packets.html#Packets

use log::debug;
use std::fmt::Write;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{IdeviceError, ReadWrite};

pub const SERVICE_NAME: &str = "com.apple.internal.dt.remote.debugproxy";

pub struct DebugProxyClient {
    pub socket: Box<dyn ReadWrite>,
    pub noack_mode: bool,
}

pub struct DebugserverCommand {
    pub name: String,
    pub argv: Vec<String>,
}

impl DebugserverCommand {
    pub fn new(name: String, argv: Vec<String>) -> Self {
        Self { name, argv }
    }
}

impl DebugProxyClient {
    pub fn new(socket: Box<dyn ReadWrite>) -> Self {
        Self {
            socket,
            noack_mode: false,
        }
    }

    pub async fn send_command(
        &mut self,
        command: DebugserverCommand,
    ) -> Result<Option<String>, IdeviceError> {
        // Hex-encode the arguments
        let hex_args = command
            .argv
            .iter()
            .map(|arg| hex_encode(arg.as_bytes()))
            .collect::<Vec<String>>()
            .join("");

        // Construct the packet data (command + hex-encoded arguments)
        let packet_data = format!("{}{}", command.name, hex_args);

        // Calculate the checksum
        let checksum = calculate_checksum(&packet_data);

        // Construct the full packet
        let packet = format!("${}#{}", packet_data, checksum);

        // Log the packet for debugging
        debug!("Sending packet: {}", packet);

        // Send the packet
        self.socket.write_all(packet.as_bytes()).await?;

        // Read the response
        let response = self.read_response().await?;
        Ok(response)
    }

    pub async fn read_response(&mut self) -> Result<Option<String>, IdeviceError> {
        let mut buffer = Vec::new();
        let mut received_char = [0u8; 1];

        if !self.noack_mode {
            self.socket.read_exact(&mut received_char).await?;
            if received_char[0] != b'+' {
                debug!("No + ack");
                return Ok(None);
            }
        }

        self.socket.read_exact(&mut received_char).await?;
        if received_char[0] != b'$' {
            debug!("No $ response");
            return Ok(None);
        }

        loop {
            self.socket.read_exact(&mut received_char).await?;
            if received_char[0] == b'#' {
                break;
            }
            buffer.push(received_char[0]);
        }

        if !self.noack_mode {
            self.send_ack().await?;
        }

        let response = String::from_utf8(buffer)?;
        Ok(Some(response))
    }

    pub async fn send_raw(&mut self, bytes: &[u8]) -> Result<(), IdeviceError> {
        self.socket.write_all(bytes).await?;
        Ok(())
    }

    pub async fn read(&mut self, len: usize) -> Result<String, IdeviceError> {
        let mut buf = vec![0; len];
        let r = self.socket.read(&mut buf).await?;

        Ok(String::from_utf8_lossy(&buf[..r]).to_string())
    }

    pub async fn set_argv(&mut self, argv: Vec<String>) -> Result<String, IdeviceError> {
        if argv.is_empty() {
            return Err(IdeviceError::InvalidArgument);
        }

        // Calculate the total length of the packet
        let mut pkt_len = 0;
        for (i, arg) in argv.iter().enumerate() {
            let prefix = format!(",{},{},", arg.len() * 2, i);
            pkt_len += prefix.len() + arg.len() * 2;
        }

        // Allocate and initialize the packet
        let mut pkt = vec![0u8; pkt_len + 1];
        let mut pktp = 0;

        for (i, arg) in argv.iter().enumerate() {
            let prefix = format!(",{},{},", arg.len() * 2, i);
            let prefix_bytes = prefix.as_bytes();

            // Copy prefix to the packet
            pkt[pktp..pktp + prefix_bytes.len()].copy_from_slice(prefix_bytes);
            pktp += prefix_bytes.len();

            // Hex encode the argument
            for byte in arg.bytes() {
                let hex = format!("{:02X}", byte);
                pkt[pktp..pktp + 2].copy_from_slice(hex.as_bytes());
                pktp += 2;
            }
        }

        // Set the first byte of the packet
        pkt[0] = b'A';

        // Simulate sending the command and receiving a response
        self.send_raw(&pkt).await?;
        let response = self.read(16).await?;

        Ok(response)
    }

    pub async fn send_ack(&mut self) -> Result<(), IdeviceError> {
        self.socket.write_all(b"+").await?;
        Ok(())
    }

    pub async fn send_noack(&mut self) -> Result<(), IdeviceError> {
        self.socket.write_all(b"-").await?;
        Ok(())
    }

    pub fn set_ack_mode(&mut self, enabled: bool) {
        self.noack_mode = !enabled;
    }
}

fn calculate_checksum(data: &str) -> String {
    let checksum = data.bytes().fold(0u8, |acc, byte| acc.wrapping_add(byte));
    format!("{:02x}", checksum)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut output, b| {
        let _ = write!(output, "{b:02X}");
        output
    })
}

impl From<String> for DebugserverCommand {
    fn from(s: String) -> Self {
        // Split string into command and arguments
        let mut split = s.split_whitespace();
        let command = split.next().unwrap_or("").to_string();
        let arguments: Vec<String> = split.map(|s| s.to_string()).collect();
        Self::new(command, arguments)
    }
}
impl From<&str> for DebugserverCommand {
    fn from(s: &str) -> DebugserverCommand {
        s.to_string().into()
    }
}
