# 第二轮华为回复验证：用户决定、逐条文档核对、验证计划与已备制品（2026-09-25）

## 0. 说明

- 本文只记录**决定、核对与计划**：不修订 v4 预登记正文，也不修订仓B 任何登记件，不构成任何门结论；N2-H 正式状态不变（`n2h-inconclusive`）。
- v4 预登记正文（只读字节锚，本文未触碰）：`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md`，
  `sha256 a7510430f3ab402c3cd11f1ef39889b6092b8cf3ec8a4b14d2e90c25bc0c0dc7`（2026-09-25 `sha256sum` 实跑复核，与既有记录逐字一致）。
- 两轮回复原文与对照见前作 `docs/n2h-huawei-support-replies-20260923.md`（本文不重复其原文与行号锚，只在其上追加：新决定、文档核对、验证计划、已备制品）。
- 仓B 侧针对同一批决定的合规修订另见 `/home/worker/harmonyos-signing/netbird-n1bdisc/records/n2h-preregistration-v4-addendum-20260923.json`
  的 `amendment_events[1]`（只追加，不改原文；本文只引用，不重复其全文）。
- **本文不执行任何设备操作**；§3 的三份草案均为**未签发草案**，确认与签发前不得运行。

## 1. 用户决定（逐字）

| 时间 | 用户逐字原话 | 据此生效的决定 |
| --- | --- | --- |
| 2026-09-23 | 「我觉得要把华为工单回复的内容进行验证后再考虑发下一轮工单提问」 | **第三轮工单暂缓**；先对第二轮回复内容做验证，验证结果出来后再决定是否发问 |
| 2026-09-23（随后） | 「都同意你的意见」 | 批准三项：**(i)** 做带默认关闭诊断开关的实验包，以真机验证第二轮 3.1 的三条说法；**(ii)** 4.2 的原命令验证放进 0030；**(iii)** 0029 范围不变 |
| 更早（同日） | 「都做」 | 批准：驱动改为**探针失败不中止**（v11）；以及 protect 复现授权草案 |

**作废项**：此前记录的决定①「第三轮工单现在发」（原文见 `docs/n2h-huawei-support-replies-20260923.md` §4 第 1 条）据此**作废**；该轮工单改为待验证结果后再决定（草稿保留，见 §7）。
同时不动的两项（仍按原口径）：决定②「不提包计数需求」、决定④「隧道网段路由放到证据轮之后」不变；决定③「0029 第三项原样保留」由本次「(iii) 0029 范围不变」再次确认。

## 2. 第二轮回复逐条文档核对

**来源**（本节结论只在这些来源范围内成立）：

- **OpenHarmony 文档仓 master**（2026-09-25 实读）：`zh-cn/application-dev/dfx/hilog.md`、`zh-cn/application-dev/dfx/hdc.md`（后者已由 `docs/n2h-huawei-support-replies-20260923.md` §2 于 2026-09-23 实读登记）、`zh-cn/application-dev/reference/apis-network-kit/js-apis-net-vpnExtension.md`、`zh-cn/application-dev/application-models/application-context-stage.md`、`zh-cn/application-dev/file-management/app-sandbox-directory.md`。
- **本机 SDK 26.0.0.821 d.ts**：`…/sdk/default/openharmony/ets/api/@ohos.net.vpnExtension.d.ts`、`…/@ohos.net.statistics.d.ts`。
- **商用版《访问设备文件》**（第三方镜像）：商用文档的 `hdc file send/recv -b` 与沙箱目录说明；本文只按其转述的形态比对，**未逐字复核商用版原文**。
- **OpenHarmony 开源实现**：netmanager 基础组件的 `fwmark.h`（位布局定义，见 3.2 行）。

> **重要限定**：**OpenHarmony 文档不等于商用版**（HarmonyOS/商用版可能另有差异与未公开约束）。因此下表凡标「文档支持」的都只是**文档层面**成立；任何需要平台实际行为的判断一律归入「仅真机能确认」。

| 条目 | 判定 | 仅真机能确认的部分 |
| --- | --- | --- |
| **1 `/proc/net/dev`** | **文档未提及**（既无承诺也无禁止）。OpenHarmony 文档与 SDK d.ts 均未见对应用可读 `/proc/net/dev` 的承诺或禁止条款；与厂商第二轮「无公开接口或文档承诺应用可读」的措辞一致 | 应用进程内直读是否恒被拒（项目既有记录为 `os error 13`）；是否存在文档未覆盖的版本差异 |
| **2 无接口级包计数** | **文档支持**。①接口级**字节**计数存在：SDK `@ohos.net.statistics.d.ts:62/:79` `getIfaceRxBytes`、`:96/:113` `getIfaceTxBytes`；②`getSelfTrafficStats` 维度为**应用×网络类型、仅蜂窝/Wi-Fi**：同文件 `:523` `getSelfTrafficStats(networkInfo: NetworkInfo): Promise<NetStatsInfo>`，返回 `NetStatsInfo`（`:430`），入参为 `NetworkInfo`（蜂窝/Wi-Fi），故与 `vpn-tun` 无交集 | 真机上确实不存在 per-iface 包计数来源（缺口 `packets-identity=unavailable-no-public-per-iface-packet-api` 能否被真机复核为永久） |
| **3.1(a) 只看 `routes`+应用名单** | **文档支持**：`VpnConfig.routes`（SDK `:299-304`）与 `trustedApplications`（`:376`）／`blockedApplications`（`:388`）即判定输入。**「不存在自身默认排除」文档未提及**；但**官方 `create()` 示例把自身包名放进 `blockedApplications`**（侧面支持）——OpenHarmony 文档 `js-apis-net-vpnExtension.md` 的 `create` 示例中 `private blockedAppName: string = 'com.example.myvpndemo'`（即示例自身包名），且 `this.blockedApplications = [blockedAppName]`、`this.trustedApplications = []`。**「最长前缀/优先级规则」文档未提及**（`VpnConfig.routes` 未定义匹配优先级算法） | 真机上 `routes` 的实际匹配结果与优先级行为；「VPN 应用自身不被默认排除」在真机的实测表现（0021 已给出现象，机制待 0031 判） |
| **3.1(b) 步骤「建隧道→protect→create」与 protect 语义** | **文档支持**：`protect` 文档语义为「保护套接字不受 VPN 连接影响，通过该套接字发送的数据将直接基于物理网络收发，因此其流量不会通过 VPN 转发」；文档 `create`／`protect` 示例形态即「建隧道 → protect → 使用」。**「`isBlocking=true` 不 protect 会自环」文档未提及**：SDK `:357-364` 对 `isBlocking` **只有一句**「是否阻塞模式。`true` 表示阻塞模式，`false` 表示非阻塞模式，默认值为 `false`」（英文 d.ts `:358-359` 同一句），全文无自环／必须 protect 的约束句 | `protect` 的 Promise 在真机的结局（项目既证「不返回」，见 §6(ii)）；自环是否真的会发生（文档未定义，只能真机复现） |
| **3.1(c) `protectProcessNet`** | **接口能力文档支持（API 22 起）**：SDK `@ohos.net.vpnExtension.d.ts:268-275` `protectProcessNet(): Promise<void>`，`@since 22`；文档 `protectProcessNet22+` 说明「保护应用进程不受 VPN 连接影响，被保护的进程直接基于物理网络收发数据，流量不通过 VPN 转发」。**「管理／自诊断 socket 该用它」文档未提及**（文档只定义它是进程级保护，未规定用途分工）。项目侧：包装函数 `protectProcessNetWithBox` 在 HEAD `e251f17` **只有一个调用点**（S3 诊断开关，默认关），正常壳路径走 per-fd `protect()`，见 §6(iii) | 该接口在真机的结局（resolved/rejected/throw/timeout/卡死）——0031 的 R4 专测此项 |
| **3.1(d) `blockedApplications` 自身包名** | **文档支持**（整应用排除、与 `trustedApplications` 互斥）：SDK `:371`、`:383` 两处 `**Note**: Configure either **trustedApplications** or **blockedApplications** as they are mutually exclusive.`；文档 VpnConfig 表同记「被阻止的应用信息列表…该列表中的应用数据不会被 VPN 代理，其他应用可以根据 `routes` 配置被 VPN 代理」＋「注意：`trustedApplications` 和 `blockedApplications` 列表不能同时配置」。项目侧代码未使用该字段（S2 开关默认关，`client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:384-427`） | 整应用排除在真机的实际效果，以及「该走隧道的流量一并被排除」这一代价的实际范围——0031 的 R2 专测此项 |
| **3.2 `SO_MARK`／fwmark 位布局** | **公开文档／SDK 无定义**：SDK 与公开文档只承诺 `bindSocket` 语义（「将套接字绑定到特定网络」），**无** `SO_MARK` 位布局、也**无** mark↔netId 换算关系。**OpenHarmony 开源实现 `fwmark.h` 定义 netId 占低 16 位**（实现侧定义，非公开文档承诺；开源实现亦不等于商用版实现）。与 0022 观测**一致**：v4 正文 `docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:24` 记 `requested_net_id=148`、同 socket 发送前后 mark `0x00000000`→`0x00030094`；低 16 位 `0x0094`＝148，正是请求绑定的 netId 148。**仅作一致性旁证**——按既有解读纪律（`docs/n2h-posctl-socket-context-20260920.md:34`、`docs/n2h-network-context-probe-20260920.md:48`），**不据 mark 值下结论** | mark 在真机的实际取值与位含义；绑定后选路是否确实按该 netId 生效（选路结论由 0022/0031 的行为观测承担，不由 mark 数值承担） |
| **3.3 `bindSocket` 走隧道** | **文档支持**：`NetHandle.bindSocket`／`OH_NetConn_BindSocket(fd, netHandle)` 语义为「将套接字绑定到特定网络」（起始 API 12）。**0022 已真机证实**：`N2H_POSCTL_BIND_NET\|…\|requested_net_id=148\|result=bound\|errno_or_code=0`，同 socket 有 TUN from-os 观测与主机逐 `probe_id` 回执（v4 正文 `:24`） | **已由 0022 完成**，无需新增真机项；0031 的 R3 只在此基础上问「被 `blockedApplications` 排除后绑定 socket 还能否进隧道」 |
| **4.1 `/data/local/tmp`** | **名称与用途有文档**：文档记 `/data/local/tmp` 为「一般暂存区目录」，供 DevEco 调试/测试生成文件使用。**「应用不可见」文档未直接表述**（未见「应用不可见该目录」的直述句）。**0027 已真机证实**（否定结论：应用不可见该固定路径，v4 正文 `:28`、`:75`） | **已由 0027 完成**，无需新增真机项 |
| **4.2 `hdc file send/recv -b`** | **写法、前提、错误码有文档，但示例路径与真实 `filesDir` 不符**。①写法与前提：`-b` 对应的已安装应用须「使用调试证书签名，并且已在设备上启动」，API 15 起支持；错误码 `E003001`／`E005004`／`E005101`／`E005102`／`E003004` 均有文档（逐条已登记于 `docs/n2h-huawei-support-replies-20260923.md` §2 的 4.2 行）。②**路径不一致（本项是文档核对的关键发现）**：华为示例 DEST `./data/storage/el2/base` 是**沙箱根**（`el2/base`），而应用 `this.context.filesDir` 实际为 `<路径前缀>/<加密等级>/base/haps/<module>/files`；本应用 module 名 `entry` ⇒ `/data/storage/el2/base/haps/entry/files`（`application-context-stage.md` **表1**「不同级别 Context 获取的应用文件路径说明」：`filesDir` 行在 UIAbilityContext／ExtensionContext 下取 `<路径前缀>/<加密等级>/base/**haps/\**/files`。另据 `app-sandbox-directory.md` 路径对应表，`/data/storage/el2/base` 的物理路径为 `/data/app/el2/<USERID>/base/<PACKAGENAME>`）。⇒ **照华为原路径投递，应用读不到** | `-b` 在本设备/本 SDK 上是否可用（含 `E003004` 之类不支持情形），以及**两种投递路径各自的可读性**。0030 已按「原路径」与「应用实际 `filesDir`」两种路径分别实测 |
| **hilog 抓取步骤** | **官方文档说默认不落盘，华为步骤缺一步**。`hilog.md`（2026-09-25 实读）：「hilog 日志默认不落盘，需要手动开启落盘任务，才会落盘到设备 `data/log/hilog` 目录下」；落盘任务由 `-w start/stop/query/clear` 控制，其中 `clear`＝「删除已经落盘的日志文件」，且文档注明「当前没有落盘任务时，才能开启落盘任务」。⇒ 华为给的三步（`hilog -w clear` → 复现 → `hdc file recv /data/log/hilog`）**缺 `hilog -w start`**：若本就没有落盘任务，`-w clear` 之后不会有新的落盘文件产生，`file recv` 取到的只能是旧文件或空目录。另两点同样来自该文档：①**隐私遮蔽**（`-p on` 打开隐私开关，显示 `<private>`）；②**流控／超限可能丢行**（进程/domain 超限与全局丢行均会打点 `LOGLIMIT`／`Slow reader missed`／`write socket failed`，文档明示「日志已经丢失，无法恢复找回」） | 真机上落盘任务当前是否存在（`hilog -w query`）；`-w start` 是否需要、是否授权；以及实测抓取是否发生丢行。0030 已把 `-w clear/start` 列为**默认不授权**（`-w` 属持久化配置，`clear` 会永久删除手机上已落盘日志），本轮只做**只读核查** |

## 3. 验证计划与草案

三份草案均在仓B `diagnostics/`，**均为未签发草案**：`is_draft:true`、`authorized:false`、`approval.status:awaiting`、`recorded_at/valid_from/valid_until=null`、**不附 `.sha256` sidecar、不注册**（2026-09-25 逐件解析实读确认）。草案不代表授权；确认前不得执行任何设备操作。

| 草案 | 用途 | 关键内容 | 备注 |
| --- | --- | --- | --- |
| `diagnostics/AUTH-DRAFT-DEVICE-VALIDATION-20260921-0029.json` | **0029 修订草案**：v4 实现面测量（驱动 v11） | 相对原件改：`script_path`→持久路径、`authorized_script_sha256`→v11 实测值、`signed_hap`／`host_tool`→持久路径与驱动实际运行路径、`supersedes`、`self_test_evidence`、`signing_procedure`、`setup_operations` 新增第 6 条（上机前核对 `host_tool` 哈希不符即停）；其余字段逐字保留 | 原件 `diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260921-0029.json` **未改一个字节**，`sha256 d6a876ccc1e8be985ec02f3e878d3726c8437357793bb6b19ef55a302243057d`（同名 sidecar 同值）；其字节副本含 sidecar 冻结于工具包 `round-0029/prior-draft/` |
| `diagnostics/AUTH-DRAFT-DEVICE-VALIDATION-20260925-0030.json` | **0030（不改代码的验证）** | ①**protect 复现**：关闭偏离开关、抓**不过滤**的完整日志、用 `bm dump` 的 `accessTokenId` 与日志 `callerToken` 比对归属（并写明「同时出现 ≠ 因果」）；②**4.2 两种投递**：华为原路径与应用实际 `filesDir` 分别实测，并确认应用**能读到**；③**hilog 方法只读核查**（`hilog -w query`／`ls -la /data/log/hilog`／`hdc file recv`，`-w clear/start` 默认不授权）；④**型号版本复读** | 前置硬门：0029 已执行、`consumed=true` 且清理完成；本轮不安装/不卸载/不清数据/不重启；不需要主机侧对端 |
| `diagnostics/AUTH-DRAFT-DEVICE-VALIDATION-20260925-0031.json` | **0031（实验开关）**：R0–R4 固定矩阵 | 用带四个**默认关闭**诊断开关（S1–S4）的实验包真机验证 3.1 的三条说法并回答第 4 问。矩阵：**R0** 对照（仅探针不绑定）→ **R1**（+隧道网段路由，验 3.1(a)）→ **R2**（+`blockedApplications` 自身，验 3.1(d)）→ **R3**（S1+S2 且探针绑定 VPN 网络，答第 4 问）→ **R4**（+`protectProcessNet`，验 3.1(c)，**最后跑**，可能卡死扩展线程）。结束**装回 v4final** | **本文不锚定其 sha256**：撰写期间（2026-09-25 23:44–23:54）该 `.json` 与同名 `.md` 仍被**并行修订**，字节数与哈希持续变化（如 `44146 B`→`49246 B`→`48912 B`→…，哈希随之改变），属活跃改写的中间态，任何写入值随即过期；待其稳定后由签发批自行登记 |

- **顺序**：**0029 → 0030 → 0031**。各有前置硬门：0029 清理完成后才可 0030；0029 与 0030 均 `consumed=true` 且清理完成后才可 0031。
- **签发前须由用户确认手机当前无线调试地址**：三份草案内的 `target` 均为**最后已知地址** `192.168.50.199:37193`；若已变化，签发时须逐字改写 `target` 与 `allowed` 第 0 条 tconn 字面。
- 三份草案的产物均为 `is_evidence:false`／`not_a_gate_pass:true`，**不构成 N2-H 或任何门结论**。

## 4. 制品与完整性（持久目录 `/home/worker/n2h-kit/`）

上机制品已集中到持久目录，各子目录自带 `SHA256SUMS.txt`；**2026-09-25 实跑 `sha256sum -c` 全部通过**（退出码 0）：

- `cd /home/worker/n2h-kit/round-0029 && sha256sum -c SHA256SUMS.txt` ⇒ **11 项全部 `OK`**（`driver/` 4 项、`hap/` 5 项、`prior-draft/` 2 项）。
- `cd /home/worker/n2h-kit/exp-e251f17 && sha256sum -c SHA256SUMS.txt` ⇒ **8 项全部 `OK`**。
- 注：`/home/worker/n2h-kit/` 根目录本身**没有** `SHA256SUMS.txt`；校验以两个子目录各自的清单为准。

| 制品 | 路径 | sha256 |
| --- | --- | --- |
| v11 驱动（61/61 自检通过） | `/home/worker/n2h-kit/round-0029/driver/validate_private_arm_v11.py` | `5621128c982e99449856cfd62c4ad5e764690118116564583f2d8f0590a22655` |
| v4final 签名包 | `/home/worker/n2h-kit/round-0029/hap/signed.hap` | `02bdbf93173e0038524385aa39ed59489ff60e49114f0367e3e2cd465b549532` |
| nbinterop 冻结副本 | `/home/worker/n2h-kit/round-0029/hap/nbinterop` | `eca5671811cb6b50005c63d7976250ba6eef4e09b0ee7a9480629dca651e840b` |
| 实验包（未签名） | `/home/worker/n2h-kit/exp-e251f17/entry-default-unsigned.hap` | `5c33ad39a7b15e5d4a844ec4ccb9404417a5467423302790a41e32f150a4743f` |
| 实验包（签名） | `/home/worker/n2h-kit/exp-e251f17/netbird-exp-e251f17-signed.hap` | `ed3f48fcfd8d079cd56b669e49bc59b905af97ca45ff23cc998475ad016b4461` |
| core 构建标识 | 实验包与 v4final **相同** | `83bbe9227d48d07e733246de4a764dbeb3def4948e74dd9b09435636c0602ed6` |

- 实验包对应提交 `e251f17181fccc9e16a3b15aaa504fa29fdc97e9`（起草时项目仓 HEAD 即该提交，工作区干净）；冻结记录见 `exp-e251f17/HAP-FREEZE.txt` 与 `sign-result.txt`（`signApp errCode=SUCCESS_CODE`）。
- v11 驱动自检：`round-0029/driver/selftest-v11-20260923.txt` ⇒ `SELFTEST RESULT: ALL PASS (61/61 PASS)`。
- `v11` 驱动实际运行的主机工具是**代码常量**路径（`client/core/target/debug/nbinterop`），授权字段 `host_tool` 只是登记性字段、驱动不读；因此 0029 草案把「`host_tool` 哈希必须等于 `eca56718…840b`，不符即停」列为上机前人工核对项。

## 5. 事件：主机重启与 `/tmp` 清空（2026-09-23）

- 2026-09-23 约 **20:14** 主机重启，**`/tmp` 被清空**：原先放在 `/tmp` 的驱动目录、v4final 制品包、`signclasses` 等随之消失。
- **未丢数据**：因当日 **19:10** 已做备份——`/home/worker/backups/n2h-device-round-0029-20260923/`（含 `n2h-direct/` 与 `netbird-v4final-fd5f71c/` 及 `SHA256SUMS.txt`；2026-09-25 实读存在）。
- **据此的两条长期措施**：①此后**上机制品一律改放持久目录 `/home/worker/n2h-kit/`**（§4）；②**驱动运行时显式传 `--state-dir` 到持久目录**——v11 驱动的 `STATE_DIR` 默认值为 `/tmp/n2h-direct/state`，重启即清空，跨重启留存必须显式指定。
- 该事件也是 0029 修订草案本身产生的原因（登记持久路径 + 上机前哈希核对）。

## 6. 更正（2026-09-25）

本节列出 2026-09-25 核对后对既有口径的更正；除本节所列外，其余判断与 v4 正文不变。**本节只作记录，未修改任何被更正的文件。**

1. **旧说法「应用自身 socket 默认不进自身 VPN」不成立**。按厂商第二轮 3.1(a) 的规则，判定只看 `VpnConfig.routes` 与应用名单；0021 的未绑定探针之所以走 `wlan0`，是因为**该会话 VPN 的路由里没有隧道网段**（`VpnConfig.routes` 仅 `192.168.50.0/24` + 排除 `/32`，不含 `100.108.0.0/16`），而**不是**因为「它是 VPN 应用自身」。正确表述是「目的地址不匹配 `routes` 才不进」。策略路由表仍未直接观测，该边界保留。
2. **「protect 被 `MANAGE_VPN` 拦截」的因果未证实**。已证实的只有「`protect` 的 Promise 不返回」；`MANAGE_VPN` 归因与第三轮追问的对应关系见 `docs/n2h-huawei-support-replies-20260923.md` §5（含 09-16 未调用 protect 仍报权限失败行的原始日志证据）。
3. **`client/entry/src/main/module.json5:62-71` 的注释与代码不符**（仅记录，**未改**）。该注释称：「The shell therefore uses the public process-level API (protectProcessNet, API 22+, no permission tag) instead — see `NetBirdVpnExtensionAbility.protectSocketWithBox`」。实测（HEAD `e251f17`）：
   - 它引用的 `protectSocketWithBox`（`NetBirdVpnExtensionAbility.ets:2713`）**是 per-fd `connection.protect(fd)` 的包装**（`:2735`），**不是**进程级 API——注释把两个 helper 混为一谈；
   - 正常壳路径确实走 per-fd `protect()`（调用点 `:703`／`:983`／`:2166`／`:2519`／`:2681`）；
   - `protectProcessNetWithBox`（`:2774`）在 HEAD `e251f17` **只有一个调用点**，即**默认关闭**的 S3 诊断开关（`:2310`，其代码注释亦自述「Since 2026-09-23 this wrapper has exactly ONE call site: the S3 diagnostic switch」）。
   ⇒ 「壳用 `protectProcessNet`」的说法与代码不符；此项只登记，不改注释、不改代码。

## 7. 第三轮工单

- 草稿 `/home/worker/huawei-support/20260923-round3-protect-manage-vpn.md`（2026-09-25 实读存在，13680 B）**暂缓发送**；待 **0030／0031 的结果**出来后再修订，届时再决定是否发问（对应 §1 的用户决定）。
- 环境值已知：设备 **PLA-AL10**，系统 **PLA-AL10 7.0.0.105(SP10C00E105R7P3)**（2026-09-18 记录；与仓内冻结元组一致，见 `docs/n1b-disc-gate-plan.md:268`）。**0030 会复读**型号与版本，以实测为准。
