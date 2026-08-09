# E3-PHYS-PREFLIGHT process-boundary 前瞻修复（ADJ-20260808-0001 / 2026-08-08）

最后核验：2026-08-08

本文登记 `ADJ-20260808-0001`：S3/S5/S7 strict-process-boundary 探针目标语义变更——host process terminal probe 的 `PidOf` 目标从 bundle UI 进程精确改为 `<bundle>:vpn` Extension 能力进程，并同步本次 E3 sealed blocked 记录的前瞻性最小修复（UI last-known request、S4 deny 预截图、S5 探针亚秒/间隔、`operator-wait-state.json` 可轮询状态、aggregation 三态修复）。本文是 **host 侧方案/实现登记**（**非** live campaign、**非** 设备证据）：不授权任何 Live/HDC/install/device-ready 执行，不回溯改写任何历史 evidence。既有 sealed 结论保持原样：`EV-E3-PHYS1API26-20260808-0001`（及历史 0001/0002/API23）**不重判、不编辑任何字节**；下一次实机重跑必须新 campaign/evidence 且需用户重新授权。E3 未关闭，E8 保持 `CLOSED`。

## 调整记录（动态调整记录模板）

| 字段 | 内容 |
| --- | --- |
| 调整 ID | `ADJ-20260808-0001` |
| 日期与时区 | 2026-08-08（Asia/Shanghai，UTC+08:00） |
| 提出角色 | 主会话编排 + 执行子代理（本次 E3 sealed blocked 前瞻修复任务） |
| 触发证据 | `EV-E3-PHYS1API26-20260808-0001`（sealed blocked，保留不改写）；历史 `EV-E3-PHYS1API26-20260807-0002`（`reviewed-pass/blocked`，双审查 0 B/5 M）；`ADJ-20260807-0003`（host process terminal probe 与 Settings 应用信息强制停止路径） |
| 调整原因 | 旧 bundle 级 `PidOf` 在**正常 Stop 且 App UI 可见**时物理不可满足：`VpnConnection.destroy` + Extension 终止后，Entry UI 进程（bundle 主进程）仍持续运行，`pidof <bundle>` 永远非空，strict-process-boundary fallback 的连续 absent 观察在合法路径上无法成立；真正随 stop/destroy 退出的是 `<bundle>:vpn` Extension 能力进程。另修复：S3 首次 `UI_STOP_SKIPPED\|no-active-request` 与人为二次 Start 导致的 requestId 错配（UI last-known）、S4 deny 截图在 ACK+60s 后才采集无法证明拒绝画面（预截图 + 人工确认 + 无 B create 构成 deny proof）、S5 `LastProbeAt` `[string]` 转换丢亚秒导致实际间隔可低于冻结 3.0s、`scenario_aggregation.s3_clean_reactivation_proof` 对 ordered dictionary 读取恒为 false、等待阶段缺少主会话无需设备命令即可轮询的状态通道 |
| 调整内容 | 1) `PidOf` 目标从 bundle 改为 `<bundle>:vpn`（定向精确名，禁止宽泛 process list）；`BundleDump` 继续证明 bundle/主 App 仍安装存在；探针记录与 S3/S5/S7 场景记录新增 `process_target` 字段；freeze 新增必填决策字段 `process_probe_target: "<bundle>:vpn"`（进入 freeze contract hash，旧 freeze 无此字段一律拒绝）。2) UI 保留最后一次 start request id（`lastRequestId`）：bounded pending release 只清 active/pending 不清 last；Stop 在 active 为空时以 `basis=last-known-request` 使用 last-known；新 Start 覆盖 last-known；Stop 成功 / terminal rejection（含 16000001）合理清理；`startVpnExtensionAbility` resolve 文案改为 `Start request resolved` + “VPN create follows extension events”，不再显示暗示 VPN active 的 `Start resolved`。3) S4 仿照 S2：在用户点击 Deny 前确认授权/拒绝界面可见并立即 capture，随后用户点 Deny 与 ACK，完整观察 60s；预截图 + 人工确认 + 无 B create 可形成 deny proof；record 新增 `deny_screen_capture`（name/status/visible/result）。（**历史**：该人工确认/ACK 语义门被 `ADJ-20260808-0002`/`0003` 前瞻取代——新协议为点击 Deny 前机器 capture + deterministic layout gate、点 Deny 后即时 capture + 完整窗口观察，操作员只做机械动作按回车，见下方「前瞻取代标注」。）4) S5 探针调度 round-trip 精确 `DateTimeOffset`（不再 `[string]` 丢亚秒）并加 0.1s 调度裕量，实测间隔实际达到 ≥ 冻结 3.0s（阈值不降、无事后容差翻案）；连续 absent 计数达到但间隔不足时继续探而不是提前 Finished。5) 新增可轮询 `operator-wait-state.json`（EvidenceRoot 内稳定路径，主会话无需设备命令即可发现）：每次 READY/ACTION ACK/visible fact 等待落盘 scenario、phase/kind、expected token/nonce、capture_done、response_valid、updated_at，完成时写 complete；禁止 target/UDID/HAP 路径/endpoint/secret；经最终 manifest + record 引用封签。（**历史字段**：该 READY/nonce/capture_done/response_valid 字段集被 `ADJ-20260808-0002`/`0003` 的 schema v2 取代——新字段为 `scenario/step_index/step_id/expected_action/phase/capture_before/capture_after/machine_precondition/machine_postcondition/updated_at/complete/completed_at/history`，不再写 READY/nonce，见下方「前瞻取代标注」。）6) `scenario_aggregation.s3_clean_reactivation_proof` 改为按 IDictionary 索引读取并三态化：S3 实测时 true/false，S3 未探测（如早期 blocked）时 null，绝不伪装成 false。7) 明确打印 `CAPTURE_COMPLETED ... now=...` 提示，减少用户过早操作 |
| 受影响阶段 | E3（`E3-PHYS-PREFLIGHT` 唯一物理例外）、E8（保持 `CLOSED`） |
| 版本与依赖核对 | 执行 commit 为本次工作树 HEAD（含 runner/freeze example/selftest/audit/UI 变更）；新 freeze 必须绑定新 commit 与新 runner SHA-256；旧 runner/freeze hash 只绑定各自历史记录；HDC `3.2.0d` 与最终 signed HAP A/B hashes 不变（复用仅作实测、不证明 API 26 安装成功） |
| 已评估替代方案 | 保留 bundle 级 pidof + 放宽 absent 判据（不可行：bundle UI 进程在正常 Stop 后仍存在，放宽只会伪造通过）；改用 `hidumper`/全量进程列表（禁止：宽泛查询与特权命令均违反白名单）；引入跨进程文件桥/完整 UI 持久化（禁止：仅需内存态 last-known） |
| R0/SLO/补丁预算影响 | 无 |
| 重跑范围 | 下一次实机 campaign 须按治理取得用户显式设备授权 + fresh device confirmation 后以新 campaign/evidence ID 重跑；本变更只适用下一新 campaign/evidence，不回溯修改任何历史 evidence 字节 |
| T0 判定与决策依据 | 不触发 T0（E3 预检治理内的 runner/判定修正，不触及首目标、核心数据面、跨语言边界或发布门；`ADJ-20260807-0003` 已确立 host process terminal probe 路径，本调整在其边界内精确化探针目标并修复执行缺陷） |
| 生效条件 | 冻结新 freeze（含 `process_probe_target` 字段）并绑定本执行 commit 后，对下一新 campaign 生效；既有 freeze 无此字段一律拒绝 |
| 回退条件 | 若 `<bundle>:vpn` 精确名在目标元组上不可观测（pidof 无输出或进程命名不同），S3/S5/S7 strict fallback 记 blocked，不得退回 bundle 级 pidof 或伪造通过；由新路线决策决定下一步 |
| 审查状态 | host 侧 selftest `HDC_PROCESSES=0`、audit 源码约束复核、独立审查角色待 freeze 重新绑定 |

## 语义变更说明（严格进程边界）

- **旧语义**：`PidOf` 执行 `shell pidof <bundle>`，探测的是 bundle 主 UI 进程（EntryAbility 所在进程）。在正常 Stop（UI 点击 Stop → `VpnConnection.destroy` → Extension 终止）且 App UI 仍可见时，bundle 主进程**不会退出**，`pidof <bundle>` 持续返回非空。因此 S3/S5/S7 的 strict-process-boundary fallback 要求的“连续 absent host process probes”在合法路径上**物理不可满足**：要么永远 blocked（无法证明本已正确终止的 Extension），要么被迫在 finally/uninstall 后回填 absent（被明确禁止）。
- **新语义（`ADJ-20260808-0001`）**：`PidOf` 执行 `shell pidof <bundle>:vpn`，精确探测 `<bundle>:vpn` Extension 能力进程；该进程随 Extension 终止（stop/destroy/force-stop 撤销）实际退出，absent 观察才是合法终态证据。`BundleDump`（`bm dump -n <bundle>`）继续证明 bundle/主 App 仍安装存在（S3 的 bundle-present 要求与 S5 的 bundle-still-present 要求不变）。仅定向精确名 pidof；禁止宽泛 process list（`ps`/`pgrep`/`hidumper` 等）。
- **记录字段**：每个 `process-final-state-probe` 记录与 S3/S5/S7 场景记录均含 `process_target`（`<bundle>:vpn`）；freeze 必填字段 `process_probe_target: "<bundle>:vpn"` 进入 freeze contract hash（`Get-FreezeContractSha256`），缺失或异值的旧 freeze 在 DryRun/LiveSimulation/Live 全部模式拒绝。
- **适用边界**：只适用于**下一新 campaign/evidence**（新 freeze + 新 commit + 新 runner SHA-256）。`EV-E3-PHYS1API26-20260808-0001`（及全部历史记录）保持 sealed 原样，**不回溯修改**：旧记录的 bundle 级 pidof 语义是当时冻结契约的一部分，其 blocked 结论不重判。

## 本次 runner/UI/审计变更摘要

- `e3-phys-preflight-campaign.ps1`：`PidOf` 目标 `${bundle}:vpn`；`New-ProcessProbeContext.ProcessTarget`；探针与 S3/S5/S7 记录 `process_target`；S4 deny 预截图（`deny_screen_capture`）；S5 探针调度（精确 DateTimeOffset round-trip + 0.1s 裕量、间隔不足继续探）；`Write-OperatorWaitState` + `Read-OperatorResponse`/`Invoke-ScenarioObservation` 等待状态落盘 + finally `complete` 封签 + record `operator_wait_state_reference`（**历史**：`Read-OperatorResponse`/`Invoke-ScenarioObservation` 语义门与 READY/nonce 字段被 `ADJ-20260808-0002`/`0003` 强模式主路径与 schema v2 取代，见下方「前瞻取代标注」）；`scenario_aggregation.s3_clean_reactivation_proof` 三态（IDictionary 索引读取）；freeze 决策字段 `process_probe_target`（Assert-FreezeManifest / Get-FreezeContract）；`CAPTURE_COMPLETED` 提示。
- `entry/src/main/ets/pages/Index.ets`：`lastRequestId` last-known 语义（新 Start 覆盖、pending release 不清、Stop fallback `basis=last-known-request`、Stop 成功/terminal rejection 清理）；resolve 文案 `Start request resolved` + “VPN create follows extension events”；`STOP_SESSION_RELEASED_LAST_KNOWN` 新 marker；M3 bounded release 与 timer 审计约束全部保留。
- `e3-phys-preflight-freeze.example.json`：新增 `process_probe_target`。
- `tests/e3-phys-preflight-runner-selftest.ps1`：新增 C7 覆盖（UI last-known/文案、S4 预截图与 capture-先于-observation 顺序、默认 S5 间隔 ≥3.0、间隔不足继续探、`:vpn` 精确命令、wait-state 各阶段与无敏感字段/封签、aggregation 一致性），保留全部既有测试。
- `audit-physical-preflight.sh`：`assert_arkts_source` 同步新增 last-known/basis/`Start request resolved` 计数与负向约束、`STOP_SESSION_RELEASED_LAST_KNOWN` marker。

## 前瞻取代标注（`ADJ-20260808-0002` / `ADJ-20260808-0003`）

`ADJ-20260808-0001` 中以下描述被 `ADJ-20260808-0002`（[强可靠操作员信任模型](e3-physical-preflight-operator-trust-2026-08-08.md)）与 `ADJ-20260808-0003`（[强可靠执行细化](e3-physical-preflight-operator-trust-2026-08-08.md)）**前瞻取代**，仅属当时 host 侧实现登记，**不得作为下一新 campaign 的当前协议**；下述历史事实仍为本记录的真实内容并保持不变：

- **S4 人工确认 / ACK**：原“在用户点击 Deny 前确认授权/拒绝界面可见并立即 capture，随后点 Deny 与 ACK；预截图 + 人工确认 + 无 B create 构成 deny proof”中的人工确认/ACK 语义门被取代。新协议（`ADJ-20260808-0002`/`0003`）：点击 Deny 前机器 capture + deterministic layout gate（操作员不判断屏幕；错误/缺失授权 UI → scenario invalid）；点 Deny 后即时 capture + 完整窗口观察；操作员只做机械动作并按回车。deny proof = 机器预截图 layout + 无 B create（窗口内无可观察 reject/error 时）。
- **READY / ACK nonce / `Read-OperatorResponse`**：`ADJ-20260808-0002` 的 Live 主路径（`Invoke-StrongLiveCampaign`）删除 `Read-OperatorResponse` / `Confirm-VisibleFact` / READY / ACK nonce 语义门；`Read-Host` 仅接收空回车作为机械完成；`operator-attestation` 只记录机械步骤完成时间，不参与 scenario/overall 结果。
- **`operator-wait-state.json` 字段**：原“scenario/phase/kind/expected/nonce/capture_done/response_valid/updated_at”字段集被 schema v2 取代（runner 中已实现）：`scenario, step_index, step_id, expected_action, phase(waiting|operator-complete|verifying|captured|invalid|complete), capture_before, capture_after, machine_precondition, machine_postcondition, updated_at, complete, completed_at, history`；**不再写 READY/nonce 等旧字段**。

`ADJ-20260808-0001` 的 process-target / UI last-known / S5 探针调度（≥3.0s）/ aggregation 三态等底层修复**仍然有效**，并被 `ADJ-20260808-0002`/`0003` 强可靠主路径消费（`ADJ-20260808-0003` 另加 process-target verify：CREATE_ACCEPTED 后精确 `pidof <bundle>:vpn` present 校验命名元组，S3/S5/S7 只消费已校验 checkpoint）。

## 门状态

- E3 未关闭；本记录是 host 侧方案/实现登记，不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- 本记录 **不** 授权任何 Live/HDC/install/device-ready 执行；`plan_status: blocked-awaiting-device-authorization`。
- candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`、未 Live；candidate IDs 保留，`ready` freeze 可绑定同候选身份但**仅**在用户显式设备授权 + fresh device confirmation 后按治理决定重生，不能原地改 candidate、不能复用已消费 ID；新 freeze 必须含 `process_probe_target` 并绑定新 commit/新 runner SHA-256。
- 本登记期间禁止 HDC/设备命令。
