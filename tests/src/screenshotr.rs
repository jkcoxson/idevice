// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::screenshotr::ScreenshotService,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("screenshotr: connect", success, failure, async {
        match ScreenshotService::connect(provider).await {
            Ok(_) => Ok(()),
            // iOS 17+ removed this service in favour of DVT screenshot
            Err(idevice::IdeviceError::UnknownErrorType(ref s)) if s.contains("InvalidService") => {
                println!("(not available on iOS 17+, DVT screenshot used instead)");
                Ok(())
            }
            Err(e) => Err(e),
        }
    });

    let mut client = match ScreenshotService::connect(provider).await {
        Ok(c) => c,
        Err(e) if e.to_string().contains("InvalidService") => {
            println!("  screenshotr: service unavailable on this iOS version, skipping");
            return;
        }
        Err(e) => {
            println!("  screenshotr: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "screenshotr: take_screenshot returns non-empty PNG",
        success,
        failure,
        async {
            let bytes = client.take_screenshot().await?;
            if bytes.is_empty() {
                Err(idevice::IdeviceError::UnexpectedResponse(
                    "screenshot was empty".into(),
                ))
            } else {
                Ok(())
            }
        }
    );
}
