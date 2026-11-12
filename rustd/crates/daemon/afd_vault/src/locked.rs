//! The two reads and the one write the credential plane makes under locks it
//! holds: setup inspection on the caller's transaction, and a rewrite of one
//! row under its own `FOR UPDATE`.
//!
//! This crate owns the statements and the envelopes; the caller owns what the
//! plaintext means. A refresh-token rotation is a [`Vault::rewrite`] whose edit
//! compares and swaps one field, and inspection is [`Vault::open_shared_on`]
//! whose rows the caller classifies.

use af_core::id::Uuid7;
use af_crypto::aad::Aad;
use af_crypto::secret::SecretBytes;
use sqlx::{Acquire as _, PgConnection};

use crate::error::{Result, query};
use crate::secret::{SecretBody, SecretName};
use crate::write::update_row;
use crate::{StoredEnvelope, Vault, sql};

const CONTEXT_SHARED: &str = "opening declared secrets under a share lock";
const CONTEXT_REWRITE: &str = "rewriting one secret under its row lock";

/// One opened secret and a generation digest bound to its ciphertext.
///
/// `Debug` names the row and its generation; the plaintext prints redacted.
#[derive(Debug)]
pub struct Generation {
    /// The row's name.
    pub name: String,
    /// What it holds, zeroed on drop.
    pub plaintext: SecretBytes,
    /// A digest of the row's id, key version and ciphertext: it changes when
    /// the secret does, and reveals nothing about what it holds.
    pub generation: String,
}

/// What one [`Vault::rewrite`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rewritten {
    /// The row now holds the edit's plaintext.
    Replaced,
    /// The edit chose to leave the row as it was.
    Kept,
    /// No such row: nothing was written, and nothing was created.
    Absent,
}

#[derive(sqlx::FromRow)]
struct SharedRow {
    key_name: String,
    #[sqlx(flatten)]
    envelope: StoredEnvelope,
    resolution_version: String,
}

impl Vault {
    /// Opens `names` in `workspace` under a share lock on the caller's
    /// transaction, each with its generation. A name this workspace does not
    /// hold is simply absent from the answer.
    ///
    /// # Errors
    /// A statement that would not run, a row whose envelope columns are not the
    /// widths this build reads, and an envelope that would not open.
    pub async fn open_shared_on(
        &self,
        connection: &mut PgConnection,
        workspace: &Uuid7,
        names: &[String],
    ) -> Result<Vec<Generation>> {
        let rows = sqlx::query_as::<_, SharedRow>(sql::SELECT_SHARED_GENERATIONS)
            .bind(workspace)
            .bind(names)
            .fetch_all(connection)
            .await
            .map_err(query(CONTEXT_SHARED))?;
        rows.into_iter()
            .map(|row| {
                let plaintext = row
                    .envelope
                    .into_envelope()?
                    .open(&self.kek, &Aad::new(workspace.as_str(), &row.key_name))?;
                Ok(Generation {
                    name: row.key_name,
                    plaintext,
                    generation: row.resolution_version,
                })
            })
            .collect()
    }

    /// Rewrites one secret in place, inside one transaction that holds its row
    /// lock across the read, the edit and the write.
    ///
    /// `edit` sees the current plaintext and answers the replacement, `None` to
    /// leave the row untouched, or its own refusal. The replacement is a
    /// [`SecretBody`], held to the bounds a written secret is, and it is sealed
    /// under the same workspace and name with its projection, so the lock is
    /// what makes a compare in `edit` and the write see one another.
    ///
    /// # Errors
    /// A datastore that would not answer, an envelope that would not open or
    /// re-seal. `edit`'s own refusal comes back as the inner `Err`.
    pub async fn rewrite<E>(
        &self,
        workspace: &Uuid7,
        name: &SecretName,
        edit: impl FnOnce(&SecretBytes) -> core::result::Result<Option<SecretBody>, E>,
    ) -> Result<core::result::Result<Rewritten, E>> {
        let mut connection = self.directory.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_REWRITE))?;
        let row = sqlx::query_as::<_, StoredEnvelope>(sql::LOCK_SECRET_ENVELOPE)
            .bind(workspace)
            .bind(name.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(query(CONTEXT_REWRITE))?;
        // Deleted while the caller worked: writing would resurrect a secret a
        // person removed.
        let Some(row) = row else {
            return Ok(Ok(Rewritten::Absent));
        };
        let aad = Aad::new(workspace.as_str(), name.as_str());
        let current = row.into_envelope()?.open(&self.kek, &aad)?;
        let replacement = match edit(&current) {
            Ok(Some(replacement)) => replacement,
            Ok(None) => return Ok(Ok(Rewritten::Kept)),
            Err(refused) => return Ok(Err(refused)),
        };
        let sealed = self.seal(workspace, name, &replacement)?;
        update_row(
            &mut transaction,
            workspace,
            name,
            &sealed,
            replacement.projection(),
            CONTEXT_REWRITE,
        )
        .await?;
        transaction.commit().await.map_err(query(CONTEXT_REWRITE))?;
        Ok(Ok(Rewritten::Replaced))
    }
}
