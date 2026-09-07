#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc real transport selftests——真实 subprocess + fake 可执行夹具（host-only）。

被测对象：``runner/n1bdisc_transport_real.py``（RealHdcTransport / HdcStream），
契约源 = ``runner/n1bdisc_hdc.py``（只读不动）：``call(argv) → HdcTransportResult
(exit_code, stdout, stderr)``、``open_stream(argv) → 行流``。

全部命令经**真正 subprocess** 启动本目录 ``fixtures/fake_hdc_exec.py`` 的副本
（chmod +x 的纯 stdlib python 夹具，剧本驱动）——**绝不执行任何真实 hdc 命令**
（无 version/kill/list/shell 实体），不改 PATH，夹具临时路径在系统临时目录下、
与真实 SDK 路径严格不同，构造只用夹具副本的绝对路径。

覆盖：command 只收绝对可执行路径；argv 逐字（含带空格路径/token、shell 特殊字
符字面透传 = 无 shell 解释）；call 退出码/流保真/大输出不死锁/有界超时且不遗留
子进程；流 line/partial/EOF（含 partial 冲尾与非零退出码）/静默超时/stderr 满管
道不死锁；close 重复调用、子进程已退出与存活两态、进程组回收证据（/proc 扫描）。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_transport_real.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_transport_real.py
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

import n1bdisc_hdc as hdc            # noqa: E402
import n1bdisc_transport_real as tr  # noqa: E402
import fake_hdc_exec as fixture      # noqa: E402

_ASSERTS = 0

#: 夹具源文件（只读；测试执行的是它的可执行副本）。
FIXTURE_SRC = os.path.join(_HERE, "fixtures", "fake_hdc_exec.py")

#: 典型 HilogStream 形态 argv（transport 不做白名单判断，形态仅求真实）。
HILOG_ARGV = ("-t", "TGT", "shell", "hilog", "-T", "TAG", "-v", "year")


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# 夹具 helpers：可执行副本 / 剧本 / 环境变量注入 / 进程残留扫描 / 死锁护栏
# --------------------------------------------------------------------------

def make_fixture(tmp, subdir=""):
    """把夹具源拷进临时目录并 chmod +x，返回可执行副本的绝对路径。

    临时目录由 :func:`sandbox` 在系统临时目录创建——与真实 SDK 路径严格不同；
    transport 构造只用这里的绝对路径，不经 PATH 解析。
    """
    dst_dir = os.path.join(tmp, subdir) if subdir else tmp
    if subdir:
        os.makedirs(dst_dir)
    dst = os.path.join(dst_dir, "fake_hdc_exec.py")
    shutil.copyfile(FIXTURE_SRC, dst)
    os.chmod(dst, 0o755)
    return dst


def write_scene(tmp, rules):
    """剧本 JSON 落盘，返回路径（经环境变量注入夹具，不经 PATH/argv）。"""
    path = os.path.join(tmp, "scene.json")
    with open(path, "w", encoding="utf-8") as fh:
        json.dump({"rules": rules}, fh, ensure_ascii=False)
    return path


@contextlib.contextmanager
def scene_env(path):
    """临时设置夹具剧本环境变量（退出即还原，不污染其他测试）。"""
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
    """/proc 扫描：cmdline 任一参数以临时目录为前缀的残留进程（有界等待消失）。

    返回残留 pid 列表；空列表 = transport spawn 的子进程全部回收（无僵尸/无
    遗留后台子进程）的进程表证据。
    """
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
    """每用例独立临时目录 + 夹具子进程兜底清场（失败路径也不留后台进程）。"""
    tmp = tempfile.mkdtemp(prefix="n1b-fake-hdc-")
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


def read_until_eof(stream, timeout_s=5.0):
    """有界 read_event 循环 → (lines, eof_event)。"""
    lines = []
    while True:
        ev = stream.read_event(timeout_s)
        expect(ev.kind in ("line", "eof", "timeout"),
               "事件 kind 域外: %r" % (ev.kind,))
        if ev.kind == "line":
            lines.append(ev.line)
        elif ev.kind == "eof":
            return lines, ev
        # timeout：继续等（总时限由 wall_deadline 护栏保证有界）


# ==========================================================================
# A. command 校验（只收绝对可执行路径）
# ==========================================================================

def test_command_validation_absolute_executable_only():
    with sandbox() as tmp:
        # 相对路径拒绝
        try:
            tr.RealHdcTransport(os.path.join("bin", "fake_hdc_exec.py"))
            raise AssertionError("相对路径 command 必须拒绝")
        except tr.HdcTransportError as exc:
            expect("absolute" in str(exc), "相对路径拒绝消息应说明绝对路径要求")
        # 不存在的绝对路径拒绝
        try:
            tr.RealHdcTransport(os.path.join(tmp, "missing-exec"))
            raise AssertionError("不存在的 command 必须拒绝")
        except tr.HdcTransportError:
            pass
        # 存在但无执行位拒绝
        no_exec = make_fixture(tmp, "noexec")
        os.chmod(no_exec, 0o644)
        try:
            tr.RealHdcTransport(no_exec)
            raise AssertionError("无执行位的 command 必须拒绝")
        except tr.HdcTransportError as exc:
            expect("not executable" in str(exc), "无执行位拒绝消息应说明可执行要求")
        # 目录拒绝
        try:
            tr.RealHdcTransport(tmp)
            raise AssertionError("目录 command 必须拒绝")
        except tr.HdcTransportError:
            pass
        # 合法副本可构造（构造零 spawn——残留扫描为空）
        ok = tr.RealHdcTransport(make_fixture(tmp))
        expect(ok.command == make_fixture(tmp) and ok.call_timeout_s > 0,
               "合法 command 构造成功并保留配置")
        expect(leftover_fake_hdc(tmp) == [], "构造期零子进程遗留")


# ==========================================================================
# B. call：argv 逐字 / 退出码与流保真 / 大输出 / 有界超时 / 参数护栏
# ==========================================================================

def test_call_argv_verbatim_with_spaces_and_specials():
    """argv 逐字到子进程：带空格路径与 token、shell 特殊字符原样、无 shell 解释。"""
    with sandbox() as tmp:
        # command 路径本身带空格
        spaced = make_fixture(tmp, os.path.join("dir with spaces", "sub dir"))
        transport = tr.RealHdcTransport(spaced)
        argv = ["-t", "TGT 1", "shell", "bm", "dump", "-n",
                "b;rm|d$e*f", "g h", ""]
        with wall_deadline(20), scene_env(None):   # 无剧本 → 逐字 echo
            result = transport.call(argv)
        expect(isinstance(result, hdc.HdcTransportResult),
               "call 返回原契约 HdcTransportResult")
        expect(result.exit_code == 0, "echo 夹具正常退出")
        echo = json.loads(result.stdout)
        expect(echo == list(argv),
               "argv 逐字到达子进程（空格/分号/管道/美元/星号字面透传，无 shell 解释）")

        # 剧本命中规则同样逐字（echo_argv 先行）
        with scene_env(write_scene(tmp, [
                {"match": ["version"], "echo_argv": True, "exit_code": 0}])):
            result2 = transport.call(("version",))
        expect(json.loads(result2.stdout) == ["version"],
               "剧本规则下 argv 仍逐字 echo")


def test_call_exit_code_and_stream_fidelity():
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        stdout_text = "FakeHdc 0.0.0 汉字✓\nline-two  double space\n"
        stderr_text = "warn 文本 err\nsecond err line\n"
        with scene_env(write_scene(tmp, [{
                "match": ["version"],
                "chunks": [{"data": stdout_text},
                           {"fd": 2, "data": stderr_text}],
                "exit_code": 7}])):
            result = transport.call(("version",))
        expect(result.exit_code == 7, "非零退出码原样透传")
        expect(result.stdout == stdout_text, "stdout 逐字保真（含尾换行/空格/Unicode）")
        expect(result.stderr == stderr_text, "stderr 逐字保真")


def test_call_large_output_no_deadlock():
    """stdout/stderr 大输出（超管道容量）并发写 → communicate 全量排空不死锁。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [{
                # match = argv 前缀（前两个 token 是 -t 与目标）
                "match": ["-t", "TGT", "shell", "dump"],
                "chunks": [{"data": "A", "repeat": 1000000},     # 1 MB stdout
                           {"fd": 2, "data": "B", "repeat": 500000}],  # 0.5 MB stderr
                "exit_code": 0}])):
            with wall_deadline(20):
                result = transport.call(["-t", "TGT", "shell", "dump", "-n", "B"])
        expect(len(result.stdout) == 1000000 and set(result.stdout) == {"A"},
               "1 MB stdout 全量保真（不死锁、不截断）")
        expect(len(result.stderr) == 500000 and set(result.stderr) == {"B"},
               "0.5 MB stderr 全量保真")
        expect(result.exit_code == 0, "大输出用例正常退出")
        expect(leftover_fake_hdc(tmp) == [], "call 结束零子进程遗留")


def test_call_timeout_bounded_no_leak():
    """有界超时：SIGKILL 自身进程组并回收；异常消息不含 target / 完整 argv。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp), call_timeout_s=0.4)
        argv = ("-t", "SECRET-TGT-9", "shell", "pidof", "cn.example.app")
        with scene_env(write_scene(tmp, [
                {"match": ["-t", "SECRET-TGT-9", "shell", "pidof"],
                 "sleep_s": 30, "exit_code": 0}])):
            started = time.monotonic()
            with wall_deadline(15):
                try:
                    transport.call(argv)
                    raise AssertionError("超时必须抛 HdcTransportTimeout")
                except tr.HdcTransportTimeout as timeout_exc:
                    elapsed = time.monotonic() - started
                    timeout_msg = str(timeout_exc)
            expect(elapsed < 5.0, "有界超时 %.2fs 返回（而非等满夹具 30s）" % elapsed)
            msg = timeout_msg
            expect("timeout" in msg, "超时消息说明超时事实")
            for secret in ("SECRET-TGT-9", "cn.example.app", "pidof",
                           "-t", "shell"):
                expect(secret not in msg,
                       "超时消息不泄露 argv token/target: %r" % secret)
        expect(leftover_fake_hdc(tmp) == [],
               "超时路径 SIGKILL + 回收后 /proc 零残留")


def test_call_and_stream_arg_type_guards():
    """argv 必须是非空 str 序列：裸字符串/非 str 元素/空序列一律拒绝，不 spawn。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        for bad in ("version", ["-t", 3, "shell"], [], None,
                    ("-t", b"TGT", "shell")):
            try:
                transport.call(bad)
                raise AssertionError("非法 argv 必须拒绝: %r" % (bad,))
            except tr.HdcTransportError:
                pass
            try:
                transport.open_stream(bad)
                raise AssertionError("非法 argv 的 open_stream 必须拒绝: %r" % (bad,))
            except tr.HdcTransportError:
                pass
        expect(leftover_fake_hdc(tmp) == [], "护栏路径零 spawn")


# ==========================================================================
# C. 流：line/partial/静默超时 / EOF 冲尾与非零退出码 / stderr 满管道 / 迭代兼容
# ==========================================================================

def test_stream_line_partial_and_silent_timeout():
    """partial line 攒缓冲不早发；补全成行；静默无行按 timeout 返回；close 幂等。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [{
                "match": list(HILOG_ARGV[:6]),
                "chunks": [{"data": "hel"},                 # partial，无换行
                           {"delay_ms": 250},
                           {"data": "lo\n"},                # 补全 → "hello"
                           {"delay_ms": 150},
                           {"data": "second\n"},
                           {"data": "tail-partial"}],
                "sleep_s": 30, "exit_code": 0}])):
            with wall_deadline(20):
                stream = transport.open_stream(HILOG_ARGV)
                ev1 = stream.read_event(3.0)
                expect(ev1.kind == "line" and ev1.line == "hello",
                       "partial(hel) 攒缓冲、补全后整行交付 hello（不拆半行）")
                ev2 = stream.read_event(3.0)
                expect(ev2.kind == "line" and ev2.line == "second",
                       "第二行完整交付")
                t0 = time.monotonic()
                ev3 = stream.read_event(0.3)
                expect(ev3.kind == "timeout",
                       "静默无行按 timeout 事件返回（不阻塞、不误造 EOF）")
                expect(0.25 <= time.monotonic() - t0 < 2.0,
                       "timeout 事件约在时限处返回")
                stream.close()
                stream.close()          # 重复 close 幂等
                ev4 = stream.read_event(0.1)
                expect(ev4.kind == "eof", "close 后 read_event 稳定返回 eof")
        expect(leftover_fake_hdc(tmp) == [],
               "close 终止存活夹具（sleep 30）并回收，/proc 零残留")


def test_stream_eof_partial_flush_and_nonzero_exit():
    """EOF 终态：partial 尾行冲出为最后一行；eof 事件携带非零退出码；幂等。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [{
                "match": list(HILOG_ARGV[:6]),
                "chunks": [{"data": "line-a\n"},
                           {"data": "tail-no-newline"}],
                "sleep_s": 0, "exit_code": 3}])):
            with wall_deadline(20):
                stream = transport.open_stream(HILOG_ARGV)
                lines, ev = read_until_eof(stream)
                expect(lines == ["line-a", "tail-no-newline"],
                       "完整行 + EOF 冲出的 partial 尾行按序交付")
                expect(ev.kind == "eof" and ev.exit_code == 3,
                       "eof 事件携带非零退出码 3")
                expect(stream.exit_code == 3, "exit_code 属性可得")
                ev2 = stream.read_event(0.5)
                expect(ev2.kind == "eof" and ev2.exit_code == 3,
                       "终态后重复 read_event 稳定 eof")
                stream.close()          # 子进程已退出：close 不需也不误杀
                stream.close()
        expect(leftover_fake_hdc(tmp) == [], "自然退出 + close 零残留")


def test_stream_stderr_flood_no_deadlock():
    """stderr 数倍于管道容量持续写、stdout 穿插 → 双 fd 同排空，流照常收尾。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        big = "e" * 64                                  # repeat 后逐段 1 MB
        with scene_env(write_scene(tmp, [{
                "match": list(HILOG_ARGV[:6]),
                "chunks": [{"fd": 2, "data": big, "repeat": 16384},
                           {"data": "x\n"},
                           {"fd": 2, "data": big, "repeat": 16384},
                           {"data": "y\n"},
                           {"fd": 2, "data": big, "repeat": 16384}],
                "sleep_s": 0, "exit_code": 0}])):
            with wall_deadline(25):
                stream = transport.open_stream(HILOG_ARGV)
                lines, ev = read_until_eof(stream, timeout_s=10.0)
        expect(lines == ["x", "y"], "stdout 行完整到达（stderr 洪泛不吞行）")
        expect(ev.exit_code == 0, "洪泛用例正常 EOF")
        expect(leftover_fake_hdc(tmp) == [], "流终态回收零残留")


def test_stream_iter_compatibility():
    """``for line in stream`` 兼容原迭代契约：逐行直到 EOF 自然结束。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [{
                "match": list(HILOG_ARGV[:6]),
                "chunks": [{"data": "i1\ni2\n"}],
                "sleep_s": 0, "exit_code": 0}])):
            with wall_deadline(15):
                stream = transport.open_stream(HILOG_ARGV)
                lines = list(stream)
        expect(lines == ["i1", "i2"], "迭代契约逐行产出到 EOF")
        stream.close()
        stream.close()
        expect(leftover_fake_hdc(tmp) == [], "迭代耗尽 + close 零残留")


def test_stream_close_repeated_while_alive_and_after_exit():
    """close 幂等（存活态重复 close）；SIGKILL 后回收、退出码 = -SIGKILL。"""
    with sandbox() as tmp:
        transport = tr.RealHdcTransport(make_fixture(tmp))
        with scene_env(write_scene(tmp, [{
                "match": list(HILOG_ARGV[:6]),
                "sleep_s": 30, "exit_code": 0}])):
            with wall_deadline(20):
                stream = transport.open_stream(HILOG_ARGV)
                ev = stream.read_event(0.3)
                expect(ev.kind == "timeout", "静默长驻流按 timeout 返回（子进程仍在）")
                t0 = time.monotonic()
                stream.close()
                stream.close()
                stream.close()          # 存活态重复 close 幂等
                expect(time.monotonic() - t0 < 5.0, "close 有界返回（SIGKILL 后快速回收）")
                expect(stream.exit_code == -signal.SIGKILL,
                       "close 对存活子进程 SIGKILL，退出码 = -9")
                ev2 = stream.read_event(0.1)
                expect(ev2.kind == "eof", "close 后 read_event 返回 eof")
        expect(leftover_fake_hdc(tmp) == [],
               "存活态 close 后 /proc 零残留（无后台子进程遗留）")


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
    print("n1bdisc transport_real selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
