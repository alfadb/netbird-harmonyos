# N0 原生客户端可行性门：决议、范围与兼容 Oracle

最后核验：2026-08-10

本文持久化 2026-08-09 由 T0+/T0 五席三轮一致签署、用户批准的 N0 决议，以及 N0 的协议/行为/许可矩阵与 compat oracle。N0 是当前立即且唯一无物理设备门；本文不改变 E0-E8 既有门状态，E8 保持 `CLOSED`。

## 当前状态（2026-08-10）

- **N0 overall `reviewed-pass/pass`（双轴验收完成）**：N0(a) 固定 NetBird v0.76.3/f65f7b34 协议/行为/许可 inventory 与 compat oracle 已定义（见下文矩阵）；N0(b) 由 [`EV-N0-EMU24-20260810-0002`](evidence/n0-native-core-emulator-reviewed-pass-2026-08-10.md) 实测 `record_status: reviewed-pass`、`verdict: pass`（终审 `REV-N0-EMU24-20260810-0002`：`openai/gpt-5.6-sol` + `xai/grok-4.5` 双路只读独立复算 0 blocker/0 major，分别 2/6 minor，均为非阻塞记录改进建议、不扩范围修）。
- **能力接受范围**：接受使用固定正式发布（NetBird v0.76.3/f65f7b34、BoringTun 0.7.1）与真实 official Emulator runtime 数据的单一 native WG core C ABI 加载/冒烟，**不是合成 fixture**。
- **不证明**：N0 pass 不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product；这些能力仍为未验证。
- **下一步不是继续实现**：N0 pass 后不自动进入实现；仅未来用户显式授权 + fresh confirmation 后，既有 `E3-PHYS-PREFLIGHT` 仍是第一物理动作；N0 与物理 E3 **都 pass 后**才提交新 ADJ/T0 治理定义 native N1-Nx 门；治理生效前 E8 保持 `CLOSED`，不得以 native 静默替代 Go E1，不得开启产品实现。
- **E8 仍 `CLOSED`**：`EV-N0-EMU24-20260810-0002` 的 `e8_status=CLOSED`、`PHYSICAL_DEVICE_USED=false`；本状态不改变 E0-E8 既有门状态。
- **host-preflight**：`EV-N0-N0HOST-20260809-0001` 的 pending 状态由正式 0002 sealed evidence 覆盖其作为 N0(b) 证据的角色；其独立 review 字段保持 `pending`，不伪造。

## 决议元数据

- **日期与时区**：2026-08-09，`Asia/Shanghai (+08:00)`。
- **签署方**：T0+/T0 五席——`anthropic/claude-fable-5`（T0+）、`openai/gpt-5.6-sol`、`kimi-coding/k3`、`minimax/MiniMax-M3`、`xai/grok-4.5`（T0 四席）。
- **轮次**：Round 3 最终签署，10 条决议逐条 `SIGNED`，最终 `ROUND3 FINAL BALLOT: UNANIMOUS-SIGN`。
- **用户批准**：用户接受「固定正式版本线协议与可观察行为兼容」解释（第 1 条），并授权 N0 与 Emulator HDC；**不授权物理设备**（第 4 条）。
- **主会话不投票**：主会话只负责编排与记录，不参与 T0 表决。

## 准确路线措辞

平台不强制 Go。委员会建议将「跟随正式 NetBird 行为」解释为**与固定正式版本的线协议和可观察行为兼容**，不要求运行官方 Go 代码；最终由用户批准（已批准）。

不批准全面原生重写。主动研究方向 B 的准确名称：

> **NetBird 行为兼容原生客户端的分阶段可行性验证**

方向 B 构成：**ArkTS 系统壳 + 正式 C++ NAPI 薄桥 + 待证据选定的 C++/Rust native core**。

## A-E 处置

| 方向 | 处置 | 说明 |
| --- | --- | --- |
| A | 零工程投入触发式观察 | 不投入工程，仅在触发条件满足时观察 |
| B | **主动研究方向（批准）** | ArkTS 系统壳 + 正式 C++ NAPI 薄桥 + 待证据选定的 C++/Rust native core，名称为「NetBird 行为兼容原生客户端的分阶段可行性验证」 |
| C | 否决 | 未获批准 |
| D | 否决 | 因工具链/复现/许可约束否决 |
| E | 未证候选 | 只在 N0 内做公开能力/政策取证（见第 6 条），不投入实现 |

## N0 范围与非范围

### 范围（第 3 条）

N0 是当前立即且唯一无物理设备门，不改变产品架构：

- **(a) 固定 v0.76.3 协议/行为/许可清单与 compat oracle**：见下文「协议/行为/许可矩阵与 compat oracle」。
- **(b) 单一 native WireGuard core**：在许可清晰的候选中**只选一个** native WireGuard core，完成：
  - OHOS x86_64 Emulator 加载证据；
  - arm64 交叉构建最小 compile-only 证据（不声称加载）。
- 按证据 schema 双轴登记（见「双轴验收」）。

### 非范围

- **不实现** management/ICE/relay/UI（第 3 条明确禁止）。
- **不实现第二协议面**（第 9 条停止条件之一）。
- **不改变 E8**：E8 保持 `CLOSED`，不得以 native 静默替代 Go E1 或开启产品实现（第 5 条）。
- **不授权物理设备 HDC**（第 4 条）。
- **不维护 fork**（第 8 条）。
- **不给工期**：N0 及后续真实切片前不给日历/人月估算（第 10 条）。

## 双轴验收

按[证据与脱敏 Schema](evidence-schema.md)登记，双轴判定：

| 结果 | 含义 |
| --- | --- |
| `reviewed-pass` / `pass` | 继续 N0 后续步骤 |
| `reviewed-pass` / `blocked` | **停止方向 B 并返回 T0** |

`reviewed-pass` 只说明记录经独立审查合格，不自动等于功能通过；`verdict: pass` 与 `reviewed-pass` 必须同时成立才可继续。

**实际结果（2026-08-10）**：N0(b) 由 `EV-N0-EMU24-20260810-0002` 实测 `reviewed-pass/pass`（终审 `REV-N0-EMU24-20260810-0002`，0 blocker/0 major），N0 overall 双轴验收通过。按 N0 决议，`reviewed-pass/pass` 满足继续条件；但「继续 N0 后续步骤」不是继续实现——下一步仅限未来用户显式授权后的 `E3-PHYS-PREFLIGHT` 第一物理动作，N0+E3 都 pass 后才提交新 ADJ/T0 治理（见下文「E8 CLOSED 与后续 ADJ 条件」）。

## 停止条件（第 9 条）

出现以下任一情况，**立即停止并返回 T0**：

1. native core 需要私有/高维护 patch；
2. 正式 NDK 无法构建/加载；
3. 真实 VPN fd/protect 不可满足；
4. 固定版本 compat oracle 无法定义；
5. 范围扩展到第二协议面。

## E8 CLOSED 与后续 ADJ 条件（第 5 条）

- N0 与物理 E3 **都 pass 后**，先提交新 ADJ/T0 治理，定义 native N1-Nx 门并处理 E8 的 Go 专属前提。
- 治理生效前 E8 保持 `CLOSED`；不得以 native 静默替代 Go E1，不得开启产品实现。
- 本决议不改变 E0-E8 既有门状态；E1 overall Go 仍无 `reviewed-pass/pass`（v0.76.3 官方 Go 1.25.12 loader 由 `EV-E1-EMU24-20260809-0003` 实测 `reviewed-pass/blocked`）。
- **当前状态（2026-08-10）**：N0 已 `reviewed-pass/pass`（`EV-N0-EMU24-20260810-0002`）；物理 E3 尚未 pass（`plan_status: blocked-awaiting-device-authorization`）。因此新 ADJ/T0 治理**尚未触发**；下一步不是继续实现，仅未来用户显式授权 + fresh confirmation 后执行既有 `E3-PHYS-PREFLIGHT` 第一物理动作。

## 物理 E3 第一动作纪律（第 4、7 条）

- 当前**不授权任何物理设备 HDC**；Emulator HDC 仅限既有 E-campaign 与 N0(b) 证据 runner。
- 用户未来显式授权 + fresh confirmation 后，**既有 `E3-PHYS-PREFLIGHT` 仍是第一物理动作**（N0 pass 不改变该纪律）。
- 历史 fd=32/33 **不是 pass**。
- 用户授权后，在独立证据 ID 下、优先于任何 Go1.26 research，测 NetBird 正式工具链 Go1.25.12 arm64 c-shared 最小探针；结果可触发路线重议，但不是 E1 pass 的自动替代；Go1.26 只作可选研究（第 7 条）。
- Go 观察触发只限正式 Go release 相关修复或 NetBird 正式采用新工具链，然后重跑对应 E1；不维护 fork（第 8 条）。
- E 方向只在 N0 内做公开能力/政策取证；只有发现普通 phone 应用公开支持的进程启动 + fd 传递机制，才允许开独立最小 exec 探针；禁止 shell/隐藏 API/特权绕过；无公开机制则关闭 E（第 6 条）。

## 协议/行为/许可矩阵与 compat oracle

### 固定基线

- NetBird 正式 release `v0.76.3`，commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`；`go.mod` 声明 `go 1.25.5` 与 `toolchain go1.25.12`。
- 关键 replace（`go.mod`，固定 commit）：
  - `golang.zx2c4.com/wireguard` → `github.com/netbirdio/wireguard-go v0.0.0-20260628102922-2834bebf6c1a`（NetBird WireGuard fork）
  - `github.com/pion/ice/v4` → `github.com/netbirdio/ice/v4 v4.0.0-20250908184934-6202be846b51`（NetBird ICE fork）
- 历史基线 v0.74.6/v0.74.7 继续作为既有 E/R evidence 的历史实际输入，不静默替换。

### 矩阵

| 面 | 权威源码路径（固定 commit `f65f7b34`） | 协议/行为要点 | 许可边界 | N0 是否实现 |
| --- | --- | --- | --- | --- |
| management | `shared/management/proto/management.proto`（`ManagementService`：`Login`/`Sync` 等，`EncryptedMessage` 加密体）、`shared/management/proto/proxy_service.proto` | protobuf + gRPC；WireGuard 公钥加密消息体 | `management/` 目录 AGPLv3（根 LICENSE 例外） | **否** |
| signal | `shared/signal/proto/signalexchange.proto`（`SignalExchange`：`Send`/`ConnectStream`） | protobuf + gRPC；`EncryptedMessage` 用 WireGuard 私钥与远端 peer 公钥加密 | `signal/` 目录 AGPLv3（根 LICENSE 例外） | **否** |
| relay | `relay/protocol/protocol.go`（自定义 `Protocol` 类型）、`relay/server/`（`handshake.go`、`relay.go`、`listener/quic/`） | 自定义 relay 协议，QUIC 监听器 + 自定义握手 | `relay/` 目录 AGPLv3（根 LICENSE 例外） | **否** |
| ICE | `github.com/netbirdio/ice/v4@6202be846b51`（pion/ice v4 fork，`go.mod` replace） | ICE 候选交换、连接状态机 | MIT（Pion community 版权） | **否** |
| conn state | `client/internal/peer/conn.go`、`client/internal/peer/conn_status.go`、`client/internal/peer/conntype/` | peer 连接状态与类型优先级 | 根 BSD-3-Clause | **否** |
| WireGuard | `github.com/netbirdio/wireguard-go@2834bebf6c1a`（`go.mod` replace） | WireGuard 协议数据面（fork 含 fd wrapper 等平台适配） | MIT | **否**（N0 只选独立 native core 的 C ABI，不实现 Go fork） |
| routes | `client/internal/routemanager/`（`client/`、`common/`、`dynamic/`、`dnsinterceptor/`） | 路由管理、动态路由、DNS 拦截 | 根 BSD-3-Clause | **否** |
| DNS | `client/internal/dns.go`、`client/internal/dns/`、`client/internal/dnsfwd/` | DNS 配置与转发 | 根 BSD-3-Clause | **否** |
| ACL | `client/internal/acl/`（`manager.go`、`id/`） | 客户端 ACL 策略执行（策略由 management 服务端下发） | 根 BSD-3-Clause | **否** |
| state | `client/status/`、`client/internal/engine.go`、`client/internal/conn_mgr.go` | 引擎/连接状态与状态报告 | 根 BSD-3-Clause | **否** |

### 许可边界说明

- NetBird `v0.76.3` 根 LICENSE 明确：除 `management/`、`signal/`、`relay/`、`combined/` 目录外使用 **BSD-3-Clause**，列出的服务端目录使用 **AGPLv3**；REUSE 映射见 `LICENSES/REUSE.toml`。
- `netbirdio/wireguard-go@2834bebf` 与 `netbirdio/ice/v4@6202be846b51` 均为 **MIT**。
- **法律效果待专业评估，本矩阵不下硬门结论**：AGPLv3 目录的边界、客户端与服务端代码的交互是否触发 AGPL 义务、以及未来自托管服务端场景的许可影响，均须由专业法律评估确认后才可作为发布门输入。
- N0 阶段只做固定清单与 oracle 定义，不复制任何 AGPL 目录代码。

### N0 是否实现

矩阵中**全部为「否」**。N0 唯一实现的代码面是**单一 native WireGuard core 的 C ABI**（当前候选 BoringTun 0.7.1 `ffi-bindings` 的 14 个 C ABI 符号，见[预检记录](evidence/n0-native-core-host-preflight-2026-08-09.md)）。management/signal/relay/ICE/conn state/routes/DNS/ACL/state 均不在 N0 实现范围。

### compat oracle 定义

- **固定 tag 源码/IDL**：上表固定 commit `f65f7b34` 的 `.proto`、relay protocol、ICE fork、wireguard-go fork 源码与 IDL，作为协议行为的权威参考。
- **以后真实自托管 v0.76.3 服务端 + 官方客户端可观察行为**：以真实自托管 v0.76.3 management/signal/relay 服务端与官方客户端互操作的可观察行为作为行为 oracle（须在授权范围内执行，当前不授权物理设备）。
- **当前 N0 只用固定 C ABI smoke oracle**：N0 阶段仅以固定 C ABI 的 smoke 调用（构建、加载、符号调用）作为 oracle，不实现、不验证上层协议面。

## 与既有文档的关系

- 本决议不改变 [R0 章程](r0-charter.md)、[路线图](roadmap.md) 中 E0-E8 既有门状态与历史 evidence。
- 协议/许可矩阵与 [Tailscale-OHOS 数据通路审计](tailscale-ohos-netbird-port-audit.md) 的 v0.76.3 固定基线一致；本文只固定 N0 范围，不重写审计结论。
- 预检事实见 [N0 native core host-preflight 记录](evidence/n0-native-core-host-preflight-2026-08-09.md)。
