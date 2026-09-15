# 真机运行手册 — (A) 生产 HTTPS/CA 登录烟测 + (B) N2-H relay TCP 端点冻结/排除（2026-09-14 授权窗口）

> **写作声明**：本手册只描述操作，写作时**未执行任何设备命令**（hdc / uitest / install / adb 一律未运行）。
> 所有命令、路径、标记名、字段名均注明出处（file:line）；无法确证之处标 **待确认**（汇总见 §8）。
> 命令以 `docs/device-validation-run1-handoff-20260914.md`（下称 **材料1**）为最高优先；本文与其冲突时以材料1为准。

## 0. 授权与适用范围（对齐 AUTH，逐条）

授权文件（只读）：`~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002.json`（下称 **AUTH**；含 `.sha256` sidecar，已确认存在）。

| 项 | 值 | 出处 |
|---|---|---|
| 有效期 | `2026-09-14T14:29:26+08:00` 起，**至 `2026-09-15T14:29:26+08:00` 止**；单会话或 24h 取小，过期即失效、须新签，不得顺延 | AUTH `valid_from`/`valid_until`/`validity_rule` |
| 消费语义 | session-scoped；同一 AUTH 会话内可多次执行白名单内操作，每次留执行记录；**`retry_allowed:false`**（失败重试须新签） | AUTH `consumption_semantics` |
| 目标 bundle | 仅 `cn.alfadb.netbird`（debug bundle） | AUTH `target_bundle` |
| 目标设备 | `hdc tconn 192.168.50.199:37193`（TCP）；hdc 二进制 `/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/hdc` | AUTH `target_device_connection`；材料1 L54-55 |
| 证据落点 | `/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/`（含子目录），逐文件 `.sha256` sidecar + `MANIFEST.sha256` + `is_evidence:false` 声明 | AUTH `evidence_destination` 及 rules |
| 结论授权 | **本 AUTH 不授权产生任何 verdict/claim/门级结论**；N2-H 结论须独立审查后方可登记 | AUTH `forbidden` 第2条；`docs/n2h-isolation-evidence-notes.md` §0/§6 |

**顺序硬前置**（`docs/n13-relay-increment-plan-20260914.md` §5，L68-76）：
`[默认路由下启用 WSS]` 的硬前置 = **relay TCP 端点已纳入 N2-H 冻结并排除** —— 即本手册 (B)。
`(A)` 与 `(B)` 相互独立：**(A) 不启用 relay**（`relay_enabled` 保持 false），只验证 TLS/CA 与登录。

---

## 1. 前置条件（全部只读检查）

### 1.1 设备与连接

```bash
# 手机：开机已解锁；与 pod 同一局域网（192.168.50.0/24）；已开启无线调试（hdc TCP 模式）
HDC=/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/hdc

"$HDC" list targets                 # 仅限连接维持所需（AUTH allowed 第2条；禁止扩大用途的设备枚举）
"$HDC" tconn 192.168.50.199:37193   # AUTH allowed 第1条；材料1 L63-64
"$HDC" list targets                 # 复核目标已在线（同一维持目的）
```

**失败处理**：`tconn` 不成功 → **停止并上报，不得反复重试**（与 AUTH `retry_allowed:false` 一致；见 §6.6）。

只读设备核验（AUTH allowed 第3条）：

```bash
"$HDC" shell "param get const.product.software.version"   # 型号/版本记录进执行记录
"$HDC" shell "bm dump -n cn.alfadb.netbird | head -20"    # 已装版本（未安装则无输出，属预期）
"$HDC" shell "pidof cn.alfadb.netbird"                    # 进程查询仅限本项目 bundle
```

### 1.2 pod 侧到生产实例可达（只读 curl）

端点与期望码逐项来自 `~/harmonyos-signing/netbird-n1bdisc/records/prod-management-relay-probe-20260914.md` §1 表（2026-09-14 实测：api 200 / signal 302 / home-relay healthz 200 / `/relay` 426）：

```bash
curl -sS -o /dev/null -w '%{http_code} %{ssl_verify_result}\n' https://api.netcenter.alfadb.cn/api/instance   # 期望: 200 0
curl -sS -o /dev/null -w '%{http_code} %{ssl_verify_result}\n' https://signal.netcenter.alfadb.cn/            # 期望: 302 0
curl -sS -o /dev/null -w '%{http_code} %{ssl_verify_result}\n' https://home.alfadb.cn:28443/healthz           # 期望: 200 0
curl -sS -o /dev/null -w '%{http_code} %{ssl_verify_result}\n' https://home.alfadb.cn:28443/relay             # 期望: 426 0
```

`%{ssl_verify_result}`=0 表示证书链对系统根校验通过。该检查形态是对探测记录 §1 表的等效重构（**待确认 ⑦**：记录中「verify=0」列的原始命令未落仓）。
任何一项不通 → 停止 (A)，先上报运维（网络/白名单问题不归本手册处理）。

### 1.3 一次性 setup key（本手册不含任何真实 key）

- 按 `docs/ops-setup-key-request-prompt-20260914.md` 向运维索取：`usage_limit=1`、≤24h、peer 建议名 `ohos-smoke-1`（该文档 L11-13）。
- **2026-09-14 零代码确认所用一次性 key 已被消耗**（探测记录 §4），本轮必须用**新 key**。
- key 只落 **600 权限文件**（建议 `~/netbird-interop/relay-prod/`，与 2026-09-14 探测同目录约定），**不得**打印到终端/日志/提交/写进本文档（AUTH allowed 第10条；探测记录「凭据纪律」段）。

### 1.4 签名材料就绪（只读存在性检查；路径全部实测存在）

```bash
ls /home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/lib/hap-sign-tool.jar
ls /home/worker/harmonyos-signing/netbird-n1bdisc/candidate-signing/20260911-app-native-6014363/N1bdiscSignLauncher.java
ls /home/worker/harmonyos-signing/netbird-e3/private/netbird-harmonyos.p12
ls /home/worker/harmonyos-signing/netbird-e3/cert/NetBird\ E3\ Debug.cer
ls /home/worker/harmonyos-signing/netbird-n1bdisc/profiles/    # 现存: NetBird HarmonyOS Debug ACL.p7b 等
```

当晚生效的 profile `.p7b` 与 p12/cert 组合的绑定**待确认 ①**（材料1 §4 用 `<新 profile>.p7b` 占位，仓B 未登记当晚绑定）。

---

## 2. (A) 真机 HTTPS/CA 登录烟测

> 目标：客户端**仅凭打包进 HAP 的公共根证书**完成对生产 management `https://api.netcenter.alfadb.cn` 的 TLS 握手与登录。
> **本次不启用 relay**：设备配置通道不携带任何 relay 字段，`relay_enabled` 保持默认 false（`client/core/src/connector.rs:696-704`、`:784`；壳侧 `buildConnectorConfigJson` 不写 relay 字段，`client/entry/src/main/ets/vpnextensionability/NetBirdConnector.ets:343-364`）。

### 2.1 构建 HAP（命令照 `client/build.sh` 用法，build.sh L7-9）

```bash
cd /home/worker/work/base/netbird-harmonyos && bash client/build.sh
```

产物：`client/entry/build/default/outputs/default/entry-default-unsigned.hap`（build.sh L22）。
验收：脚本末尾 `VERIFY OK`（`.so` 在包内且 sha256 一致、ELF64 AArch64、`pack.info` apiVersion target/compatible=26）+ 打印的 HAP 路径/size/**sha256**。sha256 记入执行记录（AUTH allowed 第5条要求 HAP sha256 逐次登记）。

### 2.2 签名（命令照材料1 §4，L77-85，逐字）

```bash
TOOL=/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/lib/hap-sign-tool.jar
javac -cp "$TOOL" -d /tmp/signclasses \
  /home/worker/harmonyos-signing/netbird-n1bdisc/candidate-signing/20260911-app-native-6014363/N1bdiscSignLauncher.java
java -cp "$TOOL:/tmp/signclasses" N1bdiscSignLauncher \
  /home/worker/harmonyos-signing/netbird-e3/private/netbird-harmonyos.p12 \
  /home/worker/harmonyos-signing/netbird-e3/private/netbird-harmonyos.p12.pass \
  /home/worker/harmonyos-signing/netbird-e3/cert/"NetBird E3 Debug.cer" \
  /home/worker/harmonyos-signing/netbird-n1bdisc/profiles/"<当晚生效 profile>.p7b" \
  client/entry/build/default/outputs/default/entry-default-unsigned.hap /tmp/netbird-ca-smoke-signed.hap netbird-harmonyos 26
```

`<当晚生效 profile>.p7b` 按材料1 §2/L84 语义选取（须覆盖 `ohos.permission.MANAGE_VPN` 的 ACL profile）；具体文件**待确认 ①**。
签名兼容性参数 `netbird-harmonyos 26` 照材料1 原样（材料1 §4 L85）。

### 2.3 设备配置文件（主机侧准备；600 权限；不入仓库）

`netbird-device-config.json` 字段名以壳侧接口为准（`NetBirdConnector.ets:230-236`：`management_url` 必填、`private_key` 必填、`ca_pem?`/`server_name?`/`hostname?` 可选；核验逻辑 `:282-291`）：

```json
{
  "management_url": "https://api.netcenter.alfadb.cn",
  "private_key": "<新生成：base64 std, 32 字节>",
  "hostname": "ohos-smoke-1"
}
```

- **故意不写 `ca_pem`**：`management_url` 为 `https://` 且无显式 `ca_pem` 时，壳侧注入 HAP 内打包根证书 rawfile（`NetBirdVpnExtensionAbility.ets:335-356`）——这正是本烟测要验证的路径。显式 `ca_pem` 优先（`:336-339`），写了就测不到打包根。
- `private_key` 格式依据 `connector.rs:717`（`"<base64 std, 32 bytes>"`，解析 `:808-822`、缺失即配置错误 `:894-898`）。**生成命令待确认 ⑤**：沿用 2026-09-14 生产探测的做法（仅落 600 文件，仓内未登记生成命令），不得照抄历史文件值。
- setup key 写入**单独文件**（如 `~/netbird-interop/relay-prod/setupkey-smoke`，`chmod 600`），全程不回显。

```bash
chmod 600 <device-config.json> <setup-key 文件>   # 落盘后立即收紧
```

### 2.4 推送配置 + 安装（命令照材料1 §4，L88-90）

```bash
"$HDC" file send <本机 device-config.json 路径> /data/local/tmp/netbird-device-config.json
"$HDC" file send <本机 setup-key 文件路径>      /data/local/tmp/netbird-setup-key
"$HDC" install -r /tmp/netbird-ca-smoke-signed.hap
```

设备侧文件名须与壳侧默认一致：`netbird-setup-key` / `netbird-device-config.json`（`NetBirdConnector.ets:39-40`）；App 经开发期通道读 `/data/local/tmp` 同名覆盖（`NetBirdConnector.ets:43` `DEV_CREDENTIAL_DIR`、`:254-274`）。

**保护偏离分支（仅当 protect 仍被拒时）**：若签名 profile 未带 ACL 或启动中止于 `stage=mgmt-protect`，按代码既定开发期偏离通道（用户已批准 2026-09-14；`NetBirdVpnExtensionAbility.ets:273-302`）：

```bash
printf 'route-exclusion' > /tmp/netbird-protect-deviation
"$HDC" file send /tmp/netbird-protect-deviation /data/local/tmp/netbird-protect-deviation
```

该模式打 `VPN_PROTECT_DEVIATION_ARMED|...|mode=route-exclusion|...|not_a_gate_pass=true`（`:297-301`），**永不是门 pass**；走 ACL 成功则不需要此文件。两分支以当晚 hilog 实测为准（**待确认 ②**）。

### 2.5 启动 + 点击（命令照材料1 §4，L91-92）

```bash
"$HDC" shell "sh /data/local/tmp/start-netbird.sh"   # want 参数注入凭据（沙箱读不到 /data/local/tmp；脚本为 run1 遗留，不重写其内容）
"$HDC" shell "uitest uiInput click 658 1661"         # Connect VPN（App 必须在前台；系统 VPN 授权弹窗点「允许」）
```

每次点击的坐标/控件/目的**逐条登记**（AUTH allowed 第7条）。坐标 `658 1661` 为 run1 值；若当晚 UI 不一致，用 `uitest dumpLayout`（只读，同条）重新定位（**待确认 ⑥**）。

### 2.6 抓 hilog 关键标记（基础 pattern 照材料1 §4，L93；CA 标记为本手册扩展）

```bash
"$HDC" shell "hilog -x | grep -aE 'NetBirdVpn|N1BDiscVpn|N1BDISC_|N6_|N7_|VPN_CA_ROOTS|VPN_CONNECTOR|VPN_PROTECT|VPN_CONNECTION_STATUS' | tail -80" \
  | tee /home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/hilog-ca-smoke-<HHMM>.log
```

（壳侧 TAG=`NetBirdVpn`、DOMAIN=`0x2900`，`NetBirdVpnExtensionAbility.ets:21-22`；core 侧标记经 hilog 透出，grep 集合沿用材料1。）

### 2.7 判定标准

**成功（缺一不可）**：

1. `VPN_CA_ROOTS_SOURCE|requestId=…|source=rawfile|file=nb-public-roots.pem|bytes=…|certs=2`（打包根证书注入生效；rawfile 为 ISRG Root X1/X2 两张证书，`client/entry/src/main/resources/rawfile/nb-public-roots.pem`，提交 `e257bb0`；注入逻辑 `NetBirdVpnExtensionAbility.ets:340-347`）
2. 无 `VPN_CA_ROOTS_FAIL`、无 `VPN_CONNECTOR_ABORTED`
3. `VPN_CONNECTOR_STARTED|…|connectAddr=<api 解析 IP>`（`:428-430`）
4. `connector: login ok (peer address assigned)`（`connector.rs:3565`）
5. `connector: network map applied serial=… peers=… routes=…`（`connector.rs:1815`）与 `connector: signal stream registered (ice initiation armed)`（`connector.rs:2935`）
6. 系统 VPN 建立：`VPN_CONNECTION_STATUS_CHANGED`（材料1 §1 表里程碑⑥）
7. **relay 负断言**：日志中不出现 `netbird-config relay urls=…`（`connector.rs:1667`）及任何 relay/WSS 拨号痕迹——本轮 `relay_enabled=false`，出现即说明配置被误改，按失败处理

**失败看什么**：

| hilog 形态 | 含义 | 处置 |
|---|---|---|
| `VPN_CA_ROOTS_FAIL|stage=rawfile|errorClass=https-ca-roots-missing`（`:349-352`） | HAP 内 rawfile 缺失/空 | 构建问题，回 §2.1 |
| start 后 TLS 证书错误/退避（具体错误文本**待实测**，不预设） | 打包根未覆盖生产证书链 | 留存 hilog 原始段上报；不得改用系统信任库绕过 |
| `VPN_CONNECTOR_ABORTED|stage=credentials|errorClass=credentials-missing`（`:315-318`） | `/data/local/tmp` 文件未送到/名字不符 | 修正 §2.4 文件名 |
| `VPN_CONNECTOR_ABORTED|stage=dns`（`:364-368`） | 设备侧 DNS 失败 | 记录后上报 |
| `VPN_CONNECTOR_ABORTED|stage=mgmt-protect|errorClass=protect-fail-closed`（`:399-403`） | ACL 未生效 | 走 §2.4 偏离分支或修 profile |
| 登录被拒 | setup key 无效/已消耗 | 向运维换新 key；**不重试注册**（`retry_allowed:false`） |

---

## 3. (B) N2-H：relay TCP 端点纳入冻结与排除，重跑 TUN 包级负证据

> 义务依据：`docs/n13-relay-increment-plan-20260914.md` §3.4（L45-47，建连前冻结 relay 的 TCP host:port；解析失败/换址/观测不符 → fail-closed）、§4 表 N2-H 行（L62，冻结集合必须含 relay **TCP** 端点并重跑 TUN 负证据 + 端点侧投递；failover/重解析 = 新端点 = **重冻**）、§5（L73，这是「默认路由下启用 WSS」的硬前置）。
> 生效中继事实：`rels://home.alfadb.cn:28443`（`docs/n13-oracle-registration-20260914.md` §3.2；备用 `rels://relay.netcenter.alfadb.cn:443` running 但不广告，若启用 = 新端点 = 重冻）。

### 3.1 构建 CLI（命令照 `docs/n2h-isolation-evidence-notes.md` §2，L37-41）

```bash
cd /home/worker/work/base/netbird-harmonyos/client/core
export RUSTUP_HOME=/home/worker/rust/rustup CARGO_HOME=/home/worker/rust/cargo
export PATH=$CARGO_HOME/bin:$PATH
cargo build --offline --locked --features cli --bin nbinterop
```

### 3.2 冻结（pre-connect freeze check，含 relay TCP）

工具只读 config 里的 `management_url` 一个字段、不读任何密钥（`client/core/src/n2h.rs:1810-1837`）；`--outer-endpoint` 格式 `<kind:host:port>`，kind 取 `management|signal|stun|turn|relay|wg_peer|dns`（`client/core/src/bin/nbinterop.rs:237-258` 用法、`:669-692` `parse_outer_endpoint`）；relay 传输由 kind 推导为 **tcp**（`n2h.rs:140-142`，即 N2-H 修复 commit `820f665` 的落点）。

```bash
printf '{"management_url":"https://api.netcenter.alfadb.cn"}' > /tmp/n2h-deploy.json   # 仅一个非敏感字段
cd /home/worker/work/base/netbird-harmonyos/client/core
target/debug/nbinterop isolation-check \
  --config /tmp/n2h-deploy.json \
  --outer-endpoint relay:home.alfadb.cn:28443 \
  --outer-endpoint signal:signal.netcenter.alfadb.cn:443 \
  --out /home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/n2h-isolation-evidence-preconnect-<HHMM>.json
```

核对证据 JSON：`frozen_endpoints` 含 `kind:"relay"`、`proto:"tcp"`、`host:"home.alfadb.cn"`、`port:28443`、`resolved:` **本次运行新解析的 IP**、`frozen_at` 时间戳；`probes[]` 含 `probe_id:"N2H-relay-<nonce>"`、`expected_path:"outer"`（字段映射见 notes §1 表第1行）。

- **预期 verdict = `n2h-inconclusive`（退出码 20）**：无隧道会话时该模式恒 inconclusive（nbinterop 用法 L247-249；notes §2 ⑤）——这是设计行为，不是失败；`n2h-fail`（退出码 1）才是否定。
- **DDNS 纪律**：host 写**域名**，工具每次运行时重新解析（`resolved` 字段每次取新值）；**不得**把历史解析 IP 硬编码进任何脚本/配置/文档（notes §4 第1条：按冻结时刻解析结果枚举，重解析后的新端点不在证据内；plan §4 L62：换址 = 新端点 = 重冻）。参考解析值 `175.10.223.50` 仅为 2026-09-14 实测，**不得当常量用**。
- stun/dns/wg_peer 等其余类在预连模式缺席时由工具给 `absent_reason`；无理由缺席记缺口（notes §3 L75-77）。

### 3.3 端点排除（设备侧机制 = `RouteInfo.isExcludedRoute`，仓库真实字段名）

- 配置模型字段：`VpnRouteConfig.isExcludedRoute`（`client/entry/src/main/ets/vpnextensionability/NetBirdVpnConfig.ets:22-29`）。
- 基础配置来源：常量 `DEFAULT_VPN_TUNNEL_CONFIG`（`NetBirdVpnConfig.ets:65-98`）——现有唯一 base 排除 = route 3 `192.168.50.0/24`（`:86-93`，`isExcludedRoute:true`）。**仓库 HEAD（`4c68b6e`）内尚无 relay 条目** → relay /32 排除须在**构建前**按同款字段形状加入该常量（`destination:<冻结时刻新解析的 relay IP>`、`prefixLength:32`、`gateway:'10.99.0.1'`、`hasGateway:true`、`isDefaultRoute:false`、`isExcludedRoute:true`），再走 §2.1-§2.4 重建/重签/重装。该代码变更不在本手册内展开（**待确认 ④**；本任务禁止改码，diff 须按当晚决策另行完成并登记）。
- 生效链：`applyNetworkConfig` 始终保留 base 排除项（`NetBirdVpnConfig.ets:146-150`）→ `buildPlatformVpnConfig` 落为平台 `RouteInfo.isExcludedRoute=true`（`:202-223`；`isExcludedRoute` @since 20，`:8-10`）。设备侧核对标记：`VPN_VPNCONFIG_APPLIED|…|routes=…|defaultRoutes=…|excludedRoutes=…`（计数逻辑 `NetBirdVpnExtensionAbility.ets:1048-1062`）。
- **DDNS 后果（明示）**：排除项是编译期字面量，无运行时注入通道（`loadVpnTunnelConfig()` 返回固定常量，`NetBirdVpnConfig.ets:100-104`）。每次执行前用 §3.2 新 `resolved` 核对 HAP 内排除 IP：**不一致 = 换址 = 新端点 = 重冻**，并更新排除项重建（plan §4 L62）。
- 相关机制辨析：`netbird-protect-deviation`（route-exclusion 模式）只是**开发期偏离**、`not_a_gate_pass`（`NetBirdVpnExtensionAbility.ets:273-302`），**不得**被当成 relay 排除的实现或证据。

### 3.4 重跑 TUN 包级负证据与端点侧投递

**(a) 主机侧全管线回归（loopback；relay 以 TcpSink 探测）**——harness 健康度回归，不是设备证据：

```bash
cd /home/worker/work/base/netbird-harmonyos/client/core
target/debug/nbinterop isolation-check \
  --out /home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/n2h-isolation-evidence-loopback-<HHMM>.json
```

期望 `n2h-pass`（exit 0，notes §2 ① L43-45）；loopback 模式 relay sink 已是 `TcpSink`（`n2h.rs:1255-1258`、`:1337`）。

**(b) 反例守护（可选但建议；`--fault-*` 仅限无 `--config` 的 loopback 模式，互斥校验在 `nbinterop.rs` `cmd_isolation_check` 内）**：

```bash
target/debug/nbinterop isolation-check --fault-leak --out /tmp/n2h-fault-leak.json          # 必须 n2h-fail
target/debug/nbinterop isolation-check --fault-omit-endpoint relay --out /tmp/n2h-fault-omit.json   # 必须 n2h-inconclusive
```

（用法依据 nbinterop.rs L250-258；notes §2 ②④。反例产物落 /tmp，不进证据目录。）

**(c) 端点侧投递**：预连模式下 STUN 响应可作端点侧证据（notes §2 ⑤ L59）；**relay TCP 的端点侧投递收执依赖中继主机/运维侧记录**（notes §4 倒数第2条：真实服务端投递日志须运维侧提供）→ 未取得前该腿记为缺口（进 `reasons`），**不得假通**。须提前向运维约定观测窗口。

**(d) 设备侧 TUN 包级负证据**：AUTH allowed 第12条授权「TUN 包级负证据、隧道正控、端点侧投递、计数对账、VPN 撤销后复验」等设备侧测量（工具口径 = `nbinterop isolation-check` 及其等价探针）。设备侧采集的**具体入口/脚本在仓库内未见实现**（**待确认 ③**），按 AUTH 第12条口径与当晚既有测量脚本执行；判定语义固定：探针载荷命中任何 TUN 帧 = `n2h-fail`；正控缺失/对账不平 = `n2h-inconclusive`；对账恒等式 `delta.wg.rx_bytes_to_tun == delta.tun.delivered_bytes`（notes §1 表 #2/#3/#5）。

### 3.5 停止条件（plan §3.6，L53-54，出现任一即停并上报）

默认路由下 WSS 未先排除 relay TCP；无 §3.3 正证据却报 relay 成功；ICE Failed 被当作可达；令牌过期静默假通；destroy/注销后仍有 WSS 出站连接。

---

## 4. 结论口径

- 本手册全部产出的增量归属**只可写 `N13`**；`docs/n13-relay-increment-plan-20260914.md` 头部（L7）明文：**本增量不得被读作 N2-H pass 或 N6 pass**；§7（L103）：主机侧测通 ≠ N6/N2-H pass。
- oracle 登记（`docs/n13-oracle-registration-20260914.md` §5）：结论只可写 `N13 pass`（且在两端点齐备后）；不得写成 N2-H/N6 pass。
- AUTH `forbidden` 第2条：本 AUTH **不授权产生任何 verdict/claim/门级结论**；N2-H 结论须独立审查后方可登记（`docs/n2h-isolation-evidence-notes.md` §0/§6：`n2h-pass` 不是原 N2 判据 pass）。
- 所有产物落 AUTH 证据目录时一律声明 `is_evidence:false` + 「不构成任何门结论」（AUTH `evidence_destination_rules`）。
- 版本矩阵随证据显式标注（plan §7）：控制面 **0.77.0** / 中继 **0.76.3** / dashboard v2.91.1 / 我们调研 clone 0.78.1 / 治理基线 v0.76.3（`docs/n13-oracle-registration-20260914.md` §3.1）。

## 5. 证据留档（按 AUTH 目录与命名）

目录（唯一落点）：`/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002/`

```bash
EVDIR=/home/worker/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260914-0002
# 每个产物（hilog、证据 JSON、执行记录）逐文件生成 .sha256 sidecar：
sha256sum "$EVDIR"/<file> | awk '{print $1"  <file>"}' > "$EVDIR"/<file>.sha256
# 汇总 + 回验：
cd "$EVDIR" && sha256sum <file1> <file2> ... > MANIFEST.sha256 && sha256sum --check MANIFEST.sha256
```

- 声明文件（如 `DECLARATION.md`）写明 `is_evidence:false` 与「不构成任何门结论」（AUTH rules 第2条）。
- **敏感扫描**（词表同 oracle 交付自检，`docs/n13-oracle-registration-20260914.md` §1）：

```bash
grep -rInE 'token|signature|secret|password|bearer|eyJ' "$EVDIR"
```

  已知良性命中**仅允许一处形态**：我方标记词 `token_present=true|false`（`connector.rs:1667`，只输出布尔，不含令牌值）。任何 `eyJ…`（JWT）、`Bearer …`、`token=<值>`、`signature=<值>`、`secret`、`password` 形态命中 → 该文件立即从证据目录移除并记录。**不得**把 setup key / 令牌 / 私钥写进任何日志或文档。
- 会话结束或 `valid_until` 到点：AUTH `consumed` 回填，不自动续期（AUTH rules 第3条）。

## 6. 回滚与收尾

```bash
# 6.1 停应用（AUTH allowed 第6条；含其 vpn extension）
"$HDC" shell "aa force-stop cn.alfadb.netbird"

# 6.2 卸载测试 HAP（AUTH allowed 第5条：仅 cn.alfadb.netbird）
"$HDC" uninstall cn.alfadb.netbird

# 6.3 设备临时文件清理（AUTH allowed 第4条：/data/local/tmp 命名空间内读写与清理）
"$HDC" shell "rm -f /data/local/tmp/netbird-device-config.json /data/local/tmp/netbird-setup-key /data/local/tmp/netbird-protect-deviation"

# 6.4 端口转发清理（AUTH allowed 第9条；(A)/(B) 走生产实例，正常无需 fport；若曾为自建服务端加过必须清）
"$HDC" fport ls
"$HDC" fport rm <index>
"$HDC" rport ls ; "$HDC" rport rm <index>

# 6.5 主机侧（若起过自建 netbird-server / nbinterop peer）：停止并清理其数据目录与凭据文件（AUTH allowed 第10条）
```

- **key 收尾**：立即请运维 **revoke** 一次性 key；官方语义 revoke **不踢**已注册 peer，如需移除测试 peer 须在 dashboard **另行删除**（`docs/ops-setup-key-request-prompt-20260914.md` L19；探测记录 §4）。
- **撤销复验**：若 (B) 设备侧建过 VPN，收尾复核路由/DNS 复原、进程残留、转发清单（AUTH allowed 第13条）。
- **6.6 hdc 不可达**：**停止并上报，不得反复重试**；本轮失败不得原 AUTH 内重试（`retry_allowed:false`，须新签 AUTH）。

## 7. 禁止项（逐条照抄 AUTH `forbidden`，L51-62）

1. 引用或消费任何 campaign/pair/evidence ID（含已 consumed 的历史 ID）
2. 产生 verdict / claim / 任何门级结论（N2-H 结论亦须独立审查后方可登记，本 AUTH 不授权出结论）
3. 写入仓外 live/、run-state\*/、ready-freeze\*/、records/（records/ 仅限登记类文件的既有用法）
4. 修改任何已冻结判据或冻结实现（含 spikes/ 下被冻结资产）
5. 复用已 consumed 的 AUTH
6. root/privileged 操作（含 hdc shell 提权、su、改动系统参数/分区）
7. 设备枚举（list targets 超出连接维持所需）
8. 非本项目 bundle（安装/启动/停止/卸载 cn.alfadb.netbird 以外的任何 bundle）
9. 对设备 /data 下除 /data/local/tmp 外任何路径的写操作
10. 把任何诊断产物写进仓库、标记为 evidence，或省略 .sha256 sidecar

本手册写作任务附加：不 commit；不联网；凭据不进代码/命令行/日志/URL/文档。

## 8. 待确认清单（执行前逐项关闭）

| # | 待确认 | 原因（为何本手册不能确证） |
|---|---|---|
| ① | 当晚生效签名 profile `.p7b` 与 p12/cert 的绑定 | 材料1 §4 用 `<新 profile>.p7b` 占位；仓B `profiles/` 现存 `NetBird HarmonyOS Debug ACL.p7b` 等多个，绑定未登记 |
| ② | protect 生效路径（ACL 使 protect settle vs route-exclusion 偏离通道） | 取决于 profile ACL 实际授予与真机 protect 语义（材料1 §6 列为未验证项）；两分支以 hilog 实测为准 |
| ③ | 设备侧 TUN 包级负证据的具体采集入口/脚本 | AUTH allowed 第12条只授权能力与口径；仓库内未见设备侧采集实现 |
| ④ | relay /32 排除项的代码变更安排 | 仓库 HEAD 常量中无该条目（`NetBirdVpnConfig.ets:86-93` 仅 192.168.50.0/24）；需构建前改码，本任务禁止改码 |
| ⑤ | `device-config.json` 的 `private_key` 生成命令 | 格式已证（`connector.rs:717` base64 std 32B），但生成命令沿用 2026-09-14 生产探测做法、仓内无登记 |
| ⑥ | uitest 点击坐标 `658 1661` 当晚是否有效 | 为 run1 实测值，UI 可能变化；用 `uitest dumpLayout`（只读）复核 |
| ⑦ | pod 侧可达性检查「verify=0」原始命令形态 | 探测记录 §1 表未落命令；本手册 `curl -w '%{ssl_verify_result}'` 为等效重构 |
| ⑧ | start-netbird.sh 设备上现内容 | run1 遗留文件在设备侧，仓内无副本；本手册仅按材料1 §4 原样调用 |
