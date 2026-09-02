# N1BDISC r9 —— 死亡事实记录规范（归因停机后的替代设计）

最后核验：2026-09-02 ｜ 状态：`spec-archived-superseded-by-criteria-text`

本文是 r9 结构性改造的**规范输入**，不是判据本体。r10 第十轮审查后判据正文已多处超出本规范（七分量真值表、多条目聚合、五态分量值补全等）——**凡本规范与 `docs/n1b-disc-gate-plan.md` 冲突处，一律以判据正文为准**；本规范保留作为设计依据与裁量登记的历史记录。

依据：`docs/n1b-disc-r8-review-register.md` 第六之三/六之五节记载的 **T0 裁决 3/3 一致**（席 A grok-4.6、席 B gpt-5.6-sol、席 C deepseek-v4-pro）。裁决要点：停止用死因归因驱动 `verdict`；改为证据向量三态记录 + 窄正向崩溃签名 fail；行 5/6/7 的 fail 映射一并废除；**不需要新 T0 决议**（本改动是判据向决议 §4.2 收敛）。

---

## 一、主会话在两席设计分歧上的裁量（须登记，供审查席挑战）

两席对「死因分类表拆掉之后留什么」给出**不同**方案：

- **席 A**：死因表**可以留作观察分类**（给 N1b 设计者看「watchdog / destroy terminal / 未知」），只需从 finally 步骤 4/9 与 D-W 终态路径的 fail 触发器上拆掉。
- **席 B**：把**互斥因果类改成证据向量**，各分量各自三态——即替换，不是保留。

**主会话裁定：采用席 B 方案，完全删除因果分类表，不保留观察版。** 理由三条：

1. **席 B 的核心反对意见针对的正是「互斥因果类」这个数据结构本身**，而不只是它接到了 fail。保留一张观察版因果表，等于保留「必须在若干互斥标签中选一个」的强制——而证据恰恰不足以确定该选哪个。表还在，「哪一行优先」「else 落哪」这类缺陷就还会长出来：r0–r8 九轮里，该表贡献的 blocker 数远超其他任何单一构造。
2. **席 A 保留它的理由（给 N1b 设计者看）由证据向量严格更好地满足。** 读到 `process_death_observed=observed-true` / `last_visible_site=P6` / `fault_type_observed=APPFREEZE` / `marker_tail_state=possible-tail-loss` 的设计者，比读到单一标签 `platform-termination` 得到**更多**信息，且不被一个不可靠的合成结论误导。
3. **九轮的经验数据支持「减少相互作用的规则数」**。表与向量并存会新增一层「表如何由向量导出」的映射规则，那是又一个缺陷面。

**残余与风险**：本裁定与席 A 的明确建议相左。若审查席认为 N1b 侧确实需要一个合成标签，正确做法是在 **N1b r2 的判据里**由 N1b 起草人依 DISC 实测事实自行合成，而不是由 DISC 预先合成一个它的证据支撑不了的标签。**本条请审查席重点挑战。**

---

## 二、证据向量（替代因果分类表的规范记录）

死亡相关事实按下列**七个独立分量**逐项落盘。各分量**互不推导**、各自三态（`observed-true` / `observed-false` / `unobservable(cause=...)`）或取自各自的闭合枚举。**任何分量都不得由其他分量的缺失反推。**

| 分量 | 取值域 | 说明 |
|---|---|---|
| `process_death_observed` | 三态 | `:vpn` 进程是否被观察到消失。`observed-true` 须有独立死亡证据（faultlogger 条目 **或** 进程退出记录 **或** `PidOfVpn` 在有 positive baseline 前提下转 absent + capture 静默）。**禁止以 marker 缺失单独推断被杀**（E3 0001 误判教训沿用）。 |
| `last_visible_site` | 位点枚举 ∪ `unobservable` | capture 中最后一个成功发出的**阶段 marker** 所在位点。这是 **capture 可见性**的判定，**不是进程执行史的断言**。 |
| `fault_type_observed` | 归一化闭集 ∪ `other:<literal>` ∪ 三态 | faultlogger 条目的 `Fault_Type`，按既有归一化规则（去 `_`/`-`、大写）求值；域外字面记 `other:<原字面逐字>`；无条目记 `observed-false`；条目存在但字段不可解析记 `unobservable(cause=...)`。 |
| `signal_observed` | 信号枚举 ∪ 三态 | 同上口径。 |
| `destroy_call_state` | 见第四节五态表 | destroy 调用边界状态。 |
| `marker_tail_state` | 见第五节 | 尾 marker 丢失的不确定性登记。 |
| `probe_crash_signature_observed` | 三态 | **唯一进入 `verdict` 的死亡侧分量**，定义见第三节。 |

**记录义务**：七个分量在任何执行路径上都必须各自有值（含 `unobservable` 及其预注册 cause）。**七者之间不存在优先级，不存在 else 兜底，不合成任何单一「死因」标签。**

**明确删除**：原死因分类表（7 行）、其行间优先级、其 else 兜底行、以及 `probe-fault` / `platform-termination` / `unattributed` / `fault-type-unrecognized-no-platform-signature` 四个合成标签，全部删除。原 `unattributed` 若需在事实层保留可读性，改名为 `unobservable(cause=death-cause-indeterminate)`（席 B 建议，去除因果暗示），且**不进入 `verdict`**。

---

## 三、`probe_crash_signature_observed` 与窄 fail 集

`probe_crash_signature_observed = observed-true` **当且仅当**下列任一成立（闭集，穷尽，无 else）：

1. 归一化 `fault_type_observed` ∈ {`CPPCRASH`, `JSRAWERROR`}；
2. `signal_observed` ∈ {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`}；
3. 归一化 `fault_type_observed` = `APPFREEZE` **且** `last_visible_site` ∈ 冻结短时步集（D2 锁定序列 2.1–2.7）**且** marker 序列无矛盾（席 B 补充的守卫：序列自相矛盾时该位点判定本身不可信，此支不得成立）。

上述三支之外一律 `observed-false`；证据不足以求值时 `unobservable(cause=...)`。

**第 3 支的法理**（须写进判据）：短时步集内的步骤是**即返 syscall**。平台 watchdog 在一次毫秒级 `fcntl` 上冻结并杀死进程，不构成「不如预期的平台行为」的合理解释——只能是探针自身在该步挂死。故此支是正向探针缺陷证据，非平台行为。**反之，非短时步集上的 `APPFREEZE`（含 D7 的 20 s 任务）是本 campaign 要发现的平台行为本身，永远不得 fail。**

### 3.1 `verdict = fail` 的完整闭集（r9 冻结，穷尽）

死亡侧只有一条：

- **F1** `probe_crash_signature_observed = observed-true`。

其余全部 fail 触发面属**完整性轴**，与死因无关，沿既有规则不变：

- **F2** `N1BDISC_PRE` 缺失；
- **F3** 冻结全序被破坏（marker 顺序矛盾，含 `_C` 在而 `_T` 缺）；
- **F4** 增量落盘缺口 / 字段缺项 / 字段取值落在冻结域外（criteria-gap）；
- **F5** 封签失败、freeze SHA-256 复算不符、chunk / ledger / digest 校验不符；
- **F6** `d1_cmdline` 非 `:vpn`；
- **F7** HDC 白名单违规；
- **F8** 解析器 / 真值表缺口——输入在 capture 内自相矛盾（如未知 revents 位、负单调耗时）。**这是记录器未尽职，不是死因**；
- **F9** 观测窗到点进程仍被正向确认存活且协议未完成。

**除 F1–F9 外，一切结局均不 fail。**

**整合前必须核实（吸取程序失误 #19 的教训）**：F2–F9 是我依三席意见归纳的「既有完整性触发面」，**尚未逐条核对它们在现文档中的实际存在形式与措辞**。集成者须对照 WP0 清点表逐条确认：
- 每条在现文档中**确实存在** → 沿用其原措辞，不得改写；
- 现文档中**不存在** → 这是**新增规则**，须单独登记为 r9 新引入的裁量并说明理由，**不得混在「沿既有规则不变」里蒙混过去**；
- 现文档中存在但**措辞不同** → 以现文档为准，本表仅作归类。

F1 是本轮新造，必属新增登记项。

### 3.2 冻结解释句（席 A 给出，逐字写进判据）

> 「未完成预注册采集」限 PRE 缺失、顺序破坏、增量落盘/字段域缺口、封签失败、以及窄崩溃签名；**PRE 封口之后的进程消失，不论能否归因，不构成 fail。**

此句用于封堵「没跑到 POST 就算未完成采集」的误读。PRE 一旦封口，其后死亡属采集被环境截断，**已得事实仍算落盘**。

---

## 四、`destroy_call_state` 五态模型（解决 BL-2 / BL-8）

marker 只能在调用**前后**发射，**不可能在调用瞬间发射**。`_T` 与 `_C` 只能**夹住**调用，这一区间歧义**不可消除**，只能显式建模。

### 4.1 调用状态

| 观测 | `destroy_call_state` |
|---|---|
| 协议显式发出 `N1BDISC_SKIP\|item=destroy` | `not-called` —— **唯一可证「未调用」的途径** |
| `_T` 缺、`_C` 缺 | 按此前协议位点定 `not-reached`；**不得仅凭 marker 缺失推断未调用** |
| `_T` 在、`_C` 缺 | `unobservable(cause=call-boundary-incomplete)` —— 既可能尚未调用，也可能**已进入调用但未返回**。**不得写 `never-called`** |
| `_T` 在、`_C` 在 | `call-returned` —— 只能证明**调用表达式已返回完成凭据**，**不能**证明平台效果已发生 |
| `_C` 在、`_T` 缺 | marker 顺序/完整性错误 → **fail（F3）**。这是完整性轴，不是死因轴（**r10 同步注：本规范落地时判据正文为该行补了分量值 `unobservable(cause=marker-contradiction)`**——verdict 仍由 F3 fail，但七分量的全函数义务要求分量本身有值；两轴独立，判据正文为准） |

**明确废除**：原「`_T` 在而 `_C` 缺 → `destroy-never-called`」判定。它会把「调用已进入平台层但进程在返回前 terminal」这一**成功终态**误判为未调用，进而经原行 6 烧掉 ID。

### 4.2 worker 返回时序

**因 marker 字段只记毫秒，等值不能证明先后**，故两侧均取严格不等号、闭区间整体归歧义：

| 观测 | 判定 |
|---|---|
| `at_mono_ms` **<** `T_mono_ms` | `definitely-pre-invocation` |
| `at_mono_ms` **>** `C_mono_ms` | `definitely-post-invocation` |
| `T_mono_ms` **≤** `at_mono_ms` **≤** `C_mono_ms` | `unobservable(cause=invocation-window-ambiguous)` |
| `_C` 缺 | **不得作 post-invocation 归因** |

**路径①只接受严格的 `at_mono_ms > C_mono_ms`。**

**必须删除**的冲突表述：路径①条件 3 中「两侧均有 marker 锚点：`N1BDISC_DW_RETURN` 与 `N1BDISC_DW_DESTROY_T`」——它与同句后半的「取 `_C`」互斥，不得让实现者自行选边（BL-8）。`destroy_call_mono_ms` 一律取 `_C` 的 `mono_ms`。

### 4.3 共享 destroy 子协议（解决 BL-7）

**凡实际调用 `destroy()` 的路径**（含按 skip 表跳过 D-W 的 `no-live-fd` / `dup-failed` 路径）**一律走同一个 destroy 子协议**，依次发 `_T` → 调用 → `_C`。

理由：取消归因表**不得用来掩盖该 marker 缺口**（席 A、席 B 一致强调）。缺 `_C` 时 `destroy_call_state` 会记成假的「未调用」，污染 `u4` 与 OB-04 的输入事实。**但缺 `_C` 本身不再驱动 fail** —— 它只是把 `destroy_call_state` 降为 `unobservable(cause=call-boundary-incomplete)`。

---

## 五、`marker_tail_state`（原 `site_uncertainty` 的替代）

原 `site_uncertainty` 的全部复杂度来自它要去压制死因表行 2。**行 2 删除后该耦合消失**，本分量退化为纯观察登记：

| 取值 | 条件 |
|---|---|
| `tail-complete` | `N1BDISC_POST` 在（终态 marker 已封口，无尾丢失可能） |
| `possible-tail-loss` | POST 缺 **且** 存在死亡证据 **且** 死亡证据墙钟 − `last_visible_site` 对应 marker 墙钟 > `T_tail = 25000 ms` |
| `unobservable(cause=...)` | 任一墙钟不可求值 |
| `tail-loss-not-indicated` | POST 缺、有死亡证据、但上述差值 ≤ `T_tail` |

**`T_tail = 25000 ms` 的取值理由（重写，不再挂靠行 2）**：25000 = D7 冻结时长 20000 + 宽限 5000，即**单个最长合法阶段的合法时长上界**。静默跨度超过任何单一阶段的合法时长，即提示尾部可能丢失。**本阈值不再与任何 fail 判定耦合**，取值偏差只影响一个观察标签的置位与否，不影响 `verdict`。

**随之删除**：`T_uncertainty` 与行 2 判定窗 `27000` 的全部比较论证、「置位则行 2 不命中」规则、`u7` 与行 2 的联动约束，以及 gate 10 ⑪ 中「`elapsed_proxy = 27001` 命中行 2」用例。`elapsed_proxy` 若在别处无用途亦一并删除（整合时须先清点其全部使用点）。

---

## 六、按席 C 登记义务：本次删除了哪些 fail 映射、各自失去什么

席 C 裁决明确要求：改动显著收窄 fail 面，须**逐条登记**删除了哪些 fail 映射、各自失去的探针缺陷捕获、以及为何该损失是决议 §4.2 `:116` 所要求的。

| 删除的 fail 映射 | 原捕获意图 | 失去的真实探针缺陷 | 为何必须删 |
|---|---|---|---|
| 行 2 `probe-fault`（D7 超窗） | 探针在 D7 挂死超时间盒 | **长盒内挂死**且只有 `APPFREEZE` 或无任何条目 | 「D7 真超窗」与「D7 跑完但尾 marker 全丢」观测向量完全相同（三席一致）。且非短时步上的 `APPFREEZE` 本就是本 campaign 要发现的平台行为 |
| 行 5 `fault-type-unrecognized-no-platform-signature` | 探针崩溃但词根不在候选集 | **JS 崩溃写成候选集外词根**（如 `JSCRASH`）且无致命信号 | 本 tuple 从未实测过真实 faultlogger 字面（S1 已登记），未知词根同样可能是平台异词根终止。原生 `SIGSEGV` 仍由 F1 第 2 支捕获 |
| 行 6 `unattributed` | 归不出类即保守 fail | **无 faultlogger、无致命信号的静默崩溃** | 举证责任倒置：「无法排除探针缺陷」不等于「有探针缺陷」。归不出类在观测上等价于「可能是平台行为」，映射 fail 直接违反 `:116` |
| 行 7 else 兜底 fail | 穷尽性兜底 | 同上 | 同上 |
| 行 4(ii) 的收紧提案（**未采纳，登记为「明确不修」**） | 堵住 `_C` 后的探针崩溃 | `_C` 之后无 crash 签名的静默崩溃（席 B #6 构造） | 收紧会把 E3 预期的 destroy terminal 成功终态再次烧成 fail（席 A 明确禁止） |

### 6.1 残余的诚实陈述（席 B 原话口径）

新方案接受一种残余 fail-open：**没有正向崩溃签名的真实探针故障，可能以 `death_cause` 不可观测收口并使 pre-only `pass`。**

以席 B #6 构造为例，新方案下记录为 `process_death_observed=observed-true`、`last_visible_site=D6S1_B`、`destroy_call_state=call-returned`、`probe_crash_signature_observed=unobservable`。**不再伪称 `platform-termination`，因果语义上不再洗白；但 `verdict` 层面仍存在 false-negative 残余。**

### 6.2 接受该残余的理由（**禁止引用 DryRun**）

**禁止事项**：不得以「Live 之前还有一次 DryRun」为该残余背书。门 11 DryRun 是 `is_evidence=false` + **HDC0**（host-only 假 HDC），只能验证 runner / parser / 状态机 / 合成 trace / freeze 哈希，**不能**执行设备侧 native/NAPI、watchdog、destroy 竞速、线程调度或 faultlogger 缺失。席 A、席 B 独立否定该前提；主会话在裁决提问中曾误用之，已登记为程序失误 #19。静态审查与故障注入 selftest **只能降风险、不能证明消除**（席 B）。

**正确理由三条**：

1. **`verdict: pass` 的效力已被硬约束封死**：发现型 campaign 的 `pass` 不得在任何后续文档中被引用为平台行为结论或任何门的功能结论（决议 §4.2、`evidence-schema.md:90`）。一次带残余的 `pass` 不会把错误结论传播下去。
2. **N1b 正式门仍须在同一元组上复测**，DISC 事实只作为 N1b r2 判据的**预注册设计输入**（决议 §4.4）。
3. **发现型 campaign 的损失函数是「烧掉不可复用 ID」，不是「漏掉一次可能的探针挂死」**（席 A）。模糊死亡一律 fail 会把合法平台终止烧成 fail、直接违反 §4.2；模糊死亡不 fail 会漏掉无正向签名的探针故障。在 discovery 门上应选后者，并把残余逐字登记——即本节。

---

## 七、整合施工要点（给 r9 集成者）

1. **先按 WP0 清点表逐条改**，不得凭印象搜索。清点表中每一条「会导致 fail 的死因相关位点」都必须有对应处置（删除 / 改为观察 / 保留并说明理由），**不得静默跳过**。
2. 删除死因表后，须检查**所有指向该表的交叉引用**（「见死因分类节」「按死因分类收口」「沿死因表行 N」等）并改写。
3. `elapsed_proxy` 的全部使用点须清点：若仅服务于已删除的行 2，一并删除；若另有用途，保留并说明。
4. gate 10 圈码用例须与本规范逐条同步：删除断言「某死因 → fail」的用例，新增断言 F1–F9 闭集的用例，新增 `destroy_call_state` 五态与时序三分带的用例。
5. 静态断言集须加 `T_tail` 与 F1–F9 闭集的一致性检查。
6. 全文单行不得超过 400 字符。
7. 状态行改 `criteria-r9-pending-independent-review`，并在修订历史登记 r9 全部条目。

## 八、硬边界

判据未冻结；未分配 AUTH/pair 与 evidence ID；未请求也未执行任何物理 campaign。本规范不构成冻结授权。
