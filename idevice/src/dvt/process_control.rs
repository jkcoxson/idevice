// Jackson Coxson

use log::warn;
use plist::{Dictionary, Value};

use crate::{dvt::message::AuxValue, IdeviceError, ReadWrite};

use super::remote_server::{Channel, RemoteServerClient};

const IDENTIFIER: &str = "com.apple.instruments.server.services.processcontrol";

pub struct ProcessControlClient<'a, R: ReadWrite> {
    channel: Channel<'a, R>,
}

impl<'a, R: ReadWrite> ProcessControlClient<'a, R> {
    pub async fn new(client: &'a mut RemoteServerClient<R>) -> Result<Self, IdeviceError> {
        let channel = client.make_channel(IDENTIFIER).await?; // Drop `&mut client` before continuing

        Ok(Self { channel })
    }

    pub async fn launch_app(
        &mut self,
        bundle_id: impl Into<String>,
        env_vars: Option<Dictionary>,
        arguments: Option<Dictionary>,
        start_suspended: bool,
        kill_existing: bool,
    ) -> Result<u64, IdeviceError> {
        let method = Value::String(
            "launchSuspendedProcessWithDevicePath:bundleIdentifier:environment:arguments:options:"
                .into(),
        );
        let mut options = Dictionary::new();
        options.insert(
            "StartSuspendedKey".into(),
            if start_suspended { 0_u64 } else { 1 }.into(),
        );
        options.insert(
            "KillExisting".into(),
            if kill_existing { 0_u64 } else { 1 }.into(),
        );

        let env_vars = match env_vars {
            Some(e) => e,
            None => Dictionary::new(),
        };
        let arguments = match arguments {
            Some(a) => a,
            None => Dictionary::new(),
        };

        self.channel
            .call_method(
                Some(method),
                Some(vec![
                    AuxValue::archived_value("/private/"),
                    AuxValue::archived_value(bundle_id.into()),
                    AuxValue::archived_value(env_vars),
                    AuxValue::archived_value(arguments),
                    AuxValue::archived_value(options),
                ]),
                true,
            )
            .await?;

        let res = self.channel.read_message().await?;

        match res.data {
            Some(Value::Integer(p)) => match p.as_unsigned() {
                Some(p) => Ok(p),
                None => {
                    warn!("PID wasn't unsigned");
                    Err(IdeviceError::UnexpectedResponse)
                }
            },
            _ => {
                warn!("Did not get integer response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }

    pub async fn kill_app(&mut self, pid: u64) -> Result<(), IdeviceError> {
        self.channel
            .call_method(
                "killPid:".into(),
                Some(vec![AuxValue::U32(pid as u32)]),
                false,
            )
            .await?;

        Ok(())
    }

    pub async fn disable_memory_limit(&mut self, pid: u64) -> Result<(), IdeviceError> {
        self.channel
            .call_method(
                "requestDisableMemoryLimitsForPid:".into(),
                Some(vec![AuxValue::U32(pid as u32)]),
                true,
            )
            .await?;

        let res = self.channel.read_message().await?;
        match res.data {
            Some(Value::Boolean(b)) => {
                if b {
                    Ok(())
                } else {
                    warn!("Failed to disable memory limit");
                    Err(IdeviceError::DisableMemoryLimitFailed)
                }
            }
            _ => {
                warn!("Did not receive bool response");
                Err(IdeviceError::UnexpectedResponse)
            }
        }
    }
}
