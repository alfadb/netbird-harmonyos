# E1 C-only ArkTS/native/fd Emulator 子证据

最后核验：2026-07-17

本文记录普通第三方 `EntryAbility` 在 API 24 x86_64 phone Emulator 上执行的 E1 C-only 子门。它验证 ArkTS/native 同步 buffer、C pthread 到 ArkTS 的公开 Node-API threadsafe 回调和真实 fd ownership，但没有使用 Go、NetBird、PS4、TestRunner、VPN 或真机。

## 证据记录

```yaml
evidence_id: EV-E1-EMU24-20260717-0005
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E1
related_stages_or_gates: [E8, R1, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; packaged arm64 member was not executed
  sdk_api_syscap: SDK 6.1.1.125/API 24 ordinary UIAbility, ArkUI, HiLog, Node-API threadsafe function, pthread and libc fd surface; VPN SysCap not exercised
  channel: N/A; unsigned debug research HAP accepted only by this Emulator
code_sha: Git baseline 470fc466192759f8cb51efe955e2293db93addf5 plus uncommitted measured source archive SHA-256 44afc22e3918c6273d634347f16cd118070eb96533a8d948c026df58e26e6313
upstream_sha: N/A; no Go or NetBird code participated in the measured path
toolchain: Debian GNU/Linux 13 x86_64 host; Command Line Tools 6.1.1.290 with bundled Node.js 18.20.1; SDK/Native 6.1.1.125 API 24; Hvigor 6.24.3; ohpm 6.1.2.285; host Node.js 24.15.0 recorded but not selected by the stable wrappers; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; DISPLAY=:1; KVM enabled
working_directory: /home/worker/work/base/netbird-harmonyos
command: bash spikes/r1-api24-hap/e1-c-emulator-run.sh; the runner enters spikes/r1-api24-hap for clean Hvigor build and uses explicit Beta HDC target 127.0.0.1:10000 for every guest operation
input: one clean-built unsigned application HAP SHA-256 27f827ea7997759c65ec3a10f1e3fdcc728e918589000b66f3f5e798f9e273b9; no test HAP; no initially installed bundle, Emulator process or tested port listener; guest HiLog buffers set and queried at 16 MiB before measurement
expected: three distinct ordinary EntryAbility cold-start PIDs; each PID completes 10 full rounds, each round has 100 guarded synchronous buffer calls, 100 ordered pthread callbacks consumed on the main ArkTS thread, one real socketpair ownership transfer with dup/write/read/close/EBADF checks, and equal pre/post fd and thread snapshots; visible PASS page, complete unfiltered HiLog, then force-stop, uninstall and zero host residue
actual: PIDs 3148, 3243 and 3405 each passed 10 rounds with exactly 1000 buffer samples, 1000 valid callbacks, 10 fd transfers and 10 joined producer pthreads; all 30 measured resource pairs were fd 38->38 and threads 28->28, every final snapshot was 38/28, screenshots visibly showed the PID-specific E1 C-only PASS result, and uninstall/host cleanup passed
started_at: 2026-07-17T17:55:33+08:00
ended_at: 2026-07-17T17:57:36+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, guest HiLog timestamps, HAP mtime and current qemu boot-complete log segment
artifact_sha256:
  application_hap_measured: 27f827ea7997759c65ec3a10f1e3fdcc728e918589000b66f3f5e798f9e273b9
  application_x86_64_libprobe_member: caed38130bef9987fff2f5c7be9281675aeeb52d2a3fef7c402d4b6a70c69fca
  target_sdk_native_api_header: c7d241f14ecd4d3e7c9b9ae34eff1f5da40bec53686717870227285b2e97f56a
  measured_source_manifest: 1631efbd5bea2c64c525ebc10c268fee9a61a7aa372409da9cd8f625cc11c7ac
  measured_source_archive: 44afc22e3918c6273d634347f16cd118070eb96533a8d948c026df58e26e6313
raw_log_reference: repository-controlled raw/EV-E1-EMU24-20260717-0005-*; transcript c4193f059e73192317034e36cbaa83882d7f3a9c9b796782701a97466078ba94, tag HiLog bec2d6eed2b7a523611143ecb0b6ee09ee650005ab097426d260f3dcebad8503, three unfiltered HiLogs and screenshots listed below, Emulator console f061ca13ebf0a0887ee062732d3ab3638e1255619d7e7f0eb15b047c927dfeb6; repository access
verdict: pass
reviewer: deepseek/deepseek-v4-pro independent E1 C-only evidence review
reviewed_at: 2026-07-17T18:06:00+08:00
review_record: Independent review verified the recorded hashes and measured HAP identity, TSFN warm-up and callback ordering, pthread join/identity, guarded buffer assertions, fd ownership/dup/write/read/close/EBADF checks, three distinct PIDs, and cleanup; 0 blocker/major findings and 5 minor findings.
```

本记录是 E1 的 C-only 子证据，现已取得 `reviewed-pass`。它没有运行最新正式 NetBird release 声明的官方 Go 1.25.12 制品；该官方制品的已知 initial-exec TLS loader 路径仍失败。因此 E1 overall Go blocked，E8 保持 `CLOSED`，真机执行仍被禁止。后续 E2 C 网络记录 `EV-E2-EMU24-20260717-0002` 已为 `reviewed-pass/pass` 并关闭；E3 的 `0003`/`0004` 均为 `reviewed-pass/blocked`，E3 不关闭；E4-E7 为 dependency blocked、不开始。R0/R1/R2 均未退出。

## 审查附录

| 项目 | 记录 |
| --- | --- |
| M1 | measured source/archive、runner 与当前工作树关系已披露；不改变 measured artifact，后续 integrated replay 考虑。 |
| M2 | measured HAP 及其 ELF member、raw transcript、HiLog、截图哈希已逐项可追溯；不改变 measured artifact，后续 integrated replay 考虑。 |
| M3 | TSFN 一次性 warm-up 与正式 10 轮测量基线已区分；不改变 measured artifact，后续 integrated replay 考虑。 |
| M4 | fd 编号复用、`/proc/self/fd`/线程快照和三 PID 范围已明确；不改变 measured artifact，后续 integrated replay 考虑。 |
| M5 | 旧 `appspawn` crash 文件的时间和归属边界已说明；不改变 measured artifact，后续 integrated replay 考虑。 |

上述 5 项均为 minor 记录项，无 blocker/major；它们不改变本次测量、原始材料或判定。

## 实现与判定

目标 SDK 的公开 `napi/native_api.h` 将 `napi_create_threadsafe_function`、`napi_call_threadsafe_function`、`napi_acquire_threadsafe_function` 和 `napi_release_threadsafe_function` 标为 `@since 10`。最终 x86_64 ELF 真实引用这些符号以及 `pthread_create`、`socketpair`、`dup` 和 `close`；`DT_NEEDED` 只有 `libace_napi.z.so`、`libhilog_ndk.z.so`、`libc++_shared.so` 和 `libc.so`。

| 子项 | 每个 PID 的实测 | 三 PID 合计 | 判定 |
| --- | ---: | ---: | --- |
| 同步 buffer | 10 轮 x 100 个长度 `0..99` 的 guarded `Uint8Array` | 3000 次 | 确定 FNV-1a、长度/首尾字节一致，左右哨兵均未改变 |
| C 到 ArkTS 回调 | 10 个新 pthread，各 100 条 | 3000 条 | 每轮序号严格 `1..100`，payload/hash 一致，consumer TID 等于主 ArkTS TID，producer TID 不同 |
| fd ownership | 10 个真实 socketpair | 30 次 | ArkTS 接收 `37/38` 后传回 C；C `dup` 为 `39`，完成 write/read，关闭三 fd，重复关闭两原 fd 均为 `-1/EBADF(9)` |
| 资源快照 | 每轮前后各一次 | 30 对 | 每对均为 fd `38->38`、thread `28->28`，无逐轮单调增加，最终仍为 `38/28` |
| 普通应用 | 10 轮结束后保持存活并截图 | PID `3148/3243/3405` | 三个不同冷启动 PID，均显示 `E1 C-only PASS` |
| TestRunner | 未调用且 application HAP 无 test member | 0 | 仅普通 `EntryAbility` 构成主证据 |

TSFN 在第一次实际投递时会一次性建立 runtime 资源。探针显式执行并记录 `round=0` 预热，三个 PID 均从 fd/thread `37/27` 稳定到 `38/28`；预热的 100 条回调全部由 ArkTS 校验并 join producer，但不计入正式 10 轮。测量基线只在这个公开机制完成初始化后建立，随后 30 对测量快照和最终快照均无增长。

## 线程、fd 与 hash

每个 PID 的 callback consumer 都是对应主线程 `3148`、`3243`、`3405`。正式 producer TID 为：

- PID 3148：`3293,3295,3297,3303,3306,3307,3308,3309,3310,3311`
- PID 3243：`3443,3444,3445,3446,3447,3448,3449,3450,3451,3453`
- PID 3405：`3632,3633,3634,3635,3636,3637,3638,3639,3640,3641`

三个 PID 每轮都重用当时最低可用 fd `37/38/39`，但每轮都先验证 fd 有效、完成所有权转移并全部关闭，下一轮只能在前一轮无泄漏时取得相同编号。逐轮聚合 hash 在三个 PID 中完全一致：

| 轮 | buffer aggregate | callback aggregate | fd payload hash |
| ---: | ---: | ---: | ---: |
| 1 | 1003354277 | 1107886077 | 1745844509 |
| 2 | 1954082213 | 418017168 | 4098012082 |
| 3 | 94298481 | 989334770 | 416398542 |
| 4 | 1443504017 | 2198459304 | 1382337890 |
| 5 | 3976353045 | 3347215007 | 2416054757 |
| 6 | 1719213877 | 2234265595 | 1414031522 |
| 7 | 1306351913 | 884704170 | 3381443963 |
| 8 | 16833241 | 2374469709 | 3915913625 |
| 9 | 1774892357 | 3084542708 | 607522204 |
| 10 | 3360330837 | 1760219668 | 956208151 |

逐样本长度、哨兵、payload、线程身份和 hash 均保留在 tag HiLog，不以聚合值替代原始断言。

## 完整日志边界

每次运行前清空 guest HiLog，并在 16 MiB 缓冲下采集无 tag/type 过滤的完整日志：run1 `6268` 行、run2 `4505` 行、run3 `4444` 行。run2 日志在 17:57:15 处理了持久化文件 `CPP_CRASH1784278460620`，输出名为 `cppcrash-appspawn-0-20260717165420620.log`；它对应本记录开始前约一小时的 `appspawn` PID 188 旧事件，不是三个应用 PID，也晚于 PID 3243 的 E1 PASS 后才被 faultlogger 扫描。三个被测 PID 在三份日志中均无对应 fault/crash/freeze 记录，并保持存活到截图。

## 原始材料

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `raw/EV-E1-EMU24-20260717-0005-application-hap.bin` | 2658993 | `27f827ea7997759c65ec3a10f1e3fdcc728e918589000b66f3f5e798f9e273b9` |
| `raw/EV-E1-EMU24-20260717-0005-libprobe-x86_64.bin` | 21904 | `caed38130bef9987fff2f5c7be9281675aeeb52d2a3fef7c402d4b6a70c69fca` |
| `raw/EV-E1-EMU24-20260717-0005-transcript.log` | 39574 | `c4193f059e73192317034e36cbaa83882d7f3a9c9b796782701a97466078ba94` |
| `raw/EV-E1-EMU24-20260717-0005-build.log` | 3011 | `7fb396cc33b14ba187a8d14d3423fd133936c1ec8baefb5e3f45bdc7b1d06ece` |
| `raw/EV-E1-EMU24-20260717-0005-source-manifest.txt` | 1066 | `1631efbd5bea2c64c525ebc10c268fee9a61a7aa372409da9cd8f625cc11c7ac` |
| `raw/EV-E1-EMU24-20260717-0005-source.tar` | 71680 | `44afc22e3918c6273d634347f16cd118070eb96533a8d948c026df58e26e6313` |
| `raw/EV-E1-EMU24-20260717-0005-hilog-tag.log` | 1196955 | `bec2d6eed2b7a523611143ecb0b6ee09ee650005ab097426d260f3dcebad8503` |
| `raw/EV-E1-EMU24-20260717-0005-run1-hilog-full.log` | 949412 | `6bae1724da53c05aff9d3689bbafc612b5731b3992e7139c32cdf0489127db1d` |
| `raw/EV-E1-EMU24-20260717-0005-run2-hilog-full.log` | 714161 | `22c9114542bcb82153429332da010c1689c54fc8012041da9500964656f7dc61` |
| `raw/EV-E1-EMU24-20260717-0005-run3-hilog-full.log` | 709314 | `f964925ea7471c2b86fe385a7cd5e4835bbe1c415a8bec2a4e2bca3a907dd884` |
| `raw/EV-E1-EMU24-20260717-0005-emulator-console.log` | 1861 | `f061ca13ebf0a0887ee062732d3ab3638e1255619d7e7f0eb15b047c927dfeb6` |
| `raw/EV-E1-EMU24-20260717-0005-run1.png` | 133896 | `2dba01da4ec4310dd7b85ef0629e2e38bd4739fd090179d8d618873422d357a5` |
| `raw/EV-E1-EMU24-20260717-0005-run2.png` | 134357 | `31d41c90d4bf5ff765c74a37636b2c59ca8575deb9bac984a472af05b670d8cd` |
| `raw/EV-E1-EMU24-20260717-0005-run3.png` | 133414 | `dd465bdc5a1633c2fb83a142fe51e13288f805f91bef1de7746209f909ad52b8` |

## 前置尝试

同日 `0001` 至 `0004` 的 transcript 按 ID 不复用规则保留，但不能替代 `0005`：`0001` 在 Emulator 启动前因 runner 构建目录错误而阻塞；`0002` 以预热前快照为基线，正确拒绝了 TSFN 首次投递的一次性资源建立；`0003` 和 `0004` 的应用完成 PASS，但默认 HiLog 环形缓冲覆盖早期样本，证据完整性门拒绝。四份 transcript SHA-256 依次为 `ba3dc7e013f63d3418db0a7d817e4b7241e326ff10be976601f9371364b405c3`、`4ca2925d3efcf4e561cbf4700559f74cc4fee9e6d12188e07a051c15154a8214`、`611044ff2adbd05b0d9bad18965ea5cdd12a39e095d754500bdf9b8862980946` 和 `0ecc668e4630f2ee4410289b4f4bd71340a9f5d6fd8bdb7cb40ff8c0f37c58cc`。`0005` 同时修正稳定基线和 16 MiB 完整采集，不改写这些前置尝试。
