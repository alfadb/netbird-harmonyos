# N1a Emulator campaign 0008 measured-fail 记录（2026-08-31 执行）——两阶段流首次完整运行

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0007` / `EV-N1A-EMU24-20260831-0007`（AUTH-N1A-EMU24-20260831-0007 granted，判据 frozen-r3）的执行事实：**两阶段流首次完整运行并达成有效测量终态 `fail`**——不是 runner 缺陷，是判据下的真实测量结果。

## 测量事实

**Phase A（aa test / testrunner 进程）**：探针全判据 pass、`N1A_RESULT|verdict=PASS|c5=not-triggered|throughput_mibps=99.14`、ohosTest 交叉一致（缺陷 #8 修复生效）、`N1A_RES` 短 marker c7/c8 pass、probe-detail.json 重组全通、截图采集。**第五次连续 Phase A 全判据 pass。**

**Phase B（aa start / entryability 独立 UIAbility 进程）**：force-stop 后独立进程启动成功、页面探针自动运行并发射 marker——但 `N1A_RESULT|verdict=FAIL|c5=fail|throughput_mibps=81.43`、`N1A_RES` c7/c8 pass（fd 门/清理仍全过）。**C5 fail 的冻结语义**：背压阶段死锁/超时/已收包损坏之一（时间盒 10s 内未完成或有 corrupted）。c5=fail 按冻结聚合规则 → overall fail。

## 与两阶段流裁决的关系

裁决登记过"两执行 c5 允许差异（induced vs not-triggered）"——本运行正是差异的真实体现：Phase A not-triggered、Phase B **fail**（不是 induced 也不是 not-triggered）。**任一方 c5=fail → overall fail** 是裁决原文的显式条款。runner 的交叉一致判定、页窗口隔离、窗口化重复规则全部首次在产线正确工作。

## 观测性缺口（登记）

Phase B 的 Index.ets catch 不发射 detail chunk（设计如此——页窗口只承担 C9 页一致性），故 Phase B 的 C5 fail 具体形态（corrupted 计数/deadlock 超时/交付不齐）**不可从证据恢复**。若后续治理需要根因，最小改法是 Index.ets 在 catch 中也 chunk 发 detail（与 ohosTest catch 对称）——属实现层增强，不触判据。

## 处置

本 campaign **consumed-fail**（有效测量终态；两阶段流的全部机制首次验证工作正常）。N1a 首次完整测量的裁决为 **fail**——按治理，后续（重测/判据修订/门处置）须 T0/判据持有人/用户决定。探针/runner/ets 本身无已知缺陷待修（八个缺陷全部在产线验证修复生效）。
