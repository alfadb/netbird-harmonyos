# OpenHarmony/HarmonyOS 平台与发行策略

最后核验：2026-07-16

本文给出 `netbird-harmonyos` 面向 OpenHarmony 与 HarmonyOS 的双目标策略。核心结论是：以 OpenHarmony 公共 API 为可移植基线，共享协议与网络核心，同时为两个平台维护独立应用壳、构建签名、制品、测试和分发流程。
该结论是方案建议，不表示双目标应用已经实现。

## 结论与边界

### 官方资料确认

OpenHarmony 与 HarmonyOS 在 HAP 模型、ArkTS API 和 SysCap 机制上存在共同基础。公共 API 名称相同并不等于设备系统能力、签名和分发规则相同。

HarmonyOS 包模型资料：
- [HarmonyOS HAP 包说明](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/hap-package)
- [HarmonyOS 应用包结构 FAQ](https://developer.huawei.com/consumer/cn/doc/harmonyos-faqs/faqs-package-structure-5)

### 方案建议

- 以 OpenHarmony 公共 API 和明确的 SysCap 要求定义可移植基线。
- 共享 NetBird 协议核心、WireGuard 适配、通用状态模型和可复用业务逻辑。
- 分别维护 OpenHarmony 应用壳与 HarmonyOS 应用壳。
- 两个应用壳分别绑定各自 SDK、runtimeOS 声明、权限配置、签名证书、构建制品、设备测试和分发渠道。
- 平台专有 API 必须留在对应应用壳或平台适配层，不能进入无条件共享路径。

### 不作出的承诺

当前不能承诺：

- 同一个已签名 HAP 可以同时安装到 OpenHarmony 与 HarmonyOS。
- 一个 HAP 可以覆盖所有 OpenHarmony 商业发行版和设备类型。
- 通过一侧 SDK 编译即代表另一侧运行时兼容。
- 模拟器通过即代表任意真机具备所需 VPN SysCap、驱动和后台运行能力。

## 建议架构：共享核心与两个应用壳

建议的职责划分如下：

```text
netbird-harmonyos/
  core/
    netbird-go/
    wireguard/
    shared-model/
  native/
    common/
    openharmony/
    harmonyos/
  apps/
    openharmony/
    harmonyos/
  tests/
    core/
    openharmony/
    harmonyos/
  build/
    openharmony/
    harmonyos/
  docs/
```

目录只是目标布局，当前仓库尚未创建这些工程模块。

共享核心适合包含 NetBird 管理面与信令协议逻辑、可跨平台编译且不依赖 Android/Linux 专有服务的 Go 代码、WireGuard 用户态数据面、通用网络状态模型和单元测试。

平台应用壳分别负责：
- ArkTS 页面、Ability、ExtensionAbility 和 VPN 授权生命周期。
- SDK/runtimeOS、SysCap、NAPI/native fd 桥接和平台配置。
- 应用标识、签名、HAP/App Pack 构建及对应设备和渠道验证。

## 双目标构建矩阵

两个目标应分别定义，不共用隐式默认值：

| 项目 | OpenHarmony 目标 | HarmonyOS 目标 |
| --- | --- | --- |
| SDK | OpenHarmony SDK | Huawei HarmonyOS SDK |
| runtimeOS | 按目标产品声明 | 按 HarmonyOS SDK/设备声明 |
| API/SysCap | 公共 API 基线并核对产品组件 | 公共 API 基线并核对 Huawei 设备能力 |
| 应用壳 | OpenHarmony 专用 | HarmonyOS 专用 |
| 签名 | OpenHarmony/发行版信任体系 | Huawei 开发与发布签名体系 |
| 制品 | 独立 HAP/App Pack 产物 | 独立 HAP/App Pack 产物 |
| 测试 | 目标发行版模拟器或真机 | HarmonyOS Emulator 与目标真机 |
| 分发 | 设备厂商、发行版或企业渠道 | Huawei 对应市场或企业渠道 |

即使两个目标暂时使用相同源码和包结构，也应由独立构建任务产生制品，避免签名、配置和依赖串用。

## 第三方 VPN 公共 API

### 官方资料确认

HarmonyOS 与 OpenHarmony 均公开了面向第三方 VPN Extension 的 API：

- [HarmonyOS `vpnExtension`](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-net-vpnextension)
- [HarmonyOS `VpnExtensionAbility`](https://developer.huawei.com/consumer/cn/doc/harmonyos-references/js-apis-vpnextensionability)
- [OpenHarmony `vpnExtension`](https://gitee.com/openharmony/docs/raw/master/zh-cn/application-dev/reference/apis-network-kit/js-apis-net-vpnExtension.md)
- [OpenHarmony `VpnExtensionAbility`](https://gitee.com/openharmony/docs/raw/master/zh-cn/application-dev/reference/apis-network-kit/js-apis-VpnExtensionAbility.md)

公开能力从 API version 11 起提供，职责包括：

- 由 `VpnExtensionAbility` 承载第三方 VPN 的系统生命周期回调。
- 通过 `vpnExtension` 创建和管理 `VpnConnection`。
- 根据 VPN 配置建立虚拟网络接口并取得其文件描述符。
- 配置隧道地址、路由、DNS 等系统网络参数。
- 使用 `protect` 让 NetBird 的外层控制连接、信令连接和 peer UDP socket 绕过虚拟隧道，避免流量回环。

普通第三方 VPN 应用走上述公开 Extension API，不要求应用使用系统权限 `MANAGE_VPN`。
`MANAGE_VPN` 对应系统 VPN 管理能力；系统 VPN、always-on 等接口属于 system API 边界，不应作为普通市场应用的实现前提。

### 方案建议

应用壳应把 VPN 生命周期与 NetBird 核心生命周期显式连接：

1. 用户发起连接并完成系统要求的 VPN 授权。
2. 应用壳创建 `VpnConnection`，提交地址、路由、DNS 和 MTU 配置。
3. 系统返回虚拟接口 fd。
4. NAPI/native 桥接层把 fd 交给 Go/WireGuard 用户态数据面。
5. 在外层 socket 建连前调用 `protect`，再启动 peer 和管理面通信。
6. 停止或异常时按顺序关闭数据面、fd 和 VPN Connection，并向 UI 报告状态。

fd 的所有权、复制、关闭顺序和跨线程调用必须形成明确契约。
这些细节需要以真实 SDK 头文件和运行结果为准，当前尚未实现。

## NetBird、Go 与 native 桥接

### 方案建议

NetBird 核心可优先评估 Go 交叉编译为目标架构 native 库，再通过 NAPI 桥接 ArkTS 应用壳。
WireGuard 数据面应消费系统 VPN API 返回的虚拟接口 fd，而不是假定存在 Linux TUN 设备路径或 Android `VpnService`。

桥接边界至少需要覆盖：
- 初始化、登录、连接、断开、状态订阅以及路由/DNS 变更。
- 虚拟接口 fd 的传入、所有权、关闭和需要 `protect` 的外层 socket fd 回调。
- native 错误映射，以及 ExtensionAbility 终止、进程重启和网络切换后的清理与恢复。

### 尚未验证

- NetBird Go 依赖能否针对目标 OpenHarmony/HarmonyOS ABI 完成交叉编译和链接。
- WireGuard 用户态实现是否依赖目标平台缺少的 syscall、netlink、TUN 路径或 libc 行为。
- NAPI 是否能稳定传递 VPN 虚拟接口 fd，并满足线程和生命周期约束。
- `protect` 对 Go runtime 创建的 TCP/UDP socket 应在哪一层、哪个时机调用。
- 后台运行、休眠、网络切换和进程回收时的隧道保持能力。

## HAP、App Pack 与签名

### 官方资料确认

根据 Huawei 官方资料所描述的 Huawei/HarmonyOS 应用市场流程，HAP 是安装和运行单元，App Pack 是上架单元；华为市场分发时会拆分 App Pack 并重签其中的 HAP。
OpenHarmony 发行版的市场、签名及是否重签不作统一推断，必须按具体发行版确认。

OpenHarmony 提供 HAP 签名与验签相关组件：
- [OpenHarmony `hapsigner`](https://gitee.com/openharmony/developtools_hapsigner/raw/master/README.md)
- [OpenHarmony `appverify`](https://github.com/openharmony/security_appverify/blob/master/README.md)

OpenHarmony 商业发行版可以替换系统信任根和应用验证策略，因此不存在覆盖所有 OpenHarmony 产品的通用商店或通用发布签名保证。
具体产品是否接受社区签名、厂商签名或企业签名，必须向目标发行版确认。

### 方案建议

- OpenHarmony 与 HarmonyOS 分别构建、分别签名，不复用发布证书和签名配置。
- CI 中分别输出未签名中间产物、测试签名 HAP 和候选发布制品，并标记目标平台、架构、SDK 与版本。
- App Pack 只进入对应市场流程；设备侧测试以该平台实际接受的 HAP 为准。
- 签名私钥由外部密钥管理注入，不进入源码仓库、构建缓存或普通日志。
- 对市场重签后的制品执行一次安装、升级、VPN 授权和联网回归测试。

## SysCap 与设备覆盖

API 出现在 SDK 中只代表可以编译，设备能否运行还取决于 SysCap、系统组件、产品裁剪和权限策略。
尤其是 OpenHarmony 设备，厂商可按产品形态裁剪 Network Kit 或 VPN 相关组件。

### 尚未验证

- 目标 OpenHarmony 产品是否包含 VPN Extension 所需组件和 SysCap。
- 目标产品是否允许普通第三方应用声明并启动 VPN Extension。
- 模拟器是否完整实现虚拟网卡 fd、路由、DNS、IPv6 和 `protect` 行为。
- HarmonyOS 目标设备和计划支持的 OpenHarmony 设备是否具有一致的 MTU、IPv6 与后台限制。
- 不同发行版的签名信任根、安装入口、升级规则和应用市场要求。

因此，支持范围应按“发行版 + 设备型号 + 系统版本 + 架构”记录，不能只写“支持 OpenHarmony”。

## 分阶段验证

### 阶段 1：API 与最小工程
- 分别用两套 SDK 编译最小应用壳，完成 VPN Extension 授权、虚拟接口、地址、路由、DNS 和 fd 读写验证。
- 验证外层 socket 经 `protect` 后不会进入隧道。

### 阶段 2：native 与 Go
- 用最小 C/C++ NAPI 模块验证 ArkTS/native/fd 生命周期，并为目标架构交叉编译最小 Go 库。
- 接入 WireGuard 用户态数据面做本地回环和单 peer 测试，记录必要的依赖补丁。

### 阶段 3：NetBird 集成
- 接入管理面登录、信令和 peer 配置，验证 DNS、路由、IPv6、重连和网络切换。
- 分别在 HarmonyOS 和选定 OpenHarmony 产品上验证前后台及进程恢复状态。

### 阶段 4：签名与发行
- 建立两个平台独立的构建签名任务，验证测试签名 HAP 的安装、升级与卸载。
- 验证 HarmonyOS App Pack 上架和市场重签；向 OpenHarmony 发行版确认信任根、渠道、签名和更新规则。
- 发布支持矩阵，只列出已经完成设备测试的组合。

进入下一阶段前，应把上一阶段的命令、SDK 版本、设备信息和结果写入可重复执行的测试记录。
