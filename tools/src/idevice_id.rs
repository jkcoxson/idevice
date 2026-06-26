// Jackson Coxson
// Gets the devices from the muxer

use futures_util::StreamExt;
use idevice::usbmuxd::{UsbmuxdAddr, UsbmuxdConnection};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let addr = UsbmuxdAddr::from_env_var().expect("Failed to parse USBMUXD_SOCKET_ADDRESS");
    let muxer = addr.to_socket().await.unwrap();
    let mut muxer = UsbmuxdConnection::new(muxer, 0);
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
