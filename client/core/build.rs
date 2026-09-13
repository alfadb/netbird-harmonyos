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

use std::path::Path;

fn main() {
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
