# 0.0.49 上架准备状态（DRAFT）

当前结论：资料草案已整理，**尚未达到提交审核或公开发布就绪**。用户已批准公开发布准备；本包不代表授权提交审核。

## 已证事实与边界

- 候选：`rel/0.0.49 @ cac219f`，0.0.49 / versionCode 49；四文件版本增量已提交并通过既有版本自检。业务基线 main `33703ae`。
- 当前产品是面向 self-host 组织私有组网的独立、非官方 NetBird 实现；SDK API 26，Rust core + ArkTS，phone / 2in1 ARM64。
- 注册包名 `cn.alfadb.netbird`、APP ID `6917608953844302724` 保持；展示名暂沿用 Netbird，Logo/品牌授权待核。
- 已有 PC 0.0.48 连接及 DNS 实测，不是 0.0.49 新包或手机测试；详见[设备验收](device-acceptance.md)。
- 根项目许可为 AGPL-3.0；依赖合规由另一作者负责，本包不替代其审查，也不修改许可或 notices。
- 当前使用个人华为开发者账号，企业账号尚未决定；个人和企业均有上架、团队、商户与 IAP 能力。若由真实公司长期运营，建议企业认证，但不能据此认定可全球发布。个人账号不能原账号直接变更为企业，需另认证企业账号并按应用转移流程办理；APP ID / 包名可按官方条件保留，旧包与签名材料不能假定自动沿用。
- 分发目标为中国大陆及海外；官方当前仅为 HarmonyOS 智能手表和运动手表提供全球地区消费者服务，其他设备仅在中国大陆提供服务。因此本项目 phone / PC（2in1）当前商店客户端下载范围为中国大陆，海外渠道开放待官方确认。

## 用户字段与待确认状态

用户本轮明确先处理账号区别与推荐；运营主体、联系邮箱和政策 URL 暂缓提供，以下不得由草案代填；不在此提供口令、证书、私钥或设备标识。

| 字段 | 用户填写 |
| --- | --- |
| 开发者账号类型（个人/企业）及实名完成状态 | 当前个人账号；是否另申请企业账号尚未决定，实名完成状态待确认 |
| 运营主体名称 | [暂缓，待用户提供] |
| 分发国家/地区 | 目标中国大陆 + 海外；当前 phone / PC 商店下载仅中国大陆，海外渠道待官方确认 |
| 支持邮箱 / 隐私联系邮箱 | [暂缓，待用户提供] / [暂缓，待用户提供] |
| 对外隐私政策 URL（境内外可访问） | [暂缓，待用户提供] |
| 已有备案状态、主体及鸿蒙包名匹配情况 | [待确认] |
| 展示名/Logo使用权及品牌处理 | [待确认] |
| 云管理证书或手动签名方式 | SDK26 上传重签（cloud / developer）路径待选择；当前仅有 CLI，尚未登录 IDE / 控制台选择路径 |
| 控制台 versionCode 49 是否可用 | [待确认] |

此表是[商店文案](store-listing-draft.md)与[隐私草案](privacy-policy-draft.md)的共同信息来源；确认后统一替换，避免重复收集。

## 提交前实际缺口

1. 完成上表；发布公开隐私政策前核定运营主体、联系方式和政策内容。当前应用内尚无隐私政策或许可证入口；草案文件存在不等于应用已提供政策链接，待主体与公开 URL 确认后补页面。草案没有生效日期。
2. 按最终包盘点实际 SDK/依赖并填写声明；依赖 license 残留待清理核实，合规结果由负责作者交付后接入审核材料。公开发布前仍须落实对应源码公开及渠道条款核实。
3. 完成[0.0.49 双端验收](device-acceptance.md)与至少三张不同截图，使用虚构或脱敏内容，不展示真实个人信息。
4. 准备不可调试的 Release `.app` / AppPack，选择并核实 SDK26 上传重签（cloud / developer）及证书路径；如选手动命令行发布签名，须具备对应发布材料，不能把缺本地发布 `.cer` 视为所有路径的唯一阻断。现有 debug 签名仅用于 QA。
5. 对照分发地区、主体和实际业务核实备案/资质；境外服务器不自动免备案。官方资质页不足以推出本客户端必然仅能企业分发或必需电信许可，应按实际类别核实。
6. 向审核核实 self-host SSO 是否适用审核规则 3.16 的华为账号选项要求；尚未确认，不据此新增登录实现。账号注销按适用规则 7.3 核实，设备清除配置与注销账户有别。

## 打包和测试渠道

双设备测试继续使用既有 debug profiles 的同源包，记录两包对应同一源码与未签名制品。公开 Release 走独立渠道；`app_gallery` Release 包不能作为普通 `bm` 安装包。debug → 商店签名升级是否可行及是否保留数据须验证，不能承诺。

本轮仅写资料；未构建、签名、装机、上传、创建证书或提交审核。后续包测试与发布需各自任务授权。

## 事实来源

- 源码：[`app.json5`](../../../client/AppScope/app.json5)、[`module.json5`](../../../client/entry/src/main/module.json5)、[配置序列化](../../../client/entry/src/main/ets/setup/NetBirdSetupConfig.ets)、[登录](../../../client/entry/src/main/ets/login/NetBirdAccountLogin.ets)、[页面与注销](../../../client/entry/src/main/ets/pages/Index.ets)。隐私实现边界见[隐私草案](privacy-policy-draft.md)。
- 官方：[发布地区边界](https://developer.huawei.com/consumer/cn/doc/doccenter-submission/agc-help-release-app-area-0000002313510997)、[账号权益](https://developer.huawei.com/consumer/cn/doc/start/services-and-benefits-0000002487782205)、[账号变更](https://developer.huawei.com/consumer/cn/doc/start/account-faq-0000001953714830)、[应用转移](https://developer.huawei.com/consumer/cn/doc/App/agc-help-maintain-transfer-0000002236494390)。
- 官方：[发布应用](https://developer.huawei.com/consumer/cn/doc/harmonyos-guides/ide-publish-app)、[Stage 包结构](https://developer.huawei.com/consumer/en/doc/harmonyos-guides/application-package-structure-stage)。
- 官方：[审核规则](https://developer.huawei.com/consumer/cn/doc/app/50104)（7.1 隐私 URL、7.2 SDK 声明、7.3 注销、3.16 第三方登录及截图/不可调试要求）、[备案](https://developer.huawei.com/consumer/cn/doc/app/50130)、[资质](https://developer.huawei.com/consumer/cn/doc/distribution/app/80301)。
