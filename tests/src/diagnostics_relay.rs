// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::diagnostics_relay::DiagnosticsRelayClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("diagnostics_relay: connect", success, failure, async {
        DiagnosticsRelayClient::connect(provider).await.map(|_| ())
    });

    let mut client = match DiagnosticsRelayClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  diagnostics_relay: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!(
        "diagnostics_relay: mobilegestalt UniqueChipID",
        success,
        failure,
        async {
            client
                .mobilegestalt(Some(vec!["UniqueChipID".to_string()]))
                .await
                .map(|_| ())
        }
    );

    run_test!("diagnostics_relay: gasguage", success, failure, async {
        client.gasguage().await.map(|_| ())
    });

    run_test!("diagnostics_relay: wifi", success, failure, async {
        client.wifi().await.map(|_| ())
    });

    run_test!(
        "diagnostics_relay: ioregistry (IOPMPowerSource)",
        success,
        failure,
        async {
            client
                .ioregistry(None, Some("IOPMPowerSource"), None)
                .await
                .map(|_| ())
        }
    );
}
