# 0.0.6 资源诊断：观测量与观察指南

本增量把 HarmonyOS 客户端版本从 `5 / 0.0.5` 递增到 **`6 / 0.0.6`**：`client/build-profile.json5` 的 default product 与 `client/AppScope/app.json5` 同步、取值一致（python3 `json5` 真解析读出 `versionCode=6`、`versionName='0.0.6'`）。同时在既有 60 s `mem_sample` 上增加一组**只读、自作用域**的资源读数。

**状态：真机行为未验证。** 观测量已在源码落地并在宿主侧校验；本文只描述契约与观察方法，不声称任何保活或修复效果。

## 1. mem_sample 的新读数（同一 60 s 节拍、同一条记录）

| 字段 | 来源 | 含义 |
| --- | --- | --- |
| `cpu_ms` | `process.getPastCpuTime()` | 本进程自启动以来的**原始累计** CPU 时间（ms）；不换算占用率、不做差分 |
| `threads` | `/proc/self/status` | 本进程线程数 |
| `vmsize_kb` | `/proc/self/status` | VmSize |
| `rss_anon_kb` | `/proc/self/status` | RssAnon |
| `rss_file_kb` | `/proc/self/status` | RssFile |
| `sys_swap_total_kb` | `/proc/meminfo` | 整机 SwapTotal |
| `sys_swap_free_kb` | `/proc/meminfo` | 整机 SwapFree |
| `fd_count` | `/proc/self/fd` | 本进程 fd **条目数**（只计数；不读名称 / 目标 / inode） |
| `fd_capped` | 同上 | `true` 表示列举到 1024 上限，`fd_count` 只是下界 |

旧的 4 个字段（`rss_kb` / `swap_kb` / `heap_kb` / `sys_avail_kb`）不变，仍在同一条记录里；其中 `heap_kb` **只是 ArkTS/JS VM 堆**，不是进程总内存。

- **单位口径**：来自 `/proc` 的是内核 kB（KiB 口径）；来自 `hidebug` 的 `rss_kb` / `swap_kb` / `heap_kb` / `sys_avail_kb` 字节口径未定义，只按字段名读作 KB。两组不要做换算意义上的合并比较（字段契约见 `docs/phone-diagnostics-background.md` §1.4）。
- **不可用一律写 `na`，绝不写 `0`**；某个 `/proc` 节点读取失败时，该次运行只报一次 `DIAG_PROC_UNAVAILABLE`（不逐分钟刷 errno）。
- 三个 `/proc` 节点只用 `self`（**从不传 pid**），单次读取上限 16 KiB，fd 在 `finally` 关闭。**不用** PSS / `getAppNativeMemInfo()` / `smaps_rollup`；**没有新增权限**。

## 2. 怎么观察（不改用户行为）

- **按自然使用观察**：WiFi / 蜂窝由用户自行选择，不要为了取样本强制切网或强制联网，也不要制造负载。
- **看导出的 `mem_sample` 行**：每行自带 `pid`，并同时带 `cpu_ms`。要算区间用量，取**同 pid** 的两条记录，按它们自己的时间戳求差（`cpu_ms` 相减 = 该区间消耗的 CPU 时间；内存字段相减 = 该区间净变化）；不要跨 pid / 跨会话相减。
- **资源计数只在同一 pid、同一生命周期内比较**：`fd_count`、`threads`、`*_kb` 只描述本进程当时状态；进程换代（新 pid）后计数从头开始，跨代相减没有意义。
- **原始 Swap 不是阈值判据**：`sys_swap_*` / `swap_kb` 是原始读数，没有“达到多少即有问题”的阈值；只能与同 pid 序列及同刻其它字段并置观察。
- **下一次查杀归因用 `ev_time`**：死亡时刻取事件自带的 `ev_time`，临死读数取**同 pid 最接近的一条 `mem_sample`**；**不要把 UI 侧收到通知的日志时间戳当作死亡时刻**——通知会迟到（本轮实测 35 s 到 2 h 44 min）。
- **日志覆盖 ≠ 存活时长**：进程被杀瞬间无法自写末行；不要把日志跨度当 uptime。

## 3. 已知读数边界（不要越读）

- **不出 CPU 占用率**：`cpu_ms` 是原始累计值；多核设备上把它除以墙钟会得到偏高的“单核等效”百分比。要速率就按 §2 做同 pid 差分。
- **fd 计数**：`/proc/self/fd` 的自我列举是快照式计数，同一进程短时间内可能有少量抖动；`fd_capped=true` 时只能当下界。
- **不是同刻快照**：`hidebug` 与三个 `/proc` 节点是先后分别读取的，字段之间存在毫秒级时间差，不是原子快照；不要据此做微秒级相关性断言。
- **增长不等于泄漏**：内存 / 线程 / fd 增长只说明“涨了”，在缺少 PSS、同刻系统构成和多会话基线之前不足以判定泄漏；也不要据此声称“线程永不停止”——core 的停止路径会置停止标志（`connector.rs` 的 `stop()`）。
- **本增量不含**：没有 DNS 新功能，也没有任何后端 / 服务端承诺；`sys_swap_*` 只是读数，不是修复 `SWAP_FULL` 的手段。

## 4. 005 证据要点（更正后的口径）

来源：`/home/worker/n2h-kit/status-20261001/v005-memory-timeline-20261006-153717.md`（只读数据报告）。

- 005 起共 **5 次系统查杀**：`LowMemoryKill` ×2、`SwapFull` ×3。
- 以 `ev_time` 为锚，**5/5** 都能找到同 pid 且在事件**之前**的最近 `mem_sample`，距离 **5.4–52.8 s**（旧稿的“0/5”是把 UI 报告时刻当成了死亡时刻）。
- UI 通知迟到 **35 s – 2 h 44 min**；`ev_detail_ts` 也早于 `ev_time`（18–123 ms）。三个时刻**不能互换**。
- 005 首个会话（pid 16500）56.8 min 的 rss+swap 缓升**不能证明泄漏**：当时只有 4 个字段、无 PSS、无同刻系统构成，且 rss / swap 多次反相摆动。
- **没有已证明的正常路径泄漏**；故本轮不写“已修复内存泄漏 / `SWAP_FULL`”。

## 5. 接线与状态

### 5.1 shell bridge 已接线

- §1 的 9 个新读数接在**既有的** 60 s `mem_sample` 上（`NetBirdDiagnostics` / `NetBirdDiagnosticsCore`），没有新增定时器。
- 原生端点 `connector_resource_status()`（006 additive，只读、非阻塞）的消费方**已落地**：VPN 扩展把它挂在**已有的** 60 s 写入器状态节拍上（`PHONE_LOG_STATUS_SAMPLE_MS = 60000`：`samplePhoneLogStatus()` → `sampleNativeResourceStatus()`），不做每 5 s 采样、不新增间隔。
- 该快照是**两条**记录、不是一条：22 个标量叶子装不进单条有界行（超出 `MAX_LINE_CHARS` 会被记录构造器静默截断），故按 `stage=connector|seq=1` 与 `stage=transport|seq=2` 分成两条 `native_resource_sample`。字段名按原生分区加前缀；两段合计正好 22 个叶子，另加各自的 `stage`/`seq`。
- 读取走 `NetBirdConnector.ets` 的防御性包装（`connectorResourceStatus()` / `nativeResourceSample()`）：导出缺失、抛错或文档不可解析 → 整份 `null` → 每个叶子渲染显式 `na`，绝不编造 `0`/`false`。

### 5.2 两份 schema（按当前代码）

- **原生文档**：`connector`（present / generation / running / state / starts_total / stops_total）、`ice`（available / sessions_present / sessions_live / sessions_stopped / live_local_sockets）、`ice_sockets`（available / queued / taken）、`socket_supply`（installed / pending / queued）、`relay_carrier`（available / attached_lanes / pending_opens / foreign_clients）、`dns_lookups`（pending_receivers），共 **22 个标量叶子**。每个叶子独立 `try_lock`、非阻塞，忙则 `null`（未知），从不给假 `0`；`present:false`（未安装 connector）是已知的全零状态。
- **shell 扁平记录**：`native_resource_sample` 两段——seq=1 为 `conn_present` / `conn_gen` / `conn_running` / `conn_state` / `conn_starts` / `conn_stops` / `ice_available` / `ice_sessions` / `ice_live` / `ice_stopped` / `ice_fds`；seq=2 为 `icesock_ready` / `icesock_queued` / `icesock_taken` / `supply_installed` / `supply_pending` / `supply_queued` / `relay_ready` / `relay_lanes` / `relay_opens` / `relay_foreign` / `dns_pending`。全部在该事件的公开字段白名单内。

### 5.3 原生导出已随包

- 本轮做了**完整 native 构建 + HAP 打包**。`libs/arm64-v8a/libnetbird_core.so` 在未签名与已签名 HAP 中都存在，成员 sha256 与 `llvm-strip` 后的 core 产物逐字节一致（`bf708153a0ca92283fac7a3036d05ec7f687146a64c984dde696095969e56532`）。
- `connector_resource_status` 是 **NAPI 属性名**（`client/core/src/napi.rs` 的注册列表；lib 用例 `napi::tests::napi_init_attaches_registered_functions_to_exports` 断言它被挂到 exports），**不是 C 动态符号**：本轮按字节串在两个 HAP 成员里各命中 1 次；不要求 `readelf --dyn-syms` 出现同名导出（那里为 0，符合预期）。
- 上一轮（005）打包进 HAP 的成员**不含**该字串（0 次命中）——这正是本轮必须先做完整 native 构建、再打包的原因。

### 5.4 生命周期 marker（观察项，行为未改）

- 新增 `vpn_destroy_phase` 里程碑：`stage=destroy` 固定，`phase=requested|failed|settled` 只标出 teardown 进入/失败/落定；字段只有 `phase`/`stage`，不带错误文本。
- marker 语义**未改动**：`open`/`clean` 的写法与判定不变，`state=clean` 仍只由收尾的 `stopDiagnostics()` 写；`PRIOR_EXIT_UNCLEAN` **仍会过度上报**（上一轮实测：干净 `onDestroy` 之后下一会话仍报）。**不要声称已修好误报。**

### 5.5 本轮实跑证据（宿主侧）

- `cargo test --offline --locked`（cwd `client/core`，`CARGO_TARGET_DIR=/home/worker/n2h-kit/cargo-target-0034`）：**exit 0**；46 个测试目标（另 1 个 doc-test），**863 passed / 0 failed / 0 ignored**。相对 005（46 目标 / 856 passed）：+1 个目标（`tests/connector_resource_status.rs`，1 例）+ 6 个 lib 用例 = +7。
- 首次完整跑为**红**（exit 101，`n2_ring::tests::ack_gated_rotation_never_deletes_an_unacknowledged_segment` 判 FAILED，且该失败**没有任何断言文本**落盘）；该用例单跑绿、lib-only 511/511 绿，随后重跑全绿。**不可复现、根因未定**：不能据此判定是本轮新代码的确定性失败，也不能声称套件确定性全绿（原始日志在证据目录，未删改）。
- 构建：`DEVECO_SDK_HOME=... bash client/build.sh`（`CARGO_TARGET_DIR` 置空）**exit 0**；ArkTS 编译随 `hvigor assembleHap` 一并通过。pack.info：`versionCode=6 / versionName=0.0.6`，`apiVersion target=26 compatible=26`，`releaseType=Release`。
- 签名：8 参数 `N1bdiscSignLauncher` **exit 0**；`verify-app` **exit 0**（Digest verify true，SHA-256）。交付件：未签名 `5264571 B` `ba8f473e4cb2f74e15dc3ce5874231c5993e7ca0539830ecfb6d15d0749d31d5`；已签名 `5365922 B` `dd0e1aead7921df33fde6dfc0da5c2ed1a9f824e36d011f5143b3c39c63449b0`。
- 构建前后 source manifest（core src/tests/config）**逐字节一致**（只差清单自身的时间戳表头）；doc 改动之外的源码未动。

### 5.6 未验证 / 下一步部署

- 006 源码作者轮已跑绿 entry selfcheck **27/27 `run*.mts` 入口 exit=0** 与 `run-diagnostics.mts` **181 passed / 0 failed**；本轮因源码未改动**未重跑**，按原记录引用（未独立复核）。
- **真机行为未验证**：本轮无安装、无 hdc、无任何设备/主机动作。下一步部署要回答的是**未知项**而不是已修项：目标设备上 `/proc/self/status`、`/proc/meminfo`、`/proc/self/fd` 在**扩展进程**内是否可读（不可读即按设计全 `na`）；`connector_resource_status()` 在真机扩展进程里是否与宿主用例一致；以及系统查杀（`LowMemoryKill` / `SwapFull`）的真实成因。**这三项均未修复、未归因。**

## 6. 相关文件

- `client/entry/src/main/ets/vpnextensionability/NetBirdDiagnostics.ets` / `NetBirdDiagnosticsCore.ets`
- `client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets`（`connectorResourceStatus` / `nativeResourceSample` 包装）
- `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`（`sampleNativeResourceStatus` 60 s 挂载、`vpn_destroy_phase`）
- `client/entry/selfcheck/run-diagnostics.mts` / `run-phone-log.mts`（006 桥面字段白名单与两条记录）
- `client/core/src/connector.rs`（`connector_resource_status_json`）、`client/core/src/napi.rs`（属性注册）、`client/core/tests/connector_resource_status.rs`
- `docs/phone-diagnostics-background.md` §1.4（字段契约）、§1.8 / §1.10（marker 与接线）、`docs/app-install-version-policy.md`（版本口径）
