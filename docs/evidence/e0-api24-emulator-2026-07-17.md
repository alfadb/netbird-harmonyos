# E0 API 24 Emulator 普通应用证据

最后核验：2026-07-17

本文按[证据与脱敏 Schema](../evidence-schema.md)记录 API 24 x86_64 phone Emulator 上普通第三方 Stage 应用的构建、安装、`EntryAbility` 启动、可观察运行、停止、卸载与主机清理。该结论仅覆盖下述精确目标元组和 unsigned 研究 HAP，不外推 arm64、具名真机、华为商用 HarmonyOS、Go、NetBird、VPN、签名、渠道或产品支持。

## 证据记录

```yaml
evidence_id: EV-E0-EMU24-20260717-0001
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E0
related_stages_or_gates: [E8, R0, R1]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; arm64 library was packaged but not executed
  sdk_api_syscap: SDK 6.1.1.125/API 24 ordinary UIAbility, ArkUI, HiLog and Node-API runtime surface; VPN SysCap not exercised
  channel: N/A; unsigned debug research HAP accepted only by this Emulator
code_sha: baseline Git commit e35e4f475c4c43465278c8c33555741e56182f83 plus measured uncommitted source-and-driver manifest SHA-256 54dcdd179c4276a0fe57dfd3a273ba6b8f6bc163b3a3a823d36a09738c8ec41b
upstream_sha: N/A; the measured HAP contains no Go or NetBird code
toolchain: Debian GNU/Linux 13 x86_64 host; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; Node.js 18.20.1; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; DISPLAY=:1; KVM enabled
working_directory: /home/worker/work/base/netbird-harmonyos/spikes/r1-api24-hap for build and /home/worker/work/base/netbird-harmonyos for runtime
command: Clean build commands and complete runtime commands are retained in the referenced build log, source manifest and replay script; every target operation used Beta HDC with explicit target 127.0.0.1:10000
input: application HAP SHA-256 e1724ef064043ed22e0e2ea73c7a53acc8bd80272c5fb14d5b666aeeb8bb70fd and test HAP SHA-256 891f26095125c6c6641cf298cafc2a8a29dcbaa0408e60afd60e579c469e2df1; no initial bundle, Emulator process or tested port listener
expected: clean-build both unsigned HAPs; install them together; unlock without credentials; complete three fresh normal EntryAbility cold starts with distinct PIDs, lifecycle and Node-API markers, visible screenshots, then stop, run a non-substituting sidecar baseline, uninstall and remove all Emulator/HDC process and port residue
actual: both HAP builds succeeded; install and ordinary-app audit passed; EntryAbility cold-started three times as PIDs 2907, 2966 and 3123; every run returned ping=pong and version r1-api24-probe/0.0.1, rendered the expected UI and emitted onCreate/windowStage/foreground markers; sidecar baseline returned result code 0 after the normal app runs; force-stop, uninstall, package-absence and host cleanup passed
started_at: 2026-07-17T16:54:08+08:00
ended_at: 2026-07-17T16:55:22+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, guest HiLog timestamps, HAP mtimes and Emulator qemu boot-complete log
artifact_sha256:
  application_hap_measured: e1724ef064043ed22e0e2ea73c7a53acc8bd80272c5fb14d5b666aeeb8bb70fd
  test_hap_measured: 891f26095125c6c6641cf298cafc2a8a29dcbaa0408e60afd60e579c469e2df1
  application_arm64_libprobe_member_not_executed: 88c577add169ea1dd76d70521aa161278fc02557dce2d55377c0c9cfddb00fdf
  application_x86_64_libprobe_member: ddfce95e894f8e67f6e1c168b5713b99a58aa6f8f2280bce7eaf5d8901d3609a
  application_module_json: 24fb121084e7d8c473bad2487d62bf03f6d6a1d35218c8369faf50aa240892ef
  test_module_json: 3f96d7bdcb1794c9b89b9e409f5b7cdaf348d85f06fe5204f7a6b89632ff45cb
  measured_source_and_driver_manifest: 54dcdd179c4276a0fe57dfd3a273ba6b8f6bc163b3a3a823d36a09738c8ec41b
raw_log_reference: Immutable application/test HAP archives retain the measured artifact hashes; sanitized replay transcript SHA-256 0c25c9e953d2acc93180cb5d161b185392264453e19eee63318966a17db10e49, full app HiLog 1b00e7cf051788f004281933b1f5cb803f8422c133853f5be776fa19ca4a7d53, tag HiLog 12b4a1af1218636f3d49d64a1a254c3b663606ebe1b32fe0d503ccd123eec88e, Emulator console dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e, source manifest 54dcdd179c4276a0fe57dfd3a273ba6b8f6bc163b3a3a823d36a09738c8ec41b, screenshots as listed below, repository access; post-measurement clean-build log 467ca7cde1829612e1a8849d2c3dc4173d27baf7736adbb9a8561d9920a4c310 is supporting build evidence and is not the measured HAP identity
verdict: pass
reviewer: xai/grok-4.5 independent E0 evidence review
reviewed_at: 2026-07-17T17:25:00+08:00
review_record: Independent review verified the immutable measured HAP hashes, three distinct ordinary EntryAbility runs, lifecycle/Node-API/UI/cleanup evidence, and evidence boundaries; five MINOR findings are handled, the post-measurement runner hash differs from the measured driver and is disclosed, and no Go, VPN, E1-E8, R-stage, arm64, or device claim is accepted.
```

独立审查已将本记录更新为 `reviewed-pass`：功能判据与原始材料边界均已核验，因此 E0 已关闭并通过。E8 仍为 `CLOSED`，真机执行仍被禁止；下一执行门为 E1，且本记录不构成 Go、VPN、正式 R 阶段、arm64 或真机结论。

## 实现隔离

E0 版本把历史 Go/TLS loader 研究输入从当前构建图移除，并新增最小 `e0_probe.cpp`。当前 HAP 只导出 `ping()` 与 `version()`，其 x86_64 ELF 仅依赖 `libace_napi.z.so`、`libhilog_ndk.z.so`、`libc++_shared.so` 和 `libc.so`；没有 `libdl.so`、`libgoprobe.so`、`libtls-*`、Go runtime、NetBird 或 VPN 代码。应用模块的 `bm dump` 现场审计确认：

- `appPrivilegeLevel: normal`
- `isSystemApp: false`
- `mainElementName: EntryAbility`
- 普通 `entry` 模块与独立 `entry_test` 模块同时安装

`entry_test` 只在三次普通应用启动、截图和 force-stop 完成后运行，输出 `TEST_HAP_SIDECAR=pass_non_substitute`；它不能替代 E0 的普通 `EntryAbility` 证据。

## 判定明细

| 子项 | 当前实测 | 判定 |
| --- | --- | --- |
| clean build | `entry@default` 与 `entry@ohosTest` 均 `BUILD SUCCESSFUL`；唯一预期警告是没有 signing config | PASS |
| HAP 隔离 | 包内没有 `libgoprobe` 或 `libtls-*`；application/test module JSON 分离 | PASS |
| 连接与 readiness | Beta HDC shell 和 HarmonyOS distribution 通过；qemu 当前启动段出现 `guest os boot completed.` | PASS |
| 无凭据解锁 | `power-shell wakeup`、Home、`swipe 660 2500 660 500 2000` 均成功 | PASS |
| 安装 | 两份 HAP 同目录安装返回 `install bundle successfully.` | PASS |
| 冷启动 1 | 启动成功；PID 2907；Node-API marker 到达 | PASS |
| 冷启动 2 | force-stop 后 PID 2966；与 PID 2907 不同；累计 Node-API marker 2 | PASS |
| 冷启动 3 | force-stop 后 PID 3123；与前两者不同；累计 Node-API marker 3 | PASS |
| 生命周期 | `onCreate observed=true`、`onWindowStageCreate`、`onForeground`、Node-API result 各 3 条 | PASS |
| 可见 UI | 三张 1320x2856 PNG；像素 `(100,1000)` 均为 `242 246 248`；YAVG 均为 `225.599`；人工核对显示 `E0 API 24` 与 `EntryAbility running` | PASS |
| 停止 | 最终 force-stop 后 `pidof` 为空 | PASS |
| sidecar baseline | `BASELINE_RESULT\|functional=PASS\|ping=pong\|version=r1-api24-probe/0.0.1`，`TestFinished-ResultCode: 0` | PASS，非 E0 替代证据 |
| 卸载 | uninstall 成功，随后 `bm dump` 返回包不存在语义 | PASS |
| 主机清理 | 无 Emulator/qemu/HDC server 残留；10000、5555、8710 无监听 | PASS |

三次截图内容和哈希完全相同，说明三个新进程渲染到同一稳定页面，而不是说明只复用了一次进程；不同 PID 与每次累计 marker 计数提供独立的冷启动证据。

## 原始材料

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `raw/EV-E0-EMU24-20260717-0001-application-hap.bin` | 2587516 bytes | `e1724ef064043ed22e0e2ea73c7a53acc8bd80272c5fb14d5b666aeeb8bb70fd` |
| `raw/EV-E0-EMU24-20260717-0001-test-hap.bin` | 10754 bytes | `891f26095125c6c6641cf298cafc2a8a29dcbaa0408e60afd60e579c469e2df1` |
| `raw/EV-E0-EMU24-20260717-0001-transcript.log` | 224139 bytes | `0c25c9e953d2acc93180cb5d161b185392264453e19eee63318966a17db10e49` |
| `raw/EV-E0-EMU24-20260717-0001-hilog-app-full.log` | 371856 bytes | `1b00e7cf051788f004281933b1f5cb803f8422c133853f5be776fa19ca4a7d53` |
| `raw/EV-E0-EMU24-20260717-0001-hilog-tag.log` | 2586 bytes | `12b4a1af1218636f3d49d64a1a254c3b663606ebe1b32fe0d503ccd123eec88e` |
| `raw/EV-E0-EMU24-20260717-0001-emulator-console.log` | 1822 bytes | `dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e` |
| `raw/EV-E0-EMU24-20260717-0001-source-manifest.txt` | 1638 bytes | `54dcdd179c4276a0fe57dfd3a273ba6b8f6bc163b3a3a823d36a09738c8ec41b` |
| `raw/EV-E0-EMU24-20260717-0001-build.log` | 5945 bytes | `467ca7cde1829612e1a8849d2c3dc4173d27baf7736adbb9a8561d9920a4c310` |
| `raw/EV-E0-EMU24-20260717-0001-run1.png` | 104480 bytes | `88dce9447b083199f67af4f5acf4016f2de36056b0e69ab61773188f08f2ae54` |
| `raw/EV-E0-EMU24-20260717-0001-run2.png` | 104480 bytes | `88dce9447b083199f67af4f5acf4016f2de36056b0e69ab61773188f08f2ae54` |
| `raw/EV-E0-EMU24-20260717-0001-run3.png` | 104480 bytes | `88dce9447b083199f67af4f5acf4016f2de36056b0e69ab61773188f08f2ae54` |

归档 transcript 只对 `bm dump` 的两个运行时 access-token 标识做稳定占位替换，未改变行数、时间顺序、错误码、PID、计数或判定语义。现场捕获原 SHA-256 为 `e1930b7238fb739ee95f81757df156404a1de9a54dd2d76ace62c47d9d834755`；归档后的可复核 SHA-256 以上表为准。当前重放脚本在采集源处执行同一字段级脱敏，并以逐 PID 计数收紧即时 Node-API gate；其后测量修订 SHA-256 为 `998691504b3cf87de6319db8b95fe5e57f0b58e760f3dbc7d1afdca717132913`。该当前脚本不是实际测量脚本，实际测量驱动脚本哈希保留在 source manifest 中；二者差异不改写原始功能证据，归档 tag log 已逐 PID 复核并满足收紧后的条件。

## 独立审查处理

- `MINOR-1`（已处理）：未来重放将安装移动至 qemu boot-complete 与 HDC shell readiness 共同确认之后。
- `MINOR-2`（已处理）：每次 `aa start` 除现有负向错误门外还必须精确匹配 `start ability successfully.`。
- `MINOR-3`（已处理）：当前后测量重放脚本哈希与实际测量驱动脚本哈希分别披露，不把当前脚本表述为测量时脚本。
- `MINOR-4`（已处理）：当前 CMake 未引用的历史 0010 `probe.cpp` 已从当前源树删除；历史复现必须 checkout 相应提交，不能以旧源码作当前 fallback。
- `MINOR-5`（已处理）：状态文档已将 E0 记录为 `reviewed-pass` 和关闭，同时保留 E8 `CLOSED`、真机禁令及 Go/VPN/E1-E8/正式阶段未通过的边界。

审查只接受本记录的 immutable HAP、日志、截图和所列功能事实；它不将后测量 runner 变更、C-only 证据或历史 R1 研究提升为 Go、VPN、E1-E8、R-stage、arm64 或真机通过。

## 构建可复现性边界

后置 clean build 再次证明相同源码可以构建两份 unsigned HAP，但包 SHA-256 变为 application `08c7e5e21bcfa58bcef7a8cd8874a480551d69c36beed63bfdfef979080a49c6`、test `07700c305314e20fc69bcb14c3a307354415957c6f8f47e034cb0bef093eaa67`。因此当前 Hvigor unsigned HAP 打包不是字节级可复现构建，后置产物不得替代本记录运行对象；工作区输出已恢复为实际运行对象的两个 SHA-256，且两份运行对象已按上表以 `.bin` 原字节归档。E8 聚合时必须使用本记录的 measured HAP 哈希与归档对象，或用新的独立证据替代本记录，不得仅凭源码重建推定制品同一性。
