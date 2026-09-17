# N2-H 设备侧证据判据预注册——草案（DRAFT，未生效；修订第 2 版）

- **文件性质：DRAFT / 未生效**。本文件是供人类与独立审查席（T0）审查的预注册**草案**，不是已生效的预注册。本版为**修订第 2 版**：按 T0 独立审查（2026-09-17，裁定「**修订后可生效**」，审查记录 `docs/n2h-preregistration-t0-review-20260917.md`）逐项处置（文末「修订记录」），初稿为 commit `b710741`（257 行）。修订后**仍是 DRAFT**，生效前置见下。
- **生效条件与前置**：判据、oracle、停止条件、不外推声明须在测量开始前书面登记并**经独立审查确认**，测量后不得追认或修改（`docs/native-nx-governance.md:43`，§二 绑定条款第 7 条；`:35` 「开门前细化只能细化证据方法，不得删改或削弱」）。本草案在独立审查确认前**不生效**。T0 生效前置逐项：
  - **G-1** 设备侧采集实现落盘并随修订稿再审（实现已落盘 I0–I7，见 §3.1；本修订稿连同 C4/C5/C7/C8 逐字段对齐清单待 T0 复审）；
  - **G-2** 证据 ID 门代码与授权类别先定（均已定：门代码 `N2H` 入 schema，授权类别时序见 §1.3/§5 末）；
  - **G-3** 全部必改项关闭（本版逐项处置，见文末修订记录；是否成立由 T0 复审认定）；
  - **G-4【新增要求】生效程序（sha256 锚定）**：本草案经审查确认「可生效」后，**生效稿 = 定稿全文的 sha256 快照**——①定稿时计算全文 sha256 并登记（登记于仓B `records/`，文件名含日期与「effective」字样）；②**首次据以测量的 campaign AUTH 须逐字引用该 sha256**（AUTH 绑定登记）；③此后本文件任何再修改 = 新版本 = 须重新审查并重新登记 sha256。没有该锚点，治理 `:43`「测量后不得追认或修改」不可机械验证。
- **测量前置**：任何设备侧测量须**人类签发的生效 AUTH**；本草案本身不是授权（先例口径：`docs/device-validation-plan-20260913.md:9-15` 「本计划本身不是授权」）。
- **在生效之前，未来任何测量不得以本草案为依据；不得引用本草案登记任何测量、verdict 或门级结论。**
- **当前状态基线**：设备侧 N2-H 证据尚未产出；在产出并经独立审查前，本平台 N2 相关结论一律为 `UNSAT + 替代测量进行中`，且开发期偏离（`--ps protectDeviation route-exclusion`，日志带 `is_evidence=false|not_a_gate_pass=true`）不构成任何门证据（`docs/native-nx-governance.md:136`）。
- 起草日期：2026-09-17（Asia/Shanghai）。修订日期：2026-09-17（同日，吸收 T0 裁定与同日落地的仓内事实）。修订过程：只读核对仓库文档与代码（基线 HEAD `5beb084` 实读）；未执行任何设备命令、未修改其他任何文件、未运行构建。

---

## 1. 目标与范围

### 1.1 本测量要证明什么

在目标物理设备的**一个真实连接会话隔离窗口**内，由**设备侧证据采集器**（运行在 App 进程内的 core 模块 `client/core/src/n2h_collector.rs`，I0–I7 落地，随 `libnetbird_core.so` 打进签名 HAP；与主机侧 `nbinterop isolation-check` 共用同一 schema 与判定引擎 `client/core/src/n2h.rs`，见 §3.1）产出 N2-H 平台替代判据要求的设备侧证据：

1. 建连前**完整冻结并枚举** management、signal、relay、ICE/STUN/TURN、WG peer、DNS 七类外层端点，**全部排除后方可建连**；解析失败、端点变化或观测不符均 fail-closed（判据原文见 §2 首引）。
2. 以 **TUN 包级负证据**（外层探针唯一载荷不得出现在任何 TUN 帧）、**隧道正控**（发往 overlay 地址的探针必须在隧道内被观测）、**端点侧投递/源地址证明**（对端/端点记录收到每个外层探针及物理源地址）证明实际旁路；**API 返回值不作证据**（`docs/native-nx-governance.md:126`；`client/core/src/n2h.rs:22-41`）。
3. **计数对账**（隔离窗口前后设备计数与 TUN 独立清点对账）与 **VPN 撤销后复验**（`docs/native-nx-governance.md:126`；`client/core/src/n2h.rs:36-39`）。

### 1.2 本测量不证明什么

- **不证明逐 socket protect**。该义务在本平台为 `UNSAT/未满足`（`VpnConnection.protect(fd)` 与 `protectProcessNet()` 均被 `ohos.permission.MANAGE_VPN` 校验拦住、promise 不 settle、权限不可申请、声明即安装失败 9568289；前置取证门结论「无普通第三方可用的逐 socket bypass API」，`docs/native-nx-governance.md:122-123`）。证据与登记必须把该义务记为 `UNSAT/未满足` 并写明原因、替代机制、适用端点/前缀/拓扑/时间窗、oracle 结果与残余风险（`docs/native-nx-governance.md:131`）。
- **工具结论 `n2h-pass` 不是原 N2 判据的通过**（`client/core/src/n2h.rs:15-20` 治理范围声明；`docs/n2h-isolation-evidence-notes.md:114-115`），它只是「路由排除隔离」这一替代机制的单会话观测证据（`docs/n2h-isolation-evidence-notes.md:3`）。
- **主机侧/loopback 结果不代表设备路径**（`client/core/src/n2h.rs:89` residual；`docs/n2h-isolation-evidence-notes.md:85`）。主机侧回环全绿只是 harness 健康度回归，不是设备证据（`docs/device-relay-enablement-runbook-20260914.md:254-256`）。
- **运维收执缺位时的 `n2h-inconclusive` 是设计行为，不是可自证的腿**：对端自身 `unknown_peer_drops`（活值）当前无设备侧载体，mgmt/signal/relay TCP 的服务端投递日志须运维提供——缺位时设备轮 verdict 预期恒为 `n2h-inconclusive`（设计 `docs/n2h-device-collector-design-20260917.md:186`（§4.0 诚实结论）与 §13.10④；不得写成可自证）。

### 1.3 边界与前置

- **Emulator/物理双向不外推**（`docs/native-nx-governance.md:35`）；Emulator 实测缺 VPN 授权注册组件、VpnExtension fd 不可执行（`docs/native-nx-governance.md:37`）；E 项证据只适用其目标元组、不得外推（`docs/evidence-schema.md:96`）。
- **pre-E8 物理例外条款适用**：E8 `CLOSED` 期间的物理工作按受治理例外进行——每次物理 campaign 独立 AUTH/pair、冻结元组与输入哈希、白名单 HDC、单次执行不重试不换 ID、双向不外推、禁止性能/长稳/渠道/产品外扩（`docs/native-nx-governance.md:53`，§三）。
- **执行者纪律与窗口估算（显式登记）**：执行者**单一持有**（单执行者、单会话），**单次执行不重试、不换 ID**（`docs/native-nx-governance.md:53` pre-E8 条款原文）；窗口长度上界＝采集器窗口 `WINDOW_DEADLINE_MS = 60 000 ms`（`client/core/src/n2h_collector.rs:1369`），其中观察期上界 `OBSERVE_DEADLINE_MS = 5 000 ms`（`:1372`）加 settle 尾拍（`OBSERVE_SETTLE_ROUNDS = 15`，`:1378`）；整轮设备操作的预计耗时随 AUTH 窗口逐轮登记（`valid_from`/`valid_until` 原值即窗口估算的登记载体，§6）。
- **授权类别（已定，时序与效力边界）**：人类决定**先以诊断类 dry-run（例外二）验证机制，再签 campaign 类正式轮**。时序：①**诊断 dry-run 轮**——按例外二（诊断授权类别，`docs/evidence-schema.md:121-129`）签发 session-scoped AUTH：不消费/不引用任何 campaign/pair/evidence ID、不产生任何 verdict/claim/门级结论、产物目录声明 `is_evidence:false`（`:129`），用于验证采集器机制、操作面（runbook §9）与观测面；其产物**永远不得事后升级为门证据**（`:129` 隔离语义），也**不分配任何 `EV-N2H-*` ID**、不据本草案登记任何 verdict。②**campaign 正式轮**——机制验证收敛后，走 §三 pre-E8 物理例外的 pair AUTH 路径并经用户逐项显式授权（`docs/native-nx-governance.md:53,:96`），其产物才进入门证据序列（schema 状态枚举 `collected` → `reviewed-pass`，`docs/evidence-schema.md:61-73`）。本预注册的判据约束的是②正式轮；①dry-run 轮不受本预注册判据约束、也不为其供证。
- **证据 ID 门代码（已定）**：人类批准新增证据门代码 `N2H`（`docs/evidence-schema.md:28`，commit `1390ef4`「追认登记式扩展，人类批准 2026-09-17」；仓B 登记 `records/gate-code-n2h-registration-20260917.json`）。判据/证据 ID 形态统一为 `EV-N2H-<目标代码>-<YYYYMMDD>-<序号>`（`docs/evidence-schema.md:22,:28`）；ID 一经分配不得复用或改写（`:35`）；agent 不得自分配 AUTH ID（`:128`）。
- **目标元组与输入哈希冻结（生效前冻结，不留到测量时）**：生效稿登记下列冻结值，测量轮逐项比对（S17）：具名设备、完整系统版本（`param get` 读数口径）、架构、SDK/API/SysCap、渠道、**代码 SHA＝该轮实际构建并安装的 commit（生效登记时锚定，M-5 更正；起草读码基线 `5beb084` 不得登记为测量轮冻结值）**、上游 SHA、工具链（OS/base image、Command Line Tools、SDK/API、HDC、Rust、Hvigor、ohpm）、命令（§5 逐字清单）、输入（配置、拓扑、样本、前置状态）、时间戳与时区、**制品哈希**与原始日志引用（`docs/evidence-schema.md:37-59` 必填字段）。**制品冻结纪律（HAP 打包不可逐字节复现）**：同一工作树多次打包得到不同 sha256（设计 `docs/n2h-device-collector-design-20260917.md` §13.10⑦、§13.11 实测）——本轮**只打包一次**，登记**实际安装的那个 HAP 文件**的 sha256（含路径、size、时刻与签名产物名，`HAP-FREEZE.txt` + `MANIFEST.sha256`，runbook §9.1/§9.2）；**冻结后不得重新打包，不得用新包 sha256 回填旧一轮证据**（runbook `docs/device-relay-enablement-runbook-20260914.md:375-382`）；「证据 ↔ 构建输入」由 `client/core/build.sh` 末行打印的 `N2H_BUILD_SHA256` 源码摘要承担（可复现、任何源码改动都会改变它，设计 §13.11①；设备文档 `collector.build_sha256` 在 `build_source="embedded"` 下与它逐字相同，runbook §9.1-3/4）。**代码 SHA 三点比对（M-5）**：`N2H_BUILD_SHA256` 只是 crate 构建输入（`Cargo.toml`/`Cargo.lock`/`build.rs`/`proto`/`src`）的确定性摘要，**不携带 commit 信息**——测量轮须以 ①构建工作树 `git rev-parse HEAD`＝该轮登记的 commit、②`build.sh` 末行 `N2H_BUILD_SHA256`＝该轮登记值、③设备文档 `collector.build_sha256` 与②逐字相同，三点比对，任一点不符即停（详见 §4-S17）。
- **N2-H 适用边界**：仅在「逐 socket bypass 无公开 API」结论成立的目标平台上适用；官方 SDK 一旦出现逐 socket 机制，优先回到 §二 第 4 条原路径（`docs/native-nx-governance.md:129`）。本判据不改变 §五 两条：「任何 bundle 排除测量须新 T0 决定」「own-bundle 排除仅可作纵深附加，不得单独构成通过」（`docs/native-nx-governance.md:70,:133`）。
- **AGPL 现行许可不受本测量影响**：许可路线按 `docs/native-nx-governance.md:98-109`（§四 修订，唯一现行）；上游 bump 机械合规与发布门义务不在本草案范围（`:105-106`），其「未验证声明」不得被读作已合规（`:108`）。
- **实现缺口现状（如实，HEAD `5beb084`）**：设备侧采集器已落盘（I0–I7，commits `cf0a031`/`49ad574`/`8b11200`/`568c931`/`e6e4f3a`/`8929695`/`c3f0832`/`5beb084`；§3.1）。残余缺口两条，如实登记且不阻碍草案完整性：①**壳侧排除派生只覆盖 3/7 类**（`NetBirdEndpointExclusion.ets:45-51` 的 `kind` 稳定 token 仅 `management`/`signal`/`relay`）——冻结面已由采集器覆盖 7/7，排除面未补齐前本轮预期 C2 不成立（见 §2-C2 与 §9-3），**不得登记通过类结论**；②**N4（tun 写路径 EAGAIN/部分写细分）未落地**（`client/core/src/wg_device.rs:274` 对 TUN 写失败一律聚合进 `tun_write_errors`，递增点 `:1154`，无 EAGAIN/部分写细分）→ OB-01/02 观察字段本轮只能登记 `unobservable`（§7）。

---

## 2. 判据（逐条、二元可判定）

**判据原文（逐字采用 T0 草案，2026-09-14 设立生效）**：

> 「N2-H：普通第三方公开逐 socket bypass 不可用时，可采用端点派生的最小主机路由排除。VPN 创建及每次连接/重连前，完整冻结并枚举 management、signal、relay、ICE/STUN/TURN、WG peer、DNS 外层端点；全部排除后方可建连，解析失败、端点变化或观测不符均 fail-closed。以 TUN 包级负证据、隧道正控及端点侧投递/源地址证明实际旁路；API 返回值不作证据；撤销后复验。原 E5 登记 deviation，结论仅为 N2-H pass。」
> —— `docs/native-nx-governance.md:125-126`

**测量窗口定义**：一个窗口 = 一次连接会话内的**一个采集器窗口**（arm → close：控制文件 `/data/local/tmp/netbird-n2h-arm` 触发 ets 钩子 `maybeArmN2hCollector`（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:967`）→ core 窗口状态机有界运行（上界 60 s，`n2h_collector.rs:1369`）→ 定稿输出证据文档）。每份设备证据文档只覆盖一个窗口；重连/撤销后复验/换址 = 新窗口重新执行（主机侧同口径 `client/core/src/n2h.rs:38-39`；`docs/n2h-isolation-evidence-notes.md:27`；arm 尝试为每 ability 实例至多一次的 latch，`NetBirdVpnExtensionAbility.ets:305,:968-970`——复验窗口须新会话实例）。

以下 C1–C10 每条均为二元判定（成立/不成立），全部成立且经独立审查，方可登记「N2-H pass」；任一条不成立按其标注记为 `n2h-fail`（直接证伪）或 `n2h-inconclusive`（无法确立），两者都**不是**通过类结论。

| # | 判据（二元） | 判定输入（oracle 输出，带出处） | 不成立时 |
|---|---|---|---|
| C1 | 冻结完整：`frozen_endpoints` 覆盖全部 7 类（management/signal/stun/turn/wg_peer/dns）；必需类（management/signal/stun/wg_peer/dns）`present=true`；可选类（turn/relay）缺席时 `absent_reason` 非空 | 设备证据 JSON `frozen_endpoints[]`（元素结构 `n2h.rs:171-206`；必需/可选分类 `n2h.rs:151-153`；设备侧七类装配来源 token＝`shell-exclusion`/`core-network-map`/`core-relay-advertised`/`core-signal-config`/`core-dial-address`/`wg-device-outer-socket`，`n2h_collector.rs:359-373`；冻结时刻＝arm 时刻——`arm_json`（`n2h_collector.rs:1953`）内 `FreezeSources{…}` 字面量 `:1989-2000`（`frozen_at_unix:1998` / `frozen_at_mono:1999`）＋`assemble_freezes(&sources)` `:2001`（M-3 更正指针：原引 `:1878` 实为 shell_exclusions 解析中 `ShellExclusion::with_resolved` 的实参行 `:1876-1881`，语义主张不变；设计 §13.2）；判定＝judge 第 2 步 `n2h.rs:824-849`） | `n2h-inconclusive`（fail-closed，不建连；无会话时 arm 被拒不开窗，`n2h_collector.rs:1390-1395`） |
| C2 | 排除先于建连：设备侧排除集在 VPN create 被接受之前派生完成，且**排除集 ⊇ 冻结解析集（按 /32，IPv4）**；排除集来源含管理面当轮广告的 relay URL（广告即排除），DDNS 主机每次调用重新解析、零缓存 | hilog 时间序：`VPN_ENDPOINT_EXCLUSIONS_BEGIN\|relayRequired=`（`NetBirdVpnExtensionAbility.ets:869`）→ `VPN_ENDPOINT_EXCLUSIONS\|hosts=\|resolved=`（`:932`）→ `VPN_CREATE_RESOLVED\|accepted=true`（`:1585`；初稿引文未给行号，T0 B-4 补齐：审查基线 `b710741` 时为 `:1391`，I0–I7 ets 增码后本修订版 HEAD 实读 `:1585`）。**集合包含关系的字面判定（B-4）**：以 `VPN_ROUTE_ENTRY\|…\|dest=\|prefix=\|isExcludedRoute=` 逐行比对（`NetBirdRouteValidation.ets:245-253`；N13 先例即用此行，`docs/n13-device-relay-evidence-20260916.md:49`）；「逐条比对通过」＝对冻结解析集中每个 IPv4 /32 地址 A，行集中恰存在一行满足 `dest==A ∧ prefix=="32" ∧ isExcludedRoute=="true"`，且行集完整可解析；任一地址无此行、值不符、行集缺失或不可解析 → C2 不成立（不得以计数、合计或抽样顶替）。排除集动态派生与 fail-closed：commit `a545488`、`64989bd`；广告集合派生与轮换刷新：`docs/n13-device-relay-evidence-20260916.md:191`。**覆盖面现状（B-2，显式登记）**：壳侧排除派生 `kind` 仅 management/signal/relay 三类（`NetBirdEndpointExclusion.ets:45-51`）——冻结面 7/7（采集器）、排除面 3/7，**本轮预期 C2 不成立 → `n2h-inconclusive`**；未排除的必需类（stun/wg_peer/dns）探针若因此进入 TUN，则 C4 命中 → `n2h-fail`（对「全部排除后方可建连」的真实证伪，属判据设计行为，非工具故障，设计 §10-1）；判据不削弱（治理 `:126` 要求七类全排除），**排除面补齐 7/7 前不得登记 N2-H pass** | `n2h-inconclusive`；若建连先于排除完成 → 按停止条件 S9 即停 |
| C3 | fail-closed 语义在位（反例守护）：测量前于主机侧回环模式复跑三个反例全绿——注入泄漏必须 `n2h-fail`、正控缺失必须 `n2h-inconclusive`、端点枚举缺失必须 `n2h-inconclusive` | `--fault-leak` / `--fault-no-posctl` / `--fault-omit-endpoint`（`client/core/src/bin/nbinterop.rs:763-840`，fault 注入解析 `:799-801`（初稿 `:254-261`；I6/I7 增码后行号更新）；`client/core/src/n2h.rs:1203-1220`；治理登记 `docs/native-nx-governance.md:135`）。补充（非替代）：设备判定接线另有离线反例守护（泄漏→fail、正控缺失/冻结缺失/对账破坏→inconclusive、缺块/篡改拒绝，`client/core/tests/n2h_device_collector.rs:1036/:1207/:1278/:1331/:1373` 六场景 + §13.10 四例 `:1869/:1942/:1997/:2077`（同一文件内四个用例的函数起始行，M-2 更正：原引 `:1826/:1899/:1954/:2034` 为设计稿未 commit 时的工作树行号）） | 反例未按预期判 → 测量无效，harness 不可信，停止并上报 |
| C4 | TUN 包级负证据：`tun_negative.hit == false` 且 `frames_scanned >= 1` 且 **`scanned_sides` 恰为字面 `["vpn-tun"]`**（B-5：登记的采集器侧名，不符即 inconclusive） | 设备证据 JSON `tun_negative`（渲染 `n2h.rs:344-360`；设备装配 `n2h_collector.rs:2856-2861`；`scanned_sides` 字面由 `TUN_IFACE_NAME="vpn-tun"` 冻结，`n2h_collector.rs:965,:1475-1476,:1565`，测试断言字面＝crate 内联单测 `client/core/src/n2h_collector.rs:4531`（`scanned_sides` 恰 `["vpn-tun"]` 的 contains 断言；M-1 更正：原引 `tests/n2h_device_collector.rs:4531` 路径错，该文件仅 2153 行）＋集成测试侧 `client/core/tests/n2h_device_collector.rs:1099-1100`；扫描方法 `n2h.rs:97`；任意外层探针载荷命中即证伪 `n2h.rs:813-822`）。**零帧** → 设备判定引擎强制 `n2h-inconclusive`（`n2h_collector.rs:2870-2878`：零帧上的干净负扫描不可判读）；hit 方向经 `notes[]` 的 `outer-hit-direction probe_id=… direction=from-os\|to-os …` 行登记（D4 偏离：冻结渲染器无逐 hit 方向字段，改记 notes，`n2h_collector.rs:2836-2846`，设计 §13.3） | 命中 → `n2h-fail`（**直接证伪，优先于一切，且设备接线证伪不降级**，`n2h.rs:798-810`；`n2h_collector.rs:2879-2882`）；零帧扫描 → `n2h-inconclusive` |
| C5 | 隧道正控：`tunnel_positive_control.observed_in_tunnel == true`，且 `probes[]` 中 `direction=="a2b"` 与 `direction=="b2a"` **各至少一条**、全部 `sent==true` 且 `observed_in_tunnel` 非 null。**标注（B-7）**：「双向各一条」是本判据的解释层要求，**严于工具 verdict**——引擎 `observed_in_tunnel()` 只检查非空 + 全部 sent + 全部 observed，不检查方向（`n2h.rs:406-412`） | 设备证据 JSON `tunnel_positive_control`（结构 `n2h.rs:365-423`；judge 第 4 步缺失/未观测永不通过 `n2h.rs:863-885`）。设备双腿：a2b＝采集器以 wildcard UDP socket 发往对端 overlay IP:posctl_port（载荷 `N2H-posctl-<24hex>\|positive-control`；`n2h_collector.rs:2439-2478`，`DEFAULT_POSCTL_PORT=40001` `:1304`），观测＝from-os 入口命中 **且** echo 回包 to-os 命中（D5，设计 §3.4/§4.3）；b2a＝对端 `nbinterop n2h-peer` 主动发送、marker 经 arm 控制文件可选键 `peer_b2a_probe_id` 由运维中转公告（ets 透传 `NetBirdVpnExtensionAbility.ets:1046-1050`；严格形态校验非法即拒开窗，设计 §13.10②），观测＝to-os 交付侧命中（`n2h_collector.rs:2492-2535`）。引擎侧兜底（如实登记，判据不依赖）：b2a 未公告时该腿以 `sent:false` 入档，judge 第 4 步对其判 not-sent → `n2h-inconclusive`（`n2h_collector.rs:2520-2535`） | `n2h-inconclusive` |
| C6 | 端点侧投递/源地址：每个 `sent=true` 的外层探针在 `endpoint_side[]` 存在 `observed=true` 收执，且 `physical_src_source` 如实标注来源 | 设备证据 JSON `endpoint_side[]`（元素结构 `n2h.rs:431-463`；judge 第 5 步 `n2h.rs:887-902`）。设备侧收执矩阵（设计 §4.0）：stun＝`stun-binding-response` 自证闭合（`physical_src_source=endpoint-reported`）；wg_peer＝`peer-device-offpath-drop`——**对端自身 `unknown_peer_drops` 是活值（窗口开取 baseline `:2188`、观察期取增量 `:2588`），当前无设备侧载体**（设计 §13.10④：静态 arm 中转语义上不可用，`peer_report().unknown_peer_drops` 恒 None，绝不以设备自身同名计数器顶替），其唯一载体＝主机侧 `nbinterop n2h-peer` 的 `n2h-peer-stats` stderr 状态行（`bin/nbinterop.rs:1215`；runbook §9.4）作**运维收执**；mgmt/signal/relay（TCP）＝`method:"none"` 缺口（运维侧服务端投递日志未取得前不得假通）。**运维收执缺位时设备轮 verdict 预期恒为 `n2h-inconclusive`（设计 §4.0/:186 诚实结论，非工具故障）** | `n2h-inconclusive`（外路投递未证明） |
| C7 | 计数对账：`counters[]` **恰一元素**、`side=="vpn-tun"`、`reconciled==true`（恒等式 `delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`）**且 packets 恒等式成立**（`delta.wg.rx_packets == delta.tun.delivered_frames`，设备引擎加严项） | 设备证据 JSON `counters[]`（元素结构 side/before/after/delta/reconciled/reconciliation，`n2h.rs:544-575`；字节恒等式与注记 `n2h.rs:100,:560-563`；packets 恒等式 `counter_identities` `n2h_collector.rs:1195`，破坏 → 强制 `n2h-inconclusive` `:2869-2882`，理由入 `reasons` `:2820-2832`）。**两口径登记（B-6）**：① core/status 计数 `wg.rx_bytes_to_tun`（字段 `client/core/src/wg_device.rs:258`，递增点 `:1148`＝写 TUN 成功分支）；② `/proc/net/dev` vpn-tun **RX bytes**（内核接口账本；`PROC_NET_DEV_PATH` `n2h_collector.rs:968`；N13 真机先例 4624==4624 精确吻合，`docs/n13-device-relay-evidence-20260916.md:66,:83`）。**对应关系**：窗口开/关各一次**配对快照**、采集器**先读 /proc 再读 wg 计数**（读取序 `n2h_collector.rs:970-1037`）；快照即 `counters[]` 的 `before/after`，`delta` 为其差；`tun{}` 三字段口径＝/proc vpn-tun（`delivered_bytes`=RX bytes、`delivered_frames`=RX packets、`injected_frames`=TX packets，D8/D9，设计 §3.4）；来源注记 `counters-source=proc-net-dev\|hdc-proc-fallback\|tee-fallback`（`:971,:974,:977`；tee-fallback 仅离线 harness）；两恒等式在 after 配对上不平 → 有界重采样 ≤3（`MAX_PAIRING_RESAMPLES` `:981`；`notes[]` 记 `counters-pairing-resamples=`/`counters-identity=bytes-broken\|packets-broken`，`:2811-2819`），仍不平 → 如实 `reconciled:false`，不修补 | `n2h-inconclusive`（`n2h.rs:904-919`；`counters[]` 缺失同） |
| C8 | 工具结论汇聚：本窗口 verdict == `n2h-pass` | 设备证据 JSON `verdict` 字段，并与 hilog `N2H_COLLECTOR_CLOSE\|phase=closed\|verdict=…` 行（`n2h_collector.rs:2335`）**交叉核对一致**；不一致 ⇒ 该窗口**不得登记通过类结论**（记 `n2h-inconclusive`）并按 S13 停止上报；此处「以文档为准」**仅指证据载体**（文档是 verdict 的登记载体与读数源），不得读作「文档 verdict 可采信为通过」（M-4 限定）。verdict 由与主机侧**同一判定引擎**产出（`Evidence::judge`，`n2h.rs:798-926`，判定顺序：证伪优先 → 冻结 → 探针 → 正控 → 收执 → 对账）**加两条设备侧加严**（packets 恒等式破坏、零帧扫描 → 强制 inconclusive；证伪不降级，`n2h_collector.rs:2867-2882`）。退出码口径只适用主机侧工具（`nbinterop` 0/1/20；`n2h-decode` 0/21/2，runbook §9.6）——设备侧无进程退出码，读数载体即文档与标记行 | 按 `n2h-fail` 或 `n2h-inconclusive` 如实登记 |
| C9 | 撤销后复验：VPN destroy 后重跑一轮完整窗口（C1–C8 重复成立，新会话实例、新 arm），且 destroy 后无残留 WSS 出站连接 | 新窗口设备证据 JSON；`/proc/net/dev` vpn-tun 计数与进程核查（先例 `docs/n13-device-relay-evidence-20260916.md:66`）；**WSS 残留腿的逐字只读命令（E-2）**：`hdc shell "cat /proc/net/tcp"`（连接表读取；WSS 为 TCP，按冻结 relay 地址:端口过滤 ESTABLISHED 行，逐行登记；文件不可读 → 该腿记缺口，不得假通）；destroy 后不得有 WSS 出站（`docs/n13-relay-increment-plan-20260914.md:53-54`） | 任一不成立 → 复验不通过，如实登记 |
| C10 | **重冻纪律（B-3 新增）**：窗口内出现重连或端点变化（presence 打点 `peers-went-offline`/`peers-online`（commit `e3e2ba6`，`client/core/src/relay_client.rs`）；relay 广告集合变化（`VPN_RELAY_STATUS\|advertised=true\|advertisedUrls=`，先例 `docs/n13-device-relay-evidence-20260916.md:48,:150-151`）；DDNS 重解析值与冻结值不符（`VPN_ROUTE_ENTRY\|dest=` 逐行比对冻结集））时，采集器窗口的冻结集与 marker 集已定型、**无法窗口内重冻重排** → 即 fail-closed 结束该窗口并登记；**不得跨窗口拼接**（治理要件「VPN 创建及每次连接/重连前」完整冻结，`docs/native-nx-governance.md:125-126`；壳侧路由于 create 时定型，窗口内不重排） | hilog 时间序与上列标记行 + 冻结集快照（`frozen_endpoints[]` 的 `frozen_at_unix/mono`） | 该窗口按 `n2h-inconclusive` 登记，并按 **S8 的同一纪律**处置（不重试、不换端点/解析值、不跨窗口拼接）——S8 的字面触发是 `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`（§4-S8），与 C10 三类触发不同源，此处只引用其纪律、不声称触发同一停止条件（R-2 更正：原「走 S8 上报路径」易读成同一触发）；新窗口须重新 arm/重冻（窗口定义见上） |

**结论与登记口径（强制）**：

- 终局口径措辞限制（`docs/native-nx-governance.md:130`）：结论只可写 **`N2-H pass`**；五项禁词清单见治理原文（:130），本文全程不复述其字面，文末自检见 §8。
- 逐 socket protect 义务在一切证据与登记中记 `UNSAT/未满足`，不得以任何措辞掩盖（`docs/native-nx-governance.md:131`）；固定记录字段 `unsat_note` 随每份证据文档输出（`client/core/src/n2h.rs:80`）。
- 残余范围必须明示（至少）：域名/CDN、DNS 上游、动态候选、重连换址、非 LAN/NAT 拓扑、宽前缀旁路、IPv6（`docs/native-nx-governance.md:132`）；工具以固定 `residual_scope` 八条随文档输出（`client/core/src/n2h.rs:84-93`）。
- 证据 ID 形态：`EV-N2H-<目标代码>-<YYYYMMDD>-<序号>`（`docs/evidence-schema.md:22,:28`）；只在 campaign 正式轮分配（§1.3 授权类别时序）。
- 证据记录状态按 schema 枚举执行：执行后 `collected`，独立审查合格后方可 `reviewed-pass`；只有 `reviewed-pass` 可用于结论（`docs/evidence-schema.md:61-73`）。
- 设备文档自标识：`mode=="device"`（`n2h_collector.rs:1292`）与尾部 `collector{}` 块（§3.1）随文档输出；`collector.build_sha256` 须与该轮 `N2H_BUILD_SHA256` 登记值逐字一致（runbook §9.8）。
- 记录纪律：任何一步未执行就写「未验证」，不得用推断或主机侧结果顶替（`docs/device-validation-plan-20260913.md:15`）。

---

## 3. oracle 与观测点

### 3.1 主 oracle：设备侧证据采集器产出的证据文档

- **采集器（B-1 指名的设备侧产出者）**：core 新模块 `client/core/src/n2h_collector.rs`（I0–I7，commits `cf0a031`/`49ad574`/`8b11200`/`568c931`/`e6e4f3a`/`8929695`/`c3f0832`/`5beb084`），随现有 cdylib `libnetbird_core.so` 打进签名 HAP（构建目标不变：`bash client/build.sh`，runbook §9.2）；无独立诊断二进制（设计 §2.1：TUN fd 只存在于 App 进程、探针必须是 App 自身流量、冻结来源全在进程内）。四个 napi 导出 `n2h_collector_arm|status|evidence|disarm`（`client/core/src/napi.rs:119-122`）；arm 通道＝控制文件 `/data/local/tmp/netbird-n2h-arm`（`N2H_ARM_FILE`，`NetBirdVpnExtensionAbility.ets:75`；watcher 钩子 `maybeArmN2hCollector` `:967`；可选键 `peer_b2a_probe_id` `:1046-1050`）。窗口状态机有界（60 s 上界，`n2h_collector.rs:1369`）；arm 闸门 fail-closed token：`no-connector|device-not-up|tunnel-not-ready|already-armed|invalid-opts|window-failed`（`:1390-1395`）。
- **产物离设备**：hilog 分块主通道——每行 `N2H_EVIDENCE|seq=i/n|sha256=<全文档>|b64=<512B 切片>`（`CHUNK_PAYLOAD_BYTES`/`CHUNK_MARKER`，`n2h_collector.rs:86,:90`——名序与行序一致，R-3 更正：`:86`=CHUNK_PAYLOAD_BYTES、`:90`=CHUNK_MARKER）；主机侧重组＝`target/debug/nbinterop n2h-decode <hilog-capture> [--out <file>]`（分派 `bin/nbinterop.rs:558`，实现 `:1009`；缺块/摘要不符/重复/拼接 ⇒ exit 21 且不落盘，runbook §9.6）。重放面：已闭合窗口可经 napi `n2h_collector_evidence(chunk)` 反复重放（设计 §13.5）。
- **主机侧 `nbinterop isolation-check`（`client/core/src/n2h.rs`）的角色**：HOST-SIDE 工具（自我定义 `n2h.rs:43-51`；host-only 构建，`client/core/Cargo.toml`）——只承担 C3 回环反例守护与离线对拍，**不产出设备证据**；对端配合＝`nbinterop n2h-peer`（echo 逐字回写 + posctl b2a + `unknown_peer_drops` 状态行，commit `8929695`；公告行 `bin/nbinterop.rs:1185`、状态行 `:1215`；`--timeout` 到期 exit 0 为停止形态）。
- **证据 JSON 字段（治理登记的口径，`docs/native-nx-governance.md:135`；设备文档与主机侧同一渲染器 `Evidence::to_json`，`n2h.rs:931-974`）**：`frozen_endpoints` / `probes` / `tun_negative` / `tunnel_positive_control` / `endpoint_side` / `counters` / `verdict` / `residual_scope` / `unsat_note`；另有 `tool`/`schema`/`mode`/`session`/`generated_unix`/`started_mono`/`finished_mono`/`reasons`/`notes`。设备侧取值差异全量登记于设计 §3.4 差异表（D1–D10）；`notes[]` 的设备注记族：`counters-source=…`、`counters-pairing-resamples=…`、`counters-identity=bytes-broken|packets-broken`、`outer-hit-direction …`；**尾部追加 `collector{}` 自标识块**（`n2h_collector.rs:2756-2771`；文档最后一个键，尾部追加纪律）：`crate_version`/`target_triple`/`build_sha256`/`build_source`——`build_sha256` 默认＝源码摘要（可复现），`build_source="operator-reported"` 仅在 arm opts 携带 `build_sha256_reported` 时出现（设备轮 ets 白名单不转发该键 ⇒ 预期恒 `"embedded"`；实际安装 HAP 的 sha256 由 §1.3 冻结件承担，设计 §13.11②、runbook §9.1-4）。
- verdict 三值与主机侧退出码：`n2h-pass`=0 / `n2h-fail`=1 / `n2h-inconclusive`=20（`docs/n2h-isolation-evidence-notes.md:68-74`；预连模式恒 `n2h-inconclusive` 属设计行为，`docs/device-relay-enablement-runbook-20260914.md:240`）；设备侧 verdict 读数载体＝文档 + `N2H_COLLECTOR_CLOSE` 标记行（C8）。
- CLI flag（主机侧，`client/core/src/bin/nbinterop.rs:763-840`）：`--out <file>`、`--loopback`（默认回环）、`--config <file>`（预连冻结检查，只读 `management_url`、不读密钥，`client/core/src/n2h.rs:1810-1812`）、`--outer-endpoint <kind:host:port>`（可重复）、`--fault-leak` / `--fault-no-posctl` / `--fault-omit-endpoint <kind>`（反例注入，仅回环模式）。
- 路由表 / `/proc/net/route` / 配置回读 / API 返回值**只作辅助**，任何一项都不能单独产出通过类结论（`client/core/src/n2h.rs:40-41`；`docs/n2h-isolation-evidence-notes.md:28`）。

### 3.2 设备侧既有观测点（已实现，直接可用）

| 观测点 | 位置 | 用途 |
|---|---|---|
| `VPN_ENDPOINT_EXCLUSIONS_BEGIN\|relayRequired=` / `VPN_ENDPOINT_EXCLUSIONS\|hosts=\|resolved=` | `NetBirdVpnExtensionAbility.ets:869,:932`（初稿 `:808,:871`；I0–I7 ets 增码后行号更新，内容定位不变） | C2 排除先于建连的时间序与计数 |
| `VPN_CREATE_RESOLVED\|accepted=` | `NetBirdVpnExtensionAbility.ets:1585`（审查基线 `b710741` 时为 `:1391`） | C2 建连接受时刻 |
| `VPN_ROUTE_ENTRY\|…\|dest=\|prefix=\|isExcludedRoute=` 等 | `NetBirdRouteValidation.ets:245-253`（N13 先例 `docs/n13-device-relay-evidence-20260916.md:49`） | C2 集合包含关系的逐条字面比对 |
| `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`（fatal）/ `_SKIPPED`（skip-with-log）/ `_URL_MALFORMED` / `_RELAY_STATE_UNKNOWN` | `NetBirdVpnExtensionAbility.ets:942`、`:926`、`:901`、`:836`（初稿 `:881`、`:865`、`:840`、`:775`）；fail-closed 两分支语义见 commit `64989bd` 提交说明 | fail-closed 语义与缺口留痕（C2/C3 佐证；触发即按 S8 处置） |
| 排除集动态派生（含 relay、广告即排除、DDNS 零缓存重解析） | commit `a545488`（动态派生+fail-closed+纯模块自检）、`64989bd`（消费 `relay_advertised.urls`）；core 侧 relay 广告状态 `client/core/src/connector.rs:1629-1656`、广告集合消费 `:1724-1733`（先导 commit `95f6c02`） | C2 的实现依据；排除集按广告集合派生、轮换后随广告刷新（`docs/n13-device-relay-evidence-20260916.md:191`） |
| 采集器标记族 `N2H_COLLECTOR_ARM / _CLOSE / _DISARM`（core）与 `N2H_COLLECTOR_ARMED / _ARM_FAILED / _NOT_ARMED / _ARM_OPTS_IGNORED`（ets） | `n2h_collector.rs:1383-1385,:2052,:2161,:2335`；`NetBirdVpnExtensionAbility.ets:986,:1002,:1058,:1065`（读法五形态见 runbook §9.7 表） | 窗口状态、arm 拒绝、verdict/chunks/sha256 交叉核对 |
| status JSON `tx_bytes` / `rx_bytes_to_tun` 增补 | `client/core/src/wg_device.rs:254,:258`（`docs/n2h-isolation-evidence-notes.md:100-104`） | C7 计数对账的设备侧读取（口径①） |
| `/proc/net/dev` vpn-tun RX/TX | 先例 `docs/n13-device-relay-evidence-20260916.md:66,:83`；采集器直读 `n2h_collector.rs:968` | C7/C9 的独立计数口径（口径②） |
| `nbinterop n2h-peer` 公告行/状态行 | `bin/nbinterop.rs:1185,:1215`（runbook §9.4；真实样例设计 §13.9） | C5 b2a 腿 marker 来源；C6 wg_peer 运维收执 |
| `WgDeviceStats`（含 `tun_write_errors` 聚合计数） | `client/core/src/wg_device.rs:274`（递增点 `:1154`） | OB-01/02 的粗粒度观察底座（仅聚合，无 EAGAIN/部分写细分 → §7 登记 `unobservable`） |

### 3.3 观测三件套与 N4——逐项状态（F-1/F-2 重写；初稿「当前不存在」在 b710741 时成立，同日已部分落地）

| # | 观测点 | 状态（HEAD `5beb084` 实读） | 服务的判据/义务 |
|---|---|---|---|
| N1 | 状态 JSON 暴露 per-peer 会话归属（`wg_peers[]`） | **已落地**：commit `a1947b9`（状态 JSON 尾部追加 `wg_peers[]`，聚合与归因同快照）；采集器冻结来源 token `wg-device-outer-socket`（`n2h_collector.rs:369`） | C5 正控归因（正控对象＝预期对端）；冻结装配的 wg_peer 来源 |
| N2 | 探针回执计数（sent/acked） | **已落地但 host-only**：commit `7923288`（`nbinterop probe-stats` 周期行+终值行，只改 `bin/nbinterop.rs`）——**不适用设备侧**；设备文档的收执可计量性由 `endpoint_side[]` + judge 第 5 步承载（C6） | C6 端点侧投递的可计量形态（设备侧以文档字段承载） |
| N3 | presence 转换打点（逐行、带 peer 身份） | **已落地**：commit `e3e2ba6`（`peers-online`/`peers-went-offline`/`presence-cleared` 打点 + `peers_online_rx` 可断言计数，`client/core/src/relay_client.rs`） | C10 重冻纪律的端点状态核查（重连/下线窗口） |
| N4 | tun 写路径 EAGAIN/部分写细分计数 | **未落地**：`wg_device.rs:274` 对 TUN 写失败一律 `tun_write_errors` 聚合（递增点 `:1154`），无 EAGAIN/部分写细分 → OB-01/02 观察字段保持 `unobservable`（§7；F-1 裁定口径） | OB-01/02 观察字段 |

注：「轨道 A（观测三件套）」之名仍未在仓库文档立项；现行登记仍是三项诊断清单本身（`docs/n13-device-relay-evidence-20260916.md:639-651`）。上表逐项状态已按实际落地情况重写并同步 §9-3。

---

## 4. 停止条件（出现任一即停并上报；不重试、不绕过、不换通道）

**时钟与授权类**（依据：授权窗口时钟硬前置条款 1-10，`docs/native-nx-governance.md:144-153`）：

- **S1** hdc 不可达（`:148a`；先例处置 `docs/device-relay-enablement-runbook-20260914.md:335`「停止并上报，不得反复重试」）。
- **S2** 设备处于锁定/需要解锁状态（`:148b`；先例：`aa start` 被 `10106102`（屏幕锁定）拒绝即停、不计完成，`docs/n13-device-relay-evidence-20260916.md:423`）。
- **S3** 主机与设备时钟偏差超过 60 秒（T0 裁定阈值；`:148c`——偏差超阈时窗口比对结果不可信，属停止条件而非告警）。
- **S4** 窗口外/到点/剩余窗口不足以覆盖预计耗时：窗口外产物一律 `out_of_authorization_window:true`/`is_evidence:false`/`not_a_gate_pass:true`，不得作为证据、不得被后续 AUTH 追认；窗口中途到点立即停止并登记中断点，不得完成「手头这一步」（`:146,:149`）。
- **S5** AUTH 文件与 `.sha256` sidecar 校验不通过、同一 AUTH 存在多份不一致副本、`consumed:true` 或 `reusable:false` 且已消费（与窗口外同等处理）、设备时间不可读、时区未知（`:150,:151`；比对记录时刻倒退独立为 S19）。
- **S6** 单次时钟比对超过 15 分钟未重比对（`:149`）。
- **S7** 同一 AUTH 并行执行或跨会话续用（`:153`）。
- **S19**（自 S5 拆出，独立成项）单调性检查失败：本次主机时刻早于同一 AUTH 上一次已登记的 `checked_at`（比对记录时刻倒退，`:152`）→ 时钟不可信，立即停止并上报。

**测量与判据类**：

- **S8** 冻结/解析失败触发 `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`：该窗口 fail-closed 中止（这是判据设计行为，不是可重试故障）；**不得**换端点/换解析值拼凑窗口；DDNS 换址 = 新端点 = 重冻（新窗口），不得与旧窗口数据拼接（`docs/n13-relay-increment-plan-20260914.md:45-47,:62`；DDNS 须每次动态解析、不得硬编码，`docs/n13-device-relay-evidence-20260916.md:49`；C10 同口径）。
- **S9** `n2h-fail`（任意外层探针唯一载荷命中任何 TUN 帧）：直接证伪成立，立即停止扩展测量、保全证据并上报（判定顺序：证伪优先于一切，`client/core/src/n2h.rs:798-822`；设备接线证伪不降级，`n2h_collector.rs:2879-2882`）。
- **S10** 默认路由下 WSS 未先排除 relay TCP（`docs/n13-relay-increment-plan-20260914.md:53-54`）。
- **S11** 无路径正证据却报 relay 成功；ICE Failed 被当作可达；令牌过期静默假通；destroy/注销后仍有 WSS 出站连接（`docs/n13-relay-increment-plan-20260914.md:53-54`）。
- **S12** 开发期偏离通道被启用（`VPN_PROTECT_DEVIATION_ARMED|not_a_gate_pass=true` 一类标记出现）：该窗口不得作为证据，停止并上报（`docs/native-nx-governance.md:136`；`docs/device-relay-enablement-runbook-20260914.md:158-165,:250`）。
- **S13** 仪器异常（如 hilog 应用域被滤至 WARN+、流式抓取零应用行、`n2h-decode` exit 21 缺块且重抓失败）：**先停止并上报再处置**，不得自行扩大读取限额或级别设置（教训登记：`docs/n13-device-relay-evidence-20260916.md:96-98`「同类仪器故障应先停止并上报再处置」；runbook §9.7-4）。
- **S14** 反例守护（C3）未按预期判定：harness 不可信，测量无效，停止上报。
- **S15**（D-1 新增）**allowed 逐字比对失败即停**：执行中发现任何必要动作不在本次 AUTH `allowed` 字面内 → 立即停止并上报；**不得援引其他 AUTH 措辞作依据**（纪律原文：`allowed` 的逐字比对是执行前的必要动作，`docs/n13-device-relay-evidence-20260916.md:165-167`；偏差事实 `:154-159`）。
- **S16**（D-2 新增）**凭据/敏感命中即停并隔离**：证据目录敏感扫描（runbook §5，`docs/device-relay-enablement-runbook-20260914.md:304-310`）命中秘密形态（`eyJ…`/`Bearer …`/`token=<值>`/`signature=<值>`/`secret`/`password`）→ 立即停止测量；该文件移出证据目录并记录、删除受污染副本、轮换相关凭据、原证据记 `invalidated`（`docs/evidence-schema.md:141`）；不得继续、不得「扫完再说」。
- **S17**（D-3 新增）**目标元组/制品漂移即停**：实际安装 HAP 的 sha256、系统版本（`param get` 读数）、bundle 名与 §1.3 冻结值不符 → 立即停止，**不得先测了再说**；HAP 冻结后不得重新打包、不得以新包 sha256 回填（runbook §9.1；`collector.build_sha256` 与该轮 `N2H_BUILD_SHA256` 不一致同此处置，runbook §9.8）。**代码 SHA 三点比对（M-5 补）**：冻结值＝该轮**实际构建并安装**的 commit（生效登记时锚定，非起草基线 `5beb084`）；测量轮逐项比对并在运行记录登记——①构建工作树 `git rev-parse HEAD` 与该轮登记的 commit 一致、②`bash client/core/build.sh` 末行 `N2H_BUILD_SHA256=<64hex>` 与该轮登记值一致、③设备证据文档 `collector.build_sha256` 与②逐字相同（`build_source="embedded"`，runbook §9.1-3/4）；任一点不符即停，不得先测了再说。理由：`N2H_BUILD_SHA256` 是 crate 构建输入的确定性摘要（`client/core/build.sh:125-138` 实读，与 host/target/profile/构建目录无关），它不携带 commit 信息，故必须与 `git rev-parse HEAD` 配对，否则「制品 ↔ commit」之间无机械锚点。
- **S18**（D-4 新增，与 B-1 配套）**设备侧采集实现未就绪即不得开始**：安装件不具备采集器能力（arm 尝试无 `N2H_COLLECTOR_*` 标记、napi 导出不存在）或该轮构建自标识与登记值不符 → 不得开始测量（runbook §9.9-⑨ 的真机 arm 可见性待确认项未关闭前，dry-run 轮即按本条验证该前置）。

---

## 5. AUTH 需求清单（供人类签发参考；allowed 逐字列出）

> 既有教训（登记原文）：`allowed` 的**逐字比对**是执行前的必要动作；起草时的「精简」必须与执行需要同步复核——任何需要的动作（含 hilog 级别设置这类细节）必须在 `allowed` 中显式列出（`docs/n13-device-relay-evidence-20260916.md:165-167`；偏差事实 `:154-159`）。以下为候选清单，**须人类逐条确认后逐字写入 AUTH**，未经确认不得执行。逐字命令面以 runbook §9（I7 落地）为准。

**清单性质与执行序（R-1 明标）**：本节第 1–34 条是 AUTH `allowed` 的**白名单编号**（供逐字写入与逐字比对；**编号 ≠ 执行顺序**），设备侧执行序**以 runbook §9.6 的固定序为准**：日志级别（第 19 条）→ 清缓冲（第 20 条）→ arm 控制文件写入（第 18 条；其前先起第 17 条主机侧对端，runbook §9.4）→ 抓取（第 21 条）→ 重组（第 22 条）。**照本节编号顺序执行会在窗口已开后清缓冲、毁掉 `N2H_EVIDENCE` 分块**——`hilog -r` 必须在 arm **之前**（runbook `docs/device-relay-enablement-runbook-20260914.md:471` 实读标题「顺序固定：日志级别 → 清缓冲 → arm → 抓取 → 重组」；`:476`＝级别、`:479`＝清缓冲、`:481`＝arm 写入占位）。

**A. 连接维持（只读 + 连接）**
1. `hdc list targets`（仅限连接维持所需；禁止扩大用途的设备枚举）
2. `hdc tconn <目标设备地址>`（失败即停，不重试）

**B. 只读核验（含时钟硬门所需）**
3. 读取主机当前时间（带时区与 epoch）
4. `hdc shell "date"`（设备当前时间、时区、epoch）——**治理硬门要求：执行硬门所需的只读时间查询命令必须在该 AUTH 的 allowed 白名单内显式列出，未列出即不得执行设备操作**（`docs/native-nx-governance.md:151`）
5. `hdc shell "param get const.product.software.version"` 等只读系统参数（型号/版本/API 版本；与 §1.3 冻结值逐项比对）
6. `hdc shell "bm dump -n cn.alfadb.netbird"`、`hdc shell "pidof cn.alfadb.netbird"`
7. `hdc shell "cat /proc/net/dev"`（vpn-tun 计数，C7/C9 口径②与 hdc fallback）
8. `hdc shell "cat /proc/net/tcp"`（E-2 新增：destroy 后残留 WSS 出站连接的连接表读取，只读；按冻结 relay 地址:端口过滤 ESTABLISHED 行逐行登记）

**C. 构建/签名/安装/启动（E-3 新增；逐字沿 runbook §2.1-2.2/§9.2-9.3）**
9. `cd /home/worker/work/base/netbird-harmonyos && bash client/core/build.sh`（core 构建：冻结符号链校验 + 构建自标识；**末行 `N2H_BUILD_SHA256=<64hex>` 逐轮登记**）
10. `cd /home/worker/work/base/netbird-harmonyos && bash client/build.sh`（HAP 打包，末尾打印路径/size/sha256；**本轮只打包一次**，§1.3 制品冻结纪律）
11. `sha256sum client/entry/build/default/outputs/default/entry-default-unsigned.hap | tee -a "$EVDIR/HAP-FREEZE.txt"`（实际安装件 sha256 冻结登记）
12. 签名（逐字，runbook §2.2 原文，输入换成本轮 HAP、输出 `/tmp/netbird-n2h-collector-signed.hap`）：`javac -cp "$TOOL" -d /tmp/signclasses <N1bdiscSignLauncher.java 路径>` + `java -cp "$TOOL:/tmp/signclasses" N1bdiscSignLauncher <p12> <p12.pass> <cer> <profile.p7b> <unsigned.hap> /tmp/netbird-n2h-collector-signed.hap netbird-harmonyos 26`（当晚生效 profile 绑定为待确认①，每轮确认）
13. `sha256sum /tmp/netbird-n2h-collector-signed.hap | tee -a "$EVDIR/HAP-FREEZE.txt"`（签名安装件 sha 一并登记）
14. `hdc file send`（`netbird-device-config.json` / setup key 文件（600 权限、凭据不回显，纪律见 runbook `:77-81`）/ arm 控制文件（见第 18 条））
15. `hdc install -r /tmp/netbird-n2h-collector-signed.hap`
16. `hdc shell "sh /data/local/tmp/start-netbird.sh"`；`hdc shell "uitest uiInput click <x> <y>"`（每次点击的坐标/控件/目的逐条登记）；`hdc shell "uitest dumpLayout"`（只读重定位）

**D. 采集器启用与测量本体（E-1 改写；逐字沿 runbook §9.4-9.6，替换初稿第 14 条的「及其等价探针」；R-1：本段编号非执行序，执行序＝级别(19)→清缓冲(20)→arm(18)→抓取(21)→重组(22)，见本节开头与 runbook §9.6）**
17. 主机侧对端（**先起它，再写控制文件**）：`target/debug/nbinterop n2h-peer --config <主机侧对端 config 路径> --overlay <设备 overlay ip[:port]> --echo-port 40001 --posctl-interval 1000 --timeout 900 2>&1 | tee "$EVDIR/n2h-peer-<HHMM>.log"`；停止形态＝`--timeout` 到期打印末次状态行并 exit 0；从 stderr 首行公告行读 `b2a_probe_id`（runbook §9.4）
18. arm 控制文件写入（**arm 之前**）：`{"posctl_port":40001,"probe_wait_ms":800,"peer_b2a_probe_id":"N2H-posctl-<24 hex，逐字取自第 17 条公告行>"}` → `hdc file send /tmp/netbird-n2h-arm /data/local/tmp/netbird-n2h-arm`（键语义/非法形态拒绝整个 arm/时序纪律：runbook §9.5）
19. `hdc shell "hilog -D 0x2900 -b D"`（**日志级别设置——单独一条，逐字**；限本项目域、非持久、重启失效；N13 偏差教训：不在字面内即不得执行）
20. `hdc shell "hilog -r"`（清缓冲）
21. `hdc shell "hilog -x | grep -aE 'N2H_EVIDENCE|N2H_COLLECTOR|N2H_|NetBirdVpn|N1BDiscVpn'" | tee "$EVDIR/hilog-n2h-collector-<HHMM>.log"`（窗口闭合后立刻抓）
22. 重组：`target/debug/nbinterop n2h-decode "$EVDIR/hilog-n2h-collector-<HHMM>.log" --out "$EVDIR/n2h-isolation-evidence-device-<HHMM>.json"`（exit 0 才落盘；exit 21 缺块 → 缓冲仍持有时重抓一次，仍失败按 S13 停止上报，不得修补拼接）
23. 主机侧（非设备操作，一并登记）：`nbinterop isolation-check --loopback` 反例守护三连跑（`--fault-leak` / `--fault-no-posctl` / `--fault-omit-endpoint <kind>`，C3；反例产物落 /tmp 不进证据目录）
24. 工具绑定（E-1 的 sha256 要求）：本轮 `target/debug/nbinterop` 二进制的 sha256 与第 9 条登记的 `N2H_BUILD_SHA256`（源码摘要，可复现，设计 §13.11①）逐字登记进本轮冻结件——「等价探针」类开放措辞不复存在，测量口径＝上列逐字命令 + 已落盘实现

**E. 收尾**
25. `hdc shell "rm -f /data/local/tmp/netbird-n2h-arm"`（控制文件清理）
26. `hdc shell "aa force-stop cn.alfadb.netbird"`
27. `hdc uninstall cn.alfadb.netbird`（≤1 次）
28. `/data/local/tmp` 命名空间内本项目临时文件清理与复核
29. `hdc rport ls` / `rport rm`（或 fport，若曾建立反向转发）
30. 证据目录逐文件 `.sha256` sidecar + `MANIFEST.sha256` 汇总回验 + 敏感扫描（runbook §5 逐字：`sha256sum … | awk … > <file>.sha256`、`cd "$EVDIR" && sha256sum … > MANIFEST.sha256 && sha256sum --check MANIFEST.sha256`、`grep -rInE 'token|signature|secret|password|bearer|eyJ' "$EVDIR"`）
31.（建议，E-4）pod 侧只读可达性 curl（runbook §8 待确认⑦ 形态：`curl -w '%{ssl_verify_result}' …`，逐字待当轮确认）；运维侧 **revoke** 一次性 setup key + dashboard **删除测试 peer**（runbook §6.5；`docs/ops-setup-key-request-prompt-20260914.md:19`）

**F. 消费语义与禁止项（结构性，随 AUTH 文件固定）**
32. AUTH 文件须含 `consumed` / `reusable` / `retry_allowed` / `successor_auth` 四字段（`docs/evidence-schema.md:127`）；过期即失效、逾期须新签、不得顺延（`:126`；`docs/native-nx-governance.md:147`）。
33. 禁止项沿用既有口径（先例全文见 `docs/device-relay-enablement-runbook-20260914.md:337-350`）：不产生 verdict/claim/门级结论；不引用/消费任何 campaign/pair/evidence ID（含已 consumed 历史ID）；root/privileged 操作；设备枚举超维持所需；非本项目 bundle；`/data/local/tmp` 外写操作；把诊断产物写进仓库或标记为 evidence、省略 `.sha256` sidecar。
34. 凭据纪律：秘密只落 600 权限文件，不进命令行/日志/URL/文档；证据记录中一律占位符（`docs/evidence-schema.md:135-143` 脱敏规则）。

**授权类别（已定；E-5 的「未定」已消除）**：按 §1.3 登记的时序执行——**先诊断类 dry-run**（例外二口径：session-scoped AUTH、白名单逐条、产物 `is_evidence:false`、不引用任何 campaign/pair/evidence ID、不产生 verdict/claim，`docs/evidence-schema.md:121-129`）验证采集器机制与操作面；**机制收敛后再签 campaign 类 pair AUTH**（§三 pre-E8 物理例外，`docs/native-nx-governance.md:53,:96`）执行正式测量轮，其产物方可按 `EV-N2H-*` 进入门证据序列。dry-run 产物**不得事后升级**为门证据（`docs/evidence-schema.md:129`）。

---

## 6. 步骤 0：授权窗口时钟比对（硬门前置，先回报再动手）

依据：授权窗口时钟硬前置条款 1-10（`docs/native-nx-governance.md:144-153`，2026-09-15 追加，只向前生效）；操作化先例 `docs/device-relay-enablement-runbook-20260914.md:28-41`（§1.0 步骤 0，缘于 2026-09-15 越界执行教训：执行者读过 AUTH 但未比对时刻、派发稿口头声称在窗口内，超窗约 4 小时 9 分）。

在发出**任何设备命令**（含清理、卸载、撤销、force-stop、安装、点击、抓日志）之前，执行者必须：

1. **记录三时钟**：主机当前时间（带时区 + epoch）、设备当前时间（带时区 + epoch）、当次 AUTH 的 `valid_from`/`valid_until` **原值**（`docs/native-nx-governance.md:144`）。
2. **三方比对**：主机时刻与设备时刻**均**落于窗口内，方可执行第一个设备命令（`:144`）。
3. **先回报再动手**：派发稿写明 `auth_id` 与窗口原文；执行者未回报比对结果前，任何设备命令不得发出；派发稿中关于「是否在窗口内」的一切非引用性声称不构成依据（`:145`）。
4. **比对结果写入运行记录**：主机/设备各自具体时刻、时区、`valid_from`/`valid_until` 原值、窗口内/外结论——**无比对记录即视为未比对**（`:144`）。
5. **epoch 比较**：一切比较在带时区的绝对时刻（epoch）上进行，禁止墙钟字符串比较；本次主机时刻不得早于同一 AUTH 上一次已登记的 `checked_at`，倒退即时钟不可信按停止条件 **S19** 处理；禁止为通过比对修改主机或设备系统时间（`:152`）。
6. **一并读取消费状态**：`consumed` / `reusable` / `retry_allowed`；`consumed:true` 或 `reusable:false` 且已消费 → 与窗口外同等处理（`:150`）。
7. **比对有效期 ≤ 15 分钟**：超期重比对；操作序列预计结束时刻必须仍在窗口内；窗口中途到点立即停止并登记中断点（`:149`）。
8. **失败即停**：hdc 不可达、设备锁定、偏差 > 60 秒 → 立即停止并上报（`:148`；即 §4 之 S1/S2/S3）。

---

## 7. OB 处置表（对照 `docs/open-obligations-ledger.md`）

使用纪律依据：判据预注册时，起草人须核对台账中归属本门的条目并逐条给出处置（承担/观察/转移去向），遗漏即审查 blocker（`docs/open-obligations-ledger.md:11`）；条目的新增、修改、关闭仅经 T0 决议（`:13`）；关闭不改写、登记证据 ID 与日期（`:14`）。实现期口径（2026-09-12 追加节）：OB-01/02/03/04/07 五条须专门测量的义务作为**实现期定向验证项**，由各实现里程碑的判据预注册逐条处置并登记观测点、随该门受治理验证一并实测落盘，不再单发独立测量 campaign（`docs/open-obligations-ledger.md:49`）。

**OB-03/04/07 处置状态（2026-09-17 已裁定，本表引用生效登记）**：三条已经 T0 独立审查裁定 + 人类批准改派**持有门 `N6`**、状态 `pending-discovery` → **`open`**、关闭触发按台账 2026-09-17 追加节的改写版生效（`docs/open-obligations-ledger.md:52-93`，commit `d6896b8`，只追加不改写）。本表不再给「收敛建议」，只引用该**已定处置**并保留本门边界。

| OB | 内容（台账行） | 本草案处置 | 观测点/字段 | 备注 |
|---|---|---|---|---|
| OB-01 | tun fd 背压实测（`:29`；持有门 N1b（尝试）→ N6/N7（承接观察）） | **观察**：本测量窗口顺带登记被动观察点，承接 N6/N7（台账 `:93` 尾注：OB-01/02 由 N2-H 设备侧证据门观察承接） | `tun_eagain_on_write_observed`——**三态登记**（`observed-true`/`observed-false`/`unobservable`，`docs/evidence-schema.md:89`；台账 :29 预登记字段名）。**本轮取值 `unobservable`**：实现无 EAGAIN 细分（`wg_device.rs:274` 仅 `tun_write_errors` 聚合计数，递增点 `:1154`；§3.3-N4 未落地），聚合计数不构成 EAGAIN 观测 | 待 N4 落地后转可观测；关闭仍按原触发（真实 tun fd 上观测到 `induced` 并落盘，台账 :29） |
| OB-02 | tun fd 部分写实测（`:30`；同 OB-01 口径） | **观察**：同 OB-01 | `tun_partial_write_observed`——三态登记（schema :89；台账 :30 预登记字段名）；**本轮取值 `unobservable`**（依据同 OB-01） | 同上 |
| OB-03 | shutdown unblock 实测（`:31`） | **引用已定处置（不再建议）**：持有门 `N6`、状态 `open`（台账 `:65-70`，T0 裁定 + 人类批准 2026-09-17）；新关闭触发＝N6 判据预注册登记 shutdown unblock 的实测/观察项（真实 `VpnConnection.destroy()` 下唯一登记 waiter 是否可解除阻塞、能否与自身 timeout 区分），实测落盘后按使用纪律 4 登记关闭，或由新 T0 书面显式放弃。**本门不承担、不主张关闭** | 本测量仅顺带观察 destroy 后同步等待行为，不设关闭判据 | `pending-discovery` 前提消失的裁定与改派依据见台账 `:56-61` |
| OB-04 | post-destroy fd 子项（`:32`） | **引用已定处置（不再建议）**：持有门 `N6`、状态 `open`（台账 `:72-77`）；新关闭触发＝N6 判据预注册登记 destroy 后 fd 有效期与重复 close 的可观察/不可观察部分，按 schema `:89` 三态逐项落盘，不可观察部分如实记 `unobservable` 并说明边界，不得记为已满足。**本门不触 fd 合同、不承担** | 同上，仅观察登记 | 同上 |
| OB-07 | E4 MTU 实际生效 oracle（`:35`） | **引用已定处置（不再建议）**：持有门 `N6`（与 OB-06 同门收口，台账 `:34`）、状态 `open`（台账 `:79-84`）；新关闭触发＝N6 判据预注册显式登记 MTU 实际生效 oracle 的方法与判定阈（须能区分 1280 与 1400），实测落盘后登记关闭；若证实无可用 oracle，事实落盘并由新 T0 决定降级或书面放弃，不得落回 `unassigned`；硬约束保留：约 1052 B 测试包不构成证据。**本门仅记 `mtu_declared` 观察，不承担** | `mtu_declared`（观察字段，如 1400） | 同上 |
| OB-08 | E4 声明范围内 IPv6 核验（`:36`；由声明 IPv6 的门承担，触发条款） | **未触发/观察**：本门冻结配置**不声明** IPv6 地址或路由（工具 IPv4-only，`client/core/src/n2h.rs:92`；治理残余范围明示 IPv6，`docs/native-nx-governance.md:132`） | `ipv6_declared=false`（登记冻结配置的 IPv6 声明状态；N1b 先例为「无 IPv6 声明 → 登记为 undeclared 形态」，台账 :36） | 触发条款保持活络：任何后续门冻结配置一旦声明 IPv6，该门同时承担核验并须在其判据预注册显式列出（缺列即审查 blocker，`:36`） |

**核对完整性**：台账其余三条不属本门、本预注册不触碰——OB-05（E4 平台 DNS，持有门 N2a + N2 撤销后复验，`:33`）、OB-06（E4 多地址/多路由/路由表交互/MTU 变更与重建，持有门 N6，`:34`）、OB-09（首轮 IPv6 能力与失败边界探测，持有门 N6、观察类非门控，`:37`）。其中 OB-05 的「N2 撤销后复验」与本测量 C9 的撤销后复验在动作上重合，但 OB-05 关闭仍以其持有门判据登记为准，本测量不复用其结论。

---

## 8. 不外推声明与禁止措辞自检

### 8.1 不外推声明

1. **Emulator/物理双向不外推**（`docs/native-nx-governance.md:35`）：本预注册及未来测量结论只覆盖其冻结目标元组（具名设备、完整系统版本、架构、SDK/API/SysCap、代码与制品哈希，`docs/evidence-schema.md:37-59`），不外推其他设备、版本、渠道或 Emulator；反向亦然。
2. **单会话窗口边界**：每份证据文档只覆盖一个采集器窗口；重连、撤销后复验、重解析/换址后的新端点均须重新执行（`client/core/src/n2h.rs:38-39,:88`；`docs/n2h-isolation-evidence-notes.md:27,:84`；C10）。
3. **残余范围（固定随文档输出，八条）**：域名/CDN（按冻结时刻解析枚举，重解析/换址不在证据内）；DNS 上游递归路径不受观测；动态候选（srflx/relayed 派生）外层映射未覆盖；重连换址须重新执行；非 LAN/NAT 拓扑（主机侧 loopback 不代表物理路径）；宽前缀旁路（仅证被枚举 /32 零命中）；端点侧日志（真实服务端投递日志须运维侧提供）；IPv6（TUN 帧解析与端点枚举均 IPv4-only）（`client/core/src/n2h.rs:84-93`；治理要求至少明示七项，`docs/native-nx-governance.md:132`）。
4. **平台适用边界**：仅在「逐 socket bypass 无公开 API」结论成立的目标平台适用；官方 SDK 出现逐 socket 机制时优先回原路径（`docs/native-nx-governance.md:129`）。
5. **主机侧结果不外推至设备**；主机侧回环与反例守护仅证明 harness 与判定引擎行为，不构成设备证据（`client/core/src/n2h.rs:43-51`；`docs/device-relay-enablement-runbook-20260914.md:254-256`）。
6. **运维收执不外推为设备自证**：对端计数器与服务端投递日志属运维侧收执，缺位即缺口（C6）；不得以设备自身同名计数器或回包顶替（设计 §13.10④）。
7. **本草案自身不外推**：未生效前不作为任何测量依据；生效后亦只约束其登记的测量窗口与判据，不构成对其他门（N6/N7/E4 等）任何义务的替代或减免（映射不免除义务原则，`docs/native-nx-governance.md:44`）。

### 8.2 禁止措辞自检（实跑记录）

自检对象：本文件全文（修订第 2 版）。禁词清单 = 治理终局口径措辞限制所列五项（字面见 `docs/native-nx-governance.md:130`；`client/core/src/n2h.rs:2347-2349` 与 `docs/n2h-isolation-evidence-notes.md:30-32` 以测试断言在证据 JSON 上钉死同一清单）。为避免本文自身命中，自检模式以字符类拆分书写：

```bash
grep -niE 'protec[t] pass|N2 pas[s]|等同逐 socke[t] protect|waive[d]|N/[A]' \
  docs/n2h-device-evidence-preregistration-draft-20260917.md
```

实跑结果（2026-09-17，修订会话内实跑）：**0 命中，退出码 1（grep 无匹配）**——五项禁词字面在本文件全文零出现。

「N2-H pass」唯一许可形态出现次数（应仅存在于引用治理原文、结论口径与登记限制处）：

```bash
grep -c 'N2-H pass' docs/n2h-device-evidence-preregistration-draft-20260917.md
```

实跑结果（同上）：**7 次**（§2 判据原文引文 :51、§2 结论口径 :56/:73、§2-C2 覆盖面登记限制 :61、§8.2 本节说明与自检命令 :267/:270、修订记录 B-2 行 :305——后三项为 §5 插入 R-1 段（+2 行）后的当前行号，第二轮复审会话复跑计数仍为 7），均为许可形态（引用、结论口径与登记限制，非结论声明）。

**自检扩展（T0 C 维度建议，固化为生效程序配套）**：本自检不再只对草案跑一次——**生效稿定稿时、每份证据文档落盘时、每份执行记录落盘时各实跑一次**，结果随各自文件登记（与 §0 G-4 的 sha256 锚定同为生效与测量程序的一部分）。

另登记：`git status --porcelain` 实跑（修订会话）只见两个文档——本文件（修改）与 `docs/n2h-preregistration-t0-review-20260917.md`（新增），未修改其他任何文件。

---

## 9. 不确定点清单（供审查席关注）

1. ~~授权类别未定~~ **已定**（2026-09-17 人类决定）：先诊断类 dry-run（例外二，产物 `is_evidence:false`、不得事后升级、不分配 EV-N2H ID）验证机制，再签 campaign 类 pair AUTH 正式轮（§1.3/§5 末）。
2. ~~设备侧 TUN 包级负证据采集入口未实现~~ **已落地**（I0–I7，§3.1；runbook §9 操作章节）。残余待确认＝runbook §9.9 ⑨–⑫（真机 arm 文件可见性、hilog 缓冲容量、App 直读 `/proc/net/dev` 权限、`hilog -D 0x2900 -b D` 参数兼容）——属操作层待确认，非判据缺口；dry-run 轮逐项关闭。
3. ~~观测三件套未立项、N1–N4 不存在~~ **逐项状态已重写**（§3.3）：N1/N3 已落地、N2 已落地但 host-only（不适用设备侧）、N4 未落地（OB-01/02 保持 `unobservable`）；「轨道 A」之名仍未立项（三项诊断清单仍为现行登记，`docs/n13-device-relay-evidence-20260916.md:639-651`）。
4. **端点侧投递收执的外部依赖（更新）**：wg_peer 收执＝对端自身 `unknown_peer_drops` **活值**，无设备侧载体，唯一载体＝主机侧 `n2h-peer-stats` 状态行的**运维收执**（C6）；mgmt/signal/relay TCP 仍须运维侧服务端投递日志（`client/core/src/n2h.rs:91`；runbook `:273`）。运维收执缺位时设备轮 verdict 预期恒为 `n2h-inconclusive`（设计 §4.0/:186、§13.10④）——这是设计行为，判据的价值在 C4 负证据（可证伪）与其余腿的可判定性；不得假通、不得写成可自证。
5. ~~OB-03/04/07 正式改派须 T0~~ **已裁定**：改派 N6、状态 open（台账 `:52-93`，commit `d6896b8`）；本草案 §7 只引用已定处置。
6. **回环反例守护 ≠ 设备反例**：C3 在主机侧回环模式验证判定引擎；设备侧无法注入泄漏反例（无 root），设备窗口的 fail 判定依赖真实观测。补充：设备判定接线（证伪优先、加严两规则、分块校验）已有离线 harness 反例守护（`tests/n2h_device_collector.rs` 六场景），但真机无主动反例注入——两者不混淆。
7. **profile/签名绑定待确认**（先例 runbook `:356-359` 待确认①④）：初稿「构建前改常量」的待确认④已过时（排除集已是运行时动态派生，commit `a545488`/`64989bd`；T0 F-3 裁定该判定成立），但签名 profile 绑定仍须每轮确认（§5 第 12 条）；runbook §9.9 新增 ⑨–⑫ 见第 2 条。

---

## 修订记录（2026-09-17，对 T0 裁定的逐项处置）

> 审查记录：`docs/n2h-preregistration-t0-review-20260917.md`（会话要点转写归档；总裁定「修订后可生效」+ 生效前置 G-1–G-4）。本节逐条登记处置，供 T0 复审机械核对；裁定措辞以审查记录 §3 为准，此处只概述。

| 编号 | 级别 | 处置 | 落点 |
|---|---|---|---|
| A-1 | 必改 | **已改**：目标元组与输入哈希改为**生效前冻结清单**（设备/系统版本/架构/SDK/API/SysCap/渠道/代码 SHA=`5beb084`/上游 SHA/工具链/命令/输入/时间戳时区/制品哈希/日志引用，schema `:37-59`），测量轮逐项比对（S17）；并写入 HAP 不可逐字节复现纪律——只打包一次、登记**实际安装件** sha256、冻结后不得重打包、不得回填（runbook §9.1；设计 §13.10⑦/§13.11） | §1.3、§4-S17 |
| A-2 | 必改 blocker | **已改（事实已定）**：门代码 `N2H` 经人类批准入 schema 枚举（`evidence-schema.md:28`，commit `1390ef4`；仓B `records/gate-code-n2h-registration-20260917.json`）；判据/证据 ID 形态统一 `EV-N2H-<目标代码>-<YYYYMMDD>-<序号>` | §1.3、§2 结论口径 |
| A-3 | 必改 blocker | **已改（决定已定）**：时序＝先诊断类 dry-run（例外二，`is_evidence:false`、不得事后升级、不分配 EV ID、不受本预注册判据约束）验证机制，再签 campaign 类 pair AUTH 正式轮（产物方可入门证据序列）；两类产物效力边界写清 | §1.3、§5 末、§9-1 |
| A-4 | 必改 | **已改**：OB-01/02 观察字段改**三态登记**（`observed-true`/`observed-false`/`unobservable`，schema `:89`）；本轮取值 `unobservable`（实现无 EAGAIN/部分写细分，`wg_device.rs:274` 仅聚合计数） | §7 OB-01/02 |
| A-5 | 建议 | **已改**：窗口长度上界（60 s/5 s + settle，`n2h_collector.rs:1369,:1372,:1378`）与预计耗时口径（AUTH 窗口原值）显式登记；执行者**单一持有、单次执行不重试不换 ID** 显式登记（治理 `:53`） | §1.3 |
| B-1 | 必改 blocker | **已改（实现已落盘）**：设备侧采集器＝core 模块 `n2h_collector.rs` 随 HAP（构建目标 `bash client/build.sh`；四 napi 导出 `napi.rs:119-122`；arm 控制文件通道；产物＝hilog 分块 + `nbinterop n2h-decode` 重组落盘）；C4/C5/C7/C8 逐字段对齐新实现（§2 表 + 文末字段对齐清单） | §2、§3.1、文末附录 |
| B-2 | 必改 blocker | **已改**：判据不削弱；显式登记覆盖现状——冻结面 7/7（采集器）、**排除面 3/7**（`NetBirdEndpointExclusion.ets:45-51` kind 仅 management/signal/relay）→ 本轮预期 C2 不成立 → `n2h-inconclusive`（未排除必需类探针入 TUN 则 C4 命中 → `n2h-fail`，判据设计行为）；**排除面补齐 7/7 前不得登记 N2-H pass** | §2-C2、§9-3 |
| B-3 | 必改 | **已改**：新增二元判据 **C10**（重冻纪律）：窗口内重连/端点变化（presence 打点/relay 广告变化/DDNS 重解析不符）无法重冻重排 → fail-closed 结束窗口并登记，不得跨窗口拼接 | §2-C10 |
| B-4 | 必改 | **已改**：C2 集合包含关系改引 `VPN_ROUTE_ENTRY\|…\|dest=\|prefix=\|isExcludedRoute=`（`NetBirdRouteValidation.ets:245-253`；N13 先例 `:49`）并定义「逐条比对通过」字面规则（每个冻结 /32 地址恰一行 `dest==A ∧ prefix=="32" ∧ isExcludedRoute=="true"`，行集完整可解析）；`VPN_CREATE_RESOLVED` 行号补齐（审查基线 `:1391`；本修订版 HEAD 实读 `:1585`，I0–I7 ets 增码致漂移，已登记） | §2-C2、§3.2 |
| B-5 | 必改 | **已改**：C4 `scanned_sides` 改**字面枚举**：恰为 `["vpn-tun"]`（`TUN_IFACE_NAME` `n2h_collector.rs:965`；测试断言字面 `tests/n2h_device_collector.rs:4531`），不符即 `n2h-inconclusive` | §2-C4 |
| B-6 | 必改 | **已改**：C7 两口径登记＝① `wg.rx_bytes_to_tun`（`wg_device.rs:258`，递增 `:1148`）↔ ② `/proc/net/dev` vpn-tun RX bytes（`n2h_collector.rs:968`；N13 `:66` 先例 4624 吻合）；与 `counters[]` 字段对应关系写明（before/after/delta、`tun{}` 三字段口径 D8/D9、`counters-source=` 注记、配对读取序、双恒等式、重采样 ≤3） | §2-C7 |
| B-7 | 提示非阻断 | **已改**：C5 保留「双向各一条」并**标注「工具 verdict 不检查此项」**（`n2h.rs:406-412` 只查非空+全部 sent+observed）；设备文档形态的引擎兜底（b2a 未公告 → `sent:false` → judge 第 4 步 inconclusive）如实登记，判据不依赖该兜底 | §2-C5 |
| C | 无问题+建议 | **建议已改**：禁词自检扩展为「生效稿 + 每份证据文档 + 每份执行记录各跑一次」，固化为生效/测量程序（与 G-4 配套）；修订稿自身重跑 0 命中（§8.2） | §8.2、§0-G4 |
| D-1 | 必改 | **已改**：新增 **S15**（allowed 逐字比对失败即停；不得援引其他 AUTH 措辞） | §4-S15 |
| D-2 | 必改 | **已改**：新增 **S16**（凭据/敏感命中即停并隔离：移出/删污/轮换/记 `invalidated`；runbook `:304-310`、schema `:141`） | §4-S16 |
| D-3 | 必改 | **已改**：新增 **S17**（HAP sha256/系统版本/bundle 与冻结值不符即停，不得先测了再说；不得重打包回填） | §4-S17 |
| D-4 | 必改 | **已改**：新增 **S18**（设备侧采集实现未就绪即不得开始；与 B-1 配套；含自标识核对） | §4-S18 |
| D-5 | 建议 | **已改**：S5 中的单调性检查（比对记录时刻倒退）拆出为独立 **S19** | §4-S19、§6-5 |
| E-1 | 必改 blocker | **已改**：初稿第 14 条「及其等价探针」**删除**，替换为 runbook §9 逐字命令面（§5-C/D 全部条目）+ 工具绑定（`nbinterop` 二进制 sha256 与 `N2H_BUILD_SHA256` 源码摘要逐字登记） | §5-9–24 |
| E-2 | 必改 | **已改**：C9 的 WSS 残留腿补逐字只读命令 `hdc shell "cat /proc/net/tcp"`（连接表读取，按冻结 relay 地址:端口过滤 ESTABLISHED 行；不可读记缺口） | §2-C9、§5-8 |
| E-3 | 必改 | **已改**：构建/签名链逐字补入（`bash client/core/build.sh`、`bash client/build.sh`、`sha256sum … HAP-FREEZE.txt`、`N1bdiscSignLauncher` 签名两步、签名件 sha256、HAP sha256 逐次登记） | §5-9–13 |
| E-4 | 建议 | **已改**：补 pod 侧只读可达性 curl（runbook §8 待确认⑦ 形态）、证据目录 `sha256sum`/`MANIFEST.sha256` 与敏感扫描（runbook §5 逐字）、setup key 运维侧 revoke + dashboard 删除 peer（runbook §6.5） | §5-30–31 |
| E-5 | 裁定性说明 | **对应 A-3 已定**：§5 末由「未定」改写为已定时序与效力边界；原表述保留的精神（方向正确）不复存在缺口 | §5 末 |
| F-1 | 必改 | **已改**：§3.3 按逐项状态重写——N1 已落地（`a1947b9` `wg_peers[]`）、N3 已落地（`e3e2ba6` presence 打点+`peers_online_rx`）、N2 已落地但 **host-only**（`7923288` `probe-stats`，只改 `bin/nbinterop.rs`，不适用设备侧）、N4 未落地（`wg_device.rs:274` 聚合无细分 → OB-01/02 `unobservable`） | §3.3、§7 |
| F-2 | 必改 | **已改**：同 F-1（逐项状态 + file:line/commit）；§9-3 同步重写 | §3.3、§9-3 |
| F-3 | 无问题 | **无需改动**（裁定成立）；§3.2 六个行号因 I0–I7 ets/core 增码漂移，已按 HEAD `5beb084` 更新（内容定位不变，漂移逐处标注初稿行号） | §3.2 |

**生效前置（G-1–G-4）状态**：G-1 采集器已落盘（I0–I7）+ 判据逐字段对齐（下附清单）——**本修订稿待 T0 复审，复审通过前不生效**；G-2 已定（门代码 + 授权类别）；G-3 本版处置全部必改与建议项（是否成立由 T0 复审认定）；G-4 生效程序已写入 §0（sha256 锚定 + AUTH 绑定登记 + 再修改即新版本）。

### 附：C4/C5/C7/C8 与设备侧采集器实现的字段对齐清单（B-1 复审用）

| 判据 | 证据 JSON 字段（渲染契约） | 设备侧实现出处（实读 HEAD `5beb084`） | 一致性锚点 |
|---|---|---|---|
| C4 | `tun_negative.hit`（bool）/ `frames_scanned`（u64）/ `hits[]` / `scanned_sides[]` / `method` / `criteria` | 渲染 `n2h.rs:344-360`；装配 `n2h_collector.rs:2856-2861`（`hit = !outer_hits.is_empty()`、`frames_scanned`、`sides`）；`scanned_sides` 恒 `["vpn-tun"]`（`TUN_IFACE_NAME` `:965`，快照 `:1475-1476,:1565`）；零帧强制 inconclusive `:2870-2878`；证伪不降级 `:2879-2882`；hit 方向入 `notes[]`（`outer-hit-direction`，`:2836-2846`） | 测试 `client/core/src/n2h_collector.rs:4531`（crate 内联单测，断言 `"scanned_sides":["vpn-tun"]` 字面）＋集成测试 `client/core/tests/n2h_device_collector.rs:1099-1100`（长度恰 1 ∧ 首元素 `vpn-tun`；M-1 更正）与场景 1/2（`:1036/:1207`） |
| C5 | `tunnel_positive_control.observed_in_tunnel`（bool）/ `expected_path:"tunnel"` / `probes[]{probe_id,direction,tuple,expected_path,sent,sent_at_mono,observed_in_tunnel,note}` | 定义与渲染 `n2h.rs:365-423`（`observed_in_tunnel()`＝非空 ∧ 全部 sent ∧ 全部 observed，`:406-412`——**不查方向**，B-7 标注）；设备 a2b 腿 `n2h_collector.rs:2439-2478`（`direction:"a2b"`、`posctl_payload` 形态 `:1307,:1318`）；b2a 腿 `:2492-2535`（`direction:"b2a"`，marker＝arm opts `peer_b2a_probe_id`，ets 透传 `NetBirdVpnExtensionAbility.ets:1046-1050`；未公告 → `sent:false` 有理由缺口）；judge 第 4 步 `n2h.rs:863-885` | 测试场景 1/3（`:1036/:1278`）与 §13.10 四例（`:1869/:1942/:1997/:2077`，函数起始行；M-2 更正） |
| C7 | `counters[]` 恰一元素：`side:"vpn-tun"` / `before` / `after` / `delta`（`wg{…}` + `tun{injected_frames,delivered_frames,delivered_bytes}`）/ `reconciled` / `reconciliation` | 结构与渲染 `n2h.rs:544-575`（`reconciled`＝字节恒等式 `:560-563`；`RECONCILIATION_NOTE` `:100`）；设备侧 `side = TUN_IFACE_NAME`（`n2h_collector.rs:1275`）；两口径＝`wg.rx_bytes_to_tun`（`wg_device.rs:258`/递增 `:1148`，读序第二 `n2h_collector.rs:1037`）↔ `/proc/net/dev` vpn-tun RX（`PROC_NET_DEV_PATH` `:968`，读序第一 `:970-1037`）；packets 恒等式 `counter_identities` `:1195` + 强制 inconclusive `:2869-2882`；重采样 ≤3（`:981`）；来源注记 `:971,:974,:977` | 测试场景 1/5（`:1036/:1373`：字节恒等式通过而 packets 破坏 → collector 判 inconclusive） |
| C8 | `verdict`（`n2h-pass`/`n2h-fail`/`n2h-inconclusive`） | 同一判定引擎 `Evidence::judge`（`n2h.rs:798-926`，顺序：证伪→冻结→探针→正控→收执→对账）+ 两条设备加严（packets 恒等式、零帧）与证伪不降级（`n2h_collector.rs:2867-2882`）；读数＝文档 `verdict` + `N2H_COLLECTOR_CLOSE\|…\|verdict=…`（`:2335`）交叉核对 | 测试六场景 verdict 全对齐（`:1036` pass / `:1207` fail / `:1278,:1331,:1373` inconclusive）；`n2h-decode` 重组校验（exit 0/21，runbook §9.6） |

### 第二轮复审（M/R 项）处置（2026-09-17，同日，同席 T0）

> 第二轮 T0 独立复审（2026-09-17，`anthropic/claude-opus-5` 席，同席复审；归档见 `docs/n2h-preregistration-t0-review-20260917.md` 文末「第二轮复审（2026-09-17，同日，同席）」节）裁定：**修订后可生效**——框架与首轮处置成立，另有 5 项必改（M-1…M-5）与 3 项建议（R-1…R-3），**全部为字面/引用级**。本节逐条登记处置与核对依据（行号＝本节写作时工作树 `140da6e` 实读）；**首轮修订记录各项不改写，本节只追加**（版本标签仍为「修订第 2 版」）；本版仍为 DRAFT，生效仍须人类批准＋G-4 sha256 登记。

| 编号 | 级别 | 处置（原引用 → 现引用） | 核对依据（实读内容） |
|---|---|---|---|
| M-1 | 必改 | **已改**：C4 与附录的测试断言出处更正——原 `tests/n2h_device_collector.rs:4531`（路径错，该文件仅 2153 行）→ crate 内联单测 `client/core/src/n2h_collector.rs:4531`；并补集成测试侧锚点 `client/core/tests/n2h_device_collector.rs:1099-1100`。**注**：首轮记录 B-5 行同引该错路径，依「首轮记录不改写」纪律保留原文，其路径以本行为准 | `src/n2h_collector.rs:4531` 实读 `assert!(json.contains("\"scanned_sides\":[\"vpn-tun\"]"));`；`tests/n2h_device_collector.rs:1099` 实读 `assert_eq!(doc.at("tun_negative").at("scanned_sides").arr_of().len(), 1);`、`:1100` 实读 `…arr_of()[0].str_of(), Some("vpn-tun")` |
| M-2 | 必改 | **已改**：C3 补充与附录 C5 行原引 §13.10 四例 `:1826/:1899/:1954/:2034`（设计稿未 commit 时的工作树行号）→ `:1869/:1942/:1997/:2077`（均在同一文件 `client/core/tests/n2h_device_collector.rs` 内，为该节四个用例的函数起始行） | 该文件 §13.10 节横幅 `:1853-1856`（标题行 `:1854` 实读 `// §13.10 — the ops relay of the counterparty's b2a announcement into the arm`）下仅四例，`fn` 起始行实读（各行上一行为 `#[test]`）：`:1869 an_announced_peer_marker_from_the_arm_options_closes_the_b2a_leg`、`:1942 an_arm_that_carries_no_peer_marker_keeps_the_b2a_leg_a_reasoned_gap`、`:1997 an_invalid_announced_marker_is_refused_without_opening_a_window`、`:2077 the_production_host_takes_the_arm_time_peer_announcement_and_keeps_the_counters_apart` |
| M-3 | 必改 | **已改**：C1 冻结时刻指针 `n2h_collector.rs:1878` → `arm_json`（`:1953`）内 `FreezeSources{…}` 字面量 `:1989-2000`（`frozen_at_unix:1998` / `frozen_at_mono:1999`）＋`assemble_freezes(&sources)` `:2001`；语义主张（冻结时刻＝arm 时刻）不变 | `:1878` 实读为 `host,`，属 `:1876-1881` 的 `opts.shell_exclusions.push(ShellExclusion::with_resolved(...))`（即 shell_exclusions 解析，非冻结时刻）；`:1998` 实读 `frozen_at_unix: clock.unix_s(),`、`:1999` 实读 `frozen_at_mono: clock.mono_ms(),`、`:2001` 实读 `let freeze = assemble_freezes(&sources);` |
| M-4 | 必改 | **已改**：C8 原句「不一致 → 以文档为准并按 S13 仪器异常处置」→「不一致 ⇒ 该窗口**不得登记通过类结论**（记 `n2h-inconclusive`）并按 S13 停止上报；『以文档为准』**仅指证据载体**」，消除「文档 verdict 可采信为通过」的读法 | 该句为修订第 2 版新引入（初稿无），已按上改写；处置与 §4-S13（先停止并上报）及 C8「不成立时」列（按 `n2h-fail` 或 `n2h-inconclusive` 如实登记）一致 |
| M-5 | 必改 | **已改**：§1.3 冻结项「代码 SHA（本修订版基线 `5beb084`）」→「**代码 SHA＝该轮实际构建并安装的 commit（生效登记时锚定）**」，并明示 `5beb084` 只是起草读码基线、不得作测量轮冻结值；S17 补**三点比对**：①`git rev-parse HEAD` ↔ ②该轮 `N2H_BUILD_SHA256` ↔ ③文档 `collector.build_sha256`。**注**：首轮记录 A-1 行同载 `5beb084` 字样，依纪律保留原文，口径以 §1.3 修订后为准 | 依据实读 `client/core/build.sh:125-138`：`IDENT` 由 `Cargo.toml`/`Cargo.lock`/`build.rs`/`proto`/`src` 内容摘要算出并注入产物，**与 commit 无关联**（同源同内容恒同值、与 host/target/profile/构建目录无关），故必须与 `git rev-parse HEAD` 配对；文档 `collector.build_sha256`（`n2h_collector.rs:2756-2771`）在 `build_source="embedded"` 下等于该值（runbook §9.1-3/4） |
| R-1 | 建议 | **已改（明标而非重排：本节编号是 allowed 白名单编号，重排会打断全文既有交叉引用「第 9/10/12/17/18 条」「§5-9–24」「§5-30–31」）**：§5 开头新增「清单性质与执行序」段＋D 段标题加注——**白名单编号 ≠ 执行序**，执行序以 runbook §9.6 为准（级别 19 → 清缓冲 20 → arm 18 → 抓取 21 → 重组 22），并点明照编号序执行会在窗口已开后清缓冲、毁掉 `N2H_EVIDENCE` 分块 | runbook `docs/device-relay-enablement-runbook-20260914.md:471` 实读标题「### 9.6 证据抓取与重组（顺序固定：日志级别 → 清缓冲 → arm → 抓取 → 重组）」；`:476` 实读 `"$HDC" shell "hilog -D 0x2900 -b D"`、`:479` 实读 `"$HDC" shell "hilog -r"`、`:481` 实读 arm 写入占位（arm 在清缓冲**之后**） |
| R-2 | 建议 | **已改**：C10「走 S8 上报路径」→「按 **S8 的同一纪律**（不重试、不换端点/解析值、不跨窗口拼接）处置」，并注明 S8 字面触发 `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED` 与 C10 三类触发不同源、只引纪律不声称同触发 | §4-S8（`:138`）字面触发实读＝`VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`；C10 三类触发（§2-C10）＝presence 打点／relay 广告集合变化／DDNS 重解析值与冻结值不符 |
| R-3 | 建议 | **已改**：§3.1 原名序与行序相反——`CHUNK_MARKER`/`CHUNK_PAYLOAD_BYTES`，`n2h_collector.rs:86,:90` → `CHUNK_PAYLOAD_BYTES`/`CHUNK_MARKER`，`:86,:90`（名序与行序对齐） | `src/n2h_collector.rs:86` 实读 `pub const CHUNK_PAYLOAD_BYTES: usize = 512;`；`:90` 实读 `pub const CHUNK_MARKER: &str = "N2H_EVIDENCE";` |

**本轮自检重跑（追加本节后实跑）**：禁词自检（§8.2 所列同一模式，五项字面）**0 命中、退出码 1**；§8.2 登记的「唯一许可形态」行计数实跑 **7 次（未变；本节未新增该字面）**；`git status --porcelain` 实跑只见本文件与 `docs/n2h-preregistration-t0-review-20260917.md` 两份文档（均修改），未 commit。

**G-4 生效程序（复审展开为五步，归档见复审记录文末节）**：①先改完 M 项 → ②定稿 sha256 并登记仓B `records/`（文件名含 effective、带 sidecar、含仓内路径/commit/sha256/审查席与人类批准原话/生效时刻带时区）→ ③首次据以测量的 campaign AUTH 必须逐字引用该 sha256（dry-run 诊断 AUTH 不引用、不消费任何 EV/pair ID）→ ④此后任何改动＝新版本＝重新独立审查＋重新登记 sha256 → ⑤禁词自检按 §8.2 固化三点各跑一次。§0-G4 的三步为其压缩版、语义一致；G-4 不属本轮 M/R 项，§0 本次未改动。

——修订第 2 版完（含第二轮复审 M/R 项处置）。仍为 DRAFT/未生效；生效程序与前置见 §0。
