# E1 v0.76.3 stock Go loader/runtime host-preflight blocked 记录

最后核验：2026-08-09

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `e1-stock-go-replay.sh` 的 host-preflight 结果。当前 Windows 主机没有与历史 Linux worker 相同的 tuple（Emulator/Go/ffmpeg/SSH worker），因此本记录只覆盖 host 侧核验，不产生任何 runtime verdict，不形成平台结论。

## 结论边界

- 本记录是 **host-only 前置记录**：`execution: not-run-host-preflight`、`record_status: collected`、`verdict: blocked`。
- 它**不占用**后续 runtime evidence ID（`EV-E1-EMU24-20260809-0001` 及之后序号仍保留给恢复 Linux worker 后的实际重放）。
- 它**不形成平台结论**：不证明、也不否定 v0.76.3 官方 Go 1.25.12 loader/runtime 在 API 24 x86_64 Emulator 上的行为；E1 overall Go 仍为未重跑、无 pass。
- 本机没有 SSH alias、没有 WSL distro、没有 Go、没有 ffmpeg、没有与历史 worker 相同的 Linux Emulator；Windows DevEco Studio 自带 Emulator 与历史 Linux worker 的 Emulator **不等价**，不得作为替代运行环境。
- 旧 freeze 已被 `ADJ-20260809-0001`（正式采用 NetBird v0.76.3）superseded；E3 物理设备禁 HDC 的约束不变。

## 证据记录

```yaml
evidence_id: EV-E1-EMU24HOST-20260809-0001
information_status: current-measured
record_status: collected
stage_or_gate: E1
related_stages_or_gates: [E8, R1, R2]
execution: not-run-host-preflight
target_tuple:
  distribution: host-only preflight; no guest runtime executed
  device: no Emulator started; no physical device
  full_system_version: N/A; host-only record, no guest system version measured
  architecture: host x86_64 (Windows); target tuple remains API 24 x86_64 Emulator
  sdk_api_syscap: N/A; no SDK/API runtime surface exercised
  channel: N/A; no HAP, signing or distribution input
code_sha: b1d3a0102ee70c65ce675ee6febd1a72ed3e1e51 (current repository HEAD at preflight time)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (verified via gh API, see below)
toolchain: host Windows x86_64; git present; bash present; gh present; no Go, no ffmpeg, no Linux Emulator, no Beta HDC, no stable CLI, no shellcheck on this host
working_directory: [WORKSPACE]/spikes/r1-api24-hap
command: bash spikes/r1-api24-hap/e1-stock-go-replay.sh --preflight
input: fixed baseline v0.76.3 / f65f7b347ee4e7de6d98c488d3d894cd018b02b6 / go 1.25.5 / toolchain go1.25.12; snapshot commit 34d512541ca8047f8e3796abd6d85ef94cc13559; fixed Emulator target 127.0.0.1:10000; no PHYS_1_TARGET, no non-loopback target
expected: verify host paths, versions, the 34d5125 runGoProbe source snapshot and current repository state without starting an Emulator or running HDC; record blocked because the host lacks the same-tuple worker
actual: snapshot source verification passed (probe.cpp contains runGoProbe and GO_SPIKE_RESULT; TestRunner contains runGoProbe/GO_SPIKE_RESULT/BASELINE_RESULT; index.d.ts contains runGoProbe); baseline verification passed via gh (tag v0.76.3 -> commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6; go.mod contains go 1.25.5 and toolchain go1.25.12); host tool checks failed for hvigorw, ohpm, hvigor-ohos-plugin, Emulator, Beta HDC, Go, OHOS x86_64 clang and ffmpeg (8 missing); gh and bash present; shellcheck absent (bash -n fallback used)
started_at: 2026-08-09T14:37:14+08:00
ended_at: 2026-08-09T14:37:14+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256: N/A; no HAP, native library or runtime artifact was produced by this host-only preflight
raw_log_reference: docs/evidence/raw/EV-E1-EMU24HOST-20260809-0001-transcript.log, repository access; transcript contains only the preflight output summary below
raw_log_sha256: 1d6e4a67be08607308107271228ce84cc0f6a9820ea48eb03fa8418f11603d59
verdict: blocked
reviewer: pending independent review
reviewed_at: pending
review_record: pending
```

## 本机 host 状态（当前实测）

| 检查项 | 结果 | 说明 |
| --- | --- | --- |
| SSH alias | 无 | `~/.ssh/config` 无 host 条目；无历史 Linux worker 的 SSH 入口 |
| WSL distro | 无 | `wsl -l -v` 提示未安装任何发行版 |
| Go | 无 | `go` 不在 PATH，`/home/worker/go/bin/go` 不存在 |
| ffmpeg | 无 | `ffmpeg` 不在 PATH |
| 同 tuple Emulator | 无 | 历史 worker 的 Beta Command Line Tools 26.0.0.461 Emulator 路径不存在 |
| Beta HDC | 无 | 历史 worker 的 Beta HDC 3.2.0e 路径不存在 |
| 稳定 CLI | 无 | 历史 worker 的 Command Line Tools 6.1.1.290 hvigorw/ohpm 路径不存在 |
| gh | 有 | 用于 v0.76.3 tag/commit/go.mod 在线核验 |
| bash | 有 | `bash -n` 语法检查通过 |
| shellcheck | 无 | 不可用，按设计回退到 `bash -n` |

Windows DevEco Studio 6.1 自带 `hdc.exe` 与 Windows 版 Emulator，但它们是 Windows 宿主工具，与历史 Linux worker 的 Debian 13 + Beta Command Line Tools + KVM Emulator 环境**不等价**，不能用于本 runner 的完整重放，也不得作为平台结论输入。

## preflight 命令输出摘要

以下为 `bash spikes/r1-api24-hap/e1-stock-go-replay.sh --preflight` 的关键输出（完整 transcript 见 raw 引用；路径已按仓库相对形式脱敏，不含本机个人绝对路径）：

```text
RUNNER=e1-stock-go-replay.sh
BASELINE_TAG=v0.76.3
BASELINE_COMMIT=f65f7b347ee4e7de6d98c488d3d894cd018b02b6
BASELINE_GO_DIRECTIVE=go 1.25.5
BASELINE_TOOLCHAIN_DIRECTIVE=toolchain go1.25.12
SNAPSHOT_COMMIT=34d512541ca8047f8e3796abd6d85ef94cc13559
TARGET_TUPLE=HarmonyOS_6.1.1(24),API24,x86_64,phone_Emulator
EMULATOR_TARGET=127.0.0.1:10000
PHYSICAL_DEVICE_USED=false
HDC_RUN=false
GIT_HEAD=b1d3a0102ee70c65ce675ee6febd1a72ed3e1e51
SNAPSHOT_COMMIT_PRESENT=pass
SNAPSHOT_SOURCE_VERIFY=pass
BASELINE_VERIFY=pass mode=gh tag=v0.76.3 commit=f65f7b347ee4e7de6d98c488d3d894cd018b02b6
HOST_CHECK hvigorw=fail
HOST_CHECK ohpm=fail
HOST_CHECK hvigor-ohos-plugin=fail
HOST_CHECK emulator=fail
HOST_CHECK hdc=fail
HOST_CHECK go=fail
HOST_CHECK ohos-x86_64-clang=fail
HOST_CHECK ffmpeg=fail
HOST_CHECK gh=pass
HOST_CHECK bash=pass
HOST_CHECK shellcheck=fail (bash -n fallback will be used)
EXECUTION=not-run-host-preflight
RECORD_STATUS=collected
VERDICT=blocked
HOST_PREFLIGHT_VERDICT=blocked
HOST_PREFLIGHT_REASON=host lacks the same-tuple Emulator/Go/ffmpeg/SSH worker; no runtime verdict is produced
HOST_PREFLIGHT_MISSING_COUNT=8
```

## 可续跑入口

恢复历史 Linux worker（Debian 13 + Command Line Tools 6.1.1.290 + Beta 26.0.0.461 + Go 1.25.12 + ffmpeg + KVM Emulator）后，在仓库根执行单条命令即可完成完整重放：

```bash
bash spikes/r1-api24-hap/e1-stock-go-replay.sh
```

runner 固定输入：baseline `v0.76.3` / `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` / `go 1.25.5` / `toolchain go1.25.12`；runGoProbe 源码快照 `34d512541ca8047f8e3796abd6d85ef94cc13559`（git archive 提取到临时目录构建，不改当前 C-only 探针源码）；仅操作固定 Emulator target `127.0.0.1:10000`；明确拒绝 `PHYS_1_TARGET` 或任何非 127.0.0.1 emulator target。完整模式执行 stock Go 1.25.12 `libgoprobe.so` 构建与 ELF 验证（Go 1.25.12、ELF x86_64、`PT_TLS`/`R_X86_64_TPOFF64`/`STATIC_TLS`、hash）、快照 HAP clean build（app + test，验证两者内 `libgoprobe.so` byte-equal）、双 HAP 安装、`aa test`、定向 HiLog 采集、判定与清理，生成 transcript/manifest。stock loader rejection（`initial-exec TLS resolves to dynamic definition`）按设计登记为 **measured blocked**，不是 runner failure；具体判定以 `BASELINE_RESULT` + `GO_SPIKE_RESULT` 为准。

## 边界与后续

- 本记录不改变 E1 overall Go 状态：v0.76.3 官方 Go 1.25.12 loader/runtime 尚未重跑、无 pass；E8 保持 `CLOSED`。
- 本记录不占用 runtime evidence ID；恢复 worker 后的实际重放使用 `EV-E1-EMU24-20260809-0001`（或按当时日期/序号重新分配）。
- 旧 freeze（v0.74.6/v0.74.7 相关历史绑定）已被 `ADJ-20260809-0001` superseded；历史 evidence 保持原样，不改写、不重判。
- E3 物理设备禁 HDC 约束不变；本 runner 不运行 HDC（preflight 模式 `HDC_RUN=false`），完整模式也只操作 Emulator target。
- 本记录为 host-only，`record_status: collected`，待独立审查；`verdict: blocked` 只表示当前 host 无法产生 runtime verdict。
