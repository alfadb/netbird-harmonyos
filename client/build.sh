#!/usr/bin/env bash
# NetBird HarmonyOS client — one-command host-only build (no device, no
# signing). Promoted from spikes/n1b-disc-phys-hap/build.sh; product flavor.
#
# Usage: bash client/build.sh
#
# Chain:
#   1. core cross build      — reuses core/build.sh verbatim (frozen env:
#                              SDK26 native llvm toolchain, offline+locked
#                              cargo, ELF/symbol verification, exit0 gate)
#   2. copy artifact         — core/target/aarch64-unknown-linux-ohos/release/
#                              libnetbird_core.so
#                              -> entry/libs/arm64-v8a/libnetbird_core.so
#                              (hvigor packages module-dir libs/ into the HAP)
#   3. hvigor assembleHap    — unsigned release HAP via the command-line
#                              tools hvigorw wrapper (sets DEVECO_NODE_HOME /
#                              DEVECO_SDK_HOME itself)
#   4. verification          — HAP contains libs/arm64-v8a/libnetbird_core.so
#                              with sha256 identical to the core artifact;
#                              pack.info apiVersion target/compatible == 26;
#                              prints HAP path / size / sha256
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CLI_HOME="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current}"
HVIGORW="$CLI_HOME/bin/hvigorw"
SO_SRC="$ROOT/core/target/aarch64-unknown-linux-ohos/release/libnetbird_core.so"
LIBS_DIR="$ROOT/entry/libs/arm64-v8a"
SO_DST="$LIBS_DIR/libnetbird_core.so"
HAP="$ROOT/entry/build/default/outputs/default/entry-default-unsigned.hap"

log() { printf '[netbird-hap] %s\n' "$*"; }
die() { printf '[netbird-hap] ERROR: %s\n' "$*" >&2; exit 1; }

# --- 1. core build (authoritative clean-ish rebuild + verification, exit0 gate) ---
bash "$ROOT/core/build.sh"
[ -f "$SO_SRC" ] || die "core artifact missing: $SO_SRC"

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
# hvigor strips prebuilt .so members during HAP packaging (DoNativeStrip), so
# the member is compared against the llvm-strip'd canonical artifact — the
# same bytes (documented in the e3/n0 spike chains).
SDK_ROOT="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current}/sdk/default/openharmony/native"
STRIPPED="$(mktemp)"
cp "$SO_DST" "$STRIPPED"
"$SDK_ROOT/llvm/bin/llvm-strip" "$STRIPPED"
python3 - "$HAP" "$STRIPPED" <<'PYEOF'
import hashlib, json, struct, sys, zipfile

hap_path, so_ref = sys.argv[1], sys.argv[2]
MEMBER = 'libs/arm64-v8a/libnetbird_core.so'

def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

want = sha256(open(so_ref, 'rb').read())
with zipfile.ZipFile(hap_path) as z:
    names = z.namelist()
    if MEMBER not in names:
        print(f'FAIL: {MEMBER} not in HAP; members={names}', file=sys.stderr)
        sys.exit(1)
    got = sha256(z.read(MEMBER))
    if got != want:
        print(f'FAIL: sha256 mismatch HAP={got} stripped={want}', file=sys.stderr)
        sys.exit(1)
    # ELF sanity on the member: ELF64 little-endian, ET_DYN, AArch64 machine
    member = z.read(MEMBER)
    if member[:4] != b'\x7fELF' or member[4] != 2 or member[5] != 1:
        print('FAIL: member is not ELF64 little-endian', file=sys.stderr)
        sys.exit(1)
    etype, machine = struct.unpack_from('<HH', member, 16)
    if etype != 3 or machine != 183:
        print(f'FAIL: member e_type={etype} e_machine={machine}, want ET_DYN/AArch64', file=sys.stderr)
        sys.exit(1)
    pack = json.loads(z.read('pack.info').decode('utf-8'))
    mod = pack['summary']['modules'][0]
    api = mod.get('apiVersion', {})
    if api.get('target') != 26 or api.get('compatible') != 26:
        print(f'FAIL: pack.info apiVersion != 26: {api}', file=sys.stderr)
        sys.exit(1)

print(f'VERIFY OK: {MEMBER} present, sha256 identical to llvm-strip\'d core artifact')
print(f'VERIFY OK: sha256={got}')
print('VERIFY OK: member is ELF64 ET_DYN AArch64')
print(f"VERIFY OK: pack.info apiVersion target={api.get('target')} compatible={api.get('compatible')} releaseType={api.get('releaseType')}")
PYEOF
rm -f "$STRIPPED"

log "HAP: $HAP"
log "size: $(stat -c%s "$HAP") bytes"
log "sha256: $(sha256sum "$HAP" | awk '{print $1}')"
log "build OK"
