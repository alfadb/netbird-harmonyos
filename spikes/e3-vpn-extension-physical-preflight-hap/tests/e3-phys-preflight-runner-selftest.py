#!/usr/bin/env python3
"""E3-PHYS-PREFLIGHT runner selftest - Python port of
tests/e3-phys-preflight-runner-selftest.ps1 (2606 lines), U9a first batch.

Semantic-equivalent port of the PowerShell selftest for the Python runner
e3-phys-preflight-campaign.py. Every test spawns the runner as a real
subprocess (or imports it for pure-function checks) and asserts exit codes,
stdout lines, file artifacts and hashes. Test names follow the PS case
naming where one exists; each docstring carries the PS source line range
it was ported from (design document section 4).

U9a scope (this file, implemented):
  A. freeze gates        - plan_status blocked/ready acceptance matrix,
                           fixed candidate pair, decision-field
                           missing/illegal, legacy spacing, historical
                           runner binding, schema rejection
  B. TargetBindingConfirm - pass path + fake hdc, pre-record gate exit 1
                           (no record), probe blocked exit 2, tuple drift
                           blocked, double-file companion, record
                           round-trip consumption
  C. HDC whitelist       - 22 operations accepted, extra argv rejected,
                           case-insensitive equivalence, placeholder
                           substitution (audit keeps placeholders)
  D. DryRun              - is_evidence=false, HDC_PROCESSES=0, integrity
                           empty, 20 planned operations, blocked/ready
                           freeze both accepted
  E. --SelfTest          - exit 0 + SELFTEST_RESULT=PASS; mode mutual
                           exclusion; argparse errors

Run: python3 tests/e3-phys-preflight-runner-selftest.py
"""

import ast
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest

# =====================================================================
# Constants
# =====================================================================

HERE = os.path.dirname(os.path.abspath(__file__))
PROJECT = os.path.dirname(HERE)
RUNNER_SRC = os.path.join(PROJECT, 'e3-phys-preflight-campaign.py')

AUTH_ID = 'AUTH-E3-PHYS1API26-20260814-0001'
OLD_AUTH_ID = 'AUTH-E3-PHYS1API26-20260810-0002'
CANDIDATE_CAMPAIGN_ID = 'E3-PHYS-PREFLIGHT-20260814-0001'
CANDIDATE_EVIDENCE_ID = 'EV-E3-PHYS1API26-20260814-0001'
BUNDLE_A = 'cn.alfadb.netbird.e3physvpna'
BUNDLE_B = 'cn.alfadb.netbird.e3physvpnb'
MODEL = 'PLA-AL10'
BUILD = 'PLA-AL10 7.0.0.100(SP8C00E32R7P2)'
HDC_VERSION = 'SELFTEST-HDC-1.0'
FROZEN_AT = '2099-01-01T00:00:00+00:00'
TARGET = '192.168.1.100'

# HDC-MUST-NOT-START sentinel (design document section 4.2, option a):
# an executable marker script - if the runner ever executes it, a marker
# file is written so the test can detect the violation. The sentinel must
# pass the freeze's HDC file-hash gate, so its bytes are fixed and the
# hash is computed in the freeze constructor.
SENTINEL_SCRIPT = (
    '#!/bin/sh\n'
    '# E3-PHYS-PREFLIGHT HDC-MUST-NOT-START sentinel: if the runner ever\n'
    '# executes this script, a marker file is written so the test can\n'
    '# detect the violation (design document section 4.2, option a).\n'
    'printf "hdc-executed" > "${E3_HDC_MARKER:?}" 2>/dev/null || true\n'
    'exit 0\n'
)

# =====================================================================
# Runner module import (pure functions only; importing never executes any
# hdc device command - runner safety invariant).
# =====================================================================

_spec = importlib.util.spec_from_file_location('e3_phys_preflight_runner', RUNNER_SRC)
if _spec is None or _spec.loader is None:
    raise RuntimeError('unable to import runner module: %s' % RUNNER_SRC)
runner_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(runner_mod)

# =====================================================================
# Base helpers (PS Assert-True / Write-FixtureFile / Write-JsonFixture /
# Get-Sha256 / Copy-JsonObject / New-CasePaths / Invoke-Runner)
# =====================================================================


def sha256_file(path):
    """PS Get-Sha256 (L99-102): lowercase hex SHA-256 of file bytes."""
    with open(path, 'rb') as f:
        return hashlib.sha256(f.read()).hexdigest()


def sha256_text(text):
    """PS Get-TextSha256 (L94-97)."""
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def write_text(path, text):
    """PS Write-FixtureFile: UTF-8 without BOM (A1)."""
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(text)


def write_json(path, obj):
    """PS Write-JsonFixture: compact JSON + newline, UTF-8 no BOM."""
    write_text(path, json.dumps(obj, ensure_ascii=False, separators=(',', ':')) + '\n')


def deep_copy(obj):
    """PS Copy-JsonObject: JSON round-trip deep copy."""
    return json.loads(json.dumps(obj))


def make_fake_hdc(path, version=HDC_VERSION, model=MODEL, build=BUILD):
    """Fake hdc script generator: fixed fixture responses for the three
    target-binding probes (Version / TupleModel / TupleBuild). The runner
    invokes [hdc, -t, <target>, ...], so the script skips the -t pair."""
    script = (
        '#!/bin/sh\n'
        '# E3-PHYS-PREFLIGHT fake hdc: fixed fixture responses for the\n'
        '# three target-binding probes (Version / TupleModel / TupleBuild).\n'
        'if [ "$1" = "-t" ]; then shift 2; fi\n'
        'case "$1" in\n'
        '  version)\n'
        '    echo "%s"\n'
        '    exit 0\n'
        '    ;;\n'
        '  shell)\n'
        '    if [ "$2" = "param" ] && [ "$3" = "get" ] && [ "$4" = "const.product.model" ]; then\n'
        '      echo "%s"\n'
        '      exit 0\n'
        '    fi\n'
        '    if [ "$2" = "param" ] && [ "$3" = "get" ] && [ "$4" = "const.product.software.version" ]; then\n'
        '      echo "%s"\n'
        '      exit 0\n'
        '    fi\n'
        '    ;;\n'
        'esac\n'
        'exit 0\n'
    ) % (version, model, build)
    write_text(path, script)
    os.chmod(path, 0o755)
    return path


def setup_workspace():
    """Temporary git repository with the runner committed (clean,
    code_sha=HEAD) plus fixture input files and the fake/sentinel hdc
    scripts. Fixture hashes are self-referential: the freeze binds
    runner_sha256 = hash of the committed runner copy, code_sha = repo
    HEAD, artifact/source/sdk/hdc hashes = hash of the fixture files."""
    tmp = tempfile.mkdtemp(prefix='e3-phys-preflight-selftest-')
    repo = os.path.join(tmp, 'runner-repository')
    os.makedirs(repo)
    runner_dst = os.path.join(repo, 'e3-phys-preflight-campaign.py')
    shutil.copy2(RUNNER_SRC, runner_dst)
    subprocess.run(['git', 'init', '--quiet', repo], check=True)
    subprocess.run(['git', '-C', repo, 'config', 'user.email', 'e3-selftest@example.invalid'], check=True)
    subprocess.run(['git', '-C', repo, 'config', 'user.name', 'E3 Runner Selftest'], check=True)
    subprocess.run(['git', '-C', repo, 'add', 'e3-phys-preflight-campaign.py'], check=True)
    subprocess.run(['git', '-C', repo, 'commit', '--quiet', '-m', 'selftest runner snapshot'], check=True)
    head = subprocess.run(['git', '-C', repo, 'rev-parse', 'HEAD'],
                          capture_output=True, text=True, encoding='utf-8').stdout.strip()
    hap_a = os.path.join(tmp, 'final-a.hap')
    hap_b = os.path.join(tmp, 'final-b.hap')
    source_archive = os.path.join(tmp, 'source.tar')
    source_manifest = os.path.join(tmp, 'source-manifest.json')
    sdk_input = os.path.join(tmp, 'sdk-input.bin')
    write_text(hap_a, 'synthetic signed HAP A fixture')
    write_text(hap_b, 'synthetic signed HAP B fixture distinct')
    write_text(source_archive, 'synthetic source archive fixture')
    write_text(source_manifest, '{"fixture":true}')
    write_text(sdk_input, 'synthetic SDK fixture')
    sentinel = os.path.join(tmp, 'HDC-MUST-NOT-START.sh')
    write_text(sentinel, SENTINEL_SCRIPT)
    os.chmod(sentinel, 0o755)
    fake = make_fake_hdc(os.path.join(tmp, 'fake-hdc.sh'))
    # ENOEXEC sentinel (design document section 4.2, option b): chmod 755
    # but not an executable format - any exec attempt raises OSError
    # (Exec format error) which the runner maps to exit 125.
    enoexec = os.path.join(tmp, 'HDC-MUST-NOT-START-ENOEXEC.txt')
    write_text(enoexec, 'HDC-MUST-NOT-START sentinel: not an executable format\n')
    os.chmod(enoexec, 0o755)
    return {
        'tmp': tmp, 'repo': repo, 'runner': runner_dst, 'code_sha': head,
        'runner_sha256': sha256_file(runner_dst),
        'hap_a': hap_a, 'hap_b': hap_b,
        'source_archive': source_archive, 'source_manifest': source_manifest,
        'sdk_input': sdk_input,
        'hap_a_sha': sha256_file(hap_a), 'hap_b_sha': sha256_file(hap_b),
        'source_archive_sha': sha256_file(source_archive),
        'source_manifest_sha': sha256_file(source_manifest),
        'sdk_sha': sha256_file(sdk_input),
        'sentinel_hdc': sentinel, 'fake_hdc': fake, 'enoexec_hdc': enoexec,
        'hdc_marker': os.path.join(tmp, 'HDC-PROCESS-WAS-STARTED.txt'),
    }


def make_freeze(ws, plan_status='blocked', evidence_id=CANDIDATE_EVIDENCE_ID,
                campaign_id=CANDIDATE_CAMPAIGN_ID, hdc_path=None, **overrides):
    """Minimal legal freeze constructor (PS New-Freeze L250-330). All
    hashes are computed from the workspace fixtures (self-reference)."""
    hdc = hdc_path if hdc_path is not None else ws['sentinel_hdc']
    freeze = {
        'schema_version': 2,
        'plan_status': plan_status,
        'exception': 'E3-PHYS-PREFLIGHT',
        'evidence_id': evidence_id,
        'campaign_id': campaign_id,
        'attempt': 'initial',
        'prior_blocked_binding': 'N/A',
        'retry': {'basis': 'N/A', 'infrastructure_reason': 'N/A',
                  'prior_record_path': 'N/A', 'prior_record_sha256': 'N/A'},
        'scenario_window_seconds': 60,
        'device_alias': 'PHYS-1',
        'target_tuple': {
            'distribution': 'HarmonyOS',
            'device_model': MODEL,
            'full_system_build': BUILD,
            'api': '26',
            'kernel_arch': 'aarch64',
            'app_abi': 'arm64-v8a',
        },
        'settings_reallow_expected_path': 'direct-system-activation',
        'settings_reallow_path_policy': 'observation-only',
        'settings_revoke_mechanism': 'settings-app-info-force-stop',
        'settings_vpn_page_policy': 'observation-only',
        'destroy_terminal_policy': 'callback-or-strict-process-boundary',
        'process_absent_required_count': 2,
        'process_absent_probe_spacing_seconds': 3.0,
        'process_probe_target': '<bundle>:vpn',
        'operator_trust_model': 'mechanical-action-only-machine-verified-v1',
        'scenario_invalid_policy': 'stop-and-finally-cleanup-seal',
        'layout_verification_profile': 'deterministic-layout-v1',
        'vpn_conflict_rejection_codes': [2203002],
        'signing': {
            'type': 'ordinary-development',
            'device_in_profile': True,
            'device_in_profile_basis': 'selftest public verification basis',
            'public_fingerprint': 'SELFTEST-NON-SECRET',
            'verification_result': 'pass',
        },
        'artifact_sha256': {'hap_a': ws['hap_a_sha'], 'hap_b': ws['hap_b_sha']},
        'source': {
            'archive_path': ws['source_archive'],
            'archive_sha256': ws['source_archive_sha'],
            'manifest_path': ws['source_manifest'],
            'manifest_sha256': ws['source_manifest_sha'],
        },
        'sdk': {
            'version': 'synthetic-6.1.1',
            'api': '24',
            'syscap_basis': 'synthetic public VPN SysCap basis',
            'files': [{'path': ws['sdk_input'], 'sha256': ws['sdk_sha']}],
        },
        'hdc': {'version': HDC_VERSION, 'sha256': sha256_file(hdc)},
        'runner_sha256': ws['runner_sha256'],
        'code_sha': ws['code_sha'],
        'preflight_inputs_frozen_at': FROZEN_AT,
        'cleanup_baseline_frozen': True,
        'collection_ready': True,
        'independent_review_ready': True,
        'independent_review_record': {'status': 'pending'},
        'operator_role': 'selftest-operator',
        'independent_reviewer_role': 'selftest-independent-reviewer',
    }
    freeze.update(overrides)
    return freeze


def make_confirmation_record(ws, freeze, path, verdict='pass', **overrides):
    """PS Write-ConfirmationRecordFixture (L500-540): a valid
    target-binding-confirmation record + .sha256 companion. The
    confirmation_contract_sha256 is computed with the runner's own
    same-side mirror (C5/C6)."""
    record = {
        'schema_version': 1,
        'record_kind': 'target-binding-confirmation',
        'is_evidence': False,
        'authorization_id': AUTH_ID,
        'exception': 'E3-PHYS-PREFLIGHT',
        'campaign_id': freeze['campaign_id'],
        'evidence_id': freeze['evidence_id'],
        'attempt': 'initial',
        'retry': {'basis': 'N/A', 'infrastructure_reason': 'N/A'},
        'plan_status': 'ready',
        'device_alias': 'PHYS-1',
        'target_redacted': True,
        'code_sha': freeze['code_sha'],
        'runner_sha256': freeze['runner_sha256'],
        'freeze_manifest_sha256': 'e' * 64,
        'confirmation_contract_sha256': runner_mod.get_confirmation_contract_sha256(freeze),
        'hdc_sha256': freeze['hdc']['sha256'],
        'hdc_version': freeze['hdc']['version'],
        'expected_model': MODEL,
        'expected_build': BUILD,
        'observed_model': MODEL,
        'observed_build': BUILD,
        'started_at': '2098-12-31T23:59:55+00:00',
        'ended_at': '2098-12-31T23:59:59+00:00',
        'command_attempted': 3,
        'command_completed': 3,
        'command_count': 3,
        'repository_fingerprint': 'g' * 64,
        'verdict': verdict,
        'reason': 'N/A',
    }
    record.update(overrides)
    write_json(path, record)
    record_sha = sha256_file(path)
    write_text(path + '.sha256', record_sha + '\n')
    return path, record_sha


def make_review_record(ws, freeze, machine_sha, path,
                       reviewer_role='selftest-independent-reviewer', **overrides):
    """PS Write-ReviewRecordFixture (L540-570): a valid
    e3-ready-freeze-review record + .sha256 companion. Review times are
    anchored after the machine confirmation ended_at and at/before the
    final freeze preflight_inputs_frozen_at (time chain C6)."""
    record = {
        'schema_version': 1,
        'record_kind': 'e3-ready-freeze-review',
        'is_evidence': False,
        'exception': 'E3-PHYS-PREFLIGHT',
        'campaign_id': freeze['campaign_id'],
        'evidence_id': freeze['evidence_id'],
        'code_sha': freeze['code_sha'],
        'runner_sha256': freeze['runner_sha256'],
        'confirmation_contract_sha256': runner_mod.get_confirmation_contract_sha256(freeze),
        'machine_confirmation_sha256': machine_sha,
        'reviewer_role': reviewer_role,
        'operator_role': freeze['operator_role'],
        'verdict': 'pass',
        'blockers': 0,
        'majors': 0,
        'started_at': '2098-12-31T23:59:59+00:00',
        'ended_at': '2098-12-31T23:59:59+00:00',
    }
    record.update(overrides)
    write_json(path, record)
    record_sha = sha256_file(path)
    write_text(path + '.sha256', record_sha + '\n')
    return path, record_sha


def bind_ready_freeze(freeze, confirm_path, confirm_sha, review_path, review_sha):
    """PS Add-ConfirmationBinding + Add-ReviewBinding (L570-600): attach
    the machine confirmation and independent review bindings to a ready
    freeze. The confirmation contract is unchanged (plan_status and the
    binding fields are excluded from the stable projection)."""
    bound = deep_copy(freeze)
    bound['plan_status'] = 'ready'
    bound['machine_fresh_confirmation'] = {
        'status': 'pass',
        'authorization_id': AUTH_ID,
        'record_path': confirm_path,
        'record_sha256': confirm_sha,
    }
    bound['independent_review_record'] = {
        'status': 'pass',
        'record_path': review_path,
        'record_sha256': review_sha,
        'reviewer_role': 'selftest-independent-reviewer',
    }
    return bound


def make_simulation_fixture(scenario_events_overrides=None):
    """PS New-SimulationFixture (L160-260): base fixture for the complete
    live-simulation seven-scenarios phase. The fake hdc is not needed here -
    the runner's simulation layer answers every HDC operation from the
    fixture state machine and writes the fixed capture event stream (initial
    lines + per-step scenario events) into the raw HiLog file. The CANARY
    line in S2 exercises the raw-preserves / evidence-redacts split."""
    a = BUNDLE_A
    b = BUNDLE_B
    stamp = '<DEVICE_OBSERVED_AT>'
    fixture = {
        'hdc_version': HDC_VERSION,
        'capture_die_scenario': 0,
        'capture_initial_lines': [
            '2098-12-31 23:59:58.000+00:00 UI_START|bundle=%s|requestId=b4' % b,
            '2098-12-31 23:59:59.000+00:00 VPN_ONCREATE|bundle=%s|requestId=b4' % b,
            '2098-12-31 23:59:59.500+00:00 UI_START|bundle=%s|requestId=a5' % a,
        ],
        'operator': {'action_delay_seconds': 1, 'no_effect_steps': []},
        'layout_reviews': {},
        'layout_profiles': {},
        'hdc_failures': [],
        'capture_failures': [],
        'tamper_transcript_after_manifest': False,
        'tamper_payload_after_manifest': False,
        'scenario_events': {
            '1': [],
            '2': [
                {'offset_seconds': 1, 'text': '%s UI_START|bundle=%s|requestId=a2' % (stamp, a)},
                {'offset_seconds': 2, 'text': '%s VPN_ONCREATE|bundle=%s|requestId=a2' % (stamp, a)},
                {'offset_seconds': 3, 'text': '%s VPN_CREATE_RESOLVED|requestId=a2|fd=42|accepted=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 4, 'text': '%s VPN_FD_SNAPSHOT|requestId=a2|phase=post-create|open=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 5, 'text': '%s CANARY|target=target-canary.example.test:8710|ipv4=10.23.45.67:8710|ipv6=[2001:db8::1234]:8710|host=device-canary.example.test:9911|mac=00:11:22:33:44:55|serial=SN-CANARY12345678' % stamp},
            ],
            '3': [
                {'offset_seconds': 1, 'text': '%s UI_STOP|bundle=%s|requestId=a2|basis=last-known-request' % (stamp, a)},
                {'offset_seconds': 2, 'text': '%s STOP_PROMISE_RESOLVED|bundle=%s|requestId=a2' % (stamp, a)},
                {'offset_seconds': 3, 'text': '%s VPN_ONDESTROY|requestId=a2' % stamp},
                {'offset_seconds': 4, 'text': '%s VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy' % stamp},
                {'offset_seconds': 5, 'text': '%s VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_CLOSED_CONFIRMED' % stamp},
                {'offset_seconds': 6, 'text': '%s VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED' % stamp},
            ],
            '4': [
                {'offset_seconds': 1, 'text': '%s UI_START|bundle=%s|requestId=b4' % (stamp, b)},
                {'offset_seconds': 2, 'text': '%s START_PROMISE_REJECTED|bundle=%s|requestId=b4|summary=denied' % (stamp, b)},
            ],
            '5': [
                {'offset_seconds': 1, 'text': '%s UI_START|bundle=%s|requestId=a5' % (stamp, a)},
                {'offset_seconds': 2, 'text': '%s VPN_ONCREATE|bundle=%s|requestId=a5' % (stamp, a)},
                {'offset_seconds': 3, 'text': '%s VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 4, 'text': '%s VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 8, 'text': '%s VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED' % stamp},
                {'offset_seconds': 9, 'text': '%s VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED' % stamp},
            ],
            '6': [
                {'offset_seconds': 1, 'text': '%s UI_START|bundle=%s|requestId=a6' % (stamp, a)},
                {'offset_seconds': 2, 'text': '%s VPN_ONCREATE|bundle=%s|requestId=a6' % (stamp, a)},
                {'offset_seconds': 3, 'text': '%s VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 4, 'text': '%s VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED' % stamp},
                {'offset_seconds': 8, 'text': '%s UI_START|bundle=%s|requestId=b6' % (stamp, b)},
                {'offset_seconds': 9, 'text': '%s VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN' % stamp},
            ],
            '7': [
                {'offset_seconds': 1, 'text': '%s UI_STOP|bundle=%s|requestId=a6|basis=active-request' % (stamp, a)},
                {'offset_seconds': 2, 'text': '%s STOP_PROMISE_RESOLVED|bundle=%s|requestId=a6' % (stamp, a)},
                {'offset_seconds': 3, 'text': '%s VPN_ONDESTROY|requestId=a6' % stamp},
                {'offset_seconds': 4, 'text': '%s VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy' % stamp},
                {'offset_seconds': 5, 'text': '%s VPN_DESTROY_RESOLVED|requestId=a6|fdMarker=FD_CLOSED_CONFIRMED' % stamp},
                {'offset_seconds': 6, 'text': '%s VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED' % stamp},
            ],
        },
    }
    if scenario_events_overrides:
        fixture['scenario_events'].update(scenario_events_overrides)
    return fixture


def run_runner(ws, args, env=None):
    """PS Invoke-Runner (L230-250): spawn the runner as a real subprocess
    with the sentinel marker env always set."""
    cmd = [sys.executable, ws['runner']] + args
    full_env = dict(os.environ)
    full_env['E3_HDC_MARKER'] = ws['hdc_marker']
    if env:
        full_env.update(env)
    proc = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8',
                          env=full_env, timeout=180)
    return proc


def parse_runner_result(stdout):
    """Extract RUNNER_RESULT / RUNNER_FAILURE lines from stdout."""
    result = None
    failure = None
    for line in stdout.splitlines():
        if line.startswith('RUNNER_RESULT='):
            result = line
        elif line.startswith('RUNNER_FAILURE='):
            failure = line[len('RUNNER_FAILURE='):]
    return result, failure


def verify_transcript_chain(transcript_path):
    """PS Assert-ProjectionChain (L330-350): index order, previous_hash
    chain, payload raw text == payload_canonical (no object round-trip),
    entry hash. Returns a list of violations (empty = valid)."""
    violations = []
    previous_hash = '0' * 64
    expected_index = 1
    with open(transcript_path, 'r', encoding='utf-8-sig') as f:
        for line in f:
            line = line.rstrip('\n').rstrip('\r')
            if not line.strip():
                continue
            try:
                doc = json.loads(line)
                payload = doc['payload']
                idx = line.find('"payload":')
                start = idx + len('"payload":')
                _, end = json.JSONDecoder().raw_decode(line, start)
                payload_raw = line[start:end]
                canonical = doc['payload_canonical']
                entry_hash = doc['entry_hash']
                index = int(payload['index'])
                prev = str(payload['previous_hash'])
            except Exception:
                violations.append('transcript-json-invalid')
                continue
            if index != expected_index:
                violations.append('transcript-order-invalid')
            if prev != previous_hash:
                violations.append('transcript-previous-hash-invalid')
            if payload_raw != str(canonical):
                violations.append('transcript-payload-canonical-mismatch')
            if hashlib.sha256(str(canonical).encode('utf-8')).hexdigest() != str(entry_hash):
                violations.append('transcript-entry-hash-invalid')
            previous_hash = str(entry_hash)
            expected_index += 1
    return violations


def assert_evidence_outputs(evidence):
    """PS Assert-ManifestAndSeal (L310-330): sealed outputs exist and the
    manifest/seal hashes recompute."""
    manifest_path = os.path.join(evidence, 'hash-manifest.json')
    record_path = os.path.join(evidence, 'scenario-results.json')
    seal_path = os.path.join(evidence, 'campaign-seal.json')
    transcript_path = os.path.join(evidence, 'projection', 'transcript.redacted.jsonl')
    for path in (manifest_path, record_path, seal_path, transcript_path):
        if not os.path.isfile(path):
            return ['missing sealed output %s' % path]
    violations = []
    manifest = json.loads(open(manifest_path, encoding='utf-8').read())
    for entry in manifest.get('files', []):
        path = os.path.join(evidence, str(entry['path']).replace('/', os.sep))
        if sha256_file(path) != str(entry['sha256']):
            violations.append('manifest hash mismatch %s' % entry['path'])
    seal = json.loads(open(seal_path, encoding='utf-8').read())
    if sha256_file(record_path) != str(seal['record']['sha256']):
        violations.append('record seal mismatch')
    if sha256_file(manifest_path) != str(seal['manifest']['sha256']):
        violations.append('manifest seal mismatch')
    return violations


# =====================================================================
# Test base
# =====================================================================


class SelftestBase(unittest.TestCase):
    """Shared workspace: one temp git repo + fixtures per test class run."""

    @classmethod
    def setUpClass(cls):
        cls.ws = setup_workspace()

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.ws['tmp'], ignore_errors=True)

    def case_paths(self, name):
        """PS New-CasePaths: unique evidence/raw pair per case."""
        base = os.path.join(self.ws['tmp'], 'case-%s' % name)
        return base + '-evidence', base + '-raw'

    def write_freeze(self, freeze, name):
        path = os.path.join(self.ws['tmp'], name)
        write_json(path, freeze)
        return path

    def assert_rejected(self, proc, message, label):
        self.assertNotEqual(proc.returncode, 0, '%s: expected rejection, got exit 0\n%s' % (label, proc.stdout))
        self.assertRegex(proc.stdout + proc.stderr, message,
                         '%s: rejection message %r missing\nstdout=%s\nstderr=%s' % (
                             label, message, proc.stdout[-500:], proc.stderr[-500:]))


# =====================================================================
# Group A: freeze gates (PS L550-737 Assert-FreezeManifest order)
# =====================================================================


class TestFreezeGates(SelftestBase):
    """Freeze manifest gate negatives and the plan_status acceptance
    matrix. Every case runs DryRun (no device) so the failure is purely
    the freeze gate, never a device path."""

    def test_freeze_schema_rejected(self):
        """PS L550-552: schema_version must be the JSON integer 2."""
        freeze = make_freeze(self.ws, schema_version=1)
        proc = run_runner(self.ws, ['--FreezeManifest', self.write_freeze(freeze, 'freeze-schema.json'),
                                    '--EvidenceRoot'] + list(self.case_paths('schema'))[0:1] +
                           ['--RawRoot'] + list(self.case_paths('schema'))[1:2] +
                           ['--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                            '--HdcPath', self.ws['sentinel_hdc'], '--DryRun'])
        self.assert_rejected(proc, r'unsupported freeze schema_version', 'schema rejection')

    def test_freeze_plan_status_matrix(self):
        """PS L786-791 + L1740-1750 (blocked-live): DryRun accepts blocked
        and ready; LiveSimulation only accepts ready; unknown status is
        rejected for DryRun."""
        blocked = make_freeze(self.ws, plan_status='blocked')
        blocked_path = self.write_freeze(blocked, 'freeze-blocked.json')
        ev, raw = self.case_paths('plan-blocked')
        proc = run_runner(self.ws, ['--FreezeManifest', blocked_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assertEqual(proc.returncode, 0, 'DryRun blocked freeze rejected:\n%s' % proc.stdout)
        # LiveSimulation rejects blocked plan_status before any campaign work.
        fixture = os.path.join(self.ws['tmp'], 'sim-min.json')
        write_json(fixture, {'hdc_version': HDC_VERSION, 'scenario_events': {}})
        ev2, raw2 = self.case_paths('plan-blocked-live')
        proc2 = run_runner(self.ws, ['--FreezeManifest', blocked_path, '--EvidenceRoot', ev2,
                                    '--RawRoot', raw2, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--LiveSimulation', '--SimulationFixture', fixture])
        self.assert_rejected(proc2, r'Live and LiveSimulation require plan_status ready',
                             'blocked plan for LiveSimulation')
        # Unknown plan_status rejected for DryRun.
        weird = make_freeze(self.ws, plan_status='weird')
        weird_path = self.write_freeze(weird, 'freeze-weird-plan.json')
        ev3, raw3 = self.case_paths('plan-weird')
        proc3 = run_runner(self.ws, ['--FreezeManifest', weird_path, '--EvidenceRoot', ev3,
                                     '--RawRoot', raw3, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--DryRun'])
        self.assert_rejected(proc3, r'plan_status must be blocked or ready', 'unknown plan_status')

    def test_freeze_candidate_pair_fixed(self):
        """PS L832-834 + L430-440 (confirm-wrong-pair): TargetBindingConfirm
        under the current AUTH fixes one candidate pair; a wrong campaign_id
        is rejected before any HDC call."""
        freeze = make_freeze(self.ws, plan_status='blocked', campaign_id='E3-PHYS-PREFLIGHT-WRONG')
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-wrong-pair.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-wrong-pair.json')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path,
                                    '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'],
                                    '--TargetBindingConfirm', '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET})
        self.assert_rejected(proc, r'fixed candidate pair', 'wrong candidate pair')
        self.assertFalse(os.path.exists(record_path), 'rejected confirm wrote a record')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']), 'gate case launched the HDC sentinel')

    def test_freeze_decision_fields_missing(self):
        """PS L2500-2550 (legacy-decision-fields-rejected): old freezes
        without the ADJ-20260807-0003 decision fields are rejected for
        every mode (DryRun included)."""
        freeze = make_freeze(self.ws)
        del freeze['settings_revoke_mechanism']
        freeze_path = self.write_freeze(freeze, 'freeze-legacy-decisions.json')
        ev, raw = self.case_paths('legacy-decisions')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'settings_revoke_mechanism', 'missing decision field')
        self.assertFalse(os.path.exists(ev), 'rejected freeze created an evidence root')

    def test_freeze_decision_fields_illegal(self):
        """PS L2500-2550: a decision field with an illegal value is
        rejected with the explicit value gate message."""
        freeze = make_freeze(self.ws, settings_revoke_mechanism='some-other-mechanism')
        freeze_path = self.write_freeze(freeze, 'freeze-bad-decision.json')
        ev, raw = self.case_paths('bad-decision')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'settings_revoke_mechanism must be settings-app-info-force-stop',
                             'illegal decision field')

    def test_freeze_legacy_spacing_rejected(self):
        """PS L2550-2600 (legacy-spacing-field-rejected): the legacy
        `spacing` field is refused outright, never compatibly reused."""
        freeze = make_freeze(self.ws)
        del freeze['process_absent_probe_spacing_seconds']
        freeze['spacing'] = 3
        freeze_path = self.write_freeze(freeze, 'freeze-legacy-spacing.json')
        ev, raw = self.case_paths('legacy-spacing')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'legacy spacing field', 'legacy spacing')
        self.assertFalse(os.path.exists(ev), 'legacy spacing freeze created an evidence root')

    def test_freeze_runner_binding_rejected(self):
        """PS L690 (runner_sha256 gate) + L2650-2700 (plan-code-artifact
        negatives): a freeze bound to a different runner byte set is
        rejected - the historical runner binding never matches."""
        freeze = make_freeze(self.ws, runner_sha256='0' * 64)
        freeze_path = self.write_freeze(freeze, 'freeze-bad-runner.json')
        ev, raw = self.case_paths('bad-runner')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'runner SHA-256 mismatch', 'runner binding')
        self.assertFalse(os.path.exists(ev), 'bad runner binding created an evidence root')

    def test_runner_bytes_frozen(self):
        """PS L690: the freeze binds the runner file bytes; the committed
        copy must be byte-identical to the source runner (frozen-bytes
        requirement - the runner .py is never modified by the selftest)."""
        self.assertEqual(sha256_file(RUNNER_SRC), self.ws['runner_sha256'],
                         'runner source bytes differ from the committed selftest copy')


# =====================================================================
# Group B: TargetBindingConfirm (PS L1006-1205 producer + L460-700
# ready-freeze-binding consumer)
# =====================================================================


class TestTargetBindingConfirm(SelftestBase):
    """TargetBindingConfirm producer path: pass with a fake hdc, pre-record
    gate exit 1 with no record, probe/tuple blocked exit 2 with a
    best-effort blocked record, double-file companion, and record
    round-trip consumption by a ready DryRun freeze."""

    def _confirm_args(self, freeze_path, record_path, hdc_path):
        return ['--FreezeManifest', freeze_path,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', hdc_path,
                '--TargetBindingConfirm', '--ConfirmationRecord', record_path]

    def test_confirm_pass_path(self):
        """PS L1115-1205 (Invoke-TargetBindingConfirm): the three frozen
        probes (Version/TupleModel/TupleBuild) run against the fake hdc,
        the mechanical pass gate (attempted=completed=3, 3 HDC processes)
        is asserted before the record is written, and the run exits 0 with
        the confirm RUNNER_RESULT line."""
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['fake_hdc'])
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-pass.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-pass.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, self.ws['fake_hdc']),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 0, 'confirm pass failed:\n%s\n%s' % (proc.stdout, proc.stderr))
        result, _ = parse_runner_result(proc.stdout)
        self.assertEqual(result, 'RUNNER_RESULT=pass MODE=target-binding-confirm '
                                 'RECORD_KIND=target-binding-confirmation IS_EVIDENCE=false '
                                 'COMMAND_ATTEMPTED=3 COMMAND_COMPLETED=3 RECORD=%s RECORD_SHA256=%s' % (
                                     record_path, sha256_file(record_path)), str(result))
        self.assertTrue(os.path.isfile(record_path), 'pass record missing')
        self.assertTrue(os.path.isfile(record_path + '.sha256'), 'pass companion missing')
        record = json.loads(open(record_path, encoding='utf-8').read())
        self.assertEqual(record['record_kind'], 'target-binding-confirmation')
        self.assertIs(record['is_evidence'], False)
        self.assertEqual(record['verdict'], 'pass')
        self.assertEqual(record['command_attempted'], 3)
        self.assertEqual(record['command_completed'], 3)
        self.assertEqual(record['command_count'], 3)
        self.assertIs(record['target_redacted'], True)
        self.assertEqual(record['authorization_id'], AUTH_ID)
        self.assertEqual(record['campaign_id'], CANDIDATE_CAMPAIGN_ID)
        self.assertEqual(record['evidence_id'], CANDIDATE_EVIDENCE_ID)
        self.assertEqual(record['hdc_version'], HDC_VERSION)
        self.assertEqual(record['observed_model'], MODEL)
        self.assertEqual(record['observed_build'], BUILD)
        self.assertNotIn(TARGET, json.dumps(record), 'real target leaked into the record')

    def test_confirm_pre_record_gate_exit1(self):
        """PS L415-420 (in-repo ConfirmationRecord): a record path inside
        the git repository is a pre-record gate failure - exit 1, no record
        written, explicit out-of-repo message."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-in-repo.json')
        record_path = os.path.join(self.ws['repo'], 'in-repo-confirmation.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, self.ws['sentinel_hdc']),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 1, 'in-repo record gate exit code: %d\n%s' % (
            proc.returncode, proc.stdout + proc.stderr))
        self.assertRegex(proc.stderr, r'outside the git repository', 'in-repo rejection message missing')
        self.assertFalse(os.path.exists(record_path), 'in-repo record was written despite rejection')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']), 'pre-record gate launched the HDC sentinel')

    def test_confirm_probe_blocked_exit2(self):
        """PS L1115-1205: a probe failure (ENOEXEC sentinel - any exec
        attempt raises Exec format error) produces a best-effort blocked
        record + companion and exits 2; no campaign roots are created."""
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['enoexec_hdc'])
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-probe-blocked.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-probe-blocked.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, self.ws['enoexec_hdc']),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 2, 'probe blocked exit code: %d\n%s' % (
            proc.returncode, proc.stdout + proc.stderr))
        result, _ = parse_runner_result(proc.stdout)
        self.assertIsNotNone(result, 'confirm blocked run missing RUNNER_RESULT line')
        self.assertIn('RUNNER_RESULT=blocked', result or '')
        self.assertTrue(os.path.isfile(record_path), 'blocked record missing')
        self.assertTrue(os.path.isfile(record_path + '.sha256'), 'blocked companion missing')
        record = json.loads(open(record_path, encoding='utf-8').read())
        self.assertEqual(record['verdict'], 'blocked')
        self.assertEqual(record['command_attempted'], 1)
        self.assertEqual(record['command_completed'], 0)
        self.assertNotEqual(record['reason'], 'N/A', 'blocked record must carry a reason')

    def test_confirm_drift_blocked(self):
        """PS L1115-1205: a tuple drift (fake hdc returns a wrong model)
        is a preflight failure - blocked record + exit 2."""
        drift_hdc = make_fake_hdc(os.path.join(self.ws['tmp'], 'fake-hdc-drift.sh'),
                                  model='WRONG-MODEL')
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=drift_hdc)
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-drift.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-drift.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, drift_hdc),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 2, 'drift blocked exit code: %d\n%s' % (
            proc.returncode, proc.stdout + proc.stderr))
        record = json.loads(open(record_path, encoding='utf-8').read())
        self.assertEqual(record['verdict'], 'blocked')
        self.assertRegex(record['reason'], r'frozen device model mismatch', 'drift reason missing')

    def test_confirm_double_file_companion(self):
        """PS L1077-1114 (Write-TargetBindingConfirmationRecordPair): the
        .sha256 companion is the completion marker and must equal the
        record bytes hash; the record JSON must round-trip."""
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['fake_hdc'])
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-companion.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-companion.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, self.ws['fake_hdc']),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 0, 'companion case failed:\n%s' % proc.stdout)
        self.assertEqual(open(record_path + '.sha256', encoding='utf-8').read().strip(),
                         sha256_file(record_path), 'companion does not match record bytes')
        record = json.loads(open(record_path, encoding='utf-8').read())
        self.assertEqual(json.loads(json.dumps(record)), record, 'record JSON round-trip failed')

    def test_confirm_record_roundtrip(self):
        """PS L460-700 (ready-freeze-binding): a record produced by the
        confirm pass path is consumable by a ready DryRun freeze bound with
        machine_fresh_confirmation + independent_review_record - the
        producer/consumer round trip closes."""
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['fake_hdc'])
        freeze_path = self.write_freeze(freeze, 'freeze-confirm-roundtrip.json')
        record_path = os.path.join(self.ws['tmp'], 'confirm-roundtrip.json')
        proc = run_runner(self.ws, self._confirm_args(freeze_path, record_path, self.ws['fake_hdc']),
                          env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 0, 'roundtrip producer failed:\n%s' % proc.stdout)
        record_sha = sha256_file(record_path)
        review_path, review_sha = make_review_record(self.ws, freeze, record_sha,
                                                      os.path.join(self.ws['tmp'], 'review-roundtrip.json'))
        ready = bind_ready_freeze(freeze, record_path, record_sha, review_path, review_sha)
        ready_path = self.write_freeze(ready, 'freeze-ready-roundtrip.json')
        ev, raw = self.case_paths('roundtrip-ready-dryrun')
        proc2 = run_runner(self.ws, ['--FreezeManifest', ready_path, '--EvidenceRoot', ev,
                                     '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['fake_hdc'],
                                     '--DryRun'])
        self.assertEqual(proc2.returncode, 0, 'roundtrip consumer rejected the produced record:\n%s' % (
            proc2.stdout + proc2.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertEqual(record['machine_fresh_confirmation']['status'], 'pass')
        self.assertEqual(record['machine_fresh_confirmation']['record_sha256'], record_sha)
        self.assertEqual(record['independent_review_record']['status'], 'pass')
        self.assertEqual(record['independent_review_record']['record_sha256'], review_sha)
        self.assertNotIn('record_path', record['machine_fresh_confirmation'],
                         'sealed projection leaked the record path')
        self.assertNotIn('record_path', record['independent_review_record'],
                         'sealed projection leaked the review path')


# =====================================================================
# Group C: HDC whitelist (PS L1277-1358 Assert-ExactCommandParameters /
# Get-HdcInvocation / Get-LiveHdcArguments)
# =====================================================================


class TestHdcWhitelist(SelftestBase):
    """HDC whitelist pure-function checks against the runner module
    (mirrors the embedded selftest's whitelist section, PS L1277-1358)."""

    def test_hdc_whitelist_22_operations(self):
        """PS L1277-1297: the whitelist table holds exactly 22 operations
        and the alias map covers them all."""
        self.assertEqual(len(runner_mod.HDC_WHITELIST), 22)
        self.assertEqual(set(runner_mod.HDC_OPERATION_ALIASES.values()), set(runner_mod.HDC_WHITELIST))

    def test_hdc_all_operations_accepted(self):
        """PS L1277-1297: every whitelisted operation with its exact
        required parameters is accepted."""
        params = {
            'Version': {}, 'TupleModel': {}, 'TupleBuild': {}, 'MkdirStaging': {},
            'RemoveStaging': {}, 'StagingProbe': {}, 'SendA': {}, 'SendB': {},
            'InstallA': {}, 'InstallB': {}, 'FaultA': {}, 'FaultB': {}, 'HilogStream': {},
            'BundleDump': {'Bundle': BUNDLE_A}, 'PidOf': {'Bundle': BUNDLE_A},
            'Uninstall': {'Bundle': BUNDLE_A}, 'StartEntry': {'Bundle': BUNDLE_A},
            'ScreenCap': {'Name': 'scenario-1-baseline'},
            'DumpLayout': {'Name': 'scenario-1-baseline'},
            'ReceiveScreen': {'Name': 'scenario-1-baseline'},
            'ReceiveLayout': {'Name': 'scenario-1-baseline'},
            'ForceStop': {'Bundle': BUNDLE_A, 'Reason': 'final-cleanup'},
        }
        for operation, required in runner_mod.HDC_WHITELIST.items():
            with self.subTest(operation=operation):
                runner_mod.assert_exact_command_parameters(operation, params[operation])
                argv = runner_mod.get_hdc_invocation(operation, params[operation])
                self.assertIsInstance(argv, list)
                self.assertTrue(len(argv) > 0)

    def test_hdc_extra_argv_rejected(self):
        """PS L1277-1297: an extra parameter on a no-parameter operation
        and a missing required parameter are both rejected."""
        with self.assertRaises(RuntimeError):
            runner_mod.assert_exact_command_parameters('StagingProbe', {'Path': '/tmp/x'})
        with self.assertRaises(RuntimeError):
            runner_mod.assert_exact_command_parameters('BundleDump', {})
        with self.assertRaises(RuntimeError):
            runner_mod.assert_exact_command_parameters('NotAllowed', {})
        with self.assertRaises(RuntimeError):
            runner_mod.get_hdc_invocation('BundleDump', {'Bundle': 'evil-bundle'})

    def test_hdc_case_insensitive(self):
        """PS L1277-1297 (MINOR-2): operation and parameter names are
        case-insensitive; any spelling routes to the canonical name."""
        runner_mod.assert_exact_command_parameters('bundledump', {'bundle': BUNDLE_A})
        argv = runner_mod.get_hdc_invocation('BUNDLEDUMP', {'BUNDLE': BUNDLE_A})
        self.assertIn('bm', argv)
        self.assertIn('dump', argv)

    def test_hdc_placeholder_substitution(self):
        """PS L1342-1358 (Get-LiveHdcArguments): the audit argv always keeps
        the placeholders and never the real target; the live argv substitutes
        the real target and never keeps a placeholder."""
        saved = getattr(runner_mod, 'actual_target')
        setattr(runner_mod, 'actual_target', 'usb-target:8710')
        try:
            audit = runner_mod.get_hdc_invocation('BundleDump', {'Bundle': BUNDLE_A})
            live = runner_mod.get_live_hdc_arguments(audit, 'BundleDump', {'Bundle': BUNDLE_A})
            self.assertIn('<PHYS_1_TARGET>', audit)
            self.assertNotIn('usb-target:8710', audit)
            self.assertIn('usb-target:8710', live)
            self.assertNotIn('<PHYS_1_TARGET>', live)
        finally:
            setattr(runner_mod, 'actual_target', saved)

    def test_physical_target_token_rejects_flag_shapes(self):
        """PS L1265-1269 (M3): a flag-shaped PHYS_1_TARGET token (leading
        '-', e.g. '-s' or '--list') is rejected - flag-injection defense;
        a real token is still accepted."""
        self.assertFalse(runner_mod.test_physical_target_token('-s'))
        self.assertFalse(runner_mod.test_physical_target_token('--list'))
        self.assertTrue(runner_mod.test_physical_target_token('usb-target:8710'))


# =====================================================================
# Group D: DryRun (PS L340-360 blocked-plan-dry-run + L3695-3710
# Invoke-DryRunCampaign)
# =====================================================================


class TestDryRun(SelftestBase):
    """DryRun campaign: non-evidence blocked record, zero HDC processes,
    empty integrity, the 20 planned operations, and both blocked and ready
    plan_status accepted."""

    def test_dryrun_blocked_freeze(self):
        """PS L340-360 (blocked-plan-dry-run): a blocked freeze DryRun exits
        0 with is_evidence=false, record_status=blocked, HDC_PROCESSES=0,
        empty integrity violations, and sealed outputs."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-dry.json')
        ev, raw = self.case_paths('dry')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assertEqual(proc.returncode, 0, 'dry-run failed:\n%s' % proc.stdout)
        result, _ = parse_runner_result(proc.stdout)
        self.assertIsNotNone(result, 'dry-run missing RUNNER_RESULT line')
        self.assertIn('RUNNER_RESULT=blocked', result or '')
        self.assertIn('HDC_PROCESSES=0', result or '')
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertEqual(record['plan_status'], 'blocked')
        self.assertIs(record['is_evidence'], False)
        self.assertEqual(record['record_status'], 'blocked')
        self.assertEqual(record['hdc_processes_started'], 0)
        self.assertEqual(record['hdc_logical_calls'], 20, 'dry-run did not run the 20 planned operations')
        self.assertEqual(record['integrity_violations'], [], 'normal dry-run produced false integrity violations')
        self.assertIsNone(record['scenario_aggregation']['s3_clean_reactivation_proof'],
                          'aggregation masqueraded not-probed as false')
        self.assertEqual(verify_transcript_chain(os.path.join(ev, 'projection', 'transcript.redacted.jsonl')), [])
        self.assertEqual(assert_evidence_outputs(ev), [])
        self.assertFalse(os.path.exists(self.ws['hdc_marker']), 'dry-run launched the HDC sentinel')

    def test_dryrun_ready_freeze(self):
        """PS L460-700 (ready-freeze-binding): a ready DryRun freeze with a
        bound machine confirmation + review record is accepted and exits 0
        with the same non-evidence contract."""
        freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['fake_hdc'])
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, freeze, os.path.join(self.ws['tmp'], 'confirm-ready-dry.json'))
        review_path, review_sha = make_review_record(
            self.ws, freeze, confirm_sha, os.path.join(self.ws['tmp'], 'review-ready-dry.json'))
        ready = bind_ready_freeze(freeze, confirm_path, confirm_sha, review_path, review_sha)
        ready_path = self.write_freeze(ready, 'freeze-ready-dry.json')
        ev, raw = self.case_paths('ready-dry')
        proc = run_runner(self.ws, ['--FreezeManifest', ready_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['fake_hdc'],
                                    '--DryRun'])
        self.assertEqual(proc.returncode, 0, 'ready dry-run failed:\n%s' % (proc.stdout + proc.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertIs(record['is_evidence'], False)
        self.assertEqual(record['record_status'], 'blocked')
        self.assertEqual(record['hdc_processes_started'], 0)
        self.assertEqual(record['machine_fresh_confirmation']['status'], 'pass')
        self.assertEqual(record['independent_review_record']['status'], 'pass')

    def test_dryrun_20_operations_order(self):
        """PS L3695-3710 (Invoke-DryRunCampaign): the transcript records
        exactly the 20 planned operations in the fixed order."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-dry-order.json')
        ev, raw = self.case_paths('dry-order')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assertEqual(proc.returncode, 0, 'dry-run order case failed:\n%s' % proc.stdout)
        operations = []
        with open(os.path.join(ev, 'projection', 'transcript.redacted.jsonl'), encoding='utf-8') as f:
            for line in f:
                doc = json.loads(line)
                if doc['payload']['kind'] == 'hdc-command':
                    operations.append(doc['payload']['data']['operation'])
        expected = [op for op, _ in runner_mod.DRY_RUN_PLAN]
        self.assertEqual(operations, expected, 'dry-run operation order mismatch')

    def test_dryrun_sentinel_never_started(self):
        """PS L340-360 + design document section 4.2: the HDC-MUST-NOT-START
        sentinel is never executed across the whole DryRun path - the marker
        file never appears and HDC_PROCESSES=0."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-dry-sentinel.json')
        ev, raw = self.case_paths('dry-sentinel')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assertEqual(proc.returncode, 0, 'dry-run sentinel case failed:\n%s' % proc.stdout)
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'HDC sentinel was executed during DryRun')
        self.assertIn('HDC_PROCESSES=0', proc.stdout)


# =====================================================================
# Group E: --SelfTest and argparse (PS L4697-5918 Invoke-RunnerSelfTest +
# L5920-5922 Assert-ModeExclusivity + L3-17 param)
# =====================================================================


class TestSelfTestAndArgparse(SelftestBase):
    """Embedded pure-function selftest early exit and CLI gate semantics."""

    def test_selftest_exit0(self):
        """PS L4697-5918 (Invoke-RunnerSelfTest): --SelfTest exits 0 and
        prints SELFTEST_RESULT=pass HDC_PROCESSES=<count> (PS L5698 format,
        lowercase pass + read-only pgrep count); the sentinel is never
        executed (the Python runner's embedded selftest is host-only, so
        HDC_PROCESSES=0 is asserted via the marker file instead of the
        output line)."""
        proc = run_runner(self.ws, ['--SelfTest'])
        self.assertEqual(proc.returncode, 0, 'embedded selftest failed:\n%s' % proc.stdout)
        self.assertRegex(proc.stdout, r'SELFTEST_RESULT=pass HDC_PROCESSES=\d+',
                         'selftest result line missing or wrong format')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']), 'selftest launched the HDC sentinel')

    def test_selftest_mutual_exclusion(self):
        """PS L5920-5922: mode exclusivity runs BEFORE the SelfTest early
        exit, so TargetBindingConfirm+SelfTest is rejected even with
        -SelfTest."""
        proc = run_runner(self.ws, ['--SelfTest', '--TargetBindingConfirm',
                                    '--ConfirmationRecord', os.path.join(self.ws['tmp'], 'x.json')])
        self.assertEqual(proc.returncode, 1, 'mutual exclusion exit code: %d' % proc.returncode)
        self.assertRegex(proc.stdout + proc.stderr, r'mutually exclusive', 'exclusivity message missing')

    def test_dryrun_livesim_exclusive(self):
        """PS L738-752: DryRun and LiveSimulation are mutually exclusive."""
        proc = run_runner(self.ws, ['--DryRun', '--LiveSimulation',
                                    '--SimulationFixture', os.path.join(self.ws['tmp'], 'x.json')])
        self.assertEqual(proc.returncode, 1, 'dryrun/livesim exclusivity exit code: %d' % proc.returncode)
        self.assertRegex(proc.stdout + proc.stderr, r'mutually exclusive', 'exclusivity message missing')

    def test_argparse_unknown(self):
        """PS L3-17 (param): an unknown argument is an argparse error that
        maps to the pre-record gate exit code 1 (MINOR-4)."""
        proc = run_runner(self.ws, ['--NotARealArgument'])
        self.assertEqual(proc.returncode, 1, 'argparse error exit code: %d' % proc.returncode)
        self.assertNotEqual(proc.returncode, 2, 'argparse error must not map to exit 2')

    def test_argparse_missing_required(self):
        """PS L5924-5932: required parameters are validated after the
        SelfTest early exit; a missing FreezeManifest exits 1."""
        proc = run_runner(self.ws, ['--EvidenceRoot', os.path.join(self.ws['tmp'], 'ev')])
        self.assertEqual(proc.returncode, 1, 'missing required exit code: %d' % proc.returncode)
        self.assertRegex(proc.stderr, r'FreezeManifest, HapA, HapB, and HdcPath are required',
                         'missing-required message missing')


# =====================================================================
# Group E2: source-level audit (PS M1-M3 equivalent). The runner source
# must never shell out, never dynamically execute, never embed host-prep
# hdc commands, and must carry the frozen whitelist plus the
# classification / exit-code / stable-contract markers.
# =====================================================================


class TestSourceAudit(SelftestBase):
    """Source-level audit of the runner (PS M1-M3 equivalent): AST walk of
    the runner .py plus literal source markers. Host-only - no subprocess
    runner spawn, no workspace needed."""

    def test_ast_no_shell_or_dynamic_execution(self):
        """AST audit: no subprocess shell=True, no os.system/os.popen, no
        eval/exec dynamic execution anywhere in the runner source."""
        source = open(RUNNER_SRC, encoding='utf-8').read()
        tree = ast.parse(source)
        violations = []
        for node in ast.walk(tree):
            if not isinstance(node, ast.Call):
                continue
            func = node.func
            if isinstance(func, ast.Attribute):
                if func.attr in ('system', 'popen'):
                    violations.append('os.%s call at line %d' % (func.attr, node.lineno))
                if func.attr in ('run', 'Popen') and isinstance(func.value, ast.Name) \
                        and func.value.id == 'subprocess':
                    for kw in node.keywords:
                        if kw.arg == 'shell' and isinstance(kw.value, ast.Constant) \
                                and kw.value.value is True:
                            violations.append('subprocess.%s shell=True at line %d' % (func.attr, node.lineno))
            if isinstance(func, ast.Name) and func.id in ('eval', 'exec'):
                violations.append('%s() at line %d' % (func.id, node.lineno))
        self.assertEqual(violations, [],
                         'runner source must never shell out or execute dynamically: %s' % violations)

    def test_no_host_prep_hdc_commands_in_source(self):
        """host-prep steps must not be compiled into the runner: the
        'hdc tconn' and 'list targets' command strings never appear in the
        runner source."""
        source = open(RUNNER_SRC, encoding='utf-8').read()
        for forbidden in ('hdc tconn', 'list targets'):
            self.assertNotIn(forbidden, source,
                             'host-prep command %r must not be embedded in the runner' % forbidden)

    def test_hdc_whitelist_constant_in_source(self):
        """the HDC_WHITELIST dict literal in the runner source carries the
        three frozen probes (Version/TupleModel/TupleBuild) and the
        complete 22-operation list."""
        source = open(RUNNER_SRC, encoding='utf-8').read()
        tree = ast.parse(source)
        whitelist = None
        for node in ast.walk(tree):
            if isinstance(node, ast.Assign) and any(
                    isinstance(t, ast.Name) and t.id == 'HDC_WHITELIST' for t in node.targets):
                whitelist = node.value
                break
        self.assertIsNotNone(whitelist, 'HDC_WHITELIST assignment not found in runner source')
        if not isinstance(whitelist, ast.Dict):
            self.fail('HDC_WHITELIST must be a dict literal')
        keys = [key.value for key in whitelist.keys if isinstance(key, ast.Constant)]
        self.assertEqual(len(keys), 22, 'HDC_WHITELIST must hold exactly 22 operations')
        for probe in ('Version', 'TupleModel', 'TupleBuild'):
            self.assertIn(probe, keys, 'frozen probe %s missing from HDC_WHITELIST' % probe)

    def test_source_classification_markers(self):
        """PS M1-M3 equivalent: the runner source carries the FAIL_REASON
        classification texts (per the runner's actual wording), the
        exit-code semantics comment, and the confirmation_contract stable
        projection comment."""
        source = open(RUNNER_SRC, encoding='utf-8').read()
        for marker in ('scenario invalid', 'integrity invalid', 'repository-drift',
                       'model/build precheck drifted'):
            self.assertIn(marker, source,
                          'classification marker %r missing from runner source' % marker)
        self.assertIn('Exit codes: 0=pass', source,
                      'exit-code semantics comment missing from runner source')
        self.assertIn('stable two-phase projection', source,
                      'confirmation_contract stable projection comment missing')


# =====================================================================
# Group E3: consumer mutant matrix (PS L460-700). Every field mutation of
# a valid confirmation record must be rejected by the consumer with a
# clear mismatch hint.
# =====================================================================


class TestConsumerMutantMatrix(SelftestBase):
    """Confirmation consumer mutant matrix: unknown top-level field, wrong
    field type, wrong companion bytes, started>ended, wrong schema_version,
    and the old authorization_id are all rejected with a clear mismatch
    hint; no evidence root is ever created."""

    def _dryrun_args(self, freeze_path, ev, raw):
        return ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'], '--DryRun']

    def test_consumer_mutant_matrix(self):
        """PS L460-700: the consumer rejects every field mutation of a
        valid record. Field mutations re-write the companion and re-bind
        the freeze so the field-level gates fire (not just the companion
        gate); the companion-bytes mutation is rejected at the companion
        gate itself."""
        blocked = make_freeze(self.ws, plan_status='blocked')
        ready = make_freeze(self.ws, plan_status='ready')
        mutations = [
            ('unknown-top-level-field', {'evil_field': 'x'},
             r'unknown top-level field'),
            ('field-type-wrong', {'verdict': 123},
             r'verdict must be pass'),
            ('started-after-ended', {'started_at': '2098-12-31T23:59:59+00:00',
                                     'ended_at': '2098-12-31T23:59:55+00:00'},
             r'started_at must not be after ended_at'),
            ('schema-version-wrong', {'schema_version': 2},
             r'schema_version must be 1'),
            ('old-authorization-id', {'authorization_id': OLD_AUTH_ID},
             r'authorization_id mismatch'),
        ]
        for label, overrides, expected in mutations:
            with self.subTest(mutation=label):
                confirm_path, _ = make_confirmation_record(
                    self.ws, blocked, os.path.join(self.ws['tmp'], 'mutant-%s-confirmation.json' % label))
                tampered = json.loads(open(confirm_path, encoding='utf-8').read())
                tampered.update(overrides)
                write_json(confirm_path, tampered)
                new_sha = sha256_file(confirm_path)
                write_text(confirm_path + '.sha256', new_sha + '\n')
                review_path, review_sha = make_review_record(
                    self.ws, blocked, new_sha, os.path.join(self.ws['tmp'], 'mutant-%s-review.json' % label))
                bound = bind_ready_freeze(ready, confirm_path, new_sha, review_path, review_sha)
                bound_path = self.write_freeze(bound, 'freeze-mutant-%s.json' % label)
                ev, raw = self.case_paths('mutant-%s' % label)
                proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
                self.assert_rejected(proc, expected, 'mutant %s' % label)
                self.assertFalse(os.path.exists(ev), 'mutant %s created an evidence root' % label)
        # Companion bytes wrong: the companion no longer matches the record.
        confirm_path, _ = make_confirmation_record(
            self.ws, blocked, os.path.join(self.ws['tmp'], 'mutant-companion-confirmation.json'))
        write_text(confirm_path + '.sha256', '0' * 64 + '\n')
        review_path, review_sha = make_review_record(
            self.ws, blocked, sha256_file(confirm_path),
            os.path.join(self.ws['tmp'], 'mutant-companion-review.json'))
        bound = bind_ready_freeze(ready, confirm_path, sha256_file(confirm_path),
                                  review_path, review_sha)
        bound_path = self.write_freeze(bound, 'freeze-mutant-companion.json')
        ev, raw = self.case_paths('mutant-companion')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'companion does not match the record bytes',
                             'mutant companion bytes')
        self.assertFalse(os.path.exists(ev), 'mutant companion created an evidence root')


# =====================================================================
# Group E4: single-dash CLI forms (PS L3-17 param). All 14 parameters
# accept their single-dash spelling; the fixture runs a real DryRun and a
# real TargetBindingConfirm with single-dash forms.
# =====================================================================


class TestSingleDashCli(SelftestBase):
    """Single-dash CLI forms: -SelfTest and every one of the 14 parameters'
    single-dash spellings parse and run under -DryRun / -TargetBindingConfirm
    combinations (real DryRun + real TargetBindingConfirm runs)."""

    def test_single_dash_cli_forms(self):
        """PS L3-17 (param): -SelfTest runs; a real DryRun and a real
        TargetBindingConfirm accept every DryRun-/confirm-compatible
        single-dash form; the mutually exclusive -LiveSimulation single-dash
        form is verified at the parse level."""
        # 1. -SelfTest single-dash real run.
        proc = run_runner(self.ws, ['-SelfTest'])
        self.assertEqual(proc.returncode, 0, 'single-dash SelfTest failed:\n%s' % proc.stdout)
        self.assertRegex(proc.stdout, r'SELFTEST_RESULT=pass HDC_PROCESSES=\d+')
        # 2. Real DryRun with every DryRun-compatible single-dash form.
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-single-dash-dryrun.json')
        ev, raw = self.case_paths('single-dash-dryrun')
        fixture = os.path.join(self.ws['tmp'], 'single-dash-sim.json')
        write_json(fixture, {'hdc_version': HDC_VERSION, 'scenario_events': {}})
        proc = run_runner(self.ws, [
            '-FreezeManifest', freeze_path, '-EvidenceRoot', ev, '-RawRoot', raw,
            '-HapA', self.ws['hap_a'], '-HapB', self.ws['hap_b'],
            '-HdcPath', self.ws['sentinel_hdc'],
            '-HdcTimeoutSeconds', '20', '-OperatorTimeoutSeconds', '300',
            '-DryRun', '-SimulationFixture', fixture])
        self.assertEqual(proc.returncode, 0,
                         'single-dash DryRun failed:\n%s' % (proc.stdout + proc.stderr))
        # 3. Real TargetBindingConfirm with single-dash forms.
        confirm_freeze = make_freeze(self.ws, plan_status='blocked', hdc_path=self.ws['fake_hdc'])
        confirm_path = self.write_freeze(confirm_freeze, 'freeze-single-dash-confirm.json')
        confirm_record = os.path.join(self.ws['tmp'], 'single-dash-confirm-record.json')
        proc = run_runner(self.ws, [
            '-FreezeManifest', confirm_path,
            '-HapA', self.ws['hap_a'], '-HapB', self.ws['hap_b'],
            '-HdcPath', self.ws['fake_hdc'],
            '-HdcTimeoutSeconds', '20', '-OperatorTimeoutSeconds', '300',
            '-TargetBindingConfirm', '-ConfirmationRecord', confirm_record],
            env={'PHYS_1_TARGET': TARGET})
        self.assertEqual(proc.returncode, 0,
                         'single-dash TargetBindingConfirm failed:\n%s' % (proc.stdout + proc.stderr))
        # 4. -LiveSimulation single-dash parses (mutually exclusive with
        # DryRun/TargetBindingConfirm, so parse-level only).
        args = runner_mod.parse_args(['-LiveSimulation', '-SimulationFixture', 'sim.json'])
        self.assertTrue(args.live_simulation)
        self.assertEqual(args.simulation_fixture, 'sim.json')


# =====================================================================
# Group E5: numeric equivalence (MAJOR-1). The freeze numeric gates keep
# PS double/int semantics: spacing accepts int 3 and float 3.0 losslessly,
# rejects the string '3' at the type gate, and rejects 2.9 at the value
# gate (the double is parsed losslessly, so 2.9 never passes the 3-second
# gate); count accepts the integral float 2.0 and rejects 2.9.
# =====================================================================


class TestNumericEquivalence(SelftestBase):
    """Numeric equivalence: spacing/count freeze gates under DryRun."""

    def test_numeric_equivalence(self):
        """MAJOR-1: spacing=3 (int) and spacing=3.0 accepted; spacing='3'
        rejected at the type gate; spacing=2.9 rejected at the value gate
        (double semantics lossless - 2.9 is never truncated/rounded to 3);
        count=2.0 accepted; count=2.9 rejected."""
        cases = [
            ('spacing-int-3', {'process_absent_probe_spacing_seconds': 3}, True, None),
            ('spacing-float-3', {'process_absent_probe_spacing_seconds': 3.0}, True, None),
            ('spacing-string-3', {'process_absent_probe_spacing_seconds': '3'}, False,
             r'must be a JSON double'),
            ('spacing-2.9', {'process_absent_probe_spacing_seconds': 2.9}, False,
             r'must be 3 seconds'),
            ('count-float-2', {'process_absent_required_count': 2.0}, True, None),
            ('count-2.9', {'process_absent_required_count': 2.9}, False,
             r'must be a JSON integer'),
        ]
        for label, overrides, accepted, expected in cases:
            with self.subTest(case=label):
                freeze = make_freeze(self.ws, plan_status='blocked', **overrides)
                freeze_path = self.write_freeze(freeze, 'freeze-numeric-%s.json' % label)
                ev, raw = self.case_paths('numeric-%s' % label)
                proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                            '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                            '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                            '--DryRun'])
                if accepted:
                    self.assertEqual(proc.returncode, 0,
                                     'numeric case %s rejected:\n%s' % (label, proc.stdout + proc.stderr))
                else:
                    self.assert_rejected(proc, expected, 'numeric case %s' % label)


# =====================================================================
# Group F: two-phase confirmation contract (PS L763-860, ADJ-20260810-0001
# C6). The blocked confirmation freeze freezes at T1 BEFORE the machine
# confirmation runs; the final ready freeze freezes at T2 AFTER the
# confirmation/review end times. The full freeze contract hash differs
# (preflight_inputs_frozen_at advanced) but the stable confirmation contract
# is byte-identical, so the records bound on the blocked phase are
# consumable by the ready phase; the time gate
# (started<=ended<=preflight_inputs_frozen_at) is checked against the final
# ready freeze.
# =====================================================================


class TestTwoPhaseConfirmationContract(SelftestBase):
    """Two-phase confirmation contract (PS L763-860): same-side mirror hash
    checks, blocked-T1 records consumed by the ready-T2 DryRun, stable
    contract core mutation rejected, record contract binding, and the
    frozen_at time gate."""

    def _two_phase_freezes(self):
        blocked = make_freeze(self.ws, plan_status='blocked',
                              preflight_inputs_frozen_at='2099-01-01T00:00:00+00:00')
        ready = make_freeze(self.ws, plan_status='ready',
                            preflight_inputs_frozen_at='2099-01-01T00:00:10+00:00')
        return blocked, ready

    def _bound_ready_path(self, blocked, ready, confirm_name, review_name, freeze_name):
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, blocked, os.path.join(self.ws['tmp'], confirm_name))
        review_path, review_sha = make_review_record(
            self.ws, blocked, confirm_sha, os.path.join(self.ws['tmp'], review_name))
        bound = bind_ready_freeze(ready, confirm_path, confirm_sha, review_path, review_sha)
        return self.write_freeze(bound, freeze_name)

    def _dryrun_args(self, freeze_path, ev, raw):
        return ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'], '--DryRun']

    def test_two_phase_contract_hashes(self):
        """PS L763-790: blocked T1 + ready T2 full freeze contract hashes
        differ (preflight_inputs_frozen_at advanced), confirmation contract
        hashes byte-identical (same-side mirror)."""
        blocked, ready = self._two_phase_freezes()
        self.assertNotEqual(runner_mod.get_freeze_contract_sha256(blocked),
                            runner_mod.get_freeze_contract_sha256(ready),
                            'two-phase full freeze contract hashes must differ')
        self.assertEqual(runner_mod.get_confirmation_contract_sha256(blocked),
                         runner_mod.get_confirmation_contract_sha256(ready),
                         'two-phase confirmation contract hashes must be identical')

    def test_two_phase_ready_consumes_blocked_records(self):
        """PS L790-830: records bound on the blocked T1 phase are consumable
        by the ready T2 DryRun; the sealed record projects the final full
        freeze contract and the stable confirmation contract as distinct
        SHA-256 values, and the stable contract matches the same-side mirror
        in the record and in both binding projections."""
        blocked, ready = self._two_phase_freezes()
        bound_path = self._bound_ready_path(blocked, ready,
                                            'two-phase-confirmation.json',
                                            'two-phase-review.json',
                                            'freeze-two-phase-ready.json')
        ev, raw = self.case_paths('two-phase-ready-dryrun')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assertEqual(proc.returncode, 0, 'two-phase ready DryRun failed:\n%s' % (
            proc.stdout + proc.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertRegex(record['freeze_contract_sha256'], r'^[0-9a-f]{64}$')
        self.assertRegex(record['confirmation_contract_sha256'], r'^[0-9a-f]{64}$')
        self.assertNotEqual(record['freeze_contract_sha256'], record['confirmation_contract_sha256'],
                            'sealed record full freeze contract must differ from the stable confirmation contract')
        self.assertEqual(record['confirmation_contract_sha256'],
                         runner_mod.get_confirmation_contract_sha256(ready),
                         'sealed record stable confirmation contract does not match the same-side mirror')
        self.assertEqual(record['machine_fresh_confirmation']['confirmation_contract_sha256'],
                         runner_mod.get_confirmation_contract_sha256(ready),
                         'sealed machine binding does not anchor the confirmation contract')
        self.assertEqual(record['independent_review_record']['confirmation_contract_sha256'],
                         runner_mod.get_confirmation_contract_sha256(ready),
                         'sealed review binding does not anchor the confirmation contract')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'two-phase case launched the HDC sentinel')

    def test_two_phase_operator_role_mutation_rejected(self):
        """PS L830-860: mutating a stable contract core field that passes
        the freeze static value gate (operator_role) changes the contract
        hash, so the records bound on the blocked phase are rejected by the
        ready-phase consumer."""
        blocked, ready = self._two_phase_freezes()
        bound_path = self._bound_ready_path(blocked, ready,
                                            'two-phase-mutated-confirm.json',
                                            'two-phase-mutated-review.json',
                                            'freeze-two-phase-mutated.json')
        mutated = json.loads(open(bound_path, encoding='utf-8').read())
        mutated['operator_role'] = 'some-other-operator'
        write_json(bound_path, mutated)
        ev, raw = self.case_paths('two-phase-mutated-dryrun')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'confirmation_contract_sha256',
                             'two-phase stable contract core mutation')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'two-phase mutation case launched the HDC sentinel')

    def test_two_phase_record_contract_binding(self):
        """PS L790-830 (C6): the confirmation record binds the confirmation
        contract - a record whose confirmation_contract_sha256 does not match
        the current freeze contract is rejected by the ready-phase consumer."""
        blocked, ready = self._two_phase_freezes()
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, blocked, os.path.join(self.ws['tmp'], 'two-phase-wrong-contract.json'),
            confirmation_contract_sha256='0' * 64)
        review_path, review_sha = make_review_record(
            self.ws, blocked, confirm_sha, os.path.join(self.ws['tmp'], 'two-phase-wrong-contract-review.json'))
        bound = bind_ready_freeze(ready, confirm_path, confirm_sha, review_path, review_sha)
        bound_path = self.write_freeze(bound, 'freeze-two-phase-wrong-contract.json')
        ev, raw = self.case_paths('two-phase-wrong-contract')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'confirmation_contract_sha256',
                             'confirmation record contract binding')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'wrong-contract case launched the HDC sentinel')

    def test_two_phase_frozen_at_time_gate(self):
        """PS L790-830 (C6): the time gate started<=ended<=preflight_inputs_
        frozen_at is checked against the final ready freeze - a confirmation
        record whose ended_at is after the ready freeze frozen_at is
        rejected."""
        blocked, ready = self._two_phase_freezes()
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, blocked, os.path.join(self.ws['tmp'], 'two-phase-late-confirm.json'),
            started_at='2099-01-01T00:00:11+00:00', ended_at='2099-01-01T00:00:12+00:00')
        review_path, review_sha = make_review_record(
            self.ws, blocked, confirm_sha, os.path.join(self.ws['tmp'], 'two-phase-late-review.json'))
        bound = bind_ready_freeze(ready, confirm_path, confirm_sha, review_path, review_sha)
        bound_path = self.write_freeze(bound, 'freeze-two-phase-late.json')
        ev, raw = self.case_paths('two-phase-late')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'no later than freeze preflight_inputs_frozen_at',
                             'frozen_at time gate')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'time-gate case launched the HDC sentinel')


# =====================================================================
# Group G: complete live-simulation seven scenarios (PS L863-1000). The
# base fixture drives all 7 scenarios to pass under the virtual clock with
# zero real HDC processes; the RUNNER_RESULT line carries the six-field
# seal; the S6 non-frozen rejection code path stays overall blocked.
# =====================================================================


class TestLiveSimulationSevenScenarios(SelftestBase):
    """Complete live-simulation seven-scenarios (PS L863-1000): base fixture
    all 7 scenarios pass, S2 three assertions, S4 deny, S5 force-stop flow,
    S6 conflict, S7 cleanup, transcript chain, manifest/seal, wait-state
    complete, no sensitive leak; the S6 non-frozen rejection code path stays
    overall blocked."""

    def _live_args(self, freeze_path, ev, raw, fixture_path):
        return ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'],
                '--LiveSimulation', '--SimulationFixture', fixture_path]

    def _run_live(self, fixture, name):
        fixture_path = os.path.join(self.ws['tmp'], 'simulation-%s.json' % name)
        write_json(fixture_path, fixture)
        freeze = make_freeze(self.ws, plan_status='ready')
        freeze_path = self.write_freeze(freeze, 'freeze-live-%s.json' % name)
        ev, raw = self.case_paths(name)
        proc = run_runner(self.ws, self._live_args(freeze_path, ev, raw, fixture_path))
        return proc, ev, raw

    def test_live_simulation_seven_scenarios_pass(self):
        """PS L863-1000: base fixture - all 7 scenarios pass, measured
        scenario overall pass, blocked non-evidence contract, empty
        integrity violations, exit 0, transcript chain and manifest/seal
        valid, wait-state complete, no sensitive leak into evidence while
        the raw HiLog preserves the canaries."""
        proc, ev, raw = self._run_live(make_simulation_fixture(), 'live-simulation')
        self.assertEqual(proc.returncode, 0, 'live simulation failed:\n%s\n%s' % (
            proc.stdout, proc.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertEqual(record['execution_mode'], 'live-simulation')
        self.assertIs(record['is_evidence'], False)
        self.assertEqual(record['record_status'], 'blocked')
        self.assertEqual(record['verdict'], 'blocked')
        self.assertEqual(record['scenario_aggregation']['measured_scenario_overall'], 'pass')
        self.assertEqual(record['integrity_violations'], [],
                         'normal live simulation produced false transcript integrity violations')
        self.assertEqual(len(record['scenarios']), 7)
        for scenario in record['scenarios']:
            self.assertEqual(scenario['result'], 'pass',
                             'scenario %d did not pass: %s' % (scenario['scenario'], scenario['reason']))
        self.assertEqual(verify_transcript_chain(
            os.path.join(ev, 'projection', 'transcript.redacted.jsonl')), [])
        self.assertEqual(assert_evidence_outputs(ev), [])
        wait_state = json.loads(open(os.path.join(ev, 'operator-wait-state.json'),
                                     encoding='utf-8').read())
        self.assertEqual(wait_state['phase'], 'complete')
        self.assertIs(wait_state['complete'], True)
        self.assertEqual(wait_state['schema_version'], 2)
        self.assertEqual(wait_state['execution_mode'], 'live-simulation')
        evidence_text = ''
        for root, _, files in os.walk(ev):
            for name in files:
                with open(os.path.join(root, name), encoding='utf-8', errors='replace') as f:
                    evidence_text += f.read()
        for secret in ('target-canary', '10.23.45.67', '2001:db8', 'device-canary',
                       '00:11:22:33:44:55', 'SN-CANARY', 'HDC-MUST-NOT-START'):
            self.assertNotIn(secret, evidence_text,
                             'sensitive %s leaked into projected evidence' % secret)
        raw_text = ''
        for root, _, files in os.walk(raw):
            for name in files:
                with open(os.path.join(root, name), encoding='utf-8', errors='replace') as f:
                    raw_text += f.read()
        for canary in ('target-canary.example.test:8710', '10.23.45.67:8710',
                       '2001:db8::1234', '00:11:22:33:44:55', 'SN-CANARY12345678'):
            self.assertIn(canary, raw_text, 'raw HiLog did not preserve canary %s' % canary)
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'live simulation launched the HDC sentinel')

    def test_live_simulation_runner_result_seal(self):
        """PS L863-1000: the RUNNER_RESULT line is the six-field seal -
        RUNNER_RESULT / RECORD_STATUS / MODE / EVIDENCE_ROOT / RAW_ROOT_HASH /
        HDC_PROCESSES - with zero real HDC processes and no failure line."""
        proc, ev, raw = self._run_live(make_simulation_fixture(), 'live-seal')
        self.assertEqual(proc.returncode, 0, 'live simulation failed:\n%s' % proc.stdout)
        result, failure = parse_runner_result(proc.stdout)
        self.assertIsNone(failure, 'live simulation wrote a failure line')
        self.assertEqual(result, 'RUNNER_RESULT=blocked RECORD_STATUS=blocked MODE=live-simulation '
                                 'EVIDENCE_ROOT=%s RAW_ROOT_HASH=%s HDC_PROCESSES=0' % (
                                     ev, sha256_text(raw)), str(result))
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'live simulation launched the HDC sentinel')

    def test_live_simulation_scenario_details(self):
        """PS L863-1000: S1 baseline/install window, S2 three assertions +
        authorization capture + host/device timestamps, S4 deny pre-capture
        fields, S5 force-stop flow, S6 machine conflict, S7 cleanup naming,
        S3/S7 callback terminal mode + request binding + reactivation proof."""
        proc, ev, raw = self._run_live(make_simulation_fixture(), 'live-details')
        self.assertEqual(proc.returncode, 0, 'live simulation failed:\n%s' % proc.stdout)
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        scenarios = record['scenarios']
        s1 = scenarios[0]
        self.assertIs(s1['first_baseline_query_covered'], True)
        self.assertIs(s1['install_completed_within_60_seconds'], True)
        self.assertLess(s1['observation']['measured_coverage_before_action_prompt_seconds'], 0.5,
                        'scenario 1 counted pre-action setup latency into the action window')
        self.assertLessEqual(s1['install_elapsed_seconds'], 60)
        s2 = scenarios[1]
        self.assertEqual(s2['assertions'], {'allow': 'pass', 'vpn_on_create': 'pass',
                                            'vpn_connection_create_fd': 'pass'},
                         'scenario 2 three assertions mismatch')
        self.assertEqual(s2['authorization_capture']['status'], 'collected')
        self.assertEqual(s2['authorization_capture']['result'], 'pass')
        self.assertEqual(s2['authorization_capture']['name'], 'scenario-2-authorization')
        self.assertTrue(s2['observation']['events'][0]['host_observed_at'], 'host observed time missing')
        self.assertTrue(s2['observation']['events'][0]['device_observed_at'], 'parsed device time missing')
        s3 = scenarios[2]
        self.assertEqual(s3['terminal_mode'], 'callback-post-fd')
        self.assertEqual(s3['request_id'], 'a2')
        self.assertIs(s3['clean_reactivation_proof'], True)
        self.assertEqual(s3['process_target'], '%s:vpn' % BUNDLE_A)
        s4 = scenarios[3]
        self.assertEqual(s4['reason'], 'observable-B-request-rejection')
        self.assertIs(s4['deny_screen'], True)
        self.assertEqual(s4['deny_screen_capture']['status'], 'collected')
        self.assertIs(s4['deny_screen_capture']['visible'], True)
        self.assertEqual(s4['deny_screen_capture']['result'], 'pass')
        self.assertGreaterEqual(s4['observation']['measured_coverage_after_action_seconds'], 60,
                                'deny did not measure healthy coverage through action plus 60 seconds')
        self.assertIs(s4['observation']['complete_window_observed'], True)
        self.assertIs(s4['observation']['capture_health']['measured'], True)
        self.assertIs(s4['full_window_after_action'], True)
        s4_text = '\n'.join(e['text'] for e in s4['observation']['events'])
        self.assertNotIn('2098-12-31', s4_text, 'pre-anchor history entered the deny scenario')
        self.assertNotIn('VPN_ONCREATE|bundle=%s|requestId=b4' % BUNDLE_B, s4_text,
                         'pre-anchor B history entered the deny scenario')
        s5 = scenarios[4]
        self.assertEqual(s5['settings_reallow_path']['match'], True)
        self.assertEqual(s5['settings_reallow_path']['actual'], 'direct-system-activation')
        self.assertEqual(s5['settings_reallow_path']['policy'], 'observation-only')
        self.assertEqual(s5['terminal_mode'], 'settings-app-info-force-stop')
        self.assertIs(s5['app_info_force_stop_capture']['machine_verified'], True)
        self.assertIs(s5['settings_vpn_page_observation_only'], True)
        self.assertIs(s5['bundle_present_during_probe'], True)
        self.assertGreaterEqual(len(s5['process_final_state_probes']), 2)
        self.assertGreaterEqual(s5['process_absent_evidence']['measured_spacing_seconds'], 3.0)
        self.assertGreaterEqual(s5['process_final_state_probes'][1]['spacing_seconds_since_previous'], 3.0)
        self.assertEqual(s5['process_target'], '%s:vpn' % BUNDLE_A)
        s5_text = '\n'.join(e['text'] for e in s5['observation']['events'])
        self.assertNotIn('2098-12-31', s5_text, 'pre-anchor requestId history entered scenario 5')
        s6 = scenarios[5]
        self.assertIs(s6['a_accepted'], True)
        self.assertEqual(s6['reason'], 'B-explicit-conflict-rejection')
        self.assertEqual(s6['b_rejection_code'], 2203002)
        self.assertNotIn('no_dual_active_confirmed', s6, 'scenario 6 retained semantic operator fields')
        self.assertNotIn('dual_active_confirmed', s6, 'scenario 6 retained semantic operator fields')
        self.assertNotIn('operator_state', s6, 'scenario 6 retained semantic operator fields')
        s7 = scenarios[6]
        self.assertIs(s7['post_cleanup_capture'], True)
        self.assertEqual(s7['post_cleanup_capture_name'], 'scenario-7-post-cleanup')
        self.assertEqual(s7['terminal_mode'], 'callback-post-fd')
        self.assertEqual(s7['request_id'], 'a6')
        self.assertEqual(s7['process_target'], '%s:vpn' % BUNDLE_A)
        self.assertIs(record['scenario_aggregation']['s3_clean_reactivation_proof'], True)
        self.assertEqual(record['scenario_aggregation']['scenario_2_assertions']['allow'], 'pass')

    def test_live_simulation_marker_dedup(self):
        """PS L863-1000 (simulation_scenario_steps_written): each scenario
        step's fixture events are appended to the capture stdout exactly
        once - the raw HiLog contains no duplicate event line and every
        scenario observation is duplicate-free."""
        proc, ev, raw = self._run_live(make_simulation_fixture(), 'live-dedup')
        self.assertEqual(proc.returncode, 0, 'live simulation failed:\n%s' % proc.stdout)
        log_path = os.path.join(raw, 'raw-hilog-campaign.log')
        lines = open(log_path, encoding='utf-8').read().splitlines()
        self.assertEqual(len(lines), len(set(lines)),
                         'capture stdout contains duplicate fixture event lines (step write dedup broken)')
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        for scenario in record['scenarios']:
            texts = [e['text'] for e in scenario['observation']['events']]
            self.assertEqual(len(texts), len(set(texts)),
                             'scenario %d observation contains duplicate marker lines' % scenario['scenario'])

    def test_live_simulation_s6_non_frozen_code_blocked(self):
        """PS L2419-2444 (adj-0003-s6-b-non-frozen-code-blocked): a B reject
        with a NON-frozen code (2203001, freeze only freezes 2203002) is a
        platform result - S6 blocked with B-conflict-code-not-frozen, S7
        not-run-after-platform-blocked, overall blocked, NO scenario_invalid,
        finally cleanup verified, exit 0."""
        fixture = make_simulation_fixture()
        fixture['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b6' % BUNDLE_B},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203001,name=BusinessError,message=another active vpn exists'},
        ]
        proc, ev, raw = self._run_live(fixture, 's6-non-frozen')
        self.assertEqual(proc.returncode, 0, 'S6 non-frozen B code crashed the runner:\n%s' % (
            proc.stdout + proc.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertEqual(record['overall'], 'blocked')
        self.assertEqual(record['record_status'], 'blocked')
        self.assertNotIn('scenario_invalid', record,
                         'S6 non-frozen B code was misclassified as scenario invalid')
        s6 = record['scenarios'][5]
        self.assertEqual(s6['result'], 'blocked')
        self.assertEqual(s6['reason'], 'B-conflict-code-not-frozen:2203001')
        self.assertEqual(s6['b_rejection_code'], 2203001)
        self.assertIs(s6['b_accepted'], False)
        self.assertIs(s6['a_accepted'], True)
        s7 = record['scenarios'][6]
        self.assertEqual(s7['result'], 'blocked')
        self.assertEqual(s7['reason'], 'not-run-after-platform-blocked')
        self.assertIs(record['cleanup_result']['verified_absent'], True)
        self.assertEqual(record['cleanup_result']['status'], 'verified-clean')


# =====================================================================
# Group H: adj-* negative argument gates (mutual exclusion / required /
# out-of-bounds). Every case exits 1 with the explicit gate message and
# never launches the HDC sentinel.
# =====================================================================


class TestAdjNegativeArgs(SelftestBase):
    """adj-* negative argument gates (PS L5920-5932 + L373-392): mode
    mutual exclusion, required-parameter gaps, missing FreezeManifest,
    HAP hash drift / missing file, EvidenceRoot inside the repository, and
    symlink-ancestor rejection."""

    def _dryrun_args(self, freeze_path, ev, raw):
        return ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'], '--DryRun']

    def test_confirm_dryrun_mutually_exclusive(self):
        """PS L738-752: TargetBindingConfirm + DryRun is a mode
        exclusivity failure before any campaign work."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-confirm-dryrun.json')
        record_path = os.path.join(self.ws['tmp'], 'adj-confirm-dryrun.json')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path,
                                    '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'],
                                    '--TargetBindingConfirm', '--ConfirmationRecord', record_path,
                                    '--DryRun'])
        self.assert_rejected(proc, r'mutually exclusive', 'confirm+dryrun exclusivity')
        self.assertFalse(os.path.exists(record_path), 'rejected mode wrote a record')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'exclusivity case launched the HDC sentinel')

    def test_confirmation_record_without_confirm_rejected(self):
        """PS L738-752: ConfirmationRecord is only valid with
        TargetBindingConfirm."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-record-no-confirm.json')
        ev, raw = self.case_paths('adj-record-no-confirm')
        proc = run_runner(self.ws, self._dryrun_args(freeze_path, ev, raw) +
                          ['--ConfirmationRecord', os.path.join(self.ws['tmp'], 'orphan.json')])
        self.assert_rejected(proc, r'ConfirmationRecord is only valid with TargetBindingConfirm',
                             'record without confirm mode')

    def test_missing_hap_a_rejected(self):
        """PS L5924-5932: HapA is a required parameter; a missing HapA
        exits 1 before any freeze work."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-missing-hapa.json')
        ev, raw = self.case_paths('adj-missing-hapa')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'], '--DryRun'])
        self.assert_rejected(proc, r'FreezeManifest, HapA, HapB, and HdcPath are required',
                             'missing HapA')

    def test_freeze_manifest_missing(self):
        """PS L5937-5945: a FreezeManifest path that does not exist is
        rejected with the explicit missing-file message."""
        ev, raw = self.case_paths('adj-freeze-missing')
        proc = run_runner(self.ws, ['--FreezeManifest', os.path.join(self.ws['tmp'], 'no-such-freeze.json'),
                                    '--EvidenceRoot', ev, '--RawRoot', raw,
                                    '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'], '--DryRun'])
        self.assert_rejected(proc, r'FreezeManifest file missing', 'missing freeze manifest')
        self.assertFalse(os.path.exists(ev), 'missing freeze created an evidence root')

    def test_hap_hash_drift_rejected(self):
        """PS L2650-2700 (bad artifact hash): a freeze whose frozen HAP A
        hash does not match the on-disk HAP A bytes is rejected."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze['artifact_sha256']['hap_a'] = '0' * 64
        freeze_path = self.write_freeze(freeze, 'freeze-adj-hap-drift.json')
        ev, raw = self.case_paths('adj-hap-drift')
        proc = run_runner(self.ws, self._dryrun_args(freeze_path, ev, raw))
        self.assert_rejected(proc, r'FINAL HAP A SHA-256 mismatch', 'HAP A hash drift')

    def test_hap_b_missing_file_rejected(self):
        """PS L2650-2700: a freeze whose frozen HAP B path does not exist
        is rejected with the explicit missing-file message."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        missing = os.path.join(self.ws['tmp'], 'no-such-hap-b.hap')
        freeze['artifact_sha256']['hap_b'] = sha256_text('missing fixture')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-hapb-missing.json')
        ev, raw = self.case_paths('adj-hapb-missing')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', missing, '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'FINAL HAP B file missing', 'missing HAP B file')

    def test_evidence_root_in_repo_rejected(self):
        """PS L403-442: EvidenceRoot inside the git repository is rejected
        before any output is created."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-evroot-in-repo.json')
        ev = os.path.join(self.ws['repo'], 'in-repo-evidence')
        raw = os.path.join(self.ws['tmp'], 'in-repo-evidence.raw')
        proc = run_runner(self.ws, self._dryrun_args(freeze_path, ev, raw))
        self.assert_rejected(proc, r'EvidenceRoot and RawRoot must be outside the git repository',
                             'evidence root in repo')
        self.assertFalse(os.path.exists(ev), 'in-repo evidence root was created')

    def test_symlink_ancestor_rejected(self):
        """PS L2700-2750 (junction-rejection, A6): on Linux a symlink
        ancestor is the reparse-point equivalent; Assert-NoReparseAncestor
        must reject an EvidenceRoot under a symlinked directory."""
        real_dir = os.path.join(self.ws['tmp'], 'real-ancestor-dir')
        os.makedirs(real_dir)
        link_dir = os.path.join(self.ws['tmp'], 'symlink-ancestor')
        os.symlink(real_dir, link_dir)
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-adj-symlink.json')
        ev = os.path.join(link_dir, 'evidence-under-symlink')
        raw = os.path.join(self.ws['tmp'], 'symlink-ancestor.raw')
        proc = run_runner(self.ws, self._dryrun_args(freeze_path, ev, raw))
        self.assert_rejected(proc, r'junction or symlink ancestor', 'symlink ancestor')
        self.assertFalse(os.path.exists(ev), 'evidence under symlink ancestor was created')


# =====================================================================
# Group I: tamper rejection (confirmation record / companion / freeze
# contract / transcript integrity). The consumer must reject any tampered
# input; a tampered transcript invalidates the sealed record.
# =====================================================================


class TestTamperRejection(SelftestBase):
    """Tamper rejection: a tampered confirmation record, a missing
    companion, a freeze contract mutation that breaks the record's
    confirmation_contract_sha256 binding, and a truncated transcript that
    invalidates the sealed record."""

    def _bound_ready(self, name, ready_overrides=None):
        blocked = make_freeze(self.ws, plan_status='blocked')
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, blocked, os.path.join(self.ws['tmp'], '%s-confirmation.json' % name))
        review_path, review_sha = make_review_record(
            self.ws, blocked, confirm_sha, os.path.join(self.ws['tmp'], '%s-review.json' % name))
        ready = make_freeze(self.ws, plan_status='ready')
        if ready_overrides:
            ready.update(ready_overrides)
        bound = bind_ready_freeze(ready, confirm_path, confirm_sha, review_path, review_sha)
        return self.write_freeze(bound, 'freeze-%s.json' % name), confirm_path

    def _dryrun_args(self, freeze_path, ev, raw):
        return ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'], '--DryRun']

    def test_confirmation_record_tampered_rejected(self):
        """PS L460-700: a confirmation record whose bytes were tampered
        after the companion was written no longer matches its .sha256
        companion and the consumer rejects it."""
        bound_path, confirm_path = self._bound_ready('tamper-record')
        tampered = json.loads(open(confirm_path, encoding='utf-8').read())
        tampered['verdict'] = 'blocked'
        write_json(confirm_path, tampered)
        ev, raw = self.case_paths('tamper-record')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'companion does not match the record bytes',
                             'tampered confirmation record')
        self.assertFalse(os.path.exists(ev), 'tampered record case created an evidence root')

    def test_confirmation_companion_missing_rejected(self):
        """PS L460-700: a lone confirmation record without its .sha256
        companion is never consumable."""
        bound_path, confirm_path = self._bound_ready('tamper-companion-missing')
        os.remove(confirm_path + '.sha256')
        ev, raw = self.case_paths('tamper-companion-missing')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'companion missing; a lone record is never consumable',
                             'missing companion')

    def test_freeze_contract_mutation_rejected(self):
        """PS L497-548: mutating a confirmation-contract field in the final
        ready freeze (settings_reallow_expected_path to the other legal
        value) breaks the record's confirmation_contract_sha256 binding and
        the consumer rejects it."""
        bound_path, _ = self._bound_ready(
            'tamper-contract',
            {'settings_reallow_expected_path': 'system-reauthorization-UI'})
        ev, raw = self.case_paths('tamper-contract')
        proc = run_runner(self.ws, self._dryrun_args(bound_path, ev, raw))
        self.assert_rejected(proc, r'confirmation_contract_sha256 does not match the current confirmation contract',
                             'freeze contract mutation')

    def test_transcript_truncation_integrity_violation(self):
        """PS L4564-4607: a truncated transcript (tamper line appended
        after the manifest) fails the transcript chain integrity check and
        invalidates the sealed record."""
        fixture = make_simulation_fixture()
        fixture['tamper_transcript_after_manifest'] = True
        fixture_path = os.path.join(self.ws['tmp'], 'simulation-tamper-transcript.json')
        write_json(fixture_path, fixture)
        freeze = make_freeze(self.ws, plan_status='ready')
        freeze_path = self.write_freeze(freeze, 'freeze-tamper-transcript.json')
        ev, raw = self.case_paths('tamper-transcript')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--LiveSimulation', '--SimulationFixture', fixture_path])
        self.assertEqual(proc.returncode, 2, 'tampered transcript must exit 2:\n%s' % (
            proc.stdout + proc.stderr))
        record = json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())
        self.assertEqual(record['record_status'], 'invalidated')
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['verdict'], 'invalid')
        self.assertIn('transcript-json-invalid', record['integrity_violations'],
                      'truncated transcript violation missing: %s' % record['integrity_violations'])


# =====================================================================
# Group J: retry rejection (the current AUTH fixes attempt=initial; the
# generic infrastructure retry branch never enters this path).
# =====================================================================


class TestRetryRejection(SelftestBase):
    """Retry rejection (ADJ-20260810-0001 C6): the current AUTH fixes
    attempt=initial, so a retry attempt is rejected at every gate - the
    confirm path, the retry-field gate, the same-ID rerun gate, and the
    confirmation consumer."""

    def _prior_record(self, freeze, name):
        prior = {
            'is_evidence': True,
            'campaign_id': freeze['campaign_id'],
            'evidence_id': freeze['evidence_id'],
            'attempt': 'initial',
            'execution_mode': 'live',
            'record_status': 'blocked',
            'overall': 'blocked',
            'verdict': 'blocked',
            'infrastructure_reason': 'hdc-usb-interruption',
            'code_sha': freeze['code_sha'],
            'runner_sha256': freeze['runner_sha256'],
            'artifact_sha256': freeze['artifact_sha256'],
            'freeze_contract_sha256': runner_mod.get_freeze_contract_sha256(freeze),
        }
        path = os.path.join(self.ws['tmp'], '%s-prior-record.json' % name)
        write_json(path, prior)
        return path

    def _retry_freeze(self, name, plan_status='blocked'):
        freeze = make_freeze(self.ws, plan_status=plan_status)
        prior_path = self._prior_record(freeze, name)
        freeze['attempt'] = 'infrastructure-blocked-retry-1'
        freeze['retry'] = {
            'basis': 'prior-blocked-record',
            'infrastructure_reason': 'hdc-usb-interruption',
            'prior_record_path': prior_path,
            'prior_record_sha256': sha256_file(prior_path),
        }
        return self.write_freeze(freeze, 'freeze-%s.json' % name), freeze

    def test_confirm_retry_attempt_rejected(self):
        """PS L5920-5932 + ADJ-20260810-0001: TargetBindingConfirm with
        attempt=infrastructure-blocked-retry-1 is rejected - the generic
        infrastructure retry branch never enters the confirm path."""
        freeze_path, _ = self._retry_freeze('retry-confirm')
        record_path = os.path.join(self.ws['tmp'], 'retry-confirm.json')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path,
                                    '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'],
                                    '--TargetBindingConfirm', '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET})
        self.assert_rejected(proc, r'retries require new governance and cannot enter this path',
                             'confirm retry attempt')
        self.assertFalse(os.path.exists(record_path), 'rejected retry wrote a confirmation record')
        self.assertFalse(os.path.exists(self.ws['hdc_marker']),
                         'retry gate launched the HDC sentinel')

    def test_confirm_retry_basis_rejected(self):
        """PS L5920-5932: a confirm freeze whose retry.basis is not N/A is
        rejected by the initial-attempt retry-field gate."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze['retry'] = {'basis': 'prior-blocked-record', 'infrastructure_reason': 'N/A',
                           'prior_record_path': 'N/A', 'prior_record_sha256': 'N/A'}
        freeze_path = self.write_freeze(freeze, 'freeze-retry-basis.json')
        record_path = os.path.join(self.ws['tmp'], 'retry-basis.json')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path,
                                    '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                                    '--HdcPath', self.ws['sentinel_hdc'],
                                    '--TargetBindingConfirm', '--ConfirmationRecord', record_path],
                          env={'PHYS_1_TARGET': TARGET})
        self.assert_rejected(proc, r'initial attempt retry fields must be N/A',
                             'confirm retry basis')
        self.assertFalse(os.path.exists(record_path), 'rejected retry basis wrote a record')

    def test_blocked_same_id_rerun_rejected(self):
        """PS L2600-2650 (duplicate-lock-no-truncation): after a blocked
        DryRun seals its evidence root, a rerun with the same evidence ID is
        rejected - existing evidence is immutable and never truncated."""
        freeze = make_freeze(self.ws, plan_status='blocked')
        freeze_path = self.write_freeze(freeze, 'freeze-rerun-first.json')
        ev, raw = self.case_paths('rerun-same-id')
        args = ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev, '--RawRoot', raw,
                '--HapA', self.ws['hap_a'], '--HapB', self.ws['hap_b'],
                '--HdcPath', self.ws['sentinel_hdc'], '--DryRun']
        proc = run_runner(self.ws, args)
        self.assertEqual(proc.returncode, 0, 'first blocked DryRun failed:\n%s' % (
            proc.stdout + proc.stderr))
        self.assertTrue(os.path.isdir(ev), 'first blocked DryRun did not seal an evidence root')
        sealed = sha256_file(os.path.join(ev, 'scenario-results.json'))
        proc2 = run_runner(self.ws, args)
        self.assert_rejected(proc2, r'EvidenceRoot already exists; existing evidence is immutable',
                             'same-ID rerun')
        self.assertEqual(sha256_file(os.path.join(ev, 'scenario-results.json')), sealed,
                         'rerun must never touch existing evidence bytes')

    def test_confirmation_record_not_retry_basis(self):
        """PS L753-896: a ready retry freeze bound with a pass confirmation
        record is rejected - the confirmation record never constitutes a
        retry basis under the current AUTH."""
        freeze_path, freeze = self._retry_freeze('retry-consumer', plan_status='ready')
        confirm_path, confirm_sha = make_confirmation_record(
            self.ws, freeze, os.path.join(self.ws['tmp'], 'retry-consumer-confirmation.json'))
        review_path, review_sha = make_review_record(
            self.ws, freeze, confirm_sha, os.path.join(self.ws['tmp'], 'retry-consumer-review.json'))
        bound = bind_ready_freeze(freeze, confirm_path, confirm_sha, review_path, review_sha)
        bound_path = self.write_freeze(bound, 'freeze-retry-consumer-bound.json')
        ev, raw = self.case_paths('retry-consumer')
        proc = run_runner(self.ws, ['--FreezeManifest', bound_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--DryRun'])
        self.assert_rejected(proc, r'can never consume this confirmation',
                             'confirmation record as retry basis')
        self.assertFalse(os.path.exists(ev), 'rejected retry consumer created an evidence root')


# =====================================================================
# U9b skeletons: PS cases not yet ported (negative / out-of-bounds /
# mutual-exclusion groups). Each carries the PS phase + line range.
# =====================================================================


class TestU9bSkeletons(SelftestBase):
    """U9b ported phases (PS L1000-2606): c7/m3/m4 scenario refinements,
    adj-s3/s5/s7 fd and process-boundary rules, capture-degraded negatives,
    prior-blocked binding, finally flags, install/cleanup assessment,
    ADJ-20260808-0003 layout/classification gates, repository gate and
    transcript integrity. Each method ports one PS phase group."""

    def _run_live(self, fixture, name):
        fixture_path = os.path.join(self.ws['tmp'], 'simulation-%s.json' % name)
        write_json(fixture_path, fixture)
        freeze = make_freeze(self.ws, plan_status='ready')
        freeze_path = self.write_freeze(freeze, 'freeze-live-%s.json' % name)
        ev, raw = self.case_paths(name)
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--LiveSimulation', '--SimulationFixture', fixture_path])
        return proc, ev, raw

    def _record(self, ev):
        return json.loads(open(os.path.join(ev, 'scenario-results.json'), encoding='utf-8').read())

    def _transcript(self, ev):
        entries = []
        with open(os.path.join(ev, 'projection', 'transcript.redacted.jsonl'),
                  encoding='utf-8-sig') as f:
            for line in f:
                line = line.strip()
                if line:
                    entries.append(json.loads(line))
        return entries

    def test_c7_s4_deny_pre_capture_proof(self):
        """PS L1000-1050 c7-s4-deny-pre-capture-proof: deny proof capturable
        before the operator clicks Deny; capture hdc-command precedes the
        scenario-4 observation record."""
        fixture = make_simulation_fixture()
        fixture['scenario_events']['4'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4' % BUNDLE_B},
        ]
        proc, ev, raw = self._run_live(fixture, 'c7-s4-deny-pre-capture')
        self.assertEqual(proc.returncode, 0, 'S4 pre-capture deny proof crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        s4 = record['scenarios'][3]
        self.assertEqual(s4['result'], 'pass')
        self.assertEqual(s4['reason'], 'deny-layout-and-full-window-without-B-create')
        self.assertIs(s4['deny_screen'], True)
        self.assertEqual(s4['deny_screen_capture']['status'], 'collected')
        self.assertIs(s4['deny_screen_capture']['visible'], True)
        self.assertEqual(s4['deny_screen_capture']['result'], 'pass')
        entries = self._transcript(ev)
        capture_idx = [int(e['payload']['index']) for e in entries
                       if e['payload']['kind'] == 'hdc-command'
                       and e['payload']['data'].get('operation') == 'ScreenCap'
                       and 'scenario-4-authorization' in ' '.join(
                           e['payload']['data'].get('arguments', []))]
        obs_idx = [int(e['payload']['index']) for e in entries
                   if e['payload']['kind'] == 'scenario-observation'
                   and int(e['payload']['data'].get('scenario', -1)) == 4]
        self.assertEqual(len(capture_idx), 1, 'S4 deny capture hdc-command missing')
        self.assertEqual(len(obs_idx), 1, 'S4 observation record missing')
        self.assertLess(capture_idx[0], obs_idx[0],
                        'S4 deny capture happened after the observation (after the Deny action)')
        self.assertIs(s4['full_window_after_action'], True)

    def test_m3_scenario6_b_ui_start(self):
        """PS L1050-1150 m3-scenario6 new/no-new B UI_START: new B start
        passes with b6 binding; missing new B start invalidates immediately."""
        # New B UI_START: S4 releases b4, S6 starts a fresh b6 and rejects on
        # the frozen 2203002 code; the released S4 request must not pollute S6.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['4'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4' % BUNDLE_B},
        ]
        fixture['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b6' % BUNDLE_B},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN'},
        ]
        proc, ev, raw = self._run_live(fixture, 'm3-new-b-start')
        self.assertEqual(proc.returncode, 0, 'M3 new B UI_START crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        s6 = record['scenarios'][5]
        self.assertEqual(s6['result'], 'pass')
        self.assertEqual(s6['reason'], 'B-explicit-conflict-rejection')
        self.assertIs(s6['a_accepted'], True)
        self.assertEqual(s6['request_id_b'], 'b6')
        s6_text = '\n'.join(e['text'] for e in s6['observation']['events'])
        self.assertNotIn('UI_START|bundle=%s|requestId=b4' % BUNDLE_B, s6_text,
                         'released S4 request polluted scenario 6 new B UI_START correlation')
        # No new B UI_START: S6 sees UI_START_SKIPPED and invalidates immediately.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['4'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4' % BUNDLE_B},
        ]
        fixture2['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_START_SKIPPED|bundle=%s|reason=operation-pending' % BUNDLE_B},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'm3-no-new-b-start')
        self.assertEqual(proc2.returncode, 2, 'M3 no new B UI_START did not invalidate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'invalid')
        self.assertEqual(record2['scenarios'][5]['result'], 'invalid')
        self.assertRegex(record2['scenarios'][5]['reason'], r'UI_START_SKIPPED')
        self.assertNotIn('observation', record2['scenarios'][6],
                         'M3 invalid run continued into S7')
        self.assertIs(record2['cleanup_result']['verified_absent'], True)

    def test_m4_s7_active_request_binding(self):
        """PS L1150-1250 m4-s7-active-request-binding-and-screenshot-naming:
        S7 binds the calculated active request only; null activeRequest never
        falls back to window-event inference."""
        # Null activeRequest: S6 produces no UI_START; a complete foreign stop
        # chain in S7 must not backfill a pass - S6 invalidates first.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START_SKIPPED|bundle=%s|reason=operation-pending' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> UI_START_SKIPPED|bundle=%s|reason=operation-pending' % BUNDLE_B},
        ]
        fixture['scenario_events']['7'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> STOP_PROMISE_RESOLVED|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a5'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 5, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc, ev, raw = self._run_live(fixture, 'm4-null-active-s7')
        self.assertEqual(proc.returncode, 2, 'M4 null-active protocol did not invalidate in S6:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][5]['result'], 'invalid')
        self.assertRegex(record['scenarios'][5]['reason'], r'UI_START_SKIPPED')
        self.assertNotIn('observation', record['scenarios'][6], 'M4 invalid S6 still entered S7')
        self.assertIs(record['cleanup_result']['verified_absent'], True)
        # Positive binding: base fixture S6 keeps A active as a6; S7 binds a6.
        proc2, ev2, raw2 = self._run_live(make_simulation_fixture(), 'm4-bound-s7')
        self.assertEqual(proc2.returncode, 0, 'M4 bound S7 crashed:\n%s' % (proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        s7 = record2['scenarios'][6]
        self.assertEqual(s7['request_id'], 'a6')
        self.assertEqual(s7['reason'], 'terminal-and-post-destroy-snapshot-confirmed')
        self.assertEqual(s7['result'], 'pass')
        self.assertNotIn('requestId-missing', s7['reason'])
        self.assertIs(s7['post_cleanup_capture'], True)
        self.assertEqual(s7['post_cleanup_capture_name'], 'scenario-7-post-cleanup')
        screens = [r for r in record2.get('screenshot_reference', [])
                   if r.get('name') == 'scenario-7-post-cleanup']
        self.assertGreaterEqual(len(screens), 1, 'M4 post-cleanup screenshot reference missing')
        # Missing destroy postcondition: S7 stop chain without onDestroy/destroy-begin.
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['7'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> STOP_PROMISE_RESOLVED|bundle=%s|requestId=a6' % BUNDLE_A},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'm4-no-cleanup-s7')
        self.assertEqual(proc3.returncode, 2, 'M4 missing S7 destroy postcondition did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'invalid')
        self.assertEqual(record3['scenarios'][6]['result'], 'invalid')
        self.assertRegex(record3['scenarios'][6]['reason'], r'onDestroy-or-destroy-begin')
        self.assertIs(record3['cleanup_result']['verified_absent'], True)
        self.assertEqual(record3['cleanup_result']['status'], 'verified-clean')

    def test_adj_s3_strict_process_boundary_fallback(self):
        """PS L1250-1400 adj-s3-strict-process-boundary-fallback-and-
        reactivation: strict fallback passes with S5 reactivation proof;
        without reactivation S3 records clean_reactivation_proof=false and
        aggregation stays blocked."""
        # Strict fallback with S5 reactivation: destroy terminal missing, but
        # pre-destroy open + strict-process-boundary probes pass and S5 fresh
        # create proves clean reactivation.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['3'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a2'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-s3-fallback')
        self.assertEqual(proc.returncode, 0, 'S3 strict fallback crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        s3 = record['scenarios'][2]
        self.assertEqual(s3['result'], 'pass')
        self.assertEqual(s3['terminal_mode'], 'strict-process-boundary')
        self.assertEqual(s3['reason'], 'strict-process-boundary-terminal')
        self.assertGreaterEqual(len(s3['process_final_state_probes']), 2)
        self.assertIs(s3['bundle_present_during_probe'], True)
        self.assertIs(s3['clean_reactivation_proof'], True)
        self.assertEqual(record['scenario_aggregation']['measured_scenario_overall'], 'pass')
        # Without S5 reactivation: S5 fresh create lacks the post-create open
        # snapshot, so S3 records clean_reactivation_proof=false and the
        # aggregation stays blocked.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['3'] = fixture['scenario_events']['3']
        fixture2['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-s3-no-reactivation')
        self.assertEqual(proc2.returncode, 0, 'S3 fallback without reactivation crashed:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        s3b = record2['scenarios'][2]
        self.assertEqual(s3b['result'], 'pass')
        self.assertEqual(s3b['terminal_mode'], 'strict-process-boundary')
        self.assertIs(s3b['clean_reactivation_proof'], False)
        self.assertIs(record2['scenario_aggregation']['s3_clean_reactivation_proof'], False)
        s5b = record2['scenarios'][4]
        self.assertEqual(s5b['result'], 'blocked')
        self.assertEqual(s5b['reason'], 'fresh-create-proof-missing')
        self.assertEqual(record2['scenario_aggregation']['measured_scenario_overall'], 'blocked')
        self.assertEqual(record2['scenario_aggregation']['overall'], 'blocked')

    def test_adj_s5_fd_still_open(self):
        """PS L1400-1650 adj-s5 fd-still-open / pre-destroy-open / probe
        spacing / probe infra / reopen-not-open: FD_STILL_OPEN fails,
        pre-destroy open never fails, spacing override invalidates, probe
        124/125 blocks as infrastructure, reopen=true never counts as open."""
        # S5 post-destroy FD_STILL_OPEN: consecutive absent probes must never
        # override the fail verdict.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_STILL_OPEN'},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-s5-fd-still-open')
        self.assertEqual(proc.returncode, 0, 'S5 FD_STILL_OPEN crashed:\n%s' % (proc.stdout + proc.stderr))
        record = self._record(ev)
        s5 = record['scenarios'][4]
        self.assertEqual(s5['result'], 'fail')
        self.assertEqual(s5['reason'], 'FD_STILL_OPEN')
        self.assertIs(s5['fd_still_open'], True)
        absent = [p for p in s5['process_final_state_probes'] if p['status'] == 'absent']
        self.assertGreaterEqual(len(absent), 2, 'S5 FD_STILL_OPEN case had no absent probe evidence')
        self.assertIs(s5['process_absent_evidence']['met'], True)
        # Pre-destroy open snapshot is expected mid-destroy and never fails.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a5|trigger=onDestroy'},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-s5-pre-destroy-open')
        self.assertEqual(proc2.returncode, 0, 'S5 pre-destroy open crashed:\n%s' % (proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['scenarios'][4]['result'], 'pass')
        self.assertEqual(record2['scenarios'][4]['reason'], 'settings-app-info-force-stop-terminal')
        self.assertIs(record2['scenarios'][4]['fd_still_open'], False)
        # Probe spacing override below the frozen 3s stays blocked/invalid.
        fixture3 = make_simulation_fixture()
        fixture3['probe_spacing_override_seconds'] = 1
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-s5-spacing-override')
        self.assertEqual(proc3.returncode, 2, 'S5 spacing override did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'invalid')
        self.assertEqual(record3['scenarios'][4]['result'], 'invalid')
        self.assertRegex(record3['scenarios'][4]['reason'], r'probe-spacing-insufficient')
        self.assertIs(record3['cleanup_result']['verified_absent'], True)
        # Probe 124/125 classifies as infrastructure exactly like live HDC.
        fixture4 = make_simulation_fixture()
        fixture4['hdc_failures'] = [
            {'operation': 'BundleDump', 'occurrence': 14, 'exit_code': 124, 'stdout': '', 'stderr': 'timeout'},
        ]
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-s5-probe-infra')
        self.assertEqual(proc4.returncode, 2, 'S5 probe infra did not stop:\n%s' % (proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['scenarios'][4]['result'], 'blocked')
        self.assertRegex(record4['scenarios'][4]['reason'], r'force-stop-process-check-unverifiable')
        self.assertEqual(record4['infrastructure_reason'], 'hdc-usb-interruption')
        self.assertEqual(record4['overall'], 'blocked')
        # reopen=true never counts as the clean reactivation open proof.
        fixture5 = make_simulation_fixture()
        fixture5['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|reopen=true|marker=CREATE_ACCEPTED'},
        ]
        proc5, ev5, raw5 = self._run_live(fixture5, 'adj-s5-reopen-not-open')
        self.assertEqual(proc5.returncode, 0, 'S5 reopen crashed:\n%s' % (proc5.stdout + proc5.stderr))
        record5 = self._record(ev5)
        self.assertEqual(record5['scenarios'][4]['result'], 'blocked')
        self.assertEqual(record5['scenarios'][4]['reason'], 'fresh-create-proof-missing')
        # S3 FD_STILL_OPEN is not overridable by the process fallback either.
        fixture6 = make_simulation_fixture()
        fixture6['scenario_events']['3'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a2'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_STILL_OPEN'},
            {'offset_seconds': 5, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN'},
        ]
        proc6, ev6, raw6 = self._run_live(fixture6, 'adj-s3-fd-still-open')
        self.assertEqual(proc6.returncode, 0, 'S3 FD_STILL_OPEN crashed:\n%s' % (proc6.stdout + proc6.stderr))
        record6 = self._record(ev6)
        self.assertEqual(record6['scenarios'][2]['result'], 'fail')
        self.assertEqual(record6['scenarios'][2]['reason'], 'fd-still-open-after-destroy')
        self.assertEqual(record6['scenarios'][2]['terminal_mode'], 'callback-post-fd')

    def test_adj_s3_s7_terminal_missing_fd_still_open(self):
        """PS L1650-1750 adj-s3-s7-terminal-missing-post-destroy-fd-still-
        open-fail: missing destroy terminal with post-destroy FD_STILL_OPEN
        fails even with consecutive absent probes."""
        for scenario, request_id, name in ((3, 'a2', 'adj-s3-terminal-missing'),
                                           (7, 'a6', 'adj-s7-terminal-missing')):
            fixture = make_simulation_fixture()
            fixture['scenario_events'][str(scenario)] = [
                {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=%s' % (BUNDLE_A, request_id)},
                {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=%s' % request_id},
                {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=%s|trigger=onDestroy|createAccepted=true' % request_id},
                {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=%s|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN' % request_id},
                {'offset_seconds': 5, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=%s|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN' % request_id},
            ]
            fixture['process_probe_override'] = {str(scenario): [
                {'pid': 'absent', 'dump': 'present'},
                {'pid': 'absent', 'dump': 'present'},
            ]}
            proc, ev, raw = self._run_live(fixture, name)
            self.assertEqual(proc.returncode, 0,
                             'S%d terminal-missing FD_STILL_OPEN crashed:\n%s' % (
                                 scenario, proc.stdout + proc.stderr))
            record = self._record(ev)
            s = record['scenarios'][scenario - 1]
            self.assertEqual(s['result'], 'fail')
            self.assertEqual(s['reason'], 'fd-still-open-after-destroy')
            self.assertEqual(s['terminal_mode'], 'callback-post-fd')
            absent = [p for p in s['process_final_state_probes'] if p['status'] == 'absent']
            self.assertGreaterEqual(len(absent), 2,
                                    'S%d terminal-missing absent probes not recorded' % scenario)
            if scenario == 7:
                self.assertIs(s['post_cleanup_capture'], False,
                              'S7 FD_STILL_OPEN case still ran uninstall cleanup during scenario')
                self.assertIs(s['terminal_assessed'], False)

    def test_capture_degraded_decisive_negatives(self):
        """PS L1750-2200 capture-degraded decisive-capture negatives: S2
        after-allow, S4 authorization, S5 force-stop, S6 conflict, S7
        final-state capture loss invalidates; S7 final destroy fail outranks
        degradation."""
        # S5 hard FD_STILL_OPEN fail outranks capture degradation.
        fixture = make_simulation_fixture()
        fixture['capture_failures'] = ['scenario-5-app-info-force-stop']
        fixture['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_STILL_OPEN'},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-s5-fd-still-open-degraded')
        self.assertEqual(proc.returncode, 2, 'S5 decisive capture loss did not invalidate:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][4]['result'], 'invalid')
        self.assertRegex(record['scenarios'][4]['reason'], r'capture-not-collected')
        self.assertIs(record['cleanup_result']['verified_absent'], True)
        # S2 missing decisive after-Allow capture outranks functional evidence.
        fixture2 = make_simulation_fixture()
        fixture2['capture_failures'] = ['scenario-2-after-allow']
        fixture2['scenario_events']['2'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=a2|phase=create|summary=create-rejected'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-s2-create-rejected-degraded')
        self.assertEqual(proc2.returncode, 2, 'S2 decisive capture loss did not invalidate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'invalid')
        self.assertEqual(record2['scenarios'][1]['result'], 'invalid')
        self.assertRegex(record2['scenarios'][1]['reason'], r'authorization-not-dismissed|capture-not-collected')
        self.assertNotIn('observation', record2['scenarios'][2], 'S2 invalid run continued')
        self.assertIs(record2['cleanup_result']['verified_absent'], True)
        # S4 authorization pre-capture must be machine-verifiable before Deny.
        fixture3 = make_simulation_fixture()
        fixture3['capture_failures'] = ['scenario-4-authorization']
        fixture3['scenario_events']['4'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4' % BUNDLE_B},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=b4' % BUNDLE_B},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-s4-deny-created-degraded')
        self.assertEqual(proc3.returncode, 2, 'S4 authorization pre-capture loss did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['record_status'], 'invalidated')
        self.assertEqual(record3['scenarios'][3]['result'], 'invalid')
        self.assertIs(record3['cleanup_result']['verified_absent'], True)
        # S6 conflict checkpoint capture is required.
        fixture4 = make_simulation_fixture()
        fixture4['capture_failures'] = ['scenario-6-conflict']
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-s6-conflict-capture-required')
        self.assertEqual(proc4.returncode, 2, 'S6 continued after its conflict capture failed:\n%s' % (
            proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['record_status'], 'invalidated')
        self.assertEqual(record4['scenarios'][5]['result'], 'invalid')
        self.assertIs(record4['cleanup_result']['verified_absent'], True)
        # S6 legacy semantic confirmation object is ignored, never gating.
        fixture5 = make_simulation_fixture()
        fixture5['operator']['confirmations'] = {'legacy_semantic_claim': True}
        proc5, ev5, raw5 = self._run_live(fixture5, 'adj-s6-legacy-confirmation-ignored')
        self.assertEqual(proc5.returncode, 0, 'S6 legacy confirmation crashed:\n%s' % (
            proc5.stdout + proc5.stderr))
        record5 = self._record(ev5)
        self.assertEqual(record5['scenarios'][5]['result'], 'pass')
        self.assertEqual(record5['scenarios'][5]['reason'], 'B-explicit-conflict-rejection')
        self.assertNotIn('operator_state', record5['scenarios'][5])
        # Semantic layout mismatch stops before the next action.
        fixture6 = make_simulation_fixture()
        fixture6['layout_profiles']['scenario-4-authorization'] = 'wrong-page'
        proc6, ev6, raw6 = self._run_live(fixture6, 'layout-review-mismatch')
        self.assertEqual(proc6.returncode, 2, 'layout mismatch did not invalidate:\n%s' % (
            proc6.stdout + proc6.stderr))
        record6 = self._record(ev6)
        self.assertEqual(record6['record_status'], 'invalidated')
        self.assertEqual(record6['invalidated_step']['step_index'], 2)
        entries6 = self._transcript(ev6)
        checkpoints = [e for e in entries6
                       if e['payload']['kind'] == 'machine-layout-checkpoint'
                       and e['payload']['data'].get('checkpoint', {}).get('name') == 'scenario-4-authorization'
                       and e['payload']['data']['checkpoint'].get('matching') is False]
        self.assertEqual(len(checkpoints), 1, 'layout mismatch assessment missing from transcript')
        deny_actions = [e for e in entries6
                        if e['payload']['kind'] == 'operator-mechanical-action'
                        and int(e['payload']['data'].get('scenario', -1)) == 4
                        and int(e['payload']['data'].get('step_index', -1)) == 2]
        self.assertEqual(len(deny_actions), 0, 'Deny action occurred after the failed pre-action visual gate')
        transcript_text6 = open(os.path.join(ev6, 'projection', 'transcript.redacted.jsonl'),
                                encoding='utf-8').read()
        for token in ('NO-DUAL-ACTIVE-CAPTURED', 'DUAL-ACTIVE-CAPTURED', 'FINAL-CLEANUP-CAPTURED',
                      'Confirm-VisibleFact', 'Read-OperatorResponse'):
            self.assertNotIn(token, transcript_text6, 'transcript retained a removed semantic token')
        for idx in (4, 5, 6):
            self.assertEqual(record6['scenarios'][idx]['reason'], 'not-run-due-to-invalid')
        # S5 Settings>VPN page is optional and never decisive.
        fixture7 = make_simulation_fixture()
        fixture7['capture_failures'] = ['scenario-5-settings-vpn-page']
        proc7, ev7, raw7 = self._run_live(fixture7, 'adj-s5-vpn-page-not-required')
        self.assertEqual(proc7.returncode, 0, 'S5 treated the optional VPN page as decisive:\n%s' % (
            proc7.stdout + proc7.stderr))
        record7 = self._record(ev7)
        self.assertEqual(record7['scenarios'][4]['result'], 'pass')
        self.assertEqual(record7['scenarios'][4]['settings_vpn_page_capture']['status'], 'not-required')
        # S5 force-stop checkpoint capture is required (invalidates at step 4).
        fixture8 = make_simulation_fixture()
        fixture8['capture_failures'] = ['scenario-5-app-info-force-stop']
        proc8, ev8, raw8 = self._run_live(fixture8, 'adj-s5-force-stop-capture-required')
        self.assertEqual(proc8.returncode, 2, 'S5 continued after its force-stop capture failed:\n%s' % (
            proc8.stdout + proc8.stderr))
        record8 = self._record(ev8)
        self.assertEqual(record8['record_status'], 'invalidated')
        self.assertEqual(record8['invalidated_step']['step_index'], 4)
        self.assertIs(record8['cleanup_result']['verified_absent'], True)
        # S7 final destroy fail outranks the degraded final-state capture.
        fixture9 = make_simulation_fixture()
        fixture9['capture_failures'] = ['scenario-7-final-state']
        fixture9['scenario_events']['7'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a6'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a6|fdMarker=FD_STILL_OPEN'},
            {'offset_seconds': 5, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-destroy-resolved|open=true|marker=FD_STILL_OPEN'},
        ]
        proc9, ev9, raw9 = self._run_live(fixture9, 'adj-s7-final-destroy-fail-degraded')
        self.assertEqual(proc9.returncode, 0, 'S7 final destroy fail degraded crashed:\n%s' % (
            proc9.stdout + proc9.stderr))
        record9 = self._record(ev9)
        s7 = record9['scenarios'][6]
        self.assertEqual(s7['result'], 'fail')
        self.assertEqual(s7['reason'], 'fd-still-open-after-destroy')
        self.assertIs(s7['post_cleanup_capture'], False)
        self.assertIs(s7['terminal_assessed'], False)
        degraded = [r for r in record9.get('screenshot_reference', [])
                    if r.get('name') == 'scenario-7-final-state' and r.get('status') == 'degraded']
        self.assertGreaterEqual(len(degraded), 1, 'S7 degraded fixture did not degrade the final-state capture')
        self.assertEqual(record9['scenario_aggregation']['measured_scenario_overall'], 'fail')
        self.assertEqual(record9['scenario_aggregation']['overall'], 'fail')
        self.assertEqual(record9['overall'], 'fail')
        self.assertEqual(record9['verdict'], 'fail')

    def test_adj_s5_force_stop_flow(self):
        """PS L2200-2400 adj-s5-force-stop-flow: wrong-B/same-name effect,
        bundle-absent, no-fresh-create, pure-missing, override garbage and
        failure, vpn-page-optional-layout-irrelevant."""
        # Force-stop produces NO A-side effect: A <bundle>:vpn stays present.
        fixture = make_simulation_fixture()
        fixture['operator']['no_effect_steps'] = ['5.4']
        fixture['process_probe_override'] = {'5': [
            {'pid': 'present', 'dump': 'present'},
            {'pid': 'present', 'dump': 'present'},
            {'pid': 'present', 'dump': 'present'},
        ]}
        proc, ev, raw = self._run_live(fixture, 'adj-s5-missing-confirm')
        self.assertEqual(proc.returncode, 2, 'S5 force-stop process still present did not invalidate:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][4]['result'], 'invalid')
        self.assertRegex(record['scenarios'][4]['reason'], r'process-state|absent|probe|present')
        # Bundle absent during probes: non-pass BundleDump stays blocked.
        fixture2 = make_simulation_fixture()
        fixture2['process_probe_override'] = {'5': [{'pid': 'absent', 'dump': 'absent'}]}
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-s5-bundle-absent')
        self.assertEqual(proc2.returncode, 2, 'S5 bundle absent did not stop:\n%s' % (proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'blocked')
        self.assertEqual(record2['scenarios'][4]['result'], 'blocked')
        self.assertRegex(record2['scenarios'][4]['reason'],
                         r'force-stop-process-check-unverifiable|probe-unknown-or-error|absent|bundle')
        # Extra UI_STOP_SKIPPED while waiting for the create terminal is invalid.
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_STOP_SKIPPED|bundle=%s|reason=no-active-request' % BUNDLE_A},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-s5-no-fresh-create')
        self.assertEqual(proc3.returncode, 2, 'S5 no fresh create did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'invalid')
        self.assertEqual(record3['scenarios'][4]['result'], 'invalid')
        self.assertRegex(record3['scenarios'][4]['reason'], r'UI_STOP_SKIPPED|fresh-create|create-terminal')
        # Pure missing create terminal (no extra UI action) is plain blocked.
        fixture4 = make_simulation_fixture()
        fixture4['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
        ]
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-s5-pure-missing-create')
        self.assertEqual(proc4.returncode, 2, 'S5 pure-missing create terminal did not stop as blocked:\n%s' % (
            proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['overall'], 'blocked')
        self.assertEqual(record4['scenarios'][4]['result'], 'blocked')
        self.assertRegex(record4['scenarios'][4]['reason'],
                         r'platform-marker-missing|fresh-create-terminal-missing|create-terminal')
        # No post-create open snapshot: fresh-create proof blocked.
        fixture5 = make_simulation_fixture()
        fixture5['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
        ]
        proc5, ev5, raw5 = self._run_live(fixture5, 'adj-s5-no-post-create-open')
        self.assertEqual(proc5.returncode, 0, 'S5 no post-create open crashed:\n%s' % (proc5.stdout + proc5.stderr))
        record5 = self._record(ev5)
        self.assertEqual(record5['scenarios'][4]['result'], 'blocked')
        self.assertEqual(record5['scenarios'][4]['reason'], 'fresh-create-proof-missing')
        self.assertEqual(record5['scenarios'][4]['assertions']['fresh_create_proof'], 'blocked')
        # Unknown process_probe_override enum is blocked.
        fixture6 = make_simulation_fixture()
        fixture6['process_probe_override'] = {'5': [{'pid': 'not-a-status', 'dump': 'present'}]}
        proc6, ev6, raw6 = self._run_live(fixture6, 'adj-s5-override-garbage')
        self.assertEqual(proc6.returncode, 2, 'S5 garbage override did not stop:\n%s' % (proc6.stdout + proc6.stderr))
        record6 = self._record(ev6)
        self.assertEqual(record6['overall'], 'blocked')
        self.assertEqual(record6['scenarios'][4]['result'], 'blocked')
        # hdc_failures take priority over process_probe_override as blocked.
        fixture7 = make_simulation_fixture()
        fixture7['process_probe_override'] = {'5': [{'pid': 'absent', 'dump': 'present'}]}
        fixture7['hdc_failures'] = [
            {'operation': 'BundleDump', 'occurrence': 14, 'exit_code': 1, 'stdout': '', 'stderr': 'forced-failure-over-override'},
        ]
        proc7, ev7, raw7 = self._run_live(fixture7, 'adj-s5-override-failure')
        self.assertEqual(proc7.returncode, 2, 'S5 override+failure did not stop:\n%s' % (proc7.stdout + proc7.stderr))
        record7 = self._record(ev7)
        self.assertEqual(record7['overall'], 'blocked')
        self.assertEqual(record7['scenarios'][4]['result'], 'blocked')
        self.assertRegex(record7['scenarios'][4]['reason'],
                         r'force-stop-process-check-unverifiable|probe-unknown-or-error')
        # Wrong Settings>VPN page override is irrelevant: S5 still passes.
        fixture8 = make_simulation_fixture()
        fixture8['layout_profiles']['scenario-5-settings-vpn-page'] = 'wrong-page'
        proc8, ev8, raw8 = self._run_live(fixture8, 'adj-s5-vpn-page-optional')
        self.assertEqual(proc8.returncode, 0, 'S5 treated the optional VPN page as decisive:\n%s' % (
            proc8.stdout + proc8.stderr))
        record8 = self._record(ev8)
        self.assertEqual(record8['scenarios'][4]['result'], 'pass')
        self.assertEqual(record8['scenarios'][4]['settings_vpn_page_capture']['status'], 'not-required')

    def test_adj_s3_s7_request_binding_future_events(self):
        """PS L2350-2500 adj-s3-s7-request-binding-and-future-events +
        adj-s7-post-uninstall-backfeed-blocked: wrong requestId invalidates,
        future destroy arms probes from virtual elapsed time, post-uninstall
        state never backfills a terminal pass."""
        # S3 wrong requestId is protocol invalid.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['3'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a-wrong' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a-wrong'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a-wrong|trigger=onDestroy'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a-wrong|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 5, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a-wrong|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-s3-wrong-request')
        self.assertEqual(proc.returncode, 2, 'S3 wrong requestId did not invalidate:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][2]['result'], 'invalid')
        self.assertRegex(record['scenarios'][2]['reason'], r'UI_STOP-wrong-requestId|requestId')
        self.assertEqual(record['scenarios'][3]['reason'], 'not-run-due-to-invalid')
        # S7 wrong requestId is protocol invalid.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['7'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a-wrong7' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a-wrong7'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a-wrong7|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a-wrong7|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-s7-wrong-request')
        self.assertEqual(proc2.returncode, 2, 'S7 wrong requestId did not invalidate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'invalid')
        self.assertEqual(record2['scenarios'][6]['result'], 'invalid')
        self.assertRegex(record2['scenarios'][6]['reason'], r'UI_STOP-wrong-requestId|requestId')
        self.assertIs(record2['cleanup_result']['verified_absent'], True)
        # S3 future destroy: probes arm from virtual elapsed time (>= 45s).
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['3'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 45, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a2'},
            {'offset_seconds': 46, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy|createAccepted=true'},
            {'offset_seconds': 47, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a2|phase=pre-destroy|open=true|marker=PRE_DESTROY_OPEN'},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-s3-future-destroy')
        self.assertEqual(proc3.returncode, 0, 'S3 future destroy crashed:\n%s' % (proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        s3 = record3['scenarios'][2]
        self.assertEqual(s3['result'], 'pass')
        self.assertEqual(s3['terminal_mode'], 'strict-process-boundary')
        probes3 = s3['process_final_state_probes']
        self.assertGreaterEqual(len(probes3), 2, 'S3 future destroy probes missing')
        from datetime import datetime
        action_prompt = datetime.fromisoformat(s3['observation']['action_prompt_at'])
        first_probe = datetime.fromisoformat(probes3[0]['time'])
        self.assertGreaterEqual((first_probe - action_prompt).total_seconds(), 45,
                                'S3 probes armed before virtual elapsed time')
        # S7 post-uninstall state never backfills a terminal pass.
        fixture4 = make_simulation_fixture()
        fixture4['scenario_events']['7'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a6'},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a6|trigger=onDestroy'},
        ]
        fixture4['process_probe_override'] = {'7': [{'pid': 'present', 'dump': 'present'}]}
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-s7-backfeed')
        self.assertEqual(proc4.returncode, 0, 'S7 backfeed crashed:\n%s' % (proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        s7 = record4['scenarios'][6]
        self.assertEqual(s7['result'], 'blocked')
        self.assertIs(s7['post_cleanup_capture'], False)
        probes4 = s7['process_final_state_probes']
        self.assertGreaterEqual(len(probes4), 2, 'S7 pre-uninstall probes missing')
        self.assertEqual([p['status'] for p in probes4], ['present'] * len(probes4),
                         'S7 pre-uninstall probes were backfilled with post-uninstall absent')
        self.assertIs(record4['cleanup_result']['verified_absent'], True)

    def test_prior_blocked_binding_projection(self):
        """PS L2850-3050 prior-blocked-binding-projection + negatives:
        consumed-blocked projection, hash flip changes seal, bad hash /
        incomplete / bad source rejected, legacy N/A."""
        prior_scenario_sha = 'a' * 64
        prior_manifest_sha = 'b' * 64
        prior_seal_sha = 'c' * 64
        binding = {
            'source': 'consumed-blocked',
            'evidence_id': 'EV-E3-SELFTEST-20990101-0009',
            'scenario_results_sha256': prior_scenario_sha,
            'hash_manifest_sha256': prior_manifest_sha,
            'campaign_seal_sha256': prior_seal_sha,
        }
        freeze = make_freeze(self.ws, plan_status='ready', prior_blocked_binding=binding)
        freeze_path = self.write_freeze(freeze, 'freeze-prior-binding.json')
        ev, raw = self.case_paths('prior-binding')
        proc = run_runner(self.ws, ['--FreezeManifest', freeze_path, '--EvidenceRoot', ev,
                                    '--RawRoot', raw, '--HapA', self.ws['hap_a'],
                                    '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                    '--LiveSimulation', '--SimulationFixture',
                                    self._fixture_path('prior-binding')])
        self.assertEqual(proc.returncode, 0, 'prior blocked binding failed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        pbb = record['prior_blocked_binding']
        self.assertEqual(pbb['source'], 'consumed-blocked')
        self.assertEqual(pbb['evidence_id'], 'EV-E3-SELFTEST-20990101-0009')
        self.assertEqual(pbb['scenario_results_sha256'], prior_scenario_sha)
        self.assertEqual(pbb['hash_manifest_sha256'], prior_manifest_sha)
        self.assertEqual(pbb['campaign_seal_sha256'], prior_seal_sha)
        self.assertEqual(pbb['binding_source'], 'freeze-manifest')
        for forbidden in ('verified', 'reverified', 'record_path'):
            self.assertNotIn(forbidden, pbb, 'prior binding declared %s' % forbidden)
        self.assertFalse(os.path.exists(os.path.join(raw, 'prior-blocked-record.json')),
                         'prior binding copied raw prior record')
        manifest = json.loads(open(os.path.join(ev, 'hash-manifest.json'), encoding='utf-8').read())
        self.assertEqual([r for r in manifest.get('external_raw_files', [])
                          if 'PRIOR-BLOCKED' in str(r.get('reference', ''))], [],
                         'manifest sealed prior raw copies')
        record_sha = sha256_file(os.path.join(ev, 'scenario-results.json'))
        manifest_sha = sha256_file(os.path.join(ev, 'hash-manifest.json'))
        seal = json.loads(open(os.path.join(ev, 'campaign-seal.json'), encoding='utf-8').read())
        self.assertEqual(seal['record']['sha256'], record_sha)
        self.assertEqual(seal['manifest']['sha256'], manifest_sha)
        self.assertEqual(assert_evidence_outputs(ev), [])
        self.assertEqual(record['integrity_violations'], [])
        # Hash flip changes the projected record and the seal binding.
        flip = deep_copy(freeze)
        flip['prior_blocked_binding']['scenario_results_sha256'] = 'd' * 64
        flip_path = self.write_freeze(flip, 'freeze-prior-binding-hash-flip.json')
        ev2, raw2 = self.case_paths('prior-binding-hash-flip')
        proc2 = run_runner(self.ws, ['--FreezeManifest', flip_path, '--EvidenceRoot', ev2,
                                     '--RawRoot', raw2, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('prior-binding-hash-flip')])
        self.assertEqual(proc2.returncode, 0, 'hash-flip prior binding failed:\n%s' % (
            proc2.stdout + proc2.stderr))
        flip_record_sha = sha256_file(os.path.join(ev2, 'scenario-results.json'))
        flip_seal = json.loads(open(os.path.join(ev2, 'campaign-seal.json'), encoding='utf-8').read())
        self.assertNotEqual(flip_record_sha, record_sha,
                            'changing a projected prior hash must change scenario-results sha256')
        self.assertNotEqual(flip_seal['record']['sha256'], seal['record']['sha256'],
                            'changing a projected prior hash must change the seal record binding')
        # Negatives: bad hash, incomplete object, bad source, legacy N/A.
        bad = deep_copy(freeze)
        bad['prior_blocked_binding']['scenario_results_sha256'] = 'not-a-sha'
        bad_path = self.write_freeze(bad, 'freeze-prior-binding-bad-hash.json')
        ev3, raw3 = self.case_paths('prior-binding-bad-hash')
        proc3 = run_runner(self.ws, ['--FreezeManifest', bad_path, '--EvidenceRoot', ev3,
                                     '--RawRoot', raw3, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('prior-binding-bad-hash')])
        self.assert_rejected(proc3, r'scenario_results_sha256|final SHA-256', 'bad prior hash')
        self.assertFalse(os.path.exists(ev3), 'bad prior hash created an evidence root')
        incomplete = make_freeze(self.ws, plan_status='ready')
        incomplete['prior_blocked_binding'] = {
            'source': 'consumed-blocked',
            'evidence_id': 'EV-E3-SELFTEST-20990101-0009',
            'scenario_results_sha256': prior_scenario_sha,
        }
        incomplete_path = self.write_freeze(incomplete, 'freeze-prior-binding-incomplete.json')
        ev4, raw4 = self.case_paths('prior-binding-incomplete')
        proc4 = run_runner(self.ws, ['--FreezeManifest', incomplete_path, '--EvidenceRoot', ev4,
                                     '--RawRoot', raw4, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('prior-binding-incomplete')])
        self.assert_rejected(proc4, r'hash_manifest_sha256|campaign_seal_sha256|final SHA-256',
                             'incomplete prior binding')
        self.assertFalse(os.path.exists(ev4), 'incomplete prior binding created an evidence root')
        bad_source = deep_copy(freeze)
        bad_source['prior_blocked_binding']['source'] = 'retry'
        bad_source_path = self.write_freeze(bad_source, 'freeze-prior-binding-bad-source.json')
        ev5, raw5 = self.case_paths('prior-binding-bad-source')
        proc5 = run_runner(self.ws, ['--FreezeManifest', bad_source_path, '--EvidenceRoot', ev5,
                                     '--RawRoot', raw5, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('prior-binding-bad-source')])
        self.assert_rejected(proc5, r'prior_blocked_binding', 'non-consumed-blocked source')
        self.assertFalse(os.path.exists(ev5), 'bad source created an evidence root')
        legacy = make_freeze(self.ws, plan_status='ready')
        del legacy['prior_blocked_binding']
        legacy_path = self.write_freeze(legacy, 'freeze-legacy-no-prior-binding.json')
        ev6, raw6 = self.case_paths('legacy-no-prior-binding')
        proc6 = run_runner(self.ws, ['--FreezeManifest', legacy_path, '--EvidenceRoot', ev6,
                                     '--RawRoot', raw6, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('legacy-no-prior-binding')])
        self.assertEqual(proc6.returncode, 0, 'legacy freeze without prior_blocked_binding failed:\n%s' % (
            proc6.stdout + proc6.stderr))
        self.assertEqual(self._record(ev6)['prior_blocked_binding'], 'N/A')

    def _fixture_path(self, name):
        path = os.path.join(self.ws['tmp'], 'simulation-%s.json' % name)
        write_json(path, make_simulation_fixture())
        return path

    def test_finally_installation_flags(self):
        """PS L3050-3100 finally-installation-flags: finally honors
        InstalledA/InstalledB and StagingSent flags."""
        # InstallB fails: finally uninstalls only the installed A and removes staging.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'InstallB', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'signature rejected'},
        ]
        proc, ev, raw = self._run_live(fixture, 'install-b-fail')
        self.assertNotEqual(proc.returncode, 0, 'InstallB failure did not fail runner')
        record = self._record(ev)
        uninstalls = [a for a in record['cleanup_result']['actions']
                      if a.get('operation') == 'finally-uninstall']
        self.assertEqual(len(uninstalls), 1, 'finally did not honor InstalledA/InstalledB flags')
        self.assertEqual(uninstalls[0]['bundle'], BUNDLE_A)
        staging = [a for a in record['cleanup_result']['actions']
                   if a.get('operation') == 'finally-remove-staging']
        self.assertEqual(len(staging), 1, 'finally did not honor StagingSent flag')
        # InstallA fails: nothing was installed, so nothing is uninstalled.
        fixture2 = make_simulation_fixture()
        fixture2['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'signature rejected'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'install-a-fail')
        self.assertNotEqual(proc2.returncode, 0, 'InstallA failure did not fail runner')
        record2 = self._record(ev2)
        uninstalls2 = [a for a in record2['cleanup_result']['actions']
                       if a.get('operation') == 'finally-uninstall']
        self.assertEqual(len(uninstalls2), 0, 'finally uninstalled a bundle whose install never succeeded')

    def test_capture_degraded_and_death(self):
        """PS L3100-3550 capture-degraded-blocks-without-crash +
        capture-death-late-create-confirmation-and-timeout + install
        assessment negatives + staging residual + mkdir-fail."""
        # Unknown fault capture degrades to blocked, never infrastructure.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'FaultA', 'occurrence': 1, 'exit_code': 127, 'stdout': '', 'stderr': 'unknown command'},
        ]
        proc, ev, raw = self._run_live(fixture, 'fault-degraded')
        self.assertEqual(proc.returncode, 0, 'unknown fault capture crashed:\n%s' % (proc.stdout + proc.stderr))
        record = self._record(ev)
        s7 = record['scenarios'][6]
        self.assertEqual(s7['result'], 'blocked')
        self.assertIs(s7['fault_capture_degraded'], True)
        artifacts = [a for a in record['fault_reference']['artifacts']
                     if a.get('operation') == 'FaultA' and a.get('status') == 'degraded']
        self.assertEqual(len(artifacts), 1, 'failed fault artifact not referenced as degraded')
        self.assertIs(s7['observation']['complete_window_observed'], True)
        self.assertNotIn('infrastructure_reason', record)
        degraded = [d for d in record.get('capture_degraded', [])
                    if d.get('component') == 'FaultA']
        self.assertGreaterEqual(len(degraded), 1)
        self.assertEqual(degraded[0]['category'], 'non-infrastructure')
        # Permission fault also degrades, never infrastructure.
        fixture2 = make_simulation_fixture()
        fixture2['hdc_failures'] = [
            {'operation': 'FaultB', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'Permission denied'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'fault-permission')
        self.assertEqual(proc2.returncode, 0, 'permission fault crashed:\n%s' % (proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['scenarios'][6]['result'], 'blocked')
        self.assertIs(record2['scenarios'][6]['fault_capture_degraded'], True)
        self.assertIs(record2['scenarios'][6]['observation']['complete_window_observed'], True)
        self.assertNotIn('infrastructure_reason', record2)
        # Capture death stops later scenario execution, never fail-open.
        fixture3 = make_simulation_fixture()
        fixture3['capture_die_scenario'] = 4
        proc3, ev3, raw3 = self._run_live(fixture3, 'capture-dead')
        self.assertNotEqual(proc3.returncode, 0, 'capture death did not stop later scenario execution')
        record3 = self._record(ev3)
        s4 = record3['scenarios'][3]
        blocked = (s4['result'] == 'blocked' and 'observation' in s4
                   and s4['observation'].get('capture_degraded') is True
                   and not s4.get('full_window_after_action'))
        stopped = s4['result'] in ('blocked', 'invalid') and s4['result'] not in ('pass', 'fail')
        self.assertTrue(blocked or stopped, 'capture death allowed deny to fail open')
        self.assertTrue(record3.get('infrastructure_reason') == 'hdc-usb-interruption'
                        or record3['overall'] in ('blocked', 'invalid'),
                        'capture death did not set infrastructure reason or stop overall')
        s5 = record3['scenarios'][4]
        self.assertTrue('observation' not in s5 or re.search(r'not-run|capture|invalid', s5['reason']),
                        'campaign continued into scenario 5 after capture death')
        # Install exit0 semantic failure is FUNCTIONAL_FAIL, never marked installed.
        fixture4 = make_simulation_fixture()
        fixture4['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 0, 'stdout': 'error: failed to execute your command.', 'stderr': ''},
        ]
        proc4, ev4, raw4 = self._run_live(fixture4, 'install-exit0-fail')
        self.assertNotEqual(proc4.returncode, 0, 'exit0 install semantic failure did not fail runner')
        record4 = self._record(ev4)
        self.assertRegex(record4['actual'], r'FUNCTIONAL_FAIL')
        self.assertEqual([a for a in record4['cleanup_result']['actions']
                          if a.get('operation') == 'finally-uninstall'], [])
        # Install dump-absent is non-infrastructure blocked without InstalledA.
        fixture5 = make_simulation_fixture()
        fixture5['hdc_failures'] = [
            {'operation': 'BundleDump', 'occurrence': 3, 'exit_code': 0,
             'stdout': 'error: failed to get information and the parameters may be wrong.', 'stderr': ''},
        ]
        proc5, ev5, raw5 = self._run_live(fixture5, 'install-dump-absent')
        self.assertNotEqual(proc5.returncode, 0, 'install dump-absent did not fail runner')
        record5 = self._record(ev5)
        self.assertRegex(record5['actual'], r'install confirmation blocked')
        self.assertRegex(record5['actual'], r'bundle-dump-absent')
        self.assertNotRegex(record5['actual'], r'FUNCTIONAL_FAIL')
        self.assertEqual([a for a in record5['cleanup_result']['actions']
                          if a.get('operation') == 'finally-uninstall'], [])
        self.assertNotIn('infrastructure_reason', record5)
        # Install dump permission denied is non-infrastructure blocked.
        fixture6 = make_simulation_fixture()
        fixture6['hdc_failures'] = [
            {'operation': 'BundleDump', 'occurrence': 3, 'exit_code': 1, 'stdout': '', 'stderr': 'Permission denied'},
        ]
        proc6, ev6, raw6 = self._run_live(fixture6, 'install-dump-permission')
        self.assertNotEqual(proc6.returncode, 0, 'install dump-permission did not stop runner')
        record6 = self._record(ev6)
        self.assertRegex(record6['actual'], r'install confirmation blocked')
        self.assertNotRegex(record6['actual'], r'FUNCTIONAL_FAIL')
        self.assertNotIn('infrastructure_reason', record6)
        # Install warning without success is non-infrastructure blocked.
        fixture7 = make_simulation_fixture()
        fixture7['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 0, 'stdout': 'warning: cache rebuild skipped', 'stderr': ''},
        ]
        proc7, ev7, raw7 = self._run_live(fixture7, 'install-warning')
        self.assertNotEqual(proc7.returncode, 0, 'install warning-without-success did not stop runner')
        record7 = self._record(ev7)
        self.assertRegex(record7['actual'], r'install outcome blocked')
        self.assertNotRegex(record7['actual'], r'FUNCTIONAL_FAIL')
        self.assertNotIn('infrastructure_reason', record7)
        # Staging residual after failed install stays blocked-unknown-residual.
        fixture8 = make_simulation_fixture()
        fixture8['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'signature rejected'},
            {'operation': 'StagingProbe', 'occurrence': 2, 'exit_code': 0,
             'stdout': 'drwxrwxrwx 3 shell shell 4096 2026-01-01 00:00 /data/local/tmp/e3-phys-preflight', 'stderr': ''},
        ]
        proc8, ev8, raw8 = self._run_live(fixture8, 'staging-residual')
        self.assertNotEqual(proc8.returncode, 0, 'staging residual did not fail runner')
        record8 = self._record(ev8)
        cleanup8 = record8['cleanup_result']
        self.assertEqual(cleanup8['status'], 'blocked-unknown-residual')
        self.assertIs(cleanup8['staging_sent_remaining'], True)
        self.assertIs(cleanup8['verified_absent'], False)
        # Staging cannot-access is never treated as absent/clean.
        fixture9 = make_simulation_fixture()
        fixture9['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'signature rejected'},
            {'operation': 'StagingProbe', 'occurrence': 2, 'exit_code': 1, 'stdout': '',
             'stderr': "ls: cannot access '/data/local/tmp/e3-phys-preflight': Permission denied"},
        ]
        proc9, ev9, raw9 = self._run_live(fixture9, 'staging-cannot-access')
        self.assertNotEqual(proc9.returncode, 0, 'staging cannot-access did not fail runner')
        record9 = self._record(ev9)
        cleanup9 = record9['cleanup_result']
        self.assertEqual(cleanup9['status'], 'blocked-unknown-residual')
        self.assertIs(cleanup9['verified_absent'], False)
        # Mkdir failure still attempts fixed staging cleanup in finally.
        fixture10 = make_simulation_fixture()
        fixture10['hdc_failures'] = [
            {'operation': 'MkdirStaging', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'mkdir failed'},
        ]
        proc10, ev10, raw10 = self._run_live(fixture10, 'mkdir-fail')
        self.assertNotEqual(proc10.returncode, 0, 'mkdir failure did not stop runner')
        record10 = self._record(ev10)
        self.assertEqual([a for a in record10['cleanup_result']['actions']
                          if a.get('operation') == 'finally-remove-staging'],
                         [a for a in record10['cleanup_result']['actions']
                          if a.get('operation') == 'finally-remove-staging'])
        self.assertEqual(len([a for a in record10['cleanup_result']['actions']
                              if a.get('operation') == 'finally-remove-staging']), 1,
                         'mkdir failure did not attempt fixed staging cleanup in finally')

    def test_auth_capture_and_request_correlation(self):
        """PS L3550-3750 auth-capture-fail / late-b-create /
        multi-b-requestid / bm-dump-json redaction."""
        # Authorization capture failure invalidates S2 at the machine gate.
        fixture = make_simulation_fixture()
        fixture['capture_failures'] = ['scenario-2-authorization']
        proc, ev, raw = self._run_live(fixture, 'auth-capture-fail')
        self.assertEqual(proc.returncode, 2, 'authorization capture failure did not invalidate:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][1]['result'], 'invalid')
        self.assertRegex(record['scenarios'][1]['reason'],
                         r'scenario-2-authorization-capture-not-collected')
        self.assertEqual(record['scenarios'][2]['reason'], 'not-run-due-to-invalid')
        self.assertIs(record['cleanup_result']['verified_absent'], True)
        # Late B create inside the measured window is untrusted.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['4'].append(
            {'offset_seconds': 50, 'step_index': 2,
             'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=b4' % BUNDLE_B})
        proc2, ev2, raw2 = self._run_live(fixture2, 'late-b-create')
        self.assertEqual(proc2.returncode, 2, 'late B create did not invalidate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['scenarios'][3]['result'], 'invalid')
        self.assertEqual(record2['scenarios'][3]['reason'], 'deny-action-produced-create-untrusted')
        # Secondary B requestId create is protocol invalid.
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['4'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4-primary' % BUNDLE_B},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b4-secondary' % BUNDLE_B},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=b4-secondary' % BUNDLE_B},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'multi-b-requestid')
        self.assertEqual(proc3.returncode, 2, 'multi B requestId did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'invalid')
        self.assertEqual(record3['scenarios'][3]['result'], 'invalid')
        self.assertRegex(record3['scenarios'][3]['reason'], r'UI_START|expected-one')
        # bm dump JSON values are redacted in projected evidence.
        fixture4 = make_simulation_fixture()
        fixture4['scenario_events']['2'].append(
            {'offset_seconds': 6,
             'text': '<DEVICE_OBSERVED_AT> BM_DUMP_JSON {"udid":"DEVICE-UDID-SHOULD-REDACT","deviceIds":["ID-1"],"endpoint":"192.0.2.55:8710"}'})
        proc4, ev4, raw4 = self._run_live(fixture4, 'bm-dump-json')
        self.assertEqual(proc4.returncode, 0, 'bm dump JSON crashed:\n%s' % (proc4.stdout + proc4.stderr))
        evidence_text = ''
        for root, _, files in os.walk(ev4):
            for name in files:
                with open(os.path.join(root, name), encoding='utf-8', errors='replace') as f:
                    evidence_text += f.read()
        for secret in ('DEVICE-UDID-SHOULD-REDACT', '192.0.2.55'):
            self.assertNotIn(secret, evidence_text,
                             'bm dump JSON value %s leaked into projected evidence' % secret)

    def test_legacy_confirmation_objects(self):
        """PS L3750-3850 missing-confirmation / strict-boolean: legacy
        semantic confirmation objects are ignored, never gating."""
        # Missing FINAL-CLEANUP confirmation never gates S7.
        fixture = make_simulation_fixture()
        fixture['operator']['confirmations'] = {'FINAL-CLEANUP-CAPTURED': False}
        proc, ev, raw = self._run_live(fixture, 'missing-confirmation')
        self.assertEqual(proc.returncode, 0, 'legacy FINAL-CLEANUP confirmation crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['scenarios'][6]['result'], 'pass')
        self.assertNotIn('visible_cleanup_confirmed', record['scenarios'][6],
                         'S7 still depended on FINAL-CLEANUP operator confirmation')
        # String-typed legacy confirmation is never a semantic gate.
        fixture2 = make_simulation_fixture()
        fixture2['operator']['confirmations'] = {'DENY-SCREEN-CAPTURED': 'true'}
        proc2, ev2, raw2 = self._run_live(fixture2, 'strict-boolean')
        self.assertEqual(proc2.returncode, 0, 'legacy string confirmation crashed:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['scenarios'][3]['result'], 'pass')
        self.assertEqual(record2['record_status'], 'blocked')
        self.assertIs(record2['is_evidence'], False)

    def test_settings_reallow_path_observation_only(self):
        """PS L3850-4000 settings-reallow-path-observation-only + bad-policy
        freeze: path mismatch with complete functional chain passes with
        match=false; non-observation-only policy rejected."""
        # Path mismatch with a complete functional chain passes with match=false.
        fixture = make_simulation_fixture()
        fixture['layout_profiles']['scenario-5-reactivation'] = 'authorization'
        fixture['scenario_events']['5'] = [
            {'offset_seconds': 1, 'step_index': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'step_index': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 9, 'step_index': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc, ev, raw = self._run_live(fixture, 'path-mismatch-pass')
        self.assertEqual(proc.returncode, 0, 'path mismatch with complete chain crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        s5 = record['scenarios'][4]
        self.assertEqual(s5['result'], 'pass')
        self.assertIs(s5['settings_reallow_path']['match'], False)
        self.assertEqual(s5['settings_reallow_path']['expected'], 'direct-system-activation')
        self.assertEqual(s5['settings_reallow_path']['actual'], 'system-reauthorization-UI')
        self.assertEqual(s5['settings_reallow_path']['observation'], 'machine-layout-and-event-classified')
        self.assertEqual(s5['settings_reallow_path']['policy'], 'observation-only')
        self.assertEqual(record['scenario_aggregation']['measured_scenario_overall'], 'pass')
        # Missing functional markers stay blocked, never invalid.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['5'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'missing-functional-marker')
        self.assertEqual(proc2.returncode, 2, 'missing functional marker did not stop as blocked:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'blocked')
        self.assertEqual(record2['scenarios'][4]['result'], 'blocked')
        self.assertRegex(record2['scenarios'][4]['reason'],
                         r'platform-marker-missing|fresh-create-terminal-missing|create-terminal')
        # Non-observation-only path policy is rejected.
        freeze3 = make_freeze(self.ws, plan_status='ready',
                              settings_reallow_path_policy='strict-equal')
        freeze3_path = self.write_freeze(freeze3, 'freeze-bad-path-policy.json')
        ev3, raw3 = self.case_paths('bad-path-policy')
        proc3 = run_runner(self.ws, ['--FreezeManifest', freeze3_path, '--EvidenceRoot', ev3,
                                     '--RawRoot', raw3, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('bad-path-policy')])
        self.assert_rejected(proc3, r'settings_reallow_path_policy must be observation-only',
                             'bad path policy')
        self.assertFalse(os.path.exists(ev3), 'bad path policy created an evidence root')
        # Missing path policy is rejected.
        freeze4 = make_freeze(self.ws, plan_status='ready')
        del freeze4['settings_reallow_path_policy']
        freeze4_path = self.write_freeze(freeze4, 'freeze-missing-path-policy.json')
        ev4, raw4 = self.case_paths('missing-path-policy')
        proc4 = run_runner(self.ws, ['--FreezeManifest', freeze4_path, '--EvidenceRoot', ev4,
                                     '--RawRoot', raw4, '--HapA', self.ws['hap_a'],
                                     '--HapB', self.ws['hap_b'], '--HdcPath', self.ws['sentinel_hdc'],
                                     '--LiveSimulation', '--SimulationFixture',
                                     self._fixture_path('missing-path-policy')])
        self.assert_rejected(proc4, r'settings_reallow_path_policy', 'missing path policy')
        self.assertFalse(os.path.exists(ev4), 'missing path policy created an evidence root')

    def test_install_timeout_and_cleanup_unknown(self):
        """PS L4000-4100 install-timeout / cleanup-unknown: 124/125
        classified as hdc-usb-interruption; unknown residual cleanup state
        stays blocked-unknown-residual."""
        # Install timeout is infrastructure, never a functional failure.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 124, 'stdout': '', 'stderr': 'operation timeout'},
        ]
        proc, ev, raw = self._run_live(fixture, 'install-timeout')
        self.assertNotEqual(proc.returncode, 0, 'install timeout did not stop the campaign')
        record = self._record(ev)
        self.assertEqual(record['infrastructure_reason'], 'hdc-usb-interruption')
        self.assertEqual(record['overall'], 'blocked')
        self.assertEqual(record['record_status'], 'blocked')
        # Unknown residual cleanup state stays blocked-unknown-residual.
        fixture2 = make_simulation_fixture()
        fixture2['hdc_failures'] = [
            {'operation': 'InstallA', 'occurrence': 1, 'exit_code': 1, 'stdout': '', 'stderr': 'signature rejected'},
            {'operation': 'BundleDump', 'occurrence': 3, 'exit_code': 127, 'stdout': '', 'stderr': 'unknown query state'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'cleanup-unknown')
        self.assertNotEqual(proc2.returncode, 0, 'cleanup-unknown did not fail runner')
        record2 = self._record(ev2)
        cleanup2 = record2['cleanup_result']
        self.assertEqual(cleanup2['status'], 'blocked-unknown-residual')
        self.assertIs(cleanup2['verified_absent'], False)
        self.assertEqual(record2['record_status'], 'blocked')

    def test_adj_0003_slow_operator_and_extra_action(self):
        """PS L4100-4300 adj-0003-slow-operator-pre-enter-capture /
        extra-action-invalid / cross-scenario-gap-extra-action."""
        # Slow operator: pre-enter events are captured, never lost.
        fixture = make_simulation_fixture()
        fixture['operator']['action_delay_seconds'] = 8
        fixture['scenario_events']['2'] = [
            {'offset_seconds': 0.5, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 0.5, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 1, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a2|fd=42|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 2, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a2|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
        ]
        fixture['scenario_events']['3'] = [
            {'offset_seconds': 0.5, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> UI_STOP|bundle=%s|requestId=a2|basis=last-known-request' % BUNDLE_A},
            {'offset_seconds': 1, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> STOP_PROMISE_RESOLVED|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 2, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_ONDESTROY|requestId=a2'},
            {'offset_seconds': 3, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_BEGIN|requestId=a2|trigger=onDestroy'},
            {'offset_seconds': 4, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a2|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 5, 'relative_to_prompt': True, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a2|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-slow-operator')
        self.assertEqual(proc.returncode, 0, 'slow-operator pre-enter events were not captured:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['scenarios'][1]['result'], 'pass')
        self.assertEqual(record['scenarios'][2]['result'], 'pass')
        interval = record['scenarios'][1]['observation']['action_interval_seconds']
        self.assertGreaterEqual(interval, 8, 'S2 action interval did not reflect the slow operator delay')
        self.assertLess(interval, 60)
        # Extra UI action not owned by the current step is invalid.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['2'] = fixture['scenario_events']['2']
        fixture2['gap_actions'] = [
            {'scenario': 2, 'after_step_index': 1,
             'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a2-extra' % BUNDLE_A},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-extra-action')
        self.assertEqual(proc2.returncode, 2, 'extra UI action did not invalidate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'invalid')
        self.assertEqual(record2['scenarios'][1]['result'], 'invalid')
        self.assertRegex(record2['scenarios'][1]['reason'], r'stray-operator-action|UI_START')
        self.assertEqual(record2['scenarios'][2]['result'], 'invalid')
        self.assertEqual(record2['scenarios'][2]['reason'], 'not-run-due-to-invalid')
        # Cross-scenario gap action invalidates the next scenario before any prompt.
        fixture3 = make_simulation_fixture()
        fixture3['gap_actions'] = [
            {'scenario': 5, 'after_step_index': 0,
             'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5-stray' % BUNDLE_A},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-0003-cross-scenario-gap')
        self.assertEqual(proc3.returncode, 2, 'cross-scenario gap action did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'invalid')
        self.assertEqual(record3['scenarios'][3]['result'], 'pass')
        self.assertEqual(record3['scenarios'][4]['result'], 'invalid')
        self.assertRegex(record3['scenarios'][4]['reason'], r'stray-operator-action|UI_START')

    def test_adj_0003_layouts_and_resample(self):
        """PS L4300-4600 adj-0003-real-layouts / api26-auth-minimal /
        historical-attributes / layout-resample-same-name-final-only."""
        real_auth = [
            {'attributes': {'bundleName': 'com.huawei.hmos.vpndialog', 'type': 'Dialog',
                            'id': '', 'key': '', 'text': 'E3 Physical VPN Preflight'},
             'children': [
                 {'attributes': {'bundleName': '', 'type': 'Text', 'id': '', 'key': '',
                                 'text': '是否允许使用 VPN？'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_cancel_button',
                                 'key': 'permission_cancel_button', 'text': '取消'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_allow_button',
                                 'key': 'permission_allow_button', 'text': '允许'}, 'children': []},
             ]},
        ]
        # Real authorization layout matches.
        fixture = make_simulation_fixture()
        fixture['layout_profiles']['scenario-2-authorization'] = real_auth
        proc, ev, raw = self._run_live(fixture, 'adj-0003-real-layouts')
        self.assertEqual(proc.returncode, 0, 'real authorization layout did not match:\n%s' % (
            proc.stdout + proc.stderr))
        # API26 minimal authorization shape matches.
        fixture2 = make_simulation_fixture()
        fixture2['layout_profiles']['scenario-2-authorization'] = [
            {'attributes': {'bundleName': 'com.huawei.hmos.vpndialog', 'type': 'Dialog',
                            'id': '', 'key': '', 'text': ''},
             'children': [
                 {'attributes': {'bundleName': '', 'type': 'Text', 'id': '', 'key': '',
                                 'text': '是否允许使用 VPN？'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_allow_button',
                                 'key': 'permission_allow_button', 'text': '允许'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_cancel_button',
                                 'key': 'permission_cancel_button', 'text': '取消'}, 'children': []},
             ]},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-api26-auth-min')
        self.assertEqual(proc2.returncode, 0, 'API26 minimal authorization layout did not match:\n%s' % (
            proc2.stdout + proc2.stderr))
        # Historical attributes shape: deep child with English Allow/Cancel tokens.
        fixture3 = make_simulation_fixture()
        fixture3['layout_profiles']['scenario-2-authorization'] = [
            {'attributes': {'bundleName': 'com.ohos.sceneboard', 'type': 'WindowScene',
                            'id': 'session10', 'key': 'session10', 'text': ''},
             'children': [
                 {'attributes': {'bundleName': '', 'type': 'root', 'id': '', 'key': '', 'text': ''},
                  'children': [
                      {'attributes': {'bundleName': 'com.huawei.hmos.vpndialog', 'type': 'Dialog',
                                      'id': '', 'key': '', 'text': 'E3 Physical VPN Preflight'},
                       'children': [
                           {'attributes': {'bundleName': '', 'type': 'Text', 'id': '', 'key': '',
                                           'text': '是否允许使用 VPN？'}, 'children': []},
                           {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_allow_button',
                                           'key': 'permission_allow_button', 'text': 'Allow'}, 'children': []},
                           {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'permission_cancel_button',
                                           'key': 'permission_cancel_button', 'text': 'Cancel'}, 'children': []},
                       ]},
                  ]},
             ]},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-0003-historical-shape')
        self.assertEqual(proc3.returncode, 0, 'historical attributes layout did not match:\n%s' % (
            proc3.stdout + proc3.stderr))
        # Same-name layout resample keeps exactly one final capture per name.
        fixture4 = make_simulation_fixture()
        fixture4['layout_ready_delays'] = {'scenario-2-authorization': 3}
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-0003-layout-resample')
        self.assertEqual(proc4.returncode, 0, 'same-name layout resample did not converge:\n%s' % (
            proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        refs4 = [r for r in record4.get('layout_state_reference', [])
                 if r.get('name') == 'scenario-2-authorization' and r.get('status') == 'collected']
        self.assertEqual(len(refs4), 1, 'same-name layout resample left more than one final entry')
        self.assertEqual(refs4[0]['layout']['reference'], 'RAW-LAYOUT-scenario-2-authorization')
        manifest4 = json.loads(open(os.path.join(ev4, 'hash-manifest.json'), encoding='utf-8').read())
        raw_refs4 = [r for r in manifest4.get('external_raw_files', [])
                     if re.search(r'capture-scenario-2-authorization\.(json|png)$', str(r.get('reference', '')))]
        self.assertEqual(len(raw_refs4), 2, 'resample manifest references intermediate overwritten captures')
        self.assertEqual(len([r for r in raw_refs4 if r.get('reference') == 'RAW-capture-scenario-2-authorization.json']), 1)
        self.assertEqual(len([r for r in raw_refs4 if r.get('reference') == 'RAW-capture-scenario-2-authorization.png']), 1)
        attempts4 = [e for e in self._transcript(ev4)
                     if e['payload']['kind'] == 'machine-layout-resample'
                     and e['payload']['data'].get('name') == 'scenario-2-authorization']
        self.assertGreaterEqual(len(attempts4), 1, 'resample did not record machine-layout-resample attempts')
        self.assertEqual(assert_evidence_outputs(ev4), [])

    def test_adj_0003_entry_and_app_info_gates(self):
        """PS L4600-4700 adj-0003-wrong-bundle-entry / fake-vpn-app /
        settings-app-info-positive-minimal."""
        # Entry page with the right buttons but the WRONG bundle is invalid.
        fixture = make_simulation_fixture()
        fixture['layout_profiles']['scenario-2-entry-a'] = [
            {'attributes': {'bundleName': 'com.example.wrongbundle', 'type': 'root',
                            'id': '', 'key': '', 'text': ''},
             'children': [
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'start-vpn',
                                 'key': 'start-vpn', 'text': 'Start VPN'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'stop-vpn',
                                 'key': 'stop-vpn', 'text': 'Stop VPN'}, 'children': []},
             ]},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-wrong-bundle-entry')
        self.assertEqual(proc.returncode, 2, 'entry layout with wrong bundle passed the entry gate:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'invalid')
        self.assertEqual(record['scenarios'][1]['result'], 'invalid')
        self.assertRegex(record['scenarios'][1]['reason'], r'expected-bundle|layout')
        # App page containing only the word VPN fails the settings-app-info gate.
        fixture2 = make_simulation_fixture()
        fixture2['layout_profiles']['scenario-5-app-info'] = 'settings-vpn-fake-app'
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-fake-vpn-app')
        self.assertEqual(proc2.returncode, 2, 'fake VPN app page passed the settings-app-info gate:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'invalid')
        self.assertEqual(record2['scenarios'][4]['result'], 'invalid')
        # Minimal settings-app-info with generic ids/keys passes on the process effect gate.
        fixture3 = make_simulation_fixture()
        fixture3['layout_profiles']['scenario-5-app-info'] = [
            {'attributes': {'bundleName': 'com.huawei.hmos.settings', 'type': 'root',
                            'id': '', 'key': '', 'text': ''},
             'children': [
                 {'attributes': {'bundleName': '', 'type': 'Text', 'id': 'title', 'key': 'title',
                                 'text': 'E3 Physical VPN Preflight'}, 'children': []},
                 {'attributes': {'bundleName': '', 'type': 'Button', 'id': 'button1', 'key': 'button1',
                                 'text': '强制停止'}, 'children': []},
             ]},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-0003-settings-app-info-minimal')
        self.assertEqual(proc3.returncode, 0, 'minimal settings-app-info layout failed the gate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        s5 = record3['scenarios'][4]
        self.assertEqual(s5['result'], 'pass')
        self.assertIs(s5['app_info_force_stop_capture']['machine_verified'], True)
        self.assertIs(s5['process_absent_evidence']['met'], True)
        self.assertIs(s5['bundle_present_during_probe'], True)
        self.assertNotIn('scenario_invalid', record3)
        self.assertNotEqual(record3['record_status'], 'invalidated')

    def test_adj_0003_infra_capture(self):
        """PS L4700-4800 adj-0003-infra-capture-blocked /
        continuous-capture-infra-degraded: 124/125 capture propagates
        infrastructure blocked, never scenario invalid."""
        # Decisive layout checkpoint ScreenCap 124 propagates infrastructure.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'ScreenCap', 'occurrence': 2, 'exit_code': 124, 'stdout': '', 'stderr': 'operation timeout'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-infra-capture')
        self.assertNotEqual(proc.returncode, 0, 'infrastructure capture failure did not stop the campaign:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'blocked')
        self.assertEqual(record['record_status'], 'blocked')
        self.assertEqual(record['infrastructure_reason'], 'hdc-usb-interruption')
        self.assertNotEqual(record['record_status'], 'invalidated')
        self.assertNotIn('scenario_invalid', record)
        # Continuous raw-hilog capture death is infrastructure, never invalid.
        fixture2 = make_simulation_fixture()
        fixture2['capture_die_scenario'] = 2
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-continuous-infra-degraded')
        self.assertNotEqual(proc2.returncode, 0, 'continuous capture death did not stop the campaign:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'blocked')
        self.assertEqual(record2['record_status'], 'blocked')
        self.assertEqual(record2['infrastructure_reason'], 'hdc-usb-interruption')
        self.assertNotEqual(record2['record_status'], 'invalidated')
        self.assertNotIn('scenario_invalid', record2)
        infra = [d for d in record2.get('capture_degraded', [])
                 if 'raw-hilog' in str(d.get('component', '')) and d.get('category') == 'infrastructure']
        self.assertGreaterEqual(len(infra), 1, 'continuous raw-hilog infrastructure degradation not recorded')

    def test_adj_0003_layout_choice(self):
        """PS L4800-4950 adj-0003-layout-choice-s5/s6-auth-delay: dual-profile
        8s same-name resample converges to authorization with a single
        same-name capture ref."""
        # S5 reactivation layout choice converges to authorization.
        fixture = make_simulation_fixture()
        fixture['layout_profiles']['scenario-5-reactivation'] = 'authorization'
        fixture['layout_ready_delays'] = {'scenario-5-reactivation': 5}
        fixture['scenario_events']['5'] = [
            {'offset_seconds': 1, 'step_index': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 2, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a5' % BUNDLE_A},
            {'offset_seconds': 3, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a5|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'step_index': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_DESTROY_RESOLVED|requestId=a5|fdMarker=FD_CLOSED_CONFIRMED'},
            {'offset_seconds': 9, 'step_index': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a5|phase=post-destroy-resolved|open=false|marker=FD_CLOSED_CONFIRMED'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-s5-layout-choice-auth-delay')
        self.assertEqual(proc.returncode, 0, 'S5 layout-choice auth delay did not converge:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertNotIn('scenario_invalid', record)
        self.assertNotEqual(record['record_status'], 'invalidated')
        self.assertEqual(record['scenarios'][4]['settings_reallow_path']['actual'],
                         'system-reauthorization-UI')
        refs = [r for r in record.get('layout_state_reference', [])
                if r.get('name') == 'scenario-5-reactivation' and r.get('status') == 'collected']
        self.assertEqual(len(refs), 1, 'S5 layout-choice left more than one same-name capture ref')
        attempts = [e for e in self._transcript(ev)
                    if e['payload']['kind'] == 'machine-layout-choice-resample'
                    and e['payload']['data'].get('name') == 'scenario-5-reactivation']
        self.assertGreaterEqual(len(attempts), 1, 'S5 layout-choice did not record dual-profile resample attempts')
        # S6 A reactivation layout choice converges to authorization.
        fixture2 = make_simulation_fixture()
        fixture2['layout_profiles']['scenario-6-reactivation-a'] = 'authorization'
        fixture2['layout_ready_delays'] = {'scenario-6-reactivation-a': 5}
        fixture2['scenario_events']['6'] = [
            {'offset_seconds': 1, 'step_index': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'step_index': 3, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b6' % BUNDLE_B},
            {'offset_seconds': 9, 'step_index': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-s6-layout-choice-auth-delay')
        self.assertEqual(proc2.returncode, 0, 'S6 layout-choice auth delay did not converge:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertNotIn('scenario_invalid', record2)
        self.assertNotEqual(record2['record_status'], 'invalidated')
        self.assertEqual(record2['scenarios'][5]['a_reauth_path'], 'system-reauthorization-UI')
        refs2 = [r for r in record2.get('layout_state_reference', [])
                 if r.get('name') == 'scenario-6-reactivation-a' and r.get('status') == 'collected']
        self.assertEqual(len(refs2), 1, 'S6 layout-choice left more than one same-name capture ref')

    def test_adj_0003_s2_precondition(self):
        """PS L4950-5050 adj-0003-s2-process-precondition-infra-blocked /
        mismatch-blocked: PidOf infra and process-state-mismatch are blocked,
        never scenario invalid."""
        # S2 PidOf infra (exit 124) is blocked with hdc-usb-interruption.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'PidOf', 'occurrence': 3, 'exit_code': 124, 'stdout': '', 'stderr': 'operation timeout'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-s2-precondition-infra')
        self.assertNotEqual(proc.returncode, 0, 'S2 process precondition infra did not stop the campaign:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'blocked')
        self.assertEqual(record['record_status'], 'blocked')
        self.assertEqual(record['infrastructure_reason'], 'hdc-usb-interruption')
        self.assertNotIn('scenario_invalid', record)
        self.assertNotEqual(record['record_status'], 'invalidated')
        # S2 process present when expected absent is blocked process-state-mismatch.
        fixture2 = make_simulation_fixture()
        fixture2['hdc_failures'] = [
            {'operation': 'PidOf', 'occurrence': 3, 'exit_code': 0, 'stdout': '99999', 'stderr': ''},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-s2-precondition-mismatch')
        self.assertNotEqual(proc2.returncode, 0, 'S2 process precondition mismatch did not stop the campaign:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['overall'], 'blocked')
        self.assertEqual(record2['record_status'], 'blocked')
        self.assertNotIn('scenario_invalid', record2)
        self.assertNotEqual(record2['record_status'], 'invalidated')
        self.assertRegex(proc2.stdout + proc2.stderr,
                         r'machine-precondition-blocked|process-state-mismatch')

    def test_adj_0003_s6_classification(self):
        """PS L5050-5300 adj-0003-s6-b-non-frozen-code-blocked / a-reject-
        fail / a-start-promise-rejected-blocked / a-reauth-success."""
        # S6 B reject with a NON-frozen code is platform blocked, never invalid.
        fixture = make_simulation_fixture()
        fixture['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b6' % BUNDLE_B},
            {'offset_seconds': 9, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203001,name=BusinessError,message=another active vpn exists'},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-s6-b-non-frozen-code')
        self.assertEqual(proc.returncode, 0, 'S6 non-frozen B code crashed:\n%s' % (proc.stdout + proc.stderr))
        record = self._record(ev)
        self.assertEqual(record['overall'], 'blocked')
        self.assertEqual(record['record_status'], 'blocked')
        self.assertNotIn('scenario_invalid', record)
        self.assertNotEqual(record['record_status'], 'invalidated')
        s6 = record['scenarios'][5]
        self.assertEqual(s6['result'], 'blocked')
        self.assertEqual(s6['reason'], 'B-conflict-code-not-frozen:2203001')
        self.assertEqual(s6['b_rejection_code'], 2203001)
        self.assertIs(s6['b_accepted'], False)
        self.assertIs(s6['a_accepted'], True)
        self.assertEqual(record['scenarios'][6]['result'], 'blocked')
        self.assertEqual(record['scenarios'][6]['reason'], 'not-run-after-platform-blocked')
        self.assertIs(record['cleanup_result']['verified_absent'], True)
        self.assertEqual(record['cleanup_result']['status'], 'verified-clean')
        # S6 A extension create reject is a functional fail, not an operator invalid.
        fixture2 = make_simulation_fixture()
        fixture2['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=a6|phase=create|summary=code=2201001,name=BusinessError,message=create rejected'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-s6-a-reject')
        self.assertEqual(proc2.returncode, 0, 'S6 A extension reject was not classified as functional fail:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        s6b = record2['scenarios'][5]
        self.assertEqual(s6b['result'], 'fail')
        self.assertEqual(s6b['reason'], 'A-create-rejected-or-invalid-fd')
        self.assertIs(s6b['a_on_create'], True)
        self.assertIs(s6b['a_extension_rejected'], True)
        self.assertIs(s6b['a_auth_unclassified'], False)
        self.assertIs(s6b['b_accepted'], False)
        self.assertEqual(record2['scenarios'][6]['result'], 'blocked')
        self.assertRegex(record2['scenarios'][6]['reason'], r'not-run-after-functional-fail')
        self.assertEqual(record2['overall'], 'fail')
        # S6 A pure authorization-layer outcome is blocked, never fail/invalid.
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['6'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'text': '<DEVICE_OBSERVED_AT> START_PROMISE_REJECTED|bundle=%s|requestId=a6|summary=user-denied' % BUNDLE_A},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-0003-s6-a-start-promise-rejected')
        self.assertEqual(proc3.returncode, 0, 'S6 A START_PROMISE_REJECTED was not blocked:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertEqual(record3['overall'], 'blocked')
        self.assertEqual(record3['record_status'], 'blocked')
        self.assertNotIn('scenario_invalid', record3)
        self.assertNotEqual(record3['record_status'], 'invalidated')
        s6c = record3['scenarios'][5]
        self.assertEqual(s6c['result'], 'blocked')
        self.assertEqual(s6c['reason'], 'authorization-outcome-unclassified')
        self.assertIs(s6c['a_on_create'], False)
        self.assertIs(s6c['a_extension_rejected'], False)
        self.assertIs(s6c['a_auth_unclassified'], True)
        self.assertEqual(record3['scenarios'][6]['result'], 'blocked')
        self.assertEqual(record3['scenarios'][6]['reason'], 'not-run-after-platform-blocked')
        self.assertIs(record3['cleanup_result']['verified_absent'], True)
        self.assertEqual(record3['cleanup_result']['status'], 'verified-clean')
        # S6 A optional reauthorization succeeds through the B conflict path.
        fixture4 = make_simulation_fixture()
        fixture4['layout_profiles']['scenario-6-reactivation-a'] = 'authorization'
        fixture4['scenario_events']['6'] = [
            {'offset_seconds': 1, 'step_index': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 2, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_ONCREATE|bundle=%s|requestId=a6' % BUNDLE_A},
            {'offset_seconds': 3, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_RESOLVED|requestId=a6|accepted=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 4, 'step_index': 2, 'text': '<DEVICE_OBSERVED_AT> VPN_FD_SNAPSHOT|requestId=a6|phase=post-create|open=true|marker=CREATE_ACCEPTED'},
            {'offset_seconds': 8, 'step_index': 3, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=b6' % BUNDLE_B},
            {'offset_seconds': 9, 'step_index': 3, 'text': '<DEVICE_OBSERVED_AT> VPN_CREATE_REJECTED|requestId=b6|phase=create|summary=code=2203002,name=BusinessError,message=conflict with an already active VPN'},
        ]
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-0003-s6-a-reauth-success')
        self.assertEqual(proc4.returncode, 0, 'S6 A reauth success crashed:\n%s' % (proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['overall'], 'blocked')
        self.assertEqual(record4['record_status'], 'blocked')
        self.assertEqual(record4['scenario_aggregation']['measured_scenario_overall'], 'pass')
        self.assertEqual([s for s in record4['scenarios'] if s['result'] != 'pass'], [])
        s6d = record4['scenarios'][5]
        self.assertEqual(s6d['result'], 'pass')
        self.assertEqual(s6d['reason'], 'B-explicit-conflict-rejection')
        self.assertEqual(s6d['a_reauth_path'], 'system-reauthorization-UI')
        allow_steps = [st for st in s6d['observation']['operator_steps']
                       if int(st.get('step_index', -1)) == 2 and st.get('expected_action') == '点击 Allow']
        self.assertEqual(len(allow_steps), 1, 'S6 A reauth Allow step missing from the observation')

    def test_adj_0003_process_target_and_tri_state(self):
        """PS L5300-5450 adj-0003-process-target-verified / null-tri-state /
        wait-state-tamper."""
        # Process-target absent checkpoint is blocked with the explicit reason.
        fixture = make_simulation_fixture()
        fixture['hdc_failures'] = [
            {'operation': 'PidOf', 'occurrence': 5, 'exit_code': 1, 'stdout': '', 'stderr': ''},
        ]
        proc, ev, raw = self._run_live(fixture, 'adj-0003-process-target-absent')
        self.assertEqual(proc.returncode, 0, 'process-target absent checkpoint crashed:\n%s' % (
            proc.stdout + proc.stderr))
        record = self._record(ev)
        s2 = record['scenarios'][1]
        self.assertEqual(s2['result'], 'blocked')
        self.assertRegex(s2['reason'], r'process-target-unverified')
        self.assertIs(s2['process_target_verified'], False)
        self.assertEqual(record['scenarios'][2]['result'], 'blocked')
        # Process-target error (124) is blocked with the explicit reason.
        fixture2 = make_simulation_fixture()
        fixture2['hdc_failures'] = [
            {'operation': 'PidOf', 'occurrence': 5, 'exit_code': 124, 'stdout': '', 'stderr': 'timeout'},
        ]
        proc2, ev2, raw2 = self._run_live(fixture2, 'adj-0003-process-target-error')
        self.assertEqual(proc2.returncode, 0, 'process-target error checkpoint crashed:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['scenarios'][1]['result'], 'blocked')
        self.assertRegex(record2['scenarios'][1]['reason'], r'process-target-unverified')
        # Null tri-state: a present clean_reactivation_proof key with null stays null.
        fixture3 = make_simulation_fixture()
        fixture3['scenario_events']['2'] = [
            {'offset_seconds': 1, 'text': '<DEVICE_OBSERVED_AT> UI_START|bundle=%s|requestId=a2' % BUNDLE_A},
            {'offset_seconds': 8, 'text': '<DEVICE_OBSERVED_AT> UI_STOP_SKIPPED|bundle=%s|reason=no-active-request' % BUNDLE_A},
        ]
        proc3, ev3, raw3 = self._run_live(fixture3, 'adj-0003-null-tri-state')
        self.assertEqual(proc3.returncode, 2, 'null tri-state fixture did not invalidate:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertIsNone(record3['scenario_aggregation'].get('s3_clean_reactivation_proof'),
                          'null clean_reactivation_proof was cast to false')
        # Wait-state tamper is detected as evidence integrity invalid.
        fixture4 = make_simulation_fixture()
        fixture4['tamper_wait_state_after_complete'] = True
        proc4, ev4, raw4 = self._run_live(fixture4, 'adj-0003-wait-state-tamper')
        self.assertEqual(proc4.returncode, 2, 'wait-state tamper did not exit as integrity invalid:\n%s' % (
            proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['record_status'], 'invalidated')
        self.assertIn('operator-wait-state-not-complete', record4['integrity_violations'])

    def test_real_repository_gate_and_tamper(self):
        """PS L5450-2606 real-repository-gate-and-transcript-integrity:
        dirty repo gate, transcript chain tamper, payload tamper, wait-state
        tamper, reviewed-state absence across all records."""
        # Dirty repository gate: simulation must not bypass the real clean repo.
        dirty_marker = os.path.join(self.ws['repo'], 'simulation-dirty-marker.txt')
        write_text(dirty_marker, 'dirty repository gate fixture')
        try:
            proc, ev, raw = self._run_live(make_simulation_fixture(), 'repo-dirty-before')
            self.assertNotEqual(proc.returncode, 0, 'simulation bypassed the real clean repository gate')
            self.assertFalse(os.path.exists(ev), 'dirty repo run created an evidence root')
        finally:
            if os.path.exists(dirty_marker):
                os.remove(dirty_marker)
        # Transcript chain tamper is detected as integrity invalid.
        fixture2 = make_simulation_fixture()
        fixture2['tamper_transcript_after_manifest'] = True
        proc2, ev2, raw2 = self._run_live(fixture2, 'chain-tamper')
        self.assertEqual(proc2.returncode, 2, 'transcript tamper did not exit as integrity invalid:\n%s' % (
            proc2.stdout + proc2.stderr))
        record2 = self._record(ev2)
        self.assertEqual(record2['record_status'], 'invalidated')
        self.assertEqual(record2['verdict'], 'invalid')
        self.assertIn('transcript-json-invalid', record2['integrity_violations'])
        # Payload tamper is detected as integrity invalid.
        fixture3 = make_simulation_fixture()
        fixture3['tamper_payload_after_manifest'] = True
        proc3, ev3, raw3 = self._run_live(fixture3, 'payload-tamper')
        self.assertEqual(proc3.returncode, 2, 'payload tamper did not exit as integrity invalid:\n%s' % (
            proc3.stdout + proc3.stderr))
        record3 = self._record(ev3)
        self.assertIn('transcript-payload-canonical-mismatch', record3['integrity_violations'])
        self.assertEqual(record3['record_status'], 'invalidated')
        self.assertEqual(record3['verdict'], 'invalid')
        # Baseline live simulation integrity stays empty after tamper isolation.
        proc4, ev4, raw4 = self._run_live(make_simulation_fixture(), 'baseline-after-tamper')
        self.assertEqual(proc4.returncode, 0, 'baseline live simulation failed:\n%s' % (
            proc4.stdout + proc4.stderr))
        record4 = self._record(ev4)
        self.assertEqual(record4['integrity_violations'], [])
        # No record anywhere may carry reviewed state.
        for root, _, files in os.walk(self.ws['tmp']):
            for name in files:
                if name == 'scenario-results.json':
                    text = open(os.path.join(root, name), encoding='utf-8', errors='replace').read()
                    self.assertNotRegex(text, r'reviewed-pass|reviewed-fail',
                                        'runner emitted reviewed state in %s' % os.path.join(root, name))


# =====================================================================
# Entry point
# =====================================================================

if __name__ == '__main__':
    suite = unittest.defaultTestLoader.loadTestsFromModule(sys.modules[__name__])
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    passed = result.testsRun - len(result.failures) - len(result.errors) - len(result.skipped)
    print('E3_PHYS_PREFLIGHT_SELFTEST_RESULT=%s PASSED=%d FAILED=%d SKIPPED=%d' % (
        'pass' if result.wasSuccessful() else 'fail',
        passed, len(result.failures) + len(result.errors), len(result.skipped)))
    sys.exit(0 if result.wasSuccessful() else 1)
