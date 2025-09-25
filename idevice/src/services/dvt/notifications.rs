//! Notificaitons service client for iOS instruments protocol.
//!
//! Monitor memory and app notifications

use crate::{
    IdeviceError, ReadWrite,
    dvt::{
        message::AuxValue,
        remote_server::{Channel, RemoteServerClient},
    },
    obf,
};
use plist::Value;

#[derive(Debug)]
pub struct NotificationInfo {
    type_notification: String,
    mach_absolute_time: i64,
    exec_name: String,
    app_name: String,
    pid: u32,
    state_description: String,
}

pub struct NotificationsClient<'a, R: ReadWrite> {
    /// The underlying channel used for communication
    pub channel: Channel<'a, R>,
}

impl<'a, R: ReadWrite> NotificationsClient<'a, R> {
    /// Opens a new channel on the remote server client for app notifications
    ///
    /// # Arguments
    /// * `client` - The remote server client to connect with
    ///
    /// # Returns
    /// The client on success, IdeviceError on failure
    pub async fn new(client: &'a mut RemoteServerClient<R>) -> Result<Self, IdeviceError> {
        let channel = client
            .make_channel(obf!(
                "com.apple.instruments.server.services.mobilenotifications"
            ))
            .await?; // Drop `&mut client` before continuing

        Ok(Self { channel })
    }

    /// set the applicaitons and memory notifications enabled
    pub async fn start_notifications(&mut self) -> Result<(), IdeviceError> {
        let application_method = Value::String("setApplicationStateNotificationsEnabled:".into());
        self.channel
            .call_method(
                Some(application_method),
                Some(vec![AuxValue::archived_value(true)]),
                false,
            )
            .await?;
        let memory_method = Value::String("setMemoryNotificationsEnabled:".into());
        self.channel
            .call_method(
                Some(memory_method),
                Some(vec![AuxValue::archived_value(true)]),
                false,
            )
            .await?;
        Ok(())
    }

    /// print the applicaitons and memory notifications
    pub async fn get_notifications(&mut self) -> Result<NotificationInfo, IdeviceError> {
        let message = self.channel.read_message().await?;
        let mut notification = NotificationInfo {
            type_notification: "".to_string(),
            mach_absolute_time: 0,
            exec_name: String::new(),
            app_name: String::new(),
            pid: 0,
            state_description: String::new(),
        };
        if let Some(aux) = message.aux {
            for v in aux.values {
                match v {
                    AuxValue::Array(a) => match ns_keyed_archive::decode::from_bytes(&a) {
                        Ok(archive) => {
                            if let Some(dict) = archive.into_dictionary() {
                                for (key, value) in dict.iter() {
                                    match key.as_str() {
                                        "mach_absolute_time" => {
                                            if let Value::Integer(time) = value {
                                                notification.mach_absolute_time =
                                                    time.as_signed().unwrap_or(0);
                                            }
                                        }
                                        "execName" => {
                                            if let Value::String(name) = value {
                                                notification.exec_name = name.clone();
                                            }
                                        }
                                        "appName" => {
                                            if let Value::String(name) = value {
                                                notification.app_name = name.clone();
                                            }
                                        }
                                        "pid" => {
                                            if let Value::Integer(pid) = value {
                                                notification.pid =
                                                    pid.as_unsigned().unwrap_or(0) as u32;
                                            }
                                        }
                                        "state_description" => {
                                            if let Value::String(desc) = value {
                                                notification.state_description = desc.clone();
                                            }
                                        }
                                        _ => {
                                            println!("Unknown key: {} = {:?}", key, value);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            println!("Failed to decode archive: {:?}", e);
                        }
                    },
                    _ => {
                        println!("Non-array aux value: {:?}", v);
                    }
                }
            }
        }

        let data = match message.data {
            Some(Value::String(data)) => Some(data),
            _ => None,
        };
        notification.type_notification = data.unwrap();

        Ok(notification)
    }

    /// set the applicaitons and memory notifications disable
    pub async fn stop_notifications(&mut self) -> Result<(), IdeviceError> {
        let application_method = Value::String("setApplicationStateNotificationsEnabled:".into());
        self.channel
            .call_method(
                Some(application_method),
                Some(vec![AuxValue::archived_value(false)]),
                false,
            )
            .await?;
        let memory_method = Value::String("setMemoryNotificationsEnabled:".into());
        self.channel
            .call_method(
                Some(memory_method),
                Some(vec![AuxValue::archived_value(false)]),
                false,
            )
            .await?;

        Ok(())
    }
}
