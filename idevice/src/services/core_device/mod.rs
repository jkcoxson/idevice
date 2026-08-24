// Jackson Coxson
// Ported from pymobiledevice3

use async_stream::try_stream;
use futures::Stream;
use tracing::warn;

use crate::{
    IdeviceError, ReadWrite, RemoteXpcClient,
    xpc::{self, XPCObject},
};

mod app_service;
mod configuration_service;
mod diagnosticsservice;
#[cfg(feature = "display_stream")]
pub mod display_stream;
mod errors;
mod file_service;
#[cfg(feature = "display_stream")]
pub mod hid;
mod icon_service;
mod location_service;
mod openstdiosocket;
mod orientation_service;
mod pasteboard_service;
mod screencaptureservices;
pub use app_service::*;
pub use configuration_service::*;
pub use diagnosticsservice::*;
#[cfg(feature = "display_stream")]
pub use display_stream::*;
pub use errors::CoreDeviceError;
pub use file_service::*;
#[cfg(feature = "display_stream")]
pub use hid::*;
pub use icon_service::*;
pub use location_service::*;
pub use openstdiosocket::*;
pub use orientation_service::*;
pub use pasteboard_service::*;
pub use screencaptureservices::*;

const CORE_SERVICE_VERSION: &str = "443.18";

#[derive(Debug)]
pub struct CoreDeviceServiceClient<R: ReadWrite> {
    inner: RemoteXpcClient<R>,
}

impl<R: ReadWrite> CoreDeviceServiceClient<R> {
    pub async fn new(inner: R) -> Result<Self, IdeviceError> {
        let mut client = RemoteXpcClient::new(inner).await?;
        client.do_handshake().await?;
        Ok(Self { inner: client })
    }

    pub async fn invoke_with_plist(
        &mut self,
        feature: impl Into<String>,
        input: plist::Dictionary,
    ) -> Result<plist::Value, IdeviceError> {
        let input: XPCObject = plist::Value::Dictionary(input).into();
        let input = input.to_dictionary().unwrap();
        self.invoke(feature, Some(input)).await
    }

    pub async fn invoke_with_plist_action(
        &mut self,
        feature: impl Into<String>,
        input: plist::Dictionary,
        action_identifier: impl Into<String>,
    ) -> Result<plist::Value, IdeviceError> {
        let input: XPCObject = plist::Value::Dictionary(input).into();
        let input = input.to_dictionary().unwrap();
        self.invoke_inner(
            Some(feature.into()),
            Some(input),
            Some(action_identifier.into()),
        )
        .await
    }

    /// Invokes a CoreDevice action that has no feature behind it.
    ///
    /// Some services - the configuration service among them - dispatch purely on
    /// `CoreDevice.actionIdentifier` and advertise no matching feature, so the
    /// request carries neither `CoreDevice.featureIdentifier` nor
    /// `CoreDevice.action`.
    pub async fn invoke_action_with_plist(
        &mut self,
        action_identifier: impl Into<String>,
        input: plist::Dictionary,
    ) -> Result<plist::Value, IdeviceError> {
        let input: XPCObject = plist::Value::Dictionary(input).into();
        let input = input.to_dictionary().unwrap();
        self.invoke_inner(None, Some(input), Some(action_identifier.into()))
            .await
    }

    pub async fn invoke(
        &mut self,
        feature: impl Into<String>,
        input: Option<crate::xpc::Dictionary>,
    ) -> Result<plist::Value, IdeviceError> {
        self.invoke_inner(Some(feature.into()), input, None).await
    }

    /// Invokes a CoreDevice feature that returns elements over an XPC side channel.
    pub(crate) fn invoke_streaming_with_plist(
        &mut self,
        feature: impl Into<String>,
        input: plist::Dictionary,
    ) -> impl Stream<Item = Result<plist::Value, IdeviceError>> + '_ {
        let feature = feature.into();

        try_stream! {
            let input = XPCObject::from(plist::Value::Dictionary(input));
            let side_channel = uuid::Uuid::new_v4();
            let stream_input = build_streaming_input(input, side_channel);
            let mut req = xpc::Dictionary::new();
            req.insert(
                "CoreDevice.CoreDeviceDDIProtocolVersion".into(),
                XPCObject::Int64(2),
            );
            req.insert("CoreDevice.action".into(), xpc::Dictionary::new().into());
            req.insert(
                "CoreDevice.coreDeviceVersion".into(),
                create_xpc_version_from_string(CORE_SERVICE_VERSION).into(),
            );
            req.insert(
                "CoreDevice.deviceIdentifier".into(),
                XPCObject::String(uuid::Uuid::new_v4().to_string()),
            );
            req.insert(
                "CoreDevice.featureIdentifier".into(),
                XPCObject::String(feature),
            );
            req.insert("CoreDevice.input".into(), stream_input);
            req.insert(
                "CoreDevice.invocationIdentifier".into(),
                XPCObject::String(uuid::Uuid::new_v4().to_string()),
            );
            self.inner.send_object(req, true).await?;

            loop {
                let response = self.inner.recv_root().await?;

                match parse_stream_response(response)? {
                    StreamingResponse::Elements(batch) => {
                        for element in batch {
                            yield element;
                        }
                    }
                    StreamingResponse::Finished => break,
                }
            }
        }
    }

    async fn invoke_inner(
        &mut self,
        feature: Option<String>,
        input: Option<crate::xpc::Dictionary>,
        action_identifier: Option<String>,
    ) -> Result<plist::Value, IdeviceError> {
        let input: crate::xpc::XPCObject = match input {
            Some(i) => i.into(),
            None => crate::xpc::Dictionary::new().into(),
        };

        let mut req = xpc::Dictionary::new();
        let protocol_version = if action_identifier.is_some() { 2 } else { 0 };
        req.insert(
            "CoreDevice.CoreDeviceDDIProtocolVersion".into(),
            XPCObject::Int64(protocol_version),
        );
        req.insert("CoreDevice.action".into(), xpc::Dictionary::new().into());
        req.insert(
            "CoreDevice.coreDeviceVersion".into(),
            create_xpc_version_from_string(CORE_SERVICE_VERSION).into(),
        );
        req.insert(
            "CoreDevice.deviceIdentifier".into(),
            XPCObject::String(uuid::Uuid::new_v4().to_string()),
        );
        if let Some(feature) = feature {
            req.insert(
                "CoreDevice.featureIdentifier".into(),
                XPCObject::String(feature),
            );
        }
        req.insert("CoreDevice.input".into(), input);
        req.insert(
            "CoreDevice.invocationIdentifier".into(),
            XPCObject::String(uuid::Uuid::new_v4().to_string()),
        );
        if let Some(action_identifier) = action_identifier {
            req.insert(
                "CoreDevice.actionIdentifier".into(),
                XPCObject::String(action_identifier),
            );
        }

        self.inner.send_object(req, true).await?;
        let res = self.inner.recv().await?;
        let mut res = match res {
            plist::Value::Dictionary(d) => d,
            _ => {
                warn!("XPC response was not a dictionary");
                return Err(CoreDeviceError::MalformedField("(root)").into());
            }
        };

        let res = match res.remove("CoreDevice.output") {
            Some(r) => r,
            None => {
                // The device replied with an error rather than an output. Surface
                // its contents (commonly under "CoreDevice.error") so callers can
                // see why a feature invocation was rejected.
                warn!("XPC response did not have an output: {res:?}");
                return match res.get("CoreDevice.error") {
                    Some(e) => Err(CoreDeviceError::DeviceError(format!("{e:?}")).into()),
                    None => Err(CoreDeviceError::MissingField("CoreDevice.output").into()),
                };
            }
        };

        Ok(res)
    }
}

fn build_streaming_input(input: XPCObject, side_channel: uuid::Uuid) -> XPCObject {
    let mut stream_proxy = xpc::Dictionary::new();
    stream_proxy.insert("sideChannel".into(), XPCObject::Uuid(side_channel));

    let mut stream_input = xpc::Dictionary::new();
    stream_input.insert("actualInput".into(), input);
    stream_input.insert("streamProxy".into(), stream_proxy.into());
    stream_input.into()
}

enum StreamingResponse {
    Elements(Vec<plist::Value>),
    Finished,
}

fn parse_stream_response(response: plist::Value) -> Result<StreamingResponse, IdeviceError> {
    let mut response = response
        .into_dictionary()
        .ok_or(CoreDeviceError::MalformedField("(root)"))?;
    let stream_status_key: std::borrow::Cow<'static, str> =
        crate::obf!("CoreDevice.XPCMessageKey.sideChannelStatus");
    let mut status = match response.remove(stream_status_key.as_ref()) {
        Some(plist::Value::Dictionary(status)) => status,
        Some(_) => return Err(CoreDeviceError::MalformedField("sideChannelStatus").into()),
        None => {
            return match response.get("CoreDevice.error") {
                Some(error) => Err(CoreDeviceError::DeviceError(format!("{error:?}")).into()),
                None => Err(CoreDeviceError::MissingField("sideChannelStatus").into()),
            };
        }
    };

    if let Some(error) = status.remove("receivedError") {
        return Err(CoreDeviceError::DeviceError(format!("{error:?}")).into());
    }
    if status.contains_key("finishStreaming") {
        return Ok(StreamingResponse::Finished);
    }

    let pushing = status
        .remove("pushing")
        .and_then(plist::Value::into_dictionary)
        .ok_or(CoreDeviceError::MissingField("pushing"))?;
    let elements = pushing
        .get("elements")
        .and_then(plist::Value::as_array)
        .cloned()
        .ok_or(CoreDeviceError::MalformedField("elements"))?;
    Ok(StreamingResponse::Elements(elements))
}

fn create_xpc_version_from_string(version: impl Into<String>) -> xpc::Dictionary {
    let version: String = version.into();
    let mut collected_version = Vec::new();
    version.split('.').for_each(|x| {
        if let Ok(x) = x.parse() {
            collected_version.push(XPCObject::UInt64(x));
        }
    });

    let mut res = xpc::Dictionary::new();
    res.insert(
        "originalComponentsCount".into(),
        XPCObject::Int64(collected_version.len() as i64),
    );
    res.insert("components".into(), XPCObject::Array(collected_version));
    res.insert("stringValue".into(), XPCObject::String(version));
    res
}
