// Jackson Coxson

use idevice::{
    IdeviceService, ReadWrite, RsdService,
    core_device_proxy::CoreDeviceProxy,
    cryptexd::{Cryptex1Assets, CryptexdClient, NonceDomain, install_ddi, installed_ddi},
    provider::{IdeviceProvider, RsdProvider},
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist_macro::pretty_print_dictionary;

pub fn register() -> JkCommand {
    let nonce_flags = |cmd: JkCommand| {
        cmd.with_flag(
            JkFlag::new("index")
                .with_help("Nonce domain index (default: 2)")
                .with_argument(JkArgument::new().required(true)),
        )
        .with_flag(
            JkFlag::new("handle")
                .with_help("Nonce domain handle, e.g. a build identity's Cryptex1,NonceDomain")
                .with_argument(JkArgument::new().required(true)),
        )
    };

    JkCommand::new()
        .help("Query and install cryptexes over cryptexd")
        .with_subcommand(
            "list",
            JkCommand::new().help("List the cryptexes installed on the device"),
        )
        .with_subcommand(
            "identifiers",
            JkCommand::new().help("Read the device's AppleImage4 personalization identifiers"),
        )
        .with_subcommand(
            "nonce",
            nonce_flags(JkCommand::new().help("Read a nonce domain's nonce")),
        )
        .with_subcommand(
            "roll-nonce",
            nonce_flags(JkCommand::new().help(
                "Roll a nonce domain's nonce - the domain is unusable until the device reboots",
            )),
        )
        .with_subcommand(
            "uninstall",
            JkCommand::new()
                .help("Uninstall an installed cryptex")
                .with_argument(
                    JkArgument::new()
                        .with_help("cryptex identifier, e.g. com.apple.MobileAsset.DDI")
                        .required(true),
                )
                .with_argument(JkArgument::new().with_help("version to scope the removal to")),
        )
        .with_subcommand(
            "install-ddi",
            JkCommand::new()
                .help("Personalize and install the DeveloperDiskImage cryptex")
                .with_argument(
                    JkArgument::new()
                        .with_help(
                            "unpacked DDI Restore directory, e.g. \
                             /Library/Developer/DeveloperDiskImages/iOS_DDI/Restore",
                        )
                        .required(true),
                )
                .with_flag(
                    JkFlag::new("force")
                        .with_help("Install even when a DDI cryptex is already installed"),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments
        .first_subcommand()
        .expect("no subcommand passed, pass -h for help");
    let mut sub_args = sub_args.clone();

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    if let Some(service) = handshake.services.get("com.apple.security.cryptexd.remote")
        && let Some(features) = &service.features
    {
        eprintln!("cryptexd features: {}", features.join(", "));
    }

    match sub_name.as_str() {
        "list" => {
            let installed = client(&mut adapter, &mut handshake)
                .await
                .copy_installed()
                .await
                .expect("copy-installed failed");
            if installed.is_empty() {
                println!("no cryptexes installed");
            }
            for cryptex in installed {
                println!("{} {}", cryptex.identifier, cryptex.version);
            }
        }
        "identifiers" => {
            let identifiers = client(&mut adapter, &mut handshake)
                .await
                .read_personalization_identifiers()
                .await
                .expect("read-personalization-id failed");
            println!("{}", pretty_print_dictionary(&identifiers));
        }
        "nonce" => {
            let domain = nonce_domain(&mut sub_args);
            let nonce = client(&mut adapter, &mut handshake)
                .await
                .get_nonce(domain)
                .await
                .expect("get-nonce failed");
            println!("structure ({} bytes): {}", nonce.len(), hex(&nonce));
            match idevice::cryptexd::unwrap_nonce(&nonce) {
                Ok(unwrapped) => println!("nonce ({} bytes): {}", unwrapped.len(), hex(&unwrapped)),
                Err(e) => eprintln!("could not unwrap the nonce: {e}"),
            }
        }
        "roll-nonce" => {
            let domain = nonce_domain(&mut sub_args);
            client(&mut adapter, &mut handshake)
                .await
                .roll_nonce(domain)
                .await
                .expect("roll-nonce failed");
            println!("rolled; this domain has no readable nonce until the device reboots");
        }
        "uninstall" => {
            let identifier: String = sub_args.next_argument().expect("identifier required");
            let version = sub_args.next_argument::<String>();
            client(&mut adapter, &mut handshake)
                .await
                .uninstall(&identifier, version.as_deref())
                .await
                .expect("uninstall failed");
            println!("uninstalled {identifier}");
        }
        "install-ddi" => {
            let restore_dir: String = sub_args
                .next_argument()
                .expect("restore directory required");
            let assets = Cryptex1Assets::load(&restore_dir)
                .await
                .expect("failed to load the DDI assets");
            println!(
                "loaded DDI assets: image {} bytes, trustcache {} bytes, info {} bytes, volume hash {} bytes",
                assets.image.len(),
                assets.trustcache.len(),
                assets.info.len(),
                assets.volumehash.len()
            );
            println!(
                "nonce domain handle {}",
                assets.nonce_domain().expect("no nonce domain")
            );

            if !sub_args.has_flag("force")
                && let Some(installed) = installed_ddi(&mut adapter, &mut handshake)
                    .await
                    .expect("copy-installed failed")
            {
                println!(
                    "{} {} is already installed (pass --force to install anyway)",
                    installed.identifier, installed.version
                );
                return;
            }

            let installed = install_ddi(&mut adapter, &mut handshake, &assets)
                .await
                .expect("install failed");
            println!("installed {} {}", installed.identifier, installed.version);
        }
        _ => unreachable!(),
    }
}

/// cryptexd serves one routine per connection, so every call gets a fresh one.
async fn client(
    adapter: &mut impl RsdProvider,
    handshake: &mut RsdHandshake,
) -> CryptexdClient<Box<dyn ReadWrite>> {
    CryptexdClient::connect_rsd(adapter, handshake)
        .await
        .expect("no cryptexd service")
}

fn nonce_domain(args: &mut CollectedArguments) -> NonceDomain {
    if let Some(handle) = args.get_flag::<u64>("handle") {
        NonceDomain::Handle(handle)
    } else if let Some(index) = args.get_flag::<u64>("index") {
        NonceDomain::Index(index)
    } else {
        NonceDomain::default()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
