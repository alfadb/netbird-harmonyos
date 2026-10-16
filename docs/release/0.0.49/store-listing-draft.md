# Netbird 商店资料草案（DRAFT / 0.0.49）

仅供核对与填写；发布主体、地区、联系方式和品牌事项统一填写[准备状态表](release-readiness.md)。本稿尚不可直接提交审核。

## 商店文字候选

**展示名：** Netbird（暂沿用，品牌/Logo使用权待确认）

**一句话介绍：** 面向自建 NetBird 网络的 HarmonyOS 非官方客户端。

**详细介绍：**

Netbird 是独立开发的非官方 NetBird 客户端，面向已部署自建管理服务的组织及其授权用户，帮助 HarmonyOS 手机和 2in1 设备接入组织私有网络。使用前需要可访问的管理服务地址，以及组织提供的 Setup Key 或可用的账户登录方式。

你可以使用 Setup Key 注册设备，或通过管理服务配置的 SSO 与 PKCE 流程登录；查看对端及直连/中继连接状态，使用管理服务下发的网络路由、出口节点和 DNS 配置，并按界面提供的选项管理连接。应用提供本地运行日志导出，便于用户自行诊断连接问题。

网络可达性、路由权限、出口能力和身份服务由用户选择的部署及管理员配置决定。后台连接受系统资源管理和网络状态影响。应用不承诺始终后台在线或永不掉线。

本应用由独立开发者维护，非 NetBird 官方产品，不表示获得 NetBird 官方认可或授权。NetBird 名称和商标归各自权利人所有。

**版本说明候选：** 0.0.49 为公开分发准备候选；版本编号从 0.0.48 更新为 0.0.49。不得将仅版本增量描述为新增业务功能；最终说明需与验收通过的实际商店包一致。

## 截图取材清单

至少准备三张不同截图，建议首页连接状态、对端列表、路由/设置页各一张；phone 与 2in1 素材按实际分发设备补齐。截图应来自最终候选的实际界面，不伪造功能；用虚构或脱敏数据，遮去真实姓名、邮箱、服务器地址、节点名、IP、凭据及诊断细节。

## 审核填写注意

- 支持邮箱、隐私邮箱、隐私 URL、运营主体及分发地区均从集中表确认后填入；不使用占位值作为正式提交数据。
- 类别及业务资质按实际使用场景核实，不臆填为必需电信许可或仅企业可发。
- 应用有 VPN 连接、网络状态读取及后台任务相关声明；权限用途详见[隐私草案](privacy-policy-draft.md)，SDK声明以最终包盘点为准。
- self-host SSO 的审核规则适用性待核，不声称已符合华为账号选项要求。

## 功能事实定位（内部核对，不复制到商店正文）

- Setup Key：[配置构造](../../../client/entry/src/main/ets/setup/NetBirdSetupConfig.ets)；SSO/会话延长：[账户登录](../../../client/entry/src/main/ets/login/NetBirdAccountLogin.ets)。
- 对端、路由/出口、DNS及日志入口：[页面](../../../client/entry/src/main/ets/pages/Index.ets)、[路由存储](../../../client/entry/src/main/ets/routes/NetBirdRouteStore.ets)、[DNS核心](../../../client/core/src/dns_resolver.rs)、[日志导出](../../../client/entry/src/main/ets/vpnextensionability/NetBirdLogExport.ets)。这些是源码能力，0.0.49双端实测仍待完成。
