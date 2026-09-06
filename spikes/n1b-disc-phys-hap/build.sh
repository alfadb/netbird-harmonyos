#!/usr/bin/env bash
# N1BDISC physical HAP — one-command host-only build (no device, no signing).
#
# Usage: bash spikes/n1b-disc-phys-hap/build.sh
#
# Chain:
#   1. probe cross build      — reuses probe/build.sh verbatim (frozen env:
#                               SDK26 native llvm toolchain, offline+locked
#                               cargo, ELF/symbol verification, exit0 gate)
#   2. copy artifact          — probe/target/.../libn1bdisc_probe.so
#                               -> entry/libs/arm64-v8a/libn1bdisc_probe.so
#                               (hvigor packages module-dir libs/ into the HAP)
#   3. hvigor assembleHap     — unsigned release HAP via the command-line
#                               tools hvigorw wrapper (sets DEVECO_NODE_HOME /
#                               DEVECO_SDK_HOME itself)
#   4. verification           — HAP contains libs/arm64-v8a/libn1bdisc_probe.so
#                               with sha256 identical to the probe artifact;
#                               pack.info apiVersion target/compatible == 26;
#                               prints HAP path / size / sha256
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_HOME="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current}"
HVIGORW="$CLI_HOME/bin/hvigorw"
SO_SRC="$ROOT/probe/target/aarch64-unknown-linux-ohos/release/libn1bdisc_probe.so"
LIBS_DIR="$ROOT/entry/libs/arm64-v8a"
SO_DST="$LIBS_DIR/libn1bdisc_probe.so"
HAP="$ROOT/entry/build/default/outputs/default/entry-default-unsigned.hap"

log() { printf '[n1bdisc-hap] %s\n' "$*"; }
die() { printf '[n1bdisc-hap] ERROR: %s\n' "$*" >&2; exit 1; }

# --- 1. probe build (authoritative clean rebuild + verification, exit0 gate) ---
bash "$ROOT/probe/build.sh"
[ -f "$SO_SRC" ] || die "probe artifact missing: $SO_SRC"

# --- 2. copy .so into the module libs dir ---
mkdir -p "$LIBS_DIR"
cp "$SO_SRC" "$SO_DST"
log "copied .so -> $SO_DST ($(sha256sum "$SO_DST" | awk '{print $1}'))"

# --- 3. assembleHap (unsigned) ---
[ -x "$HVIGORW" ] || die "hvigorw not found/executable: $HVIGORW"
cd "$ROOT"
log "hvigorw assembleHap (release, unsigned)"
"$HVIGORW" --no-daemon assembleHap --mode module -p product=default -p buildMode=release
log "assembleHap exit 0"
[ -f "$HAP" ] || die "HAP not found: $HAP"

# --- 4. verification: .so member + sha256 identity + pack.info apiVersion ---
python3 - "$HAP" "$SO_SRC" <<'PYEOF'
import hashlib, json, stat, sys, zipfile

hap_path, so_src = sys.argv[1], sys.argv[2]
MEMBER = 'libs/arm64-v8a/libn1bdisc_probe.so'

def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

want = sha256(open(so_src, 'rb').read())
with zipfile.ZipFile(hap_path) as z:
    names = z.namelist()
    if MEMBER not in names:
        print(f'FAIL: {MEMBER} not in HAP; members={names}', file=sys.stderr)
        sys.exit(1)
    got = sha256(z.read(MEMBER))
    if got != want:
        print(f'FAIL: sha256 mismatch HAP={got} probe={want}', file=sys.stderr)
        sys.exit(1)
    pack = json.loads(z.read('pack.info').decode('utf-8'))
    mod = pack['summary']['modules'][0]
    api = mod.get('apiVersion', {})
    if api.get('target') != 26 or api.get('compatible') != 26:
        print(f'FAIL: pack.info apiVersion != 26: {api}', file=sys.stderr)
        sys.exit(1)

print(f'VERIFY OK: {MEMBER} present, sha256 identical to probe artifact')
print(f'VERIFY OK: sha256={got}')
print(f"VERIFY OK: pack.info apiVersion target={api.get('target')} compatible={api.get('compatible')} releaseType={api.get('releaseType')}")
PYEOF

log "HAP: $HAP"
log "size: $(stat -c%s "$HAP") bytes"
log "sha256: $(sha256sum "$HAP" | awk '{print $1}')"
log "build OK"
