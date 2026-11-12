//! The store provider resolution reads through: one pool and the vault that opens its rows.
//!
//! Same shape as [`afd_billing::Accounts`] and [`crate::lease::Leases`], for
//! the same reason: the pool is OWNED here and [`Providers::pool`] is
//! `pub(crate)`, so nothing outside this crate can run a statement that is not
//! in [`crate::sql::provider`], and the side-by-side parity read of that module
//! stays meaningful (Invariant 5).

use std::sync::Arc;

use af_crypto::entropy::Entropy;
use af_crypto::secret::Kek;
use afd_db::Db;

use afd_vault::Vault;

/// Reads that resolve a tenant's provider, over the api-role pool.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool and the key is
/// behind an `Arc`, so every clone shares one connection set and one key.
///
/// No queue. RESOLUTION is Postgres and arithmetic — it mints no identifier
/// and publishes nothing — which is what lets the whole interpretation half be
/// proven with neither datastore in the picture. Activation is the one verb
/// here that mints, and it carries the entropy for exactly that.
#[derive(Debug, Clone)]
pub struct Providers {
    database: Db,
    vault: Vault,
    entropy: Entropy,
}

impl Providers {
    /// A store reading through `database`, opening envelopes under `kek`.
    ///
    /// Entropy because activation MINTS: the registry entry it guarantees
    /// carries a fresh identifier, and it is drawn here rather than by the
    /// database so the id is a uuidv7 the schema's own check accepts.
    #[must_use]
    pub fn new(database: Db, kek: Arc<Kek>, entropy: Entropy) -> Self {
        Self {
            vault: Vault::new(database.clone(), kek, entropy.clone()),
            database,
            entropy,
        }
    }

    /// The source a minted registry-entry identifier is drawn from.
    pub(crate) const fn entropy(&self) -> &Entropy {
        &self.entropy
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
