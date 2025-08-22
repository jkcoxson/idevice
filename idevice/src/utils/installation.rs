//! High-level install/upgrade helpers
//!
//! This module provides convenient wrappers that mirror ideviceinstaller's
//! behavior by uploading a package to `PublicStaging` via AFC and then
//! issuing `Install`/`Upgrade` commands through InstallationProxy.
//!
//! Notes:
//! - The package path used by InstallationProxy must be a path inside the
//!   AFC jail (e.g. `PublicStaging/<name>`)
//! - For `.ipa` files, we upload the whole file to `PublicStaging/<file_name>`
//! - For directories (developer bundles), we recursively mirror the directory
//!   into `PublicStaging/<dir_name>` and pass that directory path.

use std::path::Path;

use crate::{
    IdeviceError, IdeviceService,
    provider::IdeviceProvider,
    services::{
        afc::{AfcClient, opcode::AfcFopenMode},
        installation_proxy::InstallationProxyClient,
    },
};

const PUBLIC_STAGING: &str = "PublicStaging";

/// Result of a prepared upload, containing the remote path to use in Install/Upgrade
struct UploadedPackageInfo {
    /// Path inside the AFC jail for InstallationProxy `PackagePath`
    remote_package_path: String,
}

/// Ensure `PublicStaging` exists on device via AFC
async fn ensure_public_staging(afc: &mut AfcClient) -> Result<(), IdeviceError> {
    // Try to stat and if it fails, create directory
    match afc.get_file_info(PUBLIC_STAGING).await {
        Ok(_) => Ok(()),
        Err(_) => afc.mk_dir(PUBLIC_STAGING).await,
    }
}

/// Upload a single file to a destination path on device using AFC
async fn afc_upload_file(
    afc: &mut AfcClient,
    local_path: &Path,
    remote_path: &str,
) -> Result<(), IdeviceError> {
    let mut fd = afc.open(remote_path, AfcFopenMode::WrOnly).await?;
    let bytes = tokio::fs::read(local_path).await?;
    fd.write(&bytes).await?;
    fd.close().await
}

/// Recursively upload a directory to device via AFC (mirror contents)
async fn afc_upload_dir(
    afc: &mut AfcClient,
    local_dir: &Path,
    remote_dir: &str,
) -> Result<(), IdeviceError> {
    use std::collections::VecDeque;
    afc.mk_dir(remote_dir).await.ok();

    let mut queue: VecDeque<(std::path::PathBuf, String)> = VecDeque::new();
    queue.push_back((local_dir.to_path_buf(), remote_dir.to_string()));

    while let Some((cur_local, cur_remote)) = queue.pop_front() {
        let mut rd = tokio::fs::read_dir(&cur_local).await?;
        while let Some(entry) = rd.next_entry().await? {
            let meta = entry.metadata().await?;
            let name = entry.file_name();
            let name = name.to_string_lossy().into_owned();
            if name == "." || name == ".." {
                continue;
            }
            let child_local = entry.path();
            let child_remote = format!("{}/{}", cur_remote, name);
            if meta.is_dir() {
                afc.mk_dir(&child_remote).await.ok();
                queue.push_back((child_local, child_remote));
            } else if meta.is_file() {
                afc_upload_file(afc, &child_local, &child_remote).await?;
            }
        }
    }
    Ok(())
}

/// Upload a package to `PublicStaging` and return its InstallationProxy path
///
/// - If `local_path` is a file, it will be uploaded to `PublicStaging/<name>`
/// - If it is a directory, it will be mirrored to `PublicStaging/<dir_name>`
async fn upload_package_to_public_staging<P: AsRef<Path>>(
    provider: &dyn IdeviceProvider,
    local_path: P,
) -> Result<UploadedPackageInfo, IdeviceError> {
    // Connect to AFC via the generic service connector
    let mut afc = AfcClient::connect(provider).await?;

    ensure_public_staging(&mut afc).await?;

    let local_path = local_path.as_ref();
    let file_name: String = local_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| IdeviceError::InvalidArgument)?;
    let remote_path = format!("{}/{}", PUBLIC_STAGING, file_name);

    let meta = tokio::fs::metadata(local_path).await?;
    if meta.is_dir() {
        afc_upload_dir(&mut afc, local_path, &remote_path).await?;
    } else {
        afc_upload_file(&mut afc, local_path, &remote_path).await?;
    }

    Ok(UploadedPackageInfo {
        remote_package_path: remote_path,
    })
}

/// Install an application by first uploading the local package and then invoking InstallationProxy.
///
/// - Accepts a local file path or directory path.
/// - `options` is an InstallationProxy ClientOptions dictionary; pass `None` for defaults.
pub async fn install_package<P: AsRef<Path>>(
    provider: &dyn IdeviceProvider,
    local_path: P,
    options: Option<plist::Value>,
) -> Result<(), IdeviceError> {
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_package_to_public_staging(provider, local_path).await?;

    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.install(remote_package_path, options).await
}

/// Upgrade an application by first uploading the local package and then invoking InstallationProxy.
///
/// - Accepts a local file path or directory path.
/// - `options` is an InstallationProxy ClientOptions dictionary; pass `None` for defaults.
pub async fn upgrade_package<P: AsRef<Path>>(
    provider: &dyn IdeviceProvider,
    local_path: P,
    options: Option<plist::Value>,
) -> Result<(), IdeviceError> {
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_package_to_public_staging(provider, local_path).await?;

    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.upgrade(remote_package_path, options).await
}

/// Same as `install_package` but providing a callback that receives `(percent_complete, state)`
/// updates while InstallationProxy performs the operation.
pub async fn install_package_with_callback<P: AsRef<Path>, Fut, S>(
    provider: &dyn IdeviceProvider,
    local_path: P,
    options: Option<plist::Value>,
    callback: impl Fn((u64, S)) -> Fut,
    state: S,
) -> Result<(), IdeviceError>
where
    Fut: std::future::Future<Output = ()>,
    S: Clone,
{
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_package_to_public_staging(provider, local_path).await?;

    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.install_with_callback(remote_package_path, options, callback, state)
        .await
}

/// Same as `upgrade_package` but providing a callback that receives `(percent_complete, state)`
/// updates while InstallationProxy performs the operation.
pub async fn upgrade_package_with_callback<P: AsRef<Path>, Fut, S>(
    provider: &dyn IdeviceProvider,
    local_path: P,
    options: Option<plist::Value>,
    callback: impl Fn((u64, S)) -> Fut,
    state: S,
) -> Result<(), IdeviceError>
where
    Fut: std::future::Future<Output = ()>,
    S: Clone,
{
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_package_to_public_staging(provider, local_path).await?;

    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.upgrade_with_callback(remote_package_path, options, callback, state)
        .await
}

/// Upload raw bytes to `PublicStaging/<remote_name>` via AFC and return the remote package path.
///
/// - This is useful when the package is not present on disk or is generated in-memory.
async fn upload_bytes_to_public_staging(
    provider: &dyn IdeviceProvider,
    data: impl AsRef<[u8]>,
    remote_name: &str,
) -> Result<UploadedPackageInfo, IdeviceError> {
    // Connect to AFC
    let mut afc = AfcClient::connect(provider).await?;
    ensure_public_staging(&mut afc).await?;

    let remote_path = format!("{}/{}", PUBLIC_STAGING, remote_name);
    let mut fd = afc.open(&remote_path, AfcFopenMode::WrOnly).await?;
    fd.write(data.as_ref()).await?;
    fd.close().await?;

    Ok(UploadedPackageInfo {
        remote_package_path: remote_path,
    })
}

/// Install an application from raw bytes by first uploading them to `PublicStaging` and then
/// invoking InstallationProxy `Install`.
///
/// - `remote_name` determines the remote filename under `PublicStaging`.
/// - `options` is an InstallationProxy ClientOptions dictionary; pass `None` for defaults.
pub async fn install_bytes(
    provider: &dyn IdeviceProvider,
    data: impl AsRef<[u8]>,
    remote_name: &str,
    options: Option<plist::Value>,
) -> Result<(), IdeviceError> {
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_bytes_to_public_staging(provider, data, remote_name).await?;
    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.install(remote_package_path, options).await
}

/// Same as `install_bytes` but providing a callback that receives `(percent_complete, state)`
/// updates while InstallationProxy performs the install operation.
///
/// Tip:
/// - When embedding assets into the binary, you can pass `include_bytes!("path/to/app.ipa")`
///   as the `data` argument and choose a desired `remote_name` (e.g. `"MyApp.ipa"`).
pub async fn install_bytes_with_callback<Fut, S>(
    provider: &dyn IdeviceProvider,
    data: impl AsRef<[u8]>,
    remote_name: &str,
    options: Option<plist::Value>,
    callback: impl Fn((u64, S)) -> Fut,
    state: S,
) -> Result<(), IdeviceError>
where
    Fut: std::future::Future<Output = ()>,
    S: Clone,
{
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_bytes_to_public_staging(provider, data, remote_name).await?;
    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.install_with_callback(remote_package_path, options, callback, state)
        .await
}

/// Upgrade an application from raw bytes by first uploading them to `PublicStaging` and then
/// invoking InstallationProxy `Upgrade`.
///
/// - `remote_name` determines the remote filename under `PublicStaging`.
/// - `options` is an InstallationProxy ClientOptions dictionary; pass `None` for defaults.
pub async fn upgrade_bytes(
    provider: &dyn IdeviceProvider,
    data: impl AsRef<[u8]>,
    remote_name: &str,
    options: Option<plist::Value>,
) -> Result<(), IdeviceError> {
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_bytes_to_public_staging(provider, data, remote_name).await?;
    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.upgrade(remote_package_path, options).await
}

/// Same as `upgrade_bytes` but providing a callback that receives `(percent_complete, state)`
/// updates while InstallationProxy performs the upgrade operation.
///
/// Tip:
/// - When embedding assets into the binary, you can pass `include_bytes!("path/to/app.ipa")`
///   as the `data` argument and choose a desired `remote_name` (e.g. `"MyApp.ipa"`).
pub async fn upgrade_bytes_with_callback<Fut, S>(
    provider: &dyn IdeviceProvider,
    data: impl AsRef<[u8]>,
    remote_name: &str,
    options: Option<plist::Value>,
    callback: impl Fn((u64, S)) -> Fut,
    state: S,
) -> Result<(), IdeviceError>
where
    Fut: std::future::Future<Output = ()>,
    S: Clone,
{
    let UploadedPackageInfo {
        remote_package_path,
    } = upload_bytes_to_public_staging(provider, data, remote_name).await?;
    let mut inst = InstallationProxyClient::connect(provider).await?;
    inst.upgrade_with_callback(remote_package_path, options, callback, state)
        .await
}
