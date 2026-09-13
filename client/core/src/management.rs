// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # management — NetBird management-plane client skeleton (N3-1)
//!
//! Minimal HTTP client for the NetBird **management REST API** (the
//! `/api/...` dashboard/HTTP surface). WireGuard tunnel bootstrap (gRPC
//! `Sync` network map), signal and relay are NOT here.
//!
//! ## Protocol source (do not guess)
//! All endpoint paths, field names and encodings below are read from the
//! pinned upstream reference commit (pulled OUT of this repository, see
//! `docs/n3-management-protocol-notes.md`):
//!
//! ```text
//! upstream: netbirdio/netbird @ 791401060d2b95e5f51e3439c0649729132f571e
//! ```
//!
//! | fact | source file @ pinned commit |
//! | --- | --- |
//! | REST routes are mounted under `/api` | `management/server/http/testing/testing_tools/channel/channel.go` (`PathPrefix("/api")`) |
//! | `GET /api/peers` → JSON array of PeerBatch | `shared/management/http/api/openapi.yml` (`/api/peers`), `management/server/http/handlers/peers/peers_handler.go` (`GetAllPeers`) |
//! | `GET /api/peers/{peerId}` → Peer (node config) | `openapi.yml` (`/api/peers/{peerId}`), `peers_handler.go` (`HandlePeer`) |
//! | `GET /api/users/current` → User (session check) | `openapi.yml` (`/api/users/current`), `handlers/users/users_handler.go` (`getCurrentUser`) |
//! | auth headers: `Authorization: Bearer {jwt}` or `Authorization: Token {pat}` | `management/server/http/middleware/auth_middleware.go` |
//! | error body: `{"message": string, "code": int}` | `shared/management/http/util/util.go` (`ErrorResponse`, `WriteErrorResponse`) |
//! | peer fields: id, name, ip, connected, hostname, version, dns_label, ... | `openapi.yml` (`PeerMinimum`, `Peer`, `PeerBatch` schemas) |
//!
//! ## TODO(未确认) — setup-key registration is NOT a REST endpoint upstream
//! The task asks for "register with a setup key". Verified against the pinned
//! commit AND release tags v0.28.0 / v0.35.0 / v0.43.1 / v0.78.1: `/api/peers`
//! is **GET-only**; there is no REST setup-key registration endpoint. Real
//! registration is the gRPC `ManagementService/Login` RPC
//! (`shared/management/proto/management.proto`: `LoginRequest.setupKey`,
//! `PeerKeys.wgPubKey`), which needs HTTP/2 + protobuf + encrypted bodies —
//! out of scope for this increment.
//!
//! [`ManagementClient::register`] therefore speaks a **mock-only contract**
//! (`POST /api/peers`, body `{"setup_key":..., "name":...}`, response = the
//! created `Peer` object). Every piece of it that is not upstream-confirmable
//! is marked [`ENDPOINT_REGISTER_TODO_UNCONFIRMED`]. Its purpose is to make
//! the transport + error-classification machinery testable; against a real
//! NetBird server it would receive 404/405, mapped to
//! [`ManagementError::Request`]. Do NOT ship it unmodified.
//!
//! ## Timeout & retry policy (minimal, deliberate)
//! One timeout budget per request (`DEFAULT_TIMEOUT` = 10 s), covering the
//! TCP connect and every read on the socket (DNS resolution happens once, at
//! client construction). **Zero automatic retries** — setup-key registration
//! is not idempotent; the caller decides about retrying.
//!
//! ## TLS
//! Since N3-2 the TLS path exists: [`TlsHttpTransport`] (rustls/ring, blocking
//! `StreamOwned`) + [`ManagementClient::new_tls`] + [`parse_base_url_tls`].
//! The trust root is INJECTED (`root_certs` parameter; rustls reads no system
//! store and we never add one — docs/n3-stack-freeze-20260913.md risk list).
//! Plain-text HTTP ([`PlainHttpTransport`], `http://` only) remains the local
//! mock baseline; real deployments must use `https://` (credentials in clear
//! otherwise). [`parse_base_url`] keeps rejecting `https://` — the TLS
//! transport has its own parser, so the mock-baseline contract is unchanged.

use crate::config::parse_document;
use crate::config::Json;
use crate::util::json_escape;
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Pinned upstream reference commit this module's protocol facts come from.
pub const UPSTREAM_COMMIT: &str = "791401060d2b95e5f51e3439c0649729132f571e";

/// `GET` — list peers. Upstream-confirmed (openapi.yml `/api/peers`).
pub const ENDPOINT_PEERS: &str = "/api/peers";
/// `GET` — one peer (the node's management-side config). Upstream-confirmed
/// (openapi.yml `/api/peers/{peerId}`).
pub const ENDPOINT_PEER_BY_ID: &str = "/api/peers";
/// `GET` — session sanity check. Upstream-confirmed (openapi.yml
/// `/api/users/current`).
pub const ENDPOINT_USERS_CURRENT: &str = "/api/users/current";
/// TODO(未确认): register endpoint. **Mock-only contract** — upstream has NO
/// REST registration at this path (GET-only, verified across v0.28.0..main);
/// real registration is gRPC `ManagementService/Login`. See module docs.
pub const ENDPOINT_REGISTER_TODO_UNCONFIRMED: &str = "/api/peers";

/// Per-request timeout budget (connect + read), if the caller does not pick
/// one. Deliberately visible: tests shrink it.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

// ---------------------------------------------------------------------------
// errors (fixed classification; no source-level specifics leak into Display
// beyond what the server itself sent)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagementError {
    /// TCP/DNS/socket failures (connect refused, resolve failure, broken pipe).
    Network(String),
    /// Connect or read exceeded the per-request timeout budget.
    Timeout,
    /// 401/403 from the server (bad/expired JWT or PAT).
    Auth { status: u16, message: String },
    /// Any other 4xx (bad request, not found, ...). Includes 404/405 — the
    /// register mock-contract will land here against a real server.
    /// Local (pre-flight) validation failures reuse this variant with
    /// `status: 0` (no HTTP exchange happened).
    Request { status: u16, message: String },
    /// 5xx — server-side failure.
    Server { status: u16 },
    /// Response was not the JSON shape we require (truncated body, bad
    /// encoding, unexpected status, unparseable chunked body, ...).
    Parse(String),
    /// URL scheme not usable by the selected transport (plain transport:
    /// `https://` rejected; TLS transport: `http://` rejected; plus malformed
    /// URLs on either path).
    UnsupportedUrl(String),
}

impl core::fmt::Display for ManagementError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ManagementError::Network(e) => write!(f, "network error: {e}"),
            ManagementError::Timeout => write!(f, "request timed out"),
            ManagementError::Auth { status, message } => {
                write!(f, "authentication failed ({status}): {message}")
            }
            ManagementError::Request { status, message } => {
                write!(f, "request rejected ({status}): {message}")
            }
            ManagementError::Server { status } => write!(f, "server error ({status})"),
            ManagementError::Parse(m) => write!(f, "response parse failed: {m}"),
            ManagementError::UnsupportedUrl(u) => {
                write!(f, "unsupported base url '{u}' (TLS not implemented)")
            }
        }
    }
}

impl std::error::Error for ManagementError {}

// ---------------------------------------------------------------------------
// transport abstraction
// ---------------------------------------------------------------------------

/// One HTTP/1.1 request as the client builds it. Bodies are JSON strings.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: &'static str,
    pub path: String,
    /// Value for `Authorization`, already scheme-prefixed (e.g.
    /// `Bearer x.y.z` / `Token pat`). `None` for unauthenticated calls.
    pub authorization: Option<String>,
    pub body: Option<String>,
}

/// Raw response status + body; header handling stays inside the transport.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Transport seam: plain TCP today, TLS in a later increment. The mock tests
/// drive the real [`PlainHttpTransport`] over loopback — no second impl.
pub trait HttpTransport {
    fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, ManagementError>;
}

// ---------------------------------------------------------------------------
// plain-text HTTP transport (TEST BASELINE ONLY — TLS is a later increment)
// ---------------------------------------------------------------------------

/// Minimal blocking HTTP/1.1 client over `TcpStream`: one connection per
/// request, `Connection: close`, `Content-Length` bodies both ways, chunked
/// response decoding, absolute per-request timeout.
pub struct PlainHttpTransport {
    authority: String,
    peer: SocketAddr,
    timeout: Duration,
}

impl PlainHttpTransport {
    pub fn new(host: &str, port: u16, timeout: Duration) -> Result<Self, ManagementError> {
        let authority = format!("{host}:{port}");
        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|e| ManagementError::Network(format!("resolve {host}: {e}")))?
            .collect();
        // Prefer IPv4 loopback-able literals; first v4, else first addr.
        let peer = addrs
            .iter()
            .find(|a| a.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| ManagementError::Network(format!("no address for {host}")))?;
        Ok(PlainHttpTransport { authority, peer, timeout })
    }
}

impl HttpTransport for PlainHttpTransport {
    fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, ManagementError> {
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(&self.peer, self.timeout)
            .map_err(map_io_timeout("connect"))?;
        stream
            .set_write_timeout(Some(remaining(self.timeout, started)?))
            .map_err(map_io_timeout("set write timeout"))?;
        write_request(&mut stream, req, &self.authority)?;
        read_response(&mut stream, self.timeout, started)
    }
}

/// Serialize one request head (+ body) onto a stream. Byte-identical across
/// the plain and TLS transports (the mock baseline asserts this shape).
fn write_request<S: Write>(
    stream: &mut S,
    req: &HttpRequest,
    authority: &str,
) -> Result<(), ManagementError> {
    let body = req.body.as_deref().unwrap_or("");
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: netbird-harmonyos-core/0.1 (management skeleton)\r\nAccept: application/json\r\nConnection: close\r\n",
        req.method, req.path, authority
    );
    if let Some(auth) = &req.authorization {
        head.push_str(&format!("Authorization: {auth}\r\n"));
    }
    if !body.is_empty() {
        head.push_str("Content-Type: application/json\r\n");
        head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    head.push_str("\r\n");
    stream
        .write_all(head.as_bytes())
        .map_err(map_io_timeout("write request head"))?;
    if !body.is_empty() {
        stream
            .write_all(body.as_bytes())
            .map_err(map_io_timeout("write request body"))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// TLS HTTP transport (N3-2) — https for the REST surface
// ---------------------------------------------------------------------------

/// Blocking HTTPS/1.1 transport: rustls (`ring` provider) `ClientConnection`
/// over a `TcpStream`, one connection per request — the same framing and
/// timeout rules as [`PlainHttpTransport`].
///
/// Trust root: ONLY the caller-provided CA certificates are trusted. rustls
/// reads no system store and this transport never adds one
/// (docs/n3-stack-freeze-20260913.md risk list).
pub struct TlsHttpTransport {
    authority: String,
    server_name: ServerName<'static>,
    peer: SocketAddr,
    timeout: Duration,
    config: Arc<ClientConfig>,
}

/// Decode PEM-encoded X509 certificate(s) into the DER form the transports
/// take ([`CertificateDer`]); multiple PEM blocks are all decoded.
pub fn decode_pem_certificates(
    pem: &[u8],
) -> Result<Vec<CertificateDer<'static>>, ManagementError> {
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(pem)
        .collect::<Result<_, _>>()
        .map_err(|e| {
            ManagementError::Parse(format!("PEM certificate decode failed: {e}"))
        })?;
    if certs.is_empty() {
        return Err(ManagementError::Parse(
            "PEM certificate decode failed: no CERTIFICATE block".into(),
        ));
    }
    Ok(certs)
}

impl TlsHttpTransport {
    pub fn new(
        host: &str,
        port: u16,
        timeout: Duration,
        root_certs: Vec<CertificateDer<'static>>,
    ) -> Result<Self, ManagementError> {
        if root_certs.is_empty() {
            return Err(ManagementError::Request {
                status: 0,
                message: "TLS requested but no CA certificates provided \
                          (trust root must be injected; no system store is used)"
                    .into(),
            });
        }
        let mut roots = RootCertStore::empty();
        for cert in &root_certs {
            roots.add(cert.clone()).map_err(|e| {
                ManagementError::Parse(format!("TLS root certificate rejected: {e}"))
            })?;
        }
        let config = ClientConfig::builder_with_provider(
            rustls::crypto::ring::default_provider().into(),
        )
        .with_safe_default_protocol_versions()
        .map_err(|e| ManagementError::Network(format!("tls protocol versions: {e}")))?
        .with_root_certificates(roots)
        .with_no_client_auth();

        let addrs: Vec<SocketAddr> = (host, port)
            .to_socket_addrs()
            .map_err(|e| ManagementError::Network(format!("resolve {host}: {e}")))?
            .collect();
        // Prefer IPv4 loopback-able literals; first v4, else first addr.
        let peer = addrs
            .iter()
            .find(|a| a.is_ipv4())
            .or_else(|| addrs.first())
            .copied()
            .ok_or_else(|| ManagementError::Network(format!("no address for {host}")))?;
        let server_name = ServerName::try_from(host.to_string())
            .map_err(|e| ManagementError::UnsupportedUrl(format!("{host:?}: bad TLS name ({e})")))?;
        Ok(TlsHttpTransport {
            authority: format!("{host}:{port}"),
            server_name,
            peer,
            timeout,
            config: Arc::new(config),
        })
    }
}

impl HttpTransport for TlsHttpTransport {
    fn execute(&self, req: &HttpRequest) -> Result<HttpResponse, ManagementError> {
        let started = Instant::now();
        let sock = TcpStream::connect_timeout(&self.peer, self.timeout)
            .map_err(map_io_timeout("connect"))?;
        let conn = ClientConnection::new(Arc::clone(&self.config), self.server_name.clone())
            .map_err(|e| {
                ManagementError::Network(format!("tls handshake setup: {e}"))
            })?;
        let mut stream = StreamOwned::new(conn, sock);
        stream
            .sock
            .set_write_timeout(Some(remaining(self.timeout, started)?))
            .map_err(map_io_timeout("set write timeout"))?;
        write_request(&mut stream, req, &self.authority)?;
        // TLS failures on the first read/write surface here (handshake runs
        // lazily) and classify as Network.
        read_response(&mut stream, self.timeout, started)
    }
}

fn remaining(budget: Duration, started: Instant) -> Result<Duration, ManagementError> {
    budget
        .checked_sub(started.elapsed())
        .ok_or(ManagementError::Timeout)
}

fn map_io_timeout(stage: &'static str) -> impl Fn(std::io::Error) -> ManagementError {
    move |e| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => ManagementError::Timeout,
        _ => ManagementError::Network(format!("{stage}: {e}")),
    }
}

/// A reader over which the per-request read budget can be re-armed before
/// every read: the raw TCP socket and the rustls TLS stream over a TCP socket.
trait ReadWithDeadline: Read {
    fn arm_read_deadline(&self, timeout: Option<Duration>) -> std::io::Result<()>;
}

impl ReadWithDeadline for TcpStream {
    fn arm_read_deadline(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.set_read_timeout(timeout)
    }
}

impl ReadWithDeadline for StreamOwned<ClientConnection, TcpStream> {
    fn arm_read_deadline(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        self.sock.set_read_timeout(timeout)
    }
}

fn set_read_deadline<R: ReadWithDeadline>(
    stream: &R,
    budget: Duration,
    started: Instant,
) -> Result<(), ManagementError> {
    stream
        .arm_read_deadline(Some(remaining(budget, started)?))
        .map_err(map_io_timeout("set read timeout"))
}

/// Read head + body. Body framing: `Content-Length` first, then
/// `Transfer-Encoding: chunked`, then read-to-EOF (Connection: close).
fn read_response<R: ReadWithDeadline>(
    stream: &mut R,
    budget: Duration,
    started: Instant,
) -> Result<HttpResponse, ManagementError> {
    let mut raw: Vec<u8> = Vec::with_capacity(1024);
    let head_end = loop {
        if let Some(pos) = find_head_end(&raw) {
            break pos;
        }
        if raw.len() > 64 * 1024 {
            return Err(ManagementError::Parse("response head exceeds 64 KiB".into()));
        }
        read_more(stream, &mut raw, budget, started)?;
    };

    let head = core::str::from_utf8(&raw[..head_end])
        .map_err(|_| ManagementError::Parse("response head is not UTF-8".into()))?
        .to_string();
    let mut lines = head.split("\r\n");
    let status_line = lines
        .next()
        .ok_or_else(|| ManagementError::Parse("empty response head".into()))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| {
            ManagementError::Parse(format!("bad status line: {status_line:?}"))
        })?;
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            content_length = value.parse::<usize>().ok();
        } else if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
    }
    let mut body: Vec<u8> = raw[head_end + 4..].to_vec();
    if let Some(len) = content_length {
        while body.len() < len {
            read_more(stream, &mut body, budget, started)?;
        }
        body.truncate(len);
    } else if chunked {
        // Try to decode incrementally; only a clean terminal chunk (or EOF)
        // settles the body. A truncated chunked body is a Parse error.
        loop {
            match decode_chunked(&body) {
                Ok(decoded) => {
                    body = decoded;
                    break;
                }
                Err(e) => {
                    if !read_more(stream, &mut body, budget, started)? {
                        return Err(e);
                    }
                }
            }
        }
    } else {
        while read_more(stream, &mut body, budget, started)? {}
    }
    Ok(HttpResponse { status, body })
}

/// One `read(2)` into `buf`. Returns `Ok(false)` on clean EOF; `Err(Timeout)`
/// when the per-request budget runs out; other I/O errors are `Network`.
fn read_more<R: ReadWithDeadline>(
    stream: &mut R,
    buf: &mut Vec<u8>,
    budget: Duration,
    started: Instant,
) -> Result<bool, ManagementError> {
    set_read_deadline(stream, budget, started)?;
    let mut chunk = [0u8; 4096];
    match stream.read(&mut chunk) {
        Ok(0) => Ok(false),
        Ok(n) => {
            buf.extend_from_slice(&chunk[..n]);
            Ok(true)
        }
        Err(e) => Err(map_io_timeout("read response")(e)),
    }
}

fn find_head_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n")
}

/// Decode a chunked body (sizes in hex, CRLF framing, trailers skipped).
fn decode_chunked(raw: &[u8]) -> Result<Vec<u8>, ManagementError> {
    let mut out = Vec::with_capacity(raw.len());
    let mut pos = 0usize;
    loop {
        let Some(line_end) = raw[pos..]
            .windows(2)
            .position(|w| w == b"\r\n")
            .map(|p| pos + p)
        else {
            return Err(ManagementError::Parse("truncated chunked body".into()));
        };
        let size_text = core::str::from_utf8(&raw[pos..line_end])
            .map_err(|_| ManagementError::Parse("chunk size is not UTF-8".into()))?;
        let size_text = size_text.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| ManagementError::Parse(format!("bad chunk size {size_text:?}")))?;
        pos = line_end + 2;
        if size == 0 {
            return Ok(out); // terminating chunk; trailers ignored
        }
        if pos + size > raw.len() {
            return Err(ManagementError::Parse("truncated chunk data".into()));
        }
        out.extend_from_slice(&raw[pos..pos + size]);
        pos += size;
        // consume the CRLF after the chunk data (lenient about EOF)
        if raw[pos..].starts_with(b"\r\n") {
            pos += 2;
        }
    }
}

// ---------------------------------------------------------------------------
// base URL
// ---------------------------------------------------------------------------

/// Parsed base URL: scheme (http only), host, explicit or default port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseUrl {
    pub host: String,
    pub port: u16,
}

/// `http://host[:port][/...]` → [`BaseUrl`]. `https://` is rejected with
/// [`ManagementError::UnsupportedUrl`] (the plain transport stays mock-baseline
/// only; the TLS transport uses [`parse_base_url_tls`]). Any other scheme is a
/// parse failure. A non-empty path is rejected: the endpoints above are
/// absolute (`/api/...`).
pub fn parse_base_url(url: &str) -> Result<BaseUrl, ManagementError> {
    let bad = |m: &str| ManagementError::UnsupportedUrl(format!("{url:?}: {m}"));
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| bad("missing scheme (expected http://host[:port])"))?;
    match scheme {
        "http" => {}
        "https" => return Err(ManagementError::UnsupportedUrl(url.to_string())),
        other => return Err(bad(&format!("unknown scheme '{other}': use http:// (https rejected: TLS not implemented)"))),
    }
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = split_host_port(url, host_port, 80)?;
    Ok(BaseUrl { host, port })
}

/// `https://host[:port][/...]` → [`BaseUrl`] for the TLS transport (N3-2).
/// Default port 443; a non-empty path is rejected (same rule as
/// [`parse_base_url`]). Plain `http://` is NOT accepted here — it has its own
/// transport and parser.
pub fn parse_base_url_tls(url: &str) -> Result<BaseUrl, ManagementError> {
    let bad = |m: &str| ManagementError::UnsupportedUrl(format!("{url:?}: {m}"));
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| bad("missing scheme (expected https://host[:port])"))?;
    match scheme {
        "https" => {}
        "http" => {
            return Err(bad(
                "plain http is not valid for the TLS transport (use parse_base_url + PlainHttpTransport)",
            ))
        }
        other => return Err(bad(&format!("unknown scheme '{other}': use https://"))),
    }
    let host_port = rest.split(['/', '?', '#']).next().unwrap_or("");
    let (host, port) = split_host_port(url, host_port, 443)?;
    Ok(BaseUrl { host, port })
}

/// Shared `host[:port]` splitting. `url` is only used in error text;
/// `default_port` = 80 (http) / 443 (https).
fn split_host_port(
    url: &str,
    host_port: &str,
    default_port: u16,
) -> Result<(String, u16), ManagementError> {
    let bad = |m: &str| ManagementError::UnsupportedUrl(format!("{url:?}: {m}"));
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (
            h,
            p.parse::<u16>()
                .map_err(|_| bad(&format!("bad port '{p}'")))?,
        ),
        None => (host_port, default_port),
    };
    if host.is_empty() {
        return Err(bad("empty host"));
    }
    if host.contains(':') {
        // bare IPv6 literals need bracket parsing — out of scope, be explicit
        return Err(bad("IPv6 hosts not supported yet"));
    }
    Ok((host.to_string(), port))
}

// ---------------------------------------------------------------------------
// models (response-side subset; unknown fields are IGNORED by design —
// responses must survive upstream schema additions, unlike config.rs input
// validation which rejects typos)
// ---------------------------------------------------------------------------

/// Management view of one peer. `id`/`name` are required by the upstream
/// `PeerMinimum` schema; everything else is optional and kept as `Option`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub name: String,
    pub ip: Option<String>,
    pub connected: Option<bool>,
    pub hostname: Option<String>,
    pub version: Option<String>,
    pub dns_label: Option<String>,
}

/// `User` as returned by `GET /api/users/current` (login verification).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
}

/// Credential type used to establish a session. Both header forms are
/// upstream-confirmed (`auth_middleware.go`): JWT → `Bearer {jwt}`,
/// PAT → `Token {pat}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// IdP-issued JWT → `Authorization: Bearer ...`
    Jwt(String),
    /// Personal access token → `Authorization: Token ...`
    Pat(String),
}

impl Credential {
    fn header_value(&self) -> String {
        match self {
            Credential::Jwt(t) => format!("Bearer {}", t),
            Credential::Pat(t) => format!("Token {}", t),
        }
    }
}

/// An authenticated management session: the credential plus the verified
/// `User`. Tokens never appear in `Display`/`Debug` output of errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    credential: Credential,
    pub user: User,
}

impl Session {
    fn authorization(&self) -> String {
        self.credential.header_value()
    }
}

// ---------------------------------------------------------------------------
// JSON helpers over the shared strict reader (config.rs)
// ---------------------------------------------------------------------------

fn parse_body(body: &[u8]) -> Result<Json, ManagementError> {
    let text = core::str::from_utf8(body)
        .map_err(|_| ManagementError::Parse("response body is not UTF-8".into()))?;
    parse_document(text).map_err(|e| ManagementError::Parse(format!("invalid JSON: {e}")))
}

fn j_get<'a>(v: &'a Json, key: &str) -> Option<&'a Json> {
    match v {
        Json::Obj(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
        _ => None,
    }
}

fn j_str(v: &Json, key: &str) -> Result<Option<String>, ManagementError> {
    match j_get(v, key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Str(s)) => Ok(Some(s.clone())),
        Some(_) => Err(ManagementError::Parse(format!("field '{key}' should be a string"))),
    }
}

fn j_bool(v: &Json, key: &str) -> Result<Option<bool>, ManagementError> {
    match j_get(v, key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ManagementError::Parse(format!("field '{key}' should be a boolean"))),
    }
}

fn j_required_str(v: &Json, key: &str) -> Result<String, ManagementError> {
    j_str(v, key)?.ok_or_else(|| ManagementError::Parse(format!("required field '{key}' missing")))
}

fn peer_from_json(v: &Json) -> Result<Peer, ManagementError> {
    if !matches!(v, Json::Obj(_)) {
        return Err(ManagementError::Parse("expected a JSON object for Peer".into()));
    }
    Ok(Peer {
        id: j_required_str(v, "id")?,
        name: j_required_str(v, "name")?,
        ip: j_str(v, "ip")?,
        connected: j_bool(v, "connected")?,
        hostname: j_str(v, "hostname")?,
        version: j_str(v, "version")?,
        dns_label: j_str(v, "dns_label")?,
    })
}

fn user_from_json(v: &Json) -> Result<User, ManagementError> {
    if !matches!(v, Json::Obj(_)) {
        return Err(ManagementError::Parse("expected a JSON object for User".into()));
    }
    Ok(User {
        id: j_required_str(v, "id")?,
        name: j_str(v, "name")?,
        email: j_str(v, "email")?,
        role: j_str(v, "role")?,
    })
}

/// Pull `{"message": ...}` out of an upstream error body
/// (`ErrorResponse{message,code}` in `shared/management/http/util/util.go`).
/// Falls back to a truncated raw-body excerpt.
fn error_message(body: &[u8]) -> String {
    if let Ok(json) = parse_body(body) {
        if let Ok(Some(m)) = j_str(&json, "message") {
            if !m.is_empty() {
                return m;
            }
        }
    }
    let text = String::from_utf8_lossy(body);
    let text = text.trim();
    if text.len() > 120 {
        format!("{}…", &text[..120])
    } else {
        text.to_string()
    }
}

// ---------------------------------------------------------------------------
// client
// ---------------------------------------------------------------------------

/// Management API client over any [`HttpTransport`]. Cheap to clone-free
/// reuse; every call is a fresh HTTP request.
pub struct ManagementClient<T: HttpTransport> {
    transport: T,
}

impl ManagementClient<PlainHttpTransport> {
    /// Build a client on the plain-text transport. `base_url` must be
    /// `http://host[:port]` — `https://` is an explicit
    /// [`ManagementError::UnsupportedUrl`] until TLS lands.
    pub fn new(base_url: &str, timeout: Duration) -> Result<Self, ManagementError> {
        let BaseUrl { host, port } = parse_base_url(base_url)?;
        Ok(ManagementClient {
            transport: PlainHttpTransport::new(&host, port, timeout)?,
        })
    }
}

impl ManagementClient<TlsHttpTransport> {
    /// Build a client on the TLS transport (N3-2). `base_url` must be
    /// `https://host[:port]`; `root_certs` are the DER-encoded CA
    /// certificate(s) to trust — injected by the caller (see
    /// [`decode_pem_certificates`] for the PEM form; NO system store is used).
    pub fn new_tls(
        base_url: &str,
        timeout: Duration,
        root_certs: Vec<CertificateDer<'static>>,
    ) -> Result<Self, ManagementError> {
        let BaseUrl { host, port } = parse_base_url_tls(base_url)?;
        Ok(ManagementClient {
            transport: TlsHttpTransport::new(&host, port, timeout, root_certs)?,
        })
    }
}

impl<T: HttpTransport> ManagementClient<T> {
    pub fn with_transport(transport: T) -> Self {
        ManagementClient { transport }
    }

    /// TODO(未确认) **Mock-only contract** — register this node with a setup
    /// key. Upstream has NO REST registration (see module docs): the real
    /// flow is gRPC `ManagementService/Login` with `LoginRequest.setupKey`.
    /// This call sends `POST /api/peers` with `{"setup_key":..., "name":...}`
    /// and parses the created Peer from the response. Against a real server
    /// this yields 404/405 → [`ManagementError::Request`].
    ///
    /// Note: no `Authorization` header — the setup key IS the credential in
    /// this contract (mirrors the gRPC semantics, where the setup key
    /// authorizes the Login RPC).
    pub fn register(&self, setup_key: &str, peer_name: &str) -> Result<Peer, ManagementError> {
        if setup_key.is_empty() {
            return Err(ManagementError::Request {
                status: 0,
                message: "setup key is empty".into(),
            });
        }
        let body = format!(
            "{{\"setup_key\":\"{}\",\"name\":\"{}\"}}",
            json_escape(setup_key),
            json_escape(peer_name)
        );
        let json = self.send(
            "POST",
            ENDPOINT_REGISTER_TODO_UNCONFIRMED,
            None,
            Some(&body),
        )?;
        peer_from_json(&json)
    }

    /// Establish a session from a JWT or PAT: the credential is verified
    /// with `GET /api/users/current` (upstream-confirmed) and bound to the
    /// returned `User`. An invalid/expired credential surfaces as
    /// [`ManagementError::Auth`].
    pub fn login(&self, credential: Credential) -> Result<Session, ManagementError> {
        let json = self.send(
            "GET",
            ENDPOINT_USERS_CURRENT,
            Some(credential.header_value()),
            None,
        )?;
        Ok(Session {
            user: user_from_json(&json)?,
            credential,
        })
    }

    /// `GET /api/peers` — all peers visible to the session.
    pub fn peers(&self, session: &Session) -> Result<Vec<Peer>, ManagementError> {
        let json = self.send("GET", ENDPOINT_PEERS, Some(session.authorization()), None)?;
        match json {
            Json::Arr(items) => items.iter().map(peer_from_json).collect(),
            _ => Err(ManagementError::Parse(
                "expected a JSON array of peers".into(),
            )),
        }
    }

    /// `GET /api/peers/{peerId}` — one peer's management-side config (the
    /// "本节点配置" fetch; `peer_id` = this node's peer id after register).
    pub fn peer(&self, session: &Session, peer_id: &str) -> Result<Peer, ManagementError> {
        if peer_id.is_empty() || peer_id.contains('/') {
            return Err(ManagementError::Request {
                status: 0,
                message: format!("invalid peer id {peer_id:?}"),
            });
        }
        let path = format!("{ENDPOINT_PEER_BY_ID}/{}", json_escape(peer_id));
        let json = self.send("GET", &path, Some(session.authorization()), None)?;
        peer_from_json(&json)
    }

    /// One request + status classification + JSON parse. Non-2xx bodies are
    /// best-effort decoded for `{"message":...}` and never parsed strictly.
    fn send(
        &self,
        method: &'static str,
        path: &str,
        authorization: Option<String>,
        body: Option<&str>,
    ) -> Result<Json, ManagementError> {
        let req = HttpRequest {
            method,
            path: path.to_string(),
            authorization,
            body: body.map(|b| b.to_string()),
        };
        let resp = self.transport.execute(&req)?;
        match resp.status {
            200..=299 => parse_body(&resp.body),
            401 | 403 => Err(ManagementError::Auth {
                status: resp.status,
                message: error_message(&resp.body),
            }),
            400..=499 => Err(ManagementError::Request {
                status: resp.status,
                message: error_message(&resp.body),
            }),
            500..=599 => Err(ManagementError::Server {
                status: resp.status,
            }),
            other => Err(ManagementError::Parse(format!(
                "unexpected status {other} (redirects are not followed)"
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// tests (unit: url parsing, chunked decoding, json field mapping)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_parsing() {
        assert_eq!(
            parse_base_url("http://127.0.0.1:8080").unwrap(),
            BaseUrl { host: "127.0.0.1".into(), port: 8080 }
        );
        assert_eq!(
            parse_base_url("http://mgmt.example.com").unwrap(),
            BaseUrl { host: "mgmt.example.com".into(), port: 80 }
        );
        assert_eq!(
            parse_base_url("http://host:80/").unwrap().port,
            80
        );
        assert_eq!(
            parse_base_url("https://mgmt.example.com:443").unwrap_err(),
            ManagementError::UnsupportedUrl("https://mgmt.example.com:443".into())
        );
        assert!(matches!(
            parse_base_url("ftp://x"),
            Err(ManagementError::UnsupportedUrl(_))
        ));
        assert!(matches!(
            parse_base_url("http://[::1]:80"),
            Err(ManagementError::UnsupportedUrl(_))
        ));
        assert!(parse_base_url("no-scheme").is_err());
    }

    #[test]
    fn chunked_decoding() {
        assert_eq!(
            decode_chunked(b"4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n").unwrap(),
            b"Wikipedia".to_vec()
        );
        assert_eq!(decode_chunked(b"0\r\n\r\n").unwrap(), Vec::<u8>::new());
        // chunk extensions + trailers tolerated
        assert_eq!(
            decode_chunked(b"4;ext=1\r\nWiki\r\n0\r\nX-Trailer: y\r\n\r\n").unwrap(),
            b"Wiki".to_vec()
        );
        assert!(decode_chunked(b"4\r\nWik").is_err());
        assert!(decode_chunked(b"zz\r\n").is_err());
    }

    #[test]
    fn peer_field_mapping_and_tolerance() {
        let doc = r#"{"id":"chacbco6","name":"stage-host-1","ip":"10.64.0.1",
            "connected":true,"hostname":"stage-host-1","version":"0.14.0",
            "dns_label":"stage-host-1.netbird.cloud",
            "created_at":"2023-05-05T09:00:35.477782Z","groups":[],"geoname_id":2643743}"#;
        let peer = peer_from_json(&parse_document(doc).unwrap()).unwrap();
        assert_eq!(peer.id, "chacbco6");
        assert_eq!(peer.ip.as_deref(), Some("10.64.0.1"));
        assert_eq!(peer.connected, Some(true));
        assert_eq!(peer.dns_label.as_deref(), Some("stage-host-1.netbird.cloud"));
        // unknown fields (created_at/groups/geoname_id) ignored
    }

    #[test]
    fn peer_missing_required_field_is_parse_error() {
        let err = peer_from_json(&parse_document(r#"{"id":"x"}"#).unwrap()).unwrap_err();
        assert!(err.to_string().contains("'name'"), "{err}");
    }

    #[test]
    fn error_message_extraction_from_upstream_shape() {
        let body = br#"{"message":"invalid setup key","code":401}"#;
        assert_eq!(error_message(body), "invalid setup key");
        assert_eq!(error_message(b"not json"), "not json");
    }

    #[test]
    fn credential_header_forms() {
        assert_eq!(Credential::Jwt("a.b.c".into()).header_value(), "Bearer a.b.c");
        assert_eq!(Credential::Pat("nbp_x".into()).header_value(), "Token nbp_x");
    }

    #[test]
    fn tls_base_url_parsing() {
        assert_eq!(
            parse_base_url_tls("https://mgmt.example.com:443").unwrap(),
            BaseUrl { host: "mgmt.example.com".into(), port: 443 }
        );
        assert_eq!(
            parse_base_url_tls("https://mgmt.example.com").unwrap().port,
            443,
            "https defaults to 443"
        );
        assert!(matches!(
            parse_base_url_tls("http://mgmt.example.com"),
            Err(ManagementError::UnsupportedUrl(_))
        ));
        assert!(matches!(
            parse_base_url_tls("ftp://x"),
            Err(ManagementError::UnsupportedUrl(_))
        ));
        assert!(parse_base_url_tls("no-scheme").is_err());
        assert!(parse_base_url_tls("https://[::1]:443").is_err());
    }

    #[test]
    fn pem_certificate_decoding() {
        // rcgen-shaped single PEM cert (self-signed, generated at test time in
        // tests/management_grpc.rs); here a deliberately broken PEM must fail.
        assert!(decode_pem_certificates(b"not a pem").is_err());
        assert!(decode_pem_certificates(b"").is_err());
        // a PEM block of the wrong section kind is rejected/filtered
        let key_only = b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n";
        assert!(decode_pem_certificates(key_only).is_err());
    }
}
