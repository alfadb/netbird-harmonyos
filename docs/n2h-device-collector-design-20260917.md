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
   【I6 落地后的逐字形态，2026-09-17，理由见 §13.8：`target/debug/nbinterop n2h-peer
   --config <主机侧对端 config 路径> --overlay <设备 overlay ip[:port]> --echo-port 40001
   [--posctl-interval <ms>] [--timeout <s>]`；`--timeout` 到期即退出 0（登记为停止形态）；
   `--loopback` 为离线自检对拍，不属设备轮命令面】
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

---

## 13. 实现期补充登记（2026-09-17，I5 落地偏离与接线）

本节**只追加**，不改写 §0–§12 任何正文。登记口径：I0–I5 已合入基线 HEAD `568c931`（`git log --oneline -1` 实读）；下列 file:line 均为本补充节落地后的工作树实读（未 commit）。逐条性质：① 与 ② 是本次补齐（§7-I5 清单遗漏的接线 + 冻结来源扩展，有代码改动）；③–⑥ 是对 `568c931` 内既有实现的事实登记（本节只补登记，不产生代码改动）。

### 13.1 ① §7-I5 文件清单遗漏 `connector.rs` 接线（本节即该缺口的补齐）

- **事实**：§7 表中 I5 的「触面（文件级）」列为 `n2h_collector.rs`、`napi.rs`、`lib.rs`、`NetBirdVpnExtensionAbility.ets`（本文档 :294），**未列 `connector.rs`**。后果不是风格问题而是功能不可用：生产进程从未调用 `n2h_collector::install_host`（`client/core/src/n2h_collector.rs:1672` 的进程级接缝），`n2h_collector_arm` 的 fail-closed 第一闸（同文件 `arm_json`，`no-connector`）恒命中，设备窗口根本开不起来。
- **补齐（本补充节对应的代码改动，仅 `connector.rs`）**：
  - `install_collector_host(&wg_feed)`（`client/core/src/connector.rs:3935`，函数体）在两个启动入口各调用一次：`connector_start_json`（`connector.rs:4005`）与 `connector_start_with_socket_json`（`connector.rs:4132`，N3-7 生产路径）。两处都是 `let wg_feed = Arc::new(WgDeviceFeed::new(..))` 之后立即注册——**每个入口都接线**，因为二者各自都能成为进程内唯一 connector 单例（`connector_slot()`，`connector.rs:3880-3882`）的持有者；只覆盖其一即留下一个「启动了却仍 no-connector」的入口。
  - **重复安装语义 = 后者覆盖（replace）**：`install_host` 返回「是否替换了既有 host」（`n2h_collector.rs:1672-1674`）。理由：一进程一 connector 单例一会话，重启必须把接缝重新指向**新** `WgDeviceFeed`；若保留旧 feed，重启后的每次 arm 都会以 `device-not-up` 失败（旧 feed 的设备已被 `WgDeviceFeed::clear` 摘除，`wg_device.rs:2057-2073`）。
  - **stop 侧卸载**：`connector_stop_json` 在取槽前调用 `n2h_collector::uninstall_host()`（`connector.rs:4564`）。理由：接缝生命周期 = 会话生命周期，`connector_stop()` 之后不再保留任何 connector 状态（feed/config 含 WG 私钥材料的 Arc）；已 ARMED 的窗口自带 `Arc<dyn CollectorHost>` 副本，其已闭合文档仍可供 §2.4-R2 重放（`n2h_collector.rs:2013-2027`），不受卸载影响。
  - **未 arm 零行为影响论证**：安装动作只做 `HOST` 静态槽的 `Mutex::replace`（`n2h_collector.rs:1667-1674`）——不起线程、不挂 tee（tee 只在 `arm_json` 内 `attach_tee`，`n2h_collector.rs:1901`）、不建/不发任何 socket 或包、不读时钟；未 arm 时 `HOST` 从不被查询。装机进程的可观测差异仅两处：arm 被拒的 token 由 `no-connector` 变为 `device-not-up`/`tunnel-not-ready`（会话未就绪时的真实状态），以及 stop 后回到 `no-connector`（`tests/n2h_device_collector.rs:1588` 新增用例逐条断言）。

### 13.2 ② `WgFeedHost` 冻结来源扩展（stuns / management 取自内部状态，不动公开 JSON）

- **事实（缺口）**：`WgFeedHost::freeze_facts()` 原先只读 connector 两个公开文档（`connector_status()` / `connector_network_config()`），二者都不渲染 `netbird_config.stuns[]` 与 management 拨号地址。`stun` 是**必需类**（`n2h.rs:151-153` 的 `required()` 只豁免 turn/relay），无理由缺席会让设备轮恒 `n2h-inconclusive`。
- **取值路径（内部访问器，公开 JSON 形状零改动）**：
  - `stuns[]`：`netbird_config.stuns[].uri` 由 sync 路径送入 ICE 编排器（`connector.rs:1700-1704` → `PeerIceOrchestrator::set_stuns`，`peer_conn.rs:703-711`，解析用 `ice.rs::parse_stun_uri`，`ice.rs:100`），内部读 = `ConnectorShared::stuns()`（`connector.rs:1681-1683`，取 `self.ice` 单例的 `stuns()`，`peer_conn.rs:1362-1364`）。
  - management 拨号地址：壳侧 DNS 解析后经 `connector_start_with_socket(addrJson)` 传入，落在 `GrpcManagementFactory.socket.1`（`connector.rs:572`、`connector.rs:604-618`）；内部读 = 新增 trait 默认方法 `ManagementFactory::dial_address()`（`connector.rs:551-556`，生产实现在 `connector.rs:626-630`），在 spawn 时捕获进 `ConnectorHandle::management_dial`（字段 `connector.rs:3268`，捕获点 `connector.rs:3381`）。
  - 汇合口 = `pub fn core_freeze_inputs()`（`connector.rs:4520-4551`，结构 `CoreFreezeInputs`）→ `WgFeedHost::freeze_facts()`（`n2h_collector.rs:2778-2784`）→ `process_freeze_facts_with_core`（`n2h_collector.rs:2884`）→ `apply_core_freeze_inputs`（`n2h_collector.rs:2998`）。**读取时刻 = arm 时刻**（§4.0「冻结时刻=arm 时刻」）：函数在 arm 路径内被调用（`n2h_collector.rs:1878`），因此冻结的是最后一次 sync 的 stun 集，而非启动时刻的空集。
  - **公开契约不变**：`connector_status_json()`（`connector.rs:4471`）/`connector_network_config_json()`（`connector.rs:4506`）的渲染路径未触碰（无新增字段）；`tests/n2h_device_collector.rs:1588` 的新用例断言这两个文档**不含**内部拨号地址。
  - **缺失 fail-closed**：无 connector ⇒ `CoreFreezeInputs::default()`（空）⇒ `assemble_freezes` 产出 `present:false` + 既有 `absent_reason`（`n2h_collector.rs:530-556` 口径）；不可用值（空 host / port 0 / IPv6——本设计 IPv4-only，§10-4）在 `apply_core_freeze_inputs` 内丢弃，绝不编造。新增用例：`n2h_collector.rs:4503`（`core_freeze_inputs_close_the_stun_and_management_gap`，断言 stun 变为 present 且 `source=core-network-map`、management 为 `source=core-dial-address`，以及无源时两者均为有理由缺席）与 `connector.rs:5197`（内部集只含 `stun:` URI 的实测）。
  - **残余如实登记**：`turn:` URI 被 `set_stuns` 拒绝且内部**无**留存（`peer_conn.rs:703-711`），故可选类 `turn` 保持通用理由「no turn endpoint in this session's network map」（`n2h_collector.rs:532-538`）而非 §4.0 预期的 `TURN_REJECTED_REASON`；属可选类、不影响必需类判定，登记为已知取数层局限。

### 13.3 ③ D4 的 hit 级 direction 改记 `notes[]`（理由＝渲染器冻结）

- **偏离**：§3.4 D4（本文档 :147）原定「hit 对象**尾部追加** `direction:"from-os"|"to-os"`」。实现未追加字段，改记入 `notes[]`。
- **依据**：hit 渲染器 `n2h::TunHit::to_json` 为冻结契约（§3.1 禁增删字段、§5.2 同纪律），追加字段会同时改动设备文档与 host 文档形状；`notes[]` 是 §3.1 明示允许的加法面。代码：`n2h_collector.rs:1402-1410`（`DirectedHit` 头注释与结构）、`n2h_collector.rs:2665-2676`（逐 hit 写 `outer-hit-direction probe_id=… direction=… side=… frame_len=…`）；回归守护 `n2h_collector.rs:4427-4430`（用例断言该 notes 行且断言文档无 direction 字段）。信息量不减：每个泄漏 hit 的方向在 notes 中可判读，正是取证所需。

### 13.4 ④ 新增三个 fail-closed token（`invalid-opts` / `window-failed` / `chunk-out-of-range`）

- 设计 §5.1 只登记了 `no-connector|device-not-up|tunnel-not-ready|already-armed`，实现补齐三个失败语义 token，均为稳定字面、无服务端文本、无秘密：
  - `ARM_ERR_INVALID_OPTS = "invalid-opts"`（`n2h_collector.rs:1340`；调用点 `n2h_collector.rs:1854`，arm opts JSON 解析失败——半懂不解的 arm 请求绝不开窗，`ArmOptions::parse_json`，`n2h_collector.rs:1721-1783`）；
  - `ARM_ERR_WINDOW_FAILED = "window-failed"`（`n2h_collector.rs:1341`；调用点 `n2h_collector.rs:1935`，窗口线程 spawn 失败时同步 detach tee 并清空窗口槽）；
  - `EVIDENCE_ERR_RANGE = "chunk-out-of-range"`（`n2h_collector.rs:1346`；调用点 `n2h_collector.rs:2003`，`n2h_collector_evidence(chunk)` 越界取块）。
- 依据提交：`568c931`（I5）。

### 13.5 ⑤ disarm 语义（已闭合窗保留供重放，只弃未闭合产物）

- 实现口径（`disarm_json`，`n2h_collector.rs:2013-2041`）：窗口已 `closed` ⇒ 只发 hilog 行（`discarded=false|retained=true`）并**保留**已定稿文档，供 §2.4-R2 的 `n2h_collector_evidence(chunk)` 反复重放；窗口仍开 ⇒ take 窗口槽、置 `disarmed`、同步 `detach_tee`、`discarded=true`（半成品弃置），且窗口线程在拍与判定边界检查 `disarmed` 后不再产出定稿（`n2h_collector.rs:2087-2094`、`:2452`）。幂等：恒 `{ok:true}`。
- 与 §2.2「中止：未闭合产物弃置」一致，并明确「重放面不得因 disarm 丢失」，登记为实现期收紧。
- 依据提交：`568c931`（I5）。

### 13.6 ⑥ `collector{}` 块留 I7

- §3.3 的尾部自标识块 `collector{}`（本文档 :131-138）在 I5 **未实现**：文档中 `build_source` 缺省路径只写一条注记 `build-sha256=not-reported (I7 injects the embedded build identity)`（`n2h_collector.rs:2061`），`build.rs` 的 `N2H_BUILD_SHA256` 注入亦未落地——与 §7 表 I7 行（本文档 :296）一致，属计划内留白而非遗漏；本登记仅为避免「I5 已含 A-1 自标识」的误读。
- 依据提交：`568c931`（I5）。

### 13.7 本节改动面与验收（工作树实读）

- 改动文件：`client/core/src/connector.rs`（① 接线 + ② 内部访问器）、`client/core/src/n2h_collector.rs`（② 冻结来源汇合 + 两个单测）、`client/core/tests/n2h_device_collector.rs`（① 端到端接线用例）、本文件（本节）。`connector_status()`/`connector_network_config()` 契约、渲染器 `n2h.rs`、ets 侧均未改动。
- 验收实跑（工作树，HEAD `568c931` + 未提交改动）：
  - `cd client/core && cargo test --offline --locked --color never` → 34 个测试目标全 `ok`，合计 **520 passed / 0 failed / 0 ignored**（基线 517/0 + 本节新增 3 例：`connector::tests::core_freeze_inputs_read_the_sessions_internal_stuns_and_dial_address`、`n2h_collector::tests::core_freeze_inputs_close_the_stun_and_management_gap`、`tests/n2h_device_collector.rs::a_started_connector_installs_the_production_host_with_internal_freeze_sources`）。
  - `bash client/core/build.sh` → exit 0，`14/14 frozen BoringTun ffi symbols present in dynsym`。
  - `bash client/build.sh` → exit 0（ArkTS 编译含 I5 的 ets 改动通过），`entry-default-unsigned.hap` 4 208 659 bytes，sha256 `8ec9b0bb49d860a51fc787a55c61e74ad262d6c1ee15ae9666a9e2e49191dc89`。

### 13.8 ⑦ I6 落地：`nbinterop n2h-peer`（echo + posctl 发送 + 对端状态行）

- **依据**：§7-I6 行（本文档 :295）+ §4.3（正控 a2b/b2a 语义与对端配合）+ §6.4 第 6 条（AUTH 逐字命令面）。基线 HEAD `e6e4f3a`（`git log --oneline -1` 实读）；下列 file:line 均为落地后工作树实读（未 commit）。
- **命令面（落地形态，§6.4 第 6 条已就地补注）**：
  - 活体：`target/debug/nbinterop n2h-peer --config <主机侧对端 config> --overlay <设备 overlay ip[:port]> [--echo-port <port>] [--posctl-interval <ms>] [--interval <ms>] [--timeout <s>]`（`bin/nbinterop.rs:1397` 的 `cmd_engine` 模式分支 `n2h-peer`，`:902` 的 `cmd_n2h_peer` 分派）。
  - 离线自检：`target/debug/nbinterop n2h-peer --loopback [--overlay …] [--echo-port …] [--posctl-interval …] [--timeout <s>]`（§7-I6 的「CLI 单测（loopback 对拍）」的可执行形态：一条 AF_UNIX socketpair 作隧道替身 + 对端假设备线程；`:1221`/`:1257`）。
  - **与 §6.4 示例形态的差异（唯一一处）**：设计示例未含 `--config`，但 echo 与 b2a 双腿都走隧道（对端必须是同一 NetBird 网络的 WG 成员：b2a 要经隧道抵达设备 to-os 侧，echo 要经隧道回设备），且 `unknown_peer_drops` 是**该对端自身 WG 设备**的计数器——故 `--config` 为必需项（与既有 `peer` 子命令同源）。停形态：`--timeout <s>` 到期 → 打印末次状态行并 **exit 0**（`:1721`；该模式无「目标可达」语义，超时即登记停止形态）。
- **默认关闭（§6.1）**：仅 `n2h-peer` 子命令可达；其他子命令带 `--overlay/--echo-port/--posctl-interval/--loopback` 任一 → `cmd_engine` 直接 exit 2（`:1421`）；活体缺 `--overlay` → exit 2，且在**任何** config/拨号动作之前判定（`:1402`；`--overlay` 是设备 overlay 端点，绝不猜）。
- **marker 通道（§4.3 的 b2a 前提）**：启动即生成**一次**稳定 marker `b2a_probe_id=N2H-posctl-<24 hex>`（`n2h::probe_id` + `n2h::fresh_nonce_hex`，与设备 §4.2/I3 完全同形），b2a 探针按 `--posctl-interval`（默认 1000 ms，`bin/nbinterop.rs:399` 的 `N2H_PEER_POSCTL_INTERVAL_MS`）**重复发送同一 marker**——设备侧 `build_posctl` 每窗口只读一次 `peer_report()`（`n2h_collector.rs:2427`），故「一次公告 + 重复发送」是与 I5 观测面对齐的唯一可用语义（每次换 nonce 永远不可能被观测到）。
  - **载体 = stderr 行**（`nbinterop: n2h-peer|…` 公告行 `bin/nbinterop.rs:962` + `nbinterop: n2h-peer-stats|…` 周期状态行 `:992`）。理由：§4.3 只规定「周期状态行由主机侧直接捕获、不经设备」，未指定 stderr/stdout；本 CLI 既有约定是 **stdout 仅放状态 JSONL**（模块文档 :15-28）、诊断行一律 `nbinterop: <token>|<k=v>` 走 stderr（既有 `probe-stats` 同形，`:1907`），故沿 stderr。`n2h-peer-stats` 字段固定为
    `b2a_probe_id|b2a_sent|a2b_seen|echoes_sent|echoes_skipped|last_a2b_from|own_overlay|unknown_peer_drops|unknown_peer_drops_delta`。
  - **残余缺口（如实登记，本次未接线）**：设备侧消费端仍缺——`impl CollectorHost for WgFeedHost`（`n2h_collector.rs:2815`）**未覆写** `peer_report()`，故 I5 的默认空 `PeerReport` 仍是设备轮实际取值：b2a 腿会按 §4.3 记为「对端未公告 marker」缺口、`unknown_peer_drops` 收执不进设备文档。后续增量建议（两小步，均不在 I6 触面内）：① 公告行落一个 hdc 可取文件（或把 marker 并入 arm opts，`bin/nbinterop.rs` 侧已是一行即可加）；② `WgFeedHost::peer_report()` 从该来源读 `b2a_probe_id` 与 `unknown_peer_drops`。本次不改的原因：I6 触面限定 `bin/nbinterop.rs`（+ `n2h_collector.rs` 的必要 pub 暴露），且 §4.3 明示该状态行「不经设备」。
- **`unknown_peer_drops` 的取值路径（本次唯一 pub 面扩张，理由=无其他公开路径）**：`n2h_collector` 新增默认 trait 方法 `CollectorHost::wg_stats() -> Option<WgDeviceStats>`（`n2h_collector.rs:1702`，默认 `None`）；生产实现 `WgFeedHost` 覆写为 `self.feed.with_device(|d| d.stats())`（`:2825`）。CLI 经既有的 `n2h_collector::installed_host()`（connector 启动时由 §13.1 的接线安装）取到 seam 并只读 `unknown_peer_drops`（`bin/nbinterop.rs:1034`）。`None` ⇒ 状态行打印 `unknown_peer_drops=na`（无 connector/设备未起时不猜零）。**不触碰** `connector.rs`/`wg_device.rs`/`n2h.rs`：`connector_status().wg{}` 无该字段且其渲染器是位置读冻结契约（`wg_device.rs:1532-1551`），新增字段会改 `wg` 对象形状。
- **复用 I3 契约（未另造探针形态）**：新 pub `n2h_collector::posctl_payload(id)`（`n2h_collector.rs:1318`；`send_posctl` 改用之，行为逐字不变）与 `posctl_marker(payload) -> Option<&str>`（`:1333`，严格判定 `N2H-posctl-<24 hex>` + `|positive-control`、无尾随字节）。对端**只回显通过该判定的载荷**（`bin/nbinterop.rs:1085`），故该端点不会成为反射器（对拍第 ③ 步即该负例）。
- **收发都在隧道内（无新 socket/端口）**：对端读 `tun_hand` = 隧道交付帧（设备的 a2b 探针），写 `tun_hand` = 注入隧道（b2a 与 echo）；echo = 收到的帧五元组交换、载荷逐字（设备 to-os tee 按 marker 子串匹配，`n2h_collector.rs:2541`）。会话未 ready（`wg.ready=false`）时 b2a 不发（`bin/nbinterop.rs:1661` 传入 `seen.wg_ready`、`:1056` 判定）——不向未建的隧道注入。

- **验收实跑（工作树，HEAD `e6e4f3a` + 未提交改动）**：
  - `cd client/core && cargo build --offline --locked --features cli --bin nbinterop` → exit 0。
  - `cargo test --offline --locked --color never` → 34 个测试目标全 `ok`，合计 **523 passed / 0 failed / 0 ignored**（I5 基线 520/0 + 本次 3 例：`n2h_collector::tests::posctl_payload_and_marker_round_trip_on_the_frozen_wire_shape`、`n2h_collector::tests::the_default_host_exposes_no_wg_counters`、`tests/n2h_device_collector.rs::the_production_host_reports_its_live_wg_counters_for_the_peer_status_line`）。
  - `cargo test --offline --locked --features cli --bin nbinterop` → **6 passed / 0 failed**（bin 为 `required-features = ["cli"]`，默认全量跑不含它）。
  - `bash client/core/build.sh` → exit 0，`14/14 frozen BoringTun ffi symbols present in dynsym`（.so 未受影响：bin 不入 .so）。
  - 离线对拍实跑：`./target/debug/nbinterop n2h-peer --loopback --posctl-interval 300 --timeout 3` → **exit 0**，stdout `{"tool":"nbinterop","check":"n2h-peer-loopback","ok":true,"b2a_observed":true,"a2b_echo_observed":true,"echo_verbatim":true,"echo_tuple_swapped":true,"non_posctl_echo":false,…}`（真实 stderr 状态行样例见 §13.9）。
- **改动文件**：`client/core/src/bin/nbinterop.rs`（子命令 + 活体接线 + 离线对拍 + 6 单测）、`client/core/src/n2h_collector.rs`（`posctl_payload`/`posctl_marker` 两个 pub 函数 + `CollectorHost::wg_stats` 默认方法 + `WgFeedHost` 覆写 + 2 单测）、`client/core/tests/n2h_device_collector.rs`（+1 生产 host 计数器用例）、本文件（§6.4 第 6 条补注 + 本节）。未触碰 `connector.rs`/`napi.rs`/`n2h.rs`/`wg_device.rs`/`host_sockets.rs`/ets。

### 13.9 ⑦ 真实 stderr 样例（I6 离线对拍，逐字）

`./target/debug/nbinterop n2h-peer --loopback --posctl-interval 300 --timeout 3` → exit 0：

```
nbinterop: n2h-peer|b2a_probe_id=N2H-posctl-cea8da680963ce14456c4f6d|b2a_dst=127.0.0.1:40001|echo_port=40001|posctl_interval_ms=300|loopback=true
nbinterop: n2h-peer-stats|b2a_probe_id=N2H-posctl-cea8da680963ce14456c4f6d|b2a_sent=1|a2b_seen=0|echoes_sent=0|echoes_skipped=0|last_a2b_from=na|own_overlay=127.0.0.1:40001|unknown_peer_drops=na|unknown_peer_drops_delta=na
[n2h-peer --loopback] device side observed the b2a probe N2H-posctl-cea8da680963ce14456c4f6d|positive-control (127.0.0.1:40001 -> 127.0.0.1:40001)
[n2h-peer] echoed a2b probe back to 127.0.0.1:40002 (52 bytes, verbatim)
[n2h-peer --loopback] device side received the echo 127.0.0.1:40001 -> 127.0.0.1:40002 (52 bytes, verbatim)
nbinterop: n2h-peer-stats|b2a_probe_id=N2H-posctl-cea8da680963ce14456c4f6d|b2a_sent=2|a2b_seen=1|echoes_sent=1|echoes_skipped=0|last_a2b_from=127.0.0.1:40002|own_overlay=127.0.0.1:40001|unknown_peer_drops=na|unknown_peer_drops_delta=na
[n2h-peer] loopback 对拍: ok=true b2a_observed=true a2b_echo=true verbatim=true tuple_swapped=true non_posctl_echo=false frames_seen=3
```

stdout（同一跑）：`{"tool":"nbinterop","check":"n2h-peer-loopback","ok":true,"b2a_observed":true,"a2b_echo_observed":true,"echo_verbatim":true,"echo_tuple_swapped":true,"non_posctl_echo":false,"frames_seen":3}`

活体运行时同一形状，只是 `loopback=false`、`b2a_dst`/`own_overlay` 为真实 overlay 端点、`unknown_peer_drops` 为对端 WG 设备的真实计数（`na` 仅当设备未起）。

### 13.10 ⑧ 接线 `WgFeedHost::peer_report()`：ops 中转的 b2a 公告通道（本节补齐 §13.8 末条登记的残余缺口）

- **依据**：§4.3（正控 a2b/b2a 与对端配合语义）、§4.0（收执矩阵 wg_peer 行）、§5.1（arm opts 契约）、§13.8 末条「残余缺口（本次未接线）」的后续增量建议②。基线 HEAD `8929695`（`git log --oneline -1` 实读）；下列 file:line 均为本节落地后的工作树实读（未 commit）。
- **缺口（I6 遗留，本节闭合）**：`impl CollectorHost for WgFeedHost`（`n2h_collector.rs:2890`）未覆写 `peer_report()` ⇒ I5 的默认空 `PeerReport`（`:1720-1722`）是设备侧唯一取值 ⇒ `build_posctl` 恒走 `None` 分支（`:2520-2535`）⇒ b2a 腿在设备轮永远是「对端未公告 marker」缺口，而 §4.3 要求双向各至少一条 ⇒ 设备轮正控恒不闭合（与「对端未运行」不可区分）。I6 的公告行由主机侧直接捕获、不经设备（§4.3 末条），因此设备侧**必须**另有一条运维中转的公告载体——这就是本节登记并落地的通道。

**① 通道形态（载体 / 谁写入 / 时序 / 缺失语义）**

- **载体**＝I5 既有的 arm 控制文件 `/data/local/tmp/netbird-n2h-arm`（开发期通道，与 `netbird-protect-deviation` 同目录同类，runbook :158-165；只含非敏感 opts）新增**可选键** `peer_b2a_probe_id`，值为 I6 公告行 `b2a_probe_id=N2H-posctl-<24 hex>`（§13.8/§13.9）的**逐字 id 形态**；`…|positive-control` 载荷形态**不接受**（无第二种形态：判定只有一个权威，见②）。
- **谁写入**＝运维（人）。顺序：先按 §6.4 起的 AUTH 窗口启动主机侧 `nbinterop n2h-peer`，从其 stderr 首行读 `b2a_probe_id`，再把该值写进设备上的控制文件；随后 ets 的 arm 钩子（`NetBirdVpnExtensionAbility.ets:967` `maybeArmN2hCollector`，每 ability 实例至多一次）读文件并在**键存在时**原样并入 `n2h_collector_arm` 的 opts（`:1047-1050`；不 trim、不改写、不做类型宽容），键不存在则**根本不出现**在 opts 里。
- **时序**＝arm 之前必须已写入。窗口的 marker 集在 arm 时固定：`build_posctl` 每窗口只读一次 `peer_report()`（`:2497`），而 I6 的对端对整个运行期重复发送**同一** marker（§13.8「一次公告 + 重复发送」），故一次公告覆盖整窗。控制文件在 arm 尝试之后才写入 ⇒ 该轮按「缺失」处理（ets 侧 `n2hArmAttempted` latch，`:968-970`；窗口不会再开）。
- **端到端链路（全部为既有接缝，无新状态机）**：`ArmOptions::peer_b2a_probe_id`（`:1798`，解析 `:1846-1851`）→ arm 闸门全过 **且** tee attach 成功之后（`:2006-2014`）调 `CollectorHost::note_peer_announcement`（trait 默认空实现 `:1736`）→ `WgFeedHost.peer_announcement`（`:2863`）→ `WgFeedHost::peer_report()`（`:2908-2910`）→ I5 既有消费点：注册 tee marker 并记 `sent:true` 的 b2a 腿（`:2497-2518`）。**缺失时 fail-closed**：`PeerReport::default()` ⇒ b2a `sent:false` + 既有有理由缺口（`:2520-2535` 的 note 已改为指向本通道），未 arm / 未提供 ⇒ 与 I5 逐字相同的行为，verdict 仍 `n2h-inconclusive`。
- **公告只在 arm 时生效、不跨窗口残留**：每次被接受的 arm 都调用一次（空报告＝清除），窗口线程 spawn 失败的分支把公告回收（`:2047`），未通过闸门的 arm 不触碰存储。

**② 校验与 fail-closed（外部输入不引入反射器/注入面）**

- 传入值必须过**同一**严格判定：新增 `announced_b2a_marker`（`:1355`）把 id 拼成冻结 wire 形态后交给 I6 的 `posctl_marker`（`:1333`）——即「公告的 marker」与「对端数据报必须通过的判定」是同一个，公告不可能出现设备无法归因的形态，也不可能长出第二种被接受的形式。
- 不符即**拒绝整个 arm**：`{ok:false,error:"invalid-opts"}`（§13.4 既有 token）。不开窗、不挂 tee、不写任何 host 状态、不登记任何 marker。依据＝模块既有的「半懂不解的 arm 请求绝不开窗」纪律（§13.4）；已实测拒绝形态：载荷形态（带 `|positive-control`）、非 24 hex（22 hex）、非 hex、错 kind、空串、非字符串、尾随字节/空格、JSON 注入形状（值内引号只会成为一个被拒的普通字符串）。
- **缺失 vs 非法的分野**（本节明确）：键缺失 ⇒ 不传 ⇒ b2a 有理由缺口（既有语义，行为不变）；键存在但不合法 ⇒ 拒绝 arm 并如实记因（`arm_err` 只回 token，原因进 hilog `N2H_COLLECTOR_ARM|refused|error=invalid-opts|detail=…`，经 `util::sanitize_marker_field` 转义/截断，`:2159-2165`）。外部值**不进入**证据文档，也不出现在 arm 应答里。
- 该值不驱动任何 socket／不做目的地：只被用作 TUN 扫描器的匹配 marker（`:2500`，`is_posctl=true`，b2a 命中不构成 `n2h-fail`，`:2497-2518`）与文档字段（`PosctlProbe`）；设备不会因它发出任何包。伪造面＝已受信的运维输入（arm 通道本身即信任根，§6.1/§6.4），严格形态判定防的是「随机值/笔误值被登记成扫描 marker」。
- **`napi.rs` 未改**：arm opts 是 JSON-string-in 透传（`napi.rs:611-614`），契约扩展只发生在 core 内的 opts 解析；四个导出的签名与注册表（`napi.rs:119-122`）逐字不变。

**③ 与 §4.3「不经设备」的调和**

§4.3 末条的不经设备条款约束的是**对端周期状态行**（`n2h-peer-stats`：对端计数器与收执）由主机侧直接捕获。本通道不是「设备直读对端状态」：它是运维转抄的**一次性公告**（对端启动即定、运行期不变的 marker id），经开发期控制文件人工流转，与设备→对端的任何网络路径无关。二者不冲突且互补：对端状态行仍不经设备（本节未为它新增任何设备侧通路）；而设备在没有公告时仍然**不知道** marker，也不会猜（`None` 分支即该语义的代码体现）。

**④ `unknown_peer_drops` 归属（核实 §4.0/§4.3 后如实登记，含与 §4.0 wg_peer 行的差异）**

- `CollectorHost::wg_stats()`（I6 新增，默认 `None` `:1746`；生产覆写 `:2900-2902`）＝**读它的那个进程自身** WG 设备的计数器。在设备进程里＝设备自身外层 socket 的 `unknown_peer_drops`（别的主机打到设备的未知名包，设备侧数据）；在主机侧 `nbinterop n2h-peer` 进程里＝**对端自身**的计数（§13.8 的 `n2h-peer-stats` 行即该读数）。
- `PeerReport::unknown_peer_drops` 是**对端**计数，即 §4.0 矩阵 wg_peer 行的 `peer-device-offpath-drop` 收执（证明设备的 `wg_peer` 外层探针到达对端 socket）。它是**活值**：窗口开时取 baseline（`:2193`）、观察期取增量（`:2593`），静态 arm 中转在语义上不可用（baseline ≡ now ⇒ 永不成立）。
- 故本节**只**中转 marker，**不**把对端计数接进设备文档：`WgFeedHost::note_peer_announcement` 显式丢弃该字段（`:2916-2920`），`peer_report().unknown_peer_drops` 恒 `None`，且**绝不以设备自身的 `unknown_peer_drops` 顶替**（两者同名不同量，顶替＝假收执）。分工一句话：**设备文档内的收执面只放设备侧数据；对端自身的计数器留在主机/运维侧状态行（ops 收执）**。
- **与 §4.0 的差异如实登记**：§4.0 wg_peer 行写的是「经其对端状态通道回报」到设备。本增量未为对端计数落任何设备侧载体（缺的是**活**载体，不是本节的静态公告）⇒ 设备文档的 wg_peer 收执仍为有理由缺口，依 `n2h.rs:890-902` 第 5 条，运维收执缺位时设备轮 verdict 仍为 `n2h-inconclusive`（§4.0 :186 的诚实结论不变）。**本节消除的是「正控腿在设备轮恒缺口」这一可用性缺陷**（`n2h.rs:863-885` 第 4 条），不是全部收执缺口；对端计数的设备侧（活）载体或纯 ops 收执路径登记为剩余缺口，留给后续增量。

**⑤ 措辞级改动（非行为）**：`build_posctl` 的 b2a 三条文案由「peer status channel / §4.3's peer-side report is I6」改为指向本通道（`:2517`、`:2522-2523`、`:2530`）——原文案在本增量后已属误导。判据、文档字段、verdict 语义零改动；`connector_status()`/`connector_network_config()`/`n2h.rs` 渲染器未触碰。

**⑥ 测试（新增 4 例集成 + 1 例单测；I0–I6 既有测试逐字未改）**

- ① `tests/n2h_device_collector.rs:1826` `an_announced_peer_marker_from_the_arm_options_closes_the_b2a_leg`：marker 的**唯一**载体＝arm opts（harness 不调 `announce_b2a`）⇒ arm 后 `peer_report().b2a_probe_id` 命中；tee 命中该 marker 的 to-os 帧 ⇒ 文档 b2a 腿 `sent:true`、`observed_in_tunnel` 有 hit（hit note 记 `to-os`）⇒ `tunnel_positive_control.observed_in_tunnel=true`、缺口注记消失、verdict `n2h-pass`。
- ② `…:1899` `an_arm_that_carries_no_peer_marker_keeps_the_b2a_leg_a_reasoned_gap`：键缺失 ⇒ b2a `sent:false`、note 含 `never announced`、`notes[]` 记因、a2b 不受影响、`reasons` 恰一条（`positive control  not sent`）、verdict `n2h-inconclusive`（既有语义回归）。
- ③ `…:1954` `an_invalid_announced_marker_is_refused_without_opening_a_window`：6 种非法形态逐一 `invalid-opts`、不挂 tee、无 open phase、host 状态零写入、seam 存活；随后一次合法 arm 正常开窗（拒绝未 latch）。
- ④ `…:2034` `the_production_host_takes_the_arm_time_peer_announcement_and_keeps_the_counters_apart`：真 `WgDeviceFeed` 上——①空默认 ②经 trait 对象写入（并断言「对端计数字段不被本地值填充」）③无 marker 的 arm 清空（无跨窗口残留）④设备自身 `unknown_peer_drops=1`（`wg_stats`）而对端字段仍 `None`（分工的可测量形态）。
- ⑤ 单测 `n2h_collector::tests::arm_opts_accept_only_a_frozen_shape_peer_announcement`（`n2h_collector.rs:4828`）：冻结形态往返 + 各拒绝形态 + 缺失键。

**⑦ 改动面与验收（工作树实读）**

- 改动文件：`client/core/src/n2h_collector.rs`（`announced_b2a_marker` / `ArmOptions` 新字段与解析 / `note_peer_announcement` 接缝 / `arm_json` 接线与 spawn 失败回收 / `WgFeedHost` 存储与两个覆写 / 三条文案 / 1 单测）、`client/core/tests/n2h_device_collector.rs`（harness 接缝 + 1 辅助 + 4 集成用例）、`client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`（两个 interface 的可选字段 + opts 透传 + 日志字段 `peerB2aProbeId`）、本文件（本节）。**未触碰**：`connector.rs`、`wg_device.rs`、`napi.rs`、`n2h.rs`、`host_sockets.rs`、ets 其余路径、渲染器与判据。
- 验收实跑（工作树，HEAD `8929695` + 未提交改动）：
  - `cd client/core && cargo test --offline --locked --color never` → 34 个测试目标全 `ok`，合计 **528 passed / 0 failed / 0 ignored**（I6 基线 523/0 + 本节 5 例；`tests/n2h_device_collector.rs` 目标 15 passed）。
  - `bash client/core/build.sh` → exit 0，`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`。
  - `bash client/build.sh` → exit 0（ArkTS 编译含本次 ets 改动通过），`entry-default-unsigned.hap` 4 209 559 bytes，sha256 `28ebf6bc9671d86de1d478f165f2a90a057b51de2d2728ac9f05af742ea85cfe`（同一工作树重复打包得到过不同 sha：打包侧非逐字节可复现，验收以 exit 0 + 逐项 VERIFY OK 为准）。
  - `git status --porcelain` 只含上述四个文件（无其他改动）。

### 13.11 ⑨ I7 落地：构建自标识 + `collector{}` 块 + `n2h-decode` + runbook 设备操作章节

- **依据**：§7-I7 行（本文档 :300）、§3.3（A-1 自标识块字段定义）、§3.4-D10（唯一新增差异：尾部 `collector{}`）、§2.4（hilog 分块通道与主机侧重组成交件）、§6.4（AUTH allowed 逐字命令面）、§13.6（`collector{}` 留 I7 的留白与 I5 占位注记）。基线 HEAD `c3f0832`；下列 file:line 均为落地后工作树实读（未 commit）。

**① `N2H_BUILD_SHA256` 口径（可复现优先；对 §3.3 的一处收紧，见 ② 的偏差登记）**

- 注入点：`client/core/build.rs` 新增 `emit_build_identity()`（`build.rs:101`），经 `cargo:rustc-env` 注入 `N2H_BUILD_SHA256` 与 `N2H_BUILD_TARGET`；collector 侧常量 `BUILD_SHA256` / `BUILD_TARGET_TRIPLE`（`n2h_collector.rs:2733` / `:2740`）。每个摘要输入都打 `cargo:rerun-if-changed`（`build.rs:104-106`）+ `rerun-if-env-changed=N2H_BUILD_SHA256`，摘要不可能因跳过 build.rs 而陈旧。
- **两级口径**（优先级从高到低）：
  1. 构建时环境变量 `N2H_BUILD_SHA256` 若为 64-hex ⇒ 原样采用（小写归一）——即 §3.3「由 build.sh 传入 sha256」的接缝，用于把文档绑到某一**具体制品**；给了但**不是** 64-hex ⇒ 构建期 panic（`build.rs:112-118`，不静默忽略）；
  2. 否则**源码摘要**：`sha256("n2h-build-id-v1\n" ++ Σ_{按相对路径字节序} (相对路径 ++ "\n" ++ 文件字节 ++ "\0"))`；输入集固定为 `Cargo.toml`、`Cargo.lock`（存在时）、`build.rs`、`src/**`、`proto/**`（`build.rs:71`/`:76`/`:145-192`）。`tests/**` **不入**（不参与制品字节），`target/**` 永不入。
- **可复现性**：默认口径是**源码的纯函数**——与 host/target/profile/构建目录无关；实测（工作树）：shell 独立实现重算、`client/core/build.sh`（aarch64 release 制品）打印、host 构建（x86_64 debug）文档里 `collector.build_sha256` 三者同为 `3a8e6d72c4f9e73e0b09f164502869b567f6c438b6eab5785d45f3f7f8649820`。
- **离线核对**：`client/core/build.sh` 新增第 7 步（`build.sh:124-138`）以**独立 shell 实现**重算该摘要并 `grep -aq` 制品（命中才 log 通过）；既有链未动：`boringtun-0.7.1.crate sha256 OK` / `Cargo.lock boringtun checksum OK` / `14/14 frozen BoringTun ffi symbols` 全部保留。
- **为什么不是「build.sh 传入 .so/HAP sha」**：① .so/HAP 的 sha 只在制品产出**之后**存在，而 build.rs 在编译期运行 ⇒ 单次构建无法注入（要注入就得两遍构建）；② HAP 打包**不可逐字节复现**（§13.10⑦）⇒ 采用它会把 §7-I7「同源码同摘要」的验收要求直接排除。故默认口径取源码摘要，把「绑定具体制品」交给：环境变量接缝（①）＋ `build_source="operator-reported"` 的 arm opts 回退（§3.3 原文语义）＋ runbook §9.1 的 HAP sha 冻结登记。三者分工由 `build_source` 字段如实标注。
- **偏差登记（§3.3 字面 → 落地形态，两处）**：
  1. §3.3 写 `target_triple` 用「编译期 `env!("TARGET")`」——**不可实现**：`TARGET` 是 build script 变量，crate 编译读不到（实测编译错误：`TARGET may not be available for the current Cargo target`）。落地＝build.rs 转发 `N2H_BUILD_TARGET`（同值同语义），collector 读 `env!("N2H_BUILD_TARGET")`（`n2h_collector.rs:2740`）。
  2. §3.3 的 `build_sha256` 字面口径（build.sh 传入制品 sha）如 ② 所述收紧为「源码摘要为默认、环境变量为接缝」。

**② `collector{}` 块进入设备证据文档（D10，尾部追加）**

- 渲染：`collector_block_json(reported)` / `with_collector_block(document, reported)`（`n2h_collector.rs:2756-2771`）；窗口线程在定稿处调用（`n2h_collector.rs:2311`）。**`n2h.rs` 渲染器逐字未动**——块是拼在 `Evidence::to_json()` 输出尾部（`strip_suffix('}')` + `,"collector":{…}}`），即 §3.1 的尾部追加纪律（先例 a1947b9 的 `wg_peers`），`collector` 是文档**最后一个键**。
- 字段（§3.3 逐字）：`crate_version`＝`env!("CARGO_PKG_VERSION")`、`target_triple`＝`N2H_BUILD_TARGET`、`build_sha256`、`build_source`（`embedded` / `operator-reported`）。
- **`build_sha256` 取值优先级**（如实标注来源，绝不把报告值冒充构建值）：arm opts 的 `build_sha256_reported` 非空 ⇒ 取该值 + `build_source="operator-reported"`（§3.3 的 fallback，用于绑定**实际安装的那个制品**）；否则取内嵌 `BUILD_SHA256` + `"embedded"`。两分支都不可能为空（编译期常量 + build.rs 的 64-hex 断言）。
- **I5 占位注记的移除**（§13.6 预告的收尾）：`run_window` 里两条 `notes[]` 行（`build-sha256=not-reported (I7 injects…)` 与 `build-sha256=<sha> (operator-reported, design §3.3 fallback)`）**删除**——自标识的权威载体现在是 `collector{}`（含来源），`notes[]` 回归「缺口与运行注记」语义（§3.2）。
- **与 host 文档的对齐纪律**：新增顶层键仅 `collector` 一个，且已在 §3.4-D10 登记（无新增差异）；`tests/n2h_device_collector.rs` 的 §8-1 同键同型断言据此学习该**已登记**差异（`d10_collector_paths()`，`:451`；白名单外的 device-only 路径仍 panic），并在场景 1 增断言：四字段取值、64-hex、`build_source="embedded"`、`notes[]` 内无 `build-sha256=` 前缀行、`collector{}` 为文档尾键（`:1114-1132`）。
- **残余缺口（如实登记）**：ets arm 通道**不转发** `build_sha256_reported`（`NetBirdVpnExtensionAbility.ets:1007-1050` 的白名单只有 `posctl_port`/`probe_wait_ms`/§13.10 的 `peer_b2a_probe_id`）⇒ **设备轮实际取值面为 `embedded`**；`operator-reported` 只在直接调用 `napi` 时出现。实际安装 HAP 的 sha256 因此由 runbook §9.1 的冻结件（+ `MANIFEST.sha256`）承担绑定，不由设备文档承担。

**③ `nbinterop n2h-decode`（runbook 可执行性的必需件；§7-I7 行未列，本次新增）**

- 命令面（逐字）：`target/debug/nbinterop n2h-decode <hilog-capture> [--out <file>]`（分派 `bin/nbinterop.rs:558`，实现 `:1009`）。
- 行为：读抓取文件 → 每行**搜索** `N2H_EVIDENCE` 标记并取标记到行尾（hilog 前缀/夹杂行自动跳过，`N2hCapture::read` `:955`）→ 交给**既有的** `n2h_collector::decode_evidence` 重组校验（**未另写解码逻辑**）→ 校验通过才写 `--out`（或 stdout 原始字节）；不通过**绝不落盘**。
- 退出码：`0`＝已校验文档落盘；`21`＝抓取被拒（缺块/摘要不符/重复/拼接/畸形/读失败，`EXIT_N2H_DECODE_FAILED` `:934`，非证据 verdict）；`2`＝用法错误。失败时 stderr 打**缺口明细**：`nbinterop: n2h-decode|ok=false|error=chunk-error|…|lines=<n>|chunk_lines=<n>|total=<n|na>|missing=<seq 列表|na>|detail=<ChunkError>`（`gap_line()` `:984`）+ 一条处置提示行。
- AUTH 面：该子命令属**主机侧**命令，但必须逐字进入当次 AUTH `allowed`（§6.4 新增候选；runbook §9 的抓取步骤依赖它），否则不得执行。

**④ runbook 设备操作章节（只追加）**

- `docs/device-relay-enablement-runbook-20260914.md` 追加 **§9「(C) 设备轮：N2-H 设备侧采集器启用与证据回收」**（`:367` 起，**+182 行，0 删除**；既有 §0–§8 正文逐字未动）。含：§9.1 制品冻结纪律（HAP sha 不可复现 ⇒ 只打包一次、登记**实际安装件**的 sha256、冻结后不得重打包、不得用新包 sha 回填旧证据）、§9.2 构建与登记、§9.3 签名/安装/启动逐字命令、§9.4 主机侧 `nbinterop n2h-peer`（启动命令 + 公告行读法 + `--timeout` 停止形态）、§9.5 控制文件（路径/可选键 `peer_b2a_probe_id` 来源与时序＝**arm 前**写入、非法形态拒绝整个 arm、清理命令）、§9.6 证据抓取与重组（**日志级别设置命令单列逐字**——`hilog -D 0x2900 -b D`，N13 偏差教训；`hilog -r` 清缓冲；抓取 pattern 含 `N2H_EVIDENCE|N2H_COLLECTOR|N2H_|NetBirdVpn|N1BDiscVpn`；`n2h-decode` 重组与成功/失败判据）、§9.7 状态检查（`n2h_collector_status()` 是 napi 导出、shell 不可直接调用 ⇒ 经 `N2H_COLLECTOR_*` hilog 标记读，五种形态逐条列出）与停止条件（引用 §1.0 步骤 0、§3.5、fail-closed 拒绝、缺块重抓失败、零帧/`/proc` 不可读、AUTH `retry_allowed:false`）、§9.8 收尾留档、§9.9 本节新增待确认 ⑨–⑫。

**⑤ 测试（新增 3 单测 + 4 bin 单测；I0–I6b 既有测试逐字未改，仅 §8-1 断言学习 D10）**

- 单测（默认 feature 集）：`n2h_collector::tests::the_collector_block_identifies_the_build_with_the_frozen_shape`（`:4970`）、`…::a_reported_digest_wins_and_is_labelled_operator_reported`（`:4997`）、`…::the_collector_block_is_tail_appended_to_the_frozen_renderer_output`（`:5017`）。
- CLI 单测（`--features cli --bin nbinterop`，新模块 `n2h_decode_tests` `:2485`）：往返（真 `encode_evidence` 产物 + hilog 前缀 + 夹杂行 → 字节一致）、缺块（`Missing{seq}` + `missing=` 明细）、篡改（`Sha256Mismatch`）、用法闸门（无参/未知旗标 ⇒ 2；不可读抓取 ⇒ 21）。
- 集成断言更新（唯一改动）：`tests/n2h_device_collector.rs` 的 `assert_same_schema` 增加 D10 白名单 + 场景 1 的 `collector{}`/`notes[]` 断言（见 ②）。

**⑥ 改动面与验收（工作树实读）**

- 改动文件：`client/core/build.rs`（+`emit_build_identity`/`build_inputs`/`source_digest`/`sha256_hex`）、`client/core/build.sh`（+第 7 步自标识核对）、`client/core/src/n2h_collector.rs`（`collector{}` 三常量 + 两函数 + `run_window` 接线与占位删除 + 3 单测）、`client/core/src/bin/nbinterop.rs`（`n2h-decode` 子命令 + 4 单测 + 模块文档/USAGE/退出码）、`client/core/tests/n2h_device_collector.rs`（D10 容忍 + 场景 1 断言）、`docs/device-relay-enablement-runbook-20260914.md`（§9 追加）、本文件（本节）。**未触碰**：`n2h.rs`（渲染器与判据）、`connector.rs`、`wg_device.rs`、`napi.rs`、`host_sockets.rs`、ets、`Cargo.toml`/`Cargo.lock`（无新依赖）、`.cargo/config.toml`。
- 验收实跑（工作树，HEAD `c3f0832` + 未提交改动）：
  - `cd client/core && cargo test --offline --locked --color never` → 34 个测试目标全 `ok`，合计 **531 passed / 0 failed / 0 ignored**（I6b 基线 528/0 + 本节 3 单测；`tests/n2h_device_collector.rs` 目标仍 15 passed）。
  - `cargo test --offline --locked --color never --features cli --bin nbinterop` → **10 passed / 0 failed**（I6 的 6 例 + 本节 4 例）。
  - `cargo build --offline --locked --features cli --bin nbinterop` → exit 0。
  - `bash client/core/build.sh` → exit 0；`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`；`boringtun-0.7.1.crate sha256 OK` 与 `Cargo.lock boringtun checksum OK` 两条既有校验仍在；末行 `build self-identification N2H_BUILD_SHA256=3a8e6d72c4f9e73e0b09f164502869b567f6c438b6eab5785d45f3f7f8649820 (recomputed independently; present in the artifact)`。
  - `bash client/build.sh` → exit 0（逐项 `VERIFY OK`），`entry-default-unsigned.hap` 4 204 623 bytes，sha256 `329984020c57d4b2871888d40ec3d1dccbd2524372026fae71d14a55dca477f3`（与 §13.10⑦ 记录值不同，符合「打包不可逐字节复现」；验收以 exit 0 + 逐项 VERIFY OK 为准）。
  - `n2h-decode` 三类用例（真跑，输入＝集成测试 `--nocapture` 的真实 hilog 抓取 87 行/19 块）：①往返 exit 0，落盘文档 sha256 `f2b27795370b78409684daa31a802148cbb442c7dddce81292b77e2baeb1576b` 与全部分块行声明值逐字相同，文档尾键为 `collector`（`build_sha256=3a8e6d72…`、`build_source="embedded"`，`notes[]` 无 `build-sha256=` 行）；②抽掉 `seq=7/19` 一行 ⇒ exit **21**、`chunk_lines=18|total=19|missing=7|detail=missing chunk seq 7`、**未落盘**；③翻转 `seq=11/19` 的一个 b64 字符 ⇒ exit **21**、`detail=reassembled document sha256 591a9287… does not match the declared f2b27795…`、**未落盘**。
  - `git status --porcelain` 只含上述 7 个文件（无其他改动）。

