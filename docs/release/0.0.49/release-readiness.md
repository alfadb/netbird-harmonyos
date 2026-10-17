# 0.0.49 上架准备状态（DRAFT）

当前结论：资料草案已整理，**尚未达到提交审核或公开发布就绪**。用户已批准公开发布准备；本包不代表授权提交审核。

## 已证事实与边界

- 当前候选：发布分支 `rel/0.0.49` 的源码提交为 `85c0a44716bcfd5dd0130ae1aca8cdd7b13a1600`（merge: integrate main VPN recovery fixes），包含主线 `2360a4a` 的启动 DNS 有界重试（3× + 3/15s 退避，耗尽上报 UI）、重连验证绑定本次尝试、日志语义/死亡证据细分；合并时 Node 54/54、Rust 1114 passed / 0 failed 通过。0.0.49/49 当前产物及验签结果见下节；手机已覆盖安装并完成有限验收，PC 本轮未更新、未验收。先前 `ceda4da` 候选已被此后续提交取代；当前候选仍不是正式发行签名，本轮文档增量不属于已冻结产物。
- 历史候选（已被取代）：固定提交 `37e881e`，0.0.49 / versionCode 49；Node 全部 54 组通过，已从该提交构建 `unsigned.app` 及 PC / phone debug 候选并完成验签，PC 和 phone 已各一次从 48 覆盖安装至 49 并启动。该记录不能作为当前候选的双端验收。
- 原生测试同步修复后最后完整运行为 **1114 passed / 0 failed，53 个结果单位，exit 0**；历史 1112 passed / 2 failed 与 1113 passed / 1 failed 保留为前序结果。首装管理 URL 默认值已在源码清空，相关 145 项自检通过并保留已存配置；这些后续改动尚未加入已装的 `37e881e` 包，未做首装或注销真实 UI 验证。
- 当前产品是面向 self-host 组织私有组网的独立、非官方 NetBird 实现，仅限异地组网工具使用；我方提供客户端软件，不运营或提供网络接入、线路、管理平台、信令、中继/代理等网络服务，由用户自行部署、配置和管理其组网及相关服务。该经营声明不证明没有默认 URL、不调用境外/第三方服务、不能配置出口路由或没有个人信息处理；SDK API 26，Rust core + ArkTS，phone / 2in1 ARM64。
- 注册包名 `cn.alfadb.netbird`、APP ID `6917608953844302724` 保持；展示名暂沿用 Netbird，Logo/品牌授权待核。
- 已有 PC 0.0.48 历史实测及 `37e881e` 双端覆盖安装实测：PC 连接、断开重连、两个私域 DNS/ICMP 与公网域通过；phone 初次 UI connected/公网域通过，但私域 system ping 解析失败，后续状态与验证缺口见[设备验收](device-acceptance.md)，不能称双端功能全过。
- 根项目许可为 AGPL-3.0-or-later；依赖合规由另一作者负责，本包不替代其审查，也不修改许可或 notices。
- 当前使用个人华为开发者账号，企业账号尚未决定；个人和企业均有上架、团队、商户与 IAP 能力。若由真实公司长期运营，建议企业认证；本应用的国内渠道主体/资质要求仍待核实，不能据此认定必须企业账号。个人账号不能原账号直接变更为企业，需另认证企业账号并按应用转移流程办理；APP ID / 包名可按官方条件保留，旧包与签名材料不能假定自动沿用。
- 分发范围为**仅中国大陆；海外发布不在本轮范围**。按国内公开发布场景核实渠道和法律要求，不将海外渠道开放或 GDPR 等作为本轮发布前提。仅在国内分发不代表运行时不联系境外服务器：实际接收方、服务位置及跨境情况仍取决于用户所选部署和网络配置，见[隐私草案](privacy-policy-draft.md)。

## 当前候选产物（2026-10-17）

候选产物目录为 `/home/worker/n2h-kit/release-prep-49/build-final49-85c0a447-20261017T131012.703837Z/`；下表用产物简称，磁盘文件名统一加 `netbird-0.0.49-` 前缀。SDK `verify-app` / `verify-profile` 通过，`DEBUG=false`，release 0.0.49/49，API26，phone / 2in1 ARM64；debug 签名仅用于 QA，不代表商店发布签名或上架批准。

| 产物 | 字节数 | SHA-256 |
| --- | ---: | --- |
| `unsigned.hap` | 7391474 | `985dd1bc63d20ec3ee12554a4a4970ea7484f41ac67f8958a8a521355fb1b595` |
| `unsigned.app` | 3313471 | `aca587c64552c95931654b428e189466b780358c16367d5cb49a3f712bbe3907` |
| `pc-debug-signed.hap` | 7496910 | `e29dcd6e816d2081f58ece14a35e78f5c571c765003c4f343afda2572e226a9b` |
| `phone-debug-signed.hap` | 7496917 | `c87031db46178c063e5772d3ec508b1ced1edd8bb65d948bbb7028a616eb5413` |
| `source.tar.gz` | 26994630 | `334d70f12f2dd717ef03d92545e5f8f18e8c7b933122c85cff9717778e077d28` |

当前手机状态：PHYS-1 类 API26 手机覆盖安装成功（2.743s）、启动成功，保留既有配置；首次连接与一轮断开→重连显示已连接（13/20 对端）。2026-10-17 UTC 安装后早期解析/私网 ICMP 曾失败，14:05 起恢复；14:10:29–14:15:15 两个私域及私网 IP 的 20 轮采样各 20/20，持续 connected；14:24:18.734Z 起一次重连计时在 +0.53s / +0.64s / +0.77s 分别完成两个私域解析及 ICMP、私网 IP ICMP，未观测到解析空窗。用户手机浏览器的私域与 IP HTTP URL 均可打开；空白网页截图不作为 DNS 失败证据。早期失败归因未证实且现不可复现，平台 DNS 参数提交无直接日志证据；shell 与普通应用路径差异未完全证明，浏览器/TCP/AAAA/长期稳定性未系统测试，PC 本轮未更新、未验收。完整脱敏摘要与限制见[手机验收补记](device-acceptance.md#2026-10-17-手机验收补记当前候选utc)。以上不改变 DRAFT、未发布就绪的结论，品牌授权仍待回复，主体、隐私 URL、对应源码公开及最终渠道条款等缺口继续保留。

## 用户字段与待确认状态

用户本轮明确先处理账号区别与推荐；运营主体、联系邮箱和政策 URL 暂缓提供，以下不得由草案代填；不在此提供口令、证书、私钥或设备标识。

| 字段 | 用户填写 |
| --- | --- |
| 开发者账号类型（个人/企业）及实名完成状态 | 当前个人账号；是否另申请企业账号尚未决定，实名完成状态待确认 |
| 运营主体名称 | [暂缓，待用户提供] |
| 分发国家/地区 | 仅中国大陆；海外发布不在本轮范围 |
| 支持邮箱 / 隐私联系邮箱 | [暂缓，待用户提供] / [暂缓，待用户提供] |
| 对外隐私政策 URL（国内公开可访问） | [暂缓，待用户提供] |
| 已有备案状态、主体及鸿蒙包名匹配情况 | [待确认；国内 APP 备案/隐私要求正在查证，未出最终结论] |
| 展示名/Logo使用权及品牌处理 | 展示名仍为 Netbird；用户明确“那就先走Discussions”后，已由 alfadb 于 2026-10-16T03:54:10Z 在官方 Q&A / Support 创建唯一[品牌请求 #8228](https://github.com/netbirdio/netbird/discussions/8228)，并独立读回核验 title/body/author/category；请求已发，名称/Logo 许可尚未取得，不代表接受合同或上架批准，非官方声明继续 |
| 云管理证书或手动签名方式 | SDK26 上传重签（cloud / developer）路径待选择；当前仅有 CLI，尚未登录 IDE / 控制台选择路径 |
| 控制台 versionCode 49 是否可用 | [待确认] |

此表是[商店文案](store-listing-draft.md)与[隐私草案](privacy-policy-draft.md)的共同信息来源；确认后统一替换，避免重复收集。

## 提交前实际缺口

1. 完成上表；发布公开隐私政策前核定运营主体、联系方式和政策内容。当前应用内尚无隐私政策或许可证入口；草案文件存在不等于应用已提供政策链接，待主体与公开 URL 确认后补页面。草案没有生效日期。
2. 按最终包盘点实际 SDK/依赖并填写声明；依赖 license 残留待清理核实，合规结果由负责作者交付后接入审核材料。公开发布前仍须落实对应源码公开及渠道条款核实。
3. 完成[0.0.49 双端验收](device-acceptance.md)与至少三张不同截图，使用虚构或脱敏内容，不展示真实个人信息。
4. 准备不可调试的 Release `.app` / AppPack，选择并核实 SDK26 上传重签（cloud / developer）及证书路径；如选手动命令行发布签名，须具备对应发布材料，不能把缺本地发布 `.cer` 视为所有路径的唯一阻断。现有 debug 签名仅用于 QA。
5. 已整理[中国大陆发布法律风险说明](mainland-release-legal-review.md)：可以继续准备国内分发，尚未判定合法/侵权或达到发布条件。按已明确的客户端软件提供边界，不能将我方等同跨境网络服务运营商；特定经营资质按实际活动判断，个人/公司身份与网络服务经营、品牌/开源权利分开核对。国内商标/在先权检索、图标来源授权、品牌方案、著作权许可、AGPL 随包声明/对应源码及最终渠道协议、实际 APP 备案与隐私落实仍待处理，不作免监管承诺。收费与发布主体未最终决定；官方[品牌请求 #8228](https://github.com/netbirdio/netbird/discussions/8228)已按用户授权发送，尚未取得名称/Logo 许可，不代表合同接受或上架批准。当前名称、图标、源码 repoAppID、APP ID 与包名不变，不删除重建。
   - 发行前核对默认配置与声明一致：首装管理 URL 默认值已在源码清空并通过相关 145 项自检，保留已存配置；未加入已装 `37e881e` 包。基础 VPN DNS 的既有第三方配置见[隐私草案](privacy-policy-draft.md)，最终生效配置仍需核对。
6. 向审核核实 self-host SSO 是否适用审核规则 3.16 的华为账号选项要求；尚未确认，不据此新增登录实现。账号注销按适用规则 7.3 核实，设备清除配置与注销账户有别。

## 打包和测试渠道

双设备测试继续使用既有 debug profiles 的同源包，记录两包对应同一源码与未签名制品。公开 Release 走独立渠道；`app_gallery` Release 包不能作为普通 `bm` 安装包。debug → 商店签名升级是否可行及是否保留数据须验证，不能承诺。

本轮仅写资料；未构建、签名、装机、上传、创建证书或提交审核。后续包测试与发布需各自任务授权。

## 事实来源

- 源码：[`app.json5`](../../../client/AppScope/app.json5)、[`module.json5`](../../../client/entry/src/main/module.json5)、[配置序列化](../../../client/entry/src/main/ets/setup/NetBirdSetupConfig.ets)、[登录](../../../client/entry/src/main/ets/login/NetBirdAccountLogin.ets)、[页面与注销](../../../client/entry/src/main/ets/pages/Index.ets)。隐私实现边界见[隐私草案](privacy-policy-draft.md)。
- 官方：[发布地区边界](https://developer.huawei.com/consumer/cn/doc/doccenter-submission/agc-help-release-app-area-0000002313510997)、[账号权益](https://developer.huawei.com/consumer/cn/doc/start/services-and-benefits-0000002487782205)、[账号变更](https://developer.huawei.com/consumer/cn/doc/start/account-faq-0000001953714830)、[应用转移](https://developer.huawei.com/consumer/cn/doc/App/agc-help-maintain-transfer-0000002236494390)。
- 官方：[发布应用](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-publish-app)、[Stage 包结构](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/application-package-structure-stage)。
- 官方：[审核规则](https://developer.huawei.com/consumer/cn/doc/app/50104)（7.1 隐私 URL、7.2 SDK 声明、7.3 注销、3.16 第三方登录及截图/不可调试要求）、[备案](https://developer.huawei.com/consumer/cn/doc/app/50130)、[资质](https://developer.huawei.com/consumer/cn/doc/distribution/app/80301)。
