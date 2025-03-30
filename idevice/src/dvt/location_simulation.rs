// Jackson Coxson

use plist::Value;

use crate::{dvt::message::AuxValue, IdeviceError, ReadWrite};

use super::remote_server::{Channel, RemoteServerClient};

const IDENTIFIER: &str = "com.apple.instruments.server.services.LocationSimulation";

pub struct LocationSimulationClient<'a, R: ReadWrite> {
    channel: Channel<'a, R>,
}

impl<'a, R: ReadWrite> LocationSimulationClient<'a, R> {
    pub async fn new(client: &'a mut RemoteServerClient<R>) -> Result<Self, IdeviceError> {
        let channel = client.make_channel(IDENTIFIER).await?; // Drop `&mut client` before continuing

        Ok(Self { channel })
    }

    pub async fn clear(&mut self) -> Result<(), IdeviceError> {
        let method = Value::String("stopLocationSimulation".into());

        self.channel.call_method(Some(method), None, true).await?;

        let _ = self.channel.read_message().await?;

        Ok(())
    }

    pub async fn set(&mut self, latitude: f64, longitude: f64) -> Result<(), IdeviceError> {
        let method = Value::String("simulateLocationWithLatitude:longitude:".into());

        self.channel
            .call_method(
                Some(method),
                Some(vec![
                    AuxValue::archived_value(latitude),
                    AuxValue::archived_value(longitude),
                ]),
                true,
            )
            .await?;

        // We don't actually care what's in the response, but we need to request one and read it
        let _ = self.channel.read_message().await?;

        Ok(())
    }
}
