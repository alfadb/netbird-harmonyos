# Tailscale-OHOS VPN 数据通路审计与 NetBird 映射

最后核验：2026-08-09

本文审计 `flypigJ/Tailscale-OHOS` 固定 commit
`fbd14c5207e746389e54ff9f8f8593c46942adac` 中的
HarmonyOS `VpnExtensionAbility -> NAPI -> Go -> wireguard-go` 路径，并映射到
当前R0正式基线（现v0.76.3）的固定 commit
`f65f7b347ee4e7de6d98c488d3d894cd018b02b6`。本文是源码研究，不是本仓库的
Emulator 或真机 evidence，不构成产品实现、许可证完成结论或阶段门通过。

## 结论

- 外部实现证明了一种值得独立重写验证的结构：VPN Extension 在独立进程恢复持久
  Go 后端，取得系统 TUN fd 后复制 fd，以自定义 `tun.Device` 注入用户态数据面，
  再用共享状态文件向 UI 提供心跳。对应源码见
  [Extension 恢复与建链](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L67-L164)、
  [fd 适配](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/harmony_tun.go#L41-L113)和
  [后端注入](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L182-L207)。
- 该 commit 不能复现其完整 Go 构建：`third_party/ohos-go` 和打过本地补丁的
  `third_party/tailscale` 都被忽略，仓内没有工具链 commit、补丁文件或输入哈希；
  `v1.86.5` 原版 `tsnet.Server` 也没有外部 `Tun` 字段。见
  [`.gitignore`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/.gitignore#L8-L9)、
  [`replace` 输入](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/go.mod#L77-L81)和
  [Tailscale v1.86.5 原始 `Server`](https://github.com/tailscale/tailscale/blob/db392aed39630023f969e1961fcbced785d09358/tsnet/tsnet.go#L66-L154)。
- 外层 socket 没有逐 fd 调用 HarmonyOS `protect`。实现禁用 Tailscale 的 Linux
  `netns`/socket-mark 路径，并在 VPN 配置中用
  `blockedApplications: [io.github.tailscaleohos]` 排除整个应用。见
  [`netns.SetEnabled(false)`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L160-L168)和
  [VPN 应用排除](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L123-L136)。
  这不是本项目 E5 要求的 management、signal、relay、peer TCP/UDP socket 逐一
  `protect` 证据。
- NetBird `v0.76.3` 已有可映射的 Android 移动端契约：`TunAdapter` 同时提供
  `ConfigureInterface` 和同步 `ProtectSocket`，Go 的 dialer/listener 在 socket
  `RawConn.Control` 阶段调用保护函数，Android 数据面再把平台 fd 包装成
  wireguard-go TUN。见
  [`TunAdapter`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/device/adapter.go#L3-L8)、
  [保护函数注册](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/android/client.go#L139-L142)和
  [fd 到 WireGuard](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/device/device_android.go#L57-L91)。
  `v0.74.7` 的同一契约见
  [v0.74.7 保护函数注册](https://github.com/netbirdio/netbird/blob/a1c9427d8004576e2cbb9e546d409847fa9df318/client/android/client.go#L100-L108)，
  作为历史基线保留。
- Tailscale-OHOS 固定树根部及全部 tracked tree 中没有 `LICENSE`、`COPYING` 或
  `NOTICE`。固定树可在
  [commit tree](https://github.com/flypigJ/Tailscale-OHOS/tree/fbd14c5207e746389e54ff9f8f8593c46942adac)
  复核；这是树级负面检查，无法用不存在文件的 blob URL 表示。依赖自身许可证
  不会自动给该仓 ArkTS/C++/Go glue 代码授予许可，因此当前不得复制其代码。
- 外部 README 的真机结果和 PowerShell 探针只属于外部维护者自报。本仓没有取得
  对应原始日志、固定目标元组、签名后 HAP、哈希或独立审查记录；没有运行这些
  真机脚本，也不改变 E3-E7 的 reviewed dependency-blocked aggregation exception、
  `E3-PHYS-PREFLIGHT` 尚无证据 ID 的 `blocked` 计划状态或 E8 `CLOSED`。

## 固定基线

| 对象 | 固定值 | 审计边界 |
| --- | --- | --- |
| Tailscale-OHOS | 正式非 prerelease `v0.3.19-release`，annotated tag `ae8f121e265eb519c122d7f2264db96275ffa8aa` 指向 commit `fbd14c5207e746389e54ff9f8f8593c46942adac` | 固定源码树仍没有仓内 release manifest、制品哈希或可复现构建输入 |
| Tailscale | tag `v1.86.5`，commit `db392aed39630023f969e1961fcbced785d09358` | 用于对照未打本地补丁的上游源码 |
| Tailscale wireguard-go | pseudo-version commit `1d0488a3d7da6b6ed79202519f30e7a286e0d4e6` | `go.mod` 固定依赖；外部适配器实现 `tun.Device` |
| NetBird | 正式 release `v0.76.3`，commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` | 本文的 NetBird 映射基线 |
| NetBird wireguard-go | pseudo-version commit `2834bebf6c1aea76bd217f31ea91c99f75e4a20a` | `v0.76.3` 的 replace 后实际 WireGuard 源码 |
| NetBird 历史基线 | 正式 release `v0.74.7`，commit `a1c9427d8004576e2cbb9e546d409847fa9df318` | 保留为既有审计历史事实，不静默改写 |
| 本仓原记录 | `v0.74.6`，commit `3a2f773d655d88d16ed953fc2a114a4e690a1b08` | 保留为既有 R0/E 门记录，不静默改写历史输入 |

Tailscale-OHOS 的 GitHub release 页面将 `v0.3.19-release` 标为正式非 prerelease；其
annotated tag `ae8f121e265eb519c122d7f2264db96275ffa8aa` 指向本文固定 commit
`fbd14c5207e746389e54ff9f8f8593c46942adac`。这修正了此前“不是正式 release”的
错误，但固定源码树仍没有仓内 release manifest、制品哈希或可复现构建输入，不能
据此复现本文审计的 Go/Tailscale patched 构建。

NetBird `v0.76.3` 的 GitHub release 页面为
[`v0.76.3`](https://github.com/netbirdio/netbird/releases/tag/v0.76.3)，发布时间为
`2026-08-08T12:11:41Z`，tag 直接指向上述 commit。源码仍声明
[`go 1.25.5` 和 `toolchain go1.25.12`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/go.mod#L1-L6)，
与 `v0.74.7` 相同，Go/toolchain 不变。

相对 `v0.74.7`，`v0.76.3` 前进 140 个 commit；固定
[compare](https://github.com/netbirdio/netbird/compare/v0.74.7...v0.76.3)
包含安全相关修复（management 拒绝 pending/blocked 用户访问 reverse proxy、
debug bundle 路径与上传目标收紧、daemon IPC 按本地身份授权、relay 只信任配置的
trusted proxy 的 `X-Real-Ip`、移除 deprecated Hello handshake/gob token decode、
删除用户时显式 accountID 校验等），以及 relay 早期消息缓冲上限提升、stale routing
peer 修复、nftables route 规则修复、eBPF XDP UDP checksum 修复和 WireGuard
watcher 重启修复。与本研究直接相关的依赖差异是 wireguard-go replace 从
[`8ec1ad32...`](https://github.com/netbirdio/netbird/blob/a1c9427d8004576e2cbb9e546d409847fa9df318/go.mod#L349-L351)
回退为
[`2834bebf...`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/go.mod#L331-L333)，
即 `v0.74.6` 时代使用的同一 commit（回退发生在 0.74.7-branch-sync 合并
`be677742`，main 侧版本胜出）；`2834bebf` 比 `v0.74.7` 的 `8ec1ad32` 少一个
Windows gVisor RACK 禁用 commit，`tun/tun_linux.go` 两版本逐字节一致。
Android mobile 入口有实质变化（见下文逐文件比较）。本文只记录差异；R0 已正式
采用该输入，受影响门仍须按动态调整机制用新记录重跑。

历史事实：`v0.74.7` 相对 `v0.74.6` 前进 7 个 commit，Android mobile、
`TunAdapter`、socket-protect、route 和 DNS 文件没有变化，wireguard-go replace
从 `2834bebf...` 更新为 `8ec1ad32...`；R0 已于 2026-07-18 正式采用该输入。

本次逐文件比较（`v0.74.7` a1c9427d → `v0.76.3` f65f7b34）覆盖 17 个对象：
`client/iface/device/adapter.go`、`client/android/client.go`、
`client/iface/device/device_android.go`、`client/internal/connect.go`、
`client/embed/embed.go`、`client/iface/iface_new_android.go`、
`client/iface/iface_create_android.go`、`client/net/protectsocket_android.go`、
`client/net/dialer_init_android.go`、`client/net/listener_init_android.go`、
`client/iface/bind/control.go`、`client/grpc/dialer_generic.go`、
`client/internal/relay/relay.go`、`client/internal/dns/host_android.go`、
`client/internal/routemanager/systemops/systemops_android.go`、`LICENSE`、
`LICENSES/REUSE.toml`。其中仅 3 个文件有变化：`client/android/client.go`
（+134/-19）、`client/internal/connect.go`（+37）、`client/embed/embed.go`
（+1/-1）；其余 14 个对象逐字节一致。另核验了 `client/internal/engine.go`
（+170/-58）与新增 `client/internal/engine_tunsettings.go`。

审计使用 `gh repo clone`、`gh api repos/.../releases/latest`、tag/ref/commit 查询及
本地只读 `git`/`rg`。临时 clone 位于 `/tmp`，没有进入本仓 evidence。

## 外部调用链

固定实现的实际顺序如下：

1. UI 在连接前从 UI 进程 Go 后端读取 VPN 配置，随后停止该进程内后端，再调用
   `startVpnExtensionAbility`。源码见
   [UI handoff](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/components/BridgeStatus.ets#L465-L496)。
2. 独立 VPN Extension `onCreate` 创建 `VpnConnection`，从相同 `filesDir` 恢复
   持久后端，轮询地址和控制面路由，再生成 HarmonyOS 地址、路由、DNS、MTU 和
   应用排除配置。源码见
   [Extension `onCreate`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L20-L37)和
   [配置构造](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L39-L136)。
3. Extension 先对本应用 VPN connection 调用 `destroy()` 清理陈旧实例，然后
   `create(config)` 取得原始 fd。它先调用一次只复制并关闭副本的 `tunFdProbe`，
   再把同一原始 fd 传给 Go 后端重启入口。源码见
   [create/probe/restart](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L138-L176)和
   [probe 实现](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/tun_probe.go#L5-L27)。
4. NAPI 把 ArkTS `number` 读取为 `int32_t`，调用 c-shared 导出的
   `TSBackendRestartWithTun`；Go 把 fd 转成 `int` 后创建 `harmonyTunDevice`。见
   [NAPI fd 入口](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/napi_init.cpp#L638-L682)和
   [Go C export](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/main.go#L161-L169)。
5. Go 复制 fd、设 nonblocking、包装为 `os.File` 并实现
   `github.com/tailscale/wireguard-go/tun.Device`。随后本地打补丁的
   `tsnet.Server.Tun` 把该对象传入 Tailscale userspace engine；实现自报
   netstack 在该模式下不消费 peer/subnet 流量。已 tracked 的赋值见
   [server Tun assignment](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L182-L207)，
   但 `tsnet` 补丁本身不在固定树中。
6. wireguard-go 通过 `tun.Device.Read/Write/Events/MTU/BatchSize/Close` 消费该
   外部设备；固定接口契约见
   [Tailscale wireguard-go `tun.Device`](https://github.com/tailscale/wireguard-go/blob/1d0488a3d7da6b6ed79202519f30e7a286e0d4e6/tun/tun.go#L20-L53)。

## Go 与 OpenHarmony 构建输入

外部仓声明
[`go 1.24.4`、`toolchain go1.24.5`、Tailscale `v1.86.5`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/go.mod#L1-L8)，
并把 Tailscale module 替换为本地 `../../third_party/tailscale`。构建脚本选择
`GOOS=openharmony`、`GOARCH=arm64`、`CGO_ENABLED=1`，使用 Harmony Native SDK
clang/sysroot，并生成 `-buildmode=c-shared` 的 `libtailscale_go.so`；见
[`build-go.ps1`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/build-go.ps1#L34-L58)。
CMake 再把该库作为 imported shared library 链入 NAPI so；见
[`CMakeLists.txt`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/CMakeLists.txt#L1-L15)。

固定 SHA 中可以确认的补丁需求有两类，但不能确认补丁内容：

- Go 工具链必须新增 `GOOS=openharmony` 和非标准 `runtime.IsOpenharmony`；调用点见
  [`main.go`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/main.go#L20-L29)。
- Tailscale `tsnet.Server` 必须新增外部 `tun.Device` 注入，并改变默认 netstack/TUN
  连接。原版 `v1.86.5` 只从 `tsd.System` 取得 TUN，再直接创建并启动 netstack；见
  [原版 engine/netstack 初始化](https://github.com/tailscale/tailscale/blob/db392aed39630023f969e1961fcbced785d09358/tsnet/tsnet.go#L573-L606)。
  `wgengine.Config` 本身已有可注入 `Tun tun.Device`，见
  [上游配置字段](https://github.com/tailscale/tailscale/blob/db392aed39630023f969e1961fcbced785d09358/wgengine/userspace.go#L160-L180)，
  但 `tsnet.Server` 没有对应公开字段。

固定树没有 bootstrap 脚本来取得精确 OpenHarmony Go source commit，也没有
`third_party/tailscale` patch、patch hash、source archive hash 或 SBOM。因此
README 所称的“小补丁”不能计数、复现或评估维护风险；见
[README 自述](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/README.md#L74-L85)。
这些缺失不允许作为本项目 Go/NetBird patch budget 的输入。

当前R0正式基线（现v0.76.3）使用 Go 1.25.12，并把 WireGuard 替换到
`netbirdio/wireguard-go@2834bebf...`。本仓 v0.74.6 历史证据表明其官方 Go 1.25.12
制品在 API 24 x86_64 应用 late-load 路径受 initial-exec TLS 阻断；v0.76.3 由
`EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`（[证据](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)），仍无 E1 pass。外部项目使用另一套 Go 1.24.5 arm64 OpenHarmony fork，不反驳该
历史结果，也不能替代 v0.76.3 重验或授权引入私有 Go fork。

## NAPI 导出、线程与内存

### 导出面

NAPI descriptor table 同时暴露同步和 Promise 异步函数，包括 hello、engine probe、
backend start/stop/logout/status/snapshot、auth URL、VPN config、peer/exit node/account、
网络设置、TUN fd probe 和 restart-with-TUN；固定列表见
[`napi_property_descriptor`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/napi_init.cpp#L734-L776)，
ArkTS 类型面见
[`index.d.ts`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/types/libtailscale_ohos/index.d.ts#L1-L31)。
模块通过 constructor 调用 `napi_module_register`；见
[注册入口](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/napi_init.cpp#L779-L795)。

### 线程模型

- Promise 路径使用 `napi_create_async_work`。Go 调用发生在 NAPI worker 的 execute
  callback，Promise resolve/reject 和 JS value 创建发生在 complete callback；见
  [async work](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/napi_init.cpp#L34-L85)。
- 同步导出直接在调用 ArkTS 的线程进入 Go，例如 `backendStart`、`backendStatus`、
  `backendRestartWithTun` 和 `tunFdProbe`。这些路径没有统一 timebox，不能假定不阻塞
  ArkTS 主线程。
- Go 后端启动另起 goroutine 执行 `server.Start()` 和 `LocalClient()`；见
  [`startAsync`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L1162-L1199)。
- 固定实现没有从 Go 任意线程主动调用 ArkTS；状态通过轮询 Promise 和状态文件回传，
  所以它没有证明 NetBird 所需的 Go/native -> ArkTS threadsafe event callback。
- 未看到 NAPI env cleanup hook、queued work cancellation、并发上限或 Extension 销毁时
  等待 outstanding work 的契约。这些是实现前必须补齐的未知项。

### 内存模型

Go 所有字符串结果由 `C.CString` 分配，并统一导出 `TSFreeString` 释放；见
[Go 分配/释放](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/main.go#L27-L31)和
[`TSFreeString`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/main.go#L179-L182)。
C++ 同步路径在创建 ArkTS string 后释放 C string；异步路径先复制到
`std::string` 再释放，并在 complete callback 删除 async work 与 heap work object；见
[异步复制/释放](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/cpp/napi_init.cpp#L34-L65)。

边界只传 UTF-8 字符串、boolean 和 `int32` fd，没有跨语言借用 byte buffer。优点是
所有权简单；代价是 VPN config 使用 `|`/`,` 拼接协议、状态 JSON 和字符串产生多次
复制，也没有 schema version、长度上限或敏感字段类型约束。NetBird 适配应使用明确
版本化结构，并避免把 setup key、节点私钥或完整拓扑变成普通 NAPI string。

## TUN fd 所有权

固定实现形成了以下实际所有权：

| 对象 | 创建/复制 | nonblock | 读写 | 关闭责任 |
| --- | --- | --- | --- | --- |
| HarmonyOS 原始 fd | `VpnConnection.create(config)` 返回并保存在 Extension | `dup` 共享 open-file description；在副本设置 `O_NONBLOCK` 也影响原始 fd | 外部代码不直接读写 | 没有显式 `close(fd)`；代码依赖 `VpnConnection.destroy()` 或进程回收 |
| probe 副本 | `tunFdProbe -> unix.Dup(original)` | 副本调用 `SetNonblock`，状态与原始 fd 共享 | probe 明确不读写 | probe 的 `device.Close()` 关闭副本，但不会恢复共享的 `O_NONBLOCK` |
| backend 副本 | `backendRestartWithTun -> unix.Dup(original)` | 副本调用 `SetNonblock`，状态与原始 fd 共享 | wireguard-go 通过自定义 `tun.Device` 读写 | `harmonyTunDevice.Close()` 的 `sync.Once` 关闭 `os.File` 副本 |
| 被替换的旧 backend 副本 | 新副本创建成功后关闭旧 `tsnet.Server` | 保持原状态 | 关闭前可能仍有 reader | `oldServer.Close()`；失败时新副本也关闭并返回失败 |

复制、nonblock 和 `os.File` 包装见
[`newHarmonyTunDevice`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/harmony_tun.go#L41-L60)。`dup` 隔离 close 责任，但不隔离 file status flags；这里的 `SetNonblock` 会更新共享 open-file description，probe 即使随后关闭副本也不会自动把原始 fd 恢复为 blocking。重启交换顺序见
[`restartWithTun`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L213-L242)。

自定义设备 `BatchSize()` 固定为 1；`Read` 只消费 `bufs[0]`，把 packet length 写入
`sizes[0]`；`Write` 逐 buffer 调用 `os.File.Write`；见
[Read/Write](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/harmony_tun.go#L64-L98)。
`Close` 发送 `EventDown`、关闭 event channel，再关闭 Go 拥有的 fd 副本；见
[Close](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/harmony_tun.go#L103-L113)。

需要独立修正或验证的边界：

- `Write` 在 `os.File.Write` 无错误时就执行 `written++`，没有显式拒绝
  `n < len(packet)` 的 short write；目标 fd 是否保证 packet 原子写必须由 SDK/运行
  证据确认。
- Extension `onDestroy` 清 timer 并调用 `connection.destroy()`，但没有调用
  `backendStop`；见
  [`onDestroy`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L272-L288)。
  若 Extension 进程不立即退出，Go 的 duplicate fd 和 goroutine 是否及时结束未知。
- 原始 fd 没有被 NAPI/Go 关闭，这是刻意复制后的合理方向，但必须由平台文档和
  `destroy()` 实测证明原始 fd 的最终所有者；不能只靠字段设为 `-1`。
- `dup` 没有显式添加 `CLOEXEC`。HarmonyOS 应用进程通常不执行子进程，但最终契约
  仍应明确使用 `F_DUPFD_CLOEXEC` 或等价能力是否可用，并决定共享 `O_NONBLOCK`
  是否为平台原始 fd 可接受的显式副作用。

NetBird Android 路径的契约不同：`TunAdapter.ConfigureInterface` 返回 fd 后，
`CreateUnmonitoredTUNFromFD` 直接对这个 fd 设 nonblock 并用 `os.NewFile` 接管，不先
`dup`；固定实现见
[NetBird Android device](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/device/device_android.go#L57-L80)和
[wireguard-go fd wrapper](https://github.com/netbirdio/wireguard-go/blob/2834bebf6c1aea76bd217f31ea91c99f75e4a20a/tun/tun_linux.go#L637-L657)。
`NativeTun.Close()` 最终关闭包装的 file；见
[wireguard-go close](https://github.com/netbirdio/wireguard-go/blob/2834bebf6c1aea76bd217f31ea91c99f75e4a20a/tun/tun_linux.go#L482-L498)。
因此 HarmonyOS 适配不能把平台仍拥有的原始 fd 直接交给该函数；应先固定
“platform original + Go duplicate”契约，或实现独立 `tun.Device`，并对每条失败路径
执行 EBADF、泄漏和 fd 复用测试。

## `tun.Device` 注入映射

Tailscale-OHOS 实现完整 `tailscale/wireguard-go/tun.Device` 并通过本地
`tsnet.Server.Tun` patch 注入。这一模式的可迁移点是“平台创建 TUN，Go 只消费
明确所有权的 fd”；具体 `tsnet` patch 与 NetBird 无关，不能复制。

NetBird `v0.76.3` 已经有两条入口：

- `client/android.NewClient` 接收 `TunAdapter`、外部接口发现和网络变化 listener，
  `Run`/`RunWithoutLogin` 最终进入 `ConnectClient.RunOnAndroid`；见
  [Android Client](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/android/client.go#L139-L155)和
  [`RunOnAndroid`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/connect.go#L107-L130)。
  `v0.76.3` 新增 `GetTunSettings`/`TunSettings`（routes 与 search domains 快照，
  供 TUN 重建时拉取最新设置，见
  [engine_tunsettings.go](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/engine_tunsettings.go#L1-L20)），
  且 `RunOnAndroid` 把网络变化 listener 包进 `tunnelnotifier` 再注入
  `MobileDependency`。
- `client/embed` 默认启用 userspace netstack，并默认禁用 server routes；client
  routes 仅在调用方设置 `DisableClientRoutes` 时禁用。见
  [embed options](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/embed/embed.go#L168-L206)。
  它适合无系统 TUN 的嵌入用途，不满足本项目必须覆盖 NetBird routes、DNS 和真实
  系统 VPN 流量的首个 0.x 范围。

Android 非 netstack 路径由 `NewWGIFace` 选择 `device.NewTunDevice`，再由
`CreateOnAndroid` 调 `TunAdapter.ConfigureInterface`；见
[`NewWGIFace`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/iface_new_android.go#L11-L29)和
[`CreateOnAndroid`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/iface_create_android.go#L7-L19)。但其 fd wrapper 在 `os.NewFile` 后仍调用 Linux TUN `Name()` 和 flag 初始化；见
[`CreateUnmonitoredTUNFromFD`](https://github.com/netbirdio/wireguard-go/blob/2834bebf6c1aea76bd217f31ea91c99f75e4a20a/tun/tun_linux.go#L637-L657)。HarmonyOS `vpn-tun` fd 是否支持这些 ioctl 未知；Tailscale-OHOS 的自定义 device 则直接返回静态名称而不调用 ioctl，见
[`Name()`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/harmony_tun.go#L99-L113)。因此 Android `TunAdapter` 是控制流参考，不是已验证可直接复用的 HarmonyOS fd wrapper；若改为注入自定义 `tun.Device`，NetBird 当前接口还需要一个可计数平台补丁。

`RenewTun` 与 `RenewableTUN` 已提供进程内 fd 替换；新 device 加入后旧 device 被关闭，
见
[`RenewTun`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/device/device_android.go#L114-L128)和
[`RenewableTUN.AddDevice`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/device/renewable_tun.go#L262-L287)。

主要缺口是这些文件带 Android build tag 或按 `runtime.GOOS == "android"` 分派。
OpenHarmony Go fork的 `GOOS=openharmony` 不会天然进入该路径；若继续选择
Linux build tags，则会重新引入 Linux TUN/netlink/route 假设。本项目需要在 R2
形成最小、可计数的 OpenHarmony 平台适配，而不是伪装 Android 或 Linux GOOS。
另一个接口不匹配是 `TunAdapter.ConfigureInterface` 为同步返回 fd，而 HarmonyOS
VPN 创建当前由 ArkTS Promise 完成；是否采用 TSFN 到主 ArkTS 上下文并有界等待，
还是拆成“Go 产出配置 -> ArkTS create -> Go resume”两阶段状态机，仍属未知设计项。

## 外层 socket bypass/protect

### Tailscale-OHOS 的实际策略

固定树中不存在 `VpnConnection.protect` 调用。代码先禁用 Linux `netns`，然后依靠
VPN 配置排除整个自身 bundle。这样 management/control、DERP 和 peer UDP 等所有
本应用 socket 都按应用粒度绕过 TUN，不需要逐 socket 回调；同时它也无法给出“哪些
socket 在何时被保护”的审计清单。README 也明确写出禁用 Linux bypass；见
[README](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/README.md#L74-L82)。

该策略只能作为 fallback 设计候选，不能满足当前 E5/R3 判据：

- 粒度是 bundle，不是 socket；无法单独观察首次和重连的 management、signal、relay、
  STUN/TURN、ICE peer socket。
- `blockedApplications` 在目标 HarmonyOS/OpenHarmony 版本、普通第三方权限和实际
  route 组合下的语义仍须 SDK 与运行证据确认。
- 本项目已有策略要求真实 TCP/UDP 外层 socket 在正确时点 `protect`，并以流量证明
  没进入隧道；不能用 API 成功或另一产品 README 代替。

### NetBird 固定源码的保护点

NetBird Android 在 `NewClient` 时把平台 `ProtectSocket(fd int32) bool` 注册到
`client/net`。`ControlProtectSocket` 在标准库提供的 `syscall.RawConn.Control`
回调内、socket connect/bind 之前同步调用平台函数；未设置或返回 false 都返回错误，
见
[`protectsocket_android.go`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/net/protectsocket_android.go#L11-L47)。
Android dialer 和 listener 都安装该 control，见
[dialer init](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/net/dialer_init_android.go#L1-L5)和
[listener init](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/net/listener_init_android.go#L1-L6)。
WireGuard bind 也把 listener control 加入 wireguard-go `ControlFns`；见
[bind control](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/iface/bind/control.go#L1-L15)。

management gRPC 代表路径使用 `nbnet.NewDialer`，relay TURN TCP/UDP probe 使用
`nbnet.NewDialer/NewListener`；见
[gRPC dialer](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/grpc/dialer_generic.go#L17-L43)和
[TURN probe](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/relay/relay.go#L259-L290)。
这给 HarmonyOS 适配提供了集中 hook 点，但不能直接断言所有当前和未来 socket 已覆盖；
R3 仍须枚举实际 management、signal、relay、ICE、WireGuard 和 DNS 路径，并验证每次
重连。尤其是 HarmonyOS `protect` 的线程、同步/异步和 Extension object 生命周期
能否满足 `RawConn.Control` 的同步返回契约，目前未知。

## UI 与 VPN Extension 独立进程恢复

外部 manifest 声明一个普通 `EntryAbility` 和一个非 exported `type: vpn` Extension，
只请求 `INTERNET`；见
[`module.json5`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/module.json5#L11-L52)。
连接 handoff 的核心是：

- UI 平时运行无外部 TUN 的持久 Go 后端；连接前停止 UI 后端，避免两个进程同时持有
  state/engine，再启动 VPN Extension。
- Extension 在自己的进程从 `${filesDir}/tailscale` 恢复认证状态，取得配置后创建
  TUN，再重启为外部 TUN 后端。
- Extension 每秒写入状态与 `heartbeatMs`；UI 以 5 秒 freshness 判定 Extension
  是否存活，并在显示后 6 秒复查陈旧状态。见
  [Extension heartbeat](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L221-L270)、
  [5 秒 freshness 判定](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/components/BridgeStatus.ets#L571-L576)和
  [UI recovery](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/components/BridgeStatus.ets#L152-L180)。
- stop 完成后 UI 重新启动自身后端；见
  [UI stop handoff](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/components/BridgeStatus.ets#L511-L523)。
- 项目明确不做 reboot auto-start；README 只承诺用户重启后打开应用、恢复后再手动连接，
  见
  [README lifecycle 边界](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/README.md#L68-L72)。

可复用的是“单一活动后端 + 持久 state + generation/cancel + 心跳过期”的状态机思想。
仅可借鉴的是明文 status file polling：它没有原子 rename、schema/version、文件锁或
进程身份 token，且状态和心跳来自同一进程自报。NetBird Android Go 层已有 blocking
`Run`、reboot 场景 `RunWithoutLogin`、context cancel `Stop` 和 `RenewTun`，并新增
`GetTunSettings`；见
[生命周期 API](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/android/client.go#L155-L275)。
但 NetBird release 仓只提供 Go binding，不提供 HarmonyOS UI/Extension 恢复壳；
状态持久化并发、Extension 被 kill、授权撤销、网络切换和 crash 后清理都必须由本项目
独立设计和验证。

## 路由与 DNS

Tailscale-OHOS 从 backend status 取 IPv4/IPv6 地址，并在 `RouteAll` 为 true 时收集
peer `PrimaryRoutes`；只有已选 exit node 的 `/0` AllowedIPs 被加入默认路由。配置
字符串还传递 `CorpDNS`；见
[`vpnConfig`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L252-L323)。
Extension 始终加入 Tailscale IPv4 `100.64.0.0/10`，存在 IPv6 地址时加入
`fd7a:115c:a1e0::/48`，附加动态 subnet/default routes，并在 `CorpDNS=true` 时设置
`100.100.100.100`；见
[Extension route/DNS config](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/entry/src/main/ets/vpnextensionability/TailscaleVpnExtensionAbility.ets#L73-L136)。
网络偏好保存在 mode `0600` 文件并在新进程启动后恢复；见
[preference persistence](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L783-L840)和
[restore](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/native/go_bridge/backend.go#L904-L940)。

NetBird Android 的对应边界更适合直接映射：engine 把 route range、NetBird
DNS IP 和 search domains 传入 `CreateOnAndroid`，平台 `TunAdapter` 负责创建系统
VPN；见
[engine create](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/engine.go#L2129-L2134)。
`v0.76.3` 把 `v0.74.7` 的 `InitialRouteRange()` 改为 `CurrentRouteRange()`，配合新增
`GetTunSettings`/`TunSettings` 在 TUN 重建时拉取最新 route/search domains（见
[`engine_tunsettings.go`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/engine_tunsettings.go#L1-L20)）。
Android host DNS manager 明确不改 OS DNS，因为 VPN service 负责；见
[`host_android.go`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/dns/host_android.go#L8-L24)。
Android route system operations也是 no-op，说明 route 安装属于平台 VPN 配置而非
Go netlink；见
[`systemops_android.go`](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/client/internal/routemanager/systemops/systemops_android.go#L12-L31)。

仍未知的关键项包括：network map 后续 route/DNS 更新是否必须 destroy/recreate TUN、
route selection 与 fd renew 的原子顺序、split DNS/search domain 在目标 API 的表达、
IPv6 接受范围、MTU 变化、default route 与 LAN bypass 冲突，以及重建期间流量是
fail closed 还是旁路。外部实现禁止连接中修改 route/DNS，只能说明一种规避竞态的
UI 策略，不能代替 NetBird 动态 route/DNS 行为。

## 真机脚本与 evidence 边界

外部 README 自报在 HarmonyOS 6.1 phone 上完成登录、VPN、TUN 双向 packet、peer、
subnet route、exit node、reboot 后恢复、screen-off 和 Wi-Fi reassociation；原文见
[README verified list](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/README.md#L19-L55)。
固定树提供以下 PowerShell 探针：

- engine probe 只通过 layout marker、进程存活和固定版本文本判定 userspace engine
  初始化；见
  [`device-engine-probe.ps1`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-engine-probe.ps1#L62-L104)。
- backend probe 轮询 UI status，接受 `Running`，或同时满足 `NeedsLogin` 与
  `loginURLReady=true`；见
  [`device-backend-probe.ps1`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-backend-probe.ps1#L82-L123)。
- VPN data probe 启动系统浏览器访问固定 Tailscale service IP，只要求前后
  `tunRead/tunWrite` 都增加；见
  [`device-vpn-data-probe.ps1`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-vpn-data-probe.ps1#L41-L78)。
- exit-node probe 访问 literal public IP，并同样以双向 TUN counter 增长判定；见
  [`device-exit-node-probe.ps1`](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-exit-node-probe.ps1#L41-L71)。
- UI probe 检查 unlocked device、实际 Home/Settings 控件和连接态只读规则；见
  [Home/Settings 检查](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-user-ui-probe.ps1#L77-L130)和
  [连接态只读判定](https://github.com/flypigJ/Tailscale-OHOS/blob/fbd14c5207e746389e54ff9f8f8593c46942adac/scripts/device-user-ui-probe.ps1#L131-L173)。

这些脚本是可读测试意图，不是随 commit 归档的执行证据。固定树没有提交每次执行的
完整原始输出、HAP SHA-256、source manifest、目标设备型号/完整 build string、SDK/
工具链哈希、路由表、packet capture、protect/bypass 逐 socket 记录、清理后 fd/thread
快照或独立审查。TUN counter 增长也不能单独证明目标流量的对端、outer socket 绕行、
路由正确性或无其他并发流量。因此所有真机结果只能标记为“外部维护者自报”，不能
导入本仓 `docs/evidence/`，不能关闭 E1、E3、E4、E5、E6、E7 或 E8。本次审计没有
连接、探测或操作任何真机。

## 许可证映射

- Tailscale-OHOS 固定 tracked tree 没有根或子目录 LICENSE/COPYING/NOTICE，故其
  ArkTS、C++ NAPI、Go adapter、脚本和资源没有可从该仓确认的明确许可授予。README
  也没有许可证声明。必须在复制任何表达性代码前取得维护者许可或上游补充的固定
  license；仅可独立实现接口思想。
- 其固定依赖 Tailscale `v1.86.5` 是 BSD-3-Clause，见
  [Tailscale LICENSE](https://github.com/tailscale/tailscale/blob/db392aed39630023f969e1961fcbced785d09358/LICENSE#L1-L28)；
  `tailscale/wireguard-go@1d0488a3...` 是 MIT，见
  [wireguard-go LICENSE](https://github.com/tailscale/wireguard-go/blob/1d0488a3d7da6b6ed79202519f30e7a286e0d4e6/LICENSE#L1-L17)。
  这些许可证只覆盖各自上游内容，不补足 Tailscale-OHOS glue 的缺失许可，也不能
  识别未提交 `third_party/tailscale` patch 的作者和许可边界。
- NetBird `v0.76.3` 根 LICENSE 明确除 `management/`、`signal/`、`relay/`、
  `combined/` 外使用 BSD-3-Clause，列出的服务端目录使用 AGPLv3；见
  [NetBird LICENSE](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/LICENSE#L1-L16)和
  [REUSE mapping](https://github.com/netbirdio/netbird/blob/f65f7b347ee4e7de6d98c488d3d894cd018b02b6/LICENSES/REUSE.toml#L1-L6)。
- `netbirdio/wireguard-go@2834bebf...` 是 MIT，见
  [NetBird fork LICENSE](https://github.com/netbirdio/wireguard-go/blob/2834bebf6c1aea76bd217f31ea91c99f75e4a20a/LICENSE#L1-L17)。
  后续若修改其 fd wrapper、增加 OpenHarmony 文件或复制上游文件，仍须保留版权和
  许可证文本，并进入本项目 patch/SBOM/NOTICE 审查。

## NetBird 映射表

| 关注点 | Tailscale-OHOS 固定实现 | NetBird `v0.76.3` 固定入口 | 本项目判断 |
| --- | --- | --- | --- |
| Go 入口 | c-shared 自定义 backend，`tsnet.Server` | `client/android.NewClient` + `ConnectClient.RunOnAndroid`（新增 `GetTunSettings`） | Android API 是优先映射对象；需 OpenHarmony build/runtime adapter |
| TUN 创建 | ArkTS `VpnConnection.create` 后把 fd 注入 Go | `TunAdapter.ConfigureInterface` 由 Go 请求平台创建并同步返回 fd | 生命周期顺序相反；需两阶段状态机或安全同步 callback |
| `tun.Device` | 自定义 `harmonyTunDevice`，先 `dup` | wireguard-go `CreateUnmonitoredTUNFromFD` + `RenewableTUN` | 可复用“先复制、Go 接管副本”契约；不能直接传原始 fd |
| socket bypass | 禁用 netns，bundle 级 `blockedApplications` | `ProtectSocket` + dialer/listener `RawConn.Control` | NetBird hook 更符合 E5；Harmony 回调线程/同步语义待证 |
| route | Extension 构造 overlay、subnet、exit routes | engine 传 `CurrentRouteRange()` 给 `TunAdapter`；Android systemops no-op | 平台壳负责 route；动态更新/recreate 未闭合 |
| DNS | `CorpDNS` 时配置 `100.100.100.100` | engine 传 DNS/search domains；Android VPN service 负责 OS DNS | 复用职责边界，不复用 Tailscale 地址或配置协议 |
| 生命周期 | UI backend 与 Extension backend handoff、文件心跳 | blocking Run、RunWithoutLogin、Stop、RenewTun、GetTunSettings | 需 Harmony 状态机；外部文本心跳仅作参考 |
| 依赖 | Go 1.24.5 fork、Tailscale 1.86.5、本地未提交 patch | Go 1.25.12、NetBird v0.76.3、wireguard-go 2834bebf | 不能用外部 Go fork替代本仓官方 Go 门 |
| 许可证 | 根 LICENSE 缺失 | client BSD-3-Clause，服务端目录 AGPLv3，wireguard-go MIT | 不复制外部 glue；NetBird 固定依赖继续做 SBOM/NOTICE |

## 可复用设计、仅可借鉴模式与未知项

### 可复用设计

以下仅指可独立重写的架构设计，不表示可以复制无许可证的源码：

- 平台 VPN API 拥有原始 fd，Go 只拥有显式 duplicate；每个 fd 有单一 close 责任。
- 系统壳负责地址、路由、DNS、MTU 和 VPN 授权，wireguard-go 只消费 packet fd。
- UI 与 VPN Extension 只允许一个活动 Go backend，通过持久 state 和 generation/cancel
  完成进程 handoff。
- NAPI 对潜在阻塞操作使用 Promise worker；C string 在跨边界复制后立即释放。
- TUN adapter 显式实现 `tun.Device`，固定 `BatchSize`，并把 EventUp/EventDown 与
  close 顺序纳入契约。
- NetBird 采用集中 dialer/listener socket-control hook，由平台实现保护 callback。

### 仅可借鉴模式

- Tailscale 专用 `tsnet.Server.Tun` patch 和 netstack 改造。
- bundle 级 `blockedApplications` 绕行；它只能作为逐 socket protect 不可用时的候选，
  且仍须重新讨论 E5 判据，不能静默替换。
- `|`/`,` 文本 VPN config、每秒 truncate status file 和 UI 轮询心跳。
- 连接时禁止修改 route/DNS 的 UI 策略。
- 通过 TUN counter delta、layout marker 或 README 列表证明数据面和生命周期。
- PowerShell 脚本中的默认参数、单 USB 设备假设和本地签名 HAP 路径。

### 未知项

- OpenHarmony Go fork的准确 source commit、补丁、bootstrap、许可证和 Go 1.25.12
  可迁移性。
- 未提交 Tailscale `tsnet` patch 的完整 diff、维护成本和 netstack side effect。
- HarmonyOS 普通第三方 `protect` 在目标 API 的线程、时序、返回和 fd 有效期，以及
  如何满足 NetBird 同步 `RawConn.Control`。
- Extension `onDestroy` 后 Go duplicate fd、goroutine 和 backend 的实际回收时点。
- TUN nonblocking read/write 的 `EAGAIN`、partial write、背压、shutdown unblock 和
  重复 close 行为。
- NetBird Android-tagged代码拆成 OpenHarmony 平台层所需的实际补丁数；Linux build
  tags 带入的 netlink、TUN name/flag ioctl、`SO_MARK`、DNS 和 syscall 差异，以及
  应采用 fd wrapper 还是新增外部 `tun.Device` 注入点。
- `v0.76.3` 新增的 `GetTunSettings`/`TunSettings` 与 `tunnelnotifier` 包装是否引入
  新的平台接口面（TUN 重建时拉取 route/search domains 的同步语义、网络变化事件
  转发），以及 `CurrentRouteRange()` 相对 `InitialRouteRange()` 对重建原子性的影响。
- route/DNS 动态更新、split DNS、search domains、IPv6、MTU 和 TUN recreate 的原子性。
- UI/Extension 同时恢复、state 文件锁、crash 中断、授权撤销、其他 VPN 冲突和 reboot
  后恢复。
- Tailscale-OHOS 真机自报的目标元组、制品身份、完整原始证据和独立审查结论。
- Tailscale-OHOS glue 的许可授权与未提交 patch 的版权归属。

## 对当前路线的影响

本研究只为 R2/R3 设计提供候选输入：NetBird Android `TunAdapter`、socket-control hook、
Android route/DNS 职责边界和 `RenewableTUN` 是优先评估对象；Tailscale-OHOS 的
fd duplicate、自定义 `tun.Device` 和独立进程 handoff 可用于测试设计。任何采用都必须
形成可计数 patch、固定 Go/NetBird/wireguard-go 输入、NAPI ABI、fd 所有权表和目标
SDK/Emulator evidence。

本审计不改变门状态：v0.74.6 历史 loader 负面保持原绑定；当前R0正式基线（现v0.76.3）
由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`（[证据](evidence/e1-stock-go-v0763-replay-0003-measured-blocked-2026-08-09.md)），仍无 E1 pass。Emulator E3-E7 保持 reviewed dependency-blocked aggregation
exception，不是 `pass` 或 `N/A`；2in1、Tablet 的 blocked 只覆盖 registration-layer
前置边界。`E3-PHYS-PREFLIGHT` 当前计划状态为 `blocked` 且尚无证据 ID，故 E8 保持 `CLOSED`。
只有预检 `reviewed-pass/pass` 才满足 E8 的预检必要条件，还必须取得当前R0正式基线（现v0.76.3）的 E1 pass、
哈希一致和独立聚合批准。外部 HarmonyOS 6.1 phone 自报不属于本仓目标元组，不能授权
或替代该预检；E8 `OPEN` 后仍须在具名物理设备 R2/R3 完成 E4-E7 VPN/数据面义务。
