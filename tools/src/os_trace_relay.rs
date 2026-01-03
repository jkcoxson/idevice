// Jackson Coxson

use idevice::{IdeviceService, os_trace_relay::OsTraceRelayClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("Relay OS logs")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let log_client = OsTraceRelayClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    let mut relay = log_client.start_trace(None).await.expect("Start failed");

    loop {
        println!(
            "{:#?}",
            relay.next().await.expect("Failed to read next log")
        );
    }
}
