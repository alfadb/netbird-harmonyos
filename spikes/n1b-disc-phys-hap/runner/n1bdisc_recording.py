# -*- coding: utf-8 -*-
"""n1bdisc_recording — Live campaign 增量记录原语（host-only，零设备）。

后续 Live 编排使用的**唯一记录面**：单次 run 目录内逐行增量落盘 + 原子状态文件 +
终态封签。权威规格（只读冻结）``docs/n1b-disc-gate-plan.md``：

- :432 增量落盘——「任一后续步骤崩溃不得损失既得事实」→ 每次 append 即 flush，
  此前行在磁盘上始终可读；
- :1444 门 13「按证据目录增量与状态文件时间监控」→ 固定 ``state.json`` 以
  临时文件 + ``os.replace`` 原子更新，mtime/内容持续刷新供外部监控（心跳语义）；
- :1658-1664 host finally 与封签次序 → :meth:`RunRecorder.finish` 写终结果 +
  sha256 清单（封签）；
- :1140/:1146-1156 封签失败与「未完成预注册采集」→ 终态只取显式
  ``complete``/``failed``/``incomplete``，不编造 verdict。

边界（本模块**不做**，全部由后续 engine 明确控制）：

- 不承担授权检查、ID 分配、pair/evidence 目录选址或消费决定——只逐字登记
  调用者传入的安全键与事实；
- 不接 CLI、不启动任何程序、不做任何设备操作、不做设备 cleanup；
- 不向 stdout 输出任何日志内容（记录只进 run 目录）；
- 不做日志轮转、数据库、通用 events 订阅（单文件 JSONL 足够本 campaign 用量）。

安全面：

- metadata 只收构造期登记的安全键（mode/authorisation_id/campaign_id/
  evidence_id/code_sha/freeze_hash + created_at/recorder 标识），签名上不存在
  target/endpoint/token/argv 形参；
- ``append_event`` 只收操作名 + 无敏摘要 Mapping（拒绝裸字符串负载），契约层
  禁止携带原始 argv/target；
- 目录 0700、文件 0600（umask 兜底 chmod）——受控 raw 日志，非公开输出；
- 输出根单次使用：``exist_ok=False`` 创建，已存在无论空否一律拒绝，绝不覆盖。

调用契约（单线程；一个 run 目录恰一个 recorder 实例）::

    rec = RunRecorder(root, mode=..., authorisation_id=..., campaign_id=...,
                      evidence_id=..., code_sha=..., freeze_hash=...)
    rec.append_capture(raw_line, wall_ms, mono_ms)   # 捕获原文 + 双时钟
    rec.append_event(label, details=None)            # 操作名/phase + 无敏摘要
    rec.update_state(phase, **fields)                # state.json 原子替换（心跳）
    rec.finish(result, terminal="complete")          # result.json + manifest.json

``finish`` 后一切追加/状态更新被拒。调用者未 ``finish`` 即丢弃/进程退出时，
尽力补写 ``incomplete`` 终态（best-effort；真正的防丢保证是逐 append flush）。
本模块不替调用者判 pass/fail：写盘成功不构成 campaign pass，verdict 只存在于
调用者交给 ``finish(result, ...)`` 的 result 字面里。

测试钉：``selftests/test_recording.py``。
"""

from __future__ import annotations

import hashlib
import json
import os
import time
from collections.abc import Mapping
from typing import Any, Dict, Optional

#: run 目录内固定文件名（监控与封签都按这些名字寻址）。
CAPTURE_NAME = "capture.jsonl"
EVENTS_NAME = "events.jsonl"
METADATA_NAME = "metadata.json"
STATE_NAME = "state.json"
STATE_TMP_NAME = "state.json.tmp"
RESULT_NAME = "result.json"
MANIFEST_NAME = "manifest.json"

#: ``finish(terminal=...)`` 显式终态闭域（规格 :1156「未完成预注册采集」不洗白）。
TERMINAL_STATES = ("complete", "failed", "incomplete")

#: metadata 格式标识（键集变更时递进）。
RECORDER_ID = "n1bdisc-recording/1"

_DIR_MODE = 0o700
_FILE_MODE = 0o600


class RecordingError(Exception):
    """记录器拒绝执行（根已存在 / 封签后追加 / 参数越界）。

    消息只含路径、键名与阶段，不回显调用负载（raw 行 / details 内容）。
    """


class RunRecorder:
    """单次 run 目录的增量记录器。

    输出根由调用者显式给定；构造即 ``mkdir(exist_ok=False)`` 单次目录（已存在
    无论空否拒绝，不覆盖），随后立刻写 ``metadata.json``、建空 JSONL、写初始
    ``state.json``（phase=``opened``）。构造中途失败时清掉本进程刚创建的目录，
    不留半初始化残根（目录名本身不被复用，调用者换根重试）。
    """

    def __init__(self, root: str, *, mode: str, authorisation_id: str,
                 campaign_id: str, evidence_id: str, code_sha: str,
                 freeze_hash: str) -> None:
        # 安全键登记：只收固定形参，签名上不存在 target/endpoint/token/argv。
        metadata_keys = (("mode", mode), ("authorisation_id", authorisation_id),
                         ("campaign_id", campaign_id),
                         ("evidence_id", evidence_id), ("code_sha", code_sha),
                         ("freeze_hash", freeze_hash))
        for key, value in metadata_keys:
            if not isinstance(value, str) or not value:
                raise RecordingError(
                    "metadata key %r must be a non-empty str "
                    "(format/authorisation checks stay with the engine)" % key)

        # 解释器收尾阶段模块全局可能已清空；把 __del__ 收尾路径所需的少量
        # stdlib 可调用对象在构造期绑到实例上，保证「未 finish 即退出」的
        # incomplete 补写尽量能落盘（仍是 best-effort，见 _abandon）。
        self._os = os
        self._dumps = json.dumps
        self._sha256 = hashlib.sha256
        self._time_time = time.time
        self._time_monotonic = time.monotonic
        self._time_gmtime = time.gmtime
        self._time_strftime = time.strftime

        # fail-closed：构造全程保持 sealed，全部步骤成功才就绪——构造任何
        # 一步失败（含拒绝已存在根）时，随即销毁的半初始化实例经 __del__
        # 绝不会向被拒根/残根补写任何文件。
        self._closed = True
        self._root = os.fspath(root)
        self._seq = 0
        self._captures = 0
        self._events = 0
        self._capture_fh = None
        self._events_fh = None
        try:
            self._os.makedirs(self._root, mode=_DIR_MODE, exist_ok=False)
        except FileExistsError:
            raise RecordingError(
                "output root already exists (single-use dir, refuse, never "
                "overwrite): %s" % self._root) from None
        try:
            self._os.chmod(self._root, _DIR_MODE)       # umask 兜底
            self._write_once(METADATA_NAME, {
                "mode": mode,
                "authorisation_id": authorisation_id,
                "campaign_id": campaign_id,
                "evidence_id": evidence_id,
                "code_sha": code_sha,
                "freeze_hash": freeze_hash,
                "created_at": self._now_iso_utc(),
                "recorder": RECORDER_ID,
            })
            self._capture_fh = self._open_jsonl(CAPTURE_NAME)
            self._events_fh = self._open_jsonl(EVENTS_NAME)
            self._write_state_internal("opened", {})
            self._closed = False          # 构造完成，记录器就绪
        except BaseException:
            for name in (CAPTURE_NAME, EVENTS_NAME, METADATA_NAME,
                         STATE_NAME, STATE_TMP_NAME):
                try:
                    self._os.unlink(self._os.path.join(self._root, name))
                except OSError:
                    pass
            try:
                self._os.rmdir(self._root)
            except OSError:
                pass
            raise

    # ------------------------------------------------------------------
    # 只读面
    # ------------------------------------------------------------------

    @property
    def root(self) -> str:
        """本次 run 目录（调用者由此寻址记录文件；监控盯 ``state.json`` mtime）。"""
        return self._root

    # ------------------------------------------------------------------
    # 增量记录 API
    # ------------------------------------------------------------------

    def append_capture(self, raw_line: str, wall_ms: float,
                       mono_ms: float) -> None:
        """保存捕获原文 + 双时钟为 ``capture.jsonl`` 一行（append 即 flush）。

        ``raw_line`` 逐字登记（不清洗、不截断——受控 raw 日志的本职）；
        ``wall_ms``/``mono_ms`` 为调用者采样的两时钟读数，逐字随行保存。
        """
        self._ensure_open("append_capture")
        if not isinstance(raw_line, str):
            raise RecordingError("append_capture raw_line must be str")
        for key, value in (("wall_ms", wall_ms), ("mono_ms", mono_ms)):
            if isinstance(value, bool) or not isinstance(value, (int, float)):
                raise RecordingError(
                    "append_capture %s must be an int/float millisecond "
                    "reading" % key)
        self._seq += 1
        line = {"seq": self._seq, "kind": "capture", "raw_line": raw_line,
                "wall_ms": wall_ms, "mono_ms": mono_ms}
        self._capture_fh.write(self._json_line(line) + "\n")
        self._capture_fh.flush()
        self._captures += 1

    def append_event(self, label: str,
                     details: Optional[Mapping[str, Any]] = None) -> None:
        """登记操作名/phase 事件为 ``events.jsonl`` 一行（append 即 flush）。

        只命名操作与无敏摘要：``label`` 为操作/阶段名，``details`` 必须是
        Mapping 或 None（裸字符串负载拒收——原始 argv/target 不得经此入档，
        该约束是调用契约，内容脱敏由调用者负责）。
        """
        self._ensure_open("append_event")
        if not isinstance(label, str) or not label:
            raise RecordingError("append_event label must be a non-empty str")
        if details is not None and not isinstance(details, Mapping):
            raise RecordingError(
                "append_event details must be a mapping (non-sensitive "
                "summary) or None; raw argv/target strings are refused")
        self._append_event_internal(label, details)

    def update_state(self, phase: str, **fields: Any) -> None:
        """原子更新 ``state.json``（临时文件写全 + ``os.replace`` 替换）。

        父目录是本次自有 run 目录；每次调用刷新 ``wall_ms``/``mono_ms``/
        ``updated_at`` 与计数——外部监控按 mtime/phase 做时间监控（心跳语义，
        门 13 :1444）。``fields`` 为整帧快照语义（整体替换，不与上一帧合并）。
        """
        self._ensure_open("update_state")
        if not isinstance(phase, str) or not phase:
            raise RecordingError("update_state phase must be a non-empty str")
        self._write_state_internal(phase, fields)

    def finish(self, result: Mapping[str, Any],
               terminal: str = "complete") -> None:
        """封签：终结果 + sha256 清单，随后永久拒绝追加。

        ``result`` 逐字为调用者交付的终记录（verdict 由 engine 判定并随
        result 传入——本模块不编造、不改动）；``terminal`` 是调用者显式声明的
        终态（:data:`TERMINAL_STATES` 闭域）: ``complete`` 正常封签 /
        ``failed`` 显式失败 / ``incomplete`` 未完成。写盘成功只说明封签完成，
        不构成 campaign pass。

        清单（``manifest.json``）覆盖 run 目录内每个普通文件的相对路径的
        完整 sha256 与字节数，**不含清单自身**（不自哈希闭环）。
        """
        self._ensure_open("finish")
        if terminal not in TERMINAL_STATES:
            raise RecordingError(
                "finish terminal must be one of %s, got key %r outside domain"
                % (TERMINAL_STATES, terminal))
        if not isinstance(result, Mapping):
            raise RecordingError(
                "finish result must be a mapping supplied by the caller")
        self._closed = True
        self._write_state_internal(terminal, {"terminal": terminal})
        self._close_fhs()
        self._write_once(RESULT_NAME, {
            "terminal": terminal,
            "result": dict(result),
            "finished_at": self._now_iso_utc(),
            "wall_ms": self._now_wall_ms(),
            "mono_ms": self._now_mono_ms(),
            "captures": self._captures,
            "events": self._events,
        })
        self._write_manifest()

    # ------------------------------------------------------------------
    # 未 finish 收尾（best-effort incomplete 标记；不做设备 cleanup）
    # ------------------------------------------------------------------

    def __del__(self) -> None:
        try:
            self._abandon("closed-without-finish")
        except Exception:
            pass  # finalizer 永不 raise

    def _abandon(self, reason: str) -> None:
        """未 finish 的丢弃/退出路径：补写 incomplete 终态（幂等，尽力而为）。

        防丢的第一道保证是逐 append flush（规格 :432）；本方法是第二道：
        补写 phase=``incomplete`` 状态、result.json（result=null，不编造
        verdict）与封签清单。解释器收尾期任何一步失败都被吞掉，不掩盖、
        不重试、不触发任何设备清理。
        """
        if getattr(self, "_closed", True):
            return
        self._closed = True
        try:
            self._append_event_internal("recorder-abandoned", {"reason": reason})
        except Exception:
            pass
        try:
            self._write_state_internal("incomplete", {"reason": reason})
        except Exception:
            pass
        try:
            self._close_fhs()
        except Exception:
            pass
        try:
            self._write_once(RESULT_NAME, {
                "terminal": "incomplete",
                "result": None,
                "reason": reason,
                "closed_at": self._now_iso_utc(),
                "wall_ms": self._now_wall_ms(),
                "mono_ms": self._now_mono_ms(),
                "captures": self._captures,
                "events": self._events,
            })
            self._write_manifest()
        except Exception:
            pass

    # ------------------------------------------------------------------
    # 内部：写盘原语（全部收在 self 上，收尾路径不依赖模块全局）
    # ------------------------------------------------------------------

    def _ensure_open(self, op: str) -> None:
        if self._closed:
            raise RecordingError(
                "recorder already sealed (%s refused after finish/incomplete)"
                % op)

    def _now_wall_ms(self) -> int:
        return int(self._time_time() * 1000)

    def _now_mono_ms(self) -> int:
        return int(self._time_monotonic() * 1000)

    def _now_iso_utc(self) -> str:
        return self._time_strftime("%Y-%m-%dT%H:%M:%SZ", self._time_gmtime())

    def _json_line(self, payload: Dict[str, Any]) -> str:
        return self._dumps(payload, ensure_ascii=False, sort_keys=True,
                           separators=(",", ":"))

    def _json_document(self, payload: Dict[str, Any]) -> str:
        return self._dumps(payload, ensure_ascii=False, sort_keys=True,
                           indent=2) + "\n"

    def _open_jsonl(self, name: str):
        path = self._os.path.join(self._root, name)
        fd = self._os.open(path, self._os.O_WRONLY | self._os.O_CREAT
                           | self._os.O_APPEND, _FILE_MODE)
        try:
            self._os.fchmod(fd, _FILE_MODE)
        except OSError:
            self._os.close(fd)
            raise
        return self._os.fdopen(fd, "a", encoding="utf-8")

    def _write_once(self, name: str, payload: Dict[str, Any]) -> str:
        """0600 单次写 JSON 文档（metadata/result/manifest），flush+fsync。"""
        path = self._os.path.join(self._root, name)
        fd = self._os.open(path, self._os.O_WRONLY | self._os.O_CREAT
                           | self._os.O_TRUNC, _FILE_MODE)
        with self._os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(self._json_document(payload))
            fh.flush()
            self._os.fsync(fh.fileno())
        self._os.chmod(path, _FILE_MODE)
        return path

    def _write_state_internal(self, phase: str,
                              fields: Mapping[str, Any]) -> None:
        """state 快照：临时文件写全 + fsync + ``os.replace`` 原子替换。

        崩溃任意点：读者要么见上一帧完整 JSON，要么见新帧完整 JSON，永不
        见半帧；残留 ``state.json.tmp`` 会被下一次更新覆盖。
        """
        payload = {
            "phase": phase,
            "fields": dict(fields),
            "updated_at": self._now_iso_utc(),
            "wall_ms": self._now_wall_ms(),
            "mono_ms": self._now_mono_ms(),
            "captures": self._captures,
            "events": self._events,
        }
        tmp = self._os.path.join(self._root, STATE_TMP_NAME)
        fd = self._os.open(tmp, self._os.O_WRONLY | self._os.O_CREAT
                           | self._os.O_TRUNC, _FILE_MODE)
        with self._os.fdopen(fd, "w", encoding="utf-8") as fh:
            fh.write(self._json_document(payload))
            fh.flush()
            self._os.fsync(fh.fileno())
        self._os.chmod(tmp, _FILE_MODE)
        self._os.replace(tmp, self._os.path.join(self._root, STATE_NAME))

    def _append_event_internal(self, label: str,
                               details: Optional[Mapping[str, Any]]) -> None:
        self._seq += 1
        line = {"seq": self._seq, "kind": "event", "label": label,
                "details": dict(details) if details is not None else None,
                "wall_ms": self._now_wall_ms(),
                "mono_ms": self._now_mono_ms()}
        self._events_fh.write(self._json_line(line) + "\n")
        self._events_fh.flush()
        self._events += 1

    def _close_fhs(self) -> None:
        for fh in (self._capture_fh, self._events_fh):
            if fh is not None and not fh.closed:
                fh.flush()
                self._os.fsync(fh.fileno())
                fh.close()

    def _write_manifest(self) -> str:
        """sha256 清单：run 目录内每个普通文件（相对路径，排序），不含自身。"""
        manifest_path = self._os.path.join(self._root, MANIFEST_NAME)
        files: Dict[str, Dict[str, Any]] = {}
        for dirpath, dirnames, filenames in self._os.walk(self._root):
            dirnames.sort()
            for name in sorted(filenames):
                path = self._os.path.join(dirpath, name)
                if (self._os.path.abspath(path)
                        == self._os.path.abspath(manifest_path)):
                    continue
                if not self._os.path.isfile(path):
                    continue
                rel = self._os.path.relpath(path, self._root)
                rel = rel.replace(self._os.sep, "/")
                digest = self._sha256()
                size = 0
                with open(path, "rb") as fh:
                    for chunk in iter(lambda: fh.read(1 << 20), b""):
                        digest.update(chunk)
                        size += len(chunk)
                files[rel] = {"sha256": digest.hexdigest(), "bytes": size}
        return self._write_once(MANIFEST_NAME,
                                {"algorithm": "sha256", "files": files})
