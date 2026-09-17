# N2-H 设备侧证据采集器（device-side evidence collector）— 设计文档（DRAFT，供审查）

- **文件性质：DRAFT / 设计文档，未实现、未生效**。本文是 T0 审查（2026-09-17）认定的 N2-H 预注册生效前置 blocker **B-1**（「主机侧 nbinterop/n2h 无法在设备上产出证据，必须先有设备侧采集器落盘，判据才能逐字段对齐」）的设计应答，供人类与独立审查席复审。
- **基线**：仓库 HEAD `7923288`（实测 `git rev-parse HEAD`）。本文所有 file:line 引用均在该 HEAD 实读取证。
- **写作过程声明**：只读核对仓库代码与文档；未修改任何既有文件；未运行构建/测试；未执行任何设备命令。本文获审查通过后，实现按 §7 增量切分另行派发。
- **配套文档**：判据与停止条件见预注册草案 `docs/n2h-device-evidence-preregistration-draft-20260917.md`（§2 C1–C9、§4 S1–S14；下称「预注册草案」）；主机侧参照实现说明 `docs/n2h-isolation-evidence-notes.md`；真机操作面 `docs/device-relay-enablement-runbook-20260914.md`（下称「runbook」）。

## 0. T0 必改项 → 本文档章节映射

| T0 必改项 | 要求 | 本文档落点 |
|---|---|---|
| B-1 | 指名采集器二进制/构建目标/命令/输出路径/字段 schema 对齐 | §2（构建目标与命令面）、§3（逐字段 schema 对齐表） |
| B-5 | `scanned_sides` 字面枚举（如 `["vpn-tun"]`） | §3 差异 D3、§4.1 |
| B-6 | C7 设备侧两口径：status JSON `rx_bytes_to_tun` ↔ `/proc/net/dev` vpn-tun RX | §4.4 |
| E-1 | AUTH allowed 逐字命令面 | §6.4 |
| A-1 | 目标元组冻结：采集器须输出构建/版本/哈希自标识 | §3.3 |

---

## 1. 目标与非目标

### 1.1 目标

在目标物理设备的**一个真实连接会话内**，由**运行在 App 进程内的 core（`libnetbird_core.so`）**产出与主机侧 `nbinterop isolation-check`（`client/core/src/n2h.rs`）**同 schema** 的证据 JSON，覆盖预注册草案 §2 的判据供证需求：

- C1（冻结完整）：`frozen_endpoints[]` 覆盖 7 类，来源可追溯到壳侧排除集与 core 会话知识；
- C4（TUN 包级负证据）：`tun_negative{hit,frames_scanned,scanned_sides}`；
- C5（隧道正控）：`tunnel_positive_control.observed_in_tunnel`（a2b/b2a 各至少一条）；
- C6（端点侧投递）：`endpoint_side[]`（能闭合的闭合，不能闭合的如实记缺口）；
- C7（计数对账）：`counters[]` 双口径恒等式（B-6）；
- C8（verdict）：由 n2h.rs 判定引擎同一实现产出；
- C9（撤销后复验）：destroy 后重跑一轮（工具覆盖单窗口，复验=重新执行，`client/core/src/n2h.rs:38-39`）。

C2（排除先于建连）的判定输入本来就是 hilog 时间序（既有标记 `VPN_ENDPOINT_EXCLUSIONS_BEGIN` → `VPN_CREATE_RESOLVED`，`client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:808,:871,:1391`），采集器不替代它，只通过把壳侧排除集并入 `frozen_endpoints`（§4.0）使 C1/C2 可在同一证据文档内交叉核对。C3（反例守护）仍在主机侧回环模式执行（`--fault-*`，预注册草案 :55），设备侧无法注入泄漏反例（无 root，预注册草案 §9-6）。

### 1.2 非目标（明确不做）

见 §10。核心三条：不改壳侧排除派生的 3/7 类覆盖（T0 B-2，另一条工作线）；不改变任何连接生命周期/路由/VPN 行为（纯观测+受控探针）；不产出治理结论（`n2h-pass` 仍须独立审查，`docs/n2h-isolation-evidence-notes.md:109-117`）。

---

## 2. 架构

### 2.1 决策：core 内诊断模式（napi 触发），不是独立诊断二进制

**结论**：采集器 = core 新模块 `client/core/src/n2h_collector.rs`，随现有 cdylib `libnetbird_core.so` 打进签名 HAP（构建目标不变：`bash client/build.sh`，runbook :105-109）；由 napi 新导出触发（§5），复用 `n2h.rs` 的类型、判定引擎与帧解析（`client/core/src/lib.rs:141`——`pub mod n2h` 未 cfg 门控，本就编入 cdylib）。**不新建独立二进制**。

**否决独立二进制的理由**（逐条可核）：

1. **TUN fd 只存在于 App 进程**。平台 TUN fd 由壳 `VpnConnection.create()` 创建、native 只消费 dup（fd 合同 `client/core/src/tun.rs:1-24`；投递链 `napi.rs:547-554` → `wg_device.rs:1845`）；独立进程拿不到该 fd（壳不跨进程传 fd），**看不到任何 TUN 帧**，四条证据腿里两条（C4/C5）直接不成立。
2. **探针必须是 App 自身的流量**。N2-H 的主张是「App 的外层端点流量经主机路由排除旁路隧道」；探针从 App 进程发出才受同一 VPN 路由域约束。经 `hdc shell` 起的进程运行在 shell 域，其路由域/UID 覆盖与 App 不同（且 T0 平台取证已确认逐 socket protect 不可用，`docs/native-nx-governance.md:122-123`），其探针不能代表 App 路径。
3. **冻结来源全在 core 进程内**：network map（stuns/dns/signal）、relay 广告 URL、`wg_peers[]` 端点、WG 设备计数（§4.0 表）。
4. **逐字段对齐天然成立**：设备证据文档由与主机侧同一份 `Evidence`/`judge`/`to_json` 代码装配（`n2h.rs:776-974`），差异只在取数层（§3 登记表）。
5. **先例**：`host_sockets` 即「编译进 rlib、设备路径从不主动调用、cdylib 行为不变」的诊断模块（`client/core/src/lib.rs:114-124`）；本设计沿用同一纪律（§6.1）。

### 2.2 组件与数据流

```
hdc（主机）                          设备 App 进程（cn.alfadb.netbird / vpn 扩展）          对端（主机侧）
────────                           ─────────────────────────────────────────────         ──────────────
hdc file send                 ets watcher（:905-1019，5s 轮询）
  /data/local/tmp/netbird-n2h-arm ──► 读控制文件 → napi: n2h_collector_arm(optsJson)
                                       │  opts 含壳侧排除集（deriveEndpointExclusions :756-887 的结果）
                                       ▼
                              n2h_collector（core，新模块）
                               ├─ 冻结装配（core 来源 ∪ 壳排除集）        …………… C1
                               ├─ 窗口开：配对快照（/proc/net/dev + wg stats）… C7 before
                               ├─ 探针发送（wildcard socket，App 流量）      …………… C6 前半
                               ├─ 正控 a2b（发往对端 overlay）◄──────────────► nbinterop n2h-peer
                               │   （echo 回包 / b2a 探针）                        （echo+posctl，I6）
                               ├─ TUN tee（WgDevice 两挂点，§4.1）           …………… C4/C5/C7
                               ├─ 收执闭合（STUN 自证 / 对端计数 / DNS echo / 缺口如实）
                               ├─ 窗口关：配对快照                            …………… C7 after
                               ├─ Evidence + judge（复用 n2h.rs:797-926）    …………… C8
                               └─ 分块编码 → hilog N2H_EVIDENCE|…（可按需重发）
hilog 抓取（grep N2H_） ◄──────────────────────────────────────────┘
重组+sha256 校验 → 证据 JSON 落 AUTH 证据目录
```

**窗口生命周期**（与「测量窗口 = 一次连接会话内的受控测量窗」对应，预注册草案 :47）：

- **arm**：ets watcher 检查控制文件存在且 `connector_status().wg.device_up && ready`（fail-closed：会话不 ready 不 arm，登记 `not-armed-reason`）；arm 即挂 tee、记 `opened_mono/unix`。
- **运行**：冻结装配 → before 快照 → 外层探针（7 类按冻结结果）→ a2b 正控 + 等回执 → settle 尾拍（参照主机 `OBSERVE_SETTLE_ROUNDS` 形态，`n2h.rs:1230-1232,:1734-1750`）→ after 快照 → 判定。全程 deadline 有界（不依赖长睡眠）。
- **自闭合**：窗口关 = tee detach、证据文档定稿并缓存于内存；hilog 分块输出一遍。
- **重发**：`n2h_collector_evidence(chunk)` 可反复调用（hilog 丢行时重放，§2.4-R2）。
- **中止**：`n2h_collector_disarm()`；进程死亡/force-stop 时产物丢失（仅内存+hilog 已出行），如实登记。

### 2.3 启动/停止/取回在 hdc 侧的可操作面

全部落在 runbook 已登记的命令类别内（§6.4 给出逐字清单）：`hdc file send`（控制文件）、`aa start`/`uitest uiInput click`（起 App/连 VPN，既有流程）、`hdc shell "hilog …"`（抓取）、`hdc shell "cat /proc/net/dev"`（备用口径）、`aa force-stop`、`/data/local/tmp` 清理。**无 root、无新权限、无系统参数改动。**

### 2.4 产物离设备：通道对比（容量/可靠性/敏感面）

| 通道 | 形态 | 容量 | 可靠性 | 敏感面 | 结论 |
|---|---|---|---|---|---|
| **A. hilog 分块（主通道）** | core `hilog::emit`（domain 0x2900/tag N1BDiscVpn，`client/core/src/hilog.rs:30-56`，INFO 级）逐行输出 `N2H_EVIDENCE|seq=i/n|sha256=<全文档>|b64=<chunk>` | 证据 JSON 约 8–20KB（主机侧 loopback 文档实测同量级：7 探针+2 正控+快照对）；按保守 512B/行载荷 → 20–60 行 | ① INFO 级可被设备 baselevel 滤除（S13 实录：重启后应用域被滤到 WARN+，`docs/n13-device-relay-evidence-20260916.md:91-98`）→ **按需重发**（napi 重放任意 chunk）+ AUTH 显式列日志级别设置命令（预注册草案 §5-D13 已要求逐字列出）；② 缓冲旋转 → 抓取即时（窗口闭合后立刻抓） | 证据文档含外层端点 IP/域名（`frozen_endpoints[].resolved`，判据必需；ets 日常标记只打 counts 不打 IP，`NetBirdVpnExtensionAbility.ets:755`——本通道是**有意的例外**，产物只落 AUTH 证据目录+sidecar+MANIFEST，敏感扫描与脱敏规则沿 runbook §5 | **采用** |
| B. 沙箱文件 | ets `fs` 写 cacheDir/filesDir（跨进程状态桥先例 `NetBirdVpnExtensionAbility.ets:960-964`） | 无限制 | 落盘即持久 | hdc **读不到** App 沙箱（shell 域无权限） | 仅本地留档/跨进程，**不是离设备通道** |
| C. `/data/local/tmp` 文件 + `hdc file recv` | App 进程写 `/data/local/tmp/netbird-n2h-evidence.json` → `hdc file recv`（先例：E0 轮截图回收 `docs/evidence/raw/EV-E0-EMU24-20260717-0001-transcript.log:540`；App **读**该目录已证实 world-readable，`NetBirdConnector.ets:27-33`） | 无限制 | 一次成功即完整 | 同 A；另多一个中间文件（清理入 AUTH） | **实验性备选**：App 对该目录的**写**权限未验证（§9-U2），验证通过前不依赖 |
| D. `hdc shell "cat /proc/net/dev"` | 只读计数（C7/C9 口径），非证据文档通道 | 4 行文本 | 高（N13 轮实测 3 次读取成功，`docs/n13-device-relay-evidence-20260916.md:96`） | 接口名/计数，无秘密 | **采用**（App 内直读失败时的 fallback，§4.4） |

重组脚本（主机侧）：grep `N2H_EVIDENCE` → 按 seq 拼接 b64 → 解码 → sha256 与各行声明的全文档哈希比对 → 通过才落证据目录。缺行即重放对应 chunk（经 ets 再调 napi；或接受 `n2h-inconclusive` 如实登记）。

---

## 3. 证据 JSON：与 n2h.rs 逐字段对齐

### 3.1 对齐原则

设备证据文档 = `n2h::Evidence::to_json()`（`client/core/src/n2h.rs:931-974`）的**同一渲染代码**输出：字段名、类型、顺序、嵌套形状完全一致；`SCHEMA_VERSION` 保持 `1`（`n2h.rs:66`）。所有差异只发生在**取数层与字面值**，逐条登记于 §3.2。禁止增删顶层字段；新增信息一律走 `notes[]` 与文档尾部追加的 `collector{}` 块（尾部追加纪律，先例 a1947b9 的 `wg_peers`）。

### 3.2 顶层字段逐个对齐表

| 字段（n2h.rs 出处） | 类型 | 设备侧取值/语义 | 差异 |
|---|---|---|---|
| `tool`（:952） | string | `"n2h-isolation-evidence"`（`TOOL_NAME`，:64）——**同名**（判据要求工具口径 = nbinterop isolation-check 及其等价探针，runbook :275） | 无 |
| `schema`（:953） | number | `1` | 无 |
| `mode`（:954） | string | `"device"`（host: `"loopback"`/`"preconnect-freeze"`） | **D1** |
| `session`（:955） | string | `"device session; TUN = platform vpn-tun fd (dup); outer probes from app process"` | **D2**（描述性） |
| `generated_unix` / `started_mono` / `finished_mono`（:956-958） | number | 窗口开/关与文档生成时刻（`sys::mono_ms()` + unix 钟；两者均已在设备路径使用） | 无 |
| `frozen_endpoints[]`（:959；元素结构 :171-206：`kind/present/host/port/proto/source/resolved/absent_reason/frozen_at_unix/frozen_at_mono`） | array | 7 类冻结装配（§4.0）；`kind` 取值与 `EndpointKind::ALL`（:155-163）逐一相同；必需/可选语义沿 `EndpointKind::required()`（:151-153） | **D7**（`source` 取值域扩展） |
| `probes[]`（:960；元素 :268-299：`probe_id/kind/tuple/expected_path/sent/sent_at_unix/sent_at_mono/payload_marker/payload_len`） | array | 每端点一条，独立临时 socket、五元组唯一、载荷唯一（`probe_id = N2H-<kind>-<nonce>`，nonce /dev/urandom，:754-770）；`expected_path:"outer"` | 无 |
| `tunnel_positive_control`（:961；结构 :400-423：`observed_in_tunnel/expected_path/probes[]{probe_id,direction,tuple,expected_path,sent,sent_at_mono,observed_in_tunnel,note}`） | object | a2b/b2a 各 ≥1 条（§4.3）；`direction:"a2b"|"b2a"` 字面与 host 一致 | **D5**（观测点定义，见 §4.3） |
| `tun_negative`（:962；结构 :335-361：`hit/frames_scanned/hits[]/scanned_sides/method/criteria`） | object | 两方向 tee 全帧扫描（§4.1） | **D3/D4** |
| `endpoint_side[]`（:963；元素 :431-463：`probe_id/kind/observed/physical_src/physical_src_source/at_unix/method/detail`） | array | §4.0 收执矩阵 | **D6**（`method` 取值域） |
| `counters[]`（:964；元素 :544-576：`side/before/after/delta/reconciled/reconciliation`；快照 :471-542 含 `wg{...}`/`tun{injected_frames,delivered_frames,delivered_bytes}`） | array | 单元素（一台设备一个 TUN） | **D8/D9** |
| `verdict`（:965） | string | 由同一 `Evidence::judge()`（:797-926）产出：`n2h-pass|n2h-fail|n2h-inconclusive`；判定顺序（证伪优先→冻结→探针→正控→收执→对账）逐条同实现 | 无 |
| `reasons[]` / `notes[]`（:966-967） | array | 缺口与运行注记（含设备特有注记：收执缺位、口径 fallback） | 无（内容差异自然存在） |
| `residual_scope[]` / `unsat_note`（:968-971） | array/string | **逐字**复用 `RESIDUAL_SCOPE`（:84-93）与 `UNSAT_NOTE`（:80） | 无 |
| （尾部追加）`collector{}` | object | 自标识块（§3.3） | **D10**（尾部追加） |

### 3.3 `collector{}` 自标识块（A-1：构建/版本/哈希自标识）

尾部追加一个对象（不插入既有字段之间）：

- `crate_version`：`env!("CARGO_PKG_VERSION")`（先例 `signal.rs:213` 的 `SIGNAL_CLIENT_VERSION` 同法；当前 `0.1.0`，`client/core/Cargo.toml:24`）；
- `target_triple`：编译期 `env!("TARGET")`（区分 host 构建 vs `aarch64-unknown-linux-ohos`）；
- `build_sha256`：构建期注入（`client/core/build.rs` 已存在，proto codegen 同文件追加 `cargo:rustc-env=N2H_BUILD_SHA256=…`，由 `client/build.sh` 传入 HAP/so 的 sha256——build.sh 本就计算并打印 .so/HAP sha256，runbook :109）；
- `build_source`：`"embedded"`（构建期注入）或 `"operator-reported"`（fallback：arm opts 携带操作者登记的 sha256，如实标注来源，避免「推断顶替实测」，预注册草案 :69）。

### 3.4 差异登记表（全部差异，逐条）

| # | 差异 | host（n2h.rs） | device（本设计） | 理由/依据 |
|---|---|---|---|---|
| D1 | `mode` | `"loopback"`/`"preconnect-freeze"` | `"device"` | 模式标识；oracle 消费用 |
| D2 | `session` 描述 | 双实例回环 | 设备会话描述 | 描述性 |
| D3 | `tun_negative.scanned_sides` | `["a","b"]`（每侧一个 socketpair 端点） | **字面 `["vpn-tun"]`**（设备只有一个平台 TUN，接口名常量 `vpn-tun`，`NetBirdVpnConfig.ets:45-46,:87`） | **B-5 直接落点**；单接口字面枚举 |
| D4 | `tun_negative.hits[].side` | `"a"/"b"` | `"vpn-tun"`，且 hit 对象**尾部追加** `direction:"from-os"|"to-os"`（from-os=OS 写入 TUN 的出向帧（core 读路径）；to-os=core 解密写回 TUN 的入向交付帧） | 单接口双向；方向是取证必需（泄漏方向判读），尾部追加不破坏既有字段 |
| D5 | 正控 `observed` 的观测点 | a2b 在对端 TUN 交付侧（hand_b drain，`n2h.rs:1546-1549`）；b2a 同理 | b2a=设备 to-os 向 tee（同构：接收端交付侧）；a2b=设备 from-os 向 tee（帧入隧道入口）**且** echo 回包在 to-os 向命中（端到端闭合，§4.3） | 设备无法在对端进程内 drain；入口+回环双点不弱于单点（正控语义=「隧道观测路径已被证明」，`n2h.rs:864-885`） |
| D6 | `endpoint_side[].method` 取值域 | `sink-record`/`stun-binding-response`/`peer-device-offpath-drop`（notes :25） | 前三者同（`sink-record` 仅离线 harness 用）+ 新增 `dns-nxdomain-echo`（待验证，§4.0）+ `none`（运维收执缺位，缺口如实） | 设备端点是真服务，无 mock sink |
| D7 | `frozen_endpoints[].source` 取值域 | `loopback-mock-sink`/`config`/`cli`/`wg-device-outer-socket`（:179-181） | 新增 `shell-exclusion`（壳排除集，:756-887 的 resolution 结果）/`core-network-map`/`core-relay-advertised`/`core-signal-config` | 来源可追溯性（C1/C2 交叉核对） |
| D8 | `counters[]` 元素数与 `side` | 2 元素（a/b） | **1 元素**，`side:"vpn-tun"`；`tun.*` 三字段口径 = `/proc/net/dev` vpn-tun（`delivered_*`=RX、`injected_frames`=TX packets，§4.4） | **B-6 直接落点**；单接口；独立第二账本=内核接口计数（host 是采集器自己 drain socketpair——设备上 core 自己的 tee 与 `rx_bytes_to_tun` 同源，不构成独立口径，故必须用 /proc） |
| D9 | `tun_injected_frames` 语义 | 采集器主动注入 TUN 的帧数（驱动统计，`n2h.rs:1460-1461`） | 窗口内 OS→TUN 方向帧数（/proc TX packets delta；真实系统里「注入方」是 OS 而非采集器） | 方向对应关系如实命名 |
| D10 | 尾部 `collector{}` | 无 | 有（§3.3） | A-1 |

离线 harness（§8）中 `/proc` 不可用时，`tun.*` 以 tee 独立计数替位并在 `notes[]` 声明 `counters-source=tee-fallback`——**仅离线测试**；设备真机一律 /proc（或 D 类 hdc fallback 并声明）。

---

## 4. 四个证据腿的设备侧实现方案

### 4.0 前置：冻结装配（C1 的供证）与收执矩阵

**冻结来源**（全部进程内可得，无秘密材料）：

| kind | 设备侧来源（file:line） | 备注 |
|---|---|---|
| management | 壳排除集（`shell-exclusion`，`NetBirdVpnExtensionAbility.ets:816-818`）∪ core 拨号地址 | 拨号 IP=当轮解析、零缓存 |
| signal | 壳排除集（:820-825）∪ `netbird_config.signal`（`connector_network_config` 的 `signal` 字段，`lib.rs:88`；ets 侧 `NetBirdConnector.ets:269-272`） | |
| relay | 壳排除集（:827-849）∪ `relay_advertised.advertised_urls`（core status，`connector.rs:2373,:2737-2847`；「广告即排除」语义 runbook :195） | DDNS 每次调用重解析、换址=重冻 |
| stun | core network map `netbird_config.stuns[].uri`（解析 `stun:host[:port]`，`ice.rs:100`；map 解码 `network_map.rs:617`） | 壳排除集目前**不含**此类（B-2 的 3/7 现状，如实入档） |
| turn | 同上来源；当前实现**拒绝** `turn:` URI（`peer_conn.rs:701`；host 侧同拒 `host_sockets.rs:1187-1188`） | 预期常态=`present:false` + `absent_reason:"turn: URI rejected by this build (no turn path)"`（可选类带理由缺席，非缺口，`n2h.rs:151-153`） |
| wg_peer | `connector_status().wg.wg_peers[].endpoint`（`wg_device.rs:298,:1386`；a1947b9 尾部追加面） | ICE 选中对外 UDP 端点；无会话时 fail-closed 不 arm |
| dns | core network map DNS（`dns.servers[{ip,port}]`+`interface_dns`，`connector.rs:2537-2547`；`network_map.rs:84,:329`） | |

冻结时刻=arm 时刻（会话已建立、探针未发）。`present/absent_reason/required` 判定复用 `EndpointKind`（`n2h.rs:106-168`）与 `judge` 第 2 步（:824-849）。

**收执矩阵（C6）**：

| kind | 设备侧收执方法 | `physical_src_source` |
|---|---|---|
| stun | `stun-binding-response`（txid 匹配 Binding 响应，XOR-MAPPED=端点报告的物理源址；复用真 codec `stun.rs`，与 host 同法 `n2h.rs:1158-1197`）——**自证闭合** | `endpoint-reported` |
| wg_peer | `peer-device-offpath-drop`：对端（主机侧 `nbinterop n2h-peer`）的 `unknown_peer_drops` 增量，经其对端状态通道回报（§4.3）；设备侧记 `physical_src`=探针 socket getsockname | `probe-socket-getsockname`（与 host 同法 `n2h.rs:1474-1486`） |
| dns（可选） | `dns-nxdomain-echo`：载荷=`<marker-dns安全字符>.n2h-invalid` A 查询，resolver 回包 QNAME 回含 marker 即收执（**方法未在真实 resolver 验证，默认关闭，开启后失败仅记缺口**） | `endpoint-reported` |
| management / signal / relay（TCP） | `none`：依赖运维侧服务端投递日志（预注册草案 §9-4 明示：未取得前记缺口、不得假通）→ `observed:false` → 判定如实 `n2h-inconclusive` | — |

> 诚实结论：在运维收执缺位时，**设备轮 verdict 预期恒为 `n2h-inconclusive`（收执缺口）**，这是设计行为而非工具缺陷（与预连模式恒 inconclusive 同类，runbook :240）；判据的价值在 C4 负证据（可证伪）与其余腿的可判定性。要 `n2h-pass` 须先与运维约定观测窗口（预注册草案 §5-E）。

### 4.1 腿①：TUN 帧扫描（C4）

- **挂点（tee）**：`WgDevice` 增加一个 `Option<Arc<Mutex<CollectorTee>>>` 诊断句柄，两处喂帧：
  1. **from-os 向（出向）**：`service_tun` 在 `read_frame` 成功后、`send_from_tun` 路由判定**之前**（`wg_device.rs:875-877`）——排除失效的外层探针被 OS 路由进 TUN 时，**无论后续被路由还是 `no_route_drops`，帧内容先入扫描**；
  2. **to-os 向（入向交付）**：`process_datagram` 解密写 TUN 成功后（`wg_device.rs:1011-1017`，与 `rx_bytes_to_tun += len` 同一成功分支）。
  数据面泵线程（`spawn_data_plane_pump`，`wg_device.rs:1815-1832`；100ms 拍，:1604）驱动扫描，锁为短临界区（帧拷贝+子串匹配）。
- **扫描窗口起止**：由采集器状态机控制——arm 时 attach（注册外层探针 marker+正控 marker，复用 `TunnelScanner::register`，`n2h.rs:676-683`），窗口闭合时 detach。arm 前的帧不计数（`frames_scanned` 只覆盖窗口内）。
- **帧计与 hit 判定**：`frames_scanned`=两方向帧数之和；hit=对每个帧做**全帧逐字节子串匹配**（`WireMarker::matches`，`n2h.rs:223-229`）+ `parse_ipv4_frame` 提取五元组（:598-648）；任意外层探针 marker 命中 → `n2h-fail`（证伪优先，:813-822）。
- **性能与开关**：marker 数 ≈ 探针数+正控数（≤ ~10）；每帧代价 = O(markers × 帧长) 子串扫描；窗口内帧率为 keepalive+探针级（低）。`tee=None`（默认/未 arm）时代码路径与现在完全一致——**零额外开销、cdylib 行为不变**。
- **备选挂点**（若 WgDevice 内钩子在审查中被否）：包装 `TunFd::read_frame/write_frame_budget`（`tun.rs:207-228,:263-303`）——触面更小，但 `tun.rs` 会话表面（`tun_open` 等旧探针路径）也被覆盖，语义混淆且会扫到非 WG 设备流量；列为备选不推荐。

### 4.2 腿②：探针注入器（C6 前半/C1 供证）

- **新建设备探针发送器**（不复用 host 私有函数）：UDP/TCP 一律 **wildcard 绑定 `0.0.0.0:0`**（每探针独立临时 socket，五元组唯一；EINVAL 回归教训——loopback 绑定对真实外层端点 send 报 EINVAL，`n2h.rs:1163-1170` 注释与 :2373-2396 回归测试）。host 的 `send_udp_probe` 绑 `127.0.0.1:0`（:1135）**明确不复用**。
- **载荷 magic 与指纹**：与 host 完全一致——ASCII marker=`probe_id`（`N2H-<kind>-<nonce>`，nonce 24 hex / `/dev/urandom`，:754-770）；STUN 探针=12B txid（Binding Request 定长线格式）；TCP 探针=connect+write+shutdown（:1145-1154 形态）。
- 探针经 App 正常网络栈发出（受 VPN 路由域约束：被排除 → 外路；未排除 → 入 TUN → 命中扫描 = 真实的 B-2 发现，如实 `n2h-fail`，不是工具故障）。

### 4.3 腿③：正控观测（C5）

- **a2b（设备→对端 overlay）**：采集器用 App 进程 UDP socket 向**对端 overlay IP**（network map `peers[].allowed_ips`）的约定端口发送 `N2H-posctl-<nonce>|positive-control`（payload 形态同 host，`n2h.rs:1558`）；对端运行主机侧 **`nbinterop n2h-peer`**（新子命令，§7-I6）：echo 模式把含 marker 的 UDP 载荷原样回发设备 overlay IP。设备侧观测点：from-os tee 命中（帧确实离开 OS 进入隧道入口）**且** echo 回包在 to-os tee 命中（载荷经隧道往返）→ `observed` 置位（D5）。
- **b2a（对端→设备 overlay）**：`n2h-peer` 主动向设备 overlay IP 发 posctl 载荷；设备 to-os 向 tee 命中 = 接收端 TUN 交付侧观测（与 host `hand_b` drain 同构）→ `observed` 置位。
- 双向各至少一条（预注册 C5）；任一未观测 → `n2h-inconclusive`（`n2h.rs:864-885`：正控缺失永不 pass——**它同时守护「App 自身流量可能被自身 VPN 豁免导致负证据空洞」这一平台语义未知点**，§9-U3）。
- 对端 `unknown_peer_drops` 增量（wg_peer 探针收执）由 `n2h-peer` 周期输出状态行（stdout/hilog 由主机侧直接捕获，不经设备）。

### 4.4 腿④：计数对账（C7，B-6 直接落点）

- **两口径**：`wg.rx_bytes_to_tun`（status JSON，core 计数，`wg_device.rs:1362`，递增点 :1015）↔ `/proc/net/dev` **vpn-tun RX bytes**（内核接口账本）。两者在 N13 D1 轮真机已实证恒等（`wgRxToTun=4624` == vpn-tun RX 4624，`docs/n13-device-relay-evidence-20260916.md:66,:83`）——可行性有实测先例。
- **恒等式**：`delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`（`RECONCILIATION_NOTE`，`n2h.rs:100`；`WindowCounters::reconciled`，:560-563）。**附加第二恒等式**（本设计强化）：`delta.wg.rx_packets == delta.tun.delivered_frames`（/proc RX packets）——用于配对快照的自洽校验。
- **采集时点与配对纪律**：窗口开/关各一次**配对快照**——采集器**先读 `/proc/net/dev` 再读 wg stats**（进程内两次读，间隔 µs–ms 级）；若两恒等式在 after 配对上不成立（说明两次读之间有帧穿越），**有界重采样 ≤3 次**取自洽配对；仍不成立 → 如实 `reconciled:false` → `n2h-inconclusive`（不修补、不重试到平为止）。
- **App 直读 /proc 权限未验证**（§9-U1）：fallback = AUTH 允许的 `hdc shell "cat /proc/net/dev"`（D 类；hdc 往返引入秒级 skew → 配对纪律退化为「窗口静默期 + 手工配对」，且在 `notes[]` 声明 `counters-source=hdc-proc-fallback`）。两条路径都失败 → `counters[]` 缺失 → `n2h-inconclusive`（`n2h.rs:904-910`）。
- `tun_injected_frames` = /proc vpn-tun **TX packets** delta（OS→TUN 方向，D9）。

---

## 5. napi / ets 面

### 5.1 新增 napi 导出（4 个，命名沿既有 `connector_*` 风格；同步 JSON-string-in/out，`napi.rs:1-7` 纪律）

| 导出 | 签名 | 返回 |
|---|---|---|
| `n2h_collector_arm(optsJson: string)` | opts：`{"shell_exclusions":[{kind,host,port,resolved}], "posctl_port":40001, "probe_wait_ms":800, "build_sha256_reported":"…"（可选）}` | `{ok:true,armed:true,opened_mono} \| {ok:false,error:"no-connector"\|"device-not-up"\|"tunnel-not-ready"\|"already-armed"}` |
| `n2h_collector_status()` | `()` | `{armed,phase:"armed\|probing\|observing\|closed",frames_scanned,hits,probes_sent,receipts_observed,window:{opened_mono,deadline_mono,closed_mono},verdict?}` |
| `n2h_collector_evidence(chunk: number)` | chunk=-1 取总块数 | `{seq,total,sha256,b64} \| {phase:"running"} \| {error:"no-evidence"}`（**可重复调用**，hilog 丢行重放） |
| `n2h_collector_disarm()` | `()` | `{ok:true}`（中止窗口，未闭合产物弃置并如实登记） |

注册进 `napi_init` 表（`napi.rs:91-119` 形态）与 lib.rs NAPI 表（:56-95）。

### 5.2 状态 JSON 变更：**不动 `connector_status()`**

诊断状态走独立 `n2h_collector_status()`，**不**在冻结的 `connector_status` 文档上追加字段——避免触碰消费方（ets/Index UI/host CLI）与 a1947b9 建立的尾部追加纪律（真要加也只允许尾部追加 `n2h_collector{armed,phase}`；登记为审查决策点，默认不加）。

### 5.3 ets 侧最小改动（3 处，全部在扩展进程）

1. **watcher arm 钩子**：`startStatusAndNetcfgWatcher`（`NetBirdVpnExtensionAbility.ets:905-1019`）轮询里加一次性检查——控制文件 `/data/local/tmp/netbird-n2h-arm` 存在（`fs.accessSync`，`readCredentialFileText` 同款开发期通道形态，`NetBirdConnector.ets:312-331`；该文件只含非敏感 opts）且 `status.wg.device_up && status.wg.ready`（:931-936 同源字段）→ 调 `n2h_collector_arm`，打标 `N2H_COLLECTOR_ARMED|requestId=…|diag=true|is_evidence=false`（arm 动作本身不构成门证据，产物经独立审查才可引用）。5s 轮询粒度带来的窗口起点漂移由 `opened_mono` 如实记录（不影响判据，判据只看窗口内）。
2. **壳排除集留存**：`deriveEndpointExclusions`（:756-887）的 `EndpointResolution[]` 结果留存到 ability 实例，arm 时并入 opts（C1/C2 交叉核对，D7）。
3. **无 UI、无产品配置项、不改连接生命周期**。控制文件是开发期通道（与 `netbird-protect-deviation` 同目录同类，runbook :158-165 先例），产品面零暴露。

---

## 6. 安全与治理

### 6.1 默认关闭、不进产品面

- **惰性**：无 arm 文件 → watcher 不调 arm → tee=None、collector 状态机不存在；napi 导出在未 arm 时返回 `{ok:false,error:"not-armed"}`。cdylib 其余行为逐字节不变（先例承诺形态：host_sockets「device path never calls into this module and the shipped cdylib behavior is unchanged」，`lib.rs:117-123`）。
- **治理约束的代码体现**（N6 未 pass 前不开启产品实现）：collector 不含任何路由/排除/VPN 生命周期代码路径——它是**纯观测者+受控探针发送者**；窗口内新增出站流量 ≤ ~10 个包（探针+正控）；arm 标记恒带 `diag=true|is_evidence=false`；verdict 只由预注册判据解释，工具不产生门结论（`n2h.rs:15-20` 治理范围声明逐字适用）。
- 采集器模块文档头复制 n2h.rs 的治理绑定声明（不改变治理条款；`n2h-pass` 非原 N2 判据 pass）。

### 6.2 无秘密材料

- 证据文档只含 probe_id/地址/计数/公钥级信息（n2h.rs:928-930 原则）；collector 不读 `private_key`/setup key/JWT（冻结来源全部是非凭据字段，§4.0）。
- hilog 通道的敏感面（外层端点 IP 清单）已在 §2.4 登记：仅经 AUTH 授权窗口传输、只落 AUTH 证据目录、逐文件 sidecar+MANIFEST、敏感扫描词表沿 runbook §5（:304-310）。

### 6.3 与停止条件的对接

采集器不新增停止条件；既有 S1–S14（预注册草案 §4）全部适用。采集器自身的异常（tee 未见任何帧=零帧扫描 → `n2h-inconclusive`，C4 判定输入；hilog 丢行且重放失败 → 按 S13 仪器异常先停再上报；`/proc` 双路径皆不可读 → 收腿如实登记）。

### 6.4 AUTH allowed 逐字命令面（E-1；供人类签发参考，逐字列出，未确认不得执行）

在预注册草案 §5 清单之上，采集器轮**新增/具体化**的设备侧命令（既有条目——tconn/param get/bm dump/install/aa start/uitest/force-stop/uninstall/rm——沿草案原文，不重复）：

```
1. hdc file send <主机侧 arm 文件> /data/local/tmp/netbird-n2h-arm
2. hdc shell "rm -f /data/local/tmp/netbird-n2h-arm"（收尾清理，归入草案 F-18 类）
3. hdc shell hilog 清缓冲与抓取——抓取 pattern 须含 N2H_EVIDENCE、N2H_COLLECTOR、
   N2H_（采集器标记族）与既有 NetBirdVpn/N1BDiscVpn（限本项目 tag，非持久）
4. hilog 日志级别设置（限本项目 tag、非持久、重启失效）——单独逐字列出（预注册草案 §5-D13 教训：不在字面内不得执行）
5. hdc shell "cat /proc/net/dev"（vpn-tun 计数，C7/C9；App 内直读失败时为 D 类 fallback）
6. 主机侧（非设备操作，一并登记）：nbinterop n2h-peer 的启动与停止
   （形态：target/debug/nbinterop n2h-peer --overlay <ip:port> --echo-port 40001）
```

每条的坐标/目的/次数逐条登记（沿草案 §5 纪律）；授权类别选择（campaign pair vs 诊断例外二）仍由人类/T0 决定（预注册草案 §5 末、§9-1）。

---

## 7. 实现切分（增量；每项：产出 + 测试 + 验收命令 + 触面）

依赖序：I0/I1/I2/I3/I4 相互独立可并行 → I5 汇总 → I6/I7 收尾。

| # | 增量 | 产出 | 测试 | 验收命令（host） | 触面（文件级） |
|---|---|---|---|---|---|
| I0 | 分块编码器 | `n2h_collector.rs`：证据文档→512B b64 分块+seq/total/sha256+重放 | 单测：roundtrip、sha256 抗篡改、缺块检测 | `cd client/core && cargo test --offline --locked n2h_collector` | 新文件 1 |
| I1 | 冻结装配 | 7 类来源装配 + 壳排除集合并 + `source/absent_reason` 标注（纯函数，依赖注入） | 单测：7 类齐/缺/可选缺席/来源标注 | 同上 | `n2h_collector.rs` |
| I2 | WgDevice tee | 两挂点+双向帧/字节计数+`None` 零开销路径；attach/detach | 新 `tests/n2h_device_tee.rs`：socketpair 假 TUN（`host_sockets::open_tun_standby_pair`）双 WgDevice，断言两方向帧均入 tee、计数与 `WgDeviceStats` 一致、未 arm 时行为与现状逐字节同 | `cargo test --offline --locked --test n2h_device_tee` + 全量回归 | `wg_device.rs`（2 挂点+1 字段+setter）；新测试 1 |
| I3 | 设备探针发送器 | wildcard UDP/TCP/STUN（+可选 DNS echo 收执解析） | 单测：本地 sink 收到 marker、五元组唯一（host 上 wildcard 绑定成立，`n2h.rs:2373-2396` 先例） | `cargo test --offline --locked n2h_collector` | `n2h_collector.rs` |
| I4 | /proc 对账读取 | vpn-tun 解析+配对快照+双恒等式+有界重采样（读取 seam 可注入 fixture） | 单测：fixture 文本解析、恒等/不恒等/重采样/双路径 fallback | 同上 | `n2h_collector.rs` |
| I5 | 状态机+判定+napi+ets | arm→窗口→判定→分块输出全链路；napi 4 导出；ets watcher 钩子+排除集留存 | 新 `tests/n2h_device_collector.rs`（§8 详）；napi 注册测试更新（`napi.rs:349-387` 名单加 4 项） | `cargo test --offline --locked`（全量，预期全绿）+ `client/entry/selfcheck`（若新增纯函数则镜像） | `n2h_collector.rs`、`napi.rs`、`lib.rs`（模块+表行）、`NetBirdVpnExtensionAbility.ets`（2 处） |
| I6 | `nbinterop n2h-peer` | echo+posctl 发送+`unknown_peer_drops` 状态行 | CLI 单测（loopback 对拍） | `cargo build --offline --locked --features cli --bin nbinterop && cargo test --offline --locked` | `bin/nbinterop.rs`（+n2h_collector 复用） |
| I7 | 构建自标识+runbook 增补 | build.rs 注入 `N2H_BUILD_SHA256`；`collector{}` 块；runbook 新章节（设备操作步骤） | 构建烟测：自标识字段非空且与 build.sh 打印一致 | `bash client/build.sh` 后核对输出 | `build.rs`、`n2h_collector.rs`、runbook 文档 |

每增量独立可验收、可回滚（I2 是唯一触生产文件 `wg_device.rs` 的增量，diff 限两挂点+字段）。

---

## 8. 离线测试策略（复用既有 harness）

- **假 TUN**：`host_sockets::open_tun_standby_pair`（SOCK_SEQPACKET 帧保真，`n2h.rs` loopback 驱动同款 :1269-1276；帧边界回归 `host_sockets.rs` tests）——I2/I5 的 TUN 替身。
- **忠实假服务端**：本地 `UdpSink/TcpSink`（`n2h.rs:991-1097`）作外层端点 sink（离线时 `sink-record` 收执可用）；`relay_testserver.rs`/`signal_mock`/`management_mock` 本设计不需要（不拨真实控制面）。
- **既有回归守护**：`tests/n2h_isolation.rs` 四场景（host 侧不改行为，必须原样全绿）；`tests/tun_fd_contract.rs`（tee 不破坏 fd 合同）；`tests/wg_e2e.rs`（数据面回归）。
- **新增 `tests/n2h_device_collector.rs`**（I5，设备采集器离线全链路：注入假 tee 依赖+本地 sink+进程内 echo 对端；`/proc` 不可用 → tee-fallback 口径并声明）：
  1. 干净跑：证据 JSON 可被 `config::parse_document` 解析、**与 n2h.rs schema 逐字段同键同型**（共享断言：对 host loopback 文档与 device 文档跑同一 key-walk 断言）、`scanned_sides==["vpn-tun"]`、正控双向 observed、对账恒等；
  2. 泄漏反例：进程内把外层 marker 帧注入 tee 的 from-os 向 → `n2h-fail`（守护设备判定路径的证伪优先）；
  3. 正控缺失（echo 对端不回）→ `n2h-inconclusive`；
  4. 冻结缺失（signal 来源置空）→ `n2h-inconclusive` 且 reasons 点名 `'signal'`；
  5. 对账破坏（fixture 篡改 /proc 值）→ `n2h-inconclusive`；
  6. 分块 roundtrip + 缺块 + sha256 校验失败检测。

---

## 9. 风险与备选

### 9.1 技术风险

| # | 风险 | 影响 | 缓解/备选 |
|---|---|---|---|
| R1 | App 进程读 `/proc/net/dev` 权限未验证（§9-U1） | C7 双口径退化为 hdc 秒级 skew 配对 | D 类 fallback + 窗口静默期纪律 + `notes[]` 声明；两路皆失 → 如实 inconclusive |
| R2 | hilog INFO 被滤（S13 实录） | 证据出行丢失 | 按需重放（napi）+ sha256 自校验 + AUTH 逐字列级别设置；仍失败按 S13 停止上报（不得自行扩大限额，预注册草案 :130） |
| R3 | App 自身流量被自身 VPN 豁免（平台语义未知） | 负证据空洞（恒零命中） | **正控腿就是守护**：a2b 发不进隧道 → observed=false → 恒 inconclusive，永不假 pass（`n2h.rs:864-885` 设计语义） |
| R4 | tee 只见 core 所见帧 | 内核级丢弃的帧不可观测 | 与 host 同残差（residual_scope 已含观测边界声明）；如实登记 |
| R5 | /proc RX 口径与 `rx_bytes_to_tun` 在部分写/EAGAIN 边界的差异 | 恒等式偶发破 | 双恒等式（bytes+packets）+有界重采样；不平=如实 inconclusive（对账腿的本职就是发现不一致） |
| R6 | DNS echo 收执未验证 | dns 收执缺位 | 默认关闭；缺位走 `method:"none"` 缺口，不影响其他腿 |
| R7 | 运维收执缺位（mgmt/signal/relay TCP） | verdict 恒 inconclusive | 设计行为（§4.0）；要 pass 须运维观测窗口（预注册草案 §9-4） |
| R8 | watcher 5s 粒度 arm 时延 | 窗口起点漂移 | `opened_mono` 如实记录；判据只依赖窗口内观测 |
| R9 | tee 临界区与数据面泵竞争 | 帧处理时延 | 短临界区（拷贝+子串匹配）；100ms 拍下帧率低；极坏情况=帧延迟入窗（settle 尾拍兜底，:1734-1750 先例） |
| R10 | `/data/local/tmp` 写权限未验证（通道 C） | 备选通道不可用 | 主通道为 hilog；通道 C 仅实验 |

### 9.2 设计备选

- **TUN 扫描挂点**：主案=WgDevice 两挂点（§4.1）；备选=TunFd 层包装（触面小但覆盖旧探针路径、语义混淆）；备选二=只扫 to-os 向（弱化覆盖，需 T0 明确认可弱化 `scanned_sides` 语义，**不推荐**）。
- **产物通道**：主=hilog 分块；备选=通道 C（验证后可升级为主通道，容量无限且免重组）。
- **状态 JSON**：主=不动 `connector_status`；备选=尾部追加 `n2h_collector{armed,phase}`（若审查认为 hilog 单看 arm 状态不可靠）。

---

## 10. 明确不解决什么（范围外，逐条）

1. **壳侧排除派生的 3/7 类覆盖**（T0 B-2，`NetBirdEndpointExclusion.ets:45-51` 的 kind 仅 management/signal/relay）：是另一条工作线；采集器只如实供证（未排除类被探到入 TUN → `n2h-fail` 是真发现）。
2. **N6 产品实现与治理生效**：采集器是诊断模式，不开启任何产品实现；N2-H 生效仍须用户批准+治理修订（`docs/n2h-isolation-evidence-notes.md:109-117`）。
3. **逐 socket protect**：UNSAT 记录不变（`n2h.rs:80`）。
4. **IPv6**：TUN 帧解析与端点枚举 IPv4-only（`n2h.rs:92`）。
5. **运维侧服务端投递日志的取得**：须运维约定（预注册草案 §9-4）。
6. **预注册授权类别选择、AUTH 签发、设备操作执行**：本文只供设计审查；测量须人类签发的生效 AUTH（预注册草案 :5）。
7. **Emulator**：双向不外推（预注册草案 §1.3），采集器不为 Emulator 适配。

---

## 11. 关键设计难点 / 不确定点清单（诚实标注，供审查席重点核对）

| # | 不确定点 | 现状与验证计划 |
|---|---|---|
| U1 | **App 进程能否直读 `/proc/net/dev`** | 未验证。N13 轮仅证实 hdc shell cat 可行（`docs/n13-device-relay-evidence-20260916.md:96`）。首轮诊断 AUTH 中以一条只读探测确认；失败走 D 类 fallback（§4.4） |
| U2 | **App 进程能否写 `/data/local/tmp`**（通道 C） | 未验证（读已证实，`NetBirdConnector.ets:27-33`）。不影响主通道 |
| U3 | **App 自身流量是否被自身 VPN 路由覆盖**（OHOS 平台语义未知） | 未验证。正控腿在判据上守护（最多 inconclusive、绝不假 pass）；首轮设备运行即可从 a2b 正控结果得知 |
| U4 | **`/proc` RX 与 `rx_bytes_to_tun` 的恒等在长窗口/多流量下的稳定性** | N13 D1 轮单点实证恒等（4624==4624）；本设计以双恒等式+有界重采样守护，不靠单点先例外推 |
| U5 | DNS nxdomain-echo 收执在真实 resolver 的行为 | 未验证；默认关闭、失败仅记缺口 |
| U6 | hilog 单行长度上限 | 未实测（保守取 512B 载荷；首轮实测后可调） |
| U7 | **a2b 正控对 `nbinterop n2h-peer` 的依赖**（对端必须在 overlay 可达且 echo 端口约定一致） | 设计为显式 opts（`posctl_port`）；对端不在线 → 正控未观测 → 如实 inconclusive |
| U8 | 状态 JSON 是否尾部追加 `n2h_collector` 段 | 决策点（§5.2 倾向不加）；待审查裁定 |
| U9 | **I5 离线全链路能否不拉起完整 connector**：collector 依赖（tee 句柄、wg stats、冻结来源、时钟）须做成可注入 trait/seam，生产从 connector 单例（`connector.rs:3878`）取、测试注入假实现——这是 I5 的实现纪律，若不可行则退化为「I5 只测状态机纯逻辑 + 设备轮真机验证」 | 设计已按可注入约定；实现期核对 |
| U10 | hilog 抓取的 grep pattern 与既有 AUTH 条目字面的兼容（`N2H_` 前缀 vs 既有 pattern 集） | §6.4 已单列；须人类逐字确认 |

---

## 12. 自检

- 禁词自检（字符类拆分，口径同预注册草案 §8.2）：本文全文对 `protec[t] pass|N2 pas[s]|等同逐 socke[t] protect|waive[d]|N/[A]` 零命中（写作会话实跑 `grep -niE` 退出码 1）。
- `git status --porcelain` 实跑仅见本新文件（`?? docs/n2h-device-collector-design-20260917.md`），未修改任何既有文件。

*DRAFT for T0 re-review — 2026-09-17，HEAD 7923288。*
