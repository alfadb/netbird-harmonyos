# N1b 门计划与判据预注册（native WG 数据面 × 物理 VpnExtension fd 集成）

最后核验：2026-08-31 ｜ 状态：`criteria-preregistered-r0-pending-independent-review`（独立审查确认前不得开始任何测量；测量后不得追认或修改判据--ADJ-T0-NATIVE-NX-20260830-0001 §二.7）

## 定义与归属

N1b 是 [native N1-Nx 治理决议](native-nx-governance.md) N1 拆分的物理子门：在**物理冻结元组**上，验证 **BoringTun 0.7.1 native WG 数据面核心与 VpnExtension 真实 fd 的集成**--fd 合同实测（治理绑定条款 §二.2 的全部义务）、握手、双向真实加密报文、资源稳定性。N1a（Emulator 回环泵）与本门互不外推。

- **证据身份**：campaign `N1B-PHYS1API26-<YYYYMMDD>-0001`；evidence `EV-N1B-PHYS1API26-<YYYYMMDD>-0001`；attempt initial、无 retry。具体日期在正式授权时冻结。
- **环境**：物理冻结元组（HarmonyOS / PLA-AL10 / `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` / API 26 / aarch64 / arm64-v8a）--**pre-E8 native 物理例外**（治理决议 §三），每次物理 campaign 独立 AUTH/pair、白名单 HDC、单次执行不重试不换 ID、双向不外推、禁止性能/长稳/渠道/产品外扩。
- **arm64 物理前置**（治理 §二.8）：物理运行前须有冻结元组 arm64 同核心（BoringTun 0.7.1）加载证据--本门自身即为该证据的首次产生（N0 仅有 compile-only）。
- **实现载体**：`spikes/n1b-phys-fd-hap/`（新独立目录），复用 N1a 的 Rust 泵核心设计（`spikes/n1a-native-dataplane/src/pump.rs` 双隧道回环泵逻辑）但以 **VpnExtension 真实 fd** 替代回环 socket；同 BoringTun 0.7.1 checksum、同 `--offline --locked`。

## fd 合同（治理绑定条款 §二.2 逐条落实--N1b 实测建立，不得继承）

以下**全部必须在本门的物理实测中逐条建立**（E3 探针契约只做 `fcntl(F_GETFD)` 只读快照、禁 dup，与 N1b 的 dup 副本契约不同，不可混引）：

| # | 合同项 | 实测要求（冻结判据 C2 的一部分） |
| --- | --- | --- |
| FD-1 | 原始 fd 唯一关闭责任 | 平台原始 fd 由 `VpnConnection.destroy()` 唯一关闭；native 在任何路径（成功/异常/清理）都不 close 原始 fd |
| FD-2 | native 只消费 dup 副本 | native 用 `dup()`（优先 `F_DUPFD_CLOEXEC`）创建副本，后续所有 BoringTun 读写只用副本；**禁止把原始 fd 交给 BoringTun/tun.Device** |
| FD-3 | dup 语义与 CLOEXEC | `dup()` 返回新 fd 号；`fcntl(F_DUPFD_CLOEXEC)` 后 `F_GETFD` 含 `FD_CLOEXEC` 位；登记两个 fd 号 |
| FD-4 | O_NONBLOCK 共享副作用 | native 设 `O_NONBLOCK` 于副本后，原始 fd 的 `F_GETFL` 也含 `O_NONBLOCK`（共享 open-file description）；实测记录并登记 |
| FD-5 | destroy 后 fd 有效期 | `VpnConnection.destroy()` 后：原始 fd 变 EBADF（`fcntl(F_GETFD)` 返回 -1/errno=EBADF）；dup 副本**仍然可读但 read 返回 EOF（0 字节）或 EAGAIN**（取决于 O_NONBLOCK）；两者行为差异逐字登记 |
| FD-6 | 重复 close 安全 | native close 副本后，再次操作副本得 EBADF；native 不得 close 原始 fd（FD-1 的逆向验证：close 原始 fd 的尝试被探针拒绝或跳过） |
| FD-7 | 泄漏核对 | 泵生命周期结束时：副本已 close、原始 fd 由 destroy 关闭、`/proc/self/fd` 计数回基线（进程级仅观察字段、不设门--r3 C7 同款）；`tunnel_free`×2 |

## 探针设计

- **架构**：ArkTS VpnExtensionAbility（`type: vpn`、`exported=false`）内，`VpnConnection.create()` 获取真实 tun fd -> native 层 `dup` 副本 -> 双 BoringTun 隧道互为 peer（A/B 密钥互逆）-> **A 隧道从 tun fd 副本读明文、加密后写入外部 UDP；B 隧道从外部 UDP 读密文、解密后写回 tun fd 副本** -> 双向真实加密报文。
- **外部信道**：物理设备的 loopback `127.0.0.1` UDP socketpair（模拟 WireGuard 外层传输；物理设备上 loopback 不经 VPN 隧道，不产生路由递归）。
- **UI 触发**：EntryAbility 页面按钮启动/停止 VPN Extension（沿 E3 预检模式），探针自动运行后单行 marker 报告结果。
- **NAPI 薄层**：复用 N1a 的 overlay 结构（结构化结果 + 单行 marker + 诊断快照 + detail chunk）。

## 判据（预注册 r0，全部 machine-verified）

| # | 判据 | pass 条件（fail-closed） |
| --- | --- | --- |
| C1 | 库加载 | 唯一 arm64 native 成员在 VpnExtension 进程内 dlopen 加载成功（**arm64 首次物理加载证据**--补 N0 compile-only）；加载失败 -> fail |
| C2 | fd 合同 | FD-1 至 FD-7 全部逐条实测通过（各子项登记原始 fd 号、副本 fd 号、fcntl 结果、destroy 前后状态快照）；任一子项不符 -> fail |
| C3 | 握手+数据完整性 | fd 合同通过后：双隧道握手建立（stats 双侧 `time_since_last_handshake >= 0`）+ 每方向 5 轮 × 50 包 × 1024 字节合成 IPv4 明文经 **tun fd 副本 -> BoringTun A 加密 -> UDP loopback -> BoringTun B 解密 -> tun fd 副本** 全链逐字节相等（10 轮×50×1024 = 500 KiB，保守物理首次） |
| C4 | 吞吐下限 | C3 泵窗口有效明文字节 ÷ 纯泵时间 ≥ **1 MiB/s**（物理 arm64 保守下限；Emulator N1a 实测 55-105 MiB/s，物理预留 50-100× 裕度） |
| C5 | 背压 | 独立阶段（C3 后）：缩小 socket 缓冲诱发饱和，10s 时间盒内不死锁/不损坏（三态 induced/not-triggered/fail 沿 r3 语义） |
| C6 | keepalive | `keep_alive=1`，≥3 个静默间隔各 ≥1s，每侧 ≥1 次 tick 且 ≥3 次返回 WRITE_TO_NETWORK 的 keepalive 经信道送达对端；间隔后会话存活 |
| C7 | 资源稳定 | r3 C7 同款：探针自有资源门（副本 fd 逐个 fcntl 确认关闭 + 静态无 pthread + tunnel_free×2）；进程级仅观察（标 process_model=vpnextension） |
| C8 | 清理 | `tunnel_free`×2 + 副本 close + `VpnConnection.destroy()` 唯一关闭原始 fd + Extension 进程收尾（沿 E3 S7 terminal 模式） |
| C9 | 结果 marker | 恰好一行 `N1B_RESULT\|verdict=<PASS\|FAIL>\|fd=<open\|closed>\|throughput_mibps=<x.xx>`（四字段冻结）+ 可见结果页截图非黑帧（沿 E3 预检模式） |

## 聚合规则

overall `pass` 当且仅当 C1-C4、C6-C9 均为 pass，且 C5 ∈ {pass-induced, not-triggered}。任一 Ci 为 fail -> overall `fail`。`blocked` 仅环境类（设备连接失败、OTA/元组漂移、HDC 退化、构建输入漂移）。**C2 fd 合同任何子项 fail -> overall fail（非 blocked）**。无法按字面求值 -> blocked + 返回治理。

## 停止条件

N0 决议第 9 条停止条件（5 项）全文沿用（含第 3 条划界：本门实测 VPN fd 可达性--若 fd 合同在物理元组上不可满足，触发 N0 第 3 条停止条件并返回 T0）；追加本门特化（沿 N1a r3 模式）。

## 非范围（双向不外推）

本门不主张：x86_64/Emulator（N1a 已覆盖）、socket protect（N2）、management/signal/relay/ICE（N3-N5）、产品实现、性能/长稳/渠道、其他设备/build/API。N1a↔N1b 双向不外推。

## 流程

1. 本判据文档（r0）交独立审查席确认后方可开始测量；
2. 实现（ArkTS VpnExtension + Rust 泵 + NAPI）经 host-only 验证与自测；
3. **物理 campaign 须用户显式授权**（pre-E8 native 例外，独立 AUTH/pair，沿 G0 13 门范式）；
4. 正式物理 campaign -> 证据登记（双轴）-> 记录级独立审查 -> 收口。
