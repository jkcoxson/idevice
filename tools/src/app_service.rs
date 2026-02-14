// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{AppServiceClient, OpenStdioSocketClient},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the RemoteXPC app service on the device")
        .with_subcommand("list", JkCommand::new().help("List apps on the device"))
        .with_subcommand(
            "launch",
            JkCommand::new()
                .help("Launch an app on the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("Bundle ID to launch")
                        .required(true),
                ),
        )
        .with_subcommand(
            "processes",
            JkCommand::new().help("List the processes running"),
        )
        .with_subcommand(
            "uninstall",
            JkCommand::new().help("Uninstall an app").with_argument(
                JkArgument::new()
                    .with_help("Bundle ID to uninstall")
                    .required(true),
            ),
        )
        .with_subcommand(
            "signal",
            JkCommand::new()
                .help("Uninstall an app")
                .with_argument(JkArgument::new().with_help("PID to signal").required(true))
                .with_argument(JkArgument::new().with_help("Signal to send").required(true)),
        )
        .with_subcommand(
            "icon",
            JkCommand::new()
                .help("Fetch an icon for an app")
                .with_argument(
                    JkArgument::new()
                        .with_help("Bundle ID for the app")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to save it to")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Height and width")
                        .required(true),
                )
                .with_argument(JkArgument::new().with_help("Scale").required(true)),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
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

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "list" => {
            let apps = asc
                .list_apps(true, true, true, true, true)
                .await
                .expect("Failed to get apps");
            println!("{apps:#?}");
        }
        "launch" => {
            let bundle_id: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };

            let mut stdio_conn = OpenStdioSocketClient::connect_rsd(&mut adapter, &mut handshake)
                .await
                .expect("no stdio");

            let stdio_uuid = stdio_conn.read_uuid().await.expect("no uuid");
            println!("stdio uuid: {stdio_uuid:?}");

            let res = asc
                .launch_application(bundle_id, &[], true, false, None, None, Some(stdio_uuid))
                .await
                .expect("no launch");

            println!("Launch response {res:#?}");

            let (mut remote_reader, mut remote_writer) = tokio::io::split(stdio_conn.inner);
            let mut local_stdin = tokio::io::stdin();
            let mut local_stdout = tokio::io::stdout();

            tokio::select! {
                // Task 1: Copy data from the remote process to local stdout
                res = tokio::io::copy(&mut remote_reader, &mut local_stdout) => {
                    if let Err(e) = res {
                        eprintln!("Error copying from remote to local: {}", e);
                    }
                    println!("\nRemote connection closed.");
                }
                // Task 2: Copy data from local stdin to the remote process
                res = tokio::io::copy(&mut local_stdin, &mut remote_writer) => {
                    if let Err(e) = res {
                        eprintln!("Error copying from local to remote: {}", e);
                    }
                    println!("\nLocal stdin closed.");
                }
            }
        }
        "processes" => {
            let p = asc.list_processes().await.expect("no processes?");
            println!("{p:#?}");
        }
        "uninstall" => {
            let bundle_id: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };

            asc.uninstall_app(bundle_id).await.expect("no launch")
        }
        "signal" => {
            let pid: u32 = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle PID passed");
                    return;
                }
            };
            let signal: u32 = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle signal passed");
                    return;
                }
            };

            let res = asc.send_signal(pid, signal).await.expect("no signal");
            println!("{res:#?}");
        }
        "icon" => {
            let bundle_id: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };
            let save_path: String = match sub_args.next_argument() {
                Some(b) => b,
                None => {
                    eprintln!("No bundle ID passed");
                    return;
                }
            };
            let hw: f32 = sub_args.next_argument().unwrap_or(1.0);
            let scale: f32 = sub_args.next_argument().unwrap_or(1.0);

            let res = asc
                .fetch_app_icon(bundle_id, hw, hw, scale, true)
                .await
                .expect("no signal");
            println!("{res:?}");
            tokio::fs::write(save_path, res.data)
                .await
                .expect("failed to save");
        }
        _ => unreachable!(),
    }
}
