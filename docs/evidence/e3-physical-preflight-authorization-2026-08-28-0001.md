# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-28 · 0001，consumed-blocked）

最后核验：2026-08-29

本文登记用户（直接人类决策者）于 2026-08-28 的显式治理决定：永久退役 [`AUTH-E3-PHYS1API26-20260825-0001`](e3-physical-preflight-authorization-2026-08-25-0001.md)（pair `E3-PHYS-PREFLIGHT-20260825-0001` / `EV-E3-PHYS1API26-20260825-0001`），逐字节保留其全部仓外历史对象；授权四条只读 rebind 实测确认新元组（[`EV-E3-PHYS1REBIND8-20260828-0001`](e3-physical-rebind8-2026-08-28.md)）；并建立全新 AUTH/pair 重新进入完整 13 门。

20260825-0001 退役事实（既成事实，写入本登记）：gate 1-4 host-only 均 pass（gate 4 由用户显式授权主会话代执行恰一次内存级 `hdc list targets`，恰一 token，未输出未持久化）；gate 5 `TargetBindingConfirm` 于 2026-08-28T18:30:56+08:00 执行，3/3 探针完成，model `PLA-AL10` 匹配、HDC `Ver: 3.2.0d` 匹配，但 **full system build 漂移**——冻结期望 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` vs 实测 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`（设备在 2026-08-17 与 2026-08-28 之间收到系统 OTA）→ verdict=blocked，reason=`preflight: frozen full system build mismatch`；gate 6-13 全部 not-run，未 DryRun、未 Live、未 consumed。退役标签 `governance-tuple-drift-retired`（非流程违规，runner 按设计 fail-closed 拦截）。20260825-0001 的全部仓外对象（audit-1、blocked freeze、blocked confirmation record 及各自 companion）逐字节保留为历史，不得删除、覆盖、补写或绑定到新 AUTH。

据此建立 `AUTH-E3-PHYS1API26-20260828-0001`、campaign `E3-PHYS-PREFLIGHT-20260828-0001` 与 evidence `EV-E3-PHYS1API26-20260828-0001`。这是全新 `attempt: initial`，不是 retry；20260825-0001 不构成 retry basis，也不得被新 pair 消费。

> **campaign 结果（2026-08-29 晚执行，完整 13 门）**：gate 1-12 全部 pass；gate 13 Live 完成并封签。**S1-S6 全部 machine-verified pass**（本项目历次 live 首次）：S2 机器验证 Allow/onCreate/create/fd（fd=33 open=true）、S3 严格进程边界 terminal（3.100119s 间隔 absent 探针 + 干净再激活证明）、S4 deny 全窗口无 B create、S5 Settings 应用信息强停撤销 terminal、S6 B 显式冲突拒绝（冻结码 `2203002` 实测确认）。**S7 blocked**：步骤 1 前置检查发现 B 的 `:vpn` 进程实际 active（预期 inactive）——平台在冲突拒绝后保留 B 扩展进程（两处独立观察一致），属 fail-closed 平台行为观察，非功能失败。整体 `collected/blocked`，cleanup `verified-clean`，integrity 空。证据经独立记录级审查（`isolated-anthropic-claude-opus-5-reviewer` 0 blocker / 0 major / 3 minor）定级 `reviewed-pass/blocked`，见 [`EV-E3-PHYS1API26-20260828-0001`](e3-physical-preflight-api26-20260828-0001.md)。pair 已消费（Live 单次），不得复用；S7 前置校准由 [`AUTH-E3-PHYS1API26-20260829-0001`](e3-physical-preflight-authorization-2026-08-29-0001.md) 登记。E3 未关闭，E8 保持 `CLOSED`。

> 当前状态：`consumed-blocked`。完整 13 门已执行：gate 1-12 pass、gate 13 Live 完成（S1-S6 pass / S7 blocked，证据 `reviewed-pass/blocked`）。pair 已消费，不得复用。E3 未关闭，E8 保持 `CLOSED`。

> 提交边界：当前 host-only registration 禁止 commit/push。用户已批准完整治理链；只有 registration/runner parity/selftests/current docs 完成、host-only 验证全部通过并取得独立审查 0 blocker / 0 major 后，才允许 commit/push。registration commit/push 完成后 gate 1 才可开始。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260828-0001
supersedes: AUTH-E3-PHYS1API26-20260825-0001
superseded_by: AUTH-E3-PHYS1API26-20260829-0001
exception: E3-PHYS-PREFLIGHT
information_status: historical-governance-registration
record_status: consumed-blocked
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: full-13-gates-live-completed
is_evidence: false
authorization_status: consumed-blocked
plan_status: consumed-blocked
ready: false
reusable: false
device_readiness: user-attested-ready
machine_fresh_confirmation: pass
independent_review: pass-0-blocker-0-major
blocked_confirmation_freeze: pass
ready_freeze: pass-final-created
candidate_audit_1: pass
candidate_audit_2: pass
dry_run: pass
live: completed-blocked-reviewed
campaign_status: completed-blocked
live_consumed: true
hap_source_basis_commit: 62409c5f966d00597b58f68ae5b927dd06e76e76
runner_code_basis_commit: 46fb2213beb0df45169a0af54908a54598f2def1
code_sha: b24ef557a0a951a146d5ffef035541da8ae39aa9
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
gates:
  gate_1: pass
  gate_2: pass
  gate_3: pass
  gate_4: pass-user-authorized-agent-executed
  gate_5: pass
  gate_6: pass
  gate_7: pass
  gate_8: pass
  gate_9: pass
  gate_10: pass
  gate_11: pass
  gate_12: pass
  gate_13: completed-blocked
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260828-0001
  evidence_id: EV-E3-PHYS1API26-20260828-0001
  attempt: initial
  retry: N/A
  identity_status: consumed-blocked
  consumed: true
  reusable: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260825-0001
  evidence_id: EV-E3-PHYS1API26-20260825-0001
  authorization_id: AUTH-E3-PHYS1API26-20260825-0001
  status: governance-tuple-drift-retired
  gate_reached: gate-5-build-mismatch
  gate_1: pass
  gate_2: pass
  gate_3: pass
  gate_4: pass-user-authorized-agent-executed
  gate_5: blocked-build-mismatch
  gates_6_through_13: not-run
  live: not-run
  live_consumed: false
  consumed: false
  reusable: false
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  full_system_build: PLA-AL10 7.0.0.102(SP8C00E102R7P3)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
  tuple_basis: four readonly probes measured 2026-08-28 (EV-E3-PHYS1REBIND8-20260828-0001); dist/model carried from prior measured records
record_paths:
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260828-0001.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260828-0001.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260828-0001
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260828-0001
```

`reusable: false` 是永久状态：本 pair 已完整执行 13 门并 Live 消费（单次），不得复用、补跑或转作 retry basis。gate 1-12 与 Live 的全部仓外对象（audit-1/2、blocked freeze、ready draft、final ready freeze、confirmation record、review record、DryRun/Live evidence roots 及各自 companion）逐字节保全为历史。

## 继承的冻结输入

本登记不改 S6、reviewer 或 source/HAP 逻辑，继承并继续分别绑定以下两个事实：

1. HAP source basis 固定为 `62409c5f966d00597b58f68ae5b927dd06e76e76`；source archive 由该 commit 的 git object database 导出，不读取 dirty worktree，不重建或重签 HAP。HAP/source 复用沿用 ADJ-20260806-0003 先例——安装兼容性仅实测认定，不主张成功。
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

## 新纪律：gate 4 → gate 5 顺序（历次退役的直接教训）

20260817-0002 因 gate 5 先于 gate 4 执行（顺序违规）退役，20260825-0001 因 gate 5 实测 build 漂移（设备 OTA）退役。本登记固定以下纪律：

1. gate 4 必须完成：恰一次受控 `tconn <runtime-endpoint>`，随后恰一次内存级 `list targets`；两种操作各一次，不输出、不持久化 endpoint、target 或 stdout/stderr，不得额外 discovery。gate 4 由用户本人执行，或由用户显式授权主会话代执行（本次先例：用户授权主会话代执行恰一次内存级 `list targets`，恰一 token，未输出未持久化）。
2. 只有 `PHYS_1_TARGET` 已设置为恰一个真实 target token 后，才允许运行 gate 5 `TargetBindingConfirm`。
3. gate 5 绝不允许在 gate 4 完成前被调用；任何先于 gate 4 的 confirmation 执行立即视为 gate 顺序违规并退役本 pair。
4. confirmation record 路径 single-use immutable：任何 blocked record 写入即占用该路径，本 pair 不得再向该路径写入或复用，后续只能取得新的用户治理与新 AUTH/pair。
5. gate 5 实测的 model/build 必须与冻结元组逐字匹配；任何漂移（含设备 OTA 导致的 build 变化）即 blocked record 并退役本 pair，后续须新治理与新 rebind 实测。

## 完整 13 门

1. **host-only gate 1**：同步 trusted refs/bundle；核对最终 HEAD 包含本登记和 20260828 migration、worktree exact clean，记录最终 `code_sha` 并核对 HAP source basis。绝不可调用任何 HDC executable 或子命令；HDC0 只用固定绝对 host `ps` 探针第一列判断。
2. 对 20260828-0001 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何占用立即停止。
3. 新建 blocked confirmation freeze 并由 exact reviewer 静态审查；不得复制或修改 20260825-0001 freeze。冻结绑定最终 code/source/HAP/runner/HDC/外部输入，所有执行状态保持 pending/not ready。
4. gate 1-3 完成后，用户在本地主机只执行一次受控 `tconn <runtime-endpoint>`，随后只执行一次内存级 `list targets`；两种操作各一次，不输出或持久化 endpoint、target 或 stdout/stderr，不得额外 discovery。gate 4 由用户本人执行，或由用户显式授权主会话代执行（恰一次内存级 `list targets`，恰一 token，未输出未持久化）。
5. 在 gate 4 完成且 `PHYS_1_TARGET` 已设为恰一个真实 target token 后，以 20260828-0001 confirmation record 路径运行 `-TargetBindingConfirm`；runner 只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，失败即停止。
6. 新建 ready freeze draft 并绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，尤其 reviewer role exact 不变；不得原地修改 blocked freeze。
7. 由 exact reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion，要求 0 blocker / 0 major。
8. 新建最终 ready freeze，绑定 review record、最终 exact clean HEAD、source provenance、runner bytes、同一 HAP A/B 与全部外部输入；不得修改旧 freeze。
9. 对 20260828-0001 新 pair 执行 candidate ID consumption audit-2 并写 companion；本门严格 host-only，不得执行任何设备命令或 discovery。任何占用或越界立即停止并退役。
10. 执行 host-only Python 与 PowerShell selftests，确认 `HDC_PROCESSES=0`；清理并核验无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量与状态文件时间监控，不得因暂时无终端输出中断，不得 retry。

## 当前 host-only 边界

当前只允许 20260825-0001 退役登记、本 20260828-0001 registration、rebind8 证据登记、active runner/PowerShell parity/selftests/freeze example/current docs migration，以及 host-only selftests、既有仓库 audit、diff/JSON/sensitive/pycache/HDC0 验证。禁止任何真实 HDC executable、设备、新 pair audit/freeze/record、TargetBindingConfirm、DryRun、Live、commit 或 push。fake HDC 只允许存在于 selftest 临时沙箱，不建立 gate，不产生仓外对象。

## 验证纪律（字节比对）

本机 PATH 上的 `diff` 被 HarmonyOS SDK toolchain 的 `diff` 遮蔽，对不同文件静默返回 exit 0 且无输出，不构成字节比对证据（2026-08-25 独立审查实测确认）。所有字节相等性判定必须使用绝对路径 `/usr/bin/diff`、`cmp` 或 sha256 复算；`git diff` 不受影响（git 内置实现）。gate 1 的 worktree exact clean、gate 12 的 freeze 字节不变、host-only 验证的 diff/JSON/sensitive/pycache/HDC0 检查均适用本纪律。
