//! The `remotepairingdeviced` control channel, reached over USB through the
//! `com.apple.dt.remotepairingdeviced.lockdown` lockdown service.

use crate::{Idevice, IdeviceError, IdeviceService, ReadWrite, obf};

use super::{RemotePairingClient, RpPairingSocket};

/// A connection to the `remotepairingdeviced` control channel.
#[derive(Debug)]
pub struct RemotePairingLockdownService {
    /// The underlying lockdown connection.
    pub idevice: Idevice,
}

impl IdeviceService for RemotePairingLockdownService {
    fn service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.dt.remotepairingdeviced.lockdown")
    }

    async fn from_stream(idevice: Idevice) -> Result<Self, IdeviceError> {
        Ok(Self { idevice })
    }
}

impl RemotePairingLockdownService {
    pub fn new(idevice: Idevice) -> Self {
        Self { idevice }
    }

    /// Turns the connection into a [`RemotePairingClient`] that speaks
    /// `RPPairing` over it.
    ///
    /// `sending_host` is the name this computer identifies itself by, the same
    /// value the wireless flow uses.
    pub fn into_client(
        self,
        sending_host: &str,
    ) -> Result<RemotePairingClient<RpPairingSocket<Box<dyn ReadWrite>>>, IdeviceError> {
        let socket = self
            .idevice
            .get_socket()
            .ok_or(IdeviceError::NoEstablishedConnection)?;
        Ok(RemotePairingClient::new(
            RpPairingSocket::new(socket),
            sending_host,
        ))
    }
}
