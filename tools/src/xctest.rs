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
    services::dvt::{
        errors::DvtError,
        xctest::{TestConfig, XCUITestService, listener::XCUITestListener},
    },
    services::installation_proxy::InstallationProxyClient,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist::{Dictionary, Value};

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
        .with_flag(
            JkFlag::new("env")
                .with_argument(JkArgument::new().required(true))
                .with_help(
                    "Runner environment as comma-separated KEY=VALUE pairs (escape commas as \\,)",
                ),
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
            "[XCTest] WDA device-side ports follow USE_PORT and MJPEG_SERVER_PORT from --env."
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
    let runner_env = arguments
        .get_flag::<String>("env")
        .map(|value| parse_runner_environment(&value))
        .transpose()?;
    let runner_bundle_id: String = arguments
        .next_argument()
        .expect("runner bundle ID is required");
    let target_bundle_id: Option<String> = arguments.next_argument();

    println!("[XCTest] Runner:  {}", runner_bundle_id);
    if let Some(ref t) = target_bundle_id {
        println!("[XCTest] Target:  {}", t);
    }

    let mut cfg = build_test_config(
        provider.as_ref(),
        &runner_bundle_id,
        target_bundle_id.as_deref(),
    )
    .await?;
    cfg.runner_env = runner_env;

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

fn parse_runner_environment(input: &str) -> Result<Dictionary, DvtError> {
    if input.is_empty() {
        return Err(DvtError::InvalidXCTestRunnerEnvironment(
            "environment cannot be empty".into(),
        ));
    }

    let mut environment = Dictionary::new();
    let mut entry = String::new();
    let mut escaped = false;

    for character in input.chars() {
        if escaped {
            match character {
                ',' | '\\' => entry.push(character),
                _ => {
                    entry.push('\\');
                    entry.push(character);
                }
            }
            escaped = false;
        } else {
            match character {
                '\\' => escaped = true,
                ',' => {
                    insert_runner_environment_entry(&mut environment, &entry)?;
                    entry.clear();
                }
                _ => entry.push(character),
            }
        }
    }

    if escaped {
        return Err(DvtError::InvalidXCTestRunnerEnvironment(
            "environment cannot end with an escape character".into(),
        ));
    }

    insert_runner_environment_entry(&mut environment, &entry)?;
    Ok(environment)
}

fn insert_runner_environment_entry(
    environment: &mut Dictionary,
    entry: &str,
) -> Result<(), DvtError> {
    let (key, value) = entry.split_once('=').ok_or_else(|| {
        DvtError::InvalidXCTestRunnerEnvironment(format!("expected KEY=VALUE, got {entry:?}"))
    })?;
    if key.is_empty() {
        return Err(DvtError::InvalidXCTestRunnerEnvironment(
            "environment variable name cannot be empty".into(),
        ));
    }

    environment.insert(key.to_owned(), Value::String(value.to_owned()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_runner_environment;

    #[test]
    fn parses_runner_environment() {
        let environment = parse_runner_environment(
            r"USE_PORT=8200,MJPEG_SERVER_PORT=9200,URL=http://localhost?a=1,EMPTY=",
        )
        .unwrap();

        assert_eq!(
            environment
                .get("USE_PORT")
                .and_then(|value| value.as_string()),
            Some("8200")
        );
        assert_eq!(
            environment
                .get("MJPEG_SERVER_PORT")
                .and_then(|value| value.as_string()),
            Some("9200")
        );
        assert_eq!(
            environment.get("URL").and_then(|value| value.as_string()),
            Some("http://localhost?a=1")
        );
        assert_eq!(
            environment.get("EMPTY").and_then(|value| value.as_string()),
            Some("")
        );
    }

    #[test]
    fn parses_escaped_commas_and_backslashes() {
        let environment = parse_runner_environment(r"LABEL=foo\,bar,PATH=C:\\tmp").unwrap();

        assert_eq!(
            environment.get("LABEL").and_then(|value| value.as_string()),
            Some("foo,bar")
        );
        assert_eq!(
            environment.get("PATH").and_then(|value| value.as_string()),
            Some(r"C:\tmp")
        );
    }

    #[test]
    fn duplicate_environment_variables_use_the_last_value() {
        let environment = parse_runner_environment("LANG=en,LANG=fr").unwrap();

        assert_eq!(
            environment.get("LANG").and_then(|value| value.as_string()),
            Some("fr")
        );
    }

    #[test]
    fn rejects_invalid_runner_environment() {
        for input in ["", "MISSING_VALUE", "=value", "KEY=value,", r"KEY=value\"] {
            assert!(parse_runner_environment(input).is_err(), "{input:?}");
        }
    }
}
