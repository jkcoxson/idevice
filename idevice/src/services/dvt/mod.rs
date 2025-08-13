// Jackson Coxson

use crate::{IdeviceError, ReadWrite, RsdService, obf};

#[cfg(feature = "location_simulation")]
pub mod location_simulation;
pub mod message;
pub mod process_control;
pub mod remote_server;

impl RsdService for remote_server::RemoteServerClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> std::borrow::Cow<'static, str> {
        obf!("com.apple.instruments.dtservicehub")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Ok(Self::new(stream))
    }
}
