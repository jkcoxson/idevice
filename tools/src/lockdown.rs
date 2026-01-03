// Jackson Coxson

use idevice::{IdeviceService, lockdown::LockdownClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkArgument, JkCommand};
use plist::Value;
use plist_macro::pretty_print_plist;

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Interact with lockdown")
        .with_subcommand(
            "get",
            JkCommand::new()
                .help("Gets a value from lockdown")
                .with_argument(JkArgument::new().with_help("The value to get"))
                .with_argument(JkArgument::new().with_help("The domain to get in")),
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
                )
                .with_argument(JkArgument::new().with_help("The domain to set in")),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut lockdown_client = LockdownClient::connect(&*provider)
        .await
        .expect("Unable to connect to lockdown");

    lockdown_client
        .start_session(&provider.get_pairing_file().await.expect("no pairing file"))
        .await
        .expect("no session");

    let (sub_name, sub_args) = arguments.first_subcommand().expect("No subcommand");
    let mut sub_args = sub_args.clone();

    match sub_name.as_str() {
        "get" => {
            let key: Option<String> = sub_args.next_argument();
            let domain: Option<String> = sub_args.next_argument();

            match lockdown_client
                .get_value(
                    match &key {
                        Some(k) => Some(k.as_str()),
                        None => None,
                    },
                    match &domain {
                        Some(d) => Some(d.as_str()),
                        None => None,
                    },
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
            let domain: Option<String> = sub_args.next_argument();

            let value = Value::String(value_str.clone());

            match lockdown_client
                .set_value(
                    key,
                    value,
                    match &domain {
                        Some(d) => Some(d.as_str()),
                        None => None,
                    },
                )
                .await
            {
                Ok(()) => println!("Successfully set"),
                Err(e) => eprintln!("Error setting value: {e}"),
            }
        }
        _ => unreachable!(),
    }
}
