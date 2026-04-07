// Jackson Coxson
// XCTest / WDA integration tests.
//
// Automatically discovers the WDA/IntegrationApp runner by searching installed
// apps for a name containing "WebDriverAgent" or "IntegrationApp". Override
// the discovered bundle ID by setting WDA_BUNDLE_ID.
//
// If no runner is found the entire module is skipped.
//
// Once WDA is up, the tests drive com.apple.Preferences (Settings) which is
// always present on every device.
//
// To install the runner,
// 1. git clone https://github.com/appium/appium-xcuitest-driver.git
// 2. Open that john in Xcode
// 3. Select WebDriverAgentRunner in the schemes (it doesn't show up as an app, it's fine)
// 4. Product -> Test
// 5. Trust in Settings -> General -> VPN and profile management

use std::{sync::Arc, time::Duration};

use idevice::{
    IdeviceService,
    dvt::xctest::{TestConfig, XCUITestService},
    provider::IdeviceProvider,
    services::{installation_proxy::InstallationProxyClient, wda::WdaClient},
};

use crate::run_test;

const WDA_READINESS_TIMEOUT: Duration = Duration::from_secs(120);
const SETTINGS_BUNDLE: &str = "com.apple.Preferences";
const RUNNER_NAME_KEYWORDS: &[&str] = &["webdriveragent", "integrationapp", "xctrunner"];

/// Search installed apps for a bundle whose display name or bundle name
/// contains one of the runner keywords (case-insensitive). Returns the
/// bundle ID of the first match.
async fn find_runner_bundle(provider: &dyn IdeviceProvider) -> Option<String> {
    // Honour explicit override first
    if let Ok(id) = std::env::var("WDA_BUNDLE_ID") {
        return Some(id);
    }

    let mut iproxy = InstallationProxyClient::connect(provider).await.ok()?;
    let apps = iproxy.get_apps(None, None).await.ok()?;

    for (bundle_id, info) in &apps {
        let dict = info.as_dictionary()?;

        let display_name = dict
            .get("CFBundleDisplayName")
            .or_else(|| dict.get("CFBundleName"))
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_lowercase();

        if RUNNER_NAME_KEYWORDS
            .iter()
            .any(|kw| display_name.contains(kw) || bundle_id.to_lowercase().contains(kw))
        {
            return Some(bundle_id.clone());
        }
    }

    None
}

pub async fn run_tests(provider: Arc<dyn IdeviceProvider>, success: &mut u32, failure: &mut u32) {
    let bundle_id = match find_runner_bundle(&*provider).await {
        Some(id) => id,
        None => {
            println!(
                "  xctest: no WDA/IntegrationApp runner found (set WDA_BUNDLE_ID to override), skipping"
            );
            return;
        }
    };

    println!("  xctest: runner: {bundle_id}");

    // Build TestConfig from the installed runner
    let cfg = {
        let mut iproxy = match InstallationProxyClient::connect(&*provider).await {
            Ok(c) => c,
            Err(e) => {
                println!("  xctest: InstallationProxy unavailable ({e}), skipping");
                *failure += 1;
                return;
            }
        };

        match TestConfig::from_installation_proxy(&mut iproxy, &bundle_id, None).await {
            Ok(c) => c,
            Err(e) => {
                println!("  xctest: TestConfig::from_installation_proxy failed ({e})");
                *failure += 1;
                return;
            }
        }
    };

    let service = XCUITestService::new(provider.clone());

    // Launch WDA and wait until it's reachable
    let handle = match service
        .run_until_wda_ready(cfg, WDA_READINESS_TIMEOUT)
        .await
    {
        Ok(h) => {
            println!(
                "  {:<60}\x1b[32m[ PASS ]\x1b[0m",
                "xctest: runner started + WDA ready"
            );
            *success += 1;
            h
        }
        Err(e) => {
            println!(
                "  {:<60}\x1b[31m[ FAIL ]\x1b[0m {e}",
                "xctest: runner started + WDA ready"
            );
            *failure += 1;
            return;
        }
    };

    let mut wda = WdaClient::new(&*provider).with_ports(handle.ports());

    run_test!("xctest: WDA status", success, failure, async {
        wda.status().await.map(|_| ())
    });

    // Open a session targeting Settings — always present, no extra install needed
    let session_id = match wda.start_session(Some(SETTINGS_BUNDLE)).await {
        Ok(id) => {
            println!(
                "  {:<60}\x1b[32m[ PASS ]\x1b[0m (session={id})",
                "xctest: WDA start_session Settings"
            );
            *success += 1;
            id
        }
        Err(e) => {
            println!(
                "  {:<60}\x1b[31m[ FAIL ]\x1b[0m {e}",
                "xctest: WDA start_session Settings"
            );
            *failure += 1;
            handle.abort();
            return;
        }
    };

    let sid = Some(session_id.as_str());

    run_test!(
        "xctest: WDA screenshot (Settings)",
        success,
        failure,
        async {
            let bytes = wda.screenshot(sid).await?;
            if bytes.is_empty() {
                Err(idevice::IdeviceError::UnexpectedResponse(
                    "screenshot returned empty bytes".into(),
                ))
            } else {
                println!("({} bytes)", bytes.len());
                Ok(())
            }
        }
    );

    run_test!(
        "xctest: WDA orientation (Settings)",
        success,
        failure,
        async {
            let o = wda.orientation(sid).await?;
            println!("({o})");
            Ok::<(), idevice::IdeviceError>(())
        }
    );

    run_test!(
        "xctest: WDA window_size (Settings)",
        success,
        failure,
        async {
            let size = wda.window_size(sid).await?;
            println!(
                "(w={}, h={})",
                size.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0),
                size.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0),
            );
            Ok::<(), idevice::IdeviceError>(())
        }
    );

    run_test!("xctest: WDA source (Settings)", success, failure, async {
        let src = wda.source(sid).await?;
        if src.is_empty() {
            Err(idevice::IdeviceError::UnexpectedResponse(
                "page source was empty".into(),
            ))
        } else {
            println!("({} bytes)", src.len());
            Ok(())
        }
    });

    run_test!(
        "xctest: WDA find + click Bluetooth",
        success,
        failure,
        async {
            let element_id = wda
                .find_element("predicate string", "label BEGINSWITH 'Bluetooth'", sid)
                .await?;
            wda.click(&element_id, sid).await
        }
    );

    // After tapping Bluetooth the nav bar back-button label becomes "Settings",
    // confirming we navigated to a sub-page.
    run_test!(
        "xctest: WDA verify navigation to Bluetooth",
        success,
        failure,
        async {
            wda.find_element(
                "predicate string",
                "label == 'Settings' AND type == 'XCUIElementTypeButton'",
                sid,
            )
            .await
            .map(|_| ())
        }
    );

    run_test!("xctest: WDA delete session", success, failure, async {
        wda.delete_session(&session_id).await
    });

    handle.abort();
}
