# E3 S5/S6 production layout 派生 fixture（2026-08-15）

本文登记 S5 与 sealed 0005 S6 B host-only production-layout 修复输入；不是新 AUTH、Live 或设备证据，不授权 HDC、设备操作、局部重放、AUTH 迁移、commit 或 push。它取代 [`ADJ-20260808-0003` 的历史未采样 `settings-app-info` matcher 口径](e3-physical-preflight-operator-trust-2026-08-08.md)：历史登记保留其当时事实不改写；当前生产 layout 规则以本文为准。

## 来源与完整性

- 来源：受控仓外 sealed production campaign `E3-PHYS-PREFLIGHT-20260815-0004` 的 `RAW-capture-scenario-5-app-info.json`。
- 原始 capture SHA-256：`a10d7828c7cb7d0e41d592332718f0c60e85473e4dc62244250a84421aa0c62c`，字节数 `928486`。
- hash manifest SHA-256：`bba23cbd02b6685d297be6cdbf339d3e03f71d848b60f38261254d5432d360c0`；campaign seal SHA-256：`2520c61099c38670832bd98aaa258b012f9aab8238f7d9c3c7eebce25e0fd584`；seal 中的 record SHA-256：`50d139c35436833bbc03ce9ff2e08e27faed4d49e52cbd253a2c14abe2c2142f`；sealed at `2026-08-15T15:48:30.7241620+08:00`。
- 仓内派生 fixture：`spikes/e3-vpn-extension-physical-preflight-hap/tests/fixtures/settings-app-info-production-0004.json`；SHA-256 `0d225f338b702c728096af16bb00b10fdf61ecf33c0b96ea2d8643168a3ce233`。

## 裁剪与逐项核对

fixture 不是 raw dump 副本，而是 **ancestor-preserving contraction**。它只保留 matcher 所需的 `attributes`/`children` 关系、必要祖先深度和强负污染节点；bounds、accessibility/hash、状态栏、电信网络、时间、电量、其他应用、版本详情及无关分支均删除。sceneboard 的 session id 与 host-window id 已脱敏置空。整个 raw capture 不入库。

对 fixture 中每个保留节点，均按同一节点的 `id`、`key`、`type`、`visible`、`text` 组合回查 sealed raw；没有把不同 raw 节点的字段拼成一个 fixture 节点。fixture 子节点中的空 `bundleName` 是裁剪 schema 为保持统一字段形状而写入的 **schema-normalization 占位**，不是声称 sealed raw 对应节点可按空 `bundleName` 回查的字段，也不参与 matcher；owner 只由保留的 Settings root `bundleName=com.huawei.hmos.settings` 决定。为裁剪中间祖先，fixture 中的每一条保留父子边表示 raw 中同一父节点到同一子节点的一条 ancestor-descendant 路径，**不**声称两节点在 raw 中直接相邻；隐藏祖先关系保持不变。因而 fixture 不宣称每个保留字段组合或每条边逐字逐层原样存在于 raw，但逐节点保留字段（上述归一化空 `bundleName` 除外）和祖先关系都可回查。核对后的生产结构为：

- Settings owner root：`bundleName=com.huawei.hmos.settings`，`type=root`，`visible=true`。
- 隐藏导航目的地：`type=NavDestination`，`id/key=Setting.Application`，`visible=false`。raw 不存在 `id/key=Setting.Application.ApplicationTab` 的该导航节点，fixture 不自造它。
- 真实 A/B 列表标签位于这个隐藏 `Setting.Application` 导航子树**内**：对应 resource id/key 分别带 `e3physvpna`、`e3physvpnb`，`type=Text`，`visible=true`，文本分别为 `E3 Preflight A`、`E3 Preflight B`。它们作为强负污染，不能满足 AppDetail 子树匹配。
- 当前详情页：`type=NavDestination`，`id/key=Setting.AppDetail`，`visible=true`；其真实可见子树内含 `Setting.AppDetail.title_id` 的 `E3 Preflight A` Text，以及文本为 `强行停止` 的 Button。
- sceneboard 子树外保留真实 `QuickBackText` / `E3 Physical VPN Preflight`，作为历史文本强负污染；其 session/host-window 标识已置空，它不再是 A/B matcher 的兼容标签。

生产 API26 dump 的 `Setting.AppDetail` 明确带 `visible=true`。matcher 不推断缺省可见性；缺失 `visible` 一律 fail-closed。这是已知输入边界，不表示其他系统版本必然采用相同 dump schema。

fixture 不含设备标识、设备 alias、HDC target/endpoint、地址、密码、token、密钥、证书或用户账号。公开测试 bundle/resource id 仅用于复现真实结构关系。

## 判定边界

step3 仍是严格机器门：只在 Settings owner 下唯一可见 `Setting.AppDetail` 子树内共同确认 expected bundle 的 distinct label（A=`E3 Preflight A`，B=`E3 Preflight B`）和 force-stop 控件。候选 AppDetail 自身必须 `visible=true`，从 Settings owner 到候选的所有祖先均不得为 `visible=false`；因此隐藏祖先下即使 AppDetail 自身标为可见也不能进入唯一可见集合。隐藏分支、应用列表、搜索结果、sceneboard 与近似文本均不能命中。

step4 操作员提示为 `点击强行停止，并完成随后出现的确认（如有）`。这是一个设置 UI 的机械步骤；runner 不自动点击、不猜确认框结构，也不扩 HDC 白名单或事件门。点击后的 `scenario-5-app-info-force-stop` capture 仅作 observation-only 证据，页面离开 AppDetail、结构变化或一般 capture 退化都不独立改变 campaign verdict；但连续 HiLog stream 已处于 degraded 时，仍按全局安全规则判为 blocked。

S5 最终成功仍必须满足既有机器后置条件：连续 `<bundle>:vpn` process absent 且 bundle present。进程未退出、探针不足、bundle 不可确认或其他既有机器条件不满足时继续 fail-closed。

## S6 B 首次授权来源（0005）

- 来源 campaign：受控仓外 sealed production campaign `E3-PHYS-PREFLIGHT-20260815-0005`。sealed `scenario-results.json` SHA-256 `a63ef1548bb11fa21795b136e3ddc992c9a7bcc45a62f66a0942ff72cb134bb9`，`hash-manifest.json` SHA-256 `b12f3a3f1b6b3f644030e52af3f4e5aa52ae751fd79d20ce3a379a28b381a996`，`campaign-seal.json` SHA-256 `2790412962a9fcdedc1a018889eb4424a7e6af9682df55279099b496528ff55c`；seal 内冻结 record/manifest hash 与现场复算一致。
- 实际 post-B-Start raw 文件 basename 是 `capture-scenario-6-conflict.json`，不是 step3 的 capture-before `scenario-6-entry-b`；`RAW-capture-scenario-6-conflict.json` 是 sealed public reference，不是实际 basename。raw layout SHA-256 `7f6e44d5eab7021d192a6a61f409af9a3e8507f16c1df6bcf84755c1130bb72c`、字节数 `74801`；对应 screenshot SHA-256 `b22dbc8b31fbf40c8d4eddd4f0e3bb945d0c689cf3a14aa8c53e72ba1d5a181b`、字节数 `1083162`。manifest 中两项 hash/size 与仓外 raw 文件复算一致。
- 现场事件绑定：S6 B 的新 request 只有唯一 `UI_START`，随后没有 B `VPN_ONCREATE`、`VPN_CREATE_REJECTED`、`START_PROMISE_REJECTED` 或 `CREATE_ACCEPTED` terminal；step3 decisive capture 已显示 `com.huawei.hmos.vpndialog` 的 B 授权 Dialog。因旧 runner 采集该画面后直接等待 create terminal，0005 最终为 S6 blocked `platform-marker-missing:B-create-terminal-missing`。
- 仓内派生 fixture：`spikes/e3-vpn-extension-physical-preflight-hap/tests/fixtures/s6-b-authorization-production-0005.json`，SHA-256 `8d99c65e6bfa0c9569848f6a482fd188c1bcb6b6837ca0e8386c052c33f376ad`。

该 fixture 是最小 ancestor-preserving contraction：保留 vpndialog root、Dialog 祖先、公开 B ownership label `E3 Preflight B`、`是否允许使用 VPN？` 以及可点击 `取消`/`允许` 控件；中间 Column 层按祖先路径收缩。删除 bounds、accessibility/hash、host-window/session、状态栏、运营商、时间、电量、网速、sceneboard 和警示长文。未复制 raw layout 或 screenshot，也不含 target、endpoint、设备 model/alias、账号、地址、凭据或签名私密材料。

## S6 B 当前规则

当前 S6 规则已绑定新 AUTH `AUTH-E3-PHYS1API26-20260825-0001` 及新 pair `E3-PHYS-PREFLIGHT-20260825-0001` / `EV-E3-PHYS1API26-20260825-0001`，只可从 gate 1 开始进入完整 13 门 campaign；0002 已在 gate 4（用户 host-prep）未完成时提前执行 gate 5 TargetBindingConfirm 产生 blocked confirmation record 以 `governance-order-invalid-retired` 退役，未 DryRun/Live、未 consumed、只作历史 provenance；0001 已在 gate 9 因未授权设备进程枚举退役，audit-2 文件不构成有效 gate pass，未 DryRun/Live、未 consumed，只作历史 provenance。S6 保持 A Start=`1`、A optional Allow=`2`、B Start=`3`、B optional Allow=`4`。唯一 B Start 后对 `scenario-6-conflict` 执行现有 entry/authorization 双档案 checkpoint（expected B）：entry 直接进入既有 B terminal 等待；authorization 才提示单一机械动作 `点击 Allow`，设备上的对应可见控件文字为 `允许`。step4 `capture_before` 绑定该双档案 checkpoint，其 machine precondition 只声明 authorization layout/request 已由机器验证，不伪称或复用新的 process 读数；`scenario-6-after-allow-b` 必须通过 `authorization-dismissed`。已收集但未 dismiss 的 layout mismatch 与已收集但 dismissal JSON/layout 不可验证均 fail-closed 为 blocked，reason 分别保留；capture 未收集则沿既有 decisive gate 记 invalid，若明确为 capture infrastructure 则 infrastructure blocked。step4 prompt 后到 postcondition 完成前已进入 capture 的 stray UI action 可安全归因 step4；step4 完成后才迟到进入窗口的事件无法可靠证明属于该次点击，因此不伪造 step4 归因，仍由完整 event contract 以 B terminal/window anchor 判 invalid。

B Start 前的 A exact-process 是 pre-gate。B accepted/rejected terminal 后都观察且只观察一次真正的 terminal process checkpoint 并写入 `machine_process_checkpoint`，但 checkpoint 只 gate rejected 分支；不做 Allow 后或最终聚合阶段的重复探测。terminal pass 或 nonfrozen-blocked 后统一完成一次 context、扫描一次 unexpected accepted、断言一次 event contract。窗口 accepted 计数严格绑定已验证的 A/B requestId：只有这两个 requestId 的 `CREATE_ACCEPTED` marker 计入；foreign 或 `requestId=missing` accepted 不计入，且后者优先归类为 unexpected invalid。既有 terminal 识别的 tag 关联容错仅用于 terminal 识别，不放宽 accepted 计数。任何 B accepted marker 或双 accepted 先判功能 fail，包括 frozen reject 后完整窗口内迟到的同 B request accepted，即使 terminal checkpoint 显示 A 消失/blocked 也不得降级；然后才判 rejected-terminal A 不可验证 blocked、非冻结码 blocked、窗口退化 blocked；冻结冲突码且 A checkpoint pass 才为 pass。无 B terminal 保持 runner blocked，随后 S7 为 `result=blocked / reason=not-run-after-runner-failure`。只有 S6 pass 的 verified A 才绑定 S7。runner 不自动点击、不新增 UI 输入、不扩 HDC 白名单或 discovery。record 与 `operator-wait-state` schema 不升级；新增 capture 名沿既有 manifest/record/transcript 收集机制进入封签。

## 批准范围

用户于 2026-08-17 批准保留 0001 全部仓外历史对象并迁移到 AUTH/pair 20260817-0002 的完整 13 门。0004 S5 与 0005 S6 B 的最小脱敏 production-derived fixture 继续只作 provenance；0002 已因 gate 4（用户 host-prep）未完成时提前执行 gate 5 TargetBindingConfirm 产生 blocked confirmation record 以 `governance-order-invalid-retired` 退役，gate 6-13 未运行、未 DryRun/Live、未 consumed；当前执行常量绑定 `AUTH-E3-PHYS1API26-20260825-0001` 及 pair `E3-PHYS-PREFLIGHT-20260825-0001` / `EV-E3-PHYS1API26-20260825-0001`，reviewer role exact 为 `isolated-anthropic-claude-opus-5-reviewer`。S6/reviewer/source/HAP 逻辑不改，历史 AUTH/pair 均不可回溯或复用。当前 host-only registration 禁止真实 HDC executable、设备、新 pair audits/freezes/records、TargetBindingConfirm、DryRun、Live、commit 与 push。
