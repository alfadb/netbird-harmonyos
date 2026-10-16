# THIRD-PARTY-NOTICES — 0.0.49 候选依赖与归属

本材料对应本地候选 `cac219fb3cc4773e1ed55a32e24cae21c17eb729` 的实际锁定依赖输入。候选未 push，尚未发布；许可材料仍有明确缺口，不表示已获商业法律许可或已满足公开发布门槛。项目根 LICENSE 为 AGPLv3；项目源文件政策明确采用 AGPL-3.0-or-later，第三方原许可及版权声明继续适用。

- [完整机器清单与依赖图](docs/release/0.0.49/dependencies/dependency-manifest.json)：260 个第三方 Cargo 组件，逐项与 client/core/Cargo.lock 对账；含精确版本、checksum、声明许可、来源、特性、作用域和文本指纹。该文件是项目 JSON manifest，不声称符合 CycloneDX/SPDX 标准。
- [逐组件许可证与版权文本索引](docs/release/0.0.49/dependencies/dependency-licenses.md)：原样保留离线精确版本包内发现的 458 份许可证/NOTICE 等文本，另补 13 个固定 commit 的许可/版权文本引用，合计 471 个组件文本引用；来源依据及 SHA-256 在机器清单。许可证标签不能代替版权全文，机械收集也不等于完整逐文件法律审查。
- [对应源码获取、固定上游证据、范围与发布缺口](docs/release/0.0.49/dependencies/source-availability.md)：公开仓库、候选未公开状态、发布前源码要求、构建入口、SDK 与渠道待核条款。

## 实际范围

核心目标为 aarch64-unknown-linux-ohos；client/core/build.sh 不启用 root cli feature。离线锁定 metadata 全图包含 260 个第三方组件；OHOS 过滤后的源依赖图分类为 runtime 170、build 70、dev 64、其他平台或未选中 23，分类重叠。包括新增 DNS 依赖 smoltcp 0.12.0（0BSD）和 hickory-proto 0.25.2（MIT OR Apache-2.0），其全部锁定传递依赖均列出。构建/测试依赖与跨平台包保留用于完整审计；这些是 metadata 源图上集，不是最终 libnetbird_core.so 的精确链接清单。过程宏在编译期执行，dev feature 统一和 host/target 上下文亦可能扩大集合。

两个产品 ohpm 配置均无 dependencies/devDependencies，产品目录没有 oh-package-lock.json5 或 npm 包清单/锁文件。历史 spikes 的依赖不列为产品依赖。HarmonyOS/OpenHarmony SDK、Rust 编译器/标准库及 libc.so、libhilog_ndk.z.so、libace_napi.z.so 单独登记为外部工具链/系统组件；不将它们的源码当成本项目可再分发源码，也不声称整个 SDK 进入最终 so。

## NetBird 归属

NetBird GmbH & AUTHORS 的 BSD-3-Clause 原版权、条款与免责全文保存在 [固定 67c2041 上游 LICENSE](docs/release/0.0.49/dependencies/licenses/netbird-67c2041/LICENSE)。文本取自 `67c2041fb5534c19d7b2320617cfc3aaab8e7d3d` 的 git 对象：除顶层 management/、signal/、relay/、combined/ 外适用 BSD-3-Clause，上述服务端目录适用 AGPLv3；已保存能取得的相应目录许可证。未用上游最新 main 的声明替代固定 snapshot。

引入的 management.proto 和 signalexchange.proto 位于上游 shared/；历史基线为 `791401060d2b95e5f51e3439c0649729132f571e`，management.proto 另含固定 67c2041 的 SessionExtend 追加字段。当前 SHA-256、准确路径与新增历史固定许可证据见源码说明。客户端还参照固定 67c2041 的协议和行为实现；未核全逐行来源，不作“全部只来自 BSD”或已完成所有许可义务的结论。独立非官方项目不代表 NetBird 维护、背书或支持。WireGuard 等名称/商标归各权利人所有。

## 尚存缺口

9 个原先缺独立文本项已补齐对应固定版本原文；8 个旧斜杠字段中 6 个有明确任选依据，原字段仍逐字保留，另记 OR 解释；ring 0.17.14 已完成本范围的混合来源文本/代表性源码头复核并补 fiat AUTHORS，VCS dirty=true 的限制明确保留。原 17 个组件中 15 个完成本范围资料修补，仍有 **fnv 1.0.7、minimal-lexical 0.2.1** 两项表达式解释 gap，原因见[逐项复核记录](docs/release/0.0.49/dependencies/license-gap-review.md)。历史 7914010 的固定 LICENSE/AUTHORS 也已从官方 raw HTTP 补齐，本地无该 git 对象的事实如实记录。新增[根 NOTICE](NOTICE) 列主要直接组件与完整索引。仍须核全源码逐行来源、最终工具链/二进制范围、对应源码公开获取及渠道 AGPL 兼容条款；本次不声称公开发布门槛已通过。
