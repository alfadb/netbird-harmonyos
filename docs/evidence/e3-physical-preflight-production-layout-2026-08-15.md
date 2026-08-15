# E3 S5 production layout 派生 fixture（2026-08-15）

本文登记 S5 host-only production-layout 修复输入；不是新 AUTH、Live 或设备证据，不授权 HDC、设备操作、局部重放、AUTH 迁移、commit 或 push。它取代 [`ADJ-20260808-0003` 的历史未采样 `settings-app-info` matcher 口径](e3-physical-preflight-operator-trust-2026-08-08.md)：历史登记保留其当时事实不改写；当前生产 layout 规则以本文为准。

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

## 批准范围

用户于当前任务明确选择完整 S5 修复并批准使用 0004 脱敏生产派生 fixture。批准范围仅限 host-only matcher、fixture、selftest 和必要文档修复；不包含 HDC/设备/Live、AUTH 迁移、commit 或 push。
