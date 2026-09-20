# N2-H 网络上下文快照探针（2026-09-20，diag-only）

## 事实起点（0020 诊断）

- device overlay 100.108.144.165，a2b 目标 100.108.126.29，host 逐 nonce 回执
  sameID 源 100.108.171.38（用户确认即 LAN 192.168.50.9，家宽 DMZ 暴露的
  home.alfadb.cn 主机）；device TUN 未观察 a2b；内核有 100.108/16 vpn-tun 路由但
  设备无 ip 工具。与并行 native socket SO_MARK 观测配套，本探针只补 App 侧网络
  元数据上下文。

## 官方 API（SDK 26.0.0.821 本机实读，非转述）

- `@ohos.net.connection.d.ts` 全部 Sync 形态逐字核对：`getDefaultNetSync(): NetHandle`
  （:182，since 9）、`getAppNetSync(): NetHandle`（:687，since 10）、
  `getAllNetsSync(): Array<NetHandle>`（:230，since 10）、`getNetCapabilitiesSync`
  （:349）、`getConnectionPropertiesSync`（:289）；除 getAppNetSync 外均挂
  `ohos.permission.GET_NETWORK_INFO`（已声明）。
- 字段：`NetHandle.netId`（:1680，0=无网络，有效值 ≥100）、`interfaceName`（:2106）、
  `.routes`（:2143）、`RouteInfo.destination: LinkAddress{address,prefixLength}`、
  `.gateway: NetAddress`、`.hasGateway`、`.isExcludedRoute?`（:2217，since 20）、
  `NetCapabilities.bearerTypes`（:1841）。本次**零权限变更**、零设置调用
  （无 bindSocket/setAppNet/路由修改），不读其它 UID 统计。

## 探针范围（本次落地）

- 扩展现有 `NetBirdInterfaceStatsProbe.ets`（少改动，复用其 errorCode 纪律与调用
  契约），新增导出 `runNetworkContextProbe(requestId)`：一次顺序读 default/app/all
  三个句柄 + 对 getAllNets 列表逐个读 bearerTypes 与 interfaceName/routes
  （destination/prefix/isExcludedRoute/gateway），按返回顺序、上限 8 个（超出记
  `truncated=true`，`nets=` 保留完整计数）。
- 接线点唯一：`maybeArmN2hCollector` 的 `answer.ok === true` 成功分支，紧跟 iface
  probe 之后 fire-and-forget，last-resort `.catch`（ABORTED 行只打 code/name）。
  仅诊断 ARM 成功触发；普通 VPN 不触发；全 Sync 函数体不给 arm 路径增加 await；
  无 timer/loop/重试。
- 日志：`N2H_NET_CONTEXT|requestId=…|defaultNetId=…|defaultNetError=…|appNetId=…|
  appNetError=…|nets=…|allError=…|truncated=…|netsDetail=…|startedUnixMs=…|
  finishedUnixMs=…|diag=true|is_evidence=false`（单行；netsDetail 内 net 段以 `;`
  分隔、路由以 `+`、字段以 `~`）；epoch-ms 一律 `%{public}s` 字符串，不走 `%d`。
- 诚实性：每个 API 独立 try/catch，失败只记 BusinessError code（无码记 `no-code`），
  该字段记 `unavailable`，绝不转 0、不影响其它独立读数；空句柄世界如实记
  `0/0/nets=0/detail=empty`；`isExcludedRoute` 未交付记 `absent`。不打印 payload /
  异常 message / 整对象 dump；不读 SSID/MAC/凭据（上述字段本就不含）。

## 解释边界（强调）

- **默认网络句柄与路由表存在 ≠ 本 socket 最终选路**：netId/interface/routes 是
  系统对该时刻网络拓扑的元数据视图，不证明 native 核心的 socket（或任何单条
  连接）走了哪条路径。无公开 SO_MARK 位布局，本输出不读取也不解码 SO_MARK；
  netId 至多是旁证，与 host 回执源（100.108.171.38/192.168.50.9）或 relay 行为的
  对应关系留给 reviewed run 文档，不得由本行直接推断（WG relay 不改内层 IP，
  source 变化不自动归因 relay，也不假设手机默认网关=.9）。
- 不改变现有 N2H 标记、核心状态 JSON、Evidence；本探针行 `is_evidence=false`。

## 自检与工具链证据

- `run-exclusion-generation.mts` 新增 NC1-NC5：种子值逐字透传（default/app netId、
  列表顺序、iface/bearer/routes 令牌、digit begin/end）；部分 API 拒绝有码且不影响
  独立读数；空世界如实；非 ARM 成功零触发 + 一次性语义继承；源码 pin（getter-only、
  无 bindSocket/setAppNet/setNet*、无 `.message`、调用点位于 ok 分支且在 iface
  probe 之后）。P8 import 字面同步更新。
- 全部 7 套自检真实运行通过：run 81 / run-exclusion-generation 117 /
  run-exclusion-addr-shape 25 / run-relay-exclusion 49 / run-relay-flag 9 /
  run-relay-status-log 3 / run-route-validation 49，共 333 passed / 0 failed。
- 两改动 ETS（NetBirdInterfaceStatsProbe / NetBirdVpnExtensionAbility）codelinter（6.0.240）0 告警。

## 构建状态

- 按协调，本任务未跑 HAP build（等待并行 core 改动合并后由合并验收构建统一执行）；
  无设备操作、无 commit。注意：任务期间一次并行 git 操作曾回滚本任务未提交改动（工作树
  回到 HEAD），已全部重做并复验；合并前请再次确认改动在位。
