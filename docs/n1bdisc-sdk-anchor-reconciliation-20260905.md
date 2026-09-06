# N1BDISC SDK 锚点对账（CLT 26.0.0.821 / SDK 26.0.0.105 ↔ `n1b-disc-gate-plan.md` 正文锚点）

落档日期：2026-09-05 ｜ 性质：只读对账存档（read-only reconciliation）｜ 结论：**PASS（9/9 类实测一致）**

对账对象：当前命令行工具链 **CLT 26.0.0.821（内含 Ohos_sdk_public 26.0.0.105，API Version 26）** 的实际文件内容 ↔ 冻结判据 `docs/n1b-disc-gate-plan.md`（状态行 `criteria-frozen-2026-09-02`；r26 终态——正文内修订标记最高 r26，冻结提交 `04cf222`「用户授权——第二十六轮三席全 pass 后」、register 冻结记录 `b701b97`）正文中引用 SDK/sysroot 的锚点主张。

## 0. 性质与边界声明（防误引）

- **本文件不是物理 evidence**：本对账在 linux-x64 开发机上对 SDK 安装树做只读读取，不含任何设备侧（PLA-AL10 / aarch64 真机）采集，不产生、不声称产生任何平台行为事实；不进入 `docs/evidence/` 记录链。
- **未分配 evidence ID**：本文件不持有、不占用任何 evidence 编号；后续任何记录如需引用，只能作为「开发机 SDK 内容对账」事实引用，不得写成设备实测。
- **未修改冻结判据**：`docs/n1b-disc-gate-plan.md` 全文未动；本对账不构成判据修订，不触发任何重新审查义务。
- **比较对象是 821 实际内容对冻结正文**：冻结正文引用的 461 绝对路径（`/home/worker/harmonyos/command-line-tools/26.0.0.461/...`）**已不存在**——`/home/worker/harmonyos/command-line-tools/` 下现仅有 `26.0.0.821/` 与符号链接 `current -> 26.0.0.821`。461 时代的行号引用无法对 461 逐行复验；本对账按任务定义以 **821 文件的实际内容**逐项复核冻结**正文主张**（常量值、结构形态、声明存在性、接口字段与签名、@since），并把 821 实际行号逐项记录在案。821 与 461 非同一安装包（版本号不同），本对账不主张 821 == 461，只主张冻结正文锚点主张在 821 上全部成立。
- 每一项均由本对账执行者**实际打开 821 文件读取并逐字比对**，非转述他人结论；关键文件 SHA-256 见 §2，可独立复核。
- **行号编号基线**：本文引用 `docs/n1b-disc-gate-plan.md` 的行号为冻结 commit `04cf222` 编号（CC-1 变更 +2 位移不适用于本文；现行文件对应行为本文行号 +2）。

## 1. 工具链身份（绝对路径与版本）

| 项 | 值 |
| --- | --- |
| CLT 根目录 | `/home/worker/harmonyos/command-line-tools/26.0.0.821/`（`current -> 26.0.0.821`） |
| CLT 版本 | `version.txt`：`Command Line Tools(linux-x64) Version: 26.0.0.821` |
| SDK 版本 | `version.txt`：`HarmonyOS SDK : HarmonyOS 26.0.0 Release (include Ohos_sdk_public 26.0.0.105 (API Version 26 Release))`；`sdk/default/sdk-pkg.json`：`"version": "26.0.0.105"`、`"apiVersion": "26"`、`"releaseType": "Release"` |
| native 编译器 | `/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/native/llvm/bin/clang` — `OHOS (dev) clang version 15.0.4 (llvm-project 329916b990b43824d4b7e67de911fee7a966b1c8)` |
| sysroot 根 | `/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/native/sysroot/usr/include/` |
| ArkTS d.ts 目录 | `/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/ets/api/` |
| 461 遗留 | 无——`/home/worker/harmonyos/command-line-tools/` 下不存在 `26.0.0.461` |

以下 §2–§3 中 `SRC` = sysroot 根、`DTS` = d.ts 目录（上两行绝对路径的缩写）。

## 2. 实测文件清单（SHA-256，供独立复核）

| 文件（相对 sysroot/ets 根） | 行数 | SHA-256 |
| --- | --- | --- |
| `poll.h` | 59 | `cf31c075c721bed8c66fe4a03d31410cc933e8e984a734bcfbd82390e50a1691` |
| `asm-generic/poll.h` | — | `62ee61e46039a24e436e028d114bd20d0bb5369af376de73c4090dac9c59c1ef` |
| `pthread.h` | 378 | `8a103cb38f3e0e7583ac959941310b4b4d8f9200dbdf2b26f2faa8e0c3f49e6b` |
| `linux/if_tun.h` | — | `2ea3dc307472bb29a1b52a1bb890be374eb672cc9e195536bfb95ccf135c1780` |
| `linux/if_ether.h` | — | `66d939b87ea39db94bd4ef5981746a71766c19cdb4860c9840ff566da00ad263` |
| `asm-generic/unistd.h` | — | `a6bb0f6788203131416c79708eac426b68fcbfce7713f79d900ffa3ad2af783f` |
| `aarch64-linux-ohos/bits/syscall.h` | 603 | `ecc074a87945efea65a14323f0e63981f33684fe564e7e580e23330498734524` |
| `fcntl.h` | — | `ea90e0d85136139de418021cba80f51195bee20bbbf899811af35a36d5947c5d` |
| `ets/api/@ohos.net.connection.d.ts`（2571 行） | 2571 | `8cc5c139dbf43b6b38100ceb692aa2577d602acefcae7124b86fce842c88b7c5` |
| `ets/api/@ohos.net.vpnExtension.d.ts`（391 行） | 391 | `5a5e5910b65f57f77b52ed9e5143d6617025cf22203f1c853dd0ee5dcd442589` |

## 3. 逐项对账（9 类）

**判定口径**：冻结正文主张（语义内容）与 821 实测内容一致 → PASS；任一主张不成立 → 该项 FAIL 并阻塞。行号单列记录：冻结正文引用的 461 行号不在对账域内（461 已不存在），821 实际行号照实登记；冻结行号与 821 行号**恰好相同**处单独注明。

### 3.1 `poll.h` 常量 — PASS

- **冻结主张**（`n1b-disc-gate-plan.md:811`）：「`/…26.0.0.461/.../usr/include/poll.h:12-17`」定义 `POLLIN=0x001`、`POLLPRI=0x002`、`POLLOUT=0x004`、`POLLERR=0x008`、`POLLHUP=0x010`、`POLLNVAL=0x020`。
- **821 实测**：`$SRC/poll.h:12-17` 逐字为：
  `#define POLLIN     0x001`（:12）、`#define POLLPRI    0x002`（:13）、`#define POLLOUT    0x004`（:14）、`#define POLLERR    0x008`（:15）、`#define POLLHUP    0x010`（:16）、`#define POLLNVAL   0x020`（:17）。
- **结果**：六个常量值与冻结掩码全集逐一相等，且**行号 12-17 与冻结引用完全相同**。

### 3.2 `asm-generic/poll.h` 常量一致性 — PASS

- **冻结主张**（`:811`）：与 `asm-generic/poll.h:21-26` 一致。
- **821 实测**：`$SRC/asm-generic/poll.h:21-26` 逐字为：
  `#define POLLIN 0x0001`（:21）、`#define POLLPRI 0x0002`（:22）、`#define POLLOUT 0x0004`（:23）、`#define POLLERR 0x0008`（:24）、`#define POLLHUP 0x0010`（:25）、`#define POLLNVAL 0x0020`（:26）。
- **结果**：与 `poll.h` 数值一致（3 位 vs 4 位零填充，数值相等），与冻结「一致」主张成立；**行号 21-26 与冻结引用完全相同**。

### 3.3 `pthread.h` 唯一 join 且无 timed/try join — PASS

- **冻结主张**（`:357` 与 `:717`）：sysroot `pthread.h` 仅声明 `pthread_join`（`int pthread_join(pthread_t, void **)`），无 `pthread_timedjoin_np`/`pthread_tryjoin_np` 等任何 timed/try join 变体。
- **821 实测**：`$SRC/pthread.h:91` = `int pthread_join(pthread_t, void **);`——全文件（378 行）中 `join` 仅此一处命中；`timedjoin|tryjoin` 在 `pthread.h` 零命中；对整个 sysroot `--include=*.h` 的有界检索中 `timedjoin_np|tryjoin_np` 亦零命中。
- **结果**：唯一 join 声明 + 无任何有界/非阻塞回收变体，两项主张均成立。冻结正文未钉 join 声明行号，`:91` 为本次实测登记。

### 3.4 `tun_pi` 结构 — PASS

- **冻结主张**（`:547`）：`struct tun_pi { __u16 flags; __be16 proto; }`（`native/sysroot/usr/include/linux/if_tun.h:76-79`）。
- **821 实测**：`$SRC/linux/if_tun.h:76-79` 逐字为：
  `struct tun_pi {`（:76）、`  __u16 flags;`（:77）、`  __be16 proto;`（:78）、`};`（:79）。
- **结果**：结构与冻结主张逐字一致，**行号 76-79 与冻结引用完全相同**。由此「带 PI 的 IPv4 帧前 4 字节典型为 `00 00 08 00`」的推导输入（flags 2 B + 大端 proto=`ETH_P_IP` 2 B）在 821 上保持成立。

### 3.5 `ETH_P_IP` — PASS

- **冻结主张**（`:547`）：`ETH_P_IP = 0x0800`（`native/sysroot/usr/include/linux/if_ether.h:36`）。
- **821 实测**：`$SRC/linux/if_ether.h:36` = `#define ETH_P_IP 0x0800`。
- **结果**：值与行号均一致，**行号 36 与冻结引用完全相同**。

### 3.6 两个 ppoll syscall 定义 + aarch64 目标头 — PASS

- **冻结主张**（`:836`）：`__NR_ppoll_time64 414` 位于 `asm-generic/unistd.h` 的 `#if __BITS_PER_LONG == 32` 块内；aarch64 目标头 `aarch64-linux-ohos/bits/syscall.h:74` **只有** `__NR_ppoll 73`（源 `asm-generic/unistd.h:113`）；poll 族冻结集 `{73}`。
- **821 实测**：
  - `$SRC/asm-generic/unistd.h:113` = `#define __NR_ppoll 73`（**行号与冻结引用完全相同**）；其外层守卫为 ：111 `#if defined(__ARCH_WANT_TIME32_SYSCALLS) || __BITS_PER_LONG != 32`（aarch64 的 `__BITS_PER_LONG==64` 下该条件为真，`__NR_ppoll` 对 aarch64 可见）。
  - `$SRC/asm-generic/unistd.h:375` = `#define __NR_ppoll_time64 414`，实测位于 ：363 `#if __BITS_PER_LONG == 32` 与 ：384 `#endif` 之间——确在 32 位条件块内。
  - `$SRC/aarch64-linux-ohos/bits/syscall.h`：`__NR_ppoll*` 族仅 ：74 = `#define __NR_ppoll 73`（**行号与冻结引用完全相同**）；全文件无 `__NR_ppoll_time64`（:376 的 `#define SYS_ppoll 73` 是同一 syscall 的 `SYS_` 别名，非第二个 `__NR_` 定义）。
- **结果**：`asm-generic/unistd.h` 上恰两个 ppoll 定义（:113 `__NR_ppoll 73`、:375 `__NR_ppoll_time64 414`@32 位块），aarch64 目标头仅有 `__NR_ppoll 73`——冻结「冻结集退回 `{73}`」的全部输入在 821 上成立。

### 3.7 `F_DUPFD_CLOEXEC` — PASS

- **冻结主张**（`:408`）：「`F_DUPFD_CLOEXEC` 在 API 26 sysroot 已定义」，依据链 `docs/n1b-gate-plan.md:76`（该行记 `fcntl.h:53`、cmd `1030`）。
- **821 实测**：`$SRC/fcntl.h:53` = `#define F_DUPFD_CLOEXEC 1030`；另 `$SRC/linux/fcntl.h:26` = `#define F_DUPFD_CLOEXEC (F_LINUX_SPECIFIC_BASE + 6)`（1024+6=1030，两处一致）。
- **结果**：API 26（821）sysroot 中已定义、cmd 1030，主张成立；`fcntl.h:53` 行号与 `n1b-gate-plan.md:76` 记录完全相同。

### 3.8 ArkTS d.ts 五接口形态 — PASS

冻结摘录见 `n1b-disc-gate-plan.md:307-343`（「SDK 依据」节）。以下为 821 实际形态（注意：821 的两份 d.ts 均把接口声明在 `declare namespace` 内、带缩进前缀 `export interface`，字段 @since 以 doc 注释为准）：

**RouteInfo**（`$DTS/@ohos.net.connection.d.ts:2169-2218`，`declare namespace connection` 内）：
`interface: string;`（:2176，必填，@since 8）、`destination: LinkAddress;`（:2183，必填，@since 8，对象类型）、`gateway: NetAddress;`（:2190，必填，@since 8，对象类型）、`hasGateway: boolean;`（:2198，必填，@since 8）、`isDefaultRoute: boolean;`（:2209，必填，@since 8）、`isExcludedRoute?: boolean;`（:2217，可选，@since 20）——字段集、顺序、必填/可选、@since 与冻结摘录逐项一致（含「本 campaign 不设置 isExcludedRoute」的前提仍成立）。

**LinkAddress**（`:2225-2240`）：`address: NetAddress;`（:2232，必填，@since 8）、`prefixLength: number;`（:2239，必填，@since 8）——一致。

**NetAddress**（`:2249-2277`）：`address: string;`（:2258，必填，@since 8）、`family?: number;`（:2267，可选，@since 8；doc：「The value is **1** for IPv4 and **2** for IPv6. The default value is **1**.」）、`port?: number;`（:2276，可选，@since 8；doc：「The value range is [0, 65535]. The default value is **0**.」）——与冻结摘录「1=IPv4，2=IPv6，默认 1」「0-65535」一致。

**VpnConfig**（`$DTS/@ohos.net.vpnExtension.d.ts:283-389`，`declare namespace vpnExtension` 内）：实际字段全集按序为 `vpnId?: string`（:290，@since 20）、`addresses: Array<LinkAddress>`（:298，必填，@since 11）、`routes?: Array<RouteInfo>`（:306，@since 11）、`dnsAddresses?: Array<string>`（:314）、`searchDomains?: Array<string>`（:321）、`mtu?: number`（:328）、`isIPv4Accepted?: boolean`（:338）、`isIPv6Accepted?: boolean`（:348）、`isInternal?: boolean`（:356）、`isBlocking?: boolean`（:364，@since 11；doc：「The default value is **false**」）、`trustedApplications?: Array<string>`（:376）、`blockedApplications?: Array<string>`（:388）（:314–:388 各可选字段均 @since 11）——冻结摘录列出的 `addresses`（必填）/`routes?`/`mtu?`/`isBlocking?`（默认 false）及「不设置」注释枚举的 `dnsAddresses?/searchDomains?/isIPv4Accepted?/isIPv6Accepted?/isInternal?/trustedApplications?/blockedApplications?/vpnId?` 在 821 上全部存在且可选性一致；821 将 `vpnId?` 排在 `addresses` 之前属字段排列差异，冻结摘录本就未主张字段顺序，且 `vpnId?` 已在冻结「均不设置」注释内。

**ConnectionProperties**（`$DTS/@ohos.net.connection.d.ts:2099-2162`）：`interfaceName: string`（:2106）、`domains: string`（:2113）、`linkAddresses: Array<LinkAddress>`（:2120）、`dnses: Array<NetAddress>`（:2127）、`routes: Array<RouteInfo>`（:2134）、`mtu: number`（:2141，均必填，@since 8）、`isIPv4LinkValid?: boolean`（:2151）、`isIPv6LinkValid?: boolean`（:2161，可选，`@stagemodelonly`，@since 24）——字段集、顺序、必填性、@since 与冻结摘录逐项一致。`mtu` 实际位于 **:2141**：`mtu: number;`（必填非可选、@since 8、doc「Maximum transmission unit (MTU).」）。

**行号引用漂移登记**（冻结 `:505` 引 461 行号，821 实际行号照实记录；461 已不存在，逐行复验不可行，主张内容层面全部成立）：

| 锚点 | 冻结引用（461） | 821 实际 | 内容主张 |
| --- | --- | --- | --- |
| `ConnectionProperties.mtu` | `:2221` | `:2141` | 存在、必填 number、@since 8 ✓ |
| `getConnectionProperties`（callback/Promise） | `:248`/`:263` | `:251`/`:270` | 两重载存在、@since 8、返回 `ConnectionProperties` ✓ |
| `getConnectionPropertiesSync` | `:278` | `:289` | 存在、`function getConnectionPropertiesSync(netHandle: NetHandle): ConnectionProperties;`、@since 10 ✓ |

### 3.9 `create` 签名 — PASS

- **冻结主张**（`:273`）：`create(config): Promise<number>` 只返回 fd（U5 口径）。
- **821 实测**：`$DTS/@ohos.net.vpnExtension.d.ts:210` = `create(config: VpnConfig): Promise<number>;`（`VpnConnection` 接口成员，@since 11；doc `@returns { Promise<number> } … which is the file descriptor of the virtual network interface card (vNIC).`）。
- **结果**：签名与「只返回 fd」语义一致。附带同文件实测：`destroy(): Promise<void>`（:255，@since 11）与 `destroy(vpnId: string): Promise<void>`（:266，@since 20），与冻结 D 项「destroy 是唯一关闭责任方」所引接口面相符。

## 4. 结论

**PASS**——§3.1–§3.9 共 9 类锚点在 CLT 26.0.0.821 / SDK 26.0.0.105 实际内容上**全部实测一致**，无一不符，无阻塞项：
poll 常量两处（含行号一致）、pthread 唯一 join 且零 timed/try 变体、`tun_pi` 形态（行号一致）、`ETH_P_IP=0x0800`（行号一致）、两个 ppoll 定义与 aarch64 仅 `__NR_ppoll 73`（行号一致）、`F_DUPFD_CLOEXEC 1030`（行号一致）、五个 d.ts 接口形态与 `create` 签名（内容一致；`:505` 所引 461 行号在 821 上漂移，漂移量已在 §3.8 表中登记，属 461→821 安装包差异，非内容不符）。

本结论的效力边界重申 §0：这是开发机 SDK 内容对账，不构成设备侧物理证据；冻结判据的运行时可用性条款（如 `F_DUPFD_CLOEXEC` 运行时行为由 D2 实测登记）不受本对账影响。

## 5. 核验命令（复现用）

```bash
# 工具链版本
cat /home/worker/harmonyos/command-line-tools/26.0.0.821/version.txt
cat /home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/sdk-pkg.json
/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/native/llvm/bin/clang --version
ls /home/worker/harmonyos/command-line-tools/   # 仅 26.0.0.821 与 current -> 26.0.0.821；461 不存在

SRC=/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/native/sysroot/usr/include
DTS=/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/ets/api

# 3.1/3.2 poll 常量
sed -n '12,17p' $SRC/poll.h
sed -n '21,26p' $SRC/asm-generic/poll.h

# 3.3 pthread 唯一 join
grep -n "join" $SRC/pthread.h                       # 仅 :91 pthread_join
grep -rn "timedjoin\|tryjoin" $SRC --include=*.h    # 零命中

# 3.4/3.5 tun_pi / ETH_P_IP
sed -n '76,79p' $SRC/linux/if_tun.h
sed -n '36p' $SRC/linux/if_ether.h

# 3.6 ppoll 两定义与 aarch64 头
grep -n "__NR_ppoll" $SRC/asm-generic/unistd.h                       # :113 73 / :375 414
awk 'NR==363||NR==375||NR==384' $SRC/asm-generic/unistd.h            # 414 在 32 位块 363-384 内
grep -n "ppoll" $SRC/aarch64-linux-ohos/bits/syscall.h               # :74 __NR_ppoll 73 / :376 SYS_ppoll 73

# 3.7 F_DUPFD_CLOEXEC
grep -n "F_DUPFD_CLOEXEC" $SRC/fcntl.h            # :53 1030
grep -n "F_DUPFD_CLOEXEC" $SRC/linux/fcntl.h      # :26 (F_LINUX_SPECIFIC_BASE + 6)

# 3.8/3.9 d.ts 接口与 create
grep -n "interface RouteInfo\|interface LinkAddress\|interface NetAddress\|interface ConnectionProperties" $DTS/@ohos.net.connection.d.ts
grep -n "interface VpnConfig\|create(config\|destroy(" $DTS/@ohos.net.vpnExtension.d.ts
grep -n "getConnectionProperties\|mtu:" $DTS/@ohos.net.connection.d.ts

# 文件指纹
sha256sum $SRC/poll.h $SRC/asm-generic/poll.h $SRC/pthread.h $SRC/linux/if_tun.h \
  $SRC/linux/if_ether.h $SRC/asm-generic/unistd.h $SRC/aarch64-linux-ohos/bits/syscall.h \
  $SRC/fcntl.h $DTS/@ohos.net.connection.d.ts $DTS/@ohos.net.vpnExtension.d.ts
```

## 6. 变更范围

本次仅新增本文件 `docs/n1bdisc-sdk-anchor-reconciliation-20260905.md`；未修改 `docs/n1b-disc-gate-plan.md`、`docs/evidence/`、`spikes/` 或任何其他文件（冻结判据 `git diff` 为空已核）。对账执行前工作区已有的未跟踪项（`docs/toolchain-baseline.md`、`scripts/`）非本对账产物、未触碰；对账期间工作区另出现**并行修改**（`docs/toolchain-runbook.md`、`docs/toolchain-bootstrap.md`、`docs/development-environment.md` 的未暂存改动，落档期间陆续新现）——均非本对账产物，按并行共存原则未触碰，提请主会话知悉。
