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

# N3-3 增量更新（2026-09-13）：信封 NaCl 加密层（GetServerKey + crypto_box）

解决 N3-2 遗留的互通缺口：body 加密。上游事实全部重新取证自仓外参考树
`~/harmonyos-signing/netbird-n1bdisc/refs/netbird-791401060d2b/`（commit
`791401060d2b95e5f51e3439c0649729132f571e` 不变）。

## 信封真实形态（上游逐字实证）

### 流程时序（`shared/management/client/grpc.go` `login()` L585-637）

```
客户端                                          管理服务端
  │ 1. GetServerKey(Empty) ──────────────────────▶ │
  │ ◀────────────── ServerKeyResponse{key(b64)} ── │  (L536-549)
  │ 2. seal(LoginRequest)                          │
  │    box(peer=serverKey, priv=c.key)             │
  │    nonce=crypto/rand 24B                       │
  │ 3. Login(EncryptedMessage{                     │
  │      wgPubKey: c.key.PublicKey().String(),     │  (L603-612)
  │      body: nonce||ct, version: 0 }) ─────────▶ │
  │ ◀──────────── EncryptedMessage{body=nonce||ct} │
  │ 4. open(body, peer=serverKey, priv=c.key)      │  (L627-631)
  │    → LoginResponse                             │
```

每条消息（含响应）各自取新随机 nonce；`serverKey` 一次获取、本次 Login 内复用
（L587 取一次，L595 seal 与 L627-631 open 共用）。上游 `GetServerKey` 自带 5s
context 超时（L538）；`ServerKeyResponse.expiresAt`（proto L317-319）客户端**未读**。

### 密码方案：确为 NaCl crypto_box（派发稿假设成立）

| 事实 | 出处（upstream commit 内行号） |
| --- | --- |
| 算法 = Curve25519 + XSalsa20 + Poly1305（= NaCl box；Go `golang.org/x/crypto/nacl/box`） | `encryption/encryption.go` L7、L13-15 |
| `Encrypt()`: `box.Seal(nonce[:], msg, nonce, peerPub, priv)` —— **nonce 前置**在密文前，被加密的只有 msg | encryption.go L18-24 |
| `Decrypt()`: 前 24B 为 nonce，其余为 ct；长度 <24 报 "invalid encrypted message length"；open 失败报解密失败 | encryption.go L11、L27-42 |
| nonce = 每消息 24 字节 crypto/rand 随机数（非计数器、无方向序列） | encryption.go L44-51 |
| 无 AAD、无压缩；protobuf bytes 进出 | encryption/message.go L76-106 |
| 服务端公钥来自 `GetServerKey(Empty) → ServerKeyResponse`；`key: string` = 32B WG 公钥的 base64（`wgtypes.ParseKey` = std encoding） | 仓内 `client/core/proto/management.proto` L24、L314-320；grpc.go L536-549 |
| **密钥来源 = WireGuard 私钥复用，非临时密钥对**：`config.PrivateKey`（建档时 `wgtypes.GeneratePrivateKey()` 一次，持久化）→ `connect.go:243 wgtypes.ParseKey` → `connect.go:313 mgm.NewClient(..., myPrivateKey, ...)` → `grpc.go:128 NewClient(ctx, addr, ourPrivateKey wgtypes.Key, ...)`；同一把密钥也用于 WG 握手与 signal | client/internal/connect.go L243/L313、client/internal/profilemanager/config.go L374-376/L917-919、shared/management/client/grpc.go L128 |
| 信封 `wgPubKey` 与 `PeerKeys.wgPubKey`（base64 字符串的**字节**）都取自 `c.key`，即客户端自己的公钥；Login 不设置 version | grpc.go L608、L641-647 |

## 本增量实现（client/core）

- **依赖**：`crypto_box = "0.9.1"`（RustCrypto，纯 Rust：X25519 +
  XSalsa20-Poly1305）+ `base64 = "0.22"`（与锁内既有 0.22.1 统一，零新 major）。
  crypto_box 新增传递闭包：crypto_secretbox 0.1.1、salsa20 0.10.2、poly1305 0.8.0、
  universal-hash 0.5.1、opaque-debug 0.3.1、cpufeatures 0.2.17；aead 0.5.2 /
  curve25519-dalek 4.1.3 / x25519-dalek 2.0.1 / zeroize / subtle / getrandom 0.2.17
  与 boringtun 既有版本**单副本并存**，无重复 major，代价仅为体积与编译时间。
  交叉编译先在仓外探针验证：`refs/nacl-probe/`，
  `cargo build --release --target aarch64-unknown-linux-ohos` exit 0（探针同时
  修出并验证了 `encrypt_in_place` 与 Go `box.Seal` 前缀语义的差异陷阱）。
- **`src/envelope.rs`（新）**：
  - `EnvelopeKeyPair`：客户端身份密钥对，`from_secret_bytes(&[u8;32])`（喂 WG 私钥，
    上游形态）或 `generate()`（新建档）；`public_key_base64()` 即 `wgPubKey`。
    单元测试断言其公钥与 boringtun 冻结 `x25519_public_key` 导出一致（RFC 7748 §6.1
    向量 + 交叉推导）——"WG 密钥即信封密钥"的可行性由同一把 Curve25519 密钥保证。
  - `EnvelopePublicKey`：远端公钥（客户端视角=服务端公钥），`from_base64` 解析
    `ServerKeyResponse.key`。
  - `seal(peer, keys, plaintext) -> nonce(24B)||ct`、`open(peer, keys, wire)`：
    与 Go `box.Seal/Open` 字节兼容；nonce 用 aead `OsRng`（rand_core/getrandom
    0.2.17，已在 ohos 目标编译树内）每消息随机。
- **`src/grpc.rs` 接入**：`connect()` 新增第 5 参 `envelope_keys`（上游
  `NewClient(ourPrivateKey)` 的对应物）；`login()` 默认走加密路径
  （`get_server_key()` → `seal` → `Login` → `open` → decode），`serverKey` 一次
  获取本次复用；`LoginParams.peer_keys.wg_pub_key` 在加密/明文路径都会被客户端
  身份密钥覆盖（上游单源 `c.key`，测试用哨兵值证明覆盖生效）。`GetServerKey`
  独立公开为 `get_server_key()`。
- **错误分类：复用六类，不新增**：GetServerKey 状态走 `map_grpc_status`
  （5xx→Server、Unavailable→Network、DeadlineExceeded/自管超时→Timeout）；key
  串非 base64/非 32B 与 open 失败（篡改/错钥）→ `Parse`（响应非所要求的形状/
  无法认证，语义与既有 Parse 一致）；本地 seal 失败 → `Request{status:0}`
  （既有本地预检约定，实际不可达）。新增类别会波及 NAPI 面而无行动增益。

## 明文模式的边界声明

`LoginBodyMode::Plaintext`（`login_with_mode` 显式传入）发送**原始序列化
LoginRequest** —— **非上游行为**，上游不存在该模式。仅供对非 NetBird mock
服务器的测试/调试；对真实服务端，明文 body 会被拒绝或语义未定义。
默认值 `LoginBodyMode::Encrypted` 才是唯一具备互通能力的路径；测试
`login_default_mode_is_encrypted_not_plaintext` 断言默认路径发出的 body
在明文契约服务器上确实无法按 LoginRequest 解码（即真的加密了）。

## 本增量仍未做（边界）

1. `Sync` 流、`Logout`、`GetDeviceAuthorizationFlow`/`GetPKCEAuthorizationFlow`
   （SSO JWT 真实来源）、`ExtendAuthSession`：信封原语已备（`seal`/`open` 与
   RPC 无关），但各自 RPC 未接。
2. 重连/backoff：零自动重试（策略同 N3-1/N3-2；上游 login 内仅对 Canceled
   backoff 重试，grpc.go L613-633，未复刻）。
3. 信任根来源（webpki-roots 内嵌 vs OHOS 证书目录）仍未取证（N3-2 遗留风险项）。
4. 未上真机：ohos 目标仅冻结栈交叉编译通过；envelope/grpc 均为 host 实测。
5. `ServerKeyResponse.expiresAt` 未消费（上游客户端同样不读，无互通差异）。
