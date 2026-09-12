# N3 法律评估委托材料包（事实汇编 + 问题清单）

日期：2026-09-12 ｜ 仓库基线：main @ `52405eb`（`52405eb7ffc1244132776052f4bcd87690be2a2c`）｜ 状态：待外部律师受理

**本文件性质**：这是 `netbird-harmonyos` 项目为取得**书面、可执行**外部法律结论而准备的委托材料包——只汇编事实与问题清单。本文件以及本仓库任何文档**均不构成法律意见**，不得被解读为任何许可证认定或合规结论；所有许可问题以受托律师的书面结论为准。

## 1. 背景与用途

- **项目是什么**：`netbird-harmonyos` 是一个**独立开发、非官方维护**的 NetBird 客户端移植项目，目标平台为 HarmonyOS（首目标候选 API 24）与具名 OpenHarmony 发行版。本仓库以 MIT 许可证发布（根 `LICENSE`，Copyright (c) 2026 alfadb），当前处于早期研究与门控验证阶段，尚无任何发行版本（README：「当前仓库尚未提供可用发行版本」）。
- **这份文件给谁**：直接供受托外部执业律师/法律顾问阅读与核验；同时作为项目治理门 N3 的前置材料在仓内存档。
- **治理依据**：`docs/native-nx-governance.md` §四规定 N3 硬前置（原文见 §2 引文）。配套简版见 `docs/n3-legal-brief.md`；本材料包是其展开版。

## 2. 需要结论的 5 个问题

治理文档 §四 原文（逐字，作为全部问题的统一上下文；来源：`docs/native-nx-governance.md` 第 59–64 行，main @ `52405eb`）：

> **N3 硬前置**：在任何 N3 IDL codegen、复制/转译参考实现或协议实现提交之前，取得**书面、可执行**的专业法律结论（"已委托评估"不满足）。评估对象：
>
> 1. `shared/` BSD-3 声明映射的效力（.proto IDL 与客户端参考实现按仓库声明位于 BSD-3 侧；AGPL 例外仅顶层 `management/`、`signal/`、`relay/`、`combined/` 服务端目录）；
> 2. 根 LICENSE 例外文本与 `LICENSES/REUSE.toml` 映射冲突时的优先级（`combined/` 差异即此类）；
> 3. 从 BSD-3 侧 .proto 与参考实现派生代码的义务、署名与 NOTICE 要求；
> 4. 以 `shared/relay` 为 oracle 的再实现边界；分发形态（应用市场/HAP）下的归属与 SBOM 义务。

以下五问为请求律师书面回答的中性表述；每问逐字附治理文档原始条目。所涉上游目录、文件与许可证标注均为**待贵所核验的仓内记录**（见 §3），本文件不对其作任何认定。

### 问题一（a）：`shared/` 目录许可归属的认定

**向律师提出**：请依据上游 NetBird 仓库固定 commit `f65f7b34…` 的实际文件，核验并认定：`shared/` 目录（含 `shared/management/proto/`、`shared/signal/proto/` 下的 `.proto` IDL 文件，以及 `shared/management/`、`shared/signal/`、`shared/relay/` 下的客户端参考实现文件）在根 LICENSE 目录例外条款与 `LICENSES/REUSE.toml` 映射下的许可证归属如何认定？该认定的文本依据是什么？`.proto` IDL 与客户端参考实现相对于根 LICENSE 例外条款所列四个服务端目录，是否适用不同的许可证条款？

**治理文档原始条目（逐字）**：

> 1. `shared/` BSD-3 声明映射的效力（.proto IDL 与客户端参考实现按仓库声明位于 BSD-3 侧；AGPL 例外仅顶层 `management/`、`signal/`、`relay/`、`combined/` 服务端目录）；

### 问题二（b）：根 LICENSE 例外文本与 REUSE 映射冲突的优先级

**向律师提出**：仓内核验记录显示，上游根 LICENSE 的例外文本与其 `LICENSES/REUSE.toml` 映射之间存在一处不一致（`combined/` 在根 LICENSE 例外文本内但未列入该映射；详见 §3.6 第 6 条）。请认定：两类文本冲突时以哪个为准？优先级认定的依据（许可文本、REUSE 规范、其他）是什么？该认定方法是否同样适用于其他潜在不一致情形？

**治理文档原始条目（逐字）**：

> 2. 根 LICENSE 例外文本与 `LICENSES/REUSE.toml` 映射冲突时的优先级（`combined/` 差异即此类）；

### 问题三（c）：从 BSD-3 侧 .proto 与参考实现派生代码的义务

**向律师提出**：若本项目（i）依据上游 `shared/` 侧 `.proto` IDL 文件生成客户端代码（候选工具链：Rust prost 或 C++ protobuf），或（ii）参考、复制或转译上游 `shared/` 侧客户端参考实现编写代码——上述行为分别产生哪些许可义务（版权与许可声明保留、NOTICE、来源披露、其他）？在本项目以 MIT 许可证发布、并将来以 HAP 二进制分发的背景下，这些义务应如何履行、在哪些载体上履行？

**治理文档原始条目（逐字）**：

> 3. 从 BSD-3 侧 .proto 与参考实现派生代码的义务、署名与 NOTICE 要求；

### 问题四（d）：以 `shared/relay` 作为行为 oracle 的再实现边界

**向律师提出**：本项目拟以可观察网络行为与上游 `shared/relay` 客户端参考实现作为行为兼容的参考（oracle）进行**独立再实现**。请认定：对参考实现源码的阅读、参照、转译分别到何种程度会产生衍生作品关系或归属义务？"仅依据可观察线协议行为实现"与"阅读源码后实现"两类路径的边界如何认定？（本项目的内部治理规则另在全路线禁止以 AGPL 目录源码为实现参考，见 §4；该内部规则本身不在本问范围内。）

**治理文档原始条目（逐字）**：

> 4. 以 `shared/relay` 为 oracle 的再实现边界；分发形态（应用市场/HAP）下的归属与 SBOM 义务。

（本问对应上条前半句；后半句见问题五。）

### 问题五（e）：应用市场/HAP 分发形态下的归属与 SBOM 义务

**向律师提出**：在华为应用市场以 HAP/App Pack（含 native 库）形态分发本客户端时：上游许可（含按问题一认定的 `shared/` 侧许可，以及其他第三方组件，见 §3.4 与 §6 第 9 条）对归属（attribution）展示、NOTICE 文件、SBOM 的内容与提供方式分别有什么要求？对尚未分发的开发阶段制品是否有不同要求？

**治理文档原始条目（逐字）**：

> 4. 以 `shared/relay` 为 oracle 的再实现边界；分发形态（应用市场/HAP）下的归属与 SBOM 义务。

（本问对应上条后半句。）

## 3. 事实材料索引（供律师核验）

以下仅汇编仓内既有记录与仓库状态；所有上游文件内容以律师对上游固定 commit 的直接核验为准。

### 3.1 本项目概况

| 事实 | 内容 | 仓内出处 |
| --- | --- | --- |
| 许可证 | MIT（Copyright (c) 2026 alfadb） | 根 `LICENSE`；README「本项目以 MIT 许可证发布」 |
| 身份 | 独立开发、非官方维护的 NetBird 客户端项目 | README 开头 |
| 商标立场 | 「NetBird 名称及商标归其各自权利人所有。NetBird 上游项目以及本项目引用或衍生的上游代码遵循各自的许可证；本项目的 MIT 许可证不改变这些许可条款。」 | README「许可证」节 |
| 仓库状态 | 研究文档 + 探针 spike 为主，无发行版本 | README；`docs/security-and-compliance.md`「当前实测」 |
| git 远端 | `https://github.com/alfadb/netbird-harmonyos.git` | `git remote -v` |

### 3.2 上游 NetBird 版本与 commit

- **当前正式基线**：NetBird `v0.76.3`，固定 commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`；声明 `go 1.25.5` / `toolchain go1.25.12`；官方 Release run `31256677326` 成功，固定 URL 访问日期 2026-08-09（`docs/security-and-compliance.md`「许可证与商标基线」）。
- **历史基线**（保持原版本绑定，不作为当前结论输入）：`v0.74.6` commit `3a2f773d655d88d16ed953fc2a114a4e690a1b08`（同文件载明；`v0.74.7` 分析同为历史绑定）。
- **go.mod replace 指向的上游 fork**：`netbirdio/wireguard-go @ 2834bebf6c1aea76bd217f31ea91c99f75e4a20a`、`netbirdio/ice/v4 v4.0.0-20250908184934-6202be846b51`（`docs/n0-native-client-feasibility.md`）。该文档将其标注为 MIT，并同时声明「法律效果待专业评估，本矩阵不下硬门结论」。

### 3.3 本仓库对上游文件的引用现状

- 本仓库**不含**上游 NetBird 源码副本：无 submodule（无 `.gitmodules`）；`git ls-files` 中无上游 `.proto`/`.go` 文件（仅有的两个 `.go` 文件为本项目自研探针，位于 `spikes/`）。
- 本仓库**不存在** `LICENSES/` 目录或 `REUSE.toml`；治理文档与 n0 矩阵所称 `LICENSES/REUSE.toml` 均指**上游仓库**固定 commit 下的该文件（如 `docs/tailscale-ohos-netbird-port-audit.md` 第 463 行给出其上游 URL）。
- 上游引用方式为「权威源码路径（固定 commit `f65f7b34`）+ 行为要点 + 许可边界 + 是否实现」的矩阵记录（`docs/n0-native-client-feasibility.md` §矩阵），未 vendored 任何上游文件。
- 仓内唯一对上游协议面的系统性映射文档：`docs/tailscale-ohos-netbird-port-audit.md`（Tailscale-OHOS 数据通路审计与 NetBird 映射，含对上游 `LICENSES/REUSE.toml` 的链接引用）。

### 3.4 已实现与未实现清单（截至 main @ `52405eb`）

- **控制面 = 零代码**：management / signal / relay / ICE / conn state / routes / DNS / ACL / state 九个面在 N0 矩阵中全部为「是否实现 = **否**」（`docs/n0-native-client-feasibility.md` §「N0 是否实现」：「矩阵中**全部为「否」**」）。
- **数据面 = 自研 native 实现**：N0 唯一代码面为单一 native WireGuard core 的 C ABI（BoringTun `0.7.1`，`ffi-bindings` only，14 个 C ABI 符号，C ABI smoke oracle；`spikes/n0-native-core`）；另有 N1a 自研 Rust 数据泵 spike（`spikes/n1a-native-dataplane`，双 BoringTun 隧道 + 127.0.0.1 UDP 回环泵，未涉及 management/ICE/relay/UI/VPN/TUN/protect 面）。两个 spike 的 `Cargo.toml` 均自声明 `license = "MIT"`；直接第三方依赖为 `boringtun 0.7.1` 与（仅 n1a）`libc 0.2`，传递依赖由 `Cargo.lock` 锁定。
- **未发生**：未复制任何上游目录代码；未生成任何协议实现代码；N3 尚未开启——本次书面法律结论即其硬前置。

### 3.5 分发形态

- 当前**无发行版本**（README）。目标分发形态为华为应用市场 HAP/App Pack（含 native 库；评估中）。
- README 载明长期目标覆盖 HarmonyOS 与具名 OpenHarmony 发行版**双目标**，并分别维护应用壳、构建签名、制品与分发流程；具体分发渠道尚未锁定（影响问题五范围，见 §6 第 6 条）。

### 3.6 仓内核验记录索引（供律师对照上游固定 commit `f65f7b34` 逐条核验）

以下 7 条为**本项目自行核验**的记录（`docs/n3-legal-brief.md`「事实基础」，标注 f65f7b34 逐路径核实，2026-08-30；未经独立或专业复核）：

1. 根 LICENSE 例外文本明确列出四个 AGPL 顶层目录；
2. `.proto` IDL 位于 `shared/management/proto/` 与 `shared/signal/proto/`（不在例外内）；
3. management/signal 客户端实现位于 `shared/management/` 与 `shared/signal/`（不在例外内）；
4. relay 客户端线协议位于 `shared/relay/{client,messages,auth/hmac}`（不在例外内）；
5. AGPL `relay/` 目录仅含服务端 + 39 字节类型定义；
6. `combined/` 在根 LICENSE 例外文本内但未列入 `REUSE.toml`（仓内记录为已知差异；`docs/t0-native-nx-gates-materials.md` 第 69 行同）；
7. `shared/` 顶层所有文件无 SPDX 头。

协议/许可矩阵原文（逐面「是否实现 = 否」及其许可边界标注）：`docs/n0-native-client-feasibility.md` §矩阵（第 116–138 行）；其「许可边界说明」明确「**法律效果待专业评估，本矩阵不下硬门结论**」。安全与许可基线全貌另见 `docs/security-and-compliance.md`「许可证与商标基线」节（其自述为「锁定实现和生成 SBOM 前的初始识别，不替代逐文件、依赖树、构建产物和部署方式审查」）。

## 4. 红线条款（评估完成前的不变项）

`docs/native-nx-governance.md` §四 原文（逐字）：

> 评估完成前的不变项：全路线禁止复制 AGPL 目录代码、禁止以 AGPL 目录源码为实现参考（可观察行为与 `shared/` BSD 侧源码除外）；结论为义务触发或不确定时立即返回 T0，执行者不得自行判定。净室不作默认，仅评估结论要求时启用。在取得书面结论前，不得把"担忧已收窄"写成已合规。

对律师的说明（内部流程转述，非法律表述）：在收到贵所书面结论之前，本项目任何路线不复制 AGPL 目录代码、不以其为实现参考，亦不进行 N3 IDL codegen、参考实现复制/转译或协议实现提交；若结论为"义务触发"或"不确定"，本项目将立即返回治理层（T0）重议，一线执行者无权自行判定。因此结论的**可判定性**直接影响项目能否开工。

## 5. 给律师的交付要求

1. **书面、可执行**：需出具覆盖 §2 五问的书面专业法律结论，且结论可直接执行——"建议咨询律师"式回复或"已委托评估"状态**不满足**本项目治理门要求（治理文档 §四逐字要求见 §2 引文；`docs/n3-legal-brief.md`「期望输出」同：「书面法律意见（可直接执行的结论，非"建议咨询律师"），覆盖上述五问」）。
2. **逐行为给出允许/禁止/附条件**：至少覆盖四类行为——(i) 依据 BSD-3 侧 `.proto` 进行 IDL codegen；(ii) 复制/转译 BSD-3 侧参考实现；(iii) 以 `shared/relay` 为 oracle 的再实现；(iv) HAP 分发及对应 NOTICE/SBOM/归属履行。附条件结论须写明条件内容与履行载体（源码仓库、制品包、文档、市场页面等）。
3. **结论须能据以决策**：本项目须据结论决定是否启动 N3 IDL codegen 与协议实现；请对每一问给出明确、可判定的表述（允许 / 禁止 / 附条件及条件），避免留待项目自行解释的措辞。
4. **注明依据版本**：结论应指明所核验的上游文件版本（固定 commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6`）与所依据的文本；如核验结果与 §3.6 仓内核验记录不符，请指出差异。

## 6. 待补事实（本仓库无法确认，需用户或律师补充）

以下事项在本仓库内无法确认，逐条列出待补；本文件不对其作任何推测：

1. 上游根 LICENSE 例外条款与 `LICENSES/REUSE.toml` 的**逐字文本**：本仓库无副本（不存在 `LICENSES/` 目录）；仓内仅有转述与 §3.6 的逐路径核验记录。委托前是否固定上游快照（或由律师直接核验上游仓库）待用户决定。
2. `combined/` 目录下是否存在独立的 LICENSE/NOTICE 文件及其文本内容：仓内文档仅记录「combined/ 在根 LICENSE 例外文本内但未列入 REUSE.toml」这一差异，未见对其目录内许可文件本身的记载。
3. §3.6 各条均为本项目自行核验（2026-08-30，绑定 `f65f7b34`），未经独立或专业复核；核验当日之后上游文件是否有变动，本仓库未复核。
4. 固定 commit `f65f7b34` 之后上游 NetBird 的许可文本、目录结构、REUSE 映射演进：本仓库未跟踪。
5. **委托主体与法域**：由哪个主体委托律师、意见适用的法域/执业辖区——仓库内无此信息，需用户提供。
6. 未来分发渠道范围：华为应用市场/HAP 之外，是否还包含 OpenHarmony 发行版渠道、企业内部分发或直接分发——README 提及长期双目标但渠道未锁定；影响问题五的范围。
7. 是否会自托管上游服务端（`management/`、`signal/`、`relay/`、`combined/`）用于测试乃至生产，以及服务端部署义务是否纳入本次委托范围：`docs/security-and-compliance.md` 将自托管测试服务端列为资产与威胁对象并单列 AGPL 服务端合规审查，但委托范围是否包含待用户确认。
8. 本仓库在 GitHub（`alfadb/netbird-harmonyos`）的公开/可见状态，以及是否已发生任何对外提供行为：本环境无法访问网络确认；是否影响评估范围待确认。
9. native 侧第三方依赖的逐包许可证清单：`boringtun 0.7.1`、`libc 0.2` 及 `Cargo.lock` 传递依赖尚无许可证清单（`docs/security-and-compliance.md` 将「按 v0.76.3 重建依赖锁和逐文件许可证清单」列为未完成事项）；这是问题五 SBOM 评估的事实输入。
10. 本项目最终产品名称与市场素材中的商标使用方案：README 仅载商标归属声明，未记载产品命名与素材方案；是否涉及商标问题及是否纳入本次委托范围待确认。

---

汇编生成：2026-09-12（基于 main @ `52405eb` 仓内文档与仓库状态；未访问网络、未核验上游仓库）。本文件不构成法律意见。
