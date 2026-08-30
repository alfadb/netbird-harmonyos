#!/usr/bin/env bash
set -euo pipefail

export HARMONYOS_NATIVE_HOME="${HARMONYOS_NATIVE_HOME:-/home/worker/harmonyos/command-line-tools/6.1.1.290/sdk/default/openharmony/native}"
GO_BIN="${GO_BIN:-/home/worker/go/bin/go}"
GO_TOOLCHAIN_MODE="${GO_TOOLCHAIN_MODE:-local}"
GO_EXPECTED_VERSION_PREFIX="${GO_EXPECTED_VERSION_PREFIX:-go version go1.25.12 }"

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
compiler="$HARMONYOS_NATIVE_HOME/llvm/bin/aarch64-unknown-linux-ohos-clang"
readelf="$HARMONYOS_NATIVE_HOME/llvm/bin/llvm-readelf"
if [[ ! -x "$GO_BIN" ]]; then
  printf 'Go launcher is not executable: %s\n' "$GO_BIN" >&2
  exit 1
fi
if [[ ! -x "$compiler" ]]; then
  printf 'OHOS aarch64 clang is not executable: %s\n' "$compiler" >&2
  exit 1
fi
if [[ ! -x "$readelf" ]]; then
  printf 'llvm-readelf is not executable: %s\n' "$readelf" >&2
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
    printf 'expected Go version prefix [%s], got: %s\n' "$GO_EXPECTED_VERSION_PREFIX" "$go_version" >&2
    exit 1
    ;;
esac

mkdir -p "$GO_PROBE_OUTPUT_DIR"
rm -f "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" "$GO_PROBE_OUTPUT_DIR/libgoprobe.h"

GOTOOLCHAIN="$GO_TOOLCHAIN_MODE" \
GOOS=linux \
GOARCH=arm64 \
CGO_ENABLED=1 \
CC="$script_dir/ohos-aarch64-clang" \
"$GO_BIN" -C "$script_dir" build -trimpath -buildmode=c-shared -o "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" .

printf 'built %s with %s\n' "$GO_PROBE_OUTPUT_DIR/libgoprobe.so" "$go_version"

lib="$GO_PROBE_OUTPUT_DIR/libgoprobe.so"
fail() {
  printf 'ELF assertion failed: %s\n' "$1" >&2
  exit 1
}

# a) AArch64 ELF64 shared object (ET_DYN).
readelf_h="$("$readelf" --file-header "$lib")"
printf '%s\n' "$readelf_h" | grep -Eq 'Class:[[:space:]]*ELF64' \
  || fail "header Class is not ELF64"
printf '%s\n' "$readelf_h" | grep -Eq 'Machine:[[:space:]]*AArch64' \
  || fail "header Machine is not AArch64"
printf '%s\n' "$readelf_h" | grep -Eq 'Type:[[:space:]]*DYN \(Shared object file\)' \
  || fail "header Type is not DYN (Shared object file)"
printf 'ELF_ASSERT_A_ARCH64_ELF64_DYN=pass\n'

# b) Exactly one PT_TLS program header (readelf prints the type column as "TLS",
#    never "PT_TLS"; the Section-to-Segment mapping rows start with an index).
pt_tls_count="$("$readelf" --program-headers "$lib" | awk '$1 == "TLS" { count++ } END { print count + 0 }')"
[[ "$pt_tls_count" == "1" ]] || fail "expected exactly 1 PT_TLS segment, found $pt_tls_count"
printf 'ELF_ASSERT_B_ONE_PT_TLS=pass\n'

# c) Exactly one R_AARCH64_TLS_TPREL64 relocation inside .rela.dyn.
rela_dyn="$("$readelf" --relocations "$lib" | awk "/Relocation section '\\.rela\\.dyn'/,/^\$/")"
tprel_count="$(printf '%s\n' "$rela_dyn" | grep -c 'R_AARCH64_TLS_TPREL64' || true)"
[[ "$tprel_count" == "1" ]] || fail "expected exactly 1 R_AARCH64_TLS_TPREL64 in .rela.dyn, found $tprel_count"
printf 'ELF_ASSERT_C_ONE_TPREL64_RELA=pass\n'

# d) The dynamic FLAGS entry must not carry STATIC_TLS.
readelf_d="$("$readelf" --dynamic "$lib")"
printf '%s\n' "$readelf_d" | grep -E '(FLAGS|FLAGS_1|NEEDED)' || true
flags_lines="$(printf '%s\n' "$readelf_d" | awk '$2 == "FLAGS" || $2 == "(FLAGS)"')"
if printf '%s\n' "$flags_lines" | grep -Eq 'STATIC.TLS'; then
  fail "dynamic FLAGS carries STATIC_TLS"
fi
printf 'ELF_ASSERT_D_NO_STATIC_TLS_FLAG=pass\n'

# e) NEEDED is exactly one entry and it is libc.so.
needed_lines="$(printf '%s\n' "$readelf_d" | awk '$2 == "NEEDED" || $2 == "(NEEDED)"')"
needed_count="$(printf '%s\n' "$needed_lines" | grep -c 'NEEDED' || true)"
[[ "$needed_count" == "1" ]] || fail "expected exactly 1 NEEDED entry, found $needed_count"
printf '%s\n' "$needed_lines" | grep -Eq 'Shared library: \[libc\.so\]' \
  || fail "the single NEEDED entry is not libc.so"
printf 'ELF_ASSERT_E_NEEDED_LIBC_ONLY=pass\n'

# f) Dynamic symbols export FUNC Hello and FUNC RuntimeProbe.
readelf_sym="$("$readelf" --dyn-symbols "$lib")"
printf '%s\n' "$readelf_sym" | grep -Eq '[[:space:]]FUNC[[:space:]].*[^a-zA-Z0-9_]Hello$' \
  || fail "dynamic symbols do not export FUNC Hello"
printf '%s\n' "$readelf_sym" | grep -Eq '[[:space:]]FUNC[[:space:]].*[^a-zA-Z0-9_]RuntimeProbe$' \
  || fail "dynamic symbols do not export FUNC RuntimeProbe"
printf 'ELF_ASSERT_F_EXPORTS_HELLO_RUNTIMEPROBE=pass\n'

# g) The single TPREL64 is LOCAL: r_info carries symbol index 0 (info value
#    is exactly the type 0x406) and the addend is 0. This "local IE" shape is
#    the core discriminator versus the x86_64 rejected profile.
tprel_line="$(printf '%s\n' "$rela_dyn" | grep 'R_AARCH64_TLS_TPREL64')"
tprel_info="$(printf '%s\n' "$tprel_line" | awk '{v=$2; sub(/^0+/, "", v); print toupper(v)}')"
tprel_addend="$(printf '%s\n' "$tprel_line" | awk '{print $NF}')"
[[ "$tprel_info" == "406" ]] || fail "TPREL64 r_info symbol index is not 0 (got info $tprel_info)"
[[ "$tprel_addend" == "0" ]] || fail "TPREL64 addend is not 0 (got $tprel_addend)"
printf 'ELF_ASSERT_G_TPREL64_LOCAL_ZERO_ADDEND=pass\n'

# h) FLAGS_1 carries NOW and NODELETE (Go c-shared runtime contract).
flags1_line="$(printf '%s\n' "$readelf_d" | awk '$2 == "FLAGS_1" || $2 == "(FLAGS_1)"')"
printf '%s\n' "$flags1_line" | grep -Eq 'NOW' || fail "FLAGS_1 does not carry NOW"
printf '%s\n' "$flags1_line" | grep -Eq 'NODELETE' || fail "FLAGS_1 does not carry NODELETE"
printf 'ELF_ASSERT_H_FLAGS1_NOW_NODELETE=pass\n'

# i) The runtime imports pthread_create (Type FUNC, Ndx UND) from libc.
printf '%s\n' "$readelf_sym" | awk '$4 == "FUNC" && $7 == "UND" && $8 == "pthread_create" { found = 1 } END { exit !found }' \
  || fail "dynamic symbols do not import UND FUNC pthread_create"
printf 'ELF_ASSERT_I_IMPORTS_PTHREAD_CREATE=pass\n'

printf 'libgoprobe.so sha256: %s\n' "$(sha256sum "$lib" | awk '{print $1}')"
printf 'libgoprobe.so size: %s bytes\n' "$(stat -c '%s' "$lib")"
