// jkcoxson

use idevice::{lockdown, muxer};

#[tokio::main]
async fn main() {
    let device_list = match muxer::get_devices("idevice_id").await {
        Ok(device_list) => device_list,
        Err(e) => {
            println!("Unable to fetch device list: {}", e);
            return;
        }
    };

    println!("Devices found: {}", device_list.len());
    let lockdown_client = lockdown::LockdowndClient::new(&device_list[0], "asdf".to_string())
        .await
        .unwrap();
}
