#!/usr/bin/env python3
"""G0 DryRun fake hdc (tests/fixtures/fake-hdc.py).

Executed by g0-phys-probe-campaign.py --DryRun from a private temp sandbox
under the name `fake-hdc` (never `hdc`; the host process-table probe compares
the comm column only). It answers the exact live-form argv of the 15 frozen
whitelist operations with deterministic fixture responses and keeps a JSON
state file (G0_FAKE_HDC_STATE) so the runner observes a realistic
baseline -> install -> start -> cleanup -> absent lifecycle:

    baseline    bm dump -> empty output, pidof -> empty output
    staging     mkdir / file send mutate state; bm install requires a sent hap
    runtime     aa start assigns a pid; aa force-stop clears it
    hilog       emits the pre-registered marker for G0_DRYRUN_SCRIPT:
                  pass            -> exactly one G0_RESULT verdict=PASS marker
                  dlopen-rejected -> exactly one G0_RESULT FAIL@dlopen marker
                                    (loaderError verbatim, loaderErrno=2)
                  install-fails   -> bm install answers without any 'success'
                                    text, driving the runner's sealed
                                    blocked ending (MAJOR-2 coverage)
                  (anything else  -> exit 3; the runner validates first)
    cleanup     bm uninstall clears install; rm -rf clears staging/hap
    probes      ls -ld staging errors with `No such file or directory` when
                removed (exit 1)

Any argv outside the frozen whitelist shapes is rejected (exit 5) - which
also makes the fake a second line of defense against runner whitelist bugs.

Python 3 standard library only; no network; no real device access.
"""

import json
import os
import sys

STAGING = '/data/local/tmp/netbird-g0'
BUNDLE = 'cn.alfadb.netbird.g0probe'
ABILITY = 'EntryAbility'
MODULE = 'entry'
HILOG_TAG = 'G0GoProbe'
SCRIPTS = ('pass', 'dlopen-rejected', 'install-fails')
DEFAULT_VERSION = 'G0-FAKE-HDC-1.0'


def fail(message, code):
    sys.stderr.write('fake-hdc: %s\n' % message)
    sys.exit(code)


def load_state(path):
    try:
        with open(path, 'r', encoding='utf-8-sig') as f:
            state = json.load(f)
    except (OSError, ValueError):
        state = {}
    if not isinstance(state, dict):
        state = {}
    state.setdefault('staging', False)
    state.setdefault('hap_sent', False)
    state.setdefault('installed', False)
    state.setdefault('entry_started', False)
    state.setdefault('running_pid', None)
    return state


def save_state(path, state):
    with open(path, 'w', encoding='utf-8', newline='') as f:
        f.write(json.dumps(state, sort_keys=True) + '\n')


def emit_hilog(script):
    print('2026-08-30 12:00:00.000  12345  67890 I %s: g0 probe entry aboutToAppear' % HILOG_TAG)
    if script == 'pass':
        print('2026-08-30 12:00:00.100  12345  67890 I %s: '
              'G0_RESULT|verdict=PASS|ok=true|pid=12345|stage=complete|dlopenLoaded=true|'
              'loaderErrno=0|loaderError=|hello=42|runtimeBytes=1048576' % HILOG_TAG)
    else:
        print('2026-08-30 12:00:00.100  12345  67890 E %s: '
              'G0_RESULT|verdict=FAIL|ok=false|pid=0|stage=dlopen|dlopenLoaded=false|'
              'loaderErrno=2|loaderError=initial-exec TLS resolves to dynamic definition|'
              'hello=0|runtimeBytes=0' % HILOG_TAG)


def main():
    state_path = os.environ.get('G0_FAKE_HDC_STATE')
    if not state_path:
        fail('G0_FAKE_HDC_STATE is not set', 4)
    version = os.environ.get('G0_FAKE_HDC_VERSION') or DEFAULT_VERSION
    args = list(sys.argv[1:])
    if args and args[0] == '-t':
        if len(args) < 2:
            fail('-t requires a target value', 5)
        args = args[2:]  # the fake never inspects the target value
    state = load_state(state_path)

    def persist():
        save_state(state_path, state)

    # ---- fixed-argv operations (exact tuple matches) --------------------
    if args == ['version']:
        print(version)
        return
    if args == ['shell', 'param', 'get', 'const.product.model']:
        print('PLA-AL10')
        return
    if args == ['shell', 'param', 'get', 'const.product.software.version']:
        print('PLA-AL10 7.0.0.102(SP8C00E102R7P3)')
        return
    if args == ['shell', 'mkdir', '-p', STAGING + '/hap']:
        state['staging'] = True
        persist()
        return
    if args == ['shell', 'bm', 'install', '-p', STAGING + '/hap']:
        if not state['hap_sent']:
            fail('bm install without a sent hap', 6)
        if os.environ.get('G0_DRYRUN_SCRIPT') == 'install-fails':
            # Exit 0 with no 'success' text anywhere and no installed state:
            # drives the runner's installhap-success-marker-missing branch
            # specifically (MAJOR-2 coverage of the sealed blocked ending).
            persist()
            sys.stderr.write('error: failed to install bundle\n')
            return
        state['installed'] = True
        persist()
        print('install bundle successfully')
        return
    if args == ['shell', 'hilog', '-T', HILOG_TAG, '-v', 'year', '-v', 'zone']:
        script = os.environ.get('G0_DRYRUN_SCRIPT')
        if script not in SCRIPTS:
            fail("G0_DRYRUN_SCRIPT must be one of 'pass'|'dlopen-rejected'|'install-fails'", 3)
        if not state['entry_started']:
            fail('hilog collected before the entry started', 6)
        emit_hilog(script)
        return
    if args == ['shell', 'rm', '-rf', STAGING]:
        state['staging'] = False
        state['hap_sent'] = False
        persist()
        return
    if args == ['shell', 'ls', '-ld', STAGING]:
        if state['staging']:
            print('drwxrwxr-x root root 4096 2026-08-30 12:00:00 ' + STAGING)
            return
        sys.stderr.write('ls: %s: No such file or directory\n' % STAGING)
        sys.exit(1)

    # ---- parameterized operations (exact arity, frozen values) ----------
    if len(args) == 5 and args[:4] == ['shell', 'bm', 'dump', '-n']:
        if args[4] != BUNDLE:
            fail('bm dump outside the frozen G0 bundle', 5)
        if state['installed']:
            print(json.dumps({'bundleName': BUNDLE, 'installTime': '2026-08-30 12:00:00'},
                             sort_keys=True))
        return
    if len(args) == 3 and args[:2] == ['shell', 'pidof']:
        if args[2] != BUNDLE:
            fail('pidof outside the frozen G0 bundle', 5)
        if state['running_pid']:
            print(str(state['running_pid']))
        return
    if len(args) == 4 and args[:2] == ['file', 'send'] and args[3] == STAGING + '/hap/g0.hap':
        state['staging'] = True
        state['hap_sent'] = True
        persist()
        print('FileTransfer finish')
        return
    if len(args) == 9 and args[:5] == ['shell', 'aa', 'start', '-a', ABILITY] \
            and args[5] == '-b' and args[7] == '-m':
        if args[6] != BUNDLE or args[8] != MODULE:
            fail('aa start outside the frozen G0 entry', 5)
        if not state['installed']:
            fail('aa start on a bundle that is not installed', 6)
        state['running_pid'] = 12345
        state['entry_started'] = True
        persist()
        print('start bundle successfully')
        return
    if len(args) == 10 and args[:2] == ['shell', 'find'] \
            and args[2] == '/data/log/faultlog/faultlogger' and args[3] == '-maxdepth' \
            and args[4] == '1' and args[5] == '-type' and args[6] == 'f' and args[7] == '-name' \
            and args[9] == '-print':
        if args[8] != '*%s*' % BUNDLE:
            fail('find outside the frozen G0 bundle pattern', 5)
        script = os.environ.get('G0_DRYRUN_SCRIPT')
        if script == 'dlopen-rejected' and state['entry_started']:
            print('/data/log/faultlog/faultlogger/faultlogger-0-%s-20260830120000' % BUNDLE)
        return
    if len(args) == 4 and args[:3] == ['shell', 'aa', 'force-stop']:
        if args[3] != BUNDLE:
            fail('aa force-stop outside the frozen G0 bundle', 5)
        state['running_pid'] = None
        persist()
        return
    if len(args) == 5 and args[:3] == ['shell', 'bm', 'uninstall'] and args[3] == '-n':
        if args[4] != BUNDLE:
            fail('bm uninstall outside the frozen G0 bundle', 5)
        state['installed'] = False
        state['running_pid'] = None
        persist()
        print('uninstall bundle successfully')
        return

    # Anything else (missing parameters, extra parameters, unknown operation)
    # is rejected: the fake mirrors the whitelist rejection contract.
    fail('unsupported argv: %r' % (args, ), 5)


if __name__ == '__main__':
    main()
