# E3-PHYS-PREFLIGHT API26 物理设备 live 预检证据（operator-aborted/blocked）

最后核验：2026-08-07

本文登记 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 的 initial live campaign。operator 在场景 5 将 `SETTINGS-REVOKE-CAPTURED` 误确认为真，且未操作普通系统设置即直接关闭 campaign 窗口；runner 未执行 normal finally。随后形成 operator-aborted procedural seal，recovery cleanup 验证 `verified_absent=true`。独立审查 0 blocker / 0 major。`record_status: reviewed-pass` 只表示审查完成，**不是** E3 pass；`verdict: blocked`。旧 ID 已 `consumed-blocked`，禁止局部 scenario5 重放与 ID 复用。本记录不授权任何 HDC/设备命令。

```yaml
evidence_id: EV-E3-PHYS1API26-20260807-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live
is_evidence: true
plan_status_at_live: ready
seal_mode: operator-aborted-procedural
runner_normal_seal: false
runner_finally_executed: false
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
code_sha: 5ef532d099fcb3f4cd42fd8daab2864b6a6779a8
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  HDC: 3.2.0d
  HDC_executable_sha256: fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116
source_archive_sha256: e9aa2360df2027bfbd0a84f89a926439cf7bcfb50ddbb0c4977804373fb5da36
source_manifest_sha256: e5ca08160003aeb621220bf0666a7cc8f20ab2cef3241d692814798c758e1b50
sdk_sha256: f3ed4f374f1c877c14fdce99adf6f601595de4cc9d531bded7cc111fb14130b3
runner_sha256: 19fc1a76e49b9dca66a8a0352cc6bc8291f2888e66b3ad72cdc8a91ed97312e7
freeze_manifest_sha256: 7e83110218db4ac044d6882d264b2524cb12a2616585b3abbe70a1f4ff7c989a
freeze_contract_sha256: aebe4d461c0ebe54301e43196ab628781fd2f34930b0f0acfd6e5be103c207b0
preflight_inputs_frozen_at: 2026-08-07T10:05:23+08:00
artifact_sha256:
  hap_a: 3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244
  hap_b: 1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26
campaign_id: E3-PHYS-PREFLIGHT-20260807-0001
attempt: initial
retry_basis: N/A - initial attempt; not infrastructure retry
campaign_status: consumed-blocked
scenario_window_seconds: 60
settings_reallow_expected_path: direct-system-activation
settings_reallow_path_policy: observation-only
operator: authorized user
orchestrator: main agent
working_directory: repository-relative spikes/e3-vpn-extension-physical-preflight-hap
command: governed live runner invocation with controlled external inputs; sensitive arguments are not projected
input: frozen target tuple, ordinary-development signing, FINAL A/B HAPs, source/SDK maps, runner, cleanup/collection/review gates, and controlled external EvidenceRoot/RawRoot
expected: complete scenarios 1-7 under frozen inputs with truthful operator confirmations; runner normal seal and finally cleanup
actual: continuous capture and scenarios 1-5 raw observations exist; operator misconfirmed SETTINGS-REVOKE-CAPTURED=true while ordinary Settings was not operated and the campaign window was closed directly before runner finally; canonical runner aggregation incomplete; residual cleanup verified via recovery-cleanup; overall blocked
started_at: 2026-08-07T10:12:45.2378418+08:00
ended_at: 2026-08-07T10:39:05.7003138+08:00
clock_source: runner host clock DateTimeOffset.Now; device HiLog CST frozen to +08:00
hdc_execution:
  process_count: 43
  whitelist_only: true
  note: logical whitelist calls only; no Go/NetBird/WireGuard/MANAGE_VPN/automated device input/privileged bypass
install_and_runtime:
  continuous_capture_started: true
  staging_sent: true
  hap_a_installed: true
  hap_b_installed: true
  hap_a_started: true
  hap_b_started: true
  vpn_scenario_run: true-partial
scenarios:
  scenario_1_cleanup_and_install: blocked - observations-exist-but-canonical-runner-aggregation-incomplete
  scenario_2_allow_and_fd: blocked - observations-exist-but-canonical-runner-aggregation-incomplete
  scenario_3_active_stop: blocked - observations-exist-but-canonical-runner-aggregation-incomplete
  scenario_4_deny: blocked - observations-exist-but-canonical-runner-aggregation-incomplete
  scenario_5_settings_revoke: blocked - operator-misconfirmation-SETTINGS-REVOKE-CAPTURED
  scenario_6_second_vpn_conflict: blocked - not-run
  scenario_7_final_cleanup: blocked - not-run
scenario_aggregation:
  overall_rule: any scenario fail => fail; else any scenario blocked => blocked; all seven scenarios pass => pass; evidence integrity violation => invalid
  overall: blocked
operator_correction:
  type: operator-misconfirmation
  scenario: 5
  confirmation_name: SETTINGS-REVOKE-CAPTURED
  transcript_recorded_confirmed: true
  corrected_confirmed: false
  ordinary_settings_operated: false
  window_closed_directly: true
  partial_replay_authorized: false
  reference_sha256: 2d0c465fe08581233235eb9763ea80c8817a4ceb21abdf3a9ec3ae9d600d9029
raw_hilog_reference:
  location: controlled external RawRoot
  campaign_hilog_sha256: e620ef82d1819151b331234bbf80a3fd2ce7ca2c2da1abacf6620dabec44fc36
transcript_reference:
  location: controlled external EvidenceRoot projection/transcript.redacted.jsonl
  sha256: ba29a27518364c1635f4985ab6e210392a612a3c1a132cdd4dbda53b0c6de419
  chain_head: 7cf8d72be6ea1231854e906f1459d1f0ceed22050f477881654433693ce04795
  entry_count: 99
  projection_only: true
screenshot_reference: controlled external RawRoot; scenarios 1-5 captures collected; scenarios 6-7 not produced
layout_state_reference: controlled external RawRoot; scenarios 1-5 layouts collected; scenarios 6-7 not produced
fault_reference: controlled external RawRoot as produced by live runner; not rewritten by procedural seal
hash_manifest_reference:
  location: controlled external EvidenceRoot hash-manifest.json
  sha256: 1326e60b7ea14d3ee40e6580686af4a2c611b328448ccad28305c0e03d17d541
scenario_results_reference:
  location: controlled external EvidenceRoot scenario-results.json
  sha256: b29e97cd0643eb59447120d727dd4706a919d9cf9f71a5dad3e99c3d18b4b537
campaign_seal_reference:
  location: controlled external EvidenceRoot campaign-seal.json
  sha256: 720a119d39d2028baef956b9cccceefea6bd68414144257a7574d3a6992d98ad
  seal_mode: operator-aborted-procedural
  binds: scenario-results,hash-manifest,operator-correction,recovery-cleanup,transcript-chain-tip
  sealed_at: 2026-08-07T10:45:00+08:00
recovery_cleanup_reference:
  location: controlled external EvidenceRoot recovery-cleanup.json
  sha256: 6fecd3e835a8fc44956d5cf2bce461b044b9426bdcccd923d3ec9ac59e1d77c6
  verified_absent: true
forbidden_capabilities_audit: no Go, NetBird, WireGuard, private fork, MANAGE_VPN, privileged bypass, automated device input, or non-whitelisted device query was used
cleanup_result:
  status: verified-clean
  verified_absent: true
  source: recovery-cleanup.json
  runner_finally_executed: false
  operator_closed_window_before_finally: true
  installed_a_remaining: false
  installed_b_remaining: false
  a_process_remaining: false
  b_process_remaining: false
  staging_remaining: false
  note: residual recovery cleanup force-stop/uninstall A/B and remove staging; targeted probes verified absent
integrity_violations: []
verdict: blocked
scope_statement: exact frozen PLA-AL10/API 26/arm64 initial attempt only; operator-aborted/blocked is not E3 pass and does not authorize infrastructure retry or partial scenario-5 replay; no extrapolation to another build, device, API, architecture, Emulator, HarmonyOS release, OpenHarmony product, E4-E7, data plane, or product support
reviewer: isolated anthropic/claude-sonnet-5
reviewed_at: 2026-08-07
review_findings:
  blocker: 0
  major: 0
review_record: independent operator-aborted seal review completed with 0 blocker and 0 major findings
failure: operator-aborted/blocked - authorized-user corrected scenario-5 SETTINGS-REVOKE-CAPTURED=true as misconfirmation; ordinary Settings not operated; window closed directly before runner finally; no E3 pass; restart and scenario-5 partial replay forbidden
```

## 判定解释

`record_status: reviewed-pass` 只表示独立审查已完成，且 operator-aborted seal、哈希绑定、operator correction、recovery cleanup 与范围陈述没有 blocker 或 major 发现；它不表示 E3 pass。功能判定仍是 `verdict: blocked`，E3 未关闭，E8 继续 `CLOSED`。

场景 5 的 transcript 曾记录 `SETTINGS-REVOKE-CAPTURED=true`，但授权用户于同日显式更正：该确认为误确认；普通系统设置（齿轮 → 更多连接 → VPN）未被操作；campaign 窗口被直接关闭。因此场景 5 记 `blocked`（`operator-misconfirmation-SETTINGS-REVOKE-CAPTURED`）；场景 1–4 虽有 raw/transcript 观测，但 canonical runner 聚合未完成，统一记 `blocked`；场景 6–7 未运行。overall `blocked`。

recovery cleanup 在窗口关闭后执行 force-stop/uninstall A/B 与 staging 移除，定向探针确认 bundle/process/staging 均 absent，`verified_absent=true`。procedural seal 不伪造 runner normal seal，也不回写既有 lock/transcript/raw/recovery 产物。

本次没有 `infrastructure_reason`。operator 误确认与直接关窗不属于 HDC/USB 中断、采集存储故障或 runner/宿主故障，因此 **不** 授权 `infrastructure-blocked-retry-1`，也 **禁止** 从场景 5 局部重放。任何继续执行必须先取得新的路线决策（见 `ADJ-20260807-0002`），分配新的 campaign/evidence ID，并完整重跑场景 1–7；不得复用本 campaign、attempt 或 evidence ID。

公开绑定 hash（均仓外 EvidenceRoot 对象；仓内只登记 hash）：

| 对象 | SHA-256 |
| --- | --- |
| scenario-results.json | `b29e97cd0643eb59447120d727dd4706a919d9cf9f71a5dad3e99c3d18b4b537` |
| hash-manifest.json | `1326e60b7ea14d3ee40e6580686af4a2c611b328448ccad28305c0e03d17d541` |
| campaign-seal.json | `720a119d39d2028baef956b9cccceefea6bd68414144257a7574d3a6992d98ad` |
| operator-correction.json | `2d0c465fe08581233235eb9763ea80c8817a4ceb21abdf3a9ec3ae9d600d9029` |
| recovery-cleanup.json | `6fecd3e835a8fc44956d5cf2bce461b044b9426bdcccd923d3ec9ac59e1d77c6` |
| transcript chain_head | `7cf8d72be6ea1231854e906f1459d1f0ceed22050f477881654433693ce04795` |
| transcript.redacted.jsonl | `ba29a27518364c1635f4985ab6e210392a612a3c1a132cdd4dbda53b0c6de419` |

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 状态：`consumed-blocked`，不可复用。
- 禁止局部 scenario5 重放；禁止把本 blocked 写成设备行为 retry 依据。
- 后续完整中文人工提示重跑与新 ID 见 `ADJ-20260807-0002`（`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002`）；原因是 protocol usability correction，不是设备行为 retry。
- 本登记期间禁止 HDC/设备命令。
