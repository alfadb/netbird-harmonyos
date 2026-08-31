# N1a Emulator campaign 0008 measured-fail 记录（2026-08-31 执行）——两阶段流首次完整运行

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0007` / `EV-N1A-EMU24-20260831-0007`（AUTH-N1A-EMU24-20260831-0007 granted，判据 frozen-r3）的执行事实：**两阶段流首次完整运行并达成有效测量终态 `fail`**——不是 runner 缺陷，是判据下的真实测量结果。

## 测量事实

**Phase A（aa test / testrunner 进程）**：探针全判据 pass、`N1A_RESULT|verdict=PASS|c5=not-triggered|throughput_mibps=99.14`、ohosTest 交叉一致（缺陷 #8 修复生效）、`N1A_RES` 短 marker c7/c8 pass、probe-detail.json 重组全通、截图采集。**第五次连续 Phase A 全判据 pass。**

**Phase B（aa start / entryability 独立 UIAbility 进程）**：force-stop 后独立进程启动成功、页面探针自动运行并发射 marker——但 `N1A_RESULT|verdict=FAIL|c5=fail|throughput_mibps=81.43`、`N1A_RES` c7/c8 pass（fd 门/清理仍全过）。**C5 fail 的冻结语义**：背压阶段死锁/超时/已收包损坏之一（时间盒 10s 内未完成或有 corrupted）。c5=fail 按冻结聚合规则 → overall fail。

## 与两阶段流裁决的关系

裁决登记过"两执行 c5 允许差异（induced vs not-triggered）"——本运行正是差异的真实体现：Phase A not-triggered、Phase B **fail**（不是 induced 也不是 not-triggered）。**任一方 c5=fail → overall fail** 是裁决原文的显式条款。runner 的交叉一致判定、页窗口隔离、窗口化重复规则全部首次在产线正确工作。

## 观测性缺口（登记；2026-08-31 判据持有人裁决纠正）

**裁决纠正**：本执行 Phase B 走的是 **try 成功返回 fail 结果**（hilog 页窗口第五行 "rendered from probe result"），不是 catch 路径——Index.ets **两条路径都不发射 detail chunk**，故 C5 fail 的三态形态（时间盒/重传帽/accounting/会话错）不可从证据恢复。判据持有人裁决 `C5_PHASEB_RULING=insufficient-evidence`：0008 fail 字面保留、不得升格为 N1a 数据面正式终态、不得在形态恢复前修订 C5；后续前置（实现层，不触判据）= Index.ets **双路径** chunk 发 detail + overlay 失败路径 native 自行 chunk + C5 形态短 marker（独立前缀）+ 有界 AppFreeze 采集窗 → 新 ID 重测 → 持有人凭形态终裁。另登记：Phase B 墙钟 226.7s（aboutToAppear→marker）远超探针理论时间上限（~45s），指向 cold-start UI 线程冻结——机制解释见裁决，不作为定论。

## 处置

本 campaign **consumed-fail**（有效测量终态；两阶段流的全部机制首次验证工作正常）。N1a 首次完整测量的裁决为 **fail**——按治理，后续（重测/判据修订/门处置）须 T0/判据持有人/用户决定。探针/runner/ets 本身无已知缺陷待修（八个缺陷全部在产线验证修复生效）。
