// Jackson Coxson
// idevice Rust implementation of libimobiledevice's ideviceinfo

use idevice::{IdeviceService, lockdown::LockdownClient, provider::IdeviceProvider};
use jkcli::{CollectedArguments, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new().help("ideviceinfo - get information from the idevice. Reimplementation of libimobiledevice's binary.")
}

pub async fn main(_arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    let mut lockdown_client = match LockdownClient::connect(&*provider).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Unable to connect to lockdown: {e:?}");
            return;
        }
    };

    println!(
        "{:?}",
        lockdown_client
            .get_value(Some("ProductVersion"), None)
            .await
    );

    println!(
        "{:?}",
        lockdown_client
            .start_session(
                &provider
                    .get_pairing_file()
                    .await
                    .expect("failed to get pairing file")
            )
            .await
    );
    println!("{:?}", lockdown_client.idevice.get_type().await.unwrap());
    println!("{:#?}", lockdown_client.get_value(None, None).await);
}
