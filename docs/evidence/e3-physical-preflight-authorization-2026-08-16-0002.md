# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-16 · 0002，governance-order-invalid-retired）

最后核验：2026-08-16

本文历史登记曾建立 `AUTH-E3-PHYS1API26-20260816-0002`、campaign `E3-PHYS-PREFLIGHT-20260816-0002` 与 evidence `EV-E3-PHYS1API26-20260816-0002`，`attempt: initial`、retry N/A。该授权原计划保留 20260816-0001 的 gate-3 review-blocked 历史、采用修复后的 source provenance，并进入完整 13 门；现已由 [`AUTH-E3-PHYS1API26-20260816-0003`](e3-physical-preflight-authorization-2026-08-16-0003.md) 取代。

> 退役状态：`authorization_status: governance-order-invalid-retired`，`reusable: false`。gate 1 尚未完成时，agent 误触本应位于 gate 4 的 `hdc list targets`，造成 gate 4 操作提前、治理门顺序失效。该调用的 stdout/stderr 未披露、未落盘；随后 daemon 以 host process termination 清理，并仅通过进程表确认 HDC0。命令级清理审计未持久化，故该清理不计作任何合规 gate 操作；不得主张具体清理命令，也不声称已执行设备证据。gate 1 未完成，gate 2-13 全部 `not-run`；仓外不存在 0002 audit、blocked/ready freeze、confirmation/review record、evidence 或 lock。0002 未执行 Live、未 consumed，不构成设备证据，也不构成 pair consumption，但 AUTH/pair 已按治理失败永久退役。

> 用户已批准迁移至 0003 的完整 13 门，并批准在 0 blocker / 0 major 审查和 host-only tests 通过后对 0003 registration、source provenance binding 与 pair migration 执行 commit/push。该后续授权不复活 0002；本次 host-only 治理不执行 commit/push。

## 授权状态

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260816-0002
supersedes: AUTH-E3-PHYS1API26-20260816-0001
superseded_by: AUTH-E3-PHYS1API26-20260816-0003
exception: E3-PHYS-PREFLIGHT
information_status: historical-governance-registration
record_status: registered-not-run
stage_or_gate: E3
governance_gate_reached: gate-1-incomplete
related_stages_or_gates: [E8]
execution: host-governance-order-invalid
is_evidence: false
authorization_status: governance-order-invalid-retired
plan_status: governance-order-invalid-retired
ready: false
reusable: false
device_readiness: user-attested-ready
machine_fresh_confirmation: not-run
independent_review: not-run
blocked_confirmation_freeze: not-run
ready_freeze: not-run
dry_run: not-run
live: not-run
live_consumed: false
campaign_status: not-run
hap_source_basis_commit: 62409c5f966d00597b58f68ae5b927dd06e76e76
runner_code_basis_commit: 9c1d464fa78214dee7c1c00f4870aa26549af1a2
code_sha: pending-final-clean-head
head_requirement: exact-clean-final-governance-commit-descending-from-runner-code-basis
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0002
  evidence_id: EV-E3-PHYS1API26-20260816-0002
  attempt: initial
  retry: N/A
  identity_status: governance-order-invalid-retired
  consumed: false
  reusable: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260816-0001
  evidence_id: EV-E3-PHYS1API26-20260816-0001
  authorization_id: AUTH-E3-PHYS1API26-20260816-0001
  status: governance-review-blocked-retired
  live_consumed: false
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
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260816-0002.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260816-0002.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260816-0002
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260816-0002
```

## 双 SHA 与 source provenance

冻结必须同时绑定两个独立事实，二者不得互相替代：

1. `hap_source_basis_commit` 是冻结 HAP A/B 的实际构建源码基础，精确为 `62409c5f966d00597b58f68ae5b927dd06e76e76`。新 source archive 从该 commit 的 git object database 直接导出，不读取 dirty worktree，也不重建或重签 HAP。
2. `code_sha` 是最终 runner、PowerShell parity、selftests、freeze example、本登记及当前治理文档所在的 exact clean HEAD；当前保持 `pending-final-clean-head`，只能在 gate 1 现场计算。该 HEAD 必须包含 S6 B 修复基础 `9c1d464fa78214dee7c1c00f4870aa26549af1a2` 及本次 0002 migration。

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

## 0001 退役边界

0001 audit-1 SHA-256 固定为 `a1275d416ff120c26a6de1eaaa4f7f9fde931a7f2a674401d43c1cdd5c24695d`；blocked freeze SHA-256 固定为 `70e5a1d56b74595a25c93500d7e422021e3754e66798bbb2a43a82f2e80b675a`；其 confirmation contract SHA-256 固定为 `f3ca48c79d185e0191b774bfc4be935c78c886cca88e5aeedf6e93a8d643f118`。独立审查结论为 0 blocker / 1 major：旧 source manifest 的 source basis 与实际冻结 HAP source basis 不一致。0001 因此停在 gate 3，gate 4-13 全部 `not-run`，没有 Live consumption；但其 pair 已按治理失败永久退役，不得复用、补跑、改写 freeze 或转作 0002 retry basis。

## S5 与 S6 操作员动作

S5 只接受系统 Settings owner 下唯一可见 `Setting.AppDetail` 子树，且同一子树必须同时包含 distinct label `E3 Preflight A` 与 force-stop 控件。操作员从应用管理进入 A 详情，点击 `强行停止` 并完成随后出现的确认（如有）。post-force capture 只作 observation；最终撤销仍由连续 `<bundle>:vpn` absent（至少 2 次、间隔至少 3 秒）且 bundle present 的机器门确认。

S6 固定为 A Start step 1、A optional Allow step 2、B Start step 3、B optional Allow step 4。runner 的实际 operator prompt 是 `点击 Allow`；设备上对应的可见控件文字为 `允许`。仅在机器识别 authorization profile 并显示该 prompt 时，操作员点击设备上的 `允许` 控件后按回车；无 prompt 时禁止自行寻找或点击。其余 S6 terminal 优先级、requestId 约束、冲突码 `2203002`、process checkpoint 与 fail-closed 语义不变。

S6 B production fixture 的实际仓外 raw basename 是 `capture-scenario-6-conflict.json`；public sealed reference 可带 `RAW-` 前缀，但不是文件 basename。该 raw SHA-256 为 `7f6e44d5eab7021d192a6a61f409af9a3e8507f16c1df6bcf84755c1130bb72c`；仓内最小脱敏 fixture `tests/fixtures/s6-b-authorization-production-0005.json` 的 0005 仅表示不可迁移 provenance。

## 白名单与安全边界

runner HDC 白名单仍精确为 22 项：`Version`、`TupleModel`、`TupleBuild`、`MkdirStaging`、`RemoveStaging`、`StagingProbe`、`SendA`、`SendB`、`InstallA`、`InstallB`、`FaultA`、`FaultB`、`HilogStream`、`BundleDump`、`PidOf`、`Uninstall`、`StartEntry`、`ScreenCap`、`DumpLayout`、`ReceiveScreen`、`ReceiveLayout`、`ForceStop`。`ForceStop` 只允许 `exception-cleanup` / `final-cleanup` residual cleanup，不用于 S5 revoke 或 verdict。

13 门 gate 4 的一次受控 `hdc tconn <runtime-endpoint>` 与紧随其后的一次内存级 `hdc list targets` 是 runner 外窄例外；两条各只允许一次，不输出 stdout/stderr，解析必须得到唯一非空 token，并只映射到当前 host 进程级 `PHYS_1_TARGET`。禁止额外 discovery、再次连接/列举、全量 bundle/process dump、`hidumper`、root、privileged、自动 UI 输入、Go、NetBird、product 操作或局部场景重放。

endpoint、target、serial、UDID、设备身份值、账号、凭据、私钥与签名私密材料不得写入仓库、freeze、record、evidence、日志、命令输出或聊天转录。仓内只允许公开 tuple、公开 bundle/resource id、脱敏 hash 与受控仓外路径模板。

## 历史计划的完整 13 门（未执行）

以下是 0002 退役前的计划，实际 gate 1 未完成，gate 2-13 均未运行。

1. 同步 trusted refs/bundle；核对最终 HEAD 包含 `9c1d464fa78214dee7c1c00f4870aa26549af1a2`、本登记及 0002 常量迁移，且 worktree exact clean。记录最终独立 `code_sha`；同时核对 HAP source basis 精确为 `62409c5f966d00597b58f68ae5b927dd06e76e76`。freeze 后至 campaign 结束治理字节不得改动。
2. 对 0002 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；任何 Live/evidence/seal 占用立即停止。
3. 新建 0002 blocked confirmation freeze 并完成静态独立审查；不得复制或原地修改 0001 freeze。freeze 同时绑定最终 `code_sha`、新 source archive hash、source manifest hash、同一冻结 HAP A/B 与全部外部输入；plan、machine confirmation、review、DryRun、Live 保持 pending/not ready。
4. 用户在本地主机只执行一次受控 `hdc tconn`，随后只执行一次内存级 `hdc list targets`；必须得到唯一 token 并建立进程级 `PHYS_1_TARGET`，不得输出或持久化敏感值。
5. 以 0002 confirmation record 路径运行 `-TargetBindingConfirm`；只允许 `Version`、`TupleModel`、`TupleBuild` 三探针，完成 machine fresh confirmation。任何失败停止。
6. 新建 0002 ready freeze draft，绑定 confirmation record；blocked/ready 两阶段 confirmation contract 必须一致，不得原地改 blocked freeze。
7. 由与 operator 不同的独立 reviewer 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion。
8. 新建最终 0002 ready freeze，绑定 review record、最终 exact clean HEAD、新 source provenance、runner bytes、同一 A/B HAP 与全部外部输入；不得修改旧 freeze。
9. 对 0002 新 pair 执行 candidate ID consumption audit-2，仓外记录并写 `.sha256`；任何占用立即停止。
10. 执行 host-only Python selftest 与 PowerShell selftest，确认 `HDC_PROCESSES=0`；清理并核验仓内无 `__pycache__/`、`*.pyc`，再次核对 exact clean HEAD。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、`HDC_PROCESSES=0`、`integrity_violations=[]`。
12. 独立审查 DryRun，并复算最终 ready freeze SHA-256，确认 freeze 字节未变化。
13. 仅执行一次完整 Live；设置 `PYTHONUNBUFFERED=1`，仅按证据目录增量及 `operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 监控。不得因终端暂时无输出擅自中断，不得 retry。

0002 已因治理门顺序失效停止，不允许从中间 gate 继续、只重放 S6、补跑或 retry；后续仅可按新 AUTH/pair 重新治理。

## 退役后的 host-only 边界

0002 不再允许继续任何 gate、audit、freeze、record、DryRun 或 Live，也不得创建仓外 0002 evidence/lock。后续 host-only 治理只可迁移至 0003；0002 的 HDC 误触不披露设备输出、不落盘、不登记为设备证据，并且不把 pair 标为 consumed。host process termination 后的进程表 HDC0 只确认当时无残留 daemon；命令级清理审计未持久化，因此不构成任何合规 gate 操作，也不支持对具体清理命令或设备操作作出断言。
