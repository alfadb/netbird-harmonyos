# N2 TUN `decode_fail` × IPv6 控制帧 测前修订提案（供人审阅）

- 状态：**DRAFT / UNAPPROVED** —— 未批准、未冻结、非 AUTH、非门结论。用户批准前，本文件**不改变**任何既有规则，也不得被引用为规则文本。
- 对象（两处判据）：
  1. 冻结件 `docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.2）**:39** 第 2 条（本机 TUN 判据）中的「帧丢失/解码失败均为 `invalid/UNSAT`」，以及 `:44`/`:158` 的「生产外层 TUN 零命中 / 全 campaign TUN 负证据」口径。
  2. `docs/n2-log-formats-20260928.md` **:83**（`N2_TUN_TEE_STATS` 格式行）与 **:100-101**（完整性判据）。
- 基线（2026-10-01 现算，只读）：
  - 冻结件 sha256 `80a90cfdfeff37ad9dfa048e3e6e9df44db8b5d3dc8150d5005c189ec3a3e903`（90777 B；与 `docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256` 及准备清单 `:45` 的 `52b2bac` pin 一致）
  - 日志格式文档 sha256 `7812d56162f636b59ee8b24a1e71ed284f94a295aef00b447348af4352e0f8da`（45527 B）
  - 仓库 HEAD `a40ae845e3c4aea8e8edaeb4d89b27ecb9227e21`（2026-10-01T20:28:19+08:00）
- 写者边界：本提案**只读**代码/文档/真机汇总；**未改任何已有文件**（含冻结预注册、格式文档、治理、kit 判定器/执行器）、未提交、未跑设备命令/构建/抓包、未碰 kit 产出目录。本文件是新建的唯一文件。

---

## 1. 冲突陈述

### 1.1 已证实（三轮真机，逐字相同）

三轮真机 `N2_TUN_TEE_STATS` 的完整有效字段逐字相同：

```
N2_TUN_TEE_STATS|frames=5|decode_fail=5|dropped=0|mono_ms=…|diag=true|is_evidence=false|first_fail_len=76|first_fail_byte0=0x60|first_fail_prefix=60000000|fail_v4=0|fail_v6=5|fail_other=0
```

- 0049（2026-09-30）`:160-167`：两次采样、**全文件仅此 2 行**（相差 30.267 s，`mono_ms=1274242` / `1304551`）。
- 0050（2026-10-01）`:287`；0051（2026-10-01）`:268`；两者各自记「与上一轮逐字段相同」（0050 `:291`、0051 `:272`）。
- 0049 `:172-177` 的字段判定、`:180` 的口径结论；准备清单 `:131`（§3.8）与 `:209`（B1）把该条列为「最可能的整体 UNSAT 源 / 需人工决定」。

可逐条确证的观察（均有代码级原因）：

| # | 已证实 | 依据 |
| --- | --- | --- |
| 1 | 该区间 5 个 from-OS 帧**全部**被判为拒绝；`dropped=0` ⇒ 区间内**没有任何**解码成功的 IPv4 帧 | `frames`/`decode_fail`/`dropped` = 5/5/0；不变式 `5 = 5 + 0 + 0` 成立（0049 `:172`）|
| 2 | 5 帧的**首字节高半字节全为 6**（不只是首帧） | `fail_v6`/`fail_v4`/`fail_other` = 5/0/0；`DecodeFailWitness::record` 对**每一个**被拒帧按 `frame[0] >> 4` 分桶（`wg_device.rs:469-474`）|
| 3 | 首帧 76 B、前 4 字节 `60 00 00 00` ⇒ **偏移 0 处就是 plain IPv6 头**（排除 4 B `AF_INET` 前缀假说） | `first_fail_len=76`、`first_fail_byte0=0x60`、`first_fail_prefix=60000000`；0049 `:177` |
| 4 | 数量不增长：同区间末次采样仍是 5；三轮各自都是 5 | 0048 §3.1/§4.1（`仍是 5，未再增长`）、0050/0051 同 |
| 5 | 同区间冻结目的零命中、无正控、无泄漏 | `N2_TUN_FRAMES` 的 `seen=5`、`control=0`、`production=0`、`leaks=0`；`N2_TUN_DEST` 的 `dst=119.29.29.29`、`frames=0`、`delta=0`（0049 `:176`、0050 `:290`、0051 `:270`）|
| 6 | 帧型不是「4 B AF 前缀 / 偏移错位」 | 0048 §2.3：同平台其它轮次存在 `N6_WG_DEVICE` 的 `no-route` 行（`dst=…`），只可能在 offset 0 读到版本 4 后出现 |

### 1.2 为何被计为 `decode_fail`（机制，代码级）

1. `n2h::parse_ipv4_frame` 的拒绝条件全集：`frame.len() < 20`、`frame[0] >> 4 != 4`、`ihl < 20`、`frame.len() < ihl`（`client/core/src/n2h.rs:683-694`）。**版本半字节 ≠ 4 一律拒绝**，IPv6（`0x6x`）只是其中一类。
2. tee 侧：`N2DestinationTee::on_tun_frame` 先 `tee_frames += 1`，凡 `parse_ipv4_frame` 返回 `None` 即 `decode_fail += 1` 并 `return`（`client/core/src/wg_device.rs:710-722`）；解码成功但目的不在冻结集合才记 `dropped`（`:723-727`）。
3. IPv6 在本仓库是**显式声明的作用域外**：`n2h.rs:85`（`residual_scope` = 「IPv6: TUN 帧解析与端点枚举均为 IPv4-only」）、`:667-668`（`Ipv4FrameView` 模块注释「IPv4-only」）；单测已冻结「version 6 被拒」（`n2h.rs:2144-2149`）。
4. 即：**一个已被文档声明为「作用域外」的帧类，在计数口径上被当作「解码失败」处理**——这是本冲突的实质。0048 §2.4/§4.1 已把这几帧判为「良性（高置信）」但「帧型证据不足」，并明说不建议当作干净负扫描。

### 1.3 现行规则下的后果（授标阻断）

- `docs/n2-log-formats-20260928.md:100-101`（逐字）：`Exact invariant: frames = decode_fail + dropped + Σ N2_TUN_DEST.frames.` **`A non-zero decode_fail makes the interval incomplete → invalid/UNSAT.`**
- 冻结件 `:39`（逐字）：`无前/后控、窗口空白、帧丢失/解码失败均为 invalid/UNSAT，不能外推其间未知帧。`
- 冻结件 `:44`（逐字）：`整个 campaign 生产外层 TUN 零命中` 是正向格阈值的一部分；`:158` 的阶段标签定义要求`全 campaign TUN 负证据`；`:127` 把「tee 连续性」缺件列为 `invalid/UNSAT` 停格理由。
- 因此：**只要这 5 个内核 IPv6 控制帧存在，整个 campaign 可观测窗口的 TUN 区间就是 `invalid/UNSAT`**（该窗口定义为「从 VPN create 且 tee attach 到 destroy」，`:39`），24 个适用格（`:11`）的 TUN 负证据无一可用，`N2 第一阶段通过` 标签不可能授予。**这不是「有泄漏」，而是「证据被判为不可解释」**；但现行口径下 `UNSAT` 与「泄漏」在授标上是同一后果（`:11`：任一 `invalid/UNSAT` 阻止授予）。

### 1.4 推测 / 不可确证（必须与 1.1 分开）

- **帧型**：最可能是 IPv6 链路本地控制流量（内核在 vpn-tun 挂上后那一波 RS / DAD NS / MLD report，可能含 RA），与 0048 §2.4 的方向一致。**但日志只记了首帧的前 4 字节与长度**，5 帧的源/目的地址、next header、其余 4 帧的长度**一律没有**（0048 §2.1/§4.1「帧型证据不足」）。
- **长度只给出一个与之一致的候选帧型（推断，待新诊断字段证实）**：仓库既有合成夹具「plain IPv6 neighbor-solicitation」是 64 B（`n2h.rs:2183-2192`），实测首帧 76 B ≠ 64 B ⇒ 76 B 不是无选项的 NS。与 76 B 相符的自然分解是 **40 B IPv6 头 + 8 B Hop-by-Hop（Router Alert；`frame[6]=0`、HBH 长度字节 `frame[41]=0` ⇒ `(0+1)×8 = 8 B`，HBH 的下一头 `frame[40]=58`）+ 28 B ICMPv6 MLDv2 报告（1 条记录 = 4 B ICMPv6 头 + 4 B 保留/记录数 + 20 B 记录）**，与 `ff02::16` 的 MLDv2 报告形态相符。**这是「与长度相符的推断」，不是已证事实**：链尾协议、HBH 长度、目的地址均未被现有日志记录（§1.4 首条），须由 R1 新增的 `ipv6_next`（**链尾协议**直方图）与 `first_v6_*` 字段在新诊断轮证实。**不能由长度单独定帧型。**
- **爆发时段**（挂载后头 ~5 s）：0048 §3.3 的推论（窗口两缘 iface 计数相同、`rxBytes=0|txBytes=520`、采集器窗口 0 帧），**日志无逐帧时间戳**；`520 B ÷ 5 ≈ 104 B/帧` 及其方向语义均为推测（0048 §2.4/§3.3）。
- **平台是否在本 VPN（只声明 IPv4 地址/路由）起来后向 vpn-tun 安装 IPv6 路由**：**未验证**。这是「IPv6 帧能否成为真实泄漏载具」的关键前提，见 §2。

---

## 2. IPv6 是否可能构成泄漏路径（事实核查）

1. **冻结的证据配置不捕获 IPv6，也没有 IPv6 隧道地址**：`buildPlatformVpnConfig` 只构造**一个** LinkAddress，`family: 1`（IPv4）（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:547-550`、`:580-583`）。路由目的的 `family` **只由目的字符串是否含 `:` 决定**——`NetBirdVpnConfig.ets:557` 逐字为 `const destinationFamily: number = routeConfig.destination.indexOf(':') >= 0 ? 2 : 1;`，即**任何含 `:` 的目的都用 family 2，与它是否被排除（`isExcludedRoute`）无关**（该计算在排除标记 `:568-570` **之前**，排除只改 `isExcludedRoute` 语义、不改 family）。冻结证据配置的基准路由表全是 IPv4（`:86-119`），故实际落到 family 2 的只有 N2-H 的 v6 排除目的。证据代的路由 graft 也只加 IPv4 `/32` 非排除捕获路由（`NetBirdN2Evidence.ets:110-122`），且 **N2 证据模式的捕获 IP 在解析期被强约束为 IPv4**（`NetBirdN2Evidence.ets:45-49` 的点分四段正则加逐段 `Number(part) > 255` 检查，不合格即 `invalid-capture-ip`）。
2. **因此 App 的 IPv6 外层流量不会被这些路由捕获进 vpn-tun**：它要么走物理接口出去（tee 看不见），要么根本不存在 v6 候选。这是一个**结构性覆盖缺口**，且**已由冻结规则处理**：`候选 IPv6 无对应经预审捕获方案 … 停相应格 UNSAT`（`:15`）、`IPv6 无经预审捕获方案则 Wb/Ws 对应格 UNSAT`（`:68`）。**本修订不治愈、也不扩大这一缺口。**
3. **被观测到的那 5 帧确实在 vpn-tun 里**。在 `trustedApplications=[本 App]` 前提下（`:17`/`:39`），tun 内的帧名义上都是本 App 的；但**内核为接口自身产生的 IPv6 控制帧不是任何 App 路由查找的结果**，它不来自 App 会话，也不携带 App 生产载荷。5 帧在 attach 后一次出现、之后 30 s 不增长、且 `production=0|control=0|leaks=0`，与该解释一致——**但现有日志无法证明**（没记目的前缀/next header）。这正是本修订要求补记字段的理由：**让豁免变成有条件的、可核的，而不是凭推断的**。
4. **设备是否可能有 IPv6 物理路由**：蜂窝网常见 IPv6-only/双栈，家 Wi-Fi 也可能有；三轮冒烟**既未证实也未证伪**。但即使存在，该路径**绕过 vpn-tun**，tee 结构性看不到——它影响的是「是否有格必须 `UNSAT`」（`:15`/`:68`），不是「豁免会不会掩盖它」。**这一项应在授权后做一次只读发现**（见 §6）。
5. **本修订不放宽泄漏判据**：任何 IPv4 帧的判定**逐字不变**（落在冻结目的集合 → `fail/leak`；不在 → `dropped`，`:39`）；任何**非控制类**版本 6 帧**不在豁免内**（§4 R4），而是具名 `UNSAT`/风险，能归到已知外层目的时升为 `fail`。
6. **残余风险（如实登记，不掩盖）**：若平台确实（未验证）在本 VPN 下向 vpn-tun 安装了 IPv6 路由，且 App 生产流量打向某个**全局 v6 目的**，R4 会抓住它（目的不在 `fe80::/10`/`ff02::/16` → `ipv6_other` → `UNSAT`/风险）。R1 收紧后，会被登记类吸收的形态必须**同时**满足：目的 ∈ `fe80::/10` 或 `ff02::/16`（**仅链路范围组播**）**且** 协议链只含 HBH 且**链尾 = 58** **且** ICMPv6 类型 ∈ 白名单 **且** 跳数限制相符；只要有一条不符就落入 `ipv6_other`。而生产外层目的按构造是全局可路由端点，且**任何 v6 外层候选当前都没有经预审捕获方案（`:15`）**，即该形态本来就落在冻结规则的覆盖之外。此残余在 §4 R5 明写为「负扫描覆盖仍是 IPv4-only」。

---

## 3. 方案比较

### 方案 (a)（推荐）：把控制类 IPv6 帧单列 `ipv6_out_of_scope`，并逐区间登记数量与特征；非控制类 v6 不豁免

两种实现口径，**推荐 (a1)**：

- **(a1) 保持 `decode_fail` 语义与冻结不变式不变**：`decode_fail` 仍 = 「`parse_ipv4_frame` 拒绝的全部帧」；**新增** `ipv6_out_of_scope`（+ scope/next 直方图 + 首帧 v6 见证）；完整性条件由「`decode_fail == 0`」改为「**`decode_fail == ipv6_out_of_scope`**」。
- **(a2) 字面口径**：把控制类 v6 从 `decode_fail` 中移出（即「不计 `decode_fail`」），但那样不变式必须改成 `frames = decode_fail + ipv6_out_of_scope + dropped + Σ rows`，`decode_fail` 语义变更，`fail_v6`/`first_fail_*` 的既有含义也需重新定义——改动面更大、与格式文档 `:95-101`/`:107-108` 的「既有字段名与顺序不变」承诺冲突。

(a1) 与 (a2) 效果等价，但 (a1) 只**追加**字段，不触碰任何冻结字段语义与不变式。代价：约 1 个分类函数（≤60 行，含 HBH 链尾走查、ICMPv6 类型白名单与跳数限制）+ 1 个计数器/直方图 + 1 行日志尾部拼接 + 单测；判定器需解析新字段并强制 R2 的两条条件。

泄漏检测影响：IPv4 判据不变；v6 从「一律阻断」升级为「有区分地阻断」——控制类登记，**非控制类照旧 `UNSAT`/风险，可归属时 `fail`**。不掩盖真实泄漏。

### 方案 (b)：规则层面接受 attach 后固定时窗内的 IPv6 控制帧为已知豁免

- 优点：不改解析器。
- 缺点（决定性问题）：**没有解析就无法认定「控制帧」**，豁免只能写成「窗口内任何被拒帧」，比 (a) 更宽；且「窗口」需要逐次拒绝的时间戳与计数，而当前只记**首帧**的 len/byte0（`wg_state` 见证结构 `wg_device.rs:452-497`），所以 (b) 同样要改代码（至少 `first_fail_mono_ms`/`last_fail_mono_ms` + 窗口内计数），却**不获得目的前缀/next header 这一判别力**；窗口边界也天然可被「窗口内的真实泄漏」利用。**投入相近、证据力更低、放宽更大**，不作推荐。

### 方案 (a1+b)（叠加备选，不推荐）：解析 + 挂载后时窗/帧数上限

- 口径：在 (a1) 的控制类解析之上**再叠加两道闸**——只有**挂载后 N 秒内**且**前 ≤K 帧**内的控制类 v6 帧才记 `ipv6_out_of_scope`；超出时窗或超出 K 帧的控制类帧一律记 **`ipv6_other`**（即回落到 R4 的 `UNSAT` + `:127` 全轮停测）。N/K 须在批准时定值并写入登记节。实现代价高于 (a1)：除控制类解析外还需记录挂载时刻与首个控制类帧的单调时间戳、以及自挂载起的帧计数（现有见证只记首帧 len/byte0，`wg_device.rs:452-497`，同方案 (b) 的缺口）。
- 优点：比 (a1) 更严（额外的数量与时序上限），对「窗口内被真实泄漏利用」这一形态**多一层钝保护**；能覆盖 §1.4 推测的「挂载后头 ~5 s 一波」形态。
- 缺点（取舍，决定性问题）：**会误判晚到或周期性的合法控制帧**——内核在接口存续期内会周期性发 MLD 报告、并对路由/地址变化重发 RS/NS；这些帧在时窗之后出现，会被计成 `ipv6_other` 从而把**本来干净**的区间打成 `UNSAT`（假阳性停测）。反过来，N/K 定得宽到能容纳周期性帧，就基本退化成 (a1) 而失去这层保护。即：**该叠加把「误判风险」从漏报侧搬到误报侧，而当前证据（§1.1 五帧、一次性、30 s 不增长）不支持对周期行为作任何定值假设**。
- 结论：**登记为备选，不改推荐**。推荐仍是 **(a1)**；若用户在 §9 选择 (a1+b)，N/K 必须先经一轮诊断观测（`ipv6_*` 直方图 + 逐帧时间戳）再定值，不得凭 §1.4 的推测拍定。

### 方案 (c)：维持现状，接受 `UNSAT`

- 优点：零规则变更、零代码变更；保留「任何不可解释帧即停」的钝保护；不产生任何「TUN 干净」的过度声明。
- 缺点：24 个适用格的 TUN 负证据**结构性不可得**，第一批无法授予标签（`:11`）；且该 `UNSAT` 与「泄漏」在授标后果上无法区分（`:11`/`:127`）。它是**诚实但不前进**的选项。
- 注意：`UNSAT` 本身**不是** pass，也**不能**抵消实质 fail（沿用 v4.2 的「`fail` 不得被 `UNSAT` 掩盖」纪律）；若选 (c)，应同时把「改代码或改判定口径以消除该 UNSAT」写成第二批/后续前置项，而不是把它记成可接受终态。

### 推荐：**(a1)**

理由：① 把「已声明为作用域外」的帧类从「不可解释的失败」变成「有边界的、逐区间可核的登记类」，正是现行文档自身逻辑（`n2h.rs:85`）的延伸；② 追加字段、**冻结不变式与全部既有字段语义不变**，改动可被主机单测和 diff 穷尽验收；③ 对非控制类 v6 保留 `UNSAT`/风险/可归属 `fail`，**不把任何可能的真实泄漏做成豁免**；④ 不需要动阈值、配置冻结、oracle、适用集与任何 AUTH 前置项。

---

## 4. 【需批准】精确拟改条款

**批准对象 = R1–R5 五条文本修订 + F1 一条禁令。** 除 R1/R2 的窄例外外无任何放宽；阈值、配置冻结、适用集、oracle、fail 条件均不在批准对象内。

### R1 —— `docs/n2-log-formats-20260928.md:83` 格式行（追加尾字段）

- 原文（逐字）：`N2_TUN_TEE_STATS|frames=…|decode_fail=…|dropped=…|mono_ms=…|diag=true|is_evidence=false|first_fail_len=…|first_fail_byte0=0x…|first_fail_prefix=…|fail_v4=…|fail_v6=…|fail_other=…`
- 拟新文（待批准）：在 `fail_other=…` 之后**追加**以下尾字段（既有字段名与顺序一个字不动）：

```
|ipv6_out_of_scope=…|ipv6_other=…|ipv6_scope=linklocal:…,multicast:…,other:…|ipv6_next=icmp6:…,hbh:…,udp:…,tcp:…,other:…|first_v6_len=…|first_v6_dst_prefix=…|first_v6_other_len=…|first_v6_other_dst=…|first_v6_other_next=…|first_v6_other_dport=…
```

- 字段口径（追加到 `:102-108` 的 diag-0048 尾字段说明之后）：
  - `ipv6_out_of_scope`：被判为**控制类 IPv6** 的 from-OS 帧数。控制类定义（四条**同时**成立）：
    1. 版本半字节 6 且 `len ≥ 40`；
    2. 目的地址（`frame[24..40]`）∈ **link-local `fe80::/10`** ∪ **组播 `ff02::/16`（仅链路范围组播）**。`ff00::/8` 的其余组播（如 `ff05::`/`ff0e::`）、`::1`、`::` **一律不算控制类**；`fe80::/10` 保留；
    3. **协议链（不能只看 `frame[6]`）**：以 `next_header = frame[6]` 为链首，
       - 链首 `== 0`（Hop-by-Hop）⇒ 取 HBH 长度字节 `frame[41] = n`，HBH 总长 `(n+1)×8`，**链尾协议 = `frame[40]`（HBH 自身的 next header）**，且**链尾必须 `== 58`（ICMPv6）**；
       - 链首 `== 58` ⇒ 链尾 = 58；
       - **其他扩展头（43 Routing / 44 Fragment / 60 Destination Options 等）或链尾 ≠ 58 ⇒ 非控制类**（`ipv6_other`）。越界（HBH 长度越过帧尾）⇒ 非控制类；
    4. ICMPv6 类型白名单与跳数限制：链尾为 58 时，令 ICMPv6 起始偏移 `t = (链首 == 0) ? 40 + (n+1)×8 : 40`，要求 `frame[t]`（ICMPv6 type）**∈ {131 MLD report, 132 MLD done, 133 RS, 135 NS, 136 NA, 143 MLDv2 report}**，且跳数限制 `frame[7]` 满足：**133/135/136（RS/ND）= 255**、**131/132/143（MLD）= 1**。**echo request/reply（128/129）及任何其他类型、或跳数限制不符 ⇒ 非控制类**（`ipv6_other`）。
    - 实测推测的三个帧型都落在新范围内：**RS → `ff02::2`**、**DAD NS → `ff02::1:ffxx:xxxx`**、**MLDv2 报告 → `ff02::16`**（三者目的均在 `ff02::/16` 内，符合第 2 条）。
  - `ipv6_other`：版本 6 但**不满足**上述控制类定义的被拒帧数（短帧；目的不在 `fe80::/10`/`ff02::/16`，含其余组播、回环、未指定、全局；扩展链非 HBH 或**链尾 ≠ 58**；ICMPv6 类型不在白名单；跳数限制不符）。恒有 **`fail_v6 == ipv6_out_of_scope + ipv6_other`**。
  - `ipv6_scope` / `ipv6_next`：对**全部**版本 6 被拒帧的目的 scope 与 **next header 链尾协议**直方图——`ipv6_scope` 只分 `linklocal`(`fe80::/10`)/`multicast`(`ff02::/16`)/`other`（含其余组播、回环、未指定、全局）；`ipv6_next` 记**链尾协议**（`icmp6`=链尾 58、`hbh`=链尾仍为 0（连续 HBH）或其他扩展头/越界而未走通、`udp`=17、`tcp`=6、`other`）。用于证明「全部 5 帧都是链路本地或链路范围组播、且链尾是 ICMPv6」，而不是只信首帧。
  - `first_v6_len` / `first_v6_dst_prefix`：**首个**版本 6 被拒帧的长度与目的地址**最多前 8 字节**小写 hex（空值语义同既有 `first_fail_*`：首帧出现前为空）。**只记头部元数据，永不记载荷**（沿用 `:107`）。
  - `first_v6_other_*`（**R4 可观测性，只为非控制类帧**）：`first_v6_other_len` = 首个 `ipv6_other` 帧长度；`first_v6_other_dst` = 该帧目的地址**完整 16 字节**小写 hex（32 字符，**不省略**）；`first_v6_other_next` = 该帧**链尾协议号**（十进制）；`first_v6_other_dport` = 链尾为 UDP(17)/TCP(6) 时的目的端口（十进制），否则 `-`。**只记头部，不记载荷**；无 `ipv6_other` 帧时四个字段同既有空值语义。这些字段是 R4 默认 `UNSAT` 的**可核见证**，本身不是归属结论。
  - `decode_fail` 仍 = 「`parse_ipv4_frame` 拒绝的全部帧」；`:100` 的不变式 `frames = decode_fail + dropped + Σ N2_TUN_DEST.frames` **不变**；`fail_v4`/`fail_v6`/`fail_other`/`first_fail_*` 语义**不变**。

### R2 —— `docs/n2-log-formats-20260928.md:100-101` 完整性判据（加窄例外）

- 原文（逐字）：`Exact invariant: frames = decode_fail + dropped + Σ N2_TUN_DEST.frames. **A non-zero decode_fail makes the interval incomplete → invalid/UNSAT.**`
- 拟新文（待批准）：不变式原样保留；其后改为——

> **A non-zero `decode_fail` makes the interval incomplete → `invalid/UNSAT`**，**唯一例外**：当被拒帧**全部**由控制类 IPv6 解释时。区间完整的充分必要条件见下两条，**缺一即不完整**。
>
> **（i）逐行自洽对账**：**每一条** `N2_TUN_TEE_STATS` 行（计数均为**自挂载起累计**）都必须满足四个和两两相等——
>
> - `Σ ipv6_scope == Σ ipv6_next`
> - `== fail_v6`
> - `== ipv6_out_of_scope + ipv6_other`
>
> 即：`Σipv6_scope == Σipv6_next == fail_v6 == ipv6_out_of_scope + ipv6_other`。任何一个等号不成立 ⇒ 该区间 **`invalid/UNSAT`**。
>
> **（ii）完整性条件**：**`decode_fail == ipv6_out_of_scope`**（等价于 `fail_v4 + fail_other + ipv6_other == 0`）。
>
> **字段缺失（本修订之前的构建）或任一对账不成立即回到原判据**（`decode_fail ≠ 0 ⇒ invalid/UNSAT`，fail-closed）。`ipv6_out_of_scope` 帧只被**登记**：既不计为生产命中、也不构成覆盖或 pass；负扫描覆盖仍是 IPv4-only（`n2h.rs:85`）。

### R3 —— 冻结件 `:39` 的窄例外

- 原文（逐字，被 R3 限定）：`无前/后控、窗口空白、帧丢失/解码失败均为 invalid/UNSAT，不能外推其间未知帧。`
- 拟新文（待批准）：`无前/后控、窗口空白、帧丢失/解码失败均为 invalid/UNSAT，不能外推其间未知帧（**唯一下限例外**：被 N2_TUN_TEE_STATS 的 ipv6_out_of_scope 逐区间登记、且满足 n2-log-formats §3 的 `decode_fail == ipv6_out_of_scope` 完整性条件**与 R2(i) 的逐行四和自洽对账**的**控制类 IPv6 帧**，不单独使区间不完整；字段缺失或任一对账不符即回到原判据）。`
- 说明：R3 只限定「解码失败」这一子句，`无前/后控`、`窗口空白`、`不能外推其间未知帧`一字不动。

### R4 —— 非控制类 v6 的分级、可观测性与可归属情形（不放宽，具名化）

- **`ipv6_other > 0`**（版本 6 但非控制类：短帧；目的不在 `fe80::/10`/`ff02::/16`，含其余组播、回环、未指定、全局；扩展链非 HBH 或**链尾 ≠ 58**；ICMPv6 类型不在白名单；跳数限制不符）⇒ 该区间 `invalid/UNSAT`，**不得**算作 R2 例外；同时记具名风险事件 `tun_ipv6_other_nonzero`。
- **可观测性（本条验收的前提）**：任一非控制类帧都必须留下 R1 的 `first_v6_other_{len,dst,next,dport}` 见证——长度为帧长、目的为**完整 16 字节**（不省略）、`next` 为**链尾协议号**、`dport` 仅在 UDP/TCP 时给出；**只记头部，不记载荷**。缺任一见证 ⇒ 该区间 **`invalid/UNSAT`**（无法核）。
- **无法归属即默认 `UNSAT`**：当该帧目的**不能归属到任何冻结/已知外层目的**时，默认结论是 **`UNSAT`**（fail-closed），**不得**因「无法归属」而改判为干净或豁免；`first_v6_other_dst` 是这条默认结论的可核见证，而非归属证据。
- **按冻结件 `:127` 全轮停测**（该行逐字：「泄漏风险、未知外层能力或代际失效可疑则扩大为全轮停测并交 T0/用户」）：`ipv6_other > 0` 即构成「未知外层能力可疑」，**先按 `:127` 扩大为全轮停测**并交 T0/用户，不是只停该格。
- 若此类帧的目的**可归到任何冻结/已知外层目的**（例如冻结的 v6 端点、设备 overlay 前缀）⇒ 按 `:127`/`:39` 升级为该格 **`fail`**；**`fail` 优先，不得被 `UNSAT` 掩盖**（沿用 v4.2 同纪律）。
- 任一 IPv4 帧、overlay 本地 IP、未保护首发等既有 fail 条件**逐字不变**。
- 判定器与执行器必须同步（属实现符合性，须独立审查）：解析并强制 R1/R2 字段（`/home/worker/n2h-kit/n2-campaign/judge.py:1700-1710,1935-1956`），`decode_fail > 0 且 decode_fail != ipv6_out_of_scope` → 原 problem 码 `tun_decode_fail_nonzero`；新字段缺失（含 `ipv6_other > 0` 而 `first_v6_other_*` 不齐） → `tun_tee_stats_fields_missing`（视同旧件）；**R2(i) 四和对账不成立** → 新增 problem 码 `tun_tee_stats_inconsistent`；`ipv6_other > 0` → `tun_ipv6_other_nonzero`。

### R5 —— 覆盖范围口径附注（只作登记注记，不改 `:44`/`:158` 文字）

- `:44` 的`整个 campaign 生产外层 TUN 零命中`与 `:158` 标签定义里的`全 campaign TUN 负证据`，在本修订后按如下口径解释：**相对冻结的 IPv4 外层目的集合的零命中** ＋ **控制类 IPv6 帧的逐区间登记**（不计为命中，也不计为覆盖）。负扫描覆盖范围仍是 IPv4-only，**不因本修订扩展**。
- `:15`/`:68` 的`候选 IPv6 无对应经预审捕获方案 ⇒ 相应格 UNSAT`**不变**，本修订不缩减该项。
- 治理文档文末标签定义文字**不变**；本口径**在治理文档文末同步追加注记节**登记（同 v4.1 先例 `:184`/`:187`）——该注记须与冻结件 v4.3 登记节**同版、同步追加**，**只追加不改写**，并与 `§7 第 2 步`、`§5 第 4 行` 的登记动作同批完成。
- 追加节还必须写明：**豁免帧不构成泄漏路径的证明**，只构成「该帧类不在冻结负扫描的判据范围内」；任何「无 v6 泄漏」的更强表述都超出本修订。

### F1 —— 禁令（与 R1–R5 同时生效）

- **禁止**把任何 IPv4 命中、任何非控制类 v6 帧、或任何未登记/对账不符的帧记成 `ipv6_out_of_scope`；特别地，**链尾 ≠ 58**、**ICMPv6 类型不在白名单**（含 echo 128/129）、**跳数限制不符**、或目的不在 `fe80::/10`/`ff02::/16` 的帧，一律不得记成 `ipv6_out_of_scope`；
- **禁止**测后补写/回填 `ipv6_*` / `first_v6_*` 字段（含 `first_v6_other_*`），或把旧件（缺字段的构建）的 `decode_fail` 事后重算为豁免；
- **禁止**用本修订把任一 `fail` 降级为 `UNSAT` 或豁免；
- **禁止**以「让 IPv6 控制帧不再出现」为由人为改动 tee 挂载/窗口时序或伪造干净区间（该行为同时触发 `:127` 停测）。

### 不改变的内容（逐条）

- `:3`「变更须测前回 T0 独立审查、取得用户重新批准并登记新冻结版本，不得测后追认」的程序；
- §3 阈值（3 次独立事件、三证齐全、正控 3/3、`±2000 ms`）与 `:39` 的夹心正控、`:44` 的 fail 条件；
- §2 配置冻结（`trustedApplications=[本 App]`、第 0/1 代 `/32` 并集、无 exit node、`defaultRoutes=0`、端点排除关闭）与 `:15` 的 hdc/peer 重合例外；
- 适用集 24 格 + Wb×A–D 4 个豁免格（`:11`/`:186`）与治理标签定义文字；
- `:15`/`:68` 的 IPv6 候选无预审捕获方案 ⇒ `UNSAT`；`:68` 的抓包过滤器与两个抓包点；
- `decode_fail`/`dropped`/`fail_v4`/`fail_v6`/`fail_other`/`first_fail_*` 的既有语义、`:100` 的冻结不变式、`N2_TUN_DEST`/`N2_TUN_FRAMES`/`N2_TUN_GAP` 行格式；
- `n2h::parse_ipv4_frame` 的判据与其单测（`n2h.rs:683-694`、`:2144-2149`）。

---

## 5. 最小代码 / 日志 / 工具改动清单

| # | 位置 | 改动（有界） | 不改 |
| --- | --- | --- | --- |
| 1 | `client/core/src/n2h.rs` | 新增 `ipv6_header_view`（`len ≥ 40`、version 6、`next_header = frame[6]`、hop limit `frame[7]`、目的 `frame[24..40]`，越界不 panic）、**协议链走查**（链首 `0` ⇒ HBH 长度 `frame[41]`、链尾 = `frame[40]`；链首 `58` ⇒ 链尾 = 58；其他扩展头/越界 ⇒ 链尾不可用）与 `ipv6_control_class`（目的只认 `fe80::/10`/`ff02::/16`；链尾必须 58；ICMPv6 type ∈ {131,132,133,135,136,143}；hop limit 133/135/136=255、131/132/143=1）；同时暴露**链尾协议号**供直方图与 `first_v6_other_*`。新增单测：link-local NS（复用 `:2183-2192` 夹具，但补足到 40 B 头、hop limit 255）→ 控制类；`ff02::16` + HBH(`frame[41]=0`, 链尾 58) + MLDv2 type 143 + hop limit 1 → 控制类（即 76 B 首帧形态）；`ff02::` + HBH 但**链尾 = 17 UDP** → 非控制类；ICMPv6 **echo(128)** → 非控制类；hop limit 不符（如 NS hop limit 64）→ 非控制类；`ff05::`/`::1`/`::` 目的 → 非控制类；v6 UDP 到 `2001:db8::1` → 非控制类；`len<40` → 非控制类；v4 帧结果不变 | `parse_ipv4_frame` 与其拒绝条件、`:85` 的 residual_scope 声明 |
| 2 | `client/core/src/wg_device.rs` | `N2DestinationState`（`:431-450`）加 `ipv6_out_of_scope` / `ipv6_other` / scope 直方图（`linklocal`/`multicast`/`other`）/ **链尾协议**直方图 / 首个 v6 见证 / **首个非控制类见证 `first_v6_other_{len,dst,next,dport}`（dst 完整 16 字节）**；`on_tun_frame`（`:710-722`）**在拒绝分支内**追加分类（`decode_fail += 1` 与 `first_fail.record` 照旧，顺序不变）；`stats_line`（`:566-598`）在 `fail_other` 后**追加**尾字段；同文件单测（`:3047` 旁）新增「5 帧全 v6 控制类 → `decode_fail=5`、`ipv6_out_of_scope=5`、`ipv6_other=0`，`5 = 5+0+0` 成立」与「1 帧 v6 全局目的 → `ipv6_other=1` 且 `first_v6_other_dst` 为完整 16 字节」 | `decode_fail` 计数、不变式、`N2_TUN_DEST` 归属逻辑、`:1259-1299` 的 `no-route` 路径 |
| 3 | `docs/n2-log-formats-20260928.md` | 按 R1（控制类四条件、`ipv6_scope`/`ipv6_next` 链尾直方图、`first_v6_other_{len,dst,next,dport}`）与 R2（(i) 逐行四和自洽、(ii) 完整性条件）追加字段说明与完整性条件（追加，不改既有字段语义） | `:83` 既有字段、`:100` 不变式、`:107` 的「只记头部、不记载荷」 |
| 4 | `docs/n2-device-evidence-preregistration-draft-v2-20260928.md` | 按 R3 限定 `:39` 的子句（或在文末追加节登记该例外并指明取代哪一子句）；按 R5 追加口径注记。**登记只追加不改写**（v4.3 节 + 独立 sidecar hash + **治理文档文末同步追加注记节**，格式沿用 `:226-296` 先例） | `:3`/`:11`/`:15`/`:44`/`:68`/`:127`/`:158` 的其余文字、§2/§3 全部阈值 |
| 5 | kit `/home/worker/n2h-kit/n2-campaign/judge.py` | `tun_tee_stats`（`:1700-1710`）解析新字段；`tun_continuity_problems`（`:1935-1956`）实施 R2 的**(i) 逐行四和自洽对账**与 **(ii) `decode_fail == ipv6_out_of_scope`**，及 R4 分级（含 `first_v6_other_*` 见证缺失 ⇒ `UNSAT`）；docstring `:35`/`:88`/`:1940` 同步 | 其余 problem 码与 fail 条件 |
| 6 | kit `/home/worker/n2h-kit/n2-campaign/run_campaign.py` / `selftest_judge.py` | 故障行 `:4005-4011` 与字段白名单 `selftest_judge.py:179-181,1442` 同步；新增负例（详见 §6） | 执行器时序、AUTH 门、其余自检 |

---

## 6. 测试与验收条件

**离线（主机，无设备、无 hdc/ssh）**

- `cargo test -p netbird_core` 相关单测全绿，且新增的 v6 分类用例（§5 第 1 行）逐一断言，含 HBH 链尾走查、ICMPv6 类型白名单、跳数限制、`fe80::/10`/`ff02::/16` 目的收紧与「其余组播/回环/未指定 → 非控制类」。
- 合成夹具断言同时成立：`frames=5|decode_fail=5|dropped=0|fail_v4=0|fail_v6=5|fail_other=0|ipv6_out_of_scope=5|ipv6_other=0`，`5 = 5+0+0`，且 R2(i) 四和自洽 `Σipv6_scope == Σipv6_next == fail_v6 == 5`（76 B 形态首帧 = HBH 链尾 58、type 143、hop limit 1 ⇒ 计入 `ipv6_next` 的 `icmp6` 与 `ipv6_scope` 的 `multicast`）。
- kit 自检全绿（`selftest_judge.py`、`selftest_e2e.py`），并新增以下用例：
  1. `decode_fail=5|ipv6_out_of_scope=5|ipv6_other=0` 且四和自洽 → **不再**报 `tun_decode_fail_nonzero`；
  2. `ipv6_other=1`（如 v6 UDP 到全局目的）→ `tun_ipv6_other_nonzero`，该格 `UNSAT`（若目的可归到已知外层 → `fail`）；
  3. 缺 `ipv6_out_of_scope` 字段的旧格式行 → 回到原 `tun_decode_fail_nonzero`（fail-closed）；
  4. **链尾 ≠ 58**（`ff02::16` + HBH，但 HBH 的下一头 = 17 UDP）→ `ipv6_other`，该格 `UNSAT`；
  5. **类型不在白名单**（`ff02::1:ffxx:xxxx` 的 NS 形态但 ICMPv6 type = 128 echo request）→ `ipv6_other`；**跳数限制不符**（NS hop limit = 64）→ `ipv6_other`；
  6. **四和不符**（如 `Σipv6_scope ≠ fail_v6`）→ 新增 problem 码 `tun_tee_stats_inconsistent`，该格 `UNSAT`；`ipv6_other > 0` 而 `first_v6_other_*` 缺失 → 同样 `UNSAT`。
- 单点 diff 复核：`n2h.rs` 的 IPv4 判据与 `:100` 不变式在两份文档中均未被隐式改写。

**真机（必须在设备上观察到，才算本修订可用）**

- 新一轮 `N2_TUN_TEE_STATS`（诊断轮即可用于实现核验；**用于门判定的必须是批准并登记之后的新窗口**）出现：`ipv6_out_of_scope>0`、**`decode_fail == ipv6_out_of_scope`**、`ipv6_other=0`、`ipv6_scope` 只落在 `linklocal`/`multicast` 且覆盖全部 v6 帧、`ipv6_next`（**链尾协议**）只落在 `icmp6`（58；经 HBH 时链尾仍须是 58，故 `hbh`/`udp`/`tcp`/`other` 不应出现）；四和自洽 `Σipv6_scope == Σipv6_next == fail_v6 == ipv6_out_of_scope + ipv6_other`；不变式 `frames = decode_fail + dropped + Σ N2_TUN_DEST.frames` 逐字成立。
- `first_v6_len`/`first_v6_dst_prefix` 与既有 `first_fail_len=76|first_fail_byte0=0x60|first_fail_prefix=60000000` 相容（同为首个被拒帧，语义未变）。若 §1.4 的 76 B 推断成立，则 `first_v6_len=76`、`first_v6_dst_prefix` 落在 `ff02::` 且链尾 58、ICMPv6 type 143、hop limit 1——**这是对该推断的证实点，不是判据前提**；实测不符即按 `ipv6_other` 处置。
- `first_v6_other_*` 在 `ipv6_other=0` 时为空；一旦 `ipv6_other>0`，四字段（`len`/`dst` 完整 16 字节/`next` 链尾/`dport`）必须齐全，缺一即 `UNSAT`。
- 出现 `ipv6_other>0` 时：**按冻结件 `:127` 全轮停测** + 具名风险事件 + 回 T0；目的无法归属任何冻结/已知外层目的时默认 `UNSAT`；**不得**继续按本修订判该格干净。
- **门判定窗口之前**，另做一次**只读**的「设备是否存在 IPv6 物理路由 / v6 外层候选」发现（决定是否有格必须 `UNSAT`，见 `:15`/`:68`）：在设备上只读记录 vpn-tun 的 `ip -6 addr`、`ip -6 route`，并覆盖**各路由表的 v6 默认路由**（`ip -6 rule` 与 `ip -6 route show table all`）；未做此项前，任何「v6 覆盖」表述都不成立。
  - **这些是后续授权中的设备只读命令，需另行纳入 AUTH，不在本提案授权内**：本提案不授权任何设备命令（见 §8）；上述 `ip -6 …` 只有在相应 AUTH 中明文列明、并经用户回读确认后才可执行，输出作为该轮只读证据一并归档。
- 正控 3/3、夹心前后控、`N2_TUN_DEST` 零命中、物理侧/独立网络侧 oracle 等其余验收条件**照旧**，不被本修订替代或放宽。

---

## 7. 批准后的三步（互相独立，不得合并）

1. **实施 + 主机测试 + 独立 T0 符合性审查**：按 §5 落地（`cargo test`、kit 自检），由**未参与本修订设计与实现**的独立 T0（跨厂商 family，干净上下文）逐条核对 R1–R5/F1 与代码/判定器的一一对应、`fail` 未被降级、无隐藏第三路径。
2. **新冻结登记**：在冻结件文末**只追加**一节（建议 v4.3），登记获批 R1–R5 + F1 原文、旧件 sha256（`80a90cfd…a3e903`）与新件整体 sha256 写入独立 sidecar 与治理同版登记，并**在治理文档文末同步追加 R5 的口径注记节**（只追加不改写）；**不把新件自身 hash 塞进新件自身**（沿用 `:294`）。本提案文件本身**不自称冻结**，其状态行保持 `DRAFT / UNAPPROVED`（沿用 `:230` 先例：提案不被改写一个字节）。
3. **AUTH**：两批 AUTH 仍分别签发与回读确认；本修订**不放宽任何 AUTH 前置项**（P4–P8、端点排除开关、抓包授权/`setcap`、两抓包点、`-s 96`、时钟校准等），也不代替 T7a。

---

## 8. 本提案不做的事 / 非 AUTH 声明

- **批准本提案不等于批准上机、不等于签发 AUTH、不等于授予任何标签或门结论。** 本提案**不**授权任何设备操作（无 hdc/ssh/抓包/重启/安装）、**不**授权管理端或服务器改动、**不**授权实施（实施须待批准后的第 1 步另行动工）。
- 不追认 0048–0051 或任何 `is_evidence=false` 诊断记录为证据；不得用它们在批准后充作「符合新条款」的观测行。
- 不改冻结预注册、格式文档、治理、kit 判定器/执行器/模板；不提交；不碰 kit 产出目录。
- 不声称「TUN 干净」，也不声称「无 IPv6 泄漏」；本修订只把一类已知的、作用域外的帧从「不可解释的失败」改判为「有条件、可核、逐区间登记的豁免项」，其余判据一律不动。

---

## 9. 需用户决定的事项

1. **是否批准本修订**：**(a1) 推荐**（追加字段 + `decode_fail == ipv6_out_of_scope`，冻结不变式不变）／**(a1+b) 叠加挂载后 N 秒/≤K 帧上限**（更严，但会把晚到或周期性的合法 MLD/RS 误判为 `ipv6_other` 而停测；N/K 需先经诊断轮定值，见 §3）／(a2) 字面「控制类 v6 不计 `decode_fail`」（须改不变式与判定器语义）／(b) attach 后固定时窗豁免（更宽、证据力更低、同样要改代码）／(c) 维持现状接受 `UNSAT`（第一批标签不可授予）。
2. **豁免判据的边界**：是否接受收紧后的控制类定义——目的 ∈ {`fe80::/10`, 组播 `ff02::/16`（仅链路范围）} **且** 协议链只含 HBH（链尾须 = 58）或链首即 58 **且** ICMPv6 type ∈ {131,132,133,135,136,143} **且** 跳数限制（133/135/136=255、131/132/143=1）；`ff00::/8` 的其余组播、`::1`、`::` 已明确排除；是否还要更严（例如只认 143 MLDv2 与 133 RS、连 ND 的 135/136 也不豁免）。
3. **非控制类 v6 的默认结论**：默认「无法归属外层目的即 `UNSAT`（fail-closed）+ 具名风险事件 + 按 `:127` 全轮停测，目的可归属时升 `fail`」是否符合预期；或要求一律先 `fail`。
4. **是否接受 R5 的残余**：负扫描覆盖仍是 IPv4-only；IPv6 外层候选无预审捕获方案 ⇒ 相关格 `UNSAT`（`:15`/`:68` 不变）。若要求 IPv6 也被负扫描覆盖，那是**另一个**更大的预注册修订（需先有经预审的 v6 捕获路由与枚举方案），不在本提案范围。
5. **是否先做一次只读的 IPv6 物理路由/v6 候选发现**（决定是否有格必须 `UNSAT`）：§6 已写明在**门判定窗口之前**于设备上只读记录 `ip -6 addr`/`ip -6 route`（含 `ip -6 rule` 与 `ip -6 route show table all` 的各表 v6 默认路由）——这是**需另行纳入 AUTH 的设备只读命令，不在本提案授权内**；请确认它落在哪一批授权内。
6. **实施与登记的责任分工**：由谁按 §5 落地、谁做独立 T0 符合性审查（跨厂商 family）、登记节用 v4.3 还是 v4.1.1、**治理文档文末的同步追加注记**（R5）由谁落笔，以及何时重新 pin kit（`/home/worker/n2h-kit/n2-campaign` 的文档 pin 以 `sha256(git show <commit>:<path>)` 为准，工作区文件不可冒充）。
