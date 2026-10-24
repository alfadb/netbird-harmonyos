<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# client/core/proto — 引入的上游 gRPC 协议文件（N3-2、N4a）

## management.proto

- **来源**：netbirdio/netbird tag `v0.81.4`，commit
  `7e791d3e2972ddb763bb0e5c3bff4f9e14f70033`（2026-10-24；该 tag 为 annotated
  tag，tag 对象 `2cbeca2bf22ecab597dd96d70ad53e1ed73ac4aa`）。
- **上游路径**：`shared/management/proto/management.proto`
- **复制方式**：与上述 tag 逐字复制，未做任何修改（sha256
  `8916e1256cc32c586f2a8618cd1d2a0dcf79d8f736df3d8ebc45cb32cbb3a207`，
  与 `refs/netbird` 内同路径文件一致）。
- **SessionExtend**：`PKCEAuthorizationFlowRequest.SessionExtend = 1` 由 v0.81.4
  **原生提供**，本目录为该 tag 的逐字副本、sha256 与上游一致。会话续期必须发送
  此标志，上游服务端据此强制 `prompt=login`（`applySessionExtendFlowPolicy`）；
  本仓 `client/core/src/grpc.rs` 的 `get_pkce_authorization_flow(session_extend)`
  是唯一发送方。
- **许可**：**BSD-3-Clause**。上游根 LICENSE 声明仅顶层 `management/`、`signal/`、
  `relay/`、`combined/` 四个目录为 AGPLv3；本文件位于 `shared/` 目录下，不在该
  排除列表内，适用 BSD-3-Clause。原文件**没有**内嵌版权/许可头（文件以
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

- **来源**：同上 tag `v0.81.4` / commit
  `7e791d3e2972ddb763bb0e5c3bff4f9e14f70033`。
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
# 从主仓任意子目录运行，定位主仓根目录后与 refs/netbird 对账：
repo_root="$(git rev-parse --show-toplevel)"
sha256sum "$repo_root/client/core/proto/management.proto" \
  "$repo_root/client/core/proto/signalexchange.proto" \
  "$repo_root/refs/netbird/shared/management/proto/management.proto" \
  "$repo_root/refs/netbird/shared/signal/proto/signalexchange.proto"
```

`refs/netbird` 按主仓 gitlink 固定版本，初始化命令见[根 README](../../../README.md#上游参考仓)。
两个文件均为 v0.81.4 逐字副本，上述四份 sha256 应两两一致；不一致即说明副本被改动。
