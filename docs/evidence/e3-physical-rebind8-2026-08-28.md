# E3-PHYS-PREFLIGHT HarmonyOS 7 最小只读元组重绑定证据（2026-08-28）

最后核验：2026-08-28

本文登记用户（直接人类决策者）于 2026-08-28 显式授权的一次 HarmonyOS 7 最小只读元组重绑定 discovery。它不是 campaign，不分配 campaign ID，不安装/启动 A/B，不运行 VPN 场景。`verdict: pass` **严格只表示**本次授权的四条只读 rebind 已完成；**不是** E3 pass，也不是 `E3-PHYS-PREFLIGHT` campaign 通过，不主张安装兼容性，不开放 E8。

背景：`AUTH-E3-PHYS1API26-20260825-0001` 的 gate 5 `TargetBindingConfirm` 于 2026-08-28T18:30:56+08:00 实测 full system build 与冻结元组漂移（冻结 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)` vs 实测 `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`，设备在 2026-08-17 与 2026-08-28 之间收到系统 OTA），产生 blocked record（`preflight: frozen full system build mismatch`）并退役该 pair。用户随后授权本 rebind 以实测确认新元组四项，供新 AUTH/pair 冻结。

```yaml
evidence_id: EV-E3-PHYS1REBIND8-20260828-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: live-readonly-rebind
is_evidence: true
is_campaign: false
campaign_id: N/A - user-authorized readonly rebind only; no campaign ID allocated
attempt: N/A - not a campaign attempt
plan_status_at_record: blocked
authorization: user direct decision 2026-08-28 (rebind new tuple after gate-5 build drift)
device_alias: PHYS-1
hdc_target_reference: controlled external mapping; repository alias PHYS-1 only; target token not projected
target_projection:
  distribution: HarmonyOS
  device_model: PLA-AL10
  full_system_build: PLA-AL10 7.0.0.102(SP8C00E102R7P3)
  api: "26"
  kernel_arch: aarch64
  app_abi: arm64-v8a
  tuple_basis: four readonly probes measured 2026-08-28; dist/model carried from prior measured records
code_sha: 46fb2213beb0df45169a0af54908a54598f2def1
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
operator: authorized user
orchestrator: main agent (user-authorized execution)
working_directory: N/A - no repository runner; four authorized readonly shell probes only
command: four user-authorized readonly shell probes via controlled external single-connection target; sensitive target and raw stdout are not projected
input: user direct decision 2026-08-28; prior gate-5 blocked confirmation record (build drift); frozen HDC binary at frozen path
expected: obtain const.product.software.version, const.ohos.apiversion, uname -m, and const.product.cpu.abilist; no serial/UDID/app list/install/start/VPN
actual: four probes returned projected values software.version=PLA-AL10 7.0.0.102(SP8C00E102R7P3), apiversion=26, uname_m=aarch64, abilist=arm64-v8a; no campaign action occurred
started_at: 2026-08-28T18:39:00+08:00
ended_at: 2026-08-28T18:39:01+08:00
clock_source: host clock of the controlled external capture
hdc_execution:
  process_count: 4
  whitelist_only: true
  hdc_binary_sha256: 03123a78c6dc02870f52c416a1154dc9ad7165e6067c9cdf9e134e0578182c81
  hdc_binary_path: frozen path (HarmonyOS command-line-tools current SDK openharmony toolchains hdc)
  calls:
    - param get const.product.software.version
    - param get const.ohos.apiversion
    - uname -m
    - param get const.product.cpu.abilist
  projection_calls_sha256: 29089aec7b4fb55b9073b26505b54d93e769253de8dab4d08042567881b16567
  projection_calls_hash_basis: SHA-256 of the four projected call strings joined by LF with a trailing newline; no host path, target, or raw stdout included
rebind_results_projection:
  const.product.software.version: PLA-AL10 7.0.0.102(SP8C00E102R7P3)
  const.ohos.apiversion: "26"
  uname_m: aarch64
  const.product.cpu.abilist: arm64-v8a
results_reference:
  location: N/A - session-executed readonly probes; values projected in this record only
  projection_only: true
hash_manifest_reference:
  location: N/A - no external hash manifest; this record is the sole projection
rebind_seal_reference:
  location: N/A - no external seal; session-executed readonly probes only
raw_reference: N/A - raw stdout existed only in the executing process memory; not stored in-repo or externally
forbidden_capabilities_audit: no serial/UDID; no app list; no install/start/stop; no VPN; no continuous capture; no campaign runner; no Go/NetBird/WireGuard/private fork/MANAGE_VPN/privileged bypass
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
verdict_scope: user-authorized four readonly rebind probes only (2026-08-28); not E3 pass; not campaign pass; not install compatibility; not product or platform support
scope_statement: exact PHYS-1 HarmonyOS 7 readonly rebind discovery only; no extrapolation to E3 campaign success, HAP installability on the new build, Emulator, other devices, other builds, OpenHarmony products, E4-E7, data plane, or product support
reviewer: isolated-anthropic-claude-opus-5-reviewer
reviewed_at: 2026-08-28T19:36:58+08:00
review_findings:
  blocker: 0
  major: 0
review_record: registration review 2026-08-28 by isolated-anthropic-claude-opus-5-reviewer; 0 blocker / 0 major on this record's content (probe values, projection hash, HDC binary hash, window, scope statement)
```

## 判定解释

`verdict: pass` 只覆盖本次授权的四条只读 rebind：`const.product.software.version` → `PLA-AL10 7.0.0.102(SP8C00E102R7P3)`，`const.ohos.apiversion` → `26`，`uname -m` → `aarch64`，`const.product.cpu.abilist` → `arm64-v8a`。它不表示 E3 关闭，不表示 `E3-PHYS-PREFLIGHT` campaign 通过，不主张 HAP 在新 build 上的安装兼容性，也不自动开放 E8。

新 build `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` 由本 rebind 的 `software.version` 探针实测确认；API `26` / `aarch64` / `arm64-v8a` 亦由本 rebind 实测，不从 build 推断。distribution `HarmonyOS` 与 model `PLA-AL10` 沿用既有实测记录。

raw 输出仅仓外受控保存；仓内只登记脱敏 projection 与 hash。`projection_calls_sha256` 为四条投影命令文本的现场 SHA-256，不含 target、主机路径或 raw stdout。target token 未投影、未输出、未持久化。

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`。
- E8 保持 `CLOSED`。
- 旧 pair `AUTH-E3-PHYS1API26-20260825-0001` 已因 gate 5 build 漂移以 `governance-tuple-drift-retired` 退役（见[退役登记](e3-physical-preflight-authorization-2026-08-25-0001.md)），其仓外对象逐字节保全，不得复用。
- 本 rebind 完成后，新元组冻结与新 AUTH/pair 由 [`AUTH-E3-PHYS1API26-20260828-0001`](e3-physical-preflight-authorization-2026-08-28-0001.md) 登记；HAP/source 复用沿用 ADJ-20260806-0003 先例（安装兼容性仅实测认定，HAP hash 由 freeze 校验逐字核验）。设备执行前仍须按新 AUTH 的完整 13 门重新确认。
