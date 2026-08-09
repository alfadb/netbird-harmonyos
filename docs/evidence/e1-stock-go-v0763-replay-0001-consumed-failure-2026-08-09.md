# E1 v0.76.3 stock Go 重放 EV-E1-EMU24-20260809-0001 consumed-failure 记录（runner defect）

最后核验：2026-08-09

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `e1-stock-go-replay.sh` 完整模式首次真实执行 `EV-E1-EMU24-20260809-0001` 的失败证据。该次执行在**任何测量之前**因 runner 脚本缺陷中止（`exit 1`），未产生平台判定；证据价值在于**失败本身可复现、可归因**，并据此修复 runner、把下一次正式 ID 前移到 `EV-E1-EMU24-20260809-0002`。

## 结论边界

- **ID 已消耗**：`EV-E1-EMU24-20260809-0001` 已被本次失败执行占用，**禁止同 ID 重跑**（no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit `2`）；下一次唯一正式 ID 为 `EV-E1-EMU24-20260809-0002`。
- **aborted-before-any-measurement**：执行在步骤 1（stock Go `libgoprobe.so` 构建）的 `build.sh` 调用处中止；未进行 ELF 验证、HAP 构建、安装、`aa test`、HiLog 采集与判定。
- **runner defect，不是平台结论**：失败根因是 runner 对 `readonly` 变量 `GO_PROBE_OUTPUT_DIR` 使用 bash 前缀赋值（`GO_PROBE_OUTPUT_DIR=… bash build.sh`），bash 对 readonly 变量赋值即报 `readonly variable` 并中止；同一位置的 `print_command` 打印的命令带 `env` 前缀（`env` 是外部程序，不受 shell readonly 属性限制）。打印命令与真实执行不一致，真实执行在环境赋值处中止。本次失败**不**证明也不否定 v0.76.3 官方 Go 1.25.12 loader/runtime 在 API 24 x86_64 Emulator 上的行为；E1 overall Go 仍为无测量、无 pass。
- **无 measured verdict / 无 manifest / 无 seal**：transcript 中没有 `MEASURED_VERDICT`、没有 `VERDICT` 行、没有 `manifest.txt`，因此也没有 `transcript_final_sha256` / `manifest_sha256` seal 行。
- **Emulator 未启动**：失败发生在 Emulator 冷启动之前，`emulator_started=0`；cleanup 记录 `CLEANUP_STAGING=skipped-emulator-not-started`，无 `CLEANUP_UNINSTALL`、无 `CLEANUP_EMULATOR`。
- **清理事实**：teardown 正常执行：`CLEANUP_BEGIN=teardown`、`CLEANUP_STAGING=skipped-emulator-not-started`、`CLEANUP_HDC=kill-issued`、`CLEANUP_TEMP=removed`、`CLEANUP_END=teardown-complete`；runner 在 build 阶段即中止，未触碰 Emulator/HDC 目标，无残留风险。
- **证据局限**：本记录只证明 0001 失败与 runner defect 的因果；三份 raw 文件保留原样（不改写、不删除、不重命名）；本记录 `record_status: collected`，待独立审查；不改变 E1/E8 状态，不升级 reviewed-pass。

## 证据记录

```yaml
evidence_id: EV-E1-EMU24-20260809-0001
information_status: current-measured
record_status: collected
stage_or_gate: E1
related_stages_or_gates: [E8, R1, R2]
execution: live-full-replay-aborted-before-measurement
target_tuple:
  distribution: N/A; run aborted before guest runtime; no platform measurement produced
  device: Emulator instance netbird_api24_phone; NOT started during this run
  full_system_version: N/A; no guest system version measured
  architecture: N/A; host x86_64 Debian; target tuple remains API 24 x86_64 Emulator
  sdk_api_syscap: N/A; no SDK/API runtime surface exercised
  channel: N/A; no HAP, signing or distribution input
code_sha: 19f022b7065af277bb297638c4b50be324b09233 (repository HEAD at run time; the runner defect fix lands after this record)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (verified via gh API before abort, see baseline-verify log)
toolchain: Debian worker with Command Line Tools 6.1.1.290 / Beta 26.0.0.461 / Go 1.25.12 / ffmpeg / KVM Emulator; all HOST_CHECK passed before abort (hvigorw, ohpm, hvigor-ohos-plugin, emulator, hdc, go, ohos-x86_64-clang, ffmpeg, gh, bash; shellcheck absent)
working_directory: [WORKSPACE]/spikes/r1-api24-hap
command: bash spikes/r1-api24-hap/e1-stock-go-replay.sh
input: fixed baseline v0.76.3 / f65f7b347ee4e7de6d98c488d3d894cd018b02b6 / go 1.25.5 / toolchain go1.25.12; snapshot commit 34d512541ca8047f8e3796abd6d85ef94cc13559; fixed Emulator target 127.0.0.1:10000; no PHYS_1_TARGET, no non-loopback target; default EVIDENCE_ROOT docs/evidence/raw
expected: full replay: stock Go 1.25.12 libgoprobe.so build + ELF verification + snapshot HAP build + dual-HAP install + aa test + directed HiLog + judgment + cleanup; stock loader rejection expected as measured blocked
actual: snapshot/baseline/host checks all passed; aborted at step 1 build.sh invocation with `spikes/r1-api24-hap/e1-stock-go-replay.sh: line 673: GO_PROBE_OUTPUT_DIR: readonly variable`; exit 1; no measurement, no manifest, no seal; Emulator not started; teardown cleanup completed
started_at: 2026-08-09T16:11:59+08:00
ended_at: approx 2026-08-09T16:12:00+08:00 (run aborted before section 10; ENDED_AT not printed in transcript)
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256: N/A; no HAP, native library or runtime artifact was produced (aborted before build)
raw_log_reference: docs/evidence/raw/EV-E1-EMU24-20260809-0001-transcript.log, docs/evidence/raw/EV-E1-EMU24-20260809-0001-go-build.log, docs/evidence/raw/EV-E1-EMU24-20260809-0001-baseline-verify.log (repository access; transcripts contain only the pre-abort output summarized below)
raw_log_sha256:
  transcript: e717c3778b8cba6b0246b5ceb35a65ec997fa1690ec0cf097ab8052d2d15f5b9
  go_build: 45eef9fbdb0404611102858c247af393fe9b361d7741c89818a8880dbe69a57b
  baseline_verify: 110c015277c1f9a5a7fcccf8e766b95a182a9ef24773d6bdc6d8bd34fee44849
verdict: fail (runner defect; aborted before any measurement; NOT a platform verdict)
reviewer: pending independent review
reviewed_at: pending
review_record: pending
```

## 失败分析（可复现）

runner 步骤 1 的实际调用（修复前）：

```bash
HARMONYOS_NATIVE_HOME="$NATIVE_HOME" GO_BIN="$GO_BIN" \
  GO_PROBE_OUTPUT_DIR="$GO_PROBE_OUTPUT_DIR" GO_TOOLCHAIN_MODE="$GO_TOOLCHAIN_MODE" \
  bash "$GO_PROBE_DIR/build.sh"
```

`GO_PROBE_OUTPUT_DIR` 在 runner 顶部声明为 `readonly`（`readonly GO_PROBE_OUTPUT_DIR="$PROJECT_DIR/entry/libs/x86_64"`）。bash 对 readonly 变量的前缀赋值（即使赋同值）直接报 `readonly variable` 并中止命令；同一位置的 `print_command` 打印的却是 `env HARMONYOS_NATIVE_HOME=… … bash build.sh`（`env` 是外部程序，不受 shell readonly 属性限制）。即**打印的命令与真实执行的命令不一致**：真实执行在环境赋值处中止，`build.sh` 未运行，`libgoprobe.so` 未产出（后续 `[[ -f "$GO_SO" ]] || fail` 也未到达）。

## transcript 关键行

```text
$ env HARMONYOS_NATIVE_HOME=… GO_BIN=… GO_PROBE_OUTPUT_DIR=… GO_TOOLCHAIN_MODE=… bash …/go-probe/build.sh
spikes/r1-api24-hap/e1-stock-go-replay.sh: line 673: GO_PROBE_OUTPUT_DIR: readonly variable
CLEANUP_BEGIN=teardown
CLEANUP_STAGING=skipped-emulator-not-started
CLEANUP_HDC=kill-issued
CLEANUP_TEMP=removed
CLEANUP_END=teardown-complete
```

（完整内容见 raw transcript；`go-build.log` 与 transcript 均只含上述 pre-abort 输出，因为失败发生在 `build.sh` 首次输出之前。）

## 修复与下一次 ID

- runner 已修复：`build.sh` 真实调用在环境赋值前补 `env`，与 `print_command` 输出一致；不改变测量、判定、guard、cleanup。
- `DEFAULT_EVIDENCE_ID` 已从 `EV-E1-EMU24-20260809-0001` 前移到 `EV-E1-EMU24-20260809-0002`；HOST preflight ID `EV-E1-EMU24HOST-20260809-0001` 不变。
- 下一次完整重放使用 `EV-E1-EMU24-20260809-0002`（默认值或显式 `EVIDENCE_ID`），且**不得**对 0001 同 ID 重跑。
- 0001 三份 raw 文件保持原样，供独立审查核对哈希（见上 `raw_log_sha256`）。

## 边界与后续

- 本记录不改变 E1 overall Go 状态：v0.76.3 官方 Go 1.25.12 loader/runtime 尚未产生任何测量、无 pass；E8 保持 `CLOSED`。
- 本记录 `record_status: collected`，待独立审查；`verdict: fail` 只表示 runner 层失败，不构成平台结论。
- E3 物理设备禁 HDC 约束不变；完整模式也只操作固定 Emulator target `127.0.0.1:10000`。
