<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# N3-1 Management 协议笔记（客户端骨架）

上游参考 commit：**netbirdio/netbird @ `791401060d2b95e5f51e3439c0649729132f571e`**
（main HEAD，浅克隆拉取 2026-09-12T23:23:43Z，38,784,881 字节，存于仓外
`/home/worker/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`；
许可扫描登记：`仓B/records/upstream-bump-79140106-20260913.json` + `.sha256`）。

## 端点与字段映射（均来自该 commit 的仓库内文件）

| 本模块事实 | 上游来源（文件 @ 上述 commit） |
| --- | --- |
| REST API 挂在 `/api` 前缀下 | `management/server/http/testing/testing_tools/channel/channel.go`（`PathPrefix("/api")`） |
| `GET /api/peers` → PeerBatch 数组（可带 `name`/`ip` 查询过滤） | `shared/management/http/api/openapi.yml` `/api/peers`；`management/server/http/handlers/peers/peers_handler.go` `GetAllPeers` |
| `GET /api/peers/{peerId}` → Peer（本节点管理侧配置） | `openapi.yml` `/api/peers/{peerId}`；同 handler 的 `HandlePeer` |
| `GET /api/users/current` → User（会话校验） | `openapi.yml` `/api/users/current`；`handlers/users/users_handler.go` `getCurrentUser` |
| 认证头：`Authorization: Bearer {jwt}` 或 `Token {pat}` | `management/server/http/middleware/auth_middleware.go` |
| 错误体 `{"message": string, "code": int}`，Content-Type JSON | `shared/management/http/util/util.go`（`ErrorResponse`/`WriteErrorResponse`） |
| Peer 字段 id/name 必填；ip/connected/hostname/version/dns_label 可选 | `openapi.yml` `PeerMinimum`/`Peer`/`PeerBatch` schema |
| 错误分类：401/403→Auth；其余 4xx→Request；5xx→Server | `openapi.yml` 各端点 responses + util.go |

## 注册（setup key）的真相与 TODO(未确认)

- 已核实该 commit **及** release 标签 v0.28.0/v0.35.0/v0.43.1/v0.78.1：`/api/peers`
  只有 GET，上游 **不存在 REST setup-key 注册端点**。
- 真实注册通道是 gRPC `ManagementService/Login`（`shared/management/proto/management.proto`：
  `Login(EncryptedMessage)`，`LoginRequest{setupKey, meta, jwtToken, peerKeys{sshPubKey,wgPubKey}, dnsLabels}`），
  无 `google.api.http` 注解（无 REST 网关）。
- 因此 `management.rs` 的 `register()` 是 **mock-only 契约**（`POST /api/peers`，
  body `{"setup_key":...,"name":...}`，响应按 Peer 解析），全部标注
  `TODO(未确认)`；对真实服务器会得到 404/405 → `Request` 错误。gRPC 注册留待后续增量。

## 超时与重试策略（最小化）

- 每请求一个超时预算（默认 10s），覆盖 TCP connect 与全部读取；DNS 在构造时解析。
- **零自动重试**（注册非幂等），重试决策留给调用方；3xx 不跟随。

## 未确认项清单

1. `register()` 的端点/请求体/响应形态（上游无此 REST 端点，见上）。
2. 注册响应的 HTTP 状态码（200 还是 201；mock 两者都按成功处理）。
3. 真实部署中 management REST 与 gRPC 是否同端口同前缀（本增量只在 mock 上验证）。
4. setup key 注册后"本节点 peer id 如何回填到客户端状态机"（依赖真实注册流程）。

## 本增量未做（明确边界）

- TLS 传输（`https://` 显式拒绝为 `UnsupportedUrl`，下一增量）。
- gRPC/protobuf 管理通道、Signal、Relay、WireGuard 网络映射（Sync）下发。
- 真实 NetBird 服务端联调、真机/adb/hdc 验证。
- 依赖保持不变（仍仅 boringtun；JSON 用 `config.rs` 手写读取器），未动 `Cargo.lock`。
