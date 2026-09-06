# n1bdisc_probe — N1BDISC native probe crate

Rust/NAPI cdylib for the N1BDISC physical discovery campaign. Host-only build
(cross-compile to `aarch64-unknown-linux-ohos` with the SDK26 native llvm); no
device, no HDC, no signing. Authoritative spec:
`docs/n1b-disc-gate-plan.md@04cf222+CC-1(criteria-change-1-reviewed-pass-2026-09-05)` (read-only;
CC-1 = 冻结后首次判据变更经跨厂商隔离重审通过，正文行号锚已随登记块插入 +2 位移).

- **Full NAPI surface documentation (ArkTS contract): `src/lib.rs` module docs.**
  Import: `import probe from 'libn1bdisc_probe.so'`; every export is
  synchronous and returns a JSON string; fd numbers cross the boundary as int
  (`fdOrig` from ArkTS `create()`, `fdDup` from `d2_lock()`).
- **Build + verification**: `bash build.sh` — verifies the boringtun 0.7.1
  crate sha256 and Cargo.lock checksum against the frozen value, then does a
  clean `cargo build --release --target aarch64-unknown-linux-ohos --offline
  --locked`, then checks ELF (ELF64/AArch64/DYN, NEEDED without libc.so.6),
  the 14 frozen BoringTun ffi symbols in dynsym, the napi/hilog import surface
  (A1-clean), and the `.init_array` module-registration relocation.
- **Artifact**: `target/aarch64-unknown-linux-ohos/release/libn1bdisc_probe.so`
  (DT_NEEDED: libc.so, libhilog_ndk.z.so, libace_napi.z.so).
- **Layout**: `src/sys.rs` (closed syscall table), `src/hilog.rs` (frozen
  channel 0x2900/N1BDiscVpn), `src/util.rs` (sha256/base64/hex/json),
  `src/ledger.rs` (fd ledger + canonical digest), `src/chunk.rs`, `src/net.rs`
  (frozen packet builders/parsers), `src/btkeep.rs` (14 ffi symbols kept in
  dynsym), `src/d1.rs`…`src/d8.rs`, `src/dw.rs` (D-W worker + mainline + P12),
  `src/d6.rs`, `src/napi.rs` (handwritten NAPI glue), `src/state.rs`.
- **Deviations/ambiguities register**: `src/lib.rs` DEVIATIONS section
  (A3 address-only reference tension, gettid, d1_so_sha256/d1_cmdline/d1_pid
  host-side duties, dw_inwait_proc_fd inst numbering, destroy-side close
  unobservability, D1_FAIL inline sanitization, extra mb1 params, P12
  domain-gate-violation raw reporting, POST field layout choices).
