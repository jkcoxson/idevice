// Jackson Coxson
// Mobile Backup 2 tool for iOS devices

use idevice::{
    IdeviceError, IdeviceService,
    mobilebackup2::{
        BackupDelegate, DirEntryInfo, FsBackupDelegate, MobileBackup2Client, RestoreOptions,
    },
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use std::future::Future;
use std::io::{Read, Write};
use std::path::Path;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Instant;

/// CLI backup delegate: delegates filesystem ops to [`FsBackupDelegate`],
/// adds real disk-space reporting, progress bar, and ETA.
struct CliBackupDelegate {
    fs: FsBackupDelegate,
    start_time: Mutex<Option<Instant>>,
}

impl CliBackupDelegate {
    fn new() -> Self {
        Self {
            fs: FsBackupDelegate,
            start_time: Mutex::new(None),
        }
    }

    fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = 1024 * KB;
        const GB: u64 = 1024 * MB;
        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.1} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.0} KB", bytes as f64 / KB as f64)
        } else {
            format!("{bytes} B")
        }
    }

    fn format_duration(secs: u64) -> String {
        if secs >= 3600 {
            format!(
                "{}h {:02}m {:02}s",
                secs / 3600,
                (secs % 3600) / 60,
                secs % 60
            )
        } else if secs >= 60 {
            format!("{}m {:02}s", secs / 60, secs % 60)
        } else {
            format!("{secs}s")
        }
    }
}

impl BackupDelegate for CliBackupDelegate {
    #[allow(clippy::unnecessary_cast)]
    fn get_free_disk_space(&self, path: &Path) -> u64 {
        #[cfg(unix)]
        {
            use std::ffi::CString;
            use std::mem::MaybeUninit;
            let c_path = match CString::new(path.to_string_lossy().as_bytes()) {
                Ok(p) => p,
                Err(_) => return 0,
            };
            unsafe {
                let mut stat = MaybeUninit::<libc::statvfs>::uninit();
                if libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) == 0 {
                    let stat = stat.assume_init();
                    (stat.f_bavail as u64) * (stat.f_frsize as u64)
                } else {
                    0
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            0
        }
    }

    fn on_file_received(&self, _path: &str, _file_count: u32) {}

    fn on_progress(&self, bytes_done: u64, bytes_total: u64, overall_progress: f64) {
        // Initialize start time on first call
        {
            let mut start = self.start_time.lock().unwrap();
            if start.is_none() {
                *start = Some(Instant::now());
            }
        }

        let elapsed = self.start_time.lock().unwrap().unwrap().elapsed().as_secs();

        // Use byte-level progress if we have a total, otherwise fall back to overall %
        let (progress_str, eta_str) = if bytes_total > 0 {
            let pct = (bytes_done as f64 / bytes_total as f64) * 100.0;
            let eta = if pct > 0.0 && elapsed > 0 {
                let total_secs = (elapsed as f64 / (pct / 100.0)) as u64;
                let remaining = total_secs.saturating_sub(elapsed);
                format!(" ETA: {}", Self::format_duration(remaining))
            } else {
                String::new()
            };
            (
                format!(
                    "{:.1}%  {}/{}",
                    pct,
                    Self::format_size(bytes_done),
                    Self::format_size(bytes_total),
                ),
                eta,
            )
        } else if overall_progress > 0.0 {
            let eta = if overall_progress > 0.0 && elapsed > 0 {
                let total_secs = (elapsed as f64 / (overall_progress / 100.0)) as u64;
                let remaining = total_secs.saturating_sub(elapsed);
                format!(" ETA: {}", Self::format_duration(remaining))
            } else {
                String::new()
            };
            (
                format!(
                    "{:.1}%  {}",
                    overall_progress,
                    Self::format_size(bytes_done),
                ),
                eta,
            )
        } else {
            (Self::format_size(bytes_done), String::new())
        };

        eprint!("\r\x1b[2K  {progress_str}{eta_str}");
    }

    fn open_file_read<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Read + Send>, IdeviceError>> + Send + 'a>> {
        self.fs.open_file_read(path)
    }
    fn create_file_write<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn Write + Send>, IdeviceError>> + Send + 'a>>
    {
        self.fs.create_file_write(path)
    }
    fn create_dir_all<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        self.fs.create_dir_all(path)
    }
    fn remove<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        self.fs.remove(path)
    }
    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        self.fs.rename(from, to)
    }
    fn copy<'a>(
        &'a self,
        src: &'a Path,
        dst: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), IdeviceError>> + Send + 'a>> {
        self.fs.copy(src, dst)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.fs.exists(path)
    }
    fn is_dir<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.fs.is_dir(path)
    }
    fn list_dir<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<DirEntryInfo>, IdeviceError>> + Send + 'a>> {
        self.fs.list_dir(path)
    }
}

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Mobile Backup 2 tool for iOS devices")
        .with_subcommand(
            "info",
            JkCommand::new()
                .help("Get backup information from a local backup directory")
                .with_argument(
                    JkArgument::new()
                        .with_help("Backup DIR to read from")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Source identifier (defaults to current UDID)")
                        .required(true),
                ),
        )
        .with_subcommand(
            "list",
            JkCommand::new()
                .help("List files of the last backup from a local backup directory")
                .with_argument(
                    JkArgument::new()
                        .with_help("Backup DIR to read from")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Source identifier (defaults to current UDID)")
                        .required(true),
                ),
        )
        .with_subcommand(
            "backup",
            JkCommand::new()
                .help("Start a backup operation")
                .with_argument(
                    JkArgument::new()
                        .with_help("Backup directory on host")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Target identifier for the backup")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Source identifier for the backup")
                        .required(true),
                ),
        )
        .with_subcommand(
            "restore",
            JkCommand::new()
                .help("Restore from a local backup directory (DeviceLink)")
                .with_argument(JkArgument::new().with_help("DIR").required(true))
                .with_argument(
                    JkArgument::new()
                        .with_help("Source UDID; defaults to current device UDID")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Backup password if encrypted")
                        .required(true),
                )
                .with_flag(JkFlag::new("no-reboot"))
                .with_flag(JkFlag::new("no-copy"))
                .with_flag(JkFlag::new("no-settings"))
                .with_flag(JkFlag::new("system"))
                .with_flag(JkFlag::new("remove")),
        )
        .with_subcommand(
            "unback",
            JkCommand::new()
                .help("Unpack a complete backup to device hierarchy (broken on iOS 10+)")
                .with_argument(JkArgument::new().with_help("DIR").required(true))
                .with_argument(JkArgument::new().with_help("Source"))
                .with_argument(JkArgument::new().with_help("Password")),
        )
        .with_subcommand(
            "extract",
            JkCommand::new()
                .help("Extract a file from a previous backup")
                .with_argument(JkArgument::new().with_help("DIR").required(true))
                .with_argument(JkArgument::new().with_help("Source").required(true))
                .with_argument(JkArgument::new().with_help("Domain").required(true))
                .with_argument(JkArgument::new().with_help("Path").required(true))
                .with_argument(JkArgument::new().with_help("Password").required(true)),
        )
        .with_subcommand(
            "change-password",
            JkCommand::new()
                .help("Change backup password")
                .with_argument(JkArgument::new().with_help("DIR").required(true))
                .with_argument(JkArgument::new().with_help("Old password").required(true))
                .with_argument(JkArgument::new().with_help("New password").required(true)),
        )
        .with_subcommand(
            "erase-device",
            JkCommand::new()
                .help("Erase the device via mobilebackup2")
                .with_argument(JkArgument::new().with_help("DIR").required(true)),
        )
        .with_subcommand(
            "freespace",
            JkCommand::new().help("Get free space information"),
        )
        .with_subcommand(
            "encryption",
            JkCommand::new().help("Check backup encryption status"),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut backup_client = match MobileBackup2Client::connect(&*provider).await {
        Ok(client) => client,
        Err(e) => {
            eprintln!("Unable to connect to mobilebackup2 service: {e}");
            return;
        }
    };

    let delegate = CliBackupDelegate::new();
    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "info" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();

            match backup_client
                .info_from_path(Path::new(&dir), source, &delegate)
                .await
            {
                Ok(dict) => {
                    println!("Backup Information:");
                    for (k, v) in dict {
                        println!("  {k}: {v:?}");
                    }
                }
                Err(e) => eprintln!("Failed to get info: {e}"),
            }
        }
        "list" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();

            match backup_client
                .list_from_path(Path::new(&dir), source, &delegate)
                .await
            {
                Ok(dict) => {
                    println!("List Response:");
                    for (k, v) in dict {
                        println!("  {k}: {v:?}");
                    }
                }
                Err(e) => eprintln!("Failed to list: {e}"),
            }
        }
        "backup" => {
            let dir = sub_args.next_argument::<String>().expect("dir is required");
            let _target = sub_args.next_argument::<String>();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();

            println!("Starting backup...");
            match backup_client
                .backup_from_path(Path::new(&dir), source, None, &delegate)
                .await
            {
                Ok(Some(response)) => {
                    eprintln!(); // end progress line
                    if let Some(code) = response
                        .get("ErrorCode")
                        .and_then(|v| v.as_unsigned_integer())
                    {
                        if code != 0 {
                            let desc = response
                                .get("ErrorDescription")
                                .and_then(|v| v.as_string())
                                .unwrap_or("Unknown error");
                            eprintln!("Backup failed: ErrorCode {code}: {desc}");
                        } else {
                            println!("Backup Successful.");
                        }
                    } else {
                        println!("Backup Successful.");
                    }
                }
                Ok(None) => {
                    eprintln!(); // end progress line
                    println!("Backup finished.");
                }
                Err(e) => {
                    eprintln!(); // end progress line
                    eprintln!("Backup failed: {e}");
                }
            }
        }
        "restore" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();

            let mut ropts = RestoreOptions::new();
            if sub_args.has_flag("no-reboot") {
                ropts = ropts.with_reboot(false);
            }
            if sub_args.has_flag("no-copy") {
                ropts = ropts.with_copy(false);
            }
            if sub_args.has_flag("no-settings") {
                ropts = ropts.with_preserve_settings(false);
            }
            if sub_args.has_flag("system") {
                ropts = ropts.with_system_files(true);
            }
            if sub_args.has_flag("remove") {
                ropts = ropts.with_remove_items_not_restored(true);
            }
            if let Some(pw) = sub_args.next_argument::<String>() {
                ropts = ropts.with_password(pw);
            }
            match backup_client
                .restore_from_path(Path::new(&dir), source, Some(ropts), &delegate)
                .await
            {
                Ok(Some(response)) => {
                    if let Some(code) = response
                        .get("ErrorCode")
                        .and_then(|v| v.as_unsigned_integer())
                    {
                        if code != 0 {
                            let desc = response
                                .get("ErrorDescription")
                                .and_then(|v| v.as_string())
                                .unwrap_or("Unknown error");
                            eprintln!("Restore failed: ErrorCode {code}: {desc}");
                        } else {
                            println!("Restore Successful.");
                        }
                    } else {
                        println!("Restore Successful.");
                    }
                }
                Ok(None) => {
                    println!("Restore finished.");
                }
                Err(e) => eprintln!("Restore failed: {e}"),
            }
        }
        "unback" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();
            let password = sub_args.next_argument::<String>();
            let password = password.as_deref();

            match backup_client
                .unback_from_path(Path::new(&dir), password, source, &delegate)
                .await
            {
                Ok(_) => println!("Unback finished"),
                Err(e) => eprintln!("Unback failed: {e}"),
            }
        }
        "extract" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();
            let domain = sub_args.next_argument::<String>().unwrap();
            let rel = sub_args.next_argument::<String>().unwrap();
            let password = sub_args.next_argument::<String>();
            let password = password.as_deref();

            match backup_client
                .extract_from_path(
                    domain.as_str(),
                    rel.as_str(),
                    Path::new(&dir),
                    password,
                    source,
                    &delegate,
                )
                .await
            {
                Ok(_) => println!("Extract finished"),
                Err(e) => eprintln!("Extract failed: {e}"),
            }
        }
        "change-password" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let old = sub_args.next_argument::<String>();
            let old = old.as_deref();
            let newv = sub_args.next_argument::<String>();
            let newv = newv.as_deref();

            match backup_client
                .change_password_from_path(Path::new(&dir), old, newv, &delegate)
                .await
            {
                Ok(_) => println!("Change password finished"),
                Err(e) => eprintln!("Change password failed: {e}"),
            }
        }
        "erase-device" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            match backup_client
                .erase_device_from_path(Path::new(&dir), &delegate)
                .await
            {
                Ok(_) => println!("Erase device command sent"),
                Err(e) => eprintln!("Erase device failed: {e}"),
            }
        }
        "freespace" => match backup_client.get_freespace().await {
            Ok(freespace) => {
                let freespace_gb = freespace as f64 / (1024.0 * 1024.0 * 1024.0);
                println!("Free space: {freespace} bytes ({freespace_gb:.2} GB)");
            }
            Err(e) => eprintln!("Failed to get free space: {e}"),
        },
        "encryption" => match backup_client.check_backup_encryption().await {
            Ok(is_encrypted) => {
                println!(
                    "Backup encryption: {}",
                    if is_encrypted { "Enabled" } else { "Disabled" }
                );
            }
            Err(e) => eprintln!("Failed to check backup encryption: {e}"),
        },
        _ => {
            println!("No subcommand provided. Use --help for available commands.");
        }
    }

    // Disconnect from the service
    if let Err(e) = backup_client.disconnect().await {
        eprintln!("Warning: Failed to disconnect cleanly: {e}");
    }
}
