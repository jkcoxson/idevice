//! iOS Mobile Backup 2 Service Client
//!
//! Provides functionality for interacting with the mobilebackup2 service on iOS devices,
//! which allows creating, restoring, and managing device backups.

use log::{debug, warn};
use plist::Dictionary;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use tokio::io::AsyncReadExt;

use crate::{Idevice, IdeviceError, IdeviceService, obf};

/// DeviceLink message codes used in MobileBackup2 binary streams
pub const DL_CODE_SUCCESS: u8 = 0x00;
pub const DL_CODE_ERROR_LOCAL: u8 = 0x06;
pub const DL_CODE_ERROR_REMOTE: u8 = 0x0b;
pub const DL_CODE_FILE_DATA: u8 = 0x0c;

/// Client for interacting with the iOS mobile backup 2 service
///
/// This service provides access to device backup functionality including
/// creating backups, restoring from backups, and managing backup data.
pub struct MobileBackup2Client {
    /// The underlying device connection with established mobilebackup2 service
    pub idevice: Idevice,
    /// Protocol version negotiated with the device
    pub protocol_version: f64,
}

impl IdeviceService for MobileBackup2Client {
    /// Returns the mobile backup 2 service name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.mobilebackup2")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, crate::IdeviceError> {
        let mut client = Self::new(idevice);
        // Perform DeviceLink handshake first
        client.dl_version_exchange().await?;
        // Perform version exchange after connection
        client.version_exchange().await?;
        Ok(client)
    }
}

/// Backup message types used in the mobilebackup2 protocol
#[derive(Debug, Clone)]
pub enum BackupMessageType {
    /// Request to start a backup operation
    BackupMessageTypeBackup,
    /// Request to restore from a backup
    BackupMessageTypeRestore,
    /// Information message
    BackupMessageTypeInfo,
    /// List available backups
    BackupMessageTypeList,
    /// Upload files to backup
    BackupMessageTypeUploadFiles,
    /// Download files from backup
    BackupMessageTypeDownloadFiles,
    /// Clear backup data
    BackupMessageTypeClearBackupData,
    /// Move files in backup
    BackupMessageTypeMoveFiles,
    /// Remove files from backup
    BackupMessageTypeRemoveFiles,
    /// Create directory in backup
    BackupMessageTypeCreateDirectory,
    /// Acquire lock for backup operation
    BackupMessageTypeAcquireLock,
    /// Release lock after backup operation
    BackupMessageTypeReleaseLock,
    /// Copy item in backup
    BackupMessageTypeCopyItem,
    /// Disconnect from service
    BackupMessageTypeDisconnect,
    /// Process message
    BackupMessageTypeProcessMessage,
    /// Get freespace information
    BackupMessageTypeGetFreespace,
    /// Factory info
    BackupMessageTypeFactoryInfo,
    /// Check if backup is encrypted
    BackupMessageTypeCheckBackupEncryption,
}

impl BackupMessageType {
    /// Convert message type to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            // These map to MobileBackup2 request names per libimobiledevice
            BackupMessageType::BackupMessageTypeBackup => "Backup",
            BackupMessageType::BackupMessageTypeRestore => "Restore",
            BackupMessageType::BackupMessageTypeInfo => "Info",
            BackupMessageType::BackupMessageTypeList => "List",
            // The following are DL control messages and not sent via MessageName
            BackupMessageType::BackupMessageTypeUploadFiles => "DLMessageUploadFiles",
            BackupMessageType::BackupMessageTypeDownloadFiles => "DLMessageDownloadFiles",
            BackupMessageType::BackupMessageTypeClearBackupData => "DLMessageClearBackupData",
            BackupMessageType::BackupMessageTypeMoveFiles => "DLMessageMoveFiles",
            BackupMessageType::BackupMessageTypeRemoveFiles => "DLMessageRemoveFiles",
            BackupMessageType::BackupMessageTypeCreateDirectory => "DLMessageCreateDirectory",
            BackupMessageType::BackupMessageTypeAcquireLock => "DLMessageAcquireLock",
            BackupMessageType::BackupMessageTypeReleaseLock => "DLMessageReleaseLock",
            BackupMessageType::BackupMessageTypeCopyItem => "DLMessageCopyItem",
            BackupMessageType::BackupMessageTypeDisconnect => "DLMessageDisconnect",
            BackupMessageType::BackupMessageTypeProcessMessage => "DLMessageProcessMessage",
            BackupMessageType::BackupMessageTypeGetFreespace => "DLMessageGetFreeDiskSpace",
            BackupMessageType::BackupMessageTypeFactoryInfo => "FactoryInfo",
            BackupMessageType::BackupMessageTypeCheckBackupEncryption => "CheckBackupEncryption",
        }
    }
}

/// Backup information structure
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// Backup UUID
    pub uuid: String,
    /// Device name
    pub device_name: String,
    /// Display name
    pub display_name: String,
    /// Last backup date
    pub last_backup_date: Option<String>,
    /// Backup version
    pub version: String,
    /// Whether backup is encrypted
    pub is_encrypted: bool,
}

/// High-level builder for restore options so callers don't need to remember raw keys
#[derive(Debug, Clone)]
pub struct RestoreOptions {
    pub reboot: bool,
    pub copy: bool,
    pub preserve_settings: bool,
    pub system_files: bool,
    pub remove_items_not_restored: bool,
    pub password: Option<String>,
}

impl Default for RestoreOptions {
    fn default() -> Self {
        Self {
            reboot: true,
            copy: true,
            preserve_settings: true,
            system_files: false,
            remove_items_not_restored: false,
            password: None,
        }
    }
}

impl RestoreOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_reboot(mut self, reboot: bool) -> Self {
        self.reboot = reboot;
        self
    }
    pub fn with_copy(mut self, copy: bool) -> Self {
        self.copy = copy;
        self
    }
    pub fn with_preserve_settings(mut self, preserve: bool) -> Self {
        self.preserve_settings = preserve;
        self
    }
    pub fn with_system_files(mut self, system: bool) -> Self {
        self.system_files = system;
        self
    }
    pub fn with_remove_items_not_restored(mut self, remove: bool) -> Self {
        self.remove_items_not_restored = remove;
        self
    }
    pub fn with_password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    pub fn to_plist(&self) -> Dictionary {
        crate::plist!(dict {
            "RestoreShouldReboot": self.reboot,
            "RestoreDontCopyBackup": !self.copy,
            "RestorePreserveSettings": self.preserve_settings,
            "RestoreSystemFiles": self.system_files,
            "RemoveItemsNotRestored": self.remove_items_not_restored,
            "Password":? self.password.clone()
        })
    }
}

impl MobileBackup2Client {
    /// Creates a new mobile backup 2 client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self {
            idevice,
            protocol_version: 0.0,
        }
    }

    /// Performs DeviceLink version exchange handshake
    ///
    /// Sequence:
    /// 1) Receive ["DLMessageVersionExchange", major, minor]
    /// 2) Send   ["DLMessageVersionExchange", "DLVersionsOk", 400]
    /// 3) Receive ["DLMessageDeviceReady"]
    async fn dl_version_exchange(&mut self) -> Result<(), IdeviceError> {
        debug!("Starting DeviceLink version exchange");
        // 1) Receive DLMessageVersionExchange
        let (msg, _arr) = self.receive_dl_message().await?;
        if msg != "DLMessageVersionExchange" {
            warn!("Expected DLMessageVersionExchange, got {msg}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        // 2) Send DLVersionsOk with version 400
        let out = vec![
            plist::Value::String("DLMessageVersionExchange".into()),
            plist::Value::String("DLVersionsOk".into()),
            plist::Value::Integer(400u64.into()),
        ];
        self.send_dl_array(out).await?;

        // 3) Receive DLMessageDeviceReady
        let (msg2, _arr2) = self.receive_dl_message().await?;
        if msg2 != "DLMessageDeviceReady" {
            warn!("Expected DLMessageDeviceReady, got {msg2}");
            return Err(IdeviceError::UnexpectedResponse);
        }
        Ok(())
    }

    /// Sends a raw DL array as binary plist
    async fn send_dl_array(&mut self, array: Vec<plist::Value>) -> Result<(), IdeviceError> {
        self.idevice.send_bplist(plist::Value::Array(array)).await
    }

    /// Receives any DL* message and returns (message_tag, full_array_value)
    pub async fn receive_dl_message(&mut self) -> Result<(String, plist::Value), IdeviceError> {
        if let Some(socket) = &mut self.idevice.socket {
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await?;
            let len = u32::from_be_bytes(buf);
            let mut body = vec![0; len as usize];
            socket.read_exact(&mut body).await?;
            let value: plist::Value = plist::from_bytes(&body)?;
            if let plist::Value::Array(arr) = &value
                && let Some(plist::Value::String(tag)) = arr.first()
            {
                return Ok((tag.clone(), value));
            }
            warn!("Invalid DL message format");
            Err(IdeviceError::UnexpectedResponse)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Performs version exchange with the device
    ///
    /// This is required by the mobilebackup2 protocol and must be called
    /// before any other operations.
    ///
    /// # Returns
    /// `Ok(())` on successful version negotiation
    ///
    /// # Errors
    /// Returns `IdeviceError` if version exchange fails
    async fn version_exchange(&mut self) -> Result<(), IdeviceError> {
        debug!("Starting mobilebackup2 version exchange");

        // Send supported protocol versions (matching libimobiledevice)
        let hello_dict = crate::plist!(dict {
            "SupportedProtocolVersions": [
                2.0, 2.1
            ]
        });

        self.send_device_link_message("Hello", Some(hello_dict))
            .await?;

        // Receive response
        let response = self.receive_device_link_message("Response").await?;

        // Check for error
        if let Some(error_code) = response.get("ErrorCode")
            && let Some(code) = error_code.as_unsigned_integer()
            && code != 0
        {
            warn!("Version exchange failed with error code: {code}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        // Get negotiated protocol version
        if let Some(version) = response.get("ProtocolVersion").and_then(|v| v.as_real()) {
            self.protocol_version = version;
            debug!("Negotiated protocol version: {version}");
        } else {
            warn!("No protocol version in response");
            return Err(IdeviceError::UnexpectedResponse);
        }

        Ok(())
    }

    /// Sends a device link message (DLMessageProcessMessage format)
    ///
    /// This follows the device_link_service protocol used by mobilebackup2
    ///
    /// # Arguments
    /// * `message_name` - The message name (e.g., "Hello", "kBackupMessageTypeInfo")
    /// * `options` - Optional dictionary of options for the message
    ///
    /// # Returns
    /// `Ok(())` on successful message send
    ///
    /// # Errors
    /// Returns `IdeviceError` if communication fails
    async fn send_device_link_message(
        &mut self,
        message_name: &str,
        options: Option<Dictionary>,
    ) -> Result<(), IdeviceError> {
        // Create the actual message dictionary
        let message_dict = crate::plist!(dict {
            "MessageName": message_name,
            :<? options,
        });

        debug!("Sending device link message: {message_name}");
        self.idevice
            .send_bplist(crate::plist!(["DLMessageProcessMessage", message_dict]))
            .await
    }

    /// Receives a device link message and validates the message name
    ///
    /// Arguments
    /// * `expected_message` - The expected message name to validate
    ///
    /// # Returns
    /// The message dictionary on success
    ///
    /// # Errors
    /// Returns `IdeviceError` if communication fails or message name doesn't match
    async fn receive_device_link_message(
        &mut self,
        expected_message: &str,
    ) -> Result<Dictionary, IdeviceError> {
        // Read raw bytes and parse as plist::Value to handle array format
        if let Some(socket) = &mut self.idevice.socket {
            debug!("Reading response size");
            let mut buf = [0u8; 4];
            socket.read_exact(&mut buf).await?;
            let len = u32::from_be_bytes(buf);
            let mut buf = vec![0; len as usize];
            socket.read_exact(&mut buf).await?;
            let response_value: plist::Value = plist::from_bytes(&buf)?;

            // Parse DLMessageProcessMessage format
            if let plist::Value::Array(array) = response_value
                && array.len() >= 2
                && let Some(plist::Value::String(dl_message)) = array.first()
                && let Some(plist::Value::Dictionary(dict)) = array.get(1)
                && dl_message == "DLMessageProcessMessage"
            {
                // Check MessageName if expected
                if !expected_message.is_empty() {
                    if let Some(message_name) = dict.get("MessageName").and_then(|v| v.as_string())
                    {
                        if message_name != expected_message {
                            warn!("Expected message '{expected_message}', got '{message_name}'");
                            return Err(IdeviceError::UnexpectedResponse);
                        }
                    } else {
                        warn!("No MessageName in response");
                        return Err(IdeviceError::UnexpectedResponse);
                    }
                }
                return Ok(dict.clone());
            }

            warn!("Invalid device link message format");
            Err(IdeviceError::UnexpectedResponse)
        } else {
            Err(IdeviceError::NoEstablishedConnection)
        }
    }

    /// Sends a backup message to the device
    ///
    /// # Arguments
    /// * `message_type` - The type of backup message to send
    /// * `options` - Optional dictionary of options for the message
    ///
    /// # Returns
    /// `Ok(())` on successful message send
    ///
    /// # Errors
    /// Returns `IdeviceError` if communication fails
    async fn send_backup_message(
        &mut self,
        message_type: BackupMessageType,
        options: Option<Dictionary>,
    ) -> Result<(), IdeviceError> {
        self.send_device_link_message(message_type.as_str(), options)
            .await
    }

    /// Sends a MobileBackup2 request with proper envelope and identifiers
    pub async fn send_request(
        &mut self,
        request: &str,
        target_identifier: Option<&str>,
        source_identifier: Option<&str>,
        options: Option<Dictionary>,
    ) -> Result<(), IdeviceError> {
        let dict = crate::plist!(dict {
            "TargetIdentifier":? target_identifier,
            "SourceIdentifier":? source_identifier,
            "Options":? options,
            // Special cases like Unback/EnableCloudBackup are handled by caller if needed
        });
        self.send_device_link_message(request, Some(dict)).await
    }

    /// Sends a DLMessageStatusResponse array
    pub async fn send_status_response(
        &mut self,
        status_code: i64,
        status1: Option<&str>,
        status2: Option<plist::Value>,
    ) -> Result<(), IdeviceError> {
        let arr = vec![
            plist::Value::String("DLMessageStatusResponse".into()),
            plist::Value::Integer(status_code.into()),
            plist::Value::String(status1.unwrap_or("___EmptyParameterString___").into()),
            status2.unwrap_or_else(|| plist::Value::String("___EmptyParameterString___".into())),
        ];
        self.send_dl_array(arr).await
    }

    /// Receives a response from the backup service
    ///
    /// # Returns
    /// The response as a plist Dictionary
    ///
    /// # Errors
    /// Returns `IdeviceError` if communication fails or response is malformed
    async fn receive_backup_response(&mut self) -> Result<Dictionary, IdeviceError> {
        self.receive_device_link_message("").await
    }

    /// Requests device information for backup
    ///
    /// # Returns
    /// A dictionary containing device information
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request fails
    pub async fn request_backup_info(&mut self) -> Result<Dictionary, IdeviceError> {
        // Per protocol use MessageName "Info"
        self.send_backup_message(BackupMessageType::BackupMessageTypeInfo, None)
            .await?;

        let response = self.receive_backup_response().await?;

        // Check for error in response
        if let Some(error) = response.get("ErrorCode") {
            warn!("Backup info request failed with error: {error:?}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        Ok(response)
    }

    /// Lists available backups on the device
    ///
    /// # Returns
    /// A vector of backup information
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request fails
    pub async fn list_backups(&mut self) -> Result<Vec<BackupInfo>, IdeviceError> {
        self.send_backup_message(BackupMessageType::BackupMessageTypeList, None)
            .await?;

        let response = self.receive_backup_response().await?;

        // Check for error in response
        if let Some(error) = response.get("ErrorCode") {
            warn!("List backups request failed with error: {error:?}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        let mut backups = Vec::new();

        if let Some(plist::Value::Array(backup_list)) = response.get("BackupList") {
            for backup_item in backup_list {
                if let plist::Value::Dictionary(backup_dict) = backup_item {
                    let uuid = backup_dict
                        .get("BackupUUID")
                        .and_then(|v| v.as_string())
                        .unwrap_or_default()
                        .to_string();

                    let device_name = backup_dict
                        .get("DeviceName")
                        .and_then(|v| v.as_string())
                        .unwrap_or_default()
                        .to_string();

                    let display_name = backup_dict
                        .get("DisplayName")
                        .and_then(|v| v.as_string())
                        .unwrap_or_default()
                        .to_string();

                    let last_backup_date = backup_dict
                        .get("LastBackupDate")
                        .and_then(|v| v.as_string())
                        .map(|s| s.to_string());

                    let version = backup_dict
                        .get("Version")
                        .and_then(|v| v.as_string())
                        .unwrap_or("Unknown")
                        .to_string();

                    let is_encrypted = backup_dict
                        .get("IsEncrypted")
                        .and_then(|v| v.as_boolean())
                        .unwrap_or(false);

                    backups.push(BackupInfo {
                        uuid,
                        device_name,
                        display_name,
                        last_backup_date,
                        version,
                        is_encrypted,
                    });
                }
            }
        }

        Ok(backups)
    }

    /// Starts a backup operation
    ///
    /// # Arguments
    /// * `target_identifier` - Optional target identifier for the backup
    /// * `source_identifier` - Optional source identifier for the backup
    /// * `options` - Optional backup options
    ///
    /// # Returns
    /// `Ok(())` on successful backup start
    ///
    /// # Errors
    /// Returns `IdeviceError` if the backup fails to start
    pub async fn start_backup(
        &mut self,
        target_identifier: Option<&str>,
        source_identifier: Option<&str>,
        options: Option<Dictionary>,
    ) -> Result<(), IdeviceError> {
        self.send_request(
            BackupMessageType::BackupMessageTypeBackup.as_str(),
            target_identifier,
            source_identifier,
            options,
        )
        .await?;

        let response = self.receive_backup_response().await?;

        // Check for error in response
        if let Some(error) = response.get("ErrorCode") {
            warn!("Backup start failed with error: {error:?}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        debug!("Backup started successfully");
        Ok(())
    }

    /// Starts a restore operation
    ///
    /// # Arguments
    /// * `backup_uuid` - UUID of the backup to restore from
    /// * `options` - Optional restore options
    ///
    /// # Returns
    /// `Ok(())` on successful restore start
    ///
    /// # Errors
    /// Returns `IdeviceError` if the restore fails to start
    #[deprecated(
        note = "Use restore_from_path; restore via BackupUUID is not supported by device/mobilebackup2"
    )]
    pub async fn start_restore(
        &mut self,
        _backup_uuid: &str,
        options: Option<Dictionary>,
    ) -> Result<(), IdeviceError> {
        let mut opts = options.unwrap_or_default();
        // Align default restore options with pymobiledevice semantics
        // Caller-specified values (if any) take precedence.
        if !opts.contains_key("RestoreShouldReboot") {
            opts.insert("RestoreShouldReboot".into(), plist::Value::Boolean(true));
        }
        if !opts.contains_key("RestoreDontCopyBackup") {
            // pymobiledevice: copy=True -> RestoreDontCopyBackup=False
            opts.insert("RestoreDontCopyBackup".into(), plist::Value::Boolean(false));
        }
        if !opts.contains_key("RestorePreserveSettings") {
            opts.insert(
                "RestorePreserveSettings".into(),
                plist::Value::Boolean(true),
            );
        }
        if !opts.contains_key("RestoreSystemFiles") {
            opts.insert("RestoreSystemFiles".into(), plist::Value::Boolean(false));
        }
        if !opts.contains_key("RemoveItemsNotRestored") {
            opts.insert(
                "RemoveItemsNotRestored".into(),
                plist::Value::Boolean(false),
            );
        }
        // Avoid borrowing self while sending request
        let target_udid_owned = self.idevice.udid().map(|s| s.to_string());
        let target_udid = target_udid_owned.as_deref();
        self.send_request(
            BackupMessageType::BackupMessageTypeRestore.as_str(),
            // default identifiers to current UDID if available
            target_udid,
            target_udid,
            Some(opts),
        )
        .await?;

        let response = self.receive_backup_response().await?;

        // Check for error in response
        if let Some(error) = response.get("ErrorCode") {
            warn!("Restore start failed with error: {error:?}");
            return Err(IdeviceError::UnexpectedResponse);
        }

        debug!("Restore started successfully");
        Ok(())
    }

    /// High-level API: Restore from a local backup directory using DeviceLink file exchange
    ///
    /// - `backup_root` should point to the backup root directory (which contains the `<SourceIdentifier>` subdirectory)
    /// - If `source_identifier` is None, the current connected device's UDID will be used by default
    /// - `options` should be constructed using the `RestoreOptions` builder; if not provided, defaults will be used
    pub async fn restore_from_path(
        &mut self,
        backup_root: &Path,
        source_identifier: Option<&str>,
        options: Option<RestoreOptions>,
    ) -> Result<(), IdeviceError> {
        // Take owned UDID to avoid aliasing borrows
        let target_udid_owned = self.idevice.udid().map(|s| s.to_string());
        let target_udid = target_udid_owned.as_deref();
        let source: &str = match source_identifier {
            Some(s) => s,
            None => target_udid.ok_or(IdeviceError::InvalidHostID)?,
        };

        // Simple existence check: backup_root/source must exist
        let backup_dir = backup_root.join(source);
        if !backup_dir.exists() {
            return Err(IdeviceError::NotFound);
        }

        let opts = options.unwrap_or_default().to_plist();
        self.send_request(
            BackupMessageType::BackupMessageTypeRestore.as_str(),
            target_udid,
            Some(source),
            Some(opts),
        )
        .await?;

        // Enter the DeviceLink file exchange loop, and pass the root directory to backup_root
        // (the protocol request contains the source prefix)
        let _ = self.process_restore_dl_loop(backup_root).await?;
        Ok(())
    }

    async fn process_restore_dl_loop(
        &mut self,
        host_dir: &Path,
    ) -> Result<Option<Dictionary>, IdeviceError> {
        loop {
            let (tag, value) = self.receive_dl_message().await?;
            match tag.as_str() {
                "DLMessageDownloadFiles" => {
                    self.handle_download_files(&value, host_dir).await?;
                }
                "DLMessageUploadFiles" => {
                    self.handle_upload_files(&value, host_dir).await?;
                }
                "DLMessageGetFreeDiskSpace" => {
                    // Minimal implementation: report 0 with success
                    self.send_status_response(0, None, Some(plist::Value::Integer(0u64.into())))
                        .await?;
                }
                "DLContentsOfDirectory" => {
                    let empty = plist::Value::Dictionary(Dictionary::new());
                    self.send_status_response(0, None, Some(empty)).await?;
                }
                "DLMessageCreateDirectory" => {
                    let status = Self::create_directory_from_message(&value, host_dir);
                    self.send_status_response(status, None, None).await?;
                }
                "DLMessageMoveFiles" | "DLMessageMoveItems" => {
                    let status = Self::move_files_from_message(&value, host_dir);
                    self.send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
                }
                "DLMessageRemoveFiles" | "DLMessageRemoveItems" => {
                    let status = Self::remove_files_from_message(&value, host_dir);
                    self.send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
                }
                "DLMessageCopyItem" => {
                    let status = Self::copy_item_from_message(&value, host_dir);
                    self.send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
                }
                "DLMessageProcessMessage" => {
                    if let plist::Value::Array(arr) = value
                        && let Some(plist::Value::Dictionary(dict)) = arr.get(1)
                    {
                        return Ok(Some(dict.clone()));
                    }
                    return Ok(None);
                }
                "DLMessageDisconnect" => {
                    return Ok(None);
                }
                other => {
                    warn!("Unsupported DL message: {other}");
                    self.send_status_response(-1, Some("Operation not supported"), None)
                        .await?;
                }
            }
        }
    }

    async fn handle_download_files(
        &mut self,
        dl_value: &plist::Value,
        host_dir: &Path,
    ) -> Result<(), IdeviceError> {
        let mut err_any = false;
        if let plist::Value::Array(arr) = dl_value
            && arr.len() >= 2
            && let Some(plist::Value::Array(files)) = arr.get(1)
        {
            for pv in files {
                if let Some(path) = pv.as_string()
                    && let Err(e) = self.send_single_file(host_dir, path).await
                {
                    warn!("Failed to send file {path}: {e}");
                    err_any = true;
                }
            }
        }
        // terminating zero dword
        self.idevice.send_raw(&0u32.to_be_bytes()).await?;
        if err_any {
            self.send_status_response(
                -13,
                Some("Multi status"),
                Some(plist::Value::Dictionary(Dictionary::new())),
            )
            .await
        } else {
            self.send_status_response(0, None, Some(plist::Value::Dictionary(Dictionary::new())))
                .await
        }
    }

    async fn send_single_file(
        &mut self,
        host_dir: &Path,
        rel_path: &str,
    ) -> Result<(), IdeviceError> {
        let full = host_dir.join(rel_path);
        let path_bytes = rel_path.as_bytes().to_vec();
        let nlen = (path_bytes.len() as u32).to_be_bytes();
        self.idevice.send_raw(&nlen).await?;
        self.idevice.send_raw(&path_bytes).await?;

        let mut f = match std::fs::File::open(&full) {
            Ok(f) => f,
            Err(e) => {
                // send error
                let desc = e.to_string();
                let size = (desc.len() as u32 + 1).to_be_bytes();
                let mut hdr = Vec::with_capacity(5);
                hdr.extend_from_slice(&size);
                hdr.push(DL_CODE_ERROR_LOCAL);
                self.idevice.send_raw(&hdr).await?;
                self.idevice.send_raw(desc.as_bytes()).await?;
                return Ok(());
            }
        };
        let mut buf = [0u8; 32768];
        loop {
            let read = f.read(&mut buf).unwrap_or(0);
            if read == 0 {
                break;
            }
            let size = ((read as u32) + 1).to_be_bytes();
            let mut hdr = Vec::with_capacity(5);
            hdr.extend_from_slice(&size);
            hdr.push(DL_CODE_FILE_DATA);
            self.idevice.send_raw(&hdr).await?;
            self.idevice.send_raw(&buf[..read]).await?;
        }
        // success trailer
        let mut ok = [0u8; 5];
        ok[..4].copy_from_slice(&1u32.to_be_bytes());
        ok[4] = DL_CODE_SUCCESS;
        self.idevice.send_raw(&ok).await?;
        Ok(())
    }

    async fn handle_upload_files(
        &mut self,
        _dl_value: &plist::Value,
        host_dir: &Path,
    ) -> Result<(), IdeviceError> {
        loop {
            let dlen = self.read_be_u32().await?;
            if dlen == 0 {
                break;
            }
            let dname = self.read_exact_string(dlen as usize).await?;
            let flen = self.read_be_u32().await?;
            if flen == 0 {
                break;
            }
            let fname = self.read_exact_string(flen as usize).await?;
            let dst = host_dir.join(&fname);
            if let Some(parent) = dst.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let mut file = std::fs::File::create(&dst)
                .map_err(|e| IdeviceError::InternalError(e.to_string()))?;
            loop {
                let nlen = self.read_be_u32().await?;
                if nlen == 0 {
                    break;
                }
                let code = self.read_one().await?;
                if code == DL_CODE_FILE_DATA {
                    let size = (nlen - 1) as usize;
                    let data = self.read_exact(size).await?;
                    file.write_all(&data)
                        .map_err(|e| IdeviceError::InternalError(e.to_string()))?;
                } else {
                    let _ = self.read_exact((nlen - 1) as usize).await?;
                }
            }
            let _ = dname; // unused
        }
        self.send_status_response(0, None, Some(plist::Value::Dictionary(Dictionary::new())))
            .await
    }

    async fn read_be_u32(&mut self) -> Result<u32, IdeviceError> {
        let buf = self.idevice.read_raw(4).await?;
        Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
    }

    async fn read_one(&mut self) -> Result<u8, IdeviceError> {
        let buf = self.idevice.read_raw(1).await?;
        Ok(buf[0])
    }

    async fn read_exact(&mut self, size: usize) -> Result<Vec<u8>, IdeviceError> {
        self.idevice.read_raw(size).await
    }

    async fn read_exact_string(&mut self, size: usize) -> Result<String, IdeviceError> {
        let buf = self.idevice.read_raw(size).await?;
        Ok(String::from_utf8_lossy(&buf).to_string())
    }

    fn create_directory_from_message(dl_value: &plist::Value, host_dir: &Path) -> i64 {
        if let plist::Value::Array(arr) = dl_value
            && arr.len() >= 2
            && let Some(plist::Value::String(dir)) = arr.get(1)
        {
            let path = host_dir.join(dir);
            return match fs::create_dir_all(&path) {
                Ok(_) => 0,
                Err(_) => -1,
            };
        }
        -1
    }

    fn move_files_from_message(dl_value: &plist::Value, host_dir: &Path) -> i64 {
        if let plist::Value::Array(arr) = dl_value
            && arr.len() >= 2
            && let Some(plist::Value::Dictionary(map)) = arr.get(1)
        {
            for (from, to_v) in map.iter() {
                if let Some(to) = to_v.as_string() {
                    let old = host_dir.join(from);
                    let newp = host_dir.join(to);
                    if let Some(parent) = newp.parent() {
                        let _ = fs::create_dir_all(parent);
                    }
                    if fs::rename(&old, &newp).is_err() {
                        return -1;
                    }
                }
            }
            return 0;
        }
        -1
    }

    fn remove_files_from_message(dl_value: &plist::Value, host_dir: &Path) -> i64 {
        if let plist::Value::Array(arr) = dl_value
            && arr.len() >= 2
            && let Some(plist::Value::Array(items)) = arr.get(1)
        {
            for it in items {
                if let Some(p) = it.as_string() {
                    let path = host_dir.join(p);
                    if path.is_dir() {
                        if fs::remove_dir_all(&path).is_err() {
                            return -1;
                        }
                    } else if path.exists() && fs::remove_file(&path).is_err() {
                        return -1;
                    }
                }
            }
            return 0;
        }
        -1
    }

    fn copy_item_from_message(dl_value: &plist::Value, host_dir: &Path) -> i64 {
        if let plist::Value::Array(arr) = dl_value
            && arr.len() >= 3
            && let (Some(plist::Value::String(src)), Some(plist::Value::String(dst))) =
                (arr.get(1), arr.get(2))
        {
            let from = host_dir.join(src);
            let to = host_dir.join(dst);
            if let Some(parent) = to.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if from.is_dir() {
                return match fs::create_dir_all(&to) {
                    Ok(_) => 0,
                    Err(_) => -1,
                };
            } else {
                return match fs::copy(&from, &to) {
                    Ok(_) => 0,
                    Err(_) => -1,
                };
            }
        }
        -1
    }

    /// Starts a restore using the typed RestoreOptions builder
    #[deprecated(
        note = "Use restore_from_path; restore via BackupUUID is not supported by device/mobilebackup2"
    )]
    pub async fn start_restore_with(
        &mut self,
        _backup_uuid: &str,
        opts: RestoreOptions,
    ) -> Result<(), IdeviceError> {
        let dict = opts.to_plist();
        // Avoid borrowing self during request
        let target_udid_owned = self.idevice.udid().map(|s| s.to_string());
        let target_udid = target_udid_owned.as_deref();
        self.send_request(
            BackupMessageType::BackupMessageTypeRestore.as_str(),
            target_udid,
            target_udid,
            Some(dict),
        )
        .await?;

        let response = self.receive_backup_response().await?;
        if let Some(error) = response.get("ErrorCode") {
            warn!("Restore start failed with error: {error:?}");
            return Err(IdeviceError::UnexpectedResponse);
        }
        debug!("Restore started successfully");
        Ok(())
    }

    /// Assert backup dir structure exists for a given source identifier (UDID)
    fn assert_backup_exists(&self, backup_root: &Path, source: &str) -> Result<(), IdeviceError> {
        let device_dir = backup_root.join(source);
        if device_dir.join("Info.plist").exists()
            && device_dir.join("Manifest.plist").exists()
            && device_dir.join("Status.plist").exists()
        {
            Ok(())
        } else {
            Err(IdeviceError::NotFound)
        }
    }

    /// Get backup information using DeviceLink against a given backup root/source
    pub async fn info_from_path(
        &mut self,
        backup_root: &Path,
        source_identifier: Option<&str>,
    ) -> Result<Dictionary, IdeviceError> {
        let target_udid = self.idevice.udid();
        let source = source_identifier
            .or(target_udid)
            .ok_or(IdeviceError::InvalidHostID)?;
        self.assert_backup_exists(backup_root, source)?;

        let dict = crate::plist!(dict {
            "TargetIdentifier": target_udid.unwrap(),
            "SourceIdentifier":? source_identifier,
        });
        self.send_device_link_message("Info", Some(dict)).await?;

        match self.process_restore_dl_loop(backup_root).await? {
            Some(res) => Ok(res),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// List last backup contents (returns raw response dictionary)
    pub async fn list_from_path(
        &mut self,
        backup_root: &Path,
        source_identifier: Option<&str>,
    ) -> Result<Dictionary, IdeviceError> {
        let target_udid = self.idevice.udid();
        let source = source_identifier
            .or(target_udid)
            .ok_or(IdeviceError::InvalidHostID)?;
        self.assert_backup_exists(backup_root, source)?;

        let dict = crate::plist!(dict {
            "MessageName": "List",
            "TargetIdentifier": target_udid.unwrap(),
            "SourceIdentifier": source,
        });
        self.send_device_link_message("List", Some(dict)).await?;

        match self.process_restore_dl_loop(backup_root).await? {
            Some(res) => Ok(res),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }

    /// Unpack a complete backup to device hierarchy
    pub async fn unback_from_path(
        &mut self,
        backup_root: &Path,
        password: Option<&str>,
        source_identifier: Option<&str>,
    ) -> Result<(), IdeviceError> {
        let target_udid = self.idevice.udid();
        let source = source_identifier
            .or(target_udid)
            .ok_or(IdeviceError::InvalidHostID)?;
        self.assert_backup_exists(backup_root, source)?;
        let dict = crate::plist!(dict {
            "TargetIdentifier": target_udid.unwrap(),
            "MessageName": "Unback",
            "SourceIdentifier": source,
            "Password":? password
        });
        self.send_device_link_message("Unback", Some(dict)).await?;
        let _ = self.process_restore_dl_loop(backup_root).await?;
        Ok(())
    }

    /// Extract a single file from a previous backup
    pub async fn extract_from_path(
        &mut self,
        domain_name: &str,
        relative_path: &str,
        backup_root: &Path,
        password: Option<&str>,
        source_identifier: Option<&str>,
    ) -> Result<(), IdeviceError> {
        let target_udid = self.idevice.udid();
        let source = source_identifier
            .or(target_udid)
            .ok_or(IdeviceError::InvalidHostID)?;
        self.assert_backup_exists(backup_root, source)?;
        let dict = crate::plist!(dict {
            "MessageName": "Extract",
            "TargetIdentifier": target_udid.unwrap(),
            "DomainName": domain_name,
            "RelativePath": relative_path,
            "SourceIdentifier": source,
            "Password":? password,
        });
        self.send_device_link_message("Extract", Some(dict)).await?;
        let _ = self.process_restore_dl_loop(backup_root).await?;
        Ok(())
    }

    /// Change backup password (enable/disable if new/old missing)
    pub async fn change_password_from_path(
        &mut self,
        backup_root: &Path,
        old: Option<&str>,
        new: Option<&str>,
    ) -> Result<(), IdeviceError> {
        let target_udid = self.idevice.udid();
        let dict = crate::plist!(dict {
            "MessageName": "ChangePassword",
            "TargetIdentifier": target_udid.ok_or(IdeviceError::InvalidHostID)?,
            "OldPassword":? old,
            "NewPassword":? new
        });
        self.send_device_link_message("ChangePassword", Some(dict))
            .await?;
        let _ = self.process_restore_dl_loop(backup_root).await?;
        Ok(())
    }

    /// Erase device via mobilebackup2
    pub async fn erase_device_from_path(&mut self, backup_root: &Path) -> Result<(), IdeviceError> {
        let target_udid = self.idevice.udid();
        let dict = crate::plist!(dict {
            "MessageName": "EraseDevice",
            "TargetIdentifier": target_udid.ok_or(IdeviceError::InvalidHostID)?
        });
        self.send_device_link_message("EraseDevice", Some(dict))
            .await?;
        let _ = self.process_restore_dl_loop(backup_root).await?;
        Ok(())
    }

    /// Gets free space information from the device
    ///
    /// # Returns
    /// Free space in bytes
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request fails
    pub async fn get_freespace(&mut self) -> Result<u64, IdeviceError> {
        // Not a valid host-initiated request in protocol; device asks via DLMessageGetFreeDiskSpace
        Err(IdeviceError::UnexpectedResponse)
    }

    /// Checks if backup encryption is enabled
    ///
    /// # Returns
    /// `true` if backup encryption is enabled, `false` otherwise
    ///
    /// # Errors
    /// Returns `IdeviceError` if the request fails
    pub async fn check_backup_encryption(&mut self) -> Result<bool, IdeviceError> {
        // Not part of host-initiated MB2 protocol; caller should inspect Manifest/lockdown
        Err(IdeviceError::UnexpectedResponse)
    }

    /// Disconnects from the backup service
    ///
    /// # Returns
    /// `Ok(())` on successful disconnection
    ///
    /// # Errors
    /// Returns `IdeviceError` if disconnection fails
    pub async fn disconnect(&mut self) -> Result<(), IdeviceError> {
        // Send DLMessageDisconnect array per DeviceLink protocol
        let arr = crate::plist!(array [
            "DLMessageDisconnect",
            "___EmptyParameterString___"
        ]);
        self.send_dl_array(arr).await?;
        debug!("Disconnected from backup service");
        Ok(())
    }
}
