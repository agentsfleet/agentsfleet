//! The associated data every envelope operation binds.
//!
//! Ported byte-for-byte from `crypto_store_write.zig::buildAad`. The format is
//! three fields joined by the ASCII unit separator:
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
//! `workspace_id` is lowercased; `key_name` is not. That is what the Zig
//! implementation does — `std.ascii.allocLowerString` is applied to the
//! workspace identifier alone — and parity means copying the asymmetry rather
//! than tidying it. Lowercasing `key_name` here would make every row the Zig
//! daemon wrote with an upper-case character in its name fail to open.

use crate::KEK_VERSION;

/// The ASCII unit separator that joins the associated-data fields.
const SEPARATOR: u8 = 0x1f;

/// The associated data binding one envelope to its workspace, name and version.
#[derive(Clone, PartialEq, Eq)]
pub struct Aad(Vec<u8>);

impl Aad {
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

    /// Builds associated data from arbitrary bytes, for standard test vectors.
    ///
    /// Behind `test-util` (`M-TEST-UTIL`) because the canonical format IS this
    /// type's invariant: production has exactly two ways to build one, and both
    /// produce the daemon's format. Published AES-GCM vectors carry their own
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
