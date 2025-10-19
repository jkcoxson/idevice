// Jackson Coxson
// Gets the devices from the muxer

use futures_util::StreamExt;
use idevice::usbmuxd::UsbmuxdConnection;

#[tokio::main]
async fn main() {
    env_logger::init();

    let mut muxer = UsbmuxdConnection::default().await.unwrap();
    let res = muxer.get_devices().await.unwrap();
    println!("{res:#?}");

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "-l" {
        let mut s = muxer.listen().await.expect("listen failed");
        while let Some(dev) = s.next().await {
            let dev = dev.expect("failed to read from stream");
            println!("{dev:#?}");
        }
    }
}
