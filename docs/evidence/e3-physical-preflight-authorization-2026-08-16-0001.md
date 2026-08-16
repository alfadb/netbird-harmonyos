# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0001）

最后核验：2026-08-16

本文登记用户（直接人类决策者）于 2026-08-16 明确选择「继续：实现 B 授权分支、生产证据回归并执行新一轮完整治理」，据此批准新授权 `AUTH-E3-PHYS1API26-20260816-0001`，新 campaign `E3-PHYS-PREFLIGHT-20260816-0001`，新 evidence `EV-E3-PHYS1API26-20260816-0001`，`attempt: initial`。本授权取代并且绝不复用 [`AUTH-E3-PHYS1API26-20260815-0005`](e3-physical-preflight-authorization-2026-08-15-0005.md)：0005 已是 `sealed-blocked-consumed`，其 AUTH、campaign ID、evidence ID、sealed evidence、判定与 hash 只作为历史/provenance 保留，不得重判、覆盖、局部重放或用于 retry。

> 当前状态：`authorization_status: blocked-awaiting-full-gates`，not ready。新 pair 只适用于包含 S6 B 首次授权修复 commit `9c1d464fa78214dee7c1c00f4870aa26549af1a2` 之后治理字节的完整 13 门流程；不得从中间 gate 开始，也不得只重放 S6。machine fresh confirmation、independent review、blocked/ready freeze、DryRun 与 Live 全部为 `pending`，本文没有预写任何新 campaign pass 或产物 hash。E3 未关闭，E8 保持 `CLOSED`。

> gate 1 前置：当前 registration + pair migration 在 0B0M 审查和 host tests 通过后，获准执行为形成 gate 1 所需 exact clean HEAD 的 commit/push；该许可只覆盖此次治理提交，不覆盖 campaign evidence。提交前仍禁止 HDC、设备命令、freeze、record/audit 写入、DryRun 与 Live；单次 Live、完整 13 门及任一 gate 失败即停止、不重试、不换 ID 的纪律不变。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260816-0001
supersedes: AUTH-E3-PHYS1API26-20260815-0005
exception: E3-PHYS-PREFLIGHT
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: blocked-awaiting-full-gates
plan_status: blocked-awaiting-full-gates
ready: false
device_readiness: user-attested-ready
machine_fresh_confirmation: pending
independent_review: pending
blocked_confirmation_freeze: pending
ready_freeze: pending
dry_run: pending
live: pending
campaign_status: not-run
code_basis_commit: 9c1d464fa78214dee7c1c00f4870aa26549af1a2
head_requirement: exact-clean-final-governance-commit-descending-from-code-basis
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0001
  evidence_id: EV-E3-PHYS1API26-20260816-0001
  attempt: initial
  retry: N/A
  identity_status: pending-two-consumption-audits
  consumed: false
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
```

`code_basis_commit` 是 S6 B 修复的已提交代码基础，不是最终 freeze 的 `code_sha`。最终 freeze 只能绑定包含本登记及当前常量迁移的后续精确 clean HEAD；该 HEAD 与新 runner/freeze/selftest hash 均须在相应 gate 现场计算，本文保持 pending，不预填。

## 0005 处置与适用边界

1. 0005 sealed evidence 固定为 S1-S5 pass、S6 blocked（`scenario-6 machine-verification-blocked step=3 reason=platform-marker-missing:B-create-terminal-missing`）、S7 blocked（`not-run-after-runner-failure`），overall/verdict blocked。该事实不因本授权改变。
2. 0005 AUTH/pair 是 `sealed-blocked-consumed`，不可复用；新 20260816-0001 是全新 initial，不是 retry。任一 gate 失败立即停止，不自动 retry、不换 ID；任何后续尝试必须重新治理并取得新 AUTH/pair。
3. 新 pair 只允许完整 S1-S7 campaign 且必须从 13 门 gate 1 开始。禁止局部 S6 重放、跳门、复用旧 confirmation/review/freeze/DryRun/Live 输出或原地修改旧对象。
4. S6 B 修复代码基础固定为 commit `9c1d464fa78214dee7c1c00f4870aa26549af1a2`。最终执行 HEAD 必须包含该 commit、本登记和当前常量迁移，并在 gate 1、10、11、12、13 前后保持 exact 且 clean。
5. E3 仍 open；E8 严格保持 `CLOSED`。即使本 campaign 最终通过，也不自动开放 E8。

## S5 应用详情路径

S5 的操作路径是系统设置中的目标应用详情页：从设置的应用管理/应用列表进入 `E3 Preflight A` 的应用详情，然后点击 `强行停止`，完成随后出现的确认（如有）。操作员不得使用设置搜索页，不得从 VPN 设置页、隐藏导航、最近页面或历史文本推断目标，也不得依靠 runner 自动点击。

机器门只接受 Settings owner 下唯一可见 `Setting.AppDetail` 子树：候选自身必须 `visible=true`，Settings owner 到候选的所有祖先均不得 `visible=false`；同一子树内必须同时存在 distinct label `E3 Preflight A` 与 force-stop 控件。子树外标签、隐藏祖先、搜索页和历史文本不能命中。post-force capture 仅 observation-only；S5 最终撤销仍只由连续 `<bundle>:vpn` absent（至少 2 次、间隔至少 3 秒）且 bundle present 的既有决定性门确认。

## S6 实际流程与操作员动作

S6 的机械步骤固定为：

1. **A Start，step 1**：runner 打开 A Entry 并通过布局门后，操作员只点击 `E3 Preflight A` 的 Start，然后按回车。不得点击 Stop、重复 Start 或执行其他 UI 动作。
2. **A optional Allow，step 2**：仅当机器布局门显示 A 的 VPN authorization dialog 时，runner 才提示该步；操作员只点击 `允许`，然后按回车。若没有该提示，禁止自行寻找或点击授权。
3. **B Start，step 3**：A create terminal 与 A exact `<bundle>:vpn` active pre-gate 通过后，runner 打开 B Entry；操作员只点击 `E3 Preflight B` 的 Start，然后按回车。
4. **B optional Allow，step 4**：B Start 后 runner 对 entry/authorization 双档案分流。仅当机器识别到 B authorization profile 时才提示；操作员只点击 `允许`，然后按回车。若机器识别 entry profile，则没有 step 4，禁止额外点击。

B authorization profile 的 production 来源是 0005 sealed Live 的实际 post-B-Start capture `RAW-capture-scenario-6-conflict.json`，raw SHA-256 `7f6e44d5eab7021d192a6a61f409af9a3e8507f16c1df6bcf84755c1130bb72c`；仓内最小脱敏 fixture 为 `tests/fixtures/s6-b-authorization-production-0005.json`，SHA-256 `8d99c65e6bfa0c9569848f6a482fd188c1bcb6b6837ca0e8386c052c33f376ad`。文件名 0005 是不可迁移的 provenance，不表示复用 0005 pair。

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

## 完整 13 门

1. 同步 trusted refs/bundle；核对最终 HEAD 包含 `9c1d464fa78214dee7c1c00f4870aa26549af1a2`、本登记及当前常量迁移，且 worktree exact clean。冻结只能绑定该最终 HEAD；freeze 后至 campaign 结束治理字节不得改动。
2. 对新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何 Live/evidence/seal 占用立即停止。
3. 新建 blocked confirmation freeze 并完成静态独立审查；不得复制或原地修改 0005 freeze。其 plan、machine confirmation、review、DryRun、Live 状态保持 pending/not ready。
4. 由用户在本地主机只执行一次受控 `hdc tconn`，随后只执行一次内存级 `hdc list targets`；必须得到唯一 token 并建立进程级 `PHYS_1_TARGET`，不得输出或持久化敏感值。
5. 以新 confirmation record 路径运行 `-TargetBindingConfirm`；只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，完成 machine fresh confirmation。任何失败停止。
6. 新建 ready freeze draft，绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，不得原地改 blocked freeze。
7. 由与 operator 不同的独立 reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion。
8. 新建最终 ready freeze，绑定 review record、最终 exact clean HEAD、runner bytes、同一 A/B HAP 与全部外部输入；不得修改旧 freeze。
9. 对新 pair 执行 candidate ID consumption audit-2，仓外记录并写 `.sha256`；任何占用立即停止。
10. 执行 host-only Python selftest 与 PowerShell selftest，确认 `HDC_PROCESSES=0`；清理并核验仓内无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，并复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量及 `operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 监控。不得因终端暂时无输出擅自中断，不得 retry。

## 当前 host-only 登记任务边界

本次只允许新建本登记、迁移 runner/PowerShell runner/selftest/freeze example 与必要当前文档常量，并执行 host-only 自测、源码审计、diffcheck、敏感扫描、pycache 与 HDC 进程核验。禁止执行任何 HDC 命令、设备命令、TargetBindingConfirm、freeze 生成、record/audit 写入、DryRun 或 Live。0B0M 审查和 host tests 通过后，允许仅为形成 gate 1 所需 exact clean HEAD 执行当前 registration + pair migration 的 commit/push；campaign evidence 仍不得提前提交。当前登记完成不等于 gate 1 完成，也不使 campaign ready。
