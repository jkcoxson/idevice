// jkcoxson

use idevice::muxer;

#[tokio::main]
async fn main() {
    let device_list = match muxer::get_devices("idevice_id").await {
        Ok(device_list) => device_list,
        Err(e) => {
            println!("Unable to fetch device list: {}", e);
            return;
        }
    };

    for device in device_list {
        println!("{} ({})", device.serial_number, device.connection_type)
    }
}
