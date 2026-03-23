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

use plist::{Dictionary, Value};
use tracing::warn;

use crate::{IdeviceError, services::installation_proxy::InstallationProxyClient};
use types::{XCTCapabilities, XCTestConfiguration};

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
