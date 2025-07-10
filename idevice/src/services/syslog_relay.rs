//! iOS Device SyslogRelay Service Abstraction

use crate::{lockdown::LockdownClient, obf, Idevice, IdeviceError, IdeviceService};

/// Client for interacting with the iOS device SyslogRelay service
pub struct SyslogRelayClient {
    /// The underlying device connection with established SyslogRelay service
    pub idevice: Idevice,
}

impl IdeviceService for SyslogRelayClient {
    /// Returns the SyslogRelay service name as registered with lockdownd
    fn service_name() -> &'static str {
        obf!("com.apple.syslog_relay")
    }

    /// Establishes a connection to the SyslogRelay service
    ///
    /// # Arguments
    /// * `provider` - Device connection provider
    ///
    /// # Returns
    /// A connected `SyslogRelayClient` instance
    ///
    /// # Errors
    /// Returns `IdeviceError` if any step of the connection process fails
    ///
    /// # Process
    /// 1. Connects to lockdownd service
    /// 2. Starts a lockdown session
    /// 3. Requests the SyslogRelay service port
    /// 4. Establishes connection to the SyslogRelay port
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

impl SyslogRelayClient {
    /// Creates a new SyslogRelay client from an existing device connection
    ///
    /// # Arguments
    /// * `idevice` - Pre-established device connection
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Get the next log from the relay
    ///
    /// # Returns
    /// A string containing the log
    ///
    /// # Errors
    /// UnexpectedResponse if the service sends an EOF
    pub async fn next(&mut self) -> Result<String, IdeviceError> {
        let res = self.idevice.read_until_delim(b"\n\x00").await?;
        match res {
            Some(res) => Ok(String::from_utf8_lossy(&res).to_string()),
            None => Err(IdeviceError::UnexpectedResponse),
        }
    }
}
