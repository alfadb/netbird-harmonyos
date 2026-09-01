# N3 许可法律评估简报（供外部法律意见）

最后核验：2026-08-31 ｜ 状态：`pending-external-legal-opinion`

**这是 N3 的硬前置，agent 不可代办。** 依 [native N1-Nx 治理决议](native-nx-governance.md) §四：在任何 N3 IDL codegen、复制/转译参考实现或协议实现提交之前，须取得**书面、可执行**的专业法律结论——「已委托评估」不满足。结论为义务触发或不确定时立即返回 T0，执行者不得自行判定。

评估完成前的不变项：全路线禁止复制 AGPL 目录代码、禁止以 AGPL 目录源码为实现参考（可观察行为与 `shared/` BSD 侧源码除外）。**在取得书面结论前，不得把「担忧已收窄」写成已合规。**

本文件原存于 `/tmp`（易失），2026-08-31 迁入仓库，字节未变（`sha256:4d03b324d7a8c9a3b6f8a679430a3859f5a5fa31a7a0cd8fd23693198e5c7f6c`）。

## 评估对象
1. NetBird 仓库根 LICENSE（BSD-3-Clause）例外条款的效力：例外仅限顶层 management/、signal/、relay/、combined/ 四目录（AGPLv3）
2. LICENSES/REUSE.toml 与根 LICENSE 的差异：REUSE.toml 仅映射前三目录，不含 combined/（已知差异）
3. 从 shared/ 顶层（按声明为 BSD-3 侧）的 .proto IDL 生成客户端代码（Rust prost 或 C++ protobuf）的义务、署名与 NOTICE 要求
4. 以 shared/relay/（BSD-3 侧）客户端参考实现为 oracle 进行行为兼容再实现的边界
5. 分发形态（华为应用市场 HAP 二进制）下的 BSD-3 归属与 SBOM 义务

## 事实基础（f65f7b34 逐路径核实，2026-08-30）
- 根 LICENSE 例外文本明确列出四个 AGPL 顶层目录
- .proto IDL 位于 shared/management/proto/ 与 shared/signal/proto/（不在例外内）
- management/signal 客户端实现位于 shared/management/ 与 shared/signal/（不在例外内）
- relay 客户端线协议位于 shared/relay/{client,messages,auth/hmac}（不在例外内）
- AGPL relay/ 目录仅含服务端 + 39 字节类型定义
- combined/ 在根 LICENSE 例外文本内但未列入 REUSE.toml（差异点）
- shared/ 顶层所有文件无 SPDX 头

## 问题（待法律意见回答）
Q1: 根 LICENSE 的目录例外条款是否具有法律效力（相对 SPDX/REUSE 标准）？
Q2: combined/ 的 LICENSE 文本 vs REUSE.toml 差异如何解决？以哪个为准？
Q3: 从 BSD-3 侧 .proto 生成代码 → 义务链是什么（署名/NOTICE/开源）？
Q4: 以 BSD-3 侧参考实现为 oracle 做行为兼容再实现 → 是否产生衍生作品风险？
Q5: 本项目（MIT）分发含上述生成代码的 HAP → 需要哪些 NOTICE/SBOM/归属？

## 期望输出
书面法律意见（可直接执行的结论，非"建议咨询律师"），覆盖上述五问。
