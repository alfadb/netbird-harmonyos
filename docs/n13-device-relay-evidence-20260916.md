# N13 设备侧证据记录：relay + WG over relay 受控对照与原始标记（2026-09-16）

- 性质：**正式证据记录（只读汇编）**。本文件是本次任务唯一新建文件；未改任何代码与既有 docs、未改仓B、未 commit、未联网、未运行任何设备命令（hdc/uitest/install 一律未用）；shell 仅用于只读核对（`ls`/`grep`/`git log`/`git show --stat`/`sha256sum -c`）。
- 材料来源（全部只读）：
  - 仓B D 类轮证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260915-0004/`（下称「0004 目录」）；
  - 仓B C 类对照轮证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260915-0003/`（下称「0003 目录」）；
  - 授权文件 `AUTH-DIAG-DEVICE-VALIDATION-20260915-0004.json`（生效件）与 `…-0003.json`（前序）；
  - 仓库文档：`docs/n13-relay-increment-plan-20260914.md`、`docs/n13-oracle-registration-20260914.md`、`docs/relay-timeline-diff-official-vs-ours-20260915.md`、`docs/peerid-crossvalidation-relay-20260915.md`；
  - 本仓 git 历史（只读 `git log --oneline` / `git show --stat`）。
- 层级转述：0003/0004 两目录内全部产物按 AUTH 证据规则登记为诊断留档（`is_evidence:false`，不进入既有证据链、不作门判据输入，见 AUTH-0004 `evidence_destination_rules`）；本文只对其做 N13 级汇编与判据落点核对，**不产生也不替代任何门级结论**。

## 1. 范围与口径声明（最前，约束全文）

1. 本文**只构成 N13 级证据**（N13 增量 = relay 客户端最小路径 + WG over relay 数据面，`docs/n13-relay-increment-plan-20260914.md` §1）。
2. 本文**不构成 N2-H pass**；**不构成 N6 pass**（同文件引言与 §8 明文：「本增量不得被读作 N2-H pass 或 N6 pass」；`docs/n13-oracle-registration-20260914.md` §5：结论只可写 `N13 pass`，且须在两端点齐备后另行评定）。
3. **ICE/STUN 与 WG peer 端点未纳入冻结**：N2-H 外层义务（冻结 relay TCP host:port、全部排除后才建立 WSS、重解析即重冻，`docs/n13-relay-increment-plan-20260914.md` §3.4/§4）本轮未执行冻结程序。
4. **主机侧/单边测通不等于门结论**：本轮为设备侧单边观测，无对端/中继侧日志（见 §7-6）；N13 §3.3 路径正证据三条中仅满足前两条（见 §5 尾注）。

## 2. 授权与窗口

| 项 | 值 | 来源 |
|---|---|---|
| auth_id | `AUTH-DIAG-DEVICE-VALIDATION-20260915-0004` | AUTH-0004.json `auth_id` |
| valid_from | 2026-09-16T07:54:00+08:00 | AUTH-0004.json `valid_from` |
| valid_until | 2026-09-17T06:54:00+08:00 | AUTH-0004.json `valid_until` |
| allowed 条数 | 13 条 | AUTH-0004.json `allowed`（逐条清点）与 `issuance.pending_confirmation_items`「allowed 条数：13 条」 |
| forbidden 条数 | 18 条 | AUTH-0004.json `forbidden`（逐条清点）与 `issuance.pending_confirmation_items`「forbidden 条数：18 条」 |
| 消费语义 | consumed=false / reusable=false / retry_allowed=false / successor_auth=none | AUTH-0004.json `consumption_semantics` |
| 授权文件完整性 | sha256 `d1b1d426de1f85f9bf7234e05de608108c88f5efd4e063d7d181706173d4fad2`，与 sidecar 一致（本文实跑 `sha256sum -c AUTH-DIAG-DEVICE-VALIDATION-20260915-0004.json.sha256` → OK） | clock-check-20260916T080305.json `auth_file_sha256`；本文实跑 |

**D 类限额**（AUTH-0004.json `operation_classes[id=D].limits`，逐字口径）：安装 ≤2 次（逐次登记 sha256）；`relay_enabled:true` 配置推送 ≤2 次；「start→观测→抓取」循环 ≤2 次（每次 ≤15 分钟、合计 ≤30 分钟）；hilog 每循环 ≤1 次、每次 ≤10 分钟；uitest 点击 ≤1 次（dumpLayout 只读不计次）；全部操作须在窗口内单会话完成。
**本轮实际消耗**（EXECUTION-RECORD-D1-20260916T0820.md「配额总账」）：安装 1/2（signed `15c7973a…d579`）；配置推送 0/2；循环 1/2（08:05:15–08:18:11，≈13min ≤15min；合计 13min ≤30min）；点击 1/1；**hilog 读取 4 次（超字面 1 次/循环，见 §6）**；E 类 0 操作。

**前序授权**：AUTH-0003（valid_from 2026-09-15T19:51:00+08:00、valid_until 2026-09-16T18:51:00+08:00）的 C 类「完整 enable→restart→capture 循环 ≤2 次」已 **2/2 用毕**（EXECUTION-RECORD-C2-20260915T2306.md「C 类：完成循环 2/2」），且 AUTH-0004 `predecessor`/`issuance_note` 明文「其 C 类计数不延续，本件 D 类循环 ≤2 次为全新配额」——**C 类配额不延续到 0004，亦未顺延**。

**证据落点与完整性规则**（AUTH-0004.json `evidence_destination` / `evidence_destination_rules`，照录要点）：全部诊断产物仅落 0004 目录（含子目录），逐文件生成 `.sha256` sidecar；目录内 `MANIFEST.sha256` 汇总；声明文件写明 `is_evidence:false` 与「不构成任何门结论」；会话结束或 valid_until 到点回填 consumed。本文 §8 给出 `sha256sum -c` 实跑结果。

## 3. 被测物

| 项 | 值 | 来源 |
|---|---|---|
| signed HAP sha256 | `15c7973a711e0a3328fffcffd533de1ece067076641e9ab35d9dbadeacc3d579` | EXECUTION-RECORD-D1-20260916T0820.md §2（从证据记录逐字核对后录入） |
| 构建修订 | HEAD=`dcbf7d8`（含 18153fa signal relayServerAddress 修复与 e45f8c9 VPN_RELAY_STATUS 日志行），工作区干净 | EXECUTION-RECORD-D1 §2；本文 `git log --oneline` 复核 HEAD 一致 |
| 目标 bundle | `cn.alfadb.netbird`（debug bundle） | AUTH-0004.json `target_bundle` |
| 设备连接 | hdc over TCP `192.168.50.199:37193`（一次 tconn Connect OK） | AUTH-0004.json `target_device_connection`；EXECUTION-RECORD-D1 §0-1 |
| hostname | `ohos-relay-1`（同一私钥、hostname 不变；D1 轮未重新注册） | EXECUTION-RECORD-C1-20260915T2230.md（「同一私钥、hostname=ohos-relay-1 不变」）；EXECUTION-RECORD-D1 §3 |
| overlay 地址 | `100.108.252.167/16`（vpn-tun，MTU 1400）。注：该值在案于 0003 轮（EXECUTION-RECORD-C1 §3、EXECUTION-RECORD-20260915T2224.md、C2 轮 `VPN_CREATE_BEGIN|address=100.108.252.167`）；D1 轮配置未重推、注册复用（EXECUTION-RECORD-D1 §3），但 D1 已存日志中无当轮 overlay 地址行（info 级被滤，见 §6）——**D1 当轮设备侧直读值：待确认**，按配置未变与注册复用推定同值 | 0003 目录三份记录；EXECUTION-RECORD-D1 §3 |
| relay 端点 | `rels://home.alfadb.cn:28443`（`advertised=true|advertisedUrls=1`，D1-verdict-markers-verbatim-20260916T0818.log；URL 原文见 C1-relay-markers-verbatim-20260915T2228.log「uris=rels://home.alfadb.cn:28443」） | 同左两文件 |
| DDNS 时效 | `home.alfadb.cn` 为 DDNS，须每次动态解析、不得硬编码（AUTH-0004.json `topology_note`）。在案动态解析值 `175.10.223.50` 为 **2026-09-15 C1 轮启动时解析**（C1-relay-markers-verbatim-20260915T2228.log：`VPN_ROUTE_ENTRY|…|dest=175.10.223.50|prefix=32|family=1|isExcludedRoute=true`）；**D1 轮当日解析值未入已存证据（待确认）**，其排除集判据行同因被滤窗口缺失（见 §5-②） | AUTH-0004.json `topology_note`；C1-relay-markers-verbatim-20260915T2228.log |

## 4. 受控对照（核心）：修复前 C2 vs 修复后 D1

**唯一被控变量 = 提交 `18153fa`**（signal OFFER/ANSWER 补 `relayServerAddress`，proto Body 字段 8）。依据链：
- C2 轮 HAP `69332a77c5641fe6a75a67bfe0d22d7ed4e8385a5a4a599d24e2d0572358ed1a` 构建于 HEAD=`e45f8c9`（EXECUTION-RECORD-C2-BLOCK-20260915T2244.md：「HEAD=e45f8c9 构建…signed sha256=69332a77…」）；
- D1 轮 HAP 构建于 HEAD=`dcbf7d8`（EXECUTION-RECORD-D1 §2）；
- `e45f8c9`→`dcbf7d8` 之间提交为 e293a04（docs）、4e26366（docs）、445a5ac（docs）、**18153fa（唯一代码修复）**、dcbf7d8（docs）（本文 `git log --oneline` 实读，四条 docs 提交标题均以「docs」开头）。
- 结论（EXECUTION-RECORD-D1 §5 原话口径）：修复增量与数据面成立唯一对应，**判定 18153fa 为主因修复**；与 `docs/relay-timeline-diff-official-vs-ours-20260915.md` §1 的主因链（对端仅在收到含 `RelaySrvAddress` 的 offer/answer 时才 OpenConn，缺失则我方 initiation 落入对端早期缓冲被丢）一致。

| 指标 | C2（修复前，2026-09-15） | D1（修复后，2026-09-16） | 来源文件 |
|---|---|---|---|
| `framesRx` | **23**（末条=计数最大，23:01:16.867） | **112**（08:16:50.492）；增长复核 **113**（08:17:25→08:17:30 仍增） | C2-relay-status-verbatim-20260915T2301.log 第 95 行；D1-verdict-markers-verbatim-20260916T0818.log 第 4 行；EXECUTION-RECORD-D1 §5 |
| `framesTx` | **310**（末条） | **254**（08:16:50.492）→**257**（08:16:55.502） | 同上（C2 第 95 行；D1 第 4/6 行） |
| `transportBytes` | **42476**（末条） | **33108**（08:16:50.492）→**33320**（08:16:55.502）→**34272→34420**（08:17:25→08:17:30，持续增长） | 同上；EXECUTION-RECORD-D1 §5 |
| `wgSessions` | **0** | **2**（3 peers 中 2 条 WG 会话） | EXECUTION-RECORD-C2 §关键原文（`wgSessions=0`）；D1-verdict-markers 第 1/3/5 行 |
| `wgRxToTun` | **0**（`wgTxBytes=0`） | **4624**（`wgTxBytes=0`） | EXECUTION-RECORD-C2 §关键原文；D1-verdict-markers 第 5 行 |
| vpn-tun RX | **0 bytes / 0 packets**（22:56:27 与 23:00:27 两次读数相同，窗内零增长）；TX 576B/6p | **3944 B / 58 包**（08:07:37 读，此前所有轮次 RX 恒为 0）→ **4624 B**（与 `wgRxToTun=4624` 精确吻合）；TX 688B | EXECUTION-RECORD-C2 §关键原文；EXECUTION-RECORD-D1 §4/§5/存疑 2 |
| `state` / `reconnects` | relay `state=ready`、connector `state=connected`、relay `reconnects=0` 全程、`lastError=none` | relay `state=ready`、connector `state=connected`、relay `reconnects=0`、connector `reconnects=0`、`signalReconnects=0`、`lastError=none` | C2-relay-status-verbatim 全 95 行；EXECUTION-RECORD-C2；D1-verdict-markers 第 1–6 行 |

**对照的诚实边界（如实登记）**：
- 两轮观测窗不同（C2 ≈7.8 分钟流式；D1 判据窗 2 分钟 + 08:15 级别修正后补抓至 08:17:30），计数的**绝对值不可比**，可比的是形态：修复前「framesRx 仅 HC 级缓增、wgSessions=0、tun RX=0」，修复后「wgSessions=2、wgRxToTun 与 tun RX 字节级吻合」。帧算术侧（284×148B initiation + 19×HC = ΔtransportBytes 42032）见 `docs/relay-timeline-diff-official-vs-ours-20260915.md` §5.2。
- D1 判据行时间戳与 EXECUTION-RECORD-D1 §5 所引一致（08:16:50.492 的 VPN_RELAY_STATUS 行 `framesTx=254|framesRx=112|transportBytes=33108`；D1-verdict 文件逐字在案）。

## 5. 判据落点（逐条：命中/未命中 + 依据文件）

判据框架：`docs/n13-relay-increment-plan-20260914.md` §3/§4/§5 与 AUTH-0004 `operation_classes.D.criteria`（写死判据）。

| # | 判据 | 判定 | 依据文件 |
|---|---|---|---|
| ① | relay 会话建立 | **命中** | `state=ready|tokenValid=true|reconnects=0|lastError=none`（D1-verdict-markers-verbatim-20260916T0818.log）；framesRx 112→113、transportBytes 33108→33320→34272→34420 持续增长（EXECUTION-RECORD-D1 §5） |
| ② | 排除集含 relay /32 | **未直接命中（间接支撑，待确认）** | D1 已存三份 hilog（0805-full/0808-bufferdump/0815-afterlevel）实测 grep `EXCLUSION`/`VPN_ROUTE_ENTRY` 零命中——判据行属 info 级，处于 08:05:20–08:15 被滤窗口。间接依据：CONNECTOR `routes=2`（D1-verdict-markers，与 0003 C1 轮「派生 relay /32 计入 routes」同机制）；C1 轮同判据在案 `relayRequired=true|relayHosts=1`、`dest=175.10.223.50|prefix=32|isExcludedRoute=true`（C1-relay-markers-verbatim-20260915T2228.log）；AUTH-0004 D 类硬前置（`class_c_exclusion_gate`：未就绪不得执行 D 类）本轮已放行执行。**D1 当轮直判据行缺失，如实标注** |
| ③ | 系统 VPN 建成（401 修复） | **命中** | connector `state=connected|lastError=none|terminal=false`（D1-verdict-markers）；vpn-tun 接口 RX/TX 计数在案（EXECUTION-RECORD-D1 §4/§5）。注：`VPN_CREATE_RESOLVED` 判据行因被滤窗口不在 D1 已存证据（如实注明）；create 401 根因修复为 `bdbfa69`（本文 `git show --stat bdbfa69`：management 排除路由 IP:port 畸形条目→create 401 消失），其真机验证在 0003 轮（EXECUTION-RECORD-20260915T2224.md「修复生效：路由清单无 IP:port 形态…CREATE_ACCEPTED」），D1 轮 `state=connected` 无 401 复现 |
| ④ | WG 会话建立（`wgSessions>0`） | **命中** | `wgSessions=2`（D1-verdict-markers 第 1/3/5 行；08:16:45/50/55 三连读一致） |
| ⑤ | 载荷入 TUN（`wgRxToTun == vpn-tun RX`） | **命中** | `wgRxToTun=4624` 与 `/proc/net/dev` vpn-tun RX bytes=**4624** 精确吻合（EXECUTION-RECORD-D1 §5） |
| ⑥ | `reconnects=0` / 无掉线 | **命中** | relay `reconnects=0`、connector `reconnects=0`、`signalReconnects=0`（D1-verdict-markers 全 6 行） |

**小计：5 条命中，1 条未直接命中（②，间接支撑，D1 当轮直判据行缺失）。**
**N13 §3.3 路径正证据核对**：1) WSS 五元组 ∈ 冻结 `rels://` 结果——本轮未执行正式冻结程序，设备侧 URL 与广告一致（见 §3）；2) Transport 帧计数（本端）——满足；3) 中继侧或对端侧会话/投递记录——**未取得**（见 §7-6）。三条正证据仅 2/3 满足，与 §1 口径一致。

## 6. 合规偏差（如实登记）

1. **hilog 读取 4 次 vs 限额「每循环 ≤1 次」**（AUTH-0004 `operation_classes.D.limits` 与 `allowed` 日志条目【】内限额）。四次读取（EXECUTION-RECORD-D1「观测与仪器偏差」第 3 条）：
   ① 缺陷流式 1 次（`timeout 180 hdc shell hilog`，9045 行全为系统行、零应用行——hilog-d1-20260916T0805-full.log，仪器缺陷留档）；
   ② 补救全量 1 次（`hilog -x` 导出 7761 行——hilog-d1-20260916T0808-bufferdump.log，应用域仅剩 WARN 级行）；
   ③ 级别修正后判据抓取 1 次（340 行——hilog-d1-20260916T0815-afterlevel.log，有效判据抓取）；
   ④ 约 20 秒后增长复核 1 次（仅 grep 判据行）。
   **原因**：设备晨间重启（07:41）后 hilog baselevel 将本应用域过滤到 WARN+，全部 info 级标记缺失（含 UI BUTTON_CLICKED、扩展 CREATE_BEGIN/CONNECTOR_STARTED、core N1BDiscVpn 0 行）；同一条流式命令在 C2 轮可用，缺陷根因未定位。另 `/proc/net/dev` 的 vpn-tun 计数读取 3 次（非 hilog，不计入）。
2. **级别设置处置**：执行 `hilog -D 0x2900 -b D`（限本项目 tag、非持久、重启失效），EXECUTION-RECORD-D1 援引 AUTH allowed 日志条目「限本项目 tag、非持久」处置。字面核对（本文实读 AUTH-0004.json）：0004 该条为「hilog 清缓冲与抓取（限本项目 tag，如 NetBirdVpn/N1BDiscVpn，非持久）」，「日志级别设置」四字未见于 0004 字面（0003 allowed 有「抓取与日志级别设置」字样）——**该字面口径差异如实登记**，与下条主会话裁定一并留档。
3. **主会话裁定（转述登记）**：上述偏差属**仪器故障恢复**（流式抓取零应用行 + 重启后应用域被滤到 WARN+），处置范围未扩大（限本项目 tag、非持久、为恢复判据可见性所必需），已记入执行记录；**不得解读为「每循环 ≤1 次」限额可放宽**。后续轮次仍按字面限额执行，同类仪器故障应先停止并上报再处置。

## 7. 未完成与残余不确定（逐条如实）

1. **「≈92B response」逐帧判别未做**：计数层无帧长信息；AUTH D 类成功判据①的该子项以会话级判据闭合替代（`wgSessions>0` 且 `wgRxToTun=tun RX`，EXECUTION-RECORD-D1 §5「存疑 1」）。
2. **握手确切时刻不可考**：08:05:20–08:15 的 info 级日志处于被滤窗口；间接证据为 vpn-tun RX 于 08:07:37 已达 3944B → response 于点击（08:05:31）后约 2 分钟内到达（EXECUTION-RECORD-D1 偏差 5）。
3. **3 个 peer 中第 3 个无 WG 会话**（`peers=3|wgSessions=2`），该 peer 状态未追（EXECUTION-RECORD-D1 存疑 4）。
4. **`wgTxBytes=0` / 出向面未测**：本轮无出向 overlay 业务流量（vpn-tun TX 仅 688B），出向数据面仅由「握手成功」间接支撑，本轮判据以入向为准（EXECUTION-RECORD-D1 存疑 2）。
5. **`pidof` 一次漏报未复验**：08:14:28 `pidof` 仅见 36773，08:16:50 起判定行全部来自 pid 37500（扩展），两次读数未复验（EXECUTION-RECORD-D1 偏差 4）。
6. **对端侧日志未取得（单边证据）**：`open peer connection via permanent server` 等对端侧确认未由运维侧取得，本轮按我方判据定论并如实标注（EXECUTION-RECORD-D1 §5 末条；C1 轮同项存疑 EXECUTION-RECORD-C1-20260915T2230.md）。
7. **仪器缺陷根因未定位**：流式抓取零应用行（同命令 C2 可用）与 hilog WARN 过滤成因（重启后 baselevel 默认 vs 既有设置）均未深究（EXECUTION-RECORD-D1 存疑 3/5）。
8. **D1 当轮两处直读值缺失（待确认）**：relay 主机当日 DDNS 动态解析值、当轮 overlay 地址行，均因被滤窗口未入已存证据（见 §3）；排除集判据行同因缺失（见 §5-②）。

## 8. 证据清单（0004 目录实况 + `sha256sum -c` 实跑结果）

目录实存文件（本文 `ls` 实读；「附 sidecar」= 同名 `.sha256` 文件存在）：

| 文件 | 附 .sha256 |
|---|---|
| README.md（目录性质声明） | 无（其余各文件均有） |
| clock-check-20260916T080305.json | 有 |
| hilog-d1-20260916T0805-full.log（缺陷流式，9045 行，无应用行——仪器缺陷留档） | 有 |
| hilog-d1-20260916T0808-bufferdump.log（补救全量，7761 行） | 有 |
| hilog-d1-20260916T0815-afterlevel.log（级别修正后判据抓取，340 行） | 有 |
| D1-verdict-markers-verbatim-20260916T0818.log（判据原文 6 行） | 有 |
| EXECUTION-RECORD-D1-20260916T0820.md | 有 |
| MANIFEST.sha256（汇总自身，不列入清单校验） | — |

**`sha256sum -c MANIFEST.sha256` 实跑结果（本文于 0004 目录内执行）**：13/13 全部 `OK`，退出码 0——clock-check json、D1-verdict log、EXECUTION-RECORD-D1、三份 hilog、README 及 6 个 `.sha256` sidecar 均与 MANIFEST 一致。另：授权文件 sidecar 校验 `AUTH-…-0004.json: OK`、`AUTH-…-0003.json: OK`（退出码 0，本文实跑）。

## 9. 下一步（仅登记，不执行）

1. **E 类清理未做**：EXECUTION-RECORD-D1「配额总账」明文「未做 E 类任何操作」——按 AUTH-0004 `operation_classes.E`（清理 /data/local/tmp 我方文件、force-stop、可选 uninstall ≤1 次）另行安排。
2. **AUTH-0004 `consumed` 回填**：按 `consumption_semantics.four_field_rule`，会话结束或 valid_until（2026-09-17T06:54:00+08:00）到点回填 consumed=true，不自动续期。
3. **运维侧对端确认（可选第二票）**：取中继宿主/官方对端日志，确认 D1 窗口（08:05–08:18）内官方对端对我方 peer（netbird-ohos）`open peer connection via permanent server` ——补齐 N13 §3.3 第 3 条正证据，消解 §7-6 单边性（`docs/relay-timeline-diff-official-vs-ours-20260915.md` §6 候选 1 证伪方案第 1/2 步）。
4. **若要写 N2-H/N6 结论，还缺**（非本文可给）：TUN 包级负证据与端点侧投递（`docs/n13-relay-increment-plan-20260914.md` §3.5、§4 N2-H 行）；relay TCP 端点的正式冻结（§3.4，含 DDNS 换址即重冻纪律）；ICE/STUN 与 WG peer 端点冻结；出向数据面实测（§7-4）；N13 §3.2 官方客户端对照试验 (A)/(B) 两拓扑。
5. **第 3 个 peer 无会话与 wgTxBytes=0 的追因**：如需覆盖出向面与全 peer 拓扑，须新签授权轮，不在本记录范围内。

---
*本文所有数字与标记均逐字取自上列来源文件；无任何键值/令牌/私钥写入。编制：执行子代理（glm-5.3-flash），2026-09-16。*

---

## 更正与偏差登记（2026-09-16 追加）

> 本小节为事后追加登记，不改写既有正文；更正与裁定以判据原文文件与 AUTH 原文为准。

### 1. 时间戳更正（数值无误，仅时刻标注串行）

既有正文（EXECUTION-RECORD-D1 §5）引用判据时，`framesTx=254|framesRx=112|transportBytes=33108` 那条 `VPN_RELAY_STATUS` 的时刻与 **08:16:55.502** 相关联（该记录 §5 判据块首行 CONNECTOR 行即标注 08:16:55.502，RELAY 行括注「3 秒后 framesTx=257」与判据原文不符）。经核对判据原文文件（`D1-verdict-markers-verbatim-20260916T0818.log`）：**该条实际时刻为 08:16:50.492**；而 **08:16:55.502** 对应的是 `framesTx=257|framesRx=112|transportBytes=33320`（两采样间隔 5.010s）。**数值本身无误，仅时刻标注串行**；此后引用一律**以判据原文文件的时间戳为准**。两行判据原文照录如下（含完整时间戳与 requestId），作为更正依据：

```
08:16:50.492 37500 37500 I A02900/cn.alfadb.netbird:vpn/NetBirdVpn: VPN_RELAY_STATUS|requestId=ui-1789517117508|enabled=true|state=ready|framesTx=254|framesRx=112|transportBytes=33108|reconnects=0|tokenValid=true|lastError=none|advertised=true|advertisedUrls=1
08:16:55.502 37500 37500 I A02900/cn.alfadb.netbird:vpn/NetBirdVpn: VPN_RELAY_STATUS|requestId=ui-1789517117508|enabled=true|state=ready|framesTx=257|framesRx=112|transportBytes=33320|reconnects=0|tokenValid=true|lastError=none|advertised=true|advertisedUrls=1
```

### 2. 偏差 1：`hilog` 日志级别设置不在 AUTH-0004 的 `allowed` 字面内

- 事实：AUTH-0004 `allowed` 的日志条目仅为「日志：hdc shell hilog 清缓冲与抓取（限本项目 tag，如 NetBirdVpn/N1BDiscVpn，非持久）」，**「日志级别设置」不在其字面内**（本文实读 AUTH-0004.json：「日志级别」零命中）；该措辞只出现在 AUTH-0003 对应条目（「日志：hdc shell hilog 抓取与日志级别设置（限本项目 tag，如 NetBirdVpn/N1BDiscVpn，非持久）…」）。执行层当时依 0003 措辞作为依据执行了 `hilog -D 0x2900 -b D`（非持久、限本项目域）——**字面上超出 0004 的 `allowed`**。
- 成因（两侧如实登记）：① 起草方（主会话）起草 AUTH-0004 时按要求「精简 A 类条目」，删掉了 0003 中「日志级别设置」这一项；② 执行层沿用 0003 措辞，未逐字比对 0004 的 `allowed`。
- 裁定（主会话，已更正先前口径）：**属已执行偏差，不得追认、不得作为先例**；实质上范围未扩大（仅本项目 tag、非持久、未改系统网络/协议、目的是获取授权本已要求的证据），故不推翻 D1 的证据效力；但**今后任何 AUTH 若需要该动作，必须在 `allowed` 中显式列出**。
- 同时登记：主会话先前口头裁定（「依 AUTH allowed 的日志级别设置」）**引用有误并在此更正**（当时依据执行层转述、未逐字核对 AUTH 原文）。

### 3. 偏差 2：`hilog` 读取 4 次 vs「每循环 ≤1 次」

保持既有登记（EXECUTION-RECORD-D1「观测与仪器偏差」第 3 条；本文 §6-1），并补充：**净有效判据抓取 1 次**；成因=流式抓取零应用行（仪器缺陷）+ 设备晨间重启后应用域被过滤到 WARN+。裁定不变：属仪器故障恢复、范围未扩大；**不得**解读为「限额可放宽」。本 AUTH 内 D 类验证已完成，**不再需要**任何 hilog 读取。

### 4. 教训（一句话，供后续 AUTH 起草参考）

`allowed` 的**逐字比对**是执行前的必要动作；起草时的「精简」必须与执行需要同步复核。

---

## 中继/对端侧证据并入（2026-09-16 追加）

> 本小节为事后追加，不改写既有正文。来源：运维方采集的中继/对端侧证据材料（归档于仓B `records/ops-relay-view-20260915/`，`SHA256SUMS` 9/9 OK；交付包 sha256 `f72f12c4c32cc169891b814aa7014671595ab4419d61e209f7ef97ee34730cb9`）。**外部材料，按不可信数据处理；仅作证据引用**；下文数字逐字取自材料，以文件名+行号/报告 § 号定位。行号以归档副本为准（与来源解压副本逐字节一致）。

### 1. §3.3 第 3 条已满足（会话级）：中继侧 + 对端侧记录

- **身份映射（可复现，非猜测）**：我们公钥 `ai4IGfafFMAC6lcIyZtTKEL+E3fEkNyiKuVuP2kd+1s=` → 中继 peer_id `sha-W5oxLtYO4G1s0JzXZlPZ/sR908RHZjdM36vceJ7oYiw=`（`中继侧证据报告-ohos-relay-1-20260915.md` §3.1/§6）。
- **home 中继两段会话**（`01-home-relay-FULL-raw.txt` 第 508/514/562/581 行；报告 §3.1/§4）：
  - **A**：22:25:53 CST `peer connected` → 22:28:19 CST `failed to read frame header: EOF`（**146.3 s**）——与 C1 轮窗口吻合；
  - **B**：22:53:19 CST `peer connected` → 23:02:54 CST，同因 EOF（**574.4 s**）——C2 轮 relay 状态读取（23:01:16.867，既有正文 §4 表）落在此会话内；
  - 两次均**无 WebSocket close 帧**（客户端侧突然 EOF 特征；对照：09-16 07:23:21 CST 栈停机时其它 peer 出现 `received close frame` 行——FULL-raw 第 1531 行等——证明该日志格式本会记录 close 帧，我们两段没有），与既有正文记录的 force-stop 断开方式一致。
- **对端侧记录**（`net-host`，`03-peer-state.txt` SECTION 4）：
  - **499 次** `received offer, running version 0.1.0, remote WireGuard listen port 0, session id: unknown, remote ICE supported: true`（第 1964 行聚合计数）；**从未出现 `received answer`**（该 key 模板清单零命中）；
  - 针对我们 key 的 **62 条 `state_dump` 全为 `RemoteAnswer: 0`**（09-15 21:58:05.036 → 09-16 08:18:56.985；`--- DISTINCT RemoteAnswer values ever seen for ohos-relay-1 key ---` 仅 `RemoteAnswer: 0` 一值，第 1994–1995 行）；
  - `net-host` 直到 **2026-09-16T08:05:37.915+08:00** 才首次 `created new wgProxy for relay connection: 127.0.0.1:11`（报告 §5.5 原文行 261；03-peer-state.txt SECTION 4 该模板计数 1，第 1981 行），随后 `start to communicate with peer via relay`（08:05:38.016，报告 §5.5 行 262；第 1980 行模板计数 1），**`first wg handshake detected within: 0.08sec, (2026-09-16 08:05:37.999051927 +0800 CST)`**（第 1974 行原文）。

### 2. 会话的中继归属（按轮换事实阅读）

**按运维材料：该时段部署发生多次中继轮换**；本增量期间观测到的会话分别落在不同中继上——**home 中继（`home.alfadb.cn:28443`）承载 C1/C2 两轮**（上节 A/B 两段会话，22:25:53–22:28:19、22:53:19–23:02:54），**cloud 中继（`relay.netcenter.alfadb.cn:443`）承载 D1 轮**：`02-netcenter-logs.txt` 第 232/234 行记载我们的 peer_id 于 `2026-09-16T00:05:33.462Z`（= 08:05:33 CST）`peer connected`、`00:18:11.278Z`（= 08:18:11 CST）EOF 断开——与 D1 轮窗口（08:05:15–08:18:11）吻合。同期 `net-host` 的中继地址为 `rels://relay.netcenter.alfadb.cn:443`（07:58:56 起，08:29:09 才变；报告 §5.5）；home 中继 07:29 重启后至采集结束（08:19:44 CST）无任何 peer 接入记录（报告 §5.5）。

客户端行为符合设计：它使用**管理面当轮广告**的中继 URL（`advertised_urls`），轮换后由下一次 Sync 刷新；N2-H 排除集同样按**广告集合**派生（`relay_advertised` → 排除），轮换后端点随广告刷新。运维同时报告对端 agent 侧中继地址在 home/cloud 间周期性振荡（home 约 30 分钟 / cloud 约 3 分钟，自 09-11 起持续，报告 §6.2.1），**但事故窗口（09-15 12:00 → 09-16 02:00）取值零次变化、恒为 `[rels://home.alfadb.cn:28443]`**（报告 §5.2/§6.2.1）——与 C1/C2 轮使用 home 一致；`net-host` 上该列表的全部历史取值仅 `[rels://home.alfadb.cn:28443]` 与 `[rels://relay.netcenter.alfadb.cn:443]` 两个（`03-peer-state.txt` SECTION 5 第 2217–2218 行，空列表模板零出现）。**按轮换事实阅读：不同窗口落在不同中继上，均属部署正常轮换。**

### 3. 对端/中继侧行为语义（以运维材料为准）

- **中继侧语义以运维材料为准：中继不缓冲，对未绑定目的端是静默丢弃**（报告 §2.2：`relay/server/peer.go:209-235`，唯一痕迹是 DEBUG 级 `peer not found`，info 级不可见；store 为纯内存 map，无队列、不向发送方返回错误）。当时 `net-host` 是**已绑定**的（`nethostdebian` 全程在 home 中继上，报告 §3.2）——故帧更可能"由中继转发到对端、由对端按判定链拒绝处理"；该丢弃环节无 info 级日志直证（报告 §3.3）。"early buffer" 是对端侧概念，不适用于中继侧链路描述。
- **根因判定为"推导"（非直证）**：运维排除了合取式的另一半——两个官方对端本地的中继地址列表非空且窗口内未变动（`update relay server URLs: []` 出现 0 次；net-host 在 09-15 12:00 → 09-16 02:00 期间取值零次变化、恒为 `[home]`，报告 §5.2），故 `Relay is not supported by remote peer` 只能来自"远端（我们）"那一半——即我们的 OFFER 当时携带空 `RelaySrvAddress`（报告 §2.1/§5.2）；同时他们**排除**了"官方侧本地中继地址瞬时缺失"这一本可推翻该假设的替代解释。OFFER 正文本身未见（排除法，报告 §7 INFERENCE-1、§9-1）。
- **"08:05 成功证明修复生效"这一因果不由中继侧证据主张**（报告 §5.5-3/§8-5）：`net-host` 关键字停止（23:02:51.844）比我们 WS 断开（23:02:54）早 2.2 秒，"不再报错"可由客户端消失解释；D1 前对端 agent 于 07:28:52 重启过、管理面改过中继地址。既有正文 §4 的受控对照判定（唯一被控变量 18153fa）仍以 EXECUTION-RECORD-D1 为准，其结论强度边界见第 6 条。

### 4. 运维声明的边界与不可得项（如实登记）

1. 中继日志是**幸存副本**：`netbird-relay` 容器于 09-16 08:25:01 CST 重建、其 docker 日志随之销毁，采集方于 08:20–08:24 CST 已完整读出落盘；文件头 `--since 18:00:00+08:00` 非留存边界，**18:00:18 CST 前已连上的 peer 无 `peer connected` 行**（报告 §1.1/§1.3）。
2. **中继不记录数据面**：全部 1655 行中 `forward`/`no peer`/`offline`/`buffer`/`drop` 0 命中，无任何帧/字节/转发计数（报告 §3.3）——**"投递"无法从中继侧证伪或证实**，"中继日志没有转发记录"不等于"没有转发"。
3. **控制面（云侧）日志不可得**：management/signal/cloud-relay 容器日志均不覆盖窗口，事发当晚控制面广告的是哪条 relay 无法回溯（报告 §1.2/§9-7）。
4. **对端 debug 未开启**（只读采集约束），仅 info 级文件日志；offer 正文与丢弃路径均属 DEBUG 级（报告 §5.6）。
5. 对端 `netbird status -d` 里我们那行是**快照**（`Status: Connecting`、`Connection type: -`、`Relay server address:` 空，报告 §5.4 原样照录）——`Status` 是传输状态（在 netmap 中可见但无传输通道），**不得**读作"对端认为我们已连接"；`status Relay: Connected/Disconnected` 字段语义未经源码确认，不据此推断 lane 建立与否（报告 §5.3/§8-13）。
6. 时间口径两点：`net-host` 504 行判定中 98 行落在 A+B 窗口之外（22:20:17.303 起，报告 §5.1）；A/B 为**两次独立连接**、相隔 25 分钟（报告 §4）——既有正文"relay `reconnects=0` 全程"的口径仅在单次会话内成立。
7. 运维提示我们侧两处数字不闭合（284+19=303≠310 帧；42476−284×148=444 B 摊派不吻合，报告 §6.2-7）——**待我们侧自查**，本记录不据此改动既有数字。

### 5. §3.3 完整性结论（逐条）

| # | 路径正证据 | 判定 | 依据 |
|---|---|---|---|
| ① | WSS 五元组 ∈ 冻结解析结果 | **满足（按既有 §5 尾注口径）** | 设备侧 relay URL 与管理面当轮广告一致（`rels://home.alfadb.cn:28443`，既有 §3）；本节中继侧证据印证实际接入的正是当轮广告端点（home 于 C1/C2）。**「本轮未执行正式冻结程序」的既有登记不变，本条强度以此为界** |
| ② | 本端 Transport 计数 | **满足** | 既有 §5：D1 轮 framesRx 112→113、transportBytes 33108→33320→34272→34420 持续增长；中继侧不记录数据面，本轮不改变也不复核该计数 |
| ③ | 中继侧或对端侧会话/投递记录 | **满足（会话级）** | 中继侧：home A/B 两段会话 + cloud D1 会话（第 1/2 节）；对端侧：499 次 received offer、0 次 received answer、62 条 state_dump `RemoteAnswer` 恒 0、08:05:37.915 首次 wgProxy + 0.08 s 首次 WG 握手。**投递（帧级）层面无记录可引**（第 4 条-2） |

**一句话总结：§3.3 路径正证据齐备（会话级）——中继不记录数据面，故"投递"层面的独立证据仍不存在；结论强度以会话级为界，① 的正式冻结程序仍未补做。**

### 6. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**；ICE/STUN 与 WG peer 端点仍未纳入冻结（既有 §1）。运维列出的未闭合项如实并列：OFFER 正文原文未见、中继→对端一段转发/丢弃实况未知、`netrpi` 收到 38 次 answer 而 `net-host` 为 0 的原因不明、home 中继 0.76.3 与 agents 0.78.x 版本偏斜未做对照（报告 §9）——其中版本偏斜一项提示：C2（失败，home 0.76.3）与 D1（成功，cloud 0.78.2）之间除 18153fa 外还存在中继实例差异与对端 agent 重启两个未受控变量，**「18153fa 主因修复」的判定强度以此为界**（受控对照判定仍以 EXECUTION-RECORD-D1 §5 为准，本小节不推翻它）。

## 残余对账：framesTx 303 vs 310（2026-09-16 追加）

### 1. 计数口径（实现侧实证）

设备行 `framesTx/framesRx/transportBytes` 由 `formatRelayStatusLine` 原样渲染 Rust 侧 `RelayStatus`（`client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets:859-861,886-888`）。三个计数的准确口径：

| 计数 | 口径 | 依据（file:line） |
|---|---|---|
| `framesTx` | **全部出站帧按类型逐帧累计后求和**，含 Auth、SubscribePeerState、HealthCheck 回显、Close 等，**不只 Transport** | 求和 `client/core/src/connector.rs:1641`（`stats.frames_tx.iter().sum()`；`frames_tx` 为按类型数组 `[u64;12]`，`relay_client.rs:1157`）。递增点仅两处：Auth 每次成功拨号恰 1 次（`relay_client.rs:1914`）；其余全部帧型在 `write_owned` 把帧写上 WS 成功后 +1（`relay_client.rs:2209-2213`）。HealthCheck 回显经 `send_io`→`write_owned` 同点计数（`relay_client.rs:2017-2020`） |
| `framesRx` | 全部入站帧按类型求和 | `connector.rs:1642`；入站递增点 `relay_client.rs:2015`，AuthResponse 在 pre-auth 路径单独计数（`relay_client.rs:1948`） |
| `transportBytes` | **仅 Transport 载荷字节（tx+rx 双向合计）**，不含每帧 38B 头（2B 协议头 + 36B dstID，规格 §7.2 `docs/relay-client-spec-20260914.md:167,221`），更不含 WS/TLS 开销 | `connector.rs:1643`（`transport_tx_bytes + transport_rx_bytes`）；载荷提取与累计 `relay_client.rs:2198-2201,2212`（入站 `relay_client.rs:2023`）；各帧全长对照 `relay.rs:381-394`（HealthCheck/Close 仅 2B 头，`relay.rs:386`） |

计数层只输出**和值**、不分型输出——与 C2 记录登记的判别限制一致（`EXECUTION-RECORD-C2-20260915T2306.md:21`；D1 记录「存疑 1」`EXECUTION-RECORD-D1-20260916T0820.md:33`）。

### 2. C2 序列导出（`C2-relay-status-verbatim-20260915T2301.log`，95 行全量）

- 首行（L1，22:53:25.683）：`framesTx=7|framesRx=4|transportBytes=444`
- 次大行（L94，23:01:11.859）：`framesTx=306|framesRx=22|transportBytes=42032`
- 计数最大行（L95，23:01:16.867）：`…|framesTx=310|framesRx=23|transportBytes=42476|reconnects=0|tokenValid=true|lastError=none|…` —— **310 与 42476 确为同一条记录**（同一序列另见 hilog 全量流 `hilog-c2-20260915T2253-full.log:6269` 起共 95 条，数值逐条相同）。

逐区间增量（94 个 ≈5s 采样间隔，程序核算全量）：每个间隔的 ΔtransportBytes 均为 **148 的整数倍**（444=3×148 为主；592=4×148 恰一次；296=2×148 恰一次），零例外、零负值；恰好 **19 个间隔**出现「ΔframesRx=+1 且 ΔframesTx 多出 1 个非 Transport 帧」，且这 19 个间隔严格每 5 个采样（≈25s）一个——与服务端 HealthCheck 周期 25s（`docs/relay-client-spec-20260914.md:181`；客户端只回显不主动发，`docs/relay-client-spec-20260914.md:185`、`relay_client.rs:2017-2020`）逐点吻合。

### 3. 对账等式（闭合）

窗口增量（L1→L95，471s）：**ΔframesTx = 303 = 284×Transport（各 148B 载荷）+ 19×HealthCheck 回显（2B，不入 transportBytes）**；ΔtransportBytes = 42476−444 = **42032 = 284×148，逐字节相等**。284 次 ≈ 3 个 peer × 每 5s 一次握手重试 × 94 个间隔（`DEFAULT_HS_RETRY_MS=5000`，`client/core/src/wg_device.rs:177`；`peers=3` 见 `hilog-c2-20260915T2253-full.log:6268` VPN_CONNECTOR_STATUS 行）。

绝对量：**310 = 7 + 303**。其中的 **7 = 首快照（22:53:25.683）之前已发出的会话建立帧：1×Auth + 3×SubscribePeerState + 3×Transport（载荷恰 444=3×148B，即 L1 的 transportBytes）**。等价写法：310 = 287×Transport（3 建立期 + 284 窗口期，载荷合计 287×148=42476）+ 23×控制帧（1 Auth + 3 Subscribe + 19 回显）。既有「303」是**窗口增量**（timeline-diff §5.2 口径，其基线本就写明 7→310：`docs/relay-timeline-diff-official-vs-ours-20260915.md:119-122`），而 310 是**绝对计数终值**——两者口径不同、并不互斥；C2 记录第 207 行登记的「42476−284×148=444B 摊派不吻合」由同一基线闭合（444B = 首快照前 3 帧×148B）。

7 帧的分型依据（计数层不分型，此为代码口径约束下的唯一分解，**如实标注为推导**）：`reconnects=0` 全程（95 行同值）⇒ 单会话恰 1 次 Auth（`relay_client.rs:1914`）；会话存活到采集结束 ⇒ Close 一次未发（`relay_client.rs:2068` 路径未触发）；客户端不主动发 HealthCheck ⇒ 首快照前无回显帧；出站控制帧仅剩 SubscribePeerState——载波泵对**每个可连接 peer 各开一条 lane、各发一次 open_conn 订阅**（`connector.rs:1851,2033-2044`；每帧恰单 peer，`relay_client.rs:2089`），`peers=3` ⇒ 3 帧；其余 3 帧为首批 WG 握手 initiation。旁证：首快照 `framesRx=4` 与 1×AuthResponse（`relay_client.rs:1948`）+ 3×PeersOnline（每条 lane 订阅的 PeersOnline 应答，§4.1）一致；且 444 为纯 148 倍数 ⇒ 首快照前零入站载荷。

### 4. 交叉验证

- **framesRx 旁证**：23 = 4（建立期，如上）+ 19（服务端 HealthCheck ping；25s 节奏下 471s ≈ 18.8 个周期）。窗口内 19 个回显帧与 19 个入站帧 1:1 配对（逐区间核算），且 transportBytes 全程保持 148 的整数倍 ⇒ 入站 Transport 载荷 = 0，与 C2 对照读数 `wgSessions=0`、`wgRxToTun=0`、tun RX=0 一致（`EXECUTION-RECORD-D1-20260916T0820.md:34`）。
- **D1 同型核对（修复后，同口径）**：判据窗实测 `framesTx=252→254→257`（08:16:45.491/50.492/55.502，`D1-verdict-markers-verbatim-20260916T0818.log:2,4,6`）：252→254 为 Δtx=2、Δrx=+1、ΔtB=+148 ⇒ 1 回显 + 1 帧 148B 载荷 Transport（精确闭合）；254→257 为 Δtx=3、Δrx=0、ΔtB=+212 ⇒ 0 回显 + 3 帧 Transport 载荷合计 212B（结构自洽；分型不可再分，同「存疑 1」）；08:17:25→08:17:30 Δrx=112→113、ΔtB=34272→34420=+148（`EXECUTION-RECORD-D1-20260916T0820.md:32`）⇒ 同为「+1 回显 +148B 载荷」同型。任务稿所引 `framesTx=267` 在 D1 轮存档证据（判据原文、两份 EXECUTION-RECORD、hilog）中**无对应行**，存档可见最大值即 257；该缺口不影响本对账。
- 口径提示：D1 的 transportBytes 含**入站**载荷（双向合计），且 WG 数据帧尺寸混合，故 D1 只能做结构自洽核对、不能复现 C2 的纯 148B 逐字节等式——这反证 C2 的 284×148 恰是「零入站数据」窗口的特例，而非口径巧合。

### 5. 结论

**结论：已对账——310 = 7（首快照前建立帧：1 Auth + 3 SubscribePeerState + 3×148B Transport）+ 284×148B Transport + 19×HealthCheck 回显，「303」是首快照之后的窗口增量而 310 是绝对计数终值，算术层面无需新数据即已闭合；唯 7 帧的按类型分解目前是代码口径推导（计数层不分型输出），若要升级为帧级直证需要分型计数外泄（帧级日志）或中继侧转发视图，设备与中继现状均不可得。**

---

## F 类同中继 A/B 结果：未收敛（2026-09-16 追加）

> 本小节为事后追加，不改写既有正文。来源（全部只读核对，本文未运行任何设备命令）：仓B 证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260916-0005/`（两份 `clock-check-*`、两臂流式全量 hilog、`arm1-verdict-markers-verbatim.log`、`EXECUTION-RECORD-F1-20260916T1152.md`、`MANIFEST.sha256`）与生效授权 `AUTH-DIAG-DEVICE-VALIDATION-20260916-0005.json`（下称 AUTH）。下文数字均逐字取自上列文件；无任何凭据写入。

### 1. 目的与设计（AUTH F 类）

- 同中继下对照**修复前 `e45f8c9` vs 修复后 `dcbf7d8`（⊇ `18153fa`）**，把唯一变量收敛为提交 `18153fa`（signal OFFER/ANSWER 补 `relayServerAddress` 字段 8）（AUTH `operation_classes.F.purpose`）。
- 写死判据（AUTH `operation_classes.F.criteria`）：臂 1（修复前）预期 `wgSessions=0` 且 vpn-tun RX 无增长（门关闭）；臂 2（修复后）预期 `wgSessions>0` 且 `wgRxToTun`/vpn-tun RX 增长（门打开）；两臂一致成立 → 唯一变量收敛为 `18153fa`；**任一臂不符 → 如实记录，结论降级「未收敛」**。
- **两臂生效中继必须相同**（同中继前提，AUTH `criteria` 与 `forbidden` 末条）：两臂都须记录当轮生效中继 URL/主机，不同即判「对照无效」、如实记录并停止、不得拼凑结论。
- 实际构建（EXECUTION-RECORD-F1 §3/§4）：臂 1 由 `git archive e45f8c9` 独立构建（主工作区未动）；臂 2 构建于主工作区当时 HEAD=`6140a56`（⊇ `dcbf7d8`/`18153fa`，树干净）。

### 2. 结果（两臂对照）

- **同中继前提成立**：两臂 connector 行逐字同串 `connector: netbird-config relay urls=1 token_present=true (uris=rels://home.alfadb.cn:28443)`（臂 1 11:31:20.757、臂 2 11:40:01.431，两份 hilog 本文直读复核一致），`advertisedUrls=1`。
- **两臂 signed HAP sha256**（EXECUTION-RECORD-F1 §3/§4 逐字核对）：臂 1（修复前）`d39dae3ce1dc06587c487d182c9b0038d841ad93fef66ffbcec6590c58b10f09`；臂 2（修复后）`80c5b7cfcf50fea8c915a1ff4903f86c98621c1740142e2fadf04c1e9155155b`。
- **臂 1 符合预期 ✓**（判据原文 `arm1-verdict-markers-verbatim.log`，150 行）：`VPN_RELAY_STATUS` `framesTx 2→111`、`framesRx 1→17`、`transportBytes 10212`（11:37:37.509 末条）；`wgSessions=0`、`wgRxToTun=0`；vpn-tun RX 0/0（11:34:19 与 11:37:25 两次读数相同，零增长）。
- **臂 2 不符预期 ✗**（EXECUTION-RECORD-F1 §4；末条 hilog 本文直读复核一致）：`framesTx 2→118`、`framesRx 1→17`、`transportBytes 10952`（11:46:38.413 末条）；`wgSessions=0`、`wgRxToTun=0`；vpn-tun RX 0/0（11:43:00 与 11:46:05 相同）→ **按写死规则判「未收敛」**。
- **两臂 relay 计数同量级**（111/17/10212 vs 118/17/10952，均 `state=ready`、`reconnects=0`）→ relay 传输两臂一致，**排除中继差异**。
- **时钟门两次均 proceed**：11:25:52（skew=1/bound=1）、11:39:02（skew=0/bound=1，臂 2 前）（两份 clock-check json，本文实读）。

### 3. 候选解释（按强度，如实登记；EXECUTION-RECORD-F1 §5）

1. **对端缺席（最强混杂）**：0004 成功轮在案的 nbinterop 主机 peer（pid 239360，09-14 启动）本轮实测已不在运行（ps 无进程；其 tee 日志 `/tmp/devval2-peer2.log` 亦随 /tmp 清理消失）。若无任一生产对端在线应答，WG 握手 initiation 无人回应，`wgSessions=0` 与修复无关——0004 成功时是运维对端 `net-host` 应答（见本文件「中继/对端侧证据并入」节）。
2. **身份变更**：本轮设备身份为 `ohos-smoke-1`（见第 4 条），对端 peer 表/分组可能不认识该身份——控制面 offer/answer 往来正常、WG 层无响应；该身份是否在对端 peer 表/正确分组未从设备侧证实（EXECUTION-RECORD-F1 存疑 2）。
3. **中继差异已排除**（见第 2 条末）。

### 4. 派发前提错误（显式登记，责任在主会话；偏差由执行层在 EXECUTION-RECORD-F1 §2 如实登记，本文核对属实）

- 派发稿称主机侧暂存 `~/netbird-interop/smoke-prod/device-config.json` 为 `ohos-relay-1` 配置；执行层实测该文件是 **09-15 18:41 的旧文件**（hostname=`ohos-smoke-1`、无 `relay_enabled` 字段；本文 `ls` 实读 mtime=09-15 18:41 相符）。
- **`ohos-relay-1` 的私钥已不可恢复**：随 0004 的 E 类清理与 /tmp 清理丢失（执行记录：有界检索无 `9db53981…` 备份）。
- 执行层按派发核心意图（两臂共用同一身份以保持一致性、不换 key），以该暂存键为共同身份，`jq` 增补 `relay_enabled:true` 后推送（推送 1/2），两臂全程同一配置文件，并在执行记录登记该偏差。
- ⇒ **本轮的「身份」变量与 0004 轮不同（`ohos-smoke-1` vs `ohos-relay-1`）**，这本身是新增的混杂因素。

### 5. 配额与收尾（EXECUTION-RECORD-F1「配额总账」）

- **F 类**：install 2/3（臂 1 `d39dae3c…`、臂 2 `80c5b7cf…`）；推送 1/2；**循环 2/2 用尽**（臂 1 11:31:03–11:38:44 ≈7min41s、臂 2 11:39:41–11:48:17 ≈8min36s，合计 ≈16min17s ≤30min；各臂观测窗 6min ≤6min）；hilog 每臂 1 次（流式 400s）；点击 2/2。
- **G 类**：清理 1/1；force-stop 1/2；**App 保留**（uninstall 0/1）；本轮推送的 /data/local/tmp 三文件已删、复核零残留。
- `/tmp/n13-arm1` 已删、主工作区未被改动（执行记录 git status 干净；本文追加前复核本文路径无未提交变更）。
- 证据完整性：目录 14 个文件 = 6 份证据产物逐文件附 `.sha256`（产物 + sidecar 各 6）+ `MANIFEST.sha256` + `README.md`（无 sidecar、MANIFEST 自汇总）；本文实跑 `sha256sum --check MANIFEST.sha256` → **13/13 全 OK**、退出码 0。

### 6. 结论与影响（一句话，口径不夸大）

- 本轮**未能**把「`18153fa` 是主因修复」从「机制层成立」升级为「单一变量受控」。
- 0004 的 D 类结论**不受影响**（设备侧 relay 数据面成立：`wgSessions=2`、TUN RX 4624B）——那是与**运维对端 `net-host`** 的会话。
- 机制层证据仍成立（修复前对端 499 次 offer / 0 次 answer；修复后 wgProxy + 0.08s 握手，见本文件「中继/对端侧证据并入」节）。
- **干净重跑的前提**：需要**一个确定在线且认识的应答对端**（例如我们自己控制下的主机侧对端），并保持两臂身份一致；F 类循环已用尽，重跑须新授权。

### 7. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**。

## 可见性阻断与修复：运维 ACL 根因（2026-09-16 追加）

> 本节是**运维通过会话消息交付的报告**（无文件包）的**要点浓缩转写**，非字节级副本；事实照抄。要点转写全文见仓B `netbird-n1bdisc/records/ops-acl-rootcause-20260916.md`。

- **根因（一句话）**：**不是账号、不是网络，而是账号内 ACL**——4 台 ohos peer 修复前**都只属于 `All` 组**（`All` 组 id `d29etkd27eas73a9rqeg`，账号 `d29etkd27eas73a9rqdg`、`domain=netcenter.local`、overlay `100.108.0.0/16`、peer 23 台全属它），该账号**不存在 `All → All` 策略**，两台"只在 `All` 组"的 peer **互相不可见**；账号内两条相关策略（`HostInternalPolicy`、`AdminAccessPolicy`）的**并集恰为 `{100.108.171.38, 100.108.162.237, 100.108.156.116}`**，与客户端观测的 3 台**逐台一致**。
- **事件库证明未删 peer**：`activity=5`（PeerRemovedByUser）共 18 条、最近 `2026-08-09`，无一条涉及任何 ohos peer；4 台 ohos peer 各只有 1 条 `activity=1`（PeerAddedWithSetupKey）；`2026-09-16 16:21:13–16:21:25` 的 6 条 `activity=68`（SetupKeyDeleted）删的是遗留 key 而非 peer（删/撤销 key 不影响已注册设备）→ **`ohos-smoke-1` 从未被删除**。
- **命名订正**：`ohos-ab-host-1` 是 **setup key 名**（`setup_keys.id=dal534l27eas73fo7jig`，one-off/usage_limit=1/used_times=1，创建 2026-09-16 16:21:38，到期 09-23，`revoked=f`）；它注册出的 **peer 名是 `netbird-ohos`**（peer id `dal57ft27eas73fo7ju0`，`dns_label=netbird-ohos-126-29`，`100.108.126.29`）。
- **已落地修复**（管理 REST API；临时铸造 1h PAT 用完即删，终态 `personal_access_tokens`=0，不记令牌值；未重启任何服务、未改 management/signal/relay 配置）：group `ohos-ab`（id `dal7bnl27eas73fl6nb0`，最终 4 台成员）+ policy `ohos-ab-internal`（id `dal7bnl27eas73fl6ncg`，`enabled=t`，一条 rule：`action=accept`、`protocol=all`、`bidirectional=true`、`sources=destinations=[该组]`）。
- **服务端验收**：`GET /api/peers/{id}/accessible-peers` 修复前四台各见 3 台 → 第一步后这两台各见 4 台 → 第二步后**四台各见 6 台**，`REMOVED` 为空；API 写入触发 netmap 推送，**在线 peer 无需重连即见新成员**。影响面：`groups 26→27`、`policies 18→19`、`peers 23→23`。
- **我们的独立验证**：主机侧对端（PID 254451，未重连）状态 `"peers":3` → **`"peers":6`**。
- **对前两轮的归因更正**：先前两轮（0005 F 类、0006 H 类）"未收敛/前置失败"的**真正原因是可见性缺失**（不是代码、不是中继、不是账号/网络）；修复后已具备做受控 A/B 的前提。
- **口径不变**：仅 **N13 级证据**；**不构成 N2-N6 pass**。

## J 类：可见性修复后的 2 臂 A/B —— 判定「对照无效」（2026-09-16 追加）

> 本小节为事后追加，不改写既有正文。来源（全部只读核对，本文未运行任何设备命令）：仓B 证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260916-0007/`（两份 `clock-check-*`、两臂流式全量 hilog、两臂对端日志窗口切片、`EXECUTION-RECORD-J1-20260916T2118.md`、`MANIFEST.sha256`）与生效授权 `AUTH-DIAG-DEVICE-VALIDATION-20260916-0007.json`（下称 AUTH）。下文数字均逐字取自上列文件；无任何凭据写入。

### 1. 目的与设计（AUTH J 类）

- 承 0006 H 类（前置①互认失败而停）与运维 ACL 修复（见上一节），在可见性已修复前提下重做自足 A/B：同一设备身份（`ohos-smoke-1`）、应答对端为主机 peer `netbird-ohos`，对照修复前 `e45f8c9` vs 修复后 HEAD ⊇ `18153fa`，把唯一变量收敛为提交 `18153fa`（signal OFFER/ANSWER 补 `relayServerAddress` 字段 8）。
- 前置①（可见性互认）的判定方式为**双条件**：(a) 设备启动后取自身 overlay IP 与对端 peer 列表互认——设备 `network map applied` 的 peers 计数 ≥6（或明确含对端）；(b) 对端侧停止拒收设备 relay 帧——`carrier-reject|reason=relay-peer-offline` 计数零增长，且出现对设备方向的 carrier 发送。双条件同时成立方可进入两臂。
- **两臂生效中继必须相同**（写死规则，AUTH `operation_classes.J.hard_precondition` 第③条与 `criteria`、`forbidden` 末条）：每臂开始前双端各记录当轮生效中继 URL，**两臂不同 → 对照无效、如实记录并停止，不得收敛于 `18153fa`**。

### 2. 前置①成立（可见性互认首次真正成立，双证据）

- **(a) 设备侧**：臂 1 启动后 `network map applied serial=297 peers=6`（≥6 ✓）；对端 `ice.peers=6` ✓。
- **(b) 对端侧**：设备在线后 `carrier-reject|reason=relay-peer-offline` 计数维持 25200 **零增长**（停止拒收 ✓）；臂 1 窗口内对端 `carrier_tx_packets` 3388→3594（**+206**，出现对设备方向的 carrier 发送）✓。

### 3. 两臂读数（signed sha256 / 设备计数 / 对端计数）

- **臂 1（修复前 `e45f8c9`，install 1/4）**：signed HAP sha256 `2c23ef5412b534d4306189bd406954064400d20f7dd1f4c8d07ba7622cb92492`；生效中继 `rels://home.alfadb.cn:28443`。设备侧：`VPN_CREATE_RESOLVED|accepted=true`、`VPN_RELAY_STATUS|state=ready|framesTx=263|framesRx=94`、`wgReady=true|wgSessions=1|wgRxToTun=0`、TUN RX 0/TX 576B。对端侧：`peers_with_session=0` 全窗、carrier lane attached（多次）、无 `Relay is not supported by remote peer` 行。**与预期不符**（预期 `wgSessions=0`）：可见性修复后环境含多个在线 peer，设备那条会话对象是**其他可见 peer**（非 `netbird-ohos`）。
- **臂 2（修复后 HEAD=`24b5ed1` ⊇ `18153fa`，install 2/4）**：signed HAP sha256 `7b6868146e96ebe753cba7a1c15c6ee3c0b9e52c88ec745f1b386824e3d2a3be`；生效中继 `rels://relay.netcenter.alfadb.cn:443`（对端同窗 `relay_urls` 同步切换，双端臂内一致）。设备侧：`framesTx=359|framesRx=397|transportBytes=53624`、`wgReady=true|wgSessions=2|wgRxToTun=0`、TUN RX 0。对端侧：`peers_with_session=1`（211/250 采样；臂 1 全 0）、`handshakes 1306→1337`、lane attached ✓、无 `wgProxy` 字样行；`carrier-reject` +600（25200→25800，来源未定，见第 7 节存疑⑤）。

### 4. 判定与依据：对照无效（禁止收敛）

- **两臂生效中继不同**：臂 1 `rels://home.alfadb.cn:28443`；臂 2 `rels://relay.netcenter.alfadb.cn:443`——系**管理面在两臂之间切换 relay 指派**。
- 按 J 类写死规则（两臂中继必须相同；两臂不同 → 对照无效、如实记录并停止；`forbidden`「不得跨中继拼接对照结论」在案）：**本轮判定「对照无效」，不得收敛于 `18153fa`**；「唯一变量收敛为提交 `18153fa`」不成立。
- 前置①虽首次真正成立（第 2 节双证据），但同中继前提失败，两臂读数不构成受控对照。

### 5. 方向性附注（非结论）

- 臂 2 观测形态**强于**臂 1：`framesRx` 397 vs 94、`wgSessions` 2 vs 1、对端 `peers_with_session` 1 vs 0——方向与修复预期一致；但归因被**中继切换**污染（另有臂 1 会话对象非本对端），**不作为修复生效的结论**。

### 6. 配额与收尾（EXECUTION-RECORD-J1「配额总账」）

- **J 类**：install 2/4（臂 1 `2c23ef54…`、臂 2 `7b686814…`）；推送 0/2（H 轮保留的凭据仍在且哈希一致，按「不做无意义覆写」未重推）；**臂次 2/2 用尽**（每臂 8min 窗 ≤8min，合计 ≈17.5min ≤40min）；hilog 每臂 1 次（流式 480s）；点击 2/2。
- **K 类**：清理 1/1；force-stop 1/2；**uninstall 0/1（App 保留）**；**主机对端停止 1/1**（PID 254451 已确认消失）。
- `/tmp/n13-arm1b` 已删、主工作区未被改动；证据目录 `MANIFEST.sha256` 汇总且执行记录实跑 `sha256sum --check` 通过（EXECUTION-RECORD-J1）。

### 7. 存疑与未闭合（5 条，逐条如实）

1. **两臂中继切换系管理侧行为**：同中继 A/B 需固定 relay 指派或改判据设计；
2. **可见性修复后环境含多在线 peer**：原「臂 1 必为 0」判据失效，宜改为针对 `netbird-ohos` 的定向会话；
3. **两臂 TUN RX=0**（无 overlay 载荷；对端 `connect` 模式未带 `--probe-dst`）：`wgRxToTun` 判据未获行使；
4. **对端状态 JSON 不暴露 peer 身份**：会话归因未完全闭合；
5. **`carrier-reject` +600 来源未定**。

### 8. 下一步设计建议（仅登记，不执行）

- 固定 relay 指派（管理面锁定两臂同一中继），或改判据设计以容忍中继切换；
- 会话判据改为**针对 `netbird-ohos` 的定向会话**（不再依赖「臂 1 必为 0」）；
- 对端带 `--probe-dst` 以产生 overlay 载荷，行使 `wgRxToTun`/TUN RX 增长判据；
- J 类臂次已 2/2 用尽，重跑须新授权。

### 9. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**。

## L 类臂 A1 的反证与实验设计教训（2026-09-16 追加）

> 本小节为事后追加，不改写既有正文。材料（全部只读核对，本文未运行任何设备命令）：仓B 证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260916-0008/`（`clock-check-20260916T224744.json`、臂 A1 流式全量 hilog `hilog-L-arm1-pre-20260916T2250-full.log`、臂 A1 对端窗口日志 `peer-log-L-arm1-window.log`、`EXECUTION-RECORD-L1-BLOCK-20260916T2305.md`）。**注**：本轮核对时该目录**未见 `MANIFEST.sha256`**（如实登记，见第 9 节存疑 1）。下文数字均逐字取自上列文件；无任何凭据写入。

### 1. 臂 A1 结果（修复前构建 `e45f8c9`，不含 `18153fa`）

- **设备侧**（`hilog-L-arm1-pre-20260916T2250-full.log`，窗 22:50:03–22:57:54）：
  - `connector: netbird-config relay urls=1 token_present=true (uris=rels://home.alfadb.cn:28443)`；
  - `VPN_CREATE_RESOLVED|…|accepted=true`；
  - 末次读数（22:57:54）：`VPN_RELAY_STATUS|…|state=ready|framesTx=438|framesRx=533`、**`wgSessions=1|wgRxToTun=8385`**；
  - vpn-tun RX `0 → 2107B(窗中段) → 8385B/195p(窗末)`（`wgRxToTun` 阶梯 0→2107→4214→6235→8385 逐字见 hilog；≈对端探针 1 包/秒的速率）。
- **对端侧**（peer `netbird-ohos`，`--probe-dst 100.108.144.165`＝本设备 overlay IP）：`write failed=0`、**无任何 `carrier-reject`**（窗口切片状态 JSON `carrier_rejects:0`）、lane 正常（relay `state=ready`）；双端中继一致（`rels://home.alfadb.cn:28443`）。
- → **不含 `18153fa` 的构建在可见性/策略修复后的环境中，即可经中继与我方对端建立 WG 会话并收到载荷**；因此 **"`relayServerAddress` 是 WG over relay 无会话的必要条件"这一假设，在【我方对端】场景下被反证**（会话对象＝`netbird-ohos`：探针 dst＝本设备 IP，载荷只能来自其会话——EXECUTION-RECORD §5 口径）。

### 2. 机理解释（为什么）

- `18153fa` 补的 signal 字段是**官方客户端**的门禁条件（上游 `worker_relay.go:122-126` → `:69`：`RelaySrvAddress != ""` 才 `OpenConn`）；而**我方实现的 relay lane 依据中继的 `PeersOnline` 开启，不读对端的 signal 中继地址** → 我方对端**不含该门禁** → 拿它测该门禁的修复，**仪器里没有待测机制**。

### 3. 设计教训（主会话自认，显式登记）

- 本次受控 A/B 选用"我方自控对端"以消除"对端缺席/在线性"混杂，但该对端**恰好绕过待测机制** → 属**结构性（非环境性）设计错误**；正确仪器只能是**官方对端**（或一个显式实现该门禁的测试替身）。
- 教训一句话：**选对照仪器前，先确认仪器里是否包含待测机制**。

### 4. `18153fa` 适用范围的精确表述（据现有全部证据重述，不夸大）

- 对**官方客户端**：修复是其**愿意向我方 OpenConn/建 lane 的必要条件**——证据＝运维对端侧时间线（修复前对端收到我们 **499 次 offer / 0 次 answer**、无 lane；修复后 `created new wgProxy` + `first wg handshake 0.08s`，见本文件「中继/对端侧证据并入」节）＋ D 轮实证（`wgSessions 0→2`、TUN RX 0→4624B）。
- 对**我方实现的对端**：**非必要条件**（本节臂 A1 反证）。
- 因此**互操作性表述**：该修复解决的是"**官方对端不认我方为 relay 可用**"的问题；对外互操作必需，对内自测路径不必要。**不得**再写成"WG over relay 无会话的主因"。

### 5. 顺带登记的互操作差异（观察，非缺陷判定）

- 我方实现比官方**更宽松**（不门禁对端的中继广告即开 lane）——是否需要与官方对齐（避免为不使用中继的对端白开 lane）**留待后续评估**，本次不改代码。

### 6. 对历史轮次归因的影响（如实、不夸大）

- 0005 F 类"未收敛"与 0006 H 类"前置①失败"的**直接阻断**是可见性/策略缺失（0007 已修复并由运维与主会话双向验证）；但其"修复后臂仍无会话"**是否可归因于 `18153fa`，在本轮之后不再成立**——在我方对端场景该修复非必要；对官方对端场景的归因依据来自**运维对端侧证据**而非本系列同轮 A/B。

### 7. 臂 A2 状态

- 修复后 HAP（signed `de953f70…8b76`）已装（install 2/4），但 **23:00:45 `aa start` 被 `10106102`（屏幕锁定）拒绝** → 按硬约束停止、未重试、无 capture、**不计完成臂次**。
- 现场保留（EXECUTION-RECORD「状态与保留」节）：设备配置/key/脚本保留（App 未运行、pidof 空）；**主机对端 PID 286618 保留在线**（未做 M 类停止）。
  - **PID 口径**：以证据文件实际记录为准——`EXECUTION-RECORD-L1-BLOCK-20260916T2305.md` 记 **286618**；本轮派发稿曾写"286498…"，与证据文件不符，不采用（见第 9 节存疑 2）。
- 配额现状（同记录）：install 2/4、推送 1/2、臂次 1/2 完成（A2 阻断不计完成）、hilog 1 次（臂 A1）、点击 1/4。

### 8. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**。

### 9. 本轮追加的存疑与口径说明（如实登记）

1. **`MANIFEST.sha256` 缺失**：0008 目录内本轮核对（`ls -la` 全量列目录）未见该文件，亦无逐文件 `.sha256` sidecar；目录 README（mtime 09-16 22:43）仍写"占位目录，尚无任何执行产物"，与目录现有产物不符——占位说明未随后续产物更新，汇总清单待补（待确认）。
2. **对端 PID 两处口径不一致**：派发稿"286498…" vs 证据文件 `EXECUTION-RECORD-L1-BLOCK-20260916T2305.md`"286618"；本文一律以证据文件为准（**286618**），"286498"来源不明、不得采用。
3. **`--probe-dst 100.108.144.165` 字样**：对端窗口切片内未见该 flag 原文；`100.108.144.165` 为设备侧 `VPN_CONFIG_APPLIED` 记录的本机 overlay IP，"探针 dst＝本设备 IP"为 EXECUTION-RECORD §5 口径。
4. 目录内另有第二份时钟门采样 `clock-check-20260916T230310.json`（23:03:10，`result=proceed`，`prev_checked_at=22:47:44`），不在本小节引用材料清单内，一并如实登记。

## L 类最终结果：A1/A2 同中继配对 —— 方向相反（2026-09-16 追加）

> 本小节为事后追加，不改写既有正文。来源（全部只读核对，本文未运行任何设备命令）：仓B 证据目录 `~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260916-0008/`（三份 `clock-check-*`、两臂流式全量 hilog `hilog-L-arm1-pre-20260916T2250-full.log` / `hilog-L-arm2-post-20260916T2313-full.log`、两臂对端窗口日志 `peer-log-L-arm1-window.log` / `peer-log-L-arm2-window.log`、`EXECUTION-RECORD-L1-20260916T2327.md`、`EXECUTION-RECORD-L1-BLOCK-20260916T2305.md`、`MANIFEST.sha256`）与生效授权 `AUTH-DIAG-DEVICE-VALIDATION-20260916-0008.json`。下文数字均逐字取自上列文件；无任何凭据写入（敏感扫描 `eyJ` 零命中，仅系统 `token=`/`Bearer` 词形）。注：上一节登记的「`MANIFEST.sha256` 缺失」系彼时（A1 阻断留档期）的目录状态；本轮核对时目录已补齐 `MANIFEST.sha256` 与 `EXECUTION-RECORD-L1-20260916T2327.md`——18 个产物文件全附 `.sha256` sidecar，本文实跑 `sha256sum -c MANIFEST.sha256` 全 OK（19 项含 README）。

### 1. 同中继配对成立（本轮管理面未轮换）

- 两臂（A1/A2）**双端生效中继均为 `rels://home.alfadb.cn:28443`**，臂内一致且**跨臂未变——本轮管理面未轮换**（与 0007 J 类两臂被切换的情形不同）。
- 同一设备身份（`ohos-smoke-1`/同私钥）、同一对端（主机 peer `netbird-ohos`，PID 286618，connect 模式）、同一探针配置（`--probe-dst 100.108.144.165 --probe-interval 1000`）。
- **唯一差异＝构建**（修复前 `e45f8c9` vs 修复后 HEAD=`aa6a9c5` ⊇ `18153fa`）。

### 2. 逐臂表（signed sha256 / 设备计数 / 对端计数与拒收增量）

| 项 | A1（修复前 `e45f8c9`，install 1/4） | A2（修复后 HEAD=`aa6a9c5` ⊇ `18153fa`，install 2/4） |
|---|---|---|
| signed HAP sha256 | `d1195ca67a5ccc15a38af625efef4d68e1a00ec5bb3565b11243dbd01c3373fe` | `de953f70bbd2ab29381151271cf557619e43e698efa060b655c1490797268b76` |
| 双端生效中继 | `rels://home.alfadb.cn:28443` | 同 A1（相同 ✓） |
| 执行时刻 | 22:49:40 启动 → 22:50:02 点击（1/4）→ 8 分钟窗 | **23:00 首启遇 `10106102`（锁屏）停止上报；解锁后 23:13 重跑**（点击 2/4）→ 8 分钟窗（23:13:5x–23:21:4x） |
| 设备侧 create | `VPN_CREATE_RESOLVED\|…\|accepted=true` | `VPN_CREATE_RESOLVED\|…\|accepted=true` |
| 设备侧 relay | `VPN_RELAY_STATUS\|…\|state=ready\|framesTx=438\|framesRx=533` | `state=ready\|framesTx=223\|framesRx=21` |
| 设备侧 WG | **`wgSessions=1\|wgRxToTun=8385`** | **`wgReady=false\|wgSessions=0\|wgRxToTun=0`** |
| 设备侧 TUN RX | 0→2107B（窗中段）→ **8385B/195p**（窗末）；overlay `100.108.144.165` | **0/0**（TX 520B/5p，零增长） |
| 对端侧拒收 | **无 `carrier-reject`**（臂 A1 前基线 0 行） | **`carrier-reject\|reason=relay-peer-offline` count 100→800（+700，与本臂窗口重合）**；臂 A2 前基线 100 来源未定（见第 8 节存疑③） |
| 对端侧探针/lane | `write failed=0`（探针全部写成功）；lane 正常；无 `Relay is not supported` 行 | 探针写成功但握手攻势无响应：`handshake-campaign-deadline` ×7；**无建 lane 通向我方的证据** |

### 3. 无效判据检查：两臂均有效

两臂对端侧 `ice.connected=0`、无 `carrier->direct` → 未触发无效条款，**两臂均有效**（均可作对照证据）。

### 4. 配对结论（如实，禁止拼凑）：我方对端场景的互操作回归信号

- 同中继 + 同身份 + 同对端 + 探针同配置，唯一差异＝构建；观测到**方向完全相反**：修复前被对端接受（建会话、收探针载荷）、修复后被对端拒收（无会话、零收包）→ **这是【我方对端】场景的互操作回归信号**（EXECUTION-RECORD-L1 §6 判读：「修复后构建的帧被对端以 relay-peer-offline 拒收……与 A1 完全相反」）。
- **定性资格限制**：该场景**不具备对 `18153fa` 主因定性资格**——A1 已反证「修复前必然无会话」前提（见上一节），故本配对**不能**得出「`18153fa` 是/不是 0005/0006 无会话主因」的结论；且 **D 轮（0004，含同一修复 `18153fa`）曾对官方对端建会话成功** → 「含字段即被拒」**不成立**，拒收条件更细（对端实现/模式/状态相关）；回归机制**未从代码/协议侧验证**。

### 5. 主会话独立假设（待验证候选，非结论）

- 两臂之间可能还有第三个变量：**设备在 A2 前掉线过一次并重连**（A1 时设备是新上线、对端全新订阅）。
- 怀疑**我方对端的「对端在线状态」未随重连刷新**（订阅只做一次 / `PeersWentOffline` 后不重订阅 / 重新上线无新 `PeersOnline`），从而出现「A1 收、A2 拒」。
- **该假设正在另行只读分析（另案）中，本节只登记为待验证候选，不得写成结论。**

### 6. 配额与收尾（EXECUTION-RECORD-L1「配额总账」）

- **L 类**：install 2/4（`d1195ca6…73fe` / `de953f70…8b76`）；推送 1/2；**臂次 2/4**（A1 8min 窗 + A2 8min 窗；A2 首启锁屏不计完成、重跑完成；合计 ≈35min ≤60min）；hilog 每臂 1 次（流式 480s）；点击 2/4。
- **M 类**：清理 1/1；force-stop 1/2；**对端停止 1/1（`kill 286618` 已确认消失）**；uninstall 0/1（App 保留）。
- 时钟门 3 次（22:47:44 / 23:03:10 / 23:12:50）**均 proceed**。

### 7. M 清理实况

设备侧本轮重推的三文件（config/key/start-netbird.sh）逐个删除 → **零残留**；`pidof` 空；**App 保留**；`/tmp/n13-arm-pp` 已删；主工作区未动。

### 8. 存疑（4 条，逐条如实）

1. **回归机制未定因**：`18153fa` 添加的 relayServerAddress 字段与对端拒收（relay-peer-offline）之间的机制联系未从代码/协议侧验证；
2. **`relay-peer-offline` 语义未从对端代码复核**（本轮只读约束）；
3. **A2 前拒收基线 100 来源未定**（臂 A1 结束后出现）；
4. **0004 成功与本轮回归的边界条件（对端模式差异）未归因**。

### 9. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**。

---

## A2 拒收的根因闭环：presence 刷新缺陷已修复（2026-09-16/17 追加）

> 本小节为事后追加，不改写既有正文。材料（全部只读核对，本文未运行任何设备命令）：`docs/relay-presence-refresh-analysis-20260916.md`（只读根因分析，全部结论带 file:line；下引上游源码路径均相对该文锚定的 `refs/netbird-791401060d2b/` 只读副本）、`client/core/tests/relay_presence_refresh.rs`（5 用例，含核心复现）、`client/core/src/relay_testserver.rs`（忠实 presence 语义 + 控制面）与 commit `0cfb723`（`git show --stat` 只读核对）。下文数字与原文均逐字取自上列材料；无任何凭据写入。

### 1. A2 的「回归信号」重新定性

上一节（L 类最终结果）把 A2 登记为「我方对端场景的互操作回归信号」，并把「设备在 A2 前掉线过一次并重连」列为待验证候选（其第 4/5 条）。**本节对该信号重新定性：它不是 `18153fa` 的回归，而是我方客户端的一个独立真缺陷**——对端掉线→重连后 presence 缓存永为 `false`、lane 永不恢复，该 peer 的 relay 承载帧在**本 relay 会话存活期内被永久拒收**（`relay-peer-offline`）。两臂之间的第三个变量＝**设备在 A2 前掉线过一次并重连**（与「构建差异」混杂），故 A2 的观测差异**不可归因于构建**。

### 2. 根因（file:line，照抄报告）

- presence 缓存仅由 `PeersOnline`→true / `PeersWentOffline`（PWO）→false 驱动、仅会话结束清空（置 true `relay_client.rs:2028-2038`、置 false `:2040-2053`、整表清空仅 `:2144`；缓存本体 `:1259-1261`）。
- `SubscribePeerState` 每 lane 只发一次：生产代码唯一构造点在 `open_conn` 内（`relay_client.rs:2089`）；`ensure_lane` 对已挂载 lane 第一行短路（`connector.rs:2035-2038`）。
- 我方 PWO 处理**不退订、不重开**（`relay_client.rs:2040-2053` 无任何后续动作）。
- 而上游服务端 **`PeersOnline` 兴趣一次性投递**（投递即删，`relay/server/store/listener.go:107-121`，删除在 ：120）、**PWO 兴趣常驻**（`listener.go:92-105`）→ 对端回来后收不到第二次 `PeersOnline`，presence 停在 `false` → 出向帧被本地拒收（判定链：`wg_device.rs:1046-1086` dispatch → `send_to_peer` 的 `is_offline` 检查 `relay_client.rs:1442-1444` → class token `relay-peer-offline`，`relay_client.rs:1604-1605`）。
- 上游客户端靠「PWO→退订+关 per-peer 连接→按需重新 OpenConn」自愈（`shared/relay/client/client.go:605-607` 分发、`:784-803` 退订），**我方移植丢了这半圈**。

### 3. 复现（先红）

`client/core/tests/relay_presence_refresh.rs` 5 用例（核心复现 `peer_returning_online_must_restore_the_relay_lane`；另含无 bounce 对照、忠实语义线上锁死、`set_peer_online` 控制面、legacy 档钉死）。修复前核心用例 RED，原文：

> DEFECT: the remote re-authenticated on the same relay server, but the lane never recovered — egress still refused with class token "relay-peer-offline" (server saw 1 SubscribePeerState frames, client received 1 PeersOnline frames; correct behavior is a fresh subscribe + PeersOnline + accepted egress)

（修复前全量 32 套件 475 passed / 1 failed，唯一失败即该用例；既有 12 个 relay e2e 与 471 项零回归。）

### 4. 修复（commit `0cfb723`）

`RelayCarrier::ensure_lane` 每次调度做 presence reconcile——attached lane 的 peer 已被 PWO 置 false 即先 detach、随即全新 `open_conn`（重新订阅，重臂服务端一次性在线兴趣）；抑制点＝每 peer `LANE_OPEN_RETRY=5s` 退避（仅 open 失败后生效，与既有 30s OpenConn 超时叠加，长离线 churn ≈2 帧/35s）；PWO 后首试不受抑制；配套公开 `RelayClient::is_offline()`。改动仅 2 个生产文件（`connector.rs` +67、`relay_client.rs` +16）。未做 PWO 补发 UnsubscribePeerState（reconcile ≤500ms 内同连接重订阅即覆盖兴趣表）。

### 5. 离线验证（实跑）

复现套件 **5/5 ok**（主会话已独立复跑确认核心用例 ok）；全量 **32 套件 476 passed / 0 failed**；复现套件与全量各连跑 3 次结果逐字一致；`bash flake-check.sh 30` → **0/30 失败轮**；`bash build.sh` exit 0（aarch64 .so，14/14 冻结符号 OK）。

### 6. 方法论教训（两条）

1. **测试替身与原件语义不一致本身是一类风险**——既有假服务器与真实服务端恰在 presence 维度不一致（假服务器结构上发不出 PWO），这正是该缺陷长期未被离线测试暴露的原因；已把假服务器改为**上游忠实**（`faithful_presence` 默认 true，legacy 档保留并由专门测试钉死）。
2. **选对照仪器前先确认仪器里是否包含待测机制**（前一轮 A1 的教训，与此处呼应）。

### 7. 对既有结论的影响

D 轮（设备侧 WG over relay 数据面成立）与 `18153fa` 适用范围的表述**均不受影响**；A2 的拒收**不再**构成「互操作回归信号」（该措辞已被本节更正为「我方独立缺陷」）。

### 8. 待办（未验证，必须显式）

**真机复验未做**——需在设备侧复跑 A2 场景（对端掉线→重连后，我方应能重新接受其帧并建立会话）；自建中继的服务端 presence 语义**偏离未核实**（偏离时恢复时序可能不同，但 fresh-subscribe 恢复路径对任意语义均正确）；多 peer 交错离线组合压测未覆盖。真机复验需**新授权**（本系列 AUTH 均已收尾）。

### 9. 口径不变

仅 **N13 级证据**；**不构成 N2-H pass**；**不构成 N6 pass**。
