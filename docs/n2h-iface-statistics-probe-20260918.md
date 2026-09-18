# N2-H iface bytes 可用性探针（2026-09-18，diag-only）

## 事实起点（0017 实测）

- 设备诊断 run 0017：native collector 首次 ARMED + 解码 exit 0，但 App 侧直接
  read `/proc/net/dev` 得到 Permission denied (os error 13)，`counters=[]`。proc
  直读路径在该沙箱内不可用，这一事实未变。
- 本探针只回答一个可用性问题：同一沙箱里，官方 `@ohos.net.statistics` 的 per-NIC
  字节 API 是否可用。它不是 C7 的替代声明，不属于设备证据 collector。

## 官方 API（SDK 26.0.0.821 实读，非猜测）

- 路径：`sdk/default/openharmony/ets/api/@ohos.net.statistics.d.ts`
- `getIfaceRxBytes(nic: string): Promise<number>`（d.ts:79）与
  `getIfaceTxBytes(nic: string): Promise<number>`（d.ts:113）。
- since 10；syscap `Communication.NetManager.Core`；两个 iface 方法**无 @permission、
  无 @systemapi**。`ohos.permission.GET_NETWORK_STATS` 只挂在 `getUidRxBytes/
  getUidTxBytes`（d.ts:264/287/310/334，since 26.0.0）上——那是 UID 维度（查询其他
  应用）的权限问题，与 iface 方法无关。本次**不新增任何权限**，module.json5 未改。
- 没有 public 的 per-iface packet API（`NetStatsInfo.rxPackets/txPackets` 仅随
  `getSelfTrafficStats`，按应用+网络类型，非按 NIC）。

## 探针范围（本次落地）

- 新文件 `client/entry/src/main/ets/vpnextensionability/NetBirdInterfaceStatsProbe.ets`：
  对 `vpn-tun` 读 RX/TX bytes；对固定负控名 `n2h-none0` 做负控——预期不存在，
  真机时用接口列表确认；如返回 0 仍不能宣称 vpn-tun 可用。该名在 IFNAMSIZ
  （15 字符）内，测的是"合法名称但无此接口"而非非法名称拒绝。四个
  Promise 逐个捕获 BusinessError code；每个接口样本记录 begin/end 时刻
  （`Date.now()`，epoch-ms 经 `%{public}s` 输出，不走 `%d`）。
- 接线点唯一：`maybeArmN2hCollector` 的 `answer.ok === true` 成功分支（ARMED 行之后）
  fire-and-forget 调用，带 last-resort `.catch`（ABORTED 行只打 code/name）。诊断
  ARM 成功才触发；普通 VPN 启动永不调用；不阻塞 ARM/native 窗口；无 timer/loop。
- 日志：`N2H_IFACE_BYTES_PROBE|requestId=…|interface=…|rxBytes=<n|unavailable>|
  txBytes=<n|unavailable>|rxError=<code|none|invalid-value>|txError=…|
  startedUnixMs=…|finishedUnixMs=…|diag=true|is_evidence=false`（每接口一行）。
  成功值必须显式 finite/nonnegative，错误不转 0；非有限/负值记 `unavailable` +
  `invalid-value`；未知接口若返回 0 如实记录 `rxBytes=0`，并**不**据此宣布 vpn-tun
  可用。不打印 payload / 异常 message。
- 自检：`run-exclusion-generation.mts` 新增 P1-P8（正常值/时间戳、逐向错误码、负控
  发生且如实、无 native ARM 成功不触发、arm 一次性语义继承、0 与垃圾值的区分、
  源码 pin）及 C8（同轮并行任务的 `peer_overlay_ip` 原样桥接：present 逐字转发 /
  absent 键不出现 / 错误类型不改造成缺省）。全部 7 套自检通过；两改动 ETS
  codelinter 0 告警。
- 登记一句：`peer_overlay_ip` 桥接为并行正控目标修复接线，不影响 stats（主要契约
  doc 由 core 任务补）。
- N2H 原日志字面、wg gate/latch、Evidence.counters/notes 全部未动；无 ARM 前后
  正式对账；无 byte→packet 换算。

## 字节/包数边界

- 本探针只产字节数。collector 的 Evidence 仍以包计数（C7 语义）为准，两者不可互相
  补齐：没有 public per-iface packet API，也不允许用 WG 自计包数伪造独立包数来源。
- 即使真机证明 iface bytes 可用，也只是给"信封侧独立读数"增加一个候选，不改变
  C7 的判定链。

## 真机对照仍待测（当前不执行）

- 待未来有设备通道时：同一时间窗内分别取（a）本探针的 `vpn-tun` RX/TX 与
  （b）`hdc shell cat /proc/net/dev` 的对应行，做两侧读数比较；差异解释留给当时的
  run 文档。本仓库当前不执行任何设备操作。

## 构建状态

- 按同轮协调，本任务未单独跑 HAP build（core n2h_collector.rs 由并行任务修改中）；
  unsigned HAP sha 由合并验收构建承担。自检与 codelinter 证据见上。
- 合并验收构建（2026-09-18）已统一执行：`bash client/build.sh` exit 0，ArkTS 编译通过；
  unsigned HAP `entry-default-unsigned.hap` 4234887 bytes，
  sha256 `c577dc6f0d82f4065f9479c5186513688cfdcfe6587f4833c12e335606ce07d5`；signed 未做。
