//! A workspace's secrets: the sealed write, the list that never decrypts, and
//! the reference lock a delete is taken under.
//!
//! # Why this is its own crate
//!
//! `afd_credential::vault` already reads `vault.secrets`, and it stays where it is.
//! That reader is the RUNNER plane's: it opens a credential a fleet declared,
//! refuses to degrade a row it cannot read, and never lists. This is the
//! workspace-ADMIN plane: it seals, it lists without a key, and it holds the
//! lock a delete must be taken under. Two failure policies over one table, and
//! folding them together would mean one of the two policies losing.
//!
//! The other two candidates are worse for the reasons `afd_fleet_lifecycle`
//! already recorded. `afd_fleet` is thirty thousand lines, so an edit to a
//! projection would rebuild the whole runner plane. `afd_state` is the
//! credential directory the authentication path cannot start without, and
//! putting AES-GCM behind it would rebuild every login when a `meta_*` column
//! moved.
//!
//! # Nothing here opens an envelope
//!
//! The workspace secret surface is four verbs — create, list, replace, delete —
//! and not one of them returns a stored value. A secret is write-only by
//! contract, so this crate SEALS and never opens, and spec Invariant 3's "list
//! reads perform zero decrypt calls" holds for every verb rather than for one.
//!
//! It is enforced twice over, independently:
//!
//! - [`Directory`] holds no key. `Envelope::open` takes a `&Kek`, so the read
//!   and delete half cannot decrypt — not "does not", cannot. A test proves it
//!   by listing a workspace's secrets through a `Directory` built in a process
//!   holding no key at all.
//! - [`sql::SELECT_SECRET_PROJECTIONS`] projects no ciphertext column, pinned
//!   by a unit test, so there is nothing to decrypt even if a key appeared.
//!
//! `crypto_store.zig` reaches the same guarantee with a `decrypt_tally` counter
//! and a `noteDecrypt` funnel every decrypt site must remember to call. A
//! counter proves what happened on the run that was measured; a missing field
//! proves what can happen at all.
//!
//! # What it does not contain
//!
//! No routing, no HTTP, no extractor. Whether a caller MAY act on a workspace
//! is decided at the edge, in `afd_api`, by a layer mounted from the route's own
//! template. This crate answers what a verb DOES once that decision is made —
//! and re-answers the narrower question of whether the SECRET is in that
//! workspace, in SQL, because that one it can enforce rather than trust.
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![deny(unused_crate_dependencies)]
// Named for the lint's benefit: the integration lane's runtime is a
// dev-dependency, and `unused_crate_dependencies` counts one against the LIB
// target unless the crate root says it knows about it.
#[cfg(test)]
use tokio as _;

// `error` is public because the router suites name `error::detail::*` — the
// sentence a refusal carries is a wire fact, and asserting on it beats
// respelling it. The other two are private modules whose worthwhile types are
// re-exported below, so the crate has one import path per name rather than two.
pub mod error;

mod delete;
mod projection;
mod read;
mod secret;
mod sql;
mod write;

use std::sync::Arc;

use afd_crypto::entropy::Entropy;
use afd_crypto::envelope::Sealer;
use afd_crypto::secret::Kek;
use afd_db::Db;

pub use self::delete::Deleted;
pub use self::error::{Error, Result};
pub use self::projection::Kind;
pub use self::read::SecretSummary;
pub use self::secret::{MAX_DATA_BYTES, SecretBody, SecretName};

/// The key-less half of the vault: what a workspace holds, and removing one.
///
/// Constructible on its own, and that is the point rather than a convenience.
/// A value of this type cannot decrypt anything, because opening an envelope
/// needs a [`Kek`] and there is no field here to hold one — so "the list
/// performs zero decrypts" is a property of the type a reader can check by
/// looking at it, not a property of a code path they would have to follow.
///
/// Cheap to clone: [`Db`] is a handle over an `Arc`-backed pool, so every clone
/// shares one connection set.
#[derive(Debug, Clone)]
pub struct Directory {
    database: Db,
}

impl Directory {
    /// A directory reading and deleting through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }
}

/// The whole workspace secret surface: the sealing half over the key-less one.
///
/// # Why the Key Encryption Key is a field, and behind an `Arc`
///
/// `crypto_primitives.zig` keeps it in a file-scoped `var g_kek`, set at boot
/// and read back through a fallible `loadKek()` — so every write path carries a
/// `MissingMasterKey` arm for a condition that can only occur before the daemon
/// serves traffic. Here it is a field: a [`Vault`] cannot be constructed
/// without one, so there is no "not yet resolved" state and no arm to write.
///
/// `Arc` rather than a clone, because [`Kek`] is `Clone` and cloning it would
/// copy thirty-two bytes of key material into every request-path handle, each
/// zeroed at a different moment. Behind an `Arc` there is one copy, zeroed once
/// when the last handle drops.
#[derive(Debug, Clone)]
pub struct Vault {
    directory: Directory,
    kek: Arc<Kek>,
    /// Draws the per-row Data Encryption Key and both nonces.
    sealer: Sealer,
    /// Draws the row identifier, which is not key material.
    ///
    /// Separate from [`Vault::sealer`] because `afd_crypto` draws that
    /// distinction itself: `Sealer` takes key and nonce bytes where it uses
    /// them so a caller never holds them, and [`Entropy`] is the public face
    /// for the non-secret identifiers a daemon mints. One field for each keeps
    /// the two uses from being read as one.
    entropy: Entropy,
}

impl Vault {
    /// Binds the vault to an already-connected pool and a resolved key.
    #[must_use]
    pub fn new(database: Db, kek: Arc<Kek>, entropy: Entropy) -> Self {
        Self {
            directory: Directory::new(database),
            kek,
            sealer: Sealer::new(),
            entropy,
        }
    }

    /// The key-less half — the list, and the delete.
    ///
    /// Handed out rather than re-exposed as methods here, so a caller that only
    /// reads holds a value that only reads.
    #[must_use]
    pub const fn directory(&self) -> &Directory {
        &self.directory
    }
}
