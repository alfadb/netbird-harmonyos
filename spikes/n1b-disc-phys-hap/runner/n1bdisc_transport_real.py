# -*- coding: utf-8 -*-
"""n1bdisc_transport_real — 真实 subprocess HDC transport（host-only 增量）。

契约来源：``runner/n1bdisc_hdc.py``（只读，本增量不改动）——
:class:`n1bdisc_hdc.HdcTransport`（``call(argv)`` 一次性命令 /
``open_stream(argv)`` 长驻流，仅 HilogStream）与
:class:`n1bdisc_hdc.HdcTransportResult`（``(exit_code, stdout, stderr)``，文本保真）。
transport 只执行 executor（白名单执行器）构造并传入的已校验 argv。

安全与进程边界（测试钉：``selftests/test_transport_real.py``）：

- ``command`` 只接受**绝对可执行路径**（存在 + 普通文件 + 可执行位）；argv 以
  list 逐字交给 ``subprocess``（``shell=False``），不拼 shell 字符串、不经 PATH
  解析；
- 不重试、不探测、不启动 HDC server、不附加任何给定 argv 之外的命令；
- ``call`` 有界墙钟超时（构造参数 ``call_timeout_s``）；stdout/stderr 全量保真；
  超时/启动失败的异常消息只含命令路径与阶段，**不含 target / 完整 argv**；
- ``open_stream`` 返回 :class:`HdcStream`：主线程可调用的有界
  ``read_event(timeout_s)``（line / eof / timeout 三事件），``selectors`` 同时
  排空 stdout 与 stderr——stderr 大输出不死锁、静默无行按 timeout 返回、
  partial line 在补全或 EOF 时交付；``for line in stream`` 仍兼容原迭代契约
  （后续 Live 消费必须走有界 ``read_event``）；
- 子进程以 ``start_new_session=True`` 落入自身进程组；超时/``close`` 只对该组
  SIGKILL 并回收——绝不调用 ``hdc kill``，绝不触及不相关进程；终态与显式
  关闭都回收进程，无后台子进程遗留。

单线程实现：无监测线程，仅 ``selectors`` + ``os.read`` 标准库方案。
"""

from __future__ import annotations

import codecs
import os
import selectors
import signal
import subprocess
import time
from typing import Iterator, List, NamedTuple, Optional, Sequence

import n1bdisc_hdc as hdc

#: ``call`` 默认有界超时（秒）；Live 调用方按各操作预算经构造参数覆写。
DEFAULT_CALL_TIMEOUT_S = 30.0
#: EOF 终态回收与 SIGKILL 后 wait 的宽限（秒）；超宽限按异常上报，不挂死主线程。
_REAP_GRACE_S = 2.0
#: 每轮 ``os.read`` 的块大小。
_READ_CHUNK = 65536


class HdcTransportError(Exception):
    """transport 执行失败（非法参数/启动失败/回收异常）。消息不含 target 与完整 argv。"""


class HdcTransportTimeout(HdcTransportError):
    """有界超时触发：子进程组已 SIGKILL 并回收，输出不交付。"""


class HdcStreamEvent(NamedTuple):
    """:meth:`HdcStream.read_event` 的一个事件。

    ``kind`` 三值：``"line"``（``line`` = 去行尾换行的一行）/ ``"eof"``（流终态，
    ``exit_code`` = 子进程退出码，信号死亡为负值）/ ``"timeout"``（时限内无行且
    流未终态，子进程可能仍在运行）。
    """

    kind: str
    line: Optional[str] = None
    exit_code: Optional[int] = None


def _kill_group(proc: subprocess.Popen) -> None:
    """SIGKILL ``proc`` 自身 spawn 的进程组并保证可回收。

    子进程经 ``start_new_session=True`` spawn，即自身进程组组长——``killpg``
    恰好覆盖本 transport 的子进程（及它自己派生的后代），不触及不相关进程；
    绝不构造、调用或模拟 ``hdc kill``。已退出（``poll()`` 不为 None）时是 no-op。
    """
    if proc.poll() is not None:
        return
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except (ProcessLookupError, PermissionError):
        pass  # 组已消亡 = 目标达成


def _wait_reaped(proc: subprocess.Popen, grace_s: float) -> int:
    """有界等待回收；宽限后对自己的进程组补 SIGKILL（防挂死，不留僵尸）。"""
    try:
        return int(proc.wait(timeout=grace_s))
    except subprocess.TimeoutExpired:
        _kill_group(proc)
        return int(proc.wait())


def _decode(data: Optional[bytes]) -> str:
    """字节 → 文本（UTF-8、非法序列替换），stdout/stderr 按原契约保真为 str。"""
    if not data:
        return ""
    return data.decode("utf-8", errors="replace")


class RealHdcTransport(hdc.HdcTransport):
    """真实 hdc transport：``[command] + executor argv`` 逐字 subprocess 执行。

    不做任何白名单判断（那是 executor + transport 侧反向校验的职责），也不实现
    fake-hdc 的剧本语义——只负责把已校验 argv 安全地交给真实可执行文件。
    """

    def __init__(self, command: str,
                 call_timeout_s: float = DEFAULT_CALL_TIMEOUT_S) -> None:
        if not isinstance(command, str) or not command:
            raise HdcTransportError(
                "command must be a non-empty path string")
        if not os.path.isabs(command):
            raise HdcTransportError(
                "command must be an absolute executable path (relative rejected)")
        if not os.path.isfile(command):
            raise HdcTransportError(
                "command is not an existing file: %s" % command)
        if not os.access(command, os.X_OK):
            raise HdcTransportError(
                "command is not executable: %s" % command)
        try:
            timeout = float(call_timeout_s)
        except (TypeError, ValueError):
            raise HdcTransportError(
                "call_timeout_s must be a positive number") from None
        if not timeout > 0:
            raise HdcTransportError("call_timeout_s must be positive")
        self.command = command
        self.call_timeout_s = timeout

    # ------------------------------------------------------------------
    # 内部：argv 校验与 spawn（shell=False、argv list、start_new_session）
    # ------------------------------------------------------------------

    @staticmethod
    def _check_argv(argv: Sequence[str]) -> List[str]:
        if isinstance(argv, (str, bytes)):
            raise HdcTransportError(
                "argv must be a sequence of str tokens, not a bare string")
        try:
            items = list(argv)
        except TypeError:
            raise HdcTransportError(
                "argv must be a sequence of str tokens") from None
        if not items:
            raise HdcTransportError("argv must be non-empty")
        if not all(isinstance(t, str) for t in items):
            raise HdcTransportError("argv tokens must all be str")
        return items

    def _spawn(self, argv: Sequence[str]) -> subprocess.Popen:
        full_argv = [self.command] + self._check_argv(argv)
        try:
            # shell=False + argv list：逐字传递，无 shell 解释；start_new_session
            # 使子进程自成进程组（超时/关闭时 killpg 恰好只覆盖自己的子进程）。
            return subprocess.Popen(
                full_argv,
                shell=False,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True)
        except OSError as exc:
            raise HdcTransportError(
                "failed to launch hdc executable %s: %s"
                % (self.command, exc.strerror or type(exc).__name__)) from None

    # ------------------------------------------------------------------
    # transport 接口：call（一次性命令，有界超时）
    # ------------------------------------------------------------------

    def call(self, argv: Sequence[str]) -> hdc.HdcTransportResult:
        proc = self._spawn(argv)
        try:
            out_b, err_b = proc.communicate(timeout=self.call_timeout_s)
        except subprocess.TimeoutExpired:
            # 有界超时：SIGKILL 自身进程组并回收；异常消息不含 target/argv。
            _kill_group(proc)
            try:
                proc.communicate(timeout=_REAP_GRACE_S)
            except subprocess.TimeoutExpired:
                raise HdcTransportError(
                    "hdc child survived SIGKILL after call timeout") from None
            raise HdcTransportTimeout(
                "hdc command exceeded bounded timeout (%gs); "
                "child group killed and reaped" % self.call_timeout_s) from None
        except BaseException:
            _kill_group(proc)   # 任意异常路径都不遗留子进程
            raise
        return hdc.HdcTransportResult(int(proc.returncode),
                                      _decode(out_b), _decode(err_b))

    # ------------------------------------------------------------------
    # transport 接口：open_stream（长驻流，仅 HilogStream）
    # ------------------------------------------------------------------

    def open_stream(self, argv: Sequence[str]) -> "HdcStream":
        return HdcStream(self._spawn(argv))


class HdcStream:
    """长驻流（selectors 单线程实现，无监测线程）。

    - :meth:`read_event`（主线程、有界）：完整行 / EOF 终态 / 静默超时三事件；
      stdout 与 stderr 同时登记排空，stderr 大输出不会塞死管道、静默无行按
      timeout 返回；partial line 攒在行缓冲，补全为行、EOF 时作为最后一行冲出；
    - EOF 终态事件携带退出码（信号死亡为负值）；未随 EOF 退出的子进程按
      :data:`_REAP_GRACE_S` 宽限后对自己的进程组 SIGKILL 并回收；
    - :meth:`close` 幂等：只终止并回收自身 spawn 的进程组，不调 ``hdc kill``；
    - ``__iter__`` 兼容原 ``for line in stream`` 契约（无超时阻塞至 EOF）。
    """

    def __init__(self, proc: subprocess.Popen) -> None:
        self._proc = proc
        self._selector = selectors.DefaultSelector()
        self._dec_out = codecs.getincrementaldecoder("utf-8")("replace")
        self._dec_err = codecs.getincrementaldecoder("utf-8")("replace")
        self._buf_out = ""                  # stdout 未换行的 partial 残段
        self._buf_err = ""                  # stderr 残段（行内容丢弃）
        self._pending: List[str] = []       # 已就绪完整行（FIFO）
        self._out_open = proc.stdout is not None
        self._err_open = proc.stderr is not None
        self._eof_tail_flushed = False
        self._eof_seen = False
        self._exit_code: Optional[int] = None
        self._closed = False
        for fobj in (proc.stdout, proc.stderr):
            if fobj is not None:
                self._selector.register(fobj, selectors.EVENT_READ)

    # ------------------------------------------------------------------
    # 有界事件读取（Live 消费入口）
    # ------------------------------------------------------------------

    def read_event(self, timeout_s: Optional[float]) -> HdcStreamEvent:
        """等待下一个事件；``timeout_s=None`` 表示不限时（迭代兼容路径用）。

        返回 :class:`HdcStreamEvent`：``line`` / ``eof`` / ``timeout``；终态
        （EOF 已交付或 :meth:`close` 之后）重复调用稳定返回 ``eof``。
        """
        if self._closed or self._eof_seen:
            return HdcStreamEvent("eof", None, self._exit_code)
        deadline = None
        if timeout_s is not None:
            deadline = time.monotonic() + max(0.0, float(timeout_s))
        while True:
            if self._pending:
                return HdcStreamEvent("line", self._pending.pop(0), None)
            if not self._out_open and not self._err_open:
                if self._flush_eof_tail():
                    continue        # EOF 冲出的 partial 尾行 → 按 line 交付
                return self._finalize_eof()
            if deadline is None:
                remaining = None
            else:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    return HdcStreamEvent("timeout", None, None)
            for key, _mask in self._selector.select(remaining):
                self._drain_fd(key.fileobj)

    # ------------------------------------------------------------------
    # 兼容原 transport 迭代契约
    # ------------------------------------------------------------------

    def __iter__(self) -> Iterator[str]:
        """``for line in stream``：逐行阻塞至 EOF（旧契约兼容）。

        后续 Live 消费必须用有界 :meth:`read_event`，不得用本迭代器做长驻采集。
        """
        while True:
            event = self.read_event(None)
            if event.kind == "line":
                yield event.line
            elif event.kind == "eof":
                return

    # ------------------------------------------------------------------
    # 关闭与回收
    # ------------------------------------------------------------------

    def close(self) -> None:
        """幂等关闭：SIGKILL 自身 spawn 的进程组（若仍在）并回收，不调 ``hdc kill``。"""
        if self._closed:
            return
        self._closed = True
        _kill_group(self._proc)
        for fobj in (self._proc.stdout, self._proc.stderr):
            if fobj is not None and not fobj.closed:
                self._unregister(fobj)
                try:
                    fobj.close()
                except OSError:
                    pass
        try:
            self._selector.close()
        except Exception:
            pass
        rc = _wait_reaped(self._proc, _REAP_GRACE_S)
        if self._exit_code is None:
            self._exit_code = rc

    @property
    def exit_code(self) -> Optional[int]:
        """子进程退出码（EOF 终态或 close 后可得；信号死亡为负值，未知为 None）。"""
        return self._exit_code

    def __enter__(self) -> "HdcStream":
        return self

    def __exit__(self, *_exc) -> None:
        self.close()

    def __del__(self) -> None:  # 兜底：调用方忘 close 也不遗留子进程
        try:
            self.close()
        except Exception:
            pass

    # ------------------------------------------------------------------
    # 内部：fd 排空 / 行切分 / EOF 终态
    # ------------------------------------------------------------------

    def _unregister(self, fobj) -> None:
        try:
            self._selector.unregister(fobj)
        except (KeyError, ValueError):
            pass

    def _drain_fd(self, fobj) -> None:
        """读一次已就绪 fd；EOF（0 字节或 fd 失效）即注销关闭该管道。"""
        try:
            chunk = os.read(fobj.fileno(), _READ_CHUNK)
        except OSError:
            chunk = b""     # fd 失效视同 EOF（本 transport 是唯一读方，无竞态阻塞）
        if not chunk:
            self._unregister(fobj)
            try:
                fobj.close()
            except OSError:
                pass
            if fobj is self._proc.stdout:
                self._out_open = False
            else:
                self._err_open = False
            return
        if fobj is self._proc.stdout:
            self._buf_out += self._dec_out.decode(chunk)
            self._buf_out = self._split_lines(self._buf_out, keep=True)
        else:
            # stderr 只排空防塞死，行内容不进事件流（call 路径才承载 stderr 契约）
            self._buf_err += self._dec_err.decode(chunk)
            self._buf_err = self._split_lines(self._buf_err, keep=False)

    def _split_lines(self, buf: str, keep: bool) -> str:
        """把 buf 中的完整行（'\\n'）切进事件队列，返回未完成的残段。"""
        while "\n" in buf:
            line, buf = buf.split("\n", 1)
            if line.endswith("\r"):
                line = line[:-1]
            if keep:
                self._pending.append(line)
        return buf

    def _flush_eof_tail(self) -> bool:
        """双侧管道 EOF 时把 stdout 的 partial 尾行冲入事件队列；只执行一次。"""
        if self._eof_tail_flushed:
            return False
        self._eof_tail_flushed = True
        self._buf_out += self._dec_out.decode(b"", final=True)
        self._dec_err.decode(b"", final=True)   # stderr 解码器状态冲掉（内容丢弃）
        self._buf_err = ""
        if self._buf_out:
            if self._buf_out.endswith("\r"):
                self._buf_out = self._buf_out[:-1]
            self._pending.append(self._buf_out)
            self._buf_out = ""
            return True
        return False

    def _finalize_eof(self) -> HdcStreamEvent:
        """终态：回收子进程（宽限后补 SIGKILL 自身组），交付带退出码的 eof 事件。"""
        if not self._eof_seen:
            self._exit_code = _wait_reaped(self._proc, _REAP_GRACE_S)
            self._eof_seen = True
        return HdcStreamEvent("eof", None, self._exit_code)


__all__ = [
    "DEFAULT_CALL_TIMEOUT_S", "HdcTransportError", "HdcTransportTimeout",
    "HdcStreamEvent", "RealHdcTransport", "HdcStream",
]
