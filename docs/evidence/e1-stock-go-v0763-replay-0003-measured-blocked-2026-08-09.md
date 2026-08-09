# E1 v0.76.3 stock Go 重放 EV-E1-EMU24-20260809-0003 measured-blocked 记录

最后核验：2026-08-09

本文按[证据与脱敏 Schema](../evidence-schema.md)登记 `e1-stock-go-replay.sh` 完整模式第三次真实执行 `EV-E1-EMU24-20260809-0003` 的**完整测量**证据。这是修复 0001/0002 两处 runner defect 后首次跑通全流程（构建 → ELF 验证 → HAP 构建 → 安装 → `aa test` → 定向 HiLog → 判定 → 清理），在真实 API 24 x86_64 phone Emulator 上测得 stock Go 1.25.12 loader 的**精确拒绝**，按预期判定为 `measured blocked`（**不是 runner failure**）。

## 结论边界

- **ID 已消耗**：`EV-E1-EMU24-20260809-0003` 已被本次完整执行占用，**禁止同 ID 重跑**（no-clobber 也会以 `REFUSE_OVERWRITE` 拒绝，exit `2`）；下一次唯一正式 ID 为 `EV-E1-EMU24-20260809-0004`（若主会话未来批准新重验再分配；当前不预分配）。
- **measured blocked，不是 runner failure**：全流程完成、exit 0；`MEASURED_VERDICT=blocked`、`VERDICT=blocked`、`RECORD_STATUS=collected`（运行结束时 runner sealed 的 pre-review 状态，formal raw 原样保留、不改写；本记录经终审后记录级 `record_status` 为 `reviewed-pass`）。blocked 的精确内容是：guest loader 在 `dlopen` 阶段拒绝 stock Go 1.25.12 `libgoprobe.so`，错误为 `Error relocating .../libgoprobe.so: res_search: initial-exec TLS resolves to dynamic definition`（`loaderErrno=2`）。这是**预期**的 stock Go loader 拒绝（runner 固定期望短语 `initial-exec TLS resolves to dynamic definition` 命中），不是 runner 缺陷。
- **真实 Emulator、非物理设备**：`PHYSICAL_DEVICE_USED=false`、`HDC_RUN=true`；runner 全程只操作固定 Emulator 实例 `netbird_api24_phone`（HDC target `127.0.0.1:10000`），未触碰任何物理设备；E3 物理设备禁 HDC 约束不变。
- **baseline 在线核验 pass**：`BASELINE_VERIFY=pass mode=gh tag=v0.76.3 commit=f65f7b347ee4e7de6d98c488d3d894cd018b02b6`；go.mod 精确行 `go 1.25.5` 与 `toolchain go1.25.12` 均在 baseline commit 处命中（见 baseline-verify raw 与带外 baseline postmortem）。
- **aa 内部 ResultCode 1 但 host rc 0**：`AA_TEST_RC=0`（runner 侧 `aa test` 命令退出码 0），而 `aa-test.log` 内 `TestFinished-ResultCode: 1` 是 **guest TestRunner 内部结果码**（表示测试用例判定失败，即 GO_SPIKE_RESULT=FAIL 的载体），不是 host 命令失败；runner 以 `BASELINE_RESULT` + `GO_SPIKE_RESULT` 判定，`BASELINE_JUDGMENT=pass`、`MEASURED_VERDICT=blocked`。
- **清理与残留**：`CLEANUP_BEGIN=teardown`、`CLEANUP_STAGING=skipped-emulator-not-started`、`CLEANUP_HDC=kill-issued`、`CLEANUP_TEMP=removed`、`CLEANUP_END=teardown-complete`；`FINAL_RESIDUAL_PROCESS=false`、`FINAL_RESIDUAL_PORT=false`；Emulator 已停止（`Stop emulator netbird_api24_phone successfully`）。
- **E8 保持 `CLOSED`**：`e8_status=CLOSED`（manifest）。本记录是 E1 的 measured blocked 证据，**不**构成 E1 pass，也不改变 E8 状态；E1 overall Go 仍无 pass。
- **证据局限与带外补充**：本记录只证明 0003 在精确目标元组上的 measured blocked；九份 raw 文件保留原样（不改写、不删除、不重命名）。两份带外补充文件 `EV-E1-EMU24-20260809-0003-qemu-boot-section.log`（QEMU boot 区间摘录）与 `EV-E1-EMU24-20260809-0003-baseline-postmortem.log`（baseline 公开 API 复核）是 **formal run 之后生成的 out-of-band 材料，不属于原运行输出**，不来自 formal transcript；它们只覆盖 formal run 原始输出中可复核缺口的补充（QEMU boot 原始行、baseline 公开事实），不改变任何 formal 判定。
- **作者自检摘要**：本记录已做一次作者自检（见文末「作者自检摘要」），结论为 **0 blocker**、证据有效；正式双路终审已完成，见 [终审记录](e1-stock-go-v0763-replay-0003-review-2026-08-09.md)（REV-E1-EMU24-20260809-0003，0 blocker/0 major）。补充前发现的 QEMU/baseline 可复核缺口（formal raw 未含 QEMU boot 原始行、baseline 仅一行 pass 摘要）已由两份 post-run 材料覆盖。

## 证据记录

```yaml
evidence_id: EV-E1-EMU24-20260809-0003
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E1
related_stages_or_gates: [E8, R1, R2]
execution: live-full-replay-measured-blocked
target_tuple:
  distribution: HarmonyOS 6.1.1(24) (Emulator image; guest software 6.1.0.125 per frozen Emulator image metadata, not separately queried in 0003)
  device: Emulator instance netbird_api24_phone (phone profile; NOT a physical device)
  full_system_version: HarmonyOS 6.1.1(24) / software 6.1.0.125 (frozen Emulator image metadata; not separately queried in 0003)
  architecture: x86_64 (guest); host x86_64 Debian
  sdk_api_syscap: API 24 (SDK/API runtime surface exercised via installed HAP + aa test)
  channel: unsigned debug HAPs; no signing or distribution input
code_sha: c6b6c12b46aa8369a4c6b8e6a55bdc5eeb4e0b4e (repository HEAD at run time)
upstream_sha: NetBird v0.76.3 commit f65f7b347ee4e7de6d98c488d3d894cd018b02b6 (verified via gh API during run, see baseline-verify log; go.mod `go 1.25.5` / `toolchain go1.25.12` exact lines verified)
snapshot_sha: 34d512541ca8047f8e3796abd6d85ef94cc13559 (runGoProbe HAP probe source snapshot, git archive verified)
toolchain: Debian worker with Command Line Tools 6.1.1.290 / Beta 26.0.0.461 / Go 1.25.12 / ffmpeg / KVM Emulator; all HOST_CHECK passed (hvigorw, ohpm, hvigor-ohos-plugin, emulator, hdc, go, ohos-x86_64-clang, ffmpeg, gh, bash, readelf, file, unzip, ss, git, tar, base64, timeout, pgrep, mktemp, sha256sum, awk; shellcheck absent -> external bash -n required)
working_directory: [WORKSPACE]/spikes/r1-api24-hap
command: EVIDENCE_ID=EV-E1-EMU24-20260809-0003 bash spikes/r1-api24-hap/e1-stock-go-replay.sh
input: fixed baseline v0.76.3 / f65f7b347ee4e7de6d98c488d3d894cd018b02b6 / go 1.25.5 / toolchain go1.25.12; snapshot commit 34d512541ca8047f8e3796abd6d85ef94cc13559; fixed Emulator target 127.0.0.1:10000; no PHYS_1_TARGET, no non-loopback target; default EVIDENCE_ROOT docs/evidence/raw
expected: full replay: stock Go 1.25.12 libgoprobe.so build + ELF verification + snapshot HAP build + dual-HAP install + aa test + directed HiLog + judgment + cleanup; stock loader rejection expected as measured blocked (expected phrase `initial-exec TLS resolves to dynamic definition`)
actual: full replay completed, exit 0; GO_BUILD_VERDICT=pass (go1.25.12 linux/amd64), GO_SO_ELF_VERIFY/PT_TLS/STATIC_TLS/TPOFF64/EXPORT all pass, GO_SO_SHA256=64e0872b…; snapshot extract + lock verify pass; HAP build pass (app/test members byte-equal to built libgoprobe.so, HAP_MEMBER_IDENTITY=pass); Emulator cold boot + HDC connect (attempt 9) + guest boot complete (attempt 27) pass; dual-HAP install pass; aa test ran (AA_TEST_RC=0; guest TestFinished-ResultCode: 1); GO_SPIKE_RESULT|verdict=FAIL|stage=dlopen|loaderErrno=2|loaderError=Error relocating …/libgoprobe.so: res_search: initial-exec TLS resolves to dynamic definition; BASELINE_RESULT|functional=PASS|ping=pong; BASELINE_JUDGMENT=pass; MEASURED_VERDICT=blocked; cleanup complete, FINAL_RESIDUAL_PROCESS=false, FINAL_RESIDUAL_PORT=false; e8_status=CLOSED
started_at: 2026-08-09T17:41:17+08:00
ended_at: 2026-08-09T17:43:10+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds
artifact_sha256:
  libgoprobe.so (stock Go 1.25.12 built input): 64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3
  app_hap (entry-default-unsigned.hap): deb6ee622a5ec05eed86e422f1ceed2700389d345a15a835d0a1aa19bd98e9f3
  test_hap (entry-ohosTest-unsigned.hap): 4df08549874e13063c2081cd9632c27a5976aad0d0b4cab36452f5faa26bf918
raw_log_reference: docs/evidence/raw/EV-E1-EMU24-20260809-0003-{transcript,aa-test,baseline-verify,build,go-build,hilog-tag,hilog-app-full,emulator-console}.log and docs/evidence/raw/EV-E1-EMU24-20260809-0003-manifest.txt (repository access; formal run outputs, byte-identical to the run; manifest is the runner-sealed `.txt`, all other formal files are `.log`); docs/evidence/raw/EV-E1-EMU24-20260809-0003-qemu-boot-section.log and docs/evidence/raw/EV-E1-EMU24-20260809-0003-baseline-postmortem.log (out-of-band post-run supplements generated after the formal run; NOT original run output)
raw_log_sha256:
  transcript: 797882fd814666674474179b636e76ac43a234bba59eda3cfc44e57184b23bec
  manifest: ce5c28d138c00f61aa81b9718b777521cd0f409ed502fb7233ff81bebc874a66 (full file; self-hash semantics below)
  aa_test: 33bc2cbf602541857b1a9de9bc7a53f37e90a7cfae32a28e44f70587539b0500
  baseline_verify: 110c015277c1f9a5a7fcccf8e766b95a182a9ef24773d6bdc6d8bd34fee44849
  build: 218333fbc46f8fcdf1669441c8a44d77d5ec9c1287c8c9ac9e60707f03d05b71
  go_build: a78d5f92aeb6c7142e782b671d58508fced35b35e82e810239321c0ebecd45bc
  hilog_tag: e976a3c41971204a17f7092b3b311b9c675a23bd199b4b888ad4c77454cd9a57
  hilog_app_full: 5f8ade6219c2a64ccbf0243aa4252411761c270888df0054846a64692aa64ef4
  emulator_console: dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e
  qemu_boot_section (out-of-band): ca98a939a905be7bfbbf113d46f6ba58c354a44ac99b1ff8b130df623a6cc52d
  baseline_postmortem (out-of-band): 8a2bba59a430c470db640e9d251020709fbd9dc11054796508261f8cf8bfe2e9
seal_sha256:
  transcript_final_sha256: 797882fd814666674474179b636e76ac43a234bba59eda3cfc44e57184b23bec (manifest line 23; equals full transcript.log hash)
  manifest_sha256: a171d6aa600283f5ed8892655a58a65c96de78cfc0e7036d9599baef3dd4ee0c (manifest self-hash: sha256 of the manifest file after appending transcript_final_sha256 line, before appending manifest_sha256 line, i.e. first 23 lines)
verdict: blocked (measured; expected stock Go 1.25.12 initial-exec TLS loader rejection; NOT a runner failure)
reviewer: anthropic/claude-opus-5 (evidence-integrity) + moonshotai/kimi-k2.7-code (status-consistency); dual independent review 0 blocker / 0 major (see review record REV-E1-EMU24-20260809-0003)
reviewed_at: 2026-08-09T18:28:32+08:00
review_record: REV-E1-EMU24-20260809-0003
```

## 测量事实（transcript 关键行）

```text
GO_BUILD_VERDICT=pass
GO_VERSION_VERIFY=pass version=go version go1.25.12 linux/amd64
GO_SO_ELF_VERIFY=pass
GO_SO_PT_TLS_VERIFY=pass
GO_SO_STATIC_TLS_VERIFY=pass
GO_SO_TPOFF64_VERIFY=pass
GO_SO_EXPORT_VERIFY=pass
GO_SO_SHA256=64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3
SNAPSHOT_EXTRACT_VERIFY=pass
SNAPSHOT_LOCK_VERIFY=pass
HAP_BUILD_VERDICT=pass
APP_MEMBER_SHA256=64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3
TEST_MEMBER_SHA256=64e0872bbad230e345f78f964af4d1ea564f468004e5f22e8ce2796cfaaccfe3
HAP_MEMBER_IDENTITY=pass
CONNECTIVITY_VERDICT=pass
READINESS_VERDICT=pass
INSTALL_VERDICT=pass
AA_TEST_RC=0
BASELINE_JUDGMENT=pass
MEASURED_VERDICT=blocked
MEASURED_VERDICT_NOTE=expected stock Go 1.25.12 initial-exec TLS loader rejection; measured blocked, not runner failure
FINAL_RESIDUAL_PROCESS=false
FINAL_RESIDUAL_PORT=false
VERDICT=blocked
RECORD_STATUS=collected
CLEANUP_END=teardown-complete
```

（完整内容见 raw transcript；`baseline-verify.log` 只含 `BASELINE_VERIFY=pass mode=gh tag=v0.76.3 commit=f65f7b347ee4e7de6d98c488d3d894cd018b02b6`。其中 `RECORD_STATUS=collected` 是 runner 在运行结束时 sealed 的 pre-review 状态，formal raw 原样保留、不改写；本记录经终审 REV-E1-EMU24-20260809-0003 后，记录级 `record_status` 为 `reviewed-pass`，`verdict` 仍为 `blocked`。）

## GO loader 精确拒绝（aa-test / hilog-tag）

`aa-test.log` 与 `hilog-tag.log` 中的判定来源行：

```text
GO_SPIKE_RESULT|verdict=FAIL|ok=false|pid=2814|preCreatedBeforeDlopen=true|dlopenLoaded=false|postCreatedAfterDlopen=false|pre=false|post=false|stage=dlopen|loaderErrno=2|loaderError=Error relocating /data/storage/el1/bundle/libs/x86_64/libgoprobe.so: res_search: initial-exec TLS resolves to dynamic definition in /data/storage/el1/bundle/libs/x86_64/libgoprobe.so|detail=dlopen libgoprobe.so failed dlerror=Error relocating /data/storage/el1/bundle/libs/x86_64/libgoprobe.so: res_search: initial-exec TLS resolves to dynamic definition in /data/storage/el1/bundle/libs/x86_64/libgoprobe.so; errno=2 (No such file or directory)
TestFinished-ResultCode: 1
```

- `stage=dlopen`、`dlopenLoaded=false`：guest loader 在 `dlopen` 阶段拒绝，未进入任何 Go runtime 初始化。
- 拒绝短语 `res_search: initial-exec TLS resolves to dynamic definition` 精确命中 runner 固定期望短语 `initial-exec TLS resolves to dynamic definition` → `MEASURED_VERDICT=blocked`（预期行为，非 runner failure）。
- `BASELINE_RESULT|functional=PASS|ping=pong|version=r1-api24-probe/0.0.1`：普通 ArkTS/native baseline 探针在 guest 正常通过，证明 guest 环境本身健康，拒绝仅发生在 stock Go 库加载路径。
- `AA_TEST_RC=0`（host 侧 `aa test` 命令退出码 0）与 `TestFinished-ResultCode: 1`（guest TestRunner 内部结果码，表示用例判定失败）是两层不同语义：host 命令成功执行完毕，guest 用例按预期判定失败；runner 以 `BASELINE_RESULT` + `GO_SPIKE_RESULT` 行判定，不依赖 TestRunner 结果码。

## manifest 自 hash 语义

manifest 末尾两行由 runner seal 追加（`seal_and_finalize`）：

```text
transcript_final_sha256=797882fd814666674474179b636e76ac43a234bba59eda3cfc44e57184b23bec
manifest_sha256=a171d6aa600283f5ed8892655a58a65c96de78cfc0e7036d9599baef3dd4ee0c
```

- `transcript_final_sha256` = 最终 transcript.log 的 sha256（teardown 输出已 flush 后计算）= `797882fd…`，与 raw transcript 文件 hash 一致。
- `manifest_sha256` = **追加 `transcript_final_sha256` 行之后、追加 `manifest_sha256` 行之前**的 manifest 文件 sha256（即前 23 行）= `a171d6aa…`；这是自 hash 语义，不是全文件 hash（全文件 hash 为 `ce5c28d1…`，见上 `raw_log_sha256.manifest`）。

## 带外补充（非 formal run 原始输出）

两份补充文件在 formal run 之后生成，**不属于原运行输出**，不来自 formal transcript；它们只覆盖 formal raw 中可复核缺口的补充，不改变任何 formal 判定：

- `docs/evidence/raw/EV-E1-EMU24-20260809-0003-qemu-boot-section.log`：QEMU boot 区间摘录（OUT-OF-BAND POST-RUN EXCERPT）。源文件 `/home/worker/harmonyos/emulator-instances/netbird_api24_phone/Log/qemu.log`（提取时 sha256 `085fe3dcea6d75814c1789e16966dedefe6bdb039dad9bf3a9880f22a35c34f6`），行范围 33670–33955（286 行，逐字复制，无区间外数据）；起始行 33670 与 transcript 的 `QEMU_CURRENT_BOOT_START_LINE=33670` 一致，boot complete 行 33751（`guest os boot completed.` 17:43:01），结束行 33955（Emulator 停止 17:43:09）。该文件覆盖 formal raw 未含的 QEMU boot 原始行缺口。
- `docs/evidence/raw/EV-E1-EMU24-20260809-0003-baseline-postmortem.log`：baseline 公开 API 复核（OUT-OF-BAND POST-RUN）。访问时间 2026-08-09T18:08:00+08:00；通过公开 `gh api` 复核 v0.76.3 tag ref（lightweight tag，ref SHA = `f65f7b34…`）、annotated tag peeled commit（404 → 无 annotated tag 对象，peeled = ref SHA）、go.mod contents（blob sha `f8a1a84b055db7958b2b5e556b31fed68e9d93f9`、size 16055、精确行 `go 1.25.5` 行 3 / `toolchain go1.25.12` 行 5）、release published_at（`2026-08-08T12:11:41Z`）、release run 31256677326（status `completed`、conclusion `success`、head_sha `f65f7b34…`）、go.mod wireguard-go replace 实际 SHA（`github.com/netbirdio/wireguard-go v0.0.0-20260628102922-2834bebf6c1a` → `2834bebf6c1a`）。不含任何 token/secret。该文件覆盖 formal baseline-verify 仅一行 pass 摘要的可复核缺口。

## 作者自检摘要

- 自检方式：只读复核（不运行设备/Emulator/replay、不修改任何文件）。
- 结论：**0 blocker**；证据有效。九份 formal raw 与 manifest 自 hash 语义复核一致；两份带外补充明确标注 OUT-OF-BAND 且不来自 formal transcript；补充前发现的 QEMU/baseline 可复核缺口已由两份 post-run 材料覆盖。
- 边界：本摘要**是作者自检，不是独立审查**；正式双路终审见 [终审记录](e1-stock-go-v0763-replay-0003-review-2026-08-09.md)（REV-E1-EMU24-20260809-0003，`anthropic/claude-opus-5` evidence-integrity + `moonshotai/kimi-k2.7-code` status-consistency，0 blocker/0 major）。本记录不改变 E1 overall Go 状态（仍无 pass），E8 保持 `CLOSED`。

## 边界与后续

- 本记录是 E1 的 measured blocked 证据：v0.76.3 官方 Go 1.25.12 loader/runtime 在 API 24 x86_64 phone Emulator 上被 guest loader 精确拒绝（`initial-exec TLS resolves to dynamic definition`）；**无 E1 pass**；E8 保持 `CLOSED`。
- 本记录 `record_status: reviewed-pass`（终审 REV-E1-EMU24-20260809-0003，0 blocker/0 major）；`verdict: blocked` 是平台测量结论（预期拒绝），不是 runner 失败。`reviewed-pass` 只表示审查完成，**不构成 E1 pass**。
- 九份 raw 文件保持原样，供独立审查核对哈希（见上 `raw_log_sha256` 与 `seal_sha256`）；两份带外补充文件同样保持原样。
- E3 物理设备禁 HDC 约束不变；完整模式也只操作固定 Emulator target `127.0.0.1:10000`。
