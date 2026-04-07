// Jackson Coxson
// pcapd captures live network packets from the device.
// Note: this service only works over USB (not Wi-Fi).

use std::time::Duration;

use crate::run_test;
use idevice::{IdeviceService, provider::IdeviceProvider, services::pcapd::PcapdClient};

const RECV_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("pcapd: connect", success, failure, async {
        PcapdClient::connect(provider).await.map(|_| ())
    });

    let mut client = match PcapdClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  pcapd: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    // next_packet blocks until a network packet arrives.  A real device always
    // has at least some background traffic, so this should complete quickly.
    run_test!("pcapd: next_packet", success, failure, async {
        match tokio::time::timeout(RECV_TIMEOUT, client.next_packet()).await {
            Ok(Ok(pkt)) => {
                println!("(iface={}, {} bytes)", pkt.interface_name, pkt.data.len());
                Ok(())
            }
            Ok(Err(e)) => Err(e),
            Err(_) => Err(idevice::IdeviceError::UnexpectedResponse(
                "timed out waiting for pcap packet".into(),
            )),
        }
    });
}
