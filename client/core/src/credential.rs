//! Credential interface (R3 skeleton): read/write/delete the local WireGuard
//! private key and the node identity through a [`CredentialStore`] trait.
//!
//! ## SECURITY STATEMENT — READ BEFORE SHIPPING
//! **NO security guarantees are made by this module.** The only implementation
//! here is [`InMemoryCredentialStore`], which is *development scaffolding*:
//! it keeps secrets as plain process memory with no encryption, no
//! zeroization, no access control, and it loses everything on process exit.
//! It MUST NOT be presented as secure storage in any user-facing way.
//!
//! Platform secure storage (HarmonyOS asset store / HUKS-backed persistence,
//! behind the ArkTS side) is a **future implementation point: NOT implemented,
//! NOT verified**. When it exists it should implement the same trait (the
//! trait is object-safe for exactly that reason: `Box<dyn CredentialStore>`)
//! and this module's contract below must hold for it.
//!
//! Contract shared by all implementations:
//! - `load_*` on an empty store returns `Ok(None)` (absence is not an error).
//! - `delete_*` on an absent value is idempotent: `Ok(())`.
//! - `store_*` validates shape (32-byte keys, sane identity strings) and
//!   rejects malformed values with [`CredentialError::Invalid`] rather than
//!   storing them.
//! - Overwrite semantics: `store_*` replaces the previous value.

/// Node identity: the persistent, non-secret label this node is known by.
/// Kept minimal on purpose (R3 skeleton); extend only when a consumer exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeIdentity {
    /// Opaque node id string (e.g. a UUID-ish label issued at enrollment).
    pub node_id: String,
}

impl NodeIdentity {
    pub fn new(node_id: impl Into<String>) -> Result<Self, CredentialError> {
        let node_id = node_id.into();
        if node_id.is_empty() {
            return Err(CredentialError::Invalid("node identity must not be empty".into()));
        }
        if node_id.len() > 128 {
            return Err(CredentialError::Invalid(format!(
                "node identity longer than 128 chars ({})",
                node_id.len()
            )));
        }
        if node_id
            .chars()
            .any(|c| (c as u32) < 0x20 || c as u32 == 0x7f)
        {
            return Err(CredentialError::Invalid(
                "node identity must not contain control characters".into(),
            ));
        }
        Ok(NodeIdentity { node_id })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CredentialError {
    /// The value presented for storage is malformed (rejected, not stored).
    Invalid(String),
    /// The backend itself failed (memory store: cannot happen today; future
    /// platform store: OS API failures surface here).
    Backend(String),
}

impl core::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CredentialError::Invalid(msg) => write!(f, "invalid credential: {msg}"),
            CredentialError::Backend(msg) => write!(f, "credential backend failure: {msg}"),
        }
    }
}

impl std::error::Error for CredentialError {}

/// Storage interface for the node's own secrets.
///
/// Object-safe (`Box<dyn CredentialStore>` works) so the future HarmonyOS
/// secure-storage implementation can be dropped in behind the same interface.
/// Keys are raw 32-byte WireGuard keys.
pub trait CredentialStore {
    fn load_private_key(&self) -> Result<Option<[u8; 32]>, CredentialError>;
    fn store_private_key(&mut self, key: &[u8; 32]) -> Result<(), CredentialError>;
    fn delete_private_key(&mut self) -> Result<(), CredentialError>;

    fn load_node_identity(&self) -> Result<Option<NodeIdentity>, CredentialError>;
    fn store_node_identity(&mut self, identity: &NodeIdentity) -> Result<(), CredentialError>;
    fn delete_node_identity(&mut self) -> Result<(), CredentialError>;
}

/// DEVELOPMENT-ONLY in-memory store — **NOT SECURE STORAGE**, see the module
/// security statement above. Plain `Option` fields, no encryption, no
/// zeroization, lost on exit.
#[derive(Clone, Debug, Default)]
pub struct InMemoryCredentialStore {
    private_key: Option<[u8; 32]>,
    identity: Option<NodeIdentity>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.private_key.is_none() && self.identity.is_none()
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn load_private_key(&self) -> Result<Option<[u8; 32]>, CredentialError> {
        Ok(self.private_key)
    }

    fn store_private_key(&mut self, key: &[u8; 32]) -> Result<(), CredentialError> {
        // shape is enforced by the type here; the all-zero key is accepted
        // as *structurally* valid — semantic key checks belong to the WG layer
        self.private_key = Some(*key);
        Ok(())
    }

    fn delete_private_key(&mut self) -> Result<(), CredentialError> {
        self.private_key = None;
        Ok(())
    }

    fn load_node_identity(&self) -> Result<Option<NodeIdentity>, CredentialError> {
        Ok(self.identity.clone())
    }

    fn store_node_identity(&mut self, identity: &NodeIdentity) -> Result<(), CredentialError> {
        // re-validate so trait-level callers cannot bypass NodeIdentity::new
        let checked = NodeIdentity::new(identity.node_id.as_str())?;
        self.identity = Some(checked);
        Ok(())
    }

    fn delete_node_identity(&mut self) -> Result<(), CredentialError> {
        self.identity = None;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn key(seed: u8) -> [u8; 32] {
        let mut k = [0u8; 32];
        for (i, b) in k.iter_mut().enumerate() {
            *b = (i as u8).wrapping_mul(seed).wrapping_add(seed);
        }
        k
    }

    /// Shared behavior suite: exercised through the trait so the future
    /// platform implementation can run the exact same checks.
    fn trait_behavior_suite(store: &mut dyn CredentialStore) {
        // absent -> None; delete idempotent
        assert_eq!(store.load_private_key().unwrap(), None);
        assert_eq!(store.load_node_identity().unwrap(), None);
        store.delete_private_key().unwrap();
        store.delete_node_identity().unwrap();

        // roundtrip
        let k = key(3);
        store.store_private_key(&k).unwrap();
        assert_eq!(store.load_private_key().unwrap(), Some(k));

        let id = NodeIdentity::new("node-7f3a").unwrap();
        store.store_node_identity(&id).unwrap();
        assert_eq!(store.load_node_identity().unwrap(), Some(id));

        // overwrite
        let k2 = key(5);
        store.store_private_key(&k2).unwrap();
        assert_eq!(store.load_private_key().unwrap(), Some(k2));
        let id2 = NodeIdentity::new("node-0002").unwrap();
        store.store_node_identity(&id2).unwrap();
        assert_eq!(store.load_node_identity().unwrap(), Some(id2));

        // delete -> absent again
        store.delete_private_key().unwrap();
        assert_eq!(store.load_private_key().unwrap(), None);
        store.delete_node_identity().unwrap();
        assert_eq!(store.load_node_identity().unwrap(), None);
    }

    #[test]
    fn memory_store_behavior_through_trait() {
        let mut store = InMemoryCredentialStore::new();
        assert!(store.is_empty());
        trait_behavior_suite(&mut store);
        assert!(store.is_empty());
    }

    #[test]
    fn works_through_boxed_dyn_trait() {
        let mut store: Box<dyn CredentialStore> = Box::new(InMemoryCredentialStore::new());
        trait_behavior_suite(store.as_mut());
    }

    #[test]
    fn identity_validation_rejects_bad_values() {
        assert!(NodeIdentity::new("").is_err());
        assert!(NodeIdentity::new("a".repeat(129)).is_err());
        assert!(NodeIdentity::new("bad\nid").is_err());
        assert!(NodeIdentity::new("bad\u{7f}id").is_err());
        assert!(NodeIdentity::new("ok-id_1.2").is_ok());

        // store path re-validates (cannot bypass via raw struct)
        let mut store = InMemoryCredentialStore::new();
        let bad = NodeIdentity { node_id: "x\ny".into() };
        let err = store.store_node_identity(&bad).unwrap_err();
        assert!(matches!(err, CredentialError::Invalid(_)), "{err}");
        assert_eq!(store.load_node_identity().unwrap(), None);
    }

    #[test]
    fn error_display_shapes() {
        let e = CredentialError::Invalid("boom".into());
        assert_eq!(e.to_string(), "invalid credential: boom");
        let e = CredentialError::Backend("os says no".into());
        assert_eq!(e.to_string(), "credential backend failure: os says no");
    }
}
