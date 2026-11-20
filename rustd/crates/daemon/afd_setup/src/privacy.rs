//! Purging private text preserves content-free replay and accounting deliveries.
//!
//! Replay receipts carry no foreign key to the draft: a retry after the purge
//! finds its receipt and no draft, and answers Gone. Usage lines name the run,
//! never the draft, so money outlives the purge.

use crate::{Access, Drafts, Result, access, draft::read, error::raise::query, sql};
use af_core::id::Uuid7;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "purge private draft text";

impl Drafts {
    /// Deletes the creator's private draft, transcript and candidates.
    ///
    /// # Errors
    /// Hides foreign/revoked drafts and propagates failed durable deletion.
    /// Shared fleet snapshots and accounting are separate copies.
    pub async fn delete(&self, access: Access<'_>, id: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        read::lock(&mut tx, access, &person, id).await?;
        let killed = purge(&mut tx, id).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        self.announce_cancelled(killed).await;
        Ok(())
    }
}

/// Deletes the draft and its active draft runs, answering each deleted
/// run's runner and id so the caller can tell the runner to stop.
pub(crate) async fn purge(
    connection: &mut PgConnection,
    id: &Uuid7,
) -> Result<Vec<(Uuid7, Uuid7)>> {
    // No foreign key reaches thread.runs from a draft, so its live runs go here;
    // its thread goes with the draft's own cascade.
    let killed: Vec<(Uuid7, Uuid7)> = sqlx::query_as(crate::setup_run::DELETE_RUNS)
        .bind(id)
        .bind(afd_runner::sql::RUN_STATUS_ACTIVE)
        .fetch_all(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(sql::draft::DELETE)
        .bind(id)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(killed)
}
