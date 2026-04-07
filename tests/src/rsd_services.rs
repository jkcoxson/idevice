// Jackson Coxson
// Tests for RSD-only services (iOS 17+).  All services here require a
// CoreDeviceProxy → software tunnel → RsdHandshake connection.
// If CoreDeviceProxy is not available (older iOS / not connected via USB) the
// entire module is skipped.

use idevice::{
    IdeviceService, ReadWrite, RsdService,
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
    services::{
        core_device::{AppServiceClient, DiagnostisServiceClient, OpenStdioSocketClient},
        debug_proxy::{DebugProxyClient, DebugserverCommand},
        installcoordination_proxy::InstallcoordinationProxy,
        restore_service::RestoreServiceClient,
    },
    tcp::handle::AdapterHandle,
};

use crate::run_test;

/// Attempt to set up a CoreDeviceProxy + software tunnel + RSD handshake.
/// Returns `None` if the device doesn't support it.
async fn rsd_connect(provider: &dyn IdeviceProvider) -> Option<(AdapterHandle, RsdHandshake)> {
    let proxy = match CoreDeviceProxy::connect(provider).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("  rsd_services: CoreDeviceProxy unavailable ({e}), skipping RSD tests");
            return None;
        }
    };
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = match proxy.create_software_tunnel() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("  rsd_services: software tunnel failed ({e}), skipping RSD tests");
            return None;
        }
    };
    let mut adapter = adapter.to_async_handle();
    let stream = match adapter.connect(rsd_port).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  rsd_services: RSD port connect failed ({e}), skipping RSD tests");
            return None;
        }
    };
    match RsdHandshake::new(stream).await {
        Ok(h) => Some((adapter, h)),
        Err(e) => {
            eprintln!("  rsd_services: RSD handshake failed ({e}), skipping RSD tests");
            None
        }
    }
}

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    // ── RestoreService ────────────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };
        println!("  rsd_services: RSD connected                             [ OK  ]");

        run_test!(
            "rsd: RestoreService connect + get_preflightinfo",
            success,
            failure,
            async {
                let mut svc =
                    RestoreServiceClient::connect_rsd(&mut adapter, &mut handshake).await?;
                let info = svc.get_preflightinfo().await?;
                println!("({} keys)", info.len());
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // ── AppService ────────────────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!("rsd: AppService list_apps", success, failure, async {
            let mut svc: AppServiceClient<Box<dyn ReadWrite>> =
                AppServiceClient::connect_rsd(&mut adapter, &mut handshake).await?;
            let apps = svc.list_apps(false, true, false, false, true).await?;
            if apps.is_empty() {
                Err(idevice::IdeviceError::UnexpectedResponse(
                    "app list was empty".into(),
                ))
            } else {
                println!("({} apps)", apps.len());
                Ok(())
            }
        });
    }

    // ── InstallcoordinationProxy ──────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!(
            "rsd: InstallcoordinationProxy query_app_path (com.apple.Preferences)",
            success,
            failure,
            async {
                let mut svc: InstallcoordinationProxy<Box<dyn ReadWrite>> =
                    InstallcoordinationProxy::connect_rsd(&mut adapter, &mut handshake).await?;
                let path = svc.query_app_path("com.apple.Preferences").await?;
                println!("({path})");
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // ── OpenStdioSocket ───────────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!("rsd: OpenStdioSocket read_uuid", success, failure, async {
            let mut svc: OpenStdioSocketClient =
                OpenStdioSocketClient::connect_rsd(&mut adapter, &mut handshake).await?;
            let uuid = svc.read_uuid().await?;
            println!("({uuid})");
            Ok::<(), idevice::IdeviceError>(())
        });
    }

    // ── DebugProxy ────────────────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!(
            "rsd: DebugProxy connect + qSupported",
            success,
            failure,
            async {
                let mut svc: DebugProxyClient<Box<dyn ReadWrite>> =
                    DebugProxyClient::connect_rsd(&mut adapter, &mut handshake).await?;
                let resp = svc
                    .send_command(DebugserverCommand::new("qSupported".into(), vec![]))
                    .await?;
                println!("({} bytes)", resp.map(|r| r.len()).unwrap_or(0));
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // ── AppService extended ───────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!("rsd: AppService list_processes", success, failure, async {
            let mut svc: AppServiceClient<Box<dyn ReadWrite>> =
                AppServiceClient::connect_rsd(&mut adapter, &mut handshake).await?;
            let procs = svc.list_processes().await?;
            if procs.is_empty() {
                Err(idevice::IdeviceError::UnexpectedResponse(
                    "process list was empty".into(),
                ))
            } else {
                println!("({} processes)", procs.len());
                Ok(())
            }
        });
    }

    // Launch Settings, verify PID, then send SIGTERM to clean up.
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!(
            "rsd: AppService launch + send_signal (com.apple.Preferences)",
            success,
            failure,
            async {
                let mut svc: AppServiceClient<Box<dyn ReadWrite>> =
                    AppServiceClient::connect_rsd(&mut adapter, &mut handshake).await?;
                let launch = svc
                    .launch_application(
                        "com.apple.Preferences",
                        &[],
                        true,  // kill_existing
                        false, // start_suspended
                        None,
                        None,
                        None,
                    )
                    .await?;
                println!("(pid {})", launch.pid);
                // SIGTERM = 15 — ask it to quit cleanly
                let _ = svc.send_signal(launch.pid, 15).await;
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // ── DiagnosticsService ────────────────────────────────────────────────────
    {
        let Some((mut adapter, mut handshake)) = rsd_connect(provider).await else {
            *failure += 1;
            return;
        };

        run_test!(
            "rsd: DiagnosticsService capture_sysdiagnose (dry_run)",
            success,
            failure,
            async {
                let mut svc: DiagnostisServiceClient<Box<dyn ReadWrite>> =
                    DiagnostisServiceClient::connect_rsd(&mut adapter, &mut handshake).await?;
                let resp = svc.capture_sysdiagnose(true).await?;
                println!(
                    "({}, {} bytes expected)",
                    resp.preferred_filename, resp.expected_length
                );
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }
}
