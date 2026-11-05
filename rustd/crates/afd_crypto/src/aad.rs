//! The associated data every envelope operation binds.
//!
//! The format is three fields joined by the ASCII unit separator, and every row
//! in `vault.secrets` was sealed under it:
//!
//! ```text
//! lower(workspace_id) 0x1f key_name 0x1f kek_version
//! ```
//!
//! # What binding it buys
//!
//! The same associated data is supplied when wrapping the DEK and when
//! encrypting the payload. A row lifted into another workspace, renamed, or
//! re-labelled with a different version fails its authentication tag instead of
//! decrypting, so the ciphertext columns are not portable on their own.
//!
//! # The asymmetry is deliberate
//!
//! `workspace_id` is lowercased; `key_name` is not. Stored rows were sealed with
//! that asymmetry, so it stays rather than being tidied: lowercasing `key_name`
//! here would make every stored row with an upper-case character in its name
//! fail to open.

use crate::KEK_VERSION;

/// The ASCII unit separator that joins the associated-data fields.
const SEPARATOR: u8 = 0x1f;

/// The associated data binding one envelope to its workspace, name and version.
#[derive(Clone, PartialEq, Eq)]
pub struct Aad(Vec<u8>);

impl Aad {
    /// Separately wrapped content-identity key; repository bytes are length-safe hex.
    #[must_use]
    pub fn checkpoint_identity(
        workspace: &afd_core::id::Uuid7,
        identity: &afd_core::id::Uuid7,
        repository: &str,
        key_version: i32,
        kek_version: i32,
    ) -> Self {
        let mut bytes = b"\0agentsfleet.checkpoint-identity.v1\0".to_vec();
        for field in [
            workspace.as_str().to_owned(),
            identity.as_str().to_owned(),
            hex::encode(repository),
            key_version.to_string(),
            kek_version.to_string(),
        ] {
            bytes.extend_from_slice(field.as_bytes());
            bytes.push(0);
        }
        Self(bytes)
    }

    /// Domain-separated ephemeral read custody for one issued external effect.
    #[must_use]
    pub fn effect_recovery(workspace: &afd_core::id::Uuid7, effect: &afd_core::id::Uuid7) -> Self {
        let mut bytes = b"\0agentsfleet.effect-recovery.v1\0".to_vec();
        bytes.extend_from_slice(workspace.as_str().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(effect.as_str().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(KEK_VERSION.to_string().as_bytes());
        Self(bytes)
    }

    /// Builds associated data for the current KEK version.
    #[must_use]
    pub fn new(workspace_id: &str, key_name: &str) -> Self {
        Self::versioned(workspace_id, key_name, KEK_VERSION)
    }

    /// Builds associated data pinned to an explicit KEK version.
    ///
    /// Exposed for the parity fixtures, which carry the version they were
    /// written under, and for the negative test that proves a version mismatch
    /// fails the tag rather than opening.
    #[must_use]
    pub fn versioned(workspace_id: &str, key_name: &str, kek_version: i32) -> Self {
        let mut bytes = workspace_id.to_ascii_lowercase().into_bytes();
        bytes.push(SEPARATOR);
        bytes.extend_from_slice(key_name.as_bytes());
        bytes.push(SEPARATOR);
        bytes.extend_from_slice(kek_version.to_string().as_bytes());
        Self(bytes)
    }

    /// The checkpoint owner supplies its separate, versioned domain format.
    /// The public vault constructors retain their original stored bytes.
    pub(crate) const fn checkpoint(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Builds associated data from arbitrary bytes, for standard test vectors.
    ///
    /// Behind `test-util` (`M-TEST-UTIL`) because the public vault API has two
    /// constructors, both preserving its stored format. The private checkpoint
    /// owner supplies its separate domain. Published AES-GCM vectors carry their own
    /// associated data — often empty — which no canonical constructor can
    /// produce, so proving the primitive against them needs this door and
    /// nothing else does.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub const fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// The associated-data bytes as the AEAD consumes them.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Renders the associated data, which carries no secret material.
///
/// A workspace identifier and a credential NAME are both loggable; the value
/// under that name is what is sensitive, and it never reaches this type.
impl std::fmt::Debug for Aad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Aad({})",
            String::from_utf8_lossy(&self.0).escape_debug()
        )
    }
}
