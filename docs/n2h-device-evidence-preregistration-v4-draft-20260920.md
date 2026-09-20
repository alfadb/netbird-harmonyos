# N2-H 设备侧证据判据预注册——修订第 4 版生效稿（2026-09-20）

> **生效声明（2026-09-20，Asia/Shanghai）**：本文件为 N2-H 设备侧证据判据预注册修订第 4 版**生效稿**，自 2026-09-20（Asia/Shanghai）起生效。生效依据：①**跨厂商独立审查＋复验已完成**——第一轮跨厂商独立审查裁定 blocker（5 项必修）并逐项落字后，经跨厂商独立审查＋复验通过（第 2–5 项已关闭、第 1 项三处微改按评审者逐字要求落定）；②**人类显式批准**——批准渠道为本次会话 ask_user_question，选项全文「批准并登记 v4（推荐）」，批准文本「批准并登记 v4」（2026-09-20）。**效力声明**：自本版生效起，v3.1（`docs/n2h-device-evidence-preregistration-draft-20260917.md`，全文 sha256 `02f96bc8e4c1a7a282fbf3cd302ce0616af367509f98c40e43439f87c26e12ff`）**据以测量的效力作废**，仅保留历史审计效力；v3.1 正文本身不被修改（G-4③）。本生效稿的唯一规范字节＝生效登记 commit 中本文件内容（§6.1 第 5 步），其全文 sha256 由仓B 登记件（`records/n2h-preregistration-effective-v4-20260920.json`）锚定，本文件不嵌入自身 sha256。
> 本文件在起草阶段为新增草案文件：起草全程只读核对，未改动任何既有文件、未执行任何设备操作；定稿、git commit 锚定与仓B 生效登记按 §6.1 第 4–5 步于 2026-09-20 执行（登记件与 sidecar 见仓B `records/`）。

- **拍板记录（2026-09-20）**：人类已对 §6.2 原四项开放项拍板——**C7 择子选项 B、carrier-only-relay 仅子集结论、DNS 择路径①、其余按草案既有文本采纳**；各处已按「已决」落字（§2、§3、§4、§5-9、§6.2）。**拍板不等于生效**：该四项拍板不构成生效批准；生效批准另经 §6.1 第 3 步于 2026-09-20 完成（见文首生效声明）。
- **审查记录（2026-09-20）**：跨厂商独立审查（第一轮，经派发会话转达）裁定本稿 **blocker、不可生效**，提出 5 项必修；本版逐项落字（文内以「审查新增/审查修订」标注），**四项人类决定不翻案**；修订后经跨厂商独立审查复验通过（第 2–5 项已关闭、第 1 项三处微改按评审者逐字要求落定），方进入生效程序第 3 步。
- **体例**：§2–§4 每项修订按四段式登记——**现有条款原文含义 → 观察到的问题 → 拟议修改 → 影响与边界**；§5 显式列出本版**不放宽**的条款；§6 按既有 G-4④ 程序写明生效步骤与已决/剩余清单。原则：**加具名、不加豁免**。
- **证据效力声明**：§1 引用的全部真机观察来自**诊断类 dry-run 轮**（逐轮 `is_evidence=false`、`not_a_gate_pass=true`，现行规格 §1.3 例外二口径），不构成门证据、永远不得事后升级；其在本文件的效力仅限**修订理由的实证输入**。
- 起草日期：2026-09-20（Asia/Shanghai）。修订过程：只读核对现行规格、采集器设计（`docs/n2h-device-collector-design-20260917.md`）与 `/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/` 下 AUTH-DIAG 产物；行号以起草时点实读为准（会漂移，以内容定位）。

---

## 1. 修订理由与实证

### 1.1 总述

三条判据缺口（wg_peer 具名缺席、端点回执、计数源）在 2026-09-18 至 09-21 的多轮诊断中反复独立复现，且 0027 已明确定性：**「三项均落在规范或环境边界，不是代码缺陷；在现行规格与当前环境下，正式门无法通过这些判据」**（`AUTH-DIAG-DEVICE-VALIDATION-20260921-0027/STAGE-RECORD.md`「三项剩余判据的最终状态」节原文）。据此需要 v4：把三处「结构性不可满足」改述为**可机械判定的具名形态 / 具名外部输入 / 具名应用内账本**，同时不放松任何证伪面。

### 1.2 实证清单（逐条，引原文）

| # | 实证 | 出处 | 支撑 |
|---|---|---|---|
| E-1 | a2b 探针显式绑定 VPN 网络后**首次端到端闭合**：`N2H_POSCTL_BIND_NET\|probe_id=N2H-posctl-5a04eafa88afc4d686069afd\|requested_net_id=148\|result=bound\|errno_or_code=0`；同 socket 发送前后 mark `0x00000000`→`0x00030094`；TUN from-os 观测 `src=100.108.144.165 dst=100.108.126.29 sport=37530 dport=40001 frame_len=80` 且 to-os 回显 80 字节；主机**逐 probe_id 回执**与设备同一 nonce：`src=100.108.144.165:37530\|dst=100.108.126.29:40001\|echo=written`（源＝设备自身 overlay）。文档 verdict 仍 `n2h-inconclusive`，reasons 原文列出 wg_peer 缺失、mgmt/signal/relay/dns 回执缺失、counter reconciliation 缺失 | `…20260921-0022/STAGE-RECORD.md`（「绑定结果」「a2b首次端到端闭合」节）；evidence.json sha256 `97aaa5c3…eeb344` | §2、§3 |
| E-2 | 同一 a2b socket 未绑定态实测（0021）：`N2H_POSCTL_SOCKET_CONTEXT\|…\|uid=20020329\|euid=20020329\|mark=0x00000000\|mark_errno=0\|bound_device=\|bound_device_errno=0`（发送前后同值，`send_len=52\|send_errno=0`）；网络句柄 `defaultNetId=143\|appNetId=0\|nets=2`（143=wlan0、bearer 1、含 `0.0.0.0/0`；vpn netId=147/148、bearer 4、仅 `192.168.50.0/24`+排除 /32）；`/proc/net/route` 主表含 `vpn-tun 100.108.0.0/16` 且无默认路由；a2b 未入本机 TUN，对端见到的源为另一机器 overlay `100.108.171.38` | `…20260921-0021/STAGE-RECORD.md`（socket 原文与「路径事实」「结论与边界」节） | §1.3（绑定修复的行为学解释链） |
| E-3 | 历次会话 relay-only：wg_peer 冻结条目恒 `present=false`，`absent_reason` 逐字 `no wg_peer endpoint: no live session peer endpoint (fail-closed: the collector is not armed without a ready session)`（0019/0020 evidence.json 实读同串）；0020 主机逐 probe_id 回执原文 `nbinterop: n2h-peer-receipt\|probe_id=N2H-posctl-6d50156456a4b70580750875\|proto=17\|src=100.108.171.38:46003\|dst=100.108.126.29:40001\|echo=written\|self_marker=false\|unix_ms=1789888654196`；0022 起双腿闭合但会话仍 relay-only | `…20260919-0019/evidence.json`、`…20260920-0020/evidence.json`、`…20260920-0020/STAGE-RECORD.md` | §2 |
| E-4 | 端点回执缺位为生产恒态：0022 `endpoint_side[]` 中 management/signal/relay/dns 条目 `observed:false`、`method:"none"`、`detail` 逐字 `outer-path delivery unproven from inside the device: this endpoint's delivery log is operator-side (§4.0)`；生产 seam `probe_receipt` 恒 `None`（`client/core/src/n2h_collector.rs:1764`，doc `:1760-1763`）；TCP 探针形态只有 connect+write+shutdown（`:733`；测试 `:4433`），协议上无设备侧可自取的应用层回执；DNS `dns-nxdomain-echo` 默认 OFF（`:30`、`:917`），`echo_received` 仅随 sent 记录落档、判定链不消费（`:690`、`:938-958`） | 同左；代码为仓内实读 | §3 |
| E-5 | 计数源三重失效：①App 直读 `/proc/net/dev` 恒 `Permission denied (os error 13)`——0019/0020/0022 三轮 evidence notes 逐字同为 `window-open counters read failed (no before edge): …`；②规格声明的 `hdc-proc-fallback` 固定路径**应用不可见**——0027 实证：窗口前两次存在性检查 `TARGET_PRESENT`（`-rw-r--r-- 1 shell shell 6241`，21:33:02.280646/21:33:03.945849，窗口约 21:33:11 开启）、清理时按唯一路径特征杀到循环 pid `30752`（循环窗口期间存活）、应用仍报 `…netbird-n2h-proc-net-dev: No such file or directory (os error 2)` ⇒ **否定结论**（该固定路径不在应用可见视图内，非待办）；0025 先证 `setsid` 真分离后文件出现且 mtime 推进（644、6241B，mtime `1789910177.199863`→`1789910181.815873`），排除「投放未成功」解释；③⇒ `counters=[]` 恒（reasons 原文 `counter reconciliation missing: no interface-counter window was collected`） | `…20260921-0027/STAGE-RECORD.md`、`…20260921-0025/probe-summary.md` §4–5、各轮 evidence.json | §4 |
| E-6 | 官方字节账本真机可用：`N2H_IFACE_BYTES_PROBE\|interface=vpn-tun\|rxBytes=560\|txBytes=520\|rxError=none\|txError=none`（0019）、RX=560/TX=576（0020）；负控名 `n2h-none0` 如实报错 `2103012`；API＝`@ohos.net.statistics getIfaceRxBytes/getIfaceTxBytes`（`NetBirdInterfaceStatsProbe.ets:4-5`，SDK 26.0.0.821 d.ts:79/113）；**无公开 per-iface 包计数 API**（`docs/n2h-iface-statistics-probe-20260918.md:52-53` 原文：本探针只产字节数、没有 public per-iface packet API、不允许用 WG 自计包数伪造独立包数来源） | 同左 | §4 |

### 1.3 方向

三项修订共同点：**加具名、不加豁免**——把「不可判定/不可满足」改述为「可机械判定的具名形态或具名输入」；E-1/E-2 证明的「探针绑定 VPN 网络才走隧道」解释链同时说明：观测面本身可修复，堵门的是判据文字与环境边界，不是观测机制。

---

## 2. C1/C2：wg_peer 的唯一具名缺席形态 `absent_reason=carrier-only-relay`

- **现有条款原文含义**：C1 要求必需类（management/signal/stun/wg_peer/dns）`present=true`，缺席即缺口 ⇒ C1 不成立；§2.1(6) 以「`wg_peer` 类 `present==true` 而代 G 的 `wgPeerEndpoints==0` ⇒ C2 不成立」约束代际一致性；wg_peer 无端点时现行唯一形态是 fail-closed 理由串（`n2h_collector.rs:553`）。
- **观察到的问题**：E-3——目标会话为 relay-only：会话活跃、relay 承载帧正常流动、正控双腿可闭合，但 `wg_peers[].endpoint` 结构性恒空（ICE 未选中直连端点，`wg_device.rs:296-298` `endpoint: Option<…>`）。这不是「漏冻」，是「该拓扑下无物可冻」；现行判据无法区分二者，永久阻塞门判定。
- **拟议修改**：C1 为 wg_peer 新增**唯一**具名缺席形态 `present=false ∧ absent_reason=="carrier-only-relay"`，当且仅当同窗口四条件**全部**满足；每条条件都有**通过侧证据契约**（审查新增，R-2/R-5 同级）——证据文档内须出现对应逐字 notes 行，且**四条件在 arm 沿与窗口闭合沿各评估一次、两沿逐字一致**（行形态 `carrier-cond-<k>\|phase=open\|close\|<键值>`）。**两沿一致的比较规则（机械化）**：对每个 `k`，把 `phase=open` 行与 `phase=close` 行各按 `\|` 切分为字段序列，**剔除 `phase=…` 段后**，其余字段的（字段名,取值）集合须**逐字段逐字相同**（比较对顺序不敏感；同一字段出现两次即判不符）；存在任一差异 ⇒ 两沿不一致 ⇒ C1 不成立。**缺任一行、任一字段、或两沿不一致 ⇒ C1 不成立（`n2h-inconclusive`）并按 S18 处置——缺字段≠可推定**，不得以 arm 成功或会话活跃推定任何条件成立：
  1. `wg.ready==true`（`wg_device.rs:1486`；arm 闸 `device_up && ready` 同源字段）——证据行 `carrier-cond-1\|…\|wg_ready=true\|source=connector_status.wg.ready`；
  2. **正控对端身份的 peer 记录去重后恰一条且 `endpoint==null`**（审查修订②＋收口(c)：不再泛统计全体 `wg_peers`）——归属锚＝**a2b 正控目标 overlay IP，必须严格取 arm 选项 `peer_overlay_ip`**（`n2h_collector.rs:1884`；裸点分 IPv4、非法即整臂拒绝 `:1963-1971`；ets 自控制文件透传）；**该选项缺失 ⇒ 条件②不成立（缺字段≠可推定），不得从 network map 回推目标**。该 IP 须被**恰一个** wg peer 的 `allowed_ips`（`wg_device.rs:299`，(addr,prefix) 前缀覆盖）覆盖 ⇒ 该 peer 即条件②对象；**去重键写死＝`pub_key_b64`（唯一键），`peer_fp`＝其公钥 sha256 前 16 hex**——同一 peer 的多条记录按该键去重后恰一条，计两次即不符；**零覆盖或多覆盖 ⇒ 条件②不成立**；陈旧/离线 peer（窗口内有 `peers-went-offline` 打点者）不计入——证据行 `carrier-cond-2\|…\|peer_overlay_ip=<ip>\|peer_fp=<16hex>\|endpoint_null=true\|stale_excluded=<k>`；缺绑定行 ⇒ 条件②不成立；
  3. `relay.enabled==true ∧ relay.state=="ready"`（状态 token 域 `connector.rs:3000-3011`，Ready＝认证会话活跃）——即本会话的承载挂载为 relay 类——证据行 `carrier-cond-3\|…\|relay_enabled=true\|relay_state=ready`（token 逐字）；
  4. **承载类别为 relay ∧ 承载端点已以 IPv4 冻结**（审查修订③④＋收口(a)：`RelayStatus.urls` 是配置/公告集、非本会话实际挂载，**不得作判据锚**；亦不得以承载帧计数推断挂载类别）——两条可判定证据**同时成立**：①冻结集内存在 kind=relay、`present==true` 且 `resolved` 为 **IPv4 `host:port` 字面**（§2.1 R-3 同款严格解析）的条目（**主机名形态条目不计**——当前实证即含 `home.alfadb.cn:28443` 形态，不满足；仅壳侧 DDNS 已解析出的 IPv4 条目可计入）；②条件② peer 的 status 具名运行时字段 **`carrier_attached==true`**（`wg_device.rs:307-308`：字段语义即「已为该 peer 注入非 UDP 出向载体（relay）」；status JSON 字段名逐字），并以窗口日志交叉核对标记 `N13_WG\|carrier-attach\|kind=relay-wss`（`wg_device.rs:829`；kind token 域 `relay_client.rs:1599-1601`）——证据行 `carrier-cond-4\|…\|peer_fp=<16hex>\|carrier_attached=true\|frozen_ipv4=<ip:port;…>`；字段缺失或标记不符 ⇒ 条件④不成立。
  任一条件不满足 ⇒ 回到现行缺口口径（C1 不成立）。并同条写明：
  - **该形态下证据范围明确不覆盖直连路径（已决，2026-09-20 人类拍板）**：即使四条件全部满足、C1–C8 全部成立，该窗口结论也**只登记为「relay 承载拓扑子集」，不等于登记 N2-H pass**；对直连拓扑判 N2-H 须在直连会话上**另行测量**（新窗口、新 AUTH、同判据），本形态不存在「直接登记 N2-H pass」的通路。ICE srflx/直连 UDP 路径在本窗口**无证据**，残余范围「动态候选」「重连换址」两条照旧保留并加注 `wg-peer-absence=carrier-only-relay`；该限定已落为 **v4 定稿文本**（本文件）§8.1 不外推声明字面条目：「carrier-only-relay 形态下的任何结论不外推至直连拓扑；直连拓扑判 N2-H 须另行测量」（**不得修改已锚定的 v3.1**，G-4③）。
  - **禁重标**：relay 条目 kind 保持 `relay`，不得把 relay 地址写为 wg_peer 条目或并入 wg_peer 冻结解析集；`absent_reason` 字面恒为 `carrier-only-relay`（现行 fail-closed 理由串在会话活跃时不再适用，须随实现具名 token 化——登记为 §6.2-5 实现面事项）。
  - **禁推定**：不得以 `wg.ready==true` 直接推定 wg_peer `present=true`——ready 只证明 WG 会话存在，不证明存在直连 UDP 端点；无②③④时 ready 与本形态无关。
  - **窗口内拓扑变化即停（C10 触发类⑤，审查新增＋收口(b)；机械触发，禁止以「理由即失效」散文代替）**：以条件②同一 `peer_fp` 的 `endpoint` 复读为准——**判据级要求＝观测期逐轮复读**：每一观测拍都复读该 peer 的 `endpoint`，任一拍非 null（落配）⇒ 触发；相邻两拍均非 null 且值不同（ICE nominated 换址）⇒ 触发。**实读结论（收口(b)核实）**：现观测循环每拍仅读 `peer_report()`（`n2h_collector.rs:3087` 起循环体；`PeerReport` 无 peers/endpoint 字段，`CollectorHost` trait `:1716-1731`）⇒ 逐轮复读**当前不可达，列为实现面待办**（§6.2-7：扩展 host seam 使每拍可读该 peer `endpoint`）；落盘前**两沿比对仅是最低要求**（开沿 null → 闭合沿非 null ⇒ 触发；两端非 null 且值不同 ⇒ 触发），**不得以两沿比对代替逐轮复读**。触发 ⇒ 窗口 fail-closed 结束、按 `n2h-inconclusive` 登记，`reasons` 记具名 token `c10-trigger=endpoint-landed|endpoint-changed`，S8 同一纪律（不重试、不拼接）。**理由**：具名缺席形态实际缩小了 C4 可命中面（无 wg_peer 探针、直连路径不入 TUN），窗口内拓扑变化必须显式终止窗口，防止以缩小的负证据面继续累积结论。
  - **cond 行输出者与 requestId 关联（收口(d)，实读写死）**：四条件数据全部在 core 侧（`wg.ready`/`wg_peers`/`relay.*` 均为 `connector_status()` 字段；ETS 不持有）⇒ **输出者＝core**：arm 沿与闭合沿各读一次 status 快照并各输出一整套 `carrier-cond-1..4` 行入 `notes[]`（**闭合沿快照读取为实现项**——现 close 路径仅做计数配对读取，§6.2-1）；行格式 `carrier-cond-<k>\|phase=open\|close\|requestId=core\|<键值>`（core 恒 `requestId=core`，`n2h_collector.rs:2234` 实读）；与 ETS `N2H_COLLECTOR_ARMED\|requestId=<ets-id>\|…\|openedMono=X`（`NetBirdVpnExtensionAbility.ets:1345`）的关联＝**core `N2H_COLLECTOR_ARM` 行 `opened_mono` 与 ARMED 行 `openedMono` 同值配对**（既有双标记纪律，机械可核）；ETS 不补 cond 行。缺任一 cond 行 ⇒ 按 S18 处置（同上）。
  - **结论上限的证据化（审查新增；写进 C8 判定面，不留散文）**：具名缺席形态窗口的证据文档 `notes[]` **必须**含逐字 token `conclusion-scope=relay-carrier-subset-only`；C8 汇聚时该 token 缺失 ⇒ 该窗口不得登记任何通过类结论（记 `n2h-inconclusive`）；且该形态下即便 verdict 汇聚为 `n2h-pass`，**登记层上限**仍为「relay 承载拓扑子集」——此上限随定稿写入 §2-C8 判据行字面与文档级必需 token 清单。
- **§2.1(6) 在该形态下的明确规则**（追加两条对称判定，(6) 款原文不动）：⑤代际行 `wgPeerEndpoints==0` ∧ wg_peer `present==false ∧ absent_reason=="carrier-only-relay"` ∧ 四条件全满足 ⇒ **一致**，(6) 款不触发；⑥`absent_reason=="carrier-only-relay"` 而代际行 `wgPeerEndpoints>0`，或 `wgPeerEndpoints==0` 而 `absent_reason` 缺失/非该字面 ⇒ **不一致 ⇒ C2 不成立（`n2h-inconclusive`）**。该形态下 C6 的 wg_peer 收执行记 `not-applicable:carrier-only-relay`（无直连端点可探测；非缺口、亦非正证据）。
- **影响与边界**：本形态是**拓扑限定**的时效形态，只对本窗口确为 relay-only 会话成立；任一 `wg_peers[].endpoint` 落配（ICE 选中直连）即按 **C10 触发类⑤** fail-closed 结束该窗口（机械触发，见上），wg_peer 须回 present 面重新登记（与 v3 对 turn 缺席「只对本 build 成立」同构）。C1/C2 其余判定（代际七条、三条硬条件、同代绑定、dns 口径）逐字不变；本形态不产生任何通过类正证据。**结论效力上限（已决）＝「relay 承载拓扑子集」**（见上）；该上限是证据边界，不是判据豁免。**判定引擎现状（审查指认，登记为实现面事项）**：judge 第 2 步对必需类 `present==false` 一律产缺口（`n2h.rs:836-846` 实读：`absent_reason` 非空的必需类仍进 reasons）——按 `absent_reason` 白名单（恰 `carrier-only-relay` ∧ 四条件证据行两沿齐备一致）放行属 §6.2 实现面事项，落盘前该形态在任何引擎输出中仍为缺口。

---

## 3. C6：management/signal/relay 回执定义为「外部运维侧投递日志」输入

- **现有条款原文含义**：C6 要求每个 `sent=true` 探针在 `endpoint_side[]` 存在 `observed=true` 收执；设计 §4.0 收执矩阵把 mgmt/signal/relay（TCP）定为 `method:"none"`（「依赖运维侧服务端投递日志……未取得前记缺口、不得假通」），wg_peer 为对端 `unknown_peer_drops` 活值的运维收执；缺位 ⇒ 恒 `n2h-inconclusive`（设计 §4.0/:187 诚实结论）。
- **观察到的问题**：E-4——生产 seam 恒 `None`、detail 逐字承认 `delivery log is operator-side`、TCP 探针协议上无应用层回执可自取；「运维侧投递日志」的**输入形态、最小字段、绑定方式、防伪造要求**在现行文本中未定义——运维真提供日志时也无机械核对规则，缺口永远只能停在「未取得」。
- **拟议修改**：把 management/signal/relay 回执定义为**外部运维侧投递日志输入**（external receipt bundle），核对规则：
  1. **最小必需字段（逐条回执记录，五项；第五项审查新增）**：`probe_id`（逐字，含 nonce）、`arrived_at`（服务侧观测的到达时刻，带时区 epoch）、`observing_endpoint`（观测端点 `host:port`）、`source_id`（来源标识＝服务组件名＋日志文件名＋该文件 sha256）、**`observed_physical_src`（端点观测到的探针物理源址 `ip:port`）**——比对规则（复验修订，**严格等值**）：其 `IP:port` 须等于该窗口设备物理出口地址，**地址来源须具名并随文档登记**（如 `source=netid-iface-addr` 读数行，逐字入 `notes[]`）；**删去推测性放宽**——不得以「NAT 改写形态」等无具名来源的措辞通过比对；确需容纳 NAT 改写时，仅允许具名形态 `nat-rewritten`，且须同时提供：具名来源（该改写点的日志/记录标识）＋独立证据（映射记录逐字行随文档登记）——两者缺一即按不符处理。**落在冻结 overlay 前缀（如 `100.108.0.0/16`）⇒ 判不符**（该条回执不成立、缺口如实——overlay 源址出现在外层服务日志＝路径或记录被污染）。`physical_src_source` 口径：外部回执场景取值域新增具名 token `operator-log-reported`，**不得**借用 `endpoint-reported`/`probe-socket-getsockname`（语义不同）。
  2. **绑定到本次运行**：`probe_id` 与该窗口 `endpoint_side[]` 中 `sent=true` 同名条目**逐字匹配**；`arrived_at` 落在窗口 `[opened,closed]` 的 unix 区间；`observing_endpoint` 与该 kind 冻结解析端点一致。**窗口 unix 开/闭沿为文档必需字段（审查新增）**：证据文档必须携带 `window_open_unix`/`window_close_unix`（由 `opened_mono`/`closed_mono` 换算，换算时钟域与偏差上界随文档登记）；**缺任一 ⇒ 时间窗绑定不可机械核对 ⇒ C6 整体缺口**。任一字段不符 ⇒ 该条不成立，缺口如实（`notes[]` 记 `external-receipt=<probe_id>\|unmatched\|<reason>`）；全部相符 ⇒ 记 `matched`，`observed` 置位只发生在文档重组后的**离线独立核对步**，采集器窗口内不作任何改写。
  3. **防拼接/伪造**：日志须以**运维原件**交付（不经执行者转写/摘录/「整理」），执行者只登记 sha256 与获取渠道；核对只认逐字字段匹配，时间近似、端口推断等推断性匹配一律判不成立；伪造/变造按 S9 同级处置（停止、保全、上报）并记 `invalidated`。
  4. **不是放宽**：缺该输入时 verdict 仍为 `n2h-inconclusive`，与现行完全一致；本条只是把「缺口」细化为「具名外部输入未提供/未匹配」，证伪面（C4）与「不得假通」禁令不变。
- **DNS——已择路径①（已决，2026-09-20 人类拍板）**：启用 `dns-nxdomain-echo` 并在回执链中**消费** `echo_received`——qname 回含 marker ⇒ 该 dns 探针收执成立；miss 仍记缺口、永不单独构成通过。现行事实：默认不回读——`dns-nxdomain-echo` 默认 OFF（`n2h_collector.rs:30`、`:917`），`echo_received` 仅落档不进判定链（`:690`、`:938-958`）；设计 §4.0 dns 行「默认关闭，开启后失败仅记缺口」。**本项属代码变更**：①发送接线（默认 OFF → 证据轮显式启用）；②判定链消费（`echo_received` 进 `endpoint_side[]` 收执判定，为进程内自证路径，不依赖 `probe_receipt` seam）；实现落盘后须在**真实 resolver 上真机重新验证该回执链**（miss 即缺口的行为）方可按本条判读，派生实现项见 §6.2。路径②（外部运维日志口径）**不再采用**，此处仅作历史登记。
- **影响与边界**：stun 自证闭合（`stun-binding-response`）不变；wg_peer 收执行在具名缺席形态下记 `not-applicable:carrier-only-relay`（§2）；本条不新增设备命令、不改变 §5 AUTH 命令面字面集合（外部日志为主机/运维侧材料）。

---

## 4. C7：承认官方 `@ohos.net.statistics` 为应用内独立内核字节账本；包数恒等式已择子选项 B

- **现有条款原文含义**：C7 要求 `counters[]` 恰一元素、`side=="vpn-tun"`、字节恒等式（`delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`）**且**包数恒等式（`delta.wg.rx_packets == delta.tun.delivered_frames`）成立；两口径＝①core 自计 `rx_bytes_to_tun`（`wg_device.rs:258`，递增 `:1148`）↔ ②`/proc/net/dev` vpn-tun（`hdc-proc-fallback` 为 D 类 fallback；`tee-fallback` 仅离线 harness）。
- **观察到的问题**：E-5——①App 直读 `/proc/net/dev` 恒 `os error 13`；②`hdc-proc-fallback` 固定路径应用不可见（0027 否定结论；0025 排除「投放未成功」解释）；③窗口无法形成 ⇒ `counters=[]` 恒 ⇒ C7 恒缺。而 E-6 证明官方 statistics API 真机可返回 vpn-tun 字节值且负控如实报错。
- **拟议修改（字节面）**：承认 `@ohos.net.statistics getIfaceRxBytes/getIfaceTxBytes` 为**应用内可自取的独立内核字节账本**——独立性依据＝内核 per-iface 计数与 core 写 TUN 成功分支自增**不同源**；**恢复基线 C7 全部机械要件（审查要求）**：`counters[]` 恰一元素、`side=="vpn-tun"`、如实 `reconciled` 不修补（有界重采样 ≤3 保留）、**读取序钉死＝先 statistics 后 wg**（`notes[]` 记 `counters-read-order=statistics-first`）。字节恒等式重述为 `delta.wg.rx_bytes_to_tun == delta.statistics.rx_bytes("vpn-tun")`（方向已由前置②(i)钉死＝API rx ↔ 内核 RX 交付向）；来源注记 token `counters-source=statistics-iface`；每窗口对固定负控名 `n2h-none0` 读一次并登记错误码，负控未报错 ⇒ 该窗口计数读数不可信（`n2h-inconclusive`）。**两侧字节 delta 均 >0 方可判恒等成立**（审查建议采纳：零流量真空窗口不得判对账通过）。`/proc` 与 hdc fallback 字面保留不删（其它环境仍可用）。
- **C7 两前置（审查新增，写死为判据级）**：
  - **①单一写者前提**：恒等式成立以「vpn-tun 的唯一写者＝WG」为前提——内核接口 RX 计数计**任何**写入者；该前提登记为具名 token `c7-premise=single-tun-writer` 随文档输出。**反证处置**：窗口内出现非 WG 写入 TUN 的证据（`tun_negative.hit==true` 或其它注入/第三方写入观察）⇒ 该窗口 C7 恒等式判**不成立**（`reconciled:false`，不修补），不得以恒等数字成立顶替前提。
  - **②真机标定——两级判据（审查前置；方向级已由只读分析完成，同源级保持前置）**：
    - **(i) 方向标定——已完成（只读分析并入，2026-09-20；诊断类读数，非门证据）**：逐轮配对第 4 版 API 打点与 shell `/proc/net/dev` vpn-tun 行（0020/0021/0022/0023/0027 五轮；API 打点均早于 /proc 3.5–4.1 s，不确定度 ±0.25 s）。**同向假设**（API rx↔内核 RX、API tx↔内核 TX）五轮增量**全非负**；**全交叉假设被否定**——若 API rx 对应内核 TX，则 `/proc TX − API rx` 在 0022/0023/0027 为 **−16/−96/−120**（后读小于先读，违反计数器单调）；量级亦支持：`|API tx − /proc TX| = 0,0,80,80,80` 远小于 `|API tx − /proc RX| = 360…736`。**0020 的 576=576 不构成证据**（间隙 3.7 s、期间确有帧活动，相等只要求该向零交付）。⇒ 方向视为**已钉死**：API rx ↔ 内核 RX（交付向）、API tx ↔ 内核 TX。
    - **(ii) 同源标定——未完成（保持前置）**：两侧读数从未同刻重叠（最小间隔 3.5 s > 单帧周期 ~1 s）；API rx 序列 (560,560,560,640,720) 与 /proc RX (936…1256) 始终不重叠，每轮 376–536 B 间隙无独立解释 ⇒ 不能排除「同向但计数基不同 / 系统 map 周期刷新」。**同源标定实验判据（一次同刻对）**：两侧读数间隔 **<1 s**（最好 API 在 /proc 之后），A→B 之间注入已知 n×80 B 单向帧；判据＝`API rx(B)−rx(A) == /proc RX(B)−RX(A)` 且 tx 同式、各向非负。跨轮读数不可比（netId 每轮不同 147/148/149/150、wlan0 计数重置、vpn-tun TX 跨轮下降），仅轮内配对有效。**未完成同源标定前不得据 statistics 判 C7**（该窗口 C7 维持现行缺口口径，如实 `n2h-inconclusive`）。
- **`tun{}` 字段取值来源（审查新增，禁止 WG 自计填充）**：`delivered_bytes`＝statistics 标定方向字节；`injected_frames`/`delivered_frames`＝**无公开 per-iface 包计数来源，恒缺席**——不得以 WG 自计（`rx_packets` 或任何由 `rx_bytes_to_tun` 派生值）填充，字段级记缺口 token（与 `packets-identity=unavailable-no-public-per-iface-packet-api` 同源）——此即择 B 缺口的字段本体。
- **包数恒等式——已择子选项 B（已决，2026-09-20 人类拍板）**：包数恒等式在本平台**降级为永久登记的具名缺口**——证据 `notes[]` 恒记 `packets-identity=unavailable-no-public-per-iface-packet-api`；`reconciled` 仅由**字节恒等式**承载（`delta.wg.rx_bytes_to_tun == delta.statistics.rx_bytes("vpn-tun")`，来源注记 `counters-source=statistics-iface`，负控名固定 `n2h-none0`）；**C7 通过判定＝字节恒等式成立 ∧ 缺口注记在位 ∧ 负控如实报错 ∧ 两侧 delta>0 ∧ 前置①未被反证 ∧ 前置②(ii)同源标定已完成**（负控未报错或同源标定未完成 ⇒ 该窗口计数读数不可信，`n2h-inconclusive`）；**包级对账缺位作为残余边界永久登记，包级结论不外推**。依据：无公开 per-iface 包计数 API，且禁止用 WG 自计包数伪造独立来源（`docs/n2h-iface-statistics-probe-20260918.md:52-53`）。**子选项 A 的保留条件**：未来平台出现具名 per-iface 包计数来源（公开 API 或具名文件）且经真机验证非空、与负控区分、可与字节同窗配对时，可经同一修订程序（独立审查＋人类批准＋重登记 sha256）重新启用包数恒等式；未启用前 A 的前提不存在，不得以任何自计来源顶替。
- **为何不能沿用 tee-fallback**：①**同源**——tee 与 `rx_bytes_to_tun` 同为 core 自侧计数，不构成独立账本（设计 D8 行原文：「设备上 core 自己的 tee 与 `rx_bytes_to_tun` 同源，不构成独立口径，故必须用 /proc」，`docs/n2h-device-collector-design-20260917.md` D8 行；`n2h_collector.rs:3230` doc 明写 OFFline caliber）；②**规格已限定离线**——`tee-fallback` 仅离线 harness（设计 :156「仅离线测试；设备真机一律 /proc（或 D 类 hdc fallback 并声明）」；`COUNTERS_SOURCE_TEE_FALLBACK` `n2h_collector.rs:981`）。
- **影响与边界**：statistics 读数**只用于 C7 计数对账**，不作为任何路径/隔离成立的正证据（「API 返回值不作证据」治理条款不改、不废）；判据行字面改写（字节恒等式挂 statistics 口径、`counter_identities` 语义随择 B 调整）已列入 §6.2 实现面清单。

---

## 5. 不变项与安全边界（本版不放宽的条款）

1. **fail-closed 原则**：解析失败、端点变化、观测不符均 fail-closed（治理 `docs/native-nx-governance.md:125-126`）；v3 扩大的 5 类必需类 fatal 面不改。
2. **tun_negative 零命中仍是唯一 fail 入口**：C4 证伪优先、不降级、零帧强制 `n2h-inconclusive` 逐字不变；本版三条修订不新增任何 fail→通过 转换。
3. **禁伪造/禁重标/禁推定/禁提权**：不伪造或转写外部日志；不把 relay 重标 wg_peer；不以 `wg.ready` 推定 wg_peer present；不为读 /proc 提权或修改系统设置（0027 判据边界原文照抄适用）。
4. **API 返回值不作证据**（治理 `:126`）：statistics 读数仅限 C7 对账用途，不得延伸为端点可达性、路由正确性或隔离成立的正证据。
5. **观测窗口与常量变更仍按 G-4④ 重新登记**：`WINDOW_DEADLINE_MS`/`OBSERVE_DEADLINE_MS`/`OBSERVE_SETTLE_ROUNDS`/`MAX_PAIRING_RESAMPLES` 等一字不改。
6. **授权类别与产物效力边界**：诊断 dry-run 产物 `is_evidence:false` 永不升级为门证据；正式轮走 §三 pre-E8 pair AUTH；§1–§4 引用的诊断实证本身不构成判据。
7. **逐 socket protect 义务**记 `UNSAT/未满足` 的固定记录义务不变。
8. **verdict 三值语义与 C8 汇聚**（hilog 交叉核对、禁词自检三点固化）不变；**择 B 后 C8 相应调整为「一条加严＋具名缺口注记必需」（审查修订，消除与 §4 的自相矛盾）**：加严＝§4 的 C7 通过判定（含两侧 delta>0、前置①②、负控报错）；缺口注记＝`packets-identity=unavailable-no-public-per-iface-packet-api` 在位；`counter_identities` 语义变更随定稿落 §2-C7/C8 判据行字面；具名缺席形态窗口另受 `conclusion-scope=relay-carrier-subset-only` 必需 token 约束（§2）。
9. **carrier-only-relay 子集结论边界（已决，2026-09-20）**：该形态下即使 C1–C8 全部成立，最高登记结论＝「relay 承载拓扑子集」；**不得据该形态登记 N2-H pass**，直连拓扑须另行测量（§2）；对应不外推声明字面已随定稿落 **v4 定稿文本**（本文件）§8.1（不改已锚定的 v3.1，G-4③）。
10. **C10 触发类⑤（拓扑落配/换址即停）不放宽（审查新增）**：窗口内 `wg_peers[].endpoint` 落配或 ICE nominated 变化 ⇒ fail-closed 结束窗口（§2 机械触发条款）；具名缺席形态实际缩小了 C4 可命中面，该触发是对负证据面的补偿性守护，非新增豁免。

---

## 6. 生效程序与待决问题

### 6.1 生效程序（按既有 G-4④，不得省步）

1. 本草案修订落稿、四段式逐项完备（本文件）；
2. **跨厂商独立审查**（干净上下文 spawn；席位按评审独立性纪律避开已占用家族；审查对象＝本草案全文及其引用实证原文）——第一轮已裁定 blocker（5 项必修），本版为必修落字稿，**须重审通过后方可进入第 3 步**；
3. **人类显式批准**（生效批准；2026-09-20 对 §6.2 原四项的拍板**不得替代**本步的生效批准）；
4. 按拍板与审查必修结果定稿，**重算全文 sha256** 并登记仓B `records/`（文件名含日期与 effective 字样、带 sidecar），登记时逐字作废 v3.1 锚点 `02f96bc8e4c1a7a282fbf3cd302ce0616af367509f98c40e43439f87c26e12ff` 的据以测量效力；
5. **规范字节定位（审查新增）**：生效登记须以 **git commit 锚定**——逐字登记「生效稿唯一规范字节＝commit <hash> 中本文件内容＋其全文 sha256」，不以工作树文件为锚（避免工作文件后续字节漂移；同 v3.1 对 v2 的定位形态，G-4③）；
6. **实现面清单（§6.2）全部落盘后随定稿再审（审查新增）**：实现与判据字面一致性（含 judge 必需类白名单、C10 触发类⑤、方向与同源标定记录）为生效前置之一；
7. 首次据以测量的 campaign AUTH **逐字引用新 sha256**（G-4②）；
8. 禁词自检按现行规格 §8.2 固化三点各跑一次（生效稿、每份证据文档、每份执行记录）。
**生效状态（2026-09-20，Asia/Shanghai）**：第 1–5 步已完成——本稿经跨厂商独立审查＋复验通过（第 2–5 项已关闭、第 1 项三处微改按评审者逐字要求落定）并获人类显式批准（渠道与原文见文首生效声明）；定稿以 git commit 锚定并登记仓B `records/`（`n2h-preregistration-effective-v4-20260920.json`，带 `.sha256` sidecar），登记时逐字作废 v3.1 锚点 `02f96bc8e4c1a7a282fbf3cd302ce0616af367509f98c40e43439f87c26e12ff` 的据以测量效力（v3.1 正文不被修改，G-4③，仅保留历史审计效力）。**第 6 步未完成**：§6.2 实现面八项仍未落盘，其各项判据前置原样保留——落盘前相关形态仍按现行缺口口径如实登记，本生效稿生效不声称实现已完成。第 7–8 步按本节字面继续适用（首次据以测量的 campaign AUTH 逐字引用本生效稿登记 sha256；禁词自检按固化三点各跑一次，生效稿首跑结果见收尾声明）。

### 6.2 已决项与剩余清单（2026-09-20 人类拍板后更新；不保留已决项当开放项）

**已决（2026-09-20 拍板，本草案内已落字）**：①C7 包数恒等式**择子选项 B**（§4）；②carrier-only-relay 形态结论**仅为「relay 承载拓扑子集」、不得登记为 N2-H pass**，直连拓扑须另行测量（§2、§5-9）；③DNS **择路径①**（§3，属代码变更，实现后真机重新验证回执链）；④C6 外部回执**五字段**/绑定/防伪造、statistics 判据字面（`counters-source=statistics-iface`、负控 `n2h-none0`、窗口开/关同进程配对读取点）与 §5 不变项**按草案既有文本采纳**。

**剩余实现面派生清单（随 7/7 补齐工作流派发，不在本纯文档修订内落地；落齐前相关腿按现行缺口口径如实登记；各项附完成判据/验收）**：

1. `absent_reason` 具名 token `carrier-only-relay` 化＋四条件证据行（`carrier-cond-1..4`，两沿）＋**judge 必需类白名单放行**（审查指认：`n2h.rs:836-846` 现状对必需类 `present==false` 一律产缺口，须按 `absent_reason` 白名单＋证据行齐备且两沿一致放行）＋**闭合沿 status 快照读取与 cond 行输出者落地**（收口(d)：输出者＝core，行含 `requestId=core`，关联＝`opened_mono`↔`openedMono` 同值配对；现 close 路径仅 `paired_read`，须扩展 host seam 增加闭合沿快照）。**完成判据**：采集器源码含逐字 cond 行与闭合沿快照调用；离线 harness 断言「四行齐备⇒无缺口、缺任一/两沿不一致⇒缺口＋S18」；
2. DNS 路径①代码变更：`dns-nxdomain-echo` 发送接线启用＋`echo_received` 进判定链消费；实现后在真实 resolver 上**真机重新验证回执链**。**完成判据**：真机窗口 notes 出现 dns 收执行（echo 命中）与 miss 缺口行各一；
3. `external-receipt` 五字段 notes 行＋离线独立核对流程（含 `observed_physical_src` overlay 前缀判不符、`window_open_unix/close_unix` 必需）。**完成判据**：离线核对单测覆盖 matched / unmatched / overlay-源址判不符 三态；
4. statistics 采集落点：读取序 statistics-first、负控 `n2h-none0` 读取与错误码登记、`counters-source=statistics-iface`、两侧 delta>0 判定。**完成判据**：真机窗口配对快照行＋三行 notes 逐字核对；
5. C7 判据行改写：字节恒等式挂 statistics 口径、`counter_identities` 语义随择 B 调整、`c7-premise=single-tun-writer` 随文档输出、`tun{}` 包字段缺口 token。**完成判据**：§2-C7 判据行 diff 与字段对齐清单逐字段一致；
6. **真机同源标定（前置②(ii)；方向级(i)已由只读分析完成）**：一次同刻对实验（两侧间隔 <1 s、最好 API 在 /proc 之后、A→B 注入已知 n×80 B 单向帧）。**完成判据**＝`API rx(B)−rx(A) == /proc RX(B)−RX(A)` 且 tx 同式、各向非负；标定记录落盘并随定稿再审；
7. **C10 触发类⑤落盘＋观测期逐轮复读**（收口(b)：现观测循环每拍仅读 `peer_report()`，无 peers/endpoint 字段——须扩展 host seam 使每拍可读条件② peer 的 `endpoint`）；落配/换址 ⇒ 触发 token `c10-trigger=endpoint-landed|endpoint-changed`。**完成判据**：离线测试覆盖两触发形态＋逐轮复读命中（中途落配、闭合沿已回落 null 的场景不得漏检）；真机窗口每拍复读行可机械核对；
8. **`peer_overlay_ip`→`allowed_ips` 归属匹配＋`carrier_attached` 采集**（收口(a)(c)）：前缀覆盖匹配（恰一 peer；零/多覆盖拒绝）＋条件② peer 的 `carrier_attached` 字段采集与 `N13_WG\|carrier-attach\|kind=relay-wss` 交叉核对。**完成判据**：离线测试覆盖恰一/零/多覆盖三态与字段缺失拒绝；真机窗口 cond-2/cond-4 行字段齐备。

---

## 8. 不外推声明（v4 定稿落字）

### 8.1 carrier-only-relay 形态不外推（已决，2026-09-20 人类拍板②）

**carrier-only-relay 形态下的任何结论不外推至直连拓扑；直连拓扑判 N2-H 须另行测量。**

落字依据：§2「该形态下证据范围明确不覆盖直连路径（已决，2026-09-20 人类拍板）」条与 §5-9 指定本字面随定稿落为本节；本节为 v4 定稿落字，不修改已锚定的 v3.1（G-4③）。本节为限制性声明，不新增、不放宽任何判据。

---

**收尾声明（生效稿口径）**：本文件自 2026-09-20（Asia/Shanghai）起为生效稿（见文首生效声明与 §6.1 生效状态）；定稿落字过程只改本文件，未修改 v3.1 及其它任何既有文件、未执行设备操作；git commit 锚定与仓B 生效登记按 §6.1 第 4–5 步执行（登记件 `records/n2h-preregistration-effective-v4-20260920.json`＋sidecar）。禁词自检（现行规格 §8.2 同一模式）已在**生效稿**上重跑（2026-09-20）：**0 命中、退出码 1**——五项禁词字面在生效稿全文零出现；「N2-H pass」唯一许可形态计数实跑 **4 次**（§2 直连路径已决边界行一处（该行含两处）、§5-9、§6.2 已决②、本收尾声明自检登记行——均为登记限制/证据边界/自检记录语境，非结论声明）。本自检结果随仓B 生效登记件登记。
