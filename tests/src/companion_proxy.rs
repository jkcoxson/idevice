// Jackson Coxson
// CompanionProxy bridges to Apple Watch.  No watch needs to be paired for
// get_device_registry to succeed — it can return an empty list.

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::companion_proxy::CompanionProxy,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("companion_proxy: connect", success, failure, async {
        CompanionProxy::connect(provider).await.map(|_| ())
    });

    let mut client = match CompanionProxy::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  companion_proxy: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "companion_proxy: get_device_registry",
        success,
        failure,
        async {
            match client.get_device_registry().await {
                Ok(devices) => {
                    println!("({} paired devices)", devices.len());
                    Ok(())
                }
                Err(idevice::IdeviceError::UnknownErrorType(_)) => {
                    // UnsupportedDevice — no paired Apple Watch
                    println!("(device does not support companion proxy - skipping)");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    );
}
