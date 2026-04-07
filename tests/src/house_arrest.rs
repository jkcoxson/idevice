// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::house_arrest::HouseArrestClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("house_arrest: connect", success, failure, async {
        HouseArrestClient::connect(provider).await.map(|_| ())
    });

    // vend_container consumes the client, requiring a fresh connect each time.
    // System apps typically reject house_arrest; treat PermDenied/ObjectNotFound as a
    // "not accessible" soft pass since we don't know what third-party apps are installed.
    run_test!(
        "house_arrest: vend_container + list_dir (com.apple.mobilenotes)",
        success,
        failure,
        async {
            let client = HouseArrestClient::connect(provider).await?;
            let mut afc = match client.vend_container("com.apple.mobilenotes").await {
                Ok(a) => a,
                Err(idevice::IdeviceError::UnknownErrorType(_)) => {
                    // Device returned e.g. InstallationLookupFailed — app not installed
                    println!("(app not found on this device - skipping)");
                    return Ok(());
                }
                Err(e) => return Err(e),
            };
            match afc.list_dir("/").await {
                Ok(entries) => {
                    println!("({} entries)", entries.len());
                    Ok(())
                }
                Err(idevice::IdeviceError::Afc(
                    idevice::services::afc::errors::AfcError::PermDenied,
                ))
                | Err(idevice::IdeviceError::Afc(
                    idevice::services::afc::errors::AfcError::ObjectNotFound,
                ))
                | Err(idevice::IdeviceError::Afc(
                    idevice::services::afc::errors::AfcError::OpNotSupported,
                )) => {
                    println!("(container not accessible - normal for system/locked apps)");
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
    );
}
