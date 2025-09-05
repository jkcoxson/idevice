// Jackson Coxson
// I couldn't find a lib that parses IP/TCP, so I guess we'll write our own

use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::Arc,
};

use log::debug;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::Mutex,
};

pub enum ProtocolNumber {
    Tcp = 6,
}

#[derive(Debug)]
pub struct Ipv4Packet {
    pub version: u8,          // always 4 for IPv4
    pub ihl: u8,              // len of header / 4
    pub tos: u8,              // nobody can agree what this is for
    pub total_length: u16,    // length of packet in bytes
    pub identification: u16,  // ID from sender to help assemble datagram
    pub flags: u8, // 3 bits; reserved: 0, may fragment: 0/1, last fragment 0 / more fragments 1
    pub fragment_offset: u16, // where in the datagram this belongs
    // If Google can ignore fragments, so can we
    pub ttl: u8,              // max amount of time this packet can live
    pub protocol: u8,         // protocol number, 6 for TCP
    pub header_checksum: u16, // wrapping add all the u16 in header, then invert all bits
    pub source: Ipv4Addr,
    pub destination: Ipv4Addr,
    pub options: Vec<u8>,
    pub payload: Vec<u8>, // if smoltcp can ignore options, so can we
}

impl Ipv4Packet {
    pub fn parse(packet: &[u8]) -> Option<Self> {
        if packet.len() < 20 {
            return None;
        }

        let version_ihl = packet[0];
        let version = version_ihl >> 4;
        let ihl = (version_ihl & 0x0F) * 4; // send help I don't understand bitwise ops

        if version != 4 || packet.len() < ihl as usize {
            return None;
        }

        let tos = packet[1];
        let total_length = u16::from_be_bytes([packet[2], packet[3]]);
        let identification = u16::from_be_bytes([packet[4], packet[5]]);
        let flags_fragment = u16::from_be_bytes([packet[6], packet[7]]);
        let flags = (flags_fragment >> 13) as u8;
        let fragment_offset = flags_fragment & 0x1FFF;
        let ttl = packet[8];
        let protocol = packet[9];
        let header_checksum = u16::from_be_bytes([packet[10], packet[11]]);
        let source = Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
        let destination = Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);

        let options_end = ihl as usize;
        let options = if options_end > 20 {
            packet[20..options_end].to_vec()
        } else {
            Vec::new()
        };

        let payload = if total_length as usize > options_end {
            packet[options_end..total_length as usize].to_vec()
        } else {
            Vec::new()
        };

        Some(Self {
            version,
            ihl,
            tos,
            total_length,
            identification,
            flags,
            fragment_offset,
            ttl,
            protocol,
            header_checksum,
            source,
            destination,
            options,
            payload,
        })
    }

    /// Asynchronously read an IPv4 packet from a Tokio AsyncRead source.
    pub async fn from_reader<R: AsyncRead + Unpin + AsyncReadExt>(
        reader: &mut R,
        log: &Option<Arc<Mutex<tokio::fs::File>>>,
    ) -> Result<Self, std::io::Error> {
        let mut log_packet = Vec::new();

        let mut header = [0u8; 20]; // Minimum IPv4 header size
        reader.read_exact(&mut header).await?;
        if log.is_some() {
            log_packet.extend_from_slice(&header);
        }

        let version_ihl = header[0];
        let version = version_ihl >> 4;
        let ihl = (version_ihl & 0x0F) * 4;

        if version != 4 || ihl < 20 {
            debug!("Got an invalid IPv4 header from reader");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid IPv4 header",
            ));
        }

        let tos = header[1];
        let total_length = u16::from_be_bytes([header[2], header[3]]);
        let identification = u16::from_be_bytes([header[4], header[5]]);
        let flags_fragment = u16::from_be_bytes([header[6], header[7]]);
        let flags = (flags_fragment >> 13) as u8;
        let fragment_offset = flags_fragment & 0x1FFF;
        let ttl = header[8];
        let protocol = header[9];
        let header_checksum = u16::from_be_bytes([header[10], header[11]]);
        let source = Ipv4Addr::new(header[12], header[13], header[14], header[15]);
        let destination = Ipv4Addr::new(header[16], header[17], header[18], header[19]);

        // Read options if the header is larger than 20 bytes
        let options_len = ihl as usize - 20;
        let mut options = vec![0u8; options_len];
        if options_len > 0 {
            reader.read_exact(&mut options).await?;
            if log.is_some() {
                log_packet.extend_from_slice(&options);
            }
        }

        // Read the payload
        let payload_len = total_length as usize - ihl as usize;
        let mut payload = vec![0u8; payload_len];
        reader.read_exact(&mut payload).await?;
        if let Some(log) = log {
            log_packet.extend_from_slice(&payload);
            super::log_packet(log, &log_packet);
        }

        Ok(Self {
            version,
            ihl,
            tos,
            total_length,
            identification,
            flags,
            fragment_offset,
            ttl,
            protocol,
            header_checksum,
            source,
            destination,
            options,
            payload,
        })
    }

    pub fn create(
        source: Ipv4Addr,
        destination: Ipv4Addr,
        protocol: ProtocolNumber,
        ttl: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let ihl: u8 = 5;
        let total_length = (ihl as usize * 4 + payload.len()) as u16;
        let identification: u16 = 0;
        let flags_fragment: u16 = 0;
        let header_checksum: u16 = 0;

        let mut packet = vec![0; total_length as usize];
        packet[0] = (4 << 4) | (ihl & 0x0F);
        packet[1] = 0;
        packet[2..4].copy_from_slice(&total_length.to_be_bytes());
        packet[4..6].copy_from_slice(&identification.to_be_bytes());
        packet[6..8].copy_from_slice(&flags_fragment.to_be_bytes());
        packet[8] = ttl;
        packet[9] = protocol as u8;
        packet[10..12].copy_from_slice(&header_checksum.to_be_bytes());
        packet[12..16].copy_from_slice(&source.octets());
        packet[16..20].copy_from_slice(&destination.octets());
        packet[20..].copy_from_slice(payload);

        Self::apply_checksum(&mut packet);
        packet
    }

    fn apply_checksum(packet: &mut [u8]) {
        packet[10] = 0;
        packet[11] = 0;
        let mut checksum: u16 = 0;
        for i in 0..packet.len() / 2 {
            let word = u16::from_be_bytes([packet[i * 2], packet[(i * 2) + 1]]);
            checksum = checksum.wrapping_add(word);
        }
        let checksum = checksum.to_be_bytes();
        packet[10] = checksum[0];
        packet[11] = checksum[1];
    }
}

pub struct Ipv6Packet {
    pub version: u8,
    pub traffic_class: u8,
    pub flow_label: u32,
    pub payload_length: u16,
    pub next_header: u8,
    pub hop_limit: u8,
    pub source: Ipv6Addr,
    pub destination: Ipv6Addr,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) enum IpParseError<T> {
    Ok { packet: T, bytes_consumed: usize },
    NotEnough,
    Invalid,
}

impl Ipv6Packet {
    pub(crate) fn parse(
        packet: &[u8],
        log: &Option<Arc<Mutex<tokio::fs::File>>>,
    ) -> IpParseError<Ipv6Packet> {
        if packet.len() < 40 {
            return IpParseError::NotEnough;
        }

        let version = packet[0] >> 4;
        if version != 6 {
            return IpParseError::Invalid;
        }

        let traffic_class = ((packet[0] & 0x0F) << 4) | (packet[1] >> 4);
        let flow_label =
            ((packet[1] as u32 & 0x0F) << 16) | ((packet[2] as u32) << 8) | packet[3] as u32;
        let payload_length = u16::from_be_bytes([packet[4], packet[5]]);
        let total_packet_len = 40 + payload_length as usize;

        if packet.len() < total_packet_len {
            return IpParseError::NotEnough;
        }

        let next_header = packet[6];
        let hop_limit = packet[7];
        let source = Ipv6Addr::new(
            u16::from_be_bytes([packet[8], packet[9]]),
            u16::from_be_bytes([packet[10], packet[11]]),
            u16::from_be_bytes([packet[12], packet[13]]),
            u16::from_be_bytes([packet[14], packet[15]]),
            u16::from_be_bytes([packet[16], packet[17]]),
            u16::from_be_bytes([packet[18], packet[19]]),
            u16::from_be_bytes([packet[20], packet[21]]),
            u16::from_be_bytes([packet[22], packet[23]]),
        );

        let destination = Ipv6Addr::new(
            u16::from_be_bytes([packet[24], packet[25]]),
            u16::from_be_bytes([packet[26], packet[27]]),
            u16::from_be_bytes([packet[28], packet[29]]),
            u16::from_be_bytes([packet[30], packet[31]]),
            u16::from_be_bytes([packet[32], packet[33]]),
            u16::from_be_bytes([packet[34], packet[35]]),
            u16::from_be_bytes([packet[36], packet[37]]),
            u16::from_be_bytes([packet[38], packet[39]]),
        );
        let payload = packet[40..total_packet_len].to_vec();

        if let Some(log) = log {
            let mut log_packet = Vec::new();
            log_packet.extend_from_slice(&packet[..40]);
            log_packet.extend_from_slice(&payload);
            super::log_packet(log, &log_packet);
        }

        IpParseError::Ok {
            packet: Self {
                version,
                traffic_class,
                flow_label,
                payload_length,
                next_header,
                hop_limit,
                source,
                destination,
                payload,
            },
            bytes_consumed: total_packet_len,
        }
    }

    pub async fn from_reader<R: AsyncRead + Unpin>(
        reader: &mut R,
        log: &Option<Arc<Mutex<tokio::fs::File>>>,
    ) -> Result<Self, std::io::Error> {
        let mut log_packet = Vec::new();

        let mut header = [0u8; 40]; // IPv6 header size is fixed at 40 bytes
        reader.read_exact(&mut header).await?;

        if log.is_some() {
            log_packet.extend_from_slice(&header);
        }

        let version = header[0] >> 4;
        if version != 6 {
            debug!("Got an invalid IPv6 header from reader");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid IPv6 header",
            ));
        }

        let traffic_class = ((header[0] & 0x0F) << 4) | ((header[1] & 0xF0) >> 4);
        let flow_label =
            ((header[1] as u32 & 0x0F) << 16) | ((header[2] as u32) << 8) | (header[3] as u32);
        let payload_length = u16::from_be_bytes([header[4], header[5]]);
        let next_header = header[6];
        let hop_limit = header[7];
        let source = Ipv6Addr::new(
            u16::from_be_bytes([header[8], header[9]]),
            u16::from_be_bytes([header[10], header[11]]),
            u16::from_be_bytes([header[12], header[13]]),
            u16::from_be_bytes([header[14], header[15]]),
            u16::from_be_bytes([header[16], header[17]]),
            u16::from_be_bytes([header[18], header[19]]),
            u16::from_be_bytes([header[20], header[21]]),
            u16::from_be_bytes([header[22], header[23]]),
        );
        let destination = Ipv6Addr::new(
            u16::from_be_bytes([header[24], header[25]]),
            u16::from_be_bytes([header[26], header[27]]),
            u16::from_be_bytes([header[28], header[29]]),
            u16::from_be_bytes([header[30], header[31]]),
            u16::from_be_bytes([header[32], header[33]]),
            u16::from_be_bytes([header[34], header[35]]),
            u16::from_be_bytes([header[36], header[37]]),
            u16::from_be_bytes([header[38], header[39]]),
        );

        // Read the payload
        let mut payload = vec![0u8; payload_length as usize];
        reader.read_exact(&mut payload).await?;
        if let Some(log) = log {
            log_packet.extend_from_slice(&payload);
            super::log_packet(log, &log_packet);
        }

        Ok(Self {
            version,
            traffic_class,
            flow_label,
            payload_length,
            next_header,
            hop_limit,
            source,
            destination,
            payload,
        })
    }

    pub fn create(
        source: Ipv6Addr,
        destination: Ipv6Addr,
        next_header: ProtocolNumber,
        hop_limit: u8,
        payload: &[u8],
    ) -> Vec<u8> {
        let mut packet = Vec::with_capacity(40 + payload.len());

        // Version (6) and Traffic Class (0)
        let version_traffic_class = 6 << 4;
        packet.push(version_traffic_class);
        packet.push(0); // The rest of the Traffic Class and the start of the Flow Label

        // Flow Label (0)
        let flow_label = 0u16;
        packet.extend_from_slice(&flow_label.to_be_bytes()[..]);

        // Payload Length (length of the payload only)
        packet.extend_from_slice(&(payload.len() as u16).to_be_bytes());

        // Next Header and Hop Limit
        packet.push(next_header as u8);
        packet.push(hop_limit);

        // Source and Destination Addresses
        packet.extend_from_slice(&source.octets());
        packet.extend_from_slice(&destination.octets());

        // Payload
        packet.extend_from_slice(payload);

        packet
    }
}

impl std::fmt::Debug for Ipv6Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ipv6Packet")
            .field("version", &self.version)
            .field("traffic_class", &self.traffic_class)
            .field("flow_label", &self.flow_label)
            .field("payload_length", &self.payload_length)
            .field("next_header", &self.next_header)
            .field("hop_limit", &self.hop_limit)
            .field("source", &self.source)
            .field("destination", &self.destination)
            .field("payload len", &self.payload.len())
            .finish()
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TcpFlags {
    pub urg: bool, // Urgent pointer flag
    pub ack: bool, // Acknowledgment flag
    pub psh: bool, // Push flag
    pub rst: bool, // Reset flag
    pub syn: bool, // Synchronize flag
    pub fin: bool, // Finish flag
}

impl TcpFlags {
    /// Create a new `TcpFlags` struct from a raw byte.
    pub fn from_byte(flags: u8) -> Self {
        Self {
            urg: (flags & 0b0010_0000) != 0, // URG flag (bit 5)
            ack: (flags & 0b0001_0000) != 0, // ACK flag (bit 4)
            psh: (flags & 0b0000_1000) != 0, // PSH flag (bit 3)
            rst: (flags & 0b0000_0100) != 0, // RST flag (bit 2)
            syn: (flags & 0b0000_0010) != 0, // SYN flag (bit 1)
            fin: (flags & 0b0000_0001) != 0, // FIN flag (bit 0)
        }
    }

    /// Convert the `TcpFlags` struct into a raw byte.
    pub fn to_byte(&self) -> u8 {
        let mut flags = 0u8;
        if self.urg {
            flags |= 0b0010_0000;
        }
        if self.ack {
            flags |= 0b0001_0000;
        }
        if self.psh {
            flags |= 0b0000_1000;
        }
        if self.rst {
            flags |= 0b0000_0100;
        }
        if self.syn {
            flags |= 0b0000_0010;
        }
        if self.fin {
            flags |= 0b0000_0001;
        }
        flags
    }
}

pub struct TcpPacket {
    pub source_port: u16,
    pub destination_port: u16,
    pub sequence_number: u32,
    pub acknowledgment_number: u32,
    pub data_offset: u8, // Header length in 32-bit words
    pub flags: TcpFlags, // TCP flags
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
    pub options: Vec<u8>, // Optional TCP options
    pub payload: Vec<u8>, // TCP payload
}

impl TcpPacket {
    pub fn parse(packet: &[u8]) -> Result<Self, std::io::Error> {
        if packet.len() < 20 {
            debug!("Got an invalid TCP header");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Not enough bytes for TCP header",
            ));
        }

        let source_port = u16::from_be_bytes([packet[0], packet[1]]);
        let destination_port = u16::from_be_bytes([packet[2], packet[3]]);
        let sequence_number = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
        let acknowledgment_number =
            u32::from_be_bytes([packet[8], packet[9], packet[10], packet[11]]);
        let data_offset = (packet[12] >> 4) * 4; // Convert from 32-bit words to bytes
        let flags = TcpFlags::from_byte(packet[13]); // Parse flags
        let window_size = u16::from_be_bytes([packet[14], packet[15]]);
        let checksum = u16::from_be_bytes([packet[16], packet[17]]);
        let urgent_pointer = u16::from_be_bytes([packet[18], packet[19]]);

        // Parse options if the header is longer than 20 bytes
        let options_end = data_offset as usize;
        let options = if options_end > 20 {
            // packet[20..options_end].to_vec()
            Vec::new()
        } else {
            Vec::new()
        };

        // Payload starts after the header
        let payload = if packet.len() > options_end {
            packet[options_end..].to_vec()
        } else {
            Vec::new()
        };

        Ok(Self {
            source_port,
            destination_port,
            sequence_number,
            acknowledgment_number,
            data_offset,
            flags,
            window_size,
            checksum,
            urgent_pointer,
            options,
            payload,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        source_ip: IpAddr,
        destination_ip: IpAddr,
        source_port: u16,
        destination_port: u16,
        sequence_number: u32,
        acknowledgment_number: u32,
        flags: TcpFlags,
        window_size: u16,
        payload: &[u8],
    ) -> Vec<u8> {
        let data_offset = 5_u8; // Header length in 32-bit words
        let mut packet = Vec::with_capacity(20 + payload.len());

        // Source and Destination Ports
        packet.extend_from_slice(&source_port.to_be_bytes());
        packet.extend_from_slice(&destination_port.to_be_bytes());

        // Sequence and Acknowledgment Numbers
        packet.extend_from_slice(&sequence_number.to_be_bytes());
        packet.extend_from_slice(&acknowledgment_number.to_be_bytes());

        // Data Offset and Flags
        packet.push(data_offset << 4); // Data offset (4 bits) and reserved bits (4 bits)
        packet.push(flags.to_byte()); // Flags byte

        // Window Size, Checksum (set to zero first), and Urgent Pointer
        packet.extend_from_slice(&window_size.to_be_bytes());
        packet.extend_from_slice(&[0, 0]); // Checksum placeholder
        packet.extend_from_slice(&[0, 0]); // Urgent pointer

        // No options, keeping it simple
        packet.extend_from_slice(payload);

        // Compute checksum with the appropriate pseudo-header
        let checksum = match (source_ip, destination_ip) {
            (IpAddr::V4(src), IpAddr::V4(dest)) => {
                let src_bytes = src.octets();
                let dest_bytes = dest.octets();
                Self::calculate_checksum(&packet, &src_bytes, &dest_bytes, false)
            }
            (IpAddr::V6(src), IpAddr::V6(dest)) => {
                let src_bytes = src.octets();
                let dest_bytes = dest.octets();
                Self::calculate_checksum(&packet, &src_bytes, &dest_bytes, true)
            }
            _ => panic!("Source and destination IP versions must match"),
        };

        packet[16..18].copy_from_slice(&checksum.to_be_bytes());

        packet
    }

    fn calculate_checksum(
        packet: &[u8],
        source_ip: &[u8],
        destination_ip: &[u8],
        is_ipv6: bool,
    ) -> u16 {
        let mut sum = 0u32;

        if is_ipv6 {
            // IPv6 pseudo-header
            // Add source and destination addresses (128 bits each)
            for chunk in source_ip.chunks(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }
            for chunk in destination_ip.chunks(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }

            // Upper layer packet length (32 bits for IPv6)
            let tcp_length = packet.len() as u32;
            sum += (tcp_length >> 16) & 0xFFFF;
            sum += tcp_length & 0xFFFF;

            // Next Header value (8 bits of zeros + 8 bits of protocol value)
            sum += 6u32; // TCP protocol number
        } else {
            // IPv4 pseudo-header
            // Add source and destination addresses (32 bits each)
            for chunk in source_ip.chunks(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }
            for chunk in destination_ip.chunks(2) {
                sum += u16::from_be_bytes([chunk[0], chunk[1]]) as u32;
            }

            // Zero byte + Protocol byte
            sum += 6u32; // TCP protocol number

            // TCP segment length (16 bits)
            sum += packet.len() as u32;
        }

        // Create a copy of the packet with checksum field zeroed out
        let mut packet_copy = packet.to_vec();
        if packet_copy.len() >= 18 {
            packet_copy[16] = 0;
            packet_copy[17] = 0;
        }

        // Sum all 16-bit words in the packet
        for chunk in packet_copy.chunks(2) {
            let word = if chunk.len() == 2 {
                u16::from_be_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], 0]) // Padding for odd-length packets
            };
            sum += word as u32;
        }

        // Fold sum to 16 bits
        while sum >> 16 != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }

        // One's complement
        !(sum as u16)
    }
}

impl std::fmt::Debug for TcpPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TcpPacket")
            .field("source_port", &self.source_port)
            .field("destination_port", &self.destination_port)
            .field("sequence_number", &self.sequence_number)
            .field("acknowledgment_number", &self.acknowledgment_number)
            .field("data_offset", &self.data_offset)
            .field("flags", &self.flags)
            .field("window_size", &self.window_size)
            .field("checksum", &self.checksum)
            .field("urgent_pointer", &self.urgent_pointer)
            .field("options", &self.options)
            .field("payload len", &self.payload.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv4() {
        let b1 = Ipv4Packet::create(
            Ipv4Addr::new(127, 0, 0, 1),
            Ipv4Addr::new(1, 1, 1, 1),
            ProtocolNumber::Tcp,
            255,
            &[1, 2, 3, 4, 5],
        );
        println!("{b1:02X?}");

        let ip1 = Ipv4Packet::parse(&b1);
        println!("{ip1:#?}");
    }

    #[test]
    fn ipv6() {
        let b1 = Ipv6Packet::create(
            Ipv6Addr::new(1, 2, 3, 4, 5, 6, 7, 8),
            Ipv6Addr::new(9, 10, 11, 12, 13, 14, 15, 16),
            ProtocolNumber::Tcp,
            255,
            &[1, 2, 3, 4, 5],
        );
        println!("{b1:02X?}");

        let ip1 = Ipv6Packet::parse(&b1, &None);
        println!("{ip1:#?}");
    }

    #[test]
    fn tcp() {
        let b1 = TcpPacket::create(
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)),
            1234,
            5678,
            420,
            6969,
            TcpFlags {
                urg: false,
                ack: false,
                psh: true,
                rst: false,
                syn: false,
                fin: false,
            },
            5555,
            &[1, 2, 3, 4, 5],
        );
        let i1 = Ipv6Packet::create(
            Ipv6Addr::new(1, 2, 3, 4, 5, 6, 7, 8),
            Ipv6Addr::new(9, 10, 11, 12, 13, 14, 15, 16),
            ProtocolNumber::Tcp,
            255,
            &b1,
        );
        println!("{i1:02X?}");

        let t1 = TcpPacket::parse(&b1);
        println!("{t1:#?}");
    }
}
