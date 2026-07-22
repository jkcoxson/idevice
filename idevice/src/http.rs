//! Minimal async HTTP/1.1 client.
//!
//! The crate only needs a handful of one-shot HTTP requests: the tunneld device
//! list, the TSS controller, and the restore data-service proxy. Rather
//! than pull in every single dependency on planet earth for that,
//! this module speaks HTTP/1.1 directly.
//!
//! It is deliberately small and not a general-purpose client.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::debug;

use crate::{IdeviceError, ReadWrite};

/// Maximum number of redirects to follow before giving up
const MAX_REDIRECTS: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpRequest {
    method: Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::Get, url)
    }

    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::Post, url)
    }

    fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: Vec::new(),
            body: Vec::new(),
        }
    }

    /// Adds a request header. Duplicate names are preserved and sent in order.
    #[must_use]
    pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Sends the request, following redirects, and returns the final response.
    pub async fn send(self) -> Result<HttpResponse, IdeviceError> {
        let mut method = self.method;
        let mut url = self.url;
        let mut headers = self.headers;
        let mut body = self.body;

        for _ in 0..=MAX_REDIRECTS {
            let resp = send_once(method, &url, &headers, &body).await?;
            match resp.status {
                301 | 302 | 303 | 307 | 308 => {
                    let Some(location) = resp.header("location") else {
                        return Ok(resp);
                    };
                    url = resolve_redirect(&url, location)?;
                    debug!("following {} redirect to {url}", resp.status);
                    // 307/308 preserve the method and body; 301/302/303 degrade to
                    // a bodyless GET, matching browser and `reqwest` behavior.
                    if !matches!(resp.status, 307 | 308) {
                        method = Method::Get;
                        body = Vec::new();
                        headers.retain(|(k, _)| {
                            !k.eq_ignore_ascii_case("content-length")
                                && !k.eq_ignore_ascii_case("content-type")
                        });
                    }
                }
                _ => return Ok(resp),
            }
        }
        Err(IdeviceError::Http(format!(
            "too many redirects (>{MAX_REDIRECTS})"
        )))
    }
}

pub async fn get(url: impl Into<String>) -> Result<HttpResponse, IdeviceError> {
    HttpRequest::get(url).send().await
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// Returns the first header matching `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    #[cfg(feature = "_serde_json")]
    pub fn json(&self) -> Result<serde_json::Value, IdeviceError> {
        Ok(serde_json::from_slice(&self.body)?)
    }
}

struct ParsedUrl {
    https: bool,
    host: String,
    authority: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl, IdeviceError> {
    let bad = |m: &str| IdeviceError::Http(format!("invalid URL `{url}`: {m}"));

    let (https, rest) = if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else {
        return Err(bad("scheme must be http or https"));
    };

    // Split authority from the path. A `?` or `#` before any `/` still ends the
    // authority (e.g. `http://h?q`).
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    let path_part = &rest[auth_end..];
    if authority.is_empty() {
        return Err(bad("missing host"));
    }
    // Drop any userinfo (`user:pass@host`); unused by our endpoints.
    let hostport = authority.rsplit('@').next().unwrap_or(authority);

    // Separate host and optional port, honoring IPv6 literals in brackets.
    let (host, port_str) = if let Some(after) = hostport.strip_prefix('[') {
        let close = after
            .find(']')
            .ok_or_else(|| bad("unterminated IPv6 literal"))?;
        let host = &after[..close];
        let tail = &after[close + 1..];
        let port = tail.strip_prefix(':');
        (host.to_string(), port)
    } else if let Some((h, p)) = hostport.rsplit_once(':') {
        (h.to_string(), Some(p))
    } else {
        (hostport.to_string(), None)
    };
    if host.is_empty() {
        return Err(bad("missing host"));
    }

    let port = match port_str {
        Some(p) => p.parse::<u16>().map_err(|_| bad("invalid port"))?,
        None => {
            if https {
                443
            } else {
                80
            }
        }
    };

    let path = if path_part.is_empty() || path_part.starts_with(['?', '#']) {
        format!("/{path_part}")
    } else {
        path_part.to_string()
    };

    Ok(ParsedUrl {
        https,
        host,
        authority: hostport.to_string(),
        port,
        path,
    })
}

/// If the redirect is relative, resolve it
fn resolve_redirect(base: &str, location: &str) -> Result<String, IdeviceError> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Ok(location.to_string());
    }
    let parsed = parse_url(base)?;
    let scheme = if parsed.https { "https" } else { "http" };
    if let Some(abs_path) = location.strip_prefix('/') {
        // Absolute-path reference: keep scheme + authority, replace the path.
        Ok(format!("{scheme}://{}/{abs_path}", parsed.authority))
    } else {
        // Relative reference: resolve against the base's directory.
        let dir = match parsed.path.rfind('/') {
            Some(i) => &parsed.path[..=i],
            None => "/",
        };
        Ok(format!("{scheme}://{}{dir}{location}", parsed.authority))
    }
}

async fn send_once(
    method: Method,
    url: &str,
    headers: &[(String, String)],
    body: &[u8],
) -> Result<HttpResponse, IdeviceError> {
    let parsed = parse_url(url)?;
    debug!("{} {} ({} body bytes)", method.as_str(), url, body.len());

    let tcp = tokio::net::TcpStream::connect((parsed.host.as_str(), parsed.port)).await?;
    let mut stream: Box<dyn ReadWrite> = if parsed.https {
        tls_connect(tcp, &parsed.host).await?
    } else {
        Box::new(tcp)
    };

    // Build the request head. We always set Host and Connection: close, and set
    // Content-Length whenever there is a body (or the method conventionally
    // carries one). Caller-supplied headers follow and may add Content-Type etc.
    let mut req = Vec::new();
    req.extend_from_slice(method.as_str().as_bytes());
    req.extend_from_slice(b" ");
    req.extend_from_slice(parsed.path.as_bytes());
    req.extend_from_slice(b" HTTP/1.1\r\n");
    push_header(&mut req, "Host", &parsed.authority);
    push_header(&mut req, "Connection", "close");
    let caller_sets_len = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("content-length"));
    if !caller_sets_len && (!body.is_empty() || method == Method::Post) {
        push_header(&mut req, "Content-Length", &body.len().to_string());
    }
    for (k, v) in headers {
        push_header(&mut req, k, v);
    }
    req.extend_from_slice(b"\r\n");
    req.extend_from_slice(body);

    stream.write_all(&req).await?;
    stream.flush().await?;

    read_response(&mut stream).await
}

fn push_header(buf: &mut Vec<u8>, name: &str, value: &str) {
    buf.extend_from_slice(name.as_bytes());
    buf.extend_from_slice(b": ");
    buf.extend_from_slice(value.as_bytes());
    buf.extend_from_slice(b"\r\n");
}

async fn read_response(stream: &mut Box<dyn ReadWrite>) -> Result<HttpResponse, IdeviceError> {
    let mut buf = Vec::new();

    // Read until the end of the header block.
    let head_end = loop {
        if let Some(pos) = find_subsequence(&buf, b"\r\n\r\n") {
            break pos;
        }
        if !read_more(stream, &mut buf).await? {
            return Err(IdeviceError::Http(
                "connection closed before HTTP headers were complete".into(),
            ));
        }
    };

    let (status, headers) = parse_head(&buf[..head_end])?;
    let body_start = head_end + 4;

    let chunked = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("transfer-encoding"))
        .is_some_and(|(_, v)| v.to_ascii_lowercase().contains("chunked"));
    let content_length = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .and_then(|(_, v)| v.trim().parse::<usize>().ok());

    let body = if chunked {
        // Connection: close means the peer closes after the final chunk, so read
        // to end-of-stream and decode the complete chunked body.
        while read_more(stream, &mut buf).await? {}
        decode_chunked(&buf[body_start..])?
    } else if let Some(len) = content_length {
        while buf.len() < body_start + len {
            if !read_more(stream, &mut buf).await? {
                break; // truncated; return what we have
            }
        }
        let end = (body_start + len).min(buf.len());
        buf[body_start..end].to_vec()
    } else {
        // No framing info: read until the peer closes.
        while read_more(stream, &mut buf).await? {}
        buf[body_start..].to_vec()
    };

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// Reads one chunk of bytes into `buf`. Returns `false` on clean EOF.
async fn read_more(
    stream: &mut Box<dyn ReadWrite>,
    buf: &mut Vec<u8>,
) -> Result<bool, IdeviceError> {
    let mut tmp = [0u8; 16384];
    let n = stream.read(&mut tmp).await?;
    if n == 0 {
        return Ok(false);
    }
    buf.extend_from_slice(&tmp[..n]);
    Ok(true)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parses the status line and headers from the header block (excluding the
/// trailing `\r\n\r\n`).
fn parse_head(head: &[u8]) -> Result<(u16, Vec<(String, String)>), IdeviceError> {
    let text = std::str::from_utf8(head)
        .map_err(|_| IdeviceError::Http("non-UTF-8 HTTP header block".into()))?;
    let mut lines = text.split("\r\n");

    let status_line = lines
        .next()
        .ok_or_else(|| IdeviceError::Http("empty HTTP response".into()))?;
    // "HTTP/1.1 200 OK" -> 200
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| IdeviceError::Http(format!("malformed HTTP status line: {status_line}")))?;

    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    Ok((status, headers))
}

/// Decodes a complete chunked transfer-encoding body.
fn decode_chunked(mut data: &[u8]) -> Result<Vec<u8>, IdeviceError> {
    let err = || IdeviceError::Http("malformed chunked HTTP body".into());
    let mut out = Vec::new();
    loop {
        // <hex-size>[;ext]\r\n
        let line_end = find_subsequence(data, b"\r\n").ok_or_else(err)?;
        let size_line = std::str::from_utf8(&data[..line_end]).map_err(|_| err())?;
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_hex, 16).map_err(|_| err())?;
        data = &data[line_end + 2..];
        if size == 0 {
            break; // last chunk; ignore any trailers
        }
        if data.len() < size + 2 {
            return Err(err());
        }
        out.extend_from_slice(&data[..size]);
        // Each chunk's data is followed by a CRLF.
        if &data[size..size + 2] != b"\r\n" {
            return Err(err());
        }
        data = &data[size + 2..];
    }
    Ok(out)
}

#[cfg(feature = "rustls")]
async fn tls_connect(
    tcp: tokio::net::TcpStream,
    host: &str,
) -> Result<Box<dyn ReadWrite>, IdeviceError> {
    use std::sync::Arc;

    crate::ensure_default_crypto_provider();

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();

    let connector = tokio_rustls::TlsConnector::from(Arc::new(config));
    let server_name = rustls::pki_types::ServerName::try_from(host.to_string())
        .map_err(|_| IdeviceError::Http(format!("invalid TLS server name `{host}`")))?;
    let tls = connector.connect(server_name, tcp).await?;
    Ok(Box::new(tls))
}

#[cfg(all(feature = "openssl", not(feature = "rustls")))]
async fn tls_connect(
    tcp: tokio::net::TcpStream,
    host: &str,
) -> Result<Box<dyn ReadWrite>, IdeviceError> {
    let connector = openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls())?.build();
    let ssl = connector.configure()?.into_ssl(host)?;
    let mut stream = tokio_openssl::SslStream::new(ssl, tcp)?;
    std::pin::Pin::new(&mut stream).connect().await?;
    Ok(Box::new(stream))
}

#[cfg(not(any(feature = "rustls", feature = "openssl")))]
async fn tls_connect(
    _tcp: tokio::net::TcpStream,
    _host: &str,
) -> Result<Box<dyn ReadWrite>, IdeviceError> {
    Err(IdeviceError::Http(
        "an https URL was requested but no TLS backend is enabled (enable `rustls` or `openssl`)"
            .into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_url() {
        let u = parse_url("http://127.0.0.1:5555/").unwrap();
        assert!(!u.https);
        assert_eq!(u.host, "127.0.0.1");
        assert_eq!(u.authority, "127.0.0.1:5555");
        assert_eq!(u.port, 5555);
        assert_eq!(u.path, "/");
    }

    #[test]
    fn default_ports_and_path() {
        let u = parse_url("http://gs.apple.com/TSS/controller?action=2").unwrap();
        assert_eq!(u.port, 80);
        assert_eq!(u.path, "/TSS/controller?action=2");

        let s = parse_url("https://example.com").unwrap();
        assert!(s.https);
        assert_eq!(s.port, 443);
        assert_eq!(s.path, "/");
    }

    #[test]
    fn ipv6_literal() {
        let u = parse_url("http://[::1]:8080/x").unwrap();
        assert_eq!(u.host, "::1");
        assert_eq!(u.authority, "[::1]:8080");
        assert_eq!(u.port, 8080);
        assert_eq!(u.path, "/x");
    }

    #[test]
    fn rejects_unknown_scheme() {
        assert!(parse_url("ftp://x/").is_err());
        assert!(parse_url("gs.apple.com").is_err());
    }

    #[test]
    fn redirect_resolution() {
        // Absolute
        assert_eq!(
            resolve_redirect("https://a.com/x", "https://b.com/y").unwrap(),
            "https://b.com/y"
        );
        // Absolute-path
        assert_eq!(
            resolve_redirect("https://a.com/x/y?q", "/z").unwrap(),
            "https://a.com/z"
        );
        // Relative
        assert_eq!(
            resolve_redirect("https://a.com/dir/page", "next").unwrap(),
            "https://a.com/dir/next"
        );
    }

    #[test]
    fn parses_head() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: text/plain";
        let (status, headers) = parse_head(head).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0], ("Content-Length".into(), "5".into()));
    }

    #[test]
    fn decodes_chunked() {
        let body = b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(body).unwrap(), b"Wikipedia");
    }

    #[test]
    fn decodes_chunked_with_extension() {
        let body = b"3;name=value\r\nabc\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(body).unwrap(), b"abc");
    }
}
