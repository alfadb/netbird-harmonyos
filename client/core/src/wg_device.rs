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
//! - **管理端下发网段（managed route overlay）**：map `routes` 里的网段
//!   （如 `192.168.50.0/24`）不进任何 peer 的 map allowed_ips——没有这一
//!   层，去往该网段的包一律 `no-route` 丢弃（真机缺陷 2026-10-04）。
//!   [`WgDevice::set_managed_routes`] 把每个网段指派给一个路由 peer 并
//!   并入其 allowed_ips；HA（同网段多个路由 peer）按「活会话优先 →
//!   metric 小者优先 → 公钥稳定序」选择，选中 peer 会话失效即切换，
//!   每次变更一行 `N6_WG_DEVICE|route-peer|…`。系统侧路由（TUN 里进什么）
//!   仍由壳层按 managed 路由安装，本层不改。
//! - **入向**：UDP 收包 → 按**报文自身**定位 peer（上游同款：transport 的
//!   receiving index / 握手包的静态密钥解密），来源地址只用来**优先**试
//!   已登记在该地址上的 peer；一个 peer 的 tunnel 都不能消费该包 → 丢弃并
//!   计 `unknown_peer_drops` → 该 peer 的 tunnel 解封装 → 明文写 TUN（dup
//!   副本，[`TunFd`] 合同）。解密失败（`OP_ERROR`）计 `decrypt_errors` 并
//!   丢弃。**标准 WireGuard roaming**：包被**认证消费**后，其来源地址成为
//!   该 peer 的新 endpoint（上游 `Peer.SetEndpointFromPacket`，
//!   `device/receive.go:374`/`:401`/`:459`/`:515`；NetBird 从不关闭
//!   roaming——`DisableSomeRoamingForBrokenMobileSemantics` 在 iOS/Android
//!   两条设备路径上都是**注释掉**的，`device_ios.go:92-94` /
//!   `device_android.go:91-93`）。未认证的包（坏 MAC1 / 静态密钥不符 /
//!   AEAD 失败 / 计数器重放 / 未知 index / 报文形态不符）**永不**改动任何
//!   状态；被消费的 cookie reply 同样不改 endpoint
//!   （`device/receive.go:284-320`）。roaming 不触碰 relay 载体：载体入向
//!   按身份（公钥）匹配、不带地址，见 [`WgDevice::handle_carrier`]。
//! - **握手**：endpoint 落配（或变更）即发起（`wireguard_force_handshake`，
//!   对应上游 `ConfigureWGEndpoint` 后的握手驱动
//!   `client/internal/peer/conn.go:444-478`）；无会话时的出向数据也会触发
//!   （boringtun 语义，见上）。重传由设备层在**注入时钟**上执行：无会话且
//!   距上次发起 ≥ `hs_retry_ms` → 重新发起（每次新 ephemeral，同
//!   boringtun `update_timers` 的 REKEY_TIMEOUT 重传语义
//!   `noise/timers.rs:13`）；自首次发起 ≥ `hs_deadline_ms` 仍无会话 →
//!   放弃该轮（REKEY_ATTEMPT_TIME，`noise/timers.rs:12`），endpoint 再变
//!   更时重开。**会话过期同样重新武装战役**（过期后 boringtun 可能仍装载
//!   旧密钥，条件不能只看 `session_established`）——见 `tunnel_ready()`
//!   语义节的「过期不是终态」。
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
//! **过期不是终态（2026-10-08 真机缺陷）**：`expired` 只表示「本端策略认为
//! 密钥窗已失效」，boringtun 自己可能仍持有旧会话，对端也可能随时发起新
//! 握手。设备对过期的处理是：
//!
//! 1. `tick` 在判过期的**同一 tick** 立即重新武装握手战役（`has_path` 仍在
//!    即可主动发起 `force_handshake`，不必等对端）；否则到期后
//!    `session_established` 仍为 true、战役条件永不成立，本端只会一直
//!    0/6 卡死（缺陷本体）。
//! 2. 只有**握手完成时刻严格晚于过期时刻**的新握手才算重新建立。
//!    boringtun `stats()` 的 `time_since_last_handshake` 单位是**秒且在整秒
//!    处截断**（`ffi/mod.rs:390`），它是「距**上次**握手的年龄」：到期后任何
//!    入站包都会读到 `hs >= 0`（旧握手的年龄，随时间继续增大），据此判恢复
//!    必然抖动。设备用 `now_ms − hs × 1000` 反推握手完成时刻，与过期时刻
//!    `session_established_ms + session_max_ms` 严格比较（截断使反推值**不早
//!    于**真实完成时刻，误差 < 1 s）；旧握手的反推值恒在 `[t₀, t₀ + 999 ms]`，
//!    与过期时刻差整整一个 `session_max_ms`（默认 540 s），绝不会被误判。
//!    判据见 [`handshake_after_expiry`]。
//! 3. 重新建立走**与首次建立完全相同的一套动作**（清 `expired`、刷新锚点、
//!    清战役、`session-established` 行、overlay 以 `session-established` 重建、
//!    冲刷排队包），所以「到期 → 恢复」后的状态投影（`wg_sessions` /
//!    `online` / `tunnel_ready` / 状态 JSON 的 `session_established`/`expired`）
//!    与首次建立后完全一致。未到期的正常 rekey（收到 fresh 握手）仍只刷新
//!    锚点，不 emit、不重建。
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
//!   新 dup 与首次 attach 走**同一个** `TunFd::dup_from_raw`，强制
//!   O_NONBLOCK（roam-1005-0014：重建代 fd 到手是阻塞的，泵在此后冻结，
//!   wgRxToTun=0/wgTxBytes=0 直到手动重连）。
//! - **会话保留是零成本的**：BoringTun tunnel 是纯字节层状态机，从不引用
//!   TUN fd；WG 外层 UDP socket（`self.fd`）在重建中不变。因此换 TUN 不
//!   触碰任何握手/密钥状态——peer 会话原样存活，重建后载荷立即双向可通，
//!   不需要重新握手。
//! - feed seam：设备在位时 `feed_tun` = 替换（[`WgDeviceFeed::feed_tun`]）；
//!   `feed_wg_socket` 同号为幂等 no-op、异号在新 fd 验证成功后替换外层 socket；
//!   失败时旧 socket 不变。

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
//! - **入向匹配映射**（`handle_udp` 的身份域的等价物）：UDP 域里身份主要
//!   由报文自带（receiving index / 握手静态解密），来源地址只决定优先顺序
//!   并被 roaming 采纳；relay 域里没有 UDP 五元组，身份由帧自带——
//!   Transport 帧的 36B 字段是**发送方 peer id**（§4.2，服务端改写），
//!   而 peer id = `PeerId::from_wg_pubkey_string(对端 WG 公钥 base64)`
//!   （relay spec §3.2）。编排层（connector 的 `RelayCarrier`）持有
//!   「注册 peer 公钥 → peer id」反查表，把 sender id 反查回 WG 公钥后调
//!   [`WgDevice::handle_carrier`]，按 `key_b64` 定位 peer——与
//!   `handle_udp` 共用同一段收包体（解封装 → TUN / 回包 / 会话建立观察）。
//!   **roaming 不碰载体**：上游 relay 入向把包连同**对端自己的 synthetic
//!   endpoint** 一起交给 wireguard-go（`wgproxy/bind/proxy.go:199`
//!   `ReceiveFromEndpoint(ctx, p.wgCurrentUsed, …)` → `ice_bind.go:378`
//!   `eps[0] = msg.Endpoint`），`SetEndpointFromPacket` 于是把它原样写回、
//!   等于 no-op；本层的载体入向根本没有地址可采纳，语义只会更严（载体报文
//!   无论认证与否都不会移动 endpoint，也不会把直连路径换成 relay）。
//!   选择「公钥匹配」而非上游的假地址注册：假地址需要一个额外的状态位区分
//!   真假 endpoint，公钥匹配则把 relay 帧的既有身份字段直接用足。
//! - **MTU / 封装开销**（登记，spec §7.2）：Transport 帧 = 2B 头 + 36B id
//!   ⇒ 38B 固定开销，再套 WS 客户端帧（2–4B 头 + 4B mask）+ TLS/TCP/IP
//!   ⇒ 典型每包 ≈ 44–46B。`send_to_peer` 单包上限 **8782B**
//!   （8820 − 38）；设备侧报文物理上限 [`WG_BUF`] = 2048B，恒在限内——
//!   超限载荷只能在载体句柄处出现，且被 `RelayClientError::FrameTooLarge`
//!   类型化拒绝（见 `relay_client` 模块文档）。对 MTU 的影响：本客户端默认
//!   1280（上游 `iface.DefaultMTU`，`client/iface/iface.go:29`），即便管理面
//!   通告的上限 1500 也安全（1500 封装后 ≈ 1546B ≪ 8782）。
//! - **fd 合同（N13-D2 / N13 plan §4 重验项）**：relay 承载**不持有、不
//!   close 任何平台 fd**——[`WgEgressCarrier`] 是纯 Rust 对象（relay
//!   client 句柄），relay 的 WSS/TCP socket 是 native 自建（同 management
//!   通道，不经 `VpnConnection.protect`），其生命周期只随 relay client 的
//!   会话；平台 TUN 原始 fd 的关闭权**仅属**壳侧 `VpnConnection.destroy()`
//!   ——本模块（含载体路径）只经 [`TunFd`]/dup 副本接触 fd，载体挂载/
//!   拆除/`connector_stop()` 都不会触碰那个号码。

use std::net::{Ipv4Addr, SocketAddr};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use base64::Engine as _;

use crate::connector::{WgPeerApplier, WgPeerEntry};
use crate::hilog::{emit, emit_evidence};
use crate::sys;
use crate::tun::TunFd;
use crate::wg::{Tunnel, OP_DONE, OP_ERROR, OP_NETWORK, OP_TUN_V4, OP_TUN_V6};

/// One WG datagram / one TUN frame working buffer (probe BUF parity; >= the
/// default MTU 1280 — upstream `iface.DefaultMTU` — + transport overhead).
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
/// (same shape as the probe's FWD_TUN_DRAIN). Raised 8 → 64 together with the
/// 100 ms → 10 ms pump tick: the old pair capped the TUN direction at
/// ~80 pps (≈0.8 Mbps at the default 1280 MTU) because a non-blocking drain
/// read at most 8 frames per 100 ms cycle regardless of how much was queued.
const DRAIN_MAX: usize = 64;
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
    /// Inbound UDP datagrams ATTRIBUTED to a peer: the source address equals
    /// a registered peer endpoint (the pre-roaming rule, and the only rule for
    /// a datagram no tunnel can consume), OR the datagram identified its peer
    /// by itself and that peer's crypto consumed it (roaming discovery —
    /// upstream identifies the peer from the datagram, `receive.go`). Counted
    /// BEFORE decapsulation, so a source-matched datagram that fails its AEAD
    /// is in here as well (and in `decrypt_errors`).
    pub rx_packets: u64,
    /// Plaintext bytes written into the TUN.
    pub rx_bytes_to_tun: u64,
    /// Handshake initiations sent (initial + retransmits + rekeys).
    pub handshake_initiations: u64,
    /// Keepalive transport packets sent (empty payload).
    pub keepalives_sent: u64,
    /// Outbound frames dropped: no allowed_ips prefix matched the dst.
    pub no_route_drops: u64,
    /// Inbound datagrams DROPPED: no peer was registered at the source
    /// address AND no peer's tunnel could consume the datagram (a WireGuard
    /// message from an address we have not seen is not a drop any more — its
    /// peer authenticates it and the source is adopted as the new endpoint,
    /// standard WireGuard roaming). A datagram that merely failed to decrypt
    /// on a source-matched peer is `decrypt_errors`, not this.
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
    // -- N13-D4 per-peer transfer counters (appended to the JSON AFTER the
    //    N13-D1/D2 peer fields; consumers read the head positionally). UNIT:
    //    PLAINTEXT (inner-IP) bytes, both directions — see each field. --
    /// Plaintext (inner-IP) bytes of this peer's data frames that were
    /// encapsulated AND handed to a WORKING bearer (UDP direct or the relay
    /// carrier), cumulative since the peer's tunnel was created. Handshake and
    /// keepalive packets carry no plaintext and contribute nothing; a datagram
    /// the bearer REFUSED (`sendto` failure / typed carrier refusal) is
    /// excluded — the unit is "transferred", not "encrypted". Frames queued
    /// behind a handshake are counted when `flush_queued` really sends them.
    /// NOT the device-level `tx_bytes`, which counts ENCAPSULATED wire bytes.
    pub tx_bytes: u64,
    /// Plaintext (inner-IP) bytes decapsulated from this peer and written into
    /// the TUN — the same success branch and unit as the device-level
    /// [`WgDeviceStats::rx_bytes_to_tun`] (a frame the TUN refused counts in
    /// neither). While no peer has been replaced or removed since the device
    /// came up, the registered peers' `rx_bytes` sum to that device counter;
    /// a replaced peer restarts at zero (the device counter keeps running).
    pub rx_bytes: u64,
}

/// One `handle_udp` outcome (observation surface for tests/pumps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WgInbound {
    /// A peer took the datagram: its source matched a registered peer
    /// endpoint, or the datagram identified its peer itself and that peer's
    /// crypto consumed it (roaming discovery).
    pub peer_matched: bool,
    /// Decapsulated plaintext bytes written to the TUN (0 = none).
    pub wrote_tun: usize,
    /// Response datagrams produced and sent (handshake response / flush).
    pub sent: usize,
}

// ---------------------------------------------------------------------------
// I2 (N2-H): TUN frame tee — the device collector's frame-observation seam
// ---------------------------------------------------------------------------

/// Direction of one TUN frame crossing a [`WgDevice`] (the tee's two feeds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunFrameDir {
    /// from-os (outbound): read off the TUN and fed BEFORE `send_from_tun`'s
    /// route decision — an unroutable frame (`no_route_drops`) is still
    /// observed, because the frame content is what the scan judges.
    FromOs,
    /// to-os (inbound delivery): decapsulated plaintext, fed on the SAME
    /// success branch that increments `rx_bytes_to_tun` (a frame the TUN
    /// refused is NOT delivered, hence not observed).
    ToOs,
}

/// I2 — the TUN frame tee seam (N2-H §4.1). A [`WgDevice`] hands EVERY frame
/// it reads from or writes to the TUN to the attached tee, in both
/// directions; the collector's scan (marker match + five-tuple extraction)
/// lives behind this seam.
///
/// Contract:
/// - called on the data-plane pump thread with the frame BORROWED: keep the
///   critical section short (copy + scan) and never block the pump;
/// - must not panic: the tee is diagnostic, the data plane keeps running;
/// - `Send + Sync` (same injection shape as [`WgEgressCarrier`]): the handle
///   is attached/detached through `&mut WgDevice` while the pump may hold the
///   device lock;
/// - attached = observation window OPEN: frames before `attach_tun_tee` and
///   after `detach_tun_tee` are not observed (`frames_scanned` covers the
///   collector window only).
///
/// The production implementation is the collector's scan tee (I5,
/// `n2h_collector.rs`: registered markers + `WireMarker::matches` + IPv4
/// five-tuple parse, carrying its own lock) — it plugs in here without
/// re-touching the data plane.
pub trait TunFrameTee: Send + Sync + 'static {
    /// Observe one frame: `dir` is the crossing direction, `frame` the exact
    /// bytes the device read from / wrote to the TUN.
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]);
}

/// I2 — minimal counting tee: this seam's per-direction frame/byte totals.
/// Stands in for the collector's scan tee until I5 merges the scanner into it
/// (`n2h_collector.rs`); the I2 test asserts frame accounting against
/// [`WgDeviceStats`] through exactly this shape.
#[derive(Debug, Default)]
pub struct TunTeeCounts {
    inner: Mutex<TunTeeTotals>,
}

/// One [`TunTeeCounts`] snapshot (copied out; the caller holds no lock).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TunTeeTotals {
    /// from-os frames observed and their byte total.
    pub from_os_frames: u64,
    pub from_os_bytes: u64,
    /// to-os frames observed and their byte total.
    pub to_os_frames: u64,
    pub to_os_bytes: u64,
}

impl TunTeeCounts {
    pub fn new() -> Self {
        TunTeeCounts::default()
    }

    /// Current totals (poison-tolerant: the same convention as the connector
    /// seam — a poisoned lock must not take the data plane down).
    pub fn totals(&self) -> TunTeeTotals {
        *self.inner.lock_poison()
    }

    /// Frames observed in BOTH directions (the §4.1 `frames_scanned` feed).
    pub fn frames_scanned(&self) -> u64 {
        let t = self.totals();
        t.from_os_frames + t.to_os_frames
    }
}

impl TunFrameTee for TunTeeCounts {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        let mut t = self.inner.lock_poison();
        match dir {
            TunFrameDir::FromOs => {
                t.from_os_frames += 1;
                t.from_os_bytes += frame.len() as u64;
            }
            TunFrameDir::ToOs => {
                t.to_os_frames += 1;
                t.to_os_bytes += frame.len() as u64;
            }
        }
    }
}

/// Diagnostic-only, session-wide from-OS destination counts, independent of
/// the collector's short-lived marker scanner.
pub struct N2DestinationTee {
    inner: Mutex<N2DestinationState>,
    control: Mutex<crate::n2_control::ControlState>,
}

struct N2DestinationState {
    targets: Vec<[u8; 4]>,
    rows: Vec<N2DestinationRow>,
    last_tick_ms: u64,
    last_emit_ms: u64,
    /// N2 T7a (item 3) — tee continuity counters, for the session-period
    /// `N2_TUN_TEE_STATS` line (`wg_device.rs`): every from-OS frame handed to
    /// this tee, the ones whose IPv4 header did not decode, and the ones that
    /// decoded but were dropped by the frozen capture set. The invariant is
    /// `tee_frames == decode_fail + dropped + sum(rows[].frames)`.
    tee_frames: u64,
    decode_fail: u64,
    dropped: u64,
    /// diag-0048 (2026-09-30): the FIRST `decode_fail` frame's witness (length
    /// + at most the first 4 header bytes, never payload), plus the refused
    /// frames bucketed by byte 0's IP-version nibble. Diagnostic only:
    /// `decode_fail` keeps its frozen meaning and the invariant above is
    /// unchanged.
    first_fail: DecodeFailWitness,
    /// R1 (approved n2-tun-ipv6-decode-amendment, 2026-10-01): the refused
    /// version-6 frames classified into the explicitly out-of-scope link-scoped
    /// control frames and everything else, with their scope / chain-tail
    /// histograms and header-only witnesses. Diagnostic only: `decode_fail`
    /// keeps its frozen meaning and the invariant above is unchanged.
    ipv6: Ipv6FailWitness,
}

/// diag-0048 — the first-decode-failure witness and the per-version failure
/// buckets (`N2_TUN_TEE_STATS` tail fields; see `stats_line`).
#[derive(Debug, Default, Clone, Copy)]
struct DecodeFailWitness {
    /// `None` until the first refusal (`len=0` is a real, recorded failure).
    len: Option<usize>,
    /// Leading bytes of that first refusal, at most `prefix.len()` of them.
    prefix: [u8; 4],
    prefix_len: usize,
    fail_v4: u64,
    fail_v6: u64,
    fail_other: u64,
}

impl DecodeFailWitness {
    /// Record one refused frame: bucket it by byte 0's IP-version nibble, and
    /// keep the FIRST frame's length + leading header bytes as the witness.
    fn record(&mut self, frame: &[u8]) {
        match frame.first().map(|byte| byte >> 4) {
            Some(4) => self.fail_v4 = self.fail_v4.saturating_add(1),
            Some(6) => self.fail_v6 = self.fail_v6.saturating_add(1),
            _ => self.fail_other = self.fail_other.saturating_add(1),
        }
        if self.len.is_none() {
            let n = frame.len().min(self.prefix.len());
            self.len = Some(frame.len());
            self.prefix_len = n;
            self.prefix[..n].copy_from_slice(&frame[..n]);
        }
    }

    /// `first_fail_prefix`'s value: lowercase hex of the recorded bytes (empty
    /// before the first failure, or for a zero-length frame).
    fn prefix_hex(&self) -> String {
        self.prefix[..self.prefix_len].iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// `first_fail_byte0`'s value: the first recorded byte in hex, or empty
    /// when the first failure carried no byte at all.
    fn byte0_hex(&self) -> String {
        match self.prefix_len {
            0 => String::new(),
            _ => format!("0x{:02x}", self.prefix[0]),
        }
    }
}

/// R1 (approved n2-tun-ipv6-decode-amendment, 2026-10-01) — the refused
/// version-6 frames. `parse_ipv4_frame` refuses every non-IPv4 frame, IPv6
/// included, so this witness makes the refusal attributable WITHOUT widening
/// any scope: the kernel's own link-scoped control frames (`out_of_scope`) are
/// registered as out of scope, every other version-6 frame (`other`) stays a
/// named defect. Header metadata only, never payload.
///
/// `fail_v6 == out_of_scope + other` and every version-6 refusal lands in
/// exactly one `ipv6_scope` and one `ipv6_next` bucket, so the R2(i) four-sum
/// reconciliation `Σipv6_scope == Σipv6_next == fail_v6 ==
/// out_of_scope + other` holds by construction.
#[derive(Debug, Default, Clone, Copy)]
struct Ipv6FailWitness {
    out_of_scope: u64,
    other: u64,
    scope_linklocal: u64,
    scope_multicast: u64,
    scope_other: u64,
    next_icmp6: u64,
    next_hbh: u64,
    next_udp: u64,
    next_tcp: u64,
    next_other: u64,
    /// `first_v6_len` / `first_v6_dst_prefix`: the FIRST refused version-6 frame
    /// (empty before one; the prefix is at most the destination's first 8
    /// bytes, and only the bytes the frame actually carries).
    first_len: Option<usize>,
    first_dst_prefix: [u8; 8],
    first_dst_prefix_len: usize,
    /// `first_v6_other_*` (R4 observability): the FIRST refused version-6 frame
    /// that is NOT control class — full 16-byte destination (only the bytes the
    /// frame actually carries), chain-tail protocol number and, for a UDP/TCP
    /// tail, the destination port. All four are empty until such a frame.
    first_other_len: Option<usize>,
    first_other_dst: [u8; 16],
    first_other_dst_len: usize,
    /// Chain-tail protocol number; `None` when the chain was not walked through
    /// (printed as `-`).
    first_other_next: Option<u8>,
    /// Destination port of a UDP(17)/TCP(6) chain tail; `None` otherwise
    /// (printed as `-`).
    first_other_dport: Option<u16>,
}

impl Ipv6FailWitness {
    /// Classify one refused frame. Only version-6 frames (byte 0's high nibble)
    /// are touched; every other refusal returns at once. A version-6 frame too
    /// short to carry the 40-byte header has no destination and no walkable
    /// chain, so it is `other` with an `other` scope and an `hbh` bucket — it
    /// still counts in `fail_v6`, so the four sums keep reconciling.
    fn record(&mut self, frame: &[u8]) {
        if frame.first().map(|byte| byte >> 4) != Some(6) {
            return;
        }
        let view = crate::n2h::parse_ipv6_frame(frame);
        let scope = view.as_ref().map(|v| v.scope()).unwrap_or(crate::n2h::Ipv6Scope::Other);
        let next = view.as_ref().map(|v| v.next_bucket()).unwrap_or(crate::n2h::Ipv6Next::Hbh);
        match scope {
            crate::n2h::Ipv6Scope::LinkLocal => self.scope_linklocal = self.scope_linklocal.saturating_add(1),
            crate::n2h::Ipv6Scope::Multicast => self.scope_multicast = self.scope_multicast.saturating_add(1),
            crate::n2h::Ipv6Scope::Other => self.scope_other = self.scope_other.saturating_add(1),
        }
        match next {
            crate::n2h::Ipv6Next::Icmp6 => self.next_icmp6 = self.next_icmp6.saturating_add(1),
            crate::n2h::Ipv6Next::Hbh => self.next_hbh = self.next_hbh.saturating_add(1),
            crate::n2h::Ipv6Next::Udp => self.next_udp = self.next_udp.saturating_add(1),
            crate::n2h::Ipv6Next::Tcp => self.next_tcp = self.next_tcp.saturating_add(1),
            crate::n2h::Ipv6Next::Other => self.next_other = self.next_other.saturating_add(1),
        }
        let control = view.as_ref().map(|v| v.is_control_class()).unwrap_or(false);
        if control {
            self.out_of_scope = self.out_of_scope.saturating_add(1);
        } else {
            self.other = self.other.saturating_add(1);
        }
        if self.first_len.is_none() {
            self.first_len = Some(frame.len());
            let n = header_bytes(frame, 24, &mut self.first_dst_prefix);
            self.first_dst_prefix_len = n;
        }
        if self.first_other_len.is_none() && !control {
            self.first_other_len = Some(frame.len());
            let n = header_bytes(frame, 24, &mut self.first_other_dst);
            self.first_other_dst_len = n;
            self.first_other_next = view.as_ref().and_then(|v| v.chain_tail);
            self.first_other_dport = view.as_ref().and_then(|v| v.dport);
        }
    }

    /// `first_v6_dst_prefix`: lowercase hex of the first recorded destination
    /// bytes (empty before the first version-6 refusal).
    fn first_dst_prefix_hex(&self) -> String {
        hex_bytes(&self.first_dst_prefix[..self.first_dst_prefix_len])
    }

    /// The four `first_v6_other_*` values, all empty until a non-control
    /// version-6 frame was refused.
    fn first_other_fields(&self) -> (String, String, String, String) {
        match self.first_other_len {
            None => (String::new(), String::new(), String::new(), String::new()),
            Some(len) => (
                len.to_string(),
                hex_bytes(&self.first_other_dst[..self.first_other_dst_len]),
                self.first_other_next.map(|proto| proto.to_string()).unwrap_or_else(|| "-".into()),
                self.first_other_dport.map(|port| port.to_string()).unwrap_or_else(|| "-".into()),
            ),
        }
    }

    /// `ipv6_scope=linklocal:…,multicast:…,other:…`
    fn scope_histogram(&self) -> String {
        use crate::n2h::Ipv6Scope;
        format!("{}:{},{}:{},{}:{}",
            Ipv6Scope::LinkLocal.name(), self.scope_linklocal,
            Ipv6Scope::Multicast.name(), self.scope_multicast,
            Ipv6Scope::Other.name(), self.scope_other)
    }

    /// `ipv6_next=icmp6:…,hbh:…,udp:…,tcp:…,other:…`
    fn next_histogram(&self) -> String {
        use crate::n2h::Ipv6Next;
        format!("{}:{},{}:{},{}:{},{}:{},{}:{}",
            Ipv6Next::Icmp6.name(), self.next_icmp6,
            Ipv6Next::Hbh.name(), self.next_hbh,
            Ipv6Next::Udp.name(), self.next_udp,
            Ipv6Next::Tcp.name(), self.next_tcp,
            Ipv6Next::Other.name(), self.next_other)
    }
}

/// Copy at most `out.len()` of the `offset`-prefixed header bytes the frame
/// actually carries (a shorter frame yields fewer bytes, never a panic).
fn header_bytes(frame: &[u8], offset: usize, out: &mut [u8]) -> usize {
    if frame.len() <= offset {
        return 0;
    }
    let n = (frame.len() - offset).min(out.len());
    out[..n].copy_from_slice(&frame[offset..offset + n]);
    n
}

/// Lowercase hex of `bytes` (empty for an empty slice).
fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

struct N2DestinationRow {
    dst: [u8; 4],
    proto: u8,
    port: u16,
    frames: u64,
    reported: u64,
    sources: Vec<[u8; 4]>,
}

impl N2DestinationTee {
    pub fn new(targets: Vec<[u8; 4]>, now_ms: u64) -> Self {
        Self {
            control: Mutex::new(crate::n2_control::ControlState::new(targets.clone())),
            inner: Mutex::new(N2DestinationState {
                targets, rows: Vec::new(), last_tick_ms: now_ms, last_emit_ms: now_ms,
                tee_frames: 0, decode_fail: 0, dropped: 0,
                first_fail: DecodeFailWitness::default(),
                ipv6: Ipv6FailWitness::default(),
            }),
        }
    }

    pub fn configure_outer(&self, targets: Vec<([u8; 4], u8, u16)>) {
        let count = targets.len();
        self.control.lock_poison().set_outer(targets);
        emit(&format!("N2_TUN_OUTER_SET|count={count}|frozen={}|diag=true|is_evidence=false", count != 0));
    }

    pub fn leak_count(&self) -> u64 { self.control.lock_poison().leaks() }

    pub fn outer_count(&self) -> usize { self.control.lock_poison().outer_count() }

    pub fn register_control(&self, id: &str, dst: [u8; 4], port: u16,
                            proto: u8, sport: u16, now: u64) -> Result<(), &'static str> {
        self.control.lock_poison().register(id, dst, port, proto, sport, now)
    }

    pub fn cancel_control(&self, id: &str) {
        self.control.lock_poison().cancel(id);
    }

    /// Called before WG and before the destination counter, so controls never
    /// become production leakage and never enter the encryption path.
    pub fn consume_control(&self, frame: &[u8], now: u64) -> bool {
        if let Some(line) = self.control.lock_poison().scan(frame, now) {
            emit(&line);
            // N2 P5: the sandwich ordering is judged on when the PRE control
            // really HIT (`control_hit` is inert while unarmed).
            if let Some(marker) = crate::n2_control::hit_marker(&line) {
                crate::n2_sandwich::control_hit(marker, now);
                // N2 C/D first batch (2026-09-29): the SAME tee observation is
                // the C/D switching sandwich's authoritative pre/post HIT
                // (`n2_cd` is inert while unarmed, so an unarmed session keeps
                // zero output).
                crate::n2_cd::control_hit(marker, now);
            }
            return true;
        }
        false
    }

    pub fn gap(&self, from: u64, to: u64, reason: &str) {
        emit(&gap_line(from, to, reason));
    }

    /// N2 T7a (item 3) — the session-period tee continuity line:
    ///
    /// ```text
    /// N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…|diag=true|is_evidence=false|first_fail_len=…|first_fail_byte0=0x…|first_fail_prefix=…|fail_v4=…|fail_v6=…|fail_other=…|ipv6_out_of_scope=…|ipv6_other=…|ipv6_scope=…|ipv6_next=…|first_v6_len=…|first_v6_dst_prefix=…|first_v6_other_len=…|first_v6_other_dst=…|first_v6_other_next=…|first_v6_other_dport=…
    /// ```
    ///
    /// `frames` counts every from-OS frame handed to this tee since the arm
    /// (the direction the destination tee exists for; to-OS frames belong to
    /// the collector tee and are not counted here). `decode_fail` counts the
    /// frames `parse_ipv4_frame` refused — a non-zero value means an interval
    /// can NOT be treated as complete (an undecodable frame could have been a
    /// marker or a capture-target frame). `dropped` counts the frames that
    /// decoded but were not attributed to any frozen capture destination (the
    /// tee's own filter, not silent loss). The exact invariant is
    /// `frames = decode_fail + dropped + the sum of the per-destination
    /// frames` (`N2_TUN_DEST`).
    ///
    /// diag-0048 (2026-09-30): the fields appended at the line tail make a
    /// `decode_fail` attributable without any payload capture. The FOUR
    /// pre-existing fields keep their names and order. `first_fail_len` /
    /// `first_fail_byte0` / `first_fail_prefix` witness the FIRST refused frame
    /// (`prefix` is the lowercase hex of at most its first 4 header bytes;
    /// all three are empty until the first failure); `fail_v4` / `fail_v6` /
    /// `fail_other` bucket every refused frame by byte 0's IP-version nibble.
    /// Only leading header bytes are ever recorded, never the payload.
    ///
    /// R1 (approved n2-tun-ipv6-decode-amendment, 2026-10-01): ten fields are
    /// appended AFTER `fail_other`; every field above keeps its name, order and
    /// meaning. `ipv6_out_of_scope` counts the refused version-6 frames that are
    /// the kernel's own link-scoped control frames (link-local `fe80::/10` or
    /// link-scoped multicast `ff02::/16` destination, a protocol chain ending at
    /// ICMPv6, a whitelisted ICMPv6 type and its mandatory hop limit);
    /// `ipv6_other` counts every other refused version-6 frame, so
    /// `fail_v6 == ipv6_out_of_scope + ipv6_other` always holds. `ipv6_scope`
    /// (`linklocal` / `multicast` / `other`) and `ipv6_next`
    /// (`icmp6` / `hbh` / `udp` / `tcp` / `other`, the CHAIN-TAIL protocol) are
    /// histograms over all refused version-6 frames, so `Σipv6_scope ==
    /// Σipv6_next == fail_v6`. `first_v6_len` / `first_v6_dst_prefix` witness
    /// the FIRST refused version-6 frame (the destination's first at most 8
    /// bytes, empty before one); `first_v6_other_*` witness the FIRST refused
    /// version-6 frame that is NOT control class — its length, its FULL 16-byte
    /// destination, its chain-tail protocol number and, for a UDP/TCP tail, its
    /// destination port (`-` when the chain was not walked through, or when the
    /// tail is neither UDP nor TCP). All four are empty while `ipv6_other=0`.
    /// Only header bytes are recorded, never payload. None of these fields
    /// widens the negative-scan scope: IPv6 stays IPv4-only out of scope.
    pub fn stats_line(&self, now_ms: u64) -> String {
        let state = self.inner.lock_poison();
        let witness = state.first_fail;
        let first_fail_len = witness.len.map(|len| len.to_string()).unwrap_or_default();
        let ipv6 = state.ipv6;
        let first_v6_len = ipv6.first_len.map(|len| len.to_string()).unwrap_or_default();
        let (first_other_len, first_other_dst, first_other_next, first_other_dport) =
            ipv6.first_other_fields();
        format!(
            "N2_TUN_TEE_STATS|frames={}|decode_fail={}|dropped={}|mono_ms={now_ms}|diag=true|is_evidence=false|\
             first_fail_len={first_fail_len}|first_fail_byte0={}|first_fail_prefix={}|\
             fail_v4={}|fail_v6={}|fail_other={}|\
             ipv6_out_of_scope={}|ipv6_other={}|ipv6_scope={}|ipv6_next={}|\
             first_v6_len={first_v6_len}|first_v6_dst_prefix={}|\
             first_v6_other_len={first_other_len}|first_v6_other_dst={first_other_dst}|\
             first_v6_other_next={first_other_next}|first_v6_other_dport={first_other_dport}",
            state.tee_frames, state.decode_fail, state.dropped,
            witness.byte0_hex(), witness.prefix_hex(),
            witness.fail_v4, witness.fail_v6, witness.fail_other,
            ipv6.out_of_scope, ipv6.other, ipv6.scope_histogram(), ipv6.next_histogram(),
            ipv6.first_dst_prefix_hex())
    }

    pub fn control_totals(&self) -> (u64, u64, u64) {
        self.control.lock_poison().totals()
    }

    pub fn control_due_lines(&self, now: u64) -> Vec<String> {
        self.control.lock_poison().due(now)
    }

    pub fn control_status(&self, id: &str) -> &'static str {
        let now = sys::mono_ms();
        let mut control = self.control.lock_poison();
        for line in control.due(now) { emit(&line); }
        control.status(id)
    }

    pub fn due_lines(&self, now_ms: u64) -> Vec<String> {
        let mut state = self.inner.lock_poison();
        if now_ms.saturating_sub(state.last_tick_ms) < 5_000 {
            return Vec::new();
        }
        state.last_tick_ms = now_ms;
        let changed = state.rows.iter().any(|row| row.frames != row.reported);
        if !changed && now_ms.saturating_sub(state.last_emit_ms) < 30_000 {
            return Vec::new();
        }
        state.last_emit_ms = now_ms;
        let mut lines = Vec::new();
        for dst in state.targets.clone() {
            let mut found = false;
            for row in state.rows.iter_mut().filter(|row| row.dst == dst) {
                found = true;
                let sources = row.sources.iter()
                    .map(|src| std::net::Ipv4Addr::from(*src).to_string())
                    .collect::<Vec<_>>().join(",");
                lines.push(dest_line(now_ms, dst, row.proto, row.port, row.frames,
                    row.frames.saturating_sub(row.reported), &sources));
                row.reported = row.frames;
            }
            if !found {
                lines.push(dest_zero_line(now_ms, dst));
            }
        }
        lines
    }
}

/// One session-period `N2_TUN_DEST` row (its exact format is part of the N2
/// log baseline; `wg_device.rs` is the only producer).
pub fn dest_line(now_ms: u64, dst: [u8; 4], proto: u8, port: u16, frames: u64, delta: u64,
    sources: &str) -> String {
    format!(
        "N2_TUN_DEST|scope=session|mono_ms={now_ms}|dst={}|proto={proto}|port={port}|frames={frames}|delta={delta}|sources={sources}|diag=true|is_evidence=false",
        std::net::Ipv4Addr::from(dst))
}

/// The zero-hit heartbeat row of a frozen destination with no observed frame
/// (`proto=any|port=any|frames=0|delta=0`; never a sent/received receipt).
pub fn dest_zero_line(now_ms: u64, dst: [u8; 4]) -> String {
    format!(
        "N2_TUN_DEST|scope=session|mono_ms={now_ms}|dst={}|proto=any|port=any|frames=0|delta=0|sources=|diag=true|is_evidence=false",
        std::net::Ipv4Addr::from(dst))
}

/// The session-period control/production totals (emitted on the same cadence as
/// `N2_TUN_DEST`).
pub fn frames_line(now_ms: u64, seen: u64, control: u64, production: u64, leaks: u64) -> String {
    format!(
        "N2_TUN_FRAMES|scope=session|mono_ms={now_ms}|seen={seen}|control={control}|production={production}|leaks={leaks}|diag=true|is_evidence=false")
}

/// One `N2_TUN_GAP` marker (N2 T7a item 3): the interval `from_mono..=to_mono`
/// in which this tee observed NOTHING. A tee mount closes the preceding
/// interval (`reason=before-tun-attach`), a TUN fd replacement closes the swap
/// (`reason=tun-replaced`), and a detach opens a new one
/// (`reason=connector-stop`, a zero-width marker at the close instant because
/// the unobserved interval then extends to the next mount).
pub fn gap_line(from: u64, to: u64, reason: &str) -> String {
    format!("N2_TUN_GAP|from_mono={from}|to_mono={to}|reason={reason}|diag=true|is_evidence=false")
}

/// The data plane's adoption report (N2 T7a item 6): the peer/endpoint counts
/// the device carries, the number of buffered egress sockets actually replayed
/// and the monotonic instant of the adoption.
pub fn wg_feed_adopted_line(peers: usize, endpoints: usize, egress_replayed: usize,
    mono_ms: u64) -> String {
    format!("N7_WG_FEED|adopted|peers={peers}|endpoints={endpoints}|egress_replayed={egress_replayed}|mono_ms={mono_ms}")
}

/// `N6_WG_DEVICE|session-established|peer=<公钥前8位>` —— 哪个 peer 的 WG
/// 会话建成（0050 §8 item 2：真机上这条行不带 peer 标识，`wgSessions=4`
/// 时无从知道是哪 4 个 peer；观测缺口，不是「其余 2 个没建成」的证据）。
/// `peer=` 一律**追加在行尾**，已有字段顺序不变，格式与 N5c_ICE 行同源
/// （同一个 [`crate::peer_conn::peer_short`]）。
pub fn session_established_line(pub_key_b64: &str) -> String {
    format!(
        "N6_WG_DEVICE|session-established|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// `N6_WG_DEVICE|session-expired|peer=<公钥前8位>` —— 同一个 peer 的 WG
/// 会话到期（会话结束侧，与 [`session_established_line`] 同源同格式）。
pub fn session_expired_line(pub_key_b64: &str) -> String {
    format!(
        "N6_WG_DEVICE|session-expired|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// 0.0.26 gap 5（真机 0.0.25 §5.1）：`N13_WG` 的**载体/直连切换行**此前不带
/// peer 标识，多 peer 同时切换时无从归因。`peer=` 一律**追加在行尾**，已有
/// 字段顺序不变，前缀格式与 [`session_established_line`] 同源（同一个
/// [`crate::peer_conn::peer_short`]）—— 公开材料：公钥前 8 位，不含密钥本身。
pub fn carrier_attach_line(kind: &str, replaced: bool, pub_key_b64: &str) -> String {
    format!(
        "N13_WG|carrier-attach|kind={kind}|replaced={replaced}|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// `N13_WG|carrier-detach|peer=<公钥前8位>` —— 见 [`carrier_attach_line`]。
pub fn carrier_detach_line(pub_key_b64: &str) -> String {
    format!(
        "N13_WG|carrier-detach|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// `N13_WG|carrier->direct|ice-selected-path|peer=<公钥前8位>` —— ICE 重新
/// 提名后从载体切回直连的那一次（见 [`carrier_attach_line`]）。
pub fn carrier_to_direct_line(pub_key_b64: &str) -> String {
    format!(
        "N13_WG|carrier->direct|ice-selected-path|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// `N13_WG|direct->carrier|no-ice-endpoint|peer=<公钥前8位>` —— 直连路径消失
/// 后转上载体的那一次（见 [`carrier_attach_line`]）。
pub fn direct_to_carrier_line(pub_key_b64: &str) -> String {
    format!(
        "N13_WG|direct->carrier|no-ice-endpoint|peer={}",
        crate::peer_conn::peer_short(pub_key_b64)
    )
}

/// Whether ONE boringtun stats reading is a handshake **strictly later than the
/// policy expiry instant** of the peer's session (`established_ms +
/// session_max_ms`) —— the only reading that may re-establish an expired
/// session (module docs, 「过期不是终态」).
///
/// `hs` is boringtun's `time_since_last_handshake` in WHOLE SECONDS
/// (`ffi/mod.rs:390` truncates `as_secs()`), i.e. the AGE of the last handshake:
/// after expiry every inbound datagram still reads `hs >= 0` (the old
/// handshake's age), so a bare `hs >= 0` test would flap the session back on the
/// spot. Deriving the completion instant as `now_ms − hs × 1000` and requiring
/// it to beat the expiry instant rejects the old handshake by a full
/// `session_max_ms` margin: its derived value is `t₀ + (age mod 1 s)`, i.e. at
/// most 999 ms past the instant the session was first observed, while the
/// expiry instant sits `session_max_ms` later (540 s by default).
///
/// Truncation direction: `hs × 1000 <= age`, so the derived instant is never
/// EARLIER than the real completion — the residual slack can only accept a
/// handshake that really completed within the last second before the expiry
/// instant. That handshake is a genuine fresh key exchange either way, and the
/// re-anchor (`session_established_ms = now_ms`, always the later instant)
/// keeps the new window strictly later, so the ambiguity cannot flap.
fn handshake_after_expiry(hs: i64, now_ms: u64, established_ms: u64, session_max_ms: u64) -> bool {
    if hs < 0 {
        return false; // no session at all
    }
    let handshake_at_ms = now_ms.saturating_sub(hs as u64 * 1000);
    handshake_at_ms > established_ms.saturating_add(session_max_ms)
}

/// `N6_WG_DEVICE|route-peer|net=<cidr>|peer=<完整公钥>|metric=<n>|live=<bool>|reason=<token>` ——
/// 一个管理端下发网段（managed route）被指派给哪个路由 peer（首次选中 /
/// HA 切换 / 释放各一行；`peer=none` = 该网段当前无可用候选，出向按
/// `no-route` 丢弃）。每次管理路由批次另发当前态 `route-selection` 摘要。
pub fn route_peer_line(
    net: ([u8; 4], u8),
    pub_key_b64: Option<&str>,
    metric: Option<i64>,
    live: bool,
    reason: &str,
) -> String {
    let (addr, plen) = net;
    let peer = match pub_key_b64 {
        Some(k) if !k.is_empty() => k.to_string(),
        _ => "none".to_string(),
    };
    format!(
        "N6_WG_DEVICE|route-peer|net={}.{}.{}.{}/{}|peer={peer}|metric={}|live={live}|reason={reason}",
        addr[0], addr[1], addr[2], addr[3], plen,
        metric.map_or_else(|| "none".to_string(), |value| value.to_string()),
    )
}

fn route_selection_line(
    net: ([u8; 4], u8),
    pub_key_b64: Option<&str>,
    metric: Option<i64>,
    live: bool,
    reason: &str,
) -> String {
    let line = route_peer_line(net, pub_key_b64, metric, live, reason);
    line.replacen("|route-peer|", "|route-selection|", 1)
}

impl TunFrameTee for N2DestinationTee {
    fn on_tun_frame(&self, dir: TunFrameDir, frame: &[u8]) {
        if dir != TunFrameDir::FromOs { return; }
        let mut state = self.inner.lock_poison();
        state.tee_frames = state.tee_frames.saturating_add(1);
        let Some(packet) = crate::n2h::parse_ipv4_frame(frame) else {
            // N2 T7a (item 3): an undecodable frame is a named continuity
            // hole, never a silent `return`. diag-0048: the refusal is also
            // witnessed (first frame's header bytes + per-version buckets).
            state.decode_fail = state.decode_fail.saturating_add(1);
            state.first_fail.record(frame);
            // R1 (approved n2-tun-ipv6-decode-amendment): a version-6 refusal is
            // additionally classified (control class out of scope vs `other`).
            // `decode_fail` and the frozen invariant are untouched.
            state.ipv6.record(frame);
            return;
        };
        if !state.targets.contains(&packet.dst) {
            // Decoded but outside the frozen capture set: counted as dropped
            // (the tee's own filter) so `frames` always reconciles.
            state.dropped = state.dropped.saturating_add(1);
            return;
        }
        let dst = packet.dst;
        let count = state.rows.iter().filter(|row| row.dst == dst).count();
        let existing = state.rows.iter().position(|row|
            row.dst == dst && row.proto == packet.proto && row.port == packet.dport);
        let (proto, port) = if existing.is_none() && count >= 64 {
            (0, 0)
        } else {
            (packet.proto, packet.dport)
        };
        let index = state.rows.iter().position(|row|
            row.dst == dst && row.proto == proto && row.port == port).unwrap_or_else(|| {
                state.rows.push(N2DestinationRow {
                    dst, proto, port, frames: 0, reported: 0, sources: Vec::new(),
                });
                state.rows.len() - 1
            });
        let row = &mut state.rows[index];
        row.frames = row.frames.saturating_add(1);
        if row.sources.len() < 8 && !row.sources.contains(&packet.src) {
            row.sources.push(packet.src);
        }
    }
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
    /// The map-provided allowed_ips (`set_peers` input) WITHOUT the managed
    /// route overlay — `allowed_ips` above is the EFFECTIVE set (base + the
    /// managed nets currently assigned to this peer by
    /// [`WgDevice::rebuild_route_overlay`]). Kept separately so a route-set
    /// or session-state change can re-derive the overlay without the next
    /// `set_peers` re-adding stale nets.
    base_allowed_ips: Vec<([u8; 4], u8)>,
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
    /// N13-D4: plaintext bytes boringtun already counted as encapsulated for
    /// this peer whose datagram the bearer then REFUSED (sendto failure /
    /// typed carrier refusal). Published `tx_bytes` =
    /// [`Tunnel::stats`]'s tunnel-lifetime plaintext TX − this, so a refused
    /// send is never reported as transferred. Boringtun counts INSIDE
    /// `encapsulate` (before the bearer sees the datagram), so the correction
    /// has to be applied here, not in boringtun.
    tx_undelivered_bytes: u64,
    /// N13-D4: plaintext bytes decapsulated from this peer and written into
    /// the TUN, counted on the same success branch as
    /// [`WgDeviceStats::rx_bytes_to_tun`] (that branch is where the local
    /// delivery — not merely the decryption — succeeds).
    rx_bytes: u64,
    /// Test-only injected boringtun handshake reading (`hs`): the device clock
    /// under test is INJECTED while boringtun's handshake age rides its own
    /// REAL `Instant` clock, so a host test cannot age a reading past the
    /// expiry instant it asserts on (in production both are the same host
    /// monotonic clock). `None` — production, always — means the real reading;
    /// only [`WgPeer::stats`] and through it the session observation use it.
    #[cfg(test)]
    hs_override: Option<i64>,
}

impl Drop for WgPeer {
    fn drop(&mut self) {
        // N11 fd contract: exactly our egress dup; provider/ICE-session fds
        // are never touched here.
        if let Some(fd) = self.egress_fd.take() {
            crate::n2_socket_trace::closed(fd);
            unsafe { sys::close(fd) };
        }
    }
}

impl WgPeer {
    fn stats(&self) -> (i64, u64, u64) {
        let (hs, tx, rx) = self.tunnel.stats();
        #[cfg(test)]
        let hs = self.hs_override.unwrap_or(hs);
        (hs, tx, rx)
    }

    fn status(&self) -> WgPeerStatus {
        let (hs, bt_tx, _) = self.stats();
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
            // N13-D4: boringtun's tunnel-lifetime plaintext TX (handshakes /
            // keepalives contribute 0) minus the encapsulations the bearer
            // refused.
            tx_bytes: bt_tx.saturating_sub(self.tx_undelivered_bytes),
            rx_bytes: self.rx_bytes,
        }
    }
}

// ---------------------------------------------------------------------------
// device
// ---------------------------------------------------------------------------

/// One managed-route network with its HA candidate set (derived from the
/// map's [`crate::network_map::ManagedRoute`] list — same network from
/// several route entries = same net with several candidates; per candidate
/// the distribution peer key and that route entry's metric, smallest wins).
/// `selected` is the CURRENT routing peer for the net (exactly one, or
/// `None` when no candidate is currently registered).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedRouteEntry {
    /// Masked network + prefix length (the allowed-ips shape).
    net: ([u8; 4], u8),
    /// (peer public key base64, best metric for that peer) in arrival order.
    candidates: Vec<(String, i64)>,
    selected: Option<String>,
}

/// Multi-peer WireGuard device: one protected UDP socket (dup copy) + one
/// [`TunFd`] + per-peer BoringTun tunnels. Single-threaded pump model: the
/// owner calls `service_tun` / `service_udp` / `tick` with an injected
/// `now_ms`; the connector seam ([`WgDeviceApplier`]) wraps a `Mutex` of it.
///
/// OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): in production this socket IS the
/// shared ICE mux socket (upstream: the mux wraps the WG bind's own conn,
/// `client/iface/bind/ice_bind.go:291-316`), and the ONE reader is the mux's
/// [`crate::ice_mux::IceMux`] thread — [`WgDevice::service_udp`] is retired
/// ([`WgDevice::set_shared_socket_reader`]) while non-STUN datagrams arrive
/// through `handle_udp`. `service_udp` remains the reader of the host-CLI /
/// device-level-test shape (no mux).
pub struct WgDevice {
    cfg: WgDeviceConfig,
    /// OUR dup of the (shell-protected) WG UDP socket. The raw fd is never
    /// stored and never closed here (fd contract, module docs).
    fd: Option<i32>,
    /// getsockname at adopt time (observability only).
    local: ([u8; 4], u16),
    tun: TunFd,
    peers: Vec<WgPeer>,
    /// Managed-route overlay (N6 路由表缺陷修复): management-issued network
    /// routes grouped by network, each with its HA candidate routing peers
    /// and the CURRENT selection. [`WgDevice::set_managed_routes`] replaces
    /// the set; [`WgDevice::rebuild_route_overlay`] re-derives assignments
    /// (and the peers' effective `allowed_ips`) whenever the route set, the
    /// peer set or a session state changes.
    managed_routes: Vec<ManagedRouteEntry>,
    next_index: u32,
    stats: WgDeviceStats,
    /// I2 (N2-H): the collector's TUN frame tee. `None` (default, never
    /// armed) = the pre-I2 data path: the two feed points test this handle
    /// and do nothing else — no copy, no scan, no counter.
    tun_tee: Option<Arc<dyn TunFrameTee>>,
    n2_dest_tee: Option<Arc<N2DestinationTee>>,
    /// OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): the shared-socket model. When a
    /// mux was built over THIS device's outer socket, that mux's reader thread is
    /// the ONE reader of the socket — up to and including WireGuard. A second
    /// reader here would steal STUN checks from the mux (and the mux's reader
    /// would steal WG datagrams from us): exactly one reader, as upstream
    /// (`ice_bind.go:209-287` is WG's receive path *and* the STUN filter, and
    /// `device_kernel_unix.go:109` is the mux reader — never both).
    ///
    /// `false` (the default, and the host-CLI/test shape) = the device reads its
    /// own socket in [`WgDevice::service_udp`], the pre-stage-3 behaviour.
    shared_socket_reader: bool,
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
                crate::n2_socket_trace::closed(fd);
                unsafe { sys::close(fd) };
                return Err(e);
            }
        };
        // N2 C/D (Wb): the takeover itself — the mirrored registration PLUS the
        // base socket's live identity and its protect-settle substitute receipt
        // (this is the only liveness point `Wb` has: it never sends). Reached
        // only after the duplicated descriptor was validated above, so a failed
        // adopt leaves no identity and no receipt behind.
        crate::n2_socket_trace::adopted(wg_socket_raw, fd);
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
            managed_routes: Vec::new(),
            next_index: INDEX_BASE,
            stats: WgDeviceStats::default(),
            tun_tee: None,
            n2_dest_tee: None,
            shared_socket_reader: false,
        })
    }

    /// N8 — controlled-recreate TUN replacement: adopt a dup of the NEW
    /// platform TUN fd and deactivate the old one, keeping peers, tunnels
    /// AND established WG sessions (the session-preserving choice, see the
    /// module docs N8 section). Semantics:
    /// - the new fd is validated by `TunFd::dup_from_raw` FIRST — the SAME
    ///   open function as the first attach, including the forced O_NONBLOCK
    ///   (a blocking platform fd would wedge the pump: roam-1005-0014); on
    ///   any failure the OLD [`TunFd`] stays active untouched (no half
    ///   state — the caller sees the error and runs its fail-closed
    ///   teardown);
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

    /// roam-1005-0124 — pre-destroy TUN release: close the CURRENT TUN dup
    /// and swap in the closed placeholder, keeping peers, tunnels AND
    /// established WG sessions (the pump's TUN half pauses — `service_tun`
    /// no-ops on a closed TunFd; UDP/tick keep running, so keepalives and
    /// session state survive the release window). Returns the closed dup's
    /// fd number, or `None` when the device held no open dup (idempotent).
    ///
    /// Why this exists: until the LAST fd over the platform TUN closes, the
    /// kernel/platform keeps the old vpn-tun netdev alive — the recreate
    /// settle leg then never sees the old interface disappear, and when the
    /// swap-time close finally lands it tears down the SAME-NAMED fresh
    /// interface (device run 1005-0124: settle cap 3839 ms → new create →
    /// old dup closed at replace → iface lost ~1 s later). Releasing BEFORE
    /// the platform destroy() lets the old netdev go the moment destroy()
    /// closes the raw fd.
    pub fn release_tun(&mut self) -> Option<i32> {
        if self.tun.fd().is_none() {
            return None;
        }
        let old = core::mem::replace(&mut self.tun, TunFd::closed());
        match old.close() {
            Ok(fd) => {
                emit("N8_WG_DEVICE|tun-released|sessions-kept");
                Some(fd)
            }
            Err(_) => None, // AlreadyClosed: a racing release already ran
        }
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
            crate::n2_socket_trace::closed(fd);
            unsafe { sys::close(fd) };
            e
        })?;
        // N2 C/D (Wb): replacing the base socket IS the next takeover — the new
        // descriptor becomes the class's live identity (and feeds its
        // protect-settle substitute receipt); the old one's close below then
        // drops the old registration, and a live entry keyed on the NEW
        // descriptor survives that close.
        crate::n2_socket_trace::adopted(wg_socket_raw, fd);
        if let Some(old) = self.fd.take() {
            crate::n2_socket_trace::closed(old);
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

    /// OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): the BORROWED fd of this device's
    /// outer socket — what the ONE shared ICE mux is built over (upstream
    /// `createOrUpdateMux` wraps the WG bind's own conn, `ice_bind.go:291-316`).
    /// The caller dups it; this device keeps and closes only its OWN dup.
    pub fn socket_fd(&self) -> Option<i32> {
        self.fd
    }

    /// OFFICIAL-ALIGNMENT STAGE 3: hand the socket's read half to the shared
    /// mux's reader thread ([`WgDevice::service_udp`] then does nothing) or take
    /// it back. Idempotent; `true` when the state actually changed.
    pub fn set_shared_socket_reader(&mut self, on: bool) -> bool {
        if self.shared_socket_reader == on {
            return false;
        }
        self.shared_socket_reader = on;
        emit(&format!(
            "N6_WG_DEVICE|shared-socket-reader|on={}|by={}",
            u8::from(on),
            if on { "ice-mux" } else { "device" }
        ));
        true
    }

    /// Whether the ONE shared reader (the mux) owns this socket's inbound half.
    pub fn shared_socket_reader(&self) -> bool {
        self.shared_socket_reader
    }

    /// Registered peer set snapshot.
    pub fn peers(&self) -> Vec<WgPeerStatus> {
        self.peers.iter().map(|p| p.status()).collect()
    }

    /// Device counters snapshot.
    pub fn stats(&self) -> WgDeviceStats {
        self.stats
    }

    /// I2 — attach the collector's TUN frame tee (arm: the observation window
    /// opens). Idempotent upsert: a re-attach replaces the previous tee and
    /// returns whether one was already attached. Frames seen before this call
    /// are not counted.
    pub fn attach_tun_tee(&mut self, tee: Arc<dyn TunFrameTee>) -> bool {
        self.tun_tee.replace(tee).is_some()
    }

    /// I2 — detach the tee (window close) and return the detached handle.
    /// Afterwards the data path is back on the `None` zero-overhead path.
    pub fn detach_tun_tee(&mut self) -> Option<Arc<dyn TunFrameTee>> {
        self.tun_tee.take()
    }

    /// I2 — is a tee attached right now (the arm-state read).
    pub fn tun_tee_attached(&self) -> bool {
        self.tun_tee.is_some()
    }

    pub fn attach_n2_dest_tee(&mut self, tee: Arc<N2DestinationTee>) {
        self.n2_dest_tee = Some(tee);
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
                Some(p) => {
                    p.base_allowed_ips = masked.clone();
                    p.allowed_ips = masked;
                }
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
                        allowed_ips: masked.clone(),
                        base_allowed_ips: masked,
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
                        // N13-D4: transfer counters start at zero with the
                        // tunnel — peer replacement (key removed and
                        // re-added, `clear()`) drops them with the old
                        // `WgPeer`; a surviving key keeps its Tunn AND its
                        // counters across endpoint/bearer changes and rekeys.
                        tx_undelivered_bytes: 0,
                        rx_bytes: 0,
                        #[cfg(test)]
                        hs_override: None,
                    });
                }
            }
        }
        // the peer set / base allowed_ips changed: re-derive the managed
        // route overlay (assignments follow the peers; vanished peers drop
        // their nets, new peers become HA candidates).
        self.rebuild_route_overlay("peers-updated");
        emit(&format!("N6_WG_DEVICE|peers={}", self.peers.len()));
        Ok(())
    }

    /// Replace the managed-route overlay from a network map's `routes`
    /// (management-issued network routes: LAN subnets behind routing peers,
    /// HA = several route entries sharing one network with different
    /// distribution peers). Grouping by (masked) network builds the HA
    /// candidate sets; selection happens in [`Self::rebuild_route_overlay`].
    /// Invalid prefixes are rejected before any mutation (no partial apply).
    ///
    /// Exit-node routes (`0.0.0.0/0`) take the SAME path: the selected
    /// routing peer receives `0.0.0.0/0` in its WG allowed_ips, so whatever
    /// the SYSTEM already routed into the TUN (the shell's N3-7 gate owns
    /// that decision and is NOT touched here) is carried by the exit peer.
    pub fn set_managed_routes(
        &mut self,
        routes: &[crate::network_map::ManagedRoute],
    ) -> Result<(), String> {
        let mut grouped: Vec<ManagedRouteEntry> = Vec::new();
        for r in routes {
            if r.network.prefix_len > 32 {
                return Err(format!(
                    "wg-device-managed-route-prefix-invalid (/{})",
                    r.network.prefix_len
                ));
            }
            let net = (mask(r.network.addr, r.network.prefix_len), r.network.prefix_len);
            let entry = match grouped.iter_mut().find(|e| e.net == net) {
                Some(e) => e,
                None => {
                    grouped.push(ManagedRouteEntry {
                        net,
                        candidates: Vec::new(),
                        selected: None,
                    });
                    grouped.last_mut().expect("just pushed")
                }
            };
            match entry.candidates.iter_mut().find(|(k, _)| *k == r.peer) {
                // same peer listed twice for one net: keep the better metric
                Some((_, m)) => {
                    if r.metric < *m {
                        *m = r.metric;
                    }
                }
                None => entry.candidates.push((r.peer.clone(), r.metric)),
            }
        }
        // carry the previous selections over (same net ⇒ same HA set): a
        // route-set refresh that changes nothing must not re-log every net
        // (the map re-syncs with equal content on every reconnect).
        for entry in grouped.iter_mut() {
            if let Some(prev) = self
                .managed_routes
                .iter()
                .find(|e| e.net == entry.net)
                .and_then(|e| e.selected.clone())
            {
                entry.selected = Some(prev);
            }
        }
        self.managed_routes = grouped;
        self.rebuild_route_overlay("routes-updated");
        self.emit_route_selections("routes-updated");
        Ok(())
    }

    fn emit_route_selections(&self, reason: &str) {
        for entry in &self.managed_routes {
            let selected = entry.selected.as_deref();
            let metric = selected.and_then(|key| {
                entry.candidates.iter().find(|(candidate, _)| candidate == key).map(|(_, metric)| *metric)
            });
            let live = selected.is_some_and(|key| {
                self.peers.iter().any(|peer| {
                    peer.key_b64 == key && peer.session_established && !peer.expired
                })
            });
            emit(&route_selection_line(entry.net, selected, metric, live, reason));
        }
    }

    /// Current overlay assignments (net → selected routing peer), snapshot
    /// for status/tests. Order follows the managed route set.
    pub fn managed_route_selections(&self) -> Vec<(([u8; 4], u8), Option<String>)> {
        self.managed_routes
            .iter()
            .map(|e| (e.net, e.selected.clone()))
            .collect()
    }

    /// Re-derive the overlay: for every managed net pick ONE routing peer
    /// (a net can only ever sit on one peer's allowed_ips — WireGuard source
    /// selection is ambiguous otherwise) and rebuild each peer's effective
    /// `allowed_ips = base_allowed_ips + assigned nets`.
    ///
    /// HA selection rule (minimal form of upstream's route-manager behavior:
    /// prefer lower metric, fail over when the chosen peer's session dies):
    ///
    /// 1. a candidate must be a REGISTERED peer; a peer whose session has
    ///    EXPIRED (`session_established && expired`) is skipped — it just
    ///    failed, keep the traffic on the HA partner until it re-handshakes
    ///    (the `session-established` rebuild re-admits it);
    /// 2. among the rest, a peer with a LIVE session wins over one without;
    /// 3. then the smaller route metric wins;
    /// 4. then the smaller public key (stable, deterministic).
    ///
    /// Recomputed on every route-set change (`set_managed_routes`), peer-set
    /// change (`set_peers`) and session-state change (establish / expire);
    /// each actual selection change logs one [`route_peer_line`].
    fn rebuild_route_overlay(&mut self, reason: &str) {
        // pass 1 (immutable): pick per net
        let picks: Vec<Option<String>> = self
            .managed_routes
            .iter()
            .map(|entry| {
                entry
                    .candidates
                    .iter()
                    .filter(|(k, _)| {
                        self.peers.iter().any(|p| {
                            p.key_b64 == *k && !(p.session_established && p.expired)
                        })
                    })
                    .min_by(|a, b| {
                        let alive = |k: &String| {
                            self.peers
                                .iter()
                                .any(|p| p.key_b64 == *k && p.session_established && !p.expired)
                        };
                        // live session first (true > false), then metric,
                        // then key — `min_by` keeps the FIRST minimal element,
                        // so candidate order (management arrival) never
                        // decides a tie the rule does not cover.
                        alive(&b.0)
                            .cmp(&alive(&a.0))
                            .then(a.1.cmp(&b.1))
                            .then(a.0.cmp(&b.0))
                    })
                    .map(|(k, _)| k.clone())
            })
            .collect();
        // pass 2: apply + log the transitions (one line per change)
        for (entry, pick) in self.managed_routes.iter_mut().zip(picks) {
            if entry.selected != pick {
                let metric = pick.as_deref().and_then(|key| {
                    entry.candidates.iter().find(|(candidate, _)| candidate == key).map(|(_, metric)| *metric)
                });
                let live = pick.as_deref().is_some_and(|key| {
                    self.peers.iter().any(|peer| {
                        peer.key_b64 == key && peer.session_established && !peer.expired
                    })
                });
                emit(&route_peer_line(entry.net, pick.as_deref(), metric, live, reason));
                entry.selected = pick;
            }
        }
        // pass 3: rebuild effective allowed_ips from the base set
        for peer in self.peers.iter_mut() {
            let mut ips = peer.base_allowed_ips.clone();
            for entry in &self.managed_routes {
                if entry.selected.as_deref() == Some(peer.key_b64.as_str()) {
                    ips.push(entry.net);
                }
            }
            peer.allowed_ips = ips;
        }
    }

    /// Drop the whole peer set (connector stop semantics); sockets/tunnels
    /// owned by the device stay until the device is dropped. The managed
    /// route overlay goes with it (stop semantics — a later map re-applies).
    pub fn clear(&mut self) {
        let had = !self.peers.is_empty() || !self.managed_routes.is_empty();
        self.peers.clear();
        self.managed_routes.clear();
        if had {
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
                crate::n2_socket_trace::closed(fd);
                unsafe { sys::close(fd) };
                return Err(e);
            }
        };
        crate::n2_socket_trace::duplicated(raw_fd, fd);
        crate::n2_socket_trace::selected_wg(fd);
        if let Some(old) = self.peers[idx].egress_fd.replace(fd) {
            crate::n2_socket_trace::closed(old);
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
        self.recycle_endpoint_at(idx);
    }

    /// The index-scoped body of [`Self::recycle_endpoint`]; `true` when the
    /// peer actually had a path to drop. Split out for N2 T5a's whole-peer-set
    /// reset (`WgPeerApplier::reset_peer_paths`), which must not re-look-up
    /// keys while iterating.
    fn recycle_endpoint_at(&mut self, idx: usize) -> bool {
        let p = &mut self.peers[idx];
        let had_path = p.endpoint.is_some() || p.egress_fd.is_some();
        p.endpoint = None;
        if let Some(fd) = p.egress_fd.take() {
            crate::n2_socket_trace::closed(fd);
            unsafe { sys::close(fd) };
        }
        p.egress_local = None;
        p.first_init_ms = None;
        p.last_init_ms = None;
        if had_path {
            emit("N11_WG|endpoint-recycled");
        }
        had_path
    }

    /// N2 T5a (§4 B column, Wb/Ws): drop EVERY peer's currently selected
    /// path (endpoint cleared, the selected egress dup closed, the handshake
    /// campaign reset) and count the peers that had one. The next ICE
    /// nomination re-attaches a fresh selected socket and a fresh handshake
    /// leaves by it. LIMIT (documented): the base device socket (`self.fd`,
    /// the `wg_fwd_open` dup) is deliberately KEPT — there is no core-side
    /// rebuild path for it; the shell re-feeds it via
    /// `connector_wg_socket_feed`.
    pub fn reset_peer_paths(&mut self) -> usize {
        let mut acted = 0;
        for idx in 0..self.peers.len() {
            if self.recycle_endpoint_at(idx) {
                acted += 1;
            }
        }
        acted
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
        // 2026-10-12（diag/0027 item 3）：载体切换行是**证据**（每对端每次
        // 切换一行），走 phone log 的小额优先预算，不再与 ICE 风暴争普通预算。
        emit_evidence(&carrier_attach_line(
            self.peers[idx]
                .carrier
                .as_ref()
                .map(|c| c.kind())
                .unwrap_or("?"),
            replaced,
            pub_key_b64,
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
            emit_evidence(&carrier_detach_line(pub_key_b64));
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

    /// Process one inbound UDP datagram from `src`.
    ///
    /// Upstream model (`device/receive.go`, netbirdio/wireguard-go
    /// @8bf8fa968f1a): the peer is identified from the DATAGRAM — transport
    /// messages by their receiving index (`indexTable.Lookup`, `:151-156`),
    /// handshake initiations by decrypting the static key with ours and
    /// looking the peer up (`noise-protocol.go:340` `ConsumeMessageInitiation`),
    /// responses by the receiver index (`noise-protocol.go:512`
    /// `ConsumeMessageResponse`) — never from the source address. The source
    /// is adopted as the peer's endpoint only once the crypto CONSUMED the
    /// packet (`Device`→`peer.SetEndpointFromPacket`: initiation `:374`,
    /// response `:401`, transport data/keepalive `:459` and `:515`). An
    /// authenticated packet from an address the peer is not registered at is
    /// therefore standard WireGuard ROAMING, not a drop; NetBird never
    /// disables roaming (see the module docs).
    ///
    /// This device keeps a fast path — the peer already bound to `src` is
    /// tried FIRST — and then hands the datagram to the remaining peers'
    /// tunnels: the first peer whose crypto does not reject it IS the packet's
    /// peer, because boringtun bundles upstream's index-table / static-key
    /// lookup into [`crate::wg::Tunnel::read`] (there is no index API to
    /// consult first — the trial decapsulation is the identification, and a
    /// failed trial mutates nothing: `receive_handshake_initialization` only
    /// touches state after the static-key check, `receive_packet_data` only
    /// after the AEAD tag). Only an AUTHENTICATED outcome adopts the source as
    /// the peer's new endpoint; a consumed cookie reply changes nothing
    /// (`receive.go:284-320`), and a rejected packet changes nothing at all.
    /// Everything else is dropped: `unknown_peer_drops` when no peer was
    /// registered at `src` either, `decrypt_errors` when one was (the
    /// pre-roaming accounting, unchanged).
    pub fn handle_udp(&mut self, datagram: &[u8], src: ([u8; 4], u16), now_ms: u64) -> WgInbound {
        let mut out = WgInbound::default();
        // The peer registered at this source (upstream's steady state). Tried
        // first, and the ONLY case the pre-roaming accounting counts as
        // received before the crypto runs.
        let known = self.peers.iter().position(|p| p.endpoint == Some(src));
        if known.is_some() {
            out.peer_matched = true;
            self.stats.rx_packets += 1;
        }
        // Pre-crypto gate (upstream `receive.go:145-148` `size < MinMessageSize
        // → continue` + the `msgType` switch, i.e. `isWireGuardMsg`): a
        // datagram that is not a WireGuard message never reaches any peer's
        // crypto, so it can identify no peer. An EMPTY datagram additionally
        // means "repeat the last call" to boringtun (`decapsulate` on `&[]`
        // drains the staged-packet queue) — never hand one to a tunnel.
        let Some(msg_type) = crate::ice_session::wg_message_type(datagram) else {
            if known.is_none() {
                self.stats.unknown_peer_drops += 1;
            } else {
                self.stats.decrypt_errors += 1;
            }
            return out;
        };
        for idx in known
            .into_iter()
            .chain((0..self.peers.len()).filter(|j| Some(*j) != known))
        {
            let (op, len, plain) = self.decapsulate(idx, datagram);
            match inbound_outcome(msg_type, op) {
                // Nothing happened (`goto skip` / `continue`): try the next
                // peer — the datagram, not the source, decides.
                InboundOutcome::Rejected => continue,
                InboundOutcome::CookieConsumed => {
                    // `receive.go:284-320`: the cookie was consumed and that is
                    // ALL — no endpoint change, no reply, no session
                    // observation.
                    out.peer_matched = true;
                    if known.is_none() {
                        self.stats.rx_packets += 1;
                    }
                    return out;
                }
                InboundOutcome::Authenticated => {
                    // Standard WireGuard roaming (`peer.SetEndpointFromPacket`,
                    // `peer.go:280-288`). A no-op when the source is already
                    // this peer's endpoint (the fast path).
                    self.adopt_endpoint(idx, src);
                    out.peer_matched = true;
                    if known.is_none() {
                        self.stats.rx_packets += 1;
                    }
                    self.ingest(idx, op, &plain[..len], InboundPath::Udp(src), now_ms, &mut out);
                    return out;
                }
            }
        }
        // WireGuard messages, but no tunnel consumed one: the source is a
        // stranger AND the crypto rejected it.
        if known.is_none() {
            self.stats.unknown_peer_drops += 1;
        } else {
            self.stats.decrypt_errors += 1;
        }
        out
    }
    /// N13-D2 — process one inbound datagram that arrived over the peer's
    /// non-UDP egress carrier (relay). Matching rule (module docs, N13-D2
    /// 「入向匹配映射」): the carrier layer already identified the SENDER —
    /// the relay Transport frame's 36B field is the sender peer id, which
    /// the orchestration layer reverses to the WG public key. Matching is
    /// therefore by registered `key_b64` — the identity-domain equivalent of
    /// `handle_udp`'s datagram identification, sharing the same ingest body
    /// (decapsulate → TUN / reply / session observation). Unknown keys are
    /// dropped + counted (`unknown_peer_drops`), never processed.
    ///
    /// NO endpoint adoption here: a carrier frame carries no source address.
    /// Upstream's relayed receive path hands the datagram to WireGuard with
    /// the peer's OWN synthetic endpoint (`wgproxy/bind/proxy.go:199` →
    /// `ice_bind.go:378`), so its roaming write is a no-op; ours has no
    /// address at all, which is the same outcome by construction.
    pub fn handle_carrier(&mut self, datagram: &[u8], pub_key_b64: &str, now_ms: u64) -> WgInbound {
        let mut out = WgInbound::default();
        let Some(idx) = self.peers.iter().position(|p| p.key_b64 == pub_key_b64) else {
            self.stats.unknown_peer_drops += 1;
            return out;
        };
        out.peer_matched = true;
        self.stats.rx_packets += 1;
        let (op, len, plain) = self.decapsulate(idx, datagram);
        self.ingest(idx, op, &plain[..len], InboundPath::Carrier, now_ms, &mut out);
        out
    }

    /// Decapsulate one datagram with ONE peer's tunnel: boringtun's
    /// `decapsulate` (upstream `RoutineDecryption`'s crypto step) plus the
    /// plaintext buffer it wrote. `&self`: the tunnel's own interior mutability
    /// carries the session/timer state (`crate::wg::Tunnel` is an
    /// `Arc<Mutex<Tunn>>`-backed handle).
    fn decapsulate(&self, idx: usize, datagram: &[u8]) -> (i32, usize, [u8; WG_BUF]) {
        let mut plain = [0u8; WG_BUF];
        let (op, len) = { self.peers[idx].tunnel.read(datagram, &mut plain) };
        (op, len, plain)
    }

    /// Upstream `device/peer.go:280-288` `SetEndpointFromPacket`: the peer's
    /// endpoint becomes the source address of the packet that just
    /// authenticated. Upstream also drops the sticky-socket `clearSrcOnTx`
    /// flag here; this device has no sticky source, so the assignment is the
    /// whole behaviour. Roaming is never disabled upstream (NetBird does not
    /// call `DisableSomeRoamingForBrokenMobileSemantics`, `device_ios.go:92-94`
    /// / `device_android.go:91-93`) and it is not disabled here. Observable on
    /// every change, and it re-arms the UDP path (`dispatch` prefers a landed
    /// endpoint over the relay carrier) — exactly upstream's "the bind sends
    /// to `peer.endpoint.val`".
    fn adopt_endpoint(&mut self, idx: usize, src: ([u8; 4], u16)) {
        let from = self.peers[idx].endpoint;
        if from == Some(src) {
            return;
        }
        self.peers[idx].endpoint = Some(src);
        emit(&wg_roam_line(&self.peers[idx].key_b64, from, src));
    }

    /// Shared inbound body of BOTH receive paths (UDP + carrier): the
    /// already-decapsulated packet → TUN / reply, then the
    /// session-establishment observation + queued packet flush. The reply path
    /// follows the arrival path for UDP (back to `src`, the pre-D2 rule) and
    /// the bearer priority for carrier arrivals (`dispatch`).
    fn ingest(
        &mut self,
        idx: usize,
        op: i32,
        plain: &[u8],
        path: InboundPath,
        now_ms: u64,
        out: &mut WgInbound,
    ) {
        self.process_outcome(idx, op, plain, path, out);

        // Session-establishment observation (boringtun stats flips to >= 0
        // the moment a handshake completes) + queued-packet flush. The TX
        // reading is the N13-D4 correction baseline for `flush_queued`.
        let (hs, bt_tx, _) = self.peers[idx].stats();
        if hs >= 0 {
            let fresh = hs == 0; // handshake observed within the last second
            // Post-expiry re-establishment (module docs, 「过期不是终态」): only
            // a handshake that completed STRICTLY AFTER the expiry instant —
            // the old handshake's age (`hs >= 0`, still readable on every
            // inbound datagram) must never pass for recovery.
            let re_established = self.peers[idx].session_established
                && self.peers[idx].expired
                && handshake_after_expiry(
                    hs,
                    now_ms,
                    self.peers[idx].session_established_ms,
                    self.cfg.session_max_ms,
                );
            if !self.peers[idx].session_established || re_established {
                // FIRST establishment and post-expiry re-establishment run the
                // SAME action set, so the state projection (`wg_sessions` /
                // `online` = established && !expired / `tunnel_ready` / JSON)
                // is identical after either.
                self.peers[idx].session_established = true;
                self.peers[idx].expired = false;
                self.peers[idx].session_established_ms = now_ms;
                self.peers[idx].first_init_ms = None; // campaign done
                emit_evidence(&session_established_line(&self.peers[idx].key_b64));
                // managed-route overlay: a live session re-ranks this peer in
                // every HA set it belongs to (may switch nets onto it)
                self.rebuild_route_overlay("session-established");
            } else if fresh {
                // rekey completed: refresh the expiry anchor
                self.peers[idx].session_established_ms = now_ms;
            }
            out.sent += self.flush_queued(idx, now_ms, bt_tx);
        }
    }

    /// I2 hook — hand one TUN-crossing frame to the attached tee. `None`
    /// (default / detached) leaves the data path exactly as before I2: one
    /// branch test, no buffer copy, no marker scan, no counter.
    fn feed_tun_tee(&self, dir: TunFrameDir, frame: &[u8]) {
        if let Some(tee) = &self.tun_tee {
            tee.on_tun_frame(dir, frame);
        }
        if let Some(tee) = &self.n2_dest_tee {
            tee.on_tun_frame(dir, frame);
        }
    }

    /// Poll + read + `send_from_tun` (device-originated frames). Non-blocking:
    /// poll timeout 0, at most `DRAIN_MAX` frames per call.
    pub fn service_tun(&mut self, now_ms: u64) -> usize {
        let Some(fd) = self.tun.fd() else {
            if let Some(tee) = &self.n2_dest_tee { tee.gap(now_ms, now_ms, "tun-fd-missing"); }
            return 0;
        };
        let mut n = 0usize;
        while n < DRAIN_MAX {
            let (ret, _e, rev) = sys::poll1(fd, sys::POLLIN, 0);
            if ret < 0 || (rev & (sys::POLLERR | sys::POLLHUP | sys::POLLNVAL)) != 0 {
                if let Some(tee) = &self.n2_dest_tee { tee.gap(now_ms, sys::mono_ms(), "tun-poll-error"); }
                break;
            }
            if ret == 0 || (rev & sys::POLLIN) == 0 { break; }
            let mut buf = [0u8; WG_BUF];
            match self.tun.read_frame(&mut buf) {
                Ok(len) => {
                    if self.n2_dest_tee.as_ref().is_some_and(|tee|
                        tee.consume_control(&buf[..len], now_ms)) {
                        n += 1;
                        continue;
                    }
                    self.feed_tun_tee(TunFrameDir::FromOs, &buf[..len]);
                    self.send_from_tun(&buf[..len], now_ms);
                    n += 1;
                }
                Err(_) => {
                    if let Some(tee) = &self.n2_dest_tee { tee.gap(now_ms, sys::mono_ms(), "tun-read-error"); }
                    break;
                }
            }
        }
        n
    }

    /// Poll + recvfrom + `handle_udp`. Non-blocking, at most `DRAIN_MAX`
    /// datagrams per call.
    ///
    /// N2 v4.1 P7: when the outer-source receipt is armed, every datagram read
    /// here ALSO produces a `n2h-outer-receipt` line (the post-NAT source of
    /// the WG outer socket). Unarmed this is one `armed()` load, so the
    /// pre-P7 behaviour and output are unchanged.
    pub fn service_udp(&mut self, now_ms: u64) -> usize {
        // OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): with a mux over this socket the
        // mux's reader thread is the ONE reader (upstream `device_kernel_unix.go:109`
        // / `ice_bind.go:209-287`): it demuxes STUN to the sessions and hands the
        // rest to `handle_udp`. Reading here as well would steal STUN checks from
        // the mux — and the mux would steal WG datagrams from us.
        if self.shared_socket_reader {
            return 0;
        }
        let Some(fd) = self.fd else { return 0 };
        let receipt = crate::n2_outer_receipt::armed();
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
            let src = (from.sin_addr, u16::from_be(from.sin_port));
            if receipt {
                let (lip, lport) = self.local;
                crate::n2_outer_receipt::note_udp(
                    Some(SocketAddr::from((Ipv4Addr::from(lip), lport))),
                    SocketAddr::from((Ipv4Addr::from(src.0), src.1)),
                    &buf[..rn as usize],
                    &self.outer_peer_label(src),
                );
            }
            self.handle_udp(&buf[..rn as usize], src, now_ms);
            n += 1;
        }
        n
    }

    /// N2 v4.1 P7: the `peer=` field of an outer-source receipt — the
    /// registered peer whose endpoint equals `src`, as the N5c 8-char
    /// alphanumeric public-key short prefix. Any other source is `unknown`
    /// (never invented; the off-path probe drop is exactly the receipt the
    /// cellular legs need).
    fn outer_peer_label(&self, src: ([u8; 4], u16)) -> String {
        self.peers
            .iter()
            .find(|p| p.endpoint == Some(src))
            .map(|p| {
                p.key_b64
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .take(8)
                    .collect()
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    /// One device-clock tick: session-expiry check, handshake retries,
    /// keepalives, plus one boringtun `update_timers` per peer (rekey/cookie
    /// housekeeping on boringtun's own real clock). Returns datagrams sent.
    pub fn tick(&mut self, now_ms: u64) -> usize {
        // N2 P2 (E3): the session-period per-fd send report, on the same
        // rhythm as the N2_TUN_DEST block below (armed-only, so an unarmed
        // session does only this one atomic load).
        if crate::n2_socket_trace::armed() {
            for line in crate::n2_socket_trace::fd_send_due_lines(now_ms) {
                emit(&line);
            }
        }
        if let Some(tee) = &self.n2_dest_tee {
            for line in tee.control_due_lines(now_ms) { emit(&line); }
            let dest_lines = tee.due_lines(now_ms);
            if !dest_lines.is_empty() {
                let (seen, control, production) = tee.control_totals();
                emit(&frames_line(now_ms, seen, control, production, tee.leak_count()));
                // N2 T7a (item 3): the same cadence carries the tee
                // continuity line, so an interval can be judged complete
                // (`decode_fail=0`) instead of merely unreported.
                emit(&tee.stats_line(now_ms));
            }
            for line in dest_lines {
                emit(&line);
            }
        }
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
                emit_evidence(&session_expired_line(&self.peers[idx].key_b64));
                // managed-route overlay: the peer just failed — fail its nets
                // over to another HA candidate (if any)
                self.rebuild_route_overlay("session-expired");
            }
            // handshake campaign (a path exists — ICE endpoint or relay
            // carrier (N13-D2) — but no LIVE session: never established, OR
            // the policy expiry declared the session dead while boringtun may
            // still hold the old keys). The expiry branch re-arms HERE, in the
            // very tick that set `expired`, so the peer re-handshakes without
            // waiting for the other side (module docs, 「过期不是终态」);
            // `force_handshake` always formats a fresh ephemeral, even with a
            // session still loaded (`noise/mod.rs:433`, force_resend=true).
            if self.has_path(idx)
                && (!self.peers[idx].session_established || self.peers[idx].expired)
            {
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

    /// Whether a NON-UDP egress carrier (relay lane) is currently attached for
    /// the peer — the guard's `relayUsedAndUp` input
    /// ([`WgPeerApplier::carrier_attached`](crate::connector::WgPeerApplier::carrier_attached)).
    /// Cheap read (one scan, no clone, no stats): called from the ICE pump's
    /// guard tick only for peers that are about to be re-offered.
    pub fn peer_carrier_attached(&self, pub_key_b64: &str) -> bool {
        self.peers
            .iter()
            .any(|p| p.key_b64 == pub_key_b64 && p.carrier.is_some())
    }

    // -- internals ----------------------------------------------------------

    /// Handle the outcome of ONE already-decapsulated datagram (upstream
    /// `RoutineSequentialReceiver`'s body): transport data → TUN, handshake
    /// responses/cookies → out (UDP: back to the arrival `src`; carrier: via
    /// the bearer-priority `dispatch`). `op`/`plain` come from
    /// [`Self::decapsulate`], the peer identification step.
    fn process_outcome(
        &mut self,
        idx: usize,
        op: i32,
        plain: &[u8],
        path: InboundPath,
        out: &mut WgInbound,
    ) {
        let len = plain.len();
        match op {
            OP_TUN_V4 | OP_TUN_V6 => {
                let frame = plain;
                match self.tun.write_frame_budget(frame, TUN_WRITE_BUDGET_MS) {
                    Ok(_) => {
                        self.stats.rx_bytes_to_tun += len as u64;
                        // N13-D4: per-peer attribution on the SAME success
                        // branch/unit as the device counter above (Σ peers ==
                        // device `rx_bytes_to_tun`).
                        self.peers[idx].rx_bytes += len as u64;
                        // I2 to-os feed: same success branch as the counter —
                        // a TUN write the device refused is not an observation.
                        self.feed_tun_tee(TunFrameDir::ToOs, frame);
                        out.wrote_tun = len;
                    }
                    Err(_) => self.stats.tun_write_errors += 1,
                }
            }
            OP_NETWORK if len > 0 => {
                let sent = match path {
                    InboundPath::Udp(src) => {
                        let fd = self.egress_fd_of(idx);
                        self.send_to(fd, plain, src)
                    }
                    // reply rides the selected bearer (ICE endpoint if it
                    // won meanwhile, the carrier otherwise)
                    InboundPath::Carrier => self.dispatch(idx, plain),
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
                emit_evidence(&carrier_to_direct_line(&self.peers[idx].key_b64));
            }
            let ep = self.peers[idx].endpoint.expect("endpoint checked above");
            let fd = self.egress_fd_of(idx);
            return self.send_to(fd, datagram, ep);
        }
        if !self.peers[idx].on_carrier {
            self.peers[idx].on_carrier = true;
            self.stats.carrier_takeovers += 1;
            emit_evidence(&direct_to_carrier_line(&self.peers[idx].key_b64));
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
            // N13-D4: boringtun counted this frame's plaintext INSIDE
            // `encapsulate` (before the bearer saw it). The datagram was
            // refused, so it is not transferred — undo that count for the
            // published per-peer `tx_bytes`. Only a DATA message carries
            // plaintext accounting: with no live session boringtun queues the
            // frame and the datagram here is the initiation (type 1, 0 bytes
            // counted), which must not be charged either.
            if wg_message_is_data(&datagram) {
                self.peers[idx].tx_undelivered_bytes += frame.len() as u64;
            }
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
    ///
    /// N13-D4: `bt_tx_entry` is boringtun's plaintext TX counter read by the
    /// caller just before this call (`ingest` reads stats anyway). A queued
    /// packet is encrypted INSIDE boringtun, so its plaintext length is only
    /// knowable through that counter — it is the baseline for correcting a
    /// refused flush (`tx_undelivered_bytes`), keeping the published per-peer
    /// `tx_bytes` a "delivered" count. On the empty-queue path (the common
    /// one) the loop breaks on the first `OP_DONE` read and reads nothing
    /// extra.
    fn flush_queued(&mut self, idx: usize, now_ms: u64, bt_tx_entry: u64) -> usize {
        let mut sent = 0usize;
        let mut bt_tx_baseline = bt_tx_entry;
        for _ in 0..FLUSH_MAX {
            let mut ct = [0u8; WG_BUF];
            let (op, len) = { self.peers[idx].tunnel.read(&[], &mut ct) };
            if op != OP_NETWORK || len == 0 {
                break;
            }
            let datagram = ct[..len].to_vec();
            if !self.dispatch(idx, &datagram) {
                // boringtun counted this flushed packet's plaintext before
                // the bearer refused it: subtract exactly the delta since the
                // last packet that WAS delivered.
                let (_, bt_tx_now, _) = self.peers[idx].stats();
                self.peers[idx].tx_undelivered_bytes +=
                    bt_tx_now.saturating_sub(bt_tx_baseline);
                break;
            }
            self.stats.tx_packets += 1;
            self.peers[idx].last_outbound_ms = Some(now_ms);
            sent += 1;
            // advance the correction baseline (rare: only reached when
            // boringtun really had a queued packet to flush)
            bt_tx_baseline = self.peers[idx].stats().1;
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
        if crate::n2_socket_trace::armed() {
            crate::n2_socket_trace::io_begin(fd, std::net::SocketAddr::from((dst.0, dst.1)));
        }
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
        if crate::n2_socket_trace::armed() {
            crate::n2_socket_trace::first_io(fd, std::net::SocketAddr::from((dst.0, dst.1)),
                if n < 0 { "failed" } else { "sent" }, "wg_sendto");
        }
        if n < 0 {
            self.stats.send_errors += 1;
            return false;
        }
        // N2 P2 (E3): the per-fd counter behind `N2_FD_SEND` — only a
        // completed sendto moves it, and only while armed.
        if crate::n2_socket_trace::armed() {
            crate::n2_socket_trace::wg_sent(fd, n as u64);
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
            crate::n2_socket_trace::closed(fd);
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

/// What one tunnel's crypto did with an inbound datagram — upstream
/// `device/receive.go`'s three possible landing places. The distinction is the
/// WHOLE of the endpoint-update rule: only [`Self::Authenticated`] may adopt
/// the packet's source address as the peer's endpoint
/// (`peer.SetEndpointFromPacket`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InboundOutcome {
    /// The crypto CONSUMED the packet as an authenticated one. Upstream reaches
    /// the endpoint update for exactly these: a valid handshake initiation
    /// (`receive.go:374`) or response (`:401`), and a decrypted
    /// transport packet — data OR keepalive — that cleared its replay filter
    /// (`:459`, `:515`).
    Authenticated,
    /// A cookie reply was consumed (`:284-320`): upstream logs it, consumes it
    /// and `goto skip`s — no endpoint update, no reply, no session change.
    CookieConsumed,
    /// Bad MAC1 / wrong static key / bad AEAD tag / unknown or replayed
    /// counter / unknown receiver index / malformed: upstream's `goto skip` /
    /// `continue` — inert, and never a peer identification.
    Rejected,
}

/// Classify one tunnel's `read` on a datagram already known to be a WireGuard
/// message of type `msg_type` ([`crate::ice_session::wg_message_type`]).
///
/// boringtun's single `OP_ERROR` covers every rejection, and its `OP_DONE` is
/// ambiguous by itself — an authenticated keepalive and a consumed cookie reply
/// both produce it — so the message TYPE breaks the tie, exactly as upstream's
/// `switch msgType` does. The `(type, op)` pairs that cannot come from a
/// consumed packet fall through to [`InboundOutcome::Rejected`].
fn inbound_outcome(msg_type: u32, op: i32) -> InboundOutcome {
    if op == OP_ERROR {
        return InboundOutcome::Rejected;
    }
    match (msg_type, op) {
        // 4 = transport: decrypted data, or an authenticated keepalive
        // (empty payload, `noise/mod.rs::validate_decapsulated_packet`).
        (4, OP_TUN_V4) | (4, OP_TUN_V6) | (4, OP_DONE) => InboundOutcome::Authenticated,
        // 1 = initiation / 2 = response: only a CONSUMED one produces a reply
        // (the response packet, or the initiating side's keepalive).
        (1, OP_NETWORK) | (2, OP_NETWORK) => InboundOutcome::Authenticated,
        // 3 = cookie reply: consumed, nothing else happens.
        (3, OP_DONE) => InboundOutcome::CookieConsumed,
        _ => InboundOutcome::Rejected,
    }
}

/// `ip:port`, or `none` when the peer had no endpoint — the `from=` field of
/// the roaming line (public routing material only).
pub fn endpoint_text(v: Option<([u8; 4], u16)>) -> String {
    match v {
        Some((ip, port)) => format!("{}.{}.{}.{}:{}", ip[0], ip[1], ip[2], ip[3], port),
        None => "none".to_string(),
    }
}

/// `N6_WG_DEVICE|roam|peer=<公钥前8位>|from=<ip:port|none>|to=<ip:port>` ——
/// 上游 `peer.SetEndpointFromPacket`（`device/peer.go:280-288`；调用点
/// `device/receive.go:459`/`:515`）在本层的落点：一个被**认证消费**的入向包
/// 把该 peer 的 endpoint 改成它的来源地址（标准 WireGuard roaming，NetBird
/// 从不关闭）。`from=none` 就是「对端此前根本没有 endpoint」——会话从零建立
/// 的那一下。公开路由信息，无密钥材料。
pub fn wg_roam_line(pub_key_b64: &str, from: Option<([u8; 4], u16)>, to: ([u8; 4], u16)) -> String {
    format!(
        "N6_WG_DEVICE|roam|peer={}|from={}|to={}",
        crate::peer_conn::peer_short(pub_key_b64),
        endpoint_text(from),
        endpoint_text(Some(to))
    )
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

/// N13-D4: is this WireGuard datagram a transport DATA message (type 4)? The
/// first 4 wire bytes are the little-endian message type (boringtun
/// `noise/mod.rs` message-type table: 1 init / 2 response / 3 cookie /
/// 4 data). Only DATA encapsulations carry plaintext bytes in
/// [`Tunnel::stats`], so this is the type test behind the refused-send
/// correction in `encap_and_send` — a handshake datagram must never be
/// charged to a peer's `tx_bytes`.
fn wg_message_is_data(datagram: &[u8]) -> bool {
    datagram.len() >= 4
        && u32::from_le_bytes([datagram[0], datagram[1], datagram[2], datagram[3]]) == 4
}

fn getsockname(fd: i32) -> Result<([u8; 4], u16), String> {
    let mut addr = sys::sockaddr_in::new([0, 0, 0, 0], 0);
    let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
    if unsafe { sys::getsockname(fd, &mut addr, &mut len) } == -1 {
        return Err(format!("wg-device-socket-name-failed (errno={})", sys::errno()));
    }
    if addr.sin_family != sys::AF_INET as u16 {
        return Err("wg-device-socket-family-invalid".to_string());
    }
    Ok((addr.sin_addr, u16::from_be(addr.sin_port)))
}

// ---------------------------------------------------------------------------
// status surface for the connector (N7)
// ---------------------------------------------------------------------------

/// Data-plane status exposed through `connector_status()` (`wg` field).
/// Counters are the REAL [`WgDeviceStats`] values once a device is up; all
/// booleans false / counters zero while the data plane is not started
/// (fail-closed: no shell feeds ⇒ no device ⇒ nothing ready). No secret
/// material: the `wg_peers` tail carries PUBLIC keys only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// 0.0.26 gap 3 (真机 0.0.25 §5.2): inbound datagrams that matched NO
    /// registered peer at all — the off-path demux drop of
    /// [`WgDeviceStats::unknown_peer_drops`]. Exported next to
    /// `dropped_no_route`/`decrypt_errors` because those three are the
    /// device's whole drop accounting and only this one was missing from the
    /// status document (the field existed on the stats struct but never left
    /// the process).
    pub unknown_peer_drops: u64,
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
    // -- N13-D1 per-peer session attribution (appended to the JSON AFTER the
    //    carrier_* counters; consumers read the head positionally) --
    /// Read-only per-peer snapshots ([`WgPeerStatus`], registration order) —
    /// the attribution surface for `peers_with_session`: WHICH peer's
    /// session is up, on which overlay prefixes and which bearer, plus its
    /// per-peer plaintext transfer counters (N13-D4, trailing JSON fields).
    /// Empty while no device is up (nothing to attribute).
    pub wg_peers: Vec<WgPeerStatus>,
}

impl WgDataplaneStatus {
    /// The unfed / not-started snapshot (all false, all zero).
    pub fn not_started() -> Self {
        WgDataplaneStatus::default()
    }

    /// The `connector_status()` `wg` JSON object.
    pub fn to_json(&self) -> String {
        format!(
            "{{{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},\"wg_peers\":[{}]}}",
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
            crate::util::jinum("unknown_peer_drops", self.unknown_peer_drops as i64),
            crate::util::jinum("carrier_tx_packets", self.carrier_tx_packets as i64),
            crate::util::jinum("carrier_tx_bytes", self.carrier_tx_bytes as i64),
            crate::util::jinum("carrier_rejects", self.carrier_rejects as i64),
            crate::util::jinum("carrier_takeovers", self.carrier_takeovers as i64),
            crate::util::jinum("direct_restores", self.direct_restores as i64),
            self.wg_peers.iter().map(wg_peer_json).collect::<Vec<String>>().join(","),
        )
    }
}

/// `"key":"ip:port"` / `"key":null` for an optional IPv4 endpoint (the
/// `route_network_string` dotted-quad convention plus the port).
fn opt_endpoint_json(key: &str, v: Option<([u8; 4], u16)>) -> String {
    match v {
        Some((ip, port)) => {
            format!("\"{key}\":\"{}.{}.{}.{}:{port}\"", ip[0], ip[1], ip[2], ip[3])
        }
        None => format!("\"{key}\":null"),
    }
}

/// `"key":<seconds>` for a handshake age, `null` while there is no session.
/// [`WgPeerStatus::last_handshake_s`] carries boringtun's -1 sentinel; the
/// JSON uses the document's `null`-for-absent convention instead, so the
/// whole `connector_status()` document stays readable by the strict reader
/// (`config::parse_document` rejects negative numbers) — the same reason
/// `session_expires_at_unix` is null when unset.
fn opt_handshake_json(key: &str, v: i64) -> String {
    if v < 0 {
        format!("\"{key}\":null")
    } else {
        crate::util::jinum(key, v)
    }
}

/// One `wg_peers` entry (N13-D1): the per-peer session attribution surface —
/// PUBLIC key, overlay prefixes (`masked`, the [`WgPeerStatus`] convention)
/// and the bearer/session flags. No secret material.
///
/// N13-D4 appends the per-peer transfer counters LAST (`tx_bytes` then
/// `rx_bytes`), after the N13-D1/D2 peer fields: positional readers of the
/// head keep working, and the unit is PLAINTEXT (inner-IP) bytes in both
/// directions ([`WgPeerStatus::tx_bytes`] / [`WgPeerStatus::rx_bytes`]) — NOT
/// the device-level `wg.tx_bytes` (encapsulated wire bytes).
fn wg_peer_json(p: &WgPeerStatus) -> String {
    let allowed_ips = p
        .allowed_ips
        .iter()
        .map(|&(ip, prefix_len)| {
            format!("\"{}.{}.{}.{}/{prefix_len}\"", ip[0], ip[1], ip[2], ip[3])
        })
        .collect::<Vec<String>>()
        .join(",");
    format!(
        "{{{},{},\"allowed_ips\":[{}],{},{},{},{},{},{},{},{}}}",
        crate::util::jstr("pub_key_b64", &p.pub_key_b64),
        opt_endpoint_json("endpoint", p.endpoint),
        allowed_ips,
        crate::util::jbool("session_established", p.session_established),
        crate::util::jbool("expired", p.expired),
        opt_handshake_json("last_handshake_s", p.last_handshake_s),
        opt_endpoint_json("egress_local", p.egress_local),
        crate::util::jbool("carrier_attached", p.carrier_attached),
        crate::util::jbool("on_carrier", p.on_carrier),
        crate::util::jinum("tx_bytes", p.tx_bytes as i64),
        crate::util::jinum("rx_bytes", p.rx_bytes as i64),
    )
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

    /// N6: management-issued network routes → device routing table (HA
    /// selection + allowed_ips overlay, see `set_managed_routes`).
    fn apply_managed_routes(
        &self,
        routes: &[crate::network_map::ManagedRoute],
    ) -> Result<(), String> {
        self.device.lock_poison().set_managed_routes(routes)
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

    /// N2 T5a: drop every peer's selected path (Wb/Ws stimulus).
    fn reset_peer_paths(&self) -> usize {
        self.device.lock_poison().reset_peer_paths()
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

    /// Guard `PartiallyConnected` input: is the peer's relay lane attached
    /// (see the trait method's contract)?
    fn carrier_attached(&self, pub_key_b64: &str) -> bool {
        self.device.lock_poison().peer_carrier_attached(pub_key_b64)
    }

    /// STAGE 3: the bare device's outer-socket fd, for the shared mux.
    fn wg_socket_fd(&self) -> Option<i32> {
        self.device.lock_poison().socket_fd()
    }

    /// STAGE 3: the shared socket's read half changes owner.
    fn set_shared_socket_reader(&self, on: bool) -> bool {
        self.device.lock_poison().set_shared_socket_reader(on)
    }
}

// ---------------------------------------------------------------------------
// WgDeviceFeed — the PRODUCTION-DEFAULT seam (N7)
// ---------------------------------------------------------------------------

/// Pump cadence for the production data-plane thread (same shape as the ICE
/// pump; the device itself is clock-injected — this loop only FEEDS it).
/// 100 ms → 10 ms (100 Hz) so `DRAIN_MAX` frames are actually available per
/// second: 64 frames × 100 cycles/s. This is the loop's only idle cost (one
/// wakeup per tick, each doing non-blocking poll/read work).
pub const WG_PUMP_TICK_MS: u64 = 10;

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
/// WG socket feeds duplicate the shell fd immediately and retain an owned
/// copy until device adoption; the shell closes its original after feeding.
/// The platform TUN fd remains borrowed and is duplicated only at device
/// construction (`TunFd::dup_from_raw`). Build failure drops the owned WG
/// copy and forgets the borrowed TUN number; a fresh pair can retry.
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
    /// Last supplied shell number, retained only for diagnostics.
    wg_socket_raw: Option<i32>,
    /// Native-owned copy of the protected outer socket until device adoption.
    wg_socket_owned: Option<OwnedFd>,
    /// Platform TUN number stays borrowed; its owner is VpnConnection.
    tun_raw: Option<i32>,
    /// Buffered peer set (full-snapshot semantics, as received).
    peers: Vec<crate::connector::WgPeerEntry>,
    /// N6: buffered managed-route set (as received; replayed AFTER the peer
    /// set at adoption — HA selection needs the peers registered).
    managed_routes: Vec<crate::network_map::ManagedRoute>,
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
    /// OFFICIAL-ALIGNMENT STAGE 3: the shared-socket model is ARMED for the
    /// device of this feed (the mux's reader thread is the ONE reader). Stored
    /// here so it is re-applied when the device is (re)built after this call —
    /// the shell arms the mux in the same step in which it feeds the socket.
    shared_udp_reader: bool,
    n2_dest_tee: Option<Arc<N2DestinationTee>>,
    n2_gap_start: Option<u64>,
    /// Audit counters (tests + honest observation; not in the status JSON).
    feeds_accepted: u64,
    build_failures: u64,
}

impl Drop for FeedInner {
    fn drop(&mut self) {
        if let Some(fd) = self.wg_socket_owned.as_ref() {
            crate::n2_socket_trace::closed(fd.as_raw_fd());
        }
    }
}

impl WgDeviceFeed {
    /// A fresh, unfed slot for `cfg` (device config: local identity +
    /// injected-clock timer policy).
    pub fn new(cfg: WgDeviceConfig) -> WgDeviceFeed {
        WgDeviceFeed { cfg, inner: Mutex::new(FeedInner::default()) }
    }

    pub fn set_n2_destination_targets(&self, targets: Vec<[u8; 4]>) -> bool {
        let mut g = self.inner.lock_poison();
        if g.device.is_some() || g.n2_dest_tee.is_some() { return false; }
        let tee = Arc::new(N2DestinationTee::new(targets, sys::mono_ms()));
        g.n2_gap_start = Some(sys::mono_ms());
        g.n2_dest_tee = Some(tee);
        true
    }

    pub fn set_n2_outer_targets(&self, targets: Vec<([u8; 4], u8, u16)>) -> bool {
        let g = self.inner.lock_poison();
        if g.device.is_some() { return false; }
        let Some(tee) = &g.n2_dest_tee else { return false; };
        tee.configure_outer(targets);
        true
    }

    pub fn n2_control_tee(&self) -> Option<Arc<N2DestinationTee>> {
        let g = self.inner.lock_poison();
        if g.device.is_none() { return None; }
        g.n2_dest_tee.clone()
    }

    /// Shell feed #1: the (protected) WG outer UDP socket fd. The number is
    /// duplicated immediately into an owned pending fd. The raw number is
    /// retained only for diagnostics; the device duplicates the owned fd
    /// again at adoption.
    pub fn feed_wg_socket(&self, raw_fd: i32) -> Result<(), WgFeedRefusal> {
        self.feed_fd(raw_fd, FeedWhich::WgSocket)
    }

    /// OFFICIAL-ALIGNMENT STAGE 3 (audit §2.7): the BORROWED fd of the live
    /// device's outer socket — what the ONE shared ICE mux is built over
    /// ([`WgDevice::socket_fd`]). `None` while no device is up (nothing to share
    /// a socket with yet — the mux arm then fails closed with `no-wg-device`).
    pub fn socket_fd(&self) -> Option<i32> {
        let g = self.inner.lock_poison();
        g.device.as_ref().and_then(|d| d.with_device(|dev| dev.socket_fd()))
    }

    /// OFFICIAL-ALIGNMENT STAGE 3: the mux now owns the socket's read half — the
    /// device must stop reading it ([`WgDevice::service_udp`]). Stored for a
    /// device built later by the same feed pair, and applied to the live device
    /// immediately when there is one.
    pub fn set_shared_socket_reader(&self, on: bool) -> bool {
        let mut g = self.inner.lock_poison();
        let changed = g.shared_udp_reader != on;
        g.shared_udp_reader = on;
        if let Some(dev) = g.device.as_ref() {
            dev.with_device(|d| d.set_shared_socket_reader(on));
        }
        changed
    }

    /// Shell feed #2: the platform TUN fd (same validation + ownership
    /// rules; `TunFd::dup_from_raw` carries the executable fd contract).
    /// N8: while the device is up this REPLACES the platform TUN fd
    /// (controlled recreate — [`WgDevice::replace_tun`]); the WG socket feed
    /// likewise accepts a validated replacement while the device is up.
    pub fn feed_tun(&self, raw_fd: i32) -> Result<(), WgFeedRefusal> {
        self.feed_fd(raw_fd, FeedWhich::Tun)
    }

    /// roam-1005-0124: the shell's pre-destroy TUN release — while the
    /// device is up, close its current TUN dup ([`WgDevice::release_tun`]),
    /// pausing the TUN half of the data plane while peers, tunnels and WG
    /// sessions stay live. The next `feed_tun` (the new connection's fd)
    /// re-attaches via the N8 replace path. `Err("device-not-up")` when no
    /// device exists (nothing to release — the shell logs and proceeds);
    /// `Ok(None)` when the device held no open dup (idempotent re-release).
    pub fn release_tun(&self) -> Result<Option<i32>, &'static str> {
        let mut g = self.inner.lock_poison();
        let Some(dev) = g.device.as_ref() else {
            return Err("device-not-up");
        };
        let released = dev.with_device(|d| d.release_tun());
        if released.is_some() {
            g.tun_raw = None; // honest status: no TUN attached right now
            emit("N8_WG_FEED|tun-released|device-kept");
        }
        Ok(released)
    }

    fn feed_fd(&self, raw_fd: i32, which: FeedWhich) -> Result<(), WgFeedRefusal> {
        if raw_fd < 0 {
            return Err(WgFeedRefusal::FdMissing);
        }
        // Retain the protected outer socket before the shell can close its fd.
        // The TUN still belongs to VpnConnection and is duped at adoption.
        let owned = match crate::mgmtsock::dup_socket_fd(raw_fd) {
            Ok(fd) => {
                if matches!(which, FeedWhich::WgSocket) {
                    crate::n2_socket_trace::duplicated(raw_fd, fd);
                }
                unsafe { OwnedFd::from_raw_fd(fd) }
            }
            Err(e) => return Err(WgFeedRefusal::FdInvalid { errno: e.errno() }),
        };
        let built = {
            let mut g = self.inner.lock_poison();
            match which {
                FeedWhich::WgSocket => {
                    if g.device.is_some() {
                        if let Some(Err(_)) = g.device.as_ref().map(|dev| {
                            dev.with_device(|d| d.reattach_socket(owned.as_raw_fd()))
                        }) {
                            crate::n2_socket_trace::closed(owned.as_raw_fd());
                            return Err(WgFeedRefusal::SocketSwapUnsupported);
                        }
                        crate::n2_socket_trace::closed(owned.as_raw_fd());
                        g.wg_socket_raw = Some(raw_fd);
                        g.feeds_accepted += 1;
                        emit("N8_WG_FEED|socket-replaced|device-kept");
                        return Ok(());
                    }
                    g.wg_socket_raw = Some(raw_fd);
                    if let Some(old) = g.wg_socket_owned.as_ref() {
                        crate::n2_socket_trace::closed(old.as_raw_fd());
                    }
                    g.wg_socket_owned = Some(owned);
                }
                FeedWhich::Tun => {
                    if let Some(dev) = g.device.as_ref() {
                        // N8 controlled recreate: a TUN feed while the device
                        // is up REPLACES the platform TUN fd (adopt the new
                        // dup, deactivate the old; sessions kept). On failure
                        // the old TUN stays active (no half state) and the
                        // refusal surfaces to the shell's fail-closed path.
                        let replace_begin = sys::mono_ms();
                        if let Err(e) = dev.with_device(|d| d.replace_tun(raw_fd)) {
                            return Err(WgFeedRefusal::TunReplaceFailed { errno: e.errno() });
                        }
                        if let Some(tee) = &g.n2_dest_tee {
                            tee.gap(replace_begin, sys::mono_ms(), "tun-replaced");
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
        let (Some(wg_raw), Some(tun_raw)) = (g.wg_socket_owned.as_ref().map(AsRawFd::as_raw_fd), g.tun_raw) else {
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
        // STAGE 3: a device built after the mux was armed inherits the shared
        // socket's reader ownership (the shell arms the mux in the same step in
        // which it feeds the socket, and the device may come up on a later feed
        // — the TUN one).
        let mut dev = dev;
        dev.set_shared_socket_reader(g.shared_udp_reader);
        let dev = dev;
        let app = Arc::new(WgDeviceApplier::new(dev));
        if let Some(tee) = g.n2_dest_tee.clone() {
            app.with_device(|d| d.attach_n2_dest_tee(tee.clone()));
            if let Some(from) = g.n2_gap_start.take() {
                tee.gap(from, sys::mono_ms(), "before-tun-attach");
            }
            // N2 P5: the frozen common anchor — the PRE control's earliest
            // legal moment is AFTER this attach/gap pair (v4.1 §4). Recorded
            // here, at the attach itself, so the sandwich state machine judges
            // "first send before attach" against the real anchor.
            crate::n2_sandwich::attach_anchor(sys::mono_ms());
        }
        // replay the buffered control-plane state; peer-set errors here mean
        // buffered entries the device rejects — fail the build (the map is
        // re-synced with the same content, so a persistent error stays loud
        // via wg_apply_failed instead of silently dropping peers)
        if let Err(e) = app.apply_peers(&g.peers) {
            return Self::build_failed(g, &format!("replay-peers-{e}"));
        }
        // N6: managed-route overlay replays after the peer set (selection
        // ranks only REGISTERED peers); a rejection fails the build like a
        // peer rejection — the map re-syncs with the same content.
        if let Err(e) = app.apply_managed_routes(&g.managed_routes) {
            return Self::build_failed(g, &format!("replay-managed-routes-{e}"));
        }
        // N11: egress FIRST (the selected transport), then the N13-D2 relay
        // carriers, then endpoints — a replayed endpoint fires its handshake,
        // which must leave via the path `dispatch` selects. A stale buffered
        // fd (session closed meanwhile) surfaces as a replay failure; the
        // next ICE selection re-attaches.
        // FAIL-CLOSED (P1): a peer whose egress replay FAILED must not get its
        // endpoint replayed either — otherwise the handshake would leave from
        // the base device socket, the exact "endpoint without egress" shape
        // `peer_conn.rs` (SelectedPair landing: attach, and only then apply
        // the endpoint) structurally forbids. The peer simply keeps no path
        // and waits for the next ICE selection. This holds whether or not the
        // n2_evidence switch is armed.
        let mut egress_failed: Vec<&str> = Vec::new();
        for (key, fd) in &g.egress {
            if let Err(e) = app.attach_egress_socket(key, *fd) {
                emit(&format!("N11_WG_FEED|replay-egress-failed|{}", e));
                egress_failed.push(key.as_str());
            }
        }
        for (key, carrier) in &g.carriers {
            if let Err(e) = app.attach_carrier(key, carrier.clone()) {
                emit(&format!("N13_WG_FEED|replay-carrier-failed|{}", e));
            }
        }
        for (key, addr, port) in &g.endpoints {
            if egress_failed.contains(&key.as_str()) {
                emit(&format!(
                    "N11_WG_FEED|replay-endpoint-skipped|peer={key}|reason=egress-replay-failed"
                ));
                continue;
            }
            // individually validated at buffer time; a replay failure would
            // mean the peer vanished from the just-applied set — logged (the
            // key is PUBLIC material), the next ICE re-selection re-lands it
            if let Err(e) = app.apply_endpoint(key, *addr, *port) {
                emit(&format!("N7_WG_FEED|replay-endpoint-failed|{}", e));
            }
        }
        emit(&wg_feed_adopted_line(g.peers.len(), g.endpoints.len(),
            // N2 T7a (item 6): the number of buffered egress sockets that were
            // actually re-attached (a failed replay is counted out), so the
            // adopted line states how much of the selected-transport state the
            // data plane really carries.
            g.egress.len().saturating_sub(egress_failed.len()), crate::sys::mono_ms()));
        g.device = Some(app);
        crate::n2_socket_trace::closed(wg_raw);
        // The pending feed-time dup is consumed by the adoption: close it AND
        // drop its trace registration, so its number can never be reused by a
        // later socket while a stale `Wb`/`Ws` entry still carries its
        // per-fd send counter (the N2 P2 zero-send evidence must not be
        // attributable to a recycled number).
        if let Some(pending) = g.wg_socket_owned.take() {
            crate::n2_socket_trace::closed(pending.as_raw_fd());
        }
        true
    }

    /// Common build-failure handling: count, log the token, drop BOTH fed
    /// fds (the pair must be re-fed to retry — keeps the state machine
    /// honest about which fds actually worked).
    fn build_failed(g: &mut FeedInner, reason: &str) -> bool {
        g.build_failures += 1;
        g.wg_socket_raw = None;
        if let Some(fd) = g.wg_socket_owned.as_ref() {
            crate::n2_socket_trace::closed(fd.as_raw_fd());
        }
        g.wg_socket_owned = None;
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

    /// N6: buffered pre-device, applied live after adoption (the overlay
    /// follows every map update — selection re-runs on the new HA sets).
    fn apply_managed_routes(
        &self,
        routes: &[crate::network_map::ManagedRoute],
    ) -> Result<(), String> {
        let mut g = self.inner.lock_poison();
        g.managed_routes = routes.to_vec();
        match g.device.as_ref() {
            Some(d) => d.apply_managed_routes(routes),
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
        g.managed_routes.clear();
        // STAGE 3: the device (and its socket) goes; the mux that read it is gone
        // too, so a device rebuilt by a later feed pair reads for itself again
        // until the shell arms a fresh mux over it.
        g.shared_udp_reader = false;
        g.wg_socket_raw = None;
        if let Some(fd) = g.wg_socket_owned.as_ref() {
            crate::n2_socket_trace::closed(fd.as_raw_fd());
        }
        g.wg_socket_owned = None;
        g.tun_raw = None;
        if let Some(tee) = &g.n2_dest_tee {
            tee.gap(sys::mono_ms(), sys::mono_ms(), "connector-stop");
        }
        g.n2_dest_tee = None;
        g.n2_gap_start = None;
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
        // pre-device: validate the fd is alive, then buffer (latest wins).
        // `probe_fd_dupable` drops the probe's trace registration BEFORE it
        // closes the probe number — otherwise the freed number keeps a
        // registration pointing at a closed fd, and whatever socket the
        // kernel hands that number to next inherits its kind/request_id/
        // original_fd (the N2 fd-reuse contract).
        if let Err(e) = crate::mgmtsock::probe_fd_dupable(raw_fd) {
            return Err(format!("wg-egress-fd-invalid (errno={})", e.errno()));
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

    /// STAGE 3 (official-alignment audit §2.7): the live device's outer-socket
    /// fd — what the ONE shared mux is built over.
    fn wg_socket_fd(&self) -> Option<i32> {
        WgDeviceFeed::socket_fd(self)
    }

    /// STAGE 3: the mux's reader thread becomes the socket's ONE reader.
    fn set_shared_socket_reader(&self, on: bool) -> bool {
        WgDeviceFeed::set_shared_socket_reader(self, on)
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

    /// N2 T5a: drop every peer's selected path on the live device, and the
    /// buffered endpoint/egress records with it (pre-device the buffer IS the
    /// only path state). Returns the peers that actually had a path.
    fn reset_peer_paths(&self) -> usize {
        let mut g = self.inner.lock_poison();
        let buffered = g.endpoints.len();
        g.endpoints.clear();
        g.egress.clear();
        match g.device.as_mut() {
            Some(d) => d.reset_peer_paths(),
            None => buffered,
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

    /// Guard `PartiallyConnected` input (see the trait method's contract):
    /// the lane exists as soon as it is injected — the pre-device buffer
    /// ([`WgDeviceFeed::attach_carrier`]'s `carriers`) counts exactly like a
    /// lane the live device holds, or a lane injected while the device is
    /// still being built would read "no relay".
    fn carrier_attached(&self, pub_key_b64: &str) -> bool {
        let g = self.inner.lock_poison();
        if g.carriers.iter().any(|(k, _)| k == pub_key_b64) {
            return true;
        }
        match g.device.as_ref() {
            Some(d) => d.with_device(|dev| dev.peer_carrier_attached(pub_key_b64)),
            None => false,
        }
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
            let (ready, sessions, peers, stats) = d.with_device(|dev| {
                // N13-D1: ONE peer snapshot — the aggregate count and the
                // per-peer attribution are read from the same view, so they
                // can never disagree in one document.
                let peers = dev.peers();
                let sessions = peers
                    .iter()
                    .filter(|p| p.session_established && !p.expired)
                    .count();
                (dev.tunnel_ready(), sessions, peers, dev.stats())
            });
            st.ready = ready;
            st.peers_with_session = sessions;
            st.wg_peers = peers;
            st.handshakes = stats.handshake_initiations;
            st.tx_packets = stats.tx_packets;
            st.tx_bytes = stats.tx_bytes;
            st.rx_packets = stats.rx_packets;
            st.rx_bytes_to_tun = stats.rx_bytes_to_tun;
            st.dropped_no_route = stats.no_route_drops;
            st.decrypt_errors = stats.decrypt_errors;
            st.unknown_peer_drops = stats.unknown_peer_drops;
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

    // N2 fd-reuse contract: the pre-device egress probe is a DUP, and its
    // registration must be gone from the armed trace table BEFORE its number
    // is closed — otherwise the freed number keeps an entry pointing at a
    // closed fd, and whatever socket the kernel hands that number to next
    // inherits this probe's kind/request_id/original_fd. Observed through the
    // public per-original view (the residue is a SECOND registration against
    // the same shell original), so the test needs no fd-number assumption.
    #[test]
    fn pre_device_egress_probe_leaves_no_registration_behind() {
        let _guard = crate::n2_stimulus::test_guard();
        let slot = WgDeviceFeed::new(WgDeviceConfig::new(test_secret_b64(0xA5)));
        slot.apply_peers(&[peer_entry("peer-egress", [10, 77, 0, 2])]).expect("buffered peers");
        let egress = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM | sys::SOCK_CLOEXEC, 0) };
        assert!(egress >= 0, "errno={}", sys::errno());

        crate::n2_socket_trace::arm(true);
        // The selected egress socket at the WG boundary (Ws = selected WG).
        crate::n2_socket_trace::protect(egress, "Ws", "ice-selected", 1, 31, 2, true);
        slot.attach_egress_socket("peer-egress", egress).expect("buffered egress");

        let rows = crate::n2_socket_trace::fd_send_totals_for_original(egress);
        assert_eq!(
            rows,
            vec![(egress, 0, 0)],
            "the closed probe must leave no second registration against the same original"
        );
        crate::n2_socket_trace::arm(false);
        unsafe { sys::close(egress) };
    }

    #[test]
    fn control_marker_drops_before_wg_and_production_still_routes() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let dst = [119, 29, 29, 119];
        assert!(slot.set_n2_destination_targets(vec![dst]));
        assert!(!slot.set_n2_destination_targets(vec![dst]));
        assert!(slot.set_n2_outer_targets(vec![(dst, 17, 53)]));
        slot.feed_wg_socket(wg_raw).unwrap();
        slot.feed_tun(tun_raw).unwrap();
        assert!(!slot.set_n2_destination_targets(vec![dst]));
        assert!(!slot.set_n2_outer_targets(vec![(dst, 17, 53)]));
        let tee = slot.n2_control_tee().unwrap();
        let now = sys::mono_ms();
        tee.register_control("pre", dst, 53, 17, 0, now).unwrap();
        let mut packet = vec![0u8; 28 + b"N2CTRL:pre".len()];
        packet[0] = 0x45;
        let len = packet.len() as u16;
        packet[2..4].copy_from_slice(&len.to_be_bytes());
        packet[9] = 17;
        packet[12..16].copy_from_slice(&[10, 77, 0, 1]);
        packet[16..20].copy_from_slice(&dst);
        packet[20..22].copy_from_slice(&40000u16.to_be_bytes());
        packet[22..24].copy_from_slice(&53u16.to_be_bytes());
        packet[24..26].copy_from_slice(&(len - 20).to_be_bytes());
        packet[28..].copy_from_slice(b"N2CTRL:pre");
        slot.with_device(|d| {
            assert_eq!(unsafe { sys::write(tun_end, packet.as_ptr().cast(), packet.len()) }, packet.len() as isize);
            assert_eq!(d.service_tun(now), 1);
            assert_eq!(d.stats.no_route_drops, 0);
        }).unwrap();
        assert_eq!(tee.control_status("pre"), "hit");
        assert_eq!(tee.control_totals(), (1, 1, 0));
        packet[28..].copy_from_slice(b"PRODUCTION");
        slot.with_device(|d| {
            assert_eq!(unsafe { sys::write(tun_end, packet.as_ptr().cast(), packet.len()) }, packet.len() as isize);
            assert_eq!(d.service_tun(now + 1), 1);
            assert_eq!(d.stats.no_route_drops, 1);
        }).unwrap();
        assert_eq!(tee.control_totals(), (2, 1, 1));
        assert_eq!(tee.leak_count(), 1);
        for fd in [wg_raw, tun_raw, tun_end] { unsafe { sys::close(fd) }; }
    }

    #[test]
    fn session_destination_observes_tun_after_collector_detach() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let destination = [119, 29, 29, 119];
        slot.set_n2_destination_targets(vec![destination]);
        slot.feed_wg_socket(wg_raw).expect("wg feed");
        slot.feed_tun(tun_raw).expect("tun feed");
        let observer = slot.inner.lock_poison().n2_dest_tee.as_ref().unwrap().clone();
        let mut packet = [0u8; 28];
        packet[0] = 0x45;
        packet[2..4].copy_from_slice(&(28u16).to_be_bytes());
        packet[8] = 64;
        packet[9] = 17;
        packet[12..16].copy_from_slice(&[10, 77, 0, 1]);
        packet[16..20].copy_from_slice(&destination);
        packet[20..22].copy_from_slice(&40000u16.to_be_bytes());
        packet[22..24].copy_from_slice(&53u16.to_be_bytes());
        packet[24..26].copy_from_slice(&8u16.to_be_bytes());
        slot.with_device(|d| {
            d.attach_tun_tee(Arc::new(TunTeeCounts::default()));
            assert_eq!(unsafe { sys::write(tun_end, packet.as_ptr().cast(), packet.len()) }, 28);
            d.service_tun(1);
            d.detach_tun_tee();
            assert!(!d.tun_tee_attached());
            assert_eq!(unsafe { sys::write(tun_end, packet.as_ptr().cast(), packet.len()) }, 28);
            d.service_tun(2);
        }).expect("live device");
        let lines = observer.due_lines(sys::mono_ms().saturating_add(5_000));
        assert!(lines[0].contains("frames=2|delta=2"), "{lines:?}");
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    #[test]
    fn session_destination_reports_tee_continuity_counters_and_gap_fields() {
        // N2 T7a (item 3): the tee's own continuity is reported, so an
        // interval can be judged complete (`decode_fail=0`) instead of merely
        // unreported, and the gap lines carry the `from_mono`/`to_mono` names.
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let destination = [119, 29, 29, 119];
        slot.set_n2_destination_targets(vec![destination]);
        slot.feed_wg_socket(wg_raw).expect("wg feed");
        slot.feed_tun(tun_raw).expect("tun feed");
        let observer = slot.inner.lock_poison().n2_dest_tee.as_ref().unwrap().clone();
        let mut target = [0u8; 28];
        target[0] = 0x45;
        target[2..4].copy_from_slice(&(28u16).to_be_bytes());
        target[8] = 64;
        target[9] = 17;
        target[12..16].copy_from_slice(&[10, 77, 0, 1]);
        target[16..20].copy_from_slice(&destination);
        target[20..22].copy_from_slice(&40000u16.to_be_bytes());
        target[22..24].copy_from_slice(&53u16.to_be_bytes());
        target[24..26].copy_from_slice(&8u16.to_be_bytes());
        // a decodable frame outside the frozen capture set
        let mut other = target;
        other[16..20].copy_from_slice(&[10, 0, 0, 1]);
        // an undecodable frame (shorter than any IPv4 header)
        let garbage = [0u8; 5];
        // diag-0048: two more undecodable shapes, one per version bucket — a
        // version-6 frame (byte0 0x6x) and a version-4 frame whose IHL is
        // below 5 (byte0 0x44).
        let mut v6 = [0u8; 22];
        v6[0] = 0x60;
        let mut bad_ihl = [0u8; 24];
        bad_ihl[0] = 0x44;
        slot.with_device(|d| {
            assert_eq!(unsafe { sys::write(tun_end, target.as_ptr().cast(), target.len()) }, 28);
            assert_eq!(unsafe { sys::write(tun_end, other.as_ptr().cast(), other.len()) }, 28);
            assert_eq!(unsafe { sys::write(tun_end, garbage.as_ptr().cast(), garbage.len()) }, 5);
            assert_eq!(unsafe { sys::write(tun_end, v6.as_ptr().cast(), v6.len()) }, 22);
            assert_eq!(unsafe { sys::write(tun_end, bad_ihl.as_ptr().cast(), bad_ihl.len()) }, 24);
            d.service_tun(1);
        }).expect("live device");
        // diag-0048: the frozen fields keep their names/order; the new witness
        // fields are appended at the line tail. The witness is the FIRST
        // failure (len 5, byte0 0x00), never overwritten by the later ones.
        // R1: the 22-byte version-6 frame is too short to carry a header, so it
        // is `ipv6_other` (scope `other`, chain bucket `hbh`) with a header
        // witness of the bytes the frame actually carries — none at the
        // destination offset.
        assert_eq!(observer.stats_line(4_242),
            "N2_TUN_TEE_STATS|frames=5|decode_fail=3|dropped=1|mono_ms=4242|diag=true|is_evidence=false|first_fail_len=5|first_fail_byte0=0x00|first_fail_prefix=00000000|fail_v4=1|fail_v6=1|fail_other=1|ipv6_out_of_scope=0|ipv6_other=1|ipv6_scope=linklocal:0,multicast:0,other:1|ipv6_next=icmp6:0,hbh:1,udp:0,tcp:0,other:0|first_v6_len=22|first_v6_dst_prefix=|first_v6_other_len=22|first_v6_other_dst=|first_v6_other_next=-|first_v6_other_dport=-");
        // no failure yet: the witness fields are present but empty, so a parser
        // keyed on the field names never special-cases their absence.
        assert_eq!(N2DestinationTee::new(vec![destination], 0).stats_line(7),
            "N2_TUN_TEE_STATS|frames=0|decode_fail=0|dropped=0|mono_ms=7|diag=true|is_evidence=false|first_fail_len=|first_fail_byte0=|first_fail_prefix=|fail_v4=0|fail_v6=0|fail_other=0|ipv6_out_of_scope=0|ipv6_other=0|ipv6_scope=linklocal:0,multicast:0,other:0|ipv6_next=icmp6:0,hbh:0,udp:0,tcp:0,other:0|first_v6_len=|first_v6_dst_prefix=|first_v6_other_len=|first_v6_other_dst=|first_v6_other_next=|first_v6_other_dport=");
        // the invariant reconciles: 5 = 1 decoded-target + 1 dropped + 3 decode failures
        let dest = observer.due_lines(sys::mono_ms().saturating_add(5_000));
        assert_eq!(dest.len(), 1, "{dest:?}");
        assert!(dest[0].contains("|frames=1|delta=1|"), "{dest:?}");
        for line in [gap_line(10, 20, "before-tun-attach"), gap_line(30, 30, "connector-stop"),
            gap_line(40, 55, "tun-replaced")] {
            assert!(line.starts_with("N2_TUN_GAP|from_mono="), "{line}");
            assert!(line.ends_with("|diag=true|is_evidence=false"), "{line}");
        }
        assert_eq!(gap_line(10, 20, "before-tun-attach"),
            "N2_TUN_GAP|from_mono=10|to_mono=20|reason=before-tun-attach|diag=true|is_evidence=false");
        assert_eq!(dest_zero_line(9, destination),
            "N2_TUN_DEST|scope=session|mono_ms=9|dst=119.29.29.119|proto=any|port=any|frames=0|delta=0|sources=|diag=true|is_evidence=false");
        assert_eq!(frames_line(9, 3, 0, 2, 0),
            "N2_TUN_FRAMES|scope=session|mono_ms=9|seen=3|control=0|production=2|leaks=0|diag=true|is_evidence=false");
        assert_eq!(wg_feed_adopted_line(2, 1, 1, 77),
            "N7_WG_FEED|adopted|peers=2|endpoints=1|egress_replayed=1|mono_ms=77");
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    // ---- R1 (approved n2-tun-ipv6-decode-amendment, 2026-10-01) --------------

    /// Build one IPv6 frame: 40-byte header + `body`.
    fn ipv6_frame(next_header: u8, hop_limit: u8, dst: [u8; 16], body: &[u8]) -> Vec<u8> {
        let mut f = vec![0x60, 0, 0, 0];
        f.extend_from_slice(&(body.len() as u16).to_be_bytes());
        f.push(next_header);
        f.push(hop_limit);
        f.extend_from_slice(&[0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        f.extend_from_slice(&dst);
        f.extend_from_slice(body);
        f
    }

    /// An ICMPv6 message of `len` bytes whose type byte is `kind`.
    fn icmpv6(kind: u8, len: usize) -> Vec<u8> {
        let mut body = vec![0u8; len];
        body[0] = kind;
        body
    }

    /// An 8-byte Hop-by-Hop header whose own next header is `tail`, followed by
    /// `body` (which then starts at offset 48).
    fn hbh(tail: u8, body: &[u8]) -> Vec<u8> {
        let mut out = vec![tail, 0, 0, 0, 0, 0, 0, 0];
        out.extend_from_slice(body);
        out
    }

    const SOLICITED_NODE_DST: [u8; 16] = [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0xff, 0, 0, 1];
    const ALL_ROUTERS_DST: [u8; 16] = [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2];
    const MLDV2_DST: [u8; 16] = [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x16];
    const LINK_LOCAL_ONE_DST: [u8; 16] = [0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    const GLOBAL_ONE_DST: [u8; 16] = [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];

    /// One `key=value` field of a `N2_TUN_TEE_STATS` line.
    fn stats_field(line: &str, key: &str) -> String {
        let prefix = format!("{key}=");
        line.split('|')
            .find_map(|field| field.strip_prefix(&prefix))
            .unwrap_or_else(|| panic!("no {key} in {line}"))
            .to_string()
    }

    /// Σ of a histogram value such as `icmp6:3,hbh:1,udp:0,tcp:0,other:0`.
    fn histogram_sum(value: &str) -> u64 {
        value
            .split(',')
            .map(|bucket| {
                bucket
                    .rsplit_once(':')
                    .unwrap_or_else(|| panic!("no count in {bucket}"))
                    .1
                    .parse::<u64>()
                    .expect("histogram count")
            })
            .sum()
    }

    /// R2(i) — the four sums must be equal on every stats line:
    /// `Σipv6_scope == Σipv6_next == fail_v6 == ipv6_out_of_scope + ipv6_other`.
    fn assert_tee_stats_reconcile(line: &str) {
        let fail_v6: u64 = stats_field(line, "fail_v6").parse().expect("fail_v6");
        let out_of_scope: u64 =
            stats_field(line, "ipv6_out_of_scope").parse().expect("ipv6_out_of_scope");
        let other: u64 = stats_field(line, "ipv6_other").parse().expect("ipv6_other");
        let scope = histogram_sum(&stats_field(line, "ipv6_scope"));
        let next = histogram_sum(&stats_field(line, "ipv6_next"));
        assert_eq!(scope, next, "Σipv6_scope != Σipv6_next: {line}");
        assert_eq!(next, fail_v6, "Σipv6_next != fail_v6: {line}");
        assert_eq!(fail_v6, out_of_scope + other, "fail_v6 != out_of_scope + other: {line}");
    }

    /// Five version-6 control frames — the 76 B Hop-by-Hop + MLDv2 shape seen on
    /// the device (diag-0048/0049/0050/0051) plus NS / RS / NA — must be
    /// registered as out of scope while `decode_fail` keeps its frozen meaning
    /// and the frozen invariant `frames = decode_fail + dropped + Σ N2_TUN_DEST`
    /// still holds.
    #[test]
    fn session_tee_stats_registers_link_scoped_ipv6_control_frames_out_of_scope() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let destination = [119, 29, 29, 119];
        slot.set_n2_destination_targets(vec![destination]);
        slot.feed_wg_socket(wg_raw).expect("wg feed");
        slot.feed_tun(tun_raw).expect("tun feed");
        let observer = slot.inner.lock_poison().n2_dest_tee.as_ref().unwrap().clone();
        let mldv2 = ipv6_frame(0, 1, MLDV2_DST, &hbh(58, &icmpv6(143, 28)));
        assert_eq!(mldv2.len(), 76, "the observed first-frame shape");
        let frames = [
            mldv2.clone(),
            mldv2.clone(),
            ipv6_frame(58, 255, SOLICITED_NODE_DST, &icmpv6(135, 32)),
            ipv6_frame(58, 255, ALL_ROUTERS_DST, &icmpv6(133, 8)),
            ipv6_frame(58, 255, LINK_LOCAL_ONE_DST, &icmpv6(136, 32)),
        ];
        slot.with_device(|d| {
            for f in &frames {
                assert_eq!(
                    unsafe { sys::write(tun_end, f.as_ptr().cast(), f.len()) },
                    f.len() as isize
                );
            }
            d.service_tun(1);
        }).expect("live device");
        let line = observer.stats_line(4_242);
        // four multicast destinations (2×ff02::16, solicited-node, ff02::2) and
        // one link-local unicast destination, all with an ICMPv6 chain tail; the
        // FIRST refused frame is the 76 B one, so `first_fail_*` and `first_v6_*`
        // witness the same frame and `first_v6_other_*` (no non-control frame in
        // this interval) stays empty.
        assert_eq!(line,
            "N2_TUN_TEE_STATS|frames=5|decode_fail=5|dropped=0|mono_ms=4242|diag=true|is_evidence=false|first_fail_len=76|first_fail_byte0=0x60|first_fail_prefix=60000000|fail_v4=0|fail_v6=5|fail_other=0|ipv6_out_of_scope=5|ipv6_other=0|ipv6_scope=linklocal:1,multicast:4,other:0|ipv6_next=icmp6:5,hbh:0,udp:0,tcp:0,other:0|first_v6_len=76|first_v6_dst_prefix=ff02000000000000|first_v6_other_len=|first_v6_other_dst=|first_v6_other_next=|first_v6_other_dport=");
        assert_tee_stats_reconcile(&line);
        // the frozen invariant: 5 = decode_fail(5) + dropped(0) + Σ N2_TUN_DEST(0)
        let dest = observer.due_lines(sys::mono_ms().saturating_add(60_000));
        assert_eq!(dest.len(), 1, "{dest:?}");
        assert!(dest[0].contains("|frames=0|delta=0|"), "{dest:?}");
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    /// A version-6 UDP datagram to a GLOBAL destination is NOT a control frame:
    /// it stays `ipv6_other` and the R4 witness carries its FULL 16-byte
    /// destination, its chain-tail protocol number and its destination port.
    #[test]
    fn session_tee_stats_names_a_non_control_ipv6_frame_with_a_full_destination() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let destination = [119, 29, 29, 119];
        slot.set_n2_destination_targets(vec![destination]);
        slot.feed_wg_socket(wg_raw).expect("wg feed");
        slot.feed_tun(tun_raw).expect("tun feed");
        let observer = slot.inner.lock_poison().n2_dest_tee.as_ref().unwrap().clone();
        let mut udp = vec![0u8; 8];
        udp[2..4].copy_from_slice(&53u16.to_be_bytes());
        let frame = ipv6_frame(17, 64, GLOBAL_ONE_DST, &udp);
        assert_eq!(frame.len(), 48);
        slot.with_device(|d| {
            assert_eq!(
                unsafe { sys::write(tun_end, frame.as_ptr().cast(), frame.len()) },
                frame.len() as isize
            );
            d.service_tun(1);
        }).expect("live device");
        let line = observer.stats_line(4_242);
        assert!(line.contains("|frames=1|decode_fail=1|dropped=0|"), "{line}");
        assert!(line.contains("|fail_v4=0|fail_v6=1|fail_other=0|"), "{line}");
        assert!(line.contains("|ipv6_out_of_scope=0|ipv6_other=1|"), "{line}");
        assert!(line.contains("|ipv6_scope=linklocal:0,multicast:0,other:1|"), "{line}");
        assert!(line.contains("|ipv6_next=icmp6:0,hbh:0,udp:1,tcp:0,other:0|"), "{line}");
        assert!(line.contains("|first_v6_len=48|first_v6_dst_prefix=20010db800000000|"), "{line}");
        assert!(line.contains("|first_v6_other_len=48|"), "{line}");
        // the destination is recorded in FULL: 16 bytes = 32 lowercase hex chars
        let dst_hex = stats_field(&line, "first_v6_other_dst");
        assert_eq!(dst_hex, "20010db8000000000000000000000001");
        assert_eq!(dst_hex.len(), 32);
        assert_eq!(stats_field(&line, "first_v6_other_next"), "17");
        assert_eq!(stats_field(&line, "first_v6_other_dport"), "53");
        assert_tee_stats_reconcile(&line);
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
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
             \"dropped_no_route\":0,\"decrypt_errors\":0,\"unknown_peer_drops\":0,\
             \"carrier_tx_packets\":0,\
             \"carrier_tx_bytes\":0,\"carrier_rejects\":0,\"carrier_takeovers\":0,\
             \"direct_restores\":0,\"wg_peers\":[]}");

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
    fn feed_slot_replaces_live_wg_socket_and_keeps_old_on_failure() {
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        slot.feed_tun(tun_raw).unwrap();
        slot.feed_wg_socket(wg_raw).unwrap();
        let old = slot.with_device(|d| (d.fd.unwrap(), d.local_addr())).unwrap();
        assert_eq!(slot.feed_wg_socket(1 << 30).unwrap_err().token(), "socket-fd-invalid");
        assert_eq!(slot.feed_wg_socket(tun_raw).unwrap_err().token(), "socket-fd-conflict");
        assert_eq!(slot.with_device(|d| (d.fd.unwrap(), d.local_addr())).unwrap(), old);
        let (replacement, unused_tun, unused_end) = fed_fds();
        slot.feed_wg_socket(replacement).unwrap();
        let current = slot.with_device(|d| (d.fd.unwrap(), d.local_addr())).unwrap();
        assert_ne!(current.0, old.0);
        assert_ne!(current.1, old.1);
        assert!(slot.device_up());
        for fd in [wg_raw, tun_raw, tun_end, replacement, unused_tun, unused_end] {
            unsafe { sys::close(fd) };
        }
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

    // -----------------------------------------------------------------------
    // N2 v4.1 P7: the outer-source receipt of the WG outer socket
    // -----------------------------------------------------------------------

    /// One loopback UDP socket: `(fd, bound port)`. The caller closes the fd.
    fn loopback_udp() -> (i32, u16) {
        let fd = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(fd >= 0);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(
            unsafe { sys::bind(fd, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0
        );
        let mut name = sys::sockaddr_in::new([0, 0, 0, 0], 0);
        let mut len = core::mem::size_of::<sys::sockaddr_in>() as u32;
        assert_eq!(unsafe { sys::getsockname(fd, &mut name, &mut len) }, 0);
        (fd, u16::from_be(name.sin_port))
    }

    /// A real loopback datagram to `port` — the "device behind NAT" stand-in.
    fn send_loopback_udp(from: i32, port: u16, payload: &[u8]) {
        let dst = sys::sockaddr_in::new([127, 0, 0, 1], port);
        let n = unsafe {
            sys::sendto(
                from,
                payload.as_ptr() as *const core::ffi::c_void,
                payload.len(),
                0,
                &dst,
                core::mem::size_of::<sys::sockaddr_in>() as u32,
            )
        };
        assert_eq!(n, payload.len() as isize);
    }

    #[test]
    fn outer_receipt_reports_known_and_unknown_sources_and_stays_silent_when_off() {
        let _g = crate::n2_outer_receipt::global_lock();
        let (slot, wg_raw, tun_raw, tun_end) = test_slot();
        let key = base64::engine::general_purpose::STANDARD.encode([0x7Au8; 32]);
        let prefix: String = key.chars().filter(|c| c.is_ascii_alphanumeric()).take(8).collect();
        slot.apply_peers(&[peer_entry(&key, [10, 7, 0, 2])]).expect("peers");
        slot.feed_tun(tun_raw).expect("tun");
        slot.feed_wg_socket(wg_raw).expect("wg");
        let (dev_ip, dev_port) = slot.with_device(|d| d.local_addr()).expect("device up");
        assert_eq!(dev_ip, [127, 0, 0, 1]);

        // the registered peer (endpoint match) + a stranger (off-path source)
        let (peer_fd, peer_port) = loopback_udp();
        let (stranger_fd, stranger_port) = loopback_udp();
        slot.apply_endpoint(&key, [127, 0, 0, 1], peer_port).expect("endpoint");

        // DISARMED negative control: the datagram IS consumed by the device
        // (unknown_peer_drops moves) and produces NO line at all.
        let drops_before = slot.with_device(|d| d.stats.unknown_peer_drops).expect("stats");
        send_loopback_udp(stranger_fd, dev_port, &[4u8; 32]);
        assert_eq!(slot.with_device(|d| d.service_udp(sys::mono_ms())).expect("device"), 1);
        assert_eq!(slot.with_device(|d| d.stats.unknown_peer_drops).expect("stats"), drops_before + 1);
        assert!(crate::n2_outer_receipt::emitted_lines().is_empty(), "switch off ⇒ zero output");

        // ARMED: the registered endpoint is named, the stranger is `unknown`
        crate::n2_outer_receipt::arm(true);
        send_loopback_udp(peer_fd, dev_port, &[1u8; 32]);
        assert_eq!(slot.with_device(|d| d.service_udp(sys::mono_ms())).expect("device"), 1);
        let lines = crate::n2_outer_receipt::emitted_lines();
        let known: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains(&format!("|src=127.0.0.1:{peer_port}|")))
            .collect();
        assert_eq!(known.len(), 1, "{lines:?}");
        assert!(known[0].starts_with("n2h-outer-receipt|first|"), "{}", known[0]);
        assert!(known[0].contains(&format!("|local=127.0.0.1:{dev_port}|")), "{}", known[0]);
        assert!(known[0].contains("|type=wg-init|"), "{}", known[0]);
        assert!(known[0].contains(&format!("|peer={prefix}|")), "{}", known[0]);

        send_loopback_udp(stranger_fd, dev_port, &[4u8; 32]);
        assert_eq!(slot.with_device(|d| d.service_udp(sys::mono_ms())).expect("device"), 1);
        let lines = crate::n2_outer_receipt::emitted_lines();
        let unknown: Vec<&String> = lines
            .iter()
            .filter(|l| l.contains(&format!("|src=127.0.0.1:{stranger_port}|")))
            .collect();
        assert_eq!(unknown.len(), 1, "{lines:?}");
        assert!(unknown[0].contains("|type=wg-data|"), "{}", unknown[0]);
        assert!(unknown[0].contains("|peer=unknown|"), "{}", unknown[0]);

        // throttle: a second datagram of the same type inside the 5 s window
        // produces nothing new for that source
        send_loopback_udp(stranger_fd, dev_port, &[4u8; 32]);
        assert_eq!(slot.with_device(|d| d.service_udp(sys::mono_ms())).expect("device"), 1);
        let after_lines = crate::n2_outer_receipt::emitted_lines();
        let after: Vec<&String> = after_lines
            .iter()
            .filter(|l| l.contains(&format!("|src=127.0.0.1:{stranger_port}|")))
            .collect();
        assert_eq!(after.len(), 1, "no per-packet spam: {after:?}");

        crate::n2_outer_receipt::arm(false);
        for fd in [wg_raw, tun_raw, tun_end, peer_fd, stranger_fd] {
            unsafe { sys::close(fd) };
        }
    }
}

// ---------------------------------------------------------------------------
// tests: the WG session-lifecycle line format (0050 §8 item 2)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod lifecycle_line_tests {
    use super::*;

    /// 0050 §8 item 2 的格式钉子：`session-established` / `session-expired`
    /// 必须带 `peer=<公钥前8位>`，且**追加在行尾**、已有字段顺序不变、与
    /// N5c_ICE 行同一个短前缀规则（`peer_conn::peer_short`）。
    ///
    /// 这两行不是 N2 行、不进 `n2-log-samples.txt`（那个夹具只钉 Rust 侧
    /// N2 行），所以格式断言落在它们的生产者模块里：`wg_device` 的会话生命
    /// 周期行此前是裸标记（`N6_WG_DEVICE|session-established`），真机上 6 个
    /// peer 都建/拆会话时无法判定是哪几个。
    #[test]
    fn session_lifecycle_lines_name_the_peer_last() {
        // 真机形态的公钥（base64）：`peer_short` 只取字母数字，`+`/`/`/`=`
        // 跳过——与 N5c_ICE 行的短 id 逐字同规则（0050 证据里
        // `FyLw4PSBg1hjr…` 的行内短 id 就是 `FyLw4PSB`）。
        let key = "FyLw4PSBg1hjr+9/x0=";
        assert_eq!(
            session_established_line(key),
            "N6_WG_DEVICE|session-established|peer=FyLw4PSB"
        );
        assert_eq!(
            session_expired_line(key),
            "N6_WG_DEVICE|session-expired|peer=FyLw4PSB"
        );
        for line in [session_established_line(key), session_expired_line(key)] {
            let parts: Vec<&str> = line.split('|').collect();
            assert_eq!(parts[0], "N6_WG_DEVICE");
            assert_eq!(parts.len(), 3, "既有字段顺序不变、peer= 在行尾: {line}");
            assert!(parts.last().expect("last field").starts_with("peer="), "{line}");
        }
        // 短/空 key 不 panic（前缀就是它自己的字母数字前缀）。
        assert_eq!(session_established_line("ab"), "N6_WG_DEVICE|session-established|peer=ab");
        assert_eq!(session_established_line(""), "N6_WG_DEVICE|session-established|peer=");
    }

    /// 真机校验用的 roaming 行：上游 `peer.SetEndpointFromPacket` 的唯一落点
    /// （只有认证消费的包会走到这里）。`from=none` = 对端此前没有 endpoint，
    /// 会话从零建立；`from=`/`to=` 都是公开路由信息。
    #[test]
    fn roam_line_names_the_peer_and_both_addresses() {
        let key = "FyLw4PSBg1hjr+9/x0=";
        assert_eq!(
            wg_roam_line(key, None, ([100, 64, 0, 7], 51820)),
            "N6_WG_DEVICE|roam|peer=FyLw4PSB|from=none|to=100.64.0.7:51820"
        );
        assert_eq!(
            wg_roam_line(key, Some(([192, 168, 1, 9], 41234)), ([100, 64, 0, 7], 51820)),
            "N6_WG_DEVICE|roam|peer=FyLw4PSB|from=192.168.1.9:41234|to=100.64.0.7:51820"
        );
        assert_eq!(endpoint_text(None), "none");
        assert_eq!(endpoint_text(Some(([0, 0, 0, 0], 0))), "0.0.0.0:0");
    }
}

// ---------------------------------------------------------------------------
// tests: the managed-route overlay (N6 route table defect, 2026-10-04:
// LAN dst hit `no-route` because managed networks never entered any peer's
// allowed_ips — hilog: dst=192.168.50.x|peers=6|routes=[…/32 only])
// ---------------------------------------------------------------------------

#[cfg(test)]
mod route_overlay_tests {
    use super::*;
    use crate::config;
    use crate::network_map::ManagedRoute;

    extern "C" {
        fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    }

    /// The HA routed network of the field defect (192.168.50.0/24 behind two
    /// routing peers).
    const LAN_NET: ([u8; 4], u8) = ([192, 168, 50, 0], 24);

    fn key32(seed: u8) -> String {
        base64::engine::general_purpose::STANDARD.encode([seed; 32])
    }

    fn peer_entry(key: &str, addr: [u8; 4]) -> crate::connector::WgPeerEntry {
        crate::connector::WgPeerEntry {
            pub_key_b64: key.to_string(),
            allowed_ips: vec![config::Route { addr, prefix_len: 32 }],
        }
    }

    fn lan_route(peer: &str, metric: i64) -> ManagedRoute {
        ManagedRoute {
            id: format!("r-lan-{peer}"),
            network: config::Route { addr: LAN_NET.0, prefix_len: LAN_NET.1 },
            domains: vec![],
            net_id: "net-lan".into(),
            network_type: 1,
            peer: peer.into(),
            metric,
            masquerade: true,
            keep_route: false,
            skip_auto_apply: false,
        }
    }

    /// Fed slot (device up) + the raw fds the caller closes at the end.
    fn fed_slot() -> (WgDeviceFeed, i32, i32, i32) {
        let wg = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
        assert!(wg >= 0);
        let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
        assert_eq!(
            unsafe { sys::bind(wg, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32) },
            0
        );
        let mut sv = [-1i32; 2];
        assert_eq!(unsafe { socketpair(1, 2, 0, &mut sv) }, 0);
        let slot = WgDeviceFeed::new(
            WgDeviceConfig::new(base64::engine::general_purpose::STANDARD.encode([0xA5u8; 32])),
        );
        slot.feed_wg_socket(wg).expect("wg feed");
        slot.feed_tun(sv[0]).expect("tun feed");
        (slot, wg, sv[0], sv[1])
    }

    /// The defect itself: with the map's allowed_ips ONLY (overlay /32s) the
    /// LAN destination hits `no-route`; with the managed routes applied the
    /// WG routing table carries 192.168.50.0/24 on ONE routing peer (smaller
    /// metric wins when no session exists) and 192.168.50.1 routes to it.
    /// A route-set change back to empty removes the overlay again.
    #[test]
    fn managed_route_overlay_routes_lan_dst_to_the_metric_peer() {
        let (slot, wg_raw, tun_raw, tun_end) = fed_slot();
        let key_a = key32(0x11);
        let key_b = key32(0x22);
        slot.apply_peers(&[
            peer_entry(&key_a, [100, 108, 1, 1]),
            peer_entry(&key_b, [100, 108, 2, 2]),
        ])
        .expect("peers");
        // pre-fix shape: nothing matches the LAN dst
        slot.with_device(|d| assert!(d.route([192, 168, 50, 1]).is_none()))
            .expect("device");
        // HA: the same network from two route entries (two routing peers).
        slot.apply_managed_routes(&[lan_route(&key_a, 10), lan_route(&key_b, 20)])
            .expect("managed routes");
        slot.with_device(|d| {
            assert_eq!(d.managed_route_selections(), vec![(LAN_NET, Some(key_a.clone()))]);
            let idx = d.route([192, 168, 50, 1]).expect("192.168.50.1 must route now");
            assert_eq!(d.peers[idx].key_b64, key_a);
            assert!(d.peers[idx].allowed_ips.contains(&LAN_NET));
            assert!(
                !d.peers
                    .iter()
                    .any(|p| p.key_b64 == key_b && p.allowed_ips.contains(&LAN_NET)),
                "the same net must sit on exactly one peer"
            );
            // the map /32s stay untouched beside the overlay net
            assert!(d.peers[idx].allowed_ips.contains(&([100, 108, 1, 1], 32)));
        })
        .expect("device");
        // route-set change: the overlay leaves with the routes
        slot.apply_managed_routes(&[]).expect("clear managed routes");
        slot.with_device(|d| {
            assert!(d.managed_route_selections().is_empty());
            assert!(d.route([192, 168, 50, 1]).is_none());
            assert!(d.peers.iter().all(|p| !p.allowed_ips.contains(&LAN_NET)));
            // the map /32s survive the overlay removal untouched
            let a = d.peers.iter().position(|p| p.key_b64 == key_a).expect("peer a");
            assert_eq!(d.peers[a].allowed_ips, vec![([100, 108, 1, 1], 32)]);
        })
        .expect("device");
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    /// HA failover: B holds the smaller metric (wins while idle); A's LIVE
    /// session outranks the metric; when A's session EXPIRES (the real `tick`
    /// path) the net fails over to B; A re-establishing wins it back.
    #[test]
    fn managed_route_ha_prefers_live_session_and_fails_over_on_expiry() {
        let (slot, wg_raw, tun_raw, tun_end) = fed_slot();
        let key_a = key32(0x11);
        let key_b = key32(0x22);
        slot.apply_peers(&[
            peer_entry(&key_a, [100, 108, 1, 1]),
            peer_entry(&key_b, [100, 108, 2, 2]),
        ])
        .expect("peers");
        slot.apply_managed_routes(&[lan_route(&key_a, 20), lan_route(&key_b, 10)])
            .expect("managed routes");
        // idle: the smaller metric (B) wins
        slot.with_device(|d| {
            assert_eq!(d.managed_route_selections(), vec![(LAN_NET, Some(key_b.clone()))]);
        })
        .expect("device");
        // A establishes a WG session: live session > metric
        slot.with_device(|d| {
            let a = d.peers.iter().position(|p| p.key_b64 == key_a).expect("peer a");
            d.peers[a].session_established = true;
            d.peers[a].expired = false;
            d.peers[a].session_established_ms = 1;
            d.rebuild_route_overlay("session-established");
            assert_eq!(d.managed_route_selections(), vec![(LAN_NET, Some(key_a.clone()))]);
            let idx = d.route([192, 168, 50, 1]).expect("routes via A");
            assert_eq!(d.peers[idx].key_b64, key_a);
        })
        .expect("device");
        // A's session expires on the injected clock (the production trigger):
        // the net fails over to B
        slot.with_device(|d| {
            let max = d.cfg.session_max_ms;
            d.tick(max + 2);
            assert_eq!(
                d.managed_route_selections(),
                vec![(LAN_NET, Some(key_b.clone()))],
                "the expired peer must lose the net to its HA partner"
            );
            let idx = d.route([192, 168, 50, 1]).expect("routes via B");
            assert_eq!(d.peers[idx].key_b64, key_b);
        })
        .expect("device");
        // A re-establishes (ingest resets `expired`): it wins the net back
        slot.with_device(|d| {
            let a = d.peers.iter().position(|p| p.key_b64 == key_a).expect("peer a");
            d.peers[a].expired = false;
            d.rebuild_route_overlay("session-established");
            assert_eq!(d.managed_route_selections(), vec![(LAN_NET, Some(key_a.clone()))]);
        })
        .expect("device");
        for fd in [wg_raw, tun_raw, tun_end] {
            unsafe { sys::close(fd) };
        }
    }

    /// Line-format pin: `route-peer` carries net + peer short + reason, and
    /// an empty candidate renders `peer=none` (never a bare `peer=`).
    #[test]
    fn route_peer_line_names_net_peer_and_reason() {
        assert_eq!(
            route_peer_line(LAN_NET, Some("FyLw4PSBg1hjr+9/x0="), Some(10), true, "session-expired"),
            "N6_WG_DEVICE|route-peer|net=192.168.50.0/24|peer=FyLw4PSBg1hjr+9/x0=|metric=10|live=true|reason=session-expired"
        );
        assert_eq!(
            route_peer_line(LAN_NET, None, None, false, "routes-updated"),
            "N6_WG_DEVICE|route-peer|net=192.168.50.0/24|peer=none|metric=none|live=false|reason=routes-updated"
        );
        assert_eq!(
            route_selection_line(LAN_NET, Some("FyLw4PSBg1hjr+9/x0="), Some(10), false, "routes-updated"),
            "N6_WG_DEVICE|route-selection|net=192.168.50.0/24|peer=FyLw4PSBg1hjr+9/x0=|metric=10|live=false|reason=routes-updated"
        );
    }
}

// ---------------------------------------------------------------------------
// tests (N13-D4 per-peer transfer counters: real traffic through TWO real
// devices on loopback UDP + TUN socketpairs — same host-test pattern as
// tests/wg_e2e.rs, kept in-crate so the counter semantics are pinned next to
// the code that produces them)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod transfer_tests {
    use super::*;

    extern "C" {
        fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    }

    const ADDR_A: [u8; 4] = [10, 99, 0, 1];
    const ADDR_B: [u8; 4] = [10, 99, 0, 2];
    const ADDR_C: [u8; 4] = [10, 99, 0, 3];

    /// boringtun's ffi installs a panic→SIGSEGV hook the first time a tunnel
    /// is created (boringtun-0.7.1 `ffi/mod.rs` PANIC_HOOK), which would turn
    /// a failed assertion in these real-tunnel tests into a silent signal-11
    /// death. Re-install a PRINTING hook (the `tests/wg_e2e.rs` helper) AFTER
    /// the first tunnel exists, so the hook wins.
    fn show_panics() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            std::panic::set_hook(Box::new(|info| eprintln!("\nTEST PANIC: {info}")));
        });
    }

    /// Deterministic identity: secret = base64(32 × `byte`); the public key is
    /// derived through the SAME frozen `x25519_public_key` export the device's
    /// tunnels use, so both ends of a pair agree by construction.
    fn identity(byte: u8) -> (String, String) {
        let secret = base64::engine::general_purpose::STANDARD.encode([byte; 32]);
        let pub_key = x25519_public_b64(&secret).expect("public key");
        (secret, pub_key)
    }

    /// Test clock policy (same shape as tests/wg_e2e.rs): fast handshake
    /// retries, keepalive/expiry effectively off — no timer traffic can
    /// pollute the transfer counters unless a test opts in.
    fn test_cfg(secret_b64: &str) -> WgDeviceConfig {
        let mut cfg = WgDeviceConfig::new(secret_b64);
        cfg.hs_retry_ms = 100;
        cfg.hs_deadline_ms = 10_000;
        cfg.keepalive_ms = u64::MAX / 2;
        cfg.session_max_ms = u64::MAX / 2;
        cfg
    }

    /// One real device: a bound loopback UDP socket (its WG "cable end") + a
    /// datagram socketpair playing the platform TUN half. The test owns the
    /// raw fds; the device holds dups only (fd contract).
    struct Dev {
        dev: WgDevice,
        /// This device's own public key (what peers register).
        key: String,
        wg_raw: i32,
        tun_raw: i32,
        tun_end: i32,
    }

    impl Drop for Dev {
        fn drop(&mut self) {
            for fd in [self.wg_raw, self.tun_raw, self.tun_end] {
                unsafe { sys::close(fd) };
            }
        }
    }

    impl Dev {
        /// Device over a fresh loopback socket, registering `peers`
        /// (public key, /32) with NO endpoint yet.
        fn new(byte: u8, peers: &[(&str, [u8; 4])]) -> Dev {
            let (secret, pub_key) = identity(byte);
            let wg_raw = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(wg_raw >= 0, "socket errno={}", sys::errno());
            let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
            assert_eq!(
                unsafe {
                    sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32)
                },
                0,
                "bind errno={}",
                sys::errno()
            );
            let mut sv = [-1i32; 2];
            assert_eq!(unsafe { socketpair(1 /* AF_UNIX */, 2 /* SOCK_DGRAM */, 0, &mut sv) }, 0);
            let tun = TunFd::dup_from_raw(sv[0]).expect("tun dup");
            let mut dev = WgDevice::adopt(test_cfg(&secret), wg_raw, tun).expect("device adopt");
            let specs: Vec<WgPeerSpec> =
                peers.iter().map(|(k, addr)| peer_spec(k, *addr)).collect();
            dev.set_peers(&specs).expect("set_peers");
            show_panics();
            Dev { dev, key: pub_key, wg_raw, tun_raw: sv[0], tun_end: sv[1] }
        }

        fn port(&self) -> u16 {
            self.dev.local_addr().1
        }

        fn stats(&self) -> WgDeviceStats {
            self.dev.stats()
        }

        fn peer(&self, key: &str) -> WgPeerStatus {
            self.dev
                .peers()
                .into_iter()
                .find(|p| p.pub_key_b64 == key)
                .unwrap_or_else(|| panic!("peer {key} is registered"))
        }

        /// One pump round: TUN half + UDP half + tick, all on the injected
        /// clock, poll timeout 0 (loopback delivery is synchronous).
        fn step(&mut self, now_ms: u64) {
            self.dev.service_tun(now_ms);
            self.dev.service_udp(now_ms);
            self.dev.tick(now_ms);
        }

        fn write_frame(&self, frame: &[u8]) {
            let (n, e) = sys::write_fd(self.tun_end, frame);
            assert_eq!((n as usize, e), (frame.len(), 0), "TUN-side test write");
        }

        /// Poll-then-read (the test end of the socketpair is blocking, so the
        /// readiness check is what keeps this non-blocking).
        fn read_frame(&self) -> Option<Vec<u8>> {
            let (ret, _e, rev) = sys::poll1(self.tun_end, sys::POLLIN, 0);
            if ret <= 0 || (rev & sys::POLLIN) == 0 {
                return None;
            }
            let mut buf = [0u8; WG_BUF];
            let (n, e) = sys::read_fd(self.tun_end, &mut buf);
            assert!(n > 0, "TUN-side test read errno={e}");
            Some(buf[..n as usize].to_vec())
        }
    }

    fn peer_spec(key: &str, addr: [u8; 4]) -> WgPeerSpec {
        WgPeerSpec::new(key.to_string(), vec![(addr, 32)])
    }

    /// Land endpoints both ways and pump until both sessions are established.
    fn establish(devs: &mut [&mut Dev], a: usize, b: usize, now: &mut u64) {
        let (pa, pb) = (devs[a].port(), devs[b].port());
        let (ka, kb) = (devs[a].key.clone(), devs[b].key.clone());
        devs[a].dev.set_endpoint(&kb, [127, 0, 0, 1], pb, *now).expect("endpoint a→b");
        devs[b].dev.set_endpoint(&ka, [127, 0, 0, 1], pa, *now).expect("endpoint b→a");
        let deadline = *now + 10_000;
        loop {
            for d in devs.iter_mut() {
                d.step(*now);
            }
            if devs[a].dev.tunnel_ready() && devs[b].dev.tunnel_ready() {
                return;
            }
            assert!(*now <= deadline, "handshake must establish within the sim deadline");
            *now += 10;
        }
    }

    /// Pump every device until a frame falls out of `to`'s TUN half (the read
    /// IS the consumption, so the frame is captured here).
    fn pump_until_delivery(devs: &mut [&mut Dev], to: usize, now: &mut u64) -> Vec<u8> {
        let deadline = *now + 5_000;
        loop {
            for d in devs.iter_mut() {
                d.step(*now);
            }
            if let Some(got) = devs[to].read_frame() {
                return got;
            }
            assert!(*now <= deadline, "frame must cross the tunnel within the sim deadline");
            *now += 10;
        }
    }

    /// Write `frame` into `from`'s TUN half and pump until it comes out of
    /// `to`'s TUN half.
    fn carry(devs: &mut [&mut Dev], from: usize, to: usize, now: &mut u64, frame: &[u8]) -> Vec<u8> {
        devs[from].write_frame(frame);
        pump_until_delivery(devs, to, now)
    }

    /// One IPv4/UDP frame with an honest total-length field (boringtun derives
    /// the received plaintext length from it, so tx and rx byte counts must
    /// agree on exactly this length).
    fn frame(src: [u8; 4], dst: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let total = (20 + 8 + payload.len()) as u16;
        let mut f = vec![0x45, 0];
        f.extend_from_slice(&total.to_be_bytes());
        f.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]); // id, flags, ttl, proto UDP, cksum
        f.extend_from_slice(&src);
        f.extend_from_slice(&dst);
        f.extend_from_slice(&5555u16.to_be_bytes());
        f.extend_from_slice(&5556u16.to_be_bytes());
        f.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
        f.extend_from_slice(&[0, 0]); // udp checksum (inner checksums are not validated)
        f.extend_from_slice(payload);
        f
    }

    /// Test carrier: records every datagram it is handed (whether it accepts
    /// it or not), so a test can prove boringtun DID encapsulate while the
    /// published counter still refuses to call it transferred.
    struct TestCarrier {
        accept: bool,
        seen: Mutex<Vec<Vec<u8>>>,
    }

    impl TestCarrier {
        fn new(accept: bool) -> Arc<TestCarrier> {
            Arc::new(TestCarrier { accept, seen: Mutex::new(Vec::new()) })
        }

        fn seen(&self) -> Vec<Vec<u8>> {
            self.seen.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    impl WgEgressCarrier for TestCarrier {
        fn send_datagram(&self, datagram: &[u8]) -> Result<(), String> {
            self.seen.lock().unwrap_or_else(|e| e.into_inner()).push(datagram.to_vec());
            if self.accept {
                Ok(())
            } else {
                Err("carrier-backpressure".to_string())
            }
        }

        fn kind(&self) -> &'static str {
            "test-carrier"
        }
    }

    /// Real traffic through two peers of ONE device: every counter moves by
    /// exactly the transferred plaintext length, the handshake itself adds
    /// nothing, and a second peer's counters stay untouched.
    #[test]
    fn peer_transfer_counters_follow_real_traffic_and_stay_isolated() {
        let (_sa, ka) = identity(0x11);
        let (_sb, kb) = identity(0x22);
        let (_sc, kc) = identity(0x33);
        let mut a = Dev::new(0x11, &[(kb.as_str(), ADDR_B), (kc.as_str(), ADDR_C)]);
        let mut b = Dev::new(0x22, &[(ka.as_str(), ADDR_A)]);
        let mut c = Dev::new(0x33, &[(ka.as_str(), ADDR_A)]);
        let mut devs: Vec<&mut Dev> = vec![&mut a, &mut b, &mut c];
        let mut now = 1_000u64;
        establish(&mut devs, 0, 1, &mut now);
        establish(&mut devs, 0, 2, &mut now);

        // the handshake (init/response) carried no plaintext: both directions
        // stay at zero on every peer
        for (i, k) in [(0usize, &kb), (0, &kc), (1, &ka), (2, &ka)] {
            let p = devs[i].peer(k);
            assert_eq!(
                (p.tx_bytes, p.rx_bytes),
                (0, 0),
                "handshake packets are not plaintext transfer ({k})"
            );
        }

        // A → B: exactly the frame length, attributed to B only
        let f1 = frame(ADDR_A, ADDR_B, &[0xA1; 24]);
        assert_eq!(carry(&mut devs, 0, 1, &mut now, &f1), f1, "payload crosses unchanged");
        assert_eq!(devs[0].peer(&kb).tx_bytes, f1.len() as u64, "A→B tx = the frame's plaintext bytes");
        assert_eq!(devs[0].peer(&kb).rx_bytes, 0, "no return traffic");
        assert_eq!(devs[0].peer(&kc).tx_bytes, 0, "peer isolation: C's tx untouched");
        assert_eq!(devs[0].peer(&kc).rx_bytes, 0, "peer isolation: C's rx untouched");
        assert_eq!(devs[1].peer(&ka).rx_bytes, f1.len() as u64, "B received f1 into its TUN");
        assert_eq!(devs[1].peer(&ka).tx_bytes, 0, "B sent nothing");
        assert_eq!(devs[2].peer(&ka).tx_bytes + devs[2].peer(&ka).rx_bytes, 0, "C saw nothing");

        // C → A: the other peer's counters move, B's do not
        let f2 = frame(ADDR_C, ADDR_A, &[0xC2; 40]);
        assert_eq!(carry(&mut devs, 2, 0, &mut now, &f2), f2, "payload crosses unchanged");
        assert_eq!(devs[2].peer(&ka).tx_bytes, f2.len() as u64, "C→A tx = f2's plaintext bytes");
        assert_eq!(devs[0].peer(&kc).rx_bytes, f2.len() as u64, "A received f2 from C");
        assert_eq!(devs[0].peer(&kb).tx_bytes, f1.len() as u64, "B's tx untouched by C's traffic");
        assert_eq!(devs[0].peer(&kb).rx_bytes, 0, "B's rx untouched by C's traffic");

        // the per-peer rx view is the device view, split by peer
        let st = devs[0].stats();
        assert_eq!(
            devs[0].peer(&kb).rx_bytes + devs[0].peer(&kc).rx_bytes,
            st.rx_bytes_to_tun,
            "Σ peer rx_bytes == device rx_bytes_to_tun"
        );
        // unit cross-check: the per-peer PLAINTEXT count + the WG transport
        // overhead is the device-level ENCAPSULATED byte counter — the two
        // fields are different units on purpose, and this pins the difference
        // (handshakes/keepalives add nothing to either)
        assert_eq!(
            st.tx_bytes,
            f1.len() as u64 + 32,
            "device tx_bytes is encapsulated (peer plaintext + 32B transport overhead)"
        );
    }

    /// A frame handed to a peer with NO live session is queued inside boringtun
    /// and flushed when the handshake completes: its plaintext bytes are
    /// attributed on the flush (never lost, never counted twice, and the
    /// initiation that carried it is not charged as plaintext).
    #[test]
    fn queued_pre_handshake_frame_is_counted_when_flushed() {
        let (_sa, ka) = identity(0x44);
        let (_sb, kb) = identity(0x55);
        let mut a = Dev::new(0x44, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x55, &[(ka.as_str(), ADDR_A)]);
        let mut devs: Vec<&mut Dev> = vec![&mut a, &mut b];
        let mut now = 1_000u64;

        // land the endpoints (each fires one initiation) but DO NOT pump yet
        let (pa, pb) = (devs[0].port(), devs[1].port());
        devs[0].dev.set_endpoint(&kb, [127, 0, 0, 1], pb, now).expect("endpoint A→B");
        devs[1].dev.set_endpoint(&ka, [127, 0, 0, 1], pa, now).expect("endpoint B→A");

        let f = frame(ADDR_A, ADDR_B, &[0x7E; 40]);
        devs[0].write_frame(&f);
        // ONE A step: no session ⇒ boringtun QUEUES the frame. The handshake
        // started by the endpoint landing is already in progress, so this
        // write emits no second initiation (boringtun only retries on its own
        // timer): exactly the one endpoint-landing datagram goes out, with
        // zero plaintext accounting.
        devs[0].step(now);
        assert!(!devs[0].peer(&kb).session_established, "no session yet");
        assert_eq!(devs[0].stats().tx_packets, 1, "no initiation for an in-progress handshake");
        assert_eq!(devs[0].peer(&kb).tx_bytes, 0, "a queued frame is not transferred yet");

        // pump: the session comes up, the queued frame is flushed and arrives
        assert_eq!(pump_until_delivery(&mut devs, 1, &mut now), f, "the queued frame arrives");
        assert_eq!(
            devs[0].peer(&kb).tx_bytes,
            f.len() as u64,
            "the flushed frame is counted exactly once"
        );
        assert_eq!(
            devs[1].peer(&ka).rx_bytes,
            f.len() as u64,
            "the peer's rx view is the same plaintext length"
        );
    }

    /// Both bearers and the refused-send rule: a carrier delivery counts the
    /// plaintext as transferred; the SAME datagram refused by the carrier does
    /// not (even though boringtun already encapsulated it).
    #[test]
    fn refused_bearer_send_is_not_counted_and_carrier_delivery_is() {
        let (_sa, ka) = identity(0x66);
        let (_sb, kb) = identity(0x77);
        let mut a = Dev::new(0x66, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x77, &[(ka.as_str(), ADDR_A)]);
        let mut devs: Vec<&mut Dev> = vec![&mut a, &mut b];
        let mut now = 1_000u64;
        establish(&mut devs, 0, 1, &mut now);
        assert_eq!(devs[0].peer(&kb).tx_bytes, 0, "handshake only so far");

        // ICE lost: the endpoint is recycled, a carrier that REFUSES is
        // attached. The frame is encapsulated but never delivered.
        let bad = TestCarrier::new(false);
        devs[0].dev.recycle_endpoint(&kb);
        devs[0].dev.set_carrier(&kb, bad.clone()).expect("carrier attach");
        let before = devs[0].stats();
        let f1 = frame(ADDR_A, ADDR_B, &[0x5A; 48]);
        devs[0].write_frame(&f1);
        devs[0].step(now);
        let refused = bad.seen();
        assert_eq!(refused.len(), 1, "the carrier really was handed the datagram");
        assert_eq!(refused[0].len(), f1.len() + 32, "…as a WG DATA packet (+32B overhead)");
        assert!(wg_message_is_data(&refused[0]), "…of transport type 4");
        assert_eq!(devs[0].peer(&kb).tx_bytes, 0, "a refused datagram is NOT transferred");
        assert_eq!(devs[0].peer(&kb).rx_bytes, 0, "nothing came back");
        let after = devs[0].stats();
        assert_eq!(after.carrier_rejects - before.carrier_rejects, 1, "typed refusal counted once");
        assert_eq!(after.carrier_tx_bytes - before.carrier_tx_bytes, 0, "refused ⇒ no carrier bytes");
        assert_eq!(after.tx_packets - before.tx_packets, 0, "refused ⇒ no tx packet counted");
        assert_eq!(after.carrier_takeovers, 1, "the refusal happened on the carrier path");

        // the carrier starts accepting: the same frame is now transferred
        let good = TestCarrier::new(true);
        devs[0].dev.set_carrier(&kb, good.clone()).expect("carrier swap");
        let f2 = frame(ADDR_A, ADDR_B, &[0x6B; 16]);
        devs[0].write_frame(&f2);
        devs[0].step(now);
        assert_eq!(devs[0].peer(&kb).tx_bytes, f2.len() as u64, "delivered carrier traffic counts");
        let seen = good.seen();
        assert_eq!(seen.len(), 1, "one datagram delivered");
        assert_eq!(seen[0].len(), f2.len() + 32, "wire bytes = plaintext + 32B transport overhead");
        assert_eq!(devs[0].stats().carrier_tx_bytes, (f2.len() + 32) as u64, "carrier_tx_bytes agrees");
        assert_eq!(devs[0].stats().carrier_tx_packets, 1);
        assert_eq!(
            devs[0].stats().carrier_takeovers,
            1,
            "still the same carrier episode (no second takeover)"
        );
    }

    /// Counter lifecycle: a surviving key keeps its tunnel AND its counters
    /// across `set_peers` snapshots; a replaced key (removed, then re-added)
    /// starts from zero.
    #[test]
    fn peer_transfer_counters_survive_resnapshot_and_reset_on_replacement() {
        let (_sa, ka) = identity(0x88);
        let (_sb, kb) = identity(0x99);
        let mut a = Dev::new(0x88, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x99, &[(ka.as_str(), ADDR_A)]);
        let mut devs: Vec<&mut Dev> = vec![&mut a, &mut b];
        let mut now = 1_000u64;
        establish(&mut devs, 0, 1, &mut now);
        let f = frame(ADDR_A, ADDR_B, &[0xD3; 20]);
        assert_eq!(carry(&mut devs, 0, 1, &mut now, &f), f);
        assert_eq!(devs[0].peer(&kb).tx_bytes, f.len() as u64);

        // the same key in the next map snapshot: tunnel (and counters) kept
        devs[0].dev.set_peers(&[peer_spec(&kb, ADDR_B)]).expect("re-snapshot");
        assert_eq!(devs[0].peer(&kb).tx_bytes, f.len() as u64, "same peer ⇒ same counters");
        assert_eq!(devs[0].peer(&kb).rx_bytes, 0);

        // peer replacement: the old WgPeer (and its Tunn) is dropped
        devs[0].dev.set_peers(&[]).expect("peer removed");
        assert!(devs[0].dev.peers().is_empty(), "peer gone");
        devs[0].dev.set_peers(&[peer_spec(&kb, ADDR_B)]).expect("peer re-added");
        let fresh = devs[0].peer(&kb);
        assert_eq!((fresh.tx_bytes, fresh.rx_bytes), (0, 0), "a replaced peer starts at zero");
        assert!(!fresh.session_established, "…with a fresh tunnel");

        // and the OTHER side's counters are untouched by A's replacement
        assert_eq!(devs[1].peer(&ka).rx_bytes, f.len() as u64);
    }
}

// ---------------------------------------------------------------------------
// tests (WG session lifecycle: policy expiry → the campaign re-arms → only a
// handshake STRICTLY after the expiry instant recovers; real BoringTun tunnels
// over loopback UDP + TUN socketpairs, injected clock — same host-test pattern
// as `transfer_tests` / tests/wg_e2e.rs)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod session_expiry_tests {
    use super::*;
    use crate::config;
    use crate::network_map::ManagedRoute;

    extern "C" {
        fn socketpair(domain: i32, ty: i32, protocol: i32, sv: *mut [i32; 2]) -> i32;
    }

    const ADDR_A: [u8; 4] = [10, 97, 0, 1];
    const ADDR_B: [u8; 4] = [10, 97, 0, 2];
    const ADDR_C: [u8; 4] = [10, 97, 0, 3];
    /// LAN net of the HA failover leg (same shape as the field defect: one
    /// managed net behind two routing peers).
    const LAN_NET: ([u8; 4], u8) = ([192, 168, 70, 0], 24);
    /// Policy session window for these tests: short enough that the injected
    /// clock reaches the expiry instant without waiting anything out in REAL
    /// time (boringtun's own clock is never aged here).
    const SESSION_MAX_MS: u64 = 3_000;

    /// boringtun's ffi installs a panic→SIGSEGV hook the first time a tunnel is
    /// created; re-install a PRINTING hook AFTER the first tunnel exists so a
    /// failed assertion reports instead of dying silently (see
    /// `transfer_tests::show_panics`).
    fn show_panics() {
        static ONCE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        ONCE.get_or_init(|| {
            std::panic::set_hook(Box::new(|info| eprintln!("\nTEST PANIC: {info}")));
        });
    }

    fn identity(byte: u8) -> (String, String) {
        let secret = base64::engine::general_purpose::STANDARD.encode([byte; 32]);
        let pub_key = x25519_public_b64(&secret).expect("public key");
        (secret, pub_key)
    }

    fn expiry_cfg(secret_b64: &str) -> WgDeviceConfig {
        let mut cfg = WgDeviceConfig::new(secret_b64);
        cfg.hs_retry_ms = 100; // fast campaign retries on the injected clock
        cfg.hs_deadline_ms = 10_000;
        cfg.keepalive_ms = u64::MAX / 2; // no keepalive traffic in these tests
        cfg.session_max_ms = SESSION_MAX_MS;
        cfg
    }

    fn peer_spec(key: &str, addr: [u8; 4]) -> WgPeerSpec {
        WgPeerSpec::new(key.to_string(), vec![(addr, 32)])
    }

    /// HA routing entry for [`LAN_NET`] via `peer` (metric decides while idle).
    fn lan_route(peer: &str, metric: i64) -> ManagedRoute {
        ManagedRoute {
            id: format!("r-lan-{metric}"),
            network: config::Route { addr: LAN_NET.0, prefix_len: LAN_NET.1 },
            domains: vec![],
            net_id: "net-lan".into(),
            network_type: 1,
            peer: peer.into(),
            metric,
            masquerade: true,
            keep_route: false,
            skip_auto_apply: false,
        }
    }

    /// One real device: a bound loopback UDP socket (its WG "cable end") + a
    /// datagram socketpair playing the platform TUN half (the test owns all raw
    /// fds; the device holds dups only).
    struct Dev {
        dev: WgDevice,
        key: String,
        wg_raw: i32,
        tun_raw: i32,
        tun_end: i32,
    }

    impl Drop for Dev {
        fn drop(&mut self) {
            for fd in [self.wg_raw, self.tun_raw, self.tun_end] {
                unsafe { sys::close(fd) };
            }
        }
    }

    impl Dev {
        fn new(byte: u8, peers: &[(&str, [u8; 4])]) -> Dev {
            let (secret, pub_key) = identity(byte);
            let wg_raw = unsafe { sys::socket(sys::AF_INET, sys::SOCK_DGRAM, 0) };
            assert!(wg_raw >= 0, "socket errno={}", sys::errno());
            let sa = sys::sockaddr_in::new([127, 0, 0, 1], 0);
            assert_eq!(
                unsafe {
                    sys::bind(wg_raw, &sa, core::mem::size_of::<sys::sockaddr_in>() as u32)
                },
                0,
                "bind errno={}",
                sys::errno()
            );
            let mut sv = [-1i32; 2];
            assert_eq!(unsafe { socketpair(1 /* AF_UNIX */, 2 /* SOCK_DGRAM */, 0, &mut sv) }, 0);
            let tun = TunFd::dup_from_raw(sv[0]).expect("tun dup");
            let mut dev = WgDevice::adopt(expiry_cfg(&secret), wg_raw, tun).expect("device adopt");
            let specs: Vec<WgPeerSpec> =
                peers.iter().map(|(k, addr)| peer_spec(k, *addr)).collect();
            dev.set_peers(&specs).expect("set_peers");
            show_panics();
            Dev { dev, key: pub_key, wg_raw, tun_raw: sv[0], tun_end: sv[1] }
        }

        fn port(&self) -> u16 {
            self.dev.local_addr().1
        }

        /// The peer's MUTABLE state (injected-clock anchors + the test-only
        /// stats reading) — the pokes the sibling test modules also use.
        fn peer_mut(&mut self, key: &str) -> &mut WgPeer {
            self.dev
                .peers
                .iter_mut()
                .find(|p| p.key_b64 == key)
                .expect("peer registered")
        }

        /// One pump round: TUN half + UDP half + tick on the injected clock.
        fn step(&mut self, now_ms: u64) {
            self.dev.service_tun(now_ms);
            self.dev.service_udp(now_ms);
            self.dev.tick(now_ms);
        }

        fn write_frame(&self, frame: &[u8]) {
            let (n, e) = sys::write_fd(self.tun_end, frame);
            assert_eq!((n as usize, e), (frame.len(), 0), "TUN-side test write");
        }
    }

    /// Land endpoints both ways and pump until both sessions are live, then
    /// settle so the handshake stragglers cannot re-anchor anything later.
    fn establish(a: &mut Dev, b: &mut Dev, now: &mut u64) {
        let (pa, pb) = (a.port(), b.port());
        let (ka, kb) = (a.key.clone(), b.key.clone());
        a.dev.set_endpoint(&kb, [127, 0, 0, 1], pb, *now).expect("endpoint a→b");
        b.dev.set_endpoint(&ka, [127, 0, 0, 1], pa, *now).expect("endpoint b→a");
        let deadline = *now + 10_000;
        loop {
            a.step(*now);
            b.step(*now);
            if a.dev.tunnel_ready() && b.dev.tunnel_ready() {
                break;
            }
            assert!(*now <= deadline, "handshake must establish within the sim deadline");
            *now += 10;
        }
        for _ in 0..8 {
            *now += 10;
            a.step(*now);
            b.step(*now);
        }
    }

    /// One IPv4/UDP frame with an honest total-length field (boringtun derives
    /// the received plaintext length from it).
    fn frame(src: [u8; 4], dst: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let total = (20 + 8 + payload.len()) as u16;
        let mut f = vec![0x45, 0];
        f.extend_from_slice(&total.to_be_bytes());
        f.extend_from_slice(&[0, 1, 0, 0, 64, 17, 0, 0]); // id, flags, ttl, proto UDP, cksum
        f.extend_from_slice(&src);
        f.extend_from_slice(&dst);
        f.extend_from_slice(&5555u16.to_be_bytes());
        f.extend_from_slice(&5556u16.to_be_bytes());
        f.extend_from_slice(&((8 + payload.len()) as u16).to_be_bytes());
        f.extend_from_slice(&[0, 0]); // udp checksum (inner checksums are not validated)
        f.extend_from_slice(payload);
        f
    }

    /// Tick device `a` past its anchor's policy window. Returns
    /// `(expiry_instant, recover_instant)`; on return the peer is expired, the
    /// campaign is re-armed (the expiry tick itself initiated) and one stale
    /// ingest has already been proven harmless.
    ///
    /// Stats injection: the injected device clock and boringtun's REAL
    /// handshake clock are unrelated in a host test, so the boringtun reading is
    /// kept production-consistent by hand — while expired it is the OLD
    /// handshake's age in whole seconds, `(recover_now − anchor)/1000` (in
    /// production that is exactly what an inbound packet reads at that instant).
    fn expire_on_the_device_clock(a: &mut Dev, kb: &str) -> (u64, u64) {
        let anchor = a.peer_mut(kb).session_established_ms;
        let expiry = anchor + SESSION_MAX_MS;
        let recover_now = expiry + 5_000;
        a.peer_mut(kb).hs_override = Some(((recover_now - anchor) / 1000) as i64);
        a.dev.tick(expiry);
        assert!(a.peer_mut(kb).expired, "the policy window closed");
        assert!(
            a.peer_mut(kb).first_init_ms.is_some(),
            "the expiry tick itself re-armed the handshake campaign"
        );
        // any datagram read while expired still carries the OLD handshake's age
        a.dev.service_udp(expiry);
        assert!(a.peer_mut(kb).expired, "the old handshake's age is not recovery");
        (expiry, recover_now)
    }

    /// Peer `b` drives a genuine NEW handshake into `a`; its session is 0 s old
    /// when `a` ingests it (the production reading right after a handshake).
    fn recover_through_a_new_handshake(a: &mut Dev, b: &mut Dev, kb: &str, recover_now: u64) {
        assert!(b.dev.initiate_handshake(0, recover_now), "B's fresh initiation went out");
        a.peer_mut(kb).hs_override = Some(0);
        assert!(a.dev.service_udp(recover_now) >= 1, "A ingests the new handshake");
        assert!(!a.peer_mut(kb).expired, "the post-expiry handshake re-established the session");
    }

    /// The defect's reading arithmetic, pinned directly: `hs` is the AGE of the
    /// last handshake, so every inbound packet after expiry still reads
    /// `hs >= 0`, and only a handshake whose implied completion is STRICTLY
    /// later than the expiry instant may re-establish the session.
    #[test]
    fn expiry_judgement_rejects_the_old_handshake_age() {
        let t0 = 1_000u64; // session anchor on the injected clock
        let max = DEFAULT_SESSION_MAX_MS; // 540 s
        let expiry = t0 + max;
        // the OLD handshake, read 5 s after the expiry tick: implied completion
        // is t0 + (age mod 1 s) — never past an expiry instant that sits a full
        // session_max_ms later
        assert!(!handshake_after_expiry(545, expiry + 5_000, t0, max));
        assert!(!handshake_after_expiry(600, expiry + 60_500, t0, max));
        // …and however far the device clock has moved on afterwards
        assert!(!handshake_after_expiry(3_600, expiry + 3_060_000, t0, max));
        // exactly AT the expiry instant is not strictly later
        assert!(!handshake_after_expiry(0, expiry, t0, max));
        // a handshake that completed after the expiry instant (hs == 0 = within
        // the last second, hs == 1 one second ago but still past the instant)
        assert!(handshake_after_expiry(0, expiry + 1, t0, max));
        assert!(handshake_after_expiry(0, expiry + 1_000, t0, max));
        assert!(handshake_after_expiry(1, expiry + 2_000, t0, max));
        // a 1 s-old reading whose implied completion is BEFORE the instant (a
        // packet delayed across the boundary) is not recovery either
        assert!(!handshake_after_expiry(1, expiry + 500, t0, max));
        // no session at all
        assert!(!handshake_after_expiry(-1, expiry + 60_000, t0, max));
    }

    /// (a) An expired session RE-ARMS the handshake campaign in the very tick
    /// that sets `expired` — the peer must not have to initiate — while a tick
    /// inside the window stays silent.
    #[test]
    fn expiry_re_arms_the_handshake_campaign() {
        let (_sa, ka) = identity(0x11);
        let (_sb, kb) = identity(0x22);
        let mut a = Dev::new(0x11, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x22, &[(ka.as_str(), ADDR_A)]);
        let mut now = 1_000u64;
        establish(&mut a, &mut b, &mut now);
        assert!(a.dev.tunnel_ready() && !a.peer_mut(&kb).expired, "live session");

        let anchor = a.peer_mut(&kb).session_established_ms;
        let initiations = a.dev.stats().handshake_initiations;
        a.step(anchor + SESSION_MAX_MS - 1);
        assert_eq!(
            a.peer_mut(&kb).session_established_ms,
            anchor,
            "no straggler re-anchored the session inside the window"
        );
        assert!(!a.peer_mut(&kb).expired, "still inside the policy window");
        assert_eq!(
            a.dev.stats().handshake_initiations,
            initiations,
            "no campaign while the session is live"
        );

        a.step(anchor + SESSION_MAX_MS);
        assert!(a.peer_mut(&kb).expired, "the policy window closed");
        assert!(
            a.peer_mut(&kb).first_init_ms.is_some(),
            "the expiry tick re-armed the campaign"
        );
        assert!(
            a.dev.stats().handshake_initiations > initiations,
            "…and initiated a fresh handshake in that same tick"
        );
        assert!(!a.dev.tunnel_ready() && !a.dev.peer_ready(&kb), "the session reads dead");
    }

    /// (b) After expiry, an inbound datagram whose boringtun reading is the OLD
    /// handshake's age must NOT restore the session (that age is readable on
    /// every packet — reading it as recovery flaps the peer back online).
    #[test]
    fn old_handshake_age_does_not_restore_an_expired_session() {
        let (_sa, ka) = identity(0x33);
        let (_sb, kb) = identity(0x44);
        let mut a = Dev::new(0x33, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x44, &[(ka.as_str(), ADDR_A)]);
        let mut now = 1_000u64;
        establish(&mut a, &mut b, &mut now);
        for _ in 0..8 {
            now += 10;
            a.step(now);
            b.step(now);
        }
        let anchor = a.peer_mut(&kb).session_established_ms;

        // B sends real traffic that A still decrypts with the OLD keys
        // (boringtun's session is alive; only the device policy expired it).
        let f = frame(ADDR_B, ADDR_A, &[0xB4; 32]);
        b.write_frame(&f);
        b.step(now);

        // Production-consistent reading at the expiry instant + 5 s: the age of
        // the OLD handshake (the host cannot age boringtun's real clock, so the
        // equivalent value is injected — see the module docs of the helper).
        let expired_now = anchor + SESSION_MAX_MS + 5_000;
        a.peer_mut(&kb).hs_override = Some(((expired_now - anchor) / 1000) as i64);
        a.dev.tick(expired_now); // policy expiry + campaign re-arm
        assert!(a.peer_mut(&kb).expired, "the policy window closed");
        let expired_anchor = a.peer_mut(&kb).session_established_ms;
        let armed = a.peer_mut(&kb).first_init_ms;
        assert!(armed.is_some(), "the expiry tick re-armed the campaign");

        let ingested = a.dev.service_udp(expired_now);
        assert!(ingested >= 1, "the old-key datagram really was ingested");
        assert!(a.peer_mut(&kb).expired, "the old handshake's age is NOT recovery");
        assert_eq!(
            a.peer_mut(&kb).session_established_ms,
            expired_anchor,
            "…and the anchor did not move"
        );
        assert_eq!(a.peer_mut(&kb).first_init_ms, armed, "…and the campaign stays armed");
        assert!(!a.dev.peer_ready(&kb) && !a.dev.tunnel_ready(), "still offline");
    }

    /// (c) A NEW handshake completing after the expiry instant re-establishes
    /// the session: `expired` cleared, anchor refreshed, campaign cleared, the
    /// `session-established` path taken (emit + overlay rebuild) and the peer
    /// online again with the first-establishment projection.
    #[test]
    fn new_handshake_after_expiry_restores_the_session_and_the_overlay() {
        let (_sa, ka) = identity(0x55);
        let (_sb, kb) = identity(0x66);
        let (_sc, kc) = identity(0x77);
        // the field's HA shape: LAN_NET behind B (metric 10, a real session) and
        // C (metric 20, registered without any path)
        let mut a = Dev::new(0x55, &[(kb.as_str(), ADDR_B), (kc.as_str(), ADDR_C)]);
        let mut b = Dev::new(0x66, &[(ka.as_str(), ADDR_A)]);
        let mut now = 1_000u64;
        a.dev
            .set_managed_routes(&[lan_route(&kb, 10), lan_route(&kc, 20)])
            .expect("managed routes");
        establish(&mut a, &mut b, &mut now);
        for _ in 0..8 {
            now += 10;
            a.step(now);
            b.step(now);
        }
        assert_eq!(
            a.dev.managed_route_selections(),
            vec![(LAN_NET, Some(kb.clone()))],
            "the live peer carries the net"
        );

        let (_expiry, recover_now) = expire_on_the_device_clock(&mut a, &kb);
        assert_eq!(
            a.dev.managed_route_selections(),
            vec![(LAN_NET, Some(kc.clone()))],
            "the expired peer loses the net (the field symptom, on the HA partner)"
        );
        assert!(!a.dev.peer_ready(&kb) && !a.dev.tunnel_ready(), "peer offline while expired");

        recover_through_a_new_handshake(&mut a, &mut b, &kb, recover_now);
        assert!(a.peer_mut(&kb).session_established, "established again");
        assert_eq!(
            a.peer_mut(&kb).session_established_ms,
            recover_now,
            "the anchor is refreshed to the recovery instant"
        );
        assert!(
            a.peer_mut(&kb).first_init_ms.is_none(),
            "the campaign is done — the FIRST-establishment action set ran \
             (the emit + overlay rebuild share that single branch)"
        );
        assert!(a.dev.peer_ready(&kb) && a.dev.tunnel_ready(), "the peer is online again");
        assert_eq!(
            a.dev.managed_route_selections(),
            vec![(LAN_NET, Some(kb.clone()))],
            "the session-established rebuild re-admits the peer to its nets"
        );
        // the status-JSON projection is the first-establishment one
        let status = a
            .dev
            .peers()
            .into_iter()
            .find(|p| p.pub_key_b64 == kb)
            .expect("peer B registered");
        let json = wg_peer_json(&status);
        assert!(
            json.contains("\"session_established\":true,\"expired\":false"),
            "recovered status JSON: {json}"
        );
        assert_eq!(
            a.dev
                .peers()
                .iter()
                .filter(|p| p.session_established && !p.expired)
                .count(),
            1,
            "wg_sessions counts the recovered peer and only it"
        );
    }

    /// (d) A live (non-expired) session receiving a fresh handshake is a normal
    /// rekey: the anchor is refreshed and NOTHING else happens — the
    /// establishment path must not steal the rekey case.
    #[test]
    fn live_rekey_only_refreshes_the_expiry_anchor() {
        let (_sa, ka) = identity(0xAA);
        let (_sb, kb) = identity(0xBB);
        let mut a = Dev::new(0xAA, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0xBB, &[(ka.as_str(), ADDR_A)]);
        let mut now = 1_000u64;
        establish(&mut a, &mut b, &mut now);
        for _ in 0..8 {
            now += 10;
            a.step(now);
            b.step(now);
        }
        let anchor = a.peer_mut(&kb).session_established_ms;
        let rekey_now = anchor + 1_000; // well inside the policy window

        assert!(a.dev.initiate_handshake(0, rekey_now), "the rekey initiation went out");
        assert!(a.peer_mut(&kb).first_init_ms.is_some(), "campaign bookkeeping armed");
        b.step(rekey_now); // B answers the initiation
        assert!(a.dev.service_udp(rekey_now) >= 1, "A ingests the handshake response");

        let p = a.peer_mut(&kb);
        assert!(!p.expired, "a rekey inside the window never expires the session");
        assert!(p.session_established, "…and never drops the established flag");
        assert_eq!(p.session_established_ms, rekey_now, "ONLY the expiry anchor moved");
        assert!(
            p.first_init_ms.is_some(),
            "the establishment path (which clears the campaign) did NOT run: rekey ≠ re-establish"
        );
    }

    /// (e) The cycle repeats: a recovered session expires AGAIN and recovers
    /// again on the same device — no one-shot state.
    #[test]
    fn expiry_recovery_can_repeat() {
        let (_sa, ka) = identity(0x88);
        let (_sb, kb) = identity(0x99);
        let mut a = Dev::new(0x88, &[(kb.as_str(), ADDR_B)]);
        let mut b = Dev::new(0x99, &[(ka.as_str(), ADDR_A)]);
        let mut now = 1_000u64;
        establish(&mut a, &mut b, &mut now);
        for _ in 0..8 {
            now += 10;
            a.step(now);
            b.step(now);
        }

        let mut anchor = a.peer_mut(&kb).session_established_ms;
        for cycle in 1..=2u32 {
            let (expiry, recover_now) = expire_on_the_device_clock(&mut a, &kb);
            assert!(expiry > anchor, "cycle {cycle}: each window is anchored later");
            recover_through_a_new_handshake(&mut a, &mut b, &kb, recover_now);
            let recovered = a.peer_mut(&kb).session_established_ms;
            assert_eq!(recovered, recover_now, "cycle {cycle}: re-anchored on recovery");
            assert!(a.dev.peer_ready(&kb) && a.dev.tunnel_ready(), "cycle {cycle}: online");
            anchor = recovered;
        }
    }
}

// ---------------------------------------------------------------------------
// tests (0.0.26 device-observability gaps: the N13_WG bearer-transition lines
// carry their peer, and the device's third drop counter is exported in the
// status document)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod observability_tests {
    use super::*;

    /// 0.0.26 gap 5（真机 0.0.25 §5.1）：载体 attach/detach 与 direct↔carrier
    /// 四条切换行都带 `peer=<公钥前8位>`（与 `session-established` 同源同格式）。
    #[test]
    fn n13_bearer_transition_lines_carry_the_peer_prefix() {
        // 8-char alphanumeric prefix of the base64 public key (peer_short)
        let key = "bjEzLWQxLXBlZXItc2Vzc2lvbg==";
        assert_eq!(
            carrier_attach_line("relay", false, key),
            "N13_WG|carrier-attach|kind=relay|replaced=false|peer=bjEzLWQx"
        );
        assert_eq!(
            carrier_attach_line("relay", true, key),
            "N13_WG|carrier-attach|kind=relay|replaced=true|peer=bjEzLWQx"
        );
        assert_eq!(
            carrier_detach_line(key),
            "N13_WG|carrier-detach|peer=bjEzLWQx"
        );
        assert_eq!(
            carrier_to_direct_line(key),
            "N13_WG|carrier->direct|ice-selected-path|peer=bjEzLWQx"
        );
        assert_eq!(
            direct_to_carrier_line(key),
            "N13_WG|direct->carrier|no-ice-endpoint|peer=bjEzLWQx"
        );
    }

    /// 0.0.26 gap 3（真机 0.0.25 §5.2）：`unknown_peer_drops` 与
    /// `dropped_no_route`/`decrypt_errors` 并排导出（0.0.25 里它只活在 stats
    /// 结构体上，从未出现在状态文档里）。
    #[test]
    fn dataplane_status_json_exports_unknown_peer_drops() {
        let st = WgDataplaneStatus {
            dropped_no_route: 11,
            decrypt_errors: 1,
            unknown_peer_drops: 42,
            ..Default::default()
        };
        let json = st.to_json();
        assert!(
            json.contains(
                "\"dropped_no_route\":11,\"decrypt_errors\":1,\"unknown_peer_drops\":42,\
                 \"carrier_tx_packets\":0,"
            ),
            "{json}"
        );
        // the not-started snapshot is an honest zero, not an absent field
        assert!(
            WgDataplaneStatus::not_started().to_json().contains("\"unknown_peer_drops\":0,"),
            "{}",
            WgDataplaneStatus::not_started().to_json()
        );
    }
}
