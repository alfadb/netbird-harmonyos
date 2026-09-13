<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# N5a ICE 候选收集笔记（host + srflx，自研 STUN）

上游参考 commit：**netbirdio/netbird @ `791401060d2b95e5f51e3439c0649729132f571e`**
（仓外 `/home/worker/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`）。
本文覆盖 `client/core/src/ice.rs`、`client/core/src/stun.rs`、
`client/core/src/sys.rs`（N5a 增量）与 `tests/ice_*.rs`。

## 一、上游侦察结论（逐行号）

### 1. ICE 用什么（pion/ice v4，netbirdio fork）

| 事实 | 上游来源 |
| --- | --- |
| Agent 配置：MDNS 禁用、UDP4+UDP6、Urls=StunTurn、候选类型、接口过滤、UDPMux、超时 | `client/internal/peer/ice/agent.go:45-66`（`MulticastDNSMode:Disabled` L52、`NetworkTypes` L53、`Urls` L54、`CandidateTypes` L55、`InterfaceFilter` L56、`UDPMux/UDPMuxSrflx` L57-58、`FailedTimeout/DisconnectedTimeout/KeepaliveInterval` L61-63） |
| keepalive 4s / disconnected 6s / failed 6s / relay-acceptance 2s | `agent.go:22-25`（`iceKeepAliveDefault` 等） |
| 候选类型集合：host + srflx + relay（强制 relay 场景只有 relay）；P2P 子集 = host + srflx；prflx 从不配置（pion 在连通性检查中推导） | `agent.go:127-133`（`CandidateTypes()`）、`agent.go:135-137`（`CandidateTypesP2P()`） |
| IPv6 可关（`DisableIPv6Discovery` → 只留 UDP4） | `agent.go:69-73`；`client/internal/engine_generic.go:14` |
| 凭据：ufrag 16 字符 / pwd 32 字符 随机 | `agent.go:114-123`（`GenerateICECredentials`） |
| Trickle 形态：`OnCandidate` 每个候选回调即发（非等 gathering 完成）；`GatherCandidates()` 后 agent dial | `client/internal/peer/worker_ice.go:228`（挂回调）、`worker_ice.go:412-433`（`onICECandidate` → 每候选 `SignalICECandidate`）、`worker_ice.go:255-263`（gather→dial） |
| 收方：每个到达候选即时 `AddRemoteCandidate` | `worker_ice.go:163-186`（`OnRemoteCandidate`） |
| interface 黑名单两层：`lo` 前缀硬编码 + 黑名单前缀匹配；未列出的 WireGuard 接口再经 wgctrl 探测剔除（Go 运行时绑定，本仓不可复刻） | `client/internal/stdnet/filter.go:13-24`（`InterfaceFilter`）、`filter.go:26-40`（wgctrl） |
| 黑名单默认值（含隧道接口 `wt0`、`wt`、`utun`、`tun0`、`wg`、`ts`、docker 等） | `client/internal/profilemanager/config.go:56-59`（`DefaultInterfaceBlacklist`）、`config.go:468-473`（空则填默认）；`wt0` 定义 `client/iface/configurer/name.go:6` |
| UDPMux：上游共用一个 socket（`SingleSocketUDPMux`） | `client/internal/engine_generic.go:15-16` |

### 2. STUN/TURN 从哪来（与 network_map 的关系）

| 事实 | 上游来源 |
| --- | --- |
| `NetbirdConfig.stuns[].uri` → `stun.ParseURI` → `[]stun.URI` | `client/internal/engine.go:1525-1541`（`updateSTUNs`，ParseURI 在 L1530） |
| `NetbirdConfig.turns[]` → ParseURI + user/password | `engine.go:1543-1562`（`updateTURNs`） |
| stuns+turns 合并存入 `stunTurn`（agent 的 `Urls`） | `engine.go:1126-1152`（`updateNetbirdConfig`，合并+Store 在 L1140-1144） |
| URI 形态：`stun:host:port`（管理面下发的就是带 `stun:` scheme 的 URL；本仓 network_map.rs 测试夹具同形：`stun:stun.netbird.io:3478`） | 同上；`shared management proto NetbirdConfig`（本仓 `proto/management.proto` 同源） |
| 本仓解析：`NetbirdServers.stuns: Vec<String>` / `turns: Vec<TurnServer>` | `client/core/src/network_map.rs:186-194`（模型）、`network_map.rs:383-391`（转换） |

### 3. 候选如何经 signal 交换

| 事实 | 上游来源 |
| --- | --- |
| CANDIDATE 消息：`Body{Type: CANDIDATE, Payload: candidate.Marshal()}`，外层 `Message{Key, RemoteKey}` | `client/internal/peer/signaler.go:32-41`（`SignalICECandidate`） |
| `Body.payload` = pion 候选串（RFC 8839 candidate-attribute：`candidate:<foundation> <component> udp <priority> <addr> <port> typ <type> [raddr <a> rport <p>] …`） | 同上 L38（`Payload: candidate.Marshal()`）；收方 `client/internal/engine.go:2063-2071`（`ice.UnmarshalCandidate`） |
| OFFER/ANSWER 的 payload = `"ufrag:pwd"`，另带 `wgListenPort`、`netBirdVersion`、Rosenpass、sessionId | `shared/signal/client/client.go:74-101`（`MarshalCredential`，payload 在 L77）；解析 `client.go:60-71`（`UnMarshalCredential`，恰好两段） |
| 对端身份绑定：`EncryptedMessage.key`（发送方 WG 公钥）/`remoteKey`（收件人）；engine 按 `msg.Key` 查 peerStore 路由到对应 Conn | `signalexchange.proto`（`key=2, remoteKey=3, body=4`）；`engine.go:2039-2042`（`peerStore.PeerConn(msg.Key)`） |
| proto 字段号（本仓 verbatim 副本同号）：`Body.type=1, payload=2, wgListenPort=3, netBirdVersion=4, sessionId=10`；enum `OFFER=0, ANSWER=1, CANDIDATE=2` | 本仓 `client/core/proto/signalexchange.proto`（`Body` 块） |
| 本仓发送/接收载体：`SignalMessage{from_key, remote_key, kind, payload, …}` | `client/core/src/signal.rs:192-224`（`payload` 字段注释即指明 CANDIDATE=候选串、OFFER/ANSWER=`"ufrag:pwd"`） |

### 4. ICE 成功后 WG endpoint 的决定（本增量只描述，不实现）

| 事实 | 上游来源 |
| --- | --- |
| 收集→拨号→**selected pair**：`agentDial` 完成连通性检查后 `GetSelectedCandidatePair()` 取 pair | `worker_ice.go:255-306`（dial 与取 pair） |
| 直连：`ep = ResolveUDPAddr(remoteConn.RemoteAddr())`（selected pair 对端地址/端口直接作为 WG endpoint）；relay：本地 wgProxy 监听地址充当 endpoint | `client/internal/peer/conn.go:444-461` |
| `ConfigureWGEndpoint(ep, presharedKey)` 落配到 WireGuard | `conn.go:476-478`（经 `endpointUpdater`） |
| 旧版直连兼容：对远端 WG 端口额外打洞 `punchRemoteWGPort` | `worker_ice.go:309-312, 392-410` |
| 保活/重协商边界：agent 4s keepalive、6s disconnected/failed（agent.go:22-24）；断开时若 relay 可用则回落 relay 并可升级 | `conn.go:489-520`（`onICEStateDisconnected` → switch back to relay） |
| **N5a 不做**：连通性检查（Binding request/response + USE-CANDIDATE）、prflx 推导、pair 提名/选择、`ConfigureWGEndpoint`、keepalive 状态机、TURN/relay、offer/answer 的 ufrag/pwd 生成与发送、重协商 | 本仓 `src/ice.rs` 模块尾注同列表 |

## 二、依赖选型与交叉编译探针

**结论：自研最小实现，零新增依赖。** 对比：

- `webrtc-ice`（webrtc-rs）候选方案：多 crate 依赖树（stun/util/if-addrs/tokio 系），
  且其 UDP socket 在库内部创建，无法保证「建连前 protect 且 fail-closed」的
  治理 §二.4 语义（要么改库、要么自写 UDPMux——工作量不小于自研）；体积与
  `--offline --locked` 冻结链引入不可控变量。
- N5a 实际只需要：host 枚举 + 一次 STUN Binding 交换 + 候选串序列化。
  自研约 500 行（stun.rs ~230 + ice.rs ~640 含文档），纯 std。

**仓外探针**（`/home/worker/harmonyos-signing/netbird-n1bdisc/refs/ice-probe/`，
零依赖 crate，验证 FFI 声明 + struct ifaddrs/addrinfo 布局 + STUN 编解码可
以 `aarch64-unknown-linux-ohos` 目标构建）：

```
$ cd refs/ice-probe && cargo build --release --target aarch64-unknown-linux-ohos --offline
    Finished `release` profile [optimized] target(s) in 0.22s
$ echo $?
0
$ cargo tree --offline | wc -l   # 依赖树规模
1
```

- 探针 exit 0；产物 `libice_probe.so` 480,984 字节（未 strip 的探针产物，非产品增量）。
- 依赖树规模 = 1（仅探针自身）；许可 = 仓库自有 AGPL 代码，**无第三方新增，
  `THIRD-PARTY-NOTICES.md` 无需追加**。
- 产品 `.so` 增量 = ice+stun 两个纯 Rust 模块（无链接面新增，见下）。

## 三、实现要点（本增量范围）

### 候选模型与 proto 对齐

- `ice::Candidate` 字段与 `Body.payload` 一一对应：foundation/component/
  transport/priority/address/port/typ/raddr/rport（`marshal`/`unmarshal`，
  `candidate:` 前缀必须，`a=` 前缀容忍，未知扩展属性跳过，raddr/rport 成对）。
- 候选类型枚举含 Prflx/Relay 以无损往返远端候选；本模块只产出 Host/Srflx。
- 优先级 = RFC 8445 §5.1.2.1（type-pref：host 126 / prflx 110 / srflx 100 /
  relay 0；local-pref 65535；component 1）。foundation = FNV-1a(transport,
  type, address, base) 确定性派生。
- UDP4-only（同上游 `DisableIPv6Discovery` 杠杆，agent.go:69-73）；IPv6
  字面量/映射显式报错，不静默丢弃。

### 接口枚举与 VPN 接口排除

- `DEFAULT_INTERFACE_BLACKLIST` 逐字对齐上游 `DefaultInterfaceBlacklist`
  （profilemanager/config.go:56-59，`wt0` 打头）；`interface_allowed` 对齐
  `stdnet.InterfaceFilter` 前缀语义（`lo` 硬编码 + 黑名单前缀，
  filter.go:13-24）。上游第二层 wgctrl 探测不可复刻，由黑名单覆盖常见隧道名
  （wt/wg/utun/tun0/…），差额已记录（见「未完成项」）。
- 枚举源为 seam：`InterfaceSource`（生产 `SystemInterfaces` = dlopen
  `libc.so` + `getifaddrs`；测试/壳侧注入 `StaticInterfaces`）。getifaddrs
  符号缺失 → 显式错误（fail-closed），不猜测地址。

### srflx（STUN Binding）

- `stun.rs`：空属性 Binding Request（RFC 5389 §2.2 合法）；响应校验
  type/cookie/transaction-id/长度/属性边界；XOR-MAPPED-ADDRESS 优先、
  MAPPED-ADDRESS 回退；ERROR-CODE 解析为 `StunReply::Error(code)`；
  IPv6 family 显式 `no-ipv4-mapping`。事务 id 取自 `/dev/urandom`
  （固定路径 O_RDONLY；熵不可用 → 失败，绝非常量 id）。
- 每个（接口 × 服务器）一个事务，同一接口 socket 上**按事务 id 分发**响应
  （避免多服务器应答互吞）；未知事务 id 的数据报 = 未认证噪声，静默丢弃 →
  该服务器按 Timeout 收场。deadline 默认 3000ms（`GatherConfig.timeout_ms`
  可注入；poll 10ms 步进，遵循 sys.rs A4 有界等待惯例）。
- 一个 STUN 服务器失败不影响其他服务器/其他接口/宿主候选——所有失败记入
  `GatherResult.errors`（context, ManagementError），部分成功可见不吞错；
  「零候选且有错」才整体 `Err`。

### 受保护 UDP socket（治理 §二.4）

- `ice::UdpSocketSource` = mgmtsock `ManagementSocketProvider` 模式的 UDP 孪生；
  `ProtectedUdpFdSource`（queue+`feed`+`pending`+`taken` 审计计数）与
  mgmtsock 的 `ProtectedSocketFdSource` 同形。
- 契约：壳侧创建**全新未绑定** AF_INET/SOCK_DGRAM socket → protect → 传 fd；
  fd 号是**借用**（消费方 `dup_socket_fd` 复制后只碰副本：set O_NONBLOCK →
  bind 接口地址:0 → getsockname 得宿主候选端口 → sendto/recvfrom → close
  副本）；原 fd 号绝不 read/write/close。空队列 → fail-closed
  （`Network("protected-udp: no-protected-socket …")`），无任何裸 socket 回退。
- 每个接口轮次重新 `take_fd`（绝不复用旧 fd）；`taken()` 与尝试次数对账
  （mgmt `taken==attempts` 同款审计，tests/ice_gather.rs 三重证明：
  taken 计数、提供 fd 事后 getsockname 端口 == 宿主候选端口（dup 共享 OFD，
  私建 socket 无法满足）、mock 只见该源端口）。

### fd / syscall surface 变更（sys.rs）

- 新增 extern：`getsockname`（绑定副本的本地端口）。
- `getifaddrs/freeifaddrs/getaddrinfo/freeaddrinfo`：**dlopen+dlsym 运行期
  解析**（abi.rs/napi.rs 惯例），不进链接面；缺失即 fail-closed。
- `/dev/urandom`（O_RDONLY 固定路径）加入 openat 白名单式用途（A6 形状），
  仅用于 12 字节事务熵。
- 均已在 sys.rs 注释与上文登记；探针先行验证（exit 0）。

## 四、测试（离线，206 通过 / 0 失败全仓）

| 文件 | 用例数 | 关键断言 |
| --- | --- | --- |
| `src/stun.rs` 内嵌 | 6 | RFC 5769 §2.1 请求头逐字节；§2.2 IPv4 响应解出 192.0.2.1:32853；§2.3 IPv6 响应显式拒绝；MAPPED 回退；ERROR-CODE→420；8 种畸形形状全为 `stun:*` Parse 且不 panic |
| `tests/ice_stun_uri.rs` | 5 | `stun:host:port`/默认 3478；`turn:` → UnsupportedUrl；坏形态分类（UnsupportedUrl vs Request{0}）；事务 id 熵（两次必不同） |
| `tests/ice_candidate.rs` | 7 | pion 规范串逐字段解析；marshal/unmarshal 无损往返 + `a=` 形态；RFC 优先级公式数值；foundation 确定性；**proto 对账**（prost 生成 Body 编解码后 payload 仍可还原候选，type=CANDIDATE(2)/payload=2 号字段）；OFFER/ANSWER `ufrag:pwd` 两段式；11 种恶意 payload 全为 `candidate:*` Parse 不 panic |
| `tests/ice_gather.rs` | 9 | **受保护 socket 三重证明**（taken==attempts 跨轮次累计、提供 fd 事后端口==宿主候选端口、mock 只见该源端口）；空 provider fail-closed；host 枚举排除 wt0/lo/utun（含前缀语义单断言表）；silent server → Timeout 且宿主候选存活；5 类敌意服务器（坏 cookie/坏 type/ERROR-CODE 400/IPv6 映射/错误事务 id）分类正确不 panic 且健康服务器仍产出；解析失败服务器记账跳过；无可用接口=空成功且不取 fd |

测试约束遵守：超时用例断言的是 deadline 结果分类（150ms），无 sleep 断言；
既有测试全部保持原强度（206 = 原有 179 + 新增 27；既有 0 改动）。

## 五、未完成项（N5b 边界，刻意不做）

1. **连通性检查**：Binding request/response 双向 + USE-CANDIDATE、重传退避
   （RFC 8445 §6/§11；上游由 pion agent 完成，worker_ice.go:255-306）。
2. **prflx 候选推导与 pair 提名/选择**（selected pair；`GetSelectedCandidatePair`
   worker_ice.go:293）。
3. **WG endpoint 落配**：`ConfigureWGEndpoint(ep, …)`（conn.go:476-478）与
   wgproxy/relay 分支（conn.go:444-461）、旧版端口打洞（worker_ice.go:392-410）。
4. **TURN/relay 候选**（`turn:` URI 当前显式 UnsupportedUrl；上游 relay 走
   另一条 relay client 通道）。
5. **offer/answer 会话层**：ufrag/pwd 生成（agent.go:114-123）、session id、
   `SignalOffer/SignalAnswer` 发送与远端候选注入循环、心跳。
6. **agent 状态机**：4s keepalive / 6s disconnected / 6s failed、断开回落
   relay、重协商（conn.go:489-520）。
7. **MESSAGE-INTEGRITY / FINGERPRINT 校验**（短串密码在 N5b 连通性检查中才有
   意义；事务 id 熵为当前唯一响应认证）。
8. 上游 wgctrl 第二层接口过滤（filter.go:26-40）——黑名单覆盖常规隧道名，
   非常规命名的 WG 接口暂不识别。
9. IPv6（UDP6 候选与 AAAA 解析）。

## 六、验收记录（2026-09-13 实跑）

| 命令 | 结果 |
| --- | --- |
| `bash client/core/build.sh` | exit 0；`libnetbird_core.so` 5,203,888 字节（ELF64/AArch64/DYN，14/14 BoringTun 符号、NAPI 导入面不变） |
| `cd client/core && cargo test --offline --locked` | **206 passed / 0 failed**（含新增 ice/stun 36 用例；既有测试零改动零降强度） |
| `bash client/build.sh` | exit 0；HAP 3,480,294 字节 |
| 探针交叉编译 | exit 0；依赖树 1 行（零新增 crate）；无需追加 NOTICES |
| `git status --short` | 仅 `client/core/**`（src/ice.rs、src/stun.rs、src/sys.rs、src/lib.rs、tests/ice_*.rs）+ `docs/n3-ice-notes.md`；未 commit/push |

## 七、N5b：ICE 会话层与连通性检查（追加）

覆盖 `client/core/src/ice_session.rs`、`tests/ice_session_{codec,e2e,state}.rs`
与 `src/ice.rs`/`src/stun.rs`/`src/lib.rs` 的小幅扩展（`pub(crate)`
`priority_for`/`seam_to_management`/`set_nonblock`/`decode_address` +
模块注册）。上文 §五「未完成项」的 **1（连通性检查）、2（prflx 子集 +
提名/选择）、6（agent 状态机）、7（MESSAGE-INTEGRITY/FINGERPRINT）** 在本
增量闭环；本节为完成记录。

### 7.1 上游侦察结论（逐行号，pinned `791401060d2b`）

| 事实 | 上游/规范来源 |
| --- | --- |
| credentials 生成：ufrag 16 / pwd 32 字符，字母表 `runesAlpha`（A-Za-z） | `client/internal/peer/ice/agent.go:114-123`（`GenerateICECredentials` = `randutil.GenerateCryptoRandomString`）、常量 `agent.go:16-18` |
| offer/answer payload = `"ufrag:pwd"`，解析要求恰好两段 | `shared/signal/client/client.go:74-101`（payload 拼接 L77）、`client.go:60-71`（`UnMarshalCredential`） |
| 长度约束：ufrag ≥ 4、pwd ≥ 22（上限取 256），字符集 `ice-char`（ALPHA/DIGIT/+//） | RFC 8445 §16、RFC 8839 §16（pion `NewAgent` 同款下限） |
| 检查包属性集合：USERNAME=`remote:local`、PRIORITY=本地候选 prflx 型优先级、ICE-CONTROLLING/CONTROLLED+tie-breaker(64bit 随机)、USE-CANDIDATE(仅提名)、MI、FP | RFC 8445 §7.1.2（分项 §7.1.2.1-§7.1.2.4）；tie-breaker 随机性 §5.1.3.1 |
| MI key = **对端**密码；收方用自己密码验请求、以自己密码答响应；请求方以对端密码验响应 | RFC 8445 §7.2.2 + RFC 5389 §10.1（短串凭证） |
| MI/FINGERPRINT 的「伪长度」规则：HMAC 输入长度指向 **MI 属性末尾（不含 FP）**；CRC 输入长度 = **完整线上长度（含 FP）** | RFC 5389 §15.4/§15.5，并以 **RFC 5769 §2.1/§2.2 向量逐字节实证**（§2.1 线上 0x58 / HMAC 0x50 / CRC 0x58；§2.2 CRC 0x3c）——本仓 codec 全对账（`tests/ice_session_codec.rs`） |
| pion 源码未在本地（go.mod `replace` 到 netbirdio/ice fork，机器无 Go module cache），检查属性语义以 RFC + RFC 5769 向量对账为准 | 本仓 `go.mod` 参考副本 L95/L338 |
| keepalive 4s / disconnected 6s / failed 6s（failed 从 disconnected 起再计 6s） | `agent.go:22-24`（常量）、`agent.go:61-63`（注入 agent） |
| 重传：RTO 初值 500ms、每事务 7 次后 pair Failed | RFC 5389 §7.2.1（RTO 默认）、RFC 5245 §16（`Rc` 默认 7）、RFC 8445 §7.2.5.2.3 |
| pair 状态机 Frozen/Waiting/InProgress/Succeeded/Failed；串行优先级序检查；pair 优先级 `2^32*MIN+2*MAX+(G>D)` | RFC 8445 §6.1.2.6、§6.1.2.3；触发检查 §7.3.1.4 |
| 提名 = regular nomination：controlling 对最佳 Succeeded pair 补发 USE-CANDIDATE 检查，响应成功即 selected；controlled 收到带 USE-CANDIDATE 的有效检查即提名 | RFC 8445 §8.1.1、§8.2、§7.3.1.5 |
| role 冲突：controlling 收 ICE-CONTROLLING → 己方 tie-breaker ≥ 对方则回 487 **并保留角色**，否则切换为 controlled；controlled 收 ICE-CONTROLLED → 己方 ≥ 对方则切换为 controlling，否则回 487 保留；收 487 必须换角色并**换 tie-breaker**，角色变化后重算 pair 优先级 | RFC 8445 §7.3.1.1、§7.2.5.1、§6.1.2.3 |
| prflx 最小子集：未知源地址的有效检查 → 以请求 PRIORITY 建 prflx 远端候选并配对触发 | RFC 8445 §7.3.1.3/§7.3.1.4 |
| 收发路径边界（本增量止于 selected pair）：dial 完成检查后 `GetSelectedCandidatePair()`；直连 ep = selected 对端地址；再 `ConfigureWGEndpoint` 落配 WG；上游 ICE socket 经 UDPMux 与业务共用 | `client/internal/peer/worker_ice.go:255-306`、`client/internal/peer/conn.go:444-461`、`conn.go:476-478`、`client/internal/engine_generic.go:15-16` |

### 7.2 依赖选择（探针结论：无新依赖）

- **方案①（引入 `sha1` crate）不可行**：`sha1` 不在项目 cargo registry
  cache（`ls $CARGO_HOME/registry/cache/*/ | grep sha1` → 空；同目录
  `hmac-0.12.1.crate`、`digest-0.10.7.crate` 在），`--offline --locked`
  无法解析——离线是硬要求，仓外交叉编译探针无须再做（依赖在取包一步即
  失败）。
- **方案②（全自写）被采纳**：SHA-1（RFC 3174，~50 行）+ HMAC-SHA1
  （RFC 2104，~20 行）+ CRC-32 IEEE 反射 0xEDB88320（~12 行，逐位实现）。
  `hmac` 0.12.1 虽在锁内但无 SHA-1 后端即不可用，而把手写核心接进
  `digest` trait 的代码量超过 SHA-1 本身。
- 代价与证据：零锁文件改动、零新 crate、零新 FFI/链接面（纯 core 代码；
  交叉编译面由 `client/core/build.sh` 步骤 4 的 aarch64-unknown-linux-ohos
  全量构建直接复验，无需独立探针）。正确性对账：RFC 2202 HMAC 向量 ×2、
  SHA-1("abc")/56 字节尾向量、CRC-32 校验值 `0xCBF43926`、RFC 5769
  §2.1/§2.2 完整报文 MI+FP 逐字节。`THIRD-PARTY-NOTICES.md` 无需追加。

### 7.3 实现要点

- **时钟注入**：模块内无墙钟；全部时序经 `run_once(now_ms)` 显式注入
  （poll 超时 0，纯非阻塞泵；调用方拥有循环与时间）。keepalive 节奏从
  selected 时刻起算；disconnected/failed 两段各 6s（§7.1）。
- **检查编解码**（`build_check_request`/`parse_stun`/
  `verify_message_integrity`/`verify_fingerprint` 公开，供 N5c 连接器与
  测试服务端复用）：属性 4 字节对齐零填充；FP 恒为末属性；401 错误响应
  故意不带 MI（请求未通过认证），487 带 MI（USERNAME 已匹配）。
- **状态机**：pair 形成即 Frozen（单组件单 check list，§6.1.1 的多列表
  thaw 不适用），`start()` 解冻为 Waiting；串行 pacing（同一时刻至多一个
  InProgress，按优先级取队首）——确定性优先，`Ta` 全速并行留待实测需要
  时再做。重传复用**同一事务 id**；7 次耗尽 pair Failed；全 pair Failed →
  会话 `Failed("all-pairs-failed")`。
- **role 切换**：清提名态、（487 路径）重抽 tie-breaker、按新角色重算
  pair 优先级重排序（§6.1.2.3 的 G/D 随角色互换），pair 状态跨重建保留。
- **受保护 socket（治理 §二.4）**：每本地候选一根长连 dup ——
  `take_fd → dup → O_NONBLOCK → bind(候选地址) → 收发`，`port 0` 由
  `getsockname` 回填候选；provider fd 号全程借用，只关自己的 dup；
  `stop()`/Drop 关闭。空 provider → `Network("protected-udp:
  no-protected-socket …")` fail-closed，模块内无任何 `socket(2)` 调用。
- **错误分类沿用六类**：套接字/熵失败=Network；候选串/凭证畸形=Parse/
  Request(status 0)；重传耗尽不作为返回错误（pair 状态 + Failed 事件）。
  检查流量中的恶意/畸形数据报**不是错误而是静默丢弃**（RFC 5389 FP 不符
  即弃），只能从状态机观测——这正是敌意数据报测试有意义的原因。

### 7.4 测试（26 新增用例，全仓 232 通过 / 0 失败）

| 文件 | 用例数 | 关键断言 |
| --- | --- | --- |
| `src/ice_session.rs` 内嵌 | 8 | RFC 2202 HMAC ×2、SHA-1 ×2（含 56 字节尾）、CRC-32 校验值、凭证生成（16/32/runesAlpha/两次必不同）、凭证校验拒绝表、built 检查往返 + 篡改 MI/FP 单字节各自翻转判定（reseal FP 隔离 MI 判定） |
| `tests/ice_session_codec.rs` | 7 | **RFC 5769 §2.1/§2.2 逐字节**：解析字段、MI/FP 全对账；篡改翻转；缺 MI/FP 拒绝；built 检查线格式（长度/对齐/USERNAME 序/角色属性）；成功与 487/401 响应往返；6 种敌意形状全 `stun-check:*` 不 panic |
| `tests/ice_session_e2e.rs` | 6 | **双 agent（真实 loopback socket，经受保护 fd provider）pair 双侧 Succeeded 且 selected pair 一致**（A.remote.port==B.local.port 互为镜像）；**双 controlling 角色冲突按 tie-breaker 收敛**（大者保留、小者切换、选择一致，用 generate() 凭证走真实 wire 路径）；错误 pwd 请求方向（401 → A 全 pair Failed、B 零提名零选择）与响应方向（对端伪造 valid-FP/wrong-MI 成功响应被拒、同事务 id 重传至耗尽）；坏 FINGERPRINT 请求静默丢弃而同一请求 FP 完好即被应答（响应 XOR-MAPPED/MI/FP 全验证）；未知事务 id 的完美签名响应仍被丢弃 |
| `tests/ice_session_state.rs` | 5 | keepalive：选中后 +4000ms 前零包、之后一包且为完整认证检查（经 provider 原 fd 线上验证 username/MI/FP）；注入时钟推进 → 恰在 +6s Disconnected、+12s Failed（此前绝不提前），断连期间 keepalive 持续；对端静默 → 恰 7 次尝试（**同一事务 id**）后 pair Failed → 会话 Failed 且无 Disconnected（无先验入站）；空 provider fail-closed（taken==0、无事件、start 拒绝、泵为 no-op）；pair 生命周期 Frozen→Waiting + relay/IPv6/重复远端不成对 |

约束遵守：测试主断言全部基于注入时钟与状态/线数据；唯一的真实等待是
e2e 坏 FP 用例对「无应答」这一负向事实的 300ms 有界 `recv` 超时；既有
232-26=206 用例零改动零降强度。

### 7.5 N5c 边界（本增量明确不做）

1. WG endpoint 落配：selected pair → `ConfigureWGEndpoint`
   （conn.go:476-478 等价物）与连接器泵循环。
2. TURN/relay 候选与 relay client；prflx 本地候选（§7.2.5.3.1 的
   XOR-MAPPED 不对称推导——环回/单 socket 拓扑不触发）。
3. IPv6、mDNS、ICE restart/重协商、session id 语义
   （worker_ice.go:69-74）。
4. 多 check list / 多组件、`Ta` 全速并行 pacing、64 连接级并发。

### 7.6 验收记录（N5b 实跑）

| 命令 | 结果 |
| --- | --- |
| `bash client/core/build.sh` | exit 0（产物大小见本轮验收输出） |
| `cd client/core && cargo test --offline --locked` | **232 passed / 0 failed**（新增 26） |
| `bash client/build.sh` | exit 0（HAP 大小见本轮验收输出） |
| 依赖探针 | 无新增依赖 → 无需（见 §7.2，sha1 不在 cache 的证据） |
| `git status --short` | 仅 `client/core/**`（src/ice_session.rs 新增、src/ice.rs、src/stun.rs、src/lib.rs、tests/ice_session_*.rs 新增）+ `docs/n3-ice-notes.md`；未 commit/push |
