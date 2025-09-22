use plist::Value;

use crate::{
    IdeviceError, ReadWrite,
    dvt::{
        message::AuxValue,
        remote_server::{Channel, RemoteServerClient},
    },
    obf,
};

struct NotificationInfo{
    mach_absolute_time: i64,
    exec_name : String,
    app_name: String,
    pid : u32,
    state_description: String,
}

pub struct NotificationsClient<'a, R:ReadWrite> {
    pub channel: Channel<'a, R>,
}

impl<'a,R:ReadWrite> NotificationsClient<'a,R> {
    pub async fn new(client: &'a mut RemoteServerClient<R>) -> Result<Self, IdeviceError> {
        let channel = client
            .make_channel(obf!(
                "com.apple.instruments.server.services.mobilenotifications"
            ))
            .await?; // Drop `&mut client` before continuing

        Ok(Self { channel})
    }

    pub async fn start_notifications(&mut self) -> Result<(), IdeviceError> {
        let application_method = Value::String("setApplicationStateNotificationsEnabled:".into());
        self.channel.call_method(Some(application_method), Some(vec![AuxValue::archived_value(true)]), false).await?;
        let memory_method = Value::String("setMemoryNotificationsEnabled:".into());
        self.channel.call_method(Some(memory_method), Some(vec![AuxValue::archived_value(true)]), false).await?;
        Ok(())
    }

    pub async fn stop_notifications(&mut self) -> Result<(), IdeviceError> {
        let application_method = Value::String("setApplicationStateNotificationsEnabled:".into());
        self.channel.call_method(Some(application_method), Some(vec![AuxValue::archived_value(false)]), false).await?;
        let memory_method = Value::String("setMemoryNotificationsEnabled:".into());
        self.channel.call_method(Some(memory_method), Some(vec![AuxValue::archived_value(false)]), false).await?;
        Ok(())
    }

}