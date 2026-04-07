// Jackson Coxson
// MobileActivationd is unusual: it requires a new lockdown connection per request,
// so MobileActivationdClient takes a provider reference rather than a single session.

use crate::run_test;
use idevice::{provider::IdeviceProvider, services::mobileactivationd::MobileActivationdClient};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    let client = MobileActivationdClient::new(provider);

    run_test!("mobileactivationd: state", success, failure, async {
        let state = client.state().await?;
        println!("({state})");
        Ok::<(), idevice::IdeviceError>(())
    });

    run_test!("mobileactivationd: activated", success, failure, async {
        let is_activated = client.activated().await?;
        println!("({is_activated})");
        Ok::<(), idevice::IdeviceError>(())
    });
}
