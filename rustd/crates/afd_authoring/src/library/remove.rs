//! Deleting a reusable entry hides history and preserves already installed pins.

use super::{CONTEXT, read};
use crate::{Access, Result, Setup, access, error::raise::query, replay};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

pub(super) const REVISIONS: &str =
    "SELECT id::text FROM core.fleet_library_revisions WHERE library_id=$1::uuid";
pub(super) const REMOVE: &str = "DELETE FROM core.fleet_libraries WHERE id=$1::uuid";
pub(super) const UNREFERENCED: &str =
    "DELETE FROM core.fleet_library_revisions r WHERE library_id=$1::uuid
AND NOT EXISTS(SELECT 1 FROM core.fleet_definition_revisions f WHERE f.library_revision_id=r.id)";

impl Setup {
    /// Removes an owned entry and unreferenced history, retaining fleet snapshots.
    ///
    /// # Errors
    /// Hides platform/foreign entries and preserves transaction failures.
    pub async fn delete_library(&self, access: Access<'_>, library: &str) -> Result<()> {
        read::valid_id(library)?;
        let mut connection = self.drafts.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        let entry = read::entry(&mut tx, &person.owner, library, true).await?;
        let ids: Vec<String> = sqlx::query_scalar(REVISIONS)
            .bind(entry.id.as_str())
            .fetch_all(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        for id in ids {
            replay::purge(&mut tx, &Uuid7::parse(&id)?).await?;
        }
        sqlx::query(REMOVE)
            .bind(entry.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        sqlx::query(UNREFERENCED)
            .bind(entry.id.as_str())
            .execute(&mut *tx)
            .await
            .map_err(query(CONTEXT))?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}
