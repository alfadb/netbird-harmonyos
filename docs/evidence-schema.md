# 证据与脱敏 Schema

最后核验：2026-08-09

本文定义 `netbird-harmonyos` 各证据门共同使用的记录、脱敏、审查和保留基线。该 schema 已建立，但当前没有因此自动获得任何阶段通过结论；每条结论仍须绑定具体证据。

## 信息状态

文档、测试记录和审查结论必须使用下列信息状态之一，不得把建议、推断或单次局部结果写成更强结论。

| 信息状态 | 定义 |
| --- | --- |
| 当前实测 | 已在本项目当前环境、制品或明确目标元组中直接执行并保留证据；结论只覆盖记录的时间与范围 |
| 官方确认 | 来自 Huawei、OpenHarmony、NetBird、Go 或其他对应上游的官方资料；运行可用性仍须由目标 SDK、设备和制品验证 |
| 方案建议 | 为实现、测试、发布或运维提出的当前方案，尚未由完整工程证据确认 |
| 尚未验证 | 缺少必要的工程、平台、真机、渠道、安全、合规或审查证据 |

现有文档中的“官方资料确认”与本 schema 的“官方确认”含义相同。引用官方资料时必须记录 URL、标题、版本或发布日期及访问日期；无法确认版本适用性时应降为“尚未验证”。

## 证据 ID

证据 ID 格式为 `EV-<门>-<目标代码>-<YYYYMMDD>-<四位序号>`；正式路线门示例为 `EV-R3-HMOS24-20260716-0001`，Emulator 投入门示例为 `EV-E0-EMU24-20260717-0001`。样例只说明格式，不代表存在对应证据或门已通过；既有 `EV-R1-EMU24-*` 历史 ID 保持不变，不重编号。

- `<门>` 使用 `R0` 至 `R10` 或 `E0` 至 `E8`；跨门记录使用产生该证据的最早门，并在“关联阶段/门”字段补充其他门。
- `<目标代码>` 使用经 R0 登记的短代码；研究期 Emulator 记录使用 `EMU24`，不得使用可能暗示真机支持的代码。
- 日期按证据开始时间所在时区转换为 `YYYYMMDD`；序号在同一门、目标代码和日期内唯一且递增。
- 证据 ID 一经分配不得复用或改写；作废、替代和重跑通过状态与关联 ID 表达。

## 必填字段

每条证据记录必须包含下列字段；只有对记录声明范围客观不适用的字段才填写 `N/A` 并说明原因，不得留空。依赖缺失、未执行、暂不可达或证据不足均不是 `N/A`，必须记录为对应的 `blocked` 或未完成状态。

| 字段 | 要求 |
| --- | --- |
| 证据 ID | 符合固定格式且全局可追踪 |
| 信息状态 | 当前实测、官方确认、方案建议或尚未验证之一 |
| 记录状态 | 使用本文定义的状态枚举 |
| 阶段/门 | 产生证据的 R 阶段或 E 投入门及所有关联阶段/门 |
| 目标元组 | 发行版、具名设备、完整系统版本、架构、SDK/API/SysCap 和渠道；研究证据须明确 Emulator 或宿主范围 |
| 代码 SHA | 本项目被测代码的完整 Git commit SHA；未提交研究代码须记录不可变归档哈希和原因 |
| 上游 SHA | NetBird 及直接参与被测路径的上游源码完整 commit SHA；纯工具链测试可填写 `N/A` 并说明 |
| 工具链 | OS/base image、Command Line Tools、SDK/API、HDC、Emulator、Go、Node.js、Hvigor、ohpm 及其他实际参与工具的版本 |
| 命令 | 可重放的非敏感完整命令、工作目录和关键环境变量名；秘密值必须替换为占位符 |
| 输入 | 测试配置、拓扑、网络条件、样本数、前置状态和输入制品引用，不包含秘密 |
| 预期结果 | 执行前确定的可判定标准和对应 R0 SLO 或阶段退出条款 |
| 实际结果 | 原始观测摘要、计数、统计口径、失败分类及与预期差异 |
| 时间戳与时区 | ISO 8601 开始与结束时间，必须包含 UTC 偏移或 `Z`，并记录所用时钟来源 |
| 制品哈希 | 所有被测 HAP、App Pack、native 库、服务端镜像、配置归档及报告的 SHA-256；无制品时填写原因 |
| 原始日志引用 | 受控存储中的不可变路径或对象 ID、日志 SHA-256、采集范围和访问级别；不得只粘贴筛选后片段 |
| 判定 | 通过、失败、阻塞或无效，并引用具体阈值；研究证据还须声明不能用于阶段退出 |
| 审查者 | 独立审查角色、审查时间和意见记录 ID；未审查时填写 `待独立审查` |

## 记录状态枚举

| 状态 | 含义 |
| --- | --- |
| `draft` | 记录结构已创建，执行或必填字段尚未完成，不可用于结论 |
| `collected` | 执行与原始材料已采集，尚未完成独立审查 |
| `reviewed-pass` | 独立审查确认范围、完整性和判定满足引用标准 |
| `reviewed-fail` | 独立审查确认证据有效，但结果不满足引用标准 |
| `blocked` | 因外部资源、工具或前置门缺失而无法形成有效判定 |
| `invalidated` | 发现污染、范围错误、哈希不一致、秘密泄露或方法缺陷，记录保留但不得继续引用，也不得作为 E8 当前成员 |
| `superseded` | 已由新证据替代，旧记录继续保留并双向引用；不等同于旧结果被删除，但不得作为 E8 当前成员 |

只有 `reviewed-pass` 可用于证明被测功能通过和阶段退出。E 项还必须同时满足 `verdict: pass`；`reviewed-pass` 只说明记录经审查合格，不能把 `blocked`、预期失败对照或范围外子项改写为功能通过。经审查的 blocked 记录只能证明其精确目标上的不可执行边界，不能当作 E3 pass，也不能登记为 `N/A`；E8 只能按下述规则把它登记为 reviewed dependency-blocked 聚合例外。

## Emulator 投入总门证据纪律

API 24 x86_64 Emulator 的 PASS、FAIL 与 blocked 均只适用于记录的目标元组、进程模型、输入和制品，不得外推 arm64、具名物理设备或华为商用 HarmonyOS。

所有客观可执行 E 项必须有独立记录，且 `record_status: reviewed-pass`、`verdict: pass` 同时成立。官方 API 24 x86_64 phone E3 记录证明其授权前置组件缺失且公开 runtime 路径不可继续；Tablet、2in1 记录只证明各自 registration-layer 前置边界缺少所需注册组件，按停止条件未安装 HAP，不能扩写为完整 runtime 不可执行结论。历史 evidence 和 raw 判定保持原样。

E3 在记录边界内为 reviewed blocked；E4-E7 因依赖 E3 授权前置而未启动，统一登记为 **reviewed dependency-blocked aggregation exception**。这一分类既不是 `pass`，也不是 `N/A`，不免除 E4-E7 的完整义务。E8 `OPEN` 后，E4-E7 必须移交到同一具名物理设备的 R2/R3 门完整执行；E8 `OPEN` 只许可物理设备投入，不表示 VPN、TUN、`protect`、双向数据面或 lifecycle 已通过。

若新的官方 Emulator image/build 增加了授权或注册组件，既有 blocked 边界对该新目标元组的聚合适用性立即失效。旧记录继续保留且不改写，但不得继续作为当前 E3-E7 blocked-exception；必须在新 image/build 上从 E3 开始重验，并按实际可达性执行后继项。

E8 聚合表必须为每个成员使用以下三种分类之一：

| 聚合分类 | 使用条件 | 对 E8 的含义 |
| --- | --- | --- |
| `pass` | 独立记录同时为 `record_status: reviewed-pass`、`verdict: pass`，且目标元组与哈希一致 | 满足该成员的正面条件 |
| `blocked-exception` | 经审查记录证明精确依赖边界 `blocked`，聚合记录明确写为 reviewed dependency-blocked aggregation exception | 只豁免在 Emulator 上伪造不可产生的 pass，不免除后续完整义务 |
| `N/A` | 该成员对聚合声明范围客观不适用，并有可审查理由 | 不得用于依赖缺失、未执行、暂不可达或证据不足 |

E8 `OPEN` 至少同时满足以下必要条件，缺一即保持 `CLOSED`：

- `E3-PHYS-PREFLIGHT` 同时为 `record_status: reviewed-pass`、`verdict: pass`；`blocked`、`fail` 或 `invalid` 均不满足。
- 所有客观可执行项均为 `pass`，并确认当前R0正式基线（现v0.76.3）的官方 Go loader/runtime 已形成 E1 `reviewed-pass/pass`。当前 loader 负面证据绑定 v0.74.6；v0.76.3 尚未重跑且没有 pass。
- 聚合记录重核目标元组、代码 SHA、相关上游 SHA、APP/TEST HAP、native/Go 库、配置和其他输入 SHA-256；不得存在目标漂移、member 不一致或缺失 hash。任何 `record_status: invalidated` 或 `record_status: superseded` 的引用都只能保留在历史追溯链中，均不得进入 E8 当前成员集合。
- 独立聚合审查核对每个 `pass`、`blocked-exception` 与客观 `N/A`，并显式作出 `OPEN` 决定。

预检通过只是上述必要条件之一，不是充分条件，也不自动 `OPEN` E8。E8 前唯一物理设备例外是 [E3-PHYS-PREFLIGHT](e3-physical-preflight.md)：一个冻结 campaign 在一台具名 HarmonyOS 6.1 arm64 设备上，用普通开发签名的纯 ArkTS/C 公共 VPN Extension A/B 探针判断 E3 可达性。证据必须绑定设备型号、完整 build、API、arm64、稳定设备别名、签名/profile、A/B HAP、源码/SDK/hash 和清理基线，并保留原始 HiLog、transcript、screenshots、状态/布局、fault list、hash manifest 和独立审查。HDC target、序列号及签名秘密不得入库；其重试、60 秒场景窗口、deny、Settings 和清理判据以专用计划为准。除该例外外，E8 前仍禁止物理设备执行。

arm64 ABI、其他真实硬件、物理网络切换、硬件密钥、能耗、渠道签名/审核/重签/最终制品和长时间稳定性等项目仍列入 E8 `OPEN` 后的具名物理设备义务。C-only 证据不能独立满足含官方 Go loader/runtime 的 E1。`EV-R1-EMU24-20260717-0010` 及其 PS4 候选保持历史研究证据；PS4 未发布且不是当前门输入，不能满足当前R0正式基线（现v0.76.3）的官方 Go loader 或 VPN runtime 门。

## 脱敏规则

- 账号、口令、setup key、token、Cookie、会话标识、恢复码、签名私钥、证书口令、节点私钥、短期密钥、可复用临时下载 URL 和完整授权头不得进入证据记录、原始日志归档或版本控制。
- 命令中的秘密统一替换为语义占位符，例如 `${SETUP_KEY_REDACTED}`；不得保留前缀、后缀、长度或可用于关联真实值的哈希。
- 设备序列号、用户标识、公网 IP、内部域名、peer 名称和文件系统个人路径按最小必要原则使用稳定假名；映射表若确有必要，必须位于证据体系之外的受控存储。
- IP、DNS、路由和拓扑只有在测试判定确实需要时才保留；对外报告使用文档保留地址或抽象标识，原始网络捕获须限制访问并按原始日志期限删除。
- 日志采集前配置源端过滤，归档前再执行自动扫描和人工抽查；发现秘密时立即隔离和删除受污染副本、轮换相关凭据，并把原证据标记为 `invalidated`。
- 脱敏不得改变时间顺序、错误码、计数、统计分布或判定所需语义；无法可靠脱敏的材料不得进入普通证据存储。
- 制品哈希用于完整性和关联，不得用于保存或变相校验低熵秘密。

## 单条证据模板

```yaml
evidence_id: EV-<R-stage-or-E-gate>-<target>-<YYYYMMDD>-<sequence>
information_status: current-measured | official-confirmed | proposal | unverified
record_status: draft | collected | reviewed-pass | reviewed-fail | blocked | invalidated | superseded
stage_or_gate: R<stage> | E<gate>
related_stages_or_gates: []
target_tuple:
  distribution: <value>
  device: <value-or-emulator>
  full_system_version: <value>
  architecture: <value>
  sdk_api_syscap: <value>
  channel: <value-or-N/A-with-reason>
code_sha: <full-sha>
upstream_sha: <full-sha-or-N/A-with-reason>
toolchain: <version-map>
working_directory: <path>
command: <redacted-replayable-command>
input: <non-secret-input-and-preconditions>
expected: <criterion-and-threshold-reference>
actual: <observation-and-statistics>
started_at: <ISO-8601-with-zone>
ended_at: <ISO-8601-with-zone>
clock_source: <value>
artifact_sha256: <map-or-N/A-with-reason>
raw_log_reference: <immutable-reference-sha256-access-level>
verdict: pass | fail | blocked | invalid
reviewer: <role-or-pending>
reviewed_at: <ISO-8601-with-zone-or-pending>
review_record: <id-or-pending>
```

## 支持矩阵模板

| 记录 ID | 信息状态 | 发行版 | 具名设备 | 完整系统版本 | 架构 | SDK/API/SysCap | 渠道与最终制品哈希 | 功能范围 | 已通过证据门 | 证据 ID | 限制与已知问题 | 最近复核 | 审查状态 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `SM-<序号>` | 尚未验证 | `<发行版>` | `<设备>` | `<完整版本>` | `<架构>` | `<依据>` | `<渠道及 SHA-256>` | `<能力>` | `<R 阶段>` | `<证据 ID 列表>` | `<限制>` | `<ISO 8601>` | `<状态>` |

支持矩阵只列入有对应目标元组证据的组合。Emulator、相近型号、同一大版本或其他渠道结果不得外推；市场重签后的最终制品必须使用其自身哈希和回归证据。

## 动态调整记录模板

| 字段 | 内容 |
| --- | --- |
| 调整 ID | `ADJ-<YYYYMMDD>-<四位序号>` |
| 日期与时区 | `<ISO 8601>` |
| 提出角色 | `<角色>` |
| 触发证据 | `<证据 ID 或官方资料引用；官方 URL 须含访问日期>` |
| 调整原因 | `<反证、不确定性或新条件>` |
| 调整内容 | `<范围、顺序、实现路线、退出标准或版本映射变化>` |
| 受影响阶段 | `<E 门与 R 阶段列表>` |
| 版本与依赖核对 | `<适用时记录 tag、commit、声明工具链、release run URL/结果/访问日和关键依赖差异>` |
| 已评估替代方案 | `<方案及未选择原因>` |
| R0/SLO/补丁预算影响 | `<无，或明确说明>` |
| 重跑范围 | `<须重开或重新验证的门、阶段与横切审查>` |
| T0 判定与决策依据 | `<不触发、待讨论、已讨论及记录引用，或明确人类直接决定及其一次性替代范围>` |
| 生效条件 | `<何时开始约束后续输入与执行>` |
| 回退条件 | `<停止、回退或重新决策条件>` |
| 审查状态 | `<角色、时间、结论及尚未完成的证据审查>` |

调整记录不得改写或删除既有证据。人类直接决策者对重大技术方向具有优先权；明确的人类直接决定可以只对该次内部 T0 触发进行替代，但必须记录决定范围、替代方案、生效条件、回退条件和审查状态，且不得虚构 T0。没有明确人类决定时，触及首目标、核心数据面、跨语言边界、VPN 能力门、发布门、支持声明，或放宽 R0 阈值与补丁预算的重大调整，仍须按章程和路线图触发 T0；用户明确要求 T0 时始终按 T0 协议执行。

## 补丁记录模板

| 字段 | 内容 |
| --- | --- |
| 补丁 ID | `PATCH-<上游代码>-<四位序号>` |
| 适用上游 | `<项目、版本、完整 SHA>` |
| 补丁制品 | `<文件或 commit 完整 SHA、SHA-256>` |
| 引入阶段 | `<R 阶段>` |
| 原因与证据 | `<问题及证据 ID>` |
| 修改范围 | `<模块、ABI、行为>` |
| 维护风险 | `低`、`中` 或 `高`，并说明依据 |
| 上游状态 | `<未提交、已提交、已接受、已拒绝或不适用及引用>` |
| 替代方案 | `<已评估方案>` |
| 验证证据 | `<正向、回归和失败证据 ID>` |
| 移除条件 | `<版本或能力条件>` |
| 预算计数 | `<R2 前、R3 累计、R4-R5 累计和总数>` |
| T0 状态 | `<未触发或记录引用>` |
| 责任与复核日期 | `<角色及 ISO 8601>` |

任何高维护风险补丁或预算超限必须先标记 T0 触发，不得通过改变计数口径继续晋级。

## 保留期

| 材料 | 最短保留期 |
| --- | --- |
| 阶段门退出证据与每个候选/发布制品的 SBOM | 对应目标 EOL 后 2 年 |
| 原始日志、抓包和原始性能数据 | 至少 90 天；若被事故、漏洞、审计或未关闭争议引用，则保留至事项关闭且满足更长期限 |
| Emulator 原始日志 | 30 天；若被 E0-E8 聚合、升级为阶段问题、事故或动态调整触发证据，则适用原始日志至少 90 天规则 |
| 脱敏后的证据结果、审查记录、支持矩阵和动态调整记录 | 项目生命周期 |
| 秘密 | 不进入证据体系，不定义证据保留期 |

到期删除必须可审计，记录材料类别、范围、删除时间和执行角色，但不得在删除记录中复述敏感内容。法律、渠道或组织政策要求更长期限时采用较长者，并记录依据。
