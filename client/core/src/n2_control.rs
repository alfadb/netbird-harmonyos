// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Session-wide N2 control markers. Only the armed destination tee owns this state.
use crate::sys;
use std::net::Ipv4Addr;

const WAIT_MS: u64 = 3_000;
const PREFIX: &str = "N2CTRL:";

struct Pending {
    id: String,
    dst: [u8; 4],
    port: u16,
    proto: u8,
    source_port: u16,
    registered_ms: u64,
}

pub struct ControlState {
    targets: Vec<[u8; 4]>,
    pending: Vec<Pending>,
    seen: u64,
    controls: u64,
    production: u64,
    completed: Vec<(String, &'static str)>,
    outer: Vec<([u8; 4], u8, u16)>,
    leaks: u64,
}

impl ControlState {
    pub fn new(targets: Vec<[u8; 4]>) -> Self {
        Self { targets, pending: Vec::new(), seen: 0, controls: 0, production: 0,
            completed: Vec::new(), outer: Vec::new(), leaks: 0 }
    }

    pub fn register(&mut self, id: &str, dst: [u8; 4], port: u16, proto: u8,
                    source_port: u16, now: u64) -> Result<(), &'static str> {
        if !self.targets.contains(&dst) { return Err("target-not-captured"); }
        if port == 0 || !matches!(proto, 6 | 17) || (proto == 6 && source_port == 0) ||
            id.is_empty() || id.len() > 64 ||
            !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return Err("invalid-control");
        }
        if self.completed.iter().any(|(old, _)| old == id) ||
            self.pending.iter().any(|p| p.id == id || (proto == 6 && p.proto == 6 &&
            p.dst == dst && p.port == port && p.source_port == source_port)) {
            return Err("duplicate-control");
        }
        self.pending.push(Pending { id: id.into(), dst, port, proto, source_port, registered_ms: now });
        Ok(())
    }

    pub fn cancel(&mut self, id: &str) {
        self.pending.retain(|p| p.id != id);
    }

    /// Return true only when the frame must be dropped before WG encryption.
    pub fn scan(&mut self, frame: &[u8], now: u64) -> Option<String> {
        self.seen = self.seen.saturating_add(1);
        let packet = crate::n2h::parse_ipv4_frame(frame)?;
        let hit = self.pending.iter().position(|p| p.dst == packet.dst && p.port == packet.dport &&
            p.proto == packet.proto && now >= p.registered_ms &&
            now - p.registered_ms <= WAIT_MS &&
            if p.proto == 17 { packet.payload == format!("{PREFIX}{}", p.id).as_bytes() }
            else { packet.sport != 0 && packet.sport == p.source_port && tcp_syn(frame) });
        if let Some(index) = hit {
            let p = self.pending.remove(index);
            self.complete(p.id.clone(), "hit");
            self.controls = self.controls.saturating_add(1);
            // N2 T7a (item 2): `id` is the request id the operator asked for
            // (`N2_CONTROL_SHELL|id=…`, `n2_control_send`'s `id`); `marker` is
            // the marker this control registry matched. The registry keys a
            // control by exactly the request id, so the two are ALWAYS the same
            // value here — both fields are emitted so a judge can look the
            // request up by id without knowing the marker convention.
            return Some(hit_line(&p.id, p.dst, p.port, p.proto, now));
        }
        if self.targets.contains(&packet.dst) { self.production = self.production.saturating_add(1); }
        if self.outer.contains(&(packet.dst, packet.proto, packet.dport)) {
            self.leaks = self.leaks.saturating_add(1);
            crate::hilog::emit(&leak_line(packet.dst, packet.dport, packet.proto, self.leaks, now));
        }
        None
    }

    pub fn due(&mut self, now: u64) -> Vec<String> {
        let mut lines = Vec::new();
        let mut expired = Vec::new();
        self.pending.retain(|p| {
            if now.saturating_sub(p.registered_ms) <= WAIT_MS { return true; }
            lines.push(miss_line(&p.id, p.dst, p.port, p.proto, now));
            expired.push(p.id.clone());
            false
        });
        for id in expired { self.complete(id, "miss"); }
        lines
    }

    fn complete(&mut self, id: String, status: &'static str) {
        if self.completed.len() == 256 { self.completed.remove(0); }
        self.completed.push((id, status));
    }

    pub fn status(&self, id: &str) -> &'static str {
        if self.pending.iter().any(|p| p.id == id) { return "pending"; }
        self.completed.iter().find(|(old, _)| old == id).map_or("unknown", |(_, status)| status)
    }

    pub fn set_outer(&mut self, outer: Vec<([u8; 4], u8, u16)>) { self.outer = outer; }
    pub fn leaks(&self) -> u64 { self.leaks }
    pub fn outer_count(&self) -> usize { self.outer.len() }
    pub fn totals(&self) -> (u64, u64, u64) { (self.seen, self.controls, self.production) }
}

fn tcp_syn(frame: &[u8]) -> bool {
    if frame.len() < 40 || frame[0] >> 4 != 4 { return false; }
    let ihl = (frame[0] & 0x0f) as usize * 4;
    ihl >= 20 && frame.len() >= ihl + 20 && (frame[ihl + 13] & 0x12) == 0x02
}


fn proto_name(proto: u8) -> &'static str { if proto == 6 { "tcp" } else { "udp" } }

/// One `N2_CONTROL_HIT` line (N2 T7a item 2: the request `id` and the matched
/// `marker` are both present; they are the same value by construction).
pub fn hit_line(id: &str, dst: [u8; 4], port: u16, proto: u8, now: u64) -> String {
    format!("N2_CONTROL_HIT|id={id}|marker={id}|dst={}:{port}|proto={}|mono_ms={now}|diag=true|is_evidence=false",
        Ipv4Addr::from(dst), proto_name(proto))
}

/// One `N2_CONTROL_MISS` line (the bounded control window expired).
pub fn miss_line(id: &str, dst: [u8; 4], port: u16, proto: u8, now: u64) -> String {
    format!("N2_CONTROL_MISS|id={id}|marker={id}|dst={}:{port}|proto={}|mono_ms={now}|diag=true|is_evidence=false",
        Ipv4Addr::from(dst), proto_name(proto))
}

/// One `N2_CONTROL_SENT` line (the marker was registered and the send call
/// returned; the HIT/MISS lines stay authoritative for what happened next).
pub fn sent_line(id: &str, destination: &str, proto: &str, source_port: u16, now: u64) -> String {
    format!("N2_CONTROL_SENT|id={id}|marker={id}|dst={destination}|proto={proto}|source_port={source_port}|mono_ms={now}|diag=true|is_evidence=false")
}

/// One `N2_TUN_LEAK|verdict=fail` line: a NON-control from-OS frame matched a
/// frozen outer destination tuple.
pub fn leak_line(dst: [u8; 4], dport: u16, proto: u8, leaks: u64, now: u64) -> String {
    format!("N2_TUN_LEAK|verdict=fail|dst={}:{dport}|proto={}|mono_ms={now}|leaks={leaks}|diag=true|is_evidence=false",
        Ipv4Addr::from(dst), proto_name(proto))
}

/// The marker of an `N2_CONTROL_HIT` line (N2 P5: the sandwich ordering is
/// judged on when the PRE control really HIT, not on when the shell sent it).
pub fn hit_marker(line: &str) -> Option<&str> {
    if !line.starts_with("N2_CONTROL_HIT|") { return None; }
    line.split("|marker=").nth(1)?.split('|').next()
}

fn local_port(fd: i32) -> Result<u16, &'static str> {
    let mut sa = sys::sockaddr_in::new([0; 4], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut sa, &mut len) } != 0 { return Err("socket-local-failed"); }
    Ok(u16::from_be(sa.sin_port))
}

/// The caller opens fd with mgmtSocketOpen/iceSocketOpen and closes it after return.
/// Register before the first send/connect; no protect or network bind occurs here.
/// `control` must be the live device's tee, not a collector-window tee.
pub fn send(control: &crate::wg_device::N2DestinationTee, fd: i32, destination: &str,
            proto_name_arg: &str, id: &str) -> Result<String, &'static str> {
    let (ip, port_text) = destination.rsplit_once(':').ok_or("invalid-target")?;
    let dst: [u8; 4] = ip.parse::<Ipv4Addr>().map_err(|_| "invalid-target")?.octets();
    let port = port_text.parse::<u16>().map_err(|_| "invalid-target")?;
    let proto = match proto_name_arg { "udp" => 17, "tcp" => 6, _ => return Err("invalid-protocol") };
    if fd < 0 { return Err("invalid-fd"); }
    let sa = sys::sockaddr_in::new(dst, port);
    let source_port = if proto == 6 { local_port(fd)? } else { 0 };
    let now = sys::mono_ms();
    control.register_control(id, dst, port, proto, source_port, now)?;
    let rc = if proto == 17 {
        let payload = format!("{PREFIX}{id}");
        unsafe { sys::sendto(fd, payload.as_ptr().cast(), payload.len(), 0,
            &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) }
    } else {
        // A blocking connect could hold the shell thread indefinitely; this
        // fd is temporarily nonblocking and is closed by the shell immediately.
        let flags = unsafe { sys::fcntl(fd, sys::F_GETFL) };
        if flags < 0 { control.cancel_control(id); return Err("socket-flags-failed"); }
        if unsafe { sys::fcntl(fd, sys::F_SETFL, flags | sys::O_NONBLOCK) } < 0 {
            control.cancel_control(id); return Err("socket-flags-failed");
        }
        unsafe { connect(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) as isize }
    };
    // UDP must have queued the whole marker; nonblocking TCP EINPROGRESS
    // means the SYN was submitted, not that a connection was established.
    if (proto == 17 && rc != (PREFIX.len() + id.len()) as isize) ||
       (proto == 6 && rc < 0 && sys::errno() != 115) {
        control.cancel_control(id);
        return Err("control-send-failed");
    }
    Ok(sent_line(id, destination, proto_name_arg,
        if proto == 6 { source_port } else { local_port(fd).unwrap_or(0) }, now))
}

extern "C" {
    fn connect(fd: i32, addr: *const sys::sockaddr_in, len: u32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame(dst: [u8; 4], proto: u8, sport: u16, port: u16, payload: &[u8]) -> Vec<u8> {
        let l4 = if proto == 17 { 8 } else { 20 };
        let mut out = vec![0; 20 + l4 + payload.len()];
        out[0] = 0x45;
        let len = out.len() as u16;
        out[2..4].copy_from_slice(&len.to_be_bytes());
        out[9] = proto;
        out[12..16].copy_from_slice(&[10, 0, 0, 1]);
        out[16..20].copy_from_slice(&dst);
        out[20..22].copy_from_slice(&sport.to_be_bytes());
        out[22..24].copy_from_slice(&port.to_be_bytes());
        if proto == 17 { out[24..26].copy_from_slice(&((l4 + payload.len()) as u16).to_be_bytes()); }
        else { out[32] = 0x50; out[33] = 0x02; }
        out[20+l4..].copy_from_slice(payload);
        out
    }

    #[test]
    fn udp_marker_hit_and_production_are_separate() {
        let mut s = ControlState::new(vec![[203, 0, 113, 1]]);
        s.register("pre-1", [203, 0, 113, 1], 53, 17, 0, 100).unwrap();
        let hit = s.scan(&frame([203, 0, 113, 1], 17, 1000, 53, b"N2CTRL:pre-1"), 101).unwrap();
        // N2 T7a (item 2): the request id and the matched marker are both
        // present (identical by construction) so the hit is findable by id.
        assert!(hit.contains("N2_CONTROL_HIT|id=pre-1|marker=pre-1|"), "{hit}");
        assert!(s.scan(&frame([203, 0, 113, 1], 17, 1000, 53, b"production"), 102).is_none());
        assert_eq!(s.totals(), (2, 1, 1));
    }

    #[test]
    fn tcp_port_hit_requires_registered_destination_and_window() {
        let mut s = ControlState::new(vec![[203, 0, 113, 1]]);
        s.register("post-1", [203, 0, 113, 1], 443, 6, 40001, 100).unwrap();
        assert!(s.scan(&frame([203, 0, 113, 1], 6, 40002, 443, b""), 101).is_none());
        assert!(s.scan(&frame([203, 0, 113, 1], 6, 40001, 443, b""), 102).unwrap().contains("marker=post-1"));
        assert_eq!(s.totals(), (2, 1, 1));
    }

    #[test]
    fn outer_tuple_leak_ignores_source_but_not_port_and_control_never_leaks() {
        let dst = [203, 0, 113, 1];
        let mut s = ControlState::new(vec![dst]);
        s.set_outer(vec![(dst, 17, 53)]);
        s.register("pre", dst, 53, 17, 0, 100).unwrap();
        assert!(s.scan(&frame(dst, 17, 4444, 53, b"N2CTRL:pre"), 101).is_some());
        assert_eq!(s.leaks(), 0);
        assert!(s.scan(&frame(dst, 17, 65000, 54, b"other port"), 102).is_none());
        assert_eq!(s.leaks(), 0);
        assert!(s.scan(&frame(dst, 17, 65000, 53, b"production"), 103).is_none());
        assert_eq!(s.leaks(), 1);
    }

    #[test]
    fn expired_marker_misses_and_uncaptured_target_refused() {
        let mut s = ControlState::new(vec![[203, 0, 113, 1]]);
        assert_eq!(s.register("x", [203, 0, 113, 2], 80, 6, 1234, 0), Err("target-not-captured"));
        s.register("x", [203, 0, 113, 1], 80, 6, 1234, 0).unwrap();
        let miss = s.due(3001);
        assert_eq!(miss.len(), 1);
        assert!(miss[0].contains("N2_CONTROL_MISS|id=x|marker=x|"), "{}", miss[0]);
        assert!(s.scan(&frame([203, 0, 113, 1], 6, 1234, 80, b""), 3002).is_none());
    }

    // N2 P5: the HIT line carries the marker the sandwich ordering is judged
    // on; a non-HIT line (or a malformed one) yields no marker.
    #[test]
    fn hit_marker_is_read_back_from_the_hit_line_only() {
        let mut s = ControlState::new(vec![[203, 0, 113, 1]]);
        s.register("n2s-I-pre", [203, 0, 113, 1], 53, 17, 0, 100).unwrap();
        let line = s.scan(&frame([203, 0, 113, 1], 17, 1000, 53, b"N2CTRL:n2s-I-pre"), 101)
            .expect("hit");
        assert_eq!(hit_marker(&line), Some("n2s-I-pre"));
        assert_eq!(hit_marker("N2_CONTROL_MISS|marker=x|diag=true"), None);
        assert_eq!(hit_marker("N2_CONTROL_HIT|dst=1.2.3.4:53"), None);
    }
}
