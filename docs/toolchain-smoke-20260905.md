# 工具链 smoke 登记（2026-09-05，非设备、非 evidence）

> 结论：两项 /tmp 工具链 smoke（A：HarmonyOS CLT26 HAP 构建链；B：rustc 1.98.1 × SDK26 clang × BoringTun 0.7.1）当日均以 exit 0 完成。本文写作前，登记代理已对 /tmp 日志与产物逐项重新复核：两目录仍在、两产物 hash 与下文记录一致，关键日志行在位（见[登记时代理复核](#登记时代理复核)），故登记为 PASS。
> 本文件是**登记性记录**，不是物理 evidence：无 evidence ID、无签名、未上设备/模拟器，不替代后续 N1BDISC 对同产物的 freeze，边界详见[边界与效力](#边界与效力)。

## 结论一览

| 项 | A：HAP 工具链 smoke | B：Rust BoringTun smoke |
| --- | --- | --- |
| 目录 | `/tmp/tc26-hap-smoke` | `/tmp/tc26-rust-boringtun-smoke` |
| 验证目标 | CLT 26.0.0.821 全链 HAP 构建 | 主机 rustc 交叉产出 OHOS aarch64 cdylib 并真实链接 BoringTun FFI |
| 命令 | `hvigorw assembleHap` | `cargo clean` 后 `cargo build --release --target aarch64-unknown-linux-ohos --offline --locked` |
| 结果 | exit 0，`BUILD SUCCESSFUL in 16 s 203 ms`，34 tasks 全量执行 | exit 0（`Finished release profile [optimized]`） |
| 产物 | `entry-default-unsigned.hap`，9572 字节 | `libtc26smoke.so`，1097456 字节 |
| SHA-256 | `692e4eea7d0be73f26c2c7349d0635ab8cc06d1cf7229bb595a016f32c7f0270` | `67505b5c248970c99db348e199ae654c62dde0959c5947f212943864f4b79295` |
| 判定 | PASS（未签名，构建链可用） | PASS（ELF64 AArch64 DYN，FFI 真实链接，未运行） |

## 边界与效力

- **临时 smoke**：两个目录都在 `/tmp`，属一次性验证现场；系统清理后产物与日志不保证留存，本文是唯一留存登记。
- **非物理 evidence**：未写入 `docs/evidence/`，不占用任何 evidence ID，不进入 evidence 台账，不满足物理设备证据的任何判据。
- **无 ID**：两项均未分配 evidence / gate 记录 ID，本文仅按日期登记。
- **无签名、无设备、无模拟器**：A 的 HAP 未签名（构建时无 signingConfig，`SignHap` 步仅 WARN），未安装到真机或模拟器；B 的 `.so` 未运行（环境无 `qemu-aarch64`），仅做静态验证。
- **不替代 N1BDISC freeze**：N1B DISC gate 后续对同产物有 freeze 要求（对冻结产物复算 SHA-256、确认字节不变），以届时流程与产物为准。本文的 hash 复核只是当日登记核对，不构成 freeze 记录，也不预支 freeze 结论。

## A：/tmp/tc26-hap-smoke —— CLT26 HAP 构建链

### 工具链版本（A）

| 项 | 值 | 依据（本次复核确认在位） |
| --- | --- | --- |
| Command Line Tools | `26.0.0.821` | `<CLT>/` 目录与 `version.txt` |
| HarmonyOS SDK | `26.0.0.105`，apiVersion 26，Release | `<CLT>/sdk/default/sdk-pkg.json`：`"version": "26.0.0.105"`、`"apiVersion": "26"`、`"platformVersion": "26.0.0"`、`"releaseType": "Release"` |
| hvigor 插件 | `6.26.4` | `<CLT>/hvigor/hvigor-ohos-plugin/package.json`：`"version": "6.26.4"`；构建日志 `+ @ohos/hvigor-ohos-plugin 6.26.4 <- ...` |
| ohpm | `26.0.0.630` | `<CLT>/ohpm/bin/ohpm --version` 本次实测输出 |

其中 `<CLT>` = `/home/worker/harmonyos/command-line-tools/26.0.0.821`。

### 配置要点（A）

- `build-profile.json5`：default 产品的 `compileSdkVersion` / `compatibleSdkVersion` / `targetSdkVersion` 均为 `'26.0.0'`，`runtimeOS: 'HarmonyOS'`。
- `oh-package.json5` 与 `hvigor/hvigor-config.json5` 的 `modelVersion` 均为 `'26.0.0'`。
- hvigor 插件以绝对路径引入（不走注册表解析）：

```json5
dependencies: {
  '@ohos/hvigor-ohos-plugin': 'file:/home/worker/harmonyos/command-line-tools/26.0.0.821/hvigor/hvigor-ohos-plugin'
}
```

### 构建过程（A）

- `hvigorw assembleHap`：exit 0，`BUILD SUCCESSFUL in 16 s 203 ms`，`34 tasks in total: 34 executed, 0 up-to-date`（全量构建，见 `hvigor-build.log`）。该次运行 17:51:37 起跑，产物 HAP mtime `17:51:53` 与之对应。
- 17:52:44 另有一次增量复跑（debug 日志：`33 tasks in total: 14 executed, 19 up-to-date`），未改写产物，HAP mtime 保持 `17:51:53`。
- `SignHap` 步输出 `WARN: No signingConfig found for product default`，即**产物未签名**，属本 smoke 的预期形态。

### 产物与 pack.info（A）

| 项 | 值 |
| --- | --- |
| 路径 | `/tmp/tc26-hap-smoke/entry/build/default/outputs/default/entry-default-unsigned.hap` |
| size | `9572` 字节 |
| SHA-256 | `692e4eea7d0be73f26c2c7349d0635ab8cc06d1cf7229bb595a016f32c7f0270` |

- 同目录 `pack.info` 的 `apiVersion` 为 `"compatible": 26, "target": 26, "releaseType": "Release"`（API26 Release），`bundleName` 为 `cn.alfadb.netbird.toolchainsmoke`。
- HAP 为未签名 zip，内含 7 个条目（`module.json`、`resources.index`、`resources/base/media/app_icon.svg`、`resources/base/profile/main_pages.json`、`ets/modules.abc`、`pack.info`、`pkgSdkInfo.json`），无签名块。

## B：/tmp/tc26-rust-boringtun-smoke —— rustc 1.98.1 × SDK26 clang × BoringTun 0.7.1

### 工具链版本（B）

- rustc `1.98.1 (48a229cea 2026-09-01)`、cargo `1.98.1 (797e8a9bc 2026-08-05)`、rustup `1.29.1 (d95a37b6a 2026-08-13)`（`RUSTUP_HOME=/home/worker/rust/rustup`，`CARGO_HOME=/home/worker/rust/cargo`）；已安装 target 含 `aarch64-unknown-linux-ohos`。
- SDK26 native clang（本次实测）：`OHOS (dev) clang version 15.0.4`；`aarch64-unknown-linux-ohos-clang` 为 wrapper，追加 `-target aarch64-linux-ohos --sysroot=<SDK26>/sysroot -D__MUSL__`。
- 链接器经 `.cargo/config.toml` 固定为 SDK26 绝对路径；`CC`/`CXX`/`AR` 仅 per-target 导出（`CC_aarch64_unknown_linux_ohos` 等），不设全局。

### 依赖与锁定（B）

- `Cargo.toml` 依赖行（逐字）：

```toml
boringtun = { version = "=0.7.1", default-features = false, features = ["ffi-bindings"] }
```

- `Cargo.lock` 中 `boringtun 0.7.1` 的 checksum 为 `15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939`（前缀 `15dd`、后缀 `6939`），与 Phase 1 在线 `cargo fetch` 后对 crates.io 缓存 `.crate` 的 `sha256sum` 一致。
- 无 `[patch]`、无 vendoring、未启用 `device` feature；crate 为 `crate-type = ["cdylib"]`。

### 构建（B）

- 流程：`cargo clean` 后执行 `cargo build --release --target aarch64-unknown-linux-ohos --offline --locked`，exit 0（`RESULT.md` 记录 `Finished release profile [optimized] target(s) in 10.68s`，ring 0.17.14 经 per-target CC/AR 编译）。
- `--offline --locked` 表明构建完全使用已锁定且 checksum 校验过的依赖，未触碰网络。

### 产物与 ELF 验证（B）

| 项 | 值 |
| --- | --- |
| 路径 | `/tmp/tc26-rust-boringtun-smoke/target/aarch64-unknown-linux-ohos/release/libtc26smoke.so` |
| size | `1097456` 字节 |
| SHA-256 | `67505b5c248970c99db348e199ae654c62dde0959c5947f212943864f4b79295` |

本次以 GNU `file`/`readelf` 与 SDK `llvm-nm`/`llvm-objdump` 复核：

- `file`：`ELF 64-bit LSB shared object, ARM aarch64, dynamically linked, not stripped`；`readelf -h`：`ELF64` / `DYN (Shared object file)` / `AArch64`。
- 动态依赖：`NEEDED` **仅** `libc.so`（`FLAGS BIND_NOW`；全程无 `libc.so.6`，即未混入 glibc 依赖）。
- 自有 FFI 导出（`.dynsym` GLOBAL FUNC）：`tc26_smoke_version`、`tc26_smoke_public_key_b64`、`tc26_smoke_tunnel_new`、`tc26_smoke_tunnel_free`、`tc26_smoke_str_free`。
- BoringTun FFI 亦在 `.dynsym` 全局导出：`new_tunnel`、`tunnel_free`、`wireguard_write`/`read`/`tick`/`stats`/`force_handshake`、`x25519_key_to_base64`/`key_to_hex`/`key_to_str_free`/`public_key`/`secret_key`、`check_base64_encoded_x25519_key`、`set_logging_function`。
- PLT 实际引用（非死代码）：`tc26_smoke_tunnel_new` 尾调用 `new_tunnel@plt`（`37d04: b 0xa8730 <new_tunnel@plt>`）；`tc26_smoke_public_key_b64` 经 PLT 调用 `x25519_secret_key`、`x25519_public_key`、`x25519_key_to_base64`、`x25519_key_to_str_free`。
- 未做运行时验证（环境无 `qemu-aarch64`）；本 smoke 的接受判据是静态可链接/可引用（dynsym 导出 + PLT 调用位），已满足。

## 登记时代理复核

本文写作前（2026-09-05），登记代理重新执行的核对及结果：

```bash
# 目录与日志在位
ls /tmp/tc26-hap-smoke /tmp/tc26-rust-boringtun-smoke        # 两目录均存在
grep -c "34 tasks in total" /tmp/tc26-hap-smoke/hvigor-build.log   # 1（BUILD SUCCESSFUL 同文件在位）
grep -A3 'name = "boringtun"' /tmp/tc26-rust-boringtun-smoke/Cargo.lock  # checksum 15dd...6939 在位

# 产物 hash 复算（本次实测，与上文记录一致）
sha256sum /tmp/tc26-hap-smoke/entry/build/default/outputs/default/entry-default-unsigned.hap
# 692e4eea7d0be73f26c2c7349d0635ab8cc06d1cf7229bb595a016f32c7f0270（9572 字节）
sha256sum /tmp/tc26-rust-boringtun-smoke/target/aarch64-unknown-linux-ohos/release/libtc26smoke.so
# 67505b5c248970c99db348e199ae654c62dde0959c5947f212943864f4b79295（1097456 字节）
```

结论：两项记录与现场证据吻合，登记为 PASS。此后 /tmp 被清理的，以本文文字记录为准，效力边界仍以[边界与效力](#边界与效力)为准。
