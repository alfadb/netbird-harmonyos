# 17 个组件许可资料缺口复核

本页只复核既有 17 个组件登记的“许可文本缺失、旧斜杠表达式、ring 混合来源资料”问题，不是逐文件法律审计或公开发布许可结论。结果为 **15 个组件完成本范围的资料修补，2 个组件保留表达式解释缺口**。9 个缺文本项均已有原文；8 个斜杠项中 6 个有明确任选表述。asn1-rs-impl 同时属于前两类，故按组件去重计数。

## 证据链与保留方式

[机器清单](dependency-manifest.json) 保留原始 `declaredLicense`，新增 `upstreamSource` 与 `licenseReview`，不把解释后的表达式冒充原 crate 已声明的规范 SPDX。每项从已校验 Cargo.lock SHA-256 的精确 `.crate` archive 取得 `.cargo_vcs_info.json` 和 `Cargo.toml.orig`；这两个原文件保存在各 `upstream-sources/<name>-<version>/crate-evidence/`。再用原 repository、完整 `git.sha1` 与 `path_in_vcs` 定位官方固定 commit 的原始包清单、README 与许可证。17 项的原始包清单均与 archive 的 `Cargo.toml.orig` 字节相同；这不证明整个源码树逐字相同。没有取默认 main 许可证替代指定版本。

下载原文原样保存在 [upstream-sources](upstream-sources/)，每个文件的原路径、固定 commit URL、SHA-256 都登记在机器清单。原先 `licenses/` 的 458 份缓存文本及固定 NetBird 67c2041 的 6 份文本完整保留。9 项新增 12 份许可/版权原文，ring 新增 1 份 fiat 作者清单，组件 `licenseFiles` 合计 471 个引用。其他 README、包清单、来源元数据和源码头作为依据单列，不算许可证文件数量。

## 原先缺独立文本的 9 项

| 组件 | archive 登记的完整发布 commit | 补入的实际官方原文及适用依据 |
| --- | --- | --- |
| asn1-rs-impl 0.2.0 | `a20e5f7319c896737ad0f2557037817b91ad854f` | [LICENSE-MIT](upstream-sources/asn1-rs-impl-0.2.0/LICENSE-MIT)、[LICENSE-APACHE](upstream-sources/asn1-rs-impl-0.2.0/LICENSE-APACHE)；`impl/Cargo.toml` 确认包名/版本/原斜杠字段，根 README 的 License 节明确任选。 |
| boringtun 0.7.1 | `051c9d47dc9c5cb36e461b7d36dcd673820dc98b` | 根 [LICENSE.md](upstream-sources/boringtun-0.7.1/LICENSE.md)，保留 Cloudflare 原版权及 BSD-3-Clause 全文；`boringtun/Cargo.toml` 与根 README 确认适用。 |
| defmt 0.3.100 | `f6decae6e09d1216aff052aa0733a6b5e632a100` | 根 [LICENSE-MIT](upstream-sources/defmt-0.3.100/LICENSE-MIT)、[LICENSE-APACHE](upstream-sources/defmt-0.3.100/LICENSE-APACHE)；`defmt-03/Cargo.toml` 与该目录 README 的 License 节对应兼容包本版本。 |
| defmt-parser 1.0.0 | `4a8cdb44891ed57b8ff5a023b6bec7137c48708f` | 根 [LICENSE-MIT](upstream-sources/defmt-parser-1.0.0/LICENSE-MIT)、[LICENSE-APACHE](upstream-sources/defmt-parser-1.0.0/LICENSE-APACHE)；`parser/README.md` 明确链接这两个根文本，`parser/Cargo.toml` 确认版本。 |
| r-efi 5.3.0 | `97b55bed1c2c91dcbf787674849f05337ff80b33` | [AUTHORS](upstream-sources/r-efi-5.3.0/AUTHORS) 包含 MIT 完整授权/免责、真实版权人与作者清单；根 README 明确 MIT OR Apache-2.0 OR LGPL-2.1-or-later。本候选资料选用 MIT 分支，不将其中 Apache/LGPL 的短声明说成它们的完整许可证全文。 |
| r-efi 6.0.0 | `7e1b0322d31d625f81a5656096330934f9cd835d` | 本版本自己的 [AUTHORS](upstream-sources/r-efi-6.0.0/AUTHORS) 及 README；同上选用 MIT 分支。两个 AUTHORS 的作者清单与哈希不同，分别保留。 |
| tonic-prost 0.14.6 | `6cb6056b5a748bc5a29bd48f4602dbc4e552bb7d` | 根 [LICENSE](upstream-sources/tonic-prost-0.14.6/LICENSE) MIT 全文；`tonic-prost/Cargo.toml` 与项目 README 确认适用。 |
| tonic-prost-build 0.14.6 | `6cb6056b5a748bc5a29bd48f4602dbc4e552bb7d` | 根 [LICENSE](upstream-sources/tonic-prost-build-0.14.6/LICENSE) MIT 全文；该包自己的 Cargo.toml 确认版本与声明。 |
| valuable 0.1.1 | `9efc29b6e58cef28f6566a47aa7e142a55fead77` | 根 [LICENSE](upstream-sources/valuable-0.1.1/LICENSE) MIT 全文；`valuable/Cargo.toml` 与项目 README 确认适用。 |

r-efi 的 AUTHORS 实际也存在于已验证缓存 archive；初轮按 LICENSE/NOTICE/COPYING/COPYRIGHT 文件名筛选漏掉了它。此次同时核对下载文件与 archive 中 AUTHORS 字节，不为作者补写版权。

## 旧斜杠原字段与解释依据

下表的 OR 是本资料基于原文给出的解释，原始字段仍逐字留在 manifest。未替第三方修改元数据，也未替项目整体改变许可策略。

| 组件 | 原字段 | 解析结果与固定版本依据 |
| --- | --- | --- |
| asn1-rs-impl 0.2.0 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[根 README](upstream-sources/asn1-rs-impl-0.2.0/README.md) 162–171 行“Licensed under either of … at your option”。 |
| bitflags 1.3.2 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[src/lib.rs](upstream-sources/bitflags-1.3.2/src/lib.rs) 5–9 行明确“or … at your option”。 |
| curve25519-dalek-derive 0.1.1 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[包 README](upstream-sources/curve25519-dalek-derive-0.1.1/curve25519-dalek-derive/README.md) 185–192 行明确任选，不能套用相邻 curve25519-dalek 的 BSD 标签。 |
| enum-as-inner 0.6.1 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[src/lib.rs](upstream-sources/enum-as-inner-0.6.1/src/lib.rs) 1–6 行明确“or … at your option”，原包 copyright.txt 继续保留。 |
| rusticata-macros 4.1.0 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[README](upstream-sources/rusticata-macros-4.1.0/README.md) 102–111 行明确任选。 |
| version_check 0.9.5 | `MIT/Apache-2.0` | 解释为 `MIT OR Apache-2.0`；[README](upstream-sources/version_check-0.9.5/README.md) 75–80 行明确“either … at your option”。 |
| fnv 1.0.7 | `Apache-2.0 / MIT` | **保留 gap**：固定 `4b4784ebfd3332dc61f0640764d6f1140e03a9ab` 的 [Cargo.toml](upstream-sources/fnv-1.0.7/Cargo.toml)、[README](upstream-sources/fnv-1.0.7/README.md)、[lib.rs](upstream-sources/fnv-1.0.7/lib.rs) 和两份实际许可证已保留，但未找到明确任选表述或明确将该旧字段解释为 OR 的官方依据。不能仅凭有两份文本断言 OR。 |
| minimal-lexical 0.2.1 | `MIT/Apache-2.0` | **保留 gap**：固定 `e997c46656ebe83e696b866bd954da1fa3f64eef` 的 [README](upstream-sources/minimal-lexical-0.2.1/README.md) 98 行及 [LICENSE.md](upstream-sources/minimal-lexical-0.2.1/LICENSE.md) 1–2 行为“dual licensed … as well as”，本轮不足以明确 OR/AND。LICENSE.md 已保留的 Go BSD-3-Clause 原声明继续适用其所述 compact 来源部分，不被 MIT/Apache 字段覆盖。 |

查阅的 [Cargo 0.26 官方旧 manifest 文档](https://raw.githubusercontent.com/rust-lang/cargo/0.26.0/src/doc/src/reference/manifest.md) 只明确旧 `/` 用法已弃用并建议用 AND/OR 消除歧义；该段未明确 `/` 一律代表 OR，故不能据此关闭上述两项。

## ring 0.17.14 的有限来源复核

[包内 LICENSE](licenses/ring-0.17.14/LICENSE) 把新代码指向 [LICENSE-other-bits](licenses/ring-0.17.14/LICENSE-other-bits) 的 ISC，把 BoringSSL 来源指向 [LICENSE-BoringSSL](licenses/ring-0.17.14/LICENSE-BoringSSL)，并说明每个来源文件头才决定 Apache/ISC 适用；once_cell 部分指向其 [Apache](licenses/ring-0.17.14/src/polyfill/once_cell/LICENSE-APACHE) / [MIT](licenses/ring-0.17.14/src/polyfill/once_cell/LICENSE-MIT) 文本。另有 [fiat LICENSE](licenses/ring-0.17.14/third_party/fiat/LICENSE) 的 Apache 声明，引用的 AUTHORS 未装入 archive，此次补入固定 commit 的 [fiat AUTHORS](upstream-sources/ring-0.17.14/third_party/fiat/AUTHORS)，保留真实作者。

已核对以上 6 个原包许可文本都与固定 `2723abbca9e83347d82b056d5b239c6604f786df` 对应文件字节一致；保存了实际 archive 的 [BoringSSL curve25519 来源头](upstream-sources/ring-0.17.14/crate-headers/crypto/curve25519/curve25519.c.txt)、[OpenSSL AES 来源头](upstream-sources/ring-0.17.14/crate-headers/crypto/fipsmodule/aes/asm/aesv8-armx.pl.txt)、[Brian Smith / Google ISC 来源头](upstream-sources/ring-0.17.14/crate-headers/src/aead/chacha.rs.txt) 和 [fiat 生成来源头](upstream-sources/ring-0.17.14/crate-headers/third_party/fiat/curve25519_64.h.txt)。这些精确摘录的源文件 SHA-256 与行范围单列于 manifest。BoringSSL 文本内 Go/Chromium 支持代码说明原样保留，不据此认定这些支持代码已进入本候选二进制。

archive 的 VCS 记录包含 `dirty: true`，所以这里只用 archive 内实际内容及已逐项比对的许可证确认上述资料，不能宣称整个 crate 等于该 git commit。此次关闭原先笼统的混合来源“资料待核”项，仅表示已经完成上述对应文本和代表性来源头复核；全部源码逐行来源、修改历史、链接产物及法律义务仍不在本范围。

## 历史 NetBird snapshot

本地参考仓库不含 `791401060d2b95e5f51e3439c0649729132f571e` 对象，`git cat-file -t` 返回失败；未修改其 remote 或 checkout。改从官方固定 commit raw HTTP 获取 [LICENSE](upstream-sources/netbird-7914010/LICENSE) 与 [AUTHORS](upstream-sources/netbird-7914010/AUTHORS)，完整 URL 和 SHA-256 登记于 `historicalUpstreamSnapshot`。该根 LICENSE 明确排除的是顶层 management/、signal/、relay/、combined/，因此 shared/ 下两份历史 proto 的目录级许可依据已补齐；这不是对全部复制/派生来源或每份 proto 字节的再次核验。67c2041 的既有 git 对象证据继续保留。
