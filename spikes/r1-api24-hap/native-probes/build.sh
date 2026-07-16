#!/usr/bin/env bash
set -euo pipefail

: "${HARMONYOS_NATIVE_HOME:?set HARMONYOS_NATIVE_HOME to the fixed SDK native directory}"
: "${NATIVE_PROBE_OUTPUT_DIR:?set NATIVE_PROBE_OUTPUT_DIR to the generated x86_64 library directory}"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
linker="$HARMONYOS_NATIVE_HOME/llvm/bin/x86_64-unknown-linux-ohos-clang"
host_cc="${HOST_CC:-/usr/bin/gcc}"
if [[ ! -x "$linker" ]]; then
  printf 'OHOS x86_64 clang is not executable: %s\n' "$linker" >&2
  exit 1
fi
if [[ ! -x "$host_cc" ]]; then
  printf 'host x86_64 GCC is not executable: %s\n' "$host_cc" >&2
  exit 1
fi
if [[ "$NATIVE_PROBE_OUTPUT_DIR" != /* ]]; then
  printf 'NATIVE_PROBE_OUTPUT_DIR must be an absolute path: %s\n' "$NATIVE_PROBE_OUTPUT_DIR" >&2
  exit 1
fi

mkdir -p "$NATIVE_PROBE_OUTPUT_DIR"
object_dir="$NATIVE_PROBE_OUTPUT_DIR/.tls-probe-objects"
rm -rf "$object_dir"
mkdir -p "$object_dir"
trap 'rm -rf "$object_dir"' EXIT

# Keep the current HAP input pure C; historical 0006/0007 generated objects are not inputs to this probe.
rm -f "$NATIVE_PROBE_OUTPUT_DIR/libgoprobe.so" "$NATIVE_PROBE_OUTPUT_DIR/libgoprobe.h" \
  "$NATIVE_PROBE_OUTPUT_DIR/libneededprobe.so" "$NATIVE_PROBE_OUTPUT_DIR/libtlsprobe.so" \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-ie.so" "$NATIVE_PROBE_OUTPUT_DIR/libtls-gd.so" \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-desc.so" "$NATIVE_PROBE_OUTPUT_DIR/libtls-ld.so"

compile_model() {
  local object_name="$1"
  local tls_model="$2"
  local tls_dialect="$3"
  "$host_cc" -c -fPIC -O2 -fvisibility=hidden -ffreestanding -fno-stack-protector \
    -fno-asynchronous-unwind-tables -fno-unwind-tables -ftls-model="$tls_model" \
    -mtls-dialect="$tls_dialect" -o "$object_dir/$object_name.o" "$script_dir/tlsprobe.c"
}

link_model() {
  local object_name="$1"
  local library_name="$2"
  "$linker" -shared -nostdlib -Wl,--no-relax -Wl,-z,now -Wl,-soname,"$library_name" \
    -o "$NATIVE_PROBE_OUTPUT_DIR/$library_name" "$object_dir/$object_name.o"
}

compile_model tls-ie initial-exec gnu
compile_model tls-gd global-dynamic gnu
compile_model tls-desc global-dynamic gnu2
compile_model tls-ld local-dynamic gnu
link_model tls-ie libtls-ie.so
link_model tls-gd libtls-gd.so
link_model tls-desc libtls-desc.so
link_model tls-ld libtls-ld.so

printf 'built pure-C TLS inputs with linker relaxation disabled:\n'
printf '  %s\n' \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-ie.so" \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-gd.so" \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-desc.so" \
  "$NATIVE_PROBE_OUTPUT_DIR/libtls-ld.so"
