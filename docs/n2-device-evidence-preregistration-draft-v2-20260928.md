# N2 真机证据预注册 v4.1（FROZEN，2026-09-28）

> **FROZEN v4.1 / 冻结时间 2026-09-28T22:07:17+08:00 / 非设备授权**。本稿覆盖全部外层类别与全部生命周期列，**取代**[第一阶段草案](n2-device-evidence-preregistration-phase1-draft-20260927.md)与[上游全量草案](n2-device-evidence-preregistration-draft-20260927.md)的操作判据；两份旧稿原样保留，仅作历史输入，不可混用其中相冲突的正控、路由或授权条款。全稿判据、oracle、阈值、停止条件、适用集及不外推规则自此冻结；变更须测前回 T0 独立审查、取得用户重新批准并登记新冻结版本，不得测后追认。两批仍须按 §7 完成工具/配置及符合性独立审查，再依 §6 分别请用户确认正式 AUTH；冻结不等于具备测量条件。本文不签发 AUTH，不授权连接设备、采集、运维改配置或修改服务器；0034/0037/0043/0044/0045/0046/0047 均是诊断/预演，`is_evidence=false` 的观察不能追认为 N2/N6 证据。

## 1. 一次冻结，分批执行

本次预注册范围为七行 **M 管理 TCP、S 信令 TCP、R relay TCP、D 受保护 DNS UDP、I ICE/STUN UDP、Wb WG base UDP、Ws ICE 选中后供 WG 的 UDP socket/dup**，与 **A 首连、B 断流后的重连/重试、C Wi-Fi -> 蜂窝、D 蜂窝 -> Wi-Fi、E 承载中受控 recreate、F 最终 VPN 撤销、G fail-closed 注入**的全部交叉格。§4 一次写定各格刺激和 oracle；§3、§5 的阈值、停止条件和不外推规则同时适用。R 须 opt-in 且有真实 relay 会话；Ws 只有确实选中本地 socket、交 WG 并从该 socket 发出 WG 外层包才算 exercised。实现需求要求但未实现的 TURN、relay QUIC/UDP、IPv6 外层等能力记 **`UNSAT`**，先实现并回 T0 更新预注册，不能以 `not implemented` 从门范围中扣除。实际未产生的可选单次事件记 `not exercised`，必要格据此 `UNSAT`；出现新的外层类别立即停测、回 T0，不用既有七行外推。

- 第一批 AUTH 仅执行 A/B/C/D 的 **28 格全体**，使用 §2 冻结的第 0 代 `VpnConfig` 和无 recreate 配置；相关格出现 recreate 就 `invalid/UNSAT`，保留原件，不换用新路由补做通过。
- 第二批 AUTH 另行签发，执行 E/F/G 所有七行；工具可在第一批后实现、测试并独立审查，但 **E/F/G 的判据、oracle、停止条件现在即随全稿冻结**。工具的版本/hash 与实施符合性须在第二批 AUTH 前再独立审查；任何改动判据、范围或阈值均须先回 T0、再取得用户批准并重新冻结，不得测后追认。两个 AUTH 预算和原件互不挪用。
- **第一批适用集固定为 28 格全体**，仅可在测量前经 T0 + 用户批准、在冻结登记中逐格写明豁免及依据后缩减；执行中和测后不得扣格。任何适用格 `not exercised`、`UNSAT` 或 `invalid/UNSAT` 均阻止授予“`N2 第一阶段通过`”，不得以可选事件未发生或能力未实现视作 pass。全部适用格达标、第一批工具和原件独立复核且 T0 + 用户批准后，**最多授予**该标签；这只是中间里程碑，**不是门结论，不得作为 E8/N6 或其他门的输入**，也不写 `N2 pass/reviewed-pass`。第二批全量通过及全量独立复核后才可依治理提出 N2 门结论。N6 按自身内层连通及默认路由判据另立 verdict；同轮原件可以共用，结论绝不互相代替。依[治理](native-nx-governance.md) §二.7，标签定义已随 v4 冻结在治理文末追加；授予仍须待第一批完成、独立复核和 T0 + 用户批准，不得自行创造阶段 pass。（v4.1：Wb 行的 A/B/C/D 四格已于 2026-09-28 测前逐格登记豁免。适用集为 24 格（M/S/R/D/I/Ws × A/B/C/D）；另有 Wb × A–D 4 个豁免格，其替代证据齐备且未触发任何 fail 条件，是授予标签的附加必要条件。豁免依据、替代证据与仍须 fail 的条件见文末「v4.1 追加修订记录」。）

## 2. 测前冻结、代际与覆盖

**两批管理端运维配置须由用户另行授权冻结**：测试设备无 exit node，NetBird routes 与 desired 路由集在全轮（含 E）不变；记录管理端快照、版本、route/peer 公告及差异监测，不擅自改服务器。默认路由在 peer 登记并 `tunnel_ready` 之前不会放开（`client/core/src/connector.rs:40-43`），而产品放开 `0.0.0.0/0` 必触发受控 recreate。两批均**不依赖默认路由**：建 VPN 时第 0 代 `VpnConfig` 下发非排除的定向捕获路由。测前分别在家 Wi-Fi 与蜂窝物理网络上用受保护解析/候选发现，记录 M/S（切网时 `NetBirdVpnExtensionAbility.ets:781,821` 会重新解析）、relay/home、两个 STUN、各网络全部 resolver、Wb/Ws 的 peer LAN/公网/可选 hairpin 目的及每个协议/端口；每个网络各发现一次并以并集合（重叠去重）在第 0 代为**所有候选外层 IPv4 各下发 `/32`**，不得只拿 Wi-Fi 或公网解析填表。`home.alfadb.cn` 的 Wi-Fi LAN 与蜂窝公网分裂解析须两者都覆盖；同 LAN direct peer 的 LAN host 也须覆盖。实际命中地址不在冻结并集中、候选 IPv6 无对应经预审捕获方案、resolver/peer/relay 后续变化或分裂解析发现不全，停相应格 `UNSAT`，不能测后扩路由。逐次核实实下发配置/生效路由、VPN 应用归属及目标覆盖；通常路由**不覆盖 hdc 控制端地址**，若 hdc 控制端与 peer/relay 局域网主机重合，方案 A 下 hdcd 不在 `trustedApplications` 内、不走 VPN，可允许 `/32` 覆盖该主机，但须测前在该重合拓扑下另核设备 hdcd 的真实发送/收件及不入 TUN，不可用 0047 手机浏览器的 (c) 代替；否则按保守规则不开批次。其他无法同时满足的情形同样不开批次。不可使用 `force_default_route=true`、S6 旧诊断捕获路由或端点排除来假造覆盖。

**VPN 范围已决 A**：仅在 N2 证据开关开启时将 `VpnConfig.trustedApplications=[本 App 包名]`，第 0 代及 E 第 1 代完全一致，只把本 App 流量纳入 VPN。现有 builder 未设置此字段（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:532-575`；`NetBirdVpnExtensionAbility.ets:2297-2302` 仅为 S2 注释）；仍须 T1 主机假配置测试与互斥的 `blockedApplications`/S2、S6 捕获路由拒绝组合及默认关闭行为无冲突。SDK `@ohos.net.vpnExtension.d.ts:365-376` 声明名单内应用按 `routes` 走 VPN（`@since 11`），`:371,383` 声明与 `blockedApplications` 互斥；华为答复 `docs/n2h-huawei-support-replies-20260923.md:60` 和 0031 R2 的提示性负结果均非正式证据。冻结前 0047 同一 `/32` 目的 `119.29.29.29` 的四点诊断判断均为 `established`，原件见 `/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-SMOKE-20260928-0047/evidence/observation.json` 的 `judgments`、`judgment_raw_lines` 及同目录 `observation-a.json`、`observation-b.json`，全部 `is_evidence=false`：

- (a) 未保护 App fd 五次真实发送均 timeout，`N2H_PROTECT_PROBE|mode=none|send_errno=0`；同 session 的 `N2_TUN_DEST|dst=119.29.29.29|proto=17|port=53` 由 `frames=3|delta=3` 增至 `frames=5|delta=2`，来源 `100.108.144.165`，证明未保护正控可入 TUN。
- (b) 已保护 fd 五次真实发送均 `reply=echo`，`reply_ms=90,61,86,68,56`，同窗 `N2_TUN_DEST|dst=119.29.29.29|frames=0`；回显是目标侧收到的诊断观察，正式生产事件仍须 §3 的设备、TUN、物理与独立网络侧三证。
- (c) 名单外手机浏览器打开 `http://119.29.29.29/d?dn=www.qq.com`，用户回报加载成功；提示在 `19:54:35`、回报在 `19:55:29`，TUN 计数从 `19:54:27.812|frames=0` 到 `19:55:16.045|frames=0|delta=0`。**末次样本早于用户回报约 13 秒**，不可声称回报后全窗零命中；0047 执行器未要求回报后采新样本。用户接受此诊断空档作为冻结前提，不接受它作为门证据。正式执行器必须在加载回报后取得同 session 新鲜计数样本，复测名单外实际发送/收件与正控；缺样本、时窗或归属即 `invalid/UNSAT`，不得拿 0047 填格。HDC 控制端 `192.168.50.3` 为 k8s 宿主机 SNAT，并非设备 hdcd 的独立不入隧道证明；若控制端与 peer/relay LAN 主机重合，仍须测前核实 hdcd，否则不开批次。
- (d) 原件记录 `N2_EVIDENCE_ARMED|trusted=cn.alfadb.netbird|capture32=119.29.29.29|endpointExclusions=false`、`N2_EVIDENCE_ROUTES|defaultRoutes=0|excludedRoutes=0`、`VPN_CREATE_RESOLVED|accepted=true` 且无 abort，诊断配置 create 成功、S2/S6 关闭时无冲突；S2/S6 拒绝关系仍待 T1 主机反例测试。

A 的诊断与用户确认允许**冻结方案而非签发取证结论**。§3 不论源的 TUN 全窗扫描仅针对本 App：同一 App 未保护正控仍经该代 `/32` 被捕获并在核心丢弃；名单外 hdcd 不应被路由入 VPN。**备选 B 未选、not used**：全局 VPN 下 TUN 帧无 uid，无法保持“任何源命中即 fail”。正式符合性测试若推翻 A 或新目标/固件语义不成立，只能停测；B 须经 T0 + 用户重新审查、批准并冻结替代 oracle，不得现场切换或测后追认。

M/S 的 create 前首次 TCP `connect`/发包以及 create 前其他类别的包处于无 VPN 路由阶段，TUN 零命中是平凡事实，只作时序与物理出口记录，**不计为**受路由覆盖的首包负证据。A 格必须进一步在 VPN 已 create、tee 已 attach 且第 0 代路由实际覆盖时，沿用对应 pre-create 已保护 fd 发真实第一批业务包并做夹心正控；无法做到的 A 格 `UNSAT`。凡在 attach 之前的帧不可见，不能把 create 前后两段拼作连续 TUN 捕获。B/C/D 同样须保持该代配置；任何目标路由变化、增删 peer/routes 引起的 recreate，按第一批 `UNSAT` 停格，留待第二批 E 的单独试验。

第二批 E 由**默认关闭的证据开关经已授权工具触发**承载中的受控 recreate，不修改管理端；不得依赖自然就绪后放开 `0.0.0.0/0`、变更 routes 或临场补“经预审覆盖路由”。无 exit node、desired routes 不变；第 1 代 `VpnConfig` **原样移植第 0 代全部冻结 `/32` 及选定的 `trustedApplications`**，逐代核对提交配置、实效路由和应用范围，不一致即 `UNSAT`，不得用第二代新解析修补第一代漏项。记录旧/新代配置、正控及跨代存活 fd 与新补给；补给/补池定时器若仍持有首代 `VpnConnection`，recreate 后请求全被拒绝，相关格不可测。产品前置为“**补给在 recreate 后改用当前连接**”并有主机回归；第一批的无 recreate 配置加逐次代际检测只能隔离该缺陷，**不能**算修复或 E/G 通过，若第一批出现 recreate 则 B/C/D 相关格 `UNSAT`。

每批冻结：专属 AUTH/sidecar 与用户回读确认、唯一执行者、设备脱敏 ID/型号/完整固件 SDK、App/VPN 包名、HAP/core/collector/runner/judge SHA-256、服务端版本及只读日志权限、relay opt-in、两侧物理网络句柄/接口和本地 IP、VPN/TUN 句柄、overlay IP、route generation、控制端 IP、所有实际外层目的及逐网络解析来源、fd/dup/socket ID、`getsockname` 的设备本地地址和可得 NAT 映射、UTC/单调时钟及误差上界、捕获起止/丢帧/过滤条件、原件路径与 hash。**无 exit node 时不存在可要求异于物理公网 IP 的“隧道出口公网 IP”**：此处只比较外层 socket 的设备本地物理地址与设备 overlay IP（相等立即 fail），并核设备物理接口出包、目标侧收件及 NAT 映射，不以“设备公网 IP != peer/relay 公网 IP”为门槛。家 Wi-Fi 同 LAN direct 时设备源为 LAN IP、peer 目的为 LAN IP、peer 侧源应与设备 LAN 地址/端口及实际 NAT 规则匹配；relay 与 peer 同在家网且访问 LAN 地址时也按 LAN 源判，不强求公网来源；访问家宽公网地址的 hairpin 则冻结公网目的和路由器 NAT 前后映射，peer/relay 收到的源可为家宽公网或路由器 LAN 地址，须 AP/路由器及目标侧双端证据消歧；蜂窝访问公网目的时以实测运营商 NAT 后公网源配对。设备、peer、relay 公用同一家宽公网出口本身不判 fail，若映射重叠无法唯一归属则 `invalid/UNSAT`；若真实外层发自 overlay、本应物理而走隧道则 fail。历史 `8.139.6.47`、`175.10.221.251`、`192.168.50.6` 及 overlay 值仅是线索，不预填本轮结果。

测前干净进程：经新 AUTH 授权的 force-stop 或重装后启动，记录本进程生命周期内 **未调用** `protectProcessNet`（S3 诊断开关关闭）、未设置 `blockedApplications`，`netbird-protect-deviation` 文件缺席、无 `VPN_PROTECT_DEVIATION_ARMED`，S2-S6 诊断臂全关。现有 `endpoint_exclusions_enabled` 默认 true，N2 T1 **复用此开关显式设 false**，不新增同义关闭开关；证明实际路由无 `/32`/`/128` 端点排除，initial 与第二批 recreate 两腿均检查。扩展拒绝“排除关 + S6 捕获路由”的既有组合，N2 多目标 `/32` 必须独立于 S6 新路径、默认关闭；选 A 时 `trustedApplications` 只含本 App，须主机测试验证与 S2 `blockedApplications` 互斥及 S6 拒绝关系未被绕过。0045 App 进程内 `/proc/net/{tcp,udp,tcp6,udp6}` 与 `/proc/self/net/{tcp,udp,tcp6,udp6}` 全部 `N2_PROC_NET|readable=false|code=13900012`（`/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-SMOKE-20260927-0045/evidence/observation.json` 的 `raw_rows`）；App 内读表及 `/proc/self/fd` inode 替代均 **not used**，不能谎称 App uid 可读。0046/0047 的 hdc shell 四表实读成功：0046 `tcp` 3、`udp` 18 条属 App uid，0047 `tcp` 3、`udp` 9 条属 App uid（各 `evidence/observation.json` 的 `procnet.tables`）；这是有权限的**外层 socket 清点视图**，不能单独证明进程身份或 protect。主方法冻结为分别运行 `hdc shell cat /proc/net/tcp`、`hdc shell cat /proc/net/udp`、`hdc shell cat /proc/net/tcp6`、`hdc shell cat /proc/net/udp6`；两批 AUTH 须逐条列白名单、间隔/次数/预算/脱敏规则。每轮从 `bm dump` 重新核本 App uid（0046/0047 为 `20020329`，不得跨安装沿用），按 uid 列过滤四表并保留退出码、原始样本、快照时间、PID/uid/inode/tuple 与遗漏记录；fd/dup 身份及 protect 时序仍由 App 日志与 socket 图佐证。两批定期快照全部四表，与设备登记的**全部外层 socket**（含备用/重拨/dup、IPv6）逐项对账；四表不可读、uid 未核或无法唯一对齐必要 socket，则相关格 `UNSAT`，不能静默换回已证不可读的 App 内路径。捕获路由生效前及期间，须确认所有外层 socket 已逐 fd protect，满足治理条件⑦；未知 socket、快照不可读或未保护者停测，未保护首发见 §5。清点不把非外层 socket 误判，也不把尚未分类的外层 socket 放过。

## 3. 共用证据与冻结判据

每个可操作的正向 A/B/C/D/E 格至少 **3 次独立可归属的真实生产发送事件**；A 为三个独立首连会话，B 为三个真实断流后的新 dial/查询/检查，C/D 各三次真实方向切换及新网络出包，E 为三次承载中 destroy/create 及跨代/新供给出包。F 的 App 有序、系统设置撤销两类路径各三次授权撤销及 60 s 观察窗；G 每个指定故障分支至少一次且证明注入确实生效。三次是最小观察数，**不作统计安全保证**。一次缺件不能用追加好样本掩盖；只在授权规定的传输中断例外中保留两次原件与预算并重跑一次。每事件均须有以下三证及正控：

1. **设备侧**逐 fd/dup 身份链 `open -> protect_begin -> settle(success) -> 首次 connect/UDP send`，后续发送、网络 generation、当前物理句柄和 `getsockname` 的本地 IP:port；本地 IP 等于 overlay IP **直接 fail**，不能拿服务端来源来洗掉。失败后绝不裸发。TCP 已建连接可跨多次请求/出包，但不能把状态行计作独立发包。
2. **本机 TUN** 在 WG 设备 `service_tun` 读循环（`client/core/src/wg_device.rs` 约 997-1011）的 from-OS/to-OS tee 保留非空扫描、原始帧、丢帧/解码计数与逐帧判定；该挂点在 attach 前后有盲区（同文件约 355-356）。对每个**生产事件紧前和紧后各发一个**正控 marker，两个都须在 from-OS 命中，时间顺序为 `前控命中 -> 生产包 -> 后控命中`，构成仅对该事件的可观察夹心；另每格至少三个可归属的正控 marker 且 **3/3 命中**（逐事件夹心的六个或更多 marker 全须命中）。无前/后控、窗口空白、帧丢失/解码失败均为 `invalid/UNSAT`，不能外推其间未知帧。冻结**从 VPN create 且 tee attach 到 destroy 的全 campaign 可观测窗口**的外层目的 IP:port 集合及版本：在 §2 的 A 方案 `trustedApplications=[本 App]` 已生效前提下，对该窗口全部 from-OS 帧按目的 IP:port 匹配，**不论源地址或设备临时源端口**，只要落在冻结外层集合就是生产泄漏命中；若未限为本 App，系统和其他应用流量无法从 TUN 帧按 uid 剔除，不得沿用这一 fail 判据。正控帧单独标识并由 collector 在核心吞掉，不计作生产泄漏。整个可观测窗口持续扫描，包括格与格之间，任何非正控的命中均立即 `fail/leak`，不能靠逐 fd 五元组筛掉未登记流量。create 前和 destroy 后不声称 tee 连续，另以设备 fd/物理侧记录判断；若正控本身流出物理口或未被核心丢弃则立刻停测并记 `invalid/UNSAT`/风险事件。
3. **物理出口 + 独立网络侧**：设备物理接口的发送元数据与实际目标侧/AP/peer 抓包或服务端可核对入口闭合，按冻结的时间误差、目标、连接区间、请求顺序及**当轮确认实际可见时**的 transaction ID 和 NAT 后来源消歧；**D 的独立 resolver 不可观测时仅适用 §4 明确冻结的无 exit node、受保护 fd 收到同 ID 响应替代 oracle**。纯 `protect` resolve、应用成功、TUN 零计数、物理 NIC 总字节或单方服务器日志均不足以证明同一事件。物理口受限须记能力缺口并由独立 AP/路由器镜像等补齐（D 的替代 oracle 除外）；不能唯一配对则 `invalid/UNSAT`。Wi-Fi 格优先申请家中路由器/AP 抓包；蜂窝另核运营商实际公网来源。

正控由同一 App 进程、同一代 `VpnConfig`、同一当前网络的隔离 socket 产生，只比生产 socket **少一次 protect**；TCP 调用与 M/S/R 生产相同的 `mgmtSocketOpen`，UDP 与 D/I/Wb/Ws 生产相同的 `iceSocketOpen`（若某类的实际生产 opener 不同，须先证明开口等价并经独立审查，不能假称已复用），不绑定 net handle。预注册 marker ID/目标 IP:port/协议、socket ID 和预期 tuple **先于发送**；UDP 用非敏感 nonce，TCP 可用单独连接 SYN 的 tuple/事件 ID 作 marker，不向认证服务写测试载荷。目标为冻结外层地址且确受路由覆盖；collector 在 WG 处理前匹配、记录并丢弃 from-OS 正控，**不要求远端独立接收器或受控测试端口，更不要求到达远端**。正控出口被放行而有外发风险时立即停测；注册/丢弃先后和生产隔离须主机反例测试。DNS 正控亦只验证同代同路由的捕获能力，DNS 响应另按 §4 的替代 oracle。

阈值为正向格 **生产 3/3 设备、TUN、物理+独立网络三证齐全，整个 campaign 生产外层 TUN 零命中，每个事件夹心正控前后均命中**；任一生产外层入 TUN 或未保护首发即 fail，不因三次干净事件抵消。时间配对容差冻结为校准后 **±2000 ms**：以设备/主机/网络侧 UTC 与单调时钟映射实测误差上界、日志时间戳粒度和 NAT/落盘延迟校核；若任一端合成上界超过 2000 ms、出现多候选或时钟跳变，停止相关格 `invalid/UNSAT` 并回审，**不测后放宽**。此数值只是候选事件检索窗，不因落窗就自动证明唯一归属；断流结束的落盘日志须用 `ts - duration` 反推开始。DNS 的独立物理侧替代判据见 §4，不伪称拥有 resolver 日志。

## 4. 七行 × 全部列的刺激与 oracle

每格都叠加 §2 的配置/清点及 §3 的判据，G 还叠加 §5 的 fail-closed 列表。`not exercised` 必须有原因，不能记 pass。E 须同时观测旧存活 socket 跨代和按需**新** socket/ICE 补池；F 同时覆盖 App 有序停止/关 fd 后 `destroy` 与用户从系统设置撤销 VPN 触发 `onDestroy` 的路径，后一路**不得假定先停 sender 再撤销**；撤销后无 TUN 时以 fd/物理口/独立网络侧 60 s 短窗证明无旧 socket 外发，不能用 TUN 零命中充数。下面“同向”仍分别采样、独立裁决；表内 F 的“停/关”描述只对应 App 有序路径，系统设置撤销路径须对同一行实测 `onDestroy` 后实际关闭顺序和无旧包。

| 行 / 独立网络侧 oracle | A 首连 | B 断流/重试 | C Wi-Fi -> 蜂窝 | D 蜂窝 -> Wi-Fi | E recreate | F 撤销 | G 故障注入 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **M TCP**；netcenter Caddy `access-api.log` 的管理入口 | 初次 dial；pre-create fd 在 create 后继续真实出包 | 已授权独立断流后三次新 `purpose=management` dial，含队列耗尽补给与 Login/Sync | 新物理源上的实际管理发送/入口 | 同向反转 | 存活管理连接与新补给，当前 connection protect | 停 Sync/补给、关 fd，撤销后无旧管理包 | 初始保护失败拒 create/裸拨；补给失败和代际拒绝后无裸拨 |
| **S TCP**；netcenter Caddy `access-signal.log` | 初次 feed/create 后继续发包 | 真实 signal 流断后新 socket/重注册 | 新网络 signal 流/入口 | 同向反转 | 存活流及新 signal feed/注册 | 停流、关 fd，撤销后无旧 signal 包 | 初始 feed 失败停 connector；重拨保护失败无裸连 |
| **R TCP**；home Caddy `/apps/netbird/data/caddy/access.log` 中 `/relay`，真实 opt-in | 真实 TLS/WSS 会话、create 后发包 | 真实 relay 断流后新 `purpose=relay` socket/会话 | 新网络 relay 会话/入口 | 同向反转 | 旧会话收尾与新受保护 relay | 关会话/fd，撤销后无旧 relay 包 | relay 补给/保护失败无裸 TLS/WSS |
| **D DNS UDP**；已保护 fd 收到匹配响应 | 三次 relay/STUN 新查询；初始系统解析另盘点 | 新查询/新 fd、响应，缓存命中不算 | 蜂窝 resolver 重发现并与冻结值核对后查询 | Wi-Fi resolver 新查询 | 当前连接上重新获取受保护 DNS fd/新查询，不假定旧 fd 常驻 | 停查询者、关 fd，撤销后无旧 fd 查询 | 无 resolver/物理句柄/保护失败时拒查询及系统解析回退 |
| **I ICE/STUN UDP**；实际目标侧 coturn `New UDP endpoint`/`BINDING` | 两个 STUN 目标逐一登记是否实际发送，三次 Binding/check | 新检查/补池 fd 与 transaction ID，未实际发包为 UNSAT | 新网络候选、映射、响应 | 同向反转 | 旧 ICE socket 与当前连接新补池、检查 | 停 ICE/关池，撤销后无旧 STUN 包 | 池单 fd 失败只减候选，全池失败拒 gather/裸发 |
| **Wb WG base UDP**；真实 peer 自有主机外侧 UDP 抓包（v4:64；v4.1 变更 2 把本行 **A–D 四格**的 vantage 改为宿主 `192.168.50.3` 限时只读抓包，见 §4「Wb/Ws 独立网络侧 oracle（v4.1）」段；**E/F/G 的判据与 oracle 文字按 v4 原文不动，其 vantage 是否同为宿主须在第二批 AUTH 前另行确认**） | **豁免登记格（替代证据）**：`wg_fwd_open` 原 fd/dup 的 open→protect_settle(success) 身份链齐备、protect 完成时刻早于 tee attach 锚点、全 fd 生命周期零发送（替代证据 E1–E6）；**不要求**真实出包 | 同左；断链刺激后 base fd 仍零发送 | 同左；新网络下 base fd 仍零发送 | 同左；跨方向反转 base fd 仍零发送 | 存活 base fd 与必要的新受保护 fd | 停 WG 发送/关 fd，撤销后无旧 WG 包 | base 保护/feed 失败销毁 VPN、无裸 WG |
| **Ws WG selected UDP**；同一 peer 外侧 UDP 抓包 + 选中到 WG dup 身份图 | 选中并由该 socket 发 WG 包 | 真实重选/重连后选中 fd 出包 | 新网络选中 fd 的 WG 包 | 同向反转 | 选中旧 fd/新选中 fd 跨代、当前连接重保护 | 停选中 socket/WG 并关 dup，撤销后无旧 WG 包 | 选中 fd/dup 保护或关闭失败后不交 WG、无裸发 |

**B 列刺激先冻结再授权**：默认 M/S/R 用证据开关由核心仅主动关闭该类已登记 fd，保留 close 时刻和新 fd 身份；这是**本地关闭/报错到重拨**路径，只证明该路径，**不外推**网络侧 RST、半开、黑洞和超时。需要网络侧断流证据时可另选用户授权的 AP 侧仅对该流目的定向阻断/恢复并留双侧原件；I/W 用同等受控断链/端点阻断触发重检/握手，D 用已授权的新查询/超时重试。不可用服务端断连（无服务器写授权）、不可关 Wi-Fi 冒充 B（与 C/D 混淆），也不可将 App 重启当新拨或跨代。刺激影响范围、恢复步骤、次数写入各批 AUTH；不可复现或不足三次则格 `UNSAT`。

**独立 oracle 细则**：M/S 在 netcenter（历史公网 `8.139.6.47`，当轮核实），R 在 home（历史 `rels://home.alfadb.cn:28443`，不是云端备用 relay），按实际发生的远端/日志逐连接对齐。HTTP/2 多请求可能共用**同一** `remote_port`/连接，不能把端口相等当多次新 socket；gRPC 长流与 WebSocket 在结束落盘时用 `ts - duration` 反推连接起点。测试窗口内其他同出口 NetBird 客户端须停止或逐个登记并可排除；做不到而同源多候选重叠，S 等格 `UNSAT`。设备本地端口不必等于 NAT 后 `remote_port`；家宽保留源端口只可在**当轮实测确认**后作附加配对，不作为默认假设。服务器日志只取白名单元数据，不能导出 headers/认证载荷；两侧 coturn 有空间轮转且扫描噪声，缺失/歧义即 UNSAT。

I 的设备侧须记录 STUN response 的 `XOR-MAPPED-ADDRESS`；先凭当轮确认可用的目的、时间、socket/transaction ID 与两端原件唯一归属 coturn `remote addr IP:port`。**coturn 默认日志通常没有 transaction ID**，测前须实际确认该字段存在才使用；不存在时不得称可按 transaction ID 跨端核对，只能结合响应侧 ID、目的、时间窗及其他独立元数据唯一配对，否则 `invalid/UNSAT`。唯一配对后两地址**逐字不相等即 `UNSAT` 并登记风险事件**（保留两端原件，不记 pass/fail 泄漏）；相等也不能只靠 `BINDING` 数量断言同一事件。Ws 若沿用同一选中 socket，可继承其已证映射，但仍须 WG peer 侧独立收件。D 在**全轮无 exit node**前提下使用明确替代判据：受保护 DNS fd 记录发出查询并收到同 ID 响应、该事件前后正控 from-OS 命中、campaign TUN 零泄漏，结合物理网络句柄和实际 resolver 目的；不强求独立 resolver 回执，E 不允许放开默认路由替换此前提。Wi-Fi 若 `192.168.50.6` 确是用户可控解析器，优先另取其查询日志强化；蜂窝 resolver 须测前发现/冻结，缓存无查询为 `not exercised`。

**Wb/Ws 独立网络侧 oracle（v4.1）**：Wb/Ws 的独立网络侧 oracle 地点改为 **k8s 宿主机 `192.168.50.3`（peer 所在 Pod 的 SNAT/DNAT 点；用户 `alfadb`）限时只读抓包**（`tcpdump`；**抓包权限是宿主机上的持久特权变更，见下「权限」；本稿不授权、不执行**）。

**过滤条件（锚定 peer 侧，固定，不得现场放宽）**：`udp and dst host <节点 IP 或 Pod IP> and dst port in {冻结的 --wg-port, 冻结的 --ice-port}`。**只按目的（peer 侧）锚定**：删除一切按设备源/蜂窝源过滤，也删除 ICE/STUN `portrange` 形式；两个端口取**冻结值**（`nbinterop --wg-port/--ice-port` 或 DNAT/hostPort 的等价冻结映射，见下「测前只读冻结」）。IPv6 无经预审捕获方案则 Wb/Ws 对应格 `UNSAT`。过滤器只覆盖设备↔peer 的外层 UDP；hdc 控制端（TCP）、relay/coturn 流量与 **conntrack 表读取**均**不在本授权内**，不得采集。

**两个抓包点（NAT 前后各一）**：① 上联/`vmbr0`（**DNAT 前**，目的为节点 `192.168.50.3` 的 hostPort/DNAT 端口）；② Pod 侧 `veth`/`cni`（**DNAT 后**，目的为 Pod `10.98.0.142` 的外层端口）。两点必须同时开、同一冻结过滤器与同一时间窗，分别落原件；只在单点看到或两点时间/五元组无法对齐即不构成覆盖证明。**不读 `conntrack`**：NAT 前后映射只以两点 pcap + 冻结的 DNAT/hostPort 规则与设备侧 `N2_SOCK` 对齐，不调用 `conntrack -L`/`ss` 等额外通道。

**测前只读冻结（缺一即相关格 `UNSAT`，不得测后补）**：Pod 固定在节点 `192.168.50.3`（`10.98.0.142` 的 SNAT/DNAT 归属须实测确认）；`--wg-port`/`--ice-port` 冻结值与节点的 DNAT/hostPort 规则（或路由器的端口映射、或 peer-reflexive 反射地址路径）**实测**并逐条留证；**蜂窝下入站流量必须只经此节点**到达 peer（若蜂窝路径存在不经该节点的旁路，则蜂窝格 `UNSAT`）。直连 P2P 可达性按 v4:64 测前确认；只有 relay 路径则 Ws 的直达 peer 收件格 `not exercised`（Wb 四格按 v4.1 豁免登记裁决）。

**snaplen**：`-s 96`。理由：链路层 14 + IPv4 20 + UDP 8 = 42 字节头部，余 54 字节载荷足以覆盖 **STUN 头 20 字节**（消息类型/长度 + 4 字节 magic cookie + 96 位 transaction ID，可与 coturn 侧按 ID 配对）与 **WG 消息类型判读所需的首字节**（1 字节类型 + 3 字节保留），并为单个 802.1Q tag（4 字节）与 IPv4 选项留余量；96 字节**不落任何 DNS 内容、凭据或完整加密载荷**（超出部分由内核截断丢弃）。snaplen 不得现场放宽。

**类型分流（同一五元组）**：同一五元组上先有 ICE check（STUN）后有 WG 握手/数据，故原件须**按类型分流**——用 STUN magic cookie `0x2112A442`（载荷偏移 4–8）识别 STUN、用载荷首字节消息类型（1/2/4）识别 WG；**只配对首包**（每类第一次出现的包），后续包仅计数与登记，不参与唯一配对。同一五元组上同时出现两类首包而无法按冻结目的/时间区分即 `invalid/UNSAT`。

**权限（宿主持久特权变更，如实登记）**：2026-09-28 晚用户以 `sudo setcap cap_net_raw,cap_net_admin=eip /usr/bin/tcpdump` 授予（主会话已核 `getcap`）；**授予时刻、命令原文、`getcap` 回读结果**入当批 AUTH 证据。撤销方式为 **campaign 结束后** `sudo setcap -r /usr/bin/tcpdump`；**撤销时刻与 `getcap` 空结果**同样入证据。**这是宿主机上的持久权限变更**，不是设备授权；本稿不授权、不执行授予/撤销，也不 SSH。

**限时**：抓包起止必须覆盖「create+tee attach → destroy」全 campaign 窗口**含格与格之间**（v4:39），并登记丢帧计数与采样缺口；抓包命令列入该批 AUTH 的操作员白名单（argv 逐条、间隔/次数/预算）。

**限量**：`-c` 包数上限与总预算写入 AUTH；超出预算即停并如实登记，不得为「抓全」临时放宽。

**脱敏**：只取白名单元数据（源/目的 IP:port、协议、长度、时刻、NAT 前后映射、消息类型）；不进凭据、JWT、setup key、证书/密钥或敏感载荷（v4:78）；宿主侧不得读取 hdc/relay 内容。

**原件与删除**：两个抓包点各留 pcap 原件，存放路径、`sha256`（现算并 sidecar）、过滤条件原文、snaplen、丢帧/缺口报告、起止时刻、执行者与授权编号入当批 AUTH 证据；**原件按当批 AUTH 规定的保留期保存，期满用授权白名单内的删除命令删除并登记删除时刻与删除后复核**（不得在 campaign 中途删除，不得留无 hash 的副本）。

**与设备 `N2_SOCK` 配对规则**：Ws 生产包以设备 `N2_SOCK|kind=Ws|fd|dup_fd|local|remote` 加发送回执（`detail=wg_sendto`；`first_mono_ms`/`first_wall_ms` 是完成时刻而非线上时刻）为设备侧锚点，经宿主侧记录的 NAT 前后映射（设备 LAN `192.168.50.199` 或当轮蜂窝实测源 → 宿主 `192.168.50.3` → Pod `10.98.0.142`/映射端口）按目的 IP:port、协议、源端口与 v4:44 的 **±2000 ms** 唯一配对；两端时钟校准与误差上界入证据，超界/多候选/时钟跳变按 v4:44 停格 `UNSAT`。同 LAN direct 取设备 LAN 源或实测 LAN NAT 源、家宽公网 hairpin 依冻结映射消歧、蜂窝以当轮实测运营商 NAT 后公网源配对（**不预填** v4:113 的 `223.153.208.166`、`8.139.6.47`、`175.10.221.251`、`192.168.50.6`）。**Wb 的裁决规则**：LAN 直连下用 **Wb 注册 tuple 零命中**（同会话/同过滤器/同窗内先命中 Ws 生产包，证明 vantage 非盲）；**蜂窝下改用 E5 的来源闭合**（每一条来自设备到 peer 冻结端口的流，其 NAT 后源地址须能归到已登记的 I/Ws 映射，归不上即 fail 候选并回 T0）；整窗零包或有覆盖缺口 → Wb 替代证据不成立（`UNSAT`）。

**语义差异必须如实登记**：宿主是 peer 的 NAT 点，不是设备物理接口；「设备物理接口发送元数据」仍按 v4:40 由设备侧独立留存，宿主抓包承担「独立网络侧」角色。**peer 应用层 `recvfrom` 回执按 Q2（见文末「v4.1 追加修订记录」第 4 项）只作辅助证据（应用层，非独立网络侧 oracle）**，不替代宿主抓包、不补救抓包缺失的格。

**A 列生产事件的操作性定义（v4.1 澄清，不改判据）**

**共同锚点（全行适用）**：每个 A 事件的**夹心前控最早时刻** = TUN tee **attach 锚点之后**，即 `N2_TUN_GAP|reason=before-tun-attach`（`wg_device.rs:2269-2271`）或 `N7_WG_FEED|adopted`（`:2303`）之后，且 `VPN_WG_DATAPLANE_FED|deviceUp=true`（`ets:3493`）之后；前控必须以 `N2_CONTROL_HIT` 命中为准。attach 之前 tee 未挂载，`n2_control_tee()` 在 device 未起时返回 `None`（`wg_device.rs:2119-2123`）、`n2_control_send_json` 回 `not-armed-or-no-tun`（`connector.rs:5207-5215`），且 `n2_tun_dest_configure` 只创建 tee 与目标集合、**不挂载**（`wg_device.rs:2102-2117`）；因此 attach 前的前控不可能命中。v4:26 已明言 attach 之前的帧不可见，本澄清只是把「最早可夹时刻」写成可判定的锚点，不新增门槛。

**触发须走生产路径**：每次 A 事件必须是同一 App 进程、同一代 `VpnConfig`、同一当前网络下，**沿用 pre-create 已保护的生产 fd/dup**（同 `mgmtsock::connect_protected`/`iceSocketOpen` 开口等价）发出的一次真实生产发送；不得新开未保护 socket，不得用 `n2_stimulus` 的重拨路径（那是 B 列，v4:60），不得用正控 marker 充当生产事件。触发通道默认关闭（v4:32），仅在对应格 AUTH 白名单内开启。

**通用原则**：**触发只能改变发送时刻，不能造出生产客户端本就不会发的报文种类**。每行的触发发送都必须是生产代码本来就会发的**真实 RPC/帧**、走**同一 fd**、**无测试载荷**、**无服务端状态副作用**（不写认证服务、不替换既有流、不触发服务端重推网络图或 recreate）。

**计数口径**：v4:36 的「A 为三个独立首连会话」按**会话**计数——每个独立首连会话只计 **1 次** A 事件；**不得**用同一会话内的两个 STUN 目标（I 行）或两个方向凑两次事件；3 次必须来自 3 个独立首连会话。

| 行 | A 事件的「事件是什么」 | 触发方式（如何造出这一次真实发送） | 标注 |
| --- | --- | --- | --- |
| **M** TCP | create+attach 后，**同一 pre-create 已保护 M fd/dup** 上的首个真实管理 RPC 发送（非裸 TCP ACK） | 证据开关下、前控 HIT 之后，在同一 live M 连接上发起一次**只读幂等**的真实管理 RPC（**例如 `GetServerKey`**，`grpc.rs:430`，登录流程本就会调用、无服务端状态副作用；同一 TCP 连接即同一 fd）。**不得「再开一条 Sync 流」**——服务端可能替换原流/重推网络图，有 recreate 风险（r2） | **澄清，不改判据** |
| **S** TCP | create+attach 后，**同一 pre-create 已保护 S fd/dup** 上的首个真实 signal 出站帧 | **核对 signal 协议后（r2）**：本实现里 signal 出站帧只有两类——(a) 对特定 peer 的 `OFFER/ANSWER/CANDIDATE`（`signal.rs:130`，会有**对端状态副作用**），(b) **自寻址 HEARTBEAT**（`signal.rs:533` `send_heartbeat`，上游 `sendReceiveProbe` 的对等物，服务端直回发送者，无对端/服务端状态副作用）。**本仓库尚未接线**（`:70-77` 记 30 s 接收看门狗属 N5 增量，原语存在但无调用者），故：(i) 若测量前实现该看门狗，则用自然发生的自寻址 HEARTBEAT 作 S(A) 生产事件（前控 HIT 后等待其发出）；(ii) 否则等**服务端驱动**的下一次需要应答的入站帧，用生产客户端本就会发的应答作事件；(iii) 两者都不可得 → **S(A) `UNSAT`**，不得现场造帧 | **澄清，不改判据** |
| **R** TCP | create+attach 后，**同一真实 relay TLS/WSS 会话**上的首个真实外层发送 | **r2 更正**：生产代码**从不主动发 HealthCheck**（客户端只在收到服务端 `HealthCheck` 时回一个 `Frame`，`relay_client.rs:2117-2120,2357`），故「客户端主动发 HealthCheck」是**诊断构造、不得使用**。改为：前控 HIT 后**等待服务端驱动的下一次 HealthCheck 应答**作为生产事件，应答发出后再发后控。**等待上限写入 AUTH（建议 ≤ 90 s，须 > 服务端心跳周期）**；超时未观测到即该事件 `UNSAT`（不得改判、不得自己造帧）。不得用 `n2_stimulus kind=R`（`shutdown`+重拨属 B，switch 文档:114） | **澄清，不改判据** |
| **D** UDP | **每个独立首连会话**里 attach 之后**第一次**受保护 DNS 查询（`Purpose::Dns` fd），并记录发出查询与**同 ID 响应**（`protected_dns.rs:151-168` + 工作区 `N2_DNS\|query_id=…\|response=matched` 回执） | **Q1 口径（T0 已裁定，见文末「v4.1 追加修订记录」第 4 项）**：优先取**自然发生**的那次查询；没有自然发生时，才用 requery 原语在该会话内**触发一次**（`n2_stimulus kind=D` 的 `force_protected_dns_requery`）；**该会话此前不得有任何刺激**；3 次须来自 3 个会话。B(D) = 同一会话中先有成功查询、再经已授权扰动后的新查询/新 fd；**原语两列共用但事件不跨列**，每条记录写明会话与前置刺激 | **澄清，不改判据**（口径已由 T0 裁定） |
| **I** UDP | attach 后 ICE 池在冻结接口 fd 上的一次真实 Binding（`detail=stun_txn_<hex>` + `XOR-MAPPED-ADDRESS`） | **App 侧夹心编排（最省）**：在 `settleCreate` 的 `protectAndFeedIceSocket()`（`ets:3381,3564-3588`）处用 `withN2Sandwich` 包住 ICE 池馈入——attach（`ets:3350`）已在其之前，时序天然成立；**两个冻结 STUN 目标仍按 v4:56 逐一登记是否实际发送**，但**只计 1 次 A 事件**（不得凑两次，见上文计数口径） | **澄清，不改判据**（只补实现） |
| **Ws** UDP | ICE 选中并把本地 socket 的 dup 交 WG 后，**从该 dup 实际发出的首个 WG 外层包**（`detail=wg_sendto`；类别由 `n2_socket_trace.rs:58-66` 从 I 改判为 Ws） | 证据开关下在 egress 挂接/首包时机做夹心编排（例如把 `apply_endpoint` 延后到前控 HIT 之后），或提供「在 live 选中路径上发一次真实 WG keepalive/handshake」的主机触发原语。**r2 约束**：延后 `apply_endpoint` 时**必须保持 `peer_conn.rs:1230-1239`「先挂 egress 再落 endpoint」的顺序**（先 `attach_egress_socket` 成功、再落 endpoint），**不得**造出「有 endpoint 无 egress」——那正是本记录第 1 项例外的失败形态；与 P1 失败关闭小修一致 | **澄清，不改判据**（只补实现） |
| **Wb** UDP | —— | 不适用：见文末「v4.1 追加修订记录」豁免登记（**不生产事件**） | —— |

**配套实现（不改判据；须 T7a 独立符合性审查，全部默认关；对应文末「v4.1 追加修订记录」前置项 P1–P8）**：

1. 产品：为 M/S/R 提供「证据开关下、在 attach 之后由执行者触发一次真实生产发送」的原语（M 用 `GetServerKey` 类只读幂等 RPC；S 见上表 (i)/(ii)/(iii)；R 只等服务端驱动的 HealthCheck 应答）；I/Ws 用 App 侧编排；D 按 Q1 口径实现（可行性报告 §5 第 2 项；前置项 P4/P5/P8）。
2. 产品：为已连接 TCP fd 补 `phase=subsequent_io`（写完成）回执（现 `n2_socket_trace.rs:121` 对非 I/Wb/Ws 用完即删；`mgmtsock.rs:355-380` 只记 `first_io`），并在 gRPC/signal/relay 写路径挂钩——否则 §3 第 1 条对 A 列永远缺「后续发送」证据（v4:38）。**前置项 P3**。
3. 产品：E3 **按 fd** 的 sendto 计数（现有 `WgDeviceStats` 为全局）与 E2 覆盖链的 T7a 复核（见「四格豁免登记表」与「替代证据 E1–E6」）。**前置项 P2**。
4. 执行器：`run_campaign.py` 的 A 列必须先等 attach 锚点再发前控（现 `:786-795` 后 `:829` 立即发前控，VPN 尚未 create/attach，必然 `not-armed-or-no-tun` → 按 §3 记 `UNSAT`）；`production()`（`:819`）的「observed from the armed session」换成真实触发调用并记录事件 id。**前置项 P6**。
5. 判定器：登记 A 的 attach 锚点与事件 id，避免把 attach 前的平凡区窗口当 A 的观测窗；并登记 D 的会话归属与前置刺激（Q1）。
6. AUTH 模板：`trigger_surface.positive_control.host_triggerable`、`production.A.host_triggerable` 按落地结果更新，且须独立验证后才置真。

## 5. 第二批 E/F/G 冻结注入与停测规则

G 按可触达类别逐一注入：`protect` reject、壳 5 s timeout、late resolve/陈旧 request ID、核心 supply 8 s timeout、物理 DNS/网络句柄缺失、fd close/dup 失败；可实现且预审的“resolve 成功但平台生效延迟”单列，确有未保护包即 fail 并回 T0。每个指定分支至少一次保留**故障确实触发**的状态、fd/dup/代际顺序与前后夹心正控及全程物理/TUN 原件；没有外层包的注入以当前覆盖路由下夹心正控与 fd/物理侧短窗证明无裸发，不以错误日志代替。失败 fd 不复用，恢复必须新建并重新 protect；跨 recreate 所有补给/补池使用**当前** `VpnConnection`，过渡代请求拒绝，旧 generation 不继续裸发。若某注入使 VPN/tee 不存在，预先的保护与路由有效窗和物理/独立侧无包、关闭顺序作为该子格的负证据，不能声称不存在的 TUN 正控；不能观测则 `UNSAT`。

F 各行均须覆盖两类授权撤销，各类**三次独立事件**：一类由 App 停 connector、ICE、WG 和补给发送者 -> 确认 fd/dup 全关闭 -> `destroy`；另一类由用户从系统设置撤销 VPN、走 `onDestroy`（`NetBirdVpnExtensionAbility.ets:3678-3700`），系统撤销与异步关闭顺序**不受 App 编排**，不得将其改写为有序路径或事后补停充数。两类每次均自撤销事件起固定 **60 s** 记录旧 fd 状态、物理口与独立网络侧；撤销后仍有旧包即 `fail`，窗口不连续、关闭次序不可核或接收侧无可归属记录即 `UNSAT`。恢复/重连在新授权范围内重新创建和 protect，不能拿撤销后天然无 TUN 证明无泄漏。E 的旧/新代路由覆盖、正控与 socket 图分别记录，任何中途不可见的帧不能跨窗外推。

发现任何真实生产外层包匹配冻结目的集合进入 from-OS TUN、未保护首发/保护失败后裸发、设备 `getsockname` 为 overlay，立即保存原件、`fail` 并停止本轮；同一格后续干净事件不抵消。缺 AUTH、用户确认、管理端冻结、干净进程、路由/hdc 隔离、socket 清点、夹心正控、实际出包、物理/网络侧 oracle、时钟或 tee 连续性，标 `invalid/UNSAT` 并停受影响格；泄漏风险、未知外层能力或代际失效可疑则扩大为全轮停测并交 T0/用户。不得用 N2-H 端点排除、`protectProcessNet`、VPN 业务成功或 N6 内层流量替代逐 fd protect；Wi-Fi 不外推蜂窝，第一批不外推 E/F/G，三次零命中不证明未来永不泄漏。本轮证据取自 `trustedApplications=[本 App]` 加定向 `/32` 的证据专用配置，与不设 `trustedApplications`、最终放开默认路由的产品配置不同；结论不外推至产品全局 VPN/默认路由配置，该部分由 N6 与条件⑦另行覆盖。

## 6. 两份 AUTH、制品与冻结流程

全稿 v4 判据与标签定义已完成测前独立审查、用户批准及治理登记，**两批均未获执行许可**。第一批 T0–T6a 工具/配置还须 T7a 独立符合性审查，第二批工具 T5b/T6b 后须 T7b 独立符合性审查；各批分别签发 AUTH 并经用户回读确认后才能执行。每批独立的 campaign 类 AUTH 必须逐项写设备/无线目标当轮核对、HDC argv 白名单（含在线查询、最多 10 次有间隔 `tconn`、只读设备时钟、经批准的 `/proc/net` 四表快照、`bm dump` App uid 当轮复核及 force-stop/重装若选用）、产物与 sidecar/hash、管理端配置证据、逐格允许刺激与切网/注入/撤销、采样命令、时间窗、预算上限和跨尝试累计、单一执行者、回滚、日志最小化与停止码。AUTH 签发后主会话回读并取用户**明确确认**，确认前不得发设备命令，连 `hdc tconn` 也不行。新 AUTH 适用治理 2026-09-27 追加登记：无固定 `valid_from/valid_until`，离线最多 10 次连接后暂停且不自动消费，传输中断仅该格最多重跑一次并留两份原件，结果不理想不能重跑；连接后核设备/主机时钟差 ≤60 s、每 15 分钟重比对，设备锁定/时间不可读/大版本变化或 hash 失配即停；跨会话续用重新回读确认、核时与 hash。第二批另做工具符合性审查、签新 AUTH，绝不沿用第一批授权。未经额外授权不登录服务器、不改日志配置、不抓 AP/peer 或读取原始敏感日志。

每格保留原始 TUN/物理/AP/peer/服务端白名单日志、socket 清点与时间快照、配置与代际变化、正控/生产对应表、过滤器和丢帧报告、sha256 清单、无效尝试和恢复记录。凭据、JWT、setup key、证书/密钥与敏感载荷不入日志；日志不足不补造。第一批 verdict 与第二批 verdict 独立归档，判定器禁止中途写 N2 门 pass。

## 7. 实施任务清单（未来工作；本次不改代码）

估算为工程人日，不含审批、真机、运维、等待或新能力开发；全部诊断开关、路由、tee、注入、网络命令**默认关闭**，主机验收用假设备和离线原件。`client/entry/selfcheck/` 有并行执行者，后续实施另行协调，本次不碰 `client/`。

| 顺序 / 规模 | 依赖与交付（主机验收） |
| --- | --- |
| **T0 预注册与运维前提 / 1-2** | 全稿 v4 已冻结；两批另取得管理端无 exit node、routes 不变的授权与快照，分别在 Wi-Fi/蜂窝受保护解析 M/S/relay、全部 resolver/STUN 和 peer LAN/公网/hairpin 目的，提前核 hdc 地址与 direct P2P。0045–0047 的方案 A 四点及 hdc 四表可读性仅作为冻结前非证据前提；0047 (c) 约 13 s 空档须修正执行器并于正式取证重测，若控制端与 peer/relay LAN 主机重合仍需额外核 hdcd；未获权限绝不代操作。 |
| **T1 第 0 代捕获 / 3-5** | 复用 `endpoint_exclusions_enabled=false`，N2 专属默认关多目标 `/32` 路径及 `trustedApplications=[本 App]` 证据开关（默认关，独立于 S6）；通常冻结 route 不覆盖 hdc，hdc 与 peer/relay LAN 主机重合时仅方案 A 且该拓扑下 hdcd 单独实测不入 TUN 才可覆盖该主机，否则不开批；管理端不变+无 recreate 监测。假 `VpnConfig` 测两网所有目的并集、排除关闭、S2 `blockedApplications` 与 trusted 互斥、S6 冲突仍拒绝、目标变化/重建即 UNSAT、默认行为不变；主机测试与 0047 四点非证据诊断是不同层次；(c) 回报后空档须修正执行器、正式复测，失败即停测回 T0 + 用户重审 B，否则不切换。T2/T3 前置。 |
| **T2 socket 库存与当前连接 / 4-6** | 设备逐 fd protect/dup/`getsockname`/外层标识，按两批 AUTH 白名单周期执行 hdc shell `/proc/net/{tcp,udp,tcp6,udp6}` 四表，逐轮 `bm dump` 复核 App uid 并按 uid 过滤，对账 fd/dup 日志；App 内 `/proc/net` 与 `/proc/self/net` 已证不可读，不作后备。四表不可读或必要 socket 无法对齐则 UNSAT。修复“补给在 recreate 后改用当前连接”并测旧代拒绝。假 socket/fd 复用、IPv6/漏登记、overlay 本地 IP、旧代定时器、条件⑦无漏保护；第一批仅无 recreate 不可免 T2 的库存。T4/E 前置。 |
| **T3 tee 与夹心正控 / 4-6** | 基于 `wg_device.rs` service_tun tee 做全 campaign 目的 IP:port from-OS 扫描/丢帧/盲区报告；同 opener 的隔离未保护 TCP/UDP 正控，不绑定 net handle，注册先发并在核心吞掉。假包测任意源命中即 fail、每事件前后控、正控 3/3、TUN 空窗/缺帧 UNSAT、误外发停测；不建接收器/测试端口。依赖 T1。 |
| **T4 设备/网络 oracle / 5-8** | M/S/R、DNS ID+响应、STUN `XOR-MAPPED-ADDRESS`、Wb/Ws fd/dup/peer 侧发送身份与时钟日志，主机解析已有服务端白名单字段（默认只读本地夹具，**不 SSH**）；假 HTTP/2 共端口、WS/gRPC 结束时间、NAT 改端口、STUN 不符即 UNSAT+风险事件、coturn 缺 transaction ID、同 LAN/hairpin/同出口并发、无 exit node DNS、只有 relay 的 W `not exercised`。依赖 T2/T3；AP/peer/解析器需另授权。 |
| **T5a B 断流刺激 / 1-2** | M/S/R 默认关证据开关仅关闭目标 fd（只证本地关闭/报错路径），I/W 定向断链，D 新查询/超时；网络侧 RST/半开/超时须另授权 AP 阻断原件，不外推。假设备测只影响目标、保留刺激时刻与恢复、三次可复现，不碰服务端/不关 Wi-Fi 冒充 B。依赖 T2/T3/T4，**第一批 T6a 前置**。 |
| **T6a 第一批执行器/判定器 / 3-4** | 默认不连设备/服务器；依赖 T0-T4、T5a。假 HDC/离线日志测第一批 AUTH、哈希/回读/预算/10 次 tconn/时钟 60 s 与 15 分钟、候选配对 ±2000 ms、仅传输中断一次保留原件、campaign 全程漏包即停、28 格全体任一未执行/UNSAT 阻止授予、第一批独立复核及 T0 + 用户批准前禁写阶段 pass、始终禁写 N2 pass；0047 (c) 须在浏览器回报后采新鲜 TUN 样本才允许判定，不得导入诊断记录为证据。 |
| **T7a 第一批符合性独立审查 / 1-2** | 全稿判据及阶段标签定义已于 v4 冻结；T0-T6a 的工具/配置/授权符合性另审后才能请用户签第一批 AUTH。假原件与工具 hash、无 recreate/无 exit node、权限/可触发性、缺件停止码逐项复核。 |
| **T5b 第二批 recreate/注入/撤销 / 3-5** | 默认关闭的工具证据开关在承载中触发 E 受控 recreate，管理端不变、无 exit node；第 1 代原样移植第 0 代 `/32` 与 `trustedApplications`，逐代核配置/实效路由；故障注入隔离与恢复默认关闭。F 分别支持 App 有序撤销及用户系统设置触发 `onDestroy` 的失序撤销，两路各三次、各 60 s。假设备测 E 当前连接及配置完全一致、F 两路径无旧包、每故障至少一次触发且无裸发；依赖 T2/T3/T4，不将第一批无 recreate 当修复。 |
| **T6b/T7b 第二批执行器及符合性审查 / 2-4 + 1-2** | 依赖 T5b；假 HDC/原件测第二批独立 AUTH、E/F/G 的七行缺件 UNSAT、两批授权和原件不混用、全量审查前禁止 N2 pass。第二批工具**后做后审**，仅审冻结判据的实现符合性，变更判据必须回 T0，用户另行决定第二批 AUTH。 |

## 冻结决策与后续授权事项

以下 1–13 项为 **v4 测前已决选择**，各项的“可选项/取舍/默认建议”保留为冻结时的比较记录，**加粗默认建议即已选路径，不再等待重复选择**，但不是设备/管理端/AP/peer 的操作授权。用户第 12 项原文「12. 同意」，第 13 项要求「13. 你自己上管理端查，如果需要登录网页，就在vnc里打开让我来登录」；2026-09-28T19:58:12+08:00 用户对接受 (c) 诊断空档、其余建议与冻结原文回复「同意，继续」。未核实的当轮事实仍须授权后验证；前提未定或缺 AUTH 不得上机、抓包或改服务器，相应格保持 `UNSAT`。诊断 0045–0047 均 `is_evidence=false`，本节不追认任何门证据。

1. **VPN 范围（N1）：已选 A**。`trustedApplications=[本 App]` 加冻结 `/32`，维持 §3 本 App 不论源的 TUN 判据；0047 的四点诊断前提及 (c) 空档见 §2，均非门证据。T1 主机假配置/互斥及正式 (c) 修正执行器后复测仍未完成；hdcd 与 peer/relay LAN 主机重合须另核。A 被正式符合性验证推翻即停测，B 全局 VPN **未选、not used**，必须另经 T0 + 用户批准新冻结判据方可使用。执行：工具实施者做默认关开关及假配置测试，正式测量只凭另签 AUTH。
2. **管理端配置冻结与恢复**。问题：谁保持两批设备无 exit node、routes/desired routes 不变并恢复原状态？可选项：获授权管理员在测试窗口只读核实已有状态并冻结变更；若现状不符，由管理员另获明确变更授权后调整并留前后快照/恢复记录；不操作则停测。取舍：前者无写风险但取决于现状，后者有运维变更与恢复责任，不能由证据执行者擅改。**默认建议只读确认现状、由用户指定管理员冻结并在窗口结束复核/恢复；不满足则不开测**。执行：用户指定管理端管理员，独立复核者核快照，设备执行者不登录修改。
3. **治理追加登记标签定义（§二.7）：已随 v4 冻结追加**。见[治理](native-nx-governance.md)文末「追加登记：N2 第一阶段标签定义（随 N2 预注册 v4 冻结，2026-09-28）」；仅冻结含义，**未授予标签、未签发 AUTH**。定义要求第一批 A/B/C/D 28 格全体（仅测前逐格登记且经 T0 + 用户批准的具体豁免除外）满足逐 socket protect、全 campaign TUN 负证据及逐事件夹心正控、物理出口与独立网络侧 oracle，经第一批工具和原件独立复核、T0 + 用户批准后才可授予；任一适用格 `not exercised`、`invalid/UNSAT` 或 `UNSAT` 均阻止授予，未实现或未产生可选事件不缩减范围。E/F/G 不因此通过，该标签不是 N2 门结论，不得作为 E8/N6 或任何其他门的输入；`N2 pass/reviewed-pass` 仍须全量条件和审批。
4. **B 断流方式**。问题：哪一种故障代表 B？可选项：M/S/R 核心证据开关只关目标 fd；经授权 AP 侧定向阻断/恢复用于网络侧断流，I/W 按端点定向断链，D 新查询/超时。取舍：主动 close 可重复、无服务器写入，却只覆盖本地报错重拨，不能外推 RST/半开/超时；AP 更接近网络故障但需抓包/恢复授权。**默认建议本地关 fd 为 M/S/R 第一批路径，并明确限制；I/W、D 按上述类别刺激，另需网络侧结论则事前批准 AP 路径**。执行：工具实施者写默认关开关，用户批准刺激，获授权 AP/设备操作者执行。
5. **E 触发方式**。问题：如何在承载中确定性 recreate？可选项：经工具的默认关证据开关触发受控 recreate；不能实现则 E `UNSAT`、回 T0 重新预注册，不借管理端 routes 或默认路由变化。取舍：开关需主机假设备验证，但可保全原配置与因果；管理端刺激改变冻结条件。**默认建议工具开关，保持无 exit node、routes 不变，第 1 代逐项移植第 0 代 `/32` 和 `trustedApplications`**。执行：壳/工具实施者与主机测试者，用户另签第二批 AUTH 后设备执行者触发。
6. **F 撤销路径**。问题：系统撤销是否纳入？可选项：App 有序停止/关 fd 后 destroy，以及用户从系统设置撤销触发 `onDestroy`。取舍：前者可编排，后者顺序不可编排、要求更强观察；只测前者不能证明系统撤销。**默认建议两路都测、各三次、每次撤销后 60 s**。执行：用户授权系统操作，设备执行者和独立网络侧操作者联合留证。
7. **时钟容差定值**。问题：跨侧如何配对？可选项：校准后 ±2000 ms 且合成误差上界不超过该值；超过则停格回审。取舍：窗口过大增加歧义，过小易受日志粒度/落盘延迟影响；窗口内仍需唯一配对。**默认建议 ±2000 ms，不测后放宽**。执行：采集/判定工具实施者校准并留原始时钟映射，独立复核者核上界。
8. **peer/relay 拓扑与分裂 DNS 测前发现**。问题：两网目的是否一致？可选项：分别在家 Wi-Fi/蜂窝经受保护解析与候选发现 M/S、home relay、STUN、resolver、peer LAN/公网/hairpin 后取并集；发现不全则不开相关格。取舍：并集 `/32` 较多但固定覆盖，单网或只记公网会漏切网和同 LAN 目的。**默认建议双网预发现并集、核 hdc 地址及 §2 重合例外，冻结 direct P2P 与 NAT 映射**。执行：设备/网络侧操作者仅在诊断或 campaign AUTH 内发现，用户指定 peer/路由器操作者协助，T0 审冻结表。
9. **`/proc/net` 可读性：已选 hdc shell 四表**。0045 App 内读 `/proc/net/*`、`/proc/self/net/*` 均 `readable=false code=13900012`；0046 与 0047 `hdc shell cat` 四表成功并按 App uid 找到 TCP/UDP 行，原件见 §2。App 进程读取与 `/proc/self/fd` inode 方案 **not used**。hdc shell 高权限视图只用于快照对账，不能冒充 App uid 可读或证明 fd protect；四表不可读、当轮 uid 未由 `bm dump` 复核或必要 socket 对账失败则 `UNSAT`。执行：正式 AUTH 仍需逐命令授权，不能复用诊断授权。
10. **`/proc/net` 正式快照白名单：已选四条 hdc shell 命令**。两批 AUTH 分别逐条列出 `hdc shell cat /proc/net/tcp`、`hdc shell cat /proc/net/udp`、`hdc shell cat /proc/net/tcp6`、`hdc shell cat /proc/net/udp6` 及 `bm dump` uid 复核的精确调用、间隔/次数、累计预算、脱敏及原件规则。执行：AUTH 拟稿者列白名单，用户回读确认，设备执行者只照单执行；本稿不授权这些命令。
11. **路由器/AP 与 peer 侧抓包**。问题：怎样证明 LAN、hairpin、公网的物理侧独立收件？可选项：授权家中 AP/路由器镜像及 peer 自有主机外侧 UDP 抓包；拒绝或不能唯一配对时相关格 `invalid/UNSAT`。取舍：镜像增进物理来源及 NAT 消歧，但需设备主人、过滤窗口和脱敏权限。**默认建议两侧只读、目标/时窗限定抓包，蜂窝另核运营商 NAT 来源**。执行：用户指定 AP/peer 操作者，独立复核者核匹配，证据执行者不越权抓取。
12. **`192.168.50.6` 查询日志**。问题：该地址当轮是否实际 Wi-Fi resolver 且日志可得？可选项：可控时另获授权读限定查询日志；不可控/未授权则按受保护 fd 同 ID 响应的已冻结 D 替代 oracle。取舍：日志提高独立性但需最小化敏感查询；替代法不应冒称独立 resolver 回执。**默认建议先核身份和权限，可得则读脱敏限定日志，否则用 D 替代并如实标记**。执行：用户指定解析器管理员与设备操作者，独立复核者核对应。
13. **同出口其他 NetBird 客户端：已选先协调暂停，不能暂停则唯一排除**。2026-09-28 08:05 管理端只读核查见同一家宽公网出口 `175.10.221.251`：AdminDingZuwei (`100.108.156.116`，Windows 11，`dingzuwei@alfadb.cn`)、NetRPI (`100.108.162.237`，Debian，`rpi`)、NetHostDebian (`100.108.171.38`，Debian，疑似 relay VM `192.168.50.9`)；测试 peer `netbird-ohos-126-29` (`100.108.126.29`) 亦同出口，须按其实际角色登记，不可当作其他客户端任意停用。**选择**由各客户端所有者经用户许可在取证窗口暂停前三者 NetBird 客户端，保留身份/暂停与恢复时刻并于窗口后恢复；不能暂停时逐个登记身份与时窗、以原始日志唯一排除，同源多候选不可消歧则 M/S/R 格 `invalid/UNSAT`。NetHostDebian 上停止 NetBird 客户端不等于停止 relay 容器；先核实其与 `.9` 的关系，绝不得擅停 relay 服务。执行：所有者停/恢复，授权设备执行者登记，独立复核者核服务端元数据。用户另行分别确认蜂窝、两向换网、relay opt-in 和两批独立 AUTH；手机已有 SIM、home.alfadb.cn relay 已启用，但仍须各批正式授权与确认。`223.114.24.186` 是 BlkServer，**不是**蜂窝出口；0044 蜂窝出口观察为 `223.153.208.166`，正式测量须重新实测而不预填旧 IP。

## v3 修订记录（2026-09-28）

在原 v2 文件内改为 v3，未新建文件、未改变其 DRAFT/非授权状态。按 2026-09-28 复审落实：阶段标签测前定义登记与 28 格适用集；STUN 地址不符 `UNSAT` + 风险事件及 coturn transaction ID 当轮核实；App 专属 `trustedApplications` 默认 A 与 S2/S6 主机互斥测试；双网络分裂解析、LAN/hairpin 源地址和无 exit node 出口定义；E 工具开关及原样跨代配置；F 系统撤销 60 s；四表诊断可读性与进程内替代、±2000 ms 配对；B 本地关 fd 不外推网络侧故障。所有实施任务仍待独立审查、用户批准及分别授权。

**v3.1 补记（2026-09-28）**：方案 A 冻结前与 `/proc/net` 探查合并真机核四点，失败转 B 须 T0 + 用户重审或停测；补 SDK/builder 与 0031 依据、hdc/peer 主机重合例外、证据配置不外推产品配置，以及蜂窝/relay 既有确认与两批 AUTH、两向换网的默认建议。不改变 DRAFT/非授权状态。

## v4 冻结记录（2026-09-28T19:58:12+08:00）

原 v2 路径内就地定稿，不新建预注册文件；v3/v3.1 修订记录为历史，不代表当前仍为 DRAFT。全稿已完成两轮 T0 审查及独立差分复核（`anthropic/claude-opus-5-5`），差分复核要求四点 A 诊断明示及不外推到产品全局 VPN/默认路由，已落实 §2/§5。用户先批准第 12 项（「12. 同意」），第 13 项指定管理端自行查验及登录由用户在 VNC 完成；在 `2026-09-28T19:58:12+08:00` 回复「同意，继续」，确认 0047 (c) 的约 13 秒采样空档、其余 11 项建议与 v4 冻结。管理端只读查验的其他同出口客户端及蜂窝出口地址纠错见第 13 项；冻结正文与[治理](native-nx-governance.md)文末阶段标签定义同版登记。

诊断 `AUTH-DIAG-DEVICE-SMOKE-20260927-0045`、`AUTH-DIAG-DEVICE-SMOKE-20260928-0046`、`AUTH-DIAG-DEVICE-SMOKE-20260928-0047` 均 `is_evidence=false`、`not_a_gate_pass=true`，只支撑方案 A 语义与 hdc shell 四表采样路径的测前选择，不得并入第一或第二批门证据。0046 的 I 级日志遭设备默认日志级别过滤，四点判断均 unknown；0047 补齐诊断但 (c) 末次计数在用户回报前，正式执行器须修正并重测。设备固件观察为 `PLA-AL10 7.0.0.109(SP6C00E105R7P3)`（此前 `.107`，大版本 7 未变），服务端日志 oracle 已启用，AUTH 含 `AMEND-0047-20260928-01`；均非未来正式 AUTH 或结果替身。两批工具尚待实施及符合性审查、两份 AUTH 尚待分别签发和回读确认，未授权任何设备/服务器操作，未授予 `N2 第一阶段通过` 或 N2/N6 门结论。

## v4.1 追加修订记录（2026-09-28T22:07:17+08:00，只追加不改写）

本记录依据 T0（`anthropic/claude-opus-5-5` 席，干净上下文，2026-09-28）评审意见（结论「**有条件**」，8 项必改）与用户 2026-09-28T22:07:17+08:00 在本会话 DSH Web GUI 对 v4.1 最终文本的批准（原文 `批准`）追加，对 v4 作三项测前修订。批准依据为已批准修订提案 [`n2-prereg-v4-revision-proposal-20260928.md`](n2-prereg-v4-revision-proposal-20260928.md)（r2，sha256 `c05b301e233cc245359af7be71e16cd22f5fb5b06c87da8602a3899b8ea945b4`，2026-09-28 现算，落盘时核对一致；该提案文件本身未改写）。修订**不改变 §3 阈值、§2 配置冻结与治理标签定义文字**；**适用集计数口径按本记录第 1 项写明**。治理文末已同版追加登记（见[治理](native-nx-governance.md)「追加登记：N2 预注册 v4.1 的 Wb 四格豁免与 Wb/Ws oracle 地点（2026-09-28）」）。

1. §4 表 Wb 行的 **A/B/C/D 四格**改为豁免登记格（依据：`wg_fwd_open` base socket 在当前 N11 架构下结构性不发包；唯一例外见下）——**E/F/G 三格与行内 oracle 文字保持 v4:57 原文，第二批已冻结判据不在本次变更范围**。替代证据 E1–E6 与仍须 fail 的条件见下文「四格豁免登记表」与「替代证据 E1–E6」；其中 **fail/回 T0 触发条件**为：任何 Wb fd 出现发送 / base fd 未 protect / TUN 命中 / **出现 `N11_WG_FEED|replay-egress-failed` / adopted 时 `endpoints>0` 而该 peer 的 egress 未重放成功**（`wg_device.rs:2285-2302` 的重放顺序例外）。配一个**另行实现与评审的失败关闭小修**（egress 未重放成功则不重放该 peer 的 endpoint，只收紧），落地并审查前不得仅凭静态论证依赖「永不发包」。**适用集 = 24 格**（M/S/R/D/I/Ws × A/B/C/D）；**另有 Wb × A–D 4 个豁免格**，其替代证据齐备且未触发任何 fail 条件，是授予标签的**附加必要条件**。不改产品回落路径。
2. §4 L64 的 Wb/Ws 独立网络侧 oracle 地点改为 k8s 宿主机 `192.168.50.3`（`alfadb`）限时只读抓包：过滤器**锚定 peer 侧目的**（`udp and dst host <节点/Pod IP> and dst port in {冻结 --wg-port,--ice-port}`）、**两个抓包点**（上联/`vmbr0` 与 Pod 侧 veth/cni，NAT 前后）、**不读 conntrack**、测前只读冻结 DNAT/hostPort 与蜂窝入站只经此节点（否则 `UNSAT`）、`-s 96` 及其理由、同一五元组按类型分流且只配对首包、setcap 授予/撤销登记、原件存放/sha256/删除规则、与 `N2_SOCK` 的 ±2000 ms 配对见 §4「Wb/Ws 独立网络侧 oracle（v4.1）」段。
3. §4 增补「A 列生产事件的操作性定义（v4.1 澄清，不改判据）」：逐行 M/S/R/D/I/Ws 的事件、夹心前控最早时刻（TUN tee attach 锚点之后）、触发方式与**通用原则（触发只能改变发送时刻，不能造出生产不会发的报文种类）**，见 §4 该小节；M/S/R 的触发均为生产本来就会发的真实 RPC/帧（M：`GetServerKey` 类只读幂等 RPC；S：见该小节 S 行；R：等服务端驱动的 HealthCheck 应答），D 按 Q1 口径（见该小节 D 行）。配套产品/执行器/判定器实现属实现符合性，须 T7a 审查，全部默认关。
4. 采纳 T0 对 Q1/Q2 的裁定：A(D)/B(D) 事件口径（Q1，见 §4「A 列生产事件的操作性定义（v4.1 澄清，不改判据）」D 行）；peer 应用层 `recvfrom` 回执作为**辅助证据（应用层，非独立网络侧 oracle）**采用，不替代宿主抓包、不补救抓包缺失的格（Q2，见 §4「Wb/Ws 独立网络侧 oracle（v4.1）」段末）。

**四格豁免登记表（逐格写明豁免及依据）**

| 格 | 豁免的判据 | 豁免依据 | 替代证据（必须齐备，缺一即该格 `UNSAT`） | 仍须 fail 的条件 |
| --- | --- | --- | --- | --- |
| **Wb × A 首连** | v4:36「3 次独立可归属的**真实生产发送事件**」与 v4:57「原 fd/dup 出包，create 后继续」 | 可行性报告 §2.6 六条代码证据：base socket 结构性不发包（唯一例外见上）；制造发送路径与 `peer_conn.rs:1224-1229` 的 fail-closed 设计直接相反（round-7 bug 形态） | E1 身份/protect 链；E2 发送点零回执；E3 按 fd 发送计数=0；E4 四表在册对账；E5 宿主抓包覆盖证明（LAN 零命中／蜂窝来源闭合）；E6 窗口覆盖 | 任何 Wb fd 出现发送；或 base fd 未 protect；或 TUN 命中；**或出现 `N11_WG_FEED\|replay-egress-failed`；或 adopted 时 `endpoints>0` 而该 peer 的 egress 未重放成功** |
| **Wb × B 断流/重试** | v4:57 B 列「真实断链后握手/包，换 fd 重保护」 | 同上；且 switch 文档:116 明记 `reset_peer_paths` **有意保留 base device socket 且无 core 侧重建路径**，base-only peer 在重选前保持无路径（可行性报告 §4.3） | E1 断链刺激前后 base fd 身份/代际不变；E2/E3 刺激后零发送；E4 对账；E5 覆盖证明；E6 窗口含刺激点 | 同 A |
| **Wb × C Wi-Fi→蜂窝** | v4:57 C 列「新网络的 base 包与 peer 收件」 | 同上；切网只重解析/重 feed ICE 池（`ets:777-784,797,812,835`），不触碰 base fd | E1 新网络下 base fd protect 代际/`net_id` 登记；E2/E3 零发送；E4 新网络下四表对账；E5 蜂窝**来源闭合**（源用当轮实测，不预填 v4:113 的 `223.153.208.166`）；E6 窗口含切网点 | 同 A |
| **Wb × D 蜂窝→Wi-Fi** | v4:57 D 列「同向反转」 | 同 C，方向反转 | 同 C，LAN 直连下 E5 用**五元组零命中**（源按 Wi-Fi 实测 `192.168.50.199` 或当轮实测 LAN/NAT 源） | 同 A |

**替代证据 E1–E6（「显式无发送负证据」的完整定义）**

- **E1 身份与 protect 登记**：`N2_SOCK|kind=Wb|fd=…|dup_fd=…|proto=17` 的 `phase=open`、`phase=protect_begin`、`phase=protect_settle|status=success`、`protect_generation`、`net_id`、`local`（`getsockname` 的物理 IP:port）齐全；且 **`protect_settle` 时刻严格早于 tee attach 锚点**（`N2_TUN_GAP|reason=before-tun-attach`，`wg_device.rs:2269-2271` / `N7_WG_FEED|adopted`，`:2303`；`ets:3331,3506-3563` 的 protect 在 `:3350,3452,3471` 的 attach 之前）。`local` 出现 `100.108.0.0/16`（`N2_SOCK_OVERLAY_LOCAL`）即 fail。
- **E2 发送点零回执（r2：覆盖性已核实，由 T7a 复核）**：WG 出向唯一发送点是 `send_to`，它对传入 fd **一视同仁**——`wg_device.rs:1556-1580` 先 `io_begin(fd, …)`、`sendto` 后 `first_io(fd, …, "wg_sendto")`，不区分 egress 还是 base；而 base socket 的 core 侧 dup 已由 `WgDevice::adopt`／`reattach_socket` 经 `n2_socket_trace::duplicated()` **登记进追踪表**（`wg_device.rs:713`、`:771`；注册实现 `n2_socket_trace.rs:50-57`），且 `receipt_due` **保证「首包必出回执」**（`n2_socket_trace.rs:103-106`）。故该 fd/dup 上**全窗口零回执**即零发送。**抽样说明**：`receipt_due` 对 `Wb`/`Ws` 只保留「首包 + 其后每 128 包」，因此回执是「有发送」的**充分信号**、不是完整计数；完整计数由 E3 提供（二者互证）。**属实现符合性，由 T7a 独立复核该覆盖链与抽样语义，不改判据。**
- **E3 按 fd 的发送计数=0（r2：实现前置项 P2）**：证据开关下**按 fd** 累计 `sendto` 调用次数（**现有 `WgDeviceStats` 是设备全局计数、不区分 fd，故须新增按 fd 计数**），全窗口为 0 并周期落盘。与 E2 双通道互证：日志可能被丢帧/日志级别过滤或被抽样跳过，按 fd 计数弥补「回执抽样/没写日志 ≠ 没发」。**实现前置项（默认关，须 T7a 审查）**。
- **E4 四表在册对账（r2：删去「发送统计无增长」）**：按 v4:32/L109-110 每轮 `hdc shell cat /proc/net/udp`（及需要的 `udp6`）经 `bm dump` 当轮复核 uid 后过滤，确认 Wb fd/dup 的 inode/tuple **在册**且与 E1 的 `local` 一致。**不要求、也不使用任何「发送统计无增长」**（`/proc/net/udp` 没有按 fd 发送计数，全局统计无法归属到该 fd）；fd 对不上、四表不可读、uid 未核 → `UNSAT`。
- **E5 vantage 覆盖证明与来源闭合（r2：见 §4「Wb/Ws 独立网络侧 oracle（v4.1）」段）**：
  - **LAN 直连（五元组零命中）**：同一抓包会话、同一过滤器（锚定 peer 侧目的 IP:port）、同一时间窗内**必须先命中 Ws 生产包**（证明 vantage 与过滤器非盲），而 Wb 注册 tuple（E1 的 `getsockname` 本地 IP:port）零命中，才构成物理侧负证据；整窗零包或覆盖缺口不构成负证据。
  - **蜂窝下改为「来源闭合」**：宿主看到的、**来自设备**（当轮实测的设备 LAN IP 或当轮蜂窝 NAT 后公网源）**到 peer 冻结端口**的**每一条流**，其 NAT 后源地址须能归到**已登记的 I/Ws 映射**（`XOR-MAPPED-ADDRESS`；v4:64 允许 Ws 继承同一选中 socket 的已证映射）；**出现任何归不上的源映射即 fail 候选并回 T0**——不得当作 Wb 零发送的负证据，不得现场改判据，也不得把该流记成别的行。
  - **E5 只覆盖测试 peer**：**peer 集合须冻结**（测试 peer 身份/地址入 §2 冻结登记）；**其余 peer 只靠 E2/E3**，E5 不为其提供零发送覆盖，登记中不得把「测试 peer 零命中」外推为「全部 peer 零命中」。
- **E6 窗口覆盖**：负证据窗口定义为**该 base fd 的生命周期** `open → close/destroy`（由 E1 的 open 与关闭/`destroy` 日志界定），且必须完整包含 v4:39 的「create+tee attach → destroy」全 campaign 可观测窗口与所有格间时段；**窗口必须覆盖 `try_build` 重放点**（`N7_WG_FEED|adopted` 及其前后的 `replay-*-failed` 行）。窗口有缺口且缺口与 base fd 可能发送的时段重叠 → `UNSAT`。

**豁免不等于 pass，且不可回退式放宽**：四格裁决登记为 `exempted-with-substitute-evidence`，**不得**写 pass、不得计入「已证明生产外发」、不得作为任何门结论输入；E1–E6 任一缺件即该格 `UNSAT` 并阻止授予标签（豁免的是「必须有真实生产发送」这条判据，不是证据门槛）。若正式测量中出现「Wb fd 有发送」，**或**出现本记录第 1 项与「四格豁免登记表」所列的 `replay-egress-failed` / 「有 endpoint 无 egress」触发条件，本豁免依据即被证伪：该格 `fail`、停该格、回 T0 + 用户重新裁决（不得现场改判据、不得测后追认）。

**T0 意见与条件（2026-09-28，`anthropic/claude-opus-5-5` 席，干净上下文）**：结论「**有条件**」，8 项必改——① 只改 Wb 行 A–D，E/F/G 保持 v4 原文；② 补 `try_build` 重放例外并写成 fail 触发条件 + 失败关闭小修前置；③ E2/E3/E4/E5 改写（覆盖链、按 fd 计数、删「发送统计无增长」、蜂窝来源闭合、只覆盖测试 peer、Ws 回执抽样）；④ 适用集口径改为 24 格 + 4 豁免格为附加必要条件，并写治理三项注记/边界；⑤ 宿主抓包改为 peer 侧锚定过滤、两抓包点、snaplen 定值、类型分流、setcap 授予/撤销登记；⑥ A 列 I/Ws 澄清、M/S 用生产真实只读幂等 RPC、R 改为等服务端驱动 HealthCheck 应答、通用原则；⑦ Q1/Q2 裁决写入；⑧ 用户回复只是方向同意，最终文本须重新批准 + 前置项清单。**条件**：8 项必改全部落实 **且** 用户对最终 v4.1 文本重新批准。**落实与满足**：8 项必改已逐条落实于提案 r2（见该提案文末「修订记录 r2」），主会话已核对；用户已于 2026-09-28T22:07:17+08:00 对最终 v4.1 文本重新批准（原文 `批准`）。P3 措辞按实现对齐为 `phase=subsequent_io` 写完成回执（已在提交 `b3817c1` 实现）。

**用户批准（原文与时间）**：2026-09-28T22:07:17+08:00，用户在本会话 DSH Web GUI 回复原文 `批准`。语境：主会话逐条说明 v4.1 五项变更（Wb×A–D 豁免与替代证据及仍然 fail 的条件、24+4 口径与标签引用注记、宿主机 `192.168.50.3` 抓包 oracle 及限制与撤销、A 列操作性定义与通用原则、Q1 D 列 A/B 界限、Q2 peer 应用层回执为辅助证据）后请求批准。此前用户对三项修订**方向**的回复（原文 `1. 可以`、`2. 已添加公钥，用户名alfadb`、`3. 可以`）只是方向同意，不是最终文本批准；最终文本批准以上述 `批准` 为准。

**主会话口径决定（随本记录登记）**：P8（S 心跳/接收看门狗）保持「可选」；第二批 Wb 行 E/F/G 的 vantage 待第二批 AUTH 前另行确认。

**实现前置项状态**：P1（`try_build` 失败关闭小修）、P2（按 fd 发送计数）、P3（已连接 TCP fd `phase=subsequent_io` 写完成回执）**已实现于提交 `b3817c1`**；P4（A 列触发原语）、P5（`withN2Sandwich` 落点）、P6（执行器 A 列时序）、P7（peer `recvfrom` 回执）、P8（S 心跳/接收看门狗，可选）**待实现**。全部前置项须经 T7a 独立符合性审查；未完成前不得按 v4.1 执行测量。

**版本哈希**：v4（原冻结版，2026-09-28T19:58:12+08:00）sha256 `528ef22b97176f0813ef0524175bbb6c9202762a072745d3753262e36c46f099`；批准依据修订提案（r2）sha256 `c05b301e233cc245359af7be71e16cd22f5fb5b06c87da8602a3899b8ea945b4`。本 v4.1 冻结时间 `2026-09-28T22:07:17+08:00`。

本记录不授予 `N2 第一阶段通过`、不签发 AUTH、不追认 0045–0047 或任何诊断记录为证据，不授权设备/服务器/抓包操作。两批 AUTH 仍须分别签发并回读确认。

## v4.2 追加修订记录（只追加不改写；登记已获批的 Wb C/D E1 次序窄修订）

> 本节在 v4.1 全文（`:1-224`，78511 字节）之后**只追加**；旧件全部字节保持为完整前缀，其 prefix sha256 仍为 `06db804da6ce788ac862b987fe7bc069517ccda93d467b24f4bc15746648bed1`（落盘后已复核）。本节**不改变**：`:57` Wb 行的 **A 格正文**与 **B 格「同左」**、**任何新 tee 的首个 Wb 注册**的初次条件、E2–E6、第二批 E/F/G 判据、§3 阈值、§2 配置冻结、治理标签定义文字。

**批准依据与性质**：本节登记**已获用户批准**的窄修订 **R1–R4 + F1**。批准对象与文本以被批准的提案快照为准：[`n2-wb-e1-ordering-amendment-proposal.md`](n2-wb-e1-ordering-amendment-proposal.md)（sha256 `758148ddd07f0ca871c11e07427441f3c64d7d425f41134e38a920be0c1cfbab`）。该提案文件**未被改写一个字节**（其状态行仍逐字为 `DRAFT / UNAPPROVED`；批准关系由批准记录建立，不通过改写提案正文表达）；批准事实见 [`n2-wb-e1-ordering-approval-record.md`](n2-wb-e1-ordering-approval-record.md)。用户所选选项原文（主会话 `ask_user_question` 选项文本，逐字引用）：`批准窄修订，继续离线工作（推荐）`。
**不伪造身份/时间/签名**：本节**不**登记用户批准发生的具体时刻，也**不**声称用户签名；本节的落盘时刻同样不登记。

**授权范围（不得代扩大）**：仅授权离线实施（代码）、离线测试与新规则登记；**不**授权任何设备操作（无 hdc/ssh/抓包、无设备/AUTH）；本登记**不是 AUTH**，不构成第一阶段标签、pass 或任何门结论，也不构成「实现已验收」。

**禁止追认旧证据（与提案 §6 一致）**：批准前已有/在跑的 C/D 数据、日志环与 `N2_CD_SANDWICH` 行**一律不得**被追认为符合新条款的证据；严格次序必须在批准并登记之后的新窗口内实测；不得改写/补写/回填 `protect_mono_ms` / `pre_hit_ms` / `post_hit_ms`；不得用墙钟换算冒充核心单调时钟。

### R1 —— `:57` Wb 行 C/D 两格「同左」的限定（A/B 不动）

- 旧 C 格（逐字，被 R1 取代）：`同左；新网络下 base fd 仍零发送`；旧 D 格（逐字，被 R1 取代）：`同左；跨方向反转 base fd 仍零发送`。
- 获批新文本（逐字引自被批准提案 §1 R1）：
  - C 格：`同左（**不含** before-attach 次序——**仅当**本次切网重开/protect/重馈 base 发生在**同一个持续挂载的 tee 生命周期内**时，E1 次序按 R2 的 C/D 后续代例外判定；其余判据不变）；新网络下 base fd 仍零发送`
  - D 格（按提案 §1 R1 原文「D 格同文，方向反转为 `跨方向反转 base fd 仍零发送`」展开）：`同左（**不含** before-attach 次序——**仅当**本次切网重开/protect/重馈 base 发生在**同一个持续挂载的 tee 生命周期内**时，E1 次序按 R2 的 C/D 后续代例外判定；其余判据不变）；跨方向反转 base fd 仍零发送`
- **A 格正文不动**：A 格仍逐字保留冻结原文 `protect 完成时刻早于 tee attach 锚点`（其初次条件由 R2 明确，格内文字不改）。**B 格「同左」不动**：B 格仍为 `同左；断链刺激后 base fd 仍零发送`，其 E1 仍是「断链刺激前后 base fd 身份/代际不变」，不引入新代次序例外。
- **其余情形不变**：不在同一持续挂载 tee 生命周期内的任何情形（含新 tee 的首个 Wb 注册、A/B 两格）**不适用**该例外，仍按 R2 的初次条件（protect 严格早于该 tee attach）；第二批 E 的受控 recreate 按第二批原规则**另判**，不在本例外的适用范围。

### R2 —— `:202`（E1）的次序子句（本次修订的核心，唯一实质放宽）

获批新文本（逐字引自被批准提案 §1 R2）：

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

（上引「实现现状」段是**提案时**的状态记录，不是规则文本；离线实施与本地测试的现状另见 [`n2-wb-e1-ordering-impl-acceptance-20260930.md`](n2-wb-e1-ordering-impl-acceptance-20260930.md)，该实施**尚未**经提案 §7/§8 的独立 T0 符合性审查。）

### R3 —— `:197`（Wb×C）替代证据列

- 旧文本（逐字，被 R3 取代）：`E1 新网络下 base fd protect 代际/`net_id` 登记；E2/E3 零发送；E4 新网络下四表对账；E5 蜂窝**来源闭合**（源用当轮实测，不预填 v4:113 的 `223.153.208.166`）；E6 窗口含切网点`
- 获批新文本（逐字引自被批准提案 §1 R3）：`E1（按 R2 新代次序，仅限**同一持续挂载 tee 生命周期内**的 C/D 后续代）新网络下 base fd 的新代 `protect_settle|success` **严格位于前控 HIT 之后、后控 HIT 之前**（`pre_hit_ms < protect_mono_ms < post_hit_ms`，含相等即 `UNSAT`），且 `(fd,dup_fd,protect_generation)`/`net_id`/`local` 齐全；E2/E3 零发送；E4 新网络下四表对账；E5 蜂窝**来源闭合**（源用当轮实测，不预填 v4:113 的 `223.153.208.166`）；E6 窗口含切网点`
- `:198`（Wb×D）「同 C，LAN 直连下 E5 用**五元组零命中**（源按 Wi-Fi 实测 `192.168.50.199` 或当轮实测 LAN/NAT 源）」文字不动，其中「同 C」引用 R3 后的 C 列。

### R4 —— 比较时钟的精确化（随 R1/R2 一并批准；属措辞精确化，非放宽）

获批新文本（逐字引自被批准提案 §1 R4）：

- 次序比较一律使用**核心单调时钟**字段：Wb 行的 `protect_mono_ms`（`n2_socket_trace.rs:195-211`、`:300-303`）与 tee 锚点的核心 `sys::mono_ms`（`wg_device.rs:2423`、`:2429`）比较；不使用壳侧 `wall_ms`。
- 采样语义（随 R2（a）一并批准）：`protect_mono_ms` 是**壳返回 protect 结果之后**、调用 core `n2_socket_trace_protect` 时由 `sys::mono_ms` 采样的**壳判决/平台完成时刻上界**——既**不是**精确的平台完成时刻，也**不是** adopt/接管时刻；与 tee 锚点的核心 `sys::mono_ms` 同钟比较，**不做墙钟换算**。

### F1 —— 禁令（与 R1–R4 同时生效）

获批新文本（逐字引自被批准提案 §1 F1）：

**禁止**以「切换时重新挂载 tee / 重设 attach 锚点 / 让 tee 在切换后再挂一次」为**凑** C/D 新代次序人为制造条件；**禁止**测后补写锚点或回执时刻；**禁止**测后追认旧数据（§6）。

本禁令**只**禁止上述为凑次序的人为重挂/重设/事后补写；**不禁止**另行授权的原第二批 E 受控 recreate / TUN 替换——该路径按第二批原规则（E 列判据）**另判**，且**不因此获得**本修订的 C/D 新代次序例外。本修订**不提供**任何隐式第三方案。

### 优先说明（本节只覆盖相应 C/D E1 子句）

- R1、R3 仅取代 `:57` Wb 行 **C/D 两格**的「同左」与 `:197` Wb×C 替代证据列中涉及 **C/D 后续代 E1 次序**的子句；`:198` 的「同 C」引用随之指向 R3 后的 C 列（该行文字本身不动）。
- R2 仅取代 `:202` E1 中 **C/D 后续代**的 before-attach 次序要求，并把初次条件明确为「每一个独立 tee 生命周期内首次成功接管登记的 Wb base」；**A、B 两格**与**任何新 tee 的首个 Wb 注册**仍适用初次条件。R4 仅精确化比较时钟与 `protect_mono_ms` 采样语义。F1 为新增禁令。
- **未列出的全部原规则不变**：A/B 两格、新 tee 首次、E2/E3/E4/E5/E6、Wb 零生产发送、§3 阈值、§2 配置冻结、治理标签定义、第二批 E/F/G 判据与第二批判据，以及全部 fail 条件（任一 Wb fd 实际发送 / **未保护 base 实际被接管或使用** / protect 失败后裸发或真实 TUN 泄漏 / from-OS TUN 命中 / `N11_WG_FEED|replay-egress-failed` / adopted 时 `endpoints>0` 而该 peer egress 未重放成功 → 该格 `fail`）；`fail` 不得被 `UNSAT` 掩盖。

**生效前置（未满足前不得按本修订判据取证）**：按提案 §7 第 1 步，须实现并测试严格边界（`pre_hit_ms < protect_mono_ms < post_hit_ms`，消除 `post_hit_ms >= receipt_ms` 的同毫秒可通过），并由**未参与本修订设计与实现**的独立 T0（跨厂商 family）完成 §8 C1–C5 符合性审查；本登记**不**表示该审查已通过，也不代替任何 AUTH 前置项。

**版本与哈希（本节不写入自身整文件 hash，避免自引用）**：v4 原冻结版（2026-09-28T19:58:12+08:00）sha256 `528ef22b97176f0813ef0524175bbb6c9202762a072745d3753262e36c46f099`；批准依据修订提案（r2）sha256 `c05b301e233cc245359af7be71e16cd22f5fb5b06c87da8602a3899b8ea945b4`；v4.1（旧件完整前缀）sha256 `06db804da6ce788ac862b987fe7bc069517ccda93d467b24f4bc15746648bed1`、78511 字节。本 v4.2 的**整文件** sha256 在完整文件落盘后另算，写入独立 sidecar `docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256` 与[治理](native-nx-governance.md)文末同版追加登记。

**kit 冻结文档 pin 边界（只登记，本任务不改 kit）**：本修订尚未提交，工作区文件不是任何 commit 的 blob；kit `n2-campaign` 的 Q-d pin 以 `sha256(git show <commit>:<path>)` 为准，故新文本目前**无法**被按现 pin 机制引用；受影响的 pin 条目见治理同版登记，须由后续任务提交并按同一规则重新 pin（不得以工作区哈希冒充）。

**非 AUTH**：本节不授予 `N2 第一阶段通过`、不签发 AUTH、不追认 0045–0047 或任何诊断记录为证据，不授权设备/服务器/抓包操作。两批 AUTH 仍须分别签发并回读确认。
