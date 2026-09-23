# 华为开发者支持工单两轮官方回复与项目现状对照（2026-09-23）

## 0. 说明

- 本文只做**归档与对照**：不修订 v4 预登记正文，也不修订仓B 登记件，不构成任何门结论；N2-H 正式状态不变。
- v4 预登记正文（字节锚定，只读，本文未修改）：`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md`，
  `sha256 a7510430f3ab402c3cd11f1ef39889b6092b8cf3ec8a4b14d2e90c25bc0c0dc7`（2026-09-23 `sha256sum` 实测）。
- 仓B 登记件（只读，本文未修改）：`/home/worker/harmonyos-signing/netbird-n1bdisc/records/n2h-preregistration-effective-v4-20260920.json`，
  `sha256 a94fb6eea1c80f757ab0334073fe02342809012f3f50a2de78772fbf652dcc83`（同日实测）。
- 原工单（仓外只读）：`/home/worker/huawei-support/20260921-vpn-extension-blockers.md`，12379 字节，
  `sha256 6bf8a0904c3f3a4d09b9e5dfd7b3b34619d9634d3fcb5b8b85c9d5c02fa91a82`（2026-09-23 更正时实测）。该文件在本文撰写期间被**并行扩写**
  （7213 B → 12231 B），新增 §七「官方回复（归档）」（L125–191，内容即本批回复原文）；本文引用的
  L31–32／L41–42／L64–66／L79–81 全部位于被追加段落**之前**，行号未受影响（逐行实读核对）。
- **sha256 过期原因（2026-09-23 更正）**：上次提交后该文件又修订一次——§七末尾「后续」行改为中性措辞（点明
  protect 不返回与 MANAGE_VPN 校验失败行「二者因果未证实」并指向第三轮草稿），字节数 12231 B → 12379 B，
  sha256 随之变化（原记录 `465a1df313477417644824b33306b91c96176c95a11efbbd84910d83e3321a8c` 已过期）；
  本次复核行号未受影响（L31–32／L41／L64–66／L79–81／L125 逐行实读一致）。
- 回复原文（仓外只读，逐字节权威）：
  - 第一轮（2026-09-21 转达）：`/home/worker/huawei-support/reply-round1-relayed-20260921.txt`，883 字节，
    `sha256 2b183aab1457e8866e65e0a67bf101d015fa58676c20fa10a472f1a6c52c5a21`
  - 第二轮（2026-09-23 转达）：`/home/worker/huawei-support/reply-round2-relayed-20260923.txt`，3119 字节，
    `sha256 27b52bec39c9b07d16e69972d51defca56f003ffe02db31ad54e5b2dd877dc81`
- 第三轮追问草稿（仓外只读，**现已存在**）：`/home/worker/huawei-support/20260923-round3-protect-manage-vpn.md`，
  附件 `/home/worker/huawei-support/20260923-round3-hilog-excerpt.txt`（2026-09-23 撰写；本文初稿曾记其尚未起草，现更正）。
- 下文 §1 两个围栏代码块（语言标记 `text`）与上述两个 txt 文件**逐字节一致**（行尾统一 LF；以 `cmp` 实测）。
  原文中的错字、半角引号与行尾空格一律**原样保留**，不作清理。

## 1. 回复原文

### 第一轮（2026-09-21 转达；答问题一、二）

```text
尊敬的开发者，您好，
针对问题一/二：
/proc/net/dev: Permission denied (os error 13)
/proc/net/dev 属于系统级 procfs 节点，不在应用沙箱可访问范围内
【已查询的资料显示】：
getIfaceRxBytes/getIfaceTxBytes虽可替代RX/TX bytes 字段，但只提供了接口级字节，不满足包计数。
statistics.getSelfTrafficStats提供的包计数为"按本应用 + 蜂窝/Wi-Fi 网络类型"不满足您的需求。
因此对于您的【VPN 类应用做“接口计数与自身引擎计数对账】疑问。
推荐您在 VpnExtensionAbility 进程内，对 vpn-tun 的 TUN fd 进行 read() / write() 计数。每次 read/write 对应一个 IP 包，可同时累计字节数和包数。同时使用getIfaceRxBytes / getIfaceTxBytes统计的字节数与 TUN fd 读写累计字节数对比，间接验证数据完整，是否满足您的需求。
```

### 第二轮（2026-09-23 转达；补答问题一至四）

```text
尊敬的开发者，您好，
关于您的反馈的问题，
1. /proc/net/dev 在应用进程内被拒
无公开接口或文档承诺应用可读 /proc/net/dev，平台无法提供书面禁止。

2. 接口级包计数缺失
"接口级包计数"确实无对应接口（getSelfTrafficStats 的包计数维度是"应用 × 网络类型"，与 vpn-tun 无交集）。如您有强烈需求，可以为您走需求流程。需要您补充反馈使用场景和需求不满足可能带来的影响，例如：
原始场景：什么样的业务场景？什么样的交互流程？哪一个过程遇到了问题？
影响：什么时间用到？是否高频？有无三方库可以做到？若提供该能力，是否会造成大工作量返工？
需求提出人： 
期待实现时间：

3.1 应用自身 socket 的报文不进入自身 VPN
判定一个包是否进 VPN 只看两件事：(a) 是否匹配 VpnConfig.routes（含最长前缀/优先级规则），(b) 发送方是否被 trustedApplications/blockedApplications 列表纳入。不存在"因为是 VPN 应用自身，所以默认被排除"的说法。隧道承载 socket 必须 protect()（开发步骤"建立隧道 → protect 隧道 socket → create"，isBlocking=true 时不做会自环）；VPN 管理/自诊断 socket 用 protectProcessNet() 做进程级排除；若希望整个应用都不走自己的 VPN，把自身包名填进 blockedApplications（注意：这会把该应用本该走隧道的流量一并排除，需按业务取舍，且不能再同时配 trustedApplications）。

3.2 SO_MARK / fwmark 位布局是否有公开文档
文档只承诺 bindSocket 的语义——"将套接字绑定到特定网络"（ArkTS NetHandle.bindSocket；C OH_NetConn_BindSocket(fd, netHandle)，起始版本 12）。没有定义 SO_MARK 的位布局，也没有定义 mark 与 netId 的换算关系，因此不能据 mark 值反推 netId 或判断"绑定到了哪个网络"。

3.3 让自身进程内某条 socket 走隧道的做法
就是您已用通的路径——拿到 VPN 网络的 netId（从 getAllNets() / getConnectionProperties() 中认出 interfaceName = vpn-tun 的网络），再 NetHandle.bindSocket(socket) 或 OH_NetConn_BindSocket(fd, netHandle)。

4.1 应用沙箱本就不可见 /data/local/tmp
/data/local/tmp 在文档中是"一般暂存区目录"，供 DevEco 调试/测试生成文件使用；它不属于应用可见范围。

4.2 诊断文件交付路径
参考命令行走应用沙箱（官方《访问设备文件》"命令行方式访问应用沙箱"）：
hdc file send -b {bundleName} "D:\diag.txt" "./data/storage/el2/base"
hdc file recv -b {bundleName} "./data/storage/el2/base/diag.txt" D:\

或者s'y定位问题常用的抓取hilog日志方法：
抓日志步骤：
1.hdc shell
2.cd data/log/hilog
3.hilog -w clear (清除多余日志，迅速复现问题，可以有效减少冗余的日志信息，高效定位关键日志)
4.复现，记下并提供问题出现的时间点。
5.exit，然后执行hdc file recv /data/log/hilog D:\log
6.打包压缩D:\log中的所有文件并上传

合作愉快。
华为开发者联盟
```

## 2. 逐条对照表

| 回复条目 | 对应原问题（工单行号） | 项目现状与证据（文件:行号） | 结论 / 影响 |
| --- | --- | --- | --- |
| 1 `/proc/net/dev`（第一轮「属于系统级 procfs 节点，不在应用沙箱可访问范围内」；第二轮「无公开接口或文档承诺应用可读 /proc/net/dev，平台无法提供书面禁止」） | L31–32（是否被平台有意禁止；若是请给书面说明） | 应用内直读恒 `os error 13`（`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:75`）；v4 已改用 `@ohos.net.statistics` 字节口径（同上 `:76`） | 归因措辞＝**不在应用可访问范围、无公开承诺可读**，而**不是**「书面禁止」；设计结论不变（不依赖 `/proc`，`/proc` 与 hdc fallback 字面保留不删） |
| 2 接口级包计数（第二轮「确实无对应接口」，可走需求流程） | L41（是否存在／可否申请接口级包计数公开接口） | v4 已登记具名缺口 `packets-identity=unavailable-no-public-per-iface-packet-api`（`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:82–83`） | 缺口获厂商**书面确认**；子选项 A 保留条件（同上 `:83`：未来出现具名 per-iface 包计数来源且经真机验证）仍**不满足**；需求流程：**用户 2026-09-23 决定不提** |
| 第一轮建议「对 vpn-tun 的 TUN fd 做 read()/write() 计数」并与 `getIfaceRxBytes/TxBytes` 对比 | L42（平台推荐的对账做法） | 与 core 自计 `rx_bytes_to_tun` **同源**（`client/core/src/wg_device.rs:258` 字段定义、`:1148` 在 TUN 写成功分支自增）；v4:82 明禁以 WG 自计填 `delivered_frames`，`:84` 给出同源排除理由（tee 与 core 自侧计数不构成独立账本） | **不采纳**（用户 2026-09-23 同意）：fd 计数由 core 自己产生，与 `rx_bytes_to_tun` 同一来源，不能满足「独立账本」要求；字节独立口径仍由 statistics 承担 |
| 3.1(a) 判定规则：只看 `VpnConfig.routes` 与 `trustedApplications/blockedApplications`；**不存在**「因为是 VPN 应用自身所以默认被排除」 | L64（第 1 问：自身流量不进自身 VPN 是否为平台预期行为） | 0021：未绑定探针 `mark=0x0`、走默认网络 wlan0；该会话 VPN 网络 routes 仅 `192.168.50.0/24` + 排除 `/32`，**不含** `100.108.0.0/16`；内核主表有 `vpn-tun 100.108.0.0/16` 且无默认路由（`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:25`）；本仓 `docs/n2h-posctl-probe-net-binding-20260920.md:5-11` 曾把机制标为「**推断**」 | 现象与厂商规则**一致**：目的地址未匹配 `VpnConfig.routes`，所以该报文不进 VPN；**更正**：项目此前「应用自身 socket 默认不进自身 VPN」的说法**不成立**，正确表述是「目的地址不匹配 routes 才不进」；策略路由表仍未直接观测，**保留该边界**。产品含义：未导出默认路由时，**其他应用**访问隧道网段也不会进 VPN——当前映射器 `client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:250-283`（`VpnConfig` 字面量在 `:276-281`）**只写** `addresses`/`routes`/`dnsAddresses`/`mtu` 四个字段，routes 不含隧道网段；用户决定放到**证据轮之后**再改，并需**单独登记** |
| 3.1(b) 隧道承载 socket **必须 protect()**（「建立隧道 → protect 隧道 socket → create」，`isBlocking=true` 时不做会自环）；VPN 管理/自诊断 socket 用 `protectProcessNet()` | 原工单**未提** protect 被拦（L64 只问 `blockedApplications`／`protectProcessNet()` 是否即官方自排除机制） | run1（2026-09-14）hilog：应用**真正调用了** protect，:27/:28/:30/:31 四条 `VPN_PROTECT_TIMEOUT|fd=26/36/39/42|boxMs=5000`，**无** settled/rejected（`/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260913-0001/device-run-1.hilog.log`，sha256 `336bfe3c655e7f2b8bc2a4595a500dea97d7702d5b4fb6acddf610af6080770d`；该日志按应用域过滤，不含平台行，`stillPending` 零命中）；2026-09-16 全量日志（`/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260915-0004/hilog-d1-20260916T0805-full.log`，sha256 `93fa3ca724bb719a13b8cf05094b7f870a699d72e9eb3b7e1fab411fa181ea51`）：该次运行应用**未调用** protect（:2556 `VPN_PROTECT_DEVIATION_ARMED`；:2558/:2559/:2736/:2858 `platform_call=not-attempted`），netmanager 仍报 :2510 `Caller is not allowed, need sys permissive`、:2738/:2851 同句、:2739/:2850 `permission check failed, permission:ohos.permission.MANAGE_VPN, callerToken:537437623`——故「protect 被 `MANAGE_VPN` 校验拦截」的**因果未证实**，已证实的只有「protect 的 Promise 不返回」；SDK **声明无权限要求**（`/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/ets/api/@ohos.net.vpnExtension.d.ts:228-275`，全文件 `@permission` 零命中）；`protectProcessNet` 的包装函数 `protectProcessNetWithBox` 在仓内**只有定义、无任何调用点**（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:2561-2592`，SDK 调用在 `:2565`）；manifest 注释同记该冲突与声明即安装失败（`client/entry/src/main/module.json5:62-71`；其中 `9568289` 为**项目内记录，无原始安装日志留存**，见 §5 第 3 条） | **与厂商「必须 protect()」陈述冲突的是 protect 不返回**（已证实：4 条 fd 的 protect Promise 5s 超时、不 settle）；「被 `MANAGE_VPN` 校验拦截」的**归因未证实**（09-16 未调用 protect 的运行同样出现该权限失败行，见上行证据），**逐 socket protect 仍为 `UNSAT`**。第三轮追问草稿**现已存在**：`/home/worker/huawei-support/20260923-round3-protect-manage-vpn.md`（附件 `/home/worker/huawei-support/20260923-round3-hilog-excerpt.txt`；工单 §七 末尾 L191 亦指向该草稿）；现行替代仍为**端点路由排除**（`route-exclusion`），属 deviation，**非**原 N2 判据 pass。附：OpenHarmony 开发指南《连接VPN》文首「说明」块载明「使用该功能需要 ohos.permission.INTERNET 权限」（<https://raw.githubusercontent.com/openharmony/docs/master/zh-cn/application-dev/network/net-vpnExtension.md>，2026-09-23 抓取核对），即该指南的权限要求**仅及 INTERNET**；仓内 manifest 实读只声明 `INTERNET` 与 `GET_NETWORK_INFO`（`client/entry/src/main/module.json5:43-61`） |
| 3.1(c) `blockedApplications` 填自身包名＝整应用排除 | L64（第 1 问） | 代码未使用；治理不允许单独以其构成 pass（`docs/native-nx-governance.md:70`、`:133`） | 厂商确认其为**官方做法**；代价：本应用**该走隧道**的流量一并被排除、且与 `trustedApplications` **互斥**；定位＝protect 不可用时的**备选**，待第三轮答复后再决策 |
| 3.2 `SO_MARK`／fwmark 位布局无公开文档，**不能据 mark 反推 netId** | L65（第 2 问） | 项目只输出原始十六进制/数值、不解码（`docs/n2h-posctl-socket-context-20260920.md:34`；`docs/n2h-network-context-probe-20260920.md:48`） | 既有**解读纪律获厂商确认**，无需改动 |
| 3.3 让自身进程内某条 socket 走隧道＝`bindSocket` 路径 | L66（第 3 问） | 提交 `2962c20`（`feat(diag): bind the posctl probe to the vpn network`，在 HEAD `fd5f71c` 祖先链上）；0022 `N2H_POSCTL_BIND_NET\|…\|result=bound`（`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:24`） | **E-1 所用方法即厂商认可路径**（从 `getAllNets()`／`getConnectionProperties()` 认出 `interfaceName = vpn-tun` 的网络再 `bindSocket`） |
| 4.1 应用沙箱本就**不可见** `/data/local/tmp` | L79（第 1 问） | 0027 **否定结论**（`docs/n2h-device-evidence-preregistration-v4-draft-20260920.md:28`、`:75`） | 厂商确认，无需改动（该固定路径已登记为「不在应用可见视图内，非待办」） |
| 4.2 官方投递路径＝`hdc file send/recv -b {bundleName}` | L80（第 2 问；**L81 未直接答复**） | 仓内**从未使用** `-b`；仓内构建为 release（`client/build.sh:48`），设备轮用 Debug 类型 profile 签名（`docs/device-relay-enablement-runbook-20260914.md:557`）；AUTH 0029 第三项即测 `hdc shell -b` 写入（仓外 `/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260921-0029.json`，实读 `is_draft=true`／`authorized=false`／`approval.status=awaiting`，`.sha256` companion 校验 OK） | 前提（OpenHarmony hdc 文档 <https://gitee.com/openharmony/docs/blob/master/zh-cn/application-dev/dfx/hdc.md>，2026-09-23 实读）：`-b` 对应的已安装应用须「**使用调试证书签名，并且已在设备上启动**」，否则报 **E003001**（指定的包名非法）；远端路径越出应用数据目录报 **E005102**（另有 E005004：SDK/ROM 不支持 `-b`）。**未经真机验证前不作任何判据依据**；AUTH 0029 第三项**原样保留**（用户 2026-09-23 决定）。第二轮回复中的 hilog 抓取步骤**留作第三轮复现时使用** |

## 3. 结论

- **v4 不修订**：两轮回复不改变 v4 已登记的判据、缺口注记与字节口径；v4 正文与仓B 登记件 sha256 均与 §0 所列一致（本文未触碰两者）。
- **正式状态仍为 `n2h-inconclusive`**：本文不构成任何门结论，也不解除任何既有义务。
- **三项缺口不变**：①`wg_peer` 点对点直连；②服务端投递回执；③计数同源标定。
- **新增两项发现**：①**protect 不返回**（厂商称隧道 socket 必须 `protect()`；实测 run1 的 4 条 fd 全部 5s `VPN_PROTECT_TIMEOUT`、Promise 永不 settle——**已证实**；「被 `ohos.permission.MANAGE_VPN` 校验拦截」的归因**未证实**，见 §2 的 3.1(b) 行与 §5 第 1 条）；②**隧道网段未在 routes 中声明**（`VpnConfig.routes` 不含 `100.108.0.0/16`，故其他应用访问隧道网段同样不进 VPN）。

## 4. 2026-09-23 用户决定

逐字批准语：

> 同意你的推荐，我现在出差了，等两天回来了再做真机部分，现在可以做其它的

据此四项决定：

1. **第三轮工单现在发**，附旧日志（内容＝`VpnConnection.protect()` 被 `ohos.permission.MANAGE_VPN` 校验拦截，见 §2 的 3.1(b) 行）。
2. **不提包计数需求**（不走厂商需求流程）。
3. **AUTH 0029 第三项原样保留**（`hdc shell -b` 沙箱写入可用性探针不动）。
4. **隧道网段路由放到证据轮之后**再改（并需单独登记）。

真机部分（安装、复现、取证）**待用户返回后进行**；在那之前不执行任何设备操作。

## 5. 更正记录（2026-09-23）

本小节列出 2026-09-23 复核后对本文初稿及既有记录的更正口径；除本节所列外，本文其余判断与 v4 正文不变。

1. **「protect 被 `MANAGE_VPN` 校验拦截」的因果未证实**。初稿把 run1 的 netmanager 权限失败行与 protect 不返回写成了因果；复核后（证据与行号见 §2 的 3.1(b) 行）：2026-09-16 全量日志中应用**未调用** protect（`hilog-d1-20260916T0805-full.log`:2556 `VPN_PROTECT_DEVIATION_ARMED`；:2558/:2559/:2736/:2858 `platform_call=not-attempted`），netmanager 仍报 :2739/:2850 `permission check failed, permission:ohos.permission.MANAGE_VPN, callerToken:537437623`。因此**已证实的只有「protect 的 Promise 不返回」**，MANAGE_VPN 归因留待第三轮答复。
2. **`docs/device-validation-run1-handoff-20260914.md:29` 的记述与原始日志不符**。该行称「实测 5 个 fd 全部 `VPN_PROTECT_TIMEOUT|...|stillPending=true`」；原始日志实为 **4 条** `VPN_PROTECT_TIMEOUT`（`device-run-1.hilog.log`:27/:28/:30/:31，fd=26/36/39/42，boxMs=5000），且**无 `stillPending` 字段**（全文件 `stillPending` 与 settled/rejected 均零命中）。该 handoff 文件本次**未修改**，此处仅登记更正。
3. **「声明 MANAGE_VPN 即安装失败（`9568289`）」无原始依据**。该结论仅见于项目自身记录（`docs/native-nx-governance.md:122`、`docs/n2h-device-evidence-preregistration-draft-20260917.md:36`、`client/entry/src/main/module.json5:66` 注释），仓内外**均无原始安装日志留存**；本文凡引用该处（§2 的 3.1(b) 行 manifest 注释）均标注为**「项目内记录，无原始安装日志留存」**，不构成独立证据。
