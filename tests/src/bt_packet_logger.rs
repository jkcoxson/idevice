// Jackson Coxson
// Note: BTPacketLogger requires the Bluetooth profile to be installed on the device.
// Without it, the service connects but delivers no data.
// https://developer.apple.com/bug-reporting/profiles-and-logs/?name=bluetooth

use std::time::Duration;

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::bt_packet_logger::BtPacketLoggerClient,
};

const RECV_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("bt_packet_logger: connect", success, failure, async {
        BtPacketLoggerClient::connect(provider).await.map(|_| ())
    });

    let mut client = match BtPacketLoggerClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  bt_packet_logger: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    // next_packet will block until a BT frame arrives.  On a device without the
    // Bluetooth profile installed the service connects but never sends data, so we
    // time out — that is treated as a soft pass (service is up, no profile installed).
    run_test!(
        "bt_packet_logger: next_packet (timeout = soft pass)",
        success,
        failure,
        async {
            match tokio::time::timeout(RECV_TIMEOUT, client.next_packet()).await {
                Ok(Ok(Some(_))) => {
                    println!("(received BT packet)");
                    Ok(())
                }
                Ok(Ok(None)) => {
                    println!("(EOF - no BT profile installed?)");
                    Ok(())
                }
                Ok(Err(e)) => Err(e),
                Err(_) => {
                    println!("(timed out - BT profile likely not installed)");
                    Ok(())
                }
            }
        }
    );
}
