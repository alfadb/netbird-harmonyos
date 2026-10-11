# 手机端诊断与后台 / VPN 生命周期说明（0.0.3 增量）

本次增量把手机端诊断模块落盘并纳入版本 0.0.3（`versionCode 3` / `versionName 0.0.3`），同时记录官方文档关于 VPN 后台生命周期与内存的事实口径。本文随后作为该模块的**长期字段契约**维护：0.0.6 增加了 §1.4 的 `/proc` 读数，以及 §1.8 / §1.10 的 `vpn_destroy_phase` 与 `native_resource_sample` 接线，相关小节已按 0.0.6 的当前代码与构建证据更新（版本口径见 `docs/v006-resource-diagnostics.md`）。0.0.14（`versionCode 14`）继续扩展该字段契约：§1.4 新增 `/proc/self/status` 的 `vmdata_kb` / `vmstk_kb` / `rss_shmem_kb`、每 10 拍的独立 `mem_detail` 记录（PSS + Rust 堆）、以及两侧的 `ui_memory_level` / `vpn_memory_level` 内存压力里程碑。0.0.15（`versionCode 15`）在同一拍再补一条 `event=mem_vm`，把“匿名内存到底是谁的”拆成可归属读数：ArkTS VM 堆形状（`vm_total_heap_kb` / `vm_array_kb`）、native（libc malloc）堆三读数与 ArkTS GC 累计统计（`gc_*`），见 §1.4；三条记录都落在同一次 single-flight 采样里，1 s 节拍不变。0.0.17（`versionCode 17`）只改 §1.8 的 marker 语义：VPN 扩展在 `onDestroy` **入口**同步写一次 `state=stopping`，修掉真机实测的 `PRIOR_EXIT_UNCLEAN` 过度上报；`state=clean` 的写法与判定不变，字段契约其余部分不动。0.0.19（`versionCode 19`）新增 §1.11：**实验性**地借用系统 DATA_TRANSFER 长时任务做后台保活，并为此增设一条诊断记录 `ui_bg_task` 与设置页开关；既有字段与本契约其余部分一字未改。0.0.20（`versionCode 20`）在 §1.11 追加同一支线的行为增量：保活生效时允许**后台**自动重连（新 `trigger=background`，以及被拒时 `result=failed` 带平台错误码）、判据②放宽到"与 marker 同 pid 的终止事件即可"、尝试上限改为"连续失败 3 次"（活跃 ≥5 分钟才清零）、VPN 不活跃后 120 s 宽限期内保留长时任务；无新增诊断事件名，字段仍全在既有公开键集内。0.0.21（`versionCode 21`）在 §1.11 补上 0.0.20 那次挂起暴露出的两处缺口：挂起 / 激活回调按 SDK 声明的字段**完整**落一条 `ui_bg_task`（新增 `state` / `bytes` 两个既有公开键，`count` 在这两个 stage 上改为平台的任务 id），以及新增一条 `ui_resume_check`——进程从平台挂起里恢复时立刻复查一次隧道状态，不再等下一拍。

**状态：真机行为未验证。** 本文只描述模块当前实现的契约与官方文档事实，不声称任何保活效果，也不声称修复了 `SWAP_FULL` 或内存泄漏；这些必须等设备试验。

## 1. 接入的诊断模块

### 1.1 本地日志文件族

| 角色 | 文件族 | 轮转与上限 |
| --- | --- | --- |
| UI | `filesDir/netbird-ui.log`（`.1` / `.2` / `.3`） | 单文件 1 MiB × 4 个 = 总 4 MiB |
| VPN | `netbird-vpn.log*`（native `phone_log`） | 单段默认 24 MiB × 4 段 ≈ 总 96 MiB |

两族的约定相同：基名是当前文件，`.N` 越大越旧，下一次轮转丢弃最旧的一段。native 侧 `configure` 以 APPEND 打开当前段，不截断旧记录，新会话由 `sess=` 标记区分。两族合计（活动文件 + 3 个 archive）的磁盘日志预算为 100 MiB = UI 4 MiB + VPN 96 MiB。

### 1.2 记录格式与边界

- 一条记录一行文本：`ts` / `pid` / `role` / `event` 加上白名单字段；字段按字典序输出，便于比对。
- 行长有界：UI 行 ≤ 512 字符（含换行），native 单条 ≤ 1024 字符（≤ 4 KiB UTF-8）；超长被截断并以 `~` 标记。
- 取不到的读数一律写显式 `na`，不会被伪造成 `0`；"系统未给出原因"与"原因是 X"在日志里可区分。

### 1.3 有界异步写入与限速

- 调用方只入队，不做磁盘 IO，也不被写线程阻塞；队列有界（UI 256 条，native 256 条），溢出计入 dropped 计数并报告一次（UI 侧 `event=diag_dropped`，native 侧 `dropped=` / `rate_suppressed`），不做静默丢弃。
- native 侧另有一秒准入预算限制写盘速率（当前默认 8192 字节/秒、64 条/秒），窗口无定时器、空闲即停，被拒的行计入计数后丢弃。
- **取值口径**：上述有界异步写入与限速已按模块常量实现并接入（native 侧默认 8192 字节/秒、64 条/秒，队列 256 条）；本文按模块当前默认值描述，真机实际行为仍未验证，取值不视为已冻结。

### 1.4 内存与资源采样（VPN 每 1 秒 / UI 每 10 秒）

`start()` 立即采一次，之后按角色各自的节拍（VPN 1 s、UI 10 s：`SAMPLE_INTERVAL_MS` / `UI_SAMPLE_INTERVAL_MS`，由 `sampleIntervalFor` 按角色选择），全部落在**同一条** `mem_sample` 记录里（字段按字典序输出）。采样是单飞的：上一次读数未落地时，到点的下一次 tick 被丢弃，不排队、不叠加定时器——因此在忙碌时并非绝对每拍一次落盘，只是"到点就试、忙就跳过"。

- `rss_kb` / `swap_kb`：写日志进程自身的 RSS 与 Swap（`hidebug.getRssInfo`，底层 `/proc/<pid>/status` 的 VmRSS / VmSwap）。
- `heap_kb`：自身 JS VM（ArkTS）堆已用量（`hidebug.getAppVMMemoryInfo`）；**只是 JS VM 堆，不是进程总内存**。
- `sys_avail_kb`：整机可用内存 MemAvailable（`hidebug.getSystemMemInfo`）。
- `cpu_ms`：本进程自启动以来的**原始累计** CPU 时间（`process.getPastCpuTime()`，ms）。只记原始计数器，不除以墙钟时间，因此不产出 CPU 占用率。
- `threads` / `vmsize_kb` / `rss_anon_kb` / `rss_file_kb`：本进程的线程数与虚拟 / 匿名 / 文件映射内存（`/proc/self/status` 的 Threads / VmSize / RssAnon / RssFile）。
- `vmdata_kb` / `vmstk_kb` / `rss_shmem_kb`：本进程的数据段 / 栈 / 共享内存（同一次 `/proc/self/status` 读取的 VmData / VmStk / RssShmem）。这三个键与上面四个键共用**同一次**读取，不新增系统调用。
- `sys_swap_total_kb` / `sys_swap_free_kb`：整机 Swap 总量与剩余量（`/proc/meminfo` 的 SwapTotal / SwapFree）；MemAvailable 仍由 `hidebug` 提供，不重复取。
- `fd_count` / `fd_capped`：本进程 `/proc/self/fd` 的**条目数**（只数个数，不读 fd 名称 / 目标 / inode）；列举上限 1024，`fd_capped=true` 表示计数到顶、该值只是下界。

**每 10 拍一条 `mem_detail`（独立记录，不并入 `mem_sample`；同拍的 `mem_vm` 见下）；VPN 每 10 s、UI 每 100 s（节拍按拍计，不随角色间隔改变）。** 采样计数器每接受一拍加一，第 10、20、…… 拍在**同一次** single-flight `runSample` 内先写 `mem_sample`、紧接着写一条 `event=mem_detail`（0.0.15 起该拍再紧跟一条 `mem_vm`，见下）；其余各拍**完全不写** `mem_detail`（这些键整条缺席，而不是写 `na`——解析方以“有没有这条记录”区分）。之所以拆成独立行：加上 PSS 与 Rust 堆之后字段集放不进 512 字符行长，截断会静默丢掉尾部。该记录携带：

- `pss_kb` / `private_dirty_kb` / `shared_dirty_kb` / `private_clean_kb` / `shared_clean_kb`：本进程 `smaps_rollup` 的 Pss+SwapPss / Private_Dirty / Shared_Dirty / Private_Clean / Shared_Clean（`hidebug.getAppNativeMemInfoAsync()`，异步接口，自 API 20 起）。
- `rust_heap_kb` / `rust_heap_peak_kb` / `rust_allocs` / `rust_frees`：**仅本进程 Rust 全局分配器**的当前字节数 / 峰值 / 分配次数 / 释放次数（native NAPI `alloc_stats()`，JSON `{"bytes":n,"peak":n,"allocs":n,"frees":n}`；两个字节数除以 1024 向下取整成 KiB，两个次数原样）。它**只含 Rust 堆**——不含 ArkTS/JS 堆、libace_napi、C/C++ 平台缓冲，也不是进程 RSS。读取器由 `startDiagnostics` 的第 4 个可选参数注入：VPN 角色传入调用 `alloc_stats` 的包装（`NetBirdConnector.nativeAllocStats`；接口缺失或回复不符形状一律返回 `undefined`，不抛），UI 角色不传，因此 **UI 进程的这四个键恒为 `na`**。

取值纪律与 `mem_sample` 相同：读不到一律写显式 `na`，绝不写成 `0`；`rust_heap_kb` 为 0 只可能来自真实的“Rust 堆当前不足 1 KiB”。异步 PSS 结果迟到 / 失败既不打乱 single-flight、也不会写到别的拍：它在**所属那一拍**的 `runSample` 内被 await，期间到点的 tick 被丢弃；本拍该写的记录总是一起入队（`mem_sample` 在前，其后依次是同拍的 `mem_detail` 与 `mem_vm`）。详情读数的失败**每次运行只说一次** `DIAG_DETAIL_UNAVAILABLE`（沿用不逐次刷 errno 日志的写法），不阻塞本角色的采样节拍。

**同一拍第三条 `event=mem_vm`（0.0.15，紧跟 `mem_detail`，同一拍同时入队）。** 它专答“匿名内存是谁的”，用**同步**接口取数，全部落在上面同一次 single-flight 的详情分支里，**不新增定时器、不改本角色的采样节拍**：

- `vm_total_heap_kb` / `vm_array_kb`：本进程 ArkTS VM 的堆总量与**全部数组对象**占用（`hidebug.getAppVMMemoryInfo()` 的 `totalHeap` / `allArraySize`，API 26 声明为 KB）。`vm_array_kb` 是 ArkTS 数组 / ArrayBuffer 一类对象的口径，与 `heap_kb`（`heapUsed`）同源同单位。
- `native_heap_kb` / `native_heap_alloc_kb` / `native_heap_free_kb`：本进程**native 分配器（libc malloc，底层 mallinfo）**的总空间 / 已分配 / 空闲（`getNativeHeapSize` / `getNativeHeapAllocatedSize` / `getNativeHeapFreeSize`，声明为**字节**，这里按 ÷1024 向下取整成 KiB）。三者只覆盖 **libc malloc 堆**：不含 ArkTS/JS 堆、不含 libace_napi 与 C++ 平台缓冲，也不是进程 RSS。
- `gc_count` / `gc_time_ms` / `gc_alloc_kb` / `gc_freed_kb`：本进程（当前线程）ArkTS GC 的**累计**统计（`hidebug.getVMRuntimeStat(item)`）：GC 次数、GC 累计耗时（`ark.gc.gc-time`，声明单位就是**毫秒**，原样写）、累计分配字节数（`ark.gc.gc-bytes-allocated`，字节 ÷1024 向下取整）与累计释放字节数（`ark.gc.gc-bytes-freed`，同样换算）。四个 item 名是纯逻辑核心里的常量（`GC_STAT_*`），运行时模块里不出现平台字符串。

**为什么又拆一条行**：把上面两类字段合到一条 `mem_detail` 里，在各字段都取到自身类型上界时约 **695 字符**（pid=42），超过 512 字符行长；所以新 9 个键单独成行，绝不截断。宿主侧已把三条记录的长度上界一起钉住（见 §1.9）：最宽的 `mem_detail` 371 字符、最宽的 `mem_vm` 381 字符、两者合一必然溢出到 512 并被标记 `~`——这正是必须拆行的证据。

三条记录的分工不重叠：`mem_sample` 每拍给 `/proc` 与 hidebug 的粗读数，`mem_detail` 给 smaps_rollup（PSS / private / shared）与 Rust 全局分配器，`mem_vm` 给 ArkTS VM 堆形状、native malloc 堆与 ArkTS GC。任一项取不到即写 `na`（不写 `0`），三类失败共用“每次运行只说一次”的 `DIAG_DETAIL_UNAVAILABLE`，只是 `stage` 标签不同（`native-mem` / `rust-alloc` / `vm-heap` / `native-heap` / `gc-stat`）。

**单位口径**：`*_kb` 后缀只是字段名约定。来自 `/proc` 的 `vmsize_kb` / `rss_anon_kb` / `rss_file_kb` / `vmdata_kb` / `vmstk_kb` / `rss_shmem_kb` / `sys_swap_*` 是内核的 kB（KiB 口径）；来自 `hidebug` 的 `rss_kb` / `swap_kb` / `heap_kb` / `sys_avail_kb` / `pss_kb` / `private_*` / `shared_*` 的字节口径未在本日志内定义，只按字段名读作 KB——两组不要在换算意义上直接合并比较。0.0.15 的 `vm_total_heap_kb` / `vm_array_kb` 是平台声明的 KB；`native_heap_*` 与 `gc_alloc_kb` / `gc_freed_kb` 由平台声明的**字节**按 ÷1024 向下取整得来（因此 1023 B 记作 0 KiB，而不是被四舍五入成 1）；`gc_count` 是次数、`gc_time_ms` 是毫秒，二者原样记录。

每项独立 try/catch：任一项在具体设备/版本上不可用即写显式 `na`（绝不写成 `0`），并在该次运行**只说一次** `DIAG_PROC_UNAVAILABLE`（不逐次刷 errno 日志）。新增读数只取 `/proc/self/*`（**从不传 pid**），单次读取上限 16 KiB，fd 在 `finally` 关闭。PSS 只走**异步**的 `getAppNativeMemInfoAsync()`；会读 `/proc/<pid>/smaps_rollup` 的**同步** `getPss()` / `getAppNativeMemInfo()` 仍然有意不用（官方 SDK 明确建议不要在主线程调用它们），本轮也没有新增任何权限。观测量与观察方法见 `docs/v006-resource-diagnostics.md`。

**内存压力里程碑（0.0.14，只记录、不释放）。** UI 与 VPN 两侧各接一个平台回调 `onMemoryLevel(level: AbilityConstant.MemoryLevel)`，只在 `state` 字段记下该有界枚举（`stage=platform`），分别写 `event=ui_memory_level` / `event=vpn_memory_level`。**回调里不做任何释放内存的动作**（SDK 明确警告：在 `onMemoryLevel` 里释放 UI 组件可能阻塞当前进程主线程）。SDK 事实（已核对本机 API 26 声明）：`Ability` 声明了 `onMemoryLevel`，`ExtensionAbility extends Ability`，但 `@ohos.app.ability.VpnExtensionAbility.d.ts` 是**不带 extends 子句的独立类**、类体也没有该方法——因此 UI 侧（`UIAbility` → `Ability`）是真正的生命周期 override，VPN 侧只是“带精确签名的新增方法”，框架是否真把该回调投递进 VPN 扩展进程**未在真机验证**。

### 1.5 系统事件（HiAppEvent）

订阅 `APP_CRASH` / `APP_FREEZE` / `RESOURCE_OVERLIMIT` / `APP_KILLED`，只读取白名单字段，整个 `params` 对象从不字符串化（`hilog[]`、`threads[]`、`frames`、异常文本、`external_log` 路径等一律不读不写）。

- `APP_KILLED` **必须先订阅**，且**异步投递：只能在下次启动时收到**——它描述的是已经发生的退出，进程被杀的那一刻无法由被杀进程自己写日志。
- 订阅只覆盖 `start()` 之后发生的事件，无法回溯订阅之前发生的查杀。
- 查杀原因始终可区分：行内始终带 `ev_kill_reason`（取退出详情，其次顶层 `reason`，系统未给出则为 `na`）。

### 1.6 导出（用户选择落点）

一个用户动作即可取走日志：经系统文件选择器（`DocumentViewPicker.save`）由用户选定落点，该 URI 自带临时读写授权，因此**不需要**在 `module.json5` 里申请宽泛存储权限。

- 只允许本应用自己的 8 个固定文件名（`netbird-ui.log[.1 .2 .3]`、`netbird-vpn.log[.1 .2 .3]`）；没有目录列举，也没有用户传入的路径。
- 上限：单文件 25 MiB、总计 104 MiB；这是**复制上限**（copy cap），不是日志存储量——见 §1.1 的 100 MiB 磁盘预算，104 MiB 覆盖该预算加尾部记录余量；被截断、被跳过、缺失与不可读都在导出文档里如实报告（含前言与逐文件头尾）。
- 复制是尽力而为的快照：写日志的进程同时仍在写，所以一次导出不保证是某一瞬间的完整拼图；导出不改名、不截断、不删除任何日志。

### 1.7 凭据与端点

- 设计上不读取配置、密钥、setup key 或状态文档，只读自己的日志与 marker 文件。
- 写入前对**已知形态**的凭据做尽力脱敏：标注型凭据字段（`setup_key`、`auth_token`、`jwt`、`password`、`psk` 等）、JWT 三元组、`Bearer <token>`、URL userinfo。
- 这是已知形态过滤，**不是"绝不含密"的保证**：未标注的凭据、未知的字段拼写、以及被长度上限截断的凭据不在覆盖范围内。
- 端点与路由信息被**有意保留**（host、`IP:port`、relay URL、`token_present=` 标志）——日志是给诊断用的，删掉这些等于删掉有用的一半。

### 1.8 诚实边界：突然被杀与生命周期 marker

- 一条记录 = 一次 `write`，**没有 fsync，也没有 ACK**。突然被杀（强杀 / 一键清理 / OOM）会丢失仍在队列里、尚未交给写线程的记录，以及未落盘的文件尾部；写错误后还可能留下半条被截断的记录，读取方需容忍。
- 生命周期 marker `netbird-diag-<role>.marker`：`start()` 写 `state=open`；`stop()` 写 `state=clean`，且只允许从真实的进程 / 扩展生命周期停止回调调用。
- 0.0.17 起，VPN 扩展的 `onDestroy` **入口第一句**（`writeStopMarkerSync`，在任何可能抛错 / 耗时的调用之前）用**同步**文件 API 再写一次 marker 为 `state=stopping`：真机上 `onDestroy` 返回后 JS 上下文约 3 ms 就被销毁、进程 21 ms 内退出，`destroyOnce` 的 continuation（`await createPromise` → `await connection.destroy()` → 收尾 `stopDiagnostics()`）根本跑不到（V016：`netbird-vpn.log:1528-1547` 停在 `vpn_destroy_phase phase=requested`，没有 `settled`），于是 marker 留在 `open`、下一会话误报 `PRIOR_EXIT_UNCLEAN`。**同步写只在这一个文件里**（`NetBirdVpnExtensionAbility.ets`）：诊断运行时 `NetBirdDiagnostics.ets` 不得出现任何同步文件 API（`run-diagnostics.mts` 对此有源码断言）。写入是 fail-soft 的：固定且在白名单内的一个路径、没有 fsync（单行；若被截断，读到的是 marker 的“未知状态”，不会变成任何判定），失败只落一条 W 级日志（每进程一次）。本进程从未走到 `startDiagnosticsLogTee()`（即从未写自己的 `open`、并不拥有这个 marker 文件）时保守跳过，以免覆盖上一会话真实的 `open`。
- `state=clean` 仍只由收尾的 `stopDiagnostics()` 写；`state=stopping` 表示“有序停止已进入，只是最后一次异步 `clean` 转换随进程一起丢了”，判定上与 `clean` 同类（不自动重连）。**因此 `PRIOR_EXIT_UNCLEAN` 的过度上报（干净 `onDestroy` 后下一会话仍报）已修复**：只有 `state=open`（且 pid 不同）才被当作上一轮异常退出的线索——系统强杀 / OOM 路径不经过 `onDestroy`，所以仍留 `open`（v015 的 4 个被系统回收进程 0 条 `vpn_lifecycle_stop`）。
- 上次 marker 仍为 `open` 且 pid 不同时，报一次 `PRIOR_EXIT_UNCLEAN`——这是"上一轮没有走到 stop 回调"的**观测**，不是崩溃证据（强杀、OOM、回调丢失在这里看起来完全一样）。
- `state=clean` 只说明**我们的** stop 回调执行过，**不证明操作系统已经结束该进程**；marker 不能当作进程退出的凭据。
- 006 新增 `vpn_destroy_phase` 里程碑（`stage=destroy` 固定，`phase=requested|failed|settled`），**只观察 teardown 的进入 / 失败 / 落定**，字段仅 `phase`/`stage`。marker 的写法与判定：`state=clean` 仍只由收尾的 `stopDiagnostics()` 写，且 destroy 走 failed 分支时收尾照样调用它；0.0.17 只**新增**了入口那一次同步 `state=stopping`，`clean` 的写法与上表判定不变。

### 1.9 宿主侧校验

纯逻辑核心（文件名与轮转顺序、记录与 marker 格式、字段白名单与脱敏、注入防护、行长边界、队列语义、导出白名单与上限）由宿主侧 selfcheck 校验，无需设备：`client/entry/selfcheck/run-diagnostics.mts`、`run-log-export.mts`、`run-system-events.mts`。

006 桥面另有 `client/entry/selfcheck/run-phone-log.mts`：它用假的原生文档跑**真实的**渲染器与记录构造器，钉住 `native_resource_sample` 与 `vpn_destroy_phase` 的事件名、字段白名单、两段 `stage`/`seq` 语义、22 个叶子一个不丢、以及每段都在行长上限内（不会被静默截断）。

0.0.14 的增量同样由宿主侧钉住：`run-diagnostics.mts` 校验 `vmdata_kb` / `vmstk_kb` / `rss_shmem_kb` 键表与满样本行长、`mem_detail` 的第 10 拍节奏（前 9 拍与 mem_sample 都不带详情键）、异步 PSS 与 Rust 读取器的解析 / `na` 规则、挂起的 PSS 不改 single-flight 且不错拍、UI 角色四个 rust 键为 `na`、以及“详情失败每次运行只说一次”；`run-phone-log.mts` 校验 `nativeAllocStats()` 的形状检查与 `undefined`（不抛）分支，以及 VPN 侧把该读取器作为 `startDiagnostics` 第 4 个参数注入、`vpn_memory_level` 里程碑的字面量与“不释放内存”。

0.0.15 的 `mem_vm` 同样落在宿主侧（同一 selfcheck，无需设备）：`run-diagnostics.mts` 钉住 `GC_STAT_*` 四个 item 名与 SDK 声明一致、`bytesToKiBText` 的精确整除（含 u64 最大值与“1023 B 记 0 KiB”）、`gc_*` 非有限 / 负值一律 `na`、`mem_vm` 的 9 键键表与逐字段取值、三条记录的长度上界（`mem_vm` 381 字符最宽；两套字段合一必然溢出 512，故必须拆行）、第 10 拍按 `mem_sample` → `mem_detail` → `mem_vm` 的顺序各出一条而其余各拍一条不出、三条记录字段互不串行、`native-heap` / `gc-stat` / `vm-heap` 三条腿各自 `na` 且三类失败合起来仍只报一次、挂起的 PSS 不改 single-flight、以及运行时模块里不出现任何平台 GC item 字符串、`setInterval` 仍只有一处（不新增定时器）。

0.0.19 的后台保活由 `run-keep-alive.mts` 钉住（三半，全部无设备、无构建）：**A** 把 `NetBirdKeepAlivePolicy.ets` **逐字节**复制成 `.ts` 后直接 import，在毫秒边界上跑完整裁决机（连接→申请；断开/注销→撤；用户划通知→本会话不再申请、下次手动连接清除；低速率/未更新→退避递增与 5 min 上限、5 次后停；`USE_ILLEGALLY`→停且手动重连不清除；开关关闭→停；无活跃隧道→撤；刷新节流 30 s；实况窗正文与"读不到写 未知"）。0.0.20 在同一处补上宽限：不活跃后 120 s 内保持（`grace`）、超过即撤（`vpn-inactive`）、重连已放弃即撤（`reconnect-gave-up`）、用户断开/注销/开关关一律绕过宽限立即撤、以及"正在重新连接…"正文。**B** 用转发到真实 node fs 的 `@ohos.file.fs` 功能桩（`keep-alive-hooks.mjs`）驱动 `NetBirdKeepAliveStore.ets` 的真实原子写入：缺失 / 损坏 / 版本不符 / 类型不符 / 超长一律回落到默认 true，往返一致，短写与 IO 错误被拒且**旧文档原样保留**，被访问过的路径只有它自己那一个文档（及其临时兄弟）。**C** 源码切片断言：适配层是全仓唯一 import `backgroundTaskManager` / `notificationManager` / `wantAgent` 的模块；申请前先 `getAllContinuousTasks` 认领；用返回 notificationId 的数组重载且**从不** `updateBackgroundRunning`；页面仍只有一个 `setInterval`、唯一一处 `startVpnExtensionAbility`；`EntryAbility` 在 `onCreate` 注册、`onDestroy` 注销并释放；`module.json5` 的 `backgroundModes` 与权限；两处版本号为 21 / 0.0.21（0.0.19 起这条断言随版本一起改）。0.0.20 的自动重连增量由 `run-reconnect-policy.mts` 钉住（同一"逐字节复制纯模块 + 源码切片"两半）：后台 trigger 与 `keepAliveActive` 只开前台门、判据②放宽的 pid 比较、连续失败 3 次与活跃 ≥5 分钟清零、被拒发起的失败记录。

0.0.21 的这两项同样落在宿主侧。挂起 / 激活回调**不再只做源码切片**：`run-keep-alive.mts` 的 **D** 把 `NetBirdKeepAliveService.ets` 逐字节复制成 `.ts`，配上逐字节复制的纯策略与一个记录用的 `./NetBirdDiagnostics` 接缝，再用平台桩**保留的回调对象**驱动**真实的**挂起 / 激活处理函数——`{continuousTaskId:7, suspendState:true, suspendReason:12, suspendMessage:{message:'x',reason:12}}` 必须落成恰好一条 `ui_bg_task{stage=suspend, result=ok, state=1, reason=12, count=7, bytes=1}`、消息原文只出现在 W 级 hilog 里；`reason=-1` 原样保留、缺字段写 `na`、`suspendState=false` 也按"重新活跃"处理、激活回调触发一次恢复信号、监听者抛错不外溢、没有监听者时是空操作、不属于本应用的任务 id 被丢弃。恢复复查的阈值与"只触发一次"由 **A** 半的纯函数在毫秒边界上钉住（`keepAlivePollGapMs` 的 0 / 时钟回跳 / 15000 与 15001 边界、`7362651 ms` 判为恢复、写完本拍时刻后下一拍不再触发），页面侧的接线（单飞 `pollRunning` + `try/finally`、信号入口、`ui_resume_check` 的 `reason` / `count`、`onForeground` 走同一条通道、仍然只有一个定时器）由 **C** 半的源码切片钉住。

### 1.10 接线状态

native `phone_log` 已注册 NAPI 导出（`phone_log_configure` / `append` / `status` / `shutdown`）；ArkTS 侧诊断、导出与系统事件模块已就位**并已接入应用入口**：

- UI 进程：`EntryAbility.onCreate` 调用 `startDiagnostics(filesDir, 'ui')` 与 `startSystemEvents('ui', ...)`，`onDestroy` 对应 `stop`。
- VPN 进程：`NetBirdVpnExtensionAbility.startDiagnosticsLogTee` 调用 `startDiagnostics(filesDir, 'vpn', ...)` 与 `startSystemEvents('vpn', ...)`；0.0.14 起第 4 个参数传入 `() => nativeAllocStats()`（`alloc_stats` 的 Rust 堆读取器），UI 进程不传该参数。
- 内存压力：`EntryAbility.onMemoryLevel` 写 `ui_memory_level`，`NetBirdVpnExtensionAbility.onMemoryLevel` 写 `vpn_memory_level`（`stage=platform`，`state` 为 `AbilityConstant.MemoryLevel` 的数值）；两侧都只记录，不释放内存。
- 日志导出：`Index.ets` 经 `NetBirdLogExport.exportDiagnostics` 触发（用户动作，落点由系统文件选择器给出）。
- VPN 进程 006 原生资源快照：`NetBirdVpnExtensionAbility.sampleNativeResourceStatus` 挂在**已有的** 60 s 状态节拍上（`samplePhoneLogStatus` 尾调用，无第二个定时器），经 `NetBirdConnector.connectorResourceStatus` / `nativeResourceSample` 读取原生 `connector_resource_status()`，每个 60 s 写**两条** `native_resource_sample`（`stage=connector|seq=1`、`stage=transport|seq=2`）；原生文档不可读时整份 `null` → 每个叶子写 `na`。
- 后台保活（0.0.19 实验）：`EntryAbility.onCreate` 在诊断写入器打开之后调用 `registerKeepAlive(this.context)` 注册三个长时任务回调，`onDestroy` 先 `unregisterKeepAlive()` 再尽力 `stopKeepAlive(...)`；开关值、裁决与平台调用分别落在 `NetBirdKeepAliveStore` / `NetBirdKeepAlivePolicy` / `NetBirdKeepAliveService`，接入点仍是页面已有的 2 s 轮询（无第二个定时器）。详见 §1.11。

### 1.11 后台保活（0.0.19 实验版，方案 A）：`ui_bg_task` 与设置开关；0.0.20 后台自动重连与宽限期

**它做什么。** VPN 扩展进程被系统回收的优先级最低，而平台只给 UIAbility 申请长时任务；申请成功后同一个应用下的**所有**进程都不会被挂起。系统没有 VPN 类型的长时任务，因此这里**借用** DATA_TRANSFER：`module.json5` 里 `EntryAbility` 声明 `backgroundModes: ['dataTransfer']` 并申请 `ohos.permission.KEEP_BACKGROUND_RUNNING`（normal / system_grant，侧载签名即可，不需要 ACL）。业务类型与用途并不对应，**系统随时可以取消它、也可以判定为违规**——这套实现不与之争辩，见 §2 与 §3。

**一条诊断记录。** 事件名 `ui_bg_task`，字段全在既有公开键集内（`stage` / `result` / `reason` / `count`，0.0.21 起挂起 / 激活两条再用 `state` / `bytes`）：

| 字段 | 取值 |
| --- | --- |
| `stage` | 闭集：`start` / `refresh` / `stop` / `cancel` / `suspend` / `active` / `claim` |
| `result` | 闭集：`ok` / `error` / `skipped` |
| `reason` | 平台给出的原因**数字串**（取消原因、错误码、挂起原因），或纯逻辑闭集 token（如 `vpn-active` / `disabled` / `vpn-inactive` / `no-notification-id` / `stale`）；平台没有给出原因时写显式 `na` |
| `count` | 本次会话已付出的平台裁决次数（取消/创建失败），或进程内的取消序号；**`stage=suspend` / `stage=active` 时是平台给的 `continuousTaskId`**（这两条不是裁决，不付任何代价） |
| `state` | 仅挂起 / 激活记录：`suspendState` 的数值形式（`1` = 被挂起，`0` = 重新活跃），与 `stage` 表达同一件事，读日志的人不必再从 stage 词反推 |
| `bytes` | 仅挂起 / 激活记录：平台 `suspendMessage` 的**字符数**（`0` = 平台没给消息）。原文只进 hilog（W 级一条，先经 `sanitizeSuspendMessage` 清洗），不落任何记录 |

写入纪律：`result=ok` 的记录逐条写（它们本身就是"这条支线真的在动"的证据，且已被决策机限速）；`error` / `skipped` 同类**每进程只写一条**（键为 `stage:result:reason`），2 s 的轮询不会刷成日志洪水。没有要做的动作时**不写**任何记录——"无事发生"不是事件。记录落在 UI 进程自己的 `netbird-ui.log` 里。

**设置开关。** 设置页新增「后台保活（实验）」，说明文字一行（"通过系统长时任务保持 VPN 在后台运行，通知栏会常驻流量通知"）。取值持久化在 `filesDir/netbird-keepalive.json`：`{"v":1,"enabled":<bool>}`，**缺省或损坏一律按 true（开）**；写入与路由偏好同一套范式（临时文件 + fsync + rename），写入失败只返回 false、不改文件、不影响开关的运行时效果。开关切换立即生效：**关 = 立刻释放任务**，开 = 立刻按裁决重新申请。这个模块只碰自己这一个文件（路径白名单），不接触凭据、设备配置或路由文档。

**判决规则（纯逻辑模块，宿主可测）。** 只有**开关开着、且最新状态文档报告隧道处于活跃态**（connected / connecting / reconnecting）时才申请任务；文档缺失或过期**先进入 120 s 宽限**（0.0.20，见下文），宽限结束或自动重连已放弃就**撤掉任务**——不持有"没有业务在跑"的长时任务本身就是违规。其余裁决：

- 用户**主动取消**（`USER_CANCEL`）或**划掉通知**（`USER_CANCEL_REMOVE_NOTIFICATION`，按平台规则划通知即停任务）：本次连接不再自动申请，**下一次手动连接（或把开关重新打开）才清除**这个抑制。
- **系统取消**（`SYSTEM_CANCEL`、`SYSTEM_CANCEL_DATA_TRANSFER_LOW_SPEED`、`SYSTEM_CANCEL_DATA_TRANSFER_NOT_UPDATE`、其它）与**创建失败**：退避 30 s → 60 s → 120 s → 240 s，上限 5 min；本次会话累计 5 次之后**不再申请**。
- **`SYSTEM_CANCEL_USE_ILLEGALLY`**（类型非法）：本次会话直接停止申请；手动重连**也不会**清除它（那等于明知故犯），只有注销/进程重启才算新会话。
- 用户断开、注销、开关关闭：任务立刻停。
- 进度更新：DATA_TRANSFER 的进度超过 10 分钟不更新会被系统取消，因此实况窗按**30 s**节流重发（同一 `notificationId`、`SlotType.LIVE_VIEW`、
  `NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW` + `downloadTemplate`，正文是"在线 x/y · ↑tx ↓rx"，进度百分比恒为 0——隧道没有完成度，不编造）。

**0.0.20 增量：保活生效时的后台自动重连 + 宽限期。** 0.0.19 的真机数据显示，持有长时任务时 UI 进程在后台确实还在跑（`ui_bg_task refresh` 每 30 s 照常出现），而系统只清理 VPN 扩展进程（AdjLabel 仍 500）、并不清理 UI 进程。于是 0.0.20 让 0.0.15 的自动重连在这段窗口里**不再要求页面在前台**，同时给重连留出时间：

- **后台触发。** `NetBirdReconnectPolicy` 的输入新增 `keepAliveActive`（长时任务当前是否存在，取自保活判决策略状态里的 `taskPresent`）。只有它为真时，页面不可见的那一拍才放行，诊断记录的 `trigger` 记为 `background`（页面可见时仍是 `foreground`）。**其它判据一条不减**：marker 必须 `open`、8 s 用户停止门与 25 s 注销门、已配置且没有比 marker 更新的连接失败通知、60 s 冷却与 3 次上限、以及判据⑦（本进程必须收到过与 marker 同 pid 的 `:vpn` 终止事件）——尤其判据⑦，它才是"后台可以发起"不会撞上仍活着的扩展进程的原因。
- **判据②放宽。** 一旦本进程收到与 marker 同 pid 的 `:vpn` 终止事件，就不必再等状态文档到期（约 10 s）：页面把这份文档当作**不存在**处理（隧道确实已死、保活据此进入宽限、进入重连分支）。判定本身没有变化——判据⑦要的正是这份证据，所以提前发问只会得到"确实已死"时才会动作；marker 被新进程重写后 pid 不同，新隧道永远不会被误判成死掉的那个。
- **上限语义改为"连续失败最多 3 次"。** 此前是"每 UI 进程最多 3 次，看到活跃文档即清零"。UI 进程现在可长期存活，这条旧语义会让"起来几秒又死"的隧道无限重连，因此改成：一次重连之后，只有 **VPN 活跃会话持续 ≥5 分钟**才清零尝试计数（`RECONNECT_ACTIVE_RESET_MS`）；不到 5 分钟的活跃只是把该次尝试的 30 s 验证窗口结束（算成功），计数仍留在同一条失败链上。60 s 最小间隔不变。`vpnInactiveSinceMs` 由页面维护（VPN 由活跃变为不活跃的时刻，从未在本进程见过活跃则不设），宽限期从那一刻算起，不受页面节拍影响。
- **`startVpnExtensionAbility` 被拒（后台是否允许发起，真机尚未回答）。** 后台发起重连时若平台拒绝，页面记一条 `ui_auto_reconnect result=failed reason=<平台错误码数字或 none>`（`trigger` / `result` / `reason` / `count` 全在既有公开键集内），并把这次**已经计入**的尝试的 30 s 验证窗口立即结束——不重复计数，60 s 间隔仍然生效。页面不可见时**不写任何界面提示**（后台只留诊断）；页面可见时沿用原"正在自动重新连接…"文案。
- **宽限期（`grace`）。** VPN 不活跃时不再立刻撤任务：从最后一次活跃起 **120 s**（`KEEP_ALIVE_GRACE_MS`）内保留长时任务（`action=none`，`reason=grace`），实况窗正文改为"正在重新连接…"（仍按 30 s 节流重发，避免进度长时间不更新被系统取消）。超过 120 s 撤任务（`reason=vpn-inactive`）；自动重连已放弃时立即撤（`reason=reconnect-gave-up`）。**用户断开 / 注销 / 开关关闭仍然立即撤**——宽限只给"被系统拿走的隧道"，不给用户自己结束的连接。
- **仍未被验证的项（本轮不声称）**：后台（持有长时任务时）`startVpnExtensionAbility` 是否被系统允许、真机上这个错误码长什么样、以及重连成功前 120 s 的宽限在真机上的实际效果。这两点在真机日志里都以 `ui_auto_reconnect` 记录本身为准。

**0.0.21 增量（一）：挂起 / 激活回调的完整记录。** 0.0.20 的真机证据里，全天只有一条挂起记录（`app-logs/netbird-ui.log:1800`）与一条激活记录（同文件 :1808）：

```text
ts=2026-10-11T08:22:03.646Z pid=39870 role=ui event=ui_bg_task count=0 reason=-1 result=ok stage=suspend
ts=2026-10-11T10:24:32.311Z pid=39870 role=ui event=ui_bg_task count=0 reason=platform-active result=ok stage=active
```

`reason` 是当时用 `String(info.suspendReason)` 写的：`String(undefined)` 会写成 `undefined`、`String(null)` 会写成 `null`，所以 `-1` 只可能是**平台自己给的数字**。核对本机 API 26 声明（`<sdk>/default/openharmony/ets/api/@ohos.resourceschedule.backgroundTaskManager.d.ts`，:512 `ContinuousTaskSuspendInfo{continuousTaskId, suspendState, suspendReason, suspendMessage?(since 26.0.0)}`、:1063 `on('continuousTaskSuspend')`、:1093 `on('continuousTaskActive')`、:1716 `ContinuousTaskSuspendReason`）后确认：原实现读的就是这几个**声明过的**字段名，没有读错字段；而 `-1` 不在 `ContinuousTaskSuspendReason`（该枚举在本 SDK 上是 4…19，含 4 低速率、11 类型非法、12 系统高负载，后者在文档里标注为 reserved）里。因此新实现把平台数字**原样**写进 `reason`（`Number.isFinite` 为假时写 `na`，绝不猜成某个 token），并把同一事件的 `suspendState`、`continuousTaskId` 一起写下（`state` / `count`），`suspendMessage` 只记字符数（`bytes`），原文走一条 W 级 hilog——**该行输出的不是平台原文**：0.0.21 起原文先经纯逻辑 `sanitizeSuspendMessage` 清洗（只保留可打印 ASCII `0x20`–`0x7e`，其余字符一律写成 `?`；长度 ≥16 的连续 `[A-Za-z0-9+/=_-]` 串折成 `<tok>`；结果截断到 120 字符），`bytes` 仍记**原始**字符数，原文不落任何记录也不再原样进日志。激活回调（`ContinuousTaskActiveInfo` 只有 `id`）同样记 `stage=active` / `state=0` / `count=<id>`。这两条 stage 的 `count` 因此从"裁决次数"改为平台任务 id，其余 stage 的语义一字未改。

**0.0.21 增量（二）：恢复后立即复查（`ui_resume_check`）。** 同一次真机证据显示，平台可以把整个 UI 进程挂起 7362.651 s（16:21:49→18:24:32 之间 2 s 轮询与 30 s 实况窗整段停摆，VPN 在 17:08 被相机清理后 1 h16 m 无人重连，长时任务却始终没有被撤销）。因此"进程又开始跑了"本身必须触发一次立即复查：

- **三条信号合流到一处。** (a) 平台的 `continuousTaskActive`（以及挂起回调里 `suspendState=false` 的那一种"重新活跃"）；(b) 页面自己的 2 s 轮询发现**与上一拍间隔 > 15 s**（`KEEP_ALIVE_RESUME_GAP_MS`，纯策略里的阈值与单次触发性质由宿主 selfcheck 在毫秒边界上钉住）；(c) UIAbility 的 `onForeground`。前两条由适配层转发到页面的唯一监听者，第三条由 ability 走同一条通道（只转发一个信号，不读任何状态文档）。
- **动作只有一个**：立即执行**已有的** `pollConnectorStatus`（复用同一个函数、同一次状态文档读取），不新增定时器、不新增文档读取；已经在跑的那一拍本身就是它要的复查，直接跳过（`pollRunning` 单飞保护，`try/finally` 保证一次异常不会把锁留在 true 上）。冷启动的首拍之前不处理：那时还没有"上一拍"。
- **记录一条** `ui_resume_check`，字段 `reason`（闭集 `active` / `gap` / `foreground`）与 `count`（两拍间隔秒数）；事件名是小写 token、不在内部事件名里，字段全在公开键集内。**一次恢复只记一条**：读完即把本拍时刻写回，下一拍的间隔就恢复正常。间隔不够大（例如只是切了个前台）时不记录，但那次立即复查照做。
- **恢复后 10 s 内的额外复查不再新增机制**：`APP_KILLED` 可能在恢复瞬间才投递，第一次复查时文档已过期却还没有终止事件，按既有策略会 skip 成 `vpn-not-confirmed-dead`（安全行为）；页面自己的 2 s 轮询在这 10 s 内就是 5 拍，每拍都重读文档、每拍都会重新判一次重连（节流与上限仍在纯策略里），所以"多做几次复查"就是既有节拍本身。
- **判据与防循环参数一字未改**：重连判据、60 s 冷却、连续失败 3 次、120 s 宽限期全部不动；本次只改"什么时候把状态文档再读一遍"。

**本轮（0.0.21）新增记录仍未验证的项**：新记录在真机上是否如期出现、平台挂起时那两条回调是否真的投递（若 `-1` 之外还有别的取值，或 `suspendMessage` 根本不来，都会如实写进日志）、以及恢复后的立即复查在真机上能否把"后台重连失效"的窗口从小时级压到秒级。

**诚实边界。** ① 通知栏会**常驻**一条流量通知，这是这类长时任务可见的代价。② 重发实况窗的是**页面自己的 2 s 定时器**：进程退到后台被冻结后，本实现不会再重发通知，因此大约 10 分钟后系统可能以"进度未更新"取消任务；届时 UI 回到前台（页面恢复轮询）才会看到取消事件，并按上面的预算重新申请。③ 取消/挂起/激活三个回调是否真的投递到 UI 进程、任务是否获批、实况窗能否显示、以及 VPN 扩展进程的回收优先级是否真的因此提高——**全部未在真机上验证**，本文不声称任何保活效果。

接线已在源码与构建产物中核实；**真机行为仍未验证**（见文首状态），本节不声称任何设备端效果。

006 构建证据（本轮实跑，非设备证据）：完整 native 构建 + HAP 打包 exit 0，pack.info 为 `versionCode=6 / versionName=0.0.6`、`apiVersion target=26 compatible=26`；`libs/arm64-v8a/libnetbird_core.so` 在未签名与已签名 HAP 中 sha256 一致（`bf708153a0ca92283fac7a3036d05ec7f687146a64c984dde696095969e56532`），且其中含 `connector_resource_status` 这一 NAPI **属性名**（不是 C 动态符号；005 打包的成员不含它，故必须完整重构建）。签名 8 参数 launcher exit 0、`verify-app` exit 0（Digest verify true）。**下一步部署要回答的仍是未知项**：扩展进程内 `/proc` 三个节点是否可读、真机上 `connector_resource_status()` 是否与宿主一致、以及系统查杀的真实成因——均未修复、未归因。

本轮调整（§1.1 存储上限、§1.4 采样节拍、§1.6 导出上限）**只落在源码**：尚未重新打包，也未重新安装到手机。上文 006 的构建与签名证据对应的是调整**之前**的产物，因此这些新值在设备上尚未生效，本文不据此声称任何手机端效果；版本号不因本次调整而变。

## 2. 官方口径下的 VPN 后台生命周期（2026-10-01 事实检索）

- **VPN 停止的两个触发条件**：① 调用 `startVpnExtensionAbility` 的应用进程退出；② VPN 服务进程销毁。仅销毁 UIAbility **不等于**进程退出（进程内还有组件时宿主进程常驻）。
- **长时任务不适用于 VPN 扩展**：Stage 模型下长时任务仅支持 UIAbility 申请，且长时任务类型表里**没有 VPN 一类**；并且长时任务不是"永生"——申请只提高被回收优先级，系统资源严重不足时仍会终止。（0.0.19 的实验版正是在这条事实之上做的取舍：不假装存在 VPN 类型，而是**借用** `dataTransfer` 由 UIAbility 申请，并把它的一切判决放在 §1.11 的规则里；这只提高（若真提高）被挂起/被回收方面的处境，不构成任何"防杀"承诺。）
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

§1.11 的口径另有**本机 SDK 声明**为直接依据（只读）：`<sdk>/default/openharmony/ets/api/@ohos.resourceschedule.backgroundTaskManager.d.ts`（`startBackgroundRunning(context, bgModes: string[], wantAgent): Promise<ContinuousTaskNotification>`、`stopBackgroundRunning(context)`、`getAllContinuousTasks(context, includeSuspended)`、三个 `on/off('continuousTask*')` 回调、`ContinuousTaskCancelReason` 与 `ContinuousTaskDetailedCancelReason`）、`.../api/@ohos.notificationManager.d.ts`（`SlotType.LIVE_VIEW`、`ContentType.NOTIFICATION_CONTENT_SYSTEM_LIVE_VIEW`、`publish`、`isNotificationEnabled` / `requestEnableNotification`）与 `.../api/notification/notificationContent.d.ts`（`NotificationSystemLiveViewContent.typeCode` / `title` / `text`）。

## 5. 相关文件

- `client/entry/src/main/ets/vpnextensionability/NetBirdDiagnostics.ets` / `NetBirdDiagnosticsCore.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdLogExport.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdSystemEvents.ets` / `NetBirdSystemEventsCore.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdKeepAlivePolicy.ets`（纯逻辑：保活裁决）/ `NetBirdKeepAliveStore.ets`（`netbird-keepalive.json`）/ `NetBirdKeepAliveService.ets`（唯一 import `backgroundTaskManager` / `notificationManager` / `wantAgent` 的模块）
- `client/core/src/phone_log.rs`
- `client/entry/selfcheck/run-diagnostics.mts` / `run-log-export.mts` / `run-system-events.mts` / `run-keep-alive.mts`（§1.11 的裁决机、持久化、平台回调与接线断言，含 keep-alive-hooks.mjs 的 `@ohos.file.fs` 功能桩与 0.0.21 起的 `backgroundTaskManager` 回调桩）、`run-reconnect-policy.mts`
