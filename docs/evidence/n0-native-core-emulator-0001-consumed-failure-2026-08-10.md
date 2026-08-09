# N0 native core Emulator EV-N0-EMU24-20260810-0001 consumed-failure 记录（runner seal defect）

最后核验：2026-08-10

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `n0-emulator-run.sh` 完整模式首次真实执行 `EV-N0-EMU24-20260810-0001` 的失败证据。该次执行**完成了全部 guest 测量面**（唯一 PASS marker、NAPI 三项 1、host aa RC=0、guest ResultCode=0、HAP member 一致、arm64 compile-only；cleanup 旧检查只观察到 Emulator/qemu 进程与 10000/5555/8710 端口为空，旧 `hdc -m -s` 顺序敏感 matcher 不足以证明 hdc daemon 无残留，seal 卡住反证有设备阶段子进程持有 pipe 写端），但 runner 在**seal 阶段**的 `wait "$TEE_PID"` 处卡住，由执行会话 30 分钟超时终止；manifest 因此缺少全部 final/seal 字段。证据价值在于**测量面已真实完成、seal 缺陷可复现、可归因**，并据此修复 runner、把下一次正式 ID 前移到 `EV-N0-EMU24-20260810-0002`。

## 结论边界

- **ID 已消耗**：`EV-N0-EMU24-20260810-0001` 已被本次失败执行占用，**禁止同 ID 重跑**（no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit `2`）；下一次唯一正式 ID 为 `EV-N0-EMU24-20260810-0002`（runner `DEFAULT_EVIDENCE_ID` 已前移）。
- **runner defect（seal 阶段），不是平台结论**：失败根因是旧 seal 机制——formal 模式用 process-substitution `exec > >(tee "$TRANSCRIPT") 2>&1` 把 runner stdout 接到 tee 管道，`seal_transcript` 先 `exec 1>&3 2>&3` 切回 fd 3 再 `wait "$TEE_PID"`；tee 只有在管道写端全部关闭后才收到 EOF，而设备阶段有子进程（helper/HDC 派生进程等）继承了管道写端，`wait` 永不返回，runner 卡死在 seal 处，由执行会话 30 分钟超时终止。**本次失败不证明也不否定** N0 单一 native WireGuard core 在 API 24 x86_64 Emulator 上的加载/冒烟行为；N0 仍为无 sealed pass。
- **guest 测量面 PASS（运行期 pre-review 状态）**：transcript 记录 `MEASURED_VERDICT=pass`、`RECORD_STATUS=collected`、`VERDICT=pass`，具体为：`MARKER_DISTINCT_COUNT=1`（唯一 `N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0`）、NAPI HiLog `N0_RUNPROBE` `smokeOk=1/x25519Ok=1/tunnelOk=1`、host `AA_TEST_RC=0`、guest `TestFinished-ResultCode: 0`、`HAP_MEMBER_IDENTITY=pass`（app/test HAP 内 `libs/x86_64/libentry.so` member 与 `out/x86_64/libentry.so` 字节一致，`4b054a7d…`）、`ARM64_MEMBER=false`（arm64 仅 cross-compile，HAP 不打包）。**cleanup 不声称无残留**：旧检查只观察到 Emulator/qemu 进程与 10000/5555/8710 端口为空（`FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`、`SENSITIVE_SCAN=pass_high_confidence_patterns`），但旧 `hdc -m -s` 顺序敏感 matcher 不足以证明 hdc daemon 无残留；seal 卡住反证有设备阶段子进程持有 pipe 写端，cleanup 残留检测按 fail-closed 方向收紧（见下）。
- **manifest 缺 final/seal 字段**：`EV-N0-EMU24-20260810-0001-manifest.txt` 止于 `sensitive_scan=pass_high_confidence_patterns`，**没有** `final_exit_code` / `run_status` / `fail_reason` / `transcript_final_bytes` / `transcript_final_sha256` / `manifest_sha256` 六项 seal 字段；transcript 止于 `TRAP_EXIT_CODE=0 RESULT=pass`（teardown 已执行完，随后卡在 seal）。manifest 内记录的逐文件 sha256 与 raw 文件一致（见下），但 manifest 自身未自哈希、transcript 未封印。
- **绝非 N0 pass / reviewed-pass**：`record_status: collected`（运行期 pre-review 状态）、`verdict: fail`（runner defect after measurement）。测量面 PASS 只是运行期中间状态，未经过 seal、未经过独立审查，**不构成 N0 pass，不升级 reviewed-pass**；N0 门保持未通过。
- **raw 未封印，hash 只作提交时带外锚点**：11 份 raw 文件保留原样（不改写、不删除、不重命名）；本记录列出的 sha256 是提交时对 raw 的带外快照锚点，**不声称内生 tamper evidence**（无 manifest 自哈希、无 transcript 封印，raw 与 manifest 之间没有可验证的 seal 链）。unsealed raw 支持 positive guest observation（测量面证据可复核），但不构成 accepted N0 pass。
- **独立审查**：`anthropic/claude-opus-5` + `moonshotai/kimi-k2.7-code` 双路只读独立审查，对上述结论一致（0 blocker/0 major）；本记录 `record_status: collected`，待正式终审。
- **E8 状态不变**：`E8_STATUS=CLOSED`、`PHYSICAL_DEVICE_USED=false`；本记录不改变 E8/E3 状态。

## 证据记录

```yaml
evidence_id: EV-N0-EMU24-20260810-0001
information_status: current-measured
record_status: collected
stage_or_gate: N0(b)
related_stages_or_gates: [E8]
execution: live-full-emulator-measured-then-seal-hung
target_tuple:
  distribution: HarmonyOS 6.1.1(24)
  device: Emulator instance netbird_api24_phone (fixed; HDC 127.0.0.1:10000)
  full_system_version: guest boot completed; distribution reported HarmonyOS (see transcript CONNECTIVITY/READINESS)
  architecture: x86_64 (guest load surface); arm64 cross-compile only, no load claim
  sdk_api_syscap: API 24; native SDK 6.1.1.125 (oh-uni-package.json)
  channel: unsigned debug HAPs; no signing/distribution input
code_sha: 7cd2973892f02768c157634463a569f3ba0e0281 (repository HEAD at run time; the seal fix lands after this record)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (fixed baseline per N0 resolution); BoringTun 0.7.1 crate sha256 15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939
toolchain: Debian worker; Command Line Tools 6.1.1.290 (stable; hvigorw/ohpm/native SDK for build) + 26.0.0.461 (beta; Emulator binary + hdc); rustc/cargo 1.92.0; OHOS clang 15.0.4; all HOST_CHECK passed
working_directory: /home/worker/work/base/netbird-harmonyos
command: bash spikes/n0-native-core/n0-emulator-run.sh
input: fixed Emulator instance netbird_api24_phone / HDC 127.0.0.1:10000; snapshot commit 2c567dc721c6582f93a15b241e843e3bbff3f7f3; default EVIDENCE_ROOT docs/evidence/raw; no PHYS_1_TARGET, no non-fixed target
expected: full formal run: offline dual-ABI build + snapshot HAP build + dual-HAP install + aa test + directed HiLog + judgment + base manifest + screenshot + cleanup + residual/sensitive scan + seal (final_exit_code/run_status/fail_reason/transcript_final_sha256/manifest_sha256)
actual: all measurement steps passed (see guest measurement surface above); base manifest written with verdict=pass; screenshot collected; cleanup old checks observed only empty Emulator/qemu processes and empty 10000/5555/8710 ports (FINAL_RESIDUAL_PROCESS=false, FINAL_RESIDUAL_PORT=false) — the old order-sensitive `hdc -m -s` matcher is insufficient to prove the hdc daemon is gone, and the seal hang itself evidences a device-phase child holding the pipe write end, so no residual-free claim is made; teardown printed TRAP_EXIT_CODE=0 RESULT=pass; runner then hung in seal_transcript's wait "$TEE_PID" (tee never saw EOF because a device-phase child held the pipe write end) and was terminated by the execution session at 30 minutes; manifest has NO final/seal fields; exit code of the runner process itself was not recorded (killed)
started_at: 2026-08-10T00:55:16+08:00
ended_at: 2026-08-10T00:56:40+08:00 (measurement ENDED_AT; runner killed ~30 min later by the execution session)
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256:
  libentry_x86_64: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
  libentry_aarch64: a9b39cb0899c0bd2a826691872418d540d08c3a17d70f9a2eb7cf29709744c51
  app_hap: 94d18f5349b812d6a8ae24941fb268a53db710665631a60c58f7da7ec6615ecc
  test_hap: b890b862fcd1b6208600d9d6e12a0bd512cd2cfb4fae786b23b06d0532bc4d1f
  app_member: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
  test_member: 4b054a7d482c26e73dea101938ceabf4b024a7d9c53f4923ebec15fb7859a3b7
raw_log_reference: docs/evidence/raw/EV-N0-EMU24-20260810-0001-* (11 files; hashes below; raw NOT sealed, hashes are out-of-band commit-time anchors only)
verdict: fail (runner defect after measurement; seal hung; NOT a platform verdict, NOT N0 pass)
reviewer: pending independent review (read-only consistency confirmed by anthropic/claude-opus-5 + moonshotai/kimi-k2.7-code)
reviewed_at: pending
review_record: pending
```

## 11 份 raw 文件与当前 sha256（带外锚点，非 seal）

| # | raw 文件 | sha256 |
| --- | --- | --- |
| 1 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-transcript.log` | `79d20089ef4d74e4d6d73e43d57bc89fe21f925e56b9dd40c0d9ad57906a8e5d` |
| 2 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-n0-build.log` | `c2806e8ad04e9133a9c8a8b826ae6e5d92523ef9785b7f32234c9ea85b7cdba8` |
| 3 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-snapshot-prep.log` | `01bd6be73d8848398393c3d52aa9873f062f9366f18f9daaf18db41adee95f58` |
| 4 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-build.log` | `6a9e56e7073f3ed99118be0ab62d5528b2c5026892bfabdeb73dc48ec99de144` |
| 5 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-aa-test.log` | `527f61cbc58d0dddc293598fe357c6e825a729180ec57b316a03cd5ef8c982a2` |
| 6 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-hilog-tag.log` | `8e8d2613629927b44ad8ab50855e8e015579dd3fcda7fa6087e1ab926ed5e96e` |
| 7 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-hilog-app-full.log` | `f3e4a8baa6792fa46962c16c131e3ae095a7c71c2a222e83e2eaa2d961bb09ed` |
| 8 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-emulator-console.log` | `dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e` |
| 9 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-source-manifest.txt` | `8346ebc22497f667cf1592bc1e9d421c3a7af4ad2d45cda253bc805894ec8679` |
| 10 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-run1.png` | `f1d2ea571a3ad31f84d25684c76668bbd22a3911201bd4f81512e07bcabd0baf` |
| 11 | `docs/evidence/raw/EV-N0-EMU24-20260810-0001-manifest.txt` | `921ec74e8804626ea83a5697151334ed7ee67f84cf2fdb71d00d838eda75b0bb` |

说明：manifest 内记录的逐文件 sha256（`aa_test_log_sha256`、`tag_hilog_sha256`、`app_hilog_sha256`、`console_sha256`、`n0_build_log_sha256`、`snapshot_log_sha256`、`build_log_sha256`、`source_manifest_sha256`、`screenshot_sha256` 等）与上表 raw 哈希一致；但 manifest 自身**没有** `manifest_sha256` 自哈希，transcript **没有** `transcript_final_sha256` 封印，因此上表哈希只作为提交时带外锚点，**不构成内生 tamper evidence**。

## transcript 关键行（seal 前）

```text
MARKER_DISTINCT_COUNT=1
N0_CORE_PROBE_RESULT_LINE=N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0
NAPI_SMOKE_OK=1 NAPI_X25519_OK=1 NAPI_TUNNEL_OK=1
GUEST_RESULT_CODE=0
MEASURED_VERDICT=pass
ENDED_AT=2026-08-10T00:56:40+08:00
RECORD_STATUS=collected
VERDICT=pass
FINAL_RESIDUAL_PROCESS=false
FINAL_RESIDUAL_PORT=false
SENSITIVE_SCAN=pass_high_confidence_patterns
CLEANUP_BEGIN=teardown
CLEANUP_STAGING=skipped-emulator-not-started
CLEANUP_HDC=kill-issued
RESIDUAL_PROCESS=''
RESIDUAL_PORT=''
CLEANUP_TEMP=removed
CLEANUP_END=teardown-complete
TRAP_EXIT_CODE=0 RESULT=pass
```

（`TRAP_EXIT_CODE=0 RESULT=pass` 之后 runner 卡在 `seal_transcript` 的 `wait "$TEE_PID"`，由执行会话 30 分钟超时终止；manifest 未追加任何 final/seal 字段。）

## 失败分析（可复现）

旧 seal 机制（修复前）：

```bash
# formal evidence setup
exec 3>&1
exec > >(tee "$TRANSCRIPT") 2>&1
TEE_PID=$!
# seal
seal_transcript() {
  exec 1>&3 2>&3
  if [[ -n "${TEE_PID:-}" ]]; then
    wait "$TEE_PID" 2>/dev/null || true
  fi
}
```

`tee` 只有在管道写端全部关闭后才收到 EOF 退出；设备阶段有子进程（helper/HDC 派生进程等）继承了 runner 的管道写端，`wait "$TEE_PID"` 永不返回。修复方向（已实施于 runner，见下）：取消 process-substitution tee/fd3/TEE_PID，formal 模式 no-clobber 后创建空 transcript 并 `exec >>"$TRANSCRIPT" 2>&1`（O_APPEND 直写）；seal 不等待、不切回外部 stdout，先把 runner 输出切到 `/dev/null`，对 manifest 追加 `final_exit_code`/`run_status`/`fail_reason`，再记录 `transcript_final_bytes`（stat size）与 `transcript_final_sha256`（前 N 字节，`head -c N` 复算），最后 manifest 自哈希；HDC 残留检测改为精确进程名 `pgrep -ax hdc`（主路径与 teardown；旧 `hdc -m -s` 顺序敏感 matcher 不足以证明 hdc daemon 无残留），可 daemonize 的 helper/HDC 调用 `</dev/null` 且不依赖外部 stdout；selftest 真实覆盖「子进程继承 transcript fd 仍存活时 seal 在有界时间内完成、记录 6 个 seal 字段、可按前 N 字节复算（含 seal 后追加一行、整文件 hash 已不同的场景）、manifest 自哈希复算、不留 sleep」。

## 修复与下一次 ID

- runner 已修复（seal 机制、HDC 残留检测、daemonize 硬化、selftest 真实覆盖），不改变测量/判定/guard 语义；cleanup 残留检测按 fail-closed 方向收紧（旧 `hdc -m -s` 顺序敏感 matcher 不足以证明 hdc daemon 无残留，改为精确进程名 `pgrep -ax hdc`）。
- `DEFAULT_EVIDENCE_ID` 已从 `EV-N0-EMU24-20260810-0001` 前移到 `EV-N0-EMU24-20260810-0002`；README 的 formal/raw 例子同步。
- 下一次完整正式执行使用 `EV-N0-EMU24-20260810-0002`（默认值或显式 `EVIDENCE_ID`），且**不得**对 0001 同 ID 重跑。
- 0001 的 11 份 raw 文件保持原样，供独立审查核对哈希（见上表）。

## 边界与后续

- 本记录不改变 N0 门状态：测量面 PASS 是运行期 pre-review 状态，未 seal、未终审，**不构成 N0 pass，不升级 reviewed-pass**；N0 门保持未通过。
- 本记录 `record_status: collected`，待独立终审；`verdict: fail` 只表示 runner 层 seal 缺陷，不构成平台结论。
- E8 保持 `CLOSED`、`PHYSICAL_DEVICE_USED=false`；E3 物理设备禁 HDC 约束不变；完整模式也只操作固定 Emulator target `127.0.0.1:10000`。
