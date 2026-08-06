# E3-PHYS-PREFLIGHT 物理设备 live 预检证据

最后核验：2026-08-06

本文登记唯一 `E3-PHYS-PREFLIGHT` 的 initial live preflight。runner 在连续采集与安装前复核冻结目标时，live model 与冻结值一致，但 live build 的脱敏投影与冻结 build 的可见 suffix 不一致，因此按预定输入漂移停止。该记录只覆盖这一次精确输入与停止边界，不形成 E3、E8、产品或平台通过结论。

```yaml
evidence_id: EV-E3-PHYS1API23-20260806-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live
is_evidence: true
plan_status: blocked
target_tuple:
  distribution: HarmonyOS
  device_model: PLA-AL10
  device_alias: PHYS-1
  frozen_full_system_build: PLA-AL10 6.1.0.117(SP6C00E115R7P7)
  live_model_projection: PLA-AL10
  live_build_projection: "PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)"
  api: "23"
  kernel_architecture: aarch64
  app_abi: arm64-v8a
  sdk_api_syscap: HarmonyOS SDK 6.1.1.125 / compile API 24; target and compatible API 23; public VPN Extension API only
  channel: ordinary-development-signing-only
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only
signing:
  type: ordinary-development
  device_in_profile: true
  public_certificate_file_sha256: c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc
code_sha: 82ebc400de89a9de691a8c9d1bd629c9845999e8
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  DevEco_Studio: 6.1.1.290 / Build 243.24978.46.36.611290
  HarmonyOS_SDK: 6.1.1.125 / API 24
  target_compatible_API: 23 / 23
  HDC: 3.2.0d
  HDC_executable_sha256: fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116
source_archive_sha256: e9aa2360df2027bfbd0a84f89a926439cf7bcfb50ddbb0c4977804373fb5da36
source_manifest_sha256: e5ca08160003aeb621220bf0666a7cc8f20ab2cef3241d692814798c758e1b50
sdk_sha256: f3ed4f374f1c877c14fdce99adf6f601595de4cc9d531bded7cc111fb14130b3
runner_sha256: 749be7f8dd7c561f0728e90220fa703f12ccc33e7eb7a22e30af482511e4a770
freeze_manifest_sha256: bba10ec13e43f3d351531079fa1371057c68f0a6a9bb42bd6ba04afac437a7fe
preflight_inputs_frozen_at: exact ISO-8601 value retained in controlled external EvidenceRoot freeze manifest
artifact_sha256:
  hap_a: 3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244
  hap_b: 1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26
profile_sha256:
  profile_a: a3abfc6ac351cf06f5639b31f108c80edcdcd96080f43ccfd48ce12a07325b05
  profile_b: f09af0f314773c53d61d90804332605317ec6a61316add0df3672067da99a16e
campaign_id: E3-PHYS-PREFLIGHT-20260806-0001
attempt: initial
retry_basis: N/A - initial attempt
campaign_started: false
scenario_window_seconds: 60
settings_reallow_expected_path: direct-system-activation
settings_reallow_path_policy: observation-only
operator: authorized user
orchestrator: main agent
working_directory: repository-relative spikes/e3-vpn-extension-physical-preflight-hap
command: governed live runner invocation with controlled external inputs; sensitive arguments are not projected
input: frozen target tuple, ordinary-development signing, FINAL A/B HAPs, source/SDK maps, runner, cleanup/collection/review gates, and controlled external EvidenceRoot/RawRoot
expected: live model and complete build must match the frozen tuple before continuous capture, staging, installation, or any scenario begins
actual: live model matched PLA-AL10; live build projection was "PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)" and its visible suffix differed from frozen "PLA-AL10 6.1.0.117(SP6C00E115R7P7)"; runner stopped at target-binding preflight
started_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
ended_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
clock_source: runner host clock; exact timestamps retained in controlled external EvidenceRoot
hdc_execution:
  process_count: 8
  whitelist_only: true
  calls:
    - host version check
    - exact model target-binding check
    - exact build target-binding check
    - finally targeted A bundle probe
    - finally targeted B bundle probe
    - finally targeted A PID probe
    - finally targeted B PID probe
    - finally fixed staging-path probe
install_and_runtime:
  continuous_capture_started: false
  staging_sent: false
  hap_a_installed: false
  hap_b_installed: false
  hap_a_started: false
  hap_b_started: false
  vpn_scenario_run: false
scenarios:
  scenario_1_cleanup_and_install: blocked - not entered because target-binding preflight stopped before continuous capture and installation
  scenario_2_allow_and_fd: blocked - not run
  scenario_3_active_stop: blocked - not run
  scenario_4_deny: blocked - not run
  scenario_5_settings_revoke: blocked - not run
  scenario_6_second_vpn_conflict: blocked - not run
  scenario_7_final_cleanup: blocked - not run; finally cleanup verification ran independently
scenario_aggregation:
  overall_rule: pre-scenario frozen-input drift stops the initial attempt with overall blocked; unrun scenarios cannot be promoted to pass or fail
  overall: blocked
raw_hilog_reference: not produced by the preplanned preflight stop before continuous capture; controlled external RawRoot contains no campaign HiLog artifact
transcript_reference:
  location: controlled external EvidenceRoot
  sha256: 2e49df8dda0a7c4ec217d4d4b38624f912a0ac4d9da89aa826c734c3331f9960
  projection_only: true
screenshot_reference: not produced because no scenario or UI action began; this is the preplanned preflight stop boundary
layout_state_reference: not produced because no scenario or UI action began; this is the preplanned preflight stop boundary
fault_reference: not produced because no A/B install or runtime began; this is the preplanned preflight stop boundary
hash_manifest_reference:
  location: controlled external EvidenceRoot
  sha256: a89d716555520d270b9e69cfc1926e162db8689ff756b25c873f6d6457aae21f
scenario_results_reference:
  location: controlled external EvidenceRoot
  sha256: 409568c8db4655a46f53108cbeafbfa03e5c094ddae139b1428bd9f21f8e11af
campaign_seal_reference:
  location: controlled external EvidenceRoot
  sha256: d95dcf04183318c4ca94e82932b1d74bd7778e6003e21ffd4e87a788ef32bce9
forbidden_capabilities_audit: no Go, NetBird, WireGuard, private fork, MANAGE_VPN, privileged bypass, automated device input, or non-whitelisted device query was used
cleanup_result:
  status: verified-clean
  verified_absent: true
  installed_a_remaining: false
  installed_b_remaining: false
  a_process_remaining: false
  b_process_remaining: false
  staging_remaining: false
  note: A/B were never installed or run; finally used only targeted A/B bundle, PID, and fixed staging-path probes
integrity_violations: []
verdict: blocked
scope_statement: exact frozen PLA-AL10/API 23/arm64 initial attempt only; no extrapolation to another build, device, API, architecture, Emulator, HarmonyOS release, OpenHarmony product, E4-E7, data plane, or product support
reviewer: isolated deepseek/deepseek-v4-pro
reviewed_at: 2026-08-06T19:21:43+08:00
review_findings:
  blocker: 0
  major: 0
review_record: independent evidence review completed with 0 blocker and 0 major findings
```

## 判定解释

`record_status: reviewed-pass` 只表示独立审查已完成，且证据完整性、停止边界、哈希、清理和范围陈述没有 blocker 或 major 发现；它不表示 E3 pass。功能判定仍是 `verdict: blocked`，E3 未关闭，E8 继续 `CLOSED`。

冻结 build 是 `PLA-AL10 6.1.0.117(SP6C00E115R7P7)`。live model 投影仍为 `PLA-AL10`，但 live build 只投影为 `PLA-AL10 <REDACTED_IPV4>(SP8C00E32R7P2)`。本记录仅依据两个可见 suffix 不同确认 build drift；不猜测 `<REDACTED_IPV4>` 所代表的完整版本，不声称或推断 OTA、人工升级或任何其他漂移原因。

runner 在连续 HiLog capture、staging 与 install 前按冻结输入门停止，因此 `campaign_started=false`，A/B 未安装、未启动，七个功能场景均未运行。raw HiLog、截图、layout/state 与定向 fault 未产生，是预定 preflight stop 的正常结果，不是材料缺失、删除或篡改。脱敏 transcript、manifest、scenario results 与 seal 已形成并通过完整性审查，`integrity_violations` 为空；finally 的定向清理探针确认 `verified-clean`。

本次没有 `infrastructure_reason`。build drift 不属于 HDC/USB 中断、采集存储故障或 runner/宿主故障，因此 initial 已消费，`infrastructure-blocked-retry-1` 不获授权。任何继续执行都必须先取得新的路线决策，冻结完整的新 build，并分配新的 campaign ID 与 evidence ID；不得复用本 campaign、attempt 或 evidence ID。

## 门状态

- E3 未关闭；本记录不是 `reviewed-pass/pass`。
- E8 保持 `CLOSED`，不授权任何预检范围外的物理设备工作。
- R0 checklist 中预检输入冻结仍为完成项；“执行 campaign 并取得 `reviewed-pass/pass`”仍未完成。
- 本结果不外推到其他 build、设备、API、架构或平台，也不改写既有 Emulator 记录。
