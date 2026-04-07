// XCTest runner tool - launches an XCTest bundle (e.g. WebDriverAgent) on device.
// Usage:
//   idevice-tools xctest <runner_bundle_id> [target_bundle_id]
//
// Example (WDA):
//   idevice-tools xctest io.github.kor1k1.WebDriverAgentRunner.xctrunner
//   idevice-tools xctest --bridge io.github.kor1k1.WebDriverAgentRunner.xctrunner

use std::{sync::Arc, time::Duration};

use idevice::{
    IdeviceError, IdeviceService,
    provider::IdeviceProvider,
    services::dvt::xctest::{TestConfig, XCUITestService, listener::XCUITestListener},
    services::installation_proxy::InstallationProxyClient,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Launch an XCTest runner bundle (e.g. WebDriverAgent) on a connected device")
        .with_flag(
            JkFlag::new("wda-debug-log")
                .with_help("Print verbose WebDriverAgent/XCTest debug log lines from the runner"),
        )
        .with_flag(
            JkFlag::new("bridge")
                .with_help("Wait for WDA and expose localhost bridge URLs for HTTP and MJPEG"),
        )
        .with_flag(
            JkFlag::new("wda-timeout")
                .with_argument(JkArgument::new().required(true))
                .with_help("WDA readiness timeout in seconds when --bridge is used (default: 30)"),
        )
        .with_argument(
            JkArgument::new()
                .required(true)
                .with_help(
                    "Bundle ID of the .xctrunner app (e.g. io.github.kor1k1.WebDriverAgentRunner.xctrunner)",
                ),
        )
        .with_argument(
            JkArgument::new()
                .required(false)
                .with_help("Optional target app bundle ID under test"),
        )
}

/// Minimal listener that prints test lifecycle events to stdout.
struct PrintListener {
    show_wda_debug_logs: bool,
}

impl XCUITestListener for PrintListener {
    async fn did_begin_executing_test_plan(&mut self) -> Result<(), IdeviceError> {
        println!("[XCTest] Test plan started");
        Ok(())
    }

    async fn did_finish_executing_test_plan(&mut self) -> Result<(), IdeviceError> {
        println!("[XCTest] Test plan finished");
        Ok(())
    }

    async fn test_runner_ready_with_capabilities(&mut self) -> Result<(), IdeviceError> {
        println!("[XCTest] Runner ready. Automation session is live.");
        println!(
            "[XCTest] If this is WDA, device-side endpoints are usually HTTP :8100 and MJPEG :9100."
        );
        Ok(())
    }

    async fn test_suite_did_start(
        &mut self,
        suite: &str,
        started_at: &str,
    ) -> Result<(), IdeviceError> {
        println!("[XCTest] Suite start: {} @ {}", suite, started_at);
        Ok(())
    }

    async fn test_case_did_start(
        &mut self,
        test_class: &str,
        method: &str,
    ) -> Result<(), IdeviceError> {
        println!("[XCTest]   CASE START: {}/{}", test_class, method);
        Ok(())
    }

    async fn test_case_did_finish(
        &mut self,
        result: idevice::services::dvt::xctest::listener::XCTestCaseResult,
    ) -> Result<(), IdeviceError> {
        println!(
            "[XCTest]   CASE END:   {}/{} -> {} ({:.3}s)",
            result.test_class, result.method, result.status, result.duration
        );
        Ok(())
    }

    async fn test_case_did_fail(
        &mut self,
        test_class: &str,
        method: &str,
        message: &str,
        file: &str,
        line: u64,
    ) -> Result<(), IdeviceError> {
        eprintln!(
            "[XCTest]   FAIL: {}/{} - {} ({}:{})",
            test_class, method, message, file, line
        );
        Ok(())
    }

    async fn log_message(&mut self, message: &str) -> Result<(), IdeviceError> {
        println!("[WDA] {}", message);
        Ok(())
    }

    async fn log_debug_message(&mut self, message: &str) -> Result<(), IdeviceError> {
        if self.show_wda_debug_logs {
            println!("[WDA DBG] {}", message);
        }
        Ok(())
    }

    async fn initialization_for_ui_testing_did_fail(
        &mut self,
        description: &str,
    ) -> Result<(), IdeviceError> {
        eprintln!("[XCTest] UI testing init FAILED: {}", description);
        Ok(())
    }

    async fn did_fail_to_bootstrap(&mut self, description: &str) -> Result<(), IdeviceError> {
        eprintln!("[XCTest] Bootstrap FAILED: {}", description);
        Ok(())
    }
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    if let Err(e) = run(arguments, provider).await {
        eprintln!("[XCTest] Error: {}", e);
        std::process::exit(1);
    }
}

async fn run(
    arguments: &CollectedArguments,
    provider: Box<dyn IdeviceProvider>,
) -> Result<(), IdeviceError> {
    let mut arguments = arguments.clone();
    let use_bridge = arguments.has_flag("bridge");
    let show_wda_debug_logs = arguments.has_flag("wda-debug-log");
    let wda_timeout = arguments.get_flag::<f64>("wda-timeout").unwrap_or(30.0);
    let runner_bundle_id: String = arguments
        .next_argument()
        .expect("runner bundle ID is required");
    let target_bundle_id: Option<String> = arguments.next_argument();

    println!("[XCTest] Runner:  {}", runner_bundle_id);
    if let Some(ref t) = target_bundle_id {
        println!("[XCTest] Target:  {}", t);
    }

    let cfg = build_test_config(
        provider.as_ref(),
        &runner_bundle_id,
        target_bundle_id.as_deref(),
    )
    .await?;

    println!("[XCTest] App path:      {}", cfg.runner_app_path);
    println!("[XCTest] Container:     {}", cfg.runner_app_container);
    println!("[XCTest] Executable:    {}", cfg.runner_bundle_executable);

    let provider: Arc<dyn IdeviceProvider> = Arc::from(provider);
    let svc = XCUITestService::new(provider);
    let mut listener = PrintListener {
        show_wda_debug_logs,
    };

    println!("[XCTest] Launching runner - this may take 15-30s ...");

    if use_bridge {
        let handle = svc
            .run_until_wda_ready_with_bridge(cfg, Duration::from_secs_f64(wda_timeout))
            .await?;
        let endpoints = handle.bridge().endpoints();
        println!("[XCTest] WDA is ready and bridged to localhost.");
        if let Some(udid) = endpoints.udid.as_deref() {
            println!("[XCTest] Device UDID: {}", udid);
        }
        println!("[XCTest] WDA URL:    {}", endpoints.wda_url);
        println!("[XCTest] MJPEG URL:  {}", endpoints.mjpeg_url);
        println!(
            "[XCTest] Local ports: HTTP {} -> device {}, MJPEG {} -> device {}",
            endpoints.local_ports.http,
            endpoints.device_ports.http,
            endpoints.local_ports.mjpeg,
            endpoints.device_ports.mjpeg
        );
        println!("[XCTest] Bridge is live. Press Ctrl+C to stop.");
        handle.wait().await?;
    } else {
        svc.run(cfg, &mut listener, None).await?;
        println!("[XCTest] Done.");
    }

    Ok(())
}
async fn build_test_config(
    provider: &dyn IdeviceProvider,
    runner_bundle_id: &str,
    target_bundle_id: Option<&str>,
) -> Result<TestConfig, IdeviceError> {
    let mut install_proxy = InstallationProxyClient::connect(provider).await?;
    TestConfig::from_installation_proxy(&mut install_proxy, runner_bundle_id, target_bundle_id)
        .await
}
