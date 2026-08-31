# N1a Emulator campaign 0004 consumed-failure 记录（2026-08-31 执行）——0002 之谜彻底解开

最后核验：2026-08-31

本文登记 `N1A-EMU24-20260831-0003` / `EV-N1A-EMU24-20260831-0003`（AUTH-N1A-EMU24-20260831-0003 granted，判据 frozen-r3，attempt initial）的执行事实：**campaign 再次因 overlay 一致性守卫触发而 marker 缺失判 fail，但本次诊断通道（缺陷 #3 修复的成果）完整捕获了触发瞬间的全部字段值，根因即刻确定**。

## 根因（诊断通道逐字证据）

```text
N1A_DIAG|stage=verdict-integrity-mismatch|ok=0|c=[1,1,1,1,2,1,1,1,1]|v=4000|mm=0|lost=0|bp=0|c7fd2=1|c7set=0|c7tid=0|c8free=2|pm=unknown
```

**探针全部判据 pass（ok=0、C5=not-triggered）且全部计数完全正常**（v=4000 恰为期望值、mm=0、lost=0、C7 fd 门 pass、C8 tunnel_free×2）——触发守卫的原因是 **overlay C++ 与 ohosTest ETS 中硬编码了 `verified != 2000` 的旧值**（0001 实现时代注释写"2 directions x 10 x 200"但数字只算了一个方向=2000），而 r3 判据下真实期望为 **4000**（2 方向 × 10 × 200）。Rust 侧 `expected_packets_total = 2 * ROUNDS * PACKETS_PER_ROUND = 4000` 一直正确；host cargo 测试走 Rust 内部断言故不触及 overlay 的 2000；Emulator 走 NAPI 层才触发。0002 与 0004 的"数学矛盾"实为此**跨语言常量不同步**——诊断通道的第一手数据一次命中。

## 探针真实测量事实（本次诊断通道完整捕获，Emulator 实测）

| 判据 | 值 |
| --- | --- |
| 全部 criteria | c1-c9 全 pass（c5=not-triggered，loopback 拓扑预期） |
| 握手 | established=true、834ms、attempts=1、首轮回环 ok |
| C3 完整性 | 4000/4000、mm=0、lost=0、字节账四方向 delta 各 2,048,000 全对 |
| C4 吞吐 | **72.47 MiB/s**（地板 5 的 14.5 倍；pump 窗 53.9ms） |
| C5 背压 | not-triggered（errno 零次；kernel drops 32,512 观察值；512/512 送达零损坏） |
| C6 tick | 3 间隔、6 次 tick、6 个网络包、全程 session 存活、后续突发 ok |
| C7 | fd 门 pass（socket 29/30 T3 全关）、fdset 差 0、新 TID 0、**进程级观察恰好 fd 30→30 / task 14→14（零漂移——0001 的"环境噪声"假设在这台模拟器的这次运行中未出现，但按 r3 已不设门）** |
| C8 | tunnel_free×2、socket 全关 |
| RSS | 95,780→97,264 KB（观察登记） |

**Emulator 上 BoringTun 数据面的全部冻结判据事实上全部满足**——fail 纯粹是 overlay/ets 的 2000 硬编码守卫在 pass 结果上误触发。

## 缺陷 #5（实现层，本次已修复）

`napi/n1a_overlay.cpp:316` 与 `napi/runN1aProbeTest.ets:29` 的 `2000` → `4000`（两处均带注释修正）。修复后 `build.sh`（13 断言）与 `--selftest`（22 用例）全过。

## 处置

本 campaign `consumed-failure`（C9 缺 marker → fail 字面记录不改写）。0002/0004 的守卫触发至此完全归因（缺陷 #5）；0003 未触达探针。**下一个 campaign（第 5 个 ID）预期首次触达完整两阶段流**。重测须新 evidence ID（用户授权）。探针侧无需任何改动。
