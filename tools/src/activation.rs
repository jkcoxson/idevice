// Jackson Coxson

use idevice::{
    IdeviceService, lockdown::LockdownClient, mobileactivationd::MobileActivationdClient,
    provider::IdeviceProvider,
};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage activation status on an iOS device")
        .with_subcommand("state", JkCommand::new().help("Gets the activation state"))
        .with_subcommand(
            "deactivate",
            JkCommand::new().help("Deactivates the device"),
        )
        .subcommand_required(true)
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let activation_client = MobileActivationdClient::new(&*provider);
    let mut lc = LockdownClient::connect(&*provider)
        .await
        .expect("no lockdown");
    lc.start_session(&provider.get_pairing_file().await.unwrap())
        .await
        .expect("no TLS");
    let udid = lc
        .get_value(Some("UniqueDeviceID"), None)
        .await
        .expect("no udid")
        .into_string()
        .unwrap();

    let (sub_name, _sub_args) = arguments.first_subcommand().expect("no subarg passed");

    match sub_name.as_str() {
        "state" => {
            let s = activation_client.state().await.expect("no state");
            println!("Activation State: {s}");
        }
        "deactivate" => {
            println!("CAUTION: You are deactivating {udid}, press enter to continue.");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).ok();
            activation_client.deactivate().await.expect("no deactivate");
        }
        _ => unreachable!(),
    }
}
