#!/usr/bin/env bash
# N1a HAP snapshot preparation (host-only).
#
# Stages the N1a overlay into a temporary copy of the fixed r1-api24-hap
# snapshot so n1a-emulator-run.sh can build the app + test HAPs from it.
# This script does NOT build a HAP, start an Emulator, or run HDC.
#
# The snapshot commit is PINNED (never dynamic HEAD): the r1-api24-hap path is
# extracted from commit 2c567dc721c6582f93a15b241e843e3bbff3f7f3 via git
# archive, so the HAP build is reproducible regardless of later repository
# state (mirror of the N0 prepare-hap-snapshot.sh mechanism).
#
# Staged content (over the r1 snapshot copy only; the repository tree is
# never touched):
#   - entry/src/main/cpp/n1a_overlay.cpp            NAPI overlay source
#   - entry/src/main/cpp/types/libentry/*           type declarations
#   - entry/src/main/ets/pages/runN1aProbeTest.ets  ohosTest assertion module
#   - entry/src/main/ets/pages/Index.ets            C9 visible result page
#                                                     (replaces the staged r1
#                                                     Index in the stage copy)
#   - entry/src/ohosTest/ets/testrunner/...          N1a ohosTest runner
#   - entry/libs/x86_64/libentry.so                 prebuilt native member
#   - entry/oh-package(.lock).json5, build-profile  libentry.so dependency,
#                                                     x86_64-only abiFilters
#
# Usage: bash spikes/n1a-native-dataplane/prepare-hap-snapshot.sh [STAGE_DIR]
#   STAGE_DIR defaults to /tmp/n1a-hap-snapshot
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$ROOT/../.." && pwd)"
SNAPSHOT_DIR="$REPO_ROOT/spikes/r1-api24-hap"
STAGE="${1:-/tmp/n1a-hap-snapshot}"

# Pinned r1-api24-hap snapshot commit (fixed; never dynamic HEAD; same pin as
# the N0 runner).
SNAPSHOT_HEAD="2c567dc721c6582f93a15b241e843e3bbff3f7f3"

die() { printf '[n1a-prep] ERROR: %s\n' "$*" >&2; exit 1; }
log() { printf '[n1a-prep] %s\n' "$*"; }

# The pinned commit must exist in this repository and must contain the r1 path.
git -C "$REPO_ROOT" cat-file -e "$SNAPSHOT_HEAD^{commit}" || \
    die "pinned snapshot commit $SNAPSHOT_HEAD not found in repository"
git -C "$REPO_ROOT" ls-tree "$SNAPSHOT_HEAD" spikes/r1-api24-hap >/dev/null || \
    die "pinned snapshot commit $SNAPSHOT_HEAD has no spikes/r1-api24-hap path"

[ -d "$SNAPSHOT_DIR" ] || die "r1-api24-hap snapshot not found at $SNAPSHOT_DIR"
[ -f "$ROOT/out/hap-snapshot/x86_64/libentry.so" ] || \
    die "run build.sh first (missing $ROOT/out/hap-snapshot/x86_64/libentry.so)"
[ -f "$ROOT/napi/pages/Index.ets" ] || \
    die "missing C9 result page source: $ROOT/napi/pages/Index.ets"

rm -rf "$STAGE"
mkdir -p "$STAGE"
git -C "$REPO_ROOT" archive "$SNAPSHOT_HEAD" spikes/r1-api24-hap | tar -x -C "$STAGE" --strip-components=2
log "snapshot staged from pinned commit $SNAPSHOT_HEAD -> $STAGE"

# Overlay sources.
cp "$ROOT/napi/n1a_overlay.cpp" "$STAGE/entry/src/main/cpp/n1a_overlay.cpp"
mkdir -p "$STAGE/entry/src/main/cpp/types/libentry"
cp "$ROOT/napi/types/index.d.ts" "$STAGE/entry/src/main/cpp/types/libentry/index.d.ts"
cp "$ROOT/napi/types/oh-package.json5" "$STAGE/entry/src/main/cpp/types/libentry/oh-package.json5"
cp "$ROOT/napi/runN1aProbeTest.ets" "$STAGE/entry/src/main/ets/pages/runN1aProbeTest.ets"

# C9 visible result page: replaces the staged r1 Index in the stage COPY only.
# The ordinary EntryAbility cold start then runs the real probe once and
# renders the verdict from the same probe result object that produced the
# N1A_RESULT marker (page/marker consistency by construction, E2 precedent).
cp "$ROOT/napi/pages/Index.ets" "$STAGE/entry/src/main/ets/pages/Index.ets"
log "pages/Index.ets: staged the N1a C9 result page over the r1 page (stage copy only)"

# N1a ohosTest test runner: the `aa test` Hypium runner must actually call
# runN1aProbeTest() (full overwrite of the r1 testrunner in the stage copy).
mkdir -p "$STAGE/entry/src/ohosTest/ets/testrunner"
cp "$ROOT/napi/ohosTest/OpenHarmonyTestRunner.ets" \
    "$STAGE/entry/src/ohosTest/ets/testrunner/OpenHarmonyTestRunner.ets"
log "ohosTest testrunner: N1a runner calls runN1aProbeTest()"

# x86_64 native member (arm64 is cross-compile only; not staged for loading).
mkdir -p "$STAGE/entry/libs/x86_64"
cp "$ROOT/out/hap-snapshot/x86_64/libentry.so" "$STAGE/entry/libs/x86_64/libentry.so"

# Declare the libentry.so types dependency in the entry module (full
# overwrite; the r1 snapshot at the pinned commit has exactly this content
# plus the two r1 deps).
cat > "$STAGE/entry/oh-package.json5" <<'JSON5'
{
  modelVersion: '6.0.0',
  name: 'entry',
  version: '1.0.0',
  description: 'Entry module for the short-lived R1 API 24 probe.',
  license: 'MIT',
  dependencies: {
    'libprobe.so': 'file:./src/main/cpp/types/libprobe',
    'libe2network.so': 'file:./src/main/cpp/types/libe2network',
    'libentry.so': 'file:./src/main/cpp/types/libentry'
  }
}
JSON5
log "entry/oh-package.json5: added libentry.so types dependency"

# Keep the per-module ohpm lock in sync with the added libentry.so dependency
# (full overwrite; same content as the pinned snapshot plus the libentry
# entry).
cat > "$STAGE/entry/oh-package-lock.json5" <<'JSON5'
{
  "meta": {
    "stableOrder": true,
    "enableUnifiedLockfile": false
  },
  "lockfileVersion": 3,
  "ATTENTION": "THIS IS AN AUTOGENERATED FILE. DO NOT EDIT THIS FILE DIRECTLY.",
  "specifiers": {
    "libe2network.so@src/main/cpp/types/libe2network": "libe2network.so@src/main/cpp/types/libe2network",
    "libentry.so@src/main/cpp/types/libentry": "libentry.so@src/main/cpp/types/libentry",
    "libprobe.so@src/main/cpp/types/libprobe": "libprobe.so@src/main/cpp/types/libprobe"
  },
  "packages": {
    "libe2network.so@src/main/cpp/types/libe2network": {
      "name": "libe2network.so",
      "version": "1.0.0",
      "resolved": "src/main/cpp/types/libe2network",
      "registryType": "local"
    },
    "libentry.so@src/main/cpp/types/libentry": {
      "name": "libentry.so",
      "version": "1.0.0",
      "resolved": "src/main/cpp/types/libentry",
      "registryType": "local"
    },
    "libprobe.so@src/main/cpp/types/libprobe": {
      "name": "libprobe.so",
      "version": "1.0.0",
      "resolved": "src/main/cpp/types/libprobe",
      "registryType": "local"
    }
  }
}
JSON5
log "entry/oh-package-lock.json5: added libentry.so lock entry"

# Restrict the HAP to x86_64 only: the N1a HAP must not package arm64
# artifacts (arm64 is cross-compile only, no load claim). Full overwrite of
# the r1 entry/build-profile.json5 in the stage copy; only abiFilters change.
cat > "$STAGE/entry/build-profile.json5" <<'JSON5'
{
  apiType: 'stageMode',
  buildOption: {
    externalNativeOptions: {
      path: './src/main/cpp/CMakeLists.txt',
      abiFilters: [
        'x86_64'
      ],
      targets: [
        'probe',
        'e2network'
      ]
    }
  },
  targets: [
    {
      name: 'default',
      runtimeOS: 'HarmonyOS',
      config: {
        deviceType: [
          'phone'
        ]
      }
    },
    {
      name: 'ohosTest',
      runtimeOS: 'HarmonyOS',
      config: {
        deviceType: [
          'phone'
        ]
      }
    }
  ]
}
JSON5
log "entry/build-profile.json5: abiFilters restricted to x86_64 (no arm64 in HAP)"

log "staged. Next step (n1a-emulator-run.sh) builds the HAPs from $STAGE, e.g.:"
log "  cd $STAGE"
log "  /home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw clean --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon"
log "  /home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap --mode module -p product=default -p module=entry@default -p buildMode=debug --no-daemon"
log "  /home/worker/harmonyos/command-line-tools/6.1.1.290/bin/hvigorw assembleHap --mode module -p product=default -p module=entry@ohosTest -p buildMode=debug --no-daemon"