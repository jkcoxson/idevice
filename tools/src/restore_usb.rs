//! nusb-backed `RecoveryTransport` for the restore CLI.

use std::time::Duration;

use idevice::{
    IdeviceError,
    restore::{
        RestoreError,
        recovery::{ControlSetup, DeviceInfo, Mode, RecoveryFuture, RecoveryTransport},
    },
};
use nusb::transfer::{Buffer, Bulk, ControlIn, ControlOut, ControlType, Out, Recipient};

const APPLE_VENDOR_ID: u16 = 0x05AC;

/// A `RecoveryTransport` implemented over an opened `nusb` device.
pub struct NusbRecoveryTransport {
    device: nusb::Device,
    interface: Option<nusb::Interface>,
    product_id: u16,
    serial: String,
}

impl std::fmt::Debug for NusbRecoveryTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NusbRecoveryTransport")
            .field("product_id", &format_args!("{:#06x}", self.product_id))
            .finish_non_exhaustive()
    }
}

/// Decodes a `bmRequestType` byte into nusb's typed direction/type/recipient.
fn decode_request_type(rt: u8) -> (bool, ControlType, Recipient) {
    let is_in = rt & 0x80 != 0;
    let control_type = match (rt >> 5) & 0b11 {
        0 => ControlType::Standard,
        1 => ControlType::Class,
        _ => ControlType::Vendor,
    };
    let recipient = match rt & 0x1F {
        1 => Recipient::Interface,
        2 => Recipient::Endpoint,
        _ => Recipient::Device,
    };
    (is_in, control_type, recipient)
}

fn usb_err<E: std::fmt::Display>(e: E) -> IdeviceError {
    IdeviceError::Restore(RestoreError::Recovery(format!("usb: {e}")))
}

impl NusbRecoveryTransport {
    fn timeout(ms: u32) -> Duration {
        Duration::from_millis(ms as u64)
    }
}

impl RecoveryTransport for NusbRecoveryTransport {
    fn control_out<'a>(
        &'a mut self,
        setup: ControlSetup,
        data: &'a [u8],
        timeout_ms: u32,
    ) -> RecoveryFuture<'a, usize> {
        let device = self.device.clone();
        let data = data.to_vec();
        Box::pin(async move {
            let (_, control_type, recipient) = decode_request_type(setup.request_type);
            device
                .control_out(
                    ControlOut {
                        control_type,
                        recipient,
                        request: setup.request,
                        value: setup.value,
                        index: setup.index,
                        data: &data,
                    },
                    Self::timeout(timeout_ms),
                )
                .await
                .map_err(usb_err)?;
            Ok(data.len())
        })
    }

    fn control_in<'a>(
        &'a mut self,
        setup: ControlSetup,
        length: u16,
        timeout_ms: u32,
    ) -> RecoveryFuture<'a, Vec<u8>> {
        let device = self.device.clone();
        Box::pin(async move {
            let (_, control_type, recipient) = decode_request_type(setup.request_type);
            let data = device
                .control_in(
                    ControlIn {
                        control_type,
                        recipient,
                        request: setup.request,
                        value: setup.value,
                        index: setup.index,
                        length,
                    },
                    Self::timeout(timeout_ms),
                )
                .await
                .map_err(usb_err)?;
            Ok(data)
        })
    }

    fn bulk_out<'a>(
        &'a mut self,
        endpoint: u8,
        data: &'a [u8],
        _timeout_ms: u32,
    ) -> RecoveryFuture<'a, usize> {
        let interface = self.interface.clone();
        let data = data.to_vec();
        Box::pin(async move {
            let interface = interface.ok_or_else(|| {
                IdeviceError::Restore(RestoreError::Recovery(
                    "no claimed interface for bulk".into(),
                ))
            })?;
            let mut ep = interface.endpoint::<Bulk, Out>(endpoint).map_err(usb_err)?;
            let len = data.len();
            ep.submit(Buffer::from(data));
            let completion = ep.next_complete().await;
            completion.status.map_err(usb_err)?;
            Ok(len)
        })
    }

    fn serial_number(&mut self) -> RecoveryFuture<'_, String> {
        let serial = self.serial.clone();
        Box::pin(async move { Ok(serial) })
    }

    fn product_id(&self) -> u16 {
        self.product_id
    }

    fn set_configuration(&mut self, configuration: u8) -> RecoveryFuture<'_, ()> {
        let device = self.device.clone();
        Box::pin(async move {
            device
                .set_configuration(configuration)
                .await
                .map_err(usb_err)
        })
    }

    fn claim_interface(&mut self, interface: u8, alt_setting: u8) -> RecoveryFuture<'_, ()> {
        Box::pin(async move {
            let iface = self
                .device
                .claim_interface(interface)
                .await
                .map_err(usb_err)?;
            if alt_setting != 0 {
                iface.set_alt_setting(alt_setting).await.map_err(usb_err)?;
            }
            self.interface = Some(iface);
            Ok(())
        })
    }

    fn reset(&mut self) -> RecoveryFuture<'_, ()> {
        let device = self.device.clone();
        Box::pin(async move { device.reset().await.map_err(usb_err) })
    }
}

/// Scans USB for an Apple recovery/DFU device, optionally matching `ecid`,
/// retrying until `timeout` elapses. Returns an opened transport.
pub async fn find_recovery_transport(
    ecid: Option<u64>,
    timeout: Duration,
) -> Result<Box<dyn RecoveryTransport>, IdeviceError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        let devices = nusb::list_devices().await.map_err(usb_err)?;
        for info in devices {
            if info.vendor_id() != APPLE_VENDOR_ID {
                continue;
            }
            if Mode::from_product_id(info.product_id()).is_none() {
                continue;
            }
            let serial = info.serial_number().unwrap_or_default().to_string();
            if let Some(want) = ecid {
                let found = DeviceInfo::parse(&serial).ecid;
                if found != Some(want) {
                    continue;
                }
            }
            // The device may still be held by the system (or our just-dropped
            // handle) right after re-enumeration; retry rather than abort.
            let device = match info.open().await {
                Ok(d) => d,
                Err(_) => continue,
            };
            return Ok(Box::new(NusbRecoveryTransport {
                product_id: info.product_id(),
                serial,
                device,
                interface: None,
            }));
        }

        if std::time::Instant::now() >= deadline {
            return Err(IdeviceError::DeviceNotFound);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
