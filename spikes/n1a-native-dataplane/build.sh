#!/usr/bin/env bash
# N1a gate data-plane probe host build: dual-ABI Rust core + C++ NAPI overlay.
#
# Scope (docs/n1a-gate-plan.md; implementation spike spikes/n1a-native-dataplane):
#   - fixed BoringTun 0.7.1, default-features = false, features = ["ffi-bindings"];
#   - NO `device` feature / socket2 patch; NO management/ICE/relay/UI/VPN/TUN/protect;
#   - x86_64: full build + ELF verification (the Emulator gate ABI); the
#     artifacts are the drop-in for the formal campaign's HAP snapshot;
#   - aarch64: cross-compile only, built alongside exactly like N0; NO load claim.
#
# Uses the official OHOS clang/sysroot for both the Rust linker and the C++
# overlay. Cargo.lock is authoritative (--locked) and the build is fully
# offline (--offline): the BoringTun crate checksum is verified against the
# cargo cache and Cargo.lock, and no network access is used during the build.
#
# This script builds and verifies artifacts only. It does NOT run any
# Emulator/HDC operation and produces no evidence files; the formal N1a
# Emulator measurement is a separate campaign step (not part of this spike).
#
# Usage: bash spikes/n1a-native-dataplane/build.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SDK_ROOT="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current/sdk}/default/openharmony/native"
LLVM_BIN="$SDK_ROOT/llvm/bin"
SYSROOT="$SDK_ROOT/sysroot"
OUT="$ROOT/out"
TARGET_DIR="$ROOT/target"

BORINGTUN_VERSION="0.7.1"
BORINGTUN_CRATE_SHA256="15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939"

log() { printf '[n1a-build] %s\n' "$*"; }
die() { printf '[n1a-build] ERROR: %s\n' "$*" >&2; exit 1; }

# --- 0. toolchain presence ---
[ -x "$LLVM_BIN/x86_64-unknown-linux-ohos-clang" ] || die "OHOS clang not found at $LLVM_BIN"
[ -d "$SYSROOT/usr/include/napi" ] || die "OHOS sysroot napi headers not found at $SYSROOT"
rustup target list --installed | grep -qx "x86_64-unknown-linux-ohos" || die "rustup target x86_64-unknown-linux-ohos not installed"
rustup target list --installed | grep -qx "aarch64-unknown-linux-ohos" || die "rustup target aarch64-unknown-linux-ohos not installed"

# --- 1. BoringTun crate checksum + Cargo.lock verification ---
CRATE_FILE="$(find "${CARGO_HOME:-$HOME/.cargo}/registry/cache" -name "boringtun-${BORINGTUN_VERSION}.crate" 2>/dev/null | head -n1)"
[ -n "$CRATE_FILE" ] || die "boringtun-${BORINGTUN_VERSION}.crate not found in cargo cache"
ACTUAL_SHA="$(sha256sum "$CRATE_FILE" | awk '{print $1}')"
[ "$ACTUAL_SHA" = "$BORINGTUN_CRATE_SHA256" ] || die "boringtun crate checksum mismatch: got $ACTUAL_SHA want $BORINGTUN_CRATE_SHA256"
log "boringtun-${BORINGTUN_VERSION}.crate sha256 OK: $ACTUAL_SHA"

LOCK_SHA="$(grep -A3 'name = "boringtun"' "$ROOT/Cargo.lock" | grep checksum | awk '{print $3}' | tr -d '"')"
[ "$LOCK_SHA" = "$BORINGTUN_CRATE_SHA256" ] || die "Cargo.lock boringtun checksum mismatch: got $LOCK_SHA"
log "Cargo.lock boringtun checksum OK: $LOCK_SHA"

# --- 1b. STATIC_NO_PTHREAD (frozen r3 C7(2)) ------------------------------
# The probe must never create threads: the four forbidden threading APIs
# must have ZERO occurrences in the probe's source tree (src/ + napi/).
# build.sh itself is excluded because this very pattern would self-match;
# README.md is excluded because it documents the criteria text (which quotes
# the API names). Note: std::thread::sleep is NOT thread creation and is
# allowed (C6's quiet gaps use it); the grep tokens below only match the
# creation APIs.
FORBIDDEN_COUNT="$( { grep -rEc 'pthread_create|std::thread::spawn|napi_create_threadsafe_function|napi_create_async_work' \
    "$ROOT/src" "$ROOT/napi" || true; } | awk -F: '{sum += $NF} END {print sum+0}')"
[ "$FORBIDDEN_COUNT" -eq 0 ] \
    || die "STATIC_NO_PTHREAD violation: $FORBIDDEN_COUNT occurrences of forbidden threading APIs in src/ + napi/"
log "STATIC_NO_PTHREAD OK: 0 forbidden threading-API occurrences in src/ + napi/"

# --- 1c. THROW_SNAPSHOT (defect #3 of EV-N1A-EMU24-20260831-0001) ---------
# Every ThrowError error path in the overlay MUST be preceded (within the
# enclosing block) by a DiagAndThrow call that emits the diagnostic
# snapshot. This assertion counts DiagAndThrow call sites and rejects any
# bare `return ThrowError(` that is NOT inside the DiagAndThrow wrapper
# itself (the wrapper's single `return ThrowError(env, msg);` line is the
# only permitted bare call).
OVERLAY="$ROOT/napi/n1a_overlay.cpp"
DIAG_CALLS="$(grep -c 'DiagAndThrow(env' "$OVERLAY" || true)"
BARE_THROWS="$(grep -c 'return ThrowError(env,' "$OVERLAY" || true)"
# The wrapper itself has exactly 1 bare `return ThrowError(env, msg);`.
[ "$BARE_THROWS" -le 1 ] \
    || die "THROW_SNAPSHOT violation: $BARE_THROWS bare ThrowError returns (expected <=1, the DiagAndThrow wrapper); every error path must go through DiagAndThrow"
[ "$DIAG_CALLS" -ge 1 ] \
    || die "THROW_SNAPSHOT violation: 0 DiagAndThrow call sites found in overlay"
log "THROW_SNAPSHOT OK: $DIAG_CALLS DiagAndThrow call sites, $BARE_THROWS bare ThrowError (wrapper only)"

# --- 2. Rust core dual-ABI build (--locked: Cargo.lock is authoritative) ---
# ring 0.17's build script compiles assembly via the `cc` crate; point it at the
# official OHOS clang so the objects match the target (same as N0).
# The `cc` crate also archives static libs: point AR at the official OHOS
# llvm-ar for BOTH targets so the archives are produced by the same toolchain.
export CC_x86_64_unknown_linux_ohos="$LLVM_BIN/x86_64-unknown-linux-ohos-clang"
export CXX_x86_64_unknown_linux_ohos="$LLVM_BIN/x86_64-unknown-linux-ohos-clang++"
export AR_x86_64_unknown_linux_ohos="$LLVM_BIN/llvm-ar"
export CC_aarch64_unknown_linux_ohos="$LLVM_BIN/aarch64-unknown-linux-ohos-clang"
export CXX_aarch64_unknown_linux_ohos="$LLVM_BIN/aarch64-unknown-linux-ohos-clang++"
export AR_aarch64_unknown_linux_ohos="$LLVM_BIN/llvm-ar"
[ -x "$LLVM_BIN/llvm-ar" ] || die "OHOS llvm-ar not found at $LLVM_BIN/llvm-ar"
cd "$ROOT"
for target in x86_64-unknown-linux-ohos aarch64-unknown-linux-ohos; do
    log "cargo build --release --target $target (linker=$LLVM_BIN/${target}-clang, ar=$LLVM_BIN/llvm-ar)"
    RUSTFLAGS="-Clinker=$LLVM_BIN/${target}-clang" \
        CARGO_TARGET_DIR="$TARGET_DIR" \
        cargo build --release --target "$target" --offline --locked
done

# --- 3. C++ NAPI overlay dual-ABI build (official OHOS clang/sysroot) ---
mkdir -p "$OUT/x86_64" "$OUT/aarch64" "$OUT/hap-snapshot/x86_64"

build_overlay() {
    local rust_target="$1" sysroot_lib="$2" out_dir="$3"
    log "clang++ overlay -> $out_dir/libentry.so ($rust_target)"
    "$LLVM_BIN/${rust_target}-clang++" -shared -fPIC -std=c++17 -O2 \
        -o "$out_dir/libentry.so" \
        "$ROOT/napi/n1a_overlay.cpp" \
        -I"$SYSROOT/usr/include" \
        -L"$SYSROOT/usr/lib/$sysroot_lib" \
        -Wl,--whole-archive "$TARGET_DIR/$rust_target/release/libn1acore.a" -Wl,--no-whole-archive \
        -lace_napi.z -lhilog_ndk.z
    # Strip debug info so the HAP-packaged member is byte-identical to this
    # artifact (same rationale as N0: hvigor strips prebuilt .so members
    # during HAP packaging; the dynamic symbol table (nm -D) and DT_NEEDED
    # survive).
    "$LLVM_BIN/llvm-strip" "$out_dir/libentry.so"
}

build_overlay x86_64-unknown-linux-ohos x86_64-linux-ohos "$OUT/x86_64"
build_overlay aarch64-unknown-linux-ohos aarch64-linux-ohos "$OUT/aarch64"

# --- 4. HAP snapshot artifacts (x86_64 only; arm64 is cross-compile only) ---
cp "$OUT/x86_64/libentry.so" "$OUT/hap-snapshot/x86_64/libentry.so"
cp "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn1acore.a" "$OUT/hap-snapshot/x86_64/libn1acore.a"
cp "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn1acore.so" "$OUT/hap-snapshot/x86_64/libn1acore.so"

# --- 5. ELF verification (x86_64 + aarch64; NEEDED must not contain libc.so.6) ---
# readelf output is captured FIRST and checked with bash string matching: a
# `readelf | grep -q` pipeline under `set -o pipefail` would fail spuriously
# (grep -q exits early on a match and SIGPIPEs readelf).
verify_elf() {
    local file="$1" expect_arch="$2"
    local desc needed
    desc="$(file "$file")"
    case "$desc" in
        *"ELF 64-bit LSB shared object"*) ;;
        *) die "not an ELF shared object: $file ($desc)" ;;
    esac
    case "$desc" in
        *"$expect_arch"*) ;;
        *) die "unexpected architecture for $file: $desc" ;;
    esac
    needed="$(readelf -d "$file" || true)"
    if [[ "$needed" == *"libc.so.6"* ]]; then
        die "NEEDED contains libc.so.6 (glibc) in $file"
    fi
    [[ "$needed" == *"libc.so"* ]] || die "NEEDED missing libc.so in $file"
    log "ELF OK: $file ($expect_arch, NEEDED has no libc.so.6)"
}

verify_elf "$OUT/x86_64/libentry.so" "x86-64"
verify_elf "$OUT/aarch64/libentry.so" "ARM aarch64"
verify_elf "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn1acore.so" "x86-64"
verify_elf "$TARGET_DIR/aarch64-unknown-linux-ohos/release/libn1acore.so" "ARM aarch64"

# --- 6. exported symbol verification (n1a_* C ABI + the real BoringTun C ABI) ---
# Note: avoid `nm | grep -q` under `set -o pipefail` (grep -q exits early and
# SIGPIPE makes the pipeline fail); dump symbols to a temp file instead.
SYMS_TMP="$(mktemp)"
trap 'rm -f "$SYMS_TMP"' EXIT
for f in "$OUT/x86_64/libentry.so" "$OUT/aarch64/libentry.so"; do
    nm -D --defined-only "$f" > "$SYMS_TMP"
    for sym in n1a_probe_version n1a_dataplane_probe n1a_result_free \
        x25519_secret_key x25519_public_key x25519_key_to_base64 \
        check_base64_encoded_x25519_key new_tunnel tunnel_free \
        wireguard_write wireguard_read wireguard_tick \
        wireguard_force_handshake wireguard_stats; do
        grep -q " T ${sym}$" "$SYMS_TMP" || die "missing exported symbol $sym in $f"
    done
    log "symbols OK: $f (n1a_* C ABI + full BoringTun data-plane C ABI)"
done
rm -f "$SYMS_TMP"
trap - EXIT

# --- 7. checksum manifest of the build products (identity for the campaign) ---
log "artifact sha256:"
for f in "$OUT/x86_64/libentry.so" "$OUT/aarch64/libentry.so" \
    "$OUT/hap-snapshot/x86_64/libentry.so" "$OUT/hap-snapshot/x86_64/libn1acore.a" \
    "$OUT/hap-snapshot/x86_64/libn1acore.so"; do
    h="$(sha256sum "$f" | awk '{print $1}')"
    log "  $h  $f"
done
[ "$(sha256sum "$OUT/x86_64/libentry.so" | awk '{print $1}')" = "$(sha256sum "$OUT/hap-snapshot/x86_64/libentry.so" | awk '{print $1}')" ] \
    || die "hap-snapshot libentry.so is not byte-identical to out/x86_64/libentry.so"
log "hap-snapshot identity OK (byte-identical)"

# --- 8. summary ---
log "build OK"
log "artifacts:"
log "  $OUT/x86_64/libentry.so        (x86_64 overlay, Emulator gate ABI)"
log "  $OUT/aarch64/libentry.so       (aarch64 overlay, cross-compile only)"
log "  $OUT/hap-snapshot/x86_64/      (libentry.so + libn1acore.a + libn1acore.so)"
