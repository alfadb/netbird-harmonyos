# 直连吞吐修复：泵节拍 100 ms → 10 ms、每轮排空 8 → 64（版本 5 / 0.0.5）

本文记录本次**两处版本配置改动**、已落盘的**数据面节流参数**，以及后续补做的**测试 / 打包 / 签名验证结论**（§10）。已跑完整测试、已出 HAP、已签名并 `verify-app` 通过；但**仍未装机、未真机运行**——真实播放（视频）与设备侧 CPU / 耗电代价**均未验证**，也**没有提交**。

## 1. 版本（已落盘，真实解析验证）

- `client/build-profile.json5` 的 default product：`versionCode` 4 → **5**，`versionName` `'0.0.4'` → **`'0.0.5'`**。
- `client/AppScope/app.json5` 同步为 **5 / `'0.0.5'`**，与权威值一致。
- 验证方式：`python3` 的 `json5` 模块**真实解析**两个文件各一次（非文本假装解析），两处取值一致；打包阶段 `client/build.sh` 的 pack.info 对账步骤**已实际运行并通过**（§10.2）。
- 基线说明：仓库 HEAD 为 `1 / 0.0.1`；工作区在本次改动前是**未提交的** `4 / 0.0.4`（保留，未回退），本次在其上递增到 `5 / 0.0.5`。设备当前安装的仍是 `0.0.4`；`5 / 0.0.5` **已构建并签名**（§10.2、§10.3），但**尚未安装**。
- 除上述两处外，`client/` 内没有其他文件携带 versionCode/versionName（已 grep 确认）。

## 2. 现象（0.0.4 已安装）

直连（P2P）**成功**，但**同一 WiFi 下视频不可用**。运行时取证（**草稿**报告，见 §8）：

- 手机 App 自报隧道投递 `wgRxToTun` 1896010 → 6540603，精确 **60.004 s**，增量 4,644,593 B = 77.4 KB/s ≈ **0.619 Mbit/s**。
- 同期主机内层抓包有效段 **29.692 s**：1,705,501 线路字节 = 57.4 KB/s ≈ **0.460 Mbit/s**；其中 `192.168.50.23:443 → 手机` 为 52.0 KB/s（0.416 Mbit/s）。
- 同一时刻手机 `wlan0` 实收 **885 KB/s（7.08 Mbit/s）**，约为隧道投递的 9.2 倍。注意：该 `wlan0` 读数**无法按 UID/流归属**（手机缺 `xt_qtaguid` 等），不能当作视频流量，只能说"同期 WiFi 链路吞吐远高于隧道投递"。
- 目标 `192.168.50.23:443`（仅凭流表头）与手机同网段，流量却被放进隧道、再经主机 `192.168.50.9` 绕回 LAN，"长号绕行"。
- 该窗口中途手机 WiFi 发生重置、承载路径迁移（LAN → 手机公网口 `223.153.50.89` → 迁回 LAN），**各段样本不得跨段平均或混算**。

## 3. 旧实现的上限（源码事实）

- ICE 泵旧节拍 **100 ms**，每个本地 socket 每轮最多读 **8** 个 → 单会话每秒约 80 个数据报。
- WG 泵旧节拍 **100 ms**，`service_tun` / `service_udp` 每轮最多 **8** 帧。
- 两处叠加约 **80 pps**；按 1400 B 内层 MTU 折算 ≈ **0.896 Mbit/s**。这是旧源码注释里的**上限推算**，不是实测带宽。

## 4. 本次已落盘的改动（逐项已在源码核对实际值）

| 项 | 旧 → 新 | 位置 |
|---|---|---|
| ICE 泵节拍 `ICE_PUMP_TICK_MS` | 100 ms → **10 ms** | `client/core/src/connector.rs:5315` |
| WG 泵节拍 `WG_PUMP_TICK_MS` | 100 ms → **10 ms** | `client/core/src/wg_device.rs:2768` |
| WG 每轮排空 `DRAIN_MAX` | 8 → **64** | `client/core/src/wg_device.rs:203` |
| ICE 单会话每轮聚合接收预算 `RECV_BUDGET_PER_RUN` | — → **64** | `client/core/src/ice_session.rs:793` |

要点（均已核对，非计划）：

- ICE 预算对**整个会话的全部本地 socket 聚合**计数，不是每 socket 各 64。
- 预算同时被数据队列空位钳制：`min(64, DATA_RX_CAP − queued)`，`DATA_RX_CAP = 64`（`ice_session.rs:781`、`:1426`）。
- 队列满时丢**最新**一个并累加 `data_dropped`，既不静默也不无界增长（`ice_session.rs:1510-1513`、`:1929`）。
- 预算被某一 socket 吃满时，下一轮从**下一个 socket** 起扫（`recv_cursor` 轮转），防单个候选洪泛饿死其他候选（`ice_session.rs:1428-1443`）。
- 未引入无界排空、未引入无界队列。

## 5. 理论量级（未实测，非设备保证）

64 个/轮 × 100 轮/s = **6400 数据报/s**；按 1400 B 内层 MTU ≈ **71.68 Mbit/s** 上限。这**只是**"节拍 × 预算"的算术上限，**未经设备实测**，也不构成对真机可达带宽的保证。

## 6. 代价与未验证项

- **唤醒代价**：每个泵的空闲唤醒从 10 Hz 升到 **100 Hz**（约 ×10）。真实 CPU 与耗电代价本轮**未验证**，计划下次装机做对照，**当前不做空闲效率声明**。
- TUN 写入预算 `TUN_WRITE_BUDGET_MS = 200` **未改**；原生循环仍可能被 CPU 或该 200 ms 写预算限制（**未测**）。
- 上传/下载与走中继的差异：本次**只证明"预算"上的不对称**，不证明哪条链路最快。
- 测试已跑完：完整 `cargo test --offline --locked` 通过（46 个测试二进制 / **856 passed / 0 failed**），新增 `tests/direct_packet_burst.rs` **8/8** 通过。同一命令的**首次**运行曾出现一次**不可复现**的偶发红（1 个 lib 测试 FAILED + boringtun panic→SIGSEGV 钩子放大为 signal 11），如实记录在 §10.1。

## 7. 保持不动的项

MTU、路由、各类超时、鉴权流程**均未改动**。

## 8. 证据与限定

- 运行时草稿报告：`/home/worker/n2h-kit/status-20261001/v004-slow-video-20261006-075713/REPORT.md`（**草稿、未定稿**）；原始抓包与计数在同目录 `raw/`、`analysis/`。
- **低重传观测不排除全部丢包**：手机无隧道侧抓包、按 UID 流量归属不可用、抓包 `snaplen 96`；内层抓包另见重复数据序号 50 段 / 48,196 B（约占服务器→手机载荷 3.2%）。因此**不得**下"不是丢包/重传问题"的绝对结论。
- 窗口中途网络变化（WiFi 重置 + 承载迁移），各段时间分段独立，**不混算**。
- 报告只使用 L2/L3/L4 表头与长度字段，**无 URL、无凭据**。

## 9. 状态

- 已落盘：版本 5 / `'0.0.5'`（两处一致，json5 真解析通过）；§4 四项参数改动（源码核对）。
- 已完成（本次补做，证据见 §10）：完整测试（46 二进制 / 856 passed / 0 failed；新增 `tests/direct_packet_burst.rs` 8/8）；打包（`pack.info` code=5 name=`0.0.5`、apiVersion 26/26、native 成员身份一致）；签名 + `verify-app`（exit 0，摘要校验 true）。
- 仍未做 / **未验证**：装机 **5 / 0.0.5**、真机直连播放（视频）是否恢复、设备侧 CPU 与耗电代价、空闲效率声明——本轮均不在任务内。

## 10. 本次补做的验证（测试 / 打包 / 签名）

证据目录：`/home/worker/n2h-kit/app-builds/throughput-v005-20261006-080844/`（工作区仍为脏、未提交、未装机）。

### 10.1 完整测试

- 命令：`cargo test --offline --locked`（cwd `client/core`；`CARGO_HOME=/home/worker/rust/cargo`、`RUSTUP_HOME=/home/worker/rust/rustup`、`CARGO_TARGET_DIR=/home/worker/n2h-kit/cargo-target-0034`、`PATH` 前缀 `/home/worker/rust/cargo/bin`）。
- 完成的一次运行：exit **0**，**46** 个测试二进制，**856 passed / 0 failed / 0 ignored / 0 measured**；新增 `client/core/tests/direct_packet_burst.rs` = **8 passed / 0 failed**（单独跑该 target 亦 8/8）。对照上一版 v004（45 个二进制 / 848 passed）：本次正好 +1 个二进制、+8 个测试，等于新增文件本身。
- **诚实记录**：同一命令的**首次**运行 exit **101**——395 个 lib 测试通过后，`n2_ring::tests::configure_validates_and_appends_durable_records` 报 FAILED，随后 lib 测试二进制以 **SIGSEGV（signal 11）** 死亡，因此未能跑到任何集成测试、也得不到总数。该次**并未复现**：单独跑该测试通过；lib-only 重跑 505/505 全绿（默认并行与 `--test-threads=1` 均绿）。SIGSEGV 的机制是 boringtun 0.7.1 FFI 在首次建隧道后安装的 panic→SIGSEGV 钩子（见 `client/core/src/wg_device.rs:4332` 注释），它会把同一测试二进制里后续的 panic 变成静默 signal 11，从而吞掉失败详情。
- 该偶发红的原因**未定论**（疑似并行测试共享全局 ring 状态 / 临时目录干扰）；因此本次**不声明**该测试套件是确定性全绿。两次运行的原始日志都在证据目录（首次另存为 `cargo-test-attempt1-RED.log`）。

### 10.2 打包

- 命令：`DEVECO_SDK_HOME=/home/worker/harmonyos/command-line-tools/current bash client/build.sh`；构建前已确认**无** hvigor/cargo 活动进程；为让 `core/build.sh` 用 `client/core/target`，构建时 **unset** `CARGO_TARGET_DIR`。
- 结果：exit **0**；`pack.info` version code=**5** name=**0.0.5**，apiVersion target=**26** compatible=**26**，releaseType=Release；`libs/arm64-v8a/libnetbird_core.so` 在 HAP 内、sha256 与 llvm-strip 后的 core 产物一致，ELF64 / ET_DYN / AArch64。
- 构建自识别：`N2H_BUILD_SHA256=98ca3f831884ecb454e89c3d06207aeb835935b5e10daa1500dfa2cd1eb81222`（由 `client/core/build.sh` 独立重算并确认嵌在产物中；v004 为 `aa523d58…`）。

### 10.3 签名与验签

- 签名：`java -cp <hap-sign-tool.jar>:<signclasses> N1bdiscSignLauncher` 精确 **8** 个参数（p12、**口令文件路径**、cer、p7b、未签名 HAP、签名输出、`netbird-harmonyos`、`26`）。口令文件**只传路径，未读取、未打印**。exit **0**，`signApp errCode=SUCCESS_CODE`。
- 验签：`java -jar hap-sign-tool.jar verify-app -inFile <signed> -outCertChain vc.cer -outProfile vp.p7b`；exit **0**，`Digest verify result: true (SHA-256)`、`verify: Verify success`。
- 产物（均在证据目录内）：

| 文件 | 字节 | SHA-256 |
|---|---|---|
| `netbird-0.0.5-unsigned.hap` | 5253567 | `cdcf1449c15f60ab0a6cc0c83800da0e6204ef96ccabc82eb6f91a15e976bc81` |
| `netbird-0.0.5-signed.hap` | 5344158 | `22a865b435e2dcc4ca9452a3d90fc0d23ed4c660a0744b657254424a6c2e1786` |
| `vc.cer` | 3134 | `eb9332eda2435d20e5778c3e2d65189fb06762aa7894072207ed67044ea9da16` |
| `vp.p7b` | 4193 | `4be623b12a203d9560a40a4a69f68586471d5355d3318990f7f9feb2ed7425c5` |

- 签名 HAP 内的 native 成员与未签名 HAP 逐字节相同（同一 stripped sha256 `0b97dd95…`），身份字段与 §10.2 一致。

### 10.4 源码未动

- 构建前后源码 / 配置清单（`client/core/src`、`client/core/tests`、`Cargo.toml|Cargo.lock|build.rs|build.sh`、`client/build-profile.json5`、`client/AppScope/app.json5`、`client/build.sh`、`client/oh-package.json5`、`client/hvigorfile.ts`）除清单自身的时间戳行外**逐字节一致**；`git status` 前后同为 **57** 条。构建生成物不计入清单。
- 全程**未安装、未真机运行、未重启、未提交、未推送**。
