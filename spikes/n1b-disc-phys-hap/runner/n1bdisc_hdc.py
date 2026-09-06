# -*- coding: utf-8 -*-
"""n1bdisc_hdc — N1BDISC host runner HDC 白名单执行器（host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``「DISC HDC 操作白名单」（:1616-1644，逐字冻结）。

- 操作名 → 完整 argv 逐字映射表（:1624-1642）：``BundleDump``/``PidOf``/``PidOfVpn``/
  ``MkdirStaging``/``SendHap``/``InstallHap``/``StartEntry``/``HilogStream``/``FaultProbe``/
  ``FaultRecv``/``ForceStop``/``Uninstall``/``RemoveStaging``/``StagingProbe``/
  ``PidOfPost``/``PidOfVpnPost`` + gate 5 三探针复用（``Version`` = ``hdc version`` 无
  ``-t`` 前缀 / ``ParamModel`` / ``ParamSoftwareVersion``）。
- 未知操作 / 多余参数 / 缺参数 / bundle 不符 / Reason 非法 → :class:`HdcViolation`
  （规格 :1643；F7 轴 → verdict ``invalid``，F7 不在 fail 闭集内，规格 :1149）。
- 操作名大小写不敏感（:1643）。
- 执行器经 ``transport`` 注入执行（本任务不实现真实 transport；fake-hdc 即默认
  transport，见 ``fake_hdc``）。
"""

from __future__ import annotations

from typing import Any, Dict, Iterator, List, Mapping, Optional, Sequence, Tuple

import n1bdisc_core as core

#: N1B bundle 名（:1618 冻结；沿 core 同一字面）。
BUNDLE = core.DEFAULT_BUNDLE
#: staging 根（:1618 冻结）。
STAGING_ROOT = "/data/local/tmp/netbird-n1bdisc"
HAP_REMOTE = STAGING_ROOT + "/hap/n1bdisc.hap"
HAP_REMOTE_DIR = STAGING_ROOT + "/hap"
#: faultlogger 目录与 FaultProbe glob（:1634）。
FAULTLOGGER_DIR = "/data/log/faultlog/faultlogger"
FAULT_GLOB = "*cn.alfadb.netbird.n1bdisc*"
#: ForceStop Reason 域（:1636 仅限两值）。
FORCE_STOP_REASONS: Tuple[str, ...] = ("exception-cleanup", "final-cleanup")

_ARGV_T = "{T}"          # 已绑定目标句柄占位（:1622）
_ARGV_HAP = "{HAP}"      # <HAP_DISC> 占位（:1618）
_ARGV_RECV_REMOTE = "{RECV_REMOTE}"
_ARGV_RECV_HOST = "{RECV_HOST}"

_ARGV = ("-t", _ARGV_T)  # 白名单表 `-t <T>` 前缀逐字（除注明者外均含，:1622）

#: 操作名 → 完整 argv 模板（规格 :1624-1642 逐字；token 序列即真实 hdc argv）。
HDC_ARGV_TABLE: Dict[str, Tuple[str, ...]] = {
    # gate 5 三探针（:1621/:1642）
    "Version": ("version",),
    "ParamModel": _ARGV + ("shell", "param", "get", "const.product.model"),
    "ParamSoftwareVersion": _ARGV + ("shell", "param", "get",
                                     "const.product.software.version"),
    # campaign 白名单（:1626-1641）
    "BundleDump": _ARGV + ("shell", "bm", "dump", "-n", BUNDLE),
    "PidOf": _ARGV + ("shell", "pidof", BUNDLE),
    "PidOfVpn": _ARGV + ("shell", "pidof", BUNDLE + ":vpn"),
    "MkdirStaging": _ARGV + ("shell", "mkdir", "-p", HAP_REMOTE_DIR),
    "SendHap": _ARGV + ("file", "send", _ARGV_HAP, HAP_REMOTE),
    "InstallHap": _ARGV + ("shell", "bm", "install", "-p", HAP_REMOTE_DIR),
    "StartEntry": _ARGV + ("shell", "aa", "start", "-a", "EntryAbility",
                           "-b", BUNDLE, "-m", "entry"),
    "HilogStream": _ARGV + ("shell", "hilog", "-T", core.HILOG_TAG,
                            "-v", "year", "-v", "zone"),
    "FaultProbe": _ARGV + ("shell", "find", FAULTLOGGER_DIR, "-maxdepth", "1",
                           "-type", "f", "-name", FAULT_GLOB, "-print"),
    "FaultRecv": _ARGV + ("file", "recv", _ARGV_RECV_REMOTE, _ARGV_RECV_HOST),
    "ForceStop": _ARGV + ("shell", "aa", "force-stop", BUNDLE),
    "Uninstall": _ARGV + ("shell", "bm", "uninstall", "-n", BUNDLE),
    "RemoveStaging": _ARGV + ("shell", "rm", "-rf", STAGING_ROOT),
    "StagingProbe": _ARGV + ("shell", "ls", "-ld", STAGING_ROOT),
    "PidOfPost": _ARGV + ("shell", "pidof", BUNDLE),
    "PidOfVpnPost": _ARGV + ("shell", "pidof", BUNDLE + ":vpn"),
}

#: 各操作允许的参数键（表外任何键 = 多余参数 → HdcViolation，:1643）。
HDC_OP_PARAMS: Dict[str, Tuple[str, ...]] = {
    "FaultRecv": ("fault_file", "host_path"),
    "ForceStop": ("reason",),
}

#: 只许流式执行的操作（HilogStream 长驻流）。
STREAM_OPS = frozenset({"HilogStream"})

_OP_LOOKUP = {name.lower(): name for name in HDC_ARGV_TABLE}


class HdcViolation(Exception):
    """HDC 白名单违规（规格 :1643 拒绝面；F7 轴 → verdict ``invalid``，:1149/:1616）。"""

    def __init__(self, reason: str, detail: Mapping[str, Any] = None) -> None:
        super().__init__("hdc-violation: %s %s" % (reason, dict(detail or {})))
        self.reason = reason
        self.detail = dict(detail or {})


def resolve_operation(name: str) -> Optional[str]:
    """操作名解析（大小写不敏感，:1643）；未知操作返回 ``None``。"""
    if not isinstance(name, str):
        return None
    return _OP_LOOKUP.get(name.strip().lower())


def expand_template(template: Sequence[str], target: str = "",
                    hap_path: str = "") -> List[str]:
    """把冻结 argv 模板的占位符按实例绑定值展开（白名单审计与 fake 反向校验共用）。"""
    return [t.replace(_ARGV_T, target).replace(_ARGV_HAP, hap_path)
             for t in template]


def fault_file_traversal_reason(fault_file: Any) -> Optional[str]:
    """FaultRecv 远端条目名的路径穿越拦截（m-10 登记）。

    远端路径由 ``FAULTLOGGER_DIR + "/" + <条目名>`` 拼装（:1634），条目名必须为纯
    文件名（FaultProbe ``find -maxdepth 1 -type f -print`` 的 basename 形态）：
    含路径分隔符（``/``、``\\``——``../`` 穿越形态与绝对路径随之被拒）或整体为
    ``..``（父目录形态）一律拒绝。返回拒绝 reason 字面；合法名返回 ``None``。
    """
    if not isinstance(fault_file, str) or not fault_file:
        return "malformed-fault-file"
    if "/" in fault_file or "\\" in fault_file:
        # 含分隔符 = 嵌套 / ``..`` 段 / 绝对路径穿越形态（前缀拼接不得越目录）
        return "path-separator-in-fault-file"
    if fault_file == "..":
        return "traversal-fault-file"
    return None


def build_argv(op: str, params: Optional[Mapping[str, str]] = None,
               target: str = "", hap_path: str = "") -> List[str]:
    """按冻结模板构造完整 argv；任何域违反抛 :class:`HdcViolation`。

    ``params`` 只接受 :data:`HDC_OP_PARAMS` 声明的键（多余参数拒绝）；必填参数缺失
    拒绝；``ForceStop.reason`` 域校验；FaultRecv 的远端路径由命中文件名拼装且文件名
    须匹配 FaultProbe glob 形态（bundle 不符拒绝）、并经路径穿越拦截（m-10：含
    ``../`` / 绝对路径 / 路径分隔符的条目名拒绝）。
    """
    canonical = resolve_operation(op)
    if canonical is None:
        raise HdcViolation("unknown-operation", {"op": op})
    allowed = HDC_OP_PARAMS.get(canonical, ())
    given = dict(params or {})
    extra = sorted(set(given) - set(allowed))
    if extra:
        raise HdcViolation("extra-parameter", {"op": canonical, "extra": extra})
    missing = [k for k in allowed if k not in given]
    if missing:
        raise HdcViolation("missing-parameter", {"op": canonical, "missing": missing})

    remote = None
    host = None
    reason = None
    if canonical == "FaultRecv":
        fault_file = given["fault_file"]
        if not isinstance(fault_file, str) or BUNDLE not in fault_file:
            # bundle 不符（glob = *cn.alfadb.netbird.n1bdisc*，:1634/:1643）
            raise HdcViolation("bundle-mismatch", {"op": canonical, "fault_file": fault_file})
        # m-10：路径穿越拦截——含 ``../`` / 绝对路径 / 路径分隔符（及父目录形态
        # ``..``）的条目名在拼装远端路径前拒绝（白名单拒绝面，:1643）。
        traversal = fault_file_traversal_reason(fault_file)
        if traversal is not None:
            raise HdcViolation(traversal, {"op": canonical, "fault_file": fault_file})
        remote = FAULTLOGGER_DIR + "/" + fault_file
        host = given["host_path"]
    if canonical == "ForceStop":
        reason = given["reason"]
        if reason not in FORCE_STOP_REASONS:
            raise HdcViolation("illegal-reason", {"op": canonical, "reason": reason})

    argv = expand_template(HDC_ARGV_TABLE[canonical], target=target,
                           hap_path=hap_path)
    if canonical == "FaultRecv":
        argv[-2] = remote
        argv[-1] = host
    return argv


class HdcTransportResult(tuple):
    """transport 单次调用结果 ``(exit_code, stdout, stderr)``。"""

    def __new__(cls, exit_code: int, stdout: str, stderr: str) -> "HdcTransportResult":
        return super().__new__(cls, (int(exit_code), stdout, stderr))

    @property
    def exit_code(self) -> int:
        return self[0]

    @property
    def stdout(self) -> str:
        return self[1]

    @property
    def stderr(self) -> str:
        return self[2]


class HdcTransport:
    """transport 注入接口：真实 transport 本任务不实现（fake-hdc 为默认 transport）。

    ``call`` = 一次性命令；``open_stream`` = 长驻流（仅 HilogStream）。
    """

    def call(self, argv: Sequence[str]) -> HdcTransportResult:
        raise NotImplementedError(
            "真实 hdc transport 未实现（host-only；live 模式显式拒绝执行）")

    def open_stream(self, argv: Sequence[str]) -> Iterator[str]:
        raise NotImplementedError(
            "真实 hdc transport 未实现（host-only；live 模式显式拒绝执行）")


class RealHdcTransportStub(HdcTransport):
    """显式的真实 transport 占位：任何调用即拒绝（live 模式骨架用）。"""


class HdcCallRecord:
    """一次白名单调用的审计记录（HDC 命令流审计 → invalid 轴输入）。"""

    def __init__(self, op: str, argv: List[str],
                 result: Optional[HdcTransportResult] = None) -> None:
        self.op = op
        self.argv = tuple(argv)
        self.result = result

    def as_dict(self) -> Dict[str, Any]:
        return {
            "op": self.op,
            "argv": list(self.argv),
            "exit_code": None if self.result is None else self.result.exit_code,
            "stdout": None if self.result is None else self.result.stdout,
            "stderr": None if self.result is None else self.result.stderr,
        }


class HdcExecutor:
    """白名单执行器：操作名 + 参数 → 冻结 argv → transport 执行。

    白名单审计直接按 :data:`HDC_ARGV_TABLE` 执行（:1622）——任何经本执行器的调用
    argv 均由冻结模板构造，transport 侧（fake-hdc）再做逐字反向校验，双侧一致方为
    合法命令流。
    """

    def __init__(self, transport: HdcTransport, target: str,
                 hap_path: str = "") -> None:
        self.transport = transport
        self.target = target
        self.hap_path = hap_path
        self.audit: List[HdcCallRecord] = []

    def execute(self, op: str, **params: str) -> HdcCallRecord:
        """执行一次性白名单操作；违规抛 :class:`HdcViolation`（不进 transport）。"""
        argv = build_argv(op, params, target=self.target, hap_path=self.hap_path)
        canonical = resolve_operation(op)
        if canonical in STREAM_OPS:
            raise HdcViolation("op-requires-stream", {"op": canonical})
        result = self.transport.call(argv)
        record = HdcCallRecord(canonical or op, argv, result)
        self.audit.append(record)
        return record

    def open_stream(self, op: str) -> Iterator[str]:
        """打开长驻流（仅 HilogStream）；返回行迭代器。"""
        argv = build_argv(op, None, target=self.target, hap_path=self.hap_path)
        canonical = resolve_operation(op)
        if canonical not in STREAM_OPS:
            raise HdcViolation("op-not-streamable", {"op": canonical})
        self.audit.append(HdcCallRecord(canonical or op, argv, None))
        return self.transport.open_stream(argv)

    def audit_dicts(self) -> List[Dict[str, Any]]:
        return [r.as_dict() for r in self.audit]


__all__ = [
    "BUNDLE", "STAGING_ROOT", "HAP_REMOTE", "HAP_REMOTE_DIR",
    "FAULTLOGGER_DIR", "FAULT_GLOB", "FORCE_STOP_REASONS",
    "HDC_ARGV_TABLE", "HDC_OP_PARAMS", "STREAM_OPS",
    "HdcViolation", "resolve_operation", "build_argv", "expand_template",
    "fault_file_traversal_reason",
    "HdcTransportResult", "HdcTransport", "RealHdcTransportStub",
    "HdcCallRecord", "HdcExecutor",
]
