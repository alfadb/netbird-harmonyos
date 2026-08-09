# Debian 13 原 Linux worker：E1 v0.76.3 stock Go loader/runtime 单次重放交接

最后核验：2026-08-09

本文只交接用户在**恢复历史 Linux worker（Debian 13 + Command Line Tools 6.1.1.290 + Beta 26.0.0.461 + Go 1.25.12 + ffmpeg + KVM Emulator）**后对 E1 v0.76.3 stock Go loader/runtime 的**一次完整重放**：`spikes/r1-api24-hap/e1-stock-go-replay.sh` 完整模式。它不授权物理设备 campaign，不改变 `R0`、`E8` 或 `E3-PHYS-PREFLIGHT` 计划状态。完整重放边界、runner 输入与判定见 runner 脚本本身及 [host preflight 记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)；E1 门语义见 [证据与脱敏 Schema](evidence-schema.md) 与 [R0 任务章程](r0-charter.md)。

**本文不自引用本次提交 SHA**：最终 expected commit 以用户收到的推送 SHA 为准，拉取后用 `git rev-parse HEAD` 与该 SHA 核对，一致才继续。

## 1. 当前状态（交接基线）

| 项 | 状态 |
| --- | --- |
| NetBird 正式基线 | `v0.76.3` / commit `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` / `go 1.25.5` / `toolchain go1.25.12`（runner 固定常量，启动时在线核验） |
| E1 v0.76.3 stock Go loader/runtime | **尚未产生任何测量、无 pass**；`EV-E1-EMU24-20260809-0001` 已于 2026-08-09 首次真实执行，但因 runner defect 在测量前中止（`exit 1`，见 [0001 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)），**ID 已消耗、禁止同 ID 重跑**；`EV-E1-EMU24-20260809-0002` 亦于同日第二次真实执行，因 runner defect（`readelf -lW` 字面 `PT_TLS` grep 假阴性）在平台测量前中止（`exit 1`，见 [0002 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0002-consumed-failure-2026-08-09.md)），**ID 已消耗、禁止同 ID 重跑**；下一次唯一正式 ID 为 `EV-E1-EMU24-20260809-0003` |
| host-only 前置证据 | `EV-E1-EMU24HOST-20260809-0001` 已登记：`execution: not-run-host-preflight`、`record_status: collected`、`verdict: blocked`；只覆盖 host 侧核验，不形成平台结论、不占用 runtime evidence ID（见 [记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)） |
| E8 | `CLOSED` |
| E3-PHYS-PREFLIGHT | `plan_status: blocked-awaiting-device-authorization`；用户显式设备授权 + fresh device confirmation 完成前**禁止 PHYS-1 HDC**、无 auto retry、无新 ID、无设备命令授权（见 [计划](e3-physical-preflight.md)） |

E3 的“禁 HDC”指物理设备 HDC。本 runner 的 HDC 只连接固定 Emulator target `127.0.0.1:10000`，不触碰任何物理设备，与 E3 约束不冲突。

## 2. 拉取与确认

在 Debian worker 上进入仓库根（历史固定位置为 `/home/worker/work/base/netbird-harmonyos`；若 clone 位置不同，以实际仓库根为准，**以下命令均在仓库根执行**）：

```bash
cd /home/worker/work/base/netbird-harmonyos
git switch main
git fetch origin
git pull --ff-only
git rev-parse HEAD
git status --short --branch
```

确认：

- `git rev-parse HEAD` 输出 == **用户收到的推送 SHA**（以推送通知为准，本文不预写）。
- `git status --short --branch` 显示 `## main...origin/main`（无 ahead/behind、无未提交改动、无未跟踪文件）——worktree 干净。
- 任一不符：停止，核对后重试；不要在不干净的 worktree 上运行重放。

## 3. Debian 环境检查（缺项停止，不安装/不升级）

按 [工具链运行手册](toolchain-runbook.md) 的恢复顺序先做只读健康检查：

```bash
"$HOME/.init/harmonyos-check.sh"
```

再手动核对固定矩阵（引用 [toolchain-runbook.md](toolchain-runbook.md) 与 [development-environment.md](development-environment.md)）：

```bash
source "$HOME/harmonyos/env.sh"
readlink -f "$HOME/harmonyos/command-line-tools/current"    # 期望 …/6.1.1.290
readlink -f "$HOME/harmonyos/emulator-current"              # 期望 …/26.0.0.461
test -d "$HOME/harmonyos/emulator-images/system-image/HarmonyOS-6.1.1" && echo images-ok
test -f "$HOME/harmonyos/emulator-instances/netbird_api24_phone.ini" && echo instance-ok
exec 9<>/dev/kvm && printf 'KVM fd opened\n' && exec 9>&-
command -v ffmpeg gh bash
/home/worker/go/bin/go version          # 期望 go version go1.25.12 linux/amd64
```

预期：稳定 6.1.1.290、Beta 26.0.0.461、镜像 `HarmonyOS 6.1.1(24)` / software `6.1.0.125`、实例 `netbird_api24_phone`、KVM fd 可打开、ffmpeg/gh/bash 在 PATH、Go 1.25.12 在位。

任何一项缺失、`WARN` 或版本漂移：**停止**，不安装、不升级、不隐式 `apt`（缺失系统库应更新 base image，见运行手册）；报告主会话后再继续。runner 完整模式自身还会再做一遍 `HOST_CHECK`（含 hvigorw/ohpm/hvigor-ohos-plugin/emulator/hdc/go/ohos-x86_64-clang/ffmpeg/gh/bash/readelf/file/unzip/ss/git/tar/base64/timeout/pgrep/mktemp/sha256sum/awk/shellcheck），任何缺失都会 fail closed。

## 4. runner 安全边界与固定输入

runner 固定常量（不可由环境覆盖，`--selftest` 会逐项验证 guard）：

| 项 | 固定值 |
| --- | --- |
| baseline | `v0.76.3` / `f65f7b347ee4e7de6d98c488d3d894cd018b02b6` / `go 1.25.5` / `toolchain go1.25.12` |
| runGoProbe 源码快照 | `34d512541ca8047f8e3796abd6d85ef94cc13559`（`git archive` 提取到临时目录构建，不改当前 C-only 探针源码） |
| Emulator target | `127.0.0.1:10000`（仅此一个） |
| Emulator 实例 | `netbird_api24_phone`（HDC 端口固定 10000） |
| bundle / test module | `cn.alfadb.netbird.r1probe` / `entry_test` |
| 期望 loader 拒绝短语 | `initial-exec TLS resolves to dynamic definition` |

**禁止输入**（guard 在任何 evidence 文件创建前拒绝）：

- `PHYS_1_TARGET` 已设置 → fail（本 runner 只操作 Emulator，绝不物理设备）。
- `TARGET` / `HDC_TARGET` / `EMULATOR_TARGET` 非 `127.0.0.1:10000` → fail。
- `EMULATOR_HDC_PORT` ≠ `10000` → fail。
- `EMULATOR_INSTANCE` ≠ `netbird_api24_phone` → fail。
- 环境默认值（`STABLE_TOOLS`/`BETA_TOOLS`/`GO_BIN`/`EMULATOR_INSTANCE_PATH` 等）指向历史 worker 固定路径，**不要改动**；正式运行 `EVIDENCE_ROOT` 用默认（仓内 `docs/evidence/raw`）。

## 5. 顺序命令（仓库根执行）

```bash
# 0) 清除可能残留的 target 变量，防止 guard 误判
unset PHYS_1_TARGET TARGET HDC_TARGET EMULATOR_TARGET

# 1) 语法检查
bash -n spikes/r1-api24-hap/e1-stock-go-replay.sh

# 2) 纯 host selftest（无网络/HDC/Emulator，不写 evidence）
bash spikes/r1-api24-hap/e1-stock-go-replay.sh --selftest
# 期望：judgment/guards/no-clobber/cleanup 各 pass，SELFTEST_RESULT=PASS

# 3) host-only preflight，使用临时 EVIDENCE_ROOT（不要用默认 root：仓内已有
#    EV-E1-EMU24HOST-20260809-0001 transcript，默认 root 会被 no-clobber 拒绝，
#    且不允许覆盖仓内 host evidence）
EVIDENCE_ROOT=/tmp/e1-stock-go-preflight-20260809-0003 \
  bash spikes/r1-api24-hap/e1-stock-go-replay.sh --preflight
# 期望：HOST_PREFLIGHT_MISSING_COUNT=0、HDC_RUN=false、
#       EXECUTION=not-run-host-preflight、RECORD_STATUS=collected、VERDICT=blocked、exit 0

# 4) 正式运行前确认无物理 target、无 Emulator/HDC 残留（期望均无输出）
env | grep -E '^(PHYS_1_TARGET|TARGET|HDC_TARGET|EMULATOR_TARGET)=' || true
pgrep -af 'emulator/Emulator.*-start|qemu-system' || true
ss -ltnp | grep -E ':(10000|5555|8710)[[:space:]]' || true

# 5) 正式完整重放，仅运行一次
EVIDENCE_ID=EV-E1-EMU24-20260809-0003 \
  bash spikes/r1-api24-hap/e1-stock-go-replay.sh
```

说明：

- preflight 是**只读**的（不启动 Emulator、不运行 HDC）；其临时 transcript 只用于确认环境与 runner 状态，可保留或删除，不进入仓库。
- 正式模式 runner 会自行：`hdc kill` → 停残留 Emulator → 检查残留 → 冷启动 `netbird_api24_phone` → 连接/就绪验收 → 安装双 HAP → `aa test` → 定向 HiLog 采集 → 判定 → 清理（删 guest staging、卸载、停 Emulator、kill hdc、检查残留端口/进程）。
- 运行期间保持终端与日志可见；不要并行启动其他 Emulator 或 HDC。

## 6. 预期分支与停止规则

| 结果 | 含义 | 处理 |
| --- | --- | --- |
| exit `0` 且 `MEASURED_VERDICT=blocked` | **预期**：stock Go 1.25.12 `initial-exec TLS` loader 拒绝，measured blocked（**不是 runner failure**）；`VERDICT=blocked`、`RECORD_STATUS=collected` | 保留产物，按第 7 节核对后交回主会话审查 |
| exit `0` 且 `MEASURED_VERDICT=pass` | 意外：stock Go 加载成功 | 保留产物，**仅待独立审查**，不自行下任何结论 |
| exit `1` 且 `FAIL_REASON=…` | runner fail（baseline 核验 / 构建 / 安装 / 判定 fail） | 保留全部材料、**停止**、不得同 ID 重跑 |
| exit `1` 且无 `FAIL_REASON`（如 0001 的 readonly 赋值中止） | runner 脚本缺陷在测量前中止（`set -e` 直接退出，未进入 `fail()`） | 保留全部材料、**停止**、不得同 ID 重跑；交回主会话修复 runner 后由主会话分配新 ID（0001 已消耗，见 [consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)） |
| exit `1` 且 `CONNECTIVITY_VERDICT=blocked` / `READINESS_VERDICT=blocked` | 基础设施失败（Emulator 起不来、HDC 连不上、guest 未就绪） | 保留全部材料、**停止**、不得同 ID 重跑 |

同 ID 重跑会被 no-clobber 以 `REFUSE_OVERWRITE` 拒绝（exit `2`）——这是设计，不是故障；任何需要重跑的情况都先停止并交回主会话决策（新 ID 或新 `EVIDENCE_ROOT` 由主会话分配）。

## 7. 产物 / 证据文件与验证

完整模式在默认 `EVIDENCE_ROOT`（`docs/evidence/raw/`）下生成，前缀 `EV-E1-EMU24-20260809-0003-`：

| 文件 | 说明 |
| --- | --- |
| `transcript.log` | 全流程 transcript（含 teardown 输出；seal 后追加最终 hash） |
| `manifest.txt` | 清单：evidence_id/code_sha/baseline/snapshot/target_tuple/各产物 sha256/`verdict`/`e8_status=CLOSED`；**末尾由 seal 追加 `transcript_final_sha256` 与 `manifest_sha256`** |
| `hilog-tag.log` | 定向 TAG HiLog（`BASELINE_RESULT`/`GO_SPIKE_RESULT` 判定来源） |
| `hilog-app-full.log` | 全量 app HiLog |
| `emulator-console.log` | Emulator 控制台输出 |
| `build.log` / `go-build.log` | HAP 构建日志 / stock Go 构建日志 |
| `baseline-verify.log` | v0.76.3 基线在线核验日志 |
| `aa-test.log` | `aa test` 输出 |

运行后核对：

```bash
git status --short --branch
# 期望：仅 docs/evidence/raw/EV-E1-EMU24-20260809-0003-* 为新增（构建产物 entry/libs/、build/ 等已被 .gitignore 忽略）

grep -E '^(verdict|transcript_final_sha256|manifest_sha256|e8_status)=' \
  docs/evidence/raw/EV-E1-EMU24-20260809-0003-manifest.txt

grep -E '^(VERDICT|MEASURED_VERDICT|RECORD_STATUS|FINAL_RESIDUAL_PROCESS|FINAL_RESIDUAL_PORT|CLEANUP_END)=' \
  docs/evidence/raw/EV-E1-EMU24-20260809-0003-transcript.log
# 期望：VERDICT/MEASURED_VERDICT 按第 6 节分支；RECORD_STATUS=collected；
#       FINAL_RESIDUAL_PROCESS=false、FINAL_RESIDUAL_PORT=false、CLEANUP_END=teardown-complete
```

## 8. 完成后交回（非敏感摘要）

不要自行把 `record_status` 从 `collected` 升级为 `reviewed-pass`；不要自行 commit。交回主会话 / 独立审查的最小非敏感摘要：

```markdown
- [ ] E1 v0.76.3 stock Go replay: exit `<0/1>`；`MEASURED_VERDICT=<blocked/pass>`；`VERDICT=<blocked/pass/fail>`；`RECORD_STATUS=collected`
- [ ] evidence 前缀: `docs/evidence/raw/EV-E1-EMU24-20260809-0003-*`
- [ ] manifest 最终 hash: `transcript_final_sha256=<…>` / `manifest_sha256=<…>`
- [ ] 残留: `FINAL_RESIDUAL_PROCESS=false` / `FINAL_RESIDUAL_PORT=false`；`git status` 仅新增 evidence 文件
- [ ] 未自行升级 reviewed-pass；未提交；未重跑同 ID
```

## 9. 关键提醒

- **完整模式此前从未在任何主机跑通过**。Windows 上只登记了 host-only preflight（`EV-E1-EMU24HOST-20260809-0001`），Windows DevEco Studio Emulator 与历史 Linux worker 的 Emulator 不等价，不能作为替代运行环境。Debian 上的首次真实执行 `EV-E1-EMU24-20260809-0001` 在测量前因 runner defect 中止（`exit 1`，ID 已消耗，见 [0001 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0001-consumed-failure-2026-08-09.md)）；第二次真实执行 `EV-E1-EMU24-20260809-0002` 亦在平台测量前因 runner defect（`readelf -lW` 字面 `PT_TLS` grep 假阴性）中止（`exit 1`，ID 已消耗，见 [0002 consumed-failure 记录](evidence/e1-stock-go-v0763-replay-0002-consumed-failure-2026-08-09.md)）；runner 已修复，本次 `EV-E1-EMU24-20260809-0003` 是修复后的完整重放。
- 遇到脚本问题**先停**：不绕过 guard、不绕过 cleanup、不绕过 no-clobber，不手工修改/删除 evidence 文件后重跑；先保留材料并交回主会话。
- runner 全程只操作 `127.0.0.1:10000` Emulator；不连接、不安装、不启动任何物理设备。

## 10. 后续顺序

1. 主会话 / 独立审查先完成 E1 结果审查（`collected` → `reviewed-pass` 或按治理重判）与路线决策（stock Go loader 负面结论绑定 v0.76.3 基线；E1 overall Go 是否形成 pass 由审查决定）。
2. **E1 结论成立后**才恢复 E3 新授权 / freeze 流程；当前 `blocked-awaiting-device-authorization` 保持不变，用户显式设备授权 + fresh device confirmation 完成前无任何设备命令授权（见 [e3-physical-preflight.md](e3-physical-preflight.md)）。

## 引用文档

- [E1 v0.76.3 host preflight 记录](evidence/e1-stock-go-v0763-host-preflight-2026-08-09.md)
- [工具链运行手册](toolchain-runbook.md)（固定矩阵、健康检查、Emulator 启停）
- [开发环境与 Linux Emulator](development-environment.md)（HOME 布局、持久化、检查脚本）
- [证据与脱敏 Schema](evidence-schema.md)（`record_status`/`verdict` 语义）
- [R0 任务章程](r0-charter.md) 与 [双目标实施路线图](roadmap.md)（R0 基线、E1/E8 门）
- [E3-PHYS-PREFLIGHT 计划](e3-physical-preflight.md)（blocked-awaiting-device-authorization、禁 PHYS-1 HDC）
