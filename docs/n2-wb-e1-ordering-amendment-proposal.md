# N2 Wb C/D protect↔tee-attach 次序窄修订提案（供人审阅）

- 状态：**DRAFT / UNAPPROVED** —— 未批准、未冻结、非 AUTH、非门结论。用户批准前，本文件**不改变**任何既有规则，也不得被引用为规则文本。
- 对象：冻结件 `docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.1）中 Wb 行的替代证据 E1 次序子句，以及 Wb×C / Wb×D 两格的替代证据列（**Wb×A、Wb×B 两格正文与「同左」均不动**）。
- 本文件**不是**新冻结本身；批准后的登记另走 §7 第 2 步（只追加不改写）。
- 写者边界：本提案只读代码与文档；未改冻结件、未改 judge/runner/模板、未委派、未提交、未使用 hdc/ssh、未碰 kit 目录。

## 1. 【需批准】精确拟替换条款（其余为背景）

**批准对象 = R1–R4 四条文本修订 + F1 一条禁令。** 除 R2 明确限定的「同一持续挂载 tee 内 C/D 后续代次序」窄例外外，无任何放宽；Wb×A / Wb×B 两格、任何新 tee 的首个 Wb 注册、第二批 E/F/G 判据均不在批准对象内。

### R1 —— 冻结 `:57` Wb 行 C/D 两格「同左」的限定（A/B 不动）

- **A 格正文不动**：A 格仍逐字保留冻结原文 `protect 完成时刻早于 tee attach 锚点`（其初次条件由 R2 明确，格内文字不改）。
- **B 格「同左」不动**：B 格仍为 `同左；断链刺激后 base fd 仍零发送`，其 E1 仍是「断链刺激前后 base fd 身份/代际不变」，不引入新代次序例外。
- 旧（逐字，C 格）：`同左；新网络下 base fd 仍零发送`；旧（逐字，D 格）：`同左；跨方向反转 base fd 仍零发送`。
- 拟新文本（待批准）：C 格 `同左（**不含** before-attach 次序——**仅当**本次切网重开/protect/重馈 base 发生在**同一个持续挂载的 tee 生命周期内**时，E1 次序按 R2 的 C/D 后续代例外判定；其余判据不变）；新网络下 base fd 仍零发送`；D 格同文，方向反转为 `跨方向反转 base fd 仍零发送`。
- **其余情形不变**：不在同一持续挂载 tee 生命周期内的任何情形（含新 tee 的首个 Wb 注册、A/B 两格）**不适用**该例外，仍按 R2 的初次条件（protect 严格早于该 tee attach）；第二批 E 的受控 recreate 按第二批原规则**另判**，不在本例外的适用范围。

### R2 —— 冻结 `:202`（E1）的次序子句（本次修订的核心，唯一实质放宽）

- 旧（逐字）：「齐全；且 **`protect_settle` 时刻严格早于 tee attach 锚点**（`N2_TUN_GAP|reason=before-tun-attach`，`wg_device.rs:2269-2271` / `N7_WG_FEED|adopted`，`:2303`；`ets:3331,3506-3563` 的 protect 在 `:3350,3452,3471` 的 attach 之前）。`local` 出现 `100.108.0.0/16`（`N2_SOCK_OVERLAY_LOCAL`）即 fail。」
- 拟新文本（待批准）：

> 齐全；且 **`protect_settle` 时刻严格早于该 tee attach 锚点** 这一要求对**每一个独立 tee 生命周期内首次成功接管登记的 Wb base**仍然成立（初次条件；**删除**「只在该 campaign 首次成功」的错误读法）；**仅**当 C/D 的后续代（**同一个持续挂载的 tee 生命周期内**、切换后新网络重新打开、protect、重馈的 base fd）时才**不适用**该 before-attach 次序，改按下列新代次序判定：
>
> （a）该次新 protect 的 `protect_settle|status=success` 时刻（核心 `protect_mono_ms`：壳返回 protect 结果后调用 core `n2_socket_trace_protect` 时由 `sys::mono_ms` 采样的**壳判决/平台完成时刻上界**——**不是**精确平台完成时刻，也**不是** adopt/接管时刻；与 tee 锚点同用核心单调时钟比较，不做墙钟换算）**必须严格晚于该格新网络前控 HIT**，且**严格早于该格后控 HIT**（`pre_hit_ms < protect_mono_ms < post_hit_ms`；**相等不能证明该严格关系**，任一等号成立即 `UNSAT`）；前后控 HIT 必须由核心 tee 在**新网络**上观察到；
> （b）新代身份 `(fd, dup_fd, protect_generation)` 必须是切换基线之外的**新三元组**（代际异于基线、`(fd,dup_fd)` 异于基线），`net_id` 为切换时实测的**新网络** netId（0 拒绝），`local` 为实测物理 IP:port，`protect=success`；任一缺件即 `UNSAT`；
> （c）该格窗口内 tee **持续挂载、无 `N2_TUN_GAP` 缺口**，且前后控均命中（vantage 非盲）；
> （d）本条款**不主张** Wb 有真实生产出包、也**不提供物理出口证明**；C/D 的 Wb 证据仍是零发送负证据 + 身份/protect 登记。
>
> **适用范围（窄例外）**：上述新代次序**仅**适用于同一持续挂载 tee 生命周期内 C/D 的后续代；**A、B 两格**与**任何新 tee 的首个 Wb 注册**仍适用初次条件；**第二批 E 的受控 recreate / TUN 替换不在本例外内**，按第二批原规则另判。
>
> **实现现状（本提案并非已实现符合）**：当前 core 后控判定用 `post_hit_ms >= receipt_ms`（`n2_cd.rs:1019-1028`），**可能落在同一毫秒**；本提案要求的严格边界 `pre_hit_ms < protect_mono_ms < post_hit_ms` **尚未实现**。获批后须按 §7 第 1 步实施并测试该严格边界，才可作为判据。
>
> **不变项**：E2/E3/E4/E5/E6 全部原要求不变；Wb 零生产发送不变；§3 阈值、§2 配置冻结、治理标签定义不变；E/F/G 与第二批判据不变。

### R3 —— 冻结 `:197`（Wb×C）替代证据列

- 旧（逐字）：「E1 新网络下 base fd protect 代际/`net_id` 登记；E2/E3 零发送；E4 新网络下四表对账；E5 蜂窝**来源闭合**（源用当轮实测，不预填 v4:113 的 `223.153.208.166`）；E6 窗口含切网点」
- 拟新文本（待批准）：「E1（按 R2 新代次序，仅限**同一持续挂载 tee 生命周期内**的 C/D 后续代）新网络下 base fd 的新代 `protect_settle|success` **严格位于前控 HIT 之后、后控 HIT 之前**（`pre_hit_ms < protect_mono_ms < post_hit_ms`，含相等即 `UNSAT`），且 `(fd,dup_fd,protect_generation)`/`net_id`/`local` 齐全；E2/E3 零发送；E4 新网络下四表对账；E5 蜂窝**来源闭合**（源用当轮实测，不预填 v4:113 的 `223.153.208.166`）；E6 窗口含切网点」
- `:198`（Wb×D）「同 C，LAN 直连下 E5 用**五元组零命中**（源按 Wi-Fi 实测 `192.168.50.199` 或当轮实测 LAN/NAT 源）」文字不动，其中「同 C」引用 R3 后的 C 列。

### R4 —— 比较时钟的精确化（随 R1/R2 一并批准；属措辞精确化，非放宽）

- 冻结 `:202` 未写明比较用哪个时钟；壳侧 `N2_SOCK|phase=protect_settle` 行今日只有 `wall_ms`（`NetBirdVpnExtensionAbility.ets:5241-5245`），核心 Wb 接管行才有 `protect_mono_ms`（`n2_socket_trace.rs:382-393`）。
- 拟新文本（待批准）：次序比较一律使用**核心单调时钟**字段：Wb 行的 `protect_mono_ms`（`n2_socket_trace.rs:195-211`、`:300-303`）与 tee 锚点的核心 `sys::mono_ms`（`wg_device.rs:2423`、`:2429`）比较；不使用壳侧 `wall_ms`。
- 采样语义（随 R2（a）一并批准）：`protect_mono_ms` 是**壳返回 protect 结果之后**、调用 core `n2_socket_trace_protect` 时由 `sys::mono_ms` 采样的**壳判决/平台完成时刻上界**——既**不是**精确的平台完成时刻，也**不是** adopt/接管时刻；与 tee 锚点的核心 `sys::mono_ms` 同钟比较，**不做墙钟换算**。

### F1 —— 禁令（与 R1–R4 同时生效）

**禁止**以「切换时重新挂载 tee / 重设 attach 锚点 / 让 tee 在切换后再挂一次」为**凑** C/D 新代次序人为制造条件；**禁止**测后补写锚点或回执时刻；**禁止**测后追认旧数据（§6）。

本禁令**只**禁止上述为凑次序的人为重挂/重设/事后补写；**不禁止**另行授权的原第二批 E 受控 recreate / TUN 替换——该路径按第二批原规则（E 列判据）**另判**，且**不因此获得**本修订的 C/D 新代次序例外。本修订**不提供**任何隐式第三方案。

## 2. 两选项（请择一）

- **选项 A（推荐，测前修订）**：在**任何正式测量之前**评审并批准 R1–R4 + F1；批准后按 §7 实施与登记，C/D 的 Wb 证据按新代次序判定。放宽仅限次序，证据门槛与 fail 条件不减。
- **选项 B（保持 v4.1 原文）**：不批准，冻结文本维持 `:57` / `:197` / `:202` 原字。此时按冻结文本，现实现的 Wb×C / Wb×D **结构性不满足 E1**（新代 protect 必然晚于 tee attach），两格只能 `UNSAT`；**不授予第一阶段标签、也不提请第一阶段通过**。UNSAT 是「证据不可得」，**不是** pass，也**不能**抵消实质 fail。

两选项都不改变：任一 Wb fd 实际发送、**未保护 base 实际被接管/使用**（或 protect 失败后**裸发 / 真实 TUN 泄漏**）、from-OS TUN 命中、`N11_WG_FEED|replay-egress-failed`、或 adopted 时 `endpoints>0` 而该 peer egress 未重放成功 → 该格 **fail**（回 T0 + 用户重新裁决）。**fail 不得被 UNSAT 掩盖**；反之，`protect=false` 且**未成功接管、无裸发**（即仅未取得 E1 成功登记）→ `UNSAT`，不升为 fail（与 N5 一致）。

## 3. 语义变更清单（明确哪条是语义变更）

- **语义变更（放宽，仅次序；严格限定 C/D 后续代）**：R1、R2（a）、R3 —— 原「每个 Wb 注册的 protect_settle 严格早于 tee attach」**并非**「每个 campaign 只判一次」的读法（**删除该错误读法**）；本修订把 before-attach 要求保留给**每一个独立 tee 生命周期内首次成功接管登记的 Wb**，**仅**同一持续挂载 tee 内 C/D 的后续代改以夹心次序 `pre_hit_ms < protect_mono_ms < post_hit_ms` 判定。除该窄例外外的**其它标准全部不变**（证据门槛、fail 条件、Wb×A / Wb×B、任何新 tee 首次、第二批 E/F/G 判据）。
- **措辞精确化（非放宽，但需一并批准才可用）**：R4（改用核心 mono 字段；`protect_mono_ms` 为壳判决/平台完成时刻**上界**，见 R2（a）与 R4 采样语义段）。
- **纯复述、非变更**：R2 的（b）（c）（d）与「不变项」段，均为 `:200-210`、`:212`、治理登记的原有要求。
- **新增明确防绕行限制（新增条款，非放宽）**：F1 —— 它**不是**对既有语义的复述，也**不是**「非变更」，而是本次新增的防绕行禁令。

## 4. 背景：原冻结文本与哈希（准确引文）

- 冻结件路径：`/home/worker/work/base/netbird-harmonyos/docs/n2-device-evidence-preregistration-draft-v2-20260928.md`
  - 本提案核对时（2026-09-30 现算）磁盘 sha256：`06db804da6ce788ac862b987fe7bc069517ccda93d467b24f4bc15746648bed1`
  - 该件自述：v4 原冻结版 sha256 `528ef22b97176f0813ef0524175bbb6c9202762a072745d3753262e36c46f099`；批准依据提案（r2）sha256 `c05b301e233cc245359af7be71e16cd22f5fb5b06c87da8602a3899b8ea945b4`（见该件 `:222`；r2 提案磁盘 sha256 实测与之一致）。
- 原批准依据提案：`docs/n2-prereg-v4-revision-proposal-20260928.md:82`（`protect 完成时刻早于 tee attach 锚点`）与 `:94-99`（Wb×C 依据「切网只重解析/重 feed ICE 池（`ets:777-784,797,812,835`），不触碰 base fd」，E1 同文）——与冻结件同文，故同受本提案约束。
- 冻结件内的行号自引已漂移，本提案一律引用**当前**行：tee 挂载锚点在 `wg_device.rs:2420-2429`（而非 `:2269-2271`），`N7_WG_FEED|adopted` 行格式在 `wg_device.rs:617`（而非 `:2303`）。

## 5. 现实现的真实冲突与代码引用

1. **初始路径满足初次条件**：壳 `wg_fwd_open` → `protectSocketWithBox(..., 'Wb')` → `connectorWgSocketFeed`（`NetBirdVpnExtensionAbility.ets:4907`、`:4932`、`:4839`）；核心 `WgDevice::adopt`（`wg_device.rs:793-825`，由 `try_build` 在 `:2415` 调用）内 `n2_socket_trace::adopted`（`:809`）**早于** tee 挂载与 `N2_TUN_GAP|reason=before-tun-attach`（`wg_device.rs:2420-2429`）。即初次 protect settle < attach 成立。
2. **切网路径必然晚于 attach**：切网恢复重开并重馈 base fd —— `wg_fwd_open`（`NetBirdVpnExtensionAbility.ets:994`）→ 门放行后 `protectSocketWithBox(vpn, opened.fd, 'Wb')`（`:1001`）→ `connectorWgSocketFeed(opened.fd)`（`:1004`）；核心在设备已起时走 `feed_fd` 的替换分支（`wg_device.rs:2313-2326`）→ `reattach_socket`（`:859-884`，只 `dup` 换 base，**不重挂 tee**）→ `adopted`（`:872`）→ 以该注册的 `protect_mono_ms` 喂 C/D 回执并打印 `phase=protect_settle` 行（`n2_socket_trace.rs:235-304`、`:382-393`）。
3. **判定器要求新代 protect 落在前控 HIT 之后**：`n2_cd.rs:792-843` —— 回执只在 `Phase::Released`（前控 HIT 之后）受理（`:806-808`），`io_mono_ms <= pre_hit_ms` 即 `io-before-pre-hit`（`:819-821`），Wb 必须带新代 `protect_settle`（`:839-843`）；后控 `observed` 要求 `post_hit_ms >= receipt_ms`（`:1019-1028`）。宿主测试钉住该语义：`tests/n2_cd.rs:677`、`:596`、`:737`，`tests/n2_cd_state_revision.rs:474`。**注意**：该 `>=` 是**当前实现**（同毫秒可通过），本修订要求的严格 `post_hit_ms > protect_mono_ms` **尚未实现**，获批后须按 §7 第 1 步实施并测试。
4. **因此矛盾**：新代 protect settle 必然 `> 前控 HIT > 正式切换 > tee attach`，与冻结 `:202` 的「严格早于 tee attach、且未给同一持续挂载 tee 内 C/D 后续代任何例外」**结构性不可同时成立**。按冻结文本，Wb×C / Wb×D 只能 `UNSAT`，不能授予第一阶段标签。
5. **原提案引用的失效**：`n2-prereg-v4-revision-proposal-20260928.md:94` 引用的 `ets:777-784,797,812,835`，当前内容为 C/D 恢复门注释（`n2CdGateState`），并非「切网只重解析/重 feed ICE 池」；且实现确实在切网恢复路径重开并重馈 base fd（见第 2 点）。「切网不碰 base fd」与现实现不符。
6. **澄清**：判定器/评审所用的宽松读法（「仅初次 protect<attach，后续仅身份」）**未获批准**，不能当作现行规则；本修订**不采用**该读法，而按 R2 的「**每个独立 tee 生命周期首次**」+「**同一持续挂载 tee 内 C/D 后续代例外**」改写。R1–R4 是把该精确文本变成「已批准文本」的唯一途径。

## 6. 禁止测后追认旧数据

- 已有/在跑的 C/D 数据、日志环、`N2_CD_SANDWICH` 行，**一律不得**在批准后被追认为符合新条款的证据：新代次序（`pre_hit_ms < protect_mono_ms < post_hit_ms`）必须是在**批准并登记之后**的新窗口内实测的原始行。
- 不得改写、补写、回填任何 `protect_mono_ms` / `pre_hit_ms` / `post_hit_ms`；不得用墙钟换算冒充核心单调时钟。
- 不得以「反正只放宽次序」为由，把旧 UNSAT 格翻成豁免格。

## 7. 批准后的三步（互相独立，不得合并）

1. **实施（代码）**：按获批文本对齐判定器与执行器（如需），**必须实现并测试严格边界**（`pre_hit_ms < protect_mono_ms < post_hit_ms`，消除当前 `post_hit_ms >= receipt_ms` 的同毫秒可通过），主机测试（`cargo test -p netbird_core n2_cd` 等本地测试，无 hdc/ssh）；由**未参与本修订设计与实现**的独立 T0 做实现符合性审查（跨厂商 family，评审独立性）。
2. **新冻结登记**：先完成新件全文（在冻结件文末**只追加不改写**一节 v4.2 或 v4.1.1，登记获批 R1–R4 + F1 原文、该追加节的版本与批准条款、**旧件 sha256**）；**完整新文件落盘后**再对新件整体计算 sha256，写入**独立 sidecar**（`<新件>.sha256`）与治理文末的同版登记——**不把新件自身的整文件 hash 塞进新件自身**。**本提案文件本身不自称冻结，也不是新冻结。**
3. **AUTH**：两批 AUTH 仍**分别**签发与回读确认；本修订不放宽任何 AUTH 前置项（P4–P8、端点排除开关、抓包授权/`setcap` 登记、两抓包点、`-s 96` 等）与第二批 E/F/G 判据。

## 8. 验收正负例与独立 T0 符合性检查点

**正例（应可判 `exempted-with-substitute-evidence`，绝不写 pass）**

- P1 初次（**每个独立 tee 生命周期内首次**成功接管登记）：`N2_SOCK|phase=protect_settle|kind=Wb|...|protect_mono_ms=T1|status=success`，且 `N2_TUN_GAP|from_mono=T0|to_mono=T2|reason=before-tun-attach`，`T1 < T2`；E1–E6 齐备。
- P2（Wb×C，**同一持续挂载 tee 生命周期内**）：切换后前控在核心 tee 命中（`pre_hit_ms` > 切换）；壳恢复路径 protect+feed 新 base；核心 `protect_mono_ms=S`，且 `pre_hit_ms < S < post_hit_ms`（**严格**，与 N3 互补）；后控 HIT 严格晚于 S（`post_hit_ms > S`，同毫秒不算）；出现 `N2_CD_SANDWICH|...|phase=receipt|result=exempted|reason=wb-protect-settle-success`；新三元组/`net_id`/`local` 齐全；tee 无缺口；E2/E3 零发送、E4 在册、E5 蜂窝来源闭合、E6 覆盖切网点。
- P3（Wb×D）：同 P2 方向反转，LAN 下 E5 用五元组零命中。

**负例（必须具名 `UNSAT` 或 `fail`，且分级不得混淆）**

- N1 初次（每个独立 tee 生命周期首次）protect_settle 晚于**该 tee** 的 attach 锚点（含相等）→ 初次条件不满足 → `UNSAT`（不得用 C/D 后续代条款补救初次，也不得用新 tee 的例外）。
- N2 C/D 后续代 protect settle ≤ 前控 HIT（`protect_mono_ms <= pre_hit_ms`，含同毫秒相等）→ `io-before-pre-hit` → `UNSAT`。
- N3 后控 HIT **不晚于**新代 protect settle（`post_hit_ms <= protect_mono_ms`，**含同毫秒相等**）→ `post-before-receipt` → `UNSAT`（与 P2 的严格 `<` 互补；相等不能证明严格关系）。
- N4 新代三元组与切换基线相同（`generation-unchanged` / `identity-unchanged`）、或 `net_id` 不匹配/未实测（`net-id-mismatch` / `net-id-unmeasured`）→ `UNSAT`。
- N5 `protect=false`（`protect-failed`）且**未成功接管、无裸发**（即仅未取得 E1 成功登记）→ `UNSAT`，且任何行不得写成 success；**若**未保护 base 实际被接管/使用、或失败后出现裸发/真实 TUN 泄漏 → 按 N7 判 `fail`（fail 优先，不降级为 UNSAT）。
- N6 窗口内 `N2_TUN_GAP` 与 base fd 可能发送时段重叠 → `UNSAT`。
- N7 **实质 fail（不得降级为 UNSAT，优先于 N5）**：任一 Wb fd 实际发送（E2 回执 / E3 按 fd 计数 / `N2_FD_SEND` 非零）、**未保护 base 实际被接管/使用**、**protect 失败后裸发或真实 TUN 泄漏**、from-OS TUN 命中、`N11_WG_FEED|replay-egress-failed`、或 adopted 时 `endpoints>0` 而该 peer egress 未重放成功 → 该格 `fail`，停该格，回 T0 + 用户重新裁决。

**独立 T0 符合性检查点（批准后一次性，干净上下文，跨厂商）**

- C1 文本一致：新冻结文本与获批 R1–R4 逐字一致，除获批 R1–R4 四条 + F1 外原规则未被隐式放宽（尤其 A/B 格与新 tee 首次、第二批 E/F/G 判据）。
- C2 判定器对应：`n2_cd.rs` 的次序/身份检查与 R2（a）（b）一一对应且为**严格**边界（`<`/`>`，相等即 `UNSAT`）；`n2_socket_trace.rs` 的 `protect_mono_ms` 语义（壳返回 protect 结果后 core 采样 `sys::mono_ms` 的**上界**——非精确平台完成时刻、非 adopt/接管时刻）与 R4 一致；无新增通过条件。
- C3 无隐藏第三路径（**不是**禁止一切 recreate）：不存在**为使 C/D 新代 protect 满足 before-attach 次序**而重挂 tee / 重设 attach 锚点的路径 —— `attach_anchor` 仅在 `wg_device.rs:2429` 设备构建时调用一次；另行授权的原第二批 E 受控 recreate / TUN 替换（`:2344-2345` 只产生 `tun-replaced` 缺口）**不复位锚点**，且**不因此获得** C/D 后续代次序例外，按第二批原规则**另判**。
- C4 负例可复现：N1–N7 在主机测试中产生具名 `unsat`/`fail`，且 N7 不被 UNSAT 掩盖。
- C5 未波及面：E/F/G 与第二批判据、oracle vantage、24 格 + 4 豁免格口径不变。

## 9. 本文件不做的事

- 不把状态写成 accepted / approved；不改冻结件、judge、runner、模板；不提交；不执行 hdc/ssh/抓包/设备操作；不碰 kit 目录（另一代理正在改正控 join）。
