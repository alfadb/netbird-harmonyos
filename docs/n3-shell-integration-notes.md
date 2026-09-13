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
- connector 死亡（Failed/预算耗尽）时 watcher 仅记录，不自动拆除 VPN（策略留后续增量）。
