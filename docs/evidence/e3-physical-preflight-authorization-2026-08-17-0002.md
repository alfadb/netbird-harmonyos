# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-17 · 0002，governance-order-invalid-retired）

最后核验：2026-08-25

本文历史登记曾建立 `AUTH-E3-PHYS1API26-20260817-0002`、campaign `E3-PHYS-PREFLIGHT-20260817-0002` 与 evidence `EV-E3-PHYS1API26-20260817-0002`，`attempt: initial`、retry N/A，取代 [`AUTH-E3-PHYS1API26-20260817-0001`](e3-physical-preflight-authorization-2026-08-17-0001.md)（gate 9 未授权设备进程枚举退役）。该 pair 已由用户（直接人类决策者）于 2026-08-25 决定退役，并由 [`AUTH-E3-PHYS1API26-20260825-0001`](e3-physical-preflight-authorization-2026-08-25-0001.md) 取代，以全新 AUTH/pair 重新进入完整 13 门。

> 退役状态：`authorization_status: governance-order-invalid-retired`，`reusable: false`。gate 1 隐含 pass（HEAD `2fc83160d196a10bd5d64471ef40942f386b6203`，worktree clean，`code_sha` 已绑定进 freeze）；gate 2 audit-1 PASS（unoccupied，2026-08-17T13:54，仓外 `$HOME/harmonyos-signing/netbird-e3/campaigns/E3-PHYS-PREFLIGHT-20260817-0002/audit/new-pair-id-consumption-audit-1.txt` + `.sha256`）；gate 3 blocked confirmation freeze 已创建（2026-08-17T13:58，仓外 `$HOME/harmonyos-signing/netbird-e3/freeze/freeze-blocked-confirm-20260817-0002.json` + `.sha256`，`plan_status=blocked`，全部执行门 pending）；gate 4（用户 host-prep `tconn` + `list targets`）not-run，无执行证据；gate 5 `TargetBindingConfirm` 在 gate 4 未完成时被提前执行（gate 顺序违规），产生 blocked record（2026-08-17T14:08，仓外 `$HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260817-0002.json` + `.sha256`）：verdict=blocked，reason=`PHYS_1_TARGET must contain exactly one real target token`，command_attempted=0、command_completed=0，0 次 HDC 调用、0 设备接触；gate 6-13 全部 not-run，未 DryRun、未 Live、未 consumed。0002 的全部仓外对象（audit-1、blocked freeze、blocked confirmation record 及各自 companion）逐字节保留为历史，不得删除、覆盖、补写或绑定到新 AUTH。AUTH/pair 永久不可复用，不得补跑或转作 retry basis。E3 未关闭，E8 保持 `CLOSED`。

> 保全边界：本登记只记录既成事实；0002 的全部仓外 audit、freeze 与 record 对象及 companion 必须逐字节保留。退役不产生设备证据，也不构成 pair consumption。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260817-0002
supersedes: AUTH-E3-PHYS1API26-20260817-0001
superseded_by: AUTH-E3-PHYS1API26-20260825-0001
exception: E3-PHYS-PREFLIGHT
information_status: historical-governance-registration
record_status: governance-order-invalid-not-live
stage_or_gate: E3
governance_gate_reached: gate-5-invalid-order
related_stages_or_gates: [E8]
execution: host-governance-order-invalid
is_evidence: false
authorization_status: governance-order-invalid-retired
plan_status: governance-order-invalid-retired
ready: false
reusable: false
device_readiness: user-attested-ready
machine_fresh_confirmation: blocked-record-order-invalid
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
runner_code_basis_commit: e1ec608722f920db3e43444c29d30056567d4004
code_sha: 2fc83160d196a10bd5d64471ef40942f386b6203
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
gate_artifacts:
  audit_1_sha256: 3ea421007f4ab5164ab4f5ed4396323156814b6ba3092e2151962f2355fff772
  audit_1_timestamp: 2026-08-17T13:54
  blocked_freeze_sha256: 317daec1fc58494771d9369b7e5229cc8959d4400f702598951f3a77ea906183
  blocked_freeze_timestamp: 2026-08-17T13:58
  blocked_freeze_plan_status: blocked
  target_binding_confirmation_sha256: ac92a93a6a5768aff988d78bda152a0b5874bd8f73562b7e131a710629c1eee8
  target_binding_confirmation_timestamp: 2026-08-17T14:08
  target_binding_confirmation_verdict: blocked
  target_binding_confirmation_reason: PHYS_1_TARGET must contain exactly one real target token
gates:
  gate_1: pass-implicit
  gate_2: pass
  gate_3: pass
  gate_4: not-run
  gate_5: invalid-early-execution-blocked-record
  gates_6_through_13: not-run
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260817-0002
  evidence_id: EV-E3-PHYS1API26-20260817-0002
  attempt: initial
  retry: N/A
  identity_status: governance-order-invalid-retired
  consumed: false
  reusable: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260817-0001
  evidence_id: EV-E3-PHYS1API26-20260817-0001
  authorization_id: AUTH-E3-PHYS1API26-20260817-0001
  status: governance-operation-invalid-retired
  gate_reached: gate-9
  gate_1_through_8: pass
  gate_9: invalid-unauthorized-hdc-shell-ps
  audit_2: file-exists-not-valid-gate-pass
  gates_10_through_13: not-run
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
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260817-0002.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260817-0002.json
evidence_roots:
  dry_run: not-created
  live: not-created
```

`reusable: false` 是永久状态：0002 未 Live、未 consumed，但 gate 5 在 gate 4 未完成时被提前执行的顺序违规已使 AUTH/pair 按治理失败永久退役，不得复用、补跑或转作 retry basis。gate 1-3 产生的仓外对象（audit-1、blocked freeze）与 gate 5 的 blocked record 均为历史字节，保持逐字节保全。

## 继承的冻结输入

0002 不改 S6、reviewer 或 source/HAP 逻辑，继承并继续分别绑定以下两个事实：

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

1. **host-only gate 1**：同步 trusted refs/bundle；核对最终 HEAD 包含本登记和 0002 migration、worktree exact clean，记录最终 `code_sha` 并核对 HAP source basis。绝不可调用任何 HDC executable 或子命令；HDC0 只用固定绝对 host `ps` 探针第一列判断。
2. 对 0002 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何占用立即停止。
3. 新建 blocked confirmation freeze 并由 exact reviewer 静态审查；不得复制或修改 0001 freeze。冻结绑定最终 code/source/HAP/runner/HDC/外部输入，所有执行状态保持 pending/not ready。
4. gate 1-3 完成后，用户在本地主机只执行一次受控 `tconn <runtime-endpoint>`，随后只执行一次内存级 `list targets`；两种操作各一次，不输出或持久化 endpoint、target 或 stdout/stderr，不得额外 discovery。
5. 以 0002 confirmation record 路径运行 `-TargetBindingConfirm`；runner 只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，失败即停止。
6. 新建 ready freeze draft 并绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，尤其 reviewer role exact 不变；不得原地修改 blocked freeze。
7. 由 exact reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion，要求 0 blocker / 0 major。
8. 新建最终 ready freeze，绑定 review record、最终 exact clean HEAD、source provenance、runner bytes、同一 HAP A/B 与全部外部输入；不得修改旧 freeze。
9. 对 0002 新 pair 执行 candidate ID consumption audit-2 并写 companion；本门严格 host-only，不得执行任何设备命令或 discovery。任何占用或越界立即停止并退役。
10. 执行 host-only Python 与 PowerShell selftests，确认 `HDC_PROCESSES=0`；清理并核验无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量与状态文件时间监控，不得因暂时无终端输出中断，不得 retry。

## 当前 host-only 边界

当前只允许 0001 退役登记、0002 registration、active runner/PowerShell parity/selftests/freeze example/current docs migration，以及 host-only selftests、既有仓库 audit、diff/JSON/sensitive/pycache/HDC0 验证。禁止任何真实 HDC executable、设备、新 pair audit/freeze/record、TargetBindingConfirm、DryRun、Live、commit 或 push。fake HDC 只允许存在于 selftest 临时沙箱，不建立 gate，不产生仓外对象。
