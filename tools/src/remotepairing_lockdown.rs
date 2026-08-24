// Jackson Coxson
// The remotepairingdeviced control channel over USB lockdown

use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
use idevice::{
    IdeviceService,
    provider::IdeviceProvider,
    remote_pairing::{RemotePairingLockdownService, RpPairingFile},
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist_macro::pretty_print_plist;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Talk to the remotepairingdeviced control channel over USB lockdown")
        .with_subcommand(
            "info",
            JkCommand::new()
                .help("Print the device's control-channel handshake info")
                .with_flag(
                    JkFlag::new("raw")
                        .with_help("Keep deviceKVSData base64 instead of decoding it"),
                ),
        )
        .with_subcommand(
            "verify",
            JkCommand::new()
                .help("Pair-verify an existing RPPairing file against the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to the RPPairing file")
                        .required(true),
                )
                .with_argument(JkArgument::new().with_help("Hostname the file was created with")),
        )
        .with_subcommand(
            "pair",
            JkCommand::new()
                .help("Pair over USB, promptlessly, and write an RPPairing file")
                .with_argument(
                    JkArgument::new()
                        .with_help("Hostname to identify this computer")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("Path to save the pairing file")
                        .required(true),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments
        .first_subcommand()
        .expect("no subcommand passed, pass -h for help");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "info" => {
            let service = RemotePairingLockdownService::connect(&*provider)
                .await
                .expect("no remotepairingdeviced lockdown service");
            let mut client = service.into_client("idevice-rs-tools").expect("no socket");

            let handshake = client
                .attempt_pair_verify()
                .await
                .expect("handshake failed");
            let handshake = if sub_args.has_flag("raw") {
                handshake
            } else {
                decode_kvs_data(handshake)
            };
            println!("{}", pretty_print_plist(&handshake));
        }
        "verify" => {
            let path: String = sub_args
                .next_argument()
                .expect("pairing file path required");
            let hostname = sub_args
                .next_argument::<String>()
                .unwrap_or_else(|| "idevice-rs-tools".to_string());
            let mut pairing_file = RpPairingFile::read_from_file(&path)
                .await
                .expect("failed to read the pairing file");

            let service = RemotePairingLockdownService::connect(&*provider)
                .await
                .expect("no remotepairingdeviced lockdown service");
            let mut client = service.into_client(&hostname).expect("no socket");
            client
                .attempt_pair_verify()
                .await
                .expect("handshake failed");
            match client.validate_pairing(&mut pairing_file).await {
                Ok(()) => println!("the device recognizes this pairing record"),
                Err(e) => eprintln!("pair-verify failed: {e:?}"),
            }
        }
        "pair" => {
            let hostname: String = sub_args.next_argument().expect("hostname required");
            let output_path: String = sub_args.next_argument().expect("output path required");

            let service = RemotePairingLockdownService::connect(&*provider)
                .await
                .expect("no remotepairingdeviced lockdown service");
            let mut client = service.into_client(&hostname).expect("no socket");

            let mut pairing_file = RpPairingFile::generate(&hostname);
            // Over the already-trusted USB transport there is no Trust prompt,
            // so the PIN callback should never fire.
            match client
                .connect(&mut pairing_file, async || "000000".to_string())
                .await
            {
                Ok(()) => {
                    pairing_file
                        .write_to_file(&output_path)
                        .await
                        .expect("failed to save the pairing file");
                    println!("paired, saved to {output_path}");
                }
                Err(e) => eprintln!("pairing failed: {e:?}"),
            }
        }
        _ => unreachable!(),
    }
}

/// `peerDeviceInfo.deviceKVSData` is the device's key-value store as a base64
/// binary plist; decode it in place so the output is readable.
fn decode_kvs_data(handshake: plist::Value) -> plist::Value {
    let Some(mut root) = handshake.clone().into_dictionary() else {
        return handshake;
    };
    let Some(mut info) = root
        .get("peerDeviceInfo")
        .and_then(|v| v.as_dictionary())
        .cloned()
    else {
        return handshake;
    };
    let Some(decoded) = info
        .get("deviceKVSData")
        .and_then(|v| v.as_string())
        .and_then(|s| B64.decode(s).ok())
        .and_then(|bytes| plist::from_bytes::<plist::Value>(&bytes).ok())
    else {
        return handshake;
    };
    info.insert("deviceKVSData".into(), decoded);
    root.insert("peerDeviceInfo".into(), plist::Value::Dictionary(info));
    plist::Value::Dictionary(root)
}
