#!/usr/bin/env bash
# N1BDISC probe — host-only cross build + verification.
#
# Usage: bash spikes/n1b-disc-phys-hap/probe/build.sh
#
# Environment (per delegation):
#   RUSTUP_HOME=/home/worker/rust/rustup CARGO_HOME=/home/worker/rust/cargo
#   PATH=$CARGO_HOME/bin:$PATH
#   linker/CC/CXX/AR = SDK26 native llvm (aarch64-unknown-linux-ohos-clang,
#   llvm-ar) — env written below, mirroring spikes/n0-native-core/build.sh
#   (read-only reference).
#
# Steps:
#   0. toolchain presence
#   1. boringtun 0.7.1 crate sha256 == frozen value (cargo cache)
#   2. first build (generates Cargo.lock if absent)
#   3. Cargo.lock boringtun checksum == frozen value
#   4. clean + rebuild: cargo build --release --target
#      aarch64-unknown-linux-ohos --offline --locked   (must exit 0)
#   5. llvm-readelf: ELF64 / AArch64 / DYN, NEEDED has no libc.so.6
#   6. llvm-nm -D: 14/14 frozen BoringTun ffi symbols + NAPI exports
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export RUSTUP_HOME="${RUSTUP_HOME:-/home/worker/rust/rustup}"
export CARGO_HOME="${CARGO_HOME:-/home/worker/rust/cargo}"
export PATH="$CARGO_HOME/bin:$PATH"

SDK_ROOT="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current}/sdk/default/openharmony/native"
LLVM_BIN="$SDK_ROOT/llvm/bin"
TARGET=aarch64-unknown-linux-ohos
OUT="$ROOT/target/$TARGET/release/libn1bdisc_probe.so"

BORINGTUN_VERSION="0.7.1"
BORINGTUN_CRATE_SHA256="15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939"

log() { printf '[n1bdisc-probe] %s\n' "$*"; }
die() { printf '[n1bdisc-probe] ERROR: %s\n' "$*" >&2; exit 1; }

# --- 0. toolchain presence ---
[ -x "$LLVM_BIN/aarch64-unknown-linux-ohos-clang" ] || die "OHOS clang not found at $LLVM_BIN"
[ -x "$LLVM_BIN/llvm-ar" ] || die "llvm-ar not found"
[ -x "$LLVM_BIN/llvm-readelf" ] || die "llvm-readelf not found"
[ -x "$LLVM_BIN/llvm-nm" ] || die "llvm-nm not found"
rustup target list --installed | grep -qx "$TARGET" || die "rustup target $TARGET not installed"

export CC_aarch64_unknown_linux_ohos="$LLVM_BIN/aarch64-unknown-linux-ohos-clang"
export CXX_aarch64_unknown_linux_ohos="$LLVM_BIN/aarch64-unknown-linux-ohos-clang++"
export AR_aarch64_unknown_linux_ohos="$LLVM_BIN/llvm-ar"

# --- 1. boringtun crate checksum (cache) ---
CRATE_FILE="$(find "$CARGO_HOME/registry/cache" -name "boringtun-${BORINGTUN_VERSION}.crate" 2>/dev/null | head -n1)"
[ -n "$CRATE_FILE" ] || die "boringtun-${BORINGTUN_VERSION}.crate not found in cargo cache"
ACTUAL_SHA="$(sha256sum "$CRATE_FILE" | awk '{print $1}')"
[ "$ACTUAL_SHA" = "$BORINGTUN_CRATE_SHA256" ] || die "boringtun crate sha256 mismatch: got $ACTUAL_SHA want $BORINGTUN_CRATE_SHA256"
log "boringtun-${BORINGTUN_VERSION}.crate sha256 OK: $ACTUAL_SHA"

cd "$ROOT"

# --- 2. first build (Cargo.lock generation if absent) ---
if [ ! -f Cargo.lock ]; then
    log "generating Cargo.lock (first build, no --locked)"
    cargo build --release --target "$TARGET"
else
    log "Cargo.lock present"
fi

# --- 3. Cargo.lock boringtun checksum ---
LOCK_SHA="$(grep -A3 'name = "boringtun"' Cargo.lock | grep checksum | awk '{print $3}' | tr -d '"')"
[ "$LOCK_SHA" = "$BORINGTUN_CRATE_SHA256" ] || die "Cargo.lock boringtun checksum mismatch: got ${LOCK_SHA:-none} want $BORINGTUN_CRATE_SHA256"
log "Cargo.lock boringtun checksum OK: $LOCK_SHA"

# --- 4. clean rebuild: offline + locked (the authoritative build) ---
log "clean + rebuild --offline --locked"
cargo clean
cargo build --release --target "$TARGET" --offline --locked
log "cargo build --release --target $TARGET --offline --locked: exit 0"
[ -f "$OUT" ] || die "artifact missing: $OUT"

# --- 5. ELF verification ---
ELF_DESC="$("$LLVM_BIN/llvm-readelf" -h "$OUT")"
echo "$ELF_DESC" | grep -q 'Class:.*ELF64' || die "not ELF64"
echo "$ELF_DESC" | grep -q 'Machine:.*AArch64' || die "not AArch64"
echo "$ELF_DESC" | grep -q 'Type:.*DYN' || die "not DYN (shared object)"
NEEDED="$("$LLVM_BIN/llvm-readelf" -d "$OUT" || true)"
if echo "$NEEDED" | grep -q 'libc.so.6'; then
    die "NEEDED contains libc.so.6 (glibc)"
fi
echo "$NEEDED" | grep -q 'libc.so' || die "NEEDED missing libc.so"
echo "$NEEDED" | grep -q 'libhilog_ndk.z.so' || die "NEEDED missing libhilog_ndk.z.so"
log "ELF OK: ELF64/AArch64/DYN; NEEDED has libc.so + libhilog_ndk.z.so, no libc.so.6"

# --- 6. symbol verification (14 frozen ffi symbols + NAPI imports) ---
"$LLVM_BIN/llvm-nm" -D "$OUT" > "$ROOT/symbols.dyn" || die "llvm-nm failed"
MISSING=0
for sym in x25519_secret_key x25519_public_key x25519_key_to_base64 \
    x25519_key_to_hex x25519_key_to_str_free check_base64_encoded_x25519_key \
    set_logging_function new_tunnel tunnel_free wireguard_write wireguard_read \
    wireguard_tick wireguard_force_handshake wireguard_stats; do
    if ! grep -q " T ${sym}$" "$ROOT/symbols.dyn"; then
        echo "MISSING: $sym" >&2
        MISSING=1
    fi
done
[ "$MISSING" -eq 0 ] || die "frozen BoringTun ffi symbols missing from dynsym (see above)"
log "14/14 frozen BoringTun ffi symbols present in dynsym (T, defined)"

for sym in napi_module_register napi_create_function napi_get_cb_info \
    napi_create_string_utf8 napi_get_value_int32 napi_get_value_bool \
    OH_LOG_Print OH_LOG_IsLoggable; do
    grep -q " U ${sym}$" "$ROOT/symbols.dyn" || die "expected imported symbol $sym not found"
done
if grep -qE ' U (napi_create_threadsafe_function|napi_create_async_work)$' "$ROOT/symbols.dyn"; then
    die "A1 violation: threadsafe-function / async-work import found"
fi
log "import surface OK (napi/hilog from host process; A1-clean)"

# NAPI module registration constructor must be wired into .init_array
"$LLVM_BIN/llvm-readelf" -r "$OUT" > "$ROOT/rel.dyn" || die "llvm-readelf -r failed"
grep -q 'R_AARCH64_RELATIVE' "$ROOT/rel.dyn" || die "no relative relocations (init_array constructor unwired?)"
log ".init_array constructor relocation present (register_module wired)"

log "build OK: $OUT"
