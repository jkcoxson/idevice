// Jackson Coxson
// This test is inteded to be run on the vphone.
// Running this test suite on a real device can have unintended consequnces.
// Only run on hardware you are okay with destroying.

use std::process::ExitCode;

use idevice::{
    IdeviceService,
    lockdown::LockdownClient,
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection},
};

mod afc;
mod amfi;
mod bt_packet_logger;
mod companion_proxy;
mod crash_reports;
mod diagnostics_relay;
mod dvt;
mod heartbeat;
mod house_arrest;
mod installation_proxy;
mod location_simulation;
mod misagent;
mod mobile_image_mounter;
mod mobileactivationd;
mod mobilebackup2;
mod notification_proxy;
mod os_trace_relay;
mod pcapd;
mod preboard_service;
mod rsd_services;
mod screenshotr;
mod springboard;
mod syslog_relay;

/// Runs an async test case, printing PASS/FAIL and updating the counters.
///
/// Usage:
///   run_test!(name_literal, &mut success, &mut failure, some_async_expr)
///
/// The async expression must produce `Result<_, impl Display>`.
#[macro_export]
macro_rules! run_test {
    ($name:expr, $success:expr, $failure:expr, $fut:expr) => {{
        print!("  {:<60}", $name);
        match $fut.await {
            Ok(_) => {
                println!("\x1b[32m[ PASS ]\x1b[0m");
                *$success += 1;
            }
            Err(e) => {
                println!("\x1b[31m[ FAIL ]\x1b[0m {e}");
                *$failure += 1;
            }
        }
    }};
}

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();

    println!("idevice test harness");

    if std::env::var("RUST_LOG").is_err() {
        println!("WARNING: RUST_LOG hasn't been set, you won't get any info on tests");
    }

    // Get the first connected device by USB
    let mut u = UsbmuxdConnection::default()
        .await
        .expect("Failed to connect to usbmuxd");

    let devs = u
        .get_devices()
        .await
        .expect("Failed to get devices from usbmxud");

    let usbmuxd_device = devs
        .into_iter()
        .find(|x| x.connection_type == Connection::Usb)
        .expect("No device connected by USB");

    let usbmuxd_device =
        usbmuxd_device.to_provider(UsbmuxdAddr::default(), "idevice-tests".to_string());

    // Get the device name for our scary warning
    let mut l = LockdownClient::connect(&usbmuxd_device)
        .await
        .expect("Failed to connect to lockdown");
    let dev_name = l
        .get_value(Some("DeviceName"), None)
        .await
        .expect("Failed to get device name");

    println!("\n---------------------------------------");
    println!("WARNING WARNING WARNING WARNING WARNING");
    println!("---------------------------------------");
    println!(
        "You are about to run the idevice test suite on {} - {}",
        dev_name.as_string().unwrap(),
        usbmuxd_device.udid
    );
    println!("This test suite can have unintended consequnces on misbehaving code.");
    println!("Make sure this is a device you are willing to destroy.");
    println!("This is your last warning. Continuing in 5...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("4...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("3...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("2...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("1...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("0...");
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    println!("Starting test suite on {}", usbmuxd_device.udid);

    let mut success = 0u32;
    let mut failure = 0u32;

    // ── Lockdown ──────────────────────────────────────────────────────────────
    println!("\n── Lockdown ─────────────────────────────────────────────────────");
    run_test!("lockdown: connect", &mut success, &mut failure, async {
        LockdownClient::connect(&usbmuxd_device).await.map(|_| ())
    });
    {
        let mut l = match LockdownClient::connect(&usbmuxd_device).await {
            Ok(c) => c,
            Err(e) => {
                println!("  lockdown: FATAL - cannot connect: {e}");
                failure += 1;
                println!("\nAll tests finished!");
                println!("Success: {success}");
                println!("Failed: {failure}");
                return ExitCode::FAILURE;
            }
        };
        run_test!(
            "lockdown: get_value DeviceName",
            &mut success,
            &mut failure,
            async { l.get_value(Some("DeviceName"), None).await.map(|_| ()) }
        );
        run_test!(
            "lockdown: get_value ProductType",
            &mut success,
            &mut failure,
            async { l.get_value(Some("ProductType"), None).await.map(|_| ()) }
        );
        run_test!(
            "lockdown: get_value ProductVersion",
            &mut success,
            &mut failure,
            async { l.get_value(Some("ProductVersion"), None).await.map(|_| ()) }
        );
        run_test!(
            "lockdown: get_value UniqueDeviceID",
            &mut success,
            &mut failure,
            async { l.get_value(Some("UniqueDeviceID"), None).await.map(|_| ()) }
        );
    }

    // ── Diagnostics Relay ─────────────────────────────────────────────────────
    println!("\n── DiagnosticsRelay ─────────────────────────────────────────────");
    diagnostics_relay::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── Screenshotr ───────────────────────────────────────────────────────────
    println!("\n── Screenshotr ──────────────────────────────────────────────────");
    screenshotr::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── InstallationProxy ─────────────────────────────────────────────────────
    println!("\n── InstallationProxy ────────────────────────────────────────────");
    installation_proxy::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── AFC ───────────────────────────────────────────────────────────────────
    println!("\n── AFC ──────────────────────────────────────────────────────────");
    afc::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── SpringBoardServices ───────────────────────────────────────────────────
    println!("\n── SpringBoardServices ──────────────────────────────────────────");
    springboard::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── NotificationProxy ─────────────────────────────────────────────────────
    println!("\n── NotificationProxy ────────────────────────────────────────────");
    notification_proxy::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── Heartbeat ─────────────────────────────────────────────────────────────
    println!("\n── Heartbeat ────────────────────────────────────────────────────");
    heartbeat::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── AMFI ──────────────────────────────────────────────────────────────────
    println!("\n── AMFI ─────────────────────────────────────────────────────────");
    amfi::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── Misagent ──────────────────────────────────────────────────────────────
    println!("\n── Misagent ─────────────────────────────────────────────────────");
    misagent::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── SyslogRelay ───────────────────────────────────────────────────────────
    println!("\n── SyslogRelay ──────────────────────────────────────────────────");
    syslog_relay::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── OsTraceRelay ──────────────────────────────────────────────────────────
    println!("\n── OsTraceRelay ─────────────────────────────────────────────────");
    os_trace_relay::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── LocationSimulation ────────────────────────────────────────────────────
    println!("\n── LocationSimulation ───────────────────────────────────────────");
    location_simulation::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── CrashReports ──────────────────────────────────────────────────────────
    println!("\n── CrashReportCopyMobile ────────────────────────────────────────");
    crash_reports::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── HouseArrest ───────────────────────────────────────────────────────────
    println!("\n── HouseArrest ──────────────────────────────────────────────────");
    house_arrest::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── BtPacketLogger ────────────────────────────────────────────────────────
    println!("\n── BtPacketLogger ───────────────────────────────────────────────");
    bt_packet_logger::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── CompanionProxy ────────────────────────────────────────────────────────
    println!("\n── CompanionProxy ───────────────────────────────────────────────");
    companion_proxy::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── MobileActivationd ─────────────────────────────────────────────────────
    println!("\n── MobileActivationd ────────────────────────────────────────────");
    mobileactivationd::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── MobileImageMounter ────────────────────────────────────────────────────
    println!("\n── MobileImageMounter ───────────────────────────────────────────");
    mobile_image_mounter::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── Pcapd ─────────────────────────────────────────────────────────────────
    println!("\n── Pcapd ────────────────────────────────────────────────────────");
    pcapd::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── PreboardService ───────────────────────────────────────────────────────
    println!("\n── PreboardService ──────────────────────────────────────────────");
    preboard_service::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── MobileBackup2 ─────────────────────────────────────────────────────────
    println!("\n── MobileBackup2 ────────────────────────────────────────────────");
    mobilebackup2::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── RSD Services (iOS 17+) ────────────────────────────────────────────────
    println!("\n── RSD Services (iOS 17+) ───────────────────────────────────────");
    rsd_services::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    // ── DVT / Instruments ─────────────────────────────────────────────────────
    println!("\n── DVT / Instruments ────────────────────────────────────────────");
    dvt::run_tests(&usbmuxd_device, &mut success, &mut failure).await;

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("All tests finished!");
    println!("  Success: {success}");
    println!("  Failed:  {failure}");

    if failure == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
