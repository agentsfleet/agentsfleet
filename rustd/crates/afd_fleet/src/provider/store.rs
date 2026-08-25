//! The store provider resolution reads through: one pool, one process key.
//!
//! Same shape as [`crate::money::Accounts`] and [`crate::lease::Leases`], for
//! the same reason: the pool is OWNED here and [`Providers::pool`] is
//! `pub(crate)`, so nothing outside this crate can run a statement that is not
//! in [`crate::sql::provider`], and the side-by-side parity read of that module
//! stays meaningful (Invariant 5).
//!
//! # Why the Key Encryption Key lives on the store
//!
//! `crypto_primitives.zig` keeps it in a file-scoped `var g_kek`, resolved at
//! boot by `serve.run` and read back through `loadKek()` — a process global,
//! with the failure mode a process global has: `loadKek` is fallible at every
//! call site because the variable might not have been set yet, so every read
//! path carries a `MissingMasterKey` arm for a condition that can only occur
//! before the daemon serves traffic.
//!
//! Here it is a field. A [`Providers`] cannot be constructed without one, so
//! there is no "not yet resolved" state to answer for and no arm to write: boot
//! either produced a key and built this value, or it refused to start. That is
//! the same move [`afd_crypto::secret::Kek`] makes about mutation — the
//! invariant becomes the type — applied one level up to availability.
//!
//! # `Arc`, not `Clone`
//!
//! [`Kek`] is `Clone`, and cloning it would copy thirty-two bytes of key
//! material into every clone of this store — one per request-path handle, each
//! zeroed at a different moment. Behind an `Arc` there is exactly one copy, and
//! it is zeroed once when the last handle drops.

use std::sync::Arc;

use afd_crypto::secret::Kek;
use afd_db::Db;

use crate::vault::Vault;

/// Reads that resolve a tenant's provider, over the api-role pool.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool and the key is
/// behind an `Arc`, so every clone shares one connection set and one key.
///
/// No queue and no entropy. Resolution is Postgres and arithmetic — it mints
/// no identifier and publishes nothing — which is what lets the whole
/// interpretation half be proven with neither datastore in the picture.
#[derive(Debug, Clone)]
pub struct Providers {
    database: Db,
    vault: Vault,
}

impl Providers {
    /// A store reading through `database`, opening envelopes under `kek`.
    #[must_use]
    pub fn new(database: Db, kek: Arc<Kek>) -> Self {
        Self {
            vault: Vault::new(database.clone(), kek),
            database,
        }
    }

    /// The pool these reads run through, for the sibling modules that add
    /// verbs to [`Providers`] in their own files.
    pub(crate) const fn pool(&self) -> &Db {
        &self.database
    }

    /// The vault a resolved credential is opened out of.
    pub(crate) const fn vault(&self) -> &Vault {
        &self.vault
    }
}
