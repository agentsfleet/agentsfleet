//! Purging private text preserves content-free replay and accounting obligations.
//!
//! Attempts intentionally have no draft foreign key. A later accounting report
//! can settle them, but absence of this draft fences all further private output.
//! Replay receipts carry no foreign key either: a retry after the purge finds
//! its receipt and no draft, and answers Gone.

use crate::{Access, Drafts, Result, access, draft::read, error::raise::query, sql};
use afd_core::id::Uuid7;
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "purge private draft text";

impl Drafts {
    /// Deletes the creator's private draft, transcript, candidates and stored results.
    ///
    /// # Errors
    /// Hides foreign/revoked drafts and propagates failed durable deletion.
    /// Shared fleet/Library snapshots and accounting are separate copies.
    pub async fn delete(&self, access: Access<'_>, id: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        read::lock(&mut tx, access, &person, id).await?;
        purge(&mut tx, id).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}

pub(crate) async fn purge(connection: &mut PgConnection, id: &Uuid7) -> Result<()> {
    sqlx::query(sql::draft::WIPE_DISPATCHES)
        .bind(id.as_str())
        .execute(&mut *connection)
        .await
        .map_err(query(CONTEXT))?;
    sqlx::query(sql::draft::DELETE)
        .bind(id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
