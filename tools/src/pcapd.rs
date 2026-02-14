// Jackson Coxson

use idevice::{
    IdeviceService,
    pcapd::{PcapFileWriter, PcapdClient},
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Writes pcap network data")
        .with_argument(JkArgument::new().with_help("Write PCAP to this file (use '-' for stdout)"))
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let out = arguments.clone().next_argument::<String>();

    let mut logger_client = PcapdClient::connect(&*provider)
        .await
        .expect("Failed to connect to pcapd! This service is only available over USB!");

    logger_client.next_packet().await.unwrap();

    // Open output (default to stdout if --out omitted)
    let mut out_writer = match out.as_deref() {
        Some(path) => Some(
            PcapFileWriter::new(tokio::fs::File::create(path).await.expect("open pcap"))
                .await
                .expect("write header"),
        ),
        _ => None,
    };

    println!("Starting packet stream");
    loop {
        let packet = logger_client
            .next_packet()
            .await
            .expect("failed to read next packet");
        if let Some(writer) = &mut out_writer {
            writer.write_packet(&packet).await.expect("write packet");
        } else {
            println!("{packet:?}");
        }
    }
}
