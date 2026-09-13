# N6 — WireGuard 数据面设备（wg_device）笔记

状态：**本仓内端到端已验证**（同进程双实例，真实 BoringTun 隧道 + 真实 loopback
UDP + socketpair 模拟 TUN）；**真机未验证**（见 §8）。上游参考（仅引
文件:行号，不抄代码）：pinned `791401060d2b95e5f51e3439c0649729132f571e`
（下称「上游」）与 boringtun-0.7.1（Cargo.lock 锁定，sha256 见
`client/core/build.sh`）。

## 1. 设备模型（`client/core/src/wg_device.rs`）

- **一个设备 = 一个受保护 UDP socket（dup 副本）+ 一个 TUN dup
  （`tun::TunFd`）+ 按 peer 一条 BoringTun 隧道**。隧道层不重写：每 peer
  一个 `crate::wg::Tunnel`（N1BDISC 真机验证过的 ffi 调用序列；
  `wg.rs::Tunnel::new_with` 只是给 `new_tunnel` 补了参数化形态——psk /
  keepalive=0 / index；`unsafe impl Send` 的安全论证见 wg.rs 内注释）。
- **peer 生命周期**：`set_peers(&[WgPeerSpec])` 全量快照对账（同 connector
  的网络图语义）——新公钥建隧道（index 设备内唯一，`INDEX_BASE` 起）、消
  失的公钥拆除、既有 peer 只更新 allowed_ips（会话/endpoint 保持）。先全
  量校验（base64/32 字节/前缀 ≤32）后变更，无半套应用。
- **fd 合同（治理 §二.4）**：WG socket 只经 `mgmtsock::dup_socket_fd`
  （F_DUPFD_CLOEXEC）取 dup，`Drop`/`reattach_socket` 只关自己的 dup，原
  始 fd 永不存储、永不由 native 关闭；TUN 半边复用 `TunFd`（dup + 唯一
  close 点）。`reattach_socket(raw)` 支持本端 socket 重生成（peer/隧道/会
  话保留）。
- **时钟**：设备层全部定时走**方法注入的 `now_ms`**（单调毫秒，与
  `peer_conn` 的注入时钟同风格）：keepalive 节奏、握手重传节奏、握手放弃
  期限、会话过期判定。boringtun 内部定时器（rekey/cookie，`wireguard_tick`）
  仍是其自身真实时钟，由设备 `tick` 定期驱动；测试中真实流逝为毫秒级不会
  触发。**测试无任何长 sleep**。

## 2. allowed_ips 路由规则

- 出向帧解析 IPv4 目的地址（byte 16..20；version≠4 或 <20B → 计
  `short_frame_drops` 丢弃，不 panic）。
- 对所有 peer 的 allowed_ips（登记时掩去主机位）做**最长前缀匹配**：前缀
  相异取更长；前缀相同取先登记者（确定性）。无匹配 → 计
  `no_route_drops` 丢弃（WG 语义：不转发即丢弃）。
- 命中 peer 后 BoringTun 封装：**无会话时 `wireguard_write` 自身排包并产
  出握手 initiation**（boringtun noise/mod.rs `encapsulate`）；会话建立后
  设备执行「decapsulate(空) 直到 Done」把排队包冲出（ffi/mod.rs 注释的
  documented contract），故「先写数据后握手完成」不丢包。
- 入向按**来源地址匹配 peer endpoint**（握手包同规则）：不匹配任何 peer
  → `unknown_peer_drops`；匹配但解密失败（`OP_ERROR`，如 tag 损坏）→
  `decrypt_errors`；成功则明文写 TUN（预算 200ms 的 `write_frame_budget`，
  失败计 `tun_write_errors`）。endpoint 变更只走 `set_endpoint`（ICE 落配
  路径），不做静默 roaming 学习。

## 3. 握手 / keepalive 参数与出处

| 参数 | 默认 | 出处 |
| --- | --- | --- |
| keepalive 间隔 | 25 s | 上游 `client/internal/peer/endpoint.go:14`（`defaultWgKeepAlive = 25 * time.Second`；endpoint.go:130 使用）；N1BDISC 探针同值 |
| 握手重传间隔 | 5 s | boringtun `noise/timers.rs:13`（`REKEY_TIMEOUT`；wireguard.pdf §6：REKEY_TIMEOUT + jitter） |
| 握手放弃期限 | 90 s | boringtun `noise/timers.rs:12`（`REKEY_ATTEMPT_TIME`） |
| 会话过期窗 | 540 s | boringtun `update_timers`（`REJECT_AFTER_TIME * 3` 清隧道；wireguard.pdf §6.2） |
| endpoint 落配即握手 | — | 上游 `conn.go:444-478`（`ConfigureWGEndpoint`，worker_ice.go:293 驱动）；endpoint 不变的重复落配是 no-op（ICE 刷新不重key） |

实现口径：boringtun 层 keepalive 传 0（禁用其内部真实时钟 keepalive），
设备层在注入时钟上发空载荷 transport 包（32 字节）；握手重传 =
`wireguard_force_handshake`（每次新 ephemeral，同 boringtun
update_timers 的重传形态）。线上只影响节奏，不影响协议正确性。

## 4. `tunnel_ready()` 语义与安全闸联动

- **定义**：至少一个 peer 满足「boringtun stats
  `time_since_last_handshake >= 0`（会话已装载）**且**注入时钟上未过期」。
  过期 = 自最近一次观察到新握手超过 `session_max_ms`（默认 540s）→ 记
  expired，`tick` 里判定。语义边界：它回答「本端加密会话能否载荷」，**对
  端存活探测仍由 ICE 层负责**（`peer_conn::ice_ready_for_default_route`），
  connector 的闸对两者相与（`ConnectorShared::apply_update`）。
- **接线（替换 N5c 近似）**：新增 `WgDeviceApplier implements
  connector::WgPeerApplier`（`wg_device.rs`）——`apply_peers` 对账设备
  peer 集、`apply_endpoint` 真的落 endpoint 并触发握手、`tunnel_ready()`
  查真实会话状态。N3-7 闸（`ShellNetworkConfig::default_route_decision`）
  因此从「registry 恒 false / peers>0 近似」变成**真实数据面就绪才放行
  0.0.0.0/0**；fail-closed 保持：无 peer → held、未握手 → held、clear →
  held、force 仅显式 dev opt-in。
- **connector 生产默认仍是 `WgPeerRegistry`**：设备驱动需要壳侧补给真实
  WG socket fd（同 ICE/signal 的 feed 模式），该 NAPI 接线属于后续增量
  （本次禁改 `client/entry/**`）。registry 的 `tunnel_ready` 仍恒 false，
  闸不回退。

## 5. 测试证据（`client/core/tests/wg_e2e.rs`，10 例全过）

双实例（一次三实例）同进程：真实 BoringTun + 真实 loopback UDP（线缆）+
datagram socketpair 模拟 TUN（沿用 tun_fd_contract 宿主模式；datagram
socketpair 保帧界，一写一读一帧）。设备访问一律走 `WgDeviceApplier`
（生产 seam 面）。注入时钟驱动，零 sleep：

1. `dual_instance_bidirectional_payload_end_to_end` — 握手前
   `tunnel_ready`=false；握手后 true；A TUN 写入 IP/UDP 包 → B TUN 读出
   **逐字节相等**；反向亦然。
2. `wire_bytes_are_ciphertext_not_plaintext` — 线缆字节（B 裸 socket
   tap）≠ 明文、长度=明文+32（WG transport 开销）、payload 任意窗口不出现
   ；同明文两次密文不同（新 nonce）且各自正确解密。
3. `allowed_ips_longest_prefix_routes_and_unmatched_drops` — /32 压过 /16
   （10.99.0.2 只到 B）、10.99.7.7 只到 C、10.200.0.1 计
   `no_route_drops` 且不出网、非 IPv4/短帧计 `short_frame_drops`。
4. `tunnel_ready_drives_the_default_route_gate_through_the_real_seam` —
   闸四态全钉：未握手 held(data-plane-not-ready)、无 peer
   held(no-usable-peer)、force allowed(forced token)、握手后
   allowed（0.0.0.0/0 真的导出）、clear 后回到 held。
5. `handshake_retransmission_recovers_a_dropped_initiation` — 首个
   initiation 在线缆上被丢弃；注入时钟推进 250ms 后恰好一次重传
   （initiations==2，148B，新 ephemeral ⇒ 字节不同）；投递后会话建立并载
   荷（glare 由 tie-breaker 收敛，双向排水确定性收敛）。
6. `endpoint_change_switches_the_path_and_keeps_traffic_flowing` — B 端
   socket 重生成（`reattach_socket`）+ A 端 `apply_endpoint` 新 port →
   新路径重握手、双向载荷走新路径、旧 socket 静默。
7. `unknown_sources_and_decrypt_failures_are_dropped_and_counted` — 陌生
   源 3 发计 `unknown_peer_drops`；真密文翻转 1 bit 计
   `decrypt_errors` 且不写 TUN；随后诚实流量照常（会话存活）。
8. `injected_clock_drives_keepalive_and_session_expiry` — 数据新鲜抑制
   keepalive；过 300ms 窗口发恰一个 32B keepalive（不产生 TUN 帧）；过
   1000ms 会话窗 `tunnel_ready`（seam 面）变 false；对端时钟独立不受染。
9. `ice_selected_pair_lands_on_the_real_device_and_carries_payload` —
   两个 `PeerIceOrchestrator`（mock signal + 注入时钟）选 pair，
   selected-pair 的 `apply_endpoint` 直接落真实设备；落配 endpoint == 对
   方被选中的候选 socket（按 port 找回 raw 并 `reattach`）→ 设备在
   ICE 选中的 socket 上完成握手、双向载荷、闸放行。
10. `applier_peer_set_follows_network_map_snapshots` — 快照对账 1→2→1
    peer、allowed_ips 原位更新、未知 peer endpoint 落配被拒（fail-closed）
    、坏前缀/坏 key 的快照整体拒绝且不改状态。

测试基础设施注：boringtun ffi 在首次 `new_tunnel` 时装了进程级
panic→SIGSEGV hook（ffi/mod.rs PANIC_HOOK），测试在首个隧道建立后重装打
印 hook，失败可诊断；cdylib 侧仍 panic=abort，设备层本身无 panic 路径。

## 6. 与真机验证的差距（明确未验证项）

- **未在 HarmonyOS 真机上运行过本增量的任何代码**。已真机验证的仅是
  `wg.rs` 的握手/加解密/单隧道 UDP 收发（N1BDISC 探针）——本增量复用该
  ffi 序列但组合形态（多隧道共存、按源分发、排包冲刷、keepalive 注入时
  钟）只在宿主上验证。
- **TUN 半边是 socketpair 模拟**：真机 TUN 的 MTU/分片、内核协议栈回包
  （ICMP/MLD 等内核杂帧，探针在 `wg_fwd_probe` 里观测过）、O_NONBLOCK 共
  享 ofd 行为、readv/EAGAIN 背压，均未验证。
- **受保护 socket（`VpnConnection.protect`）路径未验证**：宿主 socket 无
  protect 概念；壳侧 feed WG socket fd 的 NAPI/ArkTS 接线是后续增量。
- **rekey / 长会话老化**未实测（boringtun 内部真实时钟定时器——
  REKEY_AFTER_TIME 120s、cookie 限速——在测试时间尺度内不触发；设备层过
  期逻辑用注入时钟验证）。
- **性能/MTU/并发多 peer 压力**、NAT/非 loopback 网络、IPv6 数据面（设备
  IPv4-only，IPv6 帧计入 short_frame_drops）均未验证。

## 7. 文件清单

- `client/core/src/wg_device.rs`（新增，SPDX AGPL-3.0-or-later）：设备 +
  seam。
- `client/core/src/wg.rs`：`Tunnel`/op 常量/`key_to_b64` 提为
  pub(crate) + `Tunnel::new_with`（探针路径不变）。
- `client/core/src/lib.rs`：注册 `wg_device` 模块。
- `client/core/src/connector.rs`：仅文档——seam 限制声明、trait doc、闸
  doc 更新为「N6 起存在真实驱动」；无行为改动。
- `client/core/tests/wg_e2e.rs`（新增）：§5 的 10 例。
- 无新增依赖（boringtun/base64 既有），`THIRD-PARTY-NOTICES.md` 不需改。

## 8. 验收命令（实跑输出见交付回复）

1. `bash client/core/build.sh` → exit 0（ELF/符号门全过）
2. `cd client/core && cargo test --offline --locked` → 262 passed / 0
   failed（含新增 10）
3. `bash client/build.sh` → exit 0（HAP 产出）

## 9. N7 增量：生产接线（`WgDeviceFeed`——真实设备成为生产默认）

状态：**本仓内已验证**（Rust 侧 feed 入口喂入受保护 UDP socket +
socketpair TUN，双端真实握手、`tunnel_ready=true`、默认路由放行、双向载
荷）；**真机未验证**（§6 的差距清单对 N7 依然成立，另见文末）。

### 9.1 生产默认切换与开关

- `connector` 的生产默认 `WgPeerApplier` 由 `WgPeerRegistry` 切换为
  [`crate::wg_device::WgDeviceFeed`]（`connector.rs` 两个生产 start 路径
  `connector_start_with_socket_json` / `connector_start_json` 均构造并以
  其为 wg seam）。两个 NAPI start 入口均无"是否用真设备"的开关——缺
  feed 时设备不启动，这就是默认（fail-closed）。
- `WgPeerRegistry` 保留为**测试/对照** seam（既有测试全部不动）；N6 直连
  形态 `WgDeviceApplier` 供宿主测试/驱动注入。`ConnectorHandle::spawn`
  仍是注入式（`wg: Arc<dyn WgPeerApplier>` + 新增 `wg_feed:
  Option<Arc<WgDeviceFeed>>`，`Some` 时由 handle 拉起数据面泵线程并在
  stop flag 上退出）。

### 9.2 feed 契约（壳侧 → native）

- `connector_wg_socket_feed(fd)`：喂**受保护** WG 外层 UDP socket
  （`wg_fwd_open` 打开 + `VpnConnection.protect`，先于任何数据报）。fd 以
  number 过界，先做 dup 探针校验（拒绝死 fd：`socket-fd-missing` /
  `socket-fd-invalid`），原号**借用存储**；设备建成时 native 再
  `dup_socket_fd` 取自己的 dup，从不使用/关闭原号。
- `connector_tun_fd_feed(fd)`：喂平台 TUN fd（`VpnConnection.create()`
  产出）。同样先 dup 探针校验、原号借用存储；设备建成时经
  `TunFd::dup_from_raw` 取 dup（可执行 fd 合同：dup 后复查原号仍开、
  close 只发生在 dup 上）。壳侧保留原号，其关闭只属于
  `VpnConnection.destroy()`。
- 返回 `{"ok":true,"device_up":<bool>}`；错误 token 同族
  （`no-connector` / `no-wg-device` / `socket-fd-missing` /
  `socket-fd-invalid`）。
- 建成顺序无关（先 socket 后 tun 或反之均可）；**两个 feed 齐备**才
  `WgDevice::adopt`。建成失败（fd 死亡等）时丢弃两个已存 fd 号并计数、
  打点，重新喂入一对新 fd 才会重试——不做半套设备。

### 9.3 feed 前的行为（fail-closed 条件）

- 缺任一 feed ⇒ 设备不存在 ⇒ `tunnel_ready()=false` ⇒ N3-7 闸 HOLD
  `0.0.0.0/0`；控制面调用（`apply_peers`/`apply_endpoint`）**缓冲并照常
  应答**（网络图/ICE 落配不会楔死），设备建成时全量重放（endpoint 每
  peer 只保留最新一条，重复落配不触发多余握手）。
- **没有任何未保护/自建 socket 的隐式回退**；未 feed 的 slot 是纯惰性
  状态，唯一的 socket 创建者是壳侧 `wg_fwd_open` + `protect`。
- connector stop ⇒ `clear()`：设备拆除（dup 关闭）、缓冲与已喂 fd 号全部
  丢弃；之后必须重新喂入一对新 fd 才会再有数据面。

### 9.4 状态字段（`connector_status()` 的 `wg` 对象）

`wg:{fed_socket, fed_tun, device_up, ready, peers_with_session, handshakes,
tx_packets, rx_packets, dropped_no_route, decrypt_errors}`（结构体
`WgDataplaneStatus`）。全部来自 `WgDevice` 既有计数器
（`WgDeviceStats.handshake_initiations` → `handshakes`、
`no_route_drops` → `dropped_no_route`），无任何密钥材料；registry 等
"无设备能力" seam 返回 `None`，渲染为全 false/0 默认。同时
`connector_network_config()` 的默认路由判定改为**读取时实时重判**
（`refresh_net_gate`：`wg.tunnel_ready()` × ICE 可达视图 × force 旗标，
路由表从"未过滤记录"重建）——sync 之后 feed/握手才完成（生产时序必然如
此）不再需要等下一次 sync 才放行/收闸。

### 9.5 测试证据（新增）

- `client/core/tests/wg_feed_n7.rs`（4 例）：
  `production_path_real_wgdevice_closed_loop`（feed 入口喂真实 fd 对 →
  缓冲重放 → 双端真实握手 → `tunnel_ready=true` → N3-7 闸放行 → 双向载荷
  字节精确 + 真实计数器）；`connector_status_reflects_fed_device_and_stop_t
  ears_it_down`（真实 `ConnectorHandle`：`status().wg` 反映设备、
  `status_json` 含 `wg` 对象、stop 拆数据面）；`missing_feed_fails_closed_a
  nd_takes_no_unprotected_socket`（缺 feed ⇒ 不建设备、闸 HOLD、ICE
  provider `taken()==0`、死 fd 拒收）；`fd_contract_native_uses_only_dups_
  originals_stay_caller_owned`（原号在 feed 后立即可被调用方关闭而设备照
  常握手/载荷 = 只用 dup；设备拆除后原号仍 open 可用 = native 不关原号）。
- `wg_device.rs` 单测 4 例（缓冲/重放、死/缺 fd 拒收、未注册 peer 拒绝、
  clear 语义）+ `connector.rs` 单测 2 例（`status_json` 的 `wg` 契约、
  `network_config_gate_refreshes_with_live_tunnel_ready` 活跃闸）。

### 9.6 N7 新增未验证项（真机）

- `protect` 后的 WG socket 在真机上收发（运营商 NAT 下 keepalive/重钥）；
- 平台 TUN fd 的 MTU/杂帧（内核注入的 ICMP/MLD 等）与 `O_NONBLOCK` 共享
  ofd 行为（继承 §6 清单）；
- `VpnConfig` 在 `create()` 时刻固定 ⇒ 默认路由即使数据面后来 ready 也不
  会追加安装（平台限制，N3-6 已声明"运行中变更 NOT applied"）；壳侧
  `VPN_WG_DATAPLANE_FED` 之后的真实端到端连通需真机联调。

---

## 10. N8 增量：受控重建（VpnConfig 固定 ⇒ 数据面就绪后默认路由装不上）

**问题**（N7 实测确认）：`VpnConnection.create(config)` 一旦调用路由集固
定；安全闸（N3-7/N5c/N7）要求 WG 握手完成才允许 `0.0.0.0/0` 进隧道 ⇒
create 时闸必然 HOLD ⇒ 默认路由永远装不上（除非重建连接），full-tunnel
实际不可用。**N8 方案**：不绕闸、不假装放行，而是**数据面就绪后由壳侧
受控重建连接**——先按 HOLD 集合 create（无默认路由、无黑洞窗口），握手
完成后重建一次把默认路由装进新连接。

### 10.1 重建时序（核心半边）

1. `refresh_net_gate`（每次 `connector_status()` /
   `connector_network_config()` 读取时执行）照旧用**实时 readiness** 重判
   默认路由闸，并据此得出**期望路由集** `desired`（未过滤路由记录按
   allowed 过滤后的 canonical 网络串）。
2. 期望集合与**已应用集合**（壳侧 ACK 的 `applied_route_set`，见
   §10.2）不一致 ⇒ 置 `recreate.required=true`（级别信号，重复读取不重复
   计数，直到壳侧 ACK）；一致 ⇒ 清除。壳侧从未 ACK 过 ⇒ 信号保持沉默
   （N7 行为完全保留）。
3. 壳侧执行重建：destroy 旧连接 → 新 `create()`（期望路由集）→ 新
   TUN fd 经既有 `connector_tun_fd_feed` 喂入 → 设备**原位替换** TUN（见
   §10.3）→ 壳侧 ACK（`is_recreate=true`）。

### 10.2 状态面与 NAPI

- `connector_status()` 新增 `recreate:{required, count, exhausted,
  cooling_down, reason}`（`RecreateStatus`，无密钥材料）。`reason` token：
  `none` / `route-set-changed` / `limit-reached`。
- 新增 NAPI `connector_route_set_applied(routesJson, isRecreate)`：
  `routesJson = {"routes":["0.0.0.0/0","a.b.c.d/p"]}`（canonical 网络串），
  `isRecreate=false` 记录初始 create 的已应用集合（只武装比较），`true`
  记录一次完成的重建（`count+1`、武装冷却窗、清 `required`）。错误
  token：`no-connector` / `route-set-invalid`（坏形状/空集合——空平台路由
  集本身就是黑洞形态，永不接受）。
- connector stop 重置全部重建簿记（`applied_route_set` / 计数 / 冷却）。

### 10.3 TUN fd 替换语义与 fd 合同（`wg_device.rs`）

- `WgDevice::replace_tun(raw)`：先经 `TunFd::dup_from_raw` 为**新** fd 取
  dup（失败则旧 TUN 原样保留，无半状态），再换入新 `TunFd`；旧 `TunFd`
  的 Drop 只关**旧 dup**。壳侧原号的关闭仍只属于
  `VpnConnection.destroy()`——重建时序里 destroy 发生在喂新 fd **之前**，
  此时设备靠自己的旧 dup 存活（dup 持有 open-file description），native
  从不使用/关闭原号。
- **会话保留（取舍：保留而非重建会话）**：BoringTun tunnel 是纯字节层状
  态机，从不引用 TUN fd；WG 外层 UDP socket 在重建中不变。因此换 TUN
  零成本保留全部 peer 会话——重建后**不需要重新握手**（测试断言握手计数
  不变、`tunnel_ready` 全程为真、载荷立即双向可通）。重建会话方案被否：
  要拆会话、重新握手，徒增不可用窗口且无任何收益。
- feed seam：设备在位时 `feed_tun` = 替换；`feed_wg_socket` **同号幂等
  no-op**（壳侧重放 feed 序列无副作用）、**异号拒绝**
  （`socket-fd-conflict`，新增 token）——受保护外层 socket 不可换。设备
  不在位时一切仍按 N7 冷启动规则。

### 10.4 幂等 / 有界策略（防重建风暴）

- **同一期望集合只重建一次**：`required` 是级别信号，ACK 后 `desired ==
  applied` ⇒ 不再置位。
- **次数上限**：`RECREATE_MAX = 3`（每 connector 生命周期），耗尽即
  `exhausted=true`、`reason=limit-reached`，永不再请求；快照闸保持诚实
  （闸关了照样在 `default_route` 字段如实报告，只是不再自动重建）。
- **冷却**：`RECREATE_COOLDOWN_MS = 30_000`（单调钟）。一次重建完成后若
  期望集合再次改变，冷却窗内 `cooling_down=true` 且 `required=false`（壳
  侧无法被风暴）；周期性读取会在窗口过后重新升起请求。
- 双向触发：数据面死亡（ALLOWED→HOLD）同样产生 `desired != applied`——
  把默认路由**摘下来**同样重要（否则黑洞规则被绕过）；预算上限把振荡封
  顶。

### 10.5 失败语义

重建途中任一环失败（新 fd 死亡/feed 拒收/ACK 失败/壳侧 create 失败）：

- Rust 侧：替换类失败**不触碰旧状态**（设备带着旧 TUN 与会话原样运行，
  无半状态；死 fd 在边界 dup 探针即拒收 `socket-fd-invalid`）；随后壳侧
  fail-closed 拆除（connector stop ⇒ `clear()`）回落到 N7 冷态（无设备、
  无 feed、`tunnel_ready=false`、默认路由 HOLD）。
- 壳侧（见 n3 notes N8 节）：`connectorStop()` + `destroyOnce`，回到未连
  接；不重试（预算与一次性 destroy 语义天然防循环）。

### 10.6 测试证据（新增）

- `connector.rs` 单测 3 例：
  `recreate_required_raises_once_on_gate_open_and_clears_on_recreate_ack`
  （HOLD→ALLOWED 恰好产生一个请求、级别信号不重复计数、ACK 后清除、期望
  集合含 `0.0.0.0/0` 且快照 routes 真的导出）；`recreate_flap_is_bounded_
  by_cooldown_and_limit`（连续翻转受冷却窗与 `RECREATE_MAX=3` 约束、耗尽
  后 `limit-reached` 永不再请求）；`route_set_ack_rejects_bad_shapes_and_
  no_connector`（ACK 形状校验/空集合拒绝/无 connector 拒绝）。
- `tests/wg_recreate_n8.rs` 集成 3 例：
  `tun_replacement_keeps_sessions_and_carries_bidirectional_payload`（模拟
  平台 destroy 关原号 → 设备靠 dup 存活 → 喂新 fd 原位替换 → 无重新握手
  （握手计数不变）→ 新 fd 双向载荷字节精确 → 旧 fd 停用（停用 fd 接收队
  列里的帧原样未被消费）→ 设备 fd ≠ 任何原号（dup 合同）→ 新原号仍归壳
  侧）；`recreate_dead_fd_refused_no_half_state_then_fail_closed_teardown`
  （死 fd 拒收 `socket-fd-invalid`、设备/会话原样、载荷照通 = 无半状态；
  随后 `clear()` 回落冷态 + 默认路由 HOLD）；`wg_socket_feed_idempotent_
  same_fd_and_refuses_swap_while_device_up`（同号幂等不重建设备——会话保
  存、异号拒绝 `socket-fd-conflict`、clear 后冷启动规则不变）。

### 10.7 N8 新增未验证项（真机）

- `VpnConnection.destroy()` → 重新 `createVpnConnection` + `create()` 的
  平台侧行为（能否连续多代创建、destroy 后旧 fd 号是否立即失效）——宿主
  无法验证，物理门 N2b 承接；
- 重建窗口（destroy 到新 feed 之间）的真实流量中断时长与平台对 TUN fd
  的清理时机；
- `connector_route_set_applied` ACK 的 canonical 网络串与平台
  `RouteInfo` 目的串的一致性（`10.99.0.0/24` 形态）需真机比对；
- 重建后 WG 会话在真实内核 TUN 上的无缝续流（宿主 socketpair 上已证明
  逻辑无缝，真实 TUN 的缓冲/MTU 行为未验证）。
