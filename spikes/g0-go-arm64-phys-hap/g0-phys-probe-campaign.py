#!/usr/bin/env python3
"""G0-PHYS-PROBE campaign runner (g0-phys-probe-campaign.py 1.0.0).

Python campaign runner for the G0 spike: measures whether the frozen stock
(zero-patch) Go 1.25.12 arm64 c-shared probe library (``libgoprobe.so`` inside
``cn.alfadb.netbird.g0probe``) is accepted by the physical HarmonyOS device
loader, plus a minimal runtime smoke. Mirrors the E3 governance patterns
(spikes/e3-vpn-extension-physical-preflight-hap/e3-phys-preflight-campaign.py):

  * exact-whitelist HDC invocation (15 operations, audit argv frozen verbatim,
    placeholders ``<PHYS_1_TARGET>`` / ``<HAP_G0>`` never leak the real target),
  * strict freeze manifest schema (exact key sets, no missing / extra keys),
  * single-use out-of-repository double-file confirmation record
    (JSON tmp + .sha256 tmp recomputed, atomic rename, companion last),
  * host HDC process count via absolute ``/usr/bin/ps -eo comm=,args=``
    comparing the FIRST column only,
  * evidence outputs: scenario-results.json + hash-manifest.json +
    campaign-seal.json + line-chained transcript.redacted.jsonl, raw artifacts
    under RawRoot.

G0 has NO scenarios/operator/layout logic (single S1 flow). The
marker -> verdict mapping is pre-registered in
docs/g0-go-arm64-physical-probe.md and implemented verbatim:

  verdict=PASS & ok=true & stage=complete & hello=42 & runtimeBytes=1048576 -> pass
  verdict=FAIL & stage=dlopen & loaderError non-empty                       -> blocked
    (a VALID measured result: the loader rejected the library; loaderErrno /
     loaderError are recorded verbatim)
  anything else / 0 markers / >1 markers                                    -> blocked
    (reason marker-missing | marker-ambiguous | drift)
  integrity violations (non-whitelist command attempt, freeze mismatch,
  dirty worktree)                                                           -> invalid
    (highest priority)

Python 3 standard library only; no network; no real hdc is ever touched by
--SelfTest, and --DryRun never executes or hashes the frozen hdc path (it runs
tests/fixtures/fake-hdc.py from a private temp sandbox under the name
``fake-hdc``).

Exit codes
    --version / --SelfTest                       0 (selftest 1 on failure)
    --DryRun / --Live  completed flow (sealed)   0, final line VERDICT=<verdict>
    any pre-flight / validation / runner error   1 (no evidence, no seal)
    --TargetBindingConfirm pass                  0
    --TargetBindingConfirm pre-record gate       1 (no record written)
    --TargetBindingConfirm blocked               2 (blocked record written)
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid
from datetime import datetime, timedelta, timezone

# =====================================================================
# Section 0: Constants and global state
# =====================================================================

RUNNER_VERSION = '1.0.0'
RUNNER_PATH = os.path.abspath(__file__)
RUNNER_DIR = os.path.dirname(RUNNER_PATH)

BUNDLE = 'cn.alfadb.netbird.g0probe'
ABILITY = 'EntryAbility'
MODULE = 'entry'
STAGING = '/data/local/tmp/netbird-g0'
HILOG_TAG = 'G0GoProbe'
HAP_PLACEHOLDER = '<HAP_G0>'
TARGET_PLACEHOLDER = '<PHYS_1_TARGET>'
SCENARIO_WINDOW_SECONDS = 60

# The current authorization fixes one AUTH, one candidate triple, and
# attempt=initial (ADJ discipline mirrored from E3 C6); retries need new
# governance and new IDs.
AUTH_ID = 'AUTH-G0PHYS1API26-20260830-0001'
CAMPAIGN_ID = 'G0-PHYS-PROBE-20260830-0001'
EVIDENCE_ID = 'EV-G0PHYS1API26-20260830-0001'

# All runner record timestamps use the fixed +08:00 offset (deterministic,
# matches the confirmation record created_at rule).
FIXED_TZ = timezone(timedelta(hours=8))

HDC_TIMEOUT_SECONDS = 20
RAW_HILOG_GRACE_SECONDS = 10

# DryRun scripted fake hdc behaviours (environment G0_DRYRUN_SCRIPT).
# 'install-fails' drives the InstallHap success-marker-missing path so the
# CampaignBlocked -> sealed-blocked ending is exercised (review MAJOR-2).
DRY_RUN_SCRIPTS = ('pass', 'dlopen-rejected', 'install-fails')
DRY_RUN_TARGET_SENTINEL = 'DRYRUN-LOCAL-TARGET'
DRY_RUN_NON_EVIDENCE_REASON = 'host-only dry-run against the sandboxed fake hdc; no physical-device evidence'

# Pre-registered marker -> verdict mapping values.
MARKER_TOKEN = 'G0_RESULT'
PASS_FIELDS = {'verdict': 'PASS', 'ok': 'true', 'stage': 'complete', 'hello': '42', 'runtimeBytes': '1048576'}

# Mutable script-scope state (mirrors PS $script: variables; set by main()).
freeze_manifest = None
repo_root = None
actual_target = None
execution_mode = None
dry_run = False
is_evidence = False
transcript_index = 0
transcript_previous_hash = '0' * 64
projection_transcript = None
hdc_logical_call_count = 0
hdc_process_start_count = 0

# =====================================================================
# Section 1: Base utilities (design unit U1)
# =====================================================================


def sha256_text(text):
    """Lowercase hex SHA-256 of UTF-8 bytes (E3 sha256_text)."""
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def sha256_file(path):
    """Lowercase hex SHA-256 of file bytes (E3 sha256_file)."""
    with open(path, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def normalize_path(path):
    """Absolute path with trailing separators trimmed (E3 normalize_path)."""
    return os.path.abspath(path).rstrip('/\\')


def is_under_path(candidate, parent):
    """True when candidate == parent or candidate is inside parent
    (E3 is_under_path; case-sensitive POSIX semantics)."""
    candidate_path = os.path.normcase(normalize_path(candidate) + os.sep)
    parent_path = os.path.normcase(normalize_path(parent) + os.sep)
    return candidate_path.startswith(parent_path)


def test_sha256_hex(value):
    """64 lowercase hex chars only."""
    return isinstance(value, str) and re.match(r'^[0-9a-f]{64}$', value) is not None


def test_sha1_hex(value):
    """40 lowercase hex chars only (repository commit SHA)."""
    return isinstance(value, str) and re.match(r'^[0-9a-f]{40}$', value) is not None


def test_json_integer(value):
    """JSON integer: int but never bool."""
    return isinstance(value, int) and not isinstance(value, bool)


def assert_exact_keys(obj, expected_keys, label):
    """Strict-schema gate: missing AND extra keys both fail."""
    if not isinstance(obj, dict):
        raise RuntimeError('%s must be a JSON object' % label)
    missing = sorted(key for key in expected_keys if key not in obj)
    extra = sorted(key for key in obj if key not in expected_keys)
    if missing:
        raise RuntimeError('%s missing key(s): %s' % (label, ', '.join(missing)))
    if extra:
        raise RuntimeError('%s unknown key(s): %s' % (label, ', '.join(extra)))


def require_non_empty_str(container, key, label):
    value = container.get(key)
    if not isinstance(value, str) or not value.strip():
        raise RuntimeError('%s.%s must be a non-empty string' % (label, key))
    return value


def require_sha256(container, key, label):
    value = container.get(key)
    if not test_sha256_hex(value if isinstance(value, str) else None):
        raise RuntimeError('%s.%s must be a 64-hex sha256 string' % (label, key))
    return value


def require_abs_path(container, key, label):
    value = container.get(key)
    if not isinstance(value, str) or not os.path.isabs(value):
        raise RuntimeError('%s.%s must be an absolute path string' % (label, key))
    return value


def require_constant(container, key, expected, label):
    value = container.get(key)
    if value != expected:
        raise RuntimeError('%s.%s must be exactly %r (got %r)' % (label, key, expected, value))
    return value


def assert_file_hash(label, path, expected):
    """E3 assert_file_hash: hash a real file against an expected sha256."""
    if not test_sha256_hex(expected if isinstance(expected, str) else None):
        raise RuntimeError('%s sha256 must be a final 64-hex SHA-256' % label)
    if not os.path.isfile(path):
        raise RuntimeError('%s file missing: hash check impossible' % label)
    actual = sha256_file(path)
    if actual != str(expected).lower():
        raise RuntimeError('%s sha256 mismatch: file bytes do not match the frozen value' % label)


def now_iso():
    """ISO-8601 timestamp with the fixed +08:00 offset."""
    return datetime.now(FIXED_TZ).isoformat()


def canonical_json(obj):
    """Deterministic compact serialization used for transcript chain hashing
    and manifest/record bytes; sort_keys keeps verification byte-exact."""
    return json.dumps(obj, sort_keys=True, separators=(',', ':'), ensure_ascii=False)


def write_text_utf8_no_bom(path, text):
    """UTF-8 without BOM, newline='' prevents translation (E3)."""
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def read_text_utf8_sig(path):
    """Read with BOM detection (E3 read_text_utf8_sig)."""
    with open(path, 'r', encoding='utf-8-sig') as f:
        return f.read()


def write_json_file(path, obj):
    """Indented UTF-8 JSON + trailing newline (E3 write_json_file)."""
    write_text_utf8_no_bom(path, json.dumps(obj, indent=2, ensure_ascii=False) + '\n')


def resolve_repository_root():
    """git -C <runner dir> rev-parse --show-toplevel; best-effort: returns
    None when git is unavailable so host-only DryRun keeps working. --Live
    hard-requires the root in its own preflight."""
    try:
        proc = subprocess.run(
            ['git', '-C', RUNNER_DIR, 'rev-parse', '--show-toplevel'],
            capture_output=True, text=True, encoding='utf-8', errors='replace', timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    root_text = proc.stdout.strip()
    if proc.returncode != 0 or not root_text:
        return None
    return normalize_path(root_text)


def get_git_status_porcelain(repo_root_path):
    """Exactly `git status --porcelain` (empty output == clean worktree)."""
    try:
        proc = subprocess.run(['git', '-C', repo_root_path, 'status', '--porcelain'],
                              capture_output=True, text=True, encoding='utf-8',
                              errors='replace', timeout=30)
    except (OSError, subprocess.SubprocessError) as e:
        raise RuntimeError('unable to read repository state: %s' % e)
    if proc.returncode != 0:
        raise RuntimeError('unable to read repository state')
    return proc.stdout


# =====================================================================
# Section 2: Sensitive information protection (design unit U1)
# =====================================================================


def protect_sensitive_text(text):
    """The real target token never enters any in-repository or evidence
    output: every free-text projection replaces it. G0 handles no
    UDID/serial/endpoint values (governance doc)."""
    if text is None:
        return ''
    safe = str(text)
    if actual_target and actual_target.strip():
        safe = re.sub(re.escape(actual_target), '<REDACTED_TARGET>', safe, flags=re.IGNORECASE)
    return safe


def protect_sensitive_data(value):
    """Recursive dict/list walk applying protect_sensitive_text to strings."""
    if value is None:
        return None
    if isinstance(value, str):
        return protect_sensitive_text(value)
    if isinstance(value, dict):
        return {str(key): protect_sensitive_data(val) for key, val in value.items()}
    if isinstance(value, (list, tuple)):
        return [protect_sensitive_data(item) for item in value]
    return value


# =====================================================================
# Section 3: Freeze validation (design unit U2) - strict schema
# =====================================================================

FREEZE_TOP_LEVEL_KEYS = (
    'schema_version', 'authorization_id', 'campaign_id', 'evidence_id', 'attempt', 'plan_status',
    'code_sha', 'runner_py_sha256', 'runner_ps1_sha256', 'selftest_py_sha256',
    'selftest_ps1_sha256', 'hdc', 'bundle', 'ability', 'module', 'staging',
    'hilog_tag', 'scenario_window_seconds', 'target_tuple', 'artifacts',
    'elf_profile', 'evidence_roots', 'raw_roots', 'confirmation', 'review',
    'operator', 'orchestrator',
)

EXPECTED_TARGET_TUPLE = {
    'distribution': 'HarmonyOS',
    'device_model': 'PLA-AL10',
    'full_system_build': 'PLA-AL10 7.0.0.102(SP8C00E102R7P3)',
    'api': '26',
    'kernel_architecture': 'aarch64',
    'app_abi': 'arm64-v8a',
}

ARTIFACT_KEYS = ('hap_path', 'hap_sha256', 'profile_sha256', 'certificate_sha256',
                 'libgoprobe_sha256', 'libgoloader_sha256')

CONFIRMATION_KEYS = ('status', 'record_path', 'record_sha256', 'authorization_id')
REVIEW_KEYS = ('status', 'record_path', 'record_sha256')
GOVERNANCE_STATUSES = ('pass', 'pending')


def _require_sibling_hash(container, key, path, label):
    """Required 64-hex hash that is recomputed against the sibling file in
    this checkout. A freeze may no longer leave the PowerShell parity
    artifacts unbound (review round-2 NEW-MINOR-1): both runners enforce all
    four implementation hashes before any mode runs."""
    declared = require_sha256(container, key, label)
    if not os.path.isfile(path):
        raise RuntimeError('%s.%s: parity sibling file missing for recompute' % (label, key))
    actual = sha256_file(path)
    if actual != declared:
        raise RuntimeError('%s.%s does not match the sibling file in this checkout (recomputed %s)' % (label, key, actual))
    return declared


def assert_freeze_schema(freeze, mode, repo_root_path):
    """Strict freeze schema: exact key sets everywhere; wrong type, missing
    key, or extra key fails. --Live only accepts plan_status=ready; --DryRun
    and --TargetBindingConfirm accept blocked or ready."""
    assert_exact_keys(freeze, FREEZE_TOP_LEVEL_KEYS, 'freeze')
    if freeze.get('schema_version') != 1:
        raise RuntimeError('freeze.schema_version must be the JSON integer 1')
    require_constant(freeze, 'authorization_id', AUTH_ID, 'freeze')
    require_constant(freeze, 'campaign_id', CAMPAIGN_ID, 'freeze')
    require_constant(freeze, 'evidence_id', EVIDENCE_ID, 'freeze')
    require_constant(freeze, 'attempt', 'initial', 'freeze')
    plan_status = freeze.get('plan_status')
    if plan_status not in ('blocked', 'ready'):
        raise RuntimeError('freeze.plan_status must be blocked or ready')
    if mode == 'live' and plan_status != 'ready':
        raise RuntimeError('Live requires plan_status ready')
    code_sha = freeze.get('code_sha')
    if not test_sha1_hex(code_sha if isinstance(code_sha, str) else None):
        raise RuntimeError('freeze.code_sha must be a 40-hex repository commit sha')
    runner_py_sha = require_sha256(freeze, 'runner_py_sha256', 'freeze')
    # runner_py_sha256 is recomputed against this runner file at load time.
    actual_runner_sha = sha256_file(RUNNER_PATH)
    if actual_runner_sha != runner_py_sha:
        raise RuntimeError('freeze.runner_py_sha256 does not match this runner file (recomputed %s)' % actual_runner_sha)
    # Parity artifacts are REQUIRED and recomputed against their sibling
    # files in this checkout (review round-2 NEW-MINOR-1): no empty-string
    # escape, on either runner.
    spike_dir = os.path.dirname(RUNNER_PATH)
    _require_sibling_hash(freeze, 'runner_ps1_sha256',
                          os.path.join(spike_dir, 'g0-phys-probe-campaign.ps1'), 'freeze')
    _require_sibling_hash(freeze, 'selftest_py_sha256',
                          os.path.join(spike_dir, 'tests', 'g0-runner-selftest.py'), 'freeze')
    _require_sibling_hash(freeze, 'selftest_ps1_sha256',
                          os.path.join(spike_dir, 'tests', 'g0-runner-selftest.ps1'), 'freeze')

    hdc = freeze.get('hdc')
    assert_exact_keys(hdc, ('path', 'sha256', 'version'), 'freeze.hdc')
    require_abs_path(hdc, 'path', 'freeze.hdc')
    require_sha256(hdc, 'sha256', 'freeze.hdc')
    require_non_empty_str(hdc, 'version', 'freeze.hdc')

    require_constant(freeze, 'bundle', BUNDLE, 'freeze')
    require_constant(freeze, 'ability', ABILITY, 'freeze')
    require_constant(freeze, 'module', MODULE, 'freeze')
    require_constant(freeze, 'staging', STAGING, 'freeze')
    require_constant(freeze, 'hilog_tag', HILOG_TAG, 'freeze')
    if not test_json_integer(freeze.get('scenario_window_seconds')) or freeze['scenario_window_seconds'] != SCENARIO_WINDOW_SECONDS:
        raise RuntimeError('freeze.scenario_window_seconds must be the JSON integer 60')

    tuple_ = freeze.get('target_tuple')
    assert_exact_keys(tuple_, tuple(EXPECTED_TARGET_TUPLE.keys()), 'freeze.target_tuple')
    for key, expected in EXPECTED_TARGET_TUPLE.items():
        require_constant(tuple_, key, expected, 'freeze.target_tuple')

    artifacts = freeze.get('artifacts')
    assert_exact_keys(artifacts, ARTIFACT_KEYS, 'freeze.artifacts')
    hap_path = require_abs_path(artifacts, 'hap_path', 'freeze.artifacts')
    for key in ARTIFACT_KEYS[1:]:
        require_sha256(artifacts, key, 'freeze.artifacts')
    if repo_root_path is not None and (is_under_path(hap_path, repo_root_path) or normalize_path(hap_path) == repo_root_path):
        raise RuntimeError('freeze.artifacts.hap_path must be outside the git repository')

    elf = freeze.get('elf_profile')
    assert_exact_keys(elf, ('pt_tls', 'tprel64_count', 'static_tls_flag', 'needed'), 'freeze.elf_profile')
    if elf.get('pt_tls') is not True:
        raise RuntimeError('freeze.elf_profile.pt_tls must be the JSON boolean true')
    if not test_json_integer(elf.get('tprel64_count')) or elf['tprel64_count'] != 1:
        raise RuntimeError('freeze.elf_profile.tprel64_count must be the JSON integer 1')
    if elf.get('static_tls_flag') is not False:
        raise RuntimeError('freeze.elf_profile.static_tls_flag must be the JSON boolean false')
    needed = elf.get('needed')
    if not isinstance(needed, list) or len(needed) != 1 or needed[0] != 'libc.so':
        raise RuntimeError("freeze.elf_profile.needed must be exactly ['libc.so']")

    for group_label, group_key in (('freeze.evidence_roots', 'evidence_roots'), ('freeze.raw_roots', 'raw_roots')):
        group = freeze.get(group_key)
        assert_exact_keys(group, ('dry_run', 'live'), group_label)
        require_abs_path(group, 'dry_run', group_label)
        require_abs_path(group, 'live', group_label)

    # Governance role literals are pinned by the freeze schema (spec values).
    require_constant(freeze, 'operator', 'authorized user', 'freeze')
    require_constant(freeze, 'orchestrator', 'main agent', 'freeze')

    assert_governance_entries(freeze)
    return plan_status


def _verify_governance_record(label, entry):
    """A declared pass is hash-anchored: the record file must exist, its bytes
    must hash to record_sha256, and it must be parseable JSON."""
    record_path = normalize_path(str(entry.get('record_path', '')))
    if not os.path.isfile(record_path):
        raise RuntimeError('%s record file missing (status=pass requires the bound record)' % label)
    actual = sha256_file(record_path)
    if actual != str(entry.get('record_sha256', '')).lower():
        raise RuntimeError('%s record_sha256 does not match the record file bytes' % label)
    try:
        json.loads(read_text_utf8_sig(record_path))
    except Exception:
        raise RuntimeError('%s record is not parseable JSON' % label)


def assert_governance_entries(freeze):
    """confirmation/review schema + binding rules:
      * plan_status=ready requires confirmation.status=pass AND review.status=pass
        (double binding); blocked allows pending;
      * review.status=pass with a pending/absent machine confirmation is rejected
        (a declared-pass review can never ride on a pending machine side);
      * any declared pass is hash-verified against the record file."""
    confirmation = freeze.get('confirmation')
    assert_exact_keys(confirmation, CONFIRMATION_KEYS, 'freeze.confirmation')
    require_abs_path(confirmation, 'record_path', 'freeze.confirmation')
    require_sha256(confirmation, 'record_sha256', 'freeze.confirmation')
    require_non_empty_str(confirmation, 'authorization_id', 'freeze.confirmation')
    review = freeze.get('review')
    assert_exact_keys(review, REVIEW_KEYS, 'freeze.review')
    require_abs_path(review, 'record_path', 'freeze.review')
    require_sha256(review, 'record_sha256', 'freeze.review')
    for label, entry in (('freeze.confirmation', confirmation), ('freeze.review', review)):
        if entry.get('status') not in GOVERNANCE_STATUSES:
            raise RuntimeError('%s.status must be pass or pending' % label)
    confirmation_pass = confirmation['status'] == 'pass'
    review_pass = review['status'] == 'pass'
    if review_pass and not confirmation_pass:
        raise RuntimeError('review.status=pass requires confirmation.status=pass; '
                           'a pending machine confirmation cannot anchor a declared-pass review')
    if freeze.get('plan_status') == 'ready' and not (confirmation_pass and review_pass):
        raise RuntimeError('plan_status ready requires confirmation.status=pass and review.status=pass (double binding)')
    if confirmation_pass:
        _verify_governance_record('confirmation', confirmation)
        if confirmation.get('authorization_id') != freeze.get('authorization_id'):
            raise RuntimeError('confirmation.authorization_id does not match freeze.authorization_id')
    if review_pass:
        _verify_governance_record('review', review)


def load_freeze(freeze_path, mode, repo_root_path):
    """Parse + validate the freeze manifest for the given mode. Returns the
    freeze dict or raises (pre-campaign validation error -> exit 1)."""
    if not os.path.isfile(freeze_path):
        raise RuntimeError('Freeze file missing: %s' % freeze_path)
    try:
        freeze = json.loads(read_text_utf8_sig(freeze_path))
    except ValueError as e:
        raise RuntimeError('Freeze file is not valid JSON: %s' % e)
    assert_freeze_schema(freeze, mode, repo_root_path)
    return freeze


# =====================================================================
# Section 4: HDC whitelist and process execution (design unit U4)
# =====================================================================

# The 15 frozen operations; audit-form argv is built verbatim in
# get_hdc_invocation. Parameter tuples name the required parameters.
HDC_WHITELIST = {
    'Version': (), 'TupleModel': (), 'TupleBuild': (),
    'MkdirStaging': (), 'SendHap': (), 'InstallHap': (),
    'HilogStream': (), 'FaultProbe': (), 'RemoveStaging': (), 'StagingProbe': (),
    'BundleDump': ('Bundle',), 'PidOf': ('Bundle',), 'StartEntry': ('Bundle',),
    'Uninstall': ('Bundle',), 'ForceStop': ('Bundle', 'Reason'),
}

# PS-equivalent case-insensitive operation/parameter lookup (MINOR-2 style).
HDC_OPERATION_ALIASES = {name.casefold(): name for name in HDC_WHITELIST}
HDC_PARAMETER_ALIASES = {name.casefold(): name for name in ('Bundle', 'Reason')}

FORCE_STOP_REASONS = ('exception-cleanup', 'final-cleanup')


def normalize_hdc_operation(operation):
    """casefold -> canonical PascalCase; unknown names pass through so the
    whitelist rejection keeps the caller's spelling."""
    return HDC_OPERATION_ALIASES.get(str(operation).casefold(), operation)


def normalize_hdc_parameters(parameters):
    """casefold -> canonical parameter names; unknown names pass through."""
    if parameters is None:
        parameters = {}
    return {HDC_PARAMETER_ALIASES.get(str(k).casefold(), k): v for k, v in parameters.items()}


def assert_exact_command_parameters(operation, parameters):
    """Unknown operation / extra parameter / missing parameter / empty value
    -> rejected. Mirrors E3 Assert-ExactCommandParameters."""
    operation = normalize_hdc_operation(operation)
    parameters = normalize_hdc_parameters(parameters)
    if operation not in HDC_WHITELIST:
        raise RuntimeError("command rejected: operation '%s' is not allowlisted" % operation)
    required = HDC_WHITELIST[operation]
    for name in required:
        if name not in parameters or not str(parameters[name]).strip():
            raise RuntimeError("command rejected: operation '%s' requires parameter '%s'" % (operation, name))
    for name in parameters:
        if name not in required:
            raise RuntimeError("command rejected: operation '%s' does not accept parameter '%s'" % (operation, name))


def get_hdc_invocation(operation, parameters=None):
    """Whitelisted audit argv construction. The audit (projection/transcript)
    form ALWAYS keeps the placeholders <PHYS_1_TARGET>/<HAP_G0>; the real
    target never enters the projection. Live substitution happens only in
    get_live_hdc_arguments. G0 pidof targets the bundle UI process (no
    :vpn suffix); the Bundle parameter must equal the frozen bundle."""
    if parameters is None:
        parameters = {}
    operation = normalize_hdc_operation(operation)
    parameters = normalize_hdc_parameters(parameters)
    assert_exact_command_parameters(operation, parameters)
    bundle = str(parameters['Bundle']) if 'Bundle' in parameters else ''
    if bundle and bundle != BUNDLE:
        raise RuntimeError('command rejected: bundle outside the frozen G0 bundle')
    if operation == 'ForceStop':
        if str(parameters['Reason']) not in FORCE_STOP_REASONS:
            raise RuntimeError('command rejected: force-stop is cleanup-only (Reason must be exception-cleanup or final-cleanup)')
    target = ['-t', TARGET_PLACEHOLDER]
    if operation == 'Version':
        return ['version']
    if operation == 'TupleModel':
        return target + ['shell', 'param', 'get', 'const.product.model']
    if operation == 'TupleBuild':
        return target + ['shell', 'param', 'get', 'const.product.software.version']
    if operation == 'BundleDump':
        return target + ['shell', 'bm', 'dump', '-n', bundle]
    if operation == 'PidOf':
        return target + ['shell', 'pidof', bundle]
    if operation == 'MkdirStaging':
        return target + ['shell', 'mkdir', '-p', STAGING + '/hap']
    if operation == 'SendHap':
        return target + ['file', 'send', HAP_PLACEHOLDER, STAGING + '/hap/g0.hap']
    if operation == 'InstallHap':
        return target + ['shell', 'bm', 'install', '-p', STAGING + '/hap']
    if operation == 'StartEntry':
        return target + ['shell', 'aa', 'start', '-a', ABILITY, '-b', bundle, '-m', MODULE]
    if operation == 'HilogStream':
        return target + ['shell', 'hilog', '-T', HILOG_TAG, '-v', 'year', '-v', 'zone']
    if operation == 'FaultProbe':
        return target + ['shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1',
                         '-type', 'f', '-name', '*%s*' % BUNDLE, '-print']
    if operation == 'ForceStop':
        return target + ['shell', 'aa', 'force-stop', bundle]
    if operation == 'Uninstall':
        return target + ['shell', 'bm', 'uninstall', '-n', bundle]
    if operation == 'RemoveStaging':
        return target + ['shell', 'rm', '-rf', STAGING]
    if operation == 'StagingProbe':
        return target + ['shell', 'ls', '-ld', STAGING]
    raise RuntimeError("command rejected: operation '%s' is not allowlisted" % operation)


def get_live_hdc_arguments(audit_arguments, target_token, hap_path):
    """Placeholder substitution for the execution path only. The audit /
    transcript form always keeps the placeholders."""
    live = []
    for arg in audit_arguments:
        if arg == TARGET_PLACEHOLDER:
            if not target_token:
                raise RuntimeError('live target substitution requires a real target token')
            live.append(target_token)
        elif arg == HAP_PLACEHOLDER:
            if not hap_path:
                raise RuntimeError('live hap substitution requires the frozen HAP path')
            live.append(hap_path)
        else:
            live.append(arg)
    return live


def test_physical_target_token(target):
    """PS L1265-1269 Test-PhysicalTargetToken (E3-verbatim logic): non-empty,
    no leading/trailing whitespace, no whitespace/comma/semicolon, not PHYS-1
    or a placeholder, and never a flag-shaped token (leading '-')."""
    if target is None or not target.strip():
        return False
    return (target == target.strip() and not re.search(r'[\s,;]', target)
            and not target.startswith('-')
            and not re.match(r'^(?:PHYS-1|<.+>)$', target, re.IGNORECASE))


def assert_target_environment():
    """PS L1271-1275 Assert-TargetEnvironment: PHYS_1_TARGET is process-scope."""
    target = os.environ.get('PHYS_1_TARGET', '')
    if not test_physical_target_token(target):
        raise RuntimeError('PHYS_1_TARGET must contain exactly one real target token')
    global actual_target
    actual_target = target


class HdcResult:
    """Invoke-HdcOperation result object (ExitCode/Stdout/Stderr/Simulated)."""

    __slots__ = ('exit_code', 'stdout', 'stderr', 'simulated')

    def __init__(self, exit_code, stdout, stderr, simulated):
        self.exit_code = exit_code
        self.stdout = stdout
        self.stderr = stderr
        self.simulated = simulated

    def combined_text(self):
        return (str(self.stdout) + '\n' + str(self.stderr)).strip()


def count_hdc_from_ps_output(text):
    """Count host hdc processes from `ps -eo comm=,args=` output by comparing
    ONLY the first column; argv text can never make an unrelated process
    match (fake-hdc / python3 never match)."""
    count = 0
    for line in str(text or '').splitlines():
        columns = line.split(None, 1)
        if columns and columns[0] == 'hdc':
            count += 1
    return count


def count_hdc_processes():
    """Fixed absolute host probe: /usr/bin/ps -eo comm=,args=. Returns -1 if
    the probe is unavailable or fails (E3 count_hdc_processes)."""
    try:
        proc = subprocess.run(['/usr/bin/ps', '-eo', 'comm=,args='], capture_output=True,
                              text=True, encoding='utf-8', timeout=10)
    except (OSError, subprocess.SubprocessError):
        return -1
    if proc.returncode != 0:
        return -1
    return count_hdc_from_ps_output(proc.stdout)


# =====================================================================
# Section 5: Marker parsing and pre-registered verdict mapping
# =====================================================================


def parse_marker_fields(line):
    """Parse the pre-registered pipe-delimited marker body
    (`G0_RESULT|k=v|k=v|...`, docs/g0-go-arm64-physical-probe.md). Values may
    contain spaces (the C/ArkTS layers sanitize '|' to space), so each '|'
    segment is one field; a segment without '=' continues the previous value."""
    idx = line.find(MARKER_TOKEN)
    if idx < 0:
        return {}
    body = line[idx + len(MARKER_TOKEN):]
    fields = {}
    current = None
    for segment in body.split('|'):
        segment = segment.strip()
        if not segment:
            continue
        match = re.match(r'^([A-Za-z_][A-Za-z0-9_]*)=(.*)$', segment, re.DOTALL)
        if match:
            current = match.group(1)
            fields[current] = match.group(2)
        elif current is not None:
            fields[current] = (fields[current] + ' ' + segment).strip()
    return fields


def map_markers_to_verdict(marker_lines):
    """Pre-registered mapping (fail-closed). Returns
    {'verdict', 'fail_reason', 'fields'}."""
    if len(marker_lines) == 0:
        return {'verdict': 'blocked', 'fail_reason': 'marker-missing', 'fields': {}}
    if len(marker_lines) > 1:
        return {'verdict': 'blocked', 'fail_reason': 'marker-ambiguous', 'fields': {}}
    fields = parse_marker_fields(marker_lines[0])
    if all(fields.get(key) == value for key, value in PASS_FIELDS.items()):
        return {'verdict': 'pass', 'fail_reason': None, 'fields': fields}
    if (fields.get('verdict') == 'FAIL' and fields.get('stage') == 'dlopen'
            and str(fields.get('loaderError', '')).strip()):
        # A valid measured result: the loader rejected the library.
        return {'verdict': 'blocked', 'fail_reason': 'dlopen-blocked', 'fields': fields}
    return {'verdict': 'blocked', 'fail_reason': 'drift', 'fields': fields}


# =====================================================================
# Section 6: Transcript (line-chained JSONL) and integrity verification
# =====================================================================


def append_transcript_record(event, details):
    """One line per runner step: {seq, ts, event, details, prev_line_sha256,
    line_sha256} with line_sha256 = sha256(prev_line_sha256 + canonical JSON
    of the line WITHOUT line_sha256). The last line is the chain head.
    No-op until the projection transcript is initialized."""
    global transcript_index, transcript_previous_hash
    if projection_transcript is None:
        return
    entry = {
        'seq': transcript_index + 1,
        'ts': now_iso(),
        'event': str(event),
        'details': protect_sensitive_data(details if details is not None else {}),
        'prev_line_sha256': transcript_previous_hash,
    }
    canonical = canonical_json(entry)
    line_sha = sha256_text(transcript_previous_hash + canonical)
    record = dict(entry)
    record['line_sha256'] = line_sha
    with open(projection_transcript, 'a', encoding='utf-8', newline='') as f:
        f.write(canonical_json(record) + '\n')
    transcript_index += 1
    transcript_previous_hash = line_sha


def test_transcript_integrity(transcript_path):
    """Recompute the per-line chain (canonical bytes, previous hash, seq
    order, line hash). Returns the unique violation list in order."""
    violations = []
    if not os.path.isfile(transcript_path):
        return ['transcript-missing']
    previous_hash = '0' * 64
    expected_seq = 1
    with open(transcript_path, 'r', encoding='utf-8-sig') as f:
        for raw in f:
            line = raw.rstrip('\n').rstrip('\r')
            if not line.strip():
                continue
            try:
                doc = json.loads(line)
                core = {'seq': doc['seq'], 'ts': doc['ts'], 'event': doc['event'],
                        'details': doc['details'], 'prev_line_sha256': doc['prev_line_sha256']}
                stored_prev = str(doc['prev_line_sha256'])
                stored_line_sha = str(doc['line_sha256'])
                seq = int(doc['seq'])
            except Exception:
                violations.append('transcript-json-invalid')
                continue
            if seq != expected_seq:
                violations.append('transcript-order-invalid')
            if stored_prev != previous_hash:
                violations.append('transcript-previous-hash-invalid')
            recomputed = sha256_text(stored_prev + canonical_json(core))
            if recomputed != stored_line_sha:
                violations.append('transcript-line-hash-invalid')
            previous_hash = stored_line_sha
            expected_seq += 1
    return list(dict.fromkeys(violations))


def get_transcript_chain_head(transcript_path):
    """Chain head = line_sha256 of the final line ('0'*64 when empty)."""
    head = '0' * 64
    if not os.path.isfile(transcript_path):
        return head
    with open(transcript_path, 'r', encoding='utf-8-sig') as f:
        for raw in f:
            line = raw.rstrip('\n').rstrip('\r')
            if not line.strip():
                continue
            try:
                head = str(json.loads(line).get('line_sha256', head))
            except Exception:
                continue
    return head


# =====================================================================
# Section 7: Out-of-repo double-file confirmation record (design unit U3)
# =====================================================================

CONFIRMATION_RECORD_FIELDS = (
    'schema_version', 'record_kind', 'is_evidence', 'authorization_id',
    'campaign_id', 'evidence_id', 'attempt', 'plan_status', 'target_redacted',
    'code_sha', 'runner_py_sha256', 'hdc_version_expected', 'hdc_sha256',
    'expected_model', 'expected_build', 'observed_version', 'observed_model',
    'observed_build', 'command_attempted', 'command_completed', 'started_at',
    'ended_at', 'created_at', 'execution_mode', 'verdict', 'reason',
)


class PreRecordGateError(RuntimeError):
    """Pre-record gate failure in TargetBindingConfirm mode: nothing was
    written and the process must exit 1."""


def new_target_binding_confirmation_record(freeze, started_at, ended_at, verdict, reason,
                                           observed_version, observed_model, observed_build,
                                           command_attempted, command_completed):
    """The single-use confirmation record: kind g0-target-binding-confirmation,
    is_evidence=false, target_redacted=true, projected (public) model/build
    values, created_at in the fixed +08:00 zone."""
    return {
        'schema_version': 1,
        'record_kind': 'g0-target-binding-confirmation',
        'is_evidence': False,
        'authorization_id': str(freeze['authorization_id']),
        'campaign_id': str(freeze['campaign_id']),
        'evidence_id': str(freeze['evidence_id']),
        'attempt': str(freeze['attempt']),
        'plan_status': str(freeze['plan_status']),
        'target_redacted': True,
        'code_sha': str(freeze['code_sha']),
        'runner_py_sha256': str(freeze['runner_py_sha256']),
        'hdc_version_expected': str(freeze['hdc']['version']),
        'hdc_sha256': str(freeze['hdc']['sha256']),
        'expected_model': str(freeze['target_tuple']['device_model']),
        'expected_build': str(freeze['target_tuple']['full_system_build']),
        'observed_version': observed_version,
        'observed_model': observed_model,
        'observed_build': observed_build,
        'command_attempted': command_attempted,
        'command_completed': command_completed,
        'started_at': started_at.isoformat(),
        'ended_at': ended_at.isoformat(),
        'created_at': now_iso(),
        'execution_mode': 'target-binding-confirm',
        'verdict': verdict,
        'reason': 'N/A' if not reason else str(reason),
    }


def write_confirmation_record_pair(record_path, record):
    """Double-file completion marker: JSON tmp + .sha256 tmp (hash recomputed
    over the tmp bytes), then atomic rename; the companion is renamed LAST as
    the completion marker. Single-use is enforced by the pre-record gates
    (record/companion must not exist); an orphan JSON from a failed companion
    rename is never overwritten and never consumable."""
    companion_path = record_path + '.sha256'
    for candidate in (record_path, companion_path):
        if os.path.exists(candidate):
            raise PreRecordGateError('confirmation output already exists and is immutable: %s' % candidate)
    tmp_json = record_path + '.tmp-' + uuid.uuid4().hex
    tmp_sha = companion_path + '.tmp-' + uuid.uuid4().hex
    json_moved = False
    try:
        json_text = json.dumps(record, indent=2, ensure_ascii=False) + '\n'
        fd = os.open(tmp_json, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
            f.write(json_text)
            f.flush()
            os.fsync(f.fileno())
        sha = sha256_file(tmp_json)
        if sha256_file(tmp_json) != sha:
            raise RuntimeError('confirmation record hash recompute mismatch')
        fd = os.open(tmp_sha, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
            f.write(sha + '\n')
            f.flush()
            os.fsync(f.fileno())
        # Atomic rename: JSON first, companion LAST (completion marker).
        os.replace(tmp_json, record_path)
        json_moved = True
        os.replace(tmp_sha, companion_path)
    finally:
        for tmp in (tmp_json, tmp_sha):
            if os.path.exists(tmp):
                try:
                    os.unlink(tmp)
                except OSError:
                    pass
    return sha


def invoke_target_binding_confirm(freeze, freeze_path, confirmation_record_arg, repo_root_path):
    """Host-governed machine fresh target binding: exactly the three frozen
    probes (Version/TupleModel/TupleBuild) against the REAL frozen hdc
    (path + sha256 from the freeze, sha recomputed before execution), then a
    single-use out-of-repo double-file record. Exit codes: pass=0, pre-record
    gate=1 (no record), probe/tuple/write failure=2 (blocked record)."""
    del freeze_path  # binding context only; the record binds hashes, not paths
    record_path = normalize_path(confirmation_record_arg)
    if repo_root_path is not None and (record_path == repo_root_path or is_under_path(record_path, repo_root_path)):
        raise PreRecordGateError('ConfirmationRecord must be outside the git repository')
    companion_path = record_path + '.sha256'
    if os.path.exists(record_path):
        raise PreRecordGateError('ConfirmationRecord already exists; target-binding confirmation is single-use')
    if os.path.exists(companion_path):
        raise PreRecordGateError('ConfirmationRecord .sha256 companion already exists; target-binding confirmation is single-use')
    started_at = datetime.now(FIXED_TZ)
    verdict = 'blocked'
    reason = None
    observed_version = ''
    observed_model = ''
    observed_build = ''
    command_attempted = 0
    command_completed = 0
    try:
        hdc_path = normalize_path(str(freeze['hdc']['path']))
        # Recompute the frozen hdc hash immediately before execution.
        assert_file_hash('frozen hdc executable', hdc_path, str(freeze['hdc']['sha256']))
        assert_target_environment()
        hap_live = normalize_path(str(freeze['artifacts']['hap_path']))
        for operation in ('Version', 'TupleModel', 'TupleBuild'):
            command_attempted += 1
            audit_argv = get_hdc_invocation(operation)
            live_argv = get_live_hdc_arguments(audit_argv, actual_target, hap_live)
            proc = subprocess.run([hdc_path] + live_argv, capture_output=True, text=True,
                                  encoding='utf-8', errors='replace', timeout=HDC_TIMEOUT_SECONDS)
            command_completed += 1
            result = HdcResult(proc.returncode, proc.stdout, proc.stderr, False)
            if operation == 'Version':
                observed_version = result.stdout.strip()
            elif operation == 'TupleModel':
                observed_model = result.stdout.strip()
            elif operation == 'TupleBuild':
                observed_build = result.stdout.strip()
        if command_attempted != 3 or command_completed != 3:
            raise RuntimeError('target-binding confirmation requires exactly three HDC probes')
        if str(freeze['hdc']['version']) not in observed_version:
            raise RuntimeError('frozen HDC version mismatch')
        if observed_model != str(freeze['target_tuple']['device_model']):
            raise RuntimeError('frozen device model mismatch')
        if observed_build != str(freeze['target_tuple']['full_system_build']):
            raise RuntimeError('frozen full system build mismatch')
        verdict = 'pass'
    except Exception as e:
        # Probe / environment / tuple failure: still write a best-effort
        # blocked record + companion (exit 2).
        reason = protect_sensitive_text(str(e))
        verdict = 'blocked'
    ended_at = datetime.now(FIXED_TZ)
    # Device-observed values are protected before entering the record; the
    # real target never appears in any field.
    record = new_target_binding_confirmation_record(
        freeze, started_at, ended_at, verdict, reason,
        protect_sensitive_text(observed_version), protect_sensitive_text(observed_model),
        protect_sensitive_text(observed_build), command_attempted, command_completed)
    record_sha = None
    try:
        write_confirmation_record_pair(record_path, record)
        # Return and disk stay the same source: recompute over the final file.
        record_sha = sha256_file(record_path)
    except PreRecordGateError:
        raise
    except Exception as e:
        # A companion failure may leave an orphan JSON: never deleted, never
        # overwritten, never consumable. Downgrade to blocked (exit 2).
        write_failure = protect_sensitive_text(str(e))
        verdict = 'blocked'
        record_sha = None
        reason = ('record-write-failed: %s' % write_failure) if not reason \
            else ('%s; record-write-failed: %s' % (reason, write_failure))
        record = new_target_binding_confirmation_record(
            freeze, started_at, ended_at, verdict, reason,
            protect_sensitive_text(observed_version), protect_sensitive_text(observed_model),
            protect_sensitive_text(observed_build), command_attempted, command_completed)
        try:
            write_confirmation_record_pair(record_path, record)
            record_sha = sha256_file(record_path)
        except Exception:
            record_sha = None
    suffix = ' RECORD_SHA256=%s' % record_sha if record_sha else ''
    print('RUNNER_RESULT=%s MODE=target-binding-confirm RECORD_KIND=g0-target-binding-confirmation '
          'IS_EVIDENCE=false COMMAND_ATTEMPTED=%d COMMAND_COMPLETED=%d RECORD=%s%s'
          % (verdict, command_attempted, command_completed, record_path, suffix))
    return 0 if verdict == 'pass' else 2


# =====================================================================
# Section 8: Campaign flow (design unit U6/U7) - single S1 scenario
# =====================================================================


class CampaignBlocked(Exception):
    """A measured blocked ending: the flow stops, winds down with
    final-cleanup, and completes with a sealed blocked record (exit 0).
    The reason string is carried on .reason (and args[0]) so the sealed
    record and transcript always carry it (review BLOCKER-1)."""

    def __init__(self, reason):
        super().__init__(reason)
        self.reason = str(reason)


def root_key_for_mode(mode):
    """Freeze root keys are `dry_run`/`live`; modes are `dry-run`/`live`."""
    return 'dry_run' if mode == 'dry-run' else 'live'


class CampaignContext:
    """Script-scope campaign state (E3-style $script: variables, contained)."""

    def __init__(self, freeze, mode, repo_root_path, freeze_path):
        self.freeze = freeze
        self.mode = mode
        self.is_evidence = (mode == 'live')
        self.repo_root = repo_root_path
        self.freeze_path = normalize_path(freeze_path)
        self.freeze_sha256 = sha256_file(self.freeze_path)
        self.evidence_path = None
        self.raw_path = None
        self.transcript_path = None
        self.executable = None
        self.fake_env = None
        self.fake_sandbox = None
        self.hap_live = normalize_path(str(freeze['artifacts']['hap_path']))
        self.command_seq = 0
        self.hdc_logical_calls = 0
        self.hdc_process_starts = 0
        self.hdc_operations = {}
        self.command_attempted = 0
        self.command_completed = 0
        self.integrity_violations = []
        self.installed = False
        self.entry_started = False
        self.staging_prepared = False
        self.cleanup_actions = []
        self.cleanup_status = 'not-run'
        self.absent_probes = {}
        self.marker_lines = []
        # Canonical empty shape: every ending (including CampaignBlocked
        # before hilog) produces the same evidence schema (review MAJOR-2).
        self.markers = {
            'count': 0,
            'raw_lines': [],
            'fields': {},
            'hilog_lines_total': 0,
            'hilog_tag_lines_total': 0,
            'window_seconds': 0,
            'window_elapsed_full': False,
        }
        self.marker_mapping = None
        self.loader_error = None
        self.loader_errno = None
        self.observed_tuple = {}
        self.fault_lines = None
        self.steps = []
        self.started_at = None
        self.ended_at = None

    # ---- infrastructure -------------------------------------------------

    def initialize_output_roots(self):
        """EvidenceRoot/RawRoot from the freeze (mode-specific pair): must be
        absent, independent sibling trees, and outside the repository
        (E3 Initialize-OutputRoots discipline)."""
        evidence = normalize_path(str(self.freeze['evidence_roots'][root_key_for_mode(self.mode)]))
        raw = normalize_path(str(self.freeze['raw_roots'][root_key_for_mode(self.mode)]))
        if os.path.exists(evidence):
            raise RuntimeError('EvidenceRoot already exists; existing evidence is immutable and selective rerun is forbidden')
        if os.path.exists(raw):
            raise RuntimeError('RawRoot already exists; existing raw collection is immutable and selective rerun is forbidden')
        if evidence == raw or is_under_path(raw, evidence) or is_under_path(evidence, raw):
            raise RuntimeError('EvidenceRoot and RawRoot must be independent sibling trees')
        if self.repo_root is not None and (is_under_path(evidence, self.repo_root) or is_under_path(raw, self.repo_root)):
            raise RuntimeError('EvidenceRoot and RawRoot must be outside the git repository')
        os.makedirs(evidence)
        os.makedirs(raw)
        self.evidence_path = evidence
        self.raw_path = raw
        self.transcript_path = os.path.join(evidence, 'transcript.redacted.jsonl')
        global projection_transcript, transcript_index, transcript_previous_hash
        projection_transcript = self.transcript_path
        transcript_index = 0
        transcript_previous_hash = '0' * 64
        with open(self.transcript_path, 'w', encoding='utf-8', newline=''):
            pass

    def prepare_fake_hdc(self):
        """--DryRun: install tests/fixtures/fake-hdc.py into a private temp
        sandbox under the name `fake-hdc` (never `hdc`; the host process-table
        probe compares comm== only). The frozen hdc path is NEVER executed or
        hashed in DryRun."""
        fixture = os.path.join(RUNNER_DIR, 'tests', 'fixtures', 'fake-hdc.py')
        if not os.path.isfile(fixture):
            raise RuntimeError('DryRun fake hdc fixture missing: %s' % fixture)
        sandbox = tempfile.mkdtemp(prefix='g0-fake-hdc-')
        fake_path = os.path.join(sandbox, 'fake-hdc')
        shutil.copyfile(fixture, fake_path)
        os.chmod(fake_path, 0o700)
        if normalize_path(fake_path) == normalize_path(str(self.freeze['hdc']['path'])):
            raise RuntimeError('dry-run fake hdc must never be the frozen hdc executable')
        state_path = os.path.join(sandbox, 'fake-hdc-state.json')
        write_text_utf8_no_bom(state_path, '{}\n')
        self.executable = fake_path
        self.fake_sandbox = sandbox
        self.fake_env = {
            'G0_FAKE_HDC_STATE': state_path,
            'G0_FAKE_HDC_VERSION': str(self.freeze['hdc']['version']),
        }

    def child_env(self):
        env = dict(os.environ)
        if self.fake_env:
            env.update(self.fake_env)
        return env

    # ---- transcript -----------------------------------------------------

    def transcript(self, event, details):
        append_transcript_record(event, details)

    # ---- hdc execution ---------------------------------------------------

    def _record_command(self, operation, audit_argv, result, duration_ms, stdout_ref, stderr_ref):
        self.transcript('hdc-command', {
            'operation': operation,
            'executable': '<HDC_PATH>',
            'arguments': list(audit_argv),
            'exit_code': result.exit_code,
            'duration_ms': int(duration_ms),
            'stdout_bytes': len(result.stdout.encode('utf-8')),
            'stderr_bytes': len(result.stderr.encode('utf-8')),
            'stdout_raw': stdout_ref,
            'stderr_raw': stderr_ref,
            'simulated': bool(result.simulated),
        })

    def _write_raw_text(self, name, text):
        path = os.path.join(self.raw_path, name)
        write_text_utf8_no_bom(path, protect_sensitive_text(text))
        return os.path.basename(path)

    def run_command(self, operation, parameters=None, allow_failure=False):
        """One whitelisted one-shot hdc operation. Counts attempted/completed,
        writes per-command stdout/stderr raw artifacts, and appends one
        transcript line. Non-whitelist attempts are integrity violations."""
        canonical = normalize_hdc_operation(operation)
        self.hdc_logical_calls += 1
        self.hdc_operations[canonical] = self.hdc_operations.get(canonical, 0) + 1
        self.command_attempted += 1
        try:
            audit_argv = get_hdc_invocation(operation, parameters)
        except RuntimeError as e:
            self.integrity_violations.append('nonwhitelist-command-attempt')
            self.transcript('command-rejected', {'operation': str(operation), 'error': str(e)})
            raise CampaignBlocked('nonwhitelist-command-attempt')
        live_argv = get_live_hdc_arguments(audit_argv, self.target_token(), self.hap_live)
        self.command_seq += 1
        tag = '%02d-%s' % (self.command_seq, canonical.lower())
        started = time.monotonic()
        timeout_hit = False
        proc = None
        try:
            proc = subprocess.run([self.executable] + live_argv, capture_output=True, text=True,
                                  encoding='utf-8', errors='replace', timeout=HDC_TIMEOUT_SECONDS,
                                  env=self.child_env())
            self.hdc_process_starts += 1
            exit_code = proc.returncode
            stdout = proc.stdout or ''
            stderr = proc.stderr or ''
        except subprocess.TimeoutExpired as e:
            timeout_hit = True
            self.hdc_process_starts += 1
            exit_code = 124
            stdout = (e.stdout or b'').decode('utf-8', errors='replace') if isinstance(e.stdout, bytes) else str(e.stdout or '')
            stderr = (e.stderr or b'').decode('utf-8', errors='replace') if isinstance(e.stderr, bytes) else str(e.stderr or 'hdc operation timeout')
        duration_ms = (time.monotonic() - started) * 1000.0
        self.command_completed += 1
        result = HdcResult(exit_code, stdout, stderr, simulated=not self.is_evidence)
        stdout_ref = self._write_raw_text(tag + '.stdout.txt', stdout)
        stderr_ref = self._write_raw_text(tag + '.stderr.txt', stderr)
        self._record_command(canonical, audit_argv, result, duration_ms, stdout_ref, stderr_ref)
        if timeout_hit:
            raise CampaignBlocked('%s-command-timeout' % canonical.lower())
        if exit_code != 0 and not allow_failure:
            raise CampaignBlocked('%s-command-failed' % canonical.lower())
        return result

    def target_token(self):
        if self.is_evidence:
            return actual_target
        return DRY_RUN_TARGET_SENTINEL

    def collect_hilog_window(self, window_seconds):
        """HilogStream: start the stream, collect until the window elapses
        (then process-group kill) or until the stream EOFs early (the DryRun
        fake prints its scripted lines and exits). Returns the analysis
        payload; every raw line is preserved under RawRoot."""
        canonical = 'HilogStream'
        self.hdc_logical_calls += 1
        self.hdc_operations[canonical] = self.hdc_operations.get(canonical, 0) + 1
        self.command_attempted += 1
        audit_argv = get_hdc_invocation(canonical)
        live_argv = get_live_hdc_arguments(audit_argv, self.target_token(), self.hap_live)
        self.command_seq += 1
        stderr_name = '%02d-hilogstream.stderr.txt' % self.command_seq
        stderr_path = os.path.join(self.raw_path, stderr_name)
        started = time.monotonic()
        with open(stderr_path, 'wb') as stderr_file:
            proc = subprocess.Popen([self.executable] + live_argv, stdout=subprocess.PIPE,
                                    stderr=stderr_file, start_new_session=True, env=self.child_env())
            self.hdc_process_starts += 1
            timed_out = False
            try:
                out_bytes, _ = proc.communicate(timeout=window_seconds)
            except subprocess.TimeoutExpired:
                timed_out = True
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                except (ProcessLookupError, PermissionError, OSError):
                    pass
                try:
                    out_bytes, _ = proc.communicate(timeout=RAW_HILOG_GRACE_SECONDS)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    out_bytes, _ = proc.communicate()
        duration_ms = (time.monotonic() - started) * 1000.0
        self.command_completed += 1
        stdout = (out_bytes or b'').decode('utf-8', errors='replace')
        hilog_name = self._write_raw_text('hilog-raw.txt', stdout)
        lines = stdout.splitlines()
        marker_lines = [line for line in lines if MARKER_TOKEN in line]
        tag_lines = [line for line in lines if HILOG_TAG in line]
        result = HdcResult(0 if not timed_out else 0, stdout, '', simulated=not self.is_evidence)
        self.transcript('hdc-command', {
            'operation': canonical,
            'executable': '<HDC_PATH>',
            'arguments': list(audit_argv),
            'exit_code': 0,
            'duration_ms': int(duration_ms),
            'window_seconds': window_seconds,
            'window_elapsed_full': bool(timed_out),
            'lines_total': len(lines),
            'marker_lines': len(marker_lines),
            'tag_lines': len(tag_lines),
            'stdout_raw': hilog_name,
            'stderr_raw': stderr_name,
            'simulated': bool(result.simulated),
        })
        return {
            'stdout': stdout,
            'lines_total': len(lines),
            'tag_lines': tag_lines,
            'marker_lines': marker_lines,
            'window_seconds': window_seconds,
            'window_elapsed_full': bool(timed_out),
        }

    # ---- flow steps -------------------------------------------------------

    def wind_down(self, force_stop_reason):
        """State-gated best-effort cleanup: ForceStop(reason) + Uninstall when
        anything may be installed, RemoveStaging when staging may exist."""
        actions = []

        def attempt(operation, parameters):
            try:
                result = self.run_command(operation, parameters, allow_failure=True)
                actions.append({'operation': operation, 'reason': force_stop_reason,
                                'exit_code': result.exit_code, 'ok': result.exit_code == 0})
            except CampaignBlocked:
                actions.append({'operation': operation, 'reason': force_stop_reason,
                                'exit_code': None, 'ok': False})
                raise

        if self.installed or self.entry_started:
            attempt('ForceStop', {'Bundle': BUNDLE, 'Reason': force_stop_reason})
            attempt('Uninstall', {'Bundle': BUNDLE})
            self.installed = False
            self.entry_started = False
        if self.staging_prepared:
            attempt('RemoveStaging', {})
            self.staging_prepared = False
        self.cleanup_actions.extend(actions)

    def run_absent_probes(self):
        """Post-cleanup directed absent probes: bundle dump, pidof, staging."""
        dump = self.run_command('BundleDump', {'Bundle': BUNDLE}, allow_failure=True)
        pidof = self.run_command('PidOf', {'Bundle': BUNDLE}, allow_failure=True)
        staging = self.run_command('StagingProbe', allow_failure=True)
        dump_absent = (not dump.stdout.strip()) or (BUNDLE not in dump.stdout)
        pidof_absent = not pidof.stdout.strip()
        staging_absent = 'no such file' in staging.combined_text().lower()
        self.absent_probes = {
            'bundle_dump': 'absent' if dump_absent else 'present',
            'pidof': 'absent' if pidof_absent else 'present',
            'staging': 'absent' if staging_absent else 'present',
        }
        self.transcript('absent-probes', dict(self.absent_probes))
        all_absent = all(value == 'absent' for value in self.absent_probes.values())
        self.cleanup_status = 'verified-clean' if all_absent else 'incomplete'


def preflight_live(freeze, repo_root_path):
    """--Live preflight: clean worktree, ready freeze + double binding
    (already enforced at load), real target token, hdc + HAP hash recompute.
    Any failure raises before any evidence root is created (exit 1)."""
    if repo_root_path is None:
        raise RuntimeError('Live requires the git repository root')
    status = get_git_status_porcelain(repo_root_path)
    if status.strip():
        raise RuntimeError('Live requires an empty `git status --porcelain` (worktree is dirty)')
    assert_target_environment()
    hdc_path = normalize_path(str(freeze['hdc']['path']))
    if not os.path.isfile(hdc_path):
        raise RuntimeError('frozen hdc executable missing')
    assert_file_hash('frozen hdc executable', hdc_path, str(freeze['hdc']['sha256']))
    hap_path = normalize_path(str(freeze['artifacts']['hap_path']))
    if not os.path.isfile(hap_path):
        raise RuntimeError('frozen HAP file missing')
    assert_file_hash('frozen HAP', hap_path, str(freeze['artifacts']['hap_sha256']))
    return {'repository_clean': True, 'hdc_sha256_verified': True, 'hap_sha256_verified': True}


def verify_tuple(ctx):
    """Step 2: Version/TupleModel/TupleBuild re-verification; literal tuple
    match (version output must CONTAIN the frozen hdc version; model/build
    compare verbatim after strip). Drift -> blocked ending."""
    version_result = ctx.run_command('Version', allow_failure=True)
    model_result = ctx.run_command('TupleModel', allow_failure=True)
    build_result = ctx.run_command('TupleBuild', allow_failure=True)
    observed_version = version_result.stdout.strip()
    observed_model = model_result.stdout.strip()
    observed_build = build_result.stdout.strip()
    ctx.observed_tuple = {
        'hdc_version_observed': protect_sensitive_text(observed_version),
        'device_model_observed': protect_sensitive_text(observed_model),
        'full_system_build_observed': protect_sensitive_text(observed_build),
    }
    drift = []
    if str(ctx.freeze['hdc']['version']) not in version_result.stdout:
        drift.append('hdc-version')
    if observed_model != str(ctx.freeze['target_tuple']['device_model']):
        drift.append('device-model')
    if observed_build != str(ctx.freeze['target_tuple']['full_system_build']):
        drift.append('full-system-build')
    ctx.transcript('tuple-verify', {'drift': drift, 'observed': dict(ctx.observed_tuple)})
    ctx.steps.append({'step': 'tuple-verify', 'result': 'drift' if drift else 'ok', 'drift': drift})
    if drift:
        raise CampaignBlocked('target-tuple-drift')


def verify_baseline(ctx):
    """Step 3: BundleDump (no install info) + PidOf (empty)."""
    dump = ctx.run_command('BundleDump', {'Bundle': BUNDLE}, allow_failure=True)
    pidof = ctx.run_command('PidOf', {'Bundle': BUNDLE}, allow_failure=True)
    dump_absent = (not dump.stdout.strip()) or (BUNDLE not in dump.stdout)
    ctx.transcript('baseline-probe', {'bundle_dump_absent': bool(dump_absent),
                                      'pidof_empty': not bool(pidof.stdout.strip())})
    ctx.steps.append({'step': 'baseline-probe', 'result': 'ok' if dump_absent and not pidof.stdout.strip() else 'drift'})
    if not dump_absent:
        raise CampaignBlocked('baseline-bundle-present')
    if pidof.stdout.strip():
        raise CampaignBlocked('baseline-process-present')


def install_and_start(ctx):
    """Steps 4-5: MkdirStaging -> SendHap -> InstallHap (success marker) ->
    StartEntry."""
    ctx.run_command('MkdirStaging')
    ctx.staging_prepared = True
    ctx.run_command('SendHap')
    install = ctx.run_command('InstallHap')
    if 'success' not in install.combined_text().lower():
        raise CampaignBlocked('installhap-success-marker-missing')
    ctx.installed = True
    ctx.run_command('StartEntry', {'Bundle': BUNDLE})
    ctx.entry_started = True
    ctx.transcript('install-and-start', {'installed': True, 'entry_started': True})
    ctx.steps.append({'step': 'install-and-start', 'result': 'ok'})


def resolve_verdict(ctx, blocked_reason):
    """Step 10: marker mapping + cleanup/integrity overrides.
    Priority: integrity invalid > blocked reason > pass."""
    mapping = map_markers_to_verdict(ctx.marker_lines)
    ctx.marker_mapping = mapping
    fields = mapping['fields']
    ctx.loader_error = protect_sensitive_text(fields.get('loaderError')) if fields.get('loaderError') is not None else None
    ctx.loader_errno = fields.get('loaderErrno')
    verdict = mapping['verdict']
    fail_reason = mapping['fail_reason']
    if blocked_reason is not None:
        verdict = 'blocked'
        fail_reason = blocked_reason
    if ctx.cleanup_status != 'verified-clean' and verdict == 'pass':
        verdict = 'blocked'
        fail_reason = 'cleanup-incomplete'
    if ctx.integrity_violations:
        verdict = 'invalid'
        fail_reason = 'integrity-violations'
    return verdict, fail_reason


def end_of_run_integrity_checks(ctx):
    """Freeze-mismatch / worktree-drift re-checks at the end of the flow."""
    if sha256_file(ctx.freeze_path) != ctx.freeze_sha256:
        ctx.integrity_violations.append('freeze-file-drift')
    if ctx.is_evidence:
        try:
            assert_file_hash('frozen hdc executable', normalize_path(str(ctx.freeze['hdc']['path'])),
                             str(ctx.freeze['hdc']['sha256']))
        except RuntimeError:
            ctx.integrity_violations.append('hdc-sha256-mismatch')
        try:
            assert_file_hash('frozen HAP', ctx.hap_live, str(ctx.freeze['artifacts']['hap_sha256']))
        except RuntimeError:
            ctx.integrity_violations.append('hap-sha256-mismatch')
        try:
            if get_git_status_porcelain(ctx.repo_root).strip():
                ctx.integrity_violations.append('worktree-dirty')
        except RuntimeError:
            ctx.integrity_violations.append('repository-state-after-unavailable')


def emergency_cleanup(ctx):
    """Any runner exception: ForceStop(exception-cleanup) + Uninstall +
    RemoveStaging best-effort; recorded where possible, never fatal."""
    for operation, parameters in (('ForceStop', {'Bundle': BUNDLE, 'Reason': 'exception-cleanup'}),
                                  ('Uninstall', {'Bundle': BUNDLE}),
                                  ('RemoveStaging', {})):
        try:
            audit_argv = get_hdc_invocation(operation, parameters)
            live_argv = get_live_hdc_arguments(audit_argv, ctx.target_token(), ctx.hap_live)
            subprocess.run([ctx.executable] + live_argv, capture_output=True, text=True,
                           encoding='utf-8', errors='replace', timeout=HDC_TIMEOUT_SECONDS,
                           env=ctx.child_env())
            ctx.cleanup_actions.append({'operation': operation, 'reason': 'exception-cleanup',
                                        'exit_code': 0, 'ok': True})
        except Exception as e:
            ctx.cleanup_actions.append({'operation': operation, 'reason': 'exception-cleanup',
                                        'exit_code': None, 'ok': False,
                                        'error': protect_sensitive_text(str(e))})


def write_hash_manifest(ctx):
    """hash-manifest.json: sha256 of every produced file. EvidenceRoot files
    under `files` (the three seal-family files are bound by the seal itself,
    never self-referential); RawRoot files under `external_raw_files`."""
    manifest_path = os.path.join(ctx.evidence_path, 'hash-manifest.json')
    excluded = ('hash-manifest.json', 'scenario-results.json', 'campaign-seal.json')
    files = []
    for root, dirs, names in os.walk(ctx.evidence_path):
        dirs.sort()
        for name in sorted(names):
            if name in excluded:
                continue
            full = os.path.join(root, name)
            if os.path.isfile(full):
                rel = os.path.relpath(full, ctx.evidence_path).replace(os.sep, '/')
                files.append({'path': rel, 'sha256': sha256_file(full), 'bytes': os.path.getsize(full)})
    files.sort(key=lambda entry: entry['path'])
    external = []
    for root, dirs, names in os.walk(ctx.raw_path):
        dirs.sort()
        for name in sorted(names):
            full = os.path.join(root, name)
            if os.path.isfile(full):
                rel = os.path.relpath(full, ctx.raw_path).replace(os.sep, '/')
                external.append({'path': rel, 'sha256': sha256_file(full), 'bytes': os.path.getsize(full)})
    external.sort(key=lambda entry: entry['path'])
    write_json_file(manifest_path, {
        'schema_version': 1,
        'algorithm': 'SHA-256',
        'generated_at': now_iso(),
        'transcript_chain_head': get_transcript_chain_head(ctx.transcript_path),
        'scope': 'all produced evidence/raw files; scenario-results.json is bound by campaign-seal.json to avoid a self-reference cycle',
        'files': files,
        'external_raw_files': external,
    })
    return manifest_path


def build_scenario_results(ctx, verdict, fail_reason, manifest_path, repository_clean_before):
    """scenario-results.json: every measured field (verdict, markers,
    hdc_execution counts, cleanup result, integrity violations)."""
    record = {
        'schema_version': 1,
        'record_kind': 'g0-scenario-results',
        'evidence_id': str(ctx.freeze['evidence_id']),
        'campaign_id': str(ctx.freeze['campaign_id']),
        'authorization_id': str(ctx.freeze['authorization_id']),
        'attempt': str(ctx.freeze['attempt']),
        'plan_status': str(ctx.freeze['plan_status']),
        'execution_mode': ctx.mode,
        'is_evidence': bool(ctx.is_evidence),
        'non_evidence_reason': 'N/A' if ctx.is_evidence else DRY_RUN_NON_EVIDENCE_REASON,
        'bundle': BUNDLE,
        'ability': ABILITY,
        'module': MODULE,
        'staging': STAGING,
        'hilog_tag': HILOG_TAG,
        'scenario_window_seconds': SCENARIO_WINDOW_SECONDS,
        'marker_format': 'G0_RESULT|k=v|... (pipe-delimited, pre-registered)',
        'target_tuple_expected': dict(ctx.freeze['target_tuple']),
        'target_tuple_observed': dict(ctx.observed_tuple),
        'hdc': {'version': str(ctx.freeze['hdc']['version']), 'sha256': str(ctx.freeze['hdc']['sha256'])},
        'hap_sha256': str(ctx.freeze['artifacts']['hap_sha256']),
        'elf_profile': dict(ctx.freeze['elf_profile']),
        'freeze_file_sha256': ctx.freeze_sha256,
        'runner_py_sha256': str(ctx.freeze['runner_py_sha256']),
        'runner_ps1_sha256': str(ctx.freeze['runner_ps1_sha256']),
        'started_at': ctx.started_at,
        'ended_at': ctx.ended_at,
        'verdict': verdict,
        'fail_reason': fail_reason,
        'markers': dict(ctx.markers),
        'loader_error': ctx.loader_error,
        'loader_errno': ctx.loader_errno,
        'fault_probe': {'fault_lines': ctx.fault_lines,
                        'status': ('not-run' if ctx.fault_lines is None else
                                   ('fault-lines-present' if ctx.fault_lines else 'no-fault-lines'))},
        'hdc_execution': {
            'logical_calls': ctx.hdc_logical_calls,
            'process_starts': ctx.hdc_process_starts,
            'operations': dict(sorted(ctx.hdc_operations.items())),
            'command_attempted': ctx.command_attempted,
            'command_completed': ctx.command_completed,
        },
        'steps': list(ctx.steps),
        'cleanup': {'actions': list(ctx.cleanup_actions), 'status': ctx.cleanup_status},
        'absent_probes': dict(ctx.absent_probes),
        'integrity_violations': list(dict.fromkeys(ctx.integrity_violations)),
        'host_hdc_processes_after': count_hdc_processes(),
        'repository_clean_before': repository_clean_before,
        'repository_clean_after': (not get_git_status_porcelain(ctx.repo_root).strip()) if ctx.is_evidence else None,
        'transcript_reference': {
            'path': 'transcript.redacted.jsonl',
            'sha256': sha256_file(ctx.transcript_path),
            'chain_head': get_transcript_chain_head(ctx.transcript_path),
        },
        'hash_manifest_reference': {
            'path': 'hash-manifest.json',
            'sha256': sha256_file(manifest_path),
        },
        'scope_statement': 'Exact frozen G0 stock-Go arm64 c-shared loadability reachability only; '
                           'no E4-E7, product, data-plane, or E8 OPEN conclusion.',
        'reviewers': 'pending',
    }
    return record


def write_campaign_seal(ctx, verdict, fail_reason):
    """campaign-seal.json: binds scenario-results.json + hash-manifest.json
    with their sha256, plus sealed_at / final_exit_code / run_status /
    fail_reason / verdict / chain head."""
    record_path = os.path.join(ctx.evidence_path, 'scenario-results.json')
    manifest_path = os.path.join(ctx.evidence_path, 'hash-manifest.json')
    write_json_file(os.path.join(ctx.evidence_path, 'campaign-seal.json'), {
        'schema_version': 1,
        'algorithm': 'SHA-256',
        'campaign_id': str(ctx.freeze['campaign_id']),
        'evidence_id': str(ctx.freeze['evidence_id']),
        'execution_mode': ctx.mode,
        'is_evidence': bool(ctx.is_evidence),
        'record': {'path': 'scenario-results.json', 'sha256': sha256_file(record_path)},
        'manifest': {'path': 'hash-manifest.json', 'sha256': sha256_file(manifest_path)},
        'sealed_at': now_iso(),
        'final_exit_code': 0,
        'run_status': 'completed',
        'fail_reason': fail_reason,
        'verdict': verdict,
        'transcript_chain_head': get_transcript_chain_head(ctx.transcript_path),
    })


def run_campaign(freeze, mode, repo_root_path, freeze_path):
    """Full single-scenario S1 flow. Completed flows (pass/blocked/invalid
    verdicts) write scenario-results + hash-manifest + campaign-seal and exit
    0 printing `VERDICT=<verdict>`. Any runner exception triggers the
    exception-cleanup chain and exits 1 WITHOUT a seal."""
    global dry_run, is_evidence, execution_mode, freeze_manifest
    dry_run = (mode == 'dry-run')
    is_evidence = (mode == 'live')
    execution_mode = mode
    freeze_manifest = freeze

    repository_clean_before = None
    if is_evidence:
        preflight_live(freeze, repo_root_path)
        repository_clean_before = True
    else:
        script = os.environ.get('G0_DRYRUN_SCRIPT', '')
        if script not in DRY_RUN_SCRIPTS:
            raise RuntimeError("DryRun requires environment G0_DRYRUN_SCRIPT in {'pass','dlopen-rejected','install-fails'}")

    ctx = CampaignContext(freeze, mode, repo_root_path, freeze_path)
    if dry_run:
        ctx.prepare_fake_hdc()
    else:
        ctx.executable = normalize_path(str(freeze['hdc']['path']))
    ctx.initialize_output_roots()
    ctx.started_at = now_iso()
    ctx.transcript('campaign-start', {
        'campaign_id': str(freeze['campaign_id']),
        'evidence_id': str(freeze['evidence_id']),
        'attempt': str(freeze['attempt']),
        'plan_status': str(freeze['plan_status']),
        'execution_mode': mode,
        'is_evidence': bool(ctx.is_evidence),
        'freeze_file_sha256': ctx.freeze_sha256,
    })
    try:
        blocked_reason = None
        try:
            if is_evidence:
                ctx.transcript('preflight', {'repository_clean': True, 'hdc_sha256_verified': True,
                                             'hap_sha256_verified': True})
                ctx.steps.append({'step': 'preflight', 'result': 'ok'})
            verify_tuple(ctx)
            verify_baseline(ctx)
            install_and_start(ctx)
            stream = ctx.collect_hilog_window(SCENARIO_WINDOW_SECONDS)
            ctx.marker_lines = list(stream['marker_lines'])
            ctx.markers = {
                'count': len(stream['marker_lines']),
                'raw_lines': [protect_sensitive_text(line) for line in stream['marker_lines']],
                'fields': {},
                'hilog_lines_total': stream['lines_total'],
                'hilog_tag_lines_total': len(stream['tag_lines']),
                'window_seconds': stream['window_seconds'],
                'window_elapsed_full': stream['window_elapsed_full'],
            }
            ctx.transcript('hilog-collect', {'count': ctx.markers['count'],
                                             'window_elapsed_full': ctx.markers['window_elapsed_full']})
            ctx.steps.append({'step': 'hilog-collect', 'result': 'ok', 'marker_count': ctx.markers['count']})
            fault = ctx.run_command('FaultProbe', allow_failure=True)
            ctx.fault_lines = len([line for line in fault.stdout.splitlines() if line.strip()])
            ctx.transcript('fault-probe', {'fault_lines': ctx.fault_lines})
            ctx.steps.append({'step': 'fault-probe', 'result': 'ok'})
        except CampaignBlocked as blocked:
            blocked_reason = blocked.reason
            ctx.transcript('campaign-blocked', {'reason': blocked_reason})
            ctx.steps.append({'step': 'flow', 'result': 'blocked', 'reason': blocked_reason})
        # Wind-down + directed absent probes (final-cleanup), for every ending.
        try:
            ctx.wind_down('final-cleanup')
            ctx.run_absent_probes()
        except CampaignBlocked:
            ctx.cleanup_status = 'incomplete'
        ctx.transcript('cleanup', {'status': ctx.cleanup_status,
                                   'actions': protect_sensitive_data(ctx.cleanup_actions)})
        end_of_run_integrity_checks(ctx)
        ctx.ended_at = now_iso()
        verdict, fail_reason = resolve_verdict(ctx, blocked_reason)
        ctx.markers['fields'] = dict(ctx.marker_mapping['fields']) if ctx.marker_mapping else {}
        ctx.transcript('verdict-resolved', {'verdict': verdict, 'fail_reason': fail_reason,
                                            'integrity_violations': list(ctx.integrity_violations)})
        manifest_path = write_hash_manifest(ctx)
        record = build_scenario_results(ctx, verdict, fail_reason, manifest_path, repository_clean_before)
        write_json_file(os.path.join(ctx.evidence_path, 'scenario-results.json'), record)
        write_campaign_seal(ctx, verdict, fail_reason)
        print('G0_CAMPAIGN_RESULT mode=%s verdict=%s fail_reason=%s evidence_root=%s raw_root=%s '
              'hdc_process_starts=%d' % (mode, verdict, fail_reason, ctx.evidence_path, ctx.raw_path,
                                         ctx.hdc_process_starts))
        print('VERDICT=%s' % verdict)
        return 0
    except Exception:
        # Runner-level failure: best-effort exception cleanup, no seal, exit 1.
        try:
            emergency_cleanup(ctx)
        except Exception:
            pass
        raise


# =====================================================================
# Section 9: Embedded --SelfTest (basic import-level checks)
# =====================================================================


def embedded_selftest():
    """Basic import-level self-check: pure functions only - no hdc, no device,
    no evidence files. Prints SELFTEST_PASS/FAIL lines and ends with
    SELFTEST_RESULT=pass|fail HDC_PROCESSES=<n>. Returns 0/1."""
    failures = []
    hdc_process_count = count_hdc_processes()

    def check(name, condition):
        if condition:
            print('SELFTEST_PASS=%s' % name)
        else:
            failures.append(name)
            print('SELFTEST_FAIL=%s' % name)

    def raises(exc_type, fn):
        try:
            fn()
            return False
        except exc_type:
            return True
        except Exception:
            return False

    if hdc_process_count < 0:
        print('SELFTEST_WARN=hdc-process-count-unknown')
    check('sha256-empty-vector',
          sha256_text('') == 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')
    check('whitelist-operation-count', len(HDC_WHITELIST) == 15)
    check('invocation-version', get_hdc_invocation('Version') == ['version'])
    check('invocation-tuple-model',
          get_hdc_invocation('TupleModel') == ['-t', TARGET_PLACEHOLDER, 'shell', 'param', 'get', 'const.product.model'])
    check('invocation-bundledump',
          get_hdc_invocation('BundleDump', {'Bundle': BUNDLE})
          == ['-t', TARGET_PLACEHOLDER, 'shell', 'bm', 'dump', '-n', BUNDLE])
    check('invocation-pidof-ui-process',
          get_hdc_invocation('PidOf', {'Bundle': BUNDLE})
          == ['-t', TARGET_PLACEHOLDER, 'shell', 'pidof', BUNDLE])
    check('invocation-sendhap',
          get_hdc_invocation('SendHap')
          == ['-t', TARGET_PLACEHOLDER, 'file', 'send', HAP_PLACEHOLDER, STAGING + '/hap/g0.hap'])
    check('invocation-hilogstream',
          get_hdc_invocation('HilogStream')
          == ['-t', TARGET_PLACEHOLDER, 'shell', 'hilog', '-T', HILOG_TAG, '-v', 'year', '-v', 'zone'])
    check('invocation-forcestop',
          get_hdc_invocation('ForceStop', {'Bundle': BUNDLE, 'Reason': 'final-cleanup'})
          == ['-t', TARGET_PLACEHOLDER, 'shell', 'aa', 'force-stop', BUNDLE])
    check('reject-unknown-operation', raises(RuntimeError, lambda: get_hdc_invocation('Screenshot')))
    check('reject-extra-parameter',
          raises(RuntimeError, lambda: get_hdc_invocation('Version', {'Bundle': BUNDLE})))
    check('reject-missing-parameter', raises(RuntimeError, lambda: get_hdc_invocation('PidOf')))
    check('reject-foreign-bundle',
          raises(RuntimeError, lambda: get_hdc_invocation('PidOf', {'Bundle': 'cn.example.other'})))
    check('reject-forcestop-bad-reason',
          raises(RuntimeError, lambda: get_hdc_invocation('ForceStop', {'Bundle': BUNDLE, 'Reason': 'reboot'})))
    check('case-insensitive-aliases',
          get_hdc_invocation('bundledump', {'bundle': BUNDLE}) == get_hdc_invocation('BundleDump', {'Bundle': BUNDLE}))
    check('target-token-positive', test_physical_target_token('192.168.1.100:5555'))
    check('target-token-negative-placeholder',
          all(not test_physical_target_token(value) for value in
              (None, '', '   ', ' x', 'x ', 'a b', 'a,b', 'a;b', '-t', 'PHYS-1', 'phys-1', '<PHYS_1_TARGET>')))
    check('marker-pass-mapping',
          map_markers_to_verdict(['x G0GoProbe: G0_RESULT|verdict=PASS|ok=true|pid=1|stage=complete|'
                                  'dlopenLoaded=true|loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576'])
          ['verdict'] == 'pass')
    check('marker-dlopen-mapping',
          map_markers_to_verdict(['G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|'
                                  'loaderErrno=2|loaderError=initial-exec TLS resolves to dynamic definition'])
          == {'verdict': 'blocked', 'fail_reason': 'dlopen-blocked',
              'fields': dict(parse_marker_fields('G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|'
                                                 'dlopenLoaded=false|loaderErrno=2|'
                                                 'loaderError=initial-exec TLS resolves to dynamic definition'))})
    check('marker-missing-mapping', map_markers_to_verdict([])['fail_reason'] == 'marker-missing')
    check('marker-ambiguous-mapping',
          map_markers_to_verdict(['G0_RESULT|verdict=PASS', 'G0_RESULT|verdict=PASS'])['fail_reason'] == 'marker-ambiguous')
    check('marker-drift-mapping',
          map_markers_to_verdict(['G0_RESULT|verdict=PASS|ok=true|stage=complete|hello=7|runtimeBytes=1'])['fail_reason'] == 'drift')
    check('ps-count-first-column-only',
          count_hdc_from_ps_output('hdc -t foo\nfake-hdc -t bar\npython3 x.py\nhdcx y\nsshd: /usr/sbin/sshd -D') == 1)
    if hdc_process_count >= 0:
        check('host-hdc-processes-zero', hdc_process_count == 0)
    print('SELFTEST_RESULT=%s HDC_PROCESSES=%d' % ('fail' if failures else 'pass', hdc_process_count))
    return 1 if failures else 0


# =====================================================================
# Section 10: Main flow (argparse entry, mode dispatch)
# =====================================================================


class _ArgParseError(Exception):
    """argparse parse errors map to the pre-record gate exit code 1 instead of
    argparse's default SystemExit(2) (E3 MINOR-4)."""


class _G0ArgumentParser(argparse.ArgumentParser):
    def error(self, message):
        raise _ArgParseError(message)


def parse_args(argv=None):
    parser = _G0ArgumentParser(
        prog='g0-phys-probe-campaign.py',
        description='G0-PHYS-PROBE campaign runner (stock Go arm64 c-shared loader probe)',
        allow_abbrev=False)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument('--version', dest='version', action='store_true',
                      help='print the runner version and exit')
    mode.add_argument('--SelfTest', '--self-test', dest='self_test', action='store_true',
                      help='embedded import-level self-check')
    mode.add_argument('--TargetBindingConfirm', '--target-binding-confirm',
                      dest='target_binding_confirm', action='store_true',
                      help='run the three target-binding probes and write the double-file record')
    mode.add_argument('--DryRun', '--dry-run', dest='dry_run', action='store_true',
                      help='full scenario flow against the sandboxed fake hdc (is_evidence=false)')
    mode.add_argument('--Live', dest='live', action='store_true',
                      help='full scenario flow against the physical device (is_evidence=true)')
    parser.add_argument('--Freeze', '--freeze', dest='freeze', default=None,
                        help='path to the freeze manifest JSON')
    parser.add_argument('--ConfirmationRecord', '--confirmation-record', dest='confirmation_record',
                        default=None, help='out-of-repo target-binding confirmation record path (single-use)')
    return parser.parse_args(argv)


def main(argv=None):
    """Gate order: argparse -> mode exclusivity -> version/selftest early
    exits -> freeze load/validate -> dispatch. Exit codes: 0=pass/completed,
    1=pre-record gate or any pre-campaign/runner validation error, 2=probe
    blocked (TargetBindingConfirm)."""
    try:
        args = parse_args(argv)
    except _ArgParseError as e:
        sys.stderr.write('%s\n' % protect_sensitive_text(str(e)))
        return 1
    try:
        if args.version:
            print('g0-phys-probe-campaign.py %s' % RUNNER_VERSION)
            return 0
        if args.self_test:
            return embedded_selftest()
        if args.target_binding_confirm:
            mode = 'target-binding-confirm'
        elif args.dry_run:
            mode = 'dry-run'
        else:
            mode = 'live'
        if mode != 'target-binding-confirm' and (args.confirmation_record or '').strip():
            raise RuntimeError('--ConfirmationRecord is only valid with --TargetBindingConfirm')
        if mode == 'target-binding-confirm' and not (args.confirmation_record or '').strip():
            raise RuntimeError('--TargetBindingConfirm requires --ConfirmationRecord')
        if not (args.freeze or '').strip():
            raise RuntimeError('--Freeze is required for %s' % mode)
        repo_root_path = resolve_repository_root()
        freeze_path = normalize_path(args.freeze)
        freeze = load_freeze(freeze_path, mode, repo_root_path)
        if mode == 'target-binding-confirm':
            return invoke_target_binding_confirm(freeze, freeze_path, args.confirmation_record, repo_root_path)
        return run_campaign(freeze, mode, repo_root_path, freeze_path)
    except PreRecordGateError as e:
        sys.stderr.write('%s\n' % protect_sensitive_text(str(e)))
        return 1
    except Exception as e:
        sys.stderr.write('RUNNER_FAILURE=%s\n' % protect_sensitive_text(str(e)))
        return 1


if __name__ == '__main__':
    sys.exit(main())
