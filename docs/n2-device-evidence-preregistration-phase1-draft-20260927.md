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
3. 当前物理网络/接口上有对应发送元数据，且**独立网络侧**有可核对同一连接/包/查询的入口/回执；设备侧 socket 身份与目的地址、服务器看到的 NAT 后来源、校准时间窗、请求序列/会话持续时长及可用 transaction ID/nonce 闭合。NAT 后本地端口通常不等于服务器 `remote_port`，不得直接比较这两个端口；TLS 仅核对连接元数据，不声称能解读 TLS 内层；fd 整数复用、并发共享出口未解歧不得算配对。NIC 总字节、单方服务器回执均只能辅证。

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

**主方法是服务端访问日志的公网 `remote_ip:remote_port` + 设备侧逐 socket 的本地 IP:port、远端 IP:port/协议、fd/dup、物理网络句柄及 protect/首包时间 + 测前冻结的时间窗三方对齐**，再与 §3 的 TUN 零命中和同路由正控合判；服务端单方日志不是设备归属，也不能以成功登录或内层收发替代物理出口。运维回报称 2026-09-28 00:0x 已在两侧开启成功入口日志，取代[9 月 27 日旧日志盘点](/home/worker/n2h-kit/status-20260927/SERVER-LOG-ORACLE.md)所述仅有 Caddy 错误日志/缺逐笔 STUN 的**当时**状态；旧盘点的历史候选流量仍不得追认。云端 `access-api.log` 的来源字段与 0044 窗口已只读复核；home 端 `/apps/netbird/data/caddy/access.log` 已由主会话以 `ssh alfadb@192.168.50.9` + `sudo -n` 只读复核（2026-09-28 约 00:40；root 登录不可用，须用 alfadb 账户）。

- **netcenter**：公网 `8.139.6.47`，`root@100.108.227.115` 仅为 overlay SSH；Caddy `/apps/caddy/data/access-{api,signal,relay}.log` 分别记 API、signal、云端备用 relay，每请求一行 JSON，访问日志保留至少 7 天。当前第一阶段 M 对 `access-api.log`、S 对 `access-signal.log`，R 的主中继是 **home** 而非云端备用 relay；当轮若 relay 实际轮换，重冻结目标/日志，不能拿云端日志替 home。signal 混有大量其他客户端，来源 IP/时间邻近不自动归属设备。
- **home**：主中继 `192.168.50.9`（`home.alfadb.cn`，当时报公网 `175.10.221.251`，DMZ），Caddy `/apps/netbird/data/caddy/access.log` 每请求一行 JSON，root `0600`，须有只读 sudo 权限；至少保留 7 天。R 使用此日志的 `/relay` 记录，不用 relay 容器看到的 Caddy 内网 IP；relay `0.79.0` 不读 `X-Forwarded-For`，以 Caddy 的入口 `request.remote_ip` 为准。WebSocket `/relay` 行在**会话结束**落盘，`ts` 是结束时刻，`duration` 是会话时长：以 `ts - duration` 反推起点并与设备侧连接建立/存活/关闭窗口对齐，不能把落盘时刻当开始时刻。
- 两侧 Caddy JSON 的可用元数据为 `ts`、`request.remote_ip`、`request.remote_port`、`request.client_ip`、`request.proto`、`request.method`、`request.uri`、`status`、`duration`、`size`；只导出必要白名单字段，不导出 headers/请求体。运维称 `Authorization`、`Cookie`、`Sec-Websocket-Protocol` 整键删除；主会话已只读核查两侧日志持续写入并包含 `remote_ip/remote_port`，云端三份日志检索敏感头零命中。coturn 两侧开启 `log-binding`+`verbose`，逐笔有 `New UDP endpoint ... remote addr IP:port` 与 `BINDING`；Docker 日志为 `50m x 8` 空间轮转，**不承诺 7 天**。云端 `3478` 有约 115 行/分钟的互联网扫描噪声，必须限时间、来源、目的与实际请求逐笔排除。

**对齐规则。** 当轮先校准设备与服务器 UTC/单调偏差、冻结 §3 窄窗及路由/解析目的；设备记录每个生产 socket 的创建、protect settle、首次及后续 connect/send、关闭/断流、当前本地端与实际远端、network generation。TCP 在服务端按**目的服务与地址、协议、请求序列及连接持续区间**筛出候选，再用设备时序匹配；NAT/hairpin 会重写设备本地 IP:port，故服务端 `remote_port` 是映射后端口，**不可直接与设备本地端口作相等条件**。`remote_port` 可区别服务端并发连接，但只有独立 NAT 映射记录时才可把它回连到设备端口。Wi-Fi 经公网域名 hairpin 访问 home 时预期 home Caddy 来源为家宽 WAN `175.10.221.251`（当轮重核），蜂窝直连 home/netcenter 时预期为当轮运营商公网 IP；云端 Wi-Fi 出口也须按当轮观察核对，不能凭历史 IP 判定。若共享出口下多个 socket/客户端产生重叠候选、时钟/会话边界不准或路径可能另经代理/隧道而无法唯一归属，判 `invalid/UNSAT`，不得 pass。WebSocket 按结束 `ts` 减 `duration` 处理，长时 Sync/streaming 请求亦结合实际 duration 判断；`status=200` 仅表示该请求响应，非逐包/逐 fd 身份。

| 类别 | 独立入口及仍须补齐的配对 |
| --- | --- |
| M management TCP | 云端 `access-api.log` 中管理请求（如 GetServerKey/Login/Sync），与设备该 management socket 的真实远端、连接区间和请求顺序配对；API 中其他客户端不得误计。 |
| S signal TCP | 云端 `access-signal.log` 中 ConnectStream/重连请求，与设备逐 socket 生命周期、长流时长和重连序列配对；大量其他客户端与同源并发未解歧则 UNSAT。 |
| R relay TCP | home Caddy `/relay` 会话，与设备 `rels://home.alfadb.cn:28443` 的 fd/远端/起止窗口配对；按结束时刻反推起始，不能用云端备用 relay/本体代理内网地址代替。 |
| I STUN UDP | 按实际发往两侧 `:3478` 的端点分别取 coturn `New UDP endpoint`/`BINDING`，与设备 UDP sendto 时间、远端、网络句柄及可获得的 STUN transaction ID 对齐；若 coturn 文本不含 transaction ID，则需镜像/其他独立标识消除扫描或共享来源歧义，否则 UNSAT。 |
| D DNS UDP | **两侧服务端均无设备实际 resolver 的日志**。当轮重新发现 Wi-Fi/蜂窝 resolver；取得该 resolver/AP 的只读逐查询日志（查询名、查询 ID、时间、来源/响应）或经单独授权的当前物理链路只读镜像，和设备新查询 fd/网络/时间对齐；缓存命中、无可读 resolver/镜像或无法唯一归属时 `not exercised`/`UNSAT`，不能借 Caddy。 |
| Wb/Ws WG/ICE UDP | **Caddy/coturn 不记录直达 peer 的 WG/ICE 流量**，coturn Binding 不能替代选中后 WG 出包。按当轮实际 peer 外侧可控接收器逐包回执或经授权的 peer/边缘只读镜像（时间、外层目标、NAT 后来源、可归属包标记），结合设备 Wb 原 fd 或 Ws 选中 socket -> WG dup 身份链、握手/发送时序和物理网络对齐；peer 无收件/外侧镜像则相关格 UNSAT。 |

**0044 预演观察（诊断，非证据）。** [0044 `traffic/observation-01.json`](/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-SMOKE-20260927-0044/traffic/observation-01.json)中设备 probe ID 与主机 peer receipt 相同，主机见 `src=100.108.144.165:51728`、`a2b_seen+1`、`echo=written`；新会话设备 `wgTxBytes=112`。这是**隧道内**设备 -> peer 到达，不是物理外层出口证明。0044 北京时间（UTC+08）约 00:35-00:37 Wi-Fi 关转蜂窝时，云端 `access-api.log` 的 00:36 **5 条**及 00:37 **1 条**来自 `223.153.208.166`（已只读按时间/source 核对），前后窗口来源为家宽 `175.10.221.251`；home Caddy `/relay` 00:37 有一条（主会话只读核对原件） `223.153.208.166`、`duration=88` 的结束记录。这里只构成切网来源变化的预演：0044 未与设备侧逐 socket 元数据/服务端 NAT 后五元组及 relay 会话起止唯一对齐，signal 更混有大量其他客户端；没有专属 AUTH、TUN 正控和完整 oracle，不得追认为 N2/N6 证据。

访问日志已开启**不是**授权读取原始敏感日志、修改服务器或上机；后续 T5 仅在独立确认的只读权限/具名时间窗内取白名单字段，home 的 root `0600` 日志和 resolver/AP、peer 外侧可见性仍待落实。访问日志未出现某事件还须检查轮转、持续连接结束落盘、采集窗/时钟及权限；缺口记 UNSAT，不将“没看到”当零流量。服务端配置变更、网络镜像和 SSH 连接本草案均不授予操作权。

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
| **T4 设备 socket 身份与出口对齐日志 / 5-8 人日** | 壳 `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets`，核心 `client/core/src/mgmtsock.rs`、`socket_supply.rs`、`relay_client.rs`、`protected_dns.rs`、`ice.rs`、`ice_session.rs`、`wg.rs`，测 `client/core/tests/n2_phase1_socket_identity.rs` 与壳相关 selfcheck。默认无 N2 扩展日志；逐 socket 记稳定 ID、原 fd/dup、协议、实际本地 IP:port 与远端 IP:port、network generation/接口、校准时间戳、protect settle、首次及后续 connect/send、重连/关闭，标识 M/S/R/D/I/Wb/Ws 及 relay/长流会话区间，不记密钥、认证内容或载荷。主机测按需补给、fd 号复用、旧 generation 拒绝、UDP sendto、Wi-Fi/蜂窝切换、Wb/Ws 身份不混；输出足以与服务端请求序列/结束时刻减 duration 配对，**不假设设备本地端口等于 NAT 后 remote_port**。可与 T2/T3 并行，T6 前置。 |
| **T5 只读服务器 oracle 取数与对齐 / 3-5 人日** | 拟增 `tools/n2-phase1/fetch_server_oracles.py`、`tools/n2-phase1/tests/test_server_oracles.py`，分别接收 netcenter `access-api.log`/`access-signal.log`、home Caddy `/relay` 与两侧 coturn 白名单字段，以及经单独落实的 resolver/AP/peer 外侧输入。默认**只解析本地夹具，不 SSH**；另获具名只读权限后才按冻结 UTC 窗口提取 `request.remote_ip:remote_port`、目标/状态/时间/duration 等必要字段，home root `0600` 须限定 sudo 读；不得导出 headers、凭据或原始全量日志。主机测云端 M/S、home R、WS 结束时刻反推起点、coturn 噪声过滤及空间轮转缺口、NAT 改端口/共享出口/并发 signal 歧义 UNSAT、home 无权限/DNS 或 WG 无外侧 oracle UNSAT；无一对一映射时不凭 IP 和时刻硬判 pass。T6 前置；现有日志已开启，不再把开日志列为实施前提。 |
| **T6 执行器和独立判定 / 4-6 人日** | 拟增 `tools/n2-phase1/run.py`、`judge.py` 与 `tools/n2-phase1/tests/test_run.py`、`test_judge.py`；0037 只参考白名单/逐命令记账，不运行旧脚本，不套旧 verdict。默认不触设备/服务器，缺 AUTH 回读确认、hash、路由、正控、oracle 立即拒绝。假 HDC/假日志主机测 tconn 10 次/暂停、>60 秒停止/15 分钟重比、仅传输中断保留一次重跑/累计预算、漏包即停、28 格缺格 UNSAT、N2/N6 分表及两阶段前拒写 N2 pass、恢复与 hash 清单。依赖 T1-T5 和真实 oracle 可用性。 |
| **T7 审查和冻结 / 1-2 人日** | 独立审查本判据与 T1-T6 的测试、版本/hash、每格可触发性、网络权限、时间窗和停止码，结果另立 `docs/` 记录；**审查后**才由用户决定是否签第一阶段设备 AUTH。默认不产生设备命令；接受标准为所有证据前提明确且分阶段结论无越权。依赖 T1-T6。 |

**需要用户决定（不是当前授权）**：home Caddy root `0600` 访问日志及两侧 coturn 的具名窗口只读取数权限（当前会话无法以 root SSH 直读 home）；Wi-Fi AP/resolver、蜂窝 DNS/NAT 与实际 WG peer 外侧收件或只读镜像的可审查来源及必要权限；设备回来时 SIM/蜂窝能否两向切换、relay opt-in 与安全测试端口可否启用；T1-T6 完成、独立审查后是否签发专属 N2 第一阶段设备 AUTH。两侧成功入口及逐笔 coturn 日志已由运维开启，**不再待批准开日志**；缺仍待决条件的格先保持 `UNSAT`。

## 修订记录

- 2026-09-28：仍为 **DRAFT / 未生效 / 非设备授权**。据运维 00:0x 开启日志的回报及云端 access-api 只读复核，更新 §3/§5 的 NAT、会话时间窗和逐类服务器 oracle；登记 0044 隧道内流量与切网来源变化为**预演观察，非证据**；T4/T5 调整为设备逐 socket 关联与已存在日志的只读取数，移除“待开日志”事项。未改代码、未操作设备或修改服务器，未追认旧轮；家里 `/relay` 记录已由主会话经 alfadb 账户只读核对原件。
