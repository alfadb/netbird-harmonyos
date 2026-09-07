#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc_recording selftests（host-only，临时目录 + TEST 占位身份，零设备）。

钉住增量记录原语（``runner/n1bdisc_recording.py``）的冻结约束：

  T1. 单次目录：已存在（空/非空）拒绝且既有字节不变，绝不覆盖
  T2. metadata 固定安全键：恰含 mode/authorisation_id/campaign_id/evidence_id/
      code_sha/freeze_hash(+created_at/recorder)，无 target/endpoint/token 类键
  T3. 构造参数面：未登记 kwarg（target/endpoint）TypeError 拒收；空/非 str
      安全键 RecordingError 拒收
  T4. capture JSONL：原文 + wall/mono 双时钟逐行保存，append 即 flush（未
      finish 即可在盘上读到增量行，序一致）
  T5. event JSONL：操作名 + 无敏摘要（Mapping/None），裸字符串负载拒收；
      事件行自带双时钟
  T6. update_state 原子读回：phase/fields/双时钟/计数恒为完整 JSON，无
      .tmp 残留
  T7. finish：result 逐字封存（不编造 verdict）+ manifest sha256 全量可重算
      且不自哈希闭环；terminal 闭域外拒收
  T8. finish 后拒绝追加/状态更新/重复 finish，拒绝路径零字节写入
  T9. 未 finish 即丢弃：部分记录保留 + incomplete 终态（result=null，无编造
      verdict）+ 清单仍可重算
  T10. failed 显式终态：调用者 result 逐字封存，记录器不添加 verdict
  T11. 权限：目录 0700 / 文件 0600
  T12. 本模块零 stdout 输出

全部用例只用 ``tempfile`` 临时目录与 TEST/placeholder 身份，绝不触及实际
pair/evidence 目录（证据目录选址与授权是后续 engine 的事，本模块不涉）。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_recording.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_recording.py
"""

from __future__ import annotations

import gc
import hashlib
import io
import json
import os
import stat
import sys
import tempfile
from contextlib import redirect_stdout

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                os.pardir, "runner"))
import n1bdisc_recording as recording  # noqa: E402

_ASSERTS = 0


def expect(cond, msg):
    """断言计数器（main 汇总每断言数；pytest 下等价 assert）。"""
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# TEST/placeholder 身份（绝不使用真实 pair/evidence 目录与授权 ID）
# --------------------------------------------------------------------------

META_KWARGS = {
    "mode": "TEST-live",
    "authorisation_id": "TEST-AUTH-N1BDISC-0000",
    "campaign_id": "TEST-CAMPAIGN-N1BDISC-0000",
    "evidence_id": "EV-N1BDISC-TEST-19700101-0001",
    "code_sha": "0" * 64,
    "freeze_hash": "f" * 64,
}


def make_recorder(base, name="run-0001", **overrides):
    """在 base 下开一个尚不存在的新 run 根（单次目录语义）。"""
    kwargs = dict(META_KWARGS)
    kwargs.update(overrides)
    root = os.path.join(base, name)
    return recording.RunRecorder(root, **kwargs)


def read_bytes(path):
    with open(path, "rb") as fh:
        return fh.read()


def read_json(path):
    return json.loads(read_bytes(path).decode("utf-8"))


def jsonl_lines(path):
    text = read_bytes(path).decode("utf-8")
    return [json.loads(ln) for ln in text.splitlines() if ln.strip()]


def run_files(root):
    """run 目录内全部普通文件的相对路径（排序，POSIX 分隔）。"""
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames.sort()
        for name in sorted(filenames):
            rel = os.path.relpath(os.path.join(dirpath, name), root)
            found.append(rel.replace(os.sep, "/"))
    return sorted(found)


def check_manifest(root):
    """清单可重算：覆盖盘上每个文件恰一次、sha256/bytes 与盘上字节一致。"""
    manifest = read_json(os.path.join(root, recording.MANIFEST_NAME))
    expect(manifest.get("algorithm") == "sha256",
           "manifest algorithm must be sha256, got %r" % manifest.get("algorithm"))
    files = manifest["files"]
    expect("manifest.json" not in files,
           "manifest must not hash itself (no self-hash closure): %s"
           % sorted(files))
    on_disk = [rel for rel in run_files(root)
               if rel != recording.MANIFEST_NAME]
    expect(sorted(files) == on_disk,
           "manifest must cover every on-disk file exactly once: manifest=%s "
           "disk=%s" % (sorted(files), on_disk))
    for rel, entry in sorted(files.items()):
        data = read_bytes(os.path.join(root, rel))
        expect(hashlib.sha256(data).hexdigest() == entry["sha256"],
               "sha256 mismatch for %s" % rel)
        expect(entry["bytes"] == len(data),
               "byte count mismatch for %s: manifest=%d disk=%d"
               % (rel, entry["bytes"], len(data)))
    return manifest


def expect_recording_error(fn, msg):
    try:
        fn()
    except recording.RecordingError:
        expect(True, "")
        return
    raise AssertionError(msg)


# --------------------------------------------------------------------------
# T1. 单次目录：已存在拒绝且字节不变
# --------------------------------------------------------------------------

def test_existing_root_refused_bytes_unchanged():
    with tempfile.TemporaryDirectory() as base:
        # 空目录：无论空否一律拒绝
        root = os.path.join(base, "run-empty")
        os.mkdir(root)
        expect_recording_error(
            lambda: recording.RunRecorder(root, **META_KWARGS),
            "existing empty root must be refused")
        expect(os.listdir(root) == [],
               "refused recorder must not write into the existing root")

        # 非空目录：既有字节不变、无任何新文件
        root = os.path.join(base, "run-nonempty")
        os.mkdir(root)
        sentinel = os.path.join(root, "keep.txt")
        payload = "sentinel-bytes-unchanged\n"
        with open(sentinel, "w", encoding="utf-8") as fh:
            fh.write(payload)
        expect_recording_error(
            lambda: recording.RunRecorder(root, **META_KWARGS),
            "existing non-empty root must be refused")
        expect(read_bytes(sentinel).decode("utf-8") == payload,
               "pre-existing file bytes must be untouched on refusal")
        expect(os.listdir(root) == ["keep.txt"],
               "refused recorder must not add files: %s" % os.listdir(root))

        # 全新根：正常创建
        rec = make_recorder(base)
        expect(os.path.isdir(rec.root), "fresh root must be created")


# --------------------------------------------------------------------------
# T2. metadata 固定安全键（无 target/endpoint/token）
# --------------------------------------------------------------------------

def test_metadata_fixed_safe_keys():
    forbidden = ("target", "endpoint", "token", "argv", "password", "secret")
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        meta = read_json(os.path.join(rec.root, recording.METADATA_NAME))
        expect(sorted(meta) == sorted(
            ["mode", "authorisation_id", "campaign_id", "evidence_id",
             "code_sha", "freeze_hash", "created_at", "recorder"]),
            "metadata key set must be the fixed safe set, got %s" % sorted(meta))
        for key, value in META_KWARGS.items():
            expect(meta[key] == value,
                   "metadata %s must round-trip the caller-supplied value" % key)
        expect(meta["recorder"] == recording.RECORDER_ID,
               "metadata recorder id must match module constant")
        expect(isinstance(meta["created_at"], str) and meta["created_at"],
               "metadata created_at must be a timestamp string")
        raw = read_bytes(os.path.join(rec.root,
                                      recording.METADATA_NAME)).decode("utf-8")
        for word in forbidden:
            expect(word not in raw.lower(),
                   "metadata must not carry %r-shaped keys/values" % word)


# --------------------------------------------------------------------------
# T3. 构造参数面：不收 target/endpoint；安全键非空 str
# --------------------------------------------------------------------------

def test_constructor_rejects_unregistered_and_invalid_kwargs():
    with tempfile.TemporaryDirectory() as base:
        bad = dict(META_KWARGS)
        bad["target"] = "127.0.0.1:10171"
        try:
            recording.RunRecorder(os.path.join(base, "run-t"), **bad)
        except TypeError:
            expect(True, "")
        else:
            raise AssertionError("unregistered target kwarg must be refused")
        expect(not os.path.exists(os.path.join(base, "run-t")),
               "refused constructor must not create the root")

        for key in ("mode", "authorisation_id", "evidence_id"):
            bad = dict(META_KWARGS)
            bad[key] = ""
            expect_recording_error(
                lambda b=bad, k=key: recording.RunRecorder(
                    os.path.join(base, "run-e-" + k), **b),
                "empty %s must be refused" % key)
        bad = dict(META_KWARGS)
        bad["code_sha"] = 12345
        expect_recording_error(
            lambda: recording.RunRecorder(os.path.join(base, "run-n"), **bad),
            "non-str code_sha must be refused")


# --------------------------------------------------------------------------
# T4. capture JSONL：原文 + 双时钟，append 即 flush 增量可读
# --------------------------------------------------------------------------

def test_capture_lines_incremental_with_dual_clocks():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        capture_path = os.path.join(rec.root, recording.CAPTURE_NAME)
        raw_a = "N1BDISC_PRE|ledger_digest=%s|skip_summary=none" % ("a" * 64)
        raw_b = "08-30 10:00:00.000  12345  12345 D .../N1BDISC: noise line"
        rec.append_capture(raw_a, 1757100000123, 42)
        # 未 finish 即可在盘上读到第一行（append 即 flush：崩溃不损既得事实）
        lines = jsonl_lines(capture_path)
        expect(len(lines) == 1,
               "first capture line must be readable before finish, got %d"
               % len(lines))
        expect(lines[0]["raw_line"] == raw_a,
               "raw_line must be stored verbatim")
        expect(lines[0]["wall_ms"] == 1757100000123,
               "capture line must carry caller wall_ms")
        expect(lines[0]["mono_ms"] == 42,
               "capture line must carry caller mono_ms")
        expect(lines[0]["kind"] == "capture", "line kind must be capture")
        rec.append_capture(raw_b, 1757100000456, 43)
        lines = jsonl_lines(capture_path)
        expect(len(lines) == 2, "second append must land incrementally")
        expect([ln["seq"] for ln in lines] == [1, 2],
               "seq must follow append order")
        expect(lines[1]["raw_line"] == raw_b and lines[1]["wall_ms"] == 1757100000456
               and lines[1]["mono_ms"] == 43,
               "second line must keep verbatim raw + dual clocks")


# --------------------------------------------------------------------------
# T5. event JSONL：操作名 + 无敏摘要；裸字符串负载拒收
# --------------------------------------------------------------------------

def test_event_lines_label_and_summary():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        rec.append_event("host-finally", {"step": "2.faultprobe+faultrecv",
                                          "files": 3})
        rec.append_event("capture-started")
        events = jsonl_lines(os.path.join(rec.root, recording.EVENTS_NAME))
        expect(len(events) == 2, "both events must be appended")
        expect(events[0]["label"] == "host-finally"
               and events[0]["details"] == {"step": "2.faultprobe+faultrecv",
                                            "files": 3},
               "event line must round-trip label + details")
        expect(events[1]["label"] == "capture-started"
               and events[1]["details"] is None,
               "details=None must be recorded as null")
        for line in events:
            expect(isinstance(line["wall_ms"], int)
                   and isinstance(line["mono_ms"], int),
                   "event line must carry recorder-stamped dual clocks")
        expect_recording_error(
            lambda: rec.append_event("bad", "hdc list targets"),
            "bare-string payload (raw argv shape) must be refused")
        expect_recording_error(
            lambda: rec.append_event("bad", ["--target", "x"]),
            "non-mapping details must be refused")


# --------------------------------------------------------------------------
# T6. update_state 原子读回
# --------------------------------------------------------------------------

def test_state_atomic_readback():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        state_path = os.path.join(rec.root, recording.STATE_NAME)
        opened = read_json(state_path)
        expect(opened["phase"] == "opened",
               "initial state phase must be opened, got %r" % opened["phase"])
        rec.append_capture("line-a", 1, 1)
        rec.update_state("capture-stream", progress=3, note="mid-stream")
        for _ in range(3):  # 重复读回：每帧恒为完整 JSON（原子替换）
            state = read_json(state_path)
            expect(state["phase"] == "capture-stream",
                   "phase must round-trip, got %r" % state["phase"])
            expect(state["fields"] == {"progress": 3, "note": "mid-stream"},
                   "fields snapshot must round-trip, got %r" % state["fields"])
            expect(isinstance(state["wall_ms"], int)
                   and isinstance(state["mono_ms"], int)
                   and isinstance(state["updated_at"], str),
                   "state must carry dual clocks + timestamp for monitoring")
            expect(state["captures"] == 1 and state["events"] == 0,
                   "state must carry record counters, got %r"
                   % (state["captures"],))
        rec.update_state("host-finally")
        expect(read_json(state_path)["fields"] == {},
               "fields use whole-frame snapshot semantics (replaced, not merged)")
        expect(not os.path.exists(os.path.join(rec.root,
                                               recording.STATE_TMP_NAME)),
               "atomic replace must leave no .tmp behind")


# --------------------------------------------------------------------------
# T7. finish：result 逐字 + manifest 全量可重算、不自哈希
# --------------------------------------------------------------------------

def test_finish_manifest_recomputable_and_result_verbatim():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        rec.append_capture("N1BDISC_POST|d6_items=x", 100, 7)
        rec.append_event("seal", {"step": "10.integrity-close"})
        rec.update_state("pre-seal")
        result = {"verdict": "pass", "protocol": "complete",
                  "evidence_vector": {"process_death_observed": "observed-true"}}
        rec.finish(result)
        result_doc = read_json(os.path.join(rec.root, recording.RESULT_NAME))
        expect(result_doc["terminal"] == "complete",
               "terminal must echo the caller declaration")
        expect(result_doc["result"] == result,
               "caller result must be sealed verbatim, got %r"
               % result_doc["result"])
        state = read_json(os.path.join(rec.root, recording.STATE_NAME))
        expect(state["phase"] == "complete",
               "final state phase must be the terminal state")
        check_manifest(rec.root)


# --------------------------------------------------------------------------
# T8. finish 后拒绝追加；拒绝路径零字节写入
# --------------------------------------------------------------------------

def test_refuse_after_finish():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        rec.append_capture("line", 1, 2)
        rec.finish({"verdict": "pass"})
        root_before = {name: read_bytes(os.path.join(rec.root, name))
                       for name in run_files(rec.root)}
        expect_recording_error(lambda: rec.append_capture("late", 3, 4),
                               "append_capture after finish must be refused")
        expect_recording_error(lambda: rec.append_event("late"),
                               "append_event after finish must be refused")
        expect_recording_error(lambda: rec.update_state("late-phase"),
                               "update_state after finish must be refused")
        expect_recording_error(lambda: rec.finish({}),
                               "second finish must be refused")
        root_after = {name: read_bytes(os.path.join(rec.root, name))
                      for name in run_files(rec.root)}
        expect(root_before == root_after,
               "refused calls must not change any byte on disk")


# --------------------------------------------------------------------------
# T9. 未 finish 即丢弃：部分记录保留 + incomplete 终态（无编造 verdict）
# --------------------------------------------------------------------------

def test_abandon_without_finish_marks_incomplete():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        raw_a = "N1BDISC_PRE|ledger_digest=%s" % ("b" * 64)
        rec.append_capture(raw_a, 1000, 7)
        rec.append_capture("tail line", 1001, 9)
        rec.update_state("capture-stream")
        root = rec.root
        del rec
        gc.collect()  # 触发 __del__ 的 best-effort incomplete 收尾
        state = read_json(os.path.join(root, recording.STATE_NAME))
        expect(state["phase"] == "incomplete",
               "abandoned run must be marked incomplete, got %r"
               % state["phase"])
        result_doc = read_json(os.path.join(root, recording.RESULT_NAME))
        expect(result_doc["terminal"] == "incomplete",
               "abandoned result.json must carry terminal=incomplete")
        expect(result_doc["result"] is None,
               "abandoned run must not fabricate a result")
        expect("verdict" not in read_bytes(
                   os.path.join(root, recording.RESULT_NAME)).decode("utf-8"),
               "incomplete path must not fabricate a verdict")
        lines = jsonl_lines(os.path.join(root, recording.CAPTURE_NAME))
        expect([ln["raw_line"] for ln in lines] == [raw_a, "tail line"],
               "prior capture lines must survive the abandonment intact")
        check_manifest(root)


# --------------------------------------------------------------------------
# T10. failed 显式终态：result 逐字，记录器不加 verdict
# --------------------------------------------------------------------------

def test_failed_terminal_state_no_fabricated_verdict():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        rec.append_event("host-finally", {"step": "5.forcestop"})
        result = {"failure": "host finally step 8 absent-probe mismatch"}
        rec.finish(result, terminal="failed")
        result_doc = read_json(os.path.join(rec.root, recording.RESULT_NAME))
        expect(result_doc["terminal"] == "failed",
               "failed terminal must round-trip")
        expect(result_doc["result"] == result,
               "failed result must be sealed verbatim")
        authored_keys = sorted(set(result_doc) - {"result"})
        expect("verdict" not in authored_keys
               and "verdict" not in result_doc["result"],
               "recorder must not add a verdict of its own: %s" % authored_keys)
        check_manifest(rec.root)
        expect_recording_error(
            lambda: make_recorder(base, name="run-bad").finish({}, terminal="pass"),
            "terminal outside the closed domain must be refused")


# --------------------------------------------------------------------------
# T11. 权限：目录 0700 / 文件 0600
# --------------------------------------------------------------------------

def test_permissions_dir_0700_files_0600():
    with tempfile.TemporaryDirectory() as base:
        rec = make_recorder(base)
        rec.append_capture("line", 1, 2)
        rec.append_event("op")
        rec.update_state("phase")
        rec.finish({"verdict": "pass"})
        mode = stat.S_IMODE(os.stat(rec.root).st_mode)
        expect(mode == 0o700, "run dir must be 0700, got %o" % mode)
        for name in run_files(rec.root):
            mode = stat.S_IMODE(os.stat(os.path.join(rec.root, name)).st_mode)
            expect(mode == 0o600, "%s must be 0600, got %o" % (name, mode))


# --------------------------------------------------------------------------
# T12. 本模块零 stdout 输出
# --------------------------------------------------------------------------

def test_module_writes_nothing_to_stdout():
    buf = io.StringIO()
    with redirect_stdout(buf):
        with tempfile.TemporaryDirectory() as base:
            rec = make_recorder(base)
            rec.append_capture("N1BDISC_PRE|x", 1, 2)
            rec.append_event("op", {"k": "v"})
            rec.update_state("phase")
            rec.finish({"verdict": "pass"})
    expect(buf.getvalue() == "",
           "recording module must not write log content to stdout, got %r"
           % buf.getvalue())


# ==========================================================================
# main runner（pytest 不在场时的一条命令入口）
# ==========================================================================

def main() -> int:
    import time
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
    print("n1bdisc recording selftests: tests=%d passed=%d failed=%d "
          "assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
