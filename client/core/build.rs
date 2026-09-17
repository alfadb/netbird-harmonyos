// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Build script (N3-2): gRPC codegen for the NetBird management protocol.
//!
//! Pipeline (frozen stack, docs/n3-stack-freeze-20260913.md):
//!   proto/management.proto  --protox-->  FileDescriptorSet  --tonic-prost-build-->
//!   $OUT_DIR/management.rs  (messages + tonic client/server in one file)
//!
//! N4a adds the same pipeline for the signal exchange protocol:
//!   proto/signalexchange.proto --> $OUT_DIR/signalexchange.rs
//! (consumed by `src/signal.rs`; verbatim copy, see proto/README.md).
//!
//! - `protox` is a pure-Rust protobuf compiler: NO protoc, NO network access.
//!   The well-known imports of management.proto
//!   (google/protobuf/timestamp.proto, google/protobuf/duration.proto) are
//!   resolved from protox's embedded `GoogleFileResolver` descriptors, so the
//!   repository only carries the one upstream file that is actually needed
//!   (see proto/README.md for provenance/license).
//! - Server stubs are generated too: the in-process tonic test server
//!   (tests/management_grpc.rs) implements the `Login` RPC over real TLS. The
//!   production cdylib keeps the dead code (never registered as a service);
//!   codegen-level size trimming is tracked as a follow-up in
//!   docs/n3-management-protocol-notes.md.

use std::path::{Path, PathBuf};

fn main() {
    // I7 (design §3.3 / §7-I7): the build-time self-identification the device
    // evidence document carries in its trailing `collector{}` block.
    emit_build_identity();

    let proto_root = Path::new("proto");
    let management_proto = proto_root.join("management.proto");
    println!("cargo:rerun-if-changed={}", management_proto.display());
    println!("cargo:rerun-if-changed=proto/README.md");

    let file_descriptors = protox::compile([management_proto], [proto_root])
        .unwrap_or_else(|e| panic!("protox failed to compile management.proto: {e}"));

    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        // RPCs we do not use get default trait methods (unimplemented status):
        // the N3-2 test server only implements Login.
        .generate_default_stubs(true)
        .compile_fds(file_descriptors)
        .expect("tonic-prost-build failed to generate management stubs");

    // N4a: the signal exchange protocol (`SignalExchange/Send` unary +
    // `SignalExchange/ConnectStream` bidi stream). Same offline pipeline;
    // the `google/protobuf/descriptor.proto` import resolves through protox's
    // embedded GoogleFileResolver like the management well-known imports.
    let signal_proto = proto_root.join("signalexchange.proto");
    println!("cargo:rerun-if-changed={}", signal_proto.display());
    let signal_fds = protox::compile([signal_proto], [proto_root])
        .unwrap_or_else(|e| panic!("protox failed to compile signalexchange.proto: {e}"));
    tonic_prost_build::configure()
        .build_client(true)
        .build_server(true)
        .generate_default_stubs(true)
        .compile_fds(signal_fds)
        .expect("tonic-prost-build failed to generate signal stubs");
}

// ---------------------------------------------------------------------------
// I7 — build self-identification (design §3.3's `collector{}` block)
// ---------------------------------------------------------------------------

/// Frozen caliber tag of the source digest (never reuse it for another shape).
const BUILD_ID_HEADER: &[u8] = b"n2h-build-id-v1\n";

/// Inputs of the reproducible source digest: the files whose bytes determine
/// the cdylib. `tests/` is deliberately NOT an input (it is not compiled into
/// the artifact); `target/` never is.
const FIXED_INPUTS: [&str; 3] = ["Cargo.toml", "Cargo.lock", "build.rs"];
const TREE_INPUTS: [&str; 2] = ["proto", "src"];

/// Emit the crate's build self-identification (design §3.3, A-1):
/// `N2H_BUILD_SHA256` (the `collector{}` block's `build_sha256`) and
/// `N2H_BUILD_TARGET` (`target_triple`; `env!("TARGET")` is a BUILD SCRIPT
/// variable, so the crate cannot read it directly and gets it forwarded here).
///
/// Two sources, in this order:
///
/// 1. the `N2H_BUILD_SHA256` environment variable when the invoker passes a
///    64-hex digest — the design's "`client/build.sh` passes the sha256" seam,
///    which binds a document to one concrete artifact (and is therefore only
///    as reproducible as that artifact);
/// 2. otherwise the source digest — `sha256(BUILD_ID_HEADER ++ for every input
///    in ascending relative-path byte order: "<relpath>\n" ++ bytes ++ "\0")`.
///
/// (2) is the DEFAULT and the reproducible one: the same source tree always
/// yields the same digest, no matter the host, the target, the profile or the
/// build directory. `client/core/build.sh` step 7 recomputes it with an
/// independent shell pipeline and greps the shipped `.so` for it, so the value
/// the device document carries is offline-checkable.
///
/// Fail-loud: a `N2H_BUILD_SHA256` that is not a 64-hex digest is refused
/// (never silently ignored), and `collector{}` can never embed an empty value.
fn emit_build_identity() {
    let root = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into()));
    let inputs = build_inputs(&root);
    // Every digest input is watched, so the digest can never go stale: without
    // these lines Cargo would only re-run this script for the .proto files.
    for rel in &inputs {
        println!("cargo:rerun-if-changed={rel}");
    }
    println!("cargo:rerun-if-env-changed=N2H_BUILD_SHA256");

    let sha = match std::env::var("N2H_BUILD_SHA256") {
        Ok(v) if !v.is_empty() => match normalized_sha256(&v) {
            Some(sha) => sha,
            None => panic!(
                "N2H_BUILD_SHA256 must be a 64-character hex sha256 digest, got '{}' ({} chars)",
                v.chars().take(80).collect::<String>(),
                v.len()
            ),
        },
        _ => source_digest(&root, &inputs),
    };
    assert!(
        normalized_sha256(&sha).is_some(),
        "internal error: the build identity must be a 64-hex digest, got '{sha}'"
    );
    println!("cargo:rustc-env=N2H_BUILD_SHA256={sha}");
    println!(
        "cargo:rustc-env=N2H_BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".into())
    );
}

/// `Some(lowercase 64-hex)` iff `v` is exactly a 64-character hex digest.
fn normalized_sha256(v: &str) -> Option<String> {
    let ok = v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit());
    if ok {
        Some(v.to_ascii_lowercase())
    } else {
        None
    }
}

/// The digest inputs, as relative paths in ascending byte order (the ORDER IS
/// PART OF THE CALIBER — the shell re-check sorts with `LC_ALL=C sort`).
fn build_inputs(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for rel in FIXED_INPUTS {
        if root.join(rel).is_file() {
            out.push(rel.to_string());
        }
    }
    for rel in TREE_INPUTS {
        collect_files(&root.join(rel), rel, &mut out);
    }
    out.sort();
    out
}

fn collect_files(dir: &Path, rel: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let child = format!("{rel}/{name}");
        if path.is_dir() {
            collect_files(&path, &child, out);
        } else if path.is_file() {
            out.push(child);
        }
    }
}

fn source_digest(root: &Path, inputs: &[String]) -> String {
    let mut buf = Vec::new();
    buf.extend_from_slice(BUILD_ID_HEADER);
    for rel in inputs {
        let full = root.join(rel);
        let bytes = std::fs::read(&full)
            .unwrap_or_else(|e| panic!("build identity: cannot read {}: {e}", full.display()));
        buf.extend_from_slice(rel.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(&bytes);
        buf.push(0);
    }
    sha256_hex(&buf)
}

/// SHA-256 (FIPS 180-4) as lowercase hex. Self-contained on purpose: a build
/// script cannot use the crate it builds, and the build-dependency set is
/// frozen (offline + `--locked`), so no digest crate is added.
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bitlen = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bitlen.to_be_bytes());
    let mut w = [0u32; 64];
    for block in msg.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}
