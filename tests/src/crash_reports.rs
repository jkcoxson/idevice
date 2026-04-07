// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider,
    services::crashreportcopymobile::CrashReportCopyMobileClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("crash_reports: connect", success, failure, async {
        CrashReportCopyMobileClient::connect(provider)
            .await
            .map(|_| ())
    });

    let mut client = match CrashReportCopyMobileClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  crash_reports: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!("crash_reports: ls root", success, failure, async {
        let entries = client.ls(None).await?;
        println!("({} entries)", entries.len());
        Ok::<(), idevice::IdeviceError>(())
    });

    run_test!("crash_reports: ls /Diagnostics", success, failure, async {
        match client.ls(Some("/Diagnostics")).await {
            Ok(entries) => {
                println!("({} entries)", entries.len());
                Ok(())
            }
            // Directory may not exist on all devices — treat as soft pass
            Err(idevice::IdeviceError::Afc(e)) if e.to_string().contains("Object not found") => {
                println!("(not present on this device)");
                Ok(())
            }
            Err(e) => Err(e),
        }
    });
}
