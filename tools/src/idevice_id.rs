// Jackson Coxson
// Gets the devices from the muxer

use idevice::usbmuxd::UsbmuxdConnection;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut muxer = UsbmuxdConnection::default().await.unwrap();
    let res = muxer.get_devices().await.unwrap();
    println!("{res:#?}");
}
