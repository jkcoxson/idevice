// Jackson Coxson

use idevice::{
    IdeviceService, notification_proxy::NotificationProxyClient, provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Notification proxy")
        .with_subcommand(
            "observe",
            JkCommand::new()
                .help("Observe notifications from the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("The notification ID to observe")
                        .required(true),
                ),
        )
        .with_subcommand(
            "post",
            JkCommand::new()
                .help("Post a notification to the device")
                .with_argument(
                    JkArgument::new()
                        .with_help("The notification ID to post")
                        .required(true),
                ),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut client = NotificationProxyClient::connect(&*provider)
        .await
        .expect("Unable to connect to notification proxy");

    let (subcommand, sub_args) = arguments.first_subcommand().unwrap();
    let mut sub_args = sub_args.clone();

    match subcommand.as_str() {
        "observe" => {
            let input: String = sub_args
                .next_argument::<String>()
                .expect("No notification ID passed");

            let notifications: Vec<&str> = input.split_whitespace().collect();
            client
                .observe_notifications(&notifications)
                .await
                .expect("Failed to observe notifications");

            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        println!("\nShutdown signal received, exiting.");
                        break;
                    }

                    result = client.receive_notification() => {
                        match result {
                            Ok(notif) => println!("Received notification: {}", notif),
                            Err(e) => {
                                eprintln!("Failed to receive notification: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        }
        "post" => {
            let notification: String = sub_args
                .next_argument::<String>()
                .expect("No notification ID passed");

            client
                .post_notification(&notification)
                .await
                .expect("Failed to post notification");
        }
        _ => unreachable!(),
    }
}
