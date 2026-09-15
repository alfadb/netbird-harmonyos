# VPN `create()` 401「Parameter error」离线诊断（2026-09-15）

- 性质：**只读诊断**。本文不修改任何代码；未运行任何设备命令；未做真机验证。
- 交付目标：解释真机上 `VpnConnection.create()` 返回 `401 Parameter error` 的可能原因，并给出最小设备验证计划。
- 证据等级声明：
  - 「SDK 摘录 / 代码摘录」= 本机 SDK 与仓库实际读到的内容，附 `file:line`。
  - 「越界采集线索」= 题目给定的观测，**仅作待复核线索，不构成证据**。

---

## 1. 背景观测（越界采集，仅作待复核线索）

| 项 | 本次（失败） | 历史（成功，run7，2026-09-14） |
|---|---|---|
| 设备 | HarmonyOS 手机，API 26 / SDK 26.0.0.821，HAP `cn.alfadb.netbird`（VpnExtensionAbility） | 同 |
| 报错 | `VPN_START_FLOW_REJECTED\|summary=code=401,name=Error,message=Parameter error` | `VPN_CREATE_RESOLVED\|accepted=true`，无 REJECTED |
| 路由 | `VPN_CONFIG_APPLIED\|routes=5\|excludedRoutes=3`；粗略构成：默认路由（0.0.0.0/0，gateway 10.99.0.1，hasGateway true，isDefaultRoute true）+ 2 条 managed 路由 + 3 条排除路由（LAN 192.168.50.0/24 + management /32 + signal /32，均 isExcludedRoute） | `routes=1` |
| 平台侧明细 | hilog 缓冲滚动未取到（待解项） | — |

线索内部的矛盾（复核时注意）：按上述构成 1+2+3=6 ≠ `routes=5`。可能来源：某条派生 /32 与既有条目 dedup（见 §3.3）、某条 managed 路由被解析守卫跳过（见 §3.2）、或观测回忆近似。**逐字段真实值无日志可查证**（现日志只打计数，见 §3.5）。

---

## 2. SDK 约束核查（首要）

SDK 根路径：`/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/ets/api/`（下文简写 `sdk/api/`）。

### 2.1 `VpnConnection.create()` 的错误码声明

`sdk/api/@ohos.net.vpnExtension.d.ts`：

- L201：`@throws { BusinessError } 401 - Parameter error.`
- L202：`@throws { BusinessError } 2200001 - Invalid parameter value.`
- L210：`create(config: VpnConfig): Promise<number>;`
- L194-196（NOTE）：不需要 VPN 时应调用 `destroy()` 清理资源 —— 支持试验矩阵的「每步先 destroy 再 create」回滚纪律。

> 推断（待证实）：401 与 2200001 在 d.ts 中并列；观测 `message=Parameter error` 与 401 的文档文案逐字一致。401 通常是**入参在 API/参数层被拒**（对象结构/字段类型层面），2200001 才是服务侧语义校验（值不合法）。该分层判断无法从 SDK d.ts 证实，属「待确认」，但与「同一形态历史成功、仅新配置失败」的现象吻合。

### 2.2 `VpnConfig` 字段与上限

`sdk/api/@ohos.net.vpnExtension.d.ts` L283-389：

| 字段 | 行号 | 必填 | @since | 约束摘录（节译） |
|---|---|---|---|---|
| `vpnId?` | L290 | 否 | 20 | Unique VPN ID |
| `addresses` | L292-298 | **是** | 11 | vNIC 地址；API 23 前最多 **64** 个，API 23 起最多 2000 |
| `routes?` | L300-306 | 否 | 11 | API 23 前最多 **1024** 条，API 23 起最多 **10,000** 条 |
| `dnsAddresses?` | L314 | 否 | 11 | DNS 服务器 IP 列表 |
| `searchDomains?` | L321 | 否 | 11 | — |
| `mtu?` | L323-328 | 否 | 11 | 取值范围 **[576,1500]** |
| `isIPv4Accepted?` | L330-338 | 否（默认 true） | 11 | L333：若支持 IPv4，须在 `addresses` 中配置 IPv4 地址 |
| `isIPv6Accepted?` | L340-348 | 否（默认 false） | 11 | L343：若支持 IPv6，须在 `addresses` 中配置 IPv6 地址 |
| `isInternal?` | L356 | 否（默认 false） | 11 | — |
| `isBlocking?` | L364 | 否（默认 false） | 11 | — |
| `trustedApplications?` / `blockedApplications?` | L376 / L388 | 否 | 11 | L371/L383：**二者互斥**；各最多 64（API 23 起 256）个 |

### 2.3 `RouteInfo` 各字段

`sdk/api/@ohos.net.connection.d.ts` L2169-2218（vpnExtension L66 `export type RouteInfo = connection.RouteInfo`）：

| 字段 | 行号 | 必填 | @since | 摘录 |
|---|---|---|---|---|
| `interface` | L2176 | **是**（string） | 8 | NIC name |
| `destination` | L2183 | **是**（LinkAddress） | 8 | Destination address |
| `gateway` | L2190 | **是**（NetAddress） | 8 | Gateway address |
| `hasGateway` | L2198 | **是**（boolean） | 8 | Whether a gateway is present |
| `isDefaultRoute` | L2209 | **是**（boolean） | 8 | L2203-2204：IPv4 默认路由指 destination 为 **0.0.0.0/0** 的路由（IPv6 为 ::/0） |
| `isExcludedRoute?` | L2217 | 否（boolean） | **20** | Whether the route is excluded |

`LinkAddress`（L2225-2240）：`address: NetAddress`（L2232）、`prefixLength: number`（L2239，**d.ts 未声明取值范围**）。
`NetAddress`（L2249-2277）：`address: string`（L2258）、`family?: number`（L2267：**1=IPv4，2=IPv6，默认 1**）、`port?: number`（L2276：[0,65535]）。

`sdk/api/@ohos.net.vpn.d.ts`（L29-49）为系统内置 VPN 模块，仅复用 `connection.RouteInfo/LinkAddress` 类型（L36/L43），与本问题无直接关系。

### 2.4 SDK 侧小结（关键发现）

1. **routes 数量上限不可能触发**：5 条 ≪ 1024/10000（L300-306）。
2. **d.ts 成文约束中，没有**「排除路由必须落在某前缀内」「排除路由不得带 gateway」「gateway 必须可达/与 VPN 地址同网段」「prefixLength ∈ [0,32]」这类条目 —— 这类规则若存在，属于**平台实现侧未成文约束，SDK 层无法证实**（列入 §7 待确认）。
3. `isExcludedRoute` 是 **@since 20 的可选字段**（L2217），API 26 设备可用；不带该字段 = 默认 false。
4. `RouteInfo` 的 5 个基础字段（interface/destination/gateway/hasGateway/isDefaultRoute）均为**必填且类型固定** —— 任何一个是 `undefined`/非声明类型，与「401 Parameter error」的参数级语义吻合（推断，待证实）。
5. `NetAddress.family` 缺省即 1（IPv4）；本例全部 IPv4，family 无嫌疑。
6. `dnsAddresses` 在 d.ts 仅声明为 `Array<string>`，无内容约束 —— 但见 §3.6：该字段**从未在真机上验证过**。

---

## 3. 我方构造代码审查（只读）

代码根：`client/entry/src/main/ets/vpnextensionability/`。

### 3.1 create() 调用链与平台配置组装

`NetBirdVpnExtensionAbility.ets`：

- L520-524：`withEndpointExclusions(baseTemplate, exclusion.routes)` 先嫁接派生排除集 → `applyNetworkConfig(base, net)` 再叠加 core 快照（net 非空时）。
- L530-543：默认路由安全门 `gateDefaultRoutes`（允许时 0.0.0.0/0 才进配置），日志 `VPN_DEFAULT_ROUTE_GATE`。
- L558：`logConfigApplied(merged)`（只打计数，见 §3.5）。
- L560：`buildPlatformVpnConfig(merged)` 生成平台 VpnConfig。
- L572：`connection.create(config)`；L577-579 失败记 `VPN_CREATE_REJECTED|phase=create|summary=...`。
- L1255-1266：成功侧 `settleCreate` → `VPN_CREATE_RESOLVED`。
- L1147-1157：受控重建（recreate）腿的 `createConnectionBounded` 调用同一 builder 产出的 config。

### 3.2 core 快照直通 —— 候选原因 A（字段直通 undefined/NaN）

- `NetBirdConnector.ets` L752：`return JSON.parse(core.connector_network_config()) as CoreNetworkConfig;` —— 快照是 **JSON.parse 直通，无字段级校验**。
- `NetBirdConnector.ets` L196-199：`CoreRouteEntry { network: string; is_default: boolean; }` —— 仅是 TS 声明，运行时不保证 `is_default` 存在。
- `NetBirdVpnConfig.ets` L141-156（`applyNetworkConfig`）：
  - L142-147：`network.split('/')` 只守卫「段数 = 2」，**不校验前缀是否为数字**——`Number(parts[1])`（L150）对非法尾段产生 `NaN`（`Number('')` 得 0，`Number('abc')` 得 NaN）。
  - L153：`isDefaultRoute: managed[i].is_default` —— **core 漏发该键时为 `undefined`**，直通进 RouteInfo 必填 boolean 位。
  - L151：managed 路由 gateway = `net.address`（快照地址）；快照 address 为空时回落 base 隧道地址（L128-131），此路无 NaN/undefined 风险。
- `NetBirdVpnConfig.ets` L163-168：`dnsServers.push(net.dns.servers[i].ip)` —— `ip` 键缺失时同样直通 `undefined` 进 `dnsAddresses`。

**推断依据**：§2.4 第 4 条 —— RouteInfo 必填 boolean 位收到 `undefined`（或 prefixLength 收到 NaN）是能落在「参数级 401」语义上的直接机制；而本仓库所有**字面量**构造路径（base 配置 L76-109、spike 矩阵）历史均成功，唯一直通外部数据的路径就是 core 快照。

### 3.3 排除路由派生与合并

`NetBirdEndpointExclusion.ets`：

- L189-230（`buildExcludedRouteEntries`）：解析成功的端点 → **/32（IPv4）或 /128（IPv6）** 排除路由；`gateway = 隧道地址, hasGateway: true`（L218-225）；严格 dotted-quad/含冒号分类（L79-107），非法即 fail，不产生畸形字面量。
- L246-264（`mergeExcludedRouteEntries`）：派生条目按 destination+prefix 去重，**只对既有条目保真追加**。

`NetBirdVpnConfig.ets`：

- L190-204（`withEndpointExclusions`）：嫁接后 routes = base(3 条) + 派生 /32 排除。
- L141-161（`applyNetworkConfig`）：**managed 路由与排除路由之间没有任何去重或冲突检查** —— 若 core 网络图恰好包含与某排除条目同 destination+prefix 的托管路由（如 192.168.50.0/24），同一前缀会以「排除 + 非排除」两种形态**并存**进 VpnConfig。此类矛盾条目是否被平台拒绝：SDK 无成文规则（待确认）。

### 3.4 平台字段映射

`NetBirdVpnConfig.ets` L238-271（`buildPlatformVpnConfig`）：

- L250：`interface: config.interfaceName`（恒 'vpn-tun'，L79）—— 与真机已接受的 spike 同款。
- L248/L252：destination family 按「是否含 ':'」推断为 2/1 —— 本例全 IPv4，恒 1。
- L255：gateway 恒 `family: 1`。
- L259-261：仅排除路由显式带 `isExcludedRoute: true`，其余省略（与 MR4 真机接受形态一致）。
- L264-269：`addresses`（1 条隧道地址/32）+ `routes` + **`dnsAddresses` 无条件带上** + `mtu: 1400`。
- 未使用 `vpnId`（可选，L290）；未使用 `trustedApplications/blockedApplications`（互斥风险不存在）。

### 3.5 可观测性缺口

- `NetBirdVpnExtensionAbility.ets` L1229-1246（`logConfigApplied`）：只打 `routes/defaultRoutes/excludedRoutes` **计数**与 address/interface/dns/mtu/endpoint，**无逐路由字段日志** → 本次失败配置的逐字段真实值无法从日志复核（观测里「每条路由的字段明细」来源不明，列为待复核线索）。
- L123-128（`safeError`）：只输出 `code,name,message`，**丢弃 stack** → 401 的细节只能依赖平台侧 hilog。

### 3.6 真机已验证形态对照（金标准基线）

`spikes/n1b-disc-phys-hap/entry/src/main/ets/vpnextensionability/N1BDiscVpnExtensionAbility.ets`：

- L109-135（**MR4**，2026-09-12 真机 ad-hoc 验证被平台接受；判定出处 `docs/n1b-disc-handoff-20260902.md` L126「MR4 默认路由被平台接受」——注意该判定系 ad-hoc 脚本判定、非门结论）：
  - 3 条路由：10.99.0.0/24（普通）；0.0.0.0/0 + isDefaultRoute: true + gateway 10.99.0.1 + hasGateway: true（L120-125）；**192.168.50.0/24 + isExcludedRoute: true + gateway 10.99.0.1 + hasGateway: true**（L126-132）。
  - config **无 `dnsAddresses` 字段**、无 managed 路由、排除路由只有 /24 一条。
- L137-152（MR1）：单条 10.99.0.0/24（无 isExcludedRoute 键）。

对照结论 —— 失败配置相对「全部真机已接受形态」的**增量**只有四类：

| # | 增量 | 真机先例 |
|---|---|---|
| a | core 快照派生的 managed 路由（字段经 JSON.parse 直通，§3.2） | **无**（spike 全部为字面量） |
| b | /32 主机排除路由（management/signal） | 无（已验证的排除只有 /24） |
| c | `dnsAddresses` 字段本身及其内容 | **无**（`NetBirdVpnConfig.ets` L14-16 自述：dnsAddresses「has NO on-device verification in any spike」） |
| d | 排除×托管同前缀并存/重复条目（§3.3） | 无 |

---

## 4. 假设排序表

| 序 | 假设 | 支持证据 | 最小证伪/证实操作（真机各一次） | 预期观测 |
|---|---|---|---|---|
| H1（最可能） | **core 快照直通产生非法必填字段**：某条 managed 路由 `is_default` 缺失 → `isDefaultRoute: undefined`（`NetBirdVpnConfig.ets:153`），或前缀段非数字 → `prefixLength: NaN`（`:150`）；必填位非法落在参数级校验 → 401 | §3.2 全链（JSON.parse 直通 `NetBirdConnector.ets:752` + 无字段守卫）；§2.4-4（5 个必填字段类型固定）；唯一直通外部数据的路径就是它；所有字面量路径历史全成功 | T6（§5）：在已验证基线上**故意**把一条托管路由的 `isDefaultRoute` 省写成 undefined 重放 | 401 复现 → H1 证实；成功 → H1 降级 |
| H2（次之） | **平台拒绝 /32 主机排除路由**（或同前缀「排除+托管」并存条目，§3.3）：未成文校验 | §3.6-b：真机仅验证过 /24 排除；/32 排除无先例；`applyNetworkConfig`（`NetBirdVpnConfig.ets:141-161`）不去重 | T4：在已验证基线上只追加 1 条 TEST-NET /32 排除（如 203.0.113.10/32，gateway+hasGateway 形态与派生一致） | 401 → H2 证实；成功 → /32 排除排除 |
| H3（再次） | **`dnsAddresses` 字段或其内容触发 401**（字段真机从未验证；内容来自 core 直通，`ip` 可能为 undefined） | §3.6-c + `NetBirdVpnConfig.ets:14-16` 自述未验证；L163-168 直通 | T3：已验证基线 + `dnsAddresses: ['8.8.8.8']` | 401 → H3 证实（字段级）；成功 → 字段本身无问题，若 T1 原样重放仍 401 则嫌疑转向 core 提供的 dns 内容 |
| 可排除 | 路由条数（5 条） | SDK 上限 1024/10000（`@ohos.net.vpnExtension.d.ts:300-306`） | 无需 | — |
| 可排除 | mtu=1400 | 范围 [576,1500]（同文件 L323-328），且 spike 同值被接受 | 无需 | — |
| 可排除 | `interface: 'vpn-tun'` / gateway=10.99.0.1+hasGateway / 默认路由形态 / LAN /24 排除形态 | MR4 真机接受（spike `N1BDiscVpnExtensionAbility.ets:109-135`） | 无需 | — |
| 可排除 | `isExcludedRoute` API 版本不可用 | @since 20 < 设备 26（`@ohos.net.connection.d.ts:2217`） | 无需 | — |
| 待观察 | 非默认 managed 路由的一般形态（任意前缀+网关）被拒 | 与 H1/H2 同属「平台未成文校验」空间 | T5：基线 + 2 条手工字面量 managed 路由（/16、/24，is_default 显式） | 401 → 单列为主因；成功 → 收敛到 H1/H2/H3 |

---

## 5. 最小设备验证计划（试验矩阵，6 个试验步 + 1 个固定前置）

通用纪律（对每一步生效，依据 §2.1 L194-196 与现有 fail-closed 语义 `NetBirdVpnExtensionAbility.ets:1271-1275`）：

- 每步 **create 前先 destroy** 前一代；步内失败立即 destroy，不留给半配置。
- 每步改变一个变量；每步可独立回滚（destroy 即回滚）。
- 全程按 §0 固定抓取 hilog。步骤计数：T1-T6 共 6 个试验步；T0 为固定前置（日志准备），不计入试验步。
- 试验配置的注入机制（debug 开关/临时构建）不在本诊断范围内，落地时另行决定。

### T0（固定前置：日志可得性，只读操作）

1. 复现前清缓冲：`hdc shell hilog -r`；增大缓冲（如 `hilog -G`/`--buffer-size`，具体参数名以设备 hilog 版本为准，**待确认**）。
2. **全量抓取、不按 level 过滤**落盘（平台 native 报错可能是 warn/error 之外的级别）。
3. 时间锚：以我方标记 `VPN_CREATE_BEGIN`（`NetBirdVpnExtensionAbility.ets:569-571`）/ `VPN_CREATE_REJECTED`（`:577-579`）/ `VPN_CREATE_RESOLVED`（`:1264-1266`）为界截取窗口；先按 TAG=`NetBirdVpn`（DOMAIN 0x2900，`:26-27`）过滤，再取同一 pid 相邻原生平级行（平台 VPN 服务的报错通常紧邻）。
4. 平台侧 vpn 相关 hilog domain/tag 编号未知：**待确认** —— T1 的全量日志即可一次性定位。

### 试验矩阵

| 步 | VpnConfig（相对差异） | 观察标记 | 成功判据 | 定位意义 |
|---|---|---|---|---|
| T1 | 原样重放本次失败配置 | `VPN_CREATE_REJECTED` + 平台侧 401 明细行 | 稳定复现 401 并**首次拿到平台侧详细文本** | 基线确认；若不复现 → 转向环境/时序因素，参数假设全部降级 |
| T2 | MR4 已验证 3 路由字面量（无 dns、无 managed、无 /32） | `VPN_CREATE_RESOLVED\|accepted=true` | accepted | 金标准复核；若失败 → 授权/设备状态问题，先修环境 |
| T3 | T2 + `dnsAddresses: ['8.8.8.8']` | 同上 | accepted | 失败 = H3 证实（dnsAddresses 字段级） |
| T4 | T2 + 1 条 /32 排除（TEST-NET 203.0.113.10/32，gateway/hasGateway 形态同派生） | 同上 | accepted | 失败 = H2 证实（/32 排除被拒） |
| T5 | T2 + 2 条手工 managed 路由字面量（/16、/24；`is_default` 显式 true/false；gateway=隧道地址） | 同上 | accepted | 失败 = managed 一般形态被拒（升级为主因）；成功 = 形态接受 |
| T6 | T5 基础上，把其中一条 managed 路由**故意省写 `isDefaultRoute`**（复刻直通 undefined） | `VPN_CREATE_REJECTED` | 401 复现 | 失败 = H1 证实（必填字段 undefined → 参数级 401）；成功 = H1 降级 |

### 判读树

- T3 失败 → dnsAddresses 字段；T4 失败 → /32 排除；T5 失败 → managed 形态；T6 失败（且 T5 成功）→ H1 字段直通。
- T2-T6 全成功而 T1 失败 → 差异只剩 **core 快照的实际内容**（managed 具体网段/dns 具体取值/重复条目 d 类）→ 下一步在 create 前对 platform config 做逐字段 dump（属代码改动，登记后续，不在本次只读范围）。
- T2 失败 → 非参数问题（授权、设备状态），停止参数假设。

---

## 6. 无法确证清单（均给出验证方法）

| # | 事项 | 状态 | 验证方法 |
|---|---|---|---|
| 1 | 401 的抛出层（napi 参数层 vs 服务层） | 推断，待证实 | T1 平台侧 hilog 明细 |
| 2 | 平台对 /32 排除、managed 主机路由、同前缀排除+托管并存的接受性 | SDK 无成文规则 | T4/T5/内容审计 |
| 3 | run7（routes=1）配置的确切字段构成 | 越界采集线索，无法复核；代码上最接近的形态是 `applyNetworkConfig` 在 core routes 为空时仅剩 LAN 排除 1 条（`NetBirdVpnConfig.ets:157-161`），且该形态与 MR4 route-3 同款——**推断** | 复核当日构建版本与快照 |
| 4 | 失败配置 5 条路由的逐字段真实值 | 现日志只打计数（`NetBirdVpnExtensionAbility.ets:1229-1246`） | 后续加逐路由 dump（代码改动）；短期用 T1 平台日志旁证 |
| 5 | 观测分解 1+2+3=6 ≠ routes=5 的矛盾来源 | 待复核 | T1 + §6-4 的 dump |
| 6 | 平台 vpn 服务 hilog 的 domain/tag | 待确认 | T0/T1 全量抓取一次定位 |
| 7 | `hilog` 增大缓冲的确切参数名（设备版本相关） | 待确认 | 设备上 `hilog --help` |

---

## 7. 敏感与边界声明

- 本文全部观测均「来自越界采集、仅作待复核线索」；未写入任何凭据/令牌（诊断过程未接触凭据材料）。
- 未修改任何代码或既有文档；未运行设备命令；未联网。
