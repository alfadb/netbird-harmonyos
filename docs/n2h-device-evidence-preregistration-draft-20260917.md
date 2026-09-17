# N2-H 设备侧证据判据预注册——草案（DRAFT，未生效）

- **文件性质：DRAFT / 未生效**。本文件是供人类与独立审查席（T0）审查的预注册**草案**，不是已生效的预注册。
- **生效条件**：判据、oracle、停止条件、不外推声明须在测量开始前书面登记并**经独立审查确认**，测量后不得追认或修改（`docs/native-nx-governance.md:43`，§二 绑定条款第 7 条；`:35` 「开门前细化只能细化证据方法，不得删改或削弱」）。本草案在独立审查确认前**不生效**。
- **测量前置**：任何设备侧测量须**人类签发的生效 AUTH**；本草案本身不是授权（先例口径：`docs/device-validation-plan-20260913.md:9-15` 「本计划本身不是授权」）。
- **在生效之前，未来任何测量不得以本草案为依据；不得引用本草案登记任何测量、verdict 或门级结论。**
- **当前状态基线**：设备侧 N2-H 证据尚未产出；在产出并经独立审查前，本平台 N2 相关结论一律为 `UNSAT + 替代测量进行中`，且开发期偏离（`--ps protectDeviation route-exclusion`，日志带 `is_evidence=false|not_a_gate_pass=true`）不构成任何门证据（`docs/native-nx-governance.md:136`）。
- 起草日期：2026-09-17（Asia/Shanghai，`date +%Y%m%d` 实取）。起草过程：只读核对仓库文档与代码；未执行任何设备命令、未修改任何既有文件、未运行构建。

---

## 1. 目标与范围

### 1.1 本测量要证明什么

在目标物理设备的**一个真实连接会话隔离窗口**内，以 `nbinterop isolation-check`（`client/core/src/n2h.rs`）的口径产出 N2-H 平台替代判据要求的设备侧证据：

1. 建连前**完整冻结并枚举** management、signal、relay、ICE/STUN/TURN、WG peer、DNS 七类外层端点，**全部排除后方可建连**；解析失败、端点变化或观测不符均 fail-closed（判据原文见 §2 首引）。
2. 以 **TUN 包级负证据**（外层探针唯一载荷不得出现在任何 TUN 帧）、**隧道正控**（发往 overlay 地址的探针必须在隧道内被观测）、**端点侧投递/源地址证明**（对端/端点记录收到每个外层探针及物理源地址）证明实际旁路；**API 返回值不作证据**（`docs/native-nx-governance.md:126`；`client/core/src/n2h.rs:22-41`）。
3. **计数对账**（隔离窗口前后设备计数与 TUN 独立清点对账）与 **VPN 撤销后复验**（`docs/native-nx-governance.md:126`；`client/core/src/n2h.rs:36-39`）。

### 1.2 本测量不证明什么

- **不证明逐 socket protect**。该义务在本平台为 `UNSAT/未满足`（`VpnConnection.protect(fd)` 与 `protectProcessNet()` 均被 `ohos.permission.MANAGE_VPN` 校验拦住、promise 不 settle、权限不可申请、声明即安装失败 9568289；前置取证门结论「无普通第三方可用的逐 socket bypass API」，`docs/native-nx-governance.md:122-123`）。证据与登记必须把该义务记为 `UNSAT/未满足` 并写明原因、替代机制、适用端点/前缀/拓扑/时间窗、oracle 结果与残余风险（`docs/native-nx-governance.md:131`）。
- **工具结论 `n2h-pass` 不是原 N2 判据的通过**（`client/core/src/n2h.rs:15-20` 治理范围声明；`docs/n2h-isolation-evidence-notes.md:114-115`），它只是「路由排除隔离」这一替代机制的单会话观测证据（`docs/n2h-isolation-evidence-notes.md:3`）。
- **主机侧/loopback 结果不代表设备路径**（`client/core/src/n2h.rs:89` residual；`docs/n2h-isolation-evidence-notes.md:85`）。主机侧回环全绿只是 harness 健康度回归，不是设备证据（`docs/device-relay-enablement-runbook-20260914.md:254-256`）。

### 1.3 边界与前置

- **Emulator/物理双向不外推**（`docs/native-nx-governance.md:35`）；Emulator 实测缺 VPN 授权注册组件、VpnExtension fd 不可执行（`docs/native-nx-governance.md:37`）；E 项证据只适用其目标元组、不得外推（`docs/evidence-schema.md:95`）。
- **pre-E8 物理例外条款适用**：E8 `CLOSED` 期间的物理工作按受治理例外进行——每次物理 campaign 独立 AUTH/pair、冻结元组与输入哈希、白名单 HDC、单次执行不重试不换 ID、双向不外推、禁止性能/长稳/渠道/产品外扩（`docs/native-nx-governance.md:53`，§三）。
- **诊断授权类别（例外二）不承载门证据**：正式门序列、任何冻结元组绑定测量不适用例外二，仍走既有 pair AUTH 路径（`docs/evidence-schema.md:122`）；例外二产物目录声明 `is_evidence:false`、不得继续引用（`docs/evidence-schema.md:128`）。若本轮产物要作为 N2-H 门证据，授权类别须人类/T0 明确选定（见 §5 末与 §9 不确定点①）。
- **目标元组冻结**：按证据 schema 必填字段登记目标设备、完整系统版本、架构、SDK/API/SysCap、代码 SHA、上游 SHA、工具链、命令、输入、时间戳与时区、制品哈希、原始日志引用（`docs/evidence-schema.md:36-58`）；证据 ID 按格式分配（`docs/evidence-schema.md:22,33-34`，一经分配不得复用或改写）。
- **N2-H 适用边界**：仅在「逐 socket bypass 无公开 API」结论成立的目标平台上适用；官方 SDK 一旦出现逐 socket 机制，优先回到 §二 第 4 条原路径（`docs/native-nx-governance.md:129`）。本判据不改变 §五 两条：「任何 bundle 排除测量须新 T0 决定」「own-bundle 排除仅可作纵深附加，不得单独构成通过」（`docs/native-nx-governance.md:70,:133`）。
- **AGPL 现行许可不受本测量影响**：许可路线按 `docs/native-nx-governance.md:98-109`（§四 修订，唯一现行）；上游 bump 机械合规与发布门义务不在本草案范围（`:105-106`），其「未验证声明」不得被读作已合规（`:108`）。
- **实现缺口现状（如实）**：N13 已登记「若要写 N2-H 结论还缺」——TUN 包级负证据与端点侧投递、relay TCP 端点的正式冻结（含 DDNS 换址即重冻纪律）、ICE/STUN 与 WG peer 端点冻结、出向数据面实测（`docs/n13-device-relay-evidence-20260916.md:133`）；ICE/STUN 与 WG peer 端点此前未纳入冻结（`docs/n13-device-relay-evidence-20260916.md:16`）。**设备侧 TUN 包级负证据的具体采集入口/脚本在仓库内未见实现**（`docs/device-relay-enablement-runbook-20260914.md:275,:358` 待确认③）——该实现属证据方法细化，须在本草案生效前一并落盘并随草案审查，不得测量后补写。

---

## 2. 判据（逐条、二元可判定）

**判据原文（逐字采用 T0 草案，2026-09-14 设立生效）**：

> 「N2-H：普通第三方公开逐 socket bypass 不可用时，可采用端点派生的最小主机路由排除。VPN 创建及每次连接/重连前，完整冻结并枚举 management、signal、relay、ICE/STUN/TURN、WG peer、DNS 外层端点；全部排除后方可建连，解析失败、端点变化或观测不符均 fail-closed。以 TUN 包级负证据、隧道正控及端点侧投递/源地址证明实际旁路；API 返回值不作证据；撤销后复验。原 E5 登记 deviation，结论仅为 N2-H pass。」
> —— `docs/native-nx-governance.md:125-126`

**测量窗口定义**：一个窗口 = 一次连接会话（工具每份证据文档只覆盖一个窗口；重连/撤销后复验 = 重新执行，`client/core/src/n2h.rs:38-39`；`docs/n2h-isolation-evidence-notes.md:27`）。

以下 C1–C9 每条均为二元判定（成立/不成立），全部成立且经独立审查，方可登记「N2-H pass」；任一条不成立按其标注记为 `n2h-fail`（直接证伪）或 `n2h-inconclusive`（无法确立），两者都**不是**通过类结论。

| # | 判据（二元） | 判定输入（oracle 输出） | 不成立时 |
|---|---|---|---|
| C1 | 冻结完整：`frozen_endpoints` 覆盖全部 7 类（management/signal/stun/turn/relay/wg_peer/dns）；必需类（management/signal/stun/wg_peer/dns）`present=true`；可选类（turn/relay）缺席时 `absent_reason` 非空 | 证据 JSON `frozen_endpoints[]`（`client/core/src/n2h.rs:171-187` 字段；必需/可选分类 `:151-153`） | `n2h-inconclusive`（fail-closed，不建连，`client/core/src/n2h.rs:824-849`） |
| C2 | 排除先于建连：设备侧排除集在 VPN create 被接受之前派生完成，且排除集 ⊇ 冻结解析集（按 /32，IPv4）；排除集来源含管理面当轮广告的 relay URL（广告即排除），DDNS 主机每次调用重新解析、零缓存 | hilog 时间序：`VPN_ENDPOINT_EXCLUSIONS_BEGIN` → `VPN_ENDPOINT_EXCLUSIONS\|hosts=\|resolved=` → `VPN_CREATE_RESOLVED\|accepted=true`（标记定义 `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:808,:871`）；排除集动态派生与 fail-closed 实现：commit `a545488`、`64989bd`；广告集合派生与轮换刷新：`docs/n13-device-relay-evidence-20260916.md:191` | `n2h-inconclusive`；若建连先于排除完成 → 按停止条件 S9 即停 |
| C3 | fail-closed 语义在位（反例守护）：测量前于主机侧回环模式复跑三个反例全绿——注入泄漏必须 `n2h-fail`、正控缺失必须 `n2h-inconclusive`、端点枚举缺失必须 `n2h-inconclusive` | `--fault-leak` / `--fault-no-posctl` / `--fault-omit-endpoint`（`client/core/src/bin/nbinterop.rs:254-261`；`client/core/src/n2h.rs:1203-1220`；治理登记 `docs/native-nx-governance.md:135`） | 反例未按预期判 → 测量无效，harness 不可信，停止并上报 |
| C4 | TUN 包级负证据：`tun_negative.hit == false` 且 `frames_scanned >= 1` 且 `scanned_sides` 覆盖本窗口实际读出的 TUN 侧 | 证据 JSON `tun_negative`（扫描方法 `client/core/src/n2h.rs:97`；任意外层探针载荷命中即证伪 `:813-822`） | 命中 → `n2h-fail`（**直接证伪，优先于一切**，`client/core/src/n2h.rs:798-810`）；零帧扫描 → `n2h-inconclusive` |
| C5 | 隧道正控：`tunnel_positive_control.observed_in_tunnel == true`（每条 sent 正控都在隧道内被观测，双向 a2b/b2a 至少各一条） | 证据 JSON `tunnel_positive_control`（`client/core/src/n2h.rs:400-423`；缺失/未观测永不通过 `:864-885`） | `n2h-inconclusive` |
| C6 | 端点侧投递/源地址：每个 `sent=true` 的外层探针在 `endpoint_side[]` 存在 `observed=true` 收执，且 `physical_src_source` 如实标注来源（endpoint-reported / probe-socket-getsockname / peer-device-offpath-drop） | 证据 JSON `endpoint_side[]`（三种方法 `docs/n2h-isolation-evidence-notes.md:25`） | `n2h-inconclusive`（外路投递未证明，`client/core/src/n2h.rs:887-902`） |
| C7 | 计数对账：`counters[]` 非空，每个窗口 `reconciled == true`（恒等式 `delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`） | 证据 JSON `counters[]`（恒等式 `client/core/src/n2h.rs:100,:558-576`；status JSON 已增补 `tx_bytes`/`rx_bytes_to_tun`，`docs/n2h-isolation-evidence-notes.md:100-104`） | `n2h-inconclusive`（`:904-919`） |
| C8 | 工具结论汇聚：本窗口 verdict == `n2h-pass`（退出码 0） | 证据 JSON `verdict`（判定顺序 `client/core/src/n2h.rs:798-926`；退出码表 `docs/n2h-isolation-evidence-notes.md:68-74`） | 按 `n2h-fail`（退出码 1）或 `n2h-inconclusive`（退出码 20）如实登记 |
| C9 | 撤销后复验：VPN destroy 后重跑一轮完整窗口（C1–C8 重复成立），且 destroy 后无残留 WSS 出站连接 | 新窗口证据 JSON；`/proc/net/dev` vpn-tun 计数与进程核查（先例 `docs/n13-device-relay-evidence-20260916.md:66`）；destroy 后不得有 WSS 出站（`docs/n13-relay-increment-plan-20260914.md:53-54`） | 任一不成立 → 复验不通过，如实登记 |

**结论与登记口径（强制）**：

- 终局口径措辞限制（`docs/native-nx-governance.md:130`）：结论只可写 **`N2-H pass`**；五项禁词清单见治理原文（:130），本文全程不复述其字面，文末自检见 §8。
- 逐 socket protect 义务在一切证据与登记中记 `UNSAT/未满足`，不得以任何措辞掩盖（`docs/native-nx-governance.md:131`）；固定记录字段 `unsat_note` 随每份证据文档输出（`client/core/src/n2h.rs:80`）。
- 残余范围必须明示（至少）：域名/CDN、DNS 上游、动态候选、重连换址、非 LAN/NAT 拓扑、宽前缀旁路、IPv6（`docs/native-nx-governance.md:132`）；工具以固定 `residual_scope` 八条随文档输出（`client/core/src/n2h.rs:84-93`）。
- 证据记录状态按 schema 枚举执行：执行后 `collected`，独立审查合格后方可 `reviewed-pass`；只有 `reviewed-pass` 可用于结论（`docs/evidence-schema.md:60-72`）。
- 记录纪律：任何一步未执行就写「未验证」，不得用推断或主机侧结果顶替（`docs/device-validation-plan-20260913.md:15`）。

---

## 3. oracle 与观测点

### 3.1 主 oracle：`nbinterop isolation-check` 证据文档

- 工具入口 `nbinterop isolation-check`，核心模块 `client/core/src/n2h.rs`（`docs/n2h-isolation-evidence-notes.md:4-5`）。
- 证据 JSON 字段（治理登记的口径，`docs/native-nx-governance.md:135`）：`frozen_endpoints` / `probes` / `tun_negative` / `tunnel_positive_control` / `endpoint_side` / `counters` / `verdict` / `residual_scope` / `unsat_note`；另有 `mode`/`session`/`generated_unix`/`started_mono`/`finished_mono`/`reasons`/`notes`（`client/core/src/n2h.rs:776-795`）。
- verdict 三值与退出码：`n2h-pass`=0 / `n2h-fail`=1 / `n2h-inconclusive`=20（`docs/n2h-isolation-evidence-notes.md:68-74`；预连模式恒 `n2h-inconclusive` 属设计行为，`docs/device-relay-enablement-runbook-20260914.md:240`）。
- CLI flag（`client/core/src/bin/nbinterop.rs:237-261`）：`--out <file>`、`--loopback`（默认回环）、`--config <file>`（预连冻结检查，只读 `management_url`、不读密钥，`client/core/src/n2h.rs:1810-1812`）、`--outer-endpoint <kind:host:port>`（可重复）、`--fault-leak` / `--fault-no-posctl` / `--fault-omit-endpoint <kind>`（反例注入，仅回环模式）。
- 路由表 / `/proc/net/route` / 配置回读 / API 返回值**只作辅助**，任何一项都不能单独产出通过类结论（`client/core/src/n2h.rs:40-41`；`docs/n2h-isolation-evidence-notes.md:28`）。

### 3.2 设备侧既有观测点（已实现，直接可用）

| 观测点 | 位置 | 用途 |
|---|---|---|
| `VPN_ENDPOINT_EXCLUSIONS_BEGIN\|relayRequired=` / `VPN_ENDPOINT_EXCLUSIONS\|hosts=\|resolved=` | `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:808,:871` | C2 排除先于建连的时间序与计数 |
| `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`（fatal）/ `_SKIPPED`（skip-with-log）/ `_URL_MALFORMED` / `_RELAY_STATE_UNKNOWN` | 同文件 `:881`、`:865`、`:840`、`:775`；fail-closed 两分支语义见 commit `64989bd` 提交说明 | fail-closed 语义与缺口留痕（C2/C3 佐证；触发即按 S8 处置） |
| 排除集动态派生（含 relay、广告即排除、DDNS 零缓存重解析） | commit `a545488`（动态派生+fail-closed+纯模块自检）、`64989bd`（消费 `relay_advertised.urls`）；core 侧兄弟段 `client/core/src/connector.rs:1184,:1612-1639,:2373`（先导 commit `95f6c02`） | C2 的实现依据；排除集按广告集合派生、轮换后随广告刷新（`docs/n13-device-relay-evidence-20260916.md:191`） |
| `VPN_VPNCONFIG_APPLIED\|routes=\|excludedRoutes=` | 生效链与计数逻辑见 `docs/device-relay-enablement-runbook-20260914.md:248` | 排除条目落地核对（辅助） |
| `VPN_RELAY_STATUS\|advertised=true\|advertisedUrls=` | 先例原文 `docs/n13-device-relay-evidence-20260916.md:48,:150-151` | 广告集合与冻结集一致性核对（辅助） |
| status JSON `tx_bytes` / `rx_bytes_to_tun` 增补 | `docs/n2h-isolation-evidence-notes.md:100-104` | C7 计数对账的设备侧读取 |
| `/proc/net/dev` vpn-tun RX/TX | 先例 `docs/n13-device-relay-evidence-20260916.md:66,:83` | C7/C9 的独立计数口径 |
| `WgDeviceStats`（含 `tun_write_errors` 聚合计数） | `client/core/src/wg_device.rs:274`（递增点 `:1018`） | OB-01/02 的粗粒度观察底座（见 §7） |

### 3.3 需新增的观测点 —— **依赖轨道 A（观测三件套）**

以下观测点当前**不存在**，须先由「轨道 A（观测三件套）」实现后方可纳入判据观测；在轨道 A 落地前，相应判据只能按既有观测点判定，缺口如实登记为「不可观测」：

| # | 新观测点 | 轨道 A 对应项（仓库依据） | 服务的判据/义务 |
|---|---|---|---|
| N1 | 状态 JSON 暴露 **per-peer 会话归属**（peer id / overlay IP ↔ 会话） | 观测三件套第 1 项（`docs/n13-device-relay-evidence-20260916.md:639,:649`） | C5 正控归因（证明正控对象是预期对端） |
| N2 | 探针**回执计数**（sent / acked） | 观测三件套第 2 项（`docs/n13-device-relay-evidence-20260916.md:640,:650`） | C6 端点侧投递的可计量形态 |
| N3 | **presence 转换打点**（`PeersOnline` / `PeersWentOffline` 逐行日志，带 peer id） | 观测三件套第 3 项（`docs/n13-device-relay-evidence-20260916.md:641,:651`） | 重连/轮换窗口的端点状态核查（C1/C2 重冻纪律） |
| N4 | tun 写路径 **EAGAIN / 部分写细分计数**（状态 JSON 新字段；现有 `tun_write_errors` 仅为聚合，`client/core/src/wg_device.rs:274`） | 依轨道 A 同批实现（同属状态 JSON 新字段） | OB-01/02 观察字段（§7） |

注：「轨道 A（观测三件套）」之名出自本次派发语境；仓库现行文档登记的是三项诊断清单本身（`docs/n13-device-relay-evidence-20260916.md:647-651`「未来若需真机级证明，先补三项诊断」），尚未以「轨道 A」为名立项——见 §9 不确定点③。

---

## 4. 停止条件（出现任一即停并上报；不重试、不绕过、不换通道）

**时钟与授权类**（依据：授权窗口时钟硬前置条款 1-10，`docs/native-nx-governance.md:144-153`）：

- **S1** hdc 不可达（`:148a`；先例处置 `docs/device-relay-enablement-runbook-20260914.md:335`「停止并上报，不得反复重试」）。
- **S2** 设备处于锁定/需要解锁状态（`:148b`；先例：`aa start` 被 `10106102`（屏幕锁定）拒绝即停、不计完成，`docs/n13-device-relay-evidence-20260916.md:423`）。
- **S3** 主机与设备时钟偏差超过 60 秒（T0 裁定阈值；`:148c`——偏差超阈时窗口比对结果不可信，属停止条件而非告警）。
- **S4** 窗口外/到点/剩余窗口不足以覆盖预计耗时：窗口外产物一律 `out_of_authorization_window:true`/`is_evidence:false`/`not_a_gate_pass:true`，不得作为证据、不得被后续 AUTH 追认；窗口中途到点立即停止并登记中断点，不得完成「手头这一步」（`:146,:149`）。
- **S5** AUTH 文件与 `.sha256` sidecar 校验不通过、同一 AUTH 存在多份不一致副本、`consumed:true` 或 `reusable:false` 且已消费（与窗口外同等处理）、设备时间不可读、时区未知、比对记录时刻倒退（`:150,:151,:152`）。
- **S6** 单次时钟比对超过 15 分钟未重比对（`:149`）。
- **S7** 同一 AUTH 并行执行或跨会话续用（`:153`）。

**测量与判据类**：

- **S8** 冻结/解析失败触发 `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`：该窗口 fail-closed 中止（这是判据设计行为，不是可重试故障）；**不得**换端点/换解析值拼凑窗口；DDNS 换址 = 新端点 = 重冻（新窗口），不得与旧窗口数据拼接（`docs/n13-relay-increment-plan-20260914.md:45-47,:62`；DDNS 须每次动态解析、不得硬编码，`docs/n13-device-relay-evidence-20260916.md:49`）。
- **S9** `n2h-fail`（任意外层探针唯一载荷命中任何 TUN 帧）：直接证伪成立，立即停止扩展测量、保全证据并上报（判定顺序：证伪优先于一切，`client/core/src/n2h.rs:798-822`）。
- **S10** 默认路由下 WSS 未先排除 relay TCP（`docs/n13-relay-increment-plan-20260914.md:53-54`）。
- **S11** 无路径正证据却报 relay 成功；ICE Failed 被当作可达；令牌过期静默假通；destroy/注销后仍有 WSS 出站连接（`docs/n13-relay-increment-plan-20260914.md:53-54`）。
- **S12** 开发期偏离通道被启用（`VPN_PROTECT_DEVIATION_ARMED|not_a_gate_pass=true` 一类标记出现）：该窗口不得作为证据，停止并上报（`docs/native-nx-governance.md:136`；`docs/device-relay-enablement-runbook-20260914.md:158-165,:250`）。
- **S13** 仪器异常（如 hilog 应用域被滤至 WARN+、流式抓取零应用行）：**先停止并上报再处置**，不得自行扩大读取限额或级别设置（教训登记：`docs/n13-device-relay-evidence-20260916.md:96-98`「同类仪器故障应先停止并上报再处置」）。
- **S14** 反例守护（C3）未按预期判定：harness 不可信，测量无效，停止上报。

---

## 5. AUTH 需求清单（供人类签发参考；allowed 逐字列出）

> 既有教训（登记原文）：`allowed` 的**逐字比对**是执行前的必要动作；起草时的「精简」必须与执行需要同步复核——任何需要的动作（含 hilog 级别设置这类细节）必须在 `allowed` 中显式列出（`docs/n13-device-relay-evidence-20260916.md:165-167`；偏差事实 `:154-159`）。以下为候选清单，**须人类逐条确认后逐字写入 AUTH**，未经确认不得执行。

**A. 连接维持（只读 + 连接）**
1. `hdc list targets`（仅限连接维持所需；禁止扩大用途的设备枚举）
2. `hdc tconn <目标设备地址>`（失败即停，不重试）

**B. 只读核验（含时钟硬门所需）**
3. 读取主机当前时间（带时区与 epoch）
4. `hdc shell date`（设备当前时间、时区、epoch）——**治理硬门要求：执行硬门所需的只读时间查询命令必须在该 AUTH 的 allowed 白名单内显式列出，未列出即不得执行设备操作**（`docs/native-nx-governance.md:151`）
5. `hdc shell "param get const.product.software.version"` 等只读系统参数（型号/版本/API 版本）
6. `hdc shell "bm dump -n cn.alfadb.netbird"`、`hdc shell "pidof cn.alfadb.netbird"`
7. `hdc shell "cat /proc/net/dev"`（vpn-tun 计数，C7/C9 口径）

**C. 安装与启动**
8. `hdc file send`（设备配置、setup key 文件、启动脚本；600 权限、凭据不回显，纪律见 `docs/device-relay-enablement-runbook-20260914.md:77-81`）
9. `hdc install -r <signed HAP>`（逐次登记 sha256）
10. `hdc shell "sh /data/local/tmp/start-netbird.sh"` 或 `aa start`
11. `hdc shell "uitest uiInput click <x> <y>"`（每次点击的坐标/控件/目的逐条登记）；`uitest dumpLayout`（只读重定位）

**D. 日志**
12. `hdc shell hilog` 清缓冲与抓取（限本项目 tag，如 NetBirdVpn/N1BDiscVpn，非持久）
13. **`hilog` 日志级别设置（限本项目 tag、非持久、重启失效）**——单独逐字列出；2026-09-16 轮该动作不在 AUTH 字面内被登记为已执行偏差、不得追认不得作为先例（`docs/n13-device-relay-evidence-20260916.md:154-159`）

**E. 测量本体**
14. TUN 包级负证据、隧道正控、端点侧投递、计数对账、VPN 撤销后复验等设备侧测量（工具口径 = `nbinterop isolation-check` 及其等价探针；先例授权措辞见 `docs/device-relay-enablement-runbook-20260914.md:275`）
15. 主机侧（非设备操作，一并登记）：`nbinterop isolation-check` 回环反例守护三连跑；如需对端/正控，启动与停止主机侧 `nbinterop peer` 进程

**F. 收尾**
16. `hdc shell "aa force-stop cn.alfadb.netbird"`
17. `hdc uninstall cn.alfadb.netbird`（≤1 次）
18. `/data/local/tmp` 命名空间内本项目临时文件清理与复核
19. `hdc rport ls` / `rport rm`（或 fport，若曾建立反向转发）

**G. 消费语义与禁止项（结构性，随 AUTH 文件固定）**
20. AUTH 文件须含 `consumed` / `reusable` / `retry_allowed` / `successor_auth` 四字段（`docs/evidence-schema.md:126`）；过期即失效、逾期须新签、不得顺延（`docs/evidence-schema.md:125`；`docs/native-nx-governance.md:147`）。
21. 禁止项沿用既有口径（先例全文见 `docs/device-relay-enablement-runbook-20260914.md:337-350`）：不产生 verdict/claim/门级结论；不引用/消费任何 campaign/pair/evidence ID（含已 consumed 历史ID）；root/privileged 操作；设备枚举超维持所需；非本项目 bundle；`/data/local/tmp` 外写操作；把诊断产物写进仓库或标记为 evidence、省略 `.sha256` sidecar。
22. 凭据纪律：秘密只落 600 权限文件，不进命令行/日志/URL/文档；证据记录中一律占位符（`docs/evidence-schema.md:134-142` 脱敏规则）。

**授权类别前置（须人类/T0 明确决定，本草案不假定）**：N2-H 设备侧证据若要作为门证据，属正式门序列的冻结元组绑定测量，**不适用诊断例外二**，须走 §三 pre-E8 物理例外的 campaign 类 pair AUTH 路径并经用户逐项显式授权（`docs/native-nx-governance.md:53,:96`；`docs/evidence-schema.md:122,:128`）；若人类选择先以诊断类别做开发性采证，则该轮产物 `is_evidence:false`，**不得**事后作为 N2-H 门证据引用（`docs/evidence-schema.md:128`）。

---

## 6. 步骤 0：授权窗口时钟比对（硬门前置，先回报再动手）

依据：授权窗口时钟硬前置条款 1-10（`docs/native-nx-governance.md:144-153`，2026-09-15 追加，只向前生效）；操作化先例 `docs/device-relay-enablement-runbook-20260914.md:28-41`（§1.0 步骤 0，缘于 2026-09-15 越界执行教训：执行者读过 AUTH 但未比对时刻、派发稿口头声称在窗口内，超窗约 4 小时 9 分）。

在发出**任何设备命令**（含清理、卸载、撤销、force-stop、安装、点击、抓日志）之前，执行者必须：

1. **记录三时钟**：主机当前时间（带时区 + epoch）、设备当前时间（带时区 + epoch）、当次 AUTH 的 `valid_from`/`valid_until` **原值**（`docs/native-nx-governance.md:144`）。
2. **三方比对**：主机时刻与设备时刻**均**落于窗口内，方可执行第一个设备命令（`:144`）。
3. **先回报再动手**：派发稿写明 `auth_id` 与窗口原文；执行者未回报比对结果前，任何设备命令不得发出；派发稿中关于「是否在窗口内」的一切非引用性声称不构成依据（`:145`）。
4. **比对结果写入运行记录**：主机/设备各自具体时刻、时区、`valid_from`/`valid_until` 原值、窗口内/外结论——**无比对记录即视为未比对**（`:144`）。
5. **epoch 比较**：一切比较在带时区的绝对时刻（epoch）上进行，禁止墙钟字符串比较；本次主机时刻不得早于同一 AUTH 上一次已登记的 `checked_at`，倒退即时钟不可信按停止条件处理；禁止为通过比对修改主机或设备系统时间（`:152`）。
6. **一并读取消费状态**：`consumed` / `reusable` / `retry_allowed`；`consumed:true` 或 `reusable:false` 且已消费 → 与窗口外同等处理（`:150`）。
7. **比对有效期 ≤ 15 分钟**：超期重比对；操作序列预计结束时刻必须仍在窗口内；窗口中途到点立即停止并登记中断点（`:149`）。
8. **失败即停**：hdc 不可达、设备锁定、偏差 > 60 秒 → 立即停止并上报（`:148`；即 §4 之 S1/S2/S3）。

---

## 7. OB 处置表（对照 `docs/open-obligations-ledger.md`）

使用纪律依据：判据预注册时，起草人须核对台账中归属本门的条目并逐条给出处置（承担/观察/转移去向），遗漏即审查 blocker（`docs/open-obligations-ledger.md:11`）；条目的新增、修改、关闭仅经 T0 决议（`:13`）；关闭不改写、登记证据 ID 与日期（`:14`）。实现期口径（2026-09-12 追加节）：OB-01/02/03/04/07 五条须专门测量的义务作为**实现期定向验证项**，由各实现里程碑的判据预注册逐条处置并登记观测点、随该门受治理验证一并实测落盘，不再单发独立测量 campaign（`docs/open-obligations-ledger.md:49`）。

**前提声明（OB-03/04/07 专属）**：三条的 `pending-discovery` 状态以其「关闭触发」引用的 DISC D-W / D6 实测结果为前提（`docs/open-obligations-ledger.md:31,:32,:35`）；N1BDISC campaign 已登记**终态关闭**（`docs/open-obligations-ledger.md:49`），上述 DISC 前提**已随 campaign 关闭而消失且从未取得结果**——正式改派持有门须 T0；**本表的处置只是收敛建议，不是决定**。

| OB | 内容（台账行） | 本草案处置建议 | 观测点/字段 | 备注 |
|---|---|---|---|---|
| OB-01 | tun fd 背压实测（`:29`；持有门 N1b（尝试）→ N6/N7（承接观察）） | **观察**：本测量窗口顺带登记被动观察点，承接 N6/N7 | `tun_eagain_on_write_observed`（默认 false；观测到 true 须登记上下文，台账 :29 预登记字段名） | 粗粒度底座 = `tun_write_errors` 聚合计数（`client/core/src/wg_device.rs:274`）；EAGAIN 细分依赖轨道 A（§3.3-N4）。关闭仍按原触发（真实 tun fd 上观测到 `induced` 并落盘） |
| OB-02 | tun fd 部分写实测（`:30`；同 OB-01 口径） | **观察**：同 OB-01 | `tun_partial_write_observed`（台账 :30 预登记字段名） | 同上 |
| OB-03 | shutdown unblock 实测（`:31`；持有门「待 DISC D-W 结果指定」，状态 `pending-discovery`） | **转移建议**：并入 N6/E7 短循环承接并作书面登记（其预授权出口②形态）；**正式改派须 T0**（DISC D-W 前提已消失，见上前提声明） | 本测量仅顺带观察 destroy 后同步等待行为，不设关闭判据 | 本草案不主张关闭 |
| OB-04 | post-destroy fd 子项（destroy 后 fd 有效期、重复 close 的不可观察部分）（`:32`；取决 DISC D6 对 U4 的实测，`pending-discovery`） | **转移建议**：并入 N6-E7；**正式改派须 T0**（D6 未测，前提已消失） | 同上，仅观察登记 | 同上 |
| OB-07 | E4 MTU 实际生效 oracle（`:35`；`pending-discovery`） | **不承担**：本测量不判定 MTU 生效；仅记录 vpn-tun MTU 声明值（如 1400）作观察项。若本元组无生效 oracle，该事实落盘且持有门转 `unassigned`、须下一次相关 T0 指派（台账 :35 原口径） | `mtu_declared`（观察字段）；**约 1052 B 测试包不能区分生效 1280 与 1400，不构成证据**（`:35` 原文） | `pending-discovery` 前提已消失（见前提声明），正式改派须 T0 |
| OB-08 | E4 声明范围内 IPv6 核验（`:36`；由声明 IPv6 的门承担，触发条款） | **未触发/观察**：本门冻结配置**不声明** IPv6 地址或路由（工具 IPv4-only，`client/core/src/n2h.rs:92`；治理残余范围明示 IPv6，`docs/native-nx-governance.md:132`） | `ipv6_declared=false`（登记冻结配置的 IPv6 声明状态；N1b 先例为「无 IPv6 声明 → 登记为 undeclared 形态」，台账 :36） | 触发条款保持活络：任何后续门冻结配置一旦声明 IPv6，该门同时承担核验并须在其判据预注册显式列出（缺列即审查 blocker，`:36`） |

**核对完整性**：台账其余三条不属本门、本预注册不触碰——OB-05（E4 平台 DNS，持有门 N2a + N2 撤销后复验，`:33`）、OB-06（E4 多地址/多路由/路由表交互/MTU 变更与重建，持有门 N6，`:34`）、OB-09（首轮 IPv6 能力与失败边界探测，持有门 N6、观察类非门控，`:37`）。其中 OB-05 的「N2 撤销后复验」与本测量 C9 的撤销后复验在动作上重合，但 OB-05 关闭仍以其持有门判据登记为准，本测量不复用其结论。

---

## 8. 不外推声明与禁止措辞自检

### 8.1 不外推声明

1. **Emulator/物理双向不外推**（`docs/native-nx-governance.md:35`）：本预注册及未来测量结论只覆盖其冻结目标元组（具名设备、完整系统版本、架构、SDK/API/SysCap、代码与制品哈希，`docs/evidence-schema.md:36-58`），不外推其他设备、版本、渠道或 Emulator；反向亦然。
2. **单会话窗口边界**：每份证据文档只覆盖一个连接会话窗口；重连、撤销后复验、重解析/换址后的新端点均须重新执行（`client/core/src/n2h.rs:38-39,:88`；`docs/n2h-isolation-evidence-notes.md:27,:84`）。
3. **残余范围（固定随文档输出，八条）**：域名/CDN（按冻结时刻解析枚举，重解析/换址不在证据内）；DNS 上游递归路径不受观测；动态候选（srflx/relayed 派生）外层映射未覆盖；重连换址须重新执行；非 LAN/NAT 拓扑（主机侧 loopback 不代表物理路径）；宽前缀旁路（仅证被枚举 /32 零命中）；端点侧日志（真实服务端投递日志须运维侧提供）；IPv6（TUN 帧解析与端点枚举均 IPv4-only）（`client/core/src/n2h.rs:84-93`；治理要求至少明示七项，`docs/native-nx-governance.md:132`）。
4. **平台适用边界**：仅在「逐 socket bypass 无公开 API」结论成立的目标平台适用；官方 SDK 出现逐 socket 机制时优先回原路径（`docs/native-nx-governance.md:129`）。
5. **主机侧结果不外推至设备**；主机侧回环与反例守护仅证明 harness 与判定引擎行为，不构成设备证据（`client/core/src/n2h.rs:43-51`；`docs/device-relay-enablement-runbook-20260914.md:254-256`）。
6. **本草案自身不外推**：未生效前不作为任何测量依据；生效后亦只约束其登记的测量窗口与判据，不构成对其他门（N6/N7/E4 等）任何义务的替代或减免（映射不免除义务原则，`docs/native-nx-governance.md:44`）。

### 8.2 禁止措辞自检（实跑记录）

自检对象：本文件全文。禁词清单 = 治理终局口径措辞限制所列五项（字面见 `docs/native-nx-governance.md:130`；`client/core/src/n2h.rs:2347-2349` 与 `docs/n2h-isolation-evidence-notes.md:30-32` 以测试断言在证据 JSON 上钉死同一清单）。为避免本文自身命中，自检模式以字符类拆分书写：

```bash
grep -niE 'protec[t] pass|N2 pas[s]|等同逐 socke[t] protect|waive[d]|N/[A]' \
  docs/n2h-device-evidence-preregistration-draft-20260917.md
```

实跑结果（2026-09-17，起草会话内实跑）：**0 命中，退出码 1（grep 无匹配）**——五项禁词字面在本文件全文零出现。

「N2-H pass」唯一许可形态出现次数（应仅存在于引用治理原文与结论口径处）：

```bash
grep -c 'N2-H pass' docs/n2h-device-evidence-preregistration-draft-20260917.md
```

实跑结果（同上）：**5 次**（§2 判据原文引文、§2 结论口径、§8.2 本节说明性引用等，均为许可形态）。

另登记：`git status --porcelain` 实跑仅见本新文件（`?? docs/n2h-device-evidence-preregistration-draft-20260917.md`），未修改任何既有文件。

---

## 9. 不确定点清单（供审查席关注）

1. **授权类别未定**：N2-H 设备侧证据须走 campaign 类 pair AUTH（pre-E8 物理例外）还是先以诊断类别采证（产物 `is_evidence:false`、不得事后升级为门证据）——须人类/T0 明确（§5 末）。本草案不假定。
2. **设备侧 TUN 包级负证据采集入口未实现**（`docs/device-relay-enablement-runbook-20260914.md:275,:358` 待确认③）：实现属证据方法，须在本草案生效前落盘并随草案一并审查（§1.3）。
3. **「轨道 A（观测三件套）」未在仓库文档立项**：现行登记仅三项诊断清单（`docs/n13-device-relay-evidence-20260916.md:647-651`）；§3.3 的 N1–N4 观测点以其落地为前提，落地前相应判据按「不可观测」如实登记。
4. **端点侧投递收执的外部依赖**：真实服务端/中继侧投递日志须运维侧提供（`client/core/src/n2h.rs:91`；`docs/device-relay-enablement-runbook-20260914.md:273`）；未取得前 C6 只能以 STUN 自证响应与对端设备计数闭合，relay TCP 的端点侧收执须提前与运维约定观测窗口，否则该腿记缺口、不得假通。
5. **OB-03/04/07 正式改派须 T0**：本表处置仅为收敛建议（§7 前提声明）。
6. **回环反例守护 ≠ 设备反例**：C3 在主机侧回环模式验证判定引擎；设备侧无法注入泄漏反例（无 root），设备窗口的 fail 判定依赖真实观测，不存在主动反例验证。
7. **profile/签名绑定待确认**（先例 `docs/device-relay-enablement-runbook-20260914.md:356-359` 待确认①④）：本草案沿用的排除集已是运行时动态派生（commit `a545488`/`64989bd`），早先「构建前改常量」的待确认④已过时，但签名 profile 绑定仍须每轮确认。
