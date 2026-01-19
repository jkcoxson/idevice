// Jackson Coxson

use idevice::{IdeviceService, lockdown::LockdownClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};
use plist::Value;
use plist_macro::pretty_print_plist;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with lockdown")
        .with_subcommand(
            "get",
            JkCommand::new()
                .help("Gets a value from lockdown")
                .with_argument(JkArgument::new().with_help("The value to get")),
        )
        .with_subcommand(
            "set",
            JkCommand::new()
                .help("Gets a value from lockdown")
                .with_argument(
                    JkArgument::new()
                        .with_help("The value to set")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("The value key to set")
                        .required(true),
                ),
        )
        .with_subcommand(
            "recovery",
            JkCommand::new().help("Tell the device to enter recovery mode"),
        )
        .with_flag(
            JkFlag::new("domain")
                .with_help("The domain to set/get in")
                .with_argument(JkArgument::new().required(true)),
        )
        .with_flag(JkFlag::new("no-session").with_help("Don't start a TLS session"))
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut lockdown_client = LockdownClient::connect(&*provider)
        .await
        .expect("Unable to connect to lockdown");

    if !arguments.has_flag("no-session") {
        lockdown_client
            .start_session(&provider.get_pairing_file().await.expect("no pairing file"))
            .await
            .expect("no session");
    }

    let domain: Option<String> = arguments.get_flag("domain");
    let domain = domain.as_deref();

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "get" => {
            let key: Option<String> = sub_args.next_argument();

            match lockdown_client
                .get_value(
                    match &key {
                        Some(k) => Some(k.as_str()),
                        None => None,
                    },
                    domain,
                )
                .await
            {
                Ok(value) => {
                    println!("{}", pretty_print_plist(&value));
                }
                Err(e) => {
                    eprintln!("Error getting value: {e}");
                }
            }
        }
        "set" => {
            let value_str: String = sub_args.next_argument().unwrap();
            let key: String = sub_args.next_argument().unwrap();

            let value = Value::String(value_str.clone());

            match lockdown_client.set_value(key, value, domain).await {
                Ok(()) => println!("Successfully set"),
                Err(e) => eprintln!("Error setting value: {e}"),
            }
        }
        "recovery" => lockdown_client
            .enter_recovery()
            .await
            .expect("Failed to enter recovery"),
        _ => unreachable!(),
    }
}
