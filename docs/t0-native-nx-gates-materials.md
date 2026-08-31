# T0 材料包：native N1-Nx 门定义与 E8 Go 前提处置（草案 v1，供 T0 审议）

最后核验：2026-08-30 ｜ 状态：`t0-ballot-approved`（三席表决完成、用户批准；正式决议见 [native N1-Nx 治理决议](native-nx-governance.md)，本文降级为历史审议材料）

本文为 N0 决议预注册的后续治理（"N0 与物理 E3 都 pass 后，先提交新 ADJ/T0 治理，定义 native N1-Nx 门并处理 E8 的 Go 专属前提"）准备审议材料。触发条件已全部满足，且证据强于预期。

## 1. 实测证据基线（五表汇总）

| # | 测量 | 环境 | 结果 | 对本 T0 的意义 |
| --- | --- | --- | --- | --- |
| 1 | N0(b) BoringTun 0.7.1 `ffi-bindings` C ABI 加载+冒烟 | API 24 x86_64 官方 Emulator | `reviewed-pass/pass`（`EV-N0-EMU24-20260810-0002`） | native WG 数据面核心可加载、可调用；arm64 compile-only |
| 2 | E1 stock Go 1.25.12 loader | API 24 x86_64 官方 Emulator | `reviewed-pass/blocked`（`EV-E1-EMU24-20260809-0003`，`res_search: initial-exec TLS resolves to dynamic definition`，errno=2） | Go 路（Emulator 元组）堵 |
| 3 | E3-PHYS-PREFLIGHT VPN fd 可达性 S1-S7 | 物理冻结元组（PLA-AL10 / 7.0.0.102(SP8C00E102R7P3) / API 26 / arm64-v8a） | `reviewed-pass/pass`（`EV-E3-PHYS1API26-20260829-0001`，consumed-pass） | 物理 VPN Extension/fd/授权/清理链路完整可用 |
| 4 | G0 stock Go 1.25.12 arm64 c-shared loader | 同上物理元组 | `reviewed-pass/blocked`（`EV-G0PHYS1API26-20260830-0001`，`initial-exec TLS resolves to dynamic definition`（空符号字段），errno=0） | Go 路（物理 arm64 元组）堵，与 #2 同错误族 |
| 5 | Go 1.27.0 ELF TLS 对照（research-only） | host | `RS-G0-GO127ELF-20260830-0001`：IE TLS 形态逐版未变（1.25.12→1.26.0→1.27.0） | "升级 Go"逃生口关闭；上游 [#71953](https://github.com/golang/go/issues/71953) 仍为未合入 proposal |

补充事实：Tailscale-OHOS 外部项目（[审计](tailscale-ohos-netbird-port-audit.md)）证明 Go 可经**私有 OpenHarmony Go fork**（`GOOS=openharmony`）+ 打补丁的 tsnet 运行——fork 无补丁文件/哈希/许可证（整树无 LICENSE），不可复现、不可引用、不符合本项目"不维护 fork"纪律；它不反驳 #2/#4，反而佐证 stock Go 不可行。

## 2. 决议事项一：数据面/协议面路线（最优先表决）

### 2.1 路线选项

**路线 A「全 native 单核」**：ArkTS 系统壳 + C++ NAPI 薄桥 + 单一 native core（Rust 或 C++），所有协议面（management/signal 的 protobuf+gRPC、relay QUIC、ICE 子集）与数据面（WireGuard=BoringTun）在 native 层实现。

- 优点：无未验证平台依赖；TLS/loader 问题不存在（native 库无 Go runtime TLS 形态）；体积与线程模型可控；与 N0 已证核心衔接。
- 缺点：工程量最大——gRPC/QUIC/ICE 三栈都要 native 实现并按行为 oracle 校准；行为兼容风险集中在协议重实现层。

**路线 B「进程外 Go 可执行」**：NetBird Go 代码编译为**静态可执行进程**（非 c-shared dlopen——可执行文件的 TLS 在进程启动时静态分配，机制上不存在 #2/#4 测到的 dlopen IE-TLS 拒绝；此为 ELF/TLS 机制事实，设备可运行性仍属未测），ArkTS/NAPI 壳与该进程通过 IPC + fd 传递协作，复用 NetBird 全部既有行为。

- 优点：行为兼容风险最小（直接用 v0.76.3 Go 实现）；工程量集中在壳与 IPC 桥。
- 缺点/前置：**N0 决议第 6 条卡点**——"只有发现普通 phone 应用公开支持的进程启动 + fd 传递机制，才允许开独立最小 exec 探针；禁止 shell/隐藏 API/特权绕过；无公开机制则关闭 E"。当前无已知的 HarmonyOS 普通第三方应用公开进程启动机制（待专项取证）；VPN Extension 进程内 spawn、fd 传递、生命周期均为未测。**若公开机制取证仍为无，路线 B 按纪律自动关闭。**
- 附带风险：Go 工具链对 OHOS 目标仍可能需要适配（Tailscale-OHOS fork 的存在暗示 `GOOS=openharmony` 缺失；静态 `GOOS=linux` 二进制能否在设备上运行属未测假设，且即便能跑，"不维护 fork/不偏离 stock 构建"纪律下的问题须 T0 预先定界）。

**取证已完成（2026-08-30，两席独立交叉验证，结论一致：不存在）**：

- 华为官方 FAQ 明文：**"当前禁止三方应用在手机设备上 Fork 进程"**（[harmonyos-faqs/faqs-ability-29](https://developer.huawei.com/consumer/cn/doc/harmonyos-faqs/faqs-ability-29)，2026-06-26 更新）。
- 官方子进程 API（`childProcessManager.startChildProcess/startArkChildProcess/startNativeChildProcess`、`native_child_process.h` C API）设备差异条款**逐条限定 Tablet/PC/2in1**；phone 返回 `801`/`16000061`/`NCP_ERR_NOT_SUPPORTED`。fd 传递面（`ChildProcessArgs.fds`、`NativeChildProcess_FdList`≤16）仅随这些 phone 排除的 API 存在；SCM_RIGHTS 无公开应用 API。`aa start` 官方定位为 hdc shell 调试工具。
- SELinux `normal_hap` 域 `neverallow` 应用可写沙箱文件执行；唯一 exec 近路（HNP）被开发者模式门控且 VPN 扩展隔离沙箱（`vpn_isolate_hap`，无子进程豁免条款）不含 HNP 挂载。
- 附带核对：`EV-R1-EMU24-20260717-0008` 曾登记公开 Native Child API 面身份（仅证明 SDK 面存在）；本次取证补充其设备门控事实（startChildProcess=API 11 / ArkTS 带 fd 版=API 12 / native C API=API 12-13，phone 全部排除）。二者不矛盾：公开 API 面存在 ≠ phone 设备支持。
- **处置：按 N0 决议第 6 条，E 方向（进程外 exec）就此关闭；路线 B 关闭；不开启 exec 探针。T0 只需在 A 线上表决。**

### 2.2 路线 A 下的 N1-Nx 门骨架（草案，逐门判据在每门开门前细化）

| 门 | 内容 | 环境 | 核心 oracle/判据方向 |
| --- | --- | --- | --- |
| N1 | native WG 数据面×TUN 集成（**已由 T0 修正案 A2 拆分为 N1a/N1b**：N1a Emulator 回环数据泵；N1b 物理 VpnExtension fd——Emulator 三形态实测缺 VPN 授权组件，见矩阵证据） | N1a Emulator / N1b 物理 | fd 所有权表（继承 E3/Tailscale-OHOS 审计结论）、加密握手成功、双向包计数一致、资源无泄漏 |
| N2 | socket 保护与路由/DNS 壳基线（**已由 A2 拆分 N2a/N2b**；protect 公开 API 取证门为前置——opus M4） | N2a 壳侧 / N2b 物理 | protect 语义映射表、路由/DNS 由系统壳声明、VPN 生命周期撤销复验 |
| N3 | management 面一期：protobuf/gRPC native 客户端最小集（setup key 注册/Login/Sync） | Emulator（必要时物理） | **真实自托管 v0.76.3 management** 为行为 oracle；**前置：许可法律评估（BSD 声明映射效力 + combined/ 差异，见 §2.3）** |
| N4 | signal + relay 面二期：gRPC stream + QUIC relay 最小集 | Emulator + 自托管 | 真实 signal/relay 互操作 |
| N5 | ICE/conn state 三期：NetBird ICE fork 行为子集（候选交换、连接状态机） | Emulator | 对照固定 ICE fork 源码行为 + 可观察网络行为 |
| N6 | 端到端：自托管网络双 peer、direct+relay 双路径、routes/DNS/ACL 子集 | 物理（冻结元组） | R0 必选功能清单首轮覆盖 |
| N7+ | 性能/稳定/长稳（对齐 R3/R6 SLO 的 native 等效）、IPv6 探测 | 物理 | 沿用 R0 SLO 表 |

每门沿用：双轴 `reviewed-pass/pass`、预注册停止条件、Emulator/物理双向不外推、补丁预算沿用 R0 表（fork 禁令不变）。

### 2.3 协议面技术栈与许可（研究附录已并入）

**栈候选（2026-08-30 核实）**：

- **Rust 线（当前最优信号）**：tonic（MIT）+ prost（Apache-2.0）已于 2026-05 移入 CNCF `grpc/grpc-rust` 成为官方 gRPC Rust 实现；Rust 官方 Tier 2 支持 `aarch64-unknown-linux-ohos` target；quinn（Apache-2.0）覆盖 relay 的 QUIC 面；数据面核心 BoringTun 本身是 Rust——**全 Rust 单核**在语言与许可上是自洽组合。
- **C++ 线**：gRPC C++（Apache-2.0）+ protobuf/upb——musl aarch64 有 Alpine 官方包证据（grpc 1.83.0-r0，2026-07），无 OHOS 官方移植；交叉编译到 OHOS musl 属关键待验证项。
- 手写 HTTP/2 + protobuf 客户端可行但无先例背书，仅作 fallback 记录。
- protobuf codegen：prost（protoc→FDSet 工作流）或 upb；NetBird 官方 `generate.sh` 用 `protoc --go_out`，同一 `.proto` 可直接喂 prost/tonic。

**许可事实（f65f7b34 逐路径核实，修正先前担忧）**：

- 根 LICENSE 例外目录仅**顶层** `management/`、`signal/`、`relay/`、`combined/`（AGPLv3，服务端）；`shared/` 顶层不在例外内 → 按仓库声明映射为 **BSD-3**。
- `.proto` IDL 实际位于 `shared/management/proto/`、`shared/signal/proto/`（BSD 侧）；且**客户端参考实现全在 shared/**：management/signal client、relay client + 线协议（`shared/relay/{client,messages,auth/hmac}`）；AGPL `relay/` 目录仅服务端 + 一个 39 字节类型定义文件。relay 行为权威应以 `shared/relay/messages` 等 BSD 包为准。
- 已知差异一处：`combined/` 在根 LICENSE 例外文本内但未列入 `REUSE.toml` 映射——记录为待澄清事实。
- **结论定位**：协议面客户端所需的 IDL 与参考实现按仓库声明均处 BSD-3 侧；"从 IDL 生成客户端代码触发 AGPL 义务"的担忧基础已大幅收窄。**专业法律评估仍保留为 N3 硬前置**（对仓库声明映射本身的效力与 `combined/` 差异一并评估），但在事实层面不再是路线级阻塞。
- 行为兼容 oracle 沿用 N0 固定矩阵（v0.76.3/f65f7b34 权威源码路径表；研究已逐路径复核一致）。

## 3. 决议事项二：E8 Go 专属前提处置

现状：E8 OPEN 必要条件含"当前基线 E1 Go `reviewed-pass/pass`"，而 E1 已被 #2/#4/#5 三重实测证明在 stock 工具链下不可达（上游修复无期）。

| 选项 | 内容 | 取舍 |
| --- | --- | --- |
| 一（推荐） | E8 的 Go 前提替换为"native 路线达到端到端门（N6）`reviewed-pass/pass`"；E1 转为**休眠门**，触发器=Go 正式 release 合入 dynamic TLS（#71953）或 NetBird 官方采用新工具链，触发后按动态调整重开 | 标准对准可行路线、保留 E1 历史与重开路径；不降低其他 E8 必要条件（哈希一致、独立聚合审查等不变） |
| 二 | 双轨保留：E8 维持 Go 前提，等项目等 Go 上游 | 与实测证据矛盾，等于无限期停摆；列出仅为完整性 |
| 三 | E1 重定义为"所选数据面运行时在目标平台可用"（实现无关转译） | 语义改写历史门有争议；选项一以更干净的方式达到同等效果 |

E4-E7（protect/数据面/性能/长稳）本身实现无关，native 路线下直接沿用其义务并映射到 N2/N6/N7。

## 4. 不变项（任何表决不得触碰）

- N0 决议非范围项：不实现第二协议面、不给工期承诺、不维护 fork。
- R0 章程：补丁预算表、SLO、必选功能/排除项、证据 schema、双向不外推。
- E8 其余必要条件与独立聚合审查要求。
- 历史 evidence 与既有判定一律不改写。

## 5. T0 表决问题清单

1. 路线：A 全 native 单核（B 已因取证关闭：phone 无公开进程启动/fd 传递机制，华为官方 FAQ 明文禁止 fork，见 §2.1）。
2. N1-Nx 门骨架（§2.2）是否批准为基线，逐门细则按"开门前细化"处理。
3. E8 Go 前提处置：选项一/二/三（§3）。
4. 许可法律评估（对象：shared/ BSD-3 声明映射的效力、combined/ 的 REUSE.toml 差异、生成代码义务）的定位：N3 硬前置（推荐）/ 全路线前置 / 附加净室约束。
5. N2 中 bundle 排除 fallback 的预授权边界（若逐 socket protect 在 native 下不可达）。
6. E1 休眠触发器措辞（若选项一通过）。

## 附录：材料清单

- 证据：`EV-N0-EMU24-20260810-0002`、`EV-E1-EMU24-20260809-0003`、`EV-E3-PHYS1API26-20260829-0001`、`EV-G0PHYS1API26-20260830-0001`、`RS-G0-GO127ELF-20260830-0001`
- 决议：[N0 决议](n0-native-client-feasibility.md)（第 5-10 条）、[G0 计划](g0-go-arm64-physical-probe.md)
- 审计：[Tailscale-OHOS 审计](tailscale-ohos-netbird-port-audit.md)
- 研究附录：native 协议栈候选与许可事实（2026-08-30 研究报告，要点已并入 §2.3；待核实项：gRPC C++ OHOS musl 交叉编译、quiche/msquic musl 实测、各栈体积量级）
