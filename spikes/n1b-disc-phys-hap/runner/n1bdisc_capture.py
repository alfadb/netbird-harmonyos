# -*- coding: utf-8 -*-
"""n1bdisc_capture — N1BDISC host runner 有界日志捕获循环（host-only 增量）。

供后续 Live / fake 共用组装层调用的单函数捕获循环：在**同一主线程**用长驻流
``read_event(到下个 deadline 的有界 timeout)`` 逐事件消费 HilogStream 输出，
注入钟（``clock.mono_ms``/``clock.wall_ms``）驱动
三重到点退出；默认时间盒全部取自 ``n1bdisc_fsm`` 冻结常量（不复制新数字分支）：

- Allow 盒（``ALLOW_BOX_S``，自 ``mon.start_entry_mono_ms`` 起算）：到点未见
  首枚关联 marker → ``allow-deadline`` 收口；
- 求值观测窗（``OBSERVATION_WINDOW_S``，自首枚关联 marker 起算）：到点未见
  POST → ``window-deadline`` 收口；
- 墙钟兜底（``HILOG_WALLCLOCK_FALLBACK_S``，自 ``stream_started_mono_ms`` 起算）：
  runner 异常熔断 → ``wallclock-fallback`` 收口。

本模块只产出**捕获事实**（原文行 + 每行交付时刻的单调/墙钟双读数、关联 marker、
收口原因、到点标志、EOF 退出码、异常事实）；verdict、死亡采样与 host finally 的
``expire``/``close_by_host_finally`` 判定全部由集成层随后处理——这里**不**伪造
``death_observed``，**不**做 F9/四分支分类，也**不**碰 join 派生
（``join_blocked_registered``/``join_exit_rc`` 由 engine 层维护，join 运行侧不可
观察；``N1BDISC_DW_EXIT`` 在此只是被捕获的 marker 字面，不从它推导 joined/blocked）。

marker 关联完全复用 ``n1bdisc_core.scan_markers`` 的冻结规则（tag 路径三形态 +
消息体 ``N1BDISC_`` 前缀，不按 pid 过滤）；每个关联 marker 按读入时刻喂入
``mon.feed_marker``（观测窗开启 / Allow 已消费等 FSM 记账由此发生），随后才触发
``on_first_marker``——该回调恰在**首枚关联 marker 开启观测窗**时触发**恰一次**
（caller 在此做唯一 positive 基线 PidOfVpn；Allow 盒已消费后到达的迟到首 marker
不开启观测窗、不触发回调，caller 由 ``mon.closed``/``mon.close_kind`` 识别）；
无关 tag 不产事件、不喂 FSM、不触发回调；``N1BDISC_POST`` 在场即停止捕获
（POST 不关闭 FSM，四分支判定仍归集成层）。

stream ``EOF``（含非零退出码）/ 任何异常（``read_event``、回调、扫描）都立即
收口，EOF **不**当作成功；收口前已捕获的行全部保留在结果里。``stream.close()``
在 ``finally`` 中调用（回调异常也照常关闭；只回收本流自身子进程组，绝不执行
设备侧 ForceStop / ``hdc kill``）。到点核对在每次循环顶部执行（含每行处理之后
与每个 timeout 事件之后），计时从不被行处理独占。

测试可注入 fake clock 与内部 :class:`CaptureTiming` 小时间盒；生产入口不得经
CLI 开放任意降低时间盒（本模块不提供任何 CLI 面）。本模块不做任何真实 hdc/
设备/网络访问。
"""

from __future__ import annotations

import dataclasses
import time
from dataclasses import dataclass
from typing import Callable, List, Optional

import n1bdisc_core as core
import n1bdisc_fsm as fsm


class CaptureError(Exception):
    """capture 用法前置违反（缺 StartEntry 记账 / 非法钟面 / 回调不可调用）。"""


@dataclass(frozen=True)
class CaptureTiming:
    """三重到点时间盒（默认值 = ``n1bdisc_fsm`` 冻结常量；仅供测试注入覆写）。"""

    #: Allow 盒（自 ``mon.start_entry_mono_ms`` 起算）。
    allow_box_s: int = fsm.ALLOW_BOX_S
    #: 求值观测窗（自首枚关联 marker 起算）。
    observation_window_s: int = fsm.OBSERVATION_WINDOW_S
    #: 开流后墙钟兜底熔断（自 ``stream_started_mono_ms`` 起算）。
    wallclock_fallback_s: int = fsm.HILOG_WALLCLOCK_FALLBACK_S


class MonoWallClock:
    """默认钟：真单调钟 + 墙钟（毫秒整数；``mono_ms`` 须与
    ``stream_started_mono_ms`` 同域，单调钟读数非负口径同 fsm BL-4）。"""

    def mono_ms(self) -> int:
        return int(time.monotonic() * 1000)

    def wall_ms(self) -> int:
        return int(time.time() * 1000)


# -------------------------------------------------------------------------
# 收口原因闭集（capture 循环事实面；与 fsm 的 verdict 域 CLOSE_* 闭集不同层，
# 不新增任何平台 cause / 判定表）。
# -------------------------------------------------------------------------

CLOSE_REASON_POST_MARKER = "post-marker"              # 关联 POST 在场，即停
CLOSE_REASON_EOF = "eof"                              # 流终态（含非零退出码；不是成功）
CLOSE_REASON_EXCEPTION = "exception"                  # read_event/回调/扫描异常事实
CLOSE_REASON_ALLOW_DEADLINE = "allow-deadline"        # Allow 盒到点（含已消费迟到 marker）
CLOSE_REASON_WINDOW_DEADLINE = "window-deadline"      # 观测窗到点未 POST
CLOSE_REASON_WALLCLOCK_FALLBACK = "wallclock-fallback"  # 开流后兜底熔断


@dataclass(frozen=True)
class CapturedLine:
    """单行捕获事实：原文 + 该行交付时刻的单调/墙钟双读数 + 关联 marker（可有）。"""

    line_no: int                            # 本次捕获内 1 起行号
    raw: str                                # 原始行（去行尾换行）
    mono_ms: int
    wall_ms: int
    marker: Optional[core.MarkerEvent] = None


@dataclass
class CaptureResult:
    """捕获结果（纯事实面：无 verdict / 无 death_observed / 无 join 派生）。"""

    lines: List[CapturedLine]
    markers: List[core.MarkerEvent]            # 关联 marker 时序（line_no = 捕获坐标）
    first_marker: Optional[core.MarkerEvent]   # 开启观测窗的首枚关联 marker
    first_marker_mono_ms: Optional[int]
    first_marker_wall_ms: Optional[int]
    close_reason: str                          # CLOSE_REASON_* 闭集
    close_mono_ms: int
    close_wall_ms: int
    allow_deadline_reached: bool               # 到点标志（本次收口的支配盒）
    window_deadline_reached: bool
    wallclock_fallback_reached: bool
    stream_exit_code: Optional[int]            # EOF 时的子进程退出码；非 EOF 为 None
    error: Optional[str]                       # CLOSE_REASON_EXCEPTION 时的异常事实


def capture_stream(
    stream,
    mon,
    *,
    clock,
    stream_started_mono_ms: int,
    on_line: Callable[[str, int, int], None],
    on_first_marker: Callable[[core.MarkerEvent, int, int], None],
    bundle: str = core.DEFAULT_BUNDLE,
    timing: Optional[CaptureTiming] = None,
) -> CaptureResult:
    """有界日志捕获循环（单主线程；契约详见模块 docstring）。

    ``stream`` 为长驻流（HdcStream 契约：有界 ``read_event(timeout_s)`` 三事件
    line/eof/timeout + 幂等 ``close()``）；``mon`` 为 :class:`ObservationWindowFSM`，
    caller 必须已完成 operator-ready / start-entry 记账（开流早于 StartEntry 由
    集成层保证，本函数不复核位次）。``stream_started_mono_ms`` 与
    ``clock.mono_ms()`` 同单调钟域且非负（BL-4）。前置违反即抛
    :class:`CaptureError`，不产生部分结果。

    逐行次序：先 ``on_line(raw, mono_ms, wall_ms)`` 增量交付原文 + 同一交付时刻
    双钟读数，随后才做 marker 喂入与首 marker 回调；任何一步异常都按 ``exception``
    事实收口并保留此前捕获。本函数不抛流/回调异常（一律进结果），不调用
    ``mon.expire``/``close_by_host_finally``，不产生 death_observed / F9 分类。
    """
    if timing is None:
        timing = CaptureTiming()
    if not isinstance(timing, CaptureTiming):
        raise CaptureError("timing 必须是 CaptureTiming（测试注入小时间盒用）")
    start_entry_mono_ms = getattr(mon, "start_entry_mono_ms", None)
    if not isinstance(start_entry_mono_ms, int):
        raise CaptureError(
            "capture 前置缺失：mon.start_entry_mono_ms 必须已由 caller 记账"
            "（operator-ready/StartEntry 之后）")
    if not isinstance(stream_started_mono_ms, int) or stream_started_mono_ms < 0:
        raise CaptureError(
            "stream_started_mono_ms 必须为非负 int（与 clock.mono_ms 同单调钟域；BL-4）")
    if not callable(on_line) or not callable(on_first_marker):
        raise CaptureError("on_line / on_first_marker 必须可调用")

    allow_deadline = start_entry_mono_ms + timing.allow_box_s * 1000
    fallback_deadline = stream_started_mono_ms + timing.wallclock_fallback_s * 1000

    lines: List[CapturedLine] = []
    markers: List[core.MarkerEvent] = []
    first_marker = None
    first_marker_mono_ms = None
    first_marker_wall_ms = None
    window_deadline = None            # 首枚关联 marker 开启观测窗后才有
    stream_exit_code = None
    error = None
    close_reason = None
    allow_hit = window_hit = fallback_hit = False

    try:
        while True:
            now_mono = clock.mono_ms()
            # 顶部到点核对：每次循环（含每行处理之后与每个 timeout 事件之后）都
            # 核对，read_event 的时限 = 到下个支配 deadline，计时从不被行处理独占。
            if window_deadline is None:
                candidates = [(allow_deadline, CLOSE_REASON_ALLOW_DEADLINE)]
            else:
                candidates = [(window_deadline, CLOSE_REASON_WINDOW_DEADLINE)]
            candidates.append((fallback_deadline, CLOSE_REASON_WALLCLOCK_FALLBACK))
            hits = [(d, r) for d, r in candidates if now_mono >= d]
            if hits:
                close_reason = min(hits)[1]     # 最早到点者为收口原因
                hit_names = set(r for _d, r in hits)
                allow_hit = CLOSE_REASON_ALLOW_DEADLINE in hit_names
                window_hit = CLOSE_REASON_WINDOW_DEADLINE in hit_names
                fallback_hit = CLOSE_REASON_WALLCLOCK_FALLBACK in hit_names
                break
            next_deadline = min(d for d, _r in candidates)
            timeout_s = max(0.0, (next_deadline - now_mono) / 1000.0)
            event = stream.read_event(timeout_s)
            if event.kind == "line":
                mono_ms = clock.mono_ms()
                wall_ms = clock.wall_ms()       # 与 mono 同一交付时刻的双钟读数
                line_no = len(lines) + 1
                found = core.scan_markers([event.line], bundle)
                marker_ev = dataclasses.replace(found[0], line_no=line_no) if found else None
                rec = CapturedLine(line_no=line_no, raw=event.line,
                                   mono_ms=mono_ms, wall_ms=wall_ms, marker=marker_ev)
                lines.append(rec)
                on_line(event.line, mono_ms, wall_ms)   # 增量原文 + 两钟
                if marker_ev is not None:
                    markers.append(marker_ev)
                    mon.feed_marker(marker_ev.name, mono_ms)  # FSM 记账（开窗/已消费/late）
                    if first_marker is None and mon.state == "window-open":
                        # 本 marker 开启观测窗：首 marker 事实 + caller 唯一
                        # positive 基线钩子（恰一次）。
                        first_marker = marker_ev
                        first_marker_mono_ms = mono_ms
                        first_marker_wall_ms = wall_ms
                        window_deadline = mono_ms + timing.observation_window_s * 1000
                        on_first_marker(marker_ev, mono_ms, wall_ms)
                    if marker_ev.name == "N1BDISC_POST":
                        close_reason = CLOSE_REASON_POST_MARKER
                        break
                    if mon.closed:
                        # feed_marker 判定 Allow 已消费（迟到首 marker）：观测窗
                        # 不再开启，按 Allow 到点收口。
                        close_reason = CLOSE_REASON_ALLOW_DEADLINE
                        allow_hit = True
                        break
            elif event.kind == "eof":
                stream_exit_code = event.exit_code
                close_reason = CLOSE_REASON_EOF
                break
            # timeout 事件：回循环顶部先核对 deadline（有界读不独占计时）
    except Exception as exc:  # noqa: BLE001 — 异常是捕获事实，交集成层处置
        error = repr(exc)
        close_reason = CLOSE_REASON_EXCEPTION
    finally:
        close_mono_ms = clock.mono_ms()
        close_wall_ms = clock.wall_ms()
        try:
            stream.close()   # 即使回调异常也收口；只回收本流子进程组，不触设备
        except Exception as close_exc:
            if error is None:
                error = "stream.close() raised: %r" % (close_exc,)
        if close_reason is None:   # 防御：所有 break 路径均已置原因，理论不可达
            close_reason = CLOSE_REASON_EXCEPTION
            if error is None:
                error = "capture loop exited without close reason"

    return CaptureResult(
        lines=lines,
        markers=markers,
        first_marker=first_marker,
        first_marker_mono_ms=first_marker_mono_ms,
        first_marker_wall_ms=first_marker_wall_ms,
        close_reason=close_reason,
        close_mono_ms=close_mono_ms,
        close_wall_ms=close_wall_ms,
        allow_deadline_reached=allow_hit,
        window_deadline_reached=window_hit,
        wallclock_fallback_reached=fallback_hit,
        stream_exit_code=stream_exit_code,
        error=error,
    )


__all__ = [
    "CaptureError", "CaptureTiming", "MonoWallClock",
    "CLOSE_REASON_POST_MARKER", "CLOSE_REASON_EOF", "CLOSE_REASON_EXCEPTION",
    "CLOSE_REASON_ALLOW_DEADLINE", "CLOSE_REASON_WINDOW_DEADLINE",
    "CLOSE_REASON_WALLCLOCK_FALLBACK",
    "CapturedLine", "CaptureResult", "capture_stream",
]
