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
mod bt_packet_logger;
mod companion_proxy;
mod crash_logs;
mod debug_proxy;
mod diagnostics;
mod diagnosticsservice;
mod dvt_packet_parser;
mod heartbeat_client;
mod ideviceinfo;
mod ideviceinstaller;
mod installcoordination_proxy;
mod instproxy;
mod location_simulation;
mod lockdown;
mod misagent;
mod mobilebackup2;
mod mounter;
mod notifications;
mod notification_proxy_client;
mod os_trace_relay;
mod pair;
mod pcapd;
mod preboard;
mod process_control;
mod remotexpc;
mod restore_service;
mod screenshot;
mod springboardservices;
mod syslog_relay;

mod pcap;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Set the base CLI
    let arguments = JkCommand::new()
        .with_flag(
            JkFlag::new("about")
                .with_help("Prints the about message")
                .with_short_curcuit(|| {
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
                .with_short_curcuit(|| {
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
        .with_subcommand("restore_service", restore_service::register())
        .with_subcommand("screenshot", screenshot::register())
        .with_subcommand("springboard", springboardservices::register())
        .with_subcommand("syslog_relay", syslog_relay::register())
        .subcommand_required(true)
        .collect()
        .expect("Failed to collect CLI args");

    let udid = arguments.get_flag::<String>("udid");
    let host = arguments.get_flag::<String>("host");
    let pairing_file = arguments.get_flag::<String>("pairing-file");

    let provider = match get_provider(udid, host, pairing_file, "idevice-rs-tools").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };

    let (subcommand, sub_args) = match arguments.first_subcommand() {
        Some(s) => s,
        None => {
            eprintln!("No subcommand passed, pass -h for help");
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
        "restore_service" => {
            restore_service::main(sub_args, provider).await;
        }
        "screenshot" => {
            screenshot::main(sub_args, provider).await;
        }
        "springboard" => {
            springboardservices::main(sub_args, provider).await;
        }
        "syslog_relay" => {
            syslog_relay::main(sub_args, provider).await;
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
        let host = match IpAddr::from_str(host.as_str()) {
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
