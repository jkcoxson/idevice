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
    IdeviceError, IdeviceService,
    dvt::xctest::{TestConfig, XCUITestService},
    provider::IdeviceProvider,
    services::{
        installation_proxy::InstallationProxyClient,
        wda::{WdaClient, WdaPorts},
    },
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, sleep, timeout},
};

use crate::run_test;

const WDA_READINESS_TIMEOUT: Duration = Duration::from_secs(120);
const BRIDGE_READINESS_TIMEOUT: Duration = Duration::from_secs(30);
const BRIDGE_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_HTTP_HEADER_SIZE: usize = 64 * 1024;
const WDA_TEST_PORTS: WdaPorts = WdaPorts {
    http: 8200,
    mjpeg: 9200,
};
const SETTINGS_BUNDLE: &str = "com.apple.Preferences";
const RUNNER_NAME_KEYWORDS: &[&str] = &["webdriveragent", "integrationapp", "xctrunner"];

async fn read_bridge_response_head(port: u16, path: &str) -> Result<String, IdeviceError> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;

    let mut response = Vec::new();
    let head_end = timeout(BRIDGE_REQUEST_TIMEOUT, async {
        let mut chunk = [0u8; 1024];
        loop {
            let read = stream.read(&mut chunk).await?;
            if read == 0 {
                return Err(IdeviceError::UnexpectedResponse(
                    "bridge closed before returning HTTP headers".into(),
                ));
            }
            response.extend_from_slice(&chunk[..read]);

            if let Some(index) = response.windows(4).position(|window| window == b"\r\n\r\n") {
                return Ok(index + 4);
            }
            if response.len() > MAX_HTTP_HEADER_SIZE {
                return Err(IdeviceError::UnexpectedResponse(
                    "bridge returned oversized HTTP headers".into(),
                ));
            }
        }
    })
    .await
    .map_err(|_| IdeviceError::Timeout)??;

    response.truncate(head_end);
    String::from_utf8(response).map_err(IdeviceError::from)
}

async fn wait_for_bridge_response_head(port: u16, path: &str) -> Result<String, IdeviceError> {
    let deadline = Instant::now() + BRIDGE_READINESS_TIMEOUT;
    loop {
        match read_bridge_response_head(port, path).await {
            Ok(response) => return Ok(response),
            Err(_) if Instant::now() < deadline => sleep(Duration::from_millis(250)).await,
            Err(error) => return Err(error),
        }
    }
}

fn verify_bridge_response(
    response_head: &str,
    expected_content_type: Option<&str>,
) -> Result<(), IdeviceError> {
    let status = response_head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u16>().ok());
    if !status.is_some_and(|status| (200..300).contains(&status)) {
        return Err(IdeviceError::UnexpectedResponse(format!(
            "bridge returned unsuccessful HTTP status: {:?}",
            response_head.lines().next()
        )));
    }

    if let Some(expected) = expected_content_type
        && !response_head
            .to_ascii_lowercase()
            .contains(&expected.to_ascii_lowercase())
    {
        return Err(IdeviceError::UnexpectedResponse(format!(
            "bridge response did not contain content type {expected:?}"
        )));
    }

    Ok(())
}

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
    let mut cfg = {
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
    let runner_env = cfg.runner_env.get_or_insert_default();
    runner_env.insert("USE_PORT".into(), WDA_TEST_PORTS.http.to_string().into());
    runner_env.insert(
        "MJPEG_SERVER_PORT".into(),
        WDA_TEST_PORTS.mjpeg.to_string().into(),
    );

    let service = XCUITestService::new(provider.clone());

    // Launch WDA on non-default ports, wait until it's reachable, and start the bridge
    let handle = match service
        .run_until_wda_ready_with_bridge(cfg, WDA_READINESS_TIMEOUT)
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

    run_test!("xctest: WDA custom device ports", success, failure, async {
        let endpoints = handle.bridge().endpoints();
        if handle.ports() == WDA_TEST_PORTS && endpoints.device_ports == WDA_TEST_PORTS {
            Ok(())
        } else {
            Err(idevice::IdeviceError::UnexpectedResponse(format!(
                "expected device ports {:?}, got runner {:?} and bridge {:?}",
                WDA_TEST_PORTS,
                handle.ports(),
                endpoints.device_ports
            )))
        }
    });

    let local_ports = handle.bridge().endpoints().local_ports;
    run_test!("xctest: WDA HTTP bridge status", success, failure, async {
        let response = wait_for_bridge_response_head(local_ports.http, "/status").await?;
        verify_bridge_response(&response, Some("application/json"))
    });
    run_test!("xctest: WDA MJPEG bridge stream", success, failure, async {
        let response = wait_for_bridge_response_head(local_ports.mjpeg, "/").await?;
        verify_bridge_response(&response, Some("multipart/x-mixed-replace"))
    });

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
