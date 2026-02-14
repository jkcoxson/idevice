// Jackson Coxson

use idevice::{IdeviceService, provider::IdeviceProvider, syslog_relay::SyslogRelayClient};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("Relay system logs")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut log_client = SyslogRelayClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    loop {
        println!(
            "{}",
            log_client.next().await.expect("Failed to read next log")
        );
    }
}
