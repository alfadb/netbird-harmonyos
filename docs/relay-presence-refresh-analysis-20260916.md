# relay 对端在线状态在「对端重连」后是否正确刷新——根因分析（只读）

- 日期：2026-09-16。性质：只读根因分析，未改任何代码/设备/配置，未 commit，未联网（上游源码读本仓内已锚定的 `refs/netbird-791401060d2b/` 只读副本）。
- 分析对象：`client/core/src/{relay_client.rs,relay.rs,connector.rs,wg_device.rs,relay_testserver.rs}`；
  规格 `docs/relay-client-spec-20260914.md`（下称「规格」）；
  证据 `/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260916-0008/`（下称「AUTH-0008 目录」，仅读）。
- 上游引用均相对 `refs/netbird-791401060d2b/`（NetBird 0.78.1，commit `791401060d2b…`，规格 §0 声明的唯一权威依据）。
- 本文不写入任何凭据；引用的 URL 为既有诊断留档中的中继主机名，非凭据。

---

## §1 结论

**缺陷存在（置信度：高）。** 我方 relay 客户端对「远端 peer」的在线状态在**本端 relay 会话存续期间**收到 `PeersWentOffline` 后即进入 `presence=false` 的**终态**：既不退订、也不重开 lane、更无周期刷新；而上游服务端（0.78.1 语义）的「上线兴趣」对每个订阅**只投递一次**（`PeersOnline` 事件送达后即消费），因此对端重新上线后我方**收不到第二次 `PeersOnline`**，出向帧被本地以 `relay-peer-offline` 永久拒收——与本轮 AUTH-0008 臂 A2 的实测（拒收 100→800、设备侧 relay `state=ready` 而 `wgSessions=0`）完全吻合。唯一自愈路径是**我方自己的 relay 会话结束重连**（presence 全清 + lane 全 detach + 全新 `open_conn`），A2 窗口内未发生。

---

## §2 拒收判定位置（file:line）

`N13_WG|carrier-reject|kind=relay-wss|reason=relay-peer-offline` 的产生链（对端侧，即运行我方软件的 nbinterop 主机）：

1. **WG 出向载体分发**：`client/core/src/wg_device.rs:1046-1086`（`dispatch`）——无 ICE endpoint 时走注入的 carrier；`carrier.send_datagram(datagram)` 返回 `Err(reason)` 时计数 `carrier_rejects` 并**有界日志**（≤3 条或每 100 条打 1 条，`client/core/src/wg_device.rs:1076-1083`，格式串在 :1079）。这解释了 A2 窗口内恰好在 count=200/300/…/800 处各有一行日志。
2. **错误串映射**：carrier 是 `RelayWgCarrier`（`client/core/src/relay_client.rs:1537-1567`）；其 `WgEgressCarrier` trait 实现把任意错误映射为 `format!("relay-{}", relay_class_token(e.class()))`（`client/core/src/relay_client.rs:1569-1575`），`relay_class_token` 中 `RelayErrorClass::PeerOffline => "peer-offline"`（`client/core/src/relay_client.rs:1604-1605`）；`RelayClientError::PeerOffline → RelayErrorClass::PeerOffline` 的归类在 `client/core/src/relay_client.rs:625`。
3. **判定本体**：`RelayClient::send_to_peer`（`client/core/src/relay_client.rs:1433-1460`）在 ：1442-1444 做 `if self.shared.is_offline(peer) → Err(RelayClientError::PeerOffline)`；`is_offline` 的定义是**本地 presence 缓存查询**：`presence.get(peer) == Some(&false)`（`client/core/src/relay_client.rs:1292-1294`）。
4. **presence 缓存**：`Shared.presence: Mutex<HashMap<PeerId,bool>>`（`client/core/src/relay_client.rs:1259-1261`）。置 `true` **仅**因收到 `PeersOnline`（:2028-2038）；置 `false` **仅**因收到 `PeersWentOffline`（:2040-2053）；**整表清空仅发生在 relay 会话结束**（:2144）。

**判定依据结论**：拒收依据是**本地在线集合（presence 缓存）**，不是订阅结果查询、也不是向服务端核实；而该缓存的 `false` 一旦写入，会话内**没有任何代码路径能把它翻回 `true`，除非再收到一帧 `PeersOnline`**。

---

## §3 订阅与在线状态机（file:line）

### 3.1 我方何时发 `SubscribePeerState`

- `Frame::SubscribePeerState` 在生产代码中**只有一个构造点**：会话循环的 Subscribe 命令处理 `client/core/src/relay_client.rs:2084-2095`（构造在 :2089）。
- 唯一公开入口 `RelayClient::open_conn`（`client/core/src/relay_client.rs:1467-1488`）：发订阅后在 `waiters`（:1984-1987）挂 30s 超时（`OPEN_CONN_TIMEOUT`，:157-159）等 `PeersOnline`。
- 生产代码中 `open_conn` **只有一个调用方**：`RelayCarrier::ensure_lane`（`client/core/src/connector.rs:2035-2079`，调用在 :2044）。`ensure_lane` **第一行就短路**：lane 已挂载（`attached.contains_key`）直接返回 `true`（`client/core/src/connector.rs:2035-2038`），不再订阅。
- `ensure_lane` 的调用方 `ensure_lanes`（`client/core/src/connector.rs:2082-2094`）由 pump 循环在每轮 Ready 态执行（`relay_carrier_pump`，`client/core/src/connector.rs:2164-2197`；Ready 分支 ：2176，500ms 周期重查 ：2193）。
- **结论（Q2）**：`SubscribePeerState` 只在「lane 挂载」这一刻发一次。会话内重复触发只可能发生在 lane 被 detach 之后：relay 客户端整体换新（`attach_client`，`client/core/src/connector.rs:1986-1995`）、relay 非 Ready 全 detach（:2177-2178）、peer 移出网络图（`set_peers`，:2009-2020）、`shutdown`（:2150-2157）。**对端 offline→online 循环不触发其中任何一条。**
- `UnsubscribePeerState` 生产代码唯一构造点：`open_conn` 30s 超时后的退订（`client/core/src/relay_client.rs:2110-2128`，构造在 :2126）。**`PeersWentOffline` 处理路径不发退订**（:2040-2053）——这是与上游客户端的第一个偏差（见 3.3）。

### 3.2 `PeersOnline` / `PeersWentOffline` 的处理

- `PeersOnline`：`presence[peer]=true` + 唤醒该 peer 的 `open_conn` 等待者（`client/core/src/relay_client.rs:2028-2038`）。**无等待者时也照常写缓存**（这点关键：事后到达的 `PeersOnline` 本可翻回 presence）。
- `PeersWentOffline`：计数 `peers_went_offline_rx` + `presence[peer]=false` + 以 `ConnectionLost` 失败该 peer 的等待者（`client/core/src/relay_client.rs:2040-2053`）。**既不退订、不 detach lane、不触发任何重订阅。**
- relay 会话结束：presence 整表清空（`client/core/src/relay_client.rs:2144`）；pump 在非 Ready 态全 detach（`client/core/src/connector.rs:2177-2178`），Ready 后 `ensure_lanes` 重新 `open_conn`——即模块文档声明的设计「Presence/subscription state does NOT survive a reconnect — D must re-`open_conn` after observing `state() == Ready` again」（`client/core/src/relay_client.rs:101-102`）。**该设计只覆盖「我方会话重连」，未覆盖「对端 offline→online」。**

### 3.3 真实服务端语义（规格 + 上游 0.78.1 源码，回答 Q3 的「是否会被通知」）

规格对服务端推送的规定：离线目标「注册兴趣、静默等待」（规格 §4.1，`docs/relay-client-spec-20260914.md:163`）；「对端离线推送：服务端发 `PeersWentOffline` → 客户端关闭对应 per-peer 连接**并退订**」（规格 §4.2，`docs/relay-client-spec-20260914.md:173`）。规格未直接写明「重新上线时服务端是否向既有订阅者再推 `PeersOnline`」——**这一点由上游服务端源码给出确定答案**：

- 订阅时同时登记两类兴趣：`GetOnlinePeersAndRegisterInterest` 无条件 `listener.AddInterestedPeers`（`relay/server/store/store.go:81-96`，:87），两类兴趣都写入（`relay/server/store/listener.go:38-46`）。
- **`PeersOnline` 事件投递是一次性的**：`listener.peerComeOnline` 投递后即 `delete(l.interestedPeersForOnline, peerID)`（`relay/server/store/listener.go:107-121`，删除在 :120）。此后同一连接上的既有订阅**不会再收到该 peer 的 `PeersOnline` 事件**，直到一次**新的 `SubscribePeerState`** 重新登记（:38-46）。
- **`PeersWentOffline` 的兴趣是常驻的**：`peerWentOffline` 投递后不删除（`relay/server/store/listener.go:92-105`）——所以每个 offline 周期都能收到 PWO，这正解释了「PWO 收得到、第二次 PeersOnline 收不到」的不对称。
- 服务端在每个对端握手成功时广播 `PeerCameOnline`（`relay/server/relay.go:153-155`）、在连接结束时广播 `PeerWentOffline`（`relay/server/relay.go:159-163`），遍历所有 listener（`relay/server/store/notifier.go:45-61`）；退订删除两类兴趣（`relay/server/store/listener.go:48-56`）。
- 订阅时目标已在线的「立即应答」路径**不消费**在线兴趣（`relay/server/peer.go:237-256`，:250 登记兴趣、:256 直接应答，不经过 `peerComeOnline`）——即「订阅时在线」的场景下，**首个** offline→online 循环还能收到一次 `PeersOnline`（随后被消费），**之后**的循环就只收 PWO 不收 `PeersOnline`。
- 上游**客户端**靠什么不踩坑：收到 PWO 后关闭该 peer 连接**并退订**（`shared/relay/client/client.go:605-607` 分发、:784-803 `closeConnsByPeerID` 内 `UnsubscribeStateChange`），后续需要该路径时重新 `OpenConn` → 新订阅把两类兴趣都恢复。规格 §4.2 :173 记录的正是这个循环。**我方移植版丢掉了「PWO→退订/关连接→按需重新 OpenConn」这半圈。**

### 3.4 缺陷判定（Q4）

**存在。** 推演链（全部有代码依据）：

1. lane 挂载成功（曾收到一次 `PeersOnline`）→ 该 peer 的服务端在线兴趣要么在「事件投递」时已被消费、要么将在第一次 offline→online 循环时被消费（§3.3）。
2. 对端掉线 → 我方收到 `PeersWentOffline`（离线兴趣常驻，必然送达）→ `presence=false`（`relay_client.rs:2045`）。
3. 我方**不退订、不 detach lane**（:2040-2053 无任何后续动作；`ensure_lane` 短路 `connector.rs:2035-2038`）→ 不发新订阅 → 服务端在线兴趣未恢复。
4. 对端重新上线（Auth 成功，服务端广播 `PeerCameOnline`）→ 我方 listener 的在线兴趣已被消费 → **收不到 `PeersOnline`** → presence 停在 `false`。
5. 我方 WG 出向该 peer → `dispatch` → carrier → `send_to_peer` → `PeerOffline` → `relay-peer-offline`（§2），**直至我方自己的 relay 会话结束**（:2144 清缓存 + `connector.rs:2177-2178` detach + 重新 `open_conn`）。

旁证：A2 窗口内对端 relay 一直是同一会话（`state":"ready","reconnects":1` 全窗不变，见 §4），正是「会话不复位 → presence 不清」的形态；而 A1 全新启动后一切正常，因为全新订阅必然拿到一次 `PeersOnline`。

---

## §4 对端 A2 窗口日志时间序列（Q5）

材料：AUTH-0008 目录 `peer-log-L-arm1-window.log`（对端窗口，2975 行）、`peer-log-L-arm2-window.log`（2732 行）、`hilog-L-arm2-post-20260916T2313-full.log`（设备侧）、`EXECUTION-RECORD-L1-20260916T2327.md`（执行记录，含时刻）。窗口切片无逐行时间戳，时刻以执行记录为准。

| 时刻（2026-09-16） | 事件 | 证据 |
|---|---|---|
| 臂 A1 前 | 对端（PID 286618）已在跑，relay 会话为第 2 条（`reconnects:1,last_error_class:"io"`），探针 1s/条；**零 carrier-reject** | EXECUTION-RECORD §2；arm1 窗口状态行 |
| 22:49:40→22:50:02 起，8min | **臂 A1**（设备 e45f8c9 全新上线）：对端窗口**无任何 carrier-reject**；窗口内见对端 lane（重）挂载 `connector: relay carrier lane attached (peers-online)`（`peer-log-L-arm1-window.log:94`）；设备 `wgSessions=1,wgRxToTun=8385` | EXECUTION-RECORD §4 |
| ≈22:58–23:00（臂间） | A1 设备拆除重部署 → 设备 relay 连接断开。对端无逐行日志，但 **A2 前基线 reject=100**（探针 1 条/s ≈ 100s 累积）——**拒收正是从设备离线窗口开始的**，与「收到 PeersWentOffline → presence=false」的时刻吻合 | EXECUTION-RECORD §3/§5.3（其 §9.3 亦承认此 100 的来源当时未定） |
| 23:00:45 | A2 设备（HEAD）首次启动遇锁屏，未形成有效 relay 会话 | EXECUTION-RECORD §5 |
| 23:13:22 起，8min（23:13:5x–23:21:4x） | **臂 A2**：设备 relay **`state=ready,reconnects=0,framesTx=223,framesRx=21`**（framesRx≈服务端 HealthCheck 量级，即**对端零帧到达**）；设备 `wgSessions=0,wgRxToTun=0`（223 条出向握手无人应答） | `hilog-L-arm2-post-20260916T2313-full.log` 末段 `VPN_RELAY_STATUS`；EXECUTION-RECORD §5 |
| 同窗口内 | 对端 `carrier-reject|reason=relay-peer-offline` **count 100→800**：日志行 `peer-log-L-arm2-window.log:449(200) :741(300) :1166(400) :1602(500) :1947(600) :2376(700) :2727(800)`；首条状态行 relay `state":"ready","reconnects":1,"last_error_class":"io"`、`carrier_rejects:145`、`peers_with_session:0`（`peer-log-L-arm2-window.log:4`）；**全窗口无任何 lane attach/detach 行**（lane 全程挂着、从未重开）；`N6_WG_DEVICE|handshake-campaign-deadline` ×7（对端握手攻势无响应） | 见左 |

**时间序列判定（Q5）：与假设一致。** 拒收起于设备离线窗口（PWO→presence=false 的时刻），设备 23:13 重新上线且 relay 就绪后拒收**持续不减**（又 +700），对端 lane 全程未重开、relay 会话全程未复位——即「设备回来后对端在线状态未刷新」的直接实证。对端日志中**没有**（也不可能有）`PeersWentOffline`/`PeersOnline` 的逐行记录：我方客户端对这两个帧只改缓存与计数，不打日志（`relay_client.rs:2028-2053`）——时刻判定靠计数器差分，见 §7.1。

---

## §5 最小修复方案与风险（Q6，只设计不实施）

### 推荐方案 A：`PeersWentOffline` 触发 lane 重开（对齐上游生命周期）

- **机制**：connector 侧为每个已挂载 lane 检测「该 peer presence 已翻 false」（检测缝可选：给 `RelayClient` 增加公开的 `is_offline(peer)`/presence 快照查询——`Shared::is_offline` 已存在，`relay_client.rs:1292-1294`，仅差公开化；或 pump 每轮比对 `stats().peers_went_offline_rx` 差分）。命中即 **detach 该 lane**（复用 `connector.rs:2096-2108` 的单 peer 版逻辑），下一个 pump pass（≤500ms，`connector.rs:2193`）`ensure_lane` 自然重跑 → 全新 `open_conn` → 全新 `SubscribePeerState` → 服务端两类兴趣重新登记（`listener.go:38-46`）→ 若对端已回归则立即应答 `PeersOnline`（`peer.go:256`），否则事件投递。presence 由这条新路径的 `PeersOnline` 翻回 `true`（`relay_client.rs:2028-2038`）。
- **可选加固（上游同款）**：`PeersWentOffline` 时对无等待者的 peer 补发 `UnsubscribePeerState`（上游 `client.go:784-803` 语义）。非必需（重新订阅本身会重插兴趣），但保持服务端状态干净、与规格 §4.2 :173 一致。
- **理由**：改动面最小（纯 connector 编排层 + 一个查询缝），不碰协议 codec 与 WG 路径；恢复语义与上游客户端完全同构；幂等性现成——`attached` 表按 peer 键去重（`connector.rs:2035-2038`）、`open_conn` 有重复订阅守卫（`relay_client.rs:2085-2087`）、设备侧 `set_carrier` 是幂等 upsert（`wg_device.rs:696-716`）。
- **风险**：
  1. **对端长离线时的订阅空转**：`open_conn` 每 30s 超时 + 退订（`relay_client.rs:2110-2128`）后 pump 立即重试 → 每离线 peer ≈2 帧/30s 的 Subscribe/Unsubscribe 循环。量级无害，但建议给重开加小退避（复用 `lanes_open_failed` 计数，`connector.rs:1889`）。
  2. **对端快速振荡（flapping）**：每个 offline→online 周期触发一次 detach/attach，设备侧 `carrier-detach/carrier-attach` 并复位 `on_carrier`（`wg_device.rs:726-734`），握手路径小幅抖动。可加最小滞回（如 presence=false 后延迟数百 ms 再 detach，或只在「该 peer 有出向需求」时重开）。
  3. **服务端负载**：每 PWO 每 peer 一次 Subscribe，相对 Transport 流量可忽略；唯一的常驻成本是风险 1 的空转循环，已有 30s 超时上界。
- **落选方案**：(B)「首次 `PeerOffline` 拒收时重订阅」——更贴合「有流量才修」，但把恢复逻辑放上了数据路径，需另加限频防风暴；(C)「周期心跳式重订阅」——实现最笨、每周期 N 帧常驻开销，且需要给会话循环加新命令类型，侵入大于收益。均可作为 A 的后续替代。

---

## §6 可离线验证的测试设计（Q7，只设计不实现）

**前置发现（必须先补假服务器）**：现有 `relay_testserver.rs` **无法复现 A2 序列**，与真实服务端在关键处不一致：
- 其订阅语义是「在线目标立即应答且**不登记任何兴趣**」（`relay_testserver.rs:742-747`），而上游是无条件登记两类兴趣（`store.go:87`、`listener.go:38-46`）；
- 其兴趣在目标 Auth 时被整体取走（`relay_testserver.rs:632`），连接拆除时只把 PWO 发给「当前兴趣表」（:574-583）——由于目标在线期间的订阅根本不入表，**该表在拆除时刻恒为空，现有假服务器结构上发不出任何 `PeersWentOffline`**。
因此需要给 `relay_testserver` 增加一个「上游 0.78.1 忠实模式」：兴趣表按上游语义（在线兴趣一次性、离线兴趣常驻），并提供确定性控制面 `set_peer_online(peer)/set_peer_offline(peer)`（等效 `PeerCameOnline/PeerWentOffline` 广播 + 现有 `drop_connection/close_connection` 掐 TCP 的能力，`relay_testserver.rs:65-67` 已有连接控制）。

用例清单（VirtualClock，全部离线）：

- **T-P1（核心回归）**：① 客户端 A 连接 + auth；`open_conn(X)` 时 X 离线 → 静默挂起（规格 §4.1）；② `set X online` → 收 `PeersOnline`，`open_conn` 成功，lane 挂载；③ `set X offline` → 收 `PeersWentOffline`，断言 `send_to_peer(X)` = `PeerOffline`，断言（修复后）lane 被 detach/重开待命；④ `set X online` → 断言线上出现**新的** `SubscribePeerState[X]`（`frames_rx_by_type[MSG_SUBSCRIBE_PEER_STATE]` +1）→ 立即 `PeersOnline` → `send_to_peer(X)` 返回 Ok → 注入的 Transport 能被 `recv()` 收到；⑤ VirtualClock 断言全程无 35s 判死、无 30s `OpenConnTimeout`。
- **T-P2（订阅时在线路径，对齐上游）**：X 在线时订阅 → 立即应答**且**兴趣保持 → offline 收 PWO → online **能**收到（一次性消费前的）`PeersOnline`。此用例同时锁死新旧假服务器语义差异，防回归。
- **T-P3（双端回环，最接近 A2）**：两个 `RelayClient`（A=对端、B=设备）接同一假服务器；A `open_conn(B)`，双向 Transport 通；`drop_connection(B)` → A 收 PWO；B 以同一 peerID 重连 + auth → 断言 A 重订阅、A→B 帧重新可达（B 端 `recv()` 收到）。
- **T-P4（多 lane 隔离）**：X 的 PWO/重开不影响 peer Y 的 lane 与在途流量。
- **T-P5（幂等/振荡上界）**：X 快速 offline/online 连续切换 → 断言 Subscribe/Unsubscribe 计数有界、无 `SubscribeDuplicate` 死锁（`relay_client.rs:2085-2087`）、`lane_count()` 稳定；X 长离线 → 断言「30s 超时+退订+按设计节奏重试」的循环形态（观测 `observed_backoff`/帧计数）。

---

## §7 无法确证的点（如实）

1. **PWO/`PeersOnline` 的到达时刻无逐行日志**：我方客户端对这两个帧只写缓存与计数（`relay_client.rs:2028-2053`），不打 hilog。§4 中「PWO 到达≈臂间设备拆除窗口」是由 reject 计数差分（0→100，探针 1 条/s）推断的，非直接观测。
2. **对端在线兴趣被消费的具体时刻不可重构**：可确定的是 A2 窗口内设备已 auth 而 `PeersOnline` 未生效（拒收持续）；但「兴趣是在 A1 的某次事件投递中被消费，还是更早」无法从现有窗口切片判定。
3. **自建中继 `rels://home.alfadb.cn:28443` 的服务端版本/语义未核实**：§3.3 的一次性 `PeersOnline` 语义取自上游 0.78.1 源码（规格 §0 锚定的版本），未在该自建服务端上直接验证；若其行为偏离上游，§3.3 第 4 步的解释需修正——但**缺陷本身不依赖这一点**：无论服务端推不推第二次 `PeersOnline`，我方代码对 `presence=false` 都没有会话内主动恢复路径。
4. **对端 relay 单次重连（`reconnects:1,last_error_class:"io"`）的发生时刻**早于 arm1 窗口起点，无法从切片定位；它触发的「presence 清空+lane 重开」显然发生在 A1 之前（A1 零拒收），与结论不冲突。
5. **「构建差异是混杂变量」的判读**：A1/A2 真正的机制差异可能是「offline→online 循环次数」而非设备构建（同构建设备经历一次掉线也应复现）。本轮只读约束下未做「同构建 + 强制设备 offline→online」的对照复跑，此判读待实验证实。
6. **设备侧 `framesRx=21` 的逐帧构成**未解析（≈8 分钟服务端 25s HealthCheck + AuthResponse 的量级，`EXECUTION-RECORD` §5 与 hilog 尾段一致），未逐帧核对。
