# 真机验证 run 1 — 交接与恢复指引（2026-09-14 01:xx CST 收工）

> 状态：**未完成，明天从"AGC profile 授予 ACL"继续**。
> AUTH：`AUTH-DIAG-DEVICE-VALIDATION-20260913-0001`（有效期至 2026-09-14T21:45+08:00，单会话/24h 取小）。
> 证据目录：`~/harmonyos-signing/netbird-n1bdisc/diagnostics/AUTH-DIAG-DEVICE-VALIDATION-20260913-0001/`。

## 1. 真机已经证实的事实（run 1，2026-09-13 23:1x–00:5x）

| # | 里程碑 | 真机证据（hilog） |
|---|---|---|
| ① | management login（NaCl 信封加密） | `connector: login ok (peer address assigned)` |
| ② | Sync 网络图 | `connector: network map applied serial=3 peers=1 routes=0` |
| ③ | signal registered | `connector: signal stream registered (ice initiation armed)` |
| ④ | 平台 TUN fd 合同 | `N1BDISC_TUN_OPEN\|raw=42\|dup=44\|via=dupfd_cloexec\|nonblock_ofd=true` |
| ⑤ | WG 设备建成 | `N6_WG_DEVICE\|adopt` → `N7_WG_FEED\|device-up\|data-plane-started` |
| ⑥ | 系统确认 VPN 建立 | `Model.VpnController --> VPN_CONNECTION_STATUS_CHANGED` |

原始日志：同目录 `device-run-1.hilog.log`（10,349 B）。

## 2. 当前卡点（明天第一件事）

**`VpnConnection.protect()` 需要 `ohos.permission.MANAGE_VPN`（受限 ACL）**：

```
E netmanager/NetMgrCommon: permission check failed,
    permission:ohos.permission.MANAGE_VPN, callerToken:537437623
```

后果：protect 的 promise **永不 settle**（实测 5 个 fd 全部 `VPN_PROTECT_TIMEOUT|...|stillPending=true`），
我们的 fail-closed 5s 盒子因此全部判失败；数据面（ICE 候选/WG 握手）走不下去。

**根因不是我们的代码**：AGC profile 的 `acls.allowed-acls` 为空。已做：`module.json5` 声明该权限
（HAP 构建产物已确认包含）。

**明天要做**：
1. VNC 浏览器登录 AGC（账号 诸葛铁蛋 / 丁祖巍）→ 证书、APP ID和Profile → Profile。
2. **新建**一个调试 Profile（保留现有 `NetBird HarmonyOS Debug`，不删除）：
   应用 `Netbird`(cn.alfadb.netbird) / 证书 `NetBird E3 Debug` / 设备 `PHYS-1` /
   **勾选「申请权限：受控ACL权限」并包含 `ohos.permission.MANAGE_VPN`**。
   注意：该 ACL 可能需要 AGC 侧申请审批，界面能否直接勾选待验证。
3. 下载 `.p7b` 到 `~/harmonyos-signing/netbird-n1bdisc/profiles/`。
4. 用 helper 签名（见 §4）→ `hdc install -r` → 重跑 §5 的启动流程。
5. 期望新增证据：`VPN_PROTECT_SETTLED|...|elapsedMs=<小值>`、
   `VPN_ICE_FEED|fed`、ICE selected pair、`wg.peers_with_session>=1`、双向探针。

## 3. 环境与材料（均为持久目录，pod 重启后仍在）

- 服务端：`~/netbird-interop/bin/netbird-server`（0.78.1），配置 `server/config-device.yaml`
  （`exposedAddress: http://192.168.50.63:18080`，dataDir `server/data-device2`，全新且已 bootstrap）。
- 客户端材料：`~/netbird-interop/client-devval/`（600）：`device-config.json`、`device-setup-key`、
  `peer-config.json`、PAT `../server/admin.pat.device2`。
- 运维已配 LB 端口映射（**明天用完请让运维撤掉**）：`TCP 18080`、`UDP 51820` → pod。
  设备侧验证过 LB 直连可用：`http://192.168.50.63:18080/api/instance` → 200（pod 内 hairpin 也通）。
- 设备：`hdc tconn 192.168.50.199:37193`；hdc 二进制
  `~/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/hdc`。
- 设备侧已安装：签名 HAP v5（`.../netbird-client-device-validation-signed-v5.hap`）。
- 设备侧文件：`/data/local/tmp/{netbird-device-config.json,netbird-setup-key,start-netbird.sh}`。

## 4. 明天重跑的最短路径

```bash
# (0) hdc 重连（pod 重启后必须）
HDC=~/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/hdc
$HDC tconn 192.168.50.199:37193

# (1) 起服务端（后台）——注意 dataDir 必须与 PAT 匹配：若 PAT 失效（重启会失效），
#     换新 dataDir 重新 bootstrap（见 docs/interop-run-1-20260913.md 的无 IdP 自举流程）
cd ~/netbird-interop && NB_SETUP_PAT_ENABLED=true NB_DISABLE_GEOLOCATION=true \
  ./bin/netbird-server --config server/config-device.yaml

# (2) 起主机 peer（固定 ICE 端口 + 对外候选）
cd ~/work/base/netbird-harmonyos/client/core && \
  ./target/debug/nbinterop peer --config ~/netbird-interop/client-devval/peer-config.json \
  --ice-port 51820 --advertise-candidate 192.168.50.63:51820 --timeout 7200 --interval 3000

# (3) 签名（新 profile 到位后）
TOOL=~/harmonyos/command-line-tools/26.0.0.821/sdk/default/openharmony/toolchains/lib/hap-sign-tool.jar
javac -cp "$TOOL" -d /tmp/signclasses \
  ~/harmonyos-signing/netbird-n1bdisc/candidate-signing/20260911-app-native-6014363/N1bdiscSignLauncher.java
java -cp "$TOOL:/tmp/signclasses" N1bdiscSignLauncher \
  ~/harmonyos-signing/netbird-e3/private/netbird-harmonyos.p12 \
  ~/harmonyos-signing/netbird-e3/private/netbird-harmonyos.p12.pass \
  ~/harmonyos-signing/netbird-e3/cert/"NetBird E3 Debug.cer" \
  ~/harmonyos-signing/netbird-n1bdisc/profiles/"<新 profile>.p7b" \
  <unsigned.hap> <out.hap> netbird-harmonyos 26

# (4) 重推设备配置（若 setup key 变了）+ 安装 + 启动 + 点击
$HDC file send ~/netbird-interop/client-devval/device-config.json /data/local/tmp/netbird-device-config.json
$HDC file send ~/netbird-interop/client-devval/device-setup-key   /data/local/tmp/netbird-setup-key
$HDC install -r <signed.hap>
$HDC shell "sh /data/local/tmp/start-netbird.sh"     # want 参数注入凭据（沙箱读不到 /data/local/tmp）
$HDC shell "uitest uiInput click 658 1661"           # Connect VPN（App 必须在前台）
$HDC shell "hilog -x | grep -aE 'NetBirdVpn|N1BDiscVpn|N1BDISC_|N6_|N7_' | tail -40"
```

## 5. 本轮已修复的代码（已提交推送 `d9346c3`）

1. **CIDR 地址**（真机专属 bug）：`ip/prefix` 整串塞进 `LinkAddress.address` → 平台
   `invalid ip address/ParseAddress failed` → 401 拒绝整个 VpnConfig；前缀还硬编码 32。
   现剥离后缀并用其前缀；IPv6 不再交给壳侧；有回归测试。
2. **沙箱凭据**：App 读不到 `/data/local/tmp` → 改为 `aa start --ps` want 参数注入，
   由 `EntryAbility` 落到自己的 filesDir（开发期做法）。
3. **ICE socket 供给**：新增 `ice_socket_open`（未绑定 UDP）+ 壳侧 protect+feed。
4. `module.json5` 声明 `ohos.permission.MANAGE_VPN`（ACL，待 profile 授予）。

测试：`cargo test --offline --locked` = **315 passed / 0 failed**。

## 6. 明天别忘了

- 用完请运维**撤掉 LB 端口映射**（TCP 18080 / UDP 51820）。
- AUTH 到期需新签（`docs/evidence-schema.md` 例外二流程）。
- 未验证项清单：protect 真实语义（ACL 生效后复验）、ICE/WG 数据面、路由/DNS 落地、
  N8 重建路径、真机 hilog 里 `VPN_ROUTE_SET_ACK_SKIPPED|reason=no-core-snapshot-routes`
  的含义（网络图 routes=0 时属预期，但需确认）。
