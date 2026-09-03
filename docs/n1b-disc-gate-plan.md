# N1BDISC 发现 campaign 计划与判据预注册（N1b r2 设计输入 × 物理 VpnExtension 平台事实采集）

最后核验：2026-09-02 ｜ 状态：`criteria-r17-pending-independent-review`

> **修订登记（r0 → r1，正文整体取代）**：r0 已经三席跨厂商隔离独立审查——**两席 fail（分别 6 blocker 与 10 blocker）、一席 pass**；pass 席的引用抽查漏检 MR1 溢出主张、其自陈最不踏实条目恰为 post-mortem 死因分类，按 **2 fail** 处理，修订强度不因一席 pass 降低。判据**未冻结**。
> r1 依据 = 三席去重合并的 BL-1..BL-10、MJ-1..MJ-13 与 minor 清单，主会话对目标 SDK d.ts 的逐字实测（`RouteInfo`/`LinkAddress`/`NetAddress`/`VpnConfig` 真实形态，见「SDK 依据」节；
> r0 自陈「SDK d.ts 在仓外不可核实」**有误**，实测路径 `/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/ets/api/@ohos.net.connection.d.ts` 与 `@ohos.net.vpnExtension.d.ts`），**以及主会话开工后补充的 D-W in-wait 证据要求（优先级等同 blocker，与 BL-4 合并处理；见 D-W 节「in-wait 证据」与「决议约束正面登记」）**。
> r1 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **2026-09-01 第一趟事实常量更正（F1-F11）**：仅更正事实性/机械性错误（PI 前缀常量、D8b 校验和主张、MTU oracle 接口表述、poll 族冻结集 `{73}`、观测窗 467/525 数字统一、HilogStream 墙钟 ≥825 s 与求值窗 525 s 分立、complete 全序校验范围 P1-P12、ledger_digest 完整 SHA-256、决议引用 `:66`、白名单计数表述、
> `D4_READ off=both`）；状态保持 `criteria-r1-pending-independent-review`，**设计级修订待下一趟，届时统一改状态并登记**。
>
> **第二趟修订（D-W waiter 观察项集群 A1-A6）**：仅修 D-W 集群——A1 路径①采认门槛补全为六条件合取（承载决议 §三.5-①「可与自身 timeout 区分」）；A2 `dw_return_class` 冻结为有优先级的穷尽判定表（新增 `late-fd-event`/`late-data`/`interrupted`/`poll-error`/`other-revents`，`DW_RETURN` 增 `ret`/`errno` 字段）；
> A3 `/proc` stat 的 state 解析规则冻结（最后一个 `)` 后取 token）；A4 barrier 等待盒 2 s → 7 s（≥ drain 盒 + 裕量）、冻结「destroy 不得早于 `DW_BARRIER` marker」硬规则、观测窗上界 467 → 472 / 裕量 58 → 53（冻结值 525 s 不变）；A5 join 有界性裁定选 (b)（musl sysroot 实测无 timed/try join，join 不可有界本身登记为观察事实，`dw_join_result` 增 `join-blocked-observed`）；A6 自陈 (h) 与正文判定规则对齐。清单外问题不动。
> **编号区分注（防审查席误引）：本段「A1-A6」是 D-W 修订集群的内部编号，与「静态断言」表的 A1-A8 是两套独立编号（如本段 A3 = stat 解析规则，静态断言表 A3 = BoringTun 符号零调用），引用时须注明所属表**。
> 状态保持 `criteria-r1-pending-independent-review`。
>
> **第三趟修订（verdict 与终态判定集群 B1-B7）**：仅修 verdict/终态判定集群——B1 死因分类重写（新增 `platform-termination` 的 (ii) 分支作为预期成功终态〔防误引注：本行历史表述「新增第 4 类」不准确——死因分类表自始只有 3 类，`platform-termination` 是其中一类，r3 第一趟 D1 反转全序后其预期成功终态落为该类的 (ii) 分支，见 verdict 节分类表〕、
> 全序优先级、死亡位点约束、faultlogger 时间窗关联、死因在 `ForceStop` 前冻结、HDC 白名单增 `PidOfVpn`、删除无判定程序的「退出记录」签名）；
> B2 criteria-gap 收紧（仅预注册原因可记 `unobservable`，取值域外/解析缺口/字段不可求值一律 `fail` + raw 保留）；B3 操作员 Allow 超时改裁定 (a)（readiness 前置，不再扩权为 blocked）；B4 `u4_*` 摘要「最弱值」全序冻结（`unobservable` < `observed-false` < `observed-true`）；B5 全序表与 skip 表 P10/P11 编号统一（无 worker 跳过终态轮询、禁止跨表指称、时间盒表 P10 行区分盒到期与 A5(b) 阻塞两情形）；
> B6 `dw_destroy_distinguishable_from_timeout` 枚举逐字闭合为 11 类（历史登记照登不改写；**防误引注——后经 r3 第一趟 D4 给派生输入 `dw_return_class` 新增 `destroy-never-called`，本字段枚举随之闭合为 10 类 + 2 结局，现值以 D-W 节逐字列举为准**）；B7 新增静态断言 A7（stat 解析按最后一个 `)` 后取 token，禁按空格切分）。清单外问题不动。
> 状态保持 `criteria-r1-pending-independent-review`。
>
> **r2 修订总登记与审查依据**：r1 跨厂商隔离独立审查结果 = **三席全部 fail（blocker 分别 5 / 11 / 2 个）**，r2 修订以上述三席去重清单与主会话补充核实为依据，共四趟——**第一趟 F1-F11**（事实常量更正：PI 前缀常量、D8b 校验和、MTU oracle 表述、poll 族冻结集 `{73}`、观测窗数字、HilogStream/求值窗分立、complete 全序 P1-P12、ledger_digest 完整 SHA-256、决议引用、白名单计数、`D4_READ off=both`）；
> **第二趟 A1-A6**（D-W waiter 观察项集群：路径①六条件合取、`dw_return_class` 穷尽判定表、stat 解析规则、barrier 盒 7 s 与 destroy 位次硬规则、有界 join 裁定 (b)、自陈对齐）；
> **第三趟 B1-B7**（verdict 与终态判定集群：死因分类重写含 `platform-termination`、criteria-gap 收紧、Allow 超时裁定 (a)、三态最弱值全序、P10/P11 编号统一、destroy 区分枚举闭合、静态断言 A7）；
> **第四趟 C1-C9**（冻结项与生命周期集群：C1 迟到 fd 的 destroy 例外删除、P9 前禁任何 destroy；C2 D7 读钟频率上界由 50 ms `clock_nanosleep` 保证、`elapsed_ms ≥ 20000` 方可判 `observed-true`；
> C3 14 个 ffi 符号名逐字冻结（`boringtun-0.7.1/src/ffi/mod.rs`）；C4 chunk 编码冻结为 base64；C5 HDC 白名单唯一操作名集 + 完整 argv 冻结；C6 drain 五类返回转移规则闭合；C7 U1/U2 短读短写收口 `unobservable(short-or-zero-io)`；C8 `PidOfVpn` 可行性先例引用与回退条件登记；C9 in-wait 竞速明文登记、S4 措辞对齐、A 编号区分注、U1 括注去 PI 旧语境、SDK 摘录补 `ConnectionProperties.mtu`）。
> 状态改为 `criteria-r2-pending-independent-review`；r2 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r3 第一趟修订（致命项与跨趟接缝 D1-D10）**：r2 三席独立审查两席 fail（2 与 11 个 blocker）、一席 pass，本趟只修致命项与跨趟接缝——D1 死因分类全序反转为 `probe-fault` > `platform-termination` > `unattributed`（成功终态不再被兜底截胡）并冻结 `PidOfVpn` 不可观测的唯一出口；
> D2 `APPFREEZE` 位点约束反转默认（短时步集冻结为 P1 与 D2 锁定序列，其余位点走 platform-termination 不得 fail）+ 新增静态断言 A8；D3 路径① 条件 4 钉死 `dw_drain_end == eagain`；D4 `dw_return_class` 新增 `destroy-never-called`（12 类闭合）；D5 静态断言 A4 显式豁免第二趟修订集群 A5(b) 的 `pthread_join`；
> D6 host finally 插入 `PidOfVpn` 采样步；D7 删除 `criteria-gap` 运行时兜底 cause；D8 决议引用改回 `:65`；D9 自陈 6(g) 更正（已被 C5 取代）；D10 观测窗推导 P8 并发项更正（上界 467、裕量 58，冻结值 525 s 不变）。状态保持 `criteria-r2-pending-independent-review`（r3 收尾趟再改），清单外问题一律不动。
>
> **r3 第二趟修订（取值域闭合与可实现性 E1-E11）**：仅修 r2 审查某席位 blocker/major 经主会话判断成立的 11 项——E1 `dw_return_class` 判定表前新增 poll 合法域门（矛盾输入 fail + raw 入档，不伪装平台唤醒）；E2 fd ledger canonical 序列化规则 + `N1BDISC_FD` 逐次 transition marker 冻结（post-mortem digest 由 runner 依 marker 重算，与探针发者一致否则 fail；A5 冻结集增 `FD`）；
> E3 chunk 字面增 `stream=<literal>|item=<id>` 逻辑键（stream 枚举 {`dlerror`,`rejtext`,`u3hex`,`foreign`}，重组按 (stream,item) 分组，`D2_REJTEXT` 字面冻结）；
> E4 U3 枚举独立优先级 + off=both 决定性 offset=offset-4（另设 off0 观察字段）+ selftest 多类同命中用例；E5 D7 elapsed 互斥区间 `[0,20000)` / `[20000,25000]` / `>25000`（第三段具名三态观察不判 fail），正文与自陈 6(c) 对齐；
> E6 operator-ready 确认步前置到 StartEntry 之前（记录时刻与动作），其后的 Allow 超时按已消费 campaign 收口（fail + evidence 记录，不再声称未消费）；E7 fault 条目解析契约冻结（字段名/取值域/时间字段格式）+ `PidOfVpn` positive 基线前置（无基线的空输出记 `unobservable` 不得反推死亡）；
> E8 HilogStream 改按终态停止（RESULT 出现或 finally 步骤 1 即停——**防误引注：本行「RESULT」为 r3 当时的历史字面，r4 第一趟 R1 已拆为 `N1BDISC_PRE`/`N1BDISC_POST`，现行停止触发字面见 HDC 白名单表 `HilogStream` 行**），825 s 降为兜底上界；
> E9 `u1_no_route_control` 完整求值表冻结；E10 U2 接收判定收紧为完整 16 B 身份逐字节匹配且 round/seq 与本轮一致；E11 `dw_watchdog_killed` 三态赋值规则逐字冻结（不再「同 D7」）。状态保持 `criteria-r2-pending-independent-review`（下一趟收尾时改），清单外问题一律不动。
>
> **r3 修订总登记与审查依据**：r2 跨厂商隔离独立审查结果 = **两席 fail（blocker 分别 2 与 11 个）、一席 pass（0 blocker）**，r3 修订以上述三席去重清单与主会话补充核实为依据，共三趟——**第一趟 D1-D10**（致命项与跨趟接缝：D1 死因全序反转 + `PidOfVpn` 不可观测唯一出口；
> D2 `APPFREEZE` 位点默认反转 + 静态断言 A8；D3 路径①条件 4 钉死 `dw_drain_end == eagain`；D4 `dw_return_class` 新增 `destroy-never-called`（12 类闭合）；
> D5 静态断言 A4 显式豁免第二趟修订集群 A5(b) 的 `pthread_join`；D6 host finally 插入 `PidOfVpn` 采样步；D7 删除 `criteria-gap` 运行时兜底 cause；D8 决议引用改回 `:65`；D9 自陈 6(g) 更正；D10 观测窗 P8 并发项更正）；
> **第二趟 E1-E11**（取值域闭合与可实现性：E1 poll 合法域门；E2 fd ledger canonical 序列化 + `N1BDISC_FD` transition marker；E3 chunk `(stream,item)` 逻辑键；E4 U3 枚举优先级 + off=both 决定性 offset；E5 D7 elapsed 互斥区间；E6 operator-ready 前置 + 已消费收口；E7 fault 解析契约 + `PidOfVpn` positive 基线；E8 HilogStream 按终态停止；E9 `u1_no_route_control` 求值表；E10 U2 逐字节身份匹配；E11 `dw_watchdog_killed` 独立赋值规则）；
> **第三趟收尾（G1-G12）**（跨节一致性清理与定稿，无语义变更：G1 `clock_nanosleep` 时长两档表述、G2/G4 历史登记行防误引注、G3 修订集群 A5(b) 引用限定、G5 drain 五类返回 ↔ 四值 `end=` 换算、G6 G0 finally 对照括注同步、G7 (i)/E5 管辖分界说明、G8 自陈 7(g) 闭合至 D1 唯一出口条款、G9 gate 10 selftest 集中列举、G10 fail 条款 chunk 术语对齐、G11 本状态行与本登记、G12 自陈节补全）。
> 状态改为 `criteria-r3-pending-independent-review`；r3 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r4 第一趟修订（结果通道结构性拆分与死因分类总函数化 R1-R7）**：仅修结果通道与死因判定集群——R1 单一 `N1BDISC_RESULT` 拆为 `N1BDISC_PRE`（P8T，destroy 与 join 之前，承载 P1-P8 全部已得事实 + ledger 当前 digest）与 `N1BDISC_POST`（P12，D6 逐子项 + D-W 结局 + ledger 最终 digest）双终态 marker，新增 `protocol=pre-only` 合法终态（PRE 在 POST 缺：`u4_*` 逐项 `observed-false`，可 pass；PRE 缺 → fail）；
> R2 死因分类改为全定义域总函数（`unattributed` 删「无新增条目」过滤器成真残余、显式 else 兜底归 `probe-fault`、D7 超窗独立成行、fail 触发器改「分类 ≠ `platform-termination`」）；
> R3 `platform-termination` 唯一布尔式 `((i) || (ii)) && (iii)`；
> R4 `D7_BEGIN` 增 `start_mono_ms` + 死亡时刻 elapsed 墙钟代理预注册（容差 2000 ms，仅用于分类）；R5 短时步集移出 P1；
> R6 join 洗白封锁行 + 更正虚假「路径由此关闭」声明 + join 置于 PRE 之后；R7 未枚举平台值改 `unobservable(cause=value-outside-frozen-domain)` 不 fail，解析器/真值表缺口仍 fail。
>
> **r4 第二趟修订（取值域闭合与未实测依赖登记 S1-S8）**：仅修取值域闭合与自相矛盾项——S1 `Fault_Type` 字面登记为未实测依赖（候选集+归一化规则、兜底出口 `fault-type-literal-unrecognized` 优先于死因行 5/行 6、门 3 收敛前置）；S2 `revents` 冻结为十进制 bitmask（sysroot poll.h 数值、未知位归 `other-revents` 不 fail、selftest 组合域界定为 64 子集）；
> S3 canonical ledger 重建闭合（marker 增 `by=`/`inst=` 字段、closed_by 取值域与推导、`(role,inst)` 配对状态机）；
> S4 A5 冻结集删除 `D2_REJTEXT`；S5 U3 判定改互斥分区表（`ambiguous` 不再被截胡，gate 10 ⑤ 同步）；S6 chunk 重复 index 统一为「首到者优先 + 逐字节一致校验，不一致才 fail」；
> S7 `dw_watchdog_killed` 删除 skip 判 `observed-false` 分支；S8 该字段死亡证据收紧为「faultlogger 签名 或（`PidOfVpn` absent 且 `DW_DESTROY_T` 缺失）」，与 destroy 归因解混。状态维持 `criteria-r3-pending-independent-review`。
>
> **r4 第三趟修订（major 级清理 T1-T8）**：仅修 major 级清理集群——T1 gate 10 selftest 清单补全（⑩ 更新行号与 T6/T7 同步；⑪ 立项承载 E11 `dw_watchdog_killed` 三态映射真值表与 D7 死亡路径墙钟代理边界用例；登记清单↔正文对应关系，本行为索引、以各节正文为准）；T2 fault 时间窗关联冻结为**快照文件集合差分唯一判据**（时间字段仅观察登记、不参与窗界判定——理由：冻结时间形态无时区标记、设备时区来源不在白名单、墙钟偏差容限不可机器核实；行 2 墙钟代理例外说明）；T3 新增 `u3_prefix_format` 与 `u6_nonblocking_initial` 三态主字段及逐格确定派生表（detail 枚举/极性值保留不变）；
> T4 死因表行 1 删除「JSRAWERROR 类」的「类」字，字面与 S1 冻结候选集同一闭集；T5 `d1_cmdline` 与预注册名不符改登记 `process_model_mismatch` 观察事实（逐字登记实际值），仅 `process_model != vpnextension`（不含 `:vpn` 后缀）才构成完整性 fail，与 `PidOfVpn` 出口对齐；T6 行 4 布尔式扩为 `((i) || (ii) || (iii)) && (iv)`，新增 (iii) SIGKILL/SIGTERM 平台发起终止分支（原「无 crash 签名」顺移为 (iv)，全文同步；信号划分理由入正文）；
> T7 S1 兜底出口表内化为行 5 `fault-type-unrecognized`（不 fail），旧行 5/行 6 顺移为行 6/行 7，行 5 残余定义域收窄，全文引用同步；T8 `dw_poll_revents` 落盘格式与 S2 十进制 bitmask 编码逐字同步。状态维持 `criteria-r3-pending-independent-review`。
>
> **r4 第四趟修订（长行重构与定稿 W1-W2）**：仅作可审性重构与定稿登记，无语义变更——W1 长行重构（`length>400` 长行降至 ≤10：表格长单元格内容外提为表后缩进子项、密集 bullet 按语义组拆分，全部内容可逐字溯源、不删任何字面、不改任何判定规则）；
> W2 本状态行与本登记。r4 四趟汇总——**第一趟 R1-R7**（结果通道 PRE/POST 结构性拆分、死因分类总函数化、布尔式唯一化、D7 elapsed 墙钟代理、短时步集收窄、join 洗白封堵、域外平台值拆分）；**第二趟 S1-S8**（Fault_Type 归一化与兜底、revents 编码、ledger 重建、REJTEXT 矛盾、U3 分区表、chunk 重复策略、watchdog_killed 三态与归因收紧）；**第三趟 T1-T8**（selftest 补全、fault 时间关联、三态主字段、闭集对齐、cmdline 判定、SIGKILL 分支、兜底表格化、revents 格式同步）。状态改为 `criteria-r4-pending-independent-review`。
>
> **r5 修订（边界收口 U1-U12）**：r4 已经三席跨厂商独立审查——**三席全部 fail（blocker 分别 5 / 7 / 3 个）**；三席确认 r4 的 PRE/POST 结构方向正确（else-pass 已关、destroy 路径可达、join 洗白已关），否定集中于四处边界（PRE 位置太晚、fail 触发器作用域过宽、行 4 布尔 (iv) 与 (i) 口径冲突、digest 切点未冻结）。r5 修订以上述三席去重清单为依据，共 12 项：
> **U1** PRE 发射点自 P8T 前移至新增位点 P5T（P5 D8a 完成后、P6 D7 之前；承载范围收窄为 P1–P5 事实 + P5T 快照 digest；全序表/时间盒表/观测窗推导/A5/skip 主线同步）；
> **U2** 行 4 (iv) crash 签名收窄（APPFREEZE 仅短时步集内算 crash 签名，(i) 复活；(iii)/(iv) Signal 集互斥写明；gate 10 ⑩ 补用例）；
> **U3** fail 触发器收窄为仅 `protocol=pre-only` 时求值（`platform-termination`/`fault-type-unrecognized` 不 fail；complete 不进死因分类；现行触发器位置约 :718-721（r6 W8 更正：r5 登记时的 :682 因后续编辑行号漂移）与 host finally 步骤 4/9 同源对齐）；
> **U4** skip 两主线补 P5T PRE 位点；
> **U5** PRE digest 切点冻结（P5T 快照 + `open-at-pre`；最终 digest 按收口形态；一致性限同切点、禁跨时点比较）；
> **U6** pre-only 的 `u4_*` 保留死亡前已得 result（不再整体抹 false）；
> **U7** `protocol=post-mortem` 残留更正为 `pre-only`、自陈 7(g)/8(i) `RESULT` 残字清零；
> **U8** gate 10 ⑪ 代理阈值与正文行 2 同源（27000/27001）；
> **U9** `by=` 取值域按 `action` 拆分（create/not-created 恒 `none`，域校验仅施于 close）；
> **U10** Signal 解析域三段（crash 段/平台终止段/其余）；
> **U11** 自陈补第 10 项（S/T/W 三趟裁量逐项登记）并更正自陈 1/8(g) 残留；
> **U12** join 措辞统一（A5(b) 单一表述、`join-blocked-observed` 由 runner 登记）、`revents=<set>` 残留改 S2 十进制单值、chunk 重复片 `count`/`sha256` 字面不同判 fail、`closed_by=process-exit`（无产生条件）删除。状态改为 `criteria-r5-pending-independent-review`；r5 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r6 修订（边界与裁定落地，两趟）**：r5 已经三席跨厂商独立审查——**deepseek pass（0 B / 0 M / 4 m）、sol fail（5 B / 1 M / 2 m）、grok fail（2 B / 2 M / 3 m）**；三席确认 r5 四条主修法全部有效（PRE 前移主线走通、(iv) 收窄 (i) 复活、触发器作用域、u4 保留已得结果），剩余缺陷收敛于「前移后未同步的边」。r6 两趟：
> **第一趟 W1-W8（机械修正）**：W1 skip 表 dup-failed 列位点倒序更正（P5T 先于 D7）；W2 `d2_late_fd` 切点规则修正（P5T 快照一律 `open-at-pre`，删除「恒按最终形态」的时间悖论表述）；W3 恢复 `process-exit`（pre-only 收口的未关闭 fd 由内核在进程退出时回收，非 ForceStop 所关；`host-forcestop` 仅用于存活 fail-cleanup；域为七值）；W4 行 3 加「且未发出 `DW_DESTROY_T`」条件（destroy 已调用后死于 P10 的预期终态不再被截胡）；W5-W8 PRE 缺失解释、阈值分解式、HilogStream 停止条件同源、登记头行号更正。
> **第二趟 V2/V4/V6（主会话裁定落地）**：V2 u4 极性分岔（destroy 已调用 → 未执行子项 observed-false；destroy 从未调用 → unobservable(destroy-never-called)，不赋假负值——U1 前移后 pre-only 含 destroy 未调用路径，旧规则赋假负值毒化 OB-04 判定输入）；V4 位点判定不确定性显式化（`site_uncertainty=possible-tail-marker-loss` 观察项 + u7 约束：位点 P6 且 site_uncertainty 在 → u7 不得赋 observed-false）；V6 死因分类行 5 拆为 5a/5b（有独立平台签名 → 不 fail；无 → 保守 fail——封堵 sol 构造的探针崩溃洗白路径，同时保留平台异词根兜底）。
> 状态改为 `criteria-r6-pending-independent-review`；r6 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r7 修订（同键收口）**：r6 已经两席跨厂商独立审查——**sol fail（5 B / 1 M / 1 m）、grok fail（3 B / 3 M / 2 m）**（第三席两次派出均未归，记 attempt-not-counted，不计入修订依据）。r7 修订以上述两席去重清单为依据，共 10 项：
> - **X1** 行 5b 改独立类名 `fault-type-unrecognized-no-platform-signature` 并全面接入 fail 链（fail 触发器、finally 步骤 4/9、D-W 终态路径、与结果通道的关系、S1 兜底段、排序理由、gate 10 ⑥⑩ 同键；行 4 统一承载「有平台信号」，不可达的行 5a 删除，表回到 7 行）；
> **X2** gate 10 ⑩ 行 3 用例与行 3 逐字同一（补「未发出 `DW_DESTROY_C`」条件与反例）；**X3** gate 10 ⑨ 按 V2 分岔拆用例（含 destroy-never-called 支）；
> **X4** 新增 post-invocation marker `N1BDISC_DW_DESTROY_C`（destroy 调用发起后发射），V2 分岔、行 3/行 4(ii)、selftest ⑨⑩ 锚点升级，补「已调用未 resolve → `unobservable(destroy-unresolved)`」第三支，A5 冻结集加该字面；
> **X5** V4 `site_uncertainty` 置位判据比较对象改为「死亡证据墙钟 − 最后可见 marker 墙钟 > T_uncertainty」（r8 Y2 冻结减法方向——r7 版把减法写成 marker 侧在前（`最后可见 marker 墙钟` 为被减数），为落笔执行错误，死亡晚于 marker 时恒为负、永不置位；r8 Y3 阈值改挂行 2 之下：`T_uncertainty(P6) = 25000 ms` = 行 2 elapsed_proxy 阈值 20000+5000，废弃 ×1.5 = 37.5 s 旧值），u7 与死因行 2 置位时均不得作确定判定；
> **X6** ledger selftest pre-only 收口用例改 `process-exit`、另设存活 fail-cleanup → `host-forcestop` 用例；**X7** S4 与 criteria-gap 两分法同步（有效平台新值不 fail）；
> **X8** 自陈 10 三条过期陈述加防误引注；**X9** m 级清理（`by` 七值域、触发器句法、⑨ u4 分岔用例）；**X10** 本状态行与登记。
> 状态改为 `criteria-r7-pending-independent-review`；r7 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r8 修订（小修，5 项）**：r7 已经两席跨厂商独立审查（第三席 attempt-not-counted）——**sol fail（2 B / 1 M / 1 m）、grok fail（2 B / 3 M / 3 m）**；两席独立收敛到同两条 r7 落笔执行错误（Y1 主序列 `_C` 排序、Y2 X5 减法方向写反），r6 轮全部 blocker 已确认修好。r8 修订范围：
> - **Y1** 主序列改四步（`_T` → 发起 destroy（取得 Promise 不等待）→ `_C` → 有界等待 resolve），`_C` 归位调用之后 + gate 10 ⑨ 补 `_T` 在 `_C` 缺反例；
> **Y2** X5 判据全文唯一式冻结为「死亡证据墙钟 − 最后可见 marker 墙钟 > T_uncertainty」（四处：上条 X5 登记、死亡三态判定的「r6 V4 补充」、死亡位点判定第 4 条、自陈 12(c)；**此处刻意不写行号——行号随每轮修订漂移，已发生过引用失准**）；
> **Y3** `T_uncertainty(P6) = 25000 ms` 改挂行 2 之下（废弃 ×1.5 = 37.5 s），自陈 11(b)/12(c) 旧阈值加防误引注；
> **Y4** gate 10 ⑩/⑪ 补 X5 五用例（30000 置位且行 2 让位、25000/25001 边界对、24999 不置位、减法方向反例），并写明「静默跨度 == `elapsed_proxy`」只在 P8 构造（最后可见 marker = `D7_BEGIN`）下成立、两阈值方可比；
> **Y5** 本状态行与登记、自陈 13 r8 裁量、路径①条件 3 `destroy_call_mono_ms` 定义式补写（取 `_C` 的 `mono_ms`）。
> 状态改为 `criteria-r8-pending-independent-review`；r8 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r9 修订（结构性改造：归因停机；T0 裁决 3/3 落地）**：r8 经三席跨厂商隔离独立审查**全部 fail**（grok 2B/2M/2m、sol 6B/3M/0m、deepseek-v4-pro 仅审 r8 增量 2B/1M——该席连续四轮空输出后以收窄范围恢复，见 `docs/n1b-disc-r8-review-register.md`）；去重 **9 blocker / 7 major / 4 minor**。
> 其中 BL-1（死因表行 2 为空集且与 gate 10 ⑪ 自相矛盾，由 r8 的 Y3 引入）经席 A、席 B 独立收敛，与 BL-6、BL-7 共享同一结构墙：**capture 里的「最后可见 marker + 进程消失 ± APPFREEZE」对互斥死因不是单射**。据此升级为 T0 裁决问题，三席（含两席推翻各自审查阶段立场）3/3 一致：**停止用死因归因驱动 verdict，改为证据向量三态记录 + 窄正向崩溃签名 fail**；行 5/6/7 的 fail 映射一并废除；
> 不需要新 T0 决议（死因→verdict 是判据超出决议 §4.2 自造的一层，本改造是判据向决议收敛）。设计规范见 `docs/n1b-disc-r9-death-facts-spec.md`。
> r9 修订内容：
> - **R1 归因停机**：删除死因分类 7 行表及全部配套条款（优先级全序、fail-closed 兜底、条件外提块、行 3 注/行 5 收敛说明、墙钟代理与 D7 超窗管辖分界）；写入七分量证据向量（互不推导、无优先级、无 else、不合成死因）与 `probe_crash_signature_observed` 三支闭集；冻结席 A 解释句（PRE 封口之后的进程消失，不论能否归因，不构成 fail）；
> 自陈 14 逐条登记删除的 fail 映射与失去的捕获（含 DryRun 禁止事项：不得以「Live 前还有一次 DryRun」为残余 fail-open 背书，门 11 是 is_evidence=false + HDC0 host-only）。
> - **R2 fail 闭集**：F1（`probe_crash_signature_observed = observed-true` **且** `protocol != complete`）+ F2–F6、F8、F9（既有完整性触发面的归类索引，经逐条核实；**F7 HDC 白名单违规现归 invalid 轴、不在闭集内**）；「除闭集外一切结局均不 fail」。
> - **R3 destroy 调用边界**：marker 只能夹住调用、不可能在调用瞬间发射，区间歧义不可消除、只能显式建模——`destroy_call_state` 五态表 + worker 返回时序三分带（毫秒粒度等值不能证先后，闭区间 [T, C] 整体归 `invocation-window-ambiguous`）；路径①只接受严格 `at_mono_ms > C_mono_ms`；共享 destroy 子协议（凡实际调用 destroy 的路径含 dup-failed 一律发 `_T` → 调用 → `_C`）；判定表类 0 拆 `destroy-skip-proven`/`destroy-call-unobserved`（13 类）；V2 三支分岔改四支。
> - **R4 域门与 skip 编码**：未知 revents 位升格为前置门（原「归优先级 11」在表内程序下不可达、与行 7 按位与读法两处矛盾归类）；单调钟 13 字段统一冻结非负性与顺序约束（挂 F8；`_T`/`_C` 逆序沿 F3）；POST 每子项冻结 `result | skipped(cause) | unobservable(cause)` 三选一编码，`dw_return_class` 增至 15 值、`dw_join_result` 增至 7 值——**全部 create 被拒这一合法平台负面结果自此可记为 pass**（此前两条路都 fail）；
> `dw_watchdog_killed` 的 S8 收紧（`PidOfVpn absent + _C 缺失` 不再单独构成 waiter 被杀的正向证据）；barrier-never-observed 路径补发 `N1BDISC_SKIP|item=destroy`（`not-called` 在全文只有唯一证明途径）。
> - **R5 机械与一致性**：新增静态断言 A9（源码层核对 destroy 四步执行序 + 恰一个调用点——此前 A1–A8 只查 marker 字面集，r7 那条 blocker 的修复没有任何机器保证）；A5 比对口径收窄为「活规则中使用的字面」+ 豁免集（旧口径按字面实现必 fail：全文 57 字面 vs 冻结集 55）；
> ⑨ 补 6 类 ledger 状态机用例；`unobservable` 拼法统一为 `cause=` 形式并冻结「裸形式不是合法字面」；cause 正名 `site-uncertainty-tail-loss` → `marker-tail-loss`；selftest ⑥⑩⑪ 整体重写（⑩ 改为七分量 + F1 三支闭集真值表，含「非短时步 APPFREEZE 不 fail」「无 faultlogger 条目死亡不 fail（原 r4 版 fail、r9 起不 fail）」「complete 命中崩溃签名不进 F1」等钉子用例）；
> 撤销席 B 的 M-01（P8 预算——核实不成立，那 8 s 属 P10 的 worker 终态轮询盒，席 B 把它记到了 P8 名下）。
> **主会话九项裁量（均与某审查席建议相左或超出裁决直接授权，请审查席逐项挑战）**：① 删表不留观察版（席 A 建议留作观察分类；采用席 B 的证据向量替换——席 A 反对的正是「互斥因果类」这个数据结构本身）；② F1 作用域恢复 `protocol != complete`（r9 首稿误删 r5 U3 的 pre-only 作用域；恢复非扩大——T0 未授权扩大 fail 面）；③ `unattributed` 直接删、不做改名保留（席 B 建议改名 `death-cause-indeterminate`；但七分量中不存在「死亡原因」字段，该 cause 无处落脚）；
> ④ 两道前置门「合法域门先、未知位门后」（反序会把带未知位的矛盾输入洗成不 fail）；⑤ 负单调钟挂 F8 不挂 F4（F4 的「域外有效平台值不 fail」豁免以值本身有效为前提）；⑥ POST 三选一编码、不设 `skip_summary` 对应物（避免改冻结字段集、避免重蹈 MJ-5 明禁的「摘要替代逐子项」）；⑦ S8 收紧（宁缺勿误）；
> ⑧ barrier 路径补发 SKIP（`not-called` 唯一证明途径，不留旁路）；⑨ 拼法统一取 `cause=` 形式（合法域定义句式用它）。
> **执行方式**：r9 未走单次派发，按判断密度切**八个**有界工作包（清点 → 机械修正 → 核心改造 → 引用改接 → selftest 重写 → 调用边界 → 域门编码 → 收尾），每包独立验证后进下一包；F2–F9 的逐条核实纠正了规范稿（主会话起草）的 5 处错误，逐条锚见自陈 14(e)。
> 状态改为 `criteria-r9-pending-independent-review`；r9 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r10 修订（第十轮审查修复：12 blocker / 7 major / 4 minor 全处置）**：r9 经三席跨厂商隔离独立审查——**grok fail（2B/7M/5m，九项裁量全部维持）、sol fail（9B/4M/1m，维持 8 项、对 ② 提落地层挑战）、deepseek-v4-pro 收窄范围 pass（0B/1M，其 M 与另两席收敛）**；
> 两席全文席发现几乎不重叠（互补盲区），去重 12 条 blocker **全部经主会话逐条核实成立、无一证伪**，完整登记见 `docs/n1b-disc-r8-review-register.md` 第九节。共性：**r9 的 complete 主线通、pre-only（POST 缺）断**——R4 只把 POST 在的 skip 编码折进新闭域，POST 缺的死亡收口留着 r5 旧 cause 在域外；
> 以及新域门引用了 capture 中无载体的量。
> r10 修订内容（五个有界工作包，每包独立验证后进下一包）：
> - **T1 pre-only 收口**：删除 blanket 赋值，`dw_*` 按 marker 真实求值、输入不可得才记 `post-destroy-unobservable`（逐字纳入域，16/8 值）；V2 (1b) 扩为任一 `SKIP|item=destroy`；⑨ 补「无 SKIP」前提与两条验收用例（E3 成功终态、全拒+死于 POST 前，均 pass）。
> - **T2 字段域与载体**：`revents` 编码域由 0..63 封闭扩为全部非负整数（0..63 降格为已知位组合描述）；D8b 窗口钟补载体 `BEGIN|ws` / `END|we`（此前声称落盘却无任何 marker 承载）；P0 时刻命名 `p0_ready_mono_ms` 纳入钟域门（14 项）。
> - **T3 七分量全函数**：逐分量补全取值域（`process_death_observed` 补 false 分支、`marker_tail_state` 扩五值、省略号 cause 全部具名化——新造 6 复用 3）；五态第 5 行补分量值 `marker-contradiction`（两轴独立）+ SKIP 同现域门；
> 死亡证据收紧为 (a) 基线 absent+静默 或 (b) 携 SIGKILL/SIGTERM 条目（「进程退出记录」幽灵支删 4 处；faultlogger 条目证明事件不证明终止）；签名五行求值真值表冻结（无条目=确定观测 false；FaultRecv 失败=unobservable）；complete 崩溃签名登记载体改挂 runner evidence。
> - **T4 fail-open 收口**：fault 条目多条目保守聚合（任一正向优先不被稀释；混合不可解析条目无可解析正向 → 分量 unobservable——按 false 记是 false-negative）；
> D7 早退矛盾（`D7_END` 在而 `elapsed<20000`，与伪码出口机械矛盾）挂 F8 + 19999/20000 边界对；F1 第 3 支结构注（位点集全在 P5T 前，恒与 F2 叠加、verdict 由 F2 承载；保留为 N1b 诊断事实记录，防误删）。
> - **T5 收尾**：F1 索引补限定（三席收敛）+ 5 处「F1-F9」排除式同步；门 3 改 A1-A9；`T_tail` 标签挂 D7；D6 U4 旧句改按 V2；自陈 14(e) F3 行更正；裸拼法 2 处防误引注；F4/F8 论域切分（(4) vs (2)(3)）；「fail 只来自 PRE 缺失」收窄。
> **r10 主会话裁量（超出审查席最小修法或席间分歧处，请下轮审查席逐项挑战）**：① `post-destroy-unobservable` 采「删 blanket + 折入域」双管（席 A 给的是二选一）；② D8b 载体选 BEGIN|ws / END|we（时序诚实优先于单 marker 集中承载）；③ `marker_tail_state` 新值命名 `no-death-evidence`；
> ④ 死亡证据 (b) 支限定 SIGKILL/SIGTERM 条目（席 B 只要求删幽灵支，未授权正向收紧——但「条目证明事件不证明终止」是其原推导的自然延伸）；⑤ 签名真值表「无条目=observed-false」（席 B 自陈曾写 unobservable，本裁定以其 #6 构造为机械矛盾消解——该构造无条目且 FaultRecv 执行过，是确定观测）；
> ⑥ F1 第 3 支保留不删（席 A 给「删或扩」二选一，裁定取保留+结构注：删不改变任何 verdict 却丢 N1b 诊断事实）；⑦ 混合解析失败聚合挂 unobservable 不挂 false；⑧ P0 只设非负不设顺序约束（理由显式登记）。
> 状态改为 `criteria-r10-pending-independent-review`；r10 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r12 修订（第十一轮审查修复：8 blocker / 8 major / 3 minor 全处置）**：r10 经三席跨厂商隔离独立审查——**grok fail（3B/6M/3m，上轮 12 条 blocker 落地核对 10 已修）、sol fail（7B/2M，其九条修复核对 4 已修 4 部分 1 已修新缝）、deepseek 两派失败记 attempt-not-counted**；
> 主会话同期自查立案 2 条（其一与两席三路收敛）。8 条去重 blocker **全部逐条核实成立、无一证伪**，登记见 `docs/n1b-disc-r8-review-register.md` 九之二。共性：**全部是「修旧引入的新缝」**——集中在主会话起草规范、执行层机械落地的三处规则块（真值表、死亡证据、成功路径收口）；归因停机主线两席均未挑战，八项裁量多数维持。
> r12 修订内容（五个有界工作包）：
> - **U1 真值表按支求值**（三路收敛）：r10 五行按序互斥表废除，改「前置（FaultRecv 失败最先）→ 第一步按支三值求值（支 1 只看 fault_type、支 2 只看 signal、支 3 看 fault_type ∧ last_visible_site ∧ 无矛盾；每支独立 true/false/unknown）→ 第二步聚合 true > unknown > false」——已知正向不被任何旁路未知稀释；
> 多 unknown cause 冻结优先序（faultrecv-unavailable > fault-type-unparsable > signal-unparsable > no-visible-marker），全部逐字入 raw。双口径单值化：Fault_Type/Signal 字段不可解析豁免（未实测平台产物），其余字段仍 F8。聚合序冻结 = faultlogger 文件名字节序。`dw_destroy_distinguishable_from_timeout` 16+2 逐值落具名 cause（裸 unobservable 废除，complete 主线 POST 自此有合法单值）。（r13 注：该计数为 r12 拆分前旧值，r12 第三包已改 17 值；r13 第三包又改 18 值——以正文 D-W 节为准）
> - **U2 死亡证据谓词统一**（两席收敛）：(b) 支降为辅助合取——`process_death_observed=observed-true` 唯一充分条件回到 (a)（pidof 基线下 absent + capture 静默），SIGKILL/SIGTERM 条目仅在 pidof absent 确认下作并存关联证据（理由写死：glob 不绑 `:vpn`、SIGTERM 可捕获）；`dw_watchdog_killed` 四合取第四项改死亡分量合取（APPFREEZE 只证事件不证消失）；`:866`/`:776` 自立谓词废除，死亡判定唯一谓词源 = 本分量。「平台终止」11 处逐处归类。已知预期行为变化登记：u4 一构造 observed-false → unobservable。
> - **U3 收口补全**：D8b 新增 BEGIN-only 死亡收口（六字段落 `storm-incomplete-pre-only`）与**阶段未达**收口（七字段落 `stage-not-reached`，P4 范围外发现——r13 注：同一发现，标签统一为 r12-P3 施工发现）——合法平台死亡的两种 storm 形态不再落 F4；`post-destroy-unobservable` 按五态拆分（死于 P9 前落 `destroy-not-reached` 跨域复用；destroy 已过保留原 cause），域计数 17/9，五态×skip×两 cause 分流矩阵穷尽。（r13 注：该计数为 r12 当轮值——r13 第三包死亡收口再拆出 `call-boundary-incomplete` 透传值后正文为 18/10，以正文为准）
> - **U4 selftest 断言修正**：⑩ 守卫反例「不 fail」删除（verdict 由 F2/F3 承载，与 Ⅲ 同构）；「含与任意 Fault_Type 组合」限定（原把 CPPCRASH 圈进 false 断言）；「Ⅰ/Ⅱ/Ⅲ 另发 POST」限定为 Ⅰ/Ⅱ（Ⅲ 与 POST 结构不可达）；补 CPPCRASH+SIGKILL → observed-true 对照例。
> - **U5 执行层纠正主会话 2 处**：跨钟相减（ws 单调钟 − 死亡墙钟）无物理意义，按同侧墙钟落地并写明禁止；r12 登记块初稿「七个工作包」计数错（实列 8 项）——本块实列五包。
> **r12 主会话裁量（请下轮审查席逐项挑战）**：① unknown cause 优先序取该四值序（无席间先例，主会话定）；② 豁免边界限 Fault_Type/Signal 两字段；③ `destroy-not-reached` 跨域复用不新造变体（近义字面是误引雷区）；④ `dw_watchdog_killed` 死亡合取后正例构造含 SIGKILL 关联证据（非纯 APPFREEZE）；
> ⑤ u7 的 P5T–P6 窄窗「阶段未达」同构缺口未修（窗口极窄且涉 u7 求值规则本体，登记交挑战）；⑥ pre-PRE 域裸/省略号 cause（M-05 族）未逐字段具名化（verdict 不受影响，工作量与风险比未达平衡点）；⑦ `stage-not-reached` 不跨域共用 `destroy-not-reached` 字面（位点域不同）。
> 状态改为 `criteria-r12-pending-independent-review`；r12 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r13 修订（第十三轮审查修复：5 blocker / 5 major / 2 minor 全处置）**：r12 经三席跨厂商隔离独立审查——**deepseek-v4-pro（收窄至 U1 两步求值，27 格机械枚举形态）pass 0B/2M**（其 M 已当日采纳）；**grok fail 1B/3M/3m**（上轮 3B 全确认已修）；**sol fail 4B/2M/1m**（上轮 7B 全确认已修）。去重 **5 条 blocker 全部核实成立、3 条两席独立收敛**，登记见 register 九之三。轨迹 12B → 8B → 5B，全部是「收口单值化」同构问题、无新类型结构性缺陷。（r14 记账更正：实为 4 blocker、轨迹 12→8→4，见 r14 块与 register 九之三——本块计数照登历史）
> r13 修订内容（三个工作包）：
> - **V1**（两席收敛×2）：u7 阶段未达支（PRE 在、`D7_BEGIN` 缺、死亡 true、位点 ≤ P5T → `stage-not-reached`，复用 D8b 字面；skip 分支优先不截胡）；`dw_watchdog_killed` 改求值序写死的有序互斥表①-⑤（① `_C` 在+absent+EXIT 缺 → 新具名 `destroy-terminal-candidate`；② `_T` 在 `_C` 缺 → 透传 `call-boundary-incomplete`；③ **双缺+四合取+位点 P8-P10 → observed-true——废除「`_C` 缺一律不得 true」**；④ EXIT → false；⑤ 兜底）；SIGKILL 证据角色辨析成段。
> - **V2**（sol ×2）：`dw_destroy_distinguishable_from_timeout` 二维求值序（第一维 destroy 结局先判——timeout/reject 无论 class 一律 `destroy-unresolved`；第二维 class 逐值表仅对 resolved 求值——V2 落地时为 17 值，V3 的 `call-boundary-incomplete` 透传后为 18 值；正交相加病根拔除，selftest ④ 五格矩阵）；FaultRecv 按文件建模（逐文件取回状态；部分失败经混合支吸收、cause 落 `faultrecv-unavailable` 且优先序「整条证据缺失 > 证据在手字段坏」；**已有可解析正向不被取回失败稀释**）。
> - **V3**（M/m 收尾 + 主会话两处补裁）：混合支写明先于 APPFREEZE；false 支删「无任何上述死亡证据」残留合取；`call-boundary-incomplete` 透传自身 cause（不再并入 post-destroy——「destroy 已过」对五态歧义态是事实错误）；主会话补裁① 支 EXIT 缺限定（complete 正常主线语义应为 false）、u7 skip 分支优先；计数与来源标签统一。
> **r13 主会话裁量（请下轮审查挑战）**：① watchdog 有序表 ③ 允许双缺判 true（两席同向废除「一律不得」——但「waiter 被 watchdog 杀」的正向证据不含任何 watchdog 专属条目，仍只是静默+死亡+位点的合取，接受与否请裁）；② ① 支的 `destroy-terminal-candidate` 为新具名 cause（原句字面收纳）；③ u7 复用 D8b `stage-not-reached` 字面（grok 允许同字面或专名）；④ `call-boundary-incomplete` 透传后域计数 18/10。
> 状态改为 `criteria-r13-pending-independent-review`；r13 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r14 修订（第十四轮审查修复：4 blocker / 2 major / 2 minor 处置）**：r13 经三席跨厂商隔离独立审查（绑定 e3f33ec）——**deepseek-v4-pro（两表机械枚举）pass 0B/2M**、**grok fail 1B/2M/2m**、**sol fail 3B/2M/2m**；上轮 4 条修复全部确认、无回归；去重 4 条实质（轨迹 12→8→4，第十三轮记账经 sol m-02 更正）。
> **程序事件登记**：deepseek 先回、其 2M 被主会话当场采纳落盘，违反审查期间版本纪律（失误 #28），两席重绑 e3f33ec 出票；其一「不可达」论证被两席独立证伪（失误 #29：采纳审查席数学断言未独立核实——EXIT 可早于 `_T`，inwait 竞态段明文）。
> r14 修订内容（两包；P1 及 P2 均因 glm-5.3-flash 无消息失败由主会话接手执行）：
> - **W1 二维→四步有序互斥**（grok B-01 = sol B-03 收敛）：(0) skip2∪死亡收口3 透传；(1) 类 0/0b 直接映射（`_C` 缺是其路径常态，收进「仅 resolved」正是原洞）；(2) `_C` 在且无 resolve → destroy-unresolved（「已发起」钉死 `_C` 在、与五态对齐，废除可扩域措辞）；(3) 已 resolve → 普通类逐值表。不可达性注（普通类蕴含 `_C` 在——判定表 0b 截获一切无 `_C` 输入）。selftest ④ 补洞格/两读格/skip 格三钉。
> - **W2 watchdog 集群**（grok M-01 = sol B-02 收敛 + sol B-01/grok 裁量①挑战同向）：② 加 EXIT 缺（原「不可达」注被两席证伪，替换为更正注）；③ 合取 poll 进入证据（`DW_BARRIER` ∨ `dw_inwait_confirmed=observed-true`）+ 结论句去因果化（「waiter 于 destroy 未及窗内死亡且已确认进入等待」——删除一切 watchdog 归属断言；SIGKILL/OOM/生命周期在该观测不可区分）；drain 期死亡 → ⑤。字段名保留为历史名 + 语义注防 N1b 误引。⑪ 正例/反例全面同步（含 EXIT 在交错构造、drain 期构造两新钉）。
> - **W3 FaultRecv 正向段按支定义**（sol M-01）：「可解析正向」= 任一签名支完整前件真（含第 3 支 APPFREEZE+短时步+无矛盾），不被不可解析条目稀释。
> - **W4 收尾**：u7 skip-first 夹具（grok m-02）；r14 前临时修复（`dw_waiter_spawned` 定义、no-worker 删除、② 旧注）并入本块登记。
> 状态改为 `criteria-r14-pending-independent-review`；r14 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r15 修订（第十五轮审查修复：3 blocker / 3 major / 4 minor 处置）**：r14 经三席跨厂商隔离独立审查（绑定 a43d0c4）——**deepseek pass（0B/3M，两表机械枚举）**、**grok pass（0B/2M/4m，八点高风险逐格核）**、**sol fail（3B/1M/2m）**；两席 pass 票被 sol 的三条 blocker 推翻（deepseek/grok 验证了主会话不可达性注的内部逻辑、未查前置门旁路——sol 独捉；B-02 marker class 未来依赖系 r9 以来第一条全新类型）。去重 3 blocker 全部核实成立（轨迹 12→8→4→3）。
> r15 修订内容（P1 主会话在旧约定下接手——彼时执行层连续三次无消息失败；P2/P3 按新约定拆细派发）：
> - **X1 marker 删 class + 派生序重排**（B-02+B-01）：`DW_RETURN` 删 `class=<c>` 只发 raw（worker 即时发射无法预知 SKIP/`_T`/`_C` 终局——该字段自 r1 起无人质疑），runner 收齐后唯一派生、无双源；派生序冻结「合法域 → 0/0b 前置检查 → 未知位门 → 普通 1-11」（0/0b 的 SKIP/`_C` 门前移——「普通类 + `_C` 缺」自此真正不可达，原 r14 不可达性注主论证错误照登于重写注内）；selftest 补派生序补格。
> - **X2 watchdog ③ 仅认 inwait**（B-03）：BARRIER 只证到达调用点不证进入等待（:676 明文），「BARRIER∨inwait」退化为 BARRIER——③ 收紧为仅 `dw_inwait_confirmed=observed-true`，BARRIER-only → ⑤；:688 头句改历史字段名声明 + 去因果化口径；⑪ 补反例 (c)。
> - **X3 M/m 批**：② EXIT 缺移前件槽（grok M-01）；FaultRecv raw 级三谓词破循环 + 跨分量不传导（sol M-01 + grok m-04）；skip 透传理由拆分（sol m-01）；「二维/第一维/第二维」活规则索引改指四步（grok m-03）；计数残留清零（deepseek M-02/M-03、grok m-01）；r13 旧块计数原位注（sol m-02）。
> 状态改为 `criteria-r15-pending-independent-review`；r15 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r16 修订（第十六轮审查修复：5 blocker / 2 major / 5 minor 处置）**：r15 经三席跨厂商隔离独立审查（绑定 ad37cd9）——**三席全部 fail**（deepseek 1B/1M、grok 1B/1M/3m、sol 4B/1M/1m）；上轮三条 blocker 修复核对**全部确认闭合**。去重 5 条 blocker **全部核实成立**（轨迹 12→8→4→3→5——回升，但 5 条中 4 条是 r15 修复自身引入的缝：主会话 P1 手工落盘的传播缺口与 `item=D-W` 笔误、P2 收紧暴露的 inwait 两缺口、B-02 修复未传播到 POST 的死锁。**修复面即新审查面**）。（r17 更正：实为 4 minor，register 九之六点名 4 条）。
> r16 修订内容（两包，均派发执行层）：
> - **Y1 class 派生集群**（blocker 1/2/3）：派生序传播对齐 6 组活句（含「同一钉两处落盘」都改）；`item=destroy` 笔误更正 + sol 分域声明（无 RETURN 只走 skip/death 编码、类 0 仅认 `item=destroy`、`item=D-W`+RETURN 同现 → F3）；POST class 死锁解环（**主会话裁定方案 b**：complete 由探针主线程 P12 发射前唯一派生、runner 重建比对沿 E2 模式仅作校验、pre-only 由 runner 派生——**供下轮挑战**）；legacy class 负例回归钉；④ 18 类句拆写。
> - **Y2 inwait 集群**（blocker 4/5）：四字段族死亡收口（`inwait-marker-unobserved` 具名 cause、统一覆盖 INWAIT 缺 + 死亡分量 true 全形态）+ 整条 pre-only pass 夹具；state=S 假阳性收紧（**syscall 不可读不得 true**——主会话裁定宁缺勿误路线，poll-active 原子窗属探针实现变更不采）+ post-poll 阻塞反例。
> - **Y3 字面收尾**：行号锚更正、raw 谓词 (3) 归一化、门域措辞三处、「:359」残留。
> 状态改为 `criteria-r16-pending-independent-review`；r16 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> **r17 修订（第十七轮审查修复：3 blocker / 4 major / 4 minor 处置）**：r16 经三席跨厂商隔离独立审查（绑定 3cef412）——**三席全部 fail**（deepseek 1B/0M、grok 3B/4M/3m、sol 1B/1M/1m）；去重 3 blocker 全部核实（轨迹 12→8→4→3→5→3）。r16 五条修复中四条确认落地；新缝集中在 r16 的两个新声明。**主会话裁定①（P12 派生）被两席驳回**（方向未翻，落地不完整即 fail-open）；**裁定②（syscall 三合一）双维持**。
> r17 修订内容（两包；P1 主体执行层落盘、失败于收尾自查，主会话补完最小片段；P2 执行层）：
> - **Z1 假分划重写**（B-01 三席收敛）：分域四子域——(a) skip 表指派、(b) pre-only 死亡收口、(c) **类 0 不要求 RETURN**（其两条真实路径均无 RETURN，判定输入 = `SKIP|item=destroy` 存在性）、(d) **complete∧join-timeout 具名收口 `poll-never-returned`**（class 域 18→19；join 域核对后不扩——对应值即既有 `join-timeout`）；poll raw 四字段同 cause 收口；④ 计数同步；complete 与 barrier 两验收夹具。
> - **Z2 inwait 域补扩**（B-02）：`dw_inwait_confirmed` 活域逐字并入 `inwait-marker-unobserved`（与 source/samples 同句）；errno 承载更正（非独立 D 项）。
> - **Z3 比对挂载 + 输入补全**（B-03 两席收敛）：P12 派生比对不一致**逐字挂 F8(2)**（原「沿 E2 模式」未入闭集 = fail-open）；P12 输入清单补 poll/drain raw；**worker→main raw 共享快照冻结**（marker 发射前写入、P12 可读、同值）；`dw_outcome` 全部派生字段同所有权。
> - **Z4 D6b 位次裁定**（sol）：「worker 终态后」修订为「终态或 join-timeout 登记（abandoned 即主线程侧终态替代）后」；D6b 操作对象 fd_dup 与 worker 存活无冲突。
> - **Z5 M/m**：`item=D-W`+RETURN 轴改 F8(2)；「照 13 类判定表」两处改派生序；r16 登记 4 minor 更正注；「第二维」历史句加注。
> 状态改为 `criteria-r17-pending-independent-review`；r17 须再次经跨厂商隔离独立审查 0 blocker，方可请用户授权判据冻结与后续动作。
>
> 依据 [`ADJ-T0-N1B-20260831-0001`](native-nx-n1b-adjudication.md) §四授权设立门代码 `N1BDISC` 的前置发现 campaign。**判据冻结前：不得开始任何测量、不得分配 AUTH/pair 或 evidence ID**（决议 §4.2：`evidence-schema.md` 门代码扩展已完成登记（`docs/evidence-schema.md:29`），但 ID 分配仍以判据冻结 + 跨厂商隔离独立审查 0 blocker + 用户显式授权为前置）。
>
> **本 campaign 不是功能门。** 它没有任何功能 pass 条件；`verdict` 只对基础设施与完整性求值（决议 §4.2，`docs/native-nx-n1b-adjudication.md:114-119`）。平台行为「不如预期」永远不是 `fail`、不是 `blocked`，全部落入三态观察字段。`verdict` 枚举不新增任何取值（`verdict: collected` 非法，`collected` 属 `record_status`）。
>
> **本判据不构成设备 Live 授权**：物理执行须用户显式授予全新 AUTH（决议 §4.3.9，`docs/native-nx-n1b-adjudication.md:132`）；DISC 与 N1b 正式门**不得共用同一 AUTH/pair**（决议 §4.3.8，`:131`）。

## 定义与归属

N1BDISC 是按 [`ADJ-T0-N1B-20260831-0001`](native-nx-n1b-adjudication.md) §四设立的前置发现 campaign（基础决议 §二.10 的一次门顺序变更，`docs/native-nx-governance.md:46`；构成 §三 pre-E8 native 物理例外的一次新行使，`docs/native-nx-governance.md:53`）。它存在的唯一理由：N1b 正式门是单次执行、不重试、不换 ID 的不可逆物理 campaign，而其设计依赖七项从未在本元组实测的平台事实（决议 §4.1，`docs/native-nx-n1b-adjudication.md:104-109`）；DISC 用一次可控执行把这些赌注变成事实。
材料包的 D1-D7 旧草案已被三席一致否决（决议 §四），本文按决议裁定的顺序与强制条件（§4.3）重拟。

- **证据身份**（决议 §4.2，`docs/native-nx-n1b-adjudication.md:113`）：授权 `AUTH-N1BDISC-PHYS1API26-<YYYYMMDD>-0001`；campaign `N1BDISC-PHYS1API26-<YYYYMMDD>-0001`；evidence `EV-N1BDISC-PHYS1API26-<YYYYMMDD>-0001`。attempt `initial`、retry `N/A`、单次执行不重试不换 ID（沿 G0 campaign 纪律，`docs/g0-go-arm64-physical-probe.md:162-166`）。
  日期在正式授权时冻结。**不得与 N1b 共用同一 AUTH/pair。**
- **环境**：物理冻结元组 HarmonyOS / PLA-AL10 / `PLA-AL10 7.0.0.102(SP8C00E102R7P3)` / API 26 / aarch64 / arm64-v8a——pre-E8 native 物理例外（`docs/native-nx-governance.md:53`）：独立 AUTH/pair、冻结元组与输入哈希、白名单 HDC、单次执行、双向不外推、禁止性能/长稳/渠道/产品外扩。
  **版本核对警示**：E3 0002 授权记录的同一设备版本为 `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:15`），与本冻结值不同；目标绑定门（流程第 5 门）必须实测复核完整系统版本，漂移即 `blocked` 停。
- **实现载体**：新独立目录 `spikes/n1b-disc-phys-hap/`（ArkTS VpnExtensionAbility + Rust/NAPI 探针 + host runner），不与任何既有 spike 共用代码。bundle 名冻结 `cn.alfadb.netbird.n1bdisc`（沿 `cn.alfadb.netbird.*` 惯例；HDC 白名单与 faultlogger 匹配均用此名）。
- **使用边界**（决议 §4.4，`docs/native-nx-n1b-adjudication.md:134-136`）：DISC 事实只作为 N1b r2 判据的预注册设计输入，不构成 fd 合同、数据面、E4 或任何门的功能结论。`verdict: pass` 不得在任何后续文档中被引用为平台行为结论（`docs/evidence-schema.md:90`）。仅当 DISC 达到 `reviewed-pass` 且目标元组、VpnConfig、进程模型、SDK 与相关哈希与 N1b 一致时，其事实方可进入 N1b 预注册；**禁止「DISC 已见故 N1b 跳过复测」**。

## 发现目标（七项平台事实 + waiter 事实）

U1-U7 编号沿用历史材料包登记（`docs/t0-n1b-discovery-materials.md:140-146`；该材料包已降级为历史审议材料，编号仅为可追溯性保留），**口径按决议修正后执行**：U5 按 §4.3.2 修正（`create(config): Promise<number>` 只返回 fd，`spikes/e3-vpn-extension-physical-preflight-hap/entry/src/main/ets/vpnextensionability/E3PhysicalVpnExtensionAbility.ets:153`），U6 按材料包勘误 F3 修正（`isBlocking` 默认值 SDK 注释明写 false，未实测的是运行时 fd flags，`docs/t0-n1b-discovery-materials.md:31`）。
全部字段三态登记：`observed-true` / `observed-false` / `unobservable`（`docs/evidence-schema.md:88`）。

| # | 待发现问题 | 决议修正后口径 | 采集 D 项 | 落盘字段 |
| --- | --- | --- | --- | --- |
| U1 | 应用自身 socket 发往本 VPN 路由覆盖地址的流量是否投递到本应用 tun fd（**前提：至少一次 `sendto` 成功且按冻结包身份匹配；offset-0 与 offset-4 两种解析任一匹配即 true**） | 观察事实，无预期；构包/调用缺陷不得伪装成「不投递」 | D4 | `u1_socket_to_tun_delivery`、`u1_match_offset` |
| U2 | **成功 `write` 到 tun fd dup 副本的合法 IPv4/UDP 包**是否被内核投递到本应用 sink socket（**前提：至少一次 `write` 返回 `n==len`**） | 观察事实，无预期；写失败不得伪装成「不投递」 | D5 | `u2_tun_write_to_sink_delivery` |
| U3 | tun 帧格式：是否含 PI 头/填充；read 长度与 IPv4 `total_length` 的关系 | 决定性数据 = **首个逐字段匹配冻结受控包身份的成功 read**；外来包仅观察不入判定（决议 §4.3.1） | D4 | `u3_first_read_len`、`u3_first64_hex`、`u3_pi_header_present`、`u3_prefix_format`（三态主字段，r4 第三趟 T3）、`u3_readlen_vs_total_length` |
| U4 | `destroy()` 返回后进程能否同步执行 `fcntl`/`close`/`read` | 区分原始 fd 与 dup 副本两面；**逐子项三态驱动，摘要不得替代子项** | D6 | `u4_post_destroy_sync_observable`（摘要）+ `u4_orig_getfl`/`u4_orig_getfd`/`u4_orig_close`/`u4_dup_getfd`/`u4_dup_read`/`u4_dup_close`/`u4_dup_fd_reuse` |
| U5 | `RouteInfo` 候选值的接受性 | **不得声称产出「实际 interface 名」**（决议 §4.3.2，`docs/native-nx-n1b-adjudication.md:125`）；D2 至多验证预注册 interface 输入是否被接受，`create` 拒绝是事实不是 fail | D2 | `u5_routeinfo_acceptance`（逐候选） |
| U6 | create 后 tun fd 初始 flags 与 `isBlocking` 的运行时效果 | 任何 `F_SETFL` 之前采 `F_GETFL` 基线（材料包勘误 F3）；取值中性，不写预期极性 | D2 | `u6_initial_flags_and_isblocking_effect`（`o_nonblock_present` / `o_nonblock_absent` / `unobservable`）、`u6_nonblocking_initial`（三态主字段，r4 第三趟 T3，派生表见 D2 节 2.4 行） |
| U7 | Extension 进程对长同步任务的 watchdog/AppFreeze 行为 | **仅 VPN 仍 live 时执行与赋值；无 live VPN 一律 `unobservable`，不得赋 true/false**；结论只覆盖冻结时长与负载，不外推 | D7 | `u7_long_task_watchdog_behavior` |
| — | waiter 事实（OB-03 路径①/②判定输入） | 恰好一个登记在册 worker；全部结局均为观察事实，不设 pass 条件（决议 §三.3，`docs/native-nx-n1b-adjudication.md:83`） | D-W/D6 | `dw_*` 字段族（见 D-W 节） |

## 冻结的执行面

### 构建输入（逐字冻结，与 N1b 将使用的产物同一）

| 项 | 冻结值 |
| --- | --- |
| crate | `boringtun` `0.7.1` |
| checksum | `15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939` |
| feature | `default-features = false, features = ["ffi-bindings"]` |
| 禁用 | `device` feature——其 `TunSocket::new` 会把纯数字 name 直接当 fd 接管、`Drop` 无条件 `close`（同源禁令见 `docs/n1b-gate-plan.md:40`） |
| cargo | `--offline --locked` |

**D1 加载的 `.so` 必须与 N1b 将使用的 crate/checksum/feature 同一产物**（决议拆 ID 论证的前提：DISC 事实进入 N1b 预注册须哈希一致）。
**BoringTun 数据面全程不调用**：D1 仅 `dlopen`+`dlsym` 解析符号、不调用任何 `wireguard_*`/`new_tunnel`/`tunnel_free` 入口；D5 注入与 D8 阶梯均为探针自合成 raw IPv4、不经 BoringTun（避免 crypto 失败冒充平台行为）。
因此已知陷阱「`encapsulate` dst 缓冲过小 panic → ffi panic hook `raise(SIGSEGV)`」（出处 `docs/t0-n1b-discovery-materials.md:186`，转引 `ffi/mod.rs:304-309`）与「按 `total_length` 截断交付/字节账口径」（尺寸界 `>= src.len()+32` 且 `>= 148` 的冻结义务，参照 `docs/n1b-gate-plan.md:171-173`）在本 campaign **声明不适用**；静态断言 A3（见下）机器保证这一点。

### SDK 依据（r1：主会话逐字核实目标 SDK d.ts，冻结摘录）

以下摘自 API 26 SDK（`/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/ets/api/@ohos.net.connection.d.ts` 与 `@ohos.net.vpnExtension.d.ts`），为 MR 矩阵字面值的唯一依据：

```typescript
// @ohos.net.connection.d.ts
export interface RouteInfo {
  interface: string;            // 必填，@since 8
  destination: LinkAddress;     // 必填，@since 8 —— 对象，非前缀字符串
  gateway: NetAddress;          // 必填，@since 8 —— 对象，非地址字符串
  hasGateway: boolean;          // 必填，@since 8
  isDefaultRoute: boolean;      // 必填，@since 8
  isExcludedRoute?: boolean;    // 可选，@since 20（本 campaign 不设置）
}
export interface LinkAddress {
  address: NetAddress;          // 必填，@since 8
  prefixLength: number;         // 必填，@since 8
}
export interface NetAddress {
  address: string;              // 必填，@since 8
  family?: number;              // 可选，@since 8：1=IPv4，2=IPv6，默认 1
  port?: number;                // 可选，@since 8：0-65535；路由 destination/gateway 无端口语义，本 campaign 不设置
}
// @ohos.net.vpnExtension.d.ts
export interface VpnConfig {
  addresses: Array<LinkAddress>;      // 必填，@since 11
  routes?: Array<RouteInfo>;          // 可选，@since 11
  mtu?: number;                       // 可选，@since 11
  isBlocking?: boolean;               // 可选，@since 11，默认 false
  // dnsAddresses?/searchDomains?/isIPv4Accepted?/isIPv6Accepted?/isInternal?/
  // trustedApplications?/blockedApplications?/vpnId? 本 campaign 均不设置
}
// @ohos.net.connection.d.ts —— D2.7 / D8a 引用的 MTU oracle 先验所据接口形态
export interface ConnectionProperties {
  interfaceName: string;             // 必填，@since 8
  domains: string;                   // 必填，@since 8
  linkAddresses: Array<LinkAddress>; // 必填，@since 8
  dnses: Array<NetAddress>;          // 必填，@since 8
  routes: Array<RouteInfo>;          // 必填，@since 8
  mtu: number;                       // 必填（非可选），@since 8 —— "Maximum transmission unit."，type {number}
  isIPv4LinkValid?: boolean;         // 可选，@stagemodelonly，@since 24
  isIPv6LinkValid?: boolean;         // 可选，@stagemodelonly，@since 24
}
```

**溢出主张更正（r0 错误登记）**：r0 曾把 MR1 描述为「n1b 冻结值的 create 面（`docs/n1b-gate-plan.md:47-49`）」——该三行仅含 address（`10.99.0.1/32`）/ route（`10.99.0.0/24`）/ mtu（`1400`）三个值的语义，**不含** `interface`/`gateway`/`hasGateway`/`isDefaultRoute`。这四个 RouteInfo 必填字段的取值（`"wlan0"`、`10.99.0.1`、`true`、`false`）是**本 campaign 新增的 RouteInfo 必填字段候选**，为 DISC 起草选择，不得挂 N1b 名下；n1b 冻结值只贡献 `addresses`/`routes` 覆盖网段/`mtu` 的语义面。
r0 另把 `destination`/`gateway` 写成字符串字面（`"10.99.0.0/24"`/`"10.99.0.1"`），与上列 d.ts 强类型不符（ArkTS 按字面编译不过；即便绕过也会因错误原因被拒，污染 U5），r1 已按 d.ts 对象形态重写。

### 探针 syscall 面（冻结允许清单，完整一次性列出）

`dlopen` / `dlsym` / `dlerror`；
`fcntl`（仅 `F_GETFD`/`F_GETFL`/`F_SETFD`/`F_DUPFD_CLOEXEC`/`F_SETFL`——后者仅施于 dup 副本）；`dup`（仅 `F_DUPFD_CLOEXEC` 不可用时回退，两路径均登记，沿 `docs/n1b-gate-plan.md:83` 先例）；
`poll`；`read`；`write`；`close`（探针自有 fd 清理与 D6 双 close 探测）；`socket`（仅 `AF_INET`/`SOCK_DGRAM`/`0`）/ `bind` / `sendto` / `recvfrom`（D4/D5/D6b 信道与 sink）；
`clock_gettime`（仅 `CLOCK_MONOTONIC`）；`clock_nanosleep`（仅 `CLOCK_MONOTONIC`、时长两档冻结：**10 ms（barrier / in-wait / worker 终态等轮询间隔）与 50 ms（D7 外层块间时钟门控）**，与 D7 节冻结值及 D-W 节轮询间隔逐字对应，禁止用作其他等待）；
`pthread_create`（**仅 D-W 唯一登记位点**）；`pthread_join`（**仅 worker 终态原子标志已置位后调用**；标志置位不保证 join 立即返回——见 D-W 节「有界 join 可行性裁定」；禁止在标志未置位时调用，禁止 `pthread_timedjoin_np`，其不在本清单且本目标 sysroot 无此 glibc 扩展——实测 `/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/native/sysroot/usr/include/pthread.h` 仅声明 `pthread_join`，无任何 timed/try join 变体）；
`openat`（仅 `O_RDONLY`，**路径白名单恰两个**：`/proc/self/task/<tid>/stat` 与 `/proc/self/task/<tid>/syscall`，`<tid>` 限 D-W worker 的 tid——同进程自省，不触碰他进程，不落任何写路径；专用于 D-W in-wait 证据候选，见 D-W 节；读取的 `read`/`close` 复用本清单既有条目）。

**读法依据（登记待审查确认，见起草人自陈）**：决议 §4.3.5（`docs/native-nx-n1b-adjudication.md:128`）原文为「探针**可用** POSIX `read`/`write`/`dup`/`fcntl`」——「可用」是授权式表述而非「仅可用」的穷尽式排他；同句以 E3 探针契约为对照（E3 实测使用 socket 类调用面），故读作**下限清单**。本表即 DISC 的完整冻结调用面：超出本表的 syscall 一经静态审查发现即 blocker，运行时经 A4 间接约束（无其他等待原语可用）。

### fd 纪律（完整性要求，非平台判据）

- **fd ledger 强制**：`fd_orig`（create 返回的原始 fd）、`fd_dup`（唯一副本）、`d4_send_socket`、`d5_sink_socket`、`d6b_reuse_probe_socket`、`dw_inwait_proc_fd`（D-W in-wait 证据瞬态 fd：`openat` → 读取 → 即关，两个路径各一，登记开/关位点）、`d2_late_fd`（迟到 resolve 携带的 fd：只登记 `late-fd-orphaned` + fd 号 + 出现时刻，**不关闭**——关闭责任方为进程自然退出与 host finally 的 `ForceStop`，见矩阵执行协议第 3 条），逐项登记号/角色/创建点/关闭责任方/关闭点；
  **未创建的条目登记 `not-created|cause=<分支原因>`，不留空**。ledger 缺失或与 marker 流矛盾 → verdict `fail`（完整性轴）。
- **fd ledger transition marker（r3 第二趟 E2 新增，冻结字面）**：每次 fd 状态迁移（`create` / `close` / `not-created`）发生时，探针**立即**发射一条 marker（经既有 HiLog 通道）：
`N1BDISC_FD|fd=<n|none>|role=<role[#k]>|inst=<n>|action=<create|close|not-created>|at_mono_ms=<n>|by=<closed_by|none>|cause=<cause|none>`。字段约束：`role` ∈ 冻结角色集 {`fd_orig`, `fd_dup`, `d4_send_socket`, `d5_sink_socket`, `d6b_reuse_probe_socket`, `dw_inwait_proc_fd`, `d2_late_fd`}（与 ledger 条目一一对应）；
  - **`inst` 字段（r4 第二趟 S3 新增）**：同 `role` 多实例的序号，十进制自 1 递增、按创建时刻排序——单实例角色恒 `inst=1`；`dw_inwait_proc_fd` 两个 `/proc` 路径各占 `inst=1`/`inst=2`（按其开/关位点在冻结全序中的先后定 1/2）；ledger 条目与 marker 均以 `(role, inst)` 二元组唯一标识实例（canonical 行 `role` 字段写 `role#k` 形态，如 `dw_inwait_proc_fd#2`）；
  `action=create`/`close` 时 `fd=<n>` 为实际 fd 号、`by=` 字段必填、`cause=none` 占位（**字段恒在**，占位字面冻结，runner 解析无缺字段歧义）。
  **`by=` 字段取值域按 `action` 拆分（r5 U9）**：
  - `action=create` 时 `by` 恒 `none`（create 无收口方，不参与 `closed_by` 域校验）；
  - `action=close` 时 `by` ∈ `closed_by` 七值域（见下「transition 流 → canonical 行重建规则」(b)，含 r5 U5 新增 `open-at-pre`、r6 W3 恢复 `process-exit` 后为七值，r7 X9 同步）；
  - `action=not-created` 时 `fd=none`、`by=none` 且 `cause` 必填（与 ledger 的 `not-created|cause=<分支原因>` 同源）。
  **域校验规则相应拆分**：「`by=` 字段出现取值域外字面 → 完整性失败 `fail`」仅施于 `action=close` 的 `by=` 值；`create`/`not-created` 的 `by=none` 是冻结占位字面、永不构成域外 fail（r4 版 create 写 `by=none` 而域校验又按 `closed_by` 六值判定，会使全部 FD create 被读成 fail——本句消除）。
  runner 据此可**唯一重建完整 ledger**（含 D4/D5 的 socket fd 号与关闭点）并算出与探针一致的 digest。
- **ledger canonical 序列化（r3 第二趟 E2 新增，冻结——`ledger_digest` 的唯一求值依据）**：digest 输入为 ledger 的 canonical 序列化字节串，规则逐字冻结，使同一组事实只能算出唯一 digest——
  (a) 每条目一行，行内字段**顺序固定**为 `role|fd|created_at_mono_ms|closed_by|closed_at_mono_ms|cause`，分隔符为单个 `|`；
  未创建条目 `fd` 写 `none`、`created_at_mono_ms` 写 `none`、`closed_at_mono_ms` 写 `none`、`closed_by` 写 `none`、`cause` 写其冻结分支原因字面；
  已创建未关闭条目按**求值切点**登记 `closed_by`（r5 U5 切点冻结）：
  - **PRE 快照（P5T 切点）→ `open-at-pre`**；
  - **最终重建（POST 或 pre-only 收口）→ 按收口形态记 `open-at-exit`（complete）/ `process-exit`（pre-only，r6 W3 恢复——进程死亡时内核回收；`host-forcestop` 仅用于 ForceStop 前进程仍活的 fail-cleanup 形态）**；
  两种切点下 `closed_at_mono_ms` 均写 `none`；
  (b) 数值一律**十进制无前导零**（fd 号、时刻均十进制整数）；(c) 条目**排序规则 = 按创建时刻单调序**（`created_at_mono_ms` 升序；`not-created` 条目按其登记时刻参与排序；时刻相同按上列角色集出现顺序定序）；
  (d) 行序即该排序，行尾无分隔符，条目间以 `\n` 连接，UTF-8 编码，无 BOM、无尾随换行；(e) digest = 该字节串的 SHA-256 完整 64 hex。
  **发 digest 的责任分立（r4 R1 更新字面；r5 U5 切点冻结）**：探针正常收尾时由探针发（`N1BDISC_POST` 携带最终值）；pre-only 收口时探针来不及发——由 **runner 依 transition marker 重建 ledger 并按同一 canonical 规则重算** digest（r4 R1：`pre-only` 收口时由 runner 在 evidence 记录中携带，重建条目在记录中逐字标注 `rebuilt-from-transition-marker`）。
  **digest 一致性校验限同一切点（r5 U5，禁止跨时点比较）**：
  - PRE 的 `ledger_digest` 只与「P5T 切点」的重建值比对（P5T 时刻的 transition 流快照 + 当时 open 条目记 `open-at-pre`）；
  - POST/pre-only 的最终 digest 只与「全量流 + 按收口形态记 `open-at-exit`/`process-exit`」的重建值比对；
  - 同一时点两路值不一致 → verdict `fail`；
  - 不同切点的两份 digest **本来就不同**（切点间必有增量条目与 closed_by 形态差异），不得互比、不得与对方切点的重建值比对（r4 版「当前 digest 按 open-at-exit 算、pre-only 最终重建改 host-forcestop」会使每条 pre-only 都跨切点不一致 → fail，本句冻结即堵该洞）。
- **transition 流 → canonical 行重建规则（r4 第二趟 S3 新增，逐字冻结——目标：runner 仅凭 `N1BDISC_FD` transition marker 流即可唯一重建每条 canonical 行并算出与探针一致的 digest）**：(a) **各 `action` 的全字段字面**——`create`：`fd=<十进制 n>`、`inst=<k>`、`at_mono_ms=<十进制>`、`by=none`、`cause=none`，canonical 行由此得 `role#k|fd|created_at_mono_ms` 三字段，`closed_by`/`closed_at` 置待定；
    `close`：`fd=<n>`、`inst=<k>`、`at_mono_ms=<十进制>`、`by=<closed_by 字面>`、`cause=none`，canonical 行由此得 `closed_by|closed_at_mono_ms`（同 `(role,inst)` 实例自 `create` 后首条 `close` 生效）；
    `not-created`：`fd=none`、`by=none`、`cause=<冻结分支字面>`，canonical 行 `fd=none`、`created_at=none`、`closed_by=none`、`closed_at=none`、`cause=该字面`。
  (b) **`closed_by` 取值域逐字冻结（r5 U5 增 `open-at-pre`；r6 W3 恢复 `process-exit`——r5 U12 曾以「从不产生」删除，该判断有误：pre-only 收口的未关闭 fd 正是它，产生条件 = 「pre-only 收口、探针未显式关闭、进程已死亡、内核在进程退出时回收」；`host-forcestop` 保留用于 ForceStop 前进程仍活的 fail-cleanup 形态）——域为七值**：
  = {`destroy`, `d6a-probe-close`, `probe-protocol-close`, `process-exit`, `host-forcestop`, `open-at-exit`, `open-at-pre`}（r5 U12 版曾为六值并删除 `process-exit`，r6 W3 恢复后为七值；旧六值注：「该字面在 close marker 映射与两条收口形态规则下均无产生条件」——该判断随 W3 恢复而失效）；
  前三者**必须**出现在 close marker 的 `by=` 字段（逐字取用，runner 不做推断），
    映射冻结：`fd_orig` 的 close 唯一合法 `by=destroy`（或 D6a 双 close 探测的 `d6a-probe-close`）；`fd_dup`/`d4_send_socket`/`d5_sink_socket`/`d6b_reuse_probe_socket`/`dw_inwait_proc_fd` 的协议内显式 close 一律 `by=probe-protocol-close`（D6a 对 `fd_dup` 的 close 探测为 `d6a-probe-close`）；
    后三者不产生 close marker：仍 open 的实例在 canonical 化时按**求值切点**登记——
    - `complete` 收口 → `closed_by=open-at-exit`；
    - `pre-only` 收口 → `closed_by=process-exit` 且 `closed_at_mono_ms=none`（r6 W3 恢复精确字面：进程已死、内核在进程退出时回收 fd——非 ForceStop 所关；`host-forcestop` 仅用于观测窗到点收口、无死亡证据的存活 fail-cleanup 形态）；
    - **PRE 快照（P5T 切点）→ `closed_by=open-at-pre` 且 `closed_at_mono_ms=none`（r5 U5）**；
    - `d2_late_fd` 按其登记规则恒不关闭。**切点规则（r6 W2 修正时间悖论）**：P5T 快照对一切 P5T 前已出现且未关闭的 fd（含 `d2_late_fd`）**一律记 `open-at-pre`**；**最终重建**（POST 或 pre-only 收口）才按收口形态记 `open-at-exit`（complete）/ `process-exit`（pre-only，r6 W3 恢复）。旧「恒不记 `open-at-pre`、恒按最终重建形态」表述删除（r5 版该表述使 P5T 时刻须预知未来收口形态，规则自相矛盾）。
    `by=` 字段出现取值域外字面 → 完整性失败 `fail`（域校验拆分见本节上文 r5 U9：该规则仅施于 `action=close` 的 `by=` 字段）。
  (c) **`(role, inst)` create/close 配对状态机（每实例独立）**：`uncreated → open`（create）；`open → closed`（首条 close，其后同实例再收到的任何 create/close marker → **完整性失败 `fail`**）；`uncreated → not-created`（终态）；close 无对应 open、或任何其他转移 → **完整性失败 `fail`**。重建后的 canonical 行按上述 (a)-(e) canonical 序列化规则排序拼接，digest 与探针/`ledger_digest` 的一致性要求沿上 bullet 不变。
  **selftest 须含**：`dw_inwait_proc_fd` 双实例（inst=1/2）配对用例、close 先于 create（→ fail）、重复 close（→ fail）、`by=` 域外字面（→ fail）、pre-only 收口未关闭实例 → `process-exit` 的重建用例（r7 X6：r6 W3 后 pre-only 记 `process-exit`，旧「→ `host-forcestop`」用例废除）、另设「存活 fail-cleanup（观测窗到点收口、ForceStop 前进程仍活）→ `host-forcestop`」用例。
- 探针对 tun fd 的全部 `read`/`write`/`poll`/`F_SETFL` 一律经 `fd_dup`；**原始 fd 只允许 `F_GETFD`/`F_GETFL` 观察与 D6a 段的 `close` 探测**（destroy 是原始 fd 的唯一关闭责任方；D6a 的 close 是 destroy 已履责后的双 close 探测）。静态断言 A2（见下）机器保证。
- dup 副本在初始 flags 采集完成后立即 `F_SETFL(O_NONBLOCK)`（沿 `docs/n1b-gate-plan.md:61` 的「dup 一律 O_NONBLOCK + 有界 poll」纪律），此后所有 read/write 天然非阻塞；设置后回读原始 fd `F_GETFL` 登记 OFD 副作用观察字段 `of_nonblock_shared_to_orig`，取值域闭合：`observed-true`（2.6 读回的 orig flags 含 `O_NONBLOCK` 位）/ `observed-false`（不含）/ `unobservable`（2.6 返回 `-1` 或该步未执行）。
- `F_DUPFD_CLOEXEC` 在 API 26 sysroot 已定义（`docs/n1b-gate-plan.md:76`）；运行时可用性由 D2 实测登记。

### 执行位点与结果通道（冻结）

全部探针逻辑在 **VpnExtensionAbility（`type: vpn`、`exported=false`）进程**内执行；EntryAbility 仅 UI 触发，不承担任何登记。
**HiLog 通道冻结**：`DOMAIN = 0x2900`（仓内全域惯例，沿 `spikes/e3-vpn-extension-physical-preflight-hap/entry/src/main/ets/vpnextensionability/E3PhysicalVpnExtensionAbility.ets:9`）、`TAG = 'N1BDiscVpn'`（区别于 E3 的 `'E3PhysVpn'`，`:10`）。**采集关联规则冻结**（沿 E3 0001 事故教训，`docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:5`、`:163`）：

1. marker 关联接受 `<bundle>` 进程 tag 的**三形态**——entry 形态 / `:vpn` 截断形态（hilog 丢弃 `cn.` 前缀）/ `:vpn` 完整形态；只扫描 tag 路径组件，**不按 pid 过滤**（修复口径原文见 `docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:171-172`）。
2. capture 用 `hilog -T N1BDiscVpn` 单流过滤；selftest 必须含三形态正反例（流程第 10 门强制）。
3. 结构化 detail 一律**分段输出**（编码冻结，C4；逻辑键冻结，r3 第二趟 E3）：`N1BDISC_CHUNK|stream=<s>|item=<id>|index=<i>|count=<c>|sha256=<h>|payload=<base64>`。
   **`stream`/`item` 逻辑键（E3 冻结）**：`stream` 取自冻结枚举，恰一个字面值，标识该 chunk 所属的 detail 记录类别——`dlerror`（D1 的 `d1_dlerror` 原文）/ `rejtext`（D2 的 `rejection_text`，每矩阵条目一条）/ `u3hex`（D4 的 `u3_first64_hex`）/ `foreign`（D4 的外来包首 64 字节十六进制）；
   `item` 为该 stream 内的唯一序号（十进制自 0 递增）——`dlerror`/`u3hex`/`foreign` 恒 `item=0`（各恰一条记录），`rejtext` 的 `item` = 矩阵条目 `id`（`MR1`→`0`、`MR1B`→`1`、`MR2`→`2`、`MR3`→`3`、`MB1`→`4`，与 `N1BDISC_D2_ENTRY` 的 `id` 同一编号域）。
   **重组按 `(stream, item)` 二元组分组**，组内再按既有规则（index 覆盖 `0..count-1` 无缺口〔同 index 重复按下句 S6 处置〕、升序拼接、sha256 校验、UTF-8 解码）执行；连续性规则与重复处理规则冻结：组内 `index` 集合须覆盖 `{0..count-1}` 无缺口（缺口 → 该组重组失败，按 criteria-gap 处理 (2) 判 `fail` + raw 逐字入档）。
   **重复片处置（r4 第二趟 S6 统一——本句旧文同时规定「重复 → 重组失败 fail」与「首到者优先不改判定」，自相矛盾，现二选一并使正文、gate 10 ②、runner 三处同源）**：同组同 `index` 的重复片以**首个到达者为准**参与重组，且首到片与其后到达的同 index 重复片须**逐字节一致**（片 payload 各自 base64 解码后比较）——一致 → 逐字登记 `duplicate_chunk_observed`（观察项），**不改判定、不改 verdict**（hilog 对同一行的重复投递是平台行为，不得因平台重复烧 ID）；
    - 不一致 → 完整性失败，按 criteria-gap 处理 (2) 判 `fail` + raw 逐字入档；
    - **同组同 `index` 重复片的 `count`/`sha256` 字面校验（r5 U12 补全）**：相同 `index` 的重复片若 `count` 字面不同、或 `sha256` 字面不同 → 该组事实相互矛盾（同 index 同组不可能有两种切片总量或两种原始字节摘要），直接**完整性失败 `fail`** + raw 逐字入档——S6 原文只规定了 payload 一致性，未覆盖这两项字面；
    - 异步 marker 交错因此无害：不同 `(stream, item)` 的片互不混组。
   **编码与切片规则逐字冻结**：(a) 原始 detail 为 UTF-8 字节串；切片**按 UTF-8 字节**（不按字符，避免多字节字符被截断）；单片原始字节上限 **256 B**；
   (b) 每片 payload = 该片原始字节的 **base64 编码（RFC 4648 标准字母表，含 `=` 填充）**——base64 字母表不含 `|`，故 payload 内不可能出现分隔符，`|` 歧义在编码层消除；拒绝文本、raw 十六进制、errno 原文中的任何 `|` 字符均安全；
   (c) `sha256=<h>` 覆盖**切片前的原始 UTF-8 字节全体**（非 base64 编码后字节、非拼接 payload 串）；
   (d) **重组校验顺序（runner，逐序执行）**：① 按 `(stream, item)` 分组，各组内 `index` 集合须覆盖 `{0..count-1}` 无缺口（同 index 重复按上句 S6 处置：首到者为准、逐字节一致则登记观察、不一致 fail——不再因重复本身判失败）；
   ② 组内按 `index` 升序拼接各片 payload → base64 解码得原始字节（解码失败 = 解析域缺口）；③ 对原始字节计算 SHA-256 与 `<h>` 比对；④ UTF-8 解码为 detail 文本。任一步失败 → 按 verdict 节 criteria-gap 判别方法 (2) 处理（`fail` + raw 逐字入档）；**禁止只依赖单行 detailJson**（hilog 行长截断会使判定量不可恢复，`docs/n1b-gate-plan.md:192`）。
4. **增量落盘**：每项事实完成即发射 marker（经上述通道），由 runner 持续捕获落盘；任一后续步骤崩溃不得损失既得事实（决议 §4.3.4，`docs/native-nx-n1b-adjudication.md:127`；`docs/evidence-schema.md:91`）。
5. **终态 marker 双 marker 结构（r4 第一趟 R1，取代单一 `N1BDISC_RESULT`）**：终态结果通道拆为两个 marker，字段集分别逐字冻结（缺任一冻结字段 → `fail`，沿 MJ-7）——
   - **`N1BDISC_PRE`**：在全序 **P5T** 位点发射（r5 U1 前移：原 P8T 位点废除——P8T 位于 P8 之后，而 U7 的目标观察（watchdog 杀 D7 任务）发生在 P6，死于 D7 时 PRE 尚未发射、「PRE 缺失 → fail」无条件命中，使 DISC 的核心发现路径结构上不可能 pass。现位点 = **P5 的 D8a 阶梯完成之后、P6 D7 开始之前**；**先于 P6 D7、P7、P8 D-W、P9 `destroy()`、P10 `pthread_join`**——其后的任何死亡/阻塞结局均不得损及 PRE 已承载事实）。
   字段集（逐字冻结）：`N1BDISC_PRE|ledger_digest=<fd-ledger **P5T 快照** digest，完整 64 hex（切点冻结见「ledger canonical 序列化」节 r5 U5：P5T 时刻的 transition 流快照 + 当时仍 open 条目记 `closed_by=open-at-pre`）>|skip_summary=<逗号分隔 item:cause 列表|none>`；
   PRE 的承载范围 = **P1–P5 的全部已得事实（D1/D2 矩阵/D4/D5/D8a 的全部三态字段与观察字段）+ 发射时刻的 fd ledger 快照 digest**，由 PRE 发射前已落盘的既有阶段 marker 与 chunk 流承载，求值时以 capture 全流为准——PRE 本身是该承载完整性的封口断言。**D7/D8b/D-W 的字段不在 PRE 承载范围**（r5 U1：它们位于 P5T 之后，此后仍由各自独立阶段 marker 增量落盘，POST/pre-only 重建按 capture 全流求值）。
   - **`N1BDISC_POST`**：在全序 **P12** 位点发射（P11 fd 清理之后）。字段集（逐字冻结）：`N1BDISC_POST|d6_items=<D6S1..D6S7 七子项逐字 ret/errno/reuse 结果全列>|dw_outcome=<D-W 结局逐字：poll 返回类（`dw_return_class`）、join 结果（`dw_join_result`）、watchdog 判定（`dw_*` 终态字段）全列>|ledger_digest=<fd-ledger **最终** digest，完整 64 hex（含 post 段增量）>`。（r16：`dw_return_class` 由探针主线程 P12 发射前唯一派生写入，runner 校验——见 D-W worker 序列段 r16 所有权裁定；r15 的 runner 派生声明随死锁解环改述）
     **POST 子项三选一编码（r9 第五步 BL-5 新增，冻结）**：`d6_items` 与 `dw_outcome` 的每个子项取**恰一个**编码——`result`（子项正常执行，逐字列其冻结域内结果：D6 子项为 ret/errno/reuse 值，D-W 子项为各字段冻结域内取值）｜`skipped(cause=<字面>)`（协议按 skip 表跳过该子项，cause 逐字沿 skip 位点字面 `no-live-fd`/`dup-failed`；
      **r18 扩入 `join-timeout-abandoned`**：join-timeout 形态 D6b 整段 skip 的位点字面——仅 D6S4..S7 子项适用（D6a 照常执行、result 编码不变），见 D6 节 D6b 段 r18 重裁）｜`unobservable(cause=<预注册 cause>)`（cause 逐字取自该字段取值域的冻结 `unobservable` 清单）。「缺任一冻结字段 → fail（沿 MJ-7）」与逐子项不得空置的义务不变——skip 编码是合法非空落值。
     **r12**：`dw_destroy_distinguishable_from_timeout` 按其 D-W 节 r12 逐值具名枚举落值——无 cause 的裸 `unobservable` 不得落值。（r16，grok m-02：r13「现按二维求值序——第一维 destroy 结局先判、第二维逐值枚举仅对 destroy 已 resolve 求值」旧句删除，落值规则以 D-W 节 r15 四步有序互斥为准）
     **skip 主线的 POST 落值（r9 第五步 BL-5，本项验收锚）**：「五个 create 全部被平台拒绝」是合法的平台负面结果（正是本 campaign 要发现的事实之一），该终局走 `no-live-fd` 分支、主线仍 → P11 → P12、POST 照发——「省略字段 fail、填 `unobservable` 亦 fail」的双缝由此闭合：
     `dw_return_class` 与 `dw_join_result` 的取值域**逐字纳入** `unobservable(cause=no-live-fd)` 与 `unobservable(cause=dup-failed)` 两值（计数同步见 D-W 节落盘字段族与 `dw_destroy_distinguishable_from_timeout` 枚举）；
     D6 各子项以 `skipped(cause=no-live-fd)` 落值（D6S5「不得空置」约束指**执行该步则 result marker 的数字占位不得空置**，skip 时子项以 skip 编码落值，不冲突）；`dw_*` 终态字段沿其既有具名 skip `unobservable` 清单落值。
     **不设 POST `skip_summary` 对应物（r9 第五步裁量，理由冻结）**：增设新字段须改 POST 冻结字段集（缺任一 → fail）并波及 runner 解析与 selftest ③⑨；且位点级摘要会重蹈「摘要替代逐子项」（MJ-5 明令禁止）的覆辙——三选一编码复用既有字段位、逐项落值、不新增解析分支，改动更小、缝隙面更小。

## D2 候选配置矩阵（预注册）

矩阵目的：为 N1b r2 找出**可被接受的带 RouteInfo 配置形态**，并采集 U5/U6。矩阵按下列顺序逐条执行；**首个被接受的条目即「保留条目」，其 fd 进入后续全部 D 项，矩阵立即终止**（first-accept-lock，理由与代价见后）。

| # | 条目 | 冻结配置（ArkTS 字面，按「SDK 依据」节 d.ts 形态逐字展开） | 说明 |
| --- | --- | --- | --- |
| MR1 | 主候选（N1b r2 目标形态） | `addresses: [{ address: { address: "10.99.0.1", family: 1 }, prefixLength: 32 }]`；`routes: [{ interface: "wlan0", destination: { address: { address: "10.99.0.0", family: 1 }, prefixLength: 24 }, gateway: { address: "10.99.0.1", family: 1 }, hasGateway: true, isDefaultRoute: false }]`；`mtu: 1400`；`isBlocking` 省略 | 说明外提为下方「MR1 说明」子项（r4 第四趟 W1 重构，内容逐字不变） |
| MR1B | `isBlocking` 显式变体 | = MR1 + `isBlocking: true` | 仅当 MR1 以 `rejected`（含迟到窗内 late-rejected）收口时执行到；与 MR1 的接受性差异即事实 |
| MR2 | interface 候选 `eth0` | = MR1 但 `interface: "eth0"` | |
| MR3 | interface 空串对照 | = MR1 但 `interface: ""` | |
| MB1 | E3 已证形态逐字重演 | 仅 `addresses: [{ address: { address: "192.0.2.1", family: 1 }, prefixLength: 32 }]`，无 routes、无 mtu、无 isBlocking（`spikes/e3-vpn-extension-physical-preflight-hap/entry/src/main/ets/vpnextensionability/E3PhysicalVpnExtensionAbility.ets:138-148`） | 兜底保留：无路由 tun fd 仍支撑 D5/D8/D7/D-W/D6；**MB1 保留时 U1 主字段固定 `unobservable(cause=mb1-no-route)`，另设 no-route 对照字段（见 D4），不得以零匹配伪答 U1** |

**MR1 说明（自表格外提，r4 第四趟 W1；内容逐字不变）**：

- `addresses` 网段/`routes` 覆盖/`mtu` 承接 n1b 冻结值的 create 面（`docs/n1b-gate-plan.md:47-49`）；**`interface`/`destination`/`gateway`/`hasGateway`/`isDefaultRoute` 为本 campaign 新增的 RouteInfo 必填字段候选**（d.ts 全必填，n1b 冻结值未含），取值为 DISC 起草选择。
- `family: 1` 显式写入（默认值即 1，显式冻结消除默认值依赖）；`port` 不设置（路由地址无端口语义）。

**字面值一致性（硬停止条款，MJ-10）**：MR 表字面即冻结字面（SDK d.ts 复核结果已写入「SDK 依据」节）。实现冻结时，实现代码中的配置字面与本文不一致 → **不得 freeze、不得 Live**；此后任何字面变更（含与 d.ts 再对照发现的差异）都构成判据修改，**必须重新提交跨厂商独立审查**——「差异写入 freeze 记录后继续」不成立。流程第 3 门静态审查逐字核对实现字面 == 本文字面。`VpnConfig.routes?` 本身可选、E3 已证仅 `addresses` 形态可 `create`（材料包勘误 F4，`docs/t0-n1b-discovery-materials.md:32`）。

### 矩阵执行协议（timeout 仲裁 / 迟到回调隔离，预注册）

1. **逐条串行**：任一时刻至多一个在途 `create()`。每条调用点同步注册 `then`/`catch`，60 s 时间盒由单调时钟计时（ArkTS 侧 `Promise.race` 或等价计时检查）。
2. **条目结局**：`resolved`（60 s 内 resolve，取得 fd）/ `rejected`（60 s 内 reject，登记拒绝文本）/ `timeout`（60 s 到点 Promise 未定）。
3. **迟到回调隔离**：所有已收口的条目，其后再到达的 resolve/reject 回调**只允许**发迟到 marker（`N1BDISC_D2_LATE|id=<id>|kind=<resolve|reject>`）
   并（若迟到 resolve 携带 fd）**只登记不关闭**：发 `N1BDISC_D2_LATE_FD|fd=<n>|at_mono_ms=<n>`，落盘 `late-fd-orphaned`（含 fd 号与出现时刻）——**禁止 P9 之前的任何 `destroy()` 调用，含迟到 fd**（理由见 first-accept-lock 条：destroy 可能使 `:vpn` 进程 terminal，矩阵内 destroy 会让 campaign 在中段死亡；迟到 fd 的关闭责任交给进程自然退出与 host finally 的 `ForceStop`，见「host finally cleanup」节）。
   迟到回调**不得**改写任何已收口条目的终态、**不得**驱动矩阵分支。
   并发仲裁依赖 ArkTS 单事件循环串行化 + 回调入口的状态机检查（非法迁移一律降级为迟到 marker）。
4. **timeout 处理（底层终态未确定 → 不得继续矩阵）**：任一条目 `timeout` → 矩阵**暂停**并进入**迟到观察窗 60 s（全矩阵恰一次）**。窗内该条目 resolve → 按 `late-resolved` 收口；若此时 first-accept 尚未发生，该条目即保留条目（fd 正常进入后续）。
   窗内 reject → 按 `late-rejected` 收口。窗尽仍未决 → `indeterminate` 收口。**三种结局均终止矩阵**：迟到窗结束后不再执行任何后续条目（含 MR1B——MR1B 仅在 MR1 以 rejected/late-rejected 收口且矩阵未终止时执行；timeout 及其后续结局下一律 `not_attempted`，u5 记 `unobservable(cause=matrix-terminated-on-create-timeout)`）。
   理由：timeout 已证底层 create 通道慢于预注册时间盒，同进程继续提交新 create 会引入并发悬挂与双 VPN 状态；时间盒是预注册参数，到点收口是其一致延伸。
5. **矩阵终局**：有保留条目（含 late-resolved）→ 进入「保留条目锁定序列」；无保留条目（全 rejected / timeout 终止 / 窗尽 indeterminate）→ **no-live-fd 分支**（见「无 fd / dup 失败分支」节）。
6. **每条目落盘**（含被拒条目）：`entry_id`、`attempted`、`create_outcome`（`resolved`/`rejected`/`timeout`→`late-resolved`/`late-rejected`/`indeterminate`）、`rejection_text`（经 chunk）、`fd_returned`、`u5_routeinfo_acceptance`，
   取值域闭合：`observed-true`（resolved/late-resolved）/ `observed-false`（rejected/late-rejected）/ `unobservable(cause=create-indeterminate)`（窗尽）/ `unobservable(cause=matrix-terminated-on-create-timeout)`（timeout 终止后的未执行条目）/ `unobservable(cause=protocol-first-accept-lock)`（accept-lock 后的未执行条目）。

**first-accept-lock 的理由与代价（正面登记）**：E3 已证 `:vpn` 子进程 destroy 后 terminal（`docs/n1b-gate-plan.md:63` 引 E3 先例）。若对「被接受但不保留」的条目执行 destroy 以腾位继续矩阵，而 destroy 恰使进程 terminal，campaign 将在矩阵中段死亡、七项事实全部损失。故本协议**冻结**：被接受条目一律保留、矩阵终止、**P9 之前不存在任何 `destroy()` 调用**（迟到 fd 亦不例外——它同样是矩阵内 destroy，同样可使进程 terminal 烧掉 campaign；其清理责任交给进程自然退出与 host finally 的 `ForceStop`）；
后续条目 u5 记 `unobservable(cause=protocol-first-accept-lock)`。代价：仅首 accept 条目**之前**的拒绝事实可采；`isBlocking:true` 效果只在 MR1 被拒的回退路径上可测。
此牺牲为设计选择，**须独立审查确认**（见起草人自陈）。

### 保留条目锁定序列（create resolve 回调内、同一 tick 依次执行，每步 marker）

| 步 | 动作 | marker | 落盘字段（三态或原文） |
| --- | --- | --- | --- |
| 2.1 | `fcntl(fd_orig, F_GETFL)` | `N1BDISC_D2_S1|ret=<n>|errno=<e>` | `d2_getfl_orig_initial`（原始 flags 基线，**任何 F_SETFL 之前**） |
| 2.2 | `fcntl(fd_orig, F_DUPFD_CLOEXEC, 0)`；失败则 `dup()`+`F_SETFD(FD_CLOEXEC)` 两步（路径逐字登记） | `N1BDISC_D2_S2|path=<f_dupfd_cloexec|dup_fallback>|fd=<n>|errno=<e>` | `d2_dup_path`、`fd_dup`、`d2_dup_cloexec`（含 errno） |
| 2.3 | `fcntl(fd_dup, F_GETFL)` | `N1BDISC_D2_S3|ret=<n>|errno=<e>` | `d2_getfl_dup_initial`（与 2.1 对照） |
| 2.4 | `u6` 判定：`O_NONBLOCK` 位在 `d2_getfl_dup_initial` 中 present/absent | `N1BDISC_D2_S4|u6=<o_nonblock_present|o_nonblock_absent|unobservable>` | `u6_initial_flags_and_isblocking_effect`（`o_nonblock_present` / `o_nonblock_absent` / `unobservable`——中性取值，不写预期极性）；`is_blocking_explicit`（该条目是否显式 true）；`u6_nonblocking_initial` 三态主字段与确定派生表——外提为下方「2.4 派生表」子项（r4 第四趟 W1 重构，内容逐字不变） |
| 2.5 | `fcntl(fd_dup, F_SETFL, O_NONBLOCK)` | `N1BDISC_D2_S5|ret=<n>|errno=<e>` | `d2_setfl_dup_ret/errno` |
| 2.6 | `fcntl(fd_orig, F_GETFL)`（OFD 副作用观察） | `N1BDISC_D2_S6|of=<observed-true|observed-false|unobservable>` | `of_nonblock_shared_to_orig`（三态，规则见 fd 纪律） |
| 2.7 | 登记 MTU oracle 先验 → 登记 `no-preregistered-oracle`（先验细节外提为下方「2.7 说明」子项，r4 第四趟 W1 重构，内容逐字不变） | `N1BDISC_D2_S7|prior=no-preregistered-oracle` | `mtu_api_oracle`（登记项）；`mtu_oracle_exists` 取值——外提为下方「2.7 说明」子项（r4 第四趟 W1 重构，内容逐字不变） |

**2.4 `u6_nonblocking_initial` 三态主字段与确定派生表（r4 第三趟 T3 新增；detail 极性值保留不变，本字段为 schema 要求的三态主字段，派生逐格冻结；r4 第四趟 W1 自表格外提，内容逐字不变）**：

- `o_nonblock_present` → `observed-true`（`F_GETFL` 基线的 status flags 含 `O_NONBLOCK` 位——fd 初始为非阻塞）；
- `o_nonblock_absent` → `observed-false`（基线不含 `O_NONBLOCK` 位——fd 初始为阻塞）；
- `unobservable`（marker `N1BDISC_D2_S4` 缺失或对应 skip 分支）→ `unobservable(cause=…)`（cause 沿「无 fd / dup 失败分支」表：`no-live-fd` / `dup-failed` / 进程死亡路径按「死亡事实记录（证据向量）」节七分量登记收口——r9：原「沿死因分类收口」随死因表删除改写，不合成归因、不驱动 fail，除非独立命中 F1 闭集）。

派生表对 `u6_initial_flags_and_isblocking_effect` 取值域全域恰有唯一落点（总函数）。

**2.7 说明（r4 第四趟 W1 自表格外提，内容逐字不变）**：

- `VpnConfig` 无 MTU 查询字段；`ConnectionProperties.mtu`（`@ohos.net.connection.d.ts:2221`，经同文件 `:248`/`:263` 的 `getConnectionProperties` 或 `:278` 的 `getConnectionPropertiesSync` 取得）**存在**，但本 campaign **未预注册**其为 oracle（NetHandle 归属与是否回显提交值均未测）。
- `mtu_api_oracle`（登记项）；`mtu_oracle_exists` 在本 campaign 恒 `unobservable(cause=no-preregistered-oracle)`（见 D8a——写返回分界不构成 oracle，本 campaign 未预注册任何可区分 1280/1400 的 oracle）。

每条目矩阵阶段 marker：`N1BDISC_D2_ENTRY|id=<id>|phase=attempted` 先行；
结局 marker `N1BDISC_D2_ENTRY|id=<id>|outcome=<resolved|rejected|timeout|late-resolved|late-rejected|indeterminate|not_attempted>|fd=<n|none>`；
拒绝文本经 `N1BDISC_D2_REJTEXT` chunk 族（字面冻结，r3 第二趟 E3：即上节 `N1BDISC_CHUNK` 通用字面施加 `stream=rejtext|item=<该条目 id>`——`MR1`→`item=0`、`MR1B`→`1`、`MR2`→`2`、`MR3`→`3`、`MB1`→`4`，与 `N1BDISC_D2_ENTRY` 的 `id` 同一编号域；无独立 REJTEXT 专用字面；**防误引注，r9：`N1BDISC_D2_REJTEXT` 自 r4 第二趟 S4 起不在 A5 冻结集内，本处为通用 `N1BDISC_CHUNK` 字面加 `stream=rejtext` 参数的简写指称，非独立专用字面，A5 字面清点不得计入冻结集比对**）。

## D 项协议（预注册）

以下每项均可机器求值：执行者（Extension 进程内探针）、位点（全序见后节）、观察字段、marker、时间盒与超时分类均已冻结。**全部平台事实三态登记、不设任何 pass 条件。**

### D1 库加载（arm64 候选证据）

- **执行者/位点**：Extension 进程 native 探针；`onCreate` 后、任何 `create()` 之前。
- **动作与 marker**：`N1BDISC_D1_BEGIN` 先行 → `dlopen` 唯一 arm64-v8a native 成员 → 成功发 `N1BDISC_D1_LOADED|so=<member-name>`，失败发 `N1BDISC_D1_FAIL|err=<dlerror>`（经 chunk）→ `dlsym` 逐名解析冻结符号清单 → `N1BDISC_D1_SYM|total=<n>|resolved=<n>` → `N1BDISC_D1_END|load=<state>` 收口。
  **冻结符号清单（14 个，逐字冻结，本判据即完整机器输入）**：`x25519_secret_key`、`x25519_public_key`、`x25519_key_to_base64`、`x25519_key_to_hex`、`x25519_key_to_str_free`、`check_base64_encoded_x25519_key`、`set_logging_function`、`new_tunnel`、`tunnel_free`、`wireguard_write`、`wireguard_read`、`wireguard_tick`、`wireguard_force_handshake`、`wireguard_stats`。
  **来源**：主会话在 crate 源码 `boringtun-0.7.1/src/ffi/mod.rs` 逐个核实的全部 `#[no_mangle] extern "C"` 导出（按签名行序；crate checksum 已在「构建输入」节冻结，清单与该锁定源码绑定）。
  N1b r1 C1 的「七个 ffi 入口」表述为不完整先例，`docs/n1b-gate-plan.md:168`，本门不得复用。freeze 时静态审查仍须复核本清单与 checksum 锁定源码逐 `#[no_mangle]` 一致（不一致即不得 freeze）。
- **落盘与取值域**：`d1_load` ∈ {`observed-true`（dlopen 句柄非空）/ `observed-false`（dlopen 返回 NULL，`d1_dlerror` 登记原文）/ `unobservable`（D1 未执行或进程死于 D1）}——**部分 `dlsym` 失败不降级 `d1_load`**（加载与符号解析分立），符号事实由 `d1_symbols_total`、`d1_symbols_resolved`、`d1_symbols_unresolved_list` 承载。
  另落盘：`d1_so_member`、`d1_so_sha256`、`d1_pid`、`d1_cmdline`、`d1_process_model=vpnextension`、`process_model_mismatch`（r4 第三趟 T5 新增观察事实：`d1_cmdline` 与预注册名不符时置位并逐字登记实际值；见下条，不单独构成 fail）。
- **`d1_cmdline` 进程模型核验（r4 第三趟 T5 重写，与证据规则 2 的 `PidOfVpn` 出口对齐）**：判定规则逐字冻结——(a) `d1_cmdline` 含预注册名 `<bundle>:vpn` → 核验通过，无事；
  (b) 不含该精确名、但 `d1_cmdline` 为 `<任意前缀>:vpn` 形态（即进程为某 ExtensionAbility 的 `:vpn` 扩展进程实例）→ 进程模型前提仍成立：**逐字登记实际值**、置位观察事实 `process_model_mismatch`，**不 fail**——进程命名差异本身可以是平台发现事实，命名细微差异（如 bundle 前缀截断形态、命名与预期不同）照登；
  (c) `d1_cmdline` 不含 `:vpn` 后缀（如落在 UI 主进程或其他非 Extension 进程）→ `process_model != vpnextension`，campaign 的执行位点前提不成立 → verdict `fail`（完整性轴：未按预注册位点执行）。本条与证据规则 2 的「`PidOfVpn` 不可观测不得单独 fail」同一法理：观察事实与完整性 fail 分立，只有执行位点前提本身失效才构成完整性失败。
- **时间盒/超时分类**：10 s。dlopen 错误返回 → `d1_load=observed-false`，**协议继续**（负面事实不停止；决议 §4.1 结局不对称论证）；未返回且进程死亡 → 走死亡收口路径（死亡事实按「死亡事实记录（证据向量）」节七分量登记，r9：原「死因分类见 verdict 节」随死因表删除改指；r5 U1 前移后：P1 位于 P5T 之前，此时 PRE 尚未发射 → 按「PRE 缺失」完整性失败收口 `fail`；
  死亡侧 fail 触发器为 F1（`probe_crash_signature_observed = observed-true` 且 `protocol != complete`，见 verdict 节 r9 条，其非 `complete` 作用域覆盖原 r5 U3 的 pre-only 限定——r9：原「『死因分类 fail 触发器』仅适用于 P5T 之后的 pre-only 收口，见 verdict 节 r5 U3」随死因表删除改写）。
- **用途限定**：D1 事实仅作基础决议 §二.8（`docs/native-nx-governance.md:44`）arm64 同核心加载证据的**候选引用**，不得写成「N2b 加载前置已满足」；N1b 的 C1 仍须执行（决议 §4.3.3，`docs/native-nx-n1b-adjudication.md:126`）。

### D4 socket→tun 投递（U1）+ 首帧 dump（U3）

- **前置**：存在保留条目与 `fd_dup`；缺失分支见「无 fd / dup 失败分支」节。
- **冻结受控包身份**：探针自有 `AF_INET/SOCK_DGRAM` socket（`d4_send_socket`）`sendto` 冻结目的——MR* 保留时 `10.99.0.2:47001`（路由覆盖内）；MB1 保留时 `192.0.2.2:47001`（无路由对照，预期不入 tun，事实照登）。发送 payload 16 B，逐字节冻结：`"N1DISCD4"(8 B magic) | round(1 B)=0x01 | seq(2 B，大端，自 1 递增) | pad(5 B)=0x5A`。**包身份 = {目的地址, 目的端口 47001, proto=UDP(17), payload 16 B 逐字节}**——src 侧不设匹配条件（源端口为内核分配，避免引入 `getsockname`）；身份由 payload 承载。
- **动作**：共 ≤20 次发送，每次后 `poll(fd_dup, POLLIN, 500ms)` + 非阻塞 `read`。
- **匹配判定（双偏移）**：每次成功 read 的帧，对 offset-0 与 offset-4 **各自**尝试解析 IPv4（version=4、IHL=5）；任一 offset 解析成功且 proto=17、目的地址=发送目的地址、dport=47001、UDP payload 与某次已发送包的 16 B 身份逐字段相等 → 该 read 为**匹配受控包**。`u1_match_offset` ∈ {`0` / `4` / `both` / `none`}（登记首个匹配帧的可用 offset 集合）。
- **U1 判定**：**前提**——至少一次 `sendto` 返回 `n==16`；全部 sendto 返回 `-1` → `u1=unobservable(cause=send-failed)`（errno 逐次登记）；存在返回 `0` 或部分正值（`0<n<16`）但**从无一次完整成功（`n==16`）** → `u1=unobservable(cause=short-or-zero-io)`（逐次返回值原文逐字登记），**均不得**赋 true/false（构包/调用缺陷不得伪装成平台「不投递」）。
  前提成立时：窗口内出现匹配受控包 → `observed-true`（任一 offset 匹配即 true——前缀形态的 offset 差异不得系统性制造假阴性，与 `u3_pi_header_present` 的闭合枚举口径一致）；发送与窗口（总 10 s）耗尽且零匹配 → `observed-false`；无 fd/进程死亡 → `unobservable`。
  **MB1 保留时 U1 主字段固定 `unobservable(cause=mb1-no-route)`**（无路由是协议性预期，零匹配不构成「不投递」平台事实）；另设 `u1_no_route_control`（对照观察，不设预期）。
  **完整求值表（r3 第二趟 E9 冻结，与 U1 主字段同一门槛法理）**：`observed-true` —— **前提：至少一次 `sendto`（目的 `192.0.2.2:47001`）返回 `n==16`**，且窗口内出现一帧满足完整包身份匹配（按 U1 匹配判定：offset-0/offset-4 任一解析成功 + proto=17 + dport=47001 + UDP payload 与某次已发送 16 B 身份逐字段相等）；
  `observed-false` —— sendto 前提成立且发送与窗口（总 10 s）耗尽而零匹配帧；
  `unobservable(cause=short-or-zero-io)` —— 存在返回 `0` 或部分正值（`0<n<16`）但从无一次完整成功（`n==16`）（逐次返回值原文逐字登记）；
  `unobservable(cause=send-failed)` —— 全部 sendto 返回 `-1`（errno 逐次登记）；
  `unobservable(cause=no-live-fd)` / `unobservable(cause=dup-failed)` —— 对应 skip 分支（见「无 fd / dup 失败分支」表）；短写/零写一律收口 `unobservable(cause=short-or-zero-io)`，不得赋 true/false（构包/调用缺陷不得伪装成「不投递」）。
- **U3 判定（决定性数据来源 = 首个匹配受控包的成功 read）**：dump 对象、`u3_first_read_len`、`u3_first64_hex`（前 64 字节十六进制，经 chunk 通道落盘）。**零匹配（含窗口耗尽、MB1 保留）→ U3 全部 `unobservable(cause=no-controlled-read)`**；外来包不作为 dump 对象。
  `u3_pi_header_present` 改为闭合枚举（r1 更正：冻结预设常量 `00 00 00 04` **删除**——SDK 实测 Linux `tun_pi` 为 `struct tun_pi { __u16 flags; __be16 proto; }`（`native/sysroot/usr/include/linux/if_tun.h:76-79`），`ETH_P_IP = 0x0800`（`native/sysroot/usr/include/linux/if_ether.h:36`），带 PI 的 IPv4 帧前 4 字节典型为 `00 00 08 00`；「任意 4 字节前缀」与 `tun_pi` 形态分开分类，不预设任何常量）：
  `tun_pi-like`（前 2 字节 flags 与后 2 字节大端 proto=0x0800，且 offset-4 处解析出 IPv4 version=4）/ `other-prefix`（offset-4 解析出 IPv4 但前 4 字节不符 tun_pi 形态——**逐字登记该 4 字节原文**）/ `no-prefix`（offset-0 即解析出 IPv4）/ `ambiguous`（offset-0 与 offset-4 均解析出 IPv4）/ `unparsable`。
  **判定分区（r4 第二趟 S5 重写，取代 r3 第二趟 E4 的「枚举优先级自上而下首个命中」序——旧序下 both-offset 输入先命中 `tun_pi-like`/`other-prefix`/`no-prefix`，`ambiguous` 被截胡，与同段强制 selftest「须定 `ambiguous`」矛盾，实现无法同时满足）**：先按「offset-0 可解析 / offset-4 可解析」做互斥分区，再在分区内定类，分区与判定逐格冻结如下——

| offset-0 可解析 | offset-4 可解析 | 判定 |
| --- | --- | --- |
| 否 | 否 | `unparsable` |
| 是 | 否 | `no-prefix` |
| 否 | 是 | 前 4 字节符合 tun_pi 形态（前 2 字节 flags + 后 2 字节大端 proto=0x0800）→ `tun_pi-like`；否则 → `other-prefix`（**逐字登记该 4 字节原文**） |
| 是 | 是 | `ambiguous`（**无条件**——无论前 4 字节形态如何，`ambiguous` 不再被任何类截胡） |

上表是**互斥完备分区**：四行穷尽 2×2 输入域、行间输入互斥，恰有唯一落点（保持总函数性）；无优先级序、无「首个命中终止」。
  selftest 须覆盖分区表全部四行（含构造输入：offset-0 与 offset-4 均可解析且前 4 字节 = `00 00 08 00` 的帧 → **须定 `ambiguous`**——旧 E4 期望 `tun_pi-like` 的用例随 S5 废除；仅 offset-4 可解析且前 4 字节 `00 00 08 00` → `tun_pi-like`；仅 offset-4 且前 4 字节非 tun_pi 形态 → `other-prefix`；仅 offset-0 → `no-prefix`；均不可 → `unparsable`）。
  `u3_readlen_vs_total_length` ∈ {`equal` / `readlen>total_length` / `readlen<total_length` / `unparsable` / `unobservable`}。
  **求值 offset 冻结（E4）**：按该匹配帧在 U1 匹配判定中的可用 offset 求值；`off=both` 时**决定性 offset = offset-4**（理由冻结：offset-4 是 IPv4 数据报的真实起点，`total_length` 与 readlen 的关系只有在排除/计入 PI 前缀的单一口径下可解释；取 offset-4 使「含 4 B 前缀 → readlen = total_length + 4」的结构性差异显式落入 `readlen>total_length`，不被 offset-0 口径吞掉）；
  同时另行登记观察字段 `u3_readlen_vs_total_length_off0`（offset-0 口径的同构五值，纯观察、不参与 U3 判定），使 off=both 的两套口径均有据可查。
- **`u3_prefix_format` 三态主字段与确定派生表（r4 第三趟 T3 新增；detail 枚举 `u3_pi_header_present` 保留不变，本字段为 schema 要求的三态主字段，派生逐格冻结——决定性数据来源与 U3 判定同一）**：
  `tun_pi-like` → `observed-true`（首个匹配受控包 read 的帧含 tun_pi 形态 4 字节前缀，PI 头存在）；`no-prefix` → `observed-false`（帧无前缀，PI 头不存在）；
  `other-prefix` → `observed-false`（有 4 字节前缀但非 tun_pi 形态，PI 头不存在——前缀原文已按分区表逐字登记）；`ambiguous` → `unobservable(cause=prefix-ambiguous)`（offset-0 与 offset-4 均可解析，前缀归属不可判定，不得猜判 true/false）；`unparsable` → `unobservable(cause=frame-unparsable)`；零匹配（含窗口耗尽、MB1 保留）→ `unobservable(cause=no-controlled-read)`（与 U3 全字段同口径）；
  无 fd/dup 失败/进程死亡 → `unobservable`（cause 沿「无 fd / dup 失败分支」表）。上表对 `u3_pi_header_present` 闭合枚举全域恰有唯一落点（总函数）。
- **外来包**：窗口内读到的不匹配包计入 `foreign_packets_observed`（计数 + 首包前 64 字节十六进制，观察字段）。
- **时间盒/超时分类**：总 10 s、每 poll 500 ms；到点即按三态收口，超时不是 fail。
- **marker**：`N1BDISC_D4_BEGIN` / `N1BDISC_D4_SENT|n=<k>|ret=<n>|errno=<e>` / `N1BDISC_D4_READ|len=<n>|off=<0|4|both|none>` / `N1BDISC_D4_END|u1=<state>`。

### D5 tun 写入 → sink（U2）

- **前置**：同 D4。
- **冻结合法包（逐字节布局，总长 44 B，不填充）**：IPv4 头 20 B——version=4、IHL=5、TOS=0、`total_length=44`、id=0x0001、flags=0、frag_off=0、TTL=64、proto=17、`checksum`=对头 20 B 逐 16-bit 字反码和重算（含 total_length；奇数字节补零字节；结果网络序写回）、src=`10.99.0.2`、dst=`10.99.0.1`；UDP 头 8 B——sport=47001、dport=47002、`length=24`、`checksum=0x0000`（IPv4 合法，避免校验和实现依赖）；
payload 16 B——`"N1DISCD5"(8 B) | round(1 B)=0x01 | seq(2 B，大端，自 1 递增) | 0x5A×5`。
MB1 保留时 src=`192.0.2.2`、dst=`192.0.2.1`，其余同。全部多字节字段网络序（大端）。**不经 BoringTun**。
- **冻结 sink**：`d5_sink_socket` = `AF_INET/SOCK_DGRAM` bind `0.0.0.0:47002`（沿 `docs/n1b-gate-plan.md:57` 先例）；注入后 `poll(POLLIN, 500ms)` + `recvfrom`。
- **动作**：`poll(fd_dup, POLLOUT, 500ms)` 就绪后 `write(fd_dup, pkt, 44)`；共 ≤5 轮（每轮写后等待 sink 500 ms，总 10 s）。
- **U2 判定**：**前提**——至少一轮 `write` 返回 `n==44`；全部写返回 `-1` → `u2=unobservable(cause=write-failed)`（errno 逐轮登记）；存在返回 `0` 或部分正值（`0<n<44`）但**从无一次完整成功（`n==44`）** → `u2=unobservable(cause=short-or-zero-io)`（逐轮返回值原文逐字登记）——三者均不得赋 true/false。
  前提成立时：任一轮 `recvfrom` 收到包且**身份核对逐字节通过**（r3 第二趟 E10 收紧：① `recvfrom` 源 = 冻结 src 地址:47001；② payload 恰 16 B 且与**本轮**发送包的冻结 16 B 身份**逐字节相等**——magic `"N1DISCD5"` 8 B、`round` 字节与本轮 `round` 值一致、`seq` 2 B 大端与本轮发送 seq 一致、填充 5 B 恰为 `0x5A`；缺一不可，防损坏包或上一轮旧包被判 delivery true）
  → `observed-true`；轮数与窗口耗尽且零收到 → `observed-false`；无 fd → `unobservable`。
  逐轮登记 write 返回分类：`n==len` / `0<n<len`（部分写，计数 `partial_write_count`）/ `-1+errno`。
- **时间盒/超时分类**：总 10 s；到点即三态收口。
- **marker**：`N1BDISC_D5_BEGIN` / `N1BDISC_D5_WRITE|round=<r>|ret=<n>|errno=<e>` / `N1BDISC_D5_RECV|round=<r>|src=<addr>` / `N1BDISC_D5_END|u2=<state>`。

### D8a MTU 写返回谱（阶梯）+ D8b 背压/部分写可诱发性（缩减后置）

- **前置**：同 D4。
- **D8a 阶梯（每级逐字节冻结）**：级长 L ∈ {128, 512, 1024, 1200, 1280, 1352, 1400, 1401, 1480, 1500}。每级：IPv4 头 20 B——version=4、IHL=5、TOS=0、`total_length=L`、id=级序号（1..10，网络序）、flags=0、frag_off=0、TTL=64、proto=17、`checksum` **随长度逐级重算**（total_length 变化即重算，覆盖范围 = IPv4 头 20 B）、src/dst 同 D5；
UDP 头 8 B——sport=47001、dport=47002、`length=L-20`、`checksum=0x0000`；payload `L-28` B——前缀 `"N1DISCD8"(8 B) | len(2 B，大端 = L)` + `0x5A` 填充至 `L-28`。每级一次 `poll(POLLOUT, 500ms)`+`write(fd_dup, pkt, L)`，**write 长度 = L = total_length**。
ret 分类表（逐级登记）：`n==L` / `0<n<L` / `-1/EAGAIN` / `-1/EMSGSIZE` / `-1/<其他>`（errno 值逐个登记）。
- **D8a 落盘（降格，BL-10）**：`d8_write_boundary_last_success_len` ∈ {`none`（全级失败）/ `128` / `512` / `1024` / `1200` / `1280` / `1352` / `1400` / `1401` / `1480` / `1500`（全级成功）/ `unobservable`（无 fd 或阶梯未完成）}——登记「最后一级 `n==L` 成功的长度」。
MR* 保留时派生 `write_return_boundary_consistent_with_1400`：last_success=`1400` 且 1401 级 ret ∈ 失败类 → `observed-true`；其余（last_success 为其他值、`none`、`1500`）→ `observed-false`；`unobservable` 同源。
**该字段只登记写返回谱与冻结 mtu=1400 的字面一致性，不主张实际 MTU、不主张存在或不存在 MTU oracle。** MB1 保留（未声明 mtu）时 `write_return_boundary_consistent_with_1400=unobservable(cause=mb1-no-declared-mtu)`，`d8_write_boundary_last_success_len` 照常登记（默认行为观察）。
- **`mtu_oracle_exists` 恒 `unobservable(cause=no-preregistered-oracle)`**：只有经预注册、可证区分 1280/1400 且排除替代原因的 oracle 才可赋值；`ConnectionProperties.mtu`（`@ohos.net.connection.d.ts:2221`，经 `getConnectionProperties`）**存在**，但本 campaign 未预注册其为 oracle（NetHandle 归属与是否回显提交值均未测），写返回分界亦不满足排除替代原因的要求（外来干扰/缓冲实现均可造成分界）。若 freeze 前新增预注册 oracle，须重新独立审查。
- **D8b storm（缩减 + 后移）**：预注册上限 = **10 s 墙钟**（单调时钟）、累计 **4 MiB**、**50 000 次** `write`；负载 = 冻结 1024 B 合法 IPv4 包（布局同 D8a 的 L=1024 级，**id 固定 11 不递增、checksum 按该头一次性重算后复用**——r1 更正：IPv4 头校验和覆盖整个 20 字节头，Identification 字段位于字节 4-5，**在**校验和覆盖范围内，「id 递增而校验和不变」的旧表述错误；本 campaign 选择的分支是**固定 id 不递增**，故全部 storm 包校验和同一）；
持续写 `fd_dup` 直至首次 `-1/EAGAIN` 或熔断。落盘：`eagain_observed`（三态）、`partial_write_observed`（三态）、`bytes_written_total`、`write_calls`、`window_start/end_monotonic`（r10：两钟分别由 `D8_STORM_BEGIN` 的 `ws=<n>` 与 `D8_STORM_END` 的 `we=<n>` 承载——窗口开始钟于 storm 起点已可得、窗口结束钟于 END 发射时已可得，capture 可重建）、`caps_hit`。
**熔断是保险丝不是验收路径**：命中即停并登记，不构成 fail、不构成 blocked（沿 N1a C5 法理，`docs/n1a-gate-plan.md:32`）。**措辞口径（决议 §二.2 原句，`docs/native-nx-n1b-adjudication.md:65`）**：未诱发 EAGAIN 时 storm 结论措辞**固定**为 `attempted, not induced on this fd`；`not-triggered` 记录必须携带定量参数（累计字节、写调用次数、时间盒起止单调时钟；r10：时间盒起止单调时钟的载体 = `D8_STORM_BEGIN.ws` / `D8_STORM_END.we`），缺失按字段缺项 → fail。
**位次理由（BL-9 二选一，选「缩减后移」）**：保留 storm 因 OB-01/OB-02 的事实输入价值（eagain/partial 可写性直接影响 N1b r2 写路径设计）；移至 D7 之后（P7）使 watchdog 高价值事实（U7）先落盘，storm 若压死进程，损失集缩小为 D-W/D6（D8b 自身字段按下方死亡收口两支（BEGIN-only / 阶段未达）落值，不因 END 缺失 fail——r12）——与既有「位次最后」论证同构；
缩减至 10 s/4 MiB/50k（约原 1/3~1/4）降低压死概率，且该量级仍远超 N1b 实际写模式需求。
- **D8b BEGIN-only 死亡收口（r12，blocker 4；sol B-04 = grok M-02 加深）**：触发三条件同时成立——`N1BDISC_D8_STORM_BEGIN`（含 `ws=<n>`）在 capture、`N1BDISC_D8_STORM_END` 缺、`process_death_observed = observed-true`（死亡分量，r12 blocker 3 统一后的唯一死亡谓词源，见「死亡事实记录（证据向量）」节）——storm 中途进程被平台终止（如 watchdog 压死，合法平台终态）、END 未及发射：
  - **落值**：`window_end_monotonic` 与 END 承载的 5 个字段（`eagain_observed`/`partial_write_observed`/`bytes_written_total`/`write_calls`/`caps_hit`）各自记 `unobservable(cause=storm-incomplete-pre-only)`——**新具名 cause，一条 cause 覆盖本组六项**（共享同一物理原因：storm 中途进程终止、END 未发射，不为每项另立 cause）；`window_start_monotonic` 不在本支管辖内，由 BEGIN 的 `ws=<n>` 照常重建（不受 END 缺失影响）。
    落值由 runner 依 capture 重建收口（探针已死，沿 pre-only 既有机制登记入 evidence 记录、不代发探针 marker）。
  - **明确不触发字段缺项 fail**：本支的 END 缺失是死亡路径的必然形态、不是缺项——verdict 节「任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）」（F4 面）对本支不适用：各字段已有预注册 cause 落值；「单调钟派生字段绝对域门」对 `window_end_monotonic` 同理无数值可查（unobservable 落值非负值输入，不挂 F8，见该门 r12 界定条）。与「`not-triggered` 记录必须携带定量参数……缺失按字段缺项 → fail」句的分界：该句管辖 END 已发的正常收口形态，本支 END 未发、不在其域内。
  - **跨度登记**：本支触发时置观察布尔 `storm_incomplete_pre_only=true`（探针已死，由 runner 依 capture 与死亡证据登记入 evidence 记录），并逐字登记 BEGIN 的 `ws=<n>` 原值与死亡证据墙钟、登记跨度数值（登记项 `storm_incomplete_pre_only_span`，供 N1b 参考；跨度以同侧墙钟计——死亡证据墙钟 − BEGIN marker 的 capture 墙钟，取材与减法方向沿 `marker_tail_state` 既有规则；`ws` 为单调钟，与死亡证据墙钟不同源，**禁止跨钟相减**）。
  - **对照（不触发本支）**：BEGIN 在、END 缺、但 `process_death_observed != observed-true`（进程仍活或状态不可判）→ 无死亡证据即非死亡终态，本支不触发、各字段不得取 `storm-incomplete-pre-only`——storm 三熔断上界到期本应发 END 而未发，属探针未完成：字段缺项沿既有 F4 面、观测窗到点进程仍活挂 F9（真正的未完成，不是平台终态；selftest ③ r12 对照例）。
- **D8b 阶段未达死亡收口（r12，r12-P3 施工中发现；r13 注：同一发现，标签统一为 r12-P3 施工发现，原写「P3 范围外」）**：触发两条件同时成立——`process_death_observed = observed-true`（死亡分量，同上方唯一死亡谓词源）且死亡位点（证据规则 4）在 `N1BDISC_D8_STORM_BEGIN` 之前（D8b 未开始，如死于 D7 或更早）——进程死于 storm 起点之前、D8b 全部落盘字段无任何输入：
  - **落值**：D8b 全部 7 个落盘字段（`window_start_monotonic`、`window_end_monotonic`、`eagain_observed`、`partial_write_observed`、`bytes_written_total`、`write_calls`、`caps_hit`）各自记 `unobservable(cause=stage-not-reached)`——**新具名 cause**；
    与 `dw_*`/u4 死亡收口的 `destroy-not-reached`（PRE/POST 求值规则 (2) r12 分流条款）**语义平行**（同为「进程死于本项位点之前」的合法平台死亡收口），**不跨域共用字面**——`destroy-not-reached` 字面语义限 destroy 位点域（P9），本组是 D8b storm 字段域、未达位点是 storm 起点（P7），沿上方跨域注记格式注明平行关系、防近义字面误引；
    本支连 BEGIN 亦缺、`window_start_monotonic` 无 `ws` 可重建——与 BEGIN-only 支（`ws` 照常重建）就此分界；落值由 runner 依 capture 与死亡证据登记入 evidence 记录，同 BEGIN-only 支机制、不代发探针 marker。
  - **明确不触发字段缺项 fail**：verdict 节「任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）」（F4 面）对本支不适用（各字段已有预注册 cause 落值，同 BEGIN-only 支法理）；单调钟绝对域门对两钟同理无数值可查（unobservable 落值非负值输入，不挂 F8）。
  - **布尔与跨度**：本支 `storm_incomplete_pre_only=false`、不登记 `storm_incomplete_pre_only_span`——沿既有布尔置位惯例（触发才置位、否则 false），且该布尔的域限语义是「BEGIN 在而 END 缺」的 storm 未完成形态，D8b 未开始不属 storm 未完成；落值 cause `stage-not-reached` 本身即两支的区分标记（防把「未开始」误读成「已开始未完成」）。
  - **对照（不触发本支）**：无 BEGIN 且 `process_death_observed != observed-true`（进程仍活）→ D8b 未执行属真正的未完成（协议未到 P7 而进程存活）：字段缺项沿既有 F4 面、观测窗到点进程仍活挂 F9（同 BEGIN-only 支对照的法理）。
- **marker**：`N1BDISC_D8_MTU|len=<n>|ret=<n>|errno=<e>` 逐级 / `N1BDISC_D8_STORM_BEGIN|ws=<n>`（r10 新增字段 `ws`：窗口开始单调钟，发射时已可得）/ `N1BDISC_D8_STORM_END|eagain=<s>|partial=<s>|bytes=<n>|calls=<n>|caps_hit=<b>|we=<n>`（r10 新增字段 `we`：窗口结束单调钟，发射时已可得）。

### D7 live watchdog（U7）

- **执行者/位点**：Extension 进程、**VPN 仍 live 时**（destroy 之前）。决议 §4.3.1 将 watchdog 观察**重排至 destroy 之前**：若任务触发 watchdog 杀进程，此时 D1-D5、D8a 事实已落盘，且杀因不与 destroy 副作用混淆。**前置 = 保留条目存在（live VPN）**；不要求 `fd_dup`（dup-failed 分支下照常执行）。no-live-fd 分支下 skip（无 live VPN，U7 不得赋 true/false）。
- **冻结负载（伪码与常量逐字冻结，MJ-12）**：

```
buf: [u8; 4096]，初始全 0x00；x: u64 = 0；j: u64 = 0
sink: 经 volatile 写（std::ptr::write_volatile 或等价）防止死码消除
start_ms = clock_gettime(CLOCK_MONOTONIC)（毫域）；deadline_ms = start_ms + 20000
loop {                                   // 外层块：每块恰一次 clock_gettime
  for k in 0..4096 {                     // 内层固定 4096 次迭代，零 syscall
    off = (j * 8) mod 4096
    v   = u64 小端加载 buf[off..off+8]
    x   = x.wrapping_mul(2654435761).wrapping_add(v)     // 冻结混合乘数（Knuth）
    u64 小端写回 buf[off..off+8] = x
    j += 1
  }
  volatile_sink_write(x)                            // 反优化 sink
  clock_nanosleep(CLOCK_MONOTONIC, 50ms)            // 清单内 syscall；读钟频率下限 50 ms/次的保证来源
  now_ms = clock_gettime(CLOCK_MONOTONIC)           // 全任务唯一读钟位点：每外层块恰一次
  if now_ms >= deadline_ms { break }
}
```

**读钟频率上界的实现方式（冻结）**：`clock_gettime` 仅出现在外层块尾、每块恰一次；外层块内含 50 ms `clock_nanosleep`，故读钟间隔 ≥ 50 ms 由 sleep 保证、**不依赖内层块执行时长**——读钟频率上界 = 20 Hz，机器可静态核对（`clock_gettime` 调用点唯一 + sleep 常量 50 ms）。任务内 syscall 仅 `clock_gettime`（≤20 Hz，如上）与 `clock_nanosleep`（仅 `CLOCK_MONOTONIC`、50 ms、本处 D7 时钟门控使用，在「探针 syscall 面」清单授权范围内）；无任何 fd/锁/分配操作。
- **落盘**：`N1BDISC_D7_BEGIN|dur_ms=20000|load=frozen-int-mix-4k|start_mono_ms=<n>` 先行（`start_mono_ms` = 任务起始单调钟读数，r4 第一趟 R4 新增）；结束后 `N1BDISC_D7_END|iters=<n>|elapsed_ms=<n>`。`elapsed_ms` = 末次读钟值 − `start_ms`。**接受域 = 互斥区间（r3 第二趟 E5 冻结，区间无缝无叠、正文与自陈逐字同一）**：

| `elapsed_ms` 区间 | 判定 |
| --- | --- |
| `[0, 20000)` | **矛盾输入 → fail（F8）**（r10，B B-08）：`D7_END` 在而 `elapsed_ms` 与伪码出口条件机械矛盾——循环只在首次读到 ≥ deadline 时才 break，合法值不可能 < 20000，属正向探针控制流缺陷（与负 elapsed 同类，沿单调钟域门「记录器未尽职，不是死因」口径）→ **fail（F8）+ 原值逐字入档**；`d7_anomaly=early-exit-with-end-marker` 字面保留为登记标签、**该标签同时驱动 F8**，`u7=unobservable(cause=d7-early-exit-anomaly)` |
| `[20000, 25000]` | 合法取值域（`[20000, 20000+外层块时长)`，外层块时长 ≪ 5 s 宽限）→ `observed-true` 方向（任务存活跑完） |
| `> 25000` | **具名三态观察，不得判 fail**（超出 5 s 宽限的部分可由平台调度延迟造成，属平台事实而非探针缺陷）：逐字登记 elapsed/iters 原文，`u7=unobservable(cause=d7-elapsed-overshoot-beyond-grace)`，登记 `d7_anomaly=elapsed-overshoot-observed`（观察项） |

边界归属（消除旧文「`>= 20000` 判 true 与「超过 25000」探针异常在 `>= 25000` 处双命中」的叠域）：`25000` 恰值归 `[20000, 25000]` 区间。**平台调度延迟造成的边界值不得触发 fail**；本表只管辖 D7_END 存在（任务收口）路径，D7 期间进程死亡按「死亡事实记录（证据向量）」节七分量逐项登记（其 D7 位点约束见该节，不受本表改写；r9：原「走死因分类」随死因表删除改写——不合成归因、不驱动 fail，除非死亡证据独立命中 F1 三支闭集）。
`u7_long_task_watchdog_behavior` 三态赋值规则（与上表互斥区间逐字同源）：`D7_END` 存在**且** `elapsed_ms ∈ [20000, 25000]` → `observed-true`（任务存活跑完；同时登记 elapsed/iters）；
`D7_END` 存在但 `elapsed_ms < 20000` → **不得判 `observed-true`，且矛盾输入 fail（F8）**（r10，B B-08）——marker 在而 `elapsed_ms` 与伪码出口条件（只在首次读到 ≥ deadline 才 break）机械矛盾，属正向探针控制流缺陷、与负 elapsed 同类（r9「仅记观察不 fail」使该缺陷 fail-open，随本条收口；措辞沿单调钟域门「记录器未尽职，不是死因」口径）：elapsed/iters 原值逐字入档，`d7_anomaly=early-exit-with-end-marker` 标签同时驱动 F8，`u7` 记 `unobservable(cause=d7-early-exit-anomaly)`；
（r10 与其他门的关系：若同时 `elapsed_ms < 0`，单调钟域门（非负性）先命中——两门同挂 F8、同 verdict，不冲突；本条只针对 `0 ≤ elapsed_ms < 20000` 区间）；
`D7_END` 存在但 `elapsed_ms > 25000` → 按上表第三行：`unobservable(cause=d7-elapsed-overshoot-beyond-grace)`，不判 fail；`D7_END` 缺失 **且**（capture 流确认 `D7_BEGIN` 后 `:vpn` 三形态行静默（至死亡位点或观测窗尾，以先到者为准）**且** 存在进程死亡证据）→ `observed-false`（任务被杀；死亡证据口径 r10 收窄，见下句注）；
（r10：原「faultlogger 条目或进程退出记录」析取废除——「进程退出记录」为幽灵引用（r3 第三趟 B1 的 `exit-record` 已删），死亡证据口径随「死亡事实记录（证据向量）」节 `process_death_observed` 证据源闭集：`PidOfVpn` positive 基线转 absent + capture 静默（r12 起携 `Signal` ∈ {`SIGKILL`, `SIGTERM`} 的终止条目降为该确认下的并存关联证据、不再独立充分，见该节 blocker 3 修订）；事件类条目不证明进程消失）；
仅有 marker 缺失而无死亡证据 → `unobservable(cause=marker-gap-indeterminate)`。**禁止以 marker 缺失单独推断被杀**（E3 0001 误判教训，`docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:163`）。
**r13 阶段未达死亡收口支（第十三轮 blocker 1，grok B-01 = sol B1 两席收敛；与 r12 D8b 阶段未达收口同构）**：触发三条件同时成立——`process_death_observed = observed-true`（死亡分量，唯一死亡谓词源见「死亡事实记录（证据向量）」节）、`N1BDISC_D7_BEGIN` 缺、`last_visible_site` ≤ P5T（死亡位点在 P6 前，证据规则 4；PRE 已发、D7 从未开始的合法平台死亡——上方 `observed-false` 支要求「`D7_BEGIN` 后静默」，BEGIN 缺时该支前件结构性不成立，与本支互斥）：
- **落值**：`u7_long_task_watchdog_behavior` 记 `unobservable(cause=stage-not-reached)`——**复用 r12 D8b 阶段未达支既有字面**（同一「阶段未达」语义、同 post-PRE 死亡域；沿 `destroy-not-reached` 的跨域注记格式注明：本字面与 D8b 支同域同义故跨字段共用，对照 `destroy-not-reached` 因位点域不同不跨域共用，防近义字面误引）；
D7 派生落盘字段 `start_mono_ms`（`D7_BEGIN`）/`elapsed_ms`/`iters`（`D7_END`）同支各自记 `unobservable(cause=stage-not-reached)`——unobservable 落值非钟读数，单调钟绝对域门无数值可查、不挂 F8（沿 r12 D8b 界定句式）。
- **明确不触发字段缺项 fail**：各字段已有预注册 cause 落值，verdict 节「任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）」（F4 面）对本支不适用（与 D8b 两支同款声明）。
- **skip 分支优先（r13 主会话补，P1 留裁 ②）**：若本死亡路径属 skip 表 `no-live-vpn` 分支（D7 整体被 skip、尚未发出其 SKIP marker 即死），`u7` 与派生字段**先落 skip 编码** `unobservable(cause=no-live-vpn)`（skip 表指派），本支不截胡——skip 路径的 D7 从未计划执行，语义是「跳过」而非「未达」。
**r6 V4 补充（位点不确定性约束；r7 X5 同步置位判据；r8 Y2/Y3 修正方向与阈值；r9 改按 `marker_tail_state` / `T_tail` 口径——原 `site_uncertainty` / `T_uncertainty` 及其与行 2 判定窗的全部比较论证随死因表删除）**：死亡位点判定为 P6 **且** `marker_tail_state = possible-tail-loss` 存在
（置位判据 = **死亡证据墙钟 − 最后可见 marker 墙钟 > `T_tail = 25000 ms`**（谓词与五值域见「死亡事实记录（证据向量）」节 `marker_tail_state` 定义表，r10 四值域扩为五值域——补 POST 缺且无死亡证据的 `no-death-evidence` 值；取值理由 = D7（静默敏感阶段）的合法时长上界（20000 冻结时长 + 5000 宽限；r10 标签同步，B M-03）；
r8 Y3 原以「< 行 2 判定窗 27000 ms」为约束的比对论证随死因表删除；旧「P6 时间盒 25 s × 1.5」废弃））
→ `u7` 记 `unobservable(cause=marker-tail-loss)`（r9 正名：原 cause 字面内嵌已删字段名 `site_uncertainty`，随字段更名 `marker_tail_state` 一并正名，判据尚未冻结之际改字面代价最小；旧字面全文清零、不再出现），**不得赋 `observed-false`**（D7_END 可能已实际发出但未落入 capture；区分「被杀」与「marker 丢失」不可靠时宁缺勿误）。
- **结论边界**：结论只覆盖冻结的 20000 ms 与该负载形态，**不得外推更长任务或其他负载**。
- **时间盒**：任务 20 s + 宽限 5 s。
- **barrier/终态轮询原语（冻结）**：原子标志（Rust `AtomicBool` / C11 `atomic_uint`）+ `clock_nanosleep(CLOCK_MONOTONIC, 10ms)` 间隔——用于本门全部「有界轮询等待」位点（D-W barrier、worker 终态），禁止忙等、禁止其他 sleep 原语。

### D-W 唯一登记 waiter（含暴露窗口论证与归因收紧）

- **执行者**：探针 native 侧**恰好一个** `pthread_create` 调用点（文件+函数登记在册；静态断言 A1 机器保证唯一性）。决议 §4.3.5：除本 waiter 外禁止开线程（`docs/native-nx-n1b-adjudication.md:128`）。
- **worker 序列**（单调时钟贯穿）：emit `N1BDISC_DW_SPAWN|tid=<n>` → **drain**：`read(fd_dup)` 循环（自 D2.5 起非阻塞），**每类返回的转移规则冻结（C6）**：
  - `>0` → 继续，累计 `reads`/`bytes`；
  - `0` → **终止 drain**（非阻塞 fd 上 read 返回 0 不属 EAGAIN 语义，含义不可预设），登记 `end=zero-read`（观察事实；与 EAGAIN 正常终止分立登记，不影响后续时序条件）；
  - `-1/EINTR` → **重试**（不计入 `reads`，重试次数单独登记 `eintr_retries=<n>`；5 s 单调时间盒覆盖重试期，EINTR 风暴由时间盒熔断，不忙转）；
  - `-1/EAGAIN` → **正常终止**，`end=eagain`；
  - `-1/其他` → **终止并逐字登记该 errno**，`end=errno-<n>`（协议继续，drain 非正常终止本身是观察事实）。
  登记 `N1BDISC_DW_DRAIN|reads=<n>|bytes=<n>|elapsed_ms=<n>|timeout=<b>|end=<eagain|zero-read|errno-<n>|box-expiry>`（drain 时间盒 5 s，单调时钟，覆盖上述全部转移；盒到点无论处于哪类转移一律停并记 `timeout=true`、`end=box-expiry`，协议继续——drain 超时本身是观察事实，且时序条件见下仍生效）。
**五类返回与四值 `end=` 的换算（r3 收尾趟 G5 冻结，机器可对应）**：C6 五类返回中 `>0`（继续累计）与 `-1/EINTR`（重试）两类**不产生** `end=` 终态；其余三类一一映射——返回 `0` → `end=zero-read`、`-1/EAGAIN` → `end=eagain`、`-1/其他 errno` → `end=errno-<n>`（`<n>` = 该 errno 数值）；盒到点时上述任一进行中状态一律覆盖为 `end=box-expiry`（四值互斥穷尽）。
→ emit `N1BDISC_DW_BARRIER` 并置 barrier 原子标志（**紧贴 poll 调用之前**）→ `poll(fd_dup, POLLIN, 5000ms)` → 读单调时钟 → emit `N1BDISC_DW_RETURN|elapsed_ms=<n>|ret=<n>|errno=<e>|revents=<十进制 bitmask 单值（r10：编码域 = 全部非负整数，任意位组合均为合法平台观测；S2 冻结掩码全集的全部子集 ∈ 0..63 为已知位组合，含未知位的值由未知位前置门分流（r16：仅 0/0b 前置检查未命中的输入到达该门；无 SKIP 且 `_C` 缺的未知位输入先归 0b））>|at_mono_ms=<n>`（**r15 更正，sol B-02：删除原 `class=<c>` 字段**——
worker 在 poll 返回后立即发射本 marker，而最终 `dw_return_class` 依赖未来才知的事实（SKIP 是否最终出现、`_T`/`_C` 是否最终出现、`at` 相对 T/C）；
同一前缀可通向不同终局分类，即时 marker 无论填什么都至少错一支。本 marker 只发 raw（`ret`/`errno`/`revents`/`elapsed_ms`/`at_mono_ms`）；
最终 `dw_return_class` 的派生所有权——**r16 所有权裁定（sol B-01 死锁解环；主会话裁定，供下轮挑战）**：complete 形态（POST 存在）下由**探针主线程在 P12 发射 `N1BDISC_POST` 之前唯一派生**——P12 时全部输入已发生（SKIP/`_T`/`_C` 在其自身发射史、resolve 状态来自其自身等待、worker 终态来自其自身 P10 轮询、**及 poll/drain raw**（`ret`/`errno`/`revents`/`elapsed_ms`/`at_mono_ms`/drain 终态——13 类的真正输入在 worker）；r15 B-02 的洞是 **worker 即时 marker** 的未来依赖，P12 无此问题）；
**worker→main raw 共享快照（r18 机制冻结——grok 主案，sol B-01 = grok B-02 ⊇ deepseek B-01/M-01 三席收敛；原 r17「发射前写入、与 marker 同值、RETURN 缺则无 raw」三句不能同时成立，随本重写整体废除）**：
(1) **单次读 + 同源两写**：worker 在 poll 返回后**单次**读取 `ret`/`errno`/`revents`/单调钟（`elapsed_ms` 与 `at_mono_ms` 各读一次），同一组局部变量值既写共享原子字段又发射 `DW_RETURN` marker——两处同源、禁止二次读钟/读 errno（sol M-01/deepseek M-01：二次读可漂移，同 class 等价类内 raw 分叉漏检）；
(2) **快照域六字段**：五 raw + `dw_drain_end` 终态（class 行 7 的第四前件依赖它——同一 poll raw 配不同 drain 终态落不同类，P12 须有此输入；worker 在 drain 完成时先写 drain 终态进快照再发 `DW_DRAIN` marker，同源两写同款）；
(3) **发布边界 = worker 终态原子标志置位（release）**：标志置位前快照字段对主线程**不可读**——join-timeout∧标志未置 → 主线程**无视快照**（快照可能半写）、`dw_return_class` 一律落 (d) `poll-never-returned` 收口；主线程读快照（acquire）只在 P12、且仅当标志已置。r18 冻结：标志是唯一可读门——消除松散发布读到初始化值的窗口，及「发射前写入」与「RETURN 缺则无 raw」的矛盾；
`dw_outcome` 的**全部**派生字段（`dw_return_class`/`dw_join_result`/`dw_destroy_distinguishable_from_timeout` 及 `dw_*` 终态字段）同此所有权——探针 P12 写入、runner 重建比对、不一致挂 F8(2)（r17 扩展：原仅 class 声明，其余字段同批）；
**EXIT 存在性无小窗（r18，sol M-01）**：`dw_watchdog_killed` ①②支依赖 `DW_EXIT` 存在性——若 worker 序为「置标志 → emit EXIT」，P12 在标志置位后仍须等 `DW_EXIT` marker 发射（EXIT 小窗、存在性非单值）——**采标志门收口**：worker 侧序改为 写快照 → emit `DW_RETURN` → emit `DW_EXIT` → 置终态标志（下方 worker 序已同步对调），标志语义扩为「worker 全部输出已完成」（raw 已写**且** EXIT 已发射），r18 冻结；
runner 从 capture raw **独立重建并比对**，不一致 → **fail，挂 F8(2)**（capture 内自相矛盾：探针 P12 派生值与 runner 从 capture raw 的独立重建值不一致——同一事实两算不符，记录器完整性缺陷；r17 挂载，grok B-03 = sol M-01 两席收敛：原『沿 E2 模式』未入闭集字面，按局部句 fail、按闭集实现漏判 = fail-open），重建值不作为第二事实源、只作校验；
**pre-only 死亡收口形态**（POST 不存在）下由 runner 收齐 capture 后派生，两形态互斥、各形态唯一派生点（派生序见判定表段 r15 冻结；r15 原声明「runner 在收齐 SKIP/`_T`/`_C`/终态后唯一派生」随死锁解环改述——该声明下 complete 形态的 runner 须先收齐终态（含 POST）方能派生、而 POST `dw_outcome` 又须已含派生 class，循环等待、complete 成功态不可实现）；
**单源声明（r18 改述——原「raw marker 与派生结果无双源」句随快照机制重写改述）**：P12 派生的 raw 输入 = 快照（经标志门读取）；runner 重建的 raw 输入 = capture 中的 `DW_RETURN` marker——两输入面的**一致性由 F8(2) 比对背书**（同源两写 + 标志门使分叉只可能来自记录器缺陷，分叉即 fail，合法 trace 永不分叉）；快照与 marker 不是双源——同一次读取的两个投影；complete 形态唯一事实源 = 探针主线程 P12 派生值（runner 重建值仅作校验）、pre-only 形态唯一事实源 = runner 派生值，marker 不再携带候选值）
→ emit `N1BDISC_DW_EXIT` → 置 worker 终态原子标志（**r18 序对调：原「置标志 → emit EXIT」废除**——EXIT 发射纳入标志门，标志语义 = 「worker 全部输出已完成」（raw 已写**且** EXIT 已发射），sol M-01 消 P12 的 EXIT 小窗）→ 线程返回。
- **主线程序列**：以原子标志 + 10 ms `clock_nanosleep` 有界轮询等待 barrier（**≤7 s** = drain 盒 5 s + 调度裕量 2 s——worker 先 drain 再置 barrier，等待盒必须覆盖 drain 全程，否则 drain 超 2 s 时 barrier 必超时、destroy 在 drain 期间就被调用，违反决议 §三.3「destroy 之前确认进入等待」并使「drain 超时 × in-wait 确认成功」组合结构不可达；超时 → `dw_entry_confirmed=unobservable(cause=barrier-timeout)`，**跳过 in-wait 采集**，waiter 事实按 unobservable 收口）
  → **in-wait 证据采集**（barrier 确认后、destroy 之前；见下）→ emit `N1BDISC_DW_INWAIT|src=<proc-stat|proc-syscall|both|unreadable>|conf=<state>|samples=<n>|errno=<e>` → 读单调时钟并 emit `N1BDISC_DW_DESTROY_T|mono_ms=<n>`（**紧贴 `destroy()` 调用之前的时序锚**；r7 X4 保留；r9：本步至下方 `_C` 步为「共享 destroy 子协议」，见本节专条——凡实际调用 `destroy()` 的路径一律依次走这三步）
  → **发起 `destroy()` 调用**（取得 Promise，**不等待**；全程唯一一次——r8 Y1 四步序列第二步：实现契约只要求调用已发起并取得 Promise 对象，不要求 resolve）
  → emit `N1BDISC_DW_DESTROY_C|mono_ms=<n>`（**r7 X4 新增 post-invocation marker，r8 Y1 校准发射点：紧贴 `destroy()` 调用语句之后、resolve 等待之前发射**——它是「destroy 调用已发起」的精确锚；`DW_DESTROY_T` 只证「即将调用」，marker 在而调用未发生（如 marker 发出后进程立即被杀）的场景由本 marker 消除误判。**防误引注，r8 Y1：本 marker 现位于 destroy 调用之后**——r7 版主序列把 `_C` 排在 `destroy()` 之前属落笔执行错误，已按四步序列修正）
  → **有界等待 resolve**（时间盒 10 s；未 resolve → `destroy_unresolved` 观察、跳过 D6a、继续等 worker 终态）→ D6a（同调用栈）→ 有界轮询 worker 终态标志（10 ms 间隔、≤8 s；到期记 `join-timeout`，**不再调用 `pthread_join`**——worker 由进程退出回收，登记 `join-timeout-worker-abandoned=true`（观察项））
  → D-W 字段汇总 → D6b。
- **共享 destroy 子协议（r9 第四步新增；规范稿 §4.3，解决 BL-7）**：凡**实际调用 `destroy()` 的路径**——P9 D-W 主线（上方主线序列 `_T` → 调用 → `_C` 三步）与按 skip 表跳过 D-W 的 **`dup-failed` 分支**（该分支 destroy 照常执行）——一律走**同一个 destroy 子协议**，依次：emit `N1BDISC_DW_DESTROY_T|mono_ms=<n>`（紧贴调用之前）→ 发起 `destroy()` 调用（取得完成凭据，不等待）→ emit `N1BDISC_DW_DESTROY_C|mono_ms=<n>`（紧贴调用语句之后、resolve 等待之前）。
  其后的 resolve 有界等待沿各路径既有规则（主线 10 s 盒；`dup-failed` 分支的 D6a 触发同源于该 resolve）。`no-live-fd` 分支不调用 destroy（发 `N1BDISC_SKIP|item=destroy`），不在本协议域内。
  **理由（须写明，席 A/席 B 一致强调）**：取消死因归因表（r9）**不得用来掩盖该 marker 缺口**——缺 `_C` 会让 `destroy_call_state` 记成假的「未调用」，污染 `u4_*` 与 OB-04 的判定输入事实；**但缺 `_C` 本身不再驱动 fail**——它只是把该分量降为 `unobservable(cause=call-boundary-incomplete)`（「`destroy_call_state` 五态表」第 3 行），协议继续。静态断言 A9 机器背书本协议的唯一调用点与四步序。
- **destroy 位次硬规则（第二趟修订冻结）**：**destroy 不得早于 `N1BDISC_DW_BARRIER` marker**（决议 §三.3「于 destroy 之前经预注册 barrier/marker 确认进入等待」的字面行使）。
barrier 等待盒（7 s）到期仍未见 marker → destroy **顺延**：主线程继续以同一 10 ms 有界轮询等待 marker，直至 worker 终态轮询盒到期（自 barrier 盒到期起算 ≤8 s）；届时仍无 marker → 视为 worker 异常（含 watchdog 杀进程），destroy 不再执行，协议交由观测窗到点 + host finally 收口，登记 `destroy_not_executed=barrier-never-observed`（观察项；
**r9 第五步裁定：本路径随即补发 `N1BDISC_SKIP|item=destroy|cause=barrier-never-observed`，`destroy_call_state` 按五态表定 `not-called`**——五态模型规定 `not-called` 只由显式 SKIP 证明；协议在此明确知道 destroy 未被调用，若留一条「已知未调用但记 `not-reached`」的旁路，等于给「未调用」造第二种表达方式，正是历轮反复出问题的模式，故必须以唯一途径证明。
`N1BDISC_SKIP` 为冻结 marker 字面，`item=destroy` 已在其域内、无需扩充；`cause` 无封闭枚举域、按位点逐字预注册，本趟把 `cause=barrier-never-observed` 预注册入 skip 位点清单——原 r9 首稿「不另发 SKIP、记 `not-reached`」的表述废除）；
   终态按 PRE/POST 求值规则收口——r5 U1 前移后 PRE 已于 P5T 发出：P8 位于 P5T 之后，故本路径不再是无 PRE 的 `fail`，而按「PRE 在 + POST 缺 = `pre-only`」+「死亡事实记录（证据向量）」节七分量登记收口（r9：原「+ 死因分类收口」随死因表删除改写，死亡侧仅 F1 触发 fail；r4 R1「此时 PRE 未发」的旧判定随 P8T 位点废除而失效）。
- **有界 join 可行性裁定（第二趟修订冻结，选 (b)：本探针架构下 join 不可有界）**——依据：目标 API 26 musl sysroot 实测（`/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/native/sysroot/usr/include/pthread.h`，唯一 join 相关声明为 `int pthread_join(pthread_t, void **)`）不存在 `pthread_timedjoin_np`/`pthread_tryjoin_np` 等**任何**有界/非阻塞回收机制（二者是 glibc 扩展，musl 通常没有，本例实测确认）；
因此**不得声称 join 有界**，这一事实本身登记为观察事实与路径②的输入。主线程行为冻结：仅在 worker 终态原子标志置位后才调 `pthread_join`；若标志已置位而 `pthread_join` 仍阻塞，该阻塞**不设用户态超时出口**，由观测窗到点 + host finally 收口兜底——`dw_join_result=join-blocked-observed` 由 **runner 依轮询超时登记**（r5 U12 统一表述：阻塞中的线程自身不可能发出任何登记，凡「阻塞后立即/现场登记」的旧措辞一律按本句读作 runner 侧登记）。
终态轮询盒到期（标志未置位）→ 仍**不调用** `pthread_join`，worker 由进程退出回收，登记 `join-timeout-worker-abandoned=true`（观察项）。（r4 第一趟 R6c：`pthread_join` 位于 `N1BDISC_PRE` 发射之后——join 的任何阻塞/超时结局均不得损及 PRE 已承载事实。）
- **`dw_return_class`（闭合枚举，有优先级的穷尽判定表；第二趟修订冻结）**——判定输入 = `poll` 返回值 `ret`（含 errno；**新登记字段 `dw_poll_ret`/`dw_poll_errno`，`N1BDISC_DW_RETURN` marker 增 `ret=<n>|errno=<e>` 两字段**）、revents 集合、`elapsed_ms` 相对 4500 的位置（阈值 0.9×T_dw=5000ms 冻结）、`poll` 返回时刻 `at_mono_ms` 相对 destroy 调用时刻 `destroy_call_mono_ms` 的先后
（**r9：该比较一律按「worker 返回时序三分带」口径执行——`destroy_call_mono_ms` 一律取 `_C` 的 `mono_ms`，两侧严格不等号，闭区间整体归歧义**）、drain 终态。
**合法域门（r3 第二趟 E1 新增，先于下方判定表执行）**：本协议 `poll` 只监视单个 fd（`fd_dup`），故合法返回组合限于——`ret ∈ {-1, 0, 1}`；`ret == 0` 必须 `revents` 为空；`ret == 1` 必须 `revents` 非空；`ret == -1` 必须携带 errno。
**任何矛盾输入**（如 `ret=0` 且 revents 含 POLLHUP、`ret>0` 且 revents 为空、`ret` 落在 {-1,0,1} 之外、`ret==-1` 无 errno）→ verdict `fail` + `N1BDISC_DW_RETURN` 原文逐字入档（属探针/解析缺陷，不是平台事实，沿 criteria-gap 处理 (3)「字段无法求值」同一法理）——未初始化的 revents 或解析缺陷因此**不得**伪装成平台唤醒事件、不得错误解锁路径①。
合法域门通过后、进入判定表匹配前，还须经本节「`revents` 编码冻结」段的**未知位前置门**（r9 第五步 BL-3 升格）——**r15 派生序冻结（sol B-01）：合法域门 → 0/0b 前置检查 → 未知位门 → 普通行 1-11**。
**r15 重排理由（sol B-01，主会话原不可达性注的错误由此修正）**：原序（合法域 → 未知位 → 表内 0/0b 优先级）下，`ret=1,revents=64`（未知位）被未知位门**直接定类 `other-revents` 而不进表**——0b 永远看不到它；
若 worker 已 RETURN/EXIT 而进程死于 `_T` 前（`_C` 缺），最终 class=`other-revents`（普通类）而 `dw_destroy_distinguishable` 四步 (0)-(3) 全不命中 → F8 烧掉合法平台终态。
重排后 **0/0b 的 SKIP/`_C` 门先于未知位门**：一切「无 SKIP 且 `_C` 缺」的输入（含未知位）先归 0b，未知位门只对**无 SKIP 且 `_C` 在**的输入分流 `other-revents`（r16，deepseek M-01：SKIP 在的输入已被前置检查路由类 0、到不了本门，原「（或 SKIP 类 0）」措辞删除）——「普通类 + `_C` 缺」自此**真正**不可达。
**0/0b 前置检查（r15 自表内行 0/0b 提升为前置门）**：`N1BDISC_SKIP|item=destroy` 类显式 SKIP 在 → 类 0（r16 更正注：原 r15 手工落盘笔误作 `item=D-W`——那是另一枚 marker（D-W 整体被 skip 则无 poll、无 RETURN、走步 (0) 编码），类 0 的 SKIP 支与表行 0、五态表、四步映射逐字同一，只认 `item=destroy`）；无该 SKIP 且 `N1BDISC_DW_DESTROY_C` marker 缺 → 类 0b；其余进入未知位门。类 0/0b 的行条件与既有表内定义逐字不变（仅求值位置前移）。
**r17 改述（三席收敛，第十七轮 B-01）：SKIP 支求值上移至下方分域声明 (c)——先于「有无 RETURN」分域，有 `N1BDISC_SKIP|item=destroy` 即类 0、无论 RETURN 在否**（grok 原话：类 0 仅认 item=destroy SKIP，不要求 RETURN——类 0 的两条真实协议路径本就无 RETURN）；本门对 RETURN 在场输入的 SKIP 支语义不变（同一输入两侧同判类 0）；无-RETURN 侧的类 0 落点见分域声明 (c)。
**分域声明（r16，sol B-02；r17 重写为四子域，三席收敛，第十七轮 B-01）**：`dw_return_class` 的求值按下列四子域收口——
**(a) skip 表指派**（不变）：D-W 整体被 skip（`no-live-fd`/`dup-failed`）→ 由 skip 表指派 `unobservable(cause=no-live-fd)`/`unobservable(cause=dup-failed)`，不进 13 类（无 poll 返回、无分类输入）；
**(b) pre-only 死亡收口编码**（不变）：进程死亡、判定输入不可得 → 按 `destroy_call_state` 五态分流落 `destroy-not-reached`/`post-destroy-unobservable`/`call-boundary-incomplete` 三透传值，不进 13 类；
**(c) 类 0（`destroy-skip-proven`）——不要求 RETURN（r17 新增支）**：判定输入 = `N1BDISC_SKIP|item=destroy` marker 存在性（机器可判、与 poll 无关），有该 SKIP 即类 0、无论 RETURN 在否——其两条真实协议路径（skip 表 destroy 行、barrier-never-observed 顺延）都无 RETURN（grok 原话：类 0 仅认 item=destroy SKIP，不要求 RETURN）；
求值序上 SKIP 支先于「有无 RETURN」分域、且 **(a) 先行**——`SKIP|item=D-W` 同现（D-W 整体被 skip）的输入仍全量沿 skip 表指派（(a) 是字段级全量指派、其 destroy 行 SKIP 不改道类 0，skip 表与既有验收锚不变），非 (a) 域输入（barrier-never-observed 顺延等）有该 SKIP 即落类 0；
**(d) complete∧join-timeout 具名收口（r17 新具名 cause；r18 前件改述）**：`unobservable(cause=poll-never-returned)`——触发四前件 = 非 skip（无任何 `SKIP|item=destroy`）∧ 进程活到 P12 ∧ P10 终态轮询盒到期（`join-timeout` 已登记、`join-timeout-worker-abandoned=true`；
**r18 补注：join-timeout∧标志已置（迟到返回恰在盒到期前后）→ 非 (d)**——标志已置则读快照走 13 类（RETURN 是否入 capture 由 runner 侧比对/F8(2) 处理））
∧ `DW_RETURN` 缺（**以 capture 为准、P12 检验**；r18：标志未置是本支机器门，原 r17 括注「终态标志只在 RETURN 后置位 → 该路径必然无 RETURN」随快照发布边界重写废除；
**r18 归因洁净补注（D6b 重裁侧，grok B-01 = sol B-02）**：进程内 marker 发射与 capture 落盘是两个事件，迟到 RETURN 入 capture 则本前件失败——与上方标志已置补注为同一场景的两面；D6b skip（D6 节 D6b 段 r18 重裁）保证主线程在 P12 前不再触碰 `fd_dup`，晚到 RETURN 若发生不会被 D6b 的 close/read 唤醒或抑制——**归因洁净由 skip 保证，不由 close 的平台行为决定**）；该 cause 纳入 `dw_return_class` 取值域（18→19 值）；
**`dw_join_result` 不扩域（r17 核对结论）**——该形态 join 侧落既有本体值 `join-timeout`（P10 到期登记本不依赖 RETURN），join 域维持 10 值；`dw_destroy_distinguishable_from_timeout` 四步表：该值归步 (0) 透传（worker 未返回、resolve 两维前提不成立——沿 skip/死亡收口同款理由）；
RETURN/DRAIN 依赖字段落值：`dw_poll_ret`/`dw_poll_errno`/`dw_poll_revents`/`dw_poll_return_elapsed_ms` 各记同 cause（poll 未返回、RETURN raw 无值），`dw_drain_*` 不盖——poll 挂死形态下 DRAIN 先于 BARRIER 必已发射、raw 有值（沿 r10 反 blanket 法理，真实数据不得盖掉）；
**(e) 标志已置 ∧ capture 无 `DW_RETURN`（r18 新增第五形态收口——deepseek 第 6 格/grok B-02 第五形态/sol 同，三席三个角度收敛）**：join 成功 + marker 未入 capture——标志只在 RETURN 发射后置位（r18 序：标志 = worker 全部输出已完成），标志在而 capture 无 = **capture 完整性矛盾**（中间 marker 丢失，hilog 中段丢行）→
**fail(F8(2))，具名 `unobservable(cause=return-marker-missing)` 不适用**（该形态是 fail 不是 unobservable——与 `poll-never-returned` 的区别：标志已置证明 poll 已返回且 raw 已写，缺的只是 capture 记录）→ 不赋 class 值、直接 fail；
非 (a)-(e) 且有 `DW_RETURN`（(e) 已先行 fail 收口，r18）：合法域门 → 0/0b 前置检查（SKIP 支已上移，见上方 r17 改述）→ 未知位门 → 普通 1-11。**`SKIP|item=D-W` 与 `DW_RETURN` 同现** → 矛盾输入（D-W 整体被 skip 则无 poll、RETURN 不应存在）→ **F8(2) fail**（r17 轴更正，grok M-01：F3 活字面是全序/时序非单调，矛盾双 marker 属 capture 自相矛盾轴——仍在 fail 闭集、verdict 不变，仅轴归属更正）。
**r17 重写理由（三席收敛，第十七轮 B-01；r18 口径更新）**：原 r16 二分（skip∪death）漏了类 0 的两条真实路径（均无 RETURN）与 complete∧join-timeout 形态——假穷尽使两个合法形态烧 ID。**r18 更正本行的「终态标志只在 RETURN 后置位 → 该路径必然无 RETURN」旧论断**：该论断被 (d) 补注取代——标志置位与 RETURN 入 capture 是两个事件（迟到返回形态标志已置而 (d) 不吸附、第五形态标志置而 capture 无 marker 落 (e)），(d) 的机器门是「标志未置」、capture 前件在 P12 检验。
合法域门通过后，方进入下述派生序（r15 改述：原「13 类判定表」的自上而下匹配现按上述前置门序执行；类 0/0b 已前置、行 1-11 为终末匹配）。**首个命中即定类并终止匹配**：

  | 优先级 | 类 | 判定条件（命中即终止匹配，其余维度不再看） |
  | --- | --- | --- |
  | 0 | `destroy-skip-proven`（r9 改名改条件，原 `destroy-never-called` 废除——缺 `_C` ≠ 未调用，见「`destroy_call_state` 五态表」） | 协议显式发出 `N1BDISC_SKIP\|item=destroy`（即五态 `not-called`，**唯一可证「未调用」的途径**）——**不归因、不解锁路径①**（现行路径中该 SKIP 产生于 D-W 整体被 skip 的分支与 r9 第五步 barrier-never-observed 顺延路径两处，本行保证该组合在表内仍有唯一落点） |
  | 0b | `destroy-call-unobserved`（r9 新增） | 无类 0 的 SKIP 且 `N1BDISC_DW_DESTROY_C` marker 缺——destroy 调用边界不可观测：`_T` 在 `_C` 缺 → 五态 `unobservable(cause=call-boundary-incomplete)`（不得写 `never-called`）；`_T` 缺 `_C` 缺 → 五态按此前位点定 `not-reached`。两分支均歧义——**不归因、不解锁路径①**（D4 位次理由原样继承：第 4/5/7/8 类依赖 `destroy_call_mono_ms`，`_C` 缺时该锚点不存在，无本行则掉进 criteria-gap；本行只定 `dw_return_class`，五态分量另按其定义表独立求值） |
  | 1 | `interrupted` | `ret == -1` 且 `errno == EINTR`（此时 revents 无意义） |
  | 2 | `poll-error` | `ret == -1` 且 errno 为其他值（**逐字登记该 errno**） |
  | 3 | `fd-invalid` | revents 含 `POLLNVAL`（非法 fd，单独登记，**不**计入任何归因类） |
  | 4 | `pre-destroy-ready` | revents 非空（不含 POLLNVAL）且 `at_mono_ms` **<** `T_mono_ms`（r9 三分带 `definitely-pre-invocation` 口径——原 `at_mono_ms <= destroy_call_mono_ms` 会把 `_T`–`_C` 闭区间内的返回也判「destroy 前就绪」，随三分带废除；`T_mono_ms` 取 `_T` 的 `mono_ms`。destroy 前就绪：残留/外来，无论 drain） |
  | 5 | `late-fd-event` | 含 `POLLHUP` 或 `POLLERR` 且 `elapsed_ms >= 4500` 且 `at_mono_ms` **>** `C_mono_ms`（r9 三分带口径：严格大于 `_C` 的 `mono_ms`，即 `definitely-post-invocation`；歧义带与 `_C` 缺均不满足本行）——destroy 异步生效落在 poll 最后 500 ms 结构可达；登记为观察事实但**不**即时归因、不解锁路径① |
  | 6 | `late-data` | 含 `POLLIN` 且 `elapsed_ms >= 4500`（观察类，不归因） |
  | 7 | `fd-event-like` | 含 `POLLHUP` 或 `POLLERR` 且 `elapsed_ms < 4500` 且 `at_mono_ms` **>** `C_mono_ms`（r9 三分带口径同行 5：歧义带与 `_C` 缺均不满足本行、不解锁路径①）且 drain 以 EAGAIN 正常终止（`dw_drain_end == eagain`；r3 第一趟 D3 收紧——旧条件 `dw_drain_timeout == false` 在 `end=zero-read`/`end=errno-<n>` 时同为 false，会使 drain 出错后仍可归因 destroy）。**唯一可归因 destroy 的类**——外提为下方表后「行 7 注」（r4 第四趟 W1 外提；r9 改比较口径） |
  | 8 | `data-ready-post-destroy` | 含 `POLLIN`（无 HUP/ERR）且 `elapsed_ms < 4500` 且 `at_mono_ms` **>** `C_mono_ms`（r9 三分带口径，同行 7：严格大于 `_C` 的 `mono_ms`，歧义带与 `_C` 缺均不满足）且 drain 以 EAGAIN 正常终止（`dw_drain_end == eagain`，同上收紧）——观察类，**不支撑** destroy 归因 |
  | 9 | `timeout-like` | revents 空且 `elapsed_ms >= 4500`（poll 自身超时返回） |
  | 10 | `spurious-early` | revents 空且 `elapsed_ms < 4500` |
  | 11 | `other-revents` | 穷尽兜底：`ret >= 0`、revents 非空但不匹配以上任一行（如仅 `POLLPRI`）（r9：含未知位的输入已由未知位前置门在表外定类，本行的论域因此限于掩码全集内的组合）——不归因，逐字登记 revents |

**行 7 注（r4 第四趟 W1 自上表行 7 外提，内容逐字不变）**：**唯一可归因 destroy 的类**：残留数据与外来包只能产生 `POLLIN`（drain + 时序条件已排除残留，外来包仍可能在 destroy 后到达产生 POLLIN——故普通 `POLLIN` 与 `POLLNVAL` 一律**不计入**本类）。

  优先级 1-2 先于一切 revents 判定（`ret == -1` 时 revents 无意义）；POLLNVAL（优先级 3）先于 `pre-destroy-ready`，消除旧枚举「`pre-destroy` 与 `POLLNVAL` 同时命中两类」的重叠；非空 revents 在 `elapsed_ms >= 4500` 时由优先级 5-6 承载，消除旧枚举「`elapsed>=4500` 只与 revents 空配对」的漏域。
**selftest 须覆盖本表「全部 revents 组合 × `elapsed_ms` ∈ {4499, 4500} 时间边界 × drain 终态 `dw_drain_end` ∈ {`eagain`, `zero-read`, `errno-<n>`, `box-expiry`}（含 `dw_drain_timeout` 真假两值）× `ret` ∈ {`>0`, `0`, `-1`+EINTR, `-1`+其他} × `N1BDISC_DW_DESTROY_T`/`N1BDISC_DW_DESTROY_C` marker 存在/缺失 × `at_mono_ms` 三分带（r9 扩维）」的真值表**，
逐格核对类归属唯一且穷尽（r3 第一趟 D3/D4 扩维；r9 类 0/0b 拆分）。**selftest 另须覆盖合法域门（r3 第二趟 E1）**：`ret=0` + revents 非空、`ret>0` + revents 为空、`ret=2`、`ret=-1` 无 errno 等矛盾输入均须判 `fail` 且不进入 13 类判定表（r9 第五步：含未知位的输入不属矛盾输入，由未知位前置门收口为 `other-revents` 不 fail——r16：0/0b 前置检查未命中的输入方达该门，无 SKIP 且 `_C` 缺的未知位输入先归 0b、同样不 fail，见本节「`revents` 编码冻结」段）。
- **`revents` 编码冻结（r4 第二趟 S2 新增）**：`dw_poll_revents` 字段与 `N1BDISC_DW_RETURN` marker 的 `revents=<…>` 字段一律取**十进制 bitmask**（单值，无分隔符），数值以目标 sysroot 头文件为准（`/home/worker/harmonyos/command-line-tools/26.0.0.461/sdk/default/openharmony/native/sysroot/usr/include/poll.h:12-17`，与 `asm-generic/poll.h:21-26` 一致）：
`POLLIN=0x001`、`POLLPRI=0x002`、`POLLOUT=0x004`、`POLLERR=0x008`、`POLLHUP=0x010`、`POLLNVAL=0x020`。**冻结掩码全集 = {0x001, 0x002, 0x004, 0x008, 0x010, 0x020}**；r10：合法编码域 = **全部非负整数**（十进制 bitmask 单值，任意位组合均为合法平台观测），掩码全集全部子集的十进制值 0..63 是**已知位组合**的描述而非域上界——含未知位的值由下方未知位前置门分流（r16：仅 0/0b 前置检查未命中的输入；无 SKIP 且 `_C` 缺的未知位输入先归 0b）；本节判定表中「含 POLLX」一律读作「bitmask 与对应位按位与非零」。
**未知位前置门（冻结；r9 第五步 BL-3 升格——原「该组合归判定表优先级 11 `other-revents` 类」的表内兜底表述废除）**：`revents` 值超出 0..63 或含冻结掩码全集之外的任意位（**含与 `POLLIN`/`POLLERR` 等冻结位混合的编码**）→ 逐字登记原值（十进制），**直接定类 `other-revents`、不进入上方判定表匹配——本门仅在 0/0b 前置检查未命中后执行**（r16，grok B-01 传播对齐）：
`revents` 含未知位且（无 SKIP 且 `_C` 在）→ `other-revents`；**无 SKIP 且 `_C` 缺的未知位输入已在前置检查归 0b、本门不再定类 other-revents**（r15 派生序冻结的执行面，此前本句未随改——实现者按本句实现会把 `_C` 缺的未知位输入错误路由）（先于判定表执行的前置门，与合法域门同构；本段在正文中居判定表之后仅为编码定义集中，执行次序以本句与合法域门段的锚句为准）。
  **升格理由（r9 第五步 BL-3，已核实）**：判定程序是「自上而下，首个命中即定类并终止匹配」，行 7 `fd-event-like` 的「含 `POLLHUP` 或 `POLLERR`」按 S2 冻结读法 = 按位与非零——构造 `ret=1`、`revents=72`（未知位 64 + `POLLERR` 8）、`elapsed_ms=1000`、`at > C`、drain `eagain` 的混合位输入会先命中行 7 并错误解锁路径①；而行 11 自身条件「不匹配以上任一行」对该输入为假，原条款声称的「归优先级 11」在表内程序下**不可达**——同一输入存在两处互相矛盾的归类，前置门化后矛盾消除。
  **两道前置门的先后（r9 第五步冻结）**：poll 合法域门先执行、**0/0b 前置检查次、未知位门后**（r16 随派生序更新；r9 原冻结为两道门，r15 起为三段）。理由：(a) 矛盾输入属探针/解析缺陷（fail 面），未知位可能是有效平台值（不 fail）——若未知位门先行，携带未知位的矛盾输入（如 `ret=0` 且 revents 含未知位）会被洗成 `other-revents` 不 fail，违反 criteria-gap 判别方法 (2)/(3)「解析缺陷不洗白」；(b) 合法域门只判 ret/errno/revents 组合的合法性、与位集无关，先执行不依赖未知位门的结论，次序无循环依赖。
  **适用范围（r9 第五步）**：未知位门只施于 revents 有意义的输入（`ret ≥ 0`）——`ret == -1` 时 revents 无意义（判定表优先级 1-2 既有口径），仍由行 1/2 收口，未知位门不参与，不改写既有类 1/类 2 行为。
  **selftest ④ 组合域界定（r9 第五步更正表述）**：「全部 revents 组合」的 64 子集穷举界定的是**判定表行内匹配**的组合域（表内输入域不变）；升格后 revents 的完整合法输入域**不再是 0..63**——含未知位的任意十进制编码由未知位前置门收口（r16：仅 0/0b 前置检查未命中的输入定 `other-revents`、原值逐字入档、不 fail；无 SKIP 且 `_C` 缺的未知位输入先归 0b），不入表内匹配。
未知位用例**r16 拆两钉（grok B-01 传播对齐，原「纯未知位 64 → `other-revents`」单钉拆除）**：`revents=64` + 无 SKIP + `_C` 缺 → **0b**（0/0b 前置检查先于未知位门——r15 派生序验收钉）；`revents=64` + 无 SKIP + `_C` 在 → `other-revents` + 原值入档 + 不 fail（未知位门）。
混合位用例同款两钉：`revents=72`（未知位 64 + `POLLERR`）与 `revents=65`（未知位 64 + `POLLIN`）——`_C` 在支均须定类 `other-revents`、原值入档、不 fail、**不解锁路径①**（不得命中行 7/行 8），`_C` 缺支均归 0b。
  **r10 对齐注**：上句「完整合法输入域不再是 0..63」的正向域定义 = **全部非负整数**（十进制 bitmask 单值），与本包对 `N1BDISC_DW_RETURN` marker 规格及 `dw_poll_revents` 字段声明的域冻结同步生效；「64 子集穷举」仍界定判定表行内匹配组合域、不变。
- **单调钟派生字段绝对域门（r9 第五步 BL-4 新增，统一冻结；求值时先于各字段真值表/派生表执行）**：
  - **字段清点（r9 第五步逐字清点；r10 增补，全文单调钟读数/派生时长落盘字段仅此 14 项：读数 11 + 派生时长 3；marker 承载的落盘字段 `dw_poll_return_elapsed_ms`/`dw_drain_elapsed_ms` 与 D8b 的 `ws`/`we` 均与对应落盘字段同值，不另计）**——读数类：`start_mono_ms`（`N1BDISC_D7_BEGIN`）、`mono_ms`（`N1BDISC_DW_DESTROY_T`）、`mono_ms`（`N1BDISC_DW_DESTROY_C`）、`at_mono_ms`（`N1BDISC_DW_RETURN`）、`at_mono_ms`（`N1BDISC_FD` transition marker）、`at_mono_ms`（`N1BDISC_D2_LATE_FD`）、
  `window_start_monotonic`/`window_end_monotonic`（D8b 落盘；r10：载体 = `D8_STORM_BEGIN.ws`/`D8_STORM_END.we`，marker 字段与落盘字段同值）、`created_at_mono_ms`/`closed_at_mono_ms`（ledger canonical 行，E2 重建值）、`p0_ready_mono_ms`（P0 operator-ready 确认记录的单调时刻，runner 侧审计落盘字段；r10 纳入，B M-02）；
  派生时长类：`elapsed_ms`（`N1BDISC_D7_END`）、`elapsed_ms`（`N1BDISC_DW_DRAIN`）、`elapsed_ms`（`N1BDISC_DW_RETURN`）。
  派生别名 `T_mono_ms`/`C_mono_ms`/`destroy_call_mono_ms`（均取自 `_T`/`_C` 的 `mono_ms`）随被引字段受本门管辖；D7 冻结负载伪码内变量 `start_ms`/`deadline_ms`/`now_ms` 为内部量非落盘字段（对外承载即 `D7_BEGIN.start_mono_ms` 与 `D7_END.elapsed_ms`）；r10（B M-02）：P0 operator-ready 确认记录的单调时刻已命名为落盘字段 `p0_ready_mono_ms`（runner 侧审计记录），纳入本门非负性约束——原「无冻结字段名、不在本门域」的排除随命名废除。
  - **绝对域**：读数类字段 ≥ 0（单调钟读数不可能为负）；派生时长类 ≥ 0（「末次读钟 − 起始读钟」在单调钟上不可能为负）。**违反 → verdict `fail` + 原值逐字入档（挂 F8）**：负单调钟值只能来自探针计算缺陷、溢出或解析缺陷，不是任何有效平台观测值，不适用 F4 的「域外有效平台值不 fail」豁免（该豁免以「值本身有效」为前提），法理与 poll 合法域门的矛盾输入同一（沿 criteria-gap 判别方法 (3)，属记录器未尽职，不是死亡事实）。
  - **顺序约束**：`DW_DESTROY_T` 的 `mono_ms` ≤ `DW_DESTROY_C` 的 `mono_ms`（同毫秒合法——两枚 marker 分别紧贴调用语句前后发射，毫秒粒度等值不证先后亦不构成矛盾）；违反 → 属 marker 时序矛盾，**沿既有 F3（顺序破坏/marker 时序非单调）判 fail，本条不另立触发面**（与五态表第 5 行「`_C` 在 `_T` 缺 → fail（走 F3）」同一完整性轴，不重复登记）。`window_start_monotonic` ≤ `window_end_monotonic`（违反挂 F8，同上法理）；ledger 每 `(role,inst)` 实例 `created_at_mono_ms` ≤ `closed_at_mono_ms`（违反沿 S3 (c) 配对状态机的既有完整性失败面，不另立）。
  - **r10 顺序约束界定（B M-02）**：`p0_ready_mono_ms` 位于首个 `N1BDISC_` marker 之前（P0 先于全序），与任何单一受管辖读数之间无可机器核验且有判定意义的先后耦合——只设非负（读数类 ≥ 0，挂 F8），不设顺序约束。
  - **r12 界定（D8b BEGIN-only 死亡收口）**：`window_end_monotonic` 在该支记 `unobservable(cause=storm-incomplete-pre-only)`（D8b 节 r12 段）——unobservable 落值不是钟读数，非负性与顺序约束均无输入、不挂 F8；`window_start_monotonic` 照常自 BEGIN 的 `ws` 重建、仍受本门。
- **in-wait 证据候选（主会话补充设计，与 BL-4 合并处理；可用性本元组未实测）**：主线程在 barrier 标志置位后、destroy 之前，以 10 ms `clock_nanosleep` 间隔 / ≤2 s 上限有界轮询读取两个 `/proc` 同进程自省文件：
  - `/proc/self/task/<tid>/stat`（线程 state 字段的解析规则冻结：**取该行最后一个 `)` 字符之后的下一个 token 作为 state**——第 2 字段 `comm` 用圆括号包裹且**可以含空格与括号**，按空格切分会字段错位、可能把其他字段误判为 `S` 而造成**假阳性解锁路径①**，故必须从右侧定位）
  - 与 `/proc/self/task/<tid>/syscall`（第 1 字段 = 该线程当前所处系统调用号）——**比 barrier 强一档：能真正区分「到达 poll 调用点」与「已阻塞在 poll 等待中」**。判定冻结：
  - `dw_inwait_confirmed` 三态——`observed-true`：窗内**任一样本** state=`S`（可中断睡眠）**且 `proc-syscall` 可读**且 syscall 号 ∈ **poll 族冻结集 `{73}`**，三条件缺一不可。**r16 收紧（sol B-04，主会话裁定宁缺勿误路线；原「（若 `proc-syscall` 可读）」条件化措辞废除）**：syscall 不可读时**不得判 true**——该样本不可确认为 poll 等待，`dw_inwait_confirmed` 按其余样本判定、全部样本不可确认时落下方 `observed-false` 兜底；
构造（sol B-04）：poll 可先于采集窗结束返回、worker 阻塞于其他可中断睡眠（HiLog/锁/写）仍呈 state=`S`，仅凭 S 会误记「已确认进入 poll 等待」、沿 `dw_watchdog_killed` ③ 与路径① 条件 1 污染下游。
（r1 更正：此前加入 `414` 基于错误前提——`__NR_ppoll_time64 414` 位于 `asm-generic/unistd.h` 的 `#if __BITS_PER_LONG == 32` 块内，aarch64 目标头 `aarch64-linux-ohos/bits/syscall.h:74` **只有** `__NR_ppoll 73`（`asm-generic/unistd.h:113`），故冻结集退回 `{73}`；
判定规则：`proc-syscall` 可读且号 ≠ 73 → **逐字登记该号**并记 `observed-false`；同时预注册：若记录级审查发现该号实为本平台 poll 族的其他合法编号，属判据缺口，由记录级审查处理，**不改 Live verdict**。freeze 时静态审查复核该集合）；
`observed-false`：窗尽且至少一个可读样本但无一满足（state≠`S`；或 syscall 号可读且**落在 poll 族冻结集之外**——含 worker 尚在 marker 发射等其他内核调用的瞬态，轮询重采样即为此设；或 r16 收紧第三形态：state=`S` 而 `proc-syscall` 不可读——不可确认为 poll 等待、不作 true（宁缺勿误），窗内全部样本属此形态时窗尽落本支，语义为「窗内从未观测到**可确认的** poll 等待」、仍不构成对「曾在 poll 等待」的否定）；
`unobservable(cause=proc-introspection-unavailable)`：`proc-stat` 不可读（无论 `proc-syscall` 是否可读——state=`S` 无法建立）或两文件均不可读（errno 逐个登记）。
barrier-timeout → 不执行采集，`unobservable(cause=barrier-timeout)`。
**`dw_inwait_confirmed` 活域（r17 补扩，grok B-02）**：本字段取值域 = 上述三态 ∪ `unobservable(cause=barrier-timeout)`（barrier-timeout 支）∪ **`unobservable(cause=inwait-marker-unobserved)`（死亡收口支承载，r17 逐字并列补扩——与 true/false/proc-introspection-unavailable/barrier-timeout 并列；原 r16 只扩 source/samples 域、本字段漏扩，⑪(d) 夹具 pass 与 F4 非单值）**。
**死亡收口支（r16 新增，sol B-03；先于上方三态与 barrier-timeout 支求值）**：触发 = `process_death_observed = observed-true`（死亡分量，唯一死亡谓词源）**且** `N1BDISC_DW_INWAIT` marker 缺（采样前或采样中死亡、未来得及发射）**且** 死亡位点 ∈ P8 窗口（沿 `dw_watchdog_killed` ③ 的位点窗口 P8-P10：`DW_SPAWN` 之后、无 `DW_EXIT`，证据规则 4）。
落值：`N1BDISC_DW_INWAIT` 的 `src/conf/samples/errno` 四载荷字段**同句**各取「自身既有取值域 ∪ `unobservable(cause=inwait-marker-unobserved)`」（`DW_INWAIT` 为其唯一载体；落盘于 `dw_inwait_evidence_source`/`dw_inwait_confirmed`/`dw_inwait_samples` 及其 errno 承载）——**r16 新具名 cause，一条覆盖**（物理原因同源：等待确认完成前进程终止、INWAIT 未及发射）；
`dw_inwait_confirmed` 活域由上方 r17 补扩句逐字承载（与 true/false/proc-introspection-unavailable/barrier-timeout 并列），source/samples 既有同 cause 域扩保持不变。
errno 承载（r17 更正，grok M-02）：原「四载荷字段」中 errno 非独立 D 项、无独立落盘字段名——落值为 marker 载荷语义、随 `unreadable` 载荷形态（`dw_inwait_evidence_source` 条「`unreadable` 含 errno」），本支记 cause 时 errno 承载同句落 cause、不另立字段。
统一覆盖（r16，采主会话倾向、本包裁定执行）：前件不按位点细分——「P8 窗口内、INWAIT 未发 + 死亡分量 true」一条覆盖 drain 中死亡（SPAWN 后 DRAIN 内、BARRIER 前，采集窗未开始）与 BARRIER→poll 间隙 / poll 等待窗内死亡：两形态落值与诊断含义相同（等待确认不可得），且 drain 中死亡原本无任何落值面（采集未开始、barrier-timeout 支亦未到达）→ 同烧 F4，故并入本支统一收口。
**不触发 F4「missing」**：本支各字段已有预注册 cause 落值，verdict 节「任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）」对本支不适用（沿 D8b 两支与 u7 支同款声明）；单调钟域门对本支无数值输入、不挂 F8；`marker_tail_state` 的尾丢失观察独立登记、不改本支落值（该字段为纯观察登记）。skip 分支（no-live-fd/dup-failed）不进本支——无 SPAWN、位点不达 P8 窗口，四字段沿 skip 编码。
求值次序与联动：进程死亡时协议不再走到 barrier 等待盒到点观测，死亡分量 true ∧ INWAIT 缺的输入**不得**改记 `unobservable(cause=barrier-timeout)`；上方 true/false/proc-introspection-unavailable 三态仅进程存活完成采集窗时求值。本支落值时 `dw_watchdog_killed` 依有序互斥表落 ⑤（inwait 非 true、③ 前件不成立）；四字段落盘由 runner 于 pre-only 死亡收口派生（与 `dw_return_class`/`dw_join_result` 的 pre-only 死亡收口同一求值面），探针侧无补发。
  - `dw_inwait_evidence_source` ∈ {`proc-stat` / `proc-syscall` / `both` / `unreadable`}（本次判定实际可读的文件集；`unreadable` 含 errno）**∪ `unobservable(cause=inwait-marker-unobserved)`（r16 死亡收口支新增值，见上）**；`dw_inwait_samples`（样本计数；r16 取值域 = 非负计数 ∪ `unobservable(cause=inwait-marker-unobserved)`）。
**r16 核查说明（sol B-04）**：本枚举无「proc-stat-only」类专用值——仅 stat 可读、syscall 不可读时记 `proc-stat`，该值保留为记录值，但 r16 收紧后 `observed-true` **不得**仅凭它成立（须 `proc-syscall` 可读且号 ∈ `{73}`，见三态规则段）。
  - **in-wait 采集与 worker poll 返回的竞速（明文登记）**：barrier 确认只证 worker 到达 poll 调用点，采集窗（≤2 s）可能跨越 poll 实际返回的时刻——此后样本采到的是 `state≠S` / 非 poll 族 syscall 号的瞬态（worker 已在用户态发射 marker 等路径），或 poll 返回后阻塞于其他可中断睡眠的 state=`S` 样本（r16 补第三形态，sol B-04——syscall 不可读时不得据以判 true）。
**兜底机制 = 轮询重采样**：判定取窗内**任一**满足样本（r16 起「满足」= state=`S` ∧ `proc-syscall` 可读 ∧ 号 ∈ `{73}` 三条件同一样本成立）即 `observed-true`，单次瞬态假阴性被多次采样平滑；`observed-false` 只在窗尽且无一满足样本时登记，其含义是「窗内从未观测到**可确认的**阻塞于 poll」，不构成对「曾在 poll 等待」的否定。
竞速不制造假阳性（r16 按收紧口径重述，sol B-04）：瞬态样本只会压低 true 概率（假阴性方向）；state=`S` 样本本身**不足以**判 true（可产生于 poll 返回后的其他可中断睡眠），`observed-true` 须 state=`S` ∧ syscall 号 ∈ `{73}` 同一样本双证据成立，故路径①不会被该竞速或 post-poll 的 S 态睡眠误解锁。
  - **读取本身失败不是 fail，是平台事实**（SELinux 策略与 `hidepid` 挂载选项都可能使其不可读，本元组从未实测——正因如此预注册为候选而非前提）。
- **弱口径正面处理**：`DW_BARRIER` 只证 worker **到达 poll 调用点**，不证「已进入内核等待」——线程可能在调用 poll 前被调度抢占，或 poll 因残留可读数据立刻返回而从未睡眠；弱 barrier 单独**永远不构成**路径①资格（采认门槛见下）。
- **落盘字段族**：
   - `dw_waiter_spawned`（三态；r14 补定义，deepseek M-02 后半：`N1BDISC_DW_SPAWN` marker 在 → `observed-true`；
     协议走到 D-W 且非 skip 分支而 SPAWN 缺 → `unobservable(cause=marker-gap-indeterminate)`（禁止以 marker 缺失单独推断「未 spawn」）；
     skip 分支（no-live-fd/dup-failed）→ 沿 skip 编码——**本字段无 `observed-false` 支**，探针源码中 spawn 失败与否无独立 marker 可证伪，宁缺勿误）、
     `dw_entry_confirmed`（三态，弱口径如上）、`dw_drain_reads/bytes/elapsed_ms/timeout/end/eintr_retries`、`dw_inwait_evidence_source`、`dw_inwait_confirmed`（五值 = `observed-true`/`observed-false`/`unobservable(cause=proc-introspection-unavailable)`/`unobservable(cause=barrier-timeout)`/`unobservable(cause=inwait-marker-unobserved)`——r18 与上方「`dw_inwait_confirmed` 活域（r17 补扩）」句逐字同一，grok M-02：原「三态」为 r17 域扩漏改残留）、
`dw_inwait_samples`、`dw_poll_return_elapsed_ms`、`dw_poll_ret`、`dw_poll_errno`、`dw_poll_revents`（以上 poll raw 四字段 r17：complete∧join-timeout 形态各记 `unobservable(cause=poll-never-returned)`——poll 未返回、RETURN raw 无值，见分域声明 (d)；`dw_drain_*` 不盖、保 DRAIN marker 真实值；
**十进制 bitmask**，编码逐字沿本节「`revents` 编码冻结（r4 第二趟 S2）」：数值以目标 sysroot 头文件为准、冻结掩码全集 {0x001, 0x002, 0x004, 0x008, 0x010, 0x020}；r10：合法编码域 = 全部非负整数（十进制 bitmask 单值），已知位组合（掩码全集全部子集 ∈ 0..63）进判定表匹配；
含未知位的值由未知位前置门（r9 第五步 BL-3 升格，见本节「`revents` 编码冻结」段；r16：**仅 0/0b 前置检查未命中的输入**到达该门即定类 `other-revents`，无 SKIP 且 `_C` 缺的未知位输入已归 0b）直接定类 `other-revents`、不进入表内匹配（原值十进制逐字入档），不 fail——与 `N1BDISC_DW_RETURN` marker 的 `revents=<十进制 bitmask 单值>` 同一编码）、
  `dw_return_class`（判定表 13 类 + r9 第五步 BL-5 纳入的 skip 编码 2 值 `unobservable(cause=no-live-fd)`/`unobservable(cause=dup-failed)` + r10 纳入、r12 拆分、r13 第三包再拆分的 pre-only 死亡收口编码 3 值 + r17 新增 complete∧join-timeout 收口编码 1 值 `unobservable(cause=poll-never-returned)`（分域声明 (d) 支）= **19 值**；
    r12 拆分（sol M-01，verdict 节 PRE/POST 求值规则 (2) r12 分流条款）：`unobservable(cause=post-destroy-unobservable)`（destroy 已过而输入不可得，r13 第三包起限五态 `call-returned`）与 `unobservable(cause=destroy-not-reached)`（destroy 未达，复用 V2 (1a) 既有字面、跨域共用）；
    r13 第三包再拆出 `unobservable(cause=call-boundary-incomplete)`（五态 `call-boundary-incomplete`——调用边界不可判，透传自身 cause、不再并入 post-destroy，verdict 节 r13 第三包分流条款）；
    历史：r3 第一趟 D4 曾新增 `destroy-never-called`，r9 拆分为类 0 `destroy-skip-proven` + 类 0b `destroy-call-unobserved` 并废除该字面，原「上述 13 值」表述随编码纳入更正）、
  `dw_join_result`（`joined` / `join-timeout` / `join-blocked-observed` / `ESRCH` / `other+errno` + r9 第五步 BL-5 纳入的 skip 编码 2 值 + r10 纳入、r12 拆分、r13 第三包再拆分的 pre-only 死亡收口编码 3 值（同 `dw_return_class` r12 拆分、r13 第三包再拆的三 cause） = **10 值**；r17：complete∧join-timeout 形态落既有本体值 `join-timeout`（P10 到期登记、不依赖 RETURN）、本域不扩——D-W 节分域声明 (d) 支核对结论）、
  - `dw_watchdog_killed`（三态；历史字段名（r3 E11 冻结；r15 语义更正，sol B-03/grok M-02）：本字段**不再承载 watchdog 归属语义**——`observed-true` 的语义以 ③ 为准（waiter 于 destroy 未及窗内死亡且 inwait 确认已进入等待），不构成 watchdog 归属的证据（SIGKILL/OOM/生命周期终止在该观测下不可区分）；N1b 不得将该 true 引用为「平台 watchdog 杀了 poll waiter」；赋值规则仍沿 r3 第二趟 E11 的逐字冻结——原「语义方向与 D7 相反」对比句 r15 改口径：D7 判「任务被杀」，本字段不再判「waiter 被 watchdog 杀」（r15 归因语义已废、见上），故仍不得引用「同 D7」，独立三态如下——
    **r13 改有序互斥表求值（第十三轮 blocker 2，grok M-02 + sol B-02 两席收敛）**：原 true 支与 unobservable 支「`PidOfVpn` absent 且 `_C` 缺（无论 `_T` 在否——不得 true/false）」争同一构造——物理上最真的「waiter 在 poll 等待中被杀」（`_T`/`_C` 均缺）两支同时命中；sol 另指：正例构造中的 SIGKILL 条目只是平台终止关联证据、不证明 watchdog 归属。故三态改**有序互斥表**：求值序按下述①→⑤写死，**先到先得——命中即定值并终止，后续支不再求值**（分支前件在谓词层面可重叠，结局由求值序唯一化）；skip 具名清单位于表外（skip 路径无 waiter 运行、无本表求值面，见下方清单）：
    - ① `N1BDISC_DW_DESTROY_C` 在 **且** `PidOfVpn` absent（positive 基线前置满足）**且 `N1BDISC_DW_EXIT` 缺**（r13 主会话补：EXIT 在时 waiter 已正常走到终态——本字段语义上应为 ④ `observed-false`，本支不截胡 complete 正常主线）→ `unobservable(cause=destroy-terminal-candidate)`——**r13 新具名 cause，原「死亡可由 destroy 解释」句的字面收纳**：
      `_C` 在而 absent 的组合属「destroy 使 `:vpn` 进程 terminal」（预期成功终态方向，r9：原「死因分类行 4 (ii) 承载」随死因表删除——该组合现由七分量证据向量的事实组合承载：destroy 调用已发起记入 `destroy_call_state`（五态定义见「死亡事实记录（证据向量）」节五态表）、`:vpn` 消失记入 `process_death_observed`，不合成任何归因、**不驱动 fail**，
      沿「除 fail 闭集（F1–F6、F8、F9；F7 不在内）外一切结局均不 fail」，r10 排除式与闭集正文同步（A m-02））——destroy 之后的死亡不是「waiter 被 watchdog 杀」的语义窗口，waiter 归因不可判，**不得**判 waiter 被杀（否则同一物理事件被死亡事实记录与本字段双重登记、内部矛盾）；
    - ② `N1BDISC_DW_DESTROY_T` 在、`_C` 缺、且 `N1BDISC_DW_EXIT` 缺 → `unobservable(cause=call-boundary-incomplete)`（**r13 透传「`destroy_call_state` 五态表」第 3 行同名字面**：调用边界不可判——`_T` 只证「即将调用」、`_C` 缺不得反推「调用未发起」，waiter 归因随之不可判；
      r13 自原 r9 第五步「`PidOfVpn` absent 且 `_C` 缺（无论 `_T` 在否）」支拆出本支——该组合的归因不可判性源于调用边界本身，不以 pidof absent 为前提；
      **EXIT 缺已移入前件（r15，grok M-01：与 ① 同构）**（r14 更正，grok M-01 = sol B-02 两席收敛：原 r14「`_T` 在 `_C` 缺 + EXIT 在不可达」的论证**错误**——它漏掉了 EXIT 可早于 `_T` 的交错：inwait 竞态段明文「采集窗（≤2 s）可能跨越 poll 实际返回的时刻」，worker 提前 RETURN+EXIT 后主线程才发 `_T`、再死于 `_C` 前；该组合可达，②须与 ① 同带 EXIT 缺，EXIT 在则本支前件不命中、依序落 ④ `observed-false`——waiter 已正常走到终态，本支不截胡）；
    - ③ `_T`/`_C` **均缺** 且五合取（SPAWN ∧ EXIT 缺 ∧ 静默 ∧ 死亡分量 ∧ inwait 确认）且死亡位点 ∈ P8-P10——`N1BDISC_DW_SPAWN` marker 存在 ∧ `N1BDISC_DW_EXIT` marker 缺失 ∧ capture 三形态关联确认 `DW_SPAWN` 后 `:vpn` 行静默 ∧ `process_death_observed = observed-true`（死亡分量，唯一死亡谓词源）
      ∧ 死亡位点 ∈ P8-P10 窗口（`DW_SPAWN` 之后、无 `DW_EXIT`，证据规则 4）
      ∧ **poll 进入证据 = `dw_inwait_confirmed = observed-true`（仅此一项，r15 收紧，sol B-03）**——BARRIER 不再作为本支证据（弱口径见本节 in-wait 证据候选段的 barrier 定位句——r16 改锚，原 `:668` 行号漂移：其只证到达 poll 调用点、可在调用前被抢占、单独永远不证进入内核等待；
inwait true 只能在 BARRIER 后取得，原「BARRIER∨inwait」在合法 trace 上退化为 BARRIER——BARRIER→poll 调用间隙的死亡会被本支错记「已确认进入等待」；BARRIER 在而 inwait 非 true → 落 ⑤）→ **`observed-true`**（**waiter 于 destroy 未及窗内死亡且已确认进入等待**——r14 去因果化：本字段的 true 语义是「waiter 于窗内死亡且等待进入的证据在」，
      **不构成 watchdog 归属的证据**：SIGKILL/OOM/生命周期终止在该观测下不可区分，正例构造中的 SIGKILL 条目仅是死亡分量的关联证据；字段名 `dw_watchdog_killed` 保留为 r3 E11 冻结的历史名，改名波及 POST 冻结字段集与 ledger，N1b 不得将该 true 引用为「平台 watchdog 杀了 poll waiter」；
      无 poll 进入证据（inwait 未确认——drain 期死亡 BARRIER/inwait 均无；r15 起含 BARRIER 在而 inwait 非 true 的间隙死亡）→ 落 ⑤，宁缺勿误；①的「`_C` 在 + absent」构造已先行分流、不与本支重叠）；
      （**r12 沿革注**：第四项死亡分量合取 r12 定型，blocker 2（sol B-03）——原第四项〔r4 第二趟 S8 收紧、r9 第五步再收紧：独立死亡证据 = faultlogger 条目含 `APPFREEZE`/watchdog 类签名〕与 r10「事件/死亡两轴分立」正面冲突：签名条目只证明 watchdog**事件**、不证明进程消失，`pidof` 仍非空时会虚构「waiter 被杀」，故死亡状态唯一取自 `process_death_observed` 分量、签名条目仅作关联证据逐字引用。
      **前三项不蕴含死亡的辨析（r12 写明，r13 沿用）**：第三项「`DW_SPAWN` 后静默」是**事件后静默**（capture 三形态关联后无新增 `:vpn` 行），只证窗内无新行、不证进程消失，故死亡分量不可省。
      **r13 废除原 r9 第五步「`PidOfVpn` absent 且 `_C` 缺一律不得 true」**——「调用中途死亡」与「waiter 于窗内死亡」在 `_T`/`_C` 均缺时以死亡位点窗口区分：位点 ∈ P8-P10 即 waiter 等待窗、位点在窗外归⑤；r14 再加 poll 进入证据合取，drain 期死亡自此归⑤。）
    - ④ `N1BDISC_DW_EXIT` marker 存在 → `observed-false`（waiter 正常走到终态并退出，无论 poll 以何类返回——被杀的线程不可能发出 EXIT 后再死；**r4 第二趟 S7：删除旧文「D-W 因 skip 分支未执行亦判 observed-false」的规定——skip 情形一律具名 `unobservable`，skip 主线由此有唯一字段值，RESULT 不因域外值 fail**）；
    - ⑤ 其余一切输入 → `unobservable(cause=marker-gap-indeterminate)`（原默认支；含死亡分量未确认为 `observed-true`（`process_death_observed != observed-true`，r12 统一死亡谓词）、`DW_SPAWN` 缺、死亡位点在 P8-P10 窗口外、`_T`/`_C` 均缺但五合取不齐（含 **r14：无 BARRIER 且 inwait 未确认的 drain 期死亡；r15：BARRIER 在而 inwait 非 true（sol B-03）**）、`_T` 在 `_C` 缺但 EXIT 在（④ 先承载）——禁止以 marker 缺失单独推断被杀，E3 0001 教训）。
    **r13 SIGKILL 证据角色辨析（sol 点名；r14 更新）**：③正例构造中的 SIGKILL 条目仅是死亡分量的**关联证据**（r12 U2 统一后的既有规则：携 `SIGKILL`/`SIGTERM` 的终止条目只在 pidof absent 确认下作并存关联证据、不独立充分），**不证明 watchdog 归属**——r14 起 ③ 的 true 语义已去因果化（「于窗内死亡且已确认进入等待」），watchdog 归属**无任何机器可判证据**；
      faultlogger 的 `APPFREEZE`/watchdog 签名条目同理只是关联证据（只证 watchdog 事件、不证进程消失，①构造中签名在否不改变落值）——写明防止「SIGKILL ⇒ watchdog」的误读；
    - **skip 分支具名清单（S7：全部 skip 情形不得取 true/false；r14 更正 deepseek M-02：原第三字面 `no-worker` 删除——skip 表与 P10 行只有 `no-live-fd`/`dup-failed` 两种 D-W skip 情形，协议不存在第三种「worker 创建失败」skip 分支，该字面系 r3 第二趟 S7 清单多列的幽灵引用，从未有定义源）**：`unobservable(cause=no-live-fd)` / `unobservable(cause=dup-failed)`——对应 skip 分支（skip 表与 P10 行的 cause 字面）；r13 注：该清单位于有序表外，skip 路径直接落 skip 编码、不进入①-⑤求值。
    死亡分量未确认为 `observed-true`（`process_death_observed != observed-true`，r12 统一死亡谓词，原「无死亡证据」自立表述随 blocker 2 收敛）时**默认取 `unobservable`（⑤支承载），默认值不得为 observed-true**。本字段是路径①第 5 条件（`dw_watchdog_killed != observed-true`）的唯一定义来源，该条件据此可独立实现）、
  - `dw_destroy_distinguishable_from_timeout`（派生三态，**四步有序互斥（r15，原 r13 二维求值序声明改述——历史沿革注保留：r13，blocker 3，sol B-04——原 r12 把 17 个 `dw_return_class` 值与 destroy 超时/reject 2 结局按互斥枚举平级相加，
但两维正交：worker 先落 `pre-destroy-ready`、随后 destroy 10 s 未 resolve 的合法 complete trace 同时命中 no-destroy-correlated-event 与 destroy-unresolved 两行，`fd-event-like` + destroy timeout 又因缺 resolve 不满足 true 支、却被原 2 结局桶的排除句排除——合法 trace 无唯一落值；
r12 的逐值具名 cause 冻结沿袭不变（blocker 7，sol B-07——原裸 `unobservable` 不带 cause，违反 r9 字面拼法冻结的 `cause=` 逐字比对规则，complete 主线（destroy resolve + `pre-destroy-ready`）落「其余」时 POST 无合法单值））**：
    - **四步有序互斥（r14，blocker 1，grok B-01 = sol B-03 两席收敛——原 r13 二维声明下，「有 `DW_RETURN`、无 `_C`、无 resolve」的合法平台死亡两维均不命中、派生字段无落点 → F8 烧掉 PRE 后合法平台终止；「已发起而无 resolve 证据」措辞又在 E3 方向（`_C` 在、无 RETURN、无 resolve）与透传行两读。求值序先到先得、命中即终止，每步接住前步未命中的输入）**：
      - **(0) 直接透传**——`dw_return_class` ∈ {skip 编码 2 值 ∪ pre-only 死亡收口 3 值 ∪ r17 complete∧join-timeout 收口 1 值 `unobservable(cause=poll-never-returned)`} → 输出逐字等于输入编码本身（destroy 未被调用/未达、判定输入不可得、或 worker 未返回致 resolve 两维前提不成立——r17 新值沿 skip/死亡收口同款理由，resolve 与否无从谈起）；
      - **(1) 类 0/0b 直接映射**——`destroy-skip-proven` → cause 逐字沿实际发出的 `N1BDISC_SKIP|item=destroy` 位点字面（预注册 `no-live-connection`/`barrier-never-observed`，与 V2 (1b) 清单同一）；`destroy-call-unobserved` → `unobservable(cause=destroy-call-unobserved)`——**两类不入任何 resolve 分区**：`_C` 缺、resolve 证据不可得是这两类路径的常态而非异常，类 0b 的输入恰恰要求「有 RETURN、无 SKIP、无 `_C`」（判定表优先级 0b），把它收进「仅 resolved」的第二维正是原洞的成因（历史沿革句——现行口径为四步，r17 注）；
      - **(2) `_C` 在且无 resolve 证据** → 一律 `unobservable(cause=destroy-unresolved)`（复用 V2 分岔既有字面；**「已发起」的机器判据钉死为 `_C` 在**、与五态 `call-returned` 对齐——r14 废除「已发起而无 resolve」的可扩域措辞，防误引注见此；超时与 reject 同 cause、结局差异逐字入 raw 档）；
      - **(3) destroy 已 resolve** → 按下方逐值表落值——`fd-event-like` → `observed-true`（destroy 在 10 s 内 resolve 且返回类为 fd 事件，唯一归因类）；`timeout-like` → `observed-false`（返回类即自身超时）；「其余 11 类中的普通 9 类」→ 逐值具名 cause（见下 (步 3) 各行）；本步的求值前提一律为「destroy 已 resolve」。
      **对齐注（防混淆）**：类 0b `destroy-call-unobserved`（判定表类：有 RETURN、无 SKIP、无 `_C`）与死亡收口 cause `call-boundary-incomplete`（五态 `_T` 在 `_C` 缺、判定输入不可得）是 19 值域中**两个不同的值**（r17 计数同步），输入域不同，本表内前者走 (1)、后者走 (0)，无交叠。
      **不可达性注（r15 重写——原 r14 版主论证错误，sol B-01 揭出：原称「判定表优先级 0b 截获一切无 `_C` 输入」，但未知位前置门在表之前直接定类 `other-revents`、被路由的输入根本不进表，0b 从未见过它们——「`other-revents` + `_C` 缺 + 无 resolve」曾真实可达并构成 F8 烧 ID 的洞。r15 派生序重排后本注的正确论证）**：
      「普通类（步 3 域）+ `_C` 缺 + 未 resolve」结构不可达——**0/0b 前置检查先于未知位门**（派生序冻结），一切「无 SKIP 且 `_C` 缺」的输入（含未知位值）在未知位门之前归 0b；未知位门只对**无 SKIP 且 `_C` 在**的输入分流 `other-revents`（r16，deepseek M-01：SKIP 在的输入已被前置检查路由类 0、到不了本门）；行 4/5/7/8 的 `T/C` 比较依赖进一步坐实「普通类的存在蕴含 `_C` 在」。故「无 resolve」的普通类必落步 (2) 而非两步间的缝。
枚举与 `dw_return_class` 域闭合（r14 改述；r17 计数更新：19 值按步分布——(0) 承接 skip 2 + 死亡收口 3 + `poll-never-returned` 1、(1) 承接类 0/0b、(3) 承接普通 11 类含 true/false 两判定类，步序穷尽全部 19 值与「`_C` 在无 resolve」这一非 class 维度，无遗漏、无交叠、不留兜底桶）：判定表 13 类 + skip 编码 2 值 + pre-only 死亡收口编码 3 值 + r17 `poll-never-returned` 1 值 = 19 值；
    - **(步 3) destroy 前就绪/非归因类 5 值**——`pre-destroy-ready` / `interrupted` / `poll-error` / `fd-invalid` / `spurious-early` → **`unobservable(cause=no-destroy-correlated-event)`**（poll 返回未与 destroy 相关联：destroy 前就绪或 ret/errno/revents 类结局，均非 destroy 区分证据）；
    - **(步 3) destroy 后非归因类 4 值**——`late-fd-event` / `late-data` / `data-ready-post-destroy` / `other-revents` → **`unobservable(cause=destroy-uncorrelated-class)`**（返回晚于 destroy 调用但判定表明文不归因——观察类不支撑归因，本字段不因晚返回给 true）；
    - **(步 1) 类 0/0b 2 值**——`destroy-skip-proven` → cause 逐字沿实际发出的 `N1BDISC_SKIP|item=destroy` 位点字面（预注册 `no-live-connection`/`barrier-never-observed`，与 V2 (1b) 清单同一）；`destroy-call-unobserved` → **`unobservable(cause=destroy-call-unobserved)`**（r9 既有固定 cause 沿用：`_C` 缺、destroy 调用边界不可观测；原固定 cause `destroy-never-called` 废除不变，缺 `_C` 不再等值「未调用」）——判定在步 (1) 一次完成，此处重列仅为 19 值逐值完整（r17 计数同步）；
    - **(步 0) skip 编码 2 值**——`unobservable(cause=no-live-fd)` / `unobservable(cause=dup-failed)` → 本字段输出逐字等于该输入编码本身（r15 理由拆分，sol m-01——原笼统句「skip 路径无 destroy 求值输入」对 dup-failed 不实：`no-live-fd`：destroy 未被调用（无 connection 可 destroy）；`dup-failed`：destroy 经共享子协议执行、可有 resolve/reject/timeout——但**无 waiter**（dup 失败、未 spawn），destroy 结局与本字段（waiter 可区分性）无判定关联，透传仍正确）；
    - **(步 0) pre-only 死亡收口编码 3 值**（r12，sol M-01——原单值 `post-destroy-unobservable` 按死亡时 `destroy_call_state` 拆分透传，逐值表由 1 行拆为 2 支；r13 第三包（sol M-01）再拆 1 支为 3 支）——
五态 `not-reached`（死于 destroy 位点前，如 D7/P7）→ 逐字透传 `unobservable(cause=destroy-not-reached)`（复用 V2 (1a) 既有字面、跨域共用）；
五态 `call-returned`（destroy 已过而判定输入不可得——无 `DW_RETURN`/resolve 证据可据）→ 逐字透传 `unobservable(cause=post-destroy-unobservable)`；
五态 `call-boundary-incomplete` → 逐字透传 `unobservable(cause=call-boundary-incomplete)`（**r13 第三包，sol M-01：五态定义明言该态「既可能尚未调用，也可能已进入调用」，「destroy 已过」对它不成立——r12 把它与 `call-returned` 同记 post-destroy 是事实错误，改沿 `dw_watchdog_killed` r13 有序互斥表 ② 同款透传自身 cause**）。
    - **(步 0) complete∧join-timeout 收口编码 1 值**（r17 新增；**r18 注：分域 (e)「标志已置∧capture 无 RETURN」为 fail(F8(2)) 收口、不赋 class 值，不入本表——本表只收可赋值形态**）——`unobservable(cause=poll-never-returned)` → 本字段输出逐字等于该输入编码本身（worker 未返回、`_C`-resolve 两维前提均不成立，沿 skip/死亡收口同款理由；非 skip、进程活到 P12、`join-timeout` 已登记、`DW_RETURN` 缺四前件见分域声明 (d)）。
    （原 r12 第 6 支「destroy 超时或 reject 两种非 class 结局 → destroy-unresolved（`fd-event-like` 与 `timeout-like` 两类归入前三态，不在本桶）」随 r13 废除——其排除句正是 17+2 正交相加下 `fd-event-like`+timeout 无落点的成因；destroy-unresolved 自 r13 起由 destroy-结局分区承载（r15 起为步 (2)）、覆盖全部 class。）
**全部结局均为观察事实，不设 pass 条件**（决议 §三.3）。
- **「destroy 关闭原始 fd 不得预设为必然唤醒 dup 副本上的 poll」**（决议 §三.4，`docs/native-nx-n1b-adjudication.md:84`）：唤醒与否即待发现事实，由 `dw_return_class` 承载；协议对两种结局对称收口。
- **共用同一 destroy 事件**：全程恰一次 `destroy()`，即 P9 位点的那一次（D6 与 D-W 共用），不得二次 destroy；P9 之前（含 D2 迟到 fd 处置）禁止任何 `destroy()` 调用（决议 §三.3；C1 冻结；r9：skip 表 `dup-failed` 分支的 destroy 执行同为该 P9 位点这一次，经「共享 destroy 子协议」发出 `_T`/`_C`，「恰一次」按调用点计，不因分支复用变多）。
- **路径①/②采认门槛（OB-03，决议 §三.5 预授权的行使规则；r1 按主会话补充要求冻结；第二趟修订按决议 §三.5-①「destroy 语义**可与自身 timeout 区分**」补全为**六者合取，缺一不可**——原三条件无法排除「poll 因自身 5000 ms 超时返回，返回时刻同样晚于 destroy 调用」的假阳性解锁；任一字段缺失即该条件不成立**：
  1. `dw_inwait_confirmed == observed-true`（destroy 前 proc 自省直接证实 worker 已阻塞于 poll）；
  2. `dw_return_class == fd-event-like`（唯一归因类；poll 自身超时返回 revents 为空、归 `timeout-like`，由此承载「可与自身 timeout 区分」）；
  3. worker `poll` 返回时刻 `at_mono_ms` 在单调时钟上**严格晚于** destroy 调用时刻 `destroy_call_mono_ms`（**r9 删除旧文「两侧均有 marker 锚点：`N1BDISC_DW_RETURN` 与 `N1BDISC_DW_DESTROY_T`」——它与同句「取 `_C`」互斥，不得让实现者自行选边（规范稿 §4.2 必须删除条款的落地）**；
`destroy_call_mono_ms` 一律 = `N1BDISC_DW_DESTROY_C` 的 `mono_ms`（r8 Y5 定义式，r9 定为唯一口径）；路径①只接受严格的 `at_mono_ms > C_mono_ms`，即三分带的 `definitely-post-invocation`——毫秒等值不能证先后，`at_mono_ms == C_mono_ms` 与 `T_mono_ms ≤ at_mono_ms ≤ C_mono_ms` 歧义带（`invocation-window-ambiguous`）**均不解锁本条件**；`_C` 缺失时本条件不可求值、路径①同样不解锁，该输入由判定表类 0b `destroy-call-unobserved` 收口，不产生 criteria-gap）；
  4. drain 以 EAGAIN 正常终止（`dw_drain_end == eagain`；r3 第一趟 D3 钉死为 `end=` 字面——旧括注 `dw_drain_timeout == false` 在 `end=zero-read`/`end=errno-<n>` 时同为 false，与中文「已到 EAGAIN」不等价）；
  5. `dw_watchdog_killed != observed-true`；
  6. `N1BDISC_DW_RETURN` marker 存在。

  任一条件不成立 → 「waiter 可被 destroy 唤醒可测」的结论至多 `unobservable`，路径①不可解锁。
`timeout-like` / `data-ready-post-destroy` / `pre-destroy-ready` / `spurious-early` / `fd-invalid` / `interrupted` / `poll-error` / `late-fd-event` / `late-data` / `other-revents` / `destroy-skip-proven` / `destroy-call-unobserved`（r9 拆分，原 `destroy-never-called` 废除）**一律不解锁路径①**，全部走路径②（义务不删除，按决议 §三.5-② 转移 N6/E7 并书面登记；
r9 第五步：skip 编码两值 `unobservable(cause=no-live-fd)`/`unobservable(cause=dup-failed)` 同不解锁——skip 路径无 `N1BDISC_DW_RETURN` marker，条件 6 本不成立，无路径①/②判定面；
r10：pre-only 死亡收口编码（r12 拆分为 `unobservable(cause=post-destroy-unobservable)` 与 `unobservable(cause=destroy-not-reached)` 两值；r13 第三包再拆出 `unobservable(cause=call-boundary-incomplete)` 透传值，合计三值）同不解锁——该组值仅当判定输入不可得时登记，`DW_RETURN` marker 不在（条件 6 不成立）、`fd-event-like`（条件 2）不可得，无路径①/②判定面；r17：`unobservable(cause=poll-never-returned)` 同不解锁——`DW_RETURN` 不在（条件 6 不成立）、无 poll 返回输入（条件 2/3/4 均不可得），无路径①/②判定面）。
**弱 barrier（`dw_entry_confirmed`）单独永远不构成路径①资格。**`dw_destroy_distinguishable_from_timeout` 与条件 2 同源（同以 `dw_return_class` 为判定输入），仅作 OB-03 台账登记用派生观察字段，不替代上述资格判定。
- **决议约束正面登记（非自陈，供记录级审查与后续 T0 处置）**：决议 §三.3 要求 worker「于 destroy 之前经预注册 barrier/marker **确认进入**对 dup 副本的有界等待」（`docs/native-nx-n1b-adjudication.md:83`），§三.5 路径①资格条件同为「**可确认进入等待**」（`:86`）——但决议**未论证该确认在用户态是否可达**。
本 campaign 的 in-wait 证据候选（`/proc` 同进程自省）正是该可达性的实测载体；若实测 `dw_inwait_evidence_source=unreadable`（或 barrier-timeout）且无其他用户态手段，则**路径① 在本元组结构上不可达**，OB-03 只能走路径②。该约束来自决议对「确认进入等待」的要求本身，**不是本 campaign 的实现选择**；
如实落盘，以便记录级审查与后续 T0 在知情前提下处置（例如另行设计可自证的等待原语，或对路径① 显式放弃）。
- **watchdog 暴露窗口（正面论证）**：窗口 = [barrier 确认, worker 终态收口]。若平台 watchdog 因「存在阻塞在 poll 的第二线程」杀进程，D6 事实将与 waiter 一并损失（终裁自陈的已知风险，`docs/t0-n1b-discovery-materials.md:65`）。缓解（冻结）：
  1. **窗口最小化**：drain ≤5 s；barrier 等待 ≤7 s（= drain 盒 + 调度裕量，见 D-W 节）；in-wait 证据采集 ≤2 s；destroy 时间盒 10 s（非 60 s——create 才是 60 s）；D6a 为约 3 次即返 syscall；终态轮询 ≤8 s。名义窗口 ≈ T_dw(5 s)+ε ≈ 6-10 s，最坏 ≤27 s 量级。
  2. **位次最后**：D-W/D6 排在全部 fd 相关项（D4/D5/D8a/D8b）与 D7 之后，中窗死亡的最小损失集合就是 D6/D-W 自身——这正是需要正面承受的发现项（watchdog 对 waiter 的行为本身就是 U7/路径②的事实）。
  3. **归因洁净**：在 worker poll 终态（返回或 T_dw 到期）之前，主线程**不触碰 `fd_dup`**（无 `F_SETFL`/`read`/`close`——`F_SETFL(O_NONBLOCK)` 已在 D2.5 完成；drain 在 barrier 之前、由 worker 自身执行），保证任何提前唤醒只可归因于 destroy（或落入 `pre-destroy-ready`/`data-ready-post-destroy`/`spurious-early` 等非归因类）。
  （**r18 恢复为活规则**：本条窗口在 poll 挂死形态下永不闭合、主线程对 `fd_dup` 的禁触随之不因 join-timeout 而解除——r17 位次裁定③（Z4）曾以「join-timeout 后继续 D6b」与本条矛盾；该裁定经 grok B-01 = sol B-02 两席驳回、主会话撤回，join-timeout 形态 D6b 整段 skip（见 D6 节 D6b 段 r18 重裁），本条自此对全部形态无例外成立。）
  4. **终态路径（预注册，r4 R1 措辞更新；r9 触发器改按 F1 闭集）**：若进程在任意位点死亡，runner 依「capture 静默 + 死亡证据」双条件 + 「死亡事实记录（证据向量）」节七分量逐项登记收口（不合成任何死因归因）：
     - `pre-only`/`complete` 收口可 pass——死亡侧唯一 fail 触发面 = F1：`probe_crash_signature_observed = observed-true` 且 `protocol != complete` → verdict `fail`（三支闭集见「死亡事实记录（证据向量）」节；`protocol=complete` 不进 F1，命中崩溃签名仅按 verdict 节登记入 runner evidence 记录（需 N1b 关注的异常观察，r10 载体修正））；
     - 除 verdict 节 fail 闭集（F1–F6、F8、F9；F7 不在内）外一切结局均不 fail；
     - 已增量落盘事实保留；`N1BDISC_PRE` 已承载事实照常入账。
  5. D7 先于 D-W 的排序不改变 D-W 自身风险（无论先后，waiter 暴露窗口同样存在），故维持决议裁定顺序、以上述 1-4 缓解。

### D6 destroy 后同步尝试（U4，逐子项）

- **触发**：`destroy()` resolve 后**同一调用栈内**立即执行（ArkTS resolve 回调 → native 序列）。进程可能立刻 terminal（E3 先例，`docs/n1b-gate-plan.md:63`），故每步**先发 pre-marker 再执行**：「该段代码是否执行到」由 pre-marker 存在性机器判定，结果由 result-marker 判定。
- **D6a（orig 面，destroy resolve 后立即、同栈）**：

| 步 | pre-marker | 动作 | result-marker |
| --- | --- | --- | --- |
| 1 | `N1BDISC_D6S1_B` | `fcntl(fd_orig, F_GETFD)` | `N1BDISC_D6S1_R|ret=<n>|errno=<e>` |
| 2 | `N1BDISC_D6S2_B` | `fcntl(fd_orig, F_GETFL)` | `N1BDISC_D6S2_R|ret=<n>|errno=<e>` |
| 3 | `N1BDISC_D6S3_B` | `close(fd_orig)`（destroy 已履责后的双 close 探测） | `N1BDISC_D6S3_R|ret=<n>|errno=<e>` |

- **D-W 采集**：D6a 后有界轮询 worker 终态（≤8 s），汇总 `dw_*`（见 D-W 节）。
- **D6b（dup 面，worker 终态之后执行；join-timeout 形态（终态轮询盒到期、worker abandoned）整段 skip——r18 重裁，见下）**：
  **r18 重裁（grok B-01 = sol B-02 两席驳回 r17 位次裁定③，主会话撤回——审查登记九之八节终账第 1 条）**：r17 裁定「D6b 的 fd 操作对象是 `fd_dup`（非 worker 持有的原 fd）」前提反事实——worker drain/poll 的对象就是 `fd_dup`（D-W worker 序列 `poll(fd_dup, POLLIN, 5000ms)`），abandoned worker 仍阻塞在该 poll 上；
  撤回理由（续）：主线程 D6b 对同一 fd 的 read/close 副作用使 (d) 支「`DW_RETURN` 缺」前件非单值——close 唤醒 → 晚到 RETURN → 前件失败；不唤醒 → (d) 成立，class 随平台分叉；且 D6b read 会偷走 destroy 本应留在 `fd_dup` 上的 POLLIN/HUP，污染本 campaign 的核心发现目标（destroy 是否唤醒 dup 上的 poll）。
  **r18 新裁定**：join-timeout 形态下 **D6b 整段 skip**——对仍可能被 poll 的 `fd_dup` 不做任何操作（fcntl GETFD 亦 skip，与 read/close 统一处理，不引入「哪些操作安全」的新判断面）；D6b 各子项字段（`u4_dup_getfd`/`u4_dup_read`/`u4_dup_close`/`u4_dup_fd_reuse`）按「worker 终态未确认 + join-timeout 已登记」收口为 `unobservable(cause=d6b-skipped-join-timeout)`（新具名 cause，一条覆盖 D6b 全部子项——物理原因同源：abandoned worker 的 `fd_dup` 生命周期未定，主线程不得触碰）。
  skip 发 `N1BDISC_SKIP|item=D6b|cause=join-timeout-abandoned`（`cause` 无封闭枚举域、按位点逐字预注册——本裁定把 `item=D6b` 与 `cause=join-timeout-abandoned` 预注册入 skip 位点清单，沿 r9 第五步 barrier-never-observed 同款机制）；`close(fd_dup)` 交由进程退出回收（沿既有「worker 由进程退出回收」表述——`:vpn` 进程 terminal 时进程持有的 fd 由内核关闭，E3 先例）。
  POST `d6_items` D6S4..S7 各子项按三选一编码 `skipped(cause=join-timeout-abandoned)` 落值（skipped 支 cause 域同步扩入该字面，见「终态 marker 双 marker 结构」节 POST 子项三选一编码 r18 条）；`u4_dup_*` 四字段按上方新具名 cause 落值。
  **r17 位次裁定（sol B-01 尾部；r18 部分撤回）**：abandoned worker 永不达终态，`join-timeout` 的登记即主线程侧的终态替代事实——该半句保留（P10 到期后放行 P11/P12 的依据不变）；「D6b 与 worker 存活无冲突」「join-timeout 后继续 D6b」两句随裁定③撤回废除。

| 步 | pre-marker | 动作 | result-marker |
| --- | --- | --- | --- |
| 4 | `N1BDISC_D6S4_B` | `fcntl(fd_dup, F_GETFD)` | `N1BDISC_D6S4_R|ret=<n>|errno=<e>` |
| 5 | `N1BDISC_D6S5_B` | `read(fd_dup, buf, 2048)`（dup 自 D2.5 起非阻塞，天然有界；**禁止无界阻塞 read**） | `N1BDISC_D6S5_R|ret=<n>|errno=<e>`（`<n>`/`<e>` 为数字占位：成功时 `errno=0`，失败时 `ret=-1`——**占位冻结，不得空置**） |
| 6 | `N1BDISC_D6S6_B` | `close(fd_dup)` | `N1BDISC_D6S6_R|ret=<n>|errno=<e>` |
| 7 | `N1BDISC_D6S7_B` | `socket(AF_INET, SOCK_DGRAM, 0)`，登记新 fd 号是否等于已关闭的 `fd_dup` 号 | `N1BDISC_D6S7_R|fd=<n>|reuse=<bool>`（fd 号复用观察） |

- **U4 逐子项赋值（MJ-5）**：`u4_orig_getfd`（步 1）、`u4_orig_getfl`（步 2）、`u4_orig_close`（步 3）、`u4_dup_getfd`（步 4）、`u4_dup_read`（步 5）、`u4_dup_close`（步 6）、`u4_dup_fd_reuse`（步 7），
各三态：result-marker 存在且 ret 已登记 → `observed-true`（该子项 destroy 后同步可观察）；pre-marker 存在 + 死亡证据 + 零 result-marker → `observed-false`（该子项同步不可观察）；无 pre-marker / 步未执行 / marker 缺失且无死亡证据 → `unobservable(cause=…)`（防 E3 型误判；`observed-false` 关系 OB-04 分支走向，宁缺勿误；**r4 R1 特例、r5 U6 重写：`protocol=pre-only`（POST 缺失且有死亡证据）时，死亡前已有 result marker 的子项保持记录值、死亡后未执行子项按 verdict 节 V2 四支分岔赋值，本段禁止再给默认极性（r10，A M-01）**）。
摘要 `u4_post_destroy_sync_observable`：任一子项 `observed-true` → `observed-true`，否则按子项最弱值——「最弱值」全序冻结为 **`unobservable` < `observed-false` < `observed-true`**（摘要取全部子项在该全序下的最小值；理由：`unobservable` 表示该子项证据不存在、可判定性最低，`observed-false` 是一次有效的负观测，`observed-true` 是正观测——证据确定性依次增强），
故任一子项 `unobservable` 且无 `observed-true` → 摘要 `unobservable`；全子项 `observed-false` 且无 `observed-true` → 摘要 `observed-false`——**仅摘要，不得替代逐子项**；r2 划定 C10 可观察子集（决议 §六.3-b）时逐子项取用，禁用摘要。
- **时间盒**：各步即返 syscall（无等待面）；destroy 10 s；终态轮询 8 s。

### 无 fd / dup 失败分支（冻结，三席全中项）

矩阵终局与 2.2 结果决定分支；两分支下全部 skip 均发 `N1BDISC_SKIP|item=<id>|cause=<cause>`，全序不破坏：

| 位点 | `no-live-fd`（矩阵无保留条目：全 rejected / timeout 终止 / 窗尽 indeterminate） | `dup-failed`（保留条目存在、2.2 失败） |
| --- | --- | --- |
| D4 | skip；`u1`/`u3`/`u1_no_route_control`/`foreign_packets_observed` 全 `unobservable(cause=no-live-fd)` | skip；同左，cause=`dup-failed` |
| D5 | skip；`u2` 与逐轮字段 `unobservable(cause=no-live-fd)` | skip；cause=`dup-failed` |
| D8a/D8b | skip；`d8_write_boundary_last_success_len` 等全 `unobservable(cause=no-live-fd)` | skip；cause=`dup-failed` |
| D7 | **skip**；`u7=unobservable(cause=no-live-vpn)`——**无 live VPN 时 U7 不得赋 true/false** | **执行**（VPN 仍 live；D7 零 fd 依赖） |
| D-W | skip；`dw_*` 全 `unobservable(cause=no-live-fd)` | skip（waiter 对象是 `fd_dup`）；`dw_*=unobservable(cause=dup-failed)` |
| destroy | **不调用**（无 connection 可 destroy；对不存在对象盲调违禁），发 `N1BDISC_SKIP|item=destroy|cause=no-live-connection`（r9：该 SKIP 即 `destroy_call_state=not-called` 的唯一证明途径，见五态表） | **执行**（唯一一次；create 成功即有 live connection，destroy 是其清理义务；r9：经「共享 destroy 子协议」（定义见 D-W 节）执行——依次发 `DW_DESTROY_T` → 调用 → `DW_DESTROY_C`） |
| D6a | skip；`u4_orig_*=unobservable(cause=no-live-fd)` | 执行（对象 `fd_orig`） |
| P10 终态轮询与 D-W 汇总 | **skip**（无 worker 可轮询：发 `N1BDISC_SKIP|item=D-W|cause=no-live-fd` 后**直接进入 P11，不产生任何 join 等待**）；`dw_*` 全 `unobservable(cause=no-live-fd)`（D6b 一并跳过：`u4_dup_*=unobservable(cause=no-live-fd)`） | **skip**（无 worker：发 `N1BDISC_SKIP|item=D-W|cause=dup-failed` 后直接进入 P11，不产生 join 等待）；`dw_*=unobservable(cause=dup-failed)`（D6b 一并跳过：`u4_dup_*=unobservable(cause=dup-failed)`） |
| P11 探针 fd 清理 | 对已创建项执行（矩阵阶段无探针 fd 创建则登记 `none`） | 执行 |
| 主线路径 | 矩阵终局 → **P5T PRE**（r5 U4：skip 主线补 PRE 位点——P5T 先于 D7，本线在 P5 后即发 PRE）→ skip destroy（发 `N1BDISC_SKIP|item=destroy|cause=no-live-connection`）→ 跳过 P10（无 worker，发 SKIP）→ P11 → P12（**编号与「冻结全序」表 P5T/P10/P11/P12 同一，禁止「直跳 P10」式跨表指称**） | skip D4/D5/D8a → **P5T PRE**（r6 W1：P5T 先于 D7，防 PRE 缺失 fail）→ D7 执行 → skip D8b/D-W → destroy 执行 → D6a 执行 → 跳过 P10（发 SKIP）→ P11 → P12 |
| U7 | `unobservable(cause=no-live-vpn)`，**不得 true/false** | 正常三态 |

## 冻结全序与时间盒

barrier 之间不得重排；任一 D 项按其前置缺失规则跳过时，跳过本身照 marker 登记（`N1BDISC_SKIP|item=<id>|cause=<cause>`），全序不破坏：

```
P0  操作员 ready 确认 -> 操作员 Allow -> onCreate（r3 第二趟 E6：StartEntry 之前有可审计 operator-ready 确认步，见下；Allow 等待独立时间盒 300 s，不计入观测窗）
P1  D1  dlopen + dlsym（Extension 进程）
P2  D2  矩阵逐条 create（含 timeout 仲裁/迟到窗）；保留条目锁定 + dup + 初始 flags + O_NONBLOCK
P3  D4  socket->tun 投递（U1）+ 首帧 dump（U3）
P4  D5  tun 写入 -> sink（U2）
P5  D8a MTU 写返回谱阶梯（10 级轻量写）
P5T N1BDISC_PRE 终态 marker（承载 P1-P5 全部已得事实 + fd ledger P5T 快照 digest；发射点 = P5 D8a 阶梯完成之后、P6 D7 开始之前，先于 P8 D-W/P9 destroy()/P10 pthread_join——r5 U1 自原 P8T 位点前移）
P6  D7  20 s 冻结负载长同步任务（VPN 仍 live）
P7  D8b storm（10 s / 4 MiB / 50 000 写，熔断；缩减后移）
P8  D-W worker drain -> barrier -> 进入有界等待
P9  destroy()（唯一一次，与 D6 共用）-> resolve 后同栈 D6a
P10 worker 终态有界轮询 -> pthread_join（PRE 已于 P5T 发出——r4 R6c 保障在 r5 U1 前移后更强：join 位点在 PRE 之后不变）-> D-W 汇总 -> D6b（worker 终态后执行；join-timeout 已登记、worker abandoned 时整段 skip——r18 重裁：发 `N1BDISC_SKIP|item=D6b|cause=join-timeout-abandoned` 直入 P11；该登记即终态替代事实、放行依据保留）
P11 探针自有 fd 清理（d4_send_socket/d5_sink_socket/d6b_reuse_probe_socket 逐个 close 并以 F_GETFD 复核 EBADF；未创建项登记 not-created）
P12 N1BDISC_POST 终态 marker（D6 逐子项 + D-W 结局 + ledger 最终 digest，r4 R1）-> 封签 -> 进程自然退出（观察，不判定）
```

**每个等待点均有单调时钟有界时间盒**（超时分类预注册；**唯一豁免 = `pthread_join`**：按第二趟修订集群 A5(b) 裁定 join 不可有界——仅限终态标志置位后调用，其阻塞无用户态超时出口，由观测窗到点与 host finally 收口兜底，`join-blocked-observed` 由 runner 依轮询超时登记，见 D-W 节「有界 join 可行性裁定」）：

| 位点 | 时间盒 | 超时分类 |
| --- | --- | --- |
| P0 Allow 等待 | 300 s（自 `StartEntry` HDC 命令返回起，至 capture 出现首个 N1BDISC marker）；前置与超时分类外提为表后「P0 行外提」子项（r4 第四趟 W1 重构，内容逐字不变） | 同上外提 |
| P1 dlopen+dlsym | 10 s | 错误返回=observed-false 事实；未返回→死后收口路径（死亡事实记录（r9：原「死因分类」随死因表删除更名），见「死亡事实记录（证据向量）」节；现行 protocol 取值为 `complete`/`pre-only`，`post-mortem` 仅存于历史登记） |
| P2 每条 create() | 60 s | 候选记 timeout，进入迟到观察窗 |
| P2 迟到观察窗 | 60 s（全矩阵恰一次） | 窗尽→indeterminate，矩阵终止走 no-live-fd 分支 |
| P3 D4 窗口 | 总 10 s / 每 poll 500 ms | 三态收口 |
| P4 D5 窗口 | 总 10 s / 每轮 500 ms / ≤5 轮 | 三态收口 |
| P5 D8a 阶梯 | 每级 poll 500 ms + write（≤1 s/级，总 ≤10 s） | 逐级登记 |
| P5T N1BDISC_PRE 发射 | 即返（单条 marker 落盘；r5 U1 自原 P8T 行前移至本行，观测窗预算占比不变） | 未发出 → PRE 缺失完整性失败路径（verdict=`fail`，r4 R1） |
| P6 D7 任务 | 20 s + 5 s 宽限 | 任务即测量 |
| P7 D8b storm | 10 s / 4 MiB / 50 000 写（熔断） | 命中即停登记（`attempted, not induced on this fd` 口径），非 fail |
| P8 drain | 5 s | 超时登记 `timeout=true`，协议继续 |
| P8 barrier 等待 | 7 s（drain 盒 5 s + 调度裕量 2 s） | 超时→dw_entry_confirmed=unobservable，跳过 in-wait 采集；**destroy 顺延**（不得早于 `DW_BARRIER` marker，见 D-W 节硬规则） |
| P8 in-wait 证据采集 | 2 s（10 ms 间隔） | 满足样本即停；不可读/未满足→`dw_inwait_confirmed` 三态登记，协议继续 |
| P9 destroy() | 10 s | 未 resolve→destroy_unresolved 观察，跳 D6a |
| P10 worker 终态轮询 | 8 s（10 ms 间隔） | 两情形分别登记（与第二趟修订集群 A5(b) join 裁定区分，非静态断言表 A5）：**盒到期而终态标志未置位** → 不调用 `pthread_join`，登记 `join-timeout` 观察；**标志已置位后调用 join 而阻塞超出剩余盒** → 按第二趟修订集群 A5(b) 登记 `join-blocked-observed`（观察事实），runner 到点收口、不延长等待；join 调用位于 `N1BDISC_PRE` 发射之后（r4 R6c） |
| D6 各步 | 即返 syscall | — |

**P0 行外提（r4 第四趟 W1 自上表 P0 行外提，内容逐字不变）**：

- **前置 = operator-ready 确认步（r3 第二趟 E6 冻结）**：任何 gate 13 / `StartEntry` 命令与 ID 消费之前，操作员在 runner 控制台显式确认就绪，runner 逐字登记确认时刻（墙钟 + 单调时钟；r10：单调时刻命名为落盘字段 `p0_ready_mono_ms`，纳入「单调钟派生字段绝对域门」读数类非负性约束）与动作字面 `operator-ready-confirmed`——未取得该确认记录不得发 `StartEntry`、不得消费任何 AUTH/pair 或 evidence ID。该确认记录是「readiness 先于 Live」的机器可审计锚点
- 到点无 marker → **已消费 campaign 收口（E6）**：`StartEntry` 已实际发出、gate 13 已开始，AUTH/pair 与 evidence ID **已消费，不得再声称未消费**——走既有终态规则：host finally 全序列、死亡事实记录（r9：原「死因分类」措辞更名；此处通常无死亡证据与 marker，PRE 缺失按完整性失败收口：探针未完成预注册采集——r4 R1），evidence 记录正常产生（`record_status=collected`、`verdict=fail`），「操作员未 Allow」事实逐字登记。不属 blocked（第三趟 B3 裁定 (a) 修正：readiness 前置改由 StartEntry **之前**的 operator-ready 确认步承载，超时发生在启动之后即按已消费收口）

### runner 观测窗（r1 冻结数字与推导，BL-1）

- **冻结值：525 s**（runner 求值观测窗时长，自首个 `N1BDISC_` marker 起算；HilogStream 墙钟兜底上界 ≥825 s——StartEntry 之前启动；**r3 第二趟 E8：HilogStream 的实际停止条件是按终态停止，见下，825 s 不再被解释为「含启动裕量」的精确供给**）。**禁止「量级」「Live 再定」**——本值是预注册参数，变更即判据修改。
- **推导**：全序主线串行时间盒上界之和 = P1(10) + P2(≤5×60 + 迟到窗恰一次 60 = ≤360) + P3(10) + P4(10) + P5(≤10) + P6(25) + P7(10) + P8(max(barrier 等待 7, worker drain 5) + in-wait 证据 2 = 9) + P9(10) + P10(8) + P5T+P11+P12(≤5；其中 P5T 为即返单条 marker 落盘，r5 U1 前移后不占观测窗预算) = **467 s** + 收尾裕量 58 s（P12 后进程自然退出观察、capture 尾部静默确认、runner 封签准备）= **525 s**。
**P8 并发说明（r3 冻结）**：drain 由 worker 线程执行、barrier 等待由主线程执行，二者**并行**——barrier 等待盒 7 s 本身已覆盖 drain 盒 5 s + 调度裕量 2 s，故 P8 串行上界取二者最大值再加 in-wait 采集，**不是**二者相加（drain 5 + barrier 7 + in-wait 2 = 14 的旧算式把并发项串行相加，r3 更正；旧上界 472 → 467、裕量 53 → 58，冻结值 525 s 不变）。
worker 的 T_dw=5 s 与 P8 in-wait 采集 + P9 destroy 重叠，含于主线盒内不另加。
- **窗口起点**：capture 流中出现**首个以 `N1BDISC_` 开头的 marker** 的时刻（runner 单调时钟锚定；这是协议实际开始执行的首个可观测证据）。Live 序中 HilogStream 先于 StartEntry 启动，`StartEntry` → 操作员 Allow → `onCreate` → `N1BDISC_D1_BEGIN` 的链路均在窗口之前。
- **Allow 等待是否计入**：**不计入**。P0 有独立 300 s 时间盒（见上表），锚定 `StartEntry` 命令返回时刻、终点 = 首个 N1BDISC marker；该盒与观测窗首尾相接（Allow 盒终点即观测窗起点），互不挤占——HilogStream 在 StartEntry **之前**启动，兜底墙钟上界 ≥825 s（≥ 300 s Allow 盒 + 525 s 求值窗之和，作 runner 挂起时的保险丝）；
**HilogStream 停止条件（r3 第二趟 E8 冻结，按终态停止，机器可判定）**：下列任一成立即停——(a) `N1BDISC_POST` marker 已在 capture 流中出现（终态已捕获，`complete` 字面；r4 R1）；(b) host finally 序列执行至其步骤 1（finally 自身即停流，覆盖观测窗到点、死亡、挂起等一切收口路径）。825 s 仅在二者均未发生的 runner 异常情形下兜底熔断；尾窗不再依赖「启动到 StartEntry 返回之间的实际耗时 ≪ 预留裕量」这一未实测假设。
**求值窗 525 s 自首个 `N1BDISC_` marker 起算**，不按 HilogStream 启动时刻起算。Allow 盒到点未出现首个 marker → 按其自身规则收口（**已消费 campaign 收口**：StartEntry 已发出、ID 已消费，走 host finally + fail 终态，见时间盒表 P0 行与 B3 裁定 (a) 的 r3 第二趟 E6 修正），观测窗从未开启。
- **到点收口规则**：525 s 到点时 runner 停止接受新 marker 进入求值（之后到达的 N1BDISC marker 单独登记 `late_marker_observed`，不参与求值、不改 verdict），随即：若 `N1BDISC_POST` 已在 capture 中 → 正常封签（`protocol=complete`）；
若仅 `N1BDISC_PRE` 在 → 停止 HilogStream，按 host finally 序列采死亡证据：`process_death_observed = observed-true` → 按 `pre-only` 合法终态收口（求值规则见 verdict 节）；`process_death_observed != observed-true`（即 false 或 unobservable——进程仍存活或状态不可判）→ `fail`（探针未完成预注册采集；525 s > 467 s 协议上界，存活未完成不是平台事实）——**r12 两侧死亡谓词统一挂 `process_death_observed` 分量（blocker 3）**：原「有死亡证据（`:vpn` 消失/平台终止）」「无死亡证据」自立谓词废除，判定一律经「死亡事实记录（证据向量）」节闭集；
若 `N1BDISC_PRE` 缺 → `fail`（PRE 缺失完整性失败）。
**`PidOfVpn` 可观测性与终态完整性是两个独立的判定输入（r3 冻结，r4 R1 措辞更新；r9：登记落点改按证据向量分量）**：`PidOfVpn` 不可观测**单独**不构成 fail（它只使「`:vpn` 消失证据」无法建立，`process_death_observed` 按其自身规则记 `unobservable`，见「死亡事实记录（证据向量）」节证据规则 2 回退条件）；`PidOfVpn` 不可观测不单独构成 fail；fail 仍只来自 fail 闭集（本情形下真正的 fail 源通常是 F2 或 F9）（r10，A m-05：原「fail 只来自完整性条件本身——`N1BDISC_PRE` 缺失」过窄，漏 F9 等其余闭集 fail 源）。
- **出处更正（r0 错误登记）**：r0 曾写「沿 G0 固定窗口范式（60 s 量级）」——`docs/g0-go-arm64-physical-probe.md:170` 的固定 60 秒是 G0 场景 S1 的 `HilogStream` 采集窗，属 G0 单场景规格、非十三门范式共享参数，且 60 s 远小于本 campaign 时间盒上界和，引错出处且量级错误；r1 弃用该引法。

## 静态断言（freeze 前机器检查，违反即审查 blocker）

| # | 断言 |
| --- | --- |
| A1 | 探针源码恰一个 `pthread_create` 调用点 = D-W 登记位点；**零**出现以下任何线程/异步旁路：`std::thread::spawn`、`std::thread::Builder`（含 `Builder::spawn`）、`tokio::spawn`/tokio runtime、`async-std`、`smol`、`rayon`、任意 async 运行时、`napi_create_threadsafe_function`、`napi_create_async_work`/NAPI worker |
| A2 | 以 `fd_orig` 为实参的 `read`/`write`/`F_SETFL` 调用点为零；`close(fd_orig)` 调用点仅存在于 D6a 步 3 登记段（文件+行登记） |
| A3 | 除 `dlopen`/`dlsym`/`dlerror` 外，源码不引用任何 BoringTun 导出符号（数据面零调用；D1 陷阱条款因此不适用的事实由本断言背书） |
| A4 | **除第二趟修订集群 A5(b) 已登记的 `pthread_join`（worker 终态原子标志置位后调用；该调用无用户态超时出口——r3 第一趟 D5 显式豁免，freeze 静态审查不得因 join 缺超时出口判 A4 不通过）外**，全部等待点经单调时钟且带本文时间盒；其余零无界阻塞调用（零 `pthread_timedjoin_np`、零非清单 sleep/等待原语）。**P10 行与观测窗兜底的关系（冻结）**——外提为表后「A4 注」（r4 第四趟 W1 重构，内容逐字不变） |
| A5 | marker 字面集与本文冻结集逐字一致（正反例 selftest 覆盖，含 `:vpn` 三形态）；冻结集 = 本文**活规则中使用的** `N1BDISC_*` 字面，豁免集 = {`N1BDISC_D2_REJTEXT`、`N1BDISC_RESULT`}（理由见「A5 冻结集注」）——外提为表后「A5 冻结集注」（r4 第四趟 W1 重构；**r9 实现性更正：比对口径自「本文全部 `N1BDISC_*` 字面」收窄，原口径按字面实现必 fail，见冻结集注 r9 条**） |
| A6 | `openat` 调用点仅存在于 D-W in-wait 证据采集段，且实参路径字面 ∈ {`/proc/self/task/<tid>/stat`、`/proc/self/task/<tid>/syscall`}（`<tid>` 绑定 D-W worker tid；零其他文件路径、零 `O_WRONLY`/`O_RDWR` 打开） |
| A7 | 探针源码中 `/proc/.../stat` 的 state 解析实现**按该行最后一个 `)` 字符之后的下一个 token 定位 state 字段**（D-W 节冻结规则的机器背书）；**禁止**按空格切分取第 3 字段或任何左侧定位实现（`comm` 可含空格与括号，左侧切分必字段错位）；freeze 前静态审查核对源码实现与 selftest 用例（含 `comm` 带空格/括号的反例） |
| A8 | **探针源码中所有有界循环的终止条件静态核对（r3 第一趟 D2 新增，freeze 前机器检查）**：逐一核对——D7 主循环（deadline_ms 比较 break）、D8b storm 循环（10 s / 4 MiB / 50 000 写三熔断）、D-W drain 循环（5 s 时间盒 + 各转移）、各 10 ms 有界轮询等待（barrier / in-wait / 终态标志 / 迟到观察窗）——确认每个循环存在**可达的退出路径**且以**单调时钟或固定迭代计数**为界；freeze 前机器检查逐循环登记核对结论。本断言现在堵的是「探针自身 hang 无法在运行时被 verdict 捕获」这一残余的静态侧（r9：原运行时侧配对约束已随死因表删除、按 T0 裁决放弃，见自陈 14(a)） |
| A9 | **共享 destroy 子协议四步执行序的源码层静态核对（r9 新增）**：在**源码层**（而非仅 marker 字面集层）核对子协议四步——`N1BDISC_DW_DESTROY_T` 发射语句 → `destroy()` 调用语句 → `N1BDISC_DW_DESTROY_C` 发射语句 → resolve 有界等待语句——四者在**同一控制流上依次出现，其间无任何分支可跳过 `_C` 发射**；并核对 `destroy()` 在全部探针源码中**有且仅有一个调用点**（r9 第四步扩写：调用点位于「共享 destroy 子协议」内，P9 主线与 `dup-failed` 分支共用该唯一调用点）；任一不满足即 freeze 前 fail，不进入后续门（`_C` 发射先于 `destroy()` 调用的反例必 fail，用例见 gate 10 清单 ①） |
| A10 | **join-timeout 分支主线程零 `fd_dup` 操作的源码层静态核对（r18 新增）**：在**源码层**核对——`join-timeout` 登记（终态轮询盒到期、worker abandoned）后的主线程控制流内，对 `fd_dup` 的 `read`/`write`/`fcntl`/`close` 调用点为零（D6b 步 4-7 调用点全部位于 worker 终态已确认支内；join-timeout 支只发 `N1BDISC_SKIP|item=D6b|cause=join-timeout-abandoned` 直入 P11，r18 重裁）；任一不满足即 freeze 前 fail，不进入后续门（反例 = join-timeout 分支可达任一上述调用点，用例见 gate 10 清单 ④ 归因洁净反例钉） |

**A4 注（r4 第四趟 W1 自上表 A4 行外提，内容逐字不变）**：

**P10 行与观测窗兜底的关系（冻结；r5 U12 统一表述）**：标志置位后 `pthread_join` 阻塞 → **由 runner 依轮询超时**登记 `dw_join_result=join-blocked-observed`（观察项；阻塞线程自身不发出任何登记），无须等 8 s 盒用尽；其收口由观测窗到点与 host finally 承担（见 D-W 节「有界 join 可行性裁定」）；join 调用位于 `N1BDISC_PRE` 发射（P5T，r5 U1 前移）之后。

**A5 冻结集注（r4 第四趟 W1 自上表 A5 行外提，内容逐字不变）**：

- 冻结集 = 本文全部 `N1BDISC_*` 字面：`D1_BEGIN`/`D1_LOADED`/`D1_FAIL`/`D1_SYM`/`D1_END`；`D2_ENTRY`(attempted 与 outcome 两形态)/`D2_LATE`/`D2_LATE_FD`/`D2_S1`..`D2_S7`；`D4_BEGIN`/`D4_SENT`/`D4_READ`/`D4_END`；`D5_BEGIN`/`D5_WRITE`/`D5_RECV`/`D5_END`；
- `D8_MTU`/`D8_STORM_BEGIN`/`D8_STORM_END`；`D7_BEGIN`/`D7_END`；`DW_SPAWN`/`DW_DRAIN`/`DW_BARRIER`/`DW_INWAIT`/`DW_DESTROY_T`/`DW_DESTROY_C`（r7 X4 新增 post-invocation marker）/`DW_RETURN`/`DW_EXIT`；`D6S1_B`..`D6S7_B`/`D6S1_R`..`D6S7_R`；
- `FD`（r3 第二趟 E2 新增：fd ledger transition marker）；`SKIP`；`CHUNK`；`PRE`/`POST`（r4 第一趟 R1：取代 `RESULT` 的双终态 marker；`RESULT` 字面自 r4 起不在冻结集内）。
- **r4 第二趟 S4 更正：本集删除 `D2_REJTEXT`**——r3 第二趟 E3 已冻结拒绝文本统一走 `N1BDISC_CHUNK|stream=rejtext`（`CHUNK` 字面 + `stream` 逻辑键承载），无独立 REJTEXT 专用 marker；旧集同时列出两者属自相矛盾（实现无论发否必违反其一）。
- 同步核对本集其余字面无被后续趟取代的残留（`RESULT` 已删、其余逐字核对仍为在用字面）；rejtext 的 marker 归属登记为 `N1BDISC_CHUNK|stream=rejtext`（非独立 `N1BDISC_*` 前缀字面，属 `CHUNK` 条目的 stream 取值）。
- **r9 实现性更正（豁免集显式化）**：全文共 57 个不同 `N1BDISC_*` 字面、本集枚举 55 个，A5 旧口径「冻结集 = 本文全部 `N1BDISC_*` 字面」按字面实现静态断言必然 fail、不可实现；比对口径收窄为「本文**活规则中使用的** `N1BDISC_*` 字面」，**豁免集 = {`N1BDISC_D2_REJTEXT`、`N1BDISC_RESULT`}**——前者是已废的通用 chunk 族标签（r4 第二趟 S4 自本集删除，拒绝文本统一走 `N1BDISC_CHUNK` 字面加 `stream=rejtext` 参数，无独立专用字面），后者是 r4 起已被 `PRE`/`POST` 取代（R1）的历史字面，仅出现在修订登记与防误引注中、无活规则引用。

## verdict 求值与聚合（机器规则，fail-closed）

`record_status` 沿用现有语义：执行后 `collected`，独立审查合格后 `reviewed-pass`（`docs/evidence-schema.md:89`）。**双轴**：`record_status`（审查轴）与 `verdict`（完整性轴）独立；`reviewed-pass + verdict: pass` 同时成立，方可作为 N1b r2 预注册设计输入（决议 §4.4）。

**求值顺序（单一 if/else 链，消除条款重叠，MJ-13）**——判定输入仅限 marker 流（三形态关联后）、runner 侧证据与 freeze 哈希；**平台事实三态字段永不进入 verdict 求值**（r9 唯一例外：死亡侧分量 `probe_crash_signature_observed` 经 F1 进入 verdict——「死亡事实记录（证据向量）」节七分量中其余六个仍纯记录、永不进入 verdict 求值）：

```
if   任一 invalid 条件命中   -> verdict = invalid
elif 任一 fail 条件命中      -> verdict = fail
elif 任一 blocked 条件命中   -> verdict = blocked
else                         -> verdict = pass
```

条件书写顺序与求值顺序无关（上表已消除嵌套前提）：

- **`invalid`**：证据污染/秘密入档；**ready freeze 后资产**（HAP/`.so`/runner/配置矩阵/符号清单/marker 集冻结文件）SHA-256 与 freeze 记录不一致（freeze 资产被冒用/篡改）；HDC 命令流出现白名单外命令；marker/日志流存在跨 attempt 拼接证据（campaign 前缀重复、时间戳非单调等）。
- **`fail`**：终态 marker 异常——**`N1BDISC_PRE` 缺失**（探针未能在 P5T 位点（P6 D7 开始之前）落盘任何终态 marker；`N1BDISC_POST` 无 PRE 承载而单独出现亦属时序矛盾，同判 `fail`）；
PRE 与 POST **双现于非冻结位点、或任一冻结字段缺项**（字段集见「执行位点与结果通道」节第 5 条；**PRE 在而 POST 缺 = `pre-only` 合法终态，不属本条**）；
**顺序破坏——`protocol=complete` 时校验 P1-P12 全序（含 P5T）（marker 时序单调；P0 是操作员 Allow 等待、发生在首个 `N1BDISC_` marker 之前，无 P0 marker，不由本序校验管辖，仅由 operator-ready 确认记录、Allow 时间盒与已消费收口规则管辖（r3 第二趟 E6））；
`protocol=pre-only` 时只要求**死亡位点之前的已完成项 marker 有序**，之后的项按「死亡事实记录（证据向量）」节登记、未执行项按既有 u4 分岔规则赋具名 `unobservable` cause（r9：原「由死因分类与指派规则收口」随死因表删除；r5 U7：原「`protocol=post-mortem`」为 R1 改 {`complete`, `pre-only`} 双值后的未定义残留，更正）；任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）；增量落盘缺项（某 D 项完成 marker 在而其 `(stream,item)` chunk 组缺失/组内重组失败/sha256 不符——chunk 术语沿 r3 第二趟 E3 的 `(stream, item)` 分组，「chunk 族缺失/重组失败」按此读）；
fd ledger 缺失或与 marker 流矛盾；
**F1（r9 替代原死因分类 fail 触发器）——`probe_crash_signature_observed = observed-true` **且** `protocol != complete` → verdict `fail`**：死亡侧唯一 fail 触发面，三分支闭集定义与第 3 支法理见「死亡事实记录（证据向量）」节。
**作用域恢复（r9 主会话裁定）**：原触发器仅在 `protocol=pre-only` 时求值（r5 U3）。该作用域条款在 r9 首稿随分类表被一并删除，属**误删**——T0 裁决只废除「归因驱动 fail」，**未授权扩大 fail 面**，故恢复。
**限定式的等价性与边界（r9 写明，防实现者自行推断）**：`protocol` 取值域经 r5 U7 更正后**恰为两值** `{complete, pre-only}`（原 `post-mortem` 是 R1 改双值后的未定义残留，已删）。故本条的 `protocol != complete` 与 r5 U3 的「仅 `pre-only` 时求值」**在该域上等价**，是同一条作用域的两种写法，**不构成新增限定**。
边界情形：`N1BDISC_PRE` 缺失时 `protocol` 不可求值——该输入由 **F2 独立判 fail**，与 F1 是否可求值无关；无论 F1 取何值 verdict 均为 `fail`，故本条不为该情形另设规则。
恢复理由：`protocol=complete` 意味着 PRE 与 POST 双现、P1-P12 全序校验通过、冻结字段齐备、ledger digest 一致，即**采集装置完整跑完并封口**。进程级崩溃签名（`CPPCRASH` / 致命信号）与「存活到发出 POST」不相容，故 `complete` 且命中崩溃签名只可能是 (a) POST 之后收尾期的崩溃（事实已封存）或 (b) 窗内他源条目；两者都不构成「未完成预注册采集」。
对 (a) 判 fail 会在一次**完整成功采集**上烧掉不可复用 ID，与本 campaign 的损失函数正相反（席 A 裁决原话：损失函数是「烧掉不可复用 ID」，不是「漏掉一次可能的探针挂死」）。
**`complete` 下命中崩溃签名的处置**：`probe_crash_signature_observed` 仍如实记 `observed-true`，并登记入 **runner evidence 记录**为**需 N1b 关注的异常观察**（r10 修正登记载体：原「在 POST 登记」不可实现——POST 冻结字段集不承载该观察，`N1BDISC_POST` marker 字段集逐字冻结、发出即封口、不得追写字段，且崩溃常发生在 POST 之后的收尾期、POST 物理上无法承载；runner evidence 记录不受 marker 封口约束），但不驱动 verdict。
**本条请审查席重点挑战**：这是 r9 中唯一一处由主会话主动收窄 fail 面、且非三席裁决直接授权的改动（裁决只说「不再归因」，没说 `complete` 时崩溃签名如何处置）。
`platform-termination` 不 fail 支随原分类表删除（其语义已由「除闭集外一切不 fail」承载）。
死亡**归因**本身一律不进入 verdict：原 `probe-fault` / `platform-termination` / `unattributed` / `fault-type-unrecognized-no-platform-signature` / 无类各支的 fail 与不 fail 映射全部废除（r9，删除登记见自陈 14）。
`d1_cmdline` 非 `:vpn` 进程；封签失败。

**`verdict = fail` 闭集与 r9 规范稿 F 索引对照（r9 冻结）**：下列 F1–F9 是规范稿的归纳编号，经逐条核实后 **F7 不属 fail 轴**（现归 `invalid`），其余八条（F1–F6、F8、F9）构成 fail 闭集。F2–F9 只是既有完整性触发面的**归类索引**，判定一律以本节及各节既有字面为准——r9 已逐条核实其存在形式与措辞，逐条锚登记见自陈 14(e)。
- **F1** `probe_crash_signature_observed = observed-true` **且** `protocol != complete`（死亡侧唯一条目；三分支闭集定义见「死亡事实记录（证据向量）」节；r10 补限定、与上方 F1 权威定义行逐字同一——三席收敛 blocker 1，防只读索引者把 `complete` + 崩溃签名误判 fail）；
- **F2** `N1BDISC_PRE` 缺失（本节「终态 marker 异常」首项 + PRE/POST 求值规则 (3)/(4)）；
- **F3** 冻结全序被破坏（本节「顺序破坏」条：`protocol=complete` 时 P1-P12 全序校验不过、marker 时序非单调；`protocol=pre-only` 时死亡位点之前已完成项 marker 无序同归本条；r9 第五步增补例：`DW_DESTROY_T.mono_ms` 晚于 `DW_DESTROY_C.mono_ms` 的 mono 逆序同归本条，沿五态表第 5 行同轴，见 D-W 节单调钟绝对域门顺序约束）；
- **F4** 增量落盘缺口 / 冻结字段缺项 / criteria-gap 判别方法 **(4)**（未预注册 cause；r10 论域切分，A m-03：原「(2)-(4)」与 F8 的 (2)/(3) 论域重叠——同一输入（如 `elapsed_ms=-1`）可同时贴 F4/F8 两条，自本趟起 (2)(3) 唯一归 F8、(4) 唯一归本条）（本节对应条 + 「criteria-gap 处理」节；**「字段取值落在冻结域外」不属本条**——域外有效平台值记 `unobservable(cause=value-outside-frozen-domain)` 不 fail，沿判别方法 (1)）；
- **F5** 封签失败、fd ledger 缺失或与 marker 流矛盾、同切点 ledger digest 校验不符（本节对应条；**freeze 资产 SHA-256 复算不符不属本条**——现归 `invalid` 轴）；
- **F6** `d1_cmdline` 非 `:vpn` 进程；
- **F7 —— 不属 fail 闭集**：HDC 白名单违规现归 `invalid` 轴 + 停止条件 S2。规范稿把它归进 fail 面属归纳错误，此处保留编号仅为与规范稿对照可追溯，**不得据本条判 fail**；
- **F8** 解析器 / 真值表缺口（= criteria-gap 判别方法 **(2)(3)**——r10 论域切分，A m-03：F4 限判别方法 (4)，本条不再与 F4 的「(2)-(4)」重叠——(2) 输入在 capture 内自相矛盾（含探针 P12 派生字段与 runner 重建值不一致——r17）、(3) 派生规则/真值表未覆盖该输入组合——**记录器未尽职，不是死因**；**未知 `revents` 位不属本条**，归 `other-revents` 不 fail（r9 第五步升格为未知位前置门，沿 S2/④；0/0b 前置检查未命中的输入；无 SKIP 且 `_C` 缺的未知位输入归 0b，r16）；
单调钟派生字段绝对域违反（负时长、`window_start_monotonic > window_end_monotonic`）**属本条**，r9 第五步 BL-4，沿判别方法 (3)）；
- **F9** 观测窗到点进程仍被正向确认存活且协议未完成（「runner 观测窗」到点收口规则：仅 PRE 在且 `process_death_observed != observed-true` → `fail`；r12 谓词随收口规则统一挂死亡分量，原「无死亡证据」索引措辞同步）。

**除上列 fail 闭集（F1–F6、F8、F9；F7 不在内）外，一切结局均不 fail。**

**冻结解释句（r9 逐字写入，席 A 给出）**：「未完成预注册采集」限 PRE 缺失、顺序破坏、增量落盘/字段域缺口、封签失败、以及窄崩溃签名；**PRE 封口之后的进程消失，不论能否归因，不构成 fail。**

**歧义消解（r9 主会话，不改上句字面）**：上句后半「进程消失……不构成 fail」与 F1 在字面上可读出冲突——若该次消失恰好伴随窄崩溃签名，前半句要 fail、后半句要不 fail。
正确读法由前半句自身给出：前半句已把「窄崩溃签名」列入 fail 面，故后半句约束的必然是**不存在窄崩溃签名的那种消失**。
为消除实现者选边空间：**判定一律以 F1 与上列 fail 闭集为准；本冻结解释句只作法理说明，不单独构成判定规则。**
此句用于封堵「没跑到 POST 就算未完成采集」的误读。PRE 一旦封口，其后死亡属采集被环境截断，**已得事实仍算落盘**。
- **`blocked`**：**仅限**——设备连接失败/HDC 退化；目标绑定门元组漂移（含完整系统版本与冻结值不符）；**freeze 前构建输入无法产出冻结哈希**（构建输入漂移：与 invalid 的分界 = freeze 资产是否已绑定——绑定后字节不符是冒用/篡改 → invalid；绑定前构建不可再现 → blocked）。
**操作员 Allow 超时不属本清单**（B3 裁定 (a) 经 r3 第二趟 E6 修正：操作员 readiness 由 StartEntry **之前**的 operator-ready 确认步承载并留审计记录；StartEntry 之后的 Allow 超时按已消费 campaign 收口走既有终态规则——fail + evidence 记录，不属 blocked；
`docs/evidence-schema.md:84` 的发现型 blocked 只列外部基础设施不可达，不得自行扩权）。
- **`pass`**：以上均未命中——即「发现协议按冻结顺序跑完（含 `pre-only` 平台终止路径）且事实已登记落盘」。**事实本身可为负面**（能力不存在、调用被拒、无数据可采、watchdog 杀进程、destroy 未唤醒 waiter），均不影响 `pass`（`docs/evidence-schema.md:82`）。
- **PRE/POST 求值规则（逐字冻结，r4 第一趟 R1；字段缺项即 `fail`，沿 MJ-7）**：`protocol` 由双 marker 存在性唯一确定——
  (1) **`PRE` 存在 + `POST` 存在 → `protocol=complete`**：正常终态，P1-P12 全序（含 P5T）校验通过后全部事实入账。
  (2) **`PRE` 存在 + `POST` 缺失 → `protocol=pre-only`**：**合法终态，不是完整性失败**（适用前提：`process_death_observed = observed-true`，见「死亡事实记录（证据向量）」节；**r12 起死亡判定的唯一谓词源是本分量，本条不再自立谓词**——原「host finally 死亡证据采集确认 `:vpn` 进程消失或平台终止」中「或平台终止」为自立谓词，随 blocker 3 废除——PRE 在而 POST 缺且 `process_death_observed != observed-true` 属探针存活未完成，走 `fail`，见「runner 观测窗」到点收口规则）
——PRE 已承载的事实照常入账求值；
**`dw_*` 结局字段按已有 marker 真实求值（r10：删除原 blanket 赋值句「D6 全部子项与 D-W 结局字段记 `unobservable(cause=post-destroy-unobservable)`」——它把 D-W 已真实跑完、marker 已在的字段也一律盖成 unobservable，且该 cause 原不在 `dw_return_class`/`dw_join_result` 闭域内（r9 第五步只把 POST 在场的 skip 编码折入），属域外值 → 判别方法 (4) → fail（r10 订正索引归 F4，原写 F8 沿旧 F4/F8 重叠口径，同为 fail，A m-03））**；
poll 已返回、drain 已跑的子项，`dw_return_class` 按 D-W 节派生序（合法域→0/0b→未知位→1-11）对真实输入求值（E3 预期成功终态里 D-W 是完整跑完的，marker 全在，真实数据不得被 blanket 盖掉）；仅当某字段的判定输入确实不可得（如 worker 未及 poll 即死、无输入 marker 可据）才记 `unobservable`，cause 按死亡时 `destroy_call_state` 五态分流（r12，sol M-01——原单一 `post-destroy-unobservable` 把「destroy 那时还没被调用过」的死亡也盖成 post-destroy，与事实相反，拆分如下）：
五态 `not-reached`（死于 destroy 位点前，如死于 D7/P7、协议未到 P9）→ `unobservable(cause=destroy-not-reached)`（复用 V2 (1a) 既有字面、跨域共用——同一物理事实「destroy 未达即死」在 u4_* 与 dw_* 两域记同一 cause，不另造近义变体）；
五态 `call-returned`（destroy 已过而输入不可得）→ `unobservable(cause=post-destroy-unobservable)`（旧单一 cause 的语义自 r12 收窄为「destroy 已过而输入不可得」、r13 第三包再收窄为仅 `call-returned`）；
五态 `call-boundary-incomplete` → **透传 `unobservable(cause=call-boundary-incomplete)`**（r13 第三包，sol M-01：五态定义明言该态「既可能尚未调用，也可能已进入调用」，「destroy 已过」对它不成立——r12 把它与 `call-returned` 同记 post-destroy 与五态事实矛盾，改沿 `dw_watchdog_killed` r13 有序互斥表 ② 同款透传自身 cause，不并入 post-destroy）；
五态 `not-called`（显式 SKIP）不走本死亡收口——沿既有 skip 编码（`no-live-fd`/`dup-failed`）落值；
**r17 扩入 barrier-never-observed（三席收敛，第十七轮 B-01）：五态 `not-called` 的 barrier-never-observed 顺延路径死亡同样不走本死亡收口**——u4 侧未执行子项沿既有 `unobservable(cause=destroy-not-called)`（V2 (1b) 任一 SKIP 支，已有），
dw 侧 `dw_return_class` 由 D-W 节分域声明 (c) 类 0 `destroy-skip-proven` 直接承载（判定输入 = `SKIP|item=destroy` 存在性、无 RETURN 亦判）、`dw_destroy_distinguishable_from_timeout` 走四步 (1) SKIP 位点字面（`cause=barrier-never-observed`）；`dw_join_result` 落既有本体值 `join-timeout`（顺延路径的终态轮询盒按主序列既有口径先于 SKIP 到期并登记，非新 cause）；
五态 `marker-contradiction`（含 SKIP 同现域门）不落本域——该观测是完整性矛盾、独立走 F3 fail（dw_* 判定输入可得时照常求值，不可得时无预注册 cause、沿既有缺项/域缺口 fail 面，不得洗成 unobservable）。
**域同步（r10；r12 计数更新；r13 第三包计数更新）**：pre-only 死亡收口编码自 r10 起逐字纳入 `dw_return_class` 与 `dw_join_result` 取值域——r10 单值时为 16/8 值，r12 拆分后为 `dw_return_class` 判定表 13 类 + skip 2 + 死亡收口 2 = **17 值**、`dw_join_result` 5 值 + skip 2 + 死亡收口 2 = **9 值**；
r13 第三包死亡收口再拆出 `call-boundary-incomplete` 透传值（3 值：`destroy-not-reached` / `post-destroy-unobservable` / `call-boundary-incomplete`）后为 `dw_return_class` 判定表 13 类 + skip 2 + 死亡收口 3 = **18 值**、`dw_join_result` 5 值 + skip 2 + 死亡收口 3 = **10 值**，
**r17 计数更新（三席收敛，第十七轮 B-01）**：`dw_return_class` 增 complete∧join-timeout 收口 cause `unobservable(cause=poll-never-returned)` 1 值 → 判定表 13 类 + skip 2 + 死亡收口 3 + 1 = **19 值**；`dw_join_result` **不随扩**——complete∧join-timeout 形态 join 侧落既有本体值 `join-timeout`（5+2+3 = **10 值**不变，该 cause 仅入 class 域）；
计数同步见 D-W 节落盘字段族、`dw_destroy_distinguishable_from_timeout` 闭合句与路径①不解锁括注；
**管辖边界（r10）**：本句只管辖 `dw_*` 结局字段；`u4_*` 七子项不受此句管辖，一律按下方 V2 四支分岔求值（原句「D6 全部子项」字样与 V2「死亡之前已发出 result marker 的子项保持其记录值」冲突，以 V2 为准，随本条删除）。
**`u4_*` 七子项赋值（r5 U6 重写：保留已得结果，不整体抹为 false）**——按死亡位点与 marker 证据逐子项判定：
- **死亡之前已发出 result marker 的子项保持其记录值**（如进程死于 D6b 中途时，D6a 的真实正结果不得被覆盖）；
- **死亡之后未执行的子项按 destroy 调用状态分岔（r6 V2 三支；r9 对齐 `destroy_call_state` 五态改四支——「`_T` 在、`_C` 缺」自「destroy 从未发起」支拆出单列：该观测是歧义，不是「未调用」；r7 X4 锚点升级为 `DW_DESTROY_C` 的登记不变；r9 第五步第一支再按 SKIP 显式性分 1a/1b 两小支，分支计数仍为四支）**：
  - **`DW_DESTROY_T` 缺 + `DW_DESTROY_C` 缺**——r9 第五步按 SKIP 显式性分两小支（原两例同记 `not-reached`）：(1a) 无 `N1BDISC_SKIP|item=destroy`（五态 `not-reached`，如死于 D7）→ 未执行子项保持 **`unobservable(cause=destroy-not-reached)`**（r12 注：本字面自 r12 起同时为 `dw_*` 死亡收口的 destroy-未达 cause——跨域共用、各域冻结清单各自逐字纳入，见 PRE/POST 求值规则 (2) r12 分流条款；不另造 `destroy-not-reached-unobserved` 类近义变体，防误引）；
(1b) 协议已显式发出**任一** `N1BDISC_SKIP|item=destroy`——cause 无封闭枚举域、按位点字面原样登记，已预注册的有 `no-live-connection`（skip 表 destroy 行，D-W 整体被 skip 的分支）与 `barrier-never-observed`（D-W 节 destroy 位次硬规则顺延路径）两个——（五态 `not-called`）→ 未执行子项记 **`unobservable(cause=destroy-not-called)`**——两小支均**不得赋 `observed-false`**（r9：原该支 cause `destroy-never-called` 随五态废除——marker 缺失不得推断「未调用」；「destroy 后不可观察」对该路径是假负值；U4 是 OB-04 判定输入，宁缺勿误）；
**r10 扩域注**：(1b) 原字面仅认 `cause=barrier-never-observed`，使「五 create 全拒 + 死于 POST 前」路径发出的 `N1BDISC_SKIP|item=destroy|cause=no-live-connection`（skip 表 destroy 行）无落点——该路径五态为 `not-called`、若落 (1a) 即与五态表矛盾；扩为任一 SKIP 后两 cause 同归 `destroy-not-called`，验收用例见 gate 10 ⑨ r10 用例 A。
  - **`DW_DESTROY_T` 在 + `DW_DESTROY_C` 缺**（五态 `unobservable(cause=call-boundary-incomplete)`——既可能尚未调用，也可能已进入调用但未返回，后者是一次成功的 destroy terminal）→ 未执行子项记 **`unobservable(cause=call-boundary-incomplete)`**，**不得赋 `observed-false`、不得写「destroy 从未发起」**（r9：原 r8 Y1 反例按 `destroy-never-called` 收口，结论随五态改正）；
  - **`DW_DESTROY_C` 已发出且 destroy 已 resolve**（有 resolve/D6a 证据；五态方向 `call-returned`）→ 未执行子项赋 `observed-false`（destroy 后进程不可继续执行，法理成立，同 r4 R1）；
  - **`DW_DESTROY_C` 已发出但无 resolve 证据**（已调用未返回：marker 发出后、destroy 时间盒内进程即死）→ 未执行子项记 **`unobservable(cause=destroy-unresolved)`**（r7 X4 第三支，grok 补充：destroy 是否已实际生效不可判，不得赋假负值）；
- **无死亡证据（如 join 卡死但进程仍活）不进入 pre-only**（沿观测窗到点收口规则走 `fail` 路径），此时 `u4_*` **不赋 false**、按 D6 节原三态规则求值；
- 此为 pre-only 路径对 D6 节「无 pre-marker → `unobservable`」默认的显式覆盖特例（仅 POST 缺失且有死亡证据时适用；POST 在时 D6 子项一律按 D6 节原三态规则求值）。
**此路径 verdict 可 `pass`**（事实已登记落盘，destroy 后进程终止不是缺陷）。`protocol=pre-only` 由 runner 依 capture 判定并登记于 evidence 记录（不代发探针 marker；死亡证据引用登记入记录的 `death_evidence_ref` 字段）。
  (3) **`PRE` 缺失**（无论 POST 是否存在）→ 完整性失败（探针未能在 P5T 位点之前落盘任何终态——r6 W5 更正：r5 U1 前移后 PRE 的发射位点是 P5T（D7 之前）而非 destroy 之前，缺失败即 D1–P5 段事实未封口）→ `fail`。(4) **两者皆缺** → 同 (3) `fail`。
  字段归属冻结：`skip_summary` 随 PRE 发射（PRE 时点已知的 skip 列表）；
  `death_evidence_ref` 仅在 `pre-only` 收口时由 runner 登记（取值 `faultlogger文件名|pidofvpn-absent-record`，r3 第三趟 B1 的 `exit-record` 删除维持不变；r10 取值域收窄：`faultlogger文件名` 仅限携带 `Signal` ∈ {`SIGKILL`, `SIGTERM`} 平台终止信号的条目可作死亡证据引用——无终止信号的事件类条目（如 `APPFREEZE`）只喂 `fault_type_observed`/`signal_observed`，不得作为本字段的死亡证据引用），`complete` 形态不登记；
  `ledger_digest` 在 PRE（**P5T 快照值**，r5 U1 前移后即 P5T 切点）与 POST（最终值）各有一份，一致性校验**限同一切点**（r5 U5）：
  - PRE 值只与 P5T 切点的流快照重建值（open 条目记 `open-at-pre`）比对；
  - 最终值只与全量流按收口形态（`open-at-exit`/`process-exit`）的重建值比对；
  - 同一切点内探针自发值与 runner 重建值不一致 → `fail`（沿 E2），**禁止跨切点比较**（见「ledger canonical 序列化」节 r5 U5 条）。

### 死亡事实记录（证据向量三态，r9 替代死因分类；原 BL-2 归因表删除）

死后收口（现行 `protocol` 取值 `complete`/`pre-only`；历史版本曾称 post-mortem）的死亡侧判定自 r9 起**不再由死因归因驱动**。runner 在 Live 后（观测窗到点或检测到进程死亡时）采集死亡证据，按**七分量证据向量**逐项落盘：各分量互不推导、各自三态或各自闭合枚举，**七者之间不存在优先级、不存在 else 兜底、不合成任何单一「死因」标签，且除 `probe_crash_signature_observed` 外一律不进入 `verdict` 求值**。
**随本轮删除（r9）**：原死因分类表（7 行）本体、行间优先级全序冻结条款、fail-closed 兜底封闭性声明（「禁止落入无类、禁止发出任何可 pass 的 pre-only 收口」）、「行 1 / 行 4 条件外提」块、专为该表服务的注释块（行 3 注、行 5 收敛说明、死亡时刻 elapsed 墙钟代理、D7 超窗管辖分界），以及 `probe-fault` / `platform-termination` / `unattributed` / `fault-type-unrecognized-no-platform-signature` 四个合成标签在本节的全部定义性使用。
各被删 fail 映射失去的真实探针缺陷捕获、以及「为何该损失是决议 §4.2 所要求的」，逐条登记见**起草人自陈 14**。**原 `unattributed` 不做改名保留，直接删除**（r9 主会话裁定）：席 B 曾建议改名为 `unobservable(cause=death-cause-indeterminate)` 以去除因果暗示，但七分量证据向量中**不存在任何「死亡原因」字段**——不合成单一死因正是本次改造的目的。该 cause 字面无处落脚，保留它只会诱导实现者再造一个合成字段，故整体删除，全文不再出现该字面。

**证据采集规则（r9 起服务于证据向量各分量的取值；先于向量表）**：

1. **时间关联（Live 前快照）**：`StartEntry` 之前执行一次 `FaultProbe` **快照**并逐字登记命中文件名集合；死亡证据只采 **campaign 时间窗内新增**的条目（`FaultRecv` 取回的文件名 ∉ 快照集合）。无快照过滤时陈旧 fault 条目会被误当本次证据——此为机器判定前提，非裁量。
**窗界唯一判据（r4 第三趟 T2 冻结）**：条目归属本 campaign 时间窗**仅且仅由快照文件集合差分决定**（取回文件名 ∉ 快照集合 → 新增条目；∈ 快照集合 → 窗外旧条目），**不使用任何墙钟比较**、不做现场裁量。理由（冻结）：fault 条目冻结的时间形态 `YYYY-MM-DD hh:mm:ss.mmm` **不含时区标记**；目标设备系统时区的获取不在 HDC 白名单内、亦无既有 marker 证据可推；runner 与设备之间的墙钟偏差容限无法机器核实——三者中任一都不允许「新增条目」判定免于现场解释，而集合差分对全部输入机器可判定。
fault 条目内解析出的时间字段只作观察数据逐字落盘，不参与窗界判定与任何 verdict 条件（r9：原「不参与死因分类表行 1-4 的任何条件」随表删除）；唯一例外是 `marker_tail_state` 的静默跨度比较（r9 替代原行 2 的死亡路径墙钟代理，谓词见下方 `marker_tail_state` 定义）——其比较两端（faultlogger 条目时间戳与 capture/hilog 墙钟）均取自设备侧同一时钟，差值不受时区标注影响。
2. **`:vpn` 死亡证据来源**：HDC 白名单具名操作 **`PidOfVpn`**（`-t <T> shell pidof cn.alfadb.netbird.n1bdisc:vpn`，见 HDC 白名单表；观察 `:vpn` 子进程存活）；其 absent + capture 三形态关联后静默 = `:vpn` 进程消失证据。
**positive 基线前置（r3 第二趟 E7 冻结）**：`pidof` 的空输出在语义上二义（进程 absent 与探测不可观测不可凭空区分），故——runner 须在本 campaign 内、**首个 `N1BDISC_` marker 出现之后**（此时 `:vpn` 已由 create 建立）执行恰一次 `PidOfVpn` 作为 **positive 基线**并逐字登记其非空输出（含 pid 字面）；**此后**（host finally 步骤 3 等）的空输出方可解释为 absent；
若从未取得 positive 基线（marker 未出现、或基线采样本身返回空），则后续一切空输出一律记 `unobservable(cause=pidofvpn-no-positive-baseline)`，**不得反推进程死亡**。UI 进程探针（`PidOf`）**不随 `:vpn` 退出**（E3 先例），不得作为 `:vpn` 死亡证据来源。
**可行性先例（C8，主会话已核实，非本 campaign 假设）**：`shell pidof <bundle>:vpn` 的精确 Extension 探测语义由仓内裁定 `ADJ-20260808-0001` 确立（`docs/evidence/e3-physical-preflight-process-target-2026-08-08.md:30`——`PidOf` 语义改为 `shell pidof <bundle>:vpn`，精确探测 Extension 进程），
且该语义在 E3 收官 campaign 实际返回过有效结果（`docs/evidence/e3-physical-preflight-authorization-2026-08-29-0001.md:128`——S7 前置检查发现 B 的 `:vpn` 进程实际 active）；同文件 `:13`、`:98` 记载 E3 收官 campaign 在冻结元组上 S1-S7 全 pass，使用的正是该严格语义，且 E3 的 bundle 名同样远超内核 `comm` 15 字节截断长度——「`pidof` 按 comm 匹配而 bundle 名超长恒空」的担忧在该先例上不成立。
**回退条件（照 `ADJ-20260808-0001` 登记）**：若本 campaign 中 `<bundle>:vpn` 精确名不可观测（`pidof` 无输出或进程命名不同），则**依赖该证据的 `process_death_observed` 分量记 `unobservable`**（r3 第一趟 D1 收窄沿用，r9 措辞随分量名更新：不得因 `PidOfVpn` 不可观测**单独**判 fail——它只使「`:vpn` 消失证据」无法建立、该分量记 `unobservable`；fail 只来自完整性条件本身，见 verdict 节到点收口规则的独立判定输入条款；
不得据此反推死亡；此时死亡证据前提不成立，死后收口按无独立死亡证据路径处理），并将该不可观测本身逐字登记。
3. **「退出记录证 SIGKILL 类终止」签名删除**：该签名无白名单命令、无字段、无 SIGKILL 判定程序，不可机器求值；其原本承载的「无 crash 签名的进程消失」结局现由 `signal_observed`（平台终止段 {`SIGKILL`, `SIGTERM`}，r4 第三趟 T6 以 faultlogger `Signal` 字段恢复机器可求值形态——本条删除旧签名的理由「无字段、无 SIGKILL 判定程序」已不再成立）与 `fault_type_observed` 的逐字记录承载（r9：原「由 `platform-termination` 行条件 (iii)/(iv) 承载」随表删除——该结局仅记录、不驱动 fail）。
   **r10 事件/死亡两轴分立（`process_death_observed` 证据源收紧，与向量表该行同一口径；r12 修订，blocker 3，grok B-03 + sol B-02 两席收敛、主会话裁定：原 (b) 支被做成不经 `PidOfVpn` 的独立充分条件且未同步同源谓词——同一观测在分量、PRE/POST 求值规则 (2)、观测窗到点收口三处谓词下可出三个 verdict，(b) 降为辅助合取支）**：
   faultlogger 条目证明的是冻结/崩溃**事件**（喂 `fault_type_observed`/`signal_observed` 两个事件分量），**不单独证明进程消失**——条目不携带「进程已不存在」的状态语义；`process_death_observed = observed-true` 的唯一充分条件是 (a)：`PidOfVpn` 在 positive 基线前提下转 absent **且** capture 三形态静默——携 `Signal` ∈ {`SIGKILL`, `SIGTERM`} 的终止条目（原 (b) 支）仅在 `PidOfVpn` 于 positive 基线下转 absent（或 finally 采样确认 absent）时作为**并存的关联证据**逐字引用、参与 (a) 的确认，**不得在 `pidof` 仍非空时单独证死**，理由有二：
   其一，`FaultProbe` glob `*cn.alfadb.netbird.n1bdisc*` 不绑 `:vpn`，UI 进程与本探针同 bundle，SIGKILL/SIGTERM 条目可能属于 UI 进程；其二，SIGTERM 可被捕获/忽略，信号语义本身不蕴含进程终止。
   其余条目（如 `APPFREEZE`）只喂事件分量、不喂死亡分量。
4. **死亡位点（机器判定；r9 起即分量 `last_visible_site` 的定义）**：死亡位点 = capture 中**最后一个成功发出的阶段 marker** 所在位点（marker 集与位点映射沿「冻结全序」表）。
   **位点判定的不确定性（r6 V4 显式化；r7 X5 修正比较对象）**：「最后成功 marker 位点」是 **capture 可见性**的判定，**不是进程执行史的断言**——若尾 marker 未落入 capture，runner 无法区分「marker 丢失」与「死于较早阶段」。
   **r9：原 `site_uncertainty=possible-tail-marker-loss` 置位判据改组为分量 `marker_tail_state`（谓词与阈值见下方该分量定义表）；原 `T_uncertainty` 及其与行 2 判定窗 27000 的全部比较论证、「置位则行 2 不命中」规则、「按 r7 X5 约束 u7 与死因行 2」的联动条款随死因表删除——本判定为纯观察登记，不参与 verdict、不与任何 fail 判定耦合。**（r7 X5 的比较对象修正——连续尾丢失发生在最后 marker **之后**，须度量「死亡证据与最后可见 marker 之间的静默跨度」而非前一 marker 间隔——由 `marker_tail_state` 的谓词原样继承。）
**`APPFREEZE` 位点约束（r3 第一趟 D2 反转默认；r9 起服务于 `probe_crash_signature_observed` 第 3 支的位点守卫）**：默认反转——只有**真正的短时步**上的 `APPFREEZE` 才构成正向崩溃签名，该集合**冻结为 D2 保留条目锁定序列（2.1-2.7 的即返 syscall；r4 第一趟 R5：P1 移出——P1 的 dlopen/dlsym 有 10 s 合法时间盒，不是即返步，平台 watchdog 在一次合法加载上杀进程不得判成探针缺陷）**；
其余位点（P1 dlopen/dlsym 10 s 时间盒、P2 每条 create 60 s 时间盒、P3/P4 窗口、P7 storm 10 s、P6 D7 20 s、P8 D-W、P9/P10 D6）均含与 D7 同量级甚至更长的合法时间盒，**不是短时步**——平台在一次合法的长 create 或写风暴上触发 watchdog 属平台事实，其 `APPFREEZE` 仅由 `fault_type_observed` + `last_visible_site` 记录（未完成项记 `unobservable`），**不得 fail**（决议 §4.2：平台行为不如预期永远不是 fail；r9：原「一律走 `platform-termination`（行 4 条件 (i)）」随表删除）。
（r9 删除原尾句「反方向洗白由死因分类表行 2（D7 超窗行，墙钟代理求值）堵住——**A8 是 freeze 前静态检查，不充当运行时死因判据（r4 R2）**」：行 2 随表删除；A8 作为 freeze 前静态检查的定性不变，见静态断言表。）
5. **fault 条目解析契约（r3 第二趟 E7 冻结）**：对 `FaultRecv` 取回的每个 faultlogger 文本条目，按下列字段契约解析（selftest 须含各字段正反例夹具）——字段按**行首字面匹配**、值取该行 `:` 之后的首个非空白 token 起、至行尾：
   - `Fault_Type`（签名判定用；**r4 第二趟 S1 重写：字面匹配改为「冻结候选集 + 归一化规则」，归一化规则逐字冻结**——将取到的值**去除全部下划线 `_` 与连字符 `-` 后统一转为大写**，再与候选集比较；使 `APPFREEZE`/`APP_FREEZE`/`app-freeze` 归一为同一项）；
归一化后候选集**逐字冻结为恰三项**：`APPFREEZE`（归一化吸收变体 `APP_FREEZE`、`APP-FREEZE`、`appfreeze` 等）/ `CPPCRASH`（吸收 `CPP_CRASH`、`cpp-crash` 等）/ `JSRAWERROR`（吸收 `JS_RAWERROR`、`js-raw-error` 等）。
**归一化后仍不在候选集内的非空值**（含可能的异词根如 `JSCRASH`、`JSERROR`——不猜近义词根）一律记 `other:<原值逐字>`（原值 = 归一化**前**的逐字原文）——r9 起该字面仅作为 `fault_type_observed` 的域外记录值（域外记录不 fail，同 criteria-gap 判别方法 (1) 口径），不再路由至任何死因表行（原「按 S1 兜底出口处理」与「流入行 5/行 6」的分岔随表删除；未实测依赖与 freeze 前收敛义务见下方 S1 登记段）；
   - `Signal`（签名判定用；**取值域三段（r5 U10；r9 措辞随死因表删除更新）**——
      **探针 crash 段** {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`}（即 `probe_crash_signature_observed` 第 2 支的信号集）；
      **平台终止段** {`SIGKILL`, `SIGTERM`}（平台发起终止的记录段，仅由 `signal_observed` 记录、不构成崩溃签名；两段互斥，r5 U2 已写明）；
      **其余**（含空值/未知信号名）：记原文逐字入档、不命中任何签名段）；
      **时间字段** = 条目内**首个匹配 `YYYY-MM-DD hh:mm:ss.mmm` 形态（含前导零、`.` 后恰 3 位数字）的时间戳行**，**仅作观察数据逐字登记（r4 第三趟 T2 冻结）**：该形态无时区标记、设备系统时区来源不在白名单内，故**不参与** campaign 时间窗关联（窗界唯一判据 = 证据规则 1 的快照文件集合差分）；该时间戳本身是否与设备系统时区一致不做判定，逐字落盘即毕；条目中不存在可解析时间字段 → 该条目按 criteria-gap 处理 (2) 解析缺口收口（`fail` + raw 逐字入档，保守不洗白），不得默误当「时间窗内新增」或「窗外旧条目」。

   **r10 多条目聚合规则（blocker 8，B B-07：同窗 `FaultRecv` 可取回多个条目，契约按逐条目解析而 `fault_type_observed`/`signal_observed` 为单值分量——同窗新增一条 `APPFREEZE` 加一条 `CPPCRASH` 时取哪个原不定义 → 分量与 verdict 非单值；主会话裁定：保守聚合，任何正向签名优先）**：两分量均为**全窗聚合单值**——对全部时间窗内新增条目逐条解析完毕后按下述保守优先序聚合恰一次，不取「第一条」「任一条」等未定义捷径：
   - `fault_type_observed`：任一条目归一化后 ∈ {`CPPCRASH`, `JSRAWERROR`} → 按该**正向**条目记归一化值（多条正向取聚合序首个正向条目）；无正向条目但有 `APPFREEZE` → 记 `APPFREEZE`；其余情形（全部条目归一化后均域外）→ 记 `other:<第一条域外字面逐字>`——多条域外条目时**全部原值逐字入档（raw 档保留多值）**、分量单值取第一条，多值事实不丢、分量仍单值；
   - `signal_observed` 同构：任一条目携致命信号（`SIGSEGV`/`SIGABRT`/`SIGBUS`/`SIGFPE`）→ 取该信号（多条致命取聚合序首个）；无致命信号但有条目携 `SIGKILL`/`SIGTERM` → 取之；无任何条目携两具名段信号（携「其余」段信号的条目按上方既有规则仅 raw 逐字入档）→ `observed-false`；多条目信号不同时**全部信号字面逐字入档（raw 档保留多值）**、分量单值取优先序最高者（致命段 > 平台终止段）；
   - **可解析与不可解析条目混合（r10 追加一支，主会话裁定；P4 遗留观察 2；r13 扩展，blocker 4，sol B-01——取回失败文件视同不可解析条目进入本支条件，取回状态建模见 `probe_crash_signature_observed` 求值规则前置 r13 条）**：任一**可解析**条目呈正向签名 → 按该正向记（不被不可解析条目稀释，r13：也不被取回失败文件稀释）；
   **raw 级正向谓词（r15 破循环，sol M-01 + grok m-04）**：三个谓词基于**成功取回的原始条目**、不依赖聚合字段——(1) 存在可解析条目归一化 ∈ {`CPPCRASH`, `JSRAWERROR`}；(2) 存在可解析条目携致命信号；(3) 存在可解析条目**归一化后** = `APPFREEZE`（r16，grok m-03：与证据规则 5「归一化 + 候选集」同一口径，防 `APP_FREEZE`/`app-freeze` 等字面不命中）∧ 全局 `last_visible_site` ∈ 短时步集 ∧ marker 序列无矛盾。
**聚合规则**：`fault_type_observed` 侧——raw 谓词 (1) 命中 → 按该条目记（谓词 (3) 的类型条件同时在 fault 侧，命中且 (1) 不命中时记 APPFREEZE）；无 (1)(3) 且存在（不可解析条目∨取回失败）→ `faultrecv-unavailable`/`fault-type-unparsable`（按优先序）；否则域外/无条目照常。`signal_observed` 侧——谓词 (2) 命中 → 按该条目记（**仅本分量的正向**，不被失败文件稀释）；无 (2) 且存在失败源 → unobservable；否则照常。
**跨分量不传导**：raw 谓词 (3) 命中只保 fault 侧、谓词 (1) 命中只保 fault 侧、谓词 (2) 只保 signal 侧——每分量只看本分量的正向（grok m-04：仅支 3 命中时 signal 侧无正向，仍按本分量规则落值）。（r14「正向签名」按支定义随之由本块改述——其「任一支完整前件真即正向、包括第 3 支」的意图由谓词 (1)-(3) 承载，原循环读法废除）
**无可解析正向且存在（不可解析条目 ∨ 取回失败文件）** → 该分量 unobservable（r15：「可解析正向」= 上块 raw 级谓词本侧命中——fault 侧 (1)(3)、signal 侧 (2)，不读聚合后分量）——仅不可解析条目时沿原值：`unobservable(cause=fault-type-unparsable)`（`fault_type_observed` 侧）/ `unobservable(cause=signal-unparsable)`（`signal_observed` 侧）；
存在取回失败文件时落 `unobservable(cause=faultrecv-unavailable)`（冻结优先序中高于字段不可解析——前者整条证据缺失、后者证据在手但字段坏）——不可解析条目与取回失败文件中均可能存在正向签名，按 `observed-false` 记是 false-negative（fail-open）；本支先于 `APPFREEZE` 支与 `other:` 兜底与无信号 `observed-false` 兜底求值（r13 补记，grok M-01：原句漏列 `APPFREEZE` 支，则「可解析 `APPFREEZE` 条目 + 不可解析条目」构造在主规则记 `APPFREEZE` 与本支记 unobservable 间两读、非单值——求值序写死为正向支最先、本支次之、`APPFREEZE` 支仅当无不可解析条目/取回失败文件时才落到）；
与真值表按支求值口径同构（r12：分量不可解析 → 依赖它的支 unknown；unknown 只在无一支 true 时经聚合把签名落为 `unobservable`，不洗成 `observed-false`）；
   - **聚合序冻结（r12，blocker 8；三路收敛——多条正向无论取哪个 verdict 相同，但事实记录非确定）**：本段各处「聚合序首个」「第一条」的排序键 = **faultlogger 文件名字节序**——时间窗内新增条目按文件名升序字节序排列后依序求值，与 `FaultRecv` 的实际取回顺序解耦（取回顺序受命令行展开与传输时序影响、runner 间不可复现；文件名字节序对同一 capture 集合稳定）；文件名唯一，无需 tie-break；
   - **理由（冻结）**：签名判定是 fail 面，聚合必须保守（fail-closed 方向）——任一条目呈正向签名即按正向记，**不得因窗口内存在其他条目而稀释**；正向段严格优先于 `APPFREEZE` 与平台终止段，二者亦不得被域外条目挤出单值判定。

**`Fault_Type` 字面未实测依赖（r4 第二趟 S1，最高优先级登记；r9 随死因表删除改写）**：本节分量 `fault_type_observed` 与 `probe_crash_signature_observed` 第 1/3 支全部依赖 faultlogger 的 `Fault_Type` 字面 `APPFREEZE`/`CPPCRASH`/`JSRAWERROR`，**这些字面从未在本冻结元组的任何真实 faultlogger 条目上核实**，仓内亦无 faultlogger 样本文本。
后果链（r9 重写）：若真机实际写的是 `APP_FREEZE`、`JS_CRASH` 或任何其他形态且归一化后不落入候选集，则 F1 第 1/3 支在真机上**永不命中**——崩溃类死亡只以 `fault_type_observed = other:<原值逐字>` 记录，可能以 pre-only `pass` 收口（fail-open 残余，逐字登记见自陈 14(c)）；原 r4 版「落行 6 `unattributed` → `fail`、烧掉不可复用 ID」的后果随死因表删除不复存在。
为此冻结：**(1) 归一化匹配**：`fault_type_observed` 求值与 F1 各支的字面比较一律按证据规则 5 的「归一化 + 候选集」规则解释（E7 段已冻结），不再逐字面严格相等。
**(2)（r9 删除）原「兜底出口」**：原「`other:<原值逐字>` 条目按有无 SIGKILL/SIGTERM 条目分岔路由至行 4 (iii) / 行 5」的出口随死因表删除——`other:<原值逐字>` 现仅为 `fault_type_observed` 的记录值，信号侧事实由 `signal_observed` 独立记录，二者互不推导、均不驱动 fail（除非各自独立命中 F1 三支闭集的字面）。
**(3) 冻结前置收敛义务**：见流程门 3 新增条款——freeze 静态审查时若已能取得目标元组的一条真实 faultlogger 文本，须据此把候选集收敛为实测字面并重新提交独立审查；取不到则以本节候选集执行并在证据中登记该未实测依赖。
**selftest 须含归一化正反例**（`APP_FREEZE`/`app-freeze` 须命中 `APPFREEZE`；r9 改：`JSCRASH` 须记 `fault_type_observed = other:JSCRASH` 且不命中崩溃签名、不驱动 fail——原「无 SIGKILL/SIGTERM 条目 → 行 5 → fail；有 → 行 4 (iii) 不 fail」分岔随表删除）。

**七分量证据向量（r9，替代原分类表；各分量互不推导，任何分量不得由其他分量的缺失反推）**：

| 分量 | 取值域 | 说明 |
| --- | --- | --- |
| `process_death_observed` | 三态 | `:vpn` 进程是否被观察到消失。`observed-true` 的唯一充分条件（r12 修订 blocker 3：原两支闭集的 (b) 支降为辅助合取支；逐态条件见下方取值域补全段与证据规则 3）：`PidOfVpn` 在 positive 基线前提下转 absent + capture 三形态静默；携 `SIGKILL`/`SIGTERM` 条目仅在 pidof absent 时作并存关联证据逐字引用、不得单独证死。**禁止以 marker 缺失单独推断被杀**（E3 0001 误判教训，`docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:163`——与 D7 节 u7 赋值规则及证据规则 2「不得反推进程死亡」同一口径）。 |
| `last_visible_site` | 位点枚举 ∪ `unobservable` | capture 中最后一个成功发出的**阶段 marker** 所在位点（定义沿证据规则 4）。这是 **capture 可见性**的判定，**不是进程执行史的断言**。capture 中无任何成功发出的阶段 marker、位点无法建立 → `unobservable(cause=no-visible-marker)`（r10 具名）。 |
| `fault_type_observed` | 归一化闭集 ∪ `other:<literal>` ∪ 三态 | faultlogger 条目的 `Fault_Type`，按证据规则 5 既有归一化规则（去 `_`/`-`、大写）求值；归一化后不落候选集的非空值记 `other:<原值逐字>`（原值 = 归一化前的逐字原文）；无条目记 `observed-false`；条目存在但字段不可解析记 `unobservable(cause=fault-type-unparsable)`（r10 具名）。 |
| `signal_observed` | 信号枚举 ∪ 三态 | 同 `fault_type_observed` 口径（取值沿证据规则 5 的信号三段；无条目记 `observed-false`，条目存在但字段不可解析记 `unobservable(cause=signal-unparsable)`，r10 具名）。 |
| `destroy_call_state` | 五态（定义见下方「`destroy_call_state` 五态表」） | destroy 调用边界状态。`_T`/`_C` 两枚 marker 只能**夹住**调用、不可能在调用瞬间发射，区间歧义显式建模，不以 marker 缺失二值化推断「未调用」（r9 第四步落地，替代本表原占位）。 |
| `marker_tail_state` | 见下方「`marker_tail_state`」 | 尾 marker 丢失的不确定性登记（r9 替代原 `site_uncertainty`）。 |
| `probe_crash_signature_observed` | 三态 | **唯一进入 `verdict` 的死亡侧分量**，定义见下方三支闭集。 |

**`process_death_observed` 取值域补全（r10，blocker 6：三态逐态给出取值条件，任何执行路径有值；证据源口径与证据规则 3 r10 条同一）**：
- `observed-true`——独立死亡证据的唯一充分条件（r12 修订，blocker 3，grok B-03 + sol B-02 两席收敛、主会话裁定：原「恰两支」的 (b) 支降为辅助合取支——同窗条目 glob 不绑 `:vpn` 且 SIGTERM 可捕获/忽略，(b) 独立证死使同一观测在三处同源谓词下 verdict 非单值）：(a) `PidOfVpn` 在 positive 基线（证据规则 2）前提下转 absent **且** capture 三形态关联后静默。
  (b) 类条目（时间窗内新增 faultlogger 条目携 `Signal` ∈ {`SIGKILL`, `SIGTERM`}）仅在 (a) 的 pidof absent 成立（或 finally 采样确认 absent）时作为**并存的关联证据**逐字引用、参与 (a) 的确认，**不得在 `pidof` 仍非空时单独证死**——理由：`FaultProbe` glob `*cn.alfadb.netbird.n1bdisc*` 不绑 `:vpn`，条目可能属于同 bundle 的 UI 进程；SIGTERM 可被捕获/忽略，信号语义不蕴含终止。
  原第三支「进程退出记录」系幽灵引用（r3 第三趟 B1 已删 `exit-record`），随 r10 证据源修正废除；其余 faultlogger 条目（如 `APPFREEZE`）证明的是冻结/崩溃**事件**、只喂 `fault_type_observed`/`signal_observed`，不喂本分量。
- `observed-false`（r10 补全；r13 删「且无任何上述死亡证据」合取，grok M-03：(b) 类条目 r12 起已降为 pidof absent 确认下的关联证据、非 (a) 构成项——残留合取使「UI 进程 `SIGTERM` 条目在场 + `PidOfVpn` 恒非空」构造在本分量三态下无落点，删后只留「基线已取且窗内始终非空」）——positive 基线已取得，观测窗内（至 host finally 步骤 3 采样、`ForceStop` 之前）`PidOfVpn` 始终非空（进程持续在场）→ `observed-false`（进程未被观察到消失）。本值不驱动 fail——存活未完成的完整性 fail 由观测窗到点收口规则独立判定（F9 fail 面不变）。
- `unobservable`——positive 基线从未取得 → `unobservable(cause=pidofvpn-no-positive-baseline)`（复用证据规则 2 既有字面，不新造；证据规则 2 回退条件命中——`:vpn` 精确名全程不可观测——同归本 cause，其成因同为本 campaign 未曾取得可用基线；r12 同步：原「且无 (b) 类条目」限定废除——(b) 类条目降为 pidof absent 确认下的关联证据后，无基线时其存在既不能替代基线、也不能独立证死，本 cause 恒覆盖该路径）；
  基线已取得而采样结果为 absent、但 capture 静默不可判（capture 流自身缺口）→ `unobservable(cause=marker-gap-indeterminate)`（复用既有字面，宁缺勿误口径不变；r12 同步：原「且无 (b) 类条目」限定同上废除——静默不可判时 (b) 类条目不能替代静默确认，本 cause 恒覆盖该路径）。

**记录义务（r9 冻结）**：七个分量在任何执行路径上都必须各自有值（含 `unobservable` 及其预注册 cause）。**七者之间不存在优先级，不存在 else 兜底，不合成任何单一「死因」标签；任何分量都不得由其他分量的缺失反推。**

**`destroy_call_state` 五态表（r9 第四步落地，替代本节原占位；规范依据 `docs/n1b-disc-r9-death-facts-spec.md` §4.1）**：marker 只能在调用**之前**或**之后**发射，**不可能在调用瞬间发射**——`N1BDISC_DW_DESTROY_T`（调用前）与 `N1BDISC_DW_DESTROY_C`（调用后）只能**夹住**调用。这个区间歧义**不可消除**，只能显式建模——**不得**再用「marker 缺失」二值化推断「未调用」：

| 观测 | `destroy_call_state` |
| --- | --- |
| 协议显式发出 `N1BDISC_SKIP\|item=destroy` | `not-called`——**唯一可证「未调用」的途径** |
| `DW_DESTROY_T` 缺、`DW_DESTROY_C` 缺 | 按此前协议位点定 `not-reached`；**不得仅凭 marker 缺失推断未调用** |
| `DW_DESTROY_T` 在、`DW_DESTROY_C` 缺 | `unobservable(cause=call-boundary-incomplete)`——既可能尚未调用，也可能已进入调用但未返回（后者是一次**成功的 destroy terminal**）；**不得写 `never-called`** |
| `DW_DESTROY_T` 在、`DW_DESTROY_C` 在 | `call-returned`——只能证明**调用表达式已返回完成凭据**，不能证明平台效果已发生 |
| `DW_DESTROY_C` 在、`DW_DESTROY_T` 缺 | marker 顺序/完整性错误 → **fail（走 F3）**；`destroy_call_state` 本身记 **`unobservable(cause=marker-contradiction)`**（r10 补全：两轴独立——verdict 由 F3 在完整性轴 fail，本分量在死亡事实轴如实记 unobservable，原「不进本分量取值」的无值行废除，五态表五行全部有值） |

**SKIP 与 `_T`/`_C` 同现域门（r10 新增；先于五态表各行求值执行的前置门，与「无 fd / dup 失败分支」判定表合法域门同构）**：协议已显式发出 `N1BDISC_SKIP|item=destroy`（「未调用」的唯一证明途径，五态表第 1 行）却又观测到 `N1BDISC_DW_DESTROY_T` 或 `N1BDISC_DW_DESTROY_C` 任一枚调用锚 marker——矛盾输入（既声明未调用、又存在调用锚，冻结全序下不可能同时为真）：`destroy_call_state` 记 **`unobservable(cause=marker-contradiction)`**，verdict 走 **F3 fail**（顺序破坏轴）。
先于五态表执行：矛盾流不进入任何单行归类——不得按第 1 行记 `not-called`、亦不得按调用锚记 `call-returned`/`call-boundary-incomplete`；前置门先行的理由同合法域门：矛盾输入属探针/解析缺陷，不得洗成平台事实。

**明确废除（r9，规范稿 §4.1）**：原「`_T` 在而 `_C` 缺 → `destroy-never-called`」判定——它会把「调用已进入平台层、但进程在返回完成凭据前 terminal」这一**成功的 destroy terminal** 误判为未调用并连锁误判。本文原持该判定的位点（`dw_return_class` 类 0、u4 分岔、gate 10 ⑨）随本表一并改正。

**worker 返回时序三分带（r9，本分量的配套时序规则；规范稿 §4.2）**：marker 字段只记毫秒，**等值不能证明先后**，故两侧一律严格不等号、闭区间整体归歧义（`T_mono_ms`/`C_mono_ms` 分别取 `DW_DESTROY_T`/`DW_DESTROY_C` 的 `mono_ms`，`at_mono_ms` 取 `N1BDISC_DW_RETURN` 的 `at_mono_ms`）：

| 观测 | 判定 |
| --- | --- |
| `at_mono_ms` **<** `T_mono_ms` | `definitely-pre-invocation` |
| `at_mono_ms` **>** `C_mono_ms` | `definitely-post-invocation` |
| `T_mono_ms` **≤** `at_mono_ms` **≤** `C_mono_ms` | `unobservable(cause=invocation-window-ambiguous)` |
| `DW_DESTROY_C` 缺 | **不得作 post-invocation 归因** |

路径①只接受严格的 `at_mono_ms > C_mono_ms`（即 `definitely-post-invocation` 带）。

**`probe_crash_signature_observed` 三支闭集（r9 冻结）**——`observed-true` **当且仅当**下列任一成立（闭集，穷尽，无 else）：

1. 归一化 `fault_type_observed` ∈ {`CPPCRASH`, `JSRAWERROR`}（归一化规则沿证据规则 5）；
2. `signal_observed` ∈ {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`}；
3. 归一化 `fault_type_observed` = `APPFREEZE` **且** `last_visible_site` ∈ 冻结短时步集（D2 保留条目锁定序列 2.1-2.7 的即返 syscall；P1 已由 r4 第一趟 R5 移出，位点约束见证据规则 4）**且** marker 序列无矛盾（席 B 补充的守卫，r9 采：marker 序列自相矛盾时该位点判定本身不可信，此支不得成立）。

**r10 结构注（第 3 支，A M-03「F1 第 3 支结构性空集」；主会话裁定：保留本支为事实记录、不删）**：本支位点守卫冻结的短时步集（D2 保留条目锁定序列 2.1-2.7 的即返 syscall）全部位于 P5T（`N1BDISC_PRE` 发射位点）之前——`N1BDISC_PRE` 在时 `last_visible_site ≥ P5T` 永不在短时步集内，本支对 `pre-only`/`complete` 协议恒不成立（结构性空集）；`N1BDISC_PRE` 缺时 F2 已独立判 fail（verdict 节：PRE 缺失时 `protocol` 不可求值、由 F2 承载），本支即便成立也不独立改变任何 verdict。
本支的价值是对 N1b 的诊断事实记录——短时步集内的 `APPFREEZE` 是探针缺陷的正向证据（法理见下方「第 3 支法理」段），不是独立 fail 触发。r10 于此登记其与 F2 的恒叠加关系：第 3 支命中 ⇒ `N1BDISC_PRE` 必然未发射 ⇒ F2 必然同命中且为 verdict 承载面——**防后续审查误读为死代码而删**，删除该支丢失诊断事实而不改变任何 verdict。

上述三支之外一律 `observed-false`；证据不足以求值时的赋值按下方 r12 求值规则（按支求值 + 三值聚合）冻结，不另设路径。

**`probe_crash_signature_observed` 求值规则（r12 重写冻结，取代 r10 五行按序互斥真值表——blocker 1 + blocker 8 合并修，grok/sol 两席修法收敛：按支分别三值求值，再以 true > unknown > false 聚合。
废除理由（三路收敛核实）：原行 2「任一输入分量 unobservable → 签名 unobservable」先于行 5「任一支命中 → observed-true」求值，单条目 `Fault_Type=CPPCRASH` 可解析 + `Signal` 字段不可解析的构造下第 1 支字面已成立、签名却被洗成 unobservable → F1 不命中 → pre-only pass（探针崩溃 fail-open）。
闭集三支仍是 `observed-true` 的唯一字面条件，本规则冻结输入建立、按支求值与聚合赋值路径——两步完成、禁止任何其他赋值路径；输入分量仍取全窗聚合后的单值——聚合规则见证据规则 5 末段的多条目聚合规则，聚合排序键 = faultlogger 文件名字节序（r12 冻结，见该规则段））**：

**前置（最先求值；沿 r10 行 1；r13 重写为按文件建模，blocker 4，sol B-01——原前置只建模 `FaultRecv` 全局失败，但 `:931` 快照差分取回的是文件名集合、`:952` 契约按逐条目解析：部分文件取回成功、部分失败时读作全局失败会稀释已见正向（fail-open 重现）、读作全局成功则失败文件无落值，两读都错）**：
- **逐文件取回状态（r13）**：证据规则 1 快照差分确定的本窗新增文件集合中，每个文件的取回各自记状态 ∈ {取回成功, 取回失败}——取回成功者，其条目照常按证据规则 5 契约逐条解析、照常参与聚合；取回失败者，不产生条目；
- **全局失败（保留原前置行为；两种成因区分登记）**：(i) `FaultProbe` 命令本身失败（无法取回任何条目）；(ii) 命中文件**全部**取回失败——两情形下三支全部 unknown（支 1/2 因两分量不可求值、支 3 因 `fault_type_observed` 不可求值；`last_visible_site` 由 capture 派生、与 `FaultRecv` 无关，其可求值性不影响本落值——r12 措辞精确化沿袭）→ 签名 `unobservable(cause=faultrecv-unavailable)`，`fault_type_observed`/`signal_observed` 各记 `unobservable(cause=faultrecv-unavailable)`；
- **部分失败（r13 新增）**：取回成功文件与取回失败文件并存——取回成功条目照常求值与聚合；取回失败文件「可能藏有正向签名」的事实由聚合层吸收（即证据规则 5 混合支的 r13 扩展）：任一文件取回失败 → 两分量聚合中各记一个 unknown 贡献，聚合优先序中「存在取回失败文件」视同「存在不可解析条目」——无可解析正向且存在（不可解析条目 ∨ 取回失败文件）→ 分量 unobservable、落值 cause 落 `faultrecv-unavailable`；
  cause 优先序在 `faultrecv-unavailable`（取回失败）与 `fault-type-unparsable`/`signal-unparsable` 之间**保持既有次序**（见下方 unknown cause 唯一编码段）：取回失败的 unknown 优先级高于字段不可解析——前者整条证据缺失、后者证据在手但字段坏；
  **已有可解析正向 → 照常 true、签名 `observed-true`，不被取回失败稀释（三值聚合 true > unknown 天然保证，r13 钉死）**。

**第一步·按支求值（r12 冻结）**：三支各自独立求三值（true / false / unknown），支间互不拦截：
- 支 1 只依赖 `fault_type_observed`：可求值 → 归一化字面比较（证据规则 5）∈ {`CPPCRASH`, `JSRAWERROR`} → true，否则 false；不可求值 → 支 1 unknown，缺的输入 = `fault_type_observed`；
- 支 2 只依赖 `signal_observed`：可求值 → 字面比较 ∈ {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`} → true，否则 false；不可求值 → 支 2 unknown，缺的输入 = `signal_observed`；
- 支 3 依赖 `fault_type_observed` ∧ `last_visible_site` ∧ marker 序列无矛盾：所需输入全部可求值 → 按字面比较得 true/false（marker 序列自相矛盾属可判定输入——守卫不成立 → 支 3 false，非 unknown）；任一不可求值 → 支 3 unknown，缺的输入逐字登记（`fault_type_observed` / `last_visible_site`，后者不可求值 cause = `no-visible-marker`）。
- **无条目 ≠ 不可求值**（沿 r10 行 3 钉子）：`FaultRecv` 已执行且取回为空 → `fault_type_observed`/`signal_observed` 各记 `observed-false`，属确定观测——支 1/支 2 求得 false，不入 unknown。

**第二步·聚合（r12 冻结，三值序 true > unknown > false）**：任一支 true → 签名 `observed-true`——**已知正向不被任何旁路未知稀释**（支 2/支 3 的 unknown 不得压过支 1 的 true，反向同构；r10 行 2 先于行 5 的拦截路径废除）；
无 true 且至少一支 unknown → 签名 `unobservable`，cause 按下方唯一编码取值；全部支可求值且无 true → 签名 `observed-false`。

**unknown cause 唯一编码（r12 冻结；sol B-01 第二点 / grok M-04——多支 unknown 并存时 cause 非唯一则签名落值非单值）**：签名 `unobservable` 的 cause 按**冻结优先序**取第一个命中者：`faultrecv-unavailable` > `fault-type-unparsable` > `signal-unparsable` > `no-visible-marker`；命中的全部 unknown cause 逐字列入 raw 档（多值不丢）。
四 cause 即三支 unknown 缺口的闭集：`fault_type_observed` 不可求值仅 `fault-type-unparsable`（`FaultRecv` 全局失败时同记 `faultrecv-unavailable`，见前置——r12 措辞精确化）、`signal_observed` 仅 `signal-unparsable`、`last_visible_site` 仅 `no-visible-marker`（均见七分量表分量行）、
`faultrecv-unavailable` 仅对应 `FaultRecv` 失败——**r13 语义扩域（blocker 4，sol B-01）：该 cause 自「`FaultRecv` 全局失败」扩为「`FaultRecv` 全局失败 ∨ 任一命中文件取回失败」（按文件建模见前置），四 cause 闭集不变、无第五种 unknown 来源**。

**第 3 支法理（r9 冻结）**：短时步集内的步骤是**即返 syscall**。平台 watchdog 在一次毫秒级 `fcntl` 上冻结并杀死进程，不构成「不如预期的平台行为」的合理解释——只能是探针自身在该步挂死，故此支是**正向探针缺陷证据**，非平台行为。**反之，非短时步集上的 `APPFREEZE`（含 D7 的 20 s 任务）是本 campaign 要发现的平台行为本身，永远不得 fail**（决议 §4.2）。死亡侧 fail 触发面仅此一条（F1）；`verdict = fail` 的完整 fail 闭集（F1–F6、F8、F9；F7 不在内）与「未完成预注册采集」冻结解释句见「verdict 求值与聚合」节（r10 措辞同步，A m-02）。

**`marker_tail_state`（r9 新增分量，替代原 `site_uncertainty`；纯观察登记；r9 正名：旧置位派生 cause 字面同样内嵌已删字段名 `site_uncertainty`，随本字段一并正名为 `marker-tail-loss`——全文凡引用该 cause 处自本趟起一律用 `marker-tail-loss`，旧字面全文清零、不再出现；r10：四值域扩为**五值闭合枚举**——新增 `no-death-evidence` 补「POST 缺 ∧ 无死亡证据」路径，clock 不可求值 cause 具名，逐行覆盖任何执行路径）**：

| 取值 | 条件 |
| --- | --- |
| `tail-complete` | `N1BDISC_POST` 在（终态 marker 已封口，无尾丢失可能） |
| `possible-tail-loss` | POST 缺 **且** 存在死亡证据 **且** 死亡证据墙钟 − `last_visible_site` 对应 marker 墙钟 > `T_tail = 25000 ms` |
| `tail-loss-not-indicated` | POST 缺、有死亡证据、但上述差值 ≤ `T_tail` |
| `no-death-evidence`（r10 新增第五值，替代原四值表对「POST 缺 ∧ 无死亡证据」路径的无值缺口） | POST 缺 **且** 无任何死亡证据（进程仍活或状态不可判——F9 路径）：无死亡证据即无死亡证据墙钟可据，置位谓词的减法不存在输入；本值使该路径有值，与进程存活未完成的 fail 判定互不推导（fail 由观测窗到点收口规则独立出） |
| `unobservable(cause=tail-clock-unresolvable)`（r10 具名，原省略号 cause 废除） | POST 缺、有死亡证据，但任一墙钟不可求值（死亡证据墙钟或 `last_visible_site` 对应 marker 墙钟） |

**`T_tail = 25000 ms` 取值理由（r9 重写，不再挂靠任何 fail 判定行；r10 标签修正，B M-03）**：25000 = D7 冻结时长 20000 + 宽限 5000，即**D7（静默敏感阶段）的合法时长上界（20000 冻结时长 + 5000 宽限）**——原「单个最长合法阶段」旧标签不成立（P2 单次 create 时间盒 60 s > 25 s）；静默跨度超过该上界，即提示尾部可能丢失。
位点晚于 P2 的长盒（如 P2 create 60 s）静默跨度天然可能超过本阈值，该情形落 `possible-tail-loss` 属保守标注——本分量是观察项、不进 verdict，取值偏差只影响观察标签。**本阈值不与任何 fail 判定耦合**（原「置位则行 2 不命中」规则、`T_uncertainty` 与行 2 判定窗 27000 的全部比较论证随死因表删除），取值偏差只影响一个观察标签的置位与否、不影响 `verdict`。减法方向沿 r8 Y2 冻结：死亡证据晚于最后可见 marker，减法方向反转时恒为负、永不置位。死亡证据墙钟的取材沿既有规则：faultlogger 条目内时间戳，无条目时取 capture 静默判定墙钟（证据规则 1）。

**信号划分理由（冻结；r9 措辞随死因表删除更新）**：SIGSEGV/SIGABRT/SIGBUS/SIGFPE 是探针代码缺陷的典型同步信号（段错误/断言/非法指令/除零），属探针崩溃签名（r9 起由 `probe_crash_signature_observed` 第 2 支承载）；SIGKILL/SIGTERM 不可能由进程内部缺陷同步产生——它们由内核或平台管理实体异步发出（OOM killer、watchdog、生命周期管理），属**平台发起**的终止，仅由 `signal_observed` 记录、不构成崩溃签名（决议 §4.2：平台行为不是 fail）。
destroy 之后 `:vpn` 进程按 E3 先例（`docs/n1b-gate-plan.md:63`）正常静默 terminal、不产生 faultlogger 条目，是**预期成功终态**——该结局必须可 `pass`，否则一次成功执行会烧掉唯一 ID（r3 第一趟 D1 确立的方向，r9 起由「除 fail 闭集（F1–F6、F8、F9；F7 不在内）外一切结局均不 fail」的窄闭集整体承载，不再依赖任何归因类；r10 排除式与闭集正文同步，A m-02）。

**死亡事实冻结时点（r9 措辞更新，原「死因冻结时点」）**：死亡证据采集与证据向量求值在 host finally 的 **`ForceStop` 之前**冻结（见「host finally cleanup」节步骤 4；其前一步骤 3 为预注册的 `PidOfVpn` 采样步——r3 第一趟 D6）——`ForceStop` 及其后的 runner 清理动作自身会产生进程消失证据，退出证据必须在清理动作之前采集并登记，禁止以 runner 自己造成的消失反推进程死亡。

登记前提双条件不变（r9 措辞更新，原「指派前提双条件」）：runner 仅在「capture 三形态关联后确认对应位点后静默 **且** 存在独立死亡证据」时为死亡后未执行项登记具名 `unobservable(cause=...)`；单 marker 缺失不构成登记依据。（r9 删除原 r4 R6「探针 fault 被洗成平台事实 → pass 的路径的关闭声明」及其承载机制分类表行 3（join 封锁行）——该 fail 映射随死因表废除，损失登记见自陈 14(a)；join 阻塞事实仍由 `dw_join_result=join-blocked-observed` 照常登记，不因收口形态改变。）

### criteria-gap 处理（预注册，BL-7；第三趟 B2 收紧）

**删除** r0 的「判据无法按本文字面求值 → blocked + 返回 T0」条款（含 S4 对应分支）——决议与 schema 的 `blocked` 都只限外部基础设施不可达。替代规则（**两类运行形态分立，判别方法冻结**）：

- **`unobservable` 仅限「明确预注册的平台不可观察原因」**：即本文各字段取值域内逐字写明的 `unobservable(cause=…)` 清单（如 `no-live-fd`/`no-live-vpn`/`dup-failed`/`proc-introspection-unavailable`/`barrier-timeout`/`value-outside-frozen-domain`（r4 第一趟 R7 新增）等，以本文冻结取值域为准）。
  **字面拼法（r9 第六步冻结）**：cause 字面一律写作 `unobservable(cause=<name>)`，裸形式 `unobservable(<name>)` 不是合法字面——清单归属按 `cause=` 句式逐字比对，裸形式出现即属域外记录格式的拼写错误，不因缺 `cause=` 前缀而另立域外值。**`criteria-gap` 不是任何字段运行时 `unobservable` 的合法 cause（r3 第一趟 D7 删除）**：未预注册的不可观察原因一律走下方 fail 路径（fail + raw 逐字入档），禁止塞进兜底桶洗白为 `unobservable` → pass。
- **判别方法（r4 第一趟 R7 两分法）**：(1) **未枚举的平台值——不 fail**：探针已成功采集该字段的有效观测值、值本身有效但落在本文冻结取值域之外（不得把新值硬塞进最近似桶）——对发现 campaign，采到未枚举的返回值/errno/`Fault_Type` **正是产出，不是失败**
（决议：平台行为不如预期永远不是 fail）：字段记 `unobservable(cause=value-outside-frozen-domain)` 且**原值逐字入档**，verdict 不因此 fail。
(2)-(4) 为**记录器义务未尽（解析器/真值表缺口）**，命中任一即判 `fail` 并逐字保留 raw 原文——(2) **解析域缺口**：chunk 重组、marker 解析、字段提取等解析器无法处理已到达的原始数据，及 **capture 自相矛盾 / P12 派生与 runner 重建不一致**（r18 随 F8 同步扩入——同源两写 + 标志门下分叉只可能来自记录器缺陷，grok M-03）（解析器缺陷不是平台事实）；
**r12 豁免限定（「不可解析」双口径单值化，grok B-01 第二部分；裁定 = 豁免路线）**：fault 条目的 `Fault_Type`/`Signal` 两**字段**不可解析（字段行缺失或值无法提取）不走本条——走「死亡事实记录（证据向量）」节证据规则 5 的分量 unobservable（`unobservable(cause=fault-type-unparsable)`/`unobservable(cause=signal-unparsable)`，经 r12 求值规则的 unknown cause 编码进入签名聚合，不 fail）：这两个字段的字面正是本 tuple 未实测的平台产物，对解析失败记 fail 违反决议 §4.2 发现语义。
**豁免边界仅此两字段**：其余字段（时间戳、pid、文件名等）不可解析仍走本条（判别方法 (2) → F8 fail；时间字段既有收口不变），不得外推；(3) **字段无法求值**：判定输入存在但派生规则/真值表未覆盖该输入组合（如 `dw_return_class` 判定表穷尽兜底之外出现矛盾输入）；(4) **未预注册的不可观察原因**：观测无法建立、但其 cause 不在本文任何字段取值域的 `unobservable(cause=…)` 逐字清单内。
(2)-(4) 各情形均 **verdict `fail`**（完整性轴：记录器未完成其预注册采集与求值义务），raw 原文逐字入档，留待记录级独立审查定夺是否为判据缺口修订；禁止现场补「等价字段」、禁止现场改字面、禁止降为 `unobservable` 洗白探针/解析器缺陷。
- criteria-gap 的**计数与逐字段清单**仍由 runner 汇总进记录，留待**记录级独立审查**处置（仅作记录级台账，不再是任何字段的合法取值）；Live 中不改 verdict、不重跑、不消费额外判定。
- `blocked` 仅限上节所列 infra / 元组漂移 / 哈希漂移三类（Allow 盒到点按 B3 裁定 (a) 的 r3 第二趟 E6 修正不属 blocked，按已消费 campaign 收口）。

## 停止条件（逐条自足写出，不引用他门）

N0 决议五项停止条件沿用如下；出现任一即停止并返回 T0：

1. native core 需要私有/高维护 patch；
2. 正式 NDK 无法构建（**加载失败不属本条**——D1 的 dlopen 失败是负面事实，登记后协议继续）；
3. 真实 VPN fd/protect 不可满足——本 campaign 即对其**发现**：矩阵全拒（无 fd）是登记事实与协议终态之一，**不触发本条**；protect 面不在本 campaign 范围（归 N2）；
4. 固定版本 compat oracle 无法定义；
5. 范围扩展到第二协议面。

本 campaign 特化：

- **S1** 构建失败：BoringTun checksum 漂移、`--offline --locked` 失败、feature 集偏离冻结值；
- **S2** 白名单外 HDC/设备命令（白名单见「流程」节 HDC 表，AUTH 只能引用该表不得扩），或使用 `protect`、特权能力、外部 endpoint——立即停止并登记违规。**VpnExtensionAbility 与 `VpnConnection`（含 `create`/`destroy` 及 RouteInfo 配置矩阵）的使用是本 campaign 范围内行为，不构成越权**；
- **S3** 元组漂移/OTA/HDC 退化（有界短循环内不恢复即 blocked 停）；
- **S4** criteria-gap 不构成停止，也**不再是任何字段的合法 `unobservable` cause**（r3 第一趟 D7）：仅「明确预注册的平台不可观察原因」（各字段取值域逐字清单内的具名 cause）可记 `unobservable` 后协议继续；**两分法（r7 X7 与 criteria-gap 判别方法同步）**：
  - 探针已成功采集的**有效平台新值**（取值域外但值本身有效）→ 记 `unobservable(cause=value-outside-frozen-domain)` + 原值逐字入档，**不 fail**（判别方法 (1)，采到未枚举平台值正是发现产出）；
  - **解析缺口、字段无法求值、未预注册的不可观察原因三情形一律判 `fail` 并保留 raw 原文，不得记 `unobservable`**（判别方法 (2)-(4)）；
  - 停止仅限 infra 不可达/元组漂移/哈希漂移三类（S1/S3 及 verdict 节 `blocked` 清单）；
- **S5** 测量开始后判据不得修改。

## 非范围（双向不外推）

- **无任何功能 pass 条件**：本文不含 fd 合同、数据面、E4 或任何门的功能判据；U1-U7 与 waiter 事实只是 N1b r2 的预注册设计输入（决议 §4.4）。
- 不主张：x86_64/Emulator、socket protect（N2）、DNS/多地址/多路由/MTU 变更与重建（N6/N2a，见台账）、management/signal/relay/ICE（N3-N5）、产品实现、性能/吞吐/长稳、渠道、其他设备/build/API。
- **结论边界逐项**：D7 只覆盖冻结 20000 ms 与冻结负载形态；D-W/D6 只覆盖本协议单一 destroy 事件、单一 waiter、T_dw=5000 ms；U1/U2 只覆盖冻结地址与冻结包形态；D8a 只登记写返回谱与 1400 的字面一致性（`write_return_boundary_consistent_with_1400`），**不主张实际 MTU 或 oracle 存在性**。
- DISC ↔ N1b、DISC ↔ 其他元组/负载/时长双向不外推；`verdict: pass` 不得在任何后续文档中被引用为平台行为结论或任何门的功能结论（`docs/evidence-schema.md:90`）。
- **本判据不构成设备 Live 授权**：物理执行须用户显式授予新 AUTH（决议 §4.3.9）。

## 开放义务台账核对

按台账使用纪律（`docs/open-obligations-ledger.md:11`：判据预注册须核对归属本门条目，遗漏即审查 blocker），逐条处置如下。**DISC 只采集事实，不关闭任何条目**——其产出是各条关闭路径的输入：

| 条目 | 与本 campaign 关系 | 本 campaign 处置 |
| --- | --- | --- |
| OB-01 tun fd 背压（`docs/open-obligations-ledger.md:29`） | D8b storm 即其发现载体 | 仅采集 `eagain_observed` 等字段；即使 `observed-true` 亦**不关闭**（保守口径：决议 §4.4 把 DISC 限定为设计输入；台账关闭触发要求的「后续具名 campaign 观测并落盘」由 N6/N7 被动观察字段路径完成（决议 §二.5，`docs/native-nx-n1b-adjudication.md:68`）。**此保守读法与台账触发条件字面存在张力，登记待 T0/台账持有人确认**） |
| OB-02 部分写（`:30`） | D8a/D8b/D5 逐轮 write 返回 | 同 OB-01：采集 `partial_write_observed`，不关闭 |
| OB-03 shutdown unblock（`:31`） | D-W 即其发现载体 | 采集 `dw_waiter_spawned`/`dw_entry_confirmed`/`dw_return_class`/`dw_inwait_evidence_source`/`dw_inwait_confirmed`/`dw_destroy_distinguishable_from_timeout`；路径①/②按 D-W 节六条件合取采认门槛在 N1b r2 冻结时书面选定（决议 §三.5 预授权，`docs/native-nx-n1b-adjudication.md:85-87`）；路径①可达性受决议「确认进入等待」要求的用户态可达性约束（见 D-W 节「决议约束正面登记」） |
| OB-04 post-destroy fd 子项（`:32`） | D6 即 U4 载体 | 采集 `u4_*` **逐子项**三态；§六.3 a/b/c 三分支出口的判定输入（`docs/native-nx-n1b-adjudication.md:151-154`）；r2 划定 C10 子集用子项，不用摘要 |
| OB-07 MTU 实际生效 oracle（`:35`） | D2.7 先验登记 + D8a 写返回谱 | 采集 `mtu_api_oracle=no-preregistered-oracle`、`d8_write_boundary_last_success_len`、`write_return_boundary_consistent_with_1400`；`mtu_oracle_exists` 恒 `unobservable(cause=no-preregistered-oracle)`——**本 campaign 不驱动 OB-07 转 `unassigned`**（该处置属持有门 + T0；写返回分界不构成 oracle 证据；`ConnectionProperties.mtu` 存在但未预注册为 oracle） |
| OB-05 / OB-06 / OB-08 / OB-09（`:33-37`） | 不归属本 campaign | 显式声明：无归属条目、不触碰（OB-05 归 N2a、OB-06/OB-09 归 N6、OB-08 由声明 IPv6 的门承担且本 campaign 冻结配置无 IPv6 声明） |

## 流程（G0 十三门范式适配，`docs/g0-go-arm64-physical-probe.md:174-188`）

适配沿用、不照抄 G0 专属 ELF/Go 输入（G0 的被测输入在此替换为：已签名 HAP + arm64 `.so` 成员 + runner + marker capture 剖面）：

| 门 | DISC 适配 |
| --- | --- |
| 1 host-only 同步 | 同步 trusted refs/bundle；clean HEAD 含本登记+runner+selftests+docs；记录 `code_sha`；HDC0 固定绝对 host `ps` 探针 |
| 2 候选 ID 消费审计 audit-1 | 仓外双文件 + `.sha256`（AUTH-N1BDISC-… 候选 pair） |
| 3 freeze + 静态审查 | 判据 freeze；reviewer 静态审查（符号清单誊录核对、**实现配置字面 == 本文 MR 表字面（不一致即不得 freeze）**、静态断言 A1-A10（r10 纳入 A9、r18 纳入 A10，A M-04/B M-01）、时间盒表核对；**r4 第二趟 S1 新增：`Fault_Type` 候选集收敛前置——若届时已能取得目标元组的一条真实 faultlogger 文本，须据此把候选集收敛为实测字面并重新提交独立审查；若取不到，则以「死亡事实记录（证据向量）」节冻结候选集执行并在证据中登记该未实测依赖（r9：原「以死因分类节 S1 兜底出口执行」及其行 4 (iii)/行 5 分岔随死因表删除——冻结前候选集收敛义务本身保留，域外字面按该节证据规则 5 记 `other:<原值逐字>`、不驱动 fail）**） |
| 4 host-prep `tconn` + 一次内存级 `list targets` | 同（窄例外沿用） |
| 5 `-TargetBindingConfirm` | 3 白名单探针；**完整系统版本实测复核**（E3 记录 7.0.0.100 与冻结值 7.0.0.102 的差异在此裁决）；漂移即 blocked record + 退役 |
| 6 ready freeze draft | 绑定 confirmation record |
| 7 reviewer record | `n1bdisc-ready-freeze-review`（0 blocker / 0 major） |
| 8 最终 ready freeze | 绑定 clean HEAD、runner bytes、signed HAP/profile/cert、`.so` 成员 hash、符号清单、配置矩阵、全部外部输入 |
| 9 候选 ID 消费审计 audit-2 | 严格 host-only |
| 10 selftests | host-only，**全部用例类别集中列举如下**：①–⑪ 各类用例（外提为下方「gate 10 selftest 清单」，r4 第四趟 W1 重构，内容逐字不变） |
| 11 同一 ready freeze DryRun | `is_evidence=false`、HDC0、integrity empty |
| 12 DryRun 独立审查 | 复算 freeze SHA-256，确认字节不变 |
| 13 单次 Live | `PYTHONUNBUFFERED=1`、按证据目录增量与状态文件时间监控、不因暂无终端输出中断、**不 retry** |

**gate 10 selftest 清单（r4 第四趟 W1 自上表行 10 外提，内容逐字不变）**：

① marker 正反例（**含 `:vpn` tag 三形态用例**，E3 0001 事故教训，`docs/evidence/e3-physical-preflight-authorization-2026-08-14-0002.md:5`；**含 `N1BDISC_D4_READ` 的 `off=both` 取值用例**，与 `u1_match_offset` 取值域对齐；**r9 A9 反例：源码把 `N1BDISC_DW_DESTROY_C` 发射写在 `destroy()` 调用之前的四步序颠倒夹具，静态断言 A9 必须 fail**；
   **r16 legacy class 负例回归钉（sol m-01——r15 删除 `class=<c>` 字段时一并删去的回归钉，随本包补回）**：legacy `DW_RETURN|...|class=<n>` 夹具（携带已删的 class 字段）→ 静态/解析检查必须拒绝）；

② chunk 重组校验（**含 E3 冻结的 `(stream,item)` 分组用例：跨 stream 不混组、组内 index 缺口判重组失败；r4 第二趟 S6 统一：同组同 index 重复片首到者优先且须与重复片逐字节一致——一致登记 `duplicate_chunk_observed` 不改判定、不一致判 fail；同 index 重复不再仅因重复判重组失败；r5 U12 补全：同 index 重复片 `count` 或 `sha256` 字面不同 → 直接 fail**）；

③ poll 合法域门矛盾输入用例（**E1**：`ret=0`+revents 非空、`ret>0`+revents 空、`ret` 落在 {-1,0,1} 之外、`ret=-1` 无 errno——均须 fail 且不进入 13 类判定表）；**r9 第五步 BL-4 单调钟绝对域门用例**：`DW_RETURN` 携 `elapsed_ms=-1`（其余输入合法）→ fail（F8）；`DW_DRAIN` 携 `elapsed_ms=-1` → fail（F8）；`DW_DESTROY_T.mono_ms` 晚于 `DW_DESTROY_C.mono_ms` → fail（F3，顺序约束）；
   **r10 D8b 窗口钟载体用例（B B-04）**：D8b 合法完成夹具（storm 至首次 EAGAIN 正常终止或熔断收口，`D8_STORM_BEGIN|ws=<n>` 与 `D8_STORM_END` 全字段含 `we=<n>` 均在 capture）→ `window_start_monotonic`/`window_end_monotonic` 自 `ws`/`we` 重建、无字段缺项、均 ≥ 0 且 `ws` ≤ `we` → 不命中 F8 → verdict **`pass`**（D8b 成功终态稳定可达）；
   反例三支——`ws`/`we` 任一缺项 → 字段缺项 fail（r12 限定：指对应 marker 在而字段空缺的缺项；END 整条缺失落下方 r12 BEGIN-only 用例及其对照例，不属本支），`we` < `ws` → F8 顺序约束 fail，任一负值 → F8 非负性 fail（沿钟域门 r10 载体条款）；
   **r12 D8b BEGIN-only 死亡收口用例（blocker 4，sol B-04 = grok M-02）**：正例——夹具 PRE 在、D7 完成、`D8_STORM_BEGIN|ws=<n>` 在、`D8_STORM_END` 缺、平台 SIGKILL `:vpn`（`process_death_observed = observed-true`：positive 基线在、finally 采样 absent + capture 静默）、无窄崩溃签名；
   期望——`window_end_monotonic` 与 `eagain_observed`/`partial_write_observed`/`bytes_written_total`/`write_calls`/`caps_hit` 各记 `unobservable(cause=storm-incomplete-pre-only)`、`window_start_monotonic` 自 `ws` 照常重建、`storm_incomplete_pre_only=true` 与跨度登记在档 → 无字段缺项（「任一 D 项终态为 `missing`（既无探针登记亦无 post-mortem/skip 指派）」判定不适用本支）、不命中 F4/F8 → `protocol=pre-only` → verdict **`pass`**（合法平台死亡终态不得因 END 缺失烧 ID）；
   对照例——BEGIN 在、END 缺、无死亡证据（进程仍活）→ 死亡收口支不触发、各字段不得取 `storm-incomplete-pre-only` → 真正的未完成：字段缺项沿 F4 面、观测窗到点进程仍活 → **fail（F9）**（存活未完成不是平台终态，D8b 节 r12 对照条）；
   **r12 阶段未达死亡收口用例（r12-P3 施工中发现；r13 注：同一发现，标签统一为 r12-P3 施工发现，原写「P4 遗留 ②」；对应 D8b 节阶段未达支）**：死亡位点在 `N1BDISC_D8_STORM_BEGIN` 前（如死于 D7——`process_death_observed = observed-true`、BEGIN 缺）→ D8b 七字段各记 `unobservable(cause=stage-not-reached)`、`storm_incomplete_pre_only=false` → 不命中 F4 → `protocol=pre-only` → verdict **`pass`**（合法平台死亡终态，同 BEGIN-only 支法理）；
   （**r13 补全断言**：本夹具死于 D7 任务中途——`D7_BEGIN` 在、`D7_END` 缺、BEGIN 后静默 + 死亡证据 → `u7_long_task_watchdog_behavior=observed-false`（任务被杀，沿 D7 节既有 observed-false 支）、`start_mono_ms` 自 `D7_BEGIN` 照常重建——r13 前本用例不断言 u7，随 u7 窄窗支落地补全，防该分量无断言漂移）；
   **r13 u7 阶段未达正例（第十三轮 blocker 1，grok B-01 = sol B1 两席收敛；对应 D7 节 r13 阶段未达死亡收口支）**：夹具 `N1BDISC_PRE` 在、`N1BDISC_D7_BEGIN` 缺（D7 未开始）、平台死亡（`process_death_observed = observed-true`：positive 基线在、finally 采样 absent + capture 静默）、无窄崩溃签名、`last_visible_site = P5T`
   → `u7_long_task_watchdog_behavior=unobservable(cause=stage-not-reached)`、D7 派生字段 `start_mono_ms`/`elapsed_ms`/`iters` 同名落值同 cause → 无字段缺项 → 不命中 F4/F8/F1 → `protocol=pre-only` → verdict **`pass`**（grok 构造：PRE 已发、`D7_BEGIN` 未发的合法平台死亡——r13 前 u7 三支前件全假、无落点 → F4 烧 ID，随本支闭合）；
    **r14 skip-first 夹具（grok m-02 验收钉；对应 D7 节「skip 分支优先」段）**：夹具 D2 矩阵终局无保留条目（全 rejected → `no-live-vpn` 分支）+ `N1BDISC_PRE` 在 + D7 的 SKIP marker 尚未发出即平台死亡（`process_death_observed = observed-true`）→ `u7=unobservable(cause=no-live-vpn)`（skip 表 D7 行指派）、**不得 `stage-not-reached`**（skip 语义是「跳过」而非「未达」，skip 分支优先于阶段未达支）；
   **r10 D7 早退矛盾边界对（B B-08；D7 节接受域首行 r10 口径的边界承载）**：`D7_END` 在、`elapsed_ms=19999`（`0 ≤ elapsed < 20000`，其余输入全合法）→ marker 在而与伪码出口条件机械矛盾 → **fail（F8）**（原值逐字入档，`d7_anomaly=early-exit-with-end-marker` 标签同时驱动 F8）；`elapsed_ms=20000` + `D7_END` 在 → 合法下界 → 不 fail（`u7=observed-true` 方向）——同源边界对，20000 恰值归合法侧沿 E5 边界归属冻结；

④ `dw_return_class` 优先级判定表真值表（全部 revents 组合（**r4 第二趟 S2 界定 = 冻结掩码全集 {0x001,0x002,0x004,0x008,0x010,0x020} 的全部 64 个子集的十进制编码**——该穷举界定判定表行内匹配的组合域；**r16 未知位钉拆两钉（grok B-01 传播对齐，原「纯未知位 64 → `other-revents`」单钉拆除）**：`revents=64` + 无 SKIP + `_C` 缺 → **0b**（0/0b 前置检查先于未知位门——r15 派生序验收钉）、`revents=64` + 无 SKIP + `_C` 在 → `other-revents`（未知位门）不 fail；
**r9 第五步 BL-3 混合位用例，r16 同款两钉**：`revents=72`（未知位 64 + `POLLERR`）与 `revents=65`（未知位 64 + `POLLIN`）各拆两钉——`_C` 缺 → **0b**、`_C` 在 → 由未知位前置门定 `other-revents`、原值入档、不 fail、**不解锁路径①**（不得命中行 7/行 8））× elapsed 4500 边界 × drain 终态 × poll ret/errno 组合，见 D-W 节）；
   **r12 派生字段逐值落值用例（blocker 7，sol B-07 验收钉；r13 起改述（r15 起按四步口径：(0)/(1) 透传与直接映射、(2) destroy-unresolved、(3) 逐值表）；r16 拆写（sol M-01：原「18 个 class 在 resolved 前提下逐值过步 (3)」与「skip/death 走 (0)/(1)」同句两读，拆除）**：
   `dw_return_class` 19 值（r17 补 `poll-never-returned` 1 值；r13 第三包死亡收口再拆更新；当轮 17 值、原 16 值）按步分域逐值落值——步 (0) 6 值（r17：skip 2 + 死亡收口 3 + `poll-never-returned` 1）+ 步 (1) 2 值逐值透传/直接映射；**普通 11 类**按 resolved（步 (3) 逐值表）/ 无 resolve（步 (2) destroy-unresolved）分核——
`fd-event-like`+10 s 内 resolve → `observed-true`、`timeout-like`+resolve → `observed-false`、普通 11 类的其余 9 值按 r12/r13 具名 cause 落 `unobservable(cause=…)`（步 (0) 6 值逐值透传——skip 2 + 死亡收口 3 + `poll-never-returned` 1，与上句统一（r18：原「skip 5 值」为 r17 扩 `poll-never-returned` 前的旧计数残留，grok m-01 = sol m-01 两席收敛）、类 0/0b 2 值在步 (1) 直接映射，不入步 (2)/(3)——r16 计数随拆写同步：原「其余 16 值」混入了 (0)/(1) 域），逐格核对无裸 `unobservable`、无缺项
（complete 主线 `pre-destroy-ready` 构造落 `unobservable(cause=no-destroy-correlated-event)`，POST 子项三选一编码有合法单值）；
   **r13 二维矩阵用例（blocker 3，sol B-04 验收钉；`(class, destroy 结局)` 至少覆盖下列五格，逐格唯一落值、不得两行并存）**：`fd-event-like`+destroy timeout → **`unobservable(cause=destroy-unresolved)`（不是 `observed-true`——缺 resolve，步 (2) 先于 class 判定）**；`fd-event-like`+resolved → `observed-true`；`timeout-like`+resolved → `observed-false`；
`pre-destroy-ready`+destroy timeout → `unobservable(cause=destroy-unresolved)`（不是 no-destroy-correlated-event——原 r12 两行并存的构造自此单值）；`pre-destroy-ready`+resolved → `unobservable(cause=no-destroy-correlated-event)`；reject 结局任选一格复核同落 `destroy-unresolved`、结局差异只入 raw 档。
   **r14 四步补格用例（blocker 1，grok B-01 = sol B-03 验收钉——原二维声明的洞格与两读格）**：
   `destroy-call-unobserved`（0b）+ 未 resolve（`_C` 缺——worker 已 RETURN、主线程发 `_T` 后死于调用边界）→ **(1) 直接映射 `unobservable(cause=destroy-call-unobserved)`，不是 `destroy-unresolved`**；
   `post-destroy-unobservable` + 无 resolve（`_C` 在、无 RETURN——E3 方向）→ **(0) 透传 `unobservable(cause=post-destroy-unobservable)`，不是 `destroy-unresolved`**；
   `destroy-skip-proven` + 任意结局 → (1) SKIP 位点字面；三格共同钉死「先到先得」的步序。
   **r15 派生序补格（sol B-01 验收钉——未知位 + `_C` 缺的原洞格）**：`ret=1, revents=64`（或 65/72）+ worker 已 RETURN/EXIT + 进程死于 `_T`/`_C` 发出前（无 SKIP、`_C` 缺、无 resolve）→ **0/0b 前置检查先命中 → 类 0b** → `dw_destroy_distinguishable` 步 (1) `unobservable(cause=destroy-call-unobserved)`，**不是 `other-revents`、不落 F8**（r15 前未知位门先分流 other-revents、四步无落点 → F8 烧合法平台死亡，随派生序重排闭合）；
   对照组 `ret=1, revents=64` + `_C` 在 + 无 resolve → 未知位门 → `other-revents` → 步 (2) `destroy-unresolved`（未知位门只对无 SKIP 且 `_C` 在的输入分流——重排后的边界钉死；r16，deepseek M-01：SKIP 在的输入已被前置检查路由类 0、到不了本门）。
   **r16 前置检查两钉（`item=destroy` 更正 + sol B-02 分域验收）**：`ret=1, revents=64` + `N1BDISC_SKIP|item=destroy` 在 + `_C` 缺 → **类 0 `destroy-skip-proven`**（0/0b 前置检查的 SKIP 支先于未知位门——SKIP 支与表行 0、五态表、四步映射逐字同一、只认 `item=destroy`）；
`revents=64` + `N1BDISC_SKIP|item=D-W` + `DW_RETURN` 同现 → **F8(2) fail**（矛盾输入：D-W 整体被 skip 则无 poll、RETURN 不应存在——分域声明验收钉；r17 轴更正，grok M-01：F3 活字面是全序/时序非单调，矛盾双 marker 属 capture 自相矛盾轴——仍在 fail 闭集、verdict 不变，仅轴归属更正）。
   **r17 complete∧join-timeout 夹具（三席收敛，第十七轮 B-01 反例 B 验收钉；分域声明 (d) 支验收）**：夹具 create/dup 成功 → `DW_SPAWN`/`DW_DRAIN`/`DW_BARRIER`/`DW_INWAIT`（confirmed 正常采集）/`DW_DESTROY_T`/`DW_DESTROY_C` 在、destroy resolve（D6a 在）→ worker poll 挂死不返回（`DW_RETURN`/`DW_EXIT` 缺）→ P10 终态轮询盒 8 s 到期：记 `join-timeout`、`join-timeout-worker-abandoned=true`；
→ **D6b skip**（r18 重裁：发 `N1BDISC_SKIP|item=D6b|cause=join-timeout-abandoned`、各子项字段 `d6b-skipped-join-timeout`）→ P11 → P12、`N1BDISC_POST` 在 → `protocol=complete`；
期望——`dw_return_class=unobservable(cause=poll-never-returned)`（r17 新值、19 值域内；非 skip、进程活到 P12、join-timeout 已登记、RETURN 缺——四前件齐）、`dw_join_result=join-timeout`（**既有本体值——join 域不扩、10 值不变（r17 核对结论：该 cause 仅入 class 域）**）、`dw_destroy_distinguishable_from_timeout`=步 (0) 透传（输出逐字等于输入编码本身）；
`dw_poll_ret`/`dw_poll_errno`/`dw_poll_revents`/`dw_poll_return_elapsed_ms` 各记同 cause（poll 未返回、RETURN raw 无值），`dw_drain_*` 保 `DW_DRAIN` marker 真实值（不盖，r10 反 blanket 法理）→ POST 各字段有值、无字段缺项 → fail 闭集（F1-F6、F8、F9）均不命中 → verdict **`complete pass`**（终态标志只在 RETURN 后置位 → 该合法 complete 形态必然无 RETURN，不得烧 ID）。
**r18 落值断言（D6b skip 编码）**：POST `d6_items` D6S4..S7 各子项 `skipped(cause=join-timeout-abandoned)`（D6a S1..S3 result 编码不变；`SKIP|item=D6b` 不改道类 0——类 0 仅认 `item=destroy`——亦不触发 (a) 全量指派，D-W 未整体 skip）、`u4_dup_getfd`/`u4_dup_read`/`u4_dup_close`/`u4_dup_fd_reuse` 各记 `unobservable(cause=d6b-skipped-join-timeout)`。
**r18 归因洁净反例钉**：join-timeout 形态下主线程对 `fd_dup` 的任何 D6b 操作 = 违反归因洁净（「watchdog 暴露窗口」缓解 3：worker poll 终态之前主线程不触碰 `fd_dup`）——静态断言 A10 必须 fail（源码层反例 = join-timeout 分支控制流含对 `fd_dup` 的任一 read/write/fcntl/close 调用点，见静态断言表 A10）。
   **r17 barrier-never-observed 钉（grok m-03，B-01 反例 A 验收；分域声明 (c) 支验收）**：夹具 worker 卡死未发 BARRIER → 主线程 7+8 s 盒尽后发 `SKIP|item=destroy|cause=barrier-never-observed` → 交观测窗 → 进程死亡（PRE 在、POST 缺、无崩溃签名）→ 五态 `not-called` → **`dw_return_class`=类 0 `destroy-skip-proven`（分域 (c)：无 RETURN、经 SKIP 存在性直接判类）** → 四步 (1) SKIP 位点字面（`barrier-never-observed`）→ 不命中 F4/F8/F1 → pre-only **pass**。
（r18 口径注：上句「终态标志只在 RETURN 后置位」为本钉形态的描述——本形态 worker 确未返回；迟到返回形态由标志已置门分流、不入本钉，见分域声明 (d) r18 归因洁净补注与 (e) 支。）

⑤ U3 分区表全行用例（**r4 第二趟 S5 取代 E4 优先级序**：offset-0 与 offset-4 均可解析的帧**无条件**须定 `ambiguous`（含前 4 字节 `00 00 08 00` 的构造——旧 E4「两 offset 均可解析且 `00 00 08 00` 须定 `tun_pi-like`」用例随 S5 废除）；仅 offset-4 可解析且前 4 字节 `00 00 08 00` → `tun_pi-like`；仅 offset-4 且非 tun_pi 形态 → `other-prefix`；仅 offset-0 → `no-prefix`；均不可 → `unparsable`）；

⑥ fault 条目解析契约正反例夹具（E7：字段行首匹配、时间字段格式、不可解析时间条目）：
  - `Fault_Type` 归一化正反例（r4 第二趟 S1 冻结归一化规则；r9 期望结论按「死亡事实记录（证据向量）」节证据规则 5「归一化 + 候选集」重写）：`APP_FREEZE`/`app-freeze` 须命中 `APPFREEZE`，`JS_RAWERROR` 须命中 `JSRAWERROR`；
  - 域外词根 `JSCRASH`：归一化后不落候选集 → `fault_type_observed = other:JSCRASH`（原值 = 归一化前逐字原文）→ 不命中 `probe_crash_signature_observed` 三支任何一支 → `observed-false` → F1 不命中、**不 fail**（仅作事实登记）；
  - r9 同键对照组：同一 `JSCRASH` 夹具配「无平台信号」与「有 `Signal=SIGKILL`（或 SIGTERM）条目」两支，**期望结论相同**——`signal_observed` 照记、`probe_crash_signature_observed` 仍 `observed-false`、仍不 fail（SIGKILL/SIGTERM 不在致命信号集 {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`} 内）；原 r4 版「无 → 行 5 → fail；有 → 行 4 (iii) → 不 fail」分岔随死因表删除，「两支结论相同」本身即本组用例的断言点；
   - **r10 多条目聚合用例（blocker 8，B B-07；聚合规则 = 证据规则 5 末段）**：(a) 同窗双条目 `Fault_Type=APPFREEZE` 与 `Fault_Type=CPPCRASH` → 逐条解析后聚合 `fault_type_observed=CPPCRASH`（正向优先、不被 `APPFREEZE` 稀释）→ 第 1 支命中 → `probe_crash_signature_observed=observed-true` → `protocol=pre-only` 夹具 → **fail（F1）**；
   - (b) 同窗双条目：`APPFREEZE` 条目（`last_visible_site`=P6/D7 位点，非短时步集）+ 另一条目携 `Signal=SIGSEGV` → `signal_observed=SIGSEGV`（致命段优先）→ 签名经第 2 支 `observed-true` → **fail（F1）**——`APPFREEZE` 条目因位点非短时步集不构成第 3 支，聚合后由第 2 支独立命中；
   - (c) 同窗双条目均域外字面（如 `JSCRASH` + `JSERROR`）→ 两条原值**全部逐字入档**（raw 档保留多值）、`fault_type_observed` 分量取第一条（`other:JSCRASH`）→ 三支无一命中 → `observed-false` → **不 fail**；
   - (d) **r10 混合可解析性用例（可解析与不可解析条目混合；聚合规则 r10 混合支）**：同窗夹具 = 可解析条目 `Fault_Type=APPFREEZE`（`last_visible_site`=P6/D7，非短时步集）+ 一条 `Fault_Type`/`Signal` 字段均不可解析的条目 → `fault_type_observed=unobservable(cause=fault-type-unparsable)`、`signal_observed=unobservable(cause=signal-unparsable)`（无可解析正向且存在不可解析条目——不可解析条目中可能藏正向签名，按 `observed-false` 记是 false-negative）
→ 签名按 r12 两步求值落 `unobservable(cause=fault-type-unparsable)`（支 1/2/3 全 unknown、无 true，cause 沿冻结优先序取 fault-type-unparsable；原「经真值表行 2」随按支求值重写废除）→ F1 不命中 → pre-only 夹具**不 fail**（除非另命中其他闭集条）；raw 档保留全部条目原值逐字及命中的全部 unknown cause 逐字；
   - **r12 验收钉（blocker 1；正向不被旁路未知稀释）**：**单条目**夹具 `Fault_Type=CPPCRASH` 可解析 + `Signal` 字段不可解析 → `fault_type_observed=CPPCRASH`、`signal_observed=unobservable(cause=signal-unparsable)` → 按支求值：支 1 true、支 2 unknown、支 3 false（`CPPCRASH` 非短时步支字面，可求值判 false）→ 聚合 true > unknown > false → 签名 `observed-true`
（r10 行 2 结构下此构造被洗成 unobservable → pre-only pass 的 fail-open 路径，随 r12 废除）→ `protocol=pre-only` 夹具 → **fail（F1）**；
   - **r13 FaultRecv 按文件部分失败用例（blocker 4，sol B-01 验收钉；取回状态建模见求值规则前置 r13 条、聚合扩展见证据规则 5 混合支 r13 条）**：
   - (a) 部分失败 + 已知正向不被稀释：文件 A 取回成功、其条目 `Fault_Type=CPPCRASH` 可解析；文件 B 取回失败 → A 照常逐条解析参与聚合（B 不产生条目）→ 支 1 true（A 的条目）→ 聚合 true > unknown → `probe_crash_signature_observed=observed-true`
（取回失败文件的 unknown 贡献不稀释已知正向——三值聚合 true > unknown 天然保证，r13 钉死）→ `protocol=pre-only` 夹具 → **fail（F1）**；
   - (b) 部分失败 + 无正向：文件 A 取回成功、其条目 `Fault_Type=APPFREEZE`（`last_visible_site`=P6/D7 位点，非短时步集）；文件 B 取回失败 → 无可解析正向且存在取回失败文件 → `fault_type_observed`/`signal_observed` 各记 `unobservable(cause=faultrecv-unavailable)`（cause 沿冻结优先序：取回失败高于字段不可解析）
→ 三支全 unknown、无 true → 签名 `unobservable(cause=faultrecv-unavailable)` → F1 不命中 → pre-only 夹具**不 fail**（APPFREEZE 在 P6 非短时步本不构成第 3 支正向；B 中可能藏正向，宁 unobservable 勿 false）；
   - (c) 全局失败回归钉：命中文件全部取回失败（或 `FaultProbe` 命令本身失败——两成因各设一夹具）→ 原前置行为：两分量各记 `unobservable(cause=faultrecv-unavailable)`、签名同 cause → 不 fail（与 r10 行 1 原行为逐格一致）。
   - (d) **r15 raw 级谓词 × 部分失败用例（sol M-01 + grok m-04 验收钉；聚合规则 = 证据规则 5 混合支 r15 raw 级正向谓词块）**：文件 A 取回成功、其条目可解析 `Fault_Type=APPFREEZE` + 全局 `last_visible_site` ∈ D2 短时步集 + marker 序列无矛盾；
文件 B 取回失败 → `fault_type_observed=APPFREEZE`（raw 谓词 (3) 命中、不被失败稀释）、`signal_observed=unobservable(cause=faultrecv-unavailable)`（本分量无正向——谓词 (2) 不命中且存在失败源，跨分量不传导）；签名经 r12 按支求值第 3 支 true → `probe_crash_signature_observed=observed-true`，verdict 由 F2 承载（短时步 ⇒ `N1BDISC_PRE` 未发——r10 结构注同源）。

⑦ stat state 解析（A7：`comm` 带空格/括号反例）；

⑧ fake-HDC 沙箱；

⑨ PRE/POST 通道用例（r4 R1：PRE 在+POST 在 → `complete`；PRE 在+POST 缺 → `pre-only` 且 `u4_*` 按 r5 U6 + r7 X3/X4 分岔（锚点 = `DW_DESTROY_C`；r9 对齐 `destroy_call_state` 五态改四支，并补时序三分带用例））：
  - `DW_DESTROY_T` 缺 + `DW_DESTROY_C` 缺 **且无任何 `N1BDISC_SKIP|item=destroy`**（r10 补前提——已发出任一 SKIP 时五态为 `not-called`、落 (1b)，不得用本行）→ 死亡后未执行子项 `unobservable(cause=destroy-not-reached)`（五态 `not-reached`；不得假负值；r9：原「`DW_DESTROY_C` 缺 → `unobservable(destroy-never-called)`」随五态废除——marker 缺失不再等值「未调用」）；
  - r8 Y1 反例（r9 结论改正）：`DW_DESTROY_T` 在而 `DW_DESTROY_C` 缺 → 五态 `unobservable(cause=call-boundary-incomplete)`，未执行子项同 cause——既可能尚未调用、也可能已进入调用未返回；`_T` 只证「即将调用」，但 `_C` 缺也**不得**反推「调用未发起」（原 `destroy-never-called` 结论废除）；
  - 时序三分带用例（r9；`DW_DESTROY_T`/`DW_DESTROY_C` 均在，比较按「worker 返回时序三分带」）：`at_mono_ms` < `T_mono_ms` → `definitely-pre-invocation` 一例；`at_mono_ms` > `C_mono_ms` → `definitely-post-invocation` 一例（路径①条件 3 唯一接受的带）；`T_mono_ms` ≤ `at_mono_ms` ≤ `C_mono_ms`（含 `at == T`、`at == C` 等值构造——毫秒等值不能证先后）→ `unobservable(cause=invocation-window-ambiguous)` 一例，不解锁路径①条件 3、不归因；
  - `DW_DESTROY_C` 在而 `DW_DESTROY_T` 缺 → marker 顺序/完整性错误 → **fail（F3）**（五态第 5 行；r10：verdict 走完整性轴 F3、`destroy_call_state=unobservable(cause=marker-contradiction)` 在死亡事实轴如实落值——两轴独立，本用例逐格核对分量有值）；
  - skip 显式证明：`N1BDISC_SKIP|item=destroy` 显式发出（no-live-fd 分支与 r9 第五步 barrier-never-observed 顺延路径）→ `destroy_call_state=not-called`（唯一可证「未调用」的途径）；
  - **r10 SKIP 同现域门用例**：`N1BDISC_SKIP|item=destroy` 显式发出的同一 capture 流中又出现 `DW_DESTROY_T` 或 `DW_DESTROY_C` 任一枚调用锚 → 域门先于五态表命中（矛盾输入，不进入任何单行归类）→ `destroy_call_state=unobservable(cause=marker-contradiction)` → **fail（F3，顺序破坏轴）**——不得按五态第 1 行记 `not-called`、亦不得按调用锚记 `call-returned`/`call-boundary-incomplete`；
  - **r9 第五步 BL-5 验收用例**：五个 create 全部被平台拒绝 → `no-live-fd` 分支 → POST 照发——`d6_items` D6S1..S7 逐子项 `skipped(cause=no-live-fd)`、`dw_return_class=unobservable(cause=no-live-fd)`、`dw_join_result=unobservable(cause=no-live-fd)`、`dw_*` 终态字段沿具名 skip 清单落值、无字段缺项 → verdict **`pass`**（合法平台负面结果不得因 POST 无 skip 编码而 fail）；
  - **r9 第五步 barrier-never-observed 用例**：该顺延路径补发 `N1BDISC_SKIP|item=destroy|cause=barrier-never-observed` → 五态 `not-called`、死亡后未执行子项 `unobservable(cause=destroy-not-called)`（「未调用」在全文的唯一证明途径 = 显式 SKIP；本路径旧记 `not-reached` 的表述废除）；
  - **r10 用例 A（五 create 全拒 + 死于 POST 前——V2 (1b) 扩域验收）**：五个 create 全部被平台拒绝 → `no-live-fd` 分支 → 按实际协议位点依次发出 `N1BDISC_SKIP|item=destroy|cause=no-live-connection`（destroy 位点）与 `N1BDISC_SKIP|item=D-W|cause=no-live-fd`（P10 位点——该路径 D-W 整体被 skip，此 SKIP 同在，用例按实际流构造）后、`N1BDISC_POST` 发出前进程死亡；
`protocol=pre-only`、`destroy_call_state=not-called`、死亡后未执行子项 `unobservable(cause=destroy-not-called)`（V2 (1b) 任一 SKIP 支）、`dw_return_class=unobservable(cause=no-live-fd)`、`dw_join_result=unobservable(cause=no-live-fd)`（`dw_*` 按 skip 编码真实落值，`SKIP|item=D-W` 即其输入证据）→ 无域外值、无字段缺项 → verdict **`pass`**（合法平台负面结果的 pre-only 形态；
r10 前 (1b) 仅认 `barrier-never-observed`，本构造无落点 → F4（r10 订正索引，A m-03：未预注册 cause 走判别方法 (4)、归 F4——原写 F8 沿旧 F4/F8 论域重叠口径，同为 fail 闭集命中，verdict 结论不变））；
  - **r10 用例 B（E3 成功终态回归——pre-only `dw_*` 真实求值，blanket 删除验收）**：`N1BDISC_PRE` 在、`DW_DESTROY_T`/`DW_DESTROY_C` 在（destroy 经共享子协议发起）、D-W 于死亡前完整跑完（`DW_RETURN`/`DW_DRAIN`/`DW_EXIT` 在）、进程 terminal（独立死亡证据在）、无 `N1BDISC_POST`、无崩溃签名 → `protocol=pre-only`；
`dw_return_class` 按 D-W 节派生序（合法域→0/0b→未知位→1-11）对真实输入求值（如 `fd-event-like`/`timeout-like`——**不是** `unobservable`）、`dw_join_result` 按 marker 真实求值（P10 未及执行而无 join marker 时记 `unobservable(cause=post-destroy-unobservable)`，同属域内值）、死亡后未执行子项按 V2 四支分岔 → fail 闭集（F1–F6、F8、F9；F7 不在内）均不命中、无字段缺项 → verdict **`pass`**（r10 措辞同步，A m-02）
（r10 前本构造被原 blanket 句整盖为域外值 `unobservable(cause=post-destroy-unobservable)` → 判别方法 (4) → fail 烧 ID（r10 订正索引归 F4，原写 F8 沿旧 F4/F8 重叠口径，同为 fail，A m-03））；
  - **r12 死亡收口 cause 分流用例（sol M-01）**：夹具 PRE 在、死于 D7（`D7_BEGIN` 在、`D7_END` 缺、`DW_DESTROY_T`/`DW_DESTROY_C` 均缺、无任何 `N1BDISC_SKIP|item=destroy`——五态 `not-reached`；死亡证据在：positive 基线转 absent + capture 静默）、D-W 未执行、`dw_*` 判定输入不可得 → `dw_return_class` 与 `dw_join_result` 记 `unobservable(cause=destroy-not-reached)`（**不得**记 `post-destroy-unobservable`——destroy 当时从未被调用过，旧单一 cause 与事实相反；死亡收口 cause 为预注册落值、非缺项）；
    对照——`DW_DESTROY_T`/`DW_DESTROY_C` 在（destroy 已过）而 worker 未及 poll 即死 → 记 `unobservable(cause=post-destroy-unobservable)`（destroy-已过支）；
    对照二（r13 第三包，sol M-01）——`DW_DESTROY_T` 在、`DW_DESTROY_C` 缺（五态 `call-boundary-incomplete`——既可能尚未调用、也可能已进入调用未返回）而 worker 未及 poll 即死 → 记 `unobservable(cause=call-boundary-incomplete)`（**不得**记 `post-destroy-unobservable`——「destroy 已过」对该歧义态是事实错误，透传自身 cause）；
  - `DW_DESTROY_C` 在且 destroy 已 resolve → 死亡后未执行子项逐项 `observed-false`（含「进程死于 D6b 中途、D6a 正结果不被覆盖」用例）；
  - `DW_DESTROY_C` 在但无 resolve 证据 → `unobservable(cause=destroy-unresolved)`（r7 X3/X9 补分岔用例）；
  - 死亡前已有 result marker 的子项保持记录值；无死亡证据不进入 pre-only 的反用例；
  - PRE 缺 → `fail`；双 marker 任一冻结字段缺项 → `fail`；
  - PRE/POST 各自 `ledger_digest` 与同切点 `N1BDISC_FD` 重建一致性（r5 U5：PRE 对 P5T 快照切点、最终 digest 对收口形态切点，跨切点比较禁止）；
  - **r17 P12 派生比对钉（grok B-03 = sol M-01 两席收敛；F8(2) 挂载验收）**：夹具 `DW_RETURN` raw 合法、按 D-W 节派生序 runner 重建得 `timeout-like`，POST `dw_outcome.dw_return_class` 由 P12 错写 `fd-event-like`（探针 P12 派生值与 runner 从 capture raw 的独立重建值不一致——capture 内自相矛盾）→ **fail（F8(2)）**（错写值在 class 域内，域外/缺项门不接手，比对不一致唯一挂 F8(2)）；
  - **r9 补登（S3 节「selftest 须含」的 ledger 状态机用例同步入清单，措辞逐字同源）**：`dw_inwait_proc_fd` 双实例（inst=1/2）配对用例、close 先于 create（→ fail）、重复 close（→ fail）、`by=` 域外字面（→ fail）、pre-only 收口未关闭实例 → `process-exit` 的重建用例（r7 X6：r6 W3 后 pre-only 记 `process-exit`，旧「→ `host-forcestop`」用例废除）、另设「存活 fail-cleanup（观测窗到点收口、ForceStop 前进程仍活）→ `host-forcestop`」用例；

⑩ 死亡事实七分量记录 + F1 三支闭集真值表（**r9 整块重写**，替代原「死因分类总函数真值表」——被测对象改为「死亡事实记录（证据向量）」节七分量记录义务与 `probe_crash_signature_observed` 三支闭集，死亡侧 fail 面仅 verdict 节 F1（含 `protocol != complete` 限定）；原 7 行表、行 1-7 及 (i)-(iv) 分支、四合成标签、`elapsed_proxy`、`site_uncertainty` 均已随死因表删除，其旧用例随表废除）：
   - 三支命中各一例（r4 第一趟 R5 短时步集收窄沿 F1 第 3 支位点守卫承载；三例断言均为对应支命中 → `observed-true`；Ⅰ/Ⅱ 命中后 `protocol != complete` 时经 F1 → **fail**，Ⅲ 的 verdict 由 F2 承载（见行内 r10 注）、第 3 支 `observed-true` 为事实记录验证——r10 引言修正，A M-03）：
     Ⅰ=`Fault_Type=CPPCRASH`（`JSRAWERROR` 同支）新增条目 + 进程消失 + `N1BDISC_PRE` 在（死于 P5T 后，如 P6/P7）→ `protocol=pre-only`、第 1 支命中 → fail；
     Ⅱ=新增条目 `Signal=SIGSEGV`（`SIGABRT`/`SIGBUS`/`SIGFPE` 同支变体）→ 第 2 支命中 → 同判 fail；
     Ⅲ=`Fault_Type=APPFREEZE` + `last_visible_site` ∈ 冻结短时步集（D2 保留条目锁定序列 2.1-2.7 的即返 syscall，位点在 P5T 之前故 PRE 未发出）+ marker 序列无矛盾 → 第 3 支命中 → fail（r10 措辞更正，A M-03：本构造同时命中 F2（PRE 未发）、**verdict 由 F2 承载**——PRE 缺失时 `protocol` 不可求值，F1 不进入求值；第 3 支 `observed-true` 在此作为**事实记录**验证（签名分量正确赋值），非独立 fail 触发——
第 3 支位点集全部位于 P5T 之前、对 pre-only/complete 协议恒假（结构注见三支闭集第 3 支后）；原「`protocol != complete` 成立」「本用例钉死的是第 3 支独立成立」两句随 r10 废除）；
   - 第 3 支守卫反例（r12 断言修正，blocker 5）：`APPFREEZE` + 位点在短时步集内但 marker 序列自相矛盾（如位点 marker 违反时序单调/全序）→ 第 3 支**不成立** → `probe_crash_signature_observed = observed-false`（**事实记录验证**：证据不足以正向成立不得默认 true，席 B 守卫）；
**verdict 由完整性轴承载**——位点在短时步集 ⇒ `N1BDISC_PRE` 未发 ⇒ **fail（F2）**，且 marker 序列自相矛盾 ⇒ 叠加 **fail（F3）**；本用例验证的是第 3 支不稀释为 true、非 verdict 断言（原「→ 不 fail」随 r12 删除——该断言与 F2/F3 正文规则非单值，两席收敛确认）；
   - 非短时步集上的 `APPFREEZE`：`last_visible_site` = P6/D7（20 s 合法时间盒任务）或 P1/P2/P3/P4/P7/P8/P9/P10 任一 → 第 3 支位点守卫不成立、第 1/2 支亦不命中 → `observed-false`，`fault_type_observed`+`last_visible_site` 照常记录、未完成项按既有规则赋 `unobservable` → **不 fail**（本 campaign 要发现的平台行为正例，决议 §4.2）；
   - `Signal=SIGKILL`/`SIGTERM` 条目（r12 限定，blocker 6：`Fault_Type` 须不命中第 1/3 支——如 `APPFREEZE` 于非短时步位点、或域外词根；原「含与任意 `Fault_Type` 组合」废除——「任意」含 `CPPCRASH`/`JSRAWERROR` 时第 1 支必真、原断言非单值）→ 不在致命信号集 {`SIGSEGV`, `SIGABRT`, `SIGBUS`, `SIGFPE`} → 第 2 支不命中、第 1/3 支亦不命中 → `signal_observed` 照记、`probe_crash_signature_observed=observed-false` → **不 fail**；
   - r12 对照例（blocker 6）：`Fault_Type=CPPCRASH`（可解析）+ `Signal=SIGKILL` → 第 1 支真 → `probe_crash_signature_observed=observed-true`（按支求值下不受第 2 支的 SIGKILL 影响——SIGKILL/SIGTERM 不在致命信号集只使第 2 支不命中，不抵消第 1 支）→ `protocol=pre-only` 夹具 → **fail（F1）**（P1 的「`CPPCRASH` 可解析 + `Signal` 字段不可解析」验收钉在本「Signal 可解析且属平台终止段」情形下的补充钉）；
   - 无任何新增 faultlogger 条目 + 进程消失（`PidOfVpn` positive 基线后转 absent + capture 三形态静默）→ 七分量逐项取值：
     `process_death_observed=observed-true`、`last_visible_site`=最后可见 marker 位点、`fault_type_observed=observed-false`、`signal_observed=observed-false`、`destroy_call_state` 按其域独立取值、`marker_tail_state` 按五值谓词独立求值（r10 五值域）、`probe_crash_signature_observed=observed-false` → **不 fail**（**原 r4 版此构造落行 6 `unattributed` → `fail`、r9 起不 fail**——该 fail 映射随死因表删除，本例为其替代钉死用例）；
   - `protocol=complete` + 命中崩溃签名（r12 限定 Ⅰ/Ⅱ 夹具另发 `N1BDISC_POST`；原「Ⅰ/Ⅱ/Ⅲ 任一」废除——Ⅲ 的短时步位点与 POST 结构上不可达：POST 在则 `last_visible_site` 至少 P12（POST 为全序末位阶段 marker，P12 后无 `N1BDISC_` marker），第 3 支位点守卫必不满足，Ⅲ 的 complete 形态不存在，blocker 6）；
→ F1 的 `protocol != complete` 限定不满足 → **不进 F1、不 fail**，`probe_crash_signature_observed` 仍如实记 `observed-true` 并按 verdict 节登记入 runner evidence 记录（需 N1b 关注的异常观察；r10 载体修正：POST 冻结字段集不承载该观察、marker 已封口不得追写）（r9 作用域恢复条款的钉死用例）；
   - 七分量互不推导反例：任一分量不得由其他分量的缺失反推——`fault_type_observed=observed-false`（无 fault 条目）**不得**推出 `process_death_observed=observed-false`（后者仅由死亡证据闭集独立判定（r12：`PidOfVpn` 基线转 absent + capture 静默为唯一充分条件，携 `SIGKILL`/`SIGTERM` 条目仅为 pidof absent 确认下的并存关联证据；「进程退出记录」旧支已废））；
反向同构：进程消失不推出 `fault_type_observed`/`signal_observed` 任一非 `observed-false` 值；`marker_tail_state` 任何取值不参与 `probe_crash_signature_observed` 求值；
   - 逐格核对义务：上述每格七个分量各自有值（记录义务），verdict 断言仅可经 `probe_crash_signature_observed` × F1 触达。

⑪ **`dw_watchdog_killed` 三态映射真值表 + `marker_tail_state` 边界用例（r4 第三趟 T1 补全 E11 全分支；r9 更新：原「D7 死亡路径墙钟代理边界（`elapsed_proxy` 27000/27001）」与「r8 Y4 X5 置位五用例（`site_uncertainty` 置位、行 2 让位）」两组用例的被测对象（`elapsed_proxy`、`site_uncertainty`、行 2）已随死因表删除，整段废除，改挂 `marker_tail_state` / `T_tail = 25000 ms` 口径）**：
`observed-true` 五合取逐格用例（r15 校正计数：原计数沿 r13 有序表③起实况改「五合取」，deepseek M-03/grok m-01；**r12 更新，blocker 2：第四合取项改挂 `process_death_observed = observed-true`，正向用例须补死亡分量构造**——原「r9 第五步收紧后正向证据仅剩 faultlogger watchdog/`APPFREEZE` 签名路径一例」的构造随第四项改写废除；
**r13 再更新（有序互斥表③，第十三轮 blocker 2）：正例改「`_T`/`_C` 均缺」构造；r14 再更新（③ 收紧 + 去因果化，sol B-01/grok 裁量①）**——
正例：`DW_SPAWN` 在、`DW_EXIT` 缺、`DW_SPAWN` 后 capture 静默、`DW_DESTROY_T`/`DW_DESTROY_C` 均缺（destroy 未及），**且**死亡分量 = positive 基线在、finally 采样 `pidof` absent + capture 静默（携 `SIGKILL` 条目作并存关联证据——仅死亡分量的关联证据、不证明 watchdog 归属），死亡位点 P8，
**且 `dw_inwait_confirmed = observed-true`（仅此一项——poll 进入证据第五合取；r15 收紧 sol B-03：不再接受 `DW_BARRIER`，正例构造改为 inwait 采样确认）** → `observed-true`（**「waiter 于 destroy 未及窗内死亡且已确认进入等待」——r14 去因果化结论，断言中不得出现「被 watchdog 杀」**）；
**r13 反例两支（原 r9 第五步「`_T` 在 `_C` 缺与 `_T` 缺 `_C` 缺两支均须判 `unobservable(cause=marker-gap-indeterminate)`、不得 true」的双缺反例废除——双缺支随有序表改为上方③正例）**：「`DW_DESTROY_C` 在 + `PidOfVpn` absent」→ `unobservable(cause=destroy-terminal-candidate)`、不得 true（求值序①，destroy 之后的死亡不是 waiter 被杀的语义窗口）；
「`DW_DESTROY_T` 在、`_C` 缺、**`DW_EXIT` 缺**」→ `unobservable(cause=call-boundary-incomplete)`、不得 true（求值序②，调用边界不可判、waiter 归因随之不可判；r14 更正：原不带 EXIT 缺——「EXIT 在」时 ④ `observed-false` 先承载，② 不截胡）；
**r14 新增反例二支（grok M-01 = sol B-02 收敛验收钉 + sol B-01 收紧验收钉）；r15 新增反例 (c)（sol B-03 收紧验收钉）**：
（a）「`_T` 在、`_C` 缺、**`DW_EXIT` 在**」（inwait 竞态交错：worker 提前 RETURN+EXIT、主线程后发 `_T`、死于 `_C` 前）→ ④ `observed-false`（waiter 正常退出），**不得** `call-boundary-incomplete`；
（b）「`_T`/`_C` 均缺、五合取中 poll 进入证据缺**（无 `DW_BARRIER` 且 `dw_inwait_confirmed != observed-true`——drain 期死亡）**」→ ⑤ `unobservable(cause=marker-gap-indeterminate)`，**不得 true**（③ r14 收紧；r15 起「进入证据缺」= inwait 非 true，BARRIER 在否均落 ⑤，见 (c)）；
（c）「`_T`/`_C` 均缺、`DW_SPAWN` 在、`DW_EXIT` 缺、capture 静默、死亡分量确认、位点 P8，**`DW_BARRIER` 在而 `dw_inwait_confirmed != observed-true`**（drain 后、poll 调用前被抢占或未及采样——BARRIER→poll 调用间隙死亡）」→ ⑤ `unobservable(cause=marker-gap-indeterminate)`，**不得 true**（r15 收紧验收钉，sol B-03：BARRIER 不再是 ③ 的 poll 进入证据）；
（d）**r16 整条 pre-only pass 夹具（sol B-03 点名，inwait 死亡收口验收钉）**：trace = `PRE→SPAWN→DRAIN→BARRIER→死亡`（`N1BDISC_DW_INWAIT`/`DW_DESTROY_T`/`DW_DESTROY_C`/`DW_RETURN`/`DW_EXIT` 均缺；死亡分量确认、位点 P8 poll 等待窗内、无正向崩溃签名条目）——
inwait 四载荷字段（`dw_inwait_evidence_source`/`dw_inwait_confirmed`/`dw_inwait_samples` 及其 errno 承载）各记 `unobservable(cause=inwait-marker-unobserved)`；`dw_watchdog_killed` = `unobservable(cause=marker-gap-indeterminate)`（⑤，inwait 非 true、③ 前件不成立）；
`dw_return_class` = `unobservable(cause=destroy-not-reached)`（死于 P8、destroy 未达——五态核对：`_T`/`_C` 均缺、无 SKIP → `not-reached`，pre-only 收口透传）→ 不命中 F4（各字段有预注册落值）/F8（无域违反）/F1（无正向签名）→ `protocol=pre-only` → verdict **`pass`**（合法平台死亡终态）；
（e）**r16 post-poll 阻塞反例（sol B-04 点名）**：采集窗内 poll 已返回、worker 阻塞于 HiLog（state=`S`）、`proc-syscall` 不可读 → 该样本不满足收紧后三条件（state=S ∧ syscall 可读 ∧ 号 ∈ {73}）→ **不得 `observed-true`**；窗内无其他可确认样本 → `observed-false`（宁缺勿误）——不得经 `dw_watchdog_killed` ③ 或路径① 条件 1 解锁；
「`DW_DESTROY_C` 在而 `PidOfVpn` absent」原反用例（r9 记 `marker-gap-indeterminate`）随 r13 具名 cause 改记 `destroy-terminal-candidate`（并入上方反例第一支，不另设）；
五合取缺任一即不得 true 的逐格反例（r15 校正计数，deepseek M-03/grok m-01）；**r12 新增反例（死亡分量未确认时签名条目不得证 true）**：`pidof` 仍非空（positive 基线在、采样恒非空 → `process_death_observed=observed-false`）+ `APPFREEZE` 条目 + `DW_SPAWN` 在、`DW_EXIT` 缺、`DW_SPAWN` 后静默 → **不得 true**，判 `unobservable(cause=marker-gap-indeterminate)`（死亡分量未确认；APPFREEZE 只证 watchdog 事件、不证进程消失））；
`observed-false` 仅 `DW_EXIT` 在的用例；
skip 分支具名 `unobservable` cause（`no-live-fd`/`dup-failed`；r14 更正：原第三字面 `no-worker` 为 r3 S7 幽灵引用、已删）与「死亡分量未确认（`process_death_observed != observed-true`，r12 统一谓词，原「无死亡证据默认」措辞随 blocker 2 收敛）默认 `unobservable`」用例；
**r9 `marker_tail_state` 边界用例（r10 更新：五值域逐值覆盖——原四值域补 `no-death-evidence` 后的逐值覆盖；本字段为纯观察登记、已与任何 fail 判定解耦，本组用例不得断言它影响 verdict）**：
(a) 静默跨度恰 25000 ms → 谓词为严格大于（死亡证据墙钟 − 最后可见 marker 墙钟 > `T_tail = 25000 ms`）→ **不**置 `possible-tail-loss`，落 `tail-loss-not-indicated`；
(b) 静默跨度 25001 ms → 置 `possible-tail-loss`（与 (a) 构成同源边界对，句法沿本项原 27000/27001 对写法；r9 起置位仅改观察标签，不再有「行 2 让位」效应）；
(c) `N1BDISC_POST` 在 → `tail-complete`（终态 marker 已封口，无尾丢失可能，静默跨度不再参与）；
(d) POST 缺 + 有死亡证据但差值 ≤ `T_tail` → `tail-loss-not-indicated`；
(e) 任一墙钟不可求值（如 faultlogger 条目无时间戳且无 capture 静默判定墙钟）→ `unobservable(cause=tail-clock-unresolvable)`（r10 具名，原省略号废除）；
(f) 减法方向反例：死亡证据墙钟早于最后可见 marker 墙钟（差值恒为负）→ 永不置位，沿 r8 Y2 冻结的减法方向；
(g) **r10 第五值用例**：POST 缺 + 无任何死亡证据（进程仍活或状态不可判，F9 路径）→ `no-death-evidence`（原四值表对该路径无值，本用例钉死缺口闭合；本构造的 fail 由观测窗到点收口规则独立出，与本观察值互不推导）。

**清单↔正文对应关系（r4 第三趟 T1 登记：本行为索引，以各节正文为准，逐格冲突时正文优先；r9 按 ⑥⑩⑪ 重写后的实际被测对象更新该三项，①–⑤/⑦–⑨ 复核后仍指向活规则不动）**：① ↔ A5 冻结字面集与「执行位点与结果通道」节；② ↔ E3/C4 chunk 编码与 S6；③ ↔ E1 poll 合法域门；④ ↔ D-W 节 13 类判定表（r15 起前置 0/0b 检查，行 1-11）与 S2 编码；⑤ ↔ U3 分区表（S5）与 `u3_prefix_format` 派生表（T3）；⑥ ↔ E7 解析契约、S1 归一化与「死亡事实记录（证据向量）」节证据规则 5「归一化 + 候选集」/`other:` 记法（原「兜底出口」随死因表删除）；
⑦ ↔ A7 stat 解析；⑧ ↔ fake-HDC 沙箱；⑨ ↔ R1 PRE/POST 求值规则与 E2/S3 ledger 重建 + digest 一致性；⑩ ↔ 「死亡事实记录（证据向量）」节七分量记录义务 + `probe_crash_signature_observed` 三支闭集 + verdict 节 F1 闭集（原「死因分类表（含行 5 出口、R3/T6 布尔、R4 墙钟代理）」已删）；⑪ ↔ E11 `dw_watchdog_killed` 赋值规则与 `marker_tail_state` 五值表/`T_tail`（r10 四值表扩五值表；原「R4 墙钟代理规则」已删）。

（注：十三门表行 11–13 原位于本清单之后，r4 定稿后移回表内行 10 之后，孤儿行删除——位置修复，内容逐字不变。）

独立审查须为跨厂商隔离席、不得由主会话自身模型家族充任（决议 §4.3.7，`docs/native-nx-n1b-adjudication.md:130`）；0 blocker 方可请用户授权 Live。

### DISC HDC 操作白名单（逐条枚举，MJ-9）

沿 G0 白名单结构（`docs/g0-go-arm64-physical-probe.md:138-159`）；bundle `cn.alfadb.netbird.n1bdisc`，staging 根 `/data/local/tmp/netbird-n1bdisc`，占位符 `<PHYS_1_TARGET>`/`<HAP_DISC>`。除下列外任何 HDC 子命令禁止（G0 永久禁令同沿：设备进程枚举/`ps`、宽泛进程发现、UDID/serial/target discovery、`hidumper`、root/privileged、`uiInput`、外部 endpoint、全量查询、截图/layout 采集）：

1. **gate 4 host-prep**：恰一次 `tconn <runtime-endpoint>` + 恰一次内存级 `list targets`（用户本人或显式授权代执行）。
2. **gate 5 target-binding 三探针**：`hdc version`（无 `-t` 前缀）/ `-t <T> shell param get const.product.model` / `-t <T> shell param get const.product.software.version`。
3. **campaign（runner 白名单，下列具名操作 + 复用 gate 5 三探针，不得增加）**——**唯一操作名集合与每条的完整 argv 形态逐字冻结如下（`<T>` = 已绑定目标句柄占位；除注明者外均含 `-t <T>` 前缀；白名单审计直接按本表执行，不依赖 freeze record 再解释）**：

   | 操作名 | 完整 argv（冻结形态） |
   | --- | --- |
   | `BundleDump` | `-t <T> shell bm dump -n cn.alfadb.netbird.n1bdisc` |
   | `PidOf` | `-t <T> shell pidof cn.alfadb.netbird.n1bdisc`（UI 进程，无 `:vpn` 后缀） |
   | `PidOfVpn` | `-t <T> shell pidof cn.alfadb.netbird.n1bdisc:vpn`（观察 `:vpn` 子进程存活；`:vpn` 死亡证据的唯一白名单来源，死亡事实记录见「死亡事实记录（证据向量）」节（r9：原「死因分类见 verdict 节」随节更名改指）；**使用位点恰二（r3 第二趟 E7）**：① positive 基线采样——首个 `N1BDISC_` marker 出现后恰一次（见该节证据规则 2）；② host finally 步骤 3——死亡事实冻结（r9：原「死因分类冻结」）前、`ForceStop` 之前的采样步（r3 第一趟 D6）。可行性先例与回退条件见该节证据规则 2） |
   | `MkdirStaging` | `-t <T> shell mkdir -p /data/local/tmp/netbird-n1bdisc/hap` |
   | `SendHap` | `-t <T> file send <HAP_DISC> /data/local/tmp/netbird-n1bdisc/hap/n1bdisc.hap` |
   | `InstallHap` | `-t <T> shell bm install -p /data/local/tmp/netbird-n1bdisc/hap` |
   | `StartEntry` | `-t <T> shell aa start -a EntryAbility -b cn.alfadb.netbird.n1bdisc -m entry` |
   | `HilogStream` | `-t <T> shell hilog -T N1BDiscVpn -v year -v zone`（墙钟兜底上界 ≥825 s——StartEntry 之前启动；**按终态停止（r3 第二趟 E8，r4 R1 更新触发字面）**：`N1BDISC_POST` 出现、或 `pre-only` 收口完成（host finally 步骤 9；r6 W8 同源：停流仍以「POST 在或 finally 步骤 1（先到为准）」为准——步骤 1 早于步骤 9，本处仅为收口完成后的确认参考，不构成额外停流条件）、或 host finally 步骤 1 即停，825 s 仅为 runner 异常兜底；求值窗 525 s 自首个 `N1BDISC_` marker 起算，与「runner 观测窗」节逐字同一） |
   | `FaultProbe` | `-t <T> shell find /data/log/faultlog/faultlogger -maxdepth 1 -type f -name '*cn.alfadb.netbird.n1bdisc*' -print`（沿 G0 `FaultProbe` 形态，`docs/g0-go-arm64-physical-probe.md:152`） |
   | `FaultRecv` | `-t <T> file recv /data/log/faultlog/faultlogger/<命中文件> <host路径>`（仅对 FaultProbe 命中条目逐个） |
   | `ForceStop` | `-t <T> shell aa force-stop cn.alfadb.netbird.n1bdisc`（Reason 仅限 `exception-cleanup`/`final-cleanup`） |
   | `Uninstall` | `-t <T> shell bm uninstall -n cn.alfadb.netbird.n1bdisc` |
   | `RemoveStaging` | `-t <T> shell rm -rf /data/local/tmp/netbird-n1bdisc` |
   | `StagingProbe` | `-t <T> shell ls -ld /data/local/tmp/netbird-n1bdisc` |
   | `PidOfPost` | argv 同 `PidOf`（finally absent 复核） |
   | `PidOfVpnPost` | argv 同 `PidOfVpn`（finally absent 复核） |
   | gate 5 三探针复用 | `hdc version`（无 `-t` 前缀）/ `-t <T> shell param get const.product.model` / `-t <T> shell param get const.product.software.version` |
4. 操作名大小写不敏感；未知操作/多余参数/缺参数/bundle 不符/Reason 非法一律拒绝。**AUTH 记录只能引用本表，不得扩。**

### host finally cleanup（冻结，MJ-11）

Live 后 runner finally 序列**无论成败、进程死亡、观测窗到点或超时，一律执行且顺序冻结**（不依赖探针存活）：

1. 停止 HilogStream（若在采）；
2. `FaultProbe`（faultlogger find）→ 对命中条目 `FaultRecv` 逐个取回（七分量证据向量的证据源；r9：原「死因分类证据源」随死因表删除改写）；**只采 campaign 时间窗内新增条目**（与 `StartEntry` 前执行的 `FaultProbe` 快照集合比对，快照集合逐字登记——见「死亡事实记录（证据向量）」节证据规则 1）；
3. **`PidOfVpn` 采样（r3 第一趟 D6 新增）**：执行 `PidOfVpn`（白名单既有操作）登记 `:vpn` 进程存在性/absent——它是七分量证据向量的关键判定输入（r9：原「死因分类的关键判定输入」随死因表删除改写；喂给 `process_death_observed` 分量），**必须在下一步 ForceStop 之前采集**；禁止以步骤 8 的 `PidOfVpnPost` 替代本步（`PidOfVpnPost` 仅限 finally absent 复核，正文已禁止用作死亡事实判定输入）；
4. **死亡事实冻结（r9 改写，原「死因分类冻结（B1）」）**：按「死亡事实记录（证据向量）」节逐分量登记七分量取值与全部输入证据（快照集合、新增条目、步骤 3 的 `PidOfVpn` 结果、最后成功 marker 位点），**不产出任何合成分类**（七分量互不推导、不合成任何单一「死因」标签，沿该节记录义务）；**须在下一步 `ForceStop` 之前完成**——`ForceStop` 自身产生的进程消失证据不得参与证据向量求值；
   **`N1BDISC_PRE` 已发出时（r4 R1/R2；r9 起死亡侧触发器为 F1），证据向量不承载 pass/fail**——它只产出死亡事实记录与未完成项的 `unobservable` cause，pass/fail 由 PRE/POST 求值规则与 fail 闭集决定（唯一死亡侧触发面 F1：`probe_crash_signature_observed = observed-true` 且 `protocol != complete` → fail，闭集定义见「死亡事实记录（证据向量）」节；`protocol=complete` 不进 F1）；
5. `ForceStop(final-cleanup)`；
6. `Uninstall`；
7. `RemoveStaging`；
8. 定向 absent 探针（`BundleDump` 未安装、`PidOfPost` 空、`PidOfVpnPost` 空、`StagingProbe` absent）——四者均 absent 方为 `verified-clean`；
9. 终态判定与封签（r4 R1；触发器 r9 改按 fail 闭集 F1）：
   - `N1BDISC_POST` 已在 → 核验 `protocol=complete`，**不进 F1**（命中崩溃签名仅按 verdict 节登记入 runner evidence 记录——需 N1b 关注的异常观察，不驱动 verdict；r10 载体修正：POST marker 已封口、冻结字段集不得追写，观察登记落 runner evidence 记录）；
   - 仅 `N1BDISC_PRE` 在 → 按步骤 4 已冻结的死亡事实记录收口 `protocol=pre-only`（**合法终态**：死亡证据引用与 D6/D-W 的 `unobservable` cause 登记入 evidence 记录，不代发探针 marker；仅当 `probe_crash_signature_observed = observed-true` 且 `protocol != complete`（即 F1，三支闭集见「死亡事实记录（证据向量）」节）触发 `fail`；无死亡证据 → `fail`，见观测窗到点收口规则）；
   - `N1BDISC_PRE` 缺 → `fail` 收口。
10. integrity 记录收尾。

（沿 G0 场景 S1 的 finally 范式：`FaultProbe` → `ForceStop(final-cleanup)` + `Uninstall` + `RemoveStaging` → 定向 absent 探针，`docs/g0-go-arm64-physical-probe.md:170`；DISC 在 `FaultProbe` 之后、`ForceStop` 之前增补 **`FaultRecv`（步骤 2）与 `PidOfVpn` 采样步（步骤 3，r3 第一趟 D6 插入）**，并增补封签次序（步骤 9-10）——DISC finally 全序 1-10 及各步编号以「host finally cleanup」节冻结清单为准，本对照括注仅示与 G0 范式的结构对应。）

## 起草人自陈（偏向登记，决议 §九，`docs/native-nx-n1b-adjudication.md:177`）

以下为已知的自利解释方向，全部提交审查席对抗性核验。r0 自陈五项的裁断结果已吸收：第 1 项（post-mortem）与第 4 项（D8 位次与时间盒）按修法重写；第 2 项（first-accept-lock）保留并按裁定补后位 `unobservable(protocol-first-accept-lock)`（**防误引注，r10：裸拼法自 r9 字面拼法冻结（`cause=` 句式逐字比对）起非合法字面，现行字面为 `unobservable(cause=protocol-first-accept-lock)`、在活规则中（D1 节 `u1` 取值域闭合句与 u5 登记句）；本处为 r0 裁断吸收句的历史登记，照登不改字面**）与 MB1 不伪答 U1；第 3 项（close 清单）按 MJ-8 扩为完整清单读法；第 5 项（OB-01/02 保守不关闭）保留。

1. **post-mortem 死因分类的边界**：把「平台终止类死亡」纳入协议完成度（可 pass）仍降低本设计 fail 概率；r1 用「仅平台终止签名可 pass、探针 fault 与无法归因一律 fail」收紧，但 `platform-termination` 的判定仍依赖 faultlogger 的 `Fault_Type` 归类——若平台把探触发的崩溃也记为 APPFREEZE，仍可能洗白。
第三趟 B1 已收紧为「优先级全序 + APPFREEZE 死亡位点约束（r3 第一趟 D2 反转默认并经 r4 第一趟 R5 收窄：仅 D2 保留条目锁定序列 2.1-2.7 判 `probe-fault`，**P1 已由 R5 移出短时步集**，其余位点走 `platform-termination`，探针 hang 洗白由静态断言 A8 堵）+ 时间窗新增条目过滤 + 无探针 crash 签名三条件合取」，但「平台把探触发的崩溃也记为 APPFREEZE」的残余风险仍依赖 faultlogger 归类正确性，属解释而非决议明文。（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）
2. **first-accept-lock**：以牺牲矩阵后位候选的接受性事实换取协议存活率，间接有利于 verdict: pass 的可达性；r1 曾引入的「迟到 fd destroy 关闭」例外已由 r2 第四趟 C1 删除（改为只登记 `late-fd-orphaned` 不关闭）——「绝不矩阵内 destroy」现为无条件冻结，迟到 fd 的悬挂代价见自陈 7(a)。
3. **syscall 允许清单读法**：把 §4.3.5 的「可用」读为下限而非排他，便利本设计（`close`/socket 族/`clock_nanosleep`/`pthread_join` 均超出决议字面列举）；r1 已一次性写全并附读法依据，仍属解释。
4. **时间盒数值均为起草人拍板、无实测依据**：观测窗 525 s（467 s 上界 + 58 s 裕量——裕量取 58 s 是裁量；第二趟修订 barrier 盒 2 s → 7 s、第三趟 r3 更正 P8 并发项不得串行相加〔drain 与 barrier 等待并行，P8 = max(7, 5) + 2 = 9〕，上界冻结为 467、裕量 58，冻结值 525 s 不变）、
Allow 盒 300 s（第三趟 B3 裁定 (a) 经 r3 第二趟 E6 修正：readiness 前置由 StartEntry 之前的 operator-ready 确认步承载；其后的 Allow 超时按已消费 campaign 收口——fail + evidence 记录，不再归 blocked、不再「无记录」；旧「操作员属外部依赖」的引申已废弃）、
迟到观察窗 60 s 且「全矩阵恰一次」、storm 缩减参数（10 s/4 MiB/50 000）、drain 盒 5 s、D8b 移至 P7 的位次。timeout 终止矩阵（迟到窗后不再执行后续条目）牺牲了 MR1B 的一条事实路径，换取窗口上界可控与悬挂隔离。
5. **OB-01/OB-02 不关闭的保守口径**：与台账关闭触发的字面存在张力（见台账核对节），已如实登记而非静默取舍。
6. **r1 新引入的裁量**（本轮新增，逐项登记）：(a) U1 包身份不含 src 五元组（以 payload 16 B 身份替代，规避 `getsockname` 入清单）；
(b) `fd-event-like` 收紧为「仅 POLLHUP/POLLERR 可归因 destroy」——比审查席字面更严（BL-4(c) 的「排除普通 POLLIN」我读作排除归因而非排除登记，另设 `data-ready-post-destroy` 观察类）；
(c) **in-wait 证据路径（`/proc/self/task/<tid>/{stat,syscall}` 同进程自省 + poll 族号 `{73}` 核对）是主会话补充的设计，非三席审查产出**；其可用性在本元组未实测（SELinux 策略与 `hidepid` 挂载均可能使其不可读）。若不可读，DISC 仍收齐其余全部事实，仅路径①无法解锁——这是**预期内的合法结局，不是缺陷**；
该约束与决议的因果关系已在 D-W 节「决议约束正面登记」正面写出，不藏于本自陈；
(d) `family:1` 显式写入（默认值即 1，显式为消除默认值依赖）；
(e) bundle 名 `cn.alfadb.netbird.n1bdisc` 沿惯例拍板；
(f) D8b 的 1024 B storm 包 **id 固定 11 不递增**，checksum 对该 20 字节头一次性重算后全包复用（r1 更正：旧表述「id 自 11 递增、checksum 复用、id 不进校验和」错误——IPv4 Identification 字段（字节 4-5）在校验和覆盖范围内，id 变化必须重算校验和；本 campaign 选固定 id 分支）；
(g) HDC 白名单原采「语义操作表（本文）+ 字面命令行（freeze 记录誊录）」两层结构——**该表述已被第四趟 C5 取代：完整 argv 已逐字写入正文**（见「DISC HDC 操作白名单」表），不再存在两层结构；
(h) `dw_inwait_confirmed` 的样本聚合规则（窗内任一满足样本即 true、`proc-stat` 不可读即 unobservable）为起草人对主会话要求的操作化落地；第二趟更正：`proc-stat` 的 `state=S` 是**必要**证据，`proc-syscall` 可读时其号**参与判定**（≠73 即 `observed-false` 并逐字登记该号）——即 syscall 号是可否决判定结果的条件而非「仅加强核对」；
仅当 `proc-syscall` 不可读时才退化为「stat 单独充分」，与正文判定规则一致（**r16 注：正文 `observed-true` 已收紧为 state=S ∧ proc-syscall 可读 ∧ 号 ∈ {73}——「stat 单独充分」自 r16 起不再成立，本条为历史裁量原文、以 D-W 节 in-wait 三态规则为准**）。
7. **第四趟（C1-C9）新引入的裁量**（逐项登记）：(a) C1 把迟到 fd 的处置从「destroy 关闭」改为「只登记 `late-fd-orphaned`、不关闭」——动机是消灭矩阵内 destroy 的唯一例外，但代价是迟到 fd 悬挂至进程退出，若平台对未关闭 fd 有惩罚性行为（未预注册、未实测），该行为会混入后续观察；我判断该风险远小于矩阵中段进程死亡；
(b) C2 的 D7 读钟上界改由 50 ms `clock_nanosleep` 保证——这给负载形态**新增了一个周期性 syscall**（旧伪码无 sleep），watchdog 对负载形态敏感，故本修正改变了被测负载本身；我将其登记为「冻结负载定义的一次修订」而非等价改写，r2 审查席须确认可接受；
(c) C2 的 elapsed 接受域（r3 第二趟 E5 改写为互斥区间：`[0, 20000)` 异常 / `[20000, 25000]` 合法 / `> 25000` 具名三态观察不判 fail）上限 25000 是拍板值（外层块时长未实测，理论上界 = 20000 + 一次外层块时长，实际远小于 5 s 宽限；`> 25000` 不判 fail 是 E5 裁定——超出部分可由平台调度延迟造成）；
(d) C4 的单片 256 B 上限是拍板值——hilog 单行截断阈值无实测依据，256 B 原始字节 → 约 344 字符 base64，加前缀后单行约 420 字符，是否安全依赖 hilog 行为，selftest 的 chunk 用例须覆盖；
(e) C5 的 argv 表中 `-t <T>` 前缀对全部 campaign 命令统一施加（含 `HilogStream` 长驻流）——G0 先例中部分命令是否带 `-t` 未逐字核对，若目标绑定机制对长驻流有不同要求，freeze 时须复核；
(f) C6 对 read 返回 0 的「终止 drain」语义是定义选择（非阻塞 fd 上 0 的内核语义此处未穷尽论证），其后果是 `end=zero-read` 与正常终止分立登记、不影响路径①时序条件——若 r2 审查认为 0 应重试或视为 fault，属判据修改；
(g) C8 的回退条件把「PidOfVpn 不可观测」导向死因分类项 `unobservable`——**该衔接已由 r3 第一趟 D1 在正文逐字闭合，不再靠本自陈与证据规则 2 的括注衔接**：verdict 节到点收口规则已冻结「`PidOfVpn` 可观测性与终态（PRE）完整性是两个独立的判定输入」——`PidOfVpn` 不可观测不得单独判 fail（只使「`:vpn` 消失证据」无法建立、死因记 `unobservable`），fail 只来自完整性条件本身（`N1BDISC_PRE` 缺失；r5 U7 更正：原此处 `N1BDISC_RESULT` 双缺为 R1 拆分前的旧字面残留）；
`unattributed` 类以「PidOfVpn absent + capture 静默」为前提，前提不成立时自然落入无独立死亡证据路径。审查席若认为判定表与该条款仍有分界缝隙，须逐字指出（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(h) C9.2 的竞速登记只论证了「瞬态不制造假阳性」，其论证依赖「poll 是 worker 唯一长阻塞点」——该前提在 worker 序列冻结后成立（marker 发射均为即返操作），若未来 worker 序列修改，本论证须重审。
8. **r3 三趟新引入的裁量**（逐项登记，如实不粉饰）：(a) **死因全序反转**（D1：`probe-fault` > `platform-termination` > `unattributed`）——理由是旧序（platform 先判）会让成功终态被更早判定的类截胡，但反转的代价是把「platform-termination 条件不全成立」的情形后置到 `unattributed`（fail），凡证据链不齐（如 faultlogger 条目缺失或 PidOfVpn 无 positive 基线）一律 fail 收口——我把判定失败的方向压向保守，这是裁量不是必然（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(b) **APPFREEZE 短时步集收窄**（D2：仅 P1 与 D2 锁定序列 2.1-2.7）——集合边界是我按「即返 syscall」划的，P2 内 60 s create 未列入短时步（其 APPFREEZE 走 platform-termination），若平台对 create 期间的 APPFREEZE 实际含探针缺陷，将被记为平台事实而非 fail，此风险我未消除、仅由 A8 与 (i) 的 D7 正面约束部分对冲（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(c) **A8 断言引入**（D2）——它只静态核对「循环有可达退出路径」，不能证明退出路径在真实调度下及时可达（如时间盒常数过小导致正常路径也被熔断），其保证是形式性的；
(d) **`destroy-never-called` 的位次**（D4：优先级 0，高于一切依赖 destroy 时刻的类）——理由是锚点缺失时后续判定无定义，但置于最高位意味着 barrier-never-observed 等 worker 异常情形一律不归因、不解锁路径①，waiter 事实倾向 unobservable 化，可判定性让位于归因安全性，这是我的取舍（**防误引注，r9：类名已拆为 `destroy-skip-proven` / `destroy-call-unobserved`，位次理由由后者原样继承；且 barrier-never-observed 现补发 SKIP、按五态记 `not-called`，本条「一律不归因」对该路径的描述已过时，见 r9 第四/五步**）；
(e) **canonical ledger 排序规则**（E2 (c)：按 `created_at_mono_ms` 升序、not-created 按登记时刻参与、同刻按角色集出现顺序定序）——排序本身不影响 digest 的抗碰撞性，选它只为确定性；「角色集出现顺序」这一同刻 tie-break 是任意的，无深层理由；
(f) **chunk `stream` 枚举划分**（E3：恰 {`dlerror`,`rejtext`,`u3hex`,`foreign`} 四值）——恰好覆盖现有四类 detail 记录，未预留扩展位；若 freeze 后新增任何 detail 类别须扩枚举，属判据修改；
(g) **U3 优先级顺序与 `off=both` 决定性 offset**（E4）——**防误引注（r5 U11）：E4 的「优先级 `tun_pi-like` > `other-prefix` > `no-prefix` > `ambiguous` > `unparsable`」排序已被 r4 第二趟 S5 的互斥分区表取代（`ambiguous` 不再被截胡），本项对旧排序的动机自陈仅存历史价值；`off=both` 决定性 offset = offset-4 的裁量仍有效**——旧排序保证的是「判定唯一」，不保证「最符合直觉的类优先」；决定性 offset 选 offset-4 的理由（IPv4 真实起点、结构性差异显式化）成立与否依赖 PI 头确实存在的平台前提，`no-prefix` 实测成立时 off0 对照字段仍可核查，此为后手；
(h) **D7 elapsed 三区间边界归属**（E5：`25000` 恰值归 `[20000,25000]` 合法区间）——边界值归属是任意约定，选「并入合法侧」偏向不 fail；`20000` 下界归合法侧同理；
(i) **E8 选「按终态停止」而非固定 825 s**——收益是停止条件机器可判定、尾窗不再依赖未实测的启动耗时假设；代价是停止判定引入两条新依赖（终态 marker 出现检测——现字面为 `N1BDISC_POST`，r5 U7 更正：原此处 `N1BDISC_RESULT` 为 R1 拆分前旧字面残留、finally 步骤 1 到达检测），若 runner 自身挂起在两检测之外，825 s 兜底仍在但无更细的中间熔断；
(j) **operator-ready 确认步的形态**（E6：runner 控制台显式确认 + 墙钟/单调双时刻 + 动作字面 `operator-ready-confirmed`）——确认的「真实性」无技术手段核验（操作员按回车即成），它承载的是审计留痕而非操作员能力验证；我把「readiness 先于 ID 消费」做成流程约束，本质依赖操作员纪律；
(k) **本轮仍未修干净的地方（如实登记）**：① G9 集中列举的 gate 10 用例类别与各节分散要求由我逐条誊录，两处清单的一致性靠人工维护，无机器同源保证，审查席应按「以各节正文为准、gate 10 行为索引」复核；
② G5 的换算只覆盖 `dw_drain_end` 四值与 C6 五类的对应，`dw_drain_timeout` 与 `end=box-expiry` 的冗余登记仍同时存在（两字段信息重叠，未合并——改字段集属语义变更，收尾趟不做）；③ 自陈 7(b)（D7 负载因 50 ms sleep 而被修改）与 7(d)（hilog 行长无实测）仍是未实测依赖，r3 未处理；④ 全文多处「r3 冻结」「r3 第一趟」类自引没有统一索引表，跨节核对仍须通读。
9. **r4 第一趟（R1-R7）新引入的裁量**（逐项登记，如实不粉饰）：(a) 全序新增位点编号 `P8T` 而非重排 P9-P12——动机是最小化全序改动波及面；代价是编号表出现字母后缀，与其他 P 编号不规则；(b) PRE 的 P1-P8 事实以「既有 marker/chunk 流 + PRE 封口断言」承载而非在 PRE 行内重复全字段——动机是避免字段集双写漂移；代价是 PRE 单行不含事实本身，完整性依赖 capture 全流存在性判定；
(c) 墙钟代理容差取 2000 ms——无实测依据（沿自陈 4 的时间盒数值口径）（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(d) 死亡位点 ∈ {P10} 一律判 `probe-fault` 不区分 join 是否实际阻塞——按 R6 授命从宽收口，理论上覆盖「worker 正常结束后死于 P10」的误判情形（fail 方向，不洗白）（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(e) `pre-only` 由 runner 在 evidence 记录登记而不代发探针 marker——改动了旧 post-mortem 机制（**防误引注：「post-mortem」「RESULT marker」均为被取代的旧机制历史描述，现行 protocol 取值域 = {`complete`, `pre-only`}，现行终态 marker = `N1BDISC_PRE`/`N1BDISC_POST`**），封签形态随之变化；(f) `pre-only` 收口附加「host finally 须采得死亡证据」前提（无死亡证据的存活未完成仍 `fail`）——R1 授命字面未含此前提，为防「探针活着磨蹭到观测窗到点」被洗成合法终态而补加。
10. **r4 第二/三趟（S1-S8、T1-T8）与 r5（U1-U12）新引入的裁量**（逐项登记，如实不粉饰，补 r4 两趟漏登——决议 §九「全部已知自利解释须自陈」）：
(a) **`closed_by` 的 `open-at-exit`/`process-exit` 推断**（S3，r6 W3 修正：pre-only 形态由 `host-forcestop` 改回 `process-exit`）：仍 open 实例的收口 `closed_by` 由 runner 按收口形态**推断**而非探针自证——`complete` 时断言「进程自然退出时仍未关闭」、pre-only 时断言「ForceStop 是关闭责任方」（**防误引注，r7 X8：pre-only 形态现记 `process-exit`（r6 W3）——本句「ForceStop 是关闭责任方」为 W3 前旧口径**）；若平台在退出前有未观测的隐式关闭行为，该登记与事实不符且无从察觉，我选择可判定性优先；
(b) **候选集不猜近义词根**（S1）：`JSCRASH`/`JSERROR` 等异词根不并入候选集而走行 5 兜底出口——不猜是对的，但后果是若真机字面恰为异词根，行 1/2/4 全程不命中，crash 类死亡全部降级为不 fail 的观察出口，探针缺陷的 fail 侦测能力在该情形下为零，这是我接受的代价（**防误引注，r7 X8：「全部降级为不 fail」已被 r7 X1 取代——无平台签名的异词根构造现落行 5 `fault-type-unrecognized-no-platform-signature` → fail，有平台签名的落行 4 (iii) 不 fail**）（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
(c) **`d2_late_fd` 恒按最终重建形态登记**（r5 U5 沿 S3）：迟到 fd 的 PRE 快照值（`open-at-pre`）与最终值刻意不同且恒不用前者——快照一致性让位于「最终形态唯一」，PRE digest 对该条目的可复核性弱于其他条目（**防误引注，r7 X8：「恒不用前者/恒按最终重建」已被 r6 W2 取代——P5T 快照对含 `d2_late_fd` 的一切未关闭 fd 一律记 `open-at-pre`，最终重建才按收口形态**）；
(d) **chunk 重复片仅比 payload**（S6）：同 index 重复片的「逐字节一致」只施加于 payload 解码后字节，`count`/`sha256` 字面不一致 r5 U12 才补为 fail——S6 当时漏写这两项属疏漏，r5 补上，非事后加固；
(e) **P8T 编号、r5 U1 改 P5T 编号**（R1/U1）：位点编号带字母后缀而非重排 P9-P12，两次都是为最小化全序改动波及面；代价是编号表不规则、旧文档引用 `P8T` 全部失效，跨文档追溯须依赖本登记；
(f) **`inst` 双实例设计**（S3）：`dw_inwait_proc_fd` 以 `(role, inst)` 二元组承载两实例——比拆成两个 role 少冻结两个字面，但 ledger/marker 解析从此依赖二元组配对状态机，解析器复杂度上升是换取字面集最小；
(g) **PRE 前移的承载收窄**（r5 U1）：PRE 只封口 P1-P5 事实，P6 之后死亡时 D7/D8b/D-W 的已得增量依赖独立阶段 marker 而无终态封口断言——若后续阶段 marker 流本身有缺口，pre-only 收口对「已得事实是否完整」的保证弱于 r4 版 P8T 全量封口；我判断「U7 可发现性」优先于「后段封口完整性」，因后者由 POST/pre-only 全流重建兜底；
(h) **fail 触发器收窄与 digest 切点冻结**（r5 U3/U5）：二者都放宽了 fail 面（complete 活进程不再 fail、pre-only 不再因切点漂移必 fail）——方向上提高 pass 可达性，属于对本设计有利的修订，特此自陈供对抗核验：放宽的每一处都对应 r4 三席指认的结构性不可 pass 缺陷，不是为 pass 而 pass。

11. **r6 两趟新引入的裁量**（逐项登记，如实不粉饰）：
 (a) **V2 分岔判据选择 `N1BDISC_DW_DESTROY_T` 是否发出**（主会话裁定）：用 marker 存在性作分岔判据是机器可判的，但 destroy 已调用与「进程活过 destroy」不严格等价——destroy 调用后进程仍可能活过 D6b（此时 observed-false 法理成立）或死于 D6b 中途（死亡前已得 result 保留、其后未执行项 observed-false，法理仍成立）；我未找到比 DW_DESTROY_T 更精确的可判锚点，接受该近似（**防误引注，r7 X4：该锚点已被取代——r7 新增 post-invocation marker `DW_DESTROY_C` 作为 V2/行 3/行 4(ii) 的分岔锚点，见自陈 12(b)**）；
 (b) **V4 `site_uncertainty` 阈值 = 该阶段时间盒 × 1.5**（主会话裁定）：无实测依据（沿自陈 4 的时间盒拍板传统）；阈值过小会把正常慢调度误标、过大则尾 marker 丢失漏标——1.5 倍对 D7（20 s × 1.5 = 30 s）的取值纯为「调度噪声显著小于阶段时长」的先验假设（**防误引注，r8 Y3：该 ×1.5 阈值已废弃——r7 X5 落笔沿用的「25 s × 1.5 = 37.5 s」与行 2 判定窗错位，现 `T_uncertainty(P6) = 25000 ms` 挂行 2 之下，见证据规则 4 与自陈 13(b)**）（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
 (c) **V4 不确定性只约束 `u7`、不改死因分类**：site_uncertainty 存在时死因分类仍按可见位点判定（行 2 的 APPFREEZE+D7 超窗判定不受影响）——若尾 marker 丢失恰与真实超窗叠加，行 2 可能误判；我接受该残余（行 2 触发面已由 elapsed_proxy 约束），但审查席若认为应同时约束行 2，属合理收紧（**防误引注，r7 X5：审查席意见已被采纳——site_uncertainty 置位时行 2 现一律不命中，见死因分类节墙钟代理段与自陈 12(c)**）（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
 (d) **W3 恢复 `process-exit` 的字面选择**（主会话裁定）：用 `process-exit` 而非新造字面是为与 r5 U12 删除前的历史字面一致、减少解析器分支；该字面语义是「内核在进程退出时回收」，与 ForceStop 的主动 kill 有明确区分；
 (e) **V6 行 5 拆分判据 = 有无 SIGKILL/SIGTERM 条目**（主会话裁定）：该判据把「平台异词根」的认定系于信号侧证据——若平台用异词根 Fault_Type 且**不写** SIGKILL/SIGTERM 条目（纯静默终止），该情形落行 5b → fail，平台合法行为被烧；我接受该残余（S1 的 freeze 收敛前置会在拿到真实样本后缩小触发面），但这是 5a/5b 拆分的已知代价，特此自陈（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
 (f) **W4 行 3 加 `DW_DESTROY_T` 条件的方向**：该修订把「destroy 后死于 P10」从 probe-fault-fail 改为 platform-termination-pass——放宽 fail 面、提高 pass 可达性，属对本设计有利的修订；依据是 grok M1 指认的预期终态截胡（destroy 已 resolve、join 完成、D6b 发出 marker 后的 terminal 是 E3 先例的正常路径），不是为 pass 而 pass。（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）

12. **r7 新引入的裁量**（逐项登记，如实不粉饰）：
 (a) **X1 删除行 5a 的理由**：r6 行 5a 的构造（`Fault_Type` 归一化不落候选集 + 存在 SIGKILL/SIGTERM 条目 + 行 1-4 不命中）与行 4 (iii)（unknown `Fault_Type` 非 crash 签名 + `Signal` ∈ {SIGKILL, SIGTERM} → `platform-termination`，(iv) 同真）完全重合——5a 在行 4 之后按优先级永不可达，保留只会维持「5a/5b 同名 `fault-type-unrecognized`」的名实不符。
这是删除而非改判：平台异词根 + 有平台签名的发现产出**仍不 fail**，只是承载点从 5a 移回行 4 (iii) 本位；「平台用异词根且无任何信号条目」的纯静默终止落行 5 → fail，该残余沿自陈 11(e) 不变，r7 未新增对冲（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
 (b) **X4 的 `DW_DESTROY_C` 锚点选择**：V2/行 3/行 4(ii) 的分岔问题是「destroy 调用是否已发起」，`DW_DESTROY_T` 只证「即将调用」（紧贴调用之前），marker 在而调用未发生（如 marker 发出后进程立即被杀）会误判（**防误引注，r9：原死因表行 3 / 行 4(ii) 已随归因停机删除，本条的历史论证对象不复存在；现行口径为 `destroy_call_state` 五态 + F1 闭集，`_C` 现锚定五态的 `call-boundary` 侧与路径①的严格 `at > C`**）；
    sol 指出的 `D6S1_B` 或 destroy resolve 均在调用返回语义之后（resolve 前死亡时不可得），粒度不对——故新造 post-invocation 字面 `DW_DESTROY_C`，发射点钉死为「紧贴 `destroy()` 调用之后、resolve 等待之前」。
    「已调用未返回」（`DW_DESTROY_C` 在、无 resolve 证据）单列第三支记 `unobservable(destroy-unresolved)`（**防误引注，r10：裸拼法自 r9 字面拼法冻结（`cause=` 句式逐字比对）起非合法字面，现行字面为 `unobservable(cause=destroy-unresolved)`、在活规则中（verdict 节 V2 第四支与 selftest ⑨ 对应用例）；本处为 r7 X4 历史登记，照登不改字面**）而不并入 observed-false，是宁缺勿误的同一取舍；
`DW_DESTROY_T` 保留作时序锚定（**防误引注，r8 Y5：路径①条件 3 的 `destroy_call_mono_ms` 现规定取 `_C` 的 `mono_ms`，不再以 `_T` 为 destroy 时刻锚**，见该条件定义式与自陈 13(c)；**防误引注：`destroy-never-called` 判定锚已改用 `DW_DESTROY_C`，见该行**），一处 marker 两字面是成本，换取三态判据的每一支都有独立机器锚；
 (c) **X5 的比较对象修正**：r6 版「最后 marker 与其前一 marker 的间隔 > 阶段时间盒 × 1.5」在因果上错位——连续尾丢失发生在最后 marker **之后**，前一间隔再短也不能排除其后全丢（grok 构造：PRE 与 D7_BEGIN 间隔很短、D7_END/P7/P8 全丢、死于 P8 → 旧判据漏标 → 错误 u7 + pass）。
    改为「死亡证据墙钟 − 最后可见 marker 墙钟 > T_uncertainty」（**r8 Y2 防误引注：r7 登记落笔把减法方向写反了（marker 侧作被减数）——死亡晚于 marker 时恒为负、永不置位，属执行错误，现全文冻结唯一式为本式**），直接度量「最后可见证据与死亡之间的静默跨度」；~~P6 阶段时间盒钉死 25 s（20 s 冻结时长 + 5 s 宽限）消除「阶段时间盒」在该位点的歧义~~（**r8 Y3 防误引注：×1.5 阈值（37.5 s）已废弃**，r7 阈值取值与行 2 判定窗错位——现取 `T_uncertainty(P6) = 25000 ms` 挂行 2 之下，见证据规则 4）；
    同时把 site_uncertainty 的效力从「只约束 u7」扩为「u7 与死因行 2 均不得作确定判定」（行 2 不命中、该输入落行 4/5/6 按证据定类）——这收窄了行 2 的触发面，方向偏保守（多 fail），与 11(c) 当初「只约束 u7」的放宽相反，属审查席意见采纳（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）；
 (d) **V4 行 2 约束的残余**：site_uncertainty 是观察项，置位判据依赖死亡证据墙钟（faultlogger 时间戳或 capture 静默判定墙钟），死亡证据本身缺失或不可求值时置位判据同样不可求值——此时行 2 按既有「代理不可求值 → 不命中」规则处理，与 X5 约束同向，未引入新缝隙但登记依赖同源。（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）

13. **r8 新引入的裁量**（逐项登记，如实不粉饰）：
 (a) **Y1 四步序列的「取得 Promise 不等待」实现契约**：第二步只要求 destroy 调用已发起并取得 Promise 对象、不要求任何完成信号——这把「调用已发起」与「调用已生效」切开：`_C` 锚定前者，resolve 等待盒（10 s）承载后者，`destroy_unresolved` 是二者分离时的观察出口。实现侧风险：若目标 API 的 destroy 返回值不是 Promise 形态，「取得 Promise」应读作「取得该调用返回的完成凭据」；`_C` 的发射时机在调用语句与等待语句之间，属同步区，理论上有发射后进程立即被杀导致调用实际未进入平台层的小窗——接受该残余（`destroy-unresolved` 第三支兜底，宁缺勿误）；
 (b) **Y3 阈值挂行 2 之下的理由与残余**：阈值必须 < 行 2 判定窗 27000 ms，否则真 P8 死亡的静默跨度会先被行 2 误判为 D7 超窗 probe-fail、保护来不及置位；取 `T_uncertainty(P6) = 25000 ms`（= 行 2 elapsed_proxy 阈值 20000 + 5000）使保护窗自「D7 合法时长结束时」即开启，且与既有冻结值同源、不新造数。残余：25000–27000 ms 的静默跨度被保护后行 2 让位、落行 4/5/6 按证据定类——若该跨度的死亡证据不足，结局是 unobservable 而非 probe-fail，fail 侦测面在此窄带内收窄，属宁缺勿误的同一取舍；30–37.5 s 段的旧错位（既不保护又被行 2 窗吞掉）已消除；
    **防误引注，r9 第六步：本条为 r8 当时的历史自陈，三处表述不得按字面引用**——(1)「行 2 elapsed_proxy 阈值」这一称呼当年即为误称：行 2 真实阈值为 `20000 + 5000 + 2000（代理容差）= 27000 ms`，本条所引 `20000 + 5000` 只是其中前两项；(2)「保护窗自「D7 合法时长结束时」即开启」是闭区间读法，与置位谓词的严格大于不符（25000 恰值不置位）；(3) 行 2 与 `elapsed_proxy` 本体均已随 r9 死因表删除，现行口径为 `marker_tail_state` / `T_tail = 25000 ms`（严格大于、纯观察登记），见「死亡事实记录（证据向量）」节。r7 X5 登记行（修订登记头内）对同一误称的历史陈述按「登记照登不改写」惯例保留原样。
 (c) **`destroy_call_mono_ms` 在路径①条件 3 的取值规定**：`_T` 与 `_C` 两枚 mono_ms 并存，规定取 **`_C` 的 `mono_ms`**（post-invocation 锚是「调用已发起」的精确时刻；`_T` 早于调用语句，作比较基准会把 destroy 前数毫秒内的 poll 返回误判为「晚于 destroy」）——该规定使条件 3 的锚点与 V2 分岔、行 3、行 4(ii) 全部统一到 `_C`，`_T` 退为纯时序锚；代价是 `_C` 缺失时条件 3 不可求值、路径①必不解锁，该输入由判定表类 0 收口，不产生 criteria-gap。
    （**防误引注，r9：本条按 r8 时代口径陈述。「行 3、行 4(ii)」已随死因表删除；「类 0」现拆为 `destroy-skip-proven`/`destroy-call-unobserved` 两类，收口的是后者；
    条件 3 现冻结为严格 `at_mono_ms > C_mono_ms`，`T ≤ at ≤ C` 闭区间整体归 `invocation-window-ambiguous`、不解锁路径①，见 r9 第三/四步**）。
 (d) **Y4 用例组的本轮自查改正（如实登记）**：Y4 首稿把 (a) 的 30 s 构造注为「30 s < 27000 ms 本就不命中行 2」——算术方向写反（30000 > 27000），据此推出的「行 2 本就不命中」与真值相反，且所谓「另设 28000 ms 变体」与 30000 同在 27000 之上、不构成对照；同时 (b)/(c) 两例（25000、24999）同落不置位一侧，缺本文档既有句法（⑪ 的 27000 不命中 / 27001 命中）要求的跨界边界对。
    派出前自查改正：(a) 归位为「置位优先于行 2」的优先级用例，边界对改 25000/25001，24999 降为界下例，共五例。
    **登记理由**：本条与 r6/r7 已登记的落笔错误同型——批量补写规则时未逐一核对所引数量的大小关系，属主会话反复出现的失误模式，供审查席校准我方产出的可信度、勿默认算术与阈值方向已被验证。（**防误引注，r9**：本条按原死因分类表口径陈述；该表已随 r9 删除，现行口径见「死亡事实记录（证据向量）」节与 F1–F9 闭集）

14. **r9 新引入的裁量与删除登记（证据向量改造，逐项登记，如实不粉饰）**：本轮改造依据 `docs/n1b-disc-r9-death-facts-spec.md`（T0 裁决 3/3 一致的落地说明：停止用死因归因驱动 `verdict`，改证据向量三态记录 + 窄正向崩溃签名 fail），规范已决事项不在此重裁；本条只登记本轮新引入的裁量与删除损失。
 (a) **删除的 fail 映射逐条登记（席 C 义务：删了什么、各失去什么真实探针缺陷捕获、为何该损失是决议 §4.2 `:116` 所要求的）**：

| 删除的 fail 映射 | 原捕获意图 | 失去的真实探针缺陷 | 为何必须删 |
| --- | --- | --- | --- |
| 原行 2 `probe-fault`（D7 超窗行） | 探针在 D7 挂死超时间盒 | **长盒内挂死**且只有 `APPFREEZE` 或无任何条目 | 「D7 真超窗」与「D7 跑完但尾 marker 全丢」观测向量完全相同（三席一致）；且非短时步上的 `APPFREEZE` 本就是本 campaign 要发现的平台行为 |
| 原行 3 join 封锁行（**r9 补登**：规范稿 §6 表未列本行，按「逐条登记」义务补齐） | 探针侧预注册的无界等待（P10 join）不得洗成平台事实 | P10 位点死亡、无 `_C` 锚或 `join-blocked-observed`、且无正向崩溃签名的构造 | F1-F9 闭集无此条；join 阻塞事实仍由 `dw_join_result` 照常登记，其 fail-open 残余并入本条 (c) 的同一诚实陈述 |
| 原行 5 `fault-type-unrecognized-no-platform-signature` | 探针崩溃但词根不在候选集 | **JS 崩溃写成候选集外词根**（如 `JSCRASH`）且无致命信号 | 本 tuple 从未实测过真实 faultlogger 字面（S1 已登记），未知词根同样可能是平台异词根终止；原生 `SIGSEGV` 仍由 F1 第 2 支捕获 |
| 原行 6 `unattributed` | 归不出类即保守 fail | **无 faultlogger、无致命信号的静默崩溃** | 举证责任倒置：「无法排除探针缺陷」不等于「有探针缺陷」；归不出类在观测上等价于「可能是平台行为」，映射 fail 直接违反决议 §4.2 `:116` |
| 原行 7 显式 else 兜底 fail | 穷尽性兜底 | 同上 | 同上 |
| 原行 4(ii) 的收紧提案（**未采纳，登记为「明确不修」**） | 堵住 `_C` 后的探针崩溃 | `_C` 之后无 crash 签名的静默崩溃（席 B #6 构造） | 收紧会把 E3 预期的 destroy terminal 成功终态再次烧成 fail（席 A 明确禁止） |

 (b) **F1 属 r9 新增规则（登记为新引入的裁量）**：`probe_crash_signature_observed` 三支闭集与「除 F1-F9 外一切结局均不 fail」的闭集宣告均为本轮新造；其材料来源是原行 1 证据签名与短时步集约束（r3 第一趟 D2 / r4 第一趟 R5 / r5 U2）的重组，但作为 verdict 触发面是新规则，不属于「沿既有规则不变」。**F2-F9 经逐条核实均非新增**（逐条存在性与措辞差异见 (e)）。
 (c) **残余的诚实陈述（席 B 原话口径）**：新方案接受一种残余 fail-open——**没有正向崩溃签名的真实探针故障，可能以死亡事实不可观测收口并使 pre-only `pass`**。
 以席 B #6 构造为例，新方案下记录为 `process_death_observed=observed-true`、`last_visible_site=D6S1_B`、`destroy_call_state=call-returned`、`probe_crash_signature_observed=unobservable`。**不再伪称 `platform-termination`，因果语义上不再洗白；但 `verdict` 层面仍存在 false-negative 残余。**
 （**防误引注，r10**：本条为 r9 冻结审查时的自陈示例登记，按「登记照登不改写」惯例保留原样；其示例值 `probe_crash_signature_observed=unobservable` 自 r10 求值真值表冻结起**不得再按字面引用**——
 该构造（无 faultlogger 条目、`FaultRecv` 已执行且取回为空、`last_visible_site` 可求值）按真值表第 3 行是**确定观测**：`fault_type_observed`/`signal_observed` 记 `observed-false`、`probe_crash_signature_observed=observed-false`，与 selftest ⑩ 同键用例一致，原 ⑩ 写 `observed-false`、本条写 `unobservable` 的矛盾就此消解。本条的残余陈述本身不受影响——该构造仍无正向崩溃签名、F1 仍不命中、pre-only 仍可 pass，false-negative 残余照旧存在。）
 (d) **接受该残余的理由（禁止引用 DryRun）**：**禁止事项**——不得以「Live 之前还有一次 DryRun」为该残余背书：门 11 DryRun 是 `is_evidence=false` + **HDC0**（host-only 假 HDC），只能验证 runner / parser / 状态机 / 合成 trace / freeze 哈希，**不能**执行设备侧 native/NAPI、watchdog、destroy 竞速、线程调度或 faultlogger 缺失。
 席 A、席 B 独立否定该前提；主会话在裁决提问中曾误用之，已登记为程序失误 #19。静态审查与故障注入 selftest **只能降风险、不能证明消除**（席 B）。正确理由三条：
  1. `verdict: pass` 的效力已被硬约束封死——发现型 campaign 的 `pass` 不得在任何后续文档中被引用为平台行为结论或任何门的功能结论（决议 §4.2、`docs/evidence-schema.md:90`），一次带残余的 `pass` 不会把错误结论传播下去；
  2. N1b 正式门仍须在同一元组上复测，DISC 事实只作为 N1b r2 判据的**预注册设计输入**（决议 §4.4）；
  3. 发现型 campaign 的损失函数是「烧掉不可复用 ID」，不是「漏掉一次可能的探针挂死」（席 A）——模糊死亡一律 fail 会把合法平台终止烧成 fail、直接违反 §4.2；模糊死亡不 fail 会漏掉无正向签名的探针故障；在 discovery 门上应选后者，并把残余逐字登记（即本条 (c)）。
 (e) **F2-F9 逐条核实登记（r9 执行规范稿 §3.1「整合前必须核实」义务的记录；锚为现行字面）**：
  - **F2 存在、措辞一致**：verdict 节「终态 marker 异常——**`N1BDISC_PRE` 缺失**（探针未能在 P5T 位点（P6 D7 开始之前）落盘任何终态 marker……同判 `fail`）」+ PRE/POST 求值规则 (3)(4)「**`PRE` 缺失**（无论 POST 是否存在）→ 完整性失败……→ `fail`」；
  - **F3 存在、措辞不同**：verdict 节「**顺序破坏——`protocol=complete` 时校验 P1-P12 全序（含 P5T）（marker 时序单调；……）**」条；r10 更正（A M-05）：规范稿括注例「含 `_C` 在而 `_T` 缺」**现文档已有**——五态表第 5 行（`_C` 在 `_T` 缺 → fail 走 F3）与 selftest ⑨ 对应用例；核实时点早于五态落地，原「在现文档不存在、未引入」的登记照登不擦，此处更正指向；
  - **F4 存在、措辞不同**：「或任一冻结字段缺项」+「增量落盘缺项（某 D 项完成 marker 在而其 `(stream,item)` chunk 组缺失/组内重组失败/sha256 不符……）」+ criteria-gap 判别方法 (2)-(4)；规范稿「字段取值落在冻结域外」与判别方法 (1)（域外有效平台值 → `unobservable(cause=value-outside-frozen-domain)` 不 fail）**冲突**，以现文档为准、未引入；（r10 更正：判别方法论域已切分——(2)(3) 现归 F8、(4) 归 F4，见 fail 闭集 F4/F8 行的 r10 切分注）
  - **F5 存在、措辞不同**：「封签失败」+「fd ledger 缺失或与 marker 流矛盾」+「同一切点内探针自发值与 runner 重建值不一致 → `fail`（沿 E2）」；规范稿「freeze SHA-256 复算不符」在现文档归 `invalid` 轴（freeze 资产被冒用/篡改），不并入 fail 闭集；
  - **F6 存在、措辞一致**：「`d1_cmdline` 非 `:vpn` 进程」；
  - **F7 存在、归类不同**：「HDC 命令流出现白名单外命令」归 `invalid` 轴 + 停止条件 S2「立即停止并登记违规」；现文档不将其列为 fail 触发面，F 索引仅作归类登记；
  - **F8 存在、措辞不同**：即 criteria-gap 判别方法 (2) 解析域缺口 + (3) 字段无法求值（「判定输入存在但派生规则/真值表未覆盖该输入组合」）；规范稿例「未知 revents 位」与 S2/④「未知位归 `other-revents` 不 fail」**冲突**、例「负单调耗时」现文档**不存在**，均未引入（**防误引注，r9 第五步**：负单调耗时的绝对域约束已由本趟 BL-4 以「单调钟派生字段绝对域门」引入并挂 F8——本句「现文档不存在」仅描述 14(e) 核实时点，非现行状态）；
  - **F9 存在、措辞不同（规范稿为概括句）**：「runner 观测窗」到点收口规则「**无死亡证据**（进程仍存活或状态不可判）→ `fail`（探针未完成预注册采集；525 s > 467 s 协议上界，存活未完成不是平台事实）」+ PRE/POST 求值规则 (2) 适用前提句，以现文档为准。
 (f) **其余 r9 裁量**：(i) `unattributed` **直接删除、不做改名保留**——席 B 建议改名 `unobservable(cause=death-cause-indeterminate)`，但七分量向量中不存在「死亡原因」字段，该 cause 无处落脚，保留反而诱导实现者再造合成字段；此处与席 B 建议相左，请审查席挑战；(ii) `marker_tail_state` 阈值沿用 r8 Y3 已冻结的 25000 ms 数值、取值理由改写为「单个最长合法阶段的合法时长上界」并解除与任何 fail 判定的耦合（原「置位则行 2 不命中」及 27000 比较论证随表删除）；
 (iii) F1 第 3 支「marker 序列无矛盾」守卫为席 B 补充，本轮照采；(iv) `destroy_call_state` 本轮仅占位（分量名与「互不推导」纪律）——**r9 第四步已在正文落地五态表与 worker 返回时序三分带（见「死亡事实记录（证据向量）」节），本括注原「五态定义属后续工作包、本文件暂无该字段取值域」的表述随之失效**；(v) 原行 4 (ii) 预期成功终态的法理（E3 先例）保留为窄 fail 闭集的正面理由；(vi) 「字段域缺口」一词仅在冻结解释句（席 A 逐字）中出现，不构成对判别方法 (1) 的改写。
