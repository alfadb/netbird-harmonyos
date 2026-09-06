# -*- coding: utf-8 -*-
"""n1bdisc_core — N1B DISC host runner 纯函数核心库（host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结，本实现以其实际行号为准）。

本模块只实现四块"纯解析/派生"，全部为无 I/O 副作用、无网络、无 hdc 的纯函数；
状态机与 HDC 执行器由后续增量基于本模块公共 API 实现。

1. hilog 行关联与 marker 解析（规格 :415-431 采集关联规则、:1095-1097 冻结 marker 清单）
2. N1BDISC_CHUNK 分段重组（规格 :419-431，含 r4 S6 重复片处置、r5 U12 字面校验）
3. N1BDISC_FD ledger 重建（规格 :366-407 canonical 序列化/配对状态机、:829-830 单调钟域门）
4. dw_return_class 四步派生（规格 :722-823）+ dw_join_result 10 值域基础映射（规格 :870）

模块导入零副作用：仅 stdlib、仅常量与纯函数定义。
"""

from __future__ import annotations

import base64
import binascii
import hashlib
import re
from dataclasses import dataclass, field
from typing import Any, Iterable, List, Mapping, Optional, Tuple

# ---------------------------------------------------------------------------
# 冻结常量（规格字面）
# ---------------------------------------------------------------------------

#: HiLog 通道 TAG（规格 :415 冻结 ``TAG = 'N1BDiscVpn'``）。
HILOG_TAG = "N1BDiscVpn"
#: N1B bundle 名（规格 :270 冻结 ``cn.alfadb.netbird.n1bdisc``）。
DEFAULT_BUNDLE = "cn.alfadb.netbird.n1bdisc"
MARKER_PREFIX = "N1BDISC_"

# 56 个冻结 marker 字面（规格 :1095-1097 逐字清单；r9 实现性更正：豁免集单列）
_FROZEN_SHORT: Tuple[str, ...] = (
    "D1_BEGIN", "D1_LOADED", "D1_FAIL", "D1_SYM", "D1_END",
    "D2_ENTRY", "D2_LATE", "D2_LATE_FD",
    "D2_S1", "D2_S2", "D2_S3", "D2_S4", "D2_S5", "D2_S6", "D2_S7",
    "D4_BEGIN", "D4_SENT", "D4_READ", "D4_END",
    "D5_BEGIN", "D5_WRITE", "D5_RECV", "D5_END",
    "D8_MTU", "D8_STORM_BEGIN", "D8_STORM_END",
    "D7_BEGIN", "D7_END",
    "DW_SPAWN", "DW_DRAIN", "DW_BARRIER", "DW_INWAIT",
    "DW_DESTROY_T", "DW_DESTROY_C", "DW_RETURN", "DW_EXIT", "DW_RACEWIN",
    "D6S1_B", "D6S2_B", "D6S3_B", "D6S4_B", "D6S5_B", "D6S6_B", "D6S7_B",
    "D6S1_R", "D6S2_R", "D6S3_R", "D6S4_R", "D6S5_R", "D6S6_R", "D6S7_R",
    "FD", "SKIP", "CHUNK", "PRE", "POST",
)
#: 56 个冻结 marker 全名（``N1BDISC_<SHORT>``）。
FROZEN_MARKERS = frozenset(MARKER_PREFIX + s for s in _FROZEN_SHORT)
assert len(FROZEN_MARKERS) == 56, "冻结 marker 字面必须恰 56 个（规格 :1100）"
#: A5 豁免集（规格 :1100）：历史/已废字面，不属 56 冻结集。
EXEMPT_MARKERS = frozenset({"N1BDISC_D2_REJTEXT", "N1BDISC_RESULT"})

# chunk 逻辑键（规格 :419-421 E3 冻结）
CHUNK_STREAMS: Tuple[str, ...] = ("dlerror", "rejtext", "u3hex", "foreign")
#: ``item`` 恒 0 的 stream（各恰一条记录）。
CHUNK_ITEM_ZERO_STREAMS: Tuple[str, ...] = ("dlerror", "u3hex", "foreign")
#: rejtext 的 item = 矩阵条目 id 编号域（规格 :421）。
REJTEXT_ITEM_IDS: Mapping[str, int] = {"MR1": 0, "MR1B": 1, "MR2": 2, "MR3": 3, "MB1": 4}
REJTEXT_ITEM_DOMAIN = frozenset(REJTEXT_ITEM_IDS.values())
#: 单片原始字节上限（规格 :427(a)）。
CHUNK_SLICE_MAX_BYTES = 256

# fd ledger（规格 :366-407）
#: 角色集 7 个（规格 :369，顺序即 canonical 排序的同角色 tie-break 依据 :385(c)）。
FD_ROLES: Tuple[str, ...] = (
    "fd_orig", "fd_dup", "d4_send_socket", "d5_sink_socket",
    "d6b_reuse_probe_socket", "dw_inwait_proc_fd", "d2_late_fd",
)
FD_ROLE_ORDER: Mapping[str, int] = {r: i for i, r in enumerate(FD_ROLES)}
FD_ACTIONS: Tuple[str, ...] = ("create", "close", "not-created")
#: ``closed_by`` 七值域（规格 :396-397，r5 U5 增 open-at-pre、r6 W3 恢复 process-exit）。
CLOSED_BY_VALUES: Tuple[str, ...] = (
    "destroy", "d6a-probe-close", "probe-protocol-close",
    "process-exit", "host-forcestop", "open-at-exit", "open-at-pre",
)
CLOSED_BY_DOMAIN = frozenset(CLOSED_BY_VALUES)
#: ledger 重建求值切点 → 仍 open 实例的 ``closed_by`` 登记（规格 :381-383、:400-404）。
LEDGER_CUTS: Tuple[str, ...] = ("complete", "pre-only", "pre-snapshot", "host-forcestop")
CUT_OPEN_CLOSED_BY: Mapping[str, str] = {
    "complete": "open-at-exit",        # :401
    "pre-only": "process-exit",        # :402（r6 W3 恢复字面）
    "pre-snapshot": "open-at-pre",     # :403（P5T 切点，r5 U5）
    "host-forcestop": "host-forcestop",  # :402（存活 fail-cleanup 形态）
}
#: pre-only 收口重建标注（规格 :387 r4 R1）。
REBUILT_FROM_TRANSITION_MARKER = "rebuilt-from-transition-marker"

# D-W poll raw（规格 :813-814 revents 编码冻结；值取目标 sysroot poll.h）
POLLIN = 0x001
POLLPRI = 0x002
POLLOUT = 0x004
POLLERR = 0x008
POLLHUP = 0x010
POLLNVAL = 0x020
#: 冻结掩码全集 {0x001..0x020}，已知位组合 ∈ 0..0x3F（规格 :814）。
KNOWN_REVENTS_MASK = POLLIN | POLLPRI | POLLOUT | POLLERR | POLLHUP | POLLNVAL
#: D-W 阈值：elapsed_ms 相对 4500 的三分带（0.9×T_dw=5000ms 冻结，规格 :722）。
DW_EAGAIN_THRESHOLD_MS = 4500
#: poll 返回类判定表中 ``errno == EINTR``（Linux/musl 通用 errno 编号）。
EINTR = 4
#: ``pthread_join`` 返回 ``ESRCH``（Linux/musl 通用 errno 编号；dw_join_result 本体值）。
_ESRCH = 3

# dw 值域（规格 :866/:870）
#: 普通判定表 13 类（行 0/0b + 1..11）。
DW_RETURN_CLASS_13: Tuple[str, ...] = (
    "destroy-skip-proven",        # 行 0
    "destroy-call-unobserved",    # 行 0b
    "interrupted", "poll-error", "fd-invalid", "pre-destroy-ready",
    "late-fd-event", "late-data", "fd-event-like", "data-ready-post-destroy",
    "timeout-like", "spurious-early", "other-revents",
)
#: class 域的 unobservable cause（skip 2 + pre-only 死亡收口 3 + (d)/(e) 收口 2，规格 :866）。
DW_UNOBSERVABLE_CAUSES_CLASS: Tuple[str, ...] = (
    "no-live-fd", "dup-failed",
    "destroy-not-reached", "post-destroy-unobservable", "call-boundary-incomplete",
    "poll-never-returned", "flag-race-window-expired",
)
#: join 域收口 cause（r17 核对结论：join 域不含 poll-never/flag-race，规格 :870/:746）。
DW_UNOBSERVABLE_CAUSES_JOIN: Tuple[str, ...] = (
    "no-live-fd", "dup-failed",
    "destroy-not-reached", "post-destroy-unobservable", "call-boundary-incomplete",
)
#: ``dw_join_result`` 本体 5 值（规格 :870）。
DW_JOIN_TERMINAL_VALUES: Tuple[str, ...] = (
    "joined", "join-timeout", "join-blocked-observed", "ESRCH", "other+errno",
)


def unobservable_value(cause: str) -> str:
    """编码 ``unobservable(cause=<cause>)`` 冻结字面。

    cause 无封闭枚举域（规格 :717），此处只做 marker 安全字面校验
    （非空、无 ``|``/``=``/``)``、无首尾空白）；域内闭合校验由各值域常量承担。
    """
    if (
        not isinstance(cause, str) or not cause
        or any(ch in cause for ch in "|=)[]") or cause != cause.strip()
    ):
        raise ValueError("illegal unobservable cause literal: %r" % (cause,))
    return "unobservable(cause=%s)" % cause


#: ``dw_return_class`` 20 值域 = 13 类 + 7 个收口编码（规格 :866）。
DW_RETURN_CLASS_20: Tuple[str, ...] = DW_RETURN_CLASS_13 + tuple(
    unobservable_value(c) for c in DW_UNOBSERVABLE_CAUSES_CLASS
)
#: ``dw_join_result`` 10 值域 = 5 本体值 + 5 个收口编码（规格 :870）。
DW_JOIN_RESULT_10: Tuple[str, ...] = DW_JOIN_TERMINAL_VALUES + tuple(
    unobservable_value(c) for c in DW_UNOBSERVABLE_CAUSES_JOIN
)

_UINT_RE = re.compile(r"\A[0-9]+\Z")
_SHA256_RE = re.compile(r"\A[0-9a-f]{64}\Z")


def _parse_uint(text: str) -> Optional[int]:
    """十进制无符号整数字面解析（负号/空白/非数字一律拒绝）。"""
    if isinstance(text, str) and _UINT_RE.match(text):
        return int(text)
    return None


# ---------------------------------------------------------------------------
# 1) hilog 行关联与 marker 解析（规格 :415-431、:1095-1097）
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class HilogLine:
    """单行 hilog 的结构化拆解（与 bundle 无关的结构层）。"""

    line_no: int                 # 1 起行号（输入序列内）
    raw: str                     # 原始行（去行尾换行）
    tag_field: str               # tag 字段整体（可能含 `<path>/<channel>` 形态）
    tag_path: str                # tag 路径组件（关联规则唯一扫描对象，规格 :417）
    channel: Optional[str]       # tag 通道（`/` 之后；如 N1BDiscVpn；无 `/` 时为 None）
    message: str                 # 首个 `: ` 之后的消息体


@dataclass(frozen=True)
class MarkerEvent:
    """结构化 marker 事件（name + kv + 原始行 + 行号）。"""

    line_no: int
    raw: str
    name: str                    # N1BDISC_<SHORT> 字面
    kv: Mapping[str, str]        # `k=v` 段（同 key 重复时首到者优先，key 入 duplicate_keys）
    tag_form: str                # entry | truncated | complete（规格 :417 三形态）
    tag_path: str
    channel: Optional[str]
    malformed_segments: Tuple[str, ...] = ()   # 缺 `=` 或空 key 的段（逐字保留）
    duplicate_keys: Tuple[str, ...] = ()

    @property
    def known(self) -> bool:
        """是否属 56 冻结 marker 字面集（规格 :1095-1097）。"""
        return self.name in FROZEN_MARKERS


def parse_hilog_line(line: str, line_no: int = 0) -> Optional[HilogLine]:
    """把一行 hilog 文本拆为 :class:`HilogLine`；无法定位 tag 字段时返回 ``None``。

    拆解规则：首个 ``": "``（冒号+空格）为 tag 字段与消息体分界——hilog 时间戳内
    冒号后紧跟数字、tag 路径组件内 ``:vpn`` 冒号后非空格，均不会提前命中；
    tag 字段即分界前的最后一个空白分隔 token。tag 字段内 ``/`` 之后为 tag 通道
    （E3 0002 实测形态 ``<path>/<TAG>``，见规格 :417 所引修复口径）。
    本函数不做 bundle 关联、不过滤 pid（规格 :417"不按 pid 过滤"）。
    """
    if not isinstance(line, str):
        return None
    text = line.rstrip("\r\n")
    sep = text.find(": ")
    if sep < 0:
        return None
    prefix = text[:sep]
    if not prefix or prefix[-1].isspace():
        return None
    match = re.search(r"(\S+)\Z", prefix)
    if match is None:
        return None
    tag_field = match.group(1)
    if "/" in tag_field:
        tag_path, _, channel = tag_field.rpartition("/")
    else:
        tag_path, channel = tag_field, None
    return HilogLine(
        line_no=line_no,
        raw=text,
        tag_field=tag_field,
        tag_path=tag_path,
        channel=channel,
        message=text[sep + 2:],
    )


def bundle_tag_forms(bundle: str = DEFAULT_BUNDLE) -> Tuple[str, ...]:
    """返回 `<bundle>` 进程 tag 的三形态字面（entry / `:vpn` 截断 / `:vpn` 完整）。

    截断形态 = hilog 丢弃 ``cn.`` 前缀（规格 :417；E3 0002 实测
    ``cn.alfadb.netbird.e3physvpna`` → ``.alfadb.netbird.e3physvpna:vpn``）。
    bundle 不以 ``cn.`` 开头时无截断形态（仅两形态）。
    """
    forms = [bundle, bundle + ":vpn"]
    if bundle.startswith("cn."):
        forms.insert(1, "." + bundle[len("cn."):] + ":vpn")
    return tuple(forms)


def tag_form_of(tag_path: str, bundle: str = DEFAULT_BUNDLE) -> Optional[str]:
    """tag 路径组件是否命中三形态之一；命中返回形态名，否则 ``None``。

    只比较 tag 路径组件、不扫描消息体、不涉及 pid（规格 :417 修复口径）。
    """
    if not isinstance(tag_path, str):
        return None
    if tag_path == bundle:
        return "entry"
    if tag_path == bundle + ":vpn":
        return "complete"
    if bundle.startswith("cn.") and tag_path == "." + bundle[len("cn."):] + ":vpn":
        return "truncated"
    return None


def parse_marker_message(message: str) -> Tuple[str, Mapping[str, str], Tuple[str, ...], Tuple[str, ...]]:
    """解析 ``N1BDISC_<NAME>|k=v|...`` 消息体。

    返回 ``(name, kv, malformed_segments, duplicate_keys)``；kv 值保留原始字符串
    （可含空格/UTF-8/`|` 之外的任意字符）。
    """
    if not isinstance(message, str) or not message.startswith(MARKER_PREFIX):
        raise ValueError("not an N1BDISC_ marker message: %r" % (message,))
    parts = message.split("|")
    name = parts[0]
    kv: dict = {}
    malformed: List[str] = []
    duplicate: List[str] = []
    for seg in parts[1:]:
        key, eq, value = seg.partition("=")
        if not eq or not key:
            malformed.append(seg)
            continue
        if key in kv:            # 首到者优先，与 chunk 重复片同法理
            duplicate.append(key)
            continue
        kv[key] = value
    return name, kv, tuple(malformed), tuple(duplicate)


def scan_markers(lines: Iterable[str], bundle: str = DEFAULT_BUNDLE) -> List[MarkerEvent]:
    """hilog 文本行流 → 结构化 marker 事件序列。

    关联规则（规格 :417-418 冻结）：tag 路径组件命中 :func:`tag_form_of` 三形态之一
    且消息体以 ``N1BDISC_`` 开头的行产出事件；不按 pid 过滤；tag 路径组件之外的
    内容（含消息体里的 bundle 名与 marker 字面）一律不参与关联。
    """
    events: List[MarkerEvent] = []
    for line_no, line in enumerate(lines, 1):
        hl = parse_hilog_line(line, line_no)
        if hl is None:
            continue
        form = tag_form_of(hl.tag_path, bundle)
        if form is None:
            continue
        if not hl.message.startswith(MARKER_PREFIX):
            continue
        name, kv, malformed, duplicate = parse_marker_message(hl.message)
        events.append(MarkerEvent(
            line_no=line_no, raw=hl.raw, name=name, kv=kv, tag_form=form,
            tag_path=hl.tag_path, channel=hl.channel,
            malformed_segments=malformed, duplicate_keys=duplicate,
        ))
    return events


# ---------------------------------------------------------------------------
# 2) N1BDISC_CHUNK 分段重组（规格 :419-431）
# ---------------------------------------------------------------------------


def validate_chunk_stream_item(stream: str, item: int) -> None:
    """校验 ``(stream, item)`` 逻辑键域；域外抛 :class:`ValueError`（规格 :420-421）。"""
    if stream not in CHUNK_STREAMS:
        raise ValueError("chunk stream out of frozen domain: %r" % (stream,))
    if stream in CHUNK_ITEM_ZERO_STREAMS:
        if item != 0:
            raise ValueError("stream %r requires item=0, got %r" % (stream, item))
    elif item not in REJTEXT_ITEM_DOMAIN:
        raise ValueError("rejtext item out of matrix-id domain {0..4}: %r" % (item,))


def encode_chunks(text: str, stream: str, item: int) -> List[Mapping[str, str]]:
    """按冻结编码规则（规格 :427-429）把 detail 文本切片为 chunk kv 字典序列。

    切片按 UTF-8 字节（≤ :data:`CHUNK_SLICE_MAX_BYTES`/片）、payload = RFC 4648
    标准 base64（含 `=` 填充）、``sha256`` 覆盖切片前原始 UTF-8 字节全体。
    供测试与后续探针侧模拟使用；空文本产出恰一片 ``payload=""``。
    """
    validate_chunk_stream_item(stream, item)
    raw = text.encode("utf-8")
    slices = [raw[i:i + CHUNK_SLICE_MAX_BYTES]
              for i in range(0, len(raw), CHUNK_SLICE_MAX_BYTES)] or [b""]
    digest = hashlib.sha256(raw).hexdigest()
    return [
        {
            "stream": stream,
            "item": str(item),
            "index": str(index),
            "count": str(len(slices)),
            "sha256": digest,
            "payload": base64.b64encode(piece).decode("ascii"),
        }
        for index, piece in enumerate(slices)
    ]


@dataclass(frozen=True)
class ChunkPiece:
    """一片已通过字面结构校验的 chunk（数值域校验后）。"""

    stream: str
    item: int
    index: int
    count: int
    sha256: str
    payload: str
    line_no: int = 0
    raw: str = ""


@dataclass(frozen=True)
class ChunkFailure:
    """一个 ``(stream, item)`` 组的重组失败记录（raw 逐字入档，规格 :422/:431）。"""

    stream: Optional[str]
    item: Optional[int]
    reason: str                  # coverage-gap | duplicate-count-mismatch | ...
    raw_lines: Tuple[str, ...] = ()
    detail: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class ChunkReassembly:
    """重组产出：``(stream, item)`` → 文本 映射 + 失败列表 + 观察项。"""

    texts: Mapping[Tuple[str, int], str]
    failures: Tuple[ChunkFailure, ...]
    #: 观察项（规格 :423 逐字登记 ``duplicate_chunk_observed``，不改判定/verdict）。
    observations: Tuple[Mapping[str, Any], ...] = ()

    @property
    def ok(self) -> bool:
        return not self.failures


def _kv_view(obj: Any) -> Tuple[Optional[Mapping[str, str]], int, str, Optional[str]]:
    """把 MarkerEvent / kv Mapping / marker 消息字符串统一为 (kv, line_no, raw, name)。"""
    if isinstance(obj, Mapping):
        return obj, 0, "", None
    kv = getattr(obj, "kv", None)
    if isinstance(kv, Mapping):
        name = getattr(obj, "name", None)
        if name is not None and not str(name).startswith(MARKER_PREFIX):
            return None, getattr(obj, "line_no", 0), getattr(obj, "raw", ""), name
        return kv, getattr(obj, "line_no", 0), getattr(obj, "raw", ""), name
    if isinstance(obj, str):
        text = obj.strip()
        if not text.startswith(MARKER_PREFIX):
            return None, 0, obj, None
        name, parsed, _, _ = parse_marker_message(text)
        return parsed, 0, text, name
    return None, 0, "", None


def _chunk_piece_from(obj: Any) -> Tuple[Optional[ChunkPiece], Optional[ChunkFailure]]:
    kv, line_no, raw, name = _kv_view(obj)
    if kv is None:
        return None, ChunkFailure(None, None, "not-chunk-marker",
                                  (raw,) if raw else (), {"name": name})

    def fail(reason: str, stream=None, item=None, **detail: Any) -> ChunkFailure:
        return ChunkFailure(stream, item, reason, (raw,) if raw else (), detail)

    for key in ("stream", "item", "index", "count", "sha256", "payload"):
        if key not in kv:
            return None, fail("malformed-field", **{"missing_field": key})
    stream = kv["stream"]
    item = _parse_uint(kv["item"])
    index = _parse_uint(kv["index"])
    count = _parse_uint(kv["count"])
    sha256 = kv["sha256"]
    if stream not in CHUNK_STREAMS:
        return None, fail("stream-out-of-domain", stream=stream, item=item,
                          value=stream)
    if item is None or index is None or count is None:
        return None, fail("malformed-field", stream=stream,
                          detail_note="item/index/count 必须为十进制非负整数字面")
    if count == 0:
        return None, fail("malformed-field", stream=stream, item=item,
                          detail_note="count=0（detail 记录至少一片）")
    if not isinstance(sha256, str) or not _SHA256_RE.match(sha256):
        return None, fail("malformed-field", stream=stream, item=item,
                          detail_note="sha256 必须为 64 位小写 hex")
    try:
        validate_chunk_stream_item(stream, item)
    except ValueError as exc:
        return None, fail("item-domain", stream=stream, item=item, detail_note=str(exc))
    return ChunkPiece(stream, item, index, count, sha256, kv["payload"],
                      line_no=line_no, raw=raw), None


def _b64decode(payload: str) -> bytes:
    """RFC 4648 标准 base64 严格解码（规格 :428；非法字符/填充错误抛异常）。"""
    return base64.b64decode(payload.encode("ascii"), validate=True)


def reassemble_chunks(pieces: Iterable[Any]) -> ChunkReassembly:
    """chunk 片流（``N1BDISC_CHUNK`` kv / MarkerEvent / :class:`ChunkPiece` / 消息字符串）→ 重组结果。

    冻结校验序（规格 :430(d) ①→④ + r4 S6 / r5 U12）：
    ① 按 ``(stream, item)`` 分组；同组同 index 重复片首到者优先，count/sha256 字面
    不同直接 fail（r5 U12），payload 解码后逐字节不一致 fail，一致则登记
    ``duplicate_chunk_observed`` 观察项不改判定；同组任一片 count/sha256 字面与
    组首到值不同亦按同法理 fail（同组不可能有两种切片总量/摘要——r5 U12 推广，
    见交付偏差注）；index 集合须覆盖 ``{0..count-1}`` 无缺口；
    ② index 升序拼接 payload → base64 严格解码；③ sha256 比对；④ UTF-8 严格解码。
    任一步失败 → 该组进入 ``failures``（reason + raw 逐字入档），不出现在 texts。
    """
    groups: dict = {}
    failures: List[ChunkFailure] = []
    observations: List[Mapping[str, Any]] = []
    for obj in pieces:
        if isinstance(obj, ChunkPiece):
            piece, failure = obj, None
        else:
            piece, failure = _chunk_piece_from(obj)
        if failure is not None:
            failures.append(failure)
            continue
        key = (piece.stream, piece.item)
        group = groups.setdefault(key, {
            "count": None, "sha256": None, "first": {}, "raws": [],
            "failed": False,
        })
        raw_line = piece.raw
        if raw_line and (not group["raws"] or group["raws"][-1] != raw_line):
            group["raws"].append(raw_line)
        first = group["first"].get(piece.index)
        if first is not None:
            # r5 U12：同 index 重复片 count/sha256 字面校验（先于 payload 比对）
            if piece.count != first.count:
                failure = ChunkFailure(key[0], key[1], "duplicate-count-mismatch",
                                       tuple(group["raws"]),
                                       {"index": piece.index,
                                        "first": first.count, "repeat": piece.count})
            elif piece.sha256 != first.sha256:
                failure = ChunkFailure(key[0], key[1], "duplicate-sha256-mismatch",
                                       tuple(group["raws"]),
                                       {"index": piece.index})
            else:
                try:
                    same = _b64decode(first.payload) == _b64decode(piece.payload)
                except (binascii.Error, ValueError, UnicodeEncodeError):
                    same = False
                if same:
                    observations.append({
                        "type": "duplicate_chunk_observed",   # 规格 :423 逐字字面
                        "stream": piece.stream, "item": piece.item,
                        "index": piece.index,
                    })
                    continue
                failure = ChunkFailure(key[0], key[1], "duplicate-payload-mismatch",
                                       tuple(group["raws"]), {"index": piece.index})
            group["failed"] = True
            failures.append(failure)
            continue
        # 不同 index 的片：count/sha256 字面须与组内首到值一致（fail-closed 推广）
        if group["count"] is None:
            group["count"], group["sha256"] = piece.count, piece.sha256
        elif piece.count != group["count"]:
            group["failed"] = True
            failures.append(ChunkFailure(
                key[0], key[1], "count-literal-conflict", tuple(group["raws"]),
                {"index": piece.index, "first": group["count"], "repeat": piece.count}))
            continue
        elif piece.sha256 != group["sha256"]:
            group["failed"] = True
            failures.append(ChunkFailure(
                key[0], key[1], "sha256-literal-conflict", tuple(group["raws"]),
                {"index": piece.index}))
            continue
        group["first"][piece.index] = piece

    texts: dict = {}
    for key, group in groups.items():
        if group["failed"] or group["count"] is None:
            continue
        stream, item = key
        count, sha256 = group["count"], group["sha256"]
        indexes = set(group["first"])
        if indexes != set(range(count)):
            failures.append(ChunkFailure(
                stream, item, "coverage-gap", tuple(group["raws"]),
                {"expected": count, "observed": sorted(indexes)}))
            continue
        try:
            raw = b"".join(_b64decode(group["first"][i].payload)
                           for i in range(count))
        except (binascii.Error, ValueError, UnicodeEncodeError):
            failures.append(ChunkFailure(stream, item, "base64-decode",
                                         tuple(group["raws"])))
            continue
        if hashlib.sha256(raw).hexdigest() != sha256:
            failures.append(ChunkFailure(stream, item, "sha256-mismatch",
                                         tuple(group["raws"])))
            continue
        try:
            texts[key] = raw.decode("utf-8")   # 规格 :431 ④ 严格 UTF-8
        except UnicodeDecodeError as exc:
            failures.append(ChunkFailure(stream, item, "utf8-decode",
                                         tuple(group["raws"]), {"error": str(exc)}))
            continue
    return ChunkReassembly(texts, tuple(failures), tuple(observations))


# ---------------------------------------------------------------------------
# 3) N1BDISC_FD ledger 重建（规格 :366-407、:829-830）
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class FdTransition:
    """一条已通过字段/域校验的 ``N1BDISC_FD`` transition marker。"""

    action: str                  # create | close | not-created
    role: str
    inst: int
    fd: Optional[int]            # not-created 恒 None
    at_mono_ms: int
    by: str
    cause: str                   # not-created 为分支字面，create/close 恒 "none"
    line_no: int = 0
    raw: str = ""


@dataclass(frozen=True)
class FdLedgerEntry:
    """canonical ledger 条目（字段序即序列化序，规格 :379(a)）。"""

    role: str
    inst: int
    fd: Optional[int]
    created_at_mono_ms: Optional[int]
    closed_by: str
    closed_at_mono_ms: Optional[int]
    cause: Optional[str]         # not-created 条目为冻结分支字面，其余 "none"
    not_created: bool = False
    #: not-created 条目的登记时刻（排序键，规格 :385(c)"按其登记时刻参与排序"）。
    not_created_sort_at: Optional[int] = None

    @property
    def role_key(self) -> str:
        """canonical 行 role 字段的 ``role#k`` 形态（规格 :370）。"""
        return "%s#%d" % (self.role, self.inst)


@dataclass(frozen=True)
class LedgerFailure:
    """重建完整性失败记录（规格 :406(c) 状态机 fail 面 + 字段/域校验 fail 面）。"""

    reason: str
    line_no: int = 0
    raw: str = ""
    role: Optional[str] = None
    inst: Optional[int] = None
    detail: Mapping[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class LedgerRebuild:
    """ledger 重建产出（canonical 行 + digest = SHA-256 64hex，规格 :386(e)）。"""

    cut: str
    ok: bool
    entries: Tuple[FdLedgerEntry, ...]
    canonical_text: Optional[str]      # 失败时为 None（fail-closed，不得参比）
    digest: Optional[str]
    failures: Tuple[LedgerFailure, ...]
    #: 规格 :387 r4 R1：pre-only 收口重建逐字标注。
    rebuilt_from_transition_marker: bool


def _fd_transition_from(obj: Any) -> Tuple[Optional[FdTransition], Optional[LedgerFailure]]:
    if isinstance(obj, FdTransition):
        return obj, None
    kv, line_no, raw, name = _kv_view(obj)
    if kv is None:
        return None, LedgerFailure("not-fd-marker", line_no, raw, detail={"name": name})

    def fail(reason: str, role=None, inst=None, **detail: Any) -> LedgerFailure:
        return LedgerFailure(reason, line_no, raw, role, inst, detail)

    missing = [k for k in ("fd", "role", "inst", "action", "at_mono_ms", "by", "cause")
               if k not in kv]
    if missing:
        return None, fail("missing-field", missing_fields=tuple(missing))

    role_raw = kv["role"]
    role_base, hash_sep, role_inst = role_raw.partition("#")
    if role_base not in FD_ROLES:
        return None, fail("role-out-of-domain", role=role_raw, value=role_base)
    inst = _parse_uint(kv["inst"])
    if inst is None or inst < 1:
        return None, fail("inst-domain", role=role_base, inst=kv["inst"],
                          detail_note="inst 十进制自 1 递增")
    if hash_sep:
        role_inst_num = _parse_uint(role_inst)
        if role_inst_num != inst:
            return None, fail("role-inst-mismatch", role=role_base, inst=inst,
                              role_suffix=role_inst)
    action = kv["action"]
    if action not in FD_ACTIONS:
        return None, fail("action-out-of-domain", role=role_base, inst=inst,
                          value=action)
    at = _parse_uint(kv["at_mono_ms"])
    if at is None:
        return None, fail("malformed-at-mono-ms", role=role_base, inst=inst,
                          value=kv["at_mono_ms"])
    # 单调钟读数绝对域门 ≥0（规格 :829 BL-4）
    by = kv["by"]
    cause = kv["cause"]
    fd_raw = kv["fd"]
    if action == "not-created":
        if fd_raw != "none":
            return None, fail("fd-must-be-none", role=role_base, inst=inst,
                              value=fd_raw)
        if by != "none":
            return None, fail("by-expected-none", role=role_base, inst=inst,
                              value=by)
        if cause in ("", "none"):
            return None, fail("not-created-missing-cause", role=role_base,
                              inst=inst, value=cause)
        return FdTransition(action, role_base, inst, None, at, by, cause,
                            line_no=line_no, raw=raw), None
    # create / close：fd 为实际 fd 号（规格 :371/:393-394）
    fd = _parse_uint(fd_raw)
    if fd is None:
        return None, fail("fd-required", role=role_base, inst=inst, value=fd_raw)
    if cause != "none":
        return None, fail("cause-expected-none", role=role_base, inst=inst,
                          value=cause)
    if action == "close" and by not in CLOSED_BY_DOMAIN:
        # by= 域校验仅施于 action=close（规格 :374 r5 U9 拆分）
        return None, fail("by-out-of-domain", role=role_base, inst=inst, value=by)
    return FdTransition(action, role_base, inst, fd, at, by, cause,
                        line_no=line_no, raw=raw), None


def serialize_ledger(entries: Iterable[FdLedgerEntry]) -> str:
    """canonical 序列化（规格 :378-386 逐字冻结）。

    每条目一行 ``role|fd|created_at_mono_ms|closed_by|closed_at_mono_ms|cause``，
    role 写 ``role#k`` 形态；未创建条目 fd/created_at/closed_at/closed_by 写 ``none``；
    行序 = 创建时刻升序（not-created 按登记时刻），时刻相同按角色集出现顺序、
    再按 inst 升序（同角色 tie-break 为规格未规定项的确定性扩展，见交付注）；
    行间 ``\\n`` 连接、无尾随换行、UTF-8 无 BOM。
    """
    def sort_key(entry: FdLedgerEntry):
        moment = (entry.created_at_mono_ms if not entry.not_created
                  else entry.not_created_sort_at)
        return (moment, FD_ROLE_ORDER[entry.role], entry.inst)

    rows = []
    for entry in sorted(entries, key=sort_key):
        rows.append("|".join((
            entry.role_key,
            "none" if entry.fd is None else str(entry.fd),
            "none" if entry.created_at_mono_ms is None else str(entry.created_at_mono_ms),
            entry.closed_by,
            "none" if entry.closed_at_mono_ms is None else str(entry.closed_at_mono_ms),
            entry.cause or "none",
        )))
    return "\n".join(rows)


def ledger_digest(entries: Iterable[FdLedgerEntry]) -> str:
    """digest = canonical 序列化字节串（UTF-8）的 SHA-256 完整 64 hex（规格 :386(e)）。"""
    return hashlib.sha256(serialize_ledger(entries).encode("utf-8")).hexdigest()


def rebuild_fd_ledger(transitions: Iterable[Any], cut: str) -> LedgerRebuild:
    """``N1BDISC_FD`` transition marker 流 → canonical ledger + digest。

    ``cut``（求值切点）∈ :data:`LEDGER_CUTS`：complete/pre-only/pre-snapshot/
    host-forcestop；仍 open 实例按 :data:`CUT_OPEN_CLOSED_BY` 登记 ``closed_by``、
    ``closed_at=none``（规格 :381-383、:400-404）。``cut=pre-only`` 时结果逐字标注
    :data:`REBUILT_FROM_TRANSITION_MARKER`（规格 :387 r4 R1）。

    ``(role, inst)`` 配对状态机（规格 :406(c)，每实例独立）：uncreated→open（create）、
    open→closed（首条 close）、uncreated→not-created（终态）；close 无对应 open、
    closed/not-created 后再收任何 create/close、或任何其他转移 → 完整性失败；
    另按规格 :830 施行 ``created_at ≤ closed_at`` 顺序校验。
    任一失败 → ``ok=False`` 且 ``digest=None``（fail-closed，digest 不得参比），
    条目仍尽力给出供入档。接受 MarkerEvent / kv Mapping / FdTransition / 消息字符串。
    """
    if cut not in LEDGER_CUTS:
        raise ValueError("unknown ledger cut point: %r" % (cut,))
    states: dict = {}
    info: dict = {}
    not_created: List[FdTransition] = []
    failures: List[LedgerFailure] = []
    for obj in transitions:
        transition, failure = _fd_transition_from(obj)
        if failure is not None:
            failures.append(failure)
            continue
        assert transition is not None
        key = (transition.role, transition.inst)
        state = states.get(key, "uncreated")
        if transition.action == "create":
            if state != "uncreated":
                failures.append(LedgerFailure(
                    "state-conflict", transition.line_no, transition.raw,
                    transition.role, transition.inst,
                    {"state": state, "action": transition.action}))
                continue
            states[key] = "open"
            info[key] = {"fd": transition.fd, "created_at": transition.at_mono_ms}
        elif transition.action == "close":
            if state == "uncreated":
                failures.append(LedgerFailure(
                    "close-before-create", transition.line_no, transition.raw,
                    transition.role, transition.inst))
                continue
            if state != "open":
                failures.append(LedgerFailure(
                    "state-conflict", transition.line_no, transition.raw,
                    transition.role, transition.inst,
                    {"state": state, "action": "close"}))
                continue
            created_at = info[key]["created_at"]
            if transition.at_mono_ms < created_at:   # 规格 :830 顺序约束
                failures.append(LedgerFailure(
                    "closed-before-created", transition.line_no, transition.raw,
                    transition.role, transition.inst,
                    {"created_at": created_at, "closed_at": transition.at_mono_ms}))
                continue
            states[key] = "closed"
            info[key]["closed_by"] = transition.by
            info[key]["closed_at"] = transition.at_mono_ms
        else:  # not-created
            if state != "uncreated":
                failures.append(LedgerFailure(
                    "state-conflict", transition.line_no, transition.raw,
                    transition.role, transition.inst,
                    {"state": state, "action": "not-created"}))
                continue
            states[key] = "not-created"
            not_created.append(transition)

    entries: List[FdLedgerEntry] = []
    for key, state in states.items():
        role, inst = key
        if state == "not-created":
            continue
        record = info[key]
        if state == "open":
            closed_by, closed_at = CUT_OPEN_CLOSED_BY[cut], None
        else:
            closed_by, closed_at = record["closed_by"], record["closed_at"]
        entries.append(FdLedgerEntry(role, inst, record["fd"],
                                     record["created_at"], closed_by, closed_at,
                                     "none", not_created=False))
    for transition in not_created:
        entries.append(FdLedgerEntry(transition.role, transition.inst, None, None,
                                     "none", None, transition.cause, not_created=True,
                                     not_created_sort_at=transition.at_mono_ms))

    ok = not failures
    canonical = serialize_ledger(entries) if ok else None
    return LedgerRebuild(
        cut=cut,
        ok=ok,
        entries=tuple(entries),
        canonical_text=canonical,
        digest=hashlib.sha256(canonical.encode("utf-8")).hexdigest() if ok else None,
        failures=tuple(failures),
        rebuilt_from_transition_marker=(cut == "pre-only"),   # 规格 :387 r4 R1
    )


# ---------------------------------------------------------------------------
# 4) dw_return_class 派生（规格 :722-823）+ dw_join_result 基础映射（规格 :870）
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class DwReturnInput:
    """``dw_return_class`` 判定输入（marker 流中各输入的存在性为布尔承载）。"""

    ret: int
    revents: int                 # 十进制 bitmask 单值（规格 :813）
    errno: Optional[int] = None
    at_mono_ms: Optional[int] = None
    elapsed_ms: Optional[int] = None
    drain_end: Optional[str] = None          # eagain | zero-read | errno-<n> | box-expiry
    has_skip_destroy: bool = False  # ``N1BDISC_SKIP|item=destroy`` 存在性
    has_destroy_c: bool = False     # ``N1BDISC_DW_DESTROY_C`` 存在性
    t_mono_ms: Optional[int] = None  # _T 的 mono_ms
    c_mono_ms: Optional[int] = None  # _C 的 mono_ms


@dataclass(frozen=True)
class DwReturnClassResult:
    """派生结果：``outcome="class"`` 时 ``value`` 为类字面；``outcome="fail"`` 时
    ``fail_reason`` 给矛盾输入类别（verdict fail + 原文逐字入档由调用方执行）。"""

    outcome: str                 # "class" | "fail"
    value: Optional[str] = None
    fail_reason: Optional[str] = None
    detail: Mapping[str, Any] = field(default_factory=dict)


def derive_dw_return_class(inp: DwReturnInput) -> DwReturnClassResult:
    """四步有序派生（r15 派生序冻结，规格 :726）：

    合法域门 → 0/0b 前置检查 → 未知位门 → 普通行 1-11（首个命中即终止）。
    合法域门矛盾输入（ret∉{-1,0,1}、ret=0 带 revents、ret=1 空 revents、ret=-1 无
    errno）→ ``outcome="fail"``，不进判定表（规格 :724-725）；单调钟绝对域门
    （读数/派生时长 ≥0，规格 :829 BL-4）先于真值表执行。未知位门只施于 ``ret≥0``
    （规格 :819），仅 0/0b 未命中的输入到达（规格 :815 r16）。
    """
    if not isinstance(inp.ret, int) or isinstance(inp.ret, bool):
        raise TypeError("ret must be int")
    if not isinstance(inp.revents, int) or isinstance(inp.revents, bool):
        raise TypeError("revents must be int")
    if inp.revents < 0:
        raise ValueError("revents 编码域 = 全部非负整数（规格 :814 r10）")

    def fail(reason: str, **detail: Any) -> DwReturnClassResult:
        return DwReturnClassResult("fail", None, reason, detail)

    # BL-4 单调钟绝对域门（规格 :824-829，先于各字段真值表执行）
    for name in ("at_mono_ms", "elapsed_ms", "t_mono_ms", "c_mono_ms"):
        value = getattr(inp, name)
        if value is not None and value < 0:
            return fail("negative-monotonic", field=name, value=value)

    # 第 1 步：合法域门（规格 :724-725）
    if inp.ret not in (-1, 0, 1):
        return fail("ret-out-of-domain", ret=inp.ret)
    if inp.ret == 0 and inp.revents != 0:
        return fail("ret-zero-with-revents", ret=0, revents=inp.revents)
    if inp.ret == 1 and inp.revents == 0:
        return fail("ret-one-empty-revents", ret=1, revents=inp.revents)
    if inp.ret == -1 and inp.errno is None:
        return fail("ret-minus-one-missing-errno", ret=-1)

    # 第 2 步：0/0b 前置检查（规格 :730；r17 (c)：SKIP 支不要求 RETURN）
    if inp.has_skip_destroy:
        return DwReturnClassResult("class", "destroy-skip-proven",
                                   detail={"row": 0})
    if not inp.has_destroy_c:
        return DwReturnClassResult("class", "destroy-call-unobserved",
                                   detail={"row": "0b"})

    # 第 3 步：未知位门（规格 :815-819，仅 ret≥0 且 0/0b 未命中后执行）
    if inp.ret >= 0 and (inp.revents & ~KNOWN_REVENTS_MASK):
        return DwReturnClassResult(
            "class", "other-revents",
            detail={"gate": "unknown-bit", "unknown_bit_revents": inp.revents})

    # 第 4 步：普通判定表行 1-11（规格 :796-806，首个命中即终止）
    at, elapsed = inp.at_mono_ms, inp.elapsed_ms
    band_pre = at is not None and inp.t_mono_ms is not None and at < inp.t_mono_ms
    band_post = at is not None and inp.c_mono_ms is not None and at > inp.c_mono_ms
    hup_err = bool(inp.revents & (POLLHUP | POLLERR))
    has_in = bool(inp.revents & POLLIN)

    if inp.ret == -1:                                    # 行 1/2（revents 无意义）
        if inp.errno == EINTR:
            return DwReturnClassResult("class", "interrupted", detail={"errno": inp.errno})
        return DwReturnClassResult("class", "poll-error", detail={"errno": inp.errno})
    if inp.revents & POLLNVAL:                           # 行 3（先于行 4，:810）
        return DwReturnClassResult("class", "fd-invalid",
                                   detail={"revents": inp.revents})
    if inp.revents != 0 and band_pre:                    # 行 4（at 严格 < T）
        return DwReturnClassResult("class", "pre-destroy-ready", detail={"at_mono_ms": at})
    if hup_err and elapsed is not None and elapsed >= DW_EAGAIN_THRESHOLD_MS and band_post:
        return DwReturnClassResult("class", "late-fd-event", detail={"elapsed_ms": elapsed})
    if has_in and elapsed is not None and elapsed >= DW_EAGAIN_THRESHOLD_MS:
        return DwReturnClassResult("class", "late-data", detail={"elapsed_ms": elapsed})
    if (hup_err and elapsed is not None and elapsed < DW_EAGAIN_THRESHOLD_MS
            and band_post and inp.drain_end == "eagain"):
        return DwReturnClassResult("class", "fd-event-like", detail={"elapsed_ms": elapsed})
    if (has_in and not hup_err and elapsed is not None
            and elapsed < DW_EAGAIN_THRESHOLD_MS and band_post
            and inp.drain_end == "eagain"):
        return DwReturnClassResult("class", "data-ready-post-destroy",
                                   detail={"elapsed_ms": elapsed})
    if inp.revents == 0 and elapsed is not None and elapsed >= DW_EAGAIN_THRESHOLD_MS:
        return DwReturnClassResult("class", "timeout-like", detail={"elapsed_ms": elapsed})
    if inp.revents == 0 and elapsed is not None and elapsed < DW_EAGAIN_THRESHOLD_MS:
        return DwReturnClassResult("class", "spurious-early", detail={"elapsed_ms": elapsed})
    if inp.revents != 0:                                 # 行 11（掩码全集内穷尽兜底）
        return DwReturnClassResult("class", "other-revents",
                                   detail={"revents": inp.revents})
    return fail("truth-table-incomplete", ret=inp.ret, revents=inp.revents,
                detail_note="真值表未覆盖（如 ret=0 且 elapsed_ms 缺失）")


@dataclass(frozen=True)
class DwJoinInput:
    """``dw_join_result`` 完整重建的判定输入（runner 侧 capture + runner 登记）。

    - ``dw_skip_cause``：``N1BDISC_SKIP|item=D-W`` 的 cause（``no-live-fd`` /
      ``dup-failed``）——skip 表全量指派（规格 :997-1000、D-W 分域 (a) :734）。
    - ``destroy_call_state``：五态（pre-only 死亡收口分流输入，规格 :1171-1175）。
    - ``d6b_skip_present`` / ``join_timeout_registered``：join sticky 的两个等价
      前件（规格 :693：capture 中 ``SKIP|item=D6b`` 或 ``join-timeout-worker-
      abandoned=true`` 已登记）→ 重建 ``join-timeout``，不得由 ``DW_EXIT``/
      ``DW_RETURN`` 迟到完成改写。
    - ``join_blocked_registered``：runner 依轮询超时登记
      ``join-blocked-observed``（规格 :720/:1091——阻塞线程自身不发登记）。
    - ``exit_present``/``exit_rc``：``DW_EXIT`` 存在性只喂 watchdog ④、不喂 join
      轴的改写（:693）；``exit_rc`` 为 ``pthread_join`` 返回值（0=joined、
      ``ESRCH``、其他 errno → ``other+errno``）。
    - ``post_present``/``death_observed``：pre-only 死亡收口支只在
      「POST 缺 ∧ 死亡分量 observed-true」时求值（规格 :1168/:1171）。
    """

    dw_skip_cause: Optional[str] = None
    destroy_call_state: str = "not-reached"
    d6b_skip_present: bool = False
    join_timeout_registered: bool = False
    join_blocked_registered: bool = False
    exit_present: bool = False
    exit_rc: Optional[int] = None
    post_present: bool = False
    death_observed: bool = False


@dataclass(frozen=True)
class DwJoinResult:
    """重建结果：``outcome="value"`` 时 ``value`` ∈ :data:`DW_JOIN_RESULT_10`；
    ``outcome="fail"`` 时真值表未覆盖（F8(3) 面，调用方入档）。``sticky``
    记录 join sticky 是否命中（规格 :693 观察项）。"""

    outcome: str                 # "value" | "fail"
    value: Optional[str] = None
    fail_reason: Optional[str] = None
    sticky: Optional[str] = None
    detail: Mapping[str, Any] = field(default_factory=dict)


def derive_dw_join_result(inp: DwJoinInput) -> DwJoinResult:
    """``dw_join_result`` 10 值域完整重建（规格 :870；单一权威派生入口）。

    求值序（先到先得，命中即终止）：
    (1) skip 表指派（:997-1000/:734）——``SKIP|item=D-W`` cause ∈
        {``no-live-fd``, ``dup-failed``} → 同 cause 编码（无 worker、无 join）；
    (2) join sticky（:693，r19 runner 重建规则）——D6b skip 在（或 JT 已登记）→
        ``join-timeout``，迟到 ``DW_EXIT``/``DW_RETURN`` 存在性不得改写；
        barrier-never-observed 顺延路径同落本支（终态轮询盒先于 SKIP 到期登记，
        :1177）；
    (3) runner 登记 ``join-blocked-observed``（:720/:1029/:1047/:1091 A4 注）；
    (4) pre-only 死亡收口（POST 缺 ∧ 死亡分量 true；:1168/:1171-1175）——按
        ``destroy_call_state`` 五态分流：``not-reached`` → ``destroy-not-reached``、
        ``call-returned`` → ``post-destroy-unobservable``、
        ``call-boundary-incomplete`` → ``call-boundary-incomplete``（三 cause 同入
        join 域，规格 :870 计数 5+2+3=10）；``not-called`` 不走本收口（:1175）——
        其两条真实路径已由 (1)/(2) 承接，残余格为真值表缺口 → fail；
    (5) ``pthread_join`` 返回（runner 侧）：``ESRCH`` / 其他 errno → ``other+errno``；
    (6) ``DW_EXIT`` 在（或 ``exit_rc == 0``）→ ``joined``；
    (7) 其余输入组合真值表未覆盖 → fail（调用方按 F8(3) 入档，不伪装域内值）。
    全部 ``value`` 落点逐字 ∈ :data:`DW_JOIN_RESULT_10`。
    """
    def result(value: str, sticky: Optional[str] = None,
               **detail: Any) -> DwJoinResult:
        assert value in DW_JOIN_RESULT_10, "derive_dw_join_result 域外落值: %r" % (value,)
        return DwJoinResult("value", value, None, sticky, detail)

    # (1) skip 表指派（D-W 整体被 skip：无 waiter、无 join 等待）
    if inp.dw_skip_cause in ("no-live-fd", "dup-failed"):
        return result(unobservable_value(inp.dw_skip_cause),
                      detail={"branch": "skip-table"})
    # (2) join sticky（:693）：JT 登记 → join-timeout，迟到完成不得改写
    if inp.d6b_skip_present or inp.join_timeout_registered:
        return result("join-timeout", sticky="join",
                      detail={"branch": "join-sticky",
                              "d6b_skip": inp.d6b_skip_present,
                              "jt_registered": inp.join_timeout_registered})
    # (3) A5(b)：标志置位后 join 阻塞 → runner 依轮询超时登记（:720/:1091）
    if inp.join_blocked_registered:
        return result("join-blocked-observed",
                      detail={"branch": "join-blocked-registered"})
    # (4) pre-only 死亡收口（POST 缺 ∧ 死亡分量；:1168/:1171-1175 五态分流）
    if not inp.post_present and inp.death_observed:
        closure = {
            "not-reached": "destroy-not-reached",            # :1172/:1188 (1a)
            "call-returned": "post-destroy-unobservable",    # :1173
            "call-boundary-incomplete": "call-boundary-incomplete",  # :1174
        }
        if inp.destroy_call_state in closure:
            return result(unobservable_value(closure[inp.destroy_call_state]),
                          detail={"branch": "pre-only-death-closure",
                                  "destroy_call_state": inp.destroy_call_state})
        if inp.destroy_call_state == "not-called":
            # :1175 not-called 不走死亡收口——其真实路径（no-live-fd 的 D-W skip /
            # barrier-never-observed 的 JT 登记）已在 (1)/(2) 承接；到此的残余格
            # 为真值表缺口（fail-closed，不伪装域内值）。
            return DwJoinResult("fail", None,
                                "join-truth-table-gap: not-called without D-W skip "
                                "or JT registration", None,
                                {"branch": "pre-only-death-closure",
                                 "destroy_call_state": "not-called"})
        return DwJoinResult("fail", None,
                            "join-truth-table-gap: destroy_call_state=%r"
                            % (inp.destroy_call_state,), None,
                            {"branch": "pre-only-death-closure"})
    # (5) pthread_join 返回（runner 侧登记）
    if inp.exit_rc is not None and inp.exit_rc != 0:
        if inp.exit_rc == _ESRCH:
            return result("ESRCH", detail={"exit_rc": inp.exit_rc})
        return result("other+errno", detail={"exit_rc": inp.exit_rc})
    # (6) DW_EXIT 在（EXIT 只喂 ④、不喂 join 改写——此处为正常终态正面支）
    if inp.exit_present or inp.exit_rc == 0:
        return result("joined", detail={"branch": "exit-present"})
    # (7) 真值表未覆盖（如 POST 缺、进程活、无任何 join 输入）
    return DwJoinResult("fail", None, "join-truth-table-incomplete", None,
                        {"post_present": inp.post_present,
                         "death_observed": inp.death_observed,
                         "exit_present": inp.exit_present})


def map_dw_join_result(outcome: Any) -> str:
    """``dw_join_result`` 10 值域（规格 :870）的基础映射。

    接受：本体值字面（joined/join-timeout/join-blocked-observed/ESRCH）、
    ``("other", <errno:int>)`` → ``"other+errno"``、或收口 cause 字面
    （:data:`DW_UNOBSERVABLE_CAUSES_JOIN` 五值）→ ``unobservable(cause=<cause>)``。
    join 的实际判定（轮询超时登记、runner 重建等）由后续状态机增量执行。
    """
    if isinstance(outcome, tuple):
        if (len(outcome) == 2 and outcome[0] == "other"
                and isinstance(outcome[1], int) and not isinstance(outcome[1], bool)):
            return "other+errno"
        raise ValueError("illegal dw_join_result outcome tuple: %r" % (outcome,))
    if not isinstance(outcome, str):
        raise ValueError("illegal dw_join_result outcome: %r" % (outcome,))
    if outcome in DW_JOIN_TERMINAL_VALUES:
        return outcome
    if outcome in DW_UNOBSERVABLE_CAUSES_JOIN:
        return unobservable_value(outcome)
    raise ValueError("dw_join_result outcome out of 10-value domain: %r" % (outcome,))


__all__ = [
    # 常量
    "HILOG_TAG", "DEFAULT_BUNDLE", "MARKER_PREFIX", "FROZEN_MARKERS", "EXEMPT_MARKERS",
    "CHUNK_STREAMS", "CHUNK_ITEM_ZERO_STREAMS", "REJTEXT_ITEM_IDS", "REJTEXT_ITEM_DOMAIN",
    "CHUNK_SLICE_MAX_BYTES", "FD_ROLES", "FD_ROLE_ORDER", "FD_ACTIONS",
    "CLOSED_BY_VALUES", "CLOSED_BY_DOMAIN", "LEDGER_CUTS", "CUT_OPEN_CLOSED_BY",
    "REBUILT_FROM_TRANSITION_MARKER",
    "POLLIN", "POLLPRI", "POLLOUT", "POLLERR", "POLLHUP", "POLLNVAL",
    "KNOWN_REVENTS_MASK", "DW_EAGAIN_THRESHOLD_MS", "EINTR",
    "DW_RETURN_CLASS_13", "DW_RETURN_CLASS_20", "DW_JOIN_RESULT_10",
    "DW_JOIN_TERMINAL_VALUES", "DW_UNOBSERVABLE_CAUSES_CLASS", "DW_UNOBSERVABLE_CAUSES_JOIN",
    # 关联与 marker
    "HilogLine", "MarkerEvent", "parse_hilog_line", "bundle_tag_forms", "tag_form_of",
    "parse_marker_message", "scan_markers",
    # chunk
    "ChunkPiece", "ChunkFailure", "ChunkReassembly", "validate_chunk_stream_item",
    "encode_chunks", "reassemble_chunks",
    # fd ledger
    "FdTransition", "FdLedgerEntry", "LedgerFailure", "LedgerRebuild",
    "serialize_ledger", "ledger_digest", "rebuild_fd_ledger",
    # dw
    "DwReturnInput", "DwReturnClassResult", "derive_dw_return_class",
    "DwJoinInput", "DwJoinResult", "derive_dw_join_result",
    "unobservable_value", "map_dw_join_result",
]
