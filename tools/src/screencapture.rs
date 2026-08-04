// Jackson Coxson

use std::time::Duration;

use idevice::{
    IdeviceService,
    core_device::{HevcDepacketizer, RtpPacket},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

use crate::coredevice_stream::start_screen_media_stream;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Stream the device display over com.apple.coredevice.displayservice")
        .with_flag(JkFlag::new("stdout").with_help(
            "Stream the Annex-B HEVC to stdout for live playback \
                 (pipe into `ffplay -`); runs until interrupted",
        ))
        .with_argument(
            JkArgument::new()
                .with_help(
                    "Output path for the Annex-B HEVC elementary stream \
                     (default /tmp/coredevice_stream.h265; ignored with --stdout)",
                )
                .required(false),
        )
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let to_stdout = arguments.get_flag::<String>("stdout").is_some();
    let out_path = arguments
        .clone()
        .next_argument::<String>()
        .unwrap_or_else(|| "/tmp/coredevice_stream.h265".to_string());

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let session = match start_screen_media_stream(&mut adapter, &mut handshake, 1).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to start screen media stream: {e}");
            return;
        }
    };
    let mut client = session.client;
    let udp = session.video_udp;
    let receiver_ip = adapter.host_ip().to_string();
    let receiver_port = udp.local_port();

    // Receive RTP, depacketize HEVC (RFC 7798), and write an Annex-B elementary
    // stream. With `--stdout` we stream to stdout (live: pipe into `ffplay -`);
    // else we write a `.hevc` file ffmpeg/VLC can open directly.
    use std::io::Write;
    let mut sink: Box<dyn Write + Send> = if to_stdout {
        Box::new(std::io::BufWriter::new(std::io::stdout()))
    } else {
        Box::new(std::io::BufWriter::new(
            std::fs::File::create(&out_path).expect("create output"),
        ))
    };
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let writer = std::thread::spawn(move || {
        while let Ok(buf) = rx.recv() {
            if sink.write_all(&buf).is_err() {
                // Player closed the pipe (e.g. ffplay window closed).
                break;
            }
            if to_stdout {
                let _ = sink.flush();
            }
        }
        let _ = sink.flush();
    });
    if to_stdout {
        eprintln!("streaming Annex-B HEVC to stdout (pipe into ffplay -); Ctrl-C to stop");
    } else {
        eprintln!("receiving RTP on {receiver_ip}:{receiver_port} -> {out_path}");
    }

    let mut depacketizer = HevcDepacketizer::new();
    let mut count = 0usize;
    let mut nal_bytes = 0usize;
    loop {
        match tokio::time::timeout(Duration::from_secs(5), udp.recv()).await {
            Ok(Ok(dg)) => {
                if let Some(pkt) = RtpPacket::parse(&dg.data) {
                    if count == 0 {
                        eprintln!(
                            "first RTP: pt={} seq={} ts={} ssrc={:#x} marker={} payload={}B",
                            pkt.payload_type,
                            pkt.sequence_number,
                            pkt.timestamp,
                            pkt.ssrc,
                            pkt.marker,
                            pkt.payload.len()
                        );
                    }
                    depacketizer.push(pkt.sequence_number, pkt.timestamp, pkt.payload);
                    let out = depacketizer.take_output();
                    if !out.is_empty() {
                        nal_bytes += out.len();
                        if tx.send(out).is_err() {
                            // Writer thread stopped: the sink is gone (e.g. ffplay
                            // window closed and the pipe broke).
                            eprintln!("output closed; stopping");
                            break;
                        }
                    }
                    count += 1;
                } else {
                    eprintln!(
                        "non-RTP datagram: {}B from port {}",
                        dg.data.len(),
                        dg.source_port
                    );
                }
            }
            Ok(Err(e)) => {
                eprintln!("udp recv error: {e:?}");
                break;
            }
            Err(_) => {
                if to_stdout {
                    // Keep waiting; the screen may simply be idle.
                    continue;
                }
                eprintln!("no RTP for 5s; stopping");
                break;
            }
        }
    }
    drop(tx);
    let _ = writer.join();
    eprintln!(
        "received {count} RTP packets, wrote {nal_bytes} Annex-B bytes{}",
        if depacketizer.has_parameter_sets() {
            ""
        } else {
            " (warning: no VPS/SPS/PPS seen — stream may not decode)"
        }
    );

    if let Err(e) = client.stop_media_stream().await {
        eprintln!("stop_media_stream failed: {e:?}");
    }
}
