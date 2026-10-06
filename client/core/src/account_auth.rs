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
}

struct Slot {
    generation: u64,
    task: Option<JoinHandle<()>>,
    result: Option<Result<String, &'static str>>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static SLOT: OnceLock<Mutex<Slot>> = OnceLock::new();

fn slot() -> &'static Mutex<Slot> {
    SLOT.get_or_init(|| {
        Mutex::new(Slot {
            generation: 0,
            task: None,
            result: None,
        })
    })
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
    Ok(Input {
        management_url: management_url.into(),
        ca_pem: ca_pem.into(),
        server_name,
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
            .get_pkce_authorization_flow()
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
    let id = (NEXT_ID.fetch_add(1, Ordering::Relaxed) - 1) % i32::MAX as u64 + 1;
    {
        let mut state = slot().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(task) = state.task.take() {
            task.abort();
        }
        state.generation = id;
        state.result = None;
    }
    let task = crate::connector::global_runtime().spawn(async move {
        let result = discover(input).await;
        let mut state = slot().lock().unwrap_or_else(|e| e.into_inner());
        finish_if_current(&mut state, id, result);
    });
    let mut state = slot().lock().unwrap_or_else(|e| e.into_inner());
    if state.generation == id {
        state.task = Some(task);
    } else {
        task.abort();
    }
    format!("{{\"ok\":true,\"request_id\":{id}}}")
}

pub fn account_auth_discovery_poll_json(id: u64) -> String {
    let mut state = slot().lock().unwrap_or_else(|e| e.into_inner());
    if id == 0 || state.generation != id || (state.result.is_none() && state.task.is_none()) {
        return "{\"state\":\"unknown\"}".into();
    }
    match state.result.take() {
        None => "{\"state\":\"pending\"}".into(),
        Some(Ok(config)) => {
            state.task.take();
            format!("{{\"state\":\"done\",\"config\":{config}}}")
        }
        Some(Err(error)) => {
            state.task.take();
            format!("{{\"state\":\"failed\",\"error\":\"{error}\"}}")
        }
    }
}

pub fn account_auth_discovery_cancel_json(id: u64) -> String {
    let mut state = slot().lock().unwrap_or_else(|e| e.into_inner());
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
}
