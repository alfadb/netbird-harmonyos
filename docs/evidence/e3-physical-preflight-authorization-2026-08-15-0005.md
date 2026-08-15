# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-15 · 0005）

最后核验：2026-08-15

本文登记用户（直接人类决策者）于本轮明确选择继续**完整 S5 修复 + 新治理**，并批准新授权 `AUTH-E3-PHYS1API26-20260815-0005`。它取代已消费的 [`AUTH-E3-PHYS1API26-20260815-0004`](e3-physical-preflight-authorization-2026-08-15-0004.md)；0004 Live 已 sealed invalid，其 AUTH、campaign ID 与 evidence ID 均不得复用，历史登记、sealed evidence 和 fixture provenance 保留不改写。

> 状态注记：`authorization_status: granted`，`attempt: initial`，retry N/A，`device_readiness: user-attested-ready`。用户就绪声明不替代机器 fresh confirmation。本登记不是 Live evidence；本轮治理准备禁止 HDC、设备命令、Live、仓外 freeze/record/audit 创建、commit 和 push。E3 未关闭，E8 保持 `CLOSED`。

## 授权事实与修复边界

1. 0004 Live 已 sealed invalid 并消费 `E3-PHYS-PREFLIGHT-20260815-0004` / `EV-E3-PHYS1API26-20260815-0004`，不得重用、重判、覆盖或局部重放。
2. 直接根因是 production Settings UI 的按钮文案为 `强行停止`，旧 matcher 未识别；旧 matcher 还会消费整份 dump 的全局 facts，使隐藏 `Setting.Application`、A/B 列表标签、sceneboard 历史文本等子树外内容污染 app-info 判定。
3. 本次 S5 修复只接受 Settings owner 下**唯一可见** `Setting.AppDetail` 子树；候选 AppDetail 自身必须 `visible=true`，且从 Settings owner 到该候选的所有祖先均不得为 `visible=false`。在该子树内同时匹配 expected distinct label（A=`E3 Preflight A`、B=`E3 Preflight B`）与 force-stop 控件。子树外标签、隐藏祖先下的 AppDetail、隐藏导航、搜索页和历史文本均不能命中。
4. production-derived fixture 固定为 `tests/fixtures/settings-app-info-production-0004.json`；其来源、裁剪规则、raw/source hashes 与 schema-normalization 说明见 [`2026-08-15 production layout`](e3-physical-preflight-production-layout-2026-08-15.md)。fixture 文件名中的 0004 是来源 provenance，不迁移为 0005。
5. S5 step4 是单一机械动作：`点击强行停止，并完成随后出现的确认（如有）`。runner 不自动点击、不猜确认框结构；post-force capture 仅 observation-only，不因页面离开 AppDetail、结构变化或一般 capture 退化而独立 invalid/block/pass。连续 HiLog stream 已 degraded 时仍按全局安全规则 blocked。
6. S5 最终撤销效果只由既有决定性机器门确认：连续 `<bundle>:vpn` absent（至少 2 次、间隔至少 3 秒）且 bundle present。process 未退出、探针不足或 bundle presence 未确认时 fail-closed。
7. Python runner、自测、production fixture 与必要 PowerShell parity 已覆盖上述语义。`isolated-anthropic-claude-opus-5-reviewer` 对最终 S5 修复审查结果为 0 blocker / 0 major；三个 minor 仅为文档闭环，已在本次治理准备中处理。
8. A/B HAP 沿用 0004 同一 ordinary-development 签名输入及原 hash/size，不重建、不重签、不改字节。
9. Python 与 PowerShell runner/selftest 的当前候选常量均迁移到 0005；freeze example 同步绑定 0005。production fixture 路径和全部 historical evidence/docs/fixture provenance 的 0004 引用不迁移。
10. `code_sha` 暂记 `pending-final-commit`。后续 commit/push 完成后，仓外 freeze 必须绑定实际最终 HEAD；从该 commit 起，本登记文档不得再改字节，直至本 campaign 结束。若必须修改，停止并重新治理，不得让 freeze 绑定发散文档。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260815-0005
supersedes: AUTH-E3-PHYS1API26-20260815-0004
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: collected
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: not-run-authorization-registration
is_evidence: false
authorization_status: granted
approval: user explicit choice to continue full S5 repair plus new governance; 2026-08-15
device_readiness: user-attested-ready
machine_fresh_confirmation: pending
plan_status_at_registration: authorized-awaiting-linux-ready-freeze
campaign_status: candidate-new-created-not-live-pending-audits
independent_review_record: pending
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
s5_final_review: 0 blocker / 0 major
code_sha: pending-final-commit
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260815-0005
  evidence_id: EV-E3-PHYS1API26-20260815-0005
  attempt: initial
  retry: N/A
  identity_status: pending-two-consumption-audits
  live: false
  consumed: false
prior_candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260815-0004
  evidence_id: EV-E3-PHYS1API26-20260815-0004
  status: consumed-sealed-invalid
  reusable: false
repo_bytes:
  runner_path: spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.py
  runner_sha256: c2430858877befbe6371e4074953d049574ec83c20fe4184809f48191681ab48
  selftest_path: spikes/e3-vpn-extension-physical-preflight-hap/tests/e3-phys-preflight-runner-selftest.py
  selftest_sha256: ac2e91c9c2b7375c4b5b25a75a15df79736f747f1a6f2e0b240c90fc86960318
  freeze_example_path: spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-freeze.example.json
  freeze_example_sha256: 1f0bea563abb38501097b087f5f59d3ec8bd1cc2182ffcfcabe100808e6a1bb6
  powershell_parity: spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.ps1 and tests/e3-phys-preflight-runner-selftest.ps1 are not part of the frozen three hashes; host PowerShell selftest covers parity.
production_fixture:
  path: spikes/e3-vpn-extension-physical-preflight-hap/tests/fixtures/settings-app-info-production-0004.json
  sha256: 0d225f338b702c728096af16bb00b10fdf61ecf33c0b96ea2d8643168a3ce233
  source_campaign: E3-PHYS-PREFLIGHT-20260815-0004
  source_raw_capture_sha256: a10d7828c7cb7d0e41d592332718f0c60e85473e4dc62244250a84421aa0c62c
signed_hap_frozen:
  root: $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/
  hap_a_sha256: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_a_size: 133941
  hap_b_sha256: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
  hap_b_size: 133946
hdc:
  sha256: 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81
  version: 3.2.0d
roles:
  operator_role: authorized-human-operator
  independent_reviewer_role: isolated-anthropic-claude-opus-5-reviewer
record_paths:
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260815-0005.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260815-0005.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260815-0005
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0005
```

## 0004 历史处置

0004 sealed invalid 是已形成的历史 Live evidence：S1-S4 pass、S5 step1 pass，S5 step3 因 production 文案 `强行停止` 未被旧 matcher 接受而 invalid，S6-S7 `not-run-due-to-invalid`。其生产 dump 同时暴露旧全 dump facts 会让 AppDetail 子树外内容污染判定。0004 AUTH、pair、record、seal、evidence roots 与 `code_sha` 只绑定该历史执行，均不得改写或迁移到 0005。

0005 是全新 initial，不是 0004 的 retry。`retry.basis`、`infrastructure_reason`、`prior_record_path` 与 `prior_record_sha256` 均为 N/A；任一 gate 失败立即停止，不自动 retry、不切换 ID。任何后续尝试都需要新治理与新授权。

## 13 步顺序门

1. 同步 trusted refs/bundle；基于含本登记及 S5 修复的最终 commit，核对 clean worktree，并冻结实际 HEAD。此后本登记文档至 campaign 结束不得改字节。
2. 对 0005 新 pair 执行 candidate ID consumption audit-1，仓外记录并写 `.sha256`；发现任何 live/evidence/seal 占用即停止。
3. 生成全新 blocked confirmation freeze，并完成静态独立审查；不得复用或原地改写 0004 freeze。
4. 用户在本地主机只执行一次 `hdc tconn <runtime-endpoint>`，其 stdout/stderr 不向终端或日志转发；紧接着只执行一次 `hdc list targets`，输出仅在内存中解析。runtime endpoint 与返回的 target token 均不得输出、持久化或写入 freeze/record/evidence；`list targets` 必须解析出恰好一个非空 target token，否则立即 STOP。仅将该 token 映射为当前 host 进程内的 `PHYS_1_TARGET`，进程结束即失效。不得在文档、命令示例或记录中写实际 endpoint/target。
5. 以 0005 新 confirmation record 路径执行现行 Linux Python runner 的 `-TargetBindingConfirm`；仅允许 `Version`、`TupleModel`、`TupleBuild` 三个白名单探针，完成机器 fresh confirmation。
6. 新建 ready freeze draft，绑定 confirmation record hash，并保持 blocked/ready 两阶段 confirmation contract 字节一致。
7. 由 `isolated-anthropic-claude-opus-5-reviewer` 生成新的 `e3-ready-freeze-review` record 与 `.sha256` companion。
8. 新建最终 ready freeze，绑定 review record；不得修改旧 freeze。最终 freeze 绑定实际 HEAD、上述三项 repo hash、同一 HAP A/B 与全部外部输入 hash。
9. 对 0005 新 pair 执行 candidate ID consumption audit-2，仓外记录并写 `.sha256`；发现占用即停止。
10. 执行 host-only Python selftest 与 PowerShell selftest，确认 `HDC_PROCESSES=0`；运行后删除并核验仓内不存在 `__pycache__/` 与 `*.pyc`，再核验 repository clean。`.gitignore` 只防止误跟踪，不能替代该清理与核验。
11. 对同一最终 ready freeze 执行 host-only DryRun，要求 `is_evidence=false`、HDC0、integrity empty。
12. 独立审查 DryRun，并复算最终 ready freeze SHA-256，确认 freeze 字节未改变。
13. 仅执行一次 Live；环境设置 `PYTHONUNBUFFERED=1`，只按证据目录增量及 `operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 监控，不以终端输出空白擅自中断。

## 白名单、安全与敏感纪律

允许范围仅为现行 runner 白名单：target-binding 三探针、定向 A/B bundle/PID/install/start/cleanup、单一连续 `E3PhysVpn` HiLog、A/B fault、screen/layout capture，以及 S3/S5/S7 的定向 `PidOf`/`BundleDump` observation。gate 4 的一次 `hdc tconn` 加紧随其后的一次内存级 `hdc list targets` 是 **runner 外的窄 host-prep 例外**，只用于建立进程内 `PHYS_1_TARGET` 映射，不进入也不扩大 runner 命令白名单。HDC force-stop 只可用于 finally residual cleanup，绝不用于 S5 revoke 或 verdict。S5 UI 动作由普通可见 Settings UI 完成，runner 不扩展自动 UI 输入。

除 gate 4 明确列出的两条一次性 host-prep 命令外，额外 discovery 仍全部禁止，包括再次连接/列举、全量 bundle/process dump、`hidumper`、root、privileged、Go、NetBird、product 动作或局部 S5 重放。任何敏感连接值、设备身份值、凭据、账号或签名私密材料均不得写入仓库、freeze、record、evidence、日志或聊天转录。仓内只登记公开 tuple、公开 bundle/resource id、脱敏 hash 与受控仓外路径模板。

本次 host-only 治理准备不创建任何仓外 freeze/record/audit，不运行 HDC/设备/Live，不 commit/push。下一步只能在 commit/push 后按上述 13 门从第 1 步开始；E8 在任何结果下仍保持 `CLOSED`，除非后续独立治理明确改变。
