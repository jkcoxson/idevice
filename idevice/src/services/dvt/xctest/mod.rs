//! XCTest service client for iOS instruments protocol.
//!
//! This module provides orchestration for running XCTest bundles (including
//! WebDriverAgent) on iOS devices through the instruments and testmanagerd
//! protocols. It handles session setup, test runner launch, and lifecycle
//! event dispatch.
//!
//! Supports iOS 11+ via lockdown and iOS 17+ via RSD tunnel.
//!
//! # Example
//! ```rust,no_run
//! # #[cfg(feature = "xctest")]
//! # {
//! use idevice::services::dvt::xctest::{TestConfig, XCUITestService};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), idevice::IdeviceError> {
//!     // provider setup omitted
//!     Ok(())
//! }
//! # }
//! ```
// Jackson Coxson

pub mod dtx_services;
pub mod listener;
pub mod types;

use std::sync::Arc;

use plist::{Dictionary, Value};
use tracing::{debug, warn};

use crate::{
    IdeviceError, IdeviceService, ReadWrite,
    dvt::message::AuxValue,
    provider::IdeviceProvider,
    services::{
        dvt::{
            process_control::ProcessControlClient,
            remote_server::{Channel, RemoteServerClient},
        },
        installation_proxy::InstallationProxyClient,
        lockdown::LockdownClient,
    },
};
use dtx_services::{
    DVT_LEGACY_SERVICE, DVT_SERVICE, IDE_AUTHORIZE_TEST_SESSION,
    IDE_INITIATE_CTRL_SESSION_FOR_PID, IDE_INITIATE_CTRL_SESSION_FOR_PID_PROTOCOL_VERSION,
    IDE_INITIATE_CTRL_SESSION_WITH_CAPABILITIES, IDE_INITIATE_CTRL_SESSION_WITH_PROTOCOL_VERSION,
    IDE_INITIATE_SESSION_WITH_IDENTIFIER_CAPABILITIES,
    IDE_INITIATE_SESSION_WITH_IDENTIFIER_FOR_CLIENT_AT_PATH_PROTOCOL_VERSION,
    IDE_START_EXECUTING_TEST_PLAN, TESTMANAGERD_SECURE_SERVICE, TESTMANAGERD_SERVICE,
    XCTEST_MANAGER_IDE_INTERFACE, XCODE_VERSION, XCT_BUNDLE_READY,
    XCT_BUNDLE_READY_WITH_PROTOCOL_VERSION, XCT_CASE_DID_FAIL, XCT_CASE_DID_FINISH,
    XCT_CASE_DID_FINISH_ACTIVITY, XCT_CASE_DID_FINISH_ACTIVITY_ID, XCT_CASE_DID_FINISH_ID,
    XCT_CASE_DID_RECORD_ISSUE, XCT_CASE_DID_STALL, XCT_CASE_DID_START,
    XCT_CASE_DID_START_ID, XCT_CASE_WILL_START_ACTIVITY, XCT_CASE_WILL_START_ACTIVITY_ID,
    XCT_DID_BEGIN_TEST_PLAN, XCT_DID_BEGIN_UI_INIT, XCT_DID_FAIL_BOOTSTRAP,
    XCT_DID_FINISH_TEST_PLAN, XCT_EXCHANGE_PROTOCOL_VERSION, XCT_LOG_DEBUG_MESSAGE,
    XCT_LOG_MESSAGE, XCT_METHOD_DID_MEASURE_METRIC, XCT_RUNNER_READY_WITH_CAPABILITIES,
    XCT_SUITE_DID_FINISH, XCT_SUITE_DID_FINISH_ID, XCT_SUITE_DID_START,
    XCT_SUITE_DID_START_ID, XCT_UI_INIT_DID_FAIL,
};
use listener::{XCTestCaseResult, XCUITestListener};
use types::{XCActivityRecord, XCTCapabilities, XCTIssue, XCTTestIdentifier, XCTestConfiguration};

// ---------------------------------------------------------------------------
// TestConfig
// ---------------------------------------------------------------------------

/// Launch configuration for the XCTest runner and optional target application.
///
/// Built from `InstallationProxyClient` and used to generate both the
/// on-device `XCTestConfiguration` file and the process-launch environment.
///
/// # Example
/// ```rust,no_run
/// # #[cfg(feature = "xctest")]
/// # async fn example() -> Result<(), idevice::IdeviceError> {
/// // let cfg = TestConfig::from_installation_proxy(&mut proxy, "com.example.App.xctrunner", None).await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "xctest")]
#[derive(Debug, Clone)]
pub struct TestConfig {
    // --- Runner app info (from installation_proxy) -------------------------
    /// Bundle identifier of the runner app (e.g. `"com.example.App.xctrunner"`).
    pub runner_bundle_id: String,
    /// On-device path of the runner app bundle (`"Path"` key).
    pub runner_app_path: String,
    /// On-device container path of the runner app (`"Container"` key).
    pub runner_app_container: String,
    /// Executable name inside the runner bundle (`"CFBundleExecutable"` key).
    /// Must end with `"-Runner"`.
    pub runner_bundle_executable: String,

    // --- Target app (optional) --------------------------------------------
    /// Bundle identifier of the app under test, if any.
    pub target_bundle_id: Option<String>,
    /// On-device path of the target app bundle, if any.
    pub target_app_path: Option<String>,
    /// Extra environment variables forwarded to the target app.
    pub target_app_env: Option<Dictionary>,
    /// Extra launch arguments forwarded to the target app.
    pub target_app_args: Option<Vec<String>>,

    // --- Test filters ------------------------------------------------------
    /// If set, only these test identifiers are run.
    pub tests_to_run: Option<Vec<String>>,
    /// If set, these test identifiers are skipped.
    pub tests_to_skip: Option<Vec<String>>,

    // --- Runner overrides -------------------------------------------------
    /// Additional environment variables merged into the runner launch env.
    pub runner_env: Option<Dictionary>,
    /// Additional arguments appended to the runner launch args.
    pub runner_args: Option<Vec<String>>,
}

#[cfg(feature = "xctest")]
impl TestConfig {
    /// Constructs a `TestConfig` by querying `InstallationProxyClient` for
    /// the runner (and optionally target) application information.
    ///
    /// # Arguments
    /// * `install_proxy` - Connected `InstallationProxyClient`
    /// * `runner_bundle_id` - Bundle identifier of the `.xctrunner` app
    /// * `target_bundle_id` - Optional bundle identifier of the app under test
    ///
    /// # Errors
    /// * `IdeviceError::AppNotInstalled` if runner or target is not found
    /// * `IdeviceError::UnexpectedResponse` if `CFBundleExecutable` does not
    ///   end with `"-Runner"` or required keys are missing
    pub async fn from_installation_proxy(
        install_proxy: &mut InstallationProxyClient,
        runner_bundle_id: &str,
        target_bundle_id: Option<&str>,
    ) -> Result<Self, IdeviceError> {
        // Build the bundle ID list to look up in one request
        let mut ids = vec![runner_bundle_id.to_owned()];
        if let Some(t) = target_bundle_id {
            ids.push(t.to_owned());
        }

        let apps = install_proxy
            .get_apps(None, Some(ids))
            .await?;

        // --- Runner ---
        let runner_info = apps.get(runner_bundle_id).ok_or_else(|| {
            warn!("Runner app not installed: {}", runner_bundle_id);
            IdeviceError::AppNotInstalled
        })?;

        let runner_dict = runner_info.as_dictionary().ok_or_else(|| {
            warn!("Runner info is not a dictionary");
            IdeviceError::UnexpectedResponse
        })?;

        let runner_app_path = extract_str(runner_dict, "Path")?;
        let runner_app_container = extract_str(runner_dict, "Container")?;
        let runner_bundle_executable = extract_str(runner_dict, "CFBundleExecutable")?;

        if !runner_bundle_executable.ends_with("-Runner") {
            warn!(
                "CFBundleExecutable '{}' does not end with '-Runner'; this is not a valid xctest runner bundle",
                runner_bundle_executable
            );
            return Err(IdeviceError::UnexpectedResponse);
        }

        // --- Target (optional) ---
        let (target_bundle_id_out, target_app_path) = if let Some(t) = target_bundle_id {
            let target_info = apps.get(t).ok_or_else(|| {
                warn!("Target app not installed: {}", t);
                IdeviceError::AppNotInstalled
            })?;
            let target_dict = target_info.as_dictionary().ok_or_else(|| {
                warn!("Target info is not a dictionary");
                IdeviceError::UnexpectedResponse
            })?;
            let path = extract_str(target_dict, "Path")?;
            (Some(t.to_owned()), Some(path))
        } else {
            (None, None)
        };

        Ok(Self {
            runner_bundle_id: runner_bundle_id.to_owned(),
            runner_app_path,
            runner_app_container,
            runner_bundle_executable,
            target_bundle_id: target_bundle_id_out,
            target_app_path,
            target_app_env: None,
            target_app_args: None,
            tests_to_run: None,
            tests_to_skip: None,
            runner_env: None,
            runner_args: None,
        })
    }

    /// Returns the config name — the executable name with `"-Runner"` stripped.
    ///
    /// For example, `"WebDriverAgentRunner-Runner"` → `"WebDriverAgentRunner"`.
    pub fn config_name(&self) -> &str {
        self.runner_bundle_executable
            .strip_suffix("-Runner")
            .unwrap_or(&self.runner_bundle_executable)
    }

    /// Builds an [`XCTestConfiguration`] for this test run.
    ///
    /// # Arguments
    /// * `session_id` - Unique UUID for this test session
    /// * `ios_major_version` - iOS major version number (e.g. `17`)
    ///
    /// # Errors
    /// Propagates serialisation errors from nested types.
    pub fn build_xctest_configuration(
        &self,
        session_id: uuid::Uuid,
        ios_major_version: u8,
    ) -> Result<XCTestConfiguration, IdeviceError> {
        let config_name = self.config_name();

        let test_bundle_url = format!(
            "file://{}/PlugIns/{}.xctest",
            self.runner_app_path, config_name
        );

        let automation_framework_path = if ios_major_version >= 17 {
            "/System/Developer/Library/PrivateFrameworks/XCTAutomationSupport.framework".to_owned()
        } else {
            "/Developer/Library/PrivateFrameworks/XCTAutomationSupport.framework".to_owned()
        };

        // productModuleName: config_name when a target app is set, else default WDA name
        let product_module_name = if self.target_bundle_id.is_some() {
            config_name.to_owned()
        } else {
            "WebDriverAgentRunner".to_owned()
        };

        // When a target app is specified, targetApplicationEnvironment must be at
        // least an empty dict (not null) — mirrors Python's `self.target_app_env or {}`
        let target_application_environment = if self.target_bundle_id.is_some() {
            Some(
                self.target_app_env
                    .clone()
                    .unwrap_or_default(),
            )
        } else {
            None
        };

        Ok(XCTestConfiguration {
            test_bundle_url,
            session_identifier: session_id,
            product_module_name,
            automation_framework_path,
            target_application_bundle_id: self.target_bundle_id.clone(),
            target_application_path: self.target_app_path.clone(),
            target_application_environment,
            target_application_arguments: self.target_app_args.clone().unwrap_or_default(),
            tests_to_run: self.tests_to_run.clone(),
            tests_to_skip: self.tests_to_skip.clone(),
            ide_capabilities: XCTCapabilities::ide_defaults(),
        })
    }
}

// ---------------------------------------------------------------------------
// build_launch_env
// ---------------------------------------------------------------------------

/// Builds the process-launch arguments, environment, and options for the
/// XCTest runner process.
///
/// # Arguments
/// * `ios_major_version` - iOS major version number
/// * `session_id` - Test session UUID
/// * `runner_app_path` - On-device path of the runner app bundle
/// * `runner_app_container` - On-device container path of the runner app
/// * `target_name` - Config name (executable without `"-Runner"` suffix)
/// * `xctest_config_path` - Device path to the `.xctestconfiguration` file
///   (e.g. `"/tmp/{UUID}.xctestconfiguration"`)
/// * `extra_env` - Additional env vars merged on top of the base set
/// * `extra_args` - Additional args appended after the base set
///
/// # Returns
/// `(launch_args, launch_env, launch_options)` as `(Vec<String>, Dictionary, Dictionary)`
#[cfg(feature = "xctest")]
pub(crate) fn build_launch_env(
    ios_major_version: u8,
    session_id: &uuid::Uuid,
    runner_app_path: &str,
    runner_app_container: &str,
    target_name: &str,
    xctest_config_path: &str,
    extra_env: Option<&Dictionary>,
    extra_args: Option<&[String]>,
) -> (Vec<String>, Dictionary, Dictionary) {
    let session_upper = session_id.to_string().to_uppercase();

    // Base environment
    let mut env = Dictionary::new();
    let s = |v: &str| Value::String(v.to_owned());

    env.insert("CA_ASSERT_MAIN_THREAD_TRANSACTIONS".into(), s("0"));
    env.insert("CA_DEBUG_TRANSACTIONS".into(), s("0"));
    env.insert(
        "DYLD_FRAMEWORK_PATH".into(),
        s(&format!("{}/Frameworks:", runner_app_path)),
    );
    env.insert(
        "DYLD_LIBRARY_PATH".into(),
        s(&format!("{}/Frameworks", runner_app_path)),
    );
    env.insert("MTC_CRASH_ON_REPORT".into(), s("1"));
    env.insert("NSUnbufferedIO".into(), s("YES"));
    env.insert("SQLITE_ENABLE_THREAD_ASSERTIONS".into(), s("1"));
    env.insert("WDA_PRODUCT_BUNDLE_IDENTIFIER".into(), s(""));
    env.insert(
        "XCTestBundlePath".into(),
        s(&format!("{}/PlugIns/{}.xctest", runner_app_path, target_name)),
    );
    env.insert(
        "XCTestConfigurationFilePath".into(),
        s(&format!("{}{}", runner_app_container, xctest_config_path)),
    );
    env.insert(
        "XCODE_DBG_XPC_EXCLUSIONS".into(),
        s("com.apple.dt.xctestSymbolicator"),
    );
    env.insert("XCTestSessionIdentifier".into(), s(&session_upper));

    // iOS >= 11
    if ios_major_version >= 11 {
        env.insert(
            "DYLD_INSERT_LIBRARIES".into(),
            s("/Developer/usr/lib/libMainThreadChecker.dylib"),
        );
        env.insert("OS_ACTIVITY_DT_MODE".into(), s("YES"));
    }

    // iOS >= 17 — extend DYLD paths and clear config path (sent via capabilities)
    if ios_major_version >= 17 {
        let existing_fw = env
            .get("DYLD_FRAMEWORK_PATH")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_owned();
        let existing_lib = env
            .get("DYLD_LIBRARY_PATH")
            .and_then(|v| v.as_string())
            .unwrap_or("")
            .to_owned();
        // Prepend '$' so dyld expands the existing path value at launch time,
        // matching Python: f"${app_env['DYLD_FRAMEWORK_PATH']}/System/..."
        env.insert(
            "DYLD_FRAMEWORK_PATH".into(),
            s(&format!(
                "${}/System/Developer/Library/Frameworks:",
                existing_fw
            )),
        );
        env.insert(
            "DYLD_LIBRARY_PATH".into(),
            s(&format!("${}:/System/Developer/usr/lib", existing_lib)),
        );
        // Config path is sent as return value of _XCT_testRunnerReadyWithCapabilities_
        env.insert("XCTestConfigurationFilePath".into(), s(""));
        env.insert("XCTestManagerVariant".into(), s("DDI"));
    }

    // Merge caller-provided overrides
    if let Some(extra) = extra_env {
        for (k, v) in extra.iter() {
            env.insert(k.clone(), v.clone());
        }
    }

    // Launch arguments
    let mut args = vec![
        "-NSTreatUnknownArgumentsAsOpen".to_owned(),
        "NO".to_owned(),
        "-ApplePersistenceIgnoreState".to_owned(),
        "YES".to_owned(),
    ];
    if let Some(extra) = extra_args {
        args.extend_from_slice(extra);
    }

    // Launch options
    let mut opts = Dictionary::new();
    opts.insert("StartSuspendedKey".into(), Value::Boolean(false));
    if ios_major_version >= 12 {
        opts.insert("ActivateSuspended".into(), Value::Boolean(true));
    }

    (args, env, opts)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Extracts a `String` from `dict[key]`, returning `UnexpectedResponse` on failure.
fn extract_str(dict: &Dictionary, key: &str) -> Result<String, IdeviceError> {
    dict.get(key)
        .and_then(|v| v.as_string())
        .map(|s| s.to_owned())
        .ok_or_else(|| {
            warn!("Missing or non-string key '{}' in app info dict", key);
            IdeviceError::UnexpectedResponse
        })
}

// ---------------------------------------------------------------------------
// TASK 03 — testmanagerd connections
// ---------------------------------------------------------------------------

/// Active DTX connections for running XCTest.
///
/// Holds three `RemoteServerClient` instances:
/// - `ctrl`  — testmanagerd control channel connection
/// - `main`  — testmanagerd main channel connection
/// - `dvt`   — DVT instruments connection (for `ProcessControl`)
#[cfg(feature = "xctest")]
pub(super) struct TestManagerConnections {
    pub ctrl: RemoteServerClient<Box<dyn ReadWrite>>,
    pub main: RemoteServerClient<Box<dyn ReadWrite>>,
    pub dvt: RemoteServerClient<Box<dyn ReadWrite>>,
}

/// Connects to a lockdown-based DTX service, trying each name in order.
///
/// Returns the first successful `RemoteServerClient`.
#[cfg(feature = "xctest")]
async fn connect_dtx_service(
    provider: &dyn IdeviceProvider,
    service_names: &[&str],
    read_greeting: bool,
) -> Result<RemoteServerClient<Box<dyn ReadWrite>>, IdeviceError> {
    let mut lockdown = LockdownClient::connect(provider).await?;
    lockdown
        .start_session(&provider.get_pairing_file().await?)
        .await?;

    let mut last_err: Option<IdeviceError> = None;
    for &name in service_names {
        match lockdown.start_service(name).await {
            Ok((port, ssl)) => {
                let mut idevice = provider.connect(port).await?;
                if ssl {
                    idevice
                        .start_session(&provider.get_pairing_file().await?, false)
                        .await?;
                }
                let socket = idevice
                    .get_socket()
                    .ok_or(IdeviceError::NoEstablishedConnection)?;
                let mut client = RemoteServerClient::new(socket);
                if read_greeting {
                    // testmanagerd sends a capabilities hello on connect; discard it.
                    client.read_message(0).await.ok();
                }
                return Ok(client);
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }
    Err(last_err.unwrap_or(IdeviceError::ServiceNotFound))
}

/// Establishes the three DTX connections required for an XCTest run.
///
/// # Arguments
/// * `provider` - Device connection provider
/// * `ios_major_version` - iOS major version (used to select service names)
#[cfg(feature = "xctest")]
pub(super) async fn connect_testmanagerd(
    provider: &dyn IdeviceProvider,
    ios_major_version: u8,
) -> Result<TestManagerConnections, IdeviceError> {
    let tm_service = if ios_major_version >= 14 {
        TESTMANAGERD_SECURE_SERVICE
    } else {
        TESTMANAGERD_SERVICE
    };

    let ctrl = connect_dtx_service(provider, &[tm_service], true).await?;
    let main = connect_dtx_service(provider, &[tm_service], true).await?;
    let dvt = connect_dtx_service(
        provider,
        &[DVT_SERVICE, DVT_LEGACY_SERVICE],
        false,
    )
    .await?;

    Ok(TestManagerConnections { ctrl, main, dvt })
}

// ---------------------------------------------------------------------------
// TASK 04 — session init + process launch
// ---------------------------------------------------------------------------

/// Initialises the control session on the ctrl DTX channel.
///
/// Sends the appropriate IDE-initiation method based on `ios_major_version`.
#[cfg(feature = "xctest")]
pub(super) async fn init_ctrl_session<R: ReadWrite>(
    ctrl_channel: &mut Channel<'_, R>,
    ios_major_version: u8,
) -> Result<(), IdeviceError> {
    if ios_major_version >= 17 {
        let caps_bytes = AuxValue::archived_value(XCTCapabilities::empty().to_plist_value());
        ctrl_channel
            .call_method(
                Some(IDE_INITIATE_CTRL_SESSION_WITH_CAPABILITIES),
                Some(vec![caps_bytes]),
                true,
            )
            .await?;
        let reply = ctrl_channel.read_message().await?;
        debug!("init_ctrl_session (iOS 17+) reply: {:?}", reply.data);
    } else if ios_major_version >= 11 {
        let version_bytes = AuxValue::archived_value(Value::Integer((XCODE_VERSION as i64).into()));
        ctrl_channel
            .call_method(
                Some(IDE_INITIATE_CTRL_SESSION_WITH_PROTOCOL_VERSION),
                Some(vec![version_bytes]),
                true,
            )
            .await?;
        let reply = ctrl_channel.read_message().await?;
        debug!("init_ctrl_session (iOS 11-16) reply: {:?}", reply.data);
    }
    // iOS < 11: nothing to do
    Ok(())
}

/// Initialises the main test session on the main DTX channel.
#[cfg(feature = "xctest")]
pub(super) async fn init_session<R: ReadWrite>(
    main_channel: &mut Channel<'_, R>,
    ios_major_version: u8,
    session_id: &uuid::Uuid,
    xctest_config: &XCTestConfiguration,
) -> Result<(), IdeviceError> {
    let uuid_bytes = AuxValue::archived_value(Value::Data(session_id.as_bytes().to_vec()));

    if ios_major_version >= 17 {
        let caps_bytes =
            AuxValue::archived_value(XCTCapabilities::ide_defaults().to_plist_value());
        main_channel
            .call_method(
                Some(IDE_INITIATE_SESSION_WITH_IDENTIFIER_CAPABILITIES),
                Some(vec![uuid_bytes, caps_bytes]),
                true,
            )
            .await?;
    } else if ios_major_version >= 11 {
        let client_bytes =
            AuxValue::archived_value(Value::String("not-very-important".into()));
        let path_bytes = AuxValue::archived_value(Value::String(
            "/Applications/Xcode.app/Contents/Developer/usr/bin/xcodebuild".into(),
        ));
        let version_bytes =
            AuxValue::archived_value(Value::Integer((XCODE_VERSION as i64).into()));
        main_channel
            .call_method(
                Some(IDE_INITIATE_SESSION_WITH_IDENTIFIER_FOR_CLIENT_AT_PATH_PROTOCOL_VERSION),
                Some(vec![uuid_bytes, client_bytes, path_bytes, version_bytes]),
                true,
            )
            .await?;
    } else {
        return Ok(());
    }

    let _ = xctest_config; // used by caller for reply in iOS 17+; here we just wait
    let reply = main_channel.read_message().await?;
    debug!("init_session reply: {:?}", reply.data);
    Ok(())
}

/// Launches the XCTest runner process via ProcessControl.
///
/// Uses `launchSuspendedProcessWithDevicePath:bundleIdentifier:environment:arguments:options:`
/// with the provided arguments and options dictionaries.
///
/// # Returns
/// PID of the launched process.
#[cfg(feature = "xctest")]
pub(super) async fn launch_runner<R: ReadWrite>(
    process_control: &mut ProcessControlClient<'_, R>,
    bundle_id: &str,
    launch_args: Vec<String>,
    launch_env: Dictionary,
    launch_options: Dictionary,
) -> Result<u64, IdeviceError> {
    let args_array: Vec<Value> = launch_args
        .into_iter()
        .map(Value::String)
        .collect();

    process_control
        .launch_with_options(bundle_id, launch_env, args_array, launch_options)
        .await
}

// ---------------------------------------------------------------------------
// TASK 05 — authorize + driver channel + start plan
// ---------------------------------------------------------------------------

/// Authorises the test session for the launched runner process.
#[cfg(feature = "xctest")]
pub(super) async fn authorize_test<R: ReadWrite>(
    ctrl_channel: &mut Channel<'_, R>,
    ios_major_version: u8,
    pid: u64,
) -> Result<(), IdeviceError> {
    let pid_bytes = AuxValue::archived_value(Value::Integer((pid as i64).into()));

    if ios_major_version >= 12 {
        ctrl_channel
            .call_method(
                Some(IDE_AUTHORIZE_TEST_SESSION),
                Some(vec![pid_bytes]),
                true,
            )
            .await?;
        let reply = ctrl_channel.read_message().await?;
        match reply.data {
            Some(Value::Boolean(true)) | None => {
                debug!("authorize_test: OK");
            }
            Some(Value::Boolean(false)) => {
                warn!("authorize_test returned false");
                return Err(IdeviceError::UnexpectedResponse);
            }
            other => {
                debug!("authorize_test reply: {:?}", other);
            }
        }
    } else if ios_major_version >= 10 {
        let version_bytes =
            AuxValue::archived_value(Value::Integer((XCODE_VERSION as i64).into()));
        ctrl_channel
            .call_method(
                Some(IDE_INITIATE_CTRL_SESSION_FOR_PID_PROTOCOL_VERSION),
                Some(vec![pid_bytes, version_bytes]),
                true,
            )
            .await?;
        ctrl_channel.read_message().await?;
    } else {
        ctrl_channel
            .call_method(
                Some(IDE_INITIATE_CTRL_SESSION_FOR_PID),
                Some(vec![pid_bytes]),
                true,
            )
            .await?;
        ctrl_channel.read_message().await?;
    }
    Ok(())
}

/// Waits for the test runner to open the reverse `XCTestDriverInterface` channel.
///
/// After launching, the runner sends `_requestChannelWithCode:identifier:` on root
/// channel 0.  This function reads root-channel messages until that request arrives,
/// replies with an empty acknowledgement, registers the channel, and returns a
/// `Channel` handle to it.
#[cfg(feature = "xctest")]
pub(super) async fn wait_for_driver_channel(
    main_client: &mut RemoteServerClient<Box<dyn ReadWrite>>,
    timeout_secs: f64,
) -> Result<Channel<'_, Box<dyn ReadWrite>>, IdeviceError> {
    let deadline = std::time::Instant::now()
        + std::time::Duration::from_secs_f64(timeout_secs);

    loop {
        let remaining = deadline
            .checked_duration_since(std::time::Instant::now())
            .ok_or(IdeviceError::TestRunnerTimeout)?;

        let msg = tokio::time::timeout(remaining, main_client.read_message(0))
            .await
            .map_err(|_| IdeviceError::TestRunnerTimeout)??;

        let method = match &msg.data {
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };

        if method == "_requestChannelWithCode:identifier:" {
            let aux = msg
                .aux
                .as_ref()
                .map(|a| a.values.as_slice())
                .unwrap_or(&[]);
            if aux.len() < 2 {
                warn!("_requestChannelWithCode: not enough aux values");
                continue;
            }
            let channel_code = match &aux[0] {
                AuxValue::U32(v) => *v,
                _ => {
                    warn!("_requestChannelWithCode: aux[0] is not U32");
                    continue;
                }
            };
            let identifier = match aux_as_string(&aux[1]) {
                Ok(s) => s,
                Err(_) => {
                    warn!("_requestChannelWithCode: failed to decode identifier");
                    continue;
                }
            };

            if identifier == "XCTestDriverInterface" {
                debug!("Runner opened XCTestDriverInterface on channel {}", channel_code);
                // Reply with empty acknowledgement
                main_client
                    .send_raw_reply(0, msg.message_header.identifier(), &[])
                    .await?;
                // Register the channel and return a handle
                return Ok(main_client.accept_channel(channel_code));
            }
            // Non-driver channel request — acknowledge and continue
            main_client
                .send_raw_reply(0, msg.message_header.identifier(), &[])
                .await?;
        }
    }
}

/// Signals the test runner to begin executing the test plan.
#[cfg(feature = "xctest")]
pub(super) async fn start_executing_test_plan<R: ReadWrite>(
    driver_channel: &mut Channel<'_, R>,
) -> Result<(), IdeviceError> {
    let version_bytes = AuxValue::archived_value(Value::Integer((XCODE_VERSION as i64).into()));
    driver_channel
        .call_method(
            Some(IDE_START_EXECUTING_TEST_PLAN),
            Some(vec![version_bytes]),
            false,
        )
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// TASK 06 — _XCT_* dispatch + run_dispatch_loop + XCUITestService
// ---------------------------------------------------------------------------

// --- Aux-value helpers ------------------------------------------------------

fn decode_aux_archive(aux: &AuxValue) -> Result<Value, IdeviceError> {
    match aux {
        AuxValue::Array(bytes) => ns_keyed_archive::decode::from_bytes(bytes)
            .map_err(|_| IdeviceError::UnexpectedResponse),
        _ => Err(IdeviceError::UnexpectedResponse),
    }
}

fn aux_as_string(aux: &AuxValue) -> Result<String, IdeviceError> {
    match aux {
        AuxValue::String(s) => return Ok(s.clone()),
        _ => {}
    }
    match decode_aux_archive(aux)? {
        Value::String(s) => Ok(s),
        _ => Err(IdeviceError::UnexpectedResponse),
    }
}

fn aux_as_u64(aux: &AuxValue) -> Result<u64, IdeviceError> {
    match aux {
        AuxValue::U32(v) => return Ok(*v as u64),
        AuxValue::I64(v) => return Ok(*v as u64),
        _ => {}
    }
    match decode_aux_archive(aux)? {
        Value::Integer(i) => i.as_unsigned().ok_or(IdeviceError::UnexpectedResponse),
        _ => Err(IdeviceError::UnexpectedResponse),
    }
}

fn aux_as_f64(aux: &AuxValue) -> Result<f64, IdeviceError> {
    match decode_aux_archive(aux) {
        Ok(Value::Real(f)) => return Ok(f),
        Ok(Value::Integer(i)) => return Ok(i.as_unsigned().unwrap_or(0) as f64),
        _ => {}
    }
    match aux {
        AuxValue::U32(v) => Ok(*v as f64),
        AuxValue::I64(v) => Ok(*v as f64),
        _ => Err(IdeviceError::UnexpectedResponse),
    }
}

// --- Dispatch ---------------------------------------------------------------

/// Dispatches a single incoming `_XCT_*` message to the appropriate listener
/// method.
///
/// Returns `Some(reply_bytes)` if the caller must send a reply (only for
/// `_XCT_testRunnerReadyWithCapabilities_`); `None` otherwise.
#[cfg(feature = "xctest")]
pub(super) async fn dispatch_xct_message<L: XCUITestListener>(
    method: &str,
    aux: &[AuxValue],
    xctest_config: &XCTestConfiguration,
    listener: &mut L,
    done_flag: &mut bool,
) -> Result<Option<Vec<u8>>, IdeviceError> {
    match method {
        // --- logging ---
        m if m == XCT_LOG_DEBUG_MESSAGE => {
            if let Some(msg) = aux.first().map(aux_as_string).transpose()? {
                listener.log_debug_message(&msg).await?;
            }
        }
        m if m == XCT_LOG_MESSAGE => {
            if let Some(msg) = aux.first().map(aux_as_string).transpose()? {
                listener.log_message(&msg).await?;
            }
        }

        // --- protocol negotiation ---
        m if m == XCT_EXCHANGE_PROTOCOL_VERSION => {
            let current = aux.first().map(aux_as_u64).transpose()?.unwrap_or(0);
            let minimum = aux.get(1).map(aux_as_u64).transpose()?.unwrap_or(0);
            listener.exchange_protocol_version(current, minimum).await?;
        }

        // --- bundle ready ---
        m if m == XCT_BUNDLE_READY => {
            listener.test_bundle_ready().await?;
        }
        m if m == XCT_BUNDLE_READY_WITH_PROTOCOL_VERSION => {
            let proto = aux.first().map(aux_as_u64).transpose()?.unwrap_or(0);
            let min = aux.get(1).map(aux_as_u64).transpose()?.unwrap_or(0);
            listener
                .test_bundle_ready_with_protocol_version(proto, min)
                .await?;
        }
        m if m == XCT_RUNNER_READY_WITH_CAPABILITIES => {
            if let Some(raw) = aux.first() {
                if let Ok(decoded) = decode_aux_archive(raw) {
                    if let Some(caps) = XCTCapabilities::from_plist(&decoded) {
                        debug!("testRunnerReadyWithCapabilities: {:?}", caps.capabilities);
                    }
                }
            }
            let reply = xctest_config.to_archive_bytes()?;
            return Ok(Some(reply));
        }

        // --- test plan lifecycle ---
        m if m == XCT_DID_BEGIN_TEST_PLAN => {
            listener.did_begin_executing_test_plan().await?;
        }
        m if m == XCT_DID_FINISH_TEST_PLAN => {
            *done_flag = true;
            listener.did_finish_executing_test_plan().await?;
        }

        // --- suite lifecycle (legacy string-based) ---
        m if m == XCT_SUITE_DID_START => {
            let suite = aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let started_at = aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            listener.test_suite_did_start(&suite, &started_at).await?;
        }
        m if m == XCT_SUITE_DID_FINISH => {
            let suite = aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let finished_at = aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            let run_count = aux.get(2).map(aux_as_u64).transpose()?.unwrap_or(0);
            let failures = aux.get(3).map(aux_as_u64).transpose()?.unwrap_or(0);
            let unexpected = aux.get(4).map(aux_as_u64).transpose()?.unwrap_or(0);
            let test_dur = aux.get(5).map(aux_as_f64).transpose()?.unwrap_or(0.0);
            let total_dur = aux.get(6).map(aux_as_f64).transpose()?.unwrap_or(0.0);
            listener
                .test_suite_did_finish(
                    &suite, &finished_at, run_count, failures, unexpected,
                    test_dur, total_dur, 0, 0, 0,
                )
                .await?;
        }

        // --- suite lifecycle (identifier-based, iOS 14+) ---
        m if m == XCT_SUITE_DID_START_ID => {
            if let Some(raw) = aux.first() {
                if let Ok(decoded) = decode_aux_archive(raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&decoded) {
                        let tc = id.test_class();
                        if !tc.is_empty() && tc != "All tests" {
                            let started_at =
                                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
                            listener.test_suite_did_start(tc, &started_at).await?;
                        }
                    }
                }
            }
        }
        m if m == XCT_SUITE_DID_FINISH_ID => {
            if let Some(raw) = aux.first() {
                if let Ok(decoded) = decode_aux_archive(raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&decoded) {
                        let tc = id.test_class().to_owned();
                        if !tc.is_empty() && tc != "All tests" {
                            let finished_at =
                                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
                            let run_count =
                                aux.get(2).map(aux_as_u64).transpose()?.unwrap_or(0);
                            let skip_count =
                                aux.get(3).map(aux_as_u64).transpose()?.unwrap_or(0);
                            let fail_count =
                                aux.get(4).map(aux_as_u64).transpose()?.unwrap_or(0);
                            let expected_fail =
                                aux.get(5).map(aux_as_u64).transpose()?.unwrap_or(0);
                            let uncaught =
                                aux.get(6).map(aux_as_u64).transpose()?.unwrap_or(0);
                            let test_dur =
                                aux.get(7).map(aux_as_f64).transpose()?.unwrap_or(0.0);
                            let total_dur =
                                aux.get(8).map(aux_as_f64).transpose()?.unwrap_or(0.0);
                            listener
                                .test_suite_did_finish(
                                    &tc, &finished_at, run_count, fail_count, uncaught,
                                    test_dur, total_dur, skip_count, expected_fail, 0,
                                )
                                .await?;
                        }
                    }
                }
            }
        }

        // --- case lifecycle (legacy) ---
        m if m == XCT_CASE_DID_START => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            listener.test_case_did_start(&test_class, &method_name).await?;
        }
        m if m == XCT_CASE_DID_FINISH => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            let status = aux.get(2).map(aux_as_string).transpose()?.unwrap_or_default();
            let duration = aux.get(3).map(aux_as_f64).transpose()?.unwrap_or(0.0);
            listener
                .test_case_did_finish(XCTestCaseResult {
                    test_class,
                    method: method_name,
                    status,
                    duration,
                })
                .await?;
        }
        m if m == XCT_CASE_DID_FAIL => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            let message = aux.get(2).map(aux_as_string).transpose()?.unwrap_or_default();
            let file = aux.get(3).map(aux_as_string).transpose()?.unwrap_or_default();
            let line = aux.get(4).map(aux_as_u64).transpose()?.unwrap_or(0);
            listener
                .test_case_did_fail(&test_class, &method_name, &message, &file, line)
                .await?;
        }
        m if m == XCT_CASE_DID_STALL => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            let file = aux.get(2).map(aux_as_string).transpose()?.unwrap_or_default();
            let line = aux.get(3).map(aux_as_u64).transpose()?.unwrap_or(0);
            listener
                .test_case_did_stall(&test_class, &method_name, &file, line)
                .await?;
        }

        // --- case lifecycle (identifier-based, iOS 14+) ---
        m if m == XCT_CASE_DID_START_ID => {
            if let Some(raw) = aux.first() {
                if let Ok(decoded) = decode_aux_archive(raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&decoded) {
                        let method_name =
                            id.test_method().unwrap_or("").to_owned();
                        listener
                            .test_case_did_start(id.test_class(), &method_name)
                            .await?;
                    }
                }
            }
        }
        m if m == XCT_CASE_DID_FINISH_ID => {
            if let Some(raw) = aux.first() {
                if let Ok(decoded) = decode_aux_archive(raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&decoded) {
                        let test_class = id.test_class().to_owned();
                        let method_name = id.test_method().unwrap_or("").to_owned();
                        let status =
                            aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
                        let duration = aux.get(2).map(aux_as_f64).transpose()?.unwrap_or(0.0);
                        listener
                            .test_case_did_finish(XCTestCaseResult {
                                test_class,
                                method: method_name,
                                status,
                                duration,
                            })
                            .await?;
                    }
                }
            }
        }
        m if m == XCT_CASE_DID_RECORD_ISSUE => {
            if let (Some(id_raw), Some(issue_raw)) = (aux.first(), aux.get(1)) {
                if let (Ok(id_val), Ok(issue_val)) =
                    (decode_aux_archive(id_raw), decode_aux_archive(issue_raw))
                {
                    if let (Some(id), Some(issue)) = (
                        XCTTestIdentifier::from_plist(&id_val),
                        XCTIssue::from_plist(&issue_val),
                    ) {
                        let test_class = id.test_class().to_owned();
                        let method_name = id.test_method().unwrap_or("").to_owned();
                        let file = issue
                            .source_code_context
                            .as_ref()
                            .and_then(|c| c.location.as_ref())
                            .and_then(|l| l.file_path())
                            .unwrap_or("")
                            .to_owned();
                        let line = issue
                            .source_code_context
                            .as_ref()
                            .and_then(|c| c.location.as_ref())
                            .map(|l| l.line_number)
                            .unwrap_or(0);
                        listener
                            .test_case_did_fail(
                                &test_class,
                                &method_name,
                                &issue.compact_description,
                                &file,
                                line,
                            )
                            .await?;
                    }
                }
            }
        }

        // --- activities (legacy) ---
        m if m == XCT_CASE_WILL_START_ACTIVITY => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            // aux[2] is an XCActivityRecord NSKeyedArchive blob, not a plain string
            let title = aux.get(2)
                .and_then(|a| decode_aux_archive(a).ok())
                .and_then(|v| XCActivityRecord::from_plist(&v))
                .map(|r| r.title)
                .unwrap_or_default();
            listener
                .test_case_will_start_activity(&test_class, &method_name, &title)
                .await?;
        }
        m if m == XCT_CASE_DID_FINISH_ACTIVITY => {
            let test_class =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let method_name =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            // aux[2] is an XCActivityRecord NSKeyedArchive blob, not a plain string
            let title = aux.get(2)
                .and_then(|a| decode_aux_archive(a).ok())
                .and_then(|v| XCActivityRecord::from_plist(&v))
                .map(|r| r.title)
                .unwrap_or_default();
            listener
                .test_case_did_finish_activity(&test_class, &method_name, &title)
                .await?;
        }

        // --- activities (identifier-based) ---
        m if m == XCT_CASE_WILL_START_ACTIVITY_ID => {
            if let Some(id_raw) = aux.first() {
                if let Ok(id_val) = decode_aux_archive(id_raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&id_val) {
                        let method_name = id.test_method().unwrap_or("").to_owned();
                        // aux[1] is an XCActivityRecord NSKeyedArchive blob
                        let title = aux.get(1)
                            .and_then(|a| decode_aux_archive(a).ok())
                            .and_then(|v| XCActivityRecord::from_plist(&v))
                            .map(|r| r.title)
                            .unwrap_or_default();
                        listener
                            .test_case_will_start_activity(id.test_class(), &method_name, &title)
                            .await?;
                    }
                }
            }
        }
        m if m == XCT_CASE_DID_FINISH_ACTIVITY_ID => {
            if let Some(id_raw) = aux.first() {
                if let Ok(id_val) = decode_aux_archive(id_raw) {
                    if let Some(id) = XCTTestIdentifier::from_plist(&id_val) {
                        let method_name = id.test_method().unwrap_or("").to_owned();
                        // aux[1] is an XCActivityRecord NSKeyedArchive blob
                        let title = aux.get(1)
                            .and_then(|a| decode_aux_archive(a).ok())
                            .and_then(|v| XCActivityRecord::from_plist(&v))
                            .map(|r| r.title)
                            .unwrap_or_default();
                        listener
                            .test_case_did_finish_activity(id.test_class(), &method_name, &title)
                            .await?;
                    }
                }
            }
        }

        // --- metrics ---
        // Python selector: _XCT_testMethod:ofClass:didMeasureMetric:file:line:
        // → aux[0]=method, aux[1]=test_class, aux[2]=metric, aux[3]=file, aux[4]=line
        m if m == XCT_METHOD_DID_MEASURE_METRIC => {
            let method_name =
                aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            let test_class =
                aux.get(1).map(aux_as_string).transpose()?.unwrap_or_default();
            let metric = aux.get(2).map(aux_as_string).transpose()?.unwrap_or_default();
            let file = aux.get(3).map(aux_as_string).transpose()?.unwrap_or_default();
            let line = aux.get(4).map(aux_as_u64).transpose()?.unwrap_or(0);
            listener
                .test_method_did_measure_metric(&test_class, &method_name, &metric, &file, line)
                .await?;
        }

        // --- iOS 14+ UI testing ---
        m if m == XCT_DID_BEGIN_UI_INIT => {
            listener.did_begin_initializing_for_ui_testing().await?;
        }
        m if m == XCT_UI_INIT_DID_FAIL => {
            let desc = aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            listener
                .initialization_for_ui_testing_did_fail(&desc)
                .await?;
        }
        m if m == XCT_DID_FAIL_BOOTSTRAP => {
            let desc = aux.first().map(aux_as_string).transpose()?.unwrap_or_default();
            listener.did_fail_to_bootstrap(&desc).await?;
        }

        other => {
            warn!("Unknown _XCT_ method: {}", other);
        }
    }

    Ok(None)
}

/// Main event loop: reads incoming `_XCT_*` messages and dispatches them until
/// `_XCT_didFinishExecutingTestPlan` or `timeout` elapses.
#[cfg(feature = "xctest")]
pub(super) async fn run_dispatch_loop<L: XCUITestListener>(
    main_client: &mut RemoteServerClient<Box<dyn ReadWrite>>,
    channel_code: u32,
    xctest_config: &XCTestConfiguration,
    listener: &mut L,
    timeout: Option<std::time::Duration>,
) -> Result<(), IdeviceError> {
    let deadline = timeout.map(|t| std::time::Instant::now() + t);
    let mut done = false;

    loop {
        let remaining = if let Some(dl) = deadline {
            let r = dl
                .checked_duration_since(std::time::Instant::now())
                .ok_or_else(|| {
                    IdeviceError::XcTestTimeout(timeout.unwrap().as_secs_f64())
                })?;
            Some(r)
        } else {
            None
        };

        let msg = match remaining {
            Some(r) => {
                tokio::time::timeout(r, main_client.read_message(channel_code))
                    .await
                    .map_err(|_| IdeviceError::XcTestTimeout(timeout.unwrap().as_secs_f64()))??
            }
            None => main_client.read_message(channel_code).await?,
        };

        let method = match &msg.data {
            Some(Value::String(s)) => s.clone(),
            None => continue, // heartbeat / empty
            _ => {
                warn!("Non-string message data on XCTest channel");
                continue;
            }
        };

        let aux = msg
            .aux
            .as_ref()
            .map(|a| a.values.as_slice())
            .unwrap_or(&[]);

        let msg_id = msg.message_header.identifier();
        let reply_opt =
            dispatch_xct_message(&method, aux, xctest_config, listener, &mut done).await?;

        if let Some(reply_bytes) = reply_opt {
            main_client
                .send_raw_reply(channel_code, msg_id, &reply_bytes)
                .await?;
        }

        if done {
            return Ok(());
        }
    }
}

// ---------------------------------------------------------------------------
// XCUITestService
// ---------------------------------------------------------------------------

/// High-level service that orchestrates an XCTest (or WDA) run end-to-end.
///
/// # Example
/// ```rust,no_run
/// # #[cfg(feature = "xctest")]
/// # async fn example() -> Result<(), idevice::IdeviceError> {
/// // let svc = XCUITestService::new(provider);
/// // svc.run(cfg, &mut listener, None).await?;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "xctest")]
pub struct XCUITestService {
    provider: Arc<dyn IdeviceProvider>,
}

#[cfg(feature = "xctest")]
impl std::fmt::Debug for XCUITestService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("XCUITestService")
            .field("provider", &"<IdeviceProvider>")
            .finish()
    }
}

#[cfg(feature = "xctest")]
impl XCUITestService {
    /// Creates a new `XCUITestService` backed by the given provider.
    pub fn new(provider: Arc<dyn IdeviceProvider>) -> Self {
        Self { provider }
    }

    /// Runs the XCTest bundle described by `cfg` to completion.
    ///
    /// # Arguments
    /// * `cfg`      - Test configuration (runner/target app info, filters, …)
    /// * `listener` - Receives lifecycle events as the test runs
    /// * `timeout`  - Optional wall-clock timeout from when the test plan starts
    ///
    /// # Errors
    /// * `IdeviceError::XcTestTimeout` if `timeout` elapses
    /// * `IdeviceError::TestRunnerTimeout` if the runner does not open
    ///   `XCTestDriverInterface` within 30 seconds
    pub async fn run<L: XCUITestListener>(
        &self,
        cfg: TestConfig,
        listener: &mut L,
        timeout: Option<std::time::Duration>,
    ) -> Result<(), IdeviceError> {
        // 1. Session UUID + config path
        let session_id = uuid::Uuid::new_v4();
        let xctest_path = format!(
            "/tmp/{}.xctestconfiguration",
            session_id.to_string().to_uppercase()
        );

        // 2. iOS major version (needed for service selection and launch env)
        let ios_major_version: u8 = {
            let mut lockdown = LockdownClient::connect(&*self.provider).await?;
            lockdown
                .start_session(&self.provider.get_pairing_file().await?)
                .await?;
            let ver = lockdown.get_value(Some("ProductVersion"), None).await?;
            ver.as_string()
                .and_then(|s| s.split('.').next())
                .and_then(|s| s.parse().ok())
                .unwrap_or(16u8)
        };

        // 3. Build XCTestConfiguration
        let xctest_config = cfg.build_xctest_configuration(session_id, ios_major_version)?;

        // 4. Connect to testmanagerd (ctrl + main) and DVT
        let mut conns = connect_testmanagerd(&*self.provider, ios_major_version).await?;

        // 5. Open channels, init sessions, launch runner (all in scoped borrows)
        let config_name = cfg.config_name().to_owned();
        let main_channel_code: u32;
        let pid: u64;
        {
            let mut ctrl_ch = conns
                .ctrl
                .make_channel(XCTEST_MANAGER_IDE_INTERFACE)
                .await?;

            main_channel_code = {
                let mut main_ch = conns
                    .main
                    .make_channel(XCTEST_MANAGER_IDE_INTERFACE)
                    .await?;
                init_ctrl_session(&mut ctrl_ch, ios_major_version).await?;
                init_session(
                    &mut main_ch,
                    ios_major_version,
                    &session_id,
                    &xctest_config,
                )
                .await?;
                main_ch.channel_code()
            };

            // Build launch environment from the config
            let (launch_args, launch_env, launch_options) = build_launch_env(
                ios_major_version,
                &session_id,
                &cfg.runner_app_path,
                &cfg.runner_app_container,
                &config_name,
                &xctest_path,
                cfg.runner_env.as_ref(),
                cfg.runner_args.as_deref(),
            );

            pid = {
                let mut pc = ProcessControlClient::new(&mut conns.dvt).await?;
                launch_runner(&mut pc, &cfg.runner_bundle_id, launch_args, launch_env, launch_options).await?
            };
            debug!("Launched test runner pid={}", pid);

            if ios_major_version < 17 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            }

            authorize_test(&mut ctrl_ch, ios_major_version, pid).await?;
            // ctrl_ch and its borrow of conns.ctrl released here
        }

        // 6. Wait for driver channel
        let mut driver_ch = wait_for_driver_channel(&mut conns.main, 30.0).await?;

        // 7. Start test plan
        start_executing_test_plan(&mut driver_ch).await?;
        drop(driver_ch);

        // 8. Dispatch loop
        run_dispatch_loop(
            &mut conns.main,
            main_channel_code,
            &xctest_config,
            listener,
            timeout,
        )
        .await?;

        Ok(())
    }
}
