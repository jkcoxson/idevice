// Jackson Coxson

use idevice::{
    IdeviceService, ReadWrite, RsdService,
    core_device::hid::{
        ButtonState, DigitizerEdge, DigitizerEventType, DigitizerTarget, IndigoHidClient,
        ScrollTarget, UniversalHidServiceClient, scroll_momentum, scroll_phase,
    },
    core_device_proxy::CoreDeviceProxy,
    provider::IdeviceProvider,
    rsd::RsdHandshake,
    tcp::handle::AdapterHandle,
};
use jkcli::{CollectedArguments, JkArgument, JkCommand};

use crate::coredevice_stream::start_screen_media_stream;

/// Named iOS hardware buttons -> (usage_page, usage_code, hold_ms).
const NAMED_BUTTONS: &[(&str, u64, u64, u64)] = &[
    ("home", 0x0C, 0x40, 80),
    ("power", 0x0C, 0x30, 200),
    ("lock", 0x0C, 0x30, 200),
    ("sleep", 0x0C, 0x30, 200),
    ("volume-up", 0x0C, 0xE9, 80),
    ("volume-down", 0x0C, 0xEA, 80),
    ("mute", 0x0C, 0xE2, 80),
    ("siri", 0x0C, 0xCF, 1200),
];

pub fn register() -> JkCommand {
    JkCommand::new()
        .help("Remote-control the device's HID (touch + hardware buttons) over CoreDevice")
        .with_subcommand(
            "tap",
            JkCommand::new()
                .help(
                    "Tap at (X, Y) in normalized screen coordinates 0-65535 \
                     (32768 = center, 0 = top/left, 65535 = bottom/right)",
                )
                .with_argument(JkArgument::new().with_help("X (0-65535)").required(true))
                .with_argument(JkArgument::new().with_help("Y (0-65535)").required(true)),
        )
        .with_subcommand(
            "drag",
            JkCommand::new()
                .help(
                    "Drag/scroll from (X1, Y1) to (X2, Y2) in normalized screen \
                     coordinates 0-65535 (32768 = center) - NOT pixels",
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("start X (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("start Y (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("end X (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("end Y (0-65535)")
                        .required(true),
                ),
        )
        .with_subcommand(
            "button",
            JkCommand::new()
                .help(
                    "Press a named hardware button: home, power, lock, sleep, \
                     volume-up, volume-down, mute, siri",
                )
                .with_argument(JkArgument::new().with_help("button name").required(true)),
        )
        .with_subcommand(
            "type",
            JkCommand::new()
                .help("Type an ASCII string on the device's keyboard")
                .with_argument(JkArgument::new().with_help("text to type").required(true)),
        )
        .with_subcommand(
            "key",
            JkCommand::new()
                .help(
                    "Press a single key: a named key (return, esc, tab, space, \
                     backspace, delete, up, down, left, right) or a hex HID \
                     keyboard usage (e.g. 0x04 for 'a')",
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("key name or hex usage")
                        .required(true),
                ),
        )
        .with_subcommand(
            "swipe",
            JkCommand::new()
                .help("A digitizer swipe, doesn't seem to work")
                .with_argument(JkArgument::new().with_help("edge").required(true))
                .with_argument(
                    JkArgument::new()
                        .with_help("start X (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("start Y (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("end X (0-65535)")
                        .required(true),
                )
                .with_argument(
                    JkArgument::new()
                        .with_help("end Y (0-65535)")
                        .required(true),
                ),
        )
        .with_subcommand(
            "scroll",
            JkCommand::new()
                .help("Scroll the digital crown by delta (e.g. 10 or -10)")
                .with_argument(
                    JkArgument::new()
                        .with_help("scroll delta (signed)")
                        .required(true),
                ),
        )
        .with_subcommand(
            "list",
            JkCommand::new()
                .help("List the device's registered HID surfaces and their _ServiceIDs"),
        )
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

    match sub_name.as_str() {
        "tap" => {
            let x = parse_arg::<u16>(&mut sub_args, "X");
            let y = parse_arg::<u16>(&mut sub_args, "Y");
            with_touch(&mut adapter, &mut handshake, |hid| {
                Box::pin(async move {
                    hid.tap(x, y).await.expect("tap failed");
                    eprintln!("tapped ({x}, {y})");
                })
            })
            .await;
        }
        "drag" => {
            let x1 = parse_arg::<u16>(&mut sub_args, "start X");
            let y1 = parse_arg::<u16>(&mut sub_args, "start Y");
            let x2 = parse_arg::<u16>(&mut sub_args, "end X");
            let y2 = parse_arg::<u16>(&mut sub_args, "end Y");
            // 30 frames over ~0.6s reads as a deliberate drag (not a flick/tap).
            const STEPS: u32 = 30;
            const DELAY_MS: u64 = 20;
            with_touch(&mut adapter, &mut handshake, |hid| {
                Box::pin(async move {
                    hid.drag(x1, y1, x2, y2, STEPS, DELAY_MS)
                        .await
                        .expect("drag failed");
                    eprintln!("dragged ({x1}, {y1}) -> ({x2}, {y2})");
                })
            })
            .await;
        }
        "button" => {
            let name: String = sub_args
                .next_argument()
                .expect("no button name passed, pass -h for help");
            let Some(&(_, usage_page, usage_code, hold_ms)) =
                NAMED_BUTTONS.iter().find(|(n, _, _, _)| *n == name)
            else {
                eprintln!(
                    "unknown button '{name}'. Known: {}",
                    NAMED_BUTTONS
                        .iter()
                        .map(|(n, _, _, _)| *n)
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                return;
            };
            // Like every Indigo event, buttons only land while a media stream
            // holds the HID auth gate open.
            with_indigo_stream(&mut adapter, &mut handshake, |indigo| {
                Box::pin(async move {
                    indigo
                        .send_button(usage_page, usage_code, ButtonState::Down)
                        .await
                        .expect("button down failed");
                    tokio::time::sleep(std::time::Duration::from_millis(hold_ms)).await;
                    indigo
                        .send_button(usage_page, usage_code, ButtonState::Up)
                        .await
                        .expect("button up failed");
                    eprintln!("pressed {name}");
                })
            })
            .await;
        }
        "type" => {
            let text: String = sub_args
                .next_argument()
                .expect("no text passed, pass -h for help");
            with_indigo_stream(&mut adapter, &mut handshake, |indigo| {
                Box::pin(async move {
                    let mut typed = 0usize;
                    for ch in text.chars() {
                        let Some((usage, shift)) = ascii_to_usage(ch) else {
                            eprintln!("skipping unsupported character {ch:?}");
                            continue;
                        };
                        type_key(indigo, usage, shift).await;
                        typed += 1;
                    }
                    eprintln!("typed {typed} character(s)");
                })
            })
            .await;
        }
        "key" => {
            let name: String = sub_args
                .next_argument()
                .expect("no key passed, pass -h for help");
            let Some(usage) = parse_key(&name) else {
                eprintln!("unknown key '{name}'. Use a named key or hex usage like 0x28");
                return;
            };
            with_indigo_stream(&mut adapter, &mut handshake, |indigo| {
                Box::pin(async move {
                    type_key(indigo, usage, false).await;
                    eprintln!("pressed key {name}");
                })
            })
            .await;
        }
        "scroll" => {
            let delta: f64 = parse_arg(&mut sub_args, "scroll delta");
            with_indigo_stream(&mut adapter, &mut handshake, |indigo| {
                Box::pin(async move {
                    // began -> changed (the delta) -> ended, no inertial momentum.
                    indigo
                        .send_scroll(
                            (0.0, 0.0, 0.0),
                            scroll_phase::BEGAN,
                            scroll_momentum::UNDEFINED,
                            ScrollTarget::DigitalCrown,
                        )
                        .await
                        .expect("scroll began failed");
                    indigo
                        .send_scroll(
                            (0.0, delta, 0.0),
                            scroll_phase::CHANGED,
                            scroll_momentum::UNDEFINED,
                            ScrollTarget::DigitalCrown,
                        )
                        .await
                        .expect("scroll changed failed");
                    indigo
                        .send_scroll(
                            (0.0, 0.0, 0.0),
                            scroll_phase::ENDED,
                            scroll_momentum::UNDEFINED,
                            ScrollTarget::DigitalCrown,
                        )
                        .await
                        .expect("scroll ended failed");
                    eprintln!("scrolled crown by {delta}");
                })
            })
            .await;
        }
        "swipe" => {
            let edge_name: String = sub_args
                .next_argument()
                .expect("no edge passed, pass -h for help");
            let Some(edge) = parse_edge(&edge_name) else {
                eprintln!("unknown edge '{edge_name}'. Use none, top, bottom, left, right");
                return;
            };
            let x1: f64 = parse_arg(&mut sub_args, "start X");
            let y1: f64 = parse_arg(&mut sub_args, "start Y");
            let x2: f64 = parse_arg(&mut sub_args, "end X");
            let y2: f64 = parse_arg(&mut sub_args, "end Y");
            with_indigo_stream(&mut adapter, &mut handshake, |indigo| {
                Box::pin(async move {
                    swipe(indigo, edge, (x1, y1), (x2, y2)).await;
                    eprintln!("swiped ({x1}, {y1}) -> ({x2}, {y2}) edge={edge_name}");
                })
            })
            .await;
        }
        "list" => {
            let mut hid = UniversalHidServiceClient::connect_rsd(&mut adapter, &mut handshake)
                .await
                .expect("no hid.universalhidservice");
            let surfaces = hid.list_connected_services().await.expect("list failed");
            for s in surfaces {
                let product = s.product.as_deref().unwrap_or("(unnamed)");
                println!(
                    "[{}] {product} (usage {}/{})",
                    s.service_id,
                    s.primary_usage_page.unwrap_or(0),
                    s.primary_usage.unwrap_or(0),
                );
            }
        }
        _ => unreachable!(),
    }
}

/// Connect the touch surface, holding the media-stream auth gate open for the
/// duration of `gesture`. The stream's RTP is left unread (discarded).
async fn with_touch<F>(adapter: &mut AdapterHandle, handshake: &mut RsdHandshake, gesture: F)
where
    F: for<'a> FnOnce(
        &'a mut UniversalHidServiceClient<Box<dyn idevice::ReadWrite>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>,
{
    // Without the media stream, backboardd drops every digitizer event as
    // coming from an unauthenticated service.
    let mut session = match start_screen_media_stream(adapter, handshake, 1).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to start media stream (needed for touch auth gate): {e}");
            eprintln!("the device's mediastream / dtuhidd daemon may be wedged — reboot and retry");
            return;
        }
    };

    // We don't read the RTP the device streams at us, we only need the stream
    // to exist. The sockets must stay bound for the duration though (dropping
    // them unbinds the ports), so hold them until teardown. jktcp buffers the
    // unread datagrams; for a short gesture that's negligible.
    let _audio_udp = session.audio_udp;
    let _video_udp = session.video_udp;

    // backboardd needs a moment to re-match the HID surfaces against the
    // newly-authenticated stream before our reports dispatch as builtIn.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    match UniversalHidServiceClient::connect_rsd(adapter, handshake).await {
        Ok(mut hid) => gesture(&mut hid).await,
        Err(e) => eprintln!("no hid.universalhidservice: {e:?}"),
    }

    if let Err(e) = session.client.stop_media_stream().await {
        eprintln!("stop_media_stream failed: {e:?}");
    }
}

/// HID Keyboard/Keypad usage for Left Shift.
const KEY_LEFT_SHIFT: u64 = 0xE1;

/// Press a key (down then up), bracketing with Left Shift if `shift` is set.
async fn type_key(indigo: &mut IndigoHidClient<Box<dyn ReadWrite>>, usage: u64, shift: bool) {
    if shift {
        indigo
            .send_keyboard(KEY_LEFT_SHIFT, ButtonState::Down)
            .await
            .expect("shift down failed");
    }
    indigo
        .send_keyboard(usage, ButtonState::Down)
        .await
        .expect("key down failed");
    indigo
        .send_keyboard(usage, ButtonState::Up)
        .await
        .expect("key up failed");
    if shift {
        indigo
            .send_keyboard(KEY_LEFT_SHIFT, ButtonState::Up)
            .await
            .expect("shift up failed");
    }
    // A small gap so the device's input stack registers discrete keystrokes.
    tokio::time::sleep(std::time::Duration::from_millis(12)).await;
}

/// Map an ASCII character to its HID Keyboard/Keypad usage and whether Shift is
/// required (US layout).
fn ascii_to_usage(c: char) -> Option<(u64, bool)> {
    Some(match c {
        'a'..='z' => (0x04 + (c as u64 - 'a' as u64), false),
        'A'..='Z' => (0x04 + (c as u64 - 'A' as u64), true),
        '1'..='9' => (0x1E + (c as u64 - '1' as u64), false),
        '0' => (0x27, false),
        '\n' => (0x28, false), // Return
        '\t' => (0x2B, false), // Tab
        ' ' => (0x2C, false),  // Space
        '!' => (0x1E, true),
        '@' => (0x1F, true),
        '#' => (0x20, true),
        '$' => (0x21, true),
        '%' => (0x22, true),
        '^' => (0x23, true),
        '&' => (0x24, true),
        '*' => (0x25, true),
        '(' => (0x26, true),
        ')' => (0x27, true),
        '-' => (0x2D, false),
        '_' => (0x2D, true),
        '=' => (0x2E, false),
        '+' => (0x2E, true),
        '[' => (0x2F, false),
        '{' => (0x2F, true),
        ']' => (0x30, false),
        '}' => (0x30, true),
        '\\' => (0x31, false),
        '|' => (0x31, true),
        ';' => (0x33, false),
        ':' => (0x33, true),
        '\'' => (0x34, false),
        '"' => (0x34, true),
        '`' => (0x35, false),
        '~' => (0x35, true),
        ',' => (0x36, false),
        '<' => (0x36, true),
        '.' => (0x37, false),
        '>' => (0x37, true),
        '/' => (0x38, false),
        '?' => (0x38, true),
        _ => return None,
    })
}

/// Parse a named key or a hex/decimal HID keyboard usage.
fn parse_key(name: &str) -> Option<u64> {
    let usage = match name.to_ascii_lowercase().as_str() {
        "return" | "enter" => 0x28,
        "esc" | "escape" => 0x29,
        "backspace" => 0x2A,
        "delete" | "del" => 0x4C, // Delete Forward
        "tab" => 0x2B,
        "space" => 0x2C,
        "right" => 0x4F,
        "left" => 0x50,
        "down" => 0x51,
        "up" => 0x52,
        "home" => 0x4A,
        "end" => 0x4D,
        "pageup" => 0x4B,
        "pagedown" => 0x4E,
        other => {
            return if let Some(hex) = other.strip_prefix("0x") {
                u64::from_str_radix(hex, 16).ok()
            } else {
                other.parse::<u64>().ok()
            };
        }
    };
    Some(usage)
}

/// Map an edge name to a [`DigitizerEdge`].
fn parse_edge(name: &str) -> Option<DigitizerEdge> {
    Some(match name.to_ascii_lowercase().as_str() {
        "none" => DigitizerEdge::None,
        "top" => DigitizerEdge::Top,
        "left" => DigitizerEdge::Left,
        "bottom" => DigitizerEdge::Bottom,
        "right" => DigitizerEdge::Right,
        _ => return None,
    })
}

/// Drive a digitizer swipe: a `start` contact, a few `position` samples
/// advancing linearly, then `end` at the destination.
/// Does not seem to work
async fn swipe(
    indigo: &mut IndigoHidClient<Box<dyn ReadWrite>>,
    edge: DigitizerEdge,
    from: (f64, f64),
    to: (f64, f64),
) {
    const STEPS: u32 = 10;
    indigo
        .send_digitizer(
            from,
            None,
            DigitizerEventType::Start,
            edge,
            DigitizerTarget::MainScreen,
        )
        .await
        .expect("digitizer start failed");
    for i in 1..=STEPS {
        let t = i as f64 / (STEPS + 1) as f64;
        let p = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        indigo
            .send_digitizer(
                p,
                None,
                DigitizerEventType::Position,
                edge,
                DigitizerTarget::MainScreen,
            )
            .await
            .expect("digitizer position failed");
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }
    indigo
        .send_digitizer(
            to,
            None,
            DigitizerEventType::End,
            edge,
            DigitizerTarget::MainScreen,
        )
        .await
        .expect("digitizer end failed");
}

/// Like [`with_touch`] but yields an [`IndigoHidClient`] while the media-stream
/// auth gate is held open. ALL Indigo events (buttons, keyboard, scroll,
/// digitizer) need this: without an active displayservice media stream the
/// device drops the synthetic HID events `dtuhidd` posts, so nothing lands
/// on-screen even though the event decodes and dispatches cleanly.
async fn with_indigo_stream<F>(
    adapter: &mut AdapterHandle,
    handshake: &mut RsdHandshake,
    gesture: F,
) where
    F: for<'a> FnOnce(
        &'a mut IndigoHidClient<Box<dyn idevice::ReadWrite>>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>,
{
    let mut session = match start_screen_media_stream(adapter, handshake, 1).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("failed to start media stream (needed for digitizer auth gate): {e}");
            return;
        }
    };
    let _audio_udp = session.audio_udp;
    let _video_udp = session.video_udp;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    match IndigoHidClient::connect_rsd(adapter, handshake).await {
        Ok(mut indigo) => gesture(&mut indigo).await,
        Err(e) => eprintln!("no hid.indigo service: {e:?}"),
    }
    if let Err(e) = session.client.stop_media_stream().await {
        eprintln!("stop_media_stream failed: {e:?}");
    }
}

fn parse_arg<T: jkcli::JkArgumentType>(args: &mut CollectedArguments, what: &str) -> T {
    args.next_argument()
        .unwrap_or_else(|| panic!("missing or invalid {what}, pass -h for help"))
}
