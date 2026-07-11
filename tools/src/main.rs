// Jackson Coxson

use std::{
    net::{IpAddr, SocketAddr},
    str::FromStr,
};

use idevice::{
    pairing_file::PairingFile,
    provider::{IdeviceProvider, TcpProvider},
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection, UsbmuxdDevice},
};
use jkcli::{JkArgument, JkCommand, JkFlag};

mod activation;
mod afc;
mod amfi;
mod app_service;
mod application_listing;
mod bt_packet_logger;
mod companion_proxy;
mod condition_inducer;
mod coredevice_location;
mod coredevice_pasteboard;
mod coredevice_rotate;
mod coredevice_stream;
mod crash_logs;
mod debug_proxy;
mod device_info;
mod diagnostics;
mod diagnosticsservice;
mod dvt_packet_parser;
mod energy_monitor;
mod graphics;
mod heartbeat_client;
mod hid;
mod ideviceinfo;
mod ideviceinstaller;
mod installcoordination_proxy;
mod instproxy;
mod location_simulation;
mod lockdown;
mod misagent;
mod mobilebackup2;
mod mounter;
mod network_monitor;
mod notification_proxy_client;
mod notifications;
mod os_trace_relay;
mod pair;
mod pcapd;
mod preboard;
mod process_control;
mod remotexpc;
mod restore;
mod restore_service;
mod restore_usb;
mod rppairing;
mod screencapture;
mod screencaptureservice;
mod screenshot;
mod springboardservices;
mod syslog_relay;
mod sysmontap;
mod xctest;

mod pcap;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Set the base CLI
    let arguments = JkCommand::new()
        .with_flag(
            JkFlag::new("about")
                .with_help("Prints the about message")
                .with_short_circuit(|| {
                    eprintln!("idevice-rs-tools - Jackson Coxson\n");
                    eprintln!("Tools to manage and manipulate iOS devices");
                    eprintln!("Version {}", env!("CARGO_PKG_VERSION"));
                    eprintln!("https://github.com/jkcoxson/idevice");
                    eprintln!("\nOn to eternal perfection!");
                    std::process::exit(0);
                }),
        )
        .with_flag(
            JkFlag::new("version")
                .with_help("Prints the version")
                .with_short_circuit(|| {
                    println!("{}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }),
        )
        .with_flag(
            JkFlag::new("pairing-file")
                .with_argument(JkArgument::new().required(true))
                .with_help("The path to the pairing file to use"),
        )
        .with_flag(
            JkFlag::new("host")
                .with_argument(JkArgument::new().required(true))
                .with_help("The host to connect to"),
        )
        .with_flag(
            JkFlag::new("udid")
                .with_argument(JkArgument::new().required(true))
                .with_help("The UDID to use"),
        )
        .with_subcommand("activation", activation::register())
        .with_subcommand("afc", afc::register())
        .with_subcommand("amfi", amfi::register())
        .with_subcommand("app_service", app_service::register())
        .with_subcommand("bt_packet_logger", bt_packet_logger::register())
        .with_subcommand("companion_proxy", companion_proxy::register())
        .with_subcommand("crash_logs", crash_logs::register())
        .with_subcommand("debug_proxy", debug_proxy::register())
        .with_subcommand("diagnostics", diagnostics::register())
        .with_subcommand("diagnosticsservice", diagnosticsservice::register())
        .with_subcommand("dvt_packet_parser", dvt_packet_parser::register())
        .with_subcommand("heartbeat_client", heartbeat_client::register())
        .with_subcommand("hid", hid::register())
        .with_subcommand("ideviceinfo", ideviceinfo::register())
        .with_subcommand("ideviceinstaller", ideviceinstaller::register())
        .with_subcommand(
            "installcoordination_proxy",
            installcoordination_proxy::register(),
        )
        .with_subcommand("instproxy", instproxy::register())
        .with_subcommand("location_simulation", location_simulation::register())
        .with_subcommand("lockdown", lockdown::register())
        .with_subcommand("misagent", misagent::register())
        .with_subcommand("mobilebackup2", mobilebackup2::register())
        .with_subcommand("mounter", mounter::register())
        .with_subcommand("notifications", notifications::register())
        .with_subcommand("notification_proxy", notification_proxy_client::register())
        .with_subcommand("os_trace_relay", os_trace_relay::register())
        .with_subcommand("pair", pair::register())
        .with_subcommand("pcapd", pcapd::register())
        .with_subcommand("preboard", preboard::register())
        .with_subcommand("process_control", process_control::register())
        .with_subcommand("remotexpc", remotexpc::register())
        .with_subcommand("rppairing", rppairing::register())
        .with_subcommand("restore", restore::register())
        .with_subcommand("restore_service", restore_service::register())
        .with_subcommand("screenshot", screenshot::register())
        .with_subcommand("screencapture", screencapture::register())
        .with_subcommand("screencaptureservice", screencaptureservice::register())
        .with_subcommand("location", coredevice_location::register())
        .with_subcommand("pasteboard", coredevice_pasteboard::register())
        .with_subcommand("rotate", coredevice_rotate::register())
        .with_subcommand("springboard", springboardservices::register())
        .with_subcommand("syslog_relay", syslog_relay::register())
        .with_subcommand("energy_monitor", energy_monitor::register())
        .with_subcommand("graphics", graphics::register())
        .with_subcommand("device_info", device_info::register())
        .with_subcommand("application_listing", application_listing::register())
        .with_subcommand("condition_inducer", condition_inducer::register())
        .with_subcommand("network_monitor", network_monitor::register())
        .with_subcommand("sysmontap", sysmontap::register())
        .with_subcommand("xctest", xctest::register())
        .subcommand_required(true)
        .collect();

    let Some(arguments) = arguments else {
        return;
    };

    let udid = arguments.get_flag::<String>("udid");
    let host = arguments.get_flag::<String>("host");
    let pairing_file = arguments.get_flag::<String>("pairing-file");

    let (subcommand, sub_args) = match arguments.first_subcommand() {
        Some(s) => s,
        None => {
            eprintln!("No subcommand passed, pass -h for help");
            return;
        }
    };

    // I hate this
    if subcommand.as_str() == "restore" {
        let provider = get_provider(udid, host, pairing_file, "idevice-rs-tools")
            .await
            .ok();
        restore::main(sub_args, provider).await;
        return;
    }

    let provider = match get_provider(udid, host, pairing_file, "idevice-rs-tools").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    match subcommand.as_str() {
        "activation" => {
            activation::main(sub_args, provider).await;
        }
        "afc" => {
            afc::main(sub_args, provider).await;
        }
        "amfi" => {
            amfi::main(sub_args, provider).await;
        }
        "app_service" => {
            app_service::main(sub_args, provider).await;
        }
        "bt_packet_logger" => {
            bt_packet_logger::main(sub_args, provider).await;
        }
        "companion_proxy" => {
            companion_proxy::main(sub_args, provider).await;
        }
        "crash_logs" => {
            crash_logs::main(sub_args, provider).await;
        }
        "debug_proxy" => {
            debug_proxy::main(sub_args, provider).await;
        }
        "diagnostics" => {
            diagnostics::main(sub_args, provider).await;
        }
        "diagnosticsservice" => {
            diagnosticsservice::main(sub_args, provider).await;
        }
        "dvt_packet_parser" => {
            dvt_packet_parser::main(sub_args, provider).await;
        }
        "heartbeat_client" => {
            heartbeat_client::main(sub_args, provider).await;
        }
        "hid" => {
            hid::main(sub_args, provider).await;
        }
        "ideviceinfo" => {
            ideviceinfo::main(sub_args, provider).await;
        }
        "ideviceinstaller" => {
            ideviceinstaller::main(sub_args, provider).await;
        }
        "installcoordination_proxy" => {
            installcoordination_proxy::main(sub_args, provider).await;
        }
        "instproxy" => {
            instproxy::main(sub_args, provider).await;
        }
        "location_simulation" => {
            location_simulation::main(sub_args, provider).await;
        }
        "lockdown" => {
            lockdown::main(sub_args, provider).await;
        }
        "misagent" => {
            misagent::main(sub_args, provider).await;
        }
        "mobilebackup2" => {
            mobilebackup2::main(sub_args, provider).await;
        }
        "mounter" => {
            mounter::main(sub_args, provider).await;
        }
        "notifications" => {
            notifications::main(sub_args, provider).await;
        }
        "notification_proxy" => {
            notification_proxy_client::main(sub_args, provider).await;
        }
        "os_trace_relay" => {
            os_trace_relay::main(sub_args, provider).await;
        }
        "pair" => {
            pair::main(sub_args, provider).await;
        }
        "pcapd" => {
            pcapd::main(sub_args, provider).await;
        }
        "preboard" => {
            preboard::main(sub_args, provider).await;
        }
        "process_control" => {
            process_control::main(sub_args, provider).await;
        }
        "remotexpc" => {
            remotexpc::main(sub_args, provider).await;
        }
        "rppairing" => {
            rppairing::main(sub_args, provider).await;
        }
        "restore_service" => {
            restore_service::main(sub_args, provider).await;
        }
        "screenshot" => {
            screenshot::main(sub_args, provider).await;
        }
        "screencapture" => {
            screencapture::main(sub_args, provider).await;
        }
        "screencaptureservice" => {
            screencaptureservice::main(sub_args, provider).await;
        }
        "location" => {
            coredevice_location::main(sub_args, provider).await;
        }
        "pasteboard" => {
            coredevice_pasteboard::main(sub_args, provider).await;
        }
        "rotate" => {
            coredevice_rotate::main(sub_args, provider).await;
        }
        "springboard" => {
            springboardservices::main(sub_args, provider).await;
        }
        "syslog_relay" => {
            syslog_relay::main(sub_args, provider).await;
        }
        "energy_monitor" => {
            energy_monitor::main(sub_args, provider).await;
        }
        "graphics" => {
            graphics::main(sub_args, provider).await;
        }
        "device_info" => {
            device_info::main(sub_args, provider).await;
        }
        "application_listing" => {
            application_listing::main(sub_args, provider).await;
        }
        "condition_inducer" => {
            condition_inducer::main(sub_args, provider).await;
        }
        "network_monitor" => {
            network_monitor::main(sub_args, provider).await;
        }
        "sysmontap" => {
            sysmontap::main(sub_args, provider).await;
        }
        "xctest" => {
            xctest::main(sub_args, provider).await;
        }
        _ => unreachable!(),
    }
}

async fn get_provider(
    udid: Option<String>,
    host: Option<String>,
    pairing_file: Option<String>,
    label: &str,
) -> Result<Box<dyn IdeviceProvider>, String> {
    let provider: Box<dyn IdeviceProvider> = if let Some(udid) = udid {
        let mut usbmuxd = if let Ok(var) = std::env::var("USBMUXD_SOCKET_ADDRESS") {
            let socket = SocketAddr::from_str(&var).expect("Bad USBMUXD_SOCKET_ADDRESS");
            let socket = tokio::net::TcpStream::connect(socket)
                .await
                .expect("unable to connect to socket address");
            UsbmuxdConnection::new(Box::new(socket), 1)
        } else {
            UsbmuxdConnection::default()
                .await
                .expect("Unable to connect to usbmxud")
        };

        let dev = match usbmuxd.get_device(udid.as_str()).await {
            Ok(d) => d,
            Err(e) => {
                return Err(format!("Device not found: {e:?}"));
            }
        };
        Box::new(dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), label))
    } else if let Some(host) = host
        && let Some(pairing_file) = pairing_file
    {
        // split host and the optional scope_id (e.g., "fe80::1%3") if IPv6
        let (host_str, scope_id) = match host.rsplit_once('%') {
            Some((h, scope_id)) => {
                let scope_id = scope_id.parse::<u32>().ok();
                (h, scope_id)
            }
            None => (host.as_str(), None),
        };

        let host = match IpAddr::from_str(host_str) {
            Ok(h) => h,
            Err(e) => {
                return Err(format!("Invalid host: {e:?}"));
            }
        };
        let pairing_file = match PairingFile::read_from_file(pairing_file) {
            Ok(p) => p,
            Err(e) => {
                return Err(format!("Unable to read pairing file: {e:?}"));
            }
        };

        Box::new(TcpProvider {
            addr: host,
            scope_id,
            pairing_file,
            label: label.to_string(),
        })
    } else {
        let mut usbmuxd = if let Ok(var) = std::env::var("USBMUXD_SOCKET_ADDRESS") {
            let socket = SocketAddr::from_str(&var).expect("Bad USBMUXD_SOCKET_ADDRESS");
            let socket = tokio::net::TcpStream::connect(socket)
                .await
                .expect("unable to connect to socket address");
            UsbmuxdConnection::new(Box::new(socket), 1)
        } else {
            UsbmuxdConnection::default()
                .await
                .expect("Unable to connect to usbmxud")
        };
        let devs = match usbmuxd.get_devices().await {
            Ok(d) => d,
            Err(e) => {
                return Err(format!("Unable to get devices from usbmuxd: {e:?}"));
            }
        };
        let usb_devs: Vec<&UsbmuxdDevice> = devs
            .iter()
            .filter(|x| x.connection_type == Connection::Usb)
            .collect();

        if devs.is_empty() {
            return Err("No devices connected!".to_string());
        }

        let chosen_dev = if !usb_devs.is_empty() {
            usb_devs[0]
        } else {
            &devs[0]
        };
        Box::new(chosen_dev.to_provider(UsbmuxdAddr::from_env_var().unwrap(), label))
    };
    Ok(provider)
}
