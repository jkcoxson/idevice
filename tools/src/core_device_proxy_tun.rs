// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    core_device_proxy::{self},
    IdeviceService,
};
use tun_rs::AbstractDevice;

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();
    let matches = Command::new("core_device_proxy_tun")
        .about("Start a tunnel")
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
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("help")
                .short('h')
                .long("help")
                .help("Show this help message")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("core_device_proxy - Start a lockdown tunnel on the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider =
        match common::get_provider(udid, host, pairing_file, "core_device_proxy-jkcoxson").await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return;
            }
        };

    let mut tun_proxy = core_device_proxy::CoreDeviceProxy::connect(&*provider)
        .await
        .expect("Unable to connect");
    let response = tun_proxy.establish_tunnel().await.unwrap();

    let dev = tun_rs::create(&tun_rs::Configuration::default()).unwrap();
    dev.add_address_v6(response.client_parameters.address.parse().unwrap(), 32)
        .unwrap();
    dev.set_mtu(response.client_parameters.mtu).unwrap();
    dev.set_network_address(
        response.client_parameters.address,
        response.client_parameters.netmask.parse().unwrap(),
        Some(response.server_address.parse().unwrap()),
    )
    .unwrap();

    let async_dev = tun_rs::AsyncDevice::new(dev).unwrap();
    async_dev.enabled(true).unwrap();
    println!("-----------------------------");
    println!("tun device created: {:?}", async_dev.name());
    println!("server address: {}", response.server_address);
    println!("rsd port: {}", response.server_rsd_port);
    println!("-----------------------------");

    let mut buf = vec![0; 1500];
    loop {
        tokio::select! {
            Ok(len) = async_dev.recv(&mut buf) => {
                println!("tun pkt: {:?}", &buf[..len]);
                tun_proxy.send(&buf[..len]).await.unwrap();
            }
            Ok(res) = tun_proxy.recv() => {
                println!("dev pkt: {:?}", &res);
                async_dev.send(&res).await.unwrap();
            }
        }
    }
}
