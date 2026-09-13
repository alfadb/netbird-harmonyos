<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# client/core/proto — 引入的上游 gRPC 协议文件（N3-2）

## management.proto

- **来源**：netbirdio/netbird 上游参考 commit
  `791401060d2b95e5f51e3439c0649729132f571e`（main HEAD，拉取登记见仓外
  `~/harmonyos-signing/netbird-n1bdisc/records/upstream-bump-79140106-20260913.json`）。
- **上游路径**：`shared/management/proto/management.proto`
- **复制方式**：逐字复制，未做任何修改（sha256 复制前后一致；
  `9d54ca25ecc65076d8b371bb0c5c1e15e1566ab8eb66d52d50a68889a352e9f5`）。
- **许可**：**BSD-3-Clause**。上游根 LICENSE 声明仅顶层 `management/`、`signal/`、
  `relay/`、`combined/` 四个目录为 AGPLv3；本文件位于 `shared/` 目录下，按登记记录
  的目录映射（REUSE.toml `default_license = "BSD-3-Clause"`；`shared/` 在
  BSD-3 列表内）适用 BSD-3-Clause。原文件**没有**内嵌版权/许可头（文件以
  `syntax = "proto3";` 开头），因此本目录不添加、也不篡改任何头；归属与上游
  AUTHORS 版权声明按 BSD-3 条款在 [THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md)
  保留。
- **引入目的**：NetBird 真实注册/登录走 gRPC `ManagementService/Login`
  （`LoginRequest{setupKey, meta, jwtToken, peerKeys{wgPubKey,...}}`）；上游
  **不存在** REST setup-key 注册端点（N3-1 侦察结论）。此 proto 是
  `src/grpc.rs` 的 tonic/prost 代码生成输入与协议事实来源。

## import 闭包说明

`management.proto` 只 import 两个 well-known type（`google/protobuf/timestamp.proto`
与 `google/protobuf/duration.proto`）。这两个文件**未复制进仓库**：构建期
codegen（`build.rs`）通过 `protox::Compiler` 内建的
`GoogleFileResolver`（protox 依赖内嵌的 WKT 描述符池）解析，构建不依赖网络、
不依赖 protoc，也不引入多余的仓库内文件。

## 重新生成/对账

```bash
# 与仓外上游参考逐字对账（commit 变更时重跑）：
sha256sum proto/management.proto \
  ~/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/shared/management/proto/management.proto
```
