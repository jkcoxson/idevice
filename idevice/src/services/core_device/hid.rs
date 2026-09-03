//! HID back-channel for CoreDevice remote control. Served by the DDI daemon `dtuhidd`.
//!
//! **Authentication gate (applies to EVERY event kind here):** the device drops
//! the synthetic HID events `dtuhidd` posts unless a displayservice media stream
//! is active. This is true for buttons and keyboard just as much as touch.
//! Without the stream the event decodes and dispatches cleanly (the daemon even
//! logs `received peer event`) but nothing happens. Starting a
//! displayservice media stream (see `display_stream`) authenticates the HID
//! surfaces so the events route through to the system as real input.
//! The stream only needs to exist for the duration of the events;
//! its RTP payload can be discarded.

use serde::Deserialize;
use std::{borrow::Cow, collections::BTreeSet, fmt};
use web_time::{SystemTime, UNIX_EPOCH};

use crate::{
    IdeviceError, ReadWrite, RemoteXpcClient, obf,
    services::core_device::CoreDeviceError,
    xpc::{Dictionary, XPCObject},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Down,
    Up,
}

impl ButtonState {
    pub fn raw(self) -> u64 {
        match self {
            ButtonState::Down => 1,
            ButtonState::Up => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigitizerEventType {
    Start,
    Position,
    End,
}

impl DigitizerEventType {
    pub fn raw(self) -> u64 {
        match self {
            DigitizerEventType::Start => 0,
            DigitizerEventType::Position => 1,
            DigitizerEventType::End => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigitizerEdge {
    None,
    Top,
    Left,
    Bottom,
    Right,
}

impl DigitizerEdge {
    pub fn raw(self) -> u64 {
        match self {
            DigitizerEdge::None => 0,
            DigitizerEdge::Top => 1,
            DigitizerEdge::Left => 2,
            DigitizerEdge::Bottom => 3,
            DigitizerEdge::Right => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DigitizerTarget {
    MainScreen,
    Display(u64),
}

impl DigitizerTarget {
    pub fn raw(self) -> u64 {
        match self {
            DigitizerTarget::MainScreen => 0,
            DigitizerTarget::Display(n) => n,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    DigitalCrown,
    Dial,
}

impl ScrollTarget {
    pub fn raw(self) -> u64 {
        match self {
            ScrollTarget::DigitalCrown => 0,
            ScrollTarget::Dial => 1,
        }
    }
}

pub mod scroll_phase {
    pub const UNDEFINED: u64 = 0x0;
    pub const BEGAN: u64 = 0x1;
    pub const CHANGED: u64 = 0x2;
    pub const ENDED: u64 = 0x4;
    pub const CANCELLED: u64 = 0x8;
    pub const MAY_BEGIN: u64 = 0x80;
}

pub mod scroll_momentum {
    pub const UNDEFINED: u64 = 0x0;
    pub const CONTINUE: u64 = 0x1;
    pub const START: u64 = 0x2;
    pub const END: u64 = 0x4;
    pub const WILL_BEGIN: u64 = 0x8;
    pub const INTERRUPTED: u64 = 0x10;
}

pub const DIGITIZER_REPORT_ID: u8 = 0x13;
pub const TOUCHSCREEN_REPORT_ID: u8 = 0x09;
pub const TOUCHSCREEN_STATE_CONTACT: u8 = 0xC2;
pub const TOUCHSCREEN_STATE_RELEASE: u8 = 0x02;
/// Maximum number of contacts in one `mainTouchscreen` report.
pub const TOUCHSCREEN_CONTACT_COUNT_MAXIMUM: u8 = 5;

pub const DIGITIZER_SURFACE_MAIN_TOUCHSCREEN: u64 = 257;
pub const DIGITIZER_SURFACE_TOUCHSCREEN_GESTURE: u64 = 1281;

const TOUCHSCREEN_REPORT_SIZE: usize = 58;
const TOUCHSCREEN_CONTACTS_OFFSET: usize = 3;
const TOUCHSCREEN_CONTACT_SIZE: usize = 5;
const MAIN_KEYBOARD_REPORT_ID: u8 = 0x01;
const MAIN_KEYBOARD_USAGE_BITMAP_BYTES: usize = 29;
const MAIN_KEYBOARD_REPORT_SIZE: usize = 39;
const MAIN_KEYBOARD_MIN_USAGE: u16 = 0x01;
const MAIN_KEYBOARD_MAX_USAGE: u16 = 0xE7;
const MAIN_KEYBOARD_PRIMARY_USAGE: u64 = 0x06;
const MAIN_KEYBOARD_PRIMARY_USAGE_PAGE: u64 = 0x01;
const MAIN_KEYBOARD_VENDOR_ID: i64 = 0x05AC;
const MAIN_KEYBOARD_PRODUCT_ID: i64 = 0x0250;
const MAIN_KEYBOARD_REQUESTED_SERVICE_ID: u64 = 0x1_0000_2001;
const MAIN_KEYBOARD_PRODUCT: &str = "idevice mainKeyboard";
const MAIN_KEYBOARD_MANUFACTURER: &str = "idevice";
const MAX_CONNECTED_HID_SERVICES: usize = 256;
const MAX_HID_PRODUCT_BYTES: usize = 256;

// UniversalHID KeyboardReport descriptor: report ID, 232 usage bits, one
// constant byte, and eight bytes of remote timestamp/vendor data.
const MAIN_KEYBOARD_REPORT_DESCRIPTOR: [u8; 56] = [
    0x85, 0x01, 0x05, 0x07, 0x19, 0x01, 0x29, 0xE7, 0x96, 0xE8, 0x00, 0x75, 0x01, 0x15, 0x01, 0x26,
    0xE7, 0x00, 0x81, 0x02, 0xA1, 0x02, 0x06, 0x1A, 0xFF, 0x0A, 0xF1, 0xE0, 0x19, 0x00, 0x29, 0x00,
    0x75, 0x08, 0x95, 0x01, 0x81, 0x01, 0xC0, 0x06, 0x00, 0xFF, 0x0A, 0x02, 0x01, 0x15, 0x00, 0x26,
    0xFF, 0x00, 0x75, 0x08, 0x95, 0x08, 0x81, 0x02,
];

/// A validated HID Keyboard/Keypad-page usage accepted by mainKeyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyboardUsage(u8);

impl KeyboardUsage {
    pub fn new(raw: u16) -> Result<Self, MainKeyboardError> {
        if !(MAIN_KEYBOARD_MIN_USAGE..=MAIN_KEYBOARD_MAX_USAGE).contains(&raw) {
            return Err(MainKeyboardError::InvalidUsage);
        }
        Ok(Self(raw as u8))
    }

    pub const fn raw(self) -> u8 {
        self.0
    }

    const fn bitmap_position(self) -> (usize, u8) {
        let usage = self.0 as usize;
        (usage / 8, 1 << (usage % 8))
    }
}

/// Content-free failure categories for the fixed mainKeyboard lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MainKeyboardError {
    #[error("mainKeyboard transport is unavailable")]
    Transport,
    #[error("mainKeyboard response is malformed")]
    MalformedResponse,
    #[error("mainKeyboard service was not visible after creation")]
    ServiceNotVisible,
    #[error("mainKeyboard service identity was ambiguous")]
    AmbiguousIdentity,
    #[error("mainKeyboard metadata evidence was missing")]
    MetadataMissing,
    #[error("mainKeyboard metadata evidence was invalid")]
    MetadataInvalid,
    #[error("mainKeyboard usage identity did not match")]
    UsageMismatch,
    #[error("mainKeyboard codable identity evidence was missing")]
    IdentityEvidenceMissing,
    #[error("mainKeyboard virtual-service identity did not match")]
    VirtualServiceMismatch,
    #[error("mainKeyboard report descriptor identity did not match")]
    DescriptorMismatch,
    #[error("mainKeyboard constructor rollback failed")]
    RollbackFailed,
    #[error("mainKeyboard service is no longer active")]
    Inactive,
    #[error("mainKeyboard usage is outside the supported report range")]
    InvalidUsage,
    #[error("mainKeyboard key is already pressed")]
    KeyAlreadyPressed,
    #[error("mainKeyboard key is not pressed")]
    KeyNotPressed,
    #[error("mainKeyboard service remained registered after removal")]
    StillRegistered,
}

struct MainKeyboardOwnership;

/// Opaque owner token for one confirmed fixed mainKeyboard service.
///
/// The device service identifier and pressed-key report stay inside this
/// adapter. The token is intentionally neither `Clone` nor externally
/// constructible, preventing callers from aliasing cleanup ownership.
pub struct MainKeyboardService {
    service_id: u64,
    pressed: [u8; MAIN_KEYBOARD_USAGE_BITMAP_BYTES],
    active: bool,
    _ownership: MainKeyboardOwnership,
}

impl MainKeyboardService {
    pub const fn is_active(&self) -> bool {
        self.active
    }

    pub fn pressed_count(&self) -> usize {
        self.pressed
            .iter()
            .map(|byte| byte.count_ones() as usize)
            .sum()
    }
}

impl fmt::Debug for MainKeyboardService {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MainKeyboardService")
            .field("service_id", &"<redacted>")
            .field("pressed_count", &self.pressed_count())
            .field("active", &self.active)
            .finish()
    }
}

fn universal_hid_request(payload: Dictionary) -> Dictionary {
    let mut request = Dictionary::new();
    let universal_hid_feature: Cow<'static, str> =
        obf!("com.apple.coredevice.feature.remote.universalhidservice");
    request.insert(
        "featureIdentifier".into(),
        XPCObject::String(universal_hid_feature.into()),
    );
    request.insert("messageType".into(), XPCObject::String("Request".into()));
    request.insert("payload".into(), XPCObject::Dictionary(payload));
    request
}

fn build_connected_services_request() -> Dictionary {
    let mut payload = Dictionary::new();
    payload.insert(
        "connectedServices".into(),
        XPCObject::Dictionary(Dictionary::new()),
    );
    universal_hid_request(payload)
}

fn build_main_keyboard_create_request() -> Dictionary {
    let payload = crate::xpc!({
        "createService": {
            "_0": {
                "DeviceUsagePairs": [{
                    "DeviceUsage": MAIN_KEYBOARD_PRIMARY_USAGE as i64,
                    "DeviceUsagePage": MAIN_KEYBOARD_PRIMARY_USAGE_PAGE as i64,
                }],
                "PrimaryUsage": MAIN_KEYBOARD_PRIMARY_USAGE,
                "PrimaryUsagePage": MAIN_KEYBOARD_PRIMARY_USAGE_PAGE,
                "Product": MAIN_KEYBOARD_PRODUCT,
                "ProductID": MAIN_KEYBOARD_PRODUCT_ID,
                "VendorID": MAIN_KEYBOARD_VENDOR_ID,
                "_CoreDevice_codablePropertyStorage": {
                    "Manufacturer": { "string": MAIN_KEYBOARD_MANUFACTURER },
                    "Product": { "string": MAIN_KEYBOARD_PRODUCT },
                    "ProductID": { "int": MAIN_KEYBOARD_PRODUCT_ID },
                    "VendorID": { "int": MAIN_KEYBOARD_VENDOR_ID },
                    "PrimaryUsage": { "int": MAIN_KEYBOARD_PRIMARY_USAGE as i64 },
                    "PrimaryUsagePage": {
                        "int": MAIN_KEYBOARD_PRIMARY_USAGE_PAGE as i64,
                    },
                    "DeviceUsagePairs": {
                        "array": [{
                            "dictionary": {
                                "DeviceUsage": {
                                    "int": MAIN_KEYBOARD_PRIMARY_USAGE as i64,
                                },
                                "DeviceUsagePage": {
                                    "int": MAIN_KEYBOARD_PRIMARY_USAGE_PAGE as i64,
                                },
                            },
                        }],
                    },
                    "Transport": { "string": "USB" },
                    "ReportDescriptor": {
                        "data": MAIN_KEYBOARD_REPORT_DESCRIPTOR.to_vec(),
                    },
                    "UniversalControlVirtualService": { "bool": true },
                    "_ServiceID": { "uint": MAIN_KEYBOARD_REQUESTED_SERVICE_ID },
                },
                "_ServiceID": MAIN_KEYBOARD_REQUESTED_SERVICE_ID,
            },
        },
    })
    .to_dictionary()
    .expect("mainKeyboard payload is a dictionary");
    universal_hid_request(payload)
}

fn build_main_keyboard_service_request(operation: &'static str, service_id: u64) -> Dictionary {
    let mut tuple = Dictionary::new();
    tuple.insert("_0".into(), XPCObject::UInt64(service_id));
    let mut payload = Dictionary::new();
    payload.insert(operation.into(), XPCObject::Dictionary(tuple));
    universal_hid_request(payload)
}

fn main_keyboard_usage_bitmap(
    usages: impl IntoIterator<Item = KeyboardUsage>,
) -> [u8; MAIN_KEYBOARD_USAGE_BITMAP_BYTES] {
    let mut bitmap = [0; MAIN_KEYBOARD_USAGE_BITMAP_BYTES];
    for usage in usages {
        let (byte, mask) = usage.bitmap_position();
        bitmap[byte] |= mask;
    }
    bitmap
}

fn build_main_keyboard_report(bitmap: &[u8; MAIN_KEYBOARD_USAGE_BITMAP_BYTES]) -> Vec<u8> {
    let timestamp = default_timestamp();
    let mut report = Vec::with_capacity(MAIN_KEYBOARD_REPORT_SIZE);
    report.push(MAIN_KEYBOARD_REPORT_ID);
    report.extend_from_slice(bitmap);
    report.push(0);
    report.extend_from_slice(&timestamp.to_le_bytes()[..6]);
    report.extend_from_slice(&[0, 0]);
    debug_assert_eq!(report.len(), MAIN_KEYBOARD_REPORT_SIZE);
    report
}

fn build_send_report_request(service_id: u64, report: Vec<u8>) -> Dictionary {
    let mut tuple = Dictionary::new();
    tuple.insert("_0".into(), XPCObject::Data(report));
    tuple.insert("_1".into(), XPCObject::UInt64(service_id));
    let mut payload = Dictionary::new();
    payload.insert("send".into(), XPCObject::Dictionary(tuple));
    universal_hid_request(payload)
}

/// One contact in a `mainTouchscreen` HID report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TouchscreenContact {
    /// Stable identity for the lifetime of this contact. Valid values are 0..5.
    pub identity: u8,
    /// Whether the contact is touching and in range. A release keeps the same
    /// identity and coordinates with this set to `false`.
    pub touching: bool,
    /// Horizontal position in the touchscreen's normalized 16-bit space.
    pub x: u16,
    /// Vertical position in the touchscreen's normalized 16-bit space.
    pub y: u16,
}

/// A 48-bit monotonic timestamp for HID reports. The gesture recognizer only
/// cares about monotonicity and inter-frame deltas, so wall-clock nanoseconds
/// (truncated to 48 bits) are sufficient.
fn default_timestamp() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    nanos & ((1u64 << 48) - 1)
}

/// Build a 19-byte gesture/pointer HID report
///
/// `x`/`y` are signed 32-bit. `timestamp` is a 48-bit monotonic value; pass
/// `None` to use the current wall clock.
///
/// Layout: `[0x13][x:i32 LE][y:i32 LE][00 00][ts:6 LE][00 00]`.
pub fn build_digitizer_report(x: i32, y: i32, timestamp: Option<u64>) -> Vec<u8> {
    let ts = timestamp.unwrap_or_else(default_timestamp) & ((1u64 << 48) - 1);
    let mut r = Vec::with_capacity(19);
    r.push(DIGITIZER_REPORT_ID);
    r.extend_from_slice(&x.to_le_bytes());
    r.extend_from_slice(&y.to_le_bytes());
    r.extend_from_slice(&[0, 0]);
    r.extend_from_slice(&ts.to_le_bytes()[..6]);
    r.extend_from_slice(&[0, 0]);
    r
}

/// Build a 58-byte `mainTouchscreen` HID report (report ID `0x09`).
///
/// `state` is [`TOUCHSCREEN_STATE_CONTACT`] (a touch sample at `x`/`y`) or
/// [`TOUCHSCREEN_STATE_RELEASE`] (lift). `x`/`y` are unsigned 16-bit. Pass
/// `timestamp = None` to use the current wall clock.
///
/// Layout: `[0x09 0x01 0x05 state][x:u16 LE][y:u16 LE][32×00][02 00 00 00][ts:6 LE][8×00]`.
pub fn build_touchscreen_report(state: u8, x: u16, y: u16, timestamp: Option<u64>) -> Vec<u8> {
    let mut r = new_touchscreen_report(1, timestamp);
    write_touchscreen_contact(&mut r, 0, state, x, y);
    r
}

/// Build a 58-byte `mainTouchscreen` report containing up to five contacts.
///
/// Every active contact must be included in each frame. To lift a contact,
/// include it once more with [`TouchscreenContact::touching`] set to `false`
/// and the same identity used by its active frames.
pub fn build_multitouch_report(
    contacts: &[TouchscreenContact],
    timestamp: Option<u64>,
) -> Result<Vec<u8>, CoreDeviceError> {
    if contacts.len() > TOUCHSCREEN_CONTACT_COUNT_MAXIMUM as usize {
        return Err(CoreDeviceError::TooManyTouchscreenContacts(contacts.len()));
    }
    for (index, contact) in contacts.iter().enumerate() {
        if contact.identity >= TOUCHSCREEN_CONTACT_COUNT_MAXIMUM {
            return Err(CoreDeviceError::InvalidTouchscreenContactIdentity(
                contact.identity,
            ));
        }
        if contacts[..index]
            .iter()
            .any(|other| other.identity == contact.identity)
        {
            return Err(CoreDeviceError::DuplicateTouchscreenContactIdentity(
                contact.identity,
            ));
        }
    }

    let mut r = new_touchscreen_report(contacts.len() as u8, timestamp);
    for (slot, contact) in contacts.iter().enumerate() {
        let state = (if contact.touching { 0xC0 } else { 0 }) | contact.identity;
        write_touchscreen_contact(&mut r, slot, state, contact.x, contact.y);
    }
    Ok(r)
}

fn multitap_contacts(positions: &[(u16, u16)]) -> Result<Vec<TouchscreenContact>, CoreDeviceError> {
    if positions.is_empty() {
        return Err(CoreDeviceError::NoTouchscreenContacts);
    }
    if positions.len() > TOUCHSCREEN_CONTACT_COUNT_MAXIMUM as usize {
        return Err(CoreDeviceError::TooManyTouchscreenContacts(positions.len()));
    }
    Ok(positions
        .iter()
        .enumerate()
        .map(|(identity, &(x, y))| TouchscreenContact {
            identity: identity as u8,
            touching: true,
            x,
            y,
        })
        .collect())
}

fn new_touchscreen_report(contact_count: u8, timestamp: Option<u64>) -> Vec<u8> {
    let ts = timestamp.unwrap_or_else(default_timestamp) & ((1u64 << 48) - 1);
    let mut r = vec![0; TOUCHSCREEN_REPORT_SIZE];
    r[0] = TOUCHSCREEN_REPORT_ID;
    r[1] = contact_count;
    r[2] = TOUCHSCREEN_CONTACT_COUNT_MAXIMUM;
    r[40..44].copy_from_slice(&[0x02, 0x00, 0x00, 0x00]);
    r[44..50].copy_from_slice(&ts.to_le_bytes()[..6]);
    r
}

fn write_touchscreen_contact(report: &mut [u8], slot: usize, state: u8, x: u16, y: u16) {
    let offset = TOUCHSCREEN_CONTACTS_OFFSET + slot * TOUCHSCREEN_CONTACT_SIZE;
    report[offset] = state;
    report[offset + 1..offset + 3].copy_from_slice(&x.to_le_bytes());
    report[offset + 3..offset + 5].copy_from_slice(&y.to_le_bytes());
}

/// Generic Indigo HID events.
///
/// `com.apple.coredevice.hid.indigo`.
#[derive(Debug)]
pub struct IndigoHidClient<R: ReadWrite> {
    inner: RemoteXpcClient<R>,
}

#[cfg(feature = "rsd")]
impl crate::RsdService for IndigoHidClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> Cow<'static, str> {
        obf!("com.apple.coredevice.hid.indigo")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        let mut inner = RemoteXpcClient::new(stream).await?;
        inner.do_handshake().await?;
        Ok(Self { inner })
    }
}

impl<R: ReadWrite> IndigoHidClient<R> {
    pub fn new(inner: RemoteXpcClient<R>) -> Self {
        Self { inner }
    }

    /// Wrap `payload` in the shared `{messageType, payload, featureIdentifier}`
    /// envelope and send it one-way (no reply expected). This is the single
    /// dispatch path every Indigo event kind shares.
    async fn send_event(
        &mut self,
        message_type: &str,
        feature_identifier: Cow<'static, str>,
        payload: Dictionary,
    ) -> Result<(), IdeviceError> {
        let mut msg = Dictionary::new();
        msg.insert(
            "messageType".into(),
            XPCObject::String(message_type.to_string()),
        );
        msg.insert("payload".into(), XPCObject::Dictionary(payload));
        msg.insert(
            "featureIdentifier".into(),
            XPCObject::String(feature_identifier.into()),
        );
        self.inner.send_object(msg, false).await
    }

    /// Send an `IndigoButtonEvent`: a single hardware-button state change.
    ///
    /// * `usage_page` - HID usage page (e.g. `0x0C` Consumer for media keys,
    ///   `0x01` Generic Desktop for power/sleep).
    /// * `usage_code` - HID usage within that page.
    /// * `state` - [`ButtonState::Down`] or [`ButtonState::Up`].
    pub async fn send_button(
        &mut self,
        usage_page: u64,
        usage_code: u64,
        state: ButtonState,
    ) -> Result<(), IdeviceError> {
        let mut payload = Dictionary::new();
        payload.insert("state".into(), XPCObject::UInt64(state.raw()));
        payload.insert("usagePage".into(), XPCObject::UInt64(usage_page));
        payload.insert("usageCode".into(), XPCObject::UInt64(usage_code));
        self.send_event(
            "IndigoButtonEvent",
            obf!("com.apple.coredevice.feature.remote.hid.button"),
            payload,
        )
        .await
    }

    /// Send an `IndigoKeyboardButtonEvent`: a single keyboard key state change.
    ///
    /// * `usage_code` - HID Keyboard/Keypad page (`0x07`) usage, e.g. `0x04`=`a`,
    ///   `0x28`=Return, `0x2A`=Backspace, `0xE1`=Left Shift. The usage page is
    ///   implicit (keyboard); the device routes this to its `mainKeyboard`
    ///   surface.
    /// * `state` - [`ButtonState::Down`] or [`ButtonState::Up`].
    ///
    /// To type a character that needs a modifier (uppercase, symbols), press the
    /// modifier key (e.g. `0xE1`) down, then the key down/up, then the modifier
    /// up.
    pub async fn send_keyboard(
        &mut self,
        usage_code: u64,
        state: ButtonState,
    ) -> Result<(), IdeviceError> {
        let mut payload = Dictionary::new();
        payload.insert("usageCode".into(), XPCObject::UInt64(usage_code));
        payload.insert("state".into(), XPCObject::UInt64(state.raw()));
        self.send_event(
            "IndigoKeyboardButtonEvent",
            obf!("com.apple.coredevice.feature.remote.hid.keyboard"),
            payload,
        )
        .await
    }

    /// Send an `IndigoDigitizerEvent`.
    ///
    /// This is the higher-level digitizer path (distinct from the raw report
    /// path on [`UniversalHidServiceClient`]). With `edge` = [`DigitizerEdge::None`]
    /// it is a plain touch/drag at `point_one` (and optionally a second contact
    /// `point_two`); with a non-`None` edge it becomes an edge-swipe system
    /// gesture. Coordinates are `f64` in the display's pixel space.
    pub async fn send_digitizer(
        &mut self,
        point_one: (f64, f64),
        point_two: Option<(f64, f64)>,
        event_type: DigitizerEventType,
        edge: DigitizerEdge,
        target: DigitizerTarget,
    ) -> Result<(), IdeviceError> {
        fn point(x: f64, y: f64) -> XPCObject {
            let mut p = Dictionary::new();
            p.insert("x".into(), XPCObject::Double(x));
            p.insert("y".into(), XPCObject::Double(y));
            XPCObject::Dictionary(p)
        }

        let mut payload = Dictionary::new();
        payload.insert("pointOne".into(), point(point_one.0, point_one.1));
        // `pointTwo` is an `Optional` decoded with `decodeIfPresent`; omit the
        // key entirely when there's no second contact.
        if let Some((x, y)) = point_two {
            payload.insert("pointTwo".into(), point(x, y));
        }
        payload.insert("eventType".into(), XPCObject::UInt64(event_type.raw()));
        payload.insert("edge".into(), XPCObject::UInt64(edge.raw()));
        payload.insert("target".into(), XPCObject::UInt64(target.raw()));
        self.send_event(
            "IndigoDigitizerEvent",
            obf!("com.apple.coredevice.feature.remote.hid.digitizer"),
            payload,
        )
        .await
    }

    /// Send an `IndigoScrollEvent` (digital crown / dial scrolling).
    ///
    /// * `point` - scroll delta `(x, y, z)` as `f64`.
    /// * `phase` - bitmask from [`scroll_phase`].
    /// * `momentum` - bitmask from [`scroll_momentum`].
    /// * `target` - [`ScrollTarget::DigitalCrown`] or [`ScrollTarget::Dial`].
    pub async fn send_scroll(
        &mut self,
        point: (f64, f64, f64),
        phase: u64,
        momentum: u64,
        target: ScrollTarget,
    ) -> Result<(), IdeviceError> {
        let mut p = Dictionary::new();
        p.insert("x".into(), XPCObject::Double(point.0));
        p.insert("y".into(), XPCObject::Double(point.1));
        p.insert("z".into(), XPCObject::Double(point.2));

        let mut payload = Dictionary::new();
        payload.insert("point".into(), XPCObject::Dictionary(p));
        payload.insert("phase".into(), XPCObject::UInt64(phase));
        payload.insert("momentum".into(), XPCObject::UInt64(momentum));
        payload.insert("target".into(), XPCObject::UInt64(target.raw()));
        self.send_event(
            "IndigoScrollEvent",
            obf!("com.apple.coredevice.feature.remote.hid.scroll"),
            payload,
        )
        .await
    }

    /// Send an `IndigoVendorDefinedEvent`: a raw vendor-defined HID report
    /// (routed to the device's `avpCustom` surface).
    ///
    /// * `usage_page` / `usage` - the vendor usage.
    /// * `version` - vendor event version.
    /// * `data` - the opaque report bytes.
    pub async fn send_vendor_defined(
        &mut self,
        usage_page: u64,
        usage: u64,
        version: u64,
        data: Vec<u8>,
    ) -> Result<(), IdeviceError> {
        let mut payload = Dictionary::new();
        payload.insert("usagePage".into(), XPCObject::UInt64(usage_page));
        payload.insert("usage".into(), XPCObject::UInt64(usage));
        payload.insert("version".into(), XPCObject::UInt64(version));
        payload.insert("data".into(), XPCObject::Data(data));
        self.send_event(
            "IndigoVendorDefinedEvent",
            obf!("com.apple.coredevice.feature.remote.hid.vendordefined"),
            payload,
        )
        .await
    }
}

/// A HID surface the device has registered, as returned by
/// [`UniversalHidServiceClient::list_connected_services`]. The device also
/// reports a verbose `_CoreDevice_codablePropertyStorage` mirror of these
/// fields, which this skips.
#[derive(Debug, Clone, Deserialize)]
pub struct HidSurface {
    /// The surface's identifier — the `service_id` to pass to
    /// [`UniversalHidServiceClient::send_report`].
    #[serde(rename = "_ServiceID")]
    pub service_id: u64,
    /// Human-readable product string, e.g. `"CoreDevice touchscreen(nil)"`.
    #[serde(rename = "Product")]
    pub product: Option<String>,
    /// The surface's primary HID usage.
    #[serde(rename = "PrimaryUsage")]
    pub primary_usage: Option<u64>,
    /// The surface's primary HID usage page.
    #[serde(rename = "PrimaryUsagePage")]
    pub primary_usage_page: Option<u64>,
}

fn parse_hid_surfaces(response: &plist::Value) -> Result<Vec<HidSurface>, IdeviceError> {
    let services = response
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("connectedServices"))
        .ok_or(CoreDeviceError::MissingField("connectedServices"))?;
    let surfaces: Vec<HidSurface> = plist::from_value(services)
        .map_err(|_| CoreDeviceError::MalformedField("connectedServices"))?;
    if surfaces.len() > MAX_CONNECTED_HID_SERVICES {
        return Err(CoreDeviceError::MalformedField("connectedServices").into());
    }

    let mut identifiers = BTreeSet::new();
    for surface in &surfaces {
        if surface.service_id == 0 || !identifiers.insert(surface.service_id) {
            return Err(CoreDeviceError::MalformedField("connectedServices").into());
        }
        if surface.product.as_ref().is_some_and(|product| {
            product.is_empty()
                || product.len() > MAX_HID_PRODUCT_BYTES
                || product.chars().any(char::is_control)
        }) || surface
            .primary_usage
            .is_some_and(|usage| usage > u64::from(u16::MAX))
            || surface
                .primary_usage_page
                .is_some_and(|usage_page| usage_page > u64::from(u16::MAX))
        {
            return Err(CoreDeviceError::MalformedField("connectedServices").into());
        }
    }
    Ok(surfaces)
}

fn parse_created_main_keyboard_id(response: &plist::Value) -> Result<u64, MainKeyboardError> {
    response
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("serviceID"))
        .and_then(plist::Value::as_unsigned_integer)
        .filter(|service_id| *service_id != 0)
        .ok_or(MainKeyboardError::MalformedResponse)
}

fn codable_storage_value<'a>(
    storage: &'a plist::Dictionary,
    key: &str,
    tag: &str,
) -> Option<&'a plist::Value> {
    storage
        .get(key)
        .and_then(plist::Value::as_dictionary)
        .and_then(|wrapper| wrapper.get(tag))
}

fn confirm_main_keyboard_identity(
    response: &plist::Value,
    service_id: u64,
) -> Result<(), MainKeyboardError> {
    let encoded = response
        .as_dictionary()
        .and_then(|dictionary| dictionary.get("connectedServices"))
        .and_then(plist::Value::as_array)
        .ok_or(MainKeyboardError::MalformedResponse)?;
    if encoded.len() > MAX_CONNECTED_HID_SERVICES {
        return Err(MainKeyboardError::MalformedResponse);
    }

    let mut matching = encoded.iter().filter_map(|value| {
        let dictionary = value.as_dictionary()?;
        (dictionary
            .get("_ServiceID")
            .and_then(plist::Value::as_unsigned_integer)
            == Some(service_id))
        .then_some(dictionary)
    });
    let Some(surface) = matching.next() else {
        return Err(MainKeyboardError::ServiceNotVisible);
    };
    if matching.next().is_some() {
        return Err(MainKeyboardError::AmbiguousIdentity);
    }
    if surface
        .get("PrimaryUsagePage")
        .and_then(plist::Value::as_unsigned_integer)
        != Some(MAIN_KEYBOARD_PRIMARY_USAGE_PAGE)
        || surface
            .get("PrimaryUsage")
            .and_then(plist::Value::as_unsigned_integer)
            != Some(MAIN_KEYBOARD_PRIMARY_USAGE)
    {
        return Err(MainKeyboardError::UsageMismatch);
    }

    let storage = surface
        .get("_CoreDevice_codablePropertyStorage")
        .and_then(plist::Value::as_dictionary)
        .ok_or(MainKeyboardError::IdentityEvidenceMissing)?;
    if codable_storage_value(storage, "_ServiceID", "uint")
        .and_then(plist::Value::as_unsigned_integer)
        != Some(service_id)
    {
        return Err(MainKeyboardError::IdentityEvidenceMissing);
    }
    if codable_storage_value(storage, "PrimaryUsagePage", "int")
        .and_then(plist::Value::as_unsigned_integer)
        != Some(MAIN_KEYBOARD_PRIMARY_USAGE_PAGE)
        || codable_storage_value(storage, "PrimaryUsage", "int")
            .and_then(plist::Value::as_unsigned_integer)
            != Some(MAIN_KEYBOARD_PRIMARY_USAGE)
    {
        return Err(MainKeyboardError::UsageMismatch);
    }
    if codable_storage_value(storage, "UniversalControlVirtualService", "bool")
        .and_then(plist::Value::as_boolean)
        != Some(true)
    {
        return Err(MainKeyboardError::VirtualServiceMismatch);
    }
    if codable_storage_value(storage, "ReportDescriptor", "data").and_then(plist::Value::as_data)
        != Some(MAIN_KEYBOARD_REPORT_DESCRIPTOR.as_slice())
    {
        return Err(MainKeyboardError::DescriptorMismatch);
    }
    for key in ["Product", "Manufacturer"] {
        let value = codable_storage_value(storage, key, "string")
            .and_then(plist::Value::as_string)
            .ok_or(MainKeyboardError::MetadataMissing)?;
        if value.is_empty()
            || value.len() > MAX_HID_PRODUCT_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(MainKeyboardError::MetadataInvalid);
        }
    }
    Ok(())
}

trait MainKeyboardWire {
    async fn request(&mut self, request: Dictionary) -> Result<plist::Value, MainKeyboardError>;

    async fn send(&mut self, request: Dictionary) -> Result<(), MainKeyboardError>;
}

/// Inspect and drive the device's registered HID surfaces.
#[derive(Debug)]
pub struct UniversalHidServiceClient<R: ReadWrite> {
    inner: RemoteXpcClient<R>,
}

#[cfg(feature = "rsd")]
impl crate::RsdService for UniversalHidServiceClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> Cow<'static, str> {
        obf!("com.apple.coredevice.hid.universalhidservice")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        let mut inner = RemoteXpcClient::new(stream).await?;
        inner.do_handshake().await?;
        Ok(Self { inner })
    }
}

impl<R: ReadWrite> MainKeyboardWire for UniversalHidServiceClient<R> {
    async fn request(&mut self, request: Dictionary) -> Result<plist::Value, MainKeyboardError> {
        self.inner
            .send_object(request, true)
            .await
            .map_err(|_| MainKeyboardError::Transport)?;
        self.inner
            .recv()
            .await
            .map_err(|_| MainKeyboardError::Transport)
    }

    async fn send(&mut self, request: Dictionary) -> Result<(), MainKeyboardError> {
        self.inner
            .send_object(request, false)
            .await
            .map_err(|_| MainKeyboardError::Transport)
    }
}

async fn connected_services_response(
    wire: &mut impl MainKeyboardWire,
) -> Result<plist::Value, MainKeyboardError> {
    wire.request(build_connected_services_request()).await
}

async fn list_hid_surfaces_on_wire(
    wire: &mut impl MainKeyboardWire,
) -> Result<Vec<HidSurface>, MainKeyboardError> {
    let response = connected_services_response(wire).await?;
    parse_hid_surfaces(&response).map_err(|_| MainKeyboardError::MalformedResponse)
}

async fn confirm_main_keyboard_on_wire(
    wire: &mut impl MainKeyboardWire,
    service_id: u64,
) -> Result<(), MainKeyboardError> {
    let response = connected_services_response(wire).await?;
    parse_hid_surfaces(&response).map_err(|_| MainKeyboardError::MalformedResponse)?;
    confirm_main_keyboard_identity(&response, service_id)
}

async fn create_main_keyboard_on_wire(
    wire: &mut impl MainKeyboardWire,
) -> Result<MainKeyboardService, MainKeyboardError> {
    let response = wire.request(build_main_keyboard_create_request()).await?;
    let service_id = parse_created_main_keyboard_id(&response)?;

    let confirmation = confirm_main_keyboard_on_wire(wire, service_id).await;
    if let Err(error) = confirmation {
        let rollback = wire
            .send(build_main_keyboard_service_request(
                "removeService",
                service_id,
            ))
            .await;
        if rollback.is_err() {
            return Err(MainKeyboardError::RollbackFailed);
        }
        return Err(error);
    }

    Ok(MainKeyboardService {
        service_id,
        pressed: [0; MAIN_KEYBOARD_USAGE_BITMAP_BYTES],
        active: true,
        _ownership: MainKeyboardOwnership,
    })
}

async fn replace_main_keyboard_usages_on_wire(
    wire: &mut impl MainKeyboardWire,
    service: &mut MainKeyboardService,
    pressed: [u8; MAIN_KEYBOARD_USAGE_BITMAP_BYTES],
) -> Result<(), MainKeyboardError> {
    if !service.active {
        return Err(MainKeyboardError::Inactive);
    }
    wire.send(build_send_report_request(
        service.service_id,
        build_main_keyboard_report(&pressed),
    ))
    .await?;
    service.pressed = pressed;
    Ok(())
}

async fn reset_main_keyboard_on_wire(
    wire: &mut impl MainKeyboardWire,
    service: &mut MainKeyboardService,
) -> Result<(), MainKeyboardError> {
    if !service.active {
        return Ok(());
    }
    wire.send(build_main_keyboard_service_request(
        "resetGestureState",
        service.service_id,
    ))
    .await?;
    service.pressed.fill(0);
    Ok(())
}

async fn remove_main_keyboard_on_wire(
    wire: &mut impl MainKeyboardWire,
    service: &mut MainKeyboardService,
) -> Result<(), MainKeyboardError> {
    if !service.active {
        return Ok(());
    }

    // Removal is the stronger cleanup operation, so attempt it even when the
    // preceding state reset cannot be delivered.
    let _reset_result = wire
        .send(build_main_keyboard_service_request(
            "resetGestureState",
            service.service_id,
        ))
        .await;
    wire.send(build_main_keyboard_service_request(
        "removeService",
        service.service_id,
    ))
    .await?;
    let surfaces = list_hid_surfaces_on_wire(wire).await?;
    if surfaces
        .iter()
        .any(|surface| surface.service_id == service.service_id)
    {
        return Err(MainKeyboardError::StillRegistered);
    }

    service.pressed.fill(0);
    service.active = false;
    Ok(())
}

impl<R: ReadWrite> UniversalHidServiceClient<R> {
    pub fn new(inner: RemoteXpcClient<R>) -> Self {
        Self { inner }
    }

    /// Enumerate the device's currently-registered HID surfaces.
    pub async fn list_connected_services(&mut self) -> Result<Vec<HidSurface>, IdeviceError> {
        let msg = build_connected_services_request();
        self.inner.send_object(msg, true).await?;
        let res = self.inner.recv().await?;
        parse_hid_surfaces(&res)
    }

    /// Create and confirm the fixed mainKeyboard service.
    ///
    /// If the device-assigned identity cannot be confirmed, the constructor
    /// sends a bounded rollback removal before returning an error.
    pub async fn create_main_keyboard(&mut self) -> Result<MainKeyboardService, MainKeyboardError> {
        create_main_keyboard_on_wire(self).await
    }

    /// Atomically replace the complete pressed-key set for mainKeyboard.
    pub async fn set_main_keyboard_usages(
        &mut self,
        service: &mut MainKeyboardService,
        pressed: impl IntoIterator<Item = KeyboardUsage>,
    ) -> Result<(), MainKeyboardError> {
        replace_main_keyboard_usages_on_wire(self, service, main_keyboard_usage_bitmap(pressed))
            .await
    }

    /// Add one key to the pressed-key set and send the complete bounded report.
    pub async fn main_keyboard_key_down(
        &mut self,
        service: &mut MainKeyboardService,
        usage: KeyboardUsage,
    ) -> Result<(), MainKeyboardError> {
        let mut pressed = service.pressed;
        let (byte, mask) = usage.bitmap_position();
        if pressed[byte] & mask != 0 {
            return Err(MainKeyboardError::KeyAlreadyPressed);
        }
        pressed[byte] |= mask;
        replace_main_keyboard_usages_on_wire(self, service, pressed).await
    }

    /// Remove one key from the pressed-key set and send the complete report.
    pub async fn main_keyboard_key_up(
        &mut self,
        service: &mut MainKeyboardService,
        usage: KeyboardUsage,
    ) -> Result<(), MainKeyboardError> {
        let mut pressed = service.pressed;
        let (byte, mask) = usage.bitmap_position();
        if pressed[byte] & mask == 0 {
            return Err(MainKeyboardError::KeyNotPressed);
        }
        pressed[byte] &= !mask;
        replace_main_keyboard_usages_on_wire(self, service, pressed).await
    }

    /// Reset mainKeyboard gesture/key state. Repeated reset after removal is a
    /// no-op so teardown callers may safely converge.
    pub async fn reset_main_keyboard(
        &mut self,
        service: &mut MainKeyboardService,
    ) -> Result<(), MainKeyboardError> {
        reset_main_keyboard_on_wire(self, service).await
    }

    /// Reset, remove, and confirm absence of mainKeyboard. This operation is
    /// idempotent after successful removal.
    pub async fn remove_main_keyboard(
        &mut self,
        service: &mut MainKeyboardService,
    ) -> Result<(), MainKeyboardError> {
        remove_main_keyboard_on_wire(self, service).await
    }

    /// Deliver a raw HID report to one of the device's HID surfaces.
    pub async fn send_report(
        &mut self,
        service_id: u64,
        report: Vec<u8>,
    ) -> Result<(), IdeviceError> {
        // `send` is a Swift tuple `(_0: report, _1: serviceID)`.
        let msg = build_send_report_request(service_id, report);
        self.inner.send_object(msg, false).await
    }

    /// Send a single 19-byte gesture/pointer report at (`x`, `y`).
    /// For an actual on-screen touch use
    /// [`send_touchscreen`](Self::send_touchscreen).
    pub async fn send_digitizer(
        &mut self,
        x: i32,
        y: i32,
        service_id: u64,
        timestamp: Option<u64>,
    ) -> Result<(), IdeviceError> {
        self.send_report(service_id, build_digitizer_report(x, y, timestamp))
            .await
    }

    /// Send a single 58-byte `mainTouchscreen` report. `state` is
    /// [`TOUCHSCREEN_STATE_CONTACT`] for an in-progress touch sample or
    /// [`TOUCHSCREEN_STATE_RELEASE`] to lift.
    pub async fn send_touchscreen(
        &mut self,
        state: u8,
        x: u16,
        y: u16,
        timestamp: Option<u64>,
    ) -> Result<(), IdeviceError> {
        self.send_report(
            DIGITIZER_SURFACE_MAIN_TOUCHSCREEN,
            build_touchscreen_report(state, x, y, timestamp),
        )
        .await
    }

    /// Send one `mainTouchscreen` report containing up to five contacts.
    pub async fn send_multitouch(
        &mut self,
        contacts: &[TouchscreenContact],
        timestamp: Option<u64>,
    ) -> Result<(), IdeviceError> {
        let report = build_multitouch_report(contacts, timestamp)?;
        self.send_report(DIGITIZER_SURFACE_MAIN_TOUCHSCREEN, report)
            .await
    }

    /// Tap one to five touchscreen positions in a single multi-contact frame.
    ///
    /// Contact identities are assigned in slice order. This method sends the
    /// matching release frame before returning; use
    /// [`send_multitouch`](Self::send_multitouch) directly for gestures that
    /// need movement or independently timed contacts.
    pub async fn multi_tap(&mut self, positions: &[(u16, u16)]) -> Result<(), IdeviceError> {
        let mut contacts = multitap_contacts(positions)?;
        self.send_multitouch(&contacts, None).await?;
        crate::time::sleep(std::time::Duration::from_millis(50)).await;
        contacts
            .iter_mut()
            .for_each(|contact| contact.touching = false);
        self.send_multitouch(&contacts, None).await
    }

    /// A tap on the touchscreen: one contact sample, a short hold, then a
    /// release at the same point.
    pub async fn tap(&mut self, x: u16, y: u16) -> Result<(), IdeviceError> {
        self.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x, y, None)
            .await?;
        crate::time::sleep(std::time::Duration::from_millis(50)).await;
        self.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x, y, None)
            .await
    }

    /// A drag on the touchscreen from (`x1`, `y1`) to (`x2`, `y2`): a stream of
    /// `steps` contact samples advancing linearly, a final contact at the end
    /// point, then a release. `delay_ms` is slept between samples so the gesture
    /// recognizer sees a velocity (a too-fast drag reads as a tap). This is the
    /// real touch-drag used for scrolling/swiping content. `steps` is clamped to
    /// at least 1.
    pub async fn drag(
        &mut self,
        x1: u16,
        y1: u16,
        x2: u16,
        y2: u16,
        steps: u32,
        delay_ms: u64,
    ) -> Result<(), IdeviceError> {
        let steps = steps.max(1);
        for i in 0..steps {
            let t = i as f64 / steps as f64;
            let x = (x1 as f64 + (x2 as f64 - x1 as f64) * t).round() as u16;
            let y = (y1 as f64 + (y2 as f64 - y1 as f64) * t).round() as u16;
            self.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x, y, None)
                .await?;
            if delay_ms > 0 {
                crate::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
        self.send_touchscreen(TOUCHSCREEN_STATE_CONTACT, x2, y2, None)
            .await?;
        self.send_touchscreen(TOUCHSCREEN_STATE_RELEASE, x2, y2, None)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[derive(Default)]
    struct FakeMainKeyboardWire {
        replies: VecDeque<Result<plist::Value, MainKeyboardError>>,
        send_results: VecDeque<Result<(), MainKeyboardError>>,
        sent: Vec<(Dictionary, bool)>,
    }

    impl MainKeyboardWire for FakeMainKeyboardWire {
        async fn request(
            &mut self,
            request: Dictionary,
        ) -> Result<plist::Value, MainKeyboardError> {
            self.sent.push((request, true));
            self.replies
                .pop_front()
                .unwrap_or(Err(MainKeyboardError::MalformedResponse))
        }

        async fn send(&mut self, request: Dictionary) -> Result<(), MainKeyboardError> {
            self.sent.push((request, false));
            self.send_results.pop_front().unwrap_or(Ok(()))
        }
    }

    fn created_service(service_id: u64) -> plist::Value {
        crate::plist!({ "serviceID": service_id })
    }

    fn connected_services(surfaces: Vec<plist::Value>) -> plist::Value {
        crate::plist!({ "connectedServices": surfaces })
    }

    fn hid_surface(
        service_id: u64,
        product: &str,
        primary_usage_page: u64,
        primary_usage: u64,
    ) -> plist::Value {
        crate::plist!({
            "_ServiceID": service_id,
            "Product": product,
            "PrimaryUsagePage": primary_usage_page,
            "PrimaryUsage": primary_usage,
            "_CoreDevice_codablePropertyStorage": {
                "_ServiceID": { "uint": service_id },
                "Product": { "string": product },
                "Manufacturer": { "string": MAIN_KEYBOARD_MANUFACTURER },
                "PrimaryUsagePage": { "int": primary_usage_page },
                "PrimaryUsage": { "int": primary_usage },
                "UniversalControlVirtualService": { "bool": true },
                "ReportDescriptor": { "data": MAIN_KEYBOARD_REPORT_DESCRIPTOR.to_vec() },
            },
        })
    }

    fn codable_storage_mut(surface: &mut plist::Value) -> &mut plist::Dictionary {
        surface
            .as_dictionary_mut()
            .and_then(|surface| surface.get_mut("_CoreDevice_codablePropertyStorage"))
            .and_then(plist::Value::as_dictionary_mut)
            .expect("codable storage")
    }

    fn operation_payload<'a>(request: &'a Dictionary, operation: &str) -> &'a Dictionary {
        request
            .get("payload")
            .and_then(XPCObject::as_dictionary)
            .and_then(|payload| payload.get(operation))
            .and_then(XPCObject::as_dictionary)
            .expect("operation payload")
    }

    fn hid_input_bits_for_report(descriptor: &[u8], target_report_id: u8) -> usize {
        let mut offset = 0;
        let mut report_id = 0;
        let mut report_size = 0;
        let mut report_count = 0;
        let mut input_bits = 0;

        while offset < descriptor.len() {
            let prefix = descriptor[offset];
            offset += 1;
            assert_ne!(
                prefix, 0xFE,
                "long HID items are not supported by this test"
            );
            let data_len = match prefix & 0x03 {
                0 => 0,
                1 => 1,
                2 => 2,
                3 => 4,
                _ => unreachable!(),
            };
            assert!(offset + data_len <= descriptor.len());
            let value = descriptor[offset..offset + data_len]
                .iter()
                .enumerate()
                .fold(0usize, |value, (index, byte)| {
                    value | (usize::from(*byte) << (index * 8))
                });
            offset += data_len;

            let item_type = (prefix >> 2) & 0x03;
            let item_tag = prefix >> 4;
            match (item_type, item_tag) {
                (1, 7) => report_size = value,
                (1, 8) => report_id = value as u8,
                (1, 9) => report_count = value,
                (0, 8) if report_id == target_report_id => {
                    input_bits += report_size * report_count;
                }
                _ => {}
            }
        }

        input_bits
    }

    #[test]
    fn digitizer_report_layout() {
        let r = build_digitizer_report(100, -50, Some(0x0102030405));
        assert_eq!(r.len(), 19);
        assert_eq!(r[0], DIGITIZER_REPORT_ID);
        assert_eq!(&r[1..5], &100i32.to_le_bytes());
        assert_eq!(&r[5..9], &(-50i32).to_le_bytes());
        assert_eq!(&r[9..11], &[0, 0]);
        assert_eq!(&r[11..17], &0x0102030405u64.to_le_bytes()[..6]);
        assert_eq!(&r[17..19], &[0, 0]);
    }

    #[test]
    fn touchscreen_report_layout() {
        let r = build_touchscreen_report(TOUCHSCREEN_STATE_CONTACT, 375, 812, Some(0xAABBCCDD));
        assert_eq!(r.len(), 58);
        assert_eq!(&r[0..4], &[0x09, 0x01, 0x05, 0xC2]);
        assert_eq!(&r[4..6], &375u16.to_le_bytes());
        assert_eq!(&r[6..8], &812u16.to_le_bytes());
        assert_eq!(&r[8..40], &[0u8; 32]);
        assert_eq!(&r[40..44], &[0x02, 0x00, 0x00, 0x00]);
        assert_eq!(&r[44..50], &0xAABBCCDDu64.to_le_bytes()[..6]);
        assert_eq!(&r[50..58], &[0u8; 8]);
    }

    #[test]
    fn timestamp_is_truncated_to_48_bits() {
        // A timestamp above 48 bits must be masked, not overflow the 6-byte field.
        let r = build_digitizer_report(0, 0, Some(u64::MAX));
        assert_eq!(&r[11..17], &[0xFF; 6]);
    }

    #[test]
    fn multitouch_report_layout() {
        let r = build_multitouch_report(
            &[
                TouchscreenContact {
                    identity: 2,
                    touching: true,
                    x: 0x1234,
                    y: 0x5678,
                },
                TouchscreenContact {
                    identity: 3,
                    touching: false,
                    x: 0x9ABC,
                    y: 0xDEF0,
                },
            ],
            Some(0x0102030405),
        )
        .unwrap();

        assert_eq!(r.len(), 58);
        assert_eq!(&r[..3], &[0x09, 0x02, 0x05]);
        assert_eq!(&r[3..8], &[0xC2, 0x34, 0x12, 0x78, 0x56]);
        assert_eq!(&r[8..13], &[0x03, 0xBC, 0x9A, 0xF0, 0xDE]);
        assert_eq!(&r[13..40], &[0; 27]);
        assert_eq!(&r[40..44], &[0x02, 0x00, 0x00, 0x00]);
        assert_eq!(&r[44..50], &0x0102030405u64.to_le_bytes()[..6]);
        assert_eq!(&r[50..], &[0; 8]);
    }

    #[test]
    fn multitouch_report_accepts_five_contacts() {
        let contacts = [0, 1, 2, 3, 4].map(|identity| TouchscreenContact {
            identity,
            touching: true,
            x: identity as u16,
            y: identity as u16,
        });
        let r = build_multitouch_report(&contacts, Some(0)).unwrap();
        assert_eq!(&r[..3], &[TOUCHSCREEN_REPORT_ID, 5, 5]);
        assert_eq!(r[23], 0xC4);
    }

    #[test]
    fn multitouch_report_accepts_an_empty_frame() {
        let r = build_multitouch_report(&[], Some(0)).unwrap();
        assert_eq!(&r[..3], &[TOUCHSCREEN_REPORT_ID, 0, 5]);
        assert_eq!(&r[3..40], &[0; 37]);
    }

    #[test]
    fn multitouch_report_validates_contacts() {
        let contact = TouchscreenContact {
            identity: 1,
            touching: true,
            x: 0,
            y: 0,
        };
        assert!(matches!(
            build_multitouch_report(&[contact; 6], None),
            Err(CoreDeviceError::TooManyTouchscreenContacts(6))
        ));
        assert!(matches!(
            build_multitouch_report(&[contact, contact], None),
            Err(CoreDeviceError::DuplicateTouchscreenContactIdentity(1))
        ));
        assert!(matches!(
            build_multitouch_report(
                &[TouchscreenContact {
                    identity: 5,
                    ..contact
                }],
                None
            ),
            Err(CoreDeviceError::InvalidTouchscreenContactIdentity(5))
        ));
    }

    #[test]
    fn multitouch_tap_validates_positions_before_io() {
        assert!(matches!(
            multitap_contacts(&[]),
            Err(CoreDeviceError::NoTouchscreenContacts)
        ));
        assert!(matches!(
            multitap_contacts(&[(0, 0); 6]),
            Err(CoreDeviceError::TooManyTouchscreenContacts(6))
        ));
        let contacts = multitap_contacts(&[(10, 20), (30, 40)]).unwrap();
        assert_eq!(contacts[0].identity, 0);
        assert_eq!(contacts[1].identity, 1);
        assert!(contacts.iter().all(|contact| contact.touching));
    }

    #[test]
    fn main_keyboard_create_request_is_fixed_and_bounded() {
        let request = build_main_keyboard_create_request();
        assert_eq!(
            request.get("featureIdentifier"),
            Some(&XPCObject::String(
                "com.apple.coredevice.feature.remote.universalhidservice".into()
            ))
        );
        assert_eq!(
            request.get("messageType"),
            Some(&XPCObject::String("Request".into()))
        );

        let properties = operation_payload(&request, "createService")
            .get("_0")
            .and_then(XPCObject::as_dictionary)
            .expect("fixed descriptor");
        assert_eq!(
            properties.get("Product"),
            Some(&XPCObject::String(MAIN_KEYBOARD_PRODUCT.into()))
        );
        assert_eq!(
            properties.get("PrimaryUsagePage"),
            Some(&XPCObject::UInt64(1))
        );
        assert_eq!(properties.get("PrimaryUsage"), Some(&XPCObject::UInt64(6)));

        let storage = properties
            .get("_CoreDevice_codablePropertyStorage")
            .and_then(XPCObject::as_dictionary)
            .expect("codable storage");
        assert_eq!(
            storage
                .get("ReportDescriptor")
                .and_then(XPCObject::as_dictionary)
                .and_then(|value| value.get("data")),
            Some(&XPCObject::Data(MAIN_KEYBOARD_REPORT_DESCRIPTOR.to_vec()))
        );
        assert_eq!(
            storage
                .get("UniversalControlVirtualService")
                .and_then(XPCObject::as_dictionary)
                .and_then(|value| value.get("bool")),
            Some(&XPCObject::Bool(true))
        );
    }

    #[test]
    fn main_keyboard_descriptor_matches_the_report_layout() {
        assert_eq!(MAIN_KEYBOARD_REPORT_DESCRIPTOR.len(), 56);
        assert_eq!(
            &MAIN_KEYBOARD_REPORT_DESCRIPTOR[..2],
            &[0x85, MAIN_KEYBOARD_REPORT_ID]
        );

        let input_bits =
            hid_input_bits_for_report(&MAIN_KEYBOARD_REPORT_DESCRIPTOR, MAIN_KEYBOARD_REPORT_ID);
        assert_eq!(input_bits, 304);
        assert_eq!(input_bits % 8, 0);
        assert_eq!(input_bits / 8 + 1, MAIN_KEYBOARD_REPORT_SIZE);

        let report = build_main_keyboard_report(&[0; MAIN_KEYBOARD_USAGE_BITMAP_BYTES]);
        assert_eq!(report.len(), MAIN_KEYBOARD_REPORT_SIZE);
        assert_eq!(report[0], MAIN_KEYBOARD_REPORT_ID);
        assert_eq!(report[30], 0);
        assert_eq!(&report[37..], &[0, 0]);
    }

    #[tokio::test]
    async fn main_keyboard_constructor_confirms_the_assigned_identity() {
        let assigned = 0x1_0000_3001;
        let mut wire = FakeMainKeyboardWire {
            replies: VecDeque::from([
                Ok(created_service(assigned)),
                Ok(connected_services(vec![hid_surface(
                    assigned,
                    MAIN_KEYBOARD_PRODUCT,
                    MAIN_KEYBOARD_PRIMARY_USAGE_PAGE,
                    MAIN_KEYBOARD_PRIMARY_USAGE,
                )])),
            ]),
            ..FakeMainKeyboardWire::default()
        };

        let service = create_main_keyboard_on_wire(&mut wire).await.unwrap();

        assert!(service.is_active());
        assert_eq!(service.pressed_count(), 0);
        assert_eq!(wire.sent.len(), 2);
        assert!(wire.sent.iter().all(|(_, expects_reply)| *expects_reply));
        let debug = format!("{service:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(&assigned.to_string()));
    }

    #[tokio::test]
    async fn main_keyboard_constructor_rolls_back_an_identity_mismatch() {
        let assigned = 0x1_0000_3001;
        let mut mismatched = hid_surface(
            assigned,
            MAIN_KEYBOARD_PRODUCT,
            MAIN_KEYBOARD_PRIMARY_USAGE_PAGE,
            MAIN_KEYBOARD_PRIMARY_USAGE,
        );
        codable_storage_mut(&mut mismatched)
            .get_mut("ReportDescriptor")
            .and_then(plist::Value::as_dictionary_mut)
            .unwrap()
            .insert("data".into(), plist::Value::Data(vec![0]));
        let mut wire = FakeMainKeyboardWire {
            replies: VecDeque::from([
                Ok(created_service(assigned)),
                Ok(connected_services(vec![mismatched])),
            ]),
            ..FakeMainKeyboardWire::default()
        };

        assert_eq!(
            create_main_keyboard_on_wire(&mut wire).await.unwrap_err(),
            MainKeyboardError::DescriptorMismatch
        );
        assert_eq!(wire.sent.len(), 3);
        assert!(!wire.sent[2].1);
        assert_eq!(
            operation_payload(&wire.sent[2].0, "removeService").get("_0"),
            Some(&XPCObject::UInt64(assigned))
        );
    }

    #[test]
    fn main_keyboard_identity_diagnostics_remain_unique_and_content_free() {
        let assigned = 0x1_0000_3001;
        let expected = hid_surface(
            assigned,
            MAIN_KEYBOARD_PRODUCT,
            MAIN_KEYBOARD_PRIMARY_USAGE_PAGE,
            MAIN_KEYBOARD_PRIMARY_USAGE,
        );
        let mut missing_metadata = expected.clone();
        codable_storage_mut(&mut missing_metadata).remove("Product");
        let mut invalid_metadata = expected.clone();
        codable_storage_mut(&mut invalid_metadata)
            .get_mut("Manufacturer")
            .and_then(plist::Value::as_dictionary_mut)
            .unwrap()
            .insert(
                "string".into(),
                plist::Value::String("invalid\nmetadata".into()),
            );
        let failures = [
            confirm_main_keyboard_identity(&connected_services(Vec::new()), assigned).unwrap_err(),
            confirm_main_keyboard_identity(
                &connected_services(vec![expected.clone(), expected]),
                assigned,
            )
            .unwrap_err(),
            confirm_main_keyboard_identity(
                &connected_services(vec![hid_surface(assigned, MAIN_KEYBOARD_PRODUCT, 1, 7)]),
                assigned,
            )
            .unwrap_err(),
            confirm_main_keyboard_identity(&connected_services(vec![missing_metadata]), assigned)
                .unwrap_err(),
            confirm_main_keyboard_identity(&connected_services(vec![invalid_metadata]), assigned)
                .unwrap_err(),
        ];

        assert_eq!(failures[0], MainKeyboardError::ServiceNotVisible);
        assert_eq!(failures[1], MainKeyboardError::AmbiguousIdentity);
        assert_eq!(failures[2], MainKeyboardError::UsageMismatch);
        assert_eq!(failures[3], MainKeyboardError::MetadataMissing);
        assert_eq!(failures[4], MainKeyboardError::MetadataInvalid);
        for error in failures {
            let rendered = error.to_string();
            assert!(!rendered.contains(&assigned.to_string()));
            assert!(!rendered.contains("invalid\nmetadata"));
        }
    }

    #[test]
    fn main_keyboard_confirmation_accepts_a_device_normalized_top_level_product() {
        let assigned = 0x1_0000_3001;
        let mut surface = hid_surface(
            assigned,
            MAIN_KEYBOARD_PRODUCT,
            MAIN_KEYBOARD_PRIMARY_USAGE_PAGE,
            MAIN_KEYBOARD_PRIMARY_USAGE,
        );
        surface
            .as_dictionary_mut()
            .unwrap()
            .insert("Product".into(), "device-normalized keyboard".into());
        let storage = codable_storage_mut(&mut surface);
        storage
            .get_mut("Product")
            .and_then(plist::Value::as_dictionary_mut)
            .unwrap()
            .insert("string".into(), "normalized keyboard".into());
        storage
            .get_mut("Manufacturer")
            .and_then(plist::Value::as_dictionary_mut)
            .unwrap()
            .insert("string".into(), "normalized manufacturer".into());
        let response = connected_services(vec![surface]);

        assert_eq!(confirm_main_keyboard_identity(&response, assigned), Ok(()));
    }

    #[tokio::test]
    async fn main_keyboard_constructor_reports_a_failed_rollback() {
        let assigned = 0x1_0000_3001;
        let mut wire = FakeMainKeyboardWire {
            replies: VecDeque::from([
                Ok(created_service(assigned)),
                Ok(connected_services(Vec::new())),
            ]),
            send_results: VecDeque::from([Err(MainKeyboardError::Transport)]),
            ..FakeMainKeyboardWire::default()
        };

        assert_eq!(
            create_main_keyboard_on_wire(&mut wire).await.unwrap_err(),
            MainKeyboardError::RollbackFailed
        );
    }

    #[test]
    fn main_keyboard_response_and_connected_service_bounds_fail_closed() {
        for malformed in [
            crate::plist!({}),
            crate::plist!({ "serviceID": 0_u64 }),
            crate::plist!({ "serviceID": "4097" }),
            plist::Value::String("not a dictionary".into()),
        ] {
            assert_eq!(
                parse_created_main_keyboard_id(&malformed),
                Err(MainKeyboardError::MalformedResponse)
            );
        }

        let too_many = connected_services(
            (1..=MAX_CONNECTED_HID_SERVICES + 1)
                .map(|service_id| hid_surface(service_id as u64, "bounded surface", 1, 6))
                .collect(),
        );
        assert!(parse_hid_surfaces(&too_many).is_err());

        let duplicate = connected_services(vec![
            hid_surface(42, "one", 1, 6),
            hid_surface(42, "two", 1, 6),
        ]);
        assert!(parse_hid_surfaces(&duplicate).is_err());
    }

    #[tokio::test]
    async fn main_keyboard_key_state_reset_and_remove_are_owned_and_idempotent() {
        let service_id = 0x1_0000_3001;
        let mut service = MainKeyboardService {
            service_id,
            pressed: [0; MAIN_KEYBOARD_USAGE_BITMAP_BYTES],
            active: true,
            _ownership: MainKeyboardOwnership,
        };
        let mut wire = FakeMainKeyboardWire {
            replies: VecDeque::from([Ok(connected_services(Vec::new()))]),
            ..FakeMainKeyboardWire::default()
        };
        let letter_a = KeyboardUsage::new(0x04).unwrap();
        let left_shift = KeyboardUsage::new(0xE1).unwrap();

        replace_main_keyboard_usages_on_wire(
            &mut wire,
            &mut service,
            main_keyboard_usage_bitmap([letter_a, left_shift]),
        )
        .await
        .unwrap();
        assert_eq!(service.pressed_count(), 2);
        let send = operation_payload(&wire.sent[0].0, "send");
        let report = send
            .get("_0")
            .and_then(|value| match value {
                XPCObject::Data(report) => Some(report),
                _ => None,
            })
            .expect("private keyboard report");
        assert_eq!(report.len(), 39);
        assert_eq!(report[0], MAIN_KEYBOARD_REPORT_ID);
        assert_eq!(report[1], 0b0001_0000);
        assert_eq!(report[29], 0b0000_0010);
        assert_eq!(send.get("_1"), Some(&XPCObject::UInt64(service_id)));

        reset_main_keyboard_on_wire(&mut wire, &mut service)
            .await
            .unwrap();
        assert_eq!(service.pressed_count(), 0);
        remove_main_keyboard_on_wire(&mut wire, &mut service)
            .await
            .unwrap();
        assert!(!service.is_active());
        assert_eq!(wire.sent.len(), 5);
        assert_eq!(
            operation_payload(&wire.sent[1].0, "resetGestureState").get("_0"),
            Some(&XPCObject::UInt64(service_id))
        );
        assert_eq!(
            operation_payload(&wire.sent[3].0, "removeService").get("_0"),
            Some(&XPCObject::UInt64(service_id))
        );

        remove_main_keyboard_on_wire(&mut wire, &mut service)
            .await
            .unwrap();
        assert_eq!(wire.sent.len(), 5);
    }

    #[test]
    fn main_keyboard_usage_validation_matches_the_system_descriptor() {
        assert_eq!(KeyboardUsage::new(1).unwrap().raw(), 1);
        assert_eq!(KeyboardUsage::new(231).unwrap().raw(), 231);
        assert_eq!(KeyboardUsage::new(0), Err(MainKeyboardError::InvalidUsage));
        assert_eq!(
            KeyboardUsage::new(232),
            Err(MainKeyboardError::InvalidUsage)
        );
        assert_eq!(
            KeyboardUsage::new(u16::MAX),
            Err(MainKeyboardError::InvalidUsage)
        );
    }
}
