# N0 native core host-preflight 记录

最后核验：2026-08-09

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 N0(b) 单一 native WireGuard core 的 host-preflight 结果。预检在 `/tmp/n0-preflight` 进行，只覆盖 host 侧工具链与构建核验，不启动 Emulator、不运行 HDC、不产生任何 runtime verdict。

## 结论边界

- 本记录是 **host-only 前置记录**：`execution: not-run-host-preflight`、`record_status: collected`、`verdict: pass`。
- **`verdict: pass` 严格只表示 host-preflight 通过，不是 N0 pass**：N0 的 `reviewed-pass/pass` 双轴验收尚未进行，N0 门未关闭。
- 本记录**不占用**后续 N0 runtime evidence ID；N0(b) 的 Emulator 加载证据须使用新 ID 单独登记。
- 本记录**不形成平台结论**：不证明、也不否定 BoringTun 0.7.1 在 API 24 x86_64 Emulator 或 arm64 设备上的加载/运行行为。
- 本记录**不授权物理设备**：N0 决议明确当前不授权任何物理设备 HDC（见 [N0 决议](../n0-native-client-feasibility.md) 第 4 条）。

## 证据记录

```yaml
evidence_id: EV-N0-N0HOST-20260809-0001
information_status: current-measured
record_status: collected
stage_or_gate: N0
related_stages_or_gates: [E8, R1, R2]
execution: not-run-host-preflight
target_tuple:
  distribution: host-only preflight; no guest runtime executed
  device: no Emulator started; no physical device
  full_system_version: N/A; host-only record, no guest system version measured
  architecture: host x86_64 (Linux); OHOS cross targets aarch64-unknown-linux-ohos / x86_64-unknown-linux-ohos
  sdk_api_syscap: OHOS Native SDK 6.1.1.125 (apiVersion 24); no SDK/API runtime surface exercised
  channel: N/A; no HAP, signing or distribution input
code_sha: 2c567dc721c6582f93a15b241e843e3bbff3f7f3 (current repository HEAD at preflight time; working tree clean)
upstream_sha: N/A for this record; BoringTun 0.7.1 crate checksum recorded below; NetBird v0.76.3 fixed baseline f65f7b347ee4e7de6d98c488d3d894cd018b02b6 per N0 resolution
toolchain: host Linux x86_64; rustc 1.92.0 (ded5c06cf 2025-12-08); cargo 1.92.0 (344c4567c 2025-10-21); rustup 1.28.2 (e4f3ad6f8 2025-04-28); OHOS clang 15.0.4 (llvm-project 99548401bd80576251755b185ab95945e5fe7884); OHOS Native SDK 6.1.1.125
working_directory: /tmp/n0-preflight (host-only scratch; not part of the repository)
command: cargo build --release --target aarch64-unknown-linux-ohos / --target x86_64-unknown-linux-ohos (per-probe scratch crates); readelf/nm/sha256sum verification
input: BoringTun 0.7.1 crate (crates.io, checksum below); scratch crates min-hello (no deps), n0-bt-probe (boringtun 0.7.1 default-features=false, features=["ffi-bindings"]), n0-bt-device (boringtun 0.7.1 default-features=false, features=["ffi-bindings","device"])
expected: verify host Rust/OHOS toolchain, BoringTun 0.7.1 crate integrity, ffi-bindings dual-ABI cdylib build, NEEDED and exported C ABI surface; record device feature build outcome
actual: see tables below; ffi-bindings dual-ABI build succeeded; device feature build failed on socket2 0.4.10 IovLen (reproduced, see below)
started_at: 2026-08-09T23:05:52+08:00
ended_at: 2026-08-09T23:11:00+08:00
clock_source: host CLOCK_REALTIME via stat timestamps and date --iso-8601=seconds
artifact_sha256:
  libn0_bt_probe.so (aarch64-unknown-linux-ohos, release): 384813924314aba91671d51c3c90875c60de74beab777e53e56bdadbad5e1dcf
  libn0_bt_probe.so (x86_64-unknown-linux-ohos, release): 4bb46a6bf6747d1a6fbab97d03e902f93f9895d9a4be454403c67a92ffd9c758
  libmin_hello.so (aarch64-unknown-linux-ohos, release): 332879239e627613856b36529d83e510e16d4ad96f210198c5fd974dd8b4888b
  libmin_hello.so (x86_64-unknown-linux-ohos, release): 95a07508f7a88e5fe66870f455b3ad6858c14e10ec3652d56a689c26d99ed872
raw_log_reference: /tmp/n0-preflight scratch only; no repository raw log archived for this host-only preflight
verdict: pass (host-preflight only; NOT an N0 pass)
reviewer: pending independent review
reviewed_at: pending
review_record: pending
```

## 本机 host 状态（当前实测）

| 检查项 | 结果 | 说明 |
| --- | --- | --- |
| 仓库状态 | clean | `git status` 无未提交变更；HEAD `2c567dc721c6582f93a15b241e843e3bbff3f7f3` |
| rustc | 1.92.0 (ded5c06cf 2025-12-08) | stable 工具链 |
| cargo | 1.92.0 (344c4567c 2025-10-21) | 与 rustc 同版本 |
| rustup | 1.28.2 (e4f3ad6f8 2025-04-28) | 默认工具链 stable-x86_64-unknown-linux-gnu |
| 已安装 targets | `aarch64-unknown-linux-ohos`、`x86_64-unknown-linux-ohos`、`x86_64-pc-windows-gnu`、`x86_64-pc-windows-msvc`、`x86_64-unknown-linux-gnu`、`x86_64-unknown-linux-musl` | 见下方 rustup 边界说明 |
| OHOS clang | `OHOS (dev) clang version 15.0.4 (llvm-project 99548401bd80576251755b185ab95945e5fe7884)` | `$DEVECO_SDK_HOME/default/openharmony/native/llvm/bin/clang` |
| OHOS Native SDK | 6.1.1.125，apiVersion 24，releaseType Release | `oh-uni-package.json` |
| OHOS sysroot | `.../native/sysroot/usr`（include + lib；lib 含 `aarch64-linux-ohos`、`arm-linux-ohos`、`x86_64-linux-ohos`） | 正式 NDK sysroot 存在 |
| BoringTun 0.7.1 crate | checksum `15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939` | `sha256sum boringtun-0.7.1.crate` |
| ffi-bindings 双 ABI 构建 | pass | `n0-bt-probe` 在 aarch64 与 x86_64 OHOS target 均产出 release cdylib |
| NEEDED | 仅 `libc.so` | 双 ABI 均只依赖 libc.so |
| 导出 C ABI | 14 个 boringtun 符号 + 1 个 `n0_probe_version` | 见下方符号表 |
| device feature 构建 | blocked | socket2 0.4.10 `IovLen` 未定义（见下方） |

### rustup target install 边界

`aarch64-unknown-linux-ohos` 与 `x86_64-unknown-linux-ohos` 是 **host 用户工具链状态变化**（`rustup target install` 写入 `~/.rustup`），**不是仓库内容**；仓库不包含、不提交任何工具链状态。预检只记录该状态存在，不把工具链安装视为项目制品。

### BoringTun 0.7.1 发布包缺 LICENSE 文件注意

crate 发布包（`boringtun-0.7.1.crate`）内**没有 LICENSE 文件**，只有 `benches/`、`Cargo.lock`、`Cargo.toml`、`Cargo.toml.orig`、`README.md`、`src/`。`Cargo.toml.orig` 与 README 均声明 `license = "BSD-3-Clause"`（README 链接 opensource.org BSD-3-Clause）。该缺失只作注意项记录：许可声明存在但发布包未随附许可证文本，进入 N0 依赖锁定/SBOM/NOTICE 审查时须按此处理；本记录不下法律结论。

### ffi-bindings 双 ABI 构建（pass）

`n0-bt-probe`（`boringtun = { version = "0.7.1", default-features = false, features = ["ffi-bindings"] }`，`crate-type = ["cdylib"]`）在以下 target 均构建成功：

| target | 产物 | SHA-256 |
| --- | --- | --- |
| `aarch64-unknown-linux-ohos` (release) | `libn0_bt_probe.so` | `384813924314aba91671d51c3c90875c60de74beab777e53e56bdadbad5e1dcf` |
| `x86_64-unknown-linux-ohos` (release) | `libn0_bt_probe.so` | `4bb46a6bf6747d1a6fbab97d03e902f93f9895d9a4be454403c67a92ffd9c758` |

`readelf -d` 显示双 ABI 的 `DT_NEEDED` 均只有 `libc.so`。`nm -D --defined-only` 显示 aarch64 产物导出 15 个符号：14 个 boringtun C ABI + 1 个 `n0_probe_version`。

14 个 boringtun C ABI 符号（`boringtun::ffi`，`src/ffi/mod.rs`）：

```text
x25519_secret_key
x25519_public_key
x25519_key_to_base64
x25519_key_to_hex
x25519_key_to_str_free
check_base64_encoded_x25519_key
set_logging_function
new_tunnel
tunnel_free
wireguard_write
wireguard_read
wireguard_tick
wireguard_force_handshake
wireguard_stats
```

### device feature 构建（blocked，不 patch、不进入 N0）

`n0-bt-device`（`features = ["ffi-bindings", "device"]`）在 `x86_64-unknown-linux-ohos` 构建失败，已复现：

```text
error[E0412]: cannot find type `IovLen` in this scope
error[E0433]: failed to resolve: use of undeclared type `IovLen`
error: could not compile `socket2` (lib) due to 4 previous errors
```

原因：OHOS target 的 rustc cfg 为 `target_os="linux"` + `target_env="ohos"`；socket2 0.4.10 `src/sys/unix.rs` 的 `IovLen` 两个定义分支（`usize` 版要求 `target_env="gnu"` 或 uclibc 64 位；`c_int` 版要求 musl 等）**均不匹配 `target_env="ohos"`**，导致 `IovLen` 未定义。boringtun 0.7.1 的 `device` feature 依赖 socket2 0.4.10，因此 device feature 在 OHOS target 上 blocked。

按 N0 决议停止条件（native core 需要私有/高维护 patch 即停止并回 T0），本记录**不 patch socket2、不把 device feature 纳入 N0**；N0(b) 只使用 `ffi-bindings`（无 socket2 依赖）的 C ABI 面。

## 边界与后续

- 本记录 `verdict: pass` 只表示 host-preflight 通过；N0 双轴验收（`reviewed-pass/pass`）尚未进行，N0 门未关闭。
- 本记录不占用后续 N0 runtime evidence ID；N0(b) 的 API 24 x86_64 Emulator 加载证据须使用新 ID 单独登记，且按 N0 决议只允许 Emulator HDC。
- 本记录不授权物理设备；E3-PHYS-PREFLIGHT 仍是用户显式授权后的第一物理动作。
- BoringTun 0.7.1 发布包缺 LICENSE 文件、device feature 的 socket2 IovLen blocked，均进入 N0 依赖锁定/SBOM/NOTICE 审查输入。
- 本记录为 host-only，`record_status: collected`，待独立审查。
