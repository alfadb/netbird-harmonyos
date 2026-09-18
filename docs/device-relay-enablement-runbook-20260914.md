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

### 1.0 步骤 0：授权窗口时钟比对（**2026-09-15 增补，硬门前置**；条款文本见 `docs/governance-amendment-proposal-20260915.md` §二）

> **增补缘由**：本手册是 2026-09-15 越界执行的依据文档——AUTH `valid_until` 为 `2026-09-15T14:29:26+08:00`，而设备操作实际执行于当日 18:38–18:53（CST），超窗约 4 小时 9 分；执行者读过 AUTH 内容但未比对当前时刻，派发稿亦口头声称「在窗口内」。据此增补本步骤，先于 §1.1 全部检查执行。

在发出**任何设备命令**（含清理、卸载、撤销、force-stop、安装、点击、抓日志）之前：

1. 记录**主机当前时间**（带时区）与**设备当前时间**（带时区）；
2. 与当次 AUTH 的 `valid_from`/`valid_until` 原值三方比对，确认主机时刻与设备时刻**均**在窗口内；
3. 比对结果（含具体时刻、时区、`valid_from`/`valid_until` 原值、窗口内/外结论）**写入运行记录**——无比对记录即视为未比对；
4. 派发稿须写明 `auth_id` 与窗口原文，执行者**先回报比对结果再动手**；未见比对结果，任何设备命令不得发出；
5. 「口头认为还在窗口内」不构成依据；只有 AUTH 文件中的窗口与本次实测比对结果构成依据；
6. **失败即停**：hdc 不可达、设备需解锁、主机与设备时钟偏差超过阈值（提案值 60 秒，待 T0 裁定）→ 立即停止并上报，不得重试或绕过；
7. 窗口外产生的一切产物标注 `out_of_authorization_window: true` / `is_evidence: false` / `not_a_gate_pass: true`，不得作为证据、不得被后续 AUTH 追认；补签 AUTH 只对签发时刻之后生效。

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
7. **relay 判据**（**2026-09-15 判据修正**：原第 7 条把广告行本身写成失败判据，与实现不符，本次修正）：管理面正常广告中继时会出现 `connector: netbird-config relay urls=… token_present=… (uris=…)`（`connector.rs:1659-1673`）——该行是**只读广告探针**，其出现属**正常**，不表示 relay 已启用或有任何拨号（`relay_enabled:false` 为硬默认时，`connector.rs:1674-1697` 的 relay 生命周期**完全不运行**，`relay_status` 亦返回 disabled，`connector.rs:1598-1601`）。判据落在**拨号面**：本轮 `relay_enabled=false`，日志中**不得出现任何 relay/WSS 拨号痕迹**——relay 连接建立/状态迁移、relay Auth/Transport 帧、承载（carrier）帧与字节计数——出现任一即说明配置被误改，按失败处理。须始终区分「**广告**」（管理面下发配置，恒可出现）与「**启用/拨号**」（`relay_enabled=true` 才会发生）。

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

---

## 9. (C) 设备轮：N2-H 设备侧采集器启用与证据回收（**2026-09-17 增补，I7**；本节只追加，不改本文其余正文）

> **适用**：把设备侧证据采集器（设计 `docs/n2h-device-collector-design-20260917.md`，I0–I7 已合入仓库）在真机轮里启起来，并把设备产出的 `n2h-isolation-evidence` 文档完整收回（设备侧产物只能经 hilog 分块通道离设备，设计 §2.4-A）。
>
> **硬前置（本节不重复定义，逐字引用）**：本节**任何**设备命令之前先执行 **§1.0 步骤 0（授权窗口时钟比对）**；§1.0 的全部失败即停条款适用于本节。
>
> **授权口径**：设计 §6.4 第 1–5 条＝本节设备侧命令（arm 控制文件 `file send`、`rm` 清理、hilog 清缓冲与抓取、**日志级别设置单列**、`cat /proc/net/dev`），第 6 条＝主机侧 `nbinterop n2h-peer`；**I7 新增主机侧 `nbinterop n2h-decode`**（设计 §2.4「grep → 按 seq 拼接 → sha256 比对 → 通过才落盘」的落地子命令，设计 §13.11 登记）——**AUTH `allowed` 必须逐字包含它**，否则不得执行。本节全部命令未获授权一律不执行。

### 9.1 制品冻结纪律（先读；本节唯一不可协商的顺序约束）

- **事实**：HAP 打包**不可逐字节复现**——同一工作树重复打包得到过不同 sha256（I6b 实测，设计 §13.10⑦）。「重打包一次拿同一个 sha」**不是**可用手段。
- **纪律**：
  1. 本轮**只打包一次**；打包后立即记录**实际安装的那个 HAP 文件**的 `sha256`（含文件路径、size、时刻与签名产物名），登记进本轮冻结件（执行记录 + `MANIFEST.sha256`，§5）；
  2. **冻结后不得重新打包**：任何源码/资源/配置改动 ⇒ 新制品 ⇒ 必须重新登记、重新签名、重新安装，并且**不得**用新包的 sha256 回填旧一轮证据；
  3. `client/core/build.sh` 末行打印的 `N2H_BUILD_SHA256`（I7 构建自标识）逐轮登记：它＝设备文档 `collector.build_sha256` 在 `build_source="embedded"` 下的取值（§9.6 核对）；**任何源码改动都会改变它**，与证据一起登记即完成「证据 ↔ 构建输入」绑定；
  4. 设备文档 `collector{}` 块（设计 §3.3/A-1）恒带 `crate_version`/`target_triple`/`build_sha256`/`build_source` 四字段；本轮的设备侧 arm 通道**不转发** `build_sha256_reported`（ets 白名单只有 §9.5 的三个键）⇒ 预期 `build_source="embedded"`，**实际安装 HAP 的 sha256 由本节冻结件承担**，不由文档承担（设计 §13.11 登记）。

### 9.2 构建（core + HAP）与登记

（构建命令与 §2.1 同源；本节只增列 I7 的自标识输出。）

```bash
cd /home/worker/work/base/netbird-harmonyos

# a) core：冻结符号链 + I7 构建自标识（末行 N2H_BUILD_SHA256=<64hex>，逐轮登记）
bash client/core/build.sh

# b) HAP：ArkTS 打包；末尾打印 HAP 路径 / size / sha256（**这就是 §9.1 要登记的 sha**）
bash client/build.sh

# c) 登记（冻结件；§5 的 sidecar/MANIFEST 规则一并适用）
sha256sum client/entry/build/default/outputs/default/entry-default-unsigned.hap | tee -a "$EVDIR/HAP-FREEZE.txt"
```

预期输出片段（逐字）：

```text
[netbird-core] 14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)
[netbird-core] build self-identification N2H_BUILD_SHA256=<64hex> (recomputed independently; present in the artifact)
[netbird-hap] VERIFY OK: libs/arm64-v8a/libnetbird_core.so present, sha256 identical to llvm-strip'd core artifact
[netbird-hap] sha256: <HAP sha256>
```

### 9.3 签名 / 安装 / 启动（逐字；签名流程沿 §2.2 原样，本节不重复展开）

```bash
TOOL=/home/worker/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/lib/hap-sign-tool.jar
# 签名：命令同 §2.2（javac + N1bdiscSignLauncher），仅把输入换成 §9.2 的 HAP、输出换成下列名字
#   … client/entry/build/default/outputs/default/entry-default-unsigned.hap /tmp/netbird-n2h-collector-signed.hap netbird-harmonyos 26

sha256sum /tmp/netbird-n2h-collector-signed.hap | tee -a "$EVDIR/HAP-FREEZE.txt"   # 安装件的 sha，一并登记

"$HDC" file send <本机 device-config.json 路径> /data/local/tmp/netbird-device-config.json   # 同 §2.4
"$HDC" file send <本机 setup-key 文件路径>      /data/local/tmp/netbird-setup-key             # 同 §2.4
"$HDC" install -r /tmp/netbird-n2h-collector-signed.hap
"$HDC" shell "sh /data/local/tmp/start-netbird.sh"    # 凭据注入（同 §2.5；脚本为 run1 遗留，不改其内容）
"$HDC" shell "uitest uiInput click 658 1661"          # Connect VPN（同 §2.5；坐标/控件/目的逐条登记）
```

- 采集器**不开产品面**、无 UI：arm 只由控制文件触发（§9.5），窗口是**一次**有界测量（`WINDOW_DEADLINE_MS=60000`，`client/core/src/n2h_collector.rs:1369`）。
- 连接必须达到 `status.wg.device_up && status.wg.ready` 才会 arm（ets 侧闸门 `NetBirdVpnExtensionAbility.ets:975-978`）：未就绪时打 `N2H_COLLECTOR_NOT_ARMED|requestId=…|reason=session-not-ready|diag=true|is_evidence=false`（`:986`）且**不开窗**。

### 9.4 主机侧对端：`nbinterop n2h-peer`（**先起它，再写控制文件**）

设计 §4.3 的正控 a2b/b2a 双腿都需要主机侧对端；它的启动公告行 `b2a_probe_id` 是 §9.5 控制文件里 `peer_b2a_probe_id` 的**唯一来源**。

```bash
cd /home/worker/work/base/netbird-harmonyos/client/core
target/debug/nbinterop n2h-peer \
  --config <主机侧对端 config 路径> \
  --overlay <设备 overlay ip[:port]> \
  --echo-port 40001 \
  --posctl-interval 1000 \
  --timeout 900 \
  2>&1 | tee "$EVDIR/n2h-peer-<HHMM>.log"
```

- **公告行**（stderr 首行，逐字形态）：`nbinterop: n2h-peer|b2a_probe_id=N2H-posctl-<24 hex>|b2a_dst=…|echo_port=40001|posctl_interval_ms=1000|loopback=false`（`client/core/src/bin/nbinterop.rs:1185`；I6 真实样例见设计 §13.9）。
- **状态行**（周期，stderr）：`nbinterop: n2h-peer-stats|…|unknown_peer_drops=<n|na>|unknown_peer_drops_delta=<n|na>`（`:1215`）——对端自身计数器，属**运维侧收执**，不随设备文档（设计 §13.10④）。
- **停止形态**：`--timeout <s>` 到期打印末次状态行并 **exit 0**（设计 §13.8；该模式无「目标可达」语义，超时即登记停止）。
- 读公告行的动作**必须在 §9.5 写入控制文件之前完成**；公告行未取得 ⇒ 本轮 b2a 腿按「对端未公告」记为有理由缺口（不得猜、不得用 a2b 回包顶替）。

### 9.5 控制文件（arm 通道）写入与清理

**写入（arm 之前）**：

```bash
cat > /tmp/netbird-n2h-arm <<'EOF'
{"posctl_port":40001,"probe_wait_ms":800,"peer_b2a_probe_id":"N2H-posctl-<24 hex，逐字取自 §9.4 公告行>"}
EOF
"$HDC" file send /tmp/netbird-n2h-arm /data/local/tmp/netbird-n2h-arm
```

- 路径固定 `/data/local/tmp/netbird-n2h-arm`（ets `N2H_ARM_FILE`，`NetBirdVpnExtensionAbility.ets:75`），只含非敏感窗口旋钮（开发期通道，先例 §2.4 的 `netbird-protect-deviation`）。
- 键语义：`posctl_port` 缺省 40001（`n2h_collector.rs:1304`）、`probe_wait_ms` 缺省 800（`:651`）；`peer_b2a_probe_id` 为**可选键**，**只在键存在时**原样并入 arm opts（`:1047-1050`），值必须是 `N2H-posctl-<24 hex>` 逐字 id 形态——带 `|positive-control` 载荷形态、非 24 hex、空串、尾随字节等一律**拒绝整个 arm**（`{ok:false,error:"invalid-opts"}`，设计 §13.10②）；键缺失＝不公告（b2a 记有理由缺口），两者语义不同。
- **时序**：**arm 之前**写入。ets watcher 每轮轮询检查文件存在，且每个 ability 实例**至多尝试一次**（`n2hArmAttempted`，`:305`/`:967-970`）；文件在尝试之后才出现 ⇒ 该轮按缺失处理、**不会再开窗**（设计 §13.10①）。
- 文件体解析失败/为空 ⇒ 回落 §5.1 缺省并打 `N2H_COLLECTOR_ARM_OPTS_IGNORED|…|action=defaults|diag=true|is_evidence=false`（`:1002`）。

**清理（收尾，同 §6.3 命名空间；AUTH allowed 第 4 条）**：

```bash
"$HDC" shell "rm -f /data/local/tmp/netbird-n2h-arm"
```

### 9.6 证据抓取与重组（顺序固定：日志级别 → 清缓冲 → arm → 抓取 → 重组）

```bash
# (a) 日志级别设置——**单独一条，逐字**（N13 偏差教训：该动作不在 AUTH allowed 字面内即不得执行）
#     限本项目域 0x2900、非持久、重启失效（N13 实录 docs/n13-device-relay-evidence-20260916.md:97）
"$HDC" shell "hilog -D 0x2900 -b D"

# (b) 清缓冲（窗口前的空缓冲起点；设计 §6.4 第 3 条）
"$HDC" shell "hilog -r"

# >>> 此处写 §9.5 的 arm 文件并等待窗口闭合（verdict 行见 §9.7）<<<

# (c) 抓取（窗口闭合后**立刻**抓，避免缓冲旋转；pattern 必须含 N2H_EVIDENCE/N2H_COLLECTOR/N2H_ 与既有 NetBirdVpn/N1BDiscVpn）
"$HDC" shell "hilog -x | grep -aE 'N2H_EVIDENCE|N2H_COLLECTOR|N2H_|NetBirdVpn|N1BDiscVpn'" \
  | tee "$EVDIR/hilog-n2h-collector-<HHMM>.log"

# (d) 重组（I7；缺块或摘要不符 ⇒ exit 21 + 缺口明细，**不落未验证文档**）
cd /home/worker/work/base/netbird-harmonyos/client/core
target/debug/nbinterop n2h-decode "$EVDIR/hilog-n2h-collector-<HHMM>.log" \
  --out "$EVDIR/n2h-isolation-evidence-device-<HHMM>.json"
```

- 通道形态：每行 `N2H_EVIDENCE|seq=i/n|sha256=<全文档>|b64=<512B 原始切片>`（`n2h_collector.rs:86`、`:90`），文档 8–20 KB ⇒ 约 20–60 行；hilog 前缀与夹杂行由 `n2h-decode` 自动跳过（设计 §2.4）。
- **成功判据（逐字）**：stderr 打 `nbinterop: n2h-decode|ok=true|…|sha256=<全文档>|out=<路径>`、**exit 0**、产物 JSON 落指定路径；输出的 `sha256` 必须与 (c) 抓取行里 `sha256=` 字段**逐字相同**。
- **失败处置（fail-closed）**：`ok=false|error=chunk-error|…|missing=<seq 列表>|detail=…`、**exit 21**，且**不写任何文档**——先在缓冲仍持有窗口时**重抓一次** (c)+(d)；仍缺即按设计 §2.4-R2（重放对应 chunk 需经 ets 再调 napi）或**如实登记缺口并停止**，不得修补、不得拼接跨轮产物（跨轮拼接会以 `sha256` 不符被拒，这属预期）。
- 另按设计 §2.4-D 类 fallback 可选抓 `"$HDC" shell "cat /proc/net/dev"`（vpn-tun 计数，C7/C9；App 内直读失败时为备用口径）。

### 9.7 状态检查（经 shell/日志读）与停止条件

- `n2h_collector_status()` 是 napi 导出（`n2h_collector.rs:2062`），shell 域不可直接调用 ⇒ 设备轮的状态读数口径＝**hilog 标记**：

```bash
"$HDC" shell "hilog -x | grep -aE 'N2H_COLLECTOR'"
```

| 标记（出处） | 形态 | 读法 |
|---|---|---|
| `N2H_COLLECTOR_ARM`（`n2h_collector.rs:1383`/`:2052`） | `…\|requestId=core\|opened_mono=…\|deadline_mono=…\|frozen=…\|shell_exclusions=…\|diag=true\|is_evidence=false` | 窗口已开；`deadline_mono` 给出上界 |
| `N2H_COLLECTOR_ARM\|refused`（`:2161`） | `…\|refused\|error=<no-connector\|device-not-up\|tunnel-not-ready\|already-armed\|invalid-opts\|window-failed>\|detail=…` | arm 被拒（fail-closed）；**不重试绕过**，记因上报 |
| `N2H_COLLECTOR_CLOSE`（`:1384`/`:2335`） | `…\|phase=closed\|verdict=<n2h-pass\|n2h-fail\|n2h-inconclusive>\|frames_scanned=…\|hits=…\|probes_sent=…\|receipts_observed=…\|chunks=…\|sha256=<全文档>\|diag=true\|is_evidence=false` | 窗口闭合；`chunks`/`sha256` 与 §9.6(d) 核对 |
| `N2H_COLLECTOR_DISARM`（`:1385`/`:2138`/`:2152`） | `…\|discarded=true\|…`（未闭合窗）或 `…\|discarded=false\|retained=true\|…`（已闭合窗，产物留待重放） | 中止语义（设计 §13.5） |
| ets 侧 `N2H_COLLECTOR_ARMED` / `N2H_COLLECTOR_ARM_FAILED` / `N2H_COLLECTOR_NOT_ARMED` / `N2H_COLLECTOR_ARM_OPTS_IGNORED`（`NetBirdVpnExtensionAbility.ets:1058`/`:1065`/`:986`/`:1002`） | 壳侧 arm 钩子的读数（含 `peerB2aProbeId=<id\|none>`） | 判定「文件是否被看到、是否就绪、是否被拒」 |

- **判定口径**：`verdict` 只由证据 JSON 内判据解释；**运维收执缺位时设备轮预期恒为 `n2h-inconclusive`（收执缺口，设计 §4.0 诚实结论）——这是设计行为，不是工具故障**；`n2h-fail` 才是否定（证伪优先）。
- **何时停止（引用既有条款，不新增）**：
  1. §1.0 步骤 0 任一失败（时钟超阈/不可达/需解锁）⇒ **立即停止并上报**；
  2. §3.5 既有停止条件出现任一；
  3. 采集器 fail-closed：未就绪不开窗（§9.3）；arm 被拒（上表 `refused`）⇒ 记录后停止，不得改文件重试到平；
  4. 抓取缺块且重抓失败 ⇒ 按 S13 仪器异常**先停再上报**（设计 §6.3、§2.4-R2）；
  5. `frames_scanned=0`（tee 零帧）或 `/proc` 双路径皆不可读 ⇒ 该腿记缺口、verdict `n2h-inconclusive`，**不得**当清洁扫描或假通（`n2h_collector.rs:2816-2824`）；
  6. AUTH `retry_allowed:false` 与 §6.6：本轮失败不得在原 AUTH 内重试。

### 9.8 收尾与留档

```bash
# 清理控制文件（同上 §9.5）；停应用与卸载沿 §6.1/§6.2 原样
"$HDC" shell "rm -f /data/local/tmp/netbird-n2h-arm"
"$HDC" shell "aa force-stop cn.alfadb.netbird"

# 逐文件 sidecar + 汇总回验（§5 规则；本节新增产物：hilog 抓取、设备证据 JSON、n2h-peer 日志、HAP-FREEZE.txt）
cd "$EVDIR" && sha256sum hilog-n2h-collector-*.log n2h-isolation-evidence-device-*.json n2h-peer-*.log HAP-FREEZE.txt > MANIFEST.sha256 \
  && sha256sum --check MANIFEST.sha256
```

- 全部产物仍声明 `is_evidence:false` 且**不构成任何门结论**（AUTH `evidence_destination_rules`）；设备证据文档含外层端点 IP/域名（设计 §2.4-A 的有意例外），只落 AUTH 证据目录并按 §5 敏感扫描。
- 设备文档的 `collector.build_sha256` 与本轮 §9.1/§9.2 登记的 `N2H_BUILD_SHA256` 必须逐字相同；安装 HAP 的 sha256 以 §9.1 冻结件为准（HAP 不可复现，见设计 §13.10⑦）。

### 9.9 本节新增待确认（执行前逐项关闭；§8 表继续有效）

| # | 待确认 | 原因（为何本手册不能确证） |
|---|---|---|
| ⑨ | arm 文件在 watcher 轮询间隔内的可见性（`fs.accessSync` 对 `/data/local/tmp` 的读权限已证，写侧未单独复验） | 设备侧 arm 钩子为 I5 落地实现，仅离线 harness 验证；真机 arm 需本轮实测（失败形态 `N2H_COLLECTOR_NOT_ARMED`/`ARM_OPTS_IGNORED` 可分辨） |
| ⑩ | hilog 缓冲在窗口期间是否足以容纳 20–60 行且不旋转 | 依赖设备 hilog 缓冲配置与当轮日志量；失败形态＝`n2h-decode` exit 21 + `missing=<seq>` |
| ⑪ | 设备侧 `/proc/net/dev` 的 App 内直读权限（设计 §9-U1） | 未在真机验证；不可读时该腿退化为 hdc fallback 或在 `notes[]` 记 `counters-source=` 缺口 |
| ⑫ | `hilog -D 0x2900 -b D` 在本轮设备 hilog 版本的参数兼容性 | 沿用 N13 实测形态（`docs/n13-device-relay-evidence-20260916.md:97`）；参数名以设备版本为准，仍须在 AUTH allowed 字面内 |

---

## 待确认① 关闭登记（2026-09-18，纯追加）

> **本节为纯追加**：§1.1（L93、L125）与 §8 表第 ① 行（L356）原文一字未改、一行未删；本节只登记 2026-09-18 本轮人类确认结果，构成待确认①的关闭登记。

**人类选定**（2026-09-18，本会话 DSH Web GUI，`ask_user_question` 应答）：

| 项 | 值 | 出处 / 说明 |
|---|---|---|
| 当轮生效签名 profile | `profiles/NetBird HarmonyOS Debug ACL.p7b` | 2026-09-18 人类选定（本会话 DSH Web GUI） |
| verbatim 应答 | `候选 1（ACL profile）（推荐）` | 同上；逐字誊录，未改写 |
| sha256 | `4be623b12a203d9560a40a4a69f68586471d5355d3318990f7f9feb2ed7425c5` | 2026-09-18 `sha256sum` 实测（只读） |
| 大小 / mtime | 4193 B / `2026-09-14 07:20` | 同上（`stat` 实测） |

**选定依据**：`AUTH-DIAG-DEVICE-VALIDATION-20260916-0002` 的 `EXECUTION-RECORD-20260915-1900.md:21` 与 `AUTH-DIAG-DEVICE-VALIDATION-20260915-0003` 的 `EXECUTION-RECORD-20260915T2012-partial.md:15` 均**逐字点名**该 profile；后续轮次的记录沿用「ACL profile」这一称法指代同一文件。

**未选中候选与理由**：

| 候选 | 标识 | 未选中理由 |
|---|---|---|
| `NetBird HarmonyOS Debug.p7b` | `820508fb…433c0` | 其载荷与选定件**仅 `uuid` 不同**（`a8ec3a35-…f16d` vs `2ad7bb56-…7126`），且两者 `acls.allowed-acls` 均为 `[]` ⇒ 属等价候选，但非本轮人类选定 |
| `NetBird N1BDISC Debug.p7b` | `49c99c56…19a4` | 其 bundle 名 `cn.alfadb.netbird.n1bdisc` 与本轮冻结 HAP 的 `cn.alfadb.netbird` 不符 ⇒ **事实性排除**（非偏好性取舍） |

**本纪律不变**：该绑定**仍须每轮由人类确认**——本节只登记本轮（2026-09-18）确认结果，**不构成对后续轮次的预授权**；下一轮仍按 §1.1 L93/L125 与该轮人类确认重新取值并逐条登记。

**承载**：本轮选定结果写入 `AUTH-DIAG-DEVICE-VALIDATION-20260918-0011` 的 allowed 签名条（`<p12> <p12.pass> <cer> <profile.p7b>` 逐字四值，`<profile.p7b>` 位取本节选定件）。

**附注（如实登记，不推断）**：

- `docs/n1b-disc-handoff-20260902.md:86` 注② 记该 cer 文件实际内容为华为 CBG Root CA G2 根证书（文件名有误导）；本节照录该既有登记，未另作推断。
- `acls.allowed-acls` 为空 ⇒ 该 profile 在**载荷层面**不含 `MANAGE_VPN` 等 ACL 条目，与「`MANAGE_VPN` 不可授权」的既有结论一致（`docs/native-nx-governance.md:122`；`docs/n2h-isolation-evidence-notes.md:10`）。
