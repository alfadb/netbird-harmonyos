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
- `exclusion_generation`（**I3a 更正**：以上四条为 I3a 之前的字段清单，按只追加纪律**逐条保留为历史引述**；I3a 起**在该块尾部追加**第五条，既有四条的字段名/顺序/位置**不动**——块仍是文档最后一个键、仍可被严格 reader 解析、`n2h::Evidence::to_json` 仍未被触碰）：arm opts 的 `exclusion_generation` 原值（§5.1 契约行，预注册 v3.1 §2.1 R-2①）——值＝arm 时刻**最后一次被接受**的 `create()` 所建立连接代的 `gen`；**缺键**（壳侧尚无被接受的代，如实省略、不编造）⇒ 渲染 `null`（字段恒在、值缺失＝判定侧三方 `gen` 比对缺口，C2 不成立方向）。落点与判定见 §13.14①②。

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
| D10 | 尾部 `collector{}` | 无 | 有（§3.3）（**I3a 更正**：该块现含**五**字段——§3.3 原四条 + I3a 尾部追加的 `exclusion_generation`，既有四条的字段名/顺序/位置不动；此处旧「（§3.3）」指代不变，字段清单以 §3.3/§13.14① 为准） | A-1 |

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
| stun | core network map `netbird_config.stuns[].uri`（解析 `stun:host[:port]`，`ice.rs:100`；map 解码 `network_map.rs:617`） | **（I2 更正）** 原备注 `壳排除集目前**不含**此类（B-2 的 3/7 现状，如实入档）` **自 §13.13 起失效**：壳排除集现有 `kind=stun`（core 侧来源同前；壳侧经 `connector_network_config()` 尾部 `stuns:[{uri}]` 暴露 ⇒ `parseStunUri` 镜像 `stun:` 解析（默认 3478）⇒ `/32` 排除条目 ⇒ arm 为 `shell-exclusion` 源，`turn:` 以具名理由拒绝且不生成路由）。壳侧排除派生 kind 现状＝**5/7**：management/signal/relay/dns（§13.12）/stun（本文），仅余 **wg_peer** 为缺口（独立增量）。实现与验收见 §13.13 |
| turn | 同上来源；当前实现**拒绝** `turn:` URI（`peer_conn.rs:701`；host 侧同拒 `host_sockets.rs:1187-1188`） | 预期常态=`present:false` + `absent_reason:"turn: URI rejected by this build (no turn path)"`（可选类带理由缺席，非缺口，`n2h.rs:151-153`） |
| wg_peer | `connector_status().wg.wg_peers[].endpoint`（`wg_device.rs:298,:1386`；a1947b9 尾部追加面） | ICE 选中对外 UDP 端点；无会话时 fail-closed 不 arm |
| dns | 壳侧平台默认（物理）网络 DNS（`@ohos.net.connection`：`getDefaultNetSync` → `getNetCapabilitiesSync` → `getConnectionPropertiesSync(...).dnses[]`；步骤 2 不成立时才走**唯一**的 `getAllNetsSync` 过滤分支——v3.1 R-4 分支序）∪ core network map `dns.servers[{ip,port}]` 中**不落在** `address/address_prefix_len` 或任一 `routes[].network` 覆盖范围内的条目（`connector.rs:2537-2547`；`network_map.rs:84,:329`） | **v3.1 更正（本行原「`dns.servers[]`+`interface_dns`」按重审 R-4/预注册 :68 作废）**：`interface_dns` 与 overlay 内 server **一律不入**冻结集，也不入排除集（把 overlay 内解析器 host-route 排除＝把隧道内 DNS 踢出隧道、新造物理明网泄漏面）；`notes[]` 记 `dns-overlay-internal-excluded-from-freeze=<n>`（n＝落入覆盖范围的 `dns.servers[]` 条数）与 `dns-interface-dns-excluded-from-freeze=1`。非法/无端口/IPv6/空的 server 计入 dns 必需类缺口并逐条记 `dns-server-unusable=ip=…\|port=…\|reason=…`＋`dns-freeze-gap=unusable-servers=<n>`，不得静默跳过（R-3 严格 IPv4，端口缺失按 53；IPv6 不入集合）。壳侧分支记录逐字入 `notes[]` 与 `N2H_DNS_BRANCH\|` 标记：`dns-branch=default\|all-nets-filtered\|none`、`dns-physical-evidence=<handle=…;networkCap-has-NOT_VPN=15;bearerTypes-no-BEARER_VPN=4>`、缺口时 `dns-gap=<token>`；分支记录缺失即缺口。本轮 fatal 判定不扩大（仍只 relay 必需类）。权限＝`ohos.permission.GET_NETWORK_INFO`（normal/system_grant，`module.json5`）。实现与验收见 §13.12 |

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
| `n2h_collector_arm(optsJson: string)` | opts：`{"shell_exclusions":[{kind,host,port,resolved}], "posctl_port":40001, "probe_wait_ms":800, "build_sha256_reported":"…"（可选）, "dns":{"branch":"default\|all-nets-filtered\|none","physical_evidence":"<逐字依据>","gap_reason":"<token>"（可选）}}`（**v3.1 C1 dns 追加**：`dns` 段是壳侧分支记录，`branch` 走白名单、两个文本字段限可打印 ASCII 且不含 `\|`/空白/≤96 字符，非此即整臂拒绝 `invalid-opts`；缺段＝文档无 `dns-branch=` 行＝判定侧的 dns 缺口，**不得**由 core 编造。见 §13.12）（**I3a 更正**：以上 opts 字面为 I3a 之前形态，按只追加纪律**逐字保留为历史引述**；I3a 起在该字面**尾部追加** `"exclusion_generation":<n>`（可选，非负整数）——现形态＝`{…,"dns":{…},"exclusion_generation":<n>}`：值＝arm 时刻**最后一次被接受**的 `create()` 所建立连接代的 `gen`（壳侧单字段 `n2hAcceptedExclusionGeneration`，与 `N2H_COLLECTOR_ARMED` 行的 `gen=` 同源，见 §13.14①），随证据文档尾部 `collector{}` 块输出为 `collector.exclusion_generation`（§3.3；判定＝该字段、ARMED 行 `gen`、最后一次被接受的 `VPN_EXCLUSION_GENERATION` 行三方**逐字相同**，预注册 v3.1 §2.1 R-2③）；**缺键**＝壳侧尚无被接受的代（如实省略、**不编造**）⇒ core 记 `null`（字段恒在、值缺失），即判定侧的三方比对缺口（C2 不成立方向）；**类型不符**（负数/字符串/`null`/数组/对象）⇒ 整臂拒绝 `invalid-opts`。见 §13.14①②） | `{ok:true,armed:true,opened_mono} \| {ok:false,error:"no-connector"\|"device-not-up"\|"tunnel-not-ready"\|"already-armed"}` |
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

### 13.12 ⑩ I1 增量落地：dns 类按预注册 v3.1 判据（`docs/n2h-device-evidence-preregistration-draft-20260917.md` §2-C1 dns 口径 :68 / §2.1 R-3 :112 / R-4）

> 本节只追加，不改写 §0–§13.11；**唯一例外＝§4.0 冻结来源表的 dns 行与 §5.1 的 arm opts 契约行**（本增量按 v3.1 更正/扩展这两行的字面，已在原处就地更新并标注 v3.1）。基线：工作树 HEAD `c22cdc7`（干净）+ 本节未提交改动；下列 file:line 为落地后工作树实读。

**① core 侧：dns 冻结源更正（`client/core/src/n2h_collector.rs`）**

- `process_freeze_facts_with_core` 的 dns 段重写（原 `interface_dns:53` 强 push 已删）：新增 `OverlayRange`（`a.b.c.d/p`，host 位按掩码取，`address` 是**主机地址带前缀**故掩码是口径不是修补）＋ `overlay_ranges()`（`address/address_prefix_len`，前缀缺失按 /32；∪ 每条 `routes[].network`）。
- **`interface_dns` 移出冻结集**（永不入集合/排除集）；`dns.servers[]` 中落入覆盖范围的条目**排除**并计数；其余条目按 R-3 严格 IPv4 `host:port` 入集合（端口缺失/为 0、IPv6、空、非点分四段、元素非对象 ⇒ 计入 dns 必需类缺口）。
- `notes[]` 键（新增）：`dns-overlay-internal-excluded-from-freeze=<n>`（n＝落入覆盖范围的 `dns.servers[]` 条数；**恒输出**，证明过滤器已跑）、`dns-interface-dns-excluded-from-freeze=1`（`interface_dns` 非空时；把 `interface_dns` 也计入同一 `n` 会与 :68 的字面「map `dns.servers[]` 中落在覆盖范围内的条目…计数」冲突，故拆成两条，二者都不静默丢弃）、`dns-server-unusable=ip=…|port=…|reason=<empty-ip|ipv6|no-port|not-strict-ipv4-socketaddr|malformed-entry>`（逐条）、`dns-freeze-gap=unusable-servers=<n>`（汇总，n>0 时）。
- 承载：`HostFreezeFacts` 新增 `notes: Vec<String>`，`arm_json` 把它与壳侧分支记录一起并入窗内 `arm_notes`（→ 证据文档 `notes[]`）。dns 全空时 `absent_reason` 改为「no dns endpoint: the network map has no dns.servers[] entry outside the overlay's own coverage…」（旧文案把 `interface_dns` 当冻结源，已按 v3.1 更正）。
- **壳侧分支记录通道**：`ArmOptions` 新增 `dns: Option<ShellDnsReport>`（`{branch, physical_evidence, gap_reason?}`）——`branch` 走白名单 `default|all-nets-filtered|none`，两文本字段限「可打印 ASCII、无空白、无 `|`、1..=96 字符」，任一不符**整臂拒绝**（`invalid-opts`，与既有选项同一 fail-closed 纪律；`|` 被拒是因为它同时是 notes 字段分隔符，可伪造第二条注记）。渲染为 `dns-branch=…`、`dns-physical-evidence=…`、`dns-gap=…`（后者仅在有缺口时）。**缺段＝无 `dns-branch=` 行**（判定侧的 dns 缺口），core 不编造。

**② 壳侧：dns 端点派生（v3.1 R-4 唯一分支序）**

- 纯模块 `NetBirdEndpointExclusion.ets` 新增：`NET_CAPABILITY_NOT_VPN=15`/`BEARER_VPN=4`/`NET_FAMILY_*`/`DNS_RESOLVER_PORT_DEFAULT=53`、`isPhysicalNetEvidence()`（要求 NOT_VPN **显式存在**且 `bearerTypes` **不含** BEARER_VPN；缺证据≠证据）、`deriveDnsEndpointExclusions(input)`——**步骤 1** 取 default handle（netId 0＝无）；**步骤 2** 证据成立才读该 handle 的 `dnses[]`（此路径下 `getAllNetsSync` 根本不被调用）；**步骤 3** 仅当步骤 2 不成立才走唯一 all-nets 过滤分支（同谓词逐 handle）；**步骤 4** 错误/空值/无物理归属 ⇒ 记录的 dns 缺口（`api-error[-code-<n>]` / `no-physical-net` / `dnses-empty` / `dnses-not-ipv4`）。纯函数对「同时给了 default 与 all-nets」的输入仍只取 default ⇒ **结构性禁止**测量后择优。
- 平台取数在 `NetBirdConnector.ets` 的 `derivePlatformDnsEndpoints()`（唯一调用 `@ohos.net.connection` 四个同步 API 处；`readDnsResolvers()` 逐 handle 读 `properties.dnses[]`，读失败 `propertiesOk=false` ⇒ 缺口；错误 token 只取 `BusinessError.code`，不进错误文本）。
- 接线（`NetBirdVpnExtensionAbility.deriveEndpointExclusions`）：dns 端点并入 `endpoints[]`（⇒ 进排除集、进保留的壳排除集、随臂交给 core），IPv4 字面量在 fresh-resolution 处**短路**（`resolveExclusionEndpointIp` 的字面量分支，无 DNS leg）；分支记录存 `this.n2hDnsReport` 并在 arm 时逐字进 `opts.dns`；同时输出标记 `N2H_DNS_BRANCH|requestId=…|dns-branch=…|dns-physical-evidence=…|dnsResolvers=<n>|dnsGap=<token|none>|diag=true|is_evidence=false`（与文档 `notes[]` 同一逐字 token）。
- **本轮不扩大 fatal**：dns 缺口只记录（不 abort），fail-closed 五类必需端点扩面属后续增量；`VPN_DEFAULT_ROUTE_FORCED`、`gen=`/代际行/ARMED 字段/S20 均未触碰。
- 权限：`client/entry/src/main/module.json5` 的 `requestPermissions` 新增 `ohos.permission.GET_NETWORK_INFO`（注释写明用途＝四个同步网络 API、级别 normal、grantMode system_grant/API 8，与 INTERNET 同级同 mode，无需 ACL）。打包后 HAP `module.json` 实读：`requestPermissions` 含 INTERNET + GET_NETWORK_INFO 两项。

**③ selfcheck 扩展（host-only，无设备/无网络）**

- 三处 `@ohos.net.connection` 替身全部升级为**功能性** stub（新增四 API，值由 `globalThis.__NB_NET_DEFAULT_ID/__NB_NET_ALL/__NB_NET_CAPS/__NB_NET_DNSES/__NB_NET_THROW` 注入；未注入时 default netId=0 ⇒ 无端点+缺口，故既有 R/A 系列断言逐字不变）：`relay-exclusion-hooks.mjs`、`relay-flag-hooks.mjs`、`route-check-hooks.mjs`。因 `NetBirdConnector.ets` 新增对 `./NetBirdEndpointExclusion` 的**值导入**，`run-relay-flag.mts`/`run-relay-status-log.mts` 同步 stage 该真实文件＋`NetBirdEndpointExclusion.dep.ts`（`relay-flag-hooks.mjs` 增加该 specifier 的重定向），`run-relay-exclusion.mts`/`run-exclusion-addr-shape.mts` 的 `SHIM_TYPES` 补 4 个被类型剥离擦除的接口名。
- `run.mts`（纯模块）：+18 检查（常量字面、谓词「必须显式 NOT_VPN」、D1 default 成功/D2 无 NOT_VPN 走 all-nets/D2b 含 BEARER_VPN 同样降级/D3 两路皆败 ⇒ `no-physical-net`/D4 空 dnses ⇒ `dnses-empty`/D5 IPv6 ⇒ `dnses-not-ipv4`（可用 IPv4 仍入集合）/D6 API 错误 token 逐字/D7 读属性失败 ⇒ `api-error`/D7b 无合格网络 ⇒ `no-physical-net`）。
- `run-relay-exclusion.mts`（真扩展类端到端）：+10 检查（D1 default 分支 `/32` 入排除集＋标记逐字＋保留的壳排除集含 kind=dns＋`n2hDnsReport` 无 gap；D2 步骤 2 不成立走 all-nets 且 default 的解析器**不**入集；D3 无物理网络 ⇒ 无 dns 路由＋`dns-branch=none|dns-gap=no-physical-net`；D4 default 与 all-nets 同时合格时 default 胜（固定序）；D5 API 错误 ⇒ `dnsGap=api-error-code-2100002` 且 all-nets 解析器仍入集；2 条 arm 转发/标记格式的逐字接线 pin——该跳需原生 arm，host 侧不可驱动，沿用 `run-exclusion-addr-shape.mts` A5 的文本 pin 先例）。
- 其余 selfcheck（`run-relay-flag.mts`/`run-relay-status-log.mts`/`run-route-validation.mts`/`run-exclusion-addr-shape.mts`）逐字未改其断言，仅 stage 变更。

**④ core 测试（新增 2 内联单测 + 1 集成用例，改 2 断言）**

- 新增：`n2h_collector::tests::dns_freeze_caliber_v31_filters_overlay_internal_and_records_unusable_entries`（覆盖 address/20 + 两条 routes 覆盖过滤、`interface_dns` 排除、IPv6/无端口/空/非点分/非对象五种不可用逐条留痕、`dns-freeze-gap=unusable-servers=5`、全内网 ⇒ dns 仍有理由缺席且仍为必需类）；`n2h_collector::tests::arm_opts_carry_the_dns_branch_record_into_the_evidence_notes`（合法记录解析＋三行注记逐字＋8 种非法记录整臂拒绝＋缺段无注记）；集成 `dns_freeze_caliber_v31_holds_through_the_public_document_mapping`（从公开 `process_freeze_facts` + `assemble_freezes` 侧钉同一口径，含「内网解析器不得出现在冻结集任何条目」断言）。
- 更新：`process_freeze_facts_map_both_connector_documents`（dns 由 2 条改 1 条：`interface_dns` 出集＋两条注记断言）、`core_freeze_inputs_close_the_stun_and_management_gap`（`frozen.len()` 9→8）；集成 `offline_facts` 补 `notes` 字段、`offline_opts` 增加 `dns` 段、场景 1 增三条 `notes[]` 断言（**壳侧分支记录确实抵达证据文档**）。

**⑤ 改动面与验收（工作树实读，HEAD `c22cdc7` + 本节未提交改动）**

- 改动文件（15）：`client/core/src/n2h_collector.rs`、`client/core/tests/n2h_device_collector.rs`、`client/entry/src/main/ets/vpnextensionability/{NetBirdEndpointExclusion,NetBirdConnector,NetBirdVpnExtensionAbility}.ets`、`client/entry/src/main/module.json5`、`client/entry/selfcheck/{relay-exclusion-hooks.mjs,relay-flag-hooks.mjs,route-check-hooks.mjs,run.mts,run-relay-exclusion.mts,run-relay-flag.mts,run-relay-status-log.mts,run-exclusion-addr-shape.mts}`、本文件。**未触碰**：`n2h.rs`（渲染器与判据）、`connector.rs`（公开 JSON 形状）、`napi.rs`、`wg_device.rs`、预注册草案、T0 审查记录、`Cargo.toml`/`Cargo.lock`（无新依赖）。
- 验收实跑：
  - `cd client/core && cargo test --offline --locked --color never` → **34 个测试目标全 ok，合计 534 passed / 0 failed / 0 ignored**（`c22cdc7` 基线 531/0 + 本节 3 新用例）；`tests/n2h_device_collector.rs` 目标 16 passed（含新集成用例），冻结类断言按新口径全绿。
  - `bash client/core/build.sh` → exit 0；`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`；末行 `build self-identification N2H_BUILD_SHA256=72bc6e32b7dcdbab20f3185228d7bbbe493c820f70339648a9f96c85584e69b0 (recomputed independently; present in the artifact)`（源码改动 ⇒ 与 §13.11 的 `3a8e6d72…` 不同，符合该摘要「任何源码改动都会改变它」的语义）。
  - 壳侧自检（全 `exit 0`）：`run.mts` **61 passed / 0 failed**（基线 43）、`run-relay-exclusion.mts` **35 / 0**（基线 25）、`run-relay-flag.mts` **9 / 0**、`run-relay-status-log.mts` **3 / 0**、`run-route-validation.mts` **40 / 0**、`run-exclusion-addr-shape.mts` **25 / 0**。
  - `bash client/build.sh` → exit 0（`CompileArkTS` 对含本节改动的 ets 全量编译成功，仅余既有行告警），逐项 `VERIFY OK`（HAP 成员 `.so` 与 llvm-strip 后制品 sha256 相同、ELF64 ET_DYN AArch64、`pack.info` apiVersion 26）；HAP `entry-default-unsigned.hap` **4 216 311 bytes**、sha256 `ad5f3383b823496c97ef6d4cb01c032e32c6b579535fdadcc4ae336bc83f94c3`（打包不可逐字节复现，验收以 exit 0 + 逐项 VERIFY OK 为准）。
  - 残余（如实）：dns 缺口的 fatal 化、壳侧 stun/wg_peer 排除面、代际标记与 S20 仍属后续增量；本轮 dns 缺口只入 `notes[]` 与标记，判定侧据「分支记录缺失/`dns-gap=` 存在」即可判 C1 不成立——该判定接线未在本轮实现。

### 13.13 ⑪ I2 增量落地：stun 类壳侧暴露通道（形态 A 尾部追加）＋壳侧 `stun:` 解析与 /32 排除；turn 缺席形态核对

> 本节以**追加**为主，不改写 §0–§13.12；**唯一例外＝§4.0 冻结来源表的 stun 行**（本增量按 I2 现状就地更正该行备注并保留旧文本为历史引述，见 ⑦ 与 §4.0 行内标注），§5.1/§5.2 及其余各行**未触碰**。基线：工作树 HEAD `9460224`（干净，I1 已合入）+ 本节未提交改动；下列 file:line 为落地后工作树实读。

**① 暴露形态选择：形态 A（`connector_network_config()` 尾部追加），理由**

- 缺口＝**排除面缺 stun**：冻结面已由 core 内部提供（§13.2：`ConnectorShared::stuns()` `connector.rs:1681-1683` → `CoreFreezeInputs.stuns` `:4528` → `apply_core_freeze_inputs` `n2h_collector.rs:3446-3451`，`source=core-network-map`），但 `connector_network_config()` 不渲染 `netbird_config.stuns[]`，壳侧无从派生 stun 的 /32 排除路由（预注册 §2-C2 B-2 的排除面缺口）。
- **选 A 而非 B**：B（新增 napi 导出）要多两处注册 + lib.rs 表行 + 导出形状测试，且引入「同一个 `napi_init` 表里两个只读快照」的重复面；A 复用壳侧已读文档（`NetBirdConnector.ets:1059-1061` 的 `connectorNetworkConfig()` 已是 `net.signal` 的既有通道，stun 与 signal 同源、同 sync、同读取时刻），零新增导出面。A 亦有**尾部追加先例**：`wg_peers`（commit `a1947b9`）、`relay_advertised`（commit `95f6c02`）。
- **形状**：`{available:true,…,default_route:{allowed,reason},stuns:[{uri}]}` —— `stuns` 是**最后**一个字段（既有字段顺序与形状零改动；测试 pin `json.ends_with(",\"stuns\":[]}")`）。元素用 `{"uri":"…"}` 对象形态（与 wire 面 `stuns[].uri`、§4.0 stun 行的来源字面一致），值为 `netbird_config.stuns[].uri` **逐字**，**不预过滤**：`turn:` URI 也要如实到达壳侧，由壳侧按具名理由拒绝（否则「拒绝」这件事在壳侧不可观测）。空数组＝「本次会话没有广告」的如实缺口，**绝不编造**端点；无秘密材料（stun URI 是公开路由材料，与 core 自身对 relay url 的逐字 hilog 同级）。
- **纪律**：`connector_status()` 形状**未触碰**（§5.2 仍成立）；`connector_network_config()` 的 `available:false` 答案逐字节不变（集成测试 `tests/n2h_device_collector.rs:1827-1831` pin）——stun 段只存在于 `available:true` 的尾部，没有 map 就**没有** stun 段（不伪造）。
- 读取时刻语义：与 `ConnectorShared::stuns()` 同规——**带 config 的 update 覆盖**（空 stuns 也覆盖＝如实清空），**不带 config 的 update 保留上一次公告**（`connector.rs:1700-1712`/`:1784-1789` 快照 stamping，`:1854` 落盘）；因此壳侧看到的列表与 core 实际 gather 的集合同源同变。

**② 壳侧解析与排除实现要点**

- 纯模块 `NetBirdEndpointExclusion.ets`：`STUN_DEFAULT_PORT=3478`（:638）、`TURN_REJECTED_REASON='turn-rejected:no-turn-path-in-this-build'`（:643）、逐条拒绝 token（:647-653）、缺口 token `no-stuns`/`unusable-uris`（:659-660）、`parseStunUri`（:701-757）、`deriveStunEndpointExclusions`（:763-816；结果含 `announced`＝公告元素数，调用方不自行读原始输入的 `length`）。
- `parseStunUri` **镜像 core `ice.rs:100-143` 的 stun 分支**：首 `:` 切 scheme（大小写不敏感，`stun` 才收）、末 `:` 切 host/port、省略端口⇒3478、端口按 Rust `u16` 语义（接受 1 个前导 `+`、拒空/非数字/超 65535、0 单列 `port-zero`）、拒 `[...]` 括号 IPv6（core udp4-only）。镜像刻意**不比 core 更严**（更严会漏掉 core 实际 gather 的端点 ⇒ C2 包含关系出现空洞）。
- **唯一一处比 core 更严（已登记）**：切分后 host 仍含 `:`（如 `stun:a:b:3478`，core 会接受 host `a:b`）⇒ 记 `unusable-host` 拒绝。理由＝该 host 既非 IPv4 字面量也非可解析域名，若照抄会经 `resolveExclusionEndpointIp`（含 `:`⇒按括号 IPv6 原样返回）落到 `classifyAddress` 的 `ipv6` 分支，生成一条目的地非法的 /128 条目（2026-09-15 create-401 的同一失效类）；core 对该 host 的 gather 同样必然失败，故不构成 C2 空洞。
- `deriveStunEndpointExclusions(stuns)`：缺失/`null`/空/非数组/异常元素 ⇒ 缺口 token（`no-stuns` / `unusable-uris`）＋逐条 `N2H_STUN_URI_REJECTED|…|reason=<token>`，**永不**静默丢弃、**永不**编造。可用条目去重（按 host:port）后产出 `{kind:'stun',host,port}`。
- 接线（`NetBirdVpnExtensionAbility.deriveEndpointExclusions`，第 5 实参＝`net.stuns`，调用点 `:616`；方法头 `:844-846`）：stun 端点**并入同一 `endpoints[]`** ⇒ ① 走既有的 fresh-resolution（IPv4 字面量在 `resolveExclusionEndpointIp` 短路、无 DNS leg）→ `buildExcludedRouteEntries` ⇒ `/32` `isExcludedRoute:true` 条目；② 计入 `VPN_ENDPOINT_EXCLUSIONS|…|hosts=|resolved=|routes=`（该行格式**未改**：stun 只是多一条端点）；③ 留存 `n2hExclusionEndpoints`/`n2hExclusionResolutions` ⇒ arm 时按 kind='stun' 作为 **`shell-exclusion` 源**交给 core（kind token 与 `n2h.rs:120-130` 的 `EndpointKind::name()` 一致，`assemble_freezes` 对该 kind 消费壳条目，`n2h_collector.rs:509-513`/`:587-590`）。
- 标记（`diag=true|is_evidence=false`，**不进**证据 `notes[]`）：`N2H_STUN_ENDPOINTS|…|stuns=|usable=|rejected=|turnRejected=|stunGap=<token|none>`（:987-994）与逐条 `N2H_STUN_URI_REJECTED|…|index=|reason=`（:981-986）。**arm opts 契约（§5.1）本轮不动**：壳侧 stun 缺口只入 hilog 标记，不新增 opts 段（dns 那套 `opts.dns` 记录通道本轮不复制）。
- **不扩大 fatal**：`isExclusionFailClosed(build, relayRequired)` 判据逐字未改（stun 端点进了同一 `failed` 列表，故与 dns 一样落进既有的「relay 必需时的未解析端点」规则，属既有规则、非新增致命面）；五类必需端点的 fatal 扩面仍是后续增量（预注册 §2-C2 fail-closed 触发面）。

**③ turn 缺席形态核对（无代码改动，仅登记核对结论）**

- **core 侧已满足**：`TURN_REJECTED_REASON`（`n2h_collector.rs:378`）＝`"turn: URI rejected by this build (no turn path)"`；`absent_reason(EndpointKind::Turn)` 在 `turn_uris` 非空时给出该具名理由，否则给「no turn endpoint in this session's network map (optional kind)」（`:534-540`）；`assemble_freezes` 对 turn **恒定**产出 `present:false` + `absent_reason`（`:578-585`，注释逐字「turn has no freeze path in this build」），`core_candidates(Turn)` 恒空（`:514`），即**永不生成 turn 路由/冻结条目**。对应既有测试：`:3739`（注入 turn 壳排除仍得具名缺席）。
- **壳侧本轮补齐具名理由**：`turn:` URI 若出现在被暴露的 `stuns[]` 里（本 build 的 `set_stuns` 会拒它、内部无留存，故常态不会出现；暴露面逐字转发 ⇒ 一旦出现即可观测），壳侧以 `TURN_REJECTED_REASON` 拒绝、**不生成任何 turn 路由**（自检 S4：DNS 表故意为该 host 备了答案，仍无路由）。

**④ selfcheck 扩展（host-only，无设备/无网络）**

- `run.mts`（纯模块）：+20 检查（S0 常量；S1 默认端口 3478／S2 显式端口／S3 大小写+trim／S4 前导 `+` 镜像／S5 `turn:` 具名拒绝／S6 其他 scheme 与无 scheme／S7 空 host／S8 `port-zero`+越界+非数字+空端口／S9 括号 IPv6／S10 含 `:` host 不生成非法 /128；S11 去重后 `kind=stun` 端点／S12 逐条拒绝不静默／S13 部分拒绝⇒缺口；S14 空⇒`no-stuns`／S15 缺失⇒同一缺口不抛；S16 仅 turn⇒无端点+具名计数；S17 异常元素逐条报告且可用兄弟仍冻结／S18 非数组段⇒如实缺口不抛；S19 stun 端点⇒`/32` `isExcludedRoute`）。
- `run-relay-exclusion.mts`（驱动**真实** `deriveEndpointExclusions` 端到端）：+14 检查（S1 stun host 入排除集 `/32`＋标记逐字＋`VPN_ENDPOINT_EXCLUSIONS` 计数含 stun＋无拒绝标记；S2 留存壳排除集/解析（arm 的 `shell-exclusion` 源，kind=stun）；S3 IPv4 字面量短路 DNS（假 DNS 表**故意不含**该 host，仍出 `/32`，端口默认 3478）；S4 `turn:` ⇒ **无任何 turn 路由**（假 DNS 表能应答该 host 也不出路由）＋具名理由行＋`turnRejected=1`＋`stunGap=unusable-uris`；S5 `stuns` 为 undefined（I2 前的调用形态）/null/空数组 ⇒ 无 stun 路由＋`stunGap=no-stuns`＋不抛；S6 异常元素逐条报告且可用兄弟仍 `/32`；2 条 arm 接线文本 pin（`net.stuns` 第 5 实参、两个标记格式逐字）——该跳需原生 arm，沿用 `run-exclusion-addr-shape.mts` A5 文本 pin 先例）。
- 替身/链接面同步：`run-relay-exclusion.mts:65` 与 `run-exclusion-addr-shape.mts:66` 的 `SHIM_TYPES[NetBirdEndpointExclusion]` 补 4 个被类型剥离擦除的名字（`StunDerivationResult`/`StunUriEntry`/`StunParseResult`/`StunEndpoint`）；`run-relay-flag.mts`/`run-relay-status-log.mts` 写的 `NetBirdEndpointExclusion.dep.ts` 补 `StunUriEntry`（`NetBirdConnector.ets:7-9` 新增的 type-only 导入，值导入面未变）。`@ohos.net.connection` 三处替身**无需**再改（本轮不新增平台 API 调用）。
- 其余 selfcheck（`run-relay-flag.mts`/`run-relay-status-log.mts`/`run-route-validation.mts`/`run-exclusion-addr-shape.mts`）断言逐字未改，仅上述 stage 同步。

**⑤ core 测试与形状断言（先 grep 全部形状断言）**

- 新增 `connector::tests::network_config_tail_carries_the_announced_stun_uris_verbatim`（`connector.rs:5634`）：逐字入文档（含 `turn:` 与 `not-a-uri`，**不预过滤**）、顺序与尾部位置、仍可被严格 reader 解析、既有字段片段逐条仍在；config-less update 保留上一次公告；显式空公告清空为 `"stuns":[]`。
- 更新形状测试 `shell_network_config_snapshot_and_json_contract`（`:5443`）：断言 `"stuns":[]`＋`ends_with(",\"stuns\":[]}")`（尾部追加纪律 pin）。
- **【验证完整性级发现】既有静默缺口：JSON 契约测试长期未运行（已恢复，逐字登记）**：
  - 事实：`connector::tests::shell_network_config_snapshot_and_json_contract`（`connector.rs:5443`）自 commit `d9346c3`（「fix(client): 真机 run 1 的三个修复——CIDR 地址、沙箱凭据、ICE socket 供给」）起**丢失 `#[test]` 属性** ⇒ 该 JSON 契约测试（含 §13.12 及本节写入其中的断言：地址/前缀、路由条目、DNS 去重、peer 摘要、默认路由闸标记、凭据纪律、严格 reader 可解析）**在恢复前从未运行**；恢复后逐条全绿（`cargo test` 实跑，见 ⑥）。
  - 判定级别＝**验证完整性（verification integrity）**：不是产品行为缺陷，而是「有测试钉住该契约」这一**表述**在此之前部分不成立——`d9346c3` 之后凡引用「§13.12/既有形状断言已覆盖该 JSON」的验收叙述，其证据强度须按「未运行」重估（`shell_network_config_snapshot_and_json_contract` 的编译告警 `function ... is never used` 是当时唯一可见信号，未被处理）。
  - 处置：就地恢复 `#[test]` 一行（不改任何断言、不改被测代码）；恢复后该用例逐条通过，说明契约本身**未**在此期间被破坏（漂移为零），结论＝「证据缺失」而非「契约失效」。
  - **系统性有界扫描（同轮追加，防同类静默缺口）**：对 `client/core/src/**/*.rs` 与 `client/core/tests/*.rs` 做两遍扫描——① 名字含 `test_`/`_test`/`_contract`/`_snapshot`/`_holds`/`_rejects`/`_asserts`/`_is_`/`_must_` 等测试性模式且位于测试上下文（`tests/` 文件或 `#[cfg(test)] mod …`）却无 `#[test]`/`#[tokio::test]` 者；② **不依赖命名**的判据（零参数 + 无返回值 + 体内含 assert/panic + 未被任何地方调用 ⇒ 必然是「测试体」）。**结果：pass① 17 个全部为 helper**（fixture 构造器：`test_private_key_b64`(10)、`test_config_json`(4)、`shell_test_map`(17)、`relay_test_attach`(4)、`test_secret_b64`(2)、`test_slot`(5)、`test_key_b64`(4)、`make_test_ca`(17)、`plaintext_contract`(2)、`test_relay_attach`(2/5)、`base_snapshot`(7)、`fd_is_open`(11)、`fd_is_nonblock`(2)——判定依据＝**带参数或非 `()` 返回**且 `call_occurrences ≥ 2`（被测试调用），无一满足「零参数 + 无返回 + 体内断言」的测试体形态；同类的参数化断言辅助如 `assert_stays_false(window, pred, what)`（`tests/relay_*.rs`/`wg_over_relay_n13.rs`）因带参数而不入 pass② 判据，其「辅助而非测试」由签名直接判定），**pass② 0 个** ⇒ 当前工作树**无其它丢失属性的测试**。扫描器自身以 HEAD 版本（属性仍缺失）做**阳性对照**：pass② 恰好命中 `shell_network_config_snapshot_and_json_contract`（`connector.rs:5399`@HEAD）且仅此一例 ⇒ 判据非空转（扫描脚本为 /tmp 下只读解析器，未入库）。
- 集成 `tests/n2h_device_collector.rs:1827-1831` 新增断言：无 map 时 `connector_network_config()` 逐字节仍是 `{"available":false,"reason":"no-network-map"}`（stun 段只随 `available:true`，不伪造）。
- 新增 `n2h_collector::tests::freeze_assembly_takes_the_shell_stun_exclusion_and_keeps_core_stuns_too`（`n2h_collector.rs:3796`）：钉住本增量引入的**跨边界链接的 core 半边**——`shell_exclusions[kind=stun]`（arm 源）冻结为 `source=shell-exclusion`、`proto=udp`、`resolved=<shell 解析的 ip:port>`、`present:true`；与同 `resolved` 的 core-network-map 候选去重后壳条目**保留槽位**；core 源的另一个 stun 端点仍并存冻结（不因合并丢 kind）。
- 契约行：`lib.rs:88` 形状串补 `,stuns:[{uri}]` + 纪律说明（逐字转发、尾部追加、空=如实缺口、`turn:` 由壳侧具名拒绝）。
- 未触碰：`n2h.rs`（渲染器/判据）、`napi.rs`、`connector_status()` 渲染路径、`wg_device.rs`、预注册草案、T0 审查记录、`Cargo.toml`/`Cargo.lock`（无新依赖）；`n2h_collector.rs` 仅新增一个测试（生产代码零改动）。

**⑥ 改动面与验收（工作树实读，HEAD `9460224` + 本节未提交改动）**

- 改动文件（13）：`client/core/src/connector.rs`、`client/core/src/lib.rs`、`client/core/src/n2h_collector.rs`、`client/core/tests/n2h_device_collector.rs`、`client/entry/src/main/ets/vpnextensionability/{NetBirdEndpointExclusion,NetBirdConnector,NetBirdVpnExtensionAbility}.ets`、`client/entry/selfcheck/{run.mts,run-relay-exclusion.mts,run-relay-flag.mts,run-relay-status-log.mts,run-exclusion-addr-shape.mts}`、本文件。
- 验收实跑：
  - `cd client/core && cargo test --offline --locked --color never` → **34 个测试目标全 ok，合计 537 passed / 0 failed / 0 ignored**（`9460224` 基线 534/0；+2 新用例 +1 由 ⑤ 恢复的既有契约用例）。
  - `bash client/core/build.sh` → exit 0；`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`；末行 `build self-identification N2H_BUILD_SHA256=951ca4933710eca319c06492a558353043a078318965ba032d3f6a5156b3e257 (recomputed independently; present in the artifact)`（源码改动 ⇒ 与 §13.12 的 `72bc6e32…` 不同，符合该摘要语义）。
  - 壳侧自检（全 `exit 0`）：`run.mts` **81 passed / 0 failed**（§13.12 基线 61）、`run-relay-exclusion.mts` **49 / 0**（基线 35）、`run-relay-flag.mts` **9 / 0**、`run-relay-status-log.mts` **3 / 0**、`run-route-validation.mts` **40 / 0**、`run-exclusion-addr-shape.mts` **25 / 0**。
  - `bash client/build.sh` → exit 0（`CompileArkTS` 对含本节改动的 ets 全量编译成功，仅余既有行告警），逐项 `VERIFY OK`（HAP 成员 `.so` sha256 `3ee888a6947ddc625fbd80b09c843197e956444180ef95c37c3e466ebeeabbf8`，两次构建逐字节相同——摘要输入＝`Cargo.{toml,lock}`/`build.rs`/`proto`/`src`，见 `build.rs:76-77`）；HAP `entry-default-unsigned.hap` **4 225 199 bytes**、sha256 `d233f19d6128e50175d5417d4d1daee22ec7cdfb38bd2c7f7b59348536abeb0a`（HAP 容器不可逐字节复现，同一 `.so` 的两次打包 sha 不同属预期；验收以 exit 0 + 逐项 VERIFY OK + 成员 `.so` 摘要为准；`/tmp/n2h-frozen/` 的冻结制品未触碰）。
  - `git diff --stat`：13 files changed, 753 insertions(+), 14 deletions(-)（含 §4.0 stun 行就地更正的 1 行改写）。

**⑦ 范围外与如实登记**

- **被本节取代的旧字面（已按批准就地更正 + 在此同步登记）**：§4.0 冻结来源表 stun 行备注原文「壳排除集目前**不含**此类（B-2 的 3/7 现状，如实入档）」**自本节起失效**；该行已按 §13.12 处理 dns 行的同一形态**就地更正**（行内标注「（I2 更正：…）」并保留旧文本为历史引述）。现状：壳侧排除派生 kind＝management/signal/relay/dns（§13.12）/stun（本节）＝**5/7**，仅余 **wg_peer** 为缺口（其独立增量）。预注册 §2-C2 的 B-2 文字属预注册面，本增量不改（纪律），故该处的 3/7 叙述以本节为准。
- **本轮不做**（各有独立增量）：wg_peer 代际、两个新代际标记行、`gen=` 字段、ARMED gen/recreate 字段、S20 guard、5 类必需端点 fatal 扩面；`turn` 仍为可选类（`present:false` + 具名理由），本 build 无 turn 路径故不新增 turn 通道。
- **残余（如实）**：(a) 壳侧 stun 缺口只入 hilog 标记，不进证据 `notes[]`（arm opts 契约未扩，判定侧目前据冻结面 `stun` 条目的 `source`/`present` 与排除面 `VPN_ROUTE_ENTRY` 行判定）；(b) `turn:` 在被暴露的 `stuns[]` 中**常态不出现**（core `set_stuns` 拒收即不留存），故壳侧具名拒绝路径在真实流量里通常不可达，仅作为暴露面逐字转发的防御与 `turns[]`（另一字段，本轮不暴露）未来的入口；(c) 壳侧 stun 解析比 core 多一条 `unusable-host` 拒绝（② 已给理由），若未来 core 放宽 host 语义需同步复核。

### 13.14 ⑫ I3a 增量落地：C2 代际判定的机械地基（代际标记行 + 路由行 `gen=` + recreate 接受锚点 + 采集器侧 `exclusion_generation`）

> 本节以**追加**为主，不改写 §0–§13.13；**唯一例外＝§5.1 arm opts 契约行、§3.3 `collector{}` 字段清单、§3.4-D10 行**（本增量按预注册 v3.1 §2.1 R-2① 就地追加，均带「（I3a 更正）」标注并把旧文本逐字保留为历史引述，见 ⑦ 第 5 项与各该行行内标注），其余各行**未触碰**。基线：工作树 HEAD `8cd80af`（干净，I2 已合入）+ 本节未提交改动；下列 file:line 为落地后工作树实读。

**① 壳侧实现要点（代际标记行 / 路由行 `gen=` / 两个接受锚点）**

- **计数器与发射**：`NetBirdVpnExtensionAbility.ets:348` 新字段 `n2hExclusionGeneration`（每实例自 -1 起；`requestId` 在 `onCreate` 固定 ⇒ 每实例即「同 `requestId` 内」序列）；`:1734-1740` 新方法 `logExclusionGeneration(kind, routes, wgPeerEndpoints): number` —— 先 `++`、再经纯模块 `formatExclusionGeneration`（`NetBirdRouteValidation.ets:259-269`）打**恰一行**，返回 gen 供锚点使用。
- **两条腿的调用点**（均在该腿 `create()` **之前**，且**先**代际行**后**该代路由行）：initial `:709-711`（`connection.create(config)` 在 `:736`）→ `settleCreate(fd, gen)`（`:738`）；recreate `:1493-1495`（`createConnectionBounded` 在其后）。
- **计数来源＝即将提交的 config**：两处 `routes` 实参都是随后交给 `buildPlatformVpnConfig(merged)` → `create()` 的同一个 `merged.routes`（recreate 腿中间的 destroy 不改写它）⇒ `routes=`/`exclusions=` 为**实际值**而非估算；`exclusions=` 与行字面用**同一谓词**（`NetBirdRouteValidation.ets:231-239` 新增 `excludedRouteCount`，按 `effectiveExcludedText(...) === 'true'` 计；`VPN_ROUTE_TABLE` 头的 `excluded=` 一并改用该谓词 —— 见 ⑦ 第 4 项）。
- **路由行 `gen=`**：`formatRouteTable(requestId, interfaceName, routes, gen)`（`NetBirdRouteValidation.ets:279`）每行**末字段**追加 `|gen=<n>`（`:304`）—— 既有字段位置逐字不变，I3a 之前的行是当前行的**字节前缀**（N13/H1 既有 grep 与自检 T7 断言不受影响）。
- **接受锚点**：initial 保持既有 `VPN_CREATE_RESOLVED|…|accepted=<bool>|marker=…` 字面**不变**（`:1779-1780`），仅在其 `accepted` 分支就地记录接受代（`:1781-1787`；`settleCreate(fd, gen)` 定义 `:1769`）；recreate **新增** `VPN_RECREATE_RESOLVED|requestId=%{public}s|gen=%{public}d|accepted=true`（`:1557-1559`，fd 契约通过处、与 initial 腿**同接受语义**）并在同处记录接受代（`:1556`）。`VPN_RECREATE_SNAPSHOT`/`VPN_RECREATE_DONE`/`VPN_CREATE_BEGIN` 等既有行**未改写**（自检 G7 文本 pin）。
- **顺序保证**（规则 (4) 后半）：代际行与全部 `gen==G` 行都在锚点**之前**，由两腿调用点位置 + 自检 G2/G3 按 hilog 行序双重钉住。

**② core/采集器侧实现要点（`exclusion_generation`：arm → 证据文档）**

- `ArmOptions.exclusion_generation: Option<u64>`（`n2h_collector.rs:1885`；解析 `:1941-1950`）：非负整数照收；**缺键 ⇒ `None`**（循 `dns` 先例：缺失＝判定侧缺口，core **不编造**）；类型不符（负数/字符串/`null`/数组/对象）⇒ `Err` ⇒ 整臂拒绝 `invalid-opts`。
- `collector_block_json(reported_sha256, exclusion_generation)`（`:2892-2915`）：在 `collector{}` 块**尾部追加** `,"exclusion_generation":<n|null>` —— 四个 §3.3 字段的顺序/位置不动，块仍是文档最后一个键；`with_collector_block(document, reported_sha256, exclusion_generation)`（`:2920-2932`）同步扩参，唯一生产调用点 `run_window` 传入 `win.options.exclusion_generation`（`:2447-2451`）。
- **三方同源（R-2）**：壳侧唯一字段 `n2hAcceptedExclusionGeneration`（`:359`）在 arm 钩子内**一次读取**（`:1177`）→ ① `opts.exclusion_generation`（`:1178-1180`，`>= 0` 时）② ARMED 行 `|gen=%{public}d`（`:1211-1215`，插在 `requestId=` 之后，其余字段序逐字不变）；core 侧①的原值即文档字段 ⇒ 「文档字段 ＝ ARMED `gen` ＝ 最后一次被接受的代际行 gen」由结构保证（自检 G5 用真实 arm 钩子断言三方相等，G6 断言无接受代时如实缺值）。
- **纪律**：`connector_status()` 形状**未触碰**（§5.2 仍成立）；`n2h.rs`（渲染器/判据）零改动；`napi.rs`/`lib.rs` 未改（无新导出、无新依赖）。

**③ 与预注册 v3.1 §2.1 的逐字对照**

| 款 | 原文要求（v3.1 §2.1） | 落点 |
|---|---|---|
| (1) | 每代在 `create()` **之前**输出**恰一行** `VPN_EXCLUSION_GENERATION\|requestId=<id>\|gen=<n>\|kind=initial\|recreate\|exclusions=<m>\|wgPeerEndpoints=<k>\|routes=<r>`；`n` 自 0 单调递增、同 `requestId` 内不重复 | ① 的发射点与单计数器；字段名/字段序/字面逐字一致；`kind` 取纯模块常量 `EXCLUSION_GENERATION_INITIAL`/`_RECREATE`（`NetBirdRouteValidation.ets:245-246`），调用点无法造出第三值 |
| (2) | 该代全部 `VPN_ROUTE_ENTRY\|…` 行携带**同值** `gen=<n>`，行集按 `gen` 分割 | ① 的末字段追加；两腿各自一次调用、gen 取同一次发射的返回值 |
| (2a) | （R-1）四项：行数恰为 `r`；`index` 唯一连续覆盖 0..r-1；`isExcludedRoute=="true"` 行数恰为 `m`；`n` 自 0 逐次 +1 无跳号 | `r`＝`routes.length`、`m`＝同谓词计数、`index` 由数组序、`n` 由单计数器；自检 T8/G1/G3/G4 逐项断言（含第二次 recreate 的 `0,1,2` 序列） |
| (4) | initial 锚点＝`VPN_CREATE_RESOLVED\|…\|accepted=true`；recreate 锚点＝新增 `VPN_RECREATE_RESOLVED\|requestId=<id>\|gen=<n>\|accepted=true`；代际行与全部 `gen==G` 行须**早于**该接受行 | ① 的两锚点与顺序；自检 G2（initial，真实 `settleCreate`）/G3（recreate，真实整腿）按行序断言 |
| R-2① ② ③ | 代 G 作 arm 输入写入 `collector.exclusion_generation`；ARMED 行增补 `gen=G`；三者**逐字相同**，缺任一/不一致 ⇒ C2 不成立 | ② 的同源实现；G5 三方相等、G6 无接受代时缺值不编造（`-1` ⇒ 省略键 / core 记 `null` / ARMED 打 `gen=-1`） |

（R-3 冻结解析集唯一规范化算法已随 I1 落盘、R-5 S20 契约属 S20 增量，均不在本节。）

**④ selfcheck 扩展（host-only，无设备/无网络）**

- **新增套件** `client/entry/selfcheck/run-exclusion-generation.mts`（440 行，**33 检查**）+ 新替身 `client/entry/selfcheck/exclusion-generation-hooks.mjs`（179 行；功能性 stub：记录型 hilog／假 fd 探针／假网络图／假 VpnConnection／假 arm 文件）。原则同 `run-relay-exclusion.mts`：五个真实 .ets 逐字节 stage 后驱动**真实**扩展类 —— G1 initial 腿发射（字面/计数/`index`/gen）；G2 真实 `settleCreate` 出 `VPN_CREATE_RESOLVED|accepted=true` 并把该代记为接受代（行序早于锚点）；G3 真实 `performControlledRecreate` **整腿跑通**（destroy→create→fd 契约→wg feed→ACK `isRecreate=true`）出 `kind=recreate` 行（5 行/3 排除）＋ `VPN_RECREATE_RESOLVED|gen=1|accepted=true`，且 `VPN_RECREATE_SNAPSHOT`/`_DONE` 字段逐字不变；G4 第二次 recreate ⇒ `gen=2`，序列 `0,1,2`（kind `initial,recreate,recreate`）；G5 真实 arm 钩子三方逐字一致；G6 未被接受的 `create()` 不成为接受代且 arm 不编造；G7 文本 pin（两腿调用序、两锚点字面、既有行未改、`merged.routes, 0` 两处传参）。
- `run-route-validation.mts`：**+9 检查（T8）** —— 代际行逐字（含 `routes=`/`wgPeerEndpoints=`/`exclusions=` 三字段）；行数/`index` 连续/`isExcludedRoute=="true"` 行数三项对账；每行 gen == 代际行 gen 且 `gen=` 为末字段；第二代自洽（`gen=1|kind=recreate`）；表头 `excluded=` 与 `exclusions=` 同谓词。既有 T7 的两处 `formatRouteTable` 调用仅补 `gen` 实参，断言逐字未改。
- **计数（7 套全 `exit 0`）**：`run.mts` **81/0**、`run-relay-exclusion.mts` **49/0**、`run-exclusion-addr-shape.mts` **25/0**、`run-relay-flag.mts` **9/0**、`run-relay-status-log.mts` **3/0**、`run-route-validation.mts` **49/0**（I2 基线 40）、`run-exclusion-generation.mts` **33/0**（新）。

**⑤ core 测试与形状断言**

- 新增 `n2h_collector::tests::the_collector_block_carries_the_shell_reported_exclusion_generation`（`n2h_collector.rs:5572`）：块尾 `,"exclusion_generation":<n>}`（`None` ⇒ `,"exclusion_generation":null}`）、**恰五字段**且第五条为该键、严格 reader 解析后第五字段值 `Json::Num(gen)` 逐字、既有四字段顺序冻结。
- 新增 `n2h_collector::tests::arm_options_carry_the_shell_reported_exclusion_generation`（`:4648`）：`0`/`7` 照收（`0` 是真实代，不等于「缺键」）、缺键 ⇒ `None`、六类异常形态（字符串/布尔/`null`/数组/对象/负数）逐条 `Err`。
- 既有三个 I7 块用例（`the_collector_block_identifies_the_build_with_the_frozen_shape`/`a_reported_digest_wins_and_is_labelled_operator_reported`/`the_collector_block_is_tail_appended_to_the_frozen_renderer_output`）同步扩参，断言逐字保留（含尾部纪律 pin）。
- 集成 `tests/n2h_device_collector.rs`：`offline_opts` 全程携带 `"exclusion_generation":3`（`:962`/`:981`）⇒ 端到端断言设备文档 `collector.exclusion_generation == 3`（`:1170-1174`）＋尾键 pin（`:1182-1185`）；D10 例外表新增 `("$.collector.exclusion_generation", "number")`（`:463`），schema 走查逐类型核验通过。
- **未触碰**：`n2h.rs`、`napi.rs`、`lib.rs`、`connector.rs`、`wg_device.rs`、预注册草案、T0 审查记录、`Cargo.toml`/`Cargo.lock`。

**⑥ 改动面与验收（工作树实读，HEAD `8cd80af` + 本节未提交改动）**

- 改动文件（5 已有 + 2 新增 + 本文件）：`client/core/src/n2h_collector.rs`、`client/core/tests/n2h_device_collector.rs`、`client/entry/src/main/ets/vpnextensionability/{NetBirdRouteValidation,NetBirdVpnExtensionAbility}.ets`、`client/entry/selfcheck/run-route-validation.mts`、**新增** `client/entry/selfcheck/{run-exclusion-generation.mts,exclusion-generation-hooks.mjs}`、本文件。
- 验收实跑：
  - `cd client/core && cargo test --offline --locked --color never` → **34 个测试目标全 ok，合计 539 passed / 0 failed / 0 ignored**（`8cd80af` 基线 537/0；+2 新用例）。
  - `bash client/core/build.sh` → exit 0；`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`；末行 `build self-identification N2H_BUILD_SHA256=00166dfaeab1a65ae55f86583153e92923f4eb9d2f5943bcc60424eb0816cf83`（源码改动 ⇒ 与 §13.13 的 `951ca493…` 不同，符合该摘要语义）。
  - 壳侧自检 7 套全 `exit 0`：计数见 ④ 末行。
  - `bash client/build.sh` → exit 0；`CompileArkTS` 对含本节改动的 ets 全量编译成功（仅余既有行告警）；逐项 `VERIFY OK`（HAP 成员 `.so` sha256 `5fc7973276d4ef6b868a7415e23a429ee5734b2bd1aff8038c7270ea882131be`）；HAP `entry-default-unsigned.hap` **4 233 079 bytes**、sha256 `81b47b903ed4c917eecd41003a62ec53a558fcb9e1c28f1f97d78b8cdfaa9023`（HAP 容器不可逐字节复现，验收以 exit 0 + 逐项 VERIFY OK 为准；`/tmp/n2h-frozen/` 冻结制品未触碰）。
  - `git diff --stat`：**6 files changed, 494 insertions(+), 46 deletions(-)**（含本节就地更正的 3 处行内插入）；另有 2 个新增未跟踪自检文件（`run-exclusion-generation.mts` 440 行 + `exclusion-generation-hooks.mjs` 179 行），`git status --porcelain` 无其它未预期文件。
- **真实标记样例（真实 .ets fixture + 真实方法驱动，逐字）**：`VPN_EXCLUSION_GENERATION|requestId=req-i3a-sample|gen=0|kind=initial|exclusions=3|wgPeerEndpoints=0|routes=4` → 4 行 `VPN_ROUTE_ENTRY|…|gen=0` → `VPN_CREATE_RESOLVED|requestId=req-i3a-sample|fd=42|accepted=true|marker=CREATE_ACCEPTED`；recreate 腿 `…|gen=1|kind=recreate|exclusions=3|wgPeerEndpoints=0|routes=5` → 5 行 `…|gen=1` → `VPN_RECREATE_RESOLVED|requestId=req-i3a-sample|gen=1|accepted=true`；arm `N2H_COLLECTOR_ARMED|requestId=req-i3a-sample|gen=1|openedMono=4242|exclusions=0|posctlPort=40001|probeWaitMs=800|peerB2aProbeId=none|diag=true|is_evidence=false` ＋ arm opts `{"shell_exclusions":[],"posctl_port":40001,"probe_wait_ms":800,"exclusion_generation":1}`。

**⑦ 偏离、裁定与范围外（逐条如实）**

1. **R-5 推迟到 S20 增量（已裁定「批准」）**：R-5 要求 ARMED 行固化 `recreateCoolingDown`/`recreateExhausted`/`recreateReason` **且** S20 触发时输出稳定 `N2H_COLLECTOR_NOT_ARMED|reason=recreate-pending|…`；本增量范围明写 S20 guard 属后续增量，只落 `gen=G`。**只落三字段而缺 S20 分支正是 R-5 明令防的「部分实现假通」**，故整体一并落；S20 派发稿将显式写「R-5 归入本增量」。
2. **`wgPeerEndpoints=0` 与第 (6) 款后果：预期内（已裁定确认）**：wg_peer 外层端点集未派生（属 I3b），字段已在而值为 0 ⇒ 真机轮 `wg_peer present==true ∧ wgPeerEndpoints==0` 按 §2.1 (6) 使 **C2 不成立**（`n2h-inconclusive`）—— 与预注册既有登记「排除面补齐 7/7 前不得登记 N2-H pass」一致，判据不削弱。
3. **无接受代的诚实形态：已裁定批准并保留**：`n2hAcceptedExclusionGeneration == -1` ⇒ arm **省略** `exclusion_generation` 键（core 记 `null`）、ARMED 行 `gen=-1`（`:1177-1180`/`:1211`）。该状态在真实 arm 路径不可达（`device_up && ready` 只在 create 被接受后成立）；此形态保证「缺值」而非「编造值」，方向 fail-closed（三方不一致 ⇒ C2 不成立）。**不**改为 arm 前拒绝开窗。
4. **`VPN_ROUTE_TABLE` 头 `excluded=` 谓词与行渲染对齐（一行）：已裁定批准**：计数由真值判断改为 `=== 'true'`（与行字面同谓词），使 `exclusions=m`／表头／行字面三者不可能分裂；合法布尔输入下行为不变。
5. **本文件三处就地更正（本次补做关闭）**：§5.1 arm opts 契约行、§3.3 `collector{}` 字段清单、§3.4-D10 行 —— 均带「（I3a 更正）」标注并把旧文本逐字保留为历史引述；除这三处行内插入外，本增量对本文件**只追加本节**。
6. **本轮不做**（各有独立增量）：wg_peer /32 注入（I3b）、5 类必需端点 fatal 扩面、S20 壳侧 arm guard（含 R-5 三字段与 NOT_ARMED 稳定 reason）、**C2 比对脚本**（生成侧已可产出全部标记与字段，判定侧脚本未落盘）。
7. **残余（如实）**：(a) `gen` 计数为**每 ability 实例**（`requestId` 实例内固定）——撤销后复验等**新会话实例**自 0 重新起算，与 §2.1 (1)「同 `requestId` 内」口径一致，**不得跨实例拼接**；(b) 中间失败腿（route-validation 失败／destroy 失败／create 超时／fd 契约失败）会留下**已宣告但无接受锚点**的代 —— 该代**不构成 G**，判定侧不得视为被接受的代（C2 不成立方向）；(c) hilog 丢行无法由实现侧防止，这正是 `routes=`/`exclusions=` 可计算基数（(2a)）要检出截断的目的 —— 行数与申报不符 ⇒ C2 不成立；(d) `VPN_CREATE_RESOLVED` 的 `accepted` 语义＝create 被平台接受（fd 契约通过），其后的 protect/wg-feed/ICE 失败仍按既有 fail-closed 拆除 —— 接受锚点与「完整流程成功」不是同一件事（与 initial 腿既有语义一致）。

### 13.15 ⑬ I5a 增量落地：消除 arm 路径的静默分支（arm 控制文件探测的可观测性）

> 本节**只追加**，不改写 §0–§13.14 任何正文（本增量对本文档的唯一改动就是本节）。基线：工作树 HEAD `6cbda4b`（干净）+ 本节未提交改动；下列 file:line 为落地后工作树实读。范围＝**只把静默变可见**：新增一个标记行及其探测分支，既有标记的字段/顺序、arm 闸语义、`connector_status()` 形状、设备命令面**均未触碰**，core（Rust）零改动。

**① 动机（2026-09-18 真机 dry-run 实测）**

- **事实**：设备**已建立 VPN 会话**（`vpn-tun inet 100.108.144.165` 实测存在、宿主对端 `session_established=true`），但 **84 次 `N2H_COLLECTOR*` 读数全 0 行**、宿主对端 `a2b_seen=0`（设备从未回发 a2b 探针）⇒ 采集器**从未 arm**，而**没有任何标记说明为什么**。「arm 从未尝试」与「尝试后静默失败」在日志上不可区分。
- **最可能成因（本增量即该缺口）**：`maybeArmN2hCollector` 的 arm 控制文件检查是静默路径——`fs.accessSync(N2H_ARM_FILE)` 抛错 ⇒ `present=false` ⇒ **直接 return**，既不置 `n2hArmAttempted`、也不打任何标记（I6b 登记项）。设备上该文件缺省不存在，故这条路径就是「84 次 0 行且无解释」的候选解释。

**② 新标记的逐字形态与取值域（唯一新增标记）**

- 逐字（`NetBirdVpnExtensionAbility.ets:1140-1144`；`hilog.warn`）：
  `N2H_COLLECTOR_ARM_SKIPPED|requestId=<id>|reason=<token>|path=/data/local/tmp/netbird-n2h-arm|accessError=<token>|attempted=true|diag=true|is_evidence=false`
- **字段形状恒定**（恒 8 字段，只有值随探测结果变化）：`reason` 与 `accessError` 两个取值域如下，`path` 是源码常量（开发通道固定路径，**非**文件内容），`attempted=true` 与 `diag=true|is_evidence=false` 为常量。
- `reason` 取值域（**恰好二值，互斥且不合并**，常量 `:90-91`）：

| token | 判据 | 语义 |
|---|---|---|
| `arm-file-absent` | `fs.accessSync` 返回 `false`，**或**抛出的 BusinessError 码为 `13900002`（ENOENT） | **已证明**文件不存在 |
| `arm-file-unreadable` | 抛出**任何其它**错误 token（`13900012` 权限拒绝 / `13900005` I/O / `13900011` / `13900013` / `13900042` / 无 code 的抛出） | 探测失败且**未能证明**不存在——文件可能存在但本进程取不到 |

- `accessError` 取值域（只记错误码，不记路径内容、不记 message、不记文件字节）：`none`（API 以布尔回答，未抛错）／`<十进制错误码>`／`no-code`（抛出但无 code token）。它把 `arm-file-absent` 的**两种形态**（布尔 `false` ⇒ `none`；ENOENT 抛出 ⇒ `13900002`）也如实分开。
- **`fs.accessSync` 可区分性（实测/实读结论，非推测）**：SDK 26.0.0.821 的 `@ohos.file.fs.d.ts:309-336` 逐字为 `declare function accessSync(path: string, mode?: AccessModeType): boolean`，文档同时给出两条「不存在」的形态——返回 `boolean`（`true` 存在／`false` 相反，`:318`）与 `@throws { BusinessError } 13900002 - No such file or directory`（`:319`）——以及一批**不证明不存在**的错误码（`:320-330`：`13900005` I/O、`13900008`、`13900011`、`13900012` Permission denied、`13900013`、`13900018`、`13900020`、`13900023`、`13900030`、`13900033`、`13900042`）。⇒ **可区分**：以错误码为准把「ENOENT（证明不存在）」与「其它码（未证明不存在）」分开，布尔返回与 ENOENT 抛出同归 `arm-file-absent` 而由 `accessError` 保留形态差异。无 code token 的抛出**不得**冒充「不存在」，故归 `arm-file-unreadable`（fail-honest 方向）。
- 探测实现：新私有方法 `probeN2hArmFile(): N2hArmProbeOutcome`（`:1281-1296`）＋新接口 `N2hArmProbeOutcome{present,skipReason,accessError}`（`:191`）＋三个常量（`:90-92`）。除 arm 钩子外无第二个调用点。

**③ latch 语义（一次性，仍是同一个 `n2hArmAttempted`）**

- `this.n2hArmAttempted = true;` 由「探测通过后」**前移**到探测**之前**（`:1131`，紧随 `if (this.n2hArmAttempted || this.destroyRequested) return;`）⇒ **上闩的是「尝试」而不是「成功」**：每个 ability 实例的 arm 尝试仍**至多一次**，skip 行因此在**首次**到达该钩子的 tick 上打印且永不重复（自检 A2）。**未新增第二个计数器**：skip 行的 `attempted=true` 就是这**同一个**字段的可见形态（自检 A1 断言两者同时为真）。
- **行为后果（如实登记）**：旧实现在文件缺席时不上闩、每个 tick 重查一次，故「会话中途放入 arm 文件」原本仍可开窗；现在同实例不再复查（须**新会话实例**才可能 arm）。这是 ④ latch 语义的直接结果，也是让「从未尝试」与「尝试后跳过」可区分的代价；对真机取证流程（arm 文件先于会话放置，runbook 既有操作顺序）无影响。
- `destroyRequested` 短路（`:1123`）**保持静默**（范围外，见 ⑥(b)）。

**④ selfcheck 扩展（host-only，无设备/无网络）**

- 沿用 `run-exclusion-generation.mts` + `exclusion-generation-hooks.mjs`（真实 .ets 逐字节 stage、真实扩展类驱动）。**新增 A 组 13 检查**（`run-exclusion-generation.mts:417-` 起）：
  - **A1（5 检查）** ① 文件缺失（探测抛 `13900002`，即真机形态）⇒ **恰一行** skip 行且逐字等于上式、`reason=arm-file-absent|accessError=13900002`；**不调用 arm**（`__NB_ARM_OPTS` 未写入、无 ARMED/NOT_ARMED）；闩可见（行内 `attempted=true` ∧ 实例 `n2hArmAttempted===true`）；日志中无 arm 文件内容（哨兵串不出现）。
  - **A2（2 检查）** ④ 同实例第二次 tick **零行**；随后把文件置为存在再 tick 仍**零行**、不 ARMED ⇒ 闩是唯一且是「尝试」闩。
  - **A3（1 检查）** 布尔 `false` 形态 ⇒ 同一 `reason`，`accessError=none`（两种「不存在」形态由 `accessError` 区分）。
  - **A4（2 检查）** `13900012` ⇒ `reason=arm-file-unreadable|accessError=13900012`；抛出但**无 code** ⇒ `reason=arm-file-unreadable|accessError=no-code`（绝不冒充不存在）——两个 reason 不合并的直接反例。
  - **A5（2 检查）** ② 回归：文件存在 + 闸不满足（`deviceUp=false,ready=true` 与 `deviceUp=true,ready=false` 两种组合）⇒ 既有 `N2H_COLLECTOR_NOT_ARMED|…` 逐字未变、且**无** skip 行、未调用 arm（闸仍是 `device_up && ready`）。
  - **A6（1 检查）** ③ 回归：文件存在 + 闸满足 ⇒ 既有 `N2H_COLLECTOR_ARMED|…` 逐字未变（含 `gen=`／`openedMono=`／`exclusions=`／`posctlPort=`／`probeWaitMs=`／`peerB2aProbeId=`／`diag=true|is_evidence=false`），且无 skip 行。
- **替身扩展**（`exclusion-generation-hooks.mjs`，`@ohos.file.fs` stub）：由「`__NB_ARM_PRESENT===true` 才返回 true、否则抛 `13900002`」扩为四种可驱动形态——`PRESENT===true` ⇒ 返回 true；`PRESENT===false` ⇒ 返回 false；`__NB_ARM_ACCESS_ERROR` 为数 ⇒ 抛该码；为非数（如 `'no-code'`）⇒ 抛出**无 code**；两者皆未设 ⇒ 抛 `13900002`（**与原行为逐字相同的缺省**）。G1–G7 既有断言**逐字未改**（仅前述 header 注释与新增 A 组），其余 6 套自检**零改动**。
- **计数（7 套全 `exit 0`）**：`run.mts` **81/0**、`run-relay-exclusion.mts` **49/0**、`run-exclusion-addr-shape.mts` **25/0**、`run-relay-flag.mts` **9/0**、`run-relay-status-log.mts` **3/0**、`run-route-validation.mts` **49/0**、`run-exclusion-generation.mts` **46/0**（I3a 基线 33，+13）。
- **变异对照（阴性对照，/tmp 仓库外 fixture，证明新检查有牙）**：(i) 删掉 skip 的 `hilog.warn` 调用（恢复 I5a 之前的静默分支）⇒ **6 失败**（A1×3、A3、A4×2）；(ii) 把两个 reason 合并为恒 `arm-file-absent` ⇒ **2 失败**（A4×2）。两次变异均只影响新增检查，其余全绿。

**⑤ 改动面与验收（工作树实读，HEAD `6cbda4b` + 本节未提交改动）**

- 改动文件（3 已有 + 本文件）：`client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`、`client/entry/selfcheck/{run-exclusion-generation.mts,exclusion-generation-hooks.mjs}`、本文件。**未触碰**：任何 Rust 源（`client/core/**` 零改动）、`connector_status()` 形状、既有标记字面、`module.json5`、其余 6 套自检、预注册草案、T0 审查记录。
- 验收实跑：
  - `cd client/core && cargo test --offline --locked --color never` → exit 0；**34 个测试目标全 `ok`，合计 539 passed / 0 failed / 0 ignored**（与 `6cbda4b` 基线逐条相同——本增量不触碰 core）。
  - `bash client/core/build.sh` → exit 0；`14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)`；末行 `build self-identification N2H_BUILD_SHA256=00166dfaeab1a65ae55f86583153e92923f4eb9d2f5943bcc60424eb0816cf83`（与 §13.14 **相同**，符合「core 源码未改 ⇒ 摘要不变」）。
  - 壳侧自检 7 套全 `exit 0`：计数见 ④ 末行。
  - `bash client/build.sh` → exit 0；`BUILD SUCCESSFUL`、`assembleHap exit 0`；`CompileArkTS` 对含本节改动的 ets 编译成功（仅余既有 `Function may throw exceptions` 行告警）；逐项 `VERIFY OK`（HAP 成员 `.so` sha256 `5fc7973276d4ef6b868a7415e23a429ee5734b2bd1aff8038c7270ea882131be`）；HAP `entry-default-unsigned.hap` **4 233 955 bytes**、sha256 `7b377c64da89c670770f4c384b0fc00b8f0a1337a44af97af76e41cf6e1f4b52`（HAP 容器不可逐字节复现，验收以 exit 0 + 逐项 VERIFY OK 为准；`/tmp/n2h-frozen/` 冻结制品未触碰）。
  - `git diff --stat`：**4 files changed, 302 insertions(+), 18 deletions(-)**（3 个代码/自检文件 + 本节追加；本节对本文件是**纯追加**，diff 无删除行）；`git status --porcelain` 只含这 4 个 `M`，无新增未跟踪文件。
- **真实标记样例（真实 .ets fixture + 真实方法驱动，逐字；三形态 + 两回归）**：
  - ① 缺失（抛 `13900002`）：`N2H_COLLECTOR_ARM_SKIPPED|requestId=req-i5a-sample|reason=arm-file-absent|path=/data/local/tmp/netbird-n2h-arm|accessError=13900002|attempted=true|diag=true|is_evidence=false`（armCalled=false）
  - ①b 缺失（布尔 `false`）：`…|reason=arm-file-absent|path=/data/local/tmp/netbird-n2h-arm|accessError=none|attempted=true|diag=true|is_evidence=false`
  - ①c 未证明不存在（`13900012`）：`…|reason=arm-file-unreadable|path=/data/local/tmp/netbird-n2h-arm|accessError=13900012|attempted=true|diag=true|is_evidence=false`
  - ④ 同实例第二 tick：无任何行输出。
  - ② 存在 + 闸不满足（回归，逐字未变）：`N2H_COLLECTOR_NOT_ARMED|requestId=req-i5a-sample|reason=session-not-ready|deviceUp=false|ready=true|diag=true|is_evidence=false`
  - ③ 存在 + 闸满足（回归，逐字未变）：`N2H_COLLECTOR_ARMED|requestId=req-i5a-sample|gen=-1|openedMono=4242|exclusions=0|posctlPort=40001|probeWaitMs=800|peerB2aProbeId=none|diag=true|is_evidence=false`（`gen=-1` 是该 fixture 无接受代的如实缺值；有接受代时自检 A6 断言 `gen=0`）

**⑥ 残余盲区（如实，逐条）**

- **(a) 本标记的证明力边界**：`N2H_COLLECTOR_ARM_SKIPPED` 只能证明「**扩展进程跑到了这个检查点**」。它**不能**区分「扩展从未启动」与「扩展启动了但 watcher 未运行」——这两种情况同样零行。要区分需**另一层标记**（例如扩展 `onCreate` 的到达打点），那属新的增量，本次**未做**（见派发回报中的建议）。
- **(b) `destroyRequested` 短路仍静默**（`:1123`）：onDestroy 之后不再尝试 arm 且不打 skip 行——该分支被周围既有 teardown 标记（`VPN_DESTROY*` / onDestroy 路径）界定，但本身无「因为已在拆除所以没 arm」的专行；范围外，未改。
- **(c) 探测结果是时点观测**：同一实例内文件后放置不会被复查（③ 的 latch 后果），故 skip 行的**不存在**并不等价于「探测被跳过」；判定侧须以「本会话是否出现过 ARMED / NOT_ARMED / ARM_* / ARM_SKIPPED 之一」为口径。
- **(d) hilog 丢行**无法由实现侧防止：skip 行是单行、无基数可对账（与既有诊断行同级），其缺失只能与 (a) 的到达打点配合判读。
- **(e) 证据地位**：skip 行恒带 `diag=true|is_evidence=false`——它是诊断可观测性，**不是**判定证据，不改变 §6.1「默认关闭、不进产品面」与「arm 动作本身从不作 gate evidence」的纪律。



