# N1a 门计划与判据预注册（native WG 数据面 × 回环数据泵，Emulator）

最后核验：2026-08-30 ｜ 状态：`criteria-revised-r2-pending-re-review`（两轮审查修订；r2 修复停止条件节 semantic-drift 并采纳全部 minor；同一审查席复审确认前不得开始测量；测量后不得追认或修改判据——ADJ-T0-NATIVE-NX-20260830-0001 §二.7）

## 定义与归属

N1a 是 [native N1-Nx 治理决议](native-nx-governance.md) N1 拆分的 Emulator 子门：在官方 API 24 x86_64 phone Emulator 上，验证 **BoringTun 0.7.1（`ffi-bindings`，N0 同一固定 crate）作为 native WireGuard 数据面核心，经"tun.Device 等效数据泵"处理双向真实报文**——握手建立、逐包加密转发、完整性、吞吐下限、背压行为与资源稳定性。N1b（物理 VpnExtension fd）与本门互不外推。

- **证据身份**：campaign `N1A-EMU24-20260830-0001`；evidence `EV-N1A-EMU24-20260830-0001`；attempt initial、无 retry。
- **环境**：官方 API 24 x86_64 phone Emulator（`netbird_api24_phone` 实例、HDC `127.0.0.1:10000`，沿用 E0-E2/N0 冻结链路：CLI `6.1.1.290`、`phone_all_x86` image、Hvigor 构建）。Emulator HDC 属既有 E/N campaign 授权面；不触物理设备。
- **实现载体**：`spikes/n0-native-core/` 的后继扩展（同 crate、同 `--offline --locked`、同 BoringTun 0.7.1 checksum `15dd6a8a…`；`device` feature/socket2 仍禁）。

## 数据面 oracle（单列）

数据面 oracle = 同进程双 `new_tunnel` 实例互为 peer，无外部 WG/NetBird 对照。证明"真实加密转发"的 machine 条件即 C2 三次握手 + C3 的 `WRITE_TO_NETWORK`/`WRITE_TO_TUNNEL_IPV4`/密文≠明文/`tx_bytes`/`rx_bytes` 字节账。禁止任何合成 fixture、禁止绕过 ffi 的 memcpy 泵；外层信道只搬运 ffi 输出的字节。

## 探针设计（双隧道回环泵）

- 两个 `new_tunnel` 实例（A/B），密钥互逆：A 的 peer 公钥 = B 的公钥，反之（`x25519_secret_key`/`x25519_public_key` 生成，**密钥经 `x25519_key_to_base64` 以 base64 C 字符串传入**；两侧 tunnel `index` 必须不同；`keep_alive=1`）。
- 回环信道：两个 `AF_INET/SOCK_DGRAM` 已 connect 的 `127.0.0.1` socket，`O_NONBLOCK`（不使用"UDP socketpair"这一自相矛盾措辞；不用任何 VPN/TUN 平台 API）。
- 握手与数据搬运全部按 ffi 合同：`wireguard_read` 返回 `op==WRITE_TO_NETWORK` 时，对同一 tunnel 以空 src 重复 `wireguard_read` 直至 `WIREGUARD_DONE`（强制 drain）。
- NAPI 薄层沿用 N0 模式（结构化结果、fail-closed、同步导出 + 单行 marker），marker 前缀 `N1A_RESULT`、tag `N1aProbe`。

## 判据（预注册 r1，全部 machine-verified；逐字采纳第一轮独立审查修正文本）

| # | 判据 | pass 条件（fail-closed） |
| --- | --- | --- |
| C1 | 库加载 | 唯一 native 成员在普通 `EntryAbility` 内 dlopen 加载成功（N0 同路径）；非 0 退出码/异常 → fail |
| C2 | 握手建立 | 仅 A 侧调用 `wireguard_force_handshake`（禁止双侧同时 initiation）。按 ffi 合同转发：A init → B `wireguard_read` 得 `WRITE_TO_NETWORK`（response）→ 回 A `wireguard_read` 得 `WRITE_TO_NETWORK`（keepalive data）→ 回 B `wireguard_read`（空 keepalive data，该步 `op==WIREGUARD_DONE` 而非 `WRITE_TO_TUNNEL_IPV4`——实现者不得误等 IPV4）；每一次 `op==WRITE_TO_NETWORK` 必须对同一 tunnel 以空 `src` 重复 `wireguard_read` 直至 `WIREGUARD_DONE`。两侧 `wireguard_stats.time_since_last_handshake >= 0`（禁止引用不存在的 `state`）。时间盒 30s 内任一侧仍为 -1，或任一步 `WIREGUARD_ERROR` → **fail**（非 blocked） |
| C3 | 逐包完整性 | 合成包必须是合法 IPv4（version=4，IHL≥5，total length 与缓冲一致），`(direction, round, seq)` 在 payload。每包：`wireguard_write` 返回 `op==WRITE_TO_NETWORK` 且 `size >= inner_len+32`；信道 UDP 载荷 ≠ 明文且前 4 字节 LE `uint32==4`；对端 `wireguard_read` 返回 `op==WRITE_TO_TUNNEL_IPV4` 且明文逐字节相等。两侧 `stats.tx_bytes`/`rx_bytes` 增量等于该方向内层字节合计。零丢失/零额外**只计** `WRITE_TO_TUNNEL_IPV4` 内层包，不计外层握手/keepalive UDP。C3 必须在**未缩小**的 socket 缓冲上作为独立 unsaturated 阶段运行（不得与 C5 饱和共用同一计数窗口）。负载量：每方向 10 轮 × 200 包 × 1024 字节。任一不符 → fail |
| C4 | 吞吐下限 | 时钟仅覆盖 C3 unsaturated 泵送：第一包 `wireguard_write` 前启动，最后一包 `WRITE_TO_TUNNEL_IPV4` 校验后停止；排除握手、C5、C6。双向合计有效明文字节（与 C3 内层字节一致）÷ 该时间 ≥ **5 MiB/s**（1 MiB = 1048576 B）；低于即 fail，不调参重测 |
| C5 | 背压 | **独立阶段**（C3 已 pass 之后）。信道 socket `O_NONBLOCK`；预注册 `SO_RCVBUF=SO_SNDBUF=4096`（实测 `getsockopt` 记入证据）。泵必须交错收发，`EAGAIN`/`ENOBUFS` 时重试不丢已 `sendto` 成功的数据报、不损坏已 `recvfrom` 的字节；时间盒 10s 内返回、无死锁。时间盒内至少一次真实 `errno∈{EAGAIN,ENOBUFS}` → `backpressure_induced=true`；死锁/超时/已收包损坏 → fail。**零次** EAGAIN/ENOBUFS → `backpressure_induced=false`，本子项登记 **`not-triggered`（≠ fail）**，不得把 not-triggered 写成背压已验证。禁止把"部分写"列为 UDP 预期；C5 丢包不得回写 C3。聚合：C5 记录 `pass-induced`/`not-triggered`/`fail` 三态；not-triggered **不阻止** N1a overall pass（N1a 的背压主张降级为"attempted, not induced on this channel"写入 evidence）；C5 fail → overall fail |
| C6 | tick 路径 | 两侧 `new_tunnel(..., keep_alive=1, ...)`。C3 之后插入 ≥3 个无数据间隔，每间隔 ≥1s，每侧至少调用一次 `wireguard_tick`。至少 3 次 tick 返回 `op==WRITE_TO_NETWORK`（persistent keepalive；空内层，外层经信道交给对端 `wireguard_read`，对端 `op==WIREGUARD_DONE`）。禁止 `WIREGUARD_ERROR`。间隔结束后两侧 `time_since_last_handshake >= 0`。tick 次数以探针计数器记录，**不得**向 `wireguard_stats` 索取 tick 字段。不满足 → fail。本阶段耗时不计入 C4 |
| C7 | 资源稳定 | 快照 locus 与 E2 相同：主 ArkTS TID 上、同一 native 调用内 T0=创建 socket/`new_tunnel` 之前，T3=`tunnel_free`+全关 socket 之后（与 C8 同一基线）。`/proc/self/fd` 与 `/proc/self/task` 条目计数：T3==T0，否则 fail。探针不得自建 pthread。RSS 只登记、不设门 |
| C8 | 清理 | `tunnel_free`×2、socket 全关；fd 快照回到与 C7 共用的 T0 基线 |
| C9 | 结果页 | 恰好一行 `N1A_RESULT\|verdict=<PASS\|FAIL>\|c5=<induced\|not-triggered\|fail>\|throughput_mibps=<x.xx>`（marker 字段集**钉死为这四个**，禁止竖线/换行污染；其余结果字段只进结构化 evidence 对象，不得事后升格为门）。页面截图非空（沿 E2 `YAVG`/非黑帧）且页上 PASS/FAIL 与 marker 一致；缺 marker、重复 marker、页/marker 不一致 → fail（非 blocked） |

## 聚合规则（fail-closed，逐字采纳审查文本）

overall `pass` 当且仅当 C1–C4、C6–C9 均为 pass，且 C5 ∈ {pass-induced, not-triggered}。任一 Ci 为 fail → overall `fail`。`blocked` **仅**登记环境类：Emulator 启动失败、HDC 退化（E1 之 25min 模式、有界短循环不恢复）、构建输入漂移（BoringTun checksum/`--offline --locked`/SDK）。**C2 握手失败不是 blocked。** 任何判据无法按本文字面求值 → overall `blocked` + 返回 T0，禁止现场补"等价字段"。不改写仓内既有判定。

## 停止条件

N0 决议第 9 条停止条件（共 5 项，无 6-9）全文沿用如下；出现任一即停止并返回 T0：

1. native core 需要私有/高维护 patch；
2. 正式 NDK 无法构建/加载；
3. 真实 VPN fd/protect 不可满足——本门不测该项（N1a 非范围）；归 N1b/N2。N1b/N2 若证实不可满足，停并回 T0，不得在 N1a 把「本门未使用 VPN fd」写成第 3 条已触发；
4. 固定版本 compat oracle 无法定义；
5. 范围扩展到第二协议面。

并追加本门特化：

1. 构建失败（含 BoringTun checksum 漂移、cargo 离线锁失败）；
2. C1/C2 fail（加载或握手失败——测量事实，登记后停）；
3. Emulator/HDC 基础设施退化（有界短循环内不恢复即 blocked 停）；
4. 任何白名单外设备命令、越权能力（VPN/protect/特权）——立即停止并登记违规；
5. 测量开始后判据不得修改；判据缺陷导致无法按字面求值 → blocked + 返回 T0，不得现场放宽。

## 非范围（双向不外推）

本门不主张：arm64 加载（仍 compile-only）、物理设备、VpnExtension fd/TUN/destroy（N1b）、socket protect（N2）、management/signal/relay/ICE（N3-N5）、产品实现；x86_64 Emulator 结论不外推任何其他元组。背压主张上限：若 C5 为 not-triggered，本门仅主张"attempted, not induced on this channel"。

## 流程

1. 本判据文档（r1）交**同一独立审查席**复审确认后方可开始测量；
2. 实现（Rust 泵 + NAPI 薄层 + runner）经 host-only 验证与自测，并对照 r1 判据逐条核对；
3. 正式 Emulator campaign（runner 产 sealed evidence：沿 N0 `--dry-run`/formal 模式）；
4. 证据登记（双轴）→ 记录级独立审查 → 收口。
