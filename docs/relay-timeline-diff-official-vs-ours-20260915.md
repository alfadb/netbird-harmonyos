# 中继建链与数据面时序对照 — 官方客户端 vs 我方实现（2026-09-15）

- 性质：**只读对照分析**。未改代码、未碰设备、未联网；本文件是本次唯一新建文件。
- 材料标记约定：
  - `R:*` = 仓B 官方日志 `records/oracle-official-client-20260914/netbird-debug-netrpi.log`（0.77.1，netrpi 视角，16:19:56→16:23:38 +08）
  - `S:*` = 同目录其它文件（status-d-netrpi.txt / HANDOFF.md / logs-metadata.md / pcap）
  - `U:*` = 上游参考检出 `refs/netbird-791401060d2b/`（0.78.1）内路径
  - `O:*` = 我方 `client/core/src/` 内路径
- 已按任务要求排除 dstID 推导问题（`docs/peerid-crossvalidation-relay-20260915.md`，13/13 一致），本报告不重复。

---

## §1 结论与排序

1. **最可能根因（高置信）：官方对端从未建立"通向我方"的中继 lane——因为我们的 signal OFFER/ANSWER 不携带 `relayServerAddress`（Body 字段 8）。** 上游规定：收到对端 offer/answer 时才触发 `WorkerRelay.OnNewOffer`，且**必须** `answer.RelaySrvAddress != ""` 才会 `OpenConn`（`U:client/internal/peer/worker_relay.go:48-53,69,122-127`；触发点 `U:client/internal/peer/handshaker.go:128-130,150-152`）。我们的 Body 构造显式缺该字段（`O:signal.rs:472-492`，`..Default::default()`，全仓对 `relay_server_address` 只有读侧解析 `O:signal.rs:242-258`，无任何写入点）。于是我们的 WG 握手 initiation 虽经 relay 正确送达官方对端的 relay client，却落进它的**早期消息缓冲（每 peer ≤1 条、TTL 5s，注释原话"the first WireGuard handshake"）**，因对端永远不 `OpenConn` 我们而从未回放到它的 WG（`U:shared/relay/client/client.go:629-661,322-329`；`U:shared/relay/client/early_msg_buffer.go:11-20`）→ 对端 WG 永远看不到我们 → 无 response → `wgSessions=0`、`wgRxToTun=0`、`framesRx` 增量=纯 HealthCheck。
2. **次候选（同一机制的另一子情形）：C 轮时我们 signal link 可能根本未发出有效 offer/answer**（`O:peer_conn.rs:283-303` 未注册即拒发）。两种子情形在设备侧症状完全相同，区分法见 §6。
3. **帧大小算术**：上行增量 = **恰好 284 个 148B WG handshake initiation + 19 个 HealthCheck 回显帧**（284×148 = 42032 = ΔtransportBytes，逐字节吻合；19 ≈ 480s/25s 服务端 HC 节奏）。**"≈1.1 帧/秒匀速"= 多个 peer 的 WG 握手重发战役（每 peer 每 5s 一次，约 3~5 个 peer 并行），不是数据、不是保活。**
4. 保活方向/节奏候选**已排除**（reconnects=0 + framesRx 节奏吻合"服务端 25s 发、客户端回显"，我方有回显实现 `O:relay_client.rs:2017-2021`）。
5. relay 协议层（Auth→Subscribe→PeersOnline→Transport→HC）我方**逐步齐全**；缺的不是 relay 帧步骤，而是**官方用来触发"对端给我建 lane"的 signal 层字段**。

---

## §2 官方时间线（一条完整的中继对端生命周期，原文+file:line）

样本 peer：**admindingzuwei**（`x7LsrNP8NRtPNL93Lk0wwloF+XW9qsYTZdkzjEJp5Ck=`，NetBird IP 100.108.156.116，`S:status-d-netrpi.txt:130-144`：全窗口 Relayed、`Last WireGuard handshake: 1 minute, 42 seconds ago`、Transfer 7.0/4.0 KiB）。以下行号均出自 `R`。

### 2.1 relay 连接建立（Auth 之前的全部动作）

| 时刻 (+08) | 原文（关键部分） | file:line |
|---|---|---|
| 16:20:08.779 | `DEBG shared/relay/client/manager.go:161: starting relay client manager with [rels://home.alfadb.cn:28443] relay servers` | R:134 |
| 16:20:08.779 | `DEBG shared/relay/client/picker.go:48: pick server from list: [rels://…]` | R:135 |
| 16:20:08.779 | `INFO …client.go:245: create new relay connection: local peerID: F0GVt4o…, local peer hashedID: sha-27MDvCry…` | R:137 |
| 16:20:08.779 | `INFO …race_dialer.go:122: dialing Relay server via ws` / `…via quic`（QUIC+WS 竞速） | R:139-140 |
| 16:20:08.910/.918 | `successfully dialed via: ws` → `INFO …client.go:273: relay connection established` | R:142-144 |
| 16:20:12.396 | `DEBG client/internal/engine.go:2333: relay health check: healthy=true` | R:227 |

- **Auth 帧本身客户端不打印**（client→relay 方向帧）：`S:HANDOFF.md:55,90`；Auth 通过由中继侧 `WS client connected`/`peer connected` 佐证（`S:logs-metadata.md:67-73`）。Auth 帧格式与时机：连接后第一帧（`docs/relay-client-spec-20260914.md:114-123`，§3；上游 `U:shared/relay/client/client.go:273` 前的握手流程）。

### 2.2 对端级时序（OpenConn → WG 会话）

| 时刻 (+08) | 动作 | 原文（关键部分） | file:line |
|---|---|---|---|
| 16:20:12.730 | 引擎建 peer 连接 | `DEBG client/internal/engine.go:1881: creating peer connection x7LsrNP…` | R:333 |
| 16:20:12.732 | 重连守卫 | `INFO …guard/guard.go:62: starting guard for reconnection with MaxInterval: 43.336s` | R:338 |
| 16:20:12.732 | **发 signal OFFER（携带我方 relay 地址）** | `DEBG …handshaker.go:198: sending offer with serial: babf15a6af`（上游同一函数把 `RelaySrvAddress` 放进 offer，`U:handshaker.go:224-241`） | R:340 |
| 16:20:12.813 | **收到对端早期 Transport（对方 WG initiation），本地尚无 lane → 缓存** | `DEBG …client.go:662: buffered early transport message for peer: sha-z89n+zdU4…`（sha-z89n=admindingzuwei，5 个 peer 同类事件 R:366/367/386/404/476） | R:404 |
| 16:20:13.890 | 收到 signal ANSWER | `INFO …conn.go:357: OnRemoteAnswer, priority: None, status ICE: Disconnected, status relay: Disconnected` → `INFO …handshaker.go:136: received answer, … remote WireGuard listen port 51820` | R:869-870 |
| 16:20:13.890 | **OpenConn（由"收到 answer"触发）** | `DEBG shared/relay/client/manager.go:201: open peer connection via permanent server: x7LsrNP…` → `INFO …client.go:306: prepare the relayed connection, waiting for remote peer: sha-z89n…`（=SubscribePeerState+等 PeersOnline，上游 `U:client.go:299-355`） | R:873-874 |
| 16:20:13.898 | 服务端确认在线 | `DEBG …peer_subscription.go:116: peer sha-z89n… is now online` → `INFO …client.go:344: remote peer is available: sha-z89n…` | R:882-883 |
| 16:20:13.899 | lane 建成 | `DEBG …worker_relay.go:90: peer conn opened via Relay: rels://home.alfadb.cn:28443`（上游 OpenConn 时**先回放早期缓冲消息** `U:client.go:322-329`） | R:884 |
| 16:20:13.899 | **WG 衔接** | `DEBG …conn.go:571: Relay connection has been established, setup the WireGuard` → `…conn.go:879: setup proxied WireGuard connection` → `INFO …conn.go:582: created new wgProxy for relay connection: 127.0.0.1:12`（上游顺序：newProxy→ConfigureWGEndpoint→`injectPendingFirstPacket`→"start to communicate"，`U:conn.go:559-620,159-185`） | R:887-891 |
| 16:20:13.899-900 | WG watcher + responder 配置 | `…wg_watcher.go:55/80: enable/start WireGuard watcher` → `…endpoint.go:46: configure up WireGuard as responder` → `…endpoint.go:87: configure up WireGuard and wait for handshake` → `…iface.go:160: updating interface wt0 peer x7LsrNP…, endpoint <nil>, allowedIPs [100.108.156.116/32]`（responder 先不下 endpoint，5s 后延迟落配，`U:endpoint.go:40-47,83-119`） | R:892-896 |
| **16:20:13.9169** | **WG 握手完成** | `INFO …wg_watcher.go:101: first wg handshake detected within: 0.02sec, (2026-09-14 16:20:13.916918635 +0800 CST)`（16:20:43.902 打印，检测回溯）——**lane 建成后 ~18ms 内握手完成** | R:1350 |
| 16:20:14.002 | 数据面就绪 | `INFO …conn.go:619: start to communicate with peer via relay` | R:926 |
| 16:20:18.901 | responder 延迟落配 endpoint | `DEBG …iface.go:160: updating interface wt0 peer x7LsrNP…, endpoint 127.0.0.1:12, allowedIPs [100.108.156.116/32]` | R:996 |
| 16:20:25.9→16:20:46.9 | ICE 并行尝试失败（保持 Relayed） | `…worker_ice.go:513: ICE ConnectionState has changed to Failed/Closed` | R:1025-1027, 1355-1357 |
| 16:20:34.8→16:23:17 | **offer/answer 周期重发；每次收到都重触发 OpenConn（幂等复用）** | `OnRemoteAnswer/OnRemoteOffer …` → `manager.go:201: open peer connection…` → `worker_relay.go:72: handled offer by reusing existing relay connection`（x7LsrNP 例：R:1136-1143、1414-1423、1646-1649、1856-1859；间隔 ~14-36s） | 多处 |

**保活**：线上 HealthCheck 帧客户端不打印；节奏为规格锚定的"服务端每 25s 发、客户端原样回显、35s 无入站判死"（`docs/relay-client-spec-20260914.md:177-190`，§5；上游 `U:shared/relay/healthcheck/sender.go:13`、`U:shared/relay/client/client.go:618-627`）。观测节奏吻合见 §5 算术。

### 2.3 反面样本：官方对**我方设备 key** 的行为（同一日志窗口）

我方设备（netbird-ohos，`TrBfURPG8gHinPcgt2J9j1lsfe1EHwXI98U8WiG4iF4=`，`S:status-d-netrpi.txt:18-29`：`Status: Connecting`、`Relay server address:` 空、`Last WireGuard handshake: -`）：

- R:347 `engine.go:1881: creating peer connection TrBfURPG…`；R:351 guard 启动（MaxInterval 36.507s）；R:354 起 **10 次** `handshaker.go:198: sending offer with serial: 57edf7251d`（16:20:12.737 → 16:23:16.918，间隔 ~2.8→36.6s 退避：R:975/1001/1124/1501/1832/2125/2345/2505）。
- **但全窗口对 TrBfURPG：0 条 `manager.go:201 open peer connection`、0 条 `client.go:306`、0 条 `peer_subscription.go:116`**——官方从未为我方建中继 lane（当时我方离线，官方收不到我方任何 offer/answer；对比 §2.2 每一个 peer 都是"收到 answer/offer 后立刻 open"）。

---

## §3 我方时序（读码，file:line）

| 步 | 动作 | 位置 |
|---|---|---|
| 1 | relay 拨号：URL 列表 first-wave 并发 → WS 升级（30s 预算） | `O:relay_client.rs:1855-1888`（run_session）、`O:relay_client.rs:1757`（dial_first_win） |
| 2 | Auth：发 Auth 帧 → 8s 内等 AuthResponse；失败=静默断连同型 | `O:relay_client.rs:1890-1963` |
| 3 | **Ready**（state 机 Disconnected→Dialing→Handshaking→Authenticating→Ready） | `O:relay_client.rs:1965-1972,1126-1142` |
| 4 | 装载 carrier 编排：attach_client + 启动 pump（500ms 巡检） | `O:connector.rs:1583-1601,2007-2014,2147-2181` |
| 5 | **对网络图全部 peer 逐个 open_conn**（SubscribePeerState[1] → 等 PeersOnline，30s 超时→Unsubscribe） | `O:connector.rs:2065-2077`（ensure_lanes）、`O:connector.rs:2018-2062`（ensure_lane）、`O:relay_client.rs:1467-1488,2084-2095,2110-2130` |
| 6 | open_conn 成功 → `RelayWgCarrier` 挂到 WG seam（set_carrier） | `O:connector.rs:2027-2052`、`O:wg_device.rs:701-719` |
| 7 | WG 握手战役：`has_path`（endpoint 或 carrier）且无会话 → 每 5s `initiate_handshake`（148B initiation，90s 战役窗口后重置即再启） | `O:wg_device.rs:921-956`（tick/campaign）、`O:wg_device.rs:1138-1152,174-181,749-752` |
| 8 | 出向 bearer 选择 dispatch：无 ICE endpoint → carrier → `send_to_peer` → Transport 帧（38B 头+payload，满则丢） | `O:wg_device.rs:1046-1087`、`O:relay_client.rs:1433-1460,2196-2217` |
| 9 | 入向：reader_loop → HealthCheck **原样回显**；Transport → 入站队列 | `O:relay_client.rs:2153-2193,2249-2263,2017-2027` |
| 10 | 入站 Transport → 反查 peer key → `handle_carrier` → boringtun 解密 → TUN/回包；握手完成观测（session_established） | `O:connector.rs:2095-2131`、`O:wg_device.rs:818-862,1001-1037` |
| 11 | 会话建立后：25s persistent keepalive、会话过期 540s | `O:wg_device.rs:957-966,174-175` |
| 12 | signal 层（与本时序的耦合点）：OFFER/ANSWER body 只含 type/payload("ufrag:pwd")/wgListenPort/version，**`relayServerAddress`、`sessionId` 恒缺省** | `O:signal.rs:472-492`（build_message，`..Default::default()`）、`O:peer_conn.rs:190-198,283-303`（seam 只传 ufrag:pwd；未注册拒发）、`O:peer_conn.rs:764-852`（handle_signal 应答，同样不带 relay 字段）、读侧解析但编排层不使用：`O:signal.rs:242-258`、`O:connector.rs:3080-3098` |

---

## §4 逐步对比表

| # | 官方步骤（§2） | 我方是否做 | 我方位置 | 差异 | 缺失是否可能导致"帧在发但对端不回" |
|---|---|---|---|---|---|
| 1 | relay manager 启动、picker、WS(+QUIC 竞速)拨号 | ✅（仅 WS，无 QUIC 竞速） | O:relay_client.rs:1855-1888 | 传输面等价；QUIC 竞速缺失与本症状无关（服务端 WS 可用） | 否 |
| 2 | 连接后第一帧 Auth → AuthResponse | ✅ | O:relay_client.rs:1890-1963 | 等价（8s 超时、静默拒同型） | 否（设备侧 state=ready、tokenValid 佐证已通） |
| 3 | **每个 signal OFFER/ANSWER 都携带本端 relayServerAddress**（`U:handshaker.go:224-241`） | ❌ **不做** | O:signal.rs:472-492（字段恒缺）；O:peer_conn.rs:190-198（seam 不传） | **内容缺失**：官方对端据此判定"我方支持 relay"（`U:worker_relay.go:122-127`） | **是（根因）** |
| 4 | **收到对端 offer/answer 才 OpenConn 建 lane**；重发周期性复用 | ◐ 触发模型不同：我方对全图 peer 主动 eager 建 lane | O:connector.rs:2065-2077 vs U:worker_relay.go:48-96 | 我方能建自己的 lane（PeersOnline 已验证），但**没有任何机制让对端为我建 lane** | **是（根因的另一面）** |
| 5 | lane 建成即回放早期缓冲 Transport（每 peer ≤1 条、TTL 5s） | ❌ 无此缓冲，但我方入站 Transport **不依赖 lane** 直达 WG 设备 | U:client.go:322-329,629-661 vs O:connector.rs:2109-2131 | 我方语义是上游的超集，不是缺口 | 否（对端没有 lane 才是问题） |
| 6 | lane→wgProxy→WG endpoint 落配（responder/initiator 分工、延迟落配） | ✅ 等价形态（carrier→dispatch→boringtun） | O:wg_device.rs:696-752,1046-1087 | 无 kernel/userspace bind 之分，功能等价 | 否 |
| 7 | 首个 WG handshake 由"首个待发包注入"或对端主动发起触发（`U:conn.go:159-185,604`） | ◐ 我方每 peer 每 5s 主动重发（含 90s 战役窗口后自动重启） | O:wg_device.rs:936-956,1138-1152 | 我方上行呈"initiation 风暴"（§5）；官方仅对有流量/有 signal 会话的 peer 握手 | 不直接致败，但放大了 #3/#4 的暴露面 |
| 8 | 服务端 25s HealthCheck、客户端回显 | ✅ | O:relay_client.rs:2017-2021,2249-2263 | 等价；reconnects=0 证明回显被服务端接受 | 否 |
| 9 | 握手 response 到达 → 会话建立 → 数据面 | ✅（代码在，等米下锅） | O:wg_device.rs:835-862 | 未被触发的唯一原因是 #3/#4 | — |
| 10 | WG 握手完成观测/事件（wg_watcher 3m 重置） | ✅（session_established + 过期 540s） | O:wg_device.rs:845-861,926-933 | 等价 | 否 |

---

## §5 帧大小算术与解释

### 5.1 已知尺寸（WireGuard 标准/boringtun）

| WG 报文 | 字节数 | 依据 |
|---|---|---|
| handshake initiation | **148** | WG 协议定长（`O:wg_device.rs` 走 boringtun `force_handshake`，标准 148B initiation；wireguard.pdf §5.4） |
| handshake response | 92 | 同上 |
| cookie reply | 64 | 同上 |
| transport data(keepalive，空 payload) | 32 | 同上 |
| relay Transport 帧开销 | +38（2B 头+36B dstID） | `docs/relay-client-spec-20260914.md:99,167`（§2.5/§4.2） |

### 5.2 设备侧两轮读数分解（C 类观测：framesTx 7→310，transportBytes 444→42476）

- Δframes = 303，Δbytes = 42032。
- **分解：284 个 initiation（148B）+ 19 个 HealthCheck 回显（2B，不计入 transportBytes）**
  - 284 × 148 = **42032**（与 ΔtransportBytes 逐字节相等，无残差）
  - 284 + 19 = **303**（与 Δframes 相等）
  - 19 个 HC ≈ 480s ÷ 25s = 19.2 —— 与服务端 25s HC 节奏吻合（规格 §5）
- 首轮快照自洽：framesTx=7 = 3 initiation（3×148=**444**=首轮 bytes）+ 4 HC；framesRx 首轮=4 = 已收 4 个 HC。**两个计数器相互印证。**
- 节奏：284 次 ÷ 8min ≈ 0.59 次/s；我方 `DEFAULT_HS_RETRY_MS = 5s`（`O:wg_device.rs:175-176`）⇒ **≈3 个 peer 的握手战役并行**（3/5s=0.6/s）；若按任务口径"≈1.1 帧/秒"（窗口 ≈275s）⇒ ≈5 个 peer。两读法一致指向：**ensure_lanes 对全图开 lane + 每 lane 一场 WG 握手战役，全部无应答**（另注：我方 90s 战役窗口到期后清零即重启，实际是持续重发，`O:wg_device.rs:936-956` 的 `Some(_)|None` 分支）。

### 5.3 只支持哪种解释

- **"保活/心跳重发"**：WG keepalive=32B ⇒ 284×32≈9.1KB ≪ 42KB；relay HC 我方只回显不主动发。**排除**。
- **"真在发数据"**：数据帧（ping≈84B/1228B inner）会混入非 148B 尺寸且 tun 必有 RX；实测 vpn-tun RX 0B、TX 576B 零增长，且 42032 恰为纯 148B 倍数（窗口内 0 字节 tun 数据经载体）。**排除**。
- **"握手 initiation 反复重发、无人应答"**：唯一与两个计数器、节奏、tun 零流量同时吻合的解释。**成立**。

### 5.4 pcap 交叉验证（官方健康会话的帧长/节奏基线；有界抽样，未做大规模解析）

样本 `S:oracle-relayed2-relayhost-relay80.pcap`（Caddy 172.18.0.3 → relay 容器 172.18.0.2:80 明文 WS 段，16:11:13→16:12:25，即 netrpi↔admindingzuwei 窗口，`-s 128` 仅头 128B）。Caddy→relay 方向（含全部客户端流的**掩码帧**，只能取长度不可读类型字节）TCP 载荷长度分布（原始行含 In/Out 重复，去重后）：

| 长度 | 去重计数 | 解释（WS 头 2/4B+4B 掩码后 = relay 帧 = 38+WG 包） |
|---|---|---|
| 1310B | 90 | 2+2+4+1302 ⇒ WG 数据 ≈1264B ⇒ ≈ping -s 1200（45 请求+45 应答=90，与 `S:logs-metadata.md:64` 的 `ping -s 1200 -c 45` 精确吻合） |
| 8B | 39 | WS 层 ping/pong 控制帧 |
| **194B** | **3** | 2+2+4+**186** ⇒ 38+**148 = WG handshake initiation**（72s 窗口 3 个 ≈ 双向 rekey ~120s 节奏） |
| 221B | 2 | WG 数据 ≈175B（小包） |

**结论**：健康官方会话的中继上行是**流量形状的混合分布**（大数据包 + WS 控制 + 极少量 194B initiation + 零星小包），与我方设备的"148B 单一形态、0.6~1.1/s、零回包"形成鲜明对照——再次支持 §5.3 的唯一解释。注意限制：该 pcap 窗口与 debug 日志窗口（16:20-16:23）不重叠，不能逐帧对齐日志；同过滤器混入 14 条客户端 TCP 流（含周期性健康探测），故只用于"混合 vs 单一"的形态结论。

---

## §6 候选排序 + 证伪方案（只出方案；C 类授权已用毕，任何设备轮需新授权）

### 候选 1（最高优先）：官方对端没有通向我方的 lane——我们的 signal 消息不带 `relayServerAddress`

- **支持证据**：
  - 上游触发链：收到 offer/answer → `relayListener.Notify`（`U:handshaker.go:128-130,150-152`）→ `WorkerRelay.OnNewOffer` → `isRelaySupported` 要求 `answer.RelaySrvAddress != ""`（`U:worker_relay.go:122-127`）→ 才 `OpenConn`（`U:worker_relay.go:69`）；不支持时日志原话 `Relay is not supported by remote peer`（`U:worker_relay.go:50`）。
  - 我方缺失：`O:signal.rs:472-492`（Body 只设 type/payload/wgListenPort/version，注释自认"minus the fields this client does not produce yet"）；全仓 `grep relay_svr|RelaySrv|relay_srv` = 0 写入点；读侧解析了却不用（`O:signal.rs:242-258`、`O:connector.rs:3080-3098`）。
  - 丢弃机制：官方 relay client 收到无 lane 的 Transport → 早期缓冲 ≤1 条/peer、TTL 5s（`U:client.go:629-661`；`U:early_msg_buffer.go:11-12,18-20` 注释明言就是为"远端在 OpenConn 前发来的首个 WG 握手"设计）→ 只有 `OpenConn` 才回放（`U:client.go:322-329`）。对端永远不 OpenConn 我们 ⇒ 我方 5s 一发的 initiation 全部黑洞。
  - 服务端行为佐证无屏蔽：服务端按 dstID 转发、不要求接收方订阅、离线才静默丢（`U:relay/server/peer.go:209-235`）——所以我方帧确实送达了官方对端的 relay client。
  - 官方日志反例组：§2.3——官方对我方 key 连发 10 次 offer、0 次 OpenConn；§2.2——所有其它 peer 都是"收到我方(对端侧)消息即 OpenConn"。
  - 症状全解释：framesTx 涨（我方战役在发）、framesRx Δ19=纯 HC（对端从不应答）、wgSessions=0/wgRxToTun=0（无 response 可解密）、reconnects=0（HC 回显正常，relay 会话本身健康）。
- **如何证伪（按成本升序）**：
  1. **运维侧零设备轮**：中继宿主 `docker logs netbird-relay --since <C 轮窗口>`（INFO 级有 `peer connected from`/`peer exited gracefully`+peer_id，`S:logs-metadata.md:67-73`）：确认 C 轮窗口内官方对端 peer（sha-z89n…/sha-LICq…）与我方 sha-RGMnYC… 同时在线——排除候选 3。
  2. **下一授权轮最小取证（单点、5 分钟）**：官方对端 daemon 临时提级 debug，我方上线后 grep 两行定案：`grep -E "TrBfURPG|Relay is not supported by remote peer" /var/log/netbird/client.log`。若命中 `Relay is not supported by remote peer`（随我们的 offer/answer 到达打印）⇒ 候选 1 实锤；若对我们 key 完全静默 ⇒ 转候选 2。同轮在对端 `netbird status -d` 看我方行 `Status`（预期停在 Connecting）与 `Relay server address`（预期空）。
  3. **修复后回归判据**（同轮顺带）：我方 Body 补字段 8/11（可先不补 sessionId）→ 官方日志应出现对 `TrBfURPG…` 的 `manager.go:201 open peer connection` + `peer_subscription.go:116 … is now online`；我方 framesRx 应在 ≤5s 内出现 92B response（Δbytes/Δframes≈92 或 session_established 事件）。

### 候选 2：C 轮我方 signal link 未注册/未发任何有效 offer（候选 1 的前置子情形，可独立致败）

- **支持证据**：`O:peer_conn.rs:283-303`——`RealSignalExchange::send` 未注册时拒发（帧留 outbox）；若壳侧从未喂 signal socket（`O:connector.rs` 模块注记"壳不喂则拨号 fail-closed 重试，signal_ready 保持 false"），官方收不到我方任何消息 → 无 lane → 同样黑洞。仓B 09-14 窗口官方对我方"只发 offer 无回音"与此兼容（当时我方确实离线），但 09-15 C 轮状态无材料可查。
- **证伪**：与候选 1 的第 2 步同一轮判别（官方日志对 TrBfURPG 是否有"received offer … relay server: ''"字样：0.78.1 的 received offer 行包含 relay server 字段，`U:handshaker.go:119,141`）；运维侧可查官方对端 `netbird status -d` 的 peers 列表里我方状态是否长期 `Connecting`。

### 候选 3：官方对端在 C 轮不在线（relay 掉线窗口）→ 服务端静默丢

- **支持证据**：弱。我方 lanes 能 attach 成功（framesTx 持续流动 ⇒ open_conn 曾收到这些 peer 的 PeersOnline，`O:relay_client.rs:1292-1294`（presence 仅 PeersWentOffline 置假）+ `O:connector.rs:2018-2062`）说明订阅时点对端在线；但"订阅时在线、随后掉线"无法排除。
- **证伪**：候选 1 第 1 步的中继宿主日志时间线即可排除/证实，零设备轮。

### 候选 4：我方 WG initiation 字节无效（boringtun 编码不兼容）

- **支持证据**：无实证（148B 形态、boringtun 标准实现、我方与官方 0.77/0.78 内核 WG 的兼容性在 N11/N6 增量里另有验证）；列为次级仅因它同样产生"不回包"。
- **证伪**：若候选 1 修复后对端仍不应答，再抓中继宿主 :80 明文段（新授权）比对 initiation 前 4 字节（type=0x01、reserved=0）与 148B 定长；或改用官方对端 debug（kernel WG 会静默丢畸形包，需 pcap 级证据）。

### 已排除候选

- **保活方向/节奏**：我方为纯回显（`O:relay_client.rs:2017-2021`），且 reconnects=0 + framesRx Δ19≈480s/25s 证明节奏吻合（若我方不回显，服务端 45s 内判死，`U:relay/server/peer.go:186-207`，reconnects 必涨）。
- **"PeersOnline 之后还有额外订阅/握手步骤"**：无依据。上游 OpenConn=Subscribe→PeersOnline→回放早期消息→返回 conn，就此为止（`U:client.go:299-355`），无对端级确认帧；我方实现等价。
- **dstID 推导**：已由 `docs/peerid-crossvalidation-relay-20260915.md` 排除（13/13）。

---

## §7 无法确证的点（如实）

1. **C 轮（09-15）官方对端的实际状态与日志**：仓B 只有 09-14 采集；官方对端当时是否在线、是否打印过 "Relay is not supported by remote peer"、是否收到我方 initiation，均无本地材料——候选 1/2 的最终裁断依赖 §6 的取证轮。
2. **我方 C 轮 signal link 是否注册、offer/answer 是否实际发出**：设备侧运行视图（signal_ready、refused_sends、outbox 深度）不在本次只读材料内。
3. **framesTx 7→310 的口径**（全部类型合计 vs 仅 Transport）：本报告按"合计"解读（284+19 双计数器精确自洽）；若实为仅 Transport 帧计数，则分解不成立，但均值 ≈137-139B 仍指向 initiation 主导、排除 keepalive 主导（32B）。待确认。
4. **官方 capture 与 debug 日志窗口不重叠**（pcap=15:41/16:11 两段，日志=16:20-16:23）：§5.4 的帧长基线与 §2 的日志时序无法逐帧对齐，只作形态对照。
5. **掩码方向的帧类型字节不可读**（客户端→relay WS 帧强制掩码）：§5.4 全部结论仅基于长度+节奏，未读类型字节。
6. **我方 offer/answer 缺 `sessionId`（Body 字段 10）对官方 handshaker/ICE 的具体影响**：上游对空 sessionId 的容错行为未在本次上游阅读中验证（relay lane 判定只看 RelaySrvAddress，不受影响，`U:worker_relay.go:122-127`）；ICE 层影响待确认。
7. **relay server 版本差异**（生产 relay 0.76.3 vs 参考 server 代码 0.78.1）：转发/订阅语义按参考检出引用，0.76.3 若有行为差异未验证。
