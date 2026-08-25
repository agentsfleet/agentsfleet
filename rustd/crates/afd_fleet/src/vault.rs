//! One store over `vault.secrets`: the row, the envelope, the plaintext.
//!
//! # What does NOT come across from `crypto_store.zig`
//!
//! That file is two hundred and sixty-six lines and this one is a fraction of
//! it, because most of what it does is already done elsewhere or does not
//! belong on this path:
//!
//! - The envelope layout, the two AEAD opens and every fixed-width length check
//!   are [`afd_crypto::envelope::Envelope`]'s, proven against published NIST
//!   vectors and against the Zig's own assertions.
//! - `loadAllForWorkspace` reads a whole workspace for the credential LIST
//!   endpoint, which this milestone does not own. Its per-row degradation — a
//!   damaged envelope becomes a null plaintext rather than failing the read —
//!   exists so that page still answers 200, and it is deliberately absent here:
//!   both callers in this crate abort on an unreadable row, because a fleet
//!   must never run with a credential it declared and cannot read.
//! - The `decrypt_tally` and its `noteDecrypt` funnel exist to prove the
//!   LIBRARY read paths never decrypt. Nothing in this crate is a library read
//!   path — a lease decrypts by definition — so the tally has nothing to assert
//!   here and is not carried.
//!
//! # A missing row is `Ok(None)`, and a missing NAME is the caller's word
//!
//! `crypto_store.load` answers `SecretError.NotFound`, and every caller catches
//! it and renames it: `PlatformKeyMissing` on the platform path,
//! `SecretMissing` on the self-managed one, `CredentialNotFound` in the secrets
//! map. Three callers wanting three words for one absence is the shape an
//! `Option` has and an error does not, so absence arrives as `None` and the
//! naming happens one line up instead of in a `catch` at each site.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_crypto::aad::Aad;
use afd_crypto::envelope::Envelope;
use afd_crypto::secret::{Kek, SecretBytes};
use afd_db::Db;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use crate::error::{Result, query, vault_open};
use crate::sql;

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRET: &str = "vault credential";

/// Statement name, for the context a query failure carries.
const CONTEXT_SECRETS: &str = "vault credentials";

/// Where the envelope's six components start in [`sql::vault::SELECT_SECRET`].
const ENVELOPE_AT: usize = 0;

/// Where they start in [`sql::vault::SELECT_SECRETS_BY_NAMES`], which projects
/// the name and the creation instant first.
///
/// The offset is the whole reason one decrypt routine serves both statements,
/// and it is why the batch statement's column order is copied rather than
/// tidied — see that statement's own note.
const ENVELOPE_AT_BATCH: usize = 2;

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
/// `crypto_primitives.zig` keeps it in a file-scoped `var g_kek`, resolved at
/// boot by `serve.run` and read back through `loadKek()` — a process global,
/// with the failure mode a process global has: `loadKek` is fallible at every
/// call site because the variable might not have been set yet, so every read
/// path carries a `MissingMasterKey` arm for a condition that can only occur
/// before the daemon serves traffic.
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

    /// The plaintext of the credential at `key`, or nothing if none is held.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, a row whose ciphertext
    /// columns are not a well-formed envelope, and an envelope that does not
    /// authenticate. The last two are deliberately indistinguishable to a
    /// caller: which check failed is an oracle, and the operator gets the
    /// distinction in the log instead.
    pub(crate) async fn open(&self, key: KeyRef<'_>) -> Result<Option<SecretBytes>> {
        let mut connection = self.database.acquire().await?;
        let row = sqlx::query(sql::vault::SELECT_SECRET)
            .bind(key.workspace_id.as_str())
            .bind(key.name)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRET))?;

        row.map(|row| self.decrypt(&row, ENVELOPE_AT, key))
            .transpose()
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
    pub(crate) async fn open_many(
        &self,
        workspace_id: &Uuid7,
        names: &[&str],
    ) -> Result<Vec<Held>> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let mut connection = self.database.acquire().await?;
        let rows = sqlx::query(sql::vault::SELECT_SECRETS_BY_NAMES)
            .bind(workspace_id.as_str())
            .bind(names)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_SECRETS))?;

        rows.iter()
            .map(|row| {
                let name: String = row.try_get(0).map_err(query(CONTEXT_SECRETS))?;
                let key = KeyRef {
                    workspace_id,
                    name: &name,
                };
                let plaintext = self.decrypt(row, ENVELOPE_AT_BATCH, key)?;
                Ok(Held {
                    name: name.into_boxed_str(),
                    plaintext,
                })
            })
            .collect()
    }

    /// One row's envelope, rebuilt from its columns and opened.
    ///
    /// Columns are read POSITIONALLY, because the order is the contract this
    /// shares with `openEnvelopeAt`: the statement's projection and
    /// [`Envelope::from_parts`]' parameter list are the same six components in
    /// the same sequence. Reading them by name would hide a projection that had
    /// drifted out of that order, which is the one way this can go wrong
    /// silently.
    fn decrypt(&self, row: &PgRow, at: usize, key: KeyRef<'_>) -> Result<SecretBytes> {
        let column = |index: usize| {
            row.try_get::<Vec<u8>, _>(index)
                .map_err(query(CONTEXT_SECRET))
        };
        Envelope::from_parts(
            column(at)?,
            &column(at + 1)?,
            &column(at + 2)?,
            &column(at + 3)?,
            column(at + 4)?,
            &column(at + 5)?,
            row.try_get(at + 6).map_err(query(CONTEXT_SECRET))?,
        )
        .map_err(vault_open)?
        .open(&self.kek, &Aad::new(key.workspace_id.as_str(), key.name))
        .map_err(vault_open)
    }
}
