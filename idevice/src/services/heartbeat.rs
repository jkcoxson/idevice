//! iOS Device Heartbeat Service Abstraction
//!
//! iOS automatically closes service connections if there is no heartbeat client connected and
//! responding.

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the iOS device heartbeat service
///
/// The heartbeat service provides a keep-alive mechanism and can notify when
/// the device enters sleep mode or disconnects.
/// Note that a running heartbeat client is required to access other services on the device.
/// Implements the standard "Marco-Polo" protocol
/// where the host sends "Polo" in response to the device's "Marco".
pub struct HeartbeatClient {
    /// The underlying device connection with established heartbeat service
    pub idevice: Idevice,
}

impl IdeviceService for HeartbeatClient {
    /// Returns the heartbeat service name as registered with lockdownd
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.mobile.heartbeat")
    }

    /// Establishes a connection to the heartbeat service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `HeartbeatClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the heartbeat service port
    /// 4. Establishes connection to the heartbeat port
    /// 5. Optionally starts TLS if required by service
    async fn connect(
        provider: &dyn crate::provider::IdeviceProvider,
    ) -> Result<Self, IdeviceError> {
        let mut lockdown = LockdownClient::connect(provider).await?;
        lockdown
            .start_session(&provider.get_pairing_file().await?)
            .await?;

        let (port, ssl) = lockdown.start_service(Self::service_name()).await?;

        let mut idevice = provider.connect(port).await?;
        if ssl {
            idevice
                .start_session(&provider.get_pairing_file().await?)
                .await?;
        }

        Ok(Self { idevice })
    }
}

impl HeartbeatClient {
    /// Creates a new heartbeat client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Waits for and processes a "Marco" message from the device
    ///
    /// This will either:
    /// - Return the heartbeat interval if received
    /// - Return a timeout error if no message received in time
    /// - Return a sleep notification if device is going to sleep
    ///
    /// # Arguments
    /// * `interval` - Timeout in seconds to wait for message
    ///
    /// # Returns
    /// The heartbeat interval in seconds if successful
    ///
    /// # Errors
    /// - `HeartbeatTimeout` if no message received before interval
    /// - `HeartbeatSleepyTime` if device is going to sleep
    /// - `UnexpectedResponse` for malformed messages
    pub async fn get_marco(&mut self, interval: u64) -> Result<u64, IdeviceError> {
        // Get a plist or wait for the interval
        let rec = tokio::select! {
            rec = self.idevice.read_plist() => rec?,
            _ = tokio::time::sleep(tokio::time::Duration::from_secs(interval)) => {
                return Err(IdeviceError::HeartbeatTimeout)
            }
        };
        match rec.get("Interval") {
            Some(plist::Value::Integer(interval)) => {
                if let Some(interval) = interval.as_unsigned() {
                    Ok(interval)
                } else {
                    Err(IdeviceError::UnexpectedResponse)
                }
            }
            _ => match rec.get("Command") {
                Some(plist::Value::String(command)) => {
                    if command.as_str() == "SleepyTime" {
                        Err(IdeviceError::HeartbeatSleepyTime)
                    } else {
                        Err(IdeviceError::UnexpectedResponse)
                    }
                }
                _ => Err(IdeviceError::UnexpectedResponse),
            },
        }
    }

    /// Sends a "Polo" response to the device
    ///
    /// This acknowledges receipt of a "Marco" message and maintains
    /// the connection keep-alive.
    ///
    /// # Errors
    /// Returns `IdeviceError` if the message fails to send
    pub async fn send_polo(&mut self) -> Result<(), IdeviceError> {
        let mut req = plist::Dictionary::new();
        req.insert("Command".into(), "Polo".into());
        self.idevice
            .send_plist(plist::Value::Dictionary(req.clone()))
            .await?;
        Ok(())
    }
}
