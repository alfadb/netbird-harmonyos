# E3-PHYS-PREFLIGHT host remediation（ADJ-20260807-0003 runner / 2026-08-08）

最后核验：2026-08-08

本文登记 `ADJ-20260807-0003` runner 变更后的 host 侧修复与准备记录（**非** live campaign、**非** 设备证据）：执行 commit `e3fe0c642c28b8a332c0f70db2217787884334e9`（parent `c6acae746f4013ef1ac5ece7593ace2de8b3fb38`，M1/M3 probe fixes）上的 runner/freeze example/selftest 更新、host-only selftest（`HDC_PROCESSES=0`）与独立审查（0 blocker/0 major）、Windows 签名/构建主机重建后的最终 signed HAP / build manifest / source archive / blob manifest 快照、candidate freeze（`plan_status: blocked`）与 DryRun 记录。host `reviewed-pass` **不等于** E3 pass；本记录 **不** 授权任何 Live/HDC/install/device-ready 执行。E3 未关闭，E8 保持 `CLOSED`。

```yaml
evidence_id: EV-E3-PHYS1HOST-20260808-0001
exception: E3-PHYS-PREFLIGHT
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E3
related_stages_or_gates: [E8]
execution: host-only
is_evidence: false
plan_status_at_registration: blocked-awaiting-device-authorization
campaign_status: N/A - host remediation record; no live campaign; candidate prepared but not live
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
code_sha: e3fe0c642c28b8a332c0f70db2217787884334e9
code_parent_sha: c6acae746f4013ef1ac5ece7593ace2de8b3fb38
code_note: execution commit for ADJ-20260807-0003 runner/freeze/selftest changes (M1/M3 probe fixes); even if later docs-only commits exist, execution bytes remain e3fe0c642c28b8a332c0f70db2217787884334e9
upstream_sha: N/A - no Go, NetBird, WireGuard, or other upstream runtime participated
toolchain:
  HDC: 3.2.0d
  HDC_executable_sha256: fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116
runner_sha256: bound inside out-of-repo candidate freeze (8dc393...); not projected here
host_selftest:
  result: pass
  hdc_process_count: 0
  independent_review: 0 blocker / 0 major
artifact_sha256:
  hap_a: 1e902... # abbreviated projection of out-of-repo final signed HAP A
  hap_b: abb598... # abbreviated projection of out-of-repo final signed HAP B
build_manifest_sha256: 027387... # abbreviated projection of out-of-repo build manifest
source_archive_sha256: fd4b... # abbreviated projection of out-of-repo source archive
blob_manifest_sha256: a3f0... # abbreviated projection of out-of-repo blob manifest
candidate:
  campaign_id: E3-PHYS-PREFLIGHT-20260808-0001
  evidence_id: EV-E3-PHYS1API26-20260808-0001
  freeze_sha256: 8dc393... # abbreviated projection of out-of-repo candidate freeze manifest
  public_manifest_sha256: e7247... # abbreviated projection of out-of-repo public manifest
  plan_status: blocked
  live: false
  note: candidate IDs retained; a ready freeze may bind the same candidate identity only after explicit user device authorization + fresh device confirmation per governance decision; currently not executable
dry_run:
  scenario_results_sha256: 576f...
  hash_manifest_sha256: 4f552...
  campaign_seal_sha256: bbd4...
  transcript_sha256: f706...
  transcript_chain_head: b1c...
  freeze_contract_sha256: ff6...
  is_evidence: false
  hdc_process_count: 0
  integrity_violations: []
prior_0002_binding:
  scenario_results_sha256: 923bf0cad50a693225fbcc2c682ba91f4acb441ee9eeaa1523c1d096bc5bcda1
  hash_manifest_sha256: b7bf6974720632bdbbf6e208977352e52494d46a57470a52442a85afe0a87973
  campaign_seal_sha256: 6e98679a5f3c5b430d5a8a1e3334e5a39ac86eb529f97bbffd5571c4dbbf406f
old_20260807_candidate: INVALID-TIMELINE - unusable for any new live; runner changed under ADJ-20260807-0003
verdict: N/A - host remediation record; not a live campaign verdict
scope_statement: host-side remediation and preparation only; host reviewed-pass is not E3 pass; no Live/HDC/install/device-ready authorization; E3 open, E8 CLOSED
reviewer: independent host-side review
reviewed_at: 2026-08-08
review_findings:
  blocker: 0
  major: 0
```

## 判定解释

`record_status: reviewed-pass` 只表示 host 侧独立审查已完成，且 selftest、DryRun、candidate freeze 与哈希绑定没有 blocker 或 major 发现；它 **不** 表示 E3 pass，也 **不** 授权任何 Live/HDC/install/device-ready 执行。功能判定仍为：E3 未关闭，E8 继续 `CLOSED`。当前治理 `plan_status: blocked-awaiting-device-authorization`。

- **执行 commit**：`e3fe0c642c28b8a332c0f70db2217787884334e9`（parent `c6acae746f4013ef1ac5ece7593ace2de8b3fb38`），即 `ADJ-20260807-0003` runner/freeze example/selftest 变更的 execution commit（M1/M3 probe fixes）。即使后续存在 docs-only commit，execution bytes 仍为 `e3fe0c642c28b8a332c0f70db2217787884334e9`。
- **ADJ-20260807-0003 runner**：实现 S3/S7 双终态（`callback-or-strict-process-boundary`：callback terminal + post-destroy fd snapshot 优先，`FD_STILL_OPEN` 硬 fail 不可 fallback，否则严格 process-boundary fallback）、S5 撤销机制（`settings-app-info-force-stop`：人工 Settings>应用信息>A>强制停止 + 截图确认，HDC force-stop 明确 cleanup-only），并覆盖 0002 审查的 M4（S7 reason `requestId-missing` 与事件事实不符）与 M5（final record 未投影 `prior_blocked_binding`）修复。
- **host selftest**：`-SelfTest` + 完整 selftest 均 `pass`，`HDC_PROCESSES=0`（HDC 进程数 0）；独立审查 0 blocker / 0 major。
- **host 重建产物**（Windows 签名/构建主机重建后，均仓外对象，仓内只登记缩写投影）：最终 signed HAP A `1e902...`、B `abb598...`；build manifest `027387...`；source archive `fd4b...`；blob manifest `a3f0...`。
- **candidate**：campaign ID `E3-PHYS-PREFLIGHT-20260808-0001` / evidence ID `EV-E3-PHYS1API26-20260808-0001`；candidate freeze `8dc393...`、public manifest `e7247...`；freeze `plan_status: blocked`，**未 Live**。candidate IDs 已保留；`ready` freeze 可绑定同候选身份，但**仅**在用户显式设备授权 + fresh device confirmation 后按治理决定重生，**不能**原地改 candidate、**不能**复用已消费 ID；目前不可执行。
- **DryRun**：scenario-results `576f...`、hash-manifest `4f552...`、campaign-seal `bbd4...`、transcript `f706...`、transcript chain_head `b1c...`、freeze contract `ff6...`；`is_evidence: false`、`HDC_PROCESSES=0`、`integrity_violations: []`。
- **prior 0002 绑定**（`EV-E3-PHYS1API26-20260807-0002` 公开三 hash）：scenario-results `923bf0cad50a693225fbcc2c682ba91f4acb441ee9eeaa1523c1d096bc5bcda1`、hash-manifest `b7bf6974720632bdbbf6e208977352e52494d46a57470a52442a85afe0a87973`、campaign-seal `6e98679a5f3c5b430d5a8a1e3334e5a39ac86eb529f97bbffd5571c4dbbf406f`。
- **旧 20260807 candidate**：`INVALID-TIMELINE`，不可用于任何新 Live——runner 已在 `ADJ-20260807-0003`（commit `e3fe0c6`）下变更，旧 freeze 的时间线绑定（`e8eb1b6` code / 旧 runner）对新 runner 无效，仅历史保留。

## 公开绑定 hash（均仓外对象；仓内只登记缩写投影或完整值）

| 对象 | SHA-256 |
| --- | --- |
| 最终 signed HAP A | `1e902...`（缩写投影） |
| 最终 signed HAP B | `abb598...`（缩写投影） |
| build manifest | `027387...`（缩写投影） |
| source archive | `fd4b...`（缩写投影） |
| blob manifest | `a3f0...`（缩写投影） |
| candidate freeze | `8dc393...`（缩写投影） |
| candidate public manifest | `e7247...`（缩写投影） |
| DryRun scenario-results | `576f...`（缩写投影） |
| DryRun hash-manifest | `4f552...`（缩写投影） |
| DryRun campaign-seal | `bbd4...`（缩写投影） |
| DryRun transcript | `f706...`（缩写投影） |
| DryRun transcript chain_head | `b1c...`（缩写投影） |
| DryRun freeze contract | `ff6...`（缩写投影） |
| prior 0002 scenario-results | `923bf0cad50a693225fbcc2c682ba91f4acb441ee9eeaa1523c1d096bc5bcda1` |
| prior 0002 hash-manifest | `b7bf6974720632bdbbf6e208977352e52494d46a57470a52442a85afe0a87973` |
| prior 0002 campaign-seal | `6e98679a5f3c5b430d5a8a1e3334e5a39ac86eb529f97bbffd5571c4dbbf406f` |

## 门状态

- E3 未关闭；本记录不是 campaign `reviewed-pass/pass`，host `reviewed-pass` 不等于 E3 pass。
- E8 保持 `CLOSED`。
- 本记录 **不** 授权任何 Live/HDC/install/device-ready 执行；`plan_status: blocked-awaiting-device-authorization`。
- candidate `E3-PHYS-PREFLIGHT-20260808-0001` / `EV-E3-PHYS1API26-20260808-0001` 已准备、freeze `plan_status: blocked`、未 Live；candidate IDs 保留，`ready` freeze 可绑定同候选身份但仅在用户显式设备授权 + fresh device confirmation 后按治理决定；目前不可执行。
- 旧 20260807 candidate `INVALID-TIMELINE` 不可用；prior 0001/0002 历史记录保留不改写。
- 本登记期间禁止 HDC/设备命令。
