# N0 native core Emulator EV-N0-EMU24-20260810-0002 reviewed-pass 记录

最后核验：2026-08-10

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `n0-emulator-run.sh` 完整模式正式执行 `EV-N0-EMU24-20260810-0002` 的**完整测量与 seal 证据**。这是修复 0001 runner seal defect 后首次跑通全流程（离线双 ABI 构建 → snapshot HAP 构建 → 双 HAP 安装 → `aa test` → 定向 HiLog → 判定 → 截图 → 清理 → 残留/敏感扫描 → seal），在真实官方 API 24 x86_64 phone Emulator 上测得单一 native WireGuard core（BoringTun 0.7.1 `ffi-bindings`）的加载与冒烟 **PASS**。经双路只读独立终审 `REV-N0-EMU24-20260810-0002`（0 blocker/0 major）后，本记录 `record_status: reviewed-pass`、`verdict: pass`；**N0 overall 双轴验收 `reviewed-pass/pass` 成立**。

## 结论边界

- **N0 overall `reviewed-pass/pass`（双轴成立）**：N0(a) 固定 NetBird v0.76.3/f65f7b34 协议/行为/许可 inventory 与 compat oracle 已定义（见 [N0 决议](../n0-native-client-feasibility.md)）；N0(b) 由本记录 `EV-N0-EMU24-20260810-0002` 实测 `record_status: reviewed-pass`、`verdict: pass`。按 N0 双轴验收，`reviewed-pass` 与 `verdict: pass` 同时成立 → 满足 N0 继续条件（但见「下一步」：不是继续实现）。
- **ID 已消耗**：`EV-N0-EMU24-20260810-0002` 已被本次完整执行占用，**禁止同 ID 重跑**（no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit `2`）。`EV-N0-EMU24-20260810-0001`（consumed-failure，runner seal defect）**不参与本次判定**，其 11 份 raw 保持原样、不改写。
- **真实 official Emulator runtime 数据，非合成 fixture**：本记录基于官方 API 24 x86_64 phone Emulator（固定实例 `netbird_api24_phone`，HDC `127.0.0.1:10000`）真实冷启动、双 HAP 安装、`aa test`、定向 HiLog 与截图；能力接受使用**固定正式发布**（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1 crate checksum `15dd6a8a…`）与**真实 official Emulator runtime 数据**，不是合成 fixture。
- **测量面（全部 pass）**：`MARKER_DISTINCT_COUNT=1`（唯一 `N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0`）、NAPI HiLog `N0_RUNPROBE` `smokeOk=1/x25519Ok=1/tunnelOk=1`、host `AA_TEST_RC=0`、guest `TestFinished-ResultCode: 0`、`HAP_MEMBER_IDENTITY=pass`（app/test HAP 内 `libs/x86_64/libentry.so` member 与 `out/x86_64/libentry.so` 字节一致，`4b054a7d…`）、`ARM64_MEMBER=false`（arm64 仅 cross-compile，HAP 不打包）、`FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`、`SENSITIVE_SCAN=pass_high_confidence_patterns`。
- **seal 完整（六字段 + 复算一致）**：manifest 含 `final_exit_code=0`、`run_status=pass`、`fail_reason=`、`transcript_final_bytes=44069`、`transcript_final_sha256=735c2b3e…`、`manifest_sha256=65796214…` 六项 seal 字段；复算一致（见「seal 复算」）。与 0001 的 unsealed 状态形成对照：0001 的 11 份 raw 哈希只是提交时带外锚点，本记录 raw 与 manifest 之间有可验证的 seal 链。
- **不证明**：本记录**不证明** VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；N0 范围外能力仍为未验证，不得外推。
- **下一步不是继续实现**：N0 pass 后**不自动进入实现**；仅未来用户显式授权 + fresh confirmation 后，既有 `E3-PHYS-PREFLIGHT` 仍是第一物理动作；N0 与物理 E3 **都 pass 后**才提交新 ADJ/T0 治理定义 native N1-Nx 门；治理生效前 E8 保持 `CLOSED`，不得以 native 静默替代 Go E1，不得开启产品实现。
- **E8 状态不变**：`E8_STATUS=CLOSED`、`PHYSICAL_DEVICE_USED=false`；本记录不改变 E8/E3 状态。
- **host-preflight 说明**：`EV-N0-N0HOST-20260809-0001`（host-only，`record_status: collected`、`reviewer: pending independent review`）的 pending 状态由本正式 0002 sealed evidence 覆盖其作为 N0(b) 证据的角色；其独立 review 字段保持 `pending`，**不伪造**。

## 证据记录

```yaml
evidence_id: EV-N0-EMU24-20260810-0002
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: N0(b)
related_stages_or_gates: [E8]
execution: live-full-emulator-measured-pass
target_tuple:
  distribution: HarmonyOS 6.1.1(24)
  device: Emulator instance netbird_api24_phone (fixed; HDC 127.0.0.1:10000)
  full_system_version: guest boot completed; distribution reported HarmonyOS (see transcript CONNECTIVITY/READINESS)
  architecture: x86_64 (guest load surface); arm64 cross-compile only, no load claim
  sdk_api_syscap: API 24; native SDK 6.1.1.125 (oh-uni-package.json)
  channel: unsigned debug HAPs; no signing/distribution input
code_sha: ec099199598f16daa3dd0344cfef69614f9792ef (repository HEAD at run time; evidence commit 4d4caefe192e40739ce49c94bed5b5b600be8282)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (fixed baseline per N0 resolution); BoringTun 0.7.1 crate sha256 15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939
snapshot_sha: 2c567dc721c6582f93a15b241e843e3bbff3f7f3 (pinned r1-api24-hap snapshot, git archive verified)
toolchain: Debian worker; Command Line Tools 6.1.1.290 (stable; hvigorw/ohpm/native SDK for build) + 26.0.0.461 (beta; Emulator binary + hdc); hvigor 6.24.3; hdc 3.2.0e; node v24.18.1; ohpm 6.1.2.285; rustc/cargo 1.92.0; OHOS clang 15.0.4; all HOST_CHECK passed (shellcheck absent -> external bash -n required)
working_directory: /home/worker/work/base/netbird-harmonyos
command: bash spikes/n0-native-core/n0-emulator-run.sh
input: fixed Emulator instance netbird_api24_phone / HDC 127.0.0.1:10000; snapshot commit 2c567dc721c6582f93a15b241e843e3bbff3f7f3; default EVIDENCE_ROOT docs/evidence/raw; no PHYS_1_TARGET, no non-fixed target
expected: full formal run: offline dual-ABI build + snapshot HAP build + dual-HAP install + aa test + directed HiLog + judgment + base manifest + screenshot + cleanup + residual/sensitive scan + seal (final_exit_code/run_status/fail_reason/transcript_final_bytes/transcript_final_sha256/manifest_sha256)
actual: all measurement steps passed; MARKER_DISTINCT_COUNT=1 (unique N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0); NAPI HiLog N0_RUNPROBE smokeOk=1/x25519Ok=1/tunnelOk=1; host AA_TEST_RC=0; guest TestFinished-ResultCode: 0; HAP_MEMBER_IDENTITY=pass (app/test HAP libs/x86_64/libentry.so member byte-equal to out/x86_64/libentry.so, 4b054a7d…); ARM64_MEMBER=false (arm64 cross-compile only, HAP not packaged); screenshot collected; cleanup complete, FINAL_RESIDUAL_PROCESS=false, FINAL_RESIDUAL_PORT=false, SENSITIVE_SCAN=pass_high_confidence_patterns; seal complete: final_exit_code=0, run_status=pass, six seal fields present and recomputed (see seal verification)
started_at: 2026-08-10T01:53:19+08:00
ended_at: 2026-08-10T01:54:46+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256:
  libentry_x86_64: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
  libentry_aarch64: a9b39cb0899c0bd2a826691872418d540d08c3a17d70f9a2eb7cf29709744c51
  app_hap: 49d7658a56e652c1a223239b1af81e44af0cb977dba124d1876b1f03af72da5e
  test_hap: ad1de93fede44ccd7d52570a84116cb20a2a8447877148236fd725fb498f5dd6
  app_member: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
  test_member: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
  cargo_lock: 5ea1a9bfd648df3c65937e4f4f9a51036c5a7b2acd526e7d02d92f8544f11b6f
  source_manifest: dd89277c840c48ecde5a0423c2fa69504d4fcd66a13f1053d8ab0dc7520b44e6
  screenshot: 852d565fb1b56f3febab342f23e80abba08280ab3023bcae4e314569b033bd1b
raw_log_reference: docs/evidence/raw/EV-N0-EMU24-20260810-0002-* (11 files; hashes below; sealed manifest + transcript seal chain, recomputed)
verdict: pass
reviewer: openai/gpt-5.6-sol (2 minor) + xai/grok-4.5 (6 minor); dual independent read-only review 0 blocker / 0 major (see review record REV-N0-EMU24-20260810-0002)
reviewed_at: 2026-08-10T02:03:58+08:00
review_record: REV-N0-EMU24-20260810-0002
```

## 11 份 raw 文件与 sha256（sealed，复算一致）

| # | raw 文件 | sha256 |
| --- | --- | --- |
| 1 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-transcript.log` | `735c2b3ec7091fd5f539643616037d77473fae9d746ced2c23e1d7cdeeb6723d` |
| 2 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-n0-build.log` | `c2806e8ad04e9133a9c8a8b826ae6e5d92523ef9785b7f32234c9ea85b7cdba8` |
| 3 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-snapshot-prep.log` | `6ff5a201fd9d4448189be85c96b603dd26664bc19cda90e49c4b83f394f014f1` |
| 4 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-build.log` | `f268388846a50eb1c0f7b1c20badca3e26235d59601f825dfab691fc8a4d213a` |
| 5 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-aa-test.log` | `527f61cbc58d0dddc293598fe357c6e825a729180ec57b316a03cd5ef8c982a2` |
| 6 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-hilog-tag.log` | `65251bc1c00a9da3bb39a2ff777b3d075f9e72f01cb8e6140c7b33afb06fb8d5` |
| 7 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-hilog-app-full.log` | `4e44eb36a46a8b19ec7393c85bb7f9ae7138ec421ef190952bff4932d913560c` |
| 8 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-emulator-console.log` | `dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e` |
| 9 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-source-manifest.txt` | `dd89277c840c48ecde5a0423c2fa69504d4fcd66a13f1053d8ab0dc7520b44e6` |
| 10 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-run1.png` | `852d565fb1b56f3febab342f23e80abba08280ab3023bcae4e314569b033bd1b` |
| 11 | `docs/evidence/raw/EV-N0-EMU24-20260810-0002-manifest.txt` | `14149b94d7f2176334c58a2d25f96fefa5a5626be08dcd17d964ca3715deee02` |

说明：manifest 内记录的逐文件 sha256（`n0_build_log_sha256`、`snapshot_log_sha256`、`build_log_sha256`、`aa_test_log_sha256`、`tag_hilog_sha256`、`app_hilog_sha256`、`console_sha256`、`source_manifest_sha256`、`screenshot_sha256` 等）与上表 raw 哈希全部一致；manifest 含 `manifest_sha256` 自哈希，transcript 含 `transcript_final_sha256` 封印，raw 与 manifest 之间存在可验证的 seal 链（与 0001 的 unsealed 带外锚点不同）。

## seal 复算

manifest 末尾两行由 runner seal 追加（`seal_and_finalize`）：

```text
transcript_final_sha256=735c2b3ec7091fd5f539643616037d77473fae9d746ced2c23e1d7cdeeb6723d
manifest_sha256=65796214a71c4fea59b6d16142f258b7352d580ae51822bbfc30bd6b4c828165
```

- `transcript_final_sha256` = 最终 transcript.log 前 `transcript_final_bytes=44069` 字节的 sha256（`head -c 44069` 复算）= `735c2b3e…`，与 raw transcript 文件全量 hash 一致（transcript 未在 seal 后追加内容）。
- `manifest_sha256` = **追加 `transcript_final_sha256` 行之后、追加 `manifest_sha256` 行之前**的 manifest 文件 sha256（即前 53 行）= `65796214…`；这是自 hash 语义，不是全文件 hash（全文件 hash 为 `14149b94…`，见上 `raw_log_reference`）。
- 六项 seal 字段齐备：`final_exit_code=0`、`run_status=pass`、`fail_reason=`（空）、`transcript_final_bytes=44069`、`transcript_final_sha256=735c2b3e…`、`manifest_sha256=65796214…`。

## transcript 关键行

```text
MARKER_DISTINCT_COUNT=1
N0_CORE_PROBE_RESULT_LINE=N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0
NAPI_SMOKE_OK=1 NAPI_X25519_OK=1 NAPI_TUNNEL_OK=1
GUEST_RESULT_CODE=0
MEASURED_VERDICT=pass
ENDED_AT=2026-08-10T01:54:46+08:00
RECORD_STATUS=collected
VERDICT=pass
FINAL_RESIDUAL_PROCESS=false
FINAL_RESIDUAL_PORT=false
SENSITIVE_SCAN=pass_high_confidence_patterns
CLEANUP_BEGIN=teardown
CLEANUP_STAGING=skipped-emulator-not-started
CLEANUP_HDC=kill-issued
CLEANUP_TEMP=removed
CLEANUP_END=teardown-complete
TRAP_EXIT_CODE=0 RESULT=pass
```

（`RECORD_STATUS=collected` 是 runner 在运行结束时 sealed 的 pre-review 状态，formal raw 原样保留、不改写；本记录经终审 `REV-N0-EMU24-20260810-0002` 后，记录级 `record_status` 为 `reviewed-pass`，`verdict` 为 `pass`。）

## 终审记录 REV-N0-EMU24-20260810-0002

```yaml
review_id: REV-N0-EMU24-20260810-0002
evidence_id: EV-N0-EMU24-20260810-0002
reviewed_at: 2026-08-10T02:03:58+08:00
reviewers:
  - openai/gpt-5.6-sol (2 minor)
  - xai/grok-4.5 (6 minor)
review_mode: dual independent read-only review (no device/Emulator execution, no file modification)
blocker: 0
major: 0
minor: 2 (openai/gpt-5.6-sol) / 6 (xai/grok-4.5)
record_status: reviewed-pass
verdict: pass
```

- 两路独立审查（`openai/gpt-5.6-sol` 与 `xai/grok-4.5`）均**只读独立复算**：不运行设备/Emulator、不修改任何文件（含 raw）；复算 11 份 raw sha256、manifest 自 hash 语义与 transcript 前 N 字节封印，全部与记录值一致。
- 两路均 **0 blocker / 0 major**；`openai/gpt-5.6-sol` 记录 2 条 minor、`xai/grok-4.5` 记录 6 条 minor，均为**非阻塞记录改进建议**（不改变判定、不要求改写 raw、不扩范围修复）；minor 明细以审查会话 `REV-N0-EMU24-20260810-0002` 为准。
- 结论：`record_status: reviewed-pass`、`verdict: pass`。`reviewed-pass` 与 `verdict: pass` 同时成立，满足 N0 双轴验收。

## 边界与后续

- **N0 overall `reviewed-pass/pass`**：N0(a) inventory+oracle 已定义、N0(b) 实测 pass；N0 双轴验收通过。能力接受使用固定正式发布（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1）与真实 official Emulator runtime 数据，不是合成 fixture。
- **不证明**：本记录不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；这些能力仍为未验证，不得外推。
- **下一步不是继续实现**：N0 pass 后不自动进入实现；仅未来用户显式授权 + fresh confirmation 后，既有 `E3-PHYS-PREFLIGHT` 仍是第一物理动作；N0 与物理 E3 都 pass 后才提交新 ADJ/T0 治理定义 native N1-Nx 门；治理生效前 E8 保持 `CLOSED`，不得以 native 静默替代 Go E1，不得开启产品实现。
- **E8 保持 `CLOSED`**：`e8_status=CLOSED`（manifest）、`PHYSICAL_DEVICE_USED=false`；本记录不改变 E8/E3 状态。
- **仅精确 API 24 x86_64 Emulator**：结论只适用于记录的目标元组（HarmonyOS 6.1.1(24) / API 24 / x86_64 / phone Emulator `netbird_api24_phone`），不得外推 arm64、具名物理设备或华为商用 HarmonyOS。
- **不得同 ID 重跑**：`EV-N0-EMU24-20260810-0002` 已消耗，禁止同 ID 重跑；0001（consumed-failure）不参与判定，其 raw 保持原样。
- **host-preflight**：`EV-N0-N0HOST-20260809-0001` 的 pending 状态由本正式 0002 sealed evidence 覆盖其作为 N0(b) 证据的角色；其独立 review 字段保持 `pending`，不伪造。
- **E3 物理设备禁 HDC 约束不变**；完整模式也只操作固定 Emulator target `127.0.0.1:10000`。
