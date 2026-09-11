#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc_freeze_manifest — N1BDISC 候选 ready-freeze manifest 加载与机器验证（host-only）。

权威规格：``docs/n1b-disc-gate-plan.md``（只读冻结）。本模块是 gate 8「最终
ready freeze」绑定面（:1439：clean HEAD、runner bytes、signed HAP/profile/cert、
``.so`` 成员 hash、全部外部输入）的**结构化机器验证层**，替代不可机读的旧文本
freeze 记录（旧 ``ready-freeze-final-*.txt`` 仅作历史只读参考，不参与任何验证，
也不得伪装成新 manifest）。

边界（逐条硬约束）：
- 纯读取：只 ``open(..., 'rb')`` 与 ``zipfile`` 读；**零写入、零证据落盘**。
- 验证本身**不运行** git / hdc / java，不访问网络，不做任何设备交互；
  ``repo_root`` / ``current_code_sha`` / ``current_dirty`` 一律由调用者提供。
- host ``hdc`` 二进制只作为 artifact **读文件核 hash**，绝不执行它。
- signed HAP 只按容器成员（zipfile）核验；**profile / cert 的身份与签名真实性
  由独立 signing 审查验证，本模块不验签、不声称验签**，只绑定字节。
- 审查/偏差审查引用可入册为带 hash artifact（role ``review_citation``），本模块
  对其只做**字节绑定**，不赋予任何授权或裁决含义。
- manifest 自身 hash 由**外部** ``expected_manifest_sha256`` 参数绑定；核验对象
  是实际文件 bytes 的 SHA-256，manifest 自陈的任何 hash 均不作为授权来源。
- **模块不生成、不分配、不预留任何 ID**：候选 AUTH/pair/evidence ID 由治理
  流程（用户在实现与审查完成后）分配；本模块只对 manifest 作者已写入的 ID 做
  格式与三 ID 一致性检查。
- **retired 状态是治理事实，不是记录自陈**：某 pair 是否退役由调用者以
  ``retired_pair_ids``（治理退役 campaign_id 集合）显式传入核验；confirmation
  记录自身的 ``record_status`` / ``verdict`` 等 pass 字面只作记录完整性核对，
  **不得被当作 retired 豁免或授权来源**（治理可退役一条字面自陈 pass 的记录）。
- manifest 是封闭 schema：未知顶层/子对象键、未知 ``schema_version``、非 64 位
  小写 hex 的 SHA-256（旧文档省略/截断 hash 形态）一律拒绝；JSON 中不得出现
  密码/私钥内容/临时 endpoint/真实 target 或 UDID——封闭 schema 不给这些字段
  任何位置，Runtime target 由未来调用者在内存传入。

manifest v1 schema（``schema_version == 1``，封闭键集）::

    {
      "schema_version": 1,
      "manifest_type": "n1bdisc-ready-freeze-manifest",
      "created_local": "YYYY-MM-DD",
      "code_sha": "<40 位小写 hex git commit（clean HEAD，gate 1/:1432）>",
      "pair": {
        "pair_number": <正整数>,
        "authorization_id": "AUTH-N1BDISC-PHYS1API26-YYYYMMDD-NNNN",
        "campaign_id":      "N1BDISC-PHYS1API26-YYYYMMDD-NNNN",
        "evidence_id":      "EV-N1BDISC-PHYS1API26-YYYYMMDD-NNNN"
      },
      "target_tuple": {  # 逐字 == 现行冻结元组（:268；CC-5，2026-09-11 重绑）
        "os": "HarmonyOS", "model": "PLA-AL10",
        "software_version": "PLA-AL10 7.0.0.105(SP10C00E105R7P3)",
        "api_level": 26, "arch": "aarch64", "abi": "arm64-v8a"
      },
      "frozen_literals": {  # 逐字 == 当前 runner 常量（:1618 白名单 / StartEntry）
        "bundle": "cn.alfadb.netbird.n1bdisc",
        "staging_root": "/data/local/tmp/netbird-n1bdisc",
        "module": "entry", "ability": "EntryAbility",
        "hilog_tag": "N1BDiscVpn"
      },
      "runner_sources": [            # 仓库相对路径，按 path 字典序严格升序、无重复
        {"path": "spikes/n1b-disc-phys-hap/runner/<name>.py", "sha256": "<64 hex>"},
        {"path": "spikes/n1b-disc-phys-hap/selftests/<name>.py", "sha256": "<64 hex>"},
        {"path": "spikes/n1b-disc-phys-hap/staticcheck/check_static.py", "sha256": "<64 hex>"}
      ],
      "artifacts": [                 # host 绝对路径（仓外物料），role 白名单封闭
        {"role": "signed_hap",     "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "profile",        "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "cert_chain",     "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "target_binding_confirmation", "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "staticcheck_report",          "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "hdc_binary",     "path": "/abs/...", "sha256": "<64 hex>"},
        {"role": "review_citation", "path": "/abs/...", "sha256": "<64 hex>"}  # 可重复
      ],
      "hap_so_member": {
        "member": "libs/arm64-v8a/libn1bdisc_probe.so",   # 固定成员名（gate 8 :1439）
        "sha256": "<64 hex>"
      }
    }

公共 API（均不落盘）::

    load_manifest(path, *, expected_manifest_sha256=None) -> FreezeManifest
        读 bytes → 实算 SHA-256 → 外部期望 hash 绑定 → JSON 解析 →
        schema_version 门（只认 1）；任何失败抛 :class:`ManifestError`（带 code）。
    validate_manifest(manifest) -> ValidationReport
        纯结构验证（类型/封闭键/ID 格式与三 ID 同日期同序号/CC-2 元组与字面/
        runner 清单形态/artifact role 白名单/hash 格式），零文件 I/O。
    verify_inputs(manifest, *, repo_root, current_code_sha, current_dirty,
                  retired_pair_ids) -> VerifyReport
        先跑结构验证，再逐文件核验：code_sha/dirty、runner 文件集**精确覆盖**
        （无遗漏/重复，逐文件 hash）、artifact 存在性与 hash、signed HAP 内唯一
        arm64-v8a ``.so`` 成员（固定成员名 + hash）、target-binding confirmation
        深度核对（本 pair + CC-2 model/software 事实〔仅去首尾空白、保留内部
        空格〕+ passed 状态字面）；``retired_pair_ids`` 为调用者提供的治理退役
        campaign_id 集合，命中即拒（pair-retired）——不以记录自身字段自陈 pass
        赋权；旧 pair、退役/未通过记录一律拒绝。
    dryrun_precheck(manifest, *, repo_root, current_code_sha, current_dirty,
                    retired_pair_ids) -> dict
        独立预检结果对象（gate 11 前置自查）。**不改写 DryRun 记录的
        ``integrity={}`` 字面**（:1442，``n1bdisc_run.build_record`` 同源）；
        后续 CLI 据此拒启或另行单独记录。

候选 runner 文件集合语义（与 gate 8 审查席 m-1 聚合算法的文件域逐字同源）：
``spikes/n1b-disc-phys-hap/runner/*.py`` 与 ``selftests/*.py`` 的**直接子文件**
（非递归，``__pycache__`` 与子目录不计）+ ``staticcheck/check_static.py`` 单文件；
manifest 的 ``runner_sources`` 必须与该实际集合**精确相等**。

本模块常量直接取自 ``n1bdisc_core`` / ``n1bdisc_hdc``（单一事实来源，导入即断言
对齐），CC-2 元组为判据 :268 字面。模块导入零副作用。
"""

from __future__ import annotations

import datetime as _dt
import hashlib
import json
import os
import re
import zipfile
from dataclasses import dataclass
from typing import Any, Dict, List, Mapping, Optional, Tuple

import n1bdisc_core as _core
import n1bdisc_hdc as _hdc

__all__ = [
    "ManifestError", "FreezeManifest", "Failure", "ValidationReport",
    "VerifyReport", "load_manifest", "validate_manifest", "verify_inputs",
    "dryrun_precheck", "SCHEMA_VERSION", "MANIFEST_TYPE", "SPIKE_ROOT",
    "HAP_SO_MEMBER", "CC2_TARGET_TUPLE", "FROZEN_LITERALS",
    "REQUIRED_ARTIFACT_ROLES", "ALLOWED_ARTIFACT_ROLES",
]

# ---------------------------------------------------------------------------
# 冻结常量（单一事实来源 + 判据字面）
# ---------------------------------------------------------------------------

#: manifest schema 版本（唯一受支持值；未知版本在 load 即拒绝）。
SCHEMA_VERSION = 1
SUPPORTED_SCHEMA_VERSIONS: Tuple[int, ...] = (SCHEMA_VERSION,)
MANIFEST_TYPE = "n1bdisc-ready-freeze-manifest"

#: spike 在仓库内的根（runner 清单路径域前缀，仓库相对）。
SPIKE_ROOT = "spikes/n1b-disc-phys-hap"
#: runner 直接子文件目录（仓库相对，非递归 *.py）。
_RUNNER_DIRS: Tuple[str, ...] = ("runner", "selftests")
#: staticcheck 单文件（仓库相对）。
_STATICSHECK_FILE = SPIKE_ROOT + "/staticcheck/check_static.py"

#: signed HAP 内唯一 arm64-v8a ``.so`` 成员的固定名（gate 8 :1439；probe/Cargo.toml
#: ``[lib] name = "n1bdisc_probe"`` cdylib → HAP ``libs/arm64-v8a/`` 布局）。
HAP_SO_MEMBER = "libs/arm64-v8a/libn1bdisc_probe.so"

#: 现行冻结目标元组（判据 :268；CC-5，2026-09-11 用户授权重绑字面；逐字比对）。
#: 常量名 ``CC2_TARGET_TUPLE`` 为最小兼容保留（不全局重命名），值随 CC-5 更新。
CC2_TARGET_TUPLE: Dict[str, Any] = {
    "os": "HarmonyOS",
    "model": "PLA-AL10",
    "software_version": "PLA-AL10 7.0.0.105(SP10C00E105R7P3)",
    "api_level": 26,
    "arch": "aarch64",
    "abi": "arm64-v8a",
}

# StartEntry 冻结 argv 中的 module / ability 字面（判据 :1632）；导入即与当前
# runner 白名单表断言对齐——runner 常量漂移时本模块拒绝导入，杜绝双层字面漂移。
_START_ABILITY = "EntryAbility"
_START_MODULE = "entry"
assert _hdc.HDC_ARGV_TABLE["StartEntry"] == (
    "-t", "{T}", "shell", "aa", "start",
    "-a", _START_ABILITY, "-b", _core.DEFAULT_BUNDLE, "-m", _START_MODULE,
), "StartEntry 白名单 argv 漂移：module/ability 常量须随 :1632 同步复核"

#: 冻结字面集（bundle/staging 取自 core/hdc 常量——与 :1618/:1626-1632 同源）。
FROZEN_LITERALS: Dict[str, str] = {
    "bundle": _core.DEFAULT_BUNDLE,
    "staging_root": _hdc.STAGING_ROOT,
    "module": _START_MODULE,
    "ability": _START_ABILITY,
    "hilog_tag": _core.HILOG_TAG,
}

#: 必备 artifact role（gate 8 :1439 绑定面 + target-binding confirmation）。
REQUIRED_ARTIFACT_ROLES: Tuple[str, ...] = (
    "signed_hap", "profile", "cert_chain", "target_binding_confirmation",
    "staticcheck_report", "hdc_binary",
)
#: 可重复的引用型 role（审查/偏差审查引用；仅字节绑定，无授权/裁决语义）。
REPEATABLE_ARTIFACT_ROLES: Tuple[str, ...] = ("review_citation",)
ALLOWED_ARTIFACT_ROLES: Tuple[str, ...] = REQUIRED_ARTIFACT_ROLES + REPEATABLE_ARTIFACT_ROLES

# target-binding confirmation 记录核对字面（record 形态以 gate 5 产物为**形态**
# 参照；不引用任何真实记录作为有效正例）。任何非 passed 字面（含退役记录的
# ``consumed-blocked-final`` / ``blocked-tuple-drift``）拒绝；治理 retired 状态
# 另由调用者 ``retired_pair_ids`` 显式传入核对，不靠记录字段自陈。
CONF_RECORD_TYPE = "n1bdisc-target-binding-confirmation"
CONF_STATUS_PASS = "confirmed-pass"
CONF_VERDICT_PASS = "pass-tuple-bind-confirmed"

# 三 ID 家族（沿用既有治理命名：AUTH-N1BDISC-PHYS1API26-YYYYMMDD-NNNN，判据
# :1790 CC-2 块「新治理须新 AUTH/pair/evidence 三 ID」；三 ID 须同日期、同序号段）。
_ID_BODY = r"N1BDISC-PHYS1API26-(\d{8})-(\d{4})"
RE_AUTH_ID = re.compile(r"^AUTH-" + _ID_BODY + r"$")
RE_CAMPAIGN_ID = re.compile(r"^" + _ID_BODY + r"$")
RE_EVIDENCE_ID = re.compile(r"^EV-" + _ID_BODY + r"$")
RE_SHA256 = re.compile(r"^[0-9a-f]{64}$")
RE_CODE_SHA = re.compile(r"^[0-9a-f]{40}$")
RE_DATE_LOCAL = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# 封闭键集（任何未知键 = 拒绝）。
_TOP_KEYS = frozenset({
    "schema_version", "manifest_type", "created_local", "code_sha", "pair",
    "target_tuple", "frozen_literals", "runner_sources", "artifacts",
    "hap_so_member",
})
_PAIR_KEYS = frozenset({"pair_number", "authorization_id", "campaign_id", "evidence_id"})
_SO_MEMBER_KEYS = frozenset({"member", "sha256"})
_RUNNER_ENTRY_KEYS = frozenset({"path", "sha256"})
_ARTIFACT_ENTRY_KEYS = frozenset({"role", "path", "sha256"})

_ALLOWED_RUNNER_PREFIXES: Tuple[str, ...] = tuple(
    SPIKE_ROOT + "/" + d + "/" for d in _RUNNER_DIRS)


# ---------------------------------------------------------------------------
# 错误与结果对象
# ---------------------------------------------------------------------------

class ManifestError(Exception):
    """load 阶段失败（文件不可读/JSON 损坏/外部 hash 不符/未知 schema 等）。"""

    def __init__(self, code: str, message: str,
                 detail: Optional[Mapping[str, Any]] = None) -> None:
        super().__init__("freeze-manifest: %s (%s)" % (code, message))
        self.code = code
        self.message = message
        self.detail = dict(detail or {})


@dataclass(frozen=True)
class FreezeManifest:
    """已加载的候选 manifest：实际 bytes 的 hash + 原始解析数据。"""

    path: str                    # 调用者传入的路径原样（不再二次解析）
    manifest_sha256: str         # 实际文件 bytes 的 SHA-256（小写 hex）
    data: Mapping[str, Any]      # JSON 解析结果（只读视图语义）


@dataclass(frozen=True)
class Failure:
    """一条校验失败（code 稳定字面 + 结构化 detail）。"""

    code: str
    detail: Mapping[str, Any]

    def as_dict(self) -> Dict[str, Any]:
        return {"code": self.code, "detail": dict(self.detail)}


@dataclass(frozen=True)
class ValidationReport:
    """结构验证结果（零文件 I/O）。"""

    ok: bool
    failures: Tuple[Failure, ...]

    def as_dict(self) -> Dict[str, Any]:
        return {"ok": self.ok, "failures": [f.as_dict() for f in self.failures]}


@dataclass(frozen=True)
class VerifyReport:
    """输入核验结果（文件级）。``validation`` 为其前置结构验证摘要。"""

    ok: bool
    failures: Tuple[Failure, ...]
    validation: ValidationReport

    def as_dict(self) -> Dict[str, Any]:
        return {
            "ok": self.ok,
            "failures": [f.as_dict() for f in self.failures],
            "validation": self.validation.as_dict(),
        }


def _fail(out: List[Failure], code: str, **detail: Any) -> None:
    out.append(Failure(code, detail))


# ---------------------------------------------------------------------------
# load：bytes → 外部 hash 绑定 → JSON → schema_version 门
# ---------------------------------------------------------------------------

def load_manifest(path: Any, *,
                  expected_manifest_sha256: Optional[str] = None) -> FreezeManifest:
    """读取候选 manifest 并绑定外部期望 hash。

    ``expected_manifest_sha256`` 由调用者从**本模块之外**的可信通道取得；
    给出时必须为完整 64 位小写 hex（旧文档省略/截断 hash 形态直接拒绝），且与
    实际文件 bytes 的 SHA-256 逐字一致。manifest 自身内容不参与该判定。
    """
    p = os.fspath(path)
    if not os.path.isfile(p):
        raise ManifestError("manifest-not-found", "manifest 文件不存在", {"path": p})
    try:
        with open(p, "rb") as fh:
            raw = fh.read()
    except OSError as exc:
        raise ManifestError("manifest-unreadable", "manifest 读取失败",
                            {"path": p, "error": str(exc)}) from exc
    actual = hashlib.sha256(raw).hexdigest()
    if expected_manifest_sha256 is not None:
        if not (isinstance(expected_manifest_sha256, str)
                and RE_SHA256.match(expected_manifest_sha256)):
            raise ManifestError(
                "expected-manifest-sha256-format",
                "expected_manifest_sha256 必须为完整 64 位小写 hex（不接受省略/截断形态）",
                {"expected_manifest_sha256": str(expected_manifest_sha256)})
        if actual != expected_manifest_sha256:
            raise ManifestError(
                "manifest-sha256-mismatch",
                "manifest 实际 bytes SHA-256 与外部期望不符",
                {"expected_manifest_sha256": expected_manifest_sha256,
                 "actual_manifest_sha256": actual})
    try:
        data = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise ManifestError("manifest-invalid-json", "manifest 不是合法 UTF-8 JSON",
                            {"path": p, "error": str(exc)}) from exc
    if not isinstance(data, dict):
        raise ManifestError("manifest-root-type", "manifest 顶层必须为 JSON object",
                            {"path": p})
    sv = data.get("schema_version")
    if sv is None:
        raise ManifestError("schema-version-missing", "缺少 schema_version")
    if isinstance(sv, bool) or not isinstance(sv, int):
        raise ManifestError("schema-version-type", "schema_version 必须为整数",
                            {"schema_version": _render(sv)})
    if sv not in SUPPORTED_SCHEMA_VERSIONS:
        raise ManifestError("unknown-schema-version",
                            "未知 schema_version（只认 %s）"
                            % (list(SUPPORTED_SCHEMA_VERSIONS),),
                            {"schema_version": sv})
    return FreezeManifest(path=p, manifest_sha256=actual, data=data)


# ---------------------------------------------------------------------------
# validate：纯结构验证（零文件 I/O）
# ---------------------------------------------------------------------------

def _render(v: Any) -> str:
    try:
        return json.dumps(v, ensure_ascii=False)
    except (TypeError, ValueError):
        return repr(v)


def _exact_keys(obj: Any, keys: frozenset, ctx: str, out: List[Failure]) -> bool:
    if not isinstance(obj, dict):
        _fail(out, ctx + "-type", expected="object", actual=_render(obj))
        return False
    got = frozenset(obj.keys())
    if got != keys:
        _fail(out, ctx + "-keys", expected=sorted(keys), actual=sorted(got),
              unknown=sorted(got - keys), missing=sorted(keys - got))
        return False
    return True


def _check_sha256_field(container: Mapping[str, Any], key: str, ctx: str,
                        out: List[Failure]) -> None:
    v = container.get(key)
    if not (isinstance(v, str) and RE_SHA256.match(v)):
        _fail(out, ctx + "-sha256-format",
              expected="64 位小写 hex SHA-256", actual=_render(v))


def validate_manifest(manifest: FreezeManifest) -> ValidationReport:
    """结构验证：类型/封闭键/格式/一致性/常量比对；不触任何文件。"""
    out: List[Failure] = []
    data = manifest.data

    if not _exact_keys(data, _TOP_KEYS, "manifest", out):
        return ValidationReport(False, tuple(out))

    sv = data["schema_version"]
    if isinstance(sv, bool) or not isinstance(sv, int) or sv != SCHEMA_VERSION:
        _fail(out, "schema-version-unsupported", expected=SCHEMA_VERSION,
              actual=_render(sv))
    if data["manifest_type"] != MANIFEST_TYPE:
        _fail(out, "manifest-type-mismatch", expected=MANIFEST_TYPE,
              actual=_render(data["manifest_type"]))

    created = data["created_local"]
    if not (isinstance(created, str) and RE_DATE_LOCAL.match(created)):
        _fail(out, "created-local-format", expected="YYYY-MM-DD",
              actual=_render(created))
    else:
        try:
            _dt.date.fromisoformat(created)
        except ValueError:
            _fail(out, "created-local-format", expected="真实日历日期",
                  actual=created)

    code_sha = data["code_sha"]
    if not (isinstance(code_sha, str) and RE_CODE_SHA.match(code_sha)):
        _fail(out, "code-sha-format",
              expected="40 位小写 hex git commit", actual=_render(code_sha))

    # -- pair：三 ID 格式 + 同日期 + 同序号段 + 前缀体一致 --------------------
    pair = data["pair"]
    if _exact_keys(pair, _PAIR_KEYS, "pair", out):
        pn = pair["pair_number"]
        if isinstance(pn, bool) or not isinstance(pn, int) or pn < 1:
            _fail(out, "pair-number-domain",
                  expected="正整数", actual=_render(pn))
        ids = {}
        for key, rx, code in (
                ("authorization_id", RE_AUTH_ID, "pair-authorization-id-format"),
                ("campaign_id", RE_CAMPAIGN_ID, "pair-campaign-id-format"),
                ("evidence_id", RE_EVIDENCE_ID, "pair-evidence-id-format")):
            v = pair[key]
            m = rx.match(v) if isinstance(v, str) else None
            if m is None:
                _fail(out, code, expected=rx.pattern, actual=_render(v))
            else:
                ids[key] = m.group(1, 2)
        if len(ids) == 3:
            (date, serial), *_ = ids.values()
            if any(b != (date, serial) for b in ids.values()):
                _fail(out, "pair-id-inconsistent",
                      expected="三 ID 同日期段与同序号段", observed=ids)
        # 前缀体一致性（AUTH-/EV- == campaign 体）由上面三条格式正则强制：
        # 三条全匹配即保证 AUTH-<body> == auth、EV-<body> == evidence。

    # -- CC-2 元组与冻结字面：逐字相等（内部空格同样逐字，不容增删） ---------
    tup = data["target_tuple"]
    if not isinstance(tup, dict):
        _fail(out, "target-tuple-type", expected="object", actual=_render(tup))
    elif not _type_strict_equal(tup, CC2_TARGET_TUPLE):
        diff = {k: {"expected": CC2_TARGET_TUPLE[k], "actual": tup.get(k, "<缺>")}
                for k in CC2_TARGET_TUPLE if tup.get(k, _SENTINEL) != CC2_TARGET_TUPLE[k]}
        extra = sorted(set(tup) - set(CC2_TARGET_TUPLE))
        if not diff and not extra:
            diff = {"<type>": "值类型漂移（如 26.0 float 冒充 26 int / bool 冒充 int）"}
        _fail(out, "target-tuple-drift",
              note="必须逐字等于现行冻结元组（判据 :268，CC-5；类型严格）",
              diff=diff, unknown_keys=extra)
    lit = data["frozen_literals"]
    if not isinstance(lit, dict):
        _fail(out, "frozen-literals-type", expected="object", actual=_render(lit))
    elif not _type_strict_equal(lit, FROZEN_LITERALS):
        diff = {k: {"expected": FROZEN_LITERALS[k], "actual": lit.get(k, "<缺>")}
                for k in FROZEN_LITERALS if lit.get(k, _SENTINEL) != FROZEN_LITERALS[k]}
        extra = sorted(set(lit) - set(FROZEN_LITERALS))
        _fail(out, "frozen-literals-drift",
              note="必须逐字等于当前 runner 常量（:1618/:1626-1632）",
              diff=diff, unknown_keys=extra)

    _validate_runner_sources(data["runner_sources"], out)
    _validate_artifacts(data["artifacts"], out)

    so = data["hap_so_member"]
    if _exact_keys(so, _SO_MEMBER_KEYS, "hap-so-member", out):
        if so["member"] != HAP_SO_MEMBER:
            _fail(out, "so-member-name-fixed", expected=HAP_SO_MEMBER,
                  actual=_render(so["member"]))
        _check_sha256_field(so, "sha256", "hap-so-member", out)

    return ValidationReport(not out, tuple(out))


#: 比较哨兵：字段缺失与「值恰为 None」区分开。
_SENTINEL = object()


def _type_strict_equal(actual: Any, expected: Mapping[str, Any]) -> bool:
    """键集与值域**类型严格**相等（int 不容 bool/float——``26.0 == 26`` 的
    dict 相等捷径会放进 JSON 数字漂移，这里显式排除）。"""
    if not isinstance(actual, dict) or frozenset(actual) != frozenset(expected):
        return False
    for key, exp in expected.items():
        got = actual[key]
        if isinstance(exp, bool):
            if not isinstance(got, bool):
                return False
        elif isinstance(exp, int):
            if isinstance(got, bool) or not isinstance(got, int):
                return False
        elif isinstance(exp, str):
            if not isinstance(got, str):
                return False
        if got != exp:
            return False
    return True


def _validate_runner_sources(entries: Any, out: List[Failure]) -> None:
    if not isinstance(entries, list) or not entries:
        _fail(out, "runner-sources-type", expected="非空 array")
        return
    paths: List[str] = []
    for idx, ent in enumerate(entries):
        ctx = "runner-sources[%d]" % idx
        if not _exact_keys(ent, _RUNNER_ENTRY_KEYS, ctx, out):
            continue
        p = ent["path"]
        if not isinstance(p, str) or not p:
            _fail(out, ctx + "-path-type", expected="非空字符串",
                  actual=_render(p))
            continue
        if os.path.isabs(p):
            _fail(out, ctx + "-path-absolute",
                  note="runner 清单路径必须是仓库相对路径", actual=p)
        elif os.sep != "/" and os.sep in p:
            _fail(out, ctx + "-path-separator", note="必须使用 posix 分隔符", actual=p)
        elif p != os.path.normpath(p) or ".." in p.split("/"):
            _fail(out, ctx + "-path-escape",
                  note="必须为规范化相对路径（禁止 .. / 冗余段）", actual=p)
        elif not (p.startswith(_ALLOWED_RUNNER_PREFIXES) or p == _STATICSHECK_FILE):
            _fail(out, ctx + "-path-domain",
                  expected="runner/*.py | selftests/*.py | staticcheck/check_static.py"
                           "（%s 下）" % SPIKE_ROOT,
                  actual=p)
        else:
            paths.append(p)
        _check_sha256_field(ent, "sha256", ctx, out)
    if paths and paths != sorted(paths):
        _fail(out, "runner-not-sorted",
              note="runner_sources 必须按 path 字典序严格升序",
              first_out_of_order=next(
                  paths[i] for i in range(1, len(paths)) if paths[i - 1] >= paths[i]))
    if len(set(paths)) != len(paths):
        dupes = sorted({p for p in paths if paths.count(p) > 1})
        _fail(out, "runner-duplicate", duplicates=dupes)


def _validate_artifacts(entries: Any, out: List[Failure]) -> None:
    if not isinstance(entries, list) or not entries:
        _fail(out, "artifacts-type", expected="非空 array")
        return
    seen_roles: Dict[str, int] = {}
    seen_paths: Dict[str, str] = {}
    for idx, ent in enumerate(entries):
        ctx = "artifacts[%d]" % idx
        if not _exact_keys(ent, _ARTIFACT_ENTRY_KEYS, ctx, out):
            continue
        role = ent["role"]
        p = ent["path"]
        if not isinstance(role, str) or role not in ALLOWED_ARTIFACT_ROLES:
            _fail(out, ctx + "-role-unknown",
                  expected=sorted(ALLOWED_ARTIFACT_ROLES), actual=_render(role))
        elif role in seen_roles and role not in REPEATABLE_ARTIFACT_ROLES:
            _fail(out, ctx + "-role-duplicate", role=role,
                  first_index=seen_roles[role])
        else:
            seen_roles[role] = idx
        if not isinstance(p, str) or not p:
            _fail(out, ctx + "-path-type", expected="非空字符串", actual=_render(p))
        elif not os.path.isabs(p):
            _fail(out, ctx + "-path-relative",
                  note="artifact 路径必须是 host 绝对路径（仓外物料）", actual=p)
        elif p != os.path.normpath(p) or ".." in p.split(os.sep):
            _fail(out, ctx + "-path-escape",
                  note="必须为规范化绝对路径（禁止 .. / 冗余段）", actual=p)
        elif p in seen_paths:
            _fail(out, ctx + "-path-duplicate", path=p, other_role=seen_paths[p])
        else:
            seen_paths[p] = role if isinstance(role, str) else "<invalid-role>"
        _check_sha256_field(ent, "sha256", ctx, out)
    missing = [r for r in REQUIRED_ARTIFACT_ROLES if r not in seen_roles]
    if missing:
        _fail(out, "artifact-role-required-missing", missing_roles=missing)


# ---------------------------------------------------------------------------
# verify：文件级核验（纯读取；输入全部由调用者提供）
# ---------------------------------------------------------------------------

def _sha256_file(path: str) -> Optional[str]:
    """文件 bytes 的 SHA-256；读失败返回 ``None``。"""
    h = hashlib.sha256()
    try:
        with open(path, "rb") as fh:
            for chunk in iter(lambda: fh.read(1 << 20), b""):
                h.update(chunk)
    except OSError:
        return None
    return h.hexdigest()


def _actual_runner_paths(repo_root: str) -> set:
    """实际候选 runner 文件集合（仓库相对 posix 路径；m-1 聚合算法文件域）。

    ``runner/*.py`` 与 ``selftests/*.py`` 的**直接子文件**（非递归，
    ``__pycache__``/子目录不计）+ ``staticcheck/check_static.py``。
    """
    out = set()
    for d in _RUNNER_DIRS:
        base = os.path.join(repo_root, SPIKE_ROOT, d)
        if not os.path.isdir(base):
            continue
        for name in os.listdir(base):
            full = os.path.join(base, name)
            if name.endswith(".py") and os.path.isfile(full):
                out.add(SPIKE_ROOT + "/" + d + "/" + name)
    if os.path.isfile(os.path.join(repo_root, _STATICSHECK_FILE)):
        out.add(_STATICSHECK_FILE)
    return out


def verify_inputs(manifest: FreezeManifest, *, repo_root: Any,
                  current_code_sha: str, current_dirty: bool,
                  retired_pair_ids: Any) -> VerifyReport:
    """机器核验 manifest 绑定的全部输入（先结构后文件；纯读取）。

    ``repo_root``：仓库根目录（runner 清单按其解析为仓库相对路径，只解析一次）；
    ``current_code_sha``：调用者提供的当前 clean HEAD git commit（40 位小写 hex）；
    ``current_dirty``：调用者提供的树污染状态（``True`` = 拒绝，gate 1 :1432
    clean HEAD 纪律）；
    ``retired_pair_ids``：**必填**，调用者提供的治理退役 campaign_id 集合
    （空集 = 调用者明示「按当前治理状态无退役 pair」；这是显式输入断言，
    不是模块默认采信记录自陈）。manifest pair 命中即拒绝（``pair-retired``）。

    本函数不运行 git/hdc/java、不访问网络。
    """
    validation = validate_manifest(manifest)
    if not validation.ok:
        return VerifyReport(False, validation.failures, validation)

    out: List[Failure] = []
    data = manifest.data
    repo = os.fspath(repo_root)
    # 调用者输入自身合法性（契约输入不合格即拒绝，不做静默容错）
    if not (isinstance(current_code_sha, str)
            and RE_CODE_SHA.match(current_code_sha)):
        _fail(out, "caller-input-invalid", field="current_code_sha",
              expected="40 位小写 hex git commit", actual=_render(current_code_sha))
    if not isinstance(current_dirty, bool):
        _fail(out, "caller-input-invalid", field="current_dirty",
              expected="bool", actual=_render(current_dirty))
    if isinstance(retired_pair_ids, (str, bytes)) or not hasattr(
            retired_pair_ids, "__iter__"):
        _fail(out, "caller-input-invalid", field="retired_pair_ids",
              expected="campaign_id 字符串集合", actual=_render(retired_pair_ids))
    if not os.path.isdir(repo):
        _fail(out, "repo-root-invalid", repo_root=repo)
    if out:
        return VerifyReport(False, tuple(out), validation)

    retired = frozenset(str(x) for x in retired_pair_ids)

    # -- gate 1 绑定：clean HEAD code_sha + 干净树 ---------------------------
    if current_code_sha != data["code_sha"]:
        _fail(out, "code-sha-mismatch", manifest_code_sha=data["code_sha"],
              current_code_sha=current_code_sha)
    if current_dirty:
        _fail(out, "dirty-tree",
              note="gate 8 最终 freeze 只接受 clean HEAD（:1432）；"
                   "当前树有未提交/未跟踪变更即拒绝")

    # -- 治理状态：pair 是否已退役（调用者提供的治理事实，非记录自陈） -------
    campaign = data["pair"]["campaign_id"]
    if campaign in retired:
        _fail(out, "pair-retired", campaign_id=campaign,
              note="该 pair 已由治理状态退役（retired_pair_ids 命中）；"
                   "其 confirmation 不得再作为有效输入")

    # -- runner 文件集精确覆盖 + 逐文件 hash ---------------------------------
    listed = [e["path"] for e in data["runner_sources"]]
    actual = _actual_runner_paths(repo)
    for p in sorted(actual - set(listed)):
        _fail(out, "runner-file-missing", path=p,
              note="实际候选文件未列入 manifest runner_sources（无遗漏）")
    for p in sorted(set(listed) - actual):
        _fail(out, "runner-file-extra", path=p,
              note="manifest 列出了不存在的候选文件（无多列）")
    repo_abs = os.path.abspath(repo)  # 一次性定基，避免重复解析漂移
    for ent in data["runner_sources"]:
        p = ent["path"]
        if p not in actual:
            continue
        full = os.path.join(repo_abs, p)
        got = _sha256_file(full)
        if got is None:
            _fail(out, "runner-file-unreadable", path=p)
        elif got != ent["sha256"]:
            _fail(out, "runner-hash-mismatch", path=p,
                  expected_sha256=ent["sha256"], actual_sha256=got)

    # -- artifacts：存在性 + hash（hdc_binary 只读文件，绝不执行） -----------
    artifact_paths = {e["role"]: e["path"] for e in data["artifacts"]}
    for ent in data["artifacts"]:
        p = ent["path"]
        if not os.path.isfile(p):
            _fail(out, "artifact-missing", role=ent["role"], path=p)
            continue
        got = _sha256_file(p)
        if got is None:
            _fail(out, "artifact-unreadable", role=ent["role"], path=p)
        elif got != ent["sha256"]:
            _fail(out, "artifact-hash-mismatch", role=ent["role"], path=p,
                  expected_sha256=ent["sha256"], actual_sha256=got)

    # -- signed HAP：唯一 arm64-v8a .so 成员（固定成员名 + hash） ------------
    _verify_hap_so_member(data, artifact_paths.get("signed_hap"), out)

    # -- target-binding confirmation 深度核对 --------------------------------
    _verify_confirmation(data, artifact_paths.get("target_binding_confirmation"),
                         out)

    return VerifyReport(not out, tuple(out), validation)


def _verify_hap_so_member(data: Mapping[str, Any], hap_path: Optional[str],
                          out: List[Failure]) -> None:
    declared_member = data["hap_so_member"]["member"]
    declared_sha = data["hap_so_member"]["sha256"]
    if hap_path is None or not os.path.isfile(hap_path):
        return  # artifact 缺失/漂移已由通用 artifact 核验登记
    try:
        with zipfile.ZipFile(hap_path) as zf:
            so_names = [n for n in zf.namelist() if n.endswith(".so")]
            if len(so_names) != 1:
                _fail(out, "so-member-count", expected="恰 1 个 .so 成员",
                      observed=sorted(so_names))
                return
            if so_names[0] != declared_member:
                _fail(out, "so-member-name", expected=declared_member,
                      actual=so_names[0])
                return
            member_sha = hashlib.sha256(zf.read(declared_member)).hexdigest()
    except (zipfile.BadZipFile, OSError, RuntimeError) as exc:
        _fail(out, "hap-not-zip", path=hap_path, error=str(exc))
        return
    if member_sha != declared_sha:
        _fail(out, "so-member-hash-mismatch", member=declared_member,
              expected_sha256=declared_sha, actual_sha256=member_sha)


def _strip_edge(v: Any) -> Any:
    """仅去**首尾**空白（与 gate 5 复核口径同源：内部空格是字面一部分，保留）。"""
    return v.strip() if isinstance(v, str) else v


def _verify_confirmation(data: Mapping[str, Any], conf_path: Optional[str],
                         out: List[Failure]) -> None:
    if conf_path is None or not os.path.isfile(conf_path):
        return  # artifact 缺失/漂移已由通用 artifact 核验登记
    try:
        with open(conf_path, "rb") as fh:
            rec = json.loads(fh.read().decode("utf-8"))
    except (OSError, UnicodeDecodeError, ValueError) as exc:
        _fail(out, "confirmation-parse", path=conf_path, error=str(exc))
        return
    if not isinstance(rec, dict):
        _fail(out, "confirmation-shape", expected="JSON object",
              actual=_render(rec))
        return
    if rec.get("record_type") != CONF_RECORD_TYPE:
        _fail(out, "confirmation-record-type", expected=CONF_RECORD_TYPE,
              actual=_render(rec.get("record_type")))
    # passed 状态：退役（consumed-blocked-final 等）/未通过（blocked-tuple-drift
    # 等）/其他任何非 passed 字面一律拒绝（旧 pair-1 记录即落在此支）。
    if rec.get("record_status") != CONF_STATUS_PASS:
        _fail(out, "confirmation-status-not-pass", expected=CONF_STATUS_PASS,
              actual=_render(rec.get("record_status")))
    if rec.get("verdict") != CONF_VERDICT_PASS:
        _fail(out, "confirmation-verdict-not-pass", expected=CONF_VERDICT_PASS,
              actual=_render(rec.get("verdict")))
    # 本 pair 绑定：三 ID 逐一相等（旧 pair / 复用他 pair 记录在此拒绝）
    pair = data["pair"]
    for key in ("authorization_id", "campaign_id", "evidence_id"):
        if rec.get(key) != pair[key]:
            _fail(out, "confirmation-pair-mismatch", field=key,
                  expected=pair[key], actual=_render(rec.get(key)))
    # CC-2 model/software 事实：仅去首尾空白后逐字比较（内部空格保留）
    tt = rec.get("frozen_target_tuple")
    if not isinstance(tt, dict):
        _fail(out, "confirmation-shape", expected="frozen_target_tuple object",
              actual=_render(tt))
        return
    if _strip_edge(tt.get("model")) != CC2_TARGET_TUPLE["model"]:
        _fail(out, "confirmation-model-drift",
              expected=CC2_TARGET_TUPLE["model"], actual=_render(tt.get("model")))
    if _strip_edge(tt.get("software_version")) != CC2_TARGET_TUPLE["software_version"]:
        _fail(out, "confirmation-software-version-drift",
              expected=CC2_TARGET_TUPLE["software_version"],
              actual=_render(tt.get("software_version")),
              note="仅去首尾空白；内部空格必须逐字保留")


# ---------------------------------------------------------------------------
# dryrun 预检：独立结果对象，绝不改写 DryRun 记录 integrity={} 字面
# ---------------------------------------------------------------------------

def dryrun_precheck(manifest: FreezeManifest, *, repo_root: Any,
                    current_code_sha: str, current_dirty: bool,
                    retired_pair_ids: Any) -> Dict[str, Any]:
    """gate 11 前的独立 manifest 预检（gate 11 :1442：is_evidence=false、HDC0、
    integrity empty）。

    参数与 :func:`verify_inputs` 同义（``retired_pair_ids`` = **必填**，调用者
    提供的治理退役 campaign_id 集合）。返回独立的预检结果 dict（含全部失败
    清单）；**不写入任何文件**，也不触碰 ``n1bdisc_run.build_record`` 产出的
    DryRun 记录及其 ``integrity={}`` 字面。后续 CLI 以本结果决定拒启
    （refuse-to-start）或将其**单独另行记录**。
    """
    report = verify_inputs(manifest, repo_root=repo_root,
                           current_code_sha=current_code_sha,
                           current_dirty=current_dirty,
                           retired_pair_ids=retired_pair_ids)
    return {
        "precheck_type": "n1bdisc-freeze-manifest-dryrun-precheck",
        "is_evidence": False,
        "schema_version": SCHEMA_VERSION,
        "manifest_path": manifest.path,
        "manifest_sha256": manifest.manifest_sha256,
        "ok": report.ok,
        "failures": [f.as_dict() for f in report.failures],
        "note": "独立预检结果：由后续 CLI 拒启或单独记录；"
                "不改写 DryRun 记录 integrity={} 字面（判据 :1442）",
    }
