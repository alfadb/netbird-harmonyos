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
//!
//! ## N7：生产默认 seam [`WgDeviceFeed`]
//!
//! 把本模块的设备驱动接成 connector 的**生产默认**：fd（受保护 WG UDP
//! socket + 平台 TUN）由壳侧经 `connector_wg_socket_feed` /
//! `connector_tun_fd_feed` 补给，两个 feed 齐备才 adopt 设备；网络图 peer
//! 与 ICE endpoint 在此之前**缓冲**，设备建成时重放。缺任一 feed ⇒ 无设备
//! ⇒ `tunnel_ready()=false` ⇒ 默认路由 HOLD（无任何未保护回退）。生产泵
//! 线程 [`WgDeviceFeed::spawn_data_plane_pump`] 以注入单调钟驱动
//! `service_tun`/`service_udp`/`tick`。
//!
//! ## N8：受控重建下的 TUN fd 替换（会话保留）
//!
//! 平台 `VpnConfig` 在 `create()` 时固定 ⇒ 数据面就绪后默认路由要装进隧道
//! 只能**重建连接**（壳侧 destroy → 新 create → 新 TUN fd）。设备侧合同：
//!
//! - [`WgDevice::replace_tun`]：先为新 fd 取 dup（失败则旧 TUN 原样保留，
//!   无半状态），再换入新 [`TunFd`]；旧 [`TunFd`] 的 Drop 只关**旧 dup**，
//!   壳侧原号永远只归 `VpnConnection.destroy()` 关（fd 合同不变）。
//! - **会话保留是零成本的**：BoringTun tunnel 是纯字节层状态机，从不引用
//!   TUN fd；WG 外层 UDP socket（`self.fd`）在重建中不变。因此换 TUN 不
//!   触碰任何握手/密钥状态——peer 会话原样存活，重建后载荷立即双向可通，
//!   不需要重新握手。
//! - feed seam：设备在位时 `feed_tun` = 替换（[`WgDeviceFeed::feed_tun`]）；
//!   `feed_wg_socket` 同号为幂等 no-op、异号拒绝（`socket-fd-conflict`）——
//!   受保护外层 socket 不可换。

//! ## N11：WG 骑 ICE 选中连接（egress 让渡 + 入向分用）
//!
//! 上游形态：ICE 与 WG 共用同一条 UDP 传输，接收侧按包类型分用（STUN →
//! ICE mux，WG/非 STUN → WireGuard，`client/iface/bind/ice_bind.go:313-345`），
//! WG endpoint = 对端 ICE 选中地址（`conn.go:453-460`）。本仓的对应实现：
//!
//! - **出向**：每个 peer 可挂一个 **egress fd**——ICE 选中 pair 本地 socket
//!   的 **dup 副本**（[`WgDevice::set_egress_socket`]，dup-only fd 合同
//!   不变；原始号仍归 provider/壳侧）。`send` 一律优先走 peer 的 egress
//!   fd，未挂时回落设备自身 socket（选中前无 endpoint，实际不发）。
//! - **入向**：设备**从不读**选中 socket——ICE 会话是唯一读者，非 STUN
//!   包经其分用后由编排层喂给 [`WgDevice::handle_udp`]（来源匹配 peer
//!   endpoint 的既有规则不变）。保活/检查与 WG 数据在同一条 socket 上
//!   互不误伤。
//! - **回收**：ICE 断开/失败 → [`WgDevice::recycle_endpoint`]：endpoint
//!   置空（无路径可发 = fail-closed）、关 egress dup、清握手战役——绝不
//!   静默沿用旧路径（上游 `RemoveEndpointAddress`，conn.go:531）。
//!
//! ## N13-D2：relay 作为第二条 WG 承载（等价上游 wgProxy，`Relay < ICE`）
//!
//! 上游把 relayed `net.Conn` 交给 wgProxy，伪造 `127.1.x.x` 假 UDP endpoint
//! 注册进 userspace bind（spec §7.1，`proxy.go:204-226`）；本实现的等价
//! seam 更直接：**不伪造地址**，把「路径选择」收进设备的一个裁决点
//! [`WgDevice::dispatch`]——
//!
//! - **优先级 `Relay < ICE`（最小忠实子集）**：peer 的 ICE endpoint 在位
//!   ⇒ 出向一律走 UDP 直连（egress dup 优先，语义与 N11 完全一致）；
//!   endpoint 不在位（无选中/已回收 = ICE 无提名或 Failed）且已注入
//!   [`WgEgressCarrier`] ⇒ 被封装的 WG 报文经载体交付（生产实现 =
//!   `crate::relay_client::RelayWgCarrier` → `RelayClient::send_to_peer`，
//!   即上游 wgProxy 的写侧）。ICE 重新提名（`set_endpoint`）⇒ 下一包起
//!   切回直连；`recycle_endpoint` **不清除**载体 ⇒ 自动回落 relay。
//!   切换可观测：[`WgDeviceStats::carrier_takeovers`]（出向首次经载体，
//!   每次 UDP→载体 episode 计 1）、[`WgDeviceStats::direct_restores`]
//!   （载体→直连）+ 有界日志（`N13_WG|direct->carrier` /
//!   `N13_WG|carrier->direct`）。
//! - **载体故障不阻塞直连**：endpoint 在位时 dispatch 根本不触碰载体；
//!   载体拒绝（容量/未就绪/超限）是**类型化**失败（`Err(reason)`），计
//!   [`WgDeviceStats::carrier_rejects`] 并有界记日志——绝不静默丢弃。
//! - **入向匹配映射**（与 `handle_udp` 的源地址匹配等价）：UDP 域里身份
//!   = `src == endpoint`；relay 域里没有 UDP 五元组，身份由帧自带——
//!   Transport 帧的 36B 字段是**发送方 peer id**（§4.2，服务端改写），
//!   而 peer id = `PeerId::from_wg_pubkey_string(对端 WG 公钥 base64)`
//!   （relay spec §3.2）。编排层（connector 的 `RelayCarrier`）持有
//!   「注册 peer 公钥 → peer id」反查表，把 sender id 反查回 WG 公钥后调
//!   [`WgDevice::handle_carrier`]，按 `key_b64` 定位 peer——与
//!   `handle_udp` 共用同一段收包体（解封装 → TUN / 回包 / 会话建立观察），
//!   既有 UDP 语义零改动。选择「公钥匹配」而非上游的假地址注册：假地址
//!   需要一个额外的状态位区分真假 endpoint，公钥匹配则把 relay 帧的既有
//!   身份字段直接用足。
//! - **MTU / 封装开销**（登记，spec §7.2）：Transport 帧 = 2B 头 + 36B id
//!   ⇒ 38B 固定开销，再套 WS 客户端帧（2–4B 头 + 4B mask）+ TLS/TCP/IP
//!   ⇒ 典型每包 ≈ 44–46B。`send_to_peer` 单包上限 **8782B**
//!   （8820 − 38）；设备侧报文物理上限 [`WG_BUF`] = 2048B，恒在限内——
//!   超限载荷只能在载体句柄处出现，且被 `RelayClientError::FrameTooLarge`
//!   类型化拒绝（见 `relay_client` 模块文档）。对 MTU 的影响：1280/1420
//!   的 WG MTU 均安全（1420 封装后 ≈ 1466B ≪ 8782）。
//! - **fd 合同（N13-D2 / N13 plan §4 重验项）**：relay 承载**不持有、不
//!   close 任何平台 fd**——[`WgEgressCarrier`] 是纯 Rust 对象（relay
//!   client 句柄），relay 的 WSS/TCP socket 是 native 自建（同 management
//!   通道，不经 `VpnConnection.protect`），其生命周期只随 relay client 的
//!   会话；平台 TUN 原始 fd 的关闭权**仅属**壳侧 `VpnConnection.destroy()`
//!   ——本模块（含载体路径）只经 [`TunFd`]/dup 副本接触 fd，载体挂载/
//!   拆除/`connector_stop()` 都不会触碰那个号码。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

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
    // -- N13-D2 relay-carrier counters (independent of the UDP counters
    //    above: relay bytes never appear in tx_*/rx_* UDP accounting and
    //    vice versa, so the two carriers stay reconcilable separately) --
    /// Encapsulated datagrams delivered via the injected non-UDP egress
    /// carrier (relay) — disjoint from `tx_packets`.
    pub carrier_tx_packets: u64,
    /// Encapsulated bytes delivered via the carrier.
    pub carrier_tx_bytes: u64,
    /// Typed carrier refusals (backpressure / not-ready / frame too large).
    /// Counted + logged — never silently dropped (WG retransmits).
    pub carrier_rejects: u64,
    /// Outbound episodes that STARTED on the carrier (first dispatch with
    /// no ICE endpoint after a non-carrier state — the UDP→relay takeover).
    pub carrier_takeovers: u64,
    /// Outbound episodes that returned to the ICE-selected UDP path after a
    /// carrier episode (the relay→direct restore on ICE nomination).
    pub direct_restores: u64,
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
    /// N11: local address of the attached egress socket (the ICE selected
    /// pair's local candidate) — `None` until WG rides the selected path.
    pub egress_local: Option<([u8; 4], u16)>,
    /// N13-D2: a non-UDP egress carrier (relay) is injected for the peer.
    pub carrier_attached: bool,
    /// N13-D2: the peer's outbound currently leaves via the carrier (last
    /// dispatch had no ICE endpoint) — the observable bearer source.
    pub on_carrier: bool,
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
// N13-D2: non-UDP egress carrier seam (the relay bearer injection point)
// ---------------------------------------------------------------------------

/// 出向载体 seam：一条**非 UDP socket** 的 WG 外层承载（生产实现 =
/// `crate::relay_client::RelayWgCarrier`，经 `RelayClient::send_to_peer`
/// 发出；测试可注入内存桩）。
///
/// 合同：
/// - `send_datagram` 收到的是**已封装**的 WG 报文（boringtun 输出），实现
///   方只做交付，不再加解密；
/// - `Ok(())` = 已接受上承载（relay 协议无投递 ack —— 这不是送达确认）；
/// - `Err(reason)` = **类型化拒绝**（背压 / 会话未就绪 / 帧超限等，reason
///   为稳定 shape token）：设备计 `carrier_rejects` 并有界记日志，绝不静默
///   丢弃（WG 自带重传）；
/// - 实现方**不得持有或关闭任何 fd**（fd 合同：relay 承载是纯 Rust 对象，
///   模块文档 N13-D2 节）；
/// - `Send + Sync`：设备在数据面泵线程里同步调用。
pub trait WgEgressCarrier: Send + Sync + 'static {
    /// Deliver one encapsulated WG datagram. See the trait contract above.
    fn send_datagram(&self, datagram: &[u8]) -> Result<(), String>;
    /// Bearer shape token for logs/diagnostics (e.g. `"relay-wss"`).
    fn kind(&self) -> &'static str;
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
    /// N11: OUR dup of the peer's ICE-selected local socket (egress). The
    /// raw/orig numbers (provider fd, the ICE session's own dup) belong to
    /// their owners; only this dup is closed here. `None` → sends fall back
    /// to the device socket (pre-selection; no endpoint exists then anyway).
    egress_fd: Option<i32>,
    /// getsockname of the egress dup at attach time (observability/tests).
    egress_local: Option<([u8; 4], u16)>,
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
    /// N13-D2: injected non-UDP egress carrier (relay). Dropped with the
    /// peer; `remove_carrier` detaches explicitly (relay unavailable /
    /// teardown). Never an fd — a pure Rust handle (fd contract).
    carrier: Option<Arc<dyn WgEgressCarrier>>,
    /// N13-D2: the bearer the peer's outbound LAST left by (`true` = the
    /// carrier). Drives the takeover/restore switch counters in `dispatch`.
    on_carrier: bool,
}

impl Drop for WgPeer {
    fn drop(&mut self) {
        // N11 fd contract: exactly our egress dup; provider/ICE-session fds
        // are never touched here.
        if let Some(fd) = self.egress_fd.take() {
            unsafe { sys::close(fd) };
        }
    }
}

impl WgPeer {
    fn stats(&self) -> (i64, u64, u64) {
        self.tunnel.stats()
    }

    fn status(&self) -> WgPeerStatus {
        let (hs, _, _) = self.stats();
        WgPeerStatus {
            pub_key_b64: self.key_b64.clone(),
            endpoint: self.endpoint,
            allowed_ips: self.allowed_ips.clone(),
            session_established: self.session_established,
            expired: self.expired,
            last_handshake_s: hs,
            egress_local: self.egress_local,
            carrier_attached: self.carrier.is_some(),
            on_carrier: self.on_carrier,
        }
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

    /// N8 — controlled-recreate TUN replacement: adopt a dup of the NEW
    /// platform TUN fd and deactivate the old one, keeping peers, tunnels
    /// AND established WG sessions (the session-preserving choice, see the
    /// module docs N8 section). Semantics:
    /// - the new fd is validated by `TunFd::dup_from_raw` FIRST; on any
    ///   failure the OLD [`TunFd`] stays active untouched (no half state —
    ///   the caller sees the error and runs its fail-closed teardown);
    /// - the swap drops the old [`TunFd`], whose Drop closes ONLY the old
    ///   native dup (fd contract: the shell-side raw fd keeps belonging to
    ///   `VpnConnection.destroy()`; native never closes that number);
    /// - sessions survive because a BoringTun tunnel is a pure byte-level
    ///   state machine: it never references the TUN fd. Only the plaintext
    ///   sink/source changes, so payload continues over the same WG keys.
    pub fn replace_tun(&mut self, tun_raw: i32) -> Result<(), crate::tun::TunError> {
        let tun = TunFd::dup_from_raw(tun_raw)?;
        let old = core::mem::replace(&mut self.tun, tun);
        drop(old); // closes the old DUP only (fd contract)
        emit("N8_WG_DEVICE|tun-replaced|sessions-kept");
        Ok(())
    }

    /// Our current TUN dup fd number (`None` if the TunFd were closed —
    /// observability only; always distinct from the shell-side raw numbers).
    pub fn tun_fd(&self) -> Option<i32> {
        self.tun.fd()
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
                        egress_fd: None,
                        egress_local: None,
                        session_established: false,
                        session_established_ms: 0,
                        expired: false,
                        first_init_ms: None,
                        last_init_ms: None,
                        last_outbound_ms: None,
                        carrier: None,
                        on_carrier: false,
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

    /// N11 — attach the peer's egress socket: a DUP of the ICE-selected
    /// pair's local socket (upstream shape: WG rides the selected transport;
    /// `conn.go:453-460` reads the remote address off that transport and
    /// `ice_bind.go` shares the socket). The provided number is BORROWED:
    /// dup-only, original never closed here (fd contract, same as
    /// `adopt`/`reattach_socket`). Attach BEFORE `set_endpoint` — the
    /// endpoint landing fires the handshake, which must leave via the
    /// selected path. Unknown peers are REJECTED (fail-closed).
    pub fn set_egress_socket(&mut self, pub_key_b64: &str, raw_fd: i32) -> Result<(), String> {
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        };
        let fd = crate::mgmtsock::dup_socket_fd(raw_fd)
            .map_err(|e| format!("wg-device-egress-bad-fd (errno={})", e.errno()))?;
        let local = match getsockname(fd) {
            Ok(l) => l,
            Err(e) => {
                unsafe { sys::close(fd) };
                return Err(e);
            }
        };
        if let Some(old) = self.peers[idx].egress_fd.replace(fd) {
            unsafe { sys::close(old) }; // replaced attach: close OUR old dup only
        }
        self.peers[idx].egress_local = Some(local);
        emit(&format!(
            "N11_WG|egress-attach|local={}.{}.{}.{}:{}",
            local.0[0], local.0[1], local.0[2], local.0[3], local.1
        ));
        Ok(())
    }

    /// N11 — recycle the peer's endpoint (upstream `RemoveEndpointAddress`,
    /// `conn.go:531` on ICE disconnect): endpoint cleared (nothing can be
    /// sent — fail-closed), egress dup closed, handshake campaign reset.
    /// The WG session/keys are kept (rekeying on the next path is normal WG
    /// semantics); `tunnel_ready` semantics are unchanged. N13-D2: an
    /// attached relay carrier is deliberately KEPT — this is exactly the
    /// ICE-lost → relay fallback transition (`dispatch` routes the next
    /// outbound through the carrier; a fresh handshake campaign re-arms on
    /// the next tick because `has_path` stays true).
    pub fn recycle_endpoint(&mut self, pub_key_b64: &str) {
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            return;
        };
        let p = &mut self.peers[idx];
        let had_path = p.endpoint.is_some() || p.egress_fd.is_some();
        p.endpoint = None;
        if let Some(fd) = p.egress_fd.take() {
            unsafe { sys::close(fd) };
        }
        p.egress_local = None;
        p.first_init_ms = None;
        p.last_init_ms = None;
        if had_path {
            emit("N11_WG|endpoint-recycled");
        }
    }

    /// N13-D2 — inject the peer's non-UDP egress carrier (relay bearer, the
    /// wgProxy equivalent). Idempotent upsert: a re-attach replaces the
    /// previous carrier object. Unknown peers are REJECTED (fail-closed,
    /// same rule as `set_endpoint`). The carrier is a pure Rust handle —
    /// no fd crosses this seam (module docs, N13-D2 fd contract).
    pub fn set_carrier(&mut self, pub_key_b64: &str, carrier: Arc<dyn WgEgressCarrier>) -> Result<(), String> {
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        };
        let replaced = self.peers[idx].carrier.is_some();
        self.peers[idx].carrier = Some(carrier);
        emit(&format!(
            "N13_WG|carrier-attach|kind={}|replaced={replaced}",
            self.peers[idx]
                .carrier
                .as_ref()
                .map(|c| c.kind())
                .unwrap_or("?")
        ));
        Ok(())
    }

    /// N13-D2 — detach the peer's relay carrier (relay unavailable / lane
    /// revoked / teardown). After this the peer has whatever path remains —
    /// the ICE endpoint if nominated, otherwise NO path (fail-closed, the
    /// pre-D2 behavior). `true` when a carrier was actually removed.
    pub fn remove_carrier(&mut self, pub_key_b64: &str) -> bool {
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            return false;
        };
        if self.peers[idx].carrier.take().is_some() {
            // the bearer source resets with the carrier: the next dispatch
            // (direct OR a re-attached carrier) counts as a fresh transition
            self.peers[idx].on_carrier = false;
            emit("N13_WG|carrier-detach");
            true
        } else {
            false
        }
    }

    /// N13-D2 — detach every peer's relay carrier (bulk teardown).
    pub fn remove_all_carriers(&mut self) {
        for peer in self.peers.iter_mut() {
            if peer.carrier.take().is_some() {
                peer.on_carrier = false;
            }
        }
        emit("N13_WG|carrier-detach-all");
    }

    /// Whether the peer has ANY outbound path: the ICE-selected UDP path
    /// (endpoint) or an injected carrier (relay).
    fn has_path(&self, idx: usize) -> bool {
        self.peers[idx].endpoint.is_some() || self.peers[idx].carrier.is_some()
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
            // A growing no-route count with a live session means the routing
            // table lacks the destination — dump what IS installed (the probe
            // path dropped 1/s silently before this; device-validation run 6
            // showed dropped_no_route climbing while tx_bytes stayed 0).
            if self.stats.no_route_drops <= 3 || self.stats.no_route_drops % 60 == 0 {
                let table: String = self
                    .peers
                    .iter()
                    .flat_map(|p| {
                        p.allowed_ips.iter().map(move |(net, plen)| {
                            format!(
                                "{}.{}.{}.{}/{}",
                                net[0], net[1], net[2], net[3], plen
                            )
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(",");
                emit(&format!(
                    "N6_WG_DEVICE|no-route|dst={}.{}.{}.{}|peers={}|routes=[{}]",
                    dst[0],
                    dst[1],
                    dst[2],
                    dst[3],
                    self.peers.len(),
                    table
                ));
            }
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
        self.ingest(idx, datagram, InboundPath::Udp(src), now_ms, &mut out);
        out
    }
    /// N13-D2 — process one inbound datagram that arrived over the peer's
    /// non-UDP egress carrier (relay). Matching rule (module docs, N13-D2
    /// 「入向匹配映射」): the carrier layer already identified the SENDER —
    /// the relay Transport frame's 36B field is the sender peer id, which
    /// the orchestration layer reverses to the WG public key. Matching is
    /// therefore by registered `key_b64` — the identity-domain equivalent
    /// of `handle_udp`'s `src == endpoint` rule, sharing the same ingest
    /// body (decapsulate → TUN / reply / session observation). Unknown keys
    /// are dropped + counted (`unknown_peer_drops`), never processed.
    pub fn handle_carrier(&mut self, datagram: &[u8], pub_key_b64: &str, now_ms: u64) -> WgInbound {
        let mut out = WgInbound::default();
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            self.stats.unknown_peer_drops += 1;
            return out;
        };
        out.peer_matched = true;
        self.stats.rx_packets += 1;
        self.ingest(idx, datagram, InboundPath::Carrier, now_ms, &mut out);
        out
    }

    /// Shared inbound body of BOTH receive paths (UDP + carrier): decrypt →
    /// TUN / reply, then the session-establishment observation + queued
    /// packet flush. The reply path follows the arrival path for UDP (back
    /// to `src`, the pre-D2 rule) and the bearer priority for carrier
    /// arrivals (`dispatch`).
    fn ingest(
        &mut self,
        idx: usize,
        datagram: &[u8],
        path: InboundPath,
        now_ms: u64,
        out: &mut WgInbound,
    ) {
        self.process_datagram(idx, datagram, path, out);

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
            // handshake campaign (a path exists — ICE endpoint or relay
            // carrier (N13-D2) — but no live session)
            if self.has_path(idx) && !self.peers[idx].session_established {
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
            // real clock); whatever datagram becomes due goes out now over
            // the dispatch-selected bearer (N13-D2: counted when it was
            // silently dropped pre-endpoint before)
            let mut out = [0u8; WG_BUF];
            let (op, len) = { self.peers[idx].tunnel.tick(&mut out) };
            if op == OP_NETWORK && len > 0 && self.has_path(idx) {
                let datagram = out[..len].to_vec();
                if self.dispatch(idx, &datagram) {
                    sent += 1;
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
    /// handshake responses/cookies → out (UDP: back to the arrival `src`;
    /// carrier: via the bearer-priority `dispatch`).
    fn process_datagram(
        &mut self,
        idx: usize,
        datagram: &[u8],
        path: InboundPath,
        out: &mut WgInbound,
    ) {
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
                let sent = match path {
                    InboundPath::Udp(src) => {
                        let fd = self.egress_fd_of(idx);
                        self.send_to(fd, &plain[..len], src)
                    }
                    // reply rides the selected bearer (ICE endpoint if it
                    // won meanwhile, the carrier otherwise)
                    InboundPath::Carrier => self.dispatch(idx, &plain[..len]),
                };
                if sent {
                    out.sent += 1;
                }
            }
            OP_ERROR => self.stats.decrypt_errors += 1,
            _ => {} // OP_DONE: keepalive/cookie absorbed
        }
    }

    /// N13-D2 bearer dispatch — THE single egress decision point (module
    /// docs, N13-D2): ICE endpoint present ⇒ UDP direct (egress dup first,
    /// device socket fallback — the unchanged N11 semantics); otherwise an
    /// injected carrier (relay) ⇒ typed delivery; neither ⇒ counted
    /// failure (fail-closed, pre-D2 behavior). The UDP→carrier and
    /// carrier→UDP transitions are counted + logged here.
    fn dispatch(&mut self, idx: usize, datagram: &[u8]) -> bool {
        if self.peers[idx].endpoint.is_some() {
            if self.peers[idx].on_carrier {
                self.peers[idx].on_carrier = false;
                self.stats.direct_restores += 1;
                emit("N13_WG|carrier->direct|ice-selected-path");
            }
            let ep = self.peers[idx].endpoint.expect("endpoint checked above");
            let fd = self.egress_fd_of(idx);
            return self.send_to(fd, datagram, ep);
        }
        if !self.peers[idx].on_carrier {
            self.peers[idx].on_carrier = true;
            self.stats.carrier_takeovers += 1;
            emit("N13_WG|direct->carrier|no-ice-endpoint");
        }
        let Some(carrier) = self.peers[idx].carrier.clone() else {
            // routed but never given any path: fail-closed, counted
            self.stats.send_errors += 1;
            return false;
        };
        match carrier.send_datagram(datagram) {
            Ok(()) => {
                self.stats.carrier_tx_packets += 1;
                self.stats.carrier_tx_bytes += datagram.len() as u64;
                true
            }
            Err(reason) => {
                // typed refusal, never silent: counted + bounded log (the
                // reason is a shape token; the payload is never logged)
                self.stats.carrier_rejects += 1;
                if self.stats.carrier_rejects <= 3 || self.stats.carrier_rejects % 100 == 0 {
                    emit(&format!(
                        "N13_WG|carrier-reject|kind={}|reason={reason}|count={}",
                        carrier.kind(),
                        self.stats.carrier_rejects
                    ));
                }
                false
            }
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
    /// The egress bearer is picked by `dispatch` (ICE direct, else carrier).
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
        if !self.dispatch(idx, &datagram) {
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

    /// Force a handshake initiation to the peer (fresh ephemeral every
    /// call — the retransmit/rekey form). Requires a path (`has_path`); the
    /// bearer is picked by `dispatch`.
    fn initiate_handshake(&mut self, idx: usize, now_ms: u64) -> bool {
        let mut ct = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.force_handshake(&mut ct) };
        if op != OP_NETWORK || len == 0 {
            return false;
        }
        let datagram = ct[..len].to_vec();
        if !self.dispatch(idx, &datagram) {
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
    /// The bearer is picked by `dispatch`.
    fn send_keepalive(&mut self, idx: usize, now_ms: u64) -> bool {
        let mut ct = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.write(&[], &mut ct) };
        if op != OP_NETWORK || len == 0 {
            return false;
        }
        let datagram = ct[..len].to_vec();
        if !self.dispatch(idx, &datagram) {
            return false;
        }
        self.stats.tx_packets += 1;
        self.stats.keepalives_sent += 1;
        self.peers[idx].last_outbound_ms = Some(now_ms);
        true
    }

    /// Flush boringtun's queued packets after session establishment
    /// (decapsulate-empty repeat-until-Done, bounded; ffi contract).
    /// The bearer is picked by `dispatch`.
    fn flush_queued(&mut self, idx: usize, now_ms: u64) -> usize {
        let mut sent = 0usize;
        for _ in 0..FLUSH_MAX {
            let mut ct = [0u8; WG_BUF];
            let (op, len) = { self.peers[idx].tunnel.read(&[], &mut ct) };
            if op != OP_NETWORK || len == 0 {
                break;
            }
            let datagram = ct[..len].to_vec();
            if !self.dispatch(idx, &datagram) {
                break;
            }
            self.stats.tx_packets += 1;
            self.peers[idx].last_outbound_ms = Some(now_ms);
            sent += 1;
        }
        sent
    }

    /// sendto on the given socket dup (`egress_fd_of(idx)`: the peer's
    /// ICE-selected socket, falling back to the device socket). `None` or a
    /// sendto failure counts `send_errors` and returns false.
    fn send_to(&mut self, fd: Option<i32>, data: &[u8], dst: ([u8; 4], u16)) -> bool {
        let Some(fd) = fd else {
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

    /// N11: the socket this peer's datagrams leave by — the attached egress
    /// (ICE-selected socket dup) when present, the device socket otherwise.
    fn egress_fd_of(&self, idx: usize) -> Option<i32> {
        self.peers[idx].egress_fd.or(self.fd)
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

/// Which path an inbound datagram arrived by — determines where its reply
/// leaves (N13-D2: UDP replies go back to the arrival address; carrier
/// replies follow the bearer priority via `dispatch`).
#[derive(Debug, Clone, Copy)]
enum InboundPath {
    /// Plain UDP: replies go back to the arrival address (pre-D2 rule).
    Udp(([u8; 4], u16)),
    /// The relay carrier: replies follow the bearer priority.
    Carrier,
}

/// N11 (tests / host CLI): derive the x25519 PUBLIC key (base64 std) of a
/// base64(std) secret through the same frozen boringtun export the device's
/// tunnels use — a test cannot drift from the data plane by deriving keys
/// differently. `None` on malformed input (not base64 / not 32 bytes).
pub fn x25519_public_b64(secret_b64: &str) -> Option<String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(secret_b64.trim())
        .ok()?;
    let raw: [u8; 32] = raw.try_into().ok()?;
    let sk = boringtun::ffi::x25519_key { key: raw };
    let pk = boringtun::ffi::x25519_public_key(sk);
    crate::wg::key_to_b64(pk)
}

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
// status surface for the connector (N7)
// ---------------------------------------------------------------------------

/// Data-plane status exposed through `connector_status()` (`wg` field).
/// Counters are the REAL [`WgDeviceStats`] values once a device is up; all
/// booleans false / counters zero while the data plane is not started
/// (fail-closed: no shell feeds ⇒ no device ⇒ nothing ready). No key
/// material — public-key-free by construction (per-peer detail would carry
/// public keys only, but the summary does not even need that).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgDataplaneStatus {
    /// The shell fed a (protected) WG UDP socket fd.
    pub fed_socket: bool,
    /// The shell fed the platform TUN fd.
    pub fed_tun: bool,
    /// The device was constructed from both feeds (data plane pumping).
    pub device_up: bool,
    /// REAL `tunnel_ready()`: ≥1 established, non-expired WG session.
    pub ready: bool,
    /// Peers whose WG session is established and non-expired.
    pub peers_with_session: usize,
    /// Handshake initiations sent (initial + retransmits + rekeys).
    pub handshakes: u64,
    pub tx_packets: u64,
    /// Encapsulated transport BYTES handed to sendto (N2-H counter
    /// reconciliation reads byte counters, not only packet counts).
    pub tx_bytes: u64,
    pub rx_packets: u64,
    /// Plaintext bytes written into the TUN (N2-H: reconciled against an
    /// independent drain of the TUN interface).
    pub rx_bytes_to_tun: u64,
    /// Outbound frames dropped: no allowed_ips prefix matched.
    pub dropped_no_route: u64,
    /// Inbound datagrams a matched peer failed to process.
    pub decrypt_errors: u64,
    // -- N13-D2 relay-carrier counters (appended to the JSON AFTER the
    //    pre-D2 fields; consumers read the head positionally) --
    /// Encapsulated datagrams delivered via the relay carrier (disjoint
    /// from `tx_packets` — the two bearers stay separately reconcilable).
    pub carrier_tx_packets: u64,
    /// Encapsulated bytes delivered via the relay carrier.
    pub carrier_tx_bytes: u64,
    /// Typed carrier refusals (counted, never silent).
    pub carrier_rejects: u64,
    /// UDP→relay outbound takeovers (ICE lost / never nominated).
    pub carrier_takeovers: u64,
    /// relay→UDP restores (ICE (re)nominated while a carrier episode ran).
    pub direct_restores: u64,
}

impl WgDataplaneStatus {
    /// The unfed / not-started snapshot (all false, all zero).
    pub fn not_started() -> Self {
        WgDataplaneStatus::default()
    }

    /// The `connector_status()` `wg` JSON object.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}}}",
            crate::util::jbool("fed_socket", self.fed_socket),
            crate::util::jbool("fed_tun", self.fed_tun),
            crate::util::jbool("device_up", self.device_up),
            crate::util::jbool("ready", self.ready),
            crate::util::jinum("peers_with_session", self.peers_with_session as i64),
            crate::util::jinum("handshakes", self.handshakes as i64),
            crate::util::jinum("tx_packets", self.tx_packets as i64),
            crate::util::jinum("tx_bytes", self.tx_bytes as i64),
            crate::util::jinum("rx_packets", self.rx_packets as i64),
            crate::util::jinum("rx_bytes_to_tun", self.rx_bytes_to_tun as i64),
            crate::util::jinum("dropped_no_route", self.dropped_no_route as i64),
            crate::util::jinum("decrypt_errors", self.decrypt_errors as i64),
            crate::util::jinum("carrier_tx_packets", self.carrier_tx_packets as i64),
            crate::util::jinum("carrier_tx_bytes", self.carrier_tx_bytes as i64),
            crate::util::jinum("carrier_rejects", self.carrier_rejects as i64),
            crate::util::jinum("carrier_takeovers", self.carrier_takeovers as i64),
            crate::util::jinum("direct_restores", self.direct_restores as i64),
        )
    }
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

impl WgPeerApplier for WgDeviceApplier {    fn apply_peers(&self, peers: &[WgPeerEntry]) -> Result<(), String> {
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

    /// N11: dup the ICE-selected local socket into the peer's egress slot
    /// (module docs: WG rides the selected transport).
    fn attach_egress_socket(&self, pub_key_b64: &str, raw_fd: i32) -> Result<(), String> {
        self.device.lock_poison().set_egress_socket(pub_key_b64, raw_fd)
    }

    /// N11: hand a demuxed non-STUN datagram to the device (source-match +
    /// decapsulate; returns reply datagrams sent).
    fn handle_udp_inbound(&self, datagram: &[u8], src: ([u8; 4], u16), now_ms: u64) -> usize {
        self.device.lock_poison().handle_udp(datagram, src, now_ms).sent
    }

    /// N11: recycle endpoint + egress (fail-closed on ICE teardown).
    fn recycle_endpoint(&self, pub_key_b64: &str) {
        self.device.lock_poison().recycle_endpoint(pub_key_b64);
    }

    /// N13-D2: inject the peer's relay carrier into the live device.
    fn attach_carrier(
        &self,
        pub_key_b64: &str,
        carrier: std::sync::Arc<dyn WgEgressCarrier>,
    ) -> Result<(), String> {
        self.device.lock_poison().set_carrier(pub_key_b64, carrier)
    }

    /// N13-D2: detach the peer's relay carrier.
    fn detach_carrier(&self, pub_key_b64: &str) {
        self.device.lock_poison().remove_carrier(pub_key_b64);
    }

    /// N13-D2: detach EVERY carrier on the live device.
    fn detach_all_carriers(&self) {
        self.device.lock_poison().remove_all_carriers();
    }

    /// N13-D2: the seam CAN carry relay lanes (the device accepts carrier
    /// injection — the capability probe behind the connector's pump start).
    fn carrier_capable(&self) -> bool {
        true
    }

    /// N13-D2: hand a relay-arrived datagram to the device (key-matched;
    /// returns reply datagrams sent).
    fn handle_carrier_inbound(&self, datagram: &[u8], pub_key_b64: &str, now_ms: u64) -> usize {
        self.device.lock_poison().handle_carrier(datagram, pub_key_b64, now_ms).sent
    }
}

// ---------------------------------------------------------------------------
// WgDeviceFeed — the PRODUCTION-DEFAULT seam (N7)
// ---------------------------------------------------------------------------

/// Pump cadence for the production data-plane thread (same shape as the ICE
/// pump; the device itself is clock-injected — this loop only FEEDS it).
pub const WG_PUMP_TICK_MS: u64 = 100;

/// Why a [`WgDeviceFeed::feed_wg_socket`] / [`WgDeviceFeed::feed_tun`] call
/// was refused (stable tokens only — they cross the NAPI error surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgFeedRefusal {
    /// fd < 0.
    FdMissing,
    /// Not an open descriptor (dup/F_GETFD failed — dead or foreign fd).
    FdInvalid { errno: i32 },
    /// N8: a DIFFERENT WG outer socket fed while the device is up. The
    /// protected outer socket is NOT swappable (the controlled recreate
    /// replaces only the platform TUN); the same number is an idempotent
    /// no-op, a different number is refused fail-closed.
    SocketSwapUnsupported,
    /// N8: the new TUN fd could not be adopted at replace time (it died
    /// between the boundary probe and the swap). The OLD TUN stays active —
    /// no half state; the caller runs its fail-closed teardown.
    TunReplaceFailed { errno: i32 },
}

impl WgFeedRefusal {
    /// Stable error token, same family as the other feed seams.
    pub fn token(&self) -> &'static str {
        match self {
            WgFeedRefusal::FdMissing => "socket-fd-missing",
            WgFeedRefusal::FdInvalid { .. } => "socket-fd-invalid",
            WgFeedRefusal::SocketSwapUnsupported => "socket-fd-conflict",
            WgFeedRefusal::TunReplaceFailed { .. } => "tun-replace-failed",
        }
    }

    pub fn errno(&self) -> i32 {
        match self {
            WgFeedRefusal::FdMissing => 0,
            WgFeedRefusal::FdInvalid { errno } => *errno,
            WgFeedRefusal::SocketSwapUnsupported => 0,
            WgFeedRefusal::TunReplaceFailed { errno } => *errno,
        }
    }
}

/// The PRODUCTION-DEFAULT [`WgPeerApplier`] (N7): a [`WgDevice`] that comes
/// alive only when the SHELL feeds both fds the data plane needs —
///
/// - the WG outer UDP socket (native-opened via `wg_fwd_open`, then
///   `VpnConnection.protect`ed by the shell — §二.4: protect before any
///   datagram flows), fed through `connector_wg_socket_feed`;
/// - the platform TUN fd (from `VpnConnection.create()`), fed through
///   `connector_tun_fd_feed`.
///
/// ## fail-closed semantics
///
/// Until BOTH feeds arrive (and a device is successfully adopted), the seam
/// behaves like an honest registry: peer registrations and endpoint landings
/// are BUFFERED and acknowledged (the control plane keeps working), while
/// `tunnel_ready()` stays FALSE — so the N3-7 default-route gate HOLDS
/// `0.0.0.0/0`. There is NO fallback path that creates a socket or adopts a
/// device without a shell feed; a feed of a dead fd is REFUSED at the
/// boundary (dup probe) and never stored.
///
/// ## fd 合同（§二.4）
///
/// Fed fd numbers are BORROWED: stored raw numbers stay owned by the shell
/// (protect/`VpnConnection` lifecycle); at device construction native takes
/// dup copies ONLY (`dup_socket_fd` for the socket, `TunFd::dup_from_raw`
/// for the TUN) and never uses/closes the originals. When construction fails
/// (e.g. an fd died between feed and adopt), both stored numbers are dropped
/// and the data plane stays down — a fresh matching feed pair retries.
///
/// ## buffering + replay
///
/// The network map usually arrives BEFORE both feeds (sync runs while
/// create() has not happened yet), and the ICE selected pair can land before
/// them too. `apply_peers` and `apply_endpoint` are therefore buffered in
/// full and REPLAYED into the device at construction (latest endpoint per
/// peer wins — repeated landings must not re-trigger multiple handshakes).
pub struct WgDeviceFeed {
    cfg: WgDeviceConfig,
    inner: Mutex<FeedInner>,
}

#[derive(Default)]
struct FeedInner {
    /// Shell-fed RAW fd numbers (borrowed; native dup-only at adopt time).
    wg_socket_raw: Option<i32>,
    tun_raw: Option<i32>,
    /// Buffered peer set (full-snapshot semantics, as received).
    peers: Vec<crate::connector::WgPeerEntry>,
    /// Latest endpoint per peer (upsert; replay order deterministic).
    endpoints: Vec<(String, [u8; 4], u16)>,
    /// N11: buffered egress attach per peer (key → BORROWED raw fd of the
    /// ICE-selected local socket), upsert; replayed BEFORE endpoints so the
    /// replayed handshakes leave via the selected path. Each number is
    /// dup-probe validated at buffer time and again at replay time (a
    /// session that closed meanwhile fails loudly, never silently).
    egress: Vec<(String, i32)>,
    /// N13-D2: buffered relay carrier per peer (key → carrier handle),
    /// upsert; replayed after egress and BEFORE endpoints (a replayed
    /// endpoint fires its handshake, which must leave via the bearer that
    /// priority selects — the device's `dispatch` decides at send time).
    /// Carriers are pure Rust handles — no fd is stored here (fd contract).
    carriers: Vec<(String, Arc<dyn WgEgressCarrier>)>,
    /// Live device once both feeds arrived and adoption succeeded.
    device: Option<Arc<WgDeviceApplier>>,
    /// Audit counters (tests + honest observation; not in the status JSON).
    feeds_accepted: u64,
    build_failures: u64,
}

impl WgDeviceFeed {
    /// A fresh, unfed slot for `cfg` (device config: local identity +
    /// injected-clock timer policy).
    pub fn new(cfg: WgDeviceConfig) -> WgDeviceFeed {
        WgDeviceFeed { cfg, inner: Mutex::new(FeedInner::default()) }
    }

    /// Shell feed #1: the (protected) WG outer UDP socket fd. The number is
    /// validated by an immediate dup probe (fail-closed at the boundary) and
    /// stored raw; the device dups it again at adopt time.
    pub fn feed_wg_socket(&self, raw_fd: i32) -> Result<(), WgFeedRefusal> {
        self.feed_fd(raw_fd, FeedWhich::WgSocket)
    }

    /// Shell feed #2: the platform TUN fd (same validation + ownership
    /// rules; `TunFd::dup_from_raw` carries the executable fd contract).
    /// N8: while the device is up this REPLACES the platform TUN fd
    /// (controlled recreate — [`WgDevice::replace_tun`]); the WG socket feed
    /// stays first-write-wins and is idempotent while up.
    pub fn feed_tun(&self, raw_fd: i32) -> Result<(), WgFeedRefusal> {
        self.feed_fd(raw_fd, FeedWhich::Tun)
    }

    fn feed_fd(&self, raw_fd: i32, which: FeedWhich) -> Result<(), WgFeedRefusal> {
        if raw_fd < 0 {
            return Err(WgFeedRefusal::FdMissing);
        }
        // boundary validation: refuse dead fds BEFORE storing. The probe dup
        // is closed immediately (fd discipline: nothing leaks, the number is
        // stored raw and the device takes its own dup at adopt time).
        match crate::mgmtsock::dup_socket_fd(raw_fd) {
            Ok(probe) => {
                unsafe { sys::close(probe) };
            }
            Err(e) => return Err(WgFeedRefusal::FdInvalid { errno: e.errno() }),
        }
        let built = {
            let mut g = self.inner.lock_poison();
            match which {
                FeedWhich::WgSocket => {
                    // N8: while a device is up the protected WG outer socket
                    // is NOT swappable. Same number = idempotent no-op (the
                    // controlled recreate re-feeds nothing); a DIFFERENT
                    // number is refused fail-closed (the running device keeps
                    // its dup of the original protected socket).
                    if let Some(stored) = g.wg_socket_raw {
                        if g.device.is_some() {
                            if raw_fd == stored {
                                g.feeds_accepted += 1;
                                emit("N8_WG_FEED|socket-feed-idempotent|device-kept");
                                return Ok(());
                            }
                            return Err(WgFeedRefusal::SocketSwapUnsupported);
                        }
                    }
                    g.wg_socket_raw = Some(raw_fd);
                }
                FeedWhich::Tun => {
                    if let Some(dev) = g.device.as_ref() {
                        // N8 controlled recreate: a TUN feed while the device
                        // is up REPLACES the platform TUN fd (adopt the new
                        // dup, deactivate the old; sessions kept). On failure
                        // the old TUN stays active (no half state) and the
                        // refusal surfaces to the shell's fail-closed path.
                        if let Err(e) = dev.with_device(|d| d.replace_tun(raw_fd)) {
                            return Err(WgFeedRefusal::TunReplaceFailed { errno: e.errno() });
                        }
                        g.tun_raw = Some(raw_fd);
                        g.feeds_accepted += 1;
                        emit("N8_WG_FEED|tun-replaced|device-kept");
                        return Ok(());
                    }
                    g.tun_raw = Some(raw_fd);
                }
            }
            g.feeds_accepted += 1;
            Self::try_build(&mut g, &self.cfg)
        };
        if built {
            emit("N7_WG_FEED|device-up|data-plane-started");
        }
        Ok(())
    }

    /// Whether the device is live (data plane constructed and pumping).
    pub fn device_up(&self) -> bool {
        self.inner.lock_poison().device.is_some()
    }

    /// Exclusive device access for the data-plane pump / tests. `None` while
    /// the device is not up (fail-closed: nothing to pump).
    pub fn with_device<R>(&self, f: impl FnOnce(&mut WgDevice) -> R) -> Option<R> {
        let g = self.inner.lock_poison();
        g.device.as_ref().map(|d| d.with_device(f))
    }

    /// Spawn the PRODUCTION data-plane pump thread: every
    /// [`WG_PUMP_TICK_MS`] it drives `service_tun` / `service_udp` / `tick`
    /// on the real monotonic clock while a device is up, and exits on the
    /// connector stop flag. A detached std::thread (never blocks a runtime
    /// worker); without feeds it simply no-ops until the device appears.
    pub fn spawn_data_plane_pump(self: &Arc<Self>, stop_flag: Arc<AtomicBool>) {
        let slot = Arc::clone(self);
        let _ = std::thread::Builder::new().name("wg-pump".into()).spawn(move || {
            while !stop_flag.load(Ordering::Acquire) {
                std::thread::sleep(core::time::Duration::from_millis(WG_PUMP_TICK_MS));
                if stop_flag.load(Ordering::Acquire) {
                    return;
                }
                let now = sys::mono_ms();
                // one pump step; no-op while the device is not up
                let _ = slot.with_device(|d| {
                    d.service_tun(now);
                    d.service_udp(now);
                    d.tick(now);
                });
            }
        });
    }

    /// Build the device when both feeds are present. Returns whether a
    /// device came up. On ANY failure both stored fds are dropped (data
    /// plane stays down; a fresh matching feed pair retries) and the failure
    /// is counted + logged.
    fn try_build(g: &mut FeedInner, cfg: &WgDeviceConfig) -> bool {
        if g.device.is_some() {
            return false;
        }
        let (Some(wg_raw), Some(tun_raw)) = (g.wg_socket_raw, g.tun_raw) else {
            return false; // still missing a feed — the buffered state is kept
        };
        let tun = match TunFd::dup_from_raw(tun_raw) {
            Ok(t) => t,
            Err(e) => return Self::build_failed(g, &format!("tun-dup-{}-errno-{}", e.name(), e.errno())),
        };
        let dev = match WgDevice::adopt(cfg.clone(), wg_raw, tun) {
            Ok(d) => d,
            Err(e) => return Self::build_failed(g, &e),
        };
        let app = Arc::new(WgDeviceApplier::new(dev));
        // replay the buffered control-plane state; peer-set errors here mean
        // buffered entries the device rejects — fail the build (the map is
        // re-synced with the same content, so a persistent error stays loud
        // via wg_apply_failed instead of silently dropping peers)
        if let Err(e) = app.apply_peers(&g.peers) {
            return Self::build_failed(g, &format!("replay-peers-{e}"));
        }
        // N11: egress FIRST (the selected transport), then the N13-D2 relay
        // carriers, then endpoints — a replayed endpoint fires its handshake,
        // which must leave via the path `dispatch` selects. A stale buffered
        // fd (session closed meanwhile) surfaces as a replay failure; the
        // next ICE selection re-attaches.
        for (key, fd) in &g.egress {
            if let Err(e) = app.attach_egress_socket(key, *fd) {
                emit(&format!("N11_WG_FEED|replay-egress-failed|{}", e));
            }
        }
        for (key, carrier) in &g.carriers {
            if let Err(e) = app.attach_carrier(key, carrier.clone()) {
                emit(&format!("N13_WG_FEED|replay-carrier-failed|{}", e));
            }
        }
        for (key, addr, port) in &g.endpoints {
            // individually validated at buffer time; a replay failure would
            // mean the peer vanished from the just-applied set — logged (the
            // key is PUBLIC material), the next ICE re-selection re-lands it
            if let Err(e) = app.apply_endpoint(key, *addr, *port) {
                emit(&format!("N7_WG_FEED|replay-endpoint-failed|{}", e));
            }
        }
        emit(&format!(
            "N7_WG_FEED|adopted|peers={}|endpoints={}",
            g.peers.len(),
            g.endpoints.len()
        ));
        g.device = Some(app);
        true
    }

    /// Common build-failure handling: count, log the token, drop BOTH fed
    /// fds (the pair must be re-fed to retry — keeps the state machine
    /// honest about which fds actually worked).
    fn build_failed(g: &mut FeedInner, reason: &str) -> bool {
        g.build_failures += 1;
        g.wg_socket_raw = None;
        g.tun_raw = None;
        emit(&format!("N7_WG_FEED|build-failed|reason={reason}"));
        false
    }
}

#[derive(Debug, Clone, Copy)]
enum FeedWhich {
    WgSocket,
    Tun,
}

impl WgPeerApplier for WgDeviceFeed {
    fn apply_peers(&self, peers: &[crate::connector::WgPeerEntry]) -> Result<(), String> {
        let mut g = self.inner.lock_poison();
        g.peers = peers.to_vec();
        match g.device.as_ref() {
            Some(d) => d.apply_peers(peers),
            None => Ok(()), // buffered: control plane stays alive pre-feed
        }
    }

    fn clear(&self) {
        let mut g = self.inner.lock_poison();
        // full stop semantics: buffers, endpoints, egress, carriers, fed fds
        // and the device all go away — after a connector stop the data plane
        // stays down until the shell re-feeds a fresh pair. N13-D2: the
        // relay carriers (and through them the relay lane handles) go with
        // it — `connector_stop()` leaves no reachable relay bearer behind.
        g.peers.clear();
        g.endpoints.clear();
        g.egress.clear();
        g.carriers.clear();
        g.wg_socket_raw = None;
        g.tun_raw = None;
        if let Some(d) = g.device.take() {
            d.clear();
        }
    }

    /// Real data-plane readiness once a device is up; always false before
    /// (fail-closed: no feeds ⇒ no handshakes ⇒ the N3-7 gate HOLDS the
    /// default route).
    fn tunnel_ready(&self) -> bool {
        let g = self.inner.lock_poison();
        g.device.as_ref().map(|d| d.tunnel_ready()).unwrap_or(false)
    }

    fn apply_endpoint(&self, pub_key_b64: &str, addr: [u8; 4], port: u16) -> Result<(), String> {
        let mut g = self.inner.lock_poison();
        match g.device.as_ref() {
            Some(d) => d.apply_endpoint(pub_key_b64, addr, port),
            None => {
                // buffer (latest per peer wins); unregistered peers are
                // rejected exactly like the registry / device seams
                if !g.peers.iter().any(|p| p.pub_key_b64 == pub_key_b64) {
                    return Err(format!("peer '{pub_key_b64}' is not registered"));
                }
                if let Some(slot) =
                    g.endpoints.iter_mut().find(|(k, _, _)| k == pub_key_b64)
                {
                    *slot = (pub_key_b64.to_string(), addr, port);
                } else {
                    g.endpoints.push((pub_key_b64.to_string(), addr, port));
                }
                Ok(())
            }
        }
    }

    /// N11: dup the selected socket into the live device now, or buffer the
    /// (dup-probe-validated) number for replay at build time — same
    /// borrowed-number discipline as the other feeds.
    fn attach_egress_socket(&self, pub_key_b64: &str, raw_fd: i32) -> Result<(), String> {
        if raw_fd < 0 {
            return Err("wg-egress-fd-missing".to_string());
        }
        let mut g = self.inner.lock_poison();
        if let Some(d) = g.device.as_ref() {
            return d.attach_egress_socket(pub_key_b64, raw_fd);
        }
        // pre-device: validate the fd is alive, then buffer (latest wins)
        match crate::mgmtsock::dup_socket_fd(raw_fd) {
            Ok(probe) => unsafe { sys::close(probe); }
            Err(e) => return Err(format!("wg-egress-fd-invalid (errno={})", e.errno())),
        }
        if !g.peers.iter().any(|p| p.pub_key_b64 == pub_key_b64) {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        }
        if let Some(slot) = g.egress.iter_mut().find(|(k, _)| k == pub_key_b64) {
            slot.1 = raw_fd;
        } else {
            g.egress.push((pub_key_b64.to_string(), raw_fd));
        }
        Ok(())
    }

    /// N11: demuxed inbound datagram → live device (source-matched); no
    /// device ⇒ nothing to feed (fail-closed no-op).
    fn handle_udp_inbound(&self, datagram: &[u8], src: ([u8; 4], u16), now_ms: u64) -> usize {
        let g = self.inner.lock_poison();
        match g.device.as_ref() {
            Some(d) => d.handle_udp_inbound(datagram, src, now_ms),
            None => 0,
        }
    }

    /// N11: recycle endpoint + egress on the live device; pre-device, drop
    /// the buffered endpoint/egress so a rebuild never resurrects the dead
    /// path (fail-closed). N13-D2: the carrier is deliberately KEPT — this
    /// is the ICE-lost → relay fallback transition.
    fn recycle_endpoint(&self, pub_key_b64: &str) {
        let mut g = self.inner.lock_poison();
        g.endpoints.retain(|(k, _, _)| k != pub_key_b64);
        g.egress.retain(|(k, _)| k != pub_key_b64);
        if let Some(d) = g.device.as_ref() {
            d.recycle_endpoint(pub_key_b64);
        }
    }

    /// N13-D2: inject the relay carrier into the live device now, or buffer
    /// it for replay at build time (same upsert semantics as the egress
    /// buffer; the handle is a pure Rust object — no fd discipline needed).
    fn attach_carrier(
        &self,
        pub_key_b64: &str,
        carrier: std::sync::Arc<dyn WgEgressCarrier>,
    ) -> Result<(), String> {
        let mut g = self.inner.lock_poison();
        if let Some(d) = g.device.as_ref() {
            return d.attach_carrier(pub_key_b64, carrier);
        }
        if !g.peers.iter().any(|p| p.pub_key_b64 == pub_key_b64) {
            return Err(format!("peer '{pub_key_b64}' is not registered"));
        }
        if let Some(slot) = g.carriers.iter_mut().find(|(k, _)| k == pub_key_b64) {
            slot.1 = carrier;
        } else {
            g.carriers.push((pub_key_b64.to_string(), carrier));
        }
        Ok(())
    }

    /// N13-D2: detach the relay carrier — live device AND any buffered
    /// slot, so a rebuild never resurrects a revoked lane (fail-closed).
    fn detach_carrier(&self, pub_key_b64: &str) {
        let mut g = self.inner.lock_poison();
        g.carriers.retain(|(k, _)| k != pub_key_b64);
        if let Some(d) = g.device.as_ref() {
            d.detach_carrier(pub_key_b64);
        }
    }

    /// N13-D2: detach every relay carrier — live device AND the buffer.
    fn detach_all_carriers(&self) {
        let mut g = self.inner.lock_poison();
        let had = !g.carriers.is_empty();
        g.carriers.clear();
        if let Some(d) = g.device.as_ref() {
            d.detach_all_carriers();
        }
        if had {
            emit("N13_WG_FEED|carriers-detach-all");
        }
    }

    /// N13-D2: the feed CAN carry relay lanes (buffered pre-device, live
    /// after adoption).
    fn carrier_capable(&self) -> bool {
        true
    }

    /// N13-D2: relay-arrived datagram → live device (key-matched); no
    /// device ⇒ nothing to feed (fail-closed no-op).
    fn handle_carrier_inbound(&self, datagram: &[u8], pub_key_b64: &str, now_ms: u64) -> usize {
        let g = self.inner.lock_poison();
        match g.device.as_ref() {
            Some(d) => d.handle_carrier_inbound(datagram, pub_key_b64, now_ms),
            None => 0,
        }
    }

    /// The `connector_status()` `wg` field: feed/device state + REAL device
    /// counters. The not-up state reports fed flags honestly (so the shell
    /// can tell "waiting for feeds" from "running").
    fn dataplane_status(&self) -> Option<WgDataplaneStatus> {
        let g = self.inner.lock_poison();
        let mut st = WgDataplaneStatus {
            fed_socket: g.wg_socket_raw.is_some(),
            fed_tun: g.tun_raw.is_some(),
            device_up: g.device.is_some(),
            ..WgDataplaneStatus::default()
        };
        if let Some(d) = g.device.as_ref() {
            let (ready, sessions, stats) = d.with_device(|dev| {
                let sessions = dev
                    .peers()
                    .iter()
                    .filter(|p| p.session_established && !p.expired)
                    .count();
                (dev.tunnel_ready(), sessions, dev.stats())
            });
            st.ready = ready;
            st.peers_with_session = sessions;
            st.handshakes = stats.handshake_initiations;
            st.tx_packets = stats.tx_packets;
            st.tx_bytes = stats.tx_bytes;
            st.rx_packets = stats.rx_packets;
            st.rx_bytes_to_tun = stats.rx_bytes_to_tun;
            st.dropped_no_route = stats.no_route_drops;
            st.decrypt_errors = stats.decrypt_errors;
            st.carrier_tx_packets = stats.carrier_tx_packets;
            st.carrier_tx_bytes = stats.carrier_tx_bytes;
            st.carrier_rejects = stats.carrier_rejects;
            st.carrier_takeovers = stats.carrier_takeovers;
            st.direct_restores = stats.direct_restores;
        }
        Some(st)
    }
}

// ---------------------------------------------------------------------------
// tests (feed slot: buffering, replay, fail-closed refusals, clear)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod feed_tests {
    use super::*;
    use crate::config;

    extern "C" {
        fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    }

    fn test_secret_b64(byte: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode([byte; 32])
    }

    fn peer_entry(key: &str, addr: [u8; 4]) -> crate::connector::WgPeerEntry {
        crate::connector::WgPeerEntry {
            pub_key_b64: key.to_string(),
            allowed_ips: vec![config::Route { addr, prefix_len: 32 }],
        }
    }

    /// Bound loopback UDP socket (the shell-protected WG outer socket stand-
    /// in) + a datagram socketpair playing the TUN half. Returns all raw fds
    /// the caller must close.
    fn fed_fds() -> (i32, i32, i32) {
        let wg = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(wg >= 0);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(
            unsafe { sys::bind(wg, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0
        );
        let mut sv = [-1i32; 2];
        assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);
        (wg, sv[0], sv[1])
    }

    fn test_slot() -> (WgDeviceFeed, i32, i32, i32) {
        let (wg_raw, tun_raw, tun_test_end) = fed_fds();
        let slot = WgDeviceFeed::new(WgDeviceConfig::new(test_secret_b64(0xA5)));
        (slot, wg_raw, tun_raw, tun_test_end)
    }

    #[test]
    fn feed_slot_buffers_until_both_fds_then_builds_and_replays() {
        let (slot, wg_raw, tun_raw, _tun_end) = test_slot();
        let key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);

        // pre-feed: control-plane calls buffer, data plane stays down
        slot.apply_peers(&[peer_entry(&key, [10, 7, 0, 2])]).expect("buffered peers");
        slot.apply_endpoint(&key, [127, 0, 0, 1], 51820).expect("buffered endpoint");
        assert!(!slot.device_up());
        assert!(!slot.tunnel_ready(), "no feeds ⇒ never ready (fail-closed)");
        let st = slot.dataplane_status().expect("feed seam always reports");
        assert!(!st.fed_socket && !st.fed_tun && !st.device_up && !st.ready);
        assert_eq!(st.to_json(), "{\"fed_socket\":false,\"fed_tun\":false,\"device_up\":false,\
             \"ready\":false,\"peers_with_session\":0,\"handshakes\":0,\"tx_packets\":0,\
             \"tx_bytes\":0,\"rx_packets\":0,\"rx_bytes_to_tun\":0,\
             \"dropped_no_route\":0,\"decrypt_errors\":0,\"carrier_tx_packets\":0,\
             \"carrier_tx_bytes\":0,\"carrier_rejects\":0,\"carrier_takeovers\":0,\
             \"direct_restores\":0}");

        // first feed alone: still not up
        slot.feed_tun(tun_raw).expect("tun feed");
        assert!(!slot.device_up());
        let st = slot.dataplane_status().unwrap();
        assert!(st.fed_tun && !st.fed_socket && !st.device_up);

        // second feed completes the pair: device up, buffered state replayed
        slot.feed_wg_socket(wg_raw).expect("wg socket feed");
        assert!(slot.device_up());
        let st = slot.dataplane_status().unwrap();
        assert!(st.fed_socket && st.fed_tun && st.device_up);
        assert!(!st.ready, "no handshake yet");
        slot.with_device(|d| {
            assert_eq!(d.peer_count(), 1, "buffered peer set replayed");
            let peer = &d.peers()[0];
            assert_eq!(peer.pub_key_b64, key);
            assert_eq!(peer.endpoint, Some(([127, 0, 0, 1], 51820)), "endpoint replayed");
        })
        .expect("device access");

        // post-build control-plane calls land directly on the device
        slot.apply_endpoint(&key, [127, 0, 0, 1], 51821).expect("live landing");
        slot.with_device(|d| {
            assert_eq!(d.peers()[0].endpoint, Some(([127, 0, 0, 1], 51821)));
        })
        .expect("device access");

        // cleanup: the test owns the raws (device holds only dups)
        for fd in [wg_raw, tun_raw, _tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    #[test]
    fn feed_slot_refuses_missing_and_dead_fds_fail_closed() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        // missing
        assert_eq!(slot.feed_wg_socket(-1).unwrap_err().token(), "socket-fd-missing");
        assert_eq!(slot.feed_tun(-1).unwrap_err().token(), "socket-fd-missing");
        // dead: a fd number that CANNOT be open — far beyond any RLIMIT_NOFILE
        // (Linux caps per-process fd numbers far below 2^30), so the boundary
        // dup/F_GETFD probe returns EBADF deterministically and NO parallel
        // test can ever hold this number. Deliberately NOT "open one and
        // close it": fd numbers are handed out lowest-free-first from the
        // process-global table, so under parallel test execution another
        // test's socket()/dup() can re-open the just-closed number before
        // the feed lands — the expected refusal turns into Ok(()) and the
        // test flakes (observed on ~25% of lib-target runs).
        const DEAD_FD: i32 = 1 << 30;
        let err = slot.feed_tun(DEAD_FD).unwrap_err();
        assert_eq!(err.token(), "socket-fd-invalid");
        assert_eq!(err.errno(), sys::EBADF);
        // nothing was stored: the seam stays completely unfed
        let st = slot.dataplane_status().unwrap();
        assert!(!st.fed_socket && !st.fed_tun && !st.device_up);
        assert!(!slot.tunnel_ready());
        // and with only the socket fed, the data plane never starts
        slot.feed_wg_socket(wg_raw).expect("valid socket feed");
        assert!(!slot.device_up(), "one feed alone must not start the data plane");
        assert!(!slot.tunnel_ready());
        // tun_raw stayed open this round (the DEAD_FD probe replaced the
        // close-the-tun-end trick), so it is ours to close here
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    #[test]
    fn feed_slot_unregistered_endpoint_rejected_like_registry() {
        let (slot, _wg_raw, _tun_raw, _tun_end) = test_slot();
        let unknown = base64::engine::general_purpose::STANDARD.encode([9u8; 32]);
        let err = slot.apply_endpoint(&unknown, [127, 0, 0, 1], 1).unwrap_err();
        assert!(err.contains("not registered"), "{err}");
    }

    #[test]
    fn feed_slot_clear_tears_device_and_feeds_down() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let key = base64::engine::general_purpose::STANDARD.encode([7u8; 32]);
        slot.apply_peers(&[peer_entry(&key, [10, 7, 0, 2])]).expect("peers");
        slot.feed_tun(tun_raw).expect("tun");
        slot.feed_wg_socket(wg_raw).expect("wg");
        assert!(slot.device_up());
        slot.clear();
        assert!(!slot.device_up(), "clear() must tear the device down");
        assert!(!slot.tunnel_ready());
        let st = slot.dataplane_status().unwrap();
        assert!(!st.fed_socket && !st.fed_tun, "clear() must drop fed fds");
        // re-feeding a fresh pair rebuilds cleanly (buffered peers were
        // cleared too, so the map must re-sync first — exact N7 semantics)
        let (wg2, tun2, tun2_end) = fed_fds();
        slot.feed_tun(tun2).expect("re-feed tun");
        slot.feed_wg_socket(wg2).expect("re-feed wg");
        assert!(slot.device_up());
        slot.with_device(|d| assert_eq!(d.peer_count(), 0, "cleared peers stay cleared"))
            .expect("device access");
        for fd in [wg_raw, tun_end, wg2, tun2_end] {
            unsafe { sys::close(fd) };
        }
    }
}
