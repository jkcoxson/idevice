// Jackson Coxson

use idevice::{
    IdeviceService,
    lockdown::LockdownClient,
    provider::IdeviceProvider,
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection},
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Manage files in the AFC jail of a device")
        .with_argument(JkArgument::new().with_help("A UDID to override and pair with"))
        .with_flag(
            JkFlag::new("name")
                .with_help("The host name to report to the device")
                .with_argument(JkArgument::new().required(true))
                .with_short("n"),
        )
}

pub async fn main(arguments: &CollectedArguments, _provider: Box<dyn IdeviceProvider>) {
    let mut arguments = arguments.clone();
    let udid: Option<String> = arguments.next_argument();

    let mut u = UsbmuxdConnection::default()
        .await
        .expect("Failed to connect to usbmuxd");
    let dev = match udid {
        Some(udid) => u
            .get_device(udid.as_str())
            .await
            .expect("Failed to get device with specific udid"),
        None => u
            .get_devices()
            .await
            .expect("Failed to get devices")
            .into_iter()
            .find(|x| x.connection_type == Connection::Usb)
            .expect("No devices connected via USB"),
    };
    let provider = dev.to_provider(UsbmuxdAddr::default(), "pair-jkcoxson");

    let mut lockdown_client = match LockdownClient::connect(&provider).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Unable to connect to lockdown: {e:?}");
            return;
        }
    };
    let id = uuid::Uuid::new_v4().to_string().to_uppercase();

    let name = arguments.get_flag::<String>("name");
    let name = name.as_deref();

    let mut pairing_file = lockdown_client
        .pair(id, u.get_buid().await.unwrap(), name)
        .await
        .expect("Failed to pair");

    // Test the pairing file
    lockdown_client
        .start_session(&pairing_file)
        .await
        .expect("Pairing file test failed");

    // Add the UDID (jitterbug spec)
    pairing_file.udid = Some(dev.udid.clone());
    let pairing_file = pairing_file.serialize().expect("failed to serialize");

    println!("{}", String::from_utf8(pairing_file.clone()).unwrap());

    // Save with usbmuxd
    u.save_pair_record(&dev.udid, pairing_file)
        .await
        .expect("no save");
}
