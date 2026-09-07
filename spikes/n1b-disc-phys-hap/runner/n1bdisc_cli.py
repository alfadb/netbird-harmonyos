#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc_cli — N1BDISC host runner CLI 正式入口（live 冻结门 + dryrun engine 接线）。

生产 ``--live`` / ``--dryrun`` 都经 :func:`n1bdisc_engine.run_campaign` 同一 engine
（mode×transport 配对：live→RealHdcTransport、dryrun→FakeHdc 适配，由 engine 强
制）；``n1bdisc_run.main`` 是薄转发。旧 ``n1bdisc_run.run_dryrun_campaign`` helper
保留给基线单测，不再是任何 CLI 路径。

用法（三种形态）::

    # 1) 生产 Live：完整外部冻结四件套 + 新 output-root + 终端双重确认
    python3 runner/n1bdisc_cli.py --live \
        --target <T> \
        --freeze-manifest MANIFEST.json --freeze-sha256 <64hex> \
        --governance-record GOV.json --governance-sha256 <64hex> \
        --output-root NEWDIR [--json OUT.json]
    # 终端交互（严格逐字，区分大小写）：
    #   LIVE-confirm>    LIVE <manifest.pair.campaign_id 完整字面>
    #   operator-ready>  OPERATOR-READY

    # 2) 正式 DryRun（freeze-bound）：同一冻结输入预检；不消费 pair、零终端确认
    python3 runner/n1bdisc_cli.py --dryrun \
        --freeze-manifest MANIFEST.json --freeze-sha256 <64hex> \
        --governance-record GOV.json --governance-sha256 <64hex> \
        --output-root NEWDIR [--scenario happy] [--json OUT.json]

    # 3) 离线 smoke（无 manifest；freeze_bound=false / gate11_eligible=false，
    #    不具正式门 11 资格）：仍走同一 engine，不再走旧平行模拟器
    python3 runner/n1bdisc_cli.py --dryrun --scenario happy \
        [--target T] [--hap H] [--output-root NEWDIR] [--json OUT.json]

退出码：``0`` = verdict pass；``1`` = campaign verdict fail（fail-closed 反例的机
器证据）；``2`` = argparse 用法拒绝（含 SHA-256 格式、freeze 四件套部分缺失/
部分给全、形态互斥）；``3`` = 拒启（preflight 失败 / 确认不符 / claim 已存在 /
确认不可得；**零 hdc 操作、零 ID 消费**）；``4`` = 启动后运行面错误（记录写盘
失败等；claim 不回滚——operator-ready 已确认即消费）。

安全边界（与 ``n1bdisc_engine`` / ``n1bdisc_freeze_manifest`` 契约同源）：

- **preflight 先于一切**：真实 ``git rev-parse HEAD`` + ``git status --porcelain``
  与 manifest/governance 完整校验全部在任何 transport 构造 / subprocess hdc 之
  前；任一失败拒启（exit 3）。git 探针与 stdin 确认可注入（测试），生产默认调
  用真实来源，不接受 caller 自由传入的 dirty=False。
- **live 收口复核（integrity）**：preflight 通过后 CLI 构造只读 revalidate 回
  调（:func:`make_freeze_revalidate`；expected hash 取预检校验过的外部字面）
  注入 engine，host finally 清理全部结束、正式 verdict 求值/封签之前恰一次
  重复校验冻结输入；结论真实写入 ``record.integrity``（结构见
  ``n1bdisc_engine._finally_integrity_close`` docstring），失败码非空走既有
  verdict invalid 优先轴（不新造判据/cause），记录在 ``RunRecorder.finish``
  前封存。DryRun 恒 ``integrity={}``（门 11 原字面）。
- **target 仅内存**：只传 executor，不打印、不入任何文件；raw 文本定点脱敏由
  engine 承担；stdout 永远只有 run-root/阶段/失败摘要（单行 JSON），完整最终
  记录只进 ``--json`` 文件（同样不含 target，argv 永不入档）。
- **hdc 可执行路径只取 manifest artifact role=``hdc_binary``**（经 hash 核验），
  不做 PATH 查找、不允许临时覆盖；live 的 ``hap_path`` 同理只取 role=
  ``signed_hap`` 的 manifest 绑定路径（SendHap 上设备的就是冻结字节）。
- **单次性 claim**：受控 run-state 根由 governance record 指定（换 output-root
  不能绕过）；campaign 唯一 key 的 claim 目录用原子 ``mkdir(exist_ok=False)``
  建立，engine ``before_start_entry`` 回调在 StartEntry 发送前把 claim 状态推进
  到 ``start-entry-attempted``（是否真的发出不确定也不得自动重试——engine 恰一
  次语义）；已有 claim 一律拒启。DryRun 不建真实 pair claim。
- **governance record 只读**：CLI 不创建任何授权/ID，不写回治理记录；
  ``live_started`` 只是本次运行事实（进本次记录与 claim，不进治理记录）。文件
  hash 绑定不等于人类批准——生产 live 仍必须终端输入
  ``LIVE <完整campaign_id>`` 与 ``OPERATOR-READY`` 两行显式确认。

governance record schema（``n1bdisc-live-governance-record`` v1，封闭键集；
本模块只读，schema 即机器校验契约）::

    {
      "record_type": "n1bdisc-live-governance-record",
      "schema_version": 1,
      "created_local": "YYYY-MM-DD",
      "authorization_id": "AUTH-N1BDISC-PHYS1API26-YYYYMMDD-NNNN",
      "campaign_id":      "N1BDISC-PHYS1API26-YYYYMMDD-NNNN",
      "evidence_id":      "EV-N1BDISC-PHYS1API26-YYYYMMDD-NNNN",
      "code_sha": "<40 位小写 hex；与 manifest.code_sha 绑定>",
      "freeze_manifest_sha256": "<64 位小写 hex；与实际 manifest bytes SHA-256 绑定>",
      "record_status": "approved-unused",   // 读取闭域: approved-unused |
                                            // consumed | retired；仅
                                            // approved-unused 可发起
      "retired_pair_ids": ["N1BDISC-PHYS1API26-...", ...],
          // 必填非空：治理退役 campaign_id 清单（导入的退役事实）；候选 pair
          // 命中即拒。清单与治理现实的覆盖一致性是人工治理审查点，机器只校验
          // 形态、唯一性与「候选不在其中」。
      "run_state_root": "<host 绝对路径；受控 run-state 根，claim 唯一选址>"
    }

claim 文件 schema（run-state 根内 ``claims/<campaign_id>/claim.json``；run-state
事实，不是治理记录；封闭键集）::

    {
      "claim_type": "n1bdisc-live-pair-claim",
      "schema_version": 1,
      "authorization_id": "...", "campaign_id": "...", "evidence_id": "...",
      "code_sha": "<40 hex>", "freeze_manifest_sha256": "<64 hex>",
      "run_state_root": "...", "output_root": "<本次 run 目录>",
      "status": "claimed" | "start-entry-attempted",   // 单向：发送前推进
      "created_at": "<ISO-8601 UTC>", "updated_at": "<ISO-8601 UTC>"
    }

preflight 结果（独立 ``preflight.json`` 落 run 目录随 finish 封签；并作为独立
字段附进 ``--json`` 最终记录；DryRun 不改写 engine 记录 ``integrity={}`` 字面）::

    {
      "preflight_type": "n1bdisc-cli-freeze-preflight",
      "mode": "live" | "dryrun", "ok": <bool>,
      "manifest_path"/"manifest_sha256"/"governance_path"/"governance_sha256",
      "repo_root", "git": {"head_sha256", "dirty", "source"},
      "retired_pair_ids": [...],
      "failures": [{"code", "detail"}, ...]
    }

测试注入（同入口契约）：``main(argv, input_fn=..., git_probe=..., repo_root=...)``
——生产 main 不传这些参数（真实 git / 真实终端 stdin / runner 自在仓库根）。
全部 host-only：本模块不实现任何设备语义，不生成/分配任何 ID，不做签名/提交。
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from typing import Any, Callable, Dict, List, Optional, Tuple

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import n1bdisc_capture as cap                 # noqa: E402
import n1bdisc_engine as engine               # noqa: E402
import n1bdisc_freeze_manifest as fm          # noqa: E402
import n1bdisc_hdc as hdc                     # noqa: E402
import n1bdisc_recording as recording         # noqa: E402
import n1bdisc_run as run_mod                 # noqa: E402
import n1bdisc_transport_real as transport_real  # noqa: E402
import fake_hdc as fake                       # noqa: E402

#: CLI 记录面标识（``--json`` 最终记录 ``cli.cli`` 字段）。
CLI_ID = "n1bdisc-cli/1"

#: 退出码闭域（模块 docstring「退出码」同源）。
EXIT_OK = 0
EXIT_FAIL = 1
EXIT_USAGE = 2
EXIT_REFUSED = 3
EXIT_ERROR = 4

#: 终端确认字面（严格逐字、区分大小写；生产真实来源 = stdin）。
LIVE_CONFIRM_PREFIX = "LIVE "
OPERATOR_READY_LITERAL = "OPERATOR-READY"

#: smoke/正式 dryrun 的本地假执行面（FakeHdc 默认形；target 永不入档，engine 定点脱敏）。
SMOKE_TARGET = "FAKE-TARGET-1"
SMOKE_HAP = "/host/fake/n1bdisc.hap"
#: 无 manifest smoke 的 metadata freeze_hash 占位（明确不绑定任何冻结）。
SMOKE_FREEZE_HASH = "unbound-smoke"

#: run 目录内 preflight 结果文件名（finish 封签清单覆盖它）。
PREFLIGHT_NAME = "preflight.json"

_GIT_TIMEOUT_S = 60.0

# ---------------------------------------------------------------------------
# governance record（只读；封闭 schema 见模块 docstring）
# ---------------------------------------------------------------------------

GOV_RECORD_TYPE = "n1bdisc-live-governance-record"
GOV_SCHEMA_VERSION = 1
#: 读取闭域：仅 approved-unused 可发起；consumed/retired 合法存在但拒启。
GOV_STATUS_APPROVED_UNUSED = "approved-unused"
GOV_STATUS_CONSUMED = "consumed"
GOV_STATUS_RETIRED = "retired"
GOV_STATUS_DOMAIN = (GOV_STATUS_APPROVED_UNUSED, GOV_STATUS_CONSUMED,
                     GOV_STATUS_RETIRED)
_GOV_KEYS = frozenset({
    "record_type", "schema_version", "created_local",
    "authorization_id", "campaign_id", "evidence_id",
    "code_sha", "freeze_manifest_sha256", "record_status",
    "retired_pair_ids", "run_state_root",
})
_RE_DATE_LOCAL = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# ---------------------------------------------------------------------------
# claim（run-state 事实；封闭 schema 见模块 docstring）
# ---------------------------------------------------------------------------

CLAIM_TYPE = "n1bdisc-live-pair-claim"
CLAIM_SCHEMA_VERSION = 1
CLAIMS_DIRNAME = "claims"
CLAIM_FILE_NAME = "claim.json"
CLAIM_STATUS_CLAIMED = "claimed"
CLAIM_STATUS_START_ENTRY_ATTEMPTED = "start-entry-attempted"
CLAIM_STATUS_DOMAIN = (CLAIM_STATUS_CLAIMED,
                       CLAIM_STATUS_START_ENTRY_ATTEMPTED)


class GovernanceError(Exception):
    """governance record 读取/形态失败（带稳定 code + detail；不回显文件负载）。"""

    def __init__(self, code: str, message: str,
                 detail: Optional[Dict[str, Any]] = None) -> None:
        super().__init__("governance-record: %s (%s)" % (code, message))
        self.code = code
        self.message = message
        self.detail = dict(detail or {})


# ---------------------------------------------------------------------------
# 真实来源（生产默认；测试可注入同形替换）
# ---------------------------------------------------------------------------

class GitProbeError(Exception):
    """真实 git 探针失败（无仓库 / git 不可用 / 超时）；preflight 拒启。"""

    def __init__(self, code: str, message: str) -> None:
        super().__init__("git-probe: %s (%s)" % (code, message))
        self.code = code
        self.message = message


def real_git_probe(repo_root: str) -> Tuple[str, bool]:
    """生产 git 探针：``git -C <repo> rev-parse HEAD`` + ``git status --porcelain``。

    返回 ``(head_sha_40hex, dirty)``；dirty = status 输出非空（含未跟踪，gate 1
    :1432 clean HEAD 纪律）。生产唯一来源；失败抛 :class:`GitProbeError`（调用
    方转入 preflight 失败拒启）。只读两命令，零写入、零设备。
    """
    root = os.fspath(repo_root)
    try:
        head = subprocess.run(
            ["git", "-C", root, "rev-parse", "HEAD"],
            capture_output=True, text=True, timeout=_GIT_TIMEOUT_S)
        status = subprocess.run(
            ["git", "-C", root, "status", "--porcelain"],
            capture_output=True, text=True, timeout=_GIT_TIMEOUT_S)
    except (OSError, subprocess.SubprocessError) as exc:
        raise GitProbeError("git-probe-failed", repr(type(exc).__name__)) from exc
    if head.returncode != 0:
        raise GitProbeError("git-probe-failed", "rev-parse HEAD failed")
    if status.returncode != 0:
        raise GitProbeError("git-probe-failed", "status --porcelain failed")
    sha = head.stdout.strip()
    if not fm.RE_CODE_SHA.match(sha):
        raise GitProbeError("git-probe-invalid-head", "HEAD 不是 40 位小写 hex")
    return sha, bool(status.stdout.strip())


def default_input(prompt: str) -> str:
    """生产确认输入：终端 stdin 一行。"""
    return input(prompt)


def default_repo_root() -> str:
    """runner 自在仓库根（``<repo>/spikes/n1b-disc-phys-hap/runner`` 上溯三级）。"""
    runner_dir = os.path.dirname(os.path.abspath(__file__))
    return os.path.dirname(os.path.dirname(os.path.dirname(runner_dir)))


# ---------------------------------------------------------------------------
# governance record 读取与形态校验（只读；返回 (dict, failures) 或抛错）
# ---------------------------------------------------------------------------

def load_governance_record(path: str, *, expected_sha256: str) -> Dict[str, Any]:
    """读 governance record 并绑定外部期望 hash（形态同 ``fm.load_manifest`` 门）。"""
    if not os.path.isfile(path):
        raise GovernanceError("governance-not-found", "governance 文件不存在",
                              {"path": path})
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except OSError as exc:
        raise GovernanceError("governance-unreadable", "governance 读取失败",
                              {"path": path, "error": str(exc)}) from exc
    actual = hashlib.sha256(raw).hexdigest()
    if not (isinstance(expected_sha256, str)
            and fm.RE_SHA256.match(expected_sha256)):
        raise GovernanceError(
            "expected-governance-sha256-format",
            "governance-sha256 必须为完整 64 位小写 hex",
            {"expected_governance_sha256": str(expected_sha256)})
    if actual != expected_sha256:
        raise GovernanceError(
            "governance-sha256-mismatch",
            "governance 实际 bytes SHA-256 与外部期望不符",
            {"expected_governance_sha256": expected_sha256,
             "actual_governance_sha256": actual})
    try:
        data = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, ValueError) as exc:
        raise GovernanceError("governance-invalid-json",
                              "governance 不是合法 UTF-8 JSON",
                              {"path": path, "error": str(exc)}) from exc
    if not isinstance(data, dict):
        raise GovernanceError("governance-root-type",
                              "governance 顶层必须为 JSON object", {"path": path})
    return {"data": data, "governance_sha256": actual}


def _gov_fail(out: List[Dict[str, Any]], code: str, **detail: Any) -> None:
    out.append({"code": code, "detail": detail})


def validate_governance(data: Any) -> List[Dict[str, Any]]:
    """封闭 schema 形态校验（不触 manifest 绑定；绑定在 preflight 内做）。"""
    out: List[Dict[str, Any]] = []
    if not isinstance(data, dict):
        _gov_fail(out, "governance-root-type", expected="object")
        return out
    got = frozenset(data.keys())
    if got != _GOV_KEYS:
        _gov_fail(out, "governance-keys", expected=sorted(_GOV_KEYS),
                  actual=sorted(got), unknown=sorted(got - _GOV_KEYS),
                  missing=sorted(_GOV_KEYS - got))
        return out
    if data["record_type"] != GOV_RECORD_TYPE:
        _gov_fail(out, "governance-record-type", expected=GOV_RECORD_TYPE,
                  actual=data["record_type"])
    if data["schema_version"] != GOV_SCHEMA_VERSION \
            or isinstance(data["schema_version"], bool) \
            or not isinstance(data["schema_version"], int):
        _gov_fail(out, "governance-schema-version-unknown",
                  expected=GOV_SCHEMA_VERSION, actual=data["schema_version"])
    created = data["created_local"]
    if not (isinstance(created, str) and _RE_DATE_LOCAL.match(created)):
        _gov_fail(out, "governance-created-local-format",
                  expected="YYYY-MM-DD", actual=created)
    else:
        try:
            _dt.date.fromisoformat(created)
        except ValueError:
            _gov_fail(out, "governance-created-local-format",
                      expected="真实日历日期", actual=created)
    for key, rx in (("authorization_id", fm.RE_AUTH_ID),
                    ("campaign_id", fm.RE_CAMPAIGN_ID),
                    ("evidence_id", fm.RE_EVIDENCE_ID)):
        v = data[key]
        if not (isinstance(v, str) and rx.match(v)):
            _gov_fail(out, "governance-id-format", field=key,
                      expected=rx.pattern, actual=v)
    if not (isinstance(data["code_sha"], str)
            and fm.RE_CODE_SHA.match(data["code_sha"])):
        _gov_fail(out, "governance-code-sha-format",
                  expected="40 位小写 hex git commit", actual=data["code_sha"])
    if not (isinstance(data["freeze_manifest_sha256"], str)
            and fm.RE_SHA256.match(data["freeze_manifest_sha256"])):
        _gov_fail(out, "governance-freeze-sha-format",
                  expected="64 位小写 hex SHA-256",
                  actual=data["freeze_manifest_sha256"])
    if data["record_status"] not in GOV_STATUS_DOMAIN:
        _gov_fail(out, "governance-status-unknown", expected=list(GOV_STATUS_DOMAIN),
                  actual=data["record_status"])
    retired = data["retired_pair_ids"]
    if not isinstance(retired, list) or not retired:
        _gov_fail(out, "governance-retired-required",
                  note="retired_pair_ids 必填且为非空列表"
                       "（治理退役 campaign_id 导入清单）",
                  actual=retired)
    else:
        for entry in retired:
            if not (isinstance(entry, str) and fm.RE_CAMPAIGN_ID.match(entry)):
                _gov_fail(out, "governance-retired-format",
                          expected=fm.RE_CAMPAIGN_ID.pattern, actual=entry)
        if len(set(retired)) != len(retired):
            _gov_fail(out, "governance-retired-duplicate",
                      duplicates=sorted({x for x in retired
                                         if not isinstance(x, str)
                                         or retired.count(x) > 1}))
    root = data["run_state_root"]
    if not (isinstance(root, str) and os.path.isabs(root)
            and os.path.normpath(root) == root):
        _gov_fail(out, "governance-run-state-root-invalid",
                  note="必须为规范化 host 绝对路径", actual=root)
    return out


# ---------------------------------------------------------------------------
# preflight（live 与正式 dryrun 共用；全部在任何 transport/hdc 之前）
# ---------------------------------------------------------------------------

def run_preflight(*, mode: str, manifest_path: str, manifest_sha256: str,
                  governance_path: str, governance_sha256: str,
                  repo_root: str, git_probe: Callable[[str], Tuple[str, bool]],
                  output_root: Optional[str] = None,
                  live: bool = False) -> Dict[str, Any]:
    """冻结输入预检：manifest 外部 hash 绑定 → governance 形态/绑定 → 真实 git
    → ``fm.verify_inputs``（runner 精确集合/artifacts/so/confirmation/retired）
    → live 附加 claim 占用与 output-root 冲突检查。返回 JSON 可序列化报告；
    ``ok=False`` 时 ``failures`` 为稳定 code 清单。**零写盘、零 hdc、零消费**。
    """
    report: Dict[str, Any] = {
        "preflight_type": "n1bdisc-cli-freeze-preflight",
        "mode": mode,
        "ok": False,
        "manifest_path": manifest_path,
        "manifest_sha256": manifest_sha256,
        "manifest_code_sha": None,
        "governance_path": governance_path,
        "governance_sha256": governance_sha256,
        "repo_root": os.fspath(repo_root),
        "git": {"head_sha256": None, "dirty": None, "source": None},
        "retired_pair_ids": None,
        "run_state_root": None,
        "failures": [],
    }
    failures: List[Dict[str, Any]] = report["failures"]

    def fail(code: str, **detail: Any) -> None:
        failures.append({"code": code, "detail": detail})

    # 1) manifest load（外部 hash 绑定）
    try:
        manifest = fm.load_manifest(manifest_path,
                                    expected_manifest_sha256=manifest_sha256)
    except fm.ManifestError as exc:
        fail(exc.code, message=exc.message, **({"detail": exc.detail}
                                               if exc.detail else {}))
        return report
    report["manifest_code_sha"] = manifest.data["code_sha"]

    # 2) governance load（外部 hash 绑定）+ 形态校验
    governance: Optional[Dict[str, Any]] = None
    try:
        governance = load_governance_record(
            governance_path, expected_sha256= governance_sha256)["data"]
    except GovernanceError as exc:
        fail(exc.code, message=exc.message, **({"detail": exc.detail}
                                               if exc.detail else {}))
        return report
    failures.extend(validate_governance(governance))
    if any(f["code"].startswith("governance-") for f in failures):
        return report
    report["run_state_root"] = governance["run_state_root"]

    # 3) governance ↔ manifest 三重绑定（三 ID / code_sha / freeze sha）
    pair = manifest.data["pair"]
    for key in ("authorization_id", "campaign_id", "evidence_id"):
        if governance[key] != pair[key]:
            fail("governance-pair-mismatch", field=key,
                 manifest=pair[key], governance=governance[key])
    if governance["code_sha"] != manifest.data["code_sha"]:
        fail("governance-code-sha-mismatch",
             manifest_code_sha=manifest.data["code_sha"],
             governance_code_sha=governance["code_sha"])
    if governance["freeze_manifest_sha256"] != manifest.manifest_sha256:
        fail("governance-freeze-sha-mismatch",
             manifest_sha256=manifest.manifest_sha256,
             governance_freeze_sha256=governance["freeze_manifest_sha256"])
    # 4) 治理状态：候选 pair 未退役 / 记录未消费
    retired = sorted(set(str(x) for x in governance["retired_pair_ids"]))
    report["retired_pair_ids"] = retired
    if pair["campaign_id"] in retired:
        fail("governance-pair-retired", campaign_id=pair["campaign_id"],
             note="候选 pair 在 governance 退役清单内")
    if governance["record_status"] != GOV_STATUS_APPROVED_UNUSED:
        fail("governance-status-not-usable",
             expected=GOV_STATUS_APPROVED_UNUSED,
             actual=governance["record_status"])
    # 5) 受控 run-state 根 + live 附加检查（只读探测，不消费）
    if not os.path.isdir(governance["run_state_root"]):
        fail("governance-run-state-root-missing",
             run_state_root=governance["run_state_root"])
    elif live:
        if os.path.exists(claim_key_path(governance["run_state_root"],
                                         pair["campaign_id"])):
            fail("pair-claim-exists", campaign_id=pair["campaign_id"],
                 run_state_root=governance["run_state_root"],
                 note="同 pair 单次性：已有 claim 一律拒启"
                      "（换 output-root 不能绕过）")
    if output_root is not None and os.path.exists(output_root):
        fail("output-root-exists", output_root=output_root,
             note="run 目录必须为 NEWDIR（单次使用，绝不覆盖）")
    if failures:
        return report

    # 6) 真实（或注入）git 探针 → gate 1 绑定
    probe_source = ("git-rev-parse+status" if git_probe is real_git_probe
                    else "injected-probe")
    try:
        head_sha, dirty = git_probe(repo_root)
    except GitProbeError as exc:
        report["git"]["source"] = probe_source
        fail(exc.code, message=exc.message)
        return report
    report["git"] = {"head_sha256": head_sha, "dirty": bool(dirty),
                     "source": probe_source}

    # 7) manifest 完整机器核验（结构/clean HEAD/runner 精确集合/artifacts/so/
    #    confirmation/retired 命中；纯读取）
    verify = fm.verify_inputs(manifest, repo_root=repo_root,
                              current_code_sha=head_sha,
                              current_dirty=bool(dirty),
                              retired_pair_ids=retired)
    failures.extend(f.as_dict() for f in verify.failures)

    # 8) live 附加：hdc 可执行文件可用性（构造 RealHdcTransport 的前置事实）
    if live and verify.ok:
        artifacts = {e["role"]: e["path"] for e in manifest.data["artifacts"]}
        hdc_path = artifacts.get("hdc_binary")
        if not (hdc_path and os.path.isfile(hdc_path)
                and os.access(hdc_path, os.X_OK)):
            fail("hdc-binary-not-usable", path=hdc_path,
                 note="manifest 绑定 hdc_binary 必须存在且可执行"
                      "（绝不 PATH 查找）")
    report["ok"] = not failures
    return report


# ---------------------------------------------------------------------------
# claim 原子语义（run-state 根内；campaign 唯一 key）
# ---------------------------------------------------------------------------

def claim_key_path(run_state_root: str, campaign_id: str) -> str:
    """campaign 唯一 claim key（run_state_root/claims/<campaign_id>）。"""
    return os.path.join(os.fspath(run_state_root), CLAIMS_DIRNAME, campaign_id)


def _now_iso_utc() -> str:
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def _write_claim(path: str, payload: Dict[str, Any]) -> None:
    """claim.json 原子写（临时文件 + fsync + os.replace；0600/0700）。"""
    tmp = path + ".tmp"
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(payload, ensure_ascii=False, sort_keys=True,
                            indent=1) + "\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, path)


def read_claim(claim_dir: str) -> Dict[str, Any]:
    with open(os.path.join(claim_dir, CLAIM_FILE_NAME), "r",
              encoding="utf-8") as fh:
        return json.load(fh)


def create_claim(run_state_root: str, *, authorization_id: str,
                 campaign_id: str, evidence_id: str, code_sha: str,
                 freeze_manifest_sha256: str, output_root: str) -> str:
    """原子建立 campaign 唯一 claim（``mkdir exist_ok=False`` 即互斥点）。

    返回 claim 目录；已存在抛 :class:`FileExistsError`（调用方拒启，不消费二
    次）。claim 只建在 governance 指定的受控 run-state 根内——换 output-root
    不能绕过单次性。
    """
    claims_dir = os.path.join(os.fspath(run_state_root), CLAIMS_DIRNAME)
    os.makedirs(claims_dir, mode=0o700, exist_ok=True)
    key_dir = claim_key_path(run_state_root, campaign_id)
    os.mkdir(key_dir, mode=0o700)                     # 原子互斥：唯一性断点
    now = _now_iso_utc()
    payload: Dict[str, Any] = {
        "claim_type": CLAIM_TYPE,
        "schema_version": CLAIM_SCHEMA_VERSION,
        "authorization_id": authorization_id,
        "campaign_id": campaign_id,
        "evidence_id": evidence_id,
        "code_sha": code_sha,
        "freeze_manifest_sha256": freeze_manifest_sha256,
        "run_state_root": os.fspath(run_state_root),
        "output_root": output_root,
        "status": CLAIM_STATUS_CLAIMED,
        "created_at": now,
        "updated_at": now,
    }
    _write_claim(os.path.join(key_dir, CLAIM_FILE_NAME), payload)
    return key_dir


def mark_claim_start_entry_attempted(claim_dir: str) -> None:
    """engine ``before_start_entry`` 挂点：发送前把 claim 推进到
    ``start-entry-attempted``（是否真发出不确定也不得自动重试；单向推进）。"""
    path = os.path.join(claim_dir, CLAIM_FILE_NAME)
    payload = read_claim(claim_dir)
    if payload.get("status") != CLAIM_STATUS_CLAIMED:
        raise RuntimeError("claim status %r is not %r (no-retry single-use)"
                           % (payload.get("status"), CLAIM_STATUS_CLAIMED))
    payload["status"] = CLAIM_STATUS_START_ENTRY_ATTEMPTED
    payload["updated_at"] = _now_iso_utc()
    _write_claim(path, payload)


# ---------------------------------------------------------------------------
# dryrun 本地执行面：FakeHdc 的 engine 适配（read_event 流契约；EOF 收口）
# ---------------------------------------------------------------------------

class _SmokeStream:
    """FakeHdc 合成流 → engine ``read_event`` 契约适配（耗尽 → eof(0)）。"""

    def __init__(self, generator) -> None:
        self._gen = generator
        self._eof = False

    def read_event(self, timeout_s: Optional[float]):
        if self._eof:
            return transport_real.HdcStreamEvent("eof", None, 0)
        try:
            line = next(self._gen)
        except StopIteration:
            self._eof = True
            return transport_real.HdcStreamEvent("eof", None, 0)
        return transport_real.HdcStreamEvent("line", line, None)

    def close(self) -> None:
        self._eof = True   # 幂等；合成流无子进程可回收


class _SmokeTransport(hdc.HdcTransport):
    """``--dryrun`` 的 fake transport 适配（dryrun×fake 配对，engine 强制）。

    call 语义原样；FaultRecv 补真实 file recv 的 host 落盘副作用（engine 从
    host 文件读取 fault 原文）；``open_stream`` 包成有界 read_event 流。
    """

    def __init__(self, scenario: fake.FakeScenario, target: str,
                 hap_path: str) -> None:
        self.inner = fake.FakeHdc(scenario, target=target, hap_path=hap_path)

    @property
    def target(self) -> str:
        return self.inner.target

    @property
    def hap_path(self) -> str:
        return self.inner.hap_path

    def call(self, argv):
        op = self.inner._validate(argv)
        result = self.inner.call(argv)
        if result.exit_code == 0 and op == "FaultRecv":
            name = argv[4].rsplit("/", 1)[-1]
            content = self.inner.received.get(name)
            if content is not None:
                os.makedirs(os.path.dirname(argv[5]), exist_ok=True)
                with open(argv[5], "w", encoding="utf-8") as fh:
                    fh.write(content)
        return result

    def open_stream(self, argv):
        return _SmokeStream(self.inner.open_stream(argv))


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _sha256_hex_arg(value: str) -> str:
    if not (isinstance(value, str) and fm.RE_SHA256.match(value)):
        raise argparse.ArgumentTypeError(
            "必须是完整 64 位小写 hex SHA-256（不接受省略/截断/大写）")
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="n1bdisc_cli",
        description="N1BDISC host runner CLI（--live 冻结门 / --dryrun engine；"
                    "详见模块 docstring）")
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--live", action="store_true",
                      help="生产 Live：完整冻结四件套 + 终端双重确认；"
                           "preflight 全过才构造 RealHdcTransport")
    mode.add_argument("--dryrun", action="store_true",
                      help="DryRun：带冻结四件套 = 正式 freeze-bound（gate 11 "
                           "自查）；不带 = 离线 smoke（不具门 11 资格）")
    parser.add_argument("--target", default=None,
                        help="live 必填；运行时内存参数，只传 executor，"
                             "不打印/不入档")
    parser.add_argument("--hap", default=None,
                        help="仅 smoke dryrun 可选（fake 执行面 HAP 路径）；"
                             "live 的 HAP 由 manifest role=signed_hap 绑定")
    parser.add_argument("--scenario", choices=run_mod.SCENARIOS, default=None,
                        help="dryrun 剧本（缺省 happy）；live 拒绝")
    parser.add_argument("--output-root", default=None,
                        help="run 目录（NEWDIR，单次使用）；live 与正式 "
                             "dryrun 必填；smoke 缺省进系统临时目录")
    parser.add_argument("--freeze-manifest", default=None,
                        help="候选 ready-freeze manifest JSON 路径")
    parser.add_argument("--freeze-sha256", default=None, type=_sha256_hex_arg,
                        help="manifest 实际 bytes SHA-256（64 位小写 hex）")
    parser.add_argument("--governance-record", default=None,
                        help="governance record JSON 路径（schema 见 docstring）")
    parser.add_argument("--governance-sha256", default=None,
                        type=_sha256_hex_arg,
                        help="governance 实际 bytes SHA-256（64 位小写 hex）")
    parser.add_argument("--json", default=None,
                        help="最终完整记录 JSON 另存路径（附带输出；不含 "
                             "target；不取代增量 run 目录）")
    return parser


def _validate_args(parser: argparse.ArgumentParser, args) -> None:
    """跨参数形态门（parser.error → exit 2；任何模式不触 preflight）。"""
    freeze_args = (args.freeze_manifest, args.freeze_sha256,
                   args.governance_record, args.governance_sha256)
    given = sum(1 for v in freeze_args if v is not None)
    if args.live:
        if given != 4:
            parser.error("--live 必须同时提供 --freeze-manifest/--freeze-sha256/"
                         "--governance-record/--governance-sha256（完整四件套）")
        if not args.target:
            parser.error("--live 必须提供 --target（运行时内存参数）")
        if args.output_root is None:
            parser.error("--live 必须提供 --output-root（NEWDIR）")
        if args.scenario is not None:
            parser.error("--live 不接受 --scenario")
        if args.hap is not None:
            parser.error("--live 不接受 --hap（HAP 由 manifest "
                         "role=signed_hap 绑定）")
        return
    # --dryrun：冻结四件套要么全给（正式 freeze-bound）要么全不给（smoke）
    if given not in (0, 4):
        parser.error("--dryrun 的冻结四件套必须全部提供或全部省略"
                     "（部分提供 = 拒绝；无 manifest smoke 必须明确标注）")
    if given == 4:
        if args.output_root is None:
            parser.error("正式 --dryrun（freeze-bound）必须提供 --output-root")
        if args.target is not None or args.hap is not None:
            parser.error("正式 --dryrun 不接触设备，不接受 --target/--hap")


def _print_summary(payload: Dict[str, Any]) -> None:
    """控制台唯一输出形态：单行无敏 JSON（run-root/阶段/失败摘要；无 target）。"""
    print(json.dumps(payload, ensure_ascii=False, sort_keys=True))


def _write_json_maybe(path: Optional[str], doc: Dict[str, Any]) -> None:
    if path is None:
        return
    try:
        parent = os.path.dirname(os.path.abspath(path))
        os.makedirs(parent, exist_ok=True)
        with open(path, "w", encoding="utf-8") as fh:
            fh.write(json.dumps(doc, ensure_ascii=False, indent=2,
                                sort_keys=True) + "\n")
    except OSError as exc:
        # 附带输出不可写不改变主流程结果（拒启仍 exit 3；已封签 run 不受影响），
        # 事实在 stderr 登记，不默默吞掉。
        sys.stderr.write("n1bdisc_cli: --json write failed: %s\n" % exc)


def _refuse(*, mode: str, reason: str, report: Optional[Dict[str, Any]] = None,
            json_path: Optional[str] = None) -> int:
    """统一拒启面：单行摘要 + 可选 --json 拒启记录；exit 3（零 hdc/零消费）。"""
    doc: Dict[str, Any] = {"cli": CLI_ID, "mode": mode, "outcome": "refused",
                           "refused": True, "reason": reason}
    if report is not None:
        doc["failures"] = [f["code"] for f in report.get("failures", [])]
        doc["preflight"] = report
    else:
        doc["failures"] = [reason]
    _write_json_maybe(json_path, doc)
    _print_summary(doc)
    return EXIT_REFUSED


def _artifact_path(manifest: fm.FreezeManifest, role: str) -> str:
    return next(e["path"] for e in manifest.data["artifacts"]
                if e["role"] == role)


def make_freeze_revalidate(*, manifest_path: str, manifest_sha256: str,
                           governance_path: str, governance_sha256: str,
                           repo_root: str,
                           git_probe: Callable[[str], Tuple[str, bool]],
                           retired_pair_ids: List[str]
                           ) -> Callable[[], Dict[str, Any]]:
    """构造 live freeze 收口复核回调（engine host finally 步 10 恰一次调用）。

    只读、零设备、零写盘；复用既有 ``fm.load_manifest(expected hash)`` /
    :func:`load_governance_record` / ``fm.verify_inputs``，不手写第二套哈希
    逻辑。expected hash 一律取 CLI 预检时经校验的**外部字面**（本回调不重读
    被改后的 hash 自证）；逐项复核：

    - manifest / governance bytes 的外部 expected hash（漂移 → 稳定失败码）；
    - 真实（或同形注入）git 探针 code_sha / dirty；
    - manifest ↔ governance 绑定（三 ID / code_sha / freeze sha）与治理状态；
    - ``fm.verify_inputs``：runner 文件集精确覆盖与逐文件 hash、artifact 完整
      hash、HAP ``.so`` 成员、target-binding confirmation、retired 命中。

    返回载荷（engine ``record.integrity`` 的数据面；失败码 = 既有 preflight/
    verify 稳定 code 的 revalidate 前缀形态，**不新造判据/cause**）::

        {"passed": <bool>,                    # 真实结论（非「无抛异常」）
         "manifest_sha256": <64hex | None>,   # postcheck 时观测的 bytes hash
         "governance_sha256": <64hex | None>,
         "precheck": "passed",                # 预检不通过走拒启，不进本回调
         "violations": ["<稳定失败码>", ...]}  # 非空 → verdict invalid 优先轴

    回调自身不抛错（内部全捕获转为失败码；engine 侧另有兜底）。载荷只含路径、
    hash 与稳定 code，不含 target/密码/argv。
    """

    def revalidate() -> Dict[str, Any]:
        violations: List[str] = []
        manifest: Optional[fm.FreezeManifest] = None
        manifest_sha: Optional[str] = None
        try:
            manifest = fm.load_manifest(manifest_path,
                                        expected_manifest_sha256=manifest_sha256)
            manifest_sha = manifest.manifest_sha256
        except fm.ManifestError as exc:
            violations.append("manifest-revalidate: %s" % exc.code)
            observed = (exc.detail or {}).get("actual_manifest_sha256")
            if isinstance(observed, str) and fm.RE_SHA256.match(observed):
                manifest_sha = observed   # 漂移 bytes 的观测 hash（证据，非授权）
        governance: Optional[Dict[str, Any]] = None
        governance_sha: Optional[str] = None
        try:
            gov_loaded = load_governance_record(
                governance_path, expected_sha256=governance_sha256)
            governance = gov_loaded["data"]
            governance_sha = gov_loaded["governance_sha256"]
        except GovernanceError as exc:
            violations.append("governance-revalidate: %s" % exc.code)
            observed = (exc.detail or {}).get("actual_governance_sha256")
            if isinstance(observed, str) and fm.RE_SHA256.match(observed):
                governance_sha = observed
        head_sha: Optional[str] = None
        dirty: Optional[bool] = None
        try:
            head_sha, dirty = git_probe(repo_root)
        except GitProbeError as exc:
            violations.append("git-revalidate: %s" % exc.code)
        if manifest is not None and governance is not None:
            pair = manifest.data["pair"]
            if governance.get("record_status") != GOV_STATUS_APPROVED_UNUSED:
                violations.append(
                    "governance-revalidate: governance-status-not-usable")
            if governance.get("code_sha") != manifest.data["code_sha"]:
                violations.append(
                    "governance-revalidate: governance-code-sha-mismatch")
            if governance.get("freeze_manifest_sha256") \
                    != manifest.manifest_sha256:
                violations.append(
                    "governance-revalidate: governance-freeze-sha-mismatch")
            for key in ("authorization_id", "campaign_id", "evidence_id"):
                if governance.get(key) != pair.get(key):
                    violations.append(
                        "governance-revalidate: governance-pair-mismatch(%s)"
                        % key)
        if manifest is not None and head_sha is not None \
                and dirty is not None:
            verify = fm.verify_inputs(manifest, repo_root=repo_root,
                                      current_code_sha=head_sha,
                                      current_dirty=bool(dirty),
                                      retired_pair_ids=retired_pair_ids)
            violations.extend("verify: %s" % f.code for f in verify.failures)
        return {"passed": not violations, "manifest_sha256": manifest_sha,
                "governance_sha256": governance_sha, "precheck": "passed",
                "violations": violations}

    return revalidate


def _attach_cli_fields(record: Dict[str, Any], *, mode: str,
                       freeze_bound: bool, gate11_eligible: Optional[bool],
                       scenario: Optional[str], preflight: Optional[Dict[str, Any]],
                       claim_status: Optional[str], live_started: bool
                       ) -> Dict[str, Any]:
    """CLI 附带字段（进 --json 最终记录；engine result.json 字面不动）。"""
    record["cli"] = {
        "cli": CLI_ID,
        "mode": mode,
        "freeze_bound": freeze_bound,
        "gate11_eligible": gate11_eligible,
        "scenario": scenario,
        "claim_status": claim_status,
        "live_started": live_started,
    }
    if preflight is not None:
        record["preflight"] = preflight
    return record


def _write_preflight_file(recorder: recording.RunRecorder,
                          report: Dict[str, Any]) -> None:
    """preflight.json 落 run 目录（先于 engine；随 finish sha256 清单封签）。"""
    path = os.path.join(recorder.root, PREFLIGHT_NAME)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write(json.dumps(report, ensure_ascii=False, indent=1,
                            sort_keys=True) + "\n")
        fh.flush()
        os.fsync(fh.fileno())


def _ask_line(ask: Callable[[str], str], prompt: str) -> str:
    """确认输入一行：prompt 走 stderr（stdout 保持机器可读的单行无敏摘要）。"""
    sys.stderr.write(prompt)
    sys.stderr.flush()
    return (ask("") or "").strip()


def _cmd_live(args, *, ask: Callable[[str], str],
              probe: Callable[[str], Tuple[str, bool]],
              repo_root: str) -> int:
    """生产 Live：preflight → 双重终端确认 → transport（stat-only）→ 原子 claim
    → RunRecorder → preflight.json → engine（before_start_entry 推进 claim；
    revalidate_freeze 收口复核回调 → record.integrity + invalid 轴）。
    """
    report = run_preflight(
        mode="live", manifest_path=args.freeze_manifest,
        manifest_sha256=args.freeze_sha256,
        governance_path=args.governance_record,
        governance_sha256=args.governance_sha256, repo_root=repo_root,
        git_probe=probe, output_root=args.output_root, live=True)
    if not report["ok"]:
        return _refuse(mode="live", reason="preflight-failed", report=report,
                       json_path=args.json)
    manifest = None
    try:
        manifest = fm.load_manifest(args.freeze_manifest,
                                    expected_manifest_sha256=args.freeze_sha256)
    except fm.ManifestError as exc:
        # preflight 后字节漂移（外部 hash 绑定失败）：任何 transport 之前拒启。
        return _refuse(mode="live", reason=exc.code, json_path=args.json)
    pair = manifest.data["pair"]
    campaign_id = pair["campaign_id"]

    # 显式人类确认（文件 hash 绑定不算批准）：两行逐字，失败即拒启（零消费）。
    try:
        confirm = _ask_line(ask, "LIVE-confirm> ")
    except EOFError:
        confirm = ""
    if confirm != LIVE_CONFIRM_PREFIX + campaign_id:
        return _refuse(mode="live", reason="live-confirmation-mismatch",
                       json_path=args.json)
    try:
        ready = _ask_line(ask, "operator-ready> ")
    except EOFError:
        ready = ""
    if ready != OPERATOR_READY_LITERAL:
        return _refuse(mode="live", reason="operator-ready-not-confirmed",
                       json_path=args.json)

    # transport 构造（只做 stat 校验，零设备操作；路径 = manifest 绑定，
    # preflight 已核存在+可执行）。target 仅内存。
    hdc_command = _artifact_path(manifest, "hdc_binary")
    hap_path = _artifact_path(manifest, "signed_hap")
    try:
        transport = transport_real.RealHdcTransport(hdc_command)
    except transport_real.HdcTransportError:
        return _refuse(mode="live", reason="hdc-binary-not-usable",
                       json_path=args.json)

    # 原子 claim（唯一互斥点；governance 指定的受控 run-state 根，换
    # output-root 不能绕过单次性）。
    try:
        claim_dir = create_claim(
            report["run_state_root"],
            authorization_id=pair["authorization_id"],
            campaign_id=campaign_id, evidence_id=pair["evidence_id"],
            code_sha=manifest.data["code_sha"],
            freeze_manifest_sha256=manifest.manifest_sha256,
            output_root=args.output_root)
    except FileExistsError:
        return _refuse(mode="live", reason="pair-claim-exists",
                       json_path=args.json)

    try:
        recorder = recording.RunRecorder(
            args.output_root, mode="live",
            authorisation_id=pair["authorization_id"],
            campaign_id=campaign_id, evidence_id=pair["evidence_id"],
            code_sha=manifest.data["code_sha"],
            freeze_hash=manifest.manifest_sha256)
    except recording.RecordingError as exc:
        _print_summary({"cli": CLI_ID, "mode": "live",
                        "outcome": "error", "phase": "recorder-init",
                        "error": str(exc),
                        "note": "claim 已建立（operator-ready 已确认即消费；"
                                "不回滚）"})
        return EXIT_ERROR
    _write_preflight_file(recorder, report)

    executor = hdc.HdcExecutor(transport, target=args.target, hap_path=hap_path)
    # live 收口复核回调（engine host finally 步 10 调用；expected hash 取预检
    # 校验过的外部字面；retired 名单 = preflight 读定的治理事实）。
    revalidate = make_freeze_revalidate(
        manifest_path=args.freeze_manifest, manifest_sha256=args.freeze_sha256,
        governance_path=args.governance_record,
        governance_sha256=args.governance_sha256, repo_root=repo_root,
        git_probe=probe, retired_pair_ids=report["retired_pair_ids"])
    try:
        record = engine.run_campaign(
            executor=executor, target=args.target, hap_path=hap_path,
            clock=cap.MonoWallClock(), recorder=recorder, operator_ready=True,
            mode="live",
            before_start_entry=lambda: mark_claim_start_entry_attempted(
                claim_dir),
            revalidate_freeze=revalidate)
    except Exception as exc:  # noqa: BLE001 — 启动后错误面；recorder 尽力补 incomplete
        _print_summary({"cli": CLI_ID, "mode": "live", "outcome": "error",
                        "phase": "engine", "run_root": recorder.root,
                        "error": engine.redact_text(repr(type(exc).__name__),
                                                    args.target)})
        return EXIT_ERROR
    claim_status = read_claim(claim_dir)["status"]
    _attach_cli_fields(record, mode="live", freeze_bound=True,
                       gate11_eligible=None, scenario=None, preflight=report,
                       claim_status=claim_status, live_started=True)
    _write_json_maybe(args.json, record)
    _print_summary({"cli": CLI_ID, "mode": "live", "outcome": "completed",
                    "verdict": record["verdict"], "terminal": record["terminal"],
                    "run_root": recorder.root, "freeze_bound": True,
                    "claim_status": claim_status, "live_started": True})
    return EXIT_OK if record["verdict"] == "pass" else EXIT_FAIL


def _run_dryrun_engine(*, scenario_name: str, target: str, hap_path: str,
                       output_root: str, freeze_bound: bool,
                       gate11_eligible: bool,
                       preflight: Optional[Dict[str, Any]],
                       freeze_hash: str, metadata: Dict[str, str],
                       json_path: Optional[str]) -> int:
    """``--dryrun`` 共同路径：FakeHdc 适配 + 同一 ``run_campaign`` engine。

    ``freeze_bound=False``（离线 smoke）时明确 ``gate11_eligible=False``，不声称
    正式门 11；``freeze_bound=True``（正式）携带同一冻结输入预检结果（独立
    preflight 字段/文件），最终记录保持 ``is_evidence=false`` / ``integrity={}``
    （engine 原字面）。DryRun 不建真实 pair claim（零消费）。
    """
    builders = {
        "happy": fake.make_happy_path_scenario,
        "pre-only": fake.make_pre_only_scenario,
        "no-live-fd": fake.make_no_live_fd_scenario,
        "foreign-packet": fake.make_foreign_packet_scenario,
        "bad-payload": fake.make_bad_payload_scenario,
        "not-attempted-timeout": fake.make_not_attempted_timeout_scenario,
        "join-bogus": fake.make_join_bogus_scenario,
        "d8b-bogus": fake.make_d8b_bogus_scenario,
    }
    scenario = builders[scenario_name]()
    transport = _SmokeTransport(scenario, target=target, hap_path=hap_path)
    executor = hdc.HdcExecutor(transport, target=target, hap_path=hap_path)
    try:
        recorder = recording.RunRecorder(
            output_root, mode="dryrun",
            authorisation_id=metadata["authorisation_id"],
            campaign_id=metadata["campaign_id"],
            evidence_id=metadata["evidence_id"],
            code_sha=metadata["code_sha"],
            freeze_hash=freeze_hash)
    except recording.RecordingError as exc:
        # 已存在 output-root（smoke 无 preflight 检查）等记录面拒绝。
        _print_summary({"cli": CLI_ID, "mode": "dryrun", "outcome": "error",
                        "phase": "recorder-init", "error": str(exc)})
        return EXIT_ERROR
    if preflight is not None:
        _write_preflight_file(recorder, preflight)
    try:
        record = engine.run_campaign(
            executor=executor, target=target, hap_path=hap_path,
            clock=cap.MonoWallClock(), recorder=recorder, operator_ready=True,
            mode="dryrun")
    except Exception as exc:  # noqa: BLE001
        _print_summary({"cli": CLI_ID, "mode": "dryrun", "outcome": "error",
                        "phase": "engine", "run_root": recorder.root,
                        "error": engine.redact_text(repr(type(exc).__name__),
                                                    target)})
        return EXIT_ERROR
    _attach_cli_fields(record, mode="dryrun", freeze_bound=freeze_bound,
                       gate11_eligible=gate11_eligible, scenario=scenario_name,
                       preflight=preflight, claim_status=None,
                       live_started=False)
    _write_json_maybe(json_path, record)
    _print_summary({"cli": CLI_ID, "mode": "dryrun", "outcome": "completed",
                    "verdict": record["verdict"], "terminal": record["terminal"],
                    "run_root": recorder.root, "freeze_bound": freeze_bound,
                    "gate11_eligible": gate11_eligible,
                    "scenario": scenario_name})
    return EXIT_OK if record["verdict"] == "pass" else EXIT_FAIL


_SMOKE_METADATA = {
    "authorisation_id": "CLI-SMOKE-AUTH-N1BDISC-0000",
    "campaign_id": "CLI-SMOKE-CAMPAIGN-0000",
    "evidence_id": "CLI-SMOKE-EV-0000",
    "code_sha": "unbound-smoke",
}


def _cmd_dryrun(args, *, probe: Callable[[str], Tuple[str, bool]],
                repo_root: str) -> int:
    freeze_bound = args.freeze_manifest is not None
    if not freeze_bound:
        # 离线 smoke：明确不具门 11 资格；仍走同一 engine（不触旧平行模拟器）。
        output_root = args.output_root or tempfile.mkdtemp(prefix="n1bdisc-smoke-")
        return _run_dryrun_engine(
            scenario_name=args.scenario or "happy", target=args.target
            or SMOKE_TARGET, hap_path=args.hap or SMOKE_HAP,
            output_root=output_root, freeze_bound=False,
            gate11_eligible=False, preflight=None, freeze_hash=SMOKE_FREEZE_HASH,
            metadata=_SMOKE_METADATA, json_path=args.json)
    report = run_preflight(
        mode="dryrun", manifest_path=args.freeze_manifest,
        manifest_sha256=args.freeze_sha256,
        governance_path=args.governance_record,
        governance_sha256=args.governance_sha256, repo_root=repo_root,
        git_probe=probe, output_root=args.output_root, live=False)
    if not report["ok"]:
        return _refuse(mode="dryrun", reason="preflight-failed", report=report,
                       json_path=args.json)
    try:
        manifest = fm.load_manifest(args.freeze_manifest,
                                    expected_manifest_sha256=args.freeze_sha256)
    except fm.ManifestError as exc:
        return _refuse(mode="dryrun", reason=exc.code, json_path=args.json)
    pair = manifest.data["pair"]
    # 正式 dryrun：记录面绑定 manifest 三 ID/code_sha（零 pair 消费、零设备）。
    metadata = {
        "authorisation_id": pair["authorization_id"],
        "campaign_id": pair["campaign_id"],
        "evidence_id": pair["evidence_id"],
        "code_sha": manifest.data["code_sha"],
    }
    return _run_dryrun_engine(
        scenario_name=args.scenario or "happy",
        target=SMOKE_TARGET, hap_path=SMOKE_HAP,
        output_root=args.output_root, freeze_bound=True,
        gate11_eligible=True, preflight=report,
        freeze_hash=manifest.manifest_sha256, metadata=metadata,
        json_path=args.json)


def main(argv: Optional[List[str]] = None, *, input_fn: Optional[
        Callable[[str], str]] = None,
        git_probe: Optional[Callable[[str], Tuple[str, bool]]] = None,
        repo_root: Optional[str] = None) -> int:
    """CLI 唯一入口（``n1bdisc_run.main`` 薄转发至此；测试同入口注入）。

    ``input_fn``/``git_probe``/``repo_root`` 仅测试注入（stdin 确认行 / git 探针
    / 仓库根快照）；生产缺省 = 真实终端 stdin / 真实 git / runner 自在仓库根。
    """
    parser = build_parser()
    args = parser.parse_args(argv)          # 用法错误 → argparse exit 2
    _validate_args(parser, args)
    ask = input_fn if input_fn is not None else default_input
    probe = git_probe if git_probe is not None else real_git_probe
    root = repo_root if repo_root is not None else default_repo_root()
    try:
        if args.live:
            return _cmd_live(args, ask=ask, probe=probe, repo_root=root)
        return _cmd_dryrun(args, probe=probe, repo_root=root)
    except (EOFError, KeyboardInterrupt):
        return _refuse(mode="live" if args.live else "dryrun",
                       reason="confirmation-unavailable")


if __name__ == "__main__":
    sys.exit(main())
