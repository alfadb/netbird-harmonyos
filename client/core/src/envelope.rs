// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 NetBird HarmonyOS contributors

//! # envelope — NetBird management message-body NaCl encryption (N3-3)
//!
//! The NaCl layer that wraps every management RPC body: a `GetServerKey` key
//! exchange followed by `crypto_box` (X25519 + XSalsa20-Poly1305) sealed
//! payloads inside `EncryptedMessage`. This is what the N3-2 plaintext-body
//! increment lacked; without it a REAL NetBird server rejects the body.
//!
//! ## Upstream wire facts (pinned commit)
//!
//! upstream: netbirdio/netbird @ `791401060d2b95e5f51e3439c0649729132f571e`
//! ([`crate::grpc::UPSTREAM_COMMIT`]). Evidence, quoted from that tree:
//!
//! - **Cipher**: `encryption/encryption.go:13-15` — "Curve25519, XSalsa20 and
//!   Poly1305", i.e. NaCl `crypto_box` (Go `golang.org/x/crypto/nacl/box`,
//!   encryption.go:7). No other scheme is used upstream.
//! - **Envelope format** (`encryption/encryption.go`):
//!   - `Encrypt()` L18-24: `box.Seal(nonce[:], msg, nonce, peerPub, priv)` —
//!     the 24-byte nonce is PREPENDED to the ciphertext (box.Seal's first
//!     argument is the output prefix; the encrypted payload is `msg` only).
//!   - `Decrypt()` L27-42: nonce = first 24 bytes (`nonceSize`, L11), open
//!     over the remainder; `< nonceSize` → "invalid encrypted message length".
//!   - `genNonce()` L44-51: 24 **random** bytes per message (crypto/rand) —
//!     not a counter, no per-direction sequence.
//!   - protobuf bytes in/out (`encryption/message.go` L76-90 marshal →
//!     Encrypt, L93-106 Decrypt → unmarshal; no compression, no AAD).
//! - **Server key**: `management.proto` L24
//!   `rpc GetServerKey(Empty) returns (ServerKeyResponse) {}`, response
//!   `ServerKeyResponse.key: string` (L314-316) = base64 of the 32-byte
//!   server WireGuard public key (upstream parses it with
//!   `wgtypes.ParseKey` — base64 std encoding — grpc.go:545-548).
//! - **Key provenance**: the envelope keys ARE the WireGuard identity keys.
//!   `client/internal/connect.go:243`
//!   `myPrivateKey, err := wgtypes.ParseKey(c.config.PrivateKey)` (config key
//!   generated once per profile, profilemanager/config.go:917-919, and
//!   reused for WG handshakes); the SAME key is handed to the management
//!   client, `connect.go:313` `mgm.NewClient(..., myPrivateKey, ...)` →
//!   `shared/management/client/grpc.go:128`
//!   `func NewClient(ctx, addr, ourPrivateKey wgtypes.Key, ...)`. NOT a
//!   temporary key pair. Lifecycle: generated once per device profile,
//!   persisted, lives as long as the profile (rotate = re-register).
//! - **Login flow** (grpc.go:585-637): `getServerPublicKey()` (L587-590) →
//!   `encryption.EncryptMessage(*serverKey, c.key, req)` (L595) →
//!   `Login(&proto.EncryptedMessage{WgPubKey: c.key.PublicKey().String(),
//!   Body: loginReq})` (L603-612) → reply body decrypted with the SAME
//!   server key, `encryption.DecryptMessage(*serverKey, c.key, resp.Body,
//!   loginResp)` (L627-631). `EncryptedMessage.version` is not set (0)
//!   upstream on Login.
//!
//! ## Key/nonce lifecycle in this module
//!
//! - [`EnvelopeKeyPair`]: held by [`crate::grpc::ManagementGrpcClient`]
//!   (injected at `connect()`, mirroring upstream `NewClient(ourPrivateKey)`).
//!   Callers obtain it either from the persisted WireGuard private key
//!   ([`EnvelopeKeyPair::from_secret_bytes`]) or fresh
//!   ([`EnvelopeKeyPair::generate`]) at profile creation.
//! - [`EnvelopePublicKey`]: the remote side's public key. For `login()` this
//!   is the server key fetched fresh per login via `GetServerKey`
//!   (upstream does the same: `getServerPublicKey()` inside `login()`),
//!   never persisted.
//! - Nonce: 24 fresh random bytes per [`seal`] via the aead `OsRng`
//!   (rand_core/getrandom — the getrandom 0.2 already compiled for the ohos
//!   target). Nonce reuse across messages would be catastrophic for
//!   XSalsa20; a fresh random nonce per seal matches upstream `genNonce()`.
//!
//! ## Error classification (reuses the existing N3-1 classes)
//!
//! No new variant: the six [`ManagementError`] classes already cover the
//! failure modes, and adding a variant would ripple through the NAPI surface
//! without adding actionable information.
//!
//! - `GetServerKey` gRPC failure → [`crate::grpc::map_grpc_status`]
//!   (5xx → `Server`, deadline → `Timeout`, `Unavailable` → `Network`, ...).
//! - `ServerKeyResponse.key` not base64 / not 32 bytes → `Parse`
//!   (response not the shape we require — same class as a malformed body).
//! - [`open`] failure (wrong key, tampered or truncated ciphertext) →
//!   `Parse` with an explicit "authentication failed" message: the response
//!   is unusable, and its authenticity cannot be established.
//! - [`seal`] failure → `Request { status: 0 }` (local pre-flight failure,
//!   existing convention; unreachable in practice with `AAD = ()`).
//!
//! ## Interop cross-check
//!
//! `tests` below assert that [`EnvelopeKeyPair::public_key_bytes`] equals
//! boringtun's frozen `x25519_public_key` export for the same secret — the
//! concrete proof that "reuse the WG private key" is sound: the WireGuard
//! peer key and the envelope key are the same Curve25519 key upstream.

use crate::management::ManagementError;
use base64::Engine as _;
use crypto_box::aead::{AeadCore, AeadInPlace, OsRng};
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey};

/// NaCl nonce size in bytes (upstream `encryption.go:11 nonceSize = 24`).
pub const NONCE_SIZE: usize = 24;
/// X25519 key size in bytes (`crypto_box::KEY_SIZE`; wgtypes.Key = 32).
pub const KEY_SIZE: usize = 32;

// ---------------------------------------------------------------------------
// key material
// ---------------------------------------------------------------------------

/// Client-side identity key pair for the management envelope.
///
/// Upstream reuses the WireGuard private key of the device profile for BOTH
/// tunnel handshakes and this envelope (`connect.go:243` → `grpc.go:128`);
/// [`EnvelopeKeyPair::from_secret_bytes`] feeds exactly those 32 bytes. The
/// secret is zeroized when the value is dropped (crypto_box `SecretKey`).
#[derive(Clone)]
pub struct EnvelopeKeyPair {
    secret: SecretKey,
    public: PublicKey,
}

// SecretKey deliberately does not implement Debug; never leak key material
// through Debug/Display formatting.
impl core::fmt::Debug for EnvelopeKeyPair {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("EnvelopeKeyPair")
            .field("public", &self.public_key_base64())
            .finish_non_exhaustive()
    }
}

impl EnvelopeKeyPair {
    /// Fresh random key pair — for NEW device profiles (upstream
    /// `profilemanager/config.go:917-919 generateKey()`).
    pub fn generate() -> Result<Self, ManagementError> {
        let secret = SecretKey::generate(&mut OsRng);
        let public = secret.public_key();
        Ok(EnvelopeKeyPair { secret, public })
    }

    /// Build from an existing 32-byte private key — the upstream shape: the
    /// persisted WireGuard private key doubles as the envelope key.
    pub fn from_secret_bytes(secret: &[u8; KEY_SIZE]) -> Self {
        let secret = SecretKey::from_bytes(*secret);
        let public = secret.public_key();
        EnvelopeKeyPair { secret, public }
    }

    /// The matching public key, raw 32 bytes.
    pub fn public_key_bytes(&self) -> [u8; KEY_SIZE] {
        *self.public.as_bytes()
    }

    /// The matching public key, base64 (std alphabet) — the string upstream
    /// sends as `EncryptedMessage.wgPubKey` (`c.key.PublicKey().String()`).
    pub fn public_key_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.public.as_bytes())
    }
}

/// The remote peer's NaCl/X25519 public key (server key for `login()`; a
/// client key when this module's primitives are used from the server side of
/// a test).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopePublicKey(PublicKey);

impl EnvelopePublicKey {
    /// From raw 32 bytes.
    pub fn from_bytes(bytes: &[u8; KEY_SIZE]) -> Self {
        EnvelopePublicKey(PublicKey::from_bytes(*bytes))
    }

    /// From the base64 `ServerKeyResponse.key` string (upstream
    /// `wgtypes.ParseKey(resp.Key)`, grpc.go:545-548). Malformed input →
    /// `ManagementError::Parse`.
    pub fn from_base64(key: &str) -> Result<Self, ManagementError> {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(key.trim())
            .map_err(|e| ManagementError::Parse(format!("server key is not base64: {e}")))?;
        let bytes: [u8; KEY_SIZE] = raw.as_slice().try_into().map_err(|_| {
            ManagementError::Parse(format!(
                "server key must be {KEY_SIZE} bytes, got {}",
                raw.len()
            ))
        })?;
        Ok(EnvelopePublicKey(PublicKey::from_bytes(bytes)))
    }

    /// Raw 32 bytes.
    pub fn as_bytes(&self) -> [u8; KEY_SIZE] {
        *self.0.as_bytes()
    }

    /// base64 (std alphabet) form.
    pub fn to_base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(self.0.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// seal / open (upstream encryption.go shape: wire = nonce || ciphertext)
// ---------------------------------------------------------------------------

/// X25519-DH + XSalsa20-Poly1305 box between `peer` and us.
fn envelope_box(peer: &EnvelopePublicKey, keys: &EnvelopeKeyPair) -> SalsaBox {
    SalsaBox::new(&peer.0, &keys.secret)
}

/// Seal `plaintext` for `peer`: returns `nonce (24B) || ciphertext(plain+16B
/// tag)` — the exact upstream `box.Seal(nonce[:], msg, ...)` layout
/// (encryption.go:23). The nonce is 24 fresh random bytes per call
/// (encryption.go:44-51). AAD: none (crypto_box does not support AAD; Go
/// box.Seal/Open use none either).
pub fn seal(
    peer: &EnvelopePublicKey,
    keys: &EnvelopeKeyPair,
    plaintext: &[u8],
) -> Result<Vec<u8>, ManagementError> {
    let nonce: Nonce = SalsaBox::generate_nonce(&mut OsRng);
    let mut buf = plaintext.to_vec();
    envelope_box(peer, keys)
        .encrypt_in_place(&nonce, &[], &mut buf)
        .map_err(|e| {
            ManagementError::Request { status: 0, message: format!("envelope seal failed: {e:?}") }
        })?;
    let mut wire = Vec::with_capacity(NONCE_SIZE + buf.len());
    wire.extend_from_slice(&nonce);
    wire.extend_from_slice(&buf);
    Ok(wire)
}

/// Open a `nonce (24B) || ciphertext` wire produced by [`seal`] (or upstream
/// Go `box.Seal(nonce[:], ...)` — byte-compatible). Anything shorter than
/// the nonce, failing authentication (tamper / wrong key), returns
/// `ManagementError::Parse` — upstream `Decrypt` fails the same inputs
/// (encryption.go:32-39).
pub fn open(
    peer: &EnvelopePublicKey,
    keys: &EnvelopeKeyPair,
    wire: &[u8],
) -> Result<Vec<u8>, ManagementError> {
    if wire.len() < NONCE_SIZE {
        return Err(ManagementError::Parse(format!(
            "envelope too short: {} bytes < nonce {NONCE_SIZE}",
            wire.len()
        )));
    }
    let nonce = Nonce::from_slice(&wire[..NONCE_SIZE]);
    let mut buf = wire[NONCE_SIZE..].to_vec();
    envelope_box(peer, keys)
        .decrypt_in_place(nonce, &[], &mut buf)
        .map_err(|_| {
            ManagementError::Parse(
                "envelope open failed: ciphertext authentication rejected \
                 (wrong key or tampered data)"
                    .into(),
            )
        })?;
    Ok(buf)
}

// ---------------------------------------------------------------------------
// unit tests (host, no I/O)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 7748 §6.1 test vector: Alice's private/public X25519 pair.
    const RFC_ALICE_SK: [u8; 32] = [
        0x77, 0x07, 0x6d, 0x0a, 0x73, 0x18, 0xa5, 0x7d, 0x3c, 0x16, 0xc1, 0x72, 0x51, 0xb2,
        0x66, 0x45, 0xdf, 0x4c, 0x2f, 0x87, 0xeb, 0xc0, 0x99, 0x2a, 0xb1, 0x77, 0xfb, 0xa5,
        0x1d, 0xb9, 0x2c, 0x2a,
    ];
    const RFC_ALICE_PK: [u8; 32] = [
        0x85, 0x20, 0xf0, 0x09, 0x89, 0x30, 0xa7, 0x54, 0x74, 0x8b, 0x7d, 0xdc, 0xb4, 0x3e,
        0xf7, 0x5a, 0x0d, 0xbf, 0x3a, 0x0d, 0x26, 0x38, 0x1a, 0xf4, 0xeb, 0xa4, 0xa9, 0x8e,
        0xaa, 0x9b, 0x4e, 0x6a,
    ];

    fn peer_from(pair: &EnvelopeKeyPair) -> EnvelopePublicKey {
        EnvelopePublicKey::from_bytes(&pair.public_key_bytes())
    }

    #[test]
    fn public_key_matches_rfc7748_vector() {
        let keys = EnvelopeKeyPair::from_secret_bytes(&RFC_ALICE_SK);
        assert_eq!(keys.public_key_bytes(), RFC_ALICE_PK);
    }

    /// Upstream's "reuse the WG key" premise: crypto_box and boringtun's
    /// frozen `x25519_public_key` derive the SAME public key from the same
    /// secret — one Curve25519 key, two users (tunnel + envelope).
    #[test]
    fn public_key_matches_boringtun_x25519_derivation() {
        let sk: [u8; 32] = core::array::from_fn(|i| (i as u8) * 7 + 1);
        let ours = EnvelopeKeyPair::from_secret_bytes(&sk).public_key_bytes();
        let theirs = boringtun::ffi::x25519_public_key(boringtun::ffi::x25519_key { key: sk });
        assert_eq!(ours, theirs.key);
    }

    #[test]
    fn generate_makes_distinct_pairs() {
        let a = EnvelopeKeyPair::generate().unwrap();
        let b = EnvelopeKeyPair::generate().unwrap();
        assert_ne!(a.public_key_bytes(), b.public_key_bytes());
    }

    #[test]
    fn seal_open_roundtrip_matches_upstream_layout() {
        let client = EnvelopeKeyPair::generate().unwrap();
        let server = EnvelopeKeyPair::generate().unwrap();
        let server_pk = peer_from(&server);

        let wire = seal(&server_pk, &client, b"login-request-bytes").unwrap();
        // nonce (24) || plaintext (19) || tag (16)
        assert_eq!(wire.len(), NONCE_SIZE + 19 + 16);

        let opened = open(&peer_from(&client), &server, &wire).unwrap();
        assert_eq!(opened, b"login-request-bytes");
    }

    #[test]
    fn nonce_is_fresh_per_seal() {
        let pair = EnvelopeKeyPair::generate().unwrap();
        let peer = peer_from(&pair);
        let a = seal(&peer, &pair, b"same").unwrap();
        let b = seal(&peer, &pair, b"same").unwrap();
        assert_ne!(&a[..NONCE_SIZE], &b[..NONCE_SIZE], "nonce must never repeat");
        assert_ne!(a, b, "same plaintext must not yield the same wire");
    }

    #[test]
    fn tampered_ciphertext_fails_authentication() {
        let pair = EnvelopeKeyPair::generate().unwrap();
        let peer = peer_from(&pair);
        let mut wire = seal(&peer, &pair, b"payload").unwrap();
        let last = wire.len() - 1;
        wire[last] ^= 0x01;
        let err = open(&peer, &pair, &wire).unwrap_err();
        assert!(matches!(err, ManagementError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn wrong_peer_public_key_fails_to_open() {
        let client = EnvelopeKeyPair::generate().unwrap();
        let server = EnvelopeKeyPair::generate().unwrap();
        let impostor = EnvelopeKeyPair::generate().unwrap();
        let wire = seal(&peer_from(&server), &client, b"payload").unwrap();
        // receiver holds a different key pair than the sender sealed for
        let err = open(&peer_from(&client), &impostor, &wire).unwrap_err();
        assert!(matches!(err, ManagementError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn short_wire_is_rejected_before_open() {
        let pair = EnvelopeKeyPair::generate().unwrap();
        let peer = peer_from(&pair);
        for n in [0usize, 1, 23] {
            let err = open(&peer, &pair, &vec![0u8; n]).unwrap_err();
            assert!(matches!(err, ManagementError::Parse(_)), "len {n}: {err:?}");
        }
    }

    #[test]
    fn server_key_base64_parsing() {
        let raw = [9u8; 32];
        let ok = EnvelopePublicKey::from_base64(
            &base64::engine::general_purpose::STANDARD.encode(raw),
        )
        .unwrap();
        assert_eq!(ok.as_bytes(), raw);

        for bad in ["not-base64!!!", "", "QUJD", &base64::engine::general_purpose::STANDARD.encode([1u8; 31])[..]] {
            let err = EnvelopePublicKey::from_base64(bad).unwrap_err();
            assert!(matches!(err, ManagementError::Parse(_)), "{bad:?}: {err:?}");
        }
    }

    #[test]
    fn base64_forms_roundtrip() {
        let pair = EnvelopeKeyPair::generate().unwrap();
        let back = EnvelopePublicKey::from_base64(&pair.public_key_base64()).unwrap();
        assert_eq!(back.as_bytes(), pair.public_key_bytes());
    }
}
