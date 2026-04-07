// Jackson Coxson
// MobileBackup2 performs iOS device backups and restores.
// We only exercise the version exchange (connect); the higher-level
// request_backup_info / list_backups calls require the full DeviceLink
// handshake with valid UDID identifiers and do not work in a simple
// fire-and-receive pattern against a real device.

use crate::run_test;
use idevice::{
    IdeviceService, provider::IdeviceProvider, services::mobilebackup2::MobileBackup2Client,
};

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    run_test!(
        "mobilebackup2: connect (version exchange)",
        success,
        failure,
        async { MobileBackup2Client::connect(provider).await.map(|_| ()) }
    );
}
