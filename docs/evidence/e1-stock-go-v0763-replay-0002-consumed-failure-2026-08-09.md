# E1 v0.76.3 stock Go 重放 EV-E1-EMU24-20260809-0002 consumed-failure 记录（runner defect）

最后核验：2026-08-09

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `e1-stock-go-replay.sh` 完整模式第二次真实执行 `EV-E1-EMU24-20260809-0002` 的失败证据。该次执行在**任何平台测量之前**因 runner 脚本缺陷中止（`exit 1`），未产生平台判定；证据价值在于**失败本身可复现、可归因**，并据此修复 runner、把下一次正式 ID 前移到 `EV-E1-EMU24-20260809-0003`。

## 结论边界

- **ID 已消耗**：`EV-E1-EMU24-20260809-0002` 已被本次失败执行占用，**禁止同 ID 重跑**（no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit `2`）；下一次唯一正式 ID 为 `EV-E1-EMU24-20260809-0003`。
- **aborted-before-platform-measurement**：执行在步骤 2（ELF 验证）的 PT_TLS 检查处中止；未进行 HAP 构建、安装、`aa test`、HiLog 采集与判定，Emulator 未启动。stock Go `libgoprobe.so` 构建（步骤 1）已成功完成。
- **runner defect，不是平台结论**：失败根因是 runner 对 `readelf -lW` 输出做**字面量 `PT_TLS` grep**，而 readelf 的 Program Headers Type 列打印的是 `TLS`（不是 `PT_TLS`），导致对真实带 TLS program header 的 ELF 产生**假阴性**。实际 `libgoprobe.so` 具有完整 PT_TLS 语义（`TLS` program header 覆盖 `.tbss`、`-dW` 有 `STATIC_TLS`、`-rW` 有 `R_X86_64_TPOFF64`）。本次失败**不**证明也不否定 v0.76.3 官方 Go 1.25.12 loader/runtime 在 API 24 x86_64 Emulator 上的行为；E1 overall Go 仍为无测量、无 pass。
- **无 measured verdict / 无 manifest / 无 seal**：transcript 中没有 `MEASURED_VERDICT`、没有 `VERDICT` 行、没有 `manifest.txt`，因此也没有 `transcript_final_sha256` / `manifest_sha256` seal 行。
- **Emulator 未启动**：失败发生在 Emulator 冷启动之前，`emulator_started=0`；cleanup 记录 `CLEANUP_STAGING=skipped-emulator-not-started`，无 `CLEANUP_UNINSTALL`、无 `CLEANUP_EMULATOR`。
- **清理事实**：teardown 正常执行：`CLEANUP_BEGIN=teardown`、`CLEANUP_STAGING=skipped-emulator-not-started`、`CLEANUP_HDC=kill-issued`、`CLEANUP_TEMP=removed`、`CLEANUP_END=teardown-complete`；runner 在 ELF 验证阶段即中止，未触碰 Emulator/HDC 目标，无残留风险。
- **证据局限**：本记录只证明 0002 失败与 runner defect 的因果；三份 raw 文件保留原样（不改写、不删除、不重命名）；ELF 复核以带外 postmortem 文件 `EV-E1-EMU24-20260809-0002-elf-postmortem.log` 单独记录（formal run 之后生成，**不属于原运行输出**，不来自 formal transcript）；本记录 `record_status: collected`，待独立审查；不改变 E1/E8 状态，不升级 reviewed-pass。

## 证据记录

```yaml
evidence_id: EV-E1-EMU24-20260809-0002
information_status: current-measured
record_status: collected
stage_or_gate: E1
related_stages_or_gates: [E8, R1, R2]
execution: live-full-replay-aborted-before-platform-measurement
target_tuple:
  distribution: N/A; run aborted before guest runtime; no platform measurement produced
  device: Emulator instance netbird_api24_phone; NOT started during this run
  full_system_version: N/A; no guest system version measured
  architecture: N/A; host x86_64 Debian; target tuple remains API 24 x86_64 Emulator
  sdk_api_syscap: N/A; no SDK/API runtime surface exercised
  channel: N/A; no HAP, signing or distribution input
code_sha: f8a7b211259ab292079e934e05809a80324466af (repository HEAD at run time; the runner defect fix lands after this record)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (verified via gh API before abort, see baseline-verify log)
toolchain: Debian worker with Command Line Tools 6.1.1.290 / Beta 26.0.0.461 / Go 1.25.12 / ffmpeg / KVM Emulator; all HOST_CHECK passed before abort (hvigorw, ohpm, hvigor-ohos-plugin, emulator, hdc, go, ohos-x86_64-clang, ffmpeg, gh, bash; shellcheck absent)
working_directory: [WORKSPACE]/spikes/r1-api24-hap
command: bash spikes/r1-api24-hap/e1-stock-go-replay.sh
input: fixed baseline v0.76.3 / f65f7b347ee4e7de6d98c488d3d894cd018b02b6 / go 1.25.5 / toolchain go1.25.12; snapshot commit 34d512541ca8047f8e3796abd6d85ef94cc13559; fixed Emulator target 127.0.0.1:10000; no PHYS_1_TARGET, no non-loopback target; default EVIDENCE_ROOT docs/evidence/raw
expected: full replay: stock Go 1.25.12 libgoprobe.so build + ELF verification + snapshot HAP build + dual-HAP install + aa test + directed HiLog + judgment + cleanup; stock loader rejection expected as measured blocked
actual: snapshot/baseline/host checks all passed; stock Go build succeeded (GO_BUILD_VERDICT=pass, GO_VERSION_VERIFY=pass, GO_SO_ELF_VERIFY=pass); aborted at step 2 ELF verification with `FAIL_REASON=libgoprobe.so has no PT_TLS segment`; exit 1; no measurement, no manifest, no seal; Emulator not started; teardown cleanup completed
started_at: 2026-08-09T16:42:55+08:00
ended_at: approx 2026-08-09T16:43:00+08:00 (run aborted before section 10; ENDED_AT not printed in transcript)
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256:
  libgoprobe.so: 84bd840bc931849295beeeba6e94f2d9bd4273ffc3e7bcd896f477ccc00ac5f9 (built input; ELF verified below)
  app_hap: N/A; HAP build never ran (aborted at ELF verification)
  test_hap: N/A; HAP build never ran (aborted at ELF verification)
raw_log_reference: docs/evidence/raw/EV-E1-EMU24-20260809-0002-transcript.log, docs/evidence/raw/EV-E1-EMU24-20260809-0002-go-build.log, docs/evidence/raw/EV-E1-EMU24-20260809-0002-baseline-verify.log (repository access; transcripts contain only the pre-abort output summarized below); docs/evidence/raw/EV-E1-EMU24-20260809-0002-elf-postmortem.log (out-of-band postmortem generated after the formal run; NOT original run output)
raw_log_sha256:
  transcript: c7a874619cfb59547a138dfaeb649d634f2ac1db52f5a1ba116817196a2c1c6b
  go_build: a78d5f92aeb6c7142e782b671d58508fced35b35e82e810239321c0ebecd45bc
  baseline_verify: 110c015277c1f9a5a7fcccf8e766b95a182a9ef24773d6bdc6d8bd34fee44849
  elf_postmortem: 25f36261ab586753d915e8ebbc12fd073e7081cb1acda757d68a8cd3a28b225a
verdict: fail (runner defect; aborted before any platform measurement; NOT a platform verdict)
reviewer: pending independent review
reviewed_at: pending
review_record: pending
```

## 失败分析（可复现）

runner 步骤 2 的 PT_TLS 检查（修复前）：

```bash
readelf_l="$(readelf -lW "$GO_SO")"
printf '%s\n' "$readelf_l" | grep -E 'PT_TLS|LOAD' | head -10
if ! printf '%s\n' "$readelf_l" | grep -q 'PT_TLS'; then
  fail "libgoprobe.so has no PT_TLS segment"
fi
```

`readelf -lW` 的 Program Headers 表 Type 列打印的是 `TLS`（如 `TLS 0x132480 0x0000000000133480 … 0x000000 0x000008 R 0x8`），**从不出现字面量 `PT_TLS`**。因此对真实带 TLS program header 的 ELF，`grep -q 'PT_TLS'` 必然假阴性；诊断 grep `grep -E 'PT_TLS|LOAD'` 也只匹配到 4 行 `LOAD`（`TLS` 行不匹配 `PT_TLS` 模式），transcript 中可见的正是这 4 行 LOAD 后紧跟 `FAIL_REASON=libgoprobe.so has no PT_TLS segment`。

实际 ELF 的 PT_TLS 语义（带外 postmortem 复核，见 `EV-E1-EMU24-20260809-0002-elf-postmortem.log`；该文件是 formal run 之后的 out-of-band 复核记录，**不是原运行输出**，不来自 formal transcript）：

```text
$ readelf -lW libgoprobe.so        # Program Headers 表
  Type           Offset   VirtAddr           PhysAddr           FileSiz  MemSiz   Flg Align
  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x068f04 0x068f04 R   0x1000
  LOAD           0x068f20 0x0000000000069f20 0x0000000000069f20 0x0c9560 0x0c9560 R E 0x1000
  LOAD           0x132480 0x0000000000134480 0x0000000000134480 0x0d5400 0x0d5400 RW  0x1000
  LOAD           0x207880 0x000000000020a880 0x000000000020a880 0x007512 0x03c9e0 RW  0x1000
  TLS            0x132480 0x0000000000133480 0x0000000000133480 0x000000 0x000008 R   0x8
  …
 Section to Segment mapping:
  Segment Sections...
   05     .tbss

$ readelf -dW libgoprobe.so
 0x000000000000001e (FLAGS)              SYMBOLIC BIND_NOW STATIC_TLS
 0x000000006ffffffb (FLAGS_1)            Flags: NOW NODELETE

$ readelf -rW libgoprobe.so | grep -c R_X86_64_TPOFF64
1
```

即：`TLS` program header（覆盖 `.tbss`）、`STATIC_TLS` 动态标志、`R_X86_64_TPOFF64` 重定位三者齐备——**PT_TLS 语义存在，runner 的 `PT_TLS` 字面 grep 是假阴性**。`libgoprobe.so` SHA-256 `84bd840bc931849295beeeba6e94f2d9bd4273ffc3e7bcd896f477ccc00ac5f9` 由带外残留制品复算：formal run 中止后留在磁盘上的 `spikes/r1-api24-hap/entry/libs/x86_64/libgoprobe.so`（见 postmortem 的 `ARTIFACT_PATH`/`ARTIFACT_MTIME`）；go-build raw 仅证明构建成功（`GO_BUILD_VERDICT=pass`），不记录制品 hash。

## transcript 关键行

```text
$ env HARMONYOS_NATIVE_HOME=… GO_BIN=… GO_PROBE_OUTPUT_DIR=… GO_TOOLCHAIN_MODE=… bash …/go-probe/build.sh
built …/entry/libs/x86_64/libgoprobe.so with go version go1.25.12 linux/amd64
GO_BUILD_VERDICT=pass
GO_VERSION_VERIFY=pass version=go version go1.25.12 linux/amd64
GO_SO_ELF_VERIFY=pass
  LOAD           0x000000 0x0000000000000000 0x0000000000000000 0x068f04 0x068f04 R   0x1000
  LOAD           0x068f20 0x0000000000069f20 0x0000000000069f20 0x0c9560 0x0c9560 R E 0x1000
  LOAD           0x132480 0x0000000000134480 0x0000000000134480 0x0d5400 0x0d5400 RW  0x1000
  LOAD           0x207880 0x000000000020a880 0x000000000020a880 0x007512 0x03c9e0 RW  0x1000
FAIL_REASON=libgoprobe.so has no PT_TLS segment
CLEANUP_BEGIN=teardown
CLEANUP_STAGING=skipped-emulator-not-started
CLEANUP_HDC=kill-issued
CLEANUP_TEMP=removed
CLEANUP_END=teardown-complete
```

（完整内容见 raw transcript；`go-build.log` 只含 build.sh 成功输出，`baseline-verify.log` 只含 `BASELINE_VERIFY=pass mode=gh tag=v0.76.3 commit=f65f7b347ee4e7de6d98c488d3d894cd018b02b6`。）

## 修复与下一次 ID

- runner 已修复：新增可测试 helper `pt_tls_diag`，仅在 `Program Headers:` 到 `Section to Segment mapping:` 区间按首字段 `$1=="TLS"` 判断 PT_TLS，诊断输出按首字段 LOAD/TLS；保留 `GO_SO_PT_TLS_VERIFY=pass` 语义标签；`--selftest` 增加正例（真实 TLS program header）与反例（仅 section/tbss 文本，不得误判）。不改变测量、判定、guard、cleanup。
- `DEFAULT_EVIDENCE_ID` 已从 `EV-E1-EMU24-20260809-0002` 前移到 `EV-E1-EMU24-20260809-0003`；HOST preflight ID `EV-E1-EMU24HOST-20260809-0001` 不变。
- 下一次完整重放使用 `EV-E1-EMU24-20260809-0003`（默认值或显式 `EVIDENCE_ID`），且**不得**对 0002 同 ID 重跑。
- 0002 三份 raw 文件保持原样，供独立审查核对哈希（见上 `raw_log_sha256`）；ELF 复核内容以带外 postmortem 文件 `EV-E1-EMU24-20260809-0002-elf-postmortem.log` 单独记录（formal run 之后生成，不属于原运行输出），其 sha256 见上 `raw_log_sha256.elf_postmortem`。

## 边界与后续

- 本记录不改变 E1 overall Go 状态：v0.76.3 官方 Go 1.25.12 loader/runtime 尚未产生任何测量、无 pass；E8 保持 `CLOSED`。
- 本记录 `record_status: collected`，待独立审查；`verdict: fail` 只表示 runner 层失败，不构成平台结论。
- E3 物理设备禁 HDC 约束不变；完整模式也只操作固定 Emulator target `127.0.0.1:10000`。
