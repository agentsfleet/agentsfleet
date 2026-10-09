//! One store over `vault.secrets`: the row, the envelope, the plaintext.
//!
//! # What this store leaves to others
//!
//! It opens one named row, or the set a fleet declared, and nothing more:
//!
//! - The envelope layout, the two AEAD opens and every fixed-width length check
//!   are [`afd_crypto::envelope::Envelope`]'s, proven against published NIST
//!   vectors.
//! - Reading a whole workspace for the credential LIST endpoint is not this
//!   store's job. Both callers in this crate abort on an unreadable row,
//!   because a fleet must never run with a credential it declared and cannot
//!   read.
//! - Nothing here is a read path that must prove it never decrypts — a lease
//!   decrypts by definition — so there is no decrypt count to assert.
//!
//! # A missing row is `Ok(None)`, and a missing NAME is the caller's word
//!
//! Each caller names an absence in its own word: provider resolution as a
//! missing provider secret, the secrets map as a missing declared credential.
//! Callers wanting different words for one absence is the shape an `Option`
//! has and an error does not, so absence arrives as `None` and each caller
//! names it one line up.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use afd_vault::StoredEnvelope;
use sqlx::FromRow as _;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, query, vault_open};

pub mod rotate;
pub mod sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRET: &str = "vault credential";

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRETS: &str = "vault credentials";

/// Where one credential is held.
///
/// A pair rather than two arguments, because both are strings and they compile
/// clean in either order — and getting them the wrong way round would ask the
/// vault for a workspace named after a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRef<'a> {
    /// The workspace holding the row.
    pub workspace_id: &'a Uuid7,
    /// The row's name within it.
    pub name: &'a str,
}

/// One credential recovered from a batch read.
#[derive(Debug)]
pub struct Held {
    /// The name it is stored under.
    pub name: Box<str>,
    /// Its plaintext, wiped when this value is dropped.
    pub plaintext: SecretBytes,
}

/// Envelope reads over `vault.secrets`, under one process key.
///
/// Cheap to clone: `Db` is a handle over an `Arc`-backed pool and the key is
/// behind an `Arc`, so every clone shares one connection set and one key.
///
/// # Why the Key Encryption Key is a field
///
/// A process global would carry the failure mode a process global has: every
/// read is fallible because the variable might not have been set yet, so every
/// read path would carry a "missing master key" arm for a condition that can
/// only occur before the daemon serves traffic.
///
/// Here it is a field. A [`Vault`] cannot be constructed without one, so there
/// is no "not yet resolved" state to answer for and no arm to write: boot
/// either produced a key and built this value, or it refused to start. That is
/// the move [`Kek`] itself makes about mutation — the invariant becomes the
/// type — applied one level up, to availability.
///
/// `Arc` rather than a clone of the key: [`Kek`] is `Clone`, and cloning it
/// would copy thirty-two bytes of key material into every request-path handle,
/// each zeroed at a different moment. Behind an `Arc` there is one copy, zeroed
/// once when the last handle drops.
#[derive(Debug, Clone)]
pub struct Vault {
    database: Db,
    kek: Arc<Kek>,
}

impl Vault {
    /// A vault reading through `database`, opening envelopes under `kek`.
    #[must_use]
    pub const fn new(database: Db, kek: Arc<Kek>) -> Self {
        Self { database, kek }
    }

    /// The pool this vault reads and writes through.
    ///
    /// Private to the module tree: the sibling write path needs it, and nothing
    /// outside `vault` may reach a connection that can UPDATE a credential.
    const fn pool(&self) -> &Db {
        &self.database
    }

    /// The key every envelope here is sealed and opened under.
    fn kek(&self) -> &Kek {
        &self.kek
    }

    /// The plaintext of the credential at `key`, or nothing if none is held.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row whose ciphertext
    /// columns are not a well-formed envelope, and an envelope that does not
    /// authenticate. The last two are deliberately indistinguishable to a
    /// caller: which check failed is an oracle, and the operator gets the
    /// distinction in the log instead.
    pub async fn open(&self, key: KeyRef<'_>) -> Result<Option<SecretBytes>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::SELECT_SECRET)
            .bind(key.workspace_id.as_str())
            .bind(key.name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRET))?;

        row.map(|row| self.decrypt(&row, key)).transpose()
    }

    /// Every credential in `names` this workspace holds, in ONE read.
    ///
    /// One round trip rather than one per declared name, which is what the
    /// lease's per-name loop cost. Rows arrive in whatever order Postgres
    /// returns them and are matched back by name at the call site — the
    /// statement carries no ORDER BY, so imposing one here would be inventing a
    /// guarantee the SQL does not make.
    ///
    /// A name with no row is simply ABSENT from the result. Whether that is
    /// fatal is the caller's to decide, and for the secrets map it is.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and any row whose envelope
    /// will not open — see the module note on why this does not degrade.
    pub async fn open_many(&self, workspace_id: &Uuid7, names: &[&str]) -> Result<Vec<Held>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::SELECT_SECRETS_BY_NAMES)
            .bind(workspace_id.as_str())
            .bind(names)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRETS))?;

        rows.iter()
            .map(|row| {
                let name: String = row.try_get("key_name").map_err(query(CONTEXT_SECRETS))?;
                let key = KeyRef {
                    workspace_id,
                    name: &name,
                };
                let plaintext = self.decrypt(row, key)?;
                Ok(Held {
                    name: name.into_boxed_str(),
                    plaintext,
                })
            })
            .collect()
    }

    /// Decode by column name without opening a connection or changing the caller's transaction.
    fn decrypt(&self, row: &PgRow, key: KeyRef<'_>) -> Result<SecretBytes> {
        StoredEnvelope::from_row(row)
            .map_err(query(CONTEXT_SECRET))?
            .into_envelope()
            .map_err(vault_open)?
            .open(&self.kek, &Aad::new(key.workspace_id.as_str(), key.name))
            .map_err(vault_open)
    }
}
