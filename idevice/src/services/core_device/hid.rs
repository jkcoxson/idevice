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
use std::borrow::Cow;
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

impl<R: ReadWrite> UniversalHidServiceClient<R> {
    pub fn new(inner: RemoteXpcClient<R>) -> Self {
        Self { inner }
    }

    /// Build the `{featureIdentifier, messageType: "Request", payload}` envelope
    /// these requests share.
    fn request(payload: Dictionary) -> Dictionary {
        let mut msg = Dictionary::new();
        let universal_hid_feature: Cow<'static, str> =
            obf!("com.apple.coredevice.feature.remote.universalhidservice");
        msg.insert(
            "featureIdentifier".into(),
            XPCObject::String(universal_hid_feature.into()),
        );
        msg.insert("messageType".into(), XPCObject::String("Request".into()));
        msg.insert("payload".into(), XPCObject::Dictionary(payload));
        msg
    }

    /// Enumerate the device's currently-registered HID surfaces.
    pub async fn list_connected_services(&mut self) -> Result<Vec<HidSurface>, IdeviceError> {
        let mut payload = Dictionary::new();
        payload.insert(
            "connectedServices".into(),
            XPCObject::Dictionary(Dictionary::new()),
        );
        let msg = Self::request(payload);
        self.inner.send_object(msg, true).await?;
        let res = self.inner.recv().await?;

        let services = res
            .as_dictionary()
            .and_then(|d| d.get("connectedServices"))
            .ok_or(CoreDeviceError::MissingField("connectedServices"))?;
        plist::from_value(services)
            .map_err(|_| CoreDeviceError::MalformedField("connectedServices").into())
    }

    /// Deliver a raw HID report to one of the device's HID surfaces.
    pub async fn send_report(
        &mut self,
        service_id: u64,
        report: Vec<u8>,
    ) -> Result<(), IdeviceError> {
        // `send` is a Swift tuple `(_0: report, _1: serviceID)`.
        let payload = crate::xpc!({
            "send": {
                "_0": report,
                "_1": service_id
            }
        })
        .to_dictionary()
        .unwrap();

        let msg = Self::request(payload);
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
}
