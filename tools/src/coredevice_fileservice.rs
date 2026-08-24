// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{Domain, FileServiceClient},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    let domain_args = |cmd: JkCommand| {
        cmd.with_flag(
            JkFlag::new("domain")
                .with_help(
                    "appDataContainer (default), appGroupDataContainer, temporary, systemCrashLogs",
                )
                .with_argument(JkArgument::new().required(true)),
        )
        .with_flag(
            JkFlag::new("identifier")
                .with_help("Container identifier, i.e. the bundle or app-group ID")
                .with_argument(JkArgument::new().required(true)),
        )
    };

    JkCommand::new()
        .help("Browse and download files over the CoreDevice file service")
        .with_subcommand(
            "ls",
            domain_args(
                JkCommand::new()
                    .help("List a directory")
                    .with_argument(JkArgument::new().with_help("path (default: .)")),
            ),
        )
        .with_subcommand(
            "cat",
            domain_args(
                JkCommand::new()
                    .help("Download a file")
                    .with_argument(JkArgument::new().with_help("path").required(true))
                    .with_flag(
                        JkFlag::new("out")
                            .with_help("Write to this file instead of stdout")
                            .with_argument(JkArgument::new().required(true)),
                    ),
            ),
        )
        .with_subcommand(
            "touch",
            domain_args(
                JkCommand::new()
                    .help("Create an empty file")
                    .with_argument(JkArgument::new().with_help("path").required(true)),
            ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments
        .first_subcommand()
        .expect("no subcommand passed, pass -h for help");
    let mut sub_args = sub_args.clone();

    let domain_name = sub_args
        .get_flag::<String>("domain")
        .unwrap_or_else(|| "appDataContainer".to_string());
    let Some(domain) = Domain::from_name(&domain_name) else {
        eprintln!("unknown domain {domain_name:?}");
        return;
    };
    let identifier = sub_args
        .get_flag::<String>("identifier")
        .unwrap_or_default();

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    if let Some(service) = handshake
        .services
        .get("com.apple.coredevice.fileservice.control")
        && let Some(features) = &service.features
    {
        eprintln!("fileservice features: {}", features.join(", "));
    }
    let data_port = handshake
        .services
        .get("com.apple.coredevice.fileservice.data")
        .map(|s| s.port);

    let mut client = FileServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no file service");
    let session = client
        .create_session(domain, &identifier)
        .await
        .expect("failed to create a session");
    eprintln!("session {session}");

    match sub_name.as_str() {
        "ls" => {
            let path = sub_args
                .next_argument::<String>()
                .unwrap_or_else(|| ".".to_string());
            let entries = client
                .retrieve_directory_list(&path)
                .await
                .expect("failed to list the directory");
            for entry in entries {
                println!("{entry}");
            }
        }
        "cat" => {
            let path: String = sub_args.next_argument().expect("path required");
            let Some(data_port) = data_port else {
                eprintln!("the device doesn't advertise 'com.apple.coredevice.fileservice.data'");
                return;
            };
            let contents = client
                .retrieve_file(&path, async || {
                    adapter
                        .connect(data_port)
                        .await
                        .map(|s| Box::new(s) as Box<dyn idevice::ReadWrite>)
                        .map_err(idevice::IdeviceError::from)
                })
                .await
                .expect("failed to retrieve the file");

            match sub_args.get_flag::<String>("out") {
                Some(out) => {
                    tokio::fs::write(&out, &contents)
                        .await
                        .expect("failed to write the file");
                    eprintln!("wrote {} bytes to {out}", contents.len());
                }
                None => {
                    use tokio::io::AsyncWriteExt;
                    tokio::io::stdout()
                        .write_all(&contents)
                        .await
                        .expect("failed to write to stdout");
                }
            }
        }
        "touch" => {
            let path: String = sub_args.next_argument().expect("path required");
            client
                .propose_empty_file(&path, 0o644, 501, 501, 0, 0)
                .await
                .expect("failed to propose the file");
            eprintln!("created {path}");
        }
        _ => unreachable!(),
    }
}
