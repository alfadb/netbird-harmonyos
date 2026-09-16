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
