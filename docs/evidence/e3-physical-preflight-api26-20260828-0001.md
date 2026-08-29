# E3-PHYS-PREFLIGHT API26 物理设备 live 预检证据（20260828-0001 / blocked）

最后核验：2026-08-29

本文登记 `E3-PHYS-PREFLIGHT-20260828-0001` / `EV-E3-PHYS1API26-20260828-0001` 的完整场景 1–7 live campaign（`AUTH-E3-PHYS1API26-20260828-0001`，attempt initial / retry N/A）。runner 形成 `record_status: collected`、`is_evidence: true`、`overall/verdict: blocked` 的正式证据根。独立记录级审查（`isolated-anthropic-claude-opus-5-reviewer`）0 blocker / 0 major / 3 minor。`record_status: reviewed-pass` 只表示审查完成且无 integrity blocker，**不是** E3 pass；功能判定 `verdict: blocked`。本 ID 已 `consumed-blocked`（Live 单次执行，禁止 retry、禁止同 ID 重跑）。本记录不授权任何 HDC/设备命令，也不得写成 E3 pass。

```yaml
evidence_id: EV-E3-PHYS1API26-20260828-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live
is_evidence: true
plan_status_at_live: ready
plan_status_after_registration: consumed-blocked
campaign_status: completed-blocked
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
code_sha: b24ef557a0a951a146d5ffef035541da8ae39aa9
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  HDC: "Ver: 3.2.0d"
  HDC_executable_sha256: 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81
source_archive_sha256: abecc31715585b3047c00f85609a74ab0cefab2b6d328c6810ef987b5fc76888
source_manifest_sha256: 30a22dc6c3a10dd75e6f86e4b7cf06427389e318abe0cb78c72604983e302322
runner_sha256: 63436249dd24897d5b1f38f2718eb0bb8fd9d55b9fb74b6e10ce6c8ded8218ca
freeze_manifest_sha256: 8ff6b093f855f7bb85dbc6340530258a668b1eef47cf986cf3d0fc7d71b02517
confirmation_contract_sha256: 1f40215dce3f6b793f67112d0ccb002c423191e5079cc5317f7e21c550bfa613
preflight_inputs_frozen_at: 2026-08-28T20:00:20+08:00
artifact_sha256:
  hap_a: 131eef13bcfec4051eb85e706d2936225d81a34394651df2b7bea822ec43eab1
  hap_b: b050cfcec88c59ad5065f3d3089504ff02f8d4c7818f389ef87eb4a9116f6338
campaign_id: E3-PHYS-PREFLIGHT-20260828-0001
authorization_id: AUTH-E3-PHYS1API26-20260828-0001
attempt: initial
retry_basis: N/A - single governed live; no retry, no ID switch
scenario_window_seconds: 60
operator: authorized user
orchestrator: main agent (user-authorized execution; operator Enter relayed via FIFO)
working_directory: repository-relative spikes/e3-vpn-extension-physical-preflight-hap
command: governed live runner invocation with controlled external inputs; sensitive arguments are not projected
input: frozen target tuple, ordinary-development signing, FINAL A/B HAPs, source/SDK maps, Chinese-prompt runner, cleanup/collection/review gates, and controlled external EvidenceRoot/RawRoot
expected: complete scenarios 1-7 under frozen inputs with truthful operator actions; machine-verified layout/event/process gates; destroy terminal and post-destroy fd snapshot where required
actual: scenarios 1-7 executed; overall blocked; S1 pass, S2 pass, S3 pass, S4 pass, S5 pass, S6 pass, S7 blocked; cleanup verified-clean; integrity_violations and capture_degraded empty; independent record review 0 blocker / 0 major / 3 minor
started_at: 2026-08-29T19:12:05.6408700+08:00
ended_at: 2026-08-29T19:30:44.5700310+08:00
clock_source: runner host clock DateTimeOffset.Now; device HiLog CST frozen to +08:00
hdc_execution:
  process_count: 145
  logical_calls: 145
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
  scenario_3_active_stop: pass - strict-process-boundary-terminal (absent probes x2 spaced 3.100119s; bundle present; s3_clean_reactivation_proof=true)
  scenario_4_deny: pass - deny-layout-and-full-window-without-B-create (no B create marker in full window)
  scenario_5_settings_revoke: pass - settings-app-info-force-stop-terminal (app-info layout gate; absent probes x2 spaced 3.100136s; bundle present)
  scenario_6_second_vpn_conflict: pass - B-explicit-conflict-rejection (b_rejection_code=2203002; raw verbatim "VPN exist already, please execute destroy first")
  scenario_7_final_cleanup: blocked - machine-precondition-blocked step=1 reason=process-state-mismatch:cn.alfadb.netbird.e3physvpnb expected-active=False actual-active=True
scenario_aggregation:
  overall_rule: integrity invalid > scenario invalid > fail > blocked > pass
  overall: blocked
device_positive_signals_only:
  - S2: fd=33 open accepted (CREATE_ACCEPTED) with machine-verified Allow and onCreate
  - S3: strict process-boundary terminal with clean reactivation proof
  - S4: deny layout + full 60s window with no B create
  - S5: Settings app-info force-stop revoke terminal (bundle present, :vpn absent)
  - S6: B explicit conflict rejection with frozen code 2203002 while A verified active
device_positive_signals_not_e3_pass: true
platform_observation:
  finding: B's :vpn extension process survives a conflict rejection on this build
  evidence: two independent observations - S6 terminal checkpoint (observed_only=true, process_present=true) and S7 precondition probe (actual-active=True)
  interpretation: process lifecycle is owned by the startVpnExtensionAbility/stopVpnExtensionAbility pairing, independent of the create outcome; a rejected create leaves the started ability process alive until explicitly stopped
  product_implication: the future NetBird client should explicitly stop the extension ability after a failed create (or avoid keeping the ability started) to reap the process
raw_hilog_reference:
  location: controlled external RawRoot
transcript_reference:
  location: controlled external EvidenceRoot projection/transcript.redacted.jsonl
  sha256: f333ca3d02972114c0a1d09ee1bed4e411d48a27a318a9e47ddef3b963d819b1
  chain_head: ce67c18dcea2491b40d28c6d2636b38f4a017a482196394a3c79958782ef914e
  entries: 334
  projection_only: true
screenshot_reference: controlled external RawRoot; scenarios 1-7 captures collected
layout_state_reference: controlled external RawRoot; scenarios 1-7 layouts collected
fault_reference: controlled external RawRoot as produced by live runner; degraded=false
hash_manifest_reference:
  location: controlled external EvidenceRoot hash-manifest.json
  sha256: d697bc41a3ebd4a74bb30444fc4cc83f644831e80bfb3952f1776cd33ec09148
scenario_results_reference:
  location: controlled external EvidenceRoot scenario-results.json
  sha256: dfc51329fa52fc8038dc10f0f5b7d0ac9b0a4f3fad24eeb7561bad92f7cfaf3e
  runner_record_status: collected
  is_evidence: true
campaign_seal_reference:
  location: controlled external EvidenceRoot campaign-seal.json
  sha256: f24f20850f4588d128436ce3e7ab714852890c6cd47fd5b95e72ef6ccb7ba3d7
  binds: scenario-results,hash-manifest
  sealed_at: 2026-08-29T19:30:44.7641550+08:00
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
verdict: blocked
scope_statement: exact frozen PLA-AL10/API 26/arm64 full 1-7 attempt only; reviewed-pass/blocked is not E3 pass and does not authorize infrastructure retry, auto new ID, or device commands; no extrapolation to another build, device, API, architecture, Emulator, HarmonyOS release, OpenHarmony product, E4-E7, data plane, or product support
reviewers:
  - isolated-anthropic-claude-opus-5-reviewer
reviewed_at: 2026-08-29
review_findings:
  blocker: 0
  major: 0
  minor: 3
  minors_verbatim:
    - m1: accepted_session_count_in_window=2 counts marker occurrences (two CREATE_ACCEPTED markers on the same A requestId), not sessions; naming may mislead future readers
    - m2: campaign_started is not a top-level record field in this runner version (present only in the transcript runner-exception payload); not a regression
    - m3: the six-field seal contract belongs to the N0 runner; this E3 runner uses the C14 contract (schema_version/algorithm/record{path,sha256}/manifest{path,sha256}/sealed_at)
review_record: independent record review completed 2026-08-29 with 0 blocker / 0 major / 3 minor; record graded reviewed-pass
failure: blocked - scenario 7 blocked on S7 precondition process-state-mismatch (platform observation); overall blocked; not E3 pass; no auto retry; no new ID; no device command authorization
```

## 判定解释

`record_status: reviewed-pass` 只表示独立记录级审查已完成，且证据完整性、封签、哈希绑定、transcript 链、场景聚合与清理边界没有 blocker；它不表示 E3 pass。功能判定仍是 `verdict: blocked`，E3 未关闭，E8 继续 `CLOSED`。当前治理 `plan_status: consumed-blocked`。

逐场景：S1 cleanup/install `pass`；S2 机器验证 Allow → onCreate → create → fd（`fd=33 open=true` post-create 快照 + `VPN_CREATE_RESOLVED accepted=true`）`pass`；S3 严格进程边界 terminal（双 absent 探针间隔 3.100119s、bundle present、`s3_clean_reactivation_proof=true`）`pass`；S4 deny layout 预截图 + 全窗口无 B create `pass`；S5 Settings 应用信息机器门 + 强停后连续 absent 探针（3.100136s）+ bundle present `pass`；S6 B 显式冲突拒绝（`b_rejection_code=2203002`，raw 逐字 `VPN exist already, please execute destroy first`）`pass`；S7 步骤 1 前置检查发现 B 的 `:vpn` 进程实际 active（预期 inactive）→ `machine-precondition-blocked` `blocked`。overall `blocked`。

**这是本项目历次 E3-PHYS-PREFLIGHT live 首次 S1-S6 全部 machine-verified pass**。S7 的 blocked 属 fail-closed 的平台行为观察，不是功能失败：B 被冲突拒绝后其 `:vpn` 扩展进程仍存活（两处独立观察一致——S6 terminal checkpoint `observed_only=true process_present=true` 与 S7 前置实测）。进程生命周期由 `startVpnExtensionAbility`/`stopVpnExtensionAbility` 配对拥有、与 create 结果无关；产品层启示：客户端应在 create 失败后主动 stop 收回扩展进程。该观察的前瞻性修正（S7 前置只门控 A active、B 列为 observed-only）由 [`AUTH-E3-PHYS1API26-20260829-0001`](e3-physical-preflight-authorization-2026-08-29-0001.md) 登记。

`integrity_violations: []`，`capture_degraded: []`。finally 清理后 A/B bundle、A/B process、staging 均 absent，`cleanup_result.status: verified-clean`。

设备正面信号（仅限下列，**禁止**写成 E3 pass）：S2 fd33 open accepted；S3 进程边界 terminal + 干净再激活；S4 deny 全窗口无 B create；S5 强停撤销 terminal；S6 冻结冲突码 2203002 实测确认。

3 条 minor 逐字保留为审查结论，不降级也不改写 runner measured scenario 结果。

本次没有 `infrastructure_reason`，**不**授权 `infrastructure-blocked-retry-1`，**不**自动分配新 campaign/evidence ID，**不**授权任何 HDC/设备命令。继续执行须取得新的用户治理（已由 `AUTH-E3-PHYS1API26-20260829-0001` 登记）。

公开绑定 hash（均仓外 EvidenceRoot/freeze 对象；仓内只登记 hash）：

| 对象 | SHA-256 |
| --- | --- |
| scenario-results.json | `dfc51329fa52fc8038dc10f0f5b7d0ac9b0a4f3fad24eeb7561bad92f7cfaf3e` |
| hash-manifest.json | `d697bc41a3ebd4a74bb30444fc4cc83f644831e80bfb3952f1776cd33ec09148` |
| campaign-seal.json | `f24f20850f4588d128436ce3e7ab714852890c6cd47fd5b95e72ef6ccb7ba3d7` |
| transcript.redacted.jsonl | `f333ca3d02972114c0a1d09ee1bed4e411d48a27a318a9e47ddef3b963d819b1` |
| transcript chain_head | `ce67c18dcea2491b40d28c6d2636b38f4a017a482196394a3c79958782ef914e` |
| runner | `63436249dd24897d5b1f38f2718eb0bb8fd9d55b9fb74b6e10ce6c8ded8218ca` |
| final ready freeze | `8ff6b093f855f7bb85dbc6340530258a668b1eef47cf986cf3d0fc7d71b02517` |

gate 1-12 对象链（仓外，逐字节保全）：

| 门 | 对象 | SHA-256 |
| --- | --- | --- |
| gate 2 | audit-1 | `70de2862663215036f663e22aa3d7c4746cf45694a50cae2a06c9cdb3922ba1b` |
| gate 3 | blocked confirmation freeze | `9e89bd450fa8dff40e592c902f94a2e60a164de3212f4ee32f89446bbb71ebb8` |
| gate 5 | target-binding confirmation record | `cb7ac8de047c568c927c540ea4c6160f142617d916718801c5ebe2ffc6cc6a64` |
| gate 6 | ready freeze draft | `e77d1e1ec24ecf9cb185f0dbdf0473e1dcbd1984e4e54aa8177e202d370e7435` |
| gate 7 | e3-ready-freeze-review record | `86127915f20789bab00ff75b2f111355b44f9b10ca09b72ae2f1f37269ac051d` |
| gate 8 | final ready freeze | `8ff6b093f855f7bb85dbc6340530258a668b1eef47cf986cf3d0fc7d71b02517` |
| gate 9 | audit-2 | `a3c635085ba1209710c253ca4d12bf3023b5692f78e25a193d558dd33523d801` |

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- `E3-PHYS-PREFLIGHT-20260828-0001` / `EV-E3-PHYS1API26-20260828-0001` 状态：`consumed-blocked`，不可复用。
- `plan_status: consumed-blocked`；**无** auto retry、**无** 新 ID 授权、**无** 设备命令授权。
- 禁止把设备正面信号或 `reviewed-pass` 写成 E3 pass。
- 本登记期间禁止 HDC/设备命令。
