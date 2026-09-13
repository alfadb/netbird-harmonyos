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
