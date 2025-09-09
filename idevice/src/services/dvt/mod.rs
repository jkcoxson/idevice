// Jackson Coxson

use crate::provider::IdeviceProvider;
use crate::services::lockdown::LockdownClient;
use crate::{Idevice, IdeviceError, ReadWrite, RsdService, obf};

#[cfg(feature = "location_simulation")]
pub mod location_simulation;
pub mod message;
pub mod process_control;
pub mod remote_server;
pub mod screenshot;

impl RsdService for remote_server::RemoteServerClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.instruments.dtservicehub")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self::new(stream))
    }
}

// iOS version support notes:
// - com.apple.instruments.dtservicehub (RSD/XPC over HTTP2) is used on iOS 17+.
// - com.apple.instruments.remoteserver is available on pre-iOS 17 (and many older versions).
// - com.apple.instruments.remoteserver.DVTSecureSocketProxy is used by some iOS 14 builds.
//
// This impl enables Lockdown-based connection to Instruments Remote Server for iOS < 17
// by reusing the same RemoteServerClient but sourcing the transport from StartService.
impl crate::IdeviceService for remote_server::RemoteServerClient<Box<dyn ReadWrite>> {
    fn service_name() -> std::borrow::Cow<'static, str> {
        // Primary name for Instruments Remote Server
        obf!("com.apple.instruments.remoteserver")
    }

    #[allow(async_fn_in_trait)]
    async fn connect(provider: &dyn IdeviceProvider) -> Result<Self, IdeviceError> {
        // Establish Lockdown session
        let mut lockdown = LockdownClient::connect(provider).await?;
        lockdown
            .start_session(&provider.get_pairing_file().await?)
            .await?;

        // Try main Instruments service first, then DVTSecureSocketProxy (seen on iOS 14)
        let try_names = [
            obf!("com.apple.instruments.remoteserver"),
            obf!("com.apple.instruments.remoteserver.DVTSecureSocketProxy"),
        ];

        let mut last_err: Option<IdeviceError> = None;
        for name in try_names {
            match lockdown.start_service(name).await {
                Ok((port, ssl)) => {
                    let mut idevice = provider.connect(port).await?;
                    if ssl {
                        idevice
                            .start_session(&provider.get_pairing_file().await?)
                            .await?;
                    }
                    // Convert to transport and build client
                    let socket = idevice
                        .get_socket()
                        .ok_or(IdeviceError::NoEstablishedConnection)?;
                    return Ok(remote_server::RemoteServerClient::new(socket));
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.unwrap_or(IdeviceError::ServiceNotFound))
    }

    #[allow(async_fn_in_trait)]
    async fn from_stream(idevice: Idevice) -> Result<Self, IdeviceError> {
        // Not used in our overridden connect path, but implemented for completeness
        let socket = idevice
            .get_socket()
            .ok_or(IdeviceError::NoEstablishedConnection)?;
        Ok(remote_server::RemoteServerClient::new(socket))
    }
}
