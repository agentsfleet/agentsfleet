//! Purging private text preserves content-free replay and accounting obligations.
//!
//! Attempts intentionally have no draft foreign key. A later accounting report
//! can settle them, but absence of this draft fences all further private output.

use crate::{Access, Drafts, Result, access, draft::read, error::raise::query, replay, sql};
use afd_core::{clock::UnixMillis, id::Uuid7};
use sqlx::{Connection as _, PgConnection};

const CONTEXT: &str = "purge private draft text";
const WIPE_DISPATCHES: &str = "\
UPDATE fleet.authoring_dispatches SET result = NULL,
private_expires_at = $2, updated_at = $2
WHERE attempt_id IN (SELECT id FROM fleet.authoring_attempts WHERE draft_id = $1::uuid)";

impl Drafts {
    /// Deletes the creator's private draft, transcript, candidates and stored results.
    ///
    /// # Errors
    /// Hides foreign/revoked drafts and propagates failed durable deletion.
    /// Shared fleet/Library snapshots and accounting are separate copies.
    pub async fn delete(&self, access: Access<'_>, id: &Uuid7, now: UnixMillis) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let person = access::current(&mut tx, access).await?;
        read::lock(&mut tx, access, &person, id).await?;
        purge(&mut tx, id, now).await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}

pub(crate) async fn purge(
    connection: &mut PgConnection,
    id: &Uuid7,
    now: UnixMillis,
) -> Result<()> {
    replay::purge(connection, id).await?;
    sqlx::query(WIPE_DISPATCHES)
        .bind(id.as_str())
        .bind(now.as_millis())
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
