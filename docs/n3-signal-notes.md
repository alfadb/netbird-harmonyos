<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# N4a — signal 服务通道：侦察结论与实现说明（2026-09）

任务：为 `client/core` 实现 NetBird **signal 服务通道**（peer 发现/候选交换的
传输层）。本文记录上游侦察结论（逐行号出处）、协议时序、加密方案、依赖与
交叉编译结论、受保护 socket 路径、测试矩阵与未完成项。

上游参考（仓外，只读）：`~/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`
（commit `791401060d2b95e5f51e3439c0649729132f571e`，下称 pinned commit）。
本文引用格式 `文件:行号` 均指该 commit 的树。

---

## 一、signal 的真实形态：gRPC bidi 流（不是 WebSocket）

`shared/signal/proto/signalexchange.proto`：

- `service SignalExchange`（L9-14）定义两个 RPC：
  - `rpc Send(EncryptedMessage) returns (EncryptedMessage)`（L11，unary）；
  - `rpc ConnectStream(stream EncryptedMessage) returns (stream
    EncryptedMessage)`（L13，**bidi 流**）。
- 消息（L16-39）：`EncryptedMessage{ key: string = 2, remoteKey: string = 3,
  body: bytes = 4 }`（L18-28）；`Message{key, remoteKey, body: Body}` 是解密
  前后的本地表示（L31-39）。
- `Body`（L43-78）：`Type` 枚举 `OFFER=0 / ANSWER=1 / CANDIDATE=2 / MODE=4 /
  GO_IDLE=5 / HEARTBEAT=6`（L45-52）、`payload`（L54）、`wgListenPort`
  （L56）、`netBirdVersion`（L57）、`mode`（L58）、`featuresSupported`
  （L61）、`rosenpassConfig`（L64）、`relayServerAddress`（L67）、
  `sessionId`（L71）、`relayServerIP`（L77）。

**结论：不需要 WebSocket / tokio-tungstenite。** 复用既有 tonic 0.14 栈，
零新增 crate（见"四、依赖"）。

### 入口 URL 与 TLS 配置

- signal 地址来自 management `LoginResponse.netbirdConfig.signal`
  （`HostConfig`，`shared/management/proto/management.proto:341-355`：
  `uri` L348、`protocol` L349，枚举 `HTTPS = 3` L352）。
- `client/internal/connect.go:715-731 connectToSignal`：`Protocol ==
  HTTPS` → TLS 开启（L717-721），随后
  `signal.NewClient(ctx, wtConfig.Signal.Uri, ourPrivateKey, sigTLSEnabled)`
  （L722-723）。
- 上游拨号把 URI 直接交给 gRPC（`client/grpc/dialer.go:31-66
  CreateConnection`；L34-43 TLS 用**系统证书池** + 内嵌根兜底）。我们的
  实现维持 N3-2 契约：`GrpcTransport::Tls(GrpcTlsConfig)` **注入信任根**，
  绝不使用系统 store；URL 需要带 scheme（`https://`/`http://`，由 shell 按
  `Protocol` 组合），host 同时驱动 SNI/证书校验。

### 注册（register）流程：纯 header，无注册消息

- 客户端在 `ConnectStream` 请求上带 metadata
  `x-wiretrustee-peer-id: <base64 WG 公钥>`（`shared/signal/proto/constants.go:4`
  定义；`shared/signal/client/grpc.go:311-314` 附加，注释 L311 "identifying
  ourselves with a public WireGuard key"）。
- 服务端 `signal/server/signal.go:134-152 RegisterPeer` 读取该 header
  （L136），缺失 → `FailedPrecondition`（L137-140）；注册进 registry 后，
  流处理器 `ConnectStream`（L107-132）以 `SendHeader(successHeader)` 确认，
  `successHeader = x-wiretrustee-peer-registered: "1"`（L87、L117-121）。
- 客户端阻塞等 header 并**要求**该确认头（grpc.go:320-327，缺失 → 报错重试）。
  我们的 `SignalClient::register()` 同构实现（缺失确认头 → `Parse` 类，
  参与退避重试）。

### 服务端角色：纯转发者（不持密钥、不解密）

- `Send`（signal.go:95-104）：按 `msg.RemoteKey` 查 registry 转发
  （L98-101）；不在线则交给 dispatcher（L103）。
- `forwardMessageToPeer`（signal.go:161-209）：registry `Get(msg.RemoteKey)`
  （L166）后原样投递到目标流；目标不在线/超时只计数丢弃（L169-208）。
- registry：`signal/peer/peer.go:58-65`（peerId → 流），串行写
  （peer.go:29-45）。

### 心跳 / 保活

- `HEARTBEAT`（proto L51）用作**自寻址探针**：`Key = RemoteKey = 自己公钥`
  （`shared/signal/client/grpc.go:555-566 sendReceiveProbe`），服务端原路
  送回，验证接收方向活着。
- 接收看门狗：30s 无帧 → 发探针，10s 未回 → 断流重连（阈值常量
  grpc.go:29-41，看门狗 grpc.go:522-553）。
- 传输层 keepalive：`client/grpc/dialer.go:53-56`（Time 30s / Timeout 10s）。
- 本增量实现 `SignalClient::send_heartbeat()` 原语（单测+集成测试覆盖自寻址
  往返）；**看门狗定时器本身留给 N5**（与 ICE 会话调度一起做）。

### 错误与重连语义

- 重试循环 `Receive`（grpc.go:189-284）：整段 connect+register+receive 进
  重试；`defaultBackoff` = 800ms 初值 / 随机因子 1 / ×1.7 / 上限 10s /
  预算 3 个月（grpc.go:173-183）——与 `backoff.rs` 的
  `upstream_stream_default()` 完全同参。
- 接收错误分类（grpc.go:570-587）：`Canceled` → 关停；`Unavailable` →
  重试；EOF（服务端关流）→ 重试；其余错误 → 重试。
- `connectivity.Shutdown` 是唯一 `backoff.Permanent`（grpc.go:211-213）。
- **PermissionDenied → 终止**：signal 客户端自身只把 Shutdown 视为
  permanent，但 signal 客户端运行在 engine 连接循环内，engine 对
  PermissionDenied 显式 `backoff.Permanent` + `runCancel()`（"unrecoverable
  error"，`client/internal/connect.go:353-356`）。本实现把该策略固定在
  会话层：Auth 类（PermissionDenied 与 Unauthenticated，经
  `map_grpc_status`）→ `SignalStreamError::Fatal` → `run_events` 返回
  `Err` 终止——与 `crate::sync` 的既有划分（management
  `grpc.go:436-438` 同型）一致。
- 帧级解密失败**不断流**：上游在解密 worker 里只记日志继续（grpc.go:600-602
  + 610-625）。本实现以 `SignalLoopEvent::Malformed` 显式上报并保持流
  （不静默丢弃，也不误断流）。

## 二、加密：与 management 同一 NaCl 信封，密钥即 WireGuard 身份密钥

- `EncryptedMessage.body` 注释即契约："encrypted with the Wireguard private
  key and the remote Peer key"（signalexchange.proto L16-17）。
- seal/open（`encryption/encryption.go`）：`Encrypt` L18-24
  `box.Seal(nonce[:], msg, nonce, peerPub, priv)` —— wire =
  `nonce(24B) || ciphertext`；`Decrypt` L27-42 取前 24B 为 nonce；
  nonce 每条消息 24 字节随机（L44-51）。与 `crate::envelope` 的
  seal/open **字节兼容**，直接复用，零新密码学代码。
- 密钥来源：与 management/隧道同一条私钥（`connect.go:243` 解析 profile
  私钥；`connect.go:722-723` 把同一把 `ourPrivateKey` 交给 signal 客户端；
  另见 signalexchange.proto L17 注释与 `crate::envelope` 模块文档的
  密钥来源考证）。
- 方向（`shared/signal/client/grpc.go`）：
  - 解密收到的帧：peer = **发送方**公钥 `msg.key`（grpc.go:414-431
    decryptMessage）；
  - 加密发送：peer = **接收方**公钥 `msg.remote_key`（grpc.go:434-451
    encryptMessage）。
- **与 management 的差别**：signal 没有 `GetServerKey` 交换、没有 JWT；
  唯一身份就是 WG 公钥 header（grpc.go:311-314）。`EnvelopeKeyPair` 注入
  `SignalClient::connect_with_socket_source`，收发各按对方公钥封/拆。

## 三、协议时序（实测与上游代码一致）

```
A(客户端)                        Signal 服务                        B(客户端)
  | ConnectStream + x-wiretrustee-peer-id: A公钥 |                    |
  |  <- headers: x-wiretrustee-peer-registered: 1 --|                 |
  |   （注册完成，此后流长期保持）                                      |
  | ConnectStream + header B公钥 ---------------------------------->  |  (B 同理)
  | Send{key:A, remoteKey:B, body=seal(B公钥, Body{OFFER,"ufrag:pwd",port})} |
  |                            registry[ 取 remoteKey=B ] ------------> | B 用 A公钥+B私钥 open
  | <---------------------------------- 同型 ANSWER/CANDIDATE --------|
  | Send{key:A, remoteKey:A, HEARTBEAT} 原路返回（自寻址探针）           |
```

Offer/Answer 的 `payload` 为 ICE 凭据 `"ufrag:pwd"`（`shared/signal/client/
client.go:61-71 UnMarshalCredential`、L74-97 `MarshalCredential`，由
`client/internal/peer/signaler.go:24-79` 组装）；CANDIDATE 的 `payload`
为 pion ICE candidate 的 marshal 字符串（signaler.go:32-41）。引擎单点消费
解密后的流（`client/internal/engine.go:2021-2088`，`WaitStreamConnected`
L2088）。

## 四、依赖与交叉编译结论

- **形态是 gRPC → 无新依赖**。tokio-tungstenite 探针**不需要**（任务第二步
  的探针仅为 WebSocket 分支准备）。新增依赖边仅两处、零新包，`Cargo.lock`
  仅 +1 行（`netbird_core` 的依赖清单加 `futures-core`）：
  - `futures-core = "0.3"`（[dependencies]）：bidi 请求流的 `Stream` impl
    所需 trait（已在 lock 中，tonic/hyper 依赖）；
  - tokio 增加特性 `sync`（mpsc 请求流；同 crate，特性开关，lock 不变）。
- **离线交叉编译已验证**：`cargo build --release --target
  aarch64-unknown-linux-ohos --offline --locked` exit 0（见"七、验收"，
  无需仓外探针步骤；management proto 管线同款）。
- codegen：`build.rs` 增加 `protox::compile([signalexchange.proto])` →
  `$OUT_DIR/signalexchange.rs`（client/server stub 一并生成，server 侧供
  in-process mock 测试用）。proto 逐字复制入仓
  （`client/core/proto/signalexchange.proto`，sha256
  `9374413168551f7ded6c009dda2bfaa55e111bf138f2fe61780752a05c566ab9`），
  import 的 `google/protobuf/descriptor.proto` 走 protox 内嵌 WKT 解析。

## 五、接口与受保护 socket 路径

新增 `client/core/src/signal.rs`（SPDX: AGPL-3.0-or-later）：

- `SignalClient::connect_with_socket_source(endpoint, transport,
  connect_timeout, request_timeout, keys, sockets:
  Arc<dyn ManagementSocketProvider>, connect_addr)` —— **唯一构造路径**，
  复用 `crate::mgmtsock::ProtectedSocketConnector`（N3-7 组件原样使用：
  每次 dial `take_fd` → dup → protect 后 connect；提供者空 → dial 失败
  → fail-closed，无任何裸连回退）。治理 `docs/native-nx-governance.md`
  §二第 4 条把 signal 列入逐 socket protect 范围；DNS 由 shell 侧解析
  （`connect_addr`），URL host 仍驱动 TLS 校验。
  未做 trait 改名/新模块：`mgmtsock.rs` 的 seam 本就是 provider-泛型的，
  改名会波及 N3-7 已交付的 connector/napi 面与测试，收益为零。
- `register()`：开 `ConnectStream` + 身份 header + 确认头校验（返回
  `RegisteredStream{outbound: mpsc::Sender, inbound: Streaming}`）。
- `send(msg)`：unary `Send`（对 `remote_key` 封装；单次尝试 + 客户端级
  超时——上游 4 次重试阶梯 grpc.go:470-492 属发送策略，留给 N5 接线方
  决定）。
- `send_to_stream(msg)`（经 `SignalSession`）：`SendToStream` 同型
  （grpc.go:396-411）。
- `build_message / encrypt_message / decrypt_envelope / send_heartbeat`：
  组包与信封原语（单测覆盖）。
- `SignalSession`（镜像 `crate::sync::SyncSession`）：`connect` /
  `next_frame`（`IncomingFrame::Message | Malformed`）/ `run_events`（事件
  `Registered | Message | Malformed | Broken`），`with_policy` 注入
  backoff/RNG/时钟（无 sleep 参数断言），诊断 `reconnects()`,
  `observed_reconnect_delays()`, `last_stream_error()`。
  **每次重连重新 register → 通道重拨 → 重新取受保护 socket**；拿不到即
  fail-closed（`Broken` 事件 + 退避重试，与 N3-7 shell 回填契约相同）。

## 六、测试矩阵（全部离线、in-process，无 sleep 断言）

`tests/signal_mock/mod.rs`：按上游真实协议实现的最小 mock `SignalExchange`
（header 注册、registry 纯转发、可注入拒绝/损坏/无确认头），rcgen CA+叶子
TLS，计数 listener（accept 计数 + 传输层 kill + fd 源）。

`tests/signal_channel.rs`（6 用例）：

1. 注册身份 header == 本端 base64 WG 公钥；无确认头 → `Parse` fail-closed；
2. 两真实客户端 Offer/Answer/Candidate 往返（流发送 + unary Send 两路径），
   **服务端 sink 真解密**并断言明文字段（type/payload/port/version），
   且发给 A 的消息在 B 的密钥下不可解；
3. HEARTBEAT 自寻址往返；
4. **真实 TLS 握手**交换 + 不信任 CA 拨号失败（`Network`，照样先取保护
   socket）；
5. 通道只走注入 socket：初始 taken==accepted==1；kill → EOF；重连重取新
   fd、恰好一条新连接；再 kill 后提供者耗尽 → register 失败、
   `accepted` 停在 2（**无裸连**）、`pending==0`；
6. 空提供者构造即失败（`Network`），accepts==0。

`tests/signal_session.rs`（4 用例）：

1. 会话 Registered → 收到 B 的 Candidate → kill → `Broken` → 重连（新
   保护 fd）→ `Registered`（注：会话循环内 tonic 通道可能自启动并取消
   一次竞态拨号，故此处断言 `taken ≥ accepted ≥ 2` 与逐 fd 精确相等的
   channel 用例互补）；确定性退避（注入 RNG/时钟）；
2. PermissionDenied 注册 → `run_events` 返回 `Auth` 类 Err，0 次重连、
   0 个退避延时、仅 1 次尝试（与上游 engine 策略一致，connect.go:353-356）；
3. 密文损坏帧 → `Malformed` 事件、**流不断**（关损坏后下一帧正常送达，
   无 `Broken`/重连，grpc.go:600-602 同型）；
4. Unavailable 注册失败重试至退避预算耗尽 → `Err(Network)`，Broken ≥2、
   延时序列非空、连接数停在初始值。

单元测试（`src/signal.rs` 内 6 个）：常量与上游一致、组包字段、封/拆
往返与 wire 长度（nonce24||plain||tag16）、错钥/截断/伪包拒绝、错误类
Auth/Closed 划分。合计新增 **16 个用例**；全库 `cargo test --offline
--locked` 179 通过 / 0 失败（既有测试未放宽）。

## 七、验收（实跑输出，2026-09）

1. `bash client/core/build.sh` → **exit 0**，产物
   `target/aarch64-unknown-linux-ohos/release/libnetbird_core.so`
   （大小见最终验收输出）；
2. `cd client/core && cargo test --offline --locked` → **179 passed,
   0 failed**（含 signal 新增 16）；
3. `bash client/build.sh` → **exit 0**（HAP 大小见输出）；
4. 探针交叉编译：**不适用**（无新 crate；离线 ohos 交叉构建由验收 1 覆盖）；
5. `git status --short` 范围：`client/core/**`、`docs/n3-signal-notes.md`、
   `THIRD-PARTY-NOTICES.md`。

## 八、未完成项（后续增量）

- **ICE / N5 接线**：`SignalSession` 事件 → ICE 代理（offer/answer 凭据
  `"ufrag:pwd"`、candidate marshal 字符串的真正消费者）；接收看门狗
  （30s/10s 自寻址探针，grpc.go:522-553）挂在同一调度上。
- **relay**：`Body.relayServerAddress/relayServerIP` 字段已透传
  （`SignalMessage`），relay 客户端（N4b+）未实现。
- **发送重试阶梯**：上游 unary Send 4 次/递增超时（grpc.go:470-492）未复刻
  （当前单次 + 客户端级超时）。
- **ArkTS 接线**：`signal` 模块未接入 `napi.rs`/entry（`connector_*` 同款
  socket feed/start API 需为 signal 增量设计）；shell 侧 signal socket 的
  回填节奏（每个重连一个）与 DNS 解析也属该增量。
- 上游 `GO_IDLE/MODE/featuresSupported/rosenpass` 字段透传但不主动产生。

---

## 九、N5d — 真实 signal 流接入 Connector（2026-09）

任务：把 N4a 的 `SignalSession` 真实流接进 Connector 的 per-peer ICE 编排，
替换 N5c 的临时 `LoggingSignalExchange` seam。**本节为交付记录**（时序、
语义、暴露面、契约、未做项）；上游引用一律 `文件:行号`（pinned commit）。

### 9.1 接线时序（生产路径）

```
sync(netbird_config.signal) ──┐
                              ├─ 都齐备 → SignalRuntime::maybe_start()
壳 feed(fd, connect_addr) ────┘        │
   connector_signal_socket_feed        ▼
                          spawn_signal_link（peer_conn.rs）
                                       │ 1) 初始受保护拨号（失败按
                                       │    upstream backoff 重试，
                                       │    grpc.go:123-131；空 fd 源
                                       │    fail-closed，绝无裸连）
                                       ▼
                          SignalSession::run_events_with_outbox
                          （register → 收帧/发帧 select → Broken→
                           退避→重注册，每次重连重取受保护 fd）
```

- `SignalClient::connect_with_socket_source` 之外的**每次** tonic 重拨都经
  `ProtectedSocketConnector` 取新鲜 fd（N4a 语义，接线未绕过）；
  `Registered`/`Broken` 事件同步维护 `PeerIceOrchestrator::set_signal_ready`
  与适配器的 registered 闸。
- 收帧路由（connector.rs `route_signal_message`）：`from_key` =
  信封 `EncryptedMessage.key` = 发送方 WG 公钥（grpc.go:414-431），与网络图
  peer 的 `wg_pub_key` 同一身份域，直接交给
  `PeerIceOrchestrator::handle_signal(from_key, …)`——编排层按该键匹配
  （engine.go:2043-2045 按 `msg.Key` 找 peerConn 的同型；未知 key 由编排层
  丢弃计数）。`HEARTBEAT/MODE/GO_IDLE` 非 ICE 帧，路由层直接跳过
  （engine.go:2036-2040 心跳短路；GO_IDLE 走 connMgr，本增量不做）。
- 发帧：编排 outbox → `RealSignalExchange::send`（无界队列）→
  `SignalSession::send_outgoing` 密封（peer=remote_key，grpc.go:434-451）
  → 活跃流（`SendToStream`，grpc.go:396-411）。队列在 select 中与收帧并举，
  出帧不等收帧。
- signal 地址（`derive_signal_endpoint`）：显式 `http(s)://` 按 scheme；裸
  `host:port`（线上形态）**继承 management 传输安全等级**（同一注入 CA）。
  依据：`connectToSignal` 按 `HostConfig.protocol == HTTPS` 决定 TLS
  （connect.go:717-721），但本仓网络图只保留 uri（engine.go:1185
  "todo update signal"，protocol 字段未进模型），故以 management 的
  TLS 决策为代理。

### 9.2 role / 应答语义（含行号）

- signal 报文**不携带角色字段**；ICE controlling/controlled 是本地决策。
  收到对端 OFFER 而本端尚未发起时：**必须应答**——上游 handshaker 的
  `Listen` 对每条远端 OFFER 无条件 `sendAnswer()`
  （`client/internal/peer/handshaker.go:130-152`，L150）；本仓
  `PeerIceOrchestrator::handle_signal` 的 Offer 分支在无会话时建
  controlled 起始会话（`IceSession::new(creds, controlling=false, …)`）、
  采纳对端凭证并置 `answer_owed`（本地候选就绪后补发 ANSWER）。glare
  （双方同时 OFFER）沿用 N5b tie-breaker 语义收敛（RFC 8445
  §7.3.1.1/§7.2.5.1，N5c/peer_conn_e2e 已测）。
- **未就绪不发不假装**：上游发送前检查 `signaler.Ready()`，未就绪返回
  `ErrSignalIsNotReady`（handshaker.go:16 定义、L208 使用、L212-214
  `sendOffer` 前置检查）。本仓同型：`RealSignalExchange::send` 在未注册时
  返回 `Network` 类错误（计数 `refused_sends()`），编排层把帧**保留在
  outbox** 下一拍重试——与 `LoggingSignalExchange` 的"打点丢弃"（返回 Ok、
  帧消失）形成对照，测试钉死（tests/signal_link.rs 未注册用例）。

### 9.3 signal 状态暴露（connector_status）

`status.signal` 字段（N5d）：`{"registered":bool,"reconnects":u64,
"last_error":{"class","status"}|null}` —— registered 由 `Registered`/
`Broken`/worker 结束事件维护；reconnects 只计流断开（初始注册与拨号失败
不计，`crate::signal` 诊断口径）；last_error 仅分类（凭据纪律）。
`connector_network_config()` 快照新增 `signal` 字段（URI），壳侧据此补给。

### 9.4 ArkTS feed 契约

- 新 NAPI：`connector_signal_socket_feed(fd, addrJson)`，
  `addrJson = {"connect_addr":"ip:port"}` **必填**；`{ok:true,queued:N}` /
  `{ok:false,error:token}`（token：`no-connector` / `no-socket-source` /
  `socket-fd-missing` / `socket-fd-invalid` / `socket-addr-invalid`）。
- 壳侧流程（NetBirdVpnExtensionAbility `provisionSignalSocket`，位于
  connector 启动之后、`connection.create()` 之前）：解析快照 `signal` URI
  （`parseSignalEndpoint`，端口缺省 443）→ **壳侧 DNS**（DNS 不进 native，
  同 management 规则）→ `mgmt_socket_open()`（TCP，未连接）→
  `VpnConnection.protect(fd)`（5s box，fail-closed）→
  `connectorSignalFeed(fd, addr)`。**任一步失败 = connectorStop + 不建
  VPN**（没有 signal 就没有 offer/answer/候选路径，等于没有 peer 连通，
  fail-closed 并已在代码注释与本节标注）。
- 地址契约：**首个** feed 的地址在链路生命周期内生效（与 management
  factory 固定 connect_addr 同约定）；后续 feed 只补充 fd 队列（重连
  逐次消费）。重连后的再补给节奏（watcher 周期性喂）是后续增量。

### 9.5 测试（tests/signal_link.rs，全部离线 in-process）

1. `dual_peer_real_signal_sessions_converge_and_land_wg_endpoints`：双实例
   经真实 `SignalSession`（注册 header、信封加解密、mock 纯转发）交换
   offer/answer/候选 → 双方 Connected、selected pair 一致、WG endpoint 落配
   （可注入 WG seam 断言）；mock sink 服务端**真解密**断言 OFFER 字段
   （`ufrag:pwd` 16/32 规则）；`signal_ready` 仅在服务端确认注册后置真
   （事件回调内对着服务端 registry 断言，客户端无法"先亮灯"）。
2. `unregistered_sends_are_refused_retained_and_delivered_after_registration`：
   空 fd 源 → 拨号 fail-closed；强制 ready 的编排发起后 seam 显式拒绝
   （refused 计数）且帧保留，注册恢复后同帧送达（服务端恰 1 条 OFFER）。
3. `transport_break_reconnects_re_registers_and_re_takes_protected_fd`：
   传输层 kill → Broken → signal_ready 落假 → 重连（taken 增长 = 重取
   受保护 fd；服务端同 key 第二条注册 header）→ signal_ready 重新置真、
   ICE 会话保持 Connected、重注册后信号路径再次通帧。
4. `empty_signal_socket_source_fails_closed_without_any_dial`：空源
   `taken==0`、服务端永远看不到该 peer（无裸连）、pump 后 peer 停在 Idle。
   另有 connector 单测：`apply_update_arms_signal_link_and_dials_fail_
   closed_without_fds`（sync 触发启动 + 状态暴露 + 空源失败分类）、
   `signal_endpoint_derivation_rules`、`signal_feed_requires_connector_
   source_and_addr`。

### 9.6 未做项（如实）

- **relay/TURN**：`turns`/`relay` 仍不实现；Fallback 到 relay 的上游语义
  （conn.go:489-520）不存在——peer 失败即不可达。
- **ICE restart / 会话重建**：signal 断流期间已 flush 的帧不重发；握手
  中途断流的续传依赖对端重发（上游由 conn 状态机周期重 offer，本增量未做）。
  worker 结束（fatal/预算耗尽）后不自动重启链路。
- **真机**：全部为宿主 in-process 验证；`VpnConnection.protect` on-device
  行为仍属物理门（N2b），signal socket 的真机 protect 路径未验证。
- **双实例 E2E 的 WG 层**：WG 端点仍落在 `WgPeerRegistry`（本地登记 +
  endpoint 记录），无真实 WG 设备数据面、无真实握手流量。
- **接收看门狗**（30s/10s 自寻址探针，grpc.go:522-553）仍未实现
  （N4a 起遗留）；`GO_IDLE/MODE` 不产生不消费。
- 上游 `signal.NewClient` 的拨号重试预算（30 分钟形态）以
  `ExponentialBackoff::upstream_stream_default` 代理；`Send` 的 4 次重试
  阶梯（grpc.go:470-492）仍未复刻。
