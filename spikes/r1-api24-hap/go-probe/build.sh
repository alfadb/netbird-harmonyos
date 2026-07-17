#!/usr/bin/env bash
set -euo pipefail

: "${HARMONYOS_NATIVE_HOME:?set HARMONYOS_NATIVE_HOME to the fixed SDK native directory}"
: "${GO_BIN:?set GO_BIN to the Go launcher path}"
: "${GO_PROBE_OUTPUT_DIR:?set GO_PROBE_OUTPUT_DIR to the generated x86_64 library directory}"

GO_TOOLCHAIN_MODE="${GO_TOOLCHAIN_MODE:-local}"
GO_EXPECTED_VERSION_PREFIX="${GO_EXPECTED_VERSION_PREFIX:-go version go1.25.12 }"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
compiler="$HARMONYOS_NATIVE_HOME/llvm/bin/x86_64-unknown-linux-ohos-clang"
if [[ ! -x "$GO_BIN" ]]; then
  printf 'Go launcher is not executable: %s\n' "$GO_BIN" >&2
  exit 1
fi
if [[ ! -x "$compiler" ]]; then
  printf 'OHOS x86_64 clang is not executable: %s\n' "$compiler" >&2
  exit 1
fi
if [[ "$GO_PROBE_OUTPUT_DIR" != /* ]]; then
  printf 'GO_PROBE_OUTPUT_DIR must be an absolute path: %s\n' "$GO_PROBE_OUTPUT_DIR" >&2
  exit 1
fi

go_version="$(GOTOOLCHAIN="$GO_TOOLCHAIN_MODE" "$GO_BIN" version)"
case "$go_version" in
  "$GO_EXPECTED_VERSION_PREFIX"*) ;;
  *)
    printf 'expected Go version prefix %s, got: %s\n' "$GO_EXPECTED_VERSION_PREFIX" "$go_version" >&2
    exit 1
    ;;
esac

mkdir -p "$GO_PROBE_OUTPUT_DIR"
rm -f "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" "$GO_PROBE_OUTPUT_DIR/libgoprobe.h"

GOTOOLCHAIN="$GO_TOOLCHAIN_MODE" \
GOOS=linux \
GOARCH=amd64 \
GOAMD64=v1 \
CGO_ENABLED=1 \
CC="$script_dir/ohos-x86_64-clang" \
"$GO_BIN" -C "$script_dir" build -trimpath -buildmode=c-shared -o "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" .

printf 'built %s with %s\n' "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" "$go_version"
