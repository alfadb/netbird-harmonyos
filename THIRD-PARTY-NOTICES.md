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

## 归属与许可文本说明

- 各依赖的许可均取自 crates.io 该锁定版本已发布 manifest 的 `license` 字段（`license-file` 未出现）；本清单不转写各许可全文，分发制品时须按各 crate 包内随附的许可文本与版权声明保留归属。
- boringtun 上游为 cloudflare/boringtun（BSD-3-Clause）；其 WireGuard® 名称与商标归 WireGuard 等相应权利人所有。
- 上游 NetBird（netbirdio/netbird）的名称、商标与其客户端侧/服务端代码许可不在本清单范围，见 [README 许可证节](README.md) 与 [安全与合规基线](docs/security-and-compliance.md)。
