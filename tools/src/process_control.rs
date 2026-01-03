// Jackson Coxson

use idevice::provider::IdeviceProvider;
use idevice::services::lockdown::LockdownClient;
use idevice::{IdeviceService, RsdService, core_device_proxy::CoreDeviceProxy, rsd::RsdHandshake};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Launch an app with process control")
        .with_argument(
            JkArgument::new()
                .required(true)
                .with_help("The bundle ID to launch"),
        )
}

pub async fn main(arguments: &CollectedArguments, provider: Box<dyn IdeviceProvider>) {
    tracing_subscriber::fmt::init();

    let mut arguments = arguments.clone();

    let bundle_id: String = arguments.next_argument().expect("No bundle ID specified");

    let mut rs_client_opt: Option<
        idevice::dvt::remote_server::RemoteServerClient<Box<dyn idevice::ReadWrite>>,
    > = None;

    if let Ok(proxy) = CoreDeviceProxy::connect(&*provider).await {
        let rsd_port = proxy.handshake.server_rsd_port;
        let adapter = proxy.create_software_tunnel().expect("no software tunnel");
        let mut adapter = adapter.to_async_handle();
        let stream = adapter.connect(rsd_port).await.expect("no RSD connect");

        // Make the connection to RemoteXPC (iOS 17+)
        let mut handshake = RsdHandshake::new(stream).await.unwrap();
        let mut rs_client = idevice::dvt::remote_server::RemoteServerClient::connect_rsd(
            &mut adapter,
            &mut handshake,
        )
        .await
        .expect("no connect");
        rs_client.read_message(0).await.expect("no read??");
        rs_client_opt = Some(rs_client);
    }

    let mut rs_client = if let Some(c) = rs_client_opt {
        c
    } else {
        // Read iOS version to decide whether we can fallback to remoteserver
        let mut lockdown = LockdownClient::connect(&*provider)
            .await
            .expect("lockdown connect failed");
        lockdown
            .start_session(&provider.get_pairing_file().await.expect("pairing file"))
            .await
            .expect("lockdown start_session failed");
        let pv = lockdown
            .get_value(Some("ProductVersion"), None)
            .await
            .ok()
            .and_then(|v| v.as_string().map(|s| s.to_string()))
            .unwrap_or_default();
        let major: u32 = pv
            .split('.')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        if major >= 17 {
            // iOS 17+ with no CoreDeviceProxy: do not attempt remoteserver (would return InvalidService)
            panic!("iOS {pv} detected and CoreDeviceProxy unavailable. RemoteXPC tunnel required.");
        }

        // iOS 16 and earlier: fallback to Lockdown remoteserver (or DVTSecureSocketProxy)
        idevice::dvt::remote_server::RemoteServerClient::connect(&*provider)
            .await
            .expect("failed to connect to Instruments Remote Server over Lockdown (iOS16-). Ensure Developer Disk Image is mounted.")
    };

    // Note: On both transports, protocol requires reading the initial message on root channel (0)
    rs_client.read_message(0).await.expect("no read??");
    let mut pc_client = idevice::dvt::process_control::ProcessControlClient::new(&mut rs_client)
        .await
        .unwrap();

    let pid = pc_client
        .launch_app(bundle_id, None, None, false, false)
        .await
        .expect("no launch??");
    pc_client
        .disable_memory_limit(pid)
        .await
        .expect("no disable??");
    println!("PID: {pid}");
}
