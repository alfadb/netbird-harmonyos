# NetBird Relay 客户端线格式规格（可直接实现版）

- 提取来源（唯一权威依据）：`/home/worker/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`，NetBird 0.78.1，commit `791401060d2b95e5f51e3439c0649729132f571e`。
- 提取日期：2026-09-14。本文所有 `file:line` 均相对该上游根目录。
- 适用范围：本项目 `client/core/`（Rust）实现 relay 客户端，**以 WebSocket（`rel`/`rels`）传输为主**；QUIC 传输仅作背景说明（官方客户端默认会与 WS 竞争，见 §1.6）。
- 令牌来源：管理层 SyncResponse 的 `RelayConfig{urls, token_payload, token_signature}`，本项目已解析为 `client/core/src/network_map.rs:211` 的 `RelayServers`。
- 约定：本文「帧」= 一条完整的 relay 协议消息；所有偏移量从 0 开始。

---

## 1. 拨号与 WebSocket 握手

### 1.1 URL 映射
| 输入 scheme | 映射后 scheme | 路径 | 依据 |
|---|---|---|---|
| `rel://host[:port]` | `ws://host[:port]` | `/relay` | `shared/relay/client/dialer/ws/ws.go:61-79` |
| `rels://host[:port]` | `wss://host[:port]` | `/relay` | 同上 |

- 路径常量 `WebSocketURLPath = "/relay"`：`shared/relay/constants.go:5`；客户端 `parsed.Path = relay.WebSocketURLPath`（`ws.go:77`），服务端也只在该路径上 accept（`relay/server/listener/ws/listener.go:48-49`）。
- 端口：URL 显式写了端口则保留（`ws.go:60` 注释、`ws.go:74-78`）；**未写端口时上游没有 rel/rels 专用默认端口定义**，落到 ws/wss 协议默认 80/443（`ws.go:61-79` 未见改写 host/port；QUIC 拨号器则显式补 443/80，见 `shared/relay/client/dialer/quic/quic.go:82-109`）。托管网络下发的 URL 一般自带端口。

### 1.2 HTTP 头
- 客户端拨号用 `coder/websocket.Dial`，`DialOptions` 里**只**注入自定义 `HTTPClient`（`shared/relay/client/dialer/ws/dialopts_generic.go:11-15`）。即：不发 `Authorization`，不请求任何 WebSocket 子协议（`Subprotocols` 未设置），认证全部走带内 Auth 帧（§3）。
- 其余头为 RFC 6455 标准集（`Upgrade: websocket`、`Connection: Upgrade`、`Sec-WebSocket-Key`、`Sec-WebSocket-Version: 13`），由库生成（`ws.go:37`）。
- 服务端（假服务器要对齐的行为）：`websocket.Accept` 且 `OriginPatterns: ["*"]`，即**不校验 Origin**（`relay/server/listener/ws/listener.go:80-90`）。

### 1.3 握手响应判定
- 库标准行为：仅接受 `101 Switching Protocols` + 正确 `Sec-WebSocket-Accept`；非 101 视为拨号失败（错误体含响应内容），客户端随后关闭 resp body（`ws.go:37-52`）。
- 错误路径：`context.Canceled` 原样返回；`*net.OpError` 解包后返回（`ws.go:38-49`）。

### 1.4 TLS 细节
- 证书池：系统池，失败回退内置根证书（`ws.go:87-91`）。
- `ServerName`（SNI）显式设为调用方传入的 `serverName`（`ws.go:101-104`）；正常按 FQDN 拨号时即 URL host。当客户端持有服务器 IP 做「IP 直拨」时，URL host 被替换成 IP，但 TLS ServerName 仍用原 FQDN（`shared/relay/client/client.go:229-231, 453-468, 474-500`）。→ 我们实现 `rels` 时必须支持自定义 SNI。

### 1.5 读写超时
| 项 | 值 | 依据 |
|---|---|---|
| 拨号总超时 `DefaultConnectionTimeout` | 30s | `shared/relay/client/dialer/race_dialer.go:14`、`shared/relay/client/picker.go:17` |
| Auth 响应读超时 `serverResponseTimeout` | 8s | `shared/relay/client/client.go:27`（`readWithTimeout` 实现 `client.go:879-900`） |
| 服务端写超时 `writeTimeout` | 10s | `relay/server/listener/ws/conn.go:16, 55-61` |
| 服务端 HTTP 读头超时 | 5s | `relay/server/listener/ws/listener.go:52` |
| 服务端握手（pre-auth）总超时 `handshakeTimeout` | 10s | `relay/server/handshake.go:15`、`relay/server/relay.go:128-129` |
| 已建连后的 per-read/per-write deadline | **不可用**：`SetReadDeadline/SetWriteDeadline/SetDeadline` 均返回 "not implemented" | `shared/relay/client/dialer/ws/conn.go:67-77` |

→ 结论：**已建立连接上没有 IO 级超时**，活性判定完全依赖 §5 的 healthcheck；实现方需要用自己的超时手段（如 tokio `timeout` 包住读）或直接照搬 healthcheck 语义。

### 1.6 传输选择（背景）
- 环境变量 `NB_RELAY_TRANSPORT`：`auto`（默认，QUIC 与 WS 并发竞速）/`quic`/`ws`/`prefer-quic`/`prefer-ws`（`shared/relay/client/transport.go:14-59`；拨号器顺序 `shared/relay/client/dialers_generic.go:30-50`）。
- QUIC 传输：去 scheme 后默认补端口 443/80，QUIC datagram 承载帧（每 datagram 一帧），ALPN=`nb-quic`，KeepAlive 30s，MaxIdleTimeout 4min，InitialPacketSize 1232（`quic/quic.go:30-79`、`shared/relay/tls/alpn.go:3`、`shared/relay/constants.go:7-10`）。
- 对我们的影响：默认 `auto` 时官方客户端会同时向同一 host 发起 QUIC(UDP) 与 WS(TCP)，**只支持 WS 的服务器在 auto 模式下仍可被连上**（QUIC 侧失败、WS 侧成功）；netbird 官方 relay 同时监听两者。

---

## 2. 帧格式（逐字节）

### 2.1 协议头
每帧以 2 字节头开始，**无长度前缀、无多字节数值字段（无字节序问题）**：

| 偏移 | 长度 | 字段 | 值 |
|---|---|---|---|
| 0 | 1 | version | `0x01`（`CurrentProtocolVersion = 1`） |
| 1 | 1 | msg type | 见下表 |

依据：`shared/relay/messages/message.go:14, 35-37`；版本校验 `message.go:93-102`。

### 2.2 类型表（`message.go:16-32`）
| 值 | 类型 | 方向 | 说明 |
|---|---|---|---|
| 0 | Unknown | - | 非法 |
| 1 | Hello | - | 已删除的遗留类型，仅保留线值，服务端拒绝 |
| 2 | HelloResponse | - | 同上 |
| 3 | Transport | 双向 | 数据转发 |
| 4 | Close | 双向 | 优雅关闭整条 relay 连接 |
| 5 | HealthCheck | 双向 | 保活 |
| 6 | Auth | C→S | 握手认证（唯一） |
| 7 | AuthResponse | S→C | 握手成功（唯一） |
| 8 | SubscribePeerState | C→S | 订阅对端在线状态 |
| 9 | UnsubscribePeerState | C→S | 退订 |
| 10 | PeersOnline | S→C | 对端在线通知 |
| 11 | PeersWentOffline | S→C | 对端离线通知 |

客户端可发送的类型集合：6/3/4/5/8/9（`message.go:105-123`）；客户端可接收：7/3/4/5/10/11（`message.go:126-144`）。未知类型 → 丢弃该帧并继续（客户端 `client.go:574-579`）；服务端收到未知类型则断连（`relay/server/peer.go:104-108` 直接 return）。

### 2.3 magic 常量
`magicHeader = {0x21, 0x12, 0xA4, 0x42}`（`message.go:56`）。**只出现在 Auth 帧**。

### 2.4 尺寸常量（`message.go:10-12`）
| 常量 | 值 | 用途 |
|---|---|---|
| `MaxHandshakeSize` | 212 | Auth 帧总长上限 |
| `MaxHandshakeRespSize` | 8192 | AuthResponse 总长上限 |
| `MaxMessageSize` | 8820 | 运行期帧长上限（服务端读缓冲 `relay/server/peer.go:20`；客户端读缓冲 `client.go:26`；peer-state 分块依据 `peer_state.go:45`） |

### 2.5 各帧字节布局（proto header 之外的部分）
| 帧 | 布局 | 依据 |
|---|---|---|
| Auth | `[magic 4B][peerID 36B][token 二进制 N B]` | `message.go:39-49, 151-163` |
| AuthResponse | `[instanceURL ASCII, ≥1B]` | `message.go:184-206` |
| Transport | `[dstID 36B][payload]` | `message.go:47-49, 221-229` |
| Close | 无 body（仅 2B 头） | `message.go:211-216` |
| HealthCheck | 无 body（仅 2B 头，值为 `{0x01, 0x05}`） | `message.go:58, 269-271` |
| Subscribe/Unsubscribe/PeersOnline/PeersWentOffline | `[N × 36B peerID]`，N≥1，按 `(8820-2)/36 = 244` 个/帧分块 | `shared/relay/messages/peer_state.go:40-92` |

### 2.6 分片/粘包与帧边界判定
- **WS 传输：一条 relay 帧 = 一条完整 binary WebSocket 消息**。客户端读：`Reader(ctx)` 取一条消息后只做一次 `Read(b)`（`dialer/ws/conn.go:41-53`），写：每帧一次 `Write(..., MessageBinary, b)`（`dialer/ws/conn.go:55-57`）；服务端同样一消息一帧（`listener/ws/conn.go:34-61`）。非 binary 消息 → 报错（两侧都有此检查）。
- 因此**实现按「读完整条 WS 消息 = 一帧」判定帧边界，禁止把多帧并入一条 WS 消息或把一帧拆多条**。帧间无粘连，无需长度前缀或转义。
- 帧长校验：读到的帧 > 各自上限（§2.4）时的显式行为上游未见定义（客户端缓冲即 8820，超出部分单次 `Read` 读不到；QUIC 侧超路径预算映射为 `ErrDatagramTooLarge` 并触发传输回退，`client.go:709-737`、`dialer/quic/conn.go:101-107`）→ 我们实现时建议：WS 消息 > 8820 即记日志丢弃该帧并保持连接，勿 panic。
- Auth 帧上限计算：`2+4+36+1+32+len(payload) ≤ 212` ⇒ payload（ASCII 时间戳）≤ 137 字节，实际 ~11 字节。

---

## 3. Auth 帧 / peerID 推导 / token 二进制形态

### 3.1 Auth 帧完整布局（客户端→服务端，连接建立后第一帧）
| 偏移 | 长度 | 内容 |
|---|---|---|
| 0 | 1 | `0x01` version |
| 1 | 1 | `0x06` type=Auth |
| 2 | 4 | magic `21 12 A4 42` |
| 6 | 36 | peerID（原始 36 字节，见 §3.2） |
| 42 | N | token 二进制（见 §3.3） |

依据：`message.go:151-163`（MarshalAuthMsg）、偏移常量 `message.go:39-49`。服务端读取 ≤212 字节、校验 version/type/magic 后校验 token（`relay/server/handshake.go:46-96`）。

### 3.2 peerID 的精确推导（36 字节）
```
peerKeyString   = 客户端 WireGuard 私钥对应的公钥，base64 字符串形式
                  （connect.go:425：NewManager(..., myPrivateKey.PublicKey().String(), ...)）
hash            = SHA256( UTF8(peerKeyString) )           // 32 字节
peerID          = "sha-" (4 字节 ASCII) || hash (32 字节)  // 共 36 字节
```
依据：`shared/relay/messages/id.go:9-18`（`prefixLength=4`、`peerIDSize=4+sha256.Size=36`、`prefix = []byte("sha-")`）、`id.go:25-31`（HashID）、`shared/relay/client/client.go:233`（NewClient 内 `messages.HashID(peerID)`）。
- **线上是原始 36 字节，与 base64 无关**。人类可读形式 `peerID.String()` = 前 4 原始字节（即 `"sha-"`）+ `base64.StdEncoding(带 padding)`(后 32 字节)（`id.go:20-22`），仅用于日志/服务端比对，不上线。
- 同一推导用于 `OpenConn` 的目的地址：`client.go:301-302` `messages.HashID(dstPeerID)`。

### 3.3 token 二进制形态（Auth 帧 body）
| 偏移 | 长度 | 内容 | 值 |
|---|---|---|---|
| 0 | 1 | 算法字节 | `0x01`（`AuthAlgoHMACSHA256=1`，枚举从 0 起） |
| 1 | 32 | 签名（原始 32 字节 HMAC-SHA256，无 base64） | HMAC_SHA256(key, payload) |
| 33 | M | payload（ASCII 十进制字符串） | Unix 过期时间戳（秒），如 `"1770000000"` |

依据：Marshal `shared/relay/auth/hmac/v2/token.go:11-21`；算法枚举与签名长度 `v2/algo.go:8-11, 33-39`。
- 客户端从管理层的 `token_signature`（**base64 StdEncoding，带 padding**）解码出 32 字节原始签名，与 `token_payload` 的 ASCII 字节拼装（`shared/relay/auth/hmac/store.go:18-38`，`base64.StdEncoding.DecodeString` 在 `store.go:25`）。
- 签名生成（服务端校验逻辑）：`key = SHA256(relay_secret)`（32 字节），`signature = HMAC-SHA256(key, payload)`；`payload = str(unix(now + TTL))`（`shared/relay/auth/hmac/v2/generator.go:31-45`；key 推导 `management/internals/shared/grpc/token_mgr.go:88-90`；TTL 默认 12h，`token_mgr.go:25, 81-93`）。
- 旧 v1（`shared/relay/auth/hmac/token.go:35-48`）算法同为 HMAC-SHA256/TIMESTAMP，仅结构体形态不同；线上二进制统一走 v2 布局（`store.go:30-36` 固定 `AuthAlgoHMACSHA256`）。

### 3.4 AuthResponse（服务端→客户端）
- 布局：`[0x01][0x07][instanceURL ASCII]`（`message.go:184-206`）。
- 客户端处理：读 ≤8192 字节（`client.go:514`），校验 version、type 必须 =7，取偏移 2 起的 ASCII 串为实例 URL 存为 `RelayAddr`（`client.go:502-543`、`shared/relay/client/addr.go:3-13`）。该 URL 用于跨 relay 实例选择公共服务器（`message.go:180-183` 注释、`client/internal/peer/worker_relay.go:57-67`）。
- **认证失败无任何响应**：服务端直接 close 连接（`relay/server/relay.go:136-147`；`handshake.go:85-96`）。客户端表现为 8s 读超时或读错误（`client.go:879-900`）。注意服务端把无法解析 peerID 的探测连接也静默关闭（`relay.go:138-142` 的 `peerid.IsHealthCheck` 分支）。

---

## 4. OpenConn / Transport

### 4.1 OpenConn 生命周期（`shared/relay/client/client.go:296-355`）
1. `dstID = HashID(dstPeerID)`（`client.go:302`）。
2. 前置错误：relay 未连接 → `error "relay connection is not established"`；同 dstID 已存在 → `ErrConnAlreadyExists`（`client.go:305-313`）。
3. 发送 `SubscribePeerState`（1 个 peerID，`shared/relay/client/peer_subscription.go:163-176`），然后**阻塞**等 `PeersOnline`：
   - 等待超时 `OpenConnectionTimeout = 30s`（`peer_subscription.go:16, 108`），超时返回 ctx 错误并退订；
   - 重复等待同一 peer → 立即报错 "already waiting"（`peer_subscription.go:84-87`）。
   - **服务端对离线对端没有否定应答/错误码**：在线则立即回 `PeersOnline`（`relay/server/peer.go:237-257`），离线则注册兴趣、静默等待（`relay/server/store` 的 `GetOnlinePeersAndRegisterInterest`）→ 「对端不在线」只能靠 30s 超时发现。
4. 成功后返回一个实现 `net.Conn` 语义的 per-peer 连接。

### 4.2 Transport 帧语义
- 客户端写：`payload + 38B 头(dstID)`（`client.go:672-702` + `message.go:221-229`）。
- 服务端转发：取出帧内 dstID 找到目标 peer，**把 dstID 原地改写为发送方 peerID** 再转发（`message.go:258-264`、`relay/server/peer.go:209-235`）。→ 接收端帧里的 36B 字段语义是「发送方 peerID」，客户端用它路由到对应的 per-peer 连接（`client.go:629-670`）。
- 目标不在线：服务端**静默丢弃**，无错误回执（`relay/server/peer.go:216-220`）。
- 早期消息缓冲：对端数据先于本地 `OpenConn` 到达时，每 peer 缓存 ≤1 条（新的覆盖旧的），TTL 5s，总容量 10000（`shared/relay/client/early_msg_buffer.go:11-14`）；`OpenConn` 时回放（`client.go:323-329`）。
- 读取队列与背压：每连接 100 条的消息 chan（`connChannelSize`，`client.go:28`）；**chan 满或连接已关闭时新消息直接丢弃**（`client.go:113-129` 的 `select...default`）。→ 上游是有意允许丢包的（WG 自会重传），我们实现应保持「满则丢」而非阻塞读循环。
- 连接关闭：客户端发 `Close`（`[0x01][0x04]`，2B）仅在**整条 relay 连接**优雅退出时（`client.go:849-851, 871-877`）；per-peer 连接无独立关闭帧，仅本地退订+删除（`client.go:805-827`）。收到服务端 `Close` → 结束读循环并关连接（`client.go:609-612`）。服务端优雅关闭同帧（`relay/server/peer.go:149-161`）。
- 对端离线推送：服务端发 `PeersWentOffline` → 客户端关闭对应 per-peer 连接并退订（`client.go:605-607, 784-803`、`peer_subscription.go:65-78`）。

---

## 5. 保活与超时（上游常量名与值）

| 常量/项 | 值 | 方向/含义 | 依据 |
|---|---|---|---|
| `defaultHealthCheckInterval` | 25s | **服务端**每 25s 发一次 HealthCheck | `shared/relay/healthcheck/sender.go:13` |
| `defaultHealthCheckTimeout` | 20s | 发送方超时窗口 = interval+timeout = 45s | `sender.go:14, 57` |
| `defaultAttemptThreshold` | 1（env `NB_RELAY_HC_ATTEMPT_THRESHOLD` 可覆盖） | 连续失败次数阈值 | `sender.go:11`、`healthcheck/env.go:10-24` |
| `defaultHeartbeatTimeout` | 25s+10s = **35s** | **客户端**接收侧：35s 内没收到服务端 HealthCheck 即判死 | `healthcheck/receiver.go:10-12` |
| 客户端应答 | 收到 HealthCheck 立即原样回显 2 字节帧，并喂给自己 receiver | `client.go:618-627`、`message.go:58` | |
| 判死动作 | 客户端：关连接→读循环退出→关所有 peer 连接→触发 onDisconnect（重连由 Guard 接管） | `client.go:739-761, 545-592, 861-869` | |
| 服务端判死 | 超时未收到客户端回显 → 直接断连 | `relay/server/peer.go:186-207` | |
| 其他 | Auth 响应 8s（`client.go:27`）；拨号 30s（`race_dialer.go:14`）；OpenConn 等在线 30s（`peer_subscription.go:16`）；服务端写 10s（`listener/ws/conn.go:16`）；服务端 close 写 3s（`relay/server/peer.go:178-184`） | | |

连接判定死亡的充分条件（任一）：read 返回错误、healthcheck 超时、收到 `Close` 帧（`client.go:545-592`）。

---

## 6. 令牌刷新与中继切换

### 6.1 令牌获取与刷新
- 令牌唯一入口是管理层：SyncResponse 携带 `RelayConfig{urls, token_payload, token_signature}` → `e.handleRelayUpdate`（`client/internal/engine.go:1146, 1185-1214`）→ `relayManager.UpdateToken(&Token{Payload, Signature})`（`engine.go:1192-1194`、`shared/relay/client/manager.go:328-331`）→ 线程安全地换掉 TokenStore 内容（`store.go:18-38`）。
- **没有「带内重新认证」消息**：token 只在（重）连握手时被使用（§3）；已建立连接不受 TTL 影响，服务端也只在握手时验证（`relay/server/relay.go:120-147`）。协议未定义握手后的 re-auth 帧（`message.go:105-144` 的类型集合里没有）。
- 管理层刷新节奏：每 peer 一个 goroutine，ticker = `TTL × 3/4` 周期性生成新令牌并经 SyncResponse 下发（`token_mgr.go:195-208`）；TTL 默认 12h（`token_mgr.go:25`）。
- 我们的对接：`client/core` 收到新 `RelayServers` 时更新本地 token store 即可；若当前 relay 已连接，**无需主动重连**（与官方一致）。

### 6.2 中继地址变更与重连顺序（`shared/relay/client/guard.go` + `picker.go` + `manager.go`）
1. URL 列表更新：`UpdateServerURLs` 原子替换（`manager.go:323-326`）。
2. 断连后先「快速重连」原服务器（URL 仍在新列表内才尝试）：等待网络在线判定（1.5s 预算 + 200ms 稳定窗），成功即结束（`guard.go:86-92, 128-159`，常量 `guard.go:17, 22`）。
3. 快速重连失败 → 指数退避 ticker：初始 2s、×2、随机化、上限 `defaultMaxBackoffInterval=60s`（`guard.go:13, 200-211`），每轮 `PickServer`：**对列表内所有 URL 并发拨号（并发上限 `maxConcurrentServers=7`），首个成功者胜出，其余关闭**（`picker.go:16, 36-118`）。
4. 外服务器（对端所在实例 URL ≠ 本家）：临时建连（`manager.go:333-...` `openConnVia`），空闲 5s 后由 60s 周期的清理循环回收（`manager.go:19-20`）。
5. 控制端/被控端选择公共 relay：controller 用自己的实例 URL，对端用 offer 里带来的 `RelaySrvAddress`（`client/internal/peer/worker_relay.go:129-134`）。

---

## 7. 与 WireGuard 的接口形态

### 7.1 relay 连接 → WG endpoint 的映射（官方形态）
- relayed `net.Conn` 交给 wgproxy（`client/internal/peer/conn.go:559-604` `onRelayConnectionIsReady` → `conn.go:875-889` `newProxy`）。
- 伪造 UDP endpoint：以对端 NetBird 地址派生 `127.1.x.x:port` 形式的假地址（`client/iface/wgproxy/bind/proxy.go:204-226`），注册进 userspace bind，WG 把它当作普通 UDP peer endpoint（`bind/proxy.go:64-104`；endpoint 下发 `conn.go:585` `ConfigureWGEndpoint(wgProxy.EndpointAddr(), ...)`，ICE 断开回切 `conn.go:511-524` `SwitchWGEndpoint`）。
- 泵与背压：**单读泵** `proxyToLocal`：循环从 relayed conn 读 ≤`mtu+220` 字节，经 bind 喂给 WG（`bind/proxy.go:175-202`，缓冲 = `mtu + WGBufferOverhead(220)`，`client/iface/bufsize/bufsize.go:8`）；WG→对端方向由 bind 写回 relayed conn。暂停/恢复用 `Pause`/`RedirectAs`（换 endpoint 前暂停转发防丢包，`conn.go:463-484`、`bind/proxy.go:106-130`）。
- 断连联动：proxy 读错误 → `SetDisconnectListener` 通知 peer 层（`bind/proxy.go:80-82`、`conn.go:578`）。

### 7.2 MTU / 封装开销
- 官方默认 WG MTU `DefaultMTU = 1280`（`client/iface/iface.go:29`）。
- 每个出向 WG 包封装为：`Transport 帧 = 2B 头 + 36B dstID + 包`（38B 固定开销，`message.go:47-49`），再套 WS：客户端帧固定掩码 → RFC 6455 头 2B（≤125B 时）或 4B（更大），+ 4B masking key；TLS/TCP/IP 再各加层。**典型每包开销 ≈ 38 + 6~8 ≈ 44~46B**。
- 1420 MTU 的 WG 包 → 1458B relay 帧 ≪ `MaxMessageSize=8820`，无分片问题；上限是 8820-38 = 8782B 单包。
- QUIC(datagram) 传输受单包预算约束，MTU>1280 时官方直接弃用 datagram 传输（`dialers_generic.go:45-48`）；WS 是流式封装，无此约束。→ 我们只做 WS 时 1280/1420 均安全。

---

## 8. 离线自测方案（最小 relay 假服务器）

**可行性判断：完全可行。** 协议无加密握手之外的状态机依赖：服务端最小实现只需「WS accept + 6 种帧的编解码 + 一张 peerID→conn 表」。假服务器可用纯 Rust（tokio + 现有 httparse）实现，`rel://`（无 TLS）路径完全离线；若要测 `rels://` 需 rustls 自签证书（已有依赖）。

### 8.1 假服务器需实现的最小帧子集
| 帧 | 行为 |
|---|---|
| Auth (6) | 校验 magic+长度（可选：按 §3.3 重算 HMAC 验签）；成功回 `AuthResponse`（instanceURL 可为 `rels://fake:12345`），失败静默断连 |
| AuthResponse (7) | 服务端不收，忽略 |
| HealthCheck (5) | 每 25s 向客户端发；收到客户端回显即刷新计时 |
| SubscribePeerState (8) | 记录订阅；若目标 peerID 在线立即回 `PeersOnline` |
| PeersOnline (10) / PeersWentOffline (11) | 服务端按需发 |
| Transport (3) | 查表转发，dstID 改写为发送方（§4.2）；不在线静默丢弃 |
| Close (4) | 断开该连接 |
WS 细节：Accept 需回 `Sec-WebSocket-Accept = base64(SHA1(key + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"))`；只接受 binary 帧；忽略 `permessage-deflate` 扩展协商（不回该扩展即可，coder/websocket 客户端允许）。SHA-1 可用 rustls 依赖链里 ring 的 `SHA1_FOR_LEGACY_USE_ONLY`，无需新增依赖。

### 8.2 测试用例清单
| # | 用例 | 期望 |
|---|---|---|
| T1 | 正常握手 | Auth→AuthResponse，instanceURL 正确取出 |
| T2 | 坏 magic / 坏版本 / 错算法字节 | 服务端静默断连；客户端报 8s 超时或读错误，进入重连 |
| T3 | 过期签名（payload=过去时间戳，假服务器选配验签） | 同 T2 |
| T4 | 对端不在线 OpenConn | 30s 超时返回错误，无崩溃，订阅清理 |
| T5 | 对端在线回环 | Subscribe→PeersOnline→双向 Transport 回显，字节完全一致（含 0 字节 payload、8820 上限帧） |
| T6 | 帧过大 | 服务端发 >8820 的 Transport 帧；客户端不 panic（丢弃或断连需按 §2.6 决策记录） |
| T7 | 连接中断 | 服务端掐断 TCP → 客户端读循环退出、关全部 peer 连接、触发重连回退顺序（§6.2） |
| T8 | 保活 | 假服务器停发 HealthCheck >35s → 客户端自杀重连；客户端正确回显 |
| T9 | 对端离线推送 | 已建 peer 连接收到 PeersWentOffline 后本地关闭 |
| T10 | 早期消息 | 先发 Transport 再 OpenConn → 5s 内回放成功 |
| T11 | 多 peer 路由 | 两个 dstID 并发，帧按 36B ID 正确分流（验证 §4.2 「帧内 ID=发送方」的读取侧语义） |

---

## 9. 实现要点与坑

1. **36B peerID 不做 base64 上线**：线上一律原始字节 `"sha-"||sha256`；base64 只出现在日志（`id.go:20-22`）和 token_signature 的解码输入（`store.go:25`，StdEncoding 带 padding）。
2. **端序**：协议没有任何多字节数值字段（全为定长字节数组），不存在端序问题；不要自己发明长度前缀（`message.go` 全文无 length 字段）。
3. **WS 掩码**：客户端→服务端帧必须掩码（RFC 6455 强制），服务端→客户端不掩码；只发 binary 帧。
4. **`rel` vs `rels`**：仅是 ws/wss 之别，路径同为 `/relay`，其余协议完全一致；`rels` 需要 TLS+SNI（IP 直拨时 SNI 用 FQDN，`client.go:474-500`）。
5. **认证失败=静默断连**，不要等错误码；实现 8s 超时 + 退避重连。
6. **保活方向**：服务端发起、客户端回显；只发不收或只收不发都会被对端判死（§5）。
7. **帧上限 8820 / 握手 212 / 握手响应 8192**；Auth 帧超 212 服务端直接拒（`message.go:152-154`）。
8. **读侧 36B 字段是对端（发送方）ID**，不是自己填的目的 ID（服务端会改写，`relay/server/peer.go:223`）。
9. **满队列丢帧是官方行为**（`client.go:122-128`），不要改成阻塞读循环。
10. **`MaxMessageSize=8820` 与 WG MTU+220 缓冲的关系**：读缓冲按「一帧一 WS 消息」分配，贪大无益；QUIC 路径才受单包尺寸约束。
11. **重连不要只盯一个 URL**：官方语义是「快速重连原服务器 → 指数退避并发全列表抢第一」（§6.2）；实例 URL（AuthResponse）与配置 URL 是两个概念，跨实例会临时建连。
12. **token 只在握手时生效**：收到新 RelayConfig 不必重连；但重连必须用最新 token，否则在 TTL 过期后被拒（T2/T3 路径）。

---

## 10. 上游未见明确定义、需实现时自查的点（无法确证项）

1. **超长帧（WS 消息 >8820B）的官方客户端行为**：读缓冲恰为 8820 且每消息只读一次（`dialer/ws/conn.go:41-53`），超出部分的处理（丢弃/断连）在源码中无显式分支，依赖 coder/websocket 库内部语义 → 实现时按「丢弃 + 日志」处理并在 T6 记录。
2. **WS 握手非 101 响应（如 404/401）时客户端的重试区分**：库把响应体带进错误，但 `ws.go:38-49` 只解包 net.OpError，上层 Guard 不区分错误种类，一律退避重试 → 无需特判，但无「不可重试」概念可循。
3. **QUIC 传输与 WS 传输对同一服务器共存时的端口约定**：QUIC 用 UDP 同端口号（`quic/quic.go:82-109` 去掉 scheme 后 443/80 默认），但托管 URL 是否总是双栈端口上游未定义 → 假服务器只测 WS 即可，真机对接时再核实。
4. **客户端对 AuthResponse 中 instanceURL 的格式校验**：仅当字符串使用（`client.go:537-542`），无 URL 合法性检查；空串行为未定义（`message.go:202` 只要求 ≥1 字节）。
5. **HealthCheck 在客户端连上但服务端 25s 未发首帧前的窗口**：35s 接收超时从连上即起算（`client.go:287-291`），服务端 sender 也从 Peer.Work 开始即发（`relay/server/peer.go:77-79`），窗口自洽，但「握手后首个 HC 的最大延迟」无显式常量（即最长 25s）——按 25s+35s 理解即可。
