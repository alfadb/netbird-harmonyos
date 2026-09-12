//! Client configuration model (R3 skeleton): peer config for one WireGuard
//! tunnel — local private key, peer public key, optional preshared key,
//! endpoint (host:port), allowed IPs / routes (with a default-route flag),
//! DNS servers, MTU, optional listen port.
//!
//! Scope guard: this module is CONFIG ONLY. No management/signal protocol
//! interaction lives here (none is implemented anywhere in the crate yet).
//!
//! ## JSON input
//! `serde`/`serde_json` are NOT used: serde_json is not in the frozen offline
//! cargo cache, and this crate deliberately keeps "no external crates beyond
//! boringtun" (util.rs header). Instead a small strict JSON reader below maps
//! the document into [`ClientConfig`]. Accepted shape (unknown fields are
//! rejected so typos fail loudly instead of being silently ignored):
//!
//! ```json
//! {
//!   "private_key":    "<44-char base64, 32 bytes> (required)",
//!   "peer_public_key":"<44-char base64, 32 bytes> (required)",
//!   "preshared_key":  "<44-char base64, 32 bytes> | null (optional)",
//!   "endpoint":       "host:port (required)",
//!   "allowed_ips":    ["10.0.0.0/24", "0.0.0.0/0"] (required, may be empty),
//!   "dns_servers":    ["1.1.1.1"] (optional, IPv4 literals),
//!   "mtu":            1420 (optional, 576..=1500, default 1420),
//!   "listen_port":    51820 | null (optional, 0 = ephemeral)
//! }
//! ```
//!
//! Validation never panics: every failure — malformed JSON, bad key format/
//! length, bad CIDR, out-of-range MTU, bad endpoint — comes back as a
//! [`ConfigError`] with a field name and reason.
//!
//! IPv4 only for now (addresses, CIDR, endpoint hosts and DNS). IPv6 parsing
//! is deliberately not half-implemented; it is reported as an explicit
//! "not supported" error so callers see the boundary instead of a silent
//! misparse.

use crate::util::json_escape;

// ---------------------------------------------------------------------------
// public model
// ---------------------------------------------------------------------------

/// `host:port` of the peer. `host` is a hostname or an IPv4 literal; IPv6
/// literals are rejected with an explicit not-supported error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub host: String,
    pub port: u16,
    /// true when `host` parsed as a strict dotted-quad IPv4 literal.
    pub host_is_ipv4: bool,
}

impl Endpoint {
    pub fn to_host_port(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// One allowed-IP / route entry. `is_default()` marks `0.0.0.0/0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    /// Network address; host bits are required to be zero.
    pub addr: [u8; 4],
    pub prefix_len: u8,
}

impl Route {
    pub fn is_default(&self) -> bool {
        self.addr == [0, 0, 0, 0] && self.prefix_len == 0
    }
}

/// Fully validated client configuration for one peer/tunnel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    /// Local WireGuard private key (32 raw bytes).
    pub private_key: [u8; 32],
    /// Peer WireGuard public key (32 raw bytes).
    pub peer_public_key: [u8; 32],
    /// Optional WireGuard preshared key (32 raw bytes).
    pub preshared_key: Option<[u8; 32]>,
    pub endpoint: Endpoint,
    pub allowed_ips: Vec<Route>,
    /// DNS servers as IPv4 literals, in config order.
    pub dns_servers: Vec<[u8; 4]>,
    pub mtu: u16,
    /// None = do not bind a fixed port; Some(0) = ephemeral.
    pub listen_port: Option<u16>,
}

/// Default tunnel MTU when the document omits `mtu` (classic WireGuard
/// default: 1500 - 80 bytes of WireGuard/UDP/IP overhead over IPv6 worst case).
pub const DEFAULT_MTU: u16 = 1420;
/// Accepted MTU envelope: 576 = IPv4 minimum, 1500 = Ethernet.
pub const MTU_MIN: u16 = 576;
pub const MTU_MAX: u16 = 1500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// Malformed JSON document (byte offset is where parsing stopped).
    Json { offset: usize, msg: String },
    /// A named field is missing, duplicated, malformed or out of range.
    Field { field: &'static str, reason: String },
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigError::Json { offset, msg } => {
                write!(f, "invalid JSON at byte {offset}: {msg}")
            }
            ConfigError::Field { field, reason } => {
                write!(f, "config field '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

// ---------------------------------------------------------------------------
// entry points
// ---------------------------------------------------------------------------

impl ClientConfig {
    /// Parse + validate a JSON document into a [`ClientConfig`].
    pub fn from_json(text: &str) -> Result<ClientConfig, ConfigError> {
        let doc = parse_document(text)?;
        match doc {
            Json::Obj(entries) => config_from_entries(&entries),
            _ => Err(ConfigError::Field {
                field: "(root)",
                reason: "expected a JSON object".into(),
            }),
        }
    }
}

/// Validate-only helper returning the JSON verdict used by the NAPI export
/// `config_validate(json)`. On success the summary NEVER contains key
/// material — only shapes and counts.
pub fn config_validate_json(text: &str) -> String {
    match ClientConfig::from_json(text) {
        Ok(cfg) => {
            let default_route = cfg.allowed_ips.iter().any(Route::is_default);
            let listen = match cfg.listen_port {
                Some(p) => p.to_string(),
                None => "null".to_string(),
            };
            format!(
                "{{\"valid\":true,\"endpoint\":\"{}\",\"mtu\":{},\"routes\":{},\
                 \"default_route\":{},\"dns_servers\":{},\"preshared_key\":{},\
                 \"listen_port\":{}}}",
                json_escape(&cfg.endpoint.to_host_port()),
                cfg.mtu,
                cfg.allowed_ips.len(),
                default_route,
                cfg.dns_servers.len(),
                cfg.preshared_key.is_some(),
                listen,
            )
        }
        Err(e) => format!(
            "{{\"valid\":false,\"error\":\"{}\"}}",
            json_escape(&e.to_string())
        ),
    }
}

// ---------------------------------------------------------------------------
// field mapping + validation
// ---------------------------------------------------------------------------

fn config_from_entries(entries: &[(String, Json)]) -> Result<ClientConfig, ConfigError> {
    let mut private_key: Option<[u8; 32]> = None;
    let mut peer_public_key: Option<[u8; 32]> = None;
    let mut preshared_key: Option<Option<[u8; 32]>> = None;
    let mut endpoint: Option<Endpoint> = None;
    let mut allowed_ips: Option<Vec<Route>> = None;
    let mut dns_servers: Option<Vec<[u8; 4]>> = None;
    let mut mtu: Option<u16> = None;
    let mut listen_port: Option<Option<u16>> = None;

    for (key, val) in entries {
        macro_rules! dup {
            ($f:literal) => {
                return Err(ConfigError::Field {
                    field: $f,
                    reason: "duplicate field".into(),
                })
            };
        }
        match key.as_str() {
            "private_key" => {
                if private_key.is_some() {
                    dup!("private_key")
                }
                private_key = Some(field_key(val, "private_key")?);
            }
            "peer_public_key" => {
                if peer_public_key.is_some() {
                    dup!("peer_public_key")
                }
                peer_public_key = Some(field_key(val, "peer_public_key")?);
            }
            "preshared_key" => {
                if preshared_key.is_some() {
                    dup!("preshared_key")
                }
                preshared_key = Some(match val {
                    Json::Null => None,
                    v => Some(field_key(v, "preshared_key")?),
                });
            }
            "endpoint" => {
                if endpoint.is_some() {
                    dup!("endpoint")
                }
                let s = field_str(val, "endpoint")?;
                endpoint = Some(parse_endpoint(&s)?);
            }
            "allowed_ips" => {
                if allowed_ips.is_some() {
                    dup!("allowed_ips")
                }
                let arr = field_arr(val, "allowed_ips")?;
                let mut routes = Vec::with_capacity(arr.len());
                for item in arr {
                    let s = item.as_str_ok("allowed_ips")?;
                    routes.push(parse_cidr_v4(&s)?);
                }
                allowed_ips = Some(routes);
            }
            "dns_servers" => {
                if dns_servers.is_some() {
                    dup!("dns_servers")
                }
                let arr = field_arr(val, "dns_servers")?;
                let mut dns = Vec::with_capacity(arr.len());
                for item in arr {
                    let s = item.as_str_ok("dns_servers")?;
                    dns.push(parse_ipv4(&s).ok_or_else(|| ConfigError::Field {
                        field: "dns_servers",
                        reason: format!("'{s}' is not an IPv4 dotted quad"),
                    })?);
                }
                dns_servers = Some(dns);
            }
            "mtu" => {
                if mtu.is_some() {
                    dup!("mtu")
                }
                let n = field_num(val, "mtu")?;
                let m = u16::try_from(n).map_err(|_| mtu_range_err(n))?;
                if !(MTU_MIN..=MTU_MAX).contains(&m) {
                    return Err(mtu_range_err(n));
                }
                mtu = Some(m);
            }
            "listen_port" => {
                if listen_port.is_some() {
                    dup!("listen_port")
                }
                match val {
                    Json::Null => listen_port = Some(None),
                    v => {
                        let n = field_num(v, "listen_port")?;
                        let p = u16::try_from(n).map_err(|_| ConfigError::Field {
                            field: "listen_port",
                            reason: format!("{n} is outside 0..=65535"),
                        })?;
                        listen_port = Some(Some(p));
                    }
                }
            }
            other => {
                return Err(ConfigError::Field {
                    field: "(root)",
                    reason: format!("unknown field '{other}'"),
                })
            }
        }
    }

    let missing = |f: &'static str| ConfigError::Field {
        field: f,
        reason: "missing required field".into(),
    };
    Ok(ClientConfig {
        private_key: private_key.ok_or_else(|| missing("private_key"))?,
        peer_public_key: peer_public_key.ok_or_else(|| missing("peer_public_key"))?,
        preshared_key: preshared_key.unwrap_or(None),
        endpoint: endpoint.ok_or_else(|| missing("endpoint"))?,
        allowed_ips: allowed_ips.unwrap_or_default(),
        dns_servers: dns_servers.unwrap_or_default(),
        mtu: mtu.unwrap_or(DEFAULT_MTU),
        listen_port: listen_port.unwrap_or(None),
    })
}

fn mtu_range_err(got: u64) -> ConfigError {
    ConfigError::Field {
        field: "mtu",
        reason: format!("{got} is outside {MTU_MIN}..={MTU_MAX}"),
    }
}

/// 44-char base64 -> 32 raw bytes (strict RFC 4648, single '=' pad).
fn field_key(val: &Json, field: &'static str) -> Result<[u8; 32], ConfigError> {
    let s = field_str(val, field)?;
    parse_wg_key(&s).ok_or_else(|| ConfigError::Field {
        field,
        reason: "expected 44-char base64 encoding of a 32-byte key".into(),
    })
}

fn field_str<'a>(val: &'a Json, field: &'static str) -> Result<&'a str, ConfigError> {
    val.as_str_ok(field)
}

fn field_num(val: &Json, field: &'static str) -> Result<u64, ConfigError> {
    val.as_uint_ok(field)
}

fn field_arr<'a>(val: &'a Json, field: &'static str) -> Result<&'a [Json], ConfigError> {
    val.as_arr_ok(field)
}

impl Json {
    fn as_str_ok(&self, field: &'static str) -> Result<&str, ConfigError> {
        match self {
            Json::Str(s) => Ok(s),
            _ => Err(ConfigError::Field {
                field,
                reason: "expected a JSON string".into(),
            }),
        }
    }
    fn as_uint_ok(&self, field: &'static str) -> Result<u64, ConfigError> {
        match self {
            Json::Num(n) => Ok(*n),
            _ => Err(ConfigError::Field {
                field,
                reason: "expected a non-negative integer".into(),
            }),
        }
    }
    fn as_arr_ok(&self, field: &'static str) -> Result<&[Json], ConfigError> {
        match self {
            Json::Arr(items) => Ok(items),
            _ => Err(ConfigError::Field {
                field,
                reason: "expected a JSON array of strings".into(),
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// scalar parsers (no panics; explicit errors)
// ---------------------------------------------------------------------------

const B64_REV: fn(u8) -> Option<u8> = |c| match c {
    b'A'..=b'Z' => Some(c - b'A'),
    b'a'..=b'z' => Some(c - b'a' + 26),
    b'0'..=b'9' => Some(c - b'0' + 52),
    b'+' => Some(62),
    b'/' => Some(63),
    _ => None,
};

/// Strict WireGuard key text: exactly 44 chars, base64 (RFC 4648 std
/// alphabet) with one trailing '=', decoding to exactly 32 bytes.
pub fn parse_wg_key(s: &str) -> Option<[u8; 32]> {
    let b = s.as_bytes();
    if b.len() != 44 || b[43] != b'=' {
        return None;
    }
    let mut out = [0u8; 32];
    // 10 full quads: chars 0..40 -> bytes 0..30
    for (qi, quad) in b[..40].chunks_exact(4).enumerate() {
        let mut vals = [0u8; 4];
        for (i, &c) in quad.iter().enumerate() {
            vals[i] = B64_REV(c)?;
        }
        let n = ((vals[0] as u32) << 18)
            | ((vals[1] as u32) << 12)
            | ((vals[2] as u32) << 6)
            | (vals[3] as u32);
        out[qi * 3] = (n >> 16) as u8;
        out[qi * 3 + 1] = (n >> 8) as u8;
        out[qi * 3 + 2] = n as u8;
    }
    // final quad: 3 chars + '=' -> 2 bytes (30..32)
    let mut vals = [0u8; 3];
    for (i, &c) in b[40..43].iter().enumerate() {
        vals[i] = B64_REV(c)?;
    }
    let n = ((vals[0] as u32) << 18) | ((vals[1] as u32) << 12) | ((vals[2] as u32) << 6);
    out[30] = (n >> 16) as u8;
    out[31] = (n >> 8) as u8;
    Some(out)
}

/// Strict dotted-quad IPv4: 4 decimal octets 0..=255, no leading zeros
/// (except "0" itself), digits only.
pub fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || p.len() > 3 || (p.len() > 1 && p.starts_with('0')) {
            return None;
        }
        let mut v: u16 = 0;
        for c in p.bytes() {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + (c - b'0') as u16;
        }
        if v > 255 {
            return None;
        }
        out[i] = v as u8;
    }
    Some(out)
}

/// Strict IPv4 CIDR: `a.b.c.d` (implied /32) or `a.b.c.d/p`, p in 0..=32,
/// host bits must be zero.
pub fn parse_cidr_v4(s: &str) -> Result<Route, ConfigError> {
    let bad = |reason: String| -> ConfigError {
        ConfigError::Field {
            field: "allowed_ips",
            reason,
        }
    };
    let (addr_part, prefix) = match s.split_once('/') {
        None => (s, 32u8),
        Some((a, p)) => {
            let p: u8 = p.parse().map_err(|_| bad(format!("bad prefix length in '{s}'")))?;
            if p > 32 {
                return Err(bad(format!("prefix length {p} > 32 in '{s}'")));
            }
            (a, p)
        }
    };
    let addr = parse_ipv4(addr_part).ok_or_else(|| bad(format!("'{addr_part}' is not an IPv4 dotted quad")))?;
    // host bits must be zero (strict CIDR, RFC 4632)
    let prefix = prefix;
    let mask: u32 = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix as u32)
    };
    let net = u32::from_be_bytes(addr);
    if net & !mask != 0 {
        let host_zeroed = u32::to_be_bytes(net & mask);
        return Err(bad(format!(
            "host bits set in '{s}' (use {}.{}.{}.{}/{prefix})",
            host_zeroed[0], host_zeroed[1], host_zeroed[2], host_zeroed[3]
        )));
    }
    Ok(Route { addr, prefix_len: prefix })
}

/// `host:port` endpoint. host = hostname (RFC-ish charset/length) or IPv4
/// literal (fully validated). Port required, 1..=65535 (0 would make the
/// endpoint unusable as a remote). IPv6 (bare or bracketed) -> explicit
/// not-supported error.
pub fn parse_endpoint(s: &str) -> Result<Endpoint, ConfigError> {
    let bad = |reason: String| -> ConfigError {
        ConfigError::Field {
            field: "endpoint",
            reason,
        }
    };
    if s.starts_with('[') {
        return Err(bad("bracketed IPv6 endpoints not supported (IPv4 only)".into()));
    }
    // host must not contain ':'; exactly one ':' separating host and port
    if s.matches(':').count() != 1 {
        return Err(bad("expected exactly 'host:port'".into()));
    }
    let (host, port_str) = s.split_once(':').ok_or_else(|| bad("expected 'host:port'".into()))?;
    if host.is_empty() {
        return Err(bad("empty host".into()));
    }
    if host.len() > 253 {
        return Err(bad(format!("host longer than 253 chars ({})", host.len())));
    }
    for label in host.split('.') {
        if label.is_empty() {
            return Err(bad(format!("empty label in host '{host}'")));
        }
        if label.len() > 63 {
            return Err(bad(format!("label longer than 63 chars in host '{host}'")));
        }
        for c in label.bytes() {
            let ok = c.is_ascii_alphanumeric() || c == b'-' || c == b'_';
            if !ok {
                return Err(bad(format!(
                    "invalid character '{}' in host (letters/digits/-/_ only)",
                    c as char
                )));
            }
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(bad(format!("label starts/ends with '-' in host '{host}'")));
        }
    }
    let port: u32 = port_str.parse().map_err(|_| bad(format!("bad port '{port_str}'")))?;
    let port = u16::try_from(port).map_err(|_| bad(format!("port {port} outside 0..=65535")))?;
    if port == 0 {
        return Err(bad("port 0 is not a valid remote endpoint".into()));
    }
    // if it looks numeric it must be a VALID ipv4 literal (catch 999.1.1.1)
    let host_is_ipv4 = host.bytes().all(|c| c.is_ascii_digit() || c == b'.');
    if host_is_ipv4 && parse_ipv4(host).is_none() {
        return Err(bad(format!("'{host}' looks like an IPv4 literal but is invalid")));
    }
    Ok(Endpoint {
        host: host.to_string(),
        port,
        host_is_ipv4,
    })
}

// ---------------------------------------------------------------------------
// minimal strict JSON reader (serde_json is not in the offline cache)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Num(u64),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

const MAX_DEPTH: usize = 32;

fn parse_document(text: &str) -> Result<Json, ConfigError> {
    let mut p = Parser { s: text.as_bytes(), pos: 0, depth: 0 };
    let v = p.value()?;
    p.skip_ws();
    if p.pos != p.s.len() {
        return Err(p.err("trailing characters after JSON value"));
    }
    Ok(v)
}

struct Parser<'a> {
    s: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn err(&self, msg: &str) -> ConfigError {
        ConfigError::Json {
            offset: self.pos,
            msg: msg.into(),
        }
    }
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }
    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.pos += 1;
        }
    }
    fn expect(&mut self, c: u8) -> Result<(), ConfigError> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.err(&format!("expected '{}'", c as char)))
        }
    }

    fn value(&mut self) -> Result<Json, ConfigError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.err("JSON nesting too deep"));
        }
        self.skip_ws();
        let v = match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'n') => self.literal("null", Json::Null),
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(self.err("unexpected character")),
            None => Err(self.err("unexpected end of input")),
        };
        self.depth -= 1;
        v
    }

    fn literal(&mut self, word: &str, v: Json) -> Result<Json, ConfigError> {
        if self.s[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(v)
        } else {
            Err(self.err(&format!("invalid literal (expected '{word}')")))
        }
    }

    fn number(&mut self) -> Result<Json, ConfigError> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            return Err(self.err("negative numbers not accepted in config"));
        }
        while self.peek().map_or(false, |c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        if self.pos == start {
            return Err(self.err("bad number"));
        }
        let digits = &self.s[start..self.pos];
        if digits.len() > 1 && digits[0] == b'0' {
            return Err(self.err("leading zero in number"));
        }
        match self.peek() {
            Some(b'.' | b'e' | b'E') => {
                return Err(self.err("non-integer numbers not accepted in config"))
            }
            _ => {}
        }
        let text = core::str::from_utf8(digits).map_err(|_| self.err("bad number"))?;
        text.parse::<u64>()
            .map(Json::Num)
            .map_err(|_| self.err("number out of range"))
    }

    fn string(&mut self) -> Result<String, ConfigError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    let esc = self.peek().ok_or_else(|| self.err("unterminated escape"))?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => out.push(self.unicode_escape()?),
                        _ => return Err(self.err("invalid escape")),
                    }
                }
                Some(c) if c < 0x20 => {
                    return Err(self.err("raw control character in string"))
                }
                Some(_) => {
                    // input is &str (valid UTF-8); copy one full char
                    let rest = core::str::from_utf8(&self.s[self.pos..])
                        .map_err(|_| self.err("invalid UTF-8"))?;
                    let ch = rest.chars().next().ok_or_else(|| self.err("bad string"))?;
                    out.push(ch);
                    self.pos += ch.len_utf8();
                }
            }
        }
    }

    fn unicode_escape(&mut self) -> Result<char, ConfigError> {
        let hi = self.hex4()?;
        match hi {
            0xD800..=0xDBFF => {
                // surrogate pair: \uXXXX\uYYYY
                if self.peek() != Some(b'\\') {
                    return Err(self.err("lone high surrogate"));
                }
                self.pos += 1;
                if self.peek() != Some(b'u') {
                    return Err(self.err("lone high surrogate"));
                }
                self.pos += 1;
                let lo = self.hex4()?;
                if !(0xDC00..=0xDFFF).contains(&lo) {
                    return Err(self.err("invalid low surrogate"));
                }
                let cp = 0x10000 + (((hi - 0xD800) as u32) << 10) + (lo - 0xDC00) as u32;
                char::from_u32(cp).ok_or_else(|| self.err("invalid code point"))
            }
            0xDC00..=0xDFFF => Err(self.err("lone low surrogate")),
            _ => char::from_u32(hi as u32).ok_or_else(|| self.err("invalid code point")),
        }
    }

    fn hex4(&mut self) -> Result<u16, ConfigError> {
        let mut v: u16 = 0;
        for _ in 0..4 {
            let c = self.peek().ok_or_else(|| self.err("truncated \\u escape"))?;
            let d = (c as char)
                .to_digit(16)
                .ok_or_else(|| self.err("invalid hex digit in \\u escape"))?;
            v = v * 16 + d as u16;
            self.pos += 1;
        }
        Ok(v)
    }

    fn array(&mut self) -> Result<Json, ConfigError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(Json::Arr(items));
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
    }

    fn object(&mut self) -> Result<Json, ConfigError> {
        self.expect(b'{')?;
        let mut entries = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(entries));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            let val = self.value()?;
            entries.push((key, val));
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(Json::Obj(entries));
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 32 arbitrary bytes -> 44-char base64 helper (encoder from util.rs)
    fn key_text(seed: u8) -> String {
        let bytes: Vec<u8> = (0..32u8)
            .map(|i| i.wrapping_mul(seed).wrapping_add(seed))
            .collect();
        crate::util::base64(&bytes)
    }

    fn valid_doc() -> String {
        format!(
            "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
              \"endpoint\":\"vpn.example.com:51820\",\
              \"allowed_ips\":[\"10.0.0.0/24\",\"0.0.0.0/0\"],\
              \"dns_servers\":[\"1.1.1.1\"],\"mtu\":1360,\"listen_port\":33333}}",
            key_text(1),
            key_text(2)
        )
    }

    #[test]
    fn full_valid_config_parses() {
        let cfg = ClientConfig::from_json(&valid_doc()).expect("valid doc must parse");
        assert_eq!(cfg.endpoint.host, "vpn.example.com");
        assert_eq!(cfg.endpoint.port, 51820);
        assert!(!cfg.endpoint.host_is_ipv4);
        assert_eq!(cfg.mtu, 1360);
        assert_eq!(cfg.listen_port, Some(33333));
        assert_eq!(cfg.dns_servers, vec![[1, 1, 1, 1]]);
        assert_eq!(cfg.allowed_ips.len(), 2);
        assert!(cfg.allowed_ips[1].is_default());
        assert!(!cfg.allowed_ips[0].is_default());
        // key decodes back to the seeded bytes
        let expected: Vec<u8> = (0..32u8).map(|i| i.wrapping_add(1)).collect();
        assert_eq!(cfg.private_key, expected[..]);
    }

    #[test]
    fn minimal_config_gets_defaults() {
        let doc = format!(
            "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
              \"endpoint\":\"10.0.0.1:443\",\"allowed_ips\":[]}}",
            key_text(3),
            key_text(4)
        );
        let cfg = ClientConfig::from_json(&doc).expect("minimal doc must parse");
        assert_eq!(cfg.mtu, DEFAULT_MTU);
        assert_eq!(cfg.preshared_key, None);
        assert_eq!(cfg.listen_port, None);
        assert!(cfg.dns_servers.is_empty());
        assert!(cfg.allowed_ips.is_empty());
        assert_eq!(cfg.endpoint.host, "10.0.0.1");
        assert!(cfg.endpoint.host_is_ipv4);
    }

    #[test]
    fn explicit_null_optionals_accepted() {
        let doc = format!(
            "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
              \"preshared_key\":null,\"listen_port\":null,\
              \"endpoint\":\"peer:1\",\"allowed_ips\":[\"0.0.0.0/0\"]}}",
            key_text(5),
            key_text(6)
        );
        let cfg = ClientConfig::from_json(&doc).expect("null optionals accepted");
        assert_eq!(cfg.preshared_key, None);
        assert_eq!(cfg.listen_port, None);
        assert!(cfg.allowed_ips[0].is_default());
    }

    #[test]
    fn preshared_key_round_trips() {
        let doc = format!(
            "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
              \"preshared_key\":\"{}\",\
              \"endpoint\":\"p:80\",\"allowed_ips\":[\"192.168.0.0/16\"]}}",
            key_text(7),
            key_text(8),
            key_text(9)
        );
        let cfg = ClientConfig::from_json(&doc).expect("psk doc must parse");
        let expected: Vec<u8> = (0..32u8).map(|i| i.wrapping_mul(9).wrapping_add(9)).collect();
        assert_eq!(cfg.preshared_key.unwrap(), expected[..]);
    }

    #[test]
    fn rejects_bad_key_base64() {
        let doc = format!(
            "{{\"private_key\":\"not!!!a==valid==key==value==here\",\"\
             peer_public_key\":\"{}\",\"endpoint\":\"h:1\",\"allowed_ips\":[]}}",
            key_text(2)
        );
        let err = ClientConfig::from_json(&doc).unwrap_err();
        assert_eq!(err.to_string(), "config field 'private_key': expected 44-char base64 encoding of a 32-byte key");
    }

    #[test]
    fn rejects_key_with_wrong_length() {
        for truncated in [43usize, 42] {
            let mut k = key_text(1);
            k.truncate(truncated);
            let doc = format!(
                "{{\"private_key\":\"{k}\",\"peer_public_key\":\"{}\",\
                  \"endpoint\":\"h:1\",\"allowed_ips\":[]}}",
                key_text(2)
            );
            let err = ClientConfig::from_json(&doc).unwrap_err();
            assert!(
                err.to_string().contains("private_key"),
                "truncated to {truncated}: {err}"
            );
        }
        // one char too many
        let mut k = key_text(1);
        k.push('A');
        let doc = format!(
            "{{\"private_key\":\"{k}\",\"peer_public_key\":\"{}\",\
              \"endpoint\":\"h:1\",\"allowed_ips\":[]}}",
            key_text(2)
        );
        assert!(ClientConfig::from_json(&doc).is_err());
        // valid base64 of 16 bytes must not pass as a key
        let short = crate::util::base64(&[0u8; 16]);
        let doc = format!(
            "{{\"private_key\":\"{short}\",\"peer_public_key\":\"{}\",\
              \"endpoint\":\"h:1\",\"allowed_ips\":[]}}",
            key_text(2)
        );
        assert!(ClientConfig::from_json(&doc).is_err());
    }

    #[test]
    fn rejects_bad_cidr() {
        let mk = |route: &str| {
            format!(
                "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
                  \"endpoint\":\"h:1\",\"allowed_ips\":[\"{route}\"]}}",
                key_text(1),
                key_text(2)
            )
        };
        for route in ["10.0.0.0/33", "10.0.0/24", "300.1.1.1/8", "10.0.0.1/8", "010.1.0.0/8", "fe80::/10"] {
            let err = ClientConfig::from_json(&mk(route)).unwrap_err();
            assert!(
                err.to_string().contains("allowed_ips"),
                "route '{route}' rejected with wrong field: {err}"
            );
        }
        // /0 default route and bare address (implied /32) are fine
        assert!(ClientConfig::from_json(&mk("0.0.0.0/0")).is_ok());
        assert!(ClientConfig::from_json(&mk("10.1.2.3")).is_ok());
    }

    #[test]
    fn rejects_bad_mtu() {
        let mk = |mtu: &str| {
            format!(
                "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
                  \"endpoint\":\"h:1\",\"allowed_ips\":[],\"mtu\":{mtu}}}",
                key_text(1),
                key_text(2)
            )
        };
        for mtu in ["0", "575", "1501", "65536", "-1", "1420.5", "\"1420\""] {
            assert!(
                ClientConfig::from_json(&mk(mtu)).is_err(),
                "mtu {mtu} must be rejected"
            );
        }
        assert!(ClientConfig::from_json(&mk("576")).is_ok());
        assert!(ClientConfig::from_json(&mk("1500")).is_ok());
    }

    #[test]
    fn rejects_bad_endpoint() {
        let mk = |ep: &str| {
            format!(
                "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
                  \"endpoint\":\"{ep}\",\"allowed_ips\":[]}}",
                key_text(1),
                key_text(2)
            )
        };
        for ep in [
            ":51820",
            "host:",
            "host:0",
            "host:70000",
            "host:x",
            "host :51820",
            "[::1]:51820",
            "host:51820:9",
            "999.1.1.1:80",
            "1.2.3.4:80", // valid, control below
        ] {
            let res = ClientConfig::from_json(&mk(ep));
            if ep == "1.2.3.4:80" {
                assert!(res.is_ok(), "valid ipv4 endpoint rejected: {err:?}", err = res.err());
            } else {
                assert!(res.is_err(), "endpoint '{ep}' must be rejected");
            }
        }
    }

    #[test]
    fn rejects_bad_dns_and_unknown_and_duplicate_fields() {
        let mk = |dns: &str| {
            format!(
                "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\",\
                  \"endpoint\":\"h:1\",\"allowed_ips\":[],\"dns_servers\":{dns}}}",
                key_text(1),
                key_text(2)
            )
        };
        for dns in ["\"1.2.3\"", "\"1.2.3.4.5\"", "\"a.b.c.d\"", "\"010.1.1.1\"", "42"] {
            assert!(
                ClientConfig::from_json(&mk(dns)).is_err(),
                "dns {dns} must be rejected"
            );
        }
        let base = valid_doc();
        // unknown field
        let mut doc = base.trim_end_matches('}').to_string();
        doc.push_str(",\"typo_filed\":1}");
        let err = ClientConfig::from_json(&doc).unwrap_err();
        assert!(err.to_string().contains("unknown field 'typo_filed'"), "{err}");
        // duplicate required field
        let doc = format!(
            "{{\"private_key\":\"{}\",\"private_key\":\"{}\",\
              \"peer_public_key\":\"{}\",\"endpoint\":\"h:1\",\"allowed_ips\":[]}}",
            key_text(1),
            key_text(3),
            key_text(2)
        );
        let err = ClientConfig::from_json(&doc).unwrap_err();
        assert!(err.to_string().contains("duplicate field"), "{err}");
    }

    #[test]
    fn rejects_missing_required_fields() {
        let err = ClientConfig::from_json("{}").unwrap_err();
        assert!(err.to_string().contains("missing required field"), "{err}");
        let doc = format!(
            "{{\"private_key\":\"{}\",\"peer_public_key\":\"{}\"}}",
            key_text(1),
            key_text(2)
        );
        let err = ClientConfig::from_json(&doc).unwrap_err();
        assert!(err.to_string().contains("'endpoint'"), "{err}");
    }

    #[test]
    fn rejects_malformed_json_without_panicking() {
        let base = valid_doc();
        let cases = [
            String::new(),
            "{".into(),
            "null".into(),
            "[]".into(),
            "\"str\"".into(),
            "{\"a\":".into(),
            "{\"a\" 1}".into(),
            "{\"a\":\"\\x\"}".into(),
            "{\"a\":01}".into(),
            "[1,2,]]".into(),
            format!("{base} garbage"),
            "{\"a\":\"unterminated".into(),
            "{\"a\":\"\\ud800 lone\"}".into(),
            "{'a':1}".into(),
        ];
        for case in cases {
            let err = ClientConfig::from_json(&case).unwrap_err();
            assert!(!err.to_string().is_empty(), "case {case:?} gave empty error");
        }
        // deep nesting must not blow the stack
        let deep = "[".repeat(200) + &"]".repeat(200);
        assert!(ClientConfig::from_json(&deep).is_err());
    }

    #[test]
    fn validate_json_report_never_contains_key_material() {
        let ok = config_validate_json(&valid_doc());
        assert!(ok.contains("\"valid\":true"), "{ok}");
        assert!(ok.contains("\"default_route\":true"), "{ok}");
        assert!(!ok.contains(key_text(1).as_str()), "summary leaked private key: {ok}");
        let bad = config_validate_json("{\"private_key\":\"oops\"}");
        assert!(bad.contains("\"valid\":false"), "{bad}");
        assert!(bad.contains("\"error\""), "{bad}");
    }

    #[test]
    fn scalar_parsers_direct() {
        assert!(parse_ipv4("1.2.3.4").is_some());
        assert!(parse_ipv4("0.0.0.0").is_some());
        assert!(parse_ipv4("1.2.3").is_none());
        assert!(parse_ipv4("1.2.3.4.5").is_none());
        assert!(parse_ipv4("256.1.1.1").is_none());
        assert!(parse_ipv4("01.2.3.4").is_none());
        let r = parse_cidr_v4("10.0.0.0/8").unwrap();
        assert_eq!(r.prefix_len, 8);
        assert_eq!(r.addr, [10, 0, 0, 0]);
        assert!(parse_cidr_v4("10.1.2.3/8").is_err(), "host bits must be rejected");
        assert_eq!(parse_cidr_v4("10.1.2.3").unwrap().prefix_len, 32);
        let k = key_text(11);
        let decoded = parse_wg_key(&k).unwrap();
        let expected: Vec<u8> = (0..32u8).map(|i| i.wrapping_mul(11).wrapping_add(11)).collect();
        assert_eq!(decoded, expected[..]);
        assert!(parse_wg_key("AAAA").is_none());
        assert!(parse_wg_key("AAAAAAA=").is_none());
    }
}
