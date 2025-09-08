use plist::Value;

use crate::{
    IdeviceError, ReadWrite,
    dvt::{
        message::AuxValue,
        remote_server::{Channel, RemoteServerClient},
    },
    obf,
};

pub struct ScreenShotClient<'a, R: ReadWrite> {
    channel: Channel<'a, R>,
}

impl<'a, R: ReadWrite> ScreenShotClient<'a, R> {
    pub async fn new(client: &'a mut RemoteServerClient<R>) -> Result<Self, IdeviceError> {
        let channel = client
            .make_channel(obf!(
                "com.apple.instruments.server.services.screenshot"
            ))
            .await?; // Drop `&mut client` before continuing

        Ok(Self { channel })
    }

    pub async fn take_screenshot(&mut self) -> Result<Vec<u8>, IdeviceError> {
        let method = Value::String("takeScreenshot".into());

        self.channel.call_method(Some(method), None, true).await?;

        let msg = self.channel.read_message().await?;
        println!("takeScreenshot: over");
        match msg.data {
            Some(Value::Data(data)) => Ok(data),
            _ => Err(IdeviceError::UnexpectedResponse),
        }
    }
}