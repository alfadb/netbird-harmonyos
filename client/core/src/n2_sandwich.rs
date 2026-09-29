// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! N2 前置项 **P5** — I/Ws 的 App 侧夹心编排 (preregistration v4.1 §4
//! 「A 列生产事件的操作性定义」I 行 / Ws 行以及 §3 第 2 条的逐事件夹心,
//! `docs/n2-device-evidence-preregistration-draft-v2-20260928.md:108-109`).
//!
//! ## What the frozen text requires
//!
//! * The **pre** control's earliest legal moment is AFTER the TUN tee attach
//!   anchor (`N2_TUN_GAP|reason=before-tun-attach`, `wg_device.rs:2269-2271`,
//!   or `N7_WG_FEED|adopted`, `:2303`). [`attach_anchor`] is called at exactly
//!   that point.
//! * `I`: the pre control precedes the FIRST ICE pool feed / gather (the shell
//!   inserts it in `settleCreate` between the WG data-plane feed and
//!   `protectAndFeedIceSocket()`); the post control follows the first real I
//!   send (`detail=stun_txn_<hex>`).
//! * `Ws`: the pre control precedes the Ws selection; the post control follows
//!   the first real Ws `wg_sendto`. The implementation NEVER changes the ICE/WG
//!   send content or order — the control markers are inserted at wait points
//!   AFTER the attach, and the only concession the shell makes is a bounded
//!   wait for the pre-control HIT before the first send ([`PRE_WAIT_MS`]).
//!   `Ws` keeps the frozen `peer_conn.rs:1230-1239` order: attach the egress
//!   socket FIRST, land the endpoint only after (`wg_device::set_egress_socket`
//!   → `apply_endpoint`); nothing here defers or reorders that.
//! * If the first I/Ws send already happened BEFORE the attach anchor, the cell
//!   is `unsat` with the frozen token `reason=first-send-before-attach`
//!   — reported whether or not a leg was ever requested.
//!
//! ## Line
//!
//! `N2_SANDWICH|kind=I|Ws|phase=pre|post|first-send|marker=…|event_ref=…|result=…|reason=…|fd=…|dup_fd=…|remote=…|wait_ms=…|mono_ms=…|diag=true|is_evidence=false`
//!
//! `marker` is the marker of THAT phase: the pre control's own id on a
//! `phase=pre`/`phase=first-send` line and the POST control's own (different)
//! id on a `phase=post` line — N2 T7a item 1, so the shell's post control can
//! never be refused as a reused id. `event_ref` is the observation the phase is
//! anchored to (`attach@<mono_ms>` for a pre line, `first-send@<mono_ms>` /
//! `pre@<mono_ms>` / `none` for a post line).
//!
//! ## Gating
//!
//! Inert until the N2 evidence ticket arms the socket trace: unarmed, every
//! entry point is a SILENT no-op — no state, no line, no control (the shell's
//! own `n2Evidence !== undefined` guard is the outer gate, this is the inner
//! one). [`reset`] runs on every arming change.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::os::fd::RawFd;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use crate::hilog;
use crate::sys;

/// The two rows this orchestration covers (the frozen A-column sandwich for
/// Wb is the exempted row and produces no event).
pub const LEG_KINDS: [&str; 2] = ["I", "Ws"];

/// The documented upper bound of the shell's wait for the PRE control HIT
/// before it lets the first production send go (the frozen "或在首发前短暂
/// 等待前控完成，等待上限写明"; the core's control window is the same 3 s).
pub const PRE_WAIT_MS: u64 = 3_000;

/// The documented upper bound of the wait for the FIRST production send after
/// the pre control (the post control cannot exist before it). Same order of
/// magnitude as the frozen R wait.
pub const POST_WAIT_MS: u64 = 90_000;

/// One first production send of a leg, as the armed trace observed it.
#[derive(Clone, Debug)]
struct FirstSend {
    ms: u64,
    fd: RawFd,
    dup_fd: RawFd,
    remote: String,
}

#[derive(Clone, Debug)]
struct Leg {
    /// The PRE control's marker (the leg's registration key: the pre control
    /// HIT is matched on this one).
    marker: String,
    /// N2 T7a (item 1): the POST control's marker — a DIFFERENT id (the
    /// control registry refuses a reused one), handed in by the shell at
    /// registration and handed back by [`poll`] as the marker to send. Every
    /// `phase=post` line carries THIS marker, never the pre one.
    post_marker: String,
    pre_ms: u64,
    pre_hit: bool,
    pre_hit_ms: u64,
    /// The post control has already been handed to the shell (one post per leg).
    post_issued: bool,
}

#[derive(Default)]
struct State {
    attach_ms: Option<u64>,
    first: HashMap<&'static str, FirstSend>,
    legs: HashMap<&'static str, Leg>,
    last_line: Option<String>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> MutexGuard<'static, State> {
    STATE.get_or_init(|| Mutex::new(State::default())).lock().unwrap_or_else(PoisonError::into_inner)
}

/// Drop everything. Called on every arming change.
pub fn reset() {
    *state() = State::default();
}

fn static_kind(kind: &str) -> Option<&'static str> {
    LEG_KINDS.iter().copied().find(|known| *known == kind)
}

fn valid_marker(marker: &str) -> bool {
    !marker.is_empty()
        && marker.len() <= 64
        && marker.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_' | '@') { c } else { '_' })
        .collect()
}

/// One emitted line plus its parsed fields. An UNARMED call answers
/// `rejected|not-armed` with an EMPTY `line` and emits nothing.
pub struct Outcome {
    pub result: &'static str,
    pub reason: String,
    pub kind: String,
    pub marker: String,
    pub line: String,
}

fn silent(result: &'static str, reason: &str, kind: &str, marker: &str) -> Outcome {
    Outcome { result, reason: reason.to_string(), kind: kind.to_string(),
        marker: marker.to_string(), line: String::new() }
}

/// One post control the shell must send: the leg's kind and the marker the
/// core expects to see HIT.
///
/// N2 T7a (item 1): this is the POST control's OWN marker (a fresh
/// `[A-Za-z0-9_-]{1,64}` id the shell derived for the post leg and handed in
/// at [`pre`] time) — never the pre leg's marker. The control registry refuses
/// a reused id (`duplicate-control`), so reusing the pre marker here would make
/// every post control structurally unreachable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Post {
    pub kind: String,
    pub marker: String,
}

/// The exact `N2_SANDWICH` line (N2 T7a item 1 — `marker` is the marker of
/// THAT phase: the pre id on `phase=pre`, the post id on `phase=post`).
/// `phase`/`result`/`reason` are fixed internal tokens and `event_ref` is built
/// internally (`attach@…` / `first-send@…` / `pre@…`), so the documented literal
/// line is preserved; the operator-supplied marker is cleaned.
pub fn format_line(kind: &str, phase: &str, marker: &str, event_ref: &str, result: &str,
    reason: &str, fd: RawFd, dup_fd: RawFd, remote: &str, wait_ms: u64,
    mono_ms: u64) -> String {
    format!(
        "N2_SANDWICH|kind={}|phase={}|marker={}|event_ref={}|result={}|reason={}|fd={fd}|dup_fd={dup_fd}|remote={}|wait_ms={wait_ms}|mono_ms={mono_ms}|diag=true|is_evidence=false",
        clean(kind), clean(phase), clean(marker), event_ref, result, clean(reason),
        clean(remote))
}

fn record(kind: &str, phase: &str, marker: &str, event_ref: &str, result: &'static str,
    reason: &str, fd: RawFd, dup_fd: RawFd, remote: &str, wait_ms: u64) -> Outcome {
    let line = format_line(kind, phase, marker, event_ref, result, reason, fd, dup_fd, remote,
        wait_ms, sys::mono_ms());
    hilog::emit(&line);
    state().last_line = Some(line.clone());
    Outcome { result, reason: reason.to_string(), kind: kind.to_string(),
        marker: marker.to_string(), line }
}

/// The TUN tee attach anchor. The FIRST attach of the session fixes it (the
/// frozen common anchor); a later rebuild keeps the original anchor.
pub fn attach_anchor(now: u64) {
    if !crate::n2_socket_trace::armed() {
        return;
    }
    let mut state = state();
    if state.attach_ms.is_none() {
        state.attach_ms = Some(now);
    }
}

/// One completed outer send as `n2_socket_trace::first_io` reports it
/// (`dup_fd` = the core-owned descriptor, `fd` = the shell original). Only the
/// FIRST successful I/Ws send of the session is recorded.
pub fn observe_io(kind: &str, dup_fd: RawFd, fd: RawFd, remote: SocketAddr, status: &str) {
    if !crate::n2_socket_trace::armed() || status == "failed" {
        return;
    }
    let Some(key) = static_kind(kind) else { return };
    let now = sys::mono_ms();
    let attach_ms = {
        let mut state = state();
        if state.first.contains_key(key) {
            return;
        }
        state.first.insert(key, FirstSend { ms: now, fd, dup_fd, remote: remote.to_string() });
        state.attach_ms
    };
    let remote_text = remote.to_string();
    match attach_ms {
        None => {
            // The frozen UNSAT token: the first send happened before the tee
            // attach anchor, so no pre control could ever have preceded it.
            record(key, "first-send", "none", &format!("first-send@{now}"), "unsat",
                "first-send-before-attach", fd, dup_fd, &remote_text, POST_WAIT_MS);
        }
        Some(_) => {
            // A leg that was already registered but whose pre control has not
            // HIT (or HIT after this send) cannot be a valid sandwich. The
            // closing line is a `phase=post` line, so it carries the POST
            // marker (N2 T7a item 1).
            let marker = {
                let mut state = state();
                match state.legs.get(key) {
                    Some(leg) if !leg.pre_hit || leg.pre_hit_ms > now => {
                        let marker = leg.post_marker.clone();
                        state.legs.remove(key);
                        Some(marker)
                    }
                    _ => None,
                }
            };
            if let Some(marker) = marker {
                record(key, "post", &marker, &format!("first-send@{now}"), "unsat",
                    "pre-control-not-before-first-send", fd, dup_fd, &remote_text, POST_WAIT_MS);
            }
        }
    }
}

/// Register the PRE leg of one kind: called by the shell BEFORE it sends the
/// pre control. `marker` is the PRE control's own id and `post_marker` the
/// POST control's own (different) id — both are part of the leg, so every later
/// `phase=post` line and the [`poll`] answer carry the post marker while the
/// pre HIT is matched on the pre marker. Refuses honestly (`unsat`) when the
/// frozen ordering cannot hold any more.
pub fn pre(kind: &str, marker: &str, post_marker: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        // Zero behavior while unarmed: no state, no line.
        return silent("rejected", "not-armed", kind, marker);
    }
    let Some(key) = static_kind(kind) else {
        return record(kind, "pre", marker, "none", "rejected", "invalid-kind", -1, -1, "unknown",
            PRE_WAIT_MS);
    };
    if !valid_marker(marker) {
        return record(kind, "pre", marker, "none", "rejected", "invalid-marker", -1, -1,
            "unknown", PRE_WAIT_MS);
    }
    if !valid_marker(post_marker) || post_marker == marker {
        // N2 T7a (item 1): the post leg needs its OWN marker; without one the
        // leg cannot be registered (a reused id would be refused later by the
        // control registry and reported as a miss).
        return record(kind, "pre", marker, "none", "rejected", "invalid-post-marker", -1, -1,
            "unknown", PRE_WAIT_MS);
    }
    // std Mutex is NOT reentrant: every guard is released before `record`
    // (which takes the same lock) is called.
    let leg_present = state().legs.contains_key(key);
    if leg_present {
        return record(kind, "pre", marker, "none", "rejected", "leg-already-registered",
            -1, -1, "unknown", PRE_WAIT_MS);
    }
    let (attach_ms, first) = {
        let state = state();
        (state.attach_ms, state.first.get(key).cloned())
    };
    let Some(attach_ms) = attach_ms else {
        return record(kind, "pre", marker, "none", "unsat", "no-attach-anchor", -1, -1,
            "unknown", PRE_WAIT_MS);
    };
    if let Some(first) = first {
        // The frozen token when the send predates the anchor; otherwise the
        // sandwich simply cannot be built, named as such.
        let reason = if first.ms < attach_ms {
            "first-send-before-attach"
        } else {
            "first-send-before-pre-control"
        };
        return record(kind, "pre", marker, &format!("attach@{attach_ms}"), "unsat", reason,
            first.fd, first.dup_fd, &first.remote, PRE_WAIT_MS);
    }
    let now = sys::mono_ms();
    {
        let mut state = state();
        state.legs.insert(key, Leg { marker: marker.to_string(),
            post_marker: post_marker.to_string(), pre_ms: now, pre_hit: false,
            pre_hit_ms: 0, post_issued: false });
    }
    record(kind, "pre", marker, &format!("attach@{attach_ms}"), "registered", "ok", -1, -1,
        "unknown", PRE_WAIT_MS)
}

/// The armed destination tee consumed a control marker (a `N2_CONTROL_HIT`).
/// Records the pre HIT the sandwich ordering is judged on.
pub fn control_hit(marker: &str, now: u64) {
    if !crate::n2_socket_trace::armed() {
        return;
    }
    let mut state = state();
    for leg in state.legs.values_mut() {
        if leg.marker == marker && !leg.pre_hit {
            leg.pre_hit = true;
            leg.pre_hit_ms = now;
        }
    }
}

/// Which legs are ready for their POST control: the first production send has
/// been observed AND the pre control HIT before it. Each leg is offered at
/// most once ([`post`] closes it). A leg whose first send never arrives within
/// [`POST_WAIT_MS`] is closed here as `result=timeout`.
pub fn poll(now: u64) -> Vec<Post> {
    if !crate::n2_socket_trace::armed() {
        return Vec::new();
    }
    let (ready, timed_out) = {
        let state = state();
        let mut ready = Vec::new();
        let mut timed_out = Vec::new();
        for (key, leg) in state.legs.iter() {
            if leg.post_issued {
                continue;
            }
            match state.first.get(key) {
                Some(first) if leg.pre_hit && leg.pre_hit_ms <= first.ms => {
                    // N2 T7a (item 1): the shell is handed the POST leg's own
                    // marker — the id its post control must use.
                    ready.push(Post { kind: (*key).to_string(), marker: leg.post_marker.clone() });
                }
                Some(_) => {} // the ordering failure is reported by observe_io
                None if now.saturating_sub(leg.pre_ms) >= POST_WAIT_MS => {
                    timed_out.push(((*key).to_string(), leg.post_marker.clone(), leg.pre_ms));
                }
                None => {}
            }
        }
        (ready, timed_out)
    };
    for (kind, marker, pre_ms) in timed_out {
        {
            let mut state = state();
            if let Some(leg) = state.legs.get_mut(kind.as_str()) {
                if leg.marker == marker || leg.post_marker == marker {
                    leg.post_issued = true;
                }
            }
            state.legs.remove(kind.as_str());
        }
        record(&kind, "post", &marker, &format!("pre@{pre_ms}"), "timeout",
            "first-send-wait-exceeded", -1, -1, "unknown", POST_WAIT_MS);
    }
    for post in &ready {
        if let Some(leg) = state().legs.get_mut(post.kind.as_str()) {
            leg.post_issued = true;
        }
    }
    ready
}

/// Close one leg with the shell's post outcome. `marker` is the POST control's
/// OWN marker (the id the shell sent, from [`poll`]) — N2 T7a item 1: the leg
/// is located by it and the line carries it, never the pre leg's marker.
/// `result` is the shell's verdict of the post control (`observed` = the post
/// marker HIT, `unsat` = it MISSed), `reason` names the exact branch.
pub fn post(kind: &str, marker: &str, result: &str, reason: &str) -> Outcome {
    if !crate::n2_socket_trace::armed() {
        // Zero behavior while unarmed: no state, no line.
        return silent("rejected", "not-armed", kind, marker);
    }
    let Some(key) = static_kind(kind) else {
        return record(kind, "post", marker, "none", "rejected", "invalid-kind", -1, -1,
            "unknown", POST_WAIT_MS);
    };
    if !valid_marker(marker) {
        return record(kind, "post", marker, "none", "rejected", "invalid-marker", -1, -1,
            "unknown", POST_WAIT_MS);
    }
    let leg = state().legs.get(key).cloned();
    if leg.as_ref().filter(|leg| leg.post_marker == marker).is_none() {
        // No such leg: never fabricate an outcome for an event that was not
        // registered by `pre`.
        return record(kind, "post", marker, "none", "rejected", "no-registered-leg", -1, -1,
            "unknown", POST_WAIT_MS);
    }
    state().legs.remove(key);
    let first = state().first.get(key).cloned();
    let (fd, dup_fd, remote, event_ref) = match &first {
        Some(first) => (first.fd, first.dup_fd, first.remote.clone(),
            format!("first-send@{}", first.ms)),
        None => (-1, -1, "unknown".to_string(), "none".to_string()),
    };
    let result: &'static str = match result {
        "observed" => "observed",
        "unsat" => "unsat",
        "timeout" => "timeout",
        _ => "rejected",
    };
    let reason: &'static str = match (result, reason) {
        ("observed", _) => "control-hit-after-first-send",
        ("unsat", "pre-control-miss") => "pre-control-miss",
        ("unsat", "control-miss") => "post-control-miss",
        ("timeout", _) => "first-send-wait-exceeded",
        ("rejected", _) => "invalid-result",
        _ => "control-miss",
    };
    // N2 T7a (item 1): the POST leg's own marker (`marker`), not the pre one.
    record(key, "post", marker, &event_ref, result, reason, fd, dup_fd, &remote, POST_WAIT_MS)
}

// ---------------------------------------------------------------------------
// read-only inspection (host tests + the campaign judge)
// ---------------------------------------------------------------------------

/// The attach anchor of this arming (`None` = not attached yet).
pub fn attach_ms() -> Option<u64> {
    state().attach_ms
}

/// When the first successful send of `kind` was seen (`None` = never).
pub fn first_send_ms(kind: &str) -> Option<u64> {
    static_kind(kind).and_then(|key| state().first.get(key).map(|first| first.ms))
}

/// How many legs are currently registered.
pub fn leg_count() -> usize {
    state().legs.len()
}

/// Whether the leg of `kind` saw its pre control HIT.
pub fn leg_pre_hit(kind: &str) -> bool {
    static_kind(kind).and_then(|key| state().legs.get(key).map(|leg| leg.pre_hit)).unwrap_or(false)
}

/// The last `N2_SANDWICH` line this process emitted.
pub fn last_line() -> Option<String> {
    state().last_line.clone()
}

/// Host-test serialization (the SAME process-global lock the other N2 modules
/// take, so parallel test threads cannot arm/disarm each other).
#[cfg(test)]
pub(crate) fn test_guard() -> MutexGuard<'static, ()> {
    crate::n2_stimulus::test_guard()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reset_all() {
        crate::n2_socket_trace::arm(false);
        reset();
    }

    fn remote() -> SocketAddr {
        "192.0.2.9:3478".parse().unwrap()
    }

    #[test]
    fn unarmed_records_and_emits_nothing() {
        let _guard = test_guard();
        reset_all();
        attach_anchor(1_000);
        observe_io("I", 7, 6, remote(), "sent");
        let pre = pre("I", "n2s-I-pre", "n2s-I-post");
        assert_eq!(pre.result, "rejected");
        assert_eq!(pre.reason, "not-armed");
        assert!(pre.line.is_empty(), "unarmed emits no line");
        let post = post("I", "n2s-I-post", "observed", "ok");
        assert_eq!(post.result, "rejected");
        assert_eq!(post.reason, "not-armed");
        assert!(post.line.is_empty());
        assert!(poll(10_000_000).is_empty());
        assert_eq!(attach_ms(), None);
        assert_eq!(first_send_ms("I"), None);
        assert_eq!(leg_count(), 0);
        assert!(last_line().is_none(), "zero output while unarmed");
        reset_all();
    }

    #[test]
    fn pre_before_first_send_after_attach_is_a_valid_sandwich() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        let pre = pre("I", "n2s-I-pre", "n2s-I-post");
        assert_eq!(pre.result, "registered");
        assert_eq!(pre.reason, "ok");
        assert!(pre.line.contains("|phase=pre|marker=n2s-I-pre|event_ref=attach@1000|"));
        assert!(pre.line.contains("|wait_ms=3000|"), "{}", pre.line);
        assert!(poll(crate::sys::mono_ms()).is_empty(), "no send yet");
        // the pre control HITs, THEN the first I send happens
        control_hit("n2s-I-pre", 1_100);
        assert!(leg_pre_hit("I"));
        observe_io("I", 7, 6, remote(), "sent");
        assert!(first_send_ms("I").is_some());
        let posts = poll(crate::sys::mono_ms());
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].kind, "I");
        // N2 T7a (item 1): the shell is handed the POST leg's own marker.
        assert_eq!(posts[0].marker, "n2s-I-post");
        assert!(poll(crate::sys::mono_ms()).is_empty(), "offered once");
        let post = post("I", "n2s-I-post", "observed", "ok");
        assert_eq!(post.result, "observed");
        assert_eq!(post.reason, "control-hit-after-first-send");
        assert!(post.line.contains("|phase=post|marker=n2s-I-post|event_ref=first-send@"));
        assert!(post.line.contains("|result=observed|reason=control-hit-after-first-send|"));
        assert!(post.line.contains("|fd=6|dup_fd=7|remote=192.0.2.9:3478|"), "{}", post.line);
        assert_eq!(leg_count(), 0);
        reset_all();
    }

    #[test]
    fn first_send_before_attach_is_unsat_even_without_a_leg() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        observe_io("Ws", 9, 8, "192.0.2.9:51820".parse().unwrap(), "sent");
        let line = last_line().expect("unsat line");
        assert!(line.contains("|kind=Ws|phase=first-send|marker=none|"), "{line}");
        assert!(line.contains("|result=unsat|reason=first-send-before-attach|"), "{line}");
        assert!(line.contains("|fd=8|dup_fd=9|"), "{line}");
        // attaching later does not retroactively legalise it: the anchor is
        // after the send, so the frozen token still applies
        let anchor = crate::sys::mono_ms().saturating_add(1_000);
        attach_anchor(anchor);
        assert_eq!(attach_ms(), Some(anchor));
        let late = pre("Ws", "n2s-Ws-pre", "n2s-Ws-post");
        assert_eq!(late.result, "unsat");
        assert_eq!(late.reason, "first-send-before-attach");
        reset_all();
    }

    #[test]
    fn first_send_after_attach_but_before_the_pre_control_is_unsat() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        observe_io("I", 7, 6, remote(), "sent");
        let pre = pre("I", "n2s-I-pre", "n2s-I-post");
        assert_eq!(pre.result, "unsat", "{}", pre.line);
        assert_eq!(pre.reason, "first-send-before-pre-control");
        reset_all();
    }

    #[test]
    fn a_registered_leg_whose_pre_never_hit_before_the_send_is_unsat() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        assert_eq!(pre("Ws", "n2s-Ws-pre", "n2s-Ws-post").result, "registered");
        observe_io("Ws", 9, 8, remote(), "sent");
        let line = last_line().expect("unsat line");
        assert!(line.contains("|phase=post|marker=n2s-Ws-post|event_ref=first-send@"), "{line}");
        assert!(line.contains("|result=unsat|reason=pre-control-not-before-first-send|"), "{line}");
        assert_eq!(leg_count(), 0);
        reset_all();
    }

    #[test]
    fn a_pre_miss_is_named_and_closes_the_leg() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        assert_eq!(pre("I", "n2s-I-pre", "n2s-I-post").result, "registered");
        let post = post("I", "n2s-I-post", "unsat", "pre-control-miss");
        assert_eq!(post.result, "unsat");
        assert_eq!(post.reason, "pre-control-miss");
        assert_eq!(leg_count(), 0);
        // the send arriving later no longer has a leg to break
        observe_io("I", 7, 6, remote(), "sent");
        assert!(first_send_ms("I").is_some(),
            "the first send is still recorded, just without a sandwich");
        reset_all();
    }

    #[test]
    fn a_leg_without_a_first_send_times_out_at_the_documented_bound() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        assert_eq!(pre("I", "n2s-I-pre", "n2s-I-post").result, "registered");
        let base = crate::sys::mono_ms();
        assert!(poll(base.saturating_add(POST_WAIT_MS).saturating_sub(1)).is_empty());
        let ready = poll(base.saturating_add(POST_WAIT_MS).saturating_add(5_000));
        assert!(ready.is_empty(), "no post control without a production send");
        let line = last_line().expect("timeout line");
        assert!(line.contains("|phase=post|marker=n2s-I-post|event_ref=pre@"), "{line}");
        assert!(line.contains("|result=timeout|reason=first-send-wait-exceeded|"), "{line}");
        assert!(line.contains(&format!("|wait_ms={POST_WAIT_MS}|")), "{line}");
        assert_eq!(leg_count(), 0);
        reset_all();
    }

    #[test]
    fn refused_legs_are_named_and_one_leg_per_kind() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        // no attach anchor yet
        let no_attach = pre("I", "n2s-I-pre", "n2s-I-post");
        assert_eq!(no_attach.result, "unsat");
        assert_eq!(no_attach.reason, "no-attach-anchor");
        attach_anchor(1_000);
        assert_eq!(pre("I", "n2s-I-pre", "n2s-I-post").result, "registered");
        assert_eq!(pre("I", "n2s-I-pre2", "n2s-I-post2").reason, "leg-already-registered");
        assert_eq!(pre("Wb", "n2s-Wb-pre", "n2s-Wb-post").reason, "invalid-kind");
        assert_eq!(pre("I", "bad marker", "n2s-I-post").reason, "invalid-marker");
        // N2 T7a (item 1): a leg without its own (different, valid) post
        // marker is refused by name instead of registering a post control
        // that the control registry would later refuse as a reused id.
        assert_eq!(pre("Ws", "n2s-Ws-pre", "n2s-Ws-pre").reason, "invalid-post-marker");
        assert_eq!(pre("Ws", "n2s-Ws-pre", "").reason, "invalid-post-marker");
        assert_eq!(post("Wb", "n2s-Wb-pre", "observed", "ok").reason, "invalid-kind");
        assert_eq!(post("I", "n2s-I-pre", "observed", "ok").reason, "no-registered-leg");
        reset_all();
    }

    #[test]
    fn failed_sends_never_record_a_first_send() {
        let _guard = test_guard();
        reset_all();
        crate::n2_socket_trace::arm(true);
        attach_anchor(1_000);
        observe_io("I", 7, 6, remote(), "failed");
        assert_eq!(first_send_ms("I"), None);
        assert!(last_line().is_none());
        reset_all();
    }
}
