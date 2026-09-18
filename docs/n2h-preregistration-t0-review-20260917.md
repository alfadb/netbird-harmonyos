# N2-H 预注册草案 T0 独立审查记录（2026-09-17）

- **文件性质：会话记录转写（要点转写，非字节级副本）**。本文件是对 2026-09-17 一次 T0 独立审查会话的**消息要点转写归档**，不是该会话的字节级副本：裁定编号、要点与结论强度逐字保留自派发与回报消息，转写过程中未增删任何裁定结论、未改写任何结论强度；会话的完整原文以当次会话记录为准，本文件不代为断言其逐字内容。
- **转写口径**：同仓B `records/ops-acl-rootcause-20260916.md` 的「会话消息要点转写（非字节级副本）」体例。
- **归档日期**：2026-09-17（Asia/Shanghai）。归档会话内只读核对仓库文档与代码；未修改任何其他文件、未运行构建、未执行任何设备命令。

---

## 1. 审查对象与基线、审查席

- **审查对象**：`docs/n2h-device-evidence-preregistration-draft-20260917.md`——N2-H 设备侧证据判据预注册**草案**（DRAFT，未生效），257 行，commit `b710741`（`docs: N2-H 设备侧证据判据预注册草案（DRAFT 未生效，含 C1-C9 判据/停止条件 S1-S14/AUTH 清单/OB 处置表）`）。
- **审查范围**：该草案全文（判据 C1–C9、oracle 与观测点 §3、停止条件 S1–S14、AUTH 需求清单 §5、OB 处置表 §7、不外推声明与禁词自检 §8、不确定点清单 §9），对照治理 `docs/native-nx-governance.md`、`docs/evidence-schema.md`、`docs/open-obligations-ledger.md`、runbook `docs/device-relay-enablement-runbook-20260914.md`、N13 证据 `docs/n13-device-relay-evidence-20260916.md` 与代码实读（`client/core/src/n2h.rs`、`client/core/src/bin/nbinterop.rs`、`client/core/Cargo.toml`、ets 侧排除与路由模块等）。
- **审查席**：T0 独立审查，2026-09-17，`anthropic/claude-opus-5` 席，**干净上下文单席**（spawn 形态，不带主会话已完成轮次）。单席，无第二席。

## 2. 总裁定与生效前置

- **总裁定：修订后可生效**。草案方向与框架成立，但存在多项必改（含 blocker）；全部必改项关闭并复审通过前**不得生效**。
- **生效前置（G-1 至 G-4）**：
  - **G-1** 设备侧采集实现落盘并随修订稿再审（对应 B-1：主 oracle 的设备侧产出者必须存在，且判据须对落盘实现逐字段重新对齐后提交再审）。
  - **G-2** 证据 ID 门代码与授权类别先定（对应 A-2/A-3：门代码入 schema 枚举；授权类别由人类明确，两类产物效力边界写清）。
  - **G-3** 全部必改项关闭（A-1/A-4、B-1–B-6、D-1–D-4、E-1–E-3、F-1/F-2；建议项 A-5/D-5/E-4/C-建议可随修订一并处置）。
  - **G-4【新增要求】** 修订稿生效时须落 **sha256** 并与 **AUTH 绑定登记**（生效稿全文哈希逐字登记；首次据以测量的 AUTH 须引用该哈希；再修改即新版本即须重新审查与登记）。否则治理 `docs/native-nx-governance.md:43`「测量后不得追认或修改」**不可机械验证**——没有哈希锚点，生效稿与被测量稿的同一性只能靠声称。

## 3. 逐条 findings（裁定转写）

> 以下逐字转写自审查会话的 findings 清单：编号、要点与结论强度照抄，未增删、未改写。

### 维度 A 完整性

- **A-1【必改】**目标元组与输入哈希未冻结（草案 :33 只写"按 schema 必填字段登记"，把具名设备/系统版本/代码 SHA/制品 sha256 留到测量时填；§三 pre-E8 要求冻结元组与输入哈希先于每次物理 campaign）。
- **A-2【必改，blocker】**证据 ID 门代码未定义（`docs/evidence-schema.md:24-29` 枚举无 N2-H；:34 ID 一经分配不得复用或改写；:127 agent 不得自分配 AUTH ID）——须人类/T0 明确。
- **A-3【必改，blocker】**授权类别未定（草案自陈 §9.1/:175）：诊断类（例外二，evidence-schema:122,:128）产物 `is_evidence:false` 且不得事后升级为门证据；campaign 类须走 §三 pair AUTH（治理 :53,:96）。
- **A-4【必改】**OB-01/02 观察字段"默认 false"违反三态记录纪律（草案 :204；`evidence-schema.md:88` 三态 observed-true/observed-false/unobservable）。
- **A-5【建议】**缺"预计耗时/窗口长度估算"与"执行者单一持有、单次执行不重试"的显式登记。

### 维度 B 可判定性（主要阻断面）

- **B-1【必改，blocker】**主 oracle 在设备侧不存在且 C4/C5/C7 的产出者从未指名（`nbinterop` 自我定义为 HOST-SIDE；n2h.rs:43-51 host-side loopback；Cargo.toml 只有 host 构建口径）→ C4/C5/C7/C8 在设备上无实现可产出，C8 必不可达；必须指名设备侧采集器（二进制/脚本、构建目标、命令、输出路径、字段 schema）并在落盘后逐字段重新对齐再审。
- **B-2【必改，blocker】**C1/C2 超出设备侧实现覆盖面（壳侧排除派生只处理 management/signal/relay 三类；stun/turn/wg_peer/dns 既不冻结也不排除）→ 以当前实现必然 inconclusive；判据本身不应削弱（治理 :126 要求七类全排除），但预注册必须显式登记"当前实现覆盖 3/7 类 → 本轮预期 inconclusive；补齐前不得登记 N2-H pass"。
- **B-3【必改】**"每次连接/重连前"重冻这一治理要件没有对应判据（壳侧路由 create 时定型）→ 须新增一条二元判据（窗口内重连/端点变化时无法重冻重排即 fail-closed 结束该窗口并登记，不得跨窗口拼接）。
- **B-4【必改】**C2 的集合包含关系用现引标记判不出来（应改引 `VPN_ROUTE_ENTRY|...|dest=|prefix=|isExcludedRoute=`，`NetBirdRouteValidation.ets:245-253`，N13 先例 :49 即用此行）并定义"逐条比对通过"的字面规则；C2 引用的 `VPN_CREATE_RESOLVED` 未给行号（实际 `NetBirdVpnExtensionAbility.ets:1391`）。
- **B-5【必改】**C4 的 `scanned_sides` 判据含测量后才能补的模糊口子 → 改字面枚举（设备窗口须恰为登记的采集器侧名，如 `["vpn-tun"]`），不符即 inconclusive。
- **B-6【必改】**C7 在设备上的两条口径未登记（设备侧不存在工具的"采集器独立清点"；可用两口径 = status JSON `rx_bytes_to_tun` ↔ `/proc/net/dev` vpn-tun RX 字节，N13 先例 :66 实测 4624 精确吻合）→ 须写明两个来源及与 n2h.rs:100 `counters[]` 字段的对应关系。
- **B-7【提示，非阻断】**C5 的"双向 a2b/b2a 至少各一条"严于工具 verdict（n2h.rs:409-412 只要求非空且全部 observed）→ 可保留但须标注"工具 verdict 不检查此项"。

### 维度 C 措辞合规

- **无问题**（禁词 grep 0 命中；`N2-H pass` 5 次均许可形态；逐 socket protect 在 :24/:66 明记 UNSAT 并写明原因/替代/残余）。
- **建议**：禁词自检从"对草案跑一次"改为"对生效稿 + 每份证据文档 + 每份执行记录各跑一次"。

### 维度 D 停止条件

- 已覆盖 S1–S8/S13/S14 等。
- **D-1【必改】**缺"allowed 逐字比对失败即停"（须写明执行中发现任何必要动作不在本次 AUTH `allowed` 字面内 → 立即停止上报，不得援引其他 AUTH 措辞作依据）。
- **D-2【必改】**缺"凭据/敏感命中即停并隔离"（runbook:304-310 敏感扫描、evidence-schema:140 发现秘密即隔离/删除污染副本/轮换/原证据记 invalidated）。
- **D-3【必改】**缺"目标元组/制品漂移即停"（HAP sha256、系统版本、bundle 与冻结值不符即停，不得先测了再说）。
- **D-4【必改】**缺"设备侧采集实现未就绪即不得开始"的停止条件（与 B-1 配套）。
- **D-5【建议】**建议补 `:152` 单调性检查失败为独立 S 项（现挤在 S5 里易漏）。

### 维度 E AUTH 清单

- 覆盖正确的有 1/2、3/4、5/6、7、8、9、10、11、12/13、16-19、20-22（级别设置单列做得好）。
- **E-1【必改，blocker】**第 14 条"及其等价探针"是测量后才能定义的口子，与 allowed 逐字纪律冲突 → 必须替换为逐字命令/脚本路径 + sha256（依赖 B-1 的实现落盘）。
- **E-2【必改】**C9 的"destroy 后无残留 WSS 出站连接"没有任何 allowed 命令支撑（清单只有 `/proc/net/dev`）→ 补逐字只读命令（如连接表读取）或把该腿改为由 hilog 标记判定并写明标记名。
- **E-3【必改】**构建/签名链未列入（runbook §2.1-2.2 的构建与签名是 install 前置，且 HAP sha256 要逐次登记）→ 按同一逐字纪律补。
- **E-4【建议】**补 pod 侧只读可达性 curl、证据目录 `sha256sum`/`MANIFEST.sha256` 与敏感扫描、setup key 的运维侧 revoke/删除 peer。
- **E-5【裁定性说明】**授权类别前置（草案 :175）方向正确且表述完整，问题只在"未定"（A-3）。

### 维度 F 观测点依赖

- **F-1【必改，事实已过时】**草案 §3.3 称 N1–N4「当前不存在」在 commit b710741 时成立，但同日三件套已落地（N1=a1947b9 `wg_peers[]`；N3=e3e2ba6 presence 打点+`peers_online_rx`；N2=7923288 `probe-stats` 但**host-only 不适用设备侧**；N4 仍未落地：wg_device.rs 对 TUN 写失败一律 `tun_write_errors` 无 EAGAIN/部分写细分 → OB-01/02 保持 `unobservable`）。
- **F-2【必改】**§3.3 表格须按逐项状态重写（已落地/host-only/未落地 + file:line）并同步 §9.3。
- **F-3【无问题】**§3.2 六个行号全部准确；§9.7 判定 runbook 待确认④已过时——成立。

## 4. 后续处置

1. **修订预注册草案（原地，仍为 DRAFT）**：按本记录 §3 逐条处置全部必改与建议项，吸收审查后落地的仓内事实（门代码登记、授权类别决定、设备侧采集器 I0–I7、runbook §9 操作章节、OB-03/04/07 台账改派等），并在草案文末新增「修订记录（2026-09-17，对 T0 裁定的逐项处置）」节，逐条登记 A-1…F-3 的处置（已改/如何改，或为何不改+依据），供 T0 复审机械核对。修订稿仍为 DRAFT、仍不生效。
2. **提交 T0 复审**：修订稿连同逐字段对齐结果（C4/C5/C7/C8 对设备侧采集器实现的字段清单）再提交独立审查（G-1）；复审通过并满足 G-2/G-3/G-4 后方可生效。
3. **本记录的登记边界**：本文件只归档审查会话的裁定；不构成任何授权、不产生任何门级 verdict、不分配任何 AUTH-/EV- ID；修订稿内容以 `docs/n2h-device-evidence-preregistration-draft-20260917.md` 为准。

---

## 第二轮复审（2026-09-17，同日，同席）

> 本节为同一文件**追加**的第二轮归档（会话消息要点转写，非字节级副本）；首轮归档 §1–§4 不改写、内容不变。

- **审查对象**：`docs/n2h-device-evidence-preregistration-draft-20260917.md` **修订第 2 版**（335 行；工作树 HEAD `140da6e`，干净）。
- **审查席**：T0 独立复审，2026-09-17（同日），`anthropic/claude-opus-5` 席——**与首轮同席**（非跨厂商第二席），单席。
- **总裁定：修订后可生效**。首轮必改与建议项的处置成立、框架不需重写；另有 5 项必改（M-1…M-5）与 3 项建议（R-1…R-3），**全部为字面/引用级**（文件名、行号、措辞），无判据语义重开。

### 必改（M-1…M-5，要点转写）

- **M-1【必改】**草案 C4（`:63`）与附录字段对齐清单（`:330`）引的 `client/core/tests/n2h_device_collector.rs:4531` **文件错**——该字面断言实际在 crate 内联单测 `client/core/src/n2h_collector.rs:4531`（测试文件仅 2153 行）；集成测试侧的真实字面断言在 `client/core/tests/n2h_device_collector.rs:1099-1100`。→ 改成正确文件名，并补上集成测试侧锚点。
- **M-2【必改】**草案引的「§13.10 四例 `:1826/:1899/:1954/:2034`」是设计稿未 commit 时的工作树行号；HEAD 实读应为 `:1869`/`:1942`/`:1997`/`:2077`（在 `client/core/tests/n2h_device_collector.rs` 内，属 I6b 的四个用例，取四个函数起始行）。→ 按实读核对后改写。
- **M-3【必改】**C1 的「冻结时刻＝arm 时刻，`n2h_collector.rs:1878`」行号错——`:1878` 是 `ArmOptions` 的 shell_exclusions 解析；冻结时刻实际在 `arm_json` 内 `FreezeSources{frozen_at_unix/frozen_at_mono}` ＋ `assemble_freezes`（`:1989-2000`／`:2001`）。语义主张为真，**仅指针错**。→ 改指针。
- **M-4【必改】**C8（`:67`）「verdict 与 `N2H_COLLECTOR_CLOSE` 行不一致 → **以文档为准**并按 S13 处置」是修订第 2 版新引入的口径歧义（初稿无此句），字面可被读成「文档 verdict 可采信为通过」。→ 限定为：不一致 ⇒ 该窗口**不得登记通过类结论**（记 `n2h-inconclusive`）＋按 S13 停止上报；「以文档为准」**仅指证据载体**。
- **M-5【必改】**§1.3（`:40`）把**起草基线** `5beb084` 登记成测量轮冻结的「代码 SHA」。→ 改为：冻结值＝**该轮实际构建并安装的 commit**（生效登记时锚定）；测量轮以 `git rev-parse HEAD` ＋ `N2H_BUILD_SHA256` ＋ 文档 `collector.build_sha256` **三点比对**，纳入 S17（§6 或相应处同步补充）。

### 建议（R-1…R-3，要点转写）

- **R-1【建议】**§5-D 条目顺序（18 写 arm 文件 → 19 `hilog -D` → 20 `hilog -r`）与 runbook §9.6 固定执行序（级别 → 清缓冲 → arm）**相反**；照序执行会在窗口已开后清缓冲、毁掉 `N2H_EVIDENCE` 分块。→ 在 §5 明标「本清单为 allowed 白名单，执行序以 runbook §9.6 为准」（或按执行序重排，二者择一，注明依据）。
- **R-2【建议】**C10（`:69`）「走 S8 上报路径」——S8 的字面触发是 `VPN_ENDPOINT_EXCLUSION_FAIL_CLOSED`，与 C10 三类触发不同源。→ 改为「按 S8 的同一纪律（不重试、不换端点、不跨窗口拼接）处置」。
- **R-3【建议】**§3.1（`:88`）「`CHUNK_MARKER`/`CHUNK_PAYLOAD_BYTES`，`:86,:90`」名序与行序相反（`:86`＝CHUNK_PAYLOAD_BYTES、`:90`＝CHUNK_MARKER）。→ 互换对齐。

### G-4 生效程序（本轮明确为五步）

1. **先改完 M 项**（本轮 M-1…M-5）；
2. **定稿 sha256 并登记仓B `records/`**——文件名含 `effective` 字样、带 `.sha256` sidecar；登记内容含**仓内路径 / commit / sha256 / 审查席与人类批准原话 / 生效时刻（带时区）**；
3. **首次据以测量的 campaign AUTH 必须逐字引用该 sha256**（AUTH 绑定登记）；**dry-run 诊断 AUTH 不引用该 sha256，也不消费任何 EV/pair ID**；
4. **此后任何改动＝新版本＝重新独立审查＋重新登记 sha256**；
5. **禁词自检按 §8.2 固化三点各跑一次**（生效稿定稿时、每份证据文档落盘时、每份执行记录落盘时）。

### 「残余缺口登记真实性专项」结论

- 专项核查对象＝修订第 2 版如实登记的残余缺口三处：①**运维收执缺位** → 设备轮 verdict 预期恒为 `n2h-inconclusive`（§1.2/`:31`、C6、§9-4）；②**壳侧排除派生只覆盖 3/7 类**（§1.3/`:43`、§2-C2、§9-3）→ 排除面补齐 7/7 前不得登记 N2-H pass；③**N4（tun 写路径 EAGAIN/部分写细分）未落地** → OB-01/02 本轮取值 `unobservable`（§3.3-N4、§7）。
- **结论：三处均如实登记，未打扮成可自证**——未把运维侧收执写成设备自证（不以设备自身同名计数器或回包顶替）、未把 3/7 排除面写成判据已满足、未以聚合计数顶替 EAGAIN/部分写观测；缺口与「不得登记通过类结论」的限制在正文可见，无假通措辞。

- **本节登记边界**：本节只归档第二轮复审会话的裁定要点（会话消息要点转写，非字节级副本）；不构成任何授权、不产生任何门级 verdict、不分配任何 AUTH-/EV- ID；修订稿与最终生效稿内容以其自身文件为准。

---

## 第三轮裁定（2026-09-18，同日续，7/7 排除覆盖口径）

> 本节为同一文件**追加**的第三轮归档（会话消息要点转写，非字节级副本）；首轮（§1–§4）与第二轮复审归档不改写、内容不变。裁定原文的编号、要点与结论强度逐字保留自派发与回报消息；会话完整原文以当次会话记录为准，本文件不代为断言其逐字内容。归档会话只读核对仓库文档与代码（含 HarmonyOS SDK 26.0.0.821 typings 只读探测）；未修改任何文件、未运行构建、未执行任何设备命令。

- **审查对象与起因**：v2 生效稿（锚点 `bf04913959f7645ddde3a549a7ae4205a3cf34872a4fc7792f4e9296a30eb5be`）在「7/7 排除覆盖」补齐工作流派发前，其三项口径（dns 类构成／turn 缺席形态／wg_peer 代际语义）送 T0 钉口径；审查同时旁及两点（fail-closed 触发面、N8 recreate 预算静默 fail-open，即「附带两点」）。
- **审查席**：T0 独立审查，2026-09-18，`anthropic/claude-opus-5` 席，**干净上下文单席**（spawn 形态，不带主会话已完成轮次）。单席，无第二席；与 v1/v2 两轮同席——故 v3 重审须换**跨厂商席**。
- **总裁定：可推进，但须先完成 5 项前置（一次性收口进 v3）**：①项1 dns 类口径定义；②项2 turn 缺席形态时效声明；③项3 wg_peer 代际判定＋C2 七条规则；④C10 第四类＋S20；⑤fail-closed 触发面扩大。五项收口后：换跨厂商 T0 重审（`openai/gpt-5.6-sol` 席，干净上下文）→ 人类批准 → sha256 重登记（v2 锚点据以测量效力作废）。

### 三项口径结论（要点转写）

1. **dns**：dns 冻结集＝隧道外解析器端点集（①壳侧平台默认网络 DNS 属性 ∪ ②`netbird_config.dns.servers[]` 中不落在 `address/address_prefix_len` 或任一 `routes[].network` 覆盖范围内的条目）；`interface_dns` 与 overlay 内 server 一律不入冻结集，只在 `notes[]` 记 `dns-overlay-internal-excluded-from-freeze=<n>`；①②皆空 ⇒ dns 必需类缺口 ⇒ C1 不成立，dns **不得**改列可选类；**禁止**把 overlay 内解析器排除（＝把隧道内 DNS 踢出隧道＋新造物理明网泄漏面）。**DNS 能力探测（同日只读实测）**：SDK 26.0.0.821 完全暴露物理网络 DNS——`getDefaultNet(Sync)`（`@ohos.net.connection.d.ts:152/:182`）、`getConnectionProperties(Sync)`（`:270/:289`）、`ConnectionProperties.dnses[]` 必填（`:2099/:2127`）、`NetAddress`（`:2249/:2258/:2267/:2276`）；无 `@systemapi`（grep 计数 0）；仅需 `ohos.permission.GET_NETWORK_INFO`（system_grant／normal／since 8，与已用 INTERNET 同级，不受 MANAGE_VPN 受限 ACL 约束）；非 VPN 网络可筛（`getAllNetsSync` `:230`＋`getNetCapabilitiesSync` `:349`＋`BEARER_VPN=4` `:2061`／`NET_CAPABILITY_NOT_VPN=15` `:1974`）；doc `:2093` 警告 `dnses` 可能为空。
2. **turn**：`absent_reason` 只对**本 build** 成立（`ice.rs` 硬拒 `turn:`，stun-only gathering，`client/core/src/ice.rs:103/:111-113`）；后续 build 启用 turn 路径即失效，turn 须回到 present 面重新登记，旧结论不外推。只钉歧义，不改类归属（turn 保持可选类语义）。
3. **wg_peer 代际**：「冻结解析集」＝`frozen_endpoints[]` 中 `present==true` ∧ `resolved` 非空的条目的 IPv4 地址集（按 /32）；`present==false`（如 turn）不参与集合包含比对、不构成缺行。C2 按代际判定（七条，见下节）；「首次 create 前 7/7」在本平台**结构性不可满足**（endpoint 须 ICE 选中对落配后才存在，`peer_conn.rs:1197-1216`）⇒ 接受「首代 6/7」的代际等价论证（前提＝首代无默认路由：`gateDefaultRoutes` 扣住 `:612-620`、recreate 腿同理 `:1327-1330`）。

### C2 代际判定七条（要点）

证据代 G＝采集器 arm 时刻之前最后一次被接受的 `create()` 所建立的连接代。(1) 每代在其 `create()` 之前输出恰一行 `VPN_EXCLUSION_GENERATION|requestId=<id>|gen=<n>|kind=initial|recreate|exclusions=<m>|wgPeerEndpoints=<k>`，`n` 自 0 单调递增、同 `requestId` 不重复；(2) 该代全部 `VPN_ROUTE_ENTRY|…` 行携带同值 `gen=<n>`，行集按 `gen` 分割，跨代混入即不可解析；(3) 逐条比对通过＝冻结解析集每个地址 A 在 `gen==G` 行集**恰**一行 `dest==A ∧ prefix=="32" ∧ isExcludedRoute=="true"`；(4) 接受锚点：`kind=initial`＝`VPN_CREATE_RESOLVED|…|accepted=true`，`kind=recreate`＝新增行 `VPN_RECREATE_RESOLVED|requestId=<id>|gen=<n>|accepted=true`；该代 generation 行与全部 `gen==G` 的 route 行须**早于**接受行；(5) `frozen_endpoints[].frozen_at_mono`（`n2h_collector.rs:1998-1999`）须晚于代 G 接受行、早于代 G+1 的 generation 行（若存在），否则冻结集与排除集不同代 ⇒ C2 不成立；(6) 首代（`gen=0`）出现 `isDefaultRoute=="true"`，或 `wg_peer` present 而代 G `wgPeerEndpoints==0` ⇒ C2 不成立；(7) 任一行缺失、`gen` 不可解析、`kind` 非二值 ⇒ C2 不成立（`n2h-inconclusive`）。配套**三条硬条件**：①证据轮禁用 `forceDefaultRoute`（`:612-618` debug opt-in，书面登记为证据轮纪律）；②首代无宽前缀吸入面（按 `gen` 分割逐行核）；③冻结集与排除集同代（第 (5) 款）。

### C10 第四类与 S20、fail-closed 扩面

- **C10④（代际变化）**：窗口内出现 `VPN_RECREATE_BEGIN`／`VPN_EXCLUSION_GENERATION` 的 `gen` 递增／`VPN_RECREATE_DONE` ⇒ 排除集在窗口内被重新定型而冻结集无法重冻 ⇒ 即 fail-closed 结束该窗口并按 `n2h-inconclusive` 登记（S8 同一纪律）。
- **S20**：arm 时刻 `connector_status().recreate` 满足 `cooling_down==true`，或 `exhausted==true ∧ reason=="limit-reached"` ⇒ 存在待决未落地的排除集变更，不得开窗；壳侧 arm 硬前置须与 `device_up && ready` 并列检查这两项（`NetBirdVpnExtensionAbility.ets:981-990`）；`exhausted` 分支同时对应既有未结义务 OB-10。
- **fail-closed 扩面（实质变更）**：任一必需类（management/signal/stun/wg_peer/dns）解析失败 ⇒ fatal（不建连）；relay/turn 保持可选类语义（带 `absent_reason`）；**产品行为变更登记**：STUN/DNS 解析抖动将导致不建连（用户可见、重启可恢复、无数据损失），定级**中–高**；证据轮因必需类解析失败未建连 ⇒ 记 `n2h-inconclusive`（判据设计行为，S8 纪律不重试、不换端点）；实现落点 `NetBirdEndpointExclusion.ets:293-296`（现行 `relayRequired && failed.length>0` 须改按必需类判定）。经人类批准「接受治理口径（推荐）」扩到 5 类必需端点。

### 附带 (ii) 转 OB-10

N8 侧静默 fail-open（`connector.rs` `RECREATE_MAX=3`／30 s 冷却耗尽后置 `required=false, reason="limit-reached"`，desired≠applied 永不再修、无标记、无 fail-closed）**不入本工作流**：经人类批准「登记为待议项（推荐）」，由 commit `c914cf6` 登记为 **OB-10**（台账 `docs/open-obligations-ledger.md:95-125`）；判据侧已入 v3（S20 的 `exhausted` 分支＋C10 第四类），实现侧按台账待议项处置（持有门建议 N6，待 T0 确认）。

### 变更性质判定表（哪些属实质变更入 v3）

| 变更 | 性质判定 | 入 v3 形态 |
|---|---|---|
| dns 类口径（项1） | **实质变更**（新增判据输入定义；剔除错误排除面并保留必需类） | C1 追加款（逐字） |
| 「冻结解析集」定义（项3 前置） | 钉歧义（口径澄清，不改判定强度） | C2 追加款 |
| turn 时效声明（项2） | 钉歧义（build 局部事实登记） | C2 追加款 |
| C2 代际七条＋三条硬条件（项3） | **实质变更**（新增判定规则与证据轮纪律） | 新节 §2.1（逐字）＋C2 指针 |
| 机制等价说明 | 说明性（供复审理解「首代 6/7」，不改变判定） | §2.1 |
| C10 第四类（附带④） | **实质变更**（新增触发类） | C10 追加款（逐字） |
| S20 | **实质变更**（新增停止条件） | §4-S20（逐字） |
| fail-closed 扩面（附带①） | **实质变更＋产品行为变更登记**（定级中–高） | C2 追加款（逐字） |
| residual 第九条 | 残余范围登记更新（探测结论驱动，降级残余项） | §8.1-8 |
| 头部/§0 版本状态/修订记录 | 程序性（G-4④ 程序落地） | 头部＋§0＋文末 |

全部变更方向＝**加严或纠错**，符合治理 `docs/native-nx-governance.md:35`；改动时点合法（设备侧 N2-H 证据尚未产出、G-4② 未发生，`:43`「测量后不得追认或修改」未触发）。

### 推荐路径与 v3 程序（裁定转写）

一次性收口 5 项前置进 v3（纯文档，不改代码）→ **跨厂商 T0 重审**（`openai/gpt-5.6-sol` 席，干净上下文；原席 `anthropic/claude-opus-5` 已任 v1/v2 与本轮，按评审独立性纪律不再任 v3 审席）→ **人类批准** → **定稿 sha256 重登记**仓B `records/`（并注明作废 v2 锚点 `bf04913959f7645ddde3a549a7ae4205a3cf34872a4fc7792f4e9296a30eb5be` 的据以测量效力）。重登记完成前 v2 仍为唯一生效文本，v3 不得据以测量、不得引用登记任何 verdict。人类批准 verbatim（2026-09-18，DSH Web GUI）：「批准开 v3（推荐）」「接受治理口径（推荐）」「登记为待议项（推荐）」。

### 新发现（F-N1…F-N5，要点转写）

- **F-N1**：dns 冻结集构成未定义——v2 只要求「dns 类 present=true」，未定义端点集；若把 `interface_dns`/overlay 内 server 误入冻结集或排除集＝把隧道内 DNS 踢出隧道＋新造物理明网泄漏面。
- **F-N2**：turn 缺席形态是 build 局部事实——`ice.rs` 硬拒 `turn:`（stun-only gathering），后续 build 启用即失效；`absent_reason` 不得跨 build 外推。
- **F-N3**：wg_peer endpoint 在首次 create 前结构性不存在（`peer_conn.rs:1197-1216`，ICE SelectedPair 落配后才存在）⇒「首次 create 前 7/7」不可满足；排除集跨代（initial/recreate）不可比，须按代际行集分割判定；首代无默认路由（`:612-620`/`:1327-1330`）是代际等价论证前提。
- **F-N4**：fail-closed 触发面过窄——`NetBirdEndpointExclusion.ets:293-296` 现行 `relayRequired && failed.length>0` 只护 relay 类；必需类（stun/dns）解析失败不触发不建连，与治理 `:126`「解析失败、端点变化或观测不符均 fail-closed」冲突。
- **F-N5**：arm 前置缺 recreate 状态检查——`cooling_down`/`exhausted(limit-reached)`（`connector.rs:176-177/:1339-1375`）时存在待决未落地的排除集变更，此时开窗 ⇒ 冻结集与排除集跨代不同源；其 N8 侧静默 fail-open 面即附带(ii)→OB-10。

### 不确定点（U-N1…U-N5，要点转写；均待真机验证）

- **U-N1**：`dnses[]` 运行时是否非空（doc `:2093` 允许为空；空值与「无 DNS」不可区分）。
- **U-N2**：`dnses[]` 是否即物理解析器（物理归属）。
- **U-N3**：VPN 生效后 `getDefaultNet`/`getAllNets` 的语义（d.ts doc 称 default net「is not a VPN network」，与常见平台行为存在张力）。
- **U-N4**：`GET_NETWORK_INFO` 声明后真机运行时授予与行为（声明层 system_grant/normal 已确认、与 INTERNET 同级；运行时待验）。
- **U-N5**：v3 新增标记行（`VPN_EXCLUSION_GENERATION`/`VPN_RECREATE_RESOLVED`）与 `VPN_ROUTE_ENTRY` 的 `gen=` 字段尚未在实现输出（HEAD `c914cf6` 零命中）——判据先行，实现落齐前按七条第 (7) 条记 `n2h-inconclusive`。

### 抽验引用要点（裁定所引 file:line 锚点；本节落盘会话于工作树 HEAD `c914cf6` 逐项只读复核一致）

- **壳侧**：`NetBirdVpnExtensionAbility.ets:612-618`（forceDefaultRoute debug opt-in，`gateAllowed = forceDefaultRoute` 实读）、`:1327-1330`（recreate 腿 `gateDefaultRoutes`）、`:981-990`（arm 前置 `device_up`/`ready` 检查）、`:1309/:1415`（`VPN_RECREATE_BEGIN`/`VPN_RECREATE_DONE` 已在）、`:32`（TAG `NetBirdVpn`）；`NetBirdEndpointExclusion.ets:293-296`（`relayRequired && result.failed.length > 0` 实读）；`NetBirdConnector.ets:3`（import `@ohos.net.connection`）与 `:693/:753`（仅 `getAddressesByName`；`getDefaultNet`/`getConnectionProperties`/`getAllNets` 在 `client/entry/src` 零命中）；`module.json5:43-46`（仅声明 `ohos.permission.INTERNET`）；三处 selfcheck 替身只 stub `getAddressesByName`（`client/entry/selfcheck/relay-exclusion-hooks.mjs:45-54`、`relay-flag-hooks.mjs:12`、`route-check-hooks.mjs:27-28`）。
- **core**：`n2h_collector.rs:1998-1999`（`frozen_at_unix`/`frozen_at_mono` 实读）；`peer_conn.rs:1197-1216`（`IceEvent::SelectedPair` → `apply_endpoint`）；`connector.rs:176-177`（`cooling_down`/reason token）、`:1339-1375`（exhausted／`limit-reached` 分支，`:1355` 置 `reason="limit-reached"`）；`NetBirdRouteValidation.ets:245-253`（`VPN_ROUTE_ENTRY` 行格式，无 `gen=` 字段）。
- **SDK 26.0.0.821（只读）**：`.../sdk/default/openharmony/ets/api/@ohos.net.connection.d.ts` `:152/:182/:230/:270/:289/:349`（六 API）、`:1974`（NET_CAPABILITY_NOT_VPN=15）、`:2061`（BEARER_VPN=4）、`:2093`（空值逐字警告）、`:2099/:2127`（`ConnectionProperties.dnses` 必填）、`:2249/:2258/:2267/:2276`（`NetAddress`）；`grep -c "@systemapi"`＝0；`.../openharmony/toolchains/lib/PermissionDefinitions.json`：`GET_NETWORK_INFO` grantMode=system_grant／availableLevel=normal／since 8（与 `INTERNET` 同 mode 同级，INTERNET since 9）。
- **治理与台账**：`docs/native-nx-governance.md:35`（「开门前细化」只能细化证据方法，不得删改或削弱）、`:43`（测量后不得追认或修改）；`docs/open-obligations-ledger.md:95-125`（OB-10 只追加节，commit `c914cf6`）。

- **本节登记边界**：本节只归档第三轮裁定会话的裁定要点（会话消息要点转写，非字节级副本）；不构成任何授权、不产生任何门级 verdict、不分配任何 AUTH-/EV- ID；v3 修订稿内容以 `docs/n2h-device-evidence-preregistration-draft-20260917.md` 为准。

---

## 跨厂商重审（2026-09-18，openai/gpt-5.6-sol 席）

> 本节为同一文件**追加**的跨厂商重审归档（**会话消息要点转写，非字节级副本**）；首轮（§1–§4）、第二轮复审与第三轮裁定归档不改写、内容不变。裁定原文的编号、要点与结论强度逐字保留自派发与回报消息；会话完整原文以当次会话记录为准，本文件不代为断言其逐字内容。归档会话只读核对仓库文档与代码；未修改任何文件、未运行构建、未执行任何设备命令。

- **审查对象与基线**：`docs/n2h-device-evidence-preregistration-draft-20260917.md` **v3**（432 行；工作树 HEAD `714dfb8`，干净）——对 v3（含第三轮裁定五项前置收口：dns 口径/turn 时效/wg_peer 代际七条/C10④+S20/fail-closed 扩面）的跨厂商 T0 重审。
- **审查席**：T0 独立审查，2026-09-18，`openai/gpt-5.6-sol` 席，**干净上下文单席**（spawn 形态，不带主会话已完成轮次）；**未参与 v1/v2 两轮与第三轮裁定、未执行实现**——满足跨厂商评审独立性（原席 `anthropic/claude-opus-5` 已任 v1/v2/第三轮，不再任 v3 审席）。单席，无第二席。
- **总裁定：修订后可生效**。v3 框架与第三轮收口成立，但须先关闭 6 项必改（R-1…R-6）；全部关闭并修订后，按「**重算 sha256 → 复审确认 → 人类最终批准 → 再登记**」程序生效——**修订本身不自动生效**，人类最终批准不得以 2026-09-18 三项旧批准代替。

### 必改项（R-1…R-6，要点转写）

- **R-1【高】C2 行集完整性缺可计算基数**：v3 七条规则只按「缺行」判失（第 (7) 款），日志截断掉一条宽路由时硬条件②（首代无宽前缀吸入面）可被假通，(7) 单靠「缺行」无法发现。→ 代际标记行增加 `routes=<r>` 字段；对 `gen==G`：`VPN_ROUTE_ENTRY` 行数**恰为** `r`；`index` 在该代内**唯一且连续覆盖 0..r-1**；`isExcludedRoute=="true"` 行数**恰为** `m`；`n` 自 0 起**逐次 +1、无跳号**；任一不符 ⇒ **C2 不成立**（`n2h-inconclusive`）。
- **R-2【高】「冻结集与排除集同代」无同一机器域绑定**：第 (5) 款靠 `frozen_at_mono` 时钟序比对，跨进程读点不可机械复核。→ 代 G 作 arm 输入并写入证据文档（新增 `collector.exclusion_generation` 字段）；壳侧固化 `N2H_COLLECTOR_ARMED|…|gen=G`；判定＝`collector.exclusion_generation`、ARMED 的 `gen`、最后一次被接受的 `VPN_EXCLUSION_GENERATION` 的 `gen` **三者逐字相同**且指向同一代（接受锚点按 C2 第 (4) 款）；缺任一字段或不一致 ⇒ C2 不成立。替代方案（所有相关标记固化同一时钟域 mono 并明定比较算法）**不推荐**——新增字段更可机械验证，故选前者。
- **R-3【中-高】冻结地址解析无唯一规范化算法、`forceDefaultRoute` 只有书面纪律**：→ **唯一规范化规则**（预注册级，不得测量时再定）：冻结解析集元素＝`frozen_endpoints[].resolved` 经**严格 IPv4 SocketAddr 解析**（`host:port`，取 host 部分为 A，按 /32）；`resolved` 非法（非 IPv4/无端口/IPv6/空）⇒ 该条计入 dns/对应必需类缺口（C1 不成立的候选），**不得静默跳过**。**硬条件①通过侧证据**（不再只是书面纪律）：冻结输入须含 `forceDefaultRoute=false`（或等价输入记录），且该轮完整日志**不得出现** `VPN_DEFAULT_ROUTE_FORCED` 标记；任一输入记录缺失、或该标记出现 ⇒ C2 不成立。
- **R-4【中-高】dns 口径留测量后择优口子**：v3 C1 的「必要时以 all-nets 筛选」可在测量后择优选分支。→ **预先钉死的唯一分支序**：步骤 1 取 default handle（`getDefaultNetSync()`）→ 步骤 2 该 handle 非 0，且 `getNetCapabilitiesSync()` 的 `networkCap` 明确含 `NET_CAPABILITY_NOT_VPN` 且 `bearerTypes` 不含 `BEARER_VPN` → 步骤 3 仅当步骤 2 不成立时才走唯一规定的 all-nets 筛选分支（`getAllNetsSync()` 逐个 handle → 取 `NOT_VPN` 且非 `BEARER_VPN` 者）→ 步骤 4 两路径的错误、空值、无法证明「物理归属」（无 NOT_VPN 证据、或同时含 VPN bearer）均使 dns 必需类缺口（C1 不成立）。**禁止测量后择优**；分支选择过程逐字记入证据 `notes[]`（`dns-branch=default|all-nets-filtered`、`dns-physical-evidence=not-vpn-cap|...`），分支记录缺失即缺口。**唯一允许的改写处**：原文保留＋紧跟替换说明。
- **R-5【中-高】S20 无通过侧证据契约**（部分实现可假通）：→ 壳侧 ARMED 行必须固化（逐字字段名、取值域取自 `connector_status().recreate`）：`recreateCoolingDown=false|recreateExhausted=false|recreateReason=<值>`；S20 触发时（`cooling_down==true` 或 `exhausted==true ∧ reason=="limit-reached"`）输出**稳定 NOT_ARMED reason**（逐字例：`N2H_COLLECTOR_NOT_ARMED|reason=recreate-pending|coolingDown=true|exhausted=false|recreateReason=route-set-changed`）；ARMED 行缺上述字段、或触发时未输出 ⇒ 按 **S18** 处置并记 `n2h-inconclusive`。
- **R-6【中，程序必改】v2 规范字节定位须逐字入 §0**：当前已登记生效的 v2 文本不是工作文件当前字节；唯一规范字节＝git commit `0368d5ddf9194585478c957e46e0442640e69048` 中该文件内容，全文 sha256 `bf04913959f7645ddde3a549a7ae4205a3cf34872a4fc7792f4e9296a30eb5be`，仓B `records/n2h-preregistration-effective-20260917.json` 及其 sidecar 为登记锚点；v3 在跨厂商重审、人类最终批准和新 sha256 重登记完成前**不具有据以测量效力**；2026-09-18 三项人类原话仅批准开启 v3、接受 fail-closed 口径与登记 OB-10，**不等于对 v3 定稿的最终生效批准**。

### 抽验要点（重审席核对所引代码事实；本节落盘会话于工作树 HEAD `714dfb8` 逐项只读复核一致）

- **generation/recreate-resolved/gen 标记在 HEAD 零命中**：`VPN_EXCLUSION_GENERATION`、`VPN_RECREATE_RESOLVED` 在 `client/entry/src/` 全树 grep 零命中；`VPN_ROUTE_ENTRY` 行（`NetBirdRouteValidation.ets:245-253`）不携带 `gen=` 字段（grep `gen=` 零命中）——判据先行，实现落齐前按七条第 (7) 条如实记 `n2h-inconclusive`。
- **collector 仍把 `interface_dns` 纳入 dns 冻结来源**：`client/core/src/n2h_collector.rs:478`（`dns_servers` ＝ `dns.servers[]` + `interface_dns`）、`:3162-3165`（`interface_dns` 以 `:53` 端口注入 `dns_servers`）——与 v3 C1 dns 口径（`interface_dns` 一律不入冻结集）不符，属实现面待办（v3 记录未决项 5），落齐前 C1/C2 如实不成立。
- **当前 build 不能通过 C2，但判据先行**：排除面 3/7（`NetBirdEndpointExclusion.ets:45-51`）＋代际标记/gen 字段未落盘 ⇒ 按 v3 既有登记「本轮预期 C2 不成立 → `n2h-inconclusive`、排除面补齐 7/7 前不得登记通过类结论」；判据不因此削弱。
- 其余锚点复核沿用第三轮裁定节：`frozen_at_mono`（`n2h_collector.rs:1998-1999`）、recreate 状态字段（`connector.rs:170-179`：`cooling_down`/`exhausted`/`reason`，reason token 域 `none`/`route-set-changed`/`limit-reached` 见 `:177`，`:1355` 置 `limit-reached`）、arm 前置检查（`NetBirdVpnExtensionAbility.ets:981-990`）、ARMED 行现行格式（`:1058`，现无 `gen=` 与 recreate 字段——R-2/R-5 落盘项）。

### 重登记要素清单（修订后生效前逐项落齐）

1. **新定稿字节锚点**：定稿路径（`docs/n2h-device-evidence-preregistration-draft-20260917.md`）、定稿时 git commit、全文 sha256、字节与行数（`wc -c`/`wc -l` 原值）；
2. **审查席与记录锚点**：跨厂商重审（本节）＋修订后**复审确认**记录（确认 R-1…R-6 已关闭）；
3. **人类最终批准原话**：重审通过后新取得的一次批准，逐字登记——**不得以 2026-09-18 三项旧批准（开 v3/接受 fail-closed 口径/登记 OB-10）代替**；
4. **带时区生效时刻**；
5. **逐字作废声明**：「自该生效时刻起 v2 锚点 `bf04913959f7645ddde3a549a7ae4205a3cf34872a4fc7792f4e9296a30eb5be` 的据以测量效力作废，仅保留历史审计效力」；
6. **campaign AUTH 逐字引用新 v3 sha256**（AUTH 绑定登记）；dry-run 诊断 AUTH 不引用该 sha256、不消费任何 EV/pair ID；
7. **G-4④ 续行**：生效后任何改动＝新版本＝重新独立审查＋重新登记 sha256；
8. **登记记录带 `.sha256` sidecar**；禁词自检按 §8.2 固化三点（定稿时/每份证据文档落盘时/每份执行记录落盘时）各实跑一次。

### 不确定点（真机项保持未验证）

真机项一律保持未验证：U-N1 `dnses[]` 运行时非空性、U-N2 是否即物理解析器、U-N3 VPN 生效后 `getDefaultNet`/`getAllNets` 语义、U-N4 `GET_NETWORK_INFO` 运行时授予与行为；U-N5 扩面——v3.1 新要求的标记与字段（`routes=`/`index=`/`gen=`、ARMED 的 `gen` 与 recreate 字段、`VPN_DEFAULT_ROUTE_FORCED`、`collector.exclusion_generation`、dns 分支 `notes[]` 记录）均未在实现输出（HEAD 零命中，判据先行）。落齐并真机验证前不得据以登记任何通过类结论。

- **本节登记边界**：本节只归档跨厂商重审会话的裁定要点（会话消息要点转写，非字节级副本）；不构成任何授权、不产生任何门级 verdict、不分配任何 AUTH-/EV- ID；v3.1 修订稿内容以 `docs/n2h-device-evidence-preregistration-draft-20260917.md` 为准。
