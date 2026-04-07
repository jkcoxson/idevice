// Jackson Coxson

use std::time::Duration;

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::os_trace_relay::OsTraceRelayClient,
};

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("os_trace_relay: connect", success, failure, async {
        OsTraceRelayClient::connect(provider).await.map(|_| ())
    });

    let mut client = match OsTraceRelayClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  os_trace_relay: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!("os_trace_relay: get_pid_list", success, failure, async {
        let pids = client.get_pid_list().await?;
        if pids.is_empty() {
            Err(idevice::IdeviceError::UnexpectedResponse(
                "pid list was empty".into(),
            ))
        } else {
            println!("({} pids)", pids.len());
            Ok(())
        }
    });

    // start_trace consumes the client; use a fresh connection so the PidList
    // exchange above doesn't leave the socket in an unexpected state.
    run_test!(
        "os_trace_relay: start_trace + read 1 entry",
        success,
        failure,
        async {
            let fresh = OsTraceRelayClient::connect(provider).await?;
            let mut receiver = fresh.start_trace(None).await?;
            match tokio::time::timeout(RECV_TIMEOUT, receiver.next()).await {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_) => Err(idevice::IdeviceError::UnexpectedResponse(
                    "timed out waiting for os_trace entry".into(),
                )),
            }
        }
    );
}
