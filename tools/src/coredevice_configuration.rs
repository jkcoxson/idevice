// Jackson Coxson

use idevice::{
    IdeviceService, RsdService,
    core_device::{ConfigurationServiceClient, UserInterfaceStyle},
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand, JkFlag};

pub fn register() -> JkCommand {
    let toggle = |help: &'static str| {
        JkCommand::new()
            .help(help)
            .with_argument(JkArgument::new().with_help("on or off; omit to read the current value"))
    };

    JkCommand::new()
        .help("Read and write appearance and accessibility knobs over CoreDevice")
        .with_subcommand(
            "appearance",
            JkCommand::new()
                .help("Get or set the light/dark appearance")
                .with_argument(
                    JkArgument::new().with_help("dark or light; omit to read the current style"),
                ),
        )
        .with_subcommand(
            "opacity",
            JkCommand::new()
                .help("Set the system liquid-glass opacity")
                .with_argument(
                    JkArgument::new()
                        .with_help("opacity, 0.0 to 1.0")
                        .required(true),
                ),
        )
        .with_subcommand(
            "color-filter",
            JkCommand::new()
                .help("Get or set the accessibility color filter")
                .with_argument(JkArgument::new().with_help("on or off; omit to read the state"))
                .with_argument(JkArgument::new().with_help("filter preset, e.g. Protanopia"))
                .with_argument(JkArgument::new().with_help("intensity, 0.0 to 1.0")),
        )
        .with_subcommand(
            "text-size",
            JkCommand::new()
                .help("Get or set the dynamic type size")
                .with_argument(
                    JkArgument::new().with_help("size name, e.g. large; omit to read it"),
                ),
        )
        .with_subcommand("reduce-motion", toggle("Get or set Reduce Motion"))
        .with_subcommand(
            "increase-contrast",
            JkCommand::new()
                .help("Set Increase Contrast (the device exposes no getter)")
                .with_argument(JkArgument::new().with_help("on or off").required(true)),
        )
        .with_subcommand(
            "show-borders",
            toggle("Get or set the layout-debug borders overlay"),
        )
        .with_subcommand(
            "reduce-transparency",
            toggle("Get or set Reduce Transparency"),
        )
        .with_flag(JkFlag::new("raw").with_help("Print raw values instead of a friendly summary"))
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

    let mut client = ConfigurationServiceClient::connect_rsd(&mut adapter, &mut handshake)
        .await
        .expect("no configuration service");

    match sub_name.as_str() {
        "appearance" => match sub_args.next_argument::<String>().as_deref() {
            None => {
                let style = client
                    .get_user_interface_style()
                    .await
                    .expect("failed to read the appearance");
                println!("{}", style.as_str());
            }
            Some("dark") => set_appearance(&mut client, UserInterfaceStyle::Dark).await,
            Some("light") => set_appearance(&mut client, UserInterfaceStyle::Light).await,
            Some(other) => eprintln!("appearance must be 'dark' or 'light', got {other:?}"),
        },
        "opacity" => {
            let opacity: f32 = sub_args.next_argument().expect("opacity required");
            client
                .set_liquid_glass_opacity(opacity)
                .await
                .expect("failed to set the opacity");
            println!("opacity set to {opacity}");
        }
        "color-filter" => match sub_args.next_argument::<String>() {
            None => {
                let filter = client
                    .get_color_filter()
                    .await
                    .expect("failed to read the color filter");
                println!("{filter:#?}");
            }
            Some(state) => {
                let Some(enabled) = parse_bool(&state) else {
                    eprintln!("expected 'on' or 'off', got {state:?}");
                    return;
                };
                let filter_type = sub_args.next_argument::<String>();
                let intensity = sub_args.next_argument::<f32>();
                client
                    .set_color_filter(enabled, filter_type.as_deref(), intensity)
                    .await
                    .expect("failed to set the color filter");
                println!(
                    "color filter {}",
                    if enabled { "enabled" } else { "disabled" }
                );
            }
        },
        "text-size" => match sub_args.next_argument::<String>() {
            None => {
                let size = client
                    .get_device_text_size()
                    .await
                    .expect("failed to read the text size");
                println!("{size}");
            }
            Some(size) => {
                client
                    .set_device_text_size(&size)
                    .await
                    .expect("failed to set the text size");
                println!("text size set to {size}");
            }
        },
        "reduce-motion" => match sub_args.next_argument::<String>() {
            None => println!(
                "{}",
                client
                    .get_reduce_motion()
                    .await
                    .expect("failed to read reduce motion")
            ),
            Some(state) => {
                let Some(enabled) = parse_bool(&state) else {
                    eprintln!("expected 'on' or 'off', got {state:?}");
                    return;
                };
                client
                    .set_reduce_motion(enabled)
                    .await
                    .expect("failed to set reduce motion");
                println!("reduce motion {enabled}");
            }
        },
        "increase-contrast" => {
            let state: String = sub_args.next_argument().expect("on or off required");
            let Some(enabled) = parse_bool(&state) else {
                eprintln!("expected 'on' or 'off', got {state:?}");
                return;
            };
            client
                .set_increase_contrast(enabled)
                .await
                .expect("failed to set increase contrast");
            println!("increase contrast {enabled}");
        }
        "show-borders" => match sub_args.next_argument::<String>() {
            None => println!(
                "{}",
                client
                    .get_show_borders()
                    .await
                    .expect("failed to read show borders")
            ),
            Some(state) => {
                let Some(enabled) = parse_bool(&state) else {
                    eprintln!("expected 'on' or 'off', got {state:?}");
                    return;
                };
                client
                    .set_show_borders(enabled)
                    .await
                    .expect("failed to set show borders");
                println!("show borders {enabled}");
            }
        },
        "reduce-transparency" => match sub_args.next_argument::<String>() {
            None => println!(
                "{}",
                client
                    .get_reduce_transparency()
                    .await
                    .expect("failed to read reduce transparency")
            ),
            Some(state) => {
                let Some(enabled) = parse_bool(&state) else {
                    eprintln!("expected 'on' or 'off', got {state:?}");
                    return;
                };
                client
                    .set_reduce_transparency(enabled)
                    .await
                    .expect("failed to set reduce transparency");
                println!("reduce transparency {enabled}");
            }
        },
        _ => unreachable!(),
    }
}

async fn set_appearance(
    client: &mut ConfigurationServiceClient<Box<dyn idevice::ReadWrite>>,
    style: UserInterfaceStyle,
) {
    client
        .set_user_interface_style(style)
        .await
        .expect("failed to set the appearance");
    println!("appearance set to {}", style.as_str());
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}
