//! Browse and transfer files over the CoreDevice file service.

use std::borrow::Cow;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use crate::{IdeviceError, ReadWrite, RemoteXpcClient, obf, xpc::XPCObject};

use super::CoreDeviceError;

/// Fixed-size preamble the data port answers a `rwb!FILE` request with, before
/// the length-prefixed payload.
const DATA_PREAMBLE_LEN: usize = 0x24;

/// Which of the device's filesystem domains a session is scoped to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// An app's own data container. `identifier` is the bundle ID.
    AppDataContainer,
    /// A shared app-group container. `identifier` is the group ID.
    AppGroupDataContainer,
    /// The temporary directory.
    Temporary,
    /// The system crash-log store.
    SystemCrashLogs,
}

impl Domain {
    pub fn as_u64(self) -> u64 {
        match self {
            Domain::AppDataContainer => 1,
            Domain::AppGroupDataContainer => 2,
            Domain::Temporary => 3,
            Domain::SystemCrashLogs => 5,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "appDataContainer" => Some(Domain::AppDataContainer),
            "appGroupDataContainer" => Some(Domain::AppGroupDataContainer),
            "temporary" => Some(Domain::Temporary),
            "systemCrashLogs" => Some(Domain::SystemCrashLogs),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct FileServiceClient<R: ReadWrite> {
    inner: RemoteXpcClient<R>,
    session: Option<String>,
}

#[cfg(feature = "rsd")]
impl crate::RsdService for FileServiceClient<Box<dyn ReadWrite>> {
    fn rsd_service_name() -> Cow<'static, str> {
        obf!("com.apple.coredevice.fileservice.control")
    }

    async fn from_stream(stream: Box<dyn ReadWrite>) -> Result<Self, IdeviceError> {
        Self::new(stream).await
    }
}

impl<R: ReadWrite> FileServiceClient<R> {
    pub async fn new(inner: R) -> Result<Self, IdeviceError> {
        let mut inner = RemoteXpcClient::new(inner).await?;
        inner.do_handshake().await?;
        Ok(Self {
            inner,
            session: None,
        })
    }

    /// Opens a session on `domain`, which every later command is scoped to.
    ///
    /// `identifier` names the container for the container domains (a bundle ID
    /// or an app-group ID) and is ignored by the others, which take `""`.
    pub async fn create_session(
        &mut self,
        domain: Domain,
        identifier: &str,
    ) -> Result<String, IdeviceError> {
        let res = self
            .send_receive(crate::xpc!({
                "Cmd": "CreateSession",
                "Domain": XPCObject::UInt64(domain.as_u64()),
                "Identifier": identifier,
                "Session": "",
                "User": "mobile"
            }))
            .await?;

        let session = res
            .as_dictionary()
            .and_then(|d| d.get("NewSessionID"))
            .and_then(|v| v.as_string())
            .ok_or(CoreDeviceError::MissingField("NewSessionID"))?
            .to_string();
        self.session = Some(session.clone());
        Ok(session)
    }

    /// Lists `path`, relative to the session's domain root.
    pub async fn retrieve_directory_list(
        &mut self,
        path: &str,
    ) -> Result<Vec<String>, IdeviceError> {
        let session = self.session()?;
        let res = self
            .send_receive(crate::xpc!({
                "Cmd": "RetrieveDirectoryList",
                "MessageUUID": uuid::Uuid::new_v4().to_string(),
                "Path": path,
                "SessionID": session
            }))
            .await?;

        let list = res
            .as_dictionary()
            .and_then(|d| d.get("FileList"))
            .and_then(|v| v.as_array())
            .ok_or(CoreDeviceError::MissingField("FileList"))?;
        Ok(list
            .iter()
            .filter_map(|x| x.as_string().map(str::to_string))
            .collect())
    }

    /// Downloads `path`, relative to the session's domain root.
    pub async fn retrieve_file<S, F, Fut>(
        &mut self,
        path: &str,
        connect_data: F,
    ) -> Result<Vec<u8>, IdeviceError>
    where
        S: ReadWrite,
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<S, IdeviceError>>,
    {
        let session = self.session()?;
        let res = self
            .send_receive(crate::xpc!({
                "Cmd": "RetrieveFile",
                "Path": path,
                "SessionID": session
            }))
            .await?;
        let res = res
            .as_dictionary()
            .ok_or(CoreDeviceError::MalformedField("(root)"))?;

        let response = res
            .get("Response")
            .and_then(plist_u64)
            .ok_or(CoreDeviceError::MissingField("Response"))?;
        let file_id = res
            .get("NewFileID")
            .and_then(plist_u64)
            .ok_or(CoreDeviceError::MissingField("NewFileID"))?;

        let mut data_stream = connect_data().await?;

        // `rwb!FILE` then four big-endian u64s: the control reply's Response,
        // zero, the file ID it handed out, zero.
        let mut req = Vec::with_capacity(8 + 32);
        req.extend_from_slice(b"rwb!FILE");
        req.extend_from_slice(&response.to_be_bytes());
        req.extend_from_slice(&0u64.to_be_bytes());
        req.extend_from_slice(&file_id.to_be_bytes());
        req.extend_from_slice(&0u64.to_be_bytes());
        data_stream.write_all(&req).await?;
        data_stream.flush().await?;

        let mut preamble = [0u8; DATA_PREAMBLE_LEN];
        data_stream.read_exact(&mut preamble).await?;

        let mut len = [0u8; 4];
        data_stream.read_exact(&mut len).await?;
        let mut payload = vec![0u8; u32::from_be_bytes(len) as usize];
        data_stream.read_exact(&mut payload).await?;
        Ok(payload)
    }

    /// Creates an empty file at `path`, relative to the session's domain root.
    pub async fn propose_empty_file(
        &mut self,
        path: &str,
        file_permissions: u32,
        uid: u32,
        gid: u32,
        creation_time: i64,
        last_modification_time: i64,
    ) -> Result<(), IdeviceError> {
        let session = self.session()?;
        self.send_receive(crate::xpc!({
            "Cmd": "ProposeEmptyFile",
            "FileCreationTime": XPCObject::Int64(creation_time),
            "FileLastModificationTime": XPCObject::Int64(last_modification_time),
            "FilePermissions": XPCObject::Int64(file_permissions as i64),
            "FileOwnerUserID": XPCObject::Int64(uid as i64),
            "FileOwnerGroupID": XPCObject::Int64(gid as i64),
            "Path": path,
            "SessionID": session
        }))
        .await?;
        Ok(())
    }

    /// The session ID from the last [`create_session`](Self::create_session).
    pub fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }

    fn session(&self) -> Result<String, IdeviceError> {
        self.session.clone().ok_or_else(|| {
            IdeviceError::UnexpectedResponse("no file service session; call create_session".into())
        })
    }

    async fn send_receive(
        &mut self,
        request: impl Into<XPCObject>,
    ) -> Result<plist::Value, IdeviceError> {
        self.inner.send_object(request, true).await?;
        // `CreateSession` is answered on the reply channel, every later command
        // on the root channel, so wait on both.
        let res = self.inner.recv_any().await?;
        debug!("file service reply: {res:?}");

        if let Some(dict) = res.as_dictionary()
            && dict.contains_key("EncodedError")
        {
            let detail = dict
                .get("LocalizedDescription")
                .and_then(|v| v.as_string())
                .map(str::to_string)
                .unwrap_or_else(|| format!("{:?}", dict.get("EncodedError")));
            return Err(CoreDeviceError::DeviceError(detail).into());
        }
        Ok(res)
    }
}

fn plist_u64(value: &plist::Value) -> Option<u64> {
    value
        .as_unsigned_integer()
        .or_else(|| value.as_signed_integer().map(|x| x as u64))
}
