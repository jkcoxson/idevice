// Jackson Coxson
// DVT / Instruments tests.
// All tests share one RemoteServerClient connection (tries CoreDeviceProxy/RSD
// first, falls back to the legacy Lockdown-based instruments service).

use std::time::Duration;

use crate::run_test;
use idevice::{
    IdeviceService, ReadWrite, RsdService,
    core_device_proxy::CoreDeviceProxy,
    dvt::{
        application_listing::ApplicationListingClient,
        condition_inducer::ConditionInducerClient,
        device_info::DeviceInfoClient,
        energy_monitor::EnergyMonitorClient,
        graphics::GraphicsClient,
        location_simulation::LocationSimulationClient,
        network_monitor::NetworkMonitorClient,
        notifications::NotificationsClient,
        process_control::ProcessControlClient,
        remote_server::RemoteServerClient,
        screenshot::ScreenshotClient,
        sysmontap::{SysmontapClient, SysmontapConfig},
    },
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};

const STREAM_TIMEOUT: Duration = Duration::from_secs(15);

/// Try to connect via CoreDeviceProxy + RSD (iOS 17+); fall back to Lockdown.
async fn connect(provider: &dyn IdeviceProvider) -> Option<RemoteServerClient<Box<dyn ReadWrite>>> {
    match CoreDeviceProxy::connect(provider).await {
        Ok(proxy) => {
            let rsd_port = proxy.tunnel_info().server_rsd_port;
            let adapter = match proxy.create_software_tunnel() {
                Ok(a) => a,
                Err(e) => {
                    eprintln!("  dvt: software tunnel failed ({e}), falling back");
                    return connect_lockdown(provider).await;
                }
            };
            let mut adapter = adapter.to_async_handle();
            let stream = match adapter.connect(rsd_port).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("  dvt: RSD connect failed ({e}), falling back");
                    return connect_lockdown(provider).await;
                }
            };
            let mut handshake = match RsdHandshake::new(stream).await {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("  dvt: RSD handshake failed ({e}), falling back");
                    return connect_lockdown(provider).await;
                }
            };
            match RemoteServerClient::connect_rsd(&mut adapter, &mut handshake).await {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("  dvt: connect_rsd failed ({e}), falling back");
                    connect_lockdown(provider).await
                }
            }
        }
        Err(_) => connect_lockdown(provider).await,
    }
}

async fn connect_lockdown(
    provider: &dyn IdeviceProvider,
) -> Option<RemoteServerClient<Box<dyn ReadWrite>>> {
    match RemoteServerClient::connect(provider).await {
        Ok(c) => Some(c),
        Err(e) => {
            eprintln!("  dvt: lockdown fallback also failed: {e}");
            None
        }
    }
}

pub async fn run_tests(provider: &dyn IdeviceProvider, success: &mut u32, failure: &mut u32) {
    // ── NetworkMonitor (dedicated first connection) ───────────────────────────
    // Must run on its own fresh connection BEFORE the main remote is created.
    // Running it after other services on the same connection causes the device
    // to RST the channel (likely due to stale async messages from prior channels).
    {
        if let Some(mut nm_remote) = connect(provider).await {
            let nm_result = async {
                let mut nm = NetworkMonitorClient::new(&mut nm_remote).await?;
                nm.start_monitoring().await?;
                match tokio::time::timeout(STREAM_TIMEOUT, nm.next_event()).await {
                    Ok(Ok(_)) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(idevice::IdeviceError::UnexpectedResponse(
                            "timed out waiting for network event".into(),
                        ));
                    }
                }
                nm.stop_monitoring().await
            }
            .await;

            print!("  {:<60}", "dvt: NetworkMonitor start + 1 event + stop");
            match nm_result {
                Ok(_) => {
                    println!("[ PASS ]");
                    *success += 1;
                }
                Err(e) => {
                    println!("[ FAIL ] {e}");
                    *failure += 1;
                }
            }
            // nm_remote fully dropped before main remote is created
        } else {
            println!("  dvt: NetworkMonitor: cannot connect, skipping");
            *failure += 1;
        }
    }

    // Main connection for all other DVT services
    let mut remote = match connect(provider).await {
        Some(c) => c,
        None => {
            println!(
                "  dvt: cannot establish RemoteServer connection, skipping remaining DVT tests"
            );
            *failure += 1;
            return;
        }
    };
    println!("  dvt: RemoteServer connected                             [ OK  ]");

    // ── DeviceInfo ────────────────────────────────────────────────────────────
    run_test!("dvt: DeviceInfo::new", success, failure, async {
        DeviceInfoClient::new(&mut remote).await.map(|_| ())
    });
    {
        let mut di = match DeviceInfoClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: DeviceInfoClient failed ({e}), skipping DeviceInfo tests");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: DeviceInfo::mach_kernel_name",
            success,
            failure,
            async {
                let name = di.mach_kernel_name().await?;
                println!("({name})");
                Ok::<(), idevice::IdeviceError>(())
            }
        );

        run_test!(
            "dvt: DeviceInfo::running_processes",
            success,
            failure,
            async {
                let procs = di.running_processes().await?;
                if procs.is_empty() {
                    Err(idevice::IdeviceError::UnexpectedResponse(
                        "no processes returned".into(),
                    ))
                } else {
                    println!("({} procs)", procs.len());
                    Ok(())
                }
            }
        );

        run_test!(
            "dvt: DeviceInfo::hardware_information",
            success,
            failure,
            async { di.hardware_information().await.map(|_| ()) }
        );

        run_test!(
            "dvt: DeviceInfo::network_information",
            success,
            failure,
            async { di.network_information().await.map(|_| ()) }
        );

        run_test!(
            "dvt: DeviceInfo::directory_listing /",
            success,
            failure,
            async {
                let entries = di.directory_listing("/").await?;
                if entries.is_empty() {
                    Err(idevice::IdeviceError::UnexpectedResponse(
                        "empty directory listing".into(),
                    ))
                } else {
                    println!("({} entries)", entries.len());
                    Ok(())
                }
            }
        );

        run_test!(
            "dvt: DeviceInfo::sysmon_process_attributes",
            success,
            failure,
            async {
                di.sysmon_process_attributes()
                    .await
                    .map(|a| println!("({} attrs)", a.len()))
            }
        );

        run_test!(
            "dvt: DeviceInfo::sysmon_system_attributes",
            success,
            failure,
            async {
                di.sysmon_system_attributes()
                    .await
                    .map(|a| println!("({} attrs)", a.len()))
            }
        );
    } // DeviceInfoClient drops here

    // ── ApplicationListing ────────────────────────────────────────────────────
    {
        let mut al = match ApplicationListingClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: ApplicationListingClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: ApplicationListing::installed_applications",
            success,
            failure,
            async {
                let apps = al.installed_applications().await?;
                if apps.is_empty() {
                    Err(idevice::IdeviceError::UnexpectedResponse(
                        "no applications returned".into(),
                    ))
                } else {
                    println!("({} apps)", apps.len());
                    Ok(())
                }
            }
        );
    }

    // ── ConditionInducer ──────────────────────────────────────────────────────
    {
        let mut ci = match ConditionInducerClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: ConditionInducerClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: ConditionInducer::available_conditions",
            success,
            failure,
            async {
                let groups = ci.available_conditions().await?;
                println!("({} groups)", groups.len());
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // ── ProcessControl ────────────────────────────────────────────────────────
    // Launch Settings.app, then immediately kill it.
    {
        let mut pc = match ProcessControlClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: ProcessControlClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: ProcessControl launch+kill com.apple.Preferences",
            success,
            failure,
            async {
                let pid = pc
                    .launch_app("com.apple.Preferences", None, None, false, true)
                    .await?;
                println!("(pid={pid})");
                pc.kill_app(pid).await?;
                Ok::<(), idevice::IdeviceError>(())
            }
        );
    }

    // Reconnect once after ProcessControl for the streaming services.
    // ProcessControl's kill_app may leave async death notifications in the
    // channel that interfere with streaming reads.
    drop(remote);
    let mut remote = match connect(provider).await {
        Some(c) => c,
        None => {
            println!("  dvt: cannot reconnect for streaming tests, skipping remainder");
            *failure += 1;
            return;
        }
    };

    // ── Sysmontap ─────────────────────────────────────────────────────────────
    // Query the actual attribute lists from DeviceInfo first — using hardcoded
    // names risks a mismatch that causes the device to silently drop samples.
    run_test!(
        "dvt: Sysmontap set_config + start + 1 sample + stop",
        success,
        failure,
        async {
            let (proc_attrs, sys_attrs) = {
                let mut di = DeviceInfoClient::new(&mut remote).await?;
                let p = di.sysmon_process_attributes().await?;
                let s = di.sysmon_system_attributes().await?;
                (p, s)
            };

            let mut sm = SysmontapClient::new(&mut remote).await?;
            let cfg = SysmontapConfig {
                interval_ms: 500,
                process_attributes: proc_attrs,
                system_attributes: sys_attrs,
            };
            sm.set_config(&cfg).await?;
            sm.start().await?;
            match tokio::time::timeout(STREAM_TIMEOUT, sm.next_sample()).await {
                Ok(Ok(s)) => {
                    println!(
                        "(processes={}, system={})",
                        s.processes.is_some(),
                        s.system.is_some()
                    );
                }
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(idevice::IdeviceError::UnexpectedResponse(
                        "timed out waiting for sysmontap sample".into(),
                    ));
                }
            }
            sm.stop().await
        }
    );

    // ── DVT Screenshot ────────────────────────────────────────────────────────
    {
        let mut sc = match ScreenshotClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: ScreenshotClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: Screenshot::take_screenshot",
            success,
            failure,
            async {
                let bytes = sc.take_screenshot().await?;
                if bytes.is_empty() {
                    Err(idevice::IdeviceError::UnexpectedResponse(
                        "screenshot was empty".into(),
                    ))
                } else {
                    println!("({} bytes)", bytes.len());
                    Ok(())
                }
            }
        );
    }

    // ── DVT LocationSimulation ────────────────────────────────────────────────
    {
        let mut ls = match LocationSimulationClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: LocationSimulationClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: LocationSimulation set (37.3318, -122.0312)",
            success,
            failure,
            async { ls.set(37.3318, -122.0312).await }
        );

        run_test!("dvt: LocationSimulation clear", success, failure, async {
            ls.clear().await
        });
    }

    // ── EnergyMonitor ─────────────────────────────────────────────────────────
    {
        let mut em = match EnergyMonitorClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: EnergyMonitorClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        // Sample PID 1 (launchd) which is always running
        run_test!(
            "dvt: EnergyMonitor start + stop (pid 1)",
            success,
            failure,
            async {
                em.start_sampling(&[1]).await?;
                tokio::time::sleep(Duration::from_millis(500)).await;
                em.stop_sampling(&[1]).await
            }
        );
    }

    // ── Graphics ──────────────────────────────────────────────────────────────
    {
        let mut gfx = match GraphicsClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: GraphicsClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: Graphics start + 1 sample + stop",
            success,
            failure,
            async {
                gfx.start_sampling(1.0).await?;
                match tokio::time::timeout(STREAM_TIMEOUT, gfx.sample()).await {
                    Ok(Ok(s)) => println!("(fps={:?})", s.fps),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(idevice::IdeviceError::UnexpectedResponse(
                            "timed out waiting for graphics sample".into(),
                        ));
                    }
                }
                gfx.stop_sampling().await
            }
        );
    }

    // ── DVT Notifications ─────────────────────────────────────────────────────
    {
        let mut notif = match NotificationsClient::new(&mut remote).await {
            Ok(c) => c,
            Err(e) => {
                println!("  dvt: NotificationsClient failed: {e}");
                *failure += 1;
                return;
            }
        };

        run_test!(
            "dvt: Notifications start + receive 1 (best-effort)",
            success,
            failure,
            async {
                notif.start_notifications().await?;
                // This may time out on a quiet device — treat timeout as pass
                // since we at least verified start_notifications works.
                match tokio::time::timeout(Duration::from_secs(5), notif.get_notification()).await {
                    Ok(Ok(_)) | Err(_) => {
                        notif.stop_notifications().await?;
                        Ok(())
                    }
                    Ok(Err(e)) => Err(e),
                }
            }
        );
    }
}
