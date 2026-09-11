#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc freeze manifest selftests（host-only，纯 stdlib，零设备/零网络/零执行）。

被测对象：``runner/n1bdisc_freeze_manifest.py``（候选 ready-freeze manifest 的
加载与机器验证）。夹具全部为**临时目录内 FAKE 数据**：FAKE 三 ID（20991231-0099
段，非任何真实分配）、FAKE code_sha、自制 fake-HAP zip（固定 .so 成员名 +
真实计算的 SHA-256）、假二进制/材料文件；**不读取任何真实 credential、真实
target/UDID、真实签名资产或旧 freeze 记录**。CC-2 元组与 bundle/tag 等字面取自
判据公开冻结字面（非敏感）。

覆盖（每条拒绝面一个钉，通过面一个钉）：

  L. load 门：外部 expected hash 绑定（错 hash/截断 hash/正确 hash）、未知
     schema_version、schema_version 类型（含 bool 钉）、非法 JSON、根类型
  V. 结构门：封闭键集（顶层/子对象未知键）、必填类型严格（int≠bool、
     float 不容）、完整 64 hex（截断/大写拒绝）、code_sha 40 hex 域、
     三 ID 格式 + 同日期 + 同序号段 + 前缀体一致、CC-2 元组漂移、
     runner 字面漂移、runner 清单排序/去重、artifact role 白名单/必填/
     重复/绝对路径
  R. 核验门：code_sha 不符 / dirty 树 / 调用者输入非法 / repo 根非法；
     runner 文件集精确覆盖（遗漏/多列/重复/逐文件 hash/改一字节）；
     artifact 缺失/hash 不符；hdc_binary 纯读不执行（非可执行文件也过）；
     HAP 内唯一 arm64-v8a .so（额外成员/成员名漂移/hash 漂移/非 zip）；
     confirmation 深度核对（旧 pair/退役形态/内部空格删除/model 漂移/
     非 JSON；首尾空白容忍钉）
  G. 治理输入：retired_pair_ids 命中即拒（记录自陈 confirmed-pass 也不豁免）、
     非法集合输入拒绝
  D. dryrun 预检：独立结果对象、失败透传、全程零写入（目录快照前后一致）
  H. 卫生钉：manifest JSON 文本无 password/token/endpoint/udid/private 字样

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_freeze_manifest.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_freeze_manifest.py
"""

from __future__ import annotations

import hashlib
import json
import os
import shutil
import sys
import tempfile
import zipfile

_HERE = os.path.dirname(os.path.abspath(__file__))
_REPO_ROOT = os.path.abspath(os.path.join(_HERE, os.pardir, os.pardir, os.pardir))
sys.path.insert(0, os.path.join(_HERE, os.pardir, "runner"))
import n1bdisc_freeze_manifest as fm  # noqa: E402

_ASSERTS = 0


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# FAKE 夹具常量（全部非真实；绝不读取实际 credential/设备标识）
# --------------------------------------------------------------------------

FAKE_AUTH = "AUTH-N1BDISC-PHYS1API26-20991231-0099"
FAKE_CAMPAIGN = "N1BDISC-PHYS1API26-20991231-0099"
FAKE_EVIDENCE = "EV-N1BDISC-PHYS1API26-20991231-0099"
FAKE_CODE_SHA = "cafebabe" * 5                      # 40 hex，纯夹具值
FAKE_CREATED = "2099-12-31"
SO_BYTES = b"\x7fELF-FAKE-ARM64-PROBE-PAYLOAD\x00\x01"

#: CC-2 冻结元组与 bundle/tag 公开字面（判据 :268/:1618——非敏感冻结值）。
CC2 = fm.CC2_TARGET_TUPLE
LITS = fm.FROZEN_LITERALS

_RUNNER_FILES = {
    "runner/n1b_fake_a.py": "# fake runner source a\nVALUE_A = 1\n",
    "runner/n1b_fake_b.py": "# fake runner source b\nVALUE_B = 2\n",
    "selftests/test_fake_a.py": "# fake selftest a\n",
    "selftests/test_fake_b.py": "# fake selftest b\n",
    "staticcheck/check_static.py": "# fake staticcheck\n",
}


def _sha_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _sha_file(path: str) -> str:
    with open(path, "rb") as fh:
        return _sha_bytes(fh.read())


def _write(path: str, data: bytes) -> str:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    return path


def build_repo(base: str) -> str:
    """临时仓库：spike runner/selftests/staticcheck 假源码 + 干扰项
    （__pycache__ 与子目录不计入候选集——m-1 聚合算法文件域钉）。"""
    repo = os.path.join(base, "repo")
    spike = os.path.join(repo, "spikes", "n1b-disc-phys-hap")
    for rel, text in _RUNNER_FILES.items():
        _write(os.path.join(spike, rel), text.encode("utf-8"))
    # 干扰项：不得进入候选 runner 集合
    _write(os.path.join(spike, "runner", "__pycache__", "junk.pyc"), b"pyc")
    _write(os.path.join(spike, "runner", "subdir", "nested.py"), b"nested")
    return repo


def build_confirmation(path: str, *, campaign: str = FAKE_CAMPAIGN,
                       record_status: str = "confirmed-pass",
                       verdict: str = "pass-tuple-bind-confirmed",
                       model: str = CC2["model"],
                       software_version: str = CC2["software_version"]) -> str:
    """FAKE target-binding confirmation（形态镜像 gate 5 记录，值全为夹具）。"""
    rec = {
        "record_type": "n1bdisc-target-binding-confirmation",
        "information_status": "fake-test-fixture",
        "record_status": record_status,
        "is_evidence": False,
        "gate": 5,
        "pair_number": 99,
        "date_local": FAKE_CREATED,
        "code_sha_at_gate1": FAKE_CODE_SHA,
        "authorization_id": "AUTH-" + campaign,
        "campaign_id": campaign,
        "evidence_id": "EV-" + campaign,
        "frozen_target_tuple": {
            "model": model,
            "software_version": software_version,
        },
        "verdict": verdict,
    }
    return _write(path, json.dumps(rec, ensure_ascii=False,
                                   indent=1).encode("utf-8"))


def build_artifacts(base: str, *, conf_kwargs=None,
                    so_bytes: bytes = SO_BYTES,
                    extra_so: bool = False) -> dict:
    """FAKE 工件集：signed HAP（zip，固定 .so 成员）/profile/cert/staticcheck
    报告/hdc 假二进制（**非可执行纯文件**）/confirmation JSON。"""
    art_dir = os.path.join(base, "artifacts")
    os.makedirs(art_dir, exist_ok=True)
    hap_path = os.path.join(art_dir, "n1bdisc-signed.hap")
    with zipfile.ZipFile(hap_path, "w", zipfile.ZIP_STORED) as zf:
        zf.writestr("module.json", '{"fake": true}')
        zf.writestr("libs/arm64-v8a/libn1bdisc_probe.so", so_bytes)
        if extra_so:
            zf.writestr("libs/arm64-v8a/libfake_extra.so", b"EXTRA")
    return {
        "signed_hap": hap_path,
        "profile": _write(os.path.join(art_dir, "fake-profile.p7b"), b"PROFILE"),
        "cert_chain": _write(os.path.join(art_dir, "fake-chain.cer"), b"CERT-CHAIN"),
        "staticcheck_report": _write(
            os.path.join(art_dir, "fake-staticcheck.json"),
            json.dumps({"exit_code": 0, "fail": [], "results": []}).encode()),
        "hdc_binary": _write(os.path.join(art_dir, "fake-hdc"),
                             b"#!/bin/sh\necho not-really-hdc\n"),
        "target_binding_confirmation": build_confirmation(
            os.path.join(art_dir, "fake-confirmation.json"),
            **(conf_kwargs or {})),
    }


def manifest_dict(sources: dict, arts: dict, **over) -> dict:
    """合规 manifest v1 基线（可逐字段覆写构造反例）。"""
    runner_sources = [{"path": "spikes/n1b-disc-phys-hap/" + rel,
                       "sha256": _sha_bytes(text.encode("utf-8"))}
                      for rel, text in sorted(sources.items())]
    artifacts = [{"role": role, "path": os.path.abspath(path),
                  "sha256": _sha_file(path)}
                 for role, path in sorted(arts.items())]
    data = {
        "schema_version": 1,
        "manifest_type": "n1bdisc-ready-freeze-manifest",
        "created_local": FAKE_CREATED,
        "code_sha": FAKE_CODE_SHA,
        "pair": {"pair_number": 99, "authorization_id": FAKE_AUTH,
                 "campaign_id": FAKE_CAMPAIGN, "evidence_id": FAKE_EVIDENCE},
        "target_tuple": dict(CC2),
        "frozen_literals": dict(LITS),
        "runner_sources": runner_sources,
        "artifacts": artifacts,
        "hap_so_member": {"member": fm.HAP_SO_MEMBER,
                          "sha256": _sha_bytes(SO_BYTES)},
    }
    data.update(over)
    return data


def write_manifest(base: str, data: dict) -> tuple:
    """写 manifest JSON → (绝对路径, 实际 bytes 的 SHA-256)。"""
    path = os.path.join(base, "candidate-freeze-manifest.json")
    raw = json.dumps(data, ensure_ascii=False, indent=1).encode("utf-8")
    _write(path, raw)
    return path, _sha_bytes(raw)


class Env:
    """一套完整合规夹具（repo + artifacts + manifest）；测试按需变异。"""

    def __init__(self, base: str, **over):
        self.base = base
        self.repo = build_repo(base)
        self.sources = dict(_RUNNER_FILES)
        self.arts = build_artifacts(base)
        self.manifest_path, self.manifest_sha = write_manifest(
            base, manifest_dict(self.sources, self.arts, **over))

    def load(self, **kw):
        return fm.load_manifest(self.manifest_path, **kw)

    def verify(self, m=None, *, code_sha=FAKE_CODE_SHA, dirty=False,
               retired=(), **kw):
        return fm.verify_inputs(m or self.load(), repo_root=self.repo,
                                current_code_sha=code_sha,
                                current_dirty=dirty,
                                retired_pair_ids=retired, **kw)

    def codes(self, report):
        return sorted(f.code for f in report.failures)


def fresh(name) -> str:
    d = tempfile.mkdtemp(prefix="n1bfm-%s-" % name)
    return d


def codes_of_validate(report):
    return sorted(f.code for f in report.failures)


# --------------------------------------------------------------------------
# 正例
# --------------------------------------------------------------------------

def test_compliant_manifest_full_pass():
    """合规完整 manifest：load → validate → verify 全过；hash 逐层自洽。"""
    base = fresh("pass")
    try:
        env = Env(base)
        m = env.load(expected_manifest_sha256=env.manifest_sha)
        expect(m.manifest_sha256 == env.manifest_sha, "bytes hash 必须回传")
        v = fm.validate_manifest(m)
        expect(v.ok, "结构验证应通过: %s" % (env.codes(v),))
        r = env.verify(m)
        expect(r.ok, "核验应通过: %s" % (env.codes(r),))
        expect(r.validation.ok, "核验内嵌结构验证应通过")
        # review_citation 可重复（role 白名单钉）
        cite = _write(os.path.join(base, "review-note.txt"), b"REVIEW")
        d2 = manifest_dict(env.sources, env.arts)
        d2["artifacts"].append({"role": "review_citation", "path": cite,
                                "sha256": _sha_file(cite)})
        d2["artifacts"].append({"role": "review_citation", "path": cite,
                                "sha256": _sha_file(cite)})
        # 同路径两条会让 path-duplicate 触发——review 允许重复 role，但路径仍须唯一
        p2, s2 = write_manifest(base, d2)
        rep = env.verify(fm.load_manifest(p2))
        expect(not rep.ok and "artifacts[7]-path-duplicate" in env.codes(rep),
               "同路径引用必须拒绝（path 唯一性）: %s" % (env.codes(rep),))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_hdc_binary_pure_read_not_executed():
    """hdc_binary 为不可执行纯文本文件也能通过——证明只读 hash、绝不执行。"""
    base = fresh("hdc")
    try:
        env = Env(base)
        expect(not os.access(env.arts["hdc_binary"], os.X_OK),
               "夹具 hdc 假二进制必须非可执行")
        r = env.verify()
        expect(r.ok, "纯读核验应通过: %s" % (env.codes(r),))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_manifest_json_no_secret_like_fields():
    """卫生钉：manifest 不给密码/私钥/endpoint/target/UDID 任何字段位置。"""
    base = fresh("hygiene")
    try:
        env = Env(base)
        with open(env.manifest_path, "rb") as fh:
            text = fh.read().decode("utf-8").lower()
        for bad in ("password", "passwd", "secret", "token", "endpoint",
                    "udid", "serial", "private_key", "privatekey"):
            expect(bad not in text, "manifest 不得出现敏感字段字样: %s" % bad)
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# 跨层回归：现行判据元组（CC-5）↔ runner 冻结常量
# --------------------------------------------------------------------------

def _criteria_software_version():
    """从现行判据环境行独立取得冻结软件版本字面（不复制被测常量）。"""
    path = os.path.join(_REPO_ROOT, "docs", "n1b-disc-gate-plan.md")
    with open(path, encoding="utf-8") as fh:
        for line in fh:
            if "物理冻结元组" not in line:
                continue
            for chunk in line.split("`"):
                if chunk.startswith("PLA-AL10 ") and chunk.endswith(")"):
                    return chunk
    raise AssertionError("判据环境行未找到冻结元组字面: %s" % path)


def _cc5_verify(base, manifest_ver, conf_ver):
    """既有 fixtures：manifest+confirmation 同带指定版本 → 真实 verify_inputs。"""
    repo = build_repo(base)
    arts = build_artifacts(base, conf_kwargs={"software_version": conf_ver})
    data = manifest_dict(_RUNNER_FILES, arts,
                         target_tuple=dict(CC2, software_version=manifest_ver))
    p, s = write_manifest(base, data)
    return fm.verify_inputs(fm.load_manifest(p, expected_manifest_sha256=s),
                            repo_root=repo, current_code_sha=FAKE_CODE_SHA,
                            current_dirty=False, retired_pair_ids=())


def test_cc5_criteria_tuple_manifest_and_confirmation_pass():
    """现行判据元组（独立取得）经 manifest+confirmation 真实 verify 通过。"""
    ver = _criteria_software_version()
    base = fresh("cc5-pass")
    try:
        r = _cc5_verify(base, ver, ver)
        expect(r.ok, "现行判据元组应通过 verify: %s"
               % sorted(f.code for f in r.failures))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_cc5_old_sp6c_rejected_on_manifest_and_confirmation():
    """旧 CC-2 SP6C 字面在 manifest 侧与 confirmation 侧分别拒绝。"""
    ver = _criteria_software_version()
    old = "PLA-AL10 7.0.0.105(SP6C00E105R7P3)"
    for side, mv, cv, want in (
            ("manifest", old, ver, "target-tuple-drift"),
            ("confirmation", ver, old, "confirmation-software-version-drift")):
        base = fresh("cc5-neg-" + side)
        try:
            r = _cc5_verify(base, mv, cv)
            cs = sorted(f.code for f in r.failures)
            expect(not r.ok and want in cs,
                   "%s 侧旧 SP6C 必须以 %s 拒绝: %s" % (side, want, cs))
        finally:
            shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# load 门
# --------------------------------------------------------------------------

def test_external_hash_binding():
    """外部 expected hash 绑定：错 hash 拒、截断 hash 拒、正确 hash 过。"""
    base = fresh("bind")
    try:
        env = Env(base)
        wrong = "0" * 64
        try:
            env.load(expected_manifest_sha256=wrong)
            expect(False, "错 hash 必须拒绝")
        except fm.ManifestError as e:
            expect(e.code == "manifest-sha256-mismatch", e.code)
            expect(e.detail["actual_manifest_sha256"] == env.manifest_sha,
                   "detail 必须带实际 bytes hash")
        for trunc in ("0c3ef980", "0" * 63, "0" * 65, "A" * 64):
            try:
                env.load(expected_manifest_sha256=trunc)
                expect(False, "截断/大小写不合规 hash 必须拒绝: %r" % trunc)
            except fm.ManifestError as e:
                expect(e.code == "expected-manifest-sha256-format", e.code)
        m = env.load(expected_manifest_sha256=env.manifest_sha)
        expect(m.manifest_sha256 == env.manifest_sha, "正确 hash 应绑定通过")
        # 不传 expected 也可加载（绑定与否由调用者决定）
        expect(env.load().manifest_sha256 == env.manifest_sha, "无 expected 应可加载")
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_load_level_rejections():
    """load 门：未知 schema_version / 类型 / 非法 JSON / 根类型 / 缺文件。"""
    base = fresh("load")
    try:
        env = Env(base)
        cases = [
            ({"schema_version": 2}, "unknown-schema-version"),
            ({"schema_version": "1"}, "schema-version-type"),
            ({"schema_version": True}, "schema-version-type"),
            ({"schema_version": 1.0}, "schema-version-type"),
        ]
        for over, want in cases:
            d = manifest_dict(env.sources, env.arts)
            d.update(over)
            p, _ = write_manifest(base, d)
            try:
                fm.load_manifest(p)
                expect(False, "必须拒绝: %s" % want)
            except fm.ManifestError as e:
                expect(e.code == want, "want=%s got=%s" % (want, e.code))
        # 缺 schema_version
        d = manifest_dict(env.sources, env.arts)
        del d["schema_version"]
        p, _ = write_manifest(base, d)
        try:
            fm.load_manifest(p)
            expect(False, "缺 schema_version 必须拒绝")
        except fm.ManifestError as e:
            expect(e.code == "schema-version-missing", e.code)
        # 非法 JSON
        bad = _write(os.path.join(base, "bad.json"), b"{not json")
        try:
            fm.load_manifest(bad)
            expect(False, "非法 JSON 必须拒绝")
        except fm.ManifestError as e:
            expect(e.code == "manifest-invalid-json", e.code)
        # 根类型
        arr = _write(os.path.join(base, "arr.json"), b"[]")
        try:
            fm.load_manifest(arr)
            expect(False, "根数组必须拒绝")
        except fm.ManifestError as e:
            expect(e.code == "manifest-root-type", e.code)
        # 文件不存在
        try:
            fm.load_manifest(os.path.join(base, "nope.json"))
            expect(False, "缺文件必须拒绝")
        except fm.ManifestError as e:
            expect(e.code == "manifest-not-found", e.code)
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# 结构门
# --------------------------------------------------------------------------

def _val(base, over_keys=None, mutate=None):
    """构造变体 manifest → validate 结果（不触文件核验）。"""
    env = Env(base)
    d = manifest_dict(env.sources, env.arts)
    if mutate is not None:
        mutate(d)
    for k, v in (over_keys or {}).items():
        d[k] = v
    p, _ = write_manifest(base, d)
    return fm.validate_manifest(fm.load_manifest(p))


def test_structure_rejections():
    """结构门集中钉：封闭键/严格类型/hash 格式/ID/元组/字面/排序/role。"""
    base = fresh("struct")
    try:
        def expect_code(report, want, note=""):
            expect(not report.ok, "必须拒绝: %s %s" % (want, note))
            expect(want in codes_of_validate(report),
                   "want=%s got=%s %s" % (want, codes_of_validate(report), note))

        # 顶层未知键（封闭 schema）
        expect_code(_val(base, mutate=lambda d: d.update({"extra_key": 1})),
                    "manifest-keys", "顶层未知键")
        # 子对象未知键
        expect_code(_val(base, mutate=lambda d: d["pair"].update({"note": "x"})),
                    "pair-keys", "pair 未知键")
        expect_code(_val(base, mutate=lambda d: d["hap_so_member"].update(
            {"k": "v"})), "hap-so-member-keys", "so_member 未知键")
        expect_code(_val(base, mutate=lambda d: d["runner_sources"][0].update(
            {"mode": "rb"})), "runner-sources[0]-keys", "runner 条目未知键")
        expect_code(_val(base, mutate=lambda d: d["artifacts"][0].update(
            {"size": 1})), "artifacts[0]-keys", "artifact 条目未知键")
        # 缺必填键
        expect_code(_val(base, mutate=lambda d: d.pop("frozen_literals")),
                    "manifest-keys", "缺 frozen_literals")
        expect_code(_val(base, mutate=lambda d: d.pop("hap_so_member")),
                    "manifest-keys", "缺 hap_so_member")
        # 严格类型：bool 冒充 int / float 冒充 int / 元组值漂移
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"pair_number": True})), "pair-number-domain", "bool 冒充 int")
        expect_code(_val(base, mutate=lambda d: d["target_tuple"].update(
            {"api_level": 26.0})), "target-tuple-drift", "float 冒充 int")
        old = dict(CC2)
        old["software_version"] = "PLA-AL10 7.0.0.102(SP8C00E102R7P3)"
        expect_code(_val(base, over_keys={"target_tuple": old}),
                    "target-tuple-drift", "CC-1 旧元组")
        expect_code(_val(base, over_keys={"target_tuple": dict(
            CC2, arch="arm64")}), "target-tuple-drift", "abi 漂移")
        lit = dict(LITS)
        lit["bundle"] = "cn.alfadb.netbird.n1bdisc "   # 尾随空格也是漂移
        expect_code(_val(base, over_keys={"frozen_literals": lit}),
                    "frozen-literals-drift", "bundle 尾随空格")
        lit2 = dict(LITS)
        lit2["staging_root"] = "/data/local/tmp/other"
        expect_code(_val(base, over_keys={"frozen_literals": lit2}),
                    "frozen-literals-drift", "staging 漂移")
        # hash 格式：截断 / 大写
        expect_code(_val(base, mutate=lambda d: d["runner_sources"][0].update(
            {"sha256": "0" * 32})), "runner-sources[0]-sha256-format", "截断")
        expect_code(_val(base, mutate=lambda d: d["artifacts"][0].update(
            {"sha256": "A" * 64})), "artifacts[0]-sha256-format", "大写")
        # code_sha 域：40 hex 之外（64 hex sha256 冒充 git sha 也在域外）
        expect_code(_val(base, over_keys={"code_sha": "a" * 64}),
                    "code-sha-format", "64 hex 冒充 code_sha")
        expect_code(_val(base, over_keys={"code_sha": FAKE_CODE_SHA[:-1] + "g"}),
                    "code-sha-format", "非 hex")
        # created_local
        expect_code(_val(base, over_keys={"created_local": "2099/12/31"}),
                    "created-local-format", "日期格式")
        expect_code(_val(base, over_keys={"created_local": "2099-13-40"}),
                    "created-local-format", "非真实日期")
        # 三 ID：格式 / 同日期 / 同序号 / 前缀体
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"evidence_id": "EV-N1BDISC-PHYS1API26-20991231-0098"})),
            "pair-id-inconsistent", "序号段不一致")
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"authorization_id": "AUTH-N1BDISC-PHYS1API26-20990101-0099"})),
            "pair-id-inconsistent", "日期段不一致")
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"campaign_id": "N1BDISC-PHYS1API26-20991231-99"})),
            "pair-campaign-id-format", "序号段长度不足")
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"authorization_id": "auth-" + FAKE_CAMPAIGN})),
            "pair-authorization-id-format", "小写前缀")
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"evidence_id": "N1BDISC-PHYS1API26-20991231-0099"})),
            "pair-evidence-id-format", "缺 EV- 前缀 = 格式不符")
        expect_code(_val(base, mutate=lambda d: d["pair"].update(
            {"pair_number": 0})), "pair-number-domain", "序号须正整数")
        # runner 清单：排序 / 重复 / 路径域 / 穿越形态
        expect_code(_val(base, mutate=lambda d: d["runner_sources"].reverse()),
                    "runner-not-sorted", "乱序")
        expect_code(_val(base, mutate=lambda d: d["runner_sources"].append(
            dict(d["runner_sources"][0]))),
            "runner-duplicate", "重复条目")
        expect_code(_val(base, mutate=lambda d: d["runner_sources"].append(
            {"path": "spikes/n1b-disc-phys-hap/runner/../runner/x.py",
             "sha256": "0" * 64})),
            "runner-sources[5]-path-escape", ".. 段")
        expect_code(_val(base, mutate=lambda d: d["runner_sources"].append(
            {"path": "docs/n1b-disc-gate-plan.md", "sha256": "0" * 64})),
            "runner-sources[5]-path-domain", "域外路径")
        expect_code(_val(base, mutate=lambda d: d["runner_sources"].append(
            {"path": "/abs/x.py", "sha256": "0" * 64})),
            "runner-sources[5]-path-absolute", "绝对路径混入 runner 清单")
        # artifacts：role 白名单 / 必填 / 重复 / 相对路径
        expect_code(_val(base, mutate=lambda d: d["artifacts"].append(
            {"role": "mystery", "path": "/x", "sha256": "0" * 64})),
            "artifacts[6]-role-unknown", "未知 role")
        expect_code(_val(base, mutate=lambda d: d["artifacts"].pop(0)),
                    "artifact-role-required-missing", "缺必备 role")
        expect_code(_val(base, mutate=lambda d: d["artifacts"].append(
            dict(d["artifacts"][0]))),
            "artifacts[6]-path-duplicate", "同路径重复绑定")
        expect_code(_val(base, mutate=lambda d: d["artifacts"][0].update(
            {"path": "relative/path"})), "artifacts[0]-path-relative",
            "artifact 相对路径")
        expect_code(_val(base, mutate=lambda d: d["artifacts"][0].update(
            {"path": "/a/../b"})), "artifacts[0]-path-escape", "非规范绝对路径")
        # so 成员名固定
        expect_code(_val(base, mutate=lambda d: d["hap_so_member"].update(
            {"member": "libs/arm64-v8a/libother.so"})),
            "so-member-name-fixed", "成员名漂移")
        # manifest_type
        expect_code(_val(base, over_keys={"manifest_type": "other-manifest"}),
                    "manifest-type-mismatch", "类型字面")
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_review_citation_role_repeatable_but_path_unique():
    """review_citation 可多条（不同路径）；仅字节绑定，不赋授权含义。"""
    base = fresh("cite")
    try:
        env = Env(base)
        c1 = _write(os.path.join(base, "r1.txt"), b"CITE-1")
        c2 = _write(os.path.join(base, "r2.txt"), b"CITE-2")
        d = manifest_dict(env.sources, env.arts)
        d["artifacts"] += [
            {"role": "review_citation", "path": c1, "sha256": _sha_file(c1)},
            {"role": "review_citation", "path": c2, "sha256": _sha_file(c2)},
        ]
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(r.ok, "两条不同路径 review 引用应通过: %s" % (env.codes(r),))
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# 核验门
# --------------------------------------------------------------------------

def test_caller_input_and_gate1_bindings():
    """code_sha/dirty/调用者输入非法/repo 根非法。"""
    base = fresh("caller")
    try:
        env = Env(base)
        r = env.verify(code_sha="b" * 40)
        expect(not r.ok and "code-sha-mismatch" in env.codes(r), env.codes(r))
        r = env.verify(dirty=True)
        expect(not r.ok and "dirty-tree" in env.codes(r), env.codes(r))
        r = env.verify(code_sha="zz")
        expect(not r.ok and "caller-input-invalid" in env.codes(r), env.codes(r))
        r = env.verify(retired="nope")
        expect(not r.ok and "caller-input-invalid" in env.codes(r), env.codes(r))
        r = fm.verify_inputs(env.load(), repo_root=os.path.join(base, "no-repo"),
                             current_code_sha=FAKE_CODE_SHA,
                             current_dirty=False, retired_pair_ids=())
        expect(not r.ok and "repo-root-invalid" in
               sorted(f.code for f in r.failures), "repo 根非法必须拒绝")
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_runner_coverage_and_hashes():
    """runner 精确覆盖：遗漏/多列/逐文件 hash/改一字节/删文件。"""
    base = fresh("runner")
    try:
        env = Env(base)
        # 遗漏：manifest 少列一个实际文件
        d = manifest_dict(env.sources, env.arts)
        d["runner_sources"] = d["runner_sources"][1:]
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "runner-file-missing" in env.codes(r), env.codes(r))
        expect(any(f.detail.get("path", "").endswith("n1b_fake_a.py")
                   for f in r.failures), "detail 应指名遗漏文件")
        # 多列：manifest 列出不存在的文件
        d = manifest_dict(env.sources, env.arts)
        d["runner_sources"].append(
            {"path": "spikes/n1b-disc-phys-hap/runner/ghost.py",
             "sha256": "0" * 64})
        d["runner_sources"].sort(key=lambda e: e["path"])
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "runner-file-extra" in env.codes(r), env.codes(r))
        # 改一字节：runner 源文件内容漂移 → hash 不符
        src = os.path.join(env.repo, "spikes", "n1b-disc-phys-hap",
                           "runner", "n1b_fake_b.py")
        with open(src, "ab") as fh:
            fh.write(b"\x00")  # 单字节追加
        r = env.verify()
        expect(not r.ok and "runner-hash-mismatch" in env.codes(r), env.codes(r))
        # 删除一个已列文件 → extra（相对 manifest）
        os.remove(os.path.join(env.repo, "spikes", "n1b-disc-phys-hap",
                               "selftests", "test_fake_a.py"))
        r = env.verify()
        expect(not r.ok and "runner-file-extra" in env.codes(r), env.codes(r))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_artifact_byte_flip_missing_and_hash_drift():
    """artifact：改一字节 → hash 不符；删除 → missing；HAP .so 漂移各形态。"""
    base = fresh("artifact")
    try:
        env = Env(base)
        # profile 改一字节（内容级，非删除）
        with open(env.arts["profile"], "r+b") as fh:
            fh.seek(0)
            fh.write(b"p")  # 'P' -> 'p'
        r = env.verify()
        expect(not r.ok and "artifact-hash-mismatch" in env.codes(r),
               env.codes(r))
        expect(any(f.detail.get("role") == "profile" for f in r.failures),
               "detail 应指明 role")
    finally:
        shutil.rmtree(base, ignore_errors=True)

    base = fresh("artifact2")
    try:
        env = Env(base)
        os.remove(env.arts["hdc_binary"])
        r = env.verify()
        expect(not r.ok and "artifact-missing" in env.codes(r), env.codes(r))
    finally:
        shutil.rmtree(base, ignore_errors=True)

    # HAP：额外 .so 成员 / 成员名不符 / .so 字节漂移 / 非 zip
    base = fresh("hap")
    try:
        env = Env(base)
        extra = build_artifacts(os.path.join(base, "hx"), extra_so=True)
        d = manifest_dict(env.sources, extra)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "so-member-count" in env.codes(r), env.codes(r))

        so_drift = build_artifacts(os.path.join(base, "hy"), so_bytes=SO_BYTES + b"X")
        d = manifest_dict(env.sources, so_drift)
        d["hap_so_member"]["sha256"] = _sha_bytes(SO_BYTES)  # manifest 仍绑旧 hash
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "so-member-hash-mismatch" in env.codes(r),
               env.codes(r))

        notzip = os.path.join(base, "notzip.hap")
        _write(notzip, b"definitely not a zip")
        d = manifest_dict(env.sources, dict(env.arts, signed_hap=notzip))
        d["artifacts"] = [e for e in d["artifacts"]
                          if e["role"] != "signed_hap"]
        d["artifacts"].append({"role": "signed_hap", "path": notzip,
                               "sha256": _sha_file(notzip)})
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "hap-not-zip" in env.codes(r), env.codes(r))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_confirmation_deep_checks():
    """confirmation：旧 pair / 退役形态 / 内部空格删除 / model 漂移 / 非 JSON；
    首尾空白容忍钉（仅去首尾，内部空格保留）。"""
    base = fresh("confirm")
    try:
        env = Env(base)
        # 旧 pair（自陈 confirmed-pass 也拒——pair 绑定）
        old = build_artifacts(os.path.join(base, "old"), conf_kwargs={
            "campaign": "N1BDISC-PHYS1API26-20990101-0001"})
        d = manifest_dict(env.sources, old)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok and "confirmation-pair-mismatch" in env.codes(r),
               env.codes(r))
        # 退役形态（镜像 pair-1 记录的字面形态；值全 FAKE）
        retired = build_artifacts(os.path.join(base, "ret"), conf_kwargs={
            "record_status": "consumed-blocked-final",
            "verdict": "blocked-tuple-drift"})
        d = manifest_dict(env.sources, retired)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        cs = env.codes(r)
        expect("confirmation-status-not-pass" in cs and
               "confirmation-verdict-not-pass" in cs, cs)
        # 内部空格删除 → 拒（仅去首尾空白口径）
        nospace = build_artifacts(os.path.join(base, "ns"), conf_kwargs={
            "software_version": "PLA-AL107.0.0.105(SP6C00E105R7P3)"})
        d = manifest_dict(env.sources, nospace)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect("confirmation-software-version-drift" in env.codes(r),
               env.codes(r))
        # model 漂移 → 拒
        drift = build_artifacts(os.path.join(base, "md"), conf_kwargs={
            "model": "OTHER-1"})
        d = manifest_dict(env.sources, drift)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect("confirmation-model-drift" in env.codes(r), env.codes(r))
        # 首尾空白容忍（尾随空格经 strip 后逐字相等 → 通过）
        tail = build_artifacts(os.path.join(base, "tail"), conf_kwargs={
            "software_version": CC2["software_version"] + " "})
        d = manifest_dict(env.sources, tail)
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(r.ok, "尾随空白应被容忍（仅去首尾）: %s" % (env.codes(r),))
        # 非 JSON confirmation
        notjson = _write(os.path.join(base, "nj.json"), b"not json{")
        d = manifest_dict(env.sources, dict(env.arts,
                                            target_binding_confirmation=notjson))
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect("confirmation-parse" in env.codes(r), env.codes(r))
    finally:
        shutil.rmtree(base, ignore_errors=True)


def test_structure_failure_short_circuits_file_checks():
    """结构不过 → 核验直接短路返回结构失败（不做文件级判定）。"""
    base = fresh("short")
    try:
        env = Env(base)
        d = manifest_dict(env.sources, env.arts)
        d["target_tuple"]["arch"] = "x86"
        p, _ = write_manifest(base, d)
        r = env.verify(fm.load_manifest(p))
        expect(not r.ok, "必须拒绝")
        expect(env.codes(r) == ["target-tuple-drift"],
               "只应含结构失败: %s" % (env.codes(r),))
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# 治理输入：retired_pair_ids
# --------------------------------------------------------------------------

def test_retired_pair_ids_governance_gate():
    """治理退役输入：命中即拒（记录自陈 confirmed-pass 不豁免）。"""
    base = fresh("retired")
    try:
        env = Env(base)
        # 基线：空退役集 → 通过
        expect(env.verify(retired=()).ok, "空退役集应通过")
        # 命中：即使 confirmation 自陈 pass，也必须拒
        r = env.verify(retired={FAKE_CAMPAIGN})
        expect(not r.ok and "pair-retired" in env.codes(r), env.codes(r))
        expect(any(f.detail.get("campaign_id") == FAKE_CAMPAIGN
                   for f in r.failures), "detail 应带 campaign_id")
        # 字符串冒充集合 → 调用者输入非法
        r = env.verify(retired=FAKE_CAMPAIGN)
        expect(not r.ok and "caller-input-invalid" in env.codes(r), env.codes(r))
        # 预检同样透传退役拒绝
        pre = fm.dryrun_precheck(env.load(), repo_root=env.repo,
                                 current_code_sha=FAKE_CODE_SHA,
                                 current_dirty=False,
                                 retired_pair_ids=[FAKE_CAMPAIGN])
        expect(pre["ok"] is False, "预检必须透传失败")
        expect(any(f["code"] == "pair-retired" for f in pre["failures"]),
               pre["failures"])
        # 必填钉：省略 retired_pair_ids = 契约错误（不得静默默认「无退役」）
        for fn in (fm.verify_inputs, fm.dryrun_precheck):
            try:
                fn(env.load(), repo_root=env.repo,
                   current_code_sha=FAKE_CODE_SHA, current_dirty=False)
                expect(False, "省略 retired_pair_ids 必须 TypeError")
            except TypeError:
                pass
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# dryrun 预检
# --------------------------------------------------------------------------

def test_dryrun_precheck_shape_and_zero_writes():
    """预检结果独立成形、失败透传、全程零写入（目录快照前后一致）。"""
    base = fresh("precheck")
    try:
        env = Env(base)

        def snapshot(root):
            out = {}
            for dirpath, _dirs, files in os.walk(root):
                for name in files:
                    fp = os.path.join(dirpath, name)
                    out[os.path.relpath(fp, root)] = _sha_file(fp)
            return out

        before = snapshot(base)
        m = env.load()
        pre = fm.dryrun_precheck(m, repo_root=env.repo,
                                 current_code_sha=FAKE_CODE_SHA,
                                 current_dirty=False, retired_pair_ids=())
        expect(pre["precheck_type"] == "n1bdisc-freeze-manifest-dryrun-precheck",
               pre)
        expect(pre["is_evidence"] is False, "预检必须 is_evidence=false")
        expect(pre["ok"] is True and pre["failures"] == [], pre)
        expect(pre["manifest_sha256"] == env.manifest_sha, pre)
        expect(pre["schema_version"] == 1, pre)
        # 失败透传：dirty 树
        pre2 = fm.dryrun_precheck(m, repo_root=env.repo,
                                  current_code_sha=FAKE_CODE_SHA,
                                  current_dirty=True, retired_pair_ids=())
        expect(pre2["ok"] is False and
               any(f["code"] == "dirty-tree" for f in pre2["failures"]), pre2)
        # DryRun 记录的 integrity={} 字面不受本模块影响（结构钉：预检结果
        # 不含 integrity 键——它不是记录本体，绝不代写记录字段）
        expect("integrity" not in pre and "integrity" not in pre2, pre)
        after = snapshot(base)
        expect(before == after, "预检全程必须零写入")
    finally:
        shutil.rmtree(base, ignore_errors=True)


# --------------------------------------------------------------------------
# 自研 main（pytest 与双兼容；与既有 selftests 同款）
# --------------------------------------------------------------------------

def main() -> int:
    tests = [(name, fn) for name, fn in sorted(globals().items())
             if name.startswith("test_") and callable(fn)]
    failed = []
    for name, fn in tests:
        try:
            fn()
            print("PASS %s" % name)
        except AssertionError as exc:
            failed.append(name)
            print("FAIL %s: %s" % (name, exc))
        except Exception as exc:  # noqa: BLE001 — 自研 runner 兜底
            failed.append(name)
            print("ERROR %s: %r" % (name, exc))
    print("---")
    print("expect 断言计数: %d" % _ASSERTS)
    print("用例: %d  失败: %d" % (len(tests), len(failed)))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
