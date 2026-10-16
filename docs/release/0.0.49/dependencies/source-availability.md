# 0.0.49 对应源码与分发准备记录

本记录对应本地候选 `cac219fb3cc4773e1ed55a32e24cae21c17eb729` 的依赖输入，包含本轮未提交的依赖材料与元数据修正。候选尚未 push；本记录不表示已发布、已获渠道许可或已完成法律审查。

## 源码获取

真实项目仓库为 https://github.com/alfadb/netbird-harmonyos ，默认分支 main。本轮所知公开 main 基线为 `33703ae`；不可把此基线或当前 main 当成候选二进制的对应源码。候选提交尚不能声称可公开下载。没有在本轮创建 release、tag 或 source tarball，也不提供臆造下载地址。

发布负责人必须在发布前公开最终对应的完整 release commit/tag，并在随制品材料中记录该不可变 commit、公开获取地址和校验值。对应源码应包括 Rust/ArkTS 源文件、proto、构建脚本、Cargo.toml/Cargo.lock、ohpm 配置及最终实际锁文件、必要的生成输入、修改说明和本依赖材料。构建与安装所需信息须足以复现该发行版本，不应包含私钥、签名密码或个人配置。公开最终源码后应实际测试匿名获取；本记录暂不能代替该步骤。

## 构建步骤与依赖获取

从最终公开 release commit 检出后，按仓库 client/build.sh 的实际说明准备指定 HarmonyOS/OpenHarmony SDK、Node/hvigor/ohpm 与 Rust OHOS target。原生核心入口为 `bash client/core/build.sh`，应用入口为 `bash client/build.sh`。发布负责人须补记确切 SDK、Rust、Node、hvigor/ohpm 版本及可公开获取方式。本轮没有执行产品构建，因此不保证仅凭此草案可在全新机器上完整复现。

核心脚本实际执行 `cargo build --release --target aarch64-unknown-linux-ohos --offline --locked`，不传 root feature；`cli` 未启用。boringtun 使用关闭默认特性后的 ffi-bindings；smoltcp 0.12.0 和 hickory-proto 0.25.2 的确切特性由 Cargo.toml 与 manifest 记录。离线构建要求事先具备锁定 crates；应从 crates.io 对应精确版本获取并校验 Cargo.lock checksum，或提供有授权的完整依赖镜像/源码包。manifest 中仓库地址是来源参考，不能把上游最新分支当锁定版本源码。Rust std、平台 SDK/系统库单列；不声称其 SDK 源码属于本项目可再分发源码。

本次只执行两个 metadata 命令（均 offline、locked、format-version 1，其中一个增加 `--filter-platform aarch64-unknown-linux-ohos`）。全锁定第三方集合为 260 个 Cargo 组件，另有本项目 netbird_core 0.1.0。OHOS 源图分类有 runtime 170、build 70、dev 64、其他平台或未选中 23；角色可重叠。运行路径中过程宏仍属于编译期执行；metadata 分类不保证精确的链接作用。metadata 的 dev 特性统一及 host/target 上下文差异使这些集合成为保守上集，不声称 260 项或任何 SDK 全部进入最终 so。最终二进制清单需要对应构建的编译记录和链接证据。

两个产品 oh-package.json5 的 dependencies/devDependencies 均为空；产品目录没有 oh-package-lock.json5、npm package manifest 或 npm lock。历史 spikes 的锁文件不属于产品构建。SDK 随附 Node 工具的内部分发依赖不冒充产品 runtime 组件；最终工具链清单和许可义务仍需发布构建记录补齐。

## 许可、版权与来源缺口

项目根 LICENSE 为 AGPLv3 全文；`docs/license-and-headers-policy-20260913.md` 第 7 行明确新源文件采用 AGPL-3.0-or-later，本次据此修正两个 ohpm 元数据。上游原许可不被项目许可覆盖。

完整组件名、版本、Cargo checksum、已验证 crate archive SHA-256、规范化 manifest SHA-256、声明许可、作用域、依赖边和复制文本 SHA-256 见 dependency-manifest.json；相对文本链接见 dependency-licenses.md。它是项目 JSON manifest，不伪称 CycloneDX/SPDX。保存的是实际缓存包内 458 份 license/NOTICE/copyright 等命名文件（含子目录）；该机械文件名筛选不等于完成每个源码头、内嵌数据、逐目录义务与许可证分支选择的法律核查。原始作者版权文本保留不改写。

9 个原先未按独立许可文件名收集到文本的组件均已补齐本范围证据：asn1-rs-impl 0.2.0、boringtun 0.7.1、defmt 0.3.100、defmt-parser 1.0.0、r-efi 5.3.0/6.0.0、tonic-prost 0.14.6、tonic-prost-build 0.14.6、valuable 0.1.1。文本取自各自已验证 archive 的 `.cargo_vcs_info.json` 所指官方固定 commit；原始包清单与 archive 的 Cargo.toml.orig 均逐字相同。r-efi 的完整 MIT 原文及真实版权在 AUTHORS 中，本候选资料选 MIT 分支；AUTHORS 也与本版本 archive 核对，不把另两许可的短声明说成完整全文。新增来源 URL、commit、原文 SHA-256、archive 元数据见 manifest 的 upstreamSource。

8 个旧式斜杠原字段均原样保留，其中 asn1-rs-impl、bitflags 1.3.2、curve25519-dalek-derive、enum-as-inner、rusticata-macros、version_check 的 README/源码明确任选，另记解释为 MIT OR Apache-2.0，不声称原字段已是规范 SPDX。fnv 1.0.7 未找到明确任选原文，minimal-lexical 0.2.1 的“dual licensed … as well as”仍不足以消除 OR/AND 歧义，保留这两项具体 gap。ring 0.17.14 已按实际包内 LICENSE 及对应 ISC/BoringSSL/once_cell/fiat 文本复核，补入固定 commit 的 fiat AUTHORS 并保留代表性 archive 来源头；其 VCS dirty=true，不声称整个 archive 等于 git commit。已完成原 17 个组件中 15 个的本范围资料修补，仍不是逐文件法律审计。逐项依据与限制见 [许可缺口复核](license-gap-review.md)。

## NetBird 固定上游证据册

上游参考为 https://github.com/netbirdio/netbird ，固定 `67c2041fb5534c19d7b2320617cfc3aaab8e7d3d`。licenses/netbird-67c2041 内文本来自该 commit 的 git 对象，而非最新 main；来源指纹在 manifest。其根声明：除顶层 management/、signal/、relay/、combined/ 外适用 BSD-3-Clause；上述服务端目录适用 AGPLv3。版权归 NetBird GmbH & AUTHORS；保留 BSD 免责与不背书条款。项目是独立非官方客户端，不暗示 NetBird 维护或背书。

具体引入记录：management.proto 基于 `791401060d2b95e5f51e3439c0649729132f571e` 的 shared/management/proto/management.proto，追加 SessionExtend 字段来自固定 67c2041 的 shared 路径；本地 SHA-256 为 `e51eaf4c00d2e6a5136e2472c6c35f740f143b0e58e5a25d2a9f2dfdc2a5d977`。signalexchange.proto 来自同一 7914010 的 shared/signal/proto/signalexchange.proto，本地 SHA-256 为 `9374413168551f7ded6c009dda2bfaa55e111bf138f2fe61780752a05c566ab9`。这是既有 proto 来源登记与当前文件指纹；本轮确认本地参考仓库缺该 7914010 对象后，从官方固定 commit raw HTTP 取得其 LICENSE/AUTHORS，原文及指纹见 upstream-sources/netbird-7914010 与 manifest 的 historicalUpstreamSnapshot，未冒称来自本地 git 对象。该根声明把 shared 路径列在顶层服务端排除目录之外，历史目录级许可依据已补齐；未再次验证两份 proto 的全部字节或逐行派生来源。shared 路径不能因名称 management/signal 而当顶层服务端目录。

Rust 客户端还依据固定 67c2041 的客户端/shared 协议与行为实现，包括 DNS、ICE、relay、认证及路由语义；不是 Cargo 中链接整个 Go SDK。部分注释和协议说明参考服务端行为。尚未完成全部逐行复制/派生来源映射，不能因此断言不存在服务端 AGPL 复用、全部复用均已授权或全部版权义务已清零；发布前应核全并保留相应原声明。

## 发布前仍须关闭

仍须解决 fnv 1.0.7 与 minimal-lexical 0.2.1 的表达式解释缺口，核全源码逐行来源、工具链/最终二进制范围，并将相应 NOTICE 和完整许可材料随制品分发。本轮根 NOTICE 原不存在，已新增主要直接组件及完整索引入口，包括 smoltcp/hickory，项目许可表述遵循现有 AGPL-3.0-or-later 政策。目标渠道（包括华为渠道）条款与 AGPL 源码提供、署名保留义务的兼容性尚待核查并留档，不能标为已通过。现有缺口意味着政策发布门槛尚未满足；本轮仅交付可审计准备材料，不执行新公开 release。
