# E3-PHYS-PREFLIGHT API26 物理设备 live 预检证据（0002 / blocked）

最后核验：2026-08-07

本文登记 `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 的完整场景 1–7 live campaign（`ADJ-20260807-0002` protocol usability correction 重跑；非设备行为 retry；prior blocked 显式绑定 `EV-E3-PHYS1API26-20260807-0001`）。runner 形成 `record_status: collected`、`is_evidence: true`、`overall/verdict: blocked` 的正式证据根。独立双审查（`isolated kimi-coding/k3` 与 `isolated anthropic/claude-sonnet-5`）均 0 blocker / 5 major；计划 reviewer `anthropic/claude-opus-5` 超时，attempt-not-counted、不作 verdict。`record_status: reviewed-pass` 只表示审查完成且无 integrity blocker，**不是** E3 pass；功能判定 `verdict: blocked`。本 ID 已 `consumed-blocked`；`plan_status: blocked-awaiting-adjudication`。**禁止**自动 retry、自动分配新 ID、任何 HDC/设备命令授权。prior 0001 保留不改写。本记录不授权任何 HDC/设备命令，也不得写成 E3 pass。

```yaml
evidence_id: EV-E3-PHYS1API26-20260807-0002
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live
is_evidence: true
plan_status_at_live: ready
plan_status_after_registration: blocked-awaiting-adjudication
campaign_status: consumed-blocked
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  full_system_build: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  settings_manual_report: "7.0.0.100 (SP8C00E32R7P2patch09)"
  api: "26"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: 6.1.1.125 / API 24 / SystemCapability.Communication.NetManager.Vpn
  channel: ordinary-development-signing-only
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only
signing:
  type: ordinary-development
  device_in_profile: true
  public_certificate_file_sha256: c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc
code_sha: e8eb1b67a48603c55d3f55d2be686bae0dbd15e1
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  HDC: 3.2.0d
  HDC_executable_sha256: fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116
source_archive_sha256: e9aa2360df2027bfbd0a84f89a926439cf7bcfb50ddbb0c4977804373fb5da36
source_manifest_sha256: e5ca08160003aeb621220bf0666a7cc8f20ab2cef3241d692814798c758e1b50
sdk_sha256: f3ed4f374f1c877c14fdce99adf6f601595de4cc9d531bded7cc111fb14130b3
runner_sha256: 2fb2d3e99585a53adec82ea3b51ae2ea29c8f021d46e24b0828faa5415d38194
freeze_manifest_sha256: d6334c2d8d0d1bf11a2a9e26f65039ee0a1a98e377fbf644cef557ff02c55a1a
freeze_contract_sha256: 7a74c6696cb0811432bf8758c7b79f1b4b524bef23a6d7529aca5190c360ea16
preflight_inputs_frozen_at: 2026-08-07T11:23:47+08:00
artifact_sha256:
  hap_a: 3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244
  hap_b: 1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26
campaign_id: E3-PHYS-PREFLIGHT-20260807-0002
prior_blocked_binding: EV-E3-PHYS1API26-20260807-0001 # retained; consumed-blocked; no partial scenario5 replay; not infrastructure retry
attempt: initial
retry_basis: N/A - ADJ-20260807-0002 protocol usability correction full 1-7 rerun; not device-behavior or infrastructure retry
scenario_window_seconds: 60
settings_reallow_expected_path: direct-system-activation
settings_reallow_path_policy: observation-only
operator: authorized user
orchestrator: main agent
working_directory: repository-relative spikes/e3-vpn-extension-physical-preflight-hap
command: governed live runner invocation with controlled external inputs; sensitive arguments are not projected
input: frozen target tuple, ordinary-development signing, FINAL A/B HAPs, source/SDK maps, Chinese-prompt runner, cleanup/collection/review gates, and controlled external EvidenceRoot/RawRoot
expected: complete scenarios 1-7 under frozen inputs with truthful operator confirmations; destroy terminal and post-destroy fd snapshot where required; prior_blocked_binding projected
actual: scenarios 1-7 executed; overall blocked; S1 pass, S2 blocked, S3 blocked, S4 pass, S5 blocked, S6 blocked, S7 blocked; cleanup verified-clean; integrity_violations and capture_degraded empty; dual independent review 0 blocker / 5 major
started_at: 2026-08-07T11:32:09.5398219+08:00
ended_at: 2026-08-07T11:59:31.6503789+08:00
clock_source: runner host clock DateTimeOffset.Now; device HiLog CST frozen to +08:00
hdc_execution:
  process_count: 63
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
  scenario_1_cleanup_and_install: pass - cleanup-baseline-and-install
  scenario_2_allow_and_fd: blocked - allow-onCreate-create-fd; allow=pass; vpn_on_create=blocked; vpn_connection_create_fd=blocked
  scenario_3_active_stop: blocked - destroy-terminal-or-post-snapshot-missing
  scenario_4_deny: pass - deny-screenshot-and-ACK-plus-60-without-B-create
  scenario_5_settings_revoke: blocked - destroy-terminal-or-post-snapshot-missing; settings_revoke_captured=false
  scenario_6_second_vpn_conflict: blocked - B-rejected-no-replacement-destroy-required; no new S6 UI_START
  scenario_7_final_cleanup: blocked - requestId-missing (runner reason; see M4)
scenario_aggregation:
  overall_rule: any scenario fail => fail; else any scenario blocked => blocked; all seven scenarios pass => pass; evidence integrity violation => invalid
  overall: blocked
device_positive_signals_only:
  - S2: fd=32 open accepted (CREATE_ACCEPTED) but requestId association blocked (requestId=missing on VPN process cold start)
  - S4: deny 60s window with no B onCreate/create
  - S5: fd=33 accepted with direct-system-activation path match; system VPN page empty (title VPN / 没有VPN / 添加VPN网络); settings_revoke_captured=false
  - S7: Stop / VPN_ONDESTROY / pre-destroy snapshot events present for requestId cn.alfadb.netbird.e3physvpna-1786074139230-2
device_positive_signals_not_e3_pass: true
raw_visual_notes:
  scenario_5_settings: "title VPN / 没有VPN / 添加VPN网络; no registered VPN entry for revoke"
  scenario_6_conflict: "B UI same as S4 requestId Start pending; Start/Stop disabled; no new S6 UI_START event"
  scenario_7_post_cleanup: "Stop resolved; screenshot taken before finally uninstall"
raw_hilog_reference:
  location: controlled external RawRoot
  campaign_hilog_sha256: c0646f5eb3415879b5ce1e4283778fa3cf270f616e511bfbb8093a8ab2e7f7fe
transcript_reference:
  location: controlled external EvidenceRoot projection/transcript.redacted.jsonl
  sha256: 38c1985c7b787bc1faf745b97074ca9ec5877b71bd566d61a703723b20ffe3c3
  chain_head: 32152e5a7d554a417ceceaedf8191ad8c93af34f24cc8a9145428bb1bc176295
  projection_only: true
screenshot_reference: controlled external RawRoot; scenarios 1-7 captures collected
layout_state_reference: controlled external RawRoot; scenarios 1-7 layouts collected
fault_reference: controlled external RawRoot as produced by live runner; degraded=false
hash_manifest_reference:
  location: controlled external EvidenceRoot hash-manifest.json
  sha256: b7bf6974720632bdbbf6e208977352e52494d46a57470a52442a85afe0a87973
scenario_results_reference:
  location: controlled external EvidenceRoot scenario-results.json
  sha256: 923bf0cad50a693225fbcc2c682ba91f4acb441ee9eeaa1523c1d096bc5bcda1
  runner_record_status: collected
  is_evidence: true
campaign_seal_reference:
  location: controlled external EvidenceRoot campaign-seal.json
  sha256: 6e98679a5f3c5b430d5a8a1e3334e5a39ac86eb529f97bbffd5571c4dbbf406f
  binds: scenario-results,hash-manifest
  sealed_at: 2026-08-07T11:59:31.9110699+08:00
forbidden_capabilities_audit: no Go, NetBird, WireGuard, private fork, MANAGE_VPN, privileged bypass, automated device input, or non-whitelisted device query was used
cleanup_result:
  status: verified-clean
  verified_absent: true
  installed_a_remaining: false
  installed_b_remaining: false
  a_process_remaining: false
  b_process_remaining: false
  staging_remaining: false
  note: finally force-stop/uninstall A/B and remove staging; targeted probes verified A/B bundles and processes absent; staging absent
integrity_violations: []
capture_degraded: []
verdict: blocked
scope_statement: exact frozen PLA-AL10/API 26/arm64 full 1-7 attempt only; reviewed-pass/blocked is not E3 pass and does not authorize infrastructure retry, auto new ID, or device commands; prior 0001 retained; no extrapolation to another build, device, API, architecture, Emulator, HarmonyOS release, OpenHarmony product, E4-E7, data plane, or product support
reviewers:
  - isolated kimi-coding/k3
  - isolated anthropic/claude-sonnet-5
reviewer_attempt_not_counted: anthropic/claude-opus-5 timeout; not used as verdict
reviewed_at: 2026-08-07
review_findings:
  blocker: 0
  major: 5
  majors_verbatim:
    - M1: 冷启动 VPN 进程 requestId missing（S2 VPN_ONCREATE/CREATE 关联 blocked）
    - M2: S3/S7 无 destroy terminal / post-destroy snapshot
    - M3: S4 pending 延续使 S6 无新 B start（无新 S6 UI_START）
    - M4: S7 reason requestId-missing 与事件事实不符（事件含 requestId 与 Stop/ONDESTROY/pre-destroy）
    - M5: final record 未投影 prior_blocked_binding
review_record: independent dual review completed with 0 blocker and 5 major findings each; opus timeout attempt-not-counted
failure: blocked - scenarios 2/3/5/6/7 blocked; overall blocked; not E3 pass; no auto retry; no new ID; no device command authorization; awaiting adjudication
```

## 判定解释

`record_status: reviewed-pass` 只表示独立双审查已完成，且证据完整性、哈希绑定、场景聚合与清理边界没有 blocker；它不表示 E3 pass。功能判定仍是 `verdict: blocked`，E3 未关闭，E8 继续 `CLOSED`。当前治理 `plan_status: blocked-awaiting-adjudication`。

逐场景：S1 cleanup/install `pass`；S2 allow 可见但冷启动 VPN 进程 `requestId=missing`，`vpn_on_create`/`vpn_connection_create_fd` 关联 `blocked`（设备侧可见 fd=32 open accepted，**不得**升格为 scenario 或 E3 pass）；S3 active stop 缺 destroy terminal / post-destroy snapshot → `blocked`；S4 deny 截图 + ACK 后 60s 无 B create → `pass`；S5 direct-system-activation 匹配且 fd=33 accepted，但 Settings 页为“VPN / 没有VPN / 添加VPN网络”、`settings_revoke_captured=false`，且缺 destroy terminal / post snapshot → `blocked`；S6 B 仍停留在 S4 同 requestId 的 Start pending、Start/Stop disabled，无新 S6 `UI_START` → `blocked`；S7 事件含 Stop / ONDESTROY / pre-destroy，但 runner reason 记 `requestId-missing` 且缺 destroy terminal / post-destroy snapshot，post-cleanup 截图早于 finally 卸载 → `blocked`。overall `blocked`。

`integrity_violations: []`，`capture_degraded: []`。finally 清理后 A/B bundle、A/B process、staging 均 absent，`cleanup_result.status: verified-clean`。

设备正面信号（仅限下列，**禁止**写成 E3 pass）：S2 fd32 open accepted 但关联 blocked；S4 deny 60s no create；S5 fd33 accepted/direct activation 但系统 VPN 空且 revoke false；S7 Stop/ONDESTROY/pre-destroy 存在。

5 条 major 逐字保留为审查结论，不降为 minor，也不因 major 改写 runner measured scenario 结果。最终仓内登记显式投影 `prior_blocked_binding: EV-E3-PHYS1API26-20260807-0001`（补 runner 未投影的 M5 治理字段；不改写仓外 scenario-results JSON）。

本次没有 `infrastructure_reason`，**不**授权 `infrastructure-blocked-retry-1`，**不**自动分配新 campaign/evidence ID，**不**授权任何 HDC/设备命令。继续执行必须先取得新的路线裁决（adjudication）；在此之前 `plan_status` 保持 `blocked-awaiting-adjudication`。

公开绑定 hash（均仓外 EvidenceRoot/freeze 对象；仓内只登记 hash）：

| 对象 | SHA-256 |
| --- | --- |
| scenario-results.json | `923bf0cad50a693225fbcc2c682ba91f4acb441ee9eeaa1523c1d096bc5bcda1` |
| hash-manifest.json | `b7bf6974720632bdbbf6e208977352e52494d46a57470a52442a85afe0a87973` |
| campaign-seal.json | `6e98679a5f3c5b430d5a8a1e3334e5a39ac86eb529f97bbffd5571c4dbbf406f` |
| transcript.redacted.jsonl | `38c1985c7b787bc1faf745b97074ca9ec5877b71bd566d61a703723b20ffe3c3` |
| transcript chain_head | `32152e5a7d554a417ceceaedf8191ad8c93af34f24cc8a9145428bb1bc176295` |
| runner | `2fb2d3e99585a53adec82ea3b51ae2ea29c8f021d46e24b0828faa5415d38194` |
| freeze (e3-phys-preflight-freeze.json) | `d6334c2d8d0d1bf11a2a9e26f65039ee0a1a98e377fbf644cef557ff02c55a1a` |

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 状态：`consumed-blocked`，不可复用。
- prior `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 保留为 `consumed-blocked`，不改写。
- `plan_status: blocked-awaiting-adjudication`；**无** auto retry、**无** 新 ID 授权、**无** 设备命令授权。
- 禁止把设备正面信号或 `reviewed-pass` 写成 E3 pass。
- 本登记期间禁止 HDC/设备命令。
