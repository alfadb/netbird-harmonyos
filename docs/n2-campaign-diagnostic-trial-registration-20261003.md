# N2 campaign 真机诊断试跑 —— 测前登记（澄清 + 环处置 d1）（非证据 / 非 AUTH）

- 记录性质：**测前登记 / 澄清注记**，不是冻结件、不是 AUTH、不是设备授权，也不是实现符合性结论。本登记**不修改任何冻结文本**（预注册件与格式基线逐字节未动，见 §6）。
- 写入时仓库状态：`main`，HEAD `212dee7e49d53ebc52fc22371a5f87dcd702e4cb`（212dee7，未 push）。
- 只读依据：
  - 冻结预注册 v4.3：`docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（`:3` 冻结与禁止追认、`:32` §2 测前干净进程、`:127` §5 发现泄漏/未知外层即停测并交 T0/用户）。
  - 诊断模式最小改动设计：`/home/worker/n2h-kit/status-20261001/CAMPAIGN-DIAGNOSTIC-MODE-DESIGN.md`（§5 `:141-159` 澄清建议）。
  - 环处置只读分析：`/home/worker/n2h-kit/status-20261001/B2-RING-LOST-RECORDS-OPTIONS.md`（§3 推荐方案 (d1)）。
  - 上述两件均在仓库外，只读引用，本登记不改写。

## 1. 性质

campaign 真机诊断试跑属冻结件 `:3` 所述「`is_evidence=false` 测前诊断」类别：

- 不构成 N2/N6 证据；不产生、不改变任何格的 `pass`/`invalid`/`UNSAT`/`not exercised` 状态；不进入 §3 的「3 次独立事件」计数与 ±2000 ms 配对；不消耗任何一批 AUTH 的预算、次数或原件配额；不属于 §1 的 24 格适用集与 4 个豁免格。
- 不授予 `N2 第一阶段通过` 或任何门结论；任何诊断记录不得追认为证据（沿用 `:3`、v4.2 F1、v4.3 `:309-311` 的禁止追认）；诊断目录不得拷入任何正式证据目录、不得作为任何格的输入。
- 诊断是**票据级 + 主机级**分类：不新增轻量旁路实现，与正式模式共用同一执行路径（阶段名/顺序、`run_cell_rep`/`run_cd_rep` 不变），只收窄迭代矩阵；诊断产出写独立文件名 `diagnostic-report.json`，不写 `judge-report.json`。
- 本登记为**测前**作出（`CAMPAIGN-DIAGNOSTIC-MODE-DESIGN.md` §5 判断：诊断属既有 `is_evidence=false` 测前诊断类别，不需要新冻结版本，但须一条最小澄清登记）；符合 `:3`「变更须测前回 T0 独立审查、取得用户重新批准并登记新冻结版本，不得测后追认」的程序要求。

## 2. 三条澄清（测前登记，不改任何判据）

1. **泄漏与未知外层能力**：诊断中出现 TUN 泄漏（真实生产外层包匹配冻结目的集合进入 from-OS TUN）、`ipv6_other`、或未登记外层 socket，按 §5`:127` 交 T0/用户裁决；**裁决前不得开始正式运行**。诊断不豁免 `:127` 的全轮停测与人工裁决（生产门 hold 须由执行器外的人工裁决清除）。诊断中任一此类信号即保存原件、停测并如实上报，不得为「修好它」而重开 App 或重试。
2. **干净进程与设备状态**：§2`:32` 的「测前干净进程」要求，在正式运行前须处理诊断留下的设备状态。设备日志环跨 arm 保留旧段与累计计数（`lost_records` 只增不减），**当前无重置入口**；因此正式运行前须由后续「受控环重置」功能（另行实现、评审与授权）或其他经批准的方式（例如经授权的清应用数据并解决凭据重注入）建立干净基线。诊断本身不改变该要求，也不得据诊断结果声称已满足该要求。
3. **参数变更程序**：因诊断发现需改正式参数（适用格、oracle、阈值、路由/覆盖、判据）的，走 `:3` 的**测前变更程序**——回 T0 独立审查、取得用户重新批准、登记新冻结版本；不得测后追认，也不得用诊断记录填格或补做通过。

## 3. 环处置决定（方案 d1）

- 首次诊断**不缩小环段**，保持 4 MiB 默认：诊断票据的 `diagnostic_scope` **不带** `ring_segment_bytes`，验收看 `N2_RING_ARMED|...|max_bytes=4194304` 且**无** `N2_RING_SEGMENT_BYTES` 行。
- 试跑前用一次**只读** manifest 记录环基线：`N2_RING_MANIFEST`（经既有白名单 `ring_dump` 的空文档 `{}`）逐字记录 `file/sess/first_seq/last_seq/acked_seq/rotations/failures/lost_records/lost_from/lost_to/overflow/overflow_events/bytes`；判据 `first_seq=1`、`lost_records=0`、`overflow=false`、`acked_seq=0`。若出现 `lost_records>0`，B2 立即升级为「已污染」，改走清应用数据或受控环重置路线。
- 「环跨手臂前缀删除被记录」本轮登记为 **`not-observable`**，不得据此出 pass；正式 `judge-report` 不含该结论。
- 本轮**不发任何 `ring_ack`**（明确拒绝 d2：ACK 后缩段会删掉未归档旧前缀 ⇒ 环头前移 ⇒ 之后每个新 arm session 首次取回被 `ring_prefix_deleted_unarchived` 具名拒绝、整阶段 PAUSED(5)）。
- 调整说明：用户此前选择「在诊断中专门触发轮换」；因 B2 分析表明在现有环状态（`acked_seq=0`、约 3.1 MB 当前段、`.1`/`.2` 不存在）下缩段是**不可逆单向门**（约 128 KiB 新写入后要么 overflow 使 `lost_records` 永久 `>0` 污染后续正式 C/D，要么删掉未归档旧前缀而锁死后续手臂），故调整为上述 d1；缩段/轮换演练留待**受控环重置**功能（B2 方案 (c)，core+壳+白名单+AUTH+重建签名，需另行实现、评审与授权）落地后再行决策。

## 4. 诊断模式改动的独立安全评审结论摘要

本次收到的独立安全评审结论摘要（只摘要，不改判据）：

- **无绕过正式门的路径、无诊断冒充证据的路径**：诊断票据与正式票据共用同一 arm ticket 内容与同一执行路径，`applicable_*`/`exemption_registration` 由 `diagnostic_scope` 取代并受 fail-closed 约束（行 ⊆ ROWS × 列 ⊆ {C,D}，重复 1..3）；judge 顶层分叉到独立 `diagnostic_report()`，三个判定入口加 fail-closed 断言，任何 gate/pass 路径拒绝读取诊断目录；安全类运行时停测一项不少。
- **B1 已修复**：跨运行（诊断/正式）plan id 相同导致生产门 fail-open——`reconcile_cd_window` 在环拼接的**全部会话**里按同 id 取第一条记录，把旧诊断链读成本计划自己的链，门算出 `released`（应为 `blocked`）、`ProductionGateHeld` 不触发；评审探测日志已复现。修复后 runner/judge 只采用 `sess == 本 arm session` 的记录，其他会话同 id 记录被忽略并计数（`n2-campaign/README.md:1008-1020`）。离线 selftest 通过（`/home/worker/n2h-kit/status-20261001/b1-fix-selftest2-113212.log`：`23/23 checks passed`，rc=0）；**真机（非 `--fake`）路径仍未验证**，签发前须重算 6 项 pin。
- **B2 按上述处置**：环 `lost_records`/缩段按 §3 的 d1 执行（不缩段、只读基线、`not-observable` 登记、不发 ACK）。
- **B3 即本登记**：本文件的测前登记与三条澄清。

（评审原件与完整推理以 kit 侧分析件为准；本摘要不构成对判据的修改，也不替代 T0 或用户批准。）

## 5. 决定事实与边界

- 决定时间（`date -Is` 现算）：`2026-10-03T19:53:03+08:00`。
- 渠道：「本会话 DSH Web GUI 用户消息」。
- 决定过程：主会话向用户提出两项选择——(a) 环处置：方案 d1 还是受控环重置；(b) 是否登记上述三句澄清——并推荐 d1 与登记。用户回复「**全部你来决定**」；据此由主会话按推荐决定采用 **d1** 并**登记**本澄清。
- 边界：**本登记不授权上机、不授权抓包、不签发任何 AUTH**；诊断票据的签发、sidecar 与人类回读确认仍按既有流程显式完成，正式运行仍须两批 AUTH 及 T0/用户批准。本登记不追认任何 `is_evidence=false` 诊断记录为证据，不构成「实现已验收」。

## 6. 冻结件未改动的哈希证据（现算）

| 文件 | sha256（现算） | 字节 |
| --- | --- | --- |
| `docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.3 冻结预注册） | `1787075596ec5b328afe04cf4c60e0d4595ad1c9613c03b83c55421f6eb4ec94` | 104048 |

- 现算 sha256 与既有 sidecar `docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256` 逐字一致 ⇒ 冻结预注册件未被本登记改动。
- 本登记**只新建本文件**，未修改任何已有文件（含冻结件、`docs/` 内其他登记件、kit 侧设计/分析件）；未跑设备命令、未抓包、未连接设备或服务器。
- 本地提交（message `docs(n2): pre-trial registration for campaign diagnostic trial (clarifications, ring d1 decision)`），**未 push**。
