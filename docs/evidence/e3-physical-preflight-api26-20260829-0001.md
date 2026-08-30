# E3-PHYS-PREFLIGHT API26 物理设备 live 预检证据（20260829-0001 / pass）

最后核验：2026-08-30

本文登记 `E3-PHYS-PREFLIGHT-20260829-0001` / `EV-E3-PHYS1API26-20260829-0001` 的完整场景 1–7 live campaign（`AUTH-E3-PHYS1API26-20260829-0001`，attempt initial / retry N/A）。runner 形成 `record_status: collected`、`is_evidence: true`、`overall/verdict: pass` 的正式证据根。独立记录级审查（`isolated-anthropic-claude-opus-5-reviewer`）0 blocker / 0 major / 3 minor。`record_status: reviewed-pass` 表示审查完成且无 integrity blocker；功能判定 `verdict: pass`——**本项目历次 E3-PHYS-PREFLIGHT live 首次完整场景 1-7 全部 machine-verified pass**。`reviewed-pass/pass` **不是** E4-E7、产品、数据面或 E8 结论；E8 保持 `CLOSED`。本 ID 已 `consumed-pass`（Live 单次执行，禁止 retry、禁止同 ID 重跑），**无后继 AUTH**——E3-PHYS-PREFLIGHT 预检就此完结，后续路线（E8/产品化/新阶段）由用户另行治理。

```yaml
evidence_id: EV-E3-PHYS1API26-20260829-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live
is_evidence: true
plan_status_at_live: ready
plan_status_after_registration: consumed-pass
campaign_status: completed-pass
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  full_system_build: PLA-AL10 7.0.0.102(SP8C00E102R7P3)
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
  tuple_basis: four readonly probes measured 2026-08-28 (EV-E3-PHYS1REBIND8-20260828-0001)
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only; target token not projected
signing:
  type: ordinary-development
  device_in_profile: true
  public_certificate_file_sha256: c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc
code_sha: b1302cb53effe727de4fa5d208259175a7e5d7d4
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  HDC: "Ver: 3.2.0d"
  HDC_executable_sha256: 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81
source_archive_sha256: abecc31715585b3047c00f85609a74ab0cefab2b6d328c6810ef987b5fc76888
source_manifest_sha256: 30a22dc6c3a10dd75e6f86e4b7cf06427389e318abe0cb78c72604983e302322
runner_sha256: cfeaef354d464c740e3861836a34194c939b3fcb36c52e54a7c4d74b0192c85e
freeze_manifest_sha256: 1de97bbc128a2e60e0ea1d56744d9a53284ae9eae5a5ebb708ec365954cce832
confirmation_contract_sha256: 83d584ebd18434686e985a9451e2ac0d963fc1a3cdd9b9a1aed44b99558d3880
preflight_inputs_frozen_at: 2026-08-29T22:52:40+08:00
artifact_sha256:
  hap_a: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_b: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
campaign_id: E3-PHYS-PREFLIGHT-20260829-0001
authorization_id: AUTH-E3-PHYS1API26-20260829-0001
attempt: initial
retry_basis: N/A - single governed live; no retry, no ID switch
scenario_window_seconds: 60
operator: authorized user
orchestrator: main agent (user-authorized execution; operator Enter relayed via FIFO)
working_directory: repository-relative spikes/e3-vpn-extension-physical-preflight-hap
command: governed live runner invocation with controlled external inputs; sensitive arguments are not projected
input: frozen target tuple, ordinary-development signing, FINAL A/B HAPs, source/SDK maps, Chinese-prompt runner, cleanup/collection/review gates, and controlled external EvidenceRoot/RawRoot
expected: complete scenarios 1-7 under frozen inputs with truthful operator actions; machine-verified layout/event/process gates; destroy terminal and post-destroy fd snapshot where required
actual: scenarios 1-7 executed; overall pass; S1-S7 all pass; cleanup verified-clean; integrity_violations and capture_degraded empty; independent record review 0 blocker / 0 major / 3 minor
started_at: 2026-08-30T10:14:03.6426710+08:00
ended_at: 2026-08-30T10:31:19.9949990+08:00
clock_source: runner host clock DateTimeOffset.Now; device HiLog CST frozen to +08:00
hdc_execution:
  process_count: 166
  logical_calls: 166
  whitelist_only: true
  note: logical whitelist calls only; no Go/NetBird/WireGuard/MANAGE_VPN/automated device input/privileged bypass
install_and_runtime:
  continuous_capture_started: true
  staging_sent: true
  hap_a_installed: true
  hap_b_installed: true
  hap_a_started: true
  hap_b_started: true
  vpn_scenario_run: true
scenarios:
  scenario_1_cleanup_and_install: pass - machine-cleanup-baseline-and-install
  scenario_2_allow_and_fd: pass - machine-verified-Allow-onCreate-create-fd (fd=33 open=true post-create snapshot; VPN_CREATE_RESOLVED accepted=true)
  scenario_3_active_stop: pass - strict-process-boundary-terminal (absent probes x2 spaced 3.100107s; bundle present; s3_clean_reactivation_proof=true)
  scenario_4_deny: pass - deny-layout-and-full-window-without-B-create (no B create marker in full window)
  scenario_5_settings_revoke: pass - settings-app-info-force-stop-terminal (app-info layout gate; absent probes x2 spaced 3.100137s; bundle present)
  scenario_6_second_vpn_conflict: pass - B-explicit-conflict-rejection (b_rejection_code=2203002; raw verbatim "code=2203002,name=Error,message=VPN exist already, please execute destroy first"; a_accepted=true/b_accepted=false)
  scenario_7_final_cleanup: pass - strict-process-boundary-terminal (unique UI_STOP -> VPN_ONDESTROY -> pre-destroy fd=32 open -> VPN_DESTROY_BEGIN; absent probes x2 spaced 3.100098s; bundle present; window 60.109925s complete_window_observed=true; B-related events in S7 window = 0)
scenario_aggregation:
  overall_rule: integrity invalid > scenario invalid > fail > blocked > pass
  overall: pass
s7_calibration_evidence:
  pre7_checkpoint: pass - exact-process-checkpoint
  b_state: process_present=true, expected_present=null, observed_only=true (B's :vpn process survived the S6 conflict rejection and was recorded observed-only, not gated)
  a_state: expected_present=true, process_present=true (A active gated normally)
  calibration_vs_prior: the identical B state blocked the previous 20260828-0001 live at the S7 precondition (process-state-mismatch expected-active=False actual-active=True); the 2026-08-29 S7 precondition calibration (AUTH-E3-PHYS1API26-20260829-0001) was genuinely exercised, not vacuously passed
device_positive_signals_only:
  - S2: fd=33 open accepted (CREATE_ACCEPTED) with machine-verified Allow and onCreate
  - S3: strict process-boundary terminal with clean reactivation proof
  - S4: deny layout + full 60s window with no B create
  - S5: Settings app-info force-stop revoke terminal (bundle present, :vpn absent)
  - S6: B explicit conflict rejection with frozen code 2203002 while A verified active
  - S7: strict process-boundary terminal for the S6-bound A stop with B residual process recorded observed-only
device_positive_signals_not_e3_pass: true
platform_observation:
  finding: "B's :vpn extension process survives a conflict rejection on this build (confirmed again - S6 terminal observed_only + S7 precondition observed-only; recorded, not gated under the calibration)"
  interpretation: process lifecycle is owned by the startVpnExtensionAbility/stopVpnExtensionAbility pairing, independent of the create outcome; a rejected create leaves the started ability process alive until explicitly stopped
  product_implication: the future NetBird client should explicitly stop the extension ability after a failed create (or avoid keeping the ability started) to reap the process
raw_hilog_reference:
  location: controlled external RawRoot (46 files)
transcript_reference:
  location: controlled external EvidenceRoot projection/transcript.redacted.jsonl
  sha256: cef0de9402660e1202ff7f853fd1658ca192860bdc209704d8559dc4f1537b20
  chain_head: 66f6c1babd74cfdd8747ee08ad0b70c5db5ba97cbf78d6abf6eca0f02d6d7900
  entries: 382
  projection_only: true
screenshot_reference: controlled external RawRoot; scenarios 1-7 captures collected
layout_state_reference: controlled external RawRoot; scenarios 1-7 layouts collected
fault_reference: controlled external RawRoot as produced by live runner; degraded=false
hash_manifest_reference:
  location: controlled external EvidenceRoot hash-manifest.json
  sha256: 8fddaa4941f2c7f27412a5d08cfa559075b030a1935edb6bf17d7cb4560e0c8f
scenario_results_reference:
  location: controlled external EvidenceRoot scenario-results.json
  sha256: 8c948862c98f6cc3a3eab0ea063658929fccfda6751b9247f8b6d593c9c91d2d
  runner_record_status: collected
  is_evidence: true
campaign_seal_reference:
  location: controlled external EvidenceRoot campaign-seal.json
  sha256: 7bf1a42ce5337cebf719ad0ca93cb5132df0fe89cf2f0552e6ae028e04aa2ff6
  binds: scenario-results,hash-manifest
  sealed_at: 2026-08-30T10:31:20.1745980+08:00
forbidden_capabilities_audit: no Go, NetBird, WireGuard, private fork, MANAGE_VPN, privileged bypass, automated device input, or non-whitelisted device query was used
cleanup_result:
  status: verified-clean
  verified_absent: true
  installed_a_remaining: false
  installed_b_remaining: false
  a_process_remaining: false
  b_process_remaining: false
  staging_remaining: false
  note: finally force-stop/uninstall A/B and remove staging; targeted probes verified A/B bundles and processes absent; staging absent; force-stop notUsedAsRevoke residual cleanup only
integrity_violations: []
capture_degraded: []
verdict: pass
scope_statement: exact frozen PLA-AL10/API 26/arm64 full 1-7 attempt only; reviewed-pass/pass is not an E4-E7, product, data-plane, or E8 OPEN conclusion; no extrapolation to another build, device, API, architecture, Emulator, HarmonyOS release, OpenHarmony product, E4-E7, data plane, or product support
reviewers:
  - isolated-anthropic-claude-opus-5-reviewer
reviewed_at: 2026-08-30
review_findings:
  blocker: 0
  major: 0
  minor: 3
  minors_verbatim:
    - m1: scenario-7-post-cleanup review-only capture was not collected; it is observation-only by design and never enters the verdict, and the record field post_cleanup_capture:true derives from having taken the cleanup path rather than from an actually captured artifact - both facts are registered here
    - m2: accepted_session_count_in_window=2 counts marker occurrences (two CREATE_ACCEPTED markers on the same A requestId - VPN_FD_SNAPSHOT and VPN_CREATE_RESOLVED), not sessions; it does not trigger dual-accepted fail (b_accepted=0)
    - m3: the execution window is 2026-08-30 (AUTH/pair 20260829 is the governance naming date)
review_record: independent record review completed 2026-08-30 with 0 blocker / 0 major / 3 minor; record graded reviewed-pass
failure: none - all scenarios passed; overall pass; no retry; no new ID; no device command authorization beyond the consumed governed live
```

## 判定解释

`record_status: reviewed-pass` 只表示独立记录级审查已完成，且证据完整性、封签、哈希绑定、transcript 链、场景聚合与清理边界没有 blocker；功能判定是 `verdict: pass`——**S1-S7 全部 machine-verified pass，本项目历次 E3-PHYS-PREFLIGHT live 首次完整通过**。`reviewed-pass/pass` **不构成** E4-E7、产品、数据面或 E8 结论；E8 保持 `CLOSED`。当前治理 `plan_status: consumed-pass`，**无后继 AUTH**。

逐场景：S1 cleanup/install `pass`；S2 机器验证 Allow → onCreate → create → fd（`fd=33 open=true` post-create 快照 + `VPN_CREATE_RESOLVED accepted=true`）`pass`；S3 严格进程边界 terminal（双 absent 探针间隔 3.100107s、bundle present、`s3_clean_reactivation_proof=true`）`pass`；S4 deny layout 预截图 + 全窗口无 B create `pass`；S5 Settings 应用信息机器门 + 强停后连续 absent 探针（3.100137s）+ bundle present `pass`；S6 B 显式冲突拒绝（`b_rejection_code=2203002`，raw 逐字 `code=2203002,name=Error,message=VPN exist already, please execute destroy first`，`a_accepted=true`/`b_accepted=false`）`pass`；S7 严格进程边界 terminal（unique `UI_STOP`→`VPN_ONDESTROY`→pre-destroy `fd=32 open`→`VPN_DESTROY_BEGIN`，双 absent 探针 3.100098s + bundle present，窗口 60.109925s `complete_window_observed=true`，**S7 窗口内 B 相关事件 = 0**）`pass`。overall `pass`。

**S7 校准生效实证**（本次核心治理成果）：pre7 checkpoint `status=pass reason=exact-process-checkpoint`；B 为 `process_present=true / expected_present=null / observed_only=true`（进程确实存活、按 observed-only 记录、不门控），A 为 `expected_present=true / process_present=true` 正常门控。**同一 B 状态在上一代 20260828-0001 正是 blocked 掉 S7 的 `process-state-mismatch`**——校准被真实触发而非空过（见 [`AUTH-E3-PHYS1API26-20260829-0001`](e3-physical-preflight-authorization-2026-08-29-0001.md) 的 S7 前置校准小节）。

`integrity_violations: []`，`capture_degraded: []`。finally 清理后 A/B bundle、A/B process、staging 均 absent，`cleanup_result.status: verified-clean`。

设备正面信号（仅限下列，**禁止**写成 E4-E7/产品/E8 结论）：S2 fd33 open accepted；S3 进程边界 terminal + 干净再激活；S4 deny 全窗口无 B create；S5 强停撤销 terminal；S6 冻结冲突码 2203002 实测确认；S7 收尾 Stop 进程边界 terminal。

3 条 minor 逐字保留为审查结论，不降级也不改写 runner measured scenario 结果。

本次没有 `infrastructure_reason`；Live 单次执行已消费，`consumed-pass`，**不**授权 retry、**不**自动分配新 ID、**不**授权任何 HDC/设备命令。E3-PHYS-PREFLIGHT 预检就此完结；后续路线（E8 聚合/产品化/新阶段）只可由用户新的治理决定开启。

公开绑定 hash（均仓外 EvidenceRoot/freeze 对象；仓内只登记 hash）：

| 对象 | SHA-256 |
| --- | --- |
| scenario-results.json | `8c948862c98f6cc3a3eab0ea063658929fccfda6751b9247f8b6d593c9c91d2d` |
| hash-manifest.json | `8fddaa4941f2c7f27412a5d08cfa559075b030a1935edb6bf17d7cb4560e0c8f` |
| campaign-seal.json | `7bf1a42ce5337cebf719ad0ca93cb5132df0fe89cf2f0552e6ae028e04aa2ff6` |
| transcript.redacted.jsonl | `cef0de9402660e1202ff7f853fd1658ca192860bdc209704d8559dc4f1537b20` |
| transcript chain_head | `66f6c1babd74cfdd8747ee08ad0b70c5db5ba97cbf78d6abf6eca0f02d6d7900` |
| runner | `cfeaef354d464c740e3861836a34194c939b3fcb36c52e54a7c4d74b0192c85e` |
| final ready freeze | `1de97bbc128a2e60e0ea1d56744d9a53284ae9eae5a5ebb708ec365954cce832` |

gate 1-12 对象链（仓外，逐字节保全）：

| 门 | 对象 | SHA-256 |
| --- | --- | --- |
| gate 2 | audit-1 | `116db451cb60cc0611ba75c22cdaab5c69143159102b9a9ce4ac2a8d38bb3051` |
| gate 3 | blocked confirmation freeze | `cbe8892fa3ebe4efd1a69bec1235742be902b1a4fa1a648dd267673ac3342b75` |
| gate 5 | target-binding confirmation record | `93ef0b5f63788011963fbb26c1b9cf71162441c99bc3cda97c583e887bd3b811` |
| gate 6 | ready freeze draft | `177bc854923d06f5dba02ddbbf5ab3b81550e7e4e1c37a303e6e24c4c314a6ba` |
| gate 7 | e3-ready-freeze-review record | `a654e5e1b738d8f45d378049251f77ee0aa7bc95ac3f117a3234c3e90f00cfb9` |
| gate 8 | final ready freeze | `1de97bbc128a2e60e0ea1d56744d9a53284ae9eae5a5ebb708ec365954cce832` |
| gate 9 | audit-2 | `9eb1ed0d0e99d3d0a84e3503df05425f59471d572ef3440b50990bed6963dfe7` |

## 门状态

- E3-PHYS-PREFLIGHT 预检**已完整通过**（S1-S7 全部 machine-verified pass，`reviewed-pass/pass`）；这不关闭 E4-E7，不开放 E8。
- E8 保持 `CLOSED`（开放需独立聚合审查与其余门，含当前 R0 基线 `EV-E1-EMU24-20260809-0003` 的 E1 reviewed-pass/blocked 现状）。
- `E3-PHYS-PREFLIGHT-20260829-0001` / `EV-E3-PHYS1API26-20260829-0001` 状态：`consumed-pass`，不可复用。
- **无后继 AUTH/pair**；E3 预检完结，后续路线由用户另行治理。
- 禁止把设备正面信号或 `reviewed-pass/pass` 写成 E4-E7/产品/E8 结论。
- 本登记期间禁止 HDC/设备命令。
