# -*- coding: utf-8 -*-
"""n1bdisc_engine — N1BDISC campaign 单一编排 engine（真实 transport → 判定面）。

DryRun 与 Live 的**共同组装层**（后续 CLI 接线增量把 ``--dryrun``/``--live`` 都
路由到这里，不再各写一个平行模拟器）：经注入的 :class:`n1bdisc_hdc.HdcExecutor`
（冻结白名单表）驱动真实 transport（``n1bdisc_transport_real.RealHdcTransport``；
测试注入 FakeHdc 适配），按冻结协议顺序收集**真实 fact**，随后完全复用既有判定
面——``n1bdisc_run.derive_and_judge``（chunk/ledger/全序/dw/verdict 全部原函数，
零复制）与 ``n1bdisc_fsm.ObservationWindowFSM``、``n1bdisc_capture.capture_stream``、
``n1bdisc_death`` 七分量。本模块不实现任何设备侧语义，不新增 cause/marker/F 编号。

协议顺序（冻结：gate 13 主线 + host finally :1645-1662）::

    operator-ready（caller 显式确认；False → 零操作拒启）
    → gate 5 三探针 → **pre-campaign FaultProbe 快照**（窗界唯一判据的基线）
    → MkdirStaging → SendHap → InstallHap
    → **HilogStream 实际开流（早于 StartEntry，M-03）**
    → StartEntry（恰一次、不重试；FSM 记账真 clock）
    → capture_stream（有界三重到点；首枚关联 marker 后唯一 positive PidOfVpn）
    → host finally（顺序冻结，每步失败也继续后续 cleanup，不默默 pass）：
      1 停流 → 2 FaultProbe 差分 + 仅命中文件 FaultRecv → 3 ForceStop 前第二次
      PidOfVpn → 4 死亡事实冻结 → 5 ForceStop(合法 Reason) → 6 Uninstall →
      7 RemoveStaging → 8 四 absent 探针 → 9 终判+seal → 10 integrity 收尾
      （live：caller 注入的只读 freeze 收口复核回调恰一次调用，失败按既有
      verdict invalid 轴承载；DryRun 恒 ``integrity={}``）

安全与事实边界：

- ``operator_ready=False``：零 hdc 操作、零 ID 消费，记录器显式 ``finish`` 成
  ``incomplete`` 拒启记录后返回（不依赖 ``__del__``）；
- **ID 零生成零消费**：campaign/evidence ID 只存在于 caller 构造的 recorder
  metadata 里；engine 不产生、不预写任何 ID；
- ``mode`` 只控制记录面 ``is_evidence``（live=True / dryrun=False，门 11）；并
  与 transport 形态配对校验（live 必须 RealHdcTransport、dryrun 必须非
  RealHdcTransport）——防合成数据标成证据、也防真 transport 被当 DryRun 消耗；
- **target 仅内存**：不写入任何记录文件；raw HDC error / 异常文本在落 events
  前定点替换 ``<TARGET>``；**绝不全量 argv 入日志**（events 只有操作名 + exit/
  阶段脱敏摘要，含 HilogStream）；捕获 probe 原文逐字进 ``capture.jsonl``、
  fault 条目原文逐字落 run 目录——**脱敏不触碰判定数据**；
- 物理落盘只在本次 recorder 自有 run 目录（fault 条目取回落
  ``<root>/faultlogger/``，由 finish 的 sha256 清单一并封签）；记录写入失败即
  异常外抛，**不声称 sealed 成功**；
- 死亡事实、静默、span 全部来自真实 clock/捕获/probe/file stdout：capture 静默
  三态 = 时间边界背书（POST 在场 / Allow 盒或观测窗到点）→ ``True``；capture
  自身缺口（EOF 非零退出码/异常/兜底熔断，及 **EOF rc=0 早退**——流先于任何
  时间边界干净终态，尾部从未被观测到窗界）→ ``None`` → 既有
  ``marker-gap-indeterminate``；墙钟
  取设备侧 ``YYYY-MM-DD hh:mm:ss.mmm`` 字面（fault 条目与 hilog 行前缀同源），
  不可求值 → 既有 ``tail-clock-unresolvable`` / :607 F4 面——**不携带 DryRun
  合成 100000/90000/80000**；
- join 边界（独立 T0 裁定）：Live 恒 ``join_exit_rc=None``、
  ``join_blocked_registered=False``（不能由 ``DW_EXIT`` 推 join；join 阻塞与其
  他主线程挂死在 capture 上不可区分，仅作 raw 观察注登记，不新增
  cause/marker/白名单）；POST 十值域 + JT/D6b sticky 校验照常；PRE 无 POST 且
  进程活 → F9，不洗成 join-blocked-observed；
- EOF/非零 stream/timeout 不等同 pass：一切按既有 FSM 收口与 ``derive_and_judge``
  封装，verdict 只来自既有闭集；
- **live integrity 收口复核（步 10）**：``revalidate_freeze`` 为 caller（CLI）
  注入的只读零参回调，在 host finally 清理全部结束之后、正式 verdict 求值与
  ``recorder.finish`` 之前恰一次调用，重复校验冻结输入（manifest/governance
  bytes 外部 expected hash、code_sha/dirty、runner 文件集/文件 hash、artifact
  完整 hash/so/confirmation/retired 状态）。复核结论真实写入 ``record.integrity``
  （结构见 :func:`_finally_integrity_close` docstring）；失败码非空即经
  ``derive_and_judge(freeze_integrity_failures=...)`` 萰入既有 verdict invalid
  优先轴（不新造判据/cause）。回调异常不跳过 cleanup/封签：异常本身转为稳定
  失败码按 invalid 处理。DryRun/拒启记录 ``integrity={}`` 字面不变；
- ``before_start_entry``/``revalidate_freeze`` 均为行为挂点：不产任何 fact
  数据、不影响任何判定规则逻辑（integrity 失败码走既有 invalid 轴）。

测试钉：``selftests/test_engine.py``（FakeHdc 定时流适配 + RealHdcTransport 端
到端 fake-HDC 可执行夹具）；FakeClock/小时间盒只存在于测试，生产入口无任何可
注入假 fact 的 hook（``capture_timing`` 只收 :class:`n1bdisc_capture.CaptureTiming`
时间盒，不产数据）。
"""

from __future__ import annotations

import calendar
import os
import re
import sys
from typing import Any, Callable, Dict, List, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import n1bdisc_capture as cap                 # noqa: E402
import n1bdisc_core as core                   # noqa: E402
import n1bdisc_death as death                 # noqa: E402
import n1bdisc_fsm as fsm                     # noqa: E402
import n1bdisc_hdc as hdc                     # noqa: E402
import n1bdisc_recording as recording         # noqa: E402
import n1bdisc_run as run_mod                 # noqa: E402
import n1bdisc_transport_real as transport_real  # noqa: E402
import n1bdisc_verdict as verdict_mod         # noqa: E402

#: 记录面 engine 标识（metadata/result 溯源用）。
ENGINE_ID = "n1bdisc-engine/1"
#: campaign 模式闭域（门 11 dryrun / live）。
MODES: Tuple[str, ...] = ("dryrun", "live")

#: fault 条目在 run 目录内的落盘子目录（recorder 自有目录内，随封签入清单）。
FAULTLOGGER_SUBDIR = "faultlogger"

_DEVICE_TS_RE = re.compile(
    r"(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})\.(\d{3})")


class EngineError(Exception):
    """engine 用法前置违反（executor/mode/clock/recorder 配置不符；不触设备）。"""


# ---------------------------------------------------------------------------
# 纯事实 helpers（无 I/O；供测试直接钉）
# ---------------------------------------------------------------------------

def redact_text(text: Any, target: str, limit: int = 200,
                strip: bool = True) -> str:
    """落 events 前的定点脱敏：target → ``<TARGET>``。

    默认（``strip=True, limit=200``）保持既有摘要口径；``strip=False`` +
    ``limit=0`` 供命令输出留存用——只替换 target，保留换行/首尾空白、不截断
    （仍**非**原始无损字节：target 已被替换）。
    """
    value = str(text or "")
    if target:
        value = value.replace(target, "<TARGET>")
    if strip:
        value = value.strip()
    if limit and len(value) > limit:
        value = value[:limit] + "…"
    return value


class FaultSnapshotParseError(Exception):
    """FaultProbe stdout 含非合法 ``find -print`` 行（整 probe 视为 unknown）。"""


_FAULT_PATH_PREFIX = hdc.FAULTLOGGER_DIR + "/"


def parse_fault_snapshot(stdout: str) -> List[str]:
    """FaultProbe ``find -print`` stdout → 命中文件 basename 集合（字节序）。

    每个非空行必须是 ``FAULTLOGGER_DIR`` 下的**完整路径**、直接子项纯 basename、
    且命中冻结 bundle glob（``*<bundle>*``）；任何非法/混合行抛
    :class:`FaultSnapshotParseError`（调用方据此整 probe 记 unknown，不静默过滤）。
    """
    out: List[str] = []
    for raw in (stdout or "").splitlines():
        line = raw.strip()
        if not line:
            continue
        if not line.startswith(_FAULT_PATH_PREFIX):
            raise FaultSnapshotParseError("not-under-faultlogger-dir: %r" % line)
        name = line[len(_FAULT_PATH_PREFIX):]
        if (not name) or ("/" in name) or ("\\" in name) or name in (".", ".."):
            raise FaultSnapshotParseError("not-direct-basename: %r" % line)
        if core.DEFAULT_BUNDLE not in name:
            raise FaultSnapshotParseError("not-bundle-glob-match: %r" % line)
        out.append(name)
    return sorted(set(out))


def classify_fault_probe(result: Optional[hdc.HdcTransportResult]
                         ) -> Optional[List[str]]:
    """FaultProbe 三态：rc0 且 stdout/stderr 均干净且全部行合法 → 排序去重 basename
    列表（可为 ``[]``）；其余（rc≠0 / 权限 / 非空 stderr / 解析错误 / transport 异常）
    → ``None``（unknown，不写 ``[]``、不 observed-false）。"""
    if result is None:
        return None
    if result.exit_code != 0:
        return None
    if _permission_denied(result.stdout, result.stderr):
        return None
    if (result.stderr or "").strip():
        return None
    try:
        return parse_fault_snapshot(result.stdout)
    except FaultSnapshotParseError:
        return None


def device_wall_ms(text: str) -> Optional[int]:
    """设备侧 ``YYYY-MM-DD hh:mm:ss.mmm`` 字面 → 毫秒读数（UTC 口径换算）。

    只用于**同源设备侧差值**（D8b 跨度 / ``marker_tail_state`` 静默跨度——两端
    同取设备时钟，时区口径在差值中相消）；绝对值不承载时区语义，不做窗界判定
    （窗界唯一判据 = 快照集合差分，判据 T2 冻结）。无匹配返回 ``None``（由既有
    冻结求值落 unobservable/F4 面，不新造 cause）。
    """
    if not isinstance(text, str):
        return None
    match = _DEVICE_TS_RE.search(text)
    if match is None:
        return None
    year, month, day, hh, mm, ss, mmm = (int(g) for g in match.groups())
    try:
        epoch_s = calendar.timegm((year, month, day, hh, mm, ss, 0, 0, 0))
    except (ValueError, OverflowError):
        return None
    return epoch_s * 1000 + mmm


def death_evidence_wall_ms(parses) -> Optional[int]:
    """死亡证据墙钟 = 文件名字节序首个携带可解析时间戳的新增条目时间戳。"""
    for parse in sorted(parses, key=lambda p: p.file_name):
        wall = device_wall_ms(parse.timestamp or "")
        if wall is not None:
            return wall
    return None


def captured_line_wall_ms(raw: str) -> Optional[int]:
    """单条捕获 hilog 行的设备侧墙钟（行前缀时间字面，同 device_wall_ms 口径）。"""
    return device_wall_ms(raw)


def capture_silence_fact(capture: Optional[cap.CaptureResult]) -> Optional[bool]:
    """capture 静默三态事实（喂 ``death.eval_process_death`` 的 ``capture_silent``）。

    - ``True``：静默有**时间边界背书**——POST 在场收口，或 Allow 盒 / 求值观测
      窗到点收口（观测持续到边界仍无关联 marker，尾部静默可判）；
    - ``None``：capture 自身缺口——流异常终态非零退出码 / 捕获异常 / runner 兜底
      熔断，以及 **EOF rc=0 早退**（流先于任何时间边界干净终态：尾部从未被观测
      到窗界，静默与采集失效不可区分）——落既有 ``marker-gap-indeterminate``；
    - capture 从未运行（开流前中止）同归 ``None``。
    """
    if capture is None:
        return None
    if capture.close_reason in (cap.CLOSE_REASON_EXCEPTION,
                                cap.CLOSE_REASON_WALLCLOCK_FALLBACK):
        return None
    if capture.close_reason == cap.CLOSE_REASON_EOF:
        if (capture.stream_exit_code or 0) != 0:
            return None
        # EOF rc=0：干净终态 ≠ 已观测尾部到点。仅当 POST 在场或 Allow/观测窗
        # 到点标志在场（时间边界已到）才判静默；否则早退缺口 → None（收口原因
        # 语义不变、不新造 cause）。
        post_seen = any(m.name == "N1BDISC_POST" for m in capture.markers)
        if post_seen or capture.allow_deadline_reached \
                or capture.window_deadline_reached:
            return True
        return None
    return True


# ---------------------------------------------------------------------------
# 内部：单步执行 + 事件登记（操作名 + exit/阶段脱敏摘要；绝无 argv/target）
# ---------------------------------------------------------------------------

class _Campaign:
    """单次 run_campaign 的可变装配状态（单线程；成员见 run_campaign 初始化）。"""


def _event(recorder: recording.RunRecorder, label: str, target: str,
           **details: Any) -> None:
    """脱敏事件登记（details 中的自由文本定点替换 target；不含 argv）。"""
    safe: Dict[str, Any] = {}
    for key, value in details.items():
        if not isinstance(value, str):
            safe[key] = value
        elif key in ("stdout", "stderr"):
            # 命令输出留存：只做 target 定点替换，保留换行/首尾空白、不截断。
            safe[key] = redact_text(value, target, limit=0, strip=False)
        else:
            safe[key] = redact_text(value, target)
    recorder.append_event(label, safe)


def _exec_step(ctx: _Campaign, op: str, *, label: Optional[str] = None,
               purpose: Optional[str] = None, **params: str) -> hdc.HdcCallRecord:
    """执行一个白名单一次性操作并登记事件（成功/失败都登记，不吞事实）。"""
    record = ctx.executor.execute(op, **params)
    ctx.ops.append(op)
    details: Dict[str, Any] = {
        "exit_code": record.result.exit_code,
        "stdout": record.result.stdout,
        "stderr": record.result.stderr,
    }
    if purpose:
        details["purpose"] = purpose
    if record.result.exit_code != 0:
        details["failed"] = True
        first_line = (record.result.stderr or record.result.stdout or
                      "").strip().splitlines()
        if first_line:
            details["stderr_first_line"] = redact_text(first_line[0], ctx.target)
    _event(ctx.recorder, label or op, ctx.target, **details)
    return record


def _launch_exec(ctx: _Campaign, op: str, **params: str) -> hdc.HdcCallRecord:
    """启动序列步：任何异常/非零退出即中止启动序列（转 host finally cleanup）。"""
    record = _exec_step(ctx, op, **params)
    if record.result.exit_code != 0:
        raise _LaunchAbort("%s exit_code=%d" % (op, record.result.exit_code))
    return record


class _LaunchAbort(Exception):
    """启动序列中止信号（engine 内部控制流；转 host finally，不静默）。"""


# ---------------------------------------------------------------------------
# host finally 各步（顺序冻结；每步独立 guard，失败登记后继续后续 cleanup）
# ---------------------------------------------------------------------------

def _finally_stop_stream(ctx: _Campaign) -> None:
    """步 1：停止 HilogStream（幂等 close，只回收本流子进程组，不触设备）。"""
    was_open = ctx.stream is not None
    if ctx.stream is not None:
        try:
            ctx.stream.close()
        except Exception as exc:  # noqa: BLE001 — 停流失败也继续后续 cleanup
            ctx.step_failures.append("stop-hilogstream: %s"
                                     % redact_text(repr(exc), ctx.target))
    ctx.finally_log.append("1.stop-hilogstream")
    _event(ctx.recorder, "stop-hilogstream", ctx.target, was_open=was_open)


def _finally_faultprobe_recv(ctx: _Campaign) -> None:
    """步 2：FaultProbe 差分 + 仅命中（新增）文件逐个 FaultRecv（原文落 run 目录）。"""
    probe = None
    try:
        probe = _exec_step(ctx, "FaultProbe")
    except Exception as exc:  # noqa: BLE001
        ctx.step_failures.append("FaultProbe: %s"
                                 % redact_text(repr(exc), ctx.target))
    current = None if probe is None else classify_fault_probe(probe.result)
    if probe is not None and probe.result.exit_code != 0:
        ctx.step_failures.append("FaultProbe exit_code=%d"
                                 % probe.result.exit_code)
    if ctx.snapshot_files is None or current is None:
        # 任一 unknown：不 snapshot_diff、零 FaultRecv；经既有
        # aggregate_fault_components(fault_probe_failed=True) → unobservable。
        ctx.faultprobe_failed = True
        ctx.new_files = None
        ctx.step_failures.append(
            "FaultProbe diff unknown (pre=%s final=%s)"
            % ("known" if ctx.snapshot_files is not None else "unknown",
               "known" if current is not None else "unknown"))
        ctx.finally_log.append("2.faultprobe+faultrecv")
        return
    ctx.faultprobe_failed = False
    ctx.new_files = death.snapshot_diff(ctx.snapshot_files, current)
    fault_dir = os.path.join(ctx.recorder.root, FAULTLOGGER_SUBDIR)
    try:
        os.makedirs(fault_dir, exist_ok=True)
        dir_ok = True
    except OSError as exc:
        dir_ok = False
        ctx.step_failures.append("faultlogger-dir: %s" % repr(exc))
    for name in ctx.new_files:
        # m-10 对齐：条目名先过同一穿越拦截；非法名不拼 host 路径、不触设备，
        # 按 FaultRecv 部分失败登记（不中断 finally 序列）。
        if hdc.fault_file_traversal_reason(name) is not None:
            ctx.recv_failures.append(name)
            _event(ctx.recorder, "FaultRecv", ctx.target,
                   fault_file=name, failed=True, reason="traversal-rejected")
            continue
        host_path = os.path.join(fault_dir, name)
        record = None
        try:
            record = _exec_step(ctx, "FaultRecv", fault_file=name,
                                host_path=host_path)
        except hdc.HdcViolation as exc:
            ctx.recv_failures.append(name)
            _event(ctx.recorder, "FaultRecv", ctx.target, fault_file=name,
                   failed=True, reason=redact_text(exc.reason, ctx.target))
            continue
        except Exception as exc:  # noqa: BLE001 — transport 异常 = 取回失败事实
            ctx.recv_failures.append(name)
            ctx.step_failures.append("FaultRecv: %s"
                                     % redact_text(repr(exc), ctx.target))
            continue
        text = None
        if record.result.exit_code == 0 and dir_ok:
            # 真实 file recv 语义：原文在 host 路径文件里；读不到 = 取回失败。
            try:
                with open(host_path, "r", encoding="utf-8",
                          errors="replace") as fh:
                    text = fh.read()
            except OSError:
                text = None
        if text is None:
            ctx.recv_failures.append(name)
        else:
            ctx.received_texts[name] = text
    ctx.finally_log.append("2.faultprobe+faultrecv")


def _finally_pidof_vpn(ctx: _Campaign) -> None:
    """步 3：ForceStop 之前第二次 PidOfVpn 采样（死亡分量的 absent 判定输入）。

    命令失败（exit_code≠0，无论 stdout 是否为空）= 采样不可判 ≠ 进程不在场：
    登记 step_failures 并保持 ``pidof_absent=None``，绝不凭空 stdout 合成
    absent（fail-open 钉）；仅 rc=0 才以 ``stdout.strip()==""`` 判 absent。
    """
    pidof_absent: Optional[bool] = None   # 采样不可判 ≠ absent/在场（宁缺勿误）
    try:
        record = _exec_step(ctx, "PidOfVpn", purpose="finally-death-sample")
        if record.result.exit_code != 0:
            ctx.step_failures.append("PidOfVpn-finally exit_code=%d"
                                     % record.result.exit_code)
        else:
            pidof_absent = record.result.stdout.strip() == ""
    except Exception as exc:  # noqa: BLE001 — 采样失败登记为事实并继续
        ctx.step_failures.append("PidOfVpn-finally: %s"
                                 % redact_text(repr(exc), ctx.target))
        pidof_absent = None
    ctx.pidof_absent = pidof_absent
    ctx.finally_log.append("3.pidofvpn-sample")


def _finally_freeze_death_facts(ctx: _Campaign) -> None:
    """步 4：死亡事实冻结（七分量输入；ForceStop 之前，全部真实取材）。

    整步在 finally 保护下登记本步位次：任何取材异常向 run_campaign 的每步
    guard 外抛登记（清理链不中断），本步 ``finally_log`` 位次仍落档。
    """
    try:
        lines = [cl.raw for cl in ctx.capture.lines] if ctx.capture else []
        ctx.stream_events = core.scan_markers(lines)
        ctx.sites = [fsm.site_of(e.name) for e in ctx.stream_events
                     if fsm.site_of(e.name) is not None]
        ctx.last_site = death.eval_last_visible_site(ctx.sites)
        post_present = any(e.name == "N1BDISC_POST" for e in ctx.stream_events)
        skip_destroy = any(e.name == "N1BDISC_SKIP" and e.kv.get("item") == "destroy"
                           for e in ctx.stream_events)
        t_present = any(e.name == "N1BDISC_DW_DESTROY_T" for e in ctx.stream_events)
        c_present = any(e.name == "N1BDISC_DW_DESTROY_C" for e in ctx.stream_events)
        ctx.destroy_state = death.eval_destroy_call_state(skip_destroy, t_present,
                                                          c_present)
        marker_seq_ok = not run_mod._skip_anchor_conflict(ctx.stream_events)
        ctx.parses = [death.parse_fault_entry(name, text)
                      for name, text in sorted(ctx.received_texts.items())]
        # 快照基线失效（pre FaultProbe 失败）时窗界差分不可归因 → 并入 fault_probe_
        # failed（既有 unobservable(faultrecv-unavailable) 支），不把差分误当新增。
        fault_probe_failed = ctx.faultprobe_failed or not ctx.snapshot_ok
        ctx.components = death.aggregate_fault_components(
            ctx.parses, failed_files=ctx.recv_failures,
            fault_probe_failed=fault_probe_failed, last_visible_site=ctx.last_site,
            marker_seq_ok=marker_seq_ok)
        silence = capture_silence_fact(ctx.capture)
        ctx.death_observed_value = death.eval_process_death(
            positive_baseline=ctx.positive_baseline_pid is not None,
            pidof_absent=ctx.pidof_absent, capture_silent=silence)
        ctx.death_observed = ctx.death_observed_value == "observed-true"
        # 墙钟全部真实取材：死亡证据 = 新增条目设备侧时间戳；最后可见 marker /
        # D8_STORM_BEGIN = 捕获行设备侧时间字面。不可求值 → None → 既有冻结面。
        ctx.death_wall_ms = death_evidence_wall_ms(ctx.parses)
        ctx.last_marker_wall_ms = None
        ctx.begin_capture_wall_ms = None
        for cl in (ctx.capture.lines if ctx.capture else []):
            if cl.marker is None:
                continue
            wall = captured_line_wall_ms(cl.raw)
            if cl.marker.name == "N1BDISC_D8_STORM_BEGIN" \
                    and ctx.begin_capture_wall_ms is None:
                ctx.begin_capture_wall_ms = wall
            ctx.last_marker_wall_ms = wall   # 最后一次覆盖 = 最后可见 marker 行
        ctx.tail_state = death.eval_marker_tail_state(
            post_present=post_present, death_evidence_present=ctx.death_observed,
            death_wall_ms=ctx.death_wall_ms,
            last_marker_wall_ms=ctx.last_marker_wall_ms)
        ctx.signature = death.eval_probe_crash_signature(
            ctx.components.fault_type_observed, ctx.components.signal_observed,
            ctx.last_site, marker_seq_ok=marker_seq_ok)
        ctx.capture_silence = silence
    finally:
        ctx.finally_log.append("4.death-facts-frozen")


def _finally_forcestop(ctx: _Campaign) -> None:
    """步 5：ForceStop（Reason 闭域二值：异常路径 exception-cleanup / 否则 final）。"""
    abnormal = ctx.launch_aborted is not None or (
        ctx.capture is not None
        and ctx.capture.close_reason == cap.CLOSE_REASON_EXCEPTION)
    reason = "exception-cleanup" if abnormal else "final-cleanup"
    try:
        record = _exec_step(ctx, "ForceStop", reason=reason)
        if record.result.exit_code != 0:
            ctx.step_failures.append("ForceStop exit_code=%d"
                                     % record.result.exit_code)
    except Exception as exc:  # noqa: BLE001
        ctx.step_failures.append("ForceStop: %s"
                                 % redact_text(repr(exc), ctx.target))
    ctx.force_stop_reason = reason
    ctx.finally_log.append("5.forcestop")


def _finally_simple(ctx: _Campaign, step_no: int, tag: str, op: str) -> None:
    """步 6/7：Uninstall / RemoveStaging（失败登记后继续）。"""
    try:
        record = _exec_step(ctx, op)
        if record.result.exit_code != 0:
            ctx.step_failures.append("%s exit_code=%d"
                                     % (op, record.result.exit_code))
    except Exception as exc:  # noqa: BLE001
        ctx.step_failures.append("%s: %s" % (op, redact_text(repr(exc),
                                                             ctx.target)))
    ctx.finally_log.append("%d.%s" % (step_no, tag))


_PERMISSION_MARKERS = ("permission denied", "access denied",
                       "operation not permitted")
# absence/presence 一律按**整行语法**识别：关键词大小写不敏感，主语捕获后与精确
# 对象（core.DEFAULT_BUNDLE / hdc.STAGING_ROOT）逐字相等才算——工具自身错误、
# 请求/错误 JSON 回显、兄弟前后缀、其他子路径的行都不构成证据（词拼接不认）。
_BUNDLE_ABSENCE_RE = re.compile(
    r"^\s*(?:error\s*:\s*)?bundle\s+(?P<subject>\S+)\s+"
    r"(?:is\s+not\s+installed|not\s+installed|not\s+found|does\s+not\s+exist)"
    r"\s*[.!]?\s*$",
    re.IGNORECASE)
_BUNDLE_PRESENCE_RE = re.compile(
    r"^\s*bundle[\s_]?name\s*:\s*(?P<value>\S+)\s*$", re.IGNORECASE)
_STAGING_ENOENT_RE = re.compile(
    r"^\s*ls\s*:\s*(?:cannot\s+access\s*)?[\"']?(?P<subject>\S+?)[\"']?"
    r"\s*:\s*"
    r"(?:no\s+such\s+file\s+or\s+directory|no\s+such\s+file|does\s+not\s+exist)"
    r"\s*[.:]?\s*$",
    re.IGNORECASE)
_STAGING_RE = re.compile(r"(?<![A-Za-z0-9._/-])" + re.escape(hdc.STAGING_ROOT)
                         + r"(?![A-Za-z0-9._/-])")
_LS_ENTRY_RE = re.compile(r"^[dlbcps-][rwxStTs-]{9}\s")


def _permission_denied(stdout: str, stderr: str) -> bool:
    low = ("%s\n%s" % (stdout or "", stderr or "")).lower()
    return any(m in low for m in _PERMISSION_MARKERS)


def _text_lines(stdout: str, stderr: str) -> List[str]:
    return ("%s\n%s" % (stdout or "", stderr or "")).splitlines()


def classify_bundle_dump(exit_code: int, stdout: str,
                         stderr: str) -> Optional[bool]:
    """BundleDump 三态：True=已证明本精确 bundle 未安装/不存在；False=已证明在场；
    None=未知。

    True 仅认**主语就是本 bundle 的整行 absence 语法**（``error: bundle <EXACT>
    not found`` / ``bundle <EXACT> is not installed`` / ``bundle <EXACT> does
    not exist``；关键词大小写不敏感，捕获的 subject 与 DEFAULT_BUNDLE 逐字相等，
    兄弟前后缀不算）：裸工具错误（``bm not installed``）、``hdc command not
    found while querying <bundle>``、请求/错误 JSON 回显、``failed querying
    <bundle>`` 等一律 None。False 仅 rc0 且存在真实响应行 ``BundleName:
    <EXACT>``（key-value 整行，值逐字相等）；JSON 里的 bundleName 请求参数不是
    事实。absence 与 presence 证据并存（互相矛盾）→ None，不强行 True。权限/
    访问拒绝优先。
    """
    if _permission_denied(stdout, stderr):
        return None
    lines = _text_lines(stdout, stderr)
    absent = False
    for line in lines:
        m = _BUNDLE_ABSENCE_RE.match(line)
        if m and m.group("subject") == core.DEFAULT_BUNDLE:
            absent = True
    present = False
    if exit_code == 0:
        for line in lines:
            m = _BUNDLE_PRESENCE_RE.match(line)
            if m and m.group("value") == core.DEFAULT_BUNDLE:
                present = True
    if absent and present:
        return None
    if absent:
        return True
    if present:
        return False
    return None


def classify_staging_probe(exit_code: int, stdout: str,
                           stderr: str) -> Optional[bool]:
    """StagingProbe 三态：True=已证明精确 staging 路径不存在；False=已列出该路径；
    None=未知。

    True 仅认 **ls 错误整行且错误主语就是本路径**（``ls: <EXACT>: No such file
    or directory`` / ``ls: cannot access '<EXACT>': No such file or directory``；
    捕获的 subject 与 STAGING_ROOT 逐字相等）：``ls: No such file: /system/bin/ls
    (requested <root>)`` 等工具自身错误、子路径/兄弟路径 ENOENT、无路径 ENOENT
    一律 None。False 仅 rc0 且存在真正 ``ls -ld`` 条目（mode 串开头，允许 symlink
    节点）列出该精确路径；任意含路径的句子不算。矛盾证据并存 → None。权限优先。
    """
    if _permission_denied(stdout, stderr):
        return None
    lines = _text_lines(stdout, stderr)
    absent = False
    for line in lines:
        m = _STAGING_ENOENT_RE.match(line)
        if m and m.group("subject") == hdc.STAGING_ROOT:
            absent = True
    present = False
    if exit_code == 0:
        for line in lines:
            if _STAGING_RE.search(line) and _LS_ENTRY_RE.match(line):
                present = True
    if absent and present:
        return None
    if absent:
        return True
    if present:
        return False
    return None


def classify_pidof(exit_code: int, stdout: str,
                   stderr: str) -> Optional[bool]:
    """PidOf 三态：rc0 且 stdout 空且 stderr 空=True；rc0 有效正整数 PID 列表=False；
    其余（非零/权限/非空 stderr/非 PID 文本/异常）=None。

    权限/访问拒绝优先；`pidof not found` 等文本不得标 present；不裸匹配 not found。
    """
    if _permission_denied(stdout, stderr):
        return None
    if exit_code != 0:
        return None
    if (stderr or "").strip():
        return None
    out = (stdout or "").strip()
    if out == "":
        return True
    toks = out.split()
    if toks and all(t.isdigit() and int(t) > 0 for t in toks):
        return False
    return None


def _finally_absent_probes(ctx: _Campaign) -> None:
    """步 8：四定向 absent 探针，显式三态（True=已证明 absent / False=已证明
    present / None=未知）；``verified_clean`` 仅当**四者全 True**。探测失败/不可判
    记 None + step 失败（含原输出），不静默 pass、不假定 rc≠0 等价 absent。"""
    probes = (("BundleDump", "bundle_dump_absent", classify_bundle_dump),
              ("PidOfPost", "pidof_post_empty", classify_pidof),
              ("PidOfVpnPost", "pidof_vpn_post_empty", classify_pidof),
              ("StagingProbe", "staging_probe_absent", classify_staging_probe))
    for op, key, classify in probes:
        verdict: Optional[bool] = None
        try:
            record = _exec_step(ctx, op, purpose="finally-absent-probe")
            verdict = classify(record.result.exit_code,
                               record.result.stdout, record.result.stderr)
            if verdict is None:
                ctx.step_failures.append(
                    "%s exit_code=%d absent=None stdout=%r stderr=%r"
                    % (op, record.result.exit_code,
                       redact_text(record.result.stdout, ctx.target),
                       redact_text(record.result.stderr, ctx.target)))
        except Exception as exc:  # noqa: BLE001
            ctx.step_failures.append("%s: %s" % (op, redact_text(repr(exc),
                                                                 ctx.target)))
        ctx.absent_checks[key] = verdict
    ctx.verified_clean = all(v is True for v in ctx.absent_checks.values())
    ctx.finally_log.append("8.absent-probes")


def _finally_terminal_judgement(ctx: _Campaign) -> None:
    """步 9：终态判定（观测窗四分支同表；FSM 不可收口 → 登记失败，不编造）。"""
    now_mono = ctx.clock.mono_ms()
    if ctx.mon.closed:
        ctx.close_kind = ctx.mon.close_kind
    else:
        try:
            if ctx.mon.state == "window-open":
                ctx.mon.close_by_host_finally(now_mono, ctx.death_observed)
            elif ctx.mon.state == "allow-wait":
                ctx.mon.expire(now_mono, ctx.death_observed)
            else:
                raise fsm.FsmError("state %r has no close face" % (ctx.mon.state,))
            ctx.close_kind = ctx.mon.close_kind
        except fsm.FsmError as exc:
            ctx.close_kind = None
            ctx.step_failures.append("terminal-judgement: %s" % str(exc))
    ctx.finally_log.append("9.terminal-judgement:" + str(ctx.close_kind))


def _finally_integrity_close(ctx: _Campaign) -> None:
    """步 10：freeze integrity 收口复核（live hook；DryRun 恒 ``integrity={}``）。

    caller 注入的只读 ``revalidate_freeze`` 回调在 host finally 清理全部结束
    之后、正式 verdict 求值 / ``recorder.finish`` 之前恰一次调用。「无抛异常」
    **不**当成功：结论只取回调显式返回的 ``passed``/``violations``（结构违反
    或缺失一律 failed，不编造成功）；回调异常本身转为稳定失败码（不跳过
    封签，不吞事实）。``ctx.integrity`` 结构（仅 live 非空；DryRun/拒启记录
    保持 ``integrity={}`` 字面）::

        {
          "schema_version": 1,
          "manifest_sha256": "<postcheck 时观测的 manifest bytes SHA-256 或 None>",
          "governance_sha256": "<同上，governance bytes>",
          "precheck": "passed" | "failed" | None,   # caller 冻结输入预检结论
          "postcheck": "passed" | "failed",         # 本收口复核真实结论
          "violations": ["<稳定失败码>", ...]        # 非空 → verdict invalid 轴
        }

    失败码经 ``derive_and_judge(freeze_integrity_failures=...)`` 萰入既有
    verdict invalid 优先轴；失败码文本由 caller 提供（稳定 code 形态，不含
    target/argv）。
    """
    ctx.finally_log.append("10.integrity-close")
    if ctx.revalidate_freeze is None:
        _event(ctx.recorder, "integrity-close", ctx.target,
               note="empty; freeze-manifest preflight is bound and sealed "
                    "separately by the CLI (preflight.json)")
        return
    payload: Optional[Dict[str, Any]] = None
    violations: List[str] = []
    try:
        result = ctx.revalidate_freeze()
    except Exception as exc:  # noqa: BLE001 — 回调异常 = 复核失败事实；不跳过封签
        violations.append("revalidate-exception: %s"
                          % redact_text(type(exc).__name__, ctx.target))
    else:
        if isinstance(result, dict):
            payload = result
            violations.extend(str(v) for v in (result.get("violations") or []))
        else:
            violations.append("revalidate-result-malformed")
    if payload is not None:
        ctx.integrity["manifest_sha256"] = payload.get("manifest_sha256")
        ctx.integrity["governance_sha256"] = payload.get("governance_sha256")
        ctx.integrity["precheck"] = payload.get("precheck")
    # 「无抛异常」不当成功：显式 passed 标志 + violations 闭域一致才判 passed。
    callback_passed = payload is not None and payload.get("passed") is True
    if not (callback_passed and not violations) and not violations:
        violations.append("revalidate-postcheck-failed")
    ctx.integrity["violations"] = violations
    ctx.integrity["postcheck"] = ("passed"
                                  if callback_passed and not violations
                                  else "failed")
    _event(ctx.recorder, "integrity-close", ctx.target,
           postcheck=ctx.integrity["postcheck"], violations=len(violations))


# ---------------------------------------------------------------------------
# 公共入口
# ---------------------------------------------------------------------------

def run_campaign(*, executor: hdc.HdcExecutor, target: str, hap_path: str,
                 clock, recorder: recording.RunRecorder, operator_ready: bool,
                 mode: str = "dryrun",
                 capture_timing: Optional[cap.CaptureTiming] = None,
                 before_start_entry: Optional[Callable[[], None]] = None,
                 revalidate_freeze: Optional[Callable[[], Dict[str, Any]]] = None
                 ) -> Dict[str, Any]:
    """一次 campaign 编排：真实 transport 收集 fact → 既有判定面 → 记录封签。

    参数（全部显式 kwargs）：

    - ``executor``：:class:`n1bdisc_hdc.HdcExecutor`（冻结白名单表 + 注入
      transport）；``target``/``hap_path`` 必须与 executor 绑定值逐字一致
      （显式二次核对，防 caller 接线错位）；
    - ``clock``：``mono_ms()``/``wall_ms()`` 毫秒钟（Live =
      :class:`n1bdisc_capture.MonoWallClock`；测试注入 FakeClock）；
    - ``recorder``：caller 构造好的 :class:`n1bdisc_recording.RunRecorder`
      （ID/authorisation 全在 metadata，engine 不生成不消费 ID）；
    - ``operator_ready``：caller 明确确认（规格 :1053）；``False`` → 零操作拒启；
    - ``mode``：``"dryrun"``（is_evidence=False）/ ``"live"``（True）；与
      transport 形态配对校验（见模块 docstring）；
    - ``capture_timing``：仅 :class:`n1bdisc_capture.CaptureTiming` 时间盒
      （测试注入小盒；缺省 = fsm 冻结常量；不产任何 fact 数据）；
    - ``before_start_entry``：可选零参回调，在 StartEntry **发送前恰一次**调用
      （CLI 单次性 claim 状态推进挂点；回调异常 = 启动中止转 host finally，
      StartEntry 不发送、不重试；不影响任何判定/规则逻辑）；
    - ``revalidate_freeze``：live 专用只读零参复核回调，host finally 步 10
      （清理全部结束后、verdict 求值/封签前）恰一次调用，结论真实写入
      ``record.integrity``（结构见 :func:`_finally_integrity_close` docstring；
      DryRun 恒 ``integrity={}``，故 dryrun 模式传入即 :class:`EngineError`）。

    返回记录 dict（与写入 ``result.json`` 的 result 同一字面，另加
    ``sealed``/``terminal``/``run_root`` 溯源键）。记录写入失败即异常外抛，不
    返回"sealed 成功"的记录。所有失败不默默 pass：非零/异常/超时都进
    ``steps.campaign_facts`` 的事实面并经既有闭集进入 verdict。
    """
    if not isinstance(executor, hdc.HdcExecutor):
        raise EngineError("executor must be n1bdisc_hdc.HdcExecutor")
    if not isinstance(target, str) or not target:
        raise EngineError("target must be a non-empty str")
    if not isinstance(hap_path, str) or not hap_path:
        raise EngineError("hap_path must be a non-empty str")
    if executor.target != target or executor.hap_path != hap_path:
        raise EngineError(
            "executor binding mismatch (target/hap_path must match executor)")
    if mode not in MODES:
        raise EngineError("mode must be one of %r" % (MODES,))
    if capture_timing is not None and not isinstance(capture_timing,
                                                     cap.CaptureTiming):
        raise EngineError(
            "capture_timing must be n1bdisc_capture.CaptureTiming or None")
    for name in ("mono_ms", "wall_ms"):
        if not callable(getattr(clock, name, None)):
            raise EngineError("clock must expose mono_ms()/wall_ms()")
    if not isinstance(recorder, recording.RunRecorder):
        raise EngineError("recorder must be n1bdisc_recording.RunRecorder")
    if before_start_entry is not None and not callable(before_start_entry):
        raise EngineError(
            "before_start_entry must be a zero-argument callable or None")
    if revalidate_freeze is not None and not callable(revalidate_freeze):
        raise EngineError(
            "revalidate_freeze must be a zero-argument callable or None")
    if revalidate_freeze is not None and mode != "live":
        raise EngineError(
            "revalidate_freeze is a live-only hook (dryrun integrity stays {})")
    is_evidence = mode == "live"
    transport_is_real = isinstance(executor.transport,
                                   transport_real.RealHdcTransport)
    if is_evidence and not transport_is_real:
        raise EngineError(
            "live mode requires a real hdc transport "
            "(synthetic transports are refused for evidence collection)")
    if not is_evidence and transport_is_real:
        raise EngineError(
            "dryrun mode requires a synthetic transport "
            "(real hdc transport is refused outside live mode)")

    # ------------------------------------------------------------------
    # 拒启路径（零 hdc 操作、零 ID 消费；记录器显式收口）
    # ------------------------------------------------------------------
    if not operator_ready:
        _event(recorder, "campaign-refused", target,
               reason="operator-ready-not-confirmed", mode=mode,
               is_evidence=is_evidence)
        recorder.update_state("refused-operator-not-ready")
        result = _refusal_record(mode, is_evidence)
        recorder.finish(result, terminal="incomplete")
        record = dict(result)
        record.update({"sealed": True, "terminal": "incomplete",
                       "run_root": recorder.root})
        return record

    # ------------------------------------------------------------------
    # 正式路径：launch + capture（异常转 host finally）+ 冻结序 cleanup
    # ------------------------------------------------------------------
    ctx = _Campaign()
    ctx.executor = executor
    ctx.target = target
    ctx.clock = clock
    ctx.recorder = recorder
    ctx.revalidate_freeze = revalidate_freeze
    # live integrity 载荷（结构见 _finally_integrity_close docstring）；DryRun
    # 无 hook → 恒 {}（门 11 原字面）。
    ctx.integrity: Dict[str, Any] = {}
    if revalidate_freeze is not None:
        ctx.integrity = {"schema_version": 1, "manifest_sha256": None,
                         "governance_sha256": None, "precheck": None,
                         "postcheck": "failed", "violations": []}
    ctx.mon = fsm.ObservationWindowFSM()
    ctx.ops: List[str] = []
    ctx.step_failures: List[str] = []
    ctx.finally_log: List[str] = []
    ctx.snapshot_files: Optional[List[str]] = None
    ctx.snapshot_ok = False
    ctx.stream = None
    ctx.capture: Optional[cap.CaptureResult] = None
    ctx.positive_baseline_pid: Optional[str] = None
    ctx.launch_aborted: Optional[str] = None
    ctx.new_files: Optional[List[str]] = None
    ctx.recv_failures: List[str] = []
    ctx.received_texts: Dict[str, str] = {}
    ctx.faultprobe_failed = True
    ctx.pidof_absent: Optional[bool] = None
    ctx.parses: List[Any] = []
    ctx.components = None
    ctx.stream_events: List[Any] = []
    ctx.sites: List[str] = []
    ctx.last_site: Optional[str] = None
    ctx.destroy_state: Optional[str] = None
    ctx.death_observed = False
    ctx.death_observed_value: Optional[str] = None
    ctx.tail_state: Optional[str] = None
    ctx.signature = None
    ctx.capture_silence: Optional[bool] = None
    ctx.death_wall_ms: Optional[int] = None
    ctx.last_marker_wall_ms: Optional[int] = None
    ctx.begin_capture_wall_ms: Optional[int] = None
    ctx.absent_checks: Dict[str, Optional[bool]] = {}
    ctx.verified_clean = False
    ctx.close_kind: Optional[str] = None
    ctx.force_stop_reason: Optional[str] = None
    stream_started_mono_ms: Optional[int] = None

    def _on_line(raw: str, mono_ms: int, wall_ms: int) -> None:
        recorder.append_capture(raw, wall_ms, mono_ms)   # 逐行原文 + 当刻双钟

    def _on_first_marker(marker_ev, mono_ms: int, wall_ms: int) -> None:
        # 唯一 positive 基线（证据规则 2）：首枚关联 marker 开窗后恰一次。
        baseline = executor.execute("PidOfVpn")
        ctx.ops.append("PidOfVpn")
        _event(recorder, "PidOfVpn", target,
               purpose="positive-baseline", exit_code=baseline.result.exit_code)
        ctx.positive_baseline_pid = baseline.result.stdout.strip() or None

    try:
        # P0：operator-ready（真 clock 双记账）→ gate 5 三探针 → pre FaultProbe
        # 快照 → staging → HilogStream（早于 StartEntry）→ StartEntry 恰一次。
        ctx.mon.register_operator_ready(clock.mono_ms(), clock.wall_ms())
        _event(recorder, "operator-ready", target,
               action=fsm.OPERATOR_READY_ACTION)
        recorder.update_state("operator-ready", mode=mode)

        _launch_exec(ctx, "Version")
        _launch_exec(ctx, "ParamModel")
        _launch_exec(ctx, "ParamSoftwareVersion")
        # pre FaultProbe：局部 guard（**不** _launch_exec）——unknown 不得中止发现序列。
        snapshot = None
        try:
            snapshot = _exec_step(ctx, "FaultProbe",
                                  purpose="pre-campaign-snapshot")
        except Exception as exc:  # noqa: BLE001
            ctx.step_failures.append("FaultProbe pre: %s"
                                     % redact_text(repr(exc), ctx.target))
        pre_files = (None if snapshot is None
                     else classify_fault_probe(snapshot.result))
        ctx.snapshot_files = pre_files
        ctx.snapshot_ok = pre_files is not None
        if not ctx.snapshot_ok:
            ctx.faultprobe_failed = True
            ctx.step_failures.append("FaultProbe pre snapshot unknown")
            _event(recorder, "faultprobe-snapshot", target, unknown=True)
            recorder.update_state("gate5-done", snapshot_files=None)
        else:
            _event(recorder, "faultprobe-snapshot", target,
                   files=len(pre_files))
            recorder.update_state("gate5-done", snapshot_files=len(pre_files))

        _launch_exec(ctx, "MkdirStaging")
        _launch_exec(ctx, "SendHap")
        _launch_exec(ctx, "InstallHap")
        recorder.update_state("staged")

        # M-03：实际开流先于 StartEntry；开流成功即 FSM 记账（失败不留记账）。
        ctx.stream = executor.open_stream("HilogStream")
        ctx.ops.append("HilogStream")
        if not callable(getattr(ctx.stream, "read_event", None)):
            # 长驻流必须有界 read_event 契约（capture 消费入口；裸迭代器拒绝）。
            raise _LaunchAbort("HilogStream stream lacks bounded read_event "
                               "contract")
        stream_started_mono_ms = clock.mono_ms()
        ctx.mon.start_hilog_stream(stream_started_mono_ms)
        _event(recorder, "HilogStream", target, phase="opened",
               stream_started_mono_ms=stream_started_mono_ms)
        recorder.update_state("hilog-streaming")

        if before_start_entry is not None:
            # 单次 StartEntry 发送前挂点（CLI claim 状态推进）；异常即启动中止
            # 转 host finally——StartEntry 不发送、不重试（恰一次语义不变）。
            before_start_entry()
        _launch_exec(ctx, "StartEntry")
        ctx.mon.issue_start_entry(clock.mono_ms())
        recorder.update_state(
            "allow-wait", start_entry_mono_ms=ctx.mon.start_entry_mono_ms)

        # 有界捕获（三重到点 + 静默超时事件都在 capture_stream 内）；静默等待
        # 期间 state.json 保持 capture-active 可解释心跳。
        recorder.update_state("capture-active", stage="allow-wait")
        ctx.capture = cap.capture_stream(
            ctx.stream, ctx.mon, clock=clock,
            stream_started_mono_ms=stream_started_mono_ms,
            on_line=_on_line, on_first_marker=_on_first_marker,
            timing=capture_timing)
        _event(recorder, "capture-closed", target,
               close_reason=ctx.capture.close_reason,
               lines=len(ctx.capture.lines), markers=len(ctx.capture.markers),
               stream_exit_code=ctx.capture.stream_exit_code,
               error=ctx.capture.error if ctx.capture.error else None,
               allow_deadline_reached=ctx.capture.allow_deadline_reached,
               window_deadline_reached=ctx.capture.window_deadline_reached,
               wallclock_fallback_reached=ctx.capture.wallclock_fallback_reached)
        recorder.update_state("capture-closed",
                              close_reason=ctx.capture.close_reason,
                              lines=len(ctx.capture.lines),
                              markers=len(ctx.capture.markers))
    except Exception as exc:  # noqa: BLE001 — 启动序列失败转 host finally
        ctx.launch_aborted = redact_text("%s: %s" % (type(exc).__name__, exc),
                                         target)
        _event(recorder, "launch-aborted", target, error=ctx.launch_aborted)
    finally:
        # host finally（:1645-1662 冻结序）。每步独立 guard：任何单步异常（含
        # 记录器落盘失败）登记 step_failures 后继续后续 cleanup——清理链不中
        # 断、不默默 pass，封签必达（不依赖 __del__）。
        for step in (
                _finally_stop_stream,
                _finally_faultprobe_recv,
                _finally_pidof_vpn,
                _finally_freeze_death_facts,
                _finally_forcestop,
                lambda c: _finally_simple(c, 6, "uninstall", "Uninstall"),
                lambda c: _finally_simple(c, 7, "removestaging", "RemoveStaging"),
                _finally_absent_probes,
                _finally_terminal_judgement,
                _finally_integrity_close,
        ):
            try:
                step(ctx)
            except Exception as exc:  # noqa: BLE001 — 单步失败不中断清理链
                ctx.step_failures.append(
                    "finally-step: %s" % redact_text(repr(exc), target))

    # ------------------------------------------------------------------
    # 判定面（完全复用既有 derive_and_judge；join 边界 = Live 冻结值）
    # ------------------------------------------------------------------
    join_facts = {"join_blocked_registered": False, "join_exit_rc": None}
    # live integrity 收口复核失败码 → 既有 verdict invalid 优先轴（:1124）。
    freeze_integrity_failures = (list(ctx.integrity["violations"])
                                 if ctx.integrity else [])
    camp = {
        "stream_events": ctx.stream_events,
        "fsm": ctx.mon,
        "sites": ctx.sites,
        "destroy_state": ctx.destroy_state,
        "death_observed": ctx.death_observed,
        "last_site": ctx.last_site,
        "tail_state": ctx.tail_state,
        "components": ctx.components,
        "signature": ctx.signature,
        "join_facts": join_facts,
    }
    try:
        judged = run_mod.derive_and_judge(
            camp, wall_facts=run_mod.CaptureWallFacts(
                death_wall_ms=ctx.death_wall_ms,
                begin_capture_wall_ms=ctx.begin_capture_wall_ms),
            freeze_integrity_failures=freeze_integrity_failures)
        result_value = judged["verdict_result"]
        pre_present = any(e.name == "N1BDISC_PRE" for e in ctx.stream_events)
        post_present = any(e.name == "N1BDISC_POST" for e in ctx.stream_events)

        record = _campaign_record(
            mode=mode, is_evidence=is_evidence, judged=judged,
            result_value=result_value, ctx=ctx, join_facts=join_facts,
            pre_present=pre_present, post_present=post_present,
            capture_timing=capture_timing, stream_started_mono_ms=
            stream_started_mono_ms)

        terminal = "complete" if ctx.close_kind is not None else "incomplete"
        recorder.finish(record, terminal=terminal)
    except Exception as exc:  # noqa: BLE001 — 尾段兜底：显式 incomplete 封签
        # 落盘（不依赖 __del__、不声称成功），原异常继续外抛。
        _seal_incomplete_tail(recorder, mode=mode, is_evidence=is_evidence,
                              ctx=ctx, target=target, exc=exc)
        raise
    record = dict(record)
    record.update({"sealed": True, "terminal": terminal,
                   "run_root": recorder.root})
    return record


def _seal_incomplete_tail(recorder: recording.RunRecorder, *, mode: str,
                          is_evidence: bool, ctx: _Campaign, target: str,
                          exc: Exception) -> None:
    """尾段兜底封签：判定/组装/``finish`` 任一失败时显式落 incomplete 终态。

    尽力而为：兜底封签自身再失败（如记录器已封或介质故障）时静默让位——
    原异常为准外抛，:meth:`RunRecorder.__del__` 的 best-effort 补写仍在。
    """
    try:
        recorder.finish({
            "engine": ENGINE_ID,
            "mode": mode,
            "is_evidence": is_evidence,
            "refused": False,
            "integrity": dict(ctx.integrity) if ctx.integrity else {},
            "verdict": None,
            "protocol": None,
            "hdc_audit_ops": list(ctx.ops),
            "steps": {"campaign_facts": {
                "host_finally": list(ctx.finally_log),
                "step_failures": list(ctx.step_failures),
                "tail_error": redact_text("%s: %s" % (type(exc).__name__, exc),
                                          target),
            }},
        }, terminal="incomplete")
    except Exception:  # noqa: BLE001 — 兜底失败不掩盖原异常
        pass


# ---------------------------------------------------------------------------
# 记录组装（纯 dict 面；verdict/证据向量形状沿 build_record，engine 增量字段单列）
# ---------------------------------------------------------------------------

def _refusal_record(mode: str, is_evidence: bool) -> Dict[str, Any]:
    return {
        "engine": ENGINE_ID,
        "mode": mode,
        "is_evidence": is_evidence,
        "refused": True,
        "reason": "operator-ready-not-confirmed",
        "verdict": None,
        "protocol": None,
        "hdc_audit_ops": [],
        "integrity": {},
    }


def _campaign_record(*, mode: str, is_evidence: bool, judged: Dict[str, Any],
                     result_value, ctx: _Campaign, join_facts: Dict[str, Any],
                     pre_present: bool, post_present: bool,
                     capture_timing: Optional[cap.CaptureTiming],
                     stream_started_mono_ms: Optional[int]) -> Dict[str, Any]:
    """组装终记录（与 ``finish`` 的 result 同一字面；全部 JSON 可序列化）。"""
    components = ctx.components
    effective_timing = ({"allow_box_s": capture_timing.allow_box_s,
                         "observation_window_s": capture_timing.observation_window_s,
                         "wallclock_fallback_s": capture_timing.wallclock_fallback_s}
                        if capture_timing is not None else "frozen-defaults")
    return {
        "engine": ENGINE_ID,
        "mode": mode,
        "is_evidence": is_evidence,
        "refused": False,
        "integrity": dict(ctx.integrity),   # live=收口复核结论；DryRun 恒 {}（门 11）
        "verdict": result_value.verdict,
        "protocol": verdict_mod.derive_protocol(pre_present, post_present),
        "steps": {
            "gate13_verdict": result_value.as_dict(),
            "campaign_facts": {
                "operator_ready": True,
                "launch_aborted": ctx.launch_aborted,
                "capture": {
                    "close_reason": (ctx.capture.close_reason
                                     if ctx.capture else None),
                    "lines": len(ctx.capture.lines) if ctx.capture else 0,
                    "markers": (len(ctx.capture.markers)
                                if ctx.capture else 0),
                    "first_marker": (ctx.capture.first_marker.name
                                     if ctx.capture
                                     and ctx.capture.first_marker else None),
                    "stream_exit_code": (ctx.capture.stream_exit_code
                                         if ctx.capture else None),
                    "stream_exception": (redact_text(ctx.capture.error,
                                                     ctx.target)
                                         if ctx.capture and ctx.capture.error
                                         else None),
                    "allow_deadline_reached": bool(
                        ctx.capture and ctx.capture.allow_deadline_reached),
                    "window_deadline_reached": bool(
                        ctx.capture and ctx.capture.window_deadline_reached),
                    "wallclock_fallback_reached": bool(
                        ctx.capture and ctx.capture.wallclock_fallback_reached),
                },
                "capture_silence": ctx.capture_silence,
                "positive_baseline_pid": ctx.positive_baseline_pid,
                "snapshot_files": (None if ctx.snapshot_files is None
                                   else list(ctx.snapshot_files)),
                "new_fault_files": (None if ctx.new_files is None
                                    else list(ctx.new_files)),
                "faultrecv_failures": list(ctx.recv_failures),
                "pidof_absent": ctx.pidof_absent,
                "host_finally": list(ctx.finally_log),
                "force_stop_reason": ctx.force_stop_reason,
                "absent_checks": dict(ctx.absent_checks),
                "verified_clean": ctx.verified_clean,
                "close_kind": ctx.close_kind,
                "step_failures": list(ctx.step_failures),
            },
        },
        "evidence_vector": {
            "process_death_observed": ctx.death_observed_value,
            "last_visible_site": ctx.last_site,
            "fault_type_observed": components.fault_type_observed,
            "signal_observed": components.signal_observed,
            "destroy_call_state": ctx.destroy_state,
            "marker_tail_state": ctx.tail_state,
            "probe_crash_signature_observed": ctx.signature.value,
            "raw_predicates": list(components.raw_predicates),
            "unknown_causes": list(components.unknown_causes),
        },
        "ledger": judged["ledger"],
        "dw": judged["dw"],
        "platform_components": judged["platform_components"],
        "d6_items": judged["d6_items"],
        "d8b_closure": {k: v for k, v in judged["d8b_closure"].items()
                        if k in ("branch", "window_start_monotonic",
                                 "window_end_monotonic", "eagain_observed",
                                 "partial_write_observed", "bytes_written_total",
                                 "write_calls", "caps_hit",
                                 "storm_incomplete_pre_only",
                                 "storm_incomplete_pre_only_span", "raw")},
        "time_box_audit": {
            "observation_window_s": fsm.OBSERVATION_WINDOW_S,
            "allow_box_s": fsm.ALLOW_BOX_S,
            "serial_upper_bound_s": fsm.SERIAL_BOX_UPPER_BOUND_S,
            "close_margin_s": fsm.CLOSE_MARGIN_S,
            "wallclock_fallback_s": fsm.HILOG_WALLCLOCK_FALLBACK_S,
            "effective_capture_timing": effective_timing,
            "stream_started_mono_ms": stream_started_mono_ms,
        },
        "hdc_audit_ops": list(ctx.ops),
        "wall_facts": {
            "death_wall_ms": ctx.death_wall_ms,
            "last_marker_wall_ms": ctx.last_marker_wall_ms,
            "begin_capture_wall_ms": ctx.begin_capture_wall_ms,
            "rule": "device-side YYYY-MM-DD hh:mm:ss.mmm literals only "
                    "(fault entry / captured hilog line prefix); None → frozen "
                    "unobservable/undecidable paths; no synthetic values",
        },
        "join_boundary": {
            **join_facts,
            "note": "live boundary (independent T0): join blocking is "
                    "indistinguishable from other main-thread hangs in "
                    "capture; join_blocked_registered/exit_rc are never "
                    "derived from DW_EXIT (no new cause/marker/whitelist)",
        },
    }


__all__ = [
    "ENGINE_ID", "MODES", "EngineError",
    "run_campaign", "redact_text", "parse_fault_snapshot", "device_wall_ms",
    "death_evidence_wall_ms", "captured_line_wall_ms", "capture_silence_fact",
    "FAULTLOGGER_SUBDIR",
]
