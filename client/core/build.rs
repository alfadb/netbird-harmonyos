// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! Build script (N3-2): gRPC codegen for the NetBird management protocol.
//!
//! Pipeline (frozen stack, docs/n3-stack-freeze-20260913.md):
//!   proto/management.proto  --protox-->  FileDescriptorSet  --tonic-prost-build-->
//!   $OUT_DIR/management.rs  (messages + tonic client/server in one file)
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
}
