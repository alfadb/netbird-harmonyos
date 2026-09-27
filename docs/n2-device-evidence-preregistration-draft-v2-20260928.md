# N2 真机证据预注册草案 v2（2026-09-28）

> **DRAFT / 未生效 / 非设备授权**。本稿作为一份覆盖全部外层类别与全部生命周期列的预注册候选，**取代**[第一阶段草案](n2-device-evidence-preregistration-phase1-draft-20260927.md)与[上游全量草案](n2-device-evidence-preregistration-draft-20260927.md)的操作判据；两份旧稿原样保留，仅作历史输入，不可混用其中相冲突的正控、路由或授权条款。本稿尚须整份独立审查、用户批准及按[治理](native-nx-governance.md)登记冻结，之后才能测量。本文不签发 AUTH，不授权连接设备、采集、运维改配置或修改服务器；0034/0037/0043/0044 均是诊断/预演，不能追认为 N2/N6 证据。

## 1. 一次冻结，分批执行

本次预注册范围为七行 **M 管理 TCP、S 信令 TCP、R relay TCP、D 受保护 DNS UDP、I ICE/STUN UDP、Wb WG base UDP、Ws ICE 选中后供 WG 的 UDP socket/dup**，与 **A 首连、B 断流后的重连/重试、C Wi-Fi -> 蜂窝、D 蜂窝 -> Wi-Fi、E 承载中受控 recreate、F 最终 VPN 撤销、G fail-closed 注入**的全部交叉格。§4 一次写定各格刺激和 oracle；§3、§5 的阈值、停止条件和不外推规则同时适用。R 须 opt-in 且有真实 relay 会话；Ws 只有确实选中本地 socket、交 WG 并从该 socket 发出 WG 外层包才算 exercised。实现需求要求但未实现的 TURN、relay QUIC/UDP、IPv6 外层等能力记 **`UNSAT`**，先实现并回 T0 更新预注册，不能以 `not implemented` 从门范围中扣除。实际未产生的可选单次事件记 `not exercised`，必要格据此 `UNSAT`；出现新的外层类别立即停测、回 T0，不用既有七行外推。

- **第一批 AUTH** 仅执行 A/B/C/D 的 28 格，使用 §2 冻结的第 0 代 `VpnConfig` 和无 recreate 配置；相关格出现 recreate 就 `invalid/UNSAT`，保留原件，不换用新路由补做通过。
- **第二批 AUTH** 另行签发，执行 E/F/G 所有七行；工具可在第一批后实现、测试并独立审查，但 **E/F/G 的判据、oracle、停止条件现在即随全稿冻结**。工具的版本/hash 与实施符合性须在第二批 AUTH 前再独立审查；任何改动判据、范围或阈值均须先回 T0、再取得用户批准并重新冻结，不得测后追认。两个 AUTH 预算和原件互不挪用。
- 全部适用第一批格满足本稿判据、第一批工具审查和治理追加登记经 T0 + 用户批准后，**最多**登记“`N2 第一阶段通过`”；这只是中间里程碑，**不是门结论，不得作为 E8/N6 或其他门的输入**，也不写 `N2 pass/reviewed-pass`。第二批全量通过及全量独立复核后才可依治理提出 N2 门结论。N6 按自身内层连通及默认路由判据另立 verdict；同轮原件可以共用，结论绝不互相代替。未获上述标签登记批准时仅写“第一批格已测/待审”，不得自行创造阶段 pass。

## 2. 测前冻结、代际与覆盖

**第一批管理端运维配置须由用户另行授权冻结**：测试设备无 exit node，关闭会变化的 NetBird routes，保持 desired 路由集在 A/B/C/D 和两向切网中不变；记录管理端快照、版本、route/peer 公告及差异监测，不擅自改服务器。默认路由在 peer 登记并 `tunnel_ready` 之前不会放开（`client/core/src/connector.rs` 约 41 行），而产品放开 `0.0.0.0/0` 必触发受控 recreate。因此第一批**不依赖默认路由**：建 VPN 时第 0 代 `VpnConfig` 即下发非排除的定向捕获路由，对全部当轮冻结的外层目的 IPv4 各设 `/32`：管理/信令、relay 公网 IP、两个 STUN、Wi-Fi resolver、预先发现的蜂窝 resolver、peer 公网 IP（重叠去重）。实际解析 IP:port、协议和外层目的集合均冻结；随解析或蜂窝 resolver/peer/relay 地址变化而不再覆盖则停相应格 `UNSAT`，不能测后扩路由。逐次核实实下发配置/生效路由、VPN 应用归属及这些目标确在路由内，且捕获路由**不覆盖 hdc 控制端地址**；无法同时满足则不开第一批。不可使用 `force_default_route=true`、S6 旧诊断捕获路由或端点排除来假造覆盖。

M/S 的 create 前首次 TCP `connect`/发包以及 create 前其他类别的包处于无 VPN 路由阶段，TUN 零命中是平凡事实，只作时序与物理出口记录，**不计为**受路由覆盖的首包负证据。A 格必须进一步在 VPN 已 create、tee 已 attach 且第 0 代路由实际覆盖时，沿用对应 pre-create 已保护 fd 发真实第一批业务包并做夹心正控；无法做到的 A 格 `UNSAT`。凡在 attach 之前的帧不可见，不能把 create 前后两段拼作连续 TUN 捕获。B/C/D 同样须保持该代配置；任何目标路由变化、增删 peer/routes 引起的 recreate，按第一批 `UNSAT` 停格，留待第二批 E 的单独试验。

在第二批 E，允许**自然就绪后** `0.0.0.0/0` 放开或经预审的覆盖路由，必须记录旧/新代配置及实际覆盖，不强行抢开默认路由；重验证跨代存活 fd 和新补给。补给/补池定时器若仍持有首代 `VpnConnection`，recreate 后请求全被拒绝，相关格不可测。产品前置为“**补给在 recreate 后改用当前连接**”并有主机回归；第一批的无 recreate 配置加逐次代际检测只能隔离该缺陷，**不能**算修复或 E/G 通过，若第一批出现 recreate 则 B/C/D 相关格 `UNSAT`。

每批冻结：专属 AUTH/sidecar 与用户回读确认、唯一执行者、设备脱敏 ID/型号/完整固件 SDK、App/VPN 包名、HAP/core/collector/runner/judge SHA-256、服务端版本及只读日志权限、relay opt-in、两侧物理网络句柄/接口和本地 IP、VPN/TUN 句柄、overlay IP、route generation、控制端 IP、所有实际外层目的及解析来源、fd/dup/socket ID、`getsockname` 的设备本地地址和可得 NAT 映射、UTC/单调时钟及误差上界、捕获起止/丢帧/过滤条件、原件路径与 hash。**物理出口 IP 必须与任何隧道出口 IP 不同**，否则无法排除隧道代发，停相关格 `UNSAT`；共享家宽出口须消歧。历史 `8.139.6.47`、`175.10.221.251`、`192.168.50.6` 及 overlay 值仅是线索，不预填本轮结果。

测前干净进程：经新 AUTH 授权的 force-stop 或重装后启动，记录本进程生命周期内 **未调用** `protectProcessNet`（S3 诊断开关关闭）、未设置 `blockedApplications`，`netbird-protect-deviation` 文件缺席、无 `VPN_PROTECT_DEVIATION_ARMED`，S2-S6 诊断臂全关。现有 `endpoint_exclusions_enabled` 默认 true，N2 T1 **复用此开关显式设 false**，不新增同义关闭开关；证明实际路由无 `/32`/`/128` 端点排除，initial 与第二批 recreate 两腿均检查。扩展拒绝“排除关 + S6 捕获路由”的既有组合，N2 多目标 `/32` 必须独立于 S6 新路径、默认关闭，并在主机测试验证拒绝关系未被绕过。对 app uid 定期按授权白名单读取 `/proc/net/{tcp,udp,tcp6,udp6}` 全部四表，与设备登记的**全部外层 socket**（含备用/重拨/dup、IPv6；必要时结合 fd inode 关联）对账；快照、时间、PID/UID/inode、遗漏和权限失败留原件。捕获路由生效前及期间，须确认所有外层 socket 已逐 fd protect，满足治理条件⑦；未知 socket、快照不可读或未保护者停测，未保护首发见 §5。清点不把非外层 socket 误判，也不把尚未分类的外层 socket 放过。

## 3. 共用证据与冻结判据

每个可操作的正向 A/B/C/D/E 格至少 **3 次独立可归属的真实生产发送事件**；A 为三个独立首连会话，B 为三个真实断流后的新 dial/查询/检查，C/D 各三次真实方向切换及新网络出包，E 为三次承载中 destroy/create 及跨代/新供给出包。F 三次授权撤销观察窗；G 每个指定故障分支至少一次且证明注入确实生效。三次是最小观察数，**不作统计安全保证**。一次缺件不能用追加好样本掩盖；只在授权规定的传输中断例外中保留两次原件与预算并重跑一次。每事件均须有以下三证及正控：

1. **设备侧**逐 fd/dup 身份链 `open -> protect_begin -> settle(success) -> 首次 connect/UDP send`，后续发送、网络 generation、当前物理句柄和 `getsockname` 的本地 IP:port；本地 IP 等于 overlay IP **直接 fail**，不能拿服务端来源来洗掉。失败后绝不裸发。TCP 已建连接可跨多次请求/出包，但不能把状态行计作独立发包。
2. **本机 TUN** 在 WG 设备 `service_tun` 读循环（`client/core/src/wg_device.rs` 约 997-1011）的 from-OS/to-OS tee 保留非空扫描、原始帧、丢帧/解码计数与逐帧判定；该挂点在 attach 前后有盲区（同文件约 355-356）。对每个**生产事件紧前和紧后各发一个**正控 marker，两个都须在 from-OS 命中，时间顺序为 `前控命中 -> 生产包 -> 后控命中`，构成仅对该事件的可观察夹心；另每格至少三个可归属的正控 marker 且 **3/3 命中**（逐事件夹心的六个或更多 marker 全须命中）。无前/后控、窗口空白、帧丢失/解码失败均为 `invalid/UNSAT`，不能外推其间未知帧。冻结**从 VPN create 且 tee attach 到 destroy 的全 campaign 可观测窗口**的外层目的 IP:port 集合及版本：对该窗口全部 from-OS 帧按目的 IP:port 匹配，**不论源地址或设备临时源端口**，只要落在冻结外层集合就是生产泄漏命中；正控帧单独标识并由 collector 在核心吞掉，不计作生产泄漏。整个可观测窗口持续扫描，包括格与格之间，任何非正控的命中均立即 `fail/leak`，不能靠逐 fd 五元组筛掉未登记流量。create 前和 destroy 后不声称 tee 连续，另以设备 fd/物理侧记录判断；若正控本身流出物理口或未被核心丢弃则立刻停测并记 `invalid/UNSAT`/风险事件。
3. **物理出口 + 独立网络侧**：设备物理接口的发送元数据与实际目标侧/AP/peer 抓包或服务端可核对入口闭合，按冻结的时间误差、目标、连接区间、请求顺序/transaction ID 和 NAT 后来源消歧；**D 的独立 resolver 不可观测时仅适用 §4 明确冻结的无 exit node、受保护 fd 收到同 ID 响应替代 oracle**。纯 `protect` resolve、应用成功、TUN 零计数、物理 NIC 总字节或单方服务器日志均不足以证明同一事件。物理口受限须记能力缺口并由独立 AP/路由器镜像等补齐（D 的替代 oracle 除外）；不能唯一配对则 `invalid/UNSAT`。Wi-Fi 格优先申请家中路由器/AP 抓包；蜂窝另核运营商实际公网来源。

正控由同一 App 进程、同一代 `VpnConfig`、同一当前网络的隔离 socket 产生，只比生产 socket **少一次 protect**；TCP 调用与 M/S/R 生产相同的 `mgmtSocketOpen`，UDP 与 D/I/Wb/Ws 生产相同的 `iceSocketOpen`（若某类的实际生产 opener 不同，须先证明开口等价并经独立审查，不能假称已复用），不绑定 net handle。预注册 marker ID/目标 IP:port/协议、socket ID 和预期 tuple **先于发送**；UDP 用非敏感 nonce，TCP 可用单独连接 SYN 的 tuple/事件 ID 作 marker，不向认证服务写测试载荷。目标为冻结外层地址且确受路由覆盖；collector 在 WG 处理前匹配、记录并丢弃 from-OS 正控，**不要求远端独立接收器或受控测试端口，更不要求到达远端**。正控出口被放行而有外发风险时立即停测；注册/丢弃先后和生产隔离须主机反例测试。DNS 正控亦只验证同代同路由的捕获能力，DNS 响应另按 §4 的替代 oracle。

阈值为正向格 **生产 3/3 设备、TUN、物理+独立网络三证齐全，整个 campaign 生产外层 TUN 零命中，每个事件夹心正控前后均命中**；任一生产外层入 TUN 或未保护首发即 fail，不因三次干净事件抵消。时间窗在测前按 UTC/单调偏差与误差上界冻结（候选 ±2 秒，超出即回审，不在测后放宽）；断流结束的落盘日志须用 `ts - duration` 反推开始。DNS 的独立物理侧替代判据见 §4，不伪称拥有 resolver 日志。

## 4. 七行 × 全部列的刺激与 oracle

每格都叠加 §2 的配置/清点及 §3 的判据，G 还叠加 §5 的 fail-closed 列表。`not exercised` 必须有原因，不能记 pass。E 须同时观测旧存活 socket 跨代和按需**新** socket/ICE 补池；F 停 sender/connector 并关 fd 后撤销，撤销后无 TUN 时以 fd/物理口/独立网络侧短窗证明无旧 socket 外发，不能用 TUN 零命中充数。下面“同向”仍分别采样、独立裁决。

| 行 / 独立网络侧 oracle | A 首连 | B 断流/重试 | C Wi-Fi -> 蜂窝 | D 蜂窝 -> Wi-Fi | E recreate | F 撤销 | G 故障注入 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **M TCP**；netcenter Caddy `access-api.log` 的管理入口 | 初次 dial；pre-create fd 在 create 后继续真实出包 | 已授权独立断流后三次新 `purpose=management` dial，含队列耗尽补给与 Login/Sync | 新物理源上的实际管理发送/入口 | 同向反转 | 存活管理连接与新补给，当前 connection protect | 停 Sync/补给、关 fd，撤销后无旧管理包 | 初始保护失败拒 create/裸拨；补给失败和代际拒绝后无裸拨 |
| **S TCP**；netcenter Caddy `access-signal.log` | 初次 feed/create 后继续发包 | 真实 signal 流断后新 socket/重注册 | 新网络 signal 流/入口 | 同向反转 | 存活流及新 signal feed/注册 | 停流、关 fd，撤销后无旧 signal 包 | 初始 feed 失败停 connector；重拨保护失败无裸连 |
| **R TCP**；home Caddy `/apps/netbird/data/caddy/access.log` 中 `/relay`，真实 opt-in | 真实 TLS/WSS 会话、create 后发包 | 真实 relay 断流后新 `purpose=relay` socket/会话 | 新网络 relay 会话/入口 | 同向反转 | 旧会话收尾与新受保护 relay | 关会话/fd，撤销后无旧 relay 包 | relay 补给/保护失败无裸 TLS/WSS |
| **D DNS UDP**；已保护 fd 收到匹配响应 | 三次 relay/STUN 新查询；初始系统解析另盘点 | 新查询/新 fd、响应，缓存命中不算 | 蜂窝 resolver 重发现并与冻结值核对后查询 | Wi-Fi resolver 新查询 | 当前连接上重新获取受保护 DNS fd/新查询，不假定旧 fd 常驻 | 停查询者、关 fd，撤销后无旧 fd 查询 | 无 resolver/物理句柄/保护失败时拒查询及系统解析回退 |
| **I ICE/STUN UDP**；实际目标侧 coturn `New UDP endpoint`/`BINDING` | 两个 STUN 目标逐一登记是否实际发送，三次 Binding/check | 新检查/补池 fd 与 transaction ID，未实际发包为 UNSAT | 新网络候选、映射、响应 | 同向反转 | 旧 ICE socket 与当前连接新补池、检查 | 停 ICE/关池，撤销后无旧 STUN 包 | 池单 fd 失败只减候选，全池失败拒 gather/裸发 |
| **Wb WG base UDP**；真实 peer 自有主机外侧 UDP 抓包 | `wg_fwd_open` 原 fd/dup 出包，create 后继续 | 真实断链后握手/包，换 fd 重保护 | 新网络的 base 包与 peer 收件 | 同向反转 | 存活 base fd 与必要的新受保护 fd | 停 WG 发送/关 fd，撤销后无旧 WG 包 | base 保护/feed 失败销毁 VPN、无裸 WG |
| **Ws WG selected UDP**；同一 peer 外侧 UDP 抓包 + 选中到 WG dup 身份图 | 选中并由该 socket 发 WG 包 | 真实重选/重连后选中 fd 出包 | 新网络选中 fd 的 WG 包 | 同向反转 | 选中旧 fd/新选中 fd 跨代、当前连接重保护 | 停选中 socket/WG 并关 dup，撤销后无旧 WG 包 | 选中 fd/dup 保护或关闭失败后不交 WG、无裸发 |

**B 列刺激先冻结再授权**：M/S/R 可选择证据开关下仅核心主动关闭该类已登记 fd、保留 close 时刻和新 fd 身份，或用户授权的 AP 侧仅对该流目的实施定向阻断/恢复并留网络侧原件；I/W 用同等受控断链/端点阻断触发重检/握手，D 用已授权的新查询/超时重试。不可用服务端断连（无服务器写授权）、不可关 Wi-Fi 冒充 B（与 C/D 混淆），也不可将 App 重启当新拨或跨代。刺激影响范围、恢复步骤、次数写入各批 AUTH；不可复现或不足三次则格 `UNSAT`。

**独立 oracle 细则**：M/S 在 netcenter（历史公网 `8.139.6.47`，当轮核实），R 在 home（历史 `rels://home.alfadb.cn:28443`，不是云端备用 relay），按实际发生的远端/日志逐连接对齐。HTTP/2 多请求可能共用**同一** `remote_port`/连接，不能把端口相等当多次新 socket；gRPC 长流与 WebSocket 在结束落盘时用 `ts - duration` 反推连接起点。测试窗口内其他同出口 NetBird 客户端须停止或逐个登记并可排除；做不到而同源多候选重叠，S 等格 `UNSAT`。设备本地端口不必等于 NAT 后 `remote_port`；家宽保留源端口只可在**当轮实测确认**后作附加配对，不作为默认假设。服务器日志只取白名单元数据，不能导出 headers/认证载荷；两侧 coturn 有空间轮转且扫描噪声，缺失/歧义即 UNSAT。

I 的设备侧须记录 STUN response 的 `XOR-MAPPED-ADDRESS`；与当轮 coturn `remote addr IP:port` **逐字相等**且 transaction ID/时间/目的可核对，不相等则 fail 或歧义 UNSAT（先保留两端原件并判明是否归属错误），不能只靠 `BINDING` 数量；Ws 若沿用同一选中 socket，可继承其已证映射，但仍须 WG peer 侧独立收件。D 在**无 exit node**前提下使用明确替代判据：受保护 DNS fd 记录发出查询并收到同 ID 响应、该事件前后正控 from-OS 命中、campaign TUN 零泄漏，结合物理网络句柄和实际 resolver 目的；不再强求独立 resolver 回执。第二批 E/F/G 若自然放开默认路由，仍须证明无 exit node 才可沿用此 DNS 判据，否则 D 格 `UNSAT` 并回 T0 审新判据，不得现场补规则。Wi-Fi 若 `192.168.50.6` 确是用户可控解析器，优先另取其查询日志强化；蜂窝 resolver 须测前发现/冻结，缓存无查询为 `not exercised`。Wb/Ws 在 peer **自有主机的物理外侧**抓实际到达 UDP，来源应为设备 NAT 后当轮公网地址；须测前确认能建立 direct P2P，只有 relay 路径则 Wb/Ws 的直达 peer 收件格记 `not exercised`，不得用 coturn、内层 peer echo 或 relay 收件替代。

## 5. 第二批 E/F/G 冻结注入与停测规则

G 按可触达类别逐一注入：`protect` reject、壳 5 s timeout、late resolve/陈旧 request ID、核心 supply 8 s timeout、物理 DNS/网络句柄缺失、fd close/dup 失败；可实现且预审的“resolve 成功但平台生效延迟”单列，确有未保护包即 fail 并回 T0。每个指定分支至少一次保留**故障确实触发**的状态、fd/dup/代际顺序与前后夹心正控及全程物理/TUN 原件；没有外层包的注入以当前覆盖路由下夹心正控与 fd/物理侧短窗证明无裸发，不以错误日志代替。失败 fd 不复用，恢复必须新建并重新 protect；跨 recreate 所有补给/补池使用**当前** `VpnConnection`，过渡代请求拒绝，旧 generation 不继续裸发。若某注入使 VPN/tee 不存在，预先的保护与路由有效窗和物理/独立侧无包、关闭顺序作为该子格的负证据，不能声称不存在的 TUN 正控；不能观测则 `UNSAT`。

F 的授权撤销按停止 connector、ICE、WG 和补给发送者 -> 确认 fd/dup 全关闭 -> `destroy` -> 固定 **60 s** 物理口与独立网络侧观察窗进行；三次独立撤销。窗口不能持续观察/接收侧无可归属记录则 `UNSAT`，撤销后旧外层出包即 `fail`；恢复/重连在新授权范围内重新创建和 protect，不能拿撤销后天然无 TUN 证明无泄漏。E 的旧/新代路由覆盖、正控与 socket 图分别记录，任何中途不可见的帧不能跨窗外推。

发现任何真实生产外层包匹配冻结目的集合进入 from-OS TUN、未保护首发/保护失败后裸发、设备 `getsockname` 为 overlay，立即保存原件、`fail` 并停止本轮；同一格后续干净事件不抵消。缺 AUTH、用户确认、管理端冻结、干净进程、路由/hdc 隔离、socket 清点、夹心正控、实际出包、物理/网络侧 oracle、时钟或 tee 连续性，标 `invalid/UNSAT` 并停受影响格；泄漏风险、未知外层能力或代际失效可疑则扩大为全轮停测并交 T0/用户。不得用 N2-H 端点排除、`protectProcessNet`、VPN 业务成功或 N6 内层流量替代逐 fd protect；Wi-Fi 不外推蜂窝，第一批不外推 E/F/G，三次零命中不证明未来永不泄漏。

## 6. 两份 AUTH、制品与冻结流程

在全稿判据独立审查、用户批准和治理冻结之前，**两批都不得执行**。每批独立的 campaign 类 AUTH 必须逐项写设备/无线目标当轮核对、HDC argv 白名单（含在线查询、最多 10 次有间隔 `tconn`、只读设备时钟、经批准的 `/proc/net` 四表快照及 force-stop/重装若选用）、产物与 sidecar/hash、管理端配置证据、逐格允许刺激与切网/注入/撤销、采样命令、时间窗、预算上限和跨尝试累计、单一执行者、回滚、日志最小化与停止码。AUTH 签发后主会话回读并取用户**明确确认**，确认前不得发设备命令，连 `hdc tconn` 也不行。新 AUTH 适用治理 2026-09-27 追加登记：无固定 `valid_from/valid_until`，离线最多 10 次连接后暂停且不自动消费，传输中断仅该格最多重跑一次并留两份原件，结果不理想不能重跑；连接后核设备/主机时钟差 ≤60 s、每 15 分钟重比对，设备锁定/时间不可读/大版本变化或 hash 失配即停；跨会话续用重新回读确认、核时与 hash。第二批另做工具符合性审查、签新 AUTH，绝不沿用第一批授权。未经额外授权不登录服务器、不改日志配置、不抓 AP/peer 或读取原始敏感日志。

每格保留原始 TUN/物理/AP/peer/服务端白名单日志、socket 清点与时间快照、配置与代际变化、正控/生产对应表、过滤器和丢帧报告、sha256 清单、无效尝试和恢复记录。凭据、JWT、setup key、证书/密钥与敏感载荷不入日志；日志不足不补造。第一批 verdict 与第二批 verdict 独立归档，判定器禁止中途写 N2 门 pass。

## 7. 实施任务清单（未来工作；本次不改代码）

估算为工程人日，不含审批、真机、运维、等待或新能力开发；全部诊断开关、路由、tee、注入、网络命令**默认关闭**，主机验收用假设备和离线原件。`client/entry/selfcheck/` 有并行执行者，后续实施另行协调，本次不碰 `client/`。

| 顺序 / 规模 | 依赖与交付（主机验收） |
| --- | --- |
| **T0 预注册与运维前提 / 1-2** | 全稿先独立审查/用户批准/冻结，第一批另取得管理端无 exit node、routes 不变的授权与快照，提前发现两个 resolver、peer/direct P2P 和 hdc 地址；审查捕获目标/源出口不混淆，未获权限绝不代操作。 |
| **T1 第 0 代捕获 / 3-5** | 复用 `endpoint_exclusions_enabled=false`，N2 专属默认关多目标 `/32` 路径独立于 S6；冻结 route 不覆盖 hdc；管理端不变+无 recreate 监测。假 `VpnConfig` 测两个 resolver/两个 STUN/peer/relay/M/S 全覆盖、排除关闭、S6 冲突仍拒绝、目标变化/重建即 UNSAT、默认行为不变。T2/T3 前置。 |
| **T2 socket 库存与当前连接 / 4-6** | 设备逐 fd protect/dup/`getsockname`/外层标识、`/proc/net/{tcp,udp,tcp6,udp6}` uid/inode 周期快照对账；修复“补给在 recreate 后改用当前连接”并测旧代拒绝。假 socket/fd 复用、IPv6/漏登记、overlay 本地 IP、旧代定时器、条件⑦无漏保护；第一批仅无 recreate 不可免 T2 的库存。T4/E 前置。 |
| **T3 tee 与夹心正控 / 4-6** | 基于 `wg_device.rs` service_tun tee 做全 campaign 目的 IP:port from-OS 扫描/丢帧/盲区报告；同 opener 的隔离未保护 TCP/UDP 正控，不绑定 net handle，注册先发并在核心吞掉。假包测任意源命中即 fail、每事件前后控、正控 3/3、TUN 空窗/缺帧 UNSAT、误外发停测；不建接收器/测试端口。依赖 T1。 |
| **T4 设备/网络 oracle / 5-8** | M/S/R、DNS ID+响应、STUN `XOR-MAPPED-ADDRESS`、Wb/Ws fd/dup/peer 侧发送身份与时钟日志，主机解析已有服务端白名单字段（默认只读本地夹具，**不 SSH**）；假 HTTP/2 共端口、WS/gRPC 结束时间、NAT 改端口、STUN 逐字不符、同出口并发、无 exit node DNS、只有 relay 的 W `not exercised`。依赖 T2/T3；AP/peer/解析器需另授权。 |
| **T5a B 断流刺激 / 1-2** | 明确并审查主动关 fd 或 AP 定向阻断的 B 手段，默认关闭；假设备测只关闭目标 fd/目标流、保留刺激时刻与恢复、三次可复现，不碰服务端/不关 Wi-Fi 冒充 B。依赖 T2/T3/T4，**第一批 T6a 前置**。 |
| **T6a 第一批执行器/判定器 / 3-4** | 默认不连设备/服务器；依赖 T0-T4、T5a。假 HDC/离线日志测第一批 AUTH、哈希/回读/预算/10 次 tconn/时钟 60 s 与 15 分钟、仅传输中断一次保留原件、campaign 全程漏包即停、28 格缺件 UNSAT、未获登记禁写阶段 pass、禁写 N2 pass。 |
| **T7a 第一批符合性独立审查 / 1-2** | 全稿判据先审再冻结；T0-T6a 的工具/配置/授权符合性再审后才能请用户签第一批 AUTH。假原件与工具 hash、无 recreate/无 exit node、权限/可触发性、缺件停止码逐项复核。 |
| **T5b 第二批 recreate/注入/撤销 / 3-5** | 可在第一批后实现，但判据已冻结；E/F/G 的受控 recreate、关闭/撤销、故障注入隔离与恢复默认关闭。假设备测 E 当前连接、F 60 s 无旧包、每故障至少一次触发且无裸发；依赖 T2/T3/T4，不将第一批无 recreate 当修复。 |
| **T6b/T7b 第二批执行器及符合性审查 / 2-4 + 1-2** | 依赖 T5b；假 HDC/原件测第二批独立 AUTH、E/F/G 的七行缺件 UNSAT、两批授权和原件不混用、全量审查前禁止 N2 pass。第二批工具**后做后审**，仅审冻结判据的实现符合性，变更判据必须回 T0，用户另行决定第二批 AUTH。 |

## 待用户决定/授权事项

以下是**请求决策的清单，不是本稿授予的权限**；任一未决前提对应格保持 `UNSAT`，不得先上机或改服务器。

1. **管理端配置冻结**：由谁在获授权后把测试设备设为无 exit node、无会变化的 routes；具体冻结/核对/恢复方法、操作窗口与只读快照权限，确保第一批第 0 代 desired routes 不变且不 recreate。
2. **治理追加登记标签**：请 T0 与用户决定是否在第一批真实完成并独立复核后，向治理文档文末**另行**追加：“`N2 第一阶段通过`仅表示冻结预注册中第一批 A/B/C/D 的全部适用格满足逐 socket protect、TUN 负证据/夹心正控及物理出口 oracle，并经独立复核；第二批 recreate/撤销/fail-closed 尚未因此通过。此标签不是 N2 门结论，不得作为 E8/N6 或任何其他门的输入；N2 pass 仍须全量条件和治理审批。”**本次不写入治理文件**，未批准/未达证据门槛不得登记。
3. **B 断流方式**：选择且授权证据开关下核心仅关闭目标 fd，或 AP 对目标流定向阻断/恢复（可按类别不同）；写清刺激对象、原件、次数和恢复，不授权服务器断连或关 Wi-Fi 代替。
4. **设备快照命令**：允许的 `/proc/net/tcp`、`udp`、`tcp6`、`udp6` 只读命令/采样频率与次数入两批 AUTH 白名单，及 app uid/inode 对账所需最小权限；不可读取则相关格 UNSAT。
5. **物理抓包权限**：是否允许家中路由器/AP 只读抓包、peer 自有主机外侧 UDP 抓包（各自操作者、目标、保存/脱敏窗口）；确认 peer 能打出 direct P2P，否则 Wb/Ws 直连格 `not exercised`。
6. **解析器日志**：当轮 Wi-Fi resolver 是否确为用户可控 `192.168.50.6`，若是是否能启用/读取限定查询日志；不批准时 D 按 §4 的受保护 fd ID 匹配响应替代判据，不能称已有独立 resolver 日志。
7. **同出口其他 NetBird 客户端**：测试窗口是否暂停它们，或提供逐个身份/时窗登记供消歧；同源无法唯一配对（尤其 S）则 UNSAT。另确认蜂窝可用、两向换网与 relay opt-in 的授权，及两批分别签发 AUTH 的意愿。
