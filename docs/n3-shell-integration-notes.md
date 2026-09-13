# N3-6 壳侧接线笔记（控制面 ↔ ArkTS VpnExtension）

范围：`client/core`（新增只读导出 `connector_network_config`）、`client/entry`（启动/停止流程接线）。不碰真机。

## NAPI 契约

权威定义：`client/core/src/lib.rs` NAPI surface 表 + `client/core/src/connector.rs`。
新增只读导出：`connector_network_config()` → JSON：

- 有网络图：`{available:true, serial, address, address_prefix_len, interface_dns, routes:[{network,is_default}], dns:{service_enable,servers:[{ip,port}]}, peer_count, peers:[{pub_key,allowed_ips}]}`（`is_default` 标记 `0.0.0.0/0`；peer 只含**公钥**与 allowed_ips 数量，无任何密钥材料）。
- 无网络图：`{"available":false,"reason":"no-network-map"}`（尚未收到 / stop 之后）。

## ArkTS 调用时序

- 启动（`NetBirdVpnExtensionAbility.onCreate`）：`createVpnConnection` → 读 filesDir 凭据 → `connector_start(configJson,{setup_key})` → 有界轮询（15 s×500 ms）`connector_network_config` → 快照可用则合并进 `VpnTunnelConfig`（`applyNetworkConfig`）→ `connection.create(VpnConfig)` → fd 合同核验 → fail-closed protect（不变）→ 启动 5 s 周期 watcher（`connector_status` + `connector_network_config`）。
- 凭据缺失 / `started:false`：记录稳定错误分类（`credentials-missing` / `start.error` token），**不创建** VPN。
- 停止（`onDestroy`）：置 destroyRequested → 停 watcher → `connector_stop()`（幂等）→ 一次性 destroy（fd 合同不变）。UI stop 走 `stopVpnExtensionAbility` → 系统回调 onDestroy。

## 路由/DNS 应用路径与"运行中变更"结论

- 应用路径：网络图快照在 `create()` **之前**合并进 VpnConfig（`NetBirdVpnConfig.ets:applyNetworkConfig`）：快照地址/32 前缀、managed routes（默认路由标 `isDefaultRoute`）、DNS servers（取 ip）→ `buildPlatformVpnConfig` 按已验证 MR4 形态产出（`isDefaultRoute`/`isExcludedRoute`/`interface:'vpn-tun'`）；基线配置中的 host-LAN 排除路由保留。
- **结论：不支持运行中改路由。** 平台 `VpnConfig` 在 `create()` 时固定，`VpnConnection` 无路由/DNS 更新 API。启动时快照可用则应用；之后的 serial 变化只记 `VPN_NETCFG_CHANGED_NOT_APPLIED` 日志，明确标注**未应用**，不假装支持。

## 凭据读取（开发期做法、非安全存储）

- `filesDir/netbird-setup-key`（纯文本 setup key）与 `filesDir/netbird-device-config.json`（`{management_url, private_key, ca_pem?, server_name?, hostname?}`）。文件名可经 want 参数 `setupKeyFile`/`deviceConfigFile` 覆盖（仅限 filesDir 下的名字）。缺失/空文件 → `credentials-missing` 明确报错。
- **声明：这是开发期做法，非安全存储。** 两个文件都是应用沙箱内未加密明文；仅为"密钥不进源码"存在。HarmonyOS asset/keystore 安全存储为后续增量。设备私钥/setup key 不进日志、不进错误文本。

## 权限与配置

- `module.json5` 无新增权限：读自身 filesDir 与 setTimeout/setInterval 均无需权限；网络仍只需既有 `ohos.permission.INTERNET`。

## 限制（当前增量）

- 无 signal/ICE ⇒ 无法发现 endpoint、无 peer 隧道；WG 侧仅本地登记（公钥+allowed_ips）。**Connected 仅指 management 控制面**，与 peer 连通无关。
- 真实 WG 数据面未接（tun 读写/保护后的 outer socket 未接转发），当前默认路由进隧道后无转发路径——设备上不要长时间开启。
- UI 状态直接轮询本进程 `connector_status`；若平台将 VpnExtension 分置于独立进程，UI 只能看到自身空快照（显示 `unavailable`/初始值），本增量未建跨进程状态通道。
- ~~connector 死亡（Failed/预算耗尽）时 watcher 仅记录，不自动拆除 VPN~~（N3-7 起改变：watcher 检测 `terminal` 主动拆除，见下方 N3-7 节）。

---

# N3-7 fail-closed 缺口修复（N3-7 节）

范围：`client/core`（新增 `mgmtsock` 模块 + connector/grpc/napi 扩展）、`client/entry`（启动时序 + 安全闸 + 死亡拆除）、本文档。不碰真机。治理依据：`docs/native-nx-governance.md` §二 第 2 条（fd 合同）与第 4 条（N2 判据：建连前逐 socket protect + protect 失败 fail-closed，枚举含 management，且覆盖**每次重连**；API 返回值不构成证据）。

## 上游语义出处（pinned commit `791401060d2b`，只引文件:行号）

- `client/net/protectsocket_android.go:22-46`：`ControlProtectSocket` 是 `Dialer.Control` 钩子——在**每次 dial 内、connect(2) 之前**对原始 socket fd 执行平台 protect；protector 未设置（:36-38）或平台返回失败（:40-42）时 Control 返回错误 → **dial 直接失败**（上游即 fail-closed，无"未保护直连"回退）。
- `client/net/dialer_init_android.go:4-6`（`Dialer.Control = ControlProtectSocket`）与 `client/net/listener_init_android.go:5-7`：所有出站 Dialer/Listener 一律挂 protect 钩子 ⇒ 逐 socket、逐 dial（含每次重连）。
- `shared/management/client/grpc.go:224-275`：`withMgmtStream` 的 retry 循环包住 **dial+login+stream 整体**，断流重连即重新 dial ⇒ 每次重连都重新过 `ControlProtectSocket`；:322-324（login PermissionDenied = `backoff.Permanent`）、:427-440（`handleSyncStream` 打开失败同样二分）。

## 缺口 1：管理连接自身 socket 过 protect 门

- **fd 合同如何被强制**（`client/core/src/mgmtsock.rs`）：fd 以**数字**跨界；native 每次 dial 用 `dup_socket_fd()`（`F_DUPFD_CLOEXEC` 优先，`dup()`+`F_SETFD(FD_CLOEXEC)` 回退，F_GETFD 前后探测——同 `tun.rs::TunFd` 习语）取得**副本**，只在副本上 `O_NONBLOCK`+`connect(2)`；**绝不 read/write/close 原始 fd**（原始 fd 仍归壳侧所有者可使用/关闭；测试 ① 断言 dup≠原号、dial 后原号仍 open）。已登记副作用：dup 共享 open-file description，`O_NONBLOCK` 会同时作用于原号——壳侧交出 fd 后不得再做阻塞 I/O（治理第 2 条登记义务）。management socket 是 `mgmt_socket_open()` 由 native 创建/预绑定（`{fd,bind_rc,bind_errno}`，wg_fwd_open 拆分模式的 TCP 版），从交付给核心队列起按"provider 所有"对待：native 只 dup 消费，原号的退役（消费后关闭）留给 N4+ 的 resupply 策略增量，本期不引入 native 关闭原号的路径。
- **ArkTS 时序**（`NetBirdVpnExtensionAbility.startConnectorThenCreate`）：解析 management_url → `connection.getAddressesByName` **壳侧 DNS**（5 s 有界 box；失败=`dns-failed` 中止）→ `core.mgmt_socket_open()`（失败中止）→ `VpnConnection.protect(fd)` 5 s box（失败/超时=`protect-fail-closed` 中止）→ `core.connector_start_with_socket(fd, configJson, creds, {"connect_addr":"ip:port"})`。任一步失败：**connector 不启动、VPN 不创建**。TODO(未验证)：create() 之前调用 protect 的设备侧行为属 N2b 物理门。
- **Rust 通道**：`ManagementGrpcClient::connect_with_socket_source` 用 `Endpoint::connect_with_connector` + 自定义 `Service<Uri>`（`mgmtsock::ProtectedSocketConnector`）让 gRPC 通道走该 socket；**每次 connector service 被调用（首连 + tonic/hyper 每次重连 + 会话续期连接）都重新 `take_fd()` 取一条新鲜受保护 socket**，队列空 → dial 失败（`no-protected-socket`），绝不回退未保护直连；TLS 仍按 endpoint URL host 做 SNI/证书校验（socket 只换传输，不换身份校验）。
- **fail-closed 拒绝 token**：`socket-fd-missing`（fd<0）/`socket-fd-invalid`（dup 探测失败）/`socket-addr-invalid`/`invalid-config`/`invalid-credentials`/`already-running`。
- **未 protect 直连的调试模式（显式 opt-in）**：`connector_start` 默认拒绝（`management-socket-required`）；仅当 configJson 显式 `"allow_unprotected_management": true` 才启动，并在 hilog 打出大写警示（非上游行为 + 引导环路风险）。生产 ArkTS 一律走 `connector_start_with_socket`。
- **重连 resupply 机制**：`connector_socket_feed(fd)` 让壳随时补入新鲜已保护 socket（FIFO）；本期壳在启动成功后补 1 条（`feedSpareProtectedSocket`），系统性补给策略留 N4+；队列饥饿 = dial fail-closed。

## 缺口 2：默认路由安全闸

- **判定在 Rust**（`connector.rs::ShellNetworkConfig::default_route_decision`，纯函数）：`force_default_route=false`（默认）时，仅当「已登记 peer 数 > 0 且 `WgPeerApplier::tunnel_ready()`（新 trait 方法，`WgPeerRegistry` 恒 false——无 signal/ICE 即无隧道，如实声明）」才允许；否则 HOLD。原因 token：`default-route-held:no-usable-peer` / `default-route-held:data-plane-not-ready` / `default-route-allowed:peers-registered-and-tunnel-ready` / `default-route-forced:debug-opt-in-black-hole-risk`。
- **执行方式**：HOLD 时快照的 `routes` 数组**直接不含 `0.0.0.0/0`**（只做映射的壳装不上不导出的路由），`connector_network_config()` 末尾附 `"default_route":{"allowed":bool,"reason":token}`。
- **壳侧映射**（`NetBirdVpnConfig.gateDefaultRoutes` + 扩展里 `VPN_DEFAULT_ROUTE_GATE` 日志）：对合并后的配置做最后一致性剥离（兜底 base 配置/空 routes 回退也走此闸），并打印 allowed/reason。
- **显式 opt-in**：configJson `"force_default_route": true`（want 参数 `forceDefaultRoute=true` 透传），命中时 hilog `VPN_DEFAULT_ROUTE_FORCED` 警示"数据面未通时装默认路由 = 流量黑洞"。默认不装——当前无 peer 连通，`0.0.0.0/0` 默认**不进** VpnConfig。

## 缺口 3：connector 死亡自动拆 VPN

- **Rust 侧语义**：`ConnectorStatus.terminal` = `!running && state==failed`（`is_terminal_state`）——worker **自行终止**（fatal auth / retry 预算耗尽）；用户 stop（`disconnected`）与运行/重连中**不是** terminal。JSON 字段 `"terminal":true|false`，单测+集成测试钉死。
- **壳侧触发条件与动作**（watcher，5 s 周期）：(a) `status.terminal === true` → 拆除；(b) `connector_status()` 原生调用**连续 3 次**失败（`CONNECTOR_POLL_FAILURE_LIMIT`）→ 判定 connector 不可达 → 拆除。动作：`VPN_CONNECTOR_DEAD|reason=…` 日志 → 停 watcher → `connector_stop()`（幂等）→ `destroyOnce("connector-dead:<reason>")`。
- **幂等性**：三层——`connectorDeathHandled` 一次性闩、`destroyOnce` 的 destroyPromise 闩（触发名只记第一次）、`connector_stop` 本身幂等。用户主动 onDestroy 仍是最先闩；death 路径不会与之竞争。

## NAPI 导出变化（契约表见 lib.rs）

新增 `mgmt_socket_open()`、`connector_start_with_socket(fd, configJson, setupKeyJson?, addrJson)`、`connector_socket_feed(fd)`；`connector_status()` 增 `terminal`；`connector_network_config()` 增 `default_route{allowed,reason}`；`connector_start` 默认拒绝（见上）。`Cargo.toml` 新增直接依赖 `hyper-util`（features `tokio`，`TokioIo` 适配 hyper 1.x rt traits）与 `tower-service`（tonic connector bound 所需 trait）——两者**均已在 Cargo.lock 中**（tonic 既有依赖，版本不变），离线 `--locked` 构建不受影响，仅 `netbird_core` 包条目新增两条依赖边。

## 测试证据（宿主可跑，`cargo test --offline --locked`）

- 缺口 1（`client/core/tests/mgmt_socket.rs`）：`dial_consumes_a_dup_original_stays_owner_owned`（dup≠原号、原号仍 open、一 dial 一 accept）、`grpc_channel_runs_only_over_the_protected_socket`（预连接 socket 被 adopt：server 恰好 1 次 accept，自拨会出第 2 次）、`failing_provider_fails_closed_to_terminal_error`（注入失败 provider → Failed + Network 类 + `terminal:true`）、`every_redial_takes_a_fresh_fd_matching_attempts`（传输层断连后 `taken == accepts`，逐 dial 重新保护）、`start_with_socket_refusal_gate_and_feed`（缺失/死 fd/坏地址拒绝 + feed 校验）。
- 缺口 2（`connector.rs` 单测）：`default_route_gate_*` 四例 + `default_route_exported_only_when_gate_allows` + 快照 JSON 契约（held 路由不出现在 routes）。
- 缺口 3：`terminal_semantics_pinned`（单测）+ `permission_denied_on_login_is_terminal`/`retry_budget_exhaustion_is_bounded_and_sanitized`（`terminal:true`）+ stop/运行态 `terminal:false` 断言。

## 限制与未完成（N4/N5 承接）

- **signal/relay/ICE/STUN/TURN、WG peer、DNS 外层 socket 的 protect 仍待 N4/N5**：本期只覆盖 management socket（§二.4 枚举的第一项）；其余外层 socket 尚不存在于本仓库。
- management socket 原号的消费后退役、以及重连期间的系统性 resupply 策略（何时 open+protect+feed）留 N4+；本期饥饿即 fail-closed。
- `protect()` 在 `create()` 之前对未建立 VPN 的 `VpnConnection` 是否可用：设备未验证（N2b 物理门）；失败即 abort，符合 fail-closed。
- `O_NONBLOCK` 对共享 OFD 的作用面已在文档与测试注释声明；`mgmt_socket_open` 的 TCP socket 原号在 connector 生命周期内不关闭（native 创建、native 队列持有，无泄漏放大路径）。
