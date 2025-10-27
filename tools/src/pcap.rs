use idevice::bt_packet_logger::BtPacketKind;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tracing::warn;

// Classic PCAP (big-endian) global header for DLT_BLUETOOTH_HCI_H4_WITH_PHDR (201)
const PCAP_GLOBAL_HEADER_BE: [u8; 24] = [
    0xA1, 0xB2, 0xC3, 0xD4, // magic (big-endian stream)
    0x00, 0x02, // version maj
    0x00, 0x04, // version min
    0x00, 0x00, 0x00, 0x00, // thiszone
    0x00, 0x00, 0x00, 0x00, // sigfigs
    0x00, 0x00, 0x08, 0x00, // snaplen = 2048
    0x00, 0x00, 0x00, 201, // network = 201 (HCI_H4_WITH_PHDR)
];

#[inline]
fn be32(x: u32) -> [u8; 4] {
    [(x >> 24) as u8, (x >> 16) as u8, (x >> 8) as u8, x as u8]
}

#[inline]
fn dir_flag(kind: BtPacketKind) -> Option<u32> {
    use BtPacketKind::*;
    Some(match kind {
        HciCmd | AclSent | ScoSent => 0,
        HciEvt | AclRecv | ScoRecv => 1,
        _ => return None,
    })
}

pub async fn write_pcap_header<W: AsyncWrite + Unpin>(w: &mut W) -> std::io::Result<()> {
    w.write_all(&PCAP_GLOBAL_HEADER_BE).await
}

pub async fn write_pcap_record<W: AsyncWrite + Unpin>(
    w: &mut W,
    ts_sec: u32,
    ts_usec: u32,
    kind: BtPacketKind,
    h4_payload: &[u8], // starts with H4 type followed by HCI bytes
) -> std::io::Result<()> {
    // Prepend 4-byte direction flag to the packet body
    let Some(dir) = dir_flag(kind) else {
        warn!("Unknown H4 packet type: {kind:?}");
        return Ok(());
    };
    let cap_len = 4u32 + h4_payload.len() as u32;

    // PCAP record header (big-endian fields to match magic above)
    // ts_sec, ts_usec, incl_len, orig_len
    let mut rec = [0u8; 16];
    rec[0..4].copy_from_slice(&be32(ts_sec));
    rec[4..8].copy_from_slice(&be32(ts_usec));
    rec[8..12].copy_from_slice(&be32(cap_len));
    rec[12..16].copy_from_slice(&be32(cap_len));

    // Write: rec hdr, dir flag (as 4 BE bytes), then H4 bytes
    w.write_all(&rec).await?;
    w.write_all(&be32(dir)).await?;
    w.write_all(h4_payload).await?;
    Ok(())
}
