# Linux Worker 非门控 API24 x86_64 Emulator 复跑交接

最后核验：2026-08-11

本文交接一次**非门控**的 N0(b) API 24 x86_64 phone Emulator 复跑：用户将切到此前已成功跑通 N0 的 Linux worker，按本文顺序执行只读检查、透明代理绕行、连通验证、runner selftest / dry-run 与一次非门控 full rerun。本文只描述复跑操作，**不改变任何 gate 状态**；N0 的 `reviewed-pass/pass` 结论、E8 `CLOSED`、物理设备禁令均不受影响。

## 1. 目的与非门控边界

- **目的**：在 Linux worker 上对 `spikes/n0-native-core/n0-emulator-run.sh` 的完整正式流程做一次**非门控复跑**，确认此前 `EV-N0-EMU24-20260810-0002` 的 pass 结论在当前环境可复现。
- **非门控边界（必须逐条遵守）**：
  - 本次复跑**不产生正式 evidence**：evidence 写入仓外临时根，**不提交、不推送、不作为任何 gate 判定输入**。
  - **不得复用任何已有 evidence ID**，尤其 `EV-N0-EMU24-20260810-0002`（已消耗，no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit 2）；`EV-N0-EMU24-20260810-0001`（consumed-failure）同样不得复用。
  - 不改变 N0 `reviewed-pass/pass`、E8 `CLOSED`、E3 物理设备授权状态；不授权物理设备 HDC，只操作固定 Emulator target `127.0.0.1:10000`。
  - 复跑只覆盖 N0(b) 范围（单一 native WireGuard core 的 C ABI 加载/冒烟）；不证明 VPN fd/TUN/protect/management/ICE/relay/UI/arm64 load/physical/product。
  - 不包含 endpoint、target token、密钥、签名材料、UDID 或其他敏感值（见第 10 节带回模板）。

## 2. Windows 当前阻塞（背景）

Windows 侧 DevEco Studio 镜像已安装，但承载 Windows 的 Proxmox VM **无嵌套虚拟化**：Windows 版 HarmonyOS Emulator 依赖 Hyper-V/WHP（Windows Hypervisor Platform）加速，嵌套虚拟化缺失时无法启动（与 Linux 版依赖 KVM 不同；Linux worker 才检查 `/dev/kvm`）。因此本次复跑切到此前已成功跑通 N0 的 Linux worker（`/dev/kvm` 可用、工具链与镜像已就位）。Windows 交接（`windows-development-handoff.md`）只承载 E3-PHYS-PREFLIGHT 签名构建，与本次 Emulator 复跑无关；本次不涉及物理设备、不涉及签名。

## 3. Linux worker 精确环境/路径前置

以下路径是 runner 硬编码的固定输入，Linux worker 必须逐项存在（此前跑通 N0 的 worker 应已满足；缺失即停止，不自行安装）：

| 角色 | 固定路径 |
| --- | --- |
| 工作区 | `/home/worker/work/base/netbird-harmonyos`（runner 的 `WORKSPACE` 必须是脚本仓库本身，禁止覆盖） |
| 稳定构建链路 | `/home/worker/harmonyos/command-line-tools/6.1.1.290`（hvigorw/ohpm/native SDK，构建用） |
| Beta Emulator 链路 | `/home/worker/harmonyos/command-line-tools/26.0.0.461`（Emulator 二进制 + hdc 3.2.0e） |
| Emulator 镜像 | `/home/worker/harmonyos/emulator-images`（`HarmonyOS 6.1.1(24)`，software `6.1.0.125`） |
| Emulator 实例 | `/home/worker/harmonyos/emulator-instances/netbird_api24_phone`（固定实例，HDC `127.0.0.1:10000`） |
| 连接/停止 helper | `/home/worker/harmonyos/bin/emulator-connect`、`/home/worker/harmonyos/bin/emulator-stop`（runner 硬编码，禁止覆盖） |
| 环境脚本 | `$HOME/harmonyos/env.sh`（新 zsh 由 `.zshrc` 自动加载；bash 手动 `source`） |
| 健康检查 | `$HOME/.init/harmonyos-check.sh` |
| 显示环境 | `DISPLAY=:1`、`XAUTHORITY=/home/worker/.Xauthority`、`XDG_RUNTIME_DIR=/tmp/runtime-worker`（runner 硬编码） |

固定版本矩阵：稳定 HDC 3.2.0d 只服务稳定 SDK；Beta HDC 3.2.0e 只服务 Emulator 连接，两者不混用。Node.js 隔离原则：`env.sh` 只注入工具/SDK/HDC/Emulator 路径，不替换默认 Node.js（zsh 默认 Volta Node 24.15.0）；构建只调用对应版本目录顶层 `bin/hvigorw` / `bin/ohpm`。

## 4. 进入后只读检查

按顺序执行；任一步出现 `WARN`、版本漂移或缺失库时，先处理该项，**不启动 Emulator**。

```bash
# 1) 只读健康检查（不下载、不安装、不启动 Emulator）
"$HOME/.init/harmonyos-check.sh"

# 2) 登录式 zsh 自动加载与 Node 隔离
zsh -lic 'node --version; printf "%s\n" "$HARMONYOS_HOME" "$HARMONYOS_EMULATOR_HOME"'
zsh -lic 'env | grep -E "^(NODE_HOME|DEVECO_NODE_HOME)=" || true'
zsh -lic 'command -v node; command -v hvigorw; command -v ohpm; command -v emulator-connect'
# 预期：node v24.15.0；第二条无输出；wrapper 解析到稳定 current/bin

# 3) 软链接与持久目录
readlink -f "$HOME/harmonyos/command-line-tools/current"      # 预期 -> .../6.1.1.290
readlink -f "$HOME/harmonyos/emulator-current"                # 预期 -> .../26.0.0.461
test -d "$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1" && echo IMAGE_OK
test -f "$HOME/harmonyos/emulator-instances/netbird_api24_phone.ini" && echo INSTANCE_OK

# 4) KVM 实际可打开读写 fd（仅看到路径不够）
ls -l /dev/kvm
exec 9<>/dev/kvm && printf 'KVM fd opened\n' && exec 9>&-

# 5) 动态库缺失检查（不修改系统）
source "$HOME/harmonyos/env.sh"
ldd "$HARMONYOS_BUNDLED_NODE" | grep 'not found' || true
ldd "$HARMONYOS_STABLE_HDC" | grep 'not found' || true
ldd "$HARMONYOS_EMULATOR_HDC" | grep 'not found' || true
ldd "$HARMONYOS_EMULATOR_HOME/emulator/Emulator" | grep 'not found' || true

# 6) 仓库状态（full 模式硬性前置：必须 clean 且 runner 在 HEAD 中）
cd /home/worker/work/base/netbird-harmonyos
git status --short            # 必须无输出；有输出则 full 会 fail-closed，只能先清理或只跑 dry-run
git rev-parse HEAD            # 记录 code_sha，带回
git cat-file -e "2c567dc721c6582f93a15b241e843e3bbff3f7f3^{commit}" && echo SNAPSHOT_OK
git cat-file -e "HEAD:spikes/n0-native-core/n0-emulator-run.sh" && echo RUNNER_IN_HEAD
```

## 5. 透明代理绕行（可选；仅当确有必要）

**先决判断**：若 Linux worker 当前出网本就能正常绕过透明代理（公网连通检查直接通过、或已有独立出口），**完全跳过本节**，不修改任何路由。本节只在确认 worker 出网被透明代理拦截、且必须绕行时才执行。

**层次原则**：绕行只发生在**宿主侧**默认路由；**guest 保留 `10.0.2.2` slirp 网关，不得直接改 guest 路由到 `192.168.50.1`**。guest 经 Emulator slirp 用户态 NAT 出网，slirp 转发到宿主后按宿主新默认路由出网，guest 无需也不得修改。

**fail-closed 前提（全部满足才继续，否则停止回报主会话）**：

1. **管理链路安全**：若当前 SSH 会话是唯一管理链路，且无独立管理路由或带外控制台，**停止**——替换默认路由会切断当前会话，无法恢复。变更前必须在会话内确认当前操作者来源地址有显式 host route 且不受默认路由替换影响；该地址属敏感信息，**不得输出或持久化到仓库/evidence 文件**（任何含来源地址的命令输出，落盘前人工脱敏）。
2. **网关可达性**：`192.168.50.1` 必须确认是目标接口链路上的可达 next hop（见 5.1 的 `ip route get` 输出含 `dev <IFACE>`），否则替换默认路由必然断网。
3. **完整记录**：变更前必须完整记录原始路由状态（见 5.1），恢复时按记录逐项人工恢复。

```bash
# 5.1 修改前：完整记录原路由（必做；只读）
mkdir -p "$HOME/n0-rerun-evidence"
ip -details route show table all > "$HOME/n0-rerun-evidence/host-route-before.txt" 2>&1
ip route show >> "$HOME/n0-rerun-evidence/host-route-before.txt" 2>&1
ip route get 192.168.50.1 >> "$HOME/n0-rerun-evidence/host-route-before.txt" 2>&1
cat "$HOME/n0-rerun-evidence/host-route-before.txt"
# 人工确认：原默认路由完整属性（table/metric/proto/onlink 等）；ip route get 输出含 dev <IFACE>，
# 即 192.168.50.1 在目标接口链路上可达；若输出含操作者来源地址，落盘前人工脱敏

# 5.2 修改宿主默认路由到 192.168.50.1（占位：<IFACE> 替换为 5.1 确认的目标接口名；需 root）
sudo ip route replace default via 192.168.50.1 dev <IFACE>

# 5.3 验证绕行生效（见第 6 节连通检查）
ping -c 3 -W 2 192.168.50.1
curl -fsS --max-time 10 https://example.com/ -o /dev/null && echo PUBLIC_NET_OK
```

**恢复（复跑完成后执行；人工逐项，禁止自动提取/脚本化）**：按 5.1 记录的原始完整 default route 逐项人工恢复，**不得**用只带网关/网卡的简化 `ip route replace default via <OLD_GW> dev <IFACE>` 覆盖——它会丢失原路由的 table/metric/proto/onlink 等属性。步骤：

1. 若默认路由由 NetworkManager / systemd-networkd 管理（`nmcli -t -f NAME,TYPE connection show` 或 `networkctl` 确认），**优先使用该管理器的临时/既有 profile 方法**恢复（如 `nmcli connection up <既有 profile>` / `nmcli device reapply <IFACE>`）；以管理器实际列出的 profile 名为准，**不要臆造具体连接名**。
2. 若为手工静态路由，按 5.1 记录逐项重建：`sudo ip route replace <原完整属性序列>`（含 table/metric/proto/onlink 等，逐字对照记录）。
3. 恢复后验证：`ip -details route show table all` 与 5.1 记录逐项比对；`ip route get 1.1.1.1` 确认回到原路径；`ping -c 3 -W 2 <原网关>` 确认可达。

**禁止项**：不得在 guest 内执行任何路由修改（如 `hdc shell ip route replace default via 192.168.50.1 ...`）；guest 路由检查只读（见第 6 节）。

## 6. 启动 / HDC / OS API arch / route / 连通检查

以下命令会启动 Emulator，只在明确需要时执行；不要把启动加入 `.zshrc`、Pod init 或健康检查。第 6.5 节宿主侧连通检查**仅当第 5 节确有必要且已完成路由修改时执行**；若第 5 节被跳过，跳过 6.5。

```bash
# 6.1 后台 tmux 启动（显式端口 10000）
tmux new-session -d -s harmonyos-emulator-run \
  "HDC_PORT=10000 $HOME/harmonyos/bin/emulator-start"
tmux capture-pane -pt harmonyos-emulator-run   # 查看启动输出；不要立即反复重启

# 6.2 Beta HDC 显式连接 127.0.0.1:10000
HDC_PORT=10000 "$HOME/harmonyos/bin/emulator-connect"

# 6.3 最小 shell smoke + OS API arch 检查（Beta HDC 3.2.0e）
source "$HOME/harmonyos/env.sh"
HDC="$HARMONYOS_EMULATOR_HDC"; TARGET=127.0.0.1:10000
"$HDC" -t "$TARGET" shell echo netbird-hdc-smoke
"$HDC" -t "$TARGET" shell uname -a                          # 预期 x86_64
"$HDC" -t "$TARGET" shell param get bootevent.boot.completed # 预期 true
"$HDC" -t "$TARGET" shell param get const.product.os.dist.name  # 预期 HarmonyOS
"$HDC" -t "$TARGET" shell param get const.ohos.apiversion   # 预期 24（API 24）
"$HDC" -t "$TARGET" shell param get const.product.cpu.abilist   # 预期含 x86_64

# 6.4 guest route 只读检查：确认 slirp 网关 10.0.2.2 不变（0202000A 为 little-endian 表示）
"$HDC" -t "$TARGET" shell cat /proc/net/route
# 预期：default 行 gateway 列为 0202000A（=10.0.2.2）；直连网络 0002000A/00FFFFFF（=10.0.2.0/24）
# 若 gateway 列不是 0202000A：停止，回报主会话，不得自行修改 guest 路由

# 6.5 宿主侧连通检查（仅当第 5 节确有必要且完成）
ping -c 3 -W 2 192.168.50.1
curl -fsS --max-time 10 https://example.com/ -o /dev/null && echo PUBLIC_NET_OK
```

说明：runner 的 full 模式会自行冷启动 Emulator（先 `hdc kill` + `STOP_HELPER` 停止残留，再 `-bootMode coldboot` 冷启动），因此 6.1 手动启动的实例会被 runner 先正常停止，无需手动处理；若手动实例无法被 helper 停止，先手动 `"$HOME/harmonyos/bin/emulator-stop"` 再跑 full。

## 7. runner selftest

纯 host 检查：无网络、无 HDC、无 Emulator、无 evidence 文件。不依赖第 5/6 节。

```bash
cd /home/worker/work/base/netbird-harmonyos
bash spikes/n0-native-core/n0-emulator-run.sh --selftest
# 预期：SELFTEST_RESULT=PASS，exit 0
```

## 8. runner dry-run

完整 pipeline 排练（offline 双 ABI 构建 → snapshot 准备 → HAP 构建 → 成员/身份/哈希校验 → 公网字面量扫描），**无设备动作、无 evidence 文件**；需要数分钟构建时间。

**必须使用自定义 `EVIDENCE_ID` + 仓外 `EVIDENCE_ROOT`**：dry-run 也会执行 no-clobber 检查，默认 ID `EV-N0-EMU24-20260810-0002` + 默认 root `docs/evidence/raw` 下已有 11 份 raw，会以 `REFUSE_OVERWRITE` 拒绝（exit 2）。

```bash
cd /home/worker/work/base/netbird-harmonyos
EVIDENCE_ID=EV-N0LOCAL-EMU24-20260811-0001 \
EVIDENCE_ROOT="$HOME/n0-rerun-evidence" \
bash spikes/n0-native-core/n0-emulator-run.sh --dry-run
# 预期：DRY_RUN_VERDICT=pass，exit 0；不生成任何 evidence 文件
```

## 9. 一次非门控 full rerun

### 9.1 evidence 身份决策（已核对 runner 契约）

runner 允许通过环境变量自定义 evidence 身份（脚本第 105-106 行：`EVIDENCE_ROOT="${EVIDENCE_ROOT:-$WORKSPACE/docs/evidence/raw}"`、`EVIDENCE_ID="${EVIDENCE_ID:-$DEFAULT_EVIDENCE_ID}"`），且 `EVIDENCE_ID` 必须匹配 `^EV-[A-Z0-9]+-[A-Z0-9]+-[0-9]{8}-[0-9]{4}$`（同时保证 guest staging 路径 `/data/local/tmp/$EVIDENCE_ID` shell-safe）。因此：

- **EVIDENCE_ID**：`EV-N0LOCAL-EMU24-20260811-0001` —— gate 段 `N0LOCAL` 明显标识 **NON-GATING / LOCAL-CHECK**，不是任何正式 gate 名；日期段为复跑当日。
- **EVIDENCE_ROOT**：`$HOME/n0-rerun-evidence`（仓外临时根；HOME 持久化）。**不提交、不作为 gate 判定**。
- runner 硬编码的 manifest 字段（`stage_or_gate=N0(b)`、`record_status=collected`、`verdict=pass`、`e8_status=CLOSED`、`physical_device_used=false`）只写入仓外 evidence，仅作复跑记录，**不改变任何 gate 状态**；正式 gate 判定只认 `docs/evidence/` 下经独立审查的正式 evidence。
- 每次复跑用新 ID（或清空仓外 root），避免 no-clobber 冲突。

### 9.2 执行

前置：第 4 节全部通过、仓库 clean、runner 在 HEAD 中、snapshot commit 存在；第 5 节（如确有必要）路由已改并验证，若第 5 节被跳过则相应跳过第 6.5 节连通检查；第 6 节其余检查通过（full 会自行冷启动，无需先停手动实例）。

```bash
cd /home/worker/work/base/netbird-harmonyos
EVIDENCE_ID=EV-N0LOCAL-EMU24-20260811-0001 \
EVIDENCE_ROOT="$HOME/n0-rerun-evidence" \
bash spikes/n0-native-core/n0-emulator-run.sh
```

### 9.3 通过判据（与 0002 相同的双轴判据）

- exit `0`；transcript 关键行：`MARKER_DISTINCT_COUNT=1`、`N0_CORE_PROBE_RESULT_LINE=N0_CORE_PROBE_RESULT|verdict=PASS|N0 runProbe PASS version=n0-native-core/0.1.0+boringtun-0.7.1 keyLen=44 tickOp=0 tickSize=0`、`NAPI_SMOKE_OK=1 NAPI_X25519_OK=1 NAPI_TUNNEL_OK=1`、`GUEST_RESULT_CODE=0`、`MEASURED_VERDICT=pass`、`VERDICT=pass`。
- manifest 六 seal 字段齐备：`final_exit_code=0`、`run_status=pass`、`fail_reason=`（空）、`transcript_final_bytes=`、`transcript_final_sha256=`、`manifest_sha256=`。
- `FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`、`SENSITIVE_SCAN=pass_high_confidence_patterns`。
- 任一判据不满足：按 runner 输出定位（构建失败/连通失败/安装失败/无 marker/重复 marker 等），**不重跑同 ID**，回报主会话。

### 9.4 复跑后

- 若第 5 节执行过路由修改，按第 5 节「恢复」步骤人工逐项恢复宿主原路由并验证；若第 5 节被跳过则无需恢复。
- 仓外 evidence 保留在 `$HOME/n0-rerun-evidence`；**不 `git add`、不 commit、不推送**；如需长期保留，复制到仓外受控位置。

## 10. 复跑后需要带回的最小输出

只带回以下非敏感项；**不包含** endpoint、target token、密钥、签名材料、UDID、真实 IP:port（`192.168.50.1` 网关与 `10.0.2.2` slirp 网关属固定基础设施地址，可记录）。

```markdown
- [ ] EVIDENCE_ID: `EV-N0LOCAL-EMU24-20260811-0001`（NON-GATING / LOCAL-CHECK，仓外，未提交）
- [ ] 运行时间: `<started_at>` / `<ended_at>`（Asia/Shanghai）
- [ ] git HEAD (code_sha): `<full-sha>`
- [ ] runner exit code: `0`
- [ ] MEASURED_VERDICT / VERDICT: `pass` / `pass`
- [ ] MARKER_DISTINCT_COUNT: `1`
- [ ] N0_CORE_PROBE_RESULT_LINE: `<marker 行，仅含 version/keyLen/tickOp/tickSize，不含密钥值>`
- [ ] NAPI_SMOKE_OK / NAPI_X25519_OK / NAPI_TUNNEL_OK: `1 / 1 / 1`
- [ ] GUEST_RESULT_CODE: `0`
- [ ] FINAL_RESIDUAL_PROCESS / FINAL_RESIDUAL_PORT: `false / false`
- [ ] SENSITIVE_SCAN: `pass_high_confidence_patterns`
- [ ] manifest seal: `final_exit_code=0`、`run_status=pass`、`transcript_final_sha256=<...>`、`manifest_sha256=<...>`
- [ ] 宿主路由变更: 未修改（第 5 节跳过）/ 已修改（原 default route 完整属性已记录，恢复后 `ip -details route show table all` 与记录逐项比对确认）
- [ ] guest 网关确认: `10.0.2.2`（`0202000A`）未变，未修改 guest 路由
- [ ] 未提交、未推送、未作为 gate 判定输入
```

## 11. 故障分流（摘要）

| 情况 | 处理 |
| --- | --- |
| 第 4 节任一项失败 | 停止，不启动 Emulator；按 runbook 处理（缺库更新 base image，勿在健康检查中隐式 `apt`） |
| 仓库不 clean | full 会 fail-closed；先清理/提交，或只跑 dry-run（dry-run 豁免 git clean） |
| 默认 ID 触发 `REFUSE_OVERWRITE` | 使用第 8/9 节的自定义 ID + 仓外 root |
| guest route 网关不是 `10.0.2.2` | 停止，回报主会话；不得修改 guest 路由 |
| 宿主路由修改后公网不通 | 停止，按第 5 节「恢复」步骤人工恢复原路由，回报主会话 |
| full 失败 | 不重跑同 ID；按 runner 输出定位，回报主会话 |
| HDC shell 连续超时（约 25 分钟窗口） | 按 runbook 只读诊断；`Connected` 不等于 ready，不盲目重启 |
