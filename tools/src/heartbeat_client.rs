// Jackson Coxson
// Heartbeat client

use idevice::{IdeviceService, heartbeat::HeartbeatClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("heartbeat a device")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut heartbeat_client = HeartbeatClient::connect(&*provider)
        .await
        .expect("Unable to connect to heartbeat");

    let mut interval = 15;
    loop {
        interval = heartbeat_client.get_marco(interval).await.unwrap() + 5;
        heartbeat_client.send_polo().await.unwrap();
    }
}
