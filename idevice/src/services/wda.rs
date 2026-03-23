//! Minimal WebDriverAgent client over direct device connections.
//!
//! This client talks to WDA on the device port directly through
//! [`crate::provider::IdeviceProvider`], so parallel automation across many
//! devices does not require binding unique localhost ports per device.

use std::time::Duration;

use serde_json::Value;
use tokio::time::{Instant, sleep, timeout};

use crate::{Idevice, IdeviceError, provider::IdeviceProvider};

/// Default WDA HTTP port on the device.
pub const DEFAULT_WDA_PORT: u16 = 8100;

/// Default MJPEG streaming port used by many WDA builds.
pub const DEFAULT_WDA_MJPEG_PORT: u16 = 9100;

/// Device-side ports exposed by a WDA runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WdaPorts {
    /// Device port for the HTTP WebDriver endpoint.
    pub http: u16,
    /// Device port for the MJPEG stream endpoint.
    pub mjpeg: u16,
}

impl Default for WdaPorts {
    fn default() -> Self {
        Self {
            http: DEFAULT_WDA_PORT,
            mjpeg: DEFAULT_WDA_MJPEG_PORT,
        }
    }
}

/// Minimal WDA client bound to a specific device provider.
#[derive(Debug)]
pub struct WdaClient<'a> {
    provider: &'a dyn IdeviceProvider,
    ports: WdaPorts,
    timeout: Duration,
}

impl<'a> WdaClient<'a> {
    /// Creates a WDA client using the default device-side ports.
    pub fn new(provider: &'a dyn IdeviceProvider) -> Self {
        Self {
            provider,
            ports: WdaPorts::default(),
            timeout: Duration::from_secs(10),
        }
    }

    /// Overrides the device-side WDA ports.
    pub fn with_ports(mut self, ports: WdaPorts) -> Self {
        self.ports = ports;
        self
    }

    /// Overrides the per-request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Returns the configured device-side ports.
    pub fn ports(&self) -> WdaPorts {
        self.ports
    }

    /// Fetches `/status` from the WDA HTTP endpoint.
    pub async fn status(&self) -> Result<Value, IdeviceError> {
        self.request_json("GET", "/status", None).await
    }

    /// Waits until WDA begins responding on its HTTP endpoint.
    pub async fn wait_until_ready(&self, timeout_duration: Duration) -> Result<Value, IdeviceError> {
        let deadline = Instant::now() + timeout_duration;
        loop {
            match self.status().await {
                Ok(status) => return Ok(status),
                Err(_) if Instant::now() < deadline => {
                    sleep(Duration::from_millis(100)).await;
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Starts a WDA session and returns the session id.
    pub async fn start_session(&self, bundle_id: Option<&str>) -> Result<String, IdeviceError> {
        let mut caps = serde_json::Map::new();
        if let Some(bundle_id) = bundle_id {
            caps.insert("bundleId".into(), Value::String(bundle_id.to_owned()));
        }

        let mut capabilities = serde_json::Map::new();
        capabilities.insert("alwaysMatch".into(), Value::Object(caps.clone()));

        let mut payload = serde_json::Map::new();
        payload.insert("capabilities".into(), Value::Object(capabilities));
        payload.insert("desiredCapabilities".into(), Value::Object(caps));

        let payload = Value::Object(payload);

        let response = self.request_json("POST", "/session", Some(&payload)).await?;
        if let Some(session_id) = response.get("sessionId").and_then(Value::as_str) {
            return Ok(session_id.to_owned());
        }
        if let Some(session_id) = response
            .get("value")
            .and_then(Value::as_object)
            .and_then(|value| value.get("sessionId"))
            .and_then(Value::as_str)
        {
            return Ok(session_id.to_owned());
        }

        Err(IdeviceError::UnexpectedResponse)
    }

    async fn request_json(
        &self,
        method: &str,
        path: &str,
        payload: Option<&Value>,
    ) -> Result<Value, IdeviceError> {
        let body = match payload {
            Some(payload) => serde_json::to_vec(payload).map_err(|_| IdeviceError::UnexpectedResponse)?,
            None => Vec::new(),
        };

        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        if payload.is_some() {
            request.push_str("Content-Type: application/json\r\n");
        }
        request.push_str("\r\n");

        let mut idevice = self.provider.connect(self.ports.http).await?;
        timeout(self.timeout, async {
            idevice.send_raw(request.as_bytes()).await?;
            if !body.is_empty() {
                idevice.send_raw(&body).await?;
            }
            Self::read_json_response(&mut idevice).await
        })
        .await
        .map_err(|_| timeout_error("wda request"))?
    }

    async fn read_json_response(idevice: &mut Idevice) -> Result<Value, IdeviceError> {
        let mut response = Vec::new();
        let mut header_end = None;
        let mut content_length = None;

        loop {
            let chunk = idevice.read_any(8192).await?;
            if chunk.is_empty() {
                break;
            }

            response.extend_from_slice(&chunk);

            if header_end.is_none()
                && let Some(offset) = find_bytes(&response, b"\r\n\r\n")
            {
                let header_len = offset + 4;
                header_end = Some(header_len);
                let header_text = String::from_utf8_lossy(&response[..offset]);
                content_length = parse_content_length(&header_text);
            }

            if let (Some(header_len), Some(content_length)) = (header_end, content_length)
                && response.len() >= header_len + content_length
            {
                break;
            }
        }

        let header_end = header_end.ok_or(IdeviceError::UnexpectedResponse)?;
        let header_text = String::from_utf8_lossy(&response[..header_end - 4]);
        let mut lines = header_text.lines();
        let status_line = lines.next().ok_or(IdeviceError::UnexpectedResponse)?;
        let status_code = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok())
            .ok_or(IdeviceError::UnexpectedResponse)?;

        let body = &response[header_end..];
        let json: Value = serde_json::from_slice(body).map_err(|_| IdeviceError::UnexpectedResponse)?;
        if !(200..300).contains(&status_code) {
            return Err(IdeviceError::UnknownErrorType(format!(
                "WDA HTTP {status_code}: {json}"
            )));
        }
        Ok(json)
    }
}

fn parse_content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("content-length") {
            return None;
        }
        value.trim().parse::<usize>().ok()
    })
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn timeout_error(context: &str) -> IdeviceError {
    std::io::Error::new(
        std::io::ErrorKind::TimedOut,
        format!("{context} timed out"),
    )
    .into()
}
