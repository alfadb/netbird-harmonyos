# N1b 门计划与判据预注册（native WG 数据面 × 物理 VpnExtension fd 集成）

最后核验：2026-08-31 ｜ 状态：`criteria-r1-superseded-pending-disc`

> **T0 已裁定并经用户批准（2026-08-31）**：见 [`ADJ-T0-N1B-20260831-0001`](native-nx-n1b-adjudication.md)。r1 判据经三席跨厂商独立审查全部 fail，其治理级根因已裁决完毕。
>
> **下一步不是写 r2。** 按该决议 §四/§八，N1b r2 须**依 `N1BDISC` 发现 campaign 的实测事实**冻结，而非在未实测前提上迭代措辞。当前工作项是 DISC 判据重拟（材料包 D1-D7 草案已被三席一致否决）。
>
> **本门现行禁令**：不得冻结 r2、不得开始任何测量、不得分配 N1b 的 AUTH/pair 或 evidence ID。DISC 的证据 ID 须待 `evidence-schema.md` 门代码扩展完成后方可分配；DISC 与 N1b **不得共用同一 AUTH/pair**；任何物理执行仍须用户逐次显式授权新 AUTH。
>
> **r2 须落实的已裁定变更**：FD-P1 现文废止（恒真判据）；C10 `post-destroy-unobservable` → overall `blocked`；背压/部分写 `not-triggered` 不阻 pass 但义务不关闭（入[台账](open-obligations-ledger.md) OB-01/OB-02）；E4 义务按决议 §一 承担并须**实际生效 oracle**；IPv6 记 `N/A-undeclared`；时间盒表与 P2 样本数/窗口为冻结前置。
>
> 下方 r1 判据正文**保持原样不改写**，作为审查对象的字面记录。

**r1 审查结果（2026-08-31，三席跨厂商隔离独立审查，全部 fail）**：`isolated-xai-grok-4-6-reviewer` 6 blocker / 10 major / 4 minor；`isolated-openai-gpt-5-6-sol-reviewer` 10 / 7 / 2；`isolated-deepseek-v4-pro-reviewer` 1 / 1 / 5。**三席独立收敛的唯一共同 blocker**：C10 允许 `post-destroy-unobservable → blocked → overall pass`，使本门可在未建立 §二.2 post-destroy 义务时 overall pass（须改为 overall `blocked`）。三席一致确认已可冻结的部分：两条独立单向链的方向、C4/C8 与 BoringTun 0.7.1 op 序列逐字一致、吞吐判据删除不构成削弱、执行位点收敛到 VpnExtension、门代码扩展前置、双轴与终态优先级、N0 五项停止条件。下方 r1 判据正文**保持原样不改写**，作为审查对象的字面记录。

**r0 审查结果（2026-08-31，两席跨厂商隔离独立审查，均 fail）**：`isolated-xai-grok-4-6-reviewer` 8 blocker / 9 major / 6 minor；`isolated-deepseek-v4-pro-reviewer` 6 blocker / 8 major / 8 minor。第三席 `isolated-openai-gpt-5-6-sol-reviewer` 运行未干净完成，按 `EV-E3-PHYS1API26-20260807-0002` 既有先例记 **attempt-not-counted、不作 verdict**，其内容仅作非权威线索，不得作为审查席证据引用。r0 从未冻结、从未开始测量，故本次修订不受 §二.7 的「测量后不得修改」约束。r0 判据文本已由本 r1 整体取代。

## 定义与归属

N1b 是 [native N1-Nx 治理决议](native-nx-governance.md) N1 拆分的物理子门：在**物理冻结元组**上验证 **BoringTun 0.7.1 native WG 数据面核心与 VpnExtension 真实 tun fd 的集成**--fd 合同实测（治理绑定条款 §二.2 全部义务）、握手、双向真实加密报文、资源与清理。N1a（Emulator 回环泵）与本门互不外推。

- **证据身份**：授权 `AUTH-N1B-PHYS1API26-<YYYYMMDD>-0001`；campaign `N1B-PHYS1API26-<YYYYMMDD>-0001`；evidence `EV-N1B-PHYS1API26-<YYYYMMDD>-0001`。attempt `initial`、retry `N/A`、单次执行不重试不换 ID。日期在正式授权时冻结。
- **门代码前置**：`docs/evidence-schema.md` 现行门代码枚举为 R0-R10/E0-E8，未含 N 门（`N1A` 已是既成先例但 schema 未随 N1-Nx 治理更新）。**ID 分配前须由 schema 持有人登记 N 门代码扩展**，否则本门证据身份与 schema 字面冲突。
- **环境**：物理冻结元组（HarmonyOS / PLA-AL10 / `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` / API 26 / aarch64 / arm64-v8a）--**pre-E8 native 物理例外**（治理 §三）：独立 AUTH/pair、冻结元组**与输入哈希**、白名单 HDC、单次执行、双向不外推、禁止性能/长稳/渠道/产品外扩。
- **§二.8 的正确适用**：治理 §二.8 的 arm64 同核心加载前置**约束的是 N2b，不是 N1b**。N1b 的 C1 **产出**该证据，不是以该证据为启动前提。r0 的自指措辞（「物理运行前须有…本门自身即为该证据的首次产生」）已删除。
- **E4 映射义务处置**（治理 §二.8「E4（TUN 配置/地址/MTU/IPv4/清理）与 E6（双向泵）映射到 N1a/N1b，映射不免除义务」）：本门只覆盖**冻结 VpnConfig 所实际行使的那一份** TUN 配置/地址/MTU/IPv4 报文/清理。凡本冻结配置未行使的 E4 面（多地址、多路由/路由表交互、DNS 声明、MTU 变更与重建）**均未被本门覆盖，保持开放**；不得因 N1b pass 而主张 E4 整体完成，其去向由后续门治理另行指派。

## 冻结的执行面

**实现载体**：`spikes/n1b-phys-fd-hap/`（新独立目录）。Rust 核心复用 N1a 的 ffi 调用纪律（`spikes/n1a-native-dataplane/src/pump.rs`），但数据源/汇为真实 tun fd，不是回环 socket。

**构建输入（逐字冻结，不得省略）**：

| 项 | 冻结值 |
| --- | --- |
| crate | `boringtun` `0.7.1` |
| checksum | `15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939` |
| feature | `default-features = false, features = ["ffi-bindings"]` |
| 禁用 | `device` feature、`socket2`--**BoringTun `device/tun_linux.rs:65-73` 的 `TunSocket::new` 会把纯数字 name 直接当 fd 接管，`:45-49` 的 `Drop` 无条件 `close(self.fd)`；一旦启用并按 fd 号接线即违反 FD-L7** |
| cargo | `--offline --locked` |

**VpnConfig（冻结；create 时逐字提交并登记实际生效值）**：

| 字段 | 冻结值 | 理由 |
| --- | --- | --- |
| address | `10.99.0.1` / prefix `32` | tun 自身地址，IN 链目的地址 |
| route | `10.99.0.0/24` | **窄路由**：使 OUT 链生成流量进入 tun，同时使 `127.0.0.1` 外层信道不落入路由范围（避免递归；仍由 P2 实测断言） |
| mtu | `1400` | ≥ 1024 payload + 20 IPv4 + 8 UDP，留余量 |
| dns | **不设置（N/A）** | 冻结为显式 N/A，转交 N2a |

**地址与端口（冻结）**：

| 角色 | 冻结值 |
| --- | --- |
| OUT 链生成 socket 目的 | `10.99.0.2:47001`（路由内；无需任何监听方，内核照样把包投出 tun） |
| IN 链 sink socket | `bind 0.0.0.0:47002`；注入包目的为 `10.99.0.1:47002` |
| 外层信道 | 两个 `AF_INET/SOCK_DGRAM`、`127.0.0.1`、各自 bind 后互相 `connect`、`O_NONBLOCK`。**禁止使用「UDP socketpair」措辞**（UDP 无 socketpair；N1a r3 已弃用该自相矛盾表述） |
| 包身份 | payload 前 16 字节为 `magic(8)="N1BPKT01" \| dir(1) \| round(1) \| seq(2) \| len(4)`；不含该 magic 的 tun 包记入 `foreign_packets_observed` 观察字段，不参与判定 |

**tun I/O 纪律（冻结）**：dup 副本一律 `O_NONBLOCK` + 有界 `poll`。**禁止任何无界阻塞 read/write**--这既是 shutdown unblock 可测的前提，也消除 r0 的「阻塞到操作员中止」失败模式。

**执行位点（冻结）**：全部探针逻辑在 **VpnExtensionAbility（`type: vpn`、`exported=false`）进程**内同步执行；所有判定量经该进程的 HiLog marker 发射。EntryAbility 仅负责 UI 触发与 observation-only 截图，**不承担任何判定**（E3 已证 `<bundle>:vpn` 与 Entry 非同一进程且 destroy 后 terminal，跨进程 verdict 传递无合同，r0 的「结果页与 marker 一致」要求因此删除）。

## 前提与前提检查

以下两条是本门设计所依赖、但在本冻结元组上**尚无实测**的平台前提。二者均预注册为独立检查阶段，**先于**任何数据链判定执行：

- **P1（OUT 链有源）**：VPN 应用自身 socket 发往本 VPN 路由覆盖地址的流量，会被内核投递到本应用的 tun fd。若不成立，OUT 链无明文来源。
- **P2（外层不递归）**：`127.0.0.1` 外层信道流量不进入 tun。

**P1/P2 失败的分类已预注册**：任一不成立 → overall `blocked`（前提不成立，非功能 fail）+ 触发停止条件 S6 并返回 T0。**不得**现场改路由、改地址或改设计后继续测量。

## fd 合同（治理 §二.2 逐条映射；N1b 实测建立，不得继承）

E3 探针契约（`fcntl(F_GETFD)` 只读快照、明确禁 `close`/`dup`/`read`/`write`，见 `docs/e3-physical-preflight.md:99,104`）与本门 dup 副本契约不同，**不可混引**。`F_DUPFD_CLOEXEC` 在 API 26 SDK sysroot 已定义（`fcntl.h:53`，cmd `1030`）--该事实只证编译面可用，运行时行为仍由 FD-L1 实测。

### live 段（PH2 建立，destroy 之前）

| # | 合同项 | §二.2 义务 | 实测要求（pass 条件） |
| --- | --- | --- | --- |
| FD-L0 | 所有权表 | 登记所有权表 | 探针产出 fd ledger：每个 fd 的 号/角色/创建点/关闭责任方/关闭点，作为结构化证据字段落盘；缺失 → fail |
| FD-L1 | dup 语义与 CLOEXEC | dup 语义与 CLOEXEC | `fcntl(orig, F_DUPFD_CLOEXEC, 0)` 返回新 fd 号（≠ 原始号）且 `F_GETFD` 含 `FD_CLOEXEC`。**若该 cmd 在运行时不可用**：登记 errno，改用 `dup()` + `fcntl(F_SETFD, FD_CLOEXEC)` 两步并同样验证 `FD_CLOEXEC` 置位--两条路径均为 pass，路径选择逐字登记；两条都失败 → fail |
| FD-L2 | 只消费副本 | native 只消费 dup、禁止把原始 fd 交给 BoringTun/tun.Device | 静态断言：源码中 BoringTun 调用与所有 tun POSIX I/O 的 fd 实参只能是副本变量；动态断言：ledger 中原始 fd 号不出现在任何 I/O 调用点。任一不符 → fail。（注：BoringTun ffi 本身不接受 fd，只操作调用者缓冲区--见 `ffi/mod.rs:320-352`；本条约束的是 POSIX I/O 的 fd 实参） |
| FD-L3 | `O_NONBLOCK` 共享副作用 | `O_NONBLOCK` 对共享 open-file description 的副作用 | 对副本设 `O_NONBLOCK` 后，读取**原始 fd** 的 `F_GETFL`：逐字登记是否含 `O_NONBLOCK`。**本条为观察项，不设门**--共享 open-file description 是 POSIX 语义推断，本元组未实测，不得把预期写成 pass 条件 |
| FD-L4 | EAGAIN | EAGAIN | 无数据可读时对副本 `read` 必须返回 `-1/EAGAIN`（非阻塞语义成立）；至少一次真实 `EAGAIN` 被观察并登记。零次且 `poll` 亦从不超时 → 记 `not-observed`（≠ fail）；返回其它错误族 → 逐字登记 errno，fail |
| FD-L5 | 部分写 | 部分写 | 对副本 `write` 一个完整 IPv4 包，逐字登记返回值：`n == len`（完整写）/ `0 < n < len`（部分写）/ `-1/EAGAIN`。**观察项不设门**；但若出现部分写，探针必须按登记策略处理（丢弃该包并计入 `partial_write_count`，不得静默拼接） |
| FD-L6 | 背压 | 背压 | 独立阶段（C7）：在 10s 有界时间盒内持续向副本写入直至出现 `EAGAIN` 或时间盒到期；不死锁、不损坏已读字节。三态 `induced` / `not-triggered` / `fail`--**本条是 tun fd 背压，与外层 UDP socket 缓冲无关；r0 用 `SO_RCVBUF` 缩小 UDP 缓冲冒充本条的做法已删除** |
| FD-L7 | 单一 close 责任（live 面） | 单一 close 责任 | 静态断言：探针源码在任何路径（成功/异常/清理）均无 `close(原始fd)` 调用点；动态断言：destroy 之前原始 fd 的 `F_GETFD` 始终成功。r0 的「拒绝或跳过」空心表述已删除 |

### post 段（PH10，destroy 之后、进程 terminal 之前）

| # | 合同项 | §二.2 义务 | 实测要求（pass 条件） |
| --- | --- | --- | --- |
| FD-P1 | shutdown unblock | shutdown unblock | destroy 后对副本的有界 `poll`（500ms）必须在时间盒内返回（不得挂起）；返回的事件集与后续 `read` 结果逐字登记 |
| FD-P2 | destroy 后有效期 | destroy 后 fd 有效期、EBADF | **门控部分**：原始 fd 的 `fcntl(F_GETFD)` 返回 `-1/EBADF`（证明 destroy 履行了唯一 close 责任）。**观察部分**：副本的 `F_GETFD` 与 `read` 结果逐字登记 errno--**允许结果集不设门**（EOF / EAGAIN / EBADF / EIO / 其它均为合法观察）。r0 把 EOF/EAGAIN 预写为 pass 条件的做法已删除：本元组从未实测该行为，不得以平台行为偏离预写文本判 fail |
| FD-P3 | 重复 close | 重复 close | 探针 `close(副本)` 一次成功；再次 `close(副本)` 或 `fcntl(副本, F_GETFD)` 返回 `-1/EBADF`。不符 → fail |
| FD-P4 | fd 号复用 | 复用 | 副本关闭后新建一个 `socket()`，逐字登记其 fd 号是否等于已关闭的副本号。**观察项不设门** |
| FD-P5 | 泄漏 | 泄漏 | 探针自有 fd 全部关闭（C9 + 本段逐个 `fcntl` 核对）；`tunnel_free`×2 各恰好一次。进程级 `/proc/self/fd` 计数仅作观察字段（沿 N1a r3 C7 裁决，**不设门**） |

## 探针设计：两条独立单向链

r0 把「tun fd → 加密 → UDP → 解密 → tun fd」写成单条闭环，在 tun 语义下不成立：同一 tun fd 的 read 与 write **不是互逆管道**（read 取内核待发包，write 向内核注入包），写回不会成为下次可读数据。r1 改为**两条各自从独立输入闭到独立输出的单向链**，每条恰好真实行使一个方向的 tun 系统调用：

**OUT 链（真实 tun `read`）**

```
生成 socket sendto(10.99.0.2:47001)
  -> 内核按 route 10.99.0.0/24 投出 tun
  -> 探针 poll+read(副本)               <-- 真实 tun read
  -> wireguard_write(A) => WRITE_TO_NETWORK
  -> 外层 UDP 127.0.0.1
  -> wireguard_read(B)  => WRITE_TO_TUNNEL_IPV4
  -> 与「从 tun read 到的整包字节」逐字节比对
```

**IN 链（真实 tun `write` + 内核交付）**

```
探针合成合法 IPv4/UDP 包（src 10.99.0.2:47001 -> dst 10.99.0.1:47002，
  IPv4 头校验和正确；UDP 校验和正确或 0）
  -> wireguard_write(B) => WRITE_TO_NETWORK
  -> 外层 UDP 127.0.0.1
  -> wireguard_read(A)  => WRITE_TO_TUNNEL_IPV4
  -> 与合成整包逐字节比对（证明 crypto 路径）
  -> poll+write(副本)                   <-- 真实 tun write
  -> sink socket recvfrom() 收到 payload（证明内核确实接收了注入包）
```

两条链的 A/B 角色互换，因此**双向真实加密报文**由两个隧道各承担一次加密与一次解密。密钥互逆（A 的 peer 公钥 = B 的公钥，反之），经 `x25519_key_to_base64` 以 base64 C 字符串传入，两侧 `index` 必须不同，`keep_alive=1`。

## 冻结阶段顺序

r0 的 C2（含 destroy 后子项）排在 C3（需活 fd）之前，按字面执行必然自毁。r1 冻结唯一全序，barrier 之间不得重排：

```
PH0  授权与 create（操作员 Allow -> onCreate -> VpnConnection.create -> 原始 fd）
PH1  C1  库加载门（Extension 进程内）
PH2  C2  fd 合同 live 段 FD-L0..L3、L7（非破坏性）
PH3  C3  前提检查 P1 / P2
PH4  C4  握手
PH5  C5  OUT 链  /  C6  IN 链
PH6  C7  tun 背压（FD-L4/L5/L6）
PH7  C8  keepalive
PH8  C9  探针资源门 T3a：关闭并逐个核对「副本以外」的全部探针 fd；tunnel_free×2
PH9       发射 N1B_PRE marker（**destroy 之前**，承载 PH1-PH8 全部判定量）
PH10 C10 VpnConnection.destroy() -> fd 合同 post 段 FD-P1..P5（含关闭副本）
PH11 C11 发射 N1B_POST marker；封签
PH12      Extension 进程 terminal（沿 E3 S7 观察，非本门判定）
```

**每个等待点均有单调时钟有界时间盒**（超时分类预注册）：

| 阶段 | 时间盒 | 超时分类 |
| --- | --- | --- |
| PH0 create | 60s | blocked（授权/平台） |
| PH3 P1/P2 | 各 10s | blocked（前提不成立） |
| PH4 握手 | 30s | **fail**（沿 N1a C2） |
| PH5 每链 | 各 60s | fail |
| PH6 背压 | 10s | 三态见 FD-L6 |
| PH7 keepalive | ≥3 间隔 × 各 ≥1.1s，总 30s | fail |
| PH10 post 段 | 10s | 见 C10 |

## 判据（预注册 r1，全部 machine-verified，fail-closed）

| # | 判据 | pass 条件 |
| --- | --- | --- |
| C1 | 库加载 | 唯一 arm64-v8a native 成员 dlopen 成功，且**加载发生在 Extension 进程**：登记 `pid`、`/proc/self/cmdline` 含 `<bundle>:vpn`、`process_model=vpnextension`、`.so` 成员名与 SHA-256、已解析符号集（七个 ffi 入口）。加载失败须发射 `dlerror` 短 marker。任一缺失或不符 → fail。**本条 pass 即构成 §二.8 所需的 arm64 同核心加载证据；C1 fail 亦须封签为 arm64 加载负面证据** |
| C2 | fd 合同 live 段 | FD-L0、L1、L2、L7 全部 pass（L3 为观察项）。任一 fail → overall fail（非 blocked） |
| C3 | 前提检查 | P1：生成 socket 发出的带 magic 包在 10s 内从 tun 副本读到。P2：外层 UDP 收发期间 tun 副本上未读到任何 `127.0.0.1` 外层包。任一不成立 → **blocked** + 停止条件 S6 |
| C4 | 握手 | 仅 A 侧调用 `wireguard_force_handshake`（dst ≥148 B）。按 ffi 合同转发：A init → B `wireguard_read` 得 `WRITE_TO_NETWORK`（response）→ 回 A `wireguard_read` 得 `WRITE_TO_NETWORK`（keepalive）→ 回 B `wireguard_read` 得 **`WIREGUARD_DONE`**（空内层；**实现者不得误等 `WRITE_TO_TUNNEL_IPV4`**）。每一次 `op==WRITE_TO_NETWORK` 必须对同一 tunnel 以**空 src** 重复 `wireguard_read` 直至 `WIREGUARD_DONE`。两侧 `wireguard_stats.time_since_last_handshake >= 0`（禁止引用不存在的 `state` 字段）。30s 内任一侧仍为 -1，或任一步 `WIREGUARD_ERROR` → fail |
| C5 | OUT 链 | 5 轮 × 50 包 × 1024 B payload（`expected_packets_out = 250`）。每包：tun `read` 返回完整合法 IPv4（version=4、IHL≥5、total length 与读长一致）且含冻结 magic；`wireguard_write(A)` 返回 `op==WRITE_TO_NETWORK` 且 `size >= inner_len + 32`；外层 UDP 载荷 ≠ 明文且前 4 字节 LE `uint32 == 4`；`wireguard_read(B)` 返回 `op==WRITE_TO_TUNNEL_IPV4` 且与 tun 读到的整包**逐字节相等**。A `tx_bytes` / B `rx_bytes` 增量等于该方向内层字节合计。零丢失零额外（只计 `WRITE_TO_TUNNEL_IPV4` 内层包）。任一不符 → fail |
| C6 | IN 链 | 5 轮 × 50 包 × 1024 B payload（`expected_packets_in = 250`）。每包：合成包为合法 IPv4/UDP 且 IPv4 头校验和正确；`wireguard_write(B)` → `WRITE_TO_NETWORK`（同 C5 的 size / 密文断言）；`wireguard_read(A)` → `WRITE_TO_TUNNEL_IPV4` 且与合成整包逐字节相等；tun `write` 返回 `n == len`；**sink socket 在 5s 内 `recvfrom` 到该包 payload 且逐字节相等**。B `tx_bytes` / A `rx_bytes` 字节账同 C5。任一不符 → fail |
| C7 | tun 背压 | 独立阶段（C5/C6 之后，不与其共用计数窗）。FD-L4 pass；FD-L5、FD-L6 按其三态/观察规则求值。10s 时间盒内不死锁、已读字节不损坏。`fail` → overall fail；`not-triggered` 不阻止 overall pass，但本门背压主张降级为「attempted, not induced on this fd」写入证据 |
| C8 | keepalive | `keep_alive=1`。C5/C6 之后插入 ≥3 个无数据间隔、每间隔 ≥**1.1s**（1.0s 处于 BoringTun 秒级取整边界，N1a 实现取 1100ms）。每侧每间隔至少调用一次 `wireguard_tick`；累计至少 3 次 tick 返回 `op==WRITE_TO_NETWORK`，其外层经信道交给对端 `wireguard_read`，**对端返回 `op==WIREGUARD_DONE`**（空内层）。禁止 `WIREGUARD_ERROR`；tick 次数由探针计数器记录，**不得**向 `wireguard_stats` 索取 tick 字段。间隔后两侧 `time_since_last_handshake >= 0`。不满足 → fail |
| C9 | 探针资源门（T3a） | 门控对象为**探针自有资源全清单**：外层 UDP socket ×2、OUT 生成 socket、IN sink socket（副本归 C10）。T0 记录已开 fd 集合，T3a 对「T0 后新出现的探针 fd（副本除外）」逐个 `fcntl(F_GETFD)` 确认已关闭，任一仍开 → fail。`tunnel_free`×2 各恰好一次（由 Tunnel Drop 保证）--**`tunnel_free` 的唯一归属在本条，C10 不重复计数**。静态断言无 `pthread_create` / `std::thread::spawn` / NAPI worker / `napi_create_threadsafe_function`。进程级 `/proc/self/fd`、`/proc/self/task`、RSS 仅写观察字段，不设门 |
| C10 | fd 合同 post 段 | FD-P2 的门控部分（原始 fd `EBADF`）与 FD-P3 pass；FD-P1、FD-P4、FD-P5 按其规则求值。**若 Extension 进程在 PH10 完成前 terminal 致本段不可观察**：记 `post-destroy-unobservable` → C10 `blocked`（沿 E3「该元组上 marker 时序不可靠」与 strict-process-boundary 先例），**不得**写成 pass，也不得因此把 overall 写成 fail |
| C11 | 结果通道 | `N1B_PRE` 恰好一行且字段完整；`N1B_POST` 恰好一行或按 C10 记 `post-destroy-unobservable`；detail chunk 完整重组（`index/count/sha256` 一致）。`N1B_PRE` 缺失、重复、字段缺项 → **fail**（非 blocked） |

**关于吞吐**：r0 的 C4 吞吐下限（1 MiB/s）已删除。治理 §二.1 把吞吐/背压/资源判定指派给 **N1a**，把 N1b 定为「真实 fd 集成 + 握手 + 双向真实报文」；在 N1b 设吞吐门既超出本门 charter，又与本门「不主张性能」的非范围声明自相矛盾，且 500 KiB 负载在 1 MiB/s 处窗口仅约 0.5s，时钟噪声足以造成假 fail。活性由各阶段时间盒保证。实测速率作为**观察字段**（`observed_rate_mibps_out` / `_in`，含时钟起止定义）落盘，**不构成任何性能结论**。

## 结果通道（冻结）

```
N1B_PRE|live_verdict=<PASS|FAIL>|c1=<pass|fail>|c2=<pass|fail>|c3=<pass|blocked>|
  c4=<pass|fail>|c5=<pass|fail>|c6=<pass|fail>|c7=<induced|not-triggered|fail>|
  c8=<pass|fail>|c9=<pass|fail>|fd_orig=<n>|fd_dup=<n>|pkts_out=<n>|pkts_in=<n>
N1B_POST|p1=<returned|timeout>|p2=<ebadf|other>|p3=<pass|fail>|
  p4=<reused|not-reused>|p5=<pass|fail>|dup_close=<ok|fail>
```

`live_verdict` 只是 PH1-PH8 的活期判定，**不是** overall；overall 由 runner 按下节聚合规则从两个 marker 合成，任何单个 marker 都不得被当作 overall 结论。字段集冻结如上；**结构化 detail JSON 分段输出**（每片带 `index`/`count`/`sha256`，重组后校验），承载 fd ledger、逐 errno 登记、字节账、观察字段。**禁止只依赖单行 detailJson**（N1a 0001/0005 已证 hilog 行长截断会使判定量不可恢复）。任何终态（pass/fail/blocked）runner 均须封签。异常路径必须自行发射短诊断 marker（N1a 0002 曾因 overlay throw 未发射 marker 而 fail）。

## 聚合规则（fail-closed）

overall `pass` 当且仅当 C1、C2、C4、C5、C6、C8、C9、C11 均 pass，且 C3 = pass，且 C7 ∈ {`induced`, `not-triggered`}（marker 与聚合使用同一组字面量，不另设 `pass-induced` 别名），且 C10 ∈ {pass, blocked}。任一为 fail → overall `fail`。

`blocked` 仅限：C3 前提不成立、环境类（设备连接失败、OTA/元组漂移、HDC 退化、构建输入漂移）、C10 `post-destroy-unobservable`。**C2 任何 live 段子项 fail → overall fail（非 blocked）**。证据污染、hash 不一致、阶段顺序破坏或跨 attempt 拼接 → overall `invalid`（沿 E3 `invalid > fail > blocked > pass` 优先级）。任何判据无法按本文字面求值 → `blocked` + 返回 T0，禁止现场补「等价字段」。

**双轴**：`record_status`（审查轴）与 `verdict`（功能轴）独立；`reviewed-pass` 只表示记录经独立审查合格，**不**等于功能通过；只有 `reviewed-pass` + `verdict: pass` 同时成立才可继续（`docs/evidence-schema.md`）。

## 停止条件（逐条写出，不再引用他门）

N0 决议第 9 条五项沿用：

1. native core 需要私有/高维护 patch；
2. 正式 NDK 无法构建/加载；
3. 真实 VPN fd/protect 不可满足--**本门实测 fd 面**：若 fd 合同（C2/C10 门控子项）在本元组不可满足，即触发本条，停止并返回 T0；protect 面仍归 N2；
4. 固定版本 compat oracle 无法定义；
5. 范围扩展到第二协议面。

本门特化（**取代** r0 的「沿 N1a r3 模式」引用--N1a 特化第 4 条把「越权能力（VPN/protect/特权）」列为违规，照抄会使本门开局即触发）：

- **S1** 构建失败：BoringTun checksum 漂移、`--offline --locked` 失败、feature 集偏离冻结值；
- **S2** C1 fail（arm64 加载失败）--测量事实，封签后停；
- **S3** C4 fail（握手不成立）--测量事实，封签后停；
- **S4** 白名单外 HDC/设备命令，或使用 `protect`、特权能力、外部 endpoint --立即停止并登记违规。**VpnExtensionAbility 与 `VpnConnection` 的使用是本门范围内行为，不构成越权**；
- **S5** 元组漂移/OTA/HDC 退化（有界短循环内不恢复即 blocked 停）；
- **S6** C3 前提 P1/P2 不成立；
- **S7** 判据无法按字面求值 → blocked + 返回 T0，不得现场放宽；
- **S8** 测量开始后判据不得修改。

## 授权与命令白名单

物理 campaign 须**用户显式授权**新 AUTH（pre-E8 native 例外，独立 AUTH/pair，沿 G0 13 门范式）。AUTH 登记时须冻结：目标元组、`code_sha`、源码 manifest、已签名 HAP 及其成员 hash、runner hash、外部输入 hash、**HDC 命令白名单逐条枚举**。白名单外任一条命令即触发 S4。真实 HDC target 只从仓外注入，不入库。

## 非范围（双向不外推）

本门不主张：x86_64/Emulator（N1a 已覆盖）、socket protect（N2）、本冻结 VpnConfig 未行使的 E4 面（多地址、多路由/路由表交互、DNS 声明、MTU 变更与重建--去向另行治理指派）、management/signal/relay/ICE（N3-N5）、产品实现、**性能/吞吐/长稳**、渠道、其他设备/build/API。N1a ↔ N1b 双向不外推。C1 的加载证据可供 N2b 引用为 §二.8 前置，但不外推为任何数据面或产品结论。

## 流程

1. 本判据文档（r1）交独立审查席确认后方可开始测量；审查须为跨厂商隔离席，且不得由主会话自身模型家族充任；
2. 实现（ArkTS VpnExtension + Rust 核心 + NAPI 薄层 + runner）经 host-only 验证与自测，并对照冻结判据逐条核对；
3. **物理 campaign 须用户显式授权**（新 AUTH，见上）；
4. 正式物理 campaign → 证据登记（双轴）→ 记录级独立审查 → 收口。
