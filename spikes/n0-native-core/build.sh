#!/usr/bin/env bash
# N0 native core host build: dual-ABI Rust core + C++ NAPI overlay.
#
# Scope (docs/n0-native-client-feasibility.md, N0(b)):
#   - fixed BoringTun 0.7.1, default-features = false, features = ["ffi-bindings"];
#   - NO `device` feature / socket2 patch; NO management/ICE/relay/UI/VPN/TUN/protect;
#   - x86_64: full build + ELF verification; HAP snapshot artifact libentry.so;
#   - aarch64: cross-compile only; NO load claim.
#
# Uses the official OHOS clang/sysroot for both the Rust linker and the C++
# overlay. Cargo.lock is authoritative (--locked) and the build is fully
# offline (--offline): the BoringTun crate checksum is verified against the
# cargo cache and Cargo.lock, and no network access is used during the build
# (the N0 Emulator runner depends on this offline guarantee).
#
# Usage: bash spikes/n0-native-core/build.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SDK_ROOT="${DEVECO_SDK_HOME:-/home/worker/harmonyos/command-line-tools/current/sdk}/default/openharmony/native"
LLVM_BIN="$SDK_ROOT/llvm/bin"
SYSROOT="$SDK_ROOT/sysroot"
OUT="$ROOT/out"
TARGET_DIR="$ROOT/target"

BORINGTUN_VERSION="0.7.1"
BORINGTUN_CRATE_SHA256="15dd6a8a89cbe8997f37ca0cf035e6ea4d64cd2ecea4aed83ffb9f99f7126939"

log() { printf '[n0-build] %s\n' "$*"; }
die() { printf '[n0-build] ERROR: %s\n' "$*" >&2; exit 1; }

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

# --- 2. Rust core dual-ABI build (--locked: Cargo.lock is authoritative) ---
# ring 0.17's build script compiles assembly via the `cc` crate; point it at the
# official OHOS clang so the objects match the target (same as the host preflight).
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
        "$ROOT/napi/n0_overlay.cpp" \
        -I"$SYSROOT/usr/include" \
        -L"$SYSROOT/usr/lib/$sysroot_lib" \
        -Wl,--whole-archive "$TARGET_DIR/$rust_target/release/libn0core.a" -Wl,--no-whole-archive \
        -lace_napi.z -lhilog_ndk.z
    # Strip debug info so the HAP-packaged member is byte-identical to this
    # artifact: hvigor strips prebuilt .so members during HAP packaging, and
    # the OHOS llvm-strip produces the same bytes (verified against the actual
    # HAP member). The dynamic symbol table (nm -D) and DT_NEEDED survive.
    "$LLVM_BIN/llvm-strip" "$out_dir/libentry.so"
}

build_overlay x86_64-unknown-linux-ohos x86_64-linux-ohos "$OUT/x86_64"
build_overlay aarch64-unknown-linux-ohos aarch64-linux-ohos "$OUT/aarch64"

# --- 4. HAP snapshot artifacts (x86_64 only; arm64 is cross-compile only) ---
cp "$OUT/x86_64/libentry.so" "$OUT/hap-snapshot/x86_64/libentry.so"
cp "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn0core.a" "$OUT/hap-snapshot/x86_64/libn0core.a"
cp "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn0core.so" "$OUT/hap-snapshot/x86_64/libn0core.so"

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
verify_elf "$TARGET_DIR/x86_64-unknown-linux-ohos/release/libn0core.so" "x86-64"
verify_elf "$TARGET_DIR/aarch64-unknown-linux-ohos/release/libn0core.so" "ARM aarch64"

# --- 6. exported symbol verification (n0_probe_* + real BoringTun C ABI) ---
# Note: avoid `nm | grep -q` under `set -o pipefail` (grep -q exits early and
# SIGPIPE makes the pipeline fail); dump symbols to a temp file instead.
SYMS_TMP="$(mktemp)"
trap 'rm -f "$SYMS_TMP"' EXIT
for f in "$OUT/x86_64/libentry.so" "$OUT/aarch64/libentry.so"; do
    nm -D --defined-only "$f" > "$SYMS_TMP"
    for sym in n0_probe_version n0_probe_smoke x25519_secret_key x25519_public_key \
        x25519_key_to_base64 check_base64_encoded_x25519_key new_tunnel wireguard_tick \
        tunnel_free; do
        grep -q " T ${sym}$" "$SYMS_TMP" || die "missing exported symbol $sym in $f"
    done
    log "symbols OK: $f (n0_probe_* + BoringTun C ABI)"
done
rm -f "$SYMS_TMP"
trap - EXIT

# --- 7. summary ---
log "build OK"
log "artifacts:"
log "  $OUT/x86_64/libentry.so        (x86_64 overlay, HAP snapshot drop-in)"
log "  $OUT/aarch64/libentry.so       (aarch64 overlay, cross-compile only)"
log "  $OUT/hap-snapshot/x86_64/      (libentry.so + libn0core.a + libn0core.so)"
