// Jackson Coxson

use crate::run_test;
use idevice::{IdeviceService, provider::IdeviceProvider, services::heartbeat::HeartbeatClient};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("heartbeat: connect", success, failure, async {
        HeartbeatClient::connect(provider).await.map(|_| ())
    });

    let mut client = match HeartbeatClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  heartbeat: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "heartbeat: marco / polo exchange",
        success,
        failure,
        async {
            let interval = client.get_marco(30).await?;
            println!("(interval={interval}s)");
            client.send_polo().await
        }
    );
}
