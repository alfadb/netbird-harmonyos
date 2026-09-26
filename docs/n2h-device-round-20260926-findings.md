# 2026-09-26 真机四轮（0029–0032）结果、独立盲审口径与后续待办

## 0. 说明与边界

- 本文记录 **2026-09-26 四轮真机诊断（0029–0032）的目的与结果**、**同日独立盲审（openai/gpt-5.6-sol，干净上下文、只读）的结论口径**，以及由此得出的**产品含义与后续待办**。
- **四轮均为诊断观察**：各轮授权件与其证据记录自始标明 `is_evidence=false`、`not_a_gate_pass=true`，**不构成任何门结论**，不得写成逐 socket protect 的 pass／waived／N/A，也不得当作 N2-H／N6 的任何门判定。
- **本文不修订任何判据、不产生任何门结论**：v4 预登记正文（只读字节锚，本文未触碰）`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md`，`sha256 a7510430f3ab402c3cd11f1ef39889b6092b8cf3ec8a4b14d2e90c25bc0c0dc7`（本文撰写时 `sha256sum` 实跑复核，与既有记录逐字一致）；N2-H 正式状态不变（`n2h-inconclusive`）。
- **本文不执行任何设备操作**：只读引用仓B（`/home/worker/harmonyos-signing/netbird-n1bdisc`）`diagnostics/` 下四轮证据目录；下文所有 `diagnostics/…` 均指仓B 相对路径，行号按各轮 `STAGE-RECORD.md`／原始文件；本仓内的本文是本次唯一新增文件。
- 仓B 侧同一批口径的合规登记见 `/home/worker/harmonyos-signing/netbird-n1bdisc/records/n2h-preregistration-v4-addendum-20260923.json` 的 `amendment_events[2]`（外加 `B8` 条的 `superseded_by_amendment` 标注；只追加，不改原文）。本文只引用，不重复其全文。
- **环境**（四轮同设备会话，出处：`diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260925-0030/cmd-003-param-model.txt`、`cmd-004-param-swver.txt`）：PLA-AL10，系统 `7.0.0.107(SP8C00E105R7P3)`，SDK 26.0.0.821／API 26，调试类型签名，应用 `cn.alfadb.netbird`，VPN 接口名 `vpn-tun`。

## 1. 今天四轮：目的与结果

| 轮次 | 目的 | 结果 | 证据目录（仓B `diagnostics/`） |
| --- | --- | --- | --- |
| **0029** | v4 §4 前置②(ii) 同源标定、§3 路径① DNS 回执的 §6.2-2 完成判据、`hdc shell -b` 沙箱写入可用性探针 | **INCONCLUSIVE（采集时序）**：采集窗口未开、无 collector 状态 token | `AUTH-DIAG-DEVICE-VALIDATION-20260921-0029/` |
| **0030** | 不改代码的验证：protect 真机复现、4.2 两种投递路径、hilog 抓取方法只读核查、型号版本复读 | **protect 全部成功**；平台两类校验失败行与调用相互独立 | `AUTH-DIAG-DEVICE-VALIDATION-20260925-0030/` |
| **0031** | 带默认关闭开关（S1–S4）的实验包，R0–R4 固定矩阵验证第二轮 3.1 各条 | **R0–R4 全部按序完成**；结论按盲审第 5 条收窄 | `AUTH-DIAG-DEVICE-VALIDATION-20260925-0031/` |
| **0032** | **0029 补跑**（驱动 v12）：同源标定、DNS 回执、沙箱写入三项 | 窗口开闭正常、DNS 回执 hit×4 miss×0；**C7 同源标定未完成**；沙箱写入被拒 | `AUTH-DIAG-DEVICE-VALIDATION-20260926-0032/` |

### 1.1 0029：INCONCLUSIVE，原因在采集时序

- `status=INCONCLUSIVE`、`stop_reason="no collector state token"`；驱动 06:55:44 启动、06:56:05 退出（rc=0）；采集窗口未开，三项判据均无从成立。
- 出处：`AUTH-DIAG-DEVICE-VALIDATION-20260921-0029/STAGE-RECORD.md:1`（标题「三项判定轮（采集窗口未开，INCONCLUSIVE）」）、`:65`（`status=INCONCLUSIVE`、`stop_reason`、`clicks=1`、`leftovers=[]`、`device_cmd_failed=false`）、`:110`（「本轮**不构成任何门结论**」）。授权件 `consumed=true`、`consumption_semantics.retry_allowed=false`；该轮按原范围补跑于 0032。

### 1.2 0030：protect 真机复现——40 次全部 resolved

- `VpnConnection.protect(fd)`：**`SETTLED` 40 条、`REJECTED` 0 条，`elapsedMs` 3–16 ms**；`create()` 正常 resolved。
- netmanager 两类行（`Caller is not allowed, need sys permissive` 43 行、`permission check failed, permission:ohos.permission.MANAGE_VPN` 42 行）在**每个 pre-opened fd 前后均出现**，而调用本身全部成功 ⇒ **两类行与 protect 调用相互独立**。`callerToken:537437623` 与本应用 `applicationInfo.accessTokenId` 一致（`cmd-005-bm-dump.txt:15`）。
- 该轮 36 条 `VPN_PROTECT_TIMEOUT` 是**盒子计时器未取消**的日志假象：`protectSocketWithBox` 用 `Promise.race([settled, box])`，`box` 由 `setTimeout(…, 5000)` 构造且全文无 `clearTimeout`（`fd5f71c` 的 `NetBirdVpnExtensionAbility.ets:2506-2557`；`grep -c clearTimeout` = 0）。该缺陷已在项目仓提交 `b3e7b81`（`fix(vpn): clear bounded-wait box timers once the platform call settles`，3 files changed）修复。
- 出处：`AUTH-DIAG-DEVICE-VALIDATION-20260925-0030/STAGE-RECORD.md:154`（源码解释）、`:227`（`/proc/net/dev` 实测出现 `vpn-tun`）、`:235`（结论上限仅限「本平台 `VpnConnection.protect()` 的 Promise 结局观测」与「沙箱投递／hilog 抓取方法的可用性核查」）；原始行 `cmd-020-hilog-x-full-1.txt`（全量 hilog，1462 行／199605 字节）。
- 同一批原始行亦被独立盲审引用为第 1 条撤回的依据（见 §2 第 1 条）。

### 1.3 0031：R0–R4 矩阵

| 轮 | 开关组合 | 探针绑定形态 | 主机侧对端观测 | 是否进隧道 |
| --- | --- | --- | --- | --- |
| R0 | 仅不绑定（对照） | 未绑定 | 无 | **不进** |
| R1 | ＋隧道网段路由 | 未绑定 | 有 | **进** |
| R2 | ＋`blockedApplications=[自身]` | 未绑定 | 无 | **不进** |
| R3 | 路由＋blocked，探针绑定 VPN 网络 | `requested_net_id=134`、`result=bound` | 有 | **进** |
| R4 | 路由＋`protectProcessNet()` | 未绑定 | 无 | **不进** |

- 五轮探针 `sent=true` ⇒ R0／R2／R4 的「不进」不是「未发出」（`STAGE-RECORD.md:233-239`）。
- R3 专列：`N2H_POSCTL_BIND_NET|probe_id=…|requested_net_id=134|result=bound|errno_or_code=0`；同 socket mark `0x00000000`→`0x00030086`（只记原始十六进制、不解码、不据此反推 netId）。
- R4 专列：`VPN_PROTECT_PROCESS_NET|result=resolved|elapsedMs=11|switch=s3`，距 `VPN_CREATE_RESOLVED` 同一秒；同轮共 1 行，无 `rejected`／`throw`／`timeout`（`STAGE-RECORD.md:223`）。
- 平台两类行五轮各 3／2 行，现象可重复（`STAGE-RECORD.md:245-249`）；`gate.json` 记 `gate: "passed"`、`stage: "restore"`。
- **判读必须按盲审第 5 条收窄**（见 §2）：R1／R3「进」为强证据，R0／R2／R4「不进」为单次单包负结果、丢包也能解释，属**提示性证据**；本件**不得作为 N2-H 门结果复用**。

### 1.4 0032：0029 补跑

- 驱动 v12：`status=COMPLETED`、`armed=true`、`device_cmd_failed=false`；窗口开 `08:17:57.454`、闭 `08:18:02.860`，`N2H_COLLECTOR_CLOSE|verdict=n2h-inconclusive|frames_scanned=7|hits=0|probes_sent=9|receipts_observed=6`；`opened_mono`↔`openedMono`=248557622 同值配对（`STAGE-RECORD.md:86-87`）。
- **DNS 回执**：证据目录内 `dns-echo-receipt` 共 **4 条，全部 `hit`，无任何 `miss` 行**（`grep -raoE 'dns-echo-receipt=[a-z]+' .` ⇒ 4×`hit`；`dns-echo-receipt=miss` 0 命中，`STAGE-RECORD.md:106`）。四条分别对应 192.168.50.6:53、8.8.8.8:53、223.5.5.5:53、114.114.114.114:53。
- **C7 同源标定未完成**（`STAGE-RECORD.md:148`、`:170`、`:172`）：可用的 API vpn-tun 读数为 `08:17:57.455–.466`（`rxBytes=1040`、`txBytes=520`），最近的**有值** `/proc` 读数为 boot-post `device_epoch=1790381886.190865781`（`vpn-tun: 1760 B/22 包 rx`、`600 B/6 包 tx`）⇒ 间隔 **8.641 s（主机域）／8.725 s（设备域）**，且 **API 在前**（§4 所述「最好 API 在 /proc 之后」未满足）；click-pre（P1）**无 `vpn-tun` 行**（`status=unavailable`，接口当时尚未存在）⇒ A、B 两点各自缺一侧读数，两侧增量（API 沿①→② `+480/+80`、`/proc` P2→P3 `+80/+0`）分属不同时间区间，字节恒等式判据不成立。
- **沙箱写入被拒**（`STAGE-RECORD.md:184`）：`hdc shell -b cn.alfadb.netbird` 可进入沙箱 cwd `/mnt/debug/100/debug_hap/cn.alfadb.netbird`（`pwd` rc=0），但写探针文件时**远端 rc=1**、`/bin/sh: can't create …: Permission denied`（`status=unavailable`），读取回执同为 rc=1（文件不存在）⇒ 该投递路径在当前调试签名／版本下**不可写**。

## 2. 独立盲审口径（2026-09-26，逐条照录）

审查者：**openai/gpt-5.6-sol，干净上下文，只读，2026-09-26**。以下 1–6 为盲审结论口径，逐条照录，不放宽、不扩展、不收窄。

1. **撤回登记「逐 socket protect 在本平台 UNSAT / Promise 永不 settle」**。现行事实改为：「在 PLA-AL10 7.0.0.107(SP8C00E105R7P3) 上，观测到 40 次 `VpnConnection.protect(fd)` 调用全部 resolved（3–16 ms），调用前后 netmanager 打出 MANAGE_VPN 校验失败行但不影响返回」。
   - 证据（仓B）：`diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260925-0030/cmd-020-hilog-x-full-1.txt:343-354`、`:567-572`、`:1107`；`cmd-005-bm-dump.txt:15` 的 `accessTokenId=537437623` 与 `callerToken` 一致；**09-16 未调用 protect 时同样出现这些行**：`diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260915-0004/hilog-d1-20260916T0805-full.log:2556-2559`、`:2736-2739`、`:2850-2858`。
2. **限定**：返回成功**不等于**已证明该 socket 的流量绕开 VPN（端点路由排除可能掩盖）；要声称「protect 绕开语义已证实」，需另做实验：**去掉端点路由排除、存在会捕获目标的路由、对唯一标识的 socket 调用 protect，证明 TUN 未见且端点收到，并重复**。**不得写成 N2 pass**。
3. **09-14「永不 settle」的再解读**：run1（`diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260913-0001/device-run-1.hilog.log`）`fd=26` 打开于 `:3`，连接器 9 ms 后在 `:4-5` 启动；按当时代码（项目仓提交 `0834c2f` 的 `NetBirdVpnExtensionAbility.ets:295-309`），**只有 protect 返回 true 才会启动连接器** ⇒ 该次 protect **实际返回成功**；`:27` 的 `VPN_PROTECT_TIMEOUT` 是**盒子计时器未取消**（`0834c2f:1158-1177`、`fd5f71c:2506-2556`，均无 `clearTimeout`）造成的**假超时**。
   - **更正一个此前的错误说法**：09-14 日志里**没有 `SETTLED` 行**，原因是当时代码的**成功分支根本不打日志**（`0834c2f:1158-1167` 只 `resolve(true)`），**不是**「普通级别日志没抓到」。
   - **限制**：09-14 当天**未留存日志的其他尝试无法核实**；固件差异（当时 7.0.0.105 vs 现 7.0.0.107）**未单独测试**。
4. **撤出现行事实**：「ArkTS 线程被卡死、观测到两次」与「声明 `MANAGE_VPN` 导致安装失败 9568289」：**无任何留存原始日志支持**（09-14 目录唯一安装日志 `install-222946.log:1` 为成功）。撤出现行事实，历史中标注「**当时的说法，无留存原始证据、未核实**」。
5. **AUTH 0031 判读的收窄**：R1、R3（**进**隧道）为**强证据**；R0、R2、R4（**不进**）为**单次单包的负结果，丢包也能解释，属提示性证据**；R1 **只支持**「加入隧道网段路由后该未绑定 socket 进入隧道」，**不支持泛化为「路由决定一切」**。R4 的 `protectProcessNet` 在 create 后调用、探针在其后约 5.1 s 发出，**无时序混淆**，但**只证明 create 后调用的效果**。**0031 不得作为 N2-H 门结果复用**。要作厂商通则结论，需**多探针或 A/B/A 重复**。
6. **审查者**：openai/gpt-5.6-sol，干净上下文，只读，2026-09-26。

## 3. C7 与 DNS 的状态

| 项 | 现行状态 | 依据 |
| --- | --- | --- |
| **C7 同源标定**（v4 §4 前置②(ii)、§6.2-6） | **未完成**（前置保持）。0032 首次取得两侧读数，但**不同刻、不配对**：API 与最近有值 `/proc` 间隔 8.641 s／8.725 s 且 API 在前、click-pre 无 `vpn-tun` 行。C7 继续按现行缺口口径登记 | `AUTH-DIAG-DEVICE-VALIDATION-20260926-0032/STAGE-RECORD.md:148/:170/:172` |
| **DNS 回执**（v4 §3 路径①、§6.2-2 完成判据） | **部分**：回执链在真机**可达**（hit×4，qname 逐字回含），但**无 miss 行** ⇒ 完成判据「hit 与 miss 各一」**未达成**。与仓B 登记件 `A_implementation_face_status.items[2]`（DNS 路径①判 `partial`）一致，**不判 landed** | 同上 `:106/:110-113/:119-120` |
| **沙箱写入**（`hdc shell -b`） | **不可写**（当前调试签名／版本下）：可进沙箱 cwd，写文件 `Permission denied` ⇒ 该路径不能作为设备侧取证写入通道 | 同上 `:184` |
| **N2-H 正式状态** | 不变：`n2h-inconclusive`；上述四轮不产生任何门级结论 | 各轮授权件 `is_evidence=false`、`not_a_gate_pass=true` |

## 4. 对产品的含义

1. **protect 返回问题已排除。** `VpnConnection.protect(fd)` 的 Promise 在本平台会正常 settle（40/40 resolved，3–16 ms），因此产品可以**依赖调用结局**做分支，不需要再按「Promise 永不 settle」设计 fail-closed 等待；早期记录中的「永不 settle」及其派生的 UNSAT 记法**不成立**。配套的壳侧缺陷（盒子计时器不取消造成的假 `VPN_PROTECT_TIMEOUT`）已由提交 `b3e7b81` 修复。
2. **protect 的「绕开」语义仍待验证。** 返回成功只说明平台调用成功；在有端点路由排除的情况下，它**不能**证明该 socket 的流量真的没走 TUN。产品不应把「protect 返回 true」当作「流量已绕开 VPN」的证明，该命题须按 §5 待办 1 的专门实验建立。
3. **`blockedApplications` 与 `protectProcessNet()` 可用。** 单次观测中，`blockedApplications=[自身]`（R2）与 `protectProcessNet()`（R4，resolved 11 ms）都使未绑定探针不进隧道；同时 R3 表明**显式绑定到 VPN 网络的 socket 不受整应用排除影响**。即：需要让自身管理流量走物理网络时，这两条是可用手段；需要自身流量走隧道时，须显式绑定 VPN 网络。**注意这是提示性证据，需重复**（§5 待办 3）。
4. **隧道网段应声明在路由中。** R1 表明：把隧道网段加入 `VpnConfig.routes` 后，原本不进入 VPN 的未绑定探针进入了隧道。产品配置应确保**隧道网段出现在 `routes`**，否则自身流量不会进隧道（该项为强证据，但只支持这一条具体表述，不能泛化为「路由决定一切」）。
5. **C7 与 DNS 结论不能提前消费。** C7 同源标定未完成 ⇒ 统计口径的字节恒等式尚不可用；DNS 回执只有 hit、没有 miss ⇒ §6.2-2 判据未达成。二者都不得据本轮写成 landed／pass。
6. **平台 `MANAGE_VPN` 校验失败行可忽略。** 两类 netmanager 行在调用前后稳定出现、且 09-16 未调用 protect 时同样出现，与调用成败无因果；它们不影响 protect 返回成功。

## 5. 后续待办

1. **protect 绕开语义实验**（按 §2 第 2 条设计）：去掉端点路由排除 → 设置存在会捕获目标的路由 → 对**唯一标识**的 socket 调用 `protect` → 证明 **TUN 未见**且**端点收到** → **重复**。通过前不得声称「protect 绕开语义已证实」，不得写成 N2 pass。
2. **C7 同源标定 0033**：按 v4 §4 前置②(ii) 做一次**同刻对**（两侧间隔 <1 s、最好 API 在 /proc **之后**、A→B 注入已知 n×80 B 单向帧），且 A、B 两点都必须有 `vpn-tun` 的**两侧**读数（本轮 click-pre 无 `vpn-tun` 行是致命缺口）；标定记录落盘后随定稿再审。
3. **0031 负结果的重复**：R0／R2／R4 的「不进」是单次单包负结果，需**多探针或 A/B/A 重复**才能作为厂商通则结论；在此之前对厂商的表述保持「提示性证据」措辞。
4. **DNS miss 行**：构造一次真实的 miss 场景，使 §6.2-2 的「hit 与 miss 各一」完成判据达成。
5. **沙箱写入替代路径**：`hdc shell -b` 当前不可写；若产品需要设备侧写入取证，需另找可用通道，或先向厂商确认该路径的写入前提。
6. **固件差异**：7.0.0.105（09-14 run1 当时）与 7.0.0.107（本轮）未单独对照测试；涉及「当时不 settle、现在 settle」的因果归因时，须保留这一未测变量。
7. **第三轮厂商工单**：口径待按本文件收窄（第三轮追问草稿见 `/home/worker/huawei-support/20260926-round3-manage-vpn-log.md`，本文不重复其内容）。

---

### 引用与口径依据一览

- 四轮证据：仓B `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260921-0029/`、`…-20260925-0030/`、`…-20260925-0031/`、`…-20260926-0032/`（各目录 `STAGE-RECORD.md` ＋ `MANIFEST.sha256`）。
- 历史日志：仓B `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260913-0001/device-run-1.hilog.log`、`…-20260915-0004/hilog-d1-20260916T0805-full.log`。
- 代码：项目仓提交 `0834c2f`（当时的 `NetBirdVpnExtensionAbility.ets:295-309`、`:1158-1177`）、`fd5f71c`（`:2506-2556`）、`b3e7b81`（盒子计时器清理）。
- 口径依据：**独立盲审 2026-09-26（openai/gpt-5.6-sol，干净上下文、只读）**，逐条见 §2。
- 仓B 登记：`records/n2h-preregistration-v4-addendum-20260923.json` 的 `amendment_events[2]` 与 `B_vendor_statements.B8_protect_requirement_conflict.superseded_by_amendment`。

---

## 6. 晚间两轮（0033、0034）

> 本节于 2026-09-26 晚间收尾时追加；除本节外未改动本文任何字节。两轮均为**诊断观察**（仓B 各授权件 `is_evidence=false`、`not_a_gate_pass=true`），**不构成任何门结论**，不改变 §3 的 C7／DNS 状态表与 §5 待办的编号。

### 6.1 0033：C7 同源标定单轮——仍 `not-completed`，但首次取得两侧同刻对数据

- **结果**：`conclusion=not-completed`（判定工具 `analyze0033.py` `dc500dd8…e6f3`；`result-0033.json` sha256 `5802de70…6eb7`）。冻结配对 A=seq0（API `rxBytes=3280`）／B=seq17（`4536`），相隔 11.024 s；增量 API rx **+1256**／tx 0，`/proc` RX **+1840**（23 包，整 80 倍）／TX +80（1 包）；对端 b2a 账本 n ∈ [10,12]；5 个原因 token。`not-completed` 只是「原始数据未满足判据前提」的缺口标记，**不是失败**，不得据此推翻既有结论或放宽任何门。
- **新增观测**（仓B 该轮 `STAGE-RECORD.md` §6，**只作观测、不参与判定**）：17 个 API 取值**全部逐字出现在同期 `/proc` 的 RX 阶梯中**（无需任何基差修正）⇒ 两侧就本轮数据而言是**同一计数器的离散采样**；刷新周期只能给**量级 ≈1 s**（16 次取值变化的定位窗口 174–603 ms、中位 336 ms；首读间隔中位 1030 ms；按固定周期网格拟合无解）；相对 `/proc` 的同值首达滞后中位约 **215 ms**；≥2 s 的同值驻留只有 3 段（2517／2061／2085 ms），且这 3 段期间 `/proc` RX **全程不变**（是内核侧平台期，不构成刷新周期的证据）。
- **由此更正两处口径**（此前写法缺乏数据支持）：①「statistics API 读数按约 2–2.5 s 的周期阶梯刷新」；②「BURST 节拍约 600 ms」——40 拍平均 **601.2 ms**、逐拍间隔中位 **507 ms**、含 3 个 **1.6–2.2 s** 空档，「600 ms」只在平均值意义上成立。两处已按仓B 该件 `AMEND-0033-20260926-03`（`type=record-correction`）登记更正后的读法，**`consumption_events` 原文字符逐字未改**。
- **对 C7 的含义**：C7 **仍未完成**（§3 状态表保持不变），0032／0033 两轮都**不得**写成 C7 完成或任何门通过；但「同一计数器」与「量级 ≈1 s 采样」这两点从此有真机观测支持，可作为 §5 待办 2 复测设计的输入。

### 6.2 0034：protect 绕开语义——判定器 `uncertain`，观测层面强

- **结果**：冻结判定器 `judge0034.py`（`8982f7a4…9c5e`）判 **uncertain**（`verdict.json` sha256 `5ff6bf31…e611`）。七行 V1–V11（按各行适用项）**全部机械有效**；D0（无捕获、不 protect）TUN 0/5、echo 5/5（**72–161 ms**）；P0/P2（捕获路由 `N2H_DIAG_CAPTURE_ROUTE|result=added`、目标 `119.29.29.29/32` 进 `VPN_ROUTE_ENTRY`（`interface=vpn-tun`）、不 protect）echo **0/5**、timeout 5/5，但 **TUN 0/5** ⇒ 正控 C4b（≥4/5）**未满足** ⇒ 只记 uncertain；P1（`VPN_PROTECT_PROBE|outcome=settled|commit=ok|protect_result=protected`）TUN 0/5、echo 5/5（66–157 ms）⇒ C4a/C4c 满足、C5 不成立（无 `p1-tun-leak`）。
- **观测层面（并列登记，不作因果）**：同一捕获路由下，不 protect 回复**全丢**、真 protect 后**全部往返成功** ⇒ 「protect 绕开」在本设备本固件上具备**强观测证据**；缺的只是**捕获正控所需的 TUN 侧观测**。
- **复测三行与 0031 一致**：R0' **4/5**（`consistent-with-0031-r1`）、R2' **0/5**（`-r2`）、R4' **0/5** 且 `VPN_PROTECT_PROCESS_NET|result=resolved|elapsedMs=11`（`-r4`）⇒ 0031 的负结果在 **5 个唯一探针**下方向一致；其「提示性证据」定位（§2 盲审第 5 条）**不因本轮复测升级**——复测只做「方向是否一致」。

### 6.3 根因：tee 标签「先发后登记」（漏报方向）

- **根因**（收尾后确定；仓外报告 `/home/worker/n2h-kit/round-0034/tee-miss-rootcause-20260926.md`，只读）：`client/core/src/n2h_collector.rs` 的 `send_protect_probes` 在 `:4006` 发送并**等完整个 DNS 往返**（最多 `PROTECT_PROBE_REPLY_WAIT_MS = 1500 ms`）之后，才在 `:4011-4012` 调 `win.tee.register(...)`；查询帧被 TUN 读线程扫过时扫描表里还没有它的标签——帧确实被读了（`frames_scanned` 计入，P0/P2 = 23）却**永远匹配不上** ⇒ 七行 `tun_observed` 全 false。**100% 可复现的顺序缺陷，不是 TUN 观测通路故障**：同一窗口同一 tee 的 a2b posctl 帧 31 ms 内命中；离线 1:1 复现（先扫后登记 0 命中），把 `register` 前移后转 true。
- **同类缺陷**：posctl a2b 亦「先发后登记」（`:2986` `send_posctl` → `:2993` register），窗口仅微秒级，R0' 已实测丢 1/5。
- **影响方向是漏报**：在 `mode=protect`（判泄漏那一臂）上会把真泄漏读成 look-clean ⇒ **R2'/R4' 在修复前不能用来否定泄漏**。最小修复＝把 `register` 移到发送之前（建议同批加固 posctl）；**不要**用「延长 tee 生命周期／加 tail」修（帧被扫时条目根本不存在，detach 时机无关）。
- **网卡计数旁证**（同 arm 的 `N2H_IFACE_BYTES_WINDOW|interface=vpn-tun`）：tx 增量 D0 **+80**、P1 **+80**、P0 **+476**、P2 **+476**；476 = 80（被 tee 看到的那 1 个 a2b posctl 帧，`frame_len=80`）＋ **4×99**（99 = `send_len 71` ＋ 20 IP ＋ 8 UDP）⇒ 实测前 4 个查询帧确已按 99 B/帧写进隧道接口，两个独立 arm 数值一致；第 5 个在计数之后，属推断不属直测。该旁证**不改变**判定器结论（仍 uncertain）。

### 6.4 局限与更正

1. **0031 判据第 3 条的局限（本轮新发现）**：三行对端 `n2h-peer.log` 的 `a2b_seen` 均 **0 → 5**（各 5 条 `n2h-peer-receipt`），但 `last_a2b_from` 不同——R0' = **100.108.144.165**（手机隧道地址），R2'／R4' = **100.108.171.38**（另一 NetBird 节点）；同时 vpn-tun txBytes：R0' 576→976（**+400**）、R2' 576→576（**+0**）、R4' 464→464（**+0**）。⇒ R2'／R4' 的探针**没进手机隧道**（网卡计数不变），却经另一 NetBird 节点（据 `last_a2b_from` 推测为局域网内转发）到达对端。故 0031 `interpretation_rules` 第 3 条「对端 `a2b_seen` 增加 ⇒ 进隧道」**在该网络环境下不可靠**：今后判断「是否进隧道」以**网卡计数（`N2H_IFACE_BYTES_WINDOW`）／TUN 观测**为准，不以对端 `a2b_seen` 单独作准。**0031 当时 `a2b_seen` 未增加，其结论不因本条改变。**
2. **两处口径更正已登记**（仓B 各件 `type=record-correction`，原件 `consumption_events` 原文字符均未改）：0033——「API 约 2–2.5 s 周期」与「节拍约 600 ms」（见 §6.1）；0034——D0 的 echo 区间应为 **72–161 ms**（原写 66–161 ms；`66` 出现在 P1，P1 实测 66–157 ms 与原文相符）、会话时长应为 **626 s**（原写「约 10.5 分钟」；`preflight 18:59:45 → restore 19:10:11` = 626 s）。两处均为描述性文字，**不改变任何计数或结论**。
3. **`uncertain` 不等于「没有泄漏」**：本轮 `uncertain` 是判据口径的结论（C4b 正控未取得），不是对泄漏的否定；叠加 §6.3 的漏报缺陷，`mode=protect` 臂在修复前对泄漏问题**无判别力**。
4. **证据边界不变**：0033／0034 均不改 C7、DNS 回执、沙箱写入与 N2-H 的任何状态；两轮 `is_evidence=false`、`not_a_gate_pass=true`，不产生任何门级 verdict。

### 6.5 后续（对 §5 待办的增补）

1. **修 tee 登记时序后重跑 P0/P1/P2**：把 protect_probe 的 `register` 移到发送之前（`n2h_collector.rs:4006` 之前；建议同批加固 posctl `:2993` 之前的同类顺序），并补根因报告 §4 所列单测（其中并发形状测试**必须在旧顺序下失败**）；随后以**同一 arm** 重跑 P0/P1/P2，目标是取得 **C4b 的 TUN ≥4/5 正控**。修复前 R2'／R4' 不得用于否定泄漏。
2. **改配对规则的 C7 复测**：0033 的 `not-completed` 主要来自冻结配对规则把 A 点落在一次 **800 B 突增**中段（`proc_delta_RX=1840` 为最保守括界）。下一轮应在**配对规则层面**改掉（例如按 API 取值级对齐、或 A／B 各取「最近一次 `/proc` 读」的同刻对），并保持 §5 待办 2 的要求：两侧间隔 <1 s、A／B 两点都必须有 `vpn-tun` 的**两侧**读数。
3. **判据层面**：把「对端 `a2b_seen`」从进隧道的判据降为辅助观测，主判据改为网卡计数／TUN 观测（§6.4 第 1 条）。

### 本节引用

- 0033：仓B `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260926-0033.json`（`consumption_semantics.consumption_events[0]`、`amendment_events` `-01/-02/-03`）与其证据目录（`STAGE-RECORD.md` §4／§5／§6.2／§6.3／§10 第 8／9 条、`result-0033.json`、`MANIFEST.sha256` 72/72 OK）。
- 0034：仓B `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260926-0034.json`（`consumption_semantics.consumption_events[0]`、`amendment_events` `-01/-02`）与其证据目录（`STAGE-RECORD.md` §3.1–§3.6／§4／§5／§7 第 6 条、`verdict.json`、`counters.json`、`D0/P0/P1/P2` 的 `N2H_IFACE_BYTES_WINDOW` 行、`R0p|R2p|R4p/n2h-peer.log`、`MANIFEST.sha256` 504/504 OK）。
- 根因报告（仓外只读）：`/home/worker/n2h-kit/round-0034/tee-miss-rootcause-20260926.md`。
- 登记：仓B `records/n2h-preregistration-v4-addendum-20260923.json` 的 `amendment_events[3]`（`AMEND-N2H-V4-ADDENDUM-20260923-04`）。
