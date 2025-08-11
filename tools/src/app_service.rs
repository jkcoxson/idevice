// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    core_device::AppServiceClient, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake,
    IdeviceService, RsdService,
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("remotexpc")
        .about("Get services from RemoteXPC")
        .arg(
            Arg::new("host")
                .long("host")
                .value_name("HOST")
                .help("IP address of the device"),
        )
        .arg(
            Arg::new("pairing_file")
                .long("pairing-file")
                .value_name("PATH")
                .help("Path to the pairing file"),
        )
        .arg(
            Arg::new("udid")
                .value_name("UDID")
                .help("UDID of the device (overrides host/pairing file)")
                .index(1),
        )
        .arg(
            Arg::new("tunneld")
                .long("tunneld")
                .help("Use tunneld")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(Command::new("list").about("Lists the images mounted on the device"))
        .subcommand(
            Command::new("launch")
                .about("Launch the app on the device")
                .arg(
                    Arg::new("bundle_id")
                        .required(true)
                        .help("The bundle ID to launch"),
                ),
        )
        .subcommand(Command::new("processes").about("List the processes running"))
        .subcommand(
            Command::new("uninstall").about("Uninstall an app").arg(
                Arg::new("bundle_id")
                    .required(true)
                    .help("The bundle ID to uninstall"),
            ),
        )
        .subcommand(
            Command::new("signal")
                .about("Send a signal to an app")
                .arg(Arg::new("pid").required(true).help("PID to send to"))
                .arg(Arg::new("signal").required(true).help("Signal to send")),
        )
        .subcommand(
            Command::new("icon")
                .about("Send a signal to an app")
                .arg(
                    Arg::new("bundle_id")
                        .required(true)
                        .help("The bundle ID to fetch"),
                )
                .arg(
                    Arg::new("path")
                        .required(true)
                        .help("The path to save the icon to"),
                )
                .arg(Arg::new("hw").required(false).help("The height and width"))
                .arg(Arg::new("scale").required(false).help("The scale")),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("debug_proxy - connect to the debug proxy and run commands");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let pairing_file = matches.get_one::<String>("pairing_file");
    let host = matches.get_one::<String>("host");

    let provider =
        match common::get_provider(udid, host, pairing_file, "debug-proxy-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.handshake.server_rsd_port;

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();

    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut asc = AppServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    if matches.subcommand_matches("list").is_some() {
        let apps = asc
            .list_apps(true, true, true, true, true)
            .await
            .expect("Failed to get apps");
        println!("{apps:#?}");
    } else if let Some(matches) = matches.subcommand_matches("launch") {
        let bundle_id: &String = match matches.get_one("bundle_id") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };

        let res = asc
            .launch_application(bundle_id, &[], false, false, None, None)
            .await
            .expect("no launch");

        println!("{res:#?}");
    } else if matches.subcommand_matches("processes").is_some() {
        let p = asc.list_processes().await.expect("no processes?");
        println!("{p:#?}");
    } else if let Some(matches) = matches.subcommand_matches("uninstall") {
        let bundle_id: &String = match matches.get_one("bundle_id") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };

        asc.uninstall_app(bundle_id).await.expect("no launch")
    } else if let Some(matches) = matches.subcommand_matches("signal") {
        let pid: u32 = match matches.get_one::<String>("pid") {
            Some(b) => b.parse().expect("failed to parse PID as u32"),
            None => {
                eprintln!("No bundle PID passed");
                return;
            }
        };
        let signal: u32 = match matches.get_one::<String>("signal") {
            Some(b) => b.parse().expect("failed to parse signal as u32"),
            None => {
                eprintln!("No bundle signal passed");
                return;
            }
        };

        let res = asc.send_signal(pid, signal).await.expect("no signal");
        println!("{res:#?}");
    } else if let Some(matches) = matches.subcommand_matches("icon") {
        let bundle_id: &String = match matches.get_one("bundle_id") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };
        let save_path: &String = match matches.get_one("path") {
            Some(b) => b,
            None => {
                eprintln!("No bundle ID passed");
                return;
            }
        };
        let hw: f32 = match matches.get_one::<String>("hw") {
            Some(b) => b.parse().expect("failed to parse PID as f32"),
            None => 1.0,
        };
        let scale: f32 = match matches.get_one::<String>("scale") {
            Some(b) => b.parse().expect("failed to parse signal as f32"),
            None => 1.0,
        };

        let res = asc
            .fetch_app_icon(bundle_id, hw, hw, scale, true)
            .await
            .expect("no signal");
        println!("{res:?}");
        tokio::fs::write(save_path, res.data)
            .await
            .expect("failed to save");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
