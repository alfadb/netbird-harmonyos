# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-25 · 0001，governance-tuple-drift-retired）

最后核验：2026-08-28

本文历史登记曾建立 `AUTH-E3-PHYS1API26-20260825-0001`、campaign `E3-PHYS-PREFLIGHT-20260825-0001` 与 evidence `EV-E3-PHYS1API26-20260825-0001`，`attempt: initial`、retry N/A，取代 [`AUTH-E3-PHYS1API26-20260817-0002`](e3-physical-preflight-authorization-2026-08-17-0002.md)（gate 顺序违规退役）。该 pair 已由用户（直接人类决策者）于 2026-08-28 决定退役，并由 [`AUTH-E3-PHYS1API26-20260828-0001`](e3-physical-preflight-authorization-2026-08-28-0001.md) 取代，以全新 AUTH/pair 重新进入完整 13 门。

> 退役状态：`authorization_status: governance-tuple-drift-retired`，`reusable: false`。gate 1-4 host-only 均 pass（gate 1：HEAD `46fb2213beb0df45169a0af54908a54598f2def1`，worktree clean，`code_sha` 已绑定进 freeze；gate 2 audit-1 PASS（unoccupied，2026-08-25T18:08，仓外 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260825-0001/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`）；gate 3 blocked confirmation freeze 已创建（2026-08-25T18:10:21+08:00，仓外 `$HOME/harmonyos-signing/netbird-e3/freeze/freeze-blocked-confirm-20260825-0001.json` + `.sha256`，`plan_status=blocked`，全部执行门 pending）；gate 4 由用户显式授权主会话代执行恰一次内存级 `hdc list targets`（恰一 token，未输出未持久化），记为 `pass-user-authorized-agent-executed`）。gate 5 `TargetBindingConfirm` 于 2026-08-28T18:30:56+08:00 执行（仓外 `$HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260825-0001.json` + `.sha256`）：3/3 探针完成，model `PLA-AL10` 匹配、HDC `Ver: 3.2.0d` 匹配，但 **full system build 漂移**——冻结期望 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` vs 实测 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`（record 投影中版本段因 IPv4 形态自动脱敏显示 `<REDACTED_IPV4>`，完整值由后续授权 rebind 探针实测确认）→ verdict=blocked，reason=`preflight: frozen full system build mismatch`。设备在 2026-08-17（上次实测同冻结 build pass）与 2026-08-28 之间收到系统 OTA（E32R7P2 → E102R7P3）。gate 6-13 全部 not-run，未 DryRun、未 Live、未 consumed。退役标签 `governance-tuple-drift-retired`（非流程违规，runner 按设计 fail-closed 拦截）。本 pair 的全部仓外对象（audit-1、blocked freeze、blocked confirmation record 及各自 companion）逐字节保留为历史，不得删除、覆盖、补写或绑定到新 AUTH。AUTH/pair 永久不可复用，不得补跑或转作 retry basis。E3 未关闭，E8 保持 `CLOSED`。

> 保全边界：本登记只记录既成事实；本 pair 的全部仓外 audit、freeze 与 record 对象及 companion 必须逐字节保留。退役不产生设备证据，也不构成 pair consumption。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260825-0001
supersedes: AUTH-E3-PHYS1API26-20260817-0002
superseded_by: AUTH-E3-PHYS1API26-20260828-0001
exception: E3-PHYS-PREFLIGHT
information_status: historical-governance-registration
record_status: governance-tuple-drift-not-live
stage_or_gate: E3
governance_gate_reached: gate-5-build-mismatch
related_stages_or_gates: [E8]
execution: host-governance-tuple-drift
is_evidence: false
authorization_status: governance-tuple-drift-retired
plan_status: governance-tuple-drift-retired
ready: false
reusable: false
device_readiness: user-attested-ready
machine_fresh_confirmation: blocked-record-build-mismatch
independent_review: not-run
blocked_confirmation_freeze: pass-created
ready_freeze: not-run
candidate_audit_1: pass
candidate_audit_2: not-run
dry_run: not-run
live: not-run
campaign_status: not-run
live_consumed: false
hap_source_basis_commit: 62409c5f966d00597b58f68ae5b927dd06e76e76
runner_code_basis_commit: 2fc83160d196a10bd5d64471ef40942f386b6203
code_sha: 46fb2213beb0df45169a0af54908a54598f2def1
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
gate_artifacts:
  audit_1_sha256: 5bf01d84cfdc4cec6d74a554957960440bd68cf54b2f2282ba7f37220b8da307
  audit_1_timestamp: 2026-08-25T18:08
  blocked_freeze_sha256: 74c3c60414f6f38d4b05770e011639030c6a9b13244b5e6157e8bd9c1a60c3e1
  blocked_freeze_timestamp: 2026-08-25T18:10:21+08:00
  blocked_freeze_plan_status: blocked
  target_binding_confirmation_sha256: 10cde7f504dbbb6b3ca86adcd8a6bd4d09466fb11b4a0cf5cf89874e93869197
  target_binding_confirmation_timestamp: 2026-08-28T18:30:56+08:00
  target_binding_confirmation_verdict: blocked
  target_binding_confirmation_reason: "preflight: frozen full system build mismatch"
gates:
  gate_1: pass
  gate_2: pass
  gate_3: pass
  gate_4: pass-user-authorized-agent-executed
  gate_5: blocked-build-mismatch
  gates_6_through_13: not-run
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260825-0001
  evidence_id: EV-E3-PHYS1API26-20260825-0001
  attempt: initial
  retry: N/A
  identity_status: governance-tuple-drift-retired
  consumed: false
  reusable: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260817-0002
  evidence_id: EV-E3-PHYS1API26-20260817-0002
  authorization_id: AUTH-E3-PHYS1API26-20260817-0002
  status: governance-order-invalid-retired
  gate_reached: gate-5-invalid-order
  gate_1: pass-implicit
  gate_2: pass
  gate_3: pass
  gate_4: not-run
  gate_5: invalid-early-execution-blocked-record
  gates_6_through_13: not-run
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
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260825-0001.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260825-0001.json
evidence_roots:
  dry_run: not-created
  live: not-created
```

`reusable: false` 是永久状态：本 pair 未 Live、未 consumed，但 gate 5 实测 full system build 与冻结元组漂移（设备 OTA 所致）已使 AUTH/pair 按治理失败永久退役，不得复用、补跑或转作 retry basis。gate 1-4 产生的仓外对象（audit-1、blocked freeze）与 gate 5 的 blocked record 均为历史字节，保持逐字节保全。

## 继承的冻结输入

本登记不改 S6、reviewer 或 source/HAP 逻辑，继承并继续分别绑定以下两个事实：

1. HAP source basis 固定为 `62409c5f966d00597b58f68ae5b927dd06e76e76`；source archive 由该 commit 的 git object database 导出，不读取 dirty worktree，不重建或重签 HAP。
2. `code_sha` 是包含本登记、runner/PowerShell parity、selftests、freeze example 与 current docs 的最终 exact clean HEAD，只能在 gate 1 现场确定。

```yaml
source_provenance:
  root: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5
  archive_path: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/e3-phys-preflight-hap-source-62409c5.tar.gz
  archive_sha256: abecc31715585b3047c00f85609a74ab0cefab2b6d328c6810ef987b5fc76888
  archive_size: 9987
  manifest_path: $HOME/harmonyos-signing/netbird-e3/source-provenance/20260816-relabeled-62409c5/source-manifest.json
  manifest_sha256: 30a22dc6c3a10dd75e6f86e4b7cf06427389e318abe0cb78c72604983e302322
  manifest_size: 10268
signed_hap_frozen:
  root: $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/
  hap_a_sha256: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_a_size: 133941
  hap_b_sha256: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
  hap_b_size: 133946
```

source manifest 继续绑定 22 个 git blobs。S5、S6、production fixture provenance、冲突码 `2203002`、A Start1/A optional Allow2/B Start3/B optional Allow4、runner 的 22 项命令白名单与 exact reviewer role `isolated-anthropic-claude-opus-5-reviewer` 均不修改。

## HDC 与 discovery 硬边界

除 gate 4 的 exact host-prep `tconn` + `list targets` 以及 runner 22 项白名单生成的命令外，任何 HDC 子命令永久禁止；任何非白名单 `hdc shell` 永久禁止。尤其禁止设备进程列表、设备 `ps`、宽泛进程发现、额外 target/serial/UDID/endpoint discovery、`hidumper`、root 或 privileged 探针。runner 只允许 directed exact-name `pidof <bundle>:vpn`，不得扩大为进程枚举。

host HDC process count 只能启动绝对 `/usr/bin/ps`，参数必须逐字为 `-eo` 与 `comm=,args=`，并且只比较输出第一列 `comm`；不得从 argv 文本匹配进程，不得以任何 HDC executable basename 做 host cleanup/count。gate 4 的 endpoint、target 和 stdout/stderr 只存在于内存，不输出、不持久化。

## 完整 13 门

1. **host-only gate 1**：同步 trusted refs/bundle；核对最终 HEAD 包含本登记和 20260825 migration、worktree exact clean，记录最终 `code_sha` 并核对 HAP source basis。绝不可调用任何 HDC executable 或子命令；HDC0 只用固定绝对 host `ps` 探针第一列判断。
2. 对 20260825-0001 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何占用立即停止。
3. 新建 blocked confirmation freeze 并由 exact reviewer 静态审查；不得复制或修改 0002 freeze。冻结绑定最终 code/source/HAP/runner/HDC/外部输入，所有执行状态保持 pending/not ready。
4. gate 1-3 完成后，用户在本地主机只执行一次受控 `tconn <runtime-endpoint>`，随后只执行一次内存级 `list targets`；两种操作各一次，不输出或持久化 endpoint、target 或 stdout/stderr，不得额外 discovery。
5. 在 gate 4 完成且 `PHYS_1_TARGET` 已设为恰一个真实 target token 后，以 20260825-0001 confirmation record 路径运行 `-TargetBindingConfirm`；runner 只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，失败即停止。
6. 新建 ready freeze draft 并绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，尤其 reviewer role exact 不变；不得原地修改 blocked freeze。
7. 由 exact reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion，要求 0 blocker / 0 major。
8. 新建最终 ready freeze，绑定 review record、最终 exact clean HEAD、source provenance、runner bytes、同一 HAP A/B 与全部外部输入；不得修改旧 freeze。
9. 对 20260825-0001 新 pair 执行 candidate ID consumption audit-2 并写 companion；本门严格 host-only，不得执行任何设备命令或 discovery。任何占用或越界立即停止并退役。
10. 执行 host-only Python 与 PowerShell selftests，确认 `HDC_PROCESSES=0`；清理并核验无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量与状态文件时间监控，不得因暂时无终端输出中断，不得 retry。

## 当前 host-only 边界

当前只允许 0002 退役登记、本 20260825-0001 registration、active runner/PowerShell parity/selftests/freeze example/current docs migration，以及 host-only selftests、既有仓库 audit、diff/JSON/sensitive/pycache/HDC0 验证。禁止任何真实 HDC executable、设备、新 pair audit/freeze/record、TargetBindingConfirm、DryRun、Live、commit 或 push。fake HDC 只允许存在于 selftest 临时沙箱，不建立 gate，不产生仓外对象。

## 验证纪律（字节比对）

本机 PATH 上的 `diff` 被 HarmonyOS SDK toolchain 的 `diff` 遮蔽，对不同文件静默返回 exit 0 且无输出，不构成字节比对证据（2026-08-25 独立审查实测确认）。所有字节相等性判定必须使用绝对路径 `/usr/bin/diff`、`cmp` 或 sha256 复算；`git diff` 不受影响（git 内置实现）。gate 1 的 worktree exact clean、gate 12 的 freeze 字节不变、host-only 验证的 diff/JSON/sensitive/pycache/HDC0 检查均适用本纪律。
