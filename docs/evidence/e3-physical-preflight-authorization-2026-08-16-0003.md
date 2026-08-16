# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0003）

最后核验：2026-08-16

本文登记用户（直接人类决策者）本轮明确批准的原意：**将因治理门顺序失效而退役的 20260816-0002 保留为历史，并进入 AUTH/pair 20260816-0003 的完整 13 门**。据此建立全新授权 `AUTH-E3-PHYS1API26-20260816-0003`、campaign `E3-PHYS-PREFLIGHT-20260816-0003` 与 evidence `EV-E3-PHYS1API26-20260816-0003`，`attempt: initial`、retry N/A。本授权取代并且绝不复用已退役的 [`AUTH-E3-PHYS1API26-20260816-0002`](e3-physical-preflight-authorization-2026-08-16-0002.md)。

> 当前状态：`authorization_status: blocked-awaiting-full-gates`，not ready。0003 是全新 initial，不是 0002 retry。0002 因 gate 1 未完成时 agent 误触 `hdc list targets`、使 gate 4 操作提前而 `governance-order-invalid-retired`；该输出未披露、未落盘，随后 daemon 以 host process termination 清理并由进程表确认 HDC0。命令级清理审计未持久化，因此不把该清理算作任何合规 gate 操作，也不声称已执行 device evidence 或具体清理命令；gate 2-13 均未运行，仓外不存在 0002 audit/freeze/record/evidence/lock。0002 未执行 Live、未 consumed，仍永久不可复用。0003 的 candidate ID consumption audits、confirmation、review、blocked/ready freeze、DryRun 与 Live 均未执行，本文不预写任何 0003 pass、record、freeze 或 campaign hash。E3 未关闭，E8 保持 `CLOSED`。

> gate 1 前置治理选择：用户明确批准在 0 blocker / 0 major 审查和 host-only tests 通过后，对 0003 registration、修正后的 source provenance binding 与 pair migration 执行 commit/push，以形成 gate 1 所需的 exact clean HEAD。该选择只授权届时的治理提交与推送，不授权 campaign evidence；本次 host-only 登记不执行 commit/push。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260816-0003
supersedes: AUTH-E3-PHYS1API26-20260816-0002
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
dry_run: pending
live: pending
campaign_status: not-run
live_consumed: false
hap_source_basis_commit: 62409c5f966d00597b58f68ae5b927dd06e76e76
runner_code_basis_commit: 9c1d464fa78214dee7c1c00f4870aa26549af1a2
code_sha: pending-final-clean-head
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0003
  evidence_id: EV-E3-PHYS1API26-20260816-0003
  attempt: initial
  retry: N/A
  identity_status: pending-two-consumption-audits
  consumed: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0002
  evidence_id: EV-E3-PHYS1API26-20260816-0002
  authorization_id: AUTH-E3-PHYS1API26-20260816-0002
  status: governance-order-invalid-retired
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
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260816-0003.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260816-0003.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260816-0003
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260816-0003
```

`reusable: true` 仅表示 0003 尚未被治理失败或执行消费；它不表示 ready，也不允许跳过任一 gate。任一 gate 失败后须立即停止并重新治理。

## 双 SHA 与 source provenance

冻结必须同时绑定两个独立事实，二者不得互相替代：

1. `hap_source_basis_commit` 是冻结 HAP A/B 的实际构建源码基础，精确为 `62409c5f966d00597b58f68ae5b927dd06e76e76`。source archive 从该 commit 的 git object database 直接导出，不读取 dirty worktree，也不重建或重签 HAP。
2. `code_sha` 是最终 runner、PowerShell parity、selftests、freeze example、本登记及当前治理文档所在的 exact clean HEAD；当前保持 `pending-final-clean-head`，只能在 gate 1 现场计算。该 HEAD 必须包含 S6 B 修复基础 `9c1d464fa78214dee7c1c00f4870aa26549af1a2` 及本次 0003 migration。

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

freeze schema 的 `source` 对象不新增 size 字段：`archive_path`/`archive_sha256` 与 `manifest_path`/`manifest_sha256` 绑定真实对象，archive size 由已绑定 manifest 内 `source_archive.size_bytes` 核验。HAP size 同样由 source manifest 的 `frozen_hap_correspondence` 与 review 复核，不改变 freeze contract schema。

机械解包核验确认：source `build-profile.json5` 将 A/default 绑定公开 bundle `cn.alfadb.netbird.e3physvpna`，将 B/vpnB 绑定公开 bundle `cn.alfadb.netbird.e3physvpnb`；`entry/build-profile.json5` 将 default ability 绑定 `$string:ability_name`、vpnB ability 绑定 `$string:ability_name_b`，分别解析为 `E3 Preflight A` / `E3 Preflight B`。冻结 HAP A/B 的 distinct label 依据是 `module.json` 中的 label resource binding 经 `resources.index` 解析；`pack.info` 只证明公开 bundle、product 与 target，不作为 distinct label 依据。上述 HAP metadata 与 62409c5 source tree 一致，HAP hash/size 未变。archive 不含 signingConfig、证书、profile、keystore、密码或私钥材料。

证据边界：冻结 HAP 的构建早于 commit `62409c5f966d00597b58f68ae5b927dd06e76e76`；在禁止重建、重签的前提下，metadata correspondence 不能字节级证明 HAP 内 compiled `ets/modules.abc` 或 native `.so` 必由该 tree 产出。0005 production evidence 中 A/B distinct labels 仅作旁证，不是 metadata correspondence 或 compiled payload 字节证明的基础。该边界不削弱 source binding 结论：source archive 仍精确绑定 62409c5 的 22 个 git blobs，冻结 HAP metadata 在上述边界内与该 tree 对应。

## 0002 退役边界

0002 在 gate 1 尚未完成时，agent 误触本应仅允许在 gate 4 执行一次的 `hdc list targets`，造成治理操作顺序失效。该调用的 stdout/stderr 未披露、未写入仓库或仓外文件；随后 daemon 以 host process termination 清理，并仅通过进程表读取确认 HDC0。命令级清理审计未持久化，故该清理不计作任何合规 gate 操作；不得主张具体清理命令，也不声称已执行 device evidence。gate 1 未完成，gate 2-13 全部 `not-run`；仓外不存在 0002 audit、blocked/ready freeze、confirmation/review record、evidence root 或 lock。0002 没有设备证据、没有 Live、没有 pair consumption；但 AUTH/pair 已按治理失败永久退役，不得复用、补跑或作为 0003 retry basis。

## S5 与 S6 操作员动作

S5 只接受系统 Settings owner 下唯一可见 `Setting.AppDetail` 子树，且同一子树必须同时包含 distinct label `E3 Preflight A` 与 force-stop 控件。操作员从应用管理进入 A 详情，点击 `强行停止` 并完成随后出现的确认（如有）。post-force capture 只作 observation；最终撤销仍由连续 `<bundle>:vpn` absent（至少 2 次、间隔至少 3 秒）且 bundle present 的机器门确认。

S6 固定为 A Start step 1、A optional Allow step 2、B Start step 3、B optional Allow step 4。runner 的实际 operator prompt 是 `点击 Allow`；设备上对应的可见控件文字为 `允许`。仅在机器识别 authorization profile 并显示该 prompt 时，操作员点击设备上的 `允许` 控件后按回车；无 prompt 时禁止自行寻找或点击。其余 S6 terminal 优先级、requestId 约束、冲突码 `2203002`、process checkpoint 与 fail-closed 语义不变。

S6 B production fixture 的实际仓外 raw basename 是 `capture-scenario-6-conflict.json`；public sealed reference 可带 `RAW-` 前缀，但不是文件 basename。该 raw SHA-256 为 `7f6e44d5eab7021d192a6a61f409af9a3e8507f16c1df6bcf84755c1130bb72c`；仓内最小脱敏 fixture `tests/fixtures/s6-b-authorization-production-0005.json` 的 0005 仅表示不可迁移 provenance。

## 白名单、隐私与签名安全边界

runner HDC 白名单仍精确为 22 项：`Version`、`TupleModel`、`TupleBuild`、`MkdirStaging`、`RemoveStaging`、`StagingProbe`、`SendA`、`SendB`、`InstallA`、`InstallB`、`FaultA`、`FaultB`、`HilogStream`、`BundleDump`、`PidOf`、`Uninstall`、`StartEntry`、`ScreenCap`、`DumpLayout`、`ReceiveScreen`、`ReceiveLayout`、`ForceStop`。`ForceStop` 只允许 `exception-cleanup` / `final-cleanup` residual cleanup，不用于 S5 revoke 或 verdict。

13 门 gate 4 的一次受控 `hdc tconn <runtime-endpoint>` 与紧随其后的一次内存级 `hdc list targets` 是 runner 外窄例外；两条各只允许一次，不输出 stdout/stderr，解析必须得到唯一非空 token，并只映射到当前 host 进程级 `PHYS_1_TARGET`。禁止额外 discovery、再次连接/列举、全量 bundle/process dump、`hidumper`、root、privileged、自动 UI 输入、Go、NetBird、product 操作或局部场景重放。

endpoint、target、serial、UDID、设备身份值、账号、凭据、私钥与签名私密材料不得写入仓库、freeze、record、evidence、日志、命令输出或聊天转录。仓内只允许公开 tuple、公开 bundle/resource id、脱敏 hash 与受控仓外路径模板。普通开发签名的 profile/certificate/keystore/password/private key 均保持仓外；仓内只登记公开验证结论、公开 fingerprint（或带理由的 N/A）和冻结 HAP hash。不得读取、复制、输出或重新生成签名私密材料，不得重建或重签 HAP。

## 完整 13 门

1. **host-only gate 1**：同步 trusted refs/bundle；核对最终 HEAD 包含 `9c1d464fa78214dee7c1c00f4870aa26549af1a2`、本登记及 0003 常量迁移，且 worktree exact clean。记录最终独立 `code_sha`；同时核对 HAP source basis 精确为 `62409c5f966d00597b58f68ae5b927dd06e76e76`。本门只允许读取 git、仓内/受控仓外文件和主机进程表；**绝不可调用任何 HDC 可执行文件或子命令，包括 version/list/kill**。HDC0 只可由 `ps -eo comm,args` 或 `pgrep -x hdc` 判定。freeze 后至 campaign 结束治理字节不得改动。
2. 对 0003 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何 Live/evidence/seal 占用立即停止。
3. 新建 0003 blocked confirmation freeze 并完成静态独立审查；不得复制或原地修改 0002 freeze。freeze 同时绑定最终 `code_sha`、既有修复后 source archive hash、source manifest hash、同一冻结 HAP A/B 与全部外部输入；plan、machine confirmation、review、DryRun、Live 保持 pending/not ready。
4. 用户在本地主机只执行一次受控 `hdc tconn <runtime-endpoint>`，随后只执行一次内存级 `hdc list targets`；这是 gate 1-3 完成后首次允许调用 HDC。必须得到唯一 token 并建立进程级 `PHYS_1_TARGET`，不得输出或持久化 endpoint、target 或命令 stdout/stderr。
5. 以 0003 confirmation record 路径运行 `-TargetBindingConfirm`；只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，完成 machine fresh confirmation。任何失败停止。
6. 新建 0003 ready freeze draft，绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，不得原地改 blocked freeze。
7. 由与 operator 不同的独立 reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion。
8. 新建最终 0003 ready freeze，绑定 review record、最终 exact clean HEAD、既有 source provenance、runner bytes、同一 A/B HAP 与全部外部输入；不得修改旧 freeze。
9. 对 0003 新 pair 执行 candidate ID consumption audit-2，仓外记录并写 `.sha256`；任何占用立即停止。
10. 执行 host-only Python selftest 与 PowerShell selftest，确认 `HDC_PROCESSES=0`；清理并核验仓内无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，并复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量及 `operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 监控。不得因终端暂时无输出擅自中断，不得 retry。

任一 gate 失败立即停止，不自动 retry、不换 ID。任何后续尝试都必须重新治理并取得新 AUTH/pair；0003 不允许从中间 gate 开始或只重放 S6。即使 0003 最终通过，也不自动开放 E8。

## 当前 host-only 登记边界

本次只允许建立 0003 治理、迁移 runner/selftests/freeze example 与当前文档，并执行 host-only 自测、host audit、JSON/diffcheck、敏感扫描、pycache 与 HDC 进程表核验。禁止执行任何 HDC 可执行文件或子命令、设备命令、`TargetBindingConfirm`、0003 audit、任何 campaign blocked/ready freeze、record、DryRun、Live、commit 或 push。0B0M 与 host-only tests 通过后的 registration/source/pair migration commit/push 已获用户批准，但不在本次操作内执行；当前登记完成不等于 gate 1 完成，也不使 campaign ready。
