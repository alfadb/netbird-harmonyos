// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! 006: the ADDITIVE native resource snapshot endpoint
//! (`connector_resource_status` / `connector_resource_status_json`).
//!
//! ONE test fn on purpose: the endpoint reads a PROCESS-GLOBAL connector slot,
//! so the cold → started → stopped → restarted sequence must not race itself.
//! The document is validated with a STRICT reader (the crate's own reader is
//! crate-private) and every section's key set is pinned, so a future edit that
//! silently changes the schema or leaks material fails here.

use std::time::{Duration, Instant};

use base64::Engine as _;
use netbird_core::connector::{
    connector_resource_status_json, connector_start_json, connector_status_json, connector_stop_json,
};

// Host-test link surface (same as tests/connector.rs): the test binary links the
// whole crate rlib on the host triple, where libace_napi.z.so /
// libhilog_ndk.z.so do not exist. No-ops for the linker.
mod host_link_stubs {
    use core::ffi::c_void;

    #[no_mangle]
    pub extern "C" fn OH_LOG_Print(
        _log_type: i32,
        _level: i32,
        _domain: u32,
        _tag: *const u8,
        _fmt: *const u8,
        _arg: *const c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn OH_LOG_IsLoggable(_domain: u32, _tag: *const u8, _level: i32) -> bool {
        false
    }

    #[no_mangle]
    pub extern "C" fn napi_module_register(_mod_: *mut c_void) {}

    #[no_mangle]
    pub extern "C" fn napi_create_function(
        _env: *mut c_void,
        _utf8name: *const u8,
        _length: usize,
        _cb: *const c_void,
        _data: *mut c_void,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_set_named_property(
        _env: *mut c_void,
        _name: *const c_void,
        _value: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_create_string_utf8(
        _env: *mut c_void,
        _name: *const u8,
        _length: usize,
        _result: *mut *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_cb_info(
        _env: *mut c_void,
        _cbinfo: *mut c_void,
        _argc: *mut usize,
        _argv: *mut *mut c_void,
        _this: *mut c_void,
        _data: *mut c_void,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_string_utf8(
        _env: *mut c_void,
        _value: *mut c_void,
        _buf: *mut u8,
        _bufsize: usize,
        _result: *mut usize,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_int32(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut i32,
    ) -> i32 {
        0
    }

    #[no_mangle]
    pub extern "C" fn napi_get_value_bool(
        _env: *mut c_void,
        _value: *mut c_void,
        _result: *mut bool,
    ) -> i32 {
        0
    }
}

const SENTINEL_SETUP_KEY: &str = "SENTINEL-006-SETUP-KEY-DO-NOT-LEAK";

fn private_key_b64() -> String {
    base64::engine::general_purpose::STANDARD.encode([7u8; 32])
}

fn config_json() -> String {
    format!(
        "{{\"management_url\":\"http://127.0.0.1:1\",\"private_key\":\"{}\",\
          \"hostname\":\"ohos-resource\",\"allow_unprotected_management\":true}}",
        private_key_b64()
    )
}

fn secrets_json() -> String {
    format!("{{\"setup_key\":\"{SENTINEL_SETUP_KEY}\"}}")
}

#[test]
fn resource_status_snapshot_cold_start_stop_restart_and_strict_shape() {
    // -----------------------------------------------------------------------
    // COLD: no connector in this process yet.
    // -----------------------------------------------------------------------
    let t0 = Instant::now();
    let cold_doc = connector_resource_status_json();
    assert!(
        t0.elapsed() < Duration::from_secs(1),
        "the snapshot must be non-blocking by construction: {cold_doc}"
    );
    let cold = parse_json(&cold_doc);
    assert_eq!(
        cold.keys().join(","),
        "connector,dns_lookups,ice,ice_sockets,ok,relay_carrier,socket_supply,wg"
    );
    assert!(cold.at("ok").bool_of());

    let connector = cold.at("connector");
    assert_eq!(
        connector.keys().join(","),
        "generation,present,running,starts_total,state,stops_total"
    );
    assert!(!connector.at("present").bool_of(), "no connector ⇒ explicit present:false");
    assert!(!connector.at("running").bool_of(), "no connector ⇒ explicitly not running");
    assert!(connector.at("generation").is_null(), "no connector ⇒ no generation");
    assert!(connector.at("state").is_null(), "no connector object ⇒ no state token");
    let cold_starts = connector.at("starts_total").uint_of();
    let cold_stops = connector.at("stops_total").uint_of();

    // Connector-owned resources are a KNOWN disposed state: `available` is
    // false AND every count is an honest zero (nothing is owned).
    let ice = cold.at("ice");
    assert_eq!(
        ice.keys().join(","),
        "available,live_local_sockets,sessions_live,sessions_present,sessions_stopped"
    );
    assert!(!ice.at("available").bool_of());
    assert_eq!(ice.at("sessions_present").uint_of(), 0);
    assert_eq!(ice.at("sessions_live").uint_of(), 0);
    assert_eq!(ice.at("sessions_stopped").uint_of(), 0);
    assert_eq!(ice.at("live_local_sockets").uint_of(), 0);

    let ice_sockets = cold.at("ice_sockets");
    assert_eq!(ice_sockets.keys().join(","), "available,queued,taken");
    assert!(!ice_sockets.at("available").bool_of());
    assert_eq!(ice_sockets.at("queued").uint_of(), 0);
    assert_eq!(ice_sockets.at("taken").uint_of(), 0);

    let supply = cold.at("socket_supply");
    assert_eq!(supply.keys().join(","), "installed,pending,queued");
    assert!(!supply.at("installed").bool_of(), "no start ⇒ no installed supply");
    assert_eq!(supply.at("pending").uint_of(), 0);
    assert_eq!(supply.at("queued").uint_of(), 0);

    let relay = cold.at("relay_carrier");
    assert_eq!(relay.keys().join(","), "attached_lanes,available,foreign_clients,pending_opens");
    assert!(!relay.at("available").bool_of(), "no carrier armed");
    assert_eq!(relay.at("attached_lanes").uint_of(), 0);
    assert_eq!(relay.at("pending_opens").uint_of(), 0);
    assert_eq!(relay.at("foreign_clients").uint_of(), 0);

    // The DNS lookup map always exists (independent of the connector slot), so
    // its depth is a real number — never null while uncontended.
    let dns = cold.at("dns_lookups");
    assert_eq!(dns.keys().join(","), "pending_receivers");
    assert!(!dns.at("pending_receivers").is_null(), "the DNS map is a live global");

    let wg = cold.at("wg");
    assert_eq!(wg.keys().join(","), "available,reason");
    assert!(!wg.at("available").bool_of());
    assert_eq!(wg.at("reason").str_of(), "not-collected");
    // Evidence: the exact documents (visible with `-- --nocapture`).
    println!("[006 cold] {cold_doc}");

    // -----------------------------------------------------------------------
    // START: one successful start over the UNPROTECTED dev opt-in (nothing
    // listens on 127.0.0.1:1, so the worker retries in the background).
    // -----------------------------------------------------------------------
    let started = connector_start_json(&config_json(), &secrets_json());
    assert!(started.contains("\"started\":true"), "{started}");

    let running_doc = connector_resource_status_json();
    let running = parse_json(&running_doc);
    let connector = running.at("connector");
    assert!(connector.at("present").bool_of());
    assert!(connector.at("running").bool_of());
    let generation1 = connector.at("generation").uint_of();
    assert_eq!(
        connector.at("starts_total").uint_of(),
        generation1,
        "the generation IS this handle's successful-start ordinal"
    );
    assert_eq!(
        connector.at("starts_total").uint_of(),
        cold_starts + 1,
        "exactly one start happened since the cold read"
    );
    assert_eq!(connector.at("stops_total").uint_of(), cold_stops);
    assert!(
        matches!(connector.at("state").str_of(), "connecting" | "reconnecting"),
        "an unreachable endpoint keeps the worker retrying: {running_doc}"
    );

    let ice = running.at("ice");
    assert!(ice.at("available").bool_of(), "a real spawn builds the orchestrator");
    assert_eq!(ice.at("sessions_present").uint_of(), 0, "no peer ⇒ no session object");
    assert_eq!(ice.at("sessions_live").uint_of(), 0);
    assert_eq!(ice.at("sessions_stopped").uint_of(), 0);
    assert_eq!(ice.at("live_local_sockets").uint_of(), 0);

    let ice_sockets = running.at("ice_sockets");
    assert!(ice_sockets.at("available").bool_of(), "the production spawn attaches the UDP source");
    assert_eq!(ice_sockets.at("queued").uint_of(), 0);
    assert_eq!(ice_sockets.at("taken").uint_of(), 0);

    // This start path carries no protected source, so the global supply is
    // absent — a KNOWN zero, not an unknown.
    let supply = running.at("socket_supply");
    assert!(!supply.at("installed").bool_of());
    assert_eq!(supply.at("pending").uint_of(), 0);
    assert_eq!(supply.at("queued").uint_of(), 0);

    let relay = running.at("relay_carrier");
    assert!(!relay.at("available").bool_of(), "the relay opt-in is off by default");
    assert_eq!(relay.at("attached_lanes").uint_of(), 0);
    assert_eq!(relay.at("pending_opens").uint_of(), 0);
    assert_eq!(relay.at("foreign_clients").uint_of(), 0);
    println!("[006 running] {running_doc}");

    // -----------------------------------------------------------------------
    // The frozen `connector_status()` document is UNTOUCHED: first field and
    // the pre-existing trailing `ice_sockets` section keep their positions,
    // and no additive key leaks into it.
    // -----------------------------------------------------------------------
    let frozen = connector_status_json();
    assert!(frozen.starts_with("{\"running\":"), "{frozen}");
    assert!(frozen.contains("\"ice_sockets\":{\"queued\":"), "{frozen}");
    assert!(frozen.trim_end().ends_with("}}"), "{frozen}");
    for leaked in ["generation", "live_local_sockets", "sessions_present", "resource_status"] {
        assert!(!frozen.contains(leaked), "additive field `{leaked}` leaked into the frozen doc");
    }

    // -----------------------------------------------------------------------
    // STOP: the handle is disposed; the cumulative counters survive.
    // -----------------------------------------------------------------------
    let stopped = connector_stop_json();
    assert!(stopped.contains("\"already_stopped\":false"), "{stopped}");
    let after_stop = parse_json(&connector_resource_status_json());
    let connector = after_stop.at("connector");
    assert!(!connector.at("present").bool_of());
    assert!(!connector.at("running").bool_of());
    assert!(connector.at("generation").is_null(), "a disposed handle has no generation");
    assert_eq!(connector.at("starts_total").uint_of(), generation1);
    assert_eq!(connector.at("stops_total").uint_of(), cold_stops + 1, "one REAL stop");
    assert!(!after_stop.at("ice").at("available").bool_of());
    assert!(!after_stop.at("socket_supply").at("installed").bool_of());

    // An idempotent repeat is NOT counted as another stop.
    let stopped_again = connector_stop_json();
    assert!(stopped_again.contains("\"already_stopped\":true"), "{stopped_again}");
    let after_repeat = parse_json(&connector_resource_status_json());
    assert_eq!(after_repeat.at("connector").at("stops_total").uint_of(), cold_stops + 1);

    // -----------------------------------------------------------------------
    // RESTART: a NEW generation; the cumulative counters are retained, and the
    // net change is 1 (an identity/accounting fact — NOT a live-thread count).
    // -----------------------------------------------------------------------
    let started2 = connector_start_json(&config_json(), &secrets_json());
    assert!(started2.contains("\"started\":true"), "{started2}");
    let second_doc = connector_resource_status_json();
    let second = parse_json(&second_doc);
    let connector = second.at("connector");
    let generation2 = connector.at("generation").uint_of();
    assert!(
        generation2 > generation1,
        "generation must be unique per successful start: {generation2} vs {generation1}"
    );
    assert_eq!(connector.at("starts_total").uint_of(), generation2);
    assert_eq!(connector.at("stops_total").uint_of(), cold_stops + 1);
    assert!(connector.at("running").bool_of());
    assert_eq!(
        connector.at("starts_total").uint_of() - connector.at("stops_total").uint_of(),
        1,
        "exactly ONE connector is live right now, whatever the generation says"
    );
    assert!(connector_stop_json().contains("\"already_stopped\":false"));

    // -----------------------------------------------------------------------
    // Secret / material absence: counts only, in every resource document.
    // -----------------------------------------------------------------------
    for doc in [&cold_doc, &running_doc, &second_doc] {
        assert!(!doc.contains(SENTINEL_SETUP_KEY), "setup key leaked: {doc}");
        assert!(!doc.contains(&private_key_b64()), "private key leaked: {doc}");
        for forbidden in [
            "private_key",
            "setup_key",
            "jwt",
            "token",
            "pub_key",
            "://",
            "\"fd\"",
            "127.0.0.1",
            "ohos-resource",
        ] {
            assert!(!doc.contains(forbidden), "forbidden token `{forbidden}` in {doc}");
        }
    }
}

// ---------------------------------------------------------------------------
// a minimal STRICT JSON reader (the crate's own reader is crate-private) —
// rejects malformed / trailing content instead of silently accepting it, and
// refuses negative numbers the same way the crate's `parse_document` does.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(fields) => fields.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    fn at(&self, key: &str) -> &Json {
        self.get(key).unwrap_or_else(|| panic!("missing key `{key}` in {self:?}"))
    }

    /// Sorted key list — the exact-shape assertions.
    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = match self {
            Json::Obj(fields) => fields.iter().map(|(k, _)| k.clone()).collect(),
            other => panic!("not an object: {other:?}"),
        };
        keys.sort();
        keys
    }

    fn is_null(&self) -> bool {
        matches!(self, Json::Null)
    }

    fn bool_of(&self) -> bool {
        match self {
            Json::Bool(b) => *b,
            other => panic!("not a bool: {other:?}"),
        }
    }

    fn uint_of(&self) -> u64 {
        match self {
            Json::Num(n) => n.parse().unwrap_or_else(|_| panic!("not a non-negative int: {n}")),
            other => panic!("not a number: {other:?}"),
        }
    }

    fn str_of(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("not a string: {other:?}"),
        }
    }
}

fn parse_json(text: &str) -> Json {
    let bytes = text.as_bytes();
    let mut pos = 0usize;
    let value = parse_value(bytes, &mut pos);
    skip_ws(bytes, &mut pos);
    assert_eq!(pos, bytes.len(), "trailing bytes after the JSON value: {text}");
    value
}

fn skip_ws(b: &[u8], pos: &mut usize) {
    while *pos < b.len() && matches!(b[*pos], b' ' | b'\t' | b'\n' | b'\r') {
        *pos += 1;
    }
}

fn parse_value(b: &[u8], pos: &mut usize) -> Json {
    skip_ws(b, pos);
    match b.get(*pos) {
        Some(b'{') => parse_object(b, pos),
        Some(b'[') => parse_array(b, pos),
        Some(b'"') => Json::Str(parse_string(b, pos)),
        Some(b't') => {
            expect_literal(b, pos, b"true");
            Json::Bool(true)
        }
        Some(b'f') => {
            expect_literal(b, pos, b"false");
            Json::Bool(false)
        }
        Some(b'n') => {
            expect_literal(b, pos, b"null");
            Json::Null
        }
        Some(c) if c.is_ascii_digit() => Json::Num(parse_number(b, pos)),
        other => panic!("unexpected token {other:?} at byte {pos:?}"),
    }
}

fn expect_literal(b: &[u8], pos: &mut usize, literal: &[u8]) {
    assert_eq!(
        b.get(*pos..*pos + literal.len()),
        Some(literal),
        "expected literal {:?} at byte {:?}",
        core::str::from_utf8(literal).unwrap_or("?"),
        pos
    );
    *pos += literal.len();
}

fn parse_object(b: &[u8], pos: &mut usize) -> Json {
    assert_eq!(b.get(*pos), Some(&b'{'));
    *pos += 1;
    let mut fields = Vec::new();
    skip_ws(b, pos);
    if b.get(*pos) == Some(&b'}') {
        *pos += 1;
        return Json::Obj(fields);
    }
    loop {
        skip_ws(b, pos);
        assert_eq!(b.get(*pos), Some(&b'"'), "object key must be a string at byte {pos:?}");
        let key = parse_string(b, pos);
        skip_ws(b, pos);
        assert_eq!(b.get(*pos), Some(&b':'), "expected ':' at byte {pos:?}");
        *pos += 1;
        let value = parse_value(b, pos);
        fields.push((key, value));
        skip_ws(b, pos);
        match b.get(*pos) {
            Some(b',') => *pos += 1,
            Some(b'}') => {
                *pos += 1;
                return Json::Obj(fields);
            }
            other => panic!("expected ',' or '}}' at byte {pos:?}, got {other:?}"),
        }
    }
}

fn parse_array(b: &[u8], pos: &mut usize) -> Json {
    assert_eq!(b.get(*pos), Some(&b'['));
    *pos += 1;
    let mut items = Vec::new();
    skip_ws(b, pos);
    if b.get(*pos) == Some(&b']') {
        *pos += 1;
        return Json::Arr(items);
    }
    loop {
        items.push(parse_value(b, pos));
        skip_ws(b, pos);
        match b.get(*pos) {
            Some(b',') => *pos += 1,
            Some(b']') => {
                *pos += 1;
                return Json::Arr(items);
            }
            other => panic!("expected ',' or ']' at byte {pos:?}, got {other:?}"),
        }
    }
}

fn parse_number(b: &[u8], pos: &mut usize) -> String {
    let start = *pos;
    while *pos < b.len() && b[*pos].is_ascii_digit() {
        *pos += 1;
    }
    assert!(*pos > start, "empty number at byte {start}");
    // strict: no fraction/exponent/negative — the documents carry integers
    // only, and the crate's own reader rejects negative numbers.
    if let Some(c) = b.get(*pos) {
        assert!(
            matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r'),
            "unexpected byte {:?} after a number at byte {}",
            *c as char,
            pos
        );
    }
    core::str::from_utf8(&b[start..*pos]).expect("ascii digits").to_string()
}

fn parse_string(b: &[u8], pos: &mut usize) -> String {
    assert_eq!(b.get(*pos), Some(&b'"'), "expected '\"' at byte {pos:?}");
    *pos += 1;
    let mut out = String::new();
    loop {
        let c = *b.get(*pos).unwrap_or_else(|| panic!("unterminated string at byte {pos:?}"));
        *pos += 1;
        match c {
            b'"' => return out,
            b'\\' => {
                let esc = *b.get(*pos).unwrap_or_else(|| panic!("unterminated escape at byte {pos:?}"));
                *pos += 1;
                match esc {
                    b'"' => out.push('"'),
                    b'\\' => out.push('\\'),
                    b'/' => out.push('/'),
                    b'b' => out.push('\u{0008}'),
                    b'f' => out.push('\u{000c}'),
                    b'n' => out.push('\n'),
                    b'r' => out.push('\r'),
                    b't' => out.push('\t'),
                    other => panic!("unsupported escape \\{} at byte {pos:?}", other as char),
                }
            }
            control if control < 0x20 => panic!("raw control byte in a string at byte {pos:?}"),
            ascii if ascii.is_ascii() => out.push(ascii as char),
            _ => {
                // multi-byte UTF-8: copy the whole sequence
                let start = *pos - 1;
                let mut end = *pos;
                while end < b.len() && (b[end] & 0xc0) == 0x80 {
                    end += 1;
                }
                out.push_str(core::str::from_utf8(&b[start..end]).expect("valid utf-8 in a string"));
                *pos = end;
            }
        }
    }
}
