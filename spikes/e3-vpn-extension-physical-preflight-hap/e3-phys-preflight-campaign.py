#!/usr/bin/env python3
"""E3-PHYS-PREFLIGHT campaign runner - Python 3 port of
e3-phys-preflight-campaign.ps1 (5985 lines, 129 functions).

Port semantics
--------------
This single-file runner is the semantic equivalent of the PowerShell
campaign runner for the E3-PHYS-PREFLIGHT physical-device preflight
campaign. It is a port, not a rewrite: every function carries the PS
source line range it was ported from (e.g. "PS L1548-1558"), the
contract-preservation list C1-C20 and platform-difference map A1-A16 of
$HOME/migration/runner-port-design.md are the normative references, and
the message substrings asserted by the PS selftest are preserved verbatim
(R18).

Governance / AUTH binding
-------------------------
The runner is bound to AUTH-E3-PHYS1API26-20260816-0001 (ADJ-20260810-0001,
C6): one AUTH, one fixed candidate pair
(E3-PHYS-PREFLIGHT-20260816-0001 / EV-E3-PHYS1API26-20260816-0001), and
attempt=initial with retry N/A. TargetBindingConfirm (producer) and every
consumer of this AUTH's confirmation enforce the exact pair and the initial
attempt; any retry requires new governance and a new authorization and can
never consume this AUTH path. The runner_sha256 freeze binding is the
SHA-256 of THIS file's bytes (single-file runner = single hash, design
document section 1.1).

Phase status
------------
Design units U1-U4 and U8 are implemented (infrastructure, freeze contracts,
confirmation/review records, HDC layer, embedded pure-function selftest).
Units U5-U7, U9-U10 (capture state machine, layout/scenarios, records/seal,
main-flow orchestration) are explicit NotImplementedError placeholders to be
filled in later phases.

Safety invariants
-----------------
- Importing this module never executes any hdc device command. Only the
  live execution path of invoke_hdc_operation() starts processes, and only
  with list argv (shell=False), explicit UTF-8 decoding and timeouts.
- No signature-material private keys are read.
"""

import argparse
import base64
import hashlib
import ipaddress
import json
import math
import os
import re
import select
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

BUNDLE_A = 'cn.alfadb.netbird.e3physvpna'
BUNDLE_B = 'cn.alfadb.netbird.e3physvpnb'
ABILITY = 'EntryAbility'
MODULE = 'entry'
STAGING = '/data/local/tmp/e3-phys-preflight'
WINDOW_SECONDS = 60

# ADJ-20260810-0001 (C6): the current authorization fixes one AUTH, one
# candidate pair, and attempt=initial.
AUTH_ID = 'AUTH-E3-PHYS1API26-20260816-0001'
CANDIDATE_CAMPAIGN_ID = 'E3-PHYS-PREFLIGHT-20260816-0001'
CANDIDATE_EVIDENCE_ID = 'EV-E3-PHYS1API26-20260816-0001'

FROZEN_DEVICE_ZONE_MAP = {'CST': '+08:00'}
DEVICE_CLOCK_SKEW_TOLERANCE_SECONDS = 3.0
VIRTUAL_BASE = datetime(2099, 1, 1, tzinfo=timezone.utc)

# Mutable script-scope state (mirrors PS $script: variables). Set by
# main() from argparse; tests may set them directly.
freeze_manifest = None
evidence_root = None
raw_root = None
hap_a = None
hap_b = None
hdc_path = None
hdc_timeout_seconds = 20
operator_timeout_seconds = 300
dry_run = False
live_simulation = False
simulation_fixture = None
self_test = False
target_binding_confirm = False
confirmation_record = None
no_device_mode = False
execution_mode = 'live'

repo_root = None
evidence_path = None
raw_path = None
actual_target = None
public_version_literals = ['PLA-AL10 7.0.0.100(SP8C00E32R7P2)']
hdc_process_start_count = 0
hdc_logical_call_count = 0
infrastructure_reason_observed = None
simulation = None
virtual_seconds = 0.0
machine_fresh_confirmation = None
independent_review_record = None
prior_blocked_binding = None
freeze = None
campaign_capture = None
capture_degraded = []
raw_hilog_artifacts = []

# U6 campaign / simulation state (mirrors PS $script: variables, design
# document section 1.1 unit U6).
hdc_operation_counts = {}
installed_a = False
installed_b = False
staging_sent = False
staging_may_exist = False
campaign_started = False
partial_scenarios = []
observation_only_degraded = []
capture_artifacts = []
simulation_layout_first_attempt = {}
fault_artifacts = []
cleanup_actions = []
cleanup_verification = {'status': 'not-run', 'verified_absent': False, 'bundles': []}
campaign_phase = 'preflight'
simulation_installed_a = False
simulation_installed_b = False
simulation_staging_present = False
simulation_active_bundles = set()
simulation_scenario_steps_written = {}
probe_contexts = {}
current_window_end = None
operator_wait_history = []
operator_actions = []
operator_wait_current = None
scenario_invalid = None
verified_requests = {}
operator_action_guard_from = None
last_capture_infrastructure = False

# U7 record/seal state (mirrors PS $script: variables, design document
# section 1.1 unit U7).
transcript_index = 0
transcript_previous_hash = '0' * 64
projection_transcript = None

# =====================================================================
# Section 1: Base utilities (design unit U1)
# =====================================================================


def sha256_text(text):
    """PS L94-97 Get-TextSha256: lowercase hex SHA-256 of UTF-8 bytes."""
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def sha256_file(path):
    """PS L99-102 Get-FileSha256: lowercase hex SHA-256 of file bytes."""
    with open(path, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def normalize_path(path):
    """PS L104-107 Get-NormalizedPath: absolute path, trailing separators
    trimmed (matches [IO.Path]::GetFullPath + TrimEnd)."""
    return os.path.abspath(path).rstrip('/\\')


def is_under_path(candidate, parent):
    """PS L109-115 Test-IsUnderPath. PS uses OrdinalIgnoreCase (Windows);
    on Linux os.path.normcase is a no-op so the check is case-sensitive
    (POSIX semantics, design document A15)."""
    candidate_path = os.path.normcase(normalize_path(candidate) + os.sep)
    parent_path = os.path.normcase(normalize_path(parent) + os.sep)
    return candidate_path.startswith(parent_path)


def get_optional_property(obj, name, default=None):
    """PS L117-126 Get-OptionalProperty. Python dicts only (JSON-parsed
    objects); PS's pscustomobject branch has no Python equivalent."""
    if obj is None:
        return default
    if isinstance(obj, dict):
        if name in obj:
            return obj[name]
        return default
    return default


def get_required_property(obj, name):
    """PS L139-142 Get-RequiredProperty."""
    if not isinstance(obj, dict) or name not in obj:
        raise RuntimeError('freeze manifest missing property: %s' % name)
    return obj[name]


def assert_json_boolean(obj, name, expected):
    """PS L144-148 Assert-JsonBoolean."""
    value = get_required_property(obj, name)
    if not isinstance(value, bool) or value != expected:
        raise RuntimeError('%s must be the JSON Boolean %s' % (name, str(expected).lower()))


def coerce_json_integer(value):
    """MAJOR-1 PS [int] cast equivalence: accepts int and integral-valued
    float (2.0 -> 2); rejects non-integral floats (2.9), strings, bools and
    null. Losslessness is judged with float.is_integer() (exact, no
    rounding/truncation)."""
    if value is None or isinstance(value, bool):
        return None
    if isinstance(value, int):
        return value
    if isinstance(value, float) and math.isfinite(value) and value.is_integer():
        return int(value)
    return None


def coerce_json_double(value):
    """MAJOR-1 PS [double] cast equivalence: accepts int and float
    (3 -> 3.0); rejects strings, bools and non-finite values."""
    if value is None or isinstance(value, bool):
        return None
    if isinstance(value, (int, float)):
        result = float(value)
        if math.isfinite(result):
            return result
    return None


def test_json_integer(value):
    """PS L150-158 Test-JsonInteger. ADJ-20260810-0001 (C6) + MAJOR-1: JSON
    integer gate - accepts int and integral-valued float (2.0 -> 2, judged
    lossless via float.is_integer()); rejects non-integral floats (2.9),
    strings, bools and null (bool is a subclass of int in Python and must be
    rejected)."""
    return coerce_json_integer(value) is not None


def test_sha256_hex(value):
    """PS L1206-1210 Test-Sha256Hex."""
    return bool(value) and bool(re.match(r'^[0-9a-f]{64}$', str(value)))


def assert_file_hash(label, path, expected):
    """PS L444-450 Assert-FileHash."""
    if not os.path.isfile(path):
        raise RuntimeError('%s file missing' % label)
    if not re.match(r'^[0-9a-f]{64}$', str(expected)):
        raise RuntimeError('%s expected SHA-256 is not final' % label)
    if sha256_file(path) != expected:
        raise RuntimeError('%s SHA-256 mismatch' % label)


def _stj_escape_string(s):
    """System.Text.Json-compatible string escaping (R1): shortcut escapes
    for \\b/\\t/\\n/\\f/\\r, \\uXXXX lowercase hex for the remaining
    U+0000-U+001F control chars, \\u2028/\\u2029 for the line/paragraph
    separators, non-ASCII otherwise untouched, quotes/backslashes escaped."""
    out = ['"']
    for ch in s:
        o = ord(ch)
        if ch == '"':
            out.append('\\"')
        elif ch == '\\':
            out.append('\\\\')
        elif ch == '\b':
            out.append('\\b')
        elif ch == '\t':
            out.append('\\t')
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\f':
            out.append('\\f')
        elif ch == '\r':
            out.append('\\r')
        elif o < 0x20 or o in (0x2028, 0x2029):
            out.append('\\u%04x' % o)
        else:
            out.append(ch)
    out.append('"')
    return ''.join(out)


def _stj_float_repr(o):
    """System.Text.Json-compatible double formatting (R6): shortest
    round-trip digits (Python repr) re-rendered with the probed PS 7.6.4
    ConvertTo-Json notation rules - fixed notation when the decimal
    exponent E is in [-4, 16] (integral values get a trailing '.0'),
    scientific 'd.dddE±XX' otherwise with a minimum two-digit exponent
    (1E-07, 1E+17, 10000000000000000.0). Non-finite values (NaN/Infinity)
    raise ValueError - the JSON contract never emits them (MAJOR-1)."""
    if not math.isfinite(o):
        raise ValueError('Out of range float values are not JSON compliant: ' + repr(o))
    if o == 0.0:
        return '-0.0' if math.copysign(1.0, o) < 0 else '0.0'
    r = repr(o)
    neg = r.startswith('-')
    if neg:
        r = r[1:]
    if 'e' in r or 'E' in r:
        mantissa, _, exp = r.partition('e') if 'e' in r else r.partition('E')
        digits = mantissa.replace('.', '')
        e = int(exp)
    else:
        intpart, _, frac = r.partition('.')
        digits = (intpart + frac).lstrip('0') or '0'
        e = len(digits) - 1 - len(frac)
    if -4 <= e <= 16:
        if e >= 0:
            if e >= len(digits) - 1:
                body = digits + '0' * (e - len(digits) + 1) + '.0'
            else:
                body = digits[:e + 1] + '.' + digits[e + 1:]
        else:
            body = '0.' + '0' * (-e - 1) + digits
    else:
        mantissa = digits[0] + ('.' + digits[1:] if len(digits) > 1 else '')
        body = mantissa + 'E' + ('-' if e < 0 else '+') + '%02d' % abs(e)
    return ('-' if neg else '') + body


def _jsoncompat_serialize(value, out, indent, level, sort_keys):
    """Recursive PS ConvertTo-Json encoder core (MAJOR-1): accepts only the
    JSON contract types dict/list/str/int/float/bool/None; dict keys must be
    str; non-finite floats raise ValueError; any other type raises TypeError.
    Compact when indent is None, else PS-style pretty with two-space levels
    and ': ' separators."""
    if value is None:
        out.append('null')
    elif value is True:
        out.append('true')
    elif value is False:
        out.append('false')
    elif isinstance(value, str):
        out.append(_stj_escape_string(value))
    elif isinstance(value, int):
        out.append(str(value))
    elif isinstance(value, float):
        out.append(_stj_float_repr(value))
    elif isinstance(value, dict):
        if not all(isinstance(k, str) for k in value):
            raise TypeError('jsoncompat_dumps: dict keys must be str')
        items = sorted(value.items()) if sort_keys else list(value.items())
        if not items:
            out.append('{}')
            return
        if indent is None:
            out.append('{')
            for i, (key, item) in enumerate(items):
                if i:
                    out.append(',')
                out.append(_stj_escape_string(key))
                out.append(':')
                _jsoncompat_serialize(item, out, indent, level, sort_keys)
            out.append('}')
        else:
            pad = '  ' * (level + 1)
            out.append('{\n')
            for i, (key, item) in enumerate(items):
                if i:
                    out.append(',\n')
                out.append(pad)
                out.append(_stj_escape_string(key))
                out.append(': ')
                _jsoncompat_serialize(item, out, indent, level + 1, sort_keys)
            out.append('\n')
            out.append('  ' * level)
            out.append('}')
    elif isinstance(value, list):
        if not value:
            out.append('[]')
            return
        if indent is None:
            out.append('[')
            for i, item in enumerate(value):
                if i:
                    out.append(',')
                _jsoncompat_serialize(item, out, indent, level, sort_keys)
            out.append(']')
        else:
            pad = '  ' * (level + 1)
            out.append('[\n')
            for i, item in enumerate(value):
                if i:
                    out.append(',\n')
                out.append(pad)
                _jsoncompat_serialize(item, out, indent, level + 1, sort_keys)
            out.append('\n')
            out.append('  ' * level)
            out.append(']')
    else:
        raise TypeError('jsoncompat_dumps: unsupported type %s' % type(value).__name__)


def jsoncompat_dumps(obj, *, indent=None, sort_keys=False):
    """PS ConvertTo-Json compatible serializer (R1/A12), implemented as an
    independent recursive encoder - no CPython private APIs (MAJOR-1).
    Compact by default (matches -Compress); indent=2 matches the default
    (non-compress) form. Key order = insertion order unless sort_keys=True.
    Only dict/list/str/int/float/bool/None contract types are accepted;
    dict keys must be str; non-finite floats raise ValueError."""
    out = []
    _jsoncompat_serialize(obj, out, indent, 0, sort_keys)
    return ''.join(out)


def _golden_sample_obj(name):
    """Map a jsoncompat-golden.txt sample name to the equivalent Python
    object (mirrors $HOME/migration/compare-golden.py build_python_obj).
    Returns (obj, comparable); comparable=False marks samples that are not
    reproducible on the Python side (PS unordered hashtable key order)."""
    if name == 'keyorder-hashtable':
        return None, False
    if name.startswith('float-'):
        vals = [1e-07, 1e-06, 1e-05, 1e15, 1e16, 1e17, 1e20, -0.0, 1.5, 2.0,
                3.14159265358979, 1e21, 5e-324, 1.7976931348623157e308]
        return vals[int(name[6:])], True
    if name.startswith('p-'):
        return float(name[2:]), True
    if name.startswith('ctrl-'):
        return chr(int(name[5:], 16)), True
    table = {
        'esc-nl': 'a\nb', 'esc-tab': 'a\tb', 'esc-cr': 'a\rb',
        'esc-bs': 'a\bb', 'esc-ff': 'a\fb',
        'u2028': '\u2028', 'u2029': '\u2029',
        'chinese': '中文测试', 'emoji': '😀', 'astral': '𝄞',
        'nested': {'a': {'b': [1, {'c': 'x'}]}},
        'keyorder-ordered': {'z': 1, 'a': 2, 'm': 3},
        'int-0': 0, 'int-neg1': -1, 'int-2p53': 2 ** 53,
        'int-2p53p1': 2 ** 53 + 1, 'int-2p63': 2 ** 63, 'int-2p64': 2 ** 64,
        'bool-true': True, 'bool-false': False, 'null': None,
        'empty-obj': {}, 'empty-arr': [], 'empty-str': '',
        'key-space': {'a b': 1}, 'key-unicode': {'键': 1},
        'str-bs-quote': 'a\\b"c',
    }
    if name in table:
        return table[name], True
    raise KeyError('unknown golden sample name: %s' % name)


def _jsoncompat_static_checks(check, raises):
    """In-code static expectations for the PS-compatible serializer (R1),
    used when the out-of-repository golden capture is unavailable. Updated
    to the probed System.Text.Json behavior: shortcut escapes
    \\b/\\t/\\n/\\f/\\r, \\uXXXX lowercase hex for the remaining
    U+0000-U+001F, \\u2028/\\u2029 escaped, fixed float notation for
    decimal exponent in [-4, 16] with a trailing '.0' on integral values."""
    check('control-char-lowercase-hex', jsoncompat_dumps({'k': '\x01'}) == '{"k":"\\u0001"}')
    check('shortcut-newline', jsoncompat_dumps({'k': 'a\nb'}) == '{"k":"a\\nb"}')
    check('shortcut-tab', jsoncompat_dumps({'k': 'a\tb'}) == '{"k":"a\\tb"}')
    check('shortcut-cr', jsoncompat_dumps({'k': 'a\rb'}) == '{"k":"a\\rb"}')
    check('shortcut-backspace', jsoncompat_dumps({'k': 'a\bb'}) == '{"k":"a\\bb"}')
    check('shortcut-formfeed', jsoncompat_dumps({'k': 'a\fb'}) == '{"k":"a\\fb"}')
    check('quote-escaped', jsoncompat_dumps({'k': 'a"b'}) == '{"k":"a\\"b"}')
    check('backslash-escaped', jsoncompat_dumps({'k': 'a\\b'}) == '{"k":"a\\\\b"}')
    check('backslash-n-literal-preserved', jsoncompat_dumps({'k': 'a\\nb'}) == '{"k":"a\\\\nb"}')
    check('non-ascii-not-escaped', jsoncompat_dumps({'k': '中文é'}) == '{"k":"中文é"}')
    check('insertion-order-preserved', jsoncompat_dumps({'b': 1, 'a': 2}) == '{"b":1,"a":2}')
    check('sort-keys', jsoncompat_dumps({'b': 1, 'a': 2}, sort_keys=True) == '{"a":2,"b":1}')
    check('int', jsoncompat_dumps({'i': 2}) == '{"i":2}')
    check('float-3.0', jsoncompat_dumps({'f': 3.0}) == '{"f":3.0}')
    check('float-scientific-upper-E', jsoncompat_dumps({'f': 1e-7}) == '{"f":1E-07}')
    check('float-scientific-positive-exp', jsoncompat_dumps({'f': 1e21}) == '{"f":1E+21}')
    check('bool-null', jsoncompat_dumps({'b': True, 'n': None}) == '{"b":true,"n":null}')
    check('array', jsoncompat_dumps([1, 2]) == '[1,2]')
    check('nested', jsoncompat_dumps({'a': {'b': [1, {'c': 'x'}]}}) == '{"a":{"b":[1,{"c":"x"}]}}')
    check('indent-2', jsoncompat_dumps({'a': 1}, indent=2) == '{\n  "a": 1\n}')
    check('control-char-full-set',
          jsoncompat_dumps({'k': ''.join(chr(i) for i in range(0x20))})
          == '{"k":"%s"}' % ''.join(
              {8: '\\b', 9: '\\t', 10: '\\n', 12: '\\f', 13: '\\r'}.get(i, '\\u%04x' % i)
              for i in range(0x20)))
    check('unicode-astral-preserved', jsoncompat_dumps({'k': '😀'}) == '{"k":"😀"}')
    check('unicode-line-separators-escaped',
          jsoncompat_dumps({'k': '\u2028\u2029'}) == '{"k":"\\u2028\\u2029"}')
    check('nested-insertion-order',
          jsoncompat_dumps({'z': 1, 'a': [3, {'m': 2, 'n': [{'p': 1}]}]})
          == '{"z":1,"a":[3,{"m":2,"n":[{"p":1}]}]}')
    check('float-neg-zero', jsoncompat_dumps({'f': -0.0}) == '{"f":-0.0}')
    check('float-exponent-boundary-low',
          jsoncompat_dumps({'f': 1e-4}) == '{"f":0.0001}'
          and jsoncompat_dumps({'f': 1e-5}) == '{"f":1E-05}')
    check('float-exponent-boundary-high',
          jsoncompat_dumps({'f': 1e15}) == '{"f":1000000000000000.0}'
          and jsoncompat_dumps({'f': 1e16}) == '{"f":10000000000000000.0}')
    check('float-1e-07', jsoncompat_dumps({'f': 1e-7}) == '{"f":1E-07}')
    check('big-int', jsoncompat_dumps({'i': 10**40}) == '{"i":%s}' % str(10**40))
    check('nan-rejected', raises(ValueError, lambda: jsoncompat_dumps({'f': float('nan')})))
    check('infinity-rejected', raises(ValueError, lambda: jsoncompat_dumps({'f': float('inf')})))
    check('neg-infinity-rejected', raises(ValueError, lambda: jsoncompat_dumps({'f': float('-inf')})))
    check('non-str-key-rejected', raises(TypeError, lambda: jsoncompat_dumps({1: 'x'})))
    check('unsupported-type-rejected', raises(TypeError, lambda: jsoncompat_dumps({'k': b'x'})))


def jsoncompat_golden_selftest():
    """Golden self-test for the PS-compatible serializer (R1). Returns a list
    of (name, ok) tuples; the embedded selftest (U8) and the external test
    harness both consume it. When the out-of-repository PS 7.6.4
    ConvertTo-Json capture at $HOME/migration/jsoncompat-golden.txt is
    present, every sample is asserted byte-identical; otherwise a WARN is
    printed and the in-code static expectations (see
    _jsoncompat_static_checks) are used instead."""
    checks = []

    def check(name, cond):
        checks.append((name, bool(cond)))

    def raises(exc_type, fn):
        try:
            fn()
            return False
        except exc_type:
            return True
        except Exception:
            return False

    golden = os.path.join(os.path.expanduser('~'), 'migration', 'jsoncompat-golden.txt')
    if os.path.isfile(golden):
        try:
            with open(golden, 'r', encoding='utf-8') as f:
                lines = f.read().splitlines()
            for line in lines[1:]:
                if not line.strip():
                    continue
                name, b64 = line.split('\t', 1)
                obj, comparable = _golden_sample_obj(name)
                if not comparable:
                    continue
                expected = base64.b64decode(b64).decode('utf-8')
                check('golden-' + name, jsoncompat_dumps(obj) == expected)
        except Exception as exc:  # noqa: BLE001
            print('WARN: jsoncompat golden read failed (%s); falling back to static expectations' % exc)
            _jsoncompat_static_checks(check, raises)
    else:
        print('WARN: %s missing; falling back to static expectations' % golden)
        _jsoncompat_static_checks(check, raises)
    return checks


def write_text_utf8_no_bom(path, text):
    """A1/R3: UTF-8 without BOM (Python default), newline='' prevents
    translation. PS: [Text.UTF8Encoding]::new($false)."""
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def read_text_utf8_sig(path):
    """R3: read with BOM detection (utf-8-sig strips a leading BOM if
    present and is harmless for BOM-less files). PS Get-Content auto-detects."""
    with open(path, 'r', encoding='utf-8-sig') as f:
        return f.read()


def atomic_write_text(path, text):
    """A4: tempfile in the same directory + no-replace commit via
    os.link(tmp, path) + os.unlink(tmp) (same-filesystem atomic, never
    overwrites an existing destination - MAJOR-3). PS: tmp +
    [IO.File]::Move with CreateNew semantics; a pre-existing destination
    raises FileExistsError."""
    directory = os.path.dirname(os.path.abspath(path)) or '.'
    fd, tmp_path = tempfile.mkstemp(dir=directory, prefix='.e3-tmp-')
    try:
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
            f.write(text)
            f.flush()
            os.fsync(f.fileno())
        os.link(tmp_path, path)
        os.unlink(tmp_path)
    except BaseException:
        try:
            os.unlink(tmp_path)
        except OSError:
            pass
        raise


def acquire_campaign_lock(lock_path):
    """A5/R10: O_EXCL CreateNew semantics - never overwrites. PS
    Initialize-OutputRoots L403-442 (FileMode.CreateNew + FileShare.None;
    O_EXCL is the hard no-clobber requirement, flock is optional on Linux)."""
    if os.path.exists(lock_path):
        raise RuntimeError('campaign lock already exists; selective rerun is forbidden')
    assert_no_reparse_ancestor(lock_path)
    try:
        fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        raise RuntimeError('campaign lock already exists; selective rerun is forbidden')
    try:
        os.write(fd, b'E3-PHYS-PREFLIGHT\n')
        os.fsync(fd)
    finally:
        os.close(fd)


def assert_no_reparse_ancestor(path):
    """PS L373-392 Assert-NoReparseAncestor: walk up to the first existing
    ancestor, then reject any symlink (reparse point) in the chain. On Linux
    a symlink is the reparse-point equivalent (A6).
    TODO(Windows): a future Windows branch must also reject junction and
    reparse-point ancestors - os.path.islink() does not detect junctions, so
    it must additionally use os.path.isjunction() (Python 3.12+) or
    ctypes GetFileAttributesW() + FILE_ATTRIBUTE_REPARSE_POINT checks. The
    Linux port needs no junction logic: junctions do not exist on POSIX
    (MINOR-1)."""
    cursor = normalize_path(path)
    while not os.path.exists(cursor):
        parent = os.path.dirname(cursor)
        if not parent or parent == cursor:
            break
        cursor = parent
    while cursor:
        if os.path.islink(cursor):
            raise RuntimeError('output path has a junction or symlink ancestor: %s' % cursor)
        parent = os.path.dirname(cursor)
        if not parent or parent == cursor:
            break
        cursor = parent


def isoformat_7(dt):
    """A11/R4: DateTimeOffset.ToString('o') format - 7 fractional digits and
    a colon offset (+08:00). Python isoformat() gives 6 digits and %z gives
    +0800 without a colon, so both are normalized here. Microseconds are
    left-padded to 6 digits plus a trailing 0 (PS 'o' format)."""
    base = dt.strftime('%Y-%m-%dT%H:%M:%S')
    frac = dt.microsecond
    off = dt.strftime('%z')
    if len(off) == 5:
        off = off[:3] + ':' + off[3:]
    elif len(off) == 7:
        off = off[:3] + ':' + off[3:5] + ':' + off[5:]
    return '%s.%06d0%s' % (base, frac, off)


def now_iso():
    """PS Get-Now (non-simulation): DateTimeOffset.Now as 'o' string."""
    return isoformat_7(datetime.now().astimezone())


def parse_datetime(value):
    """PS L161-175 Convert-ToDateTimeOffset: strict timestamp coercion for
    governance records. Accepts ISO-8601 strings and datetime; returns an
    aware datetime or None when unparseable. Never round-trips through
    string formatting (which would lose subsecond precision)."""
    if isinstance(value, datetime):
        if value.tzinfo is None:
            return value.astimezone()
        return value
    if isinstance(value, str) and value.strip():
        s = value.strip()
        if s.endswith('Z') or s.endswith('z'):
            s = s[:-1] + '+00:00'
        try:
            return datetime.fromisoformat(s)
        except ValueError:
            pass
    return None


def get_git_repository_root():
    """PS L359-363 Get-GitRepositoryRoot: git -C <script dir> rev-parse
    --show-toplevel (A9)."""
    proc = subprocess.run(
        ['git', '-C', os.path.dirname(os.path.abspath(__file__)), 'rev-parse', '--show-toplevel'],
        capture_output=True, text=True, encoding='utf-8', errors='replace')
    root_text = proc.stdout.strip()
    if proc.returncode != 0 or not root_text:
        raise RuntimeError('unable to resolve repository root with git rev-parse')
    return normalize_path(root_text)


def get_repository_state(repo_root_path):
    """PS L365-371 Get-RepositoryState: HEAD + porcelain=v2 status;
    fingerprint = sha256(head + '\\n' + status)."""
    proc = subprocess.run(['git', '-C', repo_root_path, 'rev-parse', 'HEAD'],
                          capture_output=True, text=True, encoding='utf-8', errors='replace')
    head = proc.stdout.strip()
    if proc.returncode != 0 or not re.match(r'^[0-9a-f]{40}$', head):
        raise RuntimeError('unable to read repository HEAD')
    proc = subprocess.run(['git', '-C', repo_root_path, 'status', '--porcelain=v2', '--untracked-files=all'],
                          capture_output=True, text=True, encoding='utf-8', errors='replace')
    status = proc.stdout
    if proc.returncode != 0:
        raise RuntimeError('unable to read repository state')
    clean = not status.strip()
    return {'head': head, 'status': status, 'clean': clean,
            'fingerprint': sha256_text(head + '\n' + status)}


# =====================================================================
# Section 2: Sensitive information protection (design unit U1)
# =====================================================================

# PS L193-213 Protect-SensitiveText 12 rules (A16: (?i) -> re.IGNORECASE;
# all lookbehinds are fixed-width and supported by Python re).
_REDACTION_RULES = [
    (re.compile(r'"(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)"\s*:\s*"[^"]*"', re.IGNORECASE), r'"\1":"<REDACTED>"'),
    (re.compile(r'"(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)"\s*:\s*\[[^\]]*\]', re.IGNORECASE), r'"\1":["<REDACTED>"]'),
    (re.compile(r'\b(udid|serial|sn|token|password|passwd|private[_ -]?key|profile[_ -]?device[_ -]?id|deviceIds?)\s*[:=]\s*[^\s,;|"]+', re.IGNORECASE), r'\1=<REDACTED>'),
    (re.compile(r'\b(?:SN|UDID|SERIAL)[-_][A-Z0-9._-]{6,}\b', re.IGNORECASE), '<REDACTED_SERIAL>'),
    (re.compile(r'\b(?:tcp|udp|hdc)://[^\s|]+', re.IGNORECASE), '<REDACTED_ENDPOINT>'),
    (re.compile(r'\b(target|endpoint|host|address)\s*[:=]\s*[^\s,;|]+', re.IGNORECASE), r'\1=<REDACTED_ENDPOINT>'),
    (re.compile(r'\b(port)\s*[:=]\s*[0-9]{1,5}\b', re.IGNORECASE), r'\1=<REDACTED_PORT>'),
    (re.compile(r'\[[0-9a-f:]+\](?::[0-9]{1,5})?', re.IGNORECASE), '<REDACTED_IPV6>'),
    (re.compile(r'(?<![0-9a-f:])(?=[0-9a-f:]*[a-f])(?:[0-9a-f]{1,4}:){2,7}[0-9a-f]{0,4}(?![0-9a-f:])', re.IGNORECASE), '<REDACTED_IPV6>'),
    (re.compile(r'(?<![0-9a-f:])::1(?![0-9a-f:])', re.IGNORECASE), '<REDACTED_IPV6>'),
    (re.compile(r'(?<![0-9])(?:(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})\.){3}(?:25[0-5]|2[0-4][0-9]|1?[0-9]{1,2})(?::[0-9]{1,5})?(?![0-9])'), '<REDACTED_IPV4>'),
    (re.compile(r'\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b', re.IGNORECASE), '<REDACTED_MAC>'),
]

# PS L208-213 final MatchEvaluator rule: a bare hex-colon token is redacted
# only when it parses as a real IPv6 address.
_IPV6_TAIL_RULE = re.compile(r'(?<![0-9a-z:])[0-9a-f:]*:[0-9a-f:]+(?![0-9a-z:])', re.IGNORECASE)


def protect_sensitive_text(text):
    """PS L178-217 Protect-SensitiveText: public version literals are
    tokenized first (sorted by length descending, unique), the real target is
    replaced, the 12 rules run, then tokens are restored. The frozen HDC
    version is a legitimate public literal too (ADJ-20260810-0001 C6)."""
    if text is None:
        return ''
    safe = text
    literal_tokens = {}
    literal_index = 0
    literals = sorted({lit for lit in public_version_literals if lit and lit.strip()},
                      key=len, reverse=True)
    for literal in literals:
        token = '__E3_PUBLIC_VERSION_%d__' % literal_index
        safe = safe.replace(literal, token)
        literal_tokens[token] = literal
        literal_index += 1
    if actual_target and actual_target.strip():
        safe = re.sub(re.escape(actual_target), '<REDACTED_TARGET>', safe, flags=re.IGNORECASE)
    for pattern, replacement in _REDACTION_RULES:
        safe = pattern.sub(replacement, safe)

    def _ipv6_tail(match):
        try:
            addr = ipaddress.ip_address(match.group(0))
            if addr.version == 6:
                return '<REDACTED_IPV6>'
        except ValueError:
            pass
        return match.group(0)

    safe = _IPV6_TAIL_RULE.sub(_ipv6_tail, safe)
    for token, literal in literal_tokens.items():
        safe = safe.replace(token, literal)
    return safe


def protect_sensitive_data(value):
    """PS L218-244 Protect-SensitiveData: recursive dict/str walk plus
    list/tuple and other non-bytes/str/mapping iterables (converted to a
    list and recursed - MINOR-3); keys named
    target/endpoint/host/address/port are replaced outright. bytes and
    non-iterable scalars pass through unchanged."""
    if value is None:
        return None
    if isinstance(value, str):
        return protect_sensitive_text(value)
    if isinstance(value, bytes):
        return value
    if isinstance(value, dict):
        copy = {}
        for key, val in value.items():
            key_name = str(key)
            if re.match(r'^(?:target|endpoint|host|address|port)$', key_name, re.IGNORECASE):
                copy[key_name] = '<REDACTED_ENDPOINT>'
            else:
                copy[key_name] = protect_sensitive_data(val)
        return copy
    if isinstance(value, (list, tuple)):
        return [protect_sensitive_data(item) for item in value]
    try:
        return [protect_sensitive_data(item) for item in value]
    except TypeError:
        return value


# =====================================================================
# Section 3: Contract projection and hashing (design unit U2)
# =====================================================================

# PS L453-491 Get-FreezeContract: 30 fields, fixed order (order is part of
# the contract).
FREEZE_CONTRACT_FIELDS = [
    'exception', 'campaign_id', 'scenario_window_seconds', 'device_alias', 'target_tuple',
    'settings_reallow_expected_path', 'settings_reallow_path_policy', 'settings_revoke_mechanism',
    'settings_vpn_page_policy', 'destroy_terminal_policy', 'process_absent_required_count',
    'process_absent_probe_spacing_seconds', 'process_probe_target', 'operator_trust_model',
    'scenario_invalid_policy', 'layout_verification_profile', 'vpn_conflict_rejection_codes',
    'signing', 'artifact_sha256', 'source', 'sdk', 'hdc', 'runner_sha256', 'code_sha',
    'preflight_inputs_frozen_at', 'cleanup_baseline_frozen', 'collection_ready',
    'independent_review_ready', 'operator_role', 'independent_reviewer_role',
]

# PS L497-544 Get-ConfirmationContract: the stable two-phase projection
# (29 fields). Excludes plan_status / preflight_inputs_frozen_at /
# machine_fresh_confirmation / independent_review_record /
# independent_review_ready; adds evidence_id (ADJ-20260810-0001 C6).
CONFIRMATION_CONTRACT_FIELDS = [
    'exception', 'campaign_id', 'evidence_id', 'scenario_window_seconds', 'device_alias',
    'target_tuple', 'settings_reallow_expected_path', 'settings_reallow_path_policy',
    'settings_revoke_mechanism', 'settings_vpn_page_policy', 'destroy_terminal_policy',
    'process_absent_required_count', 'process_absent_probe_spacing_seconds', 'process_probe_target',
    'operator_trust_model', 'scenario_invalid_policy', 'layout_verification_profile',
    'vpn_conflict_rejection_codes', 'signing', 'artifact_sha256', 'source', 'sdk', 'hdc',
    'runner_sha256', 'code_sha', 'cleanup_baseline_frozen', 'collection_ready',
    'operator_role', 'independent_reviewer_role',
]


def get_freeze_contract(freeze):
    """PS L453-491 Get-FreezeContract: null-safe projection, fixed field
    order (C5)."""
    return {field: get_optional_property(freeze, field, None) for field in FREEZE_CONTRACT_FIELDS}


def get_freeze_contract_sha256(freeze):
    """PS L492-495 Get-FreezeContractSha256: sha256 of the compact JSON."""
    return sha256_text(jsoncompat_dumps(get_freeze_contract(freeze)))


def get_confirmation_contract(freeze):
    """PS L497-544 Get-ConfirmationContract: stable two-phase projection
    (C5)."""
    return {field: get_optional_property(freeze, field, None) for field in CONFIRMATION_CONTRACT_FIELDS}


def get_confirmation_contract_sha256(freeze):
    """PS L545-548 Get-ConfirmationContractSha256."""
    return sha256_text(jsoncompat_dumps(get_confirmation_contract(freeze)))


# =====================================================================
# Section 4: Freeze validation (design unit U2)
# =====================================================================

# PS L610-620 expected frozen target tuple.
EXPECTED_TUPLE = {
    'distribution': 'HarmonyOS',
    'device_model': 'PLA-AL10',
    'full_system_build': 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)',
    'api': '26',
    'kernel_arch': 'aarch64',
    'app_abi': 'arm64-v8a',
}


def get_prior_blocked_binding(freeze):
    """PS L1211-1250 Get-PriorBlockedBinding: optional governance projection
    only - N/A / missing, or a pure hash object. No path/raw copy or
    re-verification."""
    prior = get_optional_property(freeze, 'prior_blocked_binding', None)
    if prior is None:
        return None
    if isinstance(prior, str):
        if prior == 'N/A' or not prior.strip():
            return None
        raise RuntimeError("prior_blocked_binding must be N/A or an object with source='consumed-blocked', evidence_id, and three SHA-256 hashes")
    source = str(get_optional_property(prior, 'source', ''))
    evidence_id = str(get_optional_property(prior, 'evidence_id', ''))
    scenario_results_sha = str(get_optional_property(prior, 'scenario_results_sha256', ''))
    hash_manifest_sha = str(get_optional_property(prior, 'hash_manifest_sha256', ''))
    campaign_seal_sha = str(get_optional_property(prior, 'campaign_seal_sha256', ''))
    provided = [v for v in (source, evidence_id, scenario_results_sha, hash_manifest_sha, campaign_seal_sha)
                if v.strip() and v != 'N/A']
    if len(provided) == 0:
        return None
    if source != 'consumed-blocked':
        raise RuntimeError("prior_blocked_binding.source must be 'consumed-blocked'")
    if not evidence_id.strip() or evidence_id == 'N/A':
        raise RuntimeError('prior_blocked_binding.evidence_id must be non-empty')
    for key, value in (('scenario_results_sha256', scenario_results_sha),
                       ('hash_manifest_sha256', hash_manifest_sha),
                       ('campaign_seal_sha256', campaign_seal_sha)):
        if not test_sha256_hex(value):
            raise RuntimeError('prior_blocked_binding.%s must be a final SHA-256' % key)
    return {
        'source': 'consumed-blocked',
        'evidence_id': evidence_id,
        'scenario_results_sha256': scenario_results_sha.lower(),
        'hash_manifest_sha256': hash_manifest_sha.lower(),
        'campaign_seal_sha256': campaign_seal_sha.lower(),
    }


def assert_freeze_manifest(freeze, freeze_path):
    """PS L550-737 Assert-FreezeManifest: the ordered gate sequence. Every
    gate failure throws with the PS message substring (R18). Old freezes are
    rejected for every mode: the legacy `spacing` field is refused outright
    and the ADJ-20260807-0003 / ADJ-20260808-0001 decision fields are
    required (missing => Get-RequiredProperty throws)."""
    schema_version = get_required_property(freeze, 'schema_version')
    if not test_json_integer(schema_version) or int(schema_version) != 2:
        raise RuntimeError('unsupported freeze schema_version; strong operator state machine requires schema_version 2 as a JSON integer')
    plan_status = str(get_required_property(freeze, 'plan_status'))
    if dry_run or target_binding_confirm:
        if plan_status not in ('blocked', 'ready'):
            raise RuntimeError('DryRun and TargetBindingConfirm plan_status must be blocked or ready')
    elif plan_status != 'ready':
        raise RuntimeError('Live and LiveSimulation require plan_status ready')
    if str(get_required_property(freeze, 'exception')) != 'E3-PHYS-PREFLIGHT':
        raise RuntimeError('exception mismatch')
    evidence_id = str(get_required_property(freeze, 'evidence_id'))
    campaign_id = str(get_required_property(freeze, 'campaign_id'))
    if not re.match(r'^EV-E3-[A-Z0-9-]+-[0-9]{8}-[0-9]{4}$', evidence_id):
        raise RuntimeError('evidence_id format invalid')
    if not re.match(r'^E3-PHYS-PREFLIGHT-[A-Z0-9-]+$', campaign_id):
        raise RuntimeError('campaign_id format invalid')
    attempt = str(get_required_property(freeze, 'attempt'))
    if attempt not in ('initial', 'infrastructure-blocked-retry-1'):
        raise RuntimeError('attempt invalid')
    retry = get_required_property(freeze, 'retry')
    if attempt == 'initial':
        if str(get_required_property(retry, 'basis')) != 'N/A' or str(get_required_property(retry, 'infrastructure_reason')) != 'N/A':
            raise RuntimeError('initial attempt retry fields must be N/A')
    else:
        reason = str(get_required_property(retry, 'infrastructure_reason'))
        if reason not in ('hdc-usb-interruption', 'collection-storage-failure', 'runner-host-failure'):
            raise RuntimeError('retry reason is not in the infrastructure-only allowlist')
        prior_path = normalize_path(str(get_required_property(retry, 'prior_record_path')))
        assert_file_hash('prior blocked record', prior_path, str(get_required_property(retry, 'prior_record_sha256')))
        prior = json.loads(read_text_utf8_sig(prior_path))
        prior_is_evidence = get_required_property(prior, 'is_evidence')
        prior_artifact_canonical = jsoncompat_dumps(get_required_property(prior, 'artifact_sha256'))
        frozen_artifact_canonical = jsoncompat_dumps(freeze['artifact_sha256'])
        if (str(get_required_property(prior, 'campaign_id')) != campaign_id or
                str(get_required_property(prior, 'attempt')) != 'initial' or
                str(get_required_property(prior, 'execution_mode')) != 'live' or
                not isinstance(prior_is_evidence, bool) or not prior_is_evidence or
                str(get_required_property(prior, 'record_status')) != 'blocked' or
                str(get_required_property(prior, 'overall')) != 'blocked' or
                str(get_required_property(prior, 'verdict')) != 'blocked' or
                str(get_required_property(prior, 'infrastructure_reason')) != reason or
                str(get_required_property(prior, 'code_sha')) != str(freeze['code_sha']) or
                str(get_required_property(prior, 'runner_sha256')) != str(freeze['runner_sha256']) or
                prior_artifact_canonical != frozen_artifact_canonical or
                str(get_required_property(prior, 'freeze_contract_sha256')) != get_freeze_contract_sha256(freeze)):
            raise RuntimeError('prior record does not authorize the single infrastructure-blocked retry')
    # ADJ-20260810-0001 (C6): the current AUTH fixes one candidate pair and
    # attempt=initial; the generic infrastructure retry branch never applies.
    if target_binding_confirm:
        if campaign_id != CANDIDATE_CAMPAIGN_ID or evidence_id != CANDIDATE_EVIDENCE_ID:
            raise RuntimeError('TargetBindingConfirm under %s requires the fixed candidate pair %s / %s' % (AUTH_ID, CANDIDATE_CAMPAIGN_ID, CANDIDATE_EVIDENCE_ID))
        if attempt != 'initial':
            raise RuntimeError('TargetBindingConfirm under the current AUTH fixes attempt=initial; retries require new governance and cannot enter this path')
        if str(get_required_property(retry, 'basis')) != 'N/A' or str(get_required_property(retry, 'infrastructure_reason')) != 'N/A':
            raise RuntimeError('TargetBindingConfirm under the current AUTH fixes retry.basis/infrastructure_reason=N/A')
    scenario_window = get_required_property(freeze, 'scenario_window_seconds')
    if not test_json_integer(scenario_window):
        raise RuntimeError('scenario_window_seconds must be a JSON integer')
    if scenario_window != 60:
        raise RuntimeError('scenario window must be exactly 60 seconds')
    if str(get_required_property(freeze, 'device_alias')) != 'PHYS-1':
        raise RuntimeError('device alias must be PHYS-1')
    tuple_ = get_required_property(freeze, 'target_tuple')
    for key, expected in EXPECTED_TUPLE.items():
        if str(get_required_property(tuple_, key)) != expected:
            raise RuntimeError('frozen target tuple mismatch: %s' % key)
    if str(get_required_property(freeze, 'settings_reallow_expected_path')) not in ('direct-system-activation', 'system-reauthorization-UI'):
        raise RuntimeError('settings re-allow path invalid')
    if str(get_required_property(freeze, 'settings_reallow_path_policy')) != 'observation-only':
        raise RuntimeError('settings_reallow_path_policy must be observation-only')
    # ADJ-20260807-0003 decision fields: strict, fixed values. Old freezes
    # without these fields are historical only and are rejected for every
    # mode (DryRun included), never usable for a new live.
    if str(get_required_property(freeze, 'settings_revoke_mechanism')) != 'settings-app-info-force-stop':
        raise RuntimeError('settings_revoke_mechanism must be settings-app-info-force-stop')
    if str(get_required_property(freeze, 'settings_vpn_page_policy')) != 'observation-only':
        raise RuntimeError('settings_vpn_page_policy must be observation-only')
    if str(get_required_property(freeze, 'destroy_terminal_policy')) != 'callback-or-strict-process-boundary':
        raise RuntimeError('destroy_terminal_policy must be callback-or-strict-process-boundary')
    required_count = get_required_property(freeze, 'process_absent_required_count')
    if not test_json_integer(required_count):
        raise RuntimeError('process_absent_required_count must be a JSON integer')
    if required_count != 2:
        raise RuntimeError('process_absent_required_count must be 2')
    # Legacy `spacing` is an unknown field and is never compatibly reused.
    if 'spacing' in freeze:
        raise RuntimeError('legacy spacing field is not part of the freeze schema; use process_absent_probe_spacing_seconds')
    # MAJOR-1: PS treats this field as a double ([double] cast at PS L650,
    # L3822+), so the freeze accepts a JSON double or an int coerced
    # losslessly (3 -> 3.0); strings, bools and non-finite values are
    # rejected; the value gate stays 3 seconds.
    spacing = get_required_property(freeze, 'process_absent_probe_spacing_seconds')
    spacing_value = coerce_json_double(spacing)
    if spacing_value is None:
        raise RuntimeError('process_absent_probe_spacing_seconds must be a JSON double')
    if spacing_value <= 0 or spacing_value > 86400:
        raise RuntimeError('process_absent_probe_spacing_seconds must be 3 seconds')
    if spacing_value != 3.0:
        raise RuntimeError('process_absent_probe_spacing_seconds must be 3 seconds')
    # ADJ-20260808-0001 decision field: pidof targets the <bundle>:vpn
    # Extension ability process, never the bundle UI process.
    if str(get_required_property(freeze, 'process_probe_target')) != '<bundle>:vpn':
        raise RuntimeError('process_probe_target must be <bundle>:vpn (ADJ-20260808-0001 extension-process probe target)')
    if str(get_required_property(freeze, 'operator_trust_model')) != 'mechanical-action-only-machine-verified-v1':
        raise RuntimeError('operator_trust_model must be mechanical-action-only-machine-verified-v1')
    if str(get_required_property(freeze, 'scenario_invalid_policy')) != 'stop-and-finally-cleanup-seal':
        raise RuntimeError('scenario_invalid_policy must be stop-and-finally-cleanup-seal')
    if str(get_required_property(freeze, 'layout_verification_profile')) != 'deterministic-layout-v1':
        raise RuntimeError('layout_verification_profile must be deterministic-layout-v1')
    # MAJOR-1: JSON integer elements - int and integral-valued float
    # (2203002.0 -> 2203002) are accepted; non-integral floats, strings and
    # bools are rejected (no PS [int] rounding or truncation is replicated).
    conflict_codes_raw = get_required_property(freeze, 'vpn_conflict_rejection_codes')
    if not isinstance(conflict_codes_raw, list):
        raise RuntimeError('vpn_conflict_rejection_codes must be a JSON array of integers')
    conflict_codes = []
    for code in conflict_codes_raw:
        code_value = coerce_json_integer(code)
        if code_value is None:
            raise RuntimeError('vpn_conflict_rejection_codes must be a JSON array of integers')
        conflict_codes.append(code_value)
    if ','.join(str(x) for x in conflict_codes) != '2203002':
        raise RuntimeError('vpn_conflict_rejection_codes must freeze the explicit supported list [2203002]')
    signing = get_required_property(freeze, 'signing')
    if str(get_required_property(signing, 'type')) != 'ordinary-development':
        raise RuntimeError('signing type must be ordinary-development')
    assert_json_boolean(signing, 'device_in_profile', True)
    profile_basis = str(get_required_property(signing, 'device_in_profile_basis'))
    if not profile_basis.strip() or '<' in profile_basis:
        raise RuntimeError('signing.device_in_profile_basis incomplete')
    for field in ('public_fingerprint', 'verification_result'):
        value = str(get_required_property(signing, field))
        if not value.strip() or '<' in value:
            raise RuntimeError('signing.%s incomplete' % field)
    if str(signing['verification_result']) != 'pass':
        raise RuntimeError('local signature verification_result must be pass')
    artifacts = get_required_property(freeze, 'artifact_sha256')
    hap_a_hash = str(get_required_property(artifacts, 'hap_a'))
    hap_b_hash = str(get_required_property(artifacts, 'hap_b'))
    if normalize_path(hap_a) == normalize_path(hap_b) or hap_a_hash == hap_b_hash:
        raise RuntimeError('FINAL HAP A/B must be distinct files and hashes')
    assert_file_hash('FINAL HAP A', normalize_path(hap_a), hap_a_hash)
    assert_file_hash('FINAL HAP B', normalize_path(hap_b), hap_b_hash)
    source = get_required_property(freeze, 'source')
    assert_file_hash('source archive', str(get_required_property(source, 'archive_path')), str(get_required_property(source, 'archive_sha256')))
    assert_file_hash('source manifest', str(get_required_property(source, 'manifest_path')), str(get_required_property(source, 'manifest_sha256')))
    sdk = get_required_property(freeze, 'sdk')
    for field in ('version', 'api', 'syscap_basis'):
        if not str(get_required_property(sdk, field)).strip():
            raise RuntimeError('sdk.%s incomplete' % field)
    sdk_files = get_required_property(sdk, 'files')
    if len(sdk_files) < 1:
        raise RuntimeError('sdk hash map is empty')
    for sdk_file in sdk_files:
        assert_file_hash('SDK input', str(get_required_property(sdk_file, 'path')), str(get_required_property(sdk_file, 'sha256')))
    hdc = get_required_property(freeze, 'hdc')
    hdc_version = str(get_required_property(hdc, 'version'))
    if not hdc_version.strip() or '<' in hdc_version:
        raise RuntimeError('hdc.version incomplete')
    assert_file_hash('HDC executable', normalize_path(hdc_path), str(get_required_property(hdc, 'sha256')))
    assert_file_hash('runner', os.path.abspath(__file__), str(get_required_property(freeze, 'runner_sha256')))
    if not re.match(r'^[0-9a-f]{40}$', str(get_required_property(freeze, 'code_sha'))):
        raise RuntimeError('code_sha incomplete')
    frozen_at = parse_datetime(get_required_property(freeze, 'preflight_inputs_frozen_at'))
    if frozen_at is None:
        raise RuntimeError('preflight_inputs_frozen_at invalid')
    for role in ('operator_role', 'independent_reviewer_role'):
        role_value = str(get_required_property(freeze, role))
        if not role_value.strip() or '<' in role_value:
            raise RuntimeError('%s incomplete' % role)
    if str(freeze['operator_role']) == str(freeze['independent_reviewer_role']):
        raise RuntimeError('operator and independent reviewer roles must differ')
    assert_json_boolean(freeze, 'cleanup_baseline_frozen', True)
    assert_json_boolean(freeze, 'collection_ready', True)
    assert_json_boolean(freeze, 'independent_review_ready', True)
    # ADJ-20260810-0001 (C6): independent review record gate. A blocked
    # confirmation freeze keeps independent_review_ready=true as a static
    # marker and does NOT need a review record; ready Live / ready DryRun
    # require a real pass review record (enforced in
    # assert_independent_review_record).
    review_record = get_optional_property(freeze, 'independent_review_record', None)
    if review_record is None:
        if not target_binding_confirm:
            raise RuntimeError('freeze manifest missing property: independent_review_record')
    else:
        review_status = str(get_optional_property(review_record, 'status', ''))
        if review_status not in ('pending', 'pass'):
            raise RuntimeError('independent_review_record.status must be pending or pass')
        if review_status == 'pass':
            if not str(get_optional_property(review_record, 'record_path', '')).strip() or not str(get_optional_property(review_record, 'record_sha256', '')).strip():
                raise RuntimeError('independent_review_record with status=pass requires record_path and record_sha256')
    if not os.path.isfile(freeze_path):
        raise RuntimeError('FreezeManifest file missing')
    get_prior_blocked_binding(freeze)
    assert_machine_fresh_confirmation(freeze)


# =====================================================================
# Section 5: Mode exclusivity and TargetBindingConfirm (design unit U3)
# =====================================================================


def assert_mode_exclusivity(args):
    """PS L738-752 Assert-ModeExclusivity. Runs BEFORE the SelfTest early
    exit so invalid switch combinations are rejected even with -SelfTest."""
    if args.target_binding_confirm and (args.dry_run or args.live_simulation or args.self_test):
        raise RuntimeError('TargetBindingConfirm is mutually exclusive with DryRun, LiveSimulation, and SelfTest')
    if args.target_binding_confirm and not (args.confirmation_record or '').strip():
        raise RuntimeError('TargetBindingConfirm requires ConfirmationRecord')
    if not args.target_binding_confirm and (args.confirmation_record or '').strip():
        raise RuntimeError('ConfirmationRecord is only valid with TargetBindingConfirm')
    if args.target_binding_confirm and (args.evidence_root or '').strip():
        raise RuntimeError('EvidenceRoot is not allowed with TargetBindingConfirm')
    if args.target_binding_confirm and (args.raw_root or '').strip():
        raise RuntimeError('RawRoot is not allowed with TargetBindingConfirm')
    if args.dry_run and args.live_simulation:
        raise RuntimeError('DryRun and LiveSimulation are mutually exclusive')


def _get_confirmation_field(record, name):
    """PS Get-ConfirmationField (L826-830): missing/empty field rejection."""
    value = str(get_optional_property(record, name, ''))
    if not value.strip():
        raise RuntimeError('confirmation record missing or empty field: %s' % name)
    return value


def _get_review_field(record, name):
    """PS Get-ReviewField (L940-944): missing/empty field rejection."""
    value = str(get_optional_property(record, name, ''))
    if not value.strip():
        raise RuntimeError('independent review record missing or empty field: %s' % name)
    return value


# PS L823-825: exact-schema gate - any unknown top-level field makes the
# record un-consumable.
CONFIRMATION_ALLOWED_FIELDS = [
    'schema_version', 'record_kind', 'is_evidence', 'authorization_id', 'exception',
    'campaign_id', 'evidence_id', 'attempt', 'retry', 'plan_status', 'device_alias',
    'target_redacted', 'code_sha', 'runner_sha256', 'freeze_manifest_sha256',
    'confirmation_contract_sha256', 'hdc_sha256', 'hdc_version', 'expected_model',
    'expected_build', 'observed_model', 'observed_build', 'started_at', 'ended_at',
    'command_attempted', 'command_completed', 'command_count', 'repository_fingerprint',
    'verdict', 'reason',
]

# PS L937-939: exact-schema gate for the review record.
REVIEW_ALLOWED_FIELDS = [
    'schema_version', 'record_kind', 'is_evidence', 'exception', 'campaign_id',
    'evidence_id', 'code_sha', 'runner_sha256', 'confirmation_contract_sha256',
    'machine_confirmation_sha256', 'reviewer_role', 'operator_role', 'verdict',
    'blockers', 'majors', 'started_at', 'ended_at',
]


def assert_machine_fresh_confirmation(freeze):
    """PS L753-896 Assert-MachineFreshConfirmation: host-governed fresh
    confirmation consumer. TargetBindingConfirm IS the producer, so it may
    consume a pending/absent machine_fresh_confirmation on a blocked freeze.
    Live (real device) and DryRun with plan_status ready require status=pass
    bound to AUTH-E3-PHYS1API26-20260816-0001 and the fixed candidate pair,
    with a real out-of-repository double-file record (JSON + matching .sha256
    companion) whose content agrees with the freeze and whose time anchors
    satisfy started_at <= ended_at <= preflight_inputs_frozen_at. A blocked
    DryRun that declares status=pass is FULLY validated too
    (ValidateDeclaredPass)."""
    if target_binding_confirm:
        return None
    plan_status = str(freeze['plan_status'])
    require_pass = (execution_mode == 'live' and not live_simulation) or (dry_run and plan_status == 'ready')
    confirmation = get_optional_property(freeze, 'machine_fresh_confirmation', None)
    confirmation_status = '' if confirmation is None else str(get_optional_property(confirmation, 'status', ''))
    validate_machine = require_pass or (dry_run and plan_status == 'blocked' and confirmation_status == 'pass')
    if not validate_machine:
        # ADJ-20260810-0001 (C6): a blocked DryRun may skip a pending/absent
        # machine confirmation, but a declared-pass independent review can
        # never ride on it.
        if dry_run and plan_status == 'blocked':
            review = get_optional_property(freeze, 'independent_review_record', None)
            review_status = '' if review is None else str(get_optional_property(review, 'status', ''))
            if review_status == 'pass':
                raise RuntimeError('independent_review_record.status=pass requires machine_fresh_confirmation.status=pass; a pending/absent machine confirmation cannot anchor a declared-pass review')
        return None
    if require_pass and confirmation is None:
        raise RuntimeError('a ready plan_status requires machine_fresh_confirmation with status=pass and a bound target-binding confirmation record')
    if confirmation_status != 'pass':
        raise RuntimeError('machine_fresh_confirmation.status must be pass for a ready plan_status')
    authorization_id = str(get_optional_property(confirmation, 'authorization_id', ''))
    if authorization_id != AUTH_ID:
        raise RuntimeError('machine_fresh_confirmation.authorization_id does not match %s' % AUTH_ID)
    if str(get_optional_property(freeze, 'campaign_id')) != CANDIDATE_CAMPAIGN_ID or str(get_optional_property(freeze, 'evidence_id')) != CANDIDATE_EVIDENCE_ID:
        raise RuntimeError('AUTH %s fixes the candidate pair %s / %s; a ready freeze outside that pair cannot consume its confirmation' % (AUTH_ID, CANDIDATE_CAMPAIGN_ID, CANDIDATE_EVIDENCE_ID))
    if str(get_optional_property(freeze, 'attempt')) != 'initial':
        raise RuntimeError('the current AUTH fixes attempt=initial; retries require new governance and can never consume this confirmation')
    frozen_retry = get_optional_property(freeze, 'retry', None)
    if frozen_retry is not None:
        if str(get_optional_property(frozen_retry, 'basis', '')) != 'N/A' or str(get_optional_property(frozen_retry, 'infrastructure_reason', '')) != 'N/A':
            raise RuntimeError('the current AUTH fixes retry.basis/infrastructure_reason=N/A; the generic infrastructure retry branch never applies to this confirmation path')
    record_path_input = str(get_optional_property(confirmation, 'record_path', ''))
    if not record_path_input.strip():
        raise RuntimeError('machine_fresh_confirmation.record_path missing')
    record_path = normalize_path(record_path_input)
    if not os.path.isfile(record_path):
        raise RuntimeError('confirmation record file missing')
    if repo_root is not None and (record_path == repo_root or is_under_path(record_path, repo_root)):
        raise RuntimeError('confirmation record must be outside the git repository')
    assert_no_reparse_ancestor(record_path)
    companion_path = record_path + '.sha256'
    assert_no_reparse_ancestor(companion_path)
    if not os.path.isfile(companion_path):
        raise RuntimeError('confirmation record .sha256 companion missing; a lone record is never consumable')
    companion_value = read_text_utf8_sig(companion_path).strip()
    if not re.match(r'^[0-9a-f]{64}$', companion_value):
        raise RuntimeError('confirmation record companion does not contain a final SHA-256')
    if companion_value != sha256_file(record_path):
        raise RuntimeError('confirmation record .sha256 companion does not match the record bytes')
    assert_file_hash('confirmation record', record_path, str(get_optional_property(confirmation, 'record_sha256', '')))
    record = json.loads(read_text_utf8_sig(record_path))
    for field in record:
        if field not in CONFIRMATION_ALLOWED_FIELDS:
            raise RuntimeError('confirmation record has an unknown top-level field: %s' % field)
    record_schema = get_optional_property(record, 'schema_version', None)
    if not test_json_integer(record_schema) or int(record_schema) != 1:
        raise RuntimeError('confirmation record schema_version must be 1')
    if _get_confirmation_field(record, 'record_kind') != 'target-binding-confirmation':
        raise RuntimeError('confirmation record record_kind mismatch')
    if _get_confirmation_field(record, 'exception') != 'E3-PHYS-PREFLIGHT':
        raise RuntimeError('confirmation record exception mismatch')
    record_is_evidence = get_optional_property(record, 'is_evidence', None)
    if not isinstance(record_is_evidence, bool) or record_is_evidence:
        raise RuntimeError('confirmation record must be is_evidence=false')
    if _get_confirmation_field(record, 'verdict') != 'pass':
        raise RuntimeError('confirmation record verdict must be pass')
    if _get_confirmation_field(record, 'reason') != 'N/A':
        raise RuntimeError('confirmation record reason must be N/A for a pass verdict')
    if _get_confirmation_field(record, 'authorization_id') != authorization_id:
        raise RuntimeError('confirmation record authorization_id mismatch')
    if _get_confirmation_field(record, 'campaign_id') != CANDIDATE_CAMPAIGN_ID or _get_confirmation_field(record, 'evidence_id') != CANDIDATE_EVIDENCE_ID:
        raise RuntimeError('confirmation record candidate IDs do not match the fixed AUTH candidate pair')
    if _get_confirmation_field(record, 'attempt') != 'initial':
        raise RuntimeError('confirmation record attempt must be initial under the current AUTH')
    record_retry = get_optional_property(record, 'retry', None)
    if record_retry is None or str(get_optional_property(record_retry, 'basis', '')) != 'N/A' or str(get_optional_property(record_retry, 'infrastructure_reason', '')) != 'N/A':
        raise RuntimeError('confirmation record retry.basis/infrastructure_reason must be N/A under the current AUTH')
    if _get_confirmation_field(record, 'device_alias') != 'PHYS-1':
        raise RuntimeError('confirmation record device_alias must be PHYS-1')
    record_target_redacted = get_optional_property(record, 'target_redacted', None)
    if not isinstance(record_target_redacted, bool) or not record_target_redacted:
        raise RuntimeError('confirmation record target_redacted must be true')
    if _get_confirmation_field(record, 'code_sha') != str(freeze['code_sha']):
        raise RuntimeError('confirmation record code_sha does not match the freeze')
    if _get_confirmation_field(record, 'runner_sha256') != str(freeze['runner_sha256']):
        raise RuntimeError('confirmation record runner_sha256 does not match the freeze')
    if _get_confirmation_field(record, 'hdc_sha256') != str(freeze['hdc']['sha256']):
        raise RuntimeError('confirmation record hdc_sha256 does not match the freeze')
    if _get_confirmation_field(record, 'hdc_version') != str(freeze['hdc']['version']):
        raise RuntimeError('confirmation record hdc_version does not match the freeze')
    if _get_confirmation_field(record, 'confirmation_contract_sha256') != get_confirmation_contract_sha256(freeze):
        raise RuntimeError('confirmation record confirmation_contract_sha256 does not match the current confirmation contract')
    expected_model = _get_confirmation_field(record, 'expected_model')
    expected_build = _get_confirmation_field(record, 'expected_build')
    observed_model = _get_confirmation_field(record, 'observed_model')
    observed_build = _get_confirmation_field(record, 'observed_build')
    if expected_model != str(freeze['target_tuple']['device_model']) or expected_build != str(freeze['target_tuple']['full_system_build']):
        raise RuntimeError('confirmation record expected model/build do not match the freeze tuple')
    if observed_model != expected_model or observed_build != expected_build:
        raise RuntimeError('confirmation record observed model/build do not match the expected frozen tuple')
    attempted = get_optional_property(record, 'command_attempted', None)
    completed = get_optional_property(record, 'command_completed', None)
    if not test_json_integer(attempted) or int(attempted) != 3 or not test_json_integer(completed) or int(completed) != 3:
        raise RuntimeError('confirmation record command_attempted and command_completed must both be exactly 3 for a pass')
    # ADJ-20260810-0001 (C6): command_count is the producer's compatibility
    # alias of command_completed; when present it must agree.
    command_count = get_optional_property(record, 'command_count', None)
    if command_count is not None and (not test_json_integer(command_count) or int(command_count) != int(completed)):
        raise RuntimeError('confirmation record command_count must equal command_completed (compatibility alias) for a pass')
    started_at = parse_datetime(_get_confirmation_field(record, 'started_at'))
    ended_at = parse_datetime(_get_confirmation_field(record, 'ended_at'))
    frozen_at = parse_datetime(get_optional_property(freeze, 'preflight_inputs_frozen_at', None))
    if started_at is None or ended_at is None or frozen_at is None:
        raise RuntimeError('confirmation record started_at/ended_at or freeze preflight_inputs_frozen_at invalid')
    if started_at > ended_at:
        raise RuntimeError('confirmation record started_at must not be after ended_at')
    if ended_at > frozen_at:
        raise RuntimeError('confirmation record ended_at must be no later than freeze preflight_inputs_frozen_at')
    # ADJ-20260810-0001 (C6): ready Live / ready DryRun additionally require
    # the independent review record mechanical gate.
    assert_independent_review_record(freeze)
    global machine_fresh_confirmation
    machine_fresh_confirmation = {
        'status': 'pass',
        'authorization_id': authorization_id,
        'record_sha256': str(get_optional_property(confirmation, 'record_sha256', '')),
        'record_path_sha256': sha256_text(record_path),
    }
    return {'record_path': record_path, 'record_sha256': str(get_optional_property(confirmation, 'record_sha256', ''))}


def assert_independent_review_record(freeze):
    """PS L897-1005 Assert-IndependentReviewRecord: ready Live / ready DryRun
    require a mechanical independent-review record (out-of-repo JSON + .sha256
    companion) proving the ready freeze contract was reviewed by a separate
    reviewer role. The review record must bind the same freeze contract and
    the machine confirmation record hash so the three objects (confirmation
    record, review record, freeze) form one consistent chain. Time chain:
    machine ended_at <= review started_at <= review ended_at <= final ready
    freeze preflight_inputs_frozen_at."""
    if target_binding_confirm:
        return None
    plan_status = str(freeze['plan_status'])
    require_pass = (execution_mode == 'live' and not live_simulation) or (dry_run and plan_status == 'ready')
    review = get_optional_property(freeze, 'independent_review_record', None)
    review_status = '' if review is None else str(get_optional_property(review, 'status', ''))
    validate_review = require_pass or (dry_run and plan_status == 'blocked' and review_status == 'pass')
    if not validate_review:
        return None
    if require_pass and review is None:
        raise RuntimeError('a ready plan_status requires independent_review_record.status=pass with a bound out-of-repository review record')
    if review_status != 'pass':
        raise RuntimeError('independent_review_record.status must be pass for a ready plan_status; independent_review_ready=true alone is a static readiness marker and never an execution gate')
    reviewer_role = str(get_optional_property(review, 'reviewer_role', ''))
    if not reviewer_role.strip() or reviewer_role != str(freeze['independent_reviewer_role']):
        raise RuntimeError('independent_review_record.reviewer_role does not match the freeze independent_reviewer_role')
    record_path_input = str(get_optional_property(review, 'record_path', ''))
    if not record_path_input.strip():
        raise RuntimeError('independent_review_record.record_path missing')
    record_path = normalize_path(record_path_input)
    if not os.path.isfile(record_path):
        raise RuntimeError('independent review record file missing')
    if repo_root is not None and (record_path == repo_root or is_under_path(record_path, repo_root)):
        raise RuntimeError('independent review record must be outside the git repository')
    assert_no_reparse_ancestor(record_path)
    companion_path = record_path + '.sha256'
    assert_no_reparse_ancestor(companion_path)
    if not os.path.isfile(companion_path):
        raise RuntimeError('independent review record .sha256 companion missing; a lone record is never consumable')
    if read_text_utf8_sig(companion_path).strip() != sha256_file(record_path):
        raise RuntimeError('independent review record .sha256 companion does not match the record bytes')
    assert_file_hash('independent review record', record_path, str(get_optional_property(review, 'record_sha256', '')))
    record = json.loads(read_text_utf8_sig(record_path))
    for field in record:
        if field not in REVIEW_ALLOWED_FIELDS:
            raise RuntimeError('independent review record has an unknown top-level field: %s' % field)
    review_schema = get_optional_property(record, 'schema_version', None)
    if not test_json_integer(review_schema) or int(review_schema) != 1:
        raise RuntimeError('independent review record schema_version must be 1')
    if _get_review_field(record, 'record_kind') != 'e3-ready-freeze-review':
        raise RuntimeError('independent review record record_kind mismatch')
    if _get_review_field(record, 'exception') != 'E3-PHYS-PREFLIGHT':
        raise RuntimeError('independent review record exception mismatch')
    review_is_evidence = get_optional_property(record, 'is_evidence', None)
    if not isinstance(review_is_evidence, bool) or review_is_evidence:
        raise RuntimeError('independent review record must be is_evidence=false')
    if _get_review_field(record, 'verdict') != 'pass':
        raise RuntimeError('independent review record verdict must be pass')
    blockers = get_optional_property(record, 'blockers', None)
    majors = get_optional_property(record, 'majors', None)
    if not test_json_integer(blockers) or int(blockers) != 0 or not test_json_integer(majors) or int(majors) != 0:
        raise RuntimeError('independent review record requires blockers=0 and majors=0 for a pass verdict')
    if _get_review_field(record, 'reviewer_role') != reviewer_role:
        raise RuntimeError('independent review record reviewer_role mismatch')
    if reviewer_role == str(freeze['operator_role']):
        raise RuntimeError('independent review record reviewer_role must differ from the operator role')
    if _get_review_field(record, 'campaign_id') != CANDIDATE_CAMPAIGN_ID or _get_review_field(record, 'evidence_id') != CANDIDATE_EVIDENCE_ID:
        raise RuntimeError('independent review record candidate IDs do not match the fixed AUTH candidate pair')
    if _get_review_field(record, 'code_sha') != str(freeze['code_sha']):
        raise RuntimeError('independent review record code_sha does not match the freeze')
    if _get_review_field(record, 'runner_sha256') != str(freeze['runner_sha256']):
        raise RuntimeError('independent review record runner_sha256 does not match the freeze')
    if _get_review_field(record, 'confirmation_contract_sha256') != get_confirmation_contract_sha256(freeze):
        raise RuntimeError('independent review record confirmation_contract_sha256 does not match the current confirmation contract')
    machine_confirmation = get_optional_property(freeze, 'machine_fresh_confirmation', None)
    if machine_confirmation is None:
        raise RuntimeError('independent review record requires a bound machine_fresh_confirmation on the freeze')
    # ADJ-20260810-0001 (C6): a declared-pass review binds the machine
    # confirmation hash, so the machine side must itself be a fully validated
    # pass.
    if str(get_optional_property(machine_confirmation, 'status', '')) != 'pass':
        raise RuntimeError('independent review record requires machine_fresh_confirmation.status=pass; a pending/absent machine confirmation cannot anchor a declared-pass review')
    machine_sha = str(get_optional_property(machine_confirmation, 'record_sha256', ''))
    if not machine_sha.strip():
        raise RuntimeError('independent review record requires machine_fresh_confirmation.record_sha256 on the freeze')
    if _get_review_field(record, 'machine_confirmation_sha256') != machine_sha:
        raise RuntimeError('independent review record machine_confirmation_sha256 does not match the machine confirmation record')
    review_started = parse_datetime(_get_review_field(record, 'started_at'))
    review_ended = parse_datetime(_get_review_field(record, 'ended_at'))
    frozen_at = parse_datetime(get_optional_property(freeze, 'preflight_inputs_frozen_at', None))
    if review_started is None or review_ended is None or frozen_at is None:
        raise RuntimeError('independent review record started_at/ended_at or freeze preflight_inputs_frozen_at invalid')
    if review_started > review_ended:
        raise RuntimeError('independent review record started_at must not be after ended_at')
    if review_ended > frozen_at:
        raise RuntimeError('independent review record ended_at must be no later than freeze preflight_inputs_frozen_at')
    # ADJ-20260810-0001 (C6): the review happens AFTER the machine
    # confirmation: machine confirmation ended_at <= review started_at <=
    # review ended_at <= final ready freeze frozen_at.
    machine_record_path = normalize_path(str(get_optional_property(machine_confirmation, 'record_path', '')))
    if not os.path.isfile(machine_record_path):
        raise RuntimeError('independent review record requires the bound machine confirmation record file')
    machine_record = json.loads(read_text_utf8_sig(machine_record_path))
    machine_ended_at = parse_datetime(get_optional_property(machine_record, 'ended_at', None))
    if machine_ended_at is None:
        raise RuntimeError('machine confirmation record ended_at invalid')
    if machine_ended_at > review_started:
        raise RuntimeError('independent review must start no earlier than the machine confirmation ended_at')
    global independent_review_record
    independent_review_record = {
        'status': 'pass',
        'reviewer_role': reviewer_role,
        'record_sha256': str(get_optional_property(review, 'record_sha256', '')),
        'record_path_sha256': sha256_text(record_path),
    }
    return {'record_path': record_path, 'record_sha256': str(get_optional_property(review, 'record_sha256', ''))}


def get_target_binding_confirm_plan():
    """PS L1006-1010 Get-TargetBindingConfirmPlan: exactly the three frozen
    target-binding probes already allowlisted by get_hdc_invocation. No
    install, staging, capture, cleanup, bundle, or process query may ever
    appear."""
    return [
        {'operation': 'Version', 'parameters': {}},
        {'operation': 'TupleModel', 'parameters': {}},
        {'operation': 'TupleBuild', 'parameters': {}},
    ]


def new_target_binding_confirmation_record(freeze, freeze_sha256, confirmation_contract_sha256,
                                           repository_before, started_at, ended_at, verdict,
                                           reason, observed_version, observed_model,
                                           observed_build, command_attempted, command_completed):
    """PS L1017-1076 New-TargetBindingConfirmationRecord: the 30-field
    confirmation record. The record binds the STABLE confirmation contract
    (the two-phase-invariant projection), not the full freeze contract
    (ADJ-20260810-0001 C6)."""
    return {
        'schema_version': 1,
        'record_kind': 'target-binding-confirmation',
        'is_evidence': False,
        'authorization_id': AUTH_ID,
        'exception': 'E3-PHYS-PREFLIGHT',
        'campaign_id': str(freeze['campaign_id']),
        'evidence_id': str(freeze['evidence_id']),
        'attempt': str(freeze['attempt']),
        'retry': {
            'basis': str(freeze['retry']['basis']),
            'infrastructure_reason': str(freeze['retry']['infrastructure_reason']),
        },
        'plan_status': str(freeze['plan_status']),
        'device_alias': 'PHYS-1',
        'target_redacted': True,
        'code_sha': str(freeze['code_sha']),
        'runner_sha256': str(freeze['runner_sha256']),
        'freeze_manifest_sha256': freeze_sha256,
        'confirmation_contract_sha256': confirmation_contract_sha256,
        'hdc_sha256': str(freeze['hdc']['sha256']),
        'hdc_version': observed_version,
        'expected_model': str(freeze['target_tuple']['device_model']),
        'expected_build': str(freeze['target_tuple']['full_system_build']),
        'observed_model': observed_model,
        'observed_build': observed_build,
        'started_at': isoformat_7(started_at),
        'ended_at': isoformat_7(ended_at),
        'command_attempted': command_attempted,
        'command_completed': command_completed,
        'command_count': command_completed,
        'repository_fingerprint': repository_before['fingerprint'],
        'verdict': verdict,
        'reason': 'N/A' if not reason else str(reason),
    }


def write_target_binding_confirmation_record_pair(record_path, record):
    """PS L1077-1114 Write-TargetBindingConfirmationRecordPair: double-file
    completion marker. The JSON record and its .sha256 companion are the
    atomic completion pair: a consumer only accepts the record when BOTH
    files exist and the companion matches the record bytes. The JSON tmp and
    the companion tmp are written first (no-clobber CreateNew, complete
    open().write() then flush+fsync) and the hash is recomputed over the tmp
    JSON; the JSON is committed with a no-replace hard link
    (os.link + os.unlink, same filesystem), then the companion is committed
    no-replace LAST as the completion marker. A pre-existing destination
    raises FileExistsError -> PreRecordGateError (exit 1), never an
    overwrite; if the companion commit fails, the JSON is left as an orphan
    that is never consumed or overwritten. Returns the SHA-256 of the final
    JSON bytes."""
    companion_path = record_path + '.sha256'
    for candidate in (record_path, companion_path):
        if os.path.exists(candidate):
            raise PreRecordGateError('confirmation output already exists and is immutable: %s' % candidate)
    tmp_json = record_path + '.tmp-' + uuid.uuid4().hex
    tmp_sha = companion_path + '.tmp-' + uuid.uuid4().hex
    if os.path.exists(tmp_json):
        raise PreRecordGateError('confirmation JSON temp candidate already exists')
    if os.path.exists(tmp_sha):
        raise PreRecordGateError('confirmation companion temp candidate already exists')
    json_moved = False
    try:
        json_text = jsoncompat_dumps(record, indent=2) + '\n'
        fd = os.open(tmp_json, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
            f.write(json_text)
            f.flush()
            os.fsync(f.fileno())
        sha = sha256_file(tmp_json)
        sha_text = sha + '\n'
        fd = os.open(tmp_sha, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        with os.fdopen(fd, 'w', encoding='utf-8', newline='') as f:
            f.write(sha_text)
            f.flush()
            os.fsync(f.fileno())
        if sha256_file(tmp_json) != sha:
            raise RuntimeError('confirmation record hash recompute mismatch')
        os.link(tmp_json, record_path)
        os.unlink(tmp_json)
        json_moved = True
        # Companion committed LAST: its presence is the completion marker.
        os.link(tmp_sha, companion_path)
        os.unlink(tmp_sha)
    except FileExistsError as e:
        # No-replace backstop: a destination appeared after the pre-check.
        # Never overwrite; surface as a pre-record gate failure (exit 1).
        raise PreRecordGateError('confirmation output already exists and is immutable: %s' % e)
    finally:
        # A moved-but-uncompanioned JSON is an orphan: never delete it (it is
        # the only trace of the failure and the consumer requires the
        # companion, so it can never be consumed or overwritten).
        if not json_moved and os.path.exists(tmp_json):
            try:
                os.unlink(tmp_json)
            except OSError:
                pass
        if os.path.exists(tmp_sha):
            try:
                os.unlink(tmp_sha)
            except OSError:
                pass
    return sha


class PreRecordGateError(RuntimeError):
    """Pre-record gate failure in TargetBindingConfirm mode: nothing was
    written and the process must exit 1 (PS: uncaught exception exit code 1,
    design document C2)."""


def invoke_target_binding_confirm(freeze, freeze_sha256, confirmation_contract_sha256, repository_before):
    """PS L1115-1205 Invoke-TargetBindingConfirm: host-governed machine fresh
    confirmation. The confirmation record is a single-use immutable
    out-of-repository double-file object (JSON + .sha256 companion); the real
    target never enters it. Pre-record gate failures (record path issues, a
    pre-existing record/companion, checked up front AND backstopped by the
    no-replace os.link commit) raise PreRecordGateError and exit 1 with
    NO record written. Probe/preflight failures (bad target token,
    environment, tuple drift) and record-write failures produce a best-effort
    blocked record + companion and exit 2; no campaign roots are ever
    created. A pass verdict requires attempted=completed=3 and a complete
    double-file pair."""
    record_path = normalize_path(confirmation_record)
    if is_under_path(record_path, repo_root):
        raise PreRecordGateError('ConfirmationRecord must be outside the git repository')
    assert_no_reparse_ancestor(record_path)
    companion_path = record_path + '.sha256'
    assert_no_reparse_ancestor(companion_path)
    # Single-use no-clobber gates: the record AND its companion must both be
    # absent up front. A leftover orphan JSON from a previous companion
    # failure is never overwritten and, because the consumer requires both
    # files, is never consumable.
    if os.path.exists(record_path):
        raise PreRecordGateError('ConfirmationRecord already exists; target-binding confirmation is single-use')
    if os.path.exists(companion_path):
        raise PreRecordGateError('ConfirmationRecord .sha256 companion already exists; target-binding confirmation is single-use')
    started_at = datetime.now().astimezone()
    verdict = 'blocked'
    reason = None
    observed_version = None
    observed_model = None
    observed_build = None
    command_attempted = 0
    command_completed = 0
    try:
        assert_target_environment()
        for step in get_target_binding_confirm_plan():
            command_attempted += 1
            result = invoke_hdc_operation(step['operation'], step['parameters'])
            command_completed += 1
            if step['operation'] == 'Version':
                observed_version = result.stdout.strip()
            elif step['operation'] == 'TupleModel':
                observed_model = result.stdout.strip()
            elif step['operation'] == 'TupleBuild':
                observed_build = result.stdout.strip()
        if observed_version != str(freeze['hdc']['version']):
            raise RuntimeError('preflight: frozen HDC version mismatch')
        if observed_model != str(freeze['target_tuple']['device_model']):
            raise RuntimeError('preflight: frozen device model mismatch')
        if observed_build != str(freeze['target_tuple']['full_system_build']):
            raise RuntimeError('preflight: frozen full system build mismatch')
        # ADJ-20260810-0001 (C6): mechanical pass exit - a pass verdict
        # requires exactly three HDC processes started and
        # attempted=completed=3, asserted BEFORE the record is written so a
        # pass double-file pair is never generated and then downgraded.
        if command_attempted != 3 or command_completed != 3 or hdc_process_start_count != 3:
            raise RuntimeError('machine confirmation pass requires exactly 3 HDC processes started and attempted/completed=3 (hdc_processes_started=%d, attempted=%d, completed=%d)' % (hdc_process_start_count, command_attempted, command_completed))
        verdict = 'pass'
    except Exception as e:
        # Probe/tuple failure: still write a best-effort blocked record +
        # companion (exit 2).
        reason = protect_sensitive_text(str(e))
    ended_at = datetime.now().astimezone()
    # Every device-observed value is protected before it can enter the
    # record; the real target never appears in any field.
    safe_observed_version = protect_sensitive_text(observed_version)
    safe_observed_model = protect_sensitive_text(observed_model)
    safe_observed_build = protect_sensitive_text(observed_build)
    record = new_target_binding_confirmation_record(freeze, freeze_sha256, confirmation_contract_sha256,
                                                    repository_before, started_at, ended_at, verdict,
                                                    reason, safe_observed_version, safe_observed_model,
                                                    safe_observed_build, command_attempted, command_completed)
    record_sha = None
    try:
        record_sha = write_target_binding_confirmation_record_pair(record_path, record)
        # Return and disk stay the same source: recompute over the final
        # moved file.
        record_sha = sha256_file(record_path)
    except PreRecordGateError:
        # MAJOR-3: a destination appeared in the race window between the
        # pre-record gate check and the no-replace commit - this is a
        # pre-record gate failure (exit 1), never a blocked/exit-2 path.
        raise
    except Exception as e:
        # A companion failure may leave an orphan JSON. It is never deleted,
        # never overwritten, and never consumable; the run returns blocked
        # with exit 2.
        write_failure = protect_sensitive_text(str(e))
        verdict = 'blocked'
        record_sha = None
        if not reason:
            reason = 'record-write-failed: %s' % write_failure
        else:
            reason = '%s; record-write-failed: %s' % (reason, write_failure)
    return {
        'verdict': verdict,
        'reason': reason,
        'record_path': record_path,
        'record_sha256': record_sha,
        'command_attempted': command_attempted,
        'command_completed': command_completed,
        'started_at': started_at,
        'ended_at': ended_at,
    }


# =====================================================================
# Section 6: HDC whitelist and process execution (design unit U4)
# =====================================================================

# PS L1277-1297 Assert-ExactCommandParameters whitelist table: 22 operations.
HDC_WHITELIST = {
    'Version': (), 'TupleModel': (), 'TupleBuild': (), 'MkdirStaging': (), 'RemoveStaging': (),
    'StagingProbe': (), 'SendA': (), 'SendB': (), 'InstallA': (), 'InstallB': (),
    'FaultA': (), 'FaultB': (), 'HilogStream': (),
    'BundleDump': ('Bundle',), 'PidOf': ('Bundle',), 'Uninstall': ('Bundle',), 'StartEntry': ('Bundle',),
    'ScreenCap': ('Name',), 'DumpLayout': ('Name',), 'ReceiveScreen': ('Name',), 'ReceiveLayout': ('Name',),
    'ForceStop': ('Bundle', 'Reason'),
}

# MINOR-2: PS is case-insensitive for operation and parameter names; the
# whitelist keeps canonical (PascalCase) names and casefold alias maps route
# any spelling to the canonical name at the entry points.
HDC_OPERATION_ALIASES = {name.casefold(): name for name in HDC_WHITELIST}
HDC_PARAMETER_ALIASES = {name.casefold(): name for name in ('Bundle', 'Name', 'Reason')}


def normalize_hdc_operation(operation):
    """PS-equivalent case-insensitive operation name lookup (MINOR-2):
    casefold -> canonical PascalCase name; unknown names pass through so the
    whitelist rejection keeps the caller's spelling."""
    return HDC_OPERATION_ALIASES.get(str(operation).casefold(), operation)


def normalize_hdc_parameters(parameters):
    """PS-equivalent case-insensitive parameter name lookup (MINOR-2):
    casefold -> canonical name; unknown names pass through so the extra-
    parameter rejection keeps the caller's spelling."""
    return {HDC_PARAMETER_ALIASES.get(str(k).casefold(), k): v for k, v in parameters.items()}

# C17: the DryRun planned-operation list (fixed order, 20 operations).
DRY_RUN_PLAN = [
    ('Version', {}), ('TupleModel', {}), ('TupleBuild', {}),
    ('BundleDump', {'Bundle': BUNDLE_A}), ('BundleDump', {'Bundle': BUNDLE_B}),
    ('PidOf', {'Bundle': BUNDLE_A}), ('PidOf', {'Bundle': BUNDLE_B}),
    ('MkdirStaging', {}), ('SendA', {}), ('SendB', {}),
    ('InstallA', {}), ('InstallB', {}),
    ('StartEntry', {'Bundle': BUNDLE_A}), ('StartEntry', {'Bundle': BUNDLE_B}),
    ('FaultA', {}), ('FaultB', {}),
    ('Uninstall', {'Bundle': BUNDLE_B}), ('Uninstall', {'Bundle': BUNDLE_A}),
    ('RemoveStaging', {}), ('StagingProbe', {}),
]


def assert_exact_command_parameters(operation, parameters):
    """PS L1277-1297 Assert-ExactCommandParameters: unknown operation / extra
    parameter / missing parameter -> rejected. Operation and parameter names
    are case-insensitive, PS-equivalent (MINOR-2); the 22-item whitelist and
    the rejection logic are unchanged."""
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
    """PS L1298-1341 Get-HdcInvocation: whitelisted argv construction. The
    audit (projection/transcript) form always keeps the placeholders
    <PHYS_1_TARGET>/<HAP_A>/<HAP_B>/<RAW_CAPTURE>; the real target never
    enters the projection (C6)."""
    if parameters is None:
        parameters = {}
    operation = normalize_hdc_operation(operation)
    parameters = normalize_hdc_parameters(parameters)
    assert_exact_command_parameters(operation, parameters)
    bundle = str(parameters['Bundle']) if 'Bundle' in parameters else ''
    if bundle and bundle not in (BUNDLE_A, BUNDLE_B):
        raise RuntimeError('command rejected: bundle outside A/B allowlist')
    if 'Name' in parameters and not re.match(r'^scenario-[1-7](?:-[a-z]+)*$', str(parameters['Name'])):
        raise RuntimeError('command rejected: capture name outside fixed scenario paths')
    common = ['-t', '<PHYS_1_TARGET>']
    if operation == 'Version':
        return ['version']
    if operation == 'TupleModel':
        return common + ['shell', 'param', 'get', 'const.product.model']
    if operation == 'TupleBuild':
        return common + ['shell', 'param', 'get', 'const.product.software.version']
    if operation == 'BundleDump':
        return common + ['shell', 'bm', 'dump', '-n', bundle]
    # ADJ-20260808-0001: pidof targets the <bundle>:vpn Extension ability
    # process, never the bundle UI process.
    if operation == 'PidOf':
        return common + ['shell', 'pidof', '%s:vpn' % bundle]
    if operation == 'MkdirStaging':
        return common + ['shell', 'mkdir', '-p', '%s/a' % STAGING, '%s/b' % STAGING, '%s/capture' % STAGING]
    if operation == 'RemoveStaging':
        return common + ['shell', 'rm', '-rf', STAGING]
    if operation == 'StagingProbe':
        return common + ['shell', 'ls', '-ld', STAGING]
    if operation == 'SendA':
        return common + ['file', 'send', '<HAP_A>', '%s/a/a.hap' % STAGING]
    if operation == 'SendB':
        return common + ['file', 'send', '<HAP_B>', '%s/b/b.hap' % STAGING]
    if operation == 'InstallA':
        return common + ['shell', 'bm', 'install', '-p', '%s/a' % STAGING]
    if operation == 'InstallB':
        return common + ['shell', 'bm', 'install', '-p', '%s/b' % STAGING]
    if operation == 'Uninstall':
        return common + ['shell', 'bm', 'uninstall', '-n', bundle]
    if operation == 'StartEntry':
        return common + ['shell', 'aa', 'start', '-a', ABILITY, '-b', bundle, '-m', MODULE]
    if operation == 'ForceStop':
        if str(parameters['Reason']) not in ('exception-cleanup', 'final-cleanup'):
            raise RuntimeError('command rejected: force-stop is cleanup-only')
        return common + ['shell', 'aa', 'force-stop', bundle]
    if operation == 'HilogStream':
        return common + ['shell', 'hilog', '-T', 'E3PhysVpn', '-v', 'year', '-v', 'zone']
    if operation == 'FaultA':
        return common + ['shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', '*%s*' % BUNDLE_A, '-print']
    if operation == 'FaultB':
        return common + ['shell', 'find', '/data/log/faultlog/faultlogger', '-maxdepth', '1', '-type', 'f', '-name', '*%s*' % BUNDLE_B, '-print']
    if operation == 'ScreenCap':
        return common + ['shell', 'uitest', 'screenCap', '-p', '%s/capture/%s.png' % (STAGING, str(parameters['Name']))]
    if operation == 'DumpLayout':
        return common + ['shell', 'uitest', 'dumpLayout', '-p', '%s/capture/%s.json' % (STAGING, str(parameters['Name'])), '-i']
    if operation == 'ReceiveScreen':
        return common + ['file', 'recv', '%s/capture/%s.png' % (STAGING, str(parameters['Name'])), '<RAW_CAPTURE>']
    if operation == 'ReceiveLayout':
        return common + ['file', 'recv', '%s/capture/%s.json' % (STAGING, str(parameters['Name'])), '<RAW_CAPTURE>']
    raise RuntimeError("command rejected: operation '%s' is not allowlisted" % operation)


def get_live_hdc_arguments(audit_arguments, operation, parameters):
    """PS L1342-1358 Get-LiveHdcArguments: placeholder substitution for the
    live execution path only. The audit/transcript form always keeps the
    placeholders."""
    live = []
    for arg in audit_arguments:
        if arg == '<PHYS_1_TARGET>':
            live.append(actual_target)
        elif arg == '<HAP_A>':
            live.append(normalize_path(hap_a))
        elif arg == '<HAP_B>':
            live.append(normalize_path(hap_b))
        elif arg == '<RAW_CAPTURE>':
            extension = '.png' if operation == 'ReceiveScreen' else '.json'
            live.append(os.path.join(raw_path, 'capture-%s%s' % (str(parameters['Name']), extension)))
        else:
            live.append(arg)
    return live


def test_physical_target_token(target):
    """PS L1265-1269 Test-PhysicalTargetToken: non-empty, no leading/trailing
    whitespace, no whitespace/comma/semicolon, not PHYS-1 or a placeholder,
    and never a flag-shaped token (leading '-', M3 flag-injection defense)."""
    if target is None or not target.strip():
        return False
    return (target == target.strip() and not re.search(r'[\s,;]', target)
            and not target.startswith('-')
            and not re.match(r'^(?:PHYS-1|<.+>)$', target, re.IGNORECASE))


def assert_target_environment():
    """PS L1271-1275 Assert-TargetEnvironment: PHYS_1_TARGET is process-scope
    (R17)."""
    target = os.environ.get('PHYS_1_TARGET', '')
    if not test_physical_target_token(target):
        raise RuntimeError('PHYS_1_TARGET must contain exactly one real target token')
    global actual_target
    actual_target = target


class HdcResult:
    """PS Invoke-HdcOperation result object (ExitCode/Stdout/Stderr/Simulated)."""

    __slots__ = ('exit_code', 'stdout', 'stderr', 'simulated')

    def __init__(self, exit_code, stdout, stderr, simulated):
        self.exit_code = exit_code
        self.stdout = stdout
        self.stderr = stderr
        self.simulated = simulated

    def combined_text(self):
        """PS L1599-1601 Get-HdcCombinedText."""
        return (str(self.stdout) + '\n' + str(self.stderr)).strip()


def invoke_hdc_operation(operation, parameters=None, allow_failure=False, timeout_seconds=None):
    """PS L1531-1597 Invoke-HdcOperation. A3/R5: list argv, shell=False,
    explicit UTF-8 decoding with errors='replace'. R11: start_new_session +
    killpg on timeout (process-tree kill). HdcProcessStartCount increments
    only after a real Process.Start succeeds (R19); DryRun returns
    DRY_RUN_NOT_EXECUTED without starting a process. The PS timeout loop's
    Update-CampaignCapture call is a U5 integration point (inert here)."""
    global hdc_logical_call_count, hdc_process_start_count, infrastructure_reason_observed
    if parameters is None:
        parameters = {}
    # MINOR-2: PS is case-insensitive for operation and parameter names;
    # canonicalize at the entry so the transcript and the live-argument
    # substitution see canonical names too.
    operation = normalize_hdc_operation(operation)
    parameters = normalize_hdc_parameters(parameters)
    if timeout_seconds is None:
        timeout_seconds = hdc_timeout_seconds
    audit_arguments = get_hdc_invocation(operation, parameters)
    hdc_logical_call_count += 1
    add_transcript_record('hdc-command', {'operation': operation, 'executable': '<HDC_PATH>',
                                          'arguments': audit_arguments, 'timeout_seconds': timeout_seconds})
    if dry_run:
        result = HdcResult(0, 'DRY_RUN_NOT_EXECUTED', '', True)
    elif live_simulation:
        result = get_simulation_hdc_result(operation, parameters)  # U6 placeholder
    else:
        live_arguments = get_live_hdc_arguments(audit_arguments, operation, parameters)
        try:
            proc = subprocess.Popen([hdc_path] + live_arguments, shell=False,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                    start_new_session=True)
            hdc_process_start_count += 1
            try:
                stdout_bytes, stderr_bytes = proc.communicate(timeout=timeout_seconds)
                result = HdcResult(proc.returncode,
                                   stdout_bytes.decode('utf-8', errors='replace'),
                                   stderr_bytes.decode('utf-8', errors='replace'), False)
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
                except (ProcessLookupError, PermissionError, OSError):
                    pass
                try:
                    stdout_bytes, stderr_bytes = proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    stdout_bytes, stderr_bytes = proc.communicate()
                result = HdcResult(124,
                                   stdout_bytes.decode('utf-8', errors='replace') if stdout_bytes else '',
                                   'HDC timeout after %d seconds' % timeout_seconds, False)
        except Exception as e:
            result = HdcResult(125, '', 'HDC Process.Start exception: %s' % e, False)
    add_transcript_record('hdc-result', {'operation': operation, 'exit_code': result.exit_code,
                                         'stdout': str(result.stdout), 'stderr': str(result.stderr),
                                         'simulated': bool(result.simulated)})
    if result.exit_code in (124, 125) or re.search(r'(?i)\btimeout\b', result.stderr):
        infrastructure_reason_observed = 'hdc-usb-interruption'
    if result.exit_code != 0 and not allow_failure:
        safe_error = protect_sensitive_text(str(result.stderr))
        raise RuntimeError('HDC operation failed: %s exit=%d stderr=%s' % (operation, result.exit_code, safe_error))
    return result


# =====================================================================
# Section 7: Continuous capture state machine (design unit U5)
# =====================================================================


def get_now():
    """PS L245-249 Get-Now: virtual clock under LiveSimulation (R16), else
    aware local now. All capture timestamps (started_at / host_observed_at /
    anchor recorded_at) go through this so simulation fixtures see 2099
    timestamps."""
    if live_simulation:
        return VIRTUAL_BASE + timedelta(seconds=virtual_seconds)
    return datetime.now().astimezone()


def parse_hilog_device_time(text):
    """PS L1764-1791 Parse-HilogDeviceTime. Real HiLog -v year -v zone lines
    are "CST 2026-07-17 16:54:59.204 ..."; already-offset stamps produced by
    simulation/host tools are accepted too. Returns a dict with ok /
    device_observed_at ('o' format, 7 fractional digits) / device_time_zone /
    reason. Unknown zone -> unknown-device-time-zone:<zone> (A10)."""
    if text is None:
        text = ''
    text = str(text)
    m = re.match(r'^(?P<zone>[A-Za-z]{2,5})\s+(?P<stamp>\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?)\b', text)
    if m:
        zone_token = m.group('zone')
        offset = FROZEN_DEVICE_ZONE_MAP.get(zone_token)
        if offset is None:
            return {'ok': False, 'device_observed_at': None, 'device_time_zone': zone_token,
                    'reason': 'unknown-device-time-zone:%s' % zone_token}
        parsed = parse_datetime(m.group('stamp') + offset)
        if parsed is None:
            return {'ok': False, 'device_observed_at': None, 'device_time_zone': zone_token,
                    'reason': 'device-time-parse-failed'}
        return {'ok': True, 'device_observed_at': isoformat_7(parsed), 'device_time_zone': zone_token,
                'reason': None}
    m = re.search(r'(?P<stamp>\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2}))', text)
    if m:
        stamp = m.group('stamp')
        parsed = parse_datetime(stamp)
        if parsed is not None:
            suffix = re.search(r'(Z|[+-]\d{2}:?\d{2})$', stamp)
            zone_token = suffix.group(1) if suffix else 'offset'
            return {'ok': True, 'device_observed_at': isoformat_7(parsed), 'device_time_zone': zone_token,
                    'reason': None}
        return {'ok': False, 'device_observed_at': None, 'device_time_zone': None,
                'reason': 'device-time-parse-failed'}
    return {'ok': False, 'device_observed_at': None, 'device_time_zone': None,
            'reason': 'device-time-missing'}


def add_capture_degradation(capture, component, reason, scenario=-1, category='non-infrastructure',
                            infrastructure_reason=None, mark_continuous_degraded=True):
    """PS L1793-1810 Add-CaptureDegradation: records a deduplicated
    capture-degraded entry (scenario/component/reason triple) and, for
    infrastructure category, sets InfrastructureReasonObserved. The
    continuous capture's Degraded flag is set unless
    mark_continuous_degraded=False (fault-artifact style entries)."""
    global infrastructure_reason_observed
    scenario_number = scenario if scenario >= 0 else (capture['active_scenario'] if capture is not None else 0)
    safe_reason = protect_sensitive_text(reason)
    if category == 'infrastructure' and not infrastructure_reason:
        infrastructure_reason = 'hdc-usb-interruption'
    if category == 'infrastructure' and infrastructure_reason:
        infrastructure_reason_observed = infrastructure_reason
    if mark_continuous_degraded and capture is not None:
        capture['degraded'] = True
    for entry in capture_degraded:
        if entry['scenario'] == scenario_number and entry['component'] == component \
                and entry['reason'] == safe_reason:
            return
    entry = {'scenario': scenario_number, 'component': component, 'reason': safe_reason,
             'category': category, 'infrastructure_reason': infrastructure_reason}
    capture_degraded.append(entry)
    add_transcript_record('capture-degraded', entry)


def new_campaign_capture_state(stdout_path, stderr_path, process):
    """PS L1812-1824 New-CampaignCaptureState: capture state dict with the
    incremental byte cursor (read_offset / pending_bytes / pending_start_byte /
    complete_byte_offset), event list and health fields."""
    return {
        'started_at': isoformat_7(get_now()),
        'process': process,
        'stdout_path': stdout_path,
        'stderr_path': stderr_path,
        'read_offset': 0,
        'pending_bytes': bytearray(),
        'pending_start_byte': 0,
        'complete_byte_offset': 0,
        'line_count': 0,
        'events': [],
        'degraded': False,
        'stopped': False,
        'last_healthy_at': None,
        'last_stderr_bytes': 0,
        'active_scenario': 0,
        'initial_anchor': None,
        'simulated_dead': False,
    }


def test_campaign_capture_health(capture):
    """PS L1826-1850 Test-CampaignCaptureHealth: process early-exit and
    stderr growth are infrastructure (hdc-usb-interruption); local stderr-size
    read errors are non-infrastructure. Returns True when the capture is
    healthy (not degraded)."""
    if capture['stopped']:
        return False
    if capture['simulated_dead']:
        add_capture_degradation(capture, 'raw-hilog-process',
                                'simulated capture process exited during the scenario window',
                                category='infrastructure', infrastructure_reason='hdc-usb-interruption')
    process = capture['process']
    if process is not None:
        try:
            if process.poll() is not None:
                add_capture_degradation(capture, 'raw-hilog-process',
                                        'capture process exited unexpectedly with code %s' % process.returncode,
                                        category='infrastructure', infrastructure_reason='hdc-usb-interruption')
        except Exception as e:
            add_capture_degradation(capture, 'raw-hilog-health', str(e),
                                    category='infrastructure', infrastructure_reason='hdc-usb-interruption')
    if os.path.isfile(capture['stderr_path']):
        try:
            stderr_bytes = os.path.getsize(capture['stderr_path'])
            if stderr_bytes > capture['last_stderr_bytes']:
                add_capture_degradation(capture, 'raw-hilog-stderr',
                                        'capture stderr grew by %d bytes' % (stderr_bytes - capture['last_stderr_bytes']),
                                        category='infrastructure', infrastructure_reason='hdc-usb-interruption')
            capture['last_stderr_bytes'] = stderr_bytes
        except Exception as e:
            add_capture_degradation(capture, 'raw-hilog-stderr-read', str(e),
                                    category='non-infrastructure', mark_continuous_degraded=True)
    if not capture['degraded']:
        capture['last_healthy_at'] = isoformat_7(get_now())
        return True
    return False


def update_campaign_capture(capture):
    """PS L1885-1962 Update-CampaignCapture: incremental byte reader (R20).
    The stdout file is opened per poll (matching PS FileStream Open/Dispose
    per call); the byte cursor (read_offset) and partial-line buffer
    (pending_bytes) are kept in capture state so no byte is re-read or lost.
    Lines are split on \\n with a trailing \\r stripped, decoded with strict
    UTF-8 (invalid bytes -> raw-hilog-dropped-line, non-infrastructure), and
    each complete line becomes an event record with device/host timestamps."""
    test_campaign_capture_health(capture)
    if not os.path.isfile(capture['stdout_path']):
        add_capture_degradation(capture, 'raw-hilog-read', 'capture stdout file is missing',
                                category='non-infrastructure')
        return
    try:
        with open(capture['stdout_path'], 'rb') as stream:
            length = os.fstat(stream.fileno()).st_size
            if length < capture['read_offset']:
                add_capture_degradation(capture, 'raw-hilog-dropped-line',
                                        'capture stdout was truncated behind the incremental byte cursor',
                                        category='non-infrastructure')
                return
            available = length - capture['read_offset']
            stream.seek(capture['read_offset'])
            new_bytes = stream.read(available)
            read_total = len(new_bytes)
            if read_total != available:
                add_capture_degradation(capture, 'raw-hilog-dropped-line',
                                        'incremental read returned %d of %d bytes' % (read_total, available),
                                        category='non-infrastructure')
                if read_total == 0:
                    return
                new_bytes = new_bytes[:read_total]
            new_read_offset = capture['read_offset'] + read_total
            base_offset = capture['pending_start_byte'] if len(capture['pending_bytes']) > 0 else capture['read_offset']
            combined = bytes(capture['pending_bytes']) + new_bytes
            segment_start = 0
            for index, byte in enumerate(combined):
                if byte != 10:  # \n
                    continue
                segment_length = index - segment_start
                if segment_length > 0 and combined[index - 1] == 13:  # strip \r
                    segment_length -= 1
                line_bytes = combined[segment_start:segment_start + segment_length]
                try:
                    line = line_bytes.decode('utf-8', errors='strict')
                except UnicodeDecodeError:
                    add_capture_degradation(capture, 'raw-hilog-dropped-line',
                                            'invalid UTF-8 at byte %d' % (base_offset + segment_start),
                                            category='non-infrastructure')
                    line = ''
                observed_at = isoformat_7(get_now())
                parsed_time = parse_hilog_device_time(line)
                if not parsed_time['ok']:
                    add_capture_degradation(capture, 'raw-hilog-time-parse',
                                            'line %d %s' % (capture['line_count'] + 1, parsed_time['reason']),
                                            category='non-infrastructure')
                capture['line_count'] += 1
                capture['complete_byte_offset'] = base_offset + index + 1
                capture['events'].append({
                    'line_index': capture['line_count'],
                    'raw_byte_start': base_offset + segment_start,
                    'raw_byte_end': capture['complete_byte_offset'],
                    'text': line,
                    'device_observed_at': parsed_time['device_observed_at'],
                    'device_time_zone': parsed_time['device_time_zone'],
                    'host_observed_at': observed_at,
                })
                segment_start = index + 1
            if segment_start < len(combined):
                capture['pending_bytes'] = bytearray(combined[segment_start:])
                capture['pending_start_byte'] = base_offset + segment_start
            else:
                capture['pending_bytes'] = bytearray()
                capture['pending_start_byte'] = new_read_offset
            capture['read_offset'] = new_read_offset
    except Exception as e:
        add_capture_degradation(capture, 'raw-hilog-read', str(e), category='non-infrastructure')
    test_campaign_capture_health(capture)


def start_campaign_hilog_capture(freeze=None):
    """PS L1964-1994 Start-CampaignHilogCapture (A14): Popen with list argv,
    shell=False, stdout/stderr redirected to raw-hilog-campaign.log /
    .stderr.log, start_new_session=True (R11 process-tree kill). Under
    LiveSimulation the fixture's capture_initial_lines are written instead of
    starting a process. Start failure writes empty files and records a
    raw-hilog-start infrastructure degradation. The freeze parameter is kept
    for placeholder-signature stability (PS reads script globals)."""
    global hdc_logical_call_count, hdc_process_start_count, campaign_capture
    stdout_path = os.path.join(raw_path, 'raw-hilog-campaign.log')
    stderr_path = os.path.join(raw_path, 'raw-hilog-campaign.stderr.log')
    audit_arguments = get_hdc_invocation('HilogStream')
    hdc_logical_call_count += 1
    add_transcript_record('hdc-capture-start', {'scope': 'continuous-campaign', 'operation': 'HilogStream',
                                                 'arguments': audit_arguments,
                                                 'independent_raw_reference': 'RAW-HILOG-CAMPAIGN'})
    capture_process = None
    if live_simulation:
        initial_lines = get_optional_property(simulation, 'capture_initial_lines', []) or []
        initial_text = ''
        if initial_lines:
            initial_text = '\n'.join(str(line) for line in initial_lines) + '\n'
        write_text_utf8_no_bom(stdout_path, initial_text)
        write_text_utf8_no_bom(stderr_path, '')
    else:
        live_arguments = get_live_hdc_arguments(audit_arguments, 'HilogStream', {})
        try:
            stdout_file = open(stdout_path, 'wb')
            stderr_file = open(stderr_path, 'wb')
            try:
                capture_process = subprocess.Popen([hdc_path] + live_arguments, shell=False,
                                                   stdout=stdout_file, stderr=stderr_file,
                                                   start_new_session=True)
            finally:
                stdout_file.close()
                stderr_file.close()
            if capture_process is None:
                raise RuntimeError('Start-Process returned no process')
            hdc_process_start_count += 1
        except Exception:
            write_text_utf8_no_bom(stdout_path, '')
            write_text_utf8_no_bom(stderr_path, '')
            capture_process = None
    capture = new_campaign_capture_state(stdout_path, stderr_path, capture_process)
    if not live_simulation and capture_process is None:
        add_capture_degradation(capture, 'raw-hilog-start', 'unable to start continuous campaign capture',
                                category='infrastructure', infrastructure_reason='hdc-usb-interruption')
    campaign_capture = capture
    return capture


def initialize_campaign_capture_anchor(capture):
    """PS L1995-2010 Initialize-CampaignCaptureAnchor: drains the capture to a
    stable byte offset (two consecutive stable polls in live mode; a single
    update in simulation) and records the initial anchor (complete line count /
    byte offsets / partial remainder / event count) as the scenario window
    base for S1-S7."""
    if live_simulation:
        update_campaign_capture(capture)
    else:
        stable_polls = 0
        last_offset = -1
        while stable_polls < 2:
            time.sleep(0.25)
            update_campaign_capture(capture)
            if capture['read_offset'] == last_offset:
                stable_polls += 1
            else:
                stable_polls = 0
                last_offset = capture['read_offset']
    capture['initial_anchor'] = {
        'recorded_at': isoformat_7(get_now()),
        'complete_line_count': capture['line_count'],
        'complete_byte_offset': capture['complete_byte_offset'],
        'stream_byte_offset': capture['read_offset'],
        'partial_remainder_bytes': len(capture['pending_bytes']),
        'event_count': len(capture['events']),
    }
    add_transcript_record('hilog-initial-anchor', capture['initial_anchor'])


def get_scenario_window_events(capture, anchor_byte, action_prompt_at, observed_through=None):
    """PS L2078-2104 Get-ScenarioWindowEvents: byte-anchor excludes the
    historical buffer; device time bounds the action window with the frozen
    device-clock skew tolerance only (earliest = prompt - 3s). observed_through
    defaults to action_prompt_at + scenario_window_seconds (WINDOW_SECONDS).
    Returns projected event dicts with offset_seconds relative to the prompt."""
    if observed_through is None:
        observed_through = action_prompt_at + timedelta(seconds=WINDOW_SECONDS)
    earliest_device_time = action_prompt_at - timedelta(seconds=DEVICE_CLOCK_SKEW_TOLERANCE_SECONDS)
    result = []
    for event in capture['events']:
        if event['raw_byte_start'] < anchor_byte:
            continue
        if not event['device_observed_at'] or not str(event['device_observed_at']).strip():
            continue
        device_time = parse_datetime(event['device_observed_at'])
        if device_time is None:
            continue
        if device_time < earliest_device_time or device_time > observed_through:
            continue
        result.append({
            'line_index': event['line_index'],
            'raw_byte_start': event['raw_byte_start'],
            'raw_byte_end': event['raw_byte_end'],
            'offset_seconds': (device_time - action_prompt_at).total_seconds(),
            'text': event['text'],
            'device_observed_at': event['device_observed_at'],
            'device_time_zone': event['device_time_zone'],
            'host_observed_at': event['host_observed_at'],
        })
    return result


def get_scenario_context_events(capture, observed_through=None, anchor_byte=None, action_at=None):
    """PS L2158-2166 Get-ScenarioContextEvents: update the capture, then
    project the scenario window. Defaults: observed_through = now,
    anchor_byte = current read offset, action_at = capture started_at."""
    update_campaign_capture(capture)
    if observed_through is None:
        observed_through = get_now()
    if anchor_byte is None:
        anchor_byte = capture['read_offset']
    if action_at is None:
        action_at = parse_datetime(capture['started_at'])
    return get_scenario_window_events(capture, anchor_byte, action_at, observed_through)


def stop_campaign_hilog_capture(capture):
    """PS L2106-2134 Stop-CampaignHilogCapture: drain the capture (3 polls),
    kill the process tree (killpg, R11), finalize the last partial line, and
    record the raw-hilog artifacts (stdout/stderr) with sha256 + bytes for
    the collection manifest (U7)."""
    if capture['stopped']:
        return
    for _ in range(3):
        if not live_simulation:
            time.sleep(0.2)
        update_campaign_capture(capture)
    capture['stopped'] = True
    process = capture['process']
    if process is not None:
        try:
            if process.poll() is None:
                try:
                    os.killpg(os.getpgid(process.pid), signal.SIGKILL)
                except (ProcessLookupError, PermissionError, OSError):
                    process.kill()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                raise RuntimeError('hilog capture process did not exit within 5 seconds')
        except Exception as e:
            add_capture_degradation(capture, 'raw-hilog-stop', str(e), category='non-infrastructure')
    update_campaign_capture(capture)
    if len(capture['pending_bytes']) > 0:
        add_capture_degradation(capture, 'raw-hilog-incomplete-final-line',
                                'capture ended with %d uncompleted bytes' % len(capture['pending_bytes']))
    if process is not None:
        try:
            process.wait(timeout=1)
        except Exception:
            pass
    for path, reference in ((capture['stdout_path'], 'RAW-HILOG-CAMPAIGN'),
                            (capture['stderr_path'], 'RAW-HILOG-CAMPAIGN-STDERR')):
        if os.path.isfile(path):
            raw_hilog_artifacts.append({'scenario': 0, 'reference': reference, 'path': path,
                                        'sha256': sha256_file(path), 'bytes': os.path.getsize(path)})


# =====================================================================
# Section 8: E3 event parsing and condition evaluation (design unit U5)
# =====================================================================


def get_e3_event_info(event):
    """PS L2236-2245 Get-E3EventInfo: extracts the marker token (uppercase
    token immediately before a |), bundle= and requestId= fields from an
    event's text. Accepts an event dict (with 'text') or a raw line string."""
    if isinstance(event, dict):
        text = str(event.get('text', ''))
    else:
        text = str(event)
    marker = None
    m = re.search(r'(?:^|\s)([A-Z][A-Z0-9_]+)\|', text)
    if m:
        marker = m.group(1)
    bundle = None
    m = re.search(r'(?:^|\|)bundle=([^|\s]+)', text)
    if m:
        bundle = m.group(1)
    request_id = None
    m = re.search(r'(?:^|\|)requestId=([^|\s]+)', text)
    if m:
        request_id = m.group(1)
    return {'marker': marker, 'bundle': bundle, 'request_id': request_id, 'text': text, 'event': event}


def get_bundle_from_hilog_tag(text):
    """ADJ-20260814-0002 C6: extract the owning bundle from a hilog line's
    tag, accepting every process tag form of the E3 bundles: the entry
    process (cn.alfadb.netbird.e3physvpna), the :vpn Extension subprocess in
    its truncated form (.alfadb.netbird.e3physvpna:vpn - hilog drops the cn.
    prefix) and its full form (cn.alfadb.netbird.e3physvpna:vpn). Only the
    tag path component is scanned, never the message body; never filters by
    pid. Returns the bundle name or None."""
    text = str(text)
    m = re.search(r'(?<=/)([A-Za-z0-9_.-]*alfadb\.netbird\.e3physvpn[ab])(?::vpn)?(?=/)', text)
    if not m:
        return None
    candidate = m.group(1).lstrip('.')
    if not candidate.startswith('cn.'):
        candidate = 'cn.' + candidate
    return candidate


def test_line_correlated(text, request_id, bundle):
    """ADJ-20260814-0002 C6: True when the line's requestId field matches
    the target request, or the line is a :vpn Extension subprocess line
    (requestId=missing - the subprocess does not carry the UI requestId)
    whose process tag identifies the bundle in any tag form. The :vpn
    create/destroy terminal markers are correlated through the bundle's
    process tag instead of the requestId field; never by pid."""
    text = str(text)
    if re.search(r'requestId=%s(\||\s|$)' % re.escape(str(request_id)), text):
        return True
    return (re.search(r'requestId=missing(\||\s|$)', text)
            and get_bundle_from_hilog_tag(text) == bundle)


def get_rejection_error_code(text):
    """PS L2248-2263 Get-RejectionErrorCode (ADJ-20260808-0002 C6): extracts
    a numeric BusinessError code from a rejection event. Boundary-rigorous: a
    standalone code= name=value pair whose numeric value ends at a field
    boundary (| , ; whitespace EOL); prose quoted inside message=... is never
    mis-parsed. Returns None when no such code is present."""
    text = str(text)
    m = re.search(r'(?:^|\|)code=(\d+)(?=\||\s|$)', text)
    if m:
        return int(m.group(1))
    m = re.search(r'(?:^|[|,;=])\s*code=(\d+)\s*(?=,|;|$|\s)', text)
    if m:
        return int(m.group(1))
    return None


def test_unique_start(events, bundle, request_id=None):
    """PS L2265-2280 Test-UniqueStartCondition. request_id is optional: when
    provided, the observed UI_START requestId must match it. Returns a status
    dict (pass / pending / invalid) with reason; the pass result carries the
    verified request_id and bundle."""
    infos = [get_e3_event_info(e) for e in events]
    forbidden = [i for i in infos if i['marker'] in ('UI_STOP', 'UI_STOP_SKIPPED')]
    if forbidden:
        return {'status': 'invalid', 'reason': 'unexpected-%s-during-start-step' % forbidden[0]['marker']}
    skipped = [i for i in infos if i['marker'] == 'UI_START_SKIPPED']
    if skipped:
        return {'status': 'invalid', 'reason': 'UI_START_SKIPPED'}
    starts = [i for i in infos if i['marker'] == 'UI_START']
    if not starts:
        return {'status': 'pending', 'reason': 'UI_START-missing'}
    if len(starts) != 1:
        return {'status': 'invalid', 'reason': 'expected-one-UI_START-observed-%d' % len(starts)}
    if starts[0]['bundle'] != bundle:
        return {'status': 'invalid', 'reason': 'UI_START-wrong-bundle:%s' % starts[0]['bundle']}
    if not starts[0]['request_id'] or not str(starts[0]['request_id']).strip() \
            or starts[0]['request_id'] == 'missing':
        return {'status': 'invalid', 'reason': 'UI_START-requestId-missing'}
    if request_id is not None and starts[0]['request_id'] != request_id:
        return {'status': 'invalid', 'reason': 'UI_START-wrong-requestId:%s' % starts[0]['request_id']}
    return {'status': 'pass', 'reason': 'unique-UI_START', 'request_id': starts[0]['request_id'],
            'bundle': bundle}


def test_correlated_marker(events, bundle, request_id, marker):
    """PS L2593-2607 Test-CorrelatedMarker: the marker text must be present,
    the requestId must end at a field boundary, and an explicit bundle field
    must equal the target bundle. ADJ-20260814-0002 C6: :vpn subprocess
    lines (requestId=missing) are correlated through the bundle's process
    tag in any tag form instead of the requestId field."""
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        if marker not in text:
            continue
        if not test_line_correlated(text, request_id, bundle):
            continue
        m = re.search(r'bundle=([^|\s]+)', text)
        if m and m.group(1) != bundle:
            continue
        return True
    return False


def get_stop_request_from_events(events, expected_bundle=None):
    """PS L2609-2645 Get-StopRequestFromEvents: strict whole-marker tokens only
    (UI_STOP | STOP_PROMISE_RESOLVED | STOP_PROMISE_REJECTED) with real token
    boundaries on both sides; UI_STOP_SKIPPED / LATE / SESSION_RELEASED /
    destroy-only are ignored. Returns {request_id, bundle} only when exactly
    one unique requestId remains, else None."""
    candidates = {}
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        m = re.search(r'(?:^|\s)UI_STOP\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)', text)
        if not m:
            m = re.search(r'(?:^|\s)STOP_PROMISE_RESOLVED\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)', text)
        if not m:
            m = re.search(r'(?:^|\s)STOP_PROMISE_REJECTED\|bundle=([^|\s]+)\|requestId=([^|\s]+)(?:\||\s*$)', text)
        if not m:
            continue
        bundle = m.group(1)
        request_id = m.group(2)
        if request_id == 'missing':
            continue
        if expected_bundle is not None:
            if not bundle or bundle != expected_bundle:
                continue
        if request_id not in candidates:
            candidates[request_id] = bundle
    if len(candidates) != 1:
        return None
    only_key = next(iter(candidates))
    return {'request_id': only_key, 'bundle': candidates[only_key]}


def get_stop_assessment(events, bundle, request_id=None):
    """PS Get-StopRequestFromEvents L2609-2645 (the PS source has no
    Get-StopAssessment; this is the stop-request inference consumed by
    Get-DestroyAssessment / Test-StrictFallbackPrerequisites). With an
    explicit request_id it verifies a correlated stop marker instead of
    inferring. Returns {result, reason, request_id, bundle}."""
    if request_id is not None and str(request_id).strip() and str(request_id) != 'missing':
        for marker in ('UI_STOP', 'STOP_PROMISE_RESOLVED', 'STOP_PROMISE_REJECTED'):
            if test_correlated_marker(events, bundle, request_id, marker):
                return {'result': 'pass', 'reason': 'unique-stop-request',
                        'request_id': request_id, 'bundle': bundle}
        return {'result': 'blocked', 'reason': 'stop-request-marker-missing',
                'request_id': request_id, 'bundle': bundle}
    inferred = get_stop_request_from_events(events, expected_bundle=bundle)
    if inferred is None:
        return {'result': 'blocked', 'reason': 'stop-request-unique-missing',
                'request_id': None, 'bundle': bundle}
    return {'result': 'pass', 'reason': 'unique-stop-request',
            'request_id': inferred['request_id'], 'bundle': inferred['bundle']}


def test_s5_post_destroy_still_open(events, bundle, request_id):
    """PS L3082-3096 Test-S5PostDestroyStillOpen: an explicit FD_STILL_OPEN
    marker on a post-destroy-phase snapshot or destroy terminal is a leaked fd
    (hard fail); pre-destroy open snapshots never count. Same-bundle/request
    marker only; ADJ-20260814-0002 C6: :vpn subprocess lines (requestId=
    missing) are correlated through the bundle's process tag instead."""
    if request_id is None or str(request_id) == 'missing':
        return False
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        if 'FD_STILL_OPEN' not in text:
            continue
        if not test_line_correlated(text, request_id, bundle):
            continue
        m = re.search(r'bundle=([^|\s]+)', text)
        if m and m.group(1) != bundle:
            continue
        if re.search(r'VPN_DESTROY_RESOLVED|VPN_DESTROY_REJECTED|phase=post-destroy', text):
            return True
    return False


def get_destroy_assessment(events, bundle, request_id=None):
    """PS L2647-2686 Get-DestroyAssessment: callback terminal + post-destroy
    fd snapshot on the same request. requestId is inferred from a unique stop
    request when not supplied. FD_STILL_OPEN is a hard fail; missing terminal /
    snapshot / onDestroy pieces stay blocked."""
    effective_request_id = request_id
    if effective_request_id is None or not str(effective_request_id).strip() \
            or str(effective_request_id) == 'missing':
        inferred = get_stop_request_from_events(events, expected_bundle=bundle)
        if inferred is not None:
            effective_request_id = inferred['request_id']
            if inferred['bundle']:
                bundle = inferred['bundle']
    if effective_request_id is None or not str(effective_request_id).strip() \
            or str(effective_request_id) == 'missing':
        has_evidence = any(re.search(r'UI_STOP|STOP_PROMISE_|STOP_SESSION_RELEASED|VPN_ONDESTROY|VPN_DESTROY_|VPN_FD_SNAPSHOT',
                                     str(e['text']) if isinstance(e, dict) else str(e)) for e in events)
        reason = 'destroy-requestId-unresolved' if has_evidence else 'no-destroy-or-stop-marker-observed'
        return {'result': 'blocked', 'reason': reason}
    if test_s5_post_destroy_still_open(events, bundle, effective_request_id):
        return {'result': 'fail', 'reason': 'fd-still-open-after-destroy'}
    terminal = (test_correlated_marker(events, bundle, effective_request_id, 'VPN_DESTROY_RESOLVED')
                or test_correlated_marker(events, bundle, effective_request_id, 'VPN_DESTROY_REJECTED'))
    snapshot = (test_correlated_marker(events, bundle, effective_request_id, 'VPN_FD_SNAPSHOT')
                and any(test_line_correlated(str(e['text']) if isinstance(e, dict) else str(e),
                                             effective_request_id, bundle)
                        and re.search(r'phase=post-destroy-(resolved|rejected)',
                                      str(e['text']) if isinstance(e, dict) else str(e))
                        for e in events))
    if not terminal and not snapshot:
        return {'result': 'blocked', 'reason': 'destroy-terminal-or-post-snapshot-missing'}
    if not terminal:
        return {'result': 'blocked', 'reason': 'destroy-terminal-missing'}
    if not snapshot:
        return {'result': 'blocked', 'reason': 'post-destroy-snapshot-missing'}
    correlated = [str(e['text']) if isinstance(e, dict) else str(e) for e in events
                  if test_line_correlated(str(e['text']) if isinstance(e, dict) else str(e),
                                          effective_request_id, bundle)]
    combined = '\n'.join(correlated)
    if 'FD_STILL_OPEN' in combined:
        return {'result': 'fail', 'reason': 'FD_STILL_OPEN'}
    if 'FD_STATE_UNCONFIRMED' in combined:
        return {'result': 'blocked', 'reason': 'FD_STATE_UNCONFIRMED'}
    if not re.search(r'FD_CLOSED_CONFIRMED|FD_NOT_OPEN_AFTER_DESTROY', combined):
        return {'result': 'blocked', 'reason': 'destroy-fd-decision-missing'}
    if not test_correlated_marker(events, bundle, effective_request_id, 'VPN_ONDESTROY'):
        return {'result': 'blocked', 'reason': 'destroy-ondestroy-missing'}
    return {'result': 'pass', 'reason': 'terminal-and-post-destroy-snapshot-confirmed'}


def get_deny_assessment(events, bundle, request_id=None, deny_screenshot=False, full_window_observed=False):
    """PS L2688-2723 Get-DenyAssessment: any B-bundle create marker (or a
    create on a B UI_START requestId) fails; a correlated START_PROMISE_REJECTED
    / VPN_CREATE_REJECTED on the B request passes; otherwise a deny screenshot
    plus a fully observed window passes, else blocked."""
    b_request_ids = set()
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        m = re.search(r'UI_START\|bundle=%s\|requestId=([^|\s]+)' % re.escape(bundle), text)
        if m and m.group(1) != 'missing':
            b_request_ids.add(m.group(1))
    create_markers = ('VPN_ONCREATE', 'VPN_CREATE_BEGIN', 'VPN_CREATE_RESOLVED', 'CREATE_ACCEPTED')
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        has_create = any(marker in text for marker in create_markers)
        if not has_create:
            continue
        is_bundle = re.search(r'bundle=%s(\||\s|$)' % re.escape(bundle), text)
        event_request_id = None
        m = re.search(r'requestId=([^|\s]+)', text)
        if m:
            event_request_id = m.group(1)
        if is_bundle or (event_request_id is not None and event_request_id in b_request_ids):
            return {'result': 'fail', 'reason': 'deny-created-B-vpn'}
    if request_id is None or not str(request_id).strip():
        return {'result': 'blocked', 'reason': 'B-requestId-missing'}
    reject = (test_correlated_marker(events, bundle, request_id, 'START_PROMISE_REJECTED')
              or test_correlated_marker(events, bundle, request_id, 'VPN_CREATE_REJECTED'))
    if reject:
        return {'result': 'pass', 'reason': 'observable-B-request-rejection'}
    if deny_screenshot and full_window_observed:
        return {'result': 'pass', 'reason': 'deny-layout-and-full-window-without-B-create'}
    return {'result': 'blocked', 'reason': 'deny-proof-incomplete'}


def get_bundle_dump_assessment(result, bundle):
    """PS L1622-1640 Get-BundleDumpAssessment (probe classification helper):
    dump confirms presence only; unavailable / permission / absence / uncertain
    are blocked, never functional_fail; exit 124/125 is infrastructure."""
    exit_code = int(result.exit_code)
    text = result.combined_text()
    if exit_code in (124, 125):
        return {'status': 'infrastructure', 'reason': 'dump-exit-%d' % exit_code}
    if exit_code != 0:
        return {'status': 'blocked', 'reason': 'bundle-dump-unavailable'}
    if not text.strip():
        return {'status': 'blocked', 'reason': 'bundle-dump-empty'}
    if re.search(r'(?i)permission denied|access denied|not permitted|cannot access', text):
        return {'status': 'blocked', 'reason': 'bundle-dump-permission'}
    if re.search(r'(?i)failed to get information|not exist|not found|no such file', text):
        return {'status': 'blocked', 'reason': 'bundle-dump-absent'}
    if bundle not in text:
        return {'status': 'blocked', 'reason': 'bundle-dump-bundle-not-listed'}
    return {'status': 'pass', 'reason': 'bundle-dump-present'}


def get_process_probe_status(pid_result, dump_result=None, bundle=None):
    """PS L2725-2765 Get-ProcessProbeStatus. The PS signature is
    (PidResult, DumpResult, Bundle); the placeholder's single-argument form is
    extended with the two required PS arguments. PidOf decides process state
    only; BundleDump decides bundle_present only via Get-BundleDumpAssessment;
    any non-pass dump never accumulates as absent."""
    pid_exit = int(pid_result.exit_code)
    if pid_exit in (124, 125):
        return {'status': 'error', 'bundle_present': False, 'detail': 'pid-exit-infrastructure'}
    if str(pid_result.stderr).strip():
        return {'status': 'unknown', 'bundle_present': False, 'detail': 'pid-stderr'}
    pid_out = str(pid_result.stdout)
    pid_blank = not pid_out.strip()
    process_status = None
    if not pid_blank:
        if pid_exit == 0:
            process_status = 'present'
        else:
            return {'status': 'unknown', 'bundle_present': False, 'detail': 'pid-exit-%d-with-output' % pid_exit}
    else:
        if pid_exit in (0, 1):
            process_status = 'absent'
        else:
            return {'status': 'unknown', 'bundle_present': False, 'detail': 'pid-exit-%d' % pid_exit}
    if dump_result is None or bundle is None:
        return {'status': process_status, 'bundle_present': None, 'detail': 'dump-not-assessed'}
    dump_exit = int(dump_result.exit_code)
    if dump_exit in (124, 125):
        return {'status': 'error', 'bundle_present': False, 'detail': 'dump-infrastructure'}
    if str(dump_result.stderr).strip():
        return {'status': 'unknown', 'bundle_present': False, 'detail': 'dump-stderr'}
    if dump_exit != 0:
        return {'status': 'unknown', 'bundle_present': False, 'detail': 'dump-exit-%d' % dump_exit}
    dump_assessment = get_bundle_dump_assessment(dump_result, bundle)
    if dump_assessment['status'] == 'infrastructure':
        return {'status': 'error', 'bundle_present': False, 'detail': dump_assessment['reason']}
    if dump_assessment['status'] != 'pass':
        return {'status': 'unknown', 'bundle_present': False, 'detail': dump_assessment['reason']}
    return {'status': process_status, 'bundle_present': True, 'detail': None}


def test_strict_fallback_prerequisites(events, bundle, request_id=None):
    """PS L2985-3026 Test-StrictFallbackPrerequisites: strict-process-boundary
    marker gate for S3/S7 - a unique legal stop for the same bundle plus
    onDestroy plus destroy-begin (or pre-destroy snapshot). VPN_DESTROY_ISSUED
    never counts as begin/terminal."""
    stop = None
    if request_id is not None and str(request_id).strip():
        candidate = get_stop_request_from_events(events, expected_bundle=bundle)
        if candidate is None or candidate['request_id'] != str(request_id):
            return {'met': False, 'stop': None, 'request_id': str(request_id),
                    'reason': 'strict-fallback-stop-unique-missing'}
        if not (test_correlated_marker(events, bundle, str(request_id), 'UI_STOP')
                or test_correlated_marker(events, bundle, str(request_id), 'STOP_PROMISE_RESOLVED')):
            return {'met': False, 'stop': candidate, 'request_id': str(request_id),
                    'reason': 'strict-fallback-stop-marker-missing'}
        stop = candidate
    else:
        stop = get_stop_request_from_events(events, expected_bundle=bundle)
        if stop is None:
            return {'met': False, 'stop': None, 'request_id': None,
                    'reason': 'strict-fallback-stop-unique-missing'}
        rid = stop['request_id']
        if not (test_correlated_marker(events, bundle, rid, 'UI_STOP')
                or test_correlated_marker(events, bundle, rid, 'STOP_PROMISE_RESOLVED')):
            return {'met': False, 'stop': stop, 'request_id': rid,
                    'reason': 'strict-fallback-stop-marker-missing'}
    rid = stop['request_id']
    if not test_correlated_marker(events, bundle, rid, 'VPN_ONDESTROY'):
        return {'met': False, 'stop': stop, 'request_id': rid,
                'reason': 'strict-fallback-ondestroy-missing'}
    pre_snapshot = any(
        test_line_correlated(str(e['text']) if isinstance(e, dict) else str(e), rid, bundle)
        and 'VPN_FD_SNAPSHOT' in (str(e['text']) if isinstance(e, dict) else str(e))
        and 'phase=pre-destroy' in (str(e['text']) if isinstance(e, dict) else str(e))
        for e in events)
    begin = test_correlated_marker(events, bundle, rid, 'VPN_DESTROY_BEGIN') or pre_snapshot
    if not begin:
        return {'met': False, 'stop': stop, 'request_id': rid,
                'reason': 'strict-fallback-destroy-begin-missing'}
    return {'met': True, 'stop': stop, 'request_id': rid, 'reason': None}


def get_vpn_final_state(events, bundle, request_id=None, probe_state=None, require_bundle_present=False,
                        required_count=2, spacing_seconds=3.0):
    """PS L3028-3060 Get-VpnFinalState: priority 1 is the callback terminal +
    post-destroy fd snapshot (FD_STILL_OPEN is a hard fail that can never fall
    back); priority 2 is the strict-process-boundary fallback (S3/S7) where
    every missing or uncertain piece stays blocked."""
    callback = get_destroy_assessment(events, bundle, request_id)
    if callback['result'] == 'fail' and callback['reason'] in ('FD_STILL_OPEN', 'fd-still-open-after-destroy'):
        return {'result': 'fail', 'reason': callback['reason'], 'terminal_mode': 'callback-post-fd',
                'callback': callback, 'strict': None}
    if callback['result'] == 'pass':
        return {'result': 'pass', 'reason': 'terminal-and-post-destroy-snapshot-confirmed',
                'terminal_mode': 'callback-post-fd', 'callback': callback, 'strict': None}
    strict = test_strict_fallback_prerequisites(events, bundle, request_id)
    if not strict['met']:
        return {'result': 'blocked', 'reason': strict['reason'], 'terminal_mode': 'strict-process-boundary',
                'callback': callback, 'strict': strict}
    if probe_state is None or not probe_state.get('started'):
        return {'result': 'blocked', 'reason': 'strict-fallback-probes-not-started',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    if probe_state.get('aborted'):
        return {'result': 'blocked', 'reason': 'strict-fallback-probe-unknown-or-error',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    if not probe_state.get('terminal'):
        return {'result': 'blocked', 'reason': 'strict-fallback-process-absent-insufficient',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    absent_probes = [p for p in probe_state.get('probes', []) if str(p.get('status')) == 'absent']
    if len(absent_probes) < required_count:
        return {'result': 'blocked', 'reason': 'strict-fallback-process-absent-insufficient',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    last_absent = absent_probes[-1]
    previous_absent = absent_probes[-2]
    last_at = parse_datetime(str(last_absent['time']))
    previous_at = parse_datetime(str(previous_absent['time']))
    if last_at is None or previous_at is None:
        return {'result': 'blocked', 'reason': 'strict-fallback-probe-spacing-insufficient',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    spacing = (last_at - previous_at).total_seconds()
    if spacing < (spacing_seconds - 0.001):
        return {'result': 'blocked', 'reason': 'strict-fallback-probe-spacing-insufficient',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    if require_bundle_present and not probe_state.get('bundle_present'):
        return {'result': 'blocked', 'reason': 'strict-fallback-bundle-absent',
                'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}
    return {'result': 'pass', 'reason': 'strict-process-boundary-terminal',
            'terminal_mode': 'strict-process-boundary', 'callback': callback, 'strict': strict}


def test_process_absent_evidence(probe_state, required_count=2, spacing_seconds=3.0):
    """PS L3098-3131 Test-ProcessAbsentEvidence. The placeholder signature
    (events, bundle) is corrected to the PS signature (ProbeState,
    RequiredCount, SpacingSeconds): the runner never trusts execution-time
    Wait/Terminal flags alone and re-checks the recorded probe timestamps - the
    last RequiredCount probes must be consecutive absent with first-to-last
    spacing >= SpacingSeconds."""
    if probe_state is None or not probe_state.get('started'):
        return {'met': False, 'reason': 'probes-not-started', 'spacing_seconds': None}
    if probe_state.get('aborted'):
        return {'met': False, 'reason': 'probe-unknown-or-error', 'spacing_seconds': None}
    probes = probe_state.get('probes', [])
    if len(probes) < required_count:
        return {'met': False, 'reason': 'process-absent-probes-insufficient', 'spacing_seconds': None}
    tail = probes[-required_count:]
    for probe in tail:
        if str(probe.get('status')) != 'absent':
            return {'met': False, 'reason': 'process-absent-probes-insufficient', 'spacing_seconds': None}
    first_at = parse_datetime(str(tail[0]['time']))
    last_at = parse_datetime(str(tail[-1]['time']))
    if first_at is None or last_at is None:
        return {'met': False, 'reason': 'probe-spacing-insufficient', 'spacing_seconds': None}
    measured = (last_at - first_at).total_seconds()
    if measured < (spacing_seconds - 0.001):
        return {'met': False, 'reason': 'probe-spacing-insufficient', 'spacing_seconds': measured}
    return {'met': True, 'reason': None, 'spacing_seconds': measured}


def test_post_create_open(events, bundle, request_id):
    """PS L3133-3152 Test-PostCreateOpen: clean reactivation proof - the fresh
    request shows CREATE_ACCEPTED plus a post-create fd snapshot with open=true.
    Exact field-boundary match only: |open=true| counts, reopen=true never does.
    Same-bundle/request marker only; ADJ-20260814-0002 C6: :vpn subprocess
    lines (requestId=missing) are correlated through the bundle's process tag
    instead."""
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        if not test_line_correlated(text, request_id, bundle):
            continue
        if 'VPN_FD_SNAPSHOT' not in text:
            continue
        if 'phase=post-create' not in text:
            continue
        if not re.search(r'(?:^|\|)open=true(?:\||$)', text):
            continue
        m = re.search(r'bundle=([^|\s]+)', text)
        if m and m.group(1) != bundle:
            continue
        return True
    return False


def classify_event_channel(text):
    """Three-channel projection (aa-test/tag/app) for the E3 capture stream.
    The continuous capture is a single hilog stream filtered by the E3PhysVpn
    tag; the channel distinguishes:
    - 'aa-test': aa test harness output lines (aa test / TestFinished /
      printSync / OHOS_REPORT markers)
    - 'app': app-level lines carrying bundle references or app UI markers
      (bundle=cn.alfadb.netbird.e3physvpna|b, UI_START|/UI_STOP|/VPN_*)
    - 'tag': the remaining E3PhysVpn tag-channel lines
    """
    text = str(text)
    if re.search(r'(?i)aa[ -]?test|TestFinished|printSync|OHOS_REPORT', text):
        return 'aa-test'
    if re.search(r'bundle=cn\.alfadb\.netbird\.e3physvpn[ab](\||\s|$)'
                 r'|(?:^|\s)(?:UI_START|UI_STOP|UI_START_SKIPPED|UI_STOP_SKIPPED|VPN_[A-Z_]+)\|', text):
        return 'app'
    return 'tag'


def summarize_capture_events(events):
    """N0-style marker line detection and dedup counting (n0-emulator-run.sh
    MARKER_DISTINCT_COUNT pattern): every event line is scanned for a marker
    token (Get-E3EventInfo), identical marker lines are deduplicated (the same
    marker legitimately appears in multiple channels), and the summary reports
    the distinct marker count, per-marker line counts, channel projection
    counts and the total event count."""
    marker_lines = []
    marker_counts = {}
    channel_counts = {}
    for event in events:
        info = get_e3_event_info(event)
        text = info['text']
        channel = classify_event_channel(text)
        channel_counts[channel] = channel_counts.get(channel, 0) + 1
        if info['marker'] is not None:
            marker_lines.append(text)
            marker_counts[info['marker']] = marker_counts.get(info['marker'], 0) + 1
    distinct_markers = []
    seen = set()
    for line in marker_lines:
        if line not in seen:
            seen.add(line)
            distinct_markers.append(line)
    return {
        'total_events': len(events),
        'marker_lines': len(marker_lines),
        'marker_distinct_count': len(distinct_markers),
        'marker_distinct_lines': distinct_markers,
        'marker_counts': marker_counts,
        'channel_counts': channel_counts,
    }


# =====================================================================
# Section 9: Layout facts and machine checkpoints (design unit U6)
# =====================================================================


def get_optional_json_boolean(obj, name, default=False):
    """PS L128-137 Get-OptionalJsonBoolean: simulation hook booleans must be
    JSON Booleans (a string/number is a fixture error, never silently
    coerced)."""
    if obj is None or not isinstance(obj, dict) or name not in obj:
        return default
    value = obj[name]
    if value is None or not isinstance(value, bool):
        raise RuntimeError("simulation hook '%s' must be a JSON Boolean" % name)
    return bool(value)


def wait_until(target):
    """PS L250-261 Wait-Until: under LiveSimulation advances the virtual
    clock (R16); otherwise sleeps in 10-250ms steps until the target."""
    global virtual_seconds
    if live_simulation:
        delta = (target - get_now()).total_seconds()
        if delta > 0:
            virtual_seconds += delta
        return
    while get_now() < target:
        remaining_ms = (target - get_now()).total_seconds() * 1000.0
        time.sleep(max(0.01, min(0.25, remaining_ms / 1000.0)))


def get_hdc_install_assessment(result):
    """PS L1603-1620 Get-HdcInstallAssessment: three-state install outcome.
    Exit 0 alone is never sufficient; functional_fail only for explicit
    rejection evidence."""
    exit_code = int(result.exit_code)
    text = result.combined_text()
    if exit_code in (124, 125):
        return {'status': 'infrastructure', 'reason': 'install-exit-%d' % exit_code}
    reject = re.search(r'(?i)(?:error:\s*failed\s+to\s+(?:execute\s+your\s+command|install)|install\s+bundle\s+fail|install\s+fail(?:ed|ure)?\b|signature\s+(?:reject(?:ed)?|invalid|fail|mismatch)|profile\s+(?:reject(?:ed)?|mismatch|not\s+(?:match|found))|device\s+not\s+(?:in\s+)?profile|(?:error[_ ]?code|err(?:or)?code)\s*[=:]?\s*\d+)', text)
    if reject:
        return {'status': 'functional_fail', 'reason': 'install-rejected'}
    if exit_code == 0 and re.search(r'(?i)install bundle successfully', text):
        return {'status': 'pass', 'reason': 'install-success-string'}
    return {'status': 'blocked', 'reason': 'install-outcome-uncertain'}


def test_staging_absent(result):
    """PS L1648-1657 Test-StagingAbsent: StagingProbe is fixed-path ls -ld.
    Only clear path-absence evidence counts as absent; cannot access /
    permission denied are uncertain residual, not absence."""
    text = result.combined_text()
    if re.search(r'(?i)permission denied|access denied|cannot access|not permitted', text):
        return False
    if re.search(r'(?i)no such file|not found|path does not exist', text):
        return True
    if int(result.exit_code) == 0 and re.search(re.escape(STAGING), text):
        return False
    return False


def confirm_bundle_installed(operation, bundle, label):
    """PS L1659-1684 Confirm-BundleInstalled: install + BundleDump
    confirmation for the S1 A/B install gate. Infrastructure (124/125)
    throws; functional rejection throws FUNCTIONAL_FAIL; uncertain stays
    blocked."""
    install_result = invoke_hdc_operation(operation, allow_failure=True)
    if install_result.exit_code in (124, 125):
        raise RuntimeError('HDC infrastructure interruption during %s exit=%d' % (operation, install_result.exit_code))
    install_assessment = get_hdc_install_assessment(install_result)
    if install_assessment['status'] == 'functional_fail':
        raise RuntimeError('FUNCTIONAL_FAIL scenario-1 FINAL HAP %s install rejected' % label)
    if install_assessment['status'] != 'pass':
        raise RuntimeError('scenario-1 FINAL HAP %s install outcome blocked: %s' % (label, install_assessment['reason']))
    dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
    if dump_result.exit_code in (124, 125):
        raise RuntimeError('HDC infrastructure interruption during BundleDump exit=%d' % dump_result.exit_code)
    dump_assessment = get_bundle_dump_assessment(dump_result, bundle)
    if dump_assessment['status'] == 'infrastructure':
        raise RuntimeError('HDC infrastructure interruption during BundleDump: %s' % dump_assessment['reason'])
    if dump_assessment['status'] != 'pass':
        raise RuntimeError('scenario-1 FINAL HAP %s install confirmation blocked: %s' % (label, dump_assessment['reason']))


def invoke_remove_staging_verified(action_label):
    """PS L1685-1704 Invoke-RemoveStagingVerified: RemoveStaging + probe,
    records CleanupActions, clears the staging flags only on verified
    absence."""
    global staging_sent, staging_may_exist
    remove_result = invoke_hdc_operation('RemoveStaging', allow_failure=True)
    probe_result = invoke_hdc_operation('StagingProbe', allow_failure=True)
    absent = test_staging_absent(probe_result)
    cleanup_actions.append({
        'operation': action_label,
        'remove_exit_code': remove_result.exit_code,
        'probe_exit_code': probe_result.exit_code,
        'staging_absent': bool(absent),
        'staging_sent_flag': bool(staging_sent),
        'staging_may_exist_flag': bool(staging_may_exist),
    })
    if absent:
        staging_sent = False
        staging_may_exist = False
        return True
    return False


def new_simulated_ui_node(attributes, children=None):
    """PS L1360-1366 New-SimulatedUiNode: one node of the simulated layout
    fixture with the real attributes/children array shape (ADJ-20260808-0003
    C6)."""
    return {'attributes': attributes, 'children': list(children) if children else []}


def get_simulated_layout_document(name):
    """PS L1368-1470 Get-SimulatedLayoutDocument: layout_profiles override,
    name-based profile selection, and the layout_ready_delays simulation
    knob (a listed capture stays generic until the given virtual seconds
    have elapsed since its first capture attempt)."""
    global simulation_layout_first_attempt
    profiles = get_optional_property(simulation, 'layout_profiles', None)
    override = get_optional_property(profiles, name, None) if profiles is not None else None
    if override is not None and not isinstance(override, str):
        return override
    if override is not None:
        profile = str(override)
    elif re.search(r'authorization', name):
        profile = 'authorization'
    elif re.search(r'settings-vpn-page', name):
        profile = 'settings-vpn'
    elif re.search(r'app-info', name):
        profile = 'settings-app-info-a'
    elif re.search(r'scenario-6-after-allow-b', name):
        profile = 'entry-b'
    elif re.search(r'(?:entry-a|after-allow|reactivation|scenario-3-|scenario-7-)', name):
        profile = 'entry-a'
    elif re.search(r'(?:entry-b|after-deny|scenario-4-|scenario-6-conflict)', name):
        profile = 'entry-b'
    else:
        profile = 'generic'
    ready_delays = get_optional_property(simulation, 'layout_ready_delays', None)
    if ready_delays is not None and get_optional_property(ready_delays, name, None) is not None:
        delay = float(get_optional_property(ready_delays, name, 0.0))
        if name not in simulation_layout_first_attempt:
            simulation_layout_first_attempt[name] = get_now()
        elapsed = (get_now() - simulation_layout_first_attempt[name]).total_seconds()
        if elapsed < delay:
            return [new_simulated_ui_node({'bundleName': 'generic', 'type': 'root', 'id': '', 'key': '', 'text': ''})]
    if profile == 'authorization':
        return [
            new_simulated_ui_node({'bundleName': 'com.huawei.hmos.vpndialog', 'type': 'Dialog', 'id': '', 'key': '', 'text': 'E3 Physical VPN Preflight'}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': '', 'key': '', 'text': '是否允许使用 VPN？'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'permission_cancel_button', 'key': 'permission_cancel_button', 'text': '取消'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'permission_allow_button', 'key': 'permission_allow_button', 'text': '允许'}),
            ]),
        ]
    if profile == 'entry-a':
        return [
            new_simulated_ui_node({'bundleName': BUNDLE_A, 'type': 'root', 'id': '', 'key': '', 'text': ''}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'start-vpn', 'key': 'start-vpn', 'text': 'Start VPN'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'stop-vpn', 'key': 'stop-vpn', 'text': 'Stop VPN'}),
            ]),
        ]
    if profile == 'entry-b':
        return [
            new_simulated_ui_node({'bundleName': BUNDLE_B, 'type': 'root', 'id': '', 'key': '', 'text': ''}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'start-vpn', 'key': 'start-vpn', 'text': 'Start VPN'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'stop-vpn', 'key': 'stop-vpn', 'text': 'Stop VPN'}),
            ]),
        ]
    if profile == 'settings-vpn':
        return [
            new_simulated_ui_node({'bundleName': 'com.huawei.hmos.settings', 'type': 'root', 'id': '', 'key': '', 'text': ''}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': 'Setting.MobileNetwork.vpn_group_group.vpn_settings.title', 'key': 'Setting.MobileNetwork.vpn_group_group.vpn_settings.title', 'text': 'VPN'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': 'Setting.MobileNetwork.vpn_group_group.vpn_settings', 'key': 'Setting.MobileNetwork.vpn_group_group.vpn_settings', 'text': '没有 VPN'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': '', 'key': '', 'text': '添加 VPN 网络'}),
            ]),
        ]
    if profile == 'settings-vpn-fake-app':
        return [
            new_simulated_ui_node({'bundleName': BUNDLE_A, 'type': 'root', 'id': '', 'key': '', 'text': ''}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': '', 'key': '', 'text': 'VPN settings'}),
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': '', 'key': '', 'text': 'VPN connection'}),
            ]),
        ]
    if profile == 'settings-app-info-a':
        return [
            new_simulated_ui_node({'bundleName': 'com.huawei.hmos.settings', 'type': 'root', 'id': '', 'key': '', 'text': '', 'visible': 'true'}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'NavDestination', 'id': 'Setting.AppDetail', 'key': 'Setting.AppDetail', 'text': '', 'visible': 'true'}, [
                    new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': 'Setting.AppDetail.title_id', 'key': 'Setting.AppDetail.title_id', 'text': 'E3 Preflight A', 'visible': 'true'}),
                    new_simulated_ui_node({'bundleName': '', 'type': 'Button', 'id': 'force_stop_button', 'key': 'force_stop_button', 'text': '强行停止', 'visible': 'true'}),
                ]),
            ]),
        ]
    if profile == 'wrong-page':
        return [
            new_simulated_ui_node({'bundleName': 'com.example.unrelated', 'type': 'root', 'id': '', 'key': '', 'text': ''}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': '', 'key': '', 'text': 'Unrelated page'}),
            ]),
        ]
    if profile == 'authorization-missing-controls':
        return [
            new_simulated_ui_node({'bundleName': 'com.huawei.hmos.vpndialog', 'type': 'Dialog', 'id': '', 'key': '', 'text': 'E3 Physical VPN Preflight'}, [
                new_simulated_ui_node({'bundleName': '', 'type': 'Text', 'id': '', 'key': '', 'text': '是否允许使用 VPN？'}),
            ]),
        ]
    return [new_simulated_ui_node({'bundleName': 'generic', 'type': 'root', 'id': '', 'key': '', 'text': ''})]


def get_simulation_hdc_result(operation, parameters):
    """PS L1471-1530 Get-SimulationHdcResult: simulated HDC results with
    hdc_failures (operation+occurrence), capture_failures, and the
    simulation state machine (installed / staging / active bundles).
    ReceiveScreen/ReceiveLayout write the capture files under raw_path."""
    global hdc_operation_counts, simulation_installed_a, simulation_installed_b
    global simulation_staging_present, simulation_active_bundles
    if operation not in hdc_operation_counts:
        hdc_operation_counts[operation] = 0
    hdc_operation_counts[operation] += 1
    occurrence = hdc_operation_counts[operation]
    for failure in (get_optional_property(simulation, 'hdc_failures', []) or []):
        if str(get_optional_property(failure, 'operation', '')) == operation \
                and int(get_optional_property(failure, 'occurrence', 1)) == occurrence:
            return HdcResult(int(get_optional_property(failure, 'exit_code', 1)),
                             str(get_optional_property(failure, 'stdout', '')),
                             str(get_optional_property(failure, 'stderr', 'simulated command failure')), True)
    capture_failures = get_optional_property(simulation, 'capture_failures', []) or []
    if 'Name' in parameters and str(parameters['Name']) in capture_failures:
        return HdcResult(9, '', 'simulated unknown capture command', True)
    stdout = 'SIMULATED_OK'
    if operation == 'Version':
        stdout = str(get_optional_property(simulation, 'hdc_version', 'SELFTEST-HDC-1.0'))
    elif operation == 'TupleModel':
        stdout = 'PLA-AL10'
    elif operation == 'TupleBuild':
        stdout = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
    elif operation == 'BundleDump':
        bundle = str(parameters['Bundle'])
        installed = (bundle == BUNDLE_A and simulation_installed_a) or (bundle == BUNDLE_B and simulation_installed_b)
        stdout = '{ "app": { "bundleName": "%s" } }' % bundle if installed \
            else 'error: failed to get information and the parameters may be wrong.'
    elif operation == 'PidOf':
        stdout = '4242' if str(parameters['Bundle']) in simulation_active_bundles else ''
    elif operation in ('InstallA', 'InstallB'):
        stdout = 'install bundle successfully.'
    elif operation == 'StagingProbe':
        stdout = 'drwxrwxrwx 3 shell shell 4096 2026-01-01 00:00 %s' % STAGING if simulation_staging_present \
            else 'ls: %s: No such file or directory' % STAGING
    exit_code = 0
    if operation == 'StagingProbe' and not simulation_staging_present:
        exit_code = 1
    if operation in ('MkdirStaging', 'SendA', 'SendB'):
        simulation_staging_present = True
    if operation == 'RemoveStaging':
        simulation_staging_present = False
    if operation == 'InstallA':
        simulation_installed_a = True
    if operation == 'InstallB':
        simulation_installed_b = True
    if operation == 'ForceStop':
        simulation_active_bundles.discard(str(parameters['Bundle']))
    if operation == 'Uninstall':
        simulation_active_bundles.discard(str(parameters['Bundle']))
        if str(parameters['Bundle']) == BUNDLE_A:
            simulation_installed_a = False
        if str(parameters['Bundle']) == BUNDLE_B:
            simulation_installed_b = False
    if operation in ('ReceiveScreen', 'ReceiveLayout'):
        extension = '.png' if operation == 'ReceiveScreen' else '.json'
        destination = os.path.join(raw_path, 'capture-%s%s' % (str(parameters['Name']), extension))
        if operation == 'ReceiveScreen':
            with open(destination, 'wb') as f:
                f.write(bytes([1, 2, 3, 4]))
        else:
            invalid_layouts = get_optional_property(simulation, 'invalid_layout_json', []) or []
            if str(parameters['Name']) in invalid_layouts:
                write_text_utf8_no_bom(destination, '{invalid-layout-json')
            else:
                write_text_utf8_no_bom(destination, jsoncompat_dumps(get_simulated_layout_document(str(parameters['Name'])), indent=2) + '\n')
    return HdcResult(exit_code, stdout, '', True)


def get_layout_facts(layout_document):
    """PS L3286-3312 Get-LayoutFacts: flatten a layout JSON value into
    `$path=value` fact strings (attributes/children array shape,
    ADJ-20260808-0003 C6)."""
    facts = []

    def visit(current, path):
        if current is None:
            return
        if isinstance(current, (str, int, float, bool)):
            facts.append('%s=%s' % (path, current))
            return
        if isinstance(current, dict):
            for key in current:
                visit(current[key], '%s.%s' % (path, key))
            return
        if isinstance(current, (list, tuple)):
            for index, item in enumerate(current):
                visit(item, '%s[%d]' % (path, index))

    visit(layout_document, '$')
    return facts


def test_captured_layout_profile(facts, profile, expected_bundle=None):
    """PS L3313-3400 Test-CapturedLayoutProfile: deterministic layout profile
    matching over flattened facts (entry / authorization /
    authorization-dismissed / settings-vpn / settings-app-info). Returns
    {status: pass|mismatch, reason, profile, matched, required}."""
    joined = ('\n'.join(str(f) for f in facts)).lower()
    checks = {}
    attr_field = r'[^=\n]*?\.attributes\.'
    text_node = attr_field + 'text='

    def attr_value_pattern(field, value):
        return '(?:^|\n)' + attr_field + field + '=' + re.escape(value) + '(?=\n|$)'

    if expected_bundle and profile == 'entry':
        checks['expected-bundle'] = re.search(attr_value_pattern('bundlename', str(expected_bundle).lower()), joined) is not None
    if profile == 'entry':
        checks['start-control'] = re.search(attr_field + '(?:id|key)=start-vpn(?=\n|$)', joined) is not None
        checks['stop-control'] = re.search(attr_field + '(?:id|key)=stop-vpn(?=\n|$)', joined) is not None
    elif profile == 'authorization':
        checks['dialog-owner'] = re.search(attr_value_pattern('bundlename', 'com.huawei.hmos.vpndialog'), joined) is not None
        checks['dialog-type'] = re.search(attr_value_pattern('type', 'dialog'), joined) is not None
        checks['dialog-text'] = (re.search(text_node + '[^\n]*允许[^\n]*vpn[^\n]*(?=\n|$)', joined) is not None) \
            or (re.search(text_node + '[^\n]*vpn[^\n]*允许[^\n]*(?=\n|$)', joined) is not None)
        checks['allow-control'] = (re.search(text_node + '[^\n]*\ballow\b[^\n]*(?=\n|$)', joined) is not None) \
            or (re.search(text_node + '[^\n]*允许[^\n]*(?=\n|$)', joined) is not None)
        checks['cancel-control'] = (re.search(text_node + '[^\n]*(?:cancel|deny)[^\n]*(?=\n|$)', joined) is not None) \
            or (re.search(text_node + '[^\n]*取消[^\n]*(?=\n|$)', joined) is not None) \
            or (re.search(text_node + '[^\n]*拒绝[^\n]*(?=\n|$)', joined) is not None) \
            or (re.search(text_node + '[^\n]*不允许[^\n]*(?=\n|$)', joined) is not None)
    elif profile == 'authorization-dismissed':
        checks['authorization-controls-absent'] = re.search(text_node + '[^\n]*(?:允许|取消|拒绝|不允许)[^\n]*(?=\n|$)', joined) is None
        checks['entry-start-control'] = re.search(attr_field + '(?:id|key)=start-vpn(?=\n|$)', joined) is not None
    elif profile == 'settings-vpn':
        checks['settings-owner'] = re.search(attr_value_pattern('bundlename', 'com.huawei.hmos.settings'), joined) is not None
        checks['vpn-group-resource'] = re.search(attr_field + r'(?:id|key)=setting\.mobilenetwork\.vpn_group_group\.vpn_settings(?=\n|$)', joined) is not None
        checks['vpn-page-text'] = (re.search(text_node + '[^\n]*vpn[^\n]*(?=\n|$)', joined) is not None) \
            and (re.search(text_node + '[^\n]*没有 vpn[^\n]*(?=\n|$)', joined) is not None)
        checks['add-vpn-button'] = re.search(text_node + '[^\n]*添加 vpn 网络[^\n]*(?=\n|$)', joined) is not None
    elif profile == 'settings-app-info':
        fact_map = {}
        for fact in facts:
            path, separator, value = str(fact).partition('=')
            if separator:
                fact_map[path.lower()] = value

        settings_bases = {
            re.sub(r'\.attributes\.bundlename$', '', path)
            for path, value in fact_map.items()
            if path.endswith('.attributes.bundlename') and value.lower() == 'com.huawei.hmos.settings'
        }
        checks['settings-owner'] = bool(settings_bases)
        hidden_bases = {
            re.sub(r'\.attributes\.visible$', '', path)
            for path, value in fact_map.items()
            if path.endswith('.attributes.visible') and value.lower() == 'false'
        }
        detail_bases = set()
        for path, value in fact_map.items():
            if not re.search(r'\.attributes\.(?:id|key)$', path) or value.lower() != 'setting.appdetail':
                continue
            base = re.sub(r'\.attributes\.(?:id|key)$', '', path)
            if not any(base.startswith(settings_base + '.') for settings_base in settings_bases):
                continue
            if str(fact_map.get(base + '.attributes.visible', '')).lower() != 'true':
                continue
            if str(fact_map.get(base + '.attributes.type', '')).lower() != 'navdestination':
                continue
            if any(base == hidden_base or base.startswith(hidden_base + '.')
                   for hidden_base in hidden_bases):
                continue
            detail_bases.add(base)

        if str(expected_bundle) == BUNDLE_A:
            accepted_labels = {'e3 preflight a'}
        elif str(expected_bundle) == BUNDLE_B:
            accepted_labels = {'e3 preflight b'}
        else:
            accepted_labels = set()

        label_found = False
        force_stop_found = False
        for detail_base in detail_bases:
            detail_has_label = False
            detail_has_force_stop = False
            for path, value in fact_map.items():
                if not path.startswith(detail_base + '.'):
                    continue
                node_base = re.sub(r'\.attributes\.[^.]+$', '', path)
                if any(node_base == hidden_base or node_base.startswith(hidden_base + '.')
                       for hidden_base in hidden_bases):
                    continue
                normalized = re.sub(r'\s+', ' ', str(value).strip().lower())
                if path.endswith('.attributes.text'):
                    if normalized in accepted_labels:
                        detail_has_label = True
                    if normalized in {'强行停止', '强制停止', 'force stop'}:
                        detail_has_force_stop = True
                elif path.endswith('.attributes.id') or path.endswith('.attributes.key'):
                    if re.search(r'force[_-]?stop', normalized):
                        detail_has_force_stop = True
            label_found = label_found or detail_has_label
            force_stop_found = force_stop_found or (detail_has_label and detail_has_force_stop)

        checks['app-detail-structure'] = len(detail_bases) == 1
        checks['app-label'] = label_found
        checks['force-stop-control'] = force_stop_found
    failed = [k for k in checks if not checks[k]]
    return {
        'status': 'pass' if not failed else 'mismatch',
        'reason': 'deterministic-layout-match' if not failed else 'layout-fields-missing:' + ','.join(failed),
        'profile': profile,
        'matched': [k for k in checks if checks[k]],
        'required': list(checks.keys()),
    }


def _assess_layout_profile(name, profile, expected_bundle=None):
    """PS Test-CapturedLayoutProfile L3313-3400 artifact half: read the
    latest same-name collected layout artifact and evaluate the profile.
    Uncollected / invalid JSON stays unverifiable."""
    artifact = [a for a in capture_artifacts if str(a['name']) == name][-1:]
    if len(artifact) != 1 or str(artifact[0]['status']) != 'collected':
        return {'status': 'unverifiable', 'reason': 'capture-not-collected', 'profile': profile, 'matched': [], 'required': []}
    try:
        with open(artifact[0]['layout_path'], 'r', encoding='utf-8-sig') as f:
            layout = json.load(f)
    except Exception:
        return {'status': 'unverifiable', 'reason': 'layout-json-invalid', 'profile': profile, 'matched': [], 'required': []}
    return test_captured_layout_profile(get_layout_facts(layout), profile, expected_bundle)


def invoke_capture(name, scenario, observation_only=False, replace=False):
    """PS L3221-3285 Invoke-Capture: ScreenCap/DumpLayout/Receive screen+layout
    capture. Infrastructure failures (124/125 / timeout / HDC transport)
    propagate as infrastructure blocked; observation-only captures never
    enter the global CaptureDegraded list. -Replace drops previous same-name
    artifacts (bounded same-name layout resample)."""
    global last_capture_infrastructure, infrastructure_reason_observed
    last_capture_infrastructure = False
    operations = ('ScreenCap', 'DumpLayout', 'ReceiveScreen', 'ReceiveLayout')
    failures = []
    infrastructure_failures = []
    if campaign_capture is not None and campaign_capture['degraded']:
        assert_campaign_capture_healthy(campaign_capture, scenario, 'Invoke-Capture')
    else:
        for operation in operations:
            result = invoke_hdc_operation(operation, {'Name': name}, allow_failure=True)
            if result.exit_code != 0:
                failures.append('%s-exit-%d' % (operation, result.exit_code))
            if result.exit_code in (124, 125) or re.search(r'(?i)\btimeout\b|\boffline\b|\bUSB\b|\bdisconnect(?:ed)?\b|transport (?:offline|error|fail)|HDC Process\.Start', str(result.stderr)):
                infrastructure_failures.append('%s-exit-%d' % (operation, result.exit_code))
    screen_path = os.path.join(raw_path, 'capture-%s.png' % name)
    layout_path = os.path.join(raw_path, 'capture-%s.json' % name)
    if not dry_run:
        for path in (screen_path, layout_path):
            if not os.path.isfile(path) or os.path.getsize(path) == 0:
                failures.append('missing-or-empty:%s' % os.path.basename(path))
    status = 'collected' if not failures else 'degraded'
    artifact = {'scenario': scenario, 'name': name, 'status': status, 'failures': failures,
                'screen_path': screen_path, 'layout_path': layout_path}
    if replace:
        capture_artifacts[:] = [a for a in capture_artifacts if str(a['name']) != name]
    capture_artifacts.append(artifact)
    if status == 'degraded':
        is_infrastructure = bool(infrastructure_failures)
        last_capture_infrastructure = is_infrastructure
        if observation_only:
            observation_only_degraded.append({'scenario': scenario, 'name': name, 'status': 'degraded',
                                             'category': 'infrastructure' if is_infrastructure else 'non-infrastructure',
                                             'infrastructure_reason': 'hdc-usb-interruption' if is_infrastructure else None,
                                             'failures': failures, 'screen_path': screen_path, 'layout_path': layout_path})
        else:
            if is_infrastructure:
                infrastructure_reason_observed = 'hdc-usb-interruption'
                add_capture_degradation(campaign_capture, 'screen-layout-capture', ','.join(failures),
                                        scenario=scenario, category='infrastructure',
                                        infrastructure_reason='hdc-usb-interruption', mark_continuous_degraded=False)
            else:
                add_capture_degradation(campaign_capture, 'screen-layout-capture', ','.join(failures),
                                        scenario=scenario, category='non-infrastructure', mark_continuous_degraded=False)
    return status


def invoke_layout_checkpoint(scenario, name, profile, expected_bundle=None, step_index=None,
                             step_id=None, expected_action=None, observation_only=False,
                             mismatch_is_blocked=False):
    """PS L3400-3465 Invoke-LayoutCheckpoint: machine-only deterministic
    layout gate. Uncollected capture is invalid at a decisive gate (or
    infrastructure blocked); a collected mismatch is re-captured under the
    SAME name at ~1s intervals for at most 8 seconds (bounded same-name
    resample) and re-evaluated."""
    capture_status = invoke_capture(name, scenario, observation_only=observation_only)
    if capture_status != 'collected':
        checkpoint = {'status': 'unverifiable', 'name': name, 'capture_status': capture_status,
                      'profile': profile, 'matching': False, 'note': 'screenshot-or-layout-not-collected',
                      'reason': 'capture-not-collected'}
        add_transcript_record('machine-layout-checkpoint', {'scenario': scenario, 'checkpoint': checkpoint})
        if last_capture_infrastructure:
            raise RuntimeError('HDC infrastructure interruption layout-checkpoint=%s scenario=%d' % (name, scenario))
        throw_scenario_invalid(scenario, 'layout-checkpoint-%s-capture-not-collected' % name,
                               step_index, step_id, expected_action, machine_postcondition=checkpoint,
                               capture_after={'status': capture_status, 'name': name})
    assessment = _assess_layout_profile(name, profile, expected_bundle)
    attempts = 0
    resample_deadline = get_now() + timedelta(seconds=8)
    while str(assessment['status']) == 'mismatch' and get_now() < resample_deadline:
        attempts += 1
        wait_until(get_now() + timedelta(seconds=1))
        retry_status = invoke_capture(name, scenario, observation_only=observation_only, replace=True)
        if retry_status != 'collected':
            if last_capture_infrastructure:
                raise RuntimeError('HDC infrastructure interruption layout-checkpoint=%s scenario=%d (resample attempt %d)' % (name, scenario, attempts))
            break
        assessment = _assess_layout_profile(name, profile, expected_bundle)
        add_transcript_record('machine-layout-resample', {'scenario': scenario, 'name': name, 'profile': profile,
                                                          'attempt': attempts, 'matching': str(assessment['status']) == 'pass',
                                                          'reason': str(assessment['reason']),
                                                          'missing': [r for r in assessment['required'] if r not in assessment['matched']]})
    matching = str(assessment['status']) == 'pass'
    checkpoint = {'status': str(assessment['status']), 'name': name, 'capture_status': capture_status,
                  'profile': profile, 'expected_bundle': expected_bundle, 'matching': matching,
                  'reason': str(assessment['reason']), 'matched': assessment['matched'],
                  'required': assessment['required'], 'attempts': attempts, 'note': 'machine-deterministic-layout-v1'}
    add_transcript_record('machine-layout-checkpoint', {'scenario': scenario, 'checkpoint': checkpoint})
    if not matching:
        suffix = 'layout-unverifiable' if str(assessment['status']) == 'unverifiable' else 'layout-mismatch'
        if mismatch_is_blocked:
            # The new S6 authorization UI state is platform-uncertain after Allow, so both
            # outcomes fail closed while preserving whether dismissal mismatched or was unverifiable.
            blocked_reason = 'authorization-dismissal-unverifiable' if str(assessment['status']) == 'unverifiable' else 'authorization-not-dismissed'
            raise RuntimeError('scenario-%d machine-verification-blocked step=%s reason=%s:%s' %
                               (scenario, step_index, blocked_reason, str(assessment['reason'])))
        throw_scenario_invalid(scenario, 'layout-checkpoint-%s-%s' % (name, suffix),
                               step_index, step_id, expected_action, machine_postcondition=checkpoint,
                               capture_after={'status': capture_status, 'name': name})
    return checkpoint


def invoke_layout_choice_checkpoint(scenario, name, expected_bundle=None, step_index=None,
                                    step_id=None, expected_action=None, observation_only=False):
    """PS L3466-3560 Invoke-LayoutChoiceCheckpoint: dual-profile (entry OR
    authorization) machine layout gate for optional reauthorization (S5/S6 A).
    Any profile pass returns selected_profile; a final dual mismatch stays
    scenario invalid."""
    capture_status = invoke_capture(name, scenario, observation_only=observation_only)
    if capture_status != 'collected':
        checkpoint = {'status': 'unverifiable', 'name': name, 'capture_status': capture_status,
                      'selected_profile': None, 'matching': False, 'note': 'screenshot-or-layout-not-collected',
                      'reason': 'capture-not-collected', 'attempts': 0}
        add_transcript_record('machine-layout-choice-checkpoint', {'scenario': scenario, 'checkpoint': checkpoint})
        if last_capture_infrastructure:
            raise RuntimeError('HDC infrastructure interruption layout-choice-checkpoint=%s scenario=%d' % (name, scenario))
        throw_scenario_invalid(scenario, 'layout-choice-checkpoint-%s-capture-not-collected' % name,
                               step_index, step_id, expected_action, machine_postcondition=checkpoint,
                               capture_after={'status': capture_status, 'name': name})
    entry_assessment = _assess_layout_profile(name, 'entry', expected_bundle)
    auth_assessment = _assess_layout_profile(name, 'authorization', expected_bundle)
    selected_profile = None
    if str(auth_assessment['status']) == 'pass':
        selected_profile = 'authorization'
    elif str(entry_assessment['status']) == 'pass':
        selected_profile = 'entry'
    attempts = 0
    resample_deadline = get_now() + timedelta(seconds=8)
    while selected_profile is None and get_now() < resample_deadline:
        attempts += 1
        wait_until(get_now() + timedelta(seconds=1))
        retry_status = invoke_capture(name, scenario, observation_only=observation_only, replace=True)
        if retry_status != 'collected':
            if last_capture_infrastructure:
                raise RuntimeError('HDC infrastructure interruption layout-choice-checkpoint=%s scenario=%d (resample attempt %d)' % (name, scenario, attempts))
            break
        entry_assessment = _assess_layout_profile(name, 'entry', expected_bundle)
        auth_assessment = _assess_layout_profile(name, 'authorization', expected_bundle)
        if str(auth_assessment['status']) == 'pass':
            selected_profile = 'authorization'
        elif str(entry_assessment['status']) == 'pass':
            selected_profile = 'entry'
        add_transcript_record('machine-layout-choice-resample', {'scenario': scenario, 'name': name, 'attempt': attempts,
                                                                 'matching': selected_profile is not None,
                                                                 'selected_profile': selected_profile,
                                                                 'entry_reason': str(entry_assessment['reason']),
                                                                 'authorization_reason': str(auth_assessment['reason'])})
    matching = selected_profile is not None
    checkpoint = {'status': 'pass' if matching else 'mismatch', 'name': name, 'capture_status': capture_status,
                  'selected_profile': selected_profile, 'expected_bundle': expected_bundle, 'matching': matching,
                  'reason': 'layout-choice-%s' % selected_profile if matching else 'entry:%s;authorization:%s' % (str(entry_assessment['reason']), str(auth_assessment['reason'])),
                  'attempts': attempts, 'note': 'machine-deterministic-layout-choice-v1'}
    add_transcript_record('machine-layout-choice-checkpoint', {'scenario': scenario, 'checkpoint': checkpoint})
    if not matching:
        throw_scenario_invalid(scenario, 'layout-choice-checkpoint-%s-layout-mismatch' % name,
                               step_index, step_id, expected_action, machine_postcondition=checkpoint,
                               capture_after={'status': capture_status, 'name': name})
    return checkpoint


def invoke_review_only_capture(name, scenario):
    """PS L3562-3570 Invoke-ReviewOnlyCapture: final evidence only; never a
    semantic verdict input."""
    capture_status = invoke_capture(name, scenario, observation_only=True)
    checkpoint = {'status': 'review-only', 'name': name, 'capture_status': capture_status, 'matching': None,
                  'note': 'final evidence only; not used as a semantic verdict input'}
    add_transcript_record('review-only-layout-artifact', {'scenario': scenario, 'checkpoint': checkpoint})
    return capture_status


def get_exact_process_checkpoint(expected_active_bundles, observed_bundles=None):
    """PS L3582-3630 Get-ExactProcessCheckpoint: every non-pass process
    checkpoint is status=blocked (never invalid). HDC transport/timeout on
    PidOf/BundleDump records hdc-usb-interruption; observed-only bundles are
    recorded but never gate the checkpoint."""
    global infrastructure_reason_observed
    if observed_bundles is None:
        observed_bundles = []
    states = []
    valid = True
    reason = None
    infra_hit = False
    for bundle in (BUNDLE_A, BUNDLE_B):
        pid_result = invoke_hdc_operation('PidOf', {'Bundle': bundle}, allow_failure=True)
        dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
        dump = get_bundle_dump_assessment(dump_result, bundle)
        pid_is_infra = test_fault_infrastructure_failure(pid_result)
        dump_is_infra = test_fault_infrastructure_failure(dump_result) or str(dump['status']) == 'infrastructure'
        pid_known = int(pid_result.exit_code) in (0, 1) and not str(pid_result.stderr).strip()
        present = pid_known and bool(str(pid_result.stdout).strip())
        expected_present = bundle in expected_active_bundles
        observed_only = bundle in observed_bundles
        if observed_only:
            states.append({'bundle': bundle, 'bundle_present': dump['status'] == 'pass',
                           'process_target': '%s:vpn' % bundle, 'process_present': present,
                           'expected_present': None, 'observed_only': True})
            continue
        if pid_is_infra or dump_is_infra:
            valid = False
            infra_hit = True
            if reason is None:
                reason = 'hdc-usb-interruption:process-check:%s' % bundle
        elif dump['status'] != 'pass':
            valid = False
            if reason is None:
                reason = 'bundle-check-unavailable:%s' % bundle
        elif not pid_known:
            valid = False
            if reason is None:
                reason = 'process-check-unavailable:%s' % bundle
        elif present != expected_present:
            valid = False
            if reason is None:
                reason = 'process-state-mismatch:%s expected-active=%s actual-active=%s' % (bundle, expected_present, present)
        states.append({'bundle': bundle, 'bundle_present': dump['status'] == 'pass',
                       'process_target': '%s:vpn' % bundle, 'process_present': present,
                       'expected_present': expected_present, 'observed_only': False})
    if infra_hit:
        infrastructure_reason_observed = 'hdc-usb-interruption'
    return {'status': 'pass' if valid else 'blocked', 'reason': 'exact-process-checkpoint' if valid else reason, 'states': states}


def get_process_target_checkpoint(bundle):
    """PS L3632-3655 Get-ProcessTargetCheckpoint: after a machine-verified
    CREATE_ACCEPTED, a precise `pidof <bundle>:vpn` present checkpoint proves
    the naming tuple resolves to a live Extension process. Absent / unknown
    is blocked with an explicit process-target-unverified reason."""
    pid_result = invoke_hdc_operation('PidOf', {'Bundle': bundle}, allow_failure=True)
    dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
    assessment = get_process_probe_status(pid_result, dump_result, bundle)
    verified = str(assessment['status']) == 'present'
    if verified:
        reason = 'process-target-present'
    elif str(assessment['status']) == 'absent':
        reason = 'process-target-unverified:absent'
    else:
        reason = 'process-target-unverified:%s' % assessment['detail']
    return {'status': 'pass' if verified else 'blocked', 'reason': reason,
            'process_target': '%s:vpn' % bundle,
            'probe': {'pid_status': str(assessment['status']), 'detail': assessment['detail'],
                      'bundle_present': bool(assessment['bundle_present'])}}


def test_fault_infrastructure_failure(result):
    """PS L3657-3663 Test-FaultInfrastructureFailure: 124/125 or HDC
    transport/timeout text marks an infrastructure failure."""
    if int(result.exit_code) in (124, 125):
        return True
    text = result.combined_text()
    return bool(re.search(r'(?i)\boffline\b|\bUSB\b|\bdisconnect(?:ed)?\b|transport (?:offline|error)|HDC Process\.Start|\btimeout\b', text))


def invoke_fault_artifact(operation, scenario):
    """PS L3665-3693 Invoke-FaultArtifact: targeted faultlog artifact for
    scenario-7. Loss records CaptureDegraded (scenario-7 only, never marks
    continuous Capture.Degraded or shortens the observation window)."""
    global infrastructure_reason_observed
    suffix = 'a' if operation == 'FaultA' else 'b'
    path = os.path.join(raw_path, 'fault-scenario-%d-%s.txt' % (scenario, suffix))
    status = 'collected'
    failures = []
    result = invoke_hdc_operation(operation, allow_failure=True)
    fault_is_infra = test_fault_infrastructure_failure(result)
    if result.exit_code != 0:
        status = 'degraded'
        failures.append('%s-exit-%d' % (operation, result.exit_code))
    try:
        content = str(result.stdout)
        if str(result.stderr).strip():
            content += '\n' + str(result.stderr)
        write_text_utf8_no_bom(path, content)
        fault_artifacts.append({'scenario': scenario, 'operation': operation,
                                'reference': 'RAW-FAULT-%s-SCENARIO-%d' % (suffix.upper(), scenario),
                                'status': status, 'path': path, 'sha256': sha256_file(path),
                                'bytes': os.path.getsize(path), 'failures': failures})
    except Exception as e:
        status = 'degraded'
        failures.append(protect_sensitive_text(str(e)))
    if status != 'collected':
        category = 'infrastructure' if fault_is_infra else 'non-infrastructure'
        infra_reason = 'hdc-usb-interruption' if fault_is_infra else None
        if fault_is_infra:
            infrastructure_reason_observed = 'hdc-usb-interruption'
        add_capture_degradation(campaign_capture, operation,
                                'targeted fault artifact unavailable: ' + ','.join(failures),
                                scenario=scenario, category=category, infrastructure_reason=infra_reason,
                                mark_continuous_degraded=False)
    return status


# =====================================================================
# Section 10: Scenario orchestration S1-S7 (design unit U6)
# =====================================================================


def throw_scenario_invalid(scenario, reason, step_index=None, step_id=None, expected_action=None,
                           machine_precondition=None, machine_postcondition=None,
                           capture_before=None, capture_after=None):
    """PS L340-365 Throw-ScenarioInvalid: records the scenario-invalid
    state, writes the invalid operator-wait-state, and throws the
    SCENARIO_INVALID message (C19 classification prefix). Scenario invalid
    never retries (scenario_invalid_policy stop-and-finally-cleanup-seal)."""
    global scenario_invalid
    safe_reason = protect_sensitive_text(reason)
    scenario_invalid = {'scenario': scenario, 'step_index': step_index, 'step_id': step_id,
                        'reason': safe_reason, 'detected_at': isoformat_7(get_now())}
    write_operator_wait_state('invalid', scenario=scenario, step_index=step_index, step_id=step_id,
                              expected_action=expected_action, capture_before=capture_before,
                              capture_after=capture_after, machine_precondition=machine_precondition,
                              machine_postcondition=machine_postcondition if machine_postcondition is not None
                              else {'status': 'invalid', 'reason': safe_reason})
    add_transcript_record('scenario-invalid', scenario_invalid)
    raise RuntimeError('SCENARIO_INVALID scenario=%d reason=%s' % (scenario, safe_reason))


def read_operator_enter(scenario, step_index, step_id, expected_action, machine_precondition,
                       capture_before=None, timeout_seconds=None):
    """PS L1706-1763 Read-OperatorEnter: shows the mechanical prompt
    (现在只做：<ExpectedAction>。完成后按回车。), waits for Enter with the
    operator timeout (A8: select-based stdin read on POSIX), and records the
    operator-mechanical-action transcript + operator-complete wait state.
    Timeout throws step-N operator-timeout ScenarioInvalid. Under
    LiveSimulation the virtual clock advances by the fixture action delay
    (R16) and no real input is read."""
    global virtual_seconds
    write_operator_wait_state('waiting', scenario=scenario, step_index=step_index, step_id=step_id,
                              expected_action=expected_action, capture_before=capture_before,
                              machine_precondition=machine_precondition)
    print('现在只做：%s。完成后按回车。' % expected_action)
    completed_at = None
    timed_out = False
    if live_simulation:
        operator_fixture = get_optional_property(simulation, 'operator')
        delay = float(get_optional_property(operator_fixture, 'action_delay_seconds', 1.0))
        step_delays = get_optional_property(operator_fixture, 'step_delay_seconds', None)
        if step_delays is not None:
            step_delay = get_optional_property(step_delays, '%d.%d' % (scenario, step_index), None)
            if step_delay is not None:
                delay = float(step_delay)
        if campaign_capture is not None:
            update_campaign_capture(campaign_capture)
        virtual_seconds += delay
        completed_at = get_now()
    else:
        if timeout_seconds is None:
            timeout_seconds = operator_timeout_seconds
        deadline = get_now() + timedelta(seconds=timeout_seconds)
        line = None
        while True:
            remaining = (deadline - get_now()).total_seconds()
            if remaining <= 0:
                timed_out = True
                break
            try:
                ready, _, _ = select.select([sys.stdin], [], [], min(0.25, remaining))
            except (OSError, ValueError):
                # stdin not selectable (no fd / closed); fall back to a
                # blocking read so a TTY still works.
                try:
                    line = sys.stdin.readline()
                except EOFError:
                    line = ''
                break
            if ready:
                try:
                    line = sys.stdin.readline()
                except EOFError:
                    line = ''
                break
            if campaign_capture is not None:
                update_campaign_capture(campaign_capture)
        if timed_out:
            completed_at = get_now()
        else:
            completed_at = get_now()
    action_record = {'scenario': scenario, 'step_index': step_index, 'step_id': step_id,
                     'expected_action': expected_action, 'completed_at': isoformat_7(completed_at),
                     'input': 'enter', 'timed_out': bool(timed_out)}
    operator_actions.append(action_record)
    add_transcript_record('operator-mechanical-action', action_record)
    write_operator_wait_state('operator-complete', scenario=scenario, step_index=step_index,
                              step_id=step_id, expected_action=expected_action,
                              capture_before=capture_before, machine_precondition=machine_precondition,
                              machine_postcondition={'status': 'pending-verification'})
    if timed_out:
        throw_scenario_invalid(scenario, 'step-%d operator-timeout' % step_index,
                               step_index=step_index, step_id=step_id, expected_action=expected_action,
                               capture_before=capture_before, machine_precondition=machine_precondition)
    return completed_at


def get_capture_degraded_infra(scenario):
    """PS L2168-2181 Get-CaptureDegradedInfra: classify a continuous
    raw-hilog degradation affecting this scenario (or global scenario 0) as
    infrastructure vs non-infrastructure. Returns None when no continuous
    raw-hilog degradation applies."""
    entries = [d for d in capture_degraded if int(d['scenario']) in (scenario, 0)
               and re.search(r'raw-hilog', str(d['component']))]
    if not entries:
        return None
    infra = [d for d in entries if str(d['category']) == 'infrastructure'
             or str(d.get('infrastructure_reason')) == 'hdc-usb-interruption']
    return len(infra) > 0


def new_scenario_context(scenario):
    """PS L2135-2156 New-ScenarioContext: cross-scenario operator action
    guard runs before the context is created; the anchor byte is the current
    capture read offset."""
    global campaign_phase
    capture = campaign_capture
    if capture is None:
        raise RuntimeError('scenario-%d continuous capture is not initialized' % scenario)
    update_campaign_capture(capture)
    assert_no_stray_operator_actions({'scenario': scenario, 'capture': capture, 'started_at': get_now()})
    capture['active_scenario'] = scenario
    campaign_phase = 'scenario-%d' % scenario
    test_campaign_capture_health(capture)
    return {'scenario': scenario, 'capture': capture, 'anchor_byte': int(capture['read_offset']),
            'started_at': get_now(), 'first_action_at': None, 'last_action_completed_at': None, 'actions': []}


def wait_machine_condition(context, anchor_byte, action_at, condition, timeout_seconds=12.0):
    """PS L2183-2234 Wait-MachineCondition: polls the scenario window events
    against the postcondition until a non-pending status. A pending
    UI_START/UI_STOP-missing timeout is mechanical-action-missing invalid;
    any other pending timeout is platform-marker-missing blocked. Continuous
    capture degradation is never a status invalid (ADJ-20260808-0002 C6)."""
    deadline = get_now() + timedelta(seconds=timeout_seconds)
    last = {'status': 'pending', 'reason': 'event-postcondition-pending'}
    iterations = 0
    while get_now() < deadline:
        iterations += 1
        if iterations > 5000:
            break
        events = get_scenario_context_events(context['capture'], get_now(), anchor_byte, action_at)
        last = condition(events)
        if last is not None and str(last.get('status')) != 'pending':
            return last
        if get_now() >= deadline or context['capture']['degraded']:
            if context['capture']['degraded']:
                assert_campaign_capture_healthy(context['capture'], int(context['scenario']), 'Wait-MachineCondition')
            break
        if live_simulation:
            now = get_now()
            next_at = deadline
            for event in context['capture']['events']:
                if int(event['raw_byte_start']) < anchor_byte or not str(event.get('device_observed_at') or '').strip():
                    continue
                event_at = parse_datetime(str(event['device_observed_at']))
                if event_at is not None and event_at > now and event_at < next_at:
                    next_at = event_at
            if next_at <= now:
                next_at = now + timedelta(milliseconds=1)
            wait_until(next_at)
        else:
            time.sleep(0.25)
    pending_reason = str(get_optional_property(last, 'reason', 'event-postcondition-missing'))
    if pending_reason in ('UI_START-missing', 'UI_STOP-missing'):
        return {'status': 'invalid', 'reason': 'mechanical-action-missing:' + pending_reason}
    return {'status': 'blocked', 'reason': 'platform-marker-missing:' + pending_reason}


def test_unique_stop_condition(events, bundle, request_id):
    """PS L2282-2290 Test-UniqueStopCondition: exactly one UI_STOP for the
    bundle with the expected requestId; any UI_START / UI_START_SKIPPED /
    UI_STOP_SKIPPED in the stop step is invalid."""
    infos = [get_e3_event_info(e) for e in events]
    starts = [i for i in infos if i['marker'] in ('UI_START', 'UI_START_SKIPPED')]
    if starts:
        return {'status': 'invalid', 'reason': 'unexpected-%s-during-stop-step' % starts[0]['marker']}
    skipped = [i for i in infos if i['marker'] == 'UI_STOP_SKIPPED']
    if skipped:
        return {'status': 'invalid', 'reason': 'UI_STOP_SKIPPED'}
    stops = [i for i in infos if i['marker'] == 'UI_STOP']
    if not stops:
        return {'status': 'pending', 'reason': 'UI_STOP-missing'}
    if len(stops) != 1:
        return {'status': 'invalid', 'reason': 'expected-one-UI_STOP-observed-%d' % len(stops)}
    if str(stops[0]['bundle']) != bundle:
        return {'status': 'invalid', 'reason': 'UI_STOP-wrong-bundle:%s' % stops[0]['bundle']}
    if str(stops[0]['request_id']) != str(request_id):
        return {'status': 'invalid', 'reason': 'UI_STOP-wrong-requestId:%s' % stops[0]['request_id']}
    return {'status': 'pass', 'reason': 'unique-UI_STOP', 'request_id': str(request_id), 'bundle': bundle}


def test_no_operator_action(events):
    """PS L2282-2290 Test-NoOperatorAction: Allow/Deny/Settings navigation
    steps allow zero extra UI actions; auto StartEntry ENTRY events are not
    UI actions and stay allowed."""
    infos = [get_e3_event_info(e) for e in events]
    actions = [i for i in infos if i['marker'] in ('UI_START', 'UI_STOP', 'UI_STOP_SKIPPED')]
    if actions:
        return {'status': 'invalid', 'reason': 'unexpected-%s:bundle=%s:requestId=%s' % (actions[0]['marker'], actions[0]['bundle'], actions[0]['request_id'])}
    return {'status': 'pass', 'reason': 'no-extra-ui-action'}


def flush_simulation_gap_actions(scenario, step_index):
    """PS L2266-2288 Flush-SimulationGapActions: simulation-only injection of
    a stray operator UI action arriving in the gap between verified
    checkpoints; the guard scanning by host_observed_at sees it as unowned
    and invalidates."""
    global virtual_seconds, simulation
    if not live_simulation:
        return
    gap_actions = get_optional_property(simulation, 'gap_actions', []) or []
    new_list = []
    flushed = False
    for gap in gap_actions:
        if int(get_optional_property(gap, 'scenario', -1)) != scenario:
            new_list.append(gap)
            continue
        after_step = int(get_optional_property(gap, 'after_step_index', 0))
        if step_index <= after_step:
            new_list.append(gap)
            continue
        text = str(get_optional_property(gap, 'text', ''))
        delay = float(get_optional_property(gap, 'delay_seconds', 0.2))
        virtual_seconds += delay
        stamp = _device_stamp(get_now())
        line = text.replace('<DEVICE_OBSERVED_AT>', stamp)
        with open(campaign_capture['stdout_path'], 'a', encoding='utf-8', newline='') as f:
            f.write(line + '\n')
        flushed = True
    if flushed:
        simulation['gap_actions'] = new_list
    if flushed and campaign_capture is not None:
        update_campaign_capture(campaign_capture)


def assert_no_stray_operator_actions(context, step_index=None, step_id=None, expected_action=None):
    """PS L2290-2330 Assert-NoStrayOperatorActions: any UI_START / UI_STOP /
    UI_STOP_SKIPPED observed after the last verified checkpoint but not owned
    by the current mechanical step invalidates the scenario immediately.
    Ownership is decided by host observation time."""
    global operator_action_guard_from
    if context is None or 'capture' not in context:
        return
    flush_simulation_gap_actions(int(context['scenario']), step_index if step_index is not None else 0)
    update_campaign_capture(context['capture'])
    guard_from = operator_action_guard_from if operator_action_guard_from is not None else context['started_at']
    for event in context['capture']['events']:
        host_at = parse_datetime(str(event.get('host_observed_at') or ''))
        if host_at is None or host_at <= guard_from:
            continue
        info = get_e3_event_info(event)
        if info['marker'] in ('UI_START', 'UI_STOP', 'UI_STOP_SKIPPED'):
            throw_scenario_invalid(int(context['scenario']),
                                   'stray-operator-action:%s:bundle=%s:requestId=%s' % (info['marker'], info['bundle'], info['request_id']),
                                   step_index, step_id, expected_action)
    operator_action_guard_from = get_now()


def register_verified_request(request_id, bundle, scenario):
    """PS L2609-2620 Register-VerifiedRequest: a repeated requestId across
    scenarios makes attribution ambiguous and is invalid immediately."""
    global verified_requests
    if not str(request_id).strip() or str(request_id) == 'missing':
        throw_scenario_invalid(scenario, 'requestId-missing-cannot-register')
    if request_id in verified_requests:
        previous_bundle = verified_requests[request_id]
        if previous_bundle == bundle:
            reason = 'requestId-reused:%s' % request_id
        else:
            reason = 'requestId-reused-with-different-bundle:%s' % request_id
        throw_scenario_invalid(scenario, reason)
    verified_requests[request_id] = bundle


def invoke_mechanical_step(context, step_index, expected_action, machine_precondition, postcondition,
                           capture_before=None, capture_after_name=None, capture_after_profile=None,
                           capture_after_expected_bundle=None, capture_after_review_only=False,
                           capture_after_observation_only=False, capture_after_mismatch_is_blocked=False,
                           verify_timeout_seconds=12.0):
    """PS L2383-2472 Invoke-MechanicalStep: global operator action guard,
    machine-precondition gate (status=invalid is operator/protocol invalid;
    status=blocked is a plain runner blocked), operator prompt, optional
    capture-after, then Wait-MachineCondition against the postcondition.
    The event time lower bound for verification is the prompt (with frozen
    device clock skew tolerance), never the operator completedAt."""
    global operator_action_guard_from
    scenario = int(context['scenario'])
    step_id = uuid.uuid4().hex[:12]
    assert_no_stray_operator_actions(context, step_index, step_id, expected_action)
    pre_status = str(get_optional_property(machine_precondition, 'status', ''))
    if pre_status != 'pass':
        pre_reason = str(get_optional_property(machine_precondition, 'reason', 'unknown'))
        if pre_status == 'invalid':
            throw_scenario_invalid(scenario, 'step-%d machine-precondition-not-pass:%s' % (step_index, pre_reason),
                                   step_index, step_id, expected_action, machine_precondition, capture_before=capture_before)
        raise RuntimeError('scenario-%d machine-precondition-blocked step=%d reason=%s' % (scenario, step_index, pre_reason))
    update_campaign_capture(context['capture'])
    step_anchor = int(context['capture']['read_offset'])
    prompt_at = get_now()
    if context['first_action_at'] is None:
        context['first_action_at'] = prompt_at
    completed_at = read_operator_enter(scenario, step_index, step_id, expected_action, machine_precondition, capture_before)
    context['last_action_completed_at'] = completed_at
    add_simulation_scenario_step_output(context['capture'], scenario, step_index, prompt_at, completed_at)
    update_campaign_capture(context['capture'])
    capture_after = {'status': 'not-required'}
    if capture_after_name:
        if capture_after_review_only:
            capture_status = invoke_capture(capture_after_name, scenario, observation_only=True)
            capture_after = {'status': capture_status, 'name': capture_after_name, 'review_only': True,
                             'note': 'review-only capture; never a semantic operator verdict'}
            add_transcript_record('review-only-layout-artifact', {'scenario': scenario, 'checkpoint': capture_after})
        elif capture_after_profile:
            capture_after = invoke_layout_checkpoint(scenario, capture_after_name, capture_after_profile,
                                                     capture_after_expected_bundle, step_index, step_id,
                                                     expected_action, observation_only=capture_after_observation_only,
                                                     mismatch_is_blocked=capture_after_mismatch_is_blocked)
        else:
            capture_status = invoke_capture(capture_after_name, scenario, observation_only=capture_after_observation_only)
            if capture_status != 'collected':
                if last_capture_infrastructure:
                    raise RuntimeError('HDC infrastructure interruption capture-after=%s scenario=%d' % (capture_after_name, scenario))
                throw_scenario_invalid(scenario, 'step-%d capture-after-not-collected:%s' % (step_index, capture_after_name),
                                       step_index=step_index, step_id=step_id, expected_action=expected_action,
                                       machine_precondition=machine_precondition, capture_before=capture_before,
                                       capture_after={'status': capture_status, 'name': capture_after_name})
            capture_after = {'status': capture_status, 'name': capture_after_name}
    write_operator_wait_state('verifying', scenario=scenario, step_index=step_index, step_id=step_id,
                              expected_action=expected_action, capture_before=capture_before,
                              capture_after=capture_after, machine_precondition=machine_precondition,
                              machine_postcondition={'status': 'verifying'})
    outcome = wait_machine_condition(context, step_anchor, prompt_at, postcondition, verify_timeout_seconds)
    if str(outcome.get('status')) == 'blocked':
        reason = str(get_optional_property(outcome, 'reason', 'machine-verification-blocked'))
        raise RuntimeError('scenario-%d machine-verification-blocked step=%d reason=%s' % (scenario, step_index, reason))
    if str(outcome.get('status')) != 'pass':
        reason = str(get_optional_property(outcome, 'reason', 'event-postcondition-missing'))
        throw_scenario_invalid(scenario, 'step-%d %s' % (step_index, reason), step_index, step_id,
                               expected_action, machine_precondition, outcome, capture_before, capture_after)
    write_operator_wait_state('captured', scenario=scenario, step_index=step_index, step_id=step_id,
                              expected_action=expected_action, capture_before=capture_before,
                              capture_after=capture_after, machine_precondition=machine_precondition,
                              machine_postcondition=outcome)
    print('机器采集/判定完成：scenario=%d step=%d。' % (scenario, step_index))
    step = {'step_index': step_index, 'step_id': step_id, 'expected_action': expected_action,
            'prompt_at': prompt_at, 'completed_at': completed_at, 'anchor_byte': step_anchor,
            'outcome': outcome, 'capture_before': capture_before, 'capture_after': capture_after}
    context['actions'].append(step)
    operator_action_guard_from = get_now()
    return step


def complete_scenario_context(context, during_wait=None):
    """PS L2473-2545 Complete-ScenarioContext: observes the full 60s window
    after the last action (virtual clock under simulation), runs the optional
    during-wait callback (S3/S7 probe series), and builds the scenario
    observation record (C15) with redacted events."""
    global current_window_end, operator_action_guard_from
    capture = context['capture']
    action_completed_at = context['last_action_completed_at'] if context['last_action_completed_at'] is not None else context['started_at']
    required_end = action_completed_at + timedelta(seconds=WINDOW_SECONDS)
    current_window_end = required_end
    window_iterations = 0
    while get_now() < required_end and not capture['degraded']:
        window_iterations += 1
        if window_iterations > 5000:
            break
        events = get_scenario_context_events(capture, None, context['anchor_byte'], context['started_at'])
        if during_wait is not None:
            during_wait(events)
        if get_now() >= required_end or capture['degraded']:
            break
        if live_simulation:
            now = get_now()
            next_at = required_end
            for event in capture['events']:
                if int(event['raw_byte_start']) < int(context['anchor_byte']) or not str(event.get('device_observed_at') or '').strip():
                    continue
                event_at = parse_datetime(str(event['device_observed_at']))
                if event_at is not None and event_at > now and event_at < next_at:
                    next_at = event_at
            if next_at <= now:
                next_at = now + timedelta(milliseconds=1)
            wait_until(next_at)
        else:
            time.sleep(0.25)
    update_campaign_capture(capture)
    observed_through = get_now()
    events = get_scenario_context_events(capture, observed_through, context['anchor_byte'], context['started_at'])
    if during_wait is not None:
        during_wait(events)
    current_window_end = None
    operator_action_guard_from = get_now()
    test_campaign_capture_health(capture)
    window_degraded = bool(capture['degraded'])
    if capture['last_healthy_at'] is not None:
        coverage_after_action = max(0.0, (parse_datetime(capture['last_healthy_at']) - action_completed_at).total_seconds())
    else:
        coverage_after_action = 0.0
    complete_window_observed = (not window_degraded) and coverage_after_action >= WINDOW_SECONDS \
        and observed_through >= required_end
    first_action_at = context['first_action_at'] if context['first_action_at'] is not None else context['started_at']
    observation = {
        'scenario': int(context['scenario']),
        'protocol': 'mechanical-action-only-machine-verified-v1',
        'campaign_capture_started_at': capture['started_at'],
        'initial_anchor': capture['initial_anchor'],
        'scenario_anchor_byte': int(context['anchor_byte']),
        'window_started_at': isoformat_7(context['started_at']),
        'action_prompt_at': isoformat_7(first_action_at),
        'action_completed_at': isoformat_7(action_completed_at),
        'required_observation_end_at': isoformat_7(required_end),
        'observation_ended_at': isoformat_7(observed_through),
        'action_interval_seconds': (action_completed_at - first_action_at).total_seconds(),
        'measured_coverage_before_action_prompt_seconds': (first_action_at - context['started_at']).total_seconds(),
        'measured_coverage_after_action_seconds': coverage_after_action,
        'complete_window_observed': bool(complete_window_observed),
        'operator_steps': [{'step_index': s['step_index'], 'step_id': s['step_id'],
                            'expected_action': s['expected_action'], 'completed_at': isoformat_7(s['completed_at']),
                            'machine_postcondition': s['outcome']} for s in context['actions']],
        'capture_degraded': bool(window_degraded),
        'capture_health': {
            'process_present': capture['process'] is not None,
            'process_exited': (capture['process'].poll() is not None) if capture['process'] is not None else bool(capture['simulated_dead']),
            'stderr_bytes': int(capture['last_stderr_bytes']),
            'last_healthy_at': capture['last_healthy_at'],
            'measured': True,
        },
        'device_clock_skew_tolerance_seconds': DEVICE_CLOCK_SKEW_TOLERANCE_SECONDS,
        'events': protect_sensitive_data(events),
    }
    add_transcript_record('scenario-observation', observation)
    capture['active_scenario'] = 0
    return {'observation': observation, 'events': events, 'capture_degraded': window_degraded,
            'complete_window_observed': complete_window_observed}


def assert_scenario_event_contract(scenario, events, expected_starts=None, expected_stops=None):
    """PS L2547-2591 Assert-ScenarioEventContract: exact UI_START/UI_STOP
    counts, order, bundle and requestId; every marker with a requestId must be
    in the allowed set with the right bundle."""
    if expected_starts is None:
        expected_starts = []
    if expected_stops is None:
        expected_stops = []
    infos = [get_e3_event_info(e) for e in events]
    skipped = [i for i in infos if i['marker'] in ('UI_START_SKIPPED', 'UI_STOP_SKIPPED')]
    if skipped:
        throw_scenario_invalid(scenario, 'unexpected-%s' % skipped[0]['marker'])
    starts = [i for i in infos if i['marker'] == 'UI_START']
    if len(starts) != len(expected_starts):
        throw_scenario_invalid(scenario, 'UI_START-count expected=%d actual=%d' % (len(expected_starts), len(starts)))
    for index, expected in enumerate(expected_starts):
        if str(starts[index]['bundle']) != str(expected['bundle']):
            throw_scenario_invalid(scenario, 'UI_START-order-or-bundle expected=%s actual=%s' % (expected['bundle'], starts[index]['bundle']))
        if str(starts[index]['request_id']) != str(expected['request_id']):
            throw_scenario_invalid(scenario, 'UI_START-requestId expected=%s actual=%s' % (expected['request_id'], starts[index]['request_id']))
    stops = [i for i in infos if i['marker'] == 'UI_STOP']
    if len(stops) != len(expected_stops):
        throw_scenario_invalid(scenario, 'UI_STOP-count expected=%d actual=%d' % (len(expected_stops), len(stops)))
    for index, expected in enumerate(expected_stops):
        if str(stops[index]['bundle']) != str(expected['bundle']):
            throw_scenario_invalid(scenario, 'UI_STOP-bundle expected=%s actual=%s' % (expected['bundle'], stops[index]['bundle']))
        if str(stops[index]['request_id']) != str(expected['request_id']):
            throw_scenario_invalid(scenario, 'UI_STOP-requestId expected=%s actual=%s' % (expected['request_id'], stops[index]['request_id']))
    allowed = {}
    for expected in list(expected_starts) + list(expected_stops):
        allowed[str(expected['request_id'])] = str(expected['bundle'])
    for info in infos:
        if not info['marker'] or not re.match(r'^(UI|VPN|CREATE|START|STOP|DESTROY|FD|SESSION|PROMISE|ENTRY|LATE)_', info['marker']):
            continue
        if info['request_id'] and str(info['request_id']) != 'missing':
            if str(info['request_id']) not in allowed:
                throw_scenario_invalid(scenario, 'unexpected-requestId:%s marker=%s' % (info['request_id'], info['marker']))
            expected_bundle = allowed[str(info['request_id'])]
            if info['bundle'] and str(info['bundle']) != expected_bundle:
                throw_scenario_invalid(scenario, 'wrong-bundle-for-requestId:%s' % info['request_id'])
    return True


def get_accepted_marker_assessment(events, verified_requests):
    """Count CREATE_ACCEPTED markers only when their requestId exactly matches
    a machine-verified request. Missing and foreign requestIds are unexpected;
    they never contribute to accepted_session_count_in_window."""
    counts = [0 for _ in verified_requests]
    unexpected = []
    for event in events:
        text = str(event.get('text', '')) if isinstance(event, dict) else str(event)
        if not re.search(r'CREATE_ACCEPTED', text):
            continue
        matched = False
        for index, verified in enumerate(verified_requests):
            request_id = str(verified['request_id'])
            if re.search(r'requestId=%s(\||\s|$)' % re.escape(request_id), text):
                counts[index] += 1
                matched = True
                break
        if not matched:
            unexpected.append(event)
    return {'counts': counts, 'accepted_count': sum(counts), 'unexpected': unexpected}


def get_request_id_from_events(events, bundle):
    """PS L2593-2597 Get-RequestIdFromEvents."""
    for event in events:
        text = str(event['text']) if isinstance(event, dict) else str(event)
        m = re.search(r'UI_START\|bundle=%s\|requestId=([^|\s]+)' % re.escape(bundle), text)
        if m and m.group(1) != 'missing':
            return m.group(1)
    return None


def _device_stamp(dt):
    """PS (Get-Now).ToString('yyyy-MM-dd HH:mm:ss.fffzzz') equivalent for
    simulation fixture event stamps (R16)."""
    return dt.isoformat(timespec='milliseconds').replace('T', ' ')


def get_simulation_event_step_index(scenario, item):
    """PS L2019-2036 Get-SimulationEventStepIndex: fixture events without an
    explicit step_index are attributed to the mechanical step by scenario
    rules (S5 destroy-side events belong to step 4; S6 B-side events to
    step 3)."""
    explicit = get_optional_property(item, 'step_index', None)
    if explicit is not None:
        return int(explicit)
    text = str(get_optional_property(item, 'text', ''))
    if scenario == 2:
        return 1 if re.search(r'UI_START\|', text) else 2
    if scenario == 4:
        return 1 if re.search(r'UI_START\|', text) else 2
    if scenario == 5:
        return 4 if re.search(r'VPN_DESTROY_|VPN_ONDESTROY', text) else 1
    if scenario == 6:
        if re.search(r'bundle=%s|requestId=b' % re.escape(BUNDLE_B), text):
            return 3
        return 1
    return 1


def test_simulation_step_has_effect(scenario, step_index):
    """PS L2038-2043 Test-SimulationStepHasEffect: no_effect_steps fixture
    knob (live mode always has effect)."""
    if not live_simulation:
        return True
    no_effect = [str(x) for x in (get_optional_property(get_optional_property(simulation, 'operator'), 'no_effect_steps', []) or [])]
    return '%d.%d' % (scenario, step_index) not in no_effect


def add_simulation_scenario_step_output(capture, scenario, step_index, action_prompt_at, action_completed_at):
    """PS L2045-2076 Add-SimulationScenarioStepOutput: appends the fixture
    events for the completed mechanical step to the capture stdout file with
    device stamps (relative_to_prompt events are stamped against the prompt
    so a slow operator cannot shift device timestamps past the enter)."""
    global simulation_scenario_steps_written
    if not live_simulation:
        return
    key = '%d.%d' % (scenario, step_index)
    if key in simulation_scenario_steps_written:
        return
    simulation_scenario_steps_written[key] = True
    scenario_events = get_optional_property(simulation, 'scenario_events')
    items = [item for item in (get_optional_property(scenario_events, str(scenario), []) or [])
             if get_simulation_event_step_index(scenario, item) == step_index]
    items.sort(key=lambda item: float(get_optional_property(item, 'offset_seconds', 0.0)))
    minimum_offset = float(get_optional_property(items[0], 'offset_seconds', 0.0)) if items else 0.0
    for item in items:
        offset = float(get_optional_property(item, 'offset_seconds', 0.0))
        relative_to_prompt = get_optional_json_boolean(item, 'relative_to_prompt', False)
        base = action_prompt_at if relative_to_prompt else action_completed_at
        if relative_to_prompt:
            relative_offset = 0.2 + max(0.0, offset)
        else:
            relative_offset = 0.2 + max(0.0, offset - minimum_offset)
        device_stamp = _device_stamp(base + timedelta(seconds=relative_offset))
        text = str(get_optional_property(item, 'text', '')).replace('<DEVICE_OBSERVED_AT>', device_stamp)
        without_newline = get_optional_json_boolean(item, 'append_without_newline', False)
        with open(capture['stdout_path'], 'a', encoding='utf-8', newline='') as f:
            f.write(text + ('' if without_newline else '\n'))
    die_scenario = int(get_optional_property(simulation, 'capture_die_scenario', 0))
    if die_scenario == scenario and step_index == 1:
        capture['simulated_dead'] = True


def new_process_probe_context(scenario, bundle, require_bundle_present=False, required_count=2, spacing_seconds=3.0):
    """PS L2764-2780 New-ProcessProbeContext: probe target is the
    <bundle>:vpn Extension ability process (ADJ-20260808-0001 C6)."""
    return {'scenario': scenario, 'bundle': bundle, 'process_target': '%s:vpn' % bundle,
            'require_bundle_present': bool(require_bundle_present), 'required_count': int(required_count),
            'spacing_seconds': float(spacing_seconds), 'started': False, 'finished': False,
            'aborted': False, 'terminal': False, 'consecutive_absent': 0, 'bundle_present': False,
            'probes': [], 'last_probe_at': None, 'override_probe_index': 0}


def get_simulation_failure_match(operation, occurrence):
    """PS L2808-2810 Get-SimulationFailureMatch: hdc_failures always win over
    process_probe_override."""
    for failure in (get_optional_property(simulation, 'hdc_failures', []) or []):
        if str(get_optional_property(failure, 'operation', '')) == operation \
                and int(get_optional_property(failure, 'occurrence', 1)) == occurrence:
            return HdcResult(int(get_optional_property(failure, 'exit_code', 1)),
                             str(get_optional_property(failure, 'stdout', '')),
                             str(get_optional_property(failure, 'stderr', 'simulated command failure')), True)
    return None


def get_process_probe_override_result(operation, bundle, entry=None):
    """PS L2808-2840 Get-ProcessProbeOverrideResult: strict enum only;
    unknown values deliberately produce an unknown classification, never a
    default absent/present pass."""
    if operation == 'PidOf':
        pid_status = 'absent' if entry is None else str(get_optional_property(entry, 'pid', ''))
        if pid_status == 'present':
            return HdcResult(0, '12345', '', True)
        if pid_status == 'absent':
            return HdcResult(1, '', '', True)
        if pid_status == 'error':
            return HdcResult(124, '', 'simulated pidof error', True)
        if pid_status == 'unknown':
            return HdcResult(2, '', '', True)
        return HdcResult(3, 'garbage-override', 'invalid-pid-override', True)
    dump_status = 'present' if entry is None else str(get_optional_property(entry, 'dump', ''))
    if dump_status == 'present':
        return HdcResult(0, '{ "app": { "bundleName": "%s" } }' % bundle, '', True)
    if dump_status == 'absent':
        return HdcResult(0, 'error: failed to get information and the parameters may be wrong.', '', True)
    if dump_status == 'error':
        return HdcResult(124, '', 'simulated dump error', True)
    if dump_status == 'unknown':
        return HdcResult(0, 'Permission denied', '', True)
    return HdcResult(3, 'garbage-override', 'invalid-dump-override', True)


def invoke_simulation_probe_pair(context):
    """PS L2842-2900 Invoke-SimulationProbePair: two logical HDC operations
    (PidOf + BundleDump) with process_probe_override and hdc_failures;
    infrastructure classification matches live Invoke-HdcOperation."""
    global hdc_logical_call_count, infrastructure_reason_observed
    bundle = context['bundle']
    scenario = int(context['scenario'])
    hdc_logical_call_count += 2
    pid_audit = get_hdc_invocation('PidOf', {'Bundle': bundle})
    dump_audit = get_hdc_invocation('BundleDump', {'Bundle': bundle})
    add_transcript_record('hdc-command', {'operation': 'PidOf', 'executable': '<HDC_PATH>',
                                           'arguments': pid_audit, 'timeout_seconds': hdc_timeout_seconds, 'simulated': True})
    add_transcript_record('hdc-command', {'operation': 'BundleDump', 'executable': '<HDC_PATH>',
                                          'arguments': dump_audit, 'timeout_seconds': hdc_timeout_seconds, 'simulated': True})
    scenario_override = get_optional_property(get_optional_property(simulation, 'process_probe_override', {}), str(scenario), None)
    entry = None
    if scenario_override is not None:
        entries = [scenario_override] if not isinstance(scenario_override, list) else scenario_override
        index = int(context['override_probe_index'])
        if entries:
            entry = entries[index] if index < len(entries) else entries[-1]
        context['override_probe_index'] = index + 1
    if 'PidOf' not in hdc_operation_counts:
        hdc_operation_counts['PidOf'] = 0
    if 'BundleDump' not in hdc_operation_counts:
        hdc_operation_counts['BundleDump'] = 0
    hdc_operation_counts['PidOf'] += 1
    hdc_operation_counts['BundleDump'] += 1
    pid_occurrence = hdc_operation_counts['PidOf']
    dump_occurrence = hdc_operation_counts['BundleDump']
    pid_failure = get_simulation_failure_match('PidOf', pid_occurrence)
    dump_failure = get_simulation_failure_match('BundleDump', dump_occurrence)
    if pid_failure is not None:
        pid_result = pid_failure
    elif scenario_override is not None:
        pid_result = get_process_probe_override_result('PidOf', bundle, entry)
    else:
        pid_result = HdcResult(0, '', '', True)
    if dump_failure is not None:
        dump_result = dump_failure
    elif scenario_override is not None:
        dump_result = get_process_probe_override_result('BundleDump', bundle, entry)
    else:
        installed = (bundle == BUNDLE_A and simulation_installed_a) or (bundle == BUNDLE_B and simulation_installed_b)
        if installed:
            dump_result = HdcResult(0, '{ "app": { "bundleName": "%s" } }' % bundle, '', True)
        else:
            dump_result = HdcResult(0, 'error: failed to get information and the parameters may be wrong.', '', True)
    for pair in ({'op': 'PidOf', 'result': pid_result}, {'op': 'BundleDump', 'result': dump_result}):
        add_transcript_record('hdc-result', {'operation': pair['op'], 'exit_code': pair['result'].exit_code,
                                             'stdout': str(pair['result'].stdout), 'stderr': str(pair['result'].stderr),
                                             'simulated': True})
    for probe_result in (pid_result, dump_result):
        if probe_result.exit_code in (124, 125) or re.search(r'(?i)\btimeout\b', str(probe_result.stderr)):
            infrastructure_reason_observed = 'hdc-usb-interruption'
    return {'pid_result': pid_result, 'dump_result': dump_result}


def invoke_process_probe_pair(context):
    """PS L2901-2908 Invoke-ProcessProbePair."""
    bundle = context['bundle']
    if live_simulation:
        return invoke_simulation_probe_pair(context)
    pid_result = invoke_hdc_operation('PidOf', {'Bundle': bundle}, allow_failure=True)
    dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
    return {'pid_result': pid_result, 'dump_result': dump_result}


def invoke_process_final_state_probe_series(context, deadline=None):
    """PS L2910-2960 Invoke-ProcessFinalStateProbeSeries: window-bound probe
    series (never opens a fresh post-window 60s series). The recorded
    spacing reaches the frozen rule via a +0.1s scheduling margin; the rule
    threshold itself is never lowered and Test-ProcessAbsentEvidence
    re-checks recorded timestamps."""
    global current_window_end
    if context['finished']:
        return context
    if deadline is None:
        if current_window_end is not None:
            deadline = current_window_end
        else:
            return context
    if get_now() >= deadline:
        return context
    context['started'] = True
    spacing_seconds = float(context['spacing_seconds'])
    if live_simulation:
        override_spacing = get_optional_property(simulation, 'probe_spacing_override_seconds', None)
        if override_spacing is not None:
            spacing_seconds = float(override_spacing)
    while not context['finished'] and not context['aborted']:
        if context['last_probe_at'] is not None:
            next_probe_at = context['last_probe_at'] + timedelta(seconds=spacing_seconds + 0.1)
            if get_now() < next_probe_at:
                wait_until(next_probe_at)
        if get_now() >= deadline:
            break
        probe_at = get_now()
        pair = invoke_process_probe_pair(context)
        classification = get_process_probe_status(pair['pid_result'], pair['dump_result'], context['bundle'])
        status = str(classification['status'])
        if status == 'present':
            context['consecutive_absent'] = 0
        elif status == 'absent':
            context['consecutive_absent'] += 1
            context['bundle_present'] = bool(classification['bundle_present'])
        elif status in ('error', 'unknown'):
            context['aborted'] = True
            context['finished'] = True
        previous = context['probes'][-1] if context['probes'] else None
        if previous is not None:
            spacing_since_previous = (probe_at - parse_datetime(str(previous['time']))).total_seconds()
        else:
            spacing_since_previous = 0.0
        probe_record = {'time': isoformat_7(probe_at), 'status': status, 'detail': classification['detail'],
                        'process_target': context['process_target'], 'bundle_present': bool(classification['bundle_present']),
                        'consecutive_absent': int(context['consecutive_absent']),
                        'spacing_seconds_since_previous': spacing_since_previous}
        context['probes'].append(probe_record)
        add_transcript_record('process-final-state-probe', {'scenario': int(context['scenario']),
                                                            'bundle': context['bundle'], 'probe': probe_record})
        if int(context['consecutive_absent']) >= int(context['required_count']) and not context['aborted']:
            absent_tail = [p for p in context['probes'] if str(p['status']) == 'absent'][-int(context['required_count']):]
            if len(absent_tail) >= int(context['required_count']):
                first_absent_at = parse_datetime(str(absent_tail[0]['time']))
                last_absent_at = parse_datetime(str(absent_tail[-1]['time']))
                if first_absent_at is not None and last_absent_at is not None \
                        and (last_absent_at - first_absent_at).total_seconds() >= (float(context['spacing_seconds']) - 0.001):
                    context['terminal'] = True
                    context['finished'] = True
        context['last_probe_at'] = probe_at
    return context


def new_blocked_scenarios(reason):
    """PS L3173-3183 New-BlockedScenarios: 7 blocked scenario entries (S2
    carries the assertions shape)."""
    items = []
    for number in range(1, 8):
        entry = {'sequence_index': number, 'scenario': number, 'result': 'blocked', 'reason': reason}
        if number == 2:
            entry['assertions'] = {'allow': 'blocked', 'vpn_on_create': 'blocked', 'vpn_connection_create_fd': 'blocked'}
        items.append(entry)
    return items


def assert_campaign_capture_healthy(capture, scenario=None, origin=None):
    """PS L3185-3208 Assert-CampaignCaptureHealthy: a continuously degraded
    CampaignCapture (raw-hilog) is NEVER a scenario-invalid input on any
    path; infrastructure degradation authorizes the USB retry, non-infra
    stays a plain blocked. No-op when the capture is healthy."""
    global infrastructure_reason_observed
    if capture is None or not capture['degraded']:
        return
    scenario_number = 0 if scenario is None else int(scenario)
    continuous_infra = get_capture_degraded_infra(scenario_number)
    if continuous_infra is True:
        infra_entry = [d for d in capture_degraded if str(d['category']) == 'infrastructure'][:1]
        detail = protect_sensitive_text(str(get_optional_property(infra_entry[0], 'reason', 'capture process degraded')) if infra_entry else 'capture process degraded')
        infrastructure_reason_observed = 'hdc-usb-interruption'
        raise RuntimeError('scenario-%d continuous capture infrastructure failure: %s' % (scenario_number, detail))
    entry = [d for d in capture_degraded if re.search(r'raw-hilog', str(d['component']))][:1]
    detail = protect_sensitive_text(str(get_optional_property(entry[0], 'reason', 'continuous capture degraded')) if entry else 'continuous capture degraded')
    raise RuntimeError('scenario-%d continuous capture non-infrastructure blocked: %s' % (scenario_number, detail))


def assert_scenario_capture_can_continue(results, observation):
    """PS L3210-3219 Assert-ScenarioCaptureCanContinue: a scenario whose
    observation window was shortened by continuous raw-hilog degradation is a
    runner blocked (infra or non-infra), never a scenario invalid."""
    global partial_scenarios
    partial_scenarios = list(results)
    if not observation['capture_degraded']:
        return
    scenario_number = int(observation['observation']['scenario'])
    assert_campaign_capture_healthy(campaign_capture, scenario_number, 'Assert-ScenarioCaptureCanContinue')


def invoke_dry_run_campaign():
    """PS L3695-3710 Invoke-DryRunCampaign (C17: 20 planned operations in
    fixed order). DryRun Invoke-HdcOperation returns DRY_RUN_NOT_EXECUTED
    without starting a process (R19)."""
    for operation, parameters in DRY_RUN_PLAN:
        invoke_hdc_operation(operation, parameters)
    return new_blocked_scenarios('dry-run-no-device-non-evidence')


def invoke_strong_live_campaign(freeze):
    """PS L3711-4197 Invoke-StrongLiveCampaign (S1-S7, C18 fresh double
    anchor). Live and LiveSimulation share this path; the simulation layer
    (get_simulation_hdc_result / fixture events / virtual clock) supplies
    the device behavior. Returns the scenario result list (the U7 seal
    projection input)."""
    global partial_scenarios, campaign_phase, campaign_started, installed_a, installed_b
    global staging_sent, staging_may_exist, probe_contexts, current_window_end
    global simulation_active_bundles, cleanup_actions, cleanup_verification
    results = []
    campaign_phase = 'preflight'
    version_result = invoke_hdc_operation('Version')
    if str(version_result.stdout).strip() != str(freeze['hdc']['version']):
        raise RuntimeError('preflight: frozen HDC version mismatch')
    model_result = invoke_hdc_operation('TupleModel')
    build_result = invoke_hdc_operation('TupleBuild')
    if str(model_result.stdout).strip() != str(freeze['target_tuple']['device_model']) \
            or str(build_result.stdout).strip() != str(freeze['target_tuple']['full_system_build']):
        raise RuntimeError('preflight: model/build precheck drifted before continuous capture')
    campaign_capture = start_campaign_hilog_capture()
    initialize_campaign_capture_anchor(campaign_capture)
    if campaign_capture['degraded']:
        raise RuntimeError('collection preparation blocked: continuous capture unavailable before scenario-1 installation')

    # S1 is fully machine-operated; the operator is not asked to attest an
    # installation fact.
    context1 = new_scenario_context(1)
    first_baseline_query_at = get_now()
    for bundle in (BUNDLE_A, BUNDLE_B):
        dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
        if not re.search(r'failed to get information|not exist|not found', dump_result.combined_text()):
            raise RuntimeError('cleanup baseline failed: bundle already installed or query unavailable: %s' % bundle)
        process_result = invoke_hdc_operation('PidOf', {'Bundle': bundle}, allow_failure=True)
        if process_result.exit_code not in (0, 1) or str(process_result.stderr).strip() \
                or str(process_result.stdout).strip():
            raise RuntimeError('cleanup baseline failed: extension process state is not absent: %s' % bundle)
    invoke_hdc_operation('RemoveStaging', allow_failure=True)
    if not test_staging_absent(invoke_hdc_operation('StagingProbe', allow_failure=True)):
        raise RuntimeError('cleanup baseline failed: staging residual still present after RemoveStaging')
    staging_may_exist = True
    invoke_hdc_operation('MkdirStaging')
    staging_sent = True
    invoke_hdc_operation('SendA')
    invoke_hdc_operation('SendB')
    campaign_started = True
    confirm_bundle_installed('InstallA', BUNDLE_A, 'A')
    installed_a = True
    confirm_bundle_installed('InstallB', BUNDLE_B, 'B')
    installed_b = True
    install_completed_at = get_now()
    observation1 = complete_scenario_context(context1)
    assert_scenario_event_contract(1, observation1['events'])
    capture1 = invoke_capture('scenario-1-baseline', 1)
    install_seconds = (install_completed_at - context1['started_at']).total_seconds()
    scenario1_result = 'pass' if (observation1['complete_window_observed'] and not observation1['capture_degraded']
                                  and capture1 == 'collected' and install_seconds <= 60 and installed_a and installed_b) else 'blocked'
    results.append({'sequence_index': 1, 'scenario': 1, 'result': scenario1_result,
                    'reason': 'machine-cleanup-baseline-and-install',
                    'first_baseline_query_covered': first_baseline_query_at >= context1['started_at'],
                    'install_elapsed_seconds': install_seconds,
                    'install_completed_within_60_seconds': install_seconds <= 60,
                    'observation': observation1['observation']})
    assert_scenario_capture_can_continue(results, observation1)

    # S2: runner opens A, verifies the Entry layout, then allows exactly
    # Start and Allow.
    invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_A})
    entry2 = invoke_layout_checkpoint(2, 'scenario-2-entry-a', 'entry', BUNDLE_A)
    pre2 = get_exact_process_checkpoint([])
    context2 = new_scenario_context(2)
    step2_start = invoke_mechanical_step(context2, 1, '点击测试 App A 的 Start', pre2,
                                         lambda events: test_unique_start(events, BUNDLE_A), capture_before=entry2)
    request2 = str(step2_start['outcome']['request_id'])
    register_verified_request(request2, BUNDLE_A, 2)
    auth2 = invoke_layout_checkpoint(2, 'scenario-2-authorization', 'authorization', BUNDLE_A,
                                     step_index=2, step_id=step2_start['step_id'], expected_action='点击 Allow')

    def _s2_allow_postcondition(events):
        extra = test_unique_start(events, BUNDLE_A)
        if str(extra['status']) == 'pass' or str(extra['reason']) not in ('UI_START-missing',):
            return {'status': 'invalid', 'reason': 'unexpected-Start-or-Stop-after-Allow'}
        terminal = (test_correlated_marker(events, BUNDLE_A, request2, 'CREATE_ACCEPTED')
                    or test_correlated_marker(events, BUNDLE_A, request2, 'VPN_CREATE_REJECTED')
                    or test_correlated_marker(events, BUNDLE_A, request2, 'VPN_CREATE_INVALID_FD')
                    or test_correlated_marker(events, BUNDLE_A, request2, 'START_PROMISE_REJECTED'))
        if terminal:
            return {'status': 'pass', 'reason': 'create-terminal-observed', 'request_id': request2}
        return {'status': 'pending', 'reason': 'create-terminal-missing-after-Allow'}

    step2_allow = invoke_mechanical_step(context2, 2, '点击 Allow',
                                         {'status': 'pass', 'reason': 'authorization-layout-verified', 'request_id': request2},
                                         _s2_allow_postcondition, capture_before=auth2,
                                         capture_after_name='scenario-2-after-allow',
                                         capture_after_profile='authorization-dismissed',
                                         capture_after_expected_bundle=BUNDLE_A)
    after_allow = _assess_layout_profile('scenario-2-after-allow', 'authorization-dismissed', BUNDLE_A)
    if str(after_allow['status']) != 'pass':
        throw_scenario_invalid(2, 'authorization-not-dismissed-after-Allow:%s' % after_allow['reason'],
                               step_index=2, step_id=step2_allow['step_id'],
                               expected_action=step2_allow['expected_action'], machine_postcondition=after_allow)
    observation2 = complete_scenario_context(context2)
    assert_scenario_event_contract(2, observation2['events'], [{'bundle': BUNDLE_A, 'request_id': request2}])
    on_create2 = test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_ONCREATE')
    accepted2 = (test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_CREATE_RESOLVED')
                 and test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'CREATE_ACCEPTED')
                 and test_post_create_open(observation2['events'], BUNDLE_A, request2))
    extension_reject2 = (test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_CREATE_REJECTED')
                         or test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_CREATE_INVALID_FD')) and on_create2
    auth_unclassified2 = (test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_CREATE_REJECTED')
                          or test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'VPN_CREATE_INVALID_FD')
                          or test_correlated_marker(observation2['events'], BUNDLE_A, request2, 'START_PROMISE_REJECTED')) and not extension_reject2
    if extension_reject2:
        scenario2_result = 'fail'
    elif auth_unclassified2:
        scenario2_result = 'blocked'
    elif not observation2['complete_window_observed'] or observation2['capture_degraded']:
        scenario2_result = 'blocked'
    elif on_create2 and accepted2:
        scenario2_result = 'pass'
    else:
        scenario2_result = 'blocked'
    process_target2 = None
    if scenario2_result == 'pass':
        simulation_active_bundles.add(BUNDLE_A)
        process_target2 = get_process_target_checkpoint(BUNDLE_A)
        if str(process_target2['status']) != 'pass':
            scenario2_result = 'blocked'
    if extension_reject2:
        s2_reason = 'create-rejected-after-Allow'
    elif auth_unclassified2:
        s2_reason = 'authorization-outcome-unclassified'
    elif process_target2 is not None and str(process_target2['status']) != 'pass':
        s2_reason = str(process_target2['reason'])
    else:
        s2_reason = 'machine-verified-Allow-onCreate-create-fd'
    results.append({'sequence_index': 2, 'scenario': 2, 'result': scenario2_result, 'reason': s2_reason,
                    'bundle': BUNDLE_A, 'request_id': request2,
                    'process_target_verified': (str(process_target2['status']) == 'pass') if process_target2 is not None else None,
                    'process_target_checkpoint': process_target2,
                    'assertions': {'allow': 'blocked' if auth_unclassified2 else 'pass',
                                   'vpn_on_create': 'pass' if on_create2 else 'blocked',
                                   'vpn_connection_create_fd': 'pass' if accepted2 else ('fail' if extension_reject2 else 'blocked')},
                    'authorization_capture': {'name': 'scenario-2-authorization', 'status': 'collected',
                                              'result': 'pass', 'layout_checkpoint': auth2},
                    'observation': observation2['observation']})
    assert_scenario_capture_can_continue(results, observation2)

    # S3 consumes only the machine-verified S2 request and active bundle. No
    # second Start is legal.
    if scenario2_result != 'pass':
        results.append({'sequence_index': 3, 'scenario': 3, 'result': 'blocked',
                        'reason': 'S2-machine-active-checkpoint-unavailable', 'bundle': BUNDLE_A,
                        'request_id': request2, 'clean_reactivation_proof': None, 'process_target_verified': None})
    else:
        invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_A})
        entry3 = invoke_layout_checkpoint(3, 'scenario-3-entry-a', 'entry', BUNDLE_A)
        pre3 = get_exact_process_checkpoint([BUNDLE_A])
        context3 = new_scenario_context(3)
        step3_stop = invoke_mechanical_step(context3, 1, '点击测试 App A 的 Stop', pre3,
                                            lambda events: test_unique_stop_condition(events, BUNDLE_A, request2),
                                            capture_before=entry3, capture_after_name='scenario-3-after-stop',
                                            capture_after_review_only=True)
        if test_simulation_step_has_effect(3, 1):
            simulation_active_bundles.discard(BUNDLE_A)
        probe_contexts[3] = new_process_probe_context(3, BUNDLE_A, require_bundle_present=True,
                                                     required_count=int(freeze['process_absent_required_count']),
                                                     spacing_seconds=float(freeze['process_absent_probe_spacing_seconds']))

        def _s3_during(events):
            if test_strict_fallback_prerequisites(events, BUNDLE_A, request2)['met']:
                invoke_process_final_state_probe_series(probe_contexts[3], current_window_end)

        observation3 = complete_scenario_context(context3, _s3_during)
        assert_scenario_event_contract(3, observation3['events'], [], [{'bundle': BUNDLE_A, 'request_id': request2}])
        has_destroy_begin3 = test_correlated_marker(observation3['events'], BUNDLE_A, request2, 'VPN_DESTROY_BEGIN') \
            or len([e for e in observation3['events'] if re.search(r'VPN_FD_SNAPSHOT', str(e['text']))
                    and re.search(r'phase=pre-destroy', str(e['text']))
                    and test_line_correlated(str(e['text']), request2, BUNDLE_A)]) > 0
        if not test_correlated_marker(observation3['events'], BUNDLE_A, request2, 'VPN_ONDESTROY') or not has_destroy_begin3:
            throw_scenario_invalid(3, 'Stop-postcondition-missing-onDestroy-or-destroy-begin',
                                   step_index=1, step_id=step3_stop['step_id'], expected_action=step3_stop['expected_action'])
        final3 = get_vpn_final_state(observation3['events'], BUNDLE_A, request2, probe_contexts[3], True,
                                     int(freeze['process_absent_required_count']),
                                     float(freeze['process_absent_probe_spacing_seconds']))
        if str(final3['result']) == 'fail':
            scenario3_result = 'fail'
        elif not observation3['complete_window_observed'] or observation3['capture_degraded'] or str(final3['result']) != 'pass':
            scenario3_result = 'blocked'
        else:
            scenario3_result = 'pass'
        results.append({'sequence_index': 3, 'scenario': 3, 'result': scenario3_result, 'reason': final3['reason'],
                        'bundle': BUNDLE_A, 'request_id': request2, 'terminal_mode': final3['terminal_mode'],
                        'process_target': probe_contexts[3]['process_target'],
                        'process_target_verified': scenario2_result == 'pass',
                        'process_final_state_probes': list(probe_contexts[3]['probes']),
                        'bundle_present_during_probe': bool(probe_contexts[3]['bundle_present']),
                        'clean_reactivation_proof': False, 'observation': observation3['observation']})
        assert_scenario_capture_can_continue(results, observation3)

    # S4 mirrors S2, but the authorization layout is captured and verified
    # before Deny.
    invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_B})
    entry4 = invoke_layout_checkpoint(4, 'scenario-4-entry-b', 'entry', BUNDLE_B)
    context4 = new_scenario_context(4)
    step4_start = invoke_mechanical_step(context4, 1, '点击测试 App B 的 Start',
                                         {'status': 'pass', 'reason': 'B-entry-layout-verified'},
                                         lambda events: test_unique_start(events, BUNDLE_B), capture_before=entry4)
    request4 = str(step4_start['outcome']['request_id'])
    register_verified_request(request4, BUNDLE_B, 4)
    auth4 = invoke_layout_checkpoint(4, 'scenario-4-authorization', 'authorization', BUNDLE_B,
                                     step_index=2, step_id=step4_start['step_id'], expected_action='点击 Deny')

    def _s4_deny_postcondition(events):
        layout = _assess_layout_profile('scenario-4-after-deny', 'authorization-dismissed', BUNDLE_B)
        if str(layout['status']) == 'pass':
            return {'status': 'pass', 'reason': 'authorization-dismissed-after-Deny'}
        return {'status': 'invalid', 'reason': 'Deny-layout-postcondition:%s' % layout['reason']}

    step4_deny = invoke_mechanical_step(context4, 2, '点击 Deny',
                                        {'status': 'pass', 'reason': 'authorization-layout-verified', 'request_id': request4},
                                        _s4_deny_postcondition, capture_before=auth4,
                                        capture_after_name='scenario-4-after-deny',
                                        capture_after_profile='authorization-dismissed',
                                        capture_after_expected_bundle=BUNDLE_B)
    observation4 = complete_scenario_context(context4)
    assert_scenario_event_contract(4, observation4['events'], [{'bundle': BUNDLE_B, 'request_id': request4}])
    deny4 = get_deny_assessment(observation4['events'], BUNDLE_B, request4, True,
                                bool(observation4['complete_window_observed']))
    if str(deny4['result']) == 'fail' and str(deny4['reason']) == 'deny-created-B-vpn':
        throw_scenario_invalid(4, 'deny-action-produced-create-untrusted', step_index=2,
                               step_id=step4_deny['step_id'], expected_action=step4_deny['expected_action'],
                               machine_postcondition=deny4)
    if str(deny4['result']) == 'fail':
        scenario4_result = 'fail'
    elif not observation4['complete_window_observed'] or observation4['capture_degraded']:
        scenario4_result = 'blocked'
    else:
        scenario4_result = str(deny4['result'])
    results.append({'sequence_index': 4, 'scenario': 4, 'result': scenario4_result, 'reason': deny4['reason'],
                    'bundle': BUNDLE_B, 'request_id': request4, 'deny_screen': True,
                    'deny_screen_capture': {'name': 'scenario-4-authorization', 'status': 'collected',
                                            'visible': True, 'result': 'pass', 'layout_checkpoint': auth4},
                    'full_window_after_action': bool(observation4['complete_window_observed']),
                    'observation': observation4['observation']})
    assert_scenario_capture_can_continue(results, observation4)

    # S5: fresh A activation, then directly the A app-info machine gate and
    # one force-stop action. Settings>VPN is not a decisive step and is not
    # asked of the operator.
    invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_A})
    entry5 = invoke_layout_checkpoint(5, 'scenario-5-entry-a', 'entry', BUNDLE_A)
    context5 = new_scenario_context(5)
    step5_start = invoke_mechanical_step(context5, 1, '点击测试 App A 的 Start',
                                         {'status': 'pass', 'reason': 'A-entry-layout-verified'},
                                         lambda events: test_unique_start(events, BUNDLE_A), capture_before=entry5)
    request5 = str(step5_start['outcome']['request_id'])
    register_verified_request(request5, BUNDLE_A, 5)
    reactivation = invoke_layout_choice_checkpoint(5, 'scenario-5-reactivation', BUNDLE_A,
                                                   step_index=1, step_id=step5_start['step_id'],
                                                   expected_action=step5_start['expected_action'])
    actual_reallow_path = 'system-reauthorization-UI' if str(reactivation['selected_profile']) == 'authorization' else 'direct-system-activation'
    if actual_reallow_path == 'system-reauthorization-UI':
        def _s5_allow_postcondition(events):
            extra = test_no_operator_action(events)
            if str(extra['status']) != 'pass':
                return extra
            if test_correlated_marker(events, BUNDLE_A, request5, 'CREATE_ACCEPTED') \
                    or test_correlated_marker(events, BUNDLE_A, request5, 'VPN_CREATE_REJECTED'):
                return {'status': 'pass', 'reason': 'reactivation-create-terminal', 'request_id': request5}
            return {'status': 'pending', 'reason': 'reactivation-create-terminal-missing'}

        invoke_mechanical_step(context5, 2, '点击 Allow',
                               {'status': 'pass', 'reason': 'reauthorization-layout-verified'},
                               _s5_allow_postcondition, capture_before=reactivation,
                               capture_after_name='scenario-5-after-allow',
                               capture_after_profile='authorization-dismissed',
                               capture_after_expected_bundle=BUNDLE_A)

    def _s5_create_terminal(events):
        unique_start = test_unique_start(events, BUNDLE_A)
        if str(unique_start['status']) == 'invalid':
            return unique_start
        if str(unique_start['status']) == 'pending':
            return unique_start
        if test_correlated_marker(events, BUNDLE_A, request5, 'CREATE_ACCEPTED') \
                or test_correlated_marker(events, BUNDLE_A, request5, 'VPN_CREATE_REJECTED') \
                or test_correlated_marker(events, BUNDLE_A, request5, 'START_PROMISE_REJECTED'):
            return {'status': 'pass', 'reason': 'fresh-create-terminal', 'request_id': request5}
        return {'status': 'pending', 'reason': 'fresh-create-terminal-missing'}

    create_terminal5 = wait_machine_condition(context5, int(step5_start['anchor_byte']),
                                              step5_start['prompt_at'], _s5_create_terminal)
    if str(create_terminal5.get('status')) == 'blocked':
        raise RuntimeError('scenario-5 machine-verification-blocked step=1 reason=%s' % str(get_optional_property(create_terminal5, 'reason', 'machine-verification-blocked')))
    if str(create_terminal5.get('status')) != 'pass':
        throw_scenario_invalid(5, str(create_terminal5['reason']), step_index=1,
                               step_id=step5_start['step_id'], expected_action=step5_start['expected_action'])
    simulation_active_bundles.add(BUNDLE_A)

    def _s5_app_info_postcondition(events):
        extra = test_no_operator_action(events)
        if str(extra['status']) != 'pass':
            return extra
        layout = _assess_layout_profile('scenario-5-app-info', 'settings-app-info', BUNDLE_A)
        if str(layout['status']) == 'pass':
            return {'status': 'pass', 'reason': 'A-app-info-layout-match'}
        return {'status': 'invalid', 'reason': str(layout['reason'])}

    step5_info = invoke_mechanical_step(context5, 3, '打开"E3 Preflight A"的应用信息页',
                                        {'status': 'pass', 'reason': 'fresh-A-request-bound', 'request_id': request5},
                                        _s5_app_info_postcondition,
                                        capture_after_name='scenario-5-app-info',
                                        capture_after_profile='settings-app-info',
                                        capture_after_expected_bundle=BUNDLE_A)
    pre_force5 = get_exact_process_checkpoint([BUNDLE_A])
    if str(pre_force5['status']) != 'pass':
        raise RuntimeError('scenario-5 machine-verification-blocked step=4 reason=exact-process-precondition:%s' % pre_force5['reason'])
    probe_contexts[5] = new_process_probe_context(5, BUNDLE_A, require_bundle_present=True,
                                                  required_count=int(freeze['process_absent_required_count']),
                                                  spacing_seconds=float(freeze['process_absent_probe_spacing_seconds']))
    force_effect_applied = {'value': False}

    def _s5_force_postcondition(events):
        if not force_effect_applied['value']:
            if test_simulation_step_has_effect(5, 4):
                simulation_active_bundles.discard(BUNDLE_A)
            force_effect_applied['value'] = True
        extra = test_no_operator_action(events)
        if str(extra['status']) != 'pass':
            return extra
        invoke_process_final_state_probe_series(probe_contexts[5], get_now() + timedelta(seconds=20))
        absent = test_process_absent_evidence(probe_contexts[5], int(freeze['process_absent_required_count']),
                                              float(freeze['process_absent_probe_spacing_seconds']))
        if absent['met'] and probe_contexts[5]['bundle_present']:
            return {'status': 'pass', 'reason': 'fresh-request-extension-process-absent-bundle-present'}
        if probe_contexts[5]['aborted']:
            return {'status': 'blocked', 'reason': 'force-stop-process-check-unverifiable'}
        return {'status': 'invalid', 'reason': str(absent['reason'])}

    # The operator completes the visible Settings action, including its confirmation if one
    # appears. The post-action capture is evidence-only because Settings may leave AppDetail;
    # only the process-absent + bundle-present postcondition decides the force-stop effect.
    step5_force = invoke_mechanical_step(
        context5, 4, '点击强行停止，并完成随后出现的确认（如有）', pre_force5, _s5_force_postcondition,
        capture_before={'status': 'pass', 'name': 'scenario-5-app-info', 'profile': 'settings-app-info'},
        capture_after_name='scenario-5-app-info-force-stop', capture_after_review_only=True,
        verify_timeout_seconds=25)
    observation5 = complete_scenario_context(context5)
    assert_scenario_event_contract(5, observation5['events'], [{'bundle': BUNDLE_A, 'request_id': request5}])
    on_create5 = test_correlated_marker(observation5['events'], BUNDLE_A, request5, 'VPN_ONCREATE')
    create5 = test_correlated_marker(observation5['events'], BUNDLE_A, request5, 'CREATE_ACCEPTED')
    fresh_create_proof = create5 and test_post_create_open(observation5['events'], BUNDLE_A, request5)
    absent_evidence5 = test_process_absent_evidence(probe_contexts[5], int(freeze['process_absent_required_count']),
                                                    float(freeze['process_absent_probe_spacing_seconds']))
    s5_fd_still_open = test_s5_post_destroy_still_open(observation5['events'], BUNDLE_A, request5)
    if s5_fd_still_open:
        scenario5_result = 'fail'
    elif not observation5['complete_window_observed'] or observation5['capture_degraded'] \
            or not on_create5 or not fresh_create_proof or not absent_evidence5['met'] \
            or not probe_contexts[5]['bundle_present']:
        scenario5_result = 'blocked'
    else:
        scenario5_result = 'pass'
    if s5_fd_still_open:
        s5_reason = 'FD_STILL_OPEN'
    elif not fresh_create_proof:
        s5_reason = 'fresh-create-proof-missing'
    elif not absent_evidence5['met']:
        s5_reason = str(absent_evidence5['reason'])
    else:
        s5_reason = 'settings-app-info-force-stop-terminal'
    results.append({'sequence_index': 5, 'scenario': 5, 'result': scenario5_result, 'reason': s5_reason,
                    'bundle': BUNDLE_A, 'request_id': request5,
                    'settings_revoke_mechanism': str(freeze['settings_revoke_mechanism']),
                    'settings_vpn_page_policy': str(freeze['settings_vpn_page_policy']),
                    'settings_vpn_page_observation_only': True,
                    'settings_vpn_page_capture': {'name': 'scenario-5-settings-vpn-page', 'status': 'not-required',
                                                  'machine_verified': False, 'note': 'observation-only optional; not asked of the operator'},
                    'app_info_force_stop_capture': {
                        'name': 'scenario-5-app-info-force-stop',
                        'status': str(step5_force['capture_after']['status']),
                        'machine_verified': False,
                        'observation_only': True,
                    },
                    'terminal_mode': 'settings-app-info-force-stop', 'fd_still_open': bool(s5_fd_still_open),
                    'process_target': probe_contexts[5]['process_target'],
                    'process_target_verified': scenario2_result == 'pass',
                    'process_final_state_probes': list(probe_contexts[5]['probes']),
                    'process_absent_evidence': {'met': bool(absent_evidence5['met']), 'reason': absent_evidence5['reason'],
                                                 'required_count': int(freeze['process_absent_required_count']),
                                                 'required_spacing_seconds': float(freeze['process_absent_probe_spacing_seconds']),
                                                 'measured_spacing_seconds': absent_evidence5['spacing_seconds']},
                    'bundle_present_during_probe': bool(probe_contexts[5]['bundle_present']),
                    'settings_reallow_path': {'expected': str(freeze['settings_reallow_expected_path']),
                                              'actual': actual_reallow_path,
                                              'match': actual_reallow_path == str(freeze['settings_reallow_expected_path']),
                                              'observation': 'machine-layout-and-event-classified',
                                              'policy': str(freeze['settings_reallow_path_policy'])},
                    'assertions': {'vpn_on_create': 'pass' if on_create5 else 'blocked',
                                   'vpn_connection_create_fd': 'pass' if create5 else 'blocked',
                                   'fresh_create_proof': 'pass' if fresh_create_proof else 'blocked',
                                   'force_stop': 'pass' if absent_evidence5['met'] else 'blocked'},
                    'observation': observation5['observation']})
    s3_entry = [r for r in results if int(r['scenario']) == 3][:1]
    if s3_entry and 'clean_reactivation_proof' in s3_entry[0] and s3_entry[0]['clean_reactivation_proof'] is not None:
        s3_entry[0]['clean_reactivation_proof'] = bool(fresh_create_proof)
    assert_scenario_capture_can_continue(results, observation5)

    # S6: A Start, B Start, and optional B Allow on first authorization.
    # Only a frozen explicit B conflict code is a passing conflict result.
    invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_A})
    entry6a = invoke_layout_checkpoint(6, 'scenario-6-entry-a', 'entry', BUNDLE_A)
    context6 = new_scenario_context(6)
    step6a = invoke_mechanical_step(context6, 1, '点击测试 App A 的 Start',
                                    {'status': 'pass', 'reason': 'A-entry-layout-verified'},
                                    lambda events: test_unique_start(events, BUNDLE_A), capture_before=entry6a)
    request6a = str(step6a['outcome']['request_id'])
    register_verified_request(request6a, BUNDLE_A, 6)
    reauth6 = invoke_layout_choice_checkpoint(6, 'scenario-6-reactivation-a', BUNDLE_A,
                                              step_index=1, step_id=step6a['step_id'],
                                              expected_action=step6a['expected_action'])
    s6a_reauth_path = 'system-reauthorization-UI' if str(reauth6['selected_profile']) == 'authorization' else 'direct-system-activation'
    if s6a_reauth_path == 'system-reauthorization-UI':
        def _s6_allow_postcondition(events):
            if test_correlated_marker(events, BUNDLE_A, request6a, 'CREATE_ACCEPTED') \
                    or test_correlated_marker(events, BUNDLE_A, request6a, 'VPN_CREATE_REJECTED') \
                    or test_correlated_marker(events, BUNDLE_A, request6a, 'VPN_CREATE_INVALID_FD') \
                    or test_correlated_marker(events, BUNDLE_A, request6a, 'START_PROMISE_REJECTED'):
                return {'status': 'pass', 'reason': 'reauth-A-create-terminal', 'request_id': request6a}
            return {'status': 'pending', 'reason': 'reauth-A-create-terminal-missing'}

        invoke_mechanical_step(context6, 2, '点击 Allow',
                               {'status': 'pass', 'reason': 'reauthorization-layout-verified', 'request_id': request6a},
                               _s6_allow_postcondition, capture_before=reauth6,
                               capture_after_name='scenario-6-after-allow-a',
                               capture_after_profile='authorization-dismissed',
                               capture_after_expected_bundle=BUNDLE_A)

    def _s6_a_terminal(events):
        unique_start = test_unique_start(events, BUNDLE_A)
        if str(unique_start['status']) == 'invalid':
            return unique_start
        if str(unique_start['status']) == 'pending':
            return unique_start
        if test_correlated_marker(events, BUNDLE_A, request6a, 'CREATE_ACCEPTED') \
                or test_correlated_marker(events, BUNDLE_A, request6a, 'VPN_CREATE_REJECTED') \
                or test_correlated_marker(events, BUNDLE_A, request6a, 'VPN_CREATE_INVALID_FD') \
                or test_correlated_marker(events, BUNDLE_A, request6a, 'START_PROMISE_REJECTED'):
            return {'status': 'pass', 'reason': 'A-create-terminal', 'request_id': request6a}
        return {'status': 'pending', 'reason': 'A-create-terminal-missing'}

    a_terminal6 = wait_machine_condition(context6, int(step6a['anchor_byte']), step6a['prompt_at'], _s6_a_terminal)
    if str(a_terminal6.get('status')) == 'blocked':
        raise RuntimeError('scenario-6 machine-verification-blocked step=1 reason=%s' % str(get_optional_property(a_terminal6, 'reason', 'machine-verification-blocked')))
    if str(a_terminal6.get('status')) != 'pass':
        throw_scenario_invalid(6, str(a_terminal6['reason']), step_index=1,
                               step_id=step6a['step_id'], expected_action=step6a['expected_action'])
    a_accepted6 = test_correlated_marker(get_scenario_context_events(context6['capture'], None, context6['anchor_byte'], context6['started_at']), BUNDLE_A, request6a, 'CREATE_ACCEPTED')
    if a_accepted6:
        simulation_active_bundles.add(BUNDLE_A)
    context6_events = get_scenario_context_events(context6['capture'], None, context6['anchor_byte'], context6['started_at'])
    on_create6a = test_correlated_marker(context6_events, BUNDLE_A, request6a, 'VPN_ONCREATE')
    extension_reject6a = (test_correlated_marker(context6_events, BUNDLE_A, request6a, 'VPN_CREATE_REJECTED')
                          or test_correlated_marker(context6_events, BUNDLE_A, request6a, 'VPN_CREATE_INVALID_FD')) and on_create6a
    auth_unclassified6a = (test_correlated_marker(context6_events, BUNDLE_A, request6a, 'VPN_CREATE_REJECTED')
                           or test_correlated_marker(context6_events, BUNDLE_A, request6a, 'VPN_CREATE_INVALID_FD')
                           or test_correlated_marker(context6_events, BUNDLE_A, request6a, 'START_PROMISE_REJECTED')) and not extension_reject6a
    if not a_accepted6:
        observation6 = complete_scenario_context(context6)
        accepted6a = get_accepted_marker_assessment(
            observation6['events'], [{'bundle': BUNDLE_A, 'request_id': request6a}])
        if accepted6a['unexpected']:
            throw_scenario_invalid(6, 'unexpected-accepted-request-in-window', step_index=1,
                                   step_id=step6a['step_id'], expected_action=step6a['expected_action'])
        assert_scenario_event_contract(6, observation6['events'], [{'bundle': BUNDLE_A, 'request_id': request6a}])
        s6a_result = 'fail' if extension_reject6a else 'blocked'
        s6a_reason = 'A-create-rejected-or-invalid-fd' if extension_reject6a else 'authorization-outcome-unclassified'
        s7a_reason = 'not-run-after-functional-fail' if extension_reject6a else 'not-run-after-platform-blocked'
        results.append({'sequence_index': 6, 'scenario': 6, 'result': s6a_result, 'reason': s6a_reason,
                        'a_reauth_path': s6a_reauth_path, 'request_id_a': request6a, 'request_id_b': None,
                        'a_accepted': False, 'a_on_create': bool(on_create6a),
                        'a_extension_rejected': bool(extension_reject6a), 'a_auth_unclassified': bool(auth_unclassified6a),
                        'b_rejected': None, 'b_rejection_code': None, 'b_accepted': False,
                        'accepted_session_count_in_window': accepted6a['accepted_count'],
                        'conflict_capture': {'name': 'scenario-6-conflict', 'status': 'not-required', 'review_only': False},
                        'observation': observation6['observation']})
        results.append({'sequence_index': 7, 'scenario': 7, 'result': 'blocked', 'reason': s7a_reason,
                        'active_bundle': None, 'request_id': None})
        partial_scenarios = list(results)
        return results
    pre6b = get_exact_process_checkpoint([BUNDLE_A])
    if str(pre6b['status']) != 'pass':
        raise RuntimeError('scenario-6 machine-verification-blocked step=3 reason=exact-process-precondition:%s' % pre6b['reason'])
    invoke_hdc_operation('StartEntry', {'Bundle': BUNDLE_B})
    entry6b = invoke_layout_checkpoint(6, 'scenario-6-entry-b', 'entry', BUNDLE_B)
    step6b = invoke_mechanical_step(context6, 3, '点击测试 App B 的 Start', pre6b,
                                    lambda events: test_unique_start(events, BUNDLE_B),
                                    capture_before=entry6b)
    request6b = str(step6b['outcome']['request_id'])
    register_verified_request(request6b, BUNDLE_B, 6)
    b_transition6 = invoke_layout_choice_checkpoint(6, 'scenario-6-conflict', BUNDLE_B,
                                                    step_index=3, step_id=step6b['step_id'],
                                                    expected_action=step6b['expected_action'])
    if str(b_transition6['selected_profile']) == 'authorization':
        invoke_mechanical_step(
            context6, 4, '点击 Allow',
            {'status': 'pass', 'reason': 'B-authorization-layout-and-request-machine-verified', 'request_id': request6b},
            test_no_operator_action,
            capture_before=b_transition6, capture_after_name='scenario-6-after-allow-b',
            capture_after_profile='authorization-dismissed',
            capture_after_expected_bundle=BUNDLE_B,
            capture_after_mismatch_is_blocked=True)

    def _s6_b_terminal(events):
        unique_start = test_unique_start(events, BUNDLE_B)
        if str(unique_start['status']) == 'invalid':
            return unique_start
        if str(unique_start['status']) == 'pending':
            return unique_start
        if test_correlated_marker(events, BUNDLE_B, request6b, 'CREATE_ACCEPTED'):
            return {'status': 'pass', 'reason': 'B-create-accepted', 'request_id': request6b}
        rejected = [e for e in events
                    if (re.search(r'VPN_CREATE_REJECTED\|', str(e['text'])) or re.search(r'START_PROMISE_REJECTED\|', str(e['text'])))
                    and test_line_correlated(str(e['text']), request6b, BUNDLE_B)]
        if rejected:
            frozen_codes = [int(c) for c in (freeze.get('vpn_conflict_rejection_codes') or [])]
            frozen_hit = None
            first_code = None
            for rej in rejected:
                code = get_rejection_error_code(str(rej['text']))
                if first_code is None and code is not None:
                    first_code = code
                if code is not None and code in frozen_codes:
                    frozen_hit = code
                    break
            if frozen_hit is not None:
                return {'status': 'pass', 'reason': 'B-frozen-conflict-code', 'request_id': request6b, 'code': frozen_hit}
            return {'status': 'blocked', 'reason': 'B-conflict-code-not-frozen:%s' % first_code,
                    'request_id': request6b, 'code': first_code}
        return {'status': 'pending', 'reason': 'B-create-terminal-missing'}

    b_terminal6 = wait_machine_condition(context6, int(step6b['anchor_byte']), step6b['prompt_at'], _s6_b_terminal)
    non_frozen_rejection6 = (str(b_terminal6.get('status')) == 'blocked' and
                             re.match(r'^B-conflict-code-not-frozen', str(b_terminal6.get('reason', ''))))
    if str(b_terminal6.get('status')) == 'blocked' and not non_frozen_rejection6:
        raise RuntimeError('scenario-6 machine-verification-blocked step=%d reason=%s' %
                           (4 if str(b_transition6['selected_profile']) == 'authorization' else 3,
                            str(get_optional_property(b_terminal6, 'reason', 'machine-verification-blocked'))))
    if str(b_terminal6.get('status')) not in ('pass', 'blocked'):
        throw_scenario_invalid(6, str(b_terminal6['reason']), step_index=3,
                               step_id=step6b['step_id'], expected_action=step6b['expected_action'])

    b_accepted_terminal6 = (str(b_terminal6.get('status')) == 'pass' and
                            str(b_terminal6.get('reason')) == 'B-create-accepted')
    b_rejected_terminal6 = not b_accepted_terminal6
    if b_accepted_terminal6:
        simulation_active_bundles.add(BUNDLE_B)
    # Observe one post-terminal process checkpoint for both terminal outcomes.
    # It gates only a rejected B; accepted evidence remains a functional fail.
    terminal_process6 = get_exact_process_checkpoint(
        [BUNDLE_A, BUNDLE_B] if b_accepted_terminal6 else [BUNDLE_A],
        observed_bundles=[] if b_accepted_terminal6 else [BUNDLE_B])
    observation6 = complete_scenario_context(context6)
    accepted6 = get_accepted_marker_assessment(
        observation6['events'],
        [{'bundle': BUNDLE_A, 'request_id': request6a},
         {'bundle': BUNDLE_B, 'request_id': request6b}])
    if accepted6['unexpected']:
        throw_scenario_invalid(6, 'unexpected-accepted-request-in-window', step_index=3,
                               step_id=step6b['step_id'], expected_action=step6b['expected_action'])
    assert_scenario_event_contract(6, observation6['events'],
                                   [{'bundle': BUNDLE_A, 'request_id': request6a},
                                    {'bundle': BUNDLE_B, 'request_id': request6b}])
    a_accepted_count6, b_accepted_count6 = accepted6['counts']
    b_accepted6 = b_accepted_count6 > 0
    dual_accepted6 = a_accepted_count6 > 0 and b_accepted_count6 > 0

    if b_accepted6:
        scenario6_result = 'fail'
        s6_reason = 'two-accepted-sessions-observed' if dual_accepted6 else 'B-create-accepted-instead-of-conflict-rejection'
    elif b_rejected_terminal6 and str(terminal_process6['status']) != 'pass':
        scenario6_result = 'blocked'
        s6_reason = 'A-active-state-unverifiable-after-B-terminal'
    elif non_frozen_rejection6:
        scenario6_result = 'blocked'
        s6_reason = 'B-conflict-code-not-frozen:%s' % get_optional_property(b_terminal6, 'code', None)
    elif not observation6['complete_window_observed'] or observation6['capture_degraded']:
        scenario6_result = 'blocked'
        s6_reason = 'B-conflict-observation-incomplete'
    elif str(b_terminal6['reason']) == 'B-frozen-conflict-code':
        scenario6_result = 'pass'
        s6_reason = 'B-explicit-conflict-rejection'
    else:
        scenario6_result = 'blocked'
        s6_reason = 'B-conflict-terminal-unclassified'
    results.append({'sequence_index': 6, 'scenario': 6, 'result': scenario6_result, 'reason': s6_reason,
                    'a_reauth_path': s6a_reauth_path, 'request_id_a': request6a, 'request_id_b': request6b,
                    'a_accepted': bool(a_accepted_count6 > 0), 'b_rejected': bool(b_rejected_terminal6),
                    'b_rejection_code': get_optional_property(b_terminal6, 'code', None),
                    'b_accepted': bool(b_accepted6),
                    'accepted_session_count_in_window': accepted6['accepted_count'],
                    'machine_process_checkpoint': terminal_process6,
                    'conflict_capture': {'name': 'scenario-6-conflict', 'status': 'collected', 'review_only': False},
                    'observation': observation6['observation']})
    assert_scenario_capture_can_continue(results, observation6)

    # S7 consumes only a pass whose rejected-terminal A checkpoint was verified.
    if scenario6_result != 'pass':
        s7_reason = 'not-run-after-functional-fail' if scenario6_result == 'fail' else 'not-run-after-platform-blocked'
        results.append({'sequence_index': 7, 'scenario': 7, 'result': 'blocked',
                        'reason': s7_reason, 'active_bundle': None, 'request_id': None})
        partial_scenarios = list(results)
        return results
    active_bundle = BUNDLE_A
    active_request = request6a
    invoke_hdc_operation('StartEntry', {'Bundle': active_bundle})
    entry7 = invoke_layout_checkpoint(7, 'scenario-7-entry-a', 'entry', active_bundle)
    pre7 = get_exact_process_checkpoint([active_bundle])
    context7 = new_scenario_context(7)
    step7_stop = invoke_mechanical_step(context7, 1, '点击测试 App A 的 Stop', pre7,
                                        lambda events: test_unique_stop_condition(events, active_bundle, active_request),
                                        capture_before=entry7, capture_after_name='scenario-7-after-stop',
                                        capture_after_review_only=True)
    if test_simulation_step_has_effect(7, 1):
        simulation_active_bundles.discard(active_bundle)
    probe_contexts[7] = new_process_probe_context(7, active_bundle, require_bundle_present=False,
                                                  required_count=int(freeze['process_absent_required_count']),
                                                  spacing_seconds=float(freeze['process_absent_probe_spacing_seconds']))

    def _s7_during(events):
        if test_strict_fallback_prerequisites(events, active_bundle, active_request)['met']:
            invoke_process_final_state_probe_series(probe_contexts[7], current_window_end)

    observation7 = complete_scenario_context(context7, _s7_during)
    assert_scenario_event_contract(7, observation7['events'], [], [{'bundle': active_bundle, 'request_id': active_request}])
    has_destroy_begin7 = test_correlated_marker(observation7['events'], active_bundle, active_request, 'VPN_DESTROY_BEGIN') \
        or len([e for e in observation7['events'] if re.search(r'VPN_FD_SNAPSHOT', str(e['text']))
                and re.search(r'phase=pre-destroy', str(e['text']))
                and test_line_correlated(str(e['text']), active_request, active_bundle)]) > 0
    if not test_correlated_marker(observation7['events'], active_bundle, active_request, 'VPN_ONDESTROY') or not has_destroy_begin7:
        throw_scenario_invalid(7, 'Stop-postcondition-missing-onDestroy-or-destroy-begin',
                               step_index=1, step_id=step7_stop['step_id'], expected_action=step7_stop['expected_action'])
    final7 = get_vpn_final_state(observation7['events'], active_bundle, active_request, probe_contexts[7], False,
                                 int(freeze['process_absent_required_count']),
                                 float(freeze['process_absent_probe_spacing_seconds']))
    terminal_assessed7 = str(final7['result']) == 'pass'
    fault_degraded7 = False
    cleanup_done7 = False
    cleanup_verified7 = False
    cleanup_completed_at7 = None
    if terminal_assessed7:
        invoke_review_only_capture('scenario-7-pre-uninstall', 7)
        for fault_operation in ('FaultA', 'FaultB'):
            if invoke_fault_artifact(fault_operation, 7) != 'collected':
                fault_degraded7 = True
        if installed_b:
            uninstall_b = invoke_hdc_operation('Uninstall', {'Bundle': BUNDLE_B}, allow_failure=True)
            cleanup_actions.append({'operation': 'Uninstall', 'bundle': BUNDLE_B, 'exit_code': uninstall_b.exit_code})
            if uninstall_b.exit_code == 0:
                installed_b = False
        if installed_a:
            uninstall_a = invoke_hdc_operation('Uninstall', {'Bundle': BUNDLE_A}, allow_failure=True)
            cleanup_actions.append({'operation': 'Uninstall', 'bundle': BUNDLE_A, 'exit_code': uninstall_a.exit_code})
            if uninstall_a.exit_code == 0:
                installed_a = False
        if staging_sent or staging_may_exist:
            invoke_remove_staging_verified('RemoveStaging')
        cleanup_verified7 = test_targeted_cleanup_state()
        cleanup_done7 = True
        cleanup_completed_at7 = isoformat_7(get_now())
        invoke_review_only_capture('scenario-7-post-cleanup', 7)
    else:
        invoke_review_only_capture('scenario-7-final-state', 7)
    if str(final7['result']) == 'fail':
        scenario7_result = 'fail'
    elif not observation7['complete_window_observed'] or observation7['capture_degraded'] \
            or str(final7['result']) != 'pass' or not cleanup_verified7 or fault_degraded7:
        scenario7_result = 'blocked'
    else:
        scenario7_result = 'pass'
    results.append({'sequence_index': 7, 'scenario': 7, 'result': scenario7_result, 'reason': final7['reason'],
                    'active_bundle': active_bundle, 'request_id': active_request, 'terminal_mode': final7['terminal_mode'],
                    'terminal_assessed': bool(terminal_assessed7),
                    'terminal_mode_at_cleanup': final7['terminal_mode'] if terminal_assessed7 else None,
                    'process_target': probe_contexts[7]['process_target'],
                    'process_target_verified': scenario2_result == 'pass',
                    'process_final_state_probes': list(probe_contexts[7]['probes']),
                    'bundle_present_during_probe': bool(probe_contexts[7]['bundle_present']),
                    'cleanup_completed_at': cleanup_completed_at7, 'post_cleanup_capture': bool(cleanup_done7),
                    'post_cleanup_capture_name': 'scenario-7-post-cleanup' if cleanup_done7 else 'scenario-7-final-state',
                    'bundle_process_cleanup_verified': bool(cleanup_verified7),
                    'fault_capture_degraded': bool(fault_degraded7),
                    'observation': observation7['observation']})
    partial_scenarios = list(results)
    return results


def invoke_precise_finally_cleanup(cleanup_reason):
    """PS L4235-4257 Invoke-PreciseFinallyCleanup (C12): finally-block
    cleanup - ForceStop (cleanup-only reason, not_used_as_revoke=true) ->
    Uninstall -> RemoveStaging -> Test-TargetedCleanupState. DryRun never
    executes device commands. Every action is recorded in CleanupActions."""
    global installed_a, installed_b, staging_sent, staging_may_exist
    if dry_run:
        return
    if installed_b:
        force_b_result = invoke_hdc_operation('ForceStop', {'Bundle': BUNDLE_B, 'Reason': cleanup_reason}, allow_failure=True)
        cleanup_actions.append({'operation': 'finally-force-stop', 'bundle': BUNDLE_B,
                                'exit_code': force_b_result.exit_code, 'not_used_as_revoke': True})
        uninstall_b_result = invoke_hdc_operation('Uninstall', {'Bundle': BUNDLE_B}, allow_failure=True)
        cleanup_actions.append({'operation': 'finally-uninstall', 'bundle': BUNDLE_B,
                                'exit_code': uninstall_b_result.exit_code, 'installed_flag': True})
        if uninstall_b_result.exit_code == 0:
            installed_b = False
    if installed_a:
        force_a_result = invoke_hdc_operation('ForceStop', {'Bundle': BUNDLE_A, 'Reason': cleanup_reason}, allow_failure=True)
        cleanup_actions.append({'operation': 'finally-force-stop', 'bundle': BUNDLE_A,
                                'exit_code': force_a_result.exit_code, 'not_used_as_revoke': True})
        uninstall_a_result = invoke_hdc_operation('Uninstall', {'Bundle': BUNDLE_A}, allow_failure=True)
        cleanup_actions.append({'operation': 'finally-uninstall', 'bundle': BUNDLE_A,
                                'exit_code': uninstall_a_result.exit_code, 'installed_flag': True})
        if uninstall_a_result.exit_code == 0:
            installed_a = False
    if staging_sent or staging_may_exist:
        invoke_remove_staging_verified('finally-remove-staging')
    test_targeted_cleanup_state()


def test_targeted_cleanup_state():
    """PS L4198-4234 Test-TargetedCleanupState (C12): verified-clean requires
    A/B bundle absent + :vpn process absent + staging absent + no
    StagingSent/StagingMayExist flags; otherwise blocked-unknown-residual.
    On verified-clean the installed/staging flags are cleared. Returns the
    verified boolean (S7 cleanup gate) and records CleanupVerification."""
    global installed_a, installed_b, staging_sent, staging_may_exist, cleanup_verification
    bundle_states = []
    verified = True
    for bundle in (BUNDLE_A, BUNDLE_B):
        dump_result = invoke_hdc_operation('BundleDump', {'Bundle': bundle}, allow_failure=True)
        pid_result = invoke_hdc_operation('PidOf', {'Bundle': bundle}, allow_failure=True)
        dump_text = dump_result.combined_text()
        bundle_absent = bool(re.search(r'(?i)failed to get information|not exist|not found', dump_text))
        process_absent = int(pid_result.exit_code) in (0, 1) \
            and not str(pid_result.stdout).strip() and not str(pid_result.stderr).strip()
        if not bundle_absent or not process_absent:
            verified = False
        bundle_states.append({'bundle': bundle, 'bundle_query_exit': dump_result.exit_code,
                              'bundle_absent': bool(bundle_absent), 'pid_query_exit': pid_result.exit_code,
                              'process_absent': bool(process_absent)})
    staging_probe = invoke_hdc_operation('StagingProbe', allow_failure=True)
    staging_absent = test_staging_absent(staging_probe)
    if not staging_absent or staging_sent or staging_may_exist:
        verified = False
    cleanup_verification = {
        'status': 'verified-clean' if verified else 'blocked-unknown-residual',
        'verified_absent': bool(verified),
        'checked_at': isoformat_7(get_now()),
        'bundles': bundle_states,
        'staging': {'probe_exit': staging_probe.exit_code, 'staging_absent': bool(staging_absent),
                    'staging_sent_flag': bool(staging_sent), 'staging_may_exist_flag': bool(staging_may_exist)},
    }
    if verified:
        installed_a = False
        installed_b = False
        staging_sent = False
        staging_may_exist = False
    return bool(verified)


# =====================================================================
# Section 11: Records / manifest / seal / integrity (design unit U7)
# =====================================================================


def write_json_file(path, obj):
    """PS L283-287 Write-JsonFile: ConvertTo-Json -Depth 40 (indented, 2
    spaces) + NewLine, UTF-8 without BOM (A1/R2: fixed '\n')."""
    write_text_utf8_no_bom(path, jsoncompat_dumps(obj, indent=2) + '\n')


def add_transcript_record(kind, data):
    """PS L263-281 Add-TranscriptRecord (C4): JSONL chain. Each line is
    {payload, payload_canonical, entry_hash}; payload_canonical is the
    compact PS-compatible serialization of the (redacted) payload,
    entry_hash = SHA-256(payload_canonical), previous_hash chains to the
    prior entry_hash (64 zeros for the first). Appended UTF-8 no BOM with
    '\n'. No-op until the projection transcript is initialized (U10)."""
    global transcript_index, transcript_previous_hash
    if projection_transcript is None:
        return
    safe_data = protect_sensitive_data(data)
    payload = {
        'index': transcript_index + 1,
        'host_observed_at': isoformat_7(get_now()),
        'kind': kind,
        'data': safe_data,
        'previous_hash': transcript_previous_hash,
    }
    payload_canonical = jsoncompat_dumps(payload)
    entry_hash = sha256_text(payload_canonical)
    record = {'payload': payload, 'payload_canonical': payload_canonical, 'entry_hash': entry_hash}
    line = jsoncompat_dumps(record)
    with open(projection_transcript, 'a', encoding='utf-8', newline='') as f:
        f.write(line + '\n')
    transcript_index += 1
    transcript_previous_hash = entry_hash


def write_operator_wait_state(phase, **kwargs):
    """PS L288-339 Write-OperatorWaitState (C13): operator-wait-state.json
    with schema_version=2, the fixed field order, and the full history
    (including the current entry). No-op until EvidencePath is initialized.
    The final 'complete' phase is written by the U10 finally chain."""
    global operator_wait_current
    if evidence_path is None:
        return
    updated_at = isoformat_7(get_now())
    scenario = kwargs.get('scenario')
    step_index = kwargs.get('step_index')
    step_id = kwargs.get('step_id')
    expected_action = kwargs.get('expected_action')
    capture_before = kwargs.get('capture_before')
    capture_after = kwargs.get('capture_after')
    machine_precondition = kwargs.get('machine_precondition')
    machine_postcondition = kwargs.get('machine_postcondition')
    current = {
        'scenario': scenario,
        'step_index': step_index,
        'step_id': step_id,
        'expected_action': expected_action,
        'phase': phase,
        'capture_before': {'status': 'not-required'} if capture_before is None else capture_before,
        'capture_after': {'status': 'not-required'} if capture_after is None else capture_after,
        'machine_precondition': {'status': 'not-evaluated'} if machine_precondition is None else machine_precondition,
        'machine_postcondition': {'status': 'not-evaluated'} if machine_postcondition is None else machine_postcondition,
        'updated_at': updated_at,
    }
    operator_wait_current = current
    operator_wait_history.append(current)
    payload = {
        'schema_version': 2,
        'exception': 'E3-PHYS-PREFLIGHT',
        'campaign_id': str(freeze['campaign_id']) if freeze is not None else None,
        'evidence_id': str(freeze['evidence_id']) if freeze is not None else None,
        'execution_mode': execution_mode,
        'trust_model': 'mechanical-action-only-machine-verified-v1',
        'scenario': current['scenario'],
        'step_index': current['step_index'],
        'step_id': current['step_id'],
        'expected_action': current['expected_action'],
        'phase': current['phase'],
        'capture_before': current['capture_before'],
        'capture_after': current['capture_after'],
        'machine_precondition': current['machine_precondition'],
        'machine_postcondition': current['machine_postcondition'],
        'updated_at': current['updated_at'],
        'complete': phase == 'complete',
        'completed_at': updated_at if phase == 'complete' else None,
        'history': list(operator_wait_history),
    }
    write_json_file(os.path.join(evidence_path, 'operator-wait-state.json'), payload)


def get_public_raw_references():
    """PS L4259-4267 Get-PublicRawReferences: raw-hilog artifacts with
    host_path_sha256 (path-string hash, never the real path)."""
    references = []
    for artifact in raw_hilog_artifacts:
        references.append({'scenario': artifact['scenario'], 'reference': artifact['reference'],
                           'sha256': artifact['sha256'], 'bytes': artifact['bytes'],
                           'host_path_sha256': sha256_text(str(artifact['path']))})
    return references


def get_public_screenshot_references():
    """PS L4269-4280 Get-PublicScreenshotReferences."""
    references = []
    for artifact in capture_artifacts:
        entry = {'scenario': artifact['scenario'], 'name': artifact['name'],
                 'status': artifact['status'], 'failures': list(artifact['failures'])}
        if artifact['status'] == 'collected':
            entry['screen'] = {'reference': 'RAW-SCREEN-%s' % artifact['name'],
                               'sha256': sha256_file(artifact['screen_path']),
                               'bytes': os.path.getsize(artifact['screen_path']),
                               'host_path_sha256': sha256_text(str(artifact['screen_path']))}
        references.append(entry)
    return references


def get_public_layout_references():
    """PS L4282-4293 Get-PublicLayoutReferences."""
    references = []
    for artifact in capture_artifacts:
        entry = {'scenario': artifact['scenario'], 'name': artifact['name'],
                 'status': artifact['status'], 'failures': list(artifact['failures'])}
        if artifact['status'] == 'collected':
            entry['layout'] = {'reference': 'RAW-LAYOUT-%s' % artifact['name'],
                               'sha256': sha256_file(artifact['layout_path']),
                               'bytes': os.path.getsize(artifact['layout_path']),
                               'host_path_sha256': sha256_text(str(artifact['layout_path']))}
        references.append(entry)
    return references


def get_public_fault_references():
    """PS L4295-4300 Get-PublicFaultReferences."""
    references = []
    for artifact in fault_artifacts:
        references.append({'scenario': artifact['scenario'], 'operation': artifact['operation'],
                           'reference': artifact['reference'], 'status': artifact['status'],
                           'sha256': artifact['sha256'], 'bytes': artifact['bytes'],
                           'failures': list(artifact['failures']),
                           'host_path_sha256': sha256_text(str(artifact['path']))})
    return references


_OBSERVATION_SEMANTICS = ('ADJ-20260808-0002 strong-reliable protocol (mechanical-action-only-machine-verified-v1): '
                          'one continuous campaign HiLog capture; pre-scenario byte anchors exclude prior buffer; '
                          'device_observed_at bounds first mechanical action prompt through last action plus at least 60 seconds; '
                          'frozen CST=>+08:00 zone map; device clock skew tolerance 3s; operator sees only single-step '
                          '"现在只做X，完成后按回车" and Read-Host is mechanical enter only (no READY/ACK/token/y-n semantic gates); '
                          'machine layout gates (deterministic-layout-v1) before Allow/Deny and after decisive captures; '
                          'scenario 1 is fully machine-operated install; scenario 3/7 terminal prefers callback destroy terminal '
                          'plus post-destroy fd snapshot, otherwise strict-process-boundary needs unique stop/onDestroy/destroy-begin '
                          'plus consecutive absent host process probes (>=2, >=3s apart, bundle present for scenario 3); '
                          'process probes pidof only the <bundle>:vpn Extension ability process (ADJ-20260808-0001) while '
                          'BundleDump proves the bundle/main App stays installed; any extra Start/Stop/UI_STOP_SKIPPED/wrong '
                          'requestId/order is scenario invalid and stops later scenarios as not-run-due-to-invalid; scenario 5 '
                          'revokes via atomic Settings navigation steps with machine layout gates plus force-stop then :vpn absent '
                          '+ bundle present; scenario 6 is fully machine: B Start step3 branches through entry-or-authorization; optional '
                          'B Allow step4 requires dismissed UI; A exact-process before B Start is the pre-gate; one post-terminal process checkpoint is observed after accepted/rejected terminal but gates only rejected; accepted_session_count_in_window counts only CREATE_ACCEPTED markers matching the two verified requestIds; unique A CREATE_ACCEPTED + unique B CREATE_REJECTED with '
                          'frozen code 2203002, no dual accepted and no operator dual-active fields; scenario 7 binds S6 verified A '
                          'request only and never asks FINAL-CLEANUP; overall priority integrity invalid > scenario invalid > fail > '
                          'blocked > pass; probe results are recorded before any cleanup and never backfilled from finally')


def new_complete_record(freeze, scenarios, overall, record_status, started_at, ended_at,
                        fatal_message, infrastructure_reason, repository_before, freeze_sha256,
                        freeze_contract_sha256, confirmation_contract_sha256, manifest_sha256):
    """PS L4302-4518 New-CompleteRecord (C16): the scenario-results.json
    record. Field order follows the PS [ordered]@{} exactly. Tri-state
    s3_clean_reactivation_proof (true/false/null - null means S3 was never
    probed, never coerced to false). Non-evidence modes stay blocked unless
    the measured aggregation is an explicit fail/invalid."""
    scenario2 = next((s for s in scenarios if int(s['scenario']) == 2), None)
    s3_record = next((s for s in scenarios if int(s['scenario']) == 3), None)
    s3_proof_value = None
    if s3_record is not None and 'clean_reactivation_proof' in s3_record:
        raw_proof = s3_record['clean_reactivation_proof']
        s3_proof_value = None if raw_proof is None else bool(raw_proof)
    is_evidence = execution_mode == 'live'
    if not is_evidence and overall not in ('fail', 'invalid'):
        overall = 'blocked'
        record_status = 'blocked'
    retry_obj = get_optional_property(freeze, 'retry', {})
    prior_record_path = str(get_optional_property(retry_obj, 'prior_record_path', 'N/A'))
    record = {
        'schema_version': 1,
        'evidence_id': str(freeze['evidence_id']),
        'exception': 'E3-PHYS-PREFLIGHT',
        'information_status': 'current-measured',
        'plan_status': str(freeze['plan_status']),
        'record_status': record_status,
        'stage_or_gate': 'E3',
        'related_stages_or_gates': ['E8'],
        'campaign_id': str(freeze['campaign_id']),
        'attempt': str(freeze['attempt']),
        'retry': {
            'basis': str(get_optional_property(retry_obj, 'basis', 'N/A')),
            'infrastructure_reason': str(get_optional_property(retry_obj, 'infrastructure_reason', 'N/A')),
            'prior_record_reference': 'N/A' if prior_record_path == 'N/A' else 'PRIOR-BLOCKED-RECORD',
            'prior_record_path_sha256': 'N/A' if prior_record_path == 'N/A' else sha256_text(prior_record_path),
            'prior_record_sha256': str(get_optional_property(retry_obj, 'prior_record_sha256', 'N/A')),
        },
        'prior_blocked_binding': {
            'source': str(prior_blocked_binding['source']),
            'evidence_id': str(prior_blocked_binding['evidence_id']),
            'scenario_results_sha256': str(prior_blocked_binding['scenario_results_sha256']),
            'hash_manifest_sha256': str(prior_blocked_binding['hash_manifest_sha256']),
            'campaign_seal_sha256': str(prior_blocked_binding['campaign_seal_sha256']),
            'binding_source': 'freeze-manifest',
        } if prior_blocked_binding is not None else 'N/A',
        'execution_mode': execution_mode,
        'simulation': bool(live_simulation),
        'is_evidence': bool(is_evidence),
        'non_evidence_reason': 'N/A' if is_evidence else 'host-only dry-run or LiveSimulation; no physical-device evidence',
        'target_tuple': {
            'distribution': str(freeze['target_tuple']['distribution']),
            'device_model': freeze['target_tuple']['device_model'],
            'device_alias': 'PHYS-1',
            'full_system_build': freeze['target_tuple']['full_system_build'],
            'api': freeze['target_tuple']['api'],
            'architecture': 'arm64',
            'kernel_arch': freeze['target_tuple']['kernel_arch'],
            'app_abi': freeze['target_tuple']['app_abi'],
            'sdk_api_syscap': '%s / API %s / %s' % (freeze['sdk']['version'], freeze['sdk']['api'], freeze['sdk']['syscap_basis']),
            'channel': 'ordinary-development-signing-only',
        },
        'hdc_target_reference': 'PHYS-1 out-of-repository controlled mapping; real target never projected',
        'signing': freeze['signing'],
        'code_sha': str(freeze['code_sha']),
        'upstream_sha': 'N/A - no Go, NetBird, WireGuard, or other upstream runtime allowed',
        'source_archive_sha256': str(freeze['source']['archive_sha256']),
        'source_manifest_sha256': str(freeze['source']['manifest_sha256']),
        'sdk_sha256': [{'path_reference_sha256': sha256_text(str(f['path'])), 'sha256': str(f['sha256'])}
                       for f in freeze['sdk']['files']],
        'runner_sha256': str(freeze['runner_sha256']),
        'artifact_sha256': freeze['artifact_sha256'],
        'hdc': {'version': str(freeze['hdc']['version']), 'sha256': str(freeze['hdc']['sha256'])},
        'freeze_manifest_sha256': freeze_sha256,
        'freeze_contract_sha256': freeze_contract_sha256,
        'confirmation_contract_sha256': confirmation_contract_sha256,
        'preflight_inputs_frozen_at': str(freeze['preflight_inputs_frozen_at']),
        'scenario_window_seconds': 60,
        'observation_semantics': _OBSERVATION_SEMANTICS,
        'settings_reallow_expected_path': str(freeze['settings_reallow_expected_path']),
        'settings_reallow_path_policy': str(freeze['settings_reallow_path_policy']),
        'settings_revoke_mechanism': str(freeze['settings_revoke_mechanism']),
        'settings_vpn_page_policy': str(freeze['settings_vpn_page_policy']),
        'settings_vpn_page_observation_only': True,
        'destroy_terminal_policy': str(freeze['destroy_terminal_policy']),
        'process_absent_required_count': int(freeze['process_absent_required_count']),
        'process_absent_probe_spacing_seconds': float(freeze['process_absent_probe_spacing_seconds']),
        'process_probe_target': str(freeze['process_probe_target']),
        'cleanup_baseline': 'A/B absent; no A/B process; no active VPN; unrelated VPN isolated; staging removed before send',
        'scenarios': list(scenarios),
        'scenario_aggregation': {
            'mapping': '1=cleanup_and_install; 2=allow_and_fd; 3=active_stop; 4=deny; 5=settings_revoke; 6=second_vpn_conflict; 7=final_cleanup',
            'scenario_2_rule': 'overall is pass only when allow, vpn_on_create, and vpn_connection_create_fd are all pass; fail dominates blocked',
            'scenario_2_assertions': scenario2.get('assertions') if scenario2 is not None else None,
            'scenario_5_rule': 'settings-app-info-force-stop revoke under strong protocol: step3 strict machine layout gate verifies the visible AppDetail subtree, expected label, and force-stop control; step4 force-stop capture is observation-only; final effect requires :vpn Extension process consecutive absent plus bundle present; no operator technical-fact confirmation',
            'scenario_6_rule': 'machine-only conflict: B Start step3 uses entry-or-authorization; optional B Allow step4 requires authorization-dismissed; A exact-process before B Start is a pre-gate; one post-terminal checkpoint is observed after accepted/rejected terminal and gates only rejected; accepted_session_count_in_window counts CREATE_ACCEPTED markers matching verified A/B requestIds only, while foreign/missing accepted is invalid; B accepted or dual accepted is fail regardless of checkpoint status; frozen rejection passes only with verified A, while nonfrozen/rejected-terminal A unverifiable is blocked; no terminal remains runner blocked; any extra Start/Stop/order deviation is invalid; no_dual/dual operator fields are non-authoritative and must be absent/null',
            'scenario_7_rule': 'binds only S6 machine-verified active A request/bundle; expects UI_STOP/onDestroy/pre-destroy/destroy-begin and :vpn final state; wrong bundle stop or extra start is invalid; no FINAL-CLEANUP operator confirmation; uninstall cleanup only after terminal assessment; finally-absent never backfills',
            's3_strict_process_boundary_gate': 'scenario 3 strict-process-boundary fallback pass additionally requires scenario 5 same-bundle fresh request CREATE_ACCEPTED plus post-create open (clean_reactivation_proof); without it overall stays blocked',
            's3_clean_reactivation_proof': s3_proof_value,
            'overall_rule': 'integrity invalid > scenario invalid > fail > blocked > pass; first scenario invalid stops later scenarios as not-run-due-to-invalid; scenario 3 strict-process-boundary without clean reactivation proof => blocked; finally cleanup/seal never changes overall',
            'measured_scenario_overall': get_scenario_aggregation(scenarios),
            'overall': overall,
        },
        'started_at': isoformat_7(started_at),
        'ended_at': isoformat_7(ended_at),
        'clock_source': {
            'host': 'DateTimeOffset.Now recorded at observation',
            'device': 'HiLog year/zone timestamp; CST frozen to +08:00; unknown zone blocked',
            'device_zone_map': FROZEN_DEVICE_ZONE_MAP,
            'device_clock_skew_tolerance_seconds': float(DEVICE_CLOCK_SKEW_TOLERANCE_SECONDS),
            'host_observed_time_recorded': True,
            'virtual_clock': bool(live_simulation),
        },
        'raw_hilog_reference': get_public_raw_references(),
        'operator_wait_state_reference': {
            'path': 'operator-wait-state.json',
            'sha256': sha256_file(os.path.join(evidence_path, 'operator-wait-state.json')),
            'sealed_by': 'hash-manifest.json',
            'pollable_without_device_commands': True,
        },
        'transcript_reference': {
            'path': 'projection/transcript.redacted.jsonl',
            'sha256': sha256_file(projection_transcript),
            'projection_only': True,
            'raw_transcript_exists': False,
            'chain_head': transcript_previous_hash,
        },
        'screenshot_reference': get_public_screenshot_references(),
        'layout_state_reference': get_public_layout_references(),
        'fault_reference': {
            'strategy': 'static read-only A/B-targeted find by exact frozen bundle name; each output is an independent RawRoot artifact',
            'degraded': bool(len([d for d in capture_degraded if str(d['component']) in ('FaultA', 'FaultB')]) > 0),
            'artifacts': get_public_fault_references(),
        },
        'hash_manifest_reference': {'path': 'hash-manifest.json', 'sha256': manifest_sha256,
                                    'sealed_by': 'campaign-seal.json'},
        'forbidden_capabilities_audit': {
            'no_go': True, 'no_netbird': True, 'no_wireguard': True, 'no_private_fork': True,
            'no_manage_vpn': True, 'no_privileged_bypass': True, 'no_automated_device_input': True,
            'source_audit_basis': 'frozen source manifest and runner allowlist',
        },
        'actual': 'Bounded seven-scenario observation; see scenarios and raw references.' if not fatal_message
                  else 'Runner stopped: %s' % fatal_message,
        'overall': overall,
        'verdict': overall,
        'scope_statement': 'Exact frozen target E3 reachability only; no E4-E7, product, data-plane, or E8 OPEN conclusion.',
        'cleanup_result': {
            'status': cleanup_verification['status'],
            'verified_absent': bool(cleanup_verification['verified_absent']),
            'installed_a_remaining': bool(installed_a),
            'installed_b_remaining': bool(installed_b),
            'staging_sent_remaining': bool(staging_sent),
            'staging_may_exist_remaining': bool(staging_may_exist),
            'targeted_verification': cleanup_verification['bundles'],
            'actions': list(cleanup_actions),
            'pre_uninstall_fd_snapshot_required': True,
            'force_stop_role': 'notUsedAsRevoke residual cleanup only',
        },
        'capture_degraded': list(capture_degraded),
        'observation_only_degraded': list(observation_only_degraded),
        'integrity_violations': [],
        'repository_before': repository_before['fingerprint'],
        'hdc_logical_calls': hdc_logical_call_count,
        'hdc_processes_started': hdc_process_start_count,
        'operator': {'role': str(freeze['operator_role']),
                     'attestation': 'collected-separately' if is_evidence else 'not-attested-non-evidence'},
        'reviewer': 'pending',
        'reviewer_role': str(freeze['independent_reviewer_role']),
        'reviewed_at': 'pending',
        'review_record': 'pending',
        'machine_fresh_confirmation': {
            'status': 'pass',
            'authorization_id': str(machine_fresh_confirmation['authorization_id']),
            'record_sha256': str(machine_fresh_confirmation['record_sha256']),
            'record_path_sha256': str(machine_fresh_confirmation['record_path_sha256']),
            'confirmation_contract_sha256': confirmation_contract_sha256,
        } if machine_fresh_confirmation is not None else 'N/A',
        'independent_review_record': {
            'status': 'pass',
            'reviewer_role': str(independent_review_record['reviewer_role']),
            'record_sha256': str(independent_review_record['record_sha256']),
            'record_path_sha256': str(independent_review_record['record_path_sha256']),
            'confirmation_contract_sha256': confirmation_contract_sha256,
        } if independent_review_record is not None else 'N/A',
    }
    if fatal_message:
        record['failure'] = fatal_message
    if infrastructure_reason:
        record['infrastructure_reason'] = infrastructure_reason
    if scenario_invalid is not None:
        record['invalidated_step'] = {
            'scenario': int(scenario_invalid['scenario']),
            'step_index': scenario_invalid['step_index'],
            'step_id': scenario_invalid['step_id'],
            'reason': str(scenario_invalid['reason']),
            'detected_at': str(scenario_invalid['detected_at']),
        }
        record['scenario_invalid'] = scenario_invalid
    return record


def _payload_raw_text(line):
    """C4: raw JSON text of the 'payload' member without object round-trip
    (PS JsonElement.GetRawText). Returns None when not found/unparseable."""
    decoder = json.JSONDecoder()
    idx = line.find('"payload":')
    if idx < 0:
        return None
    start = idx + len('"payload":')
    try:
        _, end = decoder.raw_decode(line, start)
    except ValueError:
        return None
    return line[start:end]


def test_transcript_integrity(transcript_path):
    """PS L4564-4607 Test-TranscriptIntegrity (C4): per-line JSON parse,
    payload raw text vs payload_canonical byte comparison (no object
    round-trip), previous_hash chain, index order, and the final chain head
    against TranscriptPreviousHash. Returns unique violations in order."""
    violations = []
    if not os.path.isfile(transcript_path):
        return ['transcript-missing']
    previous_hash = '0' * 64
    expected_index = 1
    with open(transcript_path, 'r', encoding='utf-8-sig') as f:
        for line in f:
            line = line.rstrip('\n').rstrip('\r')
            if not line.strip():
                continue
            try:
                doc = json.loads(line)
            except Exception:
                violations.append('transcript-json-invalid')
                continue
            try:
                payload = doc['payload']
                payload_raw = _payload_raw_text(line)
                payload_canonical = doc['payload_canonical']
                stored_entry_hash = doc['entry_hash']
                index = int(payload['index'])
                entry_previous_hash = str(payload['previous_hash'])
            except Exception:
                violations.append('transcript-json-invalid')
                continue
            if index != expected_index:
                violations.append('transcript-order-invalid')
            if entry_previous_hash != previous_hash:
                violations.append('transcript-previous-hash-invalid')
            if payload_raw is None or payload_raw != str(payload_canonical):
                violations.append('transcript-payload-canonical-mismatch')
            entry_hash = sha256_text(str(payload_canonical))
            if entry_hash != str(stored_entry_hash):
                violations.append('transcript-entry-hash-invalid')
            previous_hash = entry_hash
            expected_index += 1
    if previous_hash != transcript_previous_hash:
        violations.append('transcript-chain-head-invalid')
    return list(dict.fromkeys(violations))


def test_evidence_integrity(evidence_path, scenarios):
    """PS L4608-4651 Test-EvidenceIntegrity: transcript chain, hash-manifest
    presence + per-file hash, scenario order 1-7, sealed operator-wait-state
    must be complete, and collected capture references must exist. Returns
    unique violations in order."""
    violations = []
    for violation in test_transcript_integrity(projection_transcript):
        violations.append(violation)
    manifest_path = os.path.join(evidence_path, 'hash-manifest.json')
    if not os.path.isfile(manifest_path):
        violations.append('hash-manifest-missing')
    else:
        try:
            manifest = json.loads(read_text_utf8_sig(manifest_path))
            for entry in manifest['files']:
                file_path = normalize_path(os.path.join(evidence_path, str(entry['path']).replace('/', os.sep)))
                if not is_under_path(file_path, evidence_path) or not os.path.isfile(file_path):
                    violations.append('manifest-file-missing:%s' % entry['path'])
                elif sha256_file(file_path) != str(entry['sha256']):
                    violations.append('manifest-hash-mismatch:%s' % entry['path'])
        except Exception:
            violations.append('hash-manifest-invalid')
    sequence = [int(s['scenario']) for s in scenarios]
    if ','.join(str(x) for x in sequence) != '1,2,3,4,5,6,7':
        violations.append('scenario-order-invalid')
    wait_state_path = os.path.join(evidence_path, 'operator-wait-state.json')
    if not os.path.isfile(wait_state_path):
        violations.append('operator-wait-state-missing')
    else:
        try:
            wait_state = json.loads(read_text_utf8_sig(wait_state_path))
            if str(wait_state['phase']) != 'complete' or not bool(wait_state['complete']) \
                    or not str(wait_state.get('completed_at') or '').strip():
                violations.append('operator-wait-state-not-complete')
        except Exception:
            violations.append('operator-wait-state-invalid')
    for artifact in capture_artifacts:
        if artifact['status'] == 'collected' and (not os.path.isfile(artifact['screen_path'])
                                                  or not os.path.isfile(artifact['layout_path'])):
            violations.append('capture-reference-missing:%s' % artifact['name'])
    return list(dict.fromkeys(violations))


def write_collection_manifest(evidence_path):
    """PS L4519-4550 Write-CollectionManifest (C14): hash-manifest.json with
    schema_version=1, algorithm='SHA-256', transcript_chain_head, files[]
    (excluding the three seal files to avoid a self-reference cycle) and
    external_raw_files[]. Returns the manifest path."""
    manifest_path = os.path.join(evidence_path, 'hash-manifest.json')
    excluded = ('hash-manifest.json', 'scenario-results.json', 'campaign-seal.json')
    files = []
    for root, dirs, names in os.walk(evidence_path):
        for name in sorted(names):
            if name in excluded:
                continue
            full = os.path.join(root, name)
            if os.path.isfile(full):
                rel = os.path.relpath(full, evidence_path).replace(os.sep, '/')
                files.append({'path': rel, 'sha256': sha256_file(full), 'bytes': os.path.getsize(full)})
    files.sort(key=lambda e: e['path'])
    external = []
    for artifact in raw_hilog_artifacts:
        external.append({'reference': artifact['reference'], 'sha256': artifact['sha256'],
                         'bytes': artifact['bytes'], 'host_path_sha256': sha256_text(str(artifact['path']))})
    for artifact in capture_artifacts:
        if artifact['status'] == 'collected':
            for path in (artifact['screen_path'], artifact['layout_path']):
                external.append({'reference': 'RAW-%s' % os.path.basename(path), 'sha256': sha256_file(path),
                                 'bytes': os.path.getsize(path), 'host_path_sha256': sha256_text(str(path))})
    for artifact in fault_artifacts:
        external.append({'reference': artifact['reference'], 'sha256': artifact['sha256'],
                         'bytes': artifact['bytes'], 'host_path_sha256': sha256_text(str(artifact['path']))})
    write_json_file(manifest_path, {
        'schema_version': 1,
        'algorithm': 'SHA-256',
        'generated_at': isoformat_7(get_now()),
        'transcript_chain_head': transcript_previous_hash,
        'scope': 'collection artifacts; scenario-results.json is sealed separately to avoid a self-reference cycle',
        'files': files,
        'external_raw_files': external,
    })
    return manifest_path


def write_campaign_seal(evidence_path):
    """PS L4551-4563 Write-CampaignSeal (C14): campaign-seal.json binding
    record{path,sha256} + manifest{path,sha256} + sealed_at,
    schema_version=1, algorithm='SHA-256'."""
    record_path = os.path.join(evidence_path, 'scenario-results.json')
    manifest_path = os.path.join(evidence_path, 'hash-manifest.json')
    write_json_file(os.path.join(evidence_path, 'campaign-seal.json'), {
        'schema_version': 1,
        'algorithm': 'SHA-256',
        'record': {'path': 'scenario-results.json', 'sha256': sha256_file(record_path)},
        'manifest': {'path': 'hash-manifest.json', 'sha256': sha256_file(manifest_path)},
        'sealed_at': isoformat_7(get_now()),
    })


# =====================================================================
# Section 12: Failure classification and aggregation (design unit U7)
# =====================================================================


def get_failure_classification(message):
    """PS L4652-4678 Get-FailureClassification (C19): message-prefix and
    regex classification. SCENARIO_INVALID -> invalid/invalidated/no retry;
    FUNCTIONAL_FAIL -> fail/collected/no retry; RUNNER_HOST_FAILURE ->
    blocked/runner-host-failure/retry; HDC transport/timeout patterns ->
    blocked/hdc-usb-interruption/retry; IO/storage patterns ->
    blocked/collection-storage-failure/retry ('access denied' alone is not
    storage); everything else blocked/no retry."""
    if message.startswith('SCENARIO_INVALID') or re.match(r'^scenario-[1-7] SCENARIO_INVALID', message):
        return {'overall': 'invalid', 'record_status': 'invalidated',
                'infrastructure_reason': None, 'retry_authorized': False}
    if message.startswith('FUNCTIONAL_FAIL'):
        return {'overall': 'fail', 'record_status': 'collected',
                'infrastructure_reason': None, 'retry_authorized': False}
    if message.startswith('RUNNER_HOST_FAILURE'):
        return {'overall': 'blocked', 'record_status': 'blocked',
                'infrastructure_reason': 'runner-host-failure', 'retry_authorized': True}
    if re.search(r'(?i)exit\s*=\s*(124|125)|\bHDC(?:\s+operation)?\s+timeout\b|HDC infrastructure interruption|hdc-usb-interruption|HDC Process\.Start|\bUSB\b|\boffline\b|\bdisconnect(?:ed)?\b|transport (?:offline|error|fail)|target.+not found|connect(?:ion)?.+fail|channel.+fail|continuous capture infrastructure failure|raw-hilog-(?:start|process|stderr)|capture process exited|unable to start continuous campaign capture', message):
        return {'overall': 'blocked', 'record_status': 'blocked',
                'infrastructure_reason': 'hdc-usb-interruption', 'retry_authorized': True}
    if re.search(r'(?i)System\.IO\.IOException|disk full|not enough space|collection-storage-failure', message):
        return {'overall': 'blocked', 'record_status': 'blocked',
                'infrastructure_reason': 'collection-storage-failure', 'retry_authorized': True}
    if re.search(r'(?i)access denied writing|access denied while writing', message):
        return {'overall': 'blocked', 'record_status': 'blocked',
                'infrastructure_reason': 'collection-storage-failure', 'retry_authorized': True}
    return {'overall': 'blocked', 'record_status': 'blocked',
            'infrastructure_reason': None, 'retry_authorized': False}


def get_scenario_aggregation(scenarios, integrity_violation=False):
    """PS L3154-3172 Get-ScenarioAggregation (C20): integrity invalid >
    scenario invalid > fail > blocked > pass; exactly 7 scenarios all pass
    for pass; S3 strict-process-boundary without a clean reactivation proof
    stays blocked."""
    if integrity_violation:
        return 'invalid'
    results = [str(s['result']) for s in scenarios]
    if 'invalid' in results:
        return 'invalid'
    if 'fail' in results:
        return 'fail'
    if 'blocked' in results:
        return 'blocked'
    if len(results) != 7 or len([r for r in results if r != 'pass']) > 0:
        return 'invalid'
    s3 = next((s for s in scenarios if int(s['scenario']) == 3), None)
    if s3 is not None and str(s3.get('terminal_mode')) == 'strict-process-boundary' \
            and not bool(s3.get('clean_reactivation_proof')):
        return 'blocked'
    return 'pass'


def set_capture_degraded_scenarios(scenarios):
    """PS L4679-4696 Set-CaptureDegradedScenarios (C20): global (scenario 0)
    or per-scenario capture degradation downgrades non-fail/non-invalid
    results to blocked with reason capture-degraded; S2 assertions are
    blocked too. An explicit fail is never downgraded."""
    if len(capture_degraded) == 0:
        return
    global_degradation = len([d for d in capture_degraded if int(d['scenario']) == 0]) > 0
    affected = sorted(set(int(d['scenario']) for d in capture_degraded if int(d['scenario']) in range(1, 8)))
    for scenario in scenarios:
        if (global_degradation or int(scenario['scenario']) in affected) \
                and str(scenario['result']) not in ('fail', 'invalid'):
            scenario['result'] = 'blocked'
            scenario['reason'] = 'capture-degraded'
            if int(scenario['scenario']) == 2:
                scenario['assertions'] = {'allow': 'blocked', 'vpn_on_create': 'blocked',
                                           'vpn_connection_create_fd': 'blocked'}


# =====================================================================
# Section 13: Embedded pure-function selftest (design unit U8)
# =====================================================================


def count_hdc_processes():
    """PS L5698 HDC_PROCESSES semantics: read-only host process count via
    pgrep hdc - never starts hdc, never connects. Returns the number of
    matching host processes (0 when none match). Fail-closed (M4): returns
    -1 when pgrep itself is unavailable or errors, so a broken probe can
    never masquerade as a clean zero count."""
    try:
        proc = subprocess.run(['pgrep', '-x', 'hdc'], capture_output=True,
                              text=True, encoding='utf-8', timeout=10)
    except (OSError, subprocess.SubprocessError):
        return -1
    if proc.returncode == 1:
        return 0
    if proc.returncode != 0:
        return -1
    return len([line for line in proc.stdout.splitlines() if line.strip()])


def selftest():
    """PS L4697-5918 Invoke-RunnerSelfTest. Design unit U8 (embedded
    pure-function selftest): host-only checks - no network, no HDC, no
    Emulator, no evidence files. Reuses the in-file pure functions and
    goldens; each check prints SELFTEST_PASS/FAIL=<name> and the final
    line is SELFTEST_RESULT=pass HDC_PROCESSES=<count> (all pass) or
    SELFTEST_RESULT=fail. HDC_PROCESSES is a read-only pgrep count taken
    before the checks run (never starts hdc, never connects). When the
    pgrep probe itself fails, HDC_PROCESSES=unknown is printed and the
    result is fail (M4 fail-closed).
    Returns 0/1 for the main() SelfTest early exit."""
    global actual_target
    failures = []
    hdc_process_count = count_hdc_processes()
    if hdc_process_count < 0:
        failures.append('hdc-process-count-unknown')
        print('SELFTEST_FAIL=hdc-process-count-unknown')

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

    try:
        # 1. PS-compatible JSON serializer golden (R1/R6).
        for name, ok in jsoncompat_golden_selftest():
            check('jsoncompat-%s' % name, ok)

        # 2. SHA-256 tools.
        check('sha256-empty-vector',
              sha256_text('') == 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855')
        check('sha256-abc-vector',
              sha256_text('abc') == 'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad')
        check('sha256-hex-gate',
              test_sha256_hex('a' * 64) and not test_sha256_hex('A' * 64)
              and not test_sha256_hex('a' * 63) and not test_sha256_hex(''))

        # 3. Contract hash stability (two-phase projection, C5/C6).
        blocked_freeze = {'plan_status': 'blocked',
                          'preflight_inputs_frozen_at': '2099-01-01T00:00:00+00:00',
                          'operator_role': 'selftest-operator'}
        ready_freeze = {'plan_status': 'ready',
                        'preflight_inputs_frozen_at': '2099-01-01T00:00:10+00:00',
                        'operator_role': 'selftest-operator'}
        check('two-phase-full-contract-hashes-differ',
              get_freeze_contract_sha256(blocked_freeze) != get_freeze_contract_sha256(ready_freeze))
        check('two-phase-confirmation-contract-hash-identical',
              get_confirmation_contract_sha256(blocked_freeze) == get_confirmation_contract_sha256(ready_freeze))
        mutated_freeze = dict(ready_freeze)
        mutated_freeze['operator_role'] = 'some-other-operator'
        check('confirmation-contract-rejects-stable-mutation',
              get_confirmation_contract_sha256(mutated_freeze) != get_confirmation_contract_sha256(ready_freeze))

        # 4. Confirmation record schema round-trip (C6).
        selftest_freeze = {
            'campaign_id': CANDIDATE_CAMPAIGN_ID,
            'evidence_id': CANDIDATE_EVIDENCE_ID,
            'attempt': 'initial',
            'retry': {'basis': 'N/A', 'infrastructure_reason': 'N/A'},
            'plan_status': 'blocked',
            'code_sha': 'a' * 40,
            'runner_sha256': 'b' * 64,
            'hdc': {'sha256': 'c' * 64, 'version': 'SELFTEST-HDC-1.0'},
            'target_tuple': {'device_model': 'PLA-AL10',
                             'full_system_build': 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'},
        }
        record = new_target_binding_confirmation_record(
            selftest_freeze, 'e' * 64, 'f' * 64, {'fingerprint': 'g' * 64},
            datetime(2099, 1, 1, tzinfo=timezone.utc),
            datetime(2099, 1, 1, 0, 0, 5, tzinfo=timezone.utc),
            'pass', 'N/A', 'SELFTEST-HDC-1.0', 'PLA-AL10',
            'PLA-AL10 7.0.0.100(SP8C00E32R7P2)', 3, 3)
        check('confirmation-record-required-fields',
              record['schema_version'] == 1
              and record['record_kind'] == 'target-binding-confirmation'
              and record['is_evidence'] is False
              and record['verdict'] == 'pass'
              and record['command_attempted'] == 3
              and record['command_completed'] == 3
              and record['command_count'] == 3
              and record['target_redacted'] is True
              and record['device_alias'] == 'PHYS-1'
              and record['attempt'] == 'initial')
        record_json = jsoncompat_dumps(record)
        check('confirmation-record-schema-roundtrip', json.loads(record_json) == record)
        check('confirmation-record-no-target-or-secret',
              not re.search(r'(?i)(udid|serial|"target"\s*:|token|password|secret|endpoint|device-canary)',
                            record_json))

        # 5. HDC whitelist integrity (22 operations, exact parameters, argv
        # placeholder substitution).
        check('hdc-whitelist-22-operations', len(HDC_WHITELIST) == 22)
        check('hdc-whitelist-alias-coverage',
              set(HDC_OPERATION_ALIASES.values()) == set(HDC_WHITELIST))
        check('hdc-required-Bundle-rejected',
              raises(RuntimeError, lambda: assert_exact_command_parameters('BundleDump', {})))
        check('hdc-required-Name-rejected',
              raises(RuntimeError, lambda: assert_exact_command_parameters('ScreenCap', {})))
        check('hdc-unknown-command-rejected',
              raises(RuntimeError, lambda: assert_exact_command_parameters('NotAllowed', {})))
        check('hdc-StagingProbe-allowlisted',
              not raises(RuntimeError, lambda: assert_exact_command_parameters('StagingProbe', {})))
        check('hdc-StagingProbe-rejects-parameters',
              raises(RuntimeError, lambda: assert_exact_command_parameters('StagingProbe', {'Path': '/tmp/x'})))
        check('hdc-case-insensitive-alias',
              not raises(RuntimeError, lambda: assert_exact_command_parameters('bundledump', {'bundle': BUNDLE_A})))
        check('hdc-bundle-outside-allowlist-rejected',
              raises(RuntimeError, lambda: get_hdc_invocation('BundleDump', {'Bundle': 'evil'})))
        saved_target = actual_target
        try:
            actual_target = 'usb-target:8710'
            audit_arguments = get_hdc_invocation('BundleDump', {'Bundle': BUNDLE_A})
            live_arguments = get_live_hdc_arguments(audit_arguments, 'BundleDump', {'Bundle': BUNDLE_A})
            check('hdc-argv-placeholder-substitution',
                  '<PHYS_1_TARGET>' in audit_arguments and 'usb-target:8710' not in audit_arguments
                  and 'usb-target:8710' in live_arguments and '<PHYS_1_TARGET>' not in live_arguments)
        finally:
            actual_target = saved_target

        # 6. Redaction canaries (U1).
        saved_target = actual_target
        try:
            actual_target = 'target-canary.example.test:8710'
            sensitive = {
                'boolean': True,
                'nested': {
                    'target': actual_target,
                    'ipv4': '10.23.45.67:8710',
                    'ipv6': '[2001:db8::1234]:8710',
                    'ipv6_compressed_numeric': '2001:4860::1',
                    'ipv6_full_numeric': '2001:4860:0:0:0:0:0:1',
                    'host': 'host=device-canary.example.test:9911',
                    'mac': '00:11:22:33:44:55',
                    'serial': 'SN-CANARY12345678',
                },
            }
            protected = protect_sensitive_data(sensitive)
            round_trip = json.loads(jsoncompat_dumps(protected))
            protected_text = jsoncompat_dumps(protected)
            check('redaction-structured-preserves-Boolean',
                  isinstance(round_trip['boolean'], bool) and round_trip['boolean'])
            check('redaction-structured-canaries',
                  not re.search(r'10\.23\.45\.67|2001:db8|2001:4860|device-canary|00:11:22:33:44:55|CANARY12345678|target-canary',
                                protected_text))
        finally:
            actual_target = saved_target
        check('redaction-preserves-build-api-and-removes-ip-port',
              (lambda t: 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' in t and 'api=26' in t
               and not re.search(r'192\.0\.2\.44|8710', t))(
                  protect_sensitive_text('build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=192.0.2.44|port=8710')))
        check('redaction-api26-build-ip-like-literal',
              (lambda t: 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' in t and 'api=26' in t
               and 'bare=<REDACTED_IPV4>' in t and not re.search(r'198\.51\.100\.77|8710', t))(
                  protect_sensitive_text('full_system_build=PLA-AL10 7.0.0.100(SP8C00E32R7P2)|api=26|peer=198.51.100.77|port=8710|bare=7.0.0.100')))
        saved_literals = list(public_version_literals)
        try:
            public_version_literals[:] = ['HDC-7.0.0.100']
            check('redaction-preserves-hdc-version-literal',
                  (lambda t: 'HDC-7.0.0.100' in t and not re.search(r'192\.0\.2\.44|8710', t))(
                      protect_sensitive_text('hdc_version=HDC-7.0.0.100|peer=192.0.2.44|port=8710')))
        finally:
            public_version_literals[:] = saved_literals
        check('redaction-array-shape',
              jsoncompat_dumps(protect_sensitive_data(['alpha', ['beta', 'gamma']])) == '["alpha",["beta","gamma"]]')
        check('redaction-host-port',
              (lambda t: not re.search(r'device-canary|8710', t)
               and 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)' in t)(
                  jsoncompat_dumps(protect_sensitive_data(
                      {'host': 'device-canary.example.test', 'port': 8710,
                       'build': 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'}))))
        check('redaction-target-case-variant',
              not re.search(r'(?i)usb-target:8710', protect_sensitive_text('target=USB-TARGET:8710')))

        # 7. Numeric gates (ADJ-20260810-0001 C6, MAJOR-1 PS equivalence).
        check('json-integer-gate',
              test_json_integer(3) and test_json_integer(2.0)
              and not test_json_integer(True) and not test_json_integer(2.9)
              and not test_json_integer('3') and not test_json_integer(None))
        check('json-integer-coerce-lossless',
              coerce_json_integer(2.0) == 2 and coerce_json_integer(2.9) is None
              and coerce_json_integer(True) is None and coerce_json_integer('2') is None
              and coerce_json_integer(None) is None)
        check('json-double-gate',
              coerce_json_double(3) == 3.0 and coerce_json_double(3.0) == 3.0
              and coerce_json_double(True) is None and coerce_json_double('3') is None
              and coerce_json_double(float('nan')) is None and coerce_json_double(float('inf')) is None)
        check('strict-simulation-Boolean',
              raises(RuntimeError, lambda: get_optional_json_boolean({'hook': 'true'}, 'hook', False)))
        check('assert-json-boolean-accepts',
              not raises(RuntimeError, lambda: assert_json_boolean({'x': True}, 'x', True)))
        check('assert-json-boolean-rejects',
              raises(RuntimeError, lambda: assert_json_boolean({'x': 1}, 'x', True)))

        # 8. Atomic write no-clobber (A4) + record pair no-clobber (A5).
        with tempfile.TemporaryDirectory(prefix='e3-selftest-') as tmp_dir:
            atomic_target = os.path.join(tmp_dir, 'atomic.txt')
            atomic_write_text(atomic_target, 'first')
            check('atomic-write-content', read_text_utf8_sig(atomic_target) == 'first')
            check('atomic-write-no-clobber',
                  raises(FileExistsError, lambda: atomic_write_text(atomic_target, 'second')))
            check('atomic-write-bytes-unchanged', read_text_utf8_sig(atomic_target) == 'first')
        check('atomic-write-leaves-no-files', not os.path.exists(tmp_dir))
        with tempfile.TemporaryDirectory(prefix='e3-selftest-') as tmp_dir:
            blocked_record = new_target_binding_confirmation_record(
                selftest_freeze, 'e' * 64, 'f' * 64, {'fingerprint': 'g' * 64},
                datetime(2099, 1, 1, tzinfo=timezone.utc),
                datetime(2099, 1, 1, tzinfo=timezone.utc),
                'blocked', 'preflight: PHYS_1_TARGET must contain exactly one real target token',
                'SELFTEST-HDC-1.0', None, None, 0, 0)
            record_path = os.path.join(tmp_dir, 'target-binding-confirmation.json')
            record_sha = write_target_binding_confirmation_record_pair(record_path, blocked_record)
            companion_path = record_path + '.sha256'
            check('blocked-record-write-companion-exists', os.path.isfile(companion_path))
            check('blocked-record-write-companion-matches',
                  read_text_utf8_sig(companion_path).strip() == record_sha
                  and record_sha == sha256_file(record_path))
            record_bytes = open(record_path, 'rb').read()
            companion_bytes = open(companion_path, 'rb').read()
            check('preexisting-record-rejected',
                  raises(PreRecordGateError,
                         lambda: write_target_binding_confirmation_record_pair(record_path, blocked_record)))
            check('preexisting-record-bytes-unchanged', open(record_path, 'rb').read() == record_bytes)
            check('preexisting-record-companion-unchanged', open(companion_path, 'rb').read() == companion_bytes)
        check('record-write-selftest-leaves-no-files', not os.path.exists(tmp_dir))

        # 9. Timestamp format (A11/R4).
        utc_dt = datetime(2099, 1, 1, 12, 34, 56, 789000, tzinfo=timezone.utc)
        cst_dt = datetime(2099, 1, 1, 12, 0, 0, tzinfo=timezone(timedelta(hours=8)))
        check('isoformat-7-fractional-digits', isoformat_7(utc_dt) == '2099-01-01T12:34:56.7890000+00:00')
        check('isoformat-7-colon-offset', isoformat_7(cst_dt) == '2099-01-01T12:00:00.0000000+08:00')
        check('timestamp-parse-roundtrip', parse_datetime(isoformat_7(utc_dt)) == utc_dt)
        check('timestamp-parse-z-suffix', parse_datetime('2099-01-01T12:34:56Z') is not None)
        check('timestamp-parse-rejects-garbage', parse_datetime('not-a-date') is None)
    except Exception as e:
        failures.append('selftest-internal-error: %s' % str(e))
        print('SELFTEST_FAIL=selftest-internal-error')
    if failures:
        if hdc_process_count < 0:
            print('SELFTEST_RESULT=fail HDC_PROCESSES=unknown')
        else:
            print('SELFTEST_RESULT=fail')
        return 1
    print('SELFTEST_RESULT=pass HDC_PROCESSES=%d' % hdc_process_count)
    return 0


# =====================================================================
# Section 14: Main flow (argparse entry, mode dispatch, RUNNER_RESULT)
# =====================================================================


class _ArgParseError(Exception):
    """MINOR-4: raised by _CampaignArgumentParser.error() so argparse parse
    errors (unknown argument, missing value, invalid type) map to the
    pre-record gate exit code 1 instead of argparse's default SystemExit(2)."""


class _CampaignArgumentParser(argparse.ArgumentParser):
    def error(self, message):
        raise _ArgParseError(message)


def parse_args(argv=None):
    """PS param() L3-17: same 14 parameters, same names and semantics
    (design document section 1.2). allow_abbrev=False matches PS's
    full-parameter-name requirement."""
    parser = _CampaignArgumentParser(
        prog='e3-phys-preflight-campaign.py',
        description='E3-PHYS-PREFLIGHT campaign runner (Python port of e3-phys-preflight-campaign.ps1)',
        allow_abbrev=False)
    parser.add_argument('--FreezeManifest', '-FreezeManifest', '--freeze-manifest', dest='freeze_manifest', default=None,
                        help='Path to the freeze manifest JSON')
    parser.add_argument('--EvidenceRoot', '-EvidenceRoot', '--evidence-root', dest='evidence_root', default=None)
    parser.add_argument('--RawRoot', '-RawRoot', '--raw-root', dest='raw_root', default=None)
    parser.add_argument('--HapA', '-HapA', '--hap-a', dest='hap_a', default=None)
    parser.add_argument('--HapB', '-HapB', '--hap-b', dest='hap_b', default=None)
    parser.add_argument('--HdcPath', '-HdcPath', '--hdc-path', dest='hdc_path', default=None)
    parser.add_argument('--HdcTimeoutSeconds', '-HdcTimeoutSeconds', '--hdc-timeout-seconds', dest='hdc_timeout_seconds',
                        type=int, default=20)
    parser.add_argument('--OperatorTimeoutSeconds', '-OperatorTimeoutSeconds', '--operator-timeout-seconds', dest='operator_timeout_seconds',
                        type=int, default=300)
    parser.add_argument('--DryRun', '-DryRun', '--dry-run', dest='dry_run', action='store_true')
    parser.add_argument('--LiveSimulation', '-LiveSimulation', '--live-simulation', dest='live_simulation', action='store_true')
    parser.add_argument('--SimulationFixture', '-SimulationFixture', '--simulation-fixture', dest='simulation_fixture', default=None)
    parser.add_argument('--SelfTest', '-SelfTest', '--self-test', dest='self_test', action='store_true')
    parser.add_argument('--TargetBindingConfirm', '-TargetBindingConfirm', '--target-binding-confirm', dest='target_binding_confirm',
                        action='store_true')
    parser.add_argument('--ConfirmationRecord', '-ConfirmationRecord', '--confirmation-record', dest='confirmation_record', default=None)
    return parser.parse_args(argv)


def _set_script_state(args):
    """Mirror PS script-scope parameter variables from argparse."""
    global freeze_manifest, evidence_root, raw_root, hap_a, hap_b, hdc_path
    global hdc_timeout_seconds, operator_timeout_seconds, dry_run, live_simulation
    global simulation_fixture, self_test, target_binding_confirm, confirmation_record
    global no_device_mode, execution_mode
    freeze_manifest = args.freeze_manifest
    evidence_root = args.evidence_root
    raw_root = args.raw_root
    hap_a = args.hap_a
    hap_b = args.hap_b
    hdc_path = args.hdc_path
    hdc_timeout_seconds = args.hdc_timeout_seconds
    operator_timeout_seconds = args.operator_timeout_seconds
    dry_run = args.dry_run
    live_simulation = args.live_simulation
    simulation_fixture = args.simulation_fixture
    self_test = args.self_test
    target_binding_confirm = args.target_binding_confirm
    confirmation_record = args.confirmation_record
    no_device_mode = bool(dry_run or live_simulation or self_test)
    if target_binding_confirm:
        execution_mode = 'target-binding-confirm'
    elif dry_run:
        execution_mode = 'dry-run'
    elif live_simulation:
        execution_mode = 'live-simulation'
    else:
        execution_mode = 'live'


def _validate_required_args(args):
    """PS L5924-5932: required parameters + timeout ranges (after the
    SelfTest early exit)."""
    if not (args.freeze_manifest or '').strip() or not (args.hap_a or '').strip() \
            or not (args.hap_b or '').strip() or not (args.hdc_path or '').strip():
        raise RuntimeError('FreezeManifest, HapA, HapB, and HdcPath are required unless SelfTest is used')
    if not args.target_binding_confirm and not (args.evidence_root or '').strip():
        raise RuntimeError('EvidenceRoot is required unless SelfTest or TargetBindingConfirm is used')
    if args.hdc_timeout_seconds < 1 or args.hdc_timeout_seconds > 120:
        raise RuntimeError('HdcTimeoutSeconds must be between 1 and 120')
    if args.operator_timeout_seconds < 1 or args.operator_timeout_seconds > 900:
        raise RuntimeError('OperatorTimeoutSeconds must be between 1 and 900')
    if args.live_simulation:
        if not (args.simulation_fixture or '').strip() or not os.path.isfile(args.simulation_fixture):
            raise RuntimeError('LiveSimulation requires SimulationFixture')


def initialize_prior_blocked_binding(freeze):
    """PS L1251-1262 Initialize-PriorBlockedBinding: projects the frozen
    prior-blocked binding into the transcript (hashes only, never paths)."""
    global prior_blocked_binding
    prior_blocked_binding = get_prior_blocked_binding(freeze)
    if prior_blocked_binding is None:
        return
    add_transcript_record('prior-blocked-binding', {
        'source': str(prior_blocked_binding['source']),
        'evidence_id': str(prior_blocked_binding['evidence_id']),
        'scenario_results_sha256': str(prior_blocked_binding['scenario_results_sha256']),
        'hash_manifest_sha256': str(prior_blocked_binding['hash_manifest_sha256']),
        'campaign_seal_sha256': str(prior_blocked_binding['campaign_seal_sha256']),
        'binding_source': 'freeze-manifest',
    })


def initialize_output_roots():
    """PS L403-442 Initialize-OutputRoots: EvidenceRoot/RawRoot must be
    independent sibling trees outside the repository; the external
    .campaign.lock is O_EXCL no-clobber; the projection transcript is
    created empty; campaign-lock.json records the lock hash. Sets
    EvidencePath/RawPath/ProjectionTranscript and resets the transcript
    chain."""
    global evidence_path, raw_path, projection_transcript, transcript_index, transcript_previous_hash
    evidence_candidate = normalize_path(evidence_root)
    if os.path.exists(evidence_candidate):
        raise RuntimeError('EvidenceRoot already exists; existing evidence is immutable and selective rerun is forbidden')
    raw_candidate_input = evidence_candidate + '.raw' if not raw_root else raw_root
    raw_candidate = normalize_path(raw_candidate_input)
    if os.path.exists(raw_candidate):
        raise RuntimeError('RawRoot already exists; existing evidence is immutable and selective rerun is forbidden')
    if raw_candidate == evidence_candidate or is_under_path(raw_candidate, evidence_candidate) \
            or is_under_path(evidence_candidate, raw_candidate):
        raise RuntimeError('EvidenceRoot and RawRoot must be independent sibling trees')
    if evidence_candidate == repo_root or is_under_path(evidence_candidate, repo_root) \
            or raw_candidate == repo_root or is_under_path(raw_candidate, repo_root):
        raise RuntimeError('EvidenceRoot and RawRoot must be outside the git repository')
    assert_no_reparse_ancestor(evidence_candidate)
    assert_no_reparse_ancestor(raw_candidate)
    external_lock = evidence_candidate + '.campaign.lock'
    if os.path.exists(external_lock):
        raise RuntimeError('campaign lock already exists; selective rerun is forbidden')
    assert_no_reparse_ancestor(external_lock)
    acquire_campaign_lock(external_lock)
    os.makedirs(evidence_candidate, exist_ok=True)
    os.makedirs(raw_candidate, exist_ok=True)
    # MAJOR-4: re-verify the output roots immediately after creation - the
    # ancestor walk now includes the created directories themselves, so a
    # symlink raced in between the pre-check and makedirs is caught here.
    # Failure raises (exit 1) before any projection file is written.
    assert_no_reparse_ancestor(evidence_candidate)
    assert_no_reparse_ancestor(raw_candidate)
    if evidence_candidate == repo_root or is_under_path(evidence_candidate, repo_root) \
            or raw_candidate == repo_root or is_under_path(raw_candidate, repo_root):
        raise RuntimeError('EvidenceRoot and RawRoot must be outside the git repository')
    projection = os.path.join(evidence_candidate, 'projection')
    os.makedirs(projection, exist_ok=True)
    projection_transcript = os.path.join(projection, 'transcript.redacted.jsonl')
    with open(projection_transcript, 'w', encoding='utf-8', newline='') as f:
        pass
    write_json_file(os.path.join(evidence_candidate, 'campaign-lock.json'), {
        'exception': 'E3-PHYS-PREFLIGHT',
        'execution_mode': execution_mode,
        'created_at': isoformat_7(get_now()),
        'external_lock_sha256': sha256_file(external_lock),
    })
    evidence_path = evidence_candidate
    raw_path = raw_candidate
    transcript_index = 0
    transcript_previous_hash = '0' * 64
    return {'evidence': evidence_candidate, 'raw': raw_candidate, 'external_lock': external_lock}


def run_campaign(freeze, freeze_sha256, freeze_contract_sha256, confirmation_contract_sha256, repository_before):
    """PS L5755-5918 main campaign orchestration (U10): Initialize-OutputRoots,
    preflight transcript records, DryRun / StrongLive dispatch, exception
    classification with partial-scenario preservation, finally cleanup +
    capture stop + degradation, attestation + complete wait state, manifest /
    complete record / seal, integrity violations (tamper hooks, repository
    drift, Test-EvidenceIntegrity), and the RUNNER_RESULT exit line. Returns
    0 (pass) or 2 (fatal / invalid)."""
    global evidence_path, raw_path, projection_transcript, transcript_index, transcript_previous_hash
    global partial_scenarios, campaign_phase, campaign_started, cleanup_verification, capture_degraded
    global infrastructure_reason_observed, scenario_invalid, operator_wait_history, operator_wait_current
    global installed_a, installed_b, staging_sent, staging_may_exist, cleanup_actions
    initialize_output_roots()
    started_at = get_now()
    scenarios = new_blocked_scenarios('not-run')
    overall = 'blocked'
    record_status = 'blocked'
    fatal_message = None
    infrastructure_reason = None
    integrity_violations = []
    try:
        add_transcript_record('preflight-gates-pass', {
            'exception': str(freeze['exception']),
            'campaign_id': str(freeze['campaign_id']),
            'attempt': str(freeze['attempt']),
            'plan_status': str(freeze['plan_status']),
            'execution_mode': execution_mode,
            'repository': repository_before['fingerprint'],
            'freeze_manifest_sha256': freeze_sha256,
        })
        add_transcript_record('machine-fresh-confirmation', {
            'status': 'pass' if machine_fresh_confirmation is not None else 'not-required',
            'authorization_id': str(machine_fresh_confirmation['authorization_id']) if machine_fresh_confirmation is not None else 'N/A',
            'record_sha256': str(machine_fresh_confirmation['record_sha256']) if machine_fresh_confirmation is not None else 'N/A',
            'record_path_sha256': str(machine_fresh_confirmation['record_path_sha256']) if machine_fresh_confirmation is not None else 'N/A',
            'confirmation_contract_sha256': confirmation_contract_sha256,
        })
        add_transcript_record('independent-review-record', {
            'status': 'pass' if independent_review_record is not None else 'not-required',
            'reviewer_role': str(independent_review_record['reviewer_role']) if independent_review_record is not None else 'N/A',
            'record_sha256': str(independent_review_record['record_sha256']) if independent_review_record is not None else 'N/A',
            'record_path_sha256': str(independent_review_record['record_path_sha256']) if independent_review_record is not None else 'N/A',
            'confirmation_contract_sha256': confirmation_contract_sha256,
        })
        initialize_prior_blocked_binding(freeze)
        if dry_run:
            scenarios = invoke_dry_run_campaign()
            overall = 'blocked'
            record_status = 'blocked'
        else:
            scenarios = invoke_strong_live_campaign(freeze)
            measured_overall = get_scenario_aggregation(scenarios)
            if live_simulation and measured_overall != 'invalid':
                overall = 'blocked'
                record_status = 'blocked'
            else:
                overall = measured_overall
                record_status = 'invalidated' if measured_overall == 'invalid' else 'collected'
    except Exception as e:
        raw_exception = str(e)
        phase = str(campaign_phase)
        phase_match = re.match(r'^scenario-([1-7])$', phase)
        if phase_match and not re.search(r'scenario-[1-7]', raw_exception):
            raw_exception = 'scenario-%s %s' % (phase_match.group(1), raw_exception)
        elif phase == 'preflight' and not re.search(r'(?i)^preflight\b|collection preparation blocked|scenario-[1-7]', raw_exception):
            raw_exception = 'preflight: %s' % raw_exception
        fatal_message = protect_sensitive_text(raw_exception)
        classification = get_failure_classification(fatal_message)
        is_scenario_invalid = scenario_invalid is not None or classification['overall'] == 'invalid'
        overall = 'invalid' if is_scenario_invalid else (classification['overall'] if execution_mode == 'live' else 'blocked')
        record_status = 'invalidated' if is_scenario_invalid else (classification['record_status'] if execution_mode == 'live' else 'blocked')
        infrastructure_reason = classification['infrastructure_reason']
        failed_scenario = None
        if scenario_invalid is not None:
            failed_scenario = int(scenario_invalid['scenario'])
        else:
            failed_match = re.search(r'scenario-([1-7])', fatal_message)
            if failed_match:
                failed_scenario = int(failed_match.group(1))
        if phase == 'preflight' or re.search(r'(?i)^preflight\b|collection preparation blocked', fatal_message):
            default_reason = 'preflight-or-collection-preparation-blocked'
        elif is_scenario_invalid:
            default_reason = 'not-run-due-to-invalid'
        else:
            default_reason = 'not-run-after-runner-failure'
        scenarios = new_blocked_scenarios(default_reason)
        for partial_scenario in partial_scenarios:
            scenarios[int(partial_scenario['scenario']) - 1] = partial_scenario
        if is_scenario_invalid:
            invalid_scenario_number = failed_scenario if failed_scenario is not None else 0
            for index in range(7):
                scenario_number = index + 1
                already_measured = len([s for s in partial_scenarios if int(s['scenario']) == scenario_number]) > 0
                if already_measured:
                    continue
                entry = scenarios[index]
                if scenario_number == invalid_scenario_number:
                    entry['result'] = 'invalid'
                    entry['reason'] = str(scenario_invalid['reason']) if scenario_invalid is not None else fatal_message
                else:
                    entry['result'] = 'invalid'
                    entry['reason'] = 'not-run-due-to-invalid'
                if scenario_number == 2 and entry.get('assertions') is None:
                    entry['assertions'] = {'allow': 'invalid', 'vpn_on_create': 'invalid',
                                           'vpn_connection_create_fd': 'invalid'}
        elif failed_scenario is not None:
            already_measured = len([s for s in partial_scenarios if int(s['scenario']) == failed_scenario]) > 0
            if not already_measured:
                scenario_entry = scenarios[failed_scenario - 1]
                scenario_entry['result'] = overall
                scenario_entry['reason'] = fatal_message
                if failed_scenario == 2:
                    scenario_entry['assertions'] = {'allow': overall, 'vpn_on_create': 'blocked',
                                                     'vpn_connection_create_fd': 'blocked'}
        add_transcript_record('runner-exception', {'message': fatal_message, 'campaign_phase': phase,
                                                   'campaign_started': bool(campaign_started),
                                                   'infrastructure_reason': infrastructure_reason})
    finally:
        try:
            cleanup_reason = 'exception-cleanup' if fatal_message is not None else 'final-cleanup'
            invoke_precise_finally_cleanup(cleanup_reason)
        except Exception as e:
            cleanup_failure = protect_sensitive_text(str(e))
            cleanup_verification = {'status': 'blocked-cleanup-exception', 'verified_absent': False, 'bundles': []}
            capture_degraded.append({'scenario': 7, 'component': 'finally-cleanup', 'reason': cleanup_failure,
                                     'category': 'non-infrastructure', 'infrastructure_reason': None})
            if fatal_message is None:
                fatal_message = 'RUNNER_HOST_FAILURE %s' % cleanup_failure
                infrastructure_reason = 'runner-host-failure'
        if campaign_capture is not None:
            try:
                stop_campaign_hilog_capture(campaign_capture)
            except Exception as e:
                stop_failure = protect_sensitive_text(str(e))
                capture_degraded.append({'scenario': 0, 'component': 'raw-hilog-finalize', 'reason': stop_failure,
                                         'category': 'non-infrastructure', 'infrastructure_reason': None})
        set_capture_degraded_scenarios(scenarios)
        if not infrastructure_reason and infrastructure_reason_observed:
            infrastructure_reason = infrastructure_reason_observed
        measured_overall = get_scenario_aggregation(scenarios)
        if measured_overall == 'invalid' or scenario_invalid is not None:
            overall = 'invalid'
            record_status = 'invalidated'
        elif len(capture_degraded) > 0 or (not dry_run and not cleanup_verification['verified_absent']):
            overall = 'fail' if measured_overall == 'fail' else 'blocked'
            record_status = 'blocked'
        elif execution_mode == 'live':
            overall = measured_overall
            record_status = 'collected'
        else:
            overall = 'fail' if measured_overall == 'fail' else 'blocked'
            record_status = 'blocked'
        if hdc_process_start_count != 0 and no_device_mode:
            integrity_violations.append('nondevice-mode-started-hdc-process')
        add_transcript_record('campaign-finalizing', {
            'overall': overall,
            'record_status': record_status,
            'installed_a': bool(installed_a),
            'installed_b': bool(installed_b),
            'staging_sent': bool(staging_sent),
            'staging_may_exist': bool(staging_may_exist),
            'hdc_processes_started': hdc_process_start_count,
        })
        ended_at = get_now()
        attestation = {
            'schema_version': 2,
            'evidence_id': str(freeze['evidence_id']),
            'campaign_id': str(freeze['campaign_id']),
            'attempt': str(freeze['attempt']),
            'execution_mode': execution_mode,
            'operator_role': str(freeze['operator_role']),
            'trust_model': 'mechanical-action-only-machine-verified-v1',
            'attested': bool(execution_mode == 'live' and fatal_message is None),
            'statement': 'Operator performed only the single-step mechanical actions prompted by the runner and pressed Enter after each action. Operator attestation records mechanical step completion times only and does not contribute semantic verdicts; all scenario results are machine-verified.',
            'mechanical_actions': list(operator_actions),
            'record_status': 'collected' if execution_mode == 'live' else 'blocked',
            'reviewer': 'pending',
            'reviewed_at': 'pending',
        }
        write_json_file(os.path.join(evidence_path, 'operator-attestation.json'), attestation)
        write_operator_wait_state('complete')
        if live_simulation and get_optional_json_boolean(simulation, 'tamper_wait_state_after_complete', False):
            wait_path = os.path.join(evidence_path, 'operator-wait-state.json')
            wait_doc = json.loads(read_text_utf8_sig(wait_path))
            wait_doc['phase'] = 'waiting'
            wait_doc['complete'] = False
            wait_doc['completed_at'] = None
            write_json_file(wait_path, wait_doc)
        manifest_path = write_collection_manifest(evidence_path)
        manifest_sha256 = sha256_file(manifest_path)
        record = new_complete_record(freeze, scenarios, overall, record_status, started_at, ended_at,
                                     fatal_message, infrastructure_reason, repository_before, freeze_sha256,
                                     freeze_contract_sha256, confirmation_contract_sha256, manifest_sha256)
        record_path = os.path.join(evidence_path, 'scenario-results.json')
        write_json_file(record_path, record)
        write_campaign_seal(evidence_path)
        try:
            repository_after = get_repository_state(repo_root)
            if repository_after['fingerprint'] != repository_before['fingerprint']:
                integrity_violations.append('repository-drift')
        except Exception:
            integrity_violations.append('repository-state-after-unavailable')
        if live_simulation and get_optional_json_boolean(simulation, 'tamper_payload_after_manifest', False):
            lines = read_text_utf8_sig(projection_transcript).splitlines()
            if len(lines) > 0:
                tampered_entry = json.loads(lines[0])
                tampered_entry['payload']['kind'] = 'simulation-tampered-payload'
                lines[0] = jsoncompat_dumps(tampered_entry)
                write_text_utf8_no_bom(projection_transcript, '\n'.join(lines) + '\n')
        if live_simulation and get_optional_json_boolean(simulation, 'tamper_transcript_after_manifest', False):
            with open(projection_transcript, 'a', encoding='utf-8', newline='') as f:
                f.write('tamper\n')
        for violation in test_evidence_integrity(evidence_path, scenarios):
            integrity_violations.append(violation)
        if len(integrity_violations) > 0:
            record['integrity_violations'] = list(dict.fromkeys(integrity_violations))
            record['record_status'] = 'invalidated'
            record['overall'] = 'invalid'
            record['verdict'] = 'invalid'
            record['scenario_aggregation']['overall'] = 'invalid'
            overall = 'invalid'
            record_status = 'invalidated'
            write_json_file(record_path, record)
            write_campaign_seal(evidence_path)
    if no_device_mode and hdc_process_start_count != 0:
        raise RuntimeError('host-only safety invariant violated: HDC process count is nonzero')
    if fatal_message is not None:
        print('RUNNER_FAILURE=%s' % fatal_message)
    print('RUNNER_RESULT=%s RECORD_STATUS=%s MODE=%s EVIDENCE_ROOT=%s RAW_ROOT_HASH=%s HDC_PROCESSES=%d' % (
        overall, record_status, execution_mode, evidence_path, sha256_text(str(raw_path)), hdc_process_start_count))
    if fatal_message is not None or overall == 'invalid':
        return 2
    return 0


def main(argv=None):
    """PS L5920-5985 main flow. Gate order is preserved: mode exclusivity
    BEFORE the SelfTest early exit, then required parameters, then freeze
    parse/validate, contract hashes, repository state, target environment,
    confirm dispatch. Exit codes: 0=pass, 1=pre-record gate failure (or any
    pre-campaign validation error, matching PS's uncaught-exception exit 1,
    including argparse parse errors - MINOR-4), 2=probe/tuple blocked."""
    try:
        args = parse_args(argv)
    except _ArgParseError as e:
        sys.stderr.write('%s\n' % protect_sensitive_text(str(e)))
        return 1
    try:
        # 1. Mode exclusivity BEFORE SelfTest early exit (PS L5920-5922).
        assert_mode_exclusivity(args)
        # 2. SelfTest early exit (PS L5922-5924).
        if args.self_test:
            return selftest()
        # 3. Script-scope state + required parameters + timeouts (PS L5924-5932).
        _set_script_state(args)
        _validate_required_args(args)
        # 4. LiveSimulation fixture (PS L5933-5936).
        if args.live_simulation:
            global simulation
            simulation = json.loads(read_text_utf8_sig(args.simulation_fixture))
        # 5. Repository root + freeze parse (PS L5937-5945).
        global repo_root, freeze
        repo_root = get_git_repository_root()
        freeze_path = normalize_path(args.freeze_manifest)
        if not os.path.isfile(freeze_path):
            raise RuntimeError('FreezeManifest file missing')
        freeze = json.loads(read_text_utf8_sig(freeze_path))
        freeze_sha256 = sha256_file(freeze_path)
        # ADJ-20260810-0001 (C6): the frozen HDC version is a legitimate
        # public literal too - without it, an IP-like HDC version would be
        # redacted before it enters the confirmation record.
        public_version_literals[:] = [str(freeze['target_tuple']['full_system_build']),
                                       str(freeze['sdk']['version']),
                                       str(freeze['hdc']['version'])]
        # 6. Freeze validation (U2).
        assert_freeze_manifest(freeze, freeze_path)
        # 7. Contract hashes (U2): full freeze contract + stable
        # confirmation contract.
        freeze_contract_sha256 = get_freeze_contract_sha256(freeze)
        confirmation_contract_sha256 = get_confirmation_contract_sha256(freeze)
        # 8. Repository state (PS L5946-5949).
        repository_before = get_repository_state(repo_root)
        if str(freeze['code_sha']) != repository_before['head']:
            raise RuntimeError('freeze code_sha does not match repository HEAD')
        if not args.dry_run and not repository_before['clean']:
            raise RuntimeError('Live, LiveSimulation, and TargetBindingConfirm require a clean repository state')
        # 9. Target environment (PS L5950).
        if not no_device_mode and not args.target_binding_confirm:
            assert_target_environment()
        # 10. Confirm dispatch (PS L5951-5958).
        if args.target_binding_confirm:
            try:
                confirmation_result = invoke_target_binding_confirm(freeze, freeze_sha256,
                                                                    confirmation_contract_sha256,
                                                                    repository_before)
            except PreRecordGateError as e:
                sys.stderr.write('%s\n' % protect_sensitive_text(str(e)))
                return 1
            result_suffix = ' RECORD_SHA256=%s' % confirmation_result['record_sha256'] if confirmation_result['verdict'] == 'pass' else ''
            print('RUNNER_RESULT=%s MODE=target-binding-confirm RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false COMMAND_ATTEMPTED=%d COMMAND_COMPLETED=%d RECORD=%s%s' % (
                confirmation_result['verdict'], confirmation_result['command_attempted'],
                confirmation_result['command_completed'], confirmation_result['record_path'], result_suffix))
            if confirmation_result['verdict'] == 'pass':
                return 0
            return 2
        # 11. Campaign orchestration (U10 placeholder).
        return run_campaign(freeze, freeze_sha256, freeze_contract_sha256,
                            confirmation_contract_sha256, repository_before)
    except NotImplementedError:
        raise
    except Exception as e:
        # Pre-campaign validation errors exit 1 (PS: uncaught exception).
        sys.stderr.write('%s\n' % protect_sensitive_text(str(e)))
        return 1


if __name__ == '__main__':
    sys.exit(main())
