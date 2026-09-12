# 许可与源文件头政策（2026-09-13）

自许可证由 MIT 变更为 **AGPL-3.0**（用户 2026-09-13 决定）之日起，本政策适用于本仓库全部新增源文件与上游引入内容。变更背景见 [security-and-compliance.md](security-and-compliance.md)「许可变更（2026-09-13）」小节。

## 新源文件 SPDX 标识

- 每个新增源文件头部必须带：`SPDX-License-Identifier: AGPL-3.0-or-later`，按语言使用对应注释形式：
  - Rust（`*.rs`）：`// SPDX-License-Identifier: AGPL-3.0-or-later`
  - ArkTS / TypeScript（`*.ets`、`*.ts`）：`/* SPDX-License-Identifier: AGPL-3.0-or-later */`
  - Shell 及构建脚本：`# SPDX-License-Identifier: AGPL-3.0-or-later`
- 建议紧邻 SPDX 行附版权行：`Copyright (C) 2026 <版权人>`。

## 上游内容

- 上游 **BSD-3-Clause** 部分（NetBird 客户端侧 / `shared/` 等宽松许可代码）保留其原始许可声明与归属，不得移除、改写或以 AGPL 声明覆盖；引入时逐项登记到 [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md)。
- 上游 AGPL-3.0 服务端组件（`management/`、`signal/`、`relay/`、`combined/`）并入时按 AGPL-3.0 保留原声明，与本项目整体许可一致。

## 上游 bump 机械动作（每次必做，按序）

1. 拉取新上游 commit，记录短哈希与日期。
2. 跑许可扫描（REUSE/SPDX 工具或等效；环境无工具则逐文件映射人工登记）。
3. 扫描结果登记到仓B（`~/harmonyos-signing/netbird-n1bdisc/`）。
4. 在登记中记录本次 bump commit 与扫描工具名称/版本。

## 发布前门槛（公开推送含控制面产物、tag/release、HAP/内测/公开下载均算发布）

- 刷新 [`THIRD-PARTY-NOTICES.md`](../THIRD-PARTY-NOTICES.md) 与 NOTICE：覆盖 Cargo.lock/ohpm/npm 全量依赖与版本，清零"待确认"项。
- 生成 SBOM（客户端制品与并入的上游组件分别成册）。
- 写明 AGPL-3.0 要求的 Corresponding Source 获取方式（随制品分发的源码获取路径说明）。
- 核对目标分发渠道（应用市场等）条款与 AGPL-3.0 义务（源码提供、署名保留）的兼容性并留档。

以上任一项未完成，不得进入该次发布。
