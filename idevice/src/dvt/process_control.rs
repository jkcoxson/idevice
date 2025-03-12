// Jackson Coxson

use crate::IdeviceError;

use super::remote_server::{Channel, RemoteServerClient};

const IDENTIFIER: &str = "com.apple.instruments.server.services.processcontrol";

pub struct ProcessControlClient<'a> {
    channel: Channel<'a>,
}

impl<'a> ProcessControlClient<'a> {
    pub async fn new(client: &'a mut RemoteServerClient) -> Result<Self, IdeviceError> {
        let channel = client.make_channel(IDENTIFIER).await?; // Drop `&mut client` before continuing

        Ok(Self { channel })
    }
}
