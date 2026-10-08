//! Browse and transfer files over the CoreDevice file service.

use std::borrow::Cow;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use crate::{
    IdeviceError, ReadWrite, RemoteXpcClient, obf,
    xpc::{Body, XPCFlag, XPCObject},
};

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

    /// Lists everything under `path`, relative to the session's domain root:
    /// every file in the subtree as a path relative to `path`.
    ///
    /// The device does not put the listing in its reply. It first sends it as
    /// wants-reply messages of its own on the root channel, each carrying the
    /// request's `MessageUUID` and a `FileList` page (128 names on iOS 26);
    /// every page is answered here with `{MessageUUID}` on the reply channel,
    /// or the device stops reading the connection. The reply to the request
    /// itself follows the last page and ends the listing; it fails on
    /// `EncodedError`. Messages for another `MessageUUID` (compared without
    /// case: the device upper-cases it) and bodyless wrappers are skipped.
    /// O(n) time, the whole list in memory.
    pub async fn retrieve_directory_list(
        &mut self,
        path: &str,
    ) -> Result<Vec<String>, IdeviceError> {
        let session = self.session()?;
        // The device echoes the UUID upper-cased, so it is sent that way and
        // compared without case.
        let uuid = uuid::Uuid::new_v4().to_string().to_uppercase();
        let request_id = self
            .send(crate::xpc!({
                "Cmd": "RetrieveDirectoryList",
                "MessageUUID": uuid.as_str(),
                "Path": path,
                "SessionID": session
            }))
            .await?;

        let reply_flag: u32 = XPCFlag::Reply.into();
        let mut names: Vec<String> = Vec::new();
        loop {
            let env = self.inner.recv_envelope(&[1, 3]).await?;
            let res = match env.body {
                Body::Object(obj) => obj.to_plist(),
                // An empty dictionary is a keepalive on its own, but as the
                // reply to the request it is a listing with nothing to add.
                Body::Empty => plist::Value::Dictionary(Default::default()),
                Body::None => continue,
            };
            let dict = res
                .as_dictionary()
                .ok_or(CoreDeviceError::MalformedField("(root)"))?;
            if env.flags & reply_flag != 0 {
                if env.message_id != request_id {
                    debug!("file service: reply to another request: {dict:?}");
                    continue;
                }
                check_error(dict)?;
                if let Some(page) = dict.get("FileList").and_then(|v| v.as_array()) {
                    names.extend(page.iter().filter_map(|x| x.as_string().map(str::to_string)));
                }
                return Ok(names);
            }
            let ours = dict
                .get("MessageUUID")
                .and_then(|v| v.as_string())
                .is_some_and(|u| u.eq_ignore_ascii_case(&uuid));
            if !ours {
                debug!("file service: message for another request: {dict:?}");
                continue;
            }
            check_error(dict)?;
            let page = dict
                .get("FileList")
                .and_then(|v| v.as_array())
                .ok_or(CoreDeviceError::MissingField("FileList"))?;
            names.extend(page.iter().filter_map(|x| x.as_string().map(str::to_string)));
            self.inner
                .send_reply_to(env.message_id, crate::xpc!({ "MessageUUID": uuid.as_str() }))
                .await?;
        }
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

    /// Sends a wants-reply request with an odd message id and returns the id.
    /// The service numbers the messages it originates 2, 4, 6, … and closes
    /// the connection on an even client id.
    async fn send(&mut self, request: impl Into<XPCObject>) -> Result<u64, IdeviceError> {
        let next = self.inner.next_message_id();
        if next % 2 == 0 {
            self.inner.set_next_message_id(next + 1);
        }
        self.inner.send_object_with_id(request, true).await
    }

    async fn send_receive(
        &mut self,
        request: impl Into<XPCObject>,
    ) -> Result<plist::Value, IdeviceError> {
        self.send(request).await?;
        // `CreateSession` is answered on the reply channel, every later command
        // on the root channel, so wait on both.
        let res = self.inner.recv_any().await?;
        debug!("file service reply: {res:?}");
        if let Some(dict) = res.as_dictionary() {
            check_error(dict)?;
        }
        Ok(res)
    }
}

/// A reply carrying `EncodedError` is the service's failure shape; its
/// `LocalizedDescription` is the detail.
fn check_error(dict: &plist::Dictionary) -> Result<(), IdeviceError> {
    if !dict.contains_key("EncodedError") {
        return Ok(());
    }
    let detail = dict
        .get("LocalizedDescription")
        .and_then(|v| v.as_string())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{:?}", dict.get("EncodedError")));
    Err(CoreDeviceError::DeviceError(detail).into())
}

fn plist_u64(value: &plist::Value) -> Option<u64> {
    value
        .as_unsigned_integer()
        .or_else(|| value.as_signed_integer().map(|x| x as u64))
}

#[cfg(test)]
mod tests {
    //! Synthetic-peer tests: the peer half of an in-memory duplex writes the
    //! device's HTTP/2 frames and reads the client's; no device or network.
    use super::*;
    use crate::xpc::{Dictionary, XPCMessage};
    use crate::xpc::http2::frame::{DataFrame, HttpFrame};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

    const ROOT: u32 = 1;
    const REPLY: u32 = 3;

    async fn client() -> (FileServiceClient<DuplexStream>, DuplexStream) {
        let (ours, peer) = tokio::io::duplex(1 << 20);
        let c = FileServiceClient {
            inner: RemoteXpcClient::new(ours).await.unwrap(),
            session: Some("S".into()),
        };
        (c, peer)
    }

    fn frame(stream: u32, flags: u32, id: u64, d: Dictionary) -> Vec<u8> {
        let bytes = XPCMessage {
            flags,
            message: Some(XPCObject::Dictionary(d)),
            message_id: Some(id),
        }
        .encode(id)
        .unwrap();
        DataFrame {
            stream_id: stream,
            payload: bytes,
            end_stream: false,
        }
        .serialize()
    }

    /// A device-originated page: wants-reply on the root channel.
    fn page(id: u64, uuid: &str, names: &[String]) -> Vec<u8> {
        let mut d = Dictionary::new();
        d.insert("MessageUUID".into(), XPCObject::String(uuid.into()));
        d.insert(
            "FileList".into(),
            XPCObject::Array(names.iter().map(|n| XPCObject::String(n.clone())).collect()),
        );
        frame(ROOT, 0x10101, id, d)
    }

    /// The reply to request `id`, on the reply channel.
    fn final_reply(id: u64, d: Dictionary) -> Vec<u8> {
        frame(REPLY, 0x20101, id, d)
    }

    /// Fails instead of hanging when the client never returns.
    async fn soon<T>(f: impl std::future::Future<Output = T>) -> T {
        tokio::time::timeout(Duration::from_secs(2), f)
            .await
            .expect("the listing ended")
    }

    fn names(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}/{i}")).collect()
    }

    /// Reads the client's bytes until `needle` appears `count` times, so the
    /// peer learns the request's random `MessageUUID` and sees the acks.
    async fn read_until(peer: &mut DuplexStream, got: &mut Vec<u8>, needle: &str, count: usize) {
        let mut buf = vec![0u8; 1 << 16];
        let occurrences = |g: &[u8]| String::from_utf8_lossy(g).matches(needle).count();
        while occurrences(got) < count {
            let n = tokio::time::timeout(Duration::from_secs(2), peer.read(&mut buf))
                .await
                .expect("client wrote what the protocol requires")
                .unwrap();
            got.extend_from_slice(&buf[..n]);
        }
    }

    /// The UUID string follows the `RetrieveDirectoryList` command in the
    /// encoded request (the dictionary keeps insertion order).
    fn request_uuid(got: &[u8]) -> String {
        let text = String::from_utf8_lossy(got).into_owned();
        let i = text.find("RetrieveDirectoryList").unwrap();
        let rest = &text[i..];
        let dash = rest.find('-').unwrap();
        rest[dash - 8..dash + 28].to_string()
    }

    #[tokio::test]
    async fn pages_are_acknowledged_and_the_reply_ends_the_listing() {
        let (mut c, mut peer) = client().await;
        let task = tokio::spawn(async move { c.retrieve_directory_list("/").await });
        let mut got = Vec::new();
        read_until(&mut peer, &mut got, "RetrieveDirectoryList", 1).await;
        let uuid = request_uuid(&got);
        let (a, b, d) = (names("a", 128), names("b", 128), names("d", 5));
        for (i, p) in [&a, &b, &d].iter().enumerate() {
            // The match is case-insensitive; the ack must carry our spelling.
            peer.write_all(&page(2 + 2 * i as u64, &uuid.to_lowercase(), p)).await.unwrap();
            // Each page is answered before the next one is sent.
            read_until(&mut peer, &mut got, &uuid, 2 + i).await;
        }
        let mut done = Dictionary::new();
        done.insert("Response".into(), XPCObject::UInt64(1));
        peer.write_all(&final_reply(1, done)).await.unwrap();
        let names = soon(task).await.unwrap().unwrap();
        assert_eq!(names, [a, b, d].concat());
    }

    #[tokio::test]
    async fn the_request_id_is_odd_and_other_requests_pages_are_skipped() {
        let (mut c, mut peer) = client().await;
        c.inner.set_next_message_id(2);
        let task = tokio::spawn(async move { c.retrieve_directory_list("/").await });
        let mut got = Vec::new();
        read_until(&mut peer, &mut got, "RetrieveDirectoryList", 1).await;
        let uuid = request_uuid(&got);
        peer.write_all(&page(2, "other", &names("stale", 3))).await.unwrap();
        peer.write_all(&page(4, &uuid, &names("a", 3))).await.unwrap();
        read_until(&mut peer, &mut got, &uuid, 2).await;
        // The request went out as id 3; a reply to id 2 is not ours.
        peer.write_all(&final_reply(2, Dictionary::new())).await.unwrap();
        peer.write_all(&final_reply(3, Dictionary::new())).await.unwrap();
        assert_eq!(soon(task).await.unwrap().unwrap(), names("a", 3));
        // No ack went out for the stale page.
        assert_eq!(String::from_utf8_lossy(&got).matches("other").count(), 0);
    }

    #[tokio::test]
    async fn an_error_reply_fails_the_listing() {
        let (mut c, mut peer) = client().await;
        let task = tokio::spawn(async move { c.retrieve_directory_list("/").await });
        let mut got = Vec::new();
        read_until(&mut peer, &mut got, "RetrieveDirectoryList", 1).await;
        let mut d = Dictionary::new();
        d.insert("EncodedError".into(), XPCObject::Data(vec![1, 2, 3]));
        d.insert("LocalizedDescription".into(), XPCObject::String("no such path".into()));
        peer.write_all(&final_reply(1, d)).await.unwrap();
        let err = soon(task).await.unwrap().unwrap_err();
        assert!(err.to_string().contains("no such path"), "{err}");
    }
}
