# R1 Go ABI 预探针与 API 24 HAP 构建证据

最后核验：2026-07-17

本文按[证据与脱敏 Schema](../evidence-schema.md)持久化 `[TEMP_WORKDIR]/REPORT.md` 的关键证据，并记录短生命周期 API 24 HAP 的实际构建结果与后续 Emulator smoke。文件名保留任务指定的 `2026-07-16`，实际证据采集发生在 Asia/Shanghai 的 2026-07-17。

## 结论边界

`EV-R1-EMU24-20260717-0001` 只覆盖编译、链接、静态检查和 unsigned HAP 打包；`EV-R1-EMU24-20260717-0002` 只覆盖无窗口 API 24 Emulator smoke；`EV-R1-EMU24-20260717-0003` 是因实验驱动干扰而保留的无效可见模式尝试；`EV-R1-EMU24-20260717-0004` 只覆盖指定 API 24 Emulator 的可见显示、readiness、现有 unsigned HAP 安装、Ability 启动尝试、HiLog 采集、停止与卸载；`EV-R1-EMU24-20260717-0005` 只覆盖本地 API 24 schema 下的最小 `ohosTest`、双 HAP unsigned 安装和短生命周期 TestRunner 直接 Node-API 调用；`EV-R1-EMU24-20260717-0006` 只覆盖固定 Go 1.25.12 Linux/amd64 c-shared ELF 的构建、同一性打包和 API 24 x86_64 TestRunner 进程直接 `dlopen`；`EV-R1-EMU24-20260717-0007` 以无 Go 的普通 initial-exec TLS so 和具有 `DT_NEEDED libgoprobe.so` 的原生 wrapper 分别复测直接与传递 late-load，并在两条路径得到受控 loader 拒绝；`EV-R1-EMU24-20260717-0008` 落实 `ADJ-20260717-0001` 的公开 Native Child phone 能力门并在实现 B0 前暂停该 B 族；`EV-R1-EMU24-20260717-0009` 落实第二个六席 T0 一致批准的 Tier1 纯 C 动态 TLS loader 门，以最终 ELF 实证区分 IE、classic GD、TLSDESC gnu2 和附加 local-dynamic，在同一 API 24 x86_64 TestRunner late-load 边界先复现 IE 拒绝，再使三个动态模型全部通过加载及主线程、加载前线程、加载后线程各 100 轮隔离检查。九条证据均不构成 R1 退出，不构成 NetBird、VPN、UIAbility、真机、产品可行性、产品支持、签名接受、渠道接受或发布证据，不改变 R0、R1、R2“未退出”；0009 只证明该 loader 对这些精确 C 动态 TLS 输入的接受，不证明 Go 1.25.12 会生成或支持这些模型，不外推 arm64、具名真机、华为商用 HarmonyOS、其他 loader 或工具链，补丁数仍为 0。

## 证据记录

```yaml
evidence_id: EV-R1-EMU24-20260717-0001
information_status: current-measured
record_status: collected
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1 API 24 build target; no guest runtime executed
  device: API 24 Emulator candidate netbird_api24_phone; explicitly not started
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125; runtime not exercised
  architecture: host x86_64; static targets arm64-v8a and x86_64
  sdk_api_syscap: SDK 6.1.1.125/API 24; ArkTS Node-API and HiLog compile surface only; runtime SysCap unverified
  channel: N/A; short-lived unsigned research HAP, not a distribution candidate
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted probe source manifest SHA-256 80726026d74b945324ed39bce1e0ee57d0e6d42bb4ec54b26f4c29dcb75cf565 because no commit was requested
upstream_sha: NetBird 3a2f773d655d88d16ed953fc2a114a4e690a1b08; NetBird wireguard-go 2834bebf6c1aea76bd217f31ea91c99f75e4a20a
toolchain: Debian GNU/Linux 13 x86_64; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; HDC 3.2.0d version query only with no target operation; Emulator not started; Go 1.25.12; Node.js 18.20.1; Hvigor 6.24.3; ohpm 6.1.2.285; OHOS clang 15.0.4; CMake 3.28.2; Ninja 1.12.0; OpenJDK 21.0.11
working_directory: [TEMP_WORKDIR] and [WORKSPACE]/spikes/r1-api24-hap
command: See the replayable command groups in this record; no secret, signing, target HDC, or Emulator command was used
input: fixed NetBird v0.74.6, Go 1.25.12, SDK/API 24, generated minimal C and Go probes, and repository short-lived Stage HAP source
expected: collect compile/link/static risk evidence and build an unsigned HAP containing ordinary Node-API libprobe.so for arm64-v8a and x86_64 without claiming R1 exit
actual: fixed probes compiled as recorded; Go has no native ohos target; forced Linux Go artifacts remain runtime-unverified; unsigned HAP and both native ABIs built successfully; no loader or syscall ran
started_at: 2026-07-17T00:06:49+08:00
ended_at: 2026-07-17T00:50:05+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds and retained artifact mtimes via stat
artifact_sha256: see the complete artifact tables below
raw_log_reference: preflight [TEMP_WORKDIR]/REPORT.md SHA-256 d0cd4393617fe224543b1d5961d5494fa78fa94244d118d50a567942d17696c1, temporary local-user access; persisted build log docs/evidence/raw/EV-R1-EMU24-20260717-0001-assembleHap.log SHA-256 7772a5d4d22632fc9957faf360e54e809e91a59cd91c3580022efcbf1c55113c, repository access
verdict: pass
verdict_scope: compile, link, static inspection, and unsigned HAP packaging only; research evidence cannot be used for stage exit
reviewer: pending formal independent review; independent preflight corrections listed below have been incorporated
reviewed_at: pending
review_record: pending
```

`record_status` 保持 `collected`，因为正式独立审查记录尚未建立，预检原始工作区仍位于临时目录，且没有签名、加载、运行、安装或设备证据。`verdict: pass` 只表示本次预先确定的编译和 unsigned 打包研究目标完成，不表示任何阶段门通过。

## 固定输入

| 项目 | 固定值 | 当前证据边界 |
| --- | --- | --- |
| NetBird | v0.74.6，commit `3a2f773d655d88d16ed953fc2a114a4e690a1b08` | 模块校验、依赖解析、Linux/arm64 archive 和最小 c-shared 静态证据 |
| wireguard-go | `github.com/netbirdio/wireguard-go` pseudo-version `v0.0.0-20260628102922-2834bebf6c1a`，commit `2834bebf6c1aea76bd217f31ea91c99f75e4a20a` | 固定替换依赖；未运行 |
| Go | 1.25.12 | 上游声明 `go 1.25.5` 和 `toolchain go1.25.12`；无 `GOOS=ohos` |
| Command Line Tools | 6.1.1.290 | 稳定构建链路 |
| SDK | HarmonyOS/OpenHarmony SDK 6.1.1.125，API 24 | C/C++编译和链接已实测；运行未验证 |
| Hvigor | 6.24.3 | 本机随包插件通过固定绝对 `file:` 依赖使用 |
| Node.js | 18.20.1 | 由稳定wrapper局部使用 |
| ohpm | 6.1.2.285 | 本地类型依赖安装和锁文件生成已实测 |
| HAP bundle | `cn.alfadb.netbird.r1probe` | unsigned短生命周期研究探针 |

## 实际命令

以下是预检的关键工具链、C和Go命令摘要；完整原始叙述保留在已哈希的临时 `REPORT.md` 中。

```bash
go version
GOTOOLCHAIN=go1.25.12 go version
GOTOOLCHAIN=go1.25.12 go tool dist list | rg 'ohos|linux/(arm64|amd64)'
[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --version
[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=aarch64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot -fPIC -shared -Wl,-soname,libhello-aarch64.so -o [TEMP_WORKDIR]/c/libhello-aarch64.so [TEMP_WORKDIR]/c/hello.c
[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=x86_64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot -fPIC -shared -Wl,-soname,libhello-x86_64.so -o [TEMP_WORKDIR]/c/libhello-x86_64.so [TEMP_WORKDIR]/c/hello.c
GOTOOLCHAIN=go1.25.12 GOOS=linux GOARCH=arm64 CGO_ENABLED=1 CC='[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=aarch64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot' go -C [TEMP_WORKDIR]/go-shared build -buildmode=c-shared -o [TEMP_WORKDIR]/go-shared/libgoprobe-aarch64.so .
GOTOOLCHAIN=go1.25.12 GOOS=linux GOARCH=amd64 CGO_ENABLED=1 CC='[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=x86_64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot' go -C [TEMP_WORKDIR]/go-shared build -buildmode=c-shared -o [TEMP_WORKDIR]/go-shared/libgoprobe-x86_64.so .
GOTOOLCHAIN=go1.25.12 GOSUMDB=sum.golang.org GOPROXY=https://proxy.golang.org,direct go -C [TEMP_WORKDIR]/netbird-probe mod download -json github.com/netbirdio/netbird@v0.74.6
GOTOOLCHAIN=go1.25.12 go -C [TEMP_WORKDIR]/netbird-probe mod verify
GOTOOLCHAIN=go1.25.12 GOOS=linux GOARCH=arm64 CGO_ENABLED=1 CC='[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=aarch64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot' go -C [TEMP_WORKDIR]/netbird-probe build -mod=mod -buildmode=archive -o [TEMP_WORKDIR]/netbird-probe/wireguard-device-linux-arm64.a golang.zx2c4.com/wireguard/device
GOTOOLCHAIN=go1.25.12 GOOS=linux GOARCH=arm64 CGO_ENABLED=1 CC='[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=aarch64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot' go -C [TEMP_WORKDIR]/netbird-probe build -mod=mod -buildmode=archive -o [TEMP_WORKDIR]/netbird-probe/netbird-client-iface-linux-arm64.a github.com/netbirdio/netbird/client/iface
GOTOOLCHAIN=go1.25.12 GOOS=linux GOARCH=arm64 CGO_ENABLED=1 CC='[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/llvm/bin/clang --target=aarch64-linux-ohos --sysroot=[HARMONY_HOME]/command-line-tools/current/sdk/default/openharmony/native/sysroot' go -C [TEMP_WORKDIR]/netbird-probe build -mod=readonly -buildmode=c-shared -o [TEMP_WORKDIR]/netbird-probe/libnetbird-iface-linux-arm64.so .
```

以下命令在短生命周期HAP工程中实际执行；第一次从仓库根运行 `ohpm install --all` 因工作目录错误而失败，改到工程目录后成功，该操作错误不计入平台构建判定。

```bash
cd [WORKSPACE]/spikes/r1-api24-hap
[HARMONY_HOME]/command-line-tools/6.1.1.290/bin/ohpm install --all
[HARMONY_HOME]/command-line-tools/6.1.1.290/bin/hvigorw clean --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon
[HARMONY_HOME]/command-line-tools/6.1.1.290/bin/hvigorw assembleHap --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon
```

最终构建日志通过同一 `assembleHap` 命令的标准输出和标准错误合流采集，开始于 `2026-07-17T00:48:58+08:00`，结束于 `2026-07-17T00:49:10+08:00`，Hvigor报告 `BUILD SUCCESSFUL in 11 s 271 ms`。

## PASS、PARTIAL、FAIL 矩阵

| 探针 | arm64/aarch64 | x86_64/amd64 | 状态 | 边界 |
| --- | --- | --- | --- | --- |
| 固定Go和上游版本 | Go 1.25.12，NetBird v0.74.6 | 同左 | PASS | 下载校验和模块验证通过 |
| Go原生OHOS目标 | `go tool dist list` 无 `ohos/arm64` | 无 `ohos/amd64` | FAIL | 首个平台阻塞；只能强制选择Linux运行时路径 |
| OHOS C shared库 | ELF64 AArch64，`NEEDED libc.so` | ELF64 x86-64，`NEEDED libc.so` | PASS | 只证明C ABI编译和静态链接 |
| 最小Go c-shared | 生成ELF64并导出探针 | 生成ELF64并导出探针 | PARTIAL | `GOOS=linux`；未加载、未运行 |
| x86_64 Go TLS | arm64未见`STATIC_TLS` | 动态段含`STATIC_TLS` | PARTIAL | x86_64首要运行风险，必须由目标loader实测 |
| wireguard-go与NetBird archive | Linux/arm64 archive命令成功 | 未执行 | PASS | 纯Go archive不是OHOS证据，不得用于平台结论 |
| archive作为OHOS证明 | 不成立 | 不成立 | FAIL | 未经过OHOS loader、runtime、syscall或设备路径 |
| 最终NetBird iface so | 生成Linux/arm64 c-shared | 未执行 | PARTIAL | 发生DCE，只引用类型并导出最小符号，不代表完整数据面已链接 |
| syscall行为 | 未运行 | 未运行 | PARTIAL | 无任何Go syscall运行证据 |
| Stage HAP构建 | 包含`arm64-v8a/libprobe.so` | 包含`x86_64/libprobe.so` | PASS | 只隔离普通C++ Node-API工程和打包路径 |
| HAP签名 | unsigned | unsigned | PARTIAL | `isSigned:false`，未读取或生成签名秘密 |
| 安装、启动、HiLog、卸载 | 未执行 | 未执行 | PARTIAL | Emulator未启动，真机不可连接 |
| R1阶段结论 | 不满足退出 | 不满足退出 | PARTIAL | R0未退出且缺具名真机闭环 |

## 首个阻塞与运行风险

- 首个平台阻塞是 Go 1.25.12 没有原生 `GOOS=ohos`；成功产物明确使用 `GOOS=linux`，因此选择Linux runtime、netpoll、syscall和平台文件。
- x86_64 Go c-shared动态段的 `STATIC_TLS` 是当前首要运行风险；它可能在HarmonyOS应用loader加载时失败，必须在API 24 Emulator短时smoke中先验证，不能由链接成功推断。
- 当前没有任何 loader、线程、goroutine、timer、内存、DNS、Dial、epoll、futex、clone、signal或其他syscall运行证据。
- 数据面首个未闭合平台门仍是普通第三方HarmonyOS VPN应用取得受支持TUN fd和socket保护或绕行机制；头文件存在不等于权限和运行语义可用。

## 独立审查修正

本记录已吸收预检独立复核提出的以下纠正，但正式schema审查者和审查记录ID仍待补齐。

- 纯Go archive只证明强制 `GOOS=linux` 的包可被Go编译器处理，不是OHOS ABI、loader、libc、syscall或运行证据。
- 最终 `libnetbird-iface-linux-arm64.so` 存在链接器DCE。archive中可见 `client/iface.(*WGIface)` 完整方法和wireguard `device.(*Device)`方法，最终so中这些方法均缺失，只残留依赖初始化符号和最小导出，因此不能声称完整NetBird iface或WireGuard实现已经进入最终可执行路径。
- x86_64 Go c-shared的 `STATIC_TLS` 必须提升为首要运行风险，而不是普通静态差异。
- 本轮没有运行任何Go syscall；SDK头文件中的号值对齐和链接时符号解析不能改写为运行兼容性。
- 固定NetBird和wireguard-go没有OHOS build tag或HarmonyOS adapter，强制Linux路径会选择Linux TUN、netlink、`SO_MARK`和主机DNS行为。OHOS适配预计会消耗R2退出前补丁预算中的若干项，具体数量必须在形成实际补丁后逐项登记；当前尚未提出补丁，预算计数仍为0。

这些修正不触发T0，因为本轮没有测试出致命ABI、libc、编译器或链接器负面结果，没有提出高维护风险补丁，也没有超过补丁预算。若目标运行失败于必需Go runtime/syscall、普通应用VPN路径被否定，或必须维护高风险Go runtime fork，则按章程立即重新进行T0讨论。

## 预检制品哈希

| 制品 | SHA-256 |
| --- | --- |
| 原始预检报告 `REPORT.md` | `d0cd4393617fe224543b1d5961d5494fa78fa94244d118d50a567942d17696c1` |
| `c/libhello-aarch64.so` | `e28cffcfd11fea53263dfa80344b6c7c4641bd94aa5d18fc4a326ea21c3501fa` |
| `c/libhello-x86_64.so` | `1279172bb76960cb0deef973db43fdf3e5194ebfa37044d10ea979b98440553c` |
| `go-shared/libgoprobe-aarch64.so` | `3336b6411a4d9a46ca8a4b635c12856b39717ae544f1744feb341449ba5b8b55` |
| `go-shared/libgoprobe-x86_64.so` | `27fe4796e8f78aceef2be3ba0a1bb0fd2a04f19356e9fcf3ababe0c3fac9886f` |
| `netbird-probe/wireguard-device-linux-arm64.a` | `013ee8a23e11a6a3898cd7dcf580e3f5d874b5d5c89b1b458b8bc4b7fdf3cc07` |
| `netbird-probe/wireguard-tun-linux-arm64.a` | `4139cfc580a14231e6893c772016e11c79f503eb854beea6fe19c27ab6c7becf` |
| `netbird-probe/netbird-iface-device-linux-arm64.a` | `6ce70de66114eb4dbbfd151da241ae0d146834c0299d6ff7d4414a7b865b0acd` |
| `netbird-probe/netbird-client-iface-linux-arm64.a` | `90092bdfbbc723bb8f8a711ed60e97ae086b4f19b7546157f467aab20ef51daf` |
| `netbird-probe/libnetbird-iface-linux-arm64.so` | `3f40185d8d8ccb13d2a5984ba97b52b3e4206c801139b2837904252d3739b8cf` |
| NetBird `v0.74.6.zip` | `85f0a0761641a6dfedae863fad990838796e247b868bc927ec64c715b34d56ab` |
| NetBird `v0.74.6.mod` | `9c1e30054e920b86e2a77177c16e565770278fad1f884a163760a4fd2fb1918e` |
| `netbird-probe/go.mod` | `b4edde8438343518c439c3f1a0b4c0afac98b625bb6e1dbef92a3850877cdf9d` |
| `netbird-probe/go.sum` | `85b78f2dacc0bcb5ce026f2352d7d12efd972fe60d70dda1f58b26931e2adc41` |

## HAP构建结果

短生命周期工程位于 `spikes/r1-api24-hap`。它只有一个最小 `UIAbility`、一个ArkUI页面、HiLog tag `R1Api24Probe` 和普通C++ Node-API模块 `libprobe.so`；ArkTS在页面加载成功后调用 `ping()` 和 `version()` 并记录结果。工程不链接Go，不包含VPN能力，不读取或生成签名秘密，也不得演化为产品壳。

| 制品 | 路径 | 大小 | SHA-256 |
| --- | --- | ---: | --- |
| unsigned HAP | `spikes/r1-api24-hap/entry/build/default/outputs/default/entry-default-unsigned.hap` | 2585745 bytes | `c3b86df72039684bd70bb5424eb79492369705f7b8bc4edd62e4f1d1957a264f` |
| stripped arm64 native | `spikes/r1-api24-hap/entry/build/default/intermediates/stripped_native_libs/default/arm64-v8a/libprobe.so` | 6408 bytes | `e38b198952f6fcd090e65a36a650b275ad59a966c249954756db1eba58af6331` |
| stripped x86_64 native | `spikes/r1-api24-hap/entry/build/default/intermediates/stripped_native_libs/default/x86_64/libprobe.so` | 6176 bytes | `f3d0fc2918cd86d35c27bc969819ec5881df91b2b045f1fda5fbd0cc5c58848a` |
| final build log | `docs/evidence/raw/EV-R1-EMU24-20260717-0001-assembleHap.log` | 2578 bytes | `7772a5d4d22632fc9957faf360e54e809e91a59cd91c3580022efcbf1c55113c` |
| ohpm lock | `spikes/r1-api24-hap/entry/oh-package-lock.json5` | generated text lock | `f201c717ddeb6fedbbc57414a57534427fa5f7efa940c495660b516871d82544` |
| probe source manifest | canonical sorted `sha256sum` manifest of non-generated files | N/A; digest only | `80726026d74b945324ed39bce1e0ee57d0e6d42bb4ec54b26f4c29dcb75cf565` |

包内两份 `libprobe.so` 通过流式SHA-256复核，与上表Hvigor stripped输出完全一致。`output_metadata.json` 记录 `isSigned:false`，文件名为 `entry-default-unsigned.hap`，HAP内没有 `META-INF/` 签名条目，构建日志记录唯一预期警告 `No signingConfig found for product default`。

## HAP内容清单

| 长度 | 条目 |
| ---: | --- |
| 1310 | `module.json` |
| 828 | `resources.index` |
| 1294904 | `libs/x86_64/libc++_shared.so` |
| 6176 | `libs/x86_64/libprobe.so` |
| 1262504 | `libs/arm64-v8a/libc++_shared.so` |
| 6408 | `libs/arm64-v8a/libprobe.so` |
| 322 | `resources/base/media/app_icon.svg` |
| 322 | `resources/base/media/start_icon.svg` |
| 23 | `resources/base/profile/main_pages.json` |
| 2153 | `ets/sourceMaps.map` |
| 8528 | `ets/modules.abc` |
| 534 | `pack.info` |
| 129 | `pkgSdkInfo.json` |

HAP共13个条目，未压缩总长度2584141 bytes。`pack.info` 确认 bundle name `cn.alfadb.netbird.r1probe`、entry模块、phone设备类型、compatible/target API 24、`deliveryWithInstall:true` 和 `installationFree:false`。

## Native ELF检查

| 架构 | ELF | SONAME | NEEDED | 动态导出 |
| --- | --- | --- | --- | --- |
| arm64-v8a | ELF64 little-endian AArch64 `ET_DYN`，System V ABI，stripped | `libprobe.so` | `libace_napi.z.so`、`libhilog_ndk.z.so`、`libc++_shared.so`、`libc.so` | `RegisterProbeModule`、`_init`、`_fini` |
| x86_64 | ELF64 little-endian AMD x86-64 `ET_DYN`，System V ABI，stripped | `libprobe.so` | `libace_napi.z.so`、`libhilog_ndk.z.so`、`libc++_shared.so`、`libc.so` | `RegisterProbeModule`、`_init`、`_fini` |

Node-API `ping` 和 `version` 是由 `napi_define_properties` 注册到ArkTS exports对象的属性，不是独立C动态导出。静态检查只确认注册入口和依赖关系，不能确认HarmonyOS loader已执行构造器或ArkTS已收到返回值。

## Emulator HAP smoke

```yaml
evidence_id: EV-R1-EMU24-20260717-0002
information_status: current-measured
record_status: blocked
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK/API 24 HAP runtime surface; SysCap and physical-device behavior unverified
  channel: N/A; unsigned short-lived research HAP, not a distribution candidate
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted probe source manifest SHA-256 80726026d74b945324ed39bce1e0ee57d0e6d42bb4ec54b26f4c29dcb75cf565 because no commit was requested
upstream_sha: N/A; this HAP contains no NetBird or Go code
toolchain: Debian GNU/Linux 13 x86_64 host; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; HarmonyOS 6.1.1(24) image software 6.1.0.125; Beta HDC 3.2.0e for every target operation; stable HDC 3.2.0d host-only version query; Go, Node.js, Hvigor and ohpm not executed in this runtime smoke
working_directory: [WORKSPACE]
command: Complete replayable command transcript is in docs/evidence/raw/EV-R1-EMU24-20260717-0002-hap-smoke.log; HDC_PORT=10000 and target 127.0.0.1:10000 were explicit; no secret or stable-HDC target command was used
input: existing entry-default-unsigned.hap SHA-256 c3b86df72039684bd70bb5424eb79492369705f7b8bc4edd62e4f1d1957a264f; cold netbird_api24_phone instance; KVM fd open; no Emulator process or port listener initially
expected: satisfy runbook readiness with Connected, shell echo, uname, bootevent.boot.completed=true and HarmonyOS distribution; install the existing HAP; launch cn.alfadb.netbird.r1probe/EntryAbility; observe R1Api24Probe module initialization plus ping=pong and version=r1-api24-probe/0.0.1; force-stop, uninstall and normally stop the Emulator
actual: both cold runs passed Connected, shell echo, uname and HarmonyOS distribution while HDC was healthy, but bootevent.boot.completed was absent with errNum 106; unsigned install succeeded and bm dump reported appSignType none; Ability start twice returned semantic Error Code 10106102 because the Emulator screen was locked/unavailable; no R1Api24Probe line was observed; force-stop, uninstall, post-uninstall absence check and normal Emulator stop completed
started_at: 2026-07-17T00:53:52+08:00
ended_at: 2026-07-17T01:06:26+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, ps lstart/etime and Emulator logs
artifact_sha256: entry-default-unsigned.hap c3b86df72039684bd70bb5424eb79492369705f7b8bc4edd62e4f1d1957a264f; output_metadata.json 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0002-hap-smoke.log SHA-256 d81f9a305931f45c159ce3a5703b884b503012bceb3e4a6f80fc1d3009562e6e, repository access; HOME Emulator archives and hashes listed below, local-user access with 30-day minimum retention
verdict: blocked
verdict_scope: Emulator readiness and Ability runtime smoke were blocked by missing boot.completed and unavailable locked display; unsigned installation and cleanup passed as sub-results; this research evidence cannot be used for stage exit
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

`record_status: blocked` 由预定判据直接得出：`bootevent.boot.completed` 在两次冷启动中均未返回 `true`，且 `EntryAbility` 两次启动均返回 `Error Code:10106102`，所以不能把安装成功提升为运行通过。HDC命令本身对参数错误和Ability错误仍可能返回0，本记录按消息语义而不是仅按进程退出码判定。

可审计性降级说明：实际安装的原始HAP SHA-256 `c3b86df72039684bd70bb5424eb79492369705f7b8bc4edd62e4f1d1957a264f` 已被后续冷构建覆盖，原始二进制未进入不可变归档，当前无法逐字节复核。因此安装成功结论保留为现场观测，但可审计性降级；不得把当前 `spikes/r1-api24-hap/entry/build/...` 路径下的构建产物当作该次安装的原始对象。后续每次运行前必须先分配受控证据对象ID与持久存储并记录hash，再执行安装与启动；本记录不虚构已归档原始HAP。

| 子项 | 当前实测 | 判定 |
| --- | --- | --- |
| KVM与Beta链路 | `/dev/kvm` fd可打开；Emulator 26.0.0.200；所有target操作均使用Beta HDC 3.2.0e与显式端口10000 | PASS |
| readiness | 两轮均得到Connected、shell echo、guest uname和`HarmonyOS`，但`bootevent.boot.completed`始终为`errNum 106` | FAIL |
| unsigned安装 | HDC报告`install bundle successfully`；guest `bm dump`报告`appSignType: none`、debug provision与x86_64 CPU ABI | PASS，仅限该研究Emulator |
| Ability启动 | 两次`aa start`均返回`10106102`，标准无凭据wakeup与上滑后结果不变；截图返回`Failed to get display pixelMap` | BLOCKED |
| `R1Api24Probe` | 两次非阻塞tag采集均无匹配行 | NOT VERIFIED |
| `ping`与`version` | Ability未启动，未观察到`ping=pong`或`r1-api24-probe/0.0.1` | NOT VERIFIED |
| 清理 | `aa force-stop`、HDC uninstall和卸载后包不存在检查完成 | PASS |
| Emulator停止 | 两轮均由stop helper正常退出；最终target Offline、端口10000无监听、无Emulator进程或运行会话，调用pane仍为原`%45` | PASS |

第一轮在一次被HDC层解释为持续流的HiLog命令被timeout终止后发生shell RPC退化：target仍为Connected，但后续`echo`超时；kernel同期出现`teleport_express`超时与`watchdog_service`重启。它们是伴随观测，不构成根因结论。现场归档为 `[HARMONY_HOME]/emulator-log-archive/20260716T170248Z-r1-smoke-run1-degraded`：`Emulator.log` SHA-256 `ca843ed6702b3dc0b85520ee7ad7d7cc5192d7c08bfae91309af3168059c0ed3`，`qemu.log` SHA-256 `8d393a4d457a6cbda9572dcd78034921c430615efa262b0a30ac652df13a96af`，`kernel.log` SHA-256 `b84dae6d03489ae9f4e55a14375bede0e1ae1093a624cef2e66f704a4ef4e387`，`crash_server.log` SHA-256 `b6ff481254c61c4e023ff48c19c8f8364935303ecebb749189557fd238ac20be`。

第二轮在数据面健康时复现锁屏启动阻塞，并完成卸载和正常停止。现场归档为 `[HARMONY_HOME]/emulator-log-archive/20260716T170555Z-r1-smoke-run2-lock-blocked`：`Emulator.log` SHA-256 `29c60e56c46be0e76b80b2c9c201907da4ac265c772c68bcd4269c2450390385`，`qemu.log` SHA-256 `099a2e26f95b0df0ceed9f7f93d8fabb61d66c1a1236348b0c14daae1fe38b6d`，`kernel.log` SHA-256 `55a1ab12eb855d6067ba130f7bba52d55fb3eeecc4f171d5912296fb9904dd4d`，`crash_server.log` SHA-256 `f898f770f8d0a3a2c46de2f789e5f75fa80bf5fb1a5633d9a30384555ee59aec`。`crash_server.log` 只有TraceTool初始化行，没有本探针crash记录；Emulator日志记录normal quit。

## API 24 可见显示根因实验

```yaml
evidence_id: EV-R1-EMU24-20260717-0003
information_status: current-measured
record_status: invalidated
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK/API 24 HAP runtime surface; SysCap and physical-device behavior unverified
  channel: N/A; unsigned short-lived research HAP, not a distribution candidate
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; existing mutable HAP object was separately bound by SHA-256 before any install attempt
upstream_sha: N/A; this HAP contains no NetBird or Go code
toolchain: Debian GNU/Linux 13 x86_64 host; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; HarmonyOS 6.1.1(24) image software 6.1.0.125; Beta HDC 3.2.0e only; Xtigervnc X11 display :1; KVM fd open
working_directory: [WORKSPACE]
command: Complete transcript is in docs/evidence/raw/EV-R1-EMU24-20260717-0003-display-hap-smoke.log; launch copied the helper command and removed only -noWindow while preserving KVM, explicit -hdcport 10000, instancePath and imageRoot
input: evidence object EO-R1-EMU24-20260717-0003-HAP-01, existing entry-default-unsigned.hap SHA-256 f1e00ec5534951137d7149b3d5001e99622aa1daacd1e31618d57699e2a1cd69, DISPLAY=:1, existing X authority, no initial Emulator/HDC/listener/runtime-tmux/root-.hvigor residue
expected: within 180 seconds obtain shell echo, uname, boot/lockscreen/launcher/bootanimation parameters, HarmonyOS distribution and screenCap; if installable, install and run the fixed HAP smoke; do not guess unsupported software-renderer parameters
actual: visible EGL and guest screenCap became available in 23 seconds, but an extra host scenario screenshot command introduced by the experiment driver coincided with instance exit and a whitespace-sensitive distribution parser incorrectly rejected the otherwise healthy HarmonyOS response; no HAP install occurred
started_at: 2026-07-17T01:34:50+08:00
ended_at: 2026-07-17T01:35:47+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds and Emulator logs
artifact_sha256: EO-R1-EMU24-20260717-0003-HAP-01 f1e00ec5534951137d7149b3d5001e99622aa1daacd1e31618d57699e2a1cd69
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0003-display-hap-smoke.log SHA-256 6cc87366aa8eeb8a746cd465690ca173a585fdb8cecffe8cd6131697e14b6954 and docs/evidence/raw/EV-R1-EMU24-20260717-0003-emulator-console.log SHA-256 6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5, repository access; four HOME logs archived with hashes below, local-user access and 30-day minimum retention
verdict: invalid
verdict_scope: experiment-driver interference and a false-negative parser make this record unusable for HAP or root-cause conclusions; it is retained and not reused
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

`EV-R1-EMU24-20260717-0003` 的四类现场日志归档于 `[HARMONY_HOME]/emulator-log-archive/20260716T173545Z-r1-api24-display-exp1`：`Emulator.log` SHA-256 `e74c96d53e6e8c40e13e0cfc98785d3ddfc32a8eaf540ad9b9c235c1e9ee26df`，`qemu.log` SHA-256 `b738606042662767786cfcdbb101656c0c3b5b495ead3781c42e0b53a8fd6b3d`，`kernel.log` SHA-256 `225070dee22e606d882e141fa41b26ef73cd258c52ca82d0d7ab8d93a46c33d3`，`crash_server.log` SHA-256 `fe8f3fb61acb944ac32888de3b558b913d6197d738376b8582cd272c9fd004bf`。

```yaml
evidence_id: EV-R1-EMU24-20260717-0004
information_status: current-measured
record_status: blocked
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK/API 24 HAP runtime surface; SysCap and physical-device behavior unverified
  channel: N/A; unsigned short-lived research HAP, not a distribution candidate
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; existing mutable HAP object was separately bound by SHA-256, size and mtime immediately before installation
upstream_sha: N/A; this HAP contains no NetBird or Go code
toolchain: Debian GNU/Linux 13 x86_64 host; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; HarmonyOS 6.1.1(24) image software 6.1.0.125; Beta HDC 3.2.0e for every target operation; Xtigervnc X11 display :1; EGL 1.5; Mesa 25.0.7-2 llvmpipe LLVM 19.1.7 renderer; KVM fd open; no stable-HDC target operation, Go, Node.js, Hvigor or ohpm execution
working_directory: [WORKSPACE]
command: Complete replayable transcript is in docs/evidence/raw/EV-R1-EMU24-20260717-0004-display-hap-smoke.log; DISPLAY=:1 and XAUTHORITY were explicit; launch copied the helper command and removed only -noWindow while preserving -start netbird_api24_phone, instancePath, imageRoot and -hdcport 10000; no HAP rebuild occurred
input: evidence object EO-R1-EMU24-20260717-0004-HAP-01, existing entry-default-unsigned.hap size 2585745 bytes and SHA-256 f1e00ec5534951137d7149b3d5001e99622aa1daacd1e31618d57699e2a1cd69, no initial Emulator/HDC/listener/runtime-tmux/root-.hvigor residue
expected: within at most 180 seconds pass standard readiness with boot.completed=true or at least obtain a usable pixelMap and launch EntryAbility; for an installable state, install and confirm the existing HAP, observe Node-API initialization plus ping=pong and version=r1-api24-probe/0.0.1, then force-stop, uninstall and normally stop
actual: shell echo, guest uname and HarmonyOS distribution passed at 19 seconds; visible EGL initialized and screenCap produced a 1320x2856 PNG at 22 seconds; boot.completed remained absent and lockscreen/launcher/bootanimation probe keys returned errors; unsigned install and bundle confirmation passed, but both aa start attempts returned semantic Error Code 10106102 after wake and standard no-credential swipe; R1Api24Probe was empty; force-stop, uninstall, normal stop, four-log archive and final residual cleanup passed
started_at: 2026-07-17T01:38:02+08:00
ended_at: 2026-07-17T01:39:01+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds and Emulator logs
artifact_sha256: EO-R1-EMU24-20260717-0004-HAP-01 f1e00ec5534951137d7149b3d5001e99622aa1daacd1e31618d57699e2a1cd69; guest screenCap b83025e8ef1b9b3cb5f9dc15b1a0e547a9c837fa26b1a5eec173724932dd0f4b
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0004-display-hap-smoke.log SHA-256 3980ee2f9adcca29cabcfe935fe459cbb0ed5b12748bd0c0157db9b02df1a16d and docs/evidence/raw/EV-R1-EMU24-20260717-0004-emulator-console.log SHA-256 6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5, repository access; runtime access-token IDs in bm dump were replaced by stable nonsecret placeholders; four HOME logs and guest screenCap archived with hashes below, local-user access and 30-day minimum retention
verdict: blocked
verdict_scope: visible rendering and pixelMap availability passed, but standard boot readiness, Ability runtime and Node-API smoke remained blocked; this Emulator research evidence cannot be used for stage exit
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

可见模式的 caller 环境最初没有`DISPLAY`、`WAYLAND_DISPLAY`或`XAUTHORITY`，但`/tmp/.X11-unix/X1`、Xtigervnc `:1`和X authority实测可用；启动时只在helper实际命令上移除`-noWindow`并显式注入`DISPLAY=:1`与X authority，KVM和`-hdcport 10000`保持不变，helper文件未修改。

renderer日志先出现一次`GLFW Error (65543): EGL: Failed to create context: Arguments are inconsistent`，随后成功记录`EGL initialized with provided display: 1.5`、Mesa、OpenGL ES 3.0、`llvmpipe (LLVM 19.1.7, 256 bits)`和`window render start 0`；因此该首个GLFW错误不是本轮renderer失败判据。`uitest screenCap`返回成功，回传PNG为1320x2856、71901字节，人工查看为全黑画面；这证明pixelMap对象可取得，但不证明launcher已经可交互。

根因只得到局部确认：相对于`EV-R1-EMU24-20260717-0002`的`-noWindow`运行，可见X11路径恢复了EGL window和pixelMap，因此无窗口显示路径可解释先前`Failed to get display pixelMap`；它不能解释全部启动失败，因为可见模式下`bootevent.boot.completed`仍缺失且`aa start`继续稳定返回`10106102`。不能把`-noWindow`写成锁屏或Ability失败的唯一根因。

`Emulator --help`只列出`-noWindow`，没有软件GPU或renderer选择参数；实验1并非因无X或renderer失败，所以实验2条件未成立，且help也不支持可合法尝试的软件渲染参数，未猜测或执行任何`swiftshader`参数。日志显示本轮可见模式已由Emulator自动选择llvmpipe，不等于CLI存在可控制的软件GPU模式。

本轮安装前与安装后HAP均保持SHA-256 `f1e00ec5534951137d7149b3d5001e99622aa1daacd1e31618d57699e2a1cd69`，`bm dump`确认bundle `cn.alfadb.netbird.r1probe`与`appSignType: none`；但对象只位于可变、被忽略的Hvigor输出路径，未复制到不可变制品归档。对象ID `EO-R1-EMU24-20260717-0004-HAP-01`只把本次现场安装绑定到记录的hash、大小与mtime，后续路径被覆盖后仍不能逐字节复核本次原始安装对象。

| 子项 | 当前实测 | 判定 |
| --- | --- | --- |
| X11与renderer | Xtigervnc `:1`可达；EGL 1.5、Mesa llvmpipe、window render启动 | PASS |
| 标准readiness | Connected、echo、uname、HarmonyOS通过；`bootevent.boot.completed`为errNum 106，相关bootanimation/launcher/lockscreen键为errNum 1002 | FAIL |
| pixelMap | `screenCap`成功并回传1320x2856 PNG；画面全黑 | PASS，仅限pixelMap取得 |
| unsigned安装与bundle | 安装成功；bundle与`appSignType: none`确认 | PASS，仅限该研究Emulator |
| Ability启动 | 首次及wake/swipe后重试均为`10106102` | BLOCKED |
| `R1Api24Probe` | `hilog -x -T R1Api24Probe`无输出 | NOT VERIFIED |
| `ping`与`version` | 未观察到`ping=pong`或`r1-api24-probe/0.0.1` | NOT VERIFIED |
| 清理与停机 | force-stop、卸载、卸载后absence、stop helper、进程退出、HDC停止、端口和tmux清理、根`.hvigor`缺失均确认 | PASS |

`EV-R1-EMU24-20260717-0004` 的现场归档为 `[HARMONY_HOME]/emulator-log-archive/20260716T173858Z-r1-api24-display-exp1-rerun`：`Emulator.log` SHA-256 `8caa7176ade347bbcfd148580065525ffff2ecbfcd3ffb271d8cce447f11a9f7`，`qemu.log` SHA-256 `08f068ee55c7531e41b2f76a382e3c575d6313424a2e07f339c8e7d6f241eb2a`，`kernel.log` SHA-256 `5d10d0d275d48704e56868e502b4152d25a741aef84ed55a5ec631ac2fea33f7`，`crash_server.log` SHA-256 `b4ae30b8a6fd1e3b35e8983a14e105e0b41d3ed93d293d93e6736e11f333a711`，`guest-screenCap.png` SHA-256 `b83025e8ef1b9b3cb5f9dc15b1a0e547a9c837fa26b1a5eec173724932dd0f4b`。

## API 24 aa test Node-API 探针

```yaml
evidence_id: EV-R1-EMU24-20260717-0005
information_status: current-measured
record_status: collected
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK/API 24 TestRunner, AbilityDelegator, Node-API and HiLog runtime surface; UIAbility and physical-device SysCap behavior unverified
  channel: N/A; unsigned short-lived application and test HAPs, not distribution candidates
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted non-generated probe code and configuration manifest SHA-256 9516415543861be9e01ab01078499f7daa2ef69b40cbb2cf781eafd8a921876f because no commit was requested; Markdown excluded from this executable-input manifest
upstream_sha: N/A; this probe contains no NetBird or Go code
toolchain: Debian GNU/Linux 13 x86_64 host; Command Line Tools 6.1.1.290; SDK 6.1.1.125/API 24; Node.js 18.20.1 through stable wrappers; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e for every target operation; Xtigervnc X11 display :1; KVM fd open
working_directory: [WORKSPACE]/spikes/r1-api24-hap for build and [WORKSPACE] for measured runtime
command: Complete replayable transcript is in docs/evidence/raw/EV-R1-EMU24-20260717-0005-aa-test.log; the measured aa command was aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000; DISPLAY=:1, XAUTHORITY, HDC port 10000 and Beta HDC were explicit
input: EO-R1-EMU24-20260717-0005-APP-HAP-01 application HAP size 2585745 and SHA-256 953b4333db8cce882f67e2197a871a5092eb3927f05c5278dd03f0b27b439418; EO-R1-EMU24-20260717-0005-TEST-HAP-01 test HAP size 11110 and SHA-256 9158ae10925b23c9b58984ffea3341a1f2c613897f9bec5b068e4bf94b8e30b9; cold visible Emulator; no initial bundle or runtime residue
expected: cold-build both unsigned HAPs; obtain bounded shell readiness and pixelMap; install both modules together; run the device-help-confirmed Stage aa test command without starting a window or Ability; observe Node-API initialization, ping=pong, version=r1-api24-probe/0.0.1 and result code 0; uninstall and normally stop
actual: final clean plus application and test builds passed; visible Emulator reached Connected, shell, uname and HarmonyOS readiness in 26 seconds from launch and produced a pixelMap while boot.completed was initially absent; both fixed-hash HAPs installed together; aa test executed and returned TestFinished-ResultCode 0 with exact ping and version; dedicated HiLog confirmed module initialization, Runner preparation, ping invocation and assertions in PID 2964; the Runner process then exited; cleanup, uninstall, normal stop and residue removal passed
started_at: 2026-07-17T01:54:43+08:00
ended_at: 2026-07-17T02:00:14+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, HAP mtimes, guest HiLog timestamps and Emulator logs
artifact_sha256: EO-R1-EMU24-20260717-0005-APP-HAP-01 953b4333db8cce882f67e2197a871a5092eb3927f05c5278dd03f0b27b439418; EO-R1-EMU24-20260717-0005-TEST-HAP-01 9158ae10925b23c9b58984ffea3341a1f2c613897f9bec5b068e4bf94b8e30b9; application output metadata 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c; test output metadata 76f9e1d064062a6e7d9a59adb4cbda96fb3ab4c710d9b6b016e9659e9941013a; tracked ohpm lock f201c717ddeb6fedbbc57414a57534427fa5f7efa940c495660b516871d82544
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0005-aa-test.log SHA-256 7b7c3becfca7f0e89018f77c314ea29dcefca94b38e4263f2e11a0607afcb309 and docs/evidence/raw/EV-R1-EMU24-20260717-0005-emulator-console.log SHA-256 6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5, repository access; HOME instance logs remained cumulative mutable files and were not represented as immutable archives
verdict: pass
verdict_scope: only the API 24 x86_64 Emulator test-framework process loaded and invoked this ordinary C++ Node-API probe; research evidence cannot be used for UIAbility, NetBird, Go, VPN, physical-device, signing, channel, product-support or stage-exit claims
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

本轮优先采用本地 API 24 官方 schema 和 Hvigor 6.24.3 源码。第一次测试模块草案使用 `testRunner.srcEntry`，`PreBuild` 以 `00303038` 明确拒绝并要求 `srcPath`；改为 `srcPath` 后完整构建通过。`TestRunner`、`AbilityDelegator.printSync` 和 `finishTest` 足够完成直接断言与结果回传，因此没有查询或引入 `@ohos/hypium`，锁文件仍只包含本地 `libprobe.so` 类型包。

`entry_test` 只有 `OpenHarmonyTestRunner`，没有 TestAbility、页面或窗口。测试 HAP 的 ABC 包含 Runner、`libprobe` 导入、`pong` 和固定版本断言；native 库由同 bundle 的应用 entry HAP 提供，因此按照本机 Hvigor 官方执行器源码把应用 HAP 与测试 HAP 推入同一目录并用一次 `bm install -p` 安装。没有采用 ServiceExtension、AppService、企业 ACL、调试解锁或系统安全策略绕过。

| 子项 | 当前实测 | 判定 |
| --- | --- | --- |
| 本地 schema | `srcEntry` 被拒绝，`srcPath` 通过 API 24 schema 与 Hvigor 6.24.3 完整构建 | PASS |
| 外部测试依赖 | 无 Hypium 或其他外部依赖；tracked lock hash 未变化 | PASS |
| 冷构建 | 单次 `clean` 后应用 HAP 和 `entry_test` HAP 分别报告 `BUILD SUCCESSFUL` | PASS |
| 可见 runtime 前置 | 26 秒内 Connected、echo、uname、HarmonyOS 和 pixelMap 通过；`boot.completed` 初始缺失，测试后变为 `true` | PASS，仅限本实验继续条件；不重写标准 readiness 历史 |
| 双 HAP 安装 | `bm install -p` 报告成功，`bm dump` 确认 entry、entry_test、x86_64 native 路径与 `appSignType: none` | PASS，仅限该研究 Emulator |
| `aa test` | 准确命令实际执行，host RC 0，`TestFinished-ResultCode: 0` | PASS |
| Node-API | HiLog 确认 initialized 和 ping invoked；控制台与 HiLog 均确认 `ping=pong`、固定版本和断言通过 | PASS，仅限 TestRunner 进程 |
| 短生命周期 | PID 2964 完成 `finishTest` 后不再出现在 guest process 或 aa dump | PASS |
| UIAbility | 本轮没有启动或重测 EntryAbility | NOT IN SCOPE |
| 清理与停机 | guest 临时文件删除、force-stop、卸载、bundle absence、正常 stop、端口/HDC/实验 tmux/根 `.hvigor` 清理完成 | PASS |

根因判别只推进到路径隔离：`EV-R1-EMU24-20260717-0005` 证明同一 API 24 x86_64 Emulator 的测试框架进程能够加载 `libprobe.so` 并正确执行两个 Node-API 函数，因此 0004 未出现 Node-API 日志不能再归因于该探针在所有进程中都存在固有 loader 或函数失败；0004 的普通 `EntryAbility` 仍在执行应用代码前返回 `10106102`，本轮既没有解释该策略结果，也没有证明 UIAbility 路径可用。`boot.completed` 从测试前缺失变为测试后 `true` 只是时序观测，没有证据把变化归因于 `aa test`。

本轮安装对象位于 ignored、可变的 Hvigor 输出路径。对象 ID 只把现场安装绑定到记录的大小、mtime 和 SHA-256；后续构建覆盖路径后不能逐字节复核本次对象，且本轮没有创建不可变 HAP 归档。两个 repository raw 日志是持久记录；`[INSTANCE]/Log` 的四个文件是累计可变日志，只在停止后记录现场 hash，没有把它们虚构成不可变归档。

## API 24 x86_64 Go c-shared runtime 探针

```yaml
evidence_id: EV-R1-EMU24-20260717-0006
information_status: current-measured
record_status: collected
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; Go artifact linux/amd64 GOAMD64=v1
  sdk_api_syscap: SDK/API 24 TestRunner, Node-API, HiLog, dlopen and loopback socket surface; Go runtime and netpoll were not reached
  channel: N/A; unsigned short-lived application and test HAPs, not distribution candidates
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted non-generated non-Markdown spike source and configuration manifest SHA-256 f5c2cab083c5a1d5c40379b2cfcb17020410073cf11366c75ee7f09800655d0c because no commit was requested
upstream_sha: N/A; the probe uses the fixed Go 1.25.12 toolchain and standard library but contains and modifies no NetBird, WireGuard, Go runtime or other upstream source
toolchain: Debian GNU/Linux 13 x86_64 host on Linux 7.0.14-4-pve; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; OHOS clang 15.0.4; Go 1.25.12; Node.js 18.20.1 through stable wrappers; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e for every target operation; DISPLAY=:1; KVM fd open
working_directory: [WORKSPACE]/spikes/r1-api24-hap for build and [WORKSPACE] for measured runtime
command: Complete replayable command transcript is docs/evidence/raw/EV-R1-EMU24-20260717-0006-go-runtime-probe.log; Go build uses explicit HARMONYOS_NATIVE_HOME, GO_BIN and GO_PROBE_OUTPUT_DIR without current; runtime uses fixed Beta HDC, DISPLAY=:1, target 127.0.0.1:10000 and aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000
input: EO-R1-EMU24-20260717-0006-GO-SO-01 size 3136128 and SHA-256 236db2d637b18a9fcb08bf44150152c2975274eaf7934853295420b9d4c980d9; EO-R1-EMU24-20260717-0006-APP-HAP-01 size 5843130 and SHA-256 37a42fd905a9f1675e943d55361ecafac75acef70d40d6ffac7e467533544a0f; EO-R1-EMU24-20260717-0006-TEST-HAP-01 size 3149616 and SHA-256 9218a8d51b0ca3588c4bb31e6a35f109d86b2b5a48ce128f6158cd8abec04c2a; cold visible Emulator; no initial bundle or runtime residue
expected: build an x86_64 Go 1.25.12 c-shared ELF; preserve exact input hash in both HAP members; load with RTLD_NOW|RTLD_LOCAL; return Hello=42, RuntimeProbe(65536)=65536 and NetDialProbe=0 against a temporary 127.0.0.1 listener; report TestFinished-ResultCode 0 without crash
actual: fixed Go build, ELF checks, cold dual-HAP build, input/member hash identity, visible readiness, dual-HAP install and Node-API prelude passed; ELF contains STATIC_TLS and NODELETE; dlopen returned a controlled error before any Go export ran because res_search initial-exec TLS resolves to a dynamic definition in libgoprobe.so; TestFinished-ResultCode was 1 while the HDC host command returned 0; no crash occurred; runtime, goroutine, channel, timer, allocation and netpoll were not executed
started_at: 2026-07-17T02:20:30+08:00
ended_at: 2026-07-17T02:29:24+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, generated artifact mtimes, guest HiLog timestamps and Emulator logs; started_at is the earliest retained final Go output mtime rounded to seconds rather than a separately sampled command-start instant
artifact_sha256: EO-R1-EMU24-20260717-0006-GO-SO-01 236db2d637b18a9fcb08bf44150152c2975274eaf7934853295420b9d4c980d9; EO-R1-EMU24-20260717-0006-APP-HAP-01 37a42fd905a9f1675e943d55361ecafac75acef70d40d6ffac7e467533544a0f; EO-R1-EMU24-20260717-0006-TEST-HAP-01 9218a8d51b0ca3588c4bb31e6a35f109d86b2b5a48ce128f6158cd8abec04c2a; generated header 3ece605e4d96e747a35511ab5cb3bd1f3f5f6fb2b32ecbe92db3682d1f5a7680; application metadata 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c; test metadata 76f9e1d064062a6e7d9a59adb4cbda96fb3ab4c710d9b6b016e9659e9941013a
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0006-go-runtime-probe.log SHA-256 21c7e6694a06992731a2ab6f7afb4041290f6c04bb20b057d01a743ccc5cac02, docs/evidence/raw/EV-R1-EMU24-20260717-0006-emulator-console.log SHA-256 6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5, docs/evidence/raw/EV-R1-EMU24-20260717-0006-hilog-app-full.log SHA-256 18a5585ad50591da0bbeeab2d4c0c7e9e1ff43413b108cfdf692ed99fcdf7755 and docs/evidence/raw/EV-R1-EMU24-20260717-0006-hilog-filtered.log SHA-256 b9c2f67010a0efc15857b49769d5a24024187faaa06065fd2877b492ee4f62e1, repository access; HOME instance logs remain cumulative mutable observational references
verdict: fail
verdict_scope: this exact Go 1.25.12 Linux/amd64 c-shared ELF cannot be dynamically loaded by the measured API 24 x86_64 application TestRunner process because of its initial-exec TLS relocation; research evidence cannot be generalized to arm64, physical devices, other process models, other Go or link strategies, NetBird, VPN, product support or stage exit
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

`record_status` 保持 `collected`，因为本次执行和原始材料已采集，但尚无正式独立审查记录。判定使用 `TestFinished-ResultCode: 1` 和结构化 `ok=false`，不使用表面为 0 的 HDC host RC。

构建首先暴露并修正了一个证据同一性问题：Hvigor 默认 `DoNativeStrip` 把 3136128 字节输入 so 改写为 2160240 字节 member，首次 member hash 因此不等于输入 hash。该对象未安装；依据本地 Hvigor 6.24.3 schema 只对 `**/libgoprobe.so` 设置 `debugSymbol.exclude` 后重新 clean 和双 HAP 构建，最终应用与测试 HAP member 均逐字节等于输入 so，其他 native 库仍执行 strip。

Go build info 明确记录 `go1.25.12`、`-buildmode=c-shared`、`-trimpath=true`、`CGO_ENABLED=1`、`GOOS=linux`、`GOARCH=amd64` 和 `GOAMD64=v1`。动态段只 `NEEDED libc.so`，`FLAGS` 包含 `SYMBOLIC BIND_NOW STATIC_TLS`，`FLAGS_1` 包含 `NOW NODELETE`，动态导出包含 `Hello`、`RuntimeProbe` 和 `NetDialProbe`。

`STATIC_TLS` 判定从 0001 的“首要静态风险”推进为该精确路径的目标 loader 负面实测：`dlopen` 原始错误是 `Error relocating /data/storage/el1/bundle/libs/x86_64/libgoprobe.so: res_search: initial-exec TLS resolves to dynamic definition in /data/storage/el1/bundle/libs/x86_64/libgoprobe.so; errno=2 (No such file or directory)`。错误在 `RTLD_NOW|RTLD_LOCAL` 重定位阶段同步返回，C++ 保留原文且没有 `dlclose`；`NODELETE` 同时存在于 ELF，但 loader 尚未成功建立句柄，不能把它写成已运行的卸载语义。

| stage | 观测 | 判定 |
| --- | --- | --- |
| Go源码与固定构建 | 三个导出、Go 1.25.12、linux/amd64、GOAMD64=v1、CGO、OHOS x86_64 clang wrapper、trimpath和c-shared均由源码与build info确认 | PASS，仅限构建 |
| ELF | x86-64 ET_DYN；NEEDED仅libc.so；STATIC_TLS和NODELETE存在；三个动态导出存在 | PASS，仅限静态身份；STATIC_TLS是运行阻断信号 |
| HAP member同一性 | 最终应用与测试HAP member均等于输入so哈希236db2d6... | PASS；HAP仍是可变ignored输出，不是归档 |
| Node-API前置 | initialized、ping=pong和固定version均通过 | PASS，仅限TestRunner进程 |
| `dlopen` | RTLD_NOW\|RTLD_LOCAL在initial-exec TLS动态定义处返回原始错误 | FAIL，最后stage=`dlopen` |
| `dlsym Hello`与`Hello=42` | loader未返回句柄 | NOT EXECUTED |
| goroutine/channel/1ms timer/allocation | `RuntimeProbe`未被解析或调用 | NOT EXECUTED |
| loopback listener与`net.DialTimeout` | loader失败发生在listener创建前，`NetDialProbe`未被解析或调用 | NOT EXECUTED |
| runtime/netpoll | 无Go初始化或Go函数执行证据 | NOT EXECUTED |
| crash | 失败经Node-API对象、TestRunner断言和`finishTest`受控返回；guest健康，crash_server无本轮探针crash | PASS，仅限无crash观测 |
| 完整与过滤HiLog | 完整app buffer和双tag过滤日志均持久化；另执行all-buffer capture但临时对象不作为repository制品 | PASS，范围按raw transcript |
| 清理 | guest临时文件、force-stop、卸载、正常stop、Beta HDC、端口和实验tmux均清理 | PASS |

完整app buffer还记录PID 3191在`finishTest`后退出；这与短生命周期TestRunner一致，不是native crash。当前运行使用0005同类的`DISPLAY=:1`可见路径并取得pixelMap，既有0004曾在该路径测得llvmpipe；本轮累计Emulator日志没有新增renderer身份行，因此只记录路径复用，不生成新的llvmpipe版本结论。

本轮增加的是项目内短生命周期探针源码、wrapper、构建配置、Node-API桥接和测试断言，不修改NetBird、WireGuard、Go runtime或其他固定上游，不属于R0定义的上游适配补丁；R2前、R3前、R4至R5、全项目累计补丁计数仍全部为0，不以移动到脚本规避计数，也没有因本轮产生高维护风险补丁。

## API 24 x86_64 无 fork late-load 边界探针

```yaml
evidence_id: EV-R1-EMU24-20260717-0007
information_status: current-measured
record_status: collected
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; E1 and E4 inputs are OHOS x86_64 ELF, while the unchanged E4 dependency is the previously measured Go linux/amd64 GOAMD64=v1 ELF
  sdk_api_syscap: SDK/API 24 TestRunner, Node-API, HiLog and dlopen surface; Go exports, Go runtime and netpoll were not executed
  channel: N/A; unsigned short-lived application and test HAPs, not distribution candidates
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted non-generated non-Markdown spike source and configuration manifest SHA-256 027a03e0fb0c13c67128b683cd48cea0d349262499b63c887c74e7819f92bca3 because no commit was requested
upstream_sha: N/A; E1 and E4 are project-local C/C++ probes, the existing Go object is unchanged from 0006, and no NetBird, WireGuard, Go runtime or other upstream source was built or modified
toolchain: Debian GNU/Linux 13 x86_64 host on Linux 7.0.14-4-pve; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; OHOS clang 15.0.4; Node.js 18.20.1 through stable wrappers; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e for every target operation; DISPLAY=:1; KVM fd open
working_directory: [WORKSPACE]/spikes/r1-api24-hap for build and [WORKSPACE] for measured runtime
command: Complete replayable command transcript is docs/evidence/raw/EV-R1-EMU24-20260717-0007-loader-probes.log; native probe build uses explicit HARMONYOS_NATIVE_HOME and NATIVE_PROBE_OUTPUT_DIR without current; runtime uses fixed Beta HDC, DISPLAY=:1, target 127.0.0.1:10000 and aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000
input: EO-R1-EMU24-20260717-0007-TLS-SO-01 size 10040 and SHA-256 1cc79f8620c8dc0de713bb425904b3e779e7184dc1a0d675b7009fb953f69a6f; EO-R1-EMU24-20260717-0007-NEEDED-SO-01 size 10192 and SHA-256 b6b721663a7f71305b03d6595c4089bc4788e6b578e5111234e8241cbfc68e4a; unchanged GO-SO SHA-256 236db2d637b18a9fcb08bf44150152c2975274eaf7934853295420b9d4c980d9; EO-R1-EMU24-20260717-0007-APP-HAP-01 size 5888248 and SHA-256 6294bf70584ae4429e5618eb1e3d632eea14142b3409d8cdf3899b9d5f0e14db; EO-R1-EMU24-20260717-0007-TEST-HAP-01 size 3175007 and SHA-256 bb561e6baa801ccb3c7081901a5c726a9a65dc80a6a3973eb7baa53b23b30990; cold visible Emulator; no initial bundle or runtime residue
expected: first pass the existing Node-API baseline; E1 must either load and return GetTLS=42 or return the exact controlled initial-exec TLS loader rejection as functional BLOCKED; E4 must either load the verified libneededprobe.so to libgoprobe.so chain without calling Hello or RuntimeProbe or return the exact controlled dependency loader rejection as functional BLOCKED; either valid observation may produce TestFinished-ResultCode 0 without representing blocked functionality as passed
actual: cold build, application HAP member identity, visible readiness, dual-HAP install and Node-API baseline passed; E1 returned functional=BLOCKED at dlopen with tlsValue=-1 and did not execute GetTLS; E4 returned functional=BLOCKED at dlopen while loading libgoprobe.so through the verified DT_NEEDED chain, dependencyChainLoaded=false, helloCalled=false and runtimeProbeCalled=false; TestFinished-ResultCode was 0 because both expected loader observations were asserted without crash
started_at: 2026-07-17T03:03:43+08:00
ended_at: 2026-07-17T03:08:59+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, generated artifact mtimes, guest HiLog timestamps and Emulator pane exit timestamp; started_at is the earliest retained final native probe output mtime rounded to seconds
artifact_sha256: EO-R1-EMU24-20260717-0007-TLS-SO-01 1cc79f8620c8dc0de713bb425904b3e779e7184dc1a0d675b7009fb953f69a6f; EO-R1-EMU24-20260717-0007-NEEDED-SO-01 b6b721663a7f71305b03d6595c4089bc4788e6b578e5111234e8241cbfc68e4a; unchanged GO-SO 236db2d637b18a9fcb08bf44150152c2975274eaf7934853295420b9d4c980d9; libprobe.so application member b1a343491e07546f2fa8b824e6e211ad30916128e98094cbdff0828f7b59a9f1; EO-R1-EMU24-20260717-0007-APP-HAP-01 6294bf70584ae4429e5618eb1e3d632eea14142b3409d8cdf3899b9d5f0e14db; EO-R1-EMU24-20260717-0007-TEST-HAP-01 bb561e6baa801ccb3c7081901a5c726a9a65dc80a6a3973eb7baa53b23b30990; application metadata 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c; test metadata 76f9e1d064062a6e7d9a59adb4cbda96fb3ab4c710d9b6b016e9659e9941013a
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0007-loader-probes.log SHA-256 bf5c3b604cb31fd3b05767687fab2514e72289e55ad34492a03fa0e66acd70ea, docs/evidence/raw/EV-R1-EMU24-20260717-0007-emulator-console.log SHA-256 326b477bfaf2703ab1c2378586e53610daedf72f6c1413400d8b35e208c0ee42 and docs/evidence/raw/EV-R1-EMU24-20260717-0007-hilog-app-full.log SHA-256 47d7f47f6e9f6df7e01ad4c3cd408e44375a5974d7e540db864e96a3cdf5df64, repository access; HOME instance logs remain cumulative mutable observational references
verdict: blocked
verdict_scope: direct and transitive no-fork late-load variants are blocked for these verified ELF inputs in this exact API 24 x86_64 TestRunner process; this is not a verdict on arm64, physical devices, process-startup linkage, a separate process, a patched Go runtime or toolchain, another loader, NetBird, VPN, product support or stage exit
reviewer: pending formal independent review
reviewed_at: pending
review_record: pending
```

`record_status` 保持 `collected`，因为执行与原始材料已采集但尚无正式独立审查；`verdict: blocked` 表示被测功能边界被 loader 阻断，而 `TestFinished-ResultCode: 0` 只表示 baseline、结构化观测和无崩溃断言完成，不能改写 E1 或 E4 的 `functional=BLOCKED`。

E1 的最终 ELF 是普通 OHOS C shared object，含 4 字节 `PT_TLS`、`R_X86_64_TPOFF64`、`STATIC_TLS` 和导出 `GetTLS`，只 `NEEDED libc.so`，没有 Go、`res_search` 动态符号、`NODELETE` 或 `SYMBOLIC`；初始默认 emulated TLS 中间物因没有真实 TLS relocation 而在打包前作废，最终对象显式使用 `-fno-emulated-tls -ftls-model=initial-exec`。loader 原文为 `Error relocating .../libtlsprobe.so: __deregister_frame_info: initial-exec TLS resolves to dynamic definition in .../libtlsprobe.so`，`GetTLS` 没有执行；`R_X86_64_TPOFF64` 的符号索引为 0，因此错误前缀中的函数名只按 loader 原始诊断保留，不把它误写成 TLS 变量身份。

E4 的最终 ELF 明确按顺序包含 `NEEDED libgoprobe.so` 与 `NEEDED libc.so`、`RUNPATH $ORIGIN`、未定义直接引用 `Hello` 与 `RuntimeProbe` 以及导出 `CallNeededProbeExports`。`runNeededProbe()` 只执行 `dlopen("libneededprobe.so", RTLD_NOW | RTLD_LOCAL)`，未执行 `dlsym` 或任何 E4/Go 函数；loader 沿 runtime dependency chain 到达 `libgoprobe.so` 后返回 0006 同类原文，故 `dependencyChainLoaded=false`、`Hello=NOT CALLED`、`RuntimeProbe=NOT CALLED`。这是 NAPI/TestRunner 运行时 `dlopen` 传递依赖实验，不是进程启动期 `DT_NEEDED`，也不能表述为启动期静态 TLS 结论。

| 检查项 | 观测 | 功能判定 |
| --- | --- | --- |
| Node-API baseline | `ping=pong`且version固定值匹配 | PASS，仅限TestRunner baseline |
| E1 ELF前提 | 真实`PT_TLS`、`R_X86_64_TPOFF64`、`STATIC_TLS`、`GetTLS`；无Go、res_search、NODELETE、SYMBOLIC | PASS，仅限实验输入身份 |
| E1 `dlopen` | ordinary shared object在initial-exec TLS动态定义处返回受控原始错误 | BLOCKED |
| E1 `GetTLS=42` | loader未返回句柄 | NOT EXECUTED |
| E4 ELF前提 | `NEEDED libgoprobe.so`、`RUNPATH $ORIGIN`、Hello/RuntimeProbe直接引用均存在 | PASS，仅限实验输入身份 |
| E4 runtime dependency chain | `dlopen libneededprobe.so`沿链到`libgoprobe.so`后在同类TLS重定位处返回错误 | BLOCKED |
| E4 Go exports | `helloCalled=false`且`runtimeProbeCalled=false` | NOT CALLED |
| HAP同一性 | app HAP内libprobe member等于Hvigor stripped输出，两个HAP内三个外部x86_64 so均等于输入哈希 | PASS，仅限打包身份 |
| TestRunner控制面 | `TestFinished-ResultCode=0`且PID 3037在finishTest后正常退出 | PASS，仅限观测断言与无crash |
| 清理 | bundle、guest staging、Emulator、qemu、Beta HDC、端口和实验tmux均无残留 | PASS |

E1 排除了 0006 由 Go、`res_search`、`NODELETE`、`SYMBOLIC` 或 E4 依赖包装本身造成拒绝的必要性：只保留真实 initial-exec TLS 的普通 late-loaded so 仍被同一 loader 拒绝。E4 则验证把 Go so 从直接 `dlopen` 改为正确 `$ORIGIN` `DT_NEEDED` wrapper 并不会把它变成进程启动依赖，运行时传递加载仍在 Go so 的 initial-exec TLS 重定位处失败。两者合起来覆盖该进程内无 fork late-load 的直接与传递入口，足以把这一精确路线边界登记为 blocked，但不覆盖独立进程、进程启动期链接、patched Go、其他 ABI、其他 loader 或真机。

本轮没有构建 Go、netgo 或 c-archive，没有修改 NetBird、WireGuard、Go runtime 或其他固定上游；新增对象是项目内研究探针，不计上游适配补丁，R2 前及全项目累计补丁数仍为 0。尽管当前没有高维护补丁或预算超限，继续选择独立进程会改变跨语言与生命周期边界，选择 patched Go 会引入潜在高维护 runtime/toolchain fork；依据路线图“涉及跨语言边界的重大调整须重新进行 T0 讨论”和 R1 负面 ABI/libc 结果条款，在实现任何替代路线前必须完成 T0 路线讨论。

| 字段 | 内容 |
| --- | --- |
| 调整 ID | `ADJ-20260717-0001` |
| 日期与时区 | `2026-07-17T03:08:59+08:00` |
| 提出角色 | 执行代理 |
| 触发证据 | `EV-R1-EMU24-20260717-0007` |
| 调整原因 | 普通initial-exec TLS直接late-load与经`DT_NEEDED libgoprobe.so`传递late-load均被目标loader拒绝，无fork同进程late-load边界已无未测入口 |
| 调整内容 | 先停止该精确 API 24 x86_64 TestRunner 无 fork late-load 路线；T0 后按公开能力门、B0 无 TLS、B1 initial-exec TLS 串行执行，B1 失败不进入 child Go；API 24 phone 能力门未通过时仅暂停该 phone B 族，A 工具链仍须另一次 T0 定 timebox |
| 受影响阶段 | R1、R2及后续依赖Go跨语言边界的阶段 |
| 已评估替代方案 | B 族 Native Child、满足前置时的公开 exec/PIE 与固定 Go executable、A 族工具链适配；patched Go/runtime fork 不作为默认后继，全部成功都不自动形成产品承诺 |
| R0/SLO/补丁预算影响 | R0、R1、R2 均保持未退出，SLO 不变，补丁计数仍为 0；负面只绑定 API 24 x86_64 phone Emulator，arm64 保持 provisional |
| T0 判定 | 已一致；完整 C1-C9 见 `docs/roadmap.md`，首个公开 phone 能力门由 `EV-R1-EMU24-20260717-0008` 执行并 blocked |
| 生效条件与回退条件 | 0008 因公开 API 将 phone 归入非 PC/2in1/Tablet 的其他设备类型而在实现前暂停 B0/B1/child Go，API 24 phone B 族补丁数为 0；华为商用 HarmonyOS 具名真机行为或公开 SDK 设备范围变化须以新证据重新核验能力门后方可重开，Tablet/2in1 不属于当前 phone 目标；只有新 T0 明确 A 族 timebox 后才能启动工具链 spike |
| 审查状态 | `anthropic/claude-opus-4-8` 独立审查于 `2026-07-17T04:27:50+08:00` 通过；无 blocker 或 major，MINOR 表述已修复 |

## API 24 phone Native Child 公开能力门

```yaml
evidence_id: EV-R1-EMU24-20260717-0008
information_status: current-measured
record_status: reviewed-pass
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: OpenHarmony 6.1.1(24) API 24 Emulator research image; guest behavior is self-consistent with the reviewed public OpenHarmony documentation, while commercial Huawei HarmonyOS phone is a separate provisional distribution
  device: netbird_api24_phone Emulator; explicitly a phone and not a named physical device
  full_system_version: OpenHarmony-6.1.1.125; image software emulator 6.1.0.125(SP9DEVC00E120R4P11)
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: SDK/API 24 Native Child Process public compile surface has SystemCapability.Ability.AbilityRuntime.Core, but the public API reference maps phone, as an other device type outside PC/2in1 and Tablet, to NCP_ERR_NOT_SUPPORTED(801); Node-API baseline only was executed
  channel: N/A; unsigned short-lived application and test HAPs, not distribution candidates
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted non-generated non-Markdown spike source and configuration manifest SHA-256 c080f0833326612e9369536c6a8076f163c897155634df1901400334985e66d9 because no commit was requested
upstream_sha: OpenHarmony public docs HEAD b84779874332bd7c2afee0ca3875494fa4793f1f; capi-native-child-process-h.md content blob 2eb9308e9b4d043a9d1dca44738212edc768db9d; capi-nativechildprocess-development-guideline.md content blob 5719a9112e1797d5ec8e7e188c6ef89018473b0c; no NetBird, WireGuard, Go runtime or other upstream source was built or modified
toolchain: Debian GNU/Linux 13 x86_64 host; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; Node.js 18.20.1 through stable wrappers; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e for every target operation; DISPLAY=:1; KVM fd open
working_directory: "[WORKSPACE]/spikes/r1-api24-hap for build and [WORKSPACE] for public-doc review and measured runtime"
command: Complete replayable transcript is docs/evidence/raw/EV-R1-EMU24-20260717-0008-native-child-public-gate.log; runtime used fixed Beta HDC, target 127.0.0.1:10000 and aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000; OH_Ability_StartNativeChildProcess and OH_Ability_CreateNativeChildProcess were not called
input: public OpenHarmony Native Child API reference and development guide at the recorded HEAD; local API 24 native_child_process.h SHA-256 cc686fe9a38a0b71411a6ce33ab3e1420c9c32c7d6ca89dcd4dee91cedca7c62; PermissionDefinitions.json SHA-256 73b8cb0010fcc6e8c8fb825434c671cfc2ebe04892419f56c445aa66e505d47d; application HAP size 5888248 and SHA-256 701c892ee14642c9fc219fc720b7c74f3fc011fccdea0998902396349f3567c5; test HAP size 3168217 and SHA-256 257c4ad70a805e41d36f35ad442c876dafb561ac380e1dc6ff19a8c83b12ed7c
expected: prove from both public documentation and the API 24 SDK whether ordinary third-party phone applications may use Native Child Process without debug, system or enterprise privileges; only if phone support is public, run no-TLS B0 and then IE-TLS B1; always pass the ordinary Node-API baseline first
actual: public API reference confirms a public ordinary-application API but explicitly limits API 14 and later to PC/2in1 and Tablet, mapping phone as an other device type outside PC/2in1 and Tablet to NCP_ERR_NOT_SUPPORTED(801); API 24 header confirms the public symbol, SysCap and enum but does not widen device support; the OpenHarmony guest behavior is self-consistent with that public documentation, while commercial Huawei HarmonyOS phone remains a separate provisional distribution; permission definitions and module schemas expose no Native Child permission or phone-enabling manifest field; installed bundle was a normal non-system PHONE-001 app with allowMultiProcess=false and only INTERNET requested; Node-API baseline passed; B0, B1, child Go and A toolchain were not run
started_at: 2026-07-17T03:58:01+08:00
ended_at: 2026-07-17T04:04:44+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, HAP mtimes, guest HiLog timestamps and Emulator pane exit timestamp
artifact_sha256: application HAP 701c892ee14642c9fc219fc720b7c74f3fc011fccdea0998902396349f3567c5; test HAP 257c4ad70a805e41d36f35ad442c876dafb561ac380e1dc6ff19a8c83b12ed7c; application metadata 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c; test metadata 76f9e1d064062a6e7d9a59adb4cbda96fb3ab4c710d9b6b016e9659e9941013a; packaged libprobe.so b1a343491e07546f2fa8b824e6e211ad30916128e98094cbdff0828f7b59a9f1
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0008-native-child-public-gate.log SHA-256 416776ae79794567bdb1e6b965cd98027642ff18f5fe34d0d339540cc60463d4, docs/evidence/raw/EV-R1-EMU24-20260717-0008-emulator-console.log SHA-256 b7387a022cb42e47bcf530638704b3a3bd37cb4d525eaf74a901172a1994747f and docs/evidence/raw/EV-R1-EMU24-20260717-0008-hilog-app-full.log SHA-256 976f71aa39683e19b8aed3e1d7fede6ab7b6a4b38906dbde0244b29abcb47652, repository access
verdict: blocked
verdict_scope: the public Native Child Process route is unsupported for the API 24 OpenHarmony Emulator phone device type, so B0/B1 were paused before implementation or invocation; this is not a runtime 801 observation, is not a verdict on commercial Huawei HarmonyOS phone, and requires a fresh capability-gate check when a named Huawei commercial HarmonyOS device is available or the public SDK device scope changes; it is not a verdict on PC/2in1, Tablet, arm64, public exec/PIE, patched Go, another toolchain, NetBird, VPN, product support or stage exit
reviewer: anthropic/claude-opus-4-8 independent evidence review
reviewed_at: 2026-07-17T04:27:50+08:00
review_record: Independent evidence review verified all key hashes, public documentation, API 24 header, HAPs and raw logs; no blocker or major finding; MINOR wording corrected
```

`record_status` 已更新为 `reviewed-pass`：正式独立审查已核验关键哈希、公开文档、API 24 头文件、HAP 和原始日志，未发现 blocker 或 major 问题，MINOR 表述已修复。`verdict: blocked` 来自实现前公开设备范围门，不是把 `TestFinished-ResultCode: 0` 或 Node-API baseline 改写成 Native Child 成功，也不虚构一次没有发生的 API 运行返回。

| 检查项 | 观测 | 判定 |
| --- | --- | --- |
| 公开 API 身份 | 最新公开 OpenHarmony API 参考和开发指导列出 `OH_Ability_StartNativeChildProcess`、头文件、库、入口与普通应用工程步骤；API 24 SDK 头文件给出同一符号、`SystemCapability.Ability.AbilityRuntime.Core` 和 API 13 起始版本 | PASS，仅证明公开 SDK 面 |
| phone 设备范围 | 公开 API 参考明确写明 API 14 及以后仅 PC/2in1、Tablet 正常使用，phone 作为非 PC/2in1/Tablet 的其他设备类型返回 `NCP_ERR_NOT_SUPPORTED` | BLOCKED，映射 `801` |
| 权限与 manifest | 公开步骤未声明 Native Child 权限、debug/system/enterprise 前置或 phone 开关；API 24 `PermissionDefinitions.json` 无 Native Child 权限，module/Hvigor schema 无 `multiProcess` 或 `allowMultiProcess` 字段 | PASS，仅证明没有可合法添加的 phone 解锁配置 |
| 普通应用身份 | guest `bm dump` 为 `PHONE-001`、`appPrivilegeLevel=normal`、`isSystemApp=false`、`appSignType=none`、`allowMultiProcess=false`，只请求 `INTERNET` | PASS，且未改系统策略 |
| 冷构建与双 HAP | 一次 clean 后应用与测试 HAP 均 `BUILD SUCCESSFUL`，`isSigned:false`，hash 和 mtime 在安装前固定 | PASS，仅限 unsigned 研究制品 |
| child 制品 | build 树和两个 HAP 均无 B0/B1 或 child ELF，Runner ABC 明确包含 gate 结果且不再调用 E1/E4 | NOT BUILT，符合前置门 |
| B0 ELF 前提与运行 | 无 B0 输入，因此无 PT_TLS/STATIC_TLS/TPOFF 检查、PID、entry 或 result | NOT RUN |
| B1 ELF 前提与运行 | 无 B1 输入，因此无真实 initial-exec TLS、`GetTLS=42` 或 loader 失败观测 | NOT RUN |
| Go child 与 A 工具链 | 未执行 child Go `dlopen`、公开 exec/PIE、固定 Go executable、patched Go 或工具链 spike | NOT RUN |
| Node-API baseline | `ping=pong`、版本匹配、`TestFinished-ResultCode: 0`，完整 HiLog 记录 Runner PID 3128 正常结束 | PASS，仅限 TestRunner baseline |
| 清理 | guest staging、screenCap、进程、bundle、Emulator、qemu、Beta HDC server、10000/8710 监听和实验 tmux 均无残留 | PASS |

本轮没有向 `module.json5` 或 `app.json5` 添加字段，因为官方 schema 不提供能把 Native Child 扩展到 phone 的合法字段；`bm dump` 的 `allowMultiProcess=false` 是设备安装态观测，不是可由普通应用清单请求的权限。把 device type 改成 Tablet、打开系统多进程策略、提升系统或企业权限、使用 debug 绕过、直接 fork/exec，都会改变被测边界并违反本轮公开普通 phone 应用前提，因此均未尝试。

当前应用和测试 HAP 仍会打包 0006/0007 遗留在 ignored `entry/libs/x86_64` 中的 `libgoprobe.so`、`libtlsprobe.so` 和 `libneededprobe.so`，但 0008 Runner ABC 不含 `runTlsProbe` 或 `runNeededProbe` 调用，完整 HiLog 也没有这些库的加载记录；它们不是 B0/B1 输入，不构成 child Go 执行。两个 HAP 与三份 raw 日志都是可变研究对象，不是签名、渠道或产品制品。

`ADJ-20260717-0001` 的新 T0 共识已在实现前得到执行：公开 phone 能力门未通过，所以 B0 不构建，B1 不运行，child Go 不运行；该 API 24 phone B 族仅暂停，补丁数为 0。华为商用 HarmonyOS 具名真机行为或公开 SDK 设备范围变化可触发重新核验能力门并重开，不构成全局永久关闭；Tablet/2in1 仍为公开文档支持范围，但不属于当前 phone 目标。由于 B0 不可用，公开 exec/PIE 与固定 Go executable 的后继条件也未触发；若要启动 A 工具链 spike，必须由另一次 T0 先明确 timebox、退出标准和维护风险，不能由本记录自动进入。

## API 24 x86_64 纯 C 动态 TLS loader Tier1 探针

```yaml
evidence_id: EV-R1-EMU24-20260717-0009
information_status: current-measured
record_status: reviewed-pass
stage: R1
related_stages: [R0, R2]
target_tuple:
  distribution: OpenHarmony 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone Emulator; explicitly not a named physical device
  full_system_version: OpenHarmony-6.1.1.125; image software emulator 6.1.0.125(SP9DEVC00E120R4P11)
  architecture: x86_64 guest on x86_64 host; all four TLS inputs are x86_64 ET_DYN
  sdk_api_syscap: SDK/API 24 TestRunner, Node-API, HiLog, pthread, dlopen, classic __tls_get_addr and TLSDESC loader/libc surface; Go runtime and NetBird were not built or executed
  channel: N/A; unsigned short-lived application and test HAPs, not distribution candidates
code_sha: baseline Git commit 3f47a140be8b27d6361b3f19abc04d45730ad714; uncommitted executable-input manifest SHA-256 be1c25398fb12d1ac3613247953a65b61b7f7062168964bbb13cc309bb496d9d because no commit was requested
upstream_sha: N/A; project-local pure C/C++/ArkTS probe only; Go issue #71953, Go CL 644975, Go CL 696635 and Go PR 75048 were checked read-only as still open or NEW, unmerged and unreleased next-gate references, not implementation inputs
toolchain: Debian GNU/Linux 13 x86_64 host; GCC 14.2.0 freestanding PIC object compiler; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; OHOS clang/lld 15.0.4 final linker with --no-relax; Node.js 18.20.1 through stable wrappers; Hvigor 6.24.3; ohpm 6.1.2.285; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; DISPLAY=:1; KVM fd open
working_directory: "[WORKSPACE]/spikes/r1-api24-hap for implementation/build and [WORKSPACE] for measured runtime"
command: Complete replayable transcript is docs/evidence/raw/EV-R1-EMU24-20260717-0009-dynamic-tls-loader.log; runtime used fixed Beta HDC, target 127.0.0.1:10000 and aa test -b cn.alfadb.netbird.r1probe -m entry_test -s unittest /ets/testrunner/OpenHarmonyTestRunner -s timeout 15000
input: libtls-ie.so size 2704 SHA-256 b8dee0046cc580339b8a09a79673c81bc75218f90819fdd9069b6b82fb08a674; libtls-gd.so size 3144 SHA-256 c00f4b8861f8c7598f76851cf6d4b90d95a0b03e6e51f58da5213586f1480946; libtls-desc.so size 2752 SHA-256 336357920dd412db0613d88d223229751f31f3e34cd2b85f051162f75e614834; libtls-ld.so size 3112 SHA-256 093fa560431152d21a0323fe9f96bc49edfec7a9983e69cc19e95461f54bec54; application HAP size 2740592 SHA-256 368bdfcde54a35867c5acc3cc5f5a0c2c6a5f865b585176831eba71a16842ada; test HAP size 28974 SHA-256 054b94fe0c3c3b58bc45682f944dc9d77211b856636519beb08e7e55f89a62eb
expected: pass ordinary Node-API baseline; require IE to reproduce the 0007-class RTLD_NOW|RTLD_LOCAL rejection or stop as environment drift; for classic GD and TLSDESC create a waiting thread before dlopen, create another after dlopen, and require main plus both threads to observe initial 42, use different values for 100 SetTLS/GetTLS cycles, reset to 42 and never cross-contaminate; Tier1 passes when classic GD or TLSDESC passes; local-dynamic is non-gating
actual: final readelf and llvm-objdump proved IE PT_TLS plus TPOFF64 plus STATIC_TLS, classic GD DTPMOD64 plus DTPOFF64 plus __tls_get_addr with no TPOFF/STATIC_TLS, TLSDESC final R_X86_64_TLSDESC with no TPOFF/STATIC_TLS, and local-dynamic DTPMOD64 plus __tls_get_addr; cold build and all HAP member identity checks passed; baseline passed; IE returned the exact initial-exec dynamic-definition rejection; classic GD, TLSDESC and local-dynamic each loaded and all three roles completed initial 42, 100 isolated set/read cycles and reset 42 with distinct values; TestFinished-ResultCode was 0 and no crash occurred
started_at: 2026-07-17T04:56:49+08:00
ended_at: 2026-07-17T05:07:01+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, final input/HAP mtimes, guest HiLog timestamps and Emulator stop timestamp
artifact_sha256: libtls-ie.so b8dee0046cc580339b8a09a79673c81bc75218f90819fdd9069b6b82fb08a674; libtls-gd.so c00f4b8861f8c7598f76851cf6d4b90d95a0b03e6e51f58da5213586f1480946; libtls-desc.so 336357920dd412db0613d88d223229751f31f3e34cd2b85f051162f75e614834; libtls-ld.so 093fa560431152d21a0323fe9f96bc49edfec7a9983e69cc19e95461f54bec54; application HAP 368bdfcde54a35867c5acc3cc5f5a0c2c6a5f865b585176831eba71a16842ada; test HAP 054b94fe0c3c3b58bc45682f944dc9d77211b856636519beb08e7e55f89a62eb; packaged libprobe.so d12de38d254ea8b0be5254df2e8570f721ebc0c172c3a63b1269fe7fa48fca68; application metadata 5562527cccb04f130a9b04efb3083e3f9211f3c8517fcdccfc7e32a3b472e76c; test metadata 76f9e1d064062a6e7d9a59adb4cbda96fb3ab4c710d9b6b016e9659e9941013a
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0009-dynamic-tls-loader.log SHA-256 51f86fa723383288f96eaf604e3d3691f95464aee8fbfd4257ff8dd112f725cc; docs/evidence/raw/EV-R1-EMU24-20260717-0009-emulator-console.log SHA-256 6ee2d8cd6e97a07465ae87a9a9a68656dad6ca9f4dba7746bbd9343e2b20d3b5; docs/evidence/raw/EV-R1-EMU24-20260717-0009-hilog-app-full.log.gz.base64 SHA-256 e44b94bda8e4465ab6135d0c8db3cb81f9940d99b36c84cf4b31aacb15bcba5d with decoded unfiltered 338-line 113049-byte HiLog SHA-256 010db21b82de5e0bcc8e4722c2d94f96c2ed0638068b813c8a977b2b7d1522b1
verdict: pass
verdict_scope: Tier1 passes for these verified pure-C dynamic TLS inputs in the exact API 24 x86_64 TestRunner late-load and thread-lifecycle boundary; this does not show that Go emits or supports an accepted model and does not generalize to arm64, named physical devices, commercial Huawei HarmonyOS, another loader/toolchain, NetBird, VPN, product support or stage exit
reviewer: anthropic/claude-opus-4-8 independent evidence review
reviewed_at: 2026-07-17
review_record: reviewed-pass; no BLOCKER or MAJOR findings; primary artifact hashes, final ELF evidence, HAP member identity, HiLog evidence, and three-role thread gates verified
```

`record_status: reviewed-pass` 表示独立证据审查已完成且无 `BLOCKER` 或 `MAJOR` 发现；审查核验 primary artifact hashes、最终 ELF 实证、HAP member 同一性、HiLog 证据和三角色线程门。`verdict: pass` 只表示第二个 T0 共识规定的 Tier1 纯 C loader 门通过，不是 R0、R1、R2 退出或 Go 路线实现通过。

| 输入 | 最终 ELF 实证 | `dlopen` | 主线程 | 加载前等待线程 | 加载后新线程 | 门判定 |
| --- | --- | --- | --- | --- | --- | --- |
| IE | `PT_TLS`、`R_X86_64_TPOFF64`、`STATIC_TLS`；无 `DTPMOD64`/`DTPOFF64`/`TLSDESC` | 按预期在 `tls_probe_value` initial-exec 动态定义处拒绝 | 未调用 | 已在 `dlopen` 前等待，拒绝后受控退出 | 未创建 | 对照通过，无环境漂移 |
| classic GD | `R_X86_64_DTPMOD64`、`R_X86_64_DTPOFF64`、`__tls_get_addr@plt`；无 `TPOFF`/`STATIC_TLS` | PASS | 初值42，值2001，100轮，reset42 | `dlopen` 前创建、加载后解析，初值42，值2002，100轮，reset42 | `dlopen` 后创建并解析，初值42，值2003，100轮，reset42 | PASS |
| TLSDESC gnu2 | 最终 `R_X86_64_TLSDESC` 和 descriptor 间接调用；无 `TPOFF`/`STATIC_TLS` | PASS | 初值42，值3001，100轮，reset42 | `dlopen` 前创建、加载后解析，初值42，值3002，100轮，reset42 | `dlopen` 后创建并解析，初值42，值3003，100轮，reset42 | PASS |
| local-dynamic | `R_X86_64_DTPMOD64`、`__tls_get_addr@plt`；无 `TPOFF`/`STATIC_TLS` | PASS | 初值42，值4001，100轮，reset42 | `dlopen` 前创建、加载后解析，初值42，值4002，100轮，reset42 | `dlopen` 后创建并解析，初值42，值4003，100轮，reset42 | 非门控 PASS |

四份最终 ELF 都由同一最小 C 源生成并导出 `GetTLS`、`SetTLS`、`ResetTLS`；host GCC 只负责编译 freestanding x86_64 PIC 对象以明确选择 classic `gnu` 或 `gnu2` 方言，固定 API 24 OHOS clang/lld 负责最终 shared link。所有 link 都使用标准 `--no-relax`，最终 `readelf` 动态重定位和 `llvm-objdump` 指令序列确认没有塌缩：IE 是直接 `%fs` 偏移，classic GD 调 `__tls_get_addr@plt`，TLSDESC 经 descriptor 间接调用，LD 使用模块描述符与 `__tls_get_addr`。

Node-API baseline 在任何 TLS `dlopen` 前得到 `ping=pong` 和固定版本。IE 在与 0007 相同的短生命周期 TestRunner、同一 `RTLD_NOW|RTLD_LOCAL` late-load 边界返回 `Error relocating .../libtls-ie.so: tls_probe_value: initial-exec TLS resolves to dynamic definition in .../libtls-ie.so`，故没有环境漂移并允许继续；classic GD 与 TLSDESC 均全过，因此 Tier1 判定为 `PASS`，附加 LD 也通过但不改变门结果。每个动态模型共完成 3 个角色各 100 轮，即每模型 300 轮、三模型合计 900 轮 set/read，不存在错误、跨线程串值或 crash。IE 的 `errno=2` 是 incidental；唯一失败判据是 `loaderError`/`dlerror` 中 `initial-exec TLS resolves to dynamic definition` 的原始 loader 文本。

成功的三个 `dlopen` 结构化结果中 `loaderError` 为空而 `loaderErrno=22`；errno 在成功 `dlopen` 后未定义，因此该环境值不参与任何失败或门判定。IE 同时具有非空 loader 原文和有意义的 errno 2。完整 app buffer 记录 PID 2950 在 `finishTest` 后由生命周期管理器正常回收；`crash_server.log` 没有 bundle、四个 TLS 库、fatal signal 或本探针 crash 条目。

四个 SO 在应用 HAP 与测试 HAP 中均逐字节等于输入，应用 HAP 的 `libprobe.so` 也等于最终 stripped 输出；当前两 HAP 均不再包含 0006/0007 遗留的 `libgoprobe.so`、`libneededprobe.so` 或旧 `libtlsprobe.so`。raw 中 executable-input manifest hash 及 application/test metadata hashes 均为 secondary self-reported 声明：其生成配方未纳入仓库，独立审查无法重算，故不作为 verdict 或 acceptance 输入；原始日志保留原样，不改写。完整 HiLog 为避免超长系统行造成工具截断，以确定性 gzip 后 Base64 无损保存；执行 `base64 -d docs/evidence/raw/EV-R1-EMU24-20260717-0009-hilog-app-full.log.gz.base64 | gzip -dc` 可恢复 decoded SHA-256 完全一致的 113049 字节原文。

本轮没有构建或修改 Go、NetBird、WireGuard、SDK 或 Go toolchain。`module.json5` 的历史 `INTERNET` 声明本轮未被调用，HiLog 和源码均无 network 路径；它不构成网络证据，且本轮不因删除该权限而重做 HAP。Go issue `#71953` 仍为 open proposal，CL `644975` 和 CL `696635` 仍为 `NEW` 且未 submitted，PR `75048` 仍 open 且 `merged=false`；四者均未合并、未发布，只是下一道独立授权 Tier2 Go/toolchain 可行性门的参考，不是 0009 输入、补丁、已合并基线或已发布能力。0009 不自动启动 Tier2，不回退或退出 R0、R1、R2，arm64 与具名真机继续 provisional，上游适配补丁计数保持 0。

清理后 bundle、guest staging、Runner、Emulator、qemu、Beta HDC server、10000/5555/8710 监听、临时对象目录和实验 tmux 均无残留；`entry/libs/x86_64` 只保留四个预期 ignored 输入供静态复核。

## 未满足项与下一步条件

- `ADJ-20260717-0001` 与 0008 的 API 24 phone Native Child 公开设备门保持有效；B0、B1 和 child Go 都是 `NOT RUN`，0009 的同进程纯 C 动态 TLS PASS 不会把受限 child 路线改写为可用。
- 第二个六席 T0 共识 `ADJ-20260717-0002` 已由 0009 执行，Tier1 结果为 `PASS`：IE 对照按预期拒绝，classic GD 与 TLSDESC 两个门控模型均通过，local-dynamic 附加通过，因此不触发“只 STOP API 24 x86_64 元组”的条件。
- 0007 对 initial-exec 和精确 Go 1.25.12 c-shared 输入的负面仍有效；0009 只把 loader 能力细分为“拒绝 IE、接受被验明的 C classic GD/TLSDESC/LD”，不能推导现有 Go 制品会使用动态模型或能加载。
- Go `#71953`、CL `644975`、CL `696635`、PR `75048` 均未合并、未发布，只能作为下一道独立授权 Tier2 Go/toolchain 可行性门的参考；在该门固定输入、timebox、退出标准、维护阈值和补丁预算前，不实现或尝试 Go/runtime/toolchain patch。
- arm64、华为商用 HarmonyOS 具名真机、PC/2in1、Tablet、公开 exec/PIE、其他 loader 与工具链保持 provisional 或未验证；0009 不从 x86_64 C loader 正面外推。
- 0004 的普通 `EntryAbility` 在可见路径下仍返回 `10106102`，TestRunner 成功只隔离测试框架进程路径；不得用 `aa test` 覆盖 UIAbility 阻塞或宣称 R1 退出。
- 本次 unsigned 接受只说明该研究 Emulator 的当前行为，不能替代开发、测试、发布签名或渠道最终制品闭环；签名材料仍只能由责任人通过仓库外受控路径提供，不得把私钥、口令、profile 或证书秘密写入仓库或普通构建日志。
- R0、R1、R2 均保持未退出，R1 仍缺 R0 具名 arm64 真机 HDC 闭环；NetBird、WireGuard、VPN、产品支持和发布主张仍无新增证据，R2 前及全项目累计上游适配补丁数保持 0。

## API 24 x86_64 Tier2 PS4 isolated compatibility experiment

```yaml
evidence_id: EV-R1-EMU24-20260717-0010
information_status: current-measured
record_status: reviewed-pass
stage: R1
related_stages: [R0, R2]
approval: recorded T0 Tier2 approval and operator approval in the 2026-07-17 execution request
timebox: 16h/2d, at most two immutable inputs, no third iteration
target_tuple:
  distribution: OpenHarmony-6.1.1.125 API 24 Emulator
  device: netbird_api24_phone phone Emulator; not a named physical device
  full_system_version: OpenHarmony-6.1.1.125; software emulator 6.1.0.125(SP9DEVC00E120R4P11)
  architecture: x86_64 guest on x86_64 host
  sdk_api_syscap: Command Line Tools 26.0.0.461; OpenHarmony SDK/API 24
  channel: N/A; unsigned research HAPs only
code_sha: e7cd00b4fd9d8db6ca3d61bf3cb081bee56ca88d; the replay consumed ignored prebuilt HAPs, bound by the artifact hashes below
upstream_sha: PS4 5f5911fabb3af7b5662ebc17ff7fa4f881df903a, parent ed3ec75df47ab8e7d6e4a30c445a8ef771382584; official Go 1.25.12 d80d9a98f7e3a8f9b3a82d2c6079f84eb1101d46 for the separate mechanical carry check
toolchain: host Debian GNU/Linux 13 repository Pod; Command Line Tools 26.0.0.461 and OpenHarmony SDK/API 24; Emulator 26.0.0.200; fixed Beta HDC /home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/toolchains/hdc reports Ver: 3.2.0e with -v; guest Toybox Linux 5.10.210; Go, Hvigor, Node.js, ohpm, NetBird, SDK build tools, patches, and probe source were not run or edited during this replay because it consumed prebuilt HAP inputs
working_directory: /home/worker/work/base/netbird-harmonyos
command: exact non-secret commands, X11 direct Emulator start, HDC target 127.0.0.1:10000, one hilog -r, ten consecutive aa test invocations, full app-buffer capture, fault/crash enumeration, and cleanup are in the retained transcript
input: same immutable candidate HAP pair as the first 0010 execution: APP 6084014 bytes SHA-256 493d791b4e4325e9202e224194108bc9def9f03b7b6b283b5f3e5c6f0324cd20; TEST 3345961 bytes SHA-256 dec6b635e8664069ba03268f563044c0f20916d9a899c98de10343c6c20021a9; each packaged libs/x86_64/libgoprobe.so member SHA-256 aa1ff164830dd9b203aa379151e43074a522cb3523f802b73c2da0702bfb80d2
expected: after HAP/member identity verification and install, one clear followed by ten consecutive TestRunner processes must each report host RC 0, TestFinished-ResultCode 0, GO_SPIKE_RESULT PASS, and PASS pre/post thread records; PIDs must be distinct and absent before teardown; unfiltered app buffer plus fault/crash list must be retained before teardown
actual: identity, target readiness, install, and post-send host identity passed. Runs 1-10 returned host RC 0 and TestFinished-ResultCode 0. The full buffer has one C++ space log and one TestRunner pipeline structured log per PID, therefore 20 GO_SPIKE_RESULT texts represent 10 runs, not 20 runs; judgment uses 10 distinct-PID structured suite records. Retained app buffer has ten structured suite PASS records, ten pre-dlopen thread PASS records, and ten post-dlopen thread PASS records for distinct exited PIDs 2799, 2844, 2911, 2956, 2988, 3042, 3074, 3108, 3156, and 3203. Each structured functional record reports Hello=42, RuntimeProbe=4194304, NetDialProbe=0, and stage=complete. The final ps check found none of those PIDs. The fault/crash listing contains only pre-run historical appspawn/sceneboard entries dated 2026-07-16 14:39 through 2026-07-17 04:02; no entry is named for the probe or bundle. The decoded unfiltered app buffer has no SIGSEGV, SIGABRT, fatal-signal, or probe-crash match.
started_at: 2026-07-17T14:11:35+08:00
ended_at: 2026-07-17T14:12:44+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds; guest HiLog timestamps are retained as observations
artifact_sha256: APP-HAP 493d791b4e4325e9202e224194108bc9def9f03b7b6b283b5f3e5c6f0324cd20 (6084014 bytes); TEST-HAP dec6b635e8664069ba03268f563044c0f20916d9a899c98de10343c6c20021a9 (3345961 bytes); APP-and-TEST libgoprobe.so member aa1ff164830dd9b203aa379151e43074a522cb3523f802b73c2da0702bfb80d2; replay transcript 3ee4e12ba95e3d173a8a26e68a778c68d2d8932cbe29c9d44535fd0591ff1206 (57139 bytes); replay console 6d617414579f01f5a0d0697b875ec56904063f5d09e5704a98a849ae8d8cf500 (2192 bytes); gzip-Base64 app buffer 44f74631410732a5a79b90b46a85a1c80661fdfce65b857e962a1a0f07b152cd (20859 bytes), decoding to 23a910d5f57a564636289cadbbf8ffd3f9a4589b8a16b90394799df33470f910 (120909 bytes, 787 lines)
raw_log_reference: docs/evidence/raw/EV-R1-EMU24-20260717-0010-same-artifact-replay.log SHA-256 3ee4e12ba95e3d173a8a26e68a778c68d2d8932cbe29c9d44535fd0591ff1206, repository access; docs/evidence/raw/EV-R1-EMU24-20260717-0010-same-artifact-replay-emulator-console.log SHA-256 6d617414579f01f5a0d0697b875ec56904063f5d09e5704a98a849ae8d8cf500, repository access; docs/evidence/raw/EV-R1-EMU24-20260717-0010-same-artifact-replay-hilog-app-full.log.gz.base64 SHA-256 44f74631410732a5a79b90b46a85a1c80661fdfce65b857e962a1a0f07b152cd, complete unfiltered app buffer after the sole clear and all ten runs, repository access
verdict: pass
verdict_scope: this is a same-artifact API 24 x86_64 Emulator TestRunner replay only. It makes no R0/R1/R2 exit, device-support, Go-toolchain adoption, NetBird, VPN, signing, or product claim; Tier2 iteration 2 remains the separately predeclared high-maintenance stop.
patch_count: 0
stage_exit: R0=NO, R1=NO, R2=NO
reviewer: anthropic/claude-opus-4-8 independent evidence review
reviewed_at: 2026-07-17T14:19:00+08:00
review_record: Reviewed-pass: all 7 artifact hashes match (APP HAP, TEST HAP, packaged libgoprobe.so member, replay transcript, replay console, gzip-Base64 app buffer, and decoded app buffer); 10 distinct exited PIDs each have host RC 0, TestFinished-ResultCode 0, structured suite PASS, and functional pre/post PASS with Hello=42, RuntimeProbe=4194304, NetDialProbe=0; the fault window has no probe or bundle entry; the old missing buffer is excluded; verdict scope, patch_count=0, and R0/R1/R2 gates remain unchanged.
```

The old 0010 `hilog-app-full` and `emulator-console` files are retained first-run summaries, not evidence; their reported checksums and retained summary content are not replayable repository evidence and are not inputs to this record, its verdict, or review. `raw_log_reference` uses only the same-artifact replay files above; that replay is the sole retained recalculable runtime evidence: `base64 -d docs/evidence/raw/EV-R1-EMU24-20260717-0010-same-artifact-replay-hilog-app-full.log.gz.base64 | gzip -dc | sha256sum` returns `23a910d5f57a564636289cadbbf8ffd3f9a4589b8a16b90394799df33470f910`.

The historical iteration-1/iteration-2 boundary remains unchanged. The stock control had `R_X86_64_TPOFF64` plus `STATIC_TLS` and was blocked before `dlsym`; the direct PS4 candidate emitted `R_X86_64_TLSDESC` without `TPOFF` or `STATIC_TLS`. Only after the original iteration-1 gate was satisfied was the unmodified PS4 binary diff mechanically applied to official Go 1.25.12. That apply stopped with conflicts in `src/cmd/internal/obj/link.go`, `src/cmd/internal/objabi/reloctype.go`, `src/cmd/internal/objabi/reloctype_string.go`, `src/runtime/cgo/callbacks.go`, and `src/runtime/tls_s390x.s`, plus missing `src/runtime/cgo/gcc_unix.c`. No conflict was resolved, no Go source was semantically edited, and no official-Go build or target run followed. This remains the predeclared high-maintenance STOP, not a replay failure; patch count remains 0 and R0/R1/R2 remain unexited.

## 2026-07-17 当前方向说明

0010 正文与 raw 证据按历史事实原样保留，但不合格用于当前 API 24 x86_64 phone Emulator E0-E8 投入总门：PS4 尚未发布，不是 NetBird 最新正式 release 的声明输入；其 TestRunner 正面不能满足普通 `EntryAbility`、NetBird 官方 Go loader 或 VPN runtime，也不退出正式 R 阶段。当前基线跟随最新正式 NetBird release；2026-07-17 核对仍为 `v0.74.6`、commit `3a2f773d655d88d16ed953fc2a114a4e690a1b08`、`go 1.25.5`、`toolchain go1.25.12`，官方 release run `29415596187` 成功，Go 1.26.5 不是 NetBird 声明基线。

当前总门为 `CLOSED`；后续 `EV-E0-EMU24-20260717-0001` 已以 `record_status: reviewed-pass`、`verdict: pass` 关闭 E0，下一执行门为 E1；NetBird 官方 Go loader 的 initial-exec TLS 路径仍失败，E1-E7 与 VPN runtime 未验证。此后 x86_64 Emulator 的 PASS 与 FAIL 均不得外推 arm64 或真机；E8 未 `OPEN` 前禁止任何真机执行，这是真机投入顺序而非 arm64 或真机负面技术结论。arm64 ABI、硬件、渠道、能耗和长稳等 Emulator 客观不能执行项只登记为后续真机专属，不计入 Emulator 总门，也不得据此提前真机；C-only 证据可先行，但不能单独打开 E8 或退出 R0/R1/R2。
