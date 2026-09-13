# N11 — WG 骑 ICE 选中连接：上游侦察与本仓实现

SPDX-License-Identifier: AGPL-3.0-or-later
Copyright (C) 2026 NetBird HarmonyOS contributors

日期：2026-09-13。范围：回答「上游究竟怎么让 WireGuard 骑上 ICE 选中的
连接」（含 文件:行号 证据），并记录本仓的最小实现、分用规则、fd 合同、
迁移语义与测试。上游引用全部锚定 pinned commit `791401060d2b`
（`/home/worker/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`）。

---

## 1. 侦察结论：上游是方案 (b) —— 单 socket 共用 + 接收侧按包类型分用

前置排除：

- **不是 (a)**「把选中 pair 的 ice.Conn 直接交给 WG 当传输」：direct（非
  relay）模式下 `ice.Conn`（`RemoteConn`）**不承载数据面**，它只用于
  连通性确认与取对端地址；数据面从头到尾走 WG 自己的 UDP socket。
- **不是 (c)**「WG 另开 socket + 对端做分用」：WG 的 endpoint 是对端的
  **共用 socket 地址**，两侧对称，不存在"对端替 WG 分用"的依赖。

证据链（文件:行号）：

1. **WireGuard bind 拥有 UDP socket，ICE 复用它做 mux**：
   - `client/iface/bind/ice_bind.go:57-96` — `ICEBind` 内嵌
     `wgConn.StdNetBind`（WireGuard 自己的 v4/v6 UDP socket），并持有
     `udpMux *udpmux.UniversalUDPMuxDefault`；`ice_bind.go:131-166`
     `Open()` 先 `StdNetBind.Open()`（打开 WG socket），再把同一对 socket
     包成 mux（`ice_bind.go:293-322 createOrUpdateMux`，
     `NewUniversalUDPMuxDefault{UDPConn: WG 的 conn}`）。
   - mux 交给 ICE agent：`client/iface/iface.go:135-141`（`Up()` 返回
     mux）→ `client/internal/engine.go:657`（`e.udpMux, err =
     e.wgInterface.Up()`）→ `client/internal/engine_generic.go:14-16`
     （`icemaker.Config{UDPMux: e.udpMux.SingleSocketUDPMux, UDPMuxSrflx:
     e.udpMux}`）→ `client/internal/peer/ice/agent.go:57-58`（pion agent
     配置）。**即：ICE 候选端口 == WG 监听端口，同一个 fd。**
2. **接收侧分用（本方案的核心）**：`ice_bind.go:283-344`
   `createReceiverFn` 包裹 WG 的接收循环，每个入包先过
   `filterOutStunMessages`（`ice_bind.go:313-345`）：
   `if isWireGuardMsg(pkt) || !stun.IsMessage(pkt) { continue }` ——
   **WG 形态或非 STUN → 交给 WG**；否则解析 STUN 并
   `mux.HandleSTUNMessage(msg, addr)`（`ice_bind.go:334-346`）→ 送 ICE
   agent。`isWireGuardMsg`（`ice_bind.go:403-421`）= 长度 ≥32 且前 4 字节
   LE u32 ∈ [1,4]；**必须先判 WG**：注释（`ice_bind.go:408-420`）明说
   WG transport 包的 receiver index 恰好等于 STUN magic cookie 时，仅凭
   cookie 判 STUN 会把整条会话的数据误喂给 STUN 处理器。
3. **发送侧**：WG 的 `Send`（`ice_bind.go:216-227`）对 direct 模式走
   `StdNetBind.Send` —— 从**共用 socket** 直接发往对端地址（endpoints
   fake-IP 表只为 relayed 连接存在）；ICE 的检查/保活包经 mux 的
   `GetSharedConn().WriteTo`（`iface/udpmux/universal.go:136-138`）从
   **同一 socket** 发出。
4. **选中 pair → WG endpoint**：
   `client/internal/peer/worker_ice.go:293`（`GetSelectedCandidatePair`）、
   `worker_ice.go:315-325`（`ICEConnInfo{RemoteConn: remoteConn, ...}`）、
   `worker_ice.go:351`（`onICEConnectionIsReady`）→
   `client/internal/peer/conn.go:414-493`：direct 模式
   `directEp = ResolveUDPAddr(iceConnInfo.RemoteConn.RemoteAddr())`
   （`conn.go:453-461`），`ConfigureWGEndpoint(ep, presharedKey)`
   （`conn.go:476`）。**WG endpoint = 对端 ICE 选中地址 = 对端共用
   socket** —— 因为两侧都是"WG 骑共用 socket"，握手天然可达。
5. **保活共存**：pion agent 每 4s 在选中 pair 上发 Binding Request 保活
   （`client/internal/peer/ice/agent.go:22` `iceKeepAliveDefault`），经
   mux 从共用 socket 出；对端 WG 接收循环按第 2 条分用回给 agent ——
   STUN 保活与 WG 数据在同一 socket 上互不误伤。
6. **ICE 重选/失败时的迁移**：
   - 断开：`conn.go:495-557` `onICEStateDisconnected` — 无 relay 时
     `WgInterface.RemoveEndpointAddress(peerKey)`（`conn.go:531`；
     `iface/iface.go:177-183` → configurer）——**回收 endpoint，WG 从此
     无路径可发（fail-closed），绝不静默沿用旧路径**。
   - 重连：新协商（新 offer → `worker_ice.go:100-160` `OnNewOffer` 按
     新 session-id **recreate agent**；`worker_ice.go:576-598`
     Disconnected/Failed → `closeAgent`）→ 成功后再次
     `ConfigureWGEndpoint`。上游不存在"旧 pair 复活复用"的路径。
   - relayed 模式（对照，不在本仓范围）：`ice.Conn` 经本地 UDP 代理
     （`wgproxy`）转交 WG，endpoint 指向代理；`receiveRelayed`
     （`ice_bind.go:383-404`）把代理包注入 WG 接收循环。

## 2. 本仓实现（最小改动，治理 §二.4 不放宽）

本仓与上游的结构差异：**每本地候选一个受保护 socket**（平台适配，见
`client/core/src/ice.rs` 模块文档），而非上游的每族一个 mux socket；
WG 设备另有壳侧 feed 的外层 socket（N7 fail-closed 合同保留）。因此
"骑"的落点是：**把选中 pair 的本地 socket 让渡（dup）给 WG 做 egress，
接收侧由 ICE 会话做与上游一致的分用后喂给设备**。

- `ice_session.rs`：
  - `is_wg_datagram` / `is_stun_message` / `demux_routes_to_wg` ——
    上游 `isWireGuardMsg`/`stun.IsMessage`/`filterOutStunMessages` 条件的
    逐字复刻（WG 判定在前，cookie 重叠不误路由）；
  - `demux_inbound`：WG/非 STUN → `data_rx` 队列（上限 64，溢出计数
    `data_dropped`）；STUN → 检查/保活机器（畸形 STUN 按 upstream 丢弃）；
  - `selected_local_fd()`：选中 pair 本地 socket（自己的 dup）号，供
    消费方 dup（borrowed-number 合同，同 `WgDeviceFeed::feed_wg_socket`）；
  - `take_data_rx()`：编排层每拍取走喂 WG。
- `wg_device.rs`：
  - `WgPeer.egress_fd/egress_local`（+`Drop` 只关自己的 dup）；
    `WgDevice::set_egress_socket`（dup-only attach）；
    `WgDevice::recycle_endpoint`（endpoint=None + 关 egress + 清握手战役，
    对应上游 `RemoveEndpointAddress`）；
  - 全部发送路径（数据/握手/保活/flush/tick 出包/入向应答）改走
    `egress_fd_of(idx)`：**peer 的 egress 优先，设备 socket 兜底**；
  - `WgPeerApplier` 新增三方法（connector.rs）：
    `attach_egress_socket`（默认 Err，fail-closed）、
    `handle_udp_inbound`（默认 0）、`recycle_endpoint`（默认 no-op）；
    `WgDeviceApplier`/`WgDeviceFeed` 完整实现；feed 侧 pre-device 阶段
    缓冲（dup-probe 校验，latest-wins），设备重建时 **egress 先于
    endpoint 重放**（落配即握手，握手必须从选中路径出）。
- `peer_conn.rs`（编排）：
  - `SelectedPair` → 先 `attach_egress_socket(selected_local_fd)` 再
    `apply_endpoint`（顺序即上游语义）；attach 失败 ⇒ 不落配、记录错误、
    不给可用信号（结构性堵死 round-7 的"从旧 socket 发握手"形态）；
  - 入向：每拍 `session.take_data_rx()` → `wg.handle_udp_inbound`
    （设备来源匹配 peer endpoint 的既有规则不变）；
  - `Disconnected` → 立即 `recycle_endpoint`（上游 conn.go:531 对应物；
    可达性翻转，默认路由闸 HOLD）；`Failed` → 回收 + **拆除死会话**
    （Drop 关闭其全部 socket dup）+ 冷却后重新发起（上游 closeAgent →
    handshaker 重启协商的对应物）；
  - `handle_signal(Offer)`：Disconnected 状态、或 Connected 状态但
    **凭证不同**（凭证每会话随机，同会话重发必然同值 → 幂等）时
    recreate 会话 —— 上游按新 session-id recreate 的等价物。
    `set_peers` 对消失的 peer 同步回收 WG endpoint。

fd 合同（§二.4）逐条保持：native 只持有 dup；provider/壳侧原始号永不
read/write/close；会话与设备各自只关自己的 dup；egress attach 是
dup-of-dup，双向只增不借；fail-closed 无未保护回退（空 provider ⇒ 无候选
⇒ 无协商 ⇒ 无 egress ⇒ tunnel_ready=false ⇒ 默认路由 HOLD）。

## 3. 测试（全部离线、注入时钟、无 sleep）

新增 `tests/wg_over_ice_n11.rs`（3 项，双实例全栈闭环：真实 loopback UDP
+ 受保护 provider + 真实 WgDeviceFeed + mock signal 总线）：

1. `wg_handshake_and_probes_ride_the_ice_selected_socket` —— 握手/数据走
   选中 socket 三重证明：egress_local == 本端选中候选、对端落配地址 ==
   本端选中 socket、unknown_peer_drops==decrypt_errors==0（无任何离径
   流量）；探针双向逐字节到达对端 TUN；泵过两个保活周期且跨过 6s
   Disconnected 阈值仍 Connected（STUN 保活与 WG 数据共存不误伤）；
   默认路由闸联动（established+reachable ⇒ allowed）。
2. `wg_from_a_non_selected_socket_never_establishes_a_session` —— 反例
   （round-7 bug 形态复现）：握手从非选中 socket 发往对端 ICE socket：
   握手计数增长、报文确实落在对端 ICE socket 上且为 WG 形态（未被当
   STUN 吞掉）、但 rx=0、peers_with_session=0、闸不放开 —— 即该形态
   永远无法建立会话；正常路径（上一测试）unknown_peer_drops=0 断言其
   不再发生。
3. `selected_connection_failure_recycles_fail_closed_then_renegotiates`
   —— 冻结对端（路径死亡）：+6s Disconnected（endpoint+egress 当拍回收、
  闸 HOLD）→ +12s Failed（会话拆除、重新协商武装、新 OFFER 出现）→
   解冻后经全新会话重收敛（egress 重挂、endpoint 重落、镜像性质
   `A.selected_remote == B.egress_local`、探针在新路径上恢复、闸重新
   放开）。

新增单测（`ice_session.rs` 内）：`wg_vs_stun_classification_matches_upstream_demux`
（四类 WG 报文/STUN 检查/cookie 重叠/畸形 STUN/噪声的分类向量）与
`data_rx_queue_is_bounded_and_counts_drops`。既有 `RecordingWg` 测试桩
（peer_conn_e2e / signal_link）补上 `attach_egress_socket` 实现以匹配新
seam 合同，并新增「attach 先于落配、每 peer 恰一次」断言——测试强度只增不减。

## 4. 与上游的差异表（有意为之）

| 维度 | 上游 791401060d2b | 本仓 N11 |
| --- | --- | --- |
| socket 归属 | WG bind 拥有，ICE mux 复用（每族一个） | 每本地候选一个受保护 socket；选中者让渡给 WG 做 egress（dup） |
| 接收分用点 | WG 接收循环（ice_bind.go） | ICE 会话泵（同一分类规则），非 STUN 喂设备 |
| direct 数据路径 | 共用 socket 直发对端共用 socket | peer egress（选中 socket dup）直发对端选中 socket |
| 保活 | agent 经 mux 从共用 socket 发 | 会话经自身 dup 发（同一 socket） |
| relayed | wgproxy 本地代理 | 不在范围（无 TURN client） |
| 断开迁移 | RemoveEndpointAddress + 新协商 | recycle_endpoint + Failed 拆会话 + 冷却重发起/新凭证 recreate |

## 5. 已知边界（不隐藏）

- Disconnected 后若 ICE 在同一会话内自愈（保活恢复）而未触发 Failed，
  WG endpoint 保持已回收状态（fail-closed 僵尸窗口）：不会泄漏数据，
  但需等 Failed→新协商才恢复转发；上游同形（agent 自愈不重触发
  ConfigureWGEndpoint，需 handshaker 新协商）。
- 设备对入向 WG 包按来源地址匹配 peer endpoint（N6 既有规则）：NAT 后
  源地址重写的拓扑下 srflx pair 的握手可能被计为 unknown_peer（本仓
  探针级设计边界，host/loopback 与真机 host-candidate 拓扑不受影响）。
