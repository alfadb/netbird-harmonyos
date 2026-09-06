# staticcheck — N1BDISC A1-A12 静态断言机器检查器

`check_static.py` 是 `docs/n1b-disc-gate-plan.md:1072`「静态断言（freeze 前机器检查，
违反即审查 blocker）」的机器检查载体（B-03 整改独立审查件）：对
:1074-1087 的 A1-A12 逐条做 grep/正则 + 括号配对的轻量结构分析，逐条产出
PASS / FAIL / QUESTION + 文件:行号证据 + 登记表，末尾汇总 JSON。

边界：host-only、只读源码、零设备、零构建、不修改任何被检源码。
纯 Python 3 标准库，无第三方依赖。

## 用法

```bash
cd spikes/n1b-disc-phys-hap
python3 staticcheck/check_static.py              # 全文报告（每条一节）+ 末尾汇总 JSON
python3 staticcheck/check_static.py --json-only  # 仅汇总 JSON（供 freeze 记录挂接/CI 消费）
```

路径锚定：以脚本自身位置定位 spike 根（`staticcheck/` 的上一级），从任意 cwd 运行均可。

### 退出码

| 退出码 | 含义 |
| --- | --- |
| 0 | 无 FAIL（QUESTION 不驱动退出码，但 freeze 前必须逐条人工裁决并登记处置结论） |
| 1 | 任一断言 FAIL（:1072 — 违反即审查 blocker） |
| 2 | 预留（当前未用） |

检查器自身异常（源码结构漂移导致锚点解析失败等）按该条 FAIL 显性上报
（`fails` 内 `tool-error:` 前缀），绝不静默放行。

### 判定语义

- **PASS** — 该断言的全部机械子检查通过。
- **FAIL** — 任一子检查被违反（`fails` 列表逐项给出位点）。
- **QUESTION** — 机械上不可完全判定、需人工裁决的点（如 selftest 反例缺失、
  结构锚不完整）。QUESTION 不等于通过。

## 断言 ↔ 检查内容速查

| # | 机器检查内容（判据 :1074-1087 + 注 :1089-1107） |
| --- | --- |
| A1 | 恰 1 个 `pthread_create` 调用点且 = D-W 登记位点 dw.rs；零线程/异步旁路（std::thread、tokio、async-std、smol、rayon、async fn）；零 `napi_create_threadsafe_function`/`napi_create_async_work`（extern 块与调用两查）。注释出现单独登记、不计违规 |
| A2 | 以 `fd_orig` 为实参的 read/write/F_SETFL 调用点 = 0；`close(fd_orig)` 恰 1 且位于 d6.rs 的 D6a S3_B/S3_R 发射行之间 |
| A3 | 14 个 BoringTun 符号（:522）全 crate 出现穷举分类；允许区 = btkeep.rs 的 `#[link_name]` 声明 / `BT_SYMBOLS` 字符串表 / `BT_FFI_KEEP #[used]` 取址数组；其余出现（尤其 call site）= 违规。**口径注释（freeze 记录须逐字收录）：取址保链不产生执行转移不计引用（审查席建议口径）** |
| A4 | 零 `pthread_timedjoin_np`/`tryjoin_np`；sleep 原语收敛为 `clock_nanosleep(CLOCK_MONOTONIC)` 单点且仅 10/50ms 档；`pthread_join` 唯一且前置终态标志门（r3-D5 显式豁免）；全部等待循环的 deadline 判定证据行（与 A8 登记表同源） |
| A5 | crate 发射的 `N1BDISC_` 字面集 == :1095-1097 冻结 56 字面（双向差集为空）；豁免集 {`N1BDISC_D2_REJTEXT`,`N1BDISC_RESULT`} 发射面零出现（runner 豁免集定义 / selftest 反例夹具中的出现单独登记归类）；ETS 文件零 `N1BDISC_` 字面 |
| A6 | `openat` 调用点仅 dw.rs inwait 区两处（read_proc_stat / read_proc_syscall），路径字面 ∈ {`/proc/self/task/{}/stat`, `/proc/self/task/{}/syscall`}，flags=`O_RDONLY`；零写标志常量；tid 绑定 `DW_TID` |
| A7 | stat 的 state 解析 = 行内最后一个 `)` 之后的下一 token（`rfind(')')` + `split_whitespace().next()`，实现行号登记，含纯函数助手调用链核对）；零左侧定位实现（split(' ')/splitn(3/nth(2)）；selftest 侧 comm 带空格/括号反例字面存在（缺 → QUESTION） |
| A8 | probe/src 全部 loop/while/for 逐循环登记（:1084 载体）：位置/kind/宿主函数/有界类/终止条件证据行。有界类 = 字面范围 / 有限集合 / 单调钟门 / 计数上界 / 模剩余 / 右移折叠 / 计数熔断；UNPROVEN → FAIL |
| A9 | ArkTS `dw_destroy_t` → `destroy()` → `dw_destroy_c` → 有界等待四语句同函数直线出现（行序 + 分支关键字扫描 + 深度可达性）；`.destroy()` 全源码恰 1 调用点；`_C` 先于调用 → FAIL；不可静态定位/条件嵌套 → QUESTION |
| A10 | join-timeout 支（else 支）内零 read/write/fcntl/close/d6b/fd_dup 记号；`d6b` 调用唯一且位于 `if (term.terminal)` 支内；支后主线程零直接 fd 操作（P11/P12 经 native）；d6.rs D6b S4-S7 全部门控于 d6b() 内 |
| A11 | poll 返回后 `mono_ms()` 恰 1 次；`sys::errno()` 零再读；ret/perrno/revents 绑定后零再赋值；快照五写与 `DW_RETURN` 发射共享同一组局部变量名（同源）；`dw_drain_end` worker 内 poll 前单写、零重读（:1104 单列口径；P12 侧快照读另行登记） |
| A12 | RACEWIN 发射全 crate 恰 1 且位于且仅位于 F=0 盒到期条件支；cut/class/RACEWIN/watchdog/poll raw 五输出绑定单次 `dw_worker_terminal` seq_cst load；:758 盒到期复验（`f_after_box`）为规格明文许可的唯一例外读。**双形态核对**：直接链形态（五输出元组单 if/else 链 + cut=f）与 B-01 结构体派生形态（`derive_post_outcome` 纯函数 + `PostOutcome` 字段 + `out.racewin` 门控 + `out.cut` 绑定）均被识别 |

## freeze 记录挂接方式

1. **冻结时点运行**：freeze 前在冻结候选树上运行
   `python3 staticcheck/check_static.py > a12_staticcheck_report.txt`，
   并将全文报告与 `--json-only` 输出一并归档。
2. **树状态钉住**：汇总 JSON 的 `source_fingerprint` 字段给出被检文件集合的
   逐文件 sha256（前 12 位）与整树聚合值 `tree_sha256_12`——freeze 记录引用该值
   即可与被检源码状态一一对应（树再变动 → 重跑 → 指纹随之更新）。
3. **结论登记口径**：freeze 记录逐条登记 `{id, verdict, fails, notes}`；
   任何 FAIL 即 :1072 审查 blocker，freeze 不得进行；QUESTION 逐条附人工裁决
   结论后关闭。A3 节输出中的口径注释（取址保链不产生执行转移不计引用）
   须逐字收录进 freeze 记录。
4. **A12 与 B-01 的关系**：A12 的判定随源码实态而定。B-01 修复（post_emit
   结构体派生重构）落盘前后工具分别按双形态核对；报告中的
   `source_fingerprint` 表明结论对应哪一形态的树。

## 自证：对抗性变异测试

工具不是恒 PASS 器。对 12 条断言各注入一类判据破坏变异（`std::thread::spawn`
注入、`read(fd_orig)` 注入、符号字面表外出现、sleep 档位 33ms、发射豁免集字面、
非白名单 /proc 路径、rfind 锚替换、无界 loop 注入、`_C` 提前于 destroy()、
d6b 挪入 join-timeout 支、poll 后二次钟读、二次 FLAG load、racewin 泄漏第二 cell），
14/14 全部命中对应 FAIL，无变异对照树全 PASS——变异测试脚本为一次性验证件，
不入库（验证记录见交付报告）。

## 文件

- `check_static.py` — 检查器本体（唯一工具文件）。
- `README.md` — 本说明。
