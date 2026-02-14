// Jackson Coxson

use idevice::{
    IdeviceService,
    crashreportcopymobile::{CrashReportCopyMobileClient, flush_reports},
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage crash logs")
        .with_subcommand(
            "list",
            JkCommand::new()
                .help("List crash logs in the directory")
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to list in")
                        .required(true),
                ),
        )
        .with_subcommand(
            "flush",
            JkCommand::new().help("Flushes reports to the directory"),
        )
        .with_subcommand(
            "pull",
            JkCommand::new()
                .help("Check the capabilities")
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to the log to pull")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to save the log to")
                        .required(true),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut crash_client = CrashReportCopyMobileClient::connect(&*provider)
        .await
        .expect("Unable to connect to misagent");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No sub command passed");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "list" => {
            let dir_path: Option<String> = sub_args.next_argument();
            let res = crash_client
                .ls(match &dir_path {
                    Some(d) => Some(d.as_str()),
                    None => None,
                })
                .await
                .expect("Failed to read dir");
            println!("{res:#?}");
        }
        "flush" => {
            flush_reports(&*provider).await.expect("Failed to flush");
        }
        "pull" => {
            let path = sub_args.next_argument::<String>().expect("No path passed");
            let save = sub_args.next_argument::<String>().expect("No path passed");

            let res = crash_client.pull(path).await.expect("Failed to pull log");
            tokio::fs::write(save, res)
                .await
                .expect("Failed to write to file");
        }
        _ => unreachable!(),
    }
}
