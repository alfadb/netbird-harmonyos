# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0001）

最后核验：2026-08-16

本文历史登记曾依据用户（直接人类决策者）于 2026-08-16 的决定建立 `AUTH-E3-PHYS1API26-20260816-0001`、campaign `E3-PHYS-PREFLIGHT-20260816-0001` 与 evidence `EV-E3-PHYS1API26-20260816-0001`，`attempt: initial`。该 pair 执行到 gate 3 静态独立审查时，因 blocked freeze 绑定的旧 source provenance 与冻结 HAP 的实际 source basis 不一致而停止；现由 [`AUTH-E3-PHYS1API26-20260816-0002`](e3-physical-preflight-authorization-2026-08-16-0002.md) 取代。

> 最终状态：`authorization_status: governance-review-blocked-retired`，not ready，reusable false。gate 1-2 已形成治理前置与 audit-1；gate 3 blocked freeze 已形成但审查为 0 blocker / 1 major，根因是旧 source mismatch。gate 4-13 全部 `not-run`，没有执行 HDC、设备命令、TargetBindingConfirm、ready freeze、audit-2、DryRun 或 Live，因此 0001 **不是 Live consumed**；但治理失败已使该 AUTH/pair 永久退役，绝不可复用、补跑、重判或作为 retry basis。

> 受保护仓外字节保持不变：audit-1 SHA-256 `a1275d416ff120c26a6de1eaaa4f7f9fde931a7f2a674401d43c1cdd5c24695d`；blocked freeze SHA-256 `70e5a1d56b74595a25c93500d7e422021e3754e66798bbb2a43a82f2e80b675a`；其 confirmation contract SHA-256 `f3ca48c79d185e0191b774bfc4be935c78c886cca88e5aeedf6e93a8d643f118`。这些对象只作历史/provenance 保留，不得修改或复制为新 campaign 输入。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260816-0001
supersedes: AUTH-E3-PHYS1API26-20260815-0005
superseded_by: AUTH-E3-PHYS1API26-20260816-0002
exception: E3-PHYS-PREFLIGHT
information_status: historical-governance-review
record_status: retired-before-live
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: stopped-at-gate-3-governance-review
is_evidence: false
authorization_status: governance-review-blocked-retired
plan_status: governance-review-blocked-retired
ready: false
reusable: false
device_readiness: user-attested-ready
machine_fresh_confirmation: not-run
independent_review: blocked-0B-1M-old-source-mismatch
blocked_confirmation_freeze: created-preserved-review-blocked
ready_freeze: not-run
dry_run: not-run
live: not-run
live_consumed: false
campaign_status: governance-review-blocked-retired
code_basis_commit: 9c1d464fa78214dee7c1c00f4870aa26549af1a2
head_requirement: exact-clean-final-governance-commit-descending-from-code-basis
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0001
  evidence_id: EV-E3-PHYS1API26-20260816-0001
  attempt: initial
  retry: N/A
  identity_status: retired-after-gate-3-review-blocked
  consumed: false
  live_consumed: false
  reusable: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260815-0005
  evidence_id: EV-E3-PHYS1API26-20260815-0005
  authorization_id: AUTH-E3-PHYS1API26-20260815-0005
  status: sealed-blocked-consumed
  reusable: false
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
record_paths:
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260816-0001.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260816-0001.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260816-0001
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260816-0001
protected_gate_objects:
  audit_1_sha256: a1275d416ff120c26a6de1eaaa4f7f9fde931a7f2a674401d43c1cdd5c24695d
  blocked_freeze_sha256: 70e5a1d56b74595a25c93500d7e422021e3754e66798bbb2a43a82f2e80b675a
  confirmation_contract_sha256: f3ca48c79d185e0191b774bfc4be935c78c886cca88e5aeedf6e93a8d643f118
  review_result: 0 blocker / 1 major
  review_root_cause: old source provenance does not match actual frozen-HAP source basis
  gate_4_through_13: not-run
```

`code_basis_commit` 是当时 S6 B 修复的已提交代码基础。0001 blocked freeze 的 `code_sha` 与仓外字节只作为历史锚点保留；由于 source provenance mismatch，0001 不得再生成 ready freeze，也不得把任何现场值补写到本 pair。修正后的双 SHA 绑定见 0002 登记。

## 退役处置与适用边界

0001 的失败发生在 gate 3 governance review，不是 device、runner、S6 或 Live 结果。旧 source archive/manifest 指向的 source basis 不能证明冻结 HAP A/B 来自该源码；因此 review 的唯一 major 足以 fail closed。保留 audit-1 与 blocked freeze 原字节，是为了让失败链可复算，而不是允许继续执行。

## 0005 处置与适用边界（历史）

以下条目只记录 0001 建立时对 0005 与当时新 pair 的处置规则；0001 退役后不构成当前授权或待执行指令，当前治理只以 0002 登记为准。

1. 0005 sealed evidence 固定为 S1-S5 pass、S6 blocked（`scenario-6 machine-verification-blocked step=3 reason=platform-marker-missing:B-create-terminal-missing`）、S7 blocked（`not-run-after-runner-failure`），overall/verdict blocked。该历史事实不因后续授权改变。
2. 0005 AUTH/pair 当时已是 `sealed-blocked-consumed`、不可复用；20260816-0001 当时登记为全新 initial、不是 retry。0001 后来也已在 gate 3 退役；后续尝试的治理要求由取代它的授权登记另行规定。
3. 当时对 0001 pair 的计划仅允许完整 S1-S7 campaign 并要求从 13 门 gate 1 开始，禁止局部 S6 重放、跳门、复用旧 confirmation/review/freeze/DryRun/Live 输出或原地修改旧对象；该计划已在 gate 3 终止。
4. 当时 S6 B 修复代码基础固定为 commit `9c1d464fa78214dee7c1c00f4870aa26549af1a2`，拟议最终执行 HEAD 需包含该 commit、本登记和当时常量迁移，并在相应 gates 保持 exact 且 clean；0001 未形成该可执行最终 HEAD。
5. 0001 处置时 E3 仍 open、E8 保持 `CLOSED`；0001 不产生开放 E8 的效力。

## S5 应用详情路径

S5 的操作路径是系统设置中的目标应用详情页：从设置的应用管理/应用列表进入 `E3 Preflight A` 的应用详情，然后点击 `强行停止`，完成随后出现的确认（如有）。操作员不得使用设置搜索页，不得从 VPN 设置页、隐藏导航、最近页面或历史文本推断目标，也不得依靠 runner 自动点击。

机器门只接受 Settings owner 下唯一可见 `Setting.AppDetail` 子树：候选自身必须 `visible=true`，Settings owner 到候选的所有祖先均不得 `visible=false`；同一子树内必须同时存在 distinct label `E3 Preflight A` 与 force-stop 控件。子树外标签、隐藏祖先、搜索页和历史文本不能命中。post-force capture 仅 observation-only；S5 最终撤销仍只由连续 `<bundle>:vpn` absent（至少 2 次、间隔至少 3 秒）且 bundle present 的既有决定性门确认。

## S6 实际流程与操作员动作

S6 的机械步骤固定为：

1. **A Start，step 1**：runner 打开 A Entry 并通过布局门后，操作员只点击 `E3 Preflight A` 的 Start，然后按回车。不得点击 Stop、重复 Start 或执行其他 UI 动作。
2. **A optional Allow，step 2**：仅当机器布局门显示 A 的 VPN authorization dialog 时，runner 才提示该步；实际 prompt 是 `点击 Allow`，设备上的对应控件文字为 `允许`。操作员只点击该控件，然后按回车；若没有该提示，禁止自行寻找或点击授权。
3. **B Start，step 3**：A create terminal 与 A exact `<bundle>:vpn` active pre-gate 通过后，runner 打开 B Entry；操作员只点击 `E3 Preflight B` 的 Start，然后按回车。
4. **B optional Allow，step 4**：B Start 后 runner 对 entry/authorization 双档案分流。仅当机器识别到 B authorization profile 时才提示 `点击 Allow`；设备上的对应控件文字为 `允许`。操作员只点击该控件，然后按回车。若机器识别 entry profile，则没有 step 4，禁止额外点击。

B authorization profile 的 production 来源是 0005 sealed Live 的实际 post-B-Start raw 文件 `capture-scenario-6-conflict.json`，raw SHA-256 `7f6e44d5eab7021d192a6a61f409af9a3e8507f16c1df6bcf84755c1130bb72c`；`RAW-capture-scenario-6-conflict.json` 是 sealed public reference，不是实际 basename。仓内最小脱敏 fixture 为 `tests/fixtures/s6-b-authorization-production-0005.json`，SHA-256 `8d99c65e6bfa0c9569848f6a482fd188c1bcb6b6837ca0e8386c052c33f376ad`。文件名 0005 是不可迁移的 provenance，不表示复用 0005 pair。

S6 terminal 判定优先级固定如下：

1. integrity violation 或场景操作/事件契约 invalid 最高优先；foreign accepted、`requestId=missing` accepted、额外 Start/Stop、重复/错序动作均不能降级成普通 blocked。
2. 窗口内 B `CREATE_ACCEPTED` 或 A+B 双 accepted 是功能 fail，优先于终态 process mismatch、rejected checkpoint blocked 与 nonfrozen rejection blocked。
3. accepted/rejected terminal 后统一执行一次 process checkpoint；accepted checkpoint 只记录 observation，rejected checkpoint 才参与 gate。
4. 只有绑定已验证 A/B requestId 的唯一 A `CREATE_ACCEPTED` 加唯一 B `CREATE_REJECTED`，且 rejection code 精确为 `2203002`、A exact-process 仍可验证，S6 才可满足功能门。
5. 非冻结 rejection code、rejected terminal 后 A 不可验证或一般机器不确定性为 blocked；没有 B terminal 也是 blocked。terminal tag 关联容错只用于识别 terminal，不放宽 accepted marker 的 requestId 计数。

## 22-operation 白名单与安全边界

runner HDC 白名单精确为 22 项：`Version`、`TupleModel`、`TupleBuild`、`MkdirStaging`、`RemoveStaging`、`StagingProbe`、`SendA`、`SendB`、`InstallA`、`InstallB`、`FaultA`、`FaultB`、`HilogStream`、`BundleDump`、`PidOf`、`Uninstall`、`StartEntry`、`ScreenCap`、`DumpLayout`、`ReceiveScreen`、`ReceiveLayout`、`ForceStop`。参数、A/B bundle、capture name 与 argv 继续 exact allowlist；`ForceStop` 只允许 `exception-cleanup` / `final-cleanup` residual cleanup，绝不用于 S5 revoke 或 verdict。

13 门 gate 4 的一次 `hdc tconn <runtime-endpoint>` 及紧随其后的一次 `hdc list targets` 是 runner 外唯一 host-prep 窄例外，不扩大上述 22 项。两条命令各只允许一次；stdout/stderr 不输出、不转录，target token 只在内存解析，必须恰好一个非空 token，并只映射为当前 host 进程级 `PHYS_1_TARGET`。0 个、多个、异常或任何泄露均立即停止。

禁止额外 discovery、再次连接/列举、全量 bundle/process dump、`hidumper`、root、privileged、自动 UI 输入、Go、NetBird、product 操作以及任何局部场景重放。唯一目标固定为上述公开 tuple 与单一进程内 target token；不得切换目标。

签名继续只接受 0005 沿用的同一 ordinary-development A/B HAP 输入：A/B 必须 distinct，设备在 development profile 内，签名类型、本地验签、公开 fingerprint、HAP hash/size、source archive/manifest 与 SDK map 都必须在 freeze 中完整绑定。不得重建、重签、修改 HAP 字节或读取/登记私钥材料。

任何 endpoint、target、serial、UDID、设备身份值、账号、凭据、私钥或签名私密材料不得写入仓库、freeze、record、evidence、日志、命令输出或聊天转录。仓内只允许公开 tuple、公开 bundle/resource id、脱敏 hash 与受控仓外路径模板。

## 完整 13 门（历史计划；在 gate 3 停止）

1. 同步 trusted refs/bundle；核对最终 HEAD 包含 `9c1d464fa78214dee7c1c00f4870aa26549af1a2`、本登记及当前常量迁移，且 worktree exact clean。冻结只能绑定该最终 HEAD；freeze 后至 campaign 结束治理字节不得改动。
2. 对新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何 Live/evidence/seal 占用立即停止。
3. 已新建 blocked confirmation freeze；静态独立审查结果 0 blocker / 1 major，因旧 source provenance mismatch 在此停止。blocked freeze 原字节保留、不得修改。
4. `not-run`：原计划由用户在本地主机只执行一次受控 `hdc tconn`，随后只执行一次内存级 `hdc list targets`；0001 未进入此门。
5. `not-run`：原计划运行 `-TargetBindingConfirm`。
6. `not-run`：原计划新建 ready freeze draft。
7. `not-run`：原计划生成独立 ready-freeze review record。
8. `not-run`：原计划新建最终 ready freeze。
9. `not-run`：原计划执行 candidate ID consumption audit-2。
10. `not-run`：原计划执行 gate-time host selftests 与 clean HEAD 核验。
11. `not-run`：原计划执行 host-only DryRun。
12. `not-run`：原计划独立审查 DryRun 并复算 freeze。
13. `not-run`：原计划单次完整 Live。0001 没有 Live consumption。

## 当前历史边界

0001 只保留为 gate-3 governance-review-blocked-retired provenance。任何活跃执行入口、runner 常量、PowerShell parity、selftests、freeze example 与 current docs 均不得再指向 0001。0001 的详细 S5/S6 和白名单段落只记录当时计划，不构成执行授权；当前完整 13 门只受 0002 登记约束。
