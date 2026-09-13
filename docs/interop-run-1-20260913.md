# Interop Run 1 — 真实 NetBird 服务端 × nbinterop 双实例联调（N10）

SPDX-License-Identifier: AGPL-3.0-or-later
Copyright (C) 2026 NetBird HarmonyOS contributors

日期：2026-09-13（本机）。结论先行：**管理面（login/Sync/signal 注册）对真实
netbird-server 0.78.1 全部打通；数据面未打通** —— signal outbound 帧
（OFFER/候选）未离开客户端进程，ICE/ WG/探针三步未验证。修复了一个本仓
实现 bug（glibc 主机上 `dlopen("libc.so")` 失败），全量测试绿。

---

## 1. 服务端制品（固化版本）

- 版本：**0.78.1**（上游 tag `v0.78.1`，goreleaser 从 `combined/` 构建，
  命令名 `netbird-server`，cobra root 显示 `combined`）。
- 获取方式：本机 docker/podman 不可用，改用 **registry API 匿名拉取 OCI
  镜像层**（`curl` + `tar`，无容器运行时）：
  - 仓库：`netbirdio/netbird-server`，tag `0.78.1`
    （`.goreleaser.yaml` L117-126/L417-427：build id `netbird-server`，
    images `netbirdio/netbird-server`、`ghcr.io/netbirdio/netbird-server`）。
  - manifest index digest：
    `sha256:3086534361a18573b85897383a0753a97b8c75d68dee9926fbd7eccab1f2fe89`
  - amd64 manifest digest：
    `sha256:3e52136c50aee28484eb77af23b8512d9dc885eac98980cacb7beab9866c8767`
  - 层 blob 逐一 sha256 校验（见 `dl/layers.txt` 三行 + `sha256sum` 输出，
    与 manifest 一致）。
  - 提取二进制：`rootfs/go/bin/netbird-server` → `bin/netbird-server`，
    **sha256 `02110cbb687c313cbae34661a7bde2173f5d4dcd2befdff84df447eaa4020cd9`**
    （ELF x86-64，动态链接，glibc 2.41 主机可运行）。
- 版本差异说明：协议参考 commit 为 main 分支 `791401060d2b95e5…`
  （2026-09-12），二进制为 2026-09-04 发布的 v0.78.1。两版本均含
  `combined` 单体服务端 + 内嵌 IdP（`combined/main.go`、
  `management/server/idp/embedded.go` 在 v0.78.1 已存在）；差异为 8 天的
  main 增量（如 agent-network 预算规则修复），与本联调涉及的
  management gRPC / signal / setup-key 路径无关。

## 2. 无 IdP 自举：成立（依据）

上游 v0.78.1 起 management **内嵌 Dex IdP（always enabled）**，peer 用
setup key 注册，**不需要任何外部 IdP（Zitadel）或 OIDC 交互**：

- `combined/config.yaml.example`（v0.78.1）L84-88：`auth.issuer` +
  `localAuthDisabled`，注释 "Embedded authentication/identity provider
  (Dex) configuration (always enabled)"。
- `management/server/idp/embedded.go` L43-45：issuer "used only for
  internal JWT validation (peers authenticate with setup keys / proxy
  tokens, not OIDC)"（同义注释见 e2e harness）。
- 无头 bootstrap 官方路径（上游 e2e harness 同款）：
  `management/server/http/handler.go` L70（v0.78.1）注册 bypass path
  `/api/setup`（免认证）；`handlers/instance/instance_handler.go` L60-86
  的 `setup()` 接受 `create_pat`，在 `NB_SETUP_PAT_ENABLED=true` 时返回
  首个 PAT（`personal_access_token`，`nbp_` 前缀，40 字符）。
- **setup key 创建**：PAT 作 `Authorization: Bearer` 调
  `POST /api/setup-keys`（handler：
  `management/server/http/handlers/setup_keys/setupkeys_handler.go`
  `AddEndpoints`，路由 `/setup-keys` POST；请求体
  `CreateSetupKeyRequest{name,type:"reusable",expires_in,usage_limit,
  auto_groups}`，见 `shared/management/http/api/types.gen.go` L2857-2877）。
  本联调创建了 `interop-key-a/b` 两把 reusable key（各 36 字符，24h 有效）。

## 3. 服务端运行与健康验证

- 数据目录（仓库外）：`/home/worker/netbird-interop/server/`，配置
  `config.yaml`（600），日志 `logs/server.log`。
- 启动：`NB_SETUP_PAT_ENABLED=true NB_DISABLE_GEOLOCATION=true
  bin/netbird-server --config …/config.yaml`（后台 job，PID 已记录）。
- 配置要点：单端口 `listenAddress ":18080"`（management+signal+relay
  多路复用），`exposedAddress http://127.0.0.1:18080`（无 TLS 配置 →
  明文 HTTP，同上游 e2e harness 形态），`stunPorts: []`（实际仍起在
  :3478，config.go L404 `len(stunPorts)>0` 语义），sqlite store，
  `auth.issuer http://127.0.0.1:18080/oauth2`。
- 健康验证（真实输出）：
  - `GET /api/instance` → `{"setup_required":true}`（引导前）→ 重启后
    `{"setup_required":false}`；
  - relay healthcheck `GET :19000/health` → `{"status":"unhealthy",…}`
    （本机无证书/外网形态，不影响管理面联调）；
  - 启动日志：`management server version 0.78.1`、
    `running HTTP server and gRPC server on the same port: [::]:18080`、
    `Signal server registered on port :18080`、
    `Relay server instance URL: rel://127.0.0.1:18080`。

## 4. 客户端准备

- 两个 peer 身份：`client/interop-{a,b}.json`（600；含
  `management_url http://127.0.0.1:18080`、内联 base64 私钥——CLI 仅支持
  内联私钥，无私钥路径字段；`hostname interop-a/b`）。公钥（状态输出可
  见，非秘密）：A `xHgwt1pN…gDFM=`、B `qkCtgzGR…qhU=`。
- setup key 经环境变量 `NETBIRD_SETUP_KEY` 注入（key 值只存在于 600 文件
  `client/setup-key-{a,b}.txt`，未进 argv/日志/仓库）。
- `selftest` 0.4s 全绿（含修复后新增 2 项）；`--dry-run` 解析配置 exit 0
  （配置无 setup key 时按设计 exit 4 提示走环境变量）。

## 5. 六项里程碑结果

| # | 里程碑 | 结果 | 证据（原始片段） |
| --- | --- | --- | --- |
| ① | login（信封加密） | **PASS** | `connector: login ok (peer address assigned)`；`state -> connected`；服务端 Sync debug：`common highest sync message version`（双向信封解密成立） |
| ② | Sync 网络图 | **PASS** | `[milestone] network-map applied (peers=1, signal='127.0.0.1:18080')`；status：`peer_count:1, route_count:0`；`N6_WG_DEVICE\|peers=1` |
| ③ | signal registered | **PASS** | 客户端：`[milestone] signal registered`；`signal.registered:true`；服务端 trace：`peer registered [qkCtgzGR…]`、`peer registered [xHgwt1pN…]`（两端各一条，流保持到超时才关闭） |
| ④ | ICE Connected + 一致选路 | **FAIL（未达）** | 两端 status 停在 `ice:{peers:1,idle:1,last_error:null}`；服务端 trace `received a new message to send` 计数 **0**（无任何信令帧到达服务器可转发） |
| ⑤ | WG handshake | **FAIL（未达）** | 依赖 ④；`wg.peers_with_session:0, handshakes:0` |
| ⑥ | 双向探针 | **FAIL（未达）** | 依赖 ④⑤；`tx_packets:0, rx_packets:0` |

## 6. 失败点与归因

**一个已修复的实现 bug + 一个已定位的开放问题。**

1. **已修复（本仓 bug，glibc 主机 ICE 全灭的直接原因）**：
   `src/ice.rs` 的 `SystemInterfaces::list()` 与 `resolve_ipv4()` 用
   `dlopen("libc.so")` 取 `getifaddrs`/`getaddrinfo` —— 这是 musl/OHOS
   的库名；glibc 主机上 `libc.so` 是 libc6-dev 的 linker-script 文本，
   `dlopen` 报 invalid ELF header 返回 NULL → 每次候选收集即失败
   （`Request{0}`），编排层每 2s（RETRY_COOLDOWN_MS）重试、日志仅
   `peer-conn: pump error (request)`、status `ice.last_error={request,0}`。
   证据链：round-2 双端各 ~20 条 pump error（40s/2s）；strace 显示零
   getifaddrs/netlink、候选收集前即失败。**修复**：新增
   `dlopen_libc()`（先 `libc.so` 后 `libc.so.6`），两处调用点替换；
   新增测试 `system_interfaces_list_works_on_this_host`、
   `resolve_ipv4_hostname_loads_libc_on_this_host`（tests/ice_gather.rs）。
   `cargo test --offline --locked` 22 个套件全绿。**设备路径不受影响**
   （OHOS/musl 首选名仍是 `libc.so`）。
2. **开放问题（我方 signal outbound 粘合层，阻塞 ④⑤⑥）**：修复后
   gather 正常（strace 可见 netlink getifaddrs、STUN binding 发往
   127.0.0.1:3478、add_local_candidate 绑定），但 **OFFER/候选帧从未
   到达服务端**：服务端 trace（logLevel=trace）`received a new message
   to send from peer` 计数 0；客户端 strace（writev/write/sendto 全 trace）
   无 signal TCP 上的 DATA 帧写出；且无任何错误面 —— 无
   `pump error`、无 `signal: undeliverable frame dropped`、无
   `Broken/Ended` 事件、`signal.reconnects:0`、worker 端
   `send_to_stream` 的全部分支（undeliverable/Broken/closed）都应留痕。
   即帧在 `RealSignalExchange → run_events_with_outbox → send_to_stream
   → tonic ConnectStream 请求体（mpsc 64）→ h2` 链条中被无声吞没，
   疑点集中在 tonic 请求体流未被 h2 发送侧继续消费（缓冲后不再写出）。
   服务端/环境已排除：register（同一条 ConnectStream）成功、management
   登录/Sync 正常、上游转发逻辑为标准路径。最小复现：本文档第 3-4 节 +
   第五轮命令（round-5 文件保留于 `/home/worker/netbird-interop/run/`）。
   现场保留：`run/nb-{a,b}.round{1..5}.{jsonl,log}`、strace
   （`/tmp/nb-b.strace`、`/tmp/nb-a.strace`、`/tmp/nb-a2.strace`）、
   服务端 trace 日志 `server/logs/server.log`。

## 7. 收尾

- 服务端已停止（job_kill + kill 确认 `no server process`），端口
  18080/19000/19090/TCP 与 3478/UDP 均已释放（ss 输出留档）。
- setup key 处置：两把 key 已用 API 发起 revoke（PUT `revoked:true`）；
  随后**服务端重启导致 setup-time PAT 失效**（首次启动 WARN：
  "DataStoreEncryptionKey generated … add it to your config file to
  persist across restarts"——未持久化时重启换钥，旧 PAT 解密失败 401），
  因此 API 撤销未能最终确认。残余风险封闭：key 仅对本机 127.0.0.1 实例
  有效、服务端已停机、且 2026-09-14T10:15Z（24h）自动过期；如需彻底
  废弃可删除 `/home/worker/netbird-interop/server/data/`。复现指引：
  重新起服务端 + `POST /api/setup`（create_pat=true）重领 PAT + 新建
  setup key，并在**首启前**把 `server.store.encryptionKey` 写进配置。
- 凭据纪律：所有秘密仅存于 `/home/worker/netbird-interop/` 下 600 文件
  （`server/admin.pat`、`server/setup-request.json`、
  `client/setup-key-{a,b}.txt`、`client/interop-{a,b}.json`）；argv/
  日志/报告/仓库均无秘密值。

## 8. 未验证项

- ICE 选路一致性、WG 双向握手、探针双向载荷（依赖第 6 节开放问题）。
- TLS/自签 CA 路径（`https://` management URL + `ca_pem` 注入）——本轮
  走明文 `http://` 形态（上游 e2e 同款）；服务端 TLS 尚未在宿主机直跑
  验证。
- relay（rels://）与 STUN srflx 实际连通性（本轮客户端按 N5a 边界不使用
  relay；STUN 服务端可达但未确认 XOR-MAPPED 回包被采信）。
- 会话续期（`session_expiry: disabled`，setup key 注册无 JWT 死线）、
  logout、网络图路由下发（route_count=0）。

## 9. 工作区改动（未 commit/push）

- `client/core/src/ice.rs`：+`dlopen_libc()` 回退，2 处调用点（+18/-2）。
- `client/core/tests/ice_gather.rs`：+2 host 回归测试（+22）。
- `docs/interop-run-1-20260913.md`：本文件（新建）。
- 其余产物均在仓库外 `/home/worker/netbird-interop/`。

---

# 第二轮（N10b）—— signal 出帧黑洞根因修复 + 数据面里程碑更新

日期：2026-09-13 晚（同一环境复用第一轮现场）。结论先行：

1. **第一轮第 6 节"开放问题"已定位并修复**：出帧从未到达服务端的根因
   是**本仓把 OFFER/候选推上了 `ConnectStream` 请求体，而 0.78.1 服务端
   从不读流**（详见 §N10b-1）。修复后信号面全通：服务端 trace 出现
   双向 `received a new message`（96 行）与 `forwarding a new message`
   （signal.go:162）。
2. **六项里程碑更新**：①②③ 维持 PASS；**④ ICE Connected + 一致选路
   本轮达成（PASS）**；⑤⑥ 未达，但归因已从"信号面黑洞"推进为**新的、
   证据完备的数据面边界问题**（WG 传输 socket ≠ ICE 选中的 socket，
   §N10b-4）。
3. 附带发现并修复两处小缺陷：网络图自身地址带 CIDR 后缀导致探针源解析
   失败；握手帧被丢后无重发机制（上游 handshaker 职责，§N10b-3）。

## N10b-1 根因（含关键代码路径与证据）

- **服务端行为（第一轮结论的修正）**：v0.78.1 `ConnectStream`
  （signal/server/signal.go:106-132）注册 + 回确认头后阻塞在
  `stream.Context().Done()`，**从不 `stream.Recv()`**；转发只发生在
  unary `Send`（signal.go:95-104 → forwardMessageToPeer L155-209）。
  第一轮"服务端 trace 接收计数 0"里，`received a new message...`
  实为 **Tracef 且只在 unary 路径**——流侧收到的帧连日志都不会有。
- **客户端行为（证据修正）**：第一轮"客户端 strace 无 DATA 帧写出"的
  观察不准确：`/tmp/nb-a2.strace`（18:46，修复 libc 后的定点复跑）
  显示 signal 连接 fd 28 上**有** 679 字节 DATA 帧（OFFER+候选，h2
  stream 1）。帧**离开了进程**，但服务端 gRPC 传输层 ACK 后应用层永不
  读取——与"零接收、零错误、reconnects=0、无 Broken"的全部观察吻合。
- **本仓缺陷路径**：`SignalSession::send_to_stream`（N4a
  `SendToStream` grpc.go:396-411 同型）把帧推入 mpsc(64) 请求体 →
  `OutboundStream` → h2。mock 测试全绿的原因：`tests/signal_mock` 按
  **旧版上游形态**实现了"读流并转发"的任务（signal_mock mod.rs 旧
  L402-416），真实服务端没有这条路径。
- **上游客户端对照**：signaler.go:36-66（OFFER/ANSWER/CANDIDATE/GO_IDLE）
  与心跳探针 grpc.go:555-566 全部走 unary `Send`；`SendToStream` 无引擎
  调用方。本仓实现把未用的上游兼容面当成了生产行为。

## N10b-2 修复（最小必要改动，全部在 `client/core/**`）

1. `src/signal.rs`：`SignalSession::send_outgoing` 改走 unary
   `SignalExchange/Send`（`SignalClient::send`，上游 signaler.go:36-66
   对应物）；删除 `send_to_stream` 及整条流侧发送机器（`OutboundStream`
   / mpsc(64) / `RegisteredStream.outbound`），`register()` 的请求体改为
   永不 yield 的 `PendingStream`——保持流打开（上游 Go 客户端同型）且
   **结构上杜绝**再向流体发帧。模块文档同步（"the server is a pure
   forwarder"小节）。
2. `tests/signal_mock/mod.rs`：mock 改为真实 0.78.1 形态——`ConnectStream`
   请求体**只计数不转发**（`stream_frames_seen`），转发只走 unary
   （`unary_sends` 计数）。既有信号测试套件在此形态下全绿即回归面。
3. `tests/signal_channel.rs`：两处流侧发送改为 unary（原断言强度不变，
   并新增 `stream_frames_seen()==0` / `unary_sends()>=3` 断言）。
4. `tests/signal_link.rs`：新增专门回归测试
   **`offer_reaches_sink_via_unary_send_while_server_never_reads_the_stream`**
   （服务端只收不回的保守场景：B 只有解密 sink、不发一帧；A 经完整
   编排→exchange→worker 链路发起）。**修复前失败证据**：还原 bug 版
   `signal.rs`（git HEAD）跑本套件——该测试在
   `signal_link.rs:555`（OFFER 必达断言）失败，且同套件 4 个双实例测试
   全部失败（converge 断言）；恢复修复后全绿。单测计数对账：修复前
   `signal_link` 套件 `1 passed; 4 failed`，修复后 `5 passed; 0 failed`。
5. `src/peer_conn.rs`：新增握手重发（`HANDSHAKE_RETRY_MS=3000`）——
   signal 服务端对未注册目的地的转发是 best-effort 丢弃
   （forwardMessageToPeer 的 not-connected 路径），首发 OFFER/候选可能
   在对端注册前被丢且无错误面；会话未 Connected 前按周期把已发帧
   （`signaled` 母本，去重）重新入队，Connected 即停。上游对应职责在
   handshaker（重发 OFFER 直到 ICE 成功）。回归钉子：
   `handshake_is_re_signaled_periodically_until_connected`。
6. 附带两处小修复：`src/host_sockets.rs` 网络图自身地址解析容忍 CIDR
   后缀（`"100.102.55.28/16"`，此前导致探针源 `own_addr` 永远缺失）+
   单测；`src/connector.rs` 快照 peers 增加 `vpn_addresses`（公开运行时
   材料），`src/bin/nbinterop.rs` 一次性打印本端/对端 VPN 地址与
   selected-pair 证据行。

**影响面**：device 路径（OHOS/musl）不受影响——改动全部在信号发送 seam
与 host 测试夹具；`send_to_stream` 的公开 API 移除已核对本仓无其他调用方
（唯一调用方是 `send_outgoing`）。既有测试强度未降低：mock 更严了
（流上帧只计数 = 真实服务端形态），原流侧发送断言改走 unary 后覆盖等价。

## N10b-3 六项里程碑更新结果（round-7 双实例，同环境）

| # | 里程碑 | 第一轮 | 第二轮（N10b） | 证据（原始片段） |
| --- | --- | --- | --- | --- |
| ① | login（信封加密） | PASS | **PASS** | `connector: login ok (peer address assigned)`；`state -> connected` |
| ② | Sync 网络图 | PASS | **PASS** | `network map applied serial=2 peers=1`；`own tunnel address [100, 102, 55, 28]; peer vpn addresses: ["100.102.1.90/32"]` |
| ③ | signal registered | PASS | **PASS** | `[milestone] signal registered`；服务端 `peer registered [xHgwt1pN…]`/`[qkCtgzGR…]` |
| ④ | ICE Connected + 一致选路 | FAIL | **PASS（新达成）** | 双端 `hilog\|N5_ICE\|selected-pair\|10.98.0.180:59554`（A→B 端点）/`…:41412`（B→A 端点）；`[milestone] ice connected peers = 1`（双端）；status 双端 `ice:{connected:1,endpoints_applied:1,reachable:1,last_error:null}` |
| ⑤ | WG handshake | FAIL | **FAIL（新归因，见 N10b-4）** | 双端 `handshakes:9/10`、`peers_with_session:0`、`rx_packets:0` |
| ⑥ | 双向探针 | FAIL | **FAIL（依赖 ⑤）** | `tx_packets:9/10, rx_packets:0` |

信号面服务端证据（round-7，logLevel=trace）：

```
19:44:11.404 TRAC signal/server/signal.go:96: received a new message to send from peer [qkCtgzGR…] to peer [xHgwt1pN…]
19:44:11.404 TRAC signal/server/signal.go:162: forwarding a new message from peer [qkCtgzGR…] to peer [xHgwt1pN…]
19:44:12.405 TRAC signal/server/signal.go:96:  received a new message to send from peer [xHgwt1pN…] to peer [qkCtgzGR…]
19:44:12.405 TRAC signal/server/signal.go:162:  forwarding a new message from peer [xHgwt1pN…] to peer [qkCtgzGR…]
```

（B→A 与 A→B 双向收+转；A→B 的首发在 B 注册前被 best-effort 丢弃，
由握手重发在 +3s 补达——N10b-2 第 5 条机制的实测生效。）

## N10b-4 ⑤⑥ 未达的新归因（证据完备，不蒙混）

- **现象**：ICE Connected 后，WG 侧双端各自发起握手 9/10 次，但
  `peers_with_session:0`、`rx_packets:0`（对端 WG 设备零接收）。
- **直接证据（round-7 双端日志）**：
  - A 的 WG 传输 socket：`N6_WG_DEVICE|adopt|local=0.0.0.0:49599`
  - B 的 WG 传输 socket：`N6_WG_DEVICE|adopt|local=0.0.0.0:46832`
  - A 落配的对端 WG endpoint：`10.98.0.180:41412`（= B 的 **ICE 候选
    socket**，非 46832）
  - B 落配的对端 WG endpoint：`10.98.0.180:59554`（= A 的 **ICE 候选
    socket**，非 49599）
- **归因**：本仓 N6 设计里 WG 设备持有**独立的受保护 UDP socket**，
  而 endpoint 落配用的是 ICE 选中候选（对端的 **ICE 检查 socket**）。
  WG 握手包被发到对端 ICE socket 上，被 ICE 会话当作非 STUN 包丢弃，
  永远到不了对端 WG socket——`rx=0`、会话永不建立。上游 netbird 的
  对应拓扑是 **WG 直接骑在 ICE 选中的连接上**（选中后把 ICE conn 作为
  WG transport，因此"endpoint=候选地址"天然成立）；本仓要在双端成立
  等价语义，需要"ICE 选中 socket 让渡/复用为 WG 传输"的架构增量
  （或对 ICE/WG 共享 socket 做 STUN/noise 分用），**超出 N10b 的
  最小修复范围**，按任务要求保留现场、如实归因，留待下一增量。
- ⑥ 双向探针依赖 ⑤ 的会话密钥，随之未达。

## N10b-5 验收（实跑输出）

1. `cargo test --offline --locked`（最终代码，连跑 3 次）：22 套件、
   **300 passed / 0 failed** ×3（新增 1 个信号回归 + 1 个重发回归 +
   1 个 CIDR 解析断言并入既有测试）。
2. `bash client/core/build.sh` → exit 0；`bash client/build.sh` → exit 0。
3. 回归测试证据见 §N10b-2 第 4 条（修复前 4 failed / 修复后全绿）。
4. 六项里程碑与 ⑤⑥ 新归因见 §N10b-3 / §N10b-4。

## N10b-6 现场与收尾

- 服务端以 `logLevel: "trace"` 重启（`config.yaml` 该键按文件内注释的
  诊断用法切回 trace），诊断结束后**已停止、端口已释放**：进程确认无
  `netbird-server`，`ss` 复验 TCP 18080/19000/19090 与 UDP 3478 均无
  监听（留档于本轮报告）。
- 第一轮现场全部保留；本轮新增产物：
  `run/nb-{a,b}.round6.{jsonl,log}`（修复前基线：信号收发已现、ICE
  因首轮 offer 丢失停在 idle/failed）、`run/nb-{a,b}.round7.{jsonl,log}`
  （④ 达成轮）、`server/logs/server-n10b.log`（trace 服务端日志）。
- 凭据纪律：第一轮 setup key 仍然有效（revoke 从未落地、24h 有效期至
  次日），本轮直接复用，未新建任何凭据；所有秘密仍在 600 文件内，
  未进 argv/日志正文/报告/仓库。
- `git status` 范围：`client/core/src/{signal.rs,peer_conn.rs,connector.rs,host_sockets.rs}`、
  `client/core/src/bin/nbinterop.rs`、
  `client/core/tests/{signal_mock/mod.rs,signal_channel.rs,signal_link.rs}`、
  `docs/interop-run-1-20260913.md`、`docs/n3-signal-notes.md`（未 commit/push）。

---

# 第三轮（N11）—— WG 骑上 ICE 选中连接，⑤⑥ 打通

日期：2026-09-13 深夜（同一环境，服务端 0.78.1 原库重启，`logLevel: trace`）。
结论先行：**N10b-4 归因的"WG 传输 socket ≠ ICE 选中 socket"已按上游架构
修复——⑤ WG handshake 与 ⑥ 双向探针本轮全部达成（PASS）**，六项里程碑
①–⑥ 首次全绿。侦察与实现细节见 `docs/n11-wg-over-ice-notes.md`（含上游
`791401060d2b` 全部 文件:行号 证据）。

## N11-1 上游侦察结论（方案 b：单 socket 共用 + 接收侧分用）

上游（userspace 路径）让 WG bind **拥有** UDP socket，把**同一 socket**
包成 `UniversalUDPMuxDefault` 交给 ICE agent（`iface/bind/ice_bind.go:
293-322` → `engine.go:657` → `engine_generic.go:14-16` → `peer/ice/
agent.go:57-58`）；接收侧在 WG 接收循环里按包类型分用：`isWireGuardMsg
(pkt) || !stun.IsMessage(pkt)` → WG，否则 STUN → mux（`ice_bind.go:313-345`，
WG 先判的 cookie 重叠防线 `ice_bind.go:403-421`）；direct 模式下 WG
endpoint = `RemoteConn.RemoteAddr()`（`conn.go:453-461`）+ `ConfigureWG
Endpoint`（`conn.go:476`）——对端地址即对端共用 socket；断开时
`RemoveEndpointAddress`（`conn.go:531`）fail-closed，恢复靠新协商
（`worker_ice.go:100-160` 按新 session-id recreate agent）。

## N11-2 本仓实现（最小改动，全部 `client/core/**`）

- `ice_session.rs`：逐字复刻上游分用谓词（`demux_routes_to_wg` =
  `isWireGuardMsg || !stun.IsMessage`），非 STUN 包入 `data_rx` 队列
  （有界 64、溢出计数）；新增 `selected_local_fd()` / `take_data_rx()`。
- `wg_device.rs`：per-peer **egress fd**（选中 pair 本地 socket 的 dup，
  `set_egress_socket`）+ `recycle_endpoint`（endpoint=None + 关 egress +
  清战役，= `RemoveEndpointAddress`）；全部发送路径优先走 peer egress。
- `connector.rs`：`WgPeerApplier` 新增 `attach_egress_socket`（默认
  Err，fail-closed）/ `handle_udp_inbound` / `recycle_endpoint`；
  `WgDeviceFeed` 完整实现并在设备重建时 **egress 先于 endpoint 重放**。
- `peer_conn.rs`：SelectedPair → **先 attach egress 再落配 endpoint**
  （attach 失败 ⇒ 不落配，结构性堵死 round-7 形态）；每拍把 `data_rx`
  喂给设备；Disconnected/Failed → 回收；Failed → 拆会话 + 冷却重发起；
  收到**不同凭证**的 Offer（Connected/Disconnected 状态）⇒ recreate 会话
  （上游新 session-id 语义的等价物，同会话重发同值幂等不误触发）。
- `nbinterop.rs`：新增 `[probe-recv]` ⑥ 载荷级证据（TUN stand-in 收到
  经隧道解封装的探针帧）。

fd 合同（§二.4）不变：native 只持 dup，原始号归 provider/壳侧；会话与
设备各关各的 dup；无任何未保护回退；`tunnel_ready` 语义、allowed_ips
路由、N3-7 默认路由闸均未改动。

## N11-3 六项里程碑更新（round-8/round-9 双实例，同环境）

| # | 里程碑 | 第二轮 | 第三轮（N11） | 证据（原始片段，round-9 `--verbose`） |
| --- | --- | --- | --- | --- |
| ①②③ | login/Sync/signal | PASS | **PASS（维持）** | `[milestone] signal registered`；服务端 trace `forwarding a new message` ×6（双向，signal.go:162） |
| ④ | ICE Connected + 一致选路 | PASS | **PASS（维持）** | A `N5_ICE\|selected-pair\|10.98.0.180:53169` + `N11_WG\|egress-attach\|local=…:55040`；B `selected-pair|…:55040` + `egress-attach|…:53169` —— **镜像闭合** |
| ⑤ | WG handshake | FAIL | **PASS（新达成）** | 双端 `N6_WG_DEVICE\|session-established`；status 双端 `wg:{ready:true, peers_with_session:1, handshakes:1, decrypt_errors:0}`（A tx=70/rx=84；B tx=80/rx=70，持续增长） |
| ⑥ | 双向探针 | FAIL | **PASS（新达成）** | A：`[probe-recv] 100.102.1.90 -> 100.102.55.28 len=43 payload="nbinterop-probe"`（×70）；B：`[probe-recv] 100.102.55.28 -> 100.102.1.90 … payload="nbinterop-probe"`（×69）—— 载荷级双向送达 |

**⑤⑥ 的关键新证据是"骑"本身**：A 的 egress local 地址 == B 落配的
selected 地址（`:55040`），B 的 egress == A 的 selected（`:53169`），
且 WG 设备不再使用自建 socket（round-7 的 `adopt|local=0.0.0.0:49599`
现在只是兜底，握手全部从选中路径出入——`unknown_peer_drops` /
`decrypt_errors` 全程为 0）。round-8（无 verbose）同样全绿：双端
`peers_with_session:1, handshakes:1, decrypt_errors:0`，探针双向送达。
B 末尾一条 `ice:disconnected` 是 A 先到 60s 超时退出后的预期静默遥测，
非中途故障（B 的 status 时间线显示断开前 14 个采样点持续
`connected:1, reachable:1`）。

## N11-4 验收（实跑输出）

1. `cargo test --offline --locked` 连跑 3 次：23 套件 **305 passed /
   0 failed ×3**（新增 N11 套件 3 项 + 分用单测 2 项；既有测试强度
   只增不减）。
2. `bash client/core/build.sh` → exit 0（ELF/AArch64/frozen symbols 全过）；
   `bash client/build.sh` → exit 0（HAP 打包校验通过）。
3. 新增测试（`tests/wg_over_ice_n11.rs`，全部离线、注入时钟）：
   `wg_handshake_and_probes_ride_the_ice_selected_socket`（闭环 + 三重
   "走选中 socket"证明 + 分用共存 + 闸联动）、
   `wg_from_a_non_selected_socket_never_establishes_a_session`（反例：
   round-7 形态复现——握手发出、落在 ICE socket、为 WG 形态、但
   永无会话）、`selected_connection_failure_recycles_fail_closed_then_
   renegotiates`（+6s Disconnected 回收 → +12s Failed 拆会话 → 新协商
   重建，闸全程 HOLD/恢复）。连跑 5 次稳定全绿。
4. **20 轮 stress（全量套件）**：`/tmp/n11-stress/run-01..20.log`，
   20/20 exit=0，累计 **6100 passed / 0 failed** —— 此前 14 轮未复现的
   一次性失败（161+1）未再现。
5. `git status` 范围：`client/core/src/{ice_session.rs,wg_device.rs,
   connector.rs,peer_conn.rs,bin/nbinterop.rs}`、`client/core/tests/
   {peer_conn_e2e.rs,signal_link.rs}`、`client/core/tests/
   wg_over_ice_n11.rs`（新）、`docs/n11-wg-over-ice-notes.md`（新）、
   本文件（未 commit/push）。

## N11-5 现场与收尾

- 服务端本轮以原配置重启（原 sqlite 库完好，`setup_required:false`，
  setup key 沿用第一轮凭据，未新建任何密钥）；诊断结束**已停止、端口
  已释放**（18080/19000/19090 TCP、3478 UDP `ss` 复验无监听）。
- 本轮新增现场产物：`run/nb-{a,b}.round8.{jsonl,log}`、
  `run/nb-{a,b}.round9.{jsonl,log}`、`server/logs/server-n11-boot.log`；
  第一/二轮现场全部保留。
- 凭据纪律：setup key 经环境变量注入（600 文件读取），argv/日志/报告/
  仓库均无秘密值。
