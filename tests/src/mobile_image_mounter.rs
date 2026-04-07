// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::mobile_image_mounter::ImageMounter,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("mobile_image_mounter: connect", success, failure, async {
        ImageMounter::connect(provider).await.map(|_| ())
    });

    let mut client = match ImageMounter::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  mobile_image_mounter: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "mobile_image_mounter: copy_devices",
        success,
        failure,
        async {
            let devices = client.copy_devices().await?;
            println!("({} mounted images)", devices.len());
            Ok::<(), idevice::IdeviceError>(())
        }
    );

    run_test!(
        "mobile_image_mounter: query_developer_mode_status",
        success,
        failure,
        async {
            let enabled = client.query_developer_mode_status().await?;
            println!("(developer mode = {enabled})");
            Ok::<(), idevice::IdeviceError>(())
        }
    );

    run_test!(
        "mobile_image_mounter: query_nonce (DeveloperDiskImage)",
        success,
        failure,
        async {
            match client.query_nonce(Some("DeveloperDiskImage")).await {
                Ok(nonce) => {
                    println!("({} bytes)", nonce.len());
                    Ok(())
                }
                // Not all devices support this image type — treat as soft pass
                Err(idevice::IdeviceError::NotFound) => {
                    println!("(DeveloperDiskImage nonce not available on this device)");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    );

    run_test!(
        "mobile_image_mounter: query_personalization_identifiers",
        success,
        failure,
        async {
            match client.query_personalization_identifiers(None).await {
                Ok(ids) => {
                    println!("({} identifiers)", ids.len());
                    Ok(())
                }
                // Older iOS may not support personalization identifiers
                Err(idevice::IdeviceError::UnexpectedResponse(_)) => {
                    println!("(not supported on this iOS version)");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    );
}
