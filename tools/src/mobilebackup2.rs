// Jackson Coxson
// Mobile Backup 2 tool for iOS devices

use idevice::{
    IdeviceService,
    mobilebackup2::{MobileBackup2Client, RestoreOptions},
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist::Dictionary;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

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
                .help("Unpack a complete backup to device hierarchy")
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

    let (sub_name, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "info" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();

            match backup_client.info_from_path(Path::new(&dir), source).await {
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

            match backup_client.list_from_path(Path::new(&dir), source).await {
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
            let target = sub_args.next_argument::<String>();
            let target = target.as_deref();
            let source = sub_args.next_argument::<String>();
            let source = source.as_deref();
            let dir = sub_args.next_argument::<String>().expect("dir is required");

            println!("Starting backup operation...");
            let res = backup_client
                .send_request("Backup", target, source, None::<Dictionary>)
                .await;
            if let Err(e) = res {
                eprintln!("Failed to send backup request: {e}");
            } else if let Err(e) = process_dl_loop(&mut backup_client, Path::new(&dir)).await {
                eprintln!("Backup failed during DL loop: {e}");
            } else {
                println!("Backup flow finished");
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
                .restore_from_path(Path::new(&dir), source, Some(ropts))
                .await
            {
                Ok(_) => println!("Restore flow finished"),
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
                .unback_from_path(Path::new(&dir), password, source)
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
                .change_password_from_path(Path::new(&dir), old, newv)
                .await
            {
                Ok(_) => println!("Change password finished"),
                Err(e) => eprintln!("Change password failed: {e}"),
            }
        }
        "erase-device" => {
            let dir = sub_args.next_argument::<String>().unwrap();
            match backup_client.erase_device_from_path(Path::new(&dir)).await {
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

use idevice::services::mobilebackup2::{
    DL_CODE_ERROR_LOCAL as CODE_ERROR_LOCAL, DL_CODE_FILE_DATA as CODE_FILE_DATA,
    DL_CODE_SUCCESS as CODE_SUCCESS,
};

async fn process_dl_loop(
    client: &mut MobileBackup2Client,
    host_dir: &Path,
) -> Result<Option<Dictionary>, idevice::IdeviceError> {
    loop {
        let (tag, value) = client.receive_dl_message().await?;
        match tag.as_str() {
            "DLMessageDownloadFiles" => {
                handle_download_files(client, &value, host_dir).await?;
            }
            "DLMessageUploadFiles" => {
                handle_upload_files(client, &value, host_dir).await?;
            }
            "DLMessageGetFreeDiskSpace" => {
                // Minimal implementation: report unknown/zero with success
                client
                    .send_status_response(0, None, Some(plist::Value::Integer(0u64.into())))
                    .await?;
            }
            "DLContentsOfDirectory" => {
                // Minimal: return empty listing
                let empty = plist::Value::Dictionary(Dictionary::new());
                client.send_status_response(0, None, Some(empty)).await?;
            }
            "DLMessageCreateDirectory" => {
                let status = create_directory_from_message(&value, host_dir);
                client.send_status_response(status, None, None).await?;
            }
            "DLMessageMoveFiles" | "DLMessageMoveItems" => {
                let status = move_files_from_message(&value, host_dir);
                client
                    .send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
            }
            "DLMessageRemoveFiles" | "DLMessageRemoveItems" => {
                let status = remove_files_from_message(&value, host_dir);
                client
                    .send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
            }
            "DLMessageCopyItem" => {
                let status = copy_item_from_message(&value, host_dir);
                client
                    .send_status_response(
                        status,
                        None,
                        Some(plist::Value::Dictionary(Dictionary::new())),
                    )
                    .await?;
            }
            "DLMessageProcessMessage" => {
                // Final status/content: return inner dict
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
                eprintln!("Unsupported DL message: {other}");
                client
                    .send_status_response(-1, Some("Operation not supported"), None)
                    .await?;
            }
        }
    }
}

async fn handle_download_files(
    client: &mut MobileBackup2Client,
    dl_value: &plist::Value,
    host_dir: &Path,
) -> Result<(), idevice::IdeviceError> {
    // dl_value is an array: ["DLMessageDownloadFiles", [paths...], progress?]
    let mut err_any = false;
    if let plist::Value::Array(arr) = dl_value
        && arr.len() >= 2
        && let Some(plist::Value::Array(files)) = arr.get(1)
    {
        for pv in files {
            if let Some(path) = pv.as_string()
                && let Err(e) = send_single_file(client, host_dir, path).await
            {
                eprintln!("Failed to send file {path}: {e}");
                err_any = true;
            }
        }
    }
    // terminating zero dword
    client.idevice.send_raw(&0u32.to_be_bytes()).await?;
    // status response
    if err_any {
        client
            .send_status_response(
                -13,
                Some("Multi status"),
                Some(plist::Value::Dictionary(Dictionary::new())),
            )
            .await
    } else {
        client
            .send_status_response(0, None, Some(plist::Value::Dictionary(Dictionary::new())))
            .await
    }
}

async fn send_single_file(
    client: &mut MobileBackup2Client,
    host_dir: &Path,
    rel_path: &str,
) -> Result<(), idevice::IdeviceError> {
    let full = host_dir.join(rel_path);
    let path_bytes = rel_path.as_bytes().to_vec();
    let nlen = (path_bytes.len() as u32).to_be_bytes();
    client.idevice.send_raw(&nlen).await?;
    client.idevice.send_raw(&path_bytes).await?;

    let mut f = match std::fs::File::open(&full) {
        Ok(f) => f,
        Err(e) => {
            // send error
            let desc = e.to_string();
            let size = (desc.len() as u32 + 1).to_be_bytes();
            let mut hdr = Vec::with_capacity(5);
            hdr.extend_from_slice(&size);
            hdr.push(CODE_ERROR_LOCAL);
            client.idevice.send_raw(&hdr).await?;
            client.idevice.send_raw(desc.as_bytes()).await?;
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
        hdr.push(CODE_FILE_DATA);
        client.idevice.send_raw(&hdr).await?;
        client.idevice.send_raw(&buf[..read]).await?;
    }
    // success trailer
    let mut ok = [0u8; 5];
    ok[..4].copy_from_slice(&1u32.to_be_bytes());
    ok[4] = CODE_SUCCESS;
    client.idevice.send_raw(&ok).await?;
    Ok(())
}

async fn handle_upload_files(
    client: &mut MobileBackup2Client,
    _dl_value: &plist::Value,
    host_dir: &Path,
) -> Result<(), idevice::IdeviceError> {
    // Minimal receiver: read pairs of (dir, filename) and block stream
    // Receive dir name
    loop {
        let dlen = read_be_u32(client).await?;
        if dlen == 0 {
            break;
        }
        let dname = read_exact_string(client, dlen as usize).await?;
        let flen = read_be_u32(client).await?;
        if flen == 0 {
            break;
        }
        let fname = read_exact_string(client, flen as usize).await?;
        let dst = host_dir.join(&fname);
        if let Some(parent) = dst.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut file = std::fs::File::create(&dst)
            .map_err(|e| idevice::IdeviceError::InternalError(e.to_string()))?;
        loop {
            let nlen = read_be_u32(client).await?;
            if nlen == 0 {
                break;
            }
            let code = read_one(client).await?;
            if code == CODE_FILE_DATA {
                let size = (nlen - 1) as usize;
                let data = read_exact(client, size).await?;
                file.write_all(&data)
                    .map_err(|e| idevice::IdeviceError::InternalError(e.to_string()))?;
            } else {
                let _ = read_exact(client, (nlen - 1) as usize).await?;
            }
        }
        let _ = dname; // not used
    }
    client
        .send_status_response(0, None, Some(plist::Value::Dictionary(Dictionary::new())))
        .await
}

async fn read_be_u32(client: &mut MobileBackup2Client) -> Result<u32, idevice::IdeviceError> {
    let buf = client.idevice.read_raw(4).await?;
    Ok(u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

async fn read_one(client: &mut MobileBackup2Client) -> Result<u8, idevice::IdeviceError> {
    let buf = client.idevice.read_raw(1).await?;
    Ok(buf[0])
}

async fn read_exact(
    client: &mut MobileBackup2Client,
    size: usize,
) -> Result<Vec<u8>, idevice::IdeviceError> {
    client.idevice.read_raw(size).await
}

async fn read_exact_string(
    client: &mut MobileBackup2Client,
    size: usize,
) -> Result<String, idevice::IdeviceError> {
    let buf = client.idevice.read_raw(size).await?;
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
            // shallow copy: create dir
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
