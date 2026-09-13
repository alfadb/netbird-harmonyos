// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # wg_device — 多 peer WireGuard 数据面设备（N6）
//!
//! 把 N1BDISC 探针级隧道（[`crate::wg`]，真机验证过的 BoringTun ffi 调用
//! 序列）扩展成一个**设备抽象**：一个本机 WG UDP socket（受保护路径，fd
//! 合同同 [`crate::tun`]：native 只持有 dup 副本，原始 fd 永不由本层关闭）
//! + 一组按公钥增删改的 peer（endpoint / allowed_ips / keepalive / 可选
//! preshared key）+ 出向路由 + 入向解封装。握手/加解密不重写——每个 peer
//! 一个 [`crate::wg::Tunnel`]（`crate::wg::Tunnel::new_with` 只是给已验证
//! 的 `new_tunnel` 调用补了参数化形态）。
//!
//! ## 数据面规则
//!
//! - **出向**：TUN 读到的 IP 包按各 peer `allowed_ips` 做**最长前缀匹配**
//!   （IPv4；前缀相同取先登记的 peer，前缀相异取更长前缀）。无匹配 →
//!   丢弃并计 `no_route_drops`（不 panic）。选中 peer 后 BoringTun 封装：
//!   无会话时 `wireguard_write` 自身会**排包 + 产出握手 initiation**
//!   （boringtun-0.7.1 `noise/mod.rs::encapsulate`——排队包在会话建立后由
//!   `decapsulate(空)` 冲出，设备在检测到会话建立时执行该冲刷）；密文经
//!   受保护 socket `sendto` 发往该 peer 的 endpoint。
//! - **入向**：UDP 收包 → **按来源地址匹配 peer**（握手包同规则；来源不
//!   属于任何 peer 的 endpoint → 丢弃并计 `unknown_peer_drops`）→ 送该
//!   peer 的 tunnel 解封装 → 明文写 TUN（dup 副本，[`TunFd`] 合同）。解密
//!   失败（`OP_ERROR`）计 `decrypt_errors` 并丢弃。
//! - **握手**：endpoint 落配（或变更）即发起（`wireguard_force_handshake`，
//!   对应上游 `ConfigureWGEndpoint` 后的握手驱动
//!   `client/internal/peer/conn.go:444-478`）；无会话时的出向数据也会触发
//!   （boringtun 语义，见上）。重传由设备层在**注入时钟**上执行：无会话且
//!   距上次发起 ≥ `hs_retry_ms` → 重新发起（每次新 ephemeral，同
//!   boringtun `update_timers` 的 REKEY_TIMEOUT 重传语义
//!   `noise/timers.rs:13`）；自首次发起 ≥ `hs_deadline_ms` 仍无会话 →
//!   放弃该轮（REKEY_ATTEMPT_TIME，`noise/timers.rs:12`），endpoint 再变
//!   更时重开。
//! - **keepalive**：boringtun 层 keepalive 传 0（禁用其内部真实时钟定
//!   时器），设备层在注入时钟上发送（空载荷 transport 包）：会话建立后
//!   距上次出向活动 ≥ `keepalive_ms` → 发 keepalive。间隔默认 25 s =
//!   上游 NetBird `defaultWgKeepAlive`
//!   （`client/internal/peer/endpoint.go:14`，N1BDISC 探针同值）。
//!
//! ## 时钟
//!
//! 设备层所有定时（keepalive / 握手重传 / 会话过期判定）都走**方法注入的
//! `now_ms`**（单调毫秒，与 [`crate::peer_conn`] 的注入时钟同风格）——测试
//! 无需长 sleep。boringtun 内部定时器（rekey/cookie，`wireguard_tick`）仍
//! 是其自身真实时钟，设备 `tick` 定期调用使其运行；测试里的真实流逝时间为
//! 毫秒级，不会触发它们。
//!
//! ## `tunnel_ready()` 语义（N3-7 安全闸输入）
//!
//! **至少一个 peer 完成了 WG 握手（boringtun stats 的
//! `time_since_last_handshake >= 0`，即会话已装载）且该会话在注入时钟上
//! 未过期**。过期 = 自最近一次观察到新握手起超过 `session_max_ms`
//! （默认 540 s = REJECT_AFTER_TIME × 3，boringtun `noise/timers.rs` 与
//! wireguard.pdf §6.2——此后无新握手即密钥窗全失效）。语义边界：本函数回
//! 答的是「**本端加密会话能否载荷**」，对端存活探测仍由 ICE 层负责
//! （`crate::peer_conn::ice_ready_for_default_route`），两者在 connector
//! 的闸里相与（`ConnectorShared::apply_update`）。
//!
//! ## fd 合同（§二.4）
//!
//! WG UDP socket 只经 [`crate::mgmtsock::dup_socket_fd`] 拿 **dup 副本**
//! （F_DUPFD_CLOEXEC），`Drop`/`reattach_socket` 只关自己的 dup；原始 fd
//! 由壳侧（protect 后）拥有。TUN 侧复用 [`TunFd`]（dup + 唯一 close 点）。

use std::sync::{Mutex, MutexGuard, PoisonError};

use base64::Engine as _;

use crate::connector::{WgPeerApplier, WgPeerEntry};
use crate::hilog::emit;
use crate::sys;
use crate::tun::TunFd;
use crate::wg::{Tunnel, OP_ERROR, OP_NETWORK, OP_TUN_V4, OP_TUN_V6};

/// One WG datagram / one TUN frame working buffer (probe BUF parity; >= MTU
/// 1420 + transport overhead).
pub const WG_BUF: usize = 2048;
/// Persistent-keepalive interval (device clock, ms). Default 25 s =
/// upstream NetBird `defaultWgKeepAlive` (client/internal/peer/endpoint.go:14).
pub const DEFAULT_KEEPALIVE_MS: u64 = 25_000;
/// Handshake re-initiation interval (device clock, ms). Default 5 s =
/// boringtun REKEY_TIMEOUT (noise/timers.rs:13, wireguard.pdf §6).
pub const DEFAULT_HS_RETRY_MS: u64 = 5_000;
/// Handshake campaign deadline (device clock, ms). Default 90 s =
/// boringtun REKEY_ATTEMPT_TIME (noise/timers.rs:12).
pub const DEFAULT_HS_DEADLINE_MS: u64 = 90_000;
/// Session-expiry window (device clock, ms). Default 540 s =
/// REJECT_AFTER_TIME × 3 (boringtun update_timers clears the tunnel there).
pub const DEFAULT_SESSION_MAX_MS: u64 = 540_000;
/// Per-pump drain cap (frames per `service_tun` / datagrams per
/// `service_udp`), so one burst cannot starve the other half of the loop
/// (same shape as the probe's FWD_TUN_DRAIN).
const DRAIN_MAX: usize = 8;
/// TUN write budget for decapsulated plaintext (bounded, slice-polled —
/// `TunFd::write_frame_budget` contract).
const TUN_WRITE_BUDGET_MS: u64 = 200;
/// Decapsulate-empty flush rounds after session establishment (boringtun
/// documented repeat-until-Done; bounded so a wedged state cannot spin).
const FLUSH_MAX: usize = 8;

// ---------------------------------------------------------------------------
// configuration / specs
// ---------------------------------------------------------------------------

/// Device configuration: local identity + device-clock timer policy. All
/// durations live on the INJECTED clock (`now_ms` arguments) — no wall-clock
/// reads, no sleeps (module docs).
#[derive(Debug, Clone)]
pub struct WgDeviceConfig {
    /// Local x25519 secret key, base64 (std). Held verbatim for tunnel
    /// creation; never logged (credential discipline: module emits carry
    /// counters and states only).
    pub local_secret_b64: String,
    pub keepalive_ms: u64,
    pub hs_retry_ms: u64,
    pub hs_deadline_ms: u64,
    pub session_max_ms: u64,
}

impl WgDeviceConfig {
    /// Defaults: keepalive 25 s (endpoint.go:14), hs retry 5 s /
    /// deadline 90 s (boringtun REKEY_TIMEOUT / REKEY_ATTEMPT_TIME),
    /// session max 540 s (REJECT_AFTER_TIME × 3).
    pub fn new(local_secret_b64: impl Into<String>) -> Self {
        WgDeviceConfig {
            local_secret_b64: local_secret_b64.into(),
            keepalive_ms: DEFAULT_KEEPALIVE_MS,
            hs_retry_ms: DEFAULT_HS_RETRY_MS,
            hs_deadline_ms: DEFAULT_HS_DEADLINE_MS,
            session_max_ms: DEFAULT_SESSION_MAX_MS,
        }
    }
}

/// One peer to register: public key (base64 std), IPv4 allowed_ips
/// (address + prefix length; host bits are masked off on registration) and
/// an optional preshared key (base64 std).
#[derive(Debug, Clone)]
pub struct WgPeerSpec {
    pub pub_key_b64: String,
    pub allowed_ips: Vec<([u8; 4], u8)>,
    pub preshared_key_b64: Option<String>,
}

impl WgPeerSpec {
    pub fn new(pub_key_b64: impl Into<String>, allowed_ips: Vec<([u8; 4], u8)>) -> Self {
        WgPeerSpec { pub_key_b64: pub_key_b64.into(), allowed_ips, preshared_key_b64: None }
    }
}

// ---------------------------------------------------------------------------
// stats / status
// ---------------------------------------------------------------------------

/// Device-level counters. Every drop path counts; nothing panics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgDeviceStats {
    /// Encapsulated transport datagrams actually handed to sendto.
    pub tx_packets: u64,
    pub tx_bytes: u64,
    /// UDP datagrams accepted for a known peer (before decapsulation).
    pub rx_packets: u64,
    /// Plaintext bytes written into the TUN.
    pub rx_bytes_to_tun: u64,
    /// Handshake initiations sent (initial + retransmits + rekeys).
    pub handshake_initiations: u64,
    /// Keepalive transport packets sent (empty payload).
    pub keepalives_sent: u64,
    /// Outbound frames dropped: no allowed_ips prefix matched the dst.
    pub no_route_drops: u64,
    /// Inbound datagrams dropped: source address matches no peer endpoint.
    pub unknown_peer_drops: u64,
    /// Inbound datagrams a matched peer failed to process (`OP_ERROR`).
    pub decrypt_errors: u64,
    /// Outbound frames dropped: not parseable IPv4 (short / non-IPv4).
    pub short_frame_drops: u64,
    /// sendto failures (socket gone / endpoint unsendable).
    pub send_errors: u64,
    /// TUN write failures for decapsulated plaintext.
    pub tun_write_errors: u64,
}

/// Read-only per-peer snapshot (public key is PUBLIC material).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgPeerStatus {
    pub pub_key_b64: String,
    pub endpoint: Option<([u8; 4], u16)>,
    pub allowed_ips: Vec<([u8; 4], u8)>,
    pub session_established: bool,
    pub expired: bool,
    /// boringtun `time_since_last_handshake` seconds (-1 = no session yet).
    pub last_handshake_s: i64,
}

/// One `handle_udp` outcome (observation surface for tests/pumps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgInbound {
    /// Source matched a registered peer endpoint.
    pub peer_matched: bool,
    /// Decapsulated plaintext bytes written to the TUN (0 = none).
    pub wrote_tun: usize,
    /// Response datagrams produced and sent (handshake response / flush).
    pub sent: usize,
}

// ---------------------------------------------------------------------------
// peer state
// ---------------------------------------------------------------------------

struct WgPeer {
    key_b64: String,
    /// Stored MASKED (host bits cleared) so the LPM compares masked==masked.
    allowed_ips: Vec<([u8; 4], u8)>,
    tunnel: Tunnel,
    endpoint: Option<([u8; 4], u16)>,
    /// Set the moment boringtun stats reports a completed handshake; see the
    /// module docs for the `tunnel_ready` semantics this feeds.
    session_established: bool,
    session_established_ms: u64,
    expired: bool,
    /// Current handshake campaign: first initiation (deadline anchor) and
    /// last initiation (retry anchor); `None` = no campaign running.
    first_init_ms: Option<u64>,
    last_init_ms: Option<u64>,
    /// Last outbound activity (data or keepalive), keepalive anchor.
    last_outbound_ms: Option<u64>,
}

impl WgPeer {
    fn stats(&self) -> (i64, u64, u64) {
        self.tunnel.stats()
    }
}

// ---------------------------------------------------------------------------
// device
// ---------------------------------------------------------------------------

/// Multi-peer WireGuard device: one protected UDP socket (dup copy) + one
/// [`TunFd`] + per-peer BoringTun tunnels. Single-threaded pump model: the
/// owner calls `service_tun` / `service_udp` / `tick` with an injected
/// `now_ms`; the connector seam ([`WgDeviceApplier`]) wraps a `Mutex` of it.
pub struct WgDevice {
    cfg: WgDeviceConfig,
    /// OUR dup of the (shell-protected) WG UDP socket. The raw fd is never
    /// stored and never closed here (fd contract, module docs).
    fd: Option<i32>,
    /// getsockname at adopt time (observability only).
    local: ([u8; 4], u16),
    tun: TunFd,
    peers: Vec<WgPeer>,
    next_index: u32,
    stats: WgDeviceStats,
}

/// Local index seed for the first peer tunnel of a device (nonzero; each
/// further peer gets the next value — indices must be unique WITHIN a device
/// because inbound datagrams are source-routed and the receiving tunnel
/// validates the receiver index against its own).
const INDEX_BASE: u32 = 0x4E36_0000; // "N6"

impl WgDevice {
    /// Adopt an ALREADY-BOUND (shell-protected / test-bound) UDP socket by
    /// raw fd — dups it (`F_DUPFD_CLOEXEC`), never touches the original
    /// again — plus the TUN half as an owned [`TunFd`] (dup of the platform
    /// fd; `TunFd` carries the fd contract). The caller keeps owning the raw
    /// fds; closing them is the caller's/platform's job.
    pub fn adopt(cfg: WgDeviceConfig, wg_socket_raw: i32, tun: TunFd) -> Result<WgDevice, String> {
        let fd = crate::mgmtsock::dup_socket_fd(wg_socket_raw)
            .map_err(|e| format!("wg-device-socket-bad-fd (errno={})", e.errno()))?;
        let local = match getsockname(fd) {
            Ok(l) => l,
            Err(e) => {
                unsafe { sys::close(fd) };
                return Err(e);
            }
        };
        emit(&format!(
            "N6_WG_DEVICE|adopt|local={}.{}.{}.{}:{}|peers=0",
            local.0[0], local.0[1], local.0[2], local.0[3], local.1
        ));
        Ok(WgDevice {
            cfg,
            fd: Some(fd),
            local,
            tun,
            peers: Vec::new(),
            next_index: INDEX_BASE,
            stats: WgDeviceStats::default(),
        })
    }

    /// Adopt a NEW socket (dup + close of the OLD dup only) keeping peers,
    /// tunnels and sessions — the hook for socket regeneration on the local
    /// side (e.g. the local candidate behind the WG socket was re-selected).
    /// Endpoints of REMOTE peers are unchanged; a remote endpoint change is
    /// `set_endpoint` (the ICE re-selection path).
    pub fn reattach_socket(&mut self, wg_socket_raw: i32) -> Result<(), String> {
        let fd = crate::mgmtsock::dup_socket_fd(wg_socket_raw)
            .map_err(|e| format!("wg-device-socket-bad-fd (errno={})", e.errno()))?;
        let local = getsockname(fd).map_err(|e| {
            unsafe { sys::close(fd) };
            e
        })?;
        if let Some(old) = self.fd.take() {
            unsafe { sys::close(old) };
        }
        self.fd = Some(fd);
        self.local = local;
        emit(&format!(
            "N6_WG_DEVICE|reattach|local={}.{}.{}.{}:{}",
            local.0[0], local.0[1], local.0[2], local.0[3], local.1
        ));
        Ok(())
    }

    /// Our socket's observed local address (adopt/reattach time).
    pub fn local_addr(&self) -> ([u8; 4], u16) {
        self.local
    }

    /// Registered peer set snapshot.
    pub fn peers(&self) -> Vec<WgPeerStatus> {
        self.peers.iter().map(|p| p.status()).collect()
    }

    /// Device counters snapshot.
    pub fn stats(&self) -> WgDeviceStats {
        self.stats
    }

    /// Peer count.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Reconcile the peer set (full-snapshot semantics, same as the network
    /// map): new keys get tunnels, vanished keys are torn down, existing keys
    /// keep tunnel/session/endpoint and only refresh allowed_ips. Validates
    /// everything BEFORE mutating (no partial application).
    pub fn set_peers(&mut self, specs: &[WgPeerSpec]) -> Result<(), String> {
        for s in specs {
            decode_key32(&s.pub_key_b64)?;
            for &(_, plen) in &s.allowed_ips {
                if plen > 32 {
                    return Err(format!("wg-device-allowed-ip-prefix-invalid (/{plen})"));
                }
            }
            if let Some(psk) = s.preshared_key_b64.as_ref() {
                decode_key32(psk)?;
            }
        }
        self.peers.retain(|p| specs.iter().any(|s| s.pub_key_b64 == p.key_b64));
        for s in specs {
            let masked: Vec<([u8; 4], u8)> =
                s.allowed_ips.iter().map(|&(a, pl)| (mask(a, pl), pl)).collect();
            match self.peers.iter_mut().find(|p| p.key_b64 == s.pub_key_b64) {
                Some(p) => p.allowed_ips = masked,
                None => {
                    let index = self.alloc_index()?;
                    let tunnel = Tunnel::new_with(
                        &self.cfg.local_secret_b64,
                        &s.pub_key_b64,
                        s.preshared_key_b64.as_deref(),
                        0, // boringtun keepalive disabled — device clock owns it
                        index,
                        "WGD",
                    )
                    .ok_or_else(|| "wg-device-tunnel-create-failed".to_string())?;
                    self.peers.push(WgPeer {
                        key_b64: s.pub_key_b64.clone(),
                        allowed_ips: masked,
                        tunnel,
                        endpoint: None,
                        session_established: false,
                        session_established_ms: 0,
                        expired: false,
                        first_init_ms: None,
                        last_init_ms: None,
                        last_outbound_ms: None,
                    });
                }
            }
        }
        emit(&format!("N6_WG_DEVICE|peers={}", self.peers.len()));
        Ok(())
    }

    /// Drop the whole peer set (connector stop semantics); sockets/tunnels
    /// owned by the device stay until the device is dropped.
    pub fn clear(&mut self) {
        if !self.peers.is_empty() {
            self.peers.clear();
            emit("N6_WG_DEVICE|cleared");
        }
    }

    /// Configure (or re-configure) one peer's endpoint and — on an actual
    /// change — start a fresh handshake campaign on the new path (the
    /// `ConfigureWGEndpoint` equivalent; an unchanged endpoint is a no-op so
    /// repeated ICE refreshes do not rekey). Unknown peers are REJECTED
    /// (fail-closed, same rule as the registry seam).
    pub fn set_endpoint(
        &mut self,
        pub_key_b64: &str,
        addr: [u8; 4],
        port: u16,
        now_ms: u64,
    ) -> Result<(), String> {
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        };
        let changed = self.peers[idx].endpoint != Some((addr, port));
        self.peers[idx].endpoint = Some((addr, port));
        if changed {
            // new path → new campaign (fresh ephemeral; an existing session
            // keeps carrying traffic while the rekey runs — WG semantics)
            self.initiate_handshake(idx, now_ms);
        }
        Ok(())
    }

    /// Route + encapsulate + send one device-originated frame. Returns true
    /// iff a datagram went out. All drop paths count, none panics.
    pub fn send_from_tun(&mut self, frame: &[u8], now_ms: u64) -> bool {
        let Some(dst) = ipv4_dst(frame) else {
            self.stats.short_frame_drops += 1;
            return false;
        };
        let Some(idx) = self.route(dst) else {
            self.stats.no_route_drops += 1;
            return false;
        };
        self.encap_and_send(idx, frame, now_ms)
    }

    /// Process one inbound UDP datagram from `src`. Source must equal a
    /// registered peer endpoint (handshakes included); everything else is
    /// dropped + counted (`unknown_peer_drops`).
    pub fn handle_udp(&mut self, datagram: &[u8], src: ([u8; 4], u16), now_ms: u64) -> WgInbound {
        let mut out = WgInbound::default();
        let Some(idx) = self.peers.iter().position(|p| p.endpoint == Some(src)) else {
            self.stats.unknown_peer_drops += 1;
            return out;
        };
        out.peer_matched = true;
        self.stats.rx_packets += 1;
        self.process_datagram(idx, datagram, src, &mut out);

        // Session-establishment observation (boringtun stats flips to >= 0
        // the moment a handshake completes) + queued-packet flush.
        let (hs, _, _) = self.peers[idx].stats();
        if hs >= 0 {
            let fresh = hs == 0; // handshake observed within the last second
            if !self.peers[idx].session_established {
                self.peers[idx].session_established = true;
                self.peers[idx].expired = false;
                self.peers[idx].session_established_ms = now_ms;
                self.peers[idx].first_init_ms = None; // campaign done
                emit("N6_WG_DEVICE|session-established");
            } else if fresh {
                // rekey completed: refresh the expiry anchor
                self.peers[idx].session_established_ms = now_ms;
            }
            out.sent += self.flush_queued(idx, now_ms);
        }
        out
    }

    /// Poll + read + `send_from_tun` (device-originated frames). Non-blocking:
    /// poll timeout 0, at most `DRAIN_MAX` frames per call.
    pub fn service_tun(&mut self, now_ms: u64) -> usize {
        let Some(fd) = self.tun.fd() else { return 0 };
        let mut n = 0usize;
        while n < DRAIN_MAX {
            let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 0);
            if ret <= 0 || (rev & sys::POLLIN) == 0 {
                break;
            }
            let mut buf = [0u8; WG_BUF];
            match self.tun.read_frame(&mut buf) {
                Ok(len) => {
                    self.send_from_tun(&buf[..len], now_ms);
                    n += 1;
                }
                Err(_) => break, // Closed/Eof/BadFd: stop draining this round
            }
        }
        n
    }

    /// Poll + recvfrom + `handle_udp`. Non-blocking, at most `DRAIN_MAX`
    /// datagrams per call.
    pub fn service_udp(&mut self, now_ms: u64) -> usize {
        let Some(fd) = self.fd else { return 0 };
        let mut n = 0usize;
        while n < DRAIN_MAX {
            let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 0);
            if ret <= 0 || (rev & sys::POLLIN) == 0 {
                break;
            }
            let mut buf = [0u8; WG_BUF];
            let mut from = sys::sockaddr_in::new([0, 0, 0, 0], 0);
            let mut flen = core::mem::size_of::<sys::sockaddr_in>() as u32;
            let rn = unsafe {
                sys::recvfrom(
                    fd,
                    buf.as_mut_ptr() as *mut core::ffi::c_void,
                    buf.len(),
                    0,
                    &mut from,
                    &mut flen,
                )
            };
            if rn <= 0 {
                break;
            }
            self.handle_udp(&buf[..rn as usize], (from.sin_addr, u16::from_be(from.sin_port)), now_ms);
            n += 1;
        }
        n
    }

    /// One device-clock tick: session-expiry check, handshake retries,
    /// keepalives, plus one boringtun `update_timers` per peer (rekey/cookie
    /// housekeeping on boringtun's own real clock). Returns datagrams sent.
    pub fn tick(&mut self, now_ms: u64) -> usize {
        let mut sent = 0usize;
        let cfg = self.cfg.clone();
        for idx in 0..self.peers.len() {
            // session expiry on the INJECTED clock (module docs: tunnel_ready)
            if self.peers[idx].session_established
                && !self.peers[idx].expired
                && now_ms.saturating_sub(self.peers[idx].session_established_ms)
                    >= cfg.session_max_ms
            {
                self.peers[idx].expired = true;
                emit("N6_WG_DEVICE|session-expired");
            }
            // handshake campaign (endpoint present, no live session)
            if self.peers[idx].endpoint.is_some() && !self.peers[idx].session_established {
                let campaign = self.peers[idx].first_init_ms;
                match campaign {
                    Some(t0) if now_ms.saturating_sub(t0) >= cfg.hs_deadline_ms => {
                        // REKEY_ATTEMPT_TIME exhausted: stop retrying until
                        // the endpoint changes or traffic re-arms a campaign
                        self.peers[idx].first_init_ms = None;
                        self.peers[idx].last_init_ms = None;
                        emit("N6_WG_DEVICE|handshake-campaign-deadline");
                    }
                    Some(_) | None => {
                        let due = match self.peers[idx].last_init_ms {
                            None => true, // armed but never fired: send now
                            Some(t) => now_ms.saturating_sub(t) >= cfg.hs_retry_ms,
                        };
                        if due {
                            sent += usize::from(self.initiate_handshake(idx, now_ms));
                        }
                    }
                }
            }
            // persistent keepalive on the device clock (empty-payload packet)
            if self.peers[idx].session_established
                && !self.peers[idx].expired
                && self
                    .peers[idx]
                    .last_outbound_ms
                    .map_or(true, |t| now_ms.saturating_sub(t) >= cfg.keepalive_ms)
            {
                sent += usize::from(self.send_keepalive(idx, now_ms));
            }
            // boringtun internal timers (rekey/cookie housekeeping on its own
            // real clock); whatever datagram becomes due goes out now
            let mut out = [0u8; WG_BUF];
            let (op, len) = { self.peers[idx].tunnel.tick(&mut out) };
            if op == OP_NETWORK && len > 0 {
                if let Some(ep) = self.peers[idx].endpoint {
                    let datagram = out[..len].to_vec();
                    if self.send_to(&datagram, ep) {
                        sent += 1;
                    }
                }
            }
        }
        sent
    }

    /// `tunnel_ready()`: at least one peer with an ESTABLISHED, NON-EXPIRED
    /// WG session (module docs for the exact semantics + gate linkage).
    pub fn tunnel_ready(&self) -> bool {
        self.peers.iter().any(|p| p.session_established && !p.expired)
    }

    /// Whether the peer's WG session is established and non-expired.
    pub fn peer_ready(&self, pub_key_b64: &str) -> bool {
        self.peers
            .iter()
            .any(|p| p.key_b64 == pub_key_b64 && p.session_established && !p.expired)
    }

    // -- internals ----------------------------------------------------------

    /// Decrypt/process one source-matched datagram: transport data → TUN,
    /// handshake responses/cookies → out (sent back to the source).
    fn process_datagram(&mut self, idx: usize, datagram: &[u8], src: ([u8; 4], u16), out: &mut WgInbound) {
        let mut plain = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.read(datagram, &mut plain) };
        match op {
            OP_TUN_V4 | OP_TUN_V6 => {
                let frame = &plain[..len];
                match self.tun.write_frame_budget(frame, TUN_WRITE_BUDGET_MS) {
                    Ok(_) => {
                        self.stats.rx_bytes_to_tun += len as u64;
                        out.wrote_tun = len;
                    }
                    Err(_) => self.stats.tun_write_errors += 1,
                }
            }
            OP_NETWORK if len > 0 => {
                if self.send_to(&plain[..len], src) {
                    out.sent += 1;
                }
            }
            OP_ERROR => self.stats.decrypt_errors += 1,
            _ => {} // OP_DONE: keepalive/cookie absorbed
        }
    }

    /// LPM over all peers' allowed_ips; longer prefix wins, ties keep the
    /// earlier-registered peer (deterministic).
    fn route(&self, dst: [u8; 4]) -> Option<usize> {
        let mut best: Option<(usize, u8)> = None;
        for (i, p) in self.peers.iter().enumerate() {
            for &(net, plen) in &p.allowed_ips {
                if prefix_match(dst, net, plen)
                    && best.map_or(true, |(_, bp)| plen > bp)
                {
                    best = Some((i, plen));
                }
            }
        }
        best.map(|(i, _)| i)
    }

    /// Encapsulate + send one frame to peer `idx`. With no session,
    /// `wireguard_write` queues the frame AND produces the handshake
    /// initiation — both handled here (init bookkeeping on the campaign).
    fn encap_and_send(&mut self, idx: usize, frame: &[u8], now_ms: u64) -> bool {
        let had_session = self.peers[idx].session_established;
        let mut ct = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.write(frame, &mut ct) };
        if op != OP_NETWORK || len == 0 {
            if op == OP_ERROR {
                // transient encapsulate error (e.g. expired handshake):
                // counted, non-fatal, the pump keeps running
                self.stats.send_errors += 1;
            }
            return false;
        }
        let datagram = ct[..len].to_vec();
        let Some(ep) = self.peers[idx].endpoint else {
            // routed but never given an endpoint (no ICE yet): nothing to
            // send to; counted so the observation is honest
            self.stats.send_errors += 1;
            return false;
        };
        let sent_ok = self.send_to(&datagram, ep);
        if !sent_ok {
            return false;
        }
        self.stats.tx_packets += 1;
        self.stats.tx_bytes += len as u64;
        self.peers[idx].last_outbound_ms = Some(now_ms);
        if !had_session {
            // the produced datagram was (part of) a handshake campaign
            self.note_initiation(idx, now_ms);
        }
        true
    }

    /// Force a handshake initiation to the peer's endpoint (fresh ephemeral
    /// every call — the retransmit/rekey form).
    fn initiate_handshake(&mut self, idx: usize, now_ms: u64) -> bool {
        let mut ct = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.force_handshake(&mut ct) };
        if op != OP_NETWORK || len == 0 {
            return false;
        }
        let datagram = ct[..len].to_vec();
        let Some(ep) = self.peers[idx].endpoint else {
            return false;
        };
        if !self.send_to(&datagram, ep) {
            return false;
        }
        self.stats.tx_packets += 1;
        self.stats.handshake_initiations += 1;
        self.note_initiation(idx, now_ms);
        true
    }

    /// Campaign bookkeeping after an initiation actually went out.
    fn note_initiation(&mut self, idx: usize, now_ms: u64) {
        let p = &mut self.peers[idx];
        if p.first_init_ms.is_none() {
            p.first_init_ms = Some(now_ms);
        }
        p.last_init_ms = Some(now_ms);
    }

    /// Send an empty-payload transport packet (WG persistent keepalive).
    fn send_keepalive(&mut self, idx: usize, now_ms: u64) -> bool {
        let mut ct = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.write(&[], &mut ct) };
        if op != OP_NETWORK || len == 0 {
            return false;
        }
        let datagram = ct[..len].to_vec();
        let Some(ep) = self.peers[idx].endpoint else {
            return false;
        };
        if !self.send_to(&datagram, ep) {
            return false;
        }
        self.stats.tx_packets += 1;
        self.stats.keepalives_sent += 1;
        self.peers[idx].last_outbound_ms = Some(now_ms);
        true
    }

    /// Flush boringtun's queued packets after session establishment
    /// (decapsulate-empty repeat-until-Done, bounded; ffi contract).
    fn flush_queued(&mut self, idx: usize, now_ms: u64) -> usize {
        let mut sent = 0usize;
        for _ in 0..FLUSH_MAX {
            let mut ct = [0u8; WG_BUF];
            let (op, len) = { self.peers[idx].tunnel.read(&[], &mut ct) };
            if op != OP_NETWORK || len == 0 {
                break;
            }
            let datagram = ct[..len].to_vec();
            let Some(ep) = self.peers[idx].endpoint else { break };
            if !self.send_to(&datagram, ep) {
                break;
            }
            self.stats.tx_packets += 1;
            self.peers[idx].last_outbound_ms = Some(now_ms);
            sent += 1;
        }
        sent
    }

    /// sendto on OUR socket dup. Returns success; failures count.
    fn send_to(&mut self, data: &[u8], dst: ([u8; 4], u16)) -> bool {
        let Some(fd) = self.fd else {
            self.stats.send_errors += 1;
            return false;
        };
        let sa = sys::sockaddr_in::new(dst.0, dst.1);
        let n = unsafe {
            sys::sendto(
                fd,
                data.as_ptr() as *const core::ffi::c_void,
                data.len(),
                0,
                &sa,
                core::mem::size_of::<sys::sockaddr_in>() as u32,
            )
        };
        if n < 0 {
            self.stats.send_errors += 1;
            return false;
        }
        true
    }

    fn alloc_index(&mut self) -> Result<u32, String> {
        let idx = self.next_index;
        self.next_index = self
            .next_index
            .checked_add(1)
            .ok_or_else(|| "wg-device-index-exhausted".to_string())?;
        Ok(idx)
    }
}

impl WgPeer {
    fn status(&self) -> WgPeerStatus {
        let (hs, _, _) = self.stats();
        WgPeerStatus {
            pub_key_b64: self.key_b64.clone(),
            endpoint: self.endpoint,
            allowed_ips: self.allowed_ips.clone(),
            session_established: self.session_established,
            expired: self.expired,
            last_handshake_s: hs,
        }
    }
}

impl Drop for WgDevice {
    fn drop(&mut self) {
        // fd contract: ONLY our dup is closed here; the raw socket belongs to
        // the shell/provider. TunFd closes its own dup via its Drop.
        if let Some(fd) = self.fd.take() {
            unsafe { sys::close(fd) };
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// base64(std) → exactly 32 nonzero-validated bytes is NOT required; 32 bytes
/// is (boringtun parses the same form).
fn decode_key32(b64: &str) -> Result<[u8; 32], String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| "wg-device-key-not-base64".to_string())?;
    raw.try_into()
        .map_err(|v: Vec<u8>| format!("wg-device-key-not-32-bytes ({})", v.len()))
}

/// Mask host bits (allowed_ips stored masked; config.rs Route convention).
fn mask(addr: [u8; 4], prefix_len: u8) -> [u8; 4] {
    let v = u32::from_be_bytes(addr);
    let m = prefix_mask(prefix_len);
    (v & m).to_be_bytes()
}

fn prefix_mask(prefix_len: u8) -> u32 {
    if prefix_len == 0 {
        0
    } else {
        u32::MAX << (32 - prefix_len as u32)
    }
}

fn prefix_match(dst: [u8; 4], net_masked: [u8; 4], prefix_len: u8) -> bool {
    let (d, n) = (u32::from_be_bytes(dst), u32::from_be_bytes(net_masked));
    (d & prefix_mask(prefix_len)) == n
}

/// Destination IPv4 of a frame (bytes 16..20; IHL-independent). `None` when
/// the frame is too short or not IPv4.
fn ipv4_dst(frame: &[u8]) -> Option<[u8; 4]> {
    if frame.len() < 20 || frame[0] >> 4 != 4 {
        return None;
    }
    Some([frame[16], frame[17], frame[18], frame[19]])
}

fn getsockname(fd: i32) -> Result<([u8; 4], u16), String> {
    let mut addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut addr, &mut len) } == -1 {
        return Err(format!("wg-device-socket-name-failed (errno={})", sys::errno()));
    }
    Ok((addr.sin_addr, u16::from_be(addr.sin_port)))
}

// ---------------------------------------------------------------------------
// the connector seam implementation
// ---------------------------------------------------------------------------

/// Poison-tolerant lock (same convention as connector/peer_conn).
trait LockPoison<T> {
    fn lock_poison(&self) -> MutexGuard<'_, T>;
}

impl<T> LockPoison<T> for Mutex<T> {
    fn lock_poison(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The REAL [`WgPeerApplier`]: drives a [`WgDevice`] from the connector's
/// network-map flow. `apply_peers` reconciles the peer set (add/remove/
/// allowed_ips update with the map snapshot), `apply_endpoint` lands the ICE
/// selected pair on the actual device AND triggers the handshake,
/// `tunnel_ready` reports the real session state — replacing the
/// "peers registered" approximation of [`crate::connector::WgPeerRegistry`]
/// in the N3-7 default-route gate.
///
/// Pump ownership: the owner drives the data plane through
/// [`WgDeviceApplier::with_device`] (`service_tun` / `service_udp` / `tick`
/// / `handle_udp`) on a worker thread with the real monotonic clock; in host
/// tests the same entry drives both instances on an injected clock.
pub struct WgDeviceApplier {
    device: Mutex<WgDevice>,
}

impl WgDeviceApplier {
    pub fn new(device: WgDevice) -> Self {
        WgDeviceApplier { device: Mutex::new(device) }
    }

    /// Exclusive device access (the data-plane pump / test entry).
    pub fn with_device<R>(&self, f: impl FnOnce(&mut WgDevice) -> R) -> R {
        f(&mut self.device.lock_poison())
    }
}

impl WgPeerApplier for WgDeviceApplier {
    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
        let specs: Vec<WgPeerSpec> = peers
            .iter()
            .map(|e| WgPeerSpec {
                pub_key_b64: e.pub_key_b64.clone(),
                allowed_ips: e
                    .allowed_ips
                    .iter()
                    .map(|r| (r.addr, r.prefix_len))
                    .collect(),
                preshared_key_b64: None,
            })
            .collect();
        self.device.lock_poison().set_peers(&specs)
    }

    fn clear(&self) {
        self.device.lock_poison().clear();
    }

    /// Real data-plane readiness (module docs): an established, non-expired
    /// WG session on at least one peer. The registry's "always false"
    /// approximation is replaced by actual session state.
    fn tunnel_ready(&self) -> bool {
        self.device.lock_poison().tunnel_ready()
    }

    /// Land the ICE selected pair on the device and trigger the handshake
    /// (upstream `ConfigureWGEndpoint`, conn.go:444-478). Fail-closed on
    /// unregistered peers (config bug, not a silent store).
    fn apply_endpoint(&self, pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        let now = sys::mono_ms();
        self.device.lock_poison().set_endpoint(pub_key_b64, addr, port, now)
    }
}
