# NetBird 上游变更盘点：v0.79.0 与 v0.80.0

> 配套文档：`.upstream-eval/netbird-upstream-0.80.0-to-0.81.4.md`（0.80.0→0.81.4 已另有报告，本文不重复）。
> 抓取时间：2026-10-24T21:43Z–21:47Z（Asia/Shanghai 2026-10-25 05:43 前后）。
> 抓取方式：GitHub 公开 REST API（releases / compare / pulls）+ releases HTML 页，全程只读。
> 口径：release note 原文一律照抄英文，中文为一句说明；提交标题（commit.message 首行）原样保留。

**证据基础**：`releases/tags/v0.79.0` 与 `releases/tags/v0.80.0` 均 200，body 全文（9344 / 14092 字符）已完整读取，非截断摘要；`compare/v0.78.1...v0.79.0` 44 个提交、`compare/v0.79.0...v0.80.0` 47 个提交，两者 `total_commits` 与返回数组长度相等（未触发 compare API 的 250 条截断上限）。

---

## 0. 先说四个会改变评估结论的发现

### 0.1 锚点 commit `7914010` 并不在 v0.78.1 上，它已经在 v0.79.0 区间内部

`7914010` 在 `compare/v0.78.1...v0.79.0` 的 44 个提交里排第 **38** 个（索引 37），完整 sha `791401060d2b95e5f51e3439c0649729132f571e`：

```
79140106 2026-09-12T10:37:23Z [management] Prevent deleting groups referenced by agent network budget rules (#7450)
```

日期 2026-09-12 与任务描述吻合，可确认就是这个提交。它之后、v0.79.0 发布之前还有 **6 个提交**（即第三方客户端连 0.79.0 区间尾部都还没吃到）：

| # | sha | 日期 | 提交标题 |
|---|---|---|---|
| 39 | `0f797f89` | 2026-09-14 | `[client] Bump wireguard-go to 8bf8fa968f1a (#7532)` |
| 40 | `2d28f900` | 2026-09-14 | `[client] Fix the Windows tray deadlock on re-entrant window creation (#7449)` |
| 41 | `a54d96cd` | 2026-09-14 | `[proxy] Make the upstream HTTP version configurable (#7410)` |
| 42 | `7cbf8234` | 2026-09-17 | `[client] Fix - Add RPM metadata required for Red Hat software certification (#7562) (#7573)` |
| 43 | `0ce7617b` | 2026-09-17 | `[management] Read X-Real-IP when extracting the peer connection IP (#7561) (#7577)` |
| 44 | `396795eb` | 2026-09-18 | `[management] Handle empty trusted peer (#7589)` |

**含义**：真实缺口不是「0.78.1 → 0.81.4」，而是「6 个提交（0.79.0 尾部）+ 47 个提交（0.80.0）+ 0.80.0→0.81.4」。说「锚定 v0.78.1 附近」会**高估**缺口规模约 37 个提交，其中还包括本文第 4 节里最关键的几条（relay 地址竞争、DNS DNAT、Android TUN 前缀等）——这些**已经在 7914010 里了**，不是缺口。

### 0.2 两个版本都**没有** Breaking Changes / Upgrade Notes / Deprecation 小节

v0.79.0 的标题层级只有：`New Feature: Desktop Light Mode` / `What's Changed` / 7 个 `####` 分组 / `Full Changelog`。
v0.80.0 的标题层级只有：`What's New` / 6 个 `###` 分组 / `What's Changed` / `New Contributors` / `Full Changelog`。

全文检索 `breaking|deprecat|upgrade|migrat|action required|important|warning` 只命中 5 处，且没有一处是小节标题（详见各版本 1.3 / 2.3 节按原文照抄的段落）。**结论：不能声称官方标注了 breaking change，只能声称「官方未设该小节，但正文里有 N 处带升级含义的表述」。**

### 0.3 这两版的 note 与 git 区间几乎完全一致——与 0.81.x 的严重漏报形成鲜明对比

- v0.79.0：note 里出现 46 个 PR 号，区间提交里也出现 46 个 PR 号，**两个方向的差集都为空**。
- v0.80.0：`What's Changed` 原始清单 47 个 bullet，与区间 47 个提交**一一对应**； curated `What's New` 覆盖全部 47 项，另多出一个幽灵条目 #7222。

也就是说，0.79.0/0.80.0 的 note 是**可信的完整清单**（0.81.1/0.81.2 那种「1 条 note 对 17 个提交」的漏报在这两版不存在）。这对评估方法是好消息：这两版可以直接以 note 为准，不需要像 0.81.x 那样以 git 区间反推。

### 0.4 v0.79.0 的 tag 打在 release 分支上，与 main 有 3 个「同内容不同 sha」的 cherry-pick

`compare/v0.79.0...v0.80.0` 返回 `status: diverged, ahead_by: 47, behind_by: 3`。那 3 个「v0.79.0 有而 v0.80.0 没有」的提交是：

- `7cbf8234` `[client] Fix - Add RPM metadata required for Red Hat software certification (#7562) (#7573)`
- `0ce7617b` `[management] Read X-Real-IP when extracting the peer connection IP (#7561) (#7577)`
- `396795eb` `[management] Handle empty trusted peer (#7589)`

它们的内容在 main 上以 `85a39133 (#7562)`、`f8c3e565 (#7561)`、`7d8f4fa3 (#7589)` 的形式重新落地（所以同时出现在 0.79.0→0.80.0 的区间里）。**不是三次功能丢失，是三次 cherry-pick**。同理 `compare/v0.78.1...v0.79.0` 的 `behind_by: 1` 是 `23a1487c (#7424)`，其等价修复在 main 上是 `8dc42725 (#7418)`。

补充：v0.78.0 → v0.78.1 只有一个提交（`23a1487c`，2026-09-04，`[management] Serve networks with peer-based routers from the SQLite network map (#7424)`），说明 **v0.78.1 是只含一个后端修复的窄补丁版**，而该提交本身不在 v0.79.0 的历史里。

---

## 1. v0.79.0 — 2026-09-18T17:06:28Z

- tag：`v0.79.0`，name：`v0.79.0`，prerelease：false，draft：false
- published_at：**2026-09-18T17:06:28Z**（created_at 2026-09-18T17:03:58Z）
- 页面：https://github.com/netbirdio/netbird/releases/tag/v0.79.0
- note 自带的 Full Changelog 链接写的是 **`v0.78.0...v0.79.0`**（不是 v0.78.1）
- 区间提交数：44（`v0.78.1...v0.79.0`）

### 1.1 Highlights（原文）

note 没有 `Highlights` 字样的小节，但有一个置顶新功能小节 + 一段导语，功能上就是 highlights：

**`### New Feature: Desktop Light Mode`**（原文）

> "The desktop app now gives you three appearance options: System, Light, and Dark. Follow your operating system's theme or choose the look you prefer. The new light theme covers the connection view, settings, profiles, and dialogs, with matching native window appearance on Windows, macOS, and Linux. [#7344](https://github.com/netbirdio/netbird/pull/7344) by @TechHutTV"

中文：桌面端新增 System / Light / Dark 三种外观，浅色主题覆盖连接页、设置、配置文件和对话框，并让 Windows/macOS/Linux 原生窗口外观跟随。

导语（原文）：

> "This release also brings shared MDM policy enforcement to the mobile SDKs, a rootless Red Hat UBI container image, and improvements to DNS, relay connections, and reverse proxy access controls."

中文：本版还把共享 MDM 策略强制下推到移动 SDK、提供 rootless Red Hat UBI 镜像，并改进了 DNS、relay 连接与反向代理访问控制。

### 1.2 What's Changed（按官方分组，原文 + 中文一句）

以下 7 个分组名、41 个 bullet 全部来自 note 原文，`####` 层级照抄。

#### Desktop Client Improvements（2 条）

1. "Fixed a Windows tray deadlock that could freeze the app when a double-click opened a window while another window was still being created. [#7449] by @pappz"
   → 修复 Windows 托盘死锁：双击开新窗口恰好撞上另一个窗口正在创建时会把应用卡死。
2. "Fixed the MDM settings snapshot so the UI correctly reports when remote jobs are managed by policy. [#7485] by @pappz"
   → 修正 MDM 设置快照，UI 现在能正确显示「远端作业受策略管控」。
   （note 原文见 `#### Desktop Client Improvements`；这两条在 0.80.0 没有对应的追加。）

#### Client Improvements（10 条）

3. "Added MDM policy bridges and shared enforcement for the iOS and Android SDKs, keeping managed settings and profile restrictions consistent with the desktop client. [#6435] by @riccardomanfrin"
   → 为 iOS/Android SDK 加 MDM 策略桥与共享强制逻辑，使托管设置与配置文件限制和桌面端一致。
4. "Accepted MDM boolean values delivered as JSON numbers, so policies using `0` and `1` are applied correctly. [#7471] by @riccardomanfrin"
   → 接受以 JSON 数字（`0`/`1`）下发的 MDM 布尔值，此前这类策略不生效。
5. "Compared MDM-managed URLs by their normalized endpoints, avoiding false conflicts between equivalent URLs. [#7472] by @riccardomanfrin"
   → MDM 托管 URL 改为按规范化 endpoint 比较，避免等价 URL 被误判为冲突。
6. "Refreshed system information on every management sync reconnect, keeping local network addresses and posture information current after network changes. [#7409] by @pappz"
   → 每次 management 同步流重连时重新采集系统信息，保证网络变化后本机地址与 posture 信息是最新的。
7. "Used host prefixes for Android TUN addresses so local network protection does not classify the entire overlay as a local network. [#7414] by @pappz"
   → Android TUN 地址改用 host 前缀，避免「本地网络保护」把整个 overlay 判成本地网络。
8. "Replaced the eBPF DNS forwarder with UDP and TCP DNAT rules when the resolver cannot listen on port 53, with rollback and cleanup for incomplete redirects. [#7439] by @lixmal"
   → 当解析器无法监听 53 端口时，用 UDP/TCP DNAT 规则替代 eBPF DNS 转发器，并带有不完整重定向的回滚与清理。
9. "Fixed a relay address race that could advertise a URL and IP from different connections during a reconnect. [#7498] by @pappz"
   → 修复 relay 地址竞争：重连期间可能把来自两个连接的一条 URL 和一个 IP 拼在一起对外通告。
10. "Returned the context cancellation or timeout error when an SSH handshake is interrupted. [#7426] by @pappz"
    → SSH 握手被中断时返回 context 取消/超时错误，而不是笼统失败。
11. "Updated wireguard-go to `8bf8fa968f1a`, fixing keepalive buffer-pool stalls, keeping timer paths non-blocking, and making netstack interface shutdown idempotent. [#7532] by @pappz"
    → wireguard-go 升到 `8bf8fa968f1a`，修 keepalive 缓冲区池卡住、timer 路径非阻塞、netstack 接口关停幂等。
12. "Allowed buffer-pool limits to be adjusted while a device is stalled, and bounded proxy-wide updates so one stuck client does not block the others. [#7452] by @riccardomanfrin"
    → 设备卡住时仍可调整缓冲区池上限；proxy 全局限流更新有界，单个卡死客户端不再阻塞其他客户端。

#### Management Improvements（8 条）

13. "Restored networks using individual peers as routers in the SQLite network map when `peer_groups` is empty or null. [#7418] by @mlsmaycon"
    → `peer_groups` 为空或 null 时，恢复「以单个 peer 作路由器」的 network 在 SQLite network map 中的呈现。
14. "Fixed SQLite network-map reads for users with null automatic groups and expanded coverage for empty and null router groups. [#7425] by @dmitri-netbird"
    → 修复 automatic groups 为 null 的用户读取 SQLite network map 失败，并扩大空/null router group 的覆盖。
15. "Included offline peers when scheduling login expiration and ensured expired peers are disconnected, while protecting peers that have just logged in again. [#7467] by @pascal-fischer"
    → 登录到期调度把离线 peer 也纳入，确保到期 peer 被断开，同时保护刚刚重新登录的 peer 不被误断。
16. "Applied duplicate-key sync protection to user-owned peers as well as peers registered with setup keys. [#7427] by @pascal-fischer"
    → 重复 key 的同步保护从 setup key 注册的 peer 扩展到用户自有 peer。
17. "Validated that a peer exists before adding it to a group. [#7486] by @pascal-fischer"
    → 加组前先校验 peer 存在。
18. "Prevented other users from deleting the account owner. [#7456] by @pascal-fischer"
    → 阻止非 owner 用户删除账户所有者。
19. "Hardened OIDC issuer validation by requiring HTTPS, rejecting credentials, query strings, and fragments in issuer URLs, refusing discovery redirects, and limiting discovery response size. [#7435] by @bcmmbaga"
    → 加固 OIDC issuer 校验：强制 HTTPS、拒绝 issuer URL 中的凭据/query/fragment、拒绝 discovery 重定向、限制 discovery 响应大小。
20. "Cleaned up resources when WebSocket-to-gRPC proxy connections close. [#7484] by @dmitri-netbird"
    → WebSocket→gRPC 代理连接关闭时清理资源（此前泄漏）。

#### Agent Network（2 条）

21. "Added managed proxy provisioning endpoints and response types to the API specification. [#7433] by @bison"
    → API spec 增加 Agent Network 托管 proxy 的开通 endpoint 与响应类型。
22. "Prevented deletion of groups referenced by Agent Network budget rules, preserving the rules' spending limits. [#7450] by @Tygiquamar"
    → 禁止删除被 Agent Network 预算规则引用的组，保住规则的消费上限。

#### Reverse Proxy Improvements（6 条）

23. "Added `NB_PROXY_UPSTREAM_HTTP_VERSION` with `auto`, `1.1`, and `2` options. The default `auto` mode negotiates with HTTPS upstreams and falls back to HTTP/1.1 when an upstream's negotiated HTTP/2 connection fails at the protocol level. [#7410] by @lixmal"
    → 新增 `NB_PROXY_UPSTREAM_HTTP_VERSION`（`auto`/`1.1`/`2`）；默认 `auto` 对 HTTPS upstream 协商，协商出的 HTTP/2 在协议层失败时回退 HTTP/1.1。
24. "Enforced group access both when issuing session cookies and when accepting existing sessions. [#7240] by @lixmal"
    → 组访问控制在「签发 session cookie」和「接受已有 session」两侧都强制执行（此前只查一侧）。
25. "Required custom domain validation before creating a service or moving one to a different custom domain. [#7341] by @mlsmaycon"
    → 创建 service 或把 service 迁到另一个自定义域名前，必须先完成域名校验。
26. "Added a 48-hour validation window for custom domain registrations. Existing pending registrations receive a fresh window on upgrade; expired registrations are removed unless they still have services attached. [#7497] by @mlsmaycon"
    → 自定义域名注册增加 48 小时校验窗口；升级时已有 pending 注册重新给一个窗口；过期且无 service 关联的注册被清理。
27. "Rejected unsupported direct-upstream IP addresses, including loopback, multicast, link-local, and IPv6 addresses with zone identifiers. [#7400] by @dmitri-netbird"
    → 拒绝不支持的直连 upstream IP：loopback、multicast、link-local、带 zone 标识的 IPv6。
28. "Validated domain names before creating certificate lock files. [#7501] by @pascal-fischer"
    → 创建证书 lock 文件前先校验域名。

#### Self-Hosting Improvements（6 条）

29. "Added a rootless Red Hat UBI image for AMD64 and ARM64, published with the `0.79.0-rootless-ubi` tag. [#7469] by @jnfrati"
    → 新增 AMD64/ARM64 的 rootless Red Hat UBI 镜像，tag 为 `0.79.0-rootless-ubi`。
30. "Supported arbitrary non-root UIDs without a passwd entry, as used by OpenShift. [#7440] by @jnfrati"
    → 支持 OpenShift 那种「无 passwd 条目的任意非 root UID」。
31. "Added RPM dependencies, license and documentation files, a generated changelog, and an example `/etc/sysconfig/netbird` to meet Red Hat software certification packaging requirements. [#7562], [#7573] by @mlsmaycon"
    → 为满足 Red Hat 软件认证的打包要求，补 RPM 依赖、license 与文档文件、生成的 changelog、示例 `/etc/sysconfig/netbird`。
32. "Kept deployments using the embedded identity provider on a single account, with stricter configuration checks and migration handling. [#7380] by @bcmmbaga"
    → 使用内嵌 IdP 的部署被限制在单账户，并加了更严格的配置检查与迁移处理。
33. "Passed the combined server's TLS configuration through to the management listener. [#7499] by @pascal-fischer"
    → combined server 的 TLS 配置现在传递到 management 监听器。
34. "Updated peer connection-IP extraction to honor configured `TrustedPeers`, with `X-Forwarded-For` taking precedence over `X-Real-IP`. The final release preserves trust in all IPv4 and IPv6 sources when `TrustedPeers` is empty; configure your reverse proxy's address or network to restrict which sources can supply forwarded-IP headers. [#7454], [#7589] by @bcmmbaga; [#7561], [#7577] by @dmitri-netbird and @bcmmbaga"
    → peer 连接 IP 提取改为遵循 `TrustedPeers`，`X-Forwarded-For` 优先于 `X-Real-IP`；`TrustedPeers` 为空时仍信任所有 IPv4/IPv6 来源（最终版的决定），因此需自行在反代上限制能提供 forwarded-IP 头的来源。

#### Internal, CI, and Docs（7 条）

35. "Added atomic `SetNX` and `GetDel` cache operations. [#7084] by @bcmmbaga"
    → 新增原子 cache 操作 `SetNX` 与 `GetDel`。
36. "Allowed binaries embedding management to extend its command tree. [#7483] by @bison"
    → 允许内嵌 management 的二进制扩展其命令树。
37. "Extracted peer update handling and added test coverage. [#7338] by @dmitri-netbird"
    → 抽出 peer update 处理逻辑并补测试。
38. "Replaced a hard-coded temporary directory in WebSocket adapter tests with the platform's temporary directory. [#7503] by @dmitri-netbird"
    → WebSocket adapter 测试里的硬编码临时目录换成平台临时目录。
39. "Switched the MinIO test image to `quay.io` after it became unavailable on Docker Hub. [#7516] by @Silex"
    → MinIO 测试镜像从 Docker Hub 换到 `quay.io`。
40. "Preserved image variant suffixes in snapshot tags. [#7511] by @jnfrati"
    → snapshot tag 保留镜像变体后缀。
41. "Skipped the protobuf breaking-change check on branch-creation pushes, which have no previous commit to compare against. [#7411] by @bison"
    → 建分支的推送跳过 protobuf breaking-change 检查（无前驱提交可比）。

### 1.3 Breaking Changes / Upgrade Notes / Deprecation

**官方没有这三类小节。** 以下是全文唯一带升级含义的原文段落，照抄并标注：

**（a）自定义域名升级行为——唯一直接出现 "on upgrade" 的一句**

> "Added a 48-hour validation window for custom domain registrations. **Existing pending registrations receive a fresh window on upgrade**; expired registrations are removed unless they still have services attached. [#7497]"

影响：升级到 0.79.0 那一刻，所有 pending 的自定义域名注册重新获得 48 小时窗口；**过期且未绑定 service 的注册会被删除**。属服务端行为变化，需要运维知会域名持有者。

**（b）`TrustedPeers` 与转发 IP 头——本版最容易被当成"安全修复"而漏掉的默认语义**

> "Updated peer connection-IP extraction to honor configured `TrustedPeers`, with `X-Forwarded-For` taking precedence over `X-Real-IP`. **The final release preserves trust in all IPv4 and IPv6 sources when `TrustedPeers` is empty**; configure your reverse proxy's address or network to restrict which sources can supply forwarded-IP headers. [#7454], [#7589], [#7561], [#7577]"

影响：不是强制收紧。`TrustedPeers` 为空 = 仍然信任任何来源的 `X-Forwarded-For` / `X-Real-IP`。若部署在反代后面且未配置 `TrustedPeers`，peer 记录的连接 IP 仍可被客户端伪造头影响（也影响 posture 里基于 IP 的判断）。**需要显式配置 `TrustedPeers` 才真正收紧。**

**（c）内嵌 IdP 单账户限制**

> "Kept deployments using the embedded identity provider on a single account, with stricter configuration checks and migration handling. [#7380]"

影响：多账户 + 内嵌 IdP 的部署在升级时会被配置检查拒绝或触发迁移，属部署侧行为变化。

**（d）镜像 tag 新增（非破坏，但发布流水线需识别）**

> "Added a rootless Red Hat UBI image for AMD64 and ARM64, published with the `0.79.0-rootless-ubi` tag. [#7469]"

**（e）关于 `#7411` 的干扰项澄清**

> "Skipped the protobuf breaking-change check on branch-creation pushes..."

这句里出现 "breaking-change" 但**与协议破坏性变更无关**，是 CI 检查被跳过的说明。**不要**把它当作 0.79.0 有 breaking change 的证据。本区间内没有任何 protobuf/协议字段变更条目。

---

## 2. v0.80.0 — 2026-10-01T16:08:09Z

- tag：`v0.80.0`，name：`v0.80.0`，prerelease：false，draft：false
- published_at：**2026-10-01T16:08:09Z**（created_at 2026-10-01T16:02:24Z）
- 页面：https://github.com/netbirdio/netbird/releases/tag/v0.80.0
- note 自带 Full Changelog：`v0.79.0...v0.80.0`（与本文用的区间一致）
- 区间提交数：47

> 结构说明：本版 note 有两层——上面是人工整理的 `## What's New`（6 个分组，47 个 bullet，带粗体小标题），下面是自动生成的 `## What's Changed`（47 条 `[前缀] 标题 by @作者 in #PR`）。两者一一对应，本文以 `What's New` 的粗体标题为准，因为它是官方给的"人话标题"。

### 2.1 Highlights

note **没有**单独的 Highlights 小节，也没有置顶新功能。从内容权重看，本版的重心是安全与访问控制（5 条，且全部是跨账户/重定向类漏洞修复）+ 客户端健壮性（13 条）。原文第一条即为最高优先级：

> "**Refused to pin an agent network gateway onto another account's host**, adding cross-account validation for gateway assignments. [#7519]"

### 2.2 What's New（按官方分组，原文 + 中文一句）

#### 🔒 Security and Access Control（5 条）

1. "**Refused to pin an agent network gateway onto another account's host**, adding cross-account validation for gateway assignments. [#7519]"
   → 拒绝把 Agent Network 网关固定到其他账户的主机上，网关分配增加跨账户校验。
2. "**Validated proxy cluster on Agent Network bootstrap**, preventing an agent network from bootstrapping onto another account's proxy cluster. [#7402]"
   → Agent Network 引导时校验 proxy cluster，防止 bootstrap 到别的账户的 proxy 集群。
3. "**Refused HTTP redirects on IdP clients**, preventing open redirect attacks against identity provider integrations. [#7579]"
   → IdP 客户端拒绝 HTTP 重定向，防开放重定向攻击。
4. "**Revoked local Dex session on embedded IdP password change**, ensuring sessions are properly invalidated when credentials are updated. [#7556]"
   → 内嵌 IdP 改密时吊销本地 Dex session。
5. "Added **upload URL signing and rate limiting** for file upload endpoints. [#7502]"
   → 文件上传 endpoint 增加上传 URL 签名与限流。

#### 🖥️ Client Improvements（13 条）

6. "**Staged install script downloads in a private temp directory**, preventing other processes from tampering with downloaded files before installation. [#7534]"
   → 安装脚本下载先落到私有临时目录，防止安装前被其他进程篡改。
7. "**Enforced HTTPS on install script downloads**, ensuring all client downloads use secure connections. [#7545]"
   → 安装脚本下载强制 HTTPS。
8. "Fixed **peers not being notified when the relay connection drops**, which could leave stale peer state after a relay disconnect. [#7490]"
   → 修复 relay 连接断开时 peer 收不到通知、导致 relay 断连后 peer 状态陈旧。
9. "Fixed **session deadline read under the status read lock**, preventing a potential race condition. [#7550]"
   → session 到期时间改在 status 读锁下读取，消除数据竞争。
10. "**Removed the empty GPO DNS policy store on Windows teardown**, cleaning up leftover DNS policy entries when the client shuts down. [#7563]"
    → Windows 退出时删除空的 GPO DNS 策略存储，清理残留 DNS 策略项。
11. "**Resolved the shared socket source address through a connected UDP probe socket**, fixing source address resolution for shared socket mode. [#7633]"
    → 用已连接的 UDP probe socket 解析 shared socket 的源地址，修复该模式下的源地址解析。
12. "**Validated saved service parameters and pinned the netsh lookup** on Windows, hardening the service configuration path. [#7584]"
    → Windows 侧校验已保存的 service 参数并固定 netsh 的查找路径，加固服务配置路径。
13. "**Used POSIX-style file read/write of JSON on Windows**, improving cross-platform config file handling. [#7631]"
    → Windows 上改用 POSIX 风格读写 JSON，改善跨平台配置文件处理。
14. "**Raised the daemon IPC receive limit to 16 MB**, matching gRPC's default and preventing truncation of large messages. [#7676]"
    → daemon IPC 接收上限提到 16 MB，对齐 gRPC 默认值，避免大消息被截断。
15. "**Skipped route firewall rule computation when no firewall** is configured, avoiding unnecessary work. [#7624]"
    → 未配置防火墙时跳过路由防火墙规则计算。
16. "Fixed **device dump clearing all peer endpoints** instead of just the target one, which could disrupt connections to other peers. [#7632]"
    → 修复 device dump 清掉**所有** peer endpoint（本应只清目标那一个），这会打断到其他 peer 的连接。
17. "Fixed **Windows IPC privilege check** to only treat LocalSystem as a privileged identity by SID. Local Service and Network Service now go through the same checks as any other caller. [#7889]"
    → Windows IPC 权限检查改为只按 SID 认 LocalSystem 为特权身份；Local Service 与 Network Service 现在走和普通调用者一样的检查。
18. "**Bumped Wails to v3.0.0-beta.25** for the desktop UI. [#7640]"
    → 桌面 UI 的 Wails 升到 v3.0.0-beta.25。

#### ⚙️ Management Improvements（11 条）

19. "**Sped up test store setup and summarized unit test runs**, improving CI performance. [#7518]"
    → 加速测试 store 搭建并汇总单测输出，改善 CI。
20. "**Read X-Real-IP when extracting the peer connection IP**, supporting deployments behind reverse proxies. [#7561]"
    → 提取 peer 连接 IP 时读取 `X-Real-IP`，支持反代后部署。
21. "**Handled empty trusted peer** to avoid errors when no trusted peer is configured. [#7589]"
    → 处理空 trusted peer，未配置时不报错。
22. "**Named the account owner in the pending approval error**, making it clear who to contact for account approval. [#7533]"
    → 待审批错误里点名账户所有者，便于知道找谁审批。
23. "Fixed **group resource validation** for proper enforcement. [#7608]"
    → 修复 group resource 校验，使其真正生效。
24. "Added **store support to filter by public ID**. [#7208]"
    → store 支持按 public ID 过滤。
25. "**Recorded proxy version on connect** for tracking and debugging. [#7630]"
    → proxy 连接时记录版本，便于追踪排障。
26. "**Split store by table** for improved database performance and organization. [#7646]"
    → store 按表拆分，改善数据库性能与组织。
27. "**Prevented deleting custom domains used by services**, protecting active proxy configurations from accidental removal. [#7515]"
    → 禁止删除仍被 service 使用的自定义域名。
28. "**Moved rate limiter to shared package** for reuse across services. [#7727]"
    → 限流器移到共享包，供多个服务复用。
29. "**Let usage_viewer read Agent Network access logs**, so users reviewing cost can now see the requests behind usage metrics. [#7750]"
    → 授予 usage_viewer 读取 Agent Network 访问日志的权限，核费时能看到用量背后的请求。

#### 🔀 Reverse Proxy（6 条）

30. "**Closed the client connection on private service denials** instead of leaving it hanging. [#7590]"
    → 私有 service 被拒时关闭客户端连接，而不是挂着。
31. "Added **proxy rate limiter** to protect against abuse. [#7568]"
    → proxy 增加限流。
32. "Added **proxy credentials limiter on management** to throttle authentication attempts. [#7569]"
    → management 侧增加 proxy 凭据限流，抑制认证爆破。
33. "Fixed **upstream HTTP version applied before cloning transports**. [#7806]"
    → 修复 upstream HTTP 版本在克隆 transport **之前**被应用的问题（顺序错了导致设置丢失）。
34. "Added **release-wired UBI image variant** for the proxy. [#7464]"
    → proxy 增加与 release 联动的 UBI 镜像变体。
35. "Fixed **flow auth secret for external Relay migrations**. [#7731]"
    → 修复外部 Relay 迁移时的 flow auth secret。

#### 🏗️ Infrastructure and CI（9 条）

36. "**Certified the rootless UBI client image on release** for Red Hat ecosystem compliance. [#7525]"
    → release 时对 rootless UBI 客户端镜像做认证，满足 Red Hat 生态合规。
37. "Fixed **RPM metadata for Red Hat software certification** (two rounds of fixes). [#7562], [#7614]"
    → 修复 Red Hat 认证所需的 RPM metadata（两轮）。
38. "Added **proxy support to enterprise setup**. [#7651]"
    → enterprise 安装脚本支持 proxy。
39. "**Built the upload server from source, nonroot on Chainguard** for a hardened container image. [#7663]"
    → upload server 在 Chainguard 上以非 root 从源码构建。
40. "**Built and linted mobile Go code in CI** for Android. [#7641]"
    → CI 对 Android 的移动端 Go 代码做构建与 lint。
41. "**Bumped workflow actions off the retired Node 20 runtime**. [#7644]"
    → workflow actions 迁离已退役的 Node 20 运行时。
42. "Added **Pyroscope profiling** to management, signal, and proxy services. [#7536]"
    → management、signal、proxy 增加 Pyroscope profiling。
43. "**Pointed the agent-config e2e providers at the mock upstream**. [#7542]"
    → agent-config e2e 的 provider 指向 mock upstream。
44. "**Used the Silo image for the S3 upload test**. [#7619]"
    → S3 上传测试改用 Silo 镜像。

#### 📝 Misc（3 条）

45. "**Pointed bug reports at Discussions and added SUPPORT.md**. [#7647]"
    → bug 报告引导到 Discussions，并新增 SUPPORT.md。
46. "Added **cloud API spec to the public OpenAPI** definition with REST client. [#7222]"
    → 把 cloud API spec 并入公开 OpenAPI 定义并提供 REST client。**⚠ 此条在 git 区间里找不到对应提交，且 PR #7222 返回 404，详见 5.2。**
47. "**Loaded AGENTS.md every session and refused attribution trailers**. [#7544]"
    → 每个 session 加载 AGENTS.md 并拒绝 attribution trailer。

#### New Contributors（原文）

> "* @thomashacker made their first contribution in [#7647]"

### 2.3 Breaking Changes / Upgrade Notes / Deprecation

**官方没有这三类小节。** 以下是正文里带行为变化含义的原文段落：

**（a）Windows IPC 权限判定的实际收紧（最像 breaking 的一条）**

> "Fixed **Windows IPC privilege check** to only treat LocalSystem as a privileged identity by SID. **Local Service and Network Service now go through the same checks as any other caller.** [#7889]"

影响：此前以 Local Service / Network Service 身份运行的调用者能绕过检查，现在不能。若有第三方组件把 daemon 装在 Local Service 下并以该身份调 IPC，升级后会开始被拒。

**（b）daemon IPC 上限从 4 MB 提到 16 MB（原文与提交标题不一致，需注意）**

note 原文：

> "**Raised the daemon IPC receive limit to 16 MB**, matching gRPC's default and preventing truncation of large messages. [#7676]"

对应提交标题是：

> `[client] Raise the daemon IPC receive limit above gRPC's 4 MB default (#7676)`

两者措辞不同但一致：原上限是 gRPC 的 4 MB 默认值，现提到 16 MB。大消息（如大 network map / 大量 peer）不再被截断，属**修复**而非破坏，但会改变 IPC 的内存占用特征。

**（c）反向代理 `X-Real-IP` 回补**

> "**Read X-Real-IP when extracting the peer connection IP**, supporting deployments behind reverse proxies. [#7561]"
> "**Handled empty trusted peer** to avoid errors when no trusted peer is configured. [#7589]"

影响：这是 0.79.0 `TrustedPeers` 改造的配套修复（0.79.0 的 note 里 #7561/#7577/#7589 已作为同一组出现）。两版合起来才是完整语义：`X-Forwarded-For` 优先、`X-Real-IP` 兜底、`TrustedPeers` 为空则信任全部来源。

**（d）Agent Network 跨账户校验（部署/配置侧）**

> "**Refused to pin an agent network gateway onto another account's host**..." / "**Validated proxy cluster on Agent Network bootstrap**..."

影响：若现有 Agent Network 配置存在跨账户引用（历史遗留），升级后会被拒绝引导。官方未设 upgrade notes，需自行检查。

**（e）自定义域名不可删**

> "**Prevented deleting custom domains used by services**, protecting active proxy configurations from accidental removal. [#7515]"

影响：0.79.0 加了 48 小时校验窗口（会清理过期且无 service 的注册），0.80.0 禁止删除仍被 service 使用的域名。两版合起来才形成完整的域名生命周期规则。

---

## 3. 标签分类统计

标签取值与配套报告一致：`客户端-移动端` / `客户端-桌面/守护进程` / `管理端/服务端`（含 proxy、signal、relay、management 等所有服务端组件）/ `依赖/构建/CI/文档` / `其他`。
计数单位 = note 里的一个 bullet（一个 bullet 可能引用多个 PR）。跨端条目计入主归属，另标 `[+…]`。

### 3.1 v0.79.0（41 个 bullet）

| 标签 | 数量 | 条目 |
|---|---|---|
| 客户端-移动端 | **2** | #6435（iOS/Android MDM 桥）、#7414（Android TUN 前缀） |
| 客户端-桌面/守护进程 | **10** | #7449、#7485、#7471 `[+移动端]`、#7472 `[+移动端]`、#7409、#7439、#7498、#7426、#7532、#7452 `[+proxy]` |
| 管理端/服务端 | **19** | Management 8 条（#7418、#7425、#7467、#7427、#7486、#7456、#7435、#7484）+ Agent Network 2 条（#7433、#7450）+ Reverse Proxy 6 条（#7410、#7240、#7341、#7497、#7400、#7501）+ Self-Hosting 3 条（#7380、#7499、#7454/#7589） |
| 依赖/构建/CI/文档 | **10** | Self-Hosting 3 条（#7469、#7440、#7562/#7573）+ Internal 7 条（#7084、#7483、#7338、#7503、#7516、#7511、#7411） |
| 其他 | **0** | — |

核对：2 + 10 + 19 + 10 + 0 = 41 ✓

### 3.2 v0.80.0（47 个 bullet）

| 标签 | 数量 | 条目 |
|---|---|---|
| 客户端-移动端 | **0** 主归属（仅 #7641 属 `[+移动端]`，主归属为 CI） | — |
| 客户端-桌面/守护进程 | **13** | #7534、#7545、#7490、#7550、#7563、#7633、#7584、#7631、#7676、#7624 `[+管理端]`、#7632、#7889、#7640 |
| 管理端/服务端 | **20** | Security 5 条（#7519、#7402、#7579、#7556、#7502）+ Management 10 条（#7561、#7589、#7533、#7608、#7208、#7630、#7646、#7515、#7727、#7750）+ Reverse Proxy 4 条（#7590、#7568、#7569、#7806）+ #7536（Pyroscope，服务端可观测性） |
| 依赖/构建/CI/文档 | **14** | #7518、#7464、#7731 + Infrastructure 8 条（#7525、#7562/#7614、#7651、#7663、#7641 `[+移动端]`、#7644、#7542、#7619）+ Misc 3 条（#7647、#7222、#7544） |
| 其他 | **0** | — |

核对：0 + 13 + 20 + 14 + 0 = 47 ✓

### 3.3 两版合并（88 个 bullet）

| 标签 | 0.79.0 | 0.80.0 | 合计 |
|---|---|---|---|
| 客户端-移动端 | 2 | 0 | **2** |
| 客户端-桌面/守护进程 | 10 | 13 | **23** |
| 管理端/服务端 | 19 | 20 | **39** |
| 依赖/构建/CI/文档 | 10 | 14 | **24** |
| 其他 | 0 | 0 | **0** |

**读法提示**：移动端主归属只有 2 条，但这不代表移动端没变化——#6435 一条覆盖了 iOS+Android 两个 SDK 的整套 MDM 通道，是这两版里对移动端最大的单项变更。

---

## 4. 对第三方客户端有实际影响的条目

筛选口径：凡涉及 client 连接行为、ICE/relay 中继选路、DNS、路由/HA 选路、登录与注册、状态上报、协议字段，无论官方归在哪个类别都收录。**顺序按「对第三方客户端的影响强度」排，不按官方分组。**

⚠ 结合 0.1 节：**0.79.0 这 10 条里有 9 条已经在锚点 7914010 里了**。锚点之后到 v0.79.0 只剩 6 个提交，其中与本节相关的只有 `#7532`（wireguard-go）一条。**真正的缺口 = 0.79.0 的 1 条（#7532）+ 0.80.0 的 12 条。**

另有一个版本口径的坑要记住：`#7561` / `#7589` 以 cherry-pick 形式（`#7577`）进了 v0.79.0 的 tag，但在 main 上它们落在 0.79.0→0.80.0 区间（`f8c3e565` / `7d8f4fa3`），所以对跟着 main 走的第三方客户端**仍是缺口**，也因此被 0.80.0 的 note 又列了一遍。下表把它们归到 4.2（0.80.0 侧）。

### 4.1 v0.79.0（10 条）

| 强度 | 原文标题 | PR | 是否已在 7914010 | 为什么影响第三方客户端 |
|---|---|---|---|---|
| ★★★ | "Refreshed system information on every management sync reconnect, keeping local network addresses and posture information current after network changes." | #7409 | ✅ 已有 | **状态上报**。peer 上报的 hostname/OS/本机地址/posture 必须在**每次** management sync 流重连时重采。自研客户端若只在启动时采一次，网络切换后会向管理端上报陈旧地址，影响连接选路与 posture 判定。 |
| ★★★ | "Fixed a relay address race that could advertise a URL and IP from different connections during a reconnect." | #7498 | ✅ 已有 | **relay 选路**。relay 地址是 (URL, IP) 二元组，重连时若分别从两个连接取值会拼出**不可达的 relay 地址**。自研实现需要保证二者原子取自同一连接。 |
| ★★★ | "Used host prefixes for Android TUN addresses so local network protection does not classify the entire overlay as a local network." | #7414 | ✅ 已有 | **路由**。TUN 地址下发成 host 前缀（如 /32），否则「本地网络保护」会把整个 overlay 当本地网络而绕过隧道。HarmonyOS VPN 有同类 local-network 判定时同理。 |
| ★★★ | "Replaced the eBPF DNS forwarder with UDP and TCP DNAT rules when the resolver cannot listen on port 53, with rollback and cleanup for incomplete redirects." | #7439 | ✅ 已有 | **DNS**。0.79.0 起 Linux 客户端在 53 端口被占用时不再依赖 eBPF，改走 DNAT，并需处理回滚/残留规则。自研客户端的 DNS 接管降级路径要对应。 |
| ★★☆ | "Updated wireguard-go to `8bf8fa968f1a`, fixing keepalive buffer-pool stalls, keeping timer paths non-blocking, and making netstack interface shutdown idempotent." | #7532 | ❌ **【缺口】** | **连接行为**。keepalive 缓冲区池卡住会表现为隧道"假死"。若第三方客户端自带 wireguard 数据面，需同步该上游修复；HarmonyOS 侧若用 netstack，关停幂等也相关。 |
| ★★☆ | "Allowed buffer-pool limits to be adjusted while a device is stalled, and bounded proxy-wide updates so one stuck client does not block the others." | #7452 | ✅ 已有 | **连接行为**。卡死状态下仍可调缓冲区池上限；proxy 全局限流更新有界。前者是自研数据面的可观测/可恢复性设计参考。 |
| ★★☆ | "Included offline peers when scheduling login expiration and ensured expired peers are disconnected, while protecting peers that have just logged in again." | #7467 | ✅ 已有 | **登录与注册**。登录到期语义改为「含离线 peer 一起调度 + 到期强制断开 + 保护刚重登的 peer」。客户端需能处理服务端主动下发的到期断开，并在重登时正确刷新到期时间。 |
| ★★☆ | "Applied duplicate-key sync protection to user-owned peers as well as peers registered with setup keys." | #7427 | ✅ 已有 | **注册/登录**。同一 wireguard key 重复注册会被拦（此前只拦 setup key 注册的 peer）。自研客户端在 key 复用/迁移场景会收到拒绝，需要错误处理路径。 |
| ★☆☆ | "Hardened OIDC issuer validation by requiring HTTPS, rejecting credentials, query strings, and fragments in issuer URLs, refusing discovery redirects, and limiting discovery response size." | #7435 | ✅ 已有 | **登录**。自建 OIDC 若 issuer 带 query/fragment 或非 HTTPS，升级后登录直接失败。 |
| ★☆☆ | "Compared MDM-managed URLs by their normalized endpoints, avoiding false conflicts between equivalent URLs." + "Accepted MDM boolean values delivered as JSON numbers" + "Added MDM policy bridges and shared enforcement for the iOS and Android SDKs" | #7472 #7471 #6435 | ✅ 已有 | **客户端行为/托管配置**。移动端若实现 MDM 托管，需按「规范化 endpoint 比较 URL」「布尔值兼容 0/1」处理，否则策略会被误判为冲突或不生效。 |

### 4.2 v0.80.0（12 条，对跟 main 走的客户端全部为缺口）

| 强度 | 原文标题 | PR | 为什么影响第三方客户端 |
|---|---|---|---|
| ★★★ | "Fixed **peers not being notified when the relay connection drops**, which could leave stale peer state after a relay disconnect." | #7490 | **relay 选路 / 连接状态**。relay 断链后 peer 状态会陈旧——这是本次两版里对中继行为最直接的一条。自研客户端必须保证 relay 断链能触发 peer 状态更新/重连，否则会持续朝已死的 relay 发包。 |
| ★★★ | "**Resolved the shared socket source address through a connected UDP probe socket**, fixing source address resolution for shared socket mode." | #7633 | **ICE/连接**。shared socket 模式下源地址解析错误会导致 STUN/ICE 拿到错的本机候选地址。自研 ICE 实现若复用同一 socket 收发，需按"已连接 UDP probe"方式取源地址。 |
| ★★★ | "Fixed **device dump clearing all peer endpoints** instead of just the target one, which could disrupt connections to other peers." | #7632 | **连接行为**。清理单个 peer endpoint 时误清全部 endpoint，会**打断到其他 peer 的连接**。任何按 peer 维度管理 endpoint 的实现都要检查这个作用域错误。 |
| ★★☆ | "**Raised the daemon IPC receive limit to 16 MB**, matching gRPC's default and preventing truncation of large messages." | #7676 | **状态上报/协议消息**。消息体积上限决定大 network map（大量 peer）会不会被截断。若第三方客户端走 gRPC 到 daemon，需确认自身接收上限，否则大账户下静默丢数据。 |
| ★★☆ | "Fixed **session deadline read under the status read lock**, preventing a potential race condition." | #7550 | **连接状态**。session 到期时间与状态读锁的竞争，会导致到期判断读到不一致值（提前或延迟断开）。 |
| ★★☆ | "**Skipped route firewall rule computation when no firewall** is configured, avoiding unnecessary work." | #7624 | **路由**。无防火墙时跳过路由防火墙规则计算——自研实现若在每次路由更新都全量计算防火墙规则，在无防火墙配置下是纯浪费（大 network map 下会卡）。 |
| ★★☆ | "Fixed **Windows IPC privilege check** to only treat LocalSystem as a privileged identity by SID. Local Service and Network Service now go through the same checks as any other caller." | #7889 | **本地权限/连接**。仅 Windows 相关，但若第三方客户端有 Windows 侧组件以 Local Service 运行，升级后会开始被拒。 |
| ★☆☆ | "**Removed the empty GPO DNS policy store on Windows teardown**, cleaning up leftover DNS policy entries when the client shuts down." | #7563 | **DNS**（Windows）。退出时残留空 GPO DNS 策略项，会让后续 DNS 行为不确定。 |
| ★☆☆ | "**Validated saved service parameters and pinned the netsh lookup** on Windows, hardening the service configuration path." | #7584 | **连接配置**（Windows）。保存的 service 参数未校验 + netsh 查找未固定 → 可被 PATH 劫持。 |
| ★☆☆ | "**Enforced HTTPS on install script downloads**" + "**Staged install script downloads in a private temp directory**" | #7545 #7534 | **安装/升级链路**。下载走 HTTPS + 私有临时目录（防篡改）。自研客户端的安装/自更新脚本应对应。 |
| ★☆☆ | "**Read X-Real-IP when extracting the peer connection IP**" + "**Handled empty trusted peer**" | #7561 #7589 | **状态上报**。服务端如何取 peer 连接 IP（影响 peer 列表展示与基于 IP 的 posture）。客户端侧需知道：反代后部署且未配 `TrustedPeers` 时，管理端看到的 IP 可能来自转发头。 |
| ★☆☆ | "**Recorded proxy version on connect**" + "**Closed the client connection on private service denials**" | #7630 #7590 | **连接行为（proxy）**。私有 service 被拒时从"挂着"改为"关连接"，客户端的重试/超时逻辑会看到行为差异。 |

### 4.3 明确**不**影响第三方客户端的条目（供排除，避免误入范围）

- 协议字段：两版**均无** protobuf/协议字段变更。v0.79.0 唯一出现 "breaking-change" 的 #7411 是 CI 检查被跳过（见 1.3(e)），v0.80.0 无相关条目。
- 纯 UI：v0.79.0 #7344（浅色主题）、#7449（Windows 托盘死锁）、#7485（MDM UI 快照）；v0.80.0 #7640（Wails 升级）。
- 纯服务端/多租户：#7519、#7402、#7533、#7608、#7208、#7646、#7727、#7750、#7502、#7556、#7579。
- 纯打包/CI：#7469、#7440、#7562/#7573、#7525、#7614、#7651、#7663、#7641、#7644、#7536、#7542、#7619、#7518、#7464、#7731、#7647、#7544、#7084、#7483、#7338、#7503、#7516、#7511、#7411。
- proxy 内部：#7806、#7568、#7569、#7515、#7410、#7240、#7341、#7497、#7400、#7501（客户端只通过 proxy 转发受间接影响）。

---

## 5. note 与 git 区间的差异

### 5.1 v0.79.0：**零差异**

- note 中出现的 PR 号：**46 个**
- `compare/v0.78.1...v0.79.0` 的 44 个提交中出现的 PR 号：**46 个**（有 2 个提交各带两个 PR 号：`(#7562) (#7573)`、`(#7561) (#7577)`）
- 「在提交里但 note 没写」：**0 条**
- 「note 写了但提交里没有」：**0 条**

即：**v0.79.0 的 release note 没有漏报任何提交。** 这与 0.81.1/0.81.2 的情况完全不同。

需要注意的口径差异（不是漏报，是范围定义不同）：

- note 自带的 Full Changelog 写 `v0.78.0...v0.79.0`，而本文用的是 `v0.78.1...v0.79.0`。
- 差的那一个提交是 `23a1487c`（`(#7424)`，2026-09-04，SQLite network map peer-router 修复），它是 **v0.78.1 的唯一提交**，且**不在 v0.79.0 的历史里**（main 上用 `8dc42725 (#7418)` 重新落地）。所以按 `v0.78.0...v0.79.0` 数会得到 45 个"提交"，但其中 1 个并不通向 v0.79.0，实际仍是 44。

### 5.2 v0.80.0：1 处幽灵条目 + 5 个 backport PR 号差异

`## What's Changed` 原始清单 **47 个 bullet ↔ 47 个提交**，逐条对应，**没有漏报的提交**。

差异只有两类：

**（1）note 写了、但 git 区间里不存在：#7222 —— 幽灵条目**

> "Added **cloud API spec to the public OpenAPI** definition with REST client. [#7222]"

- 该 PR 号在 `compare/v0.79.0...v0.80.0` 的 47 个提交里**不存在**；
- 在 `compare/v0.78.1...v0.79.0` 里也**不存在**；
- `GET /repos/netbirdio/netbird/pulls/7222` 返回 **404 Not Found**（PR 不存在，可能属于私有仓库或编号串号）。

**结论：这一条无法用任何 git 证据交叉验证，只能记录为"note 声明但未核实"。** 不要把它当作 0.80.0 的实际变更计入评估。

**（2）提交里有、note 没写的 5 个 PR 号 —— 全是 backport，不是漏报**

| 提交里的 PR 号 | 提交 sha / 日期 | note 里对应的原始 PR 号 | 判定 |
|---|---|---|---|
| #7737 | `6441417e` 2026-09-28 | #7731（flow auth secret for external Relay migrations） | 同一改动的 cherry-pick |
| #7741 | `da483ab0` 2026-09-29 | #7632（Stop dumping the whole device to clear one peer endpoint） | 同上 |
| #7813 | `7a53a7b3` 2026-09-30 | #7806（Apply the upstream HTTP version before cloning transports） | 同上 |
| #7885 | `3e2ed39d` 2026-09-30 | #7750（Let usage_viewer read Agent Network access logs） | 同上 |
| #7904 | `fca64287` 2026-10-01 | #7889（Only treat LocalSystem as a privileged identity by SID） | 同上 |

判定依据：这 5 个提交的标题形如 `… (#原始PR) (#backportPR)`，note 里列出的是括号中的**原始** PR 号，内容描述与之一致。因此**不构成 note 漏报**。

### 5.3 两版合并的差异总表

| 项 | v0.79.0 | v0.80.0 |
|---|---|---|
| 区间提交数 | 44 | 47 |
| note 覆盖的提交数 | 44（100%） | 47（100%） |
| note 漏报的提交 | 0 | 0 |
| note 写了但 git 没有 | 0 | **1（#7222，PR 404）** |
| 提交里有但 note 用别的 PR 号 | 0 | 5（均为 backport） |
| note 自带 Full Changelog 范围 | `v0.78.0...v0.79.0`（与本次用的 v0.78.1 起点差 1 个非祖先提交） | `v0.79.0...v0.80.0`（一致） |

---

## 6. 证据清单

抓取时间窗：2026-10-24T21:43Z–21:47Z。GitHub 匿名配额 60/h，抓取开始时余量充足，结束时余 2（`x-ratelimit-remaining: 2`），本文所需数据在此之前已全部落盘。

### 6.1 成功抓取（HTTP 200）

| # | URL | 状态码 | 大小 / 关键字段 |
|---|---|---|---|
| 1 | `https://api.github.com/repos/netbirdio/netbird/releases/tags/v0.79.0` | **200** | 131,549 B；body 9,344 字符；published_at `2026-09-18T17:06:28Z`；prerelease false |
| 2 | `https://api.github.com/repos/netbirdio/netbird/releases/tags/v0.80.0` | **200** | 136,379 B；body 14,092 字符；published_at `2026-10-01T16:08:09Z`；prerelease false |
| 3 | `https://api.github.com/repos/netbirdio/netbird/compare/v0.78.1...v0.79.0` | **200** | 1,367,619 B；`total_commits` 44 = 数组长度 44；`status` diverged；`ahead_by` 44；`behind_by` 1 |
| 4 | `https://api.github.com/repos/netbirdio/netbird/compare/v0.79.0...v0.80.0` | **200** | 1,244,305 B；`total_commits` 47 = 数组长度 47；`status` diverged；`ahead_by` 47；`behind_by` 3 |
| 5 | `https://api.github.com/repos/netbirdio/netbird/compare/v0.78.0...v0.78.1` | **200** | `ahead_by` 1：`23a1487c` `[management] Serve networks with peer-based routers from the SQLite network map (#7424)` |
| 6 | `https://api.github.com/repos/netbirdio/netbird/compare/v0.79.0...v0.78.1`（反向） | **200** | `ahead_by` 1 = 上述同一提交，用于确认它不在 v0.79.0 历史中 |
| 7 | `https://api.github.com/repos/netbirdio/netbird/compare/v0.80.0...v0.79.0`（反向） | **200** | `ahead_by` 3 = `7cbf8234`(#7573) / `0ce7617b`(#7577) / `396795eb`(#7589)，确认为 cherry-pick |
| 8 | `https://github.com/netbirdio/netbird/releases/tag/v0.79.0`（HTML） | **200** | 仅用于确认页面存在；正文以 API body 为准 |
| 9 | `https://github.com/netbirdio/netbird/releases/tag/v0.80.0`（HTML） | **200** | 同上 |

四个主数据源的原始 JSON 已落盘于 `.upstream-eval/raw-0.79.0-0.80.0/`（`v0790.json`、`v0800.json`、`cmp_781_790.json`、`cmp_790_800.json`，另含两个反向 compare、`cmp_780_781.json`、`pr7222.json`、两份 body 纯文本 `v0790.md`/`v0800.md` 与提交首行清单 `cmp_*.txt`），可直接复核。

### 6.2 抓取失败 / 不存在

| # | URL | 状态码 | 说明 |
|---|---|---|---|
| 10 | `https://api.github.com/repos/netbirdio/netbird/pulls/7222` | **404** | PR 不存在。这直接否证了 0.80.0 note 里 "#7222 Added cloud API spec to the public OpenAPI" 的可验证性 |

### 6.3 未获取到 / 未核实（明确留白）

1. **#7222 到底对应什么改动**：未获取到。PR 404，git 区间无对应提交，note 也没给作者。不做推测。
2. **v0.79.0 / v0.80.0 的 dashboard（netbirdio/dashboard）配套发布**：本次未抓取，两版 note 里也没有 dashboard 相关条目。若需评估管理端 UI 变更，需另抓 dashboard 仓库的 release。
3. **android-client / ios-client 仓库是否有同名版本的独立 release**：本次未抓取。0.79.0 note 里 #6435 提到 iOS/Android SDK 的 MDM 桥，但移动端 APP 的发布节奏是否与 core 版本对齐，未核实。
4. **各 PR 的实际 diff**：本次只取标题与 note 正文，未取逐 PR 的 patch。第 4 节里"为什么影响第三方客户端"的机理说明，依据是 note 原文措辞 + PR 标题，**未读代码**。凡属推断的地方已在措辞上区分（例如"会表现为隧道假死"来自 note 的 "keepalive buffer-pool stalls"，未看实现）。
5. **v0.78.1 的确切发布日**：未单独取 `releases/tags/v0.78.1`；只知道其唯一提交是 `23a1487c`（2026-09-04）。任务描述给的锚点是 commit 而非 v0.78.1 的发布日期，故未追加请求（配额也已见底）。

---

## 7. 三句话结论

1. **锚点比想象中靠后**：`7914010` 已是 `v0.79.0` 区间的第 38/44 个提交，第三方客户端的真实缺口是「0.79.0 尾部 6 个 + 0.80.0 全部 47 个 + 0.80.0→0.81.4」，而不是「从 0.78.1 起 88 个」。v0.79.0 最关键的 relay 地址竞争（#7498）、DNS DNAT（#7439）、Android TUN 前缀（#7414）**已经在手里**。

2. **这两版的 note 罕见地可信**：0.79.0 零漏报（46 PR ↔ 46 PR，双向差集为空），0.80.0 唯一瑕疵是幽灵条目 #7222（PR 404、git 无对应提交），其余 47 条与区间提交一一对应。因此这两版可以**直接以 note 为准**，不需要像 0.81.1/0.81.2 那样拿 git 区间反推。

3. **真正的增量风险集中在 0.80.0 的客户端连接侧 3 条**：#7490（relay 断链不通知 peer → 状态陈旧）、#7633（shared socket 源地址解析 → ICE 候选错误）、#7632（清 endpoint 误清全部 → 打断其他 peer 连接）；外加 #7676 的 IPC 上限变化（大 network map 截断）。这四条是本轮最该补的实现项，且**全部不在锚点里**。
