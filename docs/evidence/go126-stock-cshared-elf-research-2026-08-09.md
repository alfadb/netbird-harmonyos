# Go 1.26.0 stock c-shared ELF research 对照（RS-E1-GO126ELF-20260809-0001）

最后核验：2026-08-09

本文是一次**隔离的 research-only 对照**：用与正式 `EV-E1-EMU24-20260809-0003` 相同的冻结 code snapshot 与 stock c-shared 构建环境，仅把 Go 工具链换成 `/home/worker/.local/go/bin/go`（`go1.26.0 linux/amd64`），在 `/tmp` 构建 `libgoprobe.so`，并与 0003 formal Go 1.25.12 事实做 ELF 结构对照。**不构成 E1 pass，不改变 E8**。

## 边界声明

- **未启动/停止 Emulator**：全程无 Emulator 进程操作。
- **未调用 hdc**：无任何 hdc 命令。
- **未修改 spike 源码 / runner / 正式 evidence**：`spikes/` 与 `docs/evidence/` 下既有文件零改动；构建只在 `/tmp`。
- **不宣称 E1 pass**：本记录是 host-only ELF 结构研究，guest loader 行为未测量。
- **E8 不变**：`e8_status` 保持 `CLOSED`（与 0003 记录一致）。
- **构建只在 /tmp**：源码解出、构建输出、ELF 分析全部在唯一 `/tmp/rs-e1-go126-20260809.TyZ91K` 下完成，结束后已删除并确认无残留。

## 证据记录

```yaml
research_id: RS-E1-GO126ELF-20260809-0001
information_status: current-measured
record_status: reviewed-pass
scope: research-only (host-only ELF comparison; no Emulator, no hdc, no guest loader run)
stage_or_gate: E1 (research; does not change E1 overall Go status)
related_stages_or_gates: [E8]
target_tuple:
  distribution: host Debian (x86_64); no guest target exercised
  device: none (no Emulator, no physical device)
  full_system_version: N/A (host-only; no guest queried)
  architecture: x86_64 (host; ELF target GOARCH=amd64)
  sdk_api_syscap: N/A (no SDK/API runtime surface exercised)
  channel: N/A (no HAP, no signing)
code_sha: d2ff07e537be8f96be5f5940a65edf0874be279d (repository HEAD at run time)
source_sha: 34d512541ca8047f8e3796abd6d85ef94cc13559 (frozen snapshot, git archive verified;
  identical to the snapshot used by formal 0003; task-brief hash 34d51252d370e2358535665604554762bdf0e3b6
  does not exist in this repository - git cat-file bad object - and was not used)
upstream_sha: N/A (no NetBird source exercised; pure toolchain ELF research)
toolchain:
  go: /home/worker/.local/go/bin/go -> go version go1.26.0 linux/amd64
  go_env: GOOS=linux GOARCH=amd64 GOAMD64=v1 CGO_ENABLED=1 CC=go-probe/ohos-x86_64-clang
    (wrapper -> /home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native/llvm/bin/x86_64-unknown-linux-ohos-clang)
    GOTOOLCHAIN=local
  build_script: spikes/r1-api24-hap/go-probe/build.sh (frozen snapshot copy, unmodified;
    GO_EXPECTED_VERSION_PREFIX overridden to "go version go1.26.0 " via env, no source change)
working_directory: /home/worker/work/base/netbird-harmonyos (build executed from /tmp extracted copy)
command: env HARMONYOS_NATIVE_HOME=/home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native
  GO_BIN=/home/worker/.local/go/bin/go GO_PROBE_OUTPUT_DIR=/tmp/rs-e1-go126-20260809.TyZ91K/out
  GO_TOOLCHAIN_MODE=local GO_EXPECTED_VERSION_PREFIX="go version go1.26.0 "
  bash /tmp/rs-e1-go126-20260809.TyZ91K/spikes/r1-api24-hap/go-probe/build.sh
input: frozen snapshot 34d512541ca8047f8e3796abd6d85ef94cc13559 spikes/r1-api24-hap/go-probe
  (build.sh, go.mod, ohos-x86_64-clang, probe.go); same source as formal 0003
expected: stock Go 1.26.0 c-shared build succeeds; ELF structural facts (PT_TLS, STATIC_TLS,
  TPOFF64 count, res_search/pthread_create relocation types, Go exports) compared with 0003 formal Go 1.25.12
actual: build succeeded (BUILD_RC=0); ELF structural facts same as 0003; artifact bytes changed (expected);
  see measured result below
started_at: 2026-08-09T18:46:54+08:00
ended_at: 2026-08-09T18:50:00+08:00 (approx; cleanup completed)
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
source_hashes:
  build.sh: fa882ffc5819ea362d7f2aba9a3de4f5b9ac2c70a27b01089e69e5e9c53e45c9
  go.mod: 04ef68dad11bfb12214596e31e64671bb0de19cab1d67d779d286acf6c3e712e
  ohos-x86_64-clang: 6c81ae1e052479d1189a5293917b3861640a755319f4412a249e7af6567fa6f6
  probe.go: 55dd7bc85b445713998f7af3d3fdd3be425f797353585fa5728a558d73825aac
  source_tree_aggregate: daff7d0ff314df17662033b24d19cbe60e0643eef2b1c17160518d2448fd13a3
artifact_sha256:
  libgoprobe.so (Go 1.26.0 research): 732fd6c6bdeaa8d5622879576f2a8863db962f1c04435bdba8d3881977097c15
raw_log_reference: docs/evidence/raw/RS-E1-GO126ELF-20260809-0001.log (repository access; full commands
  and key readelf output; sha256 见下)
raw_log_sha256: 8e2df3539b3137cd4badf58753816690aaa784116c4c767630f0592b1464d08f
verdict: research-only comparison (no pass/fail gate); ELF structural facts same, artifact bytes changed
reviewer: anthropic/claude-sonnet-5 independent research-integrity review
reviewed_at: 2026-08-09T19:03:16+08:00
review_record: 初审 M1（clearenv 归因缺差集）整改后复审 0 blocker/0 major/0 minor；formal-hash 匹配的 Go 1.25.12 完整 49-entry 与 Go 1.26 50-entry 独立差集确认 only Go 1.26 clearenv、Go 1.25 empty、共同 49 顺序一致；TLS 核心与边界一致；reviewed-pass 仅 research 记录审查通过，verdict 仍 research-only 非 gate
```

## Method

1. 确认 `/home/worker/.local/go/bin/go` 为 `go version go1.26.0 linux/amd64`（严格匹配）。
2. 用 `git archive 34d512541ca8047f8e3796abd6d85ef94cc13559 spikes/r1-api24-hap/go-probe | tar -x -C /tmp/rs-e1-go126-20260809.TyZ91K` 解出与 0003 相同的冻结 go-probe 源码（任务 brief 提供的 `34d51252d370e2358535665604554762bdf0e3b6` 在仓库中不存在，未使用；0003 runner 常量 `SNAPSHOT_COMMIT=34d512541ca8047f8e3796abd6d85ef94cc13559` 即 0003 实际使用的同一冻结 snapshot）。
3. 复刻 0003 runner 的 stock c-shared 构建环境/命令（先读 `e1-stock-go-replay.sh` 与 `go-probe/build.sh` 确认）：`HARMONYOS_NATIVE_HOME`、`GO_TOOLCHAIN_MODE=local`、`GO_PROBE_OUTPUT_DIR`（指向 /tmp）、`GO_BIN`（换成 Go 1.26.0）；`build.sh` 内部 `GOTOOLCHAIN=local GOOS=linux GOARCH=amd64 GOAMD64=v1 CGO_ENABLED=1 CC=$script_dir/ohos-x86_64-clang $GO_BIN -C $script_dir build -trimpath -buildmode=c-shared -o $GO_PROBE_OUTPUT_DIR/libgoprobe.so .`。唯一环境适配：`GO_EXPECTED_VERSION_PREFIX="go version go1.26.0 "`（build.sh 显式支持该变量，默认值 go1.25.12；不改源码）。
4. 收集真实命令输出：date、cwd、source commit、go version/env、源文件 sha256、build command/RC、ELF sha256/file、`readelf -lW/-dW/-rW/-sW` 关键输出。
5. 对照 0003 formal Go 1.25.12 事实：本地 0003 artifact（`spikes/r1-api24-hap/entry/libs/x86_64/libgoprobe.so`）sha256 与 0003 formal 记录精确匹配（`64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3`），按 brief 规则做只读复核；formal transcript 只含验证 pass 行，不含完整 `readelf -rW` 输出，TPOFF64/res_search 行来自该只读复核。
6. 删除 `/tmp/rs-e1-go126-20260809.TyZ91K` 并确认无残留。

## Measured result（Go 1.26.0 research artifact）

- **Build**：`BUILD_RC=0`，`built .../libgoprobe.so with go version go1.26.0 linux/amd64`。
- **Artifact**：sha256 `732fd6c6bdeaa8d5622879576f2a8863db962f1c04435bdba8d3881977097c15`；`file`：`ELF 64-bit LSB shared object, x86-64, version 1 (SYSV), dynamically linked, Go BuildID=Sh6RjIKbtw81JIU0_X0o/Ejcdu-fY8KHnWkQ_Htco/8JuY3WY1GKDBVTIaUajl/r7GQP8DfOkfZGVKP-j43, BuildID[sha1]=2e2f44591ec7812d904779582407cd97a71ce799, with debug_info, not stripped`。
- **PT_TLS**：present（`TLS 0x1ec0c0 0x00000000001ed0c0 0x00000000001ed0c0 0x000000 0x000008 R 0x8`；`.tbss` segment 05）。
- **STATIC_TLS**：present（`FLAGS: SYMBOLIC BIND_NOW STATIC_TLS`；`FLAGS_1: NOW NODELETE`；`NEEDED: libc.so`）。
- **TPOFF64**：count=1，完整行 `000000000022a078  0000000000000012 R_X86_64_TPOFF64                          0`（符号索引 0 的未命名重定位）。
- **res_search**：`000000000022a220  0000003300000007 R_X86_64_JUMP_SLOT     0000000000000000 res_search + 0`（JUMP_SLOT，非 TPOFF64）。
- **pthread_create**：`000000000022a0f0  0000000d00000007 R_X86_64_JUMP_SLOT     0000000000000000 pthread_create + 0`。
- **Go 导出**：`Hello`/`RuntimeProbe`/`NetDialProbe` 及 `_cgoexp_*` 均 present（GLOBAL DEFAULT FUNC）。
- **.rela.plt**：50 entries（Go 1.25.12 为 49；Go 1.26.0 新增 `clearenv`）。

## Go 1.25.12 对照（0003 formal + 本地只读复核）

| 事实 | 0003 formal Go 1.25.12 | Go 1.26.0 research | 结论 |
| --- | --- | --- | --- |
| artifact sha256 | `64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3`（formal 记录；本地 artifact 精确匹配） | `732fd6c6bdeaa8d5622879576f2a8863db962f1c04435bdba8d3881977097c15` | **changed**（预期，Go 版本不同） |
| file BuildID | `sPHadNzItVcWqoVo81Ly/...` / sha1 `8193c9e5...` | `Sh6RjIKbtw81JIU0_X0o/...` / sha1 `2e2f4459...` | changed（预期） |
| PT_TLS | present（`TLS 0x132480 ... 0x000008 R 0x8`） | present（`TLS 0x1ec0c0 ... 0x000008 R 0x8`） | **same** |
| STATIC_TLS | present（`SYMBOLIC BIND_NOW STATIC_TLS`） | present（`SYMBOLIC BIND_NOW STATIC_TLS`） | **same** |
| NEEDED / FLAGS_1 | libc.so / NOW NODELETE | libc.so / NOW NODELETE | same |
| TPOFF64 count | 1 | 1 | **same** |
| TPOFF64 行 | `00000000002096d8 0000000000000012 R_X86_64_TPOFF64 0` | `000000000022a078 0000000000000012 R_X86_64_TPOFF64 0` | same 结构（offset 不同，符号索引均 0） |
| res_search 重定位 | `0000000000209878 0000003200000007 R_X86_64_JUMP_SLOT res_search + 0` | `000000000022a220 0000003300000007 R_X86_64_JUMP_SLOT res_search + 0` | same 类型（JUMP_SLOT；符号索引 0x32→0x33） |
| pthread_create 重定位 | `0000000000209748 0000000c00000007 R_X86_64_JUMP_SLOT pthread_create + 0` | `000000000022a0f0 0000000d00000007 R_X86_64_JUMP_SLOT pthread_create + 0` | same 类型 |
| Go 导出符号 | Hello/RuntimeProbe/NetDialProbe present | Hello/RuntimeProbe/NetDialProbe present | same |
| .rela.plt entries | 49 | 50（新增 clearenv） | minor changed |

**结论**：Go 1.26.0 stock c-shared 构建的 `libgoprobe.so` 与 0003 Go 1.25.12 构建在**关键 ELF 结构上 same**（PT_TLS present、STATIC_TLS present、TPOFF64 count=1、res_search/pthread_create 均为 JUMP_SLOT、Go 导出符号 present），artifact 字节级 **changed**（sha256 不同，预期因 Go 版本不同）。该结构一致性是 host-only 观察，**不构成 E1 pass**，guest loader 行为未测量。

## 与 brief 提供事实的差异（如实记录）

- **brief artifact sha**：brief 引用 0003 artifact sha `64e0872b8b19d2918310560a18c0c64c078e58cfc3e1f0cbdc75215b7a6cd529`，与 0003 formal 记录（`64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3`）**不符**。按 brief 规则「不匹配则只引用 formal transcript，不能拿可变 artifact 作正式事实」，本记录以 formal 记录为准；本地 0003 artifact 与 formal 记录精确匹配，故其只读复核可作为对照基准，brief 提供的 hash 不作为正式事实。
- **brief snapshot hash**：brief 引用 snapshot `34d51252d370e2358535665604554762bdf0e3b6`，在仓库中**不存在**（`git cat-file` bad object）；0003 实际使用的同一冻结 snapshot 为 `34d512541ca8047f8e3796abd6d85ef94cc13559`（runner 常量），本记录使用后者。
- **brief「精确 res_search TPOFF64 行」**：0003 formal transcript 不含完整 `readelf -rW` 输出；本地只读复核显示 0003 artifact 中 res_search 是 **R_X86_64_JUMP_SLOT**（非 TPOFF64），唯一 `R_X86_64_TPOFF64` 是符号索引 0 的未命名重定位。按实测记录，不按 brief 描述改写。

## 边界 / 不影响 E1 / E8

- 本记录是 host-only research：无 Emulator、无 hdc、无 guest loader 运行、无 HAP 构建/安装。
- **不构成 E1 pass**：E1 overall Go 状态不变（仍无 pass，0003 保持 `reviewed-pass/blocked`）。
- **E8 不变**：`e8_status=CLOSED` 保持。
- 未修改 spike 源码、runner、正式 evidence 或任何 `docs/evidence/` 既有文件；仅新增本记录与 raw log，并在 `docs/README.md` 增加一条 research 索引。
- **Schema 关系**：`RS-` 前缀、`research_id` 与自由文本 `verdict` 是有意的 research-only 扩展，不进入 EV 证据 ID / 门判定机读聚合；不构成 E1 pass / E8 变化。

## Cleanup

- 构建目录 `/tmp/rs-e1-go126-20260809.TyZ91K` 已删除；删除后确认无残留（见下）。
- 未创建任何 Emulator/HDC 相关进程或端口；无 guest 侧残留。
- **post-run 复核**（主运行结束后独立执行，非原运行即时输出）：`2026-08-09T18:52:13+08:00` 执行 `date --iso-8601=seconds; test -e /tmp/rs-e1-go126-20260809.TyZ91K`，结果 `absent`（目录不存在，无残留）；已追加为 raw log `=== 12. post-run cleanup verification ===`。

## 验证

- `markdownlint-cli2`：本文档与 `docs/README.md` 0 issues（MD013 关闭，见 `.markdownlint-cli2.jsonc`）。raw log `RS-E1-GO126ELF-20260809-0001.log` 报 MD041/MD009/MD037（首行非 H1、readelf 真实输出的行尾对齐空格、符号名下划线 `__` 被当作强调标记）；这与既有 raw log 同类（如 `EV-E1-EMU24-20260809-0003-transcript.log` 同样报 MD010/MD009），raw log 是命令输出日志，保持真实输出不改写。
- post-run 复核（`2026-08-09T18:52:13+08:00`，`test -e /tmp/rs-e1-go126-20260809.TyZ91K` → `absent`）已追加进 raw log `=== 12. post-run cleanup verification ===`，raw log sha256 已重算并更新为 `1a8c31fd9cab69d7471db413db2008ec1def3b34af94594d94da0e2ce036e0c9`。
- **M1 终审整改**（`2026-08-09T18:59:11+0800`，post-run review remediation，非原运行输出）：本地 0003 artifact sha256 精确等于 formal 记录 `64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3`；已读取其完整 `.rela.plt` 49 entries 并与 raw 中 Go 1.26.0 的 50 entries 做集合/顺序差集：**only-in-Go1.26 = `clearenv`，only-in-Go1.25 = 空，公共 49 符号相对顺序一致**（clearenv 插在第 5 位，`__deregister_frame_info` 之后、`fwrite` 之前）。差集精确匹配既有归因「Go 1.26.0 新增 clearenv」，归因保留并已由完整 49-entry dump + 差集支撑；完整命令、49 entries、差集结果已追加为 raw log `=== 13. post-run M1 remediation: full 0003 .rela.plt diff ===`，raw log sha256 重算并更新为 `8e2df3539b3137cd4badf58753816690aaa784116c4c767630f0592b1464d08f`。`record_status` 已升级为 `reviewed-pass`（独立复审通过，见 yaml `review_record`）。
- `git diff --check` 通过（无空白错误）。
- `git status` 仅含 3 个变更：`docs/README.md`（+1 条 research 索引）、新增 research md 与 raw log；未触碰 spike 源码、runner 或任何既有 evidence 文件。
- raw log 与本文档已 `read` 复核落盘。
