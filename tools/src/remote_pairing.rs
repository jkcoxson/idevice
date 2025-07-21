// Jackson Coxson

use idevice::{
    remote_pairing::{rp_pairing_file::RpPairingFile, RPPairingClient},
    IdeviceError,
};

#[tokio::main]
async fn main() -> Result<(), IdeviceError> {
    env_logger::init();
    let conn = tokio::net::TcpStream::connect("192.168.50.247:49152")
        .await
        .unwrap();

    let mut client = RPPairingClient::new(conn);
    client.handshake().await?;
    let pairing = RpPairingFile::generate();
    client
        .validate_pairing(pairing)
        .await
        .expect("No validate?");
    client.pair().await?;

    Ok(())
}
