// jkcoxson

use idevice::muxer;

#[tokio::main]
async fn main() {
    muxer::get_devices("idevice_id").await.unwrap();
}
