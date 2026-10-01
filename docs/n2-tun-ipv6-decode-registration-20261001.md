# N2 TUN IPv6 解码窄修订 —— v4.3 登记记录（独立哈希记录）

- 记录性质：**登记记录 / 独立哈希记录**。它不是冻结件、不是 AUTH、不是设备授权、不是实现符合性结论；规则正文以冻结件文末「v4.3 追加修订记录」与格式基线 `docs/n2-log-formats-20260928.md` 为准，本文件只登记两者落盘后的整文件 sha256 与字节数（按 52b2bac 的 sidecar 先例：独立文件、不写进被登记件自身）。
- 登记对象与批准依据（均为被登记件之外的只读依据）：
  - 冻结预注册件（v4.3 = v4.2 全文 90777 B + 文末追加的 v4.3 节）：`docs/n2-device-evidence-preregistration-draft-v2-20260928.md`
  - 格式基线（按获批 R1 追加尾字段与口径、按 R2 加唯一窄例外）：`docs/n2-log-formats-20260928.md`
  - 被批准提案快照：`docs/n2-tun-ipv6-decode-amendment-proposal.md`，sha256 `b1c9758752fd092d57cae4938b65c2e3fa82e052a045da500244e4442efd4cb7`（状态行仍为 `DRAFT / UNAPPROVED`，未被改写）
  - 批准事实记录：`docs/n2-tun-ipv6-decode-approval-record.md`，sha256 `bb95e6f43f6b8e6993392da1a851a91c5afcd3e56ac77745616e877f44866d64`
  - 治理同版登记：`docs/native-nx-governance.md` 文末「追加登记：N2 预注册 v4.3 的 IPv6 TUN 解码窄修订（只追加不改写）」
- 现算哈希（完整文件落盘后 `sha256sum` / `wc -c` 现算，未改写）：

| 文件 | sha256 | 字节 |
| --- | --- | --- |
| `docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.3） | `1787075596ec5b328afe04cf4c60e0d4595ad1c9613c03b83c55421f6eb4ec94` | 104048 |
| `docs/n2-log-formats-20260928.md`（R1/R2 落实后） | `5b94f117f08294a9478e0bd404513a86343f5cb5c2a27286d8ef8ddcbfed2af8` | 52684 |
| `docs/native-nx-governance.md`（文末追加 v4.3 注记后；前 47303 字节与 HEAD 旧件逐字节一致） | `1aaca82ea3d2b9f21557b0b682846222210894520848bab1761fe32fe61b147e` | 55697 |

- 旧件（登记前）哈希（用于前缀/旧件核对，均已作废，不得再作为现行基线）：

| 文件 | sha256 | 字节 |
| --- | --- | --- |
| `docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.2 冻结件） | `80a90cfdfeff37ad9dfa048e3e6e9df44db8b5d3dc8150d5005c189ec3a3e903` | 90777 |
| `docs/n2-log-formats-20260928.md`（旧格式基线） | `7812d56162f636b59ee8b24a1e71ed284f94a295aef00b447348af4352e0f8da` | 45527 |
| `docs/native-nx-governance.md`（登记前） | `f6bbf57d415862575edb8d6b9742c514f82ab4cdc0880acccad0811a64b13860` | 47303 |

- 原正文保留验证（只追加不改写）：登记后的预注册件前 90777 字节与 `git show HEAD:docs/n2-device-evidence-preregistration-draft-v2-20260928.md`（v4.2 冻结件）用 `/usr/bin/cmp` 逐字节一致，`head -c 90777 | sha256sum` 复算仍为 `80a90cfdfeff37ad9dfa048e3e6e9df44db8b5d3dc8150d5005c189ec3a3e903`；格式基线的既有字段名/顺序/语义与 `Exact invariant` 一句一字未动（追加尾字段 + 完整性判据加唯一窄例外，逐字登记获批提案 §4 R1/R2）。
- 独立 sidecar：`docs/n2-device-evidence-preregistration-draft-v2-20260928.md.sha256` 已同步为 v4.3 整文件哈希；格式基线不另立 `.sha256` sidecar，其哈希登记在本文件与治理文末登记节。
- 登记提交：本次本地提交（message `docs(n2): register approved v4.3 IPv6 TUN decode amendment as frozen text`），**未 push**；commit 的 blob 与工作区一致，故 `git show <commit>:<path>` 的哈希即上表现算值。
- 实现偏差：与获批提案文本的偏差按格式文档「实现偏差登记」一节记录（链未走通时 `first_v6_other_next` 记 `-`；短帧目的 hex 只记帧实际携带字节；扩展头集合按 IANA；`< 40 B` 版本 6 帧 scope=`other`/next=`hbh`）。
- **非 AUTH / 不追认旧证据 / 未验收实现**：本记录不授予 `N2 第一阶段通过`、不签发 AUTH、不授权任何设备/服务器/抓包操作（含提案 §6 的 `ip -6 addr`/`ip -6 route` 只读发现）、不追认 0048–0051 或任何 `is_evidence=false` 诊断记录为证据，也不构成「实现已验收」——按提案 §7 第 1 步，kit 判定器/执行器同步与独立 T0（跨厂商 family，干净上下文）符合性审查仍未完成。
- 写者边界：本文件为新建；除上述 docs 修改外未改代码/kit/远端，未跑设备命令与抓包。
