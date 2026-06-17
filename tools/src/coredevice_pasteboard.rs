// Jackson Coxson

use std::io::Read;

use idevice::{
    IdeviceService, RsdService,
    core_device::{
        DataInclusionPolicy, GENERAL_PASTEBOARD, PasteboardPayload, PasteboardServiceClient,
        PasteboardSnapshot,
    },
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    let policy_flag = || {
        JkFlag::new("policy")
            .with_help("Data inclusion policy: resolved, promised, matchsource, promisesecondary, threshold:<bytes>")
            .with_argument(JkArgument::new().required(true))
    };
    JkCommand::new()
        .help("Read and write the device pasteboard over CoreDevice")
        .with_flag(
            JkFlag::new("pasteboard")
                .with_help("Named pasteboard to use (default: general)")
                .with_argument(JkArgument::new().required(true)),
        )
        .with_subcommand(
            "paste",
            JkCommand::new()
                .help("Print the device pasteboard contents to stdout")
                .with_flag(
                    JkFlag::new("raw").with_help("Print the full snapshot plist instead of text"),
                ),
        )
        .with_subcommand(
            "copy",
            JkCommand::new()
                .help("Copy text onto the device pasteboard (reads stdin if no argument given)")
                .with_argument(JkArgument::new().with_help("text to copy")),
        )
        .with_subcommand(
            "items",
            JkCommand::new()
                .help("List the pasteboard items, their UTIs, and inline/promised state")
                .with_flag(policy_flag()),
        )
        .with_subcommand(
            "resolve",
            JkCommand::new()
                .help("Fetch the bytes of a promised item (RESOLVE -> DATA)")
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("item index (see `items`)"),
                )
                .with_argument(
                    JkArgument::new()
                        .required(true)
                        .with_help("UTI / type to resolve"),
                )
                .with_flag(
                    JkFlag::new("out")
                        .with_help("Write resolved bytes to this file (default: stdout)")
                        .with_argument(JkArgument::new().required(true)),
                )
                .with_flag(policy_flag()),
        )
        .with_subcommand(
            "watch",
            JkCommand::new()
                .help("Subscribe to pasteboard change notifications (AUTONOTIFY/PUSH) and stream them")
                .with_flag(policy_flag()),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let (sub_name, sub_args) = arguments
        .first_subcommand()
        .expect("no subcommand passed, pass -h for help");
    let mut sub_args = sub_args.clone();

    let pasteboard = arguments
        .get_flag::<String>("pasteboard")
        .or_else(|| sub_args.get_flag::<String>("pasteboard"))
        .unwrap_or_else(|| GENERAL_PASTEBOARD.to_string());

    let proxy = CoreDeviceProxy::connect(&*provider)
        .await
        .expect("no core device proxy");
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy.create_software_tunnel().expect("no software tunnel");
    let mut adapter = adapter.to_async_handle();
    let stream = adapter.connect(rsd_port).await.expect("no RSD connect");
    let mut handshake = RsdHandshake::new(stream).await.unwrap();

    let mut client = PasteboardServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no pasteboardservice");

    match sub_name.as_str() {
        "paste" => {
            let snapshot = client.get(&pasteboard).await.expect("PULL failed");
            if sub_args.has_flag("raw") {
                println!("{snapshot:#?}");
            } else if let Some(text) = snapshot.text() {
                print!("{text}");
            }
        }
        "copy" => {
            let text = sub_args.next_argument::<String>().unwrap_or_else(|| {
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .expect("failed to read stdin");
                buf
            });
            client
                .set_text(&text, &pasteboard)
                .await
                .expect("SET failed");
            eprintln!("copied {} byte(s) to the device pasteboard", text.len());
        }
        "items" => {
            let policy = parse_policy(&mut sub_args);
            let snapshot = client
                .get_with_policy(&pasteboard, policy)
                .await
                .expect("PULL failed");
            print_items(&snapshot);
        }
        "resolve" => {
            let index: u64 = sub_args
                .next_argument()
                .expect("missing item index, pass -h for help");
            let index = index as i64;
            let uti: String = sub_args
                .next_argument()
                .expect("missing UTI, pass -h for help");
            // RESOLVE must run on the same connection as the PULL that created
            // the snapshot, so pull before resolving.
            let pull_policy = if sub_args.has_flag("policy") {
                parse_policy(&mut sub_args)
            } else {
                DataInclusionPolicy::MatchSource
            };
            let snapshot = match client.get_with_policy(&pasteboard, pull_policy).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("PULL ({pull_policy:?}) failed: {e:?}");
                    return;
                }
            };
            eprintln!("pulled snapshot under {pull_policy:?}:");
            print_items(&snapshot);
            let reply = match client.resolve_raw(&pasteboard, index, &uti).await {
                Ok(r) => r,
                Err(e) => {
                    eprintln!(
                        "RESOLVE failed: {e:?}\n\
                         (the device aborts the channel when no promise resolver is wired \
                         for this item under {pull_policy:?} — try a different --policy)"
                    );
                    return;
                }
            };
            match reply
                .as_dictionary()
                .and_then(|d| d.get("data"))
                .and_then(|d| d.as_data())
            {
                Some(bytes) => match sub_args.get_flag::<String>("out") {
                    Some(path) => {
                        std::fs::write(&path, bytes).expect("failed to write output file");
                        eprintln!("wrote {} byte(s) to {path}", bytes.len());
                    }
                    None => {
                        use std::io::Write;
                        std::io::stdout().write_all(bytes).ok();
                    }
                },
                None => {
                    eprintln!("DATA reply carried no inline `data`; full reply structure:");
                    let mut buf = Vec::new();
                    plist::to_writer_xml(&mut buf, &reply).ok();
                    eprintln!("{}", String::from_utf8_lossy(&buf));
                }
            }
        }
        "watch" => {
            let policy = sub_args
                .has_flag("policy")
                .then(|| parse_policy(&mut sub_args));
            client
                .set_change_notifications(true, &pasteboard, policy)
                .await
                .expect("AUTONOTIFY failed");
            eprintln!("subscribed to '{pasteboard}'; waiting for changes (Ctrl-C to stop)…");
            loop {
                match client.recv_push().await {
                    Ok(push) => {
                        eprintln!("--- change ---");
                        print_summary(&push);
                    }
                    Err(e) => {
                        eprintln!("push stream ended: {e:?}");
                        break;
                    }
                }
            }
        }
        _ => unreachable!(),
    }
}

/// Parse the `--policy` flag, defaulting to `resolved`.
fn parse_policy(args: &mut CollectedArguments) -> DataInclusionPolicy {
    match args.get_flag::<String>("policy").as_deref() {
        None | Some("resolved") => DataInclusionPolicy::AllResolved,
        Some("promised") => DataInclusionPolicy::AllPromised,
        Some("matchsource") => DataInclusionPolicy::MatchSource,
        Some("promisesecondary") => DataInclusionPolicy::PromiseSecondary,
        Some(other) if other.starts_with("threshold:") => {
            let bytes = other["threshold:".len()..]
                .parse()
                .expect("threshold policy needs threshold:<bytes>");
            DataInclusionPolicy::Threshold(bytes)
        }
        Some(other) => panic!("unknown policy '{other}'"),
    }
}

/// Print one line per (item, UTI) showing inline byte count or promised size.
fn print_items(snapshot: &PasteboardSnapshot) {
    if snapshot.items.is_empty() {
        eprintln!("pasteboard is empty");
        return;
    }
    for item in &snapshot.items {
        for entry in &item.data {
            let state = match &entry.payload {
                PasteboardPayload::Inline(bytes) => format!("inline {} bytes", bytes.len()),
                PasteboardPayload::Promised { size: Some(s) } => format!("promised (size {s})"),
                PasteboardPayload::Promised { size: None } => "promised".to_string(),
                PasteboardPayload::Error(e) => format!("error: {e}"),
            };
            println!("[{}] {}: {state}", item.index, entry.uti);
        }
    }
}

/// Print the change-count and text of a snapshot, falling back to an item listing.
fn print_summary(snapshot: &PasteboardSnapshot) {
    if let Some(cc) = snapshot.change_count {
        eprintln!("changeCount: {cc}");
    }
    match snapshot.text() {
        Some(text) => println!("{text}"),
        None => print_items(snapshot),
    }
}
