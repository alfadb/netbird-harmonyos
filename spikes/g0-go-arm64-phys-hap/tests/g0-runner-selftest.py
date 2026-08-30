#!/usr/bin/env python3
"""G0-PHYS-PROBE runner selftest (tests/g0-runner-selftest.py).

Host-only selftest for g0-phys-probe-campaign.py. No real hdc, no device, no
network: the target-binding and full-campaign paths run against fake hdc
executables installed inside a per-test temp sandbox; the DryRun sentinel hdc
(spearate from the fake) writes a marker file if it is EVER executed, proving
--DryRun never touches the frozen hdc path.

Coverage (spec order):
  1. HDC whitelist: exact audit argv for all 15 operations; unknown operation /
     extra parameter / missing parameter / foreign bundle / illegal
     ForceStop Reason rejections; casefold aliases.
  2. PHYS_1_TARGET token: E3-verbatim positive/negative matrix incl. leading
     '-', embedded whitespace, 'PHYS-1', '<PHYS_1_TARGET>' placeholder; and
     assert_target_environment rejection via the process environment.
  3. Marker parsing / pre-registered verdict mapping: pass, dlopen-blocked
     (loaderError verbatim, loaderErrno=2), drift, duplicate, missing.
  4. Freeze validation: valid load; missing key; extra key; ready without a
     pass confirmation; review pass over a pending machine confirmation;
     runner_py_sha256 mismatch vs the actual runner file; declared-pass record
     hash mismatch; Live requires ready; frozen-value drift.
  5. TargetBindingConfirm via subprocess + fake hdc: pass record (double-file,
     single-use), pre-record gate exit 1, tuple drift exit 2 with a blocked
     record, invalid target token exit 2.
  6. Full --DryRun (G0_DRYRUN_SCRIPT=pass): exit 0, VERDICT=pass,
     is_evidence=false, seal binds scenario-results + hash-manifest, manifest
     hashes verify, transcript line chain recomputes, raw artifacts exist,
     target token never appears in any evidence/raw file, sentinel hdc not
     executed, host hdc comm count stays 0.
  7. Full --DryRun (dlopen-rejected): verdict=blocked with reason
     dlopen-blocked and the loader error preserved verbatim.
  8. Host HDC count probe: /usr/bin/ps -eo comm=,args= first-column-only
     semantics; no process with comm 'hdc' (the fake is named fake-hdc).

Run: python3 tests/g0-runner-selftest.py        (exit 0 iff every check passes)
Output: PASS/FAIL per check, then TOTAL/PASSED/FAILED counts and
SELFTEST_RESULT=pass|fail.
"""

import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import traceback

# =====================================================================
# Constants
# =====================================================================

HERE = os.path.dirname(os.path.abspath(__file__))
G0_DIR = os.path.dirname(HERE)
RUNNER_SRC = os.path.join(G0_DIR, 'g0-phys-probe-campaign.py')
FAKE_HDC_FIXTURE = os.path.join(HERE, 'fixtures', 'fake-hdc.py')

BUNDLE = 'cn.alfadb.netbird.g0probe'
ABILITY = 'EntryAbility'
MODULE = 'entry'
STAGING = '/data/local/tmp/netbird-g0'
HILOG_TAG = 'G0GoProbe'
AUTH_ID = 'AUTH-G0PHYS1API26-20260830-0001'
CAMPAIGN_ID = 'G0-PHYS-PROBE-20260830-0001'
EVIDENCE_ID = 'EV-G0PHYS1API26-20260830-0001'
MODEL = 'PLA-AL10'
BUILD = 'PLA-AL10 7.0.0.102(SP8C00E102R7P3)'
HDC_VERSION = 'SELFTEST-G0-HDC-1.0'
TARGET_TOKEN = '192.168.1.100:5555'
DLOPEN_ERROR = 'initial-exec TLS resolves to dynamic definition'

# HDC-MUST-NOT-START sentinel: if --DryRun ever executes the frozen hdc path,
# this script writes a marker file next to itself so the test detects it. Its
# bytes are fixed, so the freeze can pin its sha256.
SENTINEL_SCRIPT = (
    '#!/bin/sh\n'
    '# G0 DryRun HDC-MUST-NOT-START sentinel: if the runner ever executes the\n'
    '# frozen hdc path, a marker file appears next to this script.\n'
    'printf executed > "$(dirname "$0")/SENTINEL-EXECUTED" 2>/dev/null || true\n'
    'exit 0\n'
)

# Fake hdc for TargetBindingConfirm subprocess tests: answers the three
# target-binding probes (live-form argv, -t pair skipped).
TBC_FAKE_HDC_SCRIPT = (
    '#!/bin/sh\n'
    '# G0 TargetBindingConfirm fake hdc: fixture answers for the three\n'
    '# target-binding probes (Version / TupleModel / TupleBuild).\n'
    'if [ "$1" = "-t" ]; then shift 2; fi\n'
    'case "$1" in\n'
    '  version)\n'
    '    echo "OpenHarmony 3.2.0 %s"\n'
    '    exit 0\n'
    '    ;;\n'
    '  shell)\n'
    '    if [ "$2" = "param" ] && [ "$4" = "const.product.model" ]; then\n'
    '      echo "%s"\n'
    '      exit 0\n'
    '    fi\n'
    '    if [ "$2" = "param" ] && [ "$4" = "const.product.software.version" ]; then\n'
    '      echo "%s"\n'
    '      exit 0\n'
    '    fi\n'
    '    ;;\n'
    'esac\n'
    'echo "tbc-fake-hdc: unsupported argv" >&2\n'
    'exit 9\n'
) % (HDC_VERSION, MODEL, BUILD)

TBC_FAKE_HDC_DRIFT_SCRIPT = TBC_FAKE_HDC_SCRIPT.replace(
    "echo \"%s\"" % MODEL, "echo \"WRONG-MODEL-9\"")

# =====================================================================
# Runner module import (pure functions only; importing never executes hdc)
# =====================================================================

_spec = importlib.util.spec_from_file_location('g0_phys_probe_runner', RUNNER_SRC)
if _spec is None or _spec.loader is None:
    raise RuntimeError('unable to import runner module: %s' % RUNNER_SRC)
runner = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(runner)

# =====================================================================
# Small helpers
# =====================================================================


def sha256_file(path):
    with open(path, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def sha256_text(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def write_text(path, text):
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def run_runner(args, env=None, timeout=300):
    """Run the runner as a real subprocess."""
    full_env = dict(os.environ)
    full_env.pop('PHYS_1_TARGET', None)
    full_env.pop('G0_DRYRUN_SCRIPT', None)
    if env:
        full_env.update(env)
    return subprocess.run([sys.executable, RUNNER_SRC] + args, capture_output=True,
                          text=True, encoding='utf-8', errors='replace',
                          timeout=timeout, env=full_env)


def expect_raises(exc_type, fn):
    try:
        fn()
    except exc_type:
        return True
    except Exception:
        return False
    return False


# =====================================================================
# Freeze sandbox construction
# =====================================================================


def write_governance_record(sandbox, name, kind):
    """Write a pass governance record (parseable JSON) and return (path, sha)."""
    path = os.path.join(sandbox, name)
    write_text(path, json.dumps({
        'schema_version': 1,
        'record_kind': kind,
        'verdict': 'pass',
        'campaign_id': CAMPAIGN_ID,
        'evidence_id': EVIDENCE_ID,
    }, indent=2, ensure_ascii=False) + '\n')
    return path, sha256_file(path)


def build_freeze(sandbox, *, plan_status='blocked', runner_sha=None,
                 confirmation='pending', review='pending', live=False):
    """Build a schema-valid freeze in the sandbox and return its path. The
    frozen hdc path is the MUST-NOT-START sentinel; DryRun must never run it."""
    os.makedirs(sandbox, exist_ok=True)
    hap_path = os.path.join(sandbox, 'app.hap')
    write_text(hap_path, 'G0-FAKE-HAP-BYTES\n')
    sentinel_path = os.path.join(sandbox, 'hdc-sentinel.sh')
    write_text(sentinel_path, SENTINEL_SCRIPT)
    os.chmod(sentinel_path, 0o755)
    confirmation_path = os.path.join(sandbox, 'confirmation-record.json')
    review_path = os.path.join(sandbox, 'review-record.json')
    confirmation_sha = '0' * 64
    review_sha = '0' * 64
    if confirmation == 'pass':
        confirmation_path, confirmation_sha = write_governance_record(
            sandbox, 'confirmation-record.json', 'g0-target-binding-confirmation')
    if review == 'pass':
        review_path, review_sha = write_governance_record(
            sandbox, 'review-record.json', 'g0-ready-freeze-review')
    freeze = {
        'schema_version': 1,
        'authorization_id': AUTH_ID,
        'campaign_id': CAMPAIGN_ID,
        'evidence_id': EVIDENCE_ID,
        'attempt': 'initial',
        'plan_status': plan_status,
        'code_sha': 'a' * 40,
        'runner_py_sha256': runner_sha if runner_sha is not None else sha256_file(RUNNER_SRC),
        'runner_ps1_sha256': sha256_file(os.path.join(os.path.dirname(RUNNER_SRC), 'g0-phys-probe-campaign.ps1')),
        'selftest_py_sha256': sha256_file(os.path.abspath(__file__)),
        'selftest_ps1_sha256': sha256_file(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'g0-runner-selftest.ps1')),
        'hdc': {'path': sentinel_path, 'sha256': sha256_file(sentinel_path), 'version': HDC_VERSION},
        'bundle': BUNDLE,
        'ability': ABILITY,
        'module': MODULE,
        'staging': STAGING,
        'hilog_tag': HILOG_TAG,
        'scenario_window_seconds': 60,
        'target_tuple': {
            'distribution': 'HarmonyOS',
            'device_model': MODEL,
            'full_system_build': BUILD,
            'api': '26',
            'kernel_architecture': 'aarch64',
            'app_abi': 'arm64-v8a',
        },
        'artifacts': {
            'hap_path': hap_path,
            'hap_sha256': sha256_file(hap_path),
            'profile_sha256': 'c' * 64,
            'certificate_sha256': 'd' * 64,
            'libgoprobe_sha256': 'e' * 64,
            'libgoloader_sha256': 'f' * 64,
        },
        'elf_profile': {'pt_tls': True, 'tprel64_count': 1, 'static_tls_flag': False,
                        'needed': ['libc.so']},
        'evidence_roots': {
            'dry_run': os.path.join(sandbox, 'evidence-dry'),
            'live': os.path.join(sandbox, 'evidence-live'),
        },
        'raw_roots': {
            'dry_run': os.path.join(sandbox, 'raw-dry'),
            'live': os.path.join(sandbox, 'raw-live'),
        },
        'confirmation': {
            'status': confirmation,
            'record_path': confirmation_path,
            'record_sha256': confirmation_sha,
            'authorization_id': AUTH_ID,
        },
        'review': {
            'status': review,
            'record_path': review_path,
            'record_sha256': review_sha,
        },
        'operator': 'authorized user',
        'orchestrator': 'main agent',
    }
    freeze_path = os.path.join(sandbox, 'freeze-live.json' if live else 'freeze.json')
    write_text(freeze_path, json.dumps(freeze, indent=2, ensure_ascii=False) + '\n')
    return freeze_path


def load_freeze_from_file(freeze_path, mode='dry-run'):
    return runner.load_freeze(freeze_path, mode, None)


def install_tbc_fake_hdc(sandbox, script):
    path = os.path.join(sandbox, 'tbc-fake-hdc.sh')
    write_text(path, script)
    os.chmod(path, 0o755)
    return path


def retarget_freeze_hdc(freeze_path, hdc_path):
    """Point an existing freeze at a new hdc executable (hash recomputed)."""
    freeze = json.loads(open(freeze_path, 'r', encoding='utf-8-sig').read())
    freeze['hdc'] = {'path': hdc_path, 'sha256': sha256_file(hdc_path), 'version': HDC_VERSION}
    write_text(freeze_path, json.dumps(freeze, indent=2, ensure_ascii=False) + '\n')


# =====================================================================
# Tests
# =====================================================================

TESTS = []


def test(name):
    def decorator(fn):
        TESTS.append((name, fn))
        return fn
    return decorator


# ---- 1. whitelist -------------------------------------------------------------


@test('whitelist-exact-audit-argv-all-15-operations')
def _():
    target = runner.TARGET_PLACEHOLDER
    expected = {
        'Version': ['version'],
        'TupleModel': ['-t', target, 'shell', 'param', 'get', 'const.product.model'],
        'TupleBuild': ['-t', target, 'shell', 'param', 'get', 'const.product.software.version'],
        'BundleDump': ['-t', target, 'shell', 'bm', 'dump', '-n', BUNDLE],
        'PidOf': ['-t', target, 'shell', 'pidof', BUNDLE],
        'MkdirStaging': ['-t', target, 'shell', 'mkdir', '-p', STAGING + '/hap'],
        'SendHap': ['-t', target, 'file', 'send', runner.HAP_PLACEHOLDER, STAGING + '/hap/g0.hap'],
        'InstallHap': ['-t', target, 'shell', 'bm', 'install', '-p', STAGING + '/hap'],
        'StartEntry': ['-t', target, 'shell', 'aa', 'start', '-a', ABILITY, '-b', BUNDLE, '-m', MODULE],
        'HilogStream': ['-t', target, 'shell', 'hilog', '-T', HILOG_TAG, '-v', 'year', '-v', 'zone'],
        'FaultProbe': ['-t', target, 'shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth',
                       '1', '-type', 'f', '-name', '*%s*' % BUNDLE, '-print'],
        'ForceStop': ['-t', target, 'shell', 'aa', 'force-stop', BUNDLE],
        'Uninstall': ['-t', target, 'shell', 'bm', 'uninstall', '-n', BUNDLE],
        'RemoveStaging': ['-t', target, 'shell', 'rm', '-rf', STAGING],
        'StagingProbe': ['-t', target, 'shell', 'ls', '-ld', STAGING],
    }
    assert len(runner.HDC_WHITELIST) == 15, 'whitelist must hold exactly 15 operations'
    for operation, argv in expected.items():
        parameters = {}
        if 'Bundle' in runner.HDC_WHITELIST[operation]:
            parameters['Bundle'] = BUNDLE
        if 'Reason' in runner.HDC_WHITELIST[operation]:
            parameters['Reason'] = 'final-cleanup'
        assert runner.get_hdc_invocation(operation, parameters) == argv, operation
    # Audit form never leaks a real target: the placeholder is verbatim.
    for operation in expected:
        assert runner.TARGET_PLACEHOLDER not in ('192.168.1.100:5555',)


@test('whitelist-rejects-unknown-operation')
def _():
    for operation in ('Screenshot', 'ShellRaw', 'ListTargets', 'version!', ''):
        assert expect_raises(RuntimeError, lambda op=operation: runner.get_hdc_invocation(op)), operation


@test('whitelist-rejects-extra-parameter')
def _():
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation('Version', {'Bundle': BUNDLE}))
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation('TupleModel', {'Reason': 'x'}))
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation(
        'BundleDump', {'Bundle': BUNDLE, 'Extra': '1'}))


@test('whitelist-rejects-missing-parameter')
def _():
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation('PidOf'))
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation('StartEntry', {'Bundle': ''}))
    assert expect_raises(RuntimeError, lambda: runner.get_hdc_invocation('ForceStop', {'Bundle': BUNDLE}))


@test('whitelist-rejects-foreign-bundle')
def _():
    for operation in ('BundleDump', 'PidOf', 'StartEntry', 'Uninstall', 'ForceStop'):
        parameters = {'Bundle': 'cn.example.foreign'}
        if operation == 'ForceStop':
            parameters['Reason'] = 'final-cleanup'
        assert expect_raises(RuntimeError, lambda op=operation, p=dict(parameters):
                             runner.get_hdc_invocation(op, p)), operation


@test('whitelist-rejects-forcestop-illegal-reason')
def _():
    for reason in ('reboot', 'cleanup', '', 'exception-Cleanup', 'final-cleanup '):
        assert expect_raises(RuntimeError, lambda r=reason:
                             runner.get_hdc_invocation('ForceStop', {'Bundle': BUNDLE, 'Reason': r})), reason
    for reason in runner.FORCE_STOP_REASONS:
        argv = runner.get_hdc_invocation('ForceStop', {'Bundle': BUNDLE, 'Reason': reason})
        assert argv[-1] == BUNDLE and 'force-stop' in argv


@test('whitelist-case-insensitive-aliases')
def _():
    assert runner.normalize_hdc_operation('bundledump') == 'BundleDump'
    assert runner.normalize_hdc_operation('FORCESTOP') == 'ForceStop'
    argv_lower = runner.get_hdc_invocation('pidof', {'bundle': BUNDLE})
    argv_upper = runner.get_hdc_invocation('PIDOF', {'BUNDLE': BUNDLE})
    assert argv_lower == argv_upper == ['-t', runner.TARGET_PLACEHOLDER, 'shell', 'pidof', BUNDLE]


# ---- 2. target token ------------------------------------------------------------


@test('target-token-accepts-real-tokens')
def _():
    for token in ('192.168.1.100:5555', 'aabbccdd.eeff00112233', 'emulator-5554X'):
        assert runner.test_physical_target_token(token), token


@test('target-token-rejects-leading-dash')
def _():
    for token in ('-t', '--flag', '-192.168.1.100'):
        assert not runner.test_physical_target_token(token), token


@test('target-token-rejects-whitespace')
def _():
    for token in ('', '   ', ' x', 'x ', 'a b', 'a\tb', 'a\nb', None):
        assert not runner.test_physical_target_token(token), repr(token)
    assert not runner.test_physical_target_token('a,b')
    assert not runner.test_physical_target_token('a;b')


@test('target-token-rejects-phys-1-and-placeholders')
def _():
    for token in ('PHYS-1', 'phys-1', 'Phys-1', '<PHYS_1_TARGET>', '<anything>', '<T>'):
        assert not runner.test_physical_target_token(token), token


@test('assert-target-environment-rejects-invalid-env-values')
def _():
    saved = os.environ.get('PHYS_1_TARGET')
    try:
        for bad in (None, '', '   ', ' padded', '-leading', 'with space', 'a,b', 'a;b',
                    'PHYS-1', '<PHYS_1_TARGET>'):
            if bad is None:
                os.environ.pop('PHYS_1_TARGET', None)
            else:
                os.environ['PHYS_1_TARGET'] = bad
            assert expect_raises(RuntimeError, runner.assert_target_environment), repr(bad)
        os.environ['PHYS_1_TARGET'] = TARGET_TOKEN
        runner.assert_target_environment()
        assert runner.actual_target == TARGET_TOKEN
    finally:
        if saved is None:
            os.environ.pop('PHYS_1_TARGET', None)
        else:
            os.environ['PHYS_1_TARGET'] = saved


# ---- 3. marker parsing / verdict mapping -----------------------------------------


PASS_MARKER = ('2026-08-30 12:00:00.100  12345  67890 I G0GoProbe: '
               'G0_RESULT|verdict=PASS|ok=true|pid=12345|stage=complete|dlopenLoaded=true|'
               'loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576')
DLOPEN_MARKER = ('2026-08-30 12:00:00.100  12345  67890 E G0GoProbe: '
                 'G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|'
                 'loaderErrno=2|loaderError=' + DLOPEN_ERROR + '|hello=0|runtimeBytes=0')


@test('marker-pass-classification')
def _():
    mapping = runner.map_markers_to_verdict([PASS_MARKER])
    assert mapping['verdict'] == 'pass' and mapping['fail_reason'] is None
    assert mapping['fields']['hello'] == '42' and mapping['fields']['runtimeBytes'] == '1048576'
    assert mapping['fields']['stage'] == 'complete' and mapping['fields']['ok'] == 'true'


@test('marker-dlopen-blocked-classification-verbatim-loader-error')
def _():
    mapping = runner.map_markers_to_verdict([DLOPEN_MARKER])
    assert mapping['verdict'] == 'blocked' and mapping['fail_reason'] == 'dlopen-blocked'
    assert mapping['fields']['loaderError'] == DLOPEN_ERROR
    assert mapping['fields']['loaderErrno'] == '2'
    # A FAIL@dlopen marker with an EMPTY loaderError is drift, not a valid result.
    empty = runner.map_markers_to_verdict(
        [DLOPEN_MARKER.replace('loaderError=' + DLOPEN_ERROR, 'loaderError=')])
    assert empty['verdict'] == 'blocked' and empty['fail_reason'] == 'drift'


@test('marker-drift-classification')
def _():
    cases = [
        # DRIFT verdict line
        'x G0_RESULT|verdict=DRIFT|ok=false|pid=1|stage=complete',
        # wrong hello value
        PASS_MARKER.replace('hello=42', 'hello=7'),
        # wrong runtimeBytes
        PASS_MARKER.replace('runtimeBytes=1048576', 'runtimeBytes=2048'),
        # FAIL but not dlopen stage
        'x G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=native-throw|loaderErrno=0|loaderError=boom',
        # FAIL@dlopen with empty loaderError
        'x G0_RESULT|verdict=FAIL|ok=false|stage=dlopen|loaderErrno=2|loaderError=',
        # non-FAIL unexpected fields
        'x G0_RESULT|verdict=PASS|ok=false|stage=complete|hello=42|runtimeBytes=1048576',
    ]
    for line in cases:
        mapping = runner.map_markers_to_verdict([line])
        assert mapping['verdict'] == 'blocked' and mapping['fail_reason'] == 'drift', line


@test('marker-key-case-drift-classification')
def _():
    # Field keys are matched case-sensitively: a different-case key is a
    # distinct field and must never satisfy or override the exact-case
    # pre-registered lookups (review BLOCKER-2 regression matrix).
    cases = [
        # uppercase-V Verdict does not satisfy verdict=PASS
        PASS_MARKER.replace('verdict=PASS', 'Verdict=PASS'),
        # uppercase OK does not satisfy ok=true
        PASS_MARKER.replace('ok=true', 'OK=true'),
        # fully uppercase keys must not reach the dlopen-blocked branch
        ('x G0_RESULT|VERDICT=FAIL|ok=false|pid=0|STAGE=dlopen|dlopenLoaded=false|'
         'loaderErrno=2|LOADERERROR=boom|hello=0|runtimeBytes=0'),
        # a trailing different-case verdict must not override verdict=FAIL
        'x G0_RESULT|verdict=FAIL|Verdict=PASS',
        # same-case duplicate key: last occurrence wins, still not pass
        'x G0_RESULT|verdict=PASS|verdict=FAIL',
    ]
    for line in cases:
        mapping = runner.map_markers_to_verdict([line])
        assert mapping['verdict'] == 'blocked', line
        assert mapping['fail_reason'] == 'drift', (line, mapping['fail_reason'])


@test('marker-missing-classification')
def _():
    mapping = runner.map_markers_to_verdict([])
    assert mapping['verdict'] == 'blocked' and mapping['fail_reason'] == 'marker-missing'


@test('marker-ambiguous-classification')
def _():
    for lines in ([PASS_MARKER, PASS_MARKER], [PASS_MARKER, DLOPEN_MARKER]):
        mapping = runner.map_markers_to_verdict(lines)
        assert mapping['verdict'] == 'blocked' and mapping['fail_reason'] == 'marker-ambiguous'


@test('marker-field-parser-handles-spaced-values')
def _():
    fields = runner.parse_marker_fields(DLOPEN_MARKER)
    assert fields['loaderError'] == DLOPEN_ERROR
    assert fields['verdict'] == 'FAIL' and fields['stage'] == 'dlopen'
    # parser only consumes text from the marker token onward
    assert runner.parse_marker_fields('prefix G0GoProbe: 2026-08-30 12:00:00') == {}
    assert runner.parse_marker_fields('noise G0_RESULT|verdict=PASS')['verdict'] == 'PASS'


# ---- 4. freeze validation ---------------------------------------------------------


@test('freeze-valid-blocked-and-ready-load')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-ok-')
    try:
        blocked = build_freeze(os.path.join(sandbox, 'b'), plan_status='blocked')
        freeze = load_freeze_from_file(blocked)
        assert freeze['bundle'] == BUNDLE
        ready = build_freeze(os.path.join(sandbox, 'r'), plan_status='ready',
                             confirmation='pass', review='pass')
        freeze = load_freeze_from_file(ready)
        assert freeze['plan_status'] == 'ready'
        # Live mode requires ready; ready freeze passes the live gate.
        load_freeze_from_file(ready, mode='live')
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-missing-key')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-missing-')
    try:
        freeze_path = build_freeze(sandbox)
        freeze = json.loads(open(freeze_path, encoding='utf-8').read())
        for missing in ('staging', 'hilog_tag', 'plan_status', 'runner_py_sha256', 'review', 'operator'):
            mutated = {k: v for k, v in freeze.items() if k != missing}
            path = os.path.join(sandbox, 'missing-%s.json' % missing)
            write_text(path, json.dumps(mutated))
            assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p)), missing
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-extra-key')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-extra-')
    try:
        freeze_path = build_freeze(sandbox)
        freeze = json.loads(open(freeze_path, encoding='utf-8').read())
        for extra_key, extra_value in (('surprise', 1), ('spacing', 3), ('legacy', True)):
            mutated = dict(freeze)
            mutated[extra_key] = extra_value
            path = os.path.join(sandbox, 'extra-%s.json' % extra_key)
            write_text(path, json.dumps(mutated))
            assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p)), extra_key
        # nested extra keys are rejected too
        mutated = dict(freeze)
        mutated['hdc'] = dict(freeze['hdc'])
        mutated['hdc']['serial'] = 'X'
        path = os.path.join(sandbox, 'extra-nested.json')
        write_text(path, json.dumps(mutated))
        assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p))
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-ready-without-pass-confirmation')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-ready-')
    try:
        for confirmation, review in (('pending', 'pending'), ('pending', 'pass'),
                                     ('pass', 'pending')):
            # ('pending','pass') is rejected by the pending-machine rule itself;
            # ('pass','pending') by the ready double-binding rule.
            sub = os.path.join(sandbox, '%s-%s' % (confirmation, review))
            freeze_path = build_freeze(sub, plan_status='ready',
                                       confirmation=confirmation, review=review)
            assert expect_raises(RuntimeError, lambda p=freeze_path: load_freeze_from_file(p)), \
                (confirmation, review)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-review-pass-over-pending-machine-confirmation')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-machinepending-')
    try:
        freeze_path = build_freeze(sandbox, plan_status='blocked',
                                   confirmation='pending', review='pass')
        assert expect_raises(RuntimeError, lambda: load_freeze_from_file(freeze_path))
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-runner-py-hash-mismatch')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-runnersha-')
    try:
        for wrong in ('f' * 64, sha256_file(__file__), 'abc', 'A' * 64):
            freeze_path = build_freeze(sandbox, runner_sha=wrong)
            assert expect_raises(RuntimeError, lambda p=freeze_path: load_freeze_from_file(p)), wrong
        # The recomputed runner hash must equal this very runner file.
        freeze_path = build_freeze(sandbox, runner_sha=sha256_file(RUNNER_SRC))
        load_freeze_from_file(freeze_path)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-empty-or-wrong-parity-hashes')
def _():
    # The PowerShell parity hashes are REQUIRED and recomputed against the
    # sibling files: the empty-string escape is gone (review round-2
    # NEW-MINOR-1), and a drifted value is rejected.
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-paritysha-')
    try:
        spike_dir = os.path.dirname(RUNNER_SRC)
        real_ps1 = sha256_file(os.path.join(spike_dir, 'g0-phys-probe-campaign.ps1'))
        real_st_ps1 = sha256_file(os.path.join(os.path.dirname(os.path.abspath(__file__)), 'g0-runner-selftest.ps1'))
        for key, wrong in (('runner_ps1_sha256', ''), ('runner_ps1_sha256', 'e' * 64),
                           ('selftest_ps1_sha256', ''), ('selftest_py_sha256', 'b' * 64)):
            freeze_path = build_freeze(sandbox)
            with open(freeze_path, 'r', encoding='utf-8') as f:
                freeze = json.load(f)
            freeze[key] = wrong
            with open(freeze_path, 'w', encoding='utf-8') as f:
                json.dump(freeze, f)
            assert expect_raises(RuntimeError, lambda p=freeze_path: load_freeze_from_file(p)), (key, wrong)
        # The real sibling hashes are accepted.
        freeze_path = build_freeze(sandbox)
        with open(freeze_path, 'r', encoding='utf-8') as f:
            freeze = json.load(f)
        freeze['runner_ps1_sha256'] = real_ps1
        freeze['selftest_ps1_sha256'] = real_st_ps1
        with open(freeze_path, 'w', encoding='utf-8') as f:
            json.dump(freeze, f)
        load_freeze_from_file(freeze_path)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-declared-pass-record-hash-mismatch')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-recordhash-')
    try:
        freeze_path = build_freeze(sandbox, plan_status='ready',
                                   confirmation='pass', review='pass')
        freeze = json.loads(open(freeze_path, encoding='utf-8').read())
        for field in ('confirmation', 'review'):
            mutated = json.loads(json.dumps(freeze))
            mutated[field]['record_sha256'] = 'f' * 64
            path = os.path.join(sandbox, 'wronghash-%s.json' % field)
            write_text(path, json.dumps(mutated))
            assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p)), field
            # a missing record file is rejected as well
            mutated2 = json.loads(json.dumps(freeze))
            mutated2[field]['record_path'] = os.path.join(sandbox, 'does-not-exist.json')
            path2 = os.path.join(sandbox, 'nofile-%s.json' % field)
            write_text(path2, json.dumps(mutated2))
            assert expect_raises(RuntimeError, lambda p=path2: load_freeze_from_file(p)), field
        # the matching pair loads
        load_freeze_from_file(freeze_path)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-live-requires-ready')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-live-')
    try:
        blocked = build_freeze(os.path.join(sandbox, 'b'), plan_status='blocked')
        assert expect_raises(RuntimeError, lambda: load_freeze_from_file(blocked, mode='live'))
        ready = build_freeze(os.path.join(sandbox, 'r'), plan_status='ready',
                             confirmation='pass', review='pass')
        load_freeze_from_file(ready, mode='live')
        # DryRun accepts both blocked and ready.
        load_freeze_from_file(blocked, mode='dry-run')
        load_freeze_from_file(ready, mode='dry-run')
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('freeze-rejects-frozen-value-drift')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-freeze-drift-')
    try:
        freeze_path = build_freeze(sandbox)
        freeze = json.loads(open(freeze_path, encoding='utf-8').read())
        mutations = [
            ('bundle', 'cn.example.other'),
            ('ability', 'OtherAbility'),
            ('module', 'other'),
            ('staging', '/data/local/tmp/other'),
            ('hilog_tag', 'OtherTag'),
            ('scenario_window_seconds', 59),
            ('scenario_window_seconds', '60'),
            ('plan_status', 'nope'),
            ('attempt', 'retry-1'),
            ('code_sha', 'short'),
            ('authorization_id', 'AUTH-OTHER'),
            ('campaign_id', 'OTHER-CAMPAIGN'),
            ('evidence_id', 'OTHER-EVIDENCE'),
            ('operator', 'someone else'),
        ]
        for key, value in mutations:
            mutated = json.loads(json.dumps(freeze))
            mutated[key] = value
            path = os.path.join(sandbox, 'drift-%s-%s.json' % (key, re.sub(r'[^A-Za-z0-9]', '_', str(value))))
            write_text(path, json.dumps(mutated))
            assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p)), (key, value)
        nested = [
            ('target_tuple', 'device_model', 'PIXEL-9'),
            ('target_tuple', 'full_system_build', 'OTHER BUILD'),
            ('target_tuple', 'kernel_architecture', 'x86_64'),
            ('elf_profile', 'pt_tls', False),
            ('elf_profile', 'tprel64_count', 2),
            ('elf_profile', 'static_tls_flag', True),
        ]
        for section, key, value in nested:
            mutated = json.loads(json.dumps(freeze))
            mutated[section][key] = value
            path = os.path.join(sandbox, 'drift-%s-%s.json' % (section, key))
            write_text(path, json.dumps(mutated))
            assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p)), (section, key)
        # needed list drift
        mutated = json.loads(json.dumps(freeze))
        mutated['elf_profile']['needed'] = ['libc.so', 'libdl.so']
        path = os.path.join(sandbox, 'drift-needed.json')
        write_text(path, json.dumps(mutated))
        assert expect_raises(RuntimeError, lambda p=path: load_freeze_from_file(p))
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


# ---- 5. TargetBindingConfirm (subprocess + fake hdc) ------------------------------


@test('tbc-pass-writes-single-use-double-file-record')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-tbc-pass-')
    try:
        freeze_path = build_freeze(sandbox)
        hdc_path = install_tbc_fake_hdc(sandbox, TBC_FAKE_HDC_SCRIPT)
        retarget_freeze_hdc(freeze_path, hdc_path)
        record_path = os.path.join(sandbox, 'confirmation', 'record.json')
        os.makedirs(os.path.dirname(record_path))
        proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path,
                           '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET_TOKEN})
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert 'RUNNER_RESULT=pass' in proc.stdout and 'COMMAND_ATTEMPTED=3' in proc.stdout \
            and 'COMMAND_COMPLETED=3' in proc.stdout and 'IS_EVIDENCE=false' in proc.stdout, proc.stdout
        assert os.path.isfile(record_path) and os.path.isfile(record_path + '.sha256')
        assert open(record_path + '.sha256', encoding='utf-8').read().strip() == sha256_file(record_path)
        record = json.loads(open(record_path, encoding='utf-8-sig').read())
        assert record['record_kind'] == 'g0-target-binding-confirmation'
        assert record['is_evidence'] is False and record['verdict'] == 'pass'
        assert record['command_attempted'] == 3 and record['command_completed'] == 3
        assert record['target_redacted'] is True
        assert record['expected_model'] == record['observed_model'] == MODEL
        assert record['expected_build'] == record['observed_build'] == BUILD
        assert record['created_at'].endswith('+08:00')
        assert record['authorization_id'] == AUTH_ID and record['campaign_id'] == CAMPAIGN_ID \
            and record['evidence_id'] == EVIDENCE_ID
        assert record['code_sha'] == 'a' * 40
        assert record['runner_py_sha256'] == sha256_file(RUNNER_SRC)
        # the real target never enters the record
        assert TARGET_TOKEN not in open(record_path, encoding='utf-8').read()
        # single-use: rerun hits the pre-record gate -> exit 1, record untouched
        sha_before = sha256_file(record_path)
        proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path,
                           '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET_TOKEN})
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        assert sha256_file(record_path) == sha_before
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('tbc-tuple-drift-exits-2-with-blocked-record')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-tbc-drift-')
    try:
        freeze_path = build_freeze(sandbox)
        hdc_path = install_tbc_fake_hdc(sandbox, TBC_FAKE_HDC_DRIFT_SCRIPT)
        retarget_freeze_hdc(freeze_path, hdc_path)
        record_path = os.path.join(sandbox, 'record.json')
        proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path,
                           '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET_TOKEN})
        assert proc.returncode == 2, (proc.returncode, proc.stdout, proc.stderr)
        assert 'RUNNER_RESULT=blocked' in proc.stdout, proc.stdout
        record = json.loads(open(record_path, encoding='utf-8-sig').read())
        assert record['verdict'] == 'blocked' and record['reason'] != 'N/A'
        assert 'model' in record['reason'].lower()
        assert open(record_path + '.sha256', encoding='utf-8').read().strip() == sha256_file(record_path)
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('tbc-invalid-target-token-exits-2-with-blocked-record')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-tbc-token-')
    try:
        freeze_path = build_freeze(sandbox)
        hdc_path = install_tbc_fake_hdc(sandbox, TBC_FAKE_HDC_SCRIPT)
        retarget_freeze_hdc(freeze_path, hdc_path)
        for bad in ('-leading-dash', 'has space', 'PHYS-1', '<PHYS_1_TARGET>', ''):
            record_path = os.path.join(sandbox, 'record-%s.json' % re.sub(r'[^a-z0-9]+', '-', bad.lower()))
            env = {} if bad == '' else {'PHYS_1_TARGET': bad}
            proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path,
                               '--ConfirmationRecord', record_path], env=env)
            assert proc.returncode == 2, (bad, proc.returncode, proc.stdout, proc.stderr)
            record = json.loads(open(record_path, encoding='utf-8-sig').read())
            assert record['verdict'] == 'blocked' and record['command_attempted'] == 0
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


# ---- 6./7. full DryRun ------------------------------------------------------------


def verify_transcript_chain_independently(transcript_path):
    """Local recompute of line_sha256 = sha256(prev + canonical json of the
    line without line_sha256); returns (violations, chain_head)."""
    violations = []
    prev = '0' * 64
    expected_seq = 1
    head = prev
    for raw in open(transcript_path, 'r', encoding='utf-8-sig'):
        line = raw.rstrip('\n')
        if not line.strip():
            continue
        doc = json.loads(line)
        assert doc['prev_line_sha256'] == prev, 'transcript prev-hash break at seq %s' % doc.get('seq')
        assert doc['seq'] == expected_seq, 'transcript seq order break'
        core = {'seq': doc['seq'], 'ts': doc['ts'], 'event': doc['event'],
                'details': doc['details'], 'prev_line_sha256': doc['prev_line_sha256']}
        canonical = json.dumps(core, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
        recomputed = hashlib.sha256((doc['prev_line_sha256'] + canonical).encode('utf-8')).hexdigest()
        if recomputed != doc['line_sha256']:
            violations.append(doc['seq'])
        prev = doc['line_sha256']
        head = prev
        expected_seq += 1
    return violations, head


def assert_no_target_token_anywhere(paths):
    for base in paths:
        for root, _dirs, names in os.walk(base):
            for name in names:
                with open(os.path.join(root, name), 'r', encoding='utf-8', errors='replace') as f:
                    body = f.read()
                assert TARGET_TOKEN not in body, 'target token leaked into %s' % os.path.join(root, name)


@test('dryrun-pass-end-to-end')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-pass-')
    try:
        freeze_path = build_freeze(sandbox)
        evidence = os.path.join(sandbox, 'evidence-dry')
        raw = os.path.join(sandbox, 'raw-dry')
        proc = run_runner(['--DryRun', '--Freeze', freeze_path],
                          env={'G0_DRYRUN_SCRIPT': 'pass'})
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert proc.stdout.strip().splitlines()[-1] == 'VERDICT=pass', proc.stdout
        assert 'hdc_process_starts=17' in proc.stdout, proc.stdout
        # evidence layout
        results_path = os.path.join(evidence, 'scenario-results.json')
        manifest_path = os.path.join(evidence, 'hash-manifest.json')
        seal_path = os.path.join(evidence, 'campaign-seal.json')
        transcript_path = os.path.join(evidence, 'transcript.redacted.jsonl')
        for path in (results_path, manifest_path, seal_path, transcript_path):
            assert os.path.isfile(path), path
        record = json.loads(open(results_path, encoding='utf-8-sig').read())
        assert record['verdict'] == 'pass' and record['fail_reason'] is None
        assert record['is_evidence'] is False and record['execution_mode'] == 'dry-run'
        assert record['non_evidence_reason'] != 'N/A'
        # the evidence record now carries BOTH runner hashes (NEW-MINOR-1)
        spike_dir = os.path.dirname(RUNNER_SRC)
        assert record['runner_py_sha256'] == sha256_file(RUNNER_SRC)
        assert record['runner_ps1_sha256'] == sha256_file(
            os.path.join(spike_dir, 'g0-phys-probe-campaign.ps1'))
        assert record['markers']['count'] == 1
        assert record['markers']['fields']['hello'] == '42'
        assert record['markers']['fields']['runtimeBytes'] == '1048576'
        # array-valued evidence fields stay arrays through serialization
        # (review MAJOR-1 parity contract with the PowerShell runner)
        assert isinstance(record['markers']['raw_lines'], list), type(record['markers']['raw_lines'])
        assert len(record['markers']['raw_lines']) == 1
        assert isinstance(record['integrity_violations'], list)
        assert record['markers']['window_seconds'] == 60
        assert record['hdc_execution']['command_attempted'] == 17
        assert record['hdc_execution']['command_completed'] == 17
        assert record['hdc_execution']['logical_calls'] == 17
        assert record['hdc_execution']['operations']['HilogStream'] == 1
        assert record['cleanup']['status'] == 'verified-clean'
        assert record['absent_probes'] == {'bundle_dump': 'absent', 'pidof': 'absent', 'staging': 'absent'}
        assert record['integrity_violations'] == []
        # the pass marker declares loaderError= (empty) - preserved verbatim
        assert record['loader_error'] in (None, '')
        assert record['loader_errno'] in (None, '0')
        assert record['fault_probe']['status'] == 'no-fault-lines'
        # seal binds record + manifest bytes
        seal = json.loads(open(seal_path, encoding='utf-8-sig').read())
        assert seal['record']['path'] == 'scenario-results.json'
        assert seal['record']['sha256'] == sha256_file(results_path)
        assert seal['manifest']['path'] == 'hash-manifest.json'
        assert seal['manifest']['sha256'] == sha256_file(manifest_path)
        assert seal['run_status'] == 'completed' and seal['final_exit_code'] == 0
        assert seal['fail_reason'] is None and seal['verdict'] == 'pass'
        assert seal['transcript_chain_head'] == record['transcript_reference']['chain_head']
        # manifest verifies every produced file (evidence + raw)
        manifest = json.loads(open(manifest_path, encoding='utf-8-sig').read())
        assert manifest['algorithm'] == 'SHA-256'
        assert any(entry['path'] == 'transcript.redacted.jsonl' for entry in manifest['files'])
        for entry in manifest['files']:
            assert sha256_file(os.path.join(evidence, entry['path'])) == entry['sha256'], entry['path']
        for entry in manifest['external_raw_files']:
            assert sha256_file(os.path.join(raw, entry['path'])) == entry['sha256'], entry['path']
        # transcript chain: runner verifier + independent recompute agree
        assert runner.test_transcript_integrity(transcript_path) == []
        violations, head = verify_transcript_chain_independently(transcript_path)
        assert violations == [] and head == seal['transcript_chain_head']
        # raw artifacts: hilog kept the marker, per-command stdout/stderr exist
        hilog_raw = open(os.path.join(raw, 'hilog-raw.txt'), encoding='utf-8').read()
        assert 'G0_RESULT' in hilog_raw and HILOG_TAG in hilog_raw
        assert os.path.isfile(os.path.join(raw, '09-startentry.stdout.txt'))
        assert os.path.isfile(os.path.join(raw, '17-stagingprobe.stderr.txt'))
        # transcript/audit argv keep placeholders only
        transcript_text = open(transcript_path, encoding='utf-8-sig').read()
        assert runner.TARGET_PLACEHOLDER in transcript_text
        assert runner.HAP_PLACEHOLDER in transcript_text
        # redaction: the real target token appears in NO evidence/raw output
        assert_no_target_token_anywhere([evidence, raw])
        # the frozen (sentinel) hdc was never executed by --DryRun
        assert not os.path.exists(os.path.join(sandbox, 'SENTINEL-EXECUTED'))
        # host process table: the fake never appears as comm 'hdc'
        assert runner.count_hdc_processes() == 0
        assert record['host_hdc_processes_after'] == 0
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('dryrun-dlopen-rejected-end-to-end')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-dlopen-')
    try:
        freeze_path = build_freeze(sandbox)
        proc = run_runner(['--DryRun', '--Freeze', freeze_path],
                          env={'G0_DRYRUN_SCRIPT': 'dlopen-rejected'})
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert proc.stdout.strip().splitlines()[-1] == 'VERDICT=blocked', proc.stdout
        results_path = os.path.join(sandbox, 'evidence-dry', 'scenario-results.json')
        record = json.loads(open(results_path, encoding='utf-8-sig').read())
        assert record['verdict'] == 'blocked' and record['fail_reason'] == 'dlopen-blocked'
        assert record['is_evidence'] is False
        assert record['loader_error'] == DLOPEN_ERROR, record['loader_error']
        assert record['loader_errno'] == '2'
        assert record['markers']['count'] == 1
        assert record['markers']['fields']['stage'] == 'dlopen'
        assert record['markers']['fields']['verdict'] == 'FAIL'
        assert record['cleanup']['status'] == 'verified-clean'
        assert record['integrity_violations'] == []
        seal = json.loads(open(os.path.join(sandbox, 'evidence-dry', 'campaign-seal.json'),
                               encoding='utf-8-sig').read())
        assert seal['fail_reason'] == 'dlopen-blocked' and seal['verdict'] == 'blocked'
        assert seal['record']['sha256'] == sha256_file(results_path)
        # loader error also preserved verbatim in the raw hilog capture
        hilog_raw = open(os.path.join(sandbox, 'raw-dry', 'hilog-raw.txt'), encoding='utf-8').read()
        assert DLOPEN_ERROR in hilog_raw
        # dlopen-rejected yields a faultlogger line, recorded without changing the mapping
        assert record['fault_probe']['status'] == 'fault-lines-present'
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('dryrun-install-fails-end-to-end')
def _():
    # A CampaignBlocked ending must complete with a SEALED blocked record
    # (exit 0, scenario-results/hash-manifest/campaign-seal all present),
    # never crash before sealing (review BLOCKER-1 regression).
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-install-')
    try:
        freeze_path = build_freeze(sandbox)
        proc = run_runner(['--DryRun', '--Freeze', freeze_path],
                          env={'G0_DRYRUN_SCRIPT': 'install-fails'})
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert proc.stdout.strip().splitlines()[-1] == 'VERDICT=blocked', proc.stdout
        evidence = os.path.join(sandbox, 'evidence-dry')
        results_path = os.path.join(evidence, 'scenario-results.json')
        seal_path = os.path.join(evidence, 'campaign-seal.json')
        manifest_path = os.path.join(evidence, 'hash-manifest.json')
        for path in (results_path, seal_path, manifest_path):
            assert os.path.isfile(path), path
        record = json.loads(open(results_path, encoding='utf-8-sig').read())
        assert record['verdict'] == 'blocked'
        assert record['fail_reason'] == 'installhap-success-marker-missing', record['fail_reason']
        assert record['is_evidence'] is False
        assert record['markers']['count'] == 0
        # the flow stopped before the entry started; wind-down still ran
        assert record['cleanup']['status'] == 'verified-clean'
        # FaultProbe never ran on this path: status must say so, not
        # 'no-fault-lines' (review MINOR-1)
        assert record['fault_probe']['status'] == 'not-run'
        assert record['fault_probe']['fault_lines'] is None
        assert record['integrity_violations'] == []
        seal = json.loads(open(seal_path, encoding='utf-8-sig').read())
        assert seal['verdict'] == 'blocked'
        assert seal['fail_reason'] == 'installhap-success-marker-missing'
        assert seal['run_status'] == 'completed' and seal['final_exit_code'] == 0
        assert seal['record']['sha256'] == sha256_file(results_path)
        # redaction still holds on the blocked path
        assert_no_target_token_anywhere([evidence, os.path.join(sandbox, 'raw-dry')])
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('dryrun-ready-freeze-double-binding-accepted')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-ready-')
    try:
        freeze_path = build_freeze(sandbox, plan_status='ready',
                                   confirmation='pass', review='pass')
        proc = run_runner(['--DryRun', '--Freeze', freeze_path],
                          env={'G0_DRYRUN_SCRIPT': 'pass'})
        assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
        assert proc.stdout.strip().splitlines()[-1] == 'VERDICT=pass'
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('dryrun-ready-freeze-with-broken-review-binding-exits-1')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-readybad-')
    try:
        freeze_path = build_freeze(sandbox, plan_status='ready',
                                   confirmation='pass', review='pass')
        freeze = json.loads(open(freeze_path, encoding='utf-8').read())
        freeze['review']['record_sha256'] = 'f' * 64
        broken_path = os.path.join(sandbox, 'freeze-broken.json')
        write_text(broken_path, json.dumps(freeze))
        proc = run_runner(['--DryRun', '--Freeze', broken_path],
                          env={'G0_DRYRUN_SCRIPT': 'pass'})
        assert proc.returncode == 1, (proc.returncode, proc.stdout, proc.stderr)
        assert not os.path.exists(os.path.join(sandbox, 'evidence-dry'))
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('dryrun-rejects-invalid-or-missing-script-env')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-dryrun-env-')
    try:
        freeze_path = build_freeze(sandbox)
        for script in (None, 'bogus', 'PASS', ''):
            env = {} if script is None else {'G0_DRYRUN_SCRIPT': script}
            proc = run_runner(['--DryRun', '--Freeze', freeze_path], env=env)
            assert proc.returncode == 1, (script, proc.returncode, proc.stdout, proc.stderr)
            assert not os.path.exists(os.path.join(sandbox, 'evidence-dry')), script
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


# ---- CLI gates ----------------------------------------------------------------


@test('cli-mode-mutual-exclusion-and-required-args')
def _():
    sandbox = tempfile.mkdtemp(prefix='g0-selftest-cli-')
    try:
        freeze_path = build_freeze(sandbox)
        proc = run_runner(['--DryRun', '--Live', '--Freeze', freeze_path])
        assert proc.returncode == 1, proc.returncode
        proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path])
        assert proc.returncode == 1, proc.returncode
        proc = run_runner(['--TargetBindingConfirm', '--Freeze', freeze_path,
                           '--ConfirmationRecord', os.path.join(sandbox, 'r.json'),
                           '--DryRun'], env={'G0_DRYRUN_SCRIPT': 'pass'})
        assert proc.returncode == 1, proc.returncode
        proc = run_runner(['--DryRun'])
        assert proc.returncode == 1, proc.returncode
        proc = run_runner(['--ConfirmationRecord', os.path.join(sandbox, 'r.json'),
                           '--Freeze', freeze_path])
        assert proc.returncode == 1, proc.returncode
        proc = run_runner(['--DryRun', '--Freeze', os.path.join(sandbox, 'missing.json')],
                          env={'G0_DRYRUN_SCRIPT': 'pass'})
        assert proc.returncode == 1, proc.returncode
    finally:
        shutil.rmtree(sandbox, ignore_errors=True)


@test('cli-version-flag-exact-output')
def _():
    proc = run_runner(['--version'])
    assert proc.returncode == 0 and proc.stdout.strip() == 'g0-phys-probe-campaign.py 1.0.0', proc.stdout


@test('runner-embedded-selftest-passes')
def _():
    proc = run_runner(['--SelfTest'])
    assert proc.returncode == 0, (proc.returncode, proc.stdout, proc.stderr)
    assert 'SELFTEST_RESULT=pass HDC_PROCESSES=0' in proc.stdout, proc.stdout


# ---- host HDC process count probe -------------------------------------------------


@test('host-hdc-count-probe-first-column-only')
def _():
    # synthetic table: only the exact first-column 'hdc' counts
    assert runner.count_hdc_from_ps_output(
        'hdc -t foo\n'
        'fake-hdc -t bar\n'
        'python3 /tmp/fake-hdc\n'
        'hdcx wrapper\n'
        'sshd: /usr/sbin/sshd -D\n'
        'chrome /home/x --flag\n') == 1
    assert runner.count_hdc_from_ps_output('') == 0
    assert runner.count_hdc_from_ps_output('hdc\nhdc shell hilog') == 2
    # the fixed absolute host probe is used, first column compared only
    assert os.path.isfile('/usr/bin/ps')
    count = runner.count_hdc_processes()
    assert count == 0, 'expected no host process with comm hdc, got %r' % count


# =====================================================================
# Runner
# =====================================================================


def main():
    passed = 0
    failed = []
    for name, fn in TESTS:
        try:
            fn()
        except Exception as e:
            failed.append((name, e, traceback.format_exc()))
            print('FAIL %s: %s' % (name, e))
        else:
            passed += 1
            print('PASS %s' % name)
    print('---- summary ----')
    print('TOTAL=%d PASSED=%d FAILED=%d' % (len(TESTS), passed, len(failed)))
    for name, _e, trace in failed:
        print('FAILED-DETAIL %s\n%s' % (name, trace))
    print('SELFTEST_RESULT=%s' % ('fail' if failed else 'pass'))
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
