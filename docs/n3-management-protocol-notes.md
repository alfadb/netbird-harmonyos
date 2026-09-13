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

---

# N3-2 增量更新（2026-09-13）：gRPC 通道 + `Login` + TLS 传输

同一上游参考 commit（`791401060d2b95e5f51e3439c0649729132f571e`）不变的补充侦察与实现事实。

## 新增协议事实（均为该 commit 仓库内实证）

| 事实 | 来源 |
| --- | --- |
| `Login(EncryptedMessage) → EncryptedMessage`，请求体为 `LoginRequest{setupKey(1), meta(2, PeerSystemMeta), jwtToken(3), peerKeys(4, PeerKeys{sshPubKey(1), wgPubKey(2)}), dnsLabels(5)}` | `shared/management/proto/management.proto` L33-36/L175-186/L190-196（已逐字引入 `client/core/proto/`，BSD-3-Clause，见 proto/README.md） |
| `LoginResponse{netbirdConfig(1), peerConfig(2), Checks(3), sessionExpiresAt(4)}` —— **响应不含 JWT**；"会话信息" = `sessionExpiresAt` 三态（unset→无信息 / 置零→显式关闭过期 / 有效值→绝对期限）+ `PeerConfig.address`（分配的 VPN IP） | management.proto L280-293（注释 L288-291 明确三态编码）、L404-426 |
| 信封：`EncryptedMessage{wgPubKey(string)=peer WG 公钥的 base64 字符串, body, version}`；`PeerKeys.wgPubKey(bytes)` 装的是**该 base64 字符串的字节**（`[]byte(c.key.PublicKey().String())`）；Login 调用不设置 version | `shared/management/client/grpc.go` `login()`/`Register()`（L595-612、L641-647） |
| 上游在 Login 前先 `GetServerKey` 取服务端 WG 公钥，再用 NaCl box 加解密**信封 body**（`encryption.EncryptMessage(serverKey, c.key, req)`） | `shared/management/client/grpc.go` `login()` L538/L595/L607/L629 |
| 无效 setup key → 服务端返回 `PermissionDenied`（proto 注释：Login 在 PermissionDenied 时可用 setupKey 注册） | management.proto L33-35 |

## 本增量实现（client/core）

- `src/grpc.rs`：`ManagementGrpcClient`（tonic Channel + 生成 stub），
  `login(LoginParams) -> LoginOutcome{response, session_deadline_unix, peer_address}`；
  gRPC 状态码 → 既有六类错误映射（`map_grpc_status`，Auth=Unauthenticated/PermissionDenied，
  Request=InvalidArgument 等 8 码，Server=Internal/Unknown/DataLoss/ResourceExhausted，
  Network=Unavailable，Timeout=DeadlineExceeded/自管超时）。
- `src/management.rs`：`TlsHttpTransport`（rustls/ring 阻塞 StreamOwned）+
  `parse_base_url_tls` + `ManagementClient::new_tls`；`parse_base_url` 保持只收
  `http://`（mock 基线契约不变）。TLS 信任根一律**调用方注入**（PEM/DER CA），
  不读任何系统 store（冻结文档风险项）。
- `build.rs`：protox（免 protoc，WKT 走内嵌 GoogleFileResolver）→
  tonic-prost-build `compile_fds`；生成 `$OUT_DIR/management.rs`（消息+客户端+服务端
  stub 同文件；server stub 供 in-process 测试用，generate_default_stubs=true）。
- 错误分类沿用六类，未新增类别。

## 明确边界（本增量仍未做）

1. **信封 body 的 NaCl 加密层未实现**（冻结栈无 crypto_box；`GetServerKey` 未接）。
   本增量发送的是序列化 `LoginRequest` 明文 body——gRPC/TLS/stub/字段映射为真，
   对真实 NetBird 服务端互通仍差这一层（测试服务器按明文 body 契约实现）。
2. `Sync` 网络映射流、`Logout`、`GetDeviceAuthorizationFlow`/`GetPKCEAuthorizationFlow`
   （SSO JWT 的真实来源）未实现；REST `/api/peers` 的 JWT/PAT 由调用方持有。
3. 重连/backoff：上游用 backoff.Retry（仅 Canceled 重试），本增量零自动重试，策略同 N3-1。
4. 证书信任根来源（webpki-roots 内嵌 vs OHOS 证书目录）仍未取证（冻结文档风险项）。
5. gRPC 通道运行期行为（tokio/mio epoll、tonic 真实连接、ALPN）仅在 host 实测，
   ohos 目标为冻结栈编译通过（`docs/n3-stack-freeze-20260913.md`），未上真机。
6. 服务端 stub 会进入发布 cdylib（供测试；dead code 不剔除）——体积裁剪候选：
   codegen 拆分 client/server，或 `PROFILE` 条件 codegen；待体积评估增量处理。
