# -*- coding: utf-8 -*-
"""n1bdisc fake-hdc — 可编程假 hdc（host-only 唯一"hdc"形态）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结）。本模块是 runner 的默认
transport（规格门 11 DryRun = HDC0；真实 hdc transport 本任务不实现）。

能力：
1. **hilog 合成流**：按剧本输出 marker 序列（``N1BDISC_<SHORT>|k=v``）、三形态 tag
   （entry / ``:vpn`` 截断 / ``:vpn`` 完整，规格 :417）、可注入 chunk / 重复片 /
   late marker（窗到点后到达，规格 :1066）；
2. **faultlogger 目录合成**：可配置 APPFREEZE/CPPCRASH/JSRAWERROR/域外类型/多条目，
   快照（窗界判据 = 快照文件集合差分，规格 :1214）、逐文件取回失败（r13 按文件建模）、
   FaultProbe 全局失败；
3. **bundle dump / pidof 语义**：安装态、UI 与 ``:vpn`` 进程存活态；
4. **命令校验**：transport 侧与白名单 argv 逐字比对（规格 :1622-1643），违规抛
   :class:`n1bdisc_hdc.HdcViolation`。

全部行为 host-only：无真实 hdc/设备/网络端点。
"""

from __future__ import annotations

from typing import Dict, Iterable, Iterator, List, Mapping, Optional, Sequence

import n1bdisc_core as core
import n1bdisc_hdc as hdc

_UI_PID = 20001
_VPN_PID = 20002

#: 三形态 tag 路径（规格 :417；与 core.bundle_tag_forms 一致）。
_TAG_PATHS: Dict[str, str] = {
    "entry": core.DEFAULT_BUNDLE,
    "truncated": "." + core.DEFAULT_BUNDLE[len("cn."): ] + ":vpn",
    "complete": core.DEFAULT_BUNDLE + ":vpn",
}


def _mono_to_wall(mono_ms: int) -> str:
    """确定性墙钟合成：08-30 10:00:00.000 起随 mono 推进（仅 hilog 行形态需要）。"""
    total = 10 * 3600 * 1000 + mono_ms
    hh, rem = divmod(total, 3600 * 1000)
    mm, rem = divmod(rem, 60 * 1000)
    ss, ms = divmod(rem, 1000)
    return "08-30 %02d:%02d:%02d.%03d" % (hh, mm, ss, ms)


def format_hilog_line(tag_form: str, message: str, mono_ms: int,
                      pid: int) -> str:
    """合成一行可被 ``core.parse_hilog_line`` 关联的 hilog 文本。"""
    if tag_form not in _TAG_PATHS:
        raise ValueError("tag_form must be one of %r" % (sorted(_TAG_PATHS),))
    return "%s  %5d  %5d D %s/%s: %s" % (
        _mono_to_wall(mono_ms), pid, pid, _TAG_PATHS[tag_form], core.HILOG_TAG,
        message)


class FakeMarker:
    """剧本中的一枚合成 marker。

    ``late=True`` 的 marker 不进正常流，仅在窗到点后由 :meth:`FakeHdc.late_lines`
    提供（规格 :1066 ``late_marker_observed`` 的合成源）。
    """

    def __init__(self, short: str, at_mono_ms: int,
                 kv: Optional[Mapping[str, str]] = None,
                 tag_form: str = "entry", late: bool = False) -> None:
        self.short = short
        self.at_mono_ms = int(at_mono_ms)
        self.kv = dict(kv or {})
        self.tag_form = tag_form
        self.late = late

    @property
    def name(self) -> str:
        return core.MARKER_PREFIX + self.short

    def message(self) -> str:
        # 值内 ``|``/换行按 producer ``sanitize_marker_field``（util.rs:199-214）
        # 转义——POST ``d6_items`` 等含 ``|`` 的字段（B7/M2 producer-conformant）。
        parts = [self.name]
        parts.extend("%s=%s" % (k, core.escape_marker_field(str(v)))
                     for k, v in self.kv.items())
        return "|".join(parts)

    def line(self) -> str:
        pid = _UI_PID if self.tag_form == "entry" else _VPN_PID
        return format_hilog_line(self.tag_form, self.message(),
                                 self.at_mono_ms, pid)


def chunk_markers(text: str, stream: str, item: int, at_mono_ms: int,
                  tag_form: str = "entry") -> List[FakeMarker]:
    """把 detail 文本按 core 冻结编码切成 ``N1BDISC_CHUNK`` marker 剧本序列。"""
    pieces = core.encode_chunks(text, stream, item)
    return [FakeMarker("CHUNK", at_mono_ms + i, piece, tag_form)
            for i, piece in enumerate(pieces)]


def duplicate_chunk_markers(markers: Sequence[FakeMarker],
                            payload_override: Optional[str] = None
                            ) -> List[FakeMarker]:
    """复制一组 chunk marker（重复片注入）；``payload_override`` 可构造不一致重复片。"""
    out: List[FakeMarker] = []
    for m in markers:
        kv = dict(m.kv)
        if payload_override is not None:
            kv["payload"] = payload_override
        out.append(FakeMarker("CHUNK", m.at_mono_ms, kv, m.tag_form))
    return out


class FaultFileSpec:
    """一个合成 faultlogger 条目文件。"""

    def __init__(self, file_name: str, content: str,
                 recv_fail: bool = False) -> None:
        self.file_name = file_name
        self.content = content
        self.recv_fail = recv_fail

    def remote_path(self) -> str:
        return hdc.FAULTLOGGER_DIR + "/" + self.file_name


def fault_entry_text(fault_type: Optional[str], signal: Optional[str],
                     timestamp: str = "2026-09-06 10:00:00.000",
                     module: str = core.DEFAULT_BUNDLE,
                     raw_lines: Sequence[str] = ()) -> str:
    """按解析契约字段形态合成 fault 条目文本；``None`` 字段 = 字段行缺失。"""
    lines = list(raw_lines)
    if fault_type is not None:
        lines.append("Fault_Type: %s" % fault_type)
    if signal is not None:
        lines.append("Signal: %s" % signal)
    lines.append(timestamp)
    lines.append("Module: %s" % module)
    return "\n".join(lines) + "\n"


class FakeScenario:
    """一段可编程剧本：marker 序列 + faultlogger 目录 + 生命周期。

    B4-c runner 侧 join 事实（:720/:1047 的 DryRun 登记载体——live 形态由 fsm
    轮询路径供给同一布尔/整数）：``join_blocked_registered`` = runner 依轮询
    超时登记 join 阻塞；``join_exit_rc`` = runner 侧 pthread_join 返回值登记
    （0=joined、ESRCH、其他 errno）。``post_join_override`` 供反例剧本覆写
    POST ``dw_outcome`` 内层 join 列（模拟探针 bug/域外字面）。
    """

    def __init__(self, markers: Sequence[FakeMarker],
                 fault_files: Sequence[FaultFileSpec] = (),
                 snapshot_files: Sequence[str] = (),
                 die_at_end: bool = False,
                 fault_probe_fails: bool = False,
                 join_blocked_registered: bool = False,
                 join_exit_rc: Optional[int] = None,
                 post_join_override: Optional[str] = None) -> None:
        self.markers = list(markers)
        self.fault_files = {f.file_name: f for f in fault_files}
        self.snapshot_files = list(snapshot_files)
        self.die_at_end = die_at_end
        self.fault_probe_fails = fault_probe_fails
        self.join_blocked_registered = join_blocked_registered
        self.join_exit_rc = join_exit_rc
        self.post_join_override = post_join_override


class FakeHdc(hdc.HdcTransport):
    """可编程假 hdc transport（runner 默认 transport；host-only）。

    命令校验：任何进入 :meth:`call` / :meth:`open_stream` 的 argv 先与白名单模板
    （本实例的 target/hap 展开形）逐字比对，不命中即 :class:`hdc.HdcViolation`。
    """

    def __init__(self, scenario: FakeScenario, target: str = "FAKE-TARGET-1",
                 hap_path: str = "/host/fake/n1bdisc.hap") -> None:
        self.scenario = scenario
        self.target = target
        self.hap_path = hap_path
        # 生命周期状态
        self.staged = False
        self.hap_sent = False
        self.installed = False
        self.app_started = False
        self.ui_alive = False
        self.vpn_alive = False
        self.vpn_died = False
        self.force_stop_reasons: List[str] = []
        #: faultlogger 新条目物化标志：新增条目在进程死亡时刻产生（快照差分的前提）
        self.fault_materialized = False
        #: FaultRecv 取回结果：远端文件名 → 文本（host 侧落位由 fake 记录承载）
        self.received: Dict[str, str] = {}
        self.recv_failures: List[str] = []

    # ------------------------------------------------------------------
    # transport 接口
    # ------------------------------------------------------------------

    def call(self, argv: Sequence[str]) -> hdc.HdcTransportResult:
        op = self._validate(argv)
        argv = list(argv)
        if op == "FaultRecv":
            remote, host_path = argv[-2], argv[-1]
            name = remote[len(hdc.FAULTLOGGER_DIR) + 1:]
            spec = self.scenario.fault_files.get(name)
            if spec is None or spec.recv_fail:
                self.recv_failures.append(name)
                return hdc.HdcTransportResult(1, "", "file recv failed: %s\n" % name)
            self.received[name] = spec.content
            return hdc.HdcTransportResult(0, "", "")
        handler = getattr(self, "_op_%s" % op.lower())
        return handler()

    # ------------------------------------------------------------------
    # 白名单 argv 逐字反向校验
    # ------------------------------------------------------------------

    def _validate(self, argv: Sequence[str]) -> str:
        argv = list(argv)
        for op, template in hdc.HDC_ARGV_TABLE.items():
            expected = hdc.expand_template(template, target=self.target,
                                           hap_path=self.hap_path)
            if op == "FaultRecv":
                # 冻结部分逐字比对；<命中文件>/<host路径> 两参数 token 形态校验
                if (argv[:-2] == expected[:-2]
                        and len(argv) == len(expected)
                        and argv[-2].startswith(hdc.FAULTLOGGER_DIR + "/")
                        and argv[-1]):
                    return op
                continue
            if argv == expected:
                return op
        raise hdc.HdcViolation("argv-not-in-whitelist", {"argv": argv})

    # ------------------------------------------------------------------
    # hilog 合成流
    # ------------------------------------------------------------------

    def _mark_vpn_dead(self) -> None:
        self.vpn_alive = False
        self.vpn_died = True
        # 进程死亡时刻物化本窗新增 fault 条目（快照差分窗界，:1213-1215）
        self.fault_materialized = True

    def _stream_lines(self) -> Iterator[str]:
        for m in self.scenario.markers:
            if m.late:
                continue
            if m.short == "PRE" and m.kv.get("ledger_digest") == "PRE_DIGEST_PLACEHOLDER":
                yield FakeMarker("PRE", m.at_mono_ms,
                                 dict(m.kv, ledger_digest=self._pre_digest(m)),
                                 m.tag_form).line()
            elif m.short == "POST":
                yield self._post_line(m).line()
            else:
                yield m.line()
        if self.scenario.die_at_end:
            # 流末 = 进程死亡（pre-only 剧本）；finally 采样在此之后执行。
            self._mark_vpn_dead()

    # -- 设备侧（探针侧）模拟：终态 marker 字段的真实承载 ----------------

    def _fd_transitions(self) -> List[Mapping[str, str]]:
        return [m.kv for m in self.scenario.markers if m.short == "FD"]

    def _pre_digest(self, pre_marker: FakeMarker) -> str:
        """P5T 快照切点 digest（规格 :1200-1201：PRE 值只与 P5T 切点重建比对）。"""
        at = [t for t in self._fd_transitions()
              if int(t["at_mono_ms"]) <= pre_marker.at_mono_ms]
        rebuild = core.rebuild_fd_ledger(at, "pre-snapshot")
        assert rebuild.ok and rebuild.digest is not None
        return rebuild.digest

    @staticmethod
    def _dw_outcome_column(class_v: str, join_v: str, watchdog_v: str,
                           dist_v: str, poll_ret: str, poll_errno: str,
                           poll_revents: str, poll_elapsed_ms: str) -> str:
        """按探针 ``post_emit`` 字面合成 ``dw_outcome`` 内层列（观察 (ii) 对齐钉）。

        探针实际格式 = ``probe/src/dw.rs:1008-1018`` format! 字面：内层 ``;`` 分隔
        k=v、八字段位序固定（class/join/watchdog/dist/poll_ret/poll_errno/
        poll_revents/poll_elapsed_ms）；runner 解析同源契约
        （n1bdisc_verdict.DW_OUTCOME_FIELDS / parse_dw_outcome）。
        """
        return ";".join((
            "class=%s" % class_v, "join=%s" % join_v,
            "watchdog=%s" % watchdog_v, "dist=%s" % dist_v,
            "poll_ret=%s" % poll_ret, "poll_errno=%s" % poll_errno,
            "poll_revents=%s" % poll_revents,
            "poll_elapsed_ms=%s" % poll_elapsed_ms))

    def _post_line(self, post_marker: FakeMarker) -> FakeMarker:
        """POST 字段设备侧模拟：ledger 最终 digest + d6_items + dw_outcome（r16/r22）。

        digest 切口对齐探针实现（观察 (i)）：POST 最终 digest 恒按 ``complete``
        切口（仍 open 条目记 ``open-at-exit``）计算——探针 ``ledger::digest(false)``
        与 ``worker_terminal_at_p12`` FLAG 读值无关（probe/src/dw.rs:1006；判据
        :385/:392/:403 complete 收口 → open-at-exit）。FLAG 值仅按剧本登记透传。

        dw_outcome 内层列沿探针 derive_post_outcome 各格落值（dw.rs:791-916）：
        (a) D-W 整体 skip → 全列 skip cause 字面、join=同一 skip 编码（B4 探针
        面修复后 dw.rs ``join_result_fallback``：``!spawned`` → skip 字面逐字，
        不再发 pending，:443/:734/:870）；
        (e)/(f) RETURN 在 → 13 类 + poll raw 数字 + watchdog=observed-false
       （dw.rs:809-824）；flag-race 格 → 全列 flag-race-window-expired、watchdog=⑤
        marker-gap-indeterminate（dw.rs:896-913）；(d) poll-never 格同形换
        poll-never-returned 字面（dw.rs:878-895）。
        """
        rebuild = core.rebuild_fd_ledger(self._fd_transitions(), "complete")
        assert rebuild.ok and rebuild.digest is not None
        dw_skip = next((m for m in self.scenario.markers
                        if m.short == "SKIP" and m.kv.get("item") == "D-W"), None)
        ret = next((m for m in self.scenario.markers
                    if m.short == "DW_RETURN"), None)
        if dw_skip is not None:
            cause = dw_skip.kv.get("cause", "no-live-fd")
            u = core.unobservable_value(cause)
            dw_outcome = self._dw_outcome_column(u, u, u, u, u, u, u, u)
        elif ret is not None:
            result = core.derive_dw_return_class(core.DwReturnInput(
                ret=int(ret.kv["ret"]), revents=int(ret.kv["revents"]),
                errno=int(ret.kv["errno"]) if ret.kv.get("errno") not in (None, "none") else None,
                at_mono_ms=int(ret.kv["at_mono_ms"]),
                elapsed_ms=int(ret.kv["elapsed_ms"]),
                drain_end=self._drain_end(),
                has_skip_destroy=self._has_skip_destroy(),
                has_destroy_c=any(m.short == "DW_DESTROY_C"
                                  for m in self.scenario.markers),
                t_mono_ms=self._destroy_mono("DW_DESTROY_T"),
                c_mono_ms=self._destroy_mono("DW_DESTROY_C"),
            ))
            dw_class = result.value or "other-revents"
            # dist 沿 derive_distinguishable（dw.rs:619-663）：unobservable 类透传、
            # fd-event-like/timeout-like 具值、其余类 uncorrelated 收口。
            if dw_class.startswith("unobservable(cause="):
                dist = dw_class
            elif dw_class == "fd-event-like":
                dist = "observed-true"
            elif dw_class == "timeout-like":
                dist = "observed-false"
            else:
                dist = core.unobservable_value("destroy-uncorrelated-class")
            dw_outcome = self._dw_outcome_column(
                dw_class, "joined", "observed-false", dist,
                ret.kv["ret"], ret.kv["errno"], ret.kv["revents"],
                ret.kv["elapsed_ms"])
        else:
            race = any(m.short == "DW_RACEWIN" for m in self.scenario.markers)
            cause = "flag-race-window-expired" if race else "poll-never-returned"
            u = core.unobservable_value(cause)
            dw_outcome = self._dw_outcome_column(
                u, "join-timeout",
                core.unobservable_value("marker-gap-indeterminate"),
                u, u, u, u, u)
        d6_items = self._d6_items()
        kv = dict(post_marker.kv, ledger_digest=rebuild.digest,
                  d6_items=d6_items, dw_outcome=dw_outcome)
        # 反例剧本覆写 join 列（模拟探针 bug/域外字面——B4-b 反例钉）。
        override = getattr(self.scenario, "post_join_override", None)
        if override is not None:
            parts = [p for p in dw_outcome.split(";") if p]
            kv["dw_outcome"] = ";".join(
                ("join=%s" % override) if p.startswith("join=") else p
                for p in parts)
        return FakeMarker("POST", post_marker.at_mono_ms, kv, post_marker.tag_form)

    def _drain_end(self) -> str:
        drain = next((m for m in self.scenario.markers if m.short == "DW_DRAIN"), None)
        return drain.kv.get("end", "eagain") if drain else "eagain"

    def _has_skip_destroy(self) -> bool:
        return any(m.short == "SKIP" and m.kv.get("item") == "destroy"
                   for m in self.scenario.markers)

    def _destroy_mono(self, short: str) -> Optional[int]:
        m = next((x for x in self.scenario.markers if x.short == short), None)
        return int(m.kv["mono_ms"]) if m else None

    def _d6_items(self) -> str:
        """POST ``d6_items`` 设备侧模拟——**producer 实际格式与逐支逻辑**
        （dw.rs:665-743）：``;`` 分隔的 ``D6S<n>=<value>`` 三选一编码，逐段按
        剧本 marker 状态求值——D6a 段（S1-S3）：D6S1_R 在 → result 行（自
        D6Sx_R 字段投影）；destroy SKIP（no-live-connection→no-live-fd /
        barrier-never-observed）→ 对应 skipped cause；否则 destroy-unresolved。
        D6b 段（S4-S7）：D6S4_R 在 → result 行；jt（D6b skip=join-timeout-
        abandoned）→ 该 cause；D-W skip → skip cause；barrier-never → 同名；
        兜底 unobservable(cause=step-not-executed)。"""
        def _row(idx: int) -> Optional[str]:
            mk = next((m for m in self.scenario.markers
                       if m.short == "D6S%d_R" % idx), None)
            if mk is None:
                return None
            if idx == 7:
                return "D6S7=result|fd=%s|reuse=%s" % (
                    mk.kv.get("fd", "-1"), mk.kv.get("reuse", "false"))
            return "D6S%d=result|ret=%s|errno=%s" % (
                idx, mk.kv.get("ret", "0"), mk.kv.get("errno", "0"))

        destroy_skip = next(
            (m for m in self.scenario.markers
             if m.short == "SKIP" and m.kv.get("item") == "destroy"), None)
        d6b_skip = next(
            (m for m in self.scenario.markers
             if m.short == "SKIP" and m.kv.get("item") == "D6b"), None)
        dw_skip_marker = next(
            (m for m in self.scenario.markers
             if m.short == "SKIP" and m.kv.get("item") == "D-W"), None)
        dw_cause = dw_skip_marker.kv.get("cause") if dw_skip_marker else None

        # D6a 段（dw.rs:680-702）
        d6a_rows = [_row(i) for i in (1, 2, 3)]
        if all(r is not None for r in d6a_rows):
            d6a = list(d6a_rows)
        elif destroy_skip is not None:
            cause = destroy_skip.kv.get("cause", "no-live-connection")
            d6a = ["D6S%d=skipped(cause=%s)" % (i, "no-live-fd"
                                                if cause == "no-live-connection"
                                                else cause) for i in (1, 2, 3)]
        else:
            d6a = ["D6S%d=skipped(cause=destroy-unresolved)" % i
                   for i in (1, 2, 3)]

        # D6b 段（dw.rs:704-740）
        d6b_rows = [_row(i) for i in (4, 5, 6, 7)]
        if all(r is not None for r in d6b_rows):
            d6b = list(d6b_rows)
        elif d6b_skip is not None and \
                d6b_skip.kv.get("cause") == "join-timeout-abandoned":
            d6b = ["D6S%d=skipped(cause=join-timeout-abandoned)" % i
                   for i in (4, 5, 6, 7)]
        elif dw_cause in ("no-live-fd", "dup-failed"):
            d6b = ["D6S%d=skipped(cause=%s)" % (i, dw_cause)
                   for i in (4, 5, 6, 7)]
        elif destroy_skip is not None and \
                destroy_skip.kv.get("cause") == "barrier-never-observed":
            d6b = ["D6S%d=skipped(cause=barrier-never-observed)" % i
                   for i in (4, 5, 6, 7)]
        else:
            d6b = ["D6S%d=unobservable(cause=step-not-executed)" % i
                   for i in (4, 5, 6, 7)]
        return ";".join(d6a + d6b)

    def late_lines(self) -> List[str]:
        """窗到点后到达的 marker 行（不参与求值，规格 :1066）。"""
        return [m.line() for m in self.scenario.markers if m.late]

    # ------------------------------------------------------------------
    # 命令语义（_op_* 方法名与白名单操作名小写对应）
    # ------------------------------------------------------------------

    def _op_version(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, "hdc 1.2.3\n", "")

    def _op_parammodel(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, "FAKE-MODEL-1\n", "")

    def _op_paramsoftwareversion(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, "7.0.0.102\n", "")

    def _op_bundledump(self) -> hdc.HdcTransportResult:
        if not self.installed:
            return hdc.HdcTransportResult(
                1, "", "error: bundle %s not found\n" % core.DEFAULT_BUNDLE)
        return hdc.HdcTransportResult(
            0, "BundleName: %s\nAppStates: IS_INSTALLED=true\n"
               % core.DEFAULT_BUNDLE, "")

    def _op_pidof(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, str(_UI_PID) + "\n" if self.ui_alive else "", "")

    def _op_pidofpost(self) -> hdc.HdcTransportResult:
        return self._op_pidof()

    def _op_pidofvpn(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, str(_VPN_PID) + "\n" if self.vpn_alive else "", "")

    def _op_pidofvpnpost(self) -> hdc.HdcTransportResult:
        return self._op_pidofvpn()

    def _op_mkdirstaging(self) -> hdc.HdcTransportResult:
        self.staged = True
        return hdc.HdcTransportResult(0, "", "")

    def _op_sendhap(self) -> hdc.HdcTransportResult:
        if not self.staged:
            return hdc.HdcTransportResult(1, "", "file send failed: no staging dir\n")
        self.hap_sent = True
        return hdc.HdcTransportResult(0, "FileTransfer finish\n", "")

    def _op_installhap(self) -> hdc.HdcTransportResult:
        if not self.hap_sent:
            return hdc.HdcTransportResult(1, "", "bm install failed: hap missing\n")
        self.installed = True
        return hdc.HdcTransportResult(0, "install bundle successfully\n", "")

    def _op_startentry(self) -> hdc.HdcTransportResult:
        if not self.installed:
            return hdc.HdcTransportResult(1, "", "aa start failed: not installed\n")
        self.app_started = True
        self.ui_alive = True
        self.vpn_alive = True
        return hdc.HdcTransportResult(0, "start ability successfully\n", "")

    def _op_hilogstream(self) -> hdc.HdcTransportResult:
        return hdc.HdcTransportResult(0, "", "")

    def _op_faultprobe(self) -> hdc.HdcTransportResult:
        if self.scenario.fault_probe_fails:
            return hdc.HdcTransportResult(1, "", "find failed\n")
        names = set(self.scenario.snapshot_files)
        if self.fault_materialized:
            names |= set(self.scenario.fault_files)
        return hdc.HdcTransportResult(
            0, "".join(hdc.FAULTLOGGER_DIR + "/" + n + "\n" for n in sorted(names)), "")

    def _op_forcestop(self) -> hdc.HdcTransportResult:
        self.ui_alive = False
        if self.vpn_alive:
            self._mark_vpn_dead()
        self.force_stop_reasons.append("recorded")
        return hdc.HdcTransportResult(0, "", "")

    def _op_uninstall(self) -> hdc.HdcTransportResult:
        self.installed = False
        return hdc.HdcTransportResult(0, "", "")

    def _op_removestaging(self) -> hdc.HdcTransportResult:
        self.staged = False
        self.hap_sent = False
        return hdc.HdcTransportResult(0, "", "")

    def _op_stagingprobe(self) -> hdc.HdcTransportResult:
        if not self.staged:
            return hdc.HdcTransportResult(
                1, "", "ls: %s: No such file or directory\n" % hdc.STAGING_ROOT)
        return hdc.HdcTransportResult(0, "drwxrwxrwx ... %s\n" % hdc.STAGING_ROOT, "")

    def open_stream(self, argv: Sequence[str]) -> Iterator[str]:
        self._validate(argv)
        return self._stream_lines()


# ---------------------------------------------------------------------------
# 内置剧本：DryRun 冒烟用
# ---------------------------------------------------------------------------

def _fd(short_fd: str, inst: int, action: str, at: int, fd="none", by="none",
        cause="none") -> List[FakeMarker]:
    return [FakeMarker("FD", at, {"fd": str(fd), "role": short_fd,
                                  "inst": str(inst), "action": action,
                                  "at_mono_ms": str(at), "by": by, "cause": cause},
                       tag_form="complete")]


#: u3hex chunk 帧合成（48 B = PI 4 B ``00 00 08 00`` + IPv4 20 B total_length=44
#: + UDP 8 B + payload 16 B ``"N1DISCD4"``）——**D4 冻结身份方向**（B1/M2，镜像
#: d4.rs:43-44/:104-107 与 net.rs dst_peer）：src=10.99.0.1（tun 本地）→
#: **dst=10.99.0.2（发送目的）**、dport=**47001**（d4.rs ``h.dst == dest &&
#: h.udp_dport == 47001``）。offset-0 不可解析（PI 前缀 version=0）、offset-4
#: 可解析 → S5 分区 ``tun_pi-like``（规格 :551-561）；readlen 48 > total_length 44。
_U3HEX_FRAME_HEX = (
    "00000800"                                     # PI 前缀（flags=0, proto=0x0800）
    "4500002c" "00010000" "4011" "0000"            # IPv4：ver4/IHL5, total=44, proto17
    "0a630001" "0a630002"                          # src 10.99.0.1 → dst 10.99.0.2（发送目的）
    "b799b799" "0018" "0000"                       # UDP：sport 47001→dport 47001, len=24
    "4e31444953434434" "01" "0001" "5a5a5a5a5a"    # "N1DISCD4" + round/seq/pad
)
assert len(_U3HEX_FRAME_HEX) == 96


def _not_attempted_after_accept(at_mono_ms: int) -> List[FakeMarker]:
    """first-accept 后未执行条目的 not_attempted 发射（ets:249-252 逐字形态；
    B3/M2 producer-conformant：MR1B/MR2/MR3/MB1 各一条 outcome=not_attempted）。"""
    return [FakeMarker("D2_ENTRY", at_mono_ms + i,
                       {"id": eid, "outcome": "not_attempted"})
            for i, eid in enumerate(("MR1B", "MR2", "MR3", "MB1"), start=1)]


def make_happy_path_scenario() -> FakeScenario:
    """完整 happy-path：P0-P12 全 marker、正常类（fd-event-like）、POST（complete）。"""
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D1_LOADED", 110, {"ok": "true"}, "truncated"),
        FakeMarker("D1_SYM", 120, {"sym": "boringtun"}, "truncated"),
        FakeMarker("D1_END", 130, {"ok": "true"}, "truncated"),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "resolved"}),
    ]
    m += _not_attempted_after_accept(140)
    m += _fd("fd_orig", 1, "create", 150, fd="7")
    m += _fd("fd_dup", 1, "create", 160, fd="8")
    for i in range(1, 8):
        kv = {"step": str(i), "ok": "true"}
        if i == 4:
            kv["u6"] = "o_nonblock_absent"      # 规格 :492 D2_S4 u6 字面
        m.append(FakeMarker("D2_S%d" % i, 160 + i, kv))
    m += [
        FakeMarker("D4_BEGIN", 200, {"mono_ms": "200"}),
        FakeMarker("D4_SENT", 210, {"n": "1", "ret": "16", "errno": "0"}),
        FakeMarker("D4_READ", 220, {"off": "4", "len": "48"}),
        FakeMarker("D4_END", 230, {"u1": "observed-true"}),
    ]
    m += chunk_markers(_U3HEX_FRAME_HEX, "u3hex", 0, 235)
    m += _fd("d4_send_socket", 1, "create", 240, fd="9")
    m += [
        FakeMarker("D5_BEGIN", 300, {"mono_ms": "300"}),
        FakeMarker("D5_WRITE", 310, {"round": "1", "ret": "44", "errno": "0"}),
        FakeMarker("D5_RECV", 320, {"round": "1", "src": "10.99.0.2:47001"}),
        FakeMarker("D5_END", 330, {"u2": "observed-true"}),
    ]
    m += _fd("d5_sink_socket", 1, "create", 340, fd="10")
    m.append(FakeMarker("D8_MTU", 400, {"len": "1400", "ret": "0"}))
    m += chunk_markers("dlerror-detail-empty", "dlerror", 0, 410)
    m += [
        FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                 "skip_summary": "none"}),
        FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}),
        FakeMarker("D7_END", 31000, {"elapsed_ms": "20000", "iters": "400"}),
        FakeMarker("D8_STORM_BEGIN", 31100, {"ws": "31100"}),
        FakeMarker("D8_STORM_END", 41100, {"we": "41100", "eagain": "observed-true",
                                           "partial": "observed-false", "bytes": "4194304",
                                           "calls": "40000", "caps_hit": "none"}),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "create", 50100, fd="11")
    m += [
        FakeMarker("DW_SPAWN", 50200, {"tid": "20003"}, "complete"),
        FakeMarker("DW_DRAIN", 50300, {"elapsed_ms": "100", "timeout": "false",
                                       "end": "eagain", "reads": "1",
                                       "bytes": "0", "eintr_retries": "0"},
                   "complete"),
        FakeMarker("DW_BARRIER", 50400, {"mono_ms": "50400"}, "complete"),
        FakeMarker("DW_INWAIT", 50500, {"src": "both", "confirmed": "true",
                                        "samples": "3", "errno": "0"},
                   "complete"),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "close", 50600, fd="11", by="probe-protocol-close")
    m += [
        FakeMarker("DW_DESTROY_T", 60000, {"mono_ms": "60000"}, "complete"),
        FakeMarker("DW_DESTROY_C", 60010, {"mono_ms": "60010"}, "complete"),
        FakeMarker("D6S1_B", 60100, {}, "complete"),
        FakeMarker("D6S1_R", 60110, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S2_B", 60120, {}, "complete"),
        FakeMarker("D6S2_R", 60130, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S3_B", 60140, {}, "complete"),
        FakeMarker("D6S3_R", 60150, {"ret": "-1", "errno": "9"}, "complete"),
        FakeMarker("DW_RETURN", 60200,
                   {"ret": "1", "errno": "0", "revents": str(hdc.core.POLLERR),
                    "at_mono_ms": "60200", "elapsed_ms": "200"}, "complete"),
        FakeMarker("DW_EXIT", 60210, {"code": "0"}, "complete"),
        FakeMarker("D6S4_B", 60300, {}, "complete"),
        FakeMarker("D6S4_R", 60310, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S5_B", 60320, {}, "complete"),
        FakeMarker("D6S5_R", 60330, {"ret": "-1", "errno": "11"}, "complete"),
        FakeMarker("D6S6_B", 60340, {}, "complete"),
        FakeMarker("D6S6_R", 60350, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S7_B", 60360, {}, "complete"),
        FakeMarker("D6S7_R", 60370, {"fd": "12", "reuse": "false"}, "complete"),
    ]
    m += _fd("d6b_reuse_probe_socket", 1, "create", 60360, fd="12")
    m += _fd("d6b_reuse_probe_socket", 1, "close", 60380, fd="12",
             by="probe-protocol-close")
    m += _fd("d4_send_socket", 1, "close", 70000, fd="9", by="probe-protocol-close")
    m += _fd("d5_sink_socket", 1, "close", 70010, fd="10", by="probe-protocol-close")
    m.append(FakeMarker("POST", 80000,
                        {"d6_items": "POST_D6_ITEMS_PLACEHOLDER",
                         "dw_outcome": "POST_DW_OUTCOME_PLACEHOLDER",
                         "ledger_digest": "POST_DIGEST_PLACEHOLDER",
                         "worker_terminal_at_p12": "true"}))
    # B4-c：runner 侧 pthread_join 返回登记（live 形态由 fsm 轮询供给；happy
    # 主线 join 成功返回 0 → runner 重建 joined，与 POST join=joined 比对一致）。
    return FakeScenario(markers=m, die_at_end=False, join_exit_rc=0)


def make_pre_only_scenario() -> FakeScenario:
    """pre-only 剧本：PRE 后死于 D7（无 D7_END），SIGKILL fault 条目（平台终止）。"""
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D1_LOADED", 110, {"ok": "true"}, "truncated"),
        FakeMarker("D1_SYM", 120, {"sym": "boringtun"}, "truncated"),
        FakeMarker("D1_END", 130, {"ok": "true"}, "truncated"),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "resolved"}),
    ]
    m += _not_attempted_after_accept(140)
    m += _fd("fd_orig", 1, "create", 150, fd="7")
    m += _fd("fd_dup", 1, "create", 160, fd="8")
    for i in range(1, 8):
        kv = {"step": str(i), "ok": "true"}
        if i == 4:
            kv["u6"] = "o_nonblock_absent"
        m.append(FakeMarker("D2_S%d" % i, 160 + i, kv))
    m += [
        FakeMarker("D4_BEGIN", 200, {"mono_ms": "200"}),
        FakeMarker("D4_SENT", 210, {"n": "1", "ret": "16", "errno": "0"}),
        FakeMarker("D4_READ", 220, {"off": "4", "len": "48"}),
        FakeMarker("D4_END", 230, {"u1": "observed-true"}),
    ]
    m += chunk_markers(_U3HEX_FRAME_HEX, "u3hex", 0, 235)
    m += _fd("d4_send_socket", 1, "create", 240, fd="9")
    m += [
        FakeMarker("D5_BEGIN", 300, {"mono_ms": "300"}),
        FakeMarker("D5_WRITE", 310, {"round": "1", "ret": "44", "errno": "0"}),
        FakeMarker("D5_RECV", 320, {"round": "1", "src": "10.99.0.2:47001"}),
        FakeMarker("D5_END", 330, {"u2": "observed-true"}),
    ]
    m += _fd("d5_sink_socket", 1, "create", 340, fd="10")
    m.append(FakeMarker("D8_MTU", 400, {"len": "1400", "ret": "0"}))
    m.append(FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                      "skip_summary": "none"}))
    m.append(FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}))
    # 死于 D7 任务中途：无 D7_END 及其后任何 marker。
    snapshot = FaultFileSpec(
        "faultlogger-0001-preexisting-cn.alfadb.netbird.n1bdisc",
        fault_entry_text("APPFREEZE", None, "2026-08-01 09:00:00.000"))
    fresh = FaultFileSpec(
        "faultlogger-0002-cn.alfadb.netbird.n1bdisc",
        fault_entry_text("APPFREEZE", "SIGKILL", "2026-09-06 10:00:00.000"))
    return FakeScenario(markers=m, fault_files=[snapshot, fresh],
                        snapshot_files=[snapshot.file_name], die_at_end=True)


def make_no_live_fd_scenario() -> FakeScenario:
    """五 create 全拒 → no-live-fd 分支：POST 照发（skip 编码，规格 :1543 验收形态）。"""
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D1_LOADED", 110, {"ok": "true"}, "truncated"),
        FakeMarker("D1_SYM", 120, {"sym": "boringtun"}, "truncated"),
        FakeMarker("D1_END", 130, {"ok": "true"}, "truncated"),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "rejected"}),
        FakeMarker("D2_ENTRY", 141, {"id": "MR1B", "outcome": "rejected"}),
        FakeMarker("D2_ENTRY", 142, {"id": "MR2", "outcome": "rejected"}),
        FakeMarker("D2_ENTRY", 143, {"id": "MR3", "outcome": "rejected"}),
        FakeMarker("D2_ENTRY", 144, {"id": "MB1", "outcome": "rejected"}),
        FakeMarker("SKIP", 150, {"item": "destroy", "cause": "no-live-connection"},
                   "entry"),
        FakeMarker("SKIP", 151, {"item": "D4", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 152, {"item": "D5", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 153, {"item": "D8a", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 154, {"item": "D7", "cause": "no-live-vpn"}, "entry"),
        FakeMarker("SKIP", 155, {"item": "D8b", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 156, {"item": "D6a", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 160, {"item": "D-W", "cause": "no-live-fd"}, "entry"),
        FakeMarker("SKIP", 161, {"item": "D6b", "cause": "no-live-fd"}, "entry"),
        FakeMarker("PRE", 200, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                "skip_summary": "destroy:no-live-connection"}),
        # producer cut=f（dw.rs:839：!spawned 支 ``cut: f``——worker 从未 spawn、
        # 终态标志从未置位 → worker_terminal_at_p12=false）。
        FakeMarker("POST", 300,
                   {"d6_items": "POST_D6_ITEMS_PLACEHOLDER",
                    "dw_outcome": "POST_DW_OUTCOME_PLACEHOLDER",
                    "ledger_digest": "POST_DIGEST_PLACEHOLDER",
                    "worker_terminal_at_p12": "false"}),
    ]
    return FakeScenario(markers=m, die_at_end=False)


def make_flag_race_scenario() -> FakeScenario:
    """complete 形态但 ``worker_terminal_at_p12=false``（真机 flag-race 格）。

    观察 (i) 端到端验证格：POST 在（close_kind=complete-seal）而 FLAG=false——
    探针 POST digest 恒按 open-at-exit 切口算（probe/src/dw.rs:1006），runner 最终
    重建必须同切口（close_kind 驱动；判据 :390-395 限同一切点）。账本含 POST 时仍
    open 的条目（fd_orig/fd_dup 全程无 close），complete 切口（open-at-exit）与
    pre-only 切口（process-exit）digest 必然不同 → 旧 wtap 启发在此格必假 F5。

    cut-state (B)（:777-788）闭表对齐：RACEWIN 在（r22 仅此格发射，dw.rs:989-991）；
    class 与 poll raw 四字段 = ``unobservable(cause=flag-race-window-expired)``；
    watchdog = ⑤ ``unobservable(cause=marker-gap-indeterminate)``。poll 未返回
   （无 DW_RETURN/DW_EXIT，:738-749 前件）、join 超时弃收（join-timeout）、
    destroy 已调用并返回（_T/_C 在）。
    """
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "resolved"}),
    ]
    m += _not_attempted_after_accept(140)
    m += _fd("fd_orig", 1, "create", 150, fd="7")
    m += _fd("fd_dup", 1, "create", 160, fd="8")
    for i in range(1, 8):
        kv = {"step": str(i), "ok": "true"}
        if i == 4:
            kv["u6"] = "o_nonblock_absent"
        m.append(FakeMarker("D2_S%d" % i, 160 + i, kv))
    m += [
        FakeMarker("D4_BEGIN", 200, {"mono_ms": "200"}),
        FakeMarker("D4_SENT", 210, {"n": "1", "ret": "16", "errno": "0"}),
        FakeMarker("D4_READ", 220, {"off": "4", "len": "48"}),
        FakeMarker("D4_END", 230, {"u1": "observed-true"}),
    ]
    m += chunk_markers(_U3HEX_FRAME_HEX, "u3hex", 0, 235)
    m += _fd("d4_send_socket", 1, "create", 240, fd="9")
    m += [
        FakeMarker("D5_BEGIN", 300, {"mono_ms": "300"}),
        FakeMarker("D5_WRITE", 310, {"round": "1", "ret": "44", "errno": "0"}),
        FakeMarker("D5_RECV", 320, {"round": "1", "src": "10.99.0.2:47001"}),
        FakeMarker("D5_END", 330, {"u2": "observed-true"}),
    ]
    m += _fd("d5_sink_socket", 1, "create", 340, fd="10")
    m.append(FakeMarker("D8_MTU", 400, {"len": "1400", "ret": "0"}))
    m += chunk_markers("dlerror-detail-empty", "dlerror", 0, 410)
    m += [
        FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                 "skip_summary": "none"}),
        FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}),
        FakeMarker("D7_END", 31000, {"elapsed_ms": "20000", "iters": "400"}),
        FakeMarker("D8_STORM_BEGIN", 31100, {"ws": "31100"}),
        FakeMarker("D8_STORM_END", 41100, {"we": "41100",
                                           "eagain": "observed-true",
                                           "partial": "observed-false",
                                           "bytes": "4194304",
                                           "calls": "40000", "caps_hit": "none"}),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "create", 50100, fd="11")
    m += [
        FakeMarker("DW_SPAWN", 50200, {"tid": "20003"}, "complete"),
        FakeMarker("DW_DRAIN", 50300, {"elapsed_ms": "100", "timeout": "false",
                                       "end": "eagain", "reads": "1",
                                       "bytes": "0", "eintr_retries": "0"},
                   "complete"),
        FakeMarker("DW_BARRIER", 50400, {"mono_ms": "50400"}, "complete"),
        FakeMarker("DW_INWAIT", 50500, {"src": "both", "confirmed": "true",
                                        "samples": "3", "errno": "0"},
                   "complete"),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "close", 50600, fd="11", by="probe-protocol-close")
    m += [
        FakeMarker("DW_DESTROY_T", 60000, {"mono_ms": "60000"}, "complete"),
        FakeMarker("DW_DESTROY_C", 60010, {"mono_ms": "60010"}, "complete"),
        # r18 重裁（规格 :970）：join-timeout 形态 D6b 整段 skip → 发
        # N1BDISC_SKIP|item=D6b|cause=join-timeout-abandoned（:747 JT 登记载体、
        # :693 join sticky 输入、u4_dup_* 四字段 d6b-skipped-join-timeout）
        FakeMarker("SKIP", 60200, {"item": "D6b", "cause": "join-timeout-abandoned"},
                   "complete"),
    ]
    m += _fd("d4_send_socket", 1, "close", 70000, fd="9", by="probe-protocol-close")
    m += _fd("d5_sink_socket", 1, "close", 70010, fd="10", by="probe-protocol-close")
    # r22 冻结发射序：RACEWIN → POST（dw.rs:986-991，仅 flag-race 格发射）
    m.append(FakeMarker("DW_RACEWIN", 79000, {"expired": "1"}, "complete"))
    m.append(FakeMarker("POST", 80000,
                        {"d6_items": "POST_D6_ITEMS_PLACEHOLDER",
                         "dw_outcome": "POST_DW_OUTCOME_PLACEHOLDER",
                         "ledger_digest": "POST_DIGEST_PLACEHOLDER",
                         "worker_terminal_at_p12": "false"}))
    return FakeScenario(markers=m, die_at_end=False)


def _p2_prefix() -> List[FakeMarker]:
    """P1-P5 共用前缀（D1/D2 锁定序列 + D4/D5 完成 + D8a；u1-u6 有值形态）。"""
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D1_LOADED", 110, {"ok": "true"}, "truncated"),
        FakeMarker("D1_SYM", 120, {"sym": "boringtun"}, "truncated"),
        FakeMarker("D1_END", 130, {"ok": "true"}, "truncated"),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "resolved"}),
    ]
    m += _not_attempted_after_accept(140)
    m += _fd("fd_orig", 1, "create", 150, fd="7")
    m += _fd("fd_dup", 1, "create", 160, fd="8")
    for i in range(1, 8):
        kv = {"step": str(i), "ok": "true"}
        if i == 4:
            kv["u6"] = "o_nonblock_absent"
        m.append(FakeMarker("D2_S%d" % i, 160 + i, kv))
    m += [
        FakeMarker("D4_BEGIN", 200, {"mono_ms": "200"}),
        FakeMarker("D4_SENT", 210, {"n": "1", "ret": "16", "errno": "0"}),
        FakeMarker("D4_READ", 220, {"off": "4", "len": "48"}),
        FakeMarker("D4_END", 230, {"u1": "observed-true"}),
    ]
    m += chunk_markers(_U3HEX_FRAME_HEX, "u3hex", 0, 235)
    m += _fd("d4_send_socket", 1, "create", 240, fd="9")
    m += [
        FakeMarker("D5_BEGIN", 300, {"mono_ms": "300"}),
        FakeMarker("D5_WRITE", 310, {"round": "1", "ret": "44", "errno": "0"}),
        FakeMarker("D5_RECV", 320, {"round": "1", "src": "10.99.0.2:47001"}),
        FakeMarker("D5_END", 330, {"u2": "observed-true"}),
    ]
    m += _fd("d5_sink_socket", 1, "create", 340, fd="10")
    m.append(FakeMarker("D8_MTU", 400, {"len": "1400", "ret": "0"}))
    return m


_SIGKILL_ENTRY = ("faultlogger-9001-cn.alfadb.netbird.n1bdisc",
                  fault_entry_text("APPFREEZE", "SIGKILL",
                                   "2026-09-06 10:00:00.000"))


def make_death_after_pre_scenario() -> FakeScenario:
    """stage-not-reached 剧本格（r13 u7 阶段未达正例，规格 :1461；D8b 阶段未达
    支 :609-615）：PRE 在、``D7_BEGIN`` 缺（D7 未开始）、平台死亡
    （positive 基线在、finally 采样 absent + capture 静默）、无窄崩溃签名、
    ``last_visible_site = P5T`` → u7 与 D8b 七字段各
    ``unobservable(cause=stage-not-reached)``、protocol=pre-only → pass。"""
    m = _p2_prefix()
    m.append(FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                      "skip_summary": "none"}))
    # PRE 后死亡：无 D7_BEGIN 及其后任何 marker。
    return FakeScenario(markers=m, die_at_end=True)


def make_storm_death_scenario() -> FakeScenario:
    """storm-incomplete-pre-only 剧本格（BEGIN-only 死亡收口正例，规格 :1456-1457）：
    PRE 在、D7 完成、``D8_STORM_BEGIN|ws=<n>`` 在、``D8_STORM_END`` 缺、平台
    SIGKILL ``:vpn``、无窄崩溃签名 → END 承载五字段 + ``window_end`` 各
    ``unobservable(cause=storm-incomplete-pre-only)``、``window_start`` 自 ws 照常
    重建、``storm_incomplete_pre_only=true`` 与跨度在档 → pre-only → pass。"""
    m = _p2_prefix()
    m += [
        FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                 "skip_summary": "none"}),
        FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}),
        FakeMarker("D7_END", 31000, {"elapsed_ms": "20000", "iters": "400"}),
        FakeMarker("D8_STORM_BEGIN", 31100, {"ws": "31100"}),
    ]
    # storm 中途死亡：无 D8_STORM_END 及其后任何 marker。
    snapshot = FaultFileSpec(
        "faultlogger-0001-preexisting-cn.alfadb.netbird.n1bdisc",
        fault_entry_text("APPFREEZE", None, "2026-08-01 09:00:00.000"))
    fresh = FaultFileSpec(_SIGKILL_ENTRY[0], _SIGKILL_ENTRY[1])
    return FakeScenario(markers=m, fault_files=[snapshot, fresh],
                        snapshot_files=[snapshot.file_name], die_at_end=True)


def make_alive_incomplete_scenario() -> FakeScenario:
    """F9 存活未完成剧本格（规格 :1152/:1067）：PRE 在、POST 缺、无死亡证据
    （positive 基线在、窗内 ``:vpn`` 持续在场）、观测窗到点 → ``fail``（F9，
    存活未完成不是平台事实）；D-W 照常跑完（EXIT 在 → join 轴 joined）。"""
    m = _p2_prefix()
    m += chunk_markers("dlerror-detail-empty", "dlerror", 0, 410)
    m += [
        FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                 "skip_summary": "none"}),
        FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}),
        FakeMarker("D7_END", 31000, {"elapsed_ms": "20000", "iters": "400"}),
        FakeMarker("D8_STORM_BEGIN", 31100, {"ws": "31100"}),
        FakeMarker("D8_STORM_END", 41100, {"we": "41100", "eagain": "observed-true",
                                           "partial": "observed-false", "bytes": "4194304",
                                           "calls": "40000", "caps_hit": "none"}),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "create", 50100, fd="11")
    m += [
        FakeMarker("DW_SPAWN", 50200, {"tid": "20003"}, "complete"),
        FakeMarker("DW_DRAIN", 50300, {"elapsed_ms": "100", "timeout": "false",
                                       "end": "eagain", "reads": "1",
                                       "bytes": "0", "eintr_retries": "0"},
                   "complete"),
        FakeMarker("DW_BARRIER", 50400, {"mono_ms": "50400"}, "complete"),
        FakeMarker("DW_INWAIT", 50500, {"src": "both", "confirmed": "true",
                                        "samples": "3", "errno": "0"},
                   "complete"),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "close", 50600, fd="11", by="probe-protocol-close")
    m += [
        FakeMarker("DW_DESTROY_T", 60000, {"mono_ms": "60000"}, "complete"),
        FakeMarker("DW_DESTROY_C", 60010, {"mono_ms": "60010"}, "complete"),
        FakeMarker("D6S1_B", 60100, {}, "complete"),
        FakeMarker("D6S1_R", 60110, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S2_B", 60120, {}, "complete"),
        FakeMarker("D6S2_R", 60130, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S3_B", 60140, {}, "complete"),
        FakeMarker("D6S3_R", 60150, {"ret": "-1", "errno": "9"}, "complete"),
        FakeMarker("DW_RETURN", 60200,
                   {"ret": "1", "errno": "0", "revents": str(hdc.core.POLLERR),
                    "at_mono_ms": "60200", "elapsed_ms": "200"}, "complete"),
        FakeMarker("DW_EXIT", 60210, {"code": "0"}, "complete"),
        FakeMarker("D6S4_B", 60300, {}, "complete"),
        FakeMarker("D6S4_R", 60310, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S5_B", 60320, {}, "complete"),
        FakeMarker("D6S5_R", 60330, {"ret": "-1", "errno": "11"}, "complete"),
        FakeMarker("D6S6_B", 60340, {}, "complete"),
        FakeMarker("D6S6_R", 60350, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S7_B", 60360, {}, "complete"),
        FakeMarker("D6S7_R", 60370, {"fd": "12", "reuse": "false"}, "complete"),
    ]
    # 无 POST、die_at_end=False：进程持续在场 → 存活未完成（F9 面）。
    return FakeScenario(markers=m, die_at_end=False)


def make_death_in_d6b_scenario() -> FakeScenario:
    """u4 V2 (3) 支剧本格（规格 :1192：``_C`` 已发出且 destroy 已 resolve →
    死亡后未执行子项逐项 ``observed-false``；死亡前 D6a 正结果保持，:1186）：
    PRE..D6a 完成、D6S4_B 在而 D6S4_R 缺时进程死亡 → pre-only → pass。"""
    m = _p2_prefix()
    m += chunk_markers("dlerror-detail-empty", "dlerror", 0, 410)
    m += [
        FakeMarker("PRE", 1000, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                 "skip_summary": "none"}),
        FakeMarker("D7_BEGIN", 1100, {"start_mono_ms": "1100"}),
        FakeMarker("D7_END", 31000, {"elapsed_ms": "20000", "iters": "400"}),
        FakeMarker("D8_STORM_BEGIN", 31100, {"ws": "31100"}),
        FakeMarker("D8_STORM_END", 41100, {"we": "41100", "eagain": "observed-true",
                                           "partial": "observed-false", "bytes": "4194304",
                                           "calls": "40000", "caps_hit": "none"}),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "create", 50100, fd="11")
    m += [
        FakeMarker("DW_SPAWN", 50200, {"tid": "20003"}, "complete"),
        FakeMarker("DW_DRAIN", 50300, {"elapsed_ms": "100", "timeout": "false",
                                       "end": "eagain", "reads": "1",
                                       "bytes": "0", "eintr_retries": "0"},
                   "complete"),
        FakeMarker("DW_BARRIER", 50400, {"mono_ms": "50400"}, "complete"),
        FakeMarker("DW_INWAIT", 50500, {"src": "both", "confirmed": "true",
                                        "samples": "3", "errno": "0"},
                   "complete"),
    ]
    m += _fd("dw_inwait_proc_fd", 1, "close", 50600, fd="11", by="probe-protocol-close")
    m += [
        FakeMarker("DW_DESTROY_T", 60000, {"mono_ms": "60000"}, "complete"),
        FakeMarker("DW_DESTROY_C", 60010, {"mono_ms": "60010"}, "complete"),
        FakeMarker("D6S1_B", 60100, {}, "complete"),
        FakeMarker("D6S1_R", 60110, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S2_B", 60120, {}, "complete"),
        FakeMarker("D6S2_R", 60130, {"ret": "0", "errno": "0"}, "complete"),
        FakeMarker("D6S3_B", 60140, {}, "complete"),
        FakeMarker("D6S3_R", 60150, {"ret": "-1", "errno": "9"}, "complete"),
        FakeMarker("DW_RETURN", 60200,
                   {"ret": "1", "errno": "0", "revents": str(hdc.core.POLLERR),
                    "at_mono_ms": "60200", "elapsed_ms": "200"}, "complete"),
        FakeMarker("DW_EXIT", 60210, {"code": "0"}, "complete"),
        FakeMarker("D6S4_B", 60300, {}, "complete"),
        # 死于 D6b 中途：D6S4_R 及其后任何 marker 缺。
    ]
    snapshot = FaultFileSpec(
        "faultlogger-0001-preexisting-cn.alfadb.netbird.n1bdisc",
        fault_entry_text("APPFREEZE", None, "2026-08-01 09:00:00.000"))
    fresh = FaultFileSpec(_SIGKILL_ENTRY[0], _SIGKILL_ENTRY[1])
    return FakeScenario(markers=m, fault_files=[snapshot, fresh],
                        snapshot_files=[snapshot.file_name], die_at_end=True)


# ---------------------------------------------------------------------------
# gate 3 整改反例剧本（B1/B2/B3/B4/B5 端到端反例钉，M2）
# ---------------------------------------------------------------------------

def _clone_markers(scenario: "FakeScenario") -> List[FakeMarker]:
    return [FakeMarker(m.short, m.at_mono_ms, dict(m.kv), m.tag_form, m.late)
            for m in scenario.markers]


#: 外来包首 64 字节（d4.rs:127-131 foreign 流载体）：可解析 IPv4+UDP 但
#: dst=198.51.100.7 / dport=53 / payload 非 N1DISCD4 族——身份不匹配。
_FOREIGN_FRAME_HEX = (
    "45000030" "00010000" "4011" "0000"
    "c6336401" "c6336407"          # src 198.51.100.1 → dst 198.51.100.7
    "9a3f" "0035" "0000"           # UDP sport 39487 → dport 53
    "deadbeef" "cafe" "000102030405060708090a0b0c0d0e0f1011"
)


def make_foreign_packet_scenario() -> FakeScenario:
    """B1 反例：外来可解析 IPv4 包（``D4_READ|off=4`` + foreign chunk）绝不产生
    observed-true——sendto 前提成立（ret=16）、窗口收口（``D4_END|u1=
    observed-false``）→ u1=observed-false、u3 全字段 no-controlled-read。"""
    m = [x for x in _clone_markers(make_happy_path_scenario())
         if not (x.short == "CHUNK" and x.kv.get("stream") == "u3hex")]
    for x in m:
        if x.short == "D4_END":
            x.kv["u1"] = "observed-false"
    foreign_at = next(x.at_mono_ms for x in m if x.short == "D4_READ")
    m += chunk_markers(_FOREIGN_FRAME_HEX, "foreign", 0, foreign_at + 1)
    m.sort(key=lambda x: x.at_mono_ms)
    return FakeScenario(markers=m, die_at_end=False, join_exit_rc=0)


def make_bad_payload_scenario() -> FakeScenario:
    """B2 反例：RECV src 形态与冻结 src:port 逐字相符而 payload 身份未证实
    （探针 E10 收口 observed-false）→ u2=observed-false，绝不折算 true。"""
    m = _clone_markers(make_happy_path_scenario())
    for x in m:
        if x.short == "D5_END":
            x.kv["u2"] = "observed-false"
    return FakeScenario(markers=m, die_at_end=False, join_exit_rc=0)


def make_not_attempted_timeout_scenario() -> FakeScenario:
    """B3 反例（timeout 终止族）：MR1 timeout → 迟到窗尽 indeterminate（m-07
    双行）→ 矩阵终止、其余条目 not_attempted（ets:232-235 形态）→ u5 =
    create-indeterminate / matrix-terminated-on-create-timeout；无保留条目 →
    no-live-fd 分支收口（PRE+POST 照发，complete pass）。"""
    m: List[FakeMarker] = [
        FakeMarker("D1_BEGIN", 100, {"mono_ms": "100"}),
        FakeMarker("D1_END", 130, {"ok": "true"}, "truncated"),
        FakeMarker("D2_ENTRY", 140, {"id": "MR1", "outcome": "timeout"}),
        FakeMarker("D2_ENTRY", 141, {"id": "MR1", "outcome": "indeterminate"}),
        FakeMarker("D2_ENTRY", 142, {"id": "MR1B", "outcome": "not_attempted"}),
        FakeMarker("D2_ENTRY", 143, {"id": "MR2", "outcome": "not_attempted"}),
        FakeMarker("D2_ENTRY", 144, {"id": "MR3", "outcome": "not_attempted"}),
        FakeMarker("D2_ENTRY", 145, {"id": "MB1", "outcome": "not_attempted"}),
        FakeMarker("SKIP", 150, {"item": "D4", "cause": "no-live-fd"}),
        FakeMarker("SKIP", 151, {"item": "D5", "cause": "no-live-fd"}),
        FakeMarker("SKIP", 152, {"item": "D8a", "cause": "no-live-fd"}),
        FakeMarker("PRE", 200, {"ledger_digest": "PRE_DIGEST_PLACEHOLDER",
                                "skip_summary":
                                    "D4:no-live-fd,D5:no-live-fd,"
                                    "D8a:no-live-fd"}),
        FakeMarker("SKIP", 210, {"item": "D7", "cause": "no-live-vpn"}),
        FakeMarker("SKIP", 211, {"item": "D8b", "cause": "no-live-fd"}),
        FakeMarker("SKIP", 212, {"item": "destroy",
                                 "cause": "no-live-connection"}),
        FakeMarker("SKIP", 213, {"item": "D6a", "cause": "no-live-fd"}),
        FakeMarker("SKIP", 214, {"item": "D-W", "cause": "no-live-fd"}),
        FakeMarker("SKIP", 215, {"item": "D6b", "cause": "no-live-fd"}),
        # producer cut=f（dw.rs:839 !spawned 支）。
        FakeMarker("POST", 300,
                   {"d6_items": "POST_D6_ITEMS_PLACEHOLDER",
                    "dw_outcome": "POST_DW_OUTCOME_PLACEHOLDER",
                    "ledger_digest": "POST_DIGEST_PLACEHOLDER",
                    "worker_terminal_at_p12": "false"}),
    ]
    return FakeScenario(markers=m, die_at_end=False)


def make_join_bogus_scenario() -> FakeScenario:
    """B4-b 反例：POST ``dw_outcome`` 内层 ``join=pending``（域外字面——修复前
    探针 skip 形态/探针 bug）+ runner 侧 join rc 登记（=0 → 重建 joined）→
    严格十值域解析不一致 → F8(2) fail。"""
    m = _clone_markers(make_happy_path_scenario())
    return FakeScenario(markers=m, die_at_end=False, join_exit_rc=0,
                        post_join_override="pending")


def make_d8b_bogus_scenario() -> FakeScenario:
    """B5 反例：``D8_STORM_END`` 五值 bogus（eagain 域外三态 / bytes 非整数 /
    we 负值——M1 单调钟域）→ 逐字段 F8 fail-closed，不洗白为缺项。"""
    m = _clone_markers(make_happy_path_scenario())
    for x in m:
        if x.short == "D8_STORM_END":
            x.kv.update({"eagain": "maybe", "bytes": "abc", "we": "-5"})
    return FakeScenario(markers=m, die_at_end=False, join_exit_rc=0)


__all__ = [
    "FakeMarker", "chunk_markers", "duplicate_chunk_markers", "FaultFileSpec",
    "fault_entry_text", "FakeScenario", "FakeHdc", "format_hilog_line",
    "make_happy_path_scenario", "make_pre_only_scenario",
    "make_no_live_fd_scenario", "make_flag_race_scenario",
    "make_death_after_pre_scenario", "make_storm_death_scenario",
    "make_alive_incomplete_scenario", "make_death_in_d6b_scenario",
    "make_foreign_packet_scenario", "make_bad_payload_scenario",
    "make_not_attempted_timeout_scenario", "make_join_bogus_scenario",
    "make_d8b_bogus_scenario",
]
