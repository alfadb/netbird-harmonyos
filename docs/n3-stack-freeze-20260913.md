<!-- SPDX-License-Identifier: AGPL-3.0-or-later -->
# N3 栈冻结决定：Rust 侧技术栈（2026-09-13）

决定 ID：FREEZE-N3-STACK-20260913。本文件是治理决议 `docs/native-nx-governance.md` **§二.9**
要求的「N3 开门前冻结（语言/栈）」动作的执行与登记。§二.9 未被 2026-09-13 的 §四 修订
（AGPL 路线）取代，仍然现行；本文件不触碰 §四 及任何既有条款。

实测证据：仓外探针 `~/harmonyos-signing/netbird-n1bdisc/refs/stack-probe/`（未进仓库），
真实交叉编译 `cargo build --release --target aarch64-unknown-linux-ohos`，7/7 组合退出码 0；
逐项命令/退出码/产物大小/失败与 workaround 见仓B记录
`~/harmonyos-signing/netbird-n1bdisc/records/n3-stack-probe-20260913.json`
（sha256 `cf24cae125043189c1bb71843072f4de453dee6b094dc4c5f31b45420c4585c3`）。
构建链照抄 `client/core/build.sh`（rustc 1.98.1，SDK26 OHOS clang 链接器）；探针产物未上真机。

## 一、冻结结论（仅冻结实测通过组合）

| 维度 | 冻结选择 | 版本（Cargo.lock 现算） | 实测 |
|---|---|---|---|
| 语言 | Rust（edition 2021，stable 工具链） | rustc 1.98.1 | 7/7 组合 exit 0 |
| 异步运行时 | tokio（rt-multi-thread + net + time + macros） | 1.53.1 | exit 0，1,020,824 B |
| gRPC 框架 | tonic（transport+codegen+router，冻结栈加 tls-ring）+ tonic-prost | 0.14.6 / 0.14.6 | exit 0，2,955,392 B |
| protobuf | prost + prost-types；codegen = tonic-prost-build + protox（免 protoc） | 0.14.4 / 0.14.4；0.14.6 / 0.9.1 | 真实上游 management.proto codegen 通过 |
| TLS 后端 | rustls + ring provider（default-features=false, ring+std+tls12+logging） | 0.23.44 / 0.17.14 | exit 0，1,521,080 B |
| QUIC（relay） | quinn（runtime-tokio + rustls-ring + log + bloom，关默认 platform-verifier/aws-lc-rs） | 0.11.11 | exit 0，2,975,144 B |
| JSON | serde_json | 1.0.151 | exit 0，610,688 B（在线拉取成功） |
| 冻结栈整体 | 上述全部组合编译 | — | exit 0，5,331,392 B，NEEDED 仅 libc.so |

TLS 备选（实测通过但不冻结）：openssl vendored（openssl 0.10.81 + openssl-src 300.6.1+3.6.3）
exit 0，产物 5,003,256 B，静态链入确认；仅作 rustls/ring 失效时的降级路径。
选型核心为 **gRPC over HTTP/2 + TLS**：上游真实注册走 gRPC `ManagementService/Login`
（management.proto 实证），REST 仅覆盖只读查询端点，不构成替代信号。

## 二、外部事实来源（URL + 访问日期，§二.9 要求）

- `aarch64-unknown-linux-ohos` = **Tier 2 (with Host Tools)**，rustup 直发产物、链接器 wrapper 配置：
  <https://doc.rust-lang.org/rustc/platform-support/openharmony.html>（访问 2026-09-13）
- 同页 Host toolchain 节：ohos 目标 cargo 生态需 ohos-openssl
  （<https://github.com/ohos-rs/ohos-openssl>）——openssl 路线维护成本佐证（访问 2026-09-13）
- 版本与发布日期（max_stable_version / updated_at，crates.io API 实取）：
  <https://crates.io/crates/tonic>（0.14.6，2026-05-07）、/tokio（1.53.1，2026-07-20）、
  /prost（0.14.4，2026-06-07）、/quinn（0.11.11，2026-06-22）、/rustls（0.23.44，2026-09-07）、
  /ring（0.17.14，2025-03-11）、/openssl（0.10.81）、/serde_json（1.0.151，2026-07-20）、
  /protox（0.9.1）、/tonic-prost-build（0.14.6）（均访问 2026-09-13）
- feature 名实取（防臆造）：tonic 0.14.6 含 `tls-ring`、无 `prost` 特性（prost 集成已拆分）；
  quinn 0.11.11 默认含 `platform-verifier`+aws-lc-rs 线；rustls 0.23.44 默认 provider 为 aws-lc-rs：
  <https://crates.io/api/v1/crates/tonic/0.14.6> 等同 API（访问 2026-09-13）
- 上游注册走 gRPC（非 REST）为本会话 N3-1 侦察硬结论，IDL 实证：
  `refs/netbird-791401060d2b/shared/management/proto/management.proto`（仓B本地快照，2026-09-13 引用）

## 三、备选方案与否决理由

- **openssl(+openssl-sys) 为 TLS 主线**：否决（实测可编译但产物 +2.0 MB 静态 libcrypto，且官方指明
  ohos cargo 生态需维护 ohos-openssl 分支；rustls/ring 一条纯 Rust 路已实测通过）。保留为降级路径。
- **aws-lc-rs（rustls 默认 provider / quinn 默认线）**：否决——本探针未实测，且其 C 构建依赖
  cmake/NASM/Nix 工具链，跨编译面大；ring 路通过即冻结 ring。若未来 ring 停更再评估。
- **REST-only（弃 gRPC）**：否决——上游无 REST setup-key 注册端点，注册必须 gRPC `Login`。
- **若 tonic 在 ohos 不可用（触发式降级，登记如下）**：a) hyper 1.x + h2 自研 HTTP/2 gRPC 帧 +
  prost 编解码（tonic 失效时最小保真路径）；b) quinn-only（relay 走 QUIC、management 走自研 H2）；
  c) 全自研等价于「门范围变化」，按 §二.10 回 T0，不得自行切换。
- **Go 侧栈**：否决——E1/G0 既有判定（stock Go c-shared IE-TLS 阻塞），E1 dormant，不因栈冻结重开。

## 四、风险与未知项（编译通过 ≠ 运行通过）

- **运行期行为未验证**：tokio/mio 在 ohos musl 上的 epoll/timer/DNS（`getaddrinfo` 阻塞解析）
  实际行为、 tonic/rustls/quinn 真实连接、TLS 握手与 ALPN——全部属 N3 门证据，本冻结不外推。
- **证书信任根来源未定**：rustls 不读系统 store；webpki-roots 内嵌 or OHOS 证书目录加载，待 N3 取证。
- **体积影响**：冻结栈 bin 5,331,392 B（未 strip/opt 调优，且为 bin 非 cdylib）；并入 client/core
  前需 strip + 依赖裁剪（axum 为 tonic transport 依赖带入）评估。
- **并入依赖树代价**：ring/libc 与 client/core 完全同版（0.17.14/0.2.189，无冲突）；但
  getrandom 0.2/0.4、rand_core 0.6/0.10 等将多 major 并存（cargo 允许，体积/审计面增大）——只报告，未改仓库。
- **protox 与上游 proto 演进**：上游 management.proto 变更需重跑 codegen 探针；protox 对新语法
  的覆盖以再测为准。

## 五、与 §二.9 的关系

本文件即 §二.9 要求的「N3 开门前冻结」动作本身：冻结的是**编译层选型**，依据为上文真实交叉编译
证据；「冻结」不是「门 pass」——N3 各门（N3a/N3b…）的判据、oracle、物理证据义务不受影响，
仍按治理骨架逐门验证。栈选型的后续变更（含降级路径触发）涉及门范围/顺序/阈值的，按 §二.10 回 T0。
