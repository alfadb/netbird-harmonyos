# N1a native data-plane probe — implementation spike

N1a 门（[docs/n1a-gate-plan.md](../../docs/n1a-gate-plan.md)）的数据泵探针**实现**：
在同一 native 库内建立两个互逆 peer 的真实 BoringTun 0.7.1 tunnel 实例，经一对
in-process `127.0.0.1` UDP 回环 socket 做"tun.Device 等效数据泵"——双向握手建立、
逐包加密转发、逐字节完整性、吞吐、背压、tick 路径与资源稳定性，全部经真实 ffi
调用，无任何模拟值。

本 spike 是 **host-only 实现与自测**：`build.sh` 只做双 ABI 构建与 ELF/符号/
checksum 验证；`cargo test` 在 host x86_64-linux-gnu 上跑通完整真实泵（最强自测）。
它**不**启动 Emulator、不跑 HDC、不产 evidence 文件。正式 N1a Emulator 测量由
独立 campaign（判据审查通过后）执行：把 `out/x86_64/libentry.so` 作为 HAP 快照
drop-in、staged 本目录 `napi/` 的测试入口后测量（沿 N0 的
`prepare-hap-snapshot.sh` / `n0-emulator-run.sh` 模式）。

## 与 N0 的关系

| 项 | N0 (`spikes/n0-native-core`) | 本 spike |
| --- | --- | --- |
| BoringTun | 0.7.1，`default-features=false, features=["ffi-bindings"]`，checksum `15dd6a8a…` | **同一 crate、同一 checksum**（build.sh 重复验证） |
| 构建方式 | `--offline --locked`，双 OHOS target，官方 clang 链接 + llvm-ar | 完全相同 |
| Rust C ABI | `n0_probe_version` / `n0_probe_smoke`（keygen/tick 冒烟） | `n1a_probe_version` / `n1a_dataplane_probe` / `n1a_result_free`（全泵） |
| NAPI 薄层 | `runProbe()` → 结构化对象 + `N0_RUNPROBE` marker | `runN1aProbe()` → 结构化对象 + `N1A_RESULT` marker（tag `N1aProbe`，domain 0x2900 沿 N0） |
| ohosTest | `runProbeTest.ets` + runner，marker `N0_CORE_PROBE_RESULT` | `runN1aProbeTest.ets` + runner，marker `N1A_PROBE_TEST_RESULT` |
| n0 目录 | 只读参照；本 spike **未改动** n0 任何文件 | 独立目录 `spikes/n1a-native-dataplane/` |

## Layout

```text
Cargo.toml / Cargo.lock      Rust core crate (libn1acore: cdylib + staticlib);
                             依赖 = boringtun 0.7.1 (ffi-bindings) + libc (C5 setsockopt)
src/pump.rs                  真实数据泵：密钥对→双 tunnel→UDP 回环信道→握手→
                             完整性→tick 间隔→背压→资源快照；结果模型 + JSON 序列化
src/lib.rs                   C ABI：n1a_dataplane_probe() -> *mut n1a_dataplane_result
                             (ok + c1..c9 + 关键计数 + JSON 指针)，n1a_result_free()，
                             n1a_probe_version()；host 单元测试
napi/n1a_overlay.cpp         thinnest NAPI overlay：runN1aProbe() -> 结构化对象 +
                             单行 HiLog marker N1A_RESULT|...（tag N1aProbe，domain 0x2900）
napi/types/                  ArkTS 类型声明（libentry.so）
napi/runN1aProbeTest.ets     ArkTS 测试入口（断言全部子项 pass；C5 not-triggered≠失败；fail-closed）
napi/ohosTest/               ohosTest runner（实际调用 runN1aProbeTest，marker N1A_PROBE_TEST_RESULT）
build.sh                     双 ABI host 构建 + ELF/符号/checksum/快照一致性验证（全 fail-closed）
README.md                    本文件
```

## Build（host-only）

```bash
bash spikes/n1a-native-dataplane/build.sh
```

脚本（全 fail-closed，与 N0 build.sh 同构）：

1. 校验官方 OHOS clang/sysroot、`llvm-ar` 与两个 rustup OHOS target；
2. 校验 BoringTun 0.7.1 crate checksum（`15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2
   ecea4aed83ffb9f99f7126939`）对 cargo cache 与 `Cargo.lock` 双重一致；
3. `cargo build --release --offline --locked` 构建 `x86_64-unknown-linux-ohos` 与
   `aarch64-unknown-linux-ohos`（linker = 官方 OHOS clang，`AR_*` = 官方 llvm-ar）；
4. 用 OHOS clang++ 链两个 ABI 的 `libentry.so`（静态 `libn1acore.a`，
   `--whole-archive`，`-lace_napi.z -lhilog_ndk.z`）并 strip；
5. ELF 验证：架构匹配、`DT_NEEDED` 无 `libc.so.6`（glibc）、有 `libc.so`；
6. 符号验证：`n1a_probe_version` / `n1a_dataplane_probe` / `n1a_result_free` +
   BoringTun 全量数据面 C ABI（`x25519_*`、`new_tunnel`、`tunnel_free`、
   `wireguard_write/read/tick/force_handshake/stats`）；
7. 产物 sha256 清单 + HAP 快照字节一致性断言。

输出：

```text
out/x86_64/libentry.so            x86_64 overlay（Emulator 门 ABI）
out/aarch64/libentry.so           aarch64 overlay（仅 cross-compile，无加载主张）
out/hap-snapshot/x86_64/          libentry.so + libn1acore.a + libn1acore.so
target/<target>/release/          Rust core 产物
```

## 自测（host x86_64-linux-gnu，真实全泵）

```bash
cd spikes/n1a-native-dataplane
cargo test --offline --locked                          # 全部判据级断言
cargo test --offline --locked -- --test-threads=1 --nocapture   # 串行 + 打印测量 JSON
```

`full_pump_probe_passes_on_host` 在 host 上跑完整真实泵并断言：C2 握手建立
（stats 观测 + 首轮 write→read）、C3 每方向 10×200×1024 逐字节相等且零丢失/
零多余 + tx/rx 字节账精确相等、C4 ≥ 5 MiB/s、C5 不死锁且已收包零损坏
（induced 仅由 errno 判定）、C6 keep_alive=1 下 ≥3 个 ≥1s 间隔的真实 persistent
keepalive tick、C7/C8 探针自有资源门（逐 fd fcntl + `tunnel_free`×2；r3）。
参考测量（host，2026-08-31，串行）：4000/4000 包逐字节验证，吞吐 ≈ 47 MiB/s，
泵送 ≈ 83 ms；字节账四项 delta 全部 = 2,048,000（每方向）；背压 **induced=false**
（0 次 EAGAIN/ENOBUFS → 如实 `not-triggered`），`so_rcvbuf=8192` 下 512 包突发
观察到 43,435 次内核丢包（**纯观察字段**，不参与 induced 判定；171 轮有界重传
后 512/512 完整送达，0 损坏，0 重复）；C6 真实 keepalive 6 次（≥3）；fd/线程
快照精确相等（4→4、2→2）。判据断言必须串行运行：
`cargo test --offline --locked -- --test-threads=1`（判据不因 host 测试便利放宽）。

## 判据映射（C1-C9，判据文本以 docs/n1a-gate-plan.md 为准）

| # | 判据 | 本实现的观测点 |
| --- | --- | --- |
| C1 | 库加载 | 探针体一旦执行即证明 native 成员已 dlopen、C ABI 可调用；权威证明是 Emulator 侧 marker/ohosTest 完成（加载失败则永远无结果，上游判 fail） |
| C2 | 握手建立 | 双向 `wireguard_force_handshake` + 信道搬运（init→response→keepalive 四腿）后，`wireguard_stats().time_since_last_handshake >= 0`（0.7.1 stats 无 state 字段——该观测已由判据三轮审查**钉死为冻结文本本身**，非实现自选）；仅 A 侧 initiation，四腿握手 + WRITE_TO_NETWORK 强制 drain；另做握手后首轮 write→信道→read 逐字节证明。时间盒 30 s，超时/错误 → fail（非 blocked） |
| C3 | 逐包完整性 | 每方向 10 轮 × 200 包 × 1024 B，合成包为合法 IPv4（真实头校验和）+ `(direction, round, seq)` BE 标记 + xorshift 填充；逐包 ping-pong、全 1024 字节相等；计数零丢失/零多余/零损坏 |
| C4 | 吞吐下限 | 双向合计已验证明文字节（4,000,000 B）÷ 纯泵送窗口（仅数据阶段计时，不含握手/tick/背压）≥ 5 MiB/s |
| C5 | 背压 | 发送端 SO_SNDBUF=4096、接收端 SO_RCVBUF=4096（getsockopt 回读记录）+ 512 包突发；**induced 仅由 `errno∈{EAGAIN,ENOBUFS}` 判定**（sendto 真实返回）；内核满队列静默丢包按精确账目记入 `kernel_queue_drops` **纯观察字段**、永不参与 induced 判定；有界重传轮在时间盒 10 s 内完成全部轮次、已收包零丢失/零损坏 → pass-induced；**零次 errno → 如实 `not-triggered`（≠ fail，不阻止 overall pass，不得写成背压已验证）**。UDP 数据报语义无部分写，记 0 并注明 |
| C6 | tick 路径 | 两侧 `new_tunnel(..., keep_alive=1, ...)`；≥3 个无数据静默间隔（各 ≥1s，实现 1100 ms）每侧各调一次真实 `wireguard_tick`；探针计数器记录 `op==WRITE_TO_NETWORK` 的 tick（≥3 次，persistent keepalive 真实经信道送对端、对端 `WIREGUARD_DONE` 消化）；每次间隔后 stats 仍为已建立；随后 8×2 包突发证明会话仍可载业务 |
| C7 | 资源稳定（r3） | 同一 native 调用内 T0/T3 时刻不变。**门控对象仅为探针自有资源**：(1) 两个回环 socket 的 fd 号在创建时记录，T3 逐个 `fcntl(F_GETFD)`——EBADF=已关=pass，任一仍开=fail（先于 fd-set 快照执行，防 readdir fd 号复用污染门）；(2) 静态无线程创建（build.sh `STATIC_NO_PTHREAD` 断言源码树 0 命中禁列 API；新 TID 只观察）；(3) 进程级 fd/task/RSS 为 JSON 观察字段（含 `process_model: testrunner\|entryability\|unknown` 标注），**禁止**作为 pass/fail 门（r3 裁决：aa-test 环境本质噪声敏感）。r3 C7/C8 证据经短 marker `N1A_RES\|c7=<pass\|fail>\|c8=<pass\|fail>\|fd2=<closed\|open>\|fdset=<n>` + NAPI 标量（fd2Closed/fdSetDiffCount/newTidsObserved/tunnelsFreed/processModel）进入证据（不依赖 hilog 截断的单行 detailJson） |
| C8 | 清理（r3） | `tunnel_free`×2（各恰好一次，Tunnel Drop 保证）+ C7(1) 逐 fd 核对通过；不再使用进程级 fd 回 T0 作门 |
| C9 | 结果页 | 恰好一行 C9 钉死四字段 marker `N1A_RESULT\|verdict=<PASS\|FAIL>\|c5=<induced\|not-triggered\|fail>\|throughput_mibps=<x.xx>`（其余字段只进结构化对象/JSON，不得事后升格为门）；页面截图非空（沿 E2 YAVG/非黑帧）且页上 PASS/FAIL 与 marker 一致；缺/重复/不一致 → fail。**结果页由 Phase B 普通入口渲染**（`aa start -a EntryAbility`，force-stop 后独立 UIAbility 进程；两次完整探针、两窗口各自恰一行 marker、`hilog -r` 隔离失败=环境类 blocked）；ohosTest 的 `N1A_PROBE_TEST_RESULT` 仅为 Phase A 交叉一致 marker，**不是** C9 页证据（Phase A 截图仅 raw 记录） |

聚合与 fail-closed：任一子项 fail → 总 verdict fail；未测到的子项保持 fail
默认值；C5 `not-triggered` 不构成失败（任务与门计划一致）。探针内部各阶段失败
仍执行清理与后置快照，保证任何失败路径都有完整 C7/C8 证据。

## 边界与非范围（与门计划一致，双向不外推）

- 仅 x86_64 Emulator 门 ABI 有加载主张；**aarch64 仅 cross-compile**；
- 无 `device` feature / socket2 patch；无 management/ICE/relay/UI/VPN/TUN/protect
  （外层信道是普通进程内 UDP 回环 socket 对，非任何 VPN/TUN 平台 API）;
- 本 spike 不运行 Emulator/HDC、不产 evidence；正式测量属后续 campaign；
- `Cargo.lock` 权威（`--locked`），构建完全离线（`--offline`）；
- 相对门计划文本的两点实现性说明（非判据修改）：
  1. C2 观测字段：0.7.1 `wireguard_stats` 无显式 state 字段；判据 r0 初稿曾写
     "state==connected 或等价字段"，被第一轮审查以源码事实否决并钉死为
     `time_since_last_handshake >= 0` + 三次握手 drain 合同（r2 冻结）；实现即
     按该冻结文本执行，首轮 write→read 为补充证明；
  2. C5 信号：loopback UDP 的真实饱和表现为内核满队列静默丢包（实测 EAGAIN
     在该拓扑不出现）——冻结判据据此规定 induced 仅由 errno 判定，内核丢包
     （`kernel_queue_drops`）为纯观察字段；host 实测 0 次 errno → 如实
     `not-triggered`，不阻止 overall pass。

## 关键源码依据（boringtun-0.7.1，实现前逐条核对）

- `ffi/mod.rs`：`x25519_secret_key` / `x25519_public_key` / `x25519_key_to_base64`
  （44 字符 base64，`x25519_key_to_str_free` 释放）/ `new_tunnel`（base64 密钥、
  `index` 种子本地索引空间 `index<<8`）/ `tunnel_free` / `wireguard_write`
  （dst ≥ src+32 且 ≥148）/ `wireguard_read` / `wireguard_tick` /
  `wireguard_force_handshake`（dst ≥148）/ `wireguard_stats`；
- `noise/mod.rs`：响应方处理 init 即存会话（318-341）；发起方处理 response 后
  存会话并**回送传输 keepalive**（343-369）；响应方收到首个传输包才
  `set_current_session`（`handle_data`）→ 四腿交换缺一不可；
  `validate_decapsulated_packet`（464-507）按 IP total_length 截断明文 →
  合成包必须是良构 IPv4；
- `noise/rate_limiter.rs` + `noise/timers.rs`：限速只作用于握手包；每次
  `wireguard_tick`（`update_timers`）重置限速计数 → 4000 包数据阶段不可能触发；
- `noise/errors.rs`：错误码名按声明序 0..=16 映射（`wg_error_name`）。

## Emulator campaign runner（正式测量入口）

`n1a-emulator-run.sh` 是 N1a 唯一的 Emulator 证据入口（N0 runner 的结构镜像）：

```bash
bash spikes/n1a-native-dataplane/n1a-emulator-run.sh --selftest  # 纯 host 逻辑自测
bash spikes/n1a-native-dataplane/n1a-emulator-run.sh --dry-run    # host 全流程彩排
bash spikes/n1a-native-dataplane/n1a-emulator-run.sh             # formal（产 sealed evidence）
```

- 固定链路：`netbird_api24_phone` 实例、HDC `127.0.0.1:10000`、CLI `6.1.1.290` 构建
  加 beta `26.0.0.461` 运行时、r1 快照 commit `2c567dc…`（与 N0 同 pin）；物理设备
  target/端口/实例/helper 全部拒绝覆盖。
- 判定按冻结判据（`docs/n1a-gate-plan.md` frozen-r2）字面实现：四字段 marker
  `N1A_RESULT|verdict=<PASS|FAIL>|c5=<induced|not-triggered|fail>|throughput_mibps=<x.xx>`
  （恰四字段、枚举、≥5.00 MiB/s 下限）、ohosTest `N1A_PROBE_TEST_RESULT` 交叉一致、
  host aa RC=0 + guest `TestFinished-ResultCode=0`。
- **两阶段流**：Phase A（`aa test` 机器判定，N0 镜像）→ Phase B（清 HiLog 后
  `aa start` 普通入口，staged `pages/Index.ets` 结果页跑真实探针并显示 PASS/FAIL；
  页文本与 marker 出自同一探针结果对象，构造一致（E2 先例）；截图 ffmpeg
  YAVG>32.0 非黑帧门）。冻结的"恰一行/重复 marker"规则按采集窗口评估（Phase A
  判定窗口、Phase B 页窗口各自恰一行；两执行 verdict 须均为 PASS，c5 差异如实
  记录不单独判废）——该窗口化是 runner 的登记性解释，已在 run header 声明。
- 终态语义：`pass`/`blocked`（仅环境类：Emulator/HDC 退化、构建输入漂移）/
  `fail`（实测判据违反）均产 sealed evidence 并 exit 0；runner 缺陷（前置条件、
  no-clobber、缺失输入）exit 非零且不构成测量。
- `prepare-hap-snapshot.sh`：把 overlay/类型/测试模块/**C9 结果页**/ohosTest
  runner stage 进 r1 快照副本（x86_64-only abiFilters + libentry.so 依赖），
  仓库内 r1 树零改动。
- 边界：Emulator HDC 属既有 E/N campaign 授权面；不触物理设备、无公网、无
  VPN/TUN/protect/management/signal/relay/ICE；不外推 arm64/物理/其他元组。
