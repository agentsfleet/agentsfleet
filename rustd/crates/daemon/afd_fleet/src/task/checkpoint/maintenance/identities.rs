//! Event namespaces end after their objects and outstanding purge custody end.

use crate::{
    Result,
    error::query,
    task::{Tasks, transaction},
};
use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;

const BATCH: i64 = 100;

#[derive(sqlx::FromRow)]
struct Identity {
    id: Uuid7,
    workspace_id: Uuid7,
    repository: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
}

pub(super) async fn retain(
    tasks: &Tasks,
    cursor: &mut crate::task::CheckpointMaintenanceCursor,
) -> Result<(u64, u64)> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let rows: Vec<Identity> = sqlx::query_as(include_str!("../sql/identity_retention.sql"))
        .bind(super::super::TERMINAL_SECONDS)
        .bind(cursor.identity_after.as_ref().map(|(at, _)| at.to_sqlx()))
        .bind(cursor.identity_after.as_ref().map(|(_, id)| id))
        .bind(BATCH)
        .fetch_all(&mut **connection)
        .await
        .map_err(query("page old checkpoint identity namespaces"))?;
    transaction::recycle(connection);
    if rows.is_empty() {
        cursor.identity_after = None;
    }
    let (mut removed, mut deferred) = (0, 0);
    for row in rows {
        cursor.identity_after = Some((row.created_at, row.id.clone()));
        if row
            .repository
            .strip_prefix("turn:")
            .is_none_or(|id| Uuid7::parse(id).is_err())
        {
            continue;
        }
        match remove(tasks, &row).await {
            Ok(count) => removed += count,
            Err(failure) => {
                deferred += 1;
                tracing::warn!(event = "turn_identity_retention_deferred", identity_id = %row.id, error_code = %failure.code());
            }
        }
    }
    Ok((removed, deferred))
}

async fn remove(tasks: &Tasks, identity: &Identity) -> Result<u64> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    // Both registration families take this quota lock before creating a key or object.
    let removed = if super::lock(&mut tx, &identity.workspace_id).await? {
        sqlx::query(include_str!("../sql/identity_delete.sql"))
            .bind(&identity.id)
            .bind(&identity.workspace_id)
            .bind(&identity.repository)
            .bind(super::super::TERMINAL_SECONDS)
            .execute(&mut *tx)
            .await
            .map_err(query(
                "erase an unused event identity after purge custody ends",
            ))?
            .rows_affected()
    } else {
        0
    };
    tx.commit()
        .await
        .map_err(query("commit event identity retention"))?;
    transaction::recycle(connection);
    Ok(removed)
}
