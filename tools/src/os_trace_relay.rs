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

    const MAX_CONSECUTIVE_ERRORS: usize = 16;
    let mut consecutive_errors = 0usize;
    loop {
        match relay.next().await {
            Ok(log) => {
                consecutive_errors = 0;
                println!("{log:#?}");
            }
            Err(e) => {
                consecutive_errors += 1;
                eprintln!("skip log ({consecutive_errors}/{MAX_CONSECUTIVE_ERRORS}): {e:?}");
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    eprintln!("too many consecutive errors; stopping");
                    break;
                }
            }
        }
    }
}
