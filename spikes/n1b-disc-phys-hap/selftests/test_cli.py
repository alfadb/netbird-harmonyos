#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""n1bdisc CLI selftests——正式 CLI 入口（live 冻结门 / dryrun engine 接线）。

被测对象：``runner/n1bdisc_cli.py`` :func:`main`（含 ``n1bdisc_run.main`` 薄转
发）。测试**真正走 CLI**：多数用例经同一入口注入（``main(argv, input_fn=...,
git_probe=..., repo_root=...)``），live 完整生命周期另有**真 subprocess** 端到端
（tmp git 快照仓库 + 真 stdin + 真 transport spawn）。全部身份为 TEST/2099 占位
（沿用 ``test_freeze_manifest`` 夹具家族），夹具均在系统临时目录；live 测试
spawn 的唯一 hdc 角色可执行 = ``selftests/fixtures/fake_hdc_campaign.py`` 的临
时副本——绝不真实 hdc/设备/签名/提交，不改 PATH，不读真实 credential。

覆盖：

- 用法门：--help（新入口与薄转发）、argparse 拒绝（部分冻结四件套/缺 target/
  缺 output-root/形态互斥/SHA 格式）全部 exit 2 且零副作用；
- 拒启门（全部零 HDC、零消费、不建 run 目录）：manifest 缺失/外部分配 hash
  不符、governance hash/绑定（三 ID/code_sha/freeze sha）/状态/退役命中/
  run-state 根缺失、dirty 树、code_sha 不符、LIVE/operator 确认不符或不可得；
- live 完整生命周期（in-process + subprocess 双形态）：快照仓库 manifest →
  preflight.json 随封签入清单 → FakeHdc campaign 夹具全生命周期 → seal →
  claim 推进 start-entry-attempted → 全部输出/磁盘无 target；
- 单次性：同 pair 换 output-root 再启动拒绝（claim 在 governance 受控根，与
  output-root 无关）；StartEntry 注入失败 → 恰一次尝试、不重发、incomplete
  封签、claim 已消费；
- 正式 dryrun（freeze-bound）：同一预检、独立 preflight 字段/文件、
  is_evidence=false / integrity={} 原字面、不消费 pair；无 manifest smoke：
  freeze_bound=false / gate11_eligible=false（不具门 11 资格）仍走同一 engine；
- 剧本判定等价钉：8 个 CLI 剧本经 engine 与旧 helper 直调 verdict/gates 全等价
  （join 十值域闭域检查恒在——POST 在场即查，no-fact 只免除比对：join-bogus
  两路径一致 fail(F8)、d8b-bogus 依然 F8、合法 joined+no-fact 不误判）；live
  完整 manifest 路径同污染 POST（join=pending）→ F8 fail；
- 真候选工作区现实：subprocess live 对当前候选仓（脏树/不可用 manifest）在任何
  hdc 之前拒启。

一条命令运行（pytest 与自研 main 双兼容）::

    python3 spikes/n1b-disc-phys-hap/selftests/test_cli.py
    python3 -m pytest spikes/n1b-disc-phys-hap/selftests/test_cli.py
"""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
from contextlib import redirect_stdout

_HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(_HERE, os.pardir, "runner"))
sys.path.insert(1, os.path.join(_HERE, "fixtures"))

import n1bdisc_cli as cli                 # noqa: E402
import n1bdisc_freeze_manifest as fm      # noqa: E402
import n1bdisc_hdc as hdc                 # noqa: E402
import n1bdisc_run as run_mod             # noqa: E402
import fake_hdc as fake                   # noqa: E402
import fake_hdc_campaign as cli_fake      # noqa: E402 （剧本注入 env 名用）
import test_freeze_manifest as tfm        # noqa: E402

_ASSERTS = 0

#: 候选仓根（spikes/n1b-disc-phys-hap 上溯三级；只读，绝不写/绝不 commit）。
CANDIDATE_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(_HERE)))
#: fixture 源文件（live 测试执行的是它的可执行临时副本）。
CAMPAIGN_FIXTURE_SRC = os.path.join(_HERE, "fixtures", "fake_hdc_campaign.py")

#: TEST/2099 占位身份（非任何真实分配；沿用 tfm 夹具家族）。
CAMPAIGN = tfm.FAKE_CAMPAIGN
AUTH = tfm.FAKE_AUTH
EVIDENCE = tfm.FAKE_EVIDENCE
RETIRED_PAIR = "N1BDISC-PHYS1API26-20980101-0001"   # FAKE 退役旧 pair（夹具）
#: 测试专用 FAKE target 字面（只允许出现在注入侧，绝不许出现在任何输出/磁盘）。
FAKE_TARGET = "E2E-CLI-FAKE-TGT-2099"


def expect(cond, msg):
    global _ASSERTS
    _ASSERTS += 1
    if not cond:
        raise AssertionError(msg)


# --------------------------------------------------------------------------
# 通用替身与沙箱（自包含；模式沿 test_engine）
# --------------------------------------------------------------------------

def sha_file(path):
    with open(path, "rb") as fh:
        return hashlib.sha256(fh.read()).hexdigest()


def confirm_input(*lines):
    """stdin 注入：依次返回确认行（忽略 prompt）；耗尽 = stdin 关闭（EOFError）。"""
    it = iter(lines)

    def _ask(_prompt):
        try:
            return next(it)
        except StopIteration:
            raise EOFError("stdin closed (test injection)") from None
    return _ask


def probe_ok(head):
    return lambda _repo_root: (head, False)


def probe_dirty(head):
    return lambda _repo_root: (head, True)


def expect_exit(fn, want, note=""):
    try:
        rc = fn()
    except SystemExit as exc:            # argparse 用法拒绝形态
        rc = exc.code
    expect(rc == want, "exit %r != %r %s" % (rc, want, note))
    return rc


@contextlib.contextmanager
def sandbox(prefix="n1b-cli-"):
    tmp = tempfile.mkdtemp(prefix=prefix)
    try:
        yield tmp
    finally:
        for pid in leftover_fixture_procs(tmp, timeout_s=3.0):
            try:
                os.kill(pid, signal.SIGKILL)   # 只清自己 spawn 的夹具进程
            except OSError:
                pass
        shutil.rmtree(tmp, ignore_errors=True)


def leftover_fixture_procs(tmp_root, timeout_s=3.0):
    """/proc 扫描：cmdline 以临时目录为前缀的残留夹具进程（有界等待消失）。"""
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


class DeadlineExceeded(Exception):
    pass


@contextlib.contextmanager
def wall_deadline(seconds):
    """用例级墙钟护栏（subprocess 端到端防挂死）。"""
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


# --------------------------------------------------------------------------
# 快照仓库 / manifest / governance 夹具（TEST/2099 身份；tmp git 固定快照）
# --------------------------------------------------------------------------

def _git(args, cwd):
    proc = subprocess.run(["git"] + args, cwd=cwd, capture_output=True,
                          text=True)
    expect(proc.returncode == 0, "git %s failed: %s" % (args, proc.stderr))
    return proc.stdout.strip()


def build_snapshot_repo(base):
    """把候选 spike runner/selftests/staticcheck **当前字节**复制进 tmp git 仓库
    并提交——固定测试快照，不依赖候选脏工作区恰好 pass；返回 (repo, head)。"""
    src_spike = os.path.join(CANDIDATE_ROOT, "spikes", "n1b-disc-phys-hap")
    repo = os.path.join(base, "repo")
    dst_spike = os.path.join(repo, "spikes", "n1b-disc-phys-hap")
    for d in ("runner", "selftests", "staticcheck"):
        shutil.copytree(os.path.join(src_spike, d),
                        os.path.join(dst_spike, d),
                        ignore=shutil.ignore_patterns("__pycache__"))
    with open(os.path.join(repo, ".gitignore"), "w", encoding="utf-8") as fh:
        fh.write("__pycache__/\n*.pyc\n")
    ident = ["-c", "user.email=cli-selftest@example.invalid",
             "-c", "user.name=cli-selftest"]
    _git(["init", "-q"], repo)
    _git(["add", "-A"], repo)
    _git(ident + ["commit", "-q", "-m", "cli-selftest snapshot"], repo)
    return repo, _git(["rev-parse", "HEAD"], repo)


def snapshot_sources(repo):
    """快照仓的实际候选 runner 文件集合 → tfm.manifest_dict 的 sources dict。"""
    base = os.path.join(repo, "spikes", "n1b-disc-phys-hap")
    entries = []
    for d in ("runner", "selftests"):
        dd = os.path.join(base, d)
        for name in sorted(os.listdir(dd)):
            p = os.path.join(dd, name)
            if name.endswith(".py") and os.path.isfile(p):
                entries.append(("spikes/n1b-disc-phys-hap/%s/%s" % (d, name), p))
    entries.append(("spikes/n1b-disc-phys-hap/staticcheck/check_static.py",
                    os.path.join(base, "staticcheck", "check_static.py")))
    sources = {}
    for rel, p in sorted(entries):
        with open(p, "r", encoding="utf-8") as fh:
            sources[rel[len("spikes/n1b-disc-phys-hap/"):]] = fh.read()
    return sources


def make_hdc_fixture(base):
    """fake_hdc_campaign.py 可执行临时副本（live 测试唯一 hdc 角色进程路径）。"""
    exe_dir = os.path.join(base, "bin")
    os.makedirs(exe_dir, exist_ok=True)
    exe = os.path.join(exe_dir, "fake_hdc_campaign.py")
    shutil.copyfile(CAMPAIGN_FIXTURE_SRC, exe)
    os.chmod(exe, 0o755)
    return exe


def build_manifest(base, repo, head, *, hdc_executable, extra_source=None):
    """TEST 身份完整合规 manifest（三 ID=2099 段；code_sha=快照 HEAD；
    artifacts 全 fake，hdc_binary=可执行夹具副本，hash 实算绑定）。"""
    sources = snapshot_sources(repo)
    if extra_source is not None:
        sources[extra_source] = "# cli-selftest bogus extra entry\n"
    arts = tfm.build_artifacts(base)
    arts["hdc_binary"] = hdc_executable
    over = {
        "code_sha": head,
        "pair": {"pair_number": 99, "authorization_id": AUTH,
                 "campaign_id": CAMPAIGN, "evidence_id": EVIDENCE},
    }
    data = tfm.manifest_dict(sources, arts, **over)
    return tfm.write_manifest(base, data)


def build_governance(path, *, code_sha, freeze_sha, run_state_root,
                     campaign=CAMPAIGN, status="approved-unused",
                     retired=(RETIRED_PAIR,), auth=AUTH, evidence=EVIDENCE,
                     **over):
    """governance record（schema = n1bdisc_cli docstring；TEST 身份）。"""
    doc = {
        "record_type": cli.GOV_RECORD_TYPE,
        "schema_version": cli.GOV_SCHEMA_VERSION,
        "created_local": "2099-12-31",
        "authorization_id": auth,
        "campaign_id": campaign,
        "evidence_id": evidence,
        "code_sha": code_sha,
        "freeze_manifest_sha256": freeze_sha,
        "record_status": status,
        "retired_pair_ids": list(retired),
        "run_state_root": run_state_root,
    }
    doc.update(over)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    raw = json.dumps(doc, ensure_ascii=False, indent=1).encode("utf-8")
    with open(path, "wb") as fh:
        fh.write(raw)
    return hashlib.sha256(raw).hexdigest()


class LiveEnv:
    """一套 live/正式 dryrun 夹具：快照仓 + manifest + governance + 夹具 exe。"""

    def __init__(self, base, *, dirty=False, extra_source=None):
        self.base = base
        self.repo, self.head = build_snapshot_repo(base)
        self.hdc_exe = make_hdc_fixture(base)
        self.manifest_path, self.manifest_sha = build_manifest(
            base, self.repo, self.head, hdc_executable=self.hdc_exe,
            extra_source=extra_source)
        self.run_state_root = os.path.join(base, "run-state")
        os.makedirs(self.run_state_root, exist_ok=True)
        self.output_root = os.path.join(base, "output")
        self.gov_path = os.path.join(base, "governance.json")
        self.gov_sha = build_governance(
            self.gov_path, code_sha=self.head, freeze_sha=self.manifest_sha,
            run_state_root=self.run_state_root)
        self.probe = probe_dirty(self.head) if dirty else probe_ok(self.head)

    def manifest(self):
        return fm.load_manifest(self.manifest_path,
                                expected_manifest_sha256=self.manifest_sha)

    def campaign(self):
        return self.manifest().data["pair"]["campaign_id"]

    def claim_dir(self):
        return cli.claim_key_path(self.run_state_root, self.campaign())

    def live_argv(self, *, output_root=None, json_path=None, target=FAKE_TARGET):
        argv = ["--live", "--target", target,
                "--freeze-manifest", self.manifest_path,
                "--freeze-sha256", self.manifest_sha,
                "--governance-record", self.gov_path,
                "--governance-sha256", self.gov_sha,
                "--output-root", output_root or self.output_root]
        if json_path is not None:
            argv += ["--json", json_path]
        return argv

    def dryrun_argv(self, *, output_root=None, json_path=None,
                    scenario=None):
        argv = ["--dryrun", "--freeze-manifest", self.manifest_path,
                "--freeze-sha256", self.manifest_sha,
                "--governance-record", self.gov_path,
                "--governance-sha256", self.gov_sha,
                "--output-root", output_root or self.output_root]
        if scenario is not None:
            argv += ["--scenario", scenario]
        if json_path is not None:
            argv += ["--json", json_path]
        return argv


@contextlib.contextmanager
def fixture_scene(tmp, *, scene_over=None, fail_ops=(),
                  scenario=None, no_death=False):
    """fake_hdc_campaign 剧本/状态注入（返回路径供 subprocess env 用）。

    in-process：注入本进程 env（夹具子进程继承）；subprocess：调用方把返回的
    scene/state 路径放进子进程 env。``scenario`` 可换剧本建造器（默认
    death_after_pre；污染 POST 用例传 join-bogus）。``no_death`` = pass 面长驻
    流（见 make_e2e_scene——happy 等无 fault 条目面专用）。"""
    scene = make_e2e_scene(tmp, scenario=scenario, no_death=no_death)
    if scene_over:
        scene.update(scene_over)
    if fail_ops:
        scene["fail_ops"] = list(fail_ops)
    scene_path = os.path.join(tmp, "scene.json")
    state_path = os.path.join(tmp, "state.json")
    with open(scene_path, "w", encoding="utf-8") as fh:
        json.dump(scene, fh, ensure_ascii=False)
    injected = {"scene_path": scene_path, "state_path": state_path}
    os.environ[cli_fake.ENV_SCENE] = scene_path
    os.environ[cli_fake.ENV_STATE] = state_path
    try:
        yield injected
    finally:
        os.environ.pop(cli_fake.ENV_SCENE, None)
        os.environ.pop(cli_fake.ENV_STATE, None)


def make_e2e_scene(tmp, *, scenario=None, no_death=False):
    """端到端剧本（producer 格式真实 marker 流；缺省 PRE 后死亡。m1 整改后
    rc=0 早退且无 POST/到点不再背书静默 → engine 判 F9 fail-closed——需要
    pass 面的用例显式传 POST 在场剧本（如 happy）。传 join-bogus 建造器 →
    POST join=pending 域外污染流）。``no_death`` = 流长驻、无行尽死亡与无
    crash 条目物化（happy/pass 面专用）：POST 即末条 marker，capture 见 POST
    即 stream.close() SIGKILL——行尽死亡路径的 _materialize_crash_files
    （open("w") 无 fsync）与之竞态 → 空/半截 fault 条目 → F8
    fault-entry-time-unparsable 间歇失败；happy 本就不应有 fault 条目，故
    pass 面不背死亡物化（die_after_lines_keep_stream 仍会行尽物化，同样竞态，
    不采用）。die 面（pre-only/storm/join-bogus 等）保持行尽物化路径不动。"""
    scenario = scenario if scenario is not None \
        else fake.make_death_after_pre_scenario
    stream_scenario = scenario()
    stream_transport = fake.FakeHdc(stream_scenario)
    argv = hdc.build_argv("HilogStream", target=stream_transport.target,
                          hap_path=stream_transport.hap_path)
    lines = list(stream_transport.open_stream(argv))
    scene = {
        "bundle": hdc.BUNDLE,
        "model": "E2E-MODEL-CLI",
        "sw": "7.0.0.999",
        "vpn_pid": 24567,
        "ui_pid": 24566,
        "staging_root": hdc.STAGING_ROOT,
        "staging_sandbox": os.path.join(tmp, "staging-sandbox"),
        "fault_dir": os.path.join(tmp, "faultlogger-sandbox"),
        "stream": [{"after_ms": 120 + i * 120, "line": ln}
                   for i, ln in enumerate(lines)],
    }
    if no_death:
        return scene   # 纯长驻（fixture sleep 到被 SIGKILL）：零死亡物化
    scene["crash_files"] = {
        "faultlogger-7001-cn.alfadb.netbird.n1bdisc": fake.fault_entry_text(
            "APPFREEZE", "SIGKILL", "2026-09-06 10:00:00.000"),
    }
    scene["die_after_lines"] = True
    return scene


# --------------------------------------------------------------------------
# 记录/输出断言 helpers
# --------------------------------------------------------------------------

def verify_seal(root):
    """封签清单逐文件 sha256 复核；返回登记相对路径集合。"""
    with open(os.path.join(root, "manifest.json"), "r", encoding="utf-8") as fh:
        manifest = json.load(fh)
    for rel, entry in manifest["files"].items():
        path = os.path.join(root, rel)
        expect(sha_file(path) == entry["sha256"], "清单 hash 一致: %s" % rel)
    return set(manifest["files"])


def read_result(root):
    with open(os.path.join(root, "result.json"), encoding="utf-8") as fh:
        return json.load(fh)


def read_jsonl(path):
    with open(path, encoding="utf-8") as fh:
        return [json.loads(ln) for ln in fh if ln.strip()]


def event_labels(root):
    return [e["label"] for e in read_jsonl(os.path.join(root, "events.jsonl"))]


def assert_no_target(blob_or_text, note):
    data = blob_or_text
    if isinstance(data, str):
        data = data.encode("utf-8")
    expect(FAKE_TARGET.encode() not in data, "target 不出现在 %s" % note)


def assert_tree_clean(root, note):
    """目录树内所有普通文件逐字节不含 target。"""
    for dirpath, _dirs, filenames in os.walk(root):
        for name in filenames:
            with open(os.path.join(dirpath, name), "rb") as fh:
                assert_no_target(fh.read(), "%s/%s" % (note, name))


def run_main(argv, *, input_fn=None, git_probe=None, repo_root=None):
    """同入口调用 main 并收集 stdout（单行摘要断言用）。返回 (rc, stdout)。"""
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = cli.main(argv=argv, input_fn=input_fn, git_probe=git_probe,
                      repo_root=repo_root)
    return rc, buf.getvalue()


def capture(fn):
    """运行并收集 stdout（薄转发等直调入口用）。返回 (rc, stdout)。"""
    buf = io.StringIO()
    with redirect_stdout(buf):
        rc = fn()
    return rc, buf.getvalue()


def summary_of(stdout):
    lines = [ln for ln in stdout.splitlines() if ln.strip()]
    expect(len(lines) == 1, "控制台恰一行无敏 JSON 摘要: %r" % stdout)
    return json.loads(lines[0])


# ==========================================================================
# 1. --help 与薄转发
# ==========================================================================

def test_help_and_old_entry_forward():
    cli_path = os.path.join(CANDIDATE_ROOT, "spikes", "n1b-disc-phys-hap",
                            "runner", "n1bdisc_cli.py")
    run_path = os.path.join(CANDIDATE_ROOT, "spikes", "n1b-disc-phys-hap",
                            "runner", "n1bdisc_run.py")
    for path in (cli_path, run_path):
        proc = subprocess.run([sys.executable, path, "--help"],
                              capture_output=True, text=True, timeout=60)
        expect(proc.returncode == 0, "--help exit 0: %s" % path)
        for flag in ("--live", "--dryrun", "--freeze-manifest",
                     "--freeze-sha256", "--governance-record",
                     "--governance-sha256", "--output-root", "--target",
                     "--json"):
            expect(flag in proc.stdout, "%s 帮助含 %s" % (os.path.basename(path),
                                                          flag))
    # 新 CLI 帮助明确声明两种 dryrun 形态语义
    proc = subprocess.run([sys.executable, cli_path, "--help"],
                          capture_output=True, text=True, timeout=60)
    expect("gate11" in proc.stdout.replace("门 11", "gate11").lower()
           or "11" in proc.stdout, "帮助含门 11 资格说明")


# ==========================================================================
# 2. argparse 用法门（exit 2；零副作用）
# ==========================================================================

def test_argparse_rejections_zero_side_effects():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        out = os.path.join(tmp, "out")
        cases = [
            # 冻结四件套部分提供（live 与 dryrun 同拒）
            ["--live", "--target", FAKE_TARGET, "--freeze-manifest",
             env.manifest_path, "--output-root", out],
            ["--live", "--target", FAKE_TARGET, "--freeze-manifest",
             env.manifest_path, "--freeze-sha256", env.manifest_sha,
             "--governance-record", env.gov_path, "--output-root", out],
            ["--dryrun", "--freeze-manifest", env.manifest_path,
             "--output-root", out],
            # live 形态互斥 / 缺必填
            ["--live", "--target", FAKE_TARGET, "--scenario", "happy"],
            ["--live", "--freeze-manifest", env.manifest_path,
             "--freeze-sha256", env.manifest_sha, "--governance-record",
             env.gov_path, "--governance-sha256", env.gov_sha,
             "--output-root", out],
            ["--live", "--scenario", "happy"],
            ["--live", "--target", FAKE_TARGET, "--hap", "/tmp/x.hap"],
            # 官方 dryrun 形态
            ["--dryrun", "--freeze-manifest", env.manifest_path,
             "--freeze-sha256", env.manifest_sha, "--governance-record",
             env.gov_path, "--governance-sha256", env.gov_sha],
            ["--dryrun", "--freeze-manifest", env.manifest_path,
             "--freeze-sha256", env.manifest_sha, "--governance-record",
             env.gov_path, "--governance-sha256", env.gov_sha,
             "--output-root", out, "--target", FAKE_TARGET],
            # SHA 格式门（截断/大写）
            ["--live", "--target", FAKE_TARGET, "--freeze-manifest",
             env.manifest_path, "--freeze-sha256", "0" * 63,
             "--governance-record", env.gov_path,
             "--governance-sha256", env.gov_sha, "--output-root", out],
            ["--live", "--target", FAKE_TARGET, "--freeze-manifest",
             env.manifest_path, "--freeze-sha256", "A" * 64,
             "--governance-record", env.gov_path,
             "--governance-sha256", env.gov_sha, "--output-root", out],
            # 模式互斥 / 缺模式
            ["--live", "--dryrun"],
            ["--scenario", "happy"],
        ]
        for argv in cases:
            expect_exit(lambda a=argv: cli.main(argv=a), 2, note=str(argv[:2]))
        expect(not os.path.exists(out), "用法拒绝零副作用（无 run 目录）")
        expect(not os.path.exists(env.claim_dir()), "用法拒绝零消费（无 claim）")


# ==========================================================================
# 3. live 拒启门（manifest/governance/git；全部零 HDC 零消费）
# ==========================================================================

def test_live_manifest_level_refusals():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        # manifest 文件缺失
        argv = env.live_argv(json_path=os.path.join(tmp, "j.json"))
        argv[argv.index("--freeze-manifest") + 1] = os.path.join(tmp,
                                                                 "missing.json")
        rc, out = run_main(argv, input_fn=confirm_input("x", "y"),
                           git_probe=env.probe, repo_root=env.repo)
        expect(rc == cli.EXIT_REFUSED, "manifest 缺失拒启 rc=3: %s" % rc)
        s = summary_of(out)
        expect(s["outcome"] == "refused" and s["reason"] == "preflight-failed",
               "拒启摘要: %s" % s)
        expect("manifest-not-found" in s["failures"], "失败码 manifest-not-found")
        assert_no_target(out, "拒启 stdout")
        # 外部 freeze-sha256 不符
        argv = env.live_argv()
        argv[argv.index("--freeze-sha256") + 1] = "0" * 64
        rc, out = run_main(argv, input_fn=confirm_input("x", "y"),
                           git_probe=env.probe, repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED
               and "manifest-sha256-mismatch" in s["failures"],
               "错 hash 拒启: %s" % s)
        # governance-sha256 不符
        argv = env.live_argv()
        argv[argv.index("--governance-sha256") + 1] = "1" * 64
        rc, out = run_main(argv, input_fn=confirm_input("x", "y"),
                           git_probe=env.probe, repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED
               and "governance-sha256-mismatch" in s["failures"],
               "governance 错 hash 拒启: %s" % s)
        expect(not os.path.exists(env.output_root), "拒启不建 run 目录")
        expect(not os.path.exists(env.claim_dir()), "拒启零消费")


def test_live_governance_binding_refusals():
    with sandbox() as tmp:
        base_cases = [
            # 三 ID 格式合法但与本 manifest pair 不同段（同日期不同序号）
            ("governance-pair-mismatch",
             dict(campaign="N1BDISC-PHYS1API26-20991231-0098",
                  auth="AUTH-N1BDISC-PHYS1API26-20991231-0098",
                  evidence="EV-N1BDISC-PHYS1API26-20991231-0098")),
            ("governance-status-not-usable", dict(status="consumed")),
            ("governance-pair-retired", dict(retired=(RETIRED_PAIR, CAMPAIGN))),
            ("governance-retired-required", dict(retired=())),
            ("governance-retired-format", dict(retired=("OLD-PAIR-1",))),
            ("governance-run-state-root-invalid", dict(run_state_root="rel/path")),
        ]
        for want_code, over in base_cases:
            with sandbox(prefix="n1b-cli-gov-") as t2:
                env = LiveEnv(t2)
                root_arg = over.pop("run_state_root", None)
                env.gov_sha = build_governance(
                    env.gov_path, code_sha=env.head,
                    freeze_sha=env.manifest_sha,
                    run_state_root=root_arg or env.run_state_root, **over)
                rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                    "LIVE " + env.campaign(), "OPERATOR-READY"),
                    git_probe=env.probe, repo_root=env.repo)
                s = summary_of(out)
                expect(rc == cli.EXIT_REFUSED,
                       "%s 拒启 rc: %s %s" % (want_code, rc, s))
                expect(want_code in s["failures"],
                       "失败码 %s: %s" % (want_code, s["failures"]))
                expect(not os.path.exists(env.claim_dir()),
                       "%s 零消费" % want_code)
        # run-state 根目录缺失（形态合法但不存在的绝对路径）
        with sandbox(prefix="n1b-cli-gov2-") as t2:
            env = LiveEnv(t2)
            env.gov_sha = build_governance(
                env.gov_path, code_sha=env.head, freeze_sha=env.manifest_sha,
                run_state_root=os.path.join(t2, "missing-state-root"))
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            s = summary_of(out)
            expect(rc == cli.EXIT_REFUSED
                   and "governance-run-state-root-missing" in s["failures"],
                   "run-state 根缺失拒启: %s" % s)
        # code_sha / freeze sha 绑定漂移
        with sandbox(prefix="n1b-cli-gov3-") as t2:
            env = LiveEnv(t2)
            env.gov_sha = build_governance(
                env.gov_path, code_sha="b" * 40, freeze_sha=env.manifest_sha,
                run_state_root=env.run_state_root)
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            s = summary_of(out)
            expect("governance-code-sha-mismatch" in s["failures"],
                   "code_sha 绑定漂移拒启: %s" % s)
            env.gov_sha = build_governance(
                env.gov_path, code_sha=env.head, freeze_sha="c" * 64,
                run_state_root=env.run_state_root)
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            s = summary_of(out)
            expect("governance-freeze-sha-mismatch" in s["failures"],
                   "freeze sha 绑定漂移拒启: %s" % s)


def test_live_git_gate_refusals():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        # dirty 树（注入探针返回 dirty=True）
        rc, out = run_main(env.live_argv(), input_fn=confirm_input(
            "LIVE " + env.campaign(), "OPERATOR-READY"),
            git_probe=probe_dirty(env.head), repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED and "dirty-tree" in s["failures"],
               "dirty 拒启: %s" % s)
        # code_sha 不符（探针 HEAD ≠ manifest code_sha）
        other = "f" * 40 if env.head != "f" * 40 else "e" * 40
        rc, out = run_main(env.live_argv(), input_fn=confirm_input(
            "LIVE " + env.campaign(), "OPERATOR-READY"),
            git_probe=probe_ok(other), repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED and "code-sha-mismatch" in s["failures"],
               "code_sha 不符拒启: %s" % s)
        # git 探针失败（生产真实来源失败面）
        def boom(_repo_root):
            raise cli.GitProbeError("git-probe-failed", "no repo")
        rc, out = run_main(env.live_argv(), input_fn=confirm_input("x", "y"),
                           git_probe=boom, repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED and "git-probe-failed" in s["failures"],
               "git 探针失败拒启: %s" % s)
        expect(not os.path.exists(env.claim_dir()), "git 门拒启零消费")


# ==========================================================================
# 4. 确认门（文件 hash ≠ 人类批准；确认不符零消费）
# ==========================================================================

def test_live_confirmation_gating():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        # LIVE 行 campaign id 不符
        rc, out = run_main(env.live_argv(), input_fn=confirm_input(
            "LIVE N1BDISC-PHYS1API26-20991231-0098", "OPERATOR-READY"),
            git_probe=env.probe, repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED
               and s["reason"] == "live-confirmation-mismatch",
               "LIVE 行不符拒启: %s" % s)
        # operator-ready 行不符（LIVE 行正确）
        rc, out = run_main(env.live_argv(), input_fn=confirm_input(
            "LIVE " + env.campaign(), "operator-ready"),
            git_probe=env.probe, repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED
               and s["reason"] == "operator-ready-not-confirmed",
               "operator-ready 不符拒启: %s" % s)
        expect(not os.path.exists(env.claim_dir()),
               "确认不符发生在 claim 之前（零消费）")
        expect(not os.path.exists(env.output_root), "确认不符不建 run 目录")
        # 确认不可得（EOF）
        rc, out = run_main(env.live_argv(), input_fn=confirm_input(),
                           git_probe=env.probe, repo_root=env.repo)
        expect(rc == cli.EXIT_REFUSED, "确认 EOF 拒启: %s" % rc)


# ==========================================================================
# 5. live 完整生命周期（in-process；fixture 唯一 hdc 角色进程）
# ==========================================================================

def test_live_full_lifecycle_inprocess():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        json_path = os.path.join(tmp, "final.json")
        # POST 在场剧本（complete pass 面）：m1 整改后 rc=0 早退死亡流不再
        # 背书静默（→F9 fail-closed），live 全生命周期 pass 面用 happy 承载；
        # no_death = 长驻流零死亡物化（POST 收口与行尽物化竞态 → 空/半截
        # fault 条目 → F8 间歇，happy 不应有 fault 条目）。
        with fixture_scene(tmp, scenario=fake.make_happy_path_scenario,
                           no_death=True) as scene:
            rc, out = run_main(env.live_argv(json_path=json_path),
                               input_fn=confirm_input(
                                   "LIVE " + env.campaign(), "OPERATOR-READY"),
                               git_probe=env.probe, repo_root=env.repo)
            expect(rc == cli.EXIT_OK, "live exit 0: %s / %s" % (rc, out))
            s = summary_of(out)
            expect(s["outcome"] == "completed" and s["verdict"] == "pass"
                   and s["freeze_bound"] is True and s["live_started"] is True,
                   "成功摘要: %s" % s)
            expect(s["claim_status"] == "start-entry-attempted",
                   "claim 已推进: %s" % s)
            root = env.output_root
            expect(os.path.isdir(root), "run 目录存在")
            files = verify_seal(root)
            expect({"capture.jsonl", "events.jsonl", "state.json",
                    "result.json", "metadata.json", "preflight.json"}
                   <= files, "封签覆盖 preflight.json: %s" % sorted(files))
            # preflight.json 独立文件 + ok
            with open(os.path.join(root, "preflight.json"),
                      encoding="utf-8") as fh:
                pf = json.load(fh)
            expect(pf["ok"] is True and pf["mode"] == "live"
                   and pf["manifest_sha256"] == env.manifest_sha
                   and pf["git"]["dirty"] is False,
                   "preflight.json 内容: %s" % pf["ok"])
            # result.json：live 证据面 + integrity 收口复核结论（本增量起 live
            # integrity 非空：precheck/postcheck 真实通过、hash 与绑定一致）
            doc = read_result(root)
            rec = doc["result"]
            integ = rec["integrity"]
            expect(doc["terminal"] == "complete" and rec["verdict"] == "pass"
                   and rec["is_evidence"] is True,
                   "live result 面")
            expect(integ == {"schema_version": 1,
                             "manifest_sha256": env.manifest_sha,
                             "governance_sha256": env.gov_sha,
                             "precheck": "passed", "postcheck": "passed",
                             "violations": []},
                   "live integrity pre/post 真实通过且 hash 绑定一致: %s" % integ)
            expect(pf["manifest_sha256"] == integ["manifest_sha256"],
                   "integrity.manifest_sha256 == preflight 绑定值")
            with open(os.path.join(root, "metadata.json"),
                      encoding="utf-8") as fh:
                meta = json.load(fh)
            expect(meta["freeze_hash"] == integ["manifest_sha256"],
                   "integrity.manifest_sha256 == metadata freeze_hash")
            expect(rec["join_boundary"]["join_exit_rc"] is None
                   and rec["join_boundary"]["join_blocked_registered"] is False,
                   "live join 边界冻结值")
            ops = rec["hdc_audit_ops"]
            expect("HilogStream" in ops and "StartEntry" in ops
                   and ops.index("HilogStream") < ops.index("StartEntry"),
                   "审计含 HilogStream 且先于 StartEntry: %s" % ops)
            # claim 事实（governance 受控根内）
            claim = cli.read_claim(env.claim_dir())
            expect(claim["status"] == "start-entry-attempted"
                   and claim["campaign_id"] == env.campaign()
                   and claim["output_root"] == root,
                   "claim 事实: %s" % claim["status"])
            # --json 最终记录：cli/preflight 字段 + 无 target
            with open(json_path, encoding="utf-8") as fh:
                final = json.load(fh)
            expect(final["cli"]["freeze_bound"] is True
                   and final["cli"]["live_started"] is True
                   and final["preflight"]["ok"] is True,
                   "--json cli/preflight 字段")
            assert_no_target(open(json_path, "rb").read(), "--json 文件")
            assert_no_target(out, "stdout")
            assert_tree_clean(root, "run 目录")
            assert_tree_clean(env.run_state_root, "run-state 根")
            expect(leftover_fixture_procs(tmp) == [],
                   "夹具子进程组全部回收")


# ==========================================================================
# 6. 单次性：换 output-root 不能绕过；StartEntry 失败不重发
# ==========================================================================

def test_live_second_run_new_output_root_refused():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        # 首次 run 需 pass 面（claim 消费前提）→ POST 在场剧本（同上 m1 注；
        # no_death 长驻流，理由同 test_live_full_lifecycle_inprocess）。
        with fixture_scene(tmp, scenario=fake.make_happy_path_scenario,
                           no_death=True):
            rc, _ = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            expect(rc == cli.EXIT_OK, "首次 live 成功")
            other_root = os.path.join(tmp, "output-2")
            rc, out = run_main(env.live_argv(output_root=other_root),
                               input_fn=confirm_input(
                                   "LIVE " + env.campaign(),
                                   "OPERATOR-READY"),
                               git_probe=env.probe, repo_root=env.repo)
            s = summary_of(out)
            expect(rc == cli.EXIT_REFUSED
                   and "pair-claim-exists" in s["failures"],
                   "换 output-root 仍拒绝: %s" % s)
            expect(not os.path.exists(other_root), "拒绝不建新 run 目录")
            expect(cli.read_claim(env.claim_dir())["status"]
                   == "start-entry-attempted", "原 claim 状态不被二次触碰")


def test_live_startentry_failure_no_retry():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        with fixture_scene(tmp, fail_ops=("StartEntry",)):
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            expect(rc == cli.EXIT_FAIL, "StartEntry 失败 → verdict fail exit 1: "
                   "%s %s" % (rc, out))
            root = env.output_root
            doc = read_result(root)
            rec = doc["result"]
            expect(doc["terminal"] == "incomplete" and rec["verdict"] == "fail",
                   "incomplete 显式封签")
            facts = rec["steps"]["campaign_facts"]
            expect(facts["launch_aborted"] is not None
                   and "StartEntry" in facts["launch_aborted"],
                   "启动中止事实: %s" % facts["launch_aborted"])
            expect(event_labels(root).count("StartEntry") == 1,
                   "StartEntry 恰一次尝试（事件面无重发）")
            expect("HilogStream" in event_labels(root),
                   "开流已发生（先于失败点）")
            for op in ("ForceStop", "Uninstall", "RemoveStaging"):
                expect(op in rec["hdc_audit_ops"], "finally 清理 %s" % op)
            claim = cli.read_claim(env.claim_dir())
            expect(claim["status"] == "start-entry-attempted",
                   "发送前 claim 已推进（不确定是否发出也不重试）")
            verify_seal(root)
            # 消费后再启动（即使换 output-root）一律拒绝
            rc, out = run_main(env.live_argv(output_root=os.path.join(
                tmp, "output-3")), input_fn=confirm_input(
                    "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            s = summary_of(out)
            expect(rc == cli.EXIT_REFUSED
                   and "pair-claim-exists" in s["failures"],
                   "失败后 pair 已消费: %s" % s)


# ==========================================================================
# 7. 正式 dryrun（freeze-bound）：同一预检、零消费、integrity={} 原字面
# ==========================================================================

def test_dryrun_official_freeze_bound():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        json_path = os.path.join(tmp, "final.json")
        rc, out = run_main(env.dryrun_argv(json_path=json_path),
                           git_probe=env.probe, repo_root=env.repo)
        expect(rc == cli.EXIT_OK, "正式 dryrun exit 0: %s %s" % (rc, out))
        s = summary_of(out)
        expect(s["freeze_bound"] is True and s["gate11_eligible"] is True,
               "正式 dryrun 资格: %s" % s)
        root = env.output_root
        files = verify_seal(root)
        expect("preflight.json" in files, "preflight.json 随封签入清单")
        doc = read_result(root)
        rec = doc["result"]
        expect(rec["is_evidence"] is False and rec["integrity"] == {},
               "门 11 原字面不改写")
        expect(rec["mode"] == "dryrun", "engine mode=dryrun（fake transport）")
        with open(json_path, encoding="utf-8") as fh:
            final = json.load(fh)
        expect(final["cli"]["freeze_bound"] is True
               and final["cli"]["gate11_eligible"] is True
               and final["preflight"]["ok"] is True
               and final["cli"]["claim_status"] is None,
               "--json 附带字段")
        expect(not os.path.exists(env.claim_dir()),
               "DryRun 不建真实 pair claim（零消费）")
        # 同一预检拒绝面：dirty
        rc, out = run_main(env.dryrun_argv(output_root=os.path.join(
            tmp, "out-2")), git_probe=probe_dirty(env.head),
            repo_root=env.repo)
        s = summary_of(out)
        expect(rc == cli.EXIT_REFUSED and "dirty-tree" in s["failures"],
               "正式 dryrun 同一预检拒启: %s" % s)


# ==========================================================================
# 8. 无 manifest smoke：明确不具门 11 资格，仍走同一 engine
# ==========================================================================

def test_dryrun_smoke_not_gate11():
    with sandbox() as tmp:
        root = os.path.join(tmp, "smoke-run")
        json_path = os.path.join(tmp, "final.json")
        # m1 整改后：smoke 合成流 rc=0 早退且无 POST/到点 → 静默不可判 →
        # pre-only 死亡面 F9 fail-closed（engine 共同路径的真实 capture 事实）。
        rc, out = run_main(["--dryrun", "--scenario", "pre-only",
                            "--output-root", root, "--json", json_path])
        expect(rc == cli.EXIT_FAIL, "smoke pre-only F9 fail → exit 1: %s %s"
               % (rc, out))
        s = summary_of(out)
        expect(s["freeze_bound"] is False and s["gate11_eligible"] is False,
               "smoke 明确标注: %s" % s)
        verify_seal(root)
        doc = read_result(root)
        rec = doc["result"]
        expect(rec["is_evidence"] is False and rec["integrity"] == {}
               and rec["verdict"] == "fail", "smoke 判定面")
        expect("F9" in rec["steps"]["gate13_verdict"]["gates"],
               "rc=0 早退静默不可判 → F9: %s"
               % rec["steps"]["gate13_verdict"]["gates"])
        with open(os.path.join(root, "metadata.json"), encoding="utf-8") as fh:
            meta = json.load(fh)
        expect(meta["freeze_hash"] == cli.SMOKE_FREEZE_HASH,
               "smoke metadata 不绑定任何冻结")
        expect(not os.path.exists(os.path.join(root, "preflight.json")),
               "smoke 无 preflight 文件")
        with open(json_path, encoding="utf-8") as fh:
            final = json.load(fh)
        expect(final["cli"]["freeze_bound"] is False
               and final["cli"]["gate11_eligible"] is False
               and "preflight" not in final, "--json 资格标注")
        # 反例剧本 fail → exit 1（失败退出码）
        rc, _ = run_main(["--dryrun", "--scenario", "d8b-bogus",
                          "--output-root", os.path.join(tmp, "smoke-bogus")])
        expect(rc == cli.EXIT_FAIL, "d8b-bogus smoke → exit 1: %s" % rc)
        # 旧入口薄转发同语义
        rc, out = capture(lambda: run_mod.main(
            ["--dryrun", "--scenario", "happy", "--output-root",
             os.path.join(tmp, "smoke-fwd")]))
        expect(rc == cli.EXIT_OK, "n1bdisc_run.main 薄转发 exit 0: %s %s"
               % (rc, out))
        summary_of(out)   # 转发后控制台同样只有单行无敏摘要


# ==========================================================================
# 9. 剧本判定等价钉（engine 共同路径 vs 旧 helper 直调；join 十值域恒检查）
# ==========================================================================

def test_smoke_verdict_equivalence_all_scenarios():
    """8 个 CLI 剧本经 engine 与旧 helper 直调 verdict 等价（T0 join-bogus
    回归整改后：POST 在场即 DW_JOIN_RESULT_10 闭域检查，no-fact 只免除比对、
    不免除域检查——join-bogus 两路径一致 fail(F8)）。

    例外 = pre-only（m1 整改后的**有意分歧**，逐字钉）：旧 helper 合成
    "全流=静默可判"（DryRun capture_silent 恒 True），engine 按真实 capture
    事实判——smoke 合成流 rc=0 早退且无 POST/到点 → 静默不可判 → F9
    fail-closed（helper 仍 pass）。"""
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
    with sandbox() as tmp:
        for name in run_mod.SCENARIOS:
            camp = run_mod.run_dryrun_campaign(name)
            old = run_mod.derive_and_judge(camp)["verdict_result"]
            root = os.path.join(tmp, "eq-" + name)
            rc, _ = run_main(["--dryrun", "--scenario", name,
                              "--output-root", root])
            doc = read_result(root)["result"]
            if name == "pre-only":
                expect(old.verdict == "pass" and doc["verdict"] == "fail"
                       and rc == cli.EXIT_FAIL
                       and "F9" in doc["steps"]["gate13_verdict"]["gates"],
                       "pre-only 有意分歧（helper 合成静默 vs engine rc=0 早退"
                       "缺口→F9）: old=%s new=%s gates=%s"
                       % (old.verdict, doc["verdict"],
                          doc["steps"]["gate13_verdict"]["gates"]))
                continue
            expect(rc == (cli.EXIT_OK if old.verdict == "pass"
                          else cli.EXIT_FAIL),
                   "%s 退出码与 verdict 一致: rc=%s" % (name, rc))
            expect(doc["verdict"] == old.verdict
                   and doc["steps"]["gate13_verdict"]["gates"] == old.gates,
                   "%s verdict/gates 等价: old=%s/%s new=%s/%s"
                   % (name, old.verdict, old.gates, doc["verdict"],
                      doc["steps"]["gate13_verdict"]["gates"]))
        # join-bogus 专项：域检查恒在（no-fact 不豁免）→ F8 fail
        rec = read_result(os.path.join(tmp, "eq-join-bogus"))["result"]
        expect("F8" in rec["steps"]["gate13_verdict"]["gates"],
               "join-bogus F8: %s" % rec["steps"]["gate13_verdict"]["gates"])
        expect(rec["dw"]["post_join"] == "pending"
               and rec["dw"]["join_outcome"] == "no-fact"
               and rec["join_boundary"]["join_exit_rc"] is None
               and rec["join_boundary"]["join_blocked_registered"] is False,
               "pending 域外 + no-fact 免比对不免域检查 + join 边界冻结值")
        # 合法 POST join=joined + no-fact：不误判（域内、无比对面、无 F8）
        rec_h = read_result(os.path.join(tmp, "eq-happy"))["result"]
        expect(rec_h["dw"]["post_join"] == "joined"
               and rec_h["dw"]["join_outcome"] == "no-fact"
               and "F8" not in rec_h["steps"]["gate13_verdict"]["gates"]
               and rec_h["verdict"] == "pass",
               "合法 joined + no-fact 不误判: %s/%s"
               % (rec_h["dw"]["post_join"], rec_h["verdict"]))
        # d8b-bogus 从源码确认依然 F8（五值 bogus → 逐字段 F8 fail-closed）
        rec_d = read_result(os.path.join(tmp, "eq-d8b-bogus"))["result"]
        expect(rec_d["steps"]["gate13_verdict"]["gates"] == ["F8"]
               and rec_d["verdict"] == "fail", "d8b-bogus 依然 F8")


def test_live_polluted_post_join_bogus_fail():
    """完整 manifest 路径（RealHdcTransport × fake 可执行）下同污染 POST
    （join=pending 域外）→ F8 fail：exit 1、显式封签、claim 已消费、
    join 边界冻结值不动。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        with fixture_scene(tmp, scenario=fake.make_join_bogus_scenario):
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            expect(rc == cli.EXIT_FAIL, "污染 POST live → exit 1: %s %s"
                   % (rc, out))
            root = env.output_root
            doc = read_result(root)
            rec = doc["result"]
            expect(rec["is_evidence"] is True and rec["verdict"] == "fail"
                   and "F8" in rec["steps"]["gate13_verdict"]["gates"],
                   "live 污染 POST 判定面: %s/%s"
                   % (rec["verdict"], rec["steps"]["gate13_verdict"]["gates"]))
            expect(rec["dw"]["post_join"] == "pending"
                   and rec["join_boundary"]["join_exit_rc"] is None,
                   "polluted post_join 登记且 join 边界不动")
            expect(doc["terminal"] in ("complete", "incomplete"),
                   "显式封签终态: %s" % doc["terminal"])
            verify_seal(root)
            expect(cli.read_claim(env.claim_dir())["status"]
                   == "start-entry-attempted", "claim 已消费")
            assert_tree_clean(root, "polluted live run 目录")
            expect(leftover_fixture_procs(tmp) == [], "无残留夹具进程")


def test_join_domain_gate_pending_bogus_missing_legal():
    """POST join 域门（T0 裁定）：no-fact 只免除比对、不免除十值域检查——
    pending/bogus/缺值（结构违反→join 缺失）均 F8 fail；合法 joined+no-fact
    （EXIT 不喂 join、join_exit_rc=None、join_blocked=False）不误报。"""
    def core_domain():
        from n1bdisc_core import DW_JOIN_RESULT_10
        return set(DW_JOIN_RESULT_10)

    def polluted(override):
        return fake.FakeScenario(markers=fake._clone_markers(
            fake.make_happy_path_scenario()), die_at_end=False,
            join_exit_rc=None, post_join_override=override)

    cases = [
        ("pending", "fail"),        # 域外字面（B4-b 反例本体）
        ("weird-bogus", "fail"),    # 其他域外字面
        ("", "fail"),               # 空值 → 结构违反 → join 缺失
        (None, "pass"),             # 无覆写：合法 joined + no-fact
    ]
    for override, want in cases:
        camp = run_mod.run_dryrun_campaign(
            "join-domain-%s" % (override or "legal"),
            scenario=polluted(override))
        judged = run_mod.derive_and_judge(camp)
        result = judged["verdict_result"]
        post_join = judged["dw"]["post_join"]
        if want == "fail":
            expect(result.verdict == "fail"
                   and "F8" in result.gates
                   and (post_join is None
                        or post_join not in core_domain()),
                   "域外/缺值 %r → F8 fail: %s/%s (post_join=%r)"
                   % (override, result.verdict, result.gates, post_join))
        else:
            expect(result.verdict == "pass" and "F8" not in result.gates
                   and judged["dw"]["post_join"] == "joined"
                   and judged["dw"]["join_outcome"] == "no-fact",
                   "合法 joined + no-fact 不误报: %s" % result.verdict)
    # join 边界冻结值不受域门影响（engine join_facts 恒 False/None）
    camp = run_mod.run_dryrun_campaign("join-domain-legal",
                                       scenario=polluted(None))
    judged = run_mod.derive_and_judge(camp)
    expect(judged["dw"]["join_facts"]["join_blocked_registered"] is False
           and judged["dw"]["join_facts"]["join_exit_rc"] is None,
           "EXIT 不喂 join、join 事实恒 False/None")


# ==========================================================================
# 10. 真 subprocess 端到端 live（tmp git 快照仓 + 真 stdin + 真 transport）
# ==========================================================================

def _cli_script_path():
    return os.path.join(CANDIDATE_ROOT, "spikes", "n1b-disc-phys-hap",
                        "runner", "n1bdisc_cli.py")


def _subprocess_env():
    env = dict(os.environ)
    env.pop(cli_fake.ENV_SCENE, None)
    env.pop(cli_fake.ENV_STATE, None)
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    return env


def test_live_subprocess_end_to_end_snapshot_repo():
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        json_path = os.path.join(tmp, "final.json")
        # 快照仓内的 CLI 脚本：default_repo_root() 解析到快照仓 → 生产真实 git
        # 探针作用于固定快照（clean HEAD）；真 stdin 双确认；真 transport spawn。
        cli_script = os.path.join(env.repo, "spikes", "n1b-disc-phys-hap",
                                  "runner", "n1bdisc_cli.py")
        argv = [sys.executable, cli_script] + env.live_argv(
            json_path=json_path)
        # POST 在场剧本（complete pass 面，m1 注同 test_live_full_lifecycle；
        # no_death 长驻流）。
        scene_ctx = fixture_scene(tmp, scenario=fake.make_happy_path_scenario,
                                  no_death=True)
        with scene_ctx as injected, wall_deadline(180):
            env_dict = _subprocess_env()
            env_dict[cli_fake.ENV_SCENE] = injected["scene_path"]
            env_dict[cli_fake.ENV_STATE] = injected["state_path"]
            proc = subprocess.run(
                argv, cwd=tmp, env=env_dict,
                input="LIVE %s\nOPERATOR-READY\n" % env.campaign(),
                capture_output=True, text=True, timeout=170)
        expect(proc.returncode == cli.EXIT_OK,
               "subprocess live exit 0: rc=%s stdout=%s stderr=%s"
               % (proc.returncode, proc.stdout[-800:], proc.stderr[-800:]))
        s = summary_of(proc.stdout)
        expect(s["verdict"] == "pass" and s["claim_status"]
               == "start-entry-attempted", "subprocess 摘要: %s" % s)
        expect(s["run_root"] == env.output_root, "run-root 即 --output-root")
        doc = read_result(env.output_root)
        expect(doc["result"]["verdict"] == "pass"
               and doc["result"]["is_evidence"] is True,
               "subprocess live 判定面")
        integ = doc["result"]["integrity"]
        expect(integ.get("postcheck") == "passed"
               and integ.get("precheck") == "passed"
               and integ.get("manifest_sha256") == env.manifest_sha
               and integ.get("governance_sha256") == env.gov_sha
               and integ.get("violations") == [],
               "subprocess live integrity 收口真实通过: %s" % integ)
        verify_seal(env.output_root)
        claim = cli.read_claim(env.claim_dir())
        expect(claim["status"] == "start-entry-attempted", "claim 已消费")
        assert_no_target(proc.stdout, "subprocess stdout")
        assert_no_target(proc.stderr, "subprocess stderr")
        assert_tree_clean(env.output_root, "subprocess run 目录")
        assert_tree_clean(env.run_state_root, "subprocess run-state 根")
        assert_no_target(open(json_path, "rb").read(), "subprocess --json")
        expect(leftover_fixture_procs(tmp) == [], "无残留夹具进程")
        # 真 git 探针生效：快照仓应 clean；preflight 记录真实 HEAD
        with open(os.path.join(env.output_root, "preflight.json"),
                  encoding="utf-8") as fh:
            pf = json.load(fh)
        expect(pf["git"]["source"] == "git-rev-parse+status"
               and pf["git"]["head_sha256"] == env.head
               and pf["git"]["dirty"] is False,
               "生产 git 探针事实: %s" % pf["git"])


def test_live_subprocess_refused_on_candidate_worktree():
    """真候选仓现实：对当前候选工作区构造的 manifest 在任何 hdc 之前拒启。

    当前候选工作区为脏树（本增量未提交）→ dirty-tree 必拒；同时 manifest 额外
    携带一条不存在于实际候选集合的 runner 条目 → runner-file-extra 独立于脏树
    恒拒（测试不依赖「脏树恰好成立」）。零 fixture spawn、零消费。"""
    with sandbox() as tmp:
        head = _git(["rev-parse", "HEAD"], CANDIDATE_ROOT)
        spike = os.path.join(CANDIDATE_ROOT, "spikes", "n1b-disc-phys-hap")
        sources = {}
        for d in ("runner", "selftests"):
            dd = os.path.join(spike, d)
            for name in sorted(os.listdir(dd)):
                p = os.path.join(dd, name)
                if name.endswith(".py") and os.path.isfile(p):
                    rel = "spikes/n1b-disc-phys-hap/%s/%s" % (d, name)
                    with open(p, "r", encoding="utf-8") as fh:
                        sources[rel[len("spikes/n1b-disc-phys-hap/"):]] = \
                            fh.read()
        rel = "spikes/n1b-disc-phys-hap/staticcheck/check_static.py"
        with open(os.path.join(spike, "staticcheck", "check_static.py"),
                  encoding="utf-8") as fh:
            sources[rel[len("spikes/n1b-disc-phys-hap/"):]] = fh.read()
        arts = tfm.build_artifacts(tmp)
        arts["hdc_binary"] = make_hdc_fixture(tmp)
        data = tfm.manifest_dict(sources, arts, code_sha=head)
        data["pair"] = {"pair_number": 99, "authorization_id": AUTH,
                        "campaign_id": CAMPAIGN, "evidence_id": EVIDENCE}
        # 代码体已按实际候选集合列出——这里再额外注入一条候选集合外的条目，
        # 使 runner 精确集合核验独立于脏树状态恒拒。
        data["runner_sources"].append({
            "path": "spikes/n1b-disc-phys-hap/runner/bogus_not_present.py",
            "sha256": "a" * 64})
        data["runner_sources"].sort(key=lambda e: e["path"])
        manifest_path, manifest_sha = tfm.write_manifest(tmp, data)
        run_state_root = os.path.join(tmp, "run-state")
        os.makedirs(run_state_root, exist_ok=True)
        gov_path = os.path.join(tmp, "governance.json")
        gov_sha = build_governance(gov_path, code_sha=head,
                                   freeze_sha=manifest_sha,
                                   run_state_root=run_state_root)
        output_root = os.path.join(tmp, "output")
        argv = [sys.executable, _cli_script_path(), "--live",
                "--target", FAKE_TARGET,
                "--freeze-manifest", manifest_path,
                "--freeze-sha256", manifest_sha,
                "--governance-record", gov_path,
                "--governance-sha256", gov_sha,
                "--output-root", output_root,
                "--json", os.path.join(tmp, "refused.json")]
        with wall_deadline(120):
            proc = subprocess.run(argv, cwd=tmp, env=_subprocess_env(),
                                  input="LIVE %s\nOPERATOR-READY\n" % CAMPAIGN,
                                  capture_output=True, text=True, timeout=110)
        expect(proc.returncode == cli.EXIT_REFUSED,
               "候选仓现实拒启 rc=3: %s %s" % (proc.returncode, proc.stdout))
        s = summary_of(proc.stdout)
        expect("runner-file-extra" in s["failures"],
               "runner 精确集合失败码在场: %s" % s["failures"])
        dirty_now = bool(subprocess.run(
            ["git", "-C", CANDIDATE_ROOT, "status", "--porcelain"],
            capture_output=True, text=True).stdout.strip())
        if dirty_now:
            expect("dirty-tree" in s["failures"],
                   "当前候选仓为脏树 → dirty-tree 必在场: %s" % s["failures"])
        expect(not os.path.exists(output_root), "拒启不建 run 目录")
        expect(not os.path.exists(cli.claim_key_path(run_state_root,
                                                     CAMPAIGN)),
               "拒启零消费")
        assert_no_target(proc.stdout, "拒启 stdout")
        expect(leftover_fixture_procs(tmp) == [], "零 fixture spawn")


# ==========================================================================
# 11. live integrity 收口复核（漂移/篡改/异常 → 既有 verdict invalid 轴）
# ==========================================================================

def _signed_hap_path(env):
    manifest = env.manifest()
    return next(e["path"] for e in manifest.data["artifacts"]
                if e["role"] == "signed_hap")


def _flip_byte(path, offset_from_end=0):
    """翻转文件内一字节（同长度；不改文件名/路径）。"""
    with open(path, "rb") as fh:
        data = bytearray(fh.read())
    idx = len(data) - 1 - offset_from_end
    data[idx] ^= 0x01
    with open(path, "wb") as fh:
        fh.write(bytes(data))
    return sha_file(path)


def _mutate_hap_content_byte(hap_path):
    """翻转 HAP 内 module.json 内容区一字节（同长度：zip 结构/成员集/.so 成员
    hash 不变，仅 artifact 文件 bytes 漂移 → artifact-hash-mismatch 单码）。"""
    with open(hap_path, "rb") as fh:
        data = bytearray(fh.read())
    marker = b'{"fake": true}'
    idx = data.find(marker)
    expect(idx >= 0, "HAP 夹具含 module.json 内容标记")
    data[idx + 2] ^= 0x01
    with open(hap_path, "wb") as fh:
        fh.write(bytes(data))


def mutate_hap_when_event_written(hap_path, events_jsonl_path, *,
                                  marker="SendHap", deadline_s=60.0):
    """运行途中篡改 watcher：RunRecorder ``events.jsonl``（append-only）出现
    ``marker`` 事件（SendHap 完成 = HAP 已上设备）即翻转 HAP 一字节。

    用 append-only 事件流而非 state.json 相位：事件行一旦写入不会消失，无
    「相位窗口被调度饿死错过」的 flake。返回 (thread, result)；
    result["mutated"]/result["error"] 供 join 后断言。生产 CLI 无任何跳过/
    override 开关——mutation 只发生在测试侧持有的 tmp artifact 字节上
    （preflight 校验之后、收口复核之前）。
    """
    result = {"mutated": False, "error": None}

    def _watch():
        end = time.monotonic() + deadline_s
        reached = False
        while time.monotonic() < end:
            try:
                with open(events_jsonl_path, "r", encoding="utf-8") as fh:
                    if marker in fh.read():
                        reached = True
                        break
            except (OSError, ValueError):
                pass
            time.sleep(0.01)
        if not reached:
            result["error"] = "event %r never observed within %.0fs" % (
                marker, deadline_s)
            return
        try:
            _mutate_hap_content_byte(hap_path)
            result["mutated"] = True
        except OSError as exc:
            result["error"] = repr(exc)

    thread = threading.Thread(target=_watch, daemon=True)
    thread.start()
    return thread, result


def assert_invalid_by_freeze_integrity(rec, *, want_protocol=None):
    """verdict invalid 优先轴钉：gates 恰 ["invalid"]、细节为 freeze-integrity。"""
    gv = rec["steps"]["gate13_verdict"]
    expect(rec["verdict"] == "invalid" and gv["gates"] == ["invalid"],
           "既有 verdict invalid 优先轴: %s/%s" % (rec["verdict"], gv["gates"]))
    expect(any(d.startswith("freeze-integrity: ")
               for d in gv["details"].get("invalid", [])),
           "invalid 细节含 freeze-integrity 失败码: %s" % gv["details"])
    if want_protocol is not None:
        expect(rec["protocol"] == want_protocol,
               "协议事实不受 integrity 影响: %s" % rec["protocol"])


def test_live_integrity_hap_drift_midrun_invalid():
    """运行途中篡改 signed HAP 一字节 → postcheck 失败 → 既有 verdict invalid
    优先轴（协议正常——pre-only——也非 pass）；清理与封签照常、claim 已消费。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        hap_path = _signed_hap_path(env)
        with fixture_scene(tmp) as injected:
            thread, res = mutate_hap_when_event_written(
                hap_path, os.path.join(env.output_root, "events.jsonl"))
            rc, out = run_main(env.live_argv(json_path=os.path.join(
                tmp, "final.json")), input_fn=confirm_input(
                    "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            thread.join(timeout=65)
        expect(res["mutated"] and not res["error"],
               "watcher 运行途中完成篡改: %s" % res)
        expect(rc == cli.EXIT_FAIL, "漂移 → exit 1: %s %s" % (rc, out))
        s = summary_of(out)
        expect(s["outcome"] == "completed" and s["verdict"] == "invalid",
               "摘要: %s" % s)
        root = env.output_root
        doc = read_result(root)
        rec = doc["result"]
        assert_invalid_by_freeze_integrity(rec, want_protocol="pre-only")
        integ = rec["integrity"]
        expect(integ["schema_version"] == 1
               and integ["precheck"] == "passed"
               and integ["postcheck"] == "failed"
               and integ["manifest_sha256"] == env.manifest_sha
               and integ["governance_sha256"] == env.gov_sha
               and "verify: artifact-hash-mismatch" in integ["violations"],
               "integrity 失败面（manifest bytes 未漂移，artifact 漂移）: %s"
               % integ)
        expect(rec["is_evidence"] is True, "live 证据面")
        for op in ("ForceStop", "Uninstall", "RemoveStaging"):
            expect(op in rec["hdc_audit_ops"], "finally 清理 %s 照常" % op)
        verify_seal(root)
        expect(doc["terminal"] in ("complete", "incomplete"), "显式封签")
        expect(cli.read_claim(env.claim_dir())["status"]
               == "start-entry-attempted", "claim 已消费")
        with open(os.path.join(tmp, "final.json"), encoding="utf-8") as fh:
            final = json.load(fh)
        expect(final["integrity"] == integ, "--json 同一 integrity 面")
        expect(leftover_fixture_procs(tmp) == [], "无残留夹具进程")


def test_live_integrity_invalid_priority_over_protocol_fail():
    """invalid 优先于协议面 fail：污染 POST（join=pending → F8）∧ 漂移 →
    verdict invalid（invalid 轴先于 F8），非 fail。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        hap_path = _signed_hap_path(env)
        with fixture_scene(tmp, scenario=fake.make_join_bogus_scenario) as inj:
            thread, res = mutate_hap_when_event_written(
                hap_path, os.path.join(env.output_root, "events.jsonl"))
            rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                "LIVE " + env.campaign(), "OPERATOR-READY"),
                git_probe=env.probe, repo_root=env.repo)
            thread.join(timeout=65)
        expect(res["mutated"] and not res["error"], "watcher 篡改完成: %s" % res)
        expect(rc == cli.EXIT_FAIL, "invalid → exit 1: %s" % rc)
        rec = read_result(env.output_root)["result"]
        gv = rec["steps"]["gate13_verdict"]
        expect(rec["verdict"] == "invalid" and gv["gates"] == ["invalid"],
               "invalid 优先于 F8: %s/%s" % (rec["verdict"], gv["gates"]))
        expect(rec["integrity"]["postcheck"] == "failed"
               and rec["integrity"]["violations"],
               "integrity postcheck failed: %s" % rec["integrity"])


def test_live_integrity_manifest_gov_tamper_invalid():
    """收口时 manifest/governance bytes 被改 → expected hash 不符 → invalid；
    integrity 记录观测到的漂移 bytes hash（证据，非授权来源）。

    篡改挂点 = 包裹 ``cli.make_freeze_revalidate`` 工厂：在 revalidate 入口
    （preflight 校验之后、收口复核之前）翻转两个文件各一字节——不依赖任何
    调用计数，生产 CLI 不暴露跳过/override 开关。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        real_make = cli.make_freeze_revalidate

        def make_wrapper(**kw):
            inner = real_make(**kw)

            def revalidate():
                _flip_byte(env.manifest_path)
                _flip_byte(env.gov_path)
                return inner()
            return revalidate

        cli.make_freeze_revalidate = make_wrapper
        try:
            with fixture_scene(tmp):
                rc, out = run_main(env.live_argv(), input_fn=confirm_input(
                    "LIVE " + env.campaign(), "OPERATOR-READY"),
                    git_probe=env.probe, repo_root=env.repo)
        finally:
            cli.make_freeze_revalidate = real_make
        expect(rc == cli.EXIT_FAIL, "篡改 → exit 1: %s %s" % (rc, out))
        rec = read_result(env.output_root)["result"]
        assert_invalid_by_freeze_integrity(rec)
        integ = rec["integrity"]
        expect("manifest-revalidate: manifest-sha256-mismatch"
               in integ["violations"]
               and "governance-revalidate: governance-sha256-mismatch"
               in integ["violations"],
               "expected hash 不符失败码: %s" % integ["violations"])
        expect(integ["manifest_sha256"] == sha_file(env.manifest_path)
               and integ["manifest_sha256"] != env.manifest_sha,
               "integrity 记录观测漂移 manifest bytes hash")
        expect(integ["governance_sha256"] == sha_file(env.gov_path)
               and integ["governance_sha256"] != env.gov_sha,
               "integrity 记录观测漂移 governance bytes hash")
        verify_seal(env.output_root)


def test_live_integrity_revalidate_exception_still_seals():
    """收口复核回调异常 → 稳定失败码按 invalid 处理；cleanup 与封签不跳过、
    claim 已消费、--json 照常（engine 兜底：异常不产 verdict 悬空）。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        real_verify = fm.verify_inputs
        calls = {"n": 0}

        def verify_wrapper(*a, **kw):
            calls["n"] += 1
            if calls["n"] == 2:             # preflight(1) → 收口(2)
                raise RuntimeError("boom-revalidate")
            return real_verify(*a, **kw)

        fm.verify_inputs = verify_wrapper
        try:
            with fixture_scene(tmp):
                rc, out = run_main(env.live_argv(json_path=os.path.join(
                    tmp, "final.json")), input_fn=confirm_input(
                        "LIVE " + env.campaign(), "OPERATOR-READY"),
                    git_probe=env.probe, repo_root=env.repo)
        finally:
            fm.verify_inputs = real_verify
        expect(calls["n"] == 2, "verify_inputs 恰两次（预检+收口）: %s" % calls)
        expect(rc == cli.EXIT_FAIL, "异常 → exit 1: %s %s" % (rc, out))
        root = env.output_root
        doc = read_result(root)
        rec = doc["result"]
        assert_invalid_by_freeze_integrity(rec)
        expect(rec["integrity"] == {"schema_version": 1,
                                    "manifest_sha256": None,
                                    "governance_sha256": None,
                                    "precheck": None, "postcheck": "failed",
                                    "violations": ["revalidate-exception: "
                                                   "RuntimeError"]},
               "回调异常转为稳定失败码（不产伪造 hash/passed）: %s"
               % rec["integrity"])
        for op in ("ForceStop", "Uninstall", "RemoveStaging"):
            expect(op in rec["hdc_audit_ops"], "异常仍清理 %s" % op)
        verify_seal(root)
        expect(doc["terminal"] in ("complete", "incomplete"), "异常仍显式封签")
        expect(cli.read_claim(env.claim_dir())["status"]
               == "start-entry-attempted", "claim 已消费")
        expect(os.path.exists(os.path.join(tmp, "final.json")),
               "--json 照常写出")


def test_live_integrity_dryrun_and_smoke_still_empty():
    """正式 dryrun（freeze-bound）与无 manifest smoke 的 integrity 恒 {}：
    收口复核是 live 专属挂点，dryrun 不注入回调、门 11 原字面不回归。"""
    with sandbox() as tmp:
        env = LiveEnv(tmp)
        root = os.path.join(tmp, "dry")
        rc, _ = run_main(env.dryrun_argv(output_root=root),
                         git_probe=env.probe, repo_root=env.repo)
        expect(rc == cli.EXIT_OK, "正式 dryrun exit 0")
        rec = read_result(root)["result"]
        expect(rec["integrity"] == {} and rec["is_evidence"] is False,
               "freeze-bound dryrun integrity={} 原字面")
        smoke_root = os.path.join(tmp, "smoke")
        rc, out = run_main(["--dryrun", "--scenario", "happy",
                            "--output-root", smoke_root])
        expect(rc == cli.EXIT_OK, "smoke exit 0: %s" % out)
        rec = read_result(smoke_root)["result"]
        expect(rec["integrity"] == {} and rec["is_evidence"] is False,
               "smoke integrity={} 且不具 freeze 复核断言")


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
    print("n1bdisc cli selftests: tests=%d passed=%d failed=%d assertions=%d (%.0f ms)"
          % (len(tests), passed, failed, _ASSERTS, elapsed * 1000))
    return 0 if failed == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
