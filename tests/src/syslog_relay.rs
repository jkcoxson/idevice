// Jackson Coxson

use std::time::Duration;

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::syslog_relay::SyslogRelayClient,
};

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("syslog_relay: connect", success, failure, async {
        SyslogRelayClient::connect(provider).await.map(|_| ())
    });

    let mut client = match SyslogRelayClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  syslog_relay: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    // Read the first 3 syslog lines (device is always logging something)
    run_test!("syslog_relay: read 3 log lines", success, failure, async {
        for i in 0..3 {
            match tokio::time::timeout(RECV_TIMEOUT, client.next()).await {
                Ok(Ok(line)) => {
                    if i == 0 {
                        print!("(first={:?}...) ", line.get(..40).unwrap_or(&line));
                    }
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "timed out waiting for syslog line".into(),
                    ));
                }
            }
        }
        Ok(())
    });
}
