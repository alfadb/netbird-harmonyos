# NetBird 上游变更盘点：v0.80.0 → v0.81.4

> 用途：第三方 HarmonyOS 客户端（netbird-harmonyos）「是否需要跟进上游」的评估依据。
> 所有英文标题均为抓取到的原文，未翻译、未合并、未删减；说明性文字为中文。
> 抓取时间：2026-10-24T21:35+08:00（UTC 13:35）。数据来源见文末「证据清单」。

---

## 0. 实际存在的版本号清单（先回答"有没有 v0.81.4"）

**v0.81.4 存在**，是当前最新发布（2026-10-24T11:24:02Z 发布，距抓取约 2 小时）。

v0.80.0 之后、到 v0.81.4 为止，仓库里实际存在的 tag 共 **15 个**（5 个 final：v0.81.0/1/2/3/4；10 个预发布：3 个 rc + 7 个 canary，含 0.81.0 发布后生成的 `v0.81.0-canary.pr-7407.2`）。**不存在 v0.80.1**，也不存在 v0.81.5 及以上的版本。

| # | Tag | 类型 | published_at (UTC) | release note 长度 | 页面 |
|---|-----|------|--------------------|------------------|------|
| 1 | v0.80.0 | 基线 final | 2026-10-01T16:08:09Z | 14092 | https://github.com/netbirdio/netbird/releases/tag/v0.80.0 |
| 2 | v0.81.0-canary.pr-7662.1 | canary | 2026-09-26T15:53:57Z | 2981 | 见 tag |
| 3 | v0.81.0-canary.pr-7395.1 | canary | 2026-10-06T15:38:42Z | 7251 | 见 tag |
| 4 | v0.81.0-canary.pr-7890.1 | canary | 2026-10-06T21:36:25Z | 17932 | 见 tag |
| 5 | v0.81.0-canary.pr-8098.1 | canary | 2026-10-07T14:20:35Z | 385 | 见 tag |
| 6 | v0.81.0-canary.pr-7407.1 | canary | 2026-10-08T09:12:45Z | 795 | 见 tag |
| 7 | v0.81.0-rc.1 | RC | 2026-10-12T18:10:06Z | 3906 | https://github.com/netbirdio/netbird/releases/tag/v0.81.0-rc.1 |
| 8 | v0.81.0-rc.2 | RC | 2026-10-13T15:16:18Z | 238 | https://github.com/netbirdio/netbird/releases/tag/v0.81.0-rc.2 |
| 9 | v0.81.0-rc.3 | RC | 2026-10-14T16:50:38Z | 364 | https://github.com/netbirdio/netbird/releases/tag/v0.81.0-rc.3 |
| 10 | v0.81.0-canary.pr-8192.1 | canary | 2026-10-14T20:56:58Z | 88 | 见 tag |
| 11 | **v0.81.0** | final | **2026-10-15T14:05:02Z** | 20004 | https://github.com/netbirdio/netbird/releases/tag/v0.81.0 |
| 12 | **v0.81.1** | final | **2026-10-16T00:42:35Z** | 264 | https://github.com/netbirdio/netbird/releases/tag/v0.81.1 |
| 13 | **v0.81.2** | final | **2026-10-16T09:18:31Z** | 240 | https://github.com/netbirdio/netbird/releases/tag/v0.81.2 |
| 14 | **v0.81.3** | final | **2026-10-19T13:50:20Z** | 400 | https://github.com/netbirdio/netbird/releases/tag/v0.81.3 |
| 15 | **v0.81.4** | final（最新） | **2026-10-24T11:24:02Z** | 565 | https://github.com/netbirdio/netbird/releases/tag/v0.81.4 |
| — | v0.81.0-canary.pr-7407.2 | canary（0.81.0 之后） | 2026-10-21T17:30:15Z | 189 | 见 tag |

> 注意：canary tag（`v0.81.0-canary.pr-*.N`）是 PR 级预览构建，正文多为占位说明（如 `Only testable code based on ongoing development.`）或仅列部分提交，**不含完整变更清单**，不能当作发布依据。

### 一个必须先说清的数据质量问题

v0.81.1 / v0.81.2 的 release note **严重漏报**，与实际提交内容不符：

- v0.81.1 note 只有 1 条，但 `compare/v0.81.0...v0.81.1` 有 **17 个提交**；GitHub 页面自己也写着 "32 commits to main since this release"。
- v0.81.2 note 只有 1 条（#8226/#8227），实际区间 **6 个提交**。
- v0.81.4 note 有 3 个 bullet，但第 1 条和第 3 条是同一个 PR（#8331），去重后只有 2 项，与区间提交数 2 一致。

v0.81.1 与 v0.81.2 的 note 格式也与 v0.81.0 不同：没有 `by @author`，PR 链接写成 `(https://…/pull/N)` 而非 `(#N)`；v0.81.4 则混用两种写法并出现同一条重复两次。判断为发布 note 生成器更换后出问题。因此下文对 patch 版本**同时给出 note 原文与 git 区间提交**，两者都标注来源。

---

## 1. 逐版本变更清单（原文标题 + 标签）

标签取值：`客户端-移动端` / `客户端-桌面/守护进程` / `管理端/服务端` / `依赖/构建/CI/文档` / `其他`。
跨端的条目在标签后加 `[+客户端]` / `[+管理端]` 标记，便于筛选。

### 1.1 v0.81.0 — 2026-10-15T14:05:02Z

v0.81.0 的 release note **不是** PR 标题清单，而是一份人工撰写的、把多个 PR 合并成一个 bullet 的发布说明（例如 cert posture 一条 bullet 覆盖 #7535/#8052/#7890/#8174 四个 PR）。因此：

- **原始标题以「1.1.B git 区间提交」为准**（94 条，逐条原文，含 note 未收录的条目）；
- **「1.1.A」保留 note 原文**，因为 breaking/升级须知只写在 note 里。

**Full Changelog：** https://github.com/netbirdio/netbird/compare/v0.80.0...v0.81.0

#### 1.1.A 官方 release note 原文要点（逐条保留原文，不翻译）

Highlights：

- `### 🪪 Certificate Posture Checks` — "A new **Certificate** posture check requires peers to hold a device certificate issued by your own CA, and to prove they still have the private key by signing a challenge from management. A copied certificate without its key fails the check. Management repeats the challenge every 4 hours while the peer is connected, so a removed key or an expired certificate closes access within that window."
  - "The client uses the operating system's own certificate stores: the Local Computer and user Personal stores on Windows (TPM-backed keys included), the System and login keychains on macOS (Secure Enclave keys included), and PEM files in `/etc/netbird/certs`, TSS2 key files, or a tpm2-pkcs11 token on Linux."
  - **"Requires management and client **v0.81.0** or newer. The iOS and Android apps do not support certificate proofs and always fail this check."** ← 对第三方客户端最关键的一句
  - PR 列表：#7535, #8052, #7890, #8174, dashboard#798
- `### 🔗 nblink: forward into your network without root` — 新独立二进制，用户态模式（无 TUN、无 root、不碰 `/etc/resolv.conf`），`--forward 'http://3000=https://tickets.internal'`；仅支持 HTTP 转发，raw TCP/UDP/SOCKS5 计划中。PR #7955。
- `### 📋 Duplicate Policies` — dashboard 策略行菜单新增 Duplicate。dashboard#841。
- `### 🛣️ Path-based access control for Reverse Proxy services` — 每个 HTTP target 可覆盖服务的认证：`Use service authentication`（默认）/ `Bypass authentication` / `Block access`（403）。最长区分大小写字面前缀匹配；有 target 使用 access action 时，含 `..`、`//`、编码分隔符的歧义路径直接 400。
  - **"Upgrade every proxy in the cluster before using these actions; older proxies ignore them and keep applying the service authentication."**
  - PR #8114, #8115, #8116, #8117, dashboard#823
- `### Deprecation: MySQL store` — **"MySQL support for the Management store is deprecated and will be removed in an upcoming release."** 新增 `netbird-mysql-migrate` 迁移工具，SQLite/PostgreSQL 仍完全支持。PR #8095。

What's New（原文 bullet，逐条）：

**🔒 Security and Access Control**
- "Added the **Certificate posture check**, with a hardened client, fresh challenge nonces, recovery from stuck proof collections, and fixes for remaining edge cases. #7535, #8052, #7890, #8174"
- "**Used single-use codes for the OIDC session handoff** between management and the proxy, and renamed the session code query parameter. #7635, #7981"
- "**Enforced strict base64url decoding for JWT validation** in management and the proxy. #7554"
- "**Refactored the PKCE verifier store into a reusable single-use store**. #7634"
- "Added a **revocation guard hook to the proxy token API**. #7732"
- "**Returned a permission-denied error when a setup key is expired or invalid**, instead of a generic failure. #8086"

**🖥️ Client Improvements**
- "Added **nblink**, an unprivileged forwarder into the network. #7955"
- "**Replaced the eBPF WireGuard proxy with loopback endpoint addressing**, removing the eBPF dependency for kernel-mode relayed connections. #7316"
- "**Stopped one unreachable relay from churning connections on the others**, and stopped offering to every peer when the relay transport drops. #8098, #7092"
- "**Kept the advertised ICE session ID when following a remote restart**, avoiding unnecessary reconnects. #7814"
- "**Cached the box shared key per remote peer** in the Signal client, and cached the interface check shared by ICE agents. #7807, #8001"
- "**Configured the userspace device through its typed API** and **reused one wgctrl client** in the kernel configurer. #7937, #7912"
- "**Discovered interfaces lazily in stdnet** instead of at construction. #7346"
- "**Forced interactive login when extending the auth session**, evaluated the session deadline against the wall clock, and dropped the pending login flow when a login switches profile. #7216, #7959, #7882"
- "**Kept the updater silent until management decides the update mode**. #7455"
- "**Gated settings updates on value, not on field presence**. #7398"
- "Added **debug cpu start and stop** commands for CPU profiling. #7749"
- "**Classified Windows local accounts by NetBIOS name**. #7628"
- "Fixed the **Windows UI exiting on first launch**, and closed the UI before InstallValidate in the MSI. #8138, #7395"
- "Fixed the **browser login popup** shown from the daemon, and kept the delete-profile dialog open until the delete finishes. #7408, #7752"
- "Added **RTL layout support** and **Galician (gl) localization** to the desktop UI, warned on missing translations, and only offered locales listed in `_index.json`. #8076, #7041, #8090, #8091"

**📱 Mobile**
- "**Coalesced peer list change notifications** to the Android and iOS listener. #7546"
- "**Generated the Android debug bundle to a file**. #7528"
- "**Resolved the Android network route peer by HA unique ID** instead of scanning the full status. #7705"
- "**Skipped late session warnings on desktop and scheduled them in the app on Android**. #7548"

**⚙️ Management Improvements**
- "**Exposed peer MAC addresses and made peers searchable by MAC**. #6553"
- "Added an **account setting to return an error on peer name collision** during AddPeer. #8154"
- "**Refreshed only affected peers** on DNS zone, DNS record and IPv6 settings changes. #8050, #8051"
- "**Emitted activity events for batch group operations**. #8180"
- "Added a **MySQL to PostgreSQL migration tool**. #8095"
- "**Made the Let's Encrypt challenge listener address configurable** for management and signal. #7706"
- "**Tore down only the peer session that owns the stream**. #8057"
- "**Updated proxy mappings on peer IP change** and the reserved IP exclusion when generating peer IPs. #8139, #8144"
- "**Expanded loaded snapshots in place**, reducing allocations on network map computation. #8177"
- "**Cleaned up Agent Network configuration on account deletion** and added a disabled state to the managed Agent Network proxy API. #7812, #7744"
- "**Switched to libopenapi** for managing the OpenAPI-based API, and added the tenant delete endpoint to the spec. #8056, #8054"
- "**Improved Base62 encoding and decoding** performance and robustness. #3391"
- "**Removed ingress ports**. #8062"
- "Extracted a **shared database connection and data repository**, and handled connection close on error. #7649, #7740"
- "Fixed the **login filter**, created HttpConfig before applying TLS flag overrides, and moved the child-account check. #7739, #8168, #8195"

**🔀 Reverse Proxy**
- "Added **per-target HTTP access actions** (use service authentication, bypass, block), backed by middleware snapshot revisions and container E2E coverage. #8114, #8115, #8116, #8117"
- "**Required a private proxy cluster for cluster and direct upstream targets**. #7984"
- "Added an **option to refuse private addresses on direct-upstream dials**. #7913"
- "**Fell back to the IdP for the proxy session email** when the token does not carry one. #8112"

**📡 Signal and Relay**
- "**Fixed a race in Relay server shutdown**. #7607"
- "**Stopped Signal with an error when a server fails**, instead of continuing half-started. #8105"

**🏗️ Infrastructure and CI**（原文摘要，标题较长者见 1.1.B 内对应 PR 原文）
- "Added **release-wired Red Hat UBI image variants** for management, signal, relay and the combined server… #8167, #7458, #7457, #7953, #8055"
- "Fixed **Red Hat certification preflight** artifact handling. #7947, #8111"
- "**Fixed Traefik versioning and updated CrowdSec** in the self-hosted setup. #8152"
- "**Replaced the `which` dependency with `command`** in `configure.sh`. #8007"
- "Added a **Homebrew cask e2e test**, moved the FreeBSD port test to 15.1, and reported both peers' state when the connect test times out. #7618, #7999, #7944"
- "**Exempted Crowdin service PRs** from title and docs checks. #7170"
- "Test fixes for the update message tests, peer IP allocation, and mysql-migrate container tests. #8000, #8173, #8121, #8128"

**📝 Misc**
- "**Used ASCII hyphens in the LICENSE header** and extended the copyright year to 2026. #8065, #8169"

**Dashboard (v2.95.0)**：note 中另有 14 条 dashboard 变更（Certificate posture check、Duplicate、per-target HTTP access controls、peer name collision 设置、MAC 搜索、OpenCode/Pi Connect Agent、Agent Network onboarding、robots.txt 屏蔽搜索引擎、UBI 镜像、Storybook 视觉回归等）。属 dashboard 前端，与客户端协议无关，原文见 release 页面同段落。

#### 1.1.B v0.81.0 git 区间提交（94 条，原始标题）

来源：`GET /repos/netbirdio/netbird/compare/v0.80.0...v0.81.0`（ahead_by=94, merge_base=a8162aaa）。这些是该区间内全部提交标题的原文，**包含 release note 未提及的条目**（如 #7731、#7806、#7889、#7750、#7528 的完整措辞等）。

| # | 原始标题 | 标签 |
|---|---------|------|
| 1 | `[client] Stop dumping the whole device to clear one peer endpoint (#7632)` | 客户端-桌面/守护进程 |
| 2 | `[infrastructure] Fix flow auth secret for external Relay migrations (#7731)` | 管理端/服务端 |
| 3 | `[management] extract shared db conn + data repository (#7649)` | 管理端/服务端 |
| 4 | `[management] Add a revocation guard hook to the proxy token API (#7732)` | 管理端/服务端 |
| 5 | `[management, proxy] Enforce strict base64url decoding for JWT validation (#7554)` | 管理端/服务端 |
| 6 | `[management] Refactor PKCE verifier store into a reusable single-use store (#7634)` | 管理端/服务端 |
| 7 | `[management] fix login filter (#7739)` | 管理端/服务端 |
| 8 | `[management] Add a disabled state to the managed Agent Network proxy API (#7744)` | 管理端/服务端 |
| 9 | `[client] Keep the delete-profile dialog open until the delete finishes (#7752)` | 客户端-桌面/守护进程 |
| 10 | `[management,proxy] Use single-use codes for OIDC session handoff (#7635)` | 管理端/服务端 |
| 11 | `[client] Replace the eBPF WireGuard proxy with loopback endpoint addressing (#7316)` | 客户端-桌面/守护进程 |
| 12 | `[proxy] Apply the upstream HTTP version before cloning transports (#7806)` | 管理端/服务端 |
| 13 | `[client] Classify Windows local accounts by NetBIOS name (#7628)` | 客户端-桌面/守护进程 |
| 14 | `[management] handle db conn close on errpr (#7740)` | 管理端/服务端 |
| 15 | `[client] Keep the advertised ICE session ID when following a remote restart (#7814)` | 客户端-桌面/守护进程 |
| 16 | `[client] Cache the box shared key per remote peer in the Signal client (#7807)` | 客户端-桌面/守护进程 |
| 17 | `[client] Add debug cpu start and stop commands (#7749)` | 客户端-桌面/守护进程 |
| 18 | `[management] Clean up after account deletion (#7812)` | 管理端/服务端 |
| 19 | `[management] Let usage_viewer read Agent Network access logs (#7750)` | 管理端/服务端 |
| 20 | `[client] Only treat LocalSystem as a privileged identity by SID on Windows (#7889)` | 客户端-桌面/守护进程 |
| 21 | `[client] Add Homebrew cask e2e test (#7618)` | 依赖/构建/CI/文档 |
| 22 | `[client] Report both peers' state when the connect test times out (#7944)` | 依赖/构建/CI/文档 |
| 23 | `[proxy] Optionally refuse private addresses on direct-upstream dials (#7913)` | 管理端/服务端 |
| 24 | `[management,proxy] Rename the OIDC session code query parameter (#7981)` | 管理端/服务端 |
| 25 | `[management] expose peer MAC addresses and make peers searchable by MAC (#6553)` | 管理端/服务端 |
| 26 | `[misc] Move the FreeBSD port test to release 15.1 (#7999)` | 依赖/构建/CI/文档 |
| 27 | `[client] Discover interfaces lazily in stdnet instead of at construction (#7346)` | 客户端-桌面/守护进程 |
| 28 | `[management] Require a private proxy cluster for cluster and direct upstream targets (#7984)` | 管理端/服务端 |
| 29 | `[infrastructure] Create the preflight artifacts directory before submitting (#7947)` | 依赖/构建/CI/文档 |
| 30 | `[self-hosted] Replace "which" dependency by "command" from configure.sh script (#8007)` | 依赖/构建/CI/文档 |
| 31 | `[management] Improve Base62 encoding/decoding performance and robustness (#3391)` | 管理端/服务端 |
| 32 | `[management] Refresh only affected peers on DNS zone and record changes (#8050)` | 管理端/服务端 |
| 33 | `[client,android] Generate debug bundle to file (#7528)` | 客户端-移动端 |
| 34 | `[client] Resolve the Android network route peer by HA unique ID instead of scanning the full status (#7705)` | 客户端-移动端 |
| 35 | `[client] Fix browser login popup show from go (#7408)` | 客户端-桌面/守护进程 |
| 36 | `[management] Refresh only affected peers on IPv6 settings changes (#8051)` | 管理端/服务端 |
| 37 | `[client] stop offering to every peer when the relay transport drops (#7092)` | 客户端-桌面/守护进程 |
| 38 | `[client] Skip late session warnings on desktop and schedule them in the app on Android (#7548)` | 客户端-移动端 |
| 39 | `[management] add tenant delete endpoint to openapi (#8054)` | 管理端/服务端 |
| 40 | `[self-hosted] Add a UBI image variant for the combined server (#7953)` | 依赖/构建/CI/文档 |
| 41 | `[misc] Share one license collection script across the UBI images (#8055)` | 依赖/构建/CI/文档 |
| 42 | `[misc] Use ASCII hyphens in the LICENSE header (#8065)` | 其他 |
| 43 | `[client] Gate settings updates on value, not on field presence (#7398)` | 客户端-桌面/守护进程 |
| 44 | `[client] Add RTL layout support to the desktop UI (#8076)` | 客户端-桌面/守护进程 |
| 45 | `[management] fix the concurrency handling within the update message tests (#8000)` | 依赖/构建/CI/文档 |
| 46 | `[client] feat(i18n): Add Galician (gl) localization for desktop client (#7041)` | 客户端-桌面/守护进程 |
| 47 | `[client, android, ios] Coalesce peer list change notifications to the mobile listener (#7546)` | 客户端-移动端 |
| 48 | `[management] remove ingress ports (#8062)` | 管理端/服务端 |
| 49 | `[client] Warn on missing translations and check placeholders (#8090)` | 客户端-桌面/守护进程 |
| 50 | `[client] Only offer locales listed in _index.json (#8091)` | 客户端-桌面/守护进程 |
| 51 | `[client] Close the UI before InstallValidate in the MSI (#7395)` | 客户端-桌面/守护进程 |
| 52 | `[infrastructure] Exempt Crowdin service PRs from title and docs checks (#7170)` | 依赖/构建/CI/文档 |
| 53 | `[management] Return permission-denied error when setup key is expired or invalid (#8086)` | 管理端/服务端 |
| 54 | `[client] Stop one unreachable relay from churning the others (#8098)` | 客户端-桌面/守护进程 |
| 55 | `[management] Add a MySQL to Postgres migration tool (#8095)` | 管理端/服务端 |
| 56 | `[infrastructure] Find per-arch preflight results in the Red Hat certify check (#8111)` | 依赖/构建/CI/文档 |
| 57 | `[management] switch to libopenapi for managing of openapi-based api (#8056)` | 管理端/服务端 |
| 58 | `[relay] Fix race in Relay server shutdown (#7607)` | 管理端/服务端 |
| 59 | `[client] Keep the updater silent until management decides the mode (#7455)` | 客户端-桌面/守护进程 |
| 60 | `[misc] Skip mysql-migrate container tests on Windows (#8121)` | 依赖/构建/CI/文档 |
| 61 | `[misc] Exclude tools/mysql-migrate from docker tests (#8128)` | 依赖/构建/CI/文档 |
| 62 | `[client] Force interactive login when extending the auth session (#7216)` | 客户端-桌面/守护进程 |
| 63 | `[management] Tear down only the peer session that owns the stream (#8057)` | 管理端/服务端 |
| 64 | `[management] Fall back to the IdP for the proxy session email (#8112)` | 管理端/服务端 |
| 65 | `[client] Cache the WireGuard interface check shared by ICE agents (#8001)` | 客户端-桌面/守护进程 |
| 66 | `[client] Evaluate the session deadline against the wall clock (#7959)` | 客户端-桌面/守护进程 |
| 67 | `[client] Drop the pending login flow when a login switches the profile (#7882)` | 客户端-桌面/守护进程 |
| 68 | `[management,signal] Make the Let's Encrypt challenge listener address configurable (#7706)` | 管理端/服务端 |
| 69 | `[client, management] implement certificate posture check (#7535)` | 管理端/服务端 `[+客户端]` |
| 70 | `[client, management] Harden the certificate posture client and keep challenge nonces fresh (#8052)` | 管理端/服务端 `[+客户端]` |
| 71 | `[client, management] Recover from stuck proof collections and keep re-tracked accounts on the challenge refresher (#7890)` | 管理端/服务端 `[+客户端]` |
| 72 | `[management] update proxy mappings on peer IP change (#8139)` | 管理端/服务端 |
| 73 | `[client] Configure the userspace device through its typed API (#7937)` | 客户端-桌面/守护进程 |
| 74 | `[client] Reuse one wgctrl client in the kernel configurer (#7912)` | 客户端-桌面/守护进程 |
| 75 | `[proxy] Add middleware snapshot revisions (#8114)` | 管理端/服务端 |
| 76 | `[proxy] Enforce per-target HTTP access actions (#8115)` | 管理端/服务端 |
| 77 | `[management] Configure per-target HTTP access actions (#8116)` | 管理端/服务端 |
| 78 | `[management] update reserved IP exclusion when generating peer IP (#8144)` | 管理端/服务端 |
| 79 | `[signal] Stop with an error when a server fails (#8105)` | 管理端/服务端 |
| 80 | `[proxy] Exercise target access through container E2E (#8117)` | 依赖/构建/CI/文档 |
| 81 | `[management] Create HttpConfig before applying TLS flag overrides (#8168)` | 管理端/服务端 |
| 82 | `[client] Add nblink, an unprivileged forwarder into the network (#7955)` | 客户端-桌面/守护进程 |
| 83 | `[management] setting to return error on name collision during AddPeer (#8154)` | 管理端/服务端 |
| 84 | `[client] Fix Windows UI exiting on first launch (#8138)` | 客户端-桌面/守护进程 |
| 85 | `[misc] Extend the copyright year to 2026 (#8169)` | 其他 |
| 86 | `[signal] Add a release-wired UBI image variant (#7458)` | 依赖/构建/CI/文档 |
| 87 | `[relay] Add a release-wired UBI image variant (#7457)` | 依赖/构建/CI/文档 |
| 88 | `[management] Add a release-wired UBI image variant (#8167)` | 依赖/构建/CI/文档 |
| 89 | `[management] fix peer ip allocation test (#8173)` | 依赖/构建/CI/文档 |
| 90 | `[management] do in place expand of loaded snapshots (#8177) (#8179)` | 管理端/服务端 |
| 91 | `[management] Emit activity events for batch group ops (#8180) (#8186)` | 管理端/服务端 |
| 92 | `[management] move is child account (#8195) (#8206)` | 管理端/服务端 |
| 93 | `[infrastructure] Fix Traefik Versioning and Update Crowdsec (#8152) (#8193)` | 依赖/构建/CI/文档 |
| 94 | `[client, management] Fix the edge cases left open in the certificate posture check (#8174) (#8217)` | 管理端/服务端 `[+客户端]` |

### 1.2 v0.81.1 — 2026-10-16T00:42:35Z

**release note 原文（全文，仅 1 条）：**

```
## What's Changed

[client] Keep interface discovery out of the bind open path (https://github.com/netbirdio/netbird/pull/8226) (https://github.com/netbirdio/netbird/pull/8227)

**Full Changelog**: https://github.com/netbirdio/netbird/compare/v0.81.0...v0.81.1
```

**git 区间实际提交（`compare/v0.81.0...v0.81.1`，ahead_by=17）：**

| # | 原始标题 | 标签 |
|---|---------|------|
| 1 | `[management] do in place expand of loaded snapshots (#8177)` | 管理端/服务端 |
| 2 | `[client] Bound the Signal stream registration wait and track its state based on stream health (#8175)` | 客户端-桌面/守护进程 |
| 3 | `[client] Ensure onDisconnected is called on connection teardown (#8178)` | 客户端-桌面/守护进程 |
| 4 | `[management] Emit activity events for batch group ops (#8180)` | 管理端/服务端 |
| 5 | `[client] Keep HA routing peer selection latency-aware and hold switches that reset connections (#7689)` | 客户端-桌面/守护进程 |
| 6 | `[client] Migrate macOS cask template to Homebrew install steps (#7477)` | 依赖/构建/CI/文档 |
| 7 | `[infrastructure] Fix Traefik Versioning and Update Crowdsec (#8152)` | 依赖/构建/CI/文档 |
| 8 | `[management] move is child account (#8195)` | 管理端/服务端 |
| 9 | `[infrastructure] Restrict the FreeBSD port job to stable releases (#8204)` | 依赖/构建/CI/文档 |
| 10 | `[client] Prolong HA routes on the current routing peer across relay and direct transitions (#8192)` | 客户端-桌面/守护进程 |
| 11 | `[client, management] Fix the edge cases left open in the certificate posture check (#8174)` | 管理端/服务端 `[+客户端]` |
| 12 | `[client] Detect default route changes in the Linux network monitor (#8216)` | 客户端-桌面/守护进程 |
| 13 | `[proxy,client] Add --external-ip-map flag for the embedded client (#8185)` | 客户端-桌面/守护进程 |
| 14 | `[management] cancel peer expiration for account if account is gone (#8218)` | 管理端/服务端 |
| 15 | `[management] clean nul byte strings in peer meta (#8219)` | 管理端/服务端 |
| 16 | `[management] fix proxy conn lifecycle during acks (#8220)` | 管理端/服务端 |
| 17 | `[client] Keep interface discovery out of the bind open path (#8226)` | 客户端-桌面/守护进程 |

> 上表第 1/4/8/11 条的编号形式说明 v0.81.1 的内容与 v0.81.0 的发布分支存在分叉（同样的改动在 0.81.0 里以 `(#8177) (#8179)` 形式存在）。**该版本的实际新增内容以 git 区间为准，release note 只记了最后一条。**

### 1.3 v0.81.2 — 2026-10-16T09:18:31Z

**release note 原文（全文，仅 1 条）：**

```
## What's Changed
* [client] Keep interface discovery out of the bind open path (#8226) by @mlsmaycon in https://github.com/netbirdio/netbird/pull/8227

**Full Changelog**: https://github.com/netbirdio/netbird/compare/v0.81.0...v0.81.2
```

（注意：note 里的 Full Changelog 写的是 `v0.81.0...v0.81.2`，跳过了 v0.81.1。）

**git 区间实际提交（`compare/v0.81.1...v0.81.2`，ahead_by=6）：**

| # | 原始标题 | 标签 |
|---|---------|------|
| 1 | `[management] do in place expand of loaded snapshots (#8177) (#8179)` | 管理端/服务端 |
| 2 | `[management] Emit activity events for batch group ops (#8180) (#8186)` | 管理端/服务端 |
| 3 | `[management] move is child account (#8195) (#8206)` | 管理端/服务端 |
| 4 | `[infrastructure] Fix Traefik Versioning and Update Crowdsec (#8152) (#8193)` | 依赖/构建/CI/文档 |
| 5 | `[client, management] Fix the edge cases left open in the certificate posture check (#8174) (#8217)` | 管理端/服务端 `[+客户端]` |
| 6 | `[client] Keep interface discovery out of the bind open path (#8226) (#8227)` | 客户端-桌面/守护进程 |

### 1.4 v0.81.3 — 2026-10-19T13:50:20Z

**release note 原文（全文）：**

```
## What's Changed
* [client] Bound the Signal stream registration wait and track its state based on stream health (#8175) by @riccardomanfrin in https://github.com/netbirdio/netbird/pull/8254
* [client,relay] Bump quic-go to v0.64.0 (#8249) by @riccardomanfrin in https://github.com/netbirdio/netbird/pull/8253

**Full Changelog**: https://github.com/netbirdio/netbird/compare/v0.81.2...v0.81.3
```

| # | 原始标题 | 标签 |
|---|---------|------|
| 1 | `[client,relay] Bump quic-go to v0.64.0 (#8249)` | 依赖/构建/CI/文档 |
| 2 | `[client] Bound the Signal stream registration wait and track its state based on stream health (#8175)` | 客户端-桌面/守护进程 |

（与 `compare/v0.81.2...v0.81.3` ahead_by=2 一致，无漏报。）

### 1.5 v0.81.4 — 2026-10-24T11:24:02Z（最新）

**release note 原文（全文，注意第 1、3 条是同一个 PR #8331，重复了一次）：**

```
## What's Changed
* [client] Drop SSH privileges through libc in PKCS#11 package builds (https://github.com/netbirdio/netbird/pull/8323) (https://github.com/netbirdio/netbird/pull/8331)
* [client,proxy,management,signal,relay] Distrust compromised eMudhra CA keys (#8326) by @mlsmaycon in https://github.com/netbirdio/netbird/pull/8328
* [client] Drop SSH privileges through libc in PKCS#11 package builds (#8323) by @lixmal in https://github.com/netbirdio/netbird/pull/8331


**Full Changelog**: https://github.com/netbirdio/netbird/compare/v0.81.3...v0.81.4
```

**git 区间实际提交（`compare/v0.81.3...v0.81.4`，ahead_by=2）：**

| # | 原始标题 | 标签 |
|---|---------|------|
| 1 | `[client,proxy,management,signal,relay] Distrust compromised eMudhra CA keys (#8326) (#8328)` | 依赖/构建/CI/文档（安全，影响客户端 TLS 信任） |
| 2 | `[client] Drop SSH privileges through libc in PKCS#11 package builds (#8323) (#8331)` | 客户端-桌面/守护进程（仅打包） |

### 1.6 预发布（RC / canary）内容摘要

| Tag | note 原文要点 |
|-----|--------------|
| v0.81.0-rc.1 (10-12) | `## Changelog` + 36 条 `* <sha> <原始提交标题>`（从 `[management] fix peer ip allocation test (#8173)` 到 `[management] Add a MySQL to Postgres migration tool (#8095)`），即 0.81.0 发布前的开发快照 |
| v0.81.0-rc.2 (10-13) | 2 条：`[management] Emit activity events for batch group ops (#8180) (#8186)`、`[management] do in place expand of loaded snapshots (#8177) (#8179)` |
| v0.81.0-rc.3 (10-14) | 2 条：`[management] move is child account (#8195)` by @pascal-fischer、`[infrastructure] Fix Traefik Versioning and Update Crowdsec (#8152)` by @bcmmbaga |
| v0.81.0-canary.pr-8192.1 (10-14) | 仅一句描述：`Provide time hysteresis on route switching over HA group routing peers with same metrics` |
| v0.81.0-canary.pr-7407.2 (10-21) | `Only testable code based on ongoing development.` |

结论：RC/canary **没有引入 final 之外的内容**，final 版本已覆盖；不必单独跟进。

---

## 2. Breaking change / 需要客户端配合的变更（单列）

**先给结论性事实：v0.81.0 / v0.81.1 / v0.81.2 / v0.81.3 / v0.81.4 的 release note 里都没有 "Breaking Changes" 或 "Upgrade Notes" 小节**（已核对全文，见 1.1–1.5）。下面是从 note 正文、PR 正文和 proto/代码二进制差异中提取的、**真正需要客户端或其他组件配合**的条目。

### B1. 证书姿态检查（Certificate posture check）——第三方/移动客户端会**直接判失败**（最高优先级）

原文依据（v0.81.0 note，原文照抄）：

> "Requires management and client **v0.81.0** or newer. **The iOS and Android apps do not support certificate proofs and always fail this check.**"

原文依据（v0.81.0 note「Security and Access Control」）：

> "Added the **Certificate posture check**, with a hardened client, fresh challenge nonces, recovery from stuck proof collections, and fixes for remaining edge cases. #7535, #8052, #7890, #8174"

协议层依据（`shared/management/proto/management.proto`，v0.80.0 → v0.81.4 差异，原文）：

```proto
 message Checks {
   repeated string Files = 1;
+  // certificateChallenge asks the peer to prove possession of a certificate chaining to caCertificates.
+  CertificateChallenge certificateChallenge = 2;
+}
+
+message CertificateChallenge {
+  // nonce is issued by management, bound to the peer and a time window; the peer signs it.
+  bytes nonce = 1;
+  // caCertificates are PEM encoded trust anchors the presented certificate must chain to.
+  repeated string caCertificates = 2;
+}
+
+// CertificateProof demonstrates possession of the private key of chain[0] by signing the challenge nonce.
+message CertificateProof {
+  bytes nonce = 1;
+  // chain is DER encoded, leaf first.
+  repeated bytes chain = 2;
+  string sigAlg = 3;
+  bytes signature = 4;
+}
```

```proto
 message PeerSystemMeta {
   ...
   repeated PeerCapability capabilities = 18;
   int32 syncMessageVersion = 19;
+  repeated CertificateProof certificateProofs = 20;
 }
```

代码层依据：新增包 `shared/management/certposture/`（`chain.go`、`challenge.go`、`proof.go`、`certtest/`）与 `shared/management/networkmap/nmdata/posture_certificate.go` 在 v0.81.4 存在、v0.80.0 不存在（git tree 对比）。

**影响**：management 会通过 `Checks.certificateChallenge` 下发 nonce + 信任锚，客户端须签名后经 `PeerSystemMeta.certificateProofs` 回传（"Management repeats the challenge every 4 hours while the peer is connected"）。**不实现该能力的 peer 在该策略下恒为失败**，与 note 对 iOS/Android 的描述一致。HarmonyOS 客户端若要与官方移动端对齐，此处的官方基线就是"不支持、会失败"；若要支持，需自行实现 chain+sign（官方只有 Windows/macOS/Linux 实现可参考）。

### B2. `PKCEAuthorizationFlowRequest.SessionExtend`——延长 SSO 会话时的新字段

原文依据（proto 新增内容，注释照抄）：

```proto
-// PKCEAuthorizationFlowRequest empty struct for future expansion
-message PKCEAuthorizationFlowRequest {}
+// PKCEAuthorizationFlowRequest asks for the PKCE flow configuration to use for
+// an upcoming authorization request.
+message PKCEAuthorizationFlowRequest {
+  // SessionExtend indicates the flow will renew the SSO session of a peer that
+  // is already registered, rather than log in or register one. An extend is
+  // bound to the account that peer belongs to, so the server must not answer it
+  // with a configuration that lets the IdP reply from whatever session is
+  // already active: with several accounts signed in at the IdP that need not be
+  // the peer's own, and the resulting token is rejected as a peer/user mismatch
+  // with no way for the user to correct it.
+  bool SessionExtend = 1;
+}
```

配套客户端改动：`[client] Force interactive login when extending the auth session (#7216)`、`[client] Evaluate the session deadline against the wall clock (#7959)`、`[client] Drop the pending login flow when a login switches the profile (#7882)`。

**影响**：新增可选字段，protobuf 默认 false，旧客户端不发也不会被拒；但多账号 IdP 场景下的 "peer/user mismatch" 只有发送 `SessionExtend=true` 才能被服务端正确规避。属"建议跟进"，非硬性破坏。

### B3. Ingress 端口转发（port forwarding）被移除——客户端必须删功能

原文依据（v0.81.0 note「Management Improvements」）：

> "**Removed ingress ports**. #8062"

协议层依据（两处 `deprecated`，注释照抄）：

```proto
-  repeated ForwardingRule forwardingRules = 12;
+  // Unused; the ingress port-forwarding feature was discontinued.
+  repeated ForwardingRule forwardingRules = 12 [deprecated = true];
```

```proto
-  rpc ForwardingRules(EmptyRequest) returns (ForwardingRulesResponse) {}
+  // Unused; the ingress port-forwarding feature was discontinued.
+  rpc ForwardingRules(EmptyRequest) returns (ForwardingRulesResponse) {
+    option deprecated = true;
+  }
```

```proto
-  int32 NumberOfForwardingRules = 8;
+  // Unused; the ingress port-forwarding feature was discontinued.
+  int32 NumberOfForwardingRules = 8 [deprecated = true];
```

另有 `shared/management/client/rest/ingress.go` 与 `ingress_test.go` 在 v0.81.4 被**删除**（git tree 对比）。

**影响**：字段号保留（wire 兼容），但服务端不再生成 forwarding 规则。HarmonyOS 客户端若实现了 ingress 端口转发，该功能在上游已不存在对应服务端支撑。

### B4. eMudhra 中间 CA 被 distrust + Go 工具链 1.26.7 → 1.26.10（CVE-2026-111943）

原文依据（PR #8326 正文，照抄）：

> "Private keys for eight eMudhra intermediate CAs were compromised. Browsers have blocked them since 2026-10-17 … Go has no blocklist of its own, so our binaries accept a chain through those intermediates whenever the trust store still holds `emSign ECC Root CA - G3`."
>
> "**Go toolchain 1.26.7 → 1.26.10** (`go.mod`, every `golang:` builder image, the privileged test runner). In go1.26.10, `crypto/x509` rejects any chain containing one of the eight keys (CVE-2026-111943). This applies on Linux, macOS and Windows, regardless of the trust store."
>
> "go1.26.7   dv-g3b / dv-g3c / ov-g3b / ev-g3c   ACCEPTED
> go1.26.10  dv-g3b / dv-g3c / ov-g3b / ev-g3c   REJECTED: x509: no valid chains built"

改动文件（PR #8326 共 **21 个**：18 个 Dockerfile / CI 文件的 `golang:` 版本行 + `go.mod` + `docs/testing-privileged.md` + `client/testutil/privileged/runner_test.go`）：`.devcontainer/Dockerfile`、`.github/workflows/golang-test-linux.yml`、`client/Dockerfile-rootless.ubi`、`client/internal/metrics/infra/ingest/Dockerfile`、`client/testutil/privileged/runner_test.go`、`client/ui/build/docker/Dockerfile.cross`、`client/ui/build/docker/Dockerfile.server`、`combined/Dockerfile.multistage`、`combined/Dockerfile.ubi`、`docs/testing-privileged.md`、`e2e/harness/Dockerfile.client`、`e2e/harness/Dockerfile.nblink`、`go.mod`、`management/Dockerfile.multistage`、`management/Dockerfile.ubi`、`proxy/Dockerfile`、`proxy/Dockerfile.multistage`、`proxy/Dockerfile.ubi`、`relay/Dockerfile.ubi`、`signal/Dockerfile.ubi`、`upload-server/Dockerfile`。

**影响**：如果 HarmonyOS 客户端用 Go 编译（gomobile/netbird core），**必须**把工具链升到 ≥ go1.26.10，否则对 management/signal/relay 的 TLS 链校验会接受被攻破的中间 CA。若走系统 TLS（ArkTS/HarmonyOS 网络栈），则取决于系统根证书库是否已移除 `emSign ECC Root CA - G3` 下的这些中间证书——客户端自身无法通过"信任名单"规避，需关注系统更新。这是本次区间内**唯一的安全级必跟项**。

### B5. Reverse Proxy 按 target 的 access action——升级顺序有约束（服务端侧）

原文依据（v0.81.0 note）：

> "Upgrade every proxy in the cluster before using these actions; older proxies ignore them and keep applying the service authentication. **Bypass authentication** is unavailable for NetBird-Only services and Agent Network targets."

协议层依据（`shared/management/proto/proxy_service.proto` 新增）：

```proto
+enum TargetAccessAction {
+  TARGET_ACCESS_ACTION_INHERIT = 0;
+  TARGET_ACCESS_ACTION_BYPASS = 1;
+  TARGET_ACCESS_ACTION_BLOCK = 2;
+}
...
   PathTargetOptions options = 3;
+  TargetAccessAction access_action = 4;
```

**影响**：proxy↔management 内部协议，客户端不参与。仅自托管部署需要注意"先升 proxy 再启用"。

### B6. Proxy 会话校验：`session_token` 弃用，改用单次 `session_code`（服务端侧）

原文依据（`proxy_service.proto`）：

```proto
 message ValidateSessionRequest {
   string domain = 1;
-  string session_token = 2;
+  string session_token = 2 [deprecated = true];
+  // session_code is a short-lived, single-use code exchanged for a session token.
+  string session_code = 3;
 }
...
   repeated string peer_group_names = 6;
+  // session_token contains the durable token issued when session_code is redeemed.
+  string session_token = 7;
```

配套：`[management,proxy] Use single-use codes for OIDC session handoff (#7635)`、`[management,proxy] Rename the OIDC session code query parameter (#7981)`。

**影响**：proxy↔management 内部协议。若 HarmonyOS 客户端不实现 reverse-proxy SSO 会话校验，则无关。

### B7. MySQL store 弃用（部署侧）

原文依据（v0.81.0 note）：

> "### Deprecation: MySQL store — MySQL support for the Management store is deprecated and will be removed in an upcoming release. If your self-hosted deployment runs on MySQL, migrate to PostgreSQL before then. SQLite and PostgreSQL remain fully supported."

配套 PR：`[management] Add a MySQL to Postgres migration tool (#8095)`（新增 `netbird-mysql-migrate`）。

**影响**：与客户端无关；若项目的测试/演示环境用 MySQL 需提前迁移。

### B8. 其他"看起来像破坏性但其实不是"的项（已核实，供排除）

| 项 | 依据 | 核实结果 |
|----|------|---------|
| Signal 协议是否变了 | `shared/signal/proto/signalexchange.proto` v0.80.0 vs v0.81.4 **逐字节相同**（2634 bytes，diff 为空） | **无变化**，无需跟进 |
| Relay 协议是否变了 | `relay/protocol/protocol.go` 与 `relay/server/handshake.go` 在 v0.80.0 与 v0.81.4 的 git blob sha **均相同** | **无变化**，无需跟进 |
| Flow 遥测协议是否变了 | `flow/proto/flow.proto` 两版本**逐字节相同**（2061 bytes） | 无变化 |
| 客户端 daemon gRPC 是否破坏 | `client/proto/daemon.proto` 实际增删仅 **14 行**，全部是 `deprecated = true` 与注释（见 B3） | 只有 deprecation，无字段删除/重编号 |
| management.proto 是否有字段删除或重编号 | 完整 diff 138 行：新增 `certificateProofs=20`、`certificateChallenge=2`、`SessionExtend=1`、`CertificateChallenge`/`CertificateProof` 两个 message；其余为 `[deprecated = true]` + 注释改写；**无字段号复用、无类型变更** | **纯增量 + 弃用**，老客户端仍可解析 |
| 是否新增了必须声明的 PeerCapability | `PeerCapability` 枚举（`PeerCapabilityUnknown/SourcePrefixes/IPv6Overlay/ComponentNetworkMap`）两版本一致，未新增 | 无需新增能力位 |
| `syncMessageVersion` 是否抬高门槛 | 字段本身两版本相同（`int32 syncMessageVersion = 19`），差值仅新增 20 号字段 | 未见"要求更高版本才允许连接"的依据；**未进一步核实服务端是否据此拒绝旧版本**（未读取 management 校验逻辑） |
| 客户端配置文件/状态文件格式是否变 | 区间内无 `client/internal/config` 相关提交标题；未找到 release note 或 PR 标题提及 config/state 迁移 | 未获取到格式变更证据 |
| OIDC 会话交接改单次码 | `[management,proxy] Use single-use codes for OIDC session handoff (#7635)`、`…Rename the OIDC session code query parameter (#7981)` | proxy↔management 之间，客户端不参与 |

---

## 3. 移动端客户端（Android / iOS）同期发布

前提确认：这两个仓库与 Go core **不做版本号同步**，release note 里也不写内嵌的 core 版本。以下版本号/日期为各仓库自己的 tag。

### 3.1 netbirdio/android-client

| Tag | 日期 (UTC) | 关键变更（原始标题） |
|-----|-----------|---------------------|
| v0.6.2 | 2026-10-23T14:39:00Z | 见下（8 条） |
| v0.6.2-rc.1 | 2026-10-12T14:28:43Z | 与 v0.6.2 **完全相同**的 8 条（同一批提交，tag 不同） |
| v0.6.1 | 2026-09-22T08:48:45Z | `Fix DoT, keep the tunnel resolver when Private DNS is in Automatic mode` (#266) |
| v0.6.0 | 2026-09-09T10:03:00Z | UI redesign / Split tunnelling / SSH client / Session expiration / 网络与配置修复（大版本，见下） |

v0.6.2 正文原文（8 条，逐条）：

- `Point bug reports and feature requests to netbird Discussions` (#280)
- `Add an anonymization level picker to the debug bundle screen` (#263)
- `Schedule session-expiry warnings with WorkManager` (#281)
- `Keep Go calls off the Go callback thread and the main thread` (#271)
- `Generate debug bundle to file` (#267)
- `Add MDM support to the Android client` (#278)
- `Bump submodule` (#284)
- `Derive the netbird version from release-branch tags too` (#285)

v0.6.0 正文要点（原文小标题 + 关键行）：`UI redesign`（新首页大开关、peer 详情页、exit node 选择器、profile 管理、底部导航、深色主题统一、9 语言本地化与语言选择器、按连接状态的 4 种通知图标、仅手机锁竖屏）；`Split tunnelling (new)`（按 profile 存应用列表，未选则全量走隧道）；`SSH client (new)`；`Session expiration (new)`（横幅 + 通知显示 SSO 会话剩余时间，点击立即续期）；`Network / connectivity`（数据网络切换后快速重连、网络变化不重启引擎、TUN 仅在路由变化时重建、卡住的连接可从主开关取消）；`Profiles`（迁移到 profile id、`Use Build.MODEL for device hostname instead of Build.PRODUCT`）；`Bug fixes`（Android 16/17 支持等 14 条）。

### 3.2 netbirdio/ios-client

| Tag | 日期 (UTC) | 关键变更（原始标题；v0.4.0 共 29 条，此处摘录，完整列表见该 release 页面） |
|-----|-----------|--------------------------------------------------------|
| v0.4.1 | 2026-09-24T08:57:53Z | `Show exit nodes right after connect without visiting another tab` (#232)、`Add a Troubleshoot toggle for remote debug bundle jobs` (#216)、`Bump netbird core to main head` (#234)；附 TestFlight：iOS `0.4.1 (2)`、tvOS `0.4.1 (2)` |
| v0.4.0 | 2026-09-22T08:53:06Z | 29 条，含 `feat(tvos): tvOS UI redesign` (#179)、`Add dedicated exit node selection UI` (#170)、`Mdm integration` (#206)、`Deliver the MDM policy to the network extension through the App Group` (#227)、`Fix/ios reduce login requests` (#177)、`fix: stop using an ephemeral browser session for the OAuth login` (#204)、`fix(ios): repeated 2FA prompt during SSO re-login (iOS)` (#175)、`Drop the dedicated server URL file` (#202)、`Refactor own profile management in the Go core` (#147) 等 |

**关键事实：iOS 客户端 2026-09-24（v0.4.1）之后到 2026-10-24 没有任何新 release**（该仓库最新 release 就是 v0.4.1）。因此 v0.80.0→v0.81.4 期间，iOS 侧"没有对应发布"，Android 侧有 v0.6.2-rc.1(10-12) 与 v0.6.2(10-23) 两次。

### 3.3 移动客户端内嵌的 netbird core 提交（实测，非 note 声明）

用各 tag 的 git tree 中的 submodule 指针查得：

| 客户端版本 | submodule | core commit | 该提交标题 / 日期 | 相对 v0.81.0 |
|-----------|-----------|-------------|------------------|--------------|
| android-client v0.6.2 | `netbird` | `01185ba3536bcc…` | `[client] Configure the userspace device through its typed API (#7937)` / 2026-10-12T08:29:26Z | **落后 21 个提交**（`compare/01185ba...v0.81.0` → ahead_by=21, behind_by=0） |
| android-client v0.6.0 | `netbird` | `76ea72237f33…` | `[management] Add Agent Network managed proxy to the API spec (#7433)` / 2026-09-04T15:11:27Z | 更早（`compare/76ea7223...v0.80.0` → 落后 77 个提交） |
| ios-client v0.4.1 | `netbird-core` | `cb7ca8ef3f02…` | `[client,management] Skip route firewall rule computation when no firewall (#7624)` / 2026-09-23T09:50:44Z | **落后 116 个提交**；相对 v0.80.0 也落后 27 个提交 |

**含义**：连官方移动端都没有跟上 0.81.x——Android v0.6.2（10-23 发布）内嵌的 core 是 0.81.0-rc 时期（10-12）的提交，**不含 v0.81.0 最终版及之后的所有补丁**（含 cert posture 的边缘修复 #8174、信号流注册修复 #8175、eMudhra CA 修复 #8326）；iOS 更早，内嵌 core 在 v0.80.0 之前。这佐证了"移动客户端与 Go core 版本不同步"这一前提，也说明**不能用移动端版本号推断 core 行为**。

---

## 4. 证据清单

### 4.1 成功抓取（HTTP 200）

GitHub API（结构化数据，正文完整、未被截断）：
- `https://api.github.com/repos/netbirdio/netbird/releases?per_page=30` — 30 个 release 全文（含 v0.81.0 的 20004 字符正文），落盘 `releases.json`
- `https://api.github.com/repos/netbirdio/netbird/tags?per_page=100` — 确认无 v0.80.1、v0.81.4 为最新 tag
- `https://api.github.com/repos/netbirdio/netbird/compare/v0.80.0...v0.81.0` — 94 commits（merge_base `a8162aaa`）
- `https://api.github.com/repos/netbirdio/netbird/compare/v0.81.0...v0.81.1` — 17 commits
- `https://api.github.com/repos/netbirdio/netbird/compare/v0.81.1...v0.81.2` — 6 commits
- `https://api.github.com/repos/netbirdio/netbird/compare/v0.81.2...v0.81.3` — 2 commits
- `https://api.github.com/repos/netbirdio/netbird/compare/v0.81.3...v0.81.4` — 2 commits
- `https://api.github.com/repos/netbirdio/netbird/compare/01185ba3...v0.81.0` / `...v0.81.4`（Android core 定位）
- `https://api.github.com/repos/netbirdio/netbird/compare/cb7ca8ef...v0.80.0` / `...v0.81.0` / `...v0.81.4`（iOS core 定位）
- `https://api.github.com/repos/netbirdio/netbird/git/trees/v0.80.0?recursive=1`（3784 项）与 `…/git/trees/v0.81.4?recursive=1`（3994 项）— 用于文件级存在性/内容对比
- `https://api.github.com/repos/netbirdio/netbird/commits/01185ba3…`、`…/cb7ca8ef…`、`…/76ea7223…` — 提交日期与标题
- `https://api.github.com/repos/netbirdio/netbird/commits?path=shared/management/http/apiv1alpha1&sha=v0.81.4` — 确认该目录由 #8056 引入
- `https://api.github.com/repos/netbirdio/netbird/pulls/8326` 与 `…/pulls/8326/files?per_page=50` — eMudhra PR 正文与 21 个改动文件
- `https://api.github.com/repos/netbirdio/android-client/releases?per_page=30`（28 个 release）
- `https://api.github.com/repos/netbirdio/ios-client/releases?per_page=30`（14 个 release）
- `https://api.github.com/repos/netbirdio/android-client/git/trees/v0.6.2?recursive=1`、`…/v0.6.0`、`…/ios-client/git/trees/v0.4.1`、`…/v0.4.0` — submodule 指针

raw 文件（两版本对拉后 diff）：
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/shared/management/proto/management.proto` 与 `…/v0.81.4/…`（43054 → 43757 bytes）
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/shared/management/proto/proxy_service.proto` 与 `…/v0.81.4/…`（19456 → 19871）
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/shared/signal/proto/signalexchange.proto` 与 `…/v0.81.4/…`（均 2634，**IDENTICAL**）
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/flow/proto/flow.proto` 与 `…/v0.81.4/…`（均 2061，**IDENTICAL**）
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/client/proto/daemon.proto` 与 `…/v0.81.4/…`（实际增删 14 行，全为 deprecated 与注释）
- `https://raw.githubusercontent.com/netbirdio/netbird/v0.81.4/shared/management/http/apiv1alpha1/peer/peers.yaml`（新 API spec 片段）

HTML 页面（可访问性核验）：
- `https://github.com/netbirdio/netbird/releases` — HTTP 200
- `https://github.com/netbirdio/netbird/releases/tag/v0.81.0` — HTTP 200
- `https://github.com/netbirdio/netbird/releases/tag/v0.81.1` — HTTP 200，并用 web_fetch 取回渲染后的正文，与 API body 一致（页脚另显示 "32 commits to main since this release"）
- `https://github.com/netbirdio/netbird/compare/v0.80.0...v0.81.4` — HTTP 200（内容用 API 等价接口逐提交获取，未逐行渲染 HTML）
- `https://github.com/netbirdio/android-client/releases`、`https://github.com/netbirdio/ios-client/releases` — HTTP 200

### 4.2 抓取失败 / 不存在

| URL | 结果 | 处理 |
|-----|------|------|
| `https://raw.githubusercontent.com/netbirdio/netbird/main/CHANGELOG.md` | **404 — 该仓库没有 CHANGELOG 文件** | 已用 v0.81.4 全树核对：根目录只有 `release_files/rpm-changelog.sh`（RPM 打包脚本），无任何 CHANGELOG。改用 releases + compare API；未获取到 CHANGELOG 内容（不存在，非抓取失败） |
| `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/management/proto/management.proto`（及 v0.81.4） | 404 | 路径猜测错误，实际在 `shared/management/proto/`，已改抓成功 |
| `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/signal/proto/signalexchange.proto`（及 v0.81.4） | 404 | 同上，实际在 `shared/signal/proto/` |
| `https://raw.githubusercontent.com/netbirdio/netbird/v0.80.0/relay/proto/relay.proto`（及 v0.81.4） | 404 | **不存在该文件**；relay 协议是 Go 代码 `relay/protocol/protocol.go`，已改用 git tree blob sha 对比（两版本相同） |
| `https://api.github.com/repos/netbirdio/netbird/contents/management/proto?ref=v0.81.4`、`…/signal/proto`、`…/relay/proto` | 404 | 同上，目录不存在 |

### 4.3 未获取到 / 未核实（明确留白）

- **release note 之外的"官方 breaking change 说明"**：未找到。netbird 仓库没有 CHANGELOG，五个 release 也都没有 Breaking/Upgrade 小节，因此 B1–B8 是从 note 正文、PR 正文与 proto 差异中整理的，不是官方汇总名单。
- **management 服务端是否按 `syncMessageVersion` 拒绝旧客户端**：未读 management 校验代码，**未核实**。已核实的是"协议字段为纯增量"。
- **`SessionExtend` 是否在服务端强制**：只读到 proto 注释，未读 management/客户端实现，**未核实**其生效条件。
- **v0.81.1 与 v0.81.0 的分支拓扑**（为何 v0.81.1 的 note 只有 1 条、区间却有 17 个提交）：只确认了现象（note 漏报 + GitHub 页面显示 32 commits），**未获取到**发布流程说明。
- **dashboard v2.95.0 的 14 条变更**：只在 v0.81.0 note 里读到，未单独抓取 dashboard 仓库 release 正文。

---

## 5. 给评估用的三句话结论

1. **协议层不需要"跟随"**：`signalexchange.proto` 与 `flow.proto` 逐字节未变、`relay/protocol` 未变、`management.proto` 只有新增字段和弃用标记，一个已按 0.80.0 实现的客户端在 0.81.x 上仍能正常 register/sync/连接。
2. **有两个功能性跟进点**：证书姿态检查（不实现则该策略下恒失败，官方移动端也是如此）与 `SessionExtend`（延长 SSO 会话的正确性）；外加一个**安全必跟点**：若用 Go 构建，工具链须 ≥ go1.26.10（eMudhra 中间 CA 失陷，CVE-2026-111943）。
3. **官方移动端自身也没跟 0.81.x**（Android v0.6.2 内嵌 core 落后 21 个提交、iOS v0.4.1 落后 116 个），所以"官方移动端已支持某能力"不能作为 HarmonyOS 的对齐基线；能作为基线的只有 Go core 的代码与 proto。
