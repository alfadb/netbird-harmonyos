# E3-PHYS-PREFLIGHT HarmonyOS 7 最小只读元组重绑定证据

最后核验：2026-08-06

本文登记 `ADJ-20260806-0002` 授权的一次 HarmonyOS 7 最小只读元组重绑定 discovery。它不是 campaign，不分配 campaign ID，不安装/启动 A/B，不运行 VPN 场景。`record_status: reviewed-pass` 与 `verdict: pass` **严格只表示** ADJ-0002 授权的三条只读 rebind 已完成且独立审查为 0 blocker/0 major；**不是** E3 pass，也不是 `E3-PHYS-PREFLIGHT` campaign 通过。

```yaml
evidence_id: EV-E3-PHYS1REBIND7-20260806-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live-readonly-rebind
is_evidence: true
is_campaign: false
campaign_id: N/A - ADJ-20260806-0002 discovery only; no campaign ID allocated
attempt: N/A - not a campaign attempt
plan_status_at_record: blocked
authorization: ADJ-20260806-0002
device_alias: PHYS-1
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only
target_projection:
  distribution: HarmonyOS
  device_model: PLA-AL10
  settings_manual_report: "7.0.0.100 (SP8C00E32R7P2patch09)"
  hdc_build_candidate: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  hdc_build_candidate_basis: prior live build projection visible suffix (SP8C00E32R7P2) plus Settings manual report 7.0.0.100; not a frozen campaign input in this record
  api: "26"
  kernel_arch: aarch64
  app_abi: arm64-v8a
code_sha: 64ef6afe9a283d58250a77f0424b4d4dc0d2f80c
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
operator: authorized user
orchestrator: main agent
working_directory: N/A - no repository runner; three authorized readonly shell probes only
command: three ADJ-20260806-0002 whitelist readonly shell probes via controlled external PHYS_1_TARGET; sensitive target and raw stdout are not projected
input: ADJ-20260806-0002 authorization; prior EV-E3-PHYS1API23-20260806-0001 live model/build projections; Settings manual report
expected: obtain only const.ohos.apiversion, uname -m, and const.product.cpu.abilist; no dist/model/build re-query; no serial/UDID/app list/install/start/VPN
actual: three probes returned projected values api=26, kernel_arch=aarch64, app_abi=arm64-v8a; no campaign action occurred
started_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
ended_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
clock_source: host clock of the controlled external capture; exact timestamps retained in controlled external EvidenceRoot
hdc_execution:
  process_count: 3
  whitelist_only: true
  calls:
    - param get const.ohos.apiversion
    - uname -m
    - param get const.product.cpu.abilist
  projection_calls_sha256: b08ad053cae2f2378c27014662f272819826b7be8042f56d0271c089cf661160
  projection_calls_hash_basis: SHA-256 of the three projected call strings joined by LF with a trailing newline; no host path, target, or raw stdout included
rebind_results_projection:
  const.ohos.apiversion: "26"
  uname_m: aarch64
  const.product.cpu.abilist: arm64-v8a
results_reference:
  location: controlled external EvidenceRoot
  sha256: f750f1138fd74c31a481999058e6b6b8860ece74d12152a8558c614e8f7e8489
  projection_only: true
hash_manifest_reference:
  location: controlled external EvidenceRoot
  sha256: d24e2e22b4763f4cd1be27a184749fe11600eb54e64c13ec3c25dbf3b0e9263e
rebind_seal_reference:
  location: controlled external EvidenceRoot
  sha256: 4ead491cc37791780a39bc1ceb4f2a9ad60477b8df16f48d6d58dbd2406ba7fe
raw_reference: controlled external RawRoot only; repository paths abstracted; raw stdout not stored in-repo
forbidden_capabilities_audit: no dist/model/build re-query; no serial/UDID; no app list; no install/start/stop; no VPN; no continuous capture; no campaign runner; no Go/NetBird/WireGuard/private fork/MANAGE_VPN/privileged bypass
install_and_runtime:
  continuous_capture_started: false
  staging_sent: false
  hap_a_installed: false
  hap_b_installed: false
  hap_a_started: false
  hap_b_started: false
  vpn_scenario_run: false
cleanup_result:
  status: N/A - no install, start, staging, or campaign cleanup surface was entered
integrity_violations: []
verdict: pass
verdict_scope: ADJ-20260806-0002 three readonly rebind probes only; not E3 pass; not campaign pass; not product or platform support
scope_statement: exact PHYS-1 HarmonyOS 7 readonly rebind discovery only; no extrapolation to E3 campaign success, HAP installability, Emulator, other devices, other builds, OpenHarmony products, E4-E7, data plane, or product support
reviewer: isolated kimi-coding/k3
reviewed_at: 2026-08-06T19:49:49+08:00
review_findings:
  blocker: 0
  major: 0
review_record: independent evidence review completed with 0 blocker and 0 major findings
```

## 判定解释

`record_status: reviewed-pass` 与 `verdict: pass` 只覆盖 ADJ-0002 授权的三条只读 rebind：`const.ohos.apiversion` → `26`，`uname -m` → `aarch64`，`const.product.cpu.abilist` → `arm64-v8a`。它不表示 E3 关闭，不表示 `E3-PHYS-PREFLIGHT` campaign 通过，也不自动开放 E8。

HDC build 候选 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` 仍只依据旧 live 脱敏投影可见 suffix 与 Settings 人工值 `7.0.0.100 (SP8C00E32R7P2patch09)` 合成；本 discovery **未**重查 dist/model/build。Settings 中的 `patch09` 后缀属于人工 UI 报告，不升格为 HDC binding 字符串的一部分。候选在本记录形成时尚未因 rebind 本身自动冻结；冻结与后续路线由 `ADJ-20260806-0003` 另行记录。

raw 输出仅仓外受控保存；仓内只登记脱敏 projection 与 hash。`projection_calls_sha256` 为三条投影命令文本的现场 SHA-256，不含 target、主机路径或 raw stdout。`results` / `manifest` / `seal` hash 绑定仓外 EvidenceRoot 对应对象。

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- 旧 initial live `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）历史判定不改写，旧 campaign/evidence ID 仍不可复用。
- 本 rebind 完成后，HAP 复用/重建与新 campaign 路线由 `ADJ-20260806-0003` 决定；设备执行仍须再次确认，且 host 侧须完成新 HAP 复用 reverify 后才可进入 `plan_status: ready`。

## Subsequent note（非本记录 measured 值；不改写上方 YAML）

- `ADJ-20260806-0003` 随后冻结 HarmonyOS 7 / API 26 元组，并分配准备用 campaign/evidence `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（历史原始分配；该组 ID 从未 Live、未占用）。
- host reverify 随后 PASS：public manifest SHA-256 `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；FINAL HAP / signature / profile / member-list hashes 与历史登记一致、未变；**不**主张设备安装兼容性。
- `ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 随后以单条 `software.version` 实测确认 HDC build 与冻结 binding 逐字匹配（`reviewed-pass/pass`，仅 build-confirm）；API `26`/`aarch64`/`arm64-v8a` 仍以本 rebind 实测为准，不从 build 推断。
- `ADJ-20260807-0001` 随后将未执行 API26 campaign 跨日重新编号：历史准备 ID `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` 标 `superseded-unexecuted`；当前准备身份为 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`（tuple/HAP/runner/rules 未变）。
- 当前治理 `plan_status: ready` 只表示新编号 campaign host 输入 ready；最终 commit-bound freeze 须基于含本登记的提交重生；设备执行仍须明确再确认；E8 保持 `CLOSED`。
- 本后续注记不修改上方 rebind measured 字段、hash 或 `verdict: pass` 的范围（仍仅三条只读 rebind）。
