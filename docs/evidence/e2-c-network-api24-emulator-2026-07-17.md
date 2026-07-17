# E2 C 网络 API 24 Emulator 证据

最后核验：2026-07-17

本文记录普通第三方 `EntryAbility` 在 API 24 x86_64 phone Emulator 上执行的 E2 纯 C native 网络门。被测路径不使用 Go、NetBird、PS4、TestRunner、VPN 或真机，也不访问公共网络；受控 host endpoint 只由当前 Pod 内临时 C 服务提供。

## 证据记录

```yaml
evidence_id: EV-E2-EMU24-20260717-0002
information_status: current-measured
record_status: reviewed-pass
stage_or_gate: E2
related_stages_or_gates: [E1, E8, R1, R2]
target_tuple:
  distribution: HarmonyOS 6.1.1(24) API 24 Emulator research image
  device: netbird_api24_phone phone Emulator; explicitly not a named physical device
  full_system_version: HarmonyOS 6.1.1(24), image software 6.1.0.125
  architecture: x86_64 guest on x86_64 host; packaged arm64 member was not executed
  sdk_api_syscap: SDK 6.1.1.125/API 24 ordinary UIAbility, ArkUI, HiLog, Node-API, pthread and libc TCP/UDP/DNS/poll/epoll surface; VPN SysCap not exercised
  channel: N/A; unsigned debug research HAP accepted only by this Emulator
code_sha: Git baseline 46ee4e12d91ca01a4eb6684392297844a9e013bf plus uncommitted measured source archive SHA-256 dfa7e5a99f07cbc551a564c54cb32bbc6304b94996a2bb508418f98750f70cad
upstream_sha: N/A; no Go or NetBird code participated in the measured path
toolchain: Debian GNU/Linux 13 x86_64 Pod; host Linux 7.0.14-4-pve; Command Line Tools 6.1.1.290; SDK/Native 6.1.1.125 API 24; Hvigor 6.24.3; ohpm 6.1.2.285; host Node.js 24.15.0; host GCC 14.2.0; Beta Command Line Tools 26.0.0.461; Emulator 26.0.0.200; Beta HDC 3.2.0e; DISPLAY=:1; KVM enabled
working_directory: /home/worker/work/base/netbird-harmonyos
command: bash spikes/r1-api24-hap/e2-c-emulator-run.sh; the runner enters spikes/r1-api24-hap for a clean Hvigor build and uses explicit Beta HDC target 127.0.0.1:10000 for every guest operation
input: one clean-built unsigned application HAP SHA-256 76004031e937604b878049728569d0163b4a2e2e987ceb61eba4de86969baf4f; temporary host C server SHA-256 3930f257eb1f084fa7ba51e0ef362117c5c1ab3d6e4c5273a0490f4768f26d72 bound only to 127.0.0.1 TCP/UDP ports 39021/39022; no test HAP; no initially installed bundle, Emulator process, HDC server, host service or tested port listener; guest HiLog buffers set and queried at 16 MiB before measurement
expected: three distinct ordinary EntryAbility cold-start PIDs; each PID first retains the complete E1 10-round regression, then completes 10 E2 rounds covering recreated TCP loopback listener/client with chunked writes and partial reads, bidirectional UDP IDs/hashes, route-derived Pod-local host TCP/UDP echo, deterministic localhost and reserved .invalid getaddrinfo paths, epoll/poll readiness and timeout, peer-close EOF, refused connection, exact fd/thread restoration, visible E2 PASS page, complete unfiltered HiLog, force-stop, uninstall and zero host residue
actual: PIDs 2992, 3155 and 3351 each passed the complete E1 regression and 10 E2 rounds; E2 produced 30 recreated TCP loopback exchanges, 180 bidirectional loopback UDP datagrams, 30 host TCP exchanges and 90 host UDP datagrams; guest route evidence and ordinary-process getifaddrs both identified 10.0.2.0/24, selected 10.0.2.2 reached only the Pod-local loopback-bound service through Emulator NAT, all DNS and error paths passed, every round restored fd/thread counts exactly, all three screenshots visibly showed the PID-specific E2 PASS page, and uninstall plus full cleanup passed
started_at: 2026-07-17T18:47:14+08:00
ended_at: 2026-07-17T18:49:13+08:00
clock_source: host CLOCK_REALTIME via date --iso-8601=seconds, guest HiLog timestamps, HAP mtime and current qemu boot-complete log segment
artifact_sha256:
  application_hap_measured: 76004031e937604b878049728569d0163b4a2e2e987ceb61eba4de86969baf4f
  application_x86_64_libe2network_member: 7e14cfafc6fad8234be193c6787929c4042eb7d8822a9c2fee5804e67e26151a
  application_x86_64_libprobe_member: caed38130bef9987fff2f5c7be9281675aeeb52d2a3fef7c402d4b6a70c69fca
  host_server_executable: 3930f257eb1f084fa7ba51e0ef362117c5c1ab3d6e4c5273a0490f4768f26d72
  target_sdk_native_api_header: c7d241f14ecd4d3e7c9b9ae34eff1f5da40bec53686717870227285b2e97f56a
  measured_source_manifest: 08729c8cd1e75e26e603ca35863b4d55fe682992b4d9221714b0042aab48a8db
  measured_source_archive: dfa7e5a99f07cbc551a564c54cb32bbc6304b94996a2bb508418f98750f70cad
raw_log_reference: repository-controlled raw/EV-E2-EMU24-20260717-0002-*; transcript 39bc3612fff4f0f17917996b45a88fd85ded7c4bb76478a99d217be342da804c, tag HiLog a68993e760af52051acbea2d2a401ade8b32153e69c8d0fc43327fa068739c31, host raw log fe53ac625ec765f7bd8b2cd7c33162c892002cd95ffd5cd2e1dd3871684c98e6, fault list 9682018900554902a8b75b60404b40f76d424324b5d37b19937c9475a629038c, three complete unfiltered HiLogs and screenshots listed below; repository access
verdict: pass
reviewer: anthropic/claude-opus-4-8 independent E2 evidence review
reviewed_at: 2026-07-17T19:02:00+08:00
review_record: Reviewed hashes HAP=76004031e937604b878049728569d0163b4a2e2e987ceb61eba4de86969baf4f, source-manifest=08729c8cd1e75e26e603ca35863b4d55fe682992b4d9221714b0042aab48a8db, source-archive=dfa7e5a99f07cbc551a564c54cb32bbc6304b94996a2bb508418f98750f70cad; 30 TCP/180 UDP loopback; 30 host TCP/90 host UDP; loopback-only/no-public; forced partial reads/chunked writes; 3 PID/E1 regression/resources/fault/cleanup; 0001 EACCES non-contamination; 0B/0M/1minor.
```

本记录现为 `record_status: reviewed-pass`，`verdict: pass`，E2 已关闭。后续 E3 的 `0003`/`0004` 均为 `reviewed-pass/blocked`，E3 不关闭；E4-E7 为 dependency blocked、不开始。E1 整体仍因最新正式 NetBird 声明基线的官方 Go 1.25.12 loader 路径失败而 blocked；E8 仍为 `CLOSED`，真机执行禁令不变，R0/R1/R2 均未退出。

## 判定明细

| 子项 | 每个 PID 的实测 | 三 PID 合计 | 判定 |
| --- | ---: | ---: | --- |
| E1 回归 | 10 轮、1000 buffers、1000 callbacks、10 fd transfers | 各项 30 轮/3000/3000/30 | 三个 PID 均独立 `E1_C_PROBE_RESULT PASS` |
| TCP loopback | 10 个新 listener/client；每次 4096 bytes 双向 echo | 30 次、122880 bytes/方向 | 每次 14 个 client writes、142 个 server partial reads、13 个 server writes、179 个 client partial reads；hash 相等 |
| UDP loopback | 每轮 C2S 3 个、S2C 3 个固定 ID/hash datagrams | 180 个 datagrams | 全部 192 bytes，来源地址/端口、payload、ID 与 FNV-1a 均匹配 |
| host TCP | 每轮 2048 bytes chunked echo | 30 次 | host 与 Emulator 双侧 hash 一致；host 每 PID 恰为 10 次 |
| host UDP | 每轮 3 个固定 ID/hash datagrams | 90 个 | host 每 PID 恰为 30 个；原始服务日志逐 datagram 保存 ID/hash |
| DNS | `localhost` 成功且仅 loopback；AI_NUMERICHOST对非数字名确定性EAI_NONAME，不经过resolver | 各 30 次 | `localhostCount=1, ipv4=1`；`e2-reserved.invalid` 为 `EAI_NONAME(-2)` |
| 事件/错误 | 每轮 epoll、poll timeout、close EOF、关闭 listener 后拒绝 | 各 30 次 | `epollEvents=3`、timeout 25 ms、EOF 1、`EBADF(9)`、`ECONNREFUSED(111)` |
| 资源 | 每轮前后 fd/thread 相等 | 30 对 | fd 均 `38->38`；PID 2992/3351 线程 `29->29`，PID 3155 为 `28->28` |
| 可见页面 | 每 PID 完成后保持存活并截图 | 3 张 | 人工核对均显示对应 PID、`10 rounds` 与 `E2 C network PASS` |
| TestRunner | 未构建、未安装、未调用 | 0 | 仅普通 `EntryAbility` 构成运行证据 |

TCP 每轮都关闭 client/server/listener/epoll 后验证旧 listener fd 返回 `EBADF(9)`，再向旧端口连接并验证 `ECONNREFUSED(111)`；下一轮重新创建 listener。三 PID 的 30 个 listener 端口与每轮 hash 保留在 tag HiLog，不以汇总 marker 替代逐轮记录。

## 受控 Host Endpoint

host C 服务在 Emulator 启动前只绑定 Pod `127.0.0.1:39021` 与 `127.0.0.1:39022`；runner 使用 `ss` 拒绝 `0.0.0.0` 或其他非 loopback bind。服务只接受本次协议固定长度的 TCP/UDP payload，解析 PID/round/ID，记录原始 FNV-1a 后原样 echo，达到精确 `30 TCP + 90 UDP + 3 PID` 后自行退出。

正式 guest `/proc/net/route` 同时记录 default gateway `0202000A` 和直连网络 `0002000A/00FFFFFF`，按 Linux little-endian route 表示分别对应 `10.0.2.2` 与 `10.0.2.0/255.255.255.0`。普通应用不能读取该 proc 文件，因此 native 通过 `getifaddrs` 独立得到 `eth0:10.0.2.0/255.255.255.0`，按该实测网段生成候选并在首次尝试实连 `10.0.2.2`。该结果只证明本次 Emulator NAT 到 Pod 本机临时服务，不是公共网络、通用 Emulator 地址或产品 endpoint 结论。

host 服务原始日志以 `HOST_SERVER_RESULT|verdict=PASS|tcp=30|udp=90|pids=3` 结束；三个 `HOST_PID_SUMMARY` 分别绑定 PID 2992、3155、3351，均为 `tcp=10|udp=30`。服务端未绑定或访问公网地址，E2 executable inputs 的公共 URL/IP 字面量扫描通过。

## DNS 与公网边界

`getaddrinfo("localhost", "80")` 每轮确定性返回一个 IPv4 loopback 结果。对非数字名使用 `getaddrinfo("e2-reserved.invalid", ..., AI_NUMERICHOST)`，AI_NUMERICHOST对非数字名确定性EAI_NONAME，不经过resolver；因此保留 localhost 离线解析，同时不声明外部 DNS resolver 可用，也不访问镜像中配置的公共 resolver。

## 资源与故障清单

E1 预热和完整回归结束后建立 E2 资源基线。每个 E2 round 前后都在主 ArkTS TID 调用 `/proc/self/fd` 与 `/proc/self/task` 快照并要求与对应 PID 的 E2 初始基线完全相等；30 对全部通过，最终快照也相等。E2 native 不创建线程，所有临时 socket 与 epoll fd 都在成功或失败的单一 cleanup 路径关闭。

fault 目录在测量前和每个 PID 截图前各保存一次完整列表；四段列表内容 SHA-256 均为 `1af99b38d398d6bb0de7b9d7ad436dae3e390f3e7e2342c9a3723503b00bc0e5`，没有新文件。列表中的 `appspawn`、`sceneboard` 历史 fault 均早于本记录开始时间且四次不变；三个被测 PID 均保持存活到截图，三份完整 HiLog 无 E1/E2 FAIL 或 `E2_NATIVE_ERROR`。

## 原始材料

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `raw/EV-E2-EMU24-20260717-0002-application-hap.bin` | 2745827 | `76004031e937604b878049728569d0163b4a2e2e987ceb61eba4de86969baf4f` |
| `raw/EV-E2-EMU24-20260717-0002-libe2network-x86_64.bin` | 35672 | `7e14cfafc6fad8234be193c6787929c4042eb7d8822a9c2fee5804e67e26151a` |
| `raw/EV-E2-EMU24-20260717-0002-libprobe-x86_64.bin` | 21904 | `caed38130bef9987fff2f5c7be9281675aeeb52d2a3fef7c402d4b6a70c69fca` |
| `raw/EV-E2-EMU24-20260717-0002-host-server.bin` | 21432 | `3930f257eb1f084fa7ba51e0ef362117c5c1ab3d6e4c5273a0490f4768f26d72` |
| `raw/EV-E2-EMU24-20260717-0002-source-manifest.txt` | 1790 | `08729c8cd1e75e26e603ca35863b4d55fe682992b4d9221714b0042aab48a8db` |
| `raw/EV-E2-EMU24-20260717-0002-source.tar` | 153600 | `dfa7e5a99f07cbc551a564c54cb32bbc6304b94996a2bb508418f98750f70cad` |
| `raw/EV-E2-EMU24-20260717-0002-transcript.log` | 45142 | `39bc3612fff4f0f17917996b45a88fd85ded7c4bb76478a99d217be342da804c` |
| `raw/EV-E2-EMU24-20260717-0002-build.log` | 3012 | `ca4512199739d1f88a2302a1f8d2de7be1d40d1adb47a03995eb7c39977f52b5` |
| `raw/EV-E2-EMU24-20260717-0002-hilog-tag.log` | 1302900 | `a68993e760af52051acbea2d2a401ade8b32153e69c8d0fc43327fa068739c31` |
| `raw/EV-E2-EMU24-20260717-0002-host-server.log` | 13425 | `fe53ac625ec765f7bd8b2cd7c33162c892002cd95ffd5cd2e1dd3871684c98e6` |
| `raw/EV-E2-EMU24-20260717-0002-fault-list.log` | 9264 | `9682018900554902a8b75b60404b40f76d424324b5d37b19937c9475a629038c` |
| `raw/EV-E2-EMU24-20260717-0002-run1-hilog-full.log` | 997653 | `8c777cc413030273cb8922b28cbee3d89aef66be2160603bf3c3d883ffd56d67` |
| `raw/EV-E2-EMU24-20260717-0002-run2-hilog-full.log` | 780480 | `5b79ccb19f4d5fd2eda1d74ee35e6085e964fc49df9cd4cf8321b8cafb8d0fdd` |
| `raw/EV-E2-EMU24-20260717-0002-run3-hilog-full.log` | 765098 | `e01b5c9a826ca84be963c14aaa68563fe3b03756a574d420eada7eb7f99f44eb` |
| `raw/EV-E2-EMU24-20260717-0002-emulator-console.log` | 1822 | `dfb2fcd27a74f4fb4590b8eca6a368ebe50edbaf31a76c7bbcb8f1c8b7199d7e` |
| `raw/EV-E2-EMU24-20260717-0002-run1.png` | 145804 | `9e04e75357dd28f98d299282c92c8748a791e04191f85072ccc3128613a02e95` |
| `raw/EV-E2-EMU24-20260717-0002-run2.png` | 145272 | `bcc44a329d8421520dff011714028aa2f81ad25b1fbb7ecd0a1ee3b856016322` |
| `raw/EV-E2-EMU24-20260717-0002-run3.png` | 145928 | `791f81639b2f1dd415ff73b618a981f33abacad632520d8c9aa8bdf811499968` |

## 失败记录

`EV-E2-EMU24-20260717-0001` 在 PID 2935 完成完整 E1 回归和首轮 TCP/UDP loopback 后，普通应用读取 `/proc/net/route` 得到 `EACCES(13)`，runner 按预定 FAIL marker 立即停止。该 ID 未复用；其 transcript SHA-256 为 `f588f231f8a5496051e5ea361ac32b0d4c4a2e47b0f7dbfdabbafbcb43e88d71`，host 日志 SHA-256 为 `4740f27edd50f7f39316220fb26a4baab44707ffe2e046668893df3d33d2bf2e`，失败制品和日志保留在相同前缀下。

`0002` 没有绕过普通应用权限，也没有硬编码一个未经验证的 host 地址。它保留 runner 从 guest route 表取得的客观证据，再用普通进程公开 `getifaddrs` 取得相同直连网段并派生候选，最终以真实 TCP/UDP 收发决定 `10.0.2.2` 可达。失败记录不能替代 `0002`，`0002` 也不改写 `0001`。
