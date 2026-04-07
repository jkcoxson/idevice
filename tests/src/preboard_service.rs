// Jackson Coxson
// PreboardService manages the stashbag used by iOS Setup Assistant.
// The create_stashbag / commit_stashbag operations require a valid manifest,
// so we only test that the service can be connected to.

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::preboard_service::PreboardServiceClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("preboard_service: connect", success, failure, async {
        PreboardServiceClient::connect(provider).await.map(|_| ())
    });
}
