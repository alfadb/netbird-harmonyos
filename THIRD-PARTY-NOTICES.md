# THIRD-PARTY-NOTICES — 第三方依赖与归属清单

本清单只列本仓库 `client/core`（Rust/NAPI cdylib）实际解析并编译的第三方依赖，数据来源：`client/core/Cargo.toml`（直接依赖）、`client/core/Cargo.lock`（锁定版本）、crates.io API 按锁定版本查询的已发布 manifest `license` 字段。查询日期：2026-09-13。

范围说明：本版列出**直接依赖**及 boringtun 0.7.1 的 normal、非 optional 一层传递依赖（共 18 项，全部许可已确认，无"待确认"项）。dev-dependencies、optional 依赖与更深层传递依赖未在本版逐项列出；按 [发布前门槛](docs/license-and-headers-policy-20260913.md)，公开发布前须刷新本清单为 Cargo.lock 全量闭包（当前锁文件共 86 个包）并生成 SBOM。

## 直接依赖（client/core/Cargo.toml）

| 名称 | 版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| boringtun | 0.7.1（锁定 `=0.7.1`） | BSD-3-Clause | WireGuard 协议的 Rust 实现（Noise 会话与加解密）；本项目网络核心的唯一直接依赖，以 `default-features = false` + `ffi-bindings` 特性使用（TUN `device` 特性按治理条款保持禁用） |

## boringtun 0.7.1 的传递依赖（normal、非 optional，一层）

| 名称 | 锁定版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| aead | 0.5.2 | MIT OR Apache-2.0 | AEAD（认证加密）算法通用 trait |
| base64 | 0.22.1 | MIT OR Apache-2.0 | base64 编解码 |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | BLAKE2 哈希函数 |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT | ChaCha20Poly1305 认证加密（纯 Rust 实现） |
| hex | 0.4.3 | MIT OR Apache-2.0 | 十六进制编解码 |
| hmac | 0.12.1 | MIT OR Apache-2.0 | HMAC 消息认证码通用实现 |
| ip_network | 0.4.1 | BSD-2-Clause | IPv4/IPv6 网络地址结构体 |
| ip_network_table | 0.2.0 | BSD-2-Clause | IPv4/IPv6 网络快速查表 |
| libc | 0.2.189 | MIT OR Apache-2.0 | 平台 libc 的原始 FFI 绑定 |
| nix | 0.31.3 | MIT | *nix 系统 API 的 Rust 友好封装 |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | 更紧凑高效的同步原语（锁） |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | 可移植原子类型（含无原生原子目标支持） |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | 随机数生成器核心 trait |
| ring | 0.17.14 | manifest 字段 `Apache-2.0 AND ISC`；包内为逐目录许可（新代码 ISC；BoringSSL 来源代码 Apache-2.0/ISC；once_cell 来源 MIT/Apache-2.0 等，以包内 `LICENSE*` 文件为准） | 底层密码学原语；发布前刷新时须按包内许可文本逐份核对 |
| tracing | 0.1.44 | MIT | 应用层结构化日志/追踪 |
| untrusted | 0.9.0 | ISC | 不可信输入的安全零 panic 解析 |
| x25519-dalek | 2.0.1 | BSD-3-Clause | X25519 椭圆曲线 Diffie-Hellman 密钥交换（纯 Rust） |

## 引入的上游源文件（N3-2，2026-09-13）

| 文件 | 来源（上游 commit `791401060d2b95e5f51e3439c0649729132f571e`） | 许可 | 引入目的 |
| --- | --- | --- | --- |
| `client/core/proto/management.proto` | netbirdio/netbird `shared/management/proto/management.proto`（逐字复制，sha256 `9d54ca25ecc65076d8b371bb0c5c1e15e1566ab8eb66d52d50a68889a352e9f5` 复制前后一致；原文件无内嵌许可头，归属按 BSD-3 条款保留） | **BSD-3-Clause**（上游根 LICENSE 声明仅顶层 `management/`、`signal/`、`relay/`、`combined/` 为 AGPLv3；本文件在 `shared/` 下，按登记记录 `仓B/records/upstream-bump-79140106-20260913.json` 的目录映射为 BSD-3-Clause） | NetBird 真实注册/登录走 gRPC `ManagementService/Login`（上游无 REST setup-key 注册端点）；本文件是 `client/core` tonic/prost 构建期代码生成的输入（`client/core/build.rs`，protox + tonic-prost-build，免 protoc、不依赖网络）。详见 `client/core/proto/README.md` |

## 直接依赖（2026-09-13 增量，N3-2 gRPC/TLS 栈）

许可取自 cargo registry 内**锁定版本**已发布 manifest 的 `license` 字段（本环境 crates.io
API 不可达；registry 内容即 crates.io 发布清单的本地副本）。

### normal（client/core/Cargo.toml `[dependencies]`）

| 名称 | 锁定版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| tokio | 1.53.1 | MIT | 异步运行时（rt-multi-thread/net/time，gRPC 客户端驱动；`docs/n3-stack-freeze-20260913.md` 冻结项） |
| tonic | 0.14.6 | MIT | gRPC 客户端/服务端框架（transport+codegen+router+tls-ring，default-features 关闭） |
| tonic-prost | 0.14.6 | MIT | tonic 生成代码的 prost 编解码运行时（ProstCodec） |
| prost | 0.14.4 | Apache-2.0 | protobuf 编解码运行时 |
| prost-types | 0.14.4 | Apache-2.0 | protobuf well-known types（management.proto 的 Timestamp/Duration 字段） |
| rustls | 0.23.44 | Apache-2.0 OR ISC OR MIT | TLS（REST `https://` 传输 `TlsHttpTransport`；ring provider，无系统信任根——信任根由调用方注入） |
| crypto_box | 0.9.1 | Apache-2.0 OR MIT | NaCl crypto_box（X25519 + XSalsa20-Poly1305）：管理信封 body 加解密（N3-3，上游 Go `nacl/box` 的 Rust 对应物） |
| base64 | 0.22.1 | Apache-2.0 OR MIT | `ServerKeyResponse.key` / `wgPubKey` 的 base64 编解码（与 boringtun 既有 0.22.1 单副本统一） |

#### N3-3 信封新增传递依赖（crypto_box 0.9.1 闭包，normal）

| 名称 | 锁定版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| crypto_secretbox | 0.1.1 | Apache-2.0 OR MIT | XSalsa20-Poly1305 AEAD 实现（crypto_box 的底层 secretbox） |
| salsa20 | 0.10.2 | MIT OR Apache-2.0 | XSalsa20 流密码 |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT | Poly1305 消息认证码 |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | universal hash trait（poly1305 依赖） |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | 调试不泄密宏（salsa20 依赖） |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | 目标 CPU 特性探测（salsa20 依赖） |

说明：aead 0.5.2 / curve25519-dalek 4.1.3 / x25519-dalek 2.0.1 / zeroize 1.9.0 /
subtle 2.6.1 / getrandom 0.2.17 由 boringtun 0.7.1 已带入（见上节传递依赖表），
crypto_box 与其单副本并存，未引入任何重复 major。

### build（`[build-dependencies]`）

| 名称 | 锁定版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| protox | 0.9.1 | MIT OR Apache-2.0 | 纯 Rust protobuf 编译器（免 protoc；内嵌 well-known types 解析器） |
| tonic-prost-build | 0.14.6 | MIT | tonic/prost 构建期代码生成（`build.rs`） |

### dev（`[dev-dependencies]`，仅测试）

| 名称 | 锁定版本 | 许可 | 用途 |
| --- | --- | --- | --- |
| tokio（macros 特性） | 1.53.1 | MIT | `#[tokio::test]` |
| tokio-stream | 0.1.19 | MIT | 测试内 tonic 服务器的 `TcpListenerStream` |
| rcgen | 0.14.10 | MIT OR Apache-2.0 | 测试自签 CA/证书生成（真实 TLS 握手） |
| prost / prost-types | 同上 | Apache-2.0 | 测试中对 `LoginRequest`/`LoginResponse` 的编解码断言 |

## 归属与许可文本说明

- 各依赖的许可均取自 crates.io 该锁定版本已发布 manifest 的 `license` 字段（`license-file` 未出现）；本清单不转写各许可全文，分发制品时须按各 crate 包内随附的许可文本与版权声明保留归属。
- boringtun 上游为 cloudflare/boringtun（BSD-3-Clause）；其 WireGuard® 名称与商标归 WireGuard 等相应权利人所有。
- 上游 NetBird（netbirdio/netbird）的名称、商标与其客户端侧/服务端代码许可不在本清单范围，见 [README 许可证节](README.md) 与 [安全与合规基线](docs/security-and-compliance.md)。
