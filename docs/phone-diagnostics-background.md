# 手机端诊断与后台 / VPN 生命周期说明（0.0.3 增量）

本次增量把手机端诊断模块落盘并纳入版本 0.0.3（`versionCode 3` / `versionName 0.0.3`），同时记录官方文档关于 VPN 后台生命周期与内存的事实口径。本文随后作为该模块的**长期字段契约**维护：0.0.6 增加了 §1.4 的 `/proc` 读数，以及 §1.8 / §1.10 的 `vpn_destroy_phase` 与 `native_resource_sample` 接线，相关小节已按 0.0.6 的当前代码与构建证据更新（版本口径见 `docs/v006-resource-diagnostics.md`）。

**状态：真机行为未验证。** 本文只描述模块当前实现的契约与官方文档事实，不声称任何保活效果，也不声称修复了 `SWAP_FULL` 或内存泄漏；这些必须等设备试验。

## 1. 接入的诊断模块

### 1.1 本地日志文件族

| 角色 | 文件族 | 轮转与上限 |
| --- | --- | --- |
| UI | `filesDir/netbird-ui.log`（`.1` / `.2` / `.3`） | 单文件 512 KiB × 4 个 = 总 2 MiB |
| VPN | `netbird-vpn.log*`（native `phone_log`） | 单段默认 2 MiB × 4 段 ≈ 总 8 MiB |

两族的约定相同：基名是当前文件，`.N` 越大越旧，下一次轮转丢弃最旧的一段。native 侧 `configure` 以 APPEND 打开当前段，不截断旧记录，新会话由 `sess=` 标记区分。

### 1.2 记录格式与边界

- 一条记录一行文本：`ts` / `pid` / `role` / `event` 加上白名单字段；字段按字典序输出，便于比对。
- 行长有界：UI 行 ≤ 512 字符（含换行），native 单条 ≤ 1024 字符（≤ 4 KiB UTF-8）；超长被截断并以 `~` 标记。
- 取不到的读数一律写显式 `na`，不会被伪造成 `0`；"系统未给出原因"与"原因是 X"在日志里可区分。

### 1.3 有界异步写入与限速

- 调用方只入队，不做磁盘 IO，也不被写线程阻塞；队列有界（UI 256 条，native 256 条），溢出计入 dropped 计数并报告一次（UI 侧 `event=diag_dropped`，native 侧 `dropped=` / `rate_suppressed`），不做静默丢弃。
- native 侧另有一秒准入预算限制写盘速率（当前默认 8192 字节/秒、64 条/秒），窗口无定时器、空闲即停，被拒的行计入计数后丢弃。
- **取值口径**：上述有界异步写入与限速已按模块常量实现并接入（native 侧默认 8192 字节/秒、64 条/秒，队列 256 条）；本文按模块当前默认值描述，真机实际行为仍未验证，取值不视为已冻结。

### 1.4 内存与资源采样（每 60 秒）

`start()` 立即采一次，之后每 60 s 一次，全部落在**同一条** `mem_sample` 记录里（字段按字典序输出）。采样是单飞的：上一次读数未落地时，到点的下一次 tick 被丢弃，不排队、不叠加定时器。

- `rss_kb` / `swap_kb`：写日志进程自身的 RSS 与 Swap（`hidebug.getRssInfo`，底层 `/proc/<pid>/status` 的 VmRSS / VmSwap）。
- `heap_kb`：自身 JS VM（ArkTS）堆已用量（`hidebug.getAppVMMemoryInfo`）；**只是 JS VM 堆，不是进程总内存**。
- `sys_avail_kb`：整机可用内存 MemAvailable（`hidebug.getSystemMemInfo`）。
- `cpu_ms`：本进程自启动以来的**原始累计** CPU 时间（`process.getPastCpuTime()`，ms）。只记原始计数器，不除以墙钟时间，因此不产出 CPU 占用率。
- `threads` / `vmsize_kb` / `rss_anon_kb` / `rss_file_kb`：本进程的线程数与虚拟 / 匿名 / 文件映射内存（`/proc/self/status` 的 Threads / VmSize / RssAnon / RssFile）。
- `sys_swap_total_kb` / `sys_swap_free_kb`：整机 Swap 总量与剩余量（`/proc/meminfo` 的 SwapTotal / SwapFree）；MemAvailable 仍由 `hidebug` 提供，不重复取。
- `fd_count` / `fd_capped`：本进程 `/proc/self/fd` 的**条目数**（只数个数，不读 fd 名称 / 目标 / inode）；列举上限 1024，`fd_capped=true` 表示计数到顶、该值只是下界。

**单位口径**：`*_kb` 后缀只是字段名约定。来自 `/proc` 的 `vmsize_kb` / `rss_anon_kb` / `rss_file_kb` / `sys_swap_*` 是内核的 kB（KiB 口径）；来自 `hidebug` 的 `rss_kb` / `swap_kb` / `heap_kb` / `sys_avail_kb` 的字节口径未在本日志内定义，只按字段名读作 KB——两组不要在换算意义上直接合并比较。

每项独立 try/catch：任一项在具体设备/版本上不可用即写显式 `na`（绝不写成 `0`），并在该次运行**只说一次** `DIAG_PROC_UNAVAILABLE`（不逐分钟刷 errno 日志）。新增读数只取 `/proc/self/*`（**从不传 pid**），单次读取上限 16 KiB，fd 在 `finally` 关闭。有意不用会读 `/proc/<pid>/smaps_rollup` 的 `getPss()` / `getAppNativeMemInfo()`——官方 SDK 明确建议不要在主线程调用它们；本轮也没有新增任何权限。观测量与观察方法见 `docs/v006-resource-diagnostics.md`。

### 1.5 系统事件（HiAppEvent）

订阅 `APP_CRASH` / `APP_FREEZE` / `RESOURCE_OVERLIMIT` / `APP_KILLED`，只读取白名单字段，整个 `params` 对象从不字符串化（`hilog[]`、`threads[]`、`frames`、异常文本、`external_log` 路径等一律不读不写）。

- `APP_KILLED` **必须先订阅**，且**异步投递：只能在下次启动时收到**——它描述的是已经发生的退出，进程被杀的那一刻无法由被杀进程自己写日志。
- 订阅只覆盖 `start()` 之后发生的事件，无法回溯订阅之前发生的查杀。
- 查杀原因始终可区分：行内始终带 `ev_kill_reason`（取退出详情，其次顶层 `reason`，系统未给出则为 `na`）。

### 1.6 导出（用户选择落点）

一个用户动作即可取走日志：经系统文件选择器（`DocumentViewPicker.save`）由用户选定落点，该 URI 自带临时读写授权，因此**不需要**在 `module.json5` 里申请宽泛存储权限。

- 只允许本应用自己的 8 个固定文件名（`netbird-ui.log[.1 .2 .3]`、`netbird-vpn.log[.1 .2 .3]`）；没有目录列举，也没有用户传入的路径。
- 上限：单文件 2 MiB、总计 12 MiB；被截断、被跳过、缺失与不可读都在导出文档里如实报告（含前言与逐文件头尾）。
- 复制是尽力而为的快照：写日志的进程同时仍在写，所以一次导出不保证是某一瞬间的完整拼图；导出不改名、不截断、不删除任何日志。

### 1.7 凭据与端点

- 设计上不读取配置、密钥、setup key 或状态文档，只读自己的日志与 marker 文件。
- 写入前对**已知形态**的凭据做尽力脱敏：标注型凭据字段（`setup_key`、`auth_token`、`jwt`、`password`、`psk` 等）、JWT 三元组、`Bearer <token>`、URL userinfo。
- 这是已知形态过滤，**不是"绝不含密"的保证**：未标注的凭据、未知的字段拼写、以及被长度上限截断的凭据不在覆盖范围内。
- 端点与路由信息被**有意保留**（host、`IP:port`、relay URL、`token_present=` 标志）——日志是给诊断用的，删掉这些等于删掉有用的一半。

### 1.8 诚实边界：突然被杀与生命周期 marker

- 一条记录 = 一次 `write`，**没有 fsync，也没有 ACK**。突然被杀（强杀 / 一键清理 / OOM）会丢失仍在队列里、尚未交给写线程的记录，以及未落盘的文件尾部；写错误后还可能留下半条被截断的记录，读取方需容忍。
- 生命周期 marker `netbird-diag-<role>.marker`：`start()` 写 `state=open`；`stop()` 写 `state=clean`，且只允许从真实的进程 / 扩展生命周期停止回调调用。
- 上次 marker 仍为 `open` 且 pid 不同时，报一次 `PRIOR_EXIT_UNCLEAN`——这是"上一轮没有走到 stop 回调"的**观测**，不是崩溃证据（强杀、OOM、回调丢失在这里看起来完全一样）。
- `state=clean` 只说明**我们的** stop 回调执行过，**不证明操作系统已经结束该进程**；marker 不能当作进程退出的凭据。
- 006 新增 `vpn_destroy_phase` 里程碑（`stage=destroy` 固定，`phase=requested|failed|settled`），**只观察 teardown 的进入 / 失败 / 落定**，字段仅 `phase`/`stage`。marker 的写法与判定**不变**：`state=clean` 仍只由收尾的 `stopDiagnostics()` 写，且 destroy 走 failed 分支时收尾照样调用它。因此上一轮实测的 `PRIOR_EXIT_UNCLEAN` 过度上报（干净 `onDestroy` 后下一会话仍报）**仍未修**。

### 1.9 宿主侧校验

纯逻辑核心（文件名与轮转顺序、记录与 marker 格式、字段白名单与脱敏、注入防护、行长边界、队列语义、导出白名单与上限）由宿主侧 selfcheck 校验，无需设备：`client/entry/selfcheck/run-diagnostics.mts`、`run-log-export.mts`、`run-system-events.mts`。

006 桥面另有 `client/entry/selfcheck/run-phone-log.mts`：它用假的原生文档跑**真实的**渲染器与记录构造器，钉住 `native_resource_sample` 与 `vpn_destroy_phase` 的事件名、字段白名单、两段 `stage`/`seq` 语义、22 个叶子一个不丢、以及每段都在行长上限内（不会被静默截断）。

### 1.10 接线状态

native `phone_log` 已注册 NAPI 导出（`phone_log_configure` / `append` / `status` / `shutdown`）；ArkTS 侧诊断、导出与系统事件模块已就位**并已接入应用入口**：

- UI 进程：`EntryAbility.onCreate` 调用 `startDiagnostics(filesDir, 'ui')` 与 `startSystemEvents('ui', ...)`，`onDestroy` 对应 `stop`。
- VPN 进程：`NetBirdVpnExtensionAbility.startDiagnosticsLogTee` 调用 `startDiagnostics(filesDir, 'vpn', ...)` 与 `startSystemEvents('vpn', ...)`。
- 日志导出：`Index.ets` 经 `NetBirdLogExport.exportDiagnostics` 触发（用户动作，落点由系统文件选择器给出）。
- VPN 进程 006 原生资源快照：`NetBirdVpnExtensionAbility.sampleNativeResourceStatus` 挂在**已有的** 60 s 状态节拍上（`samplePhoneLogStatus` 尾调用，无第二个定时器），经 `NetBirdConnector.connectorResourceStatus` / `nativeResourceSample` 读取原生 `connector_resource_status()`，每个 60 s 写**两条** `native_resource_sample`（`stage=connector|seq=1`、`stage=transport|seq=2`）；原生文档不可读时整份 `null` → 每个叶子写 `na`。

接线已在源码与构建产物中核实；**真机行为仍未验证**（见文首状态），本节不声称任何设备端效果。

006 构建证据（本轮实跑，非设备证据）：完整 native 构建 + HAP 打包 exit 0，pack.info 为 `versionCode=6 / versionName=0.0.6`、`apiVersion target=26 compatible=26`；`libs/arm64-v8a/libnetbird_core.so` 在未签名与已签名 HAP 中 sha256 一致（`bf708153a0ca92283fac7a3036d05ec7f687146a64c984dde696095969e56532`），且其中含 `connector_resource_status` 这一 NAPI **属性名**（不是 C 动态符号；005 打包的成员不含它，故必须完整重构建）。签名 8 参数 launcher exit 0、`verify-app` exit 0（Digest verify true）。**下一步部署要回答的仍是未知项**：扩展进程内 `/proc` 三个节点是否可读、真机上 `connector_resource_status()` 是否与宿主一致、以及系统查杀的真实成因——均未修复、未归因。

## 2. 官方口径下的 VPN 后台生命周期（2026-10-01 事实检索）

- **VPN 停止的两个触发条件**：① 调用 `startVpnExtensionAbility` 的应用进程退出；② VPN 服务进程销毁。仅销毁 UIAbility **不等于**进程退出（进程内还有组件时宿主进程常驻）。
- **长时任务不适用于 VPN 扩展**：Stage 模型下长时任务仅支持 UIAbility 申请，且长时任务类型表里**没有 VPN 一类**；并且长时任务不是"永生"——申请只提高被回收优先级，系统资源严重不足时仍会终止。
- **未发现三方 always-on API**：API 26 SDK 的 `@ohos.net.vpn.d.ts` 只有类型别名、没有任何函数，也没有 always-on 声明；系统设置里的"始终开启VPN"属于系统内置 VPN 客户端，不是三方应用可注册的能力。本文只说"本轮检索的官方文档与 SDK 中未发现"，**不断言普遍不可能**。
- **低内存 / SwapFull 只有一条路径**：官方建议一律是"降低应用自身内存占用，以减少被整机查杀策略选中的概率"，没有任何豁免或白名单机制；也因此本轮的着力点是**可测量的自身占用**，不是任何保活手段。
- **仍未取证**（留待设备试验）：VPN 扩展进程退后台是否适用普通应用的挂起策略；扩展是否支持独立进程 / 多实例；扩展进程被杀能否由 `APP_KILLED` / `LastExitReason` 感知；`hidebug` 与 `HiAppEvent` 在扩展进程内的可用性与沙箱归属；`DocumentViewPicker` 的权限模型。

## 3. 不声明的事项

1. 真机行为（华为 API 26 设备）在设备试验之前均属**未验证**；第 2 节全部结论只来自官方文档与本机 SDK 声明。
2. 本次**不声称修复 `SWAP_FULL`**，也**不声称修复内存泄漏**，更不声称任何保活效果。
3. 低内存查杀只能按官方建议通过降低可测量的自身占用去降低被选中概率；没有任何"防杀"承诺。

## 4. 主要官方来源（华为商用 HarmonyOS 文档）

- [应用退出](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/app-stop)
- [连接VPN（net-vpnextension）](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/net-vpnextension)
- [@ohos.net.vpnExtension API 参考（约束限制与附录）](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/js-apis-net-vpnextension)
- [长时任务类型](https://developer.huawei.com/consumer/cn/doc/HarmonyOS-Guides/continuous-task)
- [Background Tasks Kit 简介](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/background-task-overview)
- [应用终止事件介绍（APP_KILLED）](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/hiappevent-watcher-app-killed-events)
- [App Killed（应用终止）检测](https://developer.huawei.com/consumer/cn/doc/doccenter-capabilities/appkilled-guidelines)
- [应用被查杀类问题分析方法](https://developer.huawei.com/consumer/cn/doc/best-practices/bpta-stability-app-killed-way)
- [ohos.hidebug (HiDebug)](https://developer.huawei.com/consumer/cn/doc/doccenter-references/api/js-apis-hidebug)

事实检索报告（仓外，2026-10-01）：`/home/worker/n2h-kit/status-20261001/diagnostics-background-vpn-lifecycle-api26-huawei-openharmony.md`。

## 5. 相关文件

- `client/entry/src/main/ets/vpnextensionability/NetBirdDiagnostics.ets` / `NetBirdDiagnosticsCore.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdLogExport.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdSystemEvents.ets` / `NetBirdSystemEventsCore.ets`
- `client/core/src/phone_log.rs`
- `client/entry/selfcheck/run-diagnostics.mts` / `run-log-export.mts` / `run-system-events.mts`
