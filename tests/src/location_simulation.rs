// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider,
    services::simulate_location::LocationSimulationService,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("location_simulation: connect", success, failure, async {
        match LocationSimulationService::connect(provider).await {
            Ok(_) => Ok(()),
            // iOS 17+ removed this lockdown service; use DVT location simulation instead
            Err(idevice::IdeviceError::UnknownErrorType(ref s)) if s.contains("InvalidService") => {
                println!("(not available on iOS 17+, DVT location simulation used instead)");
                Ok(())
            }
            Err(e) => Err(e),
        }
    });

    let mut client = match LocationSimulationService::connect(provider).await {
        Ok(c) => c,
        Err(e) if e.to_string().contains("InvalidService") => {
            println!("  location_simulation: service unavailable on this iOS version, skipping");
            return;
        }
        Err(e) => {
            println!("  location_simulation: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    // Set a fake GPS coordinate (Cupertino, CA)
    run_test!(
        "location_simulation: set (37.3318, -122.0312)",
        success,
        failure,
        async { client.set("37.3318", "-122.0312").await }
    );

    // Clear the simulated location
    run_test!("location_simulation: clear", success, failure, async {
        client.clear().await
    });
}
