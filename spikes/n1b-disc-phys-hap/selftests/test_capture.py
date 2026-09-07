#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc capture selftests——有界日志捕获循环（host-only，fake clock + 真 subprocess）。

被测对象：``runner/n1bdisc_capture.py`` 的 :func:`capture_stream`。
依赖接口（只读不动）：``n1bdisc_fsm.ObservationWindowFSM``（feed_marker 记账 /
冻结时间盒常量）、``n1bdisc_core.scan_markers``（冻结关联规则）、
``n1bdisc_transport_real.HdcStream.read_event/close`` 契约。

覆盖：无行 Allow 盒到点；首 marker 开启 525 观测窗 / 无后续输出照窗到点；开流后
825 兜底熔断；POST 即停（不消费余量剧本）；EOF（非零退出码）不当成功；流异常 /
回调异常仍 ``close`` 且保留此前捕获；无关 tag 不触发首 marker；首 marker 回调
恰一次；每行 mono/wall 同一次捕获成对记录；时间盒默认 = fsm 冻结常量。
另有 2 个**真正 subprocess** 用例：经 ``fixtures/fake_hdc_exec.py`` 可执行副本
（剧本驱动、纯 stdlib）启动 RealHdcTransport 长驻流，用内部小时间盒验证有界
返回与进程组回收——**绝不执行任何真实 hdc 命令**，不改 PATH，夹具在系统临时
目录、与真实 SDK 路径严格不同。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_capture.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_capture.py
"""

from __future__ import annotations

import contextlib
import json
import os
import shutil
import signal
import sys
import tempfile
import threading
import time

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, os.pardir, "runner"))
sys.path.insert(1, os.path.join(_HERE, "fixtures"))

import n1bdisc_capture as cap    # noqa: E402
import n1bdisc_core as core      # noqa: E402
import n1bdisc_fsm as fsm        # noqa: E402
import n1bdisc_transport_real as tr  # noqa: E402
import fake_hdc_exec as fixture  # noqa: E402

_ASSERTS = 0

#: 夹具源文件（只读；真实 subprocess 用例执行的是它的可执行副本）。
FIXTURE_SRC = os.path.join(_HERE, "fixtures", "fake_hdc_exec.py")

#: 典型 HilogStream 形态 argv（transport 不做白名单判断，形态仅求真实）。
HILOG_ARGV = ("-t", "TGT", "shell", "hilog", "-T", "TAG", "-v", "year")


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# 测试替身：注入钟 / 剧本化流 / 回调记录器 / FSM 记账 helper / marker 行构造
# --------------------------------------------------------------------------

class FakeClock:
    """注入钟：mono/wall 同步推进（``advance`` 显式推；``auto_step_ms`` 使每次
    读数自推——供"无行也只有 timeout 事件"的到点用例驱动时间流逝）。"""

    def __init__(self, mono_ms=0, wall_ms=0, auto_step_ms=0):
        self._mono = int(mono_ms)
        self._wall = int(wall_ms)
        self._auto = int(auto_step_ms)

    def advance(self, ms):
        self._mono += int(ms)
        self._wall += int(ms)

    def mono_ms(self):
        value = self._mono
        if self._auto:
            self.advance(self._auto)
        return value

    def wall_ms(self):
        value = self._wall
        if self._auto:
            self.advance(self._auto)
        return value


class FakeScriptedStream:
    """HdcStream 契约替身：逐次 ``read_event`` 弹一条剧本；剧本耗尽后永远
    timeout（到点退出只能靠注入钟，不能靠剧本耗尽）；``close`` 记账且幂等。"""

    def __init__(self, clock, script=()):
        self._clock = clock
        self._script = list(script)
        self._pos = 0
        self.read_calls = 0
        self.timeouts_requested = []
        self.close_calls = 0
        self.closed = False

    def read_event(self, timeout_s):
        self.read_calls += 1
        self.timeouts_requested.append(timeout_s)
        if self.closed:
            return tr.HdcStreamEvent("eof", None, None)
        if self._pos >= len(self._script):
            return tr.HdcStreamEvent("timeout", None, None)
        entry = self._script[self._pos]
        self._pos += 1
        kind = entry[0]
        if kind == "line":
            return tr.HdcStreamEvent("line", entry[1], None)
        if kind == "eof":
            return tr.HdcStreamEvent("eof", None, entry[1])
        if kind == "raise":
            raise entry[1]
        if kind == "advance_ms":            # 显式推钟后按 timeout 返回
            self._clock.advance(entry[1])
            return tr.HdcStreamEvent("timeout", None, None)
        if kind == "timeout":
            return tr.HdcStreamEvent("timeout", None, None)
        raise AssertionError("未知剧本条目: %r" % (entry,))

    def close(self):
        self.close_calls += 1
        self.closed = True


class LineRecorder:
    """on_line / on_first_marker 记录器（可选第 N 次 on_line 抛错 / first 抛错）。"""

    def __init__(self, raise_on_line_at=None, raise_on_first=False,
                 exc=None):
        self.lines = []          # (raw, mono_ms, wall_ms)
        self.first = []          # (marker, mono_ms, wall_ms)
        self._raise_at = raise_on_line_at
        self._raise_first = raise_on_first
        self._exc = exc if exc is not None else RuntimeError("callback-boom")

    def on_line(self, raw, mono_ms, wall_ms):
        if self._raise_at is not None and len(self.lines) == self._raise_at:
            raise self._exc
        self.lines.append((raw, mono_ms, wall_ms))

    def on_first_marker(self, marker, mono_ms, wall_ms):
        self.first.append((marker, mono_ms, wall_ms))
        if self._raise_first:
            raise self._exc


def make_ready_mon(start_entry_mono_ms):
    """caller 记账完成态 FSM：operator-ready → 开流（早于 StartEntry）→ StartEntry。"""
    mon = fsm.ObservationWindowFSM()
    mon.register_operator_ready(0, 0)
    mon.start_hilog_stream(int(start_entry_mono_ms) - 1000)
    mon.issue_start_entry(int(start_entry_mono_ms))
    return mon


def marker_line(name, kv=""):
    """entry 形态 tag 的关联 marker 行（经 core.scan_markers 实证可关联）。"""
    message = name + (("|" + kv) if kv else "")
    return "01-01 00:00:00.100 111 222 D %s: %s" % (core.DEFAULT_BUNDLE, message)


PRE_LINE = marker_line("N1BDISC_PRE", "stage=pre")
POST_LINE = marker_line("N1BDISC_POST", "rc=0")
UNRELATED_LINE = "01-01 00:00:00.200 333 444 D other.tag: hello world"


# --------------------------------------------------------------------------
# 真实 subprocess 夹具 helpers（与 test_transport_real.py 同型、自包含复制：
# 夹具副本 / 剧本 / 环境注入 / /proc 残留扫描 / 墙钟护栏）
# --------------------------------------------------------------------------

def make_fixture(tmp, subdir=""):
    dst_dir = os.path.join(tmp, subdir) if subdir else tmp
    if subdir:
        os.makedirs(dst_dir)
    dst = os.path.join(dst_dir, "fake_hdc_exec.py")
    shutil.copyfile(FIXTURE_SRC, dst)
    os.chmod(dst, 0o755)
    return dst


def write_scene(tmp, rules):
    path = os.path.join(tmp, "scene.json")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump({"rules": rules}, fh, ensure_ascii=False)
    return path


@contextlib.contextmanager
def scene_env(path):
    name = fixture.ENV_SCENE
    old = os.environ.get(name)
    if path is None:
        os.environ.pop(name, None)
    else:
        os.environ[name] = path
    try:
        yield
    finally:
        if old is None:
            os.environ.pop(name, None)
        else:
            os.environ[name] = old


def leftover_fake_hdc(tmp_root, timeout_s=3.0):
    """/proc 扫描：cmdline 以临时目录为前缀的残留进程（有界等待消失）。"""
    deadline = time.monotonic() + timeout_s
    while True:
        found = []
        for entry in os.listdir("/proc"):
            if not entry.isdigit():
                continue
            pid = int(entry)
            if pid == os.getpid():
                continue
            try:
                with open("/proc/%d/cmdline" % pid, "rb") as fh:
                    args = [a.decode("utf-8", "replace")
                            for a in fh.read().split(b"\x00") if a]
            except OSError:
                continue
            if any(a.startswith(tmp_root) for a in args):
                found.append(pid)
        if not found:
            return []
        if time.monotonic() >= deadline:
            return found
        time.sleep(0.05)


@contextlib.contextmanager
def sandbox():
    tmp = tempfile.mkdtemp(prefix="n1b-capture-fake-hdc-")
    try:
        yield tmp
    finally:
        for pid in leftover_fake_hdc(tmp, timeout_s=3.0):
            try:
                os.kill(pid, signal.SIGKILL)   # 只清自己 spawn 的夹具进程
            except OSError:
                pass
        shutil.rmtree(tmp, ignore_errors=True)


class DeadlineExceeded(Exception):
    pass


@contextlib.contextmanager
def wall_deadline(seconds):
    """用例级墙钟护栏：超时即判定失败（钉"不死锁/有界返回"）。"""
    if (threading.current_thread() is not threading.main_thread()
            or not hasattr(signal, "setitimer")):
        yield
        return

    def _boom(_signum, _frame):
        raise DeadlineExceeded("wall deadline %gs exceeded (deadlock?)" % seconds)

    old = signal.signal(signal.SIGALRM, _boom)
    signal.setitimer(signal.ITIMER_REAL, seconds)
    try:
        yield
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0.0)
        signal.signal(signal.SIGALRM, old)


# ==========================================================================
# A. 时间盒默认与前置
# ==========================================================================

def test_timing_defaults_are_frozen_constants():
    """CaptureTiming 默认值逐字段 = fsm 冻结常量（不复制新数字分支的钉子）。"""
    timing = cap.CaptureTiming()
    expect(timing.allow_box_s == fsm.ALLOW_BOX_S, "allow 盒默认 = fsm.ALLOW_BOX_S")
    expect(timing.observation_window_s == fsm.OBSERVATION_WINDOW_S,
           "观测窗默认 = fsm.OBSERVATION_WINDOW_S")
    expect(timing.wallclock_fallback_s == fsm.HILOG_WALLCLOCK_FALLBACK_S,
           "墙钟兜底默认 = fsm.HILOG_WALLCLOCK_FALLBACK_S")
    clock = cap.MonoWallClock()
    mono_a = clock.mono_ms()
    wall = clock.wall_ms()
    mono_b = clock.mono_ms()
    expect(isinstance(mono_a, int) and isinstance(wall, int)
           and isinstance(mono_b, int), "默认钟读数为 int 毫秒")
    expect(0 <= mono_a <= mono_b, "单调钟非负且不减")


def test_preconditions_reject_incomplete_accounting():
    """前置违反即 CaptureError：缺 StartEntry 记账 / 非法钟面 / 回调不可调用。"""
    bare = fsm.ObservationWindowFSM()          # 未记账：start_entry_mono_ms 为 None
    stream = FakeScriptedStream(FakeClock())
    recorder = LineRecorder()
    for mon in (bare, make_ready_mon(1_000)):
        for kwargs in (
                dict(stream_started_mono_ms=-1),
                dict(stream_started_mono_ms="1000"),
        ):
            try:
                cap.capture_stream(stream, mon, clock=FakeClock(), on_line=recorder.on_line,
                                   on_first_marker=recorder.on_first_marker, **kwargs)
                raise AssertionError("非法 stream_started_mono_ms 必须拒绝: %r" % (kwargs,))
            except cap.CaptureError:
                pass
    try:
        cap.capture_stream(stream, bare, clock=FakeClock(), stream_started_mono_ms=0,
                           on_line=recorder.on_line,
                           on_first_marker=recorder.on_first_marker)
        raise AssertionError("缺 StartEntry 记账必须拒绝")
    except cap.CaptureError:
        pass
    ready = make_ready_mon(1_000)
    for bad_on_line, bad_on_first in ((None, recorder.on_first_marker),
                                      (recorder.on_line, None)):
        try:
            cap.capture_stream(stream, ready, clock=FakeClock(), stream_started_mono_ms=0,
                               on_line=bad_on_line, on_first_marker=bad_on_first)
            raise AssertionError("不可调用回调必须拒绝")
        except cap.CaptureError:
            pass
    expect(stream.close_calls == 0, "前置拒绝路径不触流、不 close")


# ==========================================================================
# B. 三重到点退出（fake clock + 剧本流）
# ==========================================================================

def test_allow_deadline_with_no_lines():
    """全程无任何 stdout 行：read_event 有界 timeout + 注入钟驱动，Allow 盒到点收口。"""
    clock = FakeClock(mono_ms=0, wall_ms=50_000, auto_step_ms=10_000)
    mon = make_ready_mon(1_000)                # Allow deadline = 301_000
    stream = FakeScriptedStream(clock)          # 剧本空：永远 timeout
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_ALLOW_DEADLINE,
           "无行到 Allow 盒到点收口: %r" % result.close_reason)
    expect(result.allow_deadline_reached and not result.window_deadline_reached
           and not result.wallclock_fallback_reached, "仅 allow 到点标志在场")
    expect(result.lines == [] and result.markers == [], "无捕获事实")
    expect(result.first_marker is None and recorder.first == [],
           "无行不触发首 marker 回调")
    expect(result.close_mono_ms >= 1_000 + fsm.ALLOW_BOX_S * 1000,
           "收口单调读数 ≥ Allow deadline")
    expect(stream.close_calls == 1, "close 恰一次")
    expect(stream.timeouts_requested[0] == (1_000 + fsm.ALLOW_BOX_S * 1000) / 1000.0,
           "首个 read_event 时限 = 到 Allow deadline 的有界 timeout")
    expect(all(0.0 <= t <= (1_000 + fsm.ALLOW_BOX_S * 1000) / 1000.0 + 0.1
               for t in stream.timeouts_requested),
           "每次 read_event 时限都有界（≤ 到下个 deadline）")
    expect(mon.state == "allow-wait" and not mon.closed,
           "FSM 未被 capture 收口（expire 归集成层）")
    expect(result.stream_exit_code is None and result.error is None,
           "非 EOF/异常路径无退出码与异常事实")


def test_first_marker_opens_window_then_window_deadline():
    """首关联 marker 开窗（feed FSM + 恰一次回调）；无后续输出照 525 观测窗到点。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [
        ("line", UNRELATED_LINE),
        ("advance_ms", 5_000),
        ("line", PRE_LINE),
        ("advance_ms", 2_500),                 # 窗内无后续输出
    ])
    recorder = LineRecorder()
    timing = cap.CaptureTiming(allow_box_s=300, observation_window_s=2,
                               wallclock_fallback_s=825)
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker,
                                    timing=timing)
    expect(result.close_reason == cap.CLOSE_REASON_WINDOW_DEADLINE,
           "窗到点收口: %r" % result.close_reason)
    expect(result.window_deadline_reached and not result.allow_deadline_reached
           and not result.wallclock_fallback_reached, "仅 window 到点标志在场")
    expect(len(recorder.first) == 1, "首 marker 回调恰一次")
    marker, mono_ms, wall_ms = recorder.first[0]
    expect(marker.name == "N1BDISC_PRE" and marker.kv.get("stage") == "pre",
           "回调携带首枚关联 marker 事实")
    expect(mono_ms == 5_000 and wall_ms == 5_000, "回调携带交付时刻双钟读数")
    expect(result.first_marker_mono_ms == 5_000 and result.first_marker_wall_ms == 5_000,
           "结果记录首 marker 双钟")
    expect(mon.state == "window-open" and mon.window_start_mono_ms == 5_000
           and mon.first_marker_name == "N1BDISC_PRE",
           "feed_marker 已为 caller 开启观测窗")
    expect(sum(1 for e in mon.events if e.kind == "window-opened") == 1,
           "FSM 恰一次 window-opened")
    expect(len(result.lines) == 2 and result.markers[0].line_no == 2,
           "无关行照捕获且不产 marker；marker 行号 = 捕获坐标")
    expect(stream.timeouts_requested == [301.0, 301.0, 296.0, 2.0],
           "read_event 时限逐次 = 到支配 deadline（开窗后切到窗 deadline）: %r"
           % (stream.timeouts_requested,))
    expect(result.close_mono_ms == 7_500, "窗 deadline(7000) 后首个顶部核对收口")
    expect(stream.close_calls == 1, "close 恰一次")


def test_wallclock_fallback_backstop_mid_capture():
    """开流后兜底熔断：支配 deadline 切到 825 兜底并中途收口，已捕获行保留。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)                # allow deadline = 11_000
    stream = FakeScriptedStream(clock, [
        ("advance_ms", 5_000),
        ("line", PRE_LINE),
        ("advance_ms", 15_000),                # 越过兜底(20_000)、未到窗 deadline
    ])
    recorder = LineRecorder()
    timing = cap.CaptureTiming(allow_box_s=10, observation_window_s=500,
                               wallclock_fallback_s=20)
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker,
                                    timing=timing)
    expect(result.close_reason == cap.CLOSE_REASON_WALLCLOCK_FALLBACK,
           "兜底熔断收口: %r" % result.close_reason)
    expect(result.wallclock_fallback_reached and not result.allow_deadline_reached
           and not result.window_deadline_reached, "仅 fallback 到点标志在场")
    expect(result.first_marker is not None and len(result.lines) == 1,
           "开窗与已捕获行保留")
    expect(stream.close_calls == 1, "兜底路径 close 恰一次")


# ==========================================================================
# C. EOF / 异常 / 回调异常：立即收口、保留捕获、不当成功
# ==========================================================================

def test_eof_with_nonzero_exit_is_not_success():
    """EOF 立即收口且携带非零退出码；结果无任何 success/verdict 语义字段。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [("line", "plain line"), ("eof", 7)])
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_EOF, "EOF 收口原因")
    expect(result.stream_exit_code == 7, "EOF 携带非零退出码事实")
    expect(not result.allow_deadline_reached and not result.window_deadline_reached
           and not result.wallclock_fallback_reached, "EOF 收口无到点标志")
    expect(result.lines and result.lines[0].raw == "plain line", "已捕获行保留")
    fields = set(cap.CaptureResult.__dataclass_fields__)
    expect(not (fields & {"success", "verdict", "pass", "death_observed"}),
           "结果面无 success/verdict/death 字段（判定归集成层）")
    expect(stream.close_calls == 1, "EOF 路径 close 恰一次")


def test_stream_exception_closes_and_preserves():
    """read_event 异常立即收口：异常事实入结果、此前捕获保留、close 照常。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [
        ("line", PRE_LINE),
        ("raise", RuntimeError("stream-boom")),
    ])
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_EXCEPTION, "异常收口原因")
    expect("stream-boom" in (result.error or ""), "异常事实入结果: %r" % result.error)
    expect(len(result.lines) == 1 and result.first_marker is not None,
           "异常前已捕获的行与首 marker 保留")
    expect(stream.close_calls == 1, "异常路径 close 恰一次")


def test_on_line_exception_still_closes_and_preserves():
    """on_line 回调异常：close 在 finally 照常、此前捕获保留、后续剧本不消费。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    boom = RuntimeError("line-callback-boom")
    stream = FakeScriptedStream(clock, [
        ("line", "line-a"),
        ("line", PRE_LINE),                    # 处理该行时 on_line 抛错
        ("line", "never-read"),
        ("eof", 0),
    ])
    recorder = LineRecorder(raise_on_line_at=1, exc=boom)
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_EXCEPTION, "回调异常收口")
    expect("line-callback-boom" in (result.error or ""), "回调异常事实入结果")
    expect([raw for raw, _m, _w in recorder.lines] == ["line-a"],
           "抛错行之前交付的原文已增量回调")
    expect(len(result.lines) == 2, "抛错行本身已捕获（append 先于回调）")
    expect(result.markers == [] and result.first_marker is None
           and mon.window_start_mono_ms is None,
           "on_line 抛错后不做 marker 喂入（窗未开）")
    expect(stream.read_calls == 2, "抛错后立即收口，余量剧本不被消费")
    expect(stream.close_calls == 1, "回调异常仍 close 恰一次")


def test_on_first_marker_exception_still_closes_once():
    """on_first_marker 回调异常：窗已开事实保留、close 照常、回调不重试。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [
        ("line", PRE_LINE),
        ("line", POST_LINE),                   # 不得被消费
        ("eof", 0),
    ])
    recorder = LineRecorder(raise_on_first=True, exc=RuntimeError("first-boom"))
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_EXCEPTION, "首回调异常收口")
    expect("first-boom" in (result.error or ""), "异常事实入结果")
    expect(len(recorder.first) == 1, "首 marker 回调只尝试一次（不重试）")
    expect(result.first_marker is not None and mon.state == "window-open",
           "开窗事实已保留（喂入先于回调）")
    expect(stream.read_calls == 1, "异常即收口，后续行不消费")
    expect(stream.close_calls == 1, "close 恰一次")


# ==========================================================================
# D. 关联规则 / 恰一次 / POST 即停 / 双钟成对
# ==========================================================================

def test_unrelated_tags_never_trigger_first_marker():
    """无关 tag（含消息体提及 marker 字面、无 tag 结构行）不触发任何关联事实。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [
        ("line", "01-01 00:00:00.100 1 2 D other.tag: N1BDISC_PRE|k=1"),
        ("line", "no tag structure at all"),
        ("line", core.DEFAULT_BUNDLE + ": plain boot log mentions N1BDISC_POST literal"),
        ("advance_ms", 2_500),                 # 越过注入的 Allow 小盒(2_000)
    ])
    recorder = LineRecorder()
    timing = cap.CaptureTiming(allow_box_s=1, observation_window_s=1)
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker,
                                    timing=timing)
    expect(result.close_reason == cap.CLOSE_REASON_ALLOW_DEADLINE,
           "无关行耗尽后 Allow 盒照常到点: %r" % result.close_reason)
    expect(recorder.first == [] and result.first_marker is None,
           "无关 tag 不触发首 marker / positive 基线")
    expect(result.markers == [] and mon.window_start_mono_ms is None
           and mon.first_marker_name is None,
           "无关行不喂 FSM、不开窗")
    expect(len(result.lines) == 3, "无关行仍按事实捕获")
    expect(not mon.closed, "FSM 未收口")


def test_first_marker_callback_exactly_once():
    """多枚关联 marker 依次喂入；首 marker 回调全程恰一次。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    second_pre = marker_line("N1BDISC_PRE")
    third = marker_line("N1BDISC_D6S1_B", "step=1")
    stream = FakeScriptedStream(clock, [
        ("line", PRE_LINE),
        ("line", second_pre),
        ("line", third),
        ("eof", 0),
    ])
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(len(recorder.first) == 1 and recorder.first[0][0].name == "N1BDISC_PRE",
           "首 marker 回调全程恰一次且为首枚 marker")
    expect(len(result.markers) == 3, "三枚关联 marker 全部按事实记录")
    expect([m.name for m in result.markers] ==
           ["N1BDISC_PRE", "N1BDISC_PRE", "N1BDISC_D6S1_B"], "marker 时序保真")
    expect(mon.pre_seen and sum(1 for e in mon.events if e.kind == "window-opened") == 1,
           "FSM 只开一次窗、pre_seen 记账")
    expect(result.close_reason == cap.CLOSE_REASON_EOF, "剧本 EOF 收口")


def test_post_marker_stops_and_leftover_unread():
    """POST 在场即停：余量剧本不消费；POST 不关闭 FSM、不产生成功语义。"""
    clock = FakeClock(mono_ms=0, wall_ms=0)
    mon = make_ready_mon(1_000)
    stream = FakeScriptedStream(clock, [
        ("line", PRE_LINE),
        ("line", POST_LINE),
        ("line", "after-post-never"),          # 不得被消费
        ("eof", 0),
    ])
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=0,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect(result.close_reason == cap.CLOSE_REASON_POST_MARKER, "POST 即停收口")
    expect([m.name for m in result.markers] == ["N1BDISC_PRE", "N1BDISC_POST"],
           "PRE/POST 两枚 marker 按序捕获")
    expect(mon.post_seen and not mon.closed,
           "POST 已喂 FSM 但不关闭 FSM（四分支归集成层）")
    expect(stream.read_calls == 2, "POST 后立即停止，余量剧本不被消费")
    expect(result.stream_exit_code is None, "未经 EOF：无退出码事实（无成功语义）")
    expect(result.allow_deadline_reached is False
           and result.window_deadline_reached is False
           and result.wallclock_fallback_reached is False, "POST 收口无到点标志")


def test_mono_and_wall_recorded_per_capture():
    """每行 mono/wall 同一次捕获成对记录：回调与结果逐对一致、close 双钟在结果。"""
    clock = FakeClock(mono_ms=1_000, wall_ms=9_000)
    mon = make_ready_mon(2_000)
    stream = FakeScriptedStream(clock, [
        ("line", "l1"),
        ("advance_ms", 100),
        ("line", "l2"),
        ("advance_ms", 100),
        ("line", PRE_LINE),
        ("eof", 0),
    ])
    recorder = LineRecorder()
    with wall_deadline(15):
        result = cap.capture_stream(stream, mon, clock=clock,
                                    stream_started_mono_ms=1_000,
                                    on_line=recorder.on_line,
                                    on_first_marker=recorder.on_first_marker)
    expect([(r.raw, r.mono_ms, r.wall_ms) for r in result.lines] == recorder.lines,
           "结果行与回调逐对一致（raw + 同一对双钟）")
    expect([pair[1] for pair in recorder.lines] == [1_000, 1_100, 1_200],
           "逐行单调读数随钟推进: %r" % (recorder.lines,))
    expect(recorder.first[0][1] == 1_200 and result.first_marker_wall_ms == 9_200,
           "首 marker 双钟 = 其行交付时刻读数")
    expect(isinstance(result.close_mono_ms, int)
           and isinstance(result.close_wall_ms, int)
           and result.close_mono_ms >= 1_200 and result.close_wall_ms >= 9_200,
           "收口双钟为 int 且 ≥ 最后一次捕获")


# ==========================================================================
# E. 真正 subprocess：fake-HDC 可执行夹具（绝不真实 hdc）
# ==========================================================================

def test_real_subprocess_silent_stream_bounded_and_reaped():
    """真 subprocess 静默流：内部小时间盒有界返回，finally close 回收进程组。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [
                {"match": list(HILOG_ARGV[:6]), "sleep_s": 30, "exit_code": 0}])):
            with wall_deadline(20):
                clock = cap.MonoWallClock()
                started_mono = clock.mono_ms()
                stream = transport.open_stream(HILOG_ARGV)
                mon = make_ready_mon(clock.mono_ms())   # 开流早于 StartEntry
                t0 = time.monotonic()
                result = cap.capture_stream(
                    stream, mon, clock=clock, stream_started_mono_ms=started_mono,
                    on_line=lambda *_a: None,
                    on_first_marker=lambda *_a: (_ for _ in ()).throw(
                        AssertionError("静默流不得触发首 marker")),
                    timing=cap.CaptureTiming(allow_box_s=1, observation_window_s=1,
                                             wallclock_fallback_s=2))
                elapsed = time.monotonic() - t0
        expect(elapsed < 10.0, "有界返回 %.2fs（而非等满夹具 sleep 30s）" % elapsed)
        expect(result.close_reason == cap.CLOSE_REASON_ALLOW_DEADLINE,
               "静默流按注入的 Allow 小盒到点收口: %r" % result.close_reason)
        expect(result.allow_deadline_reached and result.lines == []
               and result.markers == [], "无行无事实、仅 allow 到点标志")
        expect(result.error is None, "有界到点非异常收口")
        expect(leftover_fake_hdc(tmp) == [],
               "finally close SIGKILL + 回收存活夹具，/proc 零残留")


def test_real_subprocess_marker_end_to_end_real_clock():
    """真 subprocess 输出关联 marker：真钟驱动开窗 + 观测窗小盒到点 + 回收。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        payload = ("01-01 00:00:00.100 1 2 D noise.tag: boot\n"
                   + marker_line("N1BDISC_PRE", "stage=pre") + "\n")
        with scene_env(write_scene(tmp, [
                {"match": list(HILOG_ARGV[:6]),
                 "chunks": [{"data": payload}],
                 "sleep_s": 30, "exit_code": 0}])):
            with wall_deadline(20):
                clock = cap.MonoWallClock()
                started_mono = clock.mono_ms()
                stream = transport.open_stream(HILOG_ARGV)
                mon = make_ready_mon(clock.mono_ms())
                recorder = LineRecorder()
                result = cap.capture_stream(
                    stream, mon, clock=clock, stream_started_mono_ms=started_mono,
                    on_line=recorder.on_line, on_first_marker=recorder.on_first_marker,
                    timing=cap.CaptureTiming(allow_box_s=5, observation_window_s=1,
                                             wallclock_fallback_s=10))
        expect(result.close_reason == cap.CLOSE_REASON_WINDOW_DEADLINE,
               "真钟下 PRE 开窗、观测窗小盒到点收口: %r" % result.close_reason)
        expect(result.window_deadline_reached and not result.allow_deadline_reached,
               "仅 window 到点标志在场")
        expect(len(recorder.first) == 1
               and recorder.first[0][0].name == "N1BDISC_PRE",
               "真 subprocess 输出的关联 marker 触发恰一次首 marker 回调")
        expect(result.first_marker_mono_ms >= started_mono,
               "首 marker 单调读数 ≥ 开流读数（真钟同域）")
        expect(any("N1BDISC_PRE" in r.raw for r in result.lines),
               "原文行逐字捕获")
        expect(mon.window_start_mono_ms == result.first_marker_mono_ms,
           "FSM 开窗读数 = capture 首 marker 读数")
        expect(leftover_fake_hdc(tmp) == [], "真 subprocess 用例零进程残留")


# ==========================================================================
# main runner（pytest 不在场时的一条命令入口）
# ==========================================================================

def main() -> int:
    tests = [(name, fn) for name, fn in sorted(globals().items())
             if name.startswith("test_") and callable(fn)]
    passed = failed = 0
    started = time.time()
    for name, fn in tests:
        try:
            fn()
            passed += 1
            print("PASS %s" % name)
        except Exception as exc:  # noqa: BLE001 — main 汇总所有失败后统一退出码
            failed += 1
            print("FAIL %s: %s" % (name, exc))
    elapsed = time.time() - started
    print("-" * 60)
    print("n1bdisc capture selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
