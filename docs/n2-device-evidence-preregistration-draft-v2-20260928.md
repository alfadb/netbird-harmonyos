# N2 真机证据预注册草案 v3（2026-09-28）

> **DRAFT / 未生效 / 非设备授权**。本稿作为一份覆盖全部外层类别与全部生命周期列的预注册候选，**取代**[第一阶段草案](n2-device-evidence-preregistration-phase1-draft-20260927.md)与[上游全量草案](n2-device-evidence-preregistration-draft-20260927.md)的操作判据；两份旧稿原样保留，仅作历史输入，不可混用其中相冲突的正控、路由或授权条款。本稿尚须整份独立审查、用户批准及按[治理](native-nx-governance.md)登记冻结，之后才能测量。本文不签发 AUTH，不授权连接设备、采集、运维改配置或修改服务器；0034/0037/0043/0044 均是诊断/预演，不能追认为 N2/N6 证据。

## 1. 一次冻结，分批执行

本次预注册范围为七行 **M 管理 TCP、S 信令 TCP、R relay TCP、D 受保护 DNS UDP、I ICE/STUN UDP、Wb WG base UDP、Ws ICE 选中后供 WG 的 UDP socket/dup**，与 **A 首连、B 断流后的重连/重试、C Wi-Fi -> 蜂窝、D 蜂窝 -> Wi-Fi、E 承载中受控 recreate、F 最终 VPN 撤销、G fail-closed 注入**的全部交叉格。§4 一次写定各格刺激和 oracle；§3、§5 的阈值、停止条件和不外推规则同时适用。R 须 opt-in 且有真实 relay 会话；Ws 只有确实选中本地 socket、交 WG 并从该 socket 发出 WG 外层包才算 exercised。实现需求要求但未实现的 TURN、relay QUIC/UDP、IPv6 外层等能力记 **`UNSAT`**，先实现并回 T0 更新预注册，不能以 `not implemented` 从门范围中扣除。实际未产生的可选单次事件记 `not exercised`，必要格据此 `UNSAT`；出现新的外层类别立即停测、回 T0，不用既有七行外推。

- 第一批 AUTH 仅执行 A/B/C/D 的 **28 格全体**，使用 §2 冻结的第 0 代 `VpnConfig` 和无 recreate 配置；相关格出现 recreate 就 `invalid/UNSAT`，保留原件，不换用新路由补做通过。
- 第二批 AUTH 另行签发，执行 E/F/G 所有七行；工具可在第一批后实现、测试并独立审查，但 **E/F/G 的判据、oracle、停止条件现在即随全稿冻结**。工具的版本/hash 与实施符合性须在第二批 AUTH 前再独立审查；任何改动判据、范围或阈值均须先回 T0、再取得用户批准并重新冻结，不得测后追认。两个 AUTH 预算和原件互不挪用。
- **第一批适用集固定为 28 格全体**，仅可在测量前经 T0 + 用户批准、在冻结登记中逐格写明豁免及依据后缩减；执行中和测后不得扣格。任何适用格 `not exercised`、`UNSAT` 或 `invalid/UNSAT` 均阻止授予“`N2 第一阶段通过`”，不得以可选事件未发生或能力未实现视作 pass。全部适用格达标、第一批工具和原件独立复核且 T0 + 用户批准后，**最多授予**该标签；这只是中间里程碑，**不是门结论，不得作为 E8/N6 或其他门的输入**，也不写 `N2 pass/reviewed-pass`。第二批全量通过及全量独立复核后才可依治理提出 N2 门结论。N6 按自身内层连通及默认路由判据另立 verdict；同轮原件可以共用，结论绝不互相代替。依[治理](native-nx-governance.md) §二.7，**标签定义须随本稿测前冻结即获独立审查和用户批准并追加登记**（拟登记全文见文末待决项 ③），授予仍须待第一批完成并复核；未登记定义不得测后补定义或自行创造阶段 pass。

## 2. 测前冻结、代际与覆盖

**两批管理端运维配置须由用户另行授权冻结**：测试设备无 exit node，NetBird routes 与 desired 路由集在全轮（含 E）不变；记录管理端快照、版本、route/peer 公告及差异监测，不擅自改服务器。默认路由在 peer 登记并 `tunnel_ready` 之前不会放开（`client/core/src/connector.rs:40-43`），而产品放开 `0.0.0.0/0` 必触发受控 recreate。两批均**不依赖默认路由**：建 VPN 时第 0 代 `VpnConfig` 下发非排除的定向捕获路由。测前分别在家 Wi-Fi 与蜂窝物理网络上用受保护解析/候选发现，记录 M/S（切网时 `NetBirdVpnExtensionAbility.ets:781,821` 会重新解析）、relay/home、两个 STUN、各网络全部 resolver、Wb/Ws 的 peer LAN/公网/可选 hairpin 目的及每个协议/端口；每个网络各发现一次并以并集合（重叠去重）在第 0 代为**所有候选外层 IPv4 各下发 `/32`**，不得只拿 Wi-Fi 或公网解析填表。`home.alfadb.cn` 的 Wi-Fi LAN 与蜂窝公网分裂解析须两者都覆盖；同 LAN direct peer 的 LAN host 也须覆盖。实际命中地址不在冻结并集中、候选 IPv6 无对应经预审捕获方案、resolver/peer/relay 后续变化或分裂解析发现不全，停相应格 `UNSAT`，不能测后扩路由。逐次核实实下发配置/生效路由、VPN 应用归属及目标覆盖；通常路由**不覆盖 hdc 控制端地址**，若 hdc 控制端与 peer/relay 局域网主机重合，方案 A 下 hdcd 不在 `trustedApplications` 内、不走 VPN，可允许 `/32` 覆盖该主机，但须测前真机核实下述 (c) 成立，否则按保守规则不开批次。其他无法同时满足的情形同样不开批次。不可使用 `force_default_route=true`、S6 旧诊断捕获路由或端点排除来假造覆盖。

**VPN 范围冻结建议 A**：仅在 N2 证据开关开启时将 `VpnConfig.trustedApplications=[本 App 包名]`，第 0 代及 E 第 1 代完全一致，只把本 App 流量纳入 VPN。现有 builder 未设置此字段（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:532-575`；`NetBirdVpnExtensionAbility.ets:2297-2302` 仅为 S2 注释）。SDK `@ohos.net.vpnExtension.d.ts:365-376` 声明名单内应用按 `routes` 走 VPN（`@since 11`），`:371,383` 声明与 `blockedApplications` 互斥；华为答复 `docs/n2h-huawei-support-replies-20260923.md:60` 否认 VPN 应用自身默认排除，0031 R2 在 `docs/n2h-device-round-20260926-findings.md:36-48` 对 `blockedApplications=[自身]` 仅有提示性负结果，**均不能代替 A 的真机验证**。须主机假配置测试其与互斥的 `blockedApplications`/S2、S6 捕获路由拒绝组合和默认关闭行为无冲突，不能默许二者同置；并在冻结前下一次有诊断授权的冒烟中，与 `/proc/net` 可读性探查合并真机核实同一冻结 `/32` 目的：(a) 本 App 未保护 socket 的真实发送进入 TUN；(b) 本 App 已 protect 的 fd 真实发送不进 TUN，留存物理/目标侧收件及正控；(c) 名单外应用或 hdcd 向该目的发送不进 TUN，留存实际发送/收件与捕获正控，hdc 与 peer/relay 主机重合时必须核实 hdcd；(d) 该配置成功 create，且 S2/S6 均关闭时不冲突。逐项留原件，不能把未发出或不可观察当作不进 TUN。任一点不成立，默认方案不得用 A：仅可改走经 T0 + 用户重新审查批准并冻结的 B，或停测。A 真机成立后，§3 不论源的 TUN 全窗扫描才只针对本 App：同一 App 的未保护正控仍经该代 `/32` 被捕获并在核心丢弃，故仍能证明本 App 未保护 socket 的可检测性；hdcd 在 A 的名单外，不路由其流量。**备选 B** 保留全局 VPN，但须额外固定 `VpnConfig` DNS、让其他应用静默并证实，且测前写死非 App 命中的识别和处理规则；TUN 帧本身无 uid，不能仅据帧区分，故无法同时保留“任何源命中即 fail”的强断言。B 削弱判据，只有经 T0 + 用户重新审查批准并冻结替代 oracle 才能采用；未选定 A/B 或 A 的上述真机验证失败且未获 B 重审批准即不开测。

M/S 的 create 前首次 TCP `connect`/发包以及 create 前其他类别的包处于无 VPN 路由阶段，TUN 零命中是平凡事实，只作时序与物理出口记录，**不计为**受路由覆盖的首包负证据。A 格必须进一步在 VPN 已 create、tee 已 attach 且第 0 代路由实际覆盖时，沿用对应 pre-create 已保护 fd 发真实第一批业务包并做夹心正控；无法做到的 A 格 `UNSAT`。凡在 attach 之前的帧不可见，不能把 create 前后两段拼作连续 TUN 捕获。B/C/D 同样须保持该代配置；任何目标路由变化、增删 peer/routes 引起的 recreate，按第一批 `UNSAT` 停格，留待第二批 E 的单独试验。

第二批 E 由**默认关闭的证据开关经已授权工具触发**承载中的受控 recreate，不修改管理端；不得依赖自然就绪后放开 `0.0.0.0/0`、变更 routes 或临场补“经预审覆盖路由”。无 exit node、desired routes 不变；第 1 代 `VpnConfig` **原样移植第 0 代全部冻结 `/32` 及选定的 `trustedApplications`**，逐代核对提交配置、实效路由和应用范围，不一致即 `UNSAT`，不得用第二代新解析修补第一代漏项。记录旧/新代配置、正控及跨代存活 fd 与新补给；补给/补池定时器若仍持有首代 `VpnConnection`，recreate 后请求全被拒绝，相关格不可测。产品前置为“**补给在 recreate 后改用当前连接**”并有主机回归；第一批的无 recreate 配置加逐次代际检测只能隔离该缺陷，**不能**算修复或 E/G 通过，若第一批出现 recreate 则 B/C/D 相关格 `UNSAT`。

每批冻结：专属 AUTH/sidecar 与用户回读确认、唯一执行者、设备脱敏 ID/型号/完整固件 SDK、App/VPN 包名、HAP/core/collector/runner/judge SHA-256、服务端版本及只读日志权限、relay opt-in、两侧物理网络句柄/接口和本地 IP、VPN/TUN 句柄、overlay IP、route generation、控制端 IP、所有实际外层目的及逐网络解析来源、fd/dup/socket ID、`getsockname` 的设备本地地址和可得 NAT 映射、UTC/单调时钟及误差上界、捕获起止/丢帧/过滤条件、原件路径与 hash。**无 exit node 时不存在可要求异于物理公网 IP 的“隧道出口公网 IP”**：此处只比较外层 socket 的设备本地物理地址与设备 overlay IP（相等立即 fail），并核设备物理接口出包、目标侧收件及 NAT 映射，不以“设备公网 IP != peer/relay 公网 IP”为门槛。家 Wi-Fi 同 LAN direct 时设备源为 LAN IP、peer 目的为 LAN IP、peer 侧源应与设备 LAN 地址/端口及实际 NAT 规则匹配；relay 与 peer 同在家网且访问 LAN 地址时也按 LAN 源判，不强求公网来源；访问家宽公网地址的 hairpin 则冻结公网目的和路由器 NAT 前后映射，peer/relay 收到的源可为家宽公网或路由器 LAN 地址，须 AP/路由器及目标侧双端证据消歧；蜂窝访问公网目的时以实测运营商 NAT 后公网源配对。设备、peer、relay 公用同一家宽公网出口本身不判 fail，若映射重叠无法唯一归属则 `invalid/UNSAT`；若真实外层发自 overlay、本应物理而走隧道则 fail。历史 `8.139.6.47`、`175.10.221.251`、`192.168.50.6` 及 overlay 值仅是线索，不预填本轮结果。

测前干净进程：经新 AUTH 授权的 force-stop 或重装后启动，记录本进程生命周期内 **未调用** `protectProcessNet`（S3 诊断开关关闭）、未设置 `blockedApplications`，`netbird-protect-deviation` 文件缺席、无 `VPN_PROTECT_DEVIATION_ARMED`，S2-S6 诊断臂全关。现有 `endpoint_exclusions_enabled` 默认 true，N2 T1 **复用此开关显式设 false**，不新增同义关闭开关；证明实际路由无 `/32`/`/128` 端点排除，initial 与第二批 recreate 两腿均检查。扩展拒绝“排除关 + S6 捕获路由”的既有组合，N2 多目标 `/32` 必须独立于 S6 新路径、默认关闭；选 A 时 `trustedApplications` 只含本 App，须主机测试验证与 S2 `blockedApplications` 互斥及 S6 拒绝关系未被绕过。**冻结前、下一次有诊断授权的冒烟**先探查本 App uid 能否读 `/proc/net/tcp`、`/proc/net/udp`、`/proc/net/tcp6`、`/proc/net/udp6`：候选只读命令分别为 `hdc shell cat /proc/net/tcp`、`hdc shell cat /proc/net/udp`、`hdc shell cat /proc/net/tcp6`、`hdc shell cat /proc/net/udp6`，逐条加入该诊断授权白名单，记录 uid、退出码、权限报错和各表原始样本；hdc shell 的可读性**不能代替 App uid 可读性**，须另以默认关闭的 App 内诊断读取相同四表核实。若 App uid 不可读，预先采用 App 进程内读取 `/proc/self/net/{tcp,udp,tcp6,udp6}`，与 `/proc/self/fd` socket inode 映射并用 uid/进程身份筛选；此路径也须在诊断冒烟确认实际可读、能覆盖已登记外层 fd。两种路径均不可用或 inode 无法对齐则正式测量相关格 `UNSAT`，不得用 hdc 高权限视图冒充。两批依 AUTH 白名单定期快照全部四表，与设备登记的**全部外层 socket**（含备用/重拨/dup、IPv6）对账；快照、时间、PID/UID/inode、遗漏和权限失败留原件。捕获路由生效前及期间，须确认所有外层 socket 已逐 fd protect，满足治理条件⑦；未知 socket、快照不可读或未保护者停测，未保护首发见 §5。清点不把非外层 socket 误判，也不把尚未分类的外层 socket 放过。

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
| **Wb WG base UDP**；真实 peer 自有主机外侧 UDP 抓包 | `wg_fwd_open` 原 fd/dup 出包，create 后继续 | 真实断链后握手/包，换 fd 重保护 | 新网络的 base 包与 peer 收件 | 同向反转 | 存活 base fd 与必要的新受保护 fd | 停 WG 发送/关 fd，撤销后无旧 WG 包 | base 保护/feed 失败销毁 VPN、无裸 WG |
| **Ws WG selected UDP**；同一 peer 外侧 UDP 抓包 + 选中到 WG dup 身份图 | 选中并由该 socket 发 WG 包 | 真实重选/重连后选中 fd 出包 | 新网络选中 fd 的 WG 包 | 同向反转 | 选中旧 fd/新选中 fd 跨代、当前连接重保护 | 停选中 socket/WG 并关 dup，撤销后无旧 WG 包 | 选中 fd/dup 保护或关闭失败后不交 WG、无裸发 |

**B 列刺激先冻结再授权**：默认 M/S/R 用证据开关由核心仅主动关闭该类已登记 fd，保留 close 时刻和新 fd 身份；这是**本地关闭/报错到重拨**路径，只证明该路径，**不外推**网络侧 RST、半开、黑洞和超时。需要网络侧断流证据时可另选用户授权的 AP 侧仅对该流目的定向阻断/恢复并留双侧原件；I/W 用同等受控断链/端点阻断触发重检/握手，D 用已授权的新查询/超时重试。不可用服务端断连（无服务器写授权）、不可关 Wi-Fi 冒充 B（与 C/D 混淆），也不可将 App 重启当新拨或跨代。刺激影响范围、恢复步骤、次数写入各批 AUTH；不可复现或不足三次则格 `UNSAT`。

**独立 oracle 细则**：M/S 在 netcenter（历史公网 `8.139.6.47`，当轮核实），R 在 home（历史 `rels://home.alfadb.cn:28443`，不是云端备用 relay），按实际发生的远端/日志逐连接对齐。HTTP/2 多请求可能共用**同一** `remote_port`/连接，不能把端口相等当多次新 socket；gRPC 长流与 WebSocket 在结束落盘时用 `ts - duration` 反推连接起点。测试窗口内其他同出口 NetBird 客户端须停止或逐个登记并可排除；做不到而同源多候选重叠，S 等格 `UNSAT`。设备本地端口不必等于 NAT 后 `remote_port`；家宽保留源端口只可在**当轮实测确认**后作附加配对，不作为默认假设。服务器日志只取白名单元数据，不能导出 headers/认证载荷；两侧 coturn 有空间轮转且扫描噪声，缺失/歧义即 UNSAT。

I 的设备侧须记录 STUN response 的 `XOR-MAPPED-ADDRESS`；先凭当轮确认可用的目的、时间、socket/transaction ID 与两端原件唯一归属 coturn `remote addr IP:port`。**coturn 默认日志通常没有 transaction ID**，测前须实际确认该字段存在才使用；不存在时不得称可按 transaction ID 跨端核对，只能结合响应侧 ID、目的、时间窗及其他独立元数据唯一配对，否则 `invalid/UNSAT`。唯一配对后两地址**逐字不相等即 `UNSAT` 并登记风险事件**（保留两端原件，不记 pass/fail 泄漏）；相等也不能只靠 `BINDING` 数量断言同一事件。Ws 若沿用同一选中 socket，可继承其已证映射，但仍须 WG peer 侧独立收件。D 在**全轮无 exit node**前提下使用明确替代判据：受保护 DNS fd 记录发出查询并收到同 ID 响应、该事件前后正控 from-OS 命中、campaign TUN 零泄漏，结合物理网络句柄和实际 resolver 目的；不强求独立 resolver 回执，E 不允许放开默认路由替换此前提。Wi-Fi 若 `192.168.50.6` 确是用户可控解析器，优先另取其查询日志强化；蜂窝 resolver 须测前发现/冻结，缓存无查询为 `not exercised`。Wb/Ws 在 peer **自有主机的物理外侧**抓实际到达 UDP：同 LAN direct 应为设备 LAN 源或实测 LAN NAT 源，家宽公网 hairpin 依冻结映射消歧，蜂窝 direct 才以运营商 NAT 后公网源配对；不得笼统要求设备 NAT 后公网来源。须测前确认能建立 direct P2P，只有 relay 路径则 Wb/Ws 的直达 peer 收件格记 `not exercised`，不得用 coturn、内层 peer echo 或 relay 收件替代。

## 5. 第二批 E/F/G 冻结注入与停测规则

G 按可触达类别逐一注入：`protect` reject、壳 5 s timeout、late resolve/陈旧 request ID、核心 supply 8 s timeout、物理 DNS/网络句柄缺失、fd close/dup 失败；可实现且预审的“resolve 成功但平台生效延迟”单列，确有未保护包即 fail 并回 T0。每个指定分支至少一次保留**故障确实触发**的状态、fd/dup/代际顺序与前后夹心正控及全程物理/TUN 原件；没有外层包的注入以当前覆盖路由下夹心正控与 fd/物理侧短窗证明无裸发，不以错误日志代替。失败 fd 不复用，恢复必须新建并重新 protect；跨 recreate 所有补给/补池使用**当前** `VpnConnection`，过渡代请求拒绝，旧 generation 不继续裸发。若某注入使 VPN/tee 不存在，预先的保护与路由有效窗和物理/独立侧无包、关闭顺序作为该子格的负证据，不能声称不存在的 TUN 正控；不能观测则 `UNSAT`。

F 各行均须覆盖两类授权撤销，各类**三次独立事件**：一类由 App 停 connector、ICE、WG 和补给发送者 -> 确认 fd/dup 全关闭 -> `destroy`；另一类由用户从系统设置撤销 VPN、走 `onDestroy`（`NetBirdVpnExtensionAbility.ets:3678-3700`），系统撤销与异步关闭顺序**不受 App 编排**，不得将其改写为有序路径或事后补停充数。两类每次均自撤销事件起固定 **60 s** 记录旧 fd 状态、物理口与独立网络侧；撤销后仍有旧包即 `fail`，窗口不连续、关闭次序不可核或接收侧无可归属记录即 `UNSAT`。恢复/重连在新授权范围内重新创建和 protect，不能拿撤销后天然无 TUN 证明无泄漏。E 的旧/新代路由覆盖、正控与 socket 图分别记录，任何中途不可见的帧不能跨窗外推。

发现任何真实生产外层包匹配冻结目的集合进入 from-OS TUN、未保护首发/保护失败后裸发、设备 `getsockname` 为 overlay，立即保存原件、`fail` 并停止本轮；同一格后续干净事件不抵消。缺 AUTH、用户确认、管理端冻结、干净进程、路由/hdc 隔离、socket 清点、夹心正控、实际出包、物理/网络侧 oracle、时钟或 tee 连续性，标 `invalid/UNSAT` 并停受影响格；泄漏风险、未知外层能力或代际失效可疑则扩大为全轮停测并交 T0/用户。不得用 N2-H 端点排除、`protectProcessNet`、VPN 业务成功或 N6 内层流量替代逐 fd protect；Wi-Fi 不外推蜂窝，第一批不外推 E/F/G，三次零命中不证明未来永不泄漏。本轮证据取自 `trustedApplications=[本 App]` 加定向 `/32` 的证据专用配置，与不设 `trustedApplications`、最终放开默认路由的产品配置不同；结论不外推至产品全局 VPN/默认路由配置，该部分由 N6 与条件⑦另行覆盖。

## 6. 两份 AUTH、制品与冻结流程

在全稿判据独立审查、用户批准和治理冻结之前，**两批都不得执行**。每批独立的 campaign 类 AUTH 必须逐项写设备/无线目标当轮核对、HDC argv 白名单（含在线查询、最多 10 次有间隔 `tconn`、只读设备时钟、经批准的 `/proc/net` 四表快照及 force-stop/重装若选用）、产物与 sidecar/hash、管理端配置证据、逐格允许刺激与切网/注入/撤销、采样命令、时间窗、预算上限和跨尝试累计、单一执行者、回滚、日志最小化与停止码。AUTH 签发后主会话回读并取用户**明确确认**，确认前不得发设备命令，连 `hdc tconn` 也不行。新 AUTH 适用治理 2026-09-27 追加登记：无固定 `valid_from/valid_until`，离线最多 10 次连接后暂停且不自动消费，传输中断仅该格最多重跑一次并留两份原件，结果不理想不能重跑；连接后核设备/主机时钟差 ≤60 s、每 15 分钟重比对，设备锁定/时间不可读/大版本变化或 hash 失配即停；跨会话续用重新回读确认、核时与 hash。第二批另做工具符合性审查、签新 AUTH，绝不沿用第一批授权。未经额外授权不登录服务器、不改日志配置、不抓 AP/peer 或读取原始敏感日志。

每格保留原始 TUN/物理/AP/peer/服务端白名单日志、socket 清点与时间快照、配置与代际变化、正控/生产对应表、过滤器和丢帧报告、sha256 清单、无效尝试和恢复记录。凭据、JWT、setup key、证书/密钥与敏感载荷不入日志；日志不足不补造。第一批 verdict 与第二批 verdict 独立归档，判定器禁止中途写 N2 门 pass。

## 7. 实施任务清单（未来工作；本次不改代码）

估算为工程人日，不含审批、真机、运维、等待或新能力开发；全部诊断开关、路由、tee、注入、网络命令**默认关闭**，主机验收用假设备和离线原件。`client/entry/selfcheck/` 有并行执行者，后续实施另行协调，本次不碰 `client/`。

| 顺序 / 规模 | 依赖与交付（主机验收） |
| --- | --- |
| **T0 预注册与运维前提 / 1-2** | 全稿先独立审查/用户批准/冻结，同步向治理追加登记第一批标签定义；两批另取得管理端无 exit node、routes 不变的授权与快照，分别在 Wi-Fi/蜂窝受保护解析 M/S/relay、全部 resolver/STUN 和 peer LAN/公网/hairpin 目的，提前核 hdc 地址与 direct P2P。冻结前下一次另获诊断授权的同一次冒烟，探查 `/proc/net` 本 App uid 可读性及 `/proc/self/net` 替代，并真机核方案 A 的 `/32` 四点：本 App 未保护入 TUN、protect fd 不入 TUN、名单外应用或 hdcd 不入 TUN、成功 create 且 S2/S6 关闭无冲突；发送、收件、TUN/正控留原件。任一点不成立则 A 不得用，只能 B 经 T0 + 用户重审或停测；未获权限绝不代操作。 |
| **T1 第 0 代捕获 / 3-5** | 复用 `endpoint_exclusions_enabled=false`，N2 专属默认关多目标 `/32` 路径及 `trustedApplications=[本 App]` 证据开关（默认关，独立于 S6）；通常冻结 route 不覆盖 hdc，hdc 与 peer/relay LAN 主机重合时仅方案 A 且 T0 真机 (c) 成立可覆盖该主机，否则不开批；管理端不变+无 recreate 监测。假 `VpnConfig` 测两网所有目的并集、排除关闭、S2 `blockedApplications` 与 trusted 互斥、S6 冲突仍拒绝、目标变化/重建即 UNSAT、默认行为不变；主机测试不替代 T0 的 (a)-(d) 真机验证，失败改 B 须 T0 + 用户重审，否则停测。T2/T3 前置。 |
| **T2 socket 库存与当前连接 / 4-6** | 设备逐 fd protect/dup/`getsockname`/外层标识，优先 App uid 四表 `/proc/net/{tcp,udp,tcp6,udp6}` 周期快照对账；不可读改用已在诊断冒烟证实可读的 App 进程内 `/proc/self/net/*` + `/proc/self/fd` inode；两者都不可用则 UNSAT。修复“补给在 recreate 后改用当前连接”并测旧代拒绝。假 socket/fd 复用、IPv6/漏登记、overlay 本地 IP、旧代定时器、条件⑦无漏保护；第一批仅无 recreate 不可免 T2 的库存。T4/E 前置。 |
| **T3 tee 与夹心正控 / 4-6** | 基于 `wg_device.rs` service_tun tee 做全 campaign 目的 IP:port from-OS 扫描/丢帧/盲区报告；同 opener 的隔离未保护 TCP/UDP 正控，不绑定 net handle，注册先发并在核心吞掉。假包测任意源命中即 fail、每事件前后控、正控 3/3、TUN 空窗/缺帧 UNSAT、误外发停测；不建接收器/测试端口。依赖 T1。 |
| **T4 设备/网络 oracle / 5-8** | M/S/R、DNS ID+响应、STUN `XOR-MAPPED-ADDRESS`、Wb/Ws fd/dup/peer 侧发送身份与时钟日志，主机解析已有服务端白名单字段（默认只读本地夹具，**不 SSH**）；假 HTTP/2 共端口、WS/gRPC 结束时间、NAT 改端口、STUN 不符即 UNSAT+风险事件、coturn 缺 transaction ID、同 LAN/hairpin/同出口并发、无 exit node DNS、只有 relay 的 W `not exercised`。依赖 T2/T3；AP/peer/解析器需另授权。 |
| **T5a B 断流刺激 / 1-2** | M/S/R 默认关证据开关仅关闭目标 fd（只证本地关闭/报错路径），I/W 定向断链，D 新查询/超时；网络侧 RST/半开/超时须另授权 AP 阻断原件，不外推。假设备测只影响目标、保留刺激时刻与恢复、三次可复现，不碰服务端/不关 Wi-Fi 冒充 B。依赖 T2/T3/T4，**第一批 T6a 前置**。 |
| **T6a 第一批执行器/判定器 / 3-4** | 默认不连设备/服务器；依赖 T0-T4、T5a。假 HDC/离线日志测第一批 AUTH、哈希/回读/预算/10 次 tconn/时钟 60 s 与 15 分钟、候选配对 ±2000 ms、仅传输中断一次保留原件、campaign 全程漏包即停、28 格全体任一未执行/UNSAT 阻止授予、未获冻结时标签定义登记禁写阶段 pass、禁写 N2 pass。 |
| **T7a 第一批符合性独立审查 / 1-2** | 全稿判据及阶段标签定义先审再冻结；T0-T6a 的工具/配置/授权符合性再审后才能请用户签第一批 AUTH。假原件与工具 hash、无 recreate/无 exit node、权限/可触发性、缺件停止码逐项复核。 |
| **T5b 第二批 recreate/注入/撤销 / 3-5** | 默认关闭的工具证据开关在承载中触发 E 受控 recreate，管理端不变、无 exit node；第 1 代原样移植第 0 代 `/32` 与 `trustedApplications`，逐代核配置/实效路由；故障注入隔离与恢复默认关闭。F 分别支持 App 有序撤销及用户系统设置触发 `onDestroy` 的失序撤销，两路各三次、各 60 s。假设备测 E 当前连接及配置完全一致、F 两路径无旧包、每故障至少一次触发且无裸发；依赖 T2/T3/T4，不将第一批无 recreate 当修复。 |
| **T6b/T7b 第二批执行器及符合性审查 / 2-4 + 1-2** | 依赖 T5b；假 HDC/原件测第二批独立 AUTH、E/F/G 的七行缺件 UNSAT、两批授权和原件不混用、全量审查前禁止 N2 pass。第二批工具**后做后审**，仅审冻结判据的实现符合性，变更判据必须回 T0，用户另行决定第二批 AUTH。 |

## 待用户决定/授权事项

以下是**测前请求决策清单，不是本稿授予的权限**。候选默认值也须独立审查、用户批准并随全文冻结；前提未定或缺授权则不得上机、抓包或改服务器，相应格保持 `UNSAT`。

1. **VPN 范围（N1）**。问题：如何避免全局 `/32` 捕获其他应用造成假泄漏？可选项：A 证据开关下 `trustedApplications=[本 App]`；B 全局 VPN 加固定 `VpnConfig` DNS、其他应用静默及测前写死非 App 命中规则。取舍：A 保住本 App 的不论源判据和同 App 未保护 socket 正控，但主机假配置测试之外，还须冻结前在下一次有诊断授权的冒烟与 `/proc/net` 探查合并真机核同一 `/32`：(a) 本 App 未保护发送进 TUN；(b) protect fd 不进 TUN；(c) 名单外应用或 hdcd 不进 TUN（控制端与 peer/relay LAN 主机重合时必须核 hdcd）；(d) 成功 create 且 S2/S6 都关不冲突，并验证与 S2 `blockedApplications` 互斥及 S6 拒绝关系。负结果须有发送/收件及捕获正控，不可拿未发送充数。任一点不成立则默认 A 不得用，仅可走经 T0 + 用户重审批准的 B 或停测；B 无法凭 TUN 帧识别 uid，削弱判据且须冻结替代 oracle。**默认建议仅在上述真机四点全成立时选 A**。执行：壳/主机工具实施者做默认关开关及假配置测试，获诊断 AUTH 的设备执行者做真机验证，T0 + 用户冻结范围；正式测量仅凭另签 AUTH。
2. **管理端配置冻结与恢复**。问题：谁保持两批设备无 exit node、routes/desired routes 不变并恢复原状态？可选项：获授权管理员在测试窗口只读核实已有状态并冻结变更；若现状不符，由管理员另获明确变更授权后调整并留前后快照/恢复记录；不操作则停测。取舍：前者无写风险但取决于现状，后者有运维变更与恢复责任，不能由证据执行者擅改。**默认建议只读确认现状、由用户指定管理员冻结并在窗口结束复核/恢复；不满足则不开测**。执行：用户指定管理端管理员，独立复核者核快照，设备执行者不登录修改。
3. **治理追加登记标签定义（§二.7）**。问题：何时登记“`N2 第一阶段通过`”？可选项：随全文冻结立即向[治理](native-nx-governance.md)文末追加定义、测后满足门槛才授予；不登记则不得使用标签。取舍：提前登记固定含义但不预先批准结论，测后才新增定义违反测前预注册。**默认建议随冻结追加，拟登记全文如下（本次仅在本稿提出，不改治理文件）**：

   > 追加登记：N2 第一阶段标签定义（随 N2 预注册 v3 测前冻结，只追加不改写）。`N2 第一阶段通过`仅可在已冻结的第一批 A/B/C/D **28 格全体**（测前经 T0 + 用户批准并逐格登记的具体豁免除外）满足逐 socket protect、全 campaign TUN 负证据与逐事件夹心正控、物理出口及独立网络侧 oracle，经第一批工具和原件独立复核、T0 + 用户批准后授予。适用格中任何 `not exercised`、`invalid/UNSAT` 或 `UNSAT` 均阻止授予；不因未实现或未产生可选事件缩减范围。此定义登记不等于标签授予，也不签发 AUTH。第二批 E/F/G 的 recreate、撤销和 fail-closed 尚未因此通过；此标签不是 N2 门结论，不得作为 E8/N6 或任何其他门的输入。`N2 pass/reviewed-pass` 仍须全量条件、独立复核和治理审批；N6 单独判定，任何判据/豁免的变更须测前回 T0 + 用户重新批准并冻结，不得测后追认。

   执行：T0 审查文本，用户批准，治理维护者在冻结时追加原文并登记依据；判定器在第一批完成复核前不得授予。
4. **B 断流方式**。问题：哪一种故障代表 B？可选项：M/S/R 核心证据开关只关目标 fd；经授权 AP 侧定向阻断/恢复用于网络侧断流，I/W 按端点定向断链，D 新查询/超时。取舍：主动 close 可重复、无服务器写入，却只覆盖本地报错重拨，不能外推 RST/半开/超时；AP 更接近网络故障但需抓包/恢复授权。**默认建议本地关 fd 为 M/S/R 第一批路径，并明确限制；I/W、D 按上述类别刺激，另需网络侧结论则事前批准 AP 路径**。执行：工具实施者写默认关开关，用户批准刺激，获授权 AP/设备操作者执行。
5. **E 触发方式**。问题：如何在承载中确定性 recreate？可选项：经工具的默认关证据开关触发受控 recreate；不能实现则 E `UNSAT`、回 T0 重新预注册，不借管理端 routes 或默认路由变化。取舍：开关需主机假设备验证，但可保全原配置与因果；管理端刺激改变冻结条件。**默认建议工具开关，保持无 exit node、routes 不变，第 1 代逐项移植第 0 代 `/32` 和 `trustedApplications`**。执行：壳/工具实施者与主机测试者，用户另签第二批 AUTH 后设备执行者触发。
6. **F 撤销路径**。问题：系统撤销是否纳入？可选项：App 有序停止/关 fd 后 destroy，以及用户从系统设置撤销触发 `onDestroy`。取舍：前者可编排，后者顺序不可编排、要求更强观察；只测前者不能证明系统撤销。**默认建议两路都测、各三次、每次撤销后 60 s**。执行：用户授权系统操作，设备执行者和独立网络侧操作者联合留证。
7. **时钟容差定值**。问题：跨侧如何配对？可选项：校准后 ±2000 ms 且合成误差上界不超过该值；超过则停格回审。取舍：窗口过大增加歧义，过小易受日志粒度/落盘延迟影响；窗口内仍需唯一配对。**默认建议 ±2000 ms，不测后放宽**。执行：采集/判定工具实施者校准并留原始时钟映射，独立复核者核上界。
8. **peer/relay 拓扑与分裂 DNS 测前发现**。问题：两网目的是否一致？可选项：分别在家 Wi-Fi/蜂窝经受保护解析与候选发现 M/S、home relay、STUN、resolver、peer LAN/公网/hairpin 后取并集；发现不全则不开相关格。取舍：并集 `/32` 较多但固定覆盖，单网或只记公网会漏切网和同 LAN 目的。**默认建议双网预发现并集、核 hdc 地址及 §2 重合例外，冻结 direct P2P 与 NAT 映射**。执行：设备/网络侧操作者仅在诊断或 campaign AUTH 内发现，用户指定 peer/路由器操作者协助，T0 审冻结表。
9. **`/proc/net` 可读性探查**。问题：本 App uid 能否读四表？可选项：下一次有诊断授权冒烟分别运行 §2 四条 `hdc shell cat` 命令并通过 App 内默认关诊断实测 uid；不可读时 App 进程内读 `/proc/self/net/*` 并映射 `/proc/self/fd` inode；均不行则 `UNSAT`。取舍：hdc 可读不能证明 App 可读，替代路径须证明完整性。**默认建议冻结前与待决第 1 项 A 方案 (a)-(d) 真机语义验证合并在同一次获授权的诊断冒烟完成**，两路实测并留失败原件；验证失败不得冻结 A。执行：诊断 AUTH 签发者和获授权设备操作者，壳/工具实施者供 App 内只读探针。
10. **`/proc/net` 正式快照白名单**。问题：两批可运行哪些只读命令、频次和预算？可选项：逐条纳入四个 `hdc shell cat /proc/net/{tcp,udp,tcp6,udp6}` 展开的具体命令；若选 App 内路径则登记调用、四表和 `/proc/self/fd` inode 采样；不授权则相应格 `UNSAT`。取舍：逐条授权可审计但增加预算和最小权限核对。**默认建议在诊断可读性确定后选实际可用路径，逐条列入两批 AUTH、固定间隔/次数与脱敏规则**。执行：AUTH 拟稿者列白名单，用户确认，设备执行者只照单执行。
11. **路由器/AP 与 peer 侧抓包**。问题：怎样证明 LAN、hairpin、公网的物理侧独立收件？可选项：授权家中 AP/路由器镜像及 peer 自有主机外侧 UDP 抓包；拒绝或不能唯一配对时相关格 `invalid/UNSAT`。取舍：镜像增进物理来源及 NAT 消歧，但需设备主人、过滤窗口和脱敏权限。**默认建议两侧只读、目标/时窗限定抓包，蜂窝另核运营商 NAT 来源**。执行：用户指定 AP/peer 操作者，独立复核者核匹配，证据执行者不越权抓取。
12. **`192.168.50.6` 查询日志**。问题：该地址当轮是否实际 Wi-Fi resolver 且日志可得？可选项：可控时另获授权读限定查询日志；不可控/未授权则按受保护 fd 同 ID 响应的已冻结 D 替代 oracle。取舍：日志提高独立性但需最小化敏感查询；替代法不应冒称独立 resolver 回执。**默认建议先核身份和权限，可得则读脱敏限定日志，否则用 D 替代并如实标记**。执行：用户指定解析器管理员与设备操作者，独立复核者核对应。
13. **同出口其他 NetBird 客户端**。问题：共享家宽出口如何避免 M/S/R 日志重叠？可选项：窗口内暂停其他客户端；不能暂停则逐个登记身份/时窗并证明唯一排除。取舍：暂停影响其他连接但配对简单；登记较少干扰但同源重叠可能 `UNSAT`。**默认建议经用户许可暂停并记录前后状态，不得由证据执行者擅停；做不到则只在唯一配对成立时测量**。执行：客户端所有者停/恢复，设备执行者登记，独立复核者核服务端元数据。另由用户分别确认蜂窝、两向换网、relay opt-in 和两批独立 AUTH；用户已确认手机有 SIM 可切蜂窝、home.alfadb.cn 的 relay 已启用，**默认建议按两批独立 AUTH 执行并测 Wi-Fi -> 蜂窝及蜂窝 -> Wi-Fi 两向换网**，仍须各批正式授权与确认。

## v3 修订记录（2026-09-28）

在原 v2 文件内改为 v3，未新建文件、未改变其 DRAFT/非授权状态。按 2026-09-28 复审落实：阶段标签测前定义登记与 28 格适用集；STUN 地址不符 `UNSAT` + 风险事件及 coturn transaction ID 当轮核实；App 专属 `trustedApplications` 默认 A 与 S2/S6 主机互斥测试；双网络分裂解析、LAN/hairpin 源地址和无 exit node 出口定义；E 工具开关及原样跨代配置；F 系统撤销 60 s；四表诊断可读性与进程内替代、±2000 ms 配对；B 本地关 fd 不外推网络侧故障。所有实施任务仍待独立审查、用户批准及分别授权。

**v3.1 补记（2026-09-28）**：方案 A 冻结前与 `/proc/net` 探查合并真机核四点，失败转 B 须 T0 + 用户重审或停测；补 SDK/builder 与 0031 依据、hdc/peer 主机重合例外、证据配置不外推产品配置，以及蜂窝/relay 既有确认与两批 AUTH、两向换网的默认建议。不改变 DRAFT/非授权状态。
