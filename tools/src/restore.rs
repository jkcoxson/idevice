use std::{sync::Arc, time::Duration};

use idevice::{
    IdeviceError, IdeviceService,
    lockdown::LockdownClient,
    preboard_service::{PreboardServiceClient, StashbagOutcome},
    provider::IdeviceProvider,
    restore::{
        RestoreCancel, RestoreContext, RestoreError, RestoreOptions, RestoreProgressEvent,
        fdr::{FDR_CTRL_PORT, FdrClient, FdrConnector, run_fdr_listener},
        img4,
        ipsw::Ipsw,
        progress_channel,
        recovery::RecoveryDevice,
        restored::RestoredClient,
        run_restore,
        state_machine::DataPortConnector,
    },
    tss::{TSSRequest, extract_img4_ticket, select_build_identity},
    usbmuxd::{Connection, UsbmuxdAddr},
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use tokio::io::BufReader;

use crate::restore_usb::find_recovery_transport;

type Fs = BufReader<tokio::fs::File>;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Restore an IPSW onto a device")
        .with_flag(
            JkFlag::new("ipsw")
                .with_short("i")
                .with_argument(JkArgument::new().required(true))
                .with_help("Path to the (unzipped or zipped) IPSW"),
        )
        .with_flag(
            JkFlag::new("filesystem")
                .with_short("f")
                .with_argument(JkArgument::new())
                .with_help("Path to a decrypted filesystem DMG (for AEA IPSWs)"),
        )
        .with_flag(JkFlag::new("erase").with_help("Erase the device (default: update)"))
        .with_flag(
            JkFlag::new("exit-recovery")
                .with_help("Just reboot a device out of recovery mode and exit"),
        )
}

pub async fn main(arguments: &CollectedArguments, provider: Option<Box<dyn IdeviceProvider>>) {
    if let Err(e) = run(arguments, provider).await {
        eprintln!("restore failed: {e}");
        std::process::exit(1);
    }
}

async fn run(
    arguments: &CollectedArguments,
    provider: Option<Box<dyn IdeviceProvider>>,
) -> Result<(), IdeviceError> {
    // Rescue path: boot a stuck device out of recovery and exit.
    if arguments.has_flag("exit-recovery") {
        let transport = find_recovery_transport(None, Duration::from_secs(5)).await?;
        let mut recovery = RecoveryDevice::new(transport).await?;
        let _ = recovery.set_autoboot(true).await;
        // The reboot command tears down the USB connection, so a transport error
        // here is expected and benign.
        let _ = recovery.reboot().await;
        println!("rebooted device out of recovery");
        return Ok(());
    }

    let ipsw_path: String = arguments
        .get_flag("ipsw")
        .ok_or_else(|| IdeviceError::Restore(RestoreError::Other("--ipsw is required".into())))?;
    let filesystem_path: Option<String> = arguments.get_flag("filesystem");
    let behavior = if arguments.has_flag("erase") {
        "Erase"
    } else {
        "Update"
    };

    if behavior == "Update"
        && let Some(provider) = provider.as_deref()
    {
        create_stashbag_if_needed(provider, &ipsw_path).await?;
    }

    // 1. Acquire a recovery/DFU device (entering recovery from normal if needed)
    let acquired = acquire_recovery_device(provider.as_deref()).await?;
    let recovery = acquired.recovery;
    let info = recovery.info().clone();
    let ap_nonce = acquired.ap_nonce.or_else(|| info.ap_nonce.clone());
    let sep_nonce = acquired.sep_nonce.or_else(|| info.sep_nonce.clone());
    if ap_nonce.is_none() {
        eprintln!(
            "warning: no ApNonce available (start from normal mode for a personalized ticket)"
        );
    }
    let board_id = info.bdid.ok_or_else(|| {
        IdeviceError::Restore(RestoreError::Other(
            "recovery device did not report BDID".into(),
        ))
    })?;
    let chip_id = info.cpid.ok_or_else(|| {
        IdeviceError::Restore(RestoreError::Other(
            "recovery device did not report CPID".into(),
        ))
    })?;
    let ecid = info.ecid.ok_or_else(|| {
        IdeviceError::Restore(RestoreError::Other(
            "recovery device did not report ECID".into(),
        ))
    })?;
    println!("recovery device: board={board_id:#x} chip={chip_id:#x} ecid={ecid:#x}");

    // 2. Open the IPSW and select the build identity
    let mut ipsw = open_ipsw(&ipsw_path).await?;
    let build_manifest = ipsw.build_manifest().await?;
    let build_identity =
        select_build_identity(&build_manifest, board_id, chip_id, Some(behavior))?.clone();
    println!("selected {behavior} build identity");

    let fs_path: std::path::PathBuf = match &filesystem_path {
        Some(p) => std::path::PathBuf::from(p),
        None => {
            let os_path =
                idevice::restore::ipsw::component_path(&build_identity, "OS").map_err(|_| {
                    IdeviceError::Restore(RestoreError::Other(
                        "build identity has no OS component".into(),
                    ))
                })?;
            let dest = std::env::temp_dir().join(format!("idevice-restore-fs-{ecid:x}.dmg"));
            println!("extracting filesystem `{os_path}` -> {}", dest.display());
            let mut out = tokio::fs::File::create(&dest).await.map_err(|e| {
                IdeviceError::Restore(RestoreError::Ipsw(format!(
                    "failed to create {}: {e}",
                    dest.display()
                )))
            })?;
            ipsw.extract_to_writer(&os_path, &mut out).await?;
            dest
        }
    };
    let mut fs_file = tokio::fs::File::open(&fs_path).await.map_err(|e| {
        IdeviceError::Restore(RestoreError::Ipsw(format!(
            "open filesystem {}: {e}",
            fs_path.display()
        )))
    })?;

    // 3. Fetch the AP ticket (ApImg4Ticket)
    let parameters = plist_macro::plist!({
        "ApProductionMode": true,
        "ApSecurityMode": true,
        "ApSupportsImg4": true
    });
    let parameters = parameters.as_dictionary().unwrap().clone();

    let mut request = TSSRequest::new();
    request.set_ap_img4_ticket(true);
    request.add_common_tags(
        board_id,
        chip_id,
        ecid,
        ap_nonce.clone(),
        sep_nonce.clone().or(Some(vec![0u8; 20])),
    );
    request.add_ap_tags(&build_identity);
    request.add_ap_manifest_tags(&build_identity, &parameters)?;
    let response = request.send().await?;
    let response = match response {
        plist::Value::Dictionary(d) => d,
        _ => {
            return Err(IdeviceError::Restore(RestoreError::TssResponse(
                "response is not a dictionary".into(),
            )));
        }
    };
    let ticket = extract_img4_ticket(&response)?;
    println!("got ApImg4Ticket ({} bytes)", ticket.len());

    // 4. Boot the restore ramdisk
    boot_to_restore(recovery, &mut ipsw, &build_identity, &ticket, ecid).await?;
    println!("device instructed to boot into restore mode");

    // 5. Connect to restored on the re-enumerated device
    let addr = UsbmuxdAddr::default();
    let mut restored =
        RestoredClient::connect_by_ecid(&addr, ecid, "idevice-restore", Duration::from_secs(60))
            .await?;
    // The usbmux id restored was found on; data-port / FDR connections reuse it so
    // they target this device and not whichever USB device usbmux happens to list
    // first (which matters when more than one device is attached).
    let device_id = restored.device_id;
    println!("connected to restored");

    // 6. Start the FDR trust channel
    let fdr_connector = Arc::new(UsbmuxFdrConnector {
        addr: addr.clone(),
        device_id,
    });
    match start_fdr(fdr_connector.clone()).await {
        Ok(()) => println!("FDR listener started"),
        Err(e) => eprintln!("warning: FDR did not start ({e}); continuing"),
    }

    // 8. Run the restore state machine
    let options = RestoreOptions::new().build();
    let mut data_ports = UsbmuxDataPorts {
        addr: addr.clone(),
        device_id,
    };

    // Progress events stream over a channel; a task drains them into the terminal
    // reporter so the concurrent transfer and request pump can both emit.
    let (progress_tx, mut progress_rx) = progress_channel();
    let progress_task = tokio::spawn(async move {
        let mut cli = CliProgress::default();
        while let Some(event) = progress_rx.recv().await {
            cli.handle(event);
        }
    });

    // Ctrl-C requests a graceful cancel: the restore reboots the device toward
    // recovery rather than leaving it wedged. A second Ctrl-C aborts hard.
    let cancel = RestoreCancel::new();
    let cancel_signal = cancel.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!("\ncancelling restore; the device will reboot to recovery…");
            cancel_signal.cancel();
        }
    });

    let ctx = RestoreContext {
        restored: &mut restored,
        build_identity: &build_identity,
        board_id,
        chip_id,
        ecid,
        tss_ticket: &ticket,
        components: &mut ipsw,
        filesystem: Some(&mut fs_file as &mut dyn idevice::restore::FilesystemImage),
        data_ports: &mut data_ports,
        progress: Some(progress_tx),
        cancel: Some(cancel),
    };

    let result = run_restore(ctx, options).await;
    // `run_restore` dropped the last sender, so the reporter drains and finishes.
    let _ = progress_task.await;
    match result {
        Ok(()) => {
            println!("restore complete, have a nice day");
            Ok(())
        }
        Err(IdeviceError::Restore(RestoreError::Cancelled)) => {
            println!("restore cancelled; device rebooted toward recovery");
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// A recovery device plus the nonces read from normal mode (if it was reached
/// through normal mode).
struct AcquiredDevice {
    recovery: RecoveryDevice,
    ap_nonce: Option<Vec<u8>>,
    sep_nonce: Option<Vec<u8>>,
}

/// Finds a recovery/DFU device, entering recovery from normal mode if none is
/// already present. When going through normal mode, the AP/SEP nonces are read
/// via lockdown (the only reliable source for a personalized ticket).
async fn acquire_recovery_device(
    provider: Option<&dyn IdeviceProvider>,
) -> Result<AcquiredDevice, IdeviceError> {
    // Already in recovery/DFU?
    if let Ok(transport) = find_recovery_transport(None, Duration::from_secs(1)).await {
        println!("found a device already in recovery/DFU mode");
        let recovery = RecoveryDevice::new(transport).await?;
        return Ok(AcquiredDevice {
            recovery,
            ap_nonce: None,
            sep_nonce: None,
        });
    }

    // Otherwise, enter recovery from normal mode (requires a normal-mode device).
    let provider = provider.ok_or_else(|| {
        IdeviceError::Restore(RestoreError::Other(
            "no device found in recovery/DFU or normal mode (is one connected?)".into(),
        ))
    })?;
    println!("no recovery device found; entering recovery from normal mode");
    let mut lockdown = LockdownClient::connect(provider).await?;
    lockdown
        .start_session(&provider.get_pairing_file().await?)
        .await?;
    let ecid = lockdown
        .get_value(Some("UniqueChipID"), None)
        .await?
        .as_unsigned_integer()
        .ok_or_else(|| IdeviceError::Restore(RestoreError::Other("UniqueChipID missing".into())))?;

    // Read the personalization nonces while still in normal mode.
    let ap_nonce = read_nonce(&mut lockdown, "ApNonce").await;
    let sep_nonce = read_nonce(&mut lockdown, "SEPNonce").await;
    println!(
        "read nonces from normal mode: ApNonce={} SEPNonce={}",
        ap_nonce.is_some(),
        sep_nonce.is_some()
    );

    lockdown.enter_recovery().await?;

    let transport = find_recovery_transport(Some(ecid), Duration::from_secs(60)).await?;
    let recovery = RecoveryDevice::new(transport).await?;
    Ok(AcquiredDevice {
        recovery,
        ap_nonce,
        sep_nonce,
    })
}

async fn create_stashbag_if_needed(
    provider: &dyn IdeviceProvider,
    ipsw_path: &str,
) -> Result<(), IdeviceError> {
    let mut lockdown = match LockdownClient::connect(provider).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("skipping stashbag: no normal-mode lockdown connection ({e})");
            return Ok(());
        }
    };
    lockdown
        .start_session(&provider.get_pairing_file().await?)
        .await?;

    let has_sidp = lockdown
        .get_value(Some("HasSiDP"), None)
        .await
        .ok()
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);
    if !has_sidp {
        println!("device does not use Secure-in-Data-Protection; no stashbag needed");
        return Ok(());
    }
    println!("update on a Secure-in-Data-Protection device; preparing stashbag...");

    async fn get_uint(ld: &mut LockdownClient, key: &str) -> Result<u64, IdeviceError> {
        ld.get_value(Some(key), None)
            .await?
            .as_unsigned_integer()
            .ok_or_else(|| {
                IdeviceError::Restore(RestoreError::Other(format!(
                    "{key} missing or not an integer"
                )))
            })
    }
    let board_id = get_uint(&mut lockdown, "BoardId").await?;
    let chip_id = get_uint(&mut lockdown, "ChipID").await?;
    let ecid = get_uint(&mut lockdown, "UniqueChipID").await?;
    let ap_nonce = read_nonce(&mut lockdown, "ApNonce").await;
    let sep_nonce = read_nonce(&mut lockdown, "SEPNonce").await;
    drop(lockdown);

    // Select the Update build identity and build the local preboard manifest.
    let mut ipsw = open_ipsw(ipsw_path).await?;
    let build_manifest = ipsw.build_manifest().await?;
    let build_identity =
        select_build_identity(&build_manifest, board_id, chip_id, Some("Update"))?.clone();
    let manifest = img4::build_preboard_manifest(&build_identity, board_id, chip_id)?;

    // Create the stashbag (may prompt for the passcode on the device).
    let mut preboard = PreboardServiceClient::connect(provider).await?;
    match preboard.create_stashbag(&manifest).await? {
        StashbagOutcome::NotRequired => {
            println!("device reported no stashbag required");
            return Ok(());
        }
        StashbagOutcome::CommitRequired => {}
    }
    drop(preboard);

    // Fetch the AP ticket and commit the stashbag against it.
    let parameters = plist_macro::plist!({
        "ApProductionMode": true,
        "ApSecurityMode": true,
        "ApSupportsImg4": true
    });
    let parameters = parameters.as_dictionary().unwrap().clone();
    let mut request = TSSRequest::new();
    request.set_ap_img4_ticket(true);
    request.add_common_tags(
        board_id,
        chip_id,
        ecid,
        ap_nonce,
        sep_nonce.or(Some(vec![0u8; 20])),
    );
    request.add_ap_tags(&build_identity);
    request.add_ap_manifest_tags(&build_identity, &parameters)?;
    let response = match request.send().await? {
        plist::Value::Dictionary(d) => d,
        _ => {
            return Err(IdeviceError::Restore(RestoreError::TssResponse(
                "response is not a dictionary".into(),
            )));
        }
    };
    let ticket = extract_img4_ticket(&response)?;

    let mut preboard = PreboardServiceClient::connect(provider).await?;
    preboard.commit_stashbag(&ticket).await?;
    println!("stashbag committed; user data will be preserved across the update");
    Ok(())
}

async fn read_nonce(lockdown: &mut LockdownClient, key: &str) -> Option<Vec<u8>> {
    lockdown
        .get_value(Some(key), None)
        .await
        .ok()
        .and_then(|v| v.as_data().map(|d| d.to_vec()))
}

/// Reads and personalizes an IPSW component, then uploads it to the device.
async fn send_component(
    recovery: &mut RecoveryDevice,
    ipsw: &mut Ipsw<Fs>,
    build_identity: &plist::Dictionary,
    ticket: &[u8],
    name: &str,
) -> Result<(), IdeviceError> {
    let raw = ipsw.read_component(build_identity, name).await?;
    let fourcc = img4::restore_fourcc_override(name);
    let personalized = img4::stitch_component(&raw, ticket, fourcc, &[])?;
    println!("sending {name} ({} bytes)", personalized.len());
    recovery.send_buffer(&personalized).await
}

async fn boot_to_restore(
    mut recovery: RecoveryDevice,
    ipsw: &mut Ipsw<Fs>,
    build_identity: &plist::Dictionary,
    ticket: &[u8],
    ecid: u64,
) -> Result<(), IdeviceError> {
    // If in recovery mode, load iBEC and jump to it (the device re-enumerates).
    if recovery.mode().is_recovery() {
        send_component(&mut recovery, ipsw, build_identity, ticket, "iBEC").await?;
        let _ = recovery.send_command_with_request("go", 1).await;
        let _ = recovery.finish_transfer().await;

        // Release our USB handle and give the device time to reset and
        // re-enumerate running iBEC before reconnecting.
        drop(recovery);
        tokio::time::sleep(Duration::from_secs(3)).await;
        println!("waiting for iBEC to come up...");
        let transport = find_recovery_transport(Some(ecid), Duration::from_secs(30)).await?;
        recovery = RecoveryDevice::new(transport).await?;
    }

    let restore_boot_args = "rd=md0 nand-enable-reformat=1 -progress";

    // Wait until iBEC is up (build-version becomes readable).
    for _ in 0..30 {
        if let Ok(bytes) = recovery.getenv("build-version").await
            && !bytes.is_empty()
        {
            break;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    recovery.set_autoboot(false).await?;

    if send_component(&mut recovery, ipsw, build_identity, ticket, "RestoreLogo")
        .await
        .is_ok()
    {
        let _ = recovery.send_command("setpicture 4").await;
        let _ = recovery.send_command("bgcolor 0 0 0").await;
    }

    // Components iBoot loads (IsLoadedByiBoot && !IsLoadedByiBootStage1).
    for name in idevice::restore::ipsw::components_loaded_by_iboot(build_identity) {
        send_component(&mut recovery, ipsw, build_identity, ticket, &name).await?;
        recovery.send_command("firmware").await?;
    }

    // Ramdisk.
    send_component(
        &mut recovery,
        ipsw,
        build_identity,
        ticket,
        "RestoreRamDisk",
    )
    .await?;
    recovery.send_command("ramdisk").await?;
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Device tree.
    send_component(
        &mut recovery,
        ipsw,
        build_identity,
        ticket,
        "RestoreDeviceTree",
    )
    .await?;
    recovery.send_command("devicetree").await?;

    if has_component(build_identity, "RestoreSEP")
        && send_component(&mut recovery, ipsw, build_identity, ticket, "RestoreSEP")
            .await
            .is_ok()
    {
        let _ = recovery.send_command("rsepfirmware").await;
    }

    // Kernelcache and boot.
    send_component(
        &mut recovery,
        ipsw,
        build_identity,
        ticket,
        "RestoreKernelCache",
    )
    .await?;
    let _ = recovery.finish_transfer().await;
    recovery
        .send_command(&format!("setenv boot-args {restore_boot_args}"))
        .await?;
    let _ = recovery.send_command_with_request("bootx", 1).await;
    Ok(())
}

/// Whether the build identity has a component.
fn has_component(build_identity: &plist::Dictionary, name: &str) -> bool {
    build_identity
        .get("Manifest")
        .and_then(|m| m.as_dictionary())
        .map(|m| m.contains_key(name))
        .unwrap_or(false)
}

async fn open_ipsw(path: &str) -> Result<Ipsw<Fs>, IdeviceError> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| IdeviceError::Restore(RestoreError::Ipsw(format!("open {path}: {e}"))))?;
    Ipsw::new(BufReader::new(file)).await
}

/// Starts the FDR trust channel: control handshake then a background listener.
///
/// The FDR control port can take a moment to become available right after the
/// device enters restore mode, so the handshake is retried a few times.
async fn start_fdr(connector: Arc<UsbmuxFdrConnector>) -> Result<(), IdeviceError> {
    let mut last_err = None;
    for attempt in 0..5 {
        match connector.connect_device_port(FDR_CTRL_PORT).await {
            Ok(ctrl) => {
                let mut fdr = FdrClient::new(ctrl);
                match fdr.ctrl_handshake().await {
                    Ok(conn_port) => {
                        tokio::spawn(run_fdr_listener(fdr, connector, conn_port));
                        return Ok(());
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            Err(e) => last_err = Some(e),
        }
        if attempt < 4 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    Err(last_err.unwrap_or_else(|| {
        IdeviceError::Restore(RestoreError::Other("FDR failed to start".into()))
    }))
}

async fn connect_usb_device_port(
    addr: &UsbmuxdAddr,
    device_id: Option<u32>,
    port: u16,
    label: &str,
) -> Result<idevice::Idevice, IdeviceError> {
    let device_id = match device_id {
        Some(id) => id,
        None => {
            let mut mux = addr.connect(1).await?;
            mux.get_devices()
                .await?
                .into_iter()
                .find(|d| d.connection_type == Connection::Usb)
                .ok_or(IdeviceError::DeviceNotFound)?
                .device_id
        }
    };
    let mux = addr.connect(1).await?;
    mux.connect_to_device(device_id, port, label).await
}

/// A terminal progress reporter for the restore: a live bar for the device's
/// restore-operation percentage, a line per host step, and a byte counter (with a
/// bar when the total is known) while large images stream.
#[derive(Default)]
struct CliProgress {
    last_step: String,
    last_transfer_mb: u64,
    last_draw: Option<(u64, u64)>,
    bar_active: bool,
}

impl CliProgress {
    const WIDTH: usize = 30;

    fn end_bar(&mut self) {
        if self.bar_active {
            println!();
            self.bar_active = false;
            self.last_draw = None;
        }
    }

    fn draw_bar(&mut self, pct: u64, suffix: &str) {
        use std::io::Write;
        let pct = pct.min(100);
        let filled = (pct as usize * Self::WIDTH).div_ceil(100).min(Self::WIDTH);
        let bar: String = "█".repeat(filled) + &"░".repeat(Self::WIDTH - filled);
        print!("\r  [{bar}] {pct:3}%  {suffix}\x1b[K");
        let _ = std::io::stdout().flush();
        self.bar_active = true;
    }
}

impl CliProgress {
    /// Renders one progress event from the restore's progress channel.
    fn handle(&mut self, event: RestoreProgressEvent) {
        match event {
            RestoreProgressEvent::Operation {
                operation,
                progress,
            } => self.operation(operation, progress),
            RestoreProgressEvent::Step(name) => self.step(&name),
            RestoreProgressEvent::Transfer {
                component,
                sent,
                total,
            } => self.transfer(&component, sent, total),
        }
    }

    fn operation(&mut self, operation: u64, progress: u64) {
        let pct = progress.min(100);
        // The device re-reports the same value repeatedly; only redraw on change.
        if self.last_draw == Some((operation, pct)) {
            return;
        }
        self.last_draw = Some((operation, pct));
        self.draw_bar(pct, &format!("(operation {operation})"));
        if pct >= 100 {
            self.end_bar();
        }
    }

    fn step(&mut self, name: &str) {
        if name != self.last_step {
            self.end_bar();
            self.last_step = name.to_string();
            self.last_transfer_mb = 0;
            println!("▶ {name}");
        }
    }

    fn transfer(&mut self, component: &str, sent: u64, total: Option<u64>) {
        let mb = sent / (1024 * 1024);
        if mb == self.last_transfer_mb {
            return;
        }
        self.last_transfer_mb = mb;
        match total {
            Some(total) if total > 0 => {
                self.draw_bar(sent * 100 / total, &format!("{component} ({mb} MB)"))
            }
            _ => {
                use std::io::Write;
                print!("\r    {component}: {mb} MB\x1b[K");
                let _ = std::io::stdout().flush();
                self.bar_active = true;
            }
        }
    }
}

/// A [`DataPortConnector`] backed by usbmux, pinned to the restore-mode device's
/// usbmux id (see [`connect_usb_device_port`]).
struct UsbmuxDataPorts {
    addr: UsbmuxdAddr,
    device_id: Option<u32>,
}

impl DataPortConnector for UsbmuxDataPorts {
    fn connect(
        &self,
        port: u16,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<idevice::Idevice, IdeviceError>> + Send>,
    > {
        let addr = self.addr.clone();
        let device_id = self.device_id;
        Box::pin(async move {
            connect_usb_device_port(&addr, device_id, port, "idevice-restore-data").await
        })
    }
}

/// An [`FdrConnector`] backed by usbmux, pinned to the restore-mode device's
/// usbmux id (see [`connect_usb_device_port`]).
struct UsbmuxFdrConnector {
    addr: UsbmuxdAddr,
    device_id: Option<u32>,
}

impl FdrConnector for UsbmuxFdrConnector {
    fn connect_device_port(
        &self,
        port: u16,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<idevice::Idevice, IdeviceError>> + Send>,
    > {
        let addr = self.addr.clone();
        let device_id = self.device_id;
        Box::pin(async move {
            connect_usb_device_port(&addr, device_id, port, "idevice-restore-fdr").await
        })
    }
}
