// Jackson Coxson

use clap::{Arg, Command};
use idevice::{
    crashreportcopymobile::{flush_reports, CrashReportCopyMobileClient},
    IdeviceService,
};

mod common;

#[tokio::main]
async fn main() {
    env_logger::init();

    let matches = Command::new("crash_logs")
        .about("Manage crash logs")
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
                .help("UDID of the device (overrides host/pairing file)"),
        )
        .arg(
            Arg::new("about")
                .long("about")
                .help("Show about information")
                .action(clap::ArgAction::SetTrue),
        )
        .subcommand(Command::new("list").about("Lists the items in the directory"))
        .subcommand(Command::new("flush").about("Flushes reports to the directory"))
        .subcommand(
            Command::new("pull")
                .about("Pulls a log")
                .arg(Arg::new("path").required(true).index(1))
                .arg(Arg::new("save").required(true).index(2))
                .arg(Arg::new("dir").required(false).index(3)),
        )
        .get_matches();

    if matches.get_flag("about") {
        println!("crash_logs - manage crash logs on the device");
        println!("Copyright (c) 2025 Jackson Coxson");
        return;
    }

    let udid = matches.get_one::<String>("udid");
    let host = matches.get_one::<String>("host");
    let pairing_file = matches.get_one::<String>("pairing_file");

    let provider = match common::get_provider(udid, host, pairing_file, "afc-jkcoxson").await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return;
        }
    };
    let mut crash_client = CrashReportCopyMobileClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    if let Some(matches) = matches.subcommand_matches("list") {
        let dir_path: Option<&String> = matches.get_one("dir");
        let res = crash_client
            .ls(dir_path.map(|x| x.as_str()))
            .await
            .expect("Failed to read dir");
        println!("{res:#?}");
    } else if matches.subcommand_matches("flush").is_some() {
        flush_reports(&*provider).await.expect("Failed to flush");
    } else if let Some(matches) = matches.subcommand_matches("pull") {
        let path = matches.get_one::<String>("path").expect("No path passed");
        let save = matches.get_one::<String>("save").expect("No path passed");

        let res = crash_client.pull(path).await.expect("Failed to pull log");
        tokio::fs::write(save, res)
            .await
            .expect("Failed to write to file");
    } else {
        eprintln!("Invalid usage, pass -h for help");
    }
}
