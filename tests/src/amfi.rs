// Jackson Coxson

use crate::run_test;
use idevice::{IdeviceService, provider::IdeviceProvider, services::amfi::AmfiClient};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("amfi: connect", success, failure, async {
        AmfiClient::connect(provider).await.map(|_| ())
    });

    let mut client = match AmfiClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  amfi: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!("amfi: get_developer_mode_status", success, failure, async {
        let enabled = client.get_developer_mode_status().await?;
        println!("(developer_mode={enabled})");
        Ok::<(), idevice::IdeviceError>(())
    });
}
