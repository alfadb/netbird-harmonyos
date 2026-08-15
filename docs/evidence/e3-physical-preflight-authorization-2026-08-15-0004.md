# E3-PHYS-PREFLIGHT 物理设备执行授权登记（2026-08-15 · 0004）

最后核验：2026-08-15

本文登记用户（直接人类决策者）于 2026-08-15 约 14:10 +08:00 的显式批准（聊天回复“继续”，方案 A）：在彻底修复 ability label 后，以**新**候选 pair 重跑 `E3-PHYS-PREFLIGHT`。本登记授权 `AUTH-E3-PHYS1API26-20260815-0004`，取代已消费的 [`AUTH-E3-PHYS1API26-20260815-0003`](e3-physical-preflight-authorization-2026-08-15-0003.md) 的未完成 Live；0003 历史登记和 sealed invalid evidence 保留不改写。

> 状态注记：本登记原始授权含义不变，且其唯一获批 Live 已执行并 sealed invalid，故该 AUTH/pair 已消费，不能复用。本文件仍是授权登记，不重判 Live evidence，也不授权当前任务运行 HDC、设备命令、Live、AUTH/pair 迁移、commit 或 push。

```yaml
authorization_id: AUTH-E3-PHYS1API26-20260815-0004
supersedes: AUTH-E3-PHYS1API26-20260815-0003
exception: E3-PHYS-PREFLIGHT
information_status: historical-execution-input
record_status: consumed-live-sealed-invalid
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live-completed-historical
is_evidence: false
authorization_status: consumed
approval: user explicit approval "继续"; plan A; 2026-08-15 approximately 14:10 +08:00
plan_status_at_registration: authorized-awaiting-linux-ready-freeze
campaign_status: candidate-new-created-not-live-pending-audits
reviewer_role: isolated-anthropic-claude-opus-5-reviewer
code_sha: 62409c5f966d00597b58f68ae5b927dd06e76e76 # frozen historical Live input; do not replace with the current worktree
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260815-0004
  evidence_id: EV-E3-PHYS1API26-20260815-0004
  attempt: initial
  retry: N/A
  identity_status: consumed-sealed-invalid
  live: true
  consumed: true
repo_bytes:
  runner_sha256: 05e3eff34694937a4a8ba3d937580a1353a2a693f89124d6fd1e8bd37a0644ed
  selftest_sha256: 56fdb79e00eab9fd7d62be10bbe12b209eee4366909742a81a749e3b2c576453
  freeze_example_sha256: 924d3c043affe9920727e278f1e2ecc717c31f0b15e410300b81ddd9ac4a6509
signed_hap_frozen:
  root: $HOME/harmonyos-signing/netbird-e3/freeze/20260815-relabeled/artifacts/{a,b}/
  hap_a_sha256: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_a_size: 133941
  hap_b_sha256: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
  hap_b_size: 133946
hdc:
  sha256: 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81
  version: 3.2.0d
record_paths:
  target_binding_confirmation: $HOME/harmonyos-signing/netbird-e3/records/target-binding-confirmation-20260815-0004.json
  ready_freeze_review: $HOME/harmonyos-signing/netbird-e3/records/e3-ready-freeze-review-20260815-0004.json
evidence_roots:
  dry_run: $HOME/harmonyos-signing/netbird-e3/evidence-dry-run-20260815-0004
  live: $HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0004
```

## 0004 Live 历史执行结果

0004 已执行 Live，结果为 **sealed invalid**，候选 `E3-PHYS-PREFLIGHT-20260815-0004` / `EV-E3-PHYS1API26-20260815-0004` 已 `consumed=true`。S1-S4 pass；S5 step1 pass；S5 step3 因当时 matcher 漏掉生产文案 `强行停止` 而 invalid；S6-S7 `not-run-due-to-invalid`。证据根为 `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0004`；生产结构修复登记见 [`2026-08-15 production layout`](e3-physical-preflight-production-layout-2026-08-15.md)。以上仅记录 sealed 执行结果，不改写、不重判其证据。

本登记所冻结的 `code_sha` 是该次 Live 的历史输入，不能用当前工作区 hash 覆盖。当前工作区包含其后的 host-only 修复，已不适用于 0004；任何后续设备执行必须获得新的 AUTH 和新的 campaign/evidence pair。

## 0003 Live 事实与本次修复

0003 Live 于 13:37:22-14:00:55 +08:00：S1 pass；S2 pass（`machine-verified-Allow-onCreate-create-fd`）；S3 pass（`strict-process-boundary-terminal`）；S4 pass（`deny-layout-and-full-window-without-B-create`）；S5 step1 pass；S5 step3 **invalid**：`layout-checkpoint-scenario-5-app-info-layout-mismatch`，`layout-fields-missing:force-stop-control`，capture 为设置搜索页；S6-S7 `not-run-due-to-invalid`；cleanup `verified-clean`；exit 2。sealed invalid evidence 占用 `E3-PHYS-PREFLIGHT-20260815-0003` / `EV-E3-PHYS1API26-20260815-0003`，不得复用，根为 `$HOME/harmonyos-signing/netbird-e3/evidence-live-20260815-0003`。

根因是显示名区分不彻底和页面状态漂移：app label 已区分，但设备 UI 显示 ability label，应用管理列表仍显示旧名，操作员无法区分 A/B，继而 capture 到设置搜索页。修复已完成并作为本 AUTH 的冻结输入：`entry/build-profile.json5` 新增 `vpnB` target，以 `source.abilities` 覆盖 `$string:ability_name_b`；`build-profile.json5` 的 `products.vpnB` 绑定 `entry@vpnB`；`string.json` 的 `ability_name` 为 `E3 Preflight A`、`ability_name_b` 为 `E3 Preflight B`；audit 脚本已适配目标路径并断言 `module.json` label 引用。重建重签 HAP 已经 audit `AUDIT_PASS`、4 项验签 exit 0、归属 `vpna`/`vpnb` 不串扰、zipfile 核验 label 引用正确。0003 的旧 HAP `828fefed...` / `c9e064ed...` 及其大小仅作为 0003 历史输入保留。

## 本次变更与冻结规则

仅迁移治理常量：

- `e3-phys-preflight-campaign.py`：AUTH ID、candidate campaign/evidence ID 和绑定 docstring 迁移至 0004。
- `tests/e3-phys-preflight-runner-selftest.py`：正向 fixture 常量迁移至 0004；`OLD_AUTH_ID` 等负向历史语义不变。
- `e3-phys-preflight-freeze.example.json`：`authorization_id`、campaign/evidence ID 迁移至 0004。

复用不变：runner/selftest/freeze 的逻辑、目标元组、HDC、freeze 决策字段、`reviewer_role`、13 步顺序门、设备连接纪律、`PYTHONUNBUFFERED=1`、E8 `CLOSED` 与 Live 前提交/推送授权。`code_sha` 必须在本登记和三项治理变更的最终 commit 后冻结；提交前重算上列三项 SHA-256，不一致即停止并重新登记。

## 顺序门与纪律

1. sync trusted refs/bundle，并在签名、build、freeze、selftest、DryRun、Live 前后核对 clean worktree 与冻结 HEAD。
2. 对新 pair 执行 audit-1；发现任何 live/evidence/seal 占用即停止。
3. 生成全新 blocked confirmation freeze 并做静态独立审查。
4. 用户本地仅一次 `hdc tconn`，继而仅一次内存级 `hdc list targets`；endpoint 和 token 不落盘，必须恰一 target token。
5. 以新 confirmation record 路径执行 `-TargetBindingConfirm`，只允许 `Version`、`TupleModel`、`TupleBuild` 三个白名单探针。
6. 新建 ready freeze draft，绑定 confirmation record 的 hash。
7. 独立 reviewer 写入新的 ready-freeze review record 与 `.sha256` companion。
8. 新建最终 ready freeze，绑定 review record。
9. 对新 pair 执行 audit-2；发现占用即停止。
10. 执行 host-only selftest，`HDC_PROCESSES=0`。
11. 对同一最终 ready freeze 执行 host-only DryRun。
12. 独立审查 DryRun，且重新计算 ready freeze SHA-256 确认字节不变。
13. 单次 Live；执行环境设置 `PYTHONUNBUFFERED=1`，主会话只按证据目录增量和 `operator-wait-state.json` / `scenario-results.json` 的 `updated_at` 监控，不擅自中断。

任一门失败即停止，不自动 retry、不换 ID。Live 前必须在应用管理列表确认 A/B ability label 已分别显示为 `E3 Preflight A` 与 `E3 Preflight B`；每次设置导航操作后均须重新 capture 验证页面，S5 step3 必须确认目标 app-info 页面出现 `force-stop-control`，否则停止并 seal invalid。禁止额外 discovery、UDID、serial、`hidumper`、root、privileged、Go、NetBird 或 product 动作；禁止将 endpoint、target token、密钥或其他敏感值写入仓库、freeze、record、evidence 或日志。

E3 未关闭；E8 保持 `CLOSED`。上述“Live 前”顺序门和提交/推送授权只描述 0004 当时的原始授权边界，不能将已消费的 0004 AUTH/pair 延伸为当前执行许可；任何继续须新的 AUTH/pair。
