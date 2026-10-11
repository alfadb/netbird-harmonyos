// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors
//
//! # account_auth — the UI process's management login action (2026-10-12)
//!
//! The official Android SSO flow runs in the same process as the VPN service:
//! `client/android/client.go:194-215` (`Run` → `auth.login(urlOpener, …)`)
//! checks `IsLoginRequired`, runs the interactive PKCE flow and then performs
//! the management login/registration WITH the fresh JWT
//! (`client/internal/auth/auth.go:184-236` → `doMgmLogin` → `registerPeer`),
//! all BEFORE the engine starts — and keeps NO token on disk
//! (`client/internal/profilemanager/config.go:62-135` has no token field).
//!
//! In this shell the PKCE flow runs in the UI (ArkWeb) process, so the same
//! management step is exposed here as a NAPI job:
//!
//! * [`account_auth_discovery_start_json`] — the PKCE provider metadata
//!   (`GetPKCEAuthorizationFlow`).
//! * [`account_login_start_json`] — the login action's management step, run
//!   with the freshly exchanged JWT and the SAME device identity / management
//!   URL the extension dials (the caller passes the connector start config it
//!   would hand to `connector_start_with_socket`). The JWT is an in-memory
//!   local: it is never written anywhere, and it is gone when the job ends.
//! * [`account_extend_start_json`] — the same shape for the interactive SESSION
//!   EXTEND (`client/android/session.go:127-155`): the JWT goes to
//!   `ExtendAuthSession` and nowhere else, the tunnel is untouched, and the
//!   reply carries the server's own 3-state deadline
//!   (`client/internal/engine_authsession.go:83-107`).
//!
//! All are one-shot async jobs with the same start/poll/cancel shape; the
//! poll reply carries a stable token only (never a server message).

use crate::config::{self, Json};
use crate::envelope::EnvelopeKeyPair;
use crate::grpc::{GrpcTransport, ManagementGrpcClient};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tokio::task::JoinHandle;

const OVERALL_TIMEOUT: Duration = Duration::from_secs(20);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RPC_TIMEOUT: Duration = Duration::from_secs(10);
const INPUT_MAX: usize = 16 * 1024;
const RESPONSE_MAX: usize = 16 * 1024;

struct Input {
    management_url: String,
    ca_pem: String,
    server_name: Option<String>,
    /// `PKCEAuthorizationFlowRequest.SessionExtend` — TRUE only for the
    /// interactive SESSION EXTEND flow (`client/android/login.go:200-207`
    /// `foregroundGetTokenInfoFlow(…, sessionExtend=true)`); the ordinary
    /// login/register flow leaves it false.
    session_extend: bool,
}

struct Slot {
    generation: u64,
    task: Option<JoinHandle<()>>,
    result: Option<Result<String, &'static str>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
static LOGIN_SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();
static EXTEND_SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();

fn slot() -> &'static Mutex<Slot> {
    SLOT.get_or_init(|| {
        Mutex::new(Slot {
            generation: 0,
            task: None,
            result: None,
        })
    })
}

fn login_slot() -> &'static Mutex<Slot> {
    LOGIN_SLOT.get_or_init(|| {
        Mutex::new(Slot {
            generation: 0,
            task: None,
            result: None,
        })
    })
}

fn extend_slot() -> &'static Mutex<Slot> {
    EXTEND_SLOT.get_or_init(|| {
        Mutex::new(Slot {
            generation: 0,
            task: None,
            result: None,
        })
    })
}

/// Start `work` as a job in `slot` and return its request id, replacing (and
/// aborting) any job already running there.
fn start_job<F>(slot: &'static Mutex<Slot>, work: F) -> u64
where
    F: std::future::Future<Output = Result<String, &'static str>> + Send + 'static,
{
    let id = (NEXT_ID.fetch_add(1, Ordering::Relaxed) - 1) % i32::MAX as u64 + 1;
    {
        let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.generation = id;
        state.result = None;
    }
    let task = crate::connector::global_runtime().spawn(async move {
        let result = work.await;
        let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
        finish_if_current(&mut state, id, result);
    });
    let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
    if state.generation == id {
        state.task = Some(task);
    } else {
        task.abort();
    }
    id
}

/// Poll one job slot: `pending` / `done` (rendered by `done` from the job's own
/// stable payload) / `failed` (the job's stable error token) / `unknown` (stale
/// or never-seen id).
fn poll_job<F>(slot: &'static Mutex<Slot>, id: u64, done: F) -> String
where
    F: FnOnce(&str) -> String,
{
    let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
    if id == 0 || state.generation != id || (state.result.is_none() && state.task.is_none()) {
        return "{\"state\":\"unknown\"}".into();
    }
    match state.result.take() {
        None => "{\"state\":\"pending\"}".into(),
        Some(Ok(payload)) => {
            state.task.take();
            done(&payload)
        }
        Some(Err(error)) => {
            state.task.take();
            format!("{{\"state\":\"failed\",\"error\":\"{error}\"}}")
        }
    }
}

fn finish_if_current(state: &mut Slot, generation: u64, result: Result<String, &'static str>) {
    if state.generation == generation {
        state.result = Some(result);
    }
}

fn cancel_slot(state: &mut Slot, id: u64) -> bool {
    if state.generation != id || state.result.is_some() || state.task.is_none() {
        return false;
    }
    if let Some(task) = state.task.take() {
        task.abort();
    }
    state.generation = 0;
    state.result = None;
    true
}

fn parse_input(text: &str) -> Result<Input, &'static str> {
    if text.len() > INPUT_MAX {
        return Err("invalid-input");
    }
    let Json::Obj(fields) = config::parse_document(text).map_err(|_| "invalid-input")? else {
        return Err("invalid-input");
    };
    let get_str = |key: &str| -> Option<&str> {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                Json::Str(s) => Some(s.as_str()),
                _ => None,
            })
    };
    let management_url = get_str("management_url")
        .filter(|s| valid_https_url(s))
        .ok_or("invalid-input")?;
    let ca_pem = get_str("ca_pem")
        .filter(|s| !s.trim().is_empty())
        .ok_or("tls-required")?;
    let server_name = match fields.iter().find(|(k, _)| k == "server_name") {
        None => None,
        Some((_, Json::Str(s))) if !s.is_empty() => Some(s.clone()),
        _ => return Err("invalid-input"),
    };
    // optional, default false: the field is absent for the login/register
    // discovery (every pre-existing caller) and `true` only for the session
    // extend flow
    let session_extend = match fields.iter().find(|(k, _)| k == "session_extend") {
        None => false,
        Some((_, Json::Bool(b))) => *b,
        Some(_) => return Err("invalid-input"),
    };
    Ok(Input {
        management_url: management_url.into(),
        ca_pem: ca_pem.into(),
        server_name,
        session_extend,
    })
}

fn valid_https_url(value: &str) -> bool {
    if !value.starts_with("https://") || value.bytes().any(|b| b.is_ascii_whitespace()) {
        return false;
    }
    let authority = value[8..].split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let valid_port = |port: &str| port.is_empty() || port.parse::<u16>().is_ok();
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return false;
        };
        let host = &rest[..end];
        let suffix = &rest[end + 1..];
        !host.is_empty()
            && host.contains(':')
            && (suffix.is_empty() || suffix.strip_prefix(':').is_some_and(valid_port))
    } else {
        let (host, port) = authority.split_once(':').unwrap_or((authority, ""));
        !host.is_empty()
            && host != "."
            && !host.starts_with('.')
            && !host.ends_with('.')
            && host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
            && valid_port(port)
    }
}

#[allow(deprecated)]
fn public_projection(
    flow: crate::grpc::proto::PkceAuthorizationFlow,
) -> Result<String, &'static str> {
    let provider = flow.provider_config.ok_or("metadata-invalid")?;
    if !provider.client_secret.is_empty() {
        return Err("unsupported-client-secret");
    }
    if provider.client_id.is_empty()
        || !valid_https_url(&provider.authorization_endpoint)
        || !valid_https_url(&provider.token_endpoint)
    {
        return Err("metadata-invalid");
    }
    if provider.redirect_ur_ls.len() > 64 || provider.redirect_ur_ls.iter().any(|s| s.len() > 2048)
    {
        return Err("metadata-invalid");
    }
    let value = Json::Obj(vec![
        ("client_id".into(), Json::Str(provider.client_id)),
        ("audience".into(), Json::Str(provider.audience)),
        (
            "authorization_endpoint".into(),
            Json::Str(provider.authorization_endpoint),
        ),
        ("token_endpoint".into(), Json::Str(provider.token_endpoint)),
        ("scope".into(), Json::Str(provider.scope)),
        ("use_id_token".into(), Json::Bool(provider.use_id_token)),
        (
            "redirect_urls".into(),
            Json::Arr(provider.redirect_ur_ls.into_iter().map(Json::Str).collect()),
        ),
        (
            "disable_prompt_login".into(),
            Json::Bool(provider.disable_prompt_login),
        ),
        ("login_flag".into(), Json::Num(provider.login_flag as u64)),
    ]);
    let encoded = encode_json(&value);
    if encoded.len() > RESPONSE_MAX {
        return Err("metadata-invalid");
    }
    Ok(encoded)
}

fn encode_json(value: &Json) -> String {
    match value {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Num(n) => n.to_string(),
        Json::Str(s) => format!("\"{}\"", crate::util::json_escape(s)),
        Json::Arr(items) => format!(
            "[{}]",
            items.iter().map(encode_json).collect::<Vec<_>>().join(",")
        ),
        Json::Obj(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(|(k, v)| format!("\"{}\":{}", crate::util::json_escape(k), encode_json(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

async fn discover(input: Input) -> Result<String, &'static str> {
    let work = async {
        let keys = EnvelopeKeyPair::generate().map_err(|_| "network-error")?;
        let mut tls = crate::grpc::GrpcTlsConfig::new(vec![input.ca_pem.into_bytes()]);
        if let Some(name) = input.server_name {
            tls = tls.with_server_name(name);
        }
        let mut client = ManagementGrpcClient::connect(
            &input.management_url,
            GrpcTransport::Tls(tls),
            CONNECT_TIMEOUT,
            RPC_TIMEOUT,
            keys,
        )
        .await
        .map_err(|_| "network-error")?;
        let flow = client
            .get_pkce_authorization_flow(input.session_extend)
            .await
            .map_err(|_| "network-error")?;
        public_projection(flow)
    };
    tokio::time::timeout(OVERALL_TIMEOUT, work)
        .await
        .map_err(|_| "timeout")?
}

pub fn account_auth_discovery_start_json(request: &str) -> String {
    let input = match parse_input(request) {
        Ok(input) => input,
        Err(error) => return format!("{{\"ok\":false,\"error\":\"{error}\"}}"),
    };
    let id = start_job(slot(), discover(input));
    format!("{{\"ok\":true,\"request_id\":{id}}}")
}

pub fn account_auth_discovery_poll_json(id: u64) -> String {
    poll_job(slot(), id, |config| {
        format!("{{\"state\":\"done\",\"config\":{config}}}")
    })
}

pub fn account_auth_discovery_cancel_json(id: u64) -> String {
    cancel_json(slot(), id)
}

// ---------------------------------------------------------------------------
// login action (the management step of the official Android SSO flow)
// ---------------------------------------------------------------------------

/// What the login action's management step means for the caller — upstream
/// `Auth.Login`'s two outcomes (client/internal/auth/auth.go:208-236: the
/// key-less `doMgmLogin`, then `registerPeer` with the JWT).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginActionVerdict {
    /// The peer is logged in / freshly registered: the JWT was consumed by the
    /// registration. Nothing about the token is kept.
    Registered,
    /// The management server does not accept the peer even with the token
    /// (registration refused again, or the token itself rejected): the account
    /// still has to log in — upstream hands the error back to the app, whose
    /// UI reports the failed login.
    NeedsLogin,
}

/// Run the official login action's management step against an already
/// connected client: a KEY-LESS, TOKEN-LESS `Login` first; only when the server
/// answers with a login-needed code does the `Register` second step run,
/// carrying the JWT (upstream `Auth.Login` → `doMgmLogin` → `registerPeer`,
/// auth.go:208-236, `isLoginNeeded` = `InvalidArgument || PermissionDenied`,
/// auth.go:491-521; the wire call is
/// [`crate::grpc::ManagementGrpcClient::register`], grpc.go:641-648).
///
/// `jwt` is a local: the caller passes it in, nothing stores it.
pub async fn login_action(
    client: &mut ManagementGrpcClient,
    meta: &crate::grpc::PeerMeta,
    jwt: &str,
) -> Result<LoginActionVerdict, crate::management::ManagementError> {
    let params = |setup_key: &str, jwt_token: &str| crate::grpc::LoginParams {
        setup_key: setup_key.to_string(),
        jwt_token: jwt_token.to_string(),
        meta: meta.clone(),
        ..Default::default()
    };
    match client.login(params("", "")).await {
        Ok(_) => Ok(LoginActionVerdict::Registered),
        Err(err) if crate::grpc::is_login_needed(&err) => {
            match client.register(params("", jwt)).await {
                Ok(_) => Ok(LoginActionVerdict::Registered),
                // the token was rejected (already used / expired / not
                // permitted) or the registration was refused again: either way
                // this account still needs a real login, which is exactly the
                // verdict the UI shows. Transient classes stay errors so the
                // caller can retry.
                Err(err) if register_refusal(&err) => Ok(LoginActionVerdict::NeedsLogin),
                Err(err) => Err(err),
            }
        }
        Err(err) => Err(err),
    }
}

/// A `Register` verdict that means "this account is not logged in" rather than
/// "the request failed": `PermissionDenied` (refused again), `InvalidArgument`
/// (nothing usable to register with) and `Unauthenticated` (the JWT itself was
/// rejected).
fn register_refusal(err: &crate::management::ManagementError) -> bool {
    matches!(
        err.status_code(),
        crate::grpc::GRPC_PERMISSION_DENIED
            | crate::grpc::GRPC_INVALID_ARGUMENT
            | crate::grpc::GRPC_UNAUTHENTICATED
    )
}

/// Stable token for one management error of the login action (never a server
/// message): `network-error` / `timeout` / `management-error`.
fn login_error_token(err: &crate::management::ManagementError) -> &'static str {
    use crate::management::ManagementError;
    match err {
        ManagementError::Timeout => "timeout",
        ManagementError::Network(_) | ManagementError::UnsupportedUrl(_) => "network-error",
        _ => "management-error",
    }
}

/// The login action's job: dial the management endpoint from the caller's OWN
/// connector start config (same device identity / management URL / trust root
/// the extension uses) and run [`login_action`] with the freshly exchanged JWT.
/// The payload is the verdict's stable token (`success` / `needs-login`).
async fn login_with_jwt(
    config_json: String,
    jwt: String,
) -> Result<String, &'static str> {
    let work = async {
        let config = crate::connector::ConnectorConfig::from_json(&config_json)
            .map_err(|_| "invalid-config")?;
        let mut client = ManagementGrpcClient::connect(
            &config.management_url,
            config.transport,
            config.connect_timeout,
            config.request_timeout,
            config.keys,
        )
        .await
        .map_err(|_| "network-error")?;
        let verdict = login_action(&mut client, &config.meta, &jwt)
            .await
            .map_err(|e| login_error_token(&e))?;
        Ok(match verdict {
            LoginActionVerdict::Registered => "success".to_string(),
            LoginActionVerdict::NeedsLogin => "needs-login".to_string(),
        })
    };
    match tokio::time::timeout(OVERALL_TIMEOUT, work).await {
        Err(_) => Err("timeout"),
        Ok(result) => result,
    }
}

/// Parse the login-action request: the connector start config document the
/// extension would dial with plus the JWT from the token exchange. Both are
/// required; nothing else is read.
fn parse_login_input(text: &str) -> Result<(String, String), &'static str> {
    if text.len() > INPUT_MAX {
        return Err("invalid-input");
    }
    let Json::Obj(fields) = config::parse_document(text).map_err(|_| "invalid-input")? else {
        return Err("invalid-input");
    };
    let get_str = |key: &str| -> Option<&str> {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                Json::Str(s) => Some(s.as_str()),
                _ => None,
            })
    };
    let config = get_str("config").filter(|s| !s.is_empty()).ok_or("invalid-input")?;
    // the token is the whole point of this call: an empty one would just be a
    // wasted round trip, and upstream refuses it the same way (registerPeer,
    // auth.go:327-331)
    let jwt = get_str("jwt").filter(|s| !s.is_empty()).ok_or("jwt-required")?;
    Ok((config.to_string(), jwt.to_string()))
}

pub fn account_login_start_json(request: &str) -> String {
    let (config, jwt) = match parse_login_input(request) {
        Ok(input) => input,
        Err(error) => return format!("{{\"ok\":false,\"error\":\"{error}\"}}"),
    };
    let id = start_job(login_slot(), login_with_jwt(config, jwt));
    format!("{{\"ok\":true,\"request_id\":{id}}}")
}

pub fn account_login_poll_json(id: u64) -> String {
    poll_job(login_slot(), id, |outcome| {
        format!("{{\"state\":\"done\",\"outcome\":\"{outcome}\"}}")
    })
}

pub fn account_login_cancel_json(id: u64) -> String {
    cancel_json(login_slot(), id)
}

// ---------------------------------------------------------------------------
// session extend (the management step of the interactive ExtendAuthSession)
// ---------------------------------------------------------------------------

/// Render one wire deadline observation as the reply's 3-state fields, the SAME
/// token vocabulary the connector publishes (`session_expiry`:
/// `unknown` / `disabled` / `set`) plus the absolute Unix-seconds value (null
/// unless `set`). This is the server's own answer — the caller keeps its
/// previous anchor only on `unknown` (`None` = the response carried no
/// deadline at all, upstream `ApplySessionDeadline`'s nil no-op).
fn deadline_fields(wire: Option<i64>) -> String {
    use crate::connector::SessionDeadline;
    let mut deadline = SessionDeadline::Unknown;
    deadline.apply_wire(wire);
    let at = match deadline {
        SessionDeadline::At(t) => t.to_string(),
        _ => "null".to_string(),
    };
    format!(
        "\"session_expiry\":\"{}\",\"session_expires_at_unix\":{}",
        deadline.as_str(),
        at
    )
}

/// The extend's job: dial the management endpoint from the caller's OWN
/// connector start config (same device identity / management URL / trust root
/// the extension runs with) and run the management `ExtendAuthSession` with the
/// freshly exchanged JWT — upstream `Engine.ExtendAuthSession`
/// (`client/internal/engine_authsession.go:83-107`): the RPC with
/// `{jwtToken, meta}`, then `ApplySessionDeadline` on the response (L102).
///
/// The tunnel is untouched: no Login, no Sync, no reconnect (L71-73 comment).
/// `jwt` is a LOCAL: it leaves no trace outside this call frame, and nothing
/// about it is logged or returned. The payload is `outcome` + the 3-state
/// deadline fields.
async fn extend_with_jwt(config_json: String, jwt: String) -> Result<String, &'static str> {
    let work = async {
        let config = crate::connector::ConnectorConfig::from_json(&config_json)
            .map_err(|_| "invalid-config")?;
        let mut client = ManagementGrpcClient::connect(
            &config.management_url,
            config.transport,
            config.connect_timeout,
            config.request_timeout,
            config.keys,
        )
        .await
        .map_err(|_| "network-error")?;
        let outcome = client
            .extend_auth_session(&config.meta, &jwt)
            .await
            .map_err(|e| login_error_token(&e))?;
        // engine_authsession.go:102 — the response IS the new anchor. Inside
        // the ENGINE process this is the one write that moves the published
        // deadline; in the UI process (the production case, see
        // [`crate::connector::session_deadline_apply_live`]) there is no
        // connector to move, and the answer travels back to the caller below.
        crate::connector::session_deadline_apply_live(outcome.session_deadline_unix);
        Ok(format!(
            "\"outcome\":\"success\",{}",
            deadline_fields(outcome.session_deadline_unix)
        ))
    };
    match tokio::time::timeout(OVERALL_TIMEOUT, work).await {
        Err(_) => Err("timeout"),
        Ok(result) => result,
    }
}

/// Parse the extend request: exactly the login action's `{config, jwt}` shape —
/// the connector start config document to dial with plus the JWT from the
/// token exchange. Both are required; nothing else is read.
pub fn account_extend_start_json(request: &str) -> String {
    let (config, jwt) = match parse_login_input(request) {
        Ok(input) => input,
        Err(error) => return format!("{{\"ok\":false,\"error\":\"{error}\"}}"),
    };
    // Single-flight: upstream refuses a SECOND concurrent extend
    // (`session.go:189-198` "session extend already in progress") because the
    // PKCE step binds one fixed loopback port. Here that PKCE wait lives in the
    // UI (one ArkWeb surface, the caller's own busy latch), so the job slot
    // keeps the login trio's replace-and-abort semantics — a new start is the
    // caller's explicit statement that the previous flow is over.
    let id = start_job(extend_slot(), extend_with_jwt(config, jwt));
    format!("{{\"ok\":true,\"request_id\":{id}}}")
}

pub fn account_extend_poll_json(id: u64) -> String {
    poll_job(extend_slot(), id, |payload| {
        format!("{{\"state\":\"done\",{payload}}}")
    })
}

pub fn account_extend_cancel_json(id: u64) -> String {
    cancel_json(extend_slot(), id)
}

/// Shared cancel reply for the job slots.
fn cancel_json(slot: &'static Mutex<Slot>, id: u64) -> String {
    let mut state = slot.lock().unwrap_or_else(|e| e.into_inner());
    if id == 0 || !cancel_slot(&mut state, id) {
        return "{\"ok\":false,\"error\":\"unknown\"}".into();
    }
    "{\"ok\":true,\"cancelled\":true}".into()
}

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use crate::grpc::proto::{PkceAuthorizationFlow, ProviderConfig};

    fn valid_input() -> &'static str {
        r#"{"management_url":"https://mgmt.example","ca_pem":"CA"}"#
    }

    #[test]
    fn cold_poll_and_unknown_cancel() {
        assert_eq!(
            account_auth_discovery_poll_json(0),
            r#"{"state":"unknown"}"#
        );
        assert_eq!(
            account_auth_discovery_cancel_json(999),
            r#"{"ok":false,"error":"unknown"}"#
        );
    }

    #[test]
    fn malformed_and_http_inputs_are_rejected_without_network() {
        assert_eq!(
            account_auth_discovery_start_json("{"),
            r#"{"ok":false,"error":"invalid-input"}"#
        );
        assert_eq!(
            account_auth_discovery_start_json(
                r#"{"management_url":"http://mgmt.example","ca_pem":"CA"}"#
            ),
            r#"{"ok":false,"error":"invalid-input"}"#
        );
        assert_eq!(
            account_auth_discovery_start_json(r#"{"management_url":"https://mgmt.example"}"#),
            r#"{"ok":false,"error":"tls-required"}"#
        );
    }

    #[test]
    fn public_projection_excludes_secret_and_requires_fields() {
        let mut provider = ProviderConfig {
            client_id: "public-client".into(),
            authorization_endpoint: "https://idp.example/authorize".into(),
            token_endpoint: "https://idp.example/token".into(),
            client_secret: String::new(),
            ..Default::default()
        };
        let json = public_projection(PkceAuthorizationFlow {
            provider_config: Some(provider.clone()),
        })
        .unwrap();
        assert!(json.contains("\"client_id\":\"public-client\""));
        assert!(!json.contains("client_secret"));
        provider.client_secret = "must-not-leak".into();
        assert_eq!(
            public_projection(PkceAuthorizationFlow {
                provider_config: Some(provider)
            }),
            Err("unsupported-client-secret")
        );
        assert_eq!(
            public_projection(PkceAuthorizationFlow {
                provider_config: None
            }),
            Err("metadata-invalid")
        );
    }

    #[test]
    fn replacement_generation_ignores_stale_completion() {
        let mut state = Slot {
            generation: 12,
            task: None,
            result: None,
        };
        finish_if_current(&mut state, 11, Ok("stale".into()));
        assert!(state.result.is_none());
        finish_if_current(&mut state, 12, Ok("current".into()));
        assert_eq!(state.result, Some(Ok("current".into())));
    }

    #[test]
    fn cancel_stale_slot_aborts_only_current_task_and_drops_result() {
        let stale = crate::connector::global_runtime().spawn(std::future::pending::<()>());
        let current = crate::connector::global_runtime().spawn(std::future::pending::<()>());
        let mut state = Slot {
            generation: 22,
            task: Some(current),
            result: None,
        };
        finish_if_current(&mut state, 21, Ok("stale".into()));
        assert!(!cancel_slot(&mut state, 21));
        assert!(state.result.is_none());
        assert!(cancel_slot(&mut state, 22));
        assert!(state.task.is_none());
        assert_eq!(state.generation, 0);
        assert!(state.result.is_none());
        finish_if_current(&mut state, 22, Ok("late completion".into()));
        assert!(state.result.is_none());
        stale.abort();
    }

    #[test]
    fn start_accepts_valid_ca_configuration() {
        assert!(parse_input(valid_input()).is_ok());
    }

    // ---- 2026-10-12: the login action's request validation -----------------

    fn valid_login_input() -> String {
        use base64::Engine as _;
        let key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        let config = format!(
            "{{\"management_url\":\"https://mgmt.example\",\"ca_pem\":\"CA\",\
             \"private_key\":\"{key}\"}}"
        );
        format!(
            "{{\"config\":\"{}\",\"jwt\":\"header.payload.sig\"}}",
            crate::util::json_escape(&config)
        )
    }

    #[test]
    fn login_input_requires_a_config_and_a_non_empty_token() {
        // the happy shape: both fields, nothing else needed
        let (config, jwt) = parse_login_input(&valid_login_input()).expect("valid login input");
        assert!(config.contains("mgmt.example"));
        assert_eq!(jwt, "header.payload.sig");

        // a missing/empty token is refused BEFORE any I/O: the whole point of
        // this entry is the JWT registration
        for text in [
            r#"{"config":"{}"}"#,
            r#"{"config":"{}","jwt":""}"#,
            r#"{"config":"","jwt":"t"}"#,
            "{",
            "[]",
            r#"{"config":5,"jwt":"t"}"#,
            r#"{"config":"{}","jwt":5}"#,
        ] {
            assert!(
                parse_login_input(text).is_err(),
                "must be refused: {text}"
            );
        }
        assert_eq!(
            parse_login_input(r#"{"config":"{}"}"#),
            Err("jwt-required")
        );
        assert_eq!(parse_login_input("{"), Err("invalid-input"));
        // the input cap is enforced before parsing
        let huge = format!(
            "{{\"config\":\"{}\",\"jwt\":\"t\"}}",
            "x".repeat(INPUT_MAX)
        );
        assert_eq!(parse_login_input(&huge), Err("invalid-input"));
    }

    #[test]
    fn login_start_rejects_bad_input_without_network_and_cold_poll_is_unknown() {
        assert_eq!(
            account_login_start_json(r#"{"config":"{}"}"#),
            r#"{"ok":false,"error":"jwt-required"}"#
        );
        assert_eq!(
            account_login_start_json("not json"),
            r#"{"ok":false,"error":"invalid-input"}"#
        );
        assert_eq!(account_login_poll_json(0), r#"{"state":"unknown"}"#);
        assert_eq!(
            account_login_cancel_json(4242),
            r#"{"ok":false,"error":"unknown"}"#
        );
    }

    /// The login action's error vocabulary is the class-only token shape (no
    /// server text), and a `Register` refusal is the login-required verdict
    /// rather than an error.
    #[test]
    fn login_action_verdict_and_error_tokens() {
        use crate::management::ManagementError;
        assert_eq!(login_error_token(&ManagementError::Timeout), "timeout");
        assert_eq!(
            login_error_token(&ManagementError::Network("refused".into())),
            "network-error"
        );
        assert_eq!(
            login_error_token(&ManagementError::UnsupportedUrl("http://x".into())),
            "network-error"
        );
        assert_eq!(
            login_error_token(&ManagementError::Auth {
                status: 16,
                message: "JWT already used".into()
            }),
            "management-error"
        );
        // a refused registration (7 / 3 / 16) is a verdict; others are errors
        for status in [7u16, 3, 16] {
            assert!(register_refusal(&ManagementError::Auth {
                status,
                message: String::new()
            }));
        }
        assert!(!register_refusal(&ManagementError::Timeout));
        assert!(!register_refusal(&ManagementError::Server { status: 13 }));
    }

    // ---- 2026-10-13: the session extend --------------------------------

    /// `session_extend` is an OPTIONAL, strictly typed discovery field: absent
    /// (every pre-existing caller) = the ordinary login/register flow, `true`
    /// = the session extend flow (`client/android/login.go:204-207`).
    #[test]
    fn discovery_session_extend_is_optional_and_typed() {
        let base = parse_input(valid_input()).expect("valid input");
        assert!(!base.session_extend, "absent field = the login flow");
        assert!(parse_input(valid_input()).is_ok());
        for extend in [true, false] {
            let text = format!(
                "{{\"management_url\":\"https://mgmt.example\",\"ca_pem\":\"CA\",\
                 \"session_extend\":{extend}}}"
            );
            assert_eq!(parse_input(&text).expect("typed flag").session_extend, extend);
        }
        // a non-boolean is refused before any I/O
        for text in [
            r#"{"management_url":"https://mgmt.example","ca_pem":"CA","session_extend":"true"}"#,
            r#"{"management_url":"https://mgmt.example","ca_pem":"CA","session_extend":1}"#,
            r#"{"management_url":"https://mgmt.example","ca_pem":"CA","session_extend":null}"#,
        ] {
            assert_eq!(parse_input(text).err(), Some("invalid-input"), "{text}");
        }
    }

    /// The extend reply's 3-state, in the connector's own vocabulary: `unknown`
    /// (no info — the caller KEEPS its anchor) / `disabled` (explicit zero) /
    /// `set` + the absolute seconds (upstream `ApplySessionDeadline`,
    /// engine_authsession.go:32-66).
    #[test]
    fn deadline_fields_render_the_3_state() {
        assert_eq!(
            deadline_fields(None),
            r#""session_expiry":"unknown","session_expires_at_unix":null"#
        );
        assert_eq!(
            deadline_fields(Some(0)),
            r#""session_expiry":"disabled","session_expires_at_unix":null"#
        );
        assert_eq!(
            deadline_fields(Some(1_893_456_000)),
            r#""session_expiry":"set","session_expires_at_unix":1893456000"#
        );
    }

    /// The extend's failure vocabulary reuses the login action's class-only
    /// tokens (never a server message) and the same `{config, jwt}` request
    /// validation.
    #[test]
    fn extend_failure_tokens_and_request_shape() {
        use crate::management::ManagementError;
        assert_eq!(login_error_token(&ManagementError::Timeout), "timeout");
        assert_eq!(
            login_error_token(&ManagementError::Network("refused".into())),
            "network-error"
        );
        assert_eq!(
            login_error_token(&ManagementError::Auth { status: 16, message: "x".into() }),
            "management-error"
        );
        assert_eq!(
            account_extend_start_json(r#"{"config":"{}"}"#),
            r#"{"ok":false,"error":"jwt-required"}"#
        );
        assert_eq!(
            account_extend_start_json("{"),
            r#"{"ok":false,"error":"invalid-input"}"#
        );
        // both entries read the SAME document shape
        let (config, jwt) = parse_login_input(&valid_login_input()).expect("extend input");
        assert!(config.contains("mgmt.example"));
        assert_eq!(jwt, "header.payload.sig");
    }
}
