//! One daemon repository; callers prove authority on this connection's transaction.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use crate::{Checkpoint, CheckpointStore, Error, PortFuture, Result};

const LOAD: &str = include_str!("load.sql");
const INSERT: &str = include_str!("insert.sql");
const UPDATE: &str = include_str!("update.sql");

/// The identity a setup conversation's checkpoint carries, assigned by the
/// authorized host adapter: one erasable owner, the draft, so no account-wide
/// or cross-host scope can be constructed.
#[must_use]
pub fn identity(draft: &Uuid7) -> String {
    format!("draft:{draft}")
}

/// Borrow the caller's connection; no pool acquisition or authority check is hidden.
pub struct Repository<'a> {
    connection: &'a mut PgConnection,
    draft: Uuid7,
    id: Uuid7,
}

impl std::fmt::Debug for Repository<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Repository")
            .field("draft", &self.draft)
            .finish_non_exhaustive()
    }
}

impl<'a> Repository<'a> {
    /// `id` is a newly minted checkpoint row identity, used only on first insert.
    #[must_use]
    pub const fn new(connection: &'a mut PgConnection, draft: Uuid7, id: Uuid7) -> Self {
        Self {
            connection,
            draft,
            id,
        }
    }

    /// The one checkpoint the draft owns, validated, or none before the first
    /// publication.
    ///
    /// # Errors
    /// The store would not answer, or the stored document is not a valid
    /// checkpoint.
    pub async fn load(&mut self) -> Result<Option<Checkpoint>> {
        let found: Option<sqlx::types::Json<Checkpoint>> = sqlx::query_scalar(LOAD)
            .bind(&self.draft)
            .fetch_optional(&mut *self.connection)
            .await
            .map_err(Error::boundary)?;
        let checkpoint = found.map(|value| value.0);
        if let Some(value) = &checkpoint {
            value.validate_document()?;
        }
        Ok(checkpoint)
    }

    async fn publish(&mut self, expected: u64, checkpoint: Checkpoint) -> Result<()> {
        checkpoint.validate_document()?;
        if checkpoint.identity.scope != identity(&self.draft)
            || expected.checked_add(1) != Some(checkpoint.generation)
        {
            return Err(Error::stale());
        }
        let generation =
            i64::try_from(checkpoint.generation).map_err(|_overflow| Error::stale())?;
        let expected = i64::try_from(expected).map_err(|_overflow| Error::stale())?;
        // The insert names the new row's id; the update finds the row by draft.
        let statement = if expected == 0 {
            sqlx::query(INSERT).bind(&self.id)
        } else {
            sqlx::query(UPDATE)
        };
        let changed = statement
            .bind(&self.draft)
            .bind(generation)
            .bind(sqlx::types::Json(&checkpoint))
            .bind(expected)
            .execute(&mut *self.connection)
            .await
            .map_err(Error::boundary)?;
        if changed.rows_affected() == 1 || self.load().await?.as_ref() == Some(&checkpoint) {
            Ok(())
        } else {
            Err(Error::stale())
        }
    }
}

impl CheckpointStore for Repository<'_> {
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(self.publish(expected_generation, checkpoint))
    }
}
