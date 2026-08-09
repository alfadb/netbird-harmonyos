# E3-PHYS-PREFLIGHT 强可靠操作员信任模型（ADJ-20260808-0002 / ADJ-20260808-0003 / 2026-08-08）

最后核验：2026-08-08

本文登记 `ADJ-20260808-0002`：用户明确选择 **强可靠模式**——Live 主路径场景 1–7 采用 `mechanical-action-only-machine-verified-v1` 操作员信任模型与 `stop-and-finally-cleanup-seal` 场景 invalid 策略；并在下节登记其执行细化 `ADJ-20260808-0003`（真实 layout 校准 / prompt-time 事件窗口 / 空档 UI action guard / infra capture 分类 / S5 去除非决定性 VPN 页步骤 / process-target verify）。本文是 **host 侧方案/实现登记**（**非** live campaign、**非** 设备证据）：不授权任何 Live/HDC/install/device-ready 执行，不回溯改写任何历史 evidence。既有 sealed 结论保持原样：`EV-E3-PHYS1API26-20260808-0001`（及历史 0001/0002/API23）**不重判、不编辑任何字节**；`ADJ-20260808-0001` 的 process-target / last-known / S5 探针间隔等底层修复全部保留。下一次实机重跑必须新 campaign/evidence 且需用户重新授权。E3 未关闭，E8 保持 `CLOSED`。

## 调整记录（动态调整记录模板）

| 字段 | 内容 |
| --- | --- |
| 调整 ID | `ADJ-20260808-0002` |
| 日期与时区 | 2026-08-08（Asia/Shanghai，UTC+08:00） |
| 提出角色 | 用户明确选择强可靠模式；主会话编排 + 执行子代理落地 |
| 触发证据 | 既有 operator 三态 / READY-ACK nonce / 人工可见事实确认在 S6 双 active、S4 deny、S5 force-stop、S7 cleanup 上把语义判定交给操作员，无法形成强可复核的机器证据链；`ADJ-20260808-0001` 已修复 process-target 等底层问题但未独立登记 operator trust model |
| 调整原因 | 需要：操作员每步只做机械动作；机器判定 request/事件/layout/进程；首个 scenario invalid 立即停止后续场景；overall 优先级 integrity invalid > scenario invalid > fail > blocked > pass；finally cleanup+seal 不改变 overall |
| 调整内容 | 见下文「协议要点」与「实现摘要」 |
| 受影响阶段 | E3（`E3-PHYS-PREFLIGHT` 唯一物理例外）、E8（保持 `CLOSED`） |
| 版本与依赖核对 | 执行工作树含 runner/freeze example/selftest/docs；新 freeze 必须绑定新 commit 与新 runner SHA-256，并含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` |
| 已评估替代方案 | 保留 READY/ACK + 三态确认（不可行：语义仍人工）；半自动 UI 注入（禁止：`uiInput`/特权）；只改文档不改 runner（不可复核） |
| R0/SLO/补丁预算影响 | 无 |
| 重跑范围 | 只适用**下一新 campaign/evidence**；历史 sealed evidence 不改；`EV-E3-PHYS1API26-20260808-0001` 保持 blocked 原样 |
| T0 判定与决策依据 | 不触发 T0（E3 预检治理内的 runner/判定重构；用户直接选择强可靠模式） |
| 生效条件 | 冻结新 freeze（含 operator trust 字段）并绑定本执行 commit 后，对下一新 campaign 生效 |
| 回退条件 | 用户撤销强可靠模式选择，或机器 layout/事件在目标元组上不可观测导致系统性 invalid 时，由新路线决策界定，不得回退为人工三态作为当前规则 |
| 审查状态 | host 侧 selftest `HDC_PROCESSES=0`、runner `-SelfTest`、parser 通过；独立审查角色待 freeze 重新绑定 |

## 协议要点（强可靠模式）

### 操作员信任模型

- freeze 必填：`operator_trust_model = mechanical-action-only-machine-verified-v1`
- freeze 必填：`scenario_invalid_policy = stop-and-finally-cleanup-seal`
- freeze 必填：`layout_verification_profile = deterministic-layout-v1`
- freeze 必填：`vpn_conflict_rejection_codes = [2203002]`
- Live 主路径 **不得** 调用 `Read-OperatorResponse` / `Confirm-VisibleFact` / READY / ACK nonce 语义门
- 操作员每步只看到：`现在只做：X。完成后按回车。`
- `Read-Host` 仅接收空回车/任意输入作为**机械完成**；不解析 y/n/token
- `operator-attestation` 仅记录机械步骤完成时间，**不参与** scenario/overall 结果

### invalid 语义

- 操作偏差 / 错误屏幕 / 额外 Start/Stop / `UI_STOP_SKIPPED` / 错误 requestId / 错误次序 → **scenario invalid**
- 授权 UI 在 decisive gate 上 capture/layout 无法收集或确定性不匹配 → **invalid**（操作偏差或错误屏幕）
- 平台按协议确实无授权 UI 的 blocked 路径须单独文档化；本 checkpoint 本身不向操作员索取语义确认
- 首个 scenario invalid 后：**停止**后续场景；record 未执行场景 `result=invalid` + `reason=not-run-due-to-invalid`（schema 仍为七场景）
- overall 优先级：`integrity invalid` > `scenario invalid` > `fail` > `blocked` > `pass`
- finally cleanup + seal **仍执行**且 **不能**改变 overall

### 分场景机器判定

1. **S1**：完全机器 install/cleanup baseline；无操作员语义门
2. **S2**：Start → 机器 layout 确认授权页 → Allow → 机器 layout 确认授权消失 + create terminal；点击 Allow 前 capture+layout deterministic gate
3. **S3**：仅消费 S2 机器验证的 request；任何额外 Start、`UI_STOP_SKIPPED`、错误 requestId → invalid；不得用 lastRequestId 掩盖协议偏差
4. **S4**：Start → 机器 layout 确认授权页 → Deny → 完整窗口；无人工 `DENY-SCREEN-CAPTURED`；pass = 可观察 reject **或** 预截图 layout + 完整窗口无 B create
5. **S5**：原子步骤（Start / 可选 Allow / VPN 页 / 应用信息 / 强制停止）；每步回车后机器 capture/layout gate；force-stop 后 `:vpn` absent + bundle present；无人工技术事实
6. **S6**：唯一 A `CREATE_ACCEPTED`；唯一 B `CREATE_REJECTED` 且明确冲突码 `2203002`；无双 accepted、无意外 Start/Stop/错误次序；**删除** `no_dual`/`dual` 作为判据（schema 可 deprecated 但必须 null/non-authoritative）；任何额外操作 → invalid 并停止后续
7. **S7**：删除 `FINAL-CLEANUP` 确认；绑定 S6 机器验证的 A request/bundle；预期 `UI_STOP` / `onDestroy` / pre-destroy / destroy-begin 与 `:vpn` 终态；错误 bundle stop / 额外 start → invalid；finally cleanup 不回填

### operator-wait-state

字段符合（schema v2）：`scenario, step_index, step_id, expected_action, phase(waiting|operator-complete|verifying|captured|invalid|complete), capture_before, capture_after, machine_precondition, machine_postcondition, updated_at, complete, completed_at, history`  
无 target/UDID/HAP path/endpoint/secret；最终 manifest 与 record 绑定。

## 历史规则标注（historical）

以下表述 **仅历史**，**不是** 当前新 campaign 规则：

- operator 三态确认（`NO-DUAL-ACTIVE-CAPTURED` / `DUAL-ACTIVE-CAPTURED`）
- READY / ACK nonce 令牌协议
- `Confirm-VisibleFact` / 人工 `DENY-SCREEN-CAPTURED` / `FINAL-CLEANUP-CAPTURED` / `SETTINGS-APP-INFO-FORCE-STOP-CAPTURED` 作为结果输入
- S6 依赖人工“未双 active”确认

`ADJ-20260808-0001` 的 process-target / UI last-known / S5 探针间隔 / aggregation 三态 **仍然有效**，并被本 ADJ 的强可靠主路径消费。

## 实现摘要

- `e3-phys-preflight-campaign.ps1`：`Invoke-StrongLiveCampaign` 为 Live 唯一主路径；删除旧 `Invoke-LiveCampaign` 与语义确认调用；`Read-OperatorEnter` + `Invoke-MechanicalStep` + 机器 layout checkpoint；`Throw-ScenarioInvalid` + not-run-due-to-invalid；S6 机器冲突码；S7 无 FINAL-CLEANUP
- freeze example：`operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes`
- selftest/live-simulation 对抗：S3 额外/SKIPPED、S4 授权页缺失、S6 错误次序、S7 错误 bundle/request、无事件回车/超时、layout mismatch、integrity tamper → overall invalid/停止后续/cleanup 执行
- 正路径：每步 wait-state 前后/机器后置、无旧 token 文本、manifest 绑定、敏感字段缺失、HDC sentinel 0

## ADJ-20260808-0003：强可靠执行细化（真实 layout 校准 / prompt-time 事件窗口 / 空档 UI action guard / infra capture 分类 / S5 去除非决定性 VPN 页步骤 / process-target verify / C6 真实 shape 与连续 capture infra 传播）

本文登记 `ADJ-20260808-0003`：在 `ADJ-20260808-0002` 强可靠模式上的执行细化，随 runner/selftest 落地。本文是 **host 侧方案/实现登记**（**非** live campaign、**非** 设备证据）：不授权任何 Live/HDC/install/device-ready 执行，不回溯改写任何历史 evidence。既有 sealed 结论保持原样：`EV-E3-PHYS1API26-20260808-0001`（及历史 0001/0002/API23）**不重判、不编辑任何字节**。下一次实机重跑必须新 campaign/evidence 且需用户重新授权。E3 未关闭，E8 保持 `CLOSED`。**C6 修订**（同一 ADJ 内的前瞻细化，仍只适用下一新 campaign）：机器 layout 档案按 API26 sealed raw 与仓库 EMU 样本的真实 **`attributes`/`children` 顶层数组 shape** 校准（`Get-LayoutFacts` 产出 `$[n][.children[m]...].attributes.<field>=<value>`），`Test-CapturedLayoutProfile` 用通用精确 fact 正则匹配任意深度 `attributes.<field>`（不依赖自造 `window`/`resourceId`；entry 仍要求 ExpectedBundle + `start-vpn`/`stop-vpn` id/key；settings-app-info 不要求 ExpectedBundle，A correctness 由 A process effect gate 证明）；连续 capture infra 传播（`CampaignCapture.Degraded` 时按 raw-hilog 条目 `category`/`infrastructure_reason` 分类，`Wait-MachineCondition`/`Invoke-Capture`/`Invoke-MechanicalStep`/直接 capture 判断均区分 `LastCaptureInfrastructure`，infra → blocked + `hdc-usb-interruption` retry、绝不 scenario invalid）；S6 非冻结 B 拒绝码 → 完成 observation、记录 blocked `B-conflict-code-not-frozen:<code>`、S7 `not-run-after-platform-blocked`、绝不 invalid。

### 调整记录（ADJ-20260808-0003 动态调整记录模板）

| 字段 | 内容 |
| --- | --- |
| 调整 ID | `ADJ-20260808-0003` |
| 日期与时区 | 2026-08-08（Asia/Shanghai，UTC+08:00） |
| 提出角色 | 主会话编排 + 执行子代理（`ADJ-20260808-0002` 强可靠模式的执行细化；runner/selftest 已落地） |
| 触发证据 | `ADJ-20260808-0002` 强可靠模式落地后，机器 layout 档案 / 事件窗口 / 额外 UI action / 采集失败分类 / S5 VPN 页步骤 / process-target 校验仍需精确化；Settings app-info 页真实结构尚未采样 |
| 调整原因 | 强可靠模式要求机器判定 request/事件/layout/进程；需要真实 layout 校准、prompt-time 事件窗口、空档 UI action guard、infra capture 分类，去除非决定性 Settings>VPN 页步骤，并校验 `<bundle>:vpn` 命名元组 |
| 调整内容 | 见下文「协议要点（ADJ-20260808-0003 执行细化）」 |
| 受影响阶段 | E3（`E3-PHYS-PREFLIGHT` 唯一物理例外）、E8（保持 `CLOSED`） |
| 版本与依赖核对 | 执行工作树含 runner/selftest 的 `ADJ-20260808-0003` 标注；新 freeze 必须绑定新 commit 与新 runner SHA-256，并含 `operator_trust_model` / `scenario_invalid_policy` / `layout_verification_profile` / `vpn_conflict_rejection_codes` / `process_probe_target` |
| 已评估替代方案 | 保留 Settings>VPN 页作为 decisive 步骤（不可行：该页非决定性且增加操作员负担）；全部 capture 失败一律 invalid（不可行：基础设施中断会被误判为操作员偏差）；不校验 `<bundle>:vpn` 命名元组直接消费（不可行：命名不成立时 strict fallback 无法证明） |
| R0/SLO/补丁预算影响 | 无 |
| 重跑范围 | 只适用**下一新 campaign/evidence**；历史 sealed evidence 不改；`EV-E3-PHYS1API26-20260808-0001` 保持 blocked 原样，不回溯 0001 |
| T0 判定与决策依据 | 不触发 T0（`ADJ-20260808-0002` 强可靠模式内的执行细化；不触及首目标、核心数据面、跨语言边界或发布门） |
| 生效条件 | 冻结新 freeze 并绑定本执行 commit 后，对下一新 campaign 生效 |
| 回退条件 | 用户撤销强可靠模式，或机器 layout/事件在目标元组上不可观测导致系统性 invalid 时，由新路线决策界定；不得回退为人工三态作为当前规则 |
| 审查状态 | host 侧 selftest（`HDC_PROCESSES=0`）、runner `-SelfTest`、parser 通过；独立审查角色待 freeze 重新绑定 |

### 协议要点（ADJ-20260808-0003 执行细化）

- **真实 layout 校准（C6 修订：真实 attributes/children shape）**：机器 layout 档案按 sealed raw layout facts 校准（只读）。API26 sealed raw 与仓库 EMU 样本实际均为**顶层数组**，每节点 `{ attributes: { bundleName, type, id, key, text, ... }, children: [...] }`；`Get-LayoutFacts` 产出 `$[n][.children[m]...].attributes.<field>=<value>`（单元素顶层数组 JSON round-trip 后根为 `$.attributes.*` / `$.children[m].attributes.*`）。`Test-CapturedLayoutProfile` 用**通用精确 fact 正则**匹配任意深度的 `attributes.<field>`，可兼容未来直接字段但**绝不**依赖自造 `window`/`resourceId` 结构——
  - `authorization`：任一节点 `attributes.bundleName=com.huawei.hmos.vpndialog`、任一节点 `attributes.type=Dialog`（API26 实际 Dialog 在 child）、标题/问题文本（允许合理空白/应用名变体但须含 `允许`+`VPN`）、Allow/允许、Cancel/Deny/取消/拒绝。request bundle **绝不**要求在授权页出现：request ownership 由 UI_START/事件门证明，不由页面证明；A/B bundle 也不要求。
  - `settings-vpn`：任一节点 `attributes.bundleName=com.huawei.hmos.settings`；VPN 组资源匹配 `attributes.id/key = Setting.MobileNetwork.vpn_group_group.vpn_settings`（页已非决定性，仅 owner + 资源 id/key 为必需）；仅含文本 `VPN` 的普通 App 页**绝不**匹配。
  - `settings-app-info`：**无可信设备样本**（末次 `scenario-5-app-info-force-stop` 实为 App B 页），故意保守：任一节点 `attributes.bundleName=com.huawei.hmos.settings` + 精确 A 应用标签 `E3 Physical VPN Preflight` + Force Stop / 强制停止 控件。**去掉**未采样的 `app-info-structure` `attributes.id/key` 要求（避免系统性地 fail-closed 于未采样结构）。**不要求 ExpectedBundle**：A/B 同名不导致假 pass——A correctness 由 A process effect gate 证明（force stop 后 A `<bundle>:vpn` absent 且 A bundle present），不由 Settings 页上的 bundle 字段。无匹配 → **invalid**；残余风险登记于本 ADJ，**下一 Live fail-closed** 待可信设备样本。
  - `entry`：**仍要求 ExpectedBundle + 按钮 id/key `start-vpn`/`stop-vpn`**（真实 EMU entry 样本 Button 节点带 `id`/`key`），不止全屏文本。
- **连续 capture infra 传播（C6 修订）**：ScreenCap / DumpLayout / Receive exit 124/125 / timeout / HDC transport → **infrastructure**（全局 CaptureDegraded + `infrastructure_reason=hdc-usb-interruption`，blocked + 单次 infrastructure retry 授权），**绝不** scenario invalid。`Invoke-Capture` 遇连续 `CampaignCapture.Degraded` 时读取该 capture/全局 CaptureDegraded 的 raw-hilog 条目的 `category`/`infrastructure_reason`，infra 则设 `LastCaptureInfrastructure=true` 并传播；`Wait-MachineCondition` 见 `Capture.Degraded` 时 infra → 抛可分类 infrastructure blocked、非 infra → blocked（**不得** status invalid）；`Invoke-MechanicalStep` 对 blocked 抛普通 runner blocked 异常使 record blocked（**不** `ThrowScenarioInvalid`）；S5/S6 A reactivation 走 `Invoke-LayoutChoiceCheckpoint` dual-profile（entry/authorization）8s 同名重采，infra/continuous capture 仍先区分 `LastCaptureInfrastructure`。仅非基础设施采集损失 invalid。
- **dual-profile 8s 重采 + process precondition blocked（C6 修订）**：S5/S6 A reactivation 用 `Invoke-LayoutChoiceCheckpoint` 对 entry 与 authorization 双档案判定，同名 `Invoke-Capture -Replace`、8 秒内每 1 秒重试；任一 profile pass 即返回 `selected_profile`/`attempts` 并写 transcript，最终双 mismatch 才 scenario invalid。`Get-ExactProcessCheckpoint` 所有非 pass 均为 `status=blocked`（HDC 124/125/timeout/offline → reason 含 `hdc-usb-interruption` 并设全局；present/absent mismatch 或 unknown 亦 blocked）；`Invoke-MechanicalStep` 仅 `MachinePrecondition.status=invalid` 才 `ThrowScenarioInvalid`，blocked 抛 `machine-precondition-blocked ...`（infra 文本可被 `Get-FailureClassification` 识别）——process mismatch **绝不** operator invalid。
- **prompt-time 事件窗口**：`relative_to_prompt=true` 的 pre-enter 事件以 prompt 时间为基准打戳（0.2s+offset），慢操作员（action_delay ≥8s）不会把设备事件推到回车之后；其余事件保持 completed-at（post-enter）语义；验证的事件时间下界为 prompt（含冻结时钟偏移容差）。
- **空档 UI action guard**：跨步/跨场景 guard——上一验证点之后、不属于当前机械步骤的 UI_START / UI_STOP / UI_STOP_SKIPPED → 立即 scenario invalid（下一个 prompt 之前）；auto StartEntry ENTRY 事件不是 UI action、不触发 guard；`VerifiedRequests` 为全局唯一性/归属寄存器。
- **step 超时分类**：`Wait-MachineCondition` pending 超时时按已出现的动作分类——期望的机械 UI action（`UI_START`/`UI_STOP`）**从未出现** → **invalid** `mechanical-action-missing`（操作员动作未注册，不是平台结果）；正确 action 已出现但平台后续 marker（create/destroy terminal）缺失 → **blocked** `platform-marker-missing`（平台/runner 不确定，绝不 invalid）。
- **S5 去除非决定性 VPN 页步骤**：Settings>VPN 页不再询问操作员（`settings_vpn_page_capture=not-required`，绝不作为 invalid/block/pass 输入）；S5 机械步骤仅 app-info + 强制停止；`settings_vpn_page_observation_only` 保留为 not-required。
- **process-target verify**：CREATE_ACCEPTED 后精确 `pidof <bundle>:vpn` present 校验（`Get-ProcessTargetCheckpoint`）证明命名元组确实解析为 live Extension 进程；absent/unknown/error → blocked `process-target-unverified`，绝不 pass；S3/S5/S7 只消费该已校验 checkpoint。
- **S6 A 可选 reauthorization**（同 S2/S5）：A Start 后 `Invoke-LayoutChoiceCheckpoint` dual-profile 8s 同名重采，判断 entry（direct activation）或 authorization（reauthorization UI）；若 authorization → 机械单步点 Allow（after capture、无额外 UI 动作 guard）再等待 A create terminal。S6 A 终态分类同 S2：纯 `START_PROMISE_REJECTED` / 无 `VPN_ONCREATE` 的授权层结果 → S6 **blocked** `authorization-outcome-unclassified`、S7 `not-run-after-platform-blocked`（绝不 fail、绝不 scenario invalid）；仅 `VPN_ONCREATE` 后 `VPN_CREATE_REJECTED`/`VPN_CREATE_INVALID_FD` → 功能 fail；`CREATE_ACCEPTED` 继续正常。
- **S6 机器判定细化（C6 修订：非冻结码不 invalid）**：A create rejected/invalid fd（`VPN_ONCREATE` 后）→ 功能 fail（非操作员 invalid，不请求 B、S7 `not-run-after-functional-fail`、finally cleanup 仍执行）；B 拒绝码**非冻结码** → 完成当前 observation、记录 S6 blocked `B-conflict-code-not-frozen:<code>`、S7 `not-run-after-platform-blocked`/blocked、返回并 finally cleanup+seal（**绝不** `ThrowScenarioInvalid`；仅额外操作/错误 request/order 才 invalid）；B 被拒后 B Extension 进程可能仍存活，仅 observed、绝不要求 absent；A 侧窗口末 `:vpn` present 校验保持。
- **operator-wait-state**：schema v2（`scenario/step_index/step_id/expected_action/phase(waiting|operator-complete|verifying|captured|invalid|complete)/capture_before/capture_after/machine_precondition/machine_postcondition/updated_at/complete/completed_at/history`）；无 READY/nonce 旧字段；无 target/UDID/HAP path/endpoint/secret；最终 manifest 与 record 绑定。

### 实现摘要（ADJ-20260808-0003）

- runner：`Test-CapturedLayoutProfile` 档案按真实 `attributes/children` 通用 fact 正则校准（无 `window`/`resourceId` 依赖；entry 需 ExpectedBundle + `start-vpn`/`stop-vpn` id/key；settings-app-info 仅 owner+精确 A 标签+Force Stop 控件，无 ExpectedBundle、无 app-info-structure id/key 要求，A correctness 由 force-stop 后 A `:vpn` absent + bundle present 的 A process effect gate 证明）；`Invoke-LayoutChoiceCheckpoint` dual-profile 8s 同名重采（S5/S6 A reactivation）；`Get-CaptureDegradedInfra` + `Wait-MachineCondition`/`Invoke-Capture` 的连续 capture infra 传播；step 超时分类（UI action missing → invalid `mechanical-action-missing`，正确 action 后平台 marker missing → blocked `platform-marker-missing`）；S6 A 可选 reauthorization（dual-profile 判定 + 单步 Allow + `authorization-outcome-unclassified` blocked）；S6 非冻结码 blocked 路径（完成 observation + 记录 blocked + S7 not-run-after-platform-blocked）；`Get-ExactProcessCheckpoint`（非 pass 一律 blocked，非 operator invalid）/ `Get-ProcessTargetCheckpoint`（`:vpn` 校验）；`Invoke-MechanicalStep` 仅 precondition `invalid` 才 scenario invalid；`relative_to_prompt` prompt-time 打戳；`OperatorActionGuardFrom` 空档 guard；`Add-CaptureDegradation -Category infrastructure`；S5 去 VPN 页步骤 + app-info/force-stop 机器步。
- selftest 对抗：layout mismatch（错误 App 页/仅含 VPN 文本/wrong-bundle entry）、真实 attributes/children 授权 fixture（API26 最小形状 + historical 深 child 形状）、settings-app-info 正向最小结构（无 app-info-structure id/key）+ wrong-B/no-effect force-stop 保持 invalid、连续 capture process/stderr 基础设施传播（决定性 step → overall blocked + `hdc-usb-interruption` + 无 scenario_invalid）、S5/S6 dual-profile `layout_ready_delays=5` authorization 收敛 + 同名 capture ref=1、S2 process precondition PidOf infra / mismatch → blocked 无 scenario_invalid、S6 非冻结码 `2203001` blocked、S6 A extension rejected fail、S6 A `START_PROMISE_REJECTED` blocked、S6 A reauth success、`:vpn` present 校验失败、wait-state tamper → 相应 invalid/blocked/停止后续。

## 门状态

- E3 未关闭；本记录是 host 侧方案/实现登记，不是 campaign `reviewed-pass/pass`
- E8 保持 `CLOSED`
- **不** 授权任何 Live/HDC/install/device-ready 执行
- `plan_status: blocked-awaiting-device-authorization`；candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 保持 blocked 且不因 `ADJ-20260808-0002`/`ADJ-20260808-0003` 重判
- 本登记期间禁止 HDC/设备命令
