# E3-PHYS-PREFLIGHT HarmonyOS 7 单条 software.version build 确认证据

最后核验：2026-08-06

本文登记 `ADJ-20260806-0004` 授权的一次用户确认只读 `const.product.software.version` build 确认。它不是 campaign，不分配 campaign ID，不安装/启动 A/B，不运行 VPN 场景，不重查 dist/model/API/arch/ABI。`record_status: reviewed-pass` 与 `verdict: pass` **严格只表示**该单条 build 确认已完成且独立审查为 0 blocker/0 major；**不是** E3 pass，也不是 `E3-PHYS-PREFLIGHT` campaign 通过。

```yaml
evidence_id: EV-E3-PHYS1BUILD7-20260806-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live-readonly-build-confirm
is_evidence: true
is_campaign: false
campaign_id: N/A - ADJ-20260806-0004 build-confirm only; no campaign ID allocated
attempt: N/A - not a campaign attempt
plan_status_at_record: ready
authorization: ADJ-20260806-0004
device_alias: PHYS-1
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only
target_projection:
  distribution: HarmonyOS
  device_model: PLA-AL10
  settings_manual_report: "7.0.0.100 (SP8C00E32R7P2patch09)"
  hdc_build_confirmed: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
  hdc_build_confirm_basis: single authorized param get const.product.software.version exact projected stdout; not inferred from Settings UI or prior synthetic candidate
  api: "26"
  api_basis: prior EV-E3-PHYS1REBIND7-20260806-0001 measured const.ohos.apiversion; not inferred from this build string
  kernel_arch: aarch64
  kernel_arch_basis: prior EV-E3-PHYS1REBIND7-20260806-0001 measured uname -m; not inferred from this build string
  app_abi: arm64-v8a
  app_abi_basis: prior EV-E3-PHYS1REBIND7-20260806-0001 measured const.product.cpu.abilist; not inferred from this build string
code_sha: 9788b8ba2a313a714354e8b8bf5a8142530633af
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
operator: authorized user
orchestrator: main agent
working_directory: N/A - no repository runner; one authorized readonly shell probe only
command: one ADJ-20260806-0004 whitelist readonly shell probe via controlled external PHYS_1_TARGET; sensitive target and raw stdout are not projected
input: ADJ-20260806-0004 authorization; ADJ-20260806-0003 frozen HDC binding candidate; prior EV-E3-PHYS1REBIND7-20260806-0001 measured API/arch/ABI
expected: obtain only const.product.software.version exact string; no dist/model/API/arch/ABI re-query; no serial/UDID/app list/install/start/VPN; no other ops
actual: one probe returned exact projected value PLA-AL10 7.0.0.100(SP8C00E32R7P2); verbatim match to ADJ-20260806-0003 frozen HDC binding; no campaign action occurred
started_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
ended_at: exact timestamp retained in controlled external EvidenceRoot; repository record is date-bounded to 2026-08-06
clock_source: host clock of the controlled external capture; exact timestamps retained in controlled external EvidenceRoot
hdc_execution:
  process_count: 1
  whitelist_only: true
  calls:
    - param get const.product.software.version
  projection_calls_sha256: 60783947cc12b369b660559b4711008a87f8a46636bb0092738e9af933647473
  projection_calls_hash_basis: SHA-256 of the single projected call string with a trailing newline; no host path, target, or raw stdout included
build_confirm_results_projection:
  const.product.software.version: PLA-AL10 7.0.0.100(SP8C00E32R7P2)
results_reference:
  location: controlled external EvidenceRoot
  sha256: fa2c63a5729c7b7e4258024b7fb9a8d5f999dc5f03c8a977308c2f3b7c3f842e
  projection_only: true
hash_manifest_reference:
  location: controlled external EvidenceRoot
  sha256: 777b123822204d66e17e6d4467cf8a276ee5fd37b3ec720134c2b3c82324465a
build_confirm_seal_reference:
  location: controlled external EvidenceRoot
  sha256: ecbf6af5a8cadf556cf37f63189a79adbc2576ee56da4359dfeb8f4fb2fcf2da
hdc_executable_sha256: fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116
raw_reference: controlled external RawRoot only; repository paths abstracted; raw stdout not stored in-repo
forbidden_capabilities_audit: no dist/model/API/arch/ABI re-query; no serial/UDID; no app list; no install/start/stop; no VPN; no continuous capture; no campaign runner; no Go/NetBird/WireGuard/private fork/MANAGE_VPN/privileged bypass; exactly one HDC call; no other ops
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
verdict_scope: ADJ-20260806-0004 single readonly software.version build-confirm only; not E3 pass; not campaign pass; not product or platform support
scope_statement: exact PHYS-1 HarmonyOS 7 single software.version build confirmation only; no extrapolation to E3 campaign success, HAP installability, Emulator, other devices, other builds, OpenHarmony products, E4-E7, data plane, or product support; API 26 / aarch64 / arm64-v8a remain prior rebind measurements and are not inferred from this build string
reviewer: isolated kimi-coding/k3
reviewed_at: 2026-08-06T20:46:00+08:00
review_findings:
  blocker: 0
  major: 0
review_record: independent evidence review completed with 0 blocker and 0 major findings
```

## 判定解释

`record_status: reviewed-pass` 与 `verdict: pass` 只覆盖 `ADJ-20260806-0004` 授权的单条只读 build 确认：`const.product.software.version` → 精确投影 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`，与 `ADJ-20260806-0003` 冻结的 HDC binding 逐字匹配。它消除了此前依据旧 live 可见 suffix + Settings 人工值合成 HDC build 候选的残余风险。它不表示 E3 关闭，不表示 `E3-PHYS-PREFLIGHT` campaign 通过，也不自动开放 E8。

API `26`、kernel arch `aarch64`、app ABI `arm64-v8a` **不是**从本 build 字符串推断；它们继续绑定前一 rebind 证据 `EV-E3-PHYS1REBIND7-20260806-0001` 的实测结果。Settings 人工报告 `7.0.0.100 (SP8C00E32R7P2patch09)` 仍只作人工 UI 补充；`patch09` 不并入 HDC binding 字符串。

raw 输出仅仓外受控保存；仓内只登记脱敏 projection 与 hash。`projection_calls_sha256` 为单条投影命令文本的现场 SHA-256，不含 target、主机路径或 raw stdout。`results` / `manifest` / `seal` hash 绑定仓外 EvidenceRoot 对应对象。HDC 可执行文件 SHA-256 为 `fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116`。本记录登记时 `code_sha` 为 `9788b8ba2a313a714354e8b8bf5a8142530633af`。

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- 旧 initial live `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）历史判定不改写，旧 campaign/evidence ID 仍不可复用。
- rebind `EV-E3-PHYS1REBIND7-20260806-0001` 历史 measured 字段不改写。
- 本 build-confirm 记录形成时不新增 campaign、不改变当时准备 ID `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（历史原始边界；该组 ID 从未 Live、未占用）。
- 本记录形成时治理 `plan_status: ready` 只表示当时新 campaign host 输入 ready；后续状态演变见 Subsequent note。

## Subsequent note（非本记录 measured 值；不改写上方 YAML）

- `ADJ-20260807-0001` 随后将未执行 API26 campaign 跨日重新编号：历史准备 ID `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` 标 `superseded-unexecuted`；当时准备身份为 `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001`（tuple/HAP/runner/rules 未变）。该 0001 身份随后 Live 并 `consumed-blocked`（保留）；`ADJ-20260807-0002` 的 `E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 随后 Live 并 `consumed-blocked`；当前 `plan_status: blocked-awaiting-adjudication`。
- 本后续注记不修改上方 build-confirm measured 字段、hash 或 `verdict: pass` 的范围（仍仅单条 software.version）。
