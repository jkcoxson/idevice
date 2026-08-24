// Jackson Coxson

use futures_util::TryStreamExt;
use idevice::{
    IdeviceService, RsdService,
    core_device::{AppServiceClient, OpenStdioSocketClient},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with the RemoteXPC app service on the device")
        .with_subcommand(
            "list",
            JkCommand::new().help("List apps on the device").with_flag(
                JkFlag::new("no-stream")
                    .with_help("Use the plain listapps feature instead of streamapplist"),
            ),
        )
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
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;

    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();

    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

    // Make the connection to RemoteXPC
    let mut handshake = RsdHandshake::new(stream).await.unwrap();
    let app_service_name = AppServiceClient::rsd_service_name();
    let supports_stream_apps = handshake
        .services
        .get(app_service_name.as_ref())
        .and_then(|service| service.features.as_ref())
        .is_some_and(|features| {
            features
                .iter()
                .any(|feature| feature == "com.apple.coredevice.feature.streamapplist")
        });

    let mut asc = AppServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no connect");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "list" => {
            // The streaming feature is preferred when advertised; --no-stream
            // forces the plain listapps invocation.
            let supports_stream_apps = supports_stream_apps && !sub_args.has_flag("no-stream");
            let apps = if supports_stream_apps {
                asc.stream_apps(true, true, true, true, true)
                    .try_collect()
                    .await
            } else {
                asc.list_apps(true, true, true, true, true).await
            }
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
        _ => unreachable!(),
    }
}
