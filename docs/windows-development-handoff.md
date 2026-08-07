# Windows + DevEco Studio 开发交接

最后核验：2026-08-07

本文只交接 `E3-PHYS-PREFLIGHT` 的 Windows 本地签名与构建准备；它不授权物理设备 campaign，也不改变 `R0`、`E8` 或预检计划状态。完整物理预检边界、场景与证据要求见 [E3-PHYS-PREFLIGHT 物理设备预检计划](e3-physical-preflight.md)。

## 交接基线

- Windows 完成回传所绑定的仓库准备基线为 commit `f44be17331e5bc67a5eff702badba41cbd7a195f`。最终 live 提交 SHA 不在本文预写，须由专用 runner 在仓外 freeze manifest 中绑定当时的 clean HEAD。
- 当前 `R0` 尚未退出；`E8` 必须保持 `CLOSED`。交接完成时 `E3-PHYS-PREFLIGHT` 曾为 `plan_status: ready`；后续 API23 initial live 已登记为 `EV-E3-PHYS1API23-20260806-0001`（`reviewed-pass/blocked`）。rebind `EV-E3-PHYS1REBIND7-20260806-0001` 已完成；`ADJ-20260806-0003` 冻结 HarmonyOS 7 新元组并曾准备 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001`（从未 Live、未占用；`ADJ-20260807-0001` 标 `superseded-unexecuted`）；host reverify 已 PASS（HAP/signature/profile/member hashes 未变；不主张安装兼容性）；`ADJ-20260806-0004` / `EV-E3-PHYS1BUILD7-20260806-0001` 单条 build 确认 PASS（仅 build-confirm；API `26` 仍 rebind 实测）；API26 live `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 已 `consumed-blocked`（operator-aborted；禁局部 scenario5 重放；保留）；`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 已 Live 并 `consumed-blocked`（`reviewed-pass/blocked`；完整 1–7；双审查 0 B/5 M），当前 `plan_status: blocked-awaiting-adjudication`（无 auto retry/新 ID/设备命令授权；本登记禁 HDC）。
- 稳定设备别名仍为 `PHYS-1`。历史 API23 冻结记录保留不改写。`ADJ-20260806-0003` 新冻结非敏感元组：`PLA-AL10`、distribution `HarmonyOS`、HDC binding build `PLA-AL10 7.0.0.100(SP8C00E32R7P2)`（`EV-E3-PHYS1BUILD7-20260806-0001` 实测确认逐字匹配）、Settings 人工观测 `7.0.0.100 (SP8C00E32R7P2patch09)`、API `26`、kernel arch `aarch64`、app ABI `arm64-v8a`（API/arch/ABI 由 rebind 实测，不从 build 推断）。
- 真实无线 HDC endpoint、target、IP:port、UDID、序列号和其他可识别设备值始终在仓库外受控保存；不得写入仓库、聊天、issue、日志、截图或普通证据。

## 已完成的本地准备

隔离项目是 `spikes/e3-vpn-extension-physical-preflight-hap/`。它已完成 API 23 受限适配，含两个独立产品和 bundle：

| 产品 | bundle |
| --- | --- |
| `default`（A） | `cn.alfadb.netbird.e3physvpna` |
| `vpnB`（B） | `cn.alfadb.netbird.e3physvpnb` |

两者均以 `compileSdkVersion: 6.1.1(24)` 编译，`targetSdkVersion` 与 `compatibleSdkVersion` 均为 `6.1.0(23)`。唯一 native payload 是 `arm64-v8a` 的纯 C `libfdprobe.so`，仅用 `fcntl(F_GETFD)` 对 fd 做只读状态检查；平台 `VpnConnection.destroy()` 是唯一关闭责任。

受限能力审计已通过：signed A/B 均仅有 `ohos.permission.INTERNET`、非导出的 `type: vpn` Extension、API `23`、debug 普通开发签名，唯一 arm64 native 成员为 `libfdprobe.so`；没有 Go、NetBird、WireGuard、`protect`、特权能力或外部 endpoint。历史 `spikes/e3-vpn-extension-hap/` 和历史 raw evidence 不得修改。旧 unsigned hash 只绑定历史本地准备阶段，不是当前 campaign 输入。

## Windows 前置条件

- Windows 授权人员须拥有华为开发者账号和对应 AGC 项目的普通开发签名权限，足以管理两个 App ID、Debug 证书、设备与 profile；权限不足时停止并由账号所有者处理。
- 安装 Git，以及 **Windows 64 位正式版** `DevEco Studio 6.1.1 Release`（本次冻结输入；禁止 Beta / Preview / Canary，也禁止旧版 `6.0.x` / `6.1.0`）。官方下载：<https://developer.huawei.com/consumer/cn/download/deveco-studio>。
- 安装后在 `Help > About DevEco Studio` 记录完整 IDE 版本串 `6.1.1.xxx`（含 Build）；在 `Help > About HarmonyOS SDK` 或 SDK Manager 确认已安装 HarmonyOS SDK `6.1.1` / API `24`，用于 compile。项目 CLI 中的 `6.1.1.290` 是 Command Line Tools 包版本，**不要求** IDE Build 尾号等于 `.290`。项目 `targetSdkVersion` 与 `compatibleSdkVersion` 必须继续为 API `23`。
- 安装可用的 HDC，并记录精确 HDC 版本。不要把 HDC target 或设备标识写入版本控制、构建日志或回传。
- `PHYS-1` 使用同一 LAN 的无线调试。仅从设备 UI 人工读取当前动态 IP:port，并且只在本机执行以下最短连接流程；不运行 shell discovery 或主机侧设备扫描，不记录、截图或回传 endpoint：

  ```text
  hdc tconn <dynamic-ip:port>
  hdc list targets
  ```

  在设备 UI 和 DevEco Studio 的可见提示中完成首次信任；随后目视设备 UI 的型号与完整 build，确认仍为 `PHYS-1` 的已冻结元组。IP:port 变化时仅重复此本机连接流程。
- 签名材料、私钥、密码和设备标识不得放入仓库，也不得放入仓库内脚本、环境文件或 signing config。

## 拉取与基线校验

在 PowerShell 中使用公开仓库 URL；不要把签名材料放在 clone 目录中。

```powershell
git clone https://github.com/alfadb/netbird-harmonyos.git netbird-harmonyos
Set-Location netbird-harmonyos
git switch main
git pull --ff-only

git fetch origin
git merge-base --is-ancestor f44be17331e5bc67a5eff702badba41cbd7a195f HEAD
if ($LASTEXITCODE -ne 0) { throw 'Windows 完成回传基线不是当前 HEAD 的祖先；停止并联系主会话。' }
git rev-parse HEAD
git status --short --branch
```

`git status` 必须在签名、构建前后检查。不要通过提交 `.p12`、`.p7b`、`.cer`、`.csr`、material、密码或签名配置来“保存进度”。

## 签名 Enrollment 的已履行边界

设备元组 discovery 已完成，绝不重跑。该 discovery 之外，唯一获批的设备 `shell` 操作只服务于 profile enrollment，不是 campaign 或 evidence。授权用户已在边界内执行一次，例外已经消耗；UDID 未留存、未回传。下列命令只作为审计记录，禁止重跑：

```text
hdc shell bm get --udid
```

此命令的长选项 `--udid` 是唯一获批过的 UDID 读取；短选项 `bm get -u` 仍禁止。stdout 已仅人工用于 AGC，未粘贴、重定向、留存或回传。已消耗的例外不授权任何其他 `shell`，也不授权 `install`、`send`、`start`、`stop`、运行 VPN 或 campaign。

## 已履行的普通开发签名流程

以下流程已按手工、可审计的普通开发签名路线履行，保留作审计与未来漂移后的重建模板；它不是再次 enrollment 或设备执行授权。

1. 在 DevEco Studio 顶部菜单选择 `Build > Generate Key and CSR`（若当前版本文字略有差异，搜索同名动作），生成仓外受控的 `.p12` 与 `.csr`；密码和 alias 只保存在本机安全位置，不得进入仓库。
2. 在 AGC 创建两个 App ID，精确对应 `cn.alfadb.netbird.e3physvpna` 和 `cn.alfadb.netbird.e3physvpnb`。
3. 上传该 `.csr`，申请并手工下载 Debug `.cer` 到仓外 `cert/`（Windows 路径为 `cert\`）。
4. 按上节唯一 enrollment 边界将 `PHYS-1` 注册到 AGC；不在任何仓内或普通输出中保留 UDID。
5. 为两个 App ID 分别创建 Debug `.p7b`，每个 profile 都必须包含该设备和对应 App ID。可以共用同一证书，但必须有两个独立 profile。

不要用 system/debug/enterprise/root 权限、`MANAGE_VPN` 或其他越权能力替代普通开发签名。

## 签名材料的安全存储

建议在仓库外使用受控目录，例如：

```text
D:\HarmonySigning\netbird-e3\
  private\
  cert\
  csr\
  profiles\
    a\
    b\
  verify-temp\
```

`.p12` 放入 `private\`，Debug `.cer` 放入 `cert\`（`cert/`），`.csr` 放入 `csr\`（`csr/`）；A、B 的 profile 分别放入 `profiles\a\` 与 `profiles\b\`。`verify-temp\`（`verify-temp/`）只存放本地人工判读的验签临时产物。根 `.gitignore` 不保证忽略 `.p12`、`.p7b`、`.cer` 或 `.csr`，因此这些文件和任何 material 都不得放入 clone 目录。

密码只使用 DevEco Studio 的安全机制或 Windows Credential Manager，绝不写进脚本、命令行、聊天、issue 或日志。profile 验证输出可能含 UDID；可以在受控目录内人工判读，但不得归档、复制或回传原始输出。

## DevEco Studio 配置与构建

1. 在 DevEco Studio 打开 `spikes/e3-vpn-extension-physical-preflight-hap/`，不要打开并改写历史 spike。
2. 建立两套独立 signing config：A 仅用于 product `default`，B 仅用于 product `vpnB`；分别在 product 级绑定 `default -> A`、`vpnB -> B`。
3. A/B 可以共用同一 `.p12` 与 Debug `.cer`，但 profile 必须不同，且分别匹配 A/B 的 App ID。不得把一个 profile 或 App ID 混用于另一 bundle。
4. `buildMode` 固定为 `debug`。`signAlg` 只使用证书与 DevEco Studio 生成的配置；不得凭空填写或改写算法。
5. 保持 `compileSdkVersion: 6.1.1(24)`、`targetSdkVersion: 6.1.0(23)` 和 `compatibleSdkVersion: 6.1.0(23)` 不变。签名前先分别 build A/B；完成 profile 绑定后，再分别重新 build A/B。按 DevEco Studio 实际显示记录输出路径的稳定别名，不硬编码未知的 Windows 输出路径。
6. 本地 `build-profile.json5`、signing config 与 material 可能变为 dirty；它们只用于本机工作区，禁止 `git add` 或 `git commit`。

这一步只生成并验证签名 HAP，不授权安装、启动、运行或停止任何 VPN，也不得修改 `spikes/e3-vpn-extension-hap/` 或历史 raw evidence。

## 签名验证与最小回传

只在仓外受控目录运行 `hap-sign-tool.jar` 的 `verify-profile` 与 `verify-app`。以下是 PowerShell 占位命令骨架；所有路径必须使用实际本机值，但不得硬编码真实路径或秘密到仓库。`$VerifyTemp` 必须指向仓外的 `verify-temp`：

```powershell
$SignTool = '<DevEco-SDK>\default\openharmony\toolchains\lib\hap-sign-tool.jar'
$VerifyTemp = '<out-of-repository>\verify-temp'
$AHap = '<signed-a-hap>'
$BHap = '<signed-b-hap>'
$AProfile = '<a-debug-profile-p7b>'
$BProfile = '<b-debug-profile-p7b>'

java -jar $SignTool verify-profile -inFile $AProfile -outFile "$VerifyTemp\a-profile.json" *> "$VerifyTemp\a-verify-profile.log"
if ($LASTEXITCODE -ne 0) { throw 'A verify-profile failed; inspect the local verify output and stop.' }
java -jar $SignTool verify-app -inFile $AHap -outCertChain "$VerifyTemp\a-cert-chain.cer" -outProfile "$VerifyTemp\a-profile.p7b" *> "$VerifyTemp\a-verify-app.log"
if ($LASTEXITCODE -ne 0) { throw 'A verify-app failed; inspect the local verify output and stop.' }
java -jar $SignTool verify-profile -inFile $BProfile -outFile "$VerifyTemp\b-profile.json" *> "$VerifyTemp\b-verify-profile.log"
if ($LASTEXITCODE -ne 0) { throw 'B verify-profile failed; inspect the local verify output and stop.' }
java -jar $SignTool verify-app -inFile $BHap -outCertChain "$VerifyTemp\b-cert-chain.cer" -outProfile "$VerifyTemp\b-profile.p7b" *> "$VerifyTemp\b-verify-app.log"
if ($LASTEXITCODE -ne 0) { throw 'B verify-app failed; inspect the local verify output and stop.' }
```

四项验证只有在各自命令 exit code 为 `0` 且人工内容核对通过时才算 pass；不能只搜索日志中的成功文本。人工核对 A 的 HAP 只对应 A profile/App ID `cn.alfadb.netbird.e3physvpna`，B 的 HAP 只对应 B profile/App ID `cn.alfadb.netbird.e3physvpnb`；两者均须是 Debug/普通开发签名、包含 `PHYS-1` 所代表设备并来自冻结的 API/架构配置。

`Get-FileHash` 的路径必须加单引号，避免空格或 PowerShell 解析改变目标：

```powershell
Get-FileHash -Algorithm SHA256 '<signed-a-hap>'
Get-FileHash -Algorithm SHA256 '<signed-b-hap>'
Get-FileHash -Algorithm SHA256 '<a-profile-p7b>'
Get-FileHash -Algorithm SHA256 '<b-profile-p7b>'
Get-FileHash -Algorithm SHA256 '<debug-certificate-cer>'
```

临时 JSON、导出的 profile、cert chain 与命令日志可能含 UDID 或其他身份数据。所有验签输出必须位于仓外 `verify-temp`，只在本机人工判读，随后删除；不得回传、归档或保留原始输出。回传仅包含：DevEco Studio/SDK/HDC 版本、git SHA、A/B bundle、signed HAP SHA-256、profile SHA-256、证书公开 SHA-256 或 fingerprint、四项验证的通过状态、以及构建输出路径的稳定别名。不得回传私钥、密码、UDID、真实 endpoint、动态 target、profile 原始验证输出或任何其他秘密。

## 完成即停止

签名 build 与验证阶段的完成即停止要求按当时事实履行：该交接本身没有安装 HAP、运行或启动 VPN。后续专用 runner 的唯一 initial live 已形成独立证据，但在 continuous capture、staging 与 install 前即因 build drift 停止，`campaign_started=false`，A/B 仍未安装或运行。

源码/SDK/final HAP hash、清理基线、受控采集根、独立 review 角色、campaign ID、固定 60 秒窗口和 Settings 预测路径曾冻结并使计划进入 `ready`。API23 live model 匹配 `PLA-AL10`，live build 脱敏投影的可见 suffix 与冻结 build 不同，initial 与原 campaign/evidence ID 已消费。rebind、`ADJ-20260806-0003`、host reverify、`ADJ-20260806-0004` build 确认、`ADJ-20260807-0001` 跨日重新编号、API26 0001 live `consumed-blocked` 与 API26 0002 live `consumed-blocked` 后：新元组已冻结；`E3-PHYS-PREFLIGHT-20260807-0002` / `EV-E3-PHYS1API26-20260807-0002` 为 `consumed-blocked`/`reviewed-pass/blocked`（prior blocked `E3-PHYS-PREFLIGHT-20260807-0001` / `EV-E3-PHYS1API26-20260807-0001` 保留；历史准备 `E3-PHYS-PREFLIGHT-20260806-0002` / `EV-E3-PHYS1API26-20260806-0001` 标 `superseded-unexecuted`），原 FINAL HAP hashes 复用 reverify PASS（public manifest `66a70a52c92b927d4b23e528ae6eaf1b52169e504291c6ff0e7efa4c7ffee010`；HAP/signature/profile/member hashes 未变；不主张安装兼容性），HDC build 已由 `EV-E3-PHYS1BUILD7-20260806-0001` 实测确认逐字匹配（仅 build-confirm；API `26` 仍 rebind 实测）；当前计划为 `blocked-awaiting-adjudication`（无 auto retry/新 ID/设备命令授权；本登记禁 HDC），E8 `CLOSED`。当前证据见 [API23 物理预检记录](evidence/e3-physical-preflight-2026-08-06.md)、[API26 0001 blocked 记录](evidence/e3-physical-preflight-api26-2026-08-07.md)、[API26 0002 blocked 记录](evidence/e3-physical-preflight-api26-0002-2026-08-07.md)、[rebind 记录](evidence/e3-physical-rebind7-2026-08-06.md) 与 [build 确认记录](evidence/e3-physical-build7-confirm-2026-08-06.md)。

## 故障分流

| 情况 | 处理 |
| --- | --- |
| 无线 HDC endpoint 变化 | 仅在仓外更新并重新连接动态 endpoint；稳定别名仍为 `PHYS-1`，不重新发现设备元组，也不修改仓库。 |
| 签名错误 | 停止构建，核对当前 product、App ID、证书、profile 与设备注册；不要通过修改权限、改 bundle 或越权签名绕过。 |
| 任何安装期错误 | 本交接不复现安装期错误，禁止为排障执行 `install`；停止并仅报告不含敏感数据的签名/构建上下文。 |
| bundle mismatch | 停止，确认 A=`cn.alfadb.netbird.e3physvpna`、B=`cn.alfadb.netbird.e3physvpnb`，以及 product/profile/App ID 一一对应；不要复用错误 profile。 |
| 证书或 profile 过期 | 在 AGC 续期或重新创建对应 Debug 证书/profile，并重新执行本地签名、验证与最终 hash；通知主会话，因为 profile/hash 输入已经变化。 |
| API 配置漂移 | 停止并恢复/复核 compile API `24`、target/compatible API `23`；不得以改 API、改 ABI 或换设备规避问题。 |

所有故障都不得用 system/debug/enterprise/root、`MANAGE_VPN`、隐藏服务或权限授予命令绕过。

## 已完成回传

2026-08-06 已按下列模板完成最小回传；不含真实路径、UDID、alias、password 或 signing material。四项验证均同时满足 exit `0` 与人工核对 `pass`，HAP 内嵌 profile 与对应外部 profile byte-equal。

```markdown
- [x] Git branch: `main`
- [x] Git preparation baseline: `f44be17331e5bc67a5eff702badba41cbd7a195f`
- [x] DevEco Studio: `6.1.1.290` / Build `243.24978.46.36.611290`
- [x] HarmonyOS SDK / compile API: `6.1.1.125 / 24`
- [x] target / compatible API: `23 / 23`
- [x] HDC: `3.2.0d`; SHA-256 `fff02abf2e61603e491e896aa6195e78db0c1779a6d7b992b89757a9a3c72116`
- [x] Product A / bundle: `default` / `cn.alfadb.netbird.e3physvpna`
- [x] Product B / bundle: `vpnB` / `cn.alfadb.netbird.e3physvpnb`
- [x] A signed HAP: SHA-256 `3a98ad68bfc6253fe10b37a262e0051267b3b3083e6943f8207d68482d00c244`; size `106210`
- [x] B signed HAP: SHA-256 `1adfa9664e59e7a9dc3da6650a59972517f5262a9b8160f4b3f6416770da3c26`; size `106212`
- [x] A profile SHA-256: `a3abfc6ac351cf06f5639b31f108c80edcdcd96080f43ccfd48ce12a07325b05`
- [x] B profile SHA-256: `f09af0f314773c53d61d90804332605317ec6a61316add0df3672067da99a16e`
- [x] Certificate file SHA-256: `c13847ecd674a330acb1dfb9df027eb68b21ccadd90eca6e21ebd5a515d6d7fc`
- [x] A/B `verify-profile` and `verify-app`: exit `0` + manual content review `pass`
- [x] Embedded A/B profiles are byte-equal to their corresponding external profiles
- [x] Signed audit: only `INTERNET`; VPN `exported=false`; API `23`; debug; sole arm64 `libfdprobe.so`
- [x] A/B member-list SHA-256: `216acabcd1f1c0efdc2ed6fbf89b4d88a1dd064bf5d508d4f692447a9b0f0166` / `4177f5c11d291bb20730ff45543b2ed5fcda9b8a349dbbe568ee01c89cdc82c2`
- [x] No signing config or material staged; temporary verification outputs removed after manual review
- [x] No HAP installed, no VPN run, no campaign started
- [x] Enrollment exception consumed once; no endpoint, UDID, password, private key, raw profile output, or signing material returned
- [x] Completion-stop requirement fulfilled; subsequent device action requires the dedicated plan
```

上述值是已完成回传。以下空白模板保留供未来发生经批准漂移后的重新回传，不授权自行重建、enrollment 或设备执行：

```markdown
- [ ] Git branch / SHA: `<branch>` / `<full-sha>`
- [ ] DevEco Studio / SDK / HDC: `<versions-and-public-hashes>`
- [ ] Product A/B bundle and FINAL HAP SHA-256/size: `<values>`
- [ ] A/B profile and certificate public SHA-256: `<values>`
- [ ] Four verification commands: exit `0` + manual review `pass`
- [ ] Embedded profiles byte-equal; signed content/member-list audit passed
- [ ] No install/run/campaign and no sensitive value returned
- [ ] Drift decision reference: `<required-before-any-repeat>`
```

## 是否需要重启

初次启用设备开发者选项时，设备会自动重启一次。当前的签名与构建通常不需要重启；无线调试开关的启用、关闭或重新连接也不需要重启。不要为了签名问题随意重启或重置设备；先按故障分流核对配置。
