# 主机侧诊断构建记录：dirty 工作树离线 HAP（2026-09-30，0001）

* **record id**：`n2h-host-build-dirty-20260930-0001`
* **性质**：主机侧**诊断**构建记录（`is_evidence: false`）。不消费任何 AUTH / campaign /
  evidence id，不签发、不安装、不推送、不跑设备；本文件与同目录 JSON 只记录本轮构建
  与产物可核验事实。
* **仓**：`/home/worker/work/base/netbird-harmonyos`
* **HEAD**：`52b2bac07e91a07af2d2b6ea006e561fdfbb8f6c`（该 commit 仅登记 docs v4.2）
* **构建源**：**未提交的 dirty 工作树**（33 条 `git status --porcelain` 路径：23 改 + 10 新增），
  **不是纯 commit 产物**。HEAD 只决定文档登记，产物字节来自这份工作树。
* **保留**：`git stash` 条目 `stash@{0}: On main: posctl-socket-ctx-wip` 原样保留；
  未做 reset / clean / stash / commit / push。

---

## 1. 命令与真实退出码

```
DEVECO_SDK_HOME=/home/worker/harmonyos/command-line-tools/current bash client/build.sh
```

| 项 | 值 |
| --- | --- |
| 构建开始（UTC） | `2026-09-30T04:30:21Z`（本地 12:30:21） |
| 构建结束（UTC） | `2026-09-30T04:30:48Z` |
| **真实 exit code** | **0** |
| 原始日志（新路径，旧证据未覆盖） | [n2h-host-build-dirty-20260930-0001.build.log](n2h-host-build-dirty-20260930-0001.build.log)（sha256 `4ca52f1fd5a196dd0792c5710781d366eb4730619cf4e32551a563f0805e7153`） |

链内事实（取自日志）：

* `cargo build --release --target aarch64-unknown-linux-ohos --offline --locked` → **exit 0**，
  `netbird_core v0.1.0` **被重新编译**（6 条既有 warning），`Finished release ... in 18.45s`；
  boringtun crate sha256 `15dd6a8a…26939` 命中冻结值；`Cargo.lock` 已存在、checksum 命中；
  14/14 冻结 BoringTun ffi 符号在 dynsym；NEEDED 无 `libc.so.6`。
* `hvigorw --no-daemon assembleHap --mode module -p product=default -p buildMode=release`
  → `BUILD SUCCESSFUL in 4 s 120 ms`；33 tasks：19 executed / 14 up-to-date；
  `PackageHap`、`DoNativeStrip`、`SignHap` 均 executed；`SignHap` 仅打印
  `WARN: No signingConfig found for product default`（**未签名**，HAP 内无 `META-INF/` 条目）。
* `client/build.sh` 第 4 步逐项 `VERIFY OK`（成员 `.so` 摘要、ELF、`pack.info` apiVersion）。

> 注意：本轮 hvigor 的 `CompileArkTS` 为 **UP-TO-DATE（增量，未重新执行）**——打包进本 HAP 的
> `ets/modules.abc` 是同一份 `.ets` 输入的既有编译产物。它不是「本轮重新编译 ArkTS」，
> 但 §5 的 abc 字符串检查证明该字节码确实携带当前壳侧新通道。

---

## 2. 产物：HAP

| 项 | 值 |
| --- | --- |
| 路径 | `client/entry/build/default/outputs/default/entry-default-unsigned.hap` |
| **sha256** | `e12224660b60b7f450054954d2d91766dff826222d228208171f53d4da5fc5f0` |
| **字节数** | **4,869,195** |
| mtime | `2026-09-30 12:30:46.934678933 +0800` |
| 签名 | 无（无签名配置、无 `META-INF/` 条目） |
| 前一占用件（构建前记录，随后被同路径覆盖） | sha256 `a35cbf7c2c6e8418e4e64c2b1f4f1028011840977f3bbb74eba3c8a0af31a6c4`，4,868,411 bytes，mtime 2026-09-30 01:30:40（可重建的构建输出，非封存证据） |

同项目既有口径已记录：HAP 容器**不可逐字节复现**，验收以 exit 0 + 逐项 `VERIFY OK` + **成员 `.so` 摘要**为准。

## 3. 内含 arm64 原生成员与对应原始 `.so`

| 层 | 路径 | sha256 | bytes | ELF |
| --- | --- | --- | --- | --- |
| HAP 内成员（hvigor `DoNativeStrip` 后） | `libs/arm64-v8a/libnetbird_core.so` | `5679ba0ab66ccfdf088a5dda6a3fea6fb8f5cff54f2923ad9b8f0b769a2700cf` | 4,615,352 | ELF64 LE `ET_DYN` `e_machine=183 (AArch64)` |
| 模块目录原始 `.so`（打包输入，未 strip） | `client/entry/libs/arm64-v8a/libnetbird_core.so` | `b9ec630141f463f6d6f6dc9a8cb49c5a4350634d6657971752c5c05264de00c7` | 6,895,432 | ELF64 LE `ET_DYN` AArch64 |
| core release 原始制品 | `client/core/target/aarch64-unknown-linux-ohos/release/libnetbird_core.so` | `b9ec630141f463f6d6f6dc9a8cb49c5a4350634d6657971752c5c05264de00c7` | 6,895,432 | ELF64 LE `ET_DYN` AArch64 |

* 模块 `.so` 与 core release 制品**逐字节相同**（`cp` 直拷）。
* 成员 sha256 == 该原始 `.so` 经 SDK `llvm-strip` 后的 sha256（项目既有校验法）。
* `NEEDED` = `libace_napi.z.so`、`libc.so`、`libhilog_ndk.z.so`；**无** `libc.so.6`（非 glibc）。
* `llvm-readelf -h` 实测：`Class: ELF64` / `Data: little endian` / `Type: DYN` / `Machine: AArch64`。

## 4. 应用包信息（API26）

`pack.info`（HAP 内）摘要模块：

| 项 | 值 |
| --- | --- |
| `apiVersion.target` / `.compatible` | **26 / 26**，`releaseType: Release` |
| bundleName / bundleType / vendor | `cn.alfadb.netbird` / `app` / `alfadb` |
| versionCode / versionName | `1` / `0.0.1` |
| module | `entry`（type `entry`，mainElement `EntryAbility`，package `entry-default`） |
| extensionAbility | `NetBirdVpnExtensionAbility` |
| deviceTypes | `phone` |

`module.json`（HAP 内）另有：`compileSdkVersion 26.0.0.105`、`minAPIVersion 260000026`、
`targetAPIVersion 260000026`、`buildMode release`、`debug false`、`bundleName cn.alfadb.netbird`。
校验法沿用项目口径（`client/build.sh` 第 4 步：HAP 内 `pack.info` apiVersion target/compatible == 26）。

## 5. 新核心入口：源码/exports 契约与构建输入包含

### 5.1 `n2_cd_switch_identified`（切网身份回执）

| 事实 | 位置 |
| --- | --- |
| NAPI 注册 | `client/core/src/napi.rs:142` `reg!("n2_cd_switch_identified", napi_n2_cd_switch_identified)` |
| exports 声明钉 | `client/core/src/napi.rs:434`（钉在 exports 对象上） |
| handler / 参数面 | `client/core/src/napi.rs:841-846`：**8 个导出参数**（4 string + 4 i32）；`Args` 的 `argv` 容量 8（`napi.rs:186`） |
| JSON 包装契约 | `client/core/src/connector.rs:5573` `n2_cd_switch_identified_json(id, verdict, net_after, local_after, net_id_after: i64, identity_fd: i32, identity_dup_fd: i32, identity_generation: i64)`；任一身身份字段 `<0` → `None` → 核心 `unsat|switch-identity-required`（fail-closed，不回退全局计数器） |
| 模块 | `client/core/src/n2_cd.rs`；`pub mod n2_cd;`（`client/core/src/lib.rs:186`） |
| 壳侧驱动 | `client/entry/src/main/ets/vpnextensionability/NetBirdVpnExtensionAbility.ets:2831`（8 参调用；身份三字段缺省 `-1`） |
| 打包成员内字符串 | `n2_cd_switch_identified` = 存在（见 5.3） |

### 5.2 `n2_ring_ack`（显式主机 ACK / session JSON）

| 事实 | 位置 |
| --- | --- |
| NAPI 注册 | `client/core/src/napi.rs:152` `reg!("n2_ring_ack", napi_n2_ring_ack)` |
| exports 声明钉 | `client/core/src/napi.rs:444` |
| handler / 参数面 | `client/core/src/napi.rs:901-903`：**1 个导出参数**（请求文档） |
| JSON 包装契约 | `client/core/src/connector.rs:5761` `n2_ring_ack_json(request_json: &str) -> String` |
| 请求文档 | `{"session":"<arm token>","up_to_seq":<durable seq>}`，**两键都必需**（`client/core/src/n2_ring.rs:759` `parse_ack_request`） |
| 拒绝面 | 解析层 `empty-request` / `request-too-long` / `invalid-json` / `invalid-request` / `invalid-session`；核心绑定层 `not-configured` / `not-armed` / `session-mismatch` / `seq-beyond-tail`（同一把环锁内判定，壳不无条件 ACK） |
| 应答 | `{ok,result,reason,session,up_to_seq,acked_seq,previous_acked_seq,advanced}`；重复 ACK 幂等（`advanced=false`），水位单调不回退 |
| 模块 | `client/core/src/n2_ring.rs`；`pub mod n2_ring;`（`client/core/src/lib.rs:192`） |
| 壳侧 | `NetBirdVpnExtensionAbility.ets:3336` **逐字**转发请求文档给 `core.n2_ring_ack`；`EntryAbility.ets:108` `N2_RING_ACK_PARAM = 'n2_ring_ack'`；`NetBirdN2Ring.ets:218` `parseN2RingAck` |
| 打包成员内字符串 | `n2_ring_ack` = 存在；命名拒绝 token 全部存在（见 5.3） |

### 5.3 构建输入包含（本轮产物的实际字节证据）

* 两个模块都在 crate 的 `src/` 下，因此都在 `build.rs` 的源码自识别摘要输入内
  （`FIXED_INPUTS=[Cargo.toml,Cargo.lock,build.rs]` + `TREE_INPUTS=[proto,src]`）。
* 独立重算（`client/core/build.sh` 第 7 步口径）得
  `N2H_BUILD_SHA256 = edbbbaef183ccb5b2cf607e8fbe516ba1582cac3eedf17b8dace58e0f18f43ce`，
  与核心构建日志一致，且该串**存在于原始 `.so` 与打包成员**内。
* 打包成员内存在：`n2_cd_switch_identified`、`n2_ring_ack`，以及
  `session-mismatch` / `seq-beyond-tail` / `not-armed` / `not-configured` /
  `invalid-session` / `switch-identity-required`。
* HAP 内 `ets/modules.abc`（246,516 bytes）存在：`n2_cd_switch_identified`、`n2_ring_ack`、
  `N2_RING_ACK_PARAM`、`N2_RING_ACK_SHELL`、`n2_cd_identity`。

> 以上只证明**构建输入与产物字节包含**该契约；**不**证明目标平台上的实际运行行为。

## 6. 构建前后源状态：工具链未改源

方法：`git ls-files -co --exclude-standard`（tracked + untracked，忽略目录排除）+ 独立哈希器
对 `git status` / `git diff --stat` 块逐字节比对。**未使用** SDK 工具链里的 `diff`
（该二进制在本机对任意输入恒返回 0，不可信）。

| 检查 | pre | post | 结论 |
| --- | --- | --- | --- |
| `git status --porcelain` 块 sha256（去掉本轮证据文件） | `7e930f46…a7715` | `7e930f46…a7715` | 相同 |
| `git diff --stat` 块 sha256 | `8fd6dbfd…c165e` | `8fd6dbfd…c165e` | 相同 |
| 非证据文件清单 | 956 | 956 | 相同 |
| 非证据源摘要 sha256 | — | `b21118a0351e16f1da54b91061d7239420683963a87f52c72d1e592be8541157` | 记录值 |
| 构建后新增路径 | — | 仅本轮 4 个证据文件 | 无源文件变化 |
| 最新 dirty 源 mtime | `2026-09-30 10:36:13 +0800`（`n2_cd.rs`） | 早于构建 12:30 | 构建未触碰 |

构建只写入被忽略目录（`client/core/target/**`、`client/entry/libs/**`、
`client/entry/build/**`、`.hvigor/**`）。构建生成的必要文件（事实报告）：
`client/entry/libs/arm64-v8a/libnetbird_core.so`（由 core 制品拷贝）与
`client/entry/build/default/outputs/default/entry-default-unsigned.hap`（及其同目录 mapping/symbol/pack.info）。

抓取文件：[prebuild](n2h-host-build-dirty-20260930-0001.prebuild-tree.txt)（sha256 `baf0d8ae…2ba0622`）、
[postbuild](n2h-host-build-dirty-20260930-0001.postbuild-tree.txt)（sha256 `660f8bed…f09af99`）。

## 7. 边界（明确没做的）

* **没有设备安装/运行**：本记录的 API26 只是构建元数据，**不是平台运行验证**；
  目标 ROM 上 `bearerTypes`/link address、`hdc file recv -b` 等仍属未验证。
* **没有签发**：无签名、无 profile、无签名 sidecar、无 AUTH / campaign / evidence id 占用。
* 没有 hdc / ssh / push；没有重跑已通过的 host 测试套件；没有改源码凑构建。
* 本工作树内**未搜到**“0048”草案文本；未改任何授权文件。

## 8. 供 0048 草案更新的精确值

| 用途 | 路径（相对仓根） | sha256 | 字节数 |
| --- | --- | --- | --- |
| 未签名 HAP（本轮） | `client/entry/build/default/outputs/default/entry-default-unsigned.hap` | `e12224660b60b7f450054954d2d91766dff826222d228208171f53d4da5fc5f0` | 4,869,195 |
| HAP 内成员 `.so` | `libs/arm64-v8a/libnetbird_core.so` | `5679ba0ab66ccfdf088a5dda6a3fea6fb8f5cff54f2923ad9b8f0b769a2700cf` | 4,615,352 |
| 打包输入原始 `.so` | `client/entry/libs/arm64-v8a/libnetbird_core.so` | `b9ec630141f463f6d6f6dc9a8cb49c5a4350634d6657971752c5c05264de00c7` | 6,895,432 |
| core release 制品 | `client/core/target/aarch64-unknown-linux-ohos/release/libnetbird_core.so` | `b9ec630141f463f6d6f6dc9a8cb49c5a4350634d6657971752c5c05264de00c7` | 6,895,432 |

构建边界：**dirty=true**，HEAD `52b2bac07e91a07af2d2b6ea006e561fdfbb8f6c`，
33 条脏路径（23 改 + 10 新增），非证据源摘要
`b21118a0351e16f1da54b91061d7239420683963a87f52c72d1e592be8541157`，
`N2H_BUILD_SHA256=edbbbaef183ccb5b2cf607e8fbe516ba1582cac3eedf17b8dace58e0f18f43ce`，
`build exit 0`，未签名，未安装。

## 9. 本轮证据文件

| 文件 | sha256 |
| --- | --- |
| [n2h-host-build-dirty-20260930-0001.build.log](n2h-host-build-dirty-20260930-0001.build.log) | `4ca52f1fd5a196dd0792c5710781d366eb4730619cf4e32551a563f0805e7153` |
| [n2h-host-build-dirty-20260930-0001.verify.log](n2h-host-build-dirty-20260930-0001.verify.log) | `13943dd83d21fc168101ef4247a04c0b3dc3822fdf43d0b38f7aae063f0c2199` |
| [n2h-host-build-dirty-20260930-0001.verify.py](n2h-host-build-dirty-20260930-0001.verify.py) | `9bf3013679897f9138dd972024be787d3f7bc4ffeba880b7c410775726ac6440` |
| [n2h-host-build-dirty-20260930-0001.prebuild-tree.txt](n2h-host-build-dirty-20260930-0001.prebuild-tree.txt) | `baf0d8ae4557a454c66fd56a1a857177d103ebf77b19fb4a96a8d22b82ba0622` |
| [n2h-host-build-dirty-20260930-0001.postbuild-tree.txt](n2h-host-build-dirty-20260930-0001.postbuild-tree.txt) | `660f8bed567632d3fb857714ff8cf144e6ea3a708b309391109256bf3f09af99` |
| [n2h-host-build-dirty-20260930-0001.exit-code](n2h-host-build-dirty-20260930-0001.exit-code) | `9a271f2a916b0b6ee6cecb2426f0b3206ef074578be55d9bc94f6f3fe3ab86aa` |
| 本文件 / JSON | 见同目录 `.sha256` sidecar（写入会改变自身字节） |
