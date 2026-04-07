// Jackson Coxson

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider,
    services::installation_proxy::InstallationProxyClient,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!("instproxy: connect", success, failure, async {
        InstallationProxyClient::connect(provider).await.map(|_| ())
    });

    let mut client = match InstallationProxyClient::connect(provider).await {
        Ok(c) => c,
        Err(e) => {
            println!("  instproxy: cannot connect ({e}), skipping remaining tests");
            *failure += 1;
            return;
        }
    };

    run_test!("instproxy: get_apps (User)", success, failure, async {
        let apps = client.get_apps(Some("User"), None).await?;
        println!("({} apps)", apps.len());
        Ok::<(), idevice::IdeviceError>(())
    });

    run_test!("instproxy: get_apps (System)", success, failure, async {
        client
            .get_apps(Some("System"), None)
            .await
            .map(|apps| println!("({} apps)", apps.len()))
    });

    run_test!("instproxy: get_apps (Any)", success, failure, async {
        client
            .get_apps(Some("Any"), None)
            .await
            .map(|apps| println!("({} apps)", apps.len()))
    });

    // Filter by a well-known bundle ID to exercise the filter path
    run_test!(
        "instproxy: get_apps filtered (com.apple.Preferences)",
        success,
        failure,
        async {
            client
                .get_apps(
                    Some("System"),
                    Some(vec!["com.apple.Preferences".to_string()]),
                )
                .await
                .map(|_| ())
        }
    );
}
