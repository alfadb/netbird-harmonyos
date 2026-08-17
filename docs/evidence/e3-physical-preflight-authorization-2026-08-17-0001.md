# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-17 · 0001）

最后核验：2026-08-17

本文登记用户（直接人类决策者）于 2026-08-17 的显式跨日确认：20260816-0003 的 gate 1-6 已跨入 2026-08-17 完成，gate 7 因 reviewer role contract mismatch 得出 0 major / 1 blocker 且未写 review record；用户确认保留全部 0003 仓外历史字节、永久退役该 AUTH/pair，并授权以修复后的 reviewer contract 建立全新 AUTH/pair，重新进入完整 13 门。

据此建立 `AUTH-E3-PHYS1API26-20260817-0001`、campaign `E3-PHYS-PREFLIGHT-20260817-0001` 与 evidence `EV-E3-PHYS1API26-20260817-0001`。这是全新 `attempt: initial`，不是 retry；它取代并且绝不复用 [`AUTH-E3-PHYS1API26-20260816-0003`](e3-physical-preflight-authorization-2026-08-16-0003.md)。

> 当前状态：`blocked-awaiting-full-gates`，not ready。新 pair 的 audit、freeze、confirmation、review、DryRun 与 Live 均未运行；本文不预写任何新 gate pass、对象 hash、record hash、freeze hash 或 campaign hash。20260816-0003 的 host daemon cleanup 只属于退役清理，不计入本 AUTH 的 gate。E3 未关闭，E8 保持 `CLOSED`。

> 阶段边界与提交授权：host remediation 期间禁止 commit/push，也不授权任何 audit、freeze、record、DryRun、Live 或设备命令。仅在 reviewer remediation 获得 0 blocker / 0 major 且 host-only tests 全部通过后，该禁令解除；此后明确允许对 registration、runner/PowerShell parity、selftests、freeze example、current docs 与 pair migration 执行 commit/push。registration commit/push 完成后，gate 1 才开始。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260817-0001
supersedes: AUTH-E3-PHYS1API26-20260816-0003
exception: E3-PHYS-PREFLIGHT
information_status: current-governance-registration
record_status: registered-not-run
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: blocked-awaiting-full-gates
plan_status: blocked-awaiting-full-gates
ready: false
reusable: true
device_readiness: user-attested-ready
machine_fresh_confirmation: pending
independent_review: pending
blocked_confirmation_freeze: pending
ready_freeze: pending
candidate_audit_1: pending
candidate_audit_2: pending
dry_run: pending
live: pending
campaign_status: not-run
live_consumed: false
hap_source_basis_commit: 62409c5f966d00597b58f68ae5b927dd06e76e76
runner_code_basis_commit: 9c1d464fa78214dee7c1c00f4870aa26549af1a2
code_sha: pending-final-clean-head
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260817-0001
  evidence_id: EV-E3-PHYS1API26-20260817-0001
  attempt: initial
  retry: N/A
  identity_status: pending-two-consumption-audits
  consumed: false
  reusable: true
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0003
  evidence_id: EV-E3-PHYS1API26-20260816-0003
  authorization_id: AUTH-E3-PHYS1API26-20260816-0003
  status: governance-review-blocked-retired
  gate_reached: gate-7
  review_result: 0-major-1-blocker-role-mismatch-no-record
  live: not-run
  live_consumed: false
  consumed: false
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
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260817-0001.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260817-0001.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260817-0001
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260817-0001
```

`reusable: true` 只表示新 AUTH/pair 尚未因治理失败或 Live 消费；它不表示 ready。任一 gate 失败立即停止并永久退役本 pair，后续只能取得新治理与新 AUTH/pair。

## 双 SHA 与 source provenance

冻结必须分别绑定 HAP 实际源码基础与最终治理执行代码，二者不得互相替代：

1. `hap_source_basis_commit` 固定为 `62409c5f966d00597b58f68ae5b927dd06e76e76`。source archive 由该 commit 的 git object database 导出，不读取 dirty worktree，不重建或重签 HAP。
2. `code_sha` 是包含 reviewer remediation、runner/PowerShell parity、selftests、freeze example、本登记和 current docs 的最终 exact clean HEAD；当前只能是 `pending-final-clean-head`，须在 gate 1 现场确定。

```yaml
source_provenance:
  root: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5
  archive_path: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/e3-phys-preflight-hap-source-62409c5.tar.gz
  archive_sha256: abecc31715585b3047c00f85609a74ab0cefab2b6d328c6810ef987b5fc76888
  archive_size: 9987
  manifest_path: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/source-manifest.json
  manifest_sha256: 30a22dc6c3a10dd75e6f86e4b7cf06427389e318abe0cb78c72604983e302322
  manifest_size: 10268
  archive_sha256_companion: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/e3-phys-preflight-hap-source-62409c5.tar.gz.sha256
  manifest_sha256_companion: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/source-manifest.json.sha256
signed_hap_frozen:
  root: $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/
  hap_a_sha256: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_a_size: 133941
  hap_b_sha256: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
  hap_b_size: 133946
```

source manifest 继续绑定 22 个 git blobs。source `build-profile.json5` 的 A/default 与 B/vpnB 分别对应公开 bundle `cn.alfadb.netbird.e3physvpna` / `cn.alfadb.netbird.e3physvpnb`；ability label 分别为 `E3 Preflight A` / `E3 Preflight B`。冻结 HAP metadata 与该 source tree 的对应边界、compiled payload 非字节级证明边界、普通开发签名的仓外安全边界均继承 0003，不扩大、不改写。S6 production fixture provenance 与冻结 HAP/source 逻辑均不变。

## Reviewer contract remediation

新执行面将 `independent_reviewer_role` 与 `independent_review_record.reviewer_role` 固定为 exact `isolated-anthropic-claude-opus-5-reviewer`。该字段属于 stable two-phase confirmation contract：blocked freeze 与 ready freeze 必须保持相同 exact role，任何其他角色在 freeze validation 阶段即拒绝；ready review record 也必须逐字匹配该角色并满足 operator/reviewer 分离、0 blocker / 0 major、双文件 hash 与时间链约束。

历史 0003 blocked freeze 与 ready draft 中的 `isolated-static-reviewer` 字节永久不改；它们只证明 0003 的 gate-7 blocker，不得被新 AUTH 消费。

## S5、S6 与 22 项白名单

S5 只接受系统 Settings owner 下唯一可见 `Setting.AppDetail` 子树；同一子树必须同时包含 A 的 distinct label `E3 Preflight A` 与 force-stop 控件。操作员仅执行应用详情中的强行停止及其确认；post-force capture 只作 observation，最终撤销仍由连续 `<bundle>:vpn` absent 至少 2 次、间隔至少 3 秒且 bundle present 的机器门确认。

S6 固定为 A Start step 1、A optional Allow step 2、B Start step 3、B optional Allow step 4。runner prompt 为 `点击 Allow`，设备可见控件为 `允许`；仅在机器识别 authorization profile 并显示 prompt 时执行。B conflict 仍固定拒绝码 `2203002`、现有 requestId/process/layout/fail-closed 语义，不修改 S6/source/HAP 逻辑。

runner HDC 白名单仍精确为 22 项：`Version`、`TupleModel`、`TupleBuild`、`MkdirStaging`、`RemoveStaging`、`StagingProbe`、`SendA`、`SendB`、`InstallA`、`InstallB`、`FaultA`、`FaultB`、`HilogStream`、`BundleDump`、`PidOf`、`Uninstall`、`StartEntry`、`ScreenCap`、`DumpLayout`、`ReceiveScreen`、`ReceiveLayout`、`ForceStop`。`ForceStop` 只允许 exception/final cleanup residual cleanup，不用于 S5 revoke 或 verdict。

## 完整 13 门

1. **host-only gate 1**：同步 trusted refs/bundle；核对最终 HEAD 包含 `9c1d464fa78214dee7c1c00f4870aa26549af1a2`、reviewer remediation、本登记及新 pair migration，且 worktree exact clean。记录最终 `code_sha`，核对 HAP source basis `62409c5f966d00597b58f68ae5b927dd06e76e76`。本门绝不可调用任何 basename HDC 可执行文件或子命令，包括 version/list/kill；HDC0 只可由主机进程表判断。
2. 对 20260817-0001 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何占用立即停止。
3. 新建 blocked confirmation freeze 并由 exact `isolated-anthropic-claude-opus-5-reviewer` 静态审查；不得复制或修改任何 0003 freeze。冻结绑定最终 code/source/HAP/runner/HDC/外部输入，所有执行状态保持 pending/not ready。
4. gate 1-3 完成后，用户在本地主机只执行一次受控 `hdc tconn <runtime-endpoint>`，随后只执行一次内存级 `hdc list targets`；两种操作各一次，不输出或持久化 endpoint、target 或 stdout/stderr，不得额外 discovery。
5. 以新 confirmation record 路径运行 `-TargetBindingConfirm`；只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，失败即停止。
6. 新建 ready freeze draft并绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，尤其 reviewer role 必须 exact 不变；不得原地修改 blocked freeze。
7. 由 exact reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion，要求 0 blocker / 0 major。
8. 新建最终 ready freeze，绑定 review record、最终 exact clean HEAD、source provenance、runner bytes、同一 HAP A/B 与全部外部输入；不得修改旧 freeze。
9. 对新 pair 执行 candidate ID consumption audit-2 并写 companion；任何占用立即停止。
10. 执行 host-only Python 与 PowerShell selftests，确认 `HDC_PROCESSES=0`；清理并核验无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量与状态文件时间监控，不得因暂时无终端输出中断，不得 retry。

## 当前 host-only 边界

当前处于 host remediation 阶段，仅允许 0003 退役登记、reviewer contract remediation、新 AUTH/pair registration、current docs 迁移，以及 host-only selftests/audit/diffcheck/JSON/sensitive/pycache/HDC0 验证。该阶段禁止任何 basename HDC 程序、设备、新 pair audit、freeze、record、TargetBindingConfirm、DryRun、Live、commit 或 push。host-only selftest 内的 fake HDC 仅存在于测试沙箱，不是设备 HDC，也不建立新 gate。

reviewer remediation 取得 0 blocker / 0 major 且 host-only tests 全部通过后，host remediation 阶段结束，commit/push 禁令解除并明确允许完成 registration commit/push；registration commit/push 完成后才进入并开始 gate 1。