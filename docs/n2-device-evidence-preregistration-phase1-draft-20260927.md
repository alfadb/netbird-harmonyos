# N2 真机证据预注册：第一阶段草案（2026-09-27）

> **DRAFT / 未生效 / 非设备授权**。本文件是在[原十节草案](n2-device-evidence-preregistration-draft-20260927.md)上收敛的第一阶段候选，仅在第一阶段范围内拟取代其操作判据；原稿原样保留，第二阶段继续参考其中的故障注入、recreate、撤销条款。判据、工具和制品须先冻结，经独立审查、用户明确批准、治理登记后才可能生效。本稿不签发 AUTH，不允许连接设备、采集或把旧诊断轮追认为 N2/N6 证据。

## 1. 范围及结论边界

第一阶段逐 socket 证明 `open -> protect(fd) 成功且生效 -> 首次 connect/UDP send`，并在实际 VPN 覆盖其目的地的条件下，证实已保护的真实外层包不进入 `vpn-tun` 而走当前物理网络；fd dup、新建拨号 fd、ICE 选中后交 WG 的 fd 分别归属。[时序等价论证](n2-protect-ordering-equivalence-20260927.md)仍是有条件的书面论证，须独立审查并由物理证据验证；Promise resolve、host test 与应用收发成功均非出口证明。

| 类别 | 生命周期 | 不在本阶段 |
| --- | --- | --- |
| M management TCP、S signal TCP、R relay TCP、D 受保护 DNS UDP、I ICE/STUN UDP、Wb WG base UDP、Ws ICE 选中后供 WG 的 UDP socket/dup | A 首连；B 真实断流重连/新查询或检查；C Wi-Fi -> 蜂窝；D 蜂窝 -> Wi-Fi | fail-closed 故障注入、承载中受控 destroy/create、最终 VPN revoke 及复验：第二阶段。TURN、relay QUIC/UDP、IPv6 外层当前逐项 `not implemented`；若当轮发现这些真实流量，停测重审库存。 |

R 必须真实 opt-in 且有可观测 relay 会话，不能以“可选”免测。D 对首连壳侧 management/signal 系统解析另列清单：create 前发生不能计作 D protect 通过；已建 VPN 下若由不可持有且不可保护的系统 socket 发出，登记 `deviation/UNSAT` 并回 T0+用户。IP 字面量没有查询则 D=`not exercised`。Wb/Ws 分开判：只有 ICE 确实选中本地 socket、向 WG 交付并用该 socket 实际发送外层包才可测 Ws；单有 ICE check 或 Wb 通过不能代替。TURN、relay QUIC/UDP、IPv6 外层虽记 `not implemented`，并非 pass：若部署需求实际要求其中任一能力，先实现并补预注册，否则阶段结论保持 `UNSAT`。

全部纳入且适用的第一阶段类别 x 生命周期格通过、独立审查及治理登记后，最多写 **“N2 第一阶段通过”**；必要格缺件写 `N2 第一阶段 UNSAT/inconclusive`，泄漏或未保护首发写 `N2 第一阶段 fail`。第一阶段**绝不**写 `N2 pass/reviewed-pass`；两阶段都通过才可依治理程序提出 N2 pass。N6 另按内层连通与默认路由判据独立裁决，允许共用原件，不许互相代替。

## 2. 测前冻结、路由覆盖与归属

当轮冻结并逐格引用：AUTH ID/回读确认/sidecar、唯一执行者；设备脱敏 ID、型号、完整固件/SDK、App 与 VPN 包名；HAP/core/采集器/执行器/判定器 SHA-256 和版本；配置、relay opt-in 与所有实际外层类别；Wi-Fi/AP、蜂窝、物理接口/网络句柄与源 IP、VPN 句柄、`vpn-tun`；服务端版本、实际宿主和日志可见性；实际管理/信令/relay/DNS/STUN/peer 域名、解析 IP:port、协议、fd/dup 身份、设备本地 IP:port、观察到的 NAT 映射。每格保存 UTC 与单调时钟/偏差上界、原始路径与 SHA-256、捕获起止/丢包数、实下发 `VpnConfig`、生效路由、控制标记。历史设备 overlay `100.108.144.165`、peer overlay `100.108.126.29` 和 0043 域名/IP 仅供核对，不预填本轮结论。日志不得包含 JWT、key、token、证书内容或敏感载荷。

必须冻结证实：所有被测目的 IP 受实际 VPN 路由覆盖，端点 `/32`/`/128` 排除关、`protectProcessNet` 关、`blockedApplications` 未设置、开发期 `netbird-protect-deviation` 缺席且无 `VPN_PROTECT_DEVIATION_ARMED`；原 S2-S6 诊断臂（含旧 capture route）均关。**N2 专用捕获路由**为新增、默认关、仅随专属授权为具名证据格开启，不借旧诊断臂绕过条件。优先使用产品在可用 peer/WG dataplane 就绪后安全放行的 `0.0.0.0/0`；否则仅允许预审的非排除定向路由覆盖目标，不准 `force_default_route=true` 或抢开可能黑洞的默认路由。初连和换网后检查新 `VpnConfig`、应用归属及路由无残留排除/进程旁路。目标没有覆盖、无法建立同路由正控或正控未入 TUN，一律 `invalid/UNSAT`，不能用 TUN 零命中声称 protect 生效。

## 3. 每格共同判据、次数与裁决

**格 = 类别 x A/B/C/D；Wb、Ws 分列为两行，共七行四列。**可共用同一次原件，不可共用“已经通过”的结论。每个可操作格至少 **3 次逐事件可识别的真实生产出包**；A 是 3 个独立首连会话，B 是 3 个真实断开后的新 socket/查询/检查/握手（不把重启伪装成重拨）；C 和 D 各 3 次实际网络转换及新网络上的包，蜂窝不可用即列 `UNSAT`。存活 TCP socket 可在同一转移内多次发包，仍须每一事件独立可归属，不把三个状态日志计成出包。每格另在**同一 App/VPN 网络上下文、同一目标 IP/协议与覆盖路由**上，用隔离的未保护测试 socket 发 3 个独特且非敏感的 marker，**3/3 命中本机 `vpn-tun` from-OS**；测试端口和独立接收器预先取得控制权并登记，正控须 3/3 入本机 TUN 且有接收器可归属回执；接收器不可达则该格 `invalid/UNSAT`，保留真实 TUN 帧但不补造回执。不能复用生产 fd、把正控事件算生产出口或向生产认证端口发测试载荷。若同目标 IP 不允许设置受控测试端口，则该格 `UNSAT`，不私自向无主端口发包。同目标 TCP/TLS 若无隔离端口，不以不同目标的可控 TCP SYN/连接或仅有路由快照替代同目标正控。

每次生产事件须同时满足：

1. 同一 fd/dup/socket ID 的 `protect_begin -> settle(success) -> first connect/send` 有序且能核对首包的物理出站，pre-create 的 TCP fd 在 VPN create **以后**仍发真实包并落入捕获覆盖期；仅 create 前天然无 TUN 不算。
2. TUN from-OS/to-OS tee 在登记匹配规则**之后才发送**，覆盖整段刺激和收尾，记录非空扫描、过滤器、逐帧匹配、丢帧/解码状况。生产包按协议、源/目的 IP:port、生命周期与时窗、可用 nonce 关联，在本机 TUN **0 命中**；任一生产外层匹配包入 TUN，哪怕物理口也见到，立刻 `fail/leak`。空窗、丢帧或身份歧义为 `invalid/UNSAT`。
3. 当前物理网络/接口上有对应发送元数据，且**独立网络侧**有可核对同一连接/包/查询的入口/回执；设备本地五元组、服务器看到的 NAT 五元组、时间和连接 ID/STUN transaction ID/可用 nonce 闭合。TLS 仅核对连接元数据，不声称能解读 TLS 内层；fd 整数复用、NAT 改端口、并发共享出口未解歧不得算配对。NIC 总字节、单方服务器回执均只能辅证。

通过阈值为生产 3/3 三证齐全、生产 TUN 0 命中、同类正控 3/3 入 TUN 且独立接收器 3/3 可归属回执，**不是**统计“绝不泄漏”的无限保证。时间先用冻结的 UTC/单调校准及误差上界拟定匹配窗口（候选 ±2 秒）；实测超出须**测前**独立审查修改，测后不得放宽以追认旧事件。原尝试的缺包、无出包、时钟失准与传输丢失保留原件并记 `invalid/UNSAT`，不因结果不理想重测。每格还记 TUN 帧/字节、接口可用读数、物理包/端点收件/正控逐 nonce/抓包丢帧及各自来源；同源 tee 自计数不当独立对账。

## 4. 七行四列实测矩阵

下表每项均叠加 §3 的生产三证/未保护正控次数；不触发则 `UNSAT/not exercised`，不得把上一个格的网络或 fd 身份平移使用。D 行为新查询，不预设 DNS 常驻 fd；切网如存活 fd 未换，则记录同一 fd 在新网络真实出包并重新确认保护生效，而非强造新 fd。

| 类别与专属接收点 | A 首连 | B 链路断后重连 | C Wi-Fi -> 蜂窝 | D 蜂窝 -> Wi-Fi |
| --- | --- | --- | --- | --- |
| **M TCP**：netcenter Caddy/management 成功入口连接及设备物理出口 | 3 个独立首次 dial；pre-create fd 继续跨 create 发包；初始系统 DNS 另盘 | 3 个真实新 dial，含队列耗尽的 `purpose=management` 按需补给；逐 fd 重保护、Login/Sync 归属 | 3 次转换，旧流结束或仍可发送状态、新网络上的登录/同步/源接口 | 同 C 的反向 3 次 |
| **S TCP**：netcenter Caddy signal `/signalexchange.SignalExchange/ConnectStream` 成功入口 | 3 个首次 feed，pre-create fd 跨 create 发包；系统 DNS 另盘 | 3 个真实新 signal socket/重注册，非仅日志有重连意图 | 3 次新网络真实信令与入口关联 | 同 C 的反向 3 次 |
| **R TCP**：**home** `rels://home.alfadb.cn:28443` 边缘入口/relay 会话（非云机 relay 容器） | 3 次真实 opt-in TLS/WSS 连接并有 relay 会话数据 | 3 个 relay 断流后的新 `purpose=relay` dial/会话 | 3 次切换后 relay 会话、新物理源 | 同 C 的反向 3 次 |
| **D DNS UDP**：当轮物理 resolver/AP 日志或只读网络镜像，不用 Caddy；Wi-Fi 0043 解析器为 `192.168.50.6:53`，蜂窝必须重发现 | 3 次受保护 relay/STUN 查询；初始管理/信令系统解析独立盘点 | 3 次新 fd/新查询及真实 resolver UDP 出包，不把缓存结果算包 | 3 次新网络物理 DNS 更新后的查询、响应 | 同 C 的反向 3 次 |
| **I ICE/STUN UDP**：netcenter coturn `stun.netcenter.alfadb.cn:3478`、home `home.alfadb.cn:3478` 按实际 send 各取可归属逐笔入口/镜像 | 3 次真实 Binding/check；2 个已公告可用端点逐一登记实际是否发送，DNS 成功不算 Binding | 3 次重试/补池 fd、新检查与 transaction ID | 3 次新网络候选与外部地址来源 | 同 C 的反向 3 次 |
| **Wb WG base UDP**：设备 `wg_fwd_open` 原 fd/dup + **实际 peer 外侧**收件/边缘镜像 | 3 次受保护 base socket 外层包，create 后仍有包 | 3 次断链后真实握手/发送；若更换 fd 则重新保护 | 3 次新物理接口 WG 包与 peer 配对 | 同 C 的反向 3 次 |
| **Ws WG selected UDP**：ICE 选中 socket -> WG dup 身份链 + **实际 peer 外侧** | 3 次确实选中并在该 socket 发出 WG 包，不能用 ICE check 替代 | 3 次真实重选/重连后从选中 fd 发包 | 3 次新网络上的选中 fd 与 WG 外层包 | 同 C 的反向 3 次 |

[0043 `connect/cmd-0014-narrow.json` stdout](/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-SMOKE-20260927-0043/connect/cmd-0014-narrow.json)记 management `connectAddr=8.139.6.47:443`，signal feed `addr=8.139.6.47:443`；管理下发 relay **1 个** `rels://home.alfadb.cn:28443`，随后 relay `ready`；STUN **2 个** usable，受保护 DNS 显示 `stun.netcenter.alfadb.cn -> 8.139.6.47`、`home.alfadb.cn -> 175.10.221.251`。这些只是历史公告、解析及状态，**不证明两个 STUN 均发送、Ws 出包或真实物理出口**；本轮重冻结实际选择和五元组。netcenter 管理/信令需查 netcenter；relay 查 home；STUN 各按实际发包主机取数；DNS 查当前 resolver，WG 查真实 peer。`root@100.108.227.115` 是 netcenter 宿主的 NetBird overlay SSH 地址，不是其公网物理入口或手机出口；0043 的 `8.139.6.47` 为 netcenter 公网地址，`175.10.221.251` 为当时 home 解析值，均须当轮复核。

## 5. 独立出口 oracle 与服务器日志边界

仅**在独立服务器读取授权后**，可经 netcenter `root@100.108.227.115` 只读导出具名 UTC 窗口的 Caddy、management、signal、coturn 日志；home 的 relay/STUN 权限须另确认，不假设 netcenter 继承了 home 的日志。现有 Caddy **错误日志**只覆盖部分远端地址/端口和失败事件，relay 容器只见 Caddy 内网 `172.18.0.6`，coturn 未有逐笔 Binding 日志；因此“日志未见”不是零流量，“容器见到 172.18.0.6”不是设备出口。[现有日志盘点](/home/worker/n2h-kit/status-20260927/SERVER-LOG-ORACLE.md)中与 `175.10.221.251` 相关的历史 tonic 活动也不能归属该设备。

归属先由设备侧 fd/dup/socket ID、本地五元组、当前物理网络句柄与窄时间窗锁定；再用 AP/路由器的 NAT 映射或独立受控出口/服务器入口，将公网 NAT 后五元组与协议、目标端、连接 ID/transaction ID/nonce 对齐，排除共享源的其它 NetBird 节点、Pod 或系统代理。若服务器仅有公网共享源 IP 和接近时间但无端口映射/唯一受控接收路径，仍 `UNSAT`。云机 `wt0` 的 `100.108.*` 不能当手机物理出包。Wi-Fi 本地 resolver、蜂窝 resolver 和 peer 的入口分别需要有权限的只读日志或路由器/网络侧镜像，网内 Pod 无法接 UDP 不作为成功或失败 oracle。

如现有数据不够，只提出**单独授权的后续服务端工作**：netcenter/home 边缘代理成功连接元数据（UTC、真实源/目标 IP:port、协议、连接标识/服务类别，不含认证内容）、两台 STUN 逐笔 transaction 的只读镜像或审定日志级别、实际 DNS resolver 与 WG peer 的只读边缘观测；relay 本体日志可辅证会话但不能单凭 Caddy 内网源归属。服务端配置变更/抓包/SSH **本稿均未授权**；无独立可归属来源的格子先停为 `UNSAT`，不以增大服务端日志级别的提议冒充已有 oracle。

## 6. 停测、不外推与第二阶段

任一生产外层匹配包入 TUN、未保护首发/失败后裸发立即冻结原件、记 `fail` 并停止本轮；三次干净事件不能冲淡泄漏。未实现类别突然发包、不可持有 DNS、Ws 身份/物理出口归属歧义、跨 create 或切网疑似失效，停相关格、记 `UNSAT/deviation`，返独立审查/T0+用户。任何排除或进程旁路残留、目标不覆盖、正控未入 TUN、tee 空窗/丢帧、时间失准、蜂窝或 home/AP/oracle 权限缺失、没有真实出包，一律该格 `invalid/UNSAT`，保留原始尝试，不把空样本写 pass。设备安全、授权边界、网络风险优先于采样。第一阶段不推出 fail-closed 注入、受控 recreate、撤销后的无漏出结论；pre-create 成功不推断跨代、Wi-Fi 不推断蜂窝、N2-H 排除结论不推断逐 fd protect、N6 流量成功不推断 N2。

## 7. 专属 AUTH 结构（待未来签发）

依据[治理 2026-09-27 追加登记](native-nx-governance.md#L165-L186)，仅对**之后新签**的 N2 第一阶段 AUTH：签发后主会话回读 AUTH/sidecar 与所有制品 hash、取得用户明确确认，**确认前连 `hdc tconn` 也不执行**。AUTH 写明设备及无线目标当轮核对、白名单 HDC argv（在线查询、最多 10 次有间隔 tconn、只读设备时间查询）、HAP/回滚、每格目标/正控/路由/网络切换、允许命令与跨尝试累计预算、单执行者、证据/脱敏和停止/恢复步骤。连通后先读设备时间/时区，主机设备差 >60 秒停止；15 分钟重比对。设备锁定、连接后时间仍不可读、AUTH/sidecar 错误、版本大变、超预算均停报。离线 tconn 最多 10 次后**暂停，不自动消费 AUTH**；仅传输中断且保留两次原件时该格最多重跑一次，两次都计预算，结果不理想不能重试。新 AUTH 取消固定 `valid_from/valid_until`，自签发至预算耗尽或用户作废有效；跨会话续用须重新回读确认和核时/核 hash/固件。此规则只替代[原草案 §8](n2-device-evidence-preregistration-draft-20260927.md#8-campaign-授权执行与保密)中旧固定窗口相关表述，不追溯旧 AUTH；没有当前 AUTH 时本稿没有任何预检例外。

## 8. 依赖排序的实施任务（均非本次代码变更）

以下是未来让判据可执行的工作包；目标文件是拟修改/新增，规模为工程人日粗估，未包含审批、上机和服务端变更。`client/entry/selfcheck/` 有并行执行者，实施须协调，本次**不动 `client/`**。每项必须在无专属授权时默认不启用，主机测试用假设备/离线原件。

| 顺序 / 规模 | 文件级落点与验收 |
| --- | --- |
| **T1 捕获路由 / 3-5 人日** | `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`、`NetBirdVpnConfig.ets`、`NetBirdEndpointExclusion.ets`，配套 `client/entry/selfcheck/` 由现执行者排期。增 N2 专用默认关覆盖开关、关闭端点排除，initial/切网重建均不残留旧旁路；不挪用 S6/`force_default_route`。主机假 VpnConfig 测默认行为不变、切换两向、目标实覆盖、冲突拒绝/不抢开默认路由；接受标准为实际配置/路由可审查且目标非排除。T2/T3 前置。 |
| **T2 五元组 TUN 收集 / 4-6 人日** | 复用 `client/core/src/wg_device.rs` 双向 tee 和 `n2h_collector.rs` 可用挂点；拟增 `client/core/src/n2_phase1_collector.rs`、桥 `napi.rs`、测 `client/core/tests/n2_phase1_collector.rs`。默认不挂 tee；先注册 nonce/五元组后发包，收集 from/to 起止、扫描数、逐帧命中及丢失，不沿用 N2-H 的 `n2h-pass`。主机夹具测漏 1 包即 fail、零帧/丢帧/错五元组/复用端口/缺块不通过。依赖 T1。 |
| **T3 同类未保护正控 / 2-4 人日** | 拟增 `client/core/src/n2_phase1_control.rs` 与 `client/core/tests/n2_phase1_control.rs`，NAPI 桥 `napi.rs`，授权入口 `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`。默认无 socket/流量，仅具名证据格在同一 IP/协议、同网络路由用隔离测试端口发非敏感 marker。主机测生产 fd 隔离、注册先于发送、3/3 from-OS 与独立接收器回执、错目标/不可覆盖/接收器不可达时 UNSAT，不触碰认证服务。依赖 T1/T2。 |
| **T4 设备 socket 五元组日志 / 4-6 人日** | 壳 `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`，核心 `client/core/src/mgmtsock.rs`、`socket_supply.rs`、`relay_client.rs`、`protected_dns.rs`、`ice.rs`、`ice_session.rs`、`wg.rs`，测 `client/core/tests/n2_phase1_socket_identity.rs` 与壳相关 selfcheck。默认无 N2 扩展日志；只记 socket ID、原 fd/dup、协议、源/目标 IP:port、网络 generation、protect/首包时序，不记密钥/内容。主机测按需补给、fd 号复用、旧 generation 拒绝、UDP sendto、Wb/Ws 不混，结果足以连接 T2 与服务端。可与 T2/T3 并行，T6 前置。 |
| **T5 只读服务器日志收集 / 2-4 人日** | 拟增 `tools/n2-phase1/fetch_server_oracles.py`、`tools/n2-phase1/tests/test_server_oracles.py`，分开 netcenter Caddy/management/signal/coturn、home relay/STUN、resolver/AP/peer 文件输入。默认**只解析本地夹具，不 SSH**；服务器单独授权后才限白名单只读指定时间/字段，不改配置。主机测 Caddy 错误日志与成功事件区分、coturn 逐笔缺口、home 无权限/NAT 歧义输出 UNSAT、凭据不出报告；不把 netcenter 容器当 home oracle。T6 前置；服务端开启新日志属另项授权。 |
| **T6 执行器和独立判定 / 4-6 人日** | 拟增 `tools/n2-phase1/run.py`、`judge.py` 与 `tools/n2-phase1/tests/test_run.py`、`test_judge.py`；0037 只参考白名单/逐命令记账，不运行旧脚本，不套旧 verdict。默认不触设备/服务器，缺 AUTH 回读确认、hash、路由、正控、oracle 立即拒绝。假 HDC/假日志主机测 tconn 10 次/暂停、>60 秒停止/15 分钟重比、仅传输中断保留一次重跑/累计预算、漏包即停、28 格缺格 UNSAT、N2/N6 分表及两阶段前拒写 N2 pass、恢复与 hash 清单。依赖 T1-T5 和真实 oracle 可用性。 |
| **T7 审查和冻结 / 1-2 人日** | 独立审查本判据与 T1-T6 的测试、版本/hash、每格可触发性、网络权限、时间窗和停止码，结果另立 `docs/` 记录；**审查后**才由用户决定是否签第一阶段设备 AUTH。默认不产生设备命令；接受标准为所有证据前提明确且分阶段结论无越权。依赖 T1-T6。 |

**需要用户决定（不是当前授权）**：home relay/STUN 的只读日志或镜像权限；Wi-Fi AP/resolver、蜂窝 DNS/NAT 与实际 WG peer 外侧收件的可审查来源；是否愿意**另行批准** netcenter/home 成功入口与逐笔 STUN 记录或网络侧只读镜像；设备回来时 SIM/蜂窝能否两向切换、relay opt-in 与安全测试端口可否启用；最终独立审查完成后是否签发专属 N2 第一阶段设备 AUTH。缺这些条件的格先保持 `UNSAT`。
