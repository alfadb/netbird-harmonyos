# T0 材料包：N1b 判据两轮失败处置、E4 范围管辖与发现门授权（草案 v1，供 T0 审议）

最后核验：2026-08-31 ｜ 状态：`t0-ballot-complete-pending-user-approval`（三席跨厂商 T0 表决 + 一席 T0+ 终裁已完成，见 §0。**正文 §1-§6 保持提交时原样，作为表决对象的字面记录；其中已坐实的事实错误由 §0.1 更正并优先于正文**）

本文为 [native N1-Nx 治理决议](native-nx-governance.md)（`ADJ-T0-NATIVE-NX-20260830-0001`）下 N1b 门的后续治理提交。触发原因：N1b 判据经两轮共五席跨厂商独立审查**全部 fail**，且失败根因中有两项属治理级、一项属门顺序变更，**均超出判据起草人权限**。

本文不请求：产品实现授权、E8 开门、物理执行授权、N1b 判据冻结。

## 0. 表决结果（2026-08-31）

三席跨厂商 T0 隔离独立表决 + 一席 T0+ 终裁。三席互不可见，主会话只编排不投票。

| 席位 | 模型 | Q1 | Q2.1 | Q2.2 | Q3 | 总体 |
| --- | --- | --- | --- | --- | --- | --- |
| 甲 | `xai/grok-4.6` | roadmap 原义 | 另拟（不 fail 不 blocked，义务不关闭） | (c) 另拟 | 有条件授权 | 附条件批准 |
| 乙 | `openai/gpt-5.6-sol` | roadmap 原义 | **(b) overall blocked** | 另拟（唯一受控 worker） | 有条件授权 | 附条件批准 |
| 丙 | `deepseek/deepseek-v4-pro` | roadmap 原义 | (a) 不阻止 pass | (b) 移出门控清单 | 有条件授权 | 附条件批准 |

**三席收敛（3/3，无需终裁）**：Q1 主轴（E4 义务定义以 `docs/roadmap.md:348` 为准，§二.8 括注是映射标签，其同句「映射不免除义务」排除删减解释）；Q3 有条件授权但**否决 §4.2 的 D1-D7 草案**；`verdict: collected` 非法；D7 置于 destroy 之后是设计错误。另 2/1 解决：DISC 沿 G0 流程范式（反对席误将「十三门」读作十三条功能门，`docs/g0-go-arm64-physical-probe.md:174-188` 显示其为 host 审计→freeze→审查→DryRun→单次 Live 的流程纪律）。

**五处未收敛，经 T0+ 终裁**：D1 背压不可诱发的处置、D2 shutdown unblock 补救、D3 C10 是否拉回、D4 是否拆 ID、D5 IPv6 去向。

**T0+ 利害声明**：终裁席 `anthropic/claude-fable-5` 与本材料包及 r1 判据的起草人（主会话 `anthropic/claude-opus-5`）同属 claude 家族，**不构成对本材料包的独立评审票**；其职权严格限于在三席已独立产出的对立立场之间终裁。

### 0.1 已坐实的事实错误（主会话逐条核验，优先于正文）

| # | 正文位置 | 错误 | 核实原文 |
| --- | --- | --- | --- |
| F1 | §4.2 | 「产出 `verdict: collected`」**违反 schema** | `docs/evidence-schema.md:137` `collected` 属 `record_status`；`:160` `verdict` 仅 `pass\|fail\|blocked\|invalid` |
| F2 | §4.2 D2 | 「产出实际 `interface` 名」**该 API 不存在** | `@ohos.net.vpnExtension.d.ts:157` `create(config): Promise<number>`；`:147` 注释「returns file descriptor」。D2 至多验证预注册 interface 输入是否被接受 |
| F3 | §4.1 U6 | 称 `isBlocking` 默认值未知 | `@ohos.net.vpnExtension.d.ts:296` 注释明写 default **false**（声明在 `:302`）。未实测的是运行时 fd flags，不是默认值 |
| F4 | §4.1 U5 | 「`RouteInfo.interface` 必填 → VpnConfig 无法冻结」**过重** | `VpnConfig.routes?` 本身可选（`:246`）；E3 已在**同一冻结元组**上以仅 `addresses`、无 routes 成功 `create()`（`spikes/e3-vpn-extension-physical-preflight-hap/.../E3PhysicalVpnExtensionAbility.ets:138-151`）。真实约束是「带 RouteInfo 的配置无法冻结」 |
| F5 | §4.2 | 自相矛盾：D1 称「同时构成 §二.8 所需 arm64 加载证据」（`:107`），前言称 DISC「不构成任何门结论」（`:100`） | 材料包内部 |
| F6 | §3.1 | 把 Q2.1 作为开放选项提交，**未披露 r1 已按 (a) 落笔** | `docs/n1b-gate-plan.md:174` 已含「attempted, not induced on this fd」原措辞；`:196` 聚合已允许 `not-triggered` 后 overall pass |
| F7 | §2.1 | 框为「两份文本冲突」，**漏第三份** | `docs/roadmap.md:36,278,280,361,688`：「E4-E7 完整义务移交 E8 `OPEN` 后同一具名设备的 R2/R3 门」 |
| F8 | §4.1 U3 | 引 `noise/mod.rs:501,504` 指按 total_length 截断 | `:501` 是 `rx_bytes += computed_len`；截断切片在 `:504` |

F1 与 F2 各自单独即足以使 §4.2 的 D 项草案不可执行。

**行号偏移提示**：本 §0 内的行号引用已按 2026-08-31 编辑后的现状校准。**正文 §1-§6 的行号引用停留在提交时状态**（保留原样纪律），其中指向 `docs/native-nx-governance.md` 的需 **+2**、指向 `docs/n1b-gate-plan.md` 的需 **+10**，才对应当前文件；指向 `roadmap.md`、`r0-charter.md`、`n1a-gate-plan.md`、SDK `.d.ts` 与 BoringTun 源码的行号未受影响。

### 0.2 起草人偏向登记

材料包在五处做出对起草人有利的框架选择：以 §二.8 括注当范围文本（F7 加剧）；不披露 E3 无路由 create 先例（F4，抬高未知计数、强化 DISC 必要性）；不披露 r1 C7 已按 Q2.1(a) 落笔（F6，把追认装成开放选择）；把 C10 划出表决范围（§5，见 §0.3 D3）；D1 夹带 §二.8 加载结论（F5）。起草人仅就 Q1 自陈偏向，其余四处未自陈。

### 0.3 T0+ 终裁（五项）

- **D1 背压不可诱发 — 采纳甲**。C7 三态维持，`not-triggered` 不 fail、不 blocked、不阻止 overall pass；主张上限逐字写死（禁止出现「tun fd 背压已验证」，措辞固定为「attempted, not induced on this fd」，且须携带定量尝试参数：累计写入字节、写调用次数、时间盒起止单调时钟，缺失按 C11 字段缺项 fail）；**§二.2 背压义务不关闭**，入开放义务台账，关闭仅限同元组后续具名 campaign 观测到 `induced`，或新 T0 书面放弃；同一口径适用 FD-L5 部分写。N6/N7 登记被动观察字段承接。r1 `:174`/`:196` 既成措辞予以追认，材料包未披露该事实登记为提交瑕疵。
  否乙理由：本门 blocked 语义（`:198`）限于**无法求值**，而有界 write-until-EAGAIN 是完成了的测量、产出确定负面观察；在 §三单次不重试约束下，乙规则把平台良性结局自动转成烧 ID + 回 T0，而 T0 对「诱发不出」无新指令可下。
- **D2 shutdown unblock — 采纳甲，乙的测量规格绑定并入**。FD-P1 现文（`:95`）**废止**（恒真判据不得保留）；r2 标记 `not-measurable-under-sync-nonblock-probe`，overall 不因该项未测而 fail；**不开线程、不从 §二.2 删除**。DISC 重拟稿新增 **D-W** waiter 观察项：恰好一个登记在册 worker 于 destroy **之前**经 barrier 确认进入对 dup 副本的有界等待，随后 destroy，以单调时钟登记返回时刻、事件集、与自身 timeout 的区分、join 结果、是否被 watchdog 杀；全部结局均为观察事实。**与 D6 共用同一 destroy 事件**，无需第二次 destroy。预授权两条关闭路径（①waiter 可存活 → r2 纳入真判据，C9 改为「除该唯一登记 waiter 外禁止自建线程」；②不可执行或 destroy 后不可观察 → 转 N6/E7 并援引 N0 停止条件 3 书面登记），**不再为此单开 T0**。
  否乙（即时开 worker）：押注 U7 与「destroy 必唤醒 dup poll」两个未实测前提。否丙（移出+「本元组不可测」）：不可测的根源是 r1 冻结的全同步/非阻塞探针架构（`:61`、`:63`、`:176`），**不是元组本身**，该措辞过宽且制造无归宿义务。
- **D3 C10 僵局 — 采纳甲**。C10 的聚合后果**拉回 T0 管辖**（起草人「判据层自行修订」的分类不成立——改变 blocked 的聚合后果即改变本门可通过性条件，属 §二.10 阈值变化）。实体维持三席结论：`post-destroy-unobservable` → overall `blocked`，并登记「经三席确认 + 终裁追认，构成收紧而非削弱」。预授权三分支出口：U4 真 → r2 保留 C10；U4 假 → r2 不得保留押注 U4 的 post 门控子项，按 D2-② 转移并重划 C10；DISC 判真而 N1b 现不可观察（矛盾）→ overall blocked、消费 ID、强制回 T0 并登记矛盾本身，**禁止以判据循环改写另开执行**。乙主张拉回的时间盒表与 P2 样本数/窗口**不拉回**（属 §二前言「细化证据方法」，P2 样本窗从未被 T0 锁值），但绑定为 r2 冻结前置，缺失即审查 blocker。
- **D4 拆 ID — 采纳甲（实体），程序上当庭完成论证**。径直批准独立 DISC（第二个 AUTH/pair + evidence ID），不退回重拟论证。理由载入决议：§二.7 要求测量前预注册，而 U1-U7 未知时功能判据只能写成赌注——两轮五席 fail 已实证该判据在此信息状态下写不稳；结局不对称（DISC 无功能 fail，一次执行确定性收齐七项；合并方案在 S8 冻结顺序下首个假前提即终止、事实收获截断，且 U3 为假会以功能 fail 形态烧掉正式门）。**代价逐字登记**：拆分保证消耗两个 AUTH/pair 与两个 evidence ID；合并方案在七个 U 全真时可省一个 ID。选项 C（同 campaign 内分阶段发现）登记「已考虑、被否」。补三席未给的 F1 具体改法：schema 持有人在 N 门代码扩展同批登记发现型 campaign 记录形态，`verdict` 仍限现枚举且**仅对基础设施/完整性求值**（infra 干净完成即 `pass`），全部平台事实落观察字段，并写死 DISC 的 `verdict: pass` 不得在任何后续文档中被引用为平台行为结论；扩展完成前不得分配 DISC 证据 ID。
- **D5 IPv6 — 新立场（甲为基底）**。N1b 记 `N/A-undeclared`（本门冻结 VpnConfig 无 IPv6 地址/路由，SDK `isIPv6Accepted` 默认 false，声明范围为空）+ 甲的触发条款（任何后续门冻结配置一旦声明 IPv6 地址或路由，该门同时承担 E4 IPv6 核验，缺列即审查 blocker）。**新增（三席均未提出）**：`docs/r0-charter.md:94` 的首轮 IPv6 探测义务（「必须探测并记录能力与失败边界」）显式指派 **N6** 承接为观察类探测项（非门控，不影响 N6 pass），N6 判据缺该项即审查 blocker。R2/R3 兜底（`docs/roadmap.md:280`）不变。
  否乙（N1b 承担最小切片）：向单次不可重试的冻结配置加入 IPv6 声明会引入新的未实测前提，与 D4 消除赌注的方向冲突，且章程明文 IPv6 非首个 0.x 强制功能。

### 0.4 新增条款：开放义务台账

Q1/D1/D2/D5 四处分歧的共同根因是「去向另行治理指派」（`docs/n1b-gate-plan.md:27`、`:229`）**无结构化载体**——义务靠散落括注传递，每次都长成新的 T0 争议。决议须建立单一登记表，逐条列出未被 N1b 关闭的 §二.2/E4 义务：义务原文出处、持有门、关闭触发、当前状态；变更仅经 T0 或已预授权路径。首批登记项：背压（D1）、部分写（D1）、shutdown unblock（D2）、post-destroy 子项（D3，视 U4）、E4 剩余面（DNS→N2a；多地址/多路由/MTU 变更与重建→N6，R2/R3 兜底）、MTU 实际生效 oracle、IPv6 探测（D5）。

### 0.5 自洽性结论与残余风险

**D1 义务开放 × D2 义务不删除 × D3 不可 pass 僵局三者叠加，不产生死锁。** 现实通过路径：DISC 重拟（含 D-W、重排、代价登记）→ 独立审查 → 用户授权 → DISC 单次执行收齐 U1-U7 与 waiter 事实 → r2 依**实测事实**冻结（择 D2 关闭路径、划定 C10）→ 独立审查 → 用户授权 → N1b 单次执行。届时 premise 类 blocked 已被 DISC 排干，剩余失败模式是真功能失败（握手、双链完整性）——正是门应当测的东西。唯一残余 blocked 风险是 DISC↔N1b 矛盾分支（D3-c，已预授权处置）。

**终裁自陈的两点不确定**：(1) D4 拆分优越性依赖对 U1-U7 为真概率的**未量化先验**——若七项实为平台常态，合并方案本可高概率省一个不可复用 ID；这是风险偏好判断，不是测量结论。(2) `destroy()` 是否唤醒 dup 副本上的 poll waiter，本元组未核实（Linux 通用语义下 close 单个 fd 不必然唤醒同 OFD 其他 fd 的 poll）；D-W 以「任何结局皆有用事实」对冲，但若 waiter 在 destroy 前触发 watchdog 杀进程，D6 事实将一并损失——D-W 的暴露窗口与相对 D6 的排序须由 DISC 重拟稿论证并经独立审查把关。

## 1. 触发事实：两轮审查基线

| 轮次 | 席位 | 模型 | VERDICT | B / M / m |
| --- | --- | --- | --- | --- |
| r0 | A | `xai/grok-4.6` | fail | 8 / 9 / 6 |
| r0 | C | `deepseek/deepseek-v4-pro` | fail | 6 / 8 / 8 |
| r0 | B | `openai/gpt-5.6-sol` | **attempt-not-counted** | 运行未干净完成，按 `EV-E3-PHYS1API26-20260807-0002` 先例不作 verdict |
| r1 | A | `xai/grok-4.6` | fail | 6 / 10 / 4 |
| r1 | B | `openai/gpt-5.6-sol` | fail | 10 / 7 / 2 |
| r1 | C | `deepseek/deepseek-v4-pro` | fail | 1 / 1 / 5 |

- 所有审查席均为隔离上下文、跨厂商；主会话模型（`anthropic/claude-opus-5`）因撰写 r1 而以作者身份与主会话身份双重排除，claude 家族未占任何席位。
- r0 从未冻结、从未开始测量；r1 同样未冻结。两轮修订均不触发 §二.7 的「测量后不得修改」。
- r1 相对 r0 已修复的项（三席一致确认可冻结）：单条闭环改两条独立单向链、握手/drain/keepalive op 序列与 BoringTun 0.7.1 逐字一致、吞吐判据删除、执行位点收敛到 VpnExtension、门代码扩展前置、双轴与终态优先级、N0 五项停止条件。
- **r1 的 blocker 三席独立收敛于同一条**：C10 允许 `post-destroy-unobservable → blocked → overall pass`，使门可在未建立 §二.2 post-destroy 义务时 overall pass。该条已确认必须改为 overall `blocked`，属判据层可自行修订，**不在本次提交的表决范围**。

## 2. 决议事项一（Q1）：E4 范围管辖与 N1b 的实际义务

### 2.1 文本冲突（两处原文逐字对照）

| 来源 | 原文 | E4 覆盖面 |
| --- | --- | --- |
| `docs/roadmap.md:348` | 「**E4 `setUp`/TUN 配置**：实际调用 `setUp` 建立虚拟接口并核验 fd、地址、**路由**、**DNS**、MTU、IPv4 及声明范围内 **IPv6** 的配置与清理。」 | fd、地址、路由、DNS、MTU、IPv4、IPv6、清理 |
| `docs/native-nx-governance.md:42`（§二.8） | 「**E4（TUN 配置/地址/MTU/IPv4/清理）**与 E6（双向泵）映射到 N1a/N1b，E5→N2b，E7→N2/N6，SLO→N7，**映射不免除义务**。」 | TUN 配置、地址、MTU、IPv4、清理（**括注不含 DNS、路由、IPv6**） |

### 2.2 为什么必须由 T0 裁定

判据起草人（本会话）在 r1 中把 DNS 与多路由义务写为「去向另行治理指派」，据的正是 §二.8 括注的较窄表述。审查席 B 指出这构成**未经授权的范围再分配**（§二.10：门范围、顺序、阈值变化必须回 T0）。

**起草人自陈**：该解释方向恰好减轻本门自身负担，属于对自己有利的歧义解释。判据起草人不应是该歧义的裁定者。

### 2.3 请求裁定

1. 两份文本冲突时以哪份为准（§二.8 括注 / roadmap E4 原义 / 另行界定）；
2. N1b 实际承担的 E4 义务清单（逐项）；
3. 未由 N1b 承担的 E4 剩余义务的**明确去向**（不得留白，§二.8「映射不免除义务」）；
4. 附带：若 N1b 只承担「本门冻结 VpnConfig 实际行使的那一份」，须明示该收窄是否本身构成范围变更。

**额外事实**：审查席 B 指出，即便只看当前配置，约 1052 B 的测试包也不能证明设备实际生效 MTU 为 1400（生效 1280 仍可通过）。因此「MTU 已覆盖」的任何主张都需要**实际生效 oracle**，而非仅登记提交值——这一点无论 Q1 如何裁定都成立。

## 3. 决议事项二（Q2）：§二.2 中 tun fd 背压与 shutdown unblock 的可满足性

§二.2（`docs/native-nx-governance.md:36`）逐字要求实测：「…**EAGAIN/部分写/背压/shutdown unblock**、EBADF/复用/泄漏」。两项在真实 tun fd 上出现结构性障碍：

### 3.1 背压：不可诱发时如何处置

N1a 对**外层 UDP socket** 的 C5 曾裁定 `not-triggered ≠ fail`。但 §二.1/§二.2 把 N1a 与 N1b 分开，且 N1b 须**自行建立** fd 合同，N1a 的 UDP 信道裁决不能自动外推到真实 tun fd。

请求裁定：有界时间盒内**未能诱发** tun fd 背压时——

- (a) 记 `attempted, not induced`，不阻止 overall pass（主张上限须写死）；或
- (b) 视为该绑定义务未建立 → overall `blocked` + 回 T0。

### 3.2 shutdown unblock：与「探针不得自建线程」的结构冲突

审查席 B 证明 r1 的操作化是**恒真的**：destroy 之后才调用一个自带 500 ms 超时的 `poll`，其按时返回可完全由自身超时造成；而 r1 同时冻结了全同步执行、tun fd 非阻塞、无 probe worker，**destroy 发生时根本不存在正在等待、需要被解除的操作**。

真正的 shutdown unblock 实测要求：一个在 destroy **之前**已确认进入等待态的阻塞操作，并以单调时钟区分「fd 事件唤醒」与「自身 timeout」。这需要并发，**与 C7/C9 沿用自 N1a r3 的「静态断言无 `pthread_create` / `std::thread::spawn` / NAPI worker」直接冲突**。

请求裁定：

- (a) 为 N1b 开放受控的探针工作线程（并相应调整资源门的静态断言范围）；或
- (b) 认定 shutdown unblock 在本元组不可测，正式将其从 N1b 的门控清单移除并登记原因；或
- (c) 其他 T0 指定的操作化方式。

## 4. 决议事项三（Q3）：N1b-DISC 发现门授权（§二.10 门顺序变更）

### 4.1 问题：单次不可重试的 campaign 押注七个未实测平台事实

r1 的设计依赖以下**在本冻结元组上从未实测**的平台行为。每一项为假都足以单独打掉一次不可复用的物理 campaign：

| # | 未实测事实 | 若为假的后果 | 现行 r1 分类 |
| --- | --- | --- | --- |
| U1 | 应用自身 socket 发往本 VPN 路由覆盖地址的流量是否投递到本应用 tun fd | OUT 链无明文来源，整门无法执行 | blocked（P1） |
| U2 | 内核是否把 tun 注入包投递到本应用 sink socket | IN 链无法闭合 | r1 误分类为 C6 **fail**（三席指出应为 blocked） |
| U3 | tun 帧格式：是否含 PI 头/填充；read 长度与 IPv4 `total_length` 的关系 | 逐字节比对必然假 fail（BoringTun 按 `total_length` 截断，`noise/mod.rs:501,504`） | 未登记 |
| U4 | `destroy()` 返回后进程能否同步执行 `fcntl`/`close`/`read` | post-destroy fd 合同不可建立（C10 症结） | 未登记为前提 |
| U5 | `RouteInfo.interface` 的实际接口名 | VpnConfig 无法冻结（该字段必填） | 未登记 |
| U6 | `isBlocking` 默认值及 tun fd 初始 flags | FD-L3 无 before 基线、FD-L4 前提不成立 | 未登记 |
| U7 | Extension 对长同步任务的 watchdog/AppFreeze 行为 | 泵中途被杀，落入不可分类终态 | 未登记 |

U5/U6 的 SDK 依据：`@ohos.net.connection.d.ts` 的 `RouteInfo` 必填 `interface`/`destination`/`gateway`/`hasGateway`/`isDefaultRoute`；`@ohos.net.vpnExtension.d.ts:302` 的 `isBlocking?: boolean`。

**风险结构判断**：这是风险分配问题，不是判据措辞问题。判据文本再精确也无法消除七个并发赌注；继续在 r2/r3 上迭代措辞，最可能的结局是 campaign 停在某个前提上，消耗一个不可复用 ID 换回若干平台事实——即**用正式门的代价做发现工作**。

### 4.2 请求授权：一个只测事实、不可功能 fail 的发现 campaign

在 N1b 正式门之前插入 `N1b-DISC`。其设计要点：

- **无功能判据**：所有 D 项均为观察项，产出 `verdict: collected` 与事实登记。**唯一的 fail/blocked 来源是基础设施与完整性**（设备连接、构建输入漂移、封签失败），平台行为「不如预期」永远不是 fail。
- **增量落盘**：每个 D 项完成即发射 marker 并落盘，任一后续步骤崩溃不损失既得事实。
- **不主张任何 N1b 判据**：DISC 的结论只作为 N1b 判据的输入事实，不构成 fd 合同、数据面或 E4 的任何门结论。

最小 D 项草案：

| # | 内容 | 产出 |
| --- | --- | --- |
| D1 | arm64 native 成员在 Extension 进程内 dlopen + 符号解析 | 加载事实（同时构成 §二.8 所需 arm64 同核心加载证据） |
| D2 | 以候选 VpnConfig 调 `create()` | 实际接受值、`interface` 名、fd 号、fd 初始 flags、`isBlocking` 效果（U5/U6） |
| D3 | 从 tun 读取内核自然产生的包，dump 前 N 字节十六进制与长度 | tun 帧格式（U3） |
| D4 | 应用 socket 发往路由内地址，观察是否出现在 tun | U1 |
| D5 | 向 tun 写入合成包，观察 sink socket 是否收到 | U2 |
| D6 | `destroy()` resolve 后同步尝试 `fcntl`（原始/副本），记录全部 errno 与「该段代码是否执行到」 | U4 |
| D7 | 有界长同步任务（登记时长），观察 watchdog 行为 | U7 |

顺序为 D1→D2→D3→D4→D5→D6→D7（D7 置于 destroy 之后，避免其风险污染前序观察）。

**为什么这是门顺序变更**：新增前置 campaign 改变 N1-Nx 骨架的执行顺序，按 §二.10 须回 T0；且它消耗一个独立 AUTH/pair 与 evidence ID，属 pre-E8 native 物理例外（§三）的一次新行使。

### 4.3 请求裁定

1. 是否授权 `N1b-DISC` 作为 N1b 的前置发现 campaign；
2. 若授权：其证据身份形态、是否沿 G0 13 门范式、DISC 结论可否直接作为 N1b 判据的预注册输入；
3. 若不授权：N1b 应以何种方式处置 U1-U7（全部预注册为 blocked 前提并接受高概率 blocked 结局，或其他）。

## 5. 附：已确认属判据层、不在本次表决范围的事项

以下由三席指出、可由判据起草人在 r2 中自行修订，登记于此仅为完整性，不请求 T0 表决：

C10 改为 overall blocked；IN 链绑定 `recvfrom` 源身份并新增对称前提；`tx_bytes`/`rx_bytes` 字节账口径改为完整 IPv4 长度；C9 的 T0 快照时刻钉死在探针 socket 创建之前；结果状态机补 blocked/not-run/failed-before-marker；HiLog tag/domain 与 chunk 协议冻结（含 `:vpn` 截断 tag 三形态关联规则，先例 `AUTH-E3-PHYS1API26-20260814-0001` 曾因此消耗一个物理 pair）；ffi 符号集逐字枚举；dst 缓冲最小尺寸冻结（`encapsulate` dst 过小会 panic，ffi panic hook `raise(SIGSEGV)`，`ffi/mod.rs:304-309`）；包身份字节序与方向常量；外层信道 EAGAIN 重试纪律；`tunnel_free` 措辞更正（无自动 Drop）；时间盒表补全并定义单调时钟锚与优先级；P2 负面判据的样本数与完整窗口；构建输入扩展到工具链与锁文件哈希；资源快照 `T0` 与阶段 `PH0` 的命名冲突。

## 6. 提交边界

本文为审议材料，非决议。表决通过前：N1b 判据不得冻结、不得开始任何测量、不得分配 `N1b-DISC` 的 AUTH/pair 或 evidence ID。任何物理执行仍须用户显式授权新 AUTH（治理 §三）。
