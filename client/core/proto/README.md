<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# client/core/proto — 引入的上游 gRPC 协议文件（N3-2、N4a）

## management.proto

- **来源**：netbirdio/netbird 上游参考 commit
  `791401060d2b95e5f51e3439c0649729132f571e`（main HEAD，拉取登记见仓外
  `~/harmonyos-signing/netbird-n1bdisc/records/upstream-bump-79140106-20260913.json`）。
- **上游路径**：`shared/management/proto/management.proto`
- **复制方式**：以上述 commit 逐字复制（原 sha256
  `9d54ca25ecc65076d8b371bb0c5c1e15e1566ab8eb66d52d50a68889a352e9f5`），
  外加**一处带出处的追加字段**（见下节）；除该处外与 pin 逐字一致。
- **2026-10-13 追加字段**：`PKCEAuthorizationFlowRequest.SessionExtend = 1`
  （含上游注释，逐字取自 commit
  `67c2041`：`shared/management/proto/management.proto:559-570`）。
  pin 7914010 里这个 message 还是空结构（`// … empty struct for future
  expansion`），而会话续期必须让管理端知道这是一次 extend——上游服务端据此强制
  `prompt=login`（`management/internals/shared/grpc/server.go:1174-1207`
  `applySessionExtendFlowPolicy`，调用点 `:1161`），Android 客户端发送该标志
  （`shared/management/client/grpc.go:744-760`，调用方
  `client/android/login.go:200-207`）。不发送该字段 = 静默授权可能来自 IdP 上
  另一个已登录账号，续期会以 peer/user 不匹配失败。字段是**纯追加**（新 tag 1，
  旧服务端忽略未知字段），wire 兼容；本仓 `client/core/src/grpc.rs` 的
  `get_pkce_authorization_flow(session_extend)` 是唯一发送方。
  追加后的文件 sha256：
  `e51eaf4c00d2e6a5136e2472c6c35f740f143b0e58e5a25d2a9f2dfdc2a5d977`。
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
与 `google/protobuf/duration.proto`）；`signalexchange.proto` 只 import
`google/protobuf/descriptor.proto`。这些文件**未复制进仓库**：构建期
codegen（`build.rs`）通过 `protox::Compiler` 内建的
`GoogleFileResolver`（protox 依赖内嵌的 WKT 描述符池）解析，构建不依赖网络、
不依赖 protoc，也不引入多余的仓库内文件。

## signalexchange.proto（N4a）

- **来源**：同上 commit `791401060d2b95e5f51e3439c0649729132f571e`。
- **上游路径**：`shared/signal/proto/signalexchange.proto`
- **复制方式**：逐字复制，未做任何修改（sha256 复制前后一致；
  `9374413168551f7ded6c009dda2bfaa55e111bf138f2fe61780752a05c566ab9`）。
- **许可**：**BSD-3-Clause**（同 management.proto 的目录映射：本文件位于
  `shared/` 下，非顶层 AGPLv3 四目录）。原文件**没有**内嵌版权/许可头
  （文件以 `syntax = "proto3";` 开头），本目录不添加、也不篡改任何头；
  归属在 [THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md) 保留。
- **引入目的**：NetBird signal 服务通道（peer 发现 / ICE 候选交换的传输层），
  `SignalExchange/Send`（unary）+ `SignalExchange/ConnectStream`（bidi 流），
  消息 `EncryptedMessage{key, remoteKey, body}` 与 `Body{type, payload,
  wgListenPort, ...}`。此 proto 是 `src/signal.rs` 的 tonic/prost 代码生成
  输入与协议事实来源；协议时序与逐行号侦察结论见 `docs/n3-signal-notes.md`。

## 重新生成/对账

```bash
# 与仓外上游参考逐字对账（commit 变更时重跑）：
sha256sum proto/management.proto proto/signalexchange.proto \
  ~/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/shared/management/proto/management.proto \
  ~/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/shared/signal/proto/signalexchange.proto
```
