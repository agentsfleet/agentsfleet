//! Private owner deletion takes the same task lock as capture and restore pins.

use crate::{
    Result,
    error::query,
    task::{TaskScope, Tasks, guard as task_guard, sql, transaction},
};
use af_core::id::Uuid7;
use jiff_sqlx::ToSqlx as _;

#[derive(sqlx::FromRow)]
struct Task {
    id: Uuid7,
    fleet_id: Uuid7,
    workspace_id: Uuid7,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    checkpoint_cleanup_at: jiff::Timestamp,
}

pub(super) async fn retain(
    tasks: &Tasks,
    cursor: &mut crate::task::CheckpointMaintenanceCursor,
) -> Result<(u64, u64)> {
    let cutoff = *cursor.cutoff.get_or_insert_with(af_core::clock::now);
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let rows: Vec<Task> = sqlx::query_as(include_str!("../sql/retention_tasks.sql"))
        .bind(cutoff.to_sqlx())
        .bind(cursor.after.as_ref().map(|(at, _id)| at.to_sqlx()))
        .bind(cursor.after.as_ref().map(|(_at, id)| id))
        .bind(super::super::GC_BATCH)
        .fetch_all(&mut **connection)
        .await
        .map_err(query("read indexed due checkpoint owners"))?;
    transaction::recycle(connection);
    if rows.is_empty() {
        cursor.after = None;
        cursor.cutoff = None;
        return Ok((0, 0));
    }
    let (mut removed, mut deferred) = (0, 0);
    for task in rows {
        // Advance before awaiting; a cancelled or contended item cannot starve later owners.
        cursor.after = Some((task.checkpoint_cleanup_at, task.id.clone()));
        match retain_one(tasks, &task).await {
            Ok(count) => removed += count,
            Err(error) => {
                deferred += 1;
                tracing::warn!(event = "dt_checkpoint_retention_deferred", task_id = %task.id, error_code = %error.code());
            }
        }
    }
    Ok((removed, deferred))
}

async fn retain_one(tasks: &Tasks, task: &Task) -> Result<u64> {
    let scope = TaskScope {
        fleet_id: task.fleet_id.clone(),
        workspace_id: task.workspace_id.clone(),
    };
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    task_guard::scope(&mut tx, &scope).await?;
    let locked: Option<String> = sqlx::query_scalar(sql::LOCK_TASK)
        .bind(&task.id)
        .bind(&task.fleet_id)
        .bind(&task.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query(
            "serialize checkpoint retention with current task state",
        ))?;
    let removed = if locked.is_some() {
        sqlx::query(include_str!("../sql/retention_abandon.sql"))
            .bind(&task.id)
            .bind(crate::lease::sql::LEASE_STATUS_ACTIVE)
            .execute(&mut *tx)
            .await
            .map_err(query("abandon expired or fenced capture owners"))?;
        let removed = sqlx::query(include_str!("../sql/retention_delete.sql"))
            .bind(&task.id)
            .bind(super::super::LIVE_GENERATIONS)
            .bind(super::super::TERMINAL_SECONDS)
            .execute(&mut *tx)
            .await
            .map_err(query(
                "remove obsolete private generations while preserving current pins",
            ))?
            .rows_affected();
        sqlx::query(include_str!("../sql/retention_next.sql"))
            .bind(&task.id)
            .bind(super::super::TERMINAL_SECONDS)
            .execute(&mut *tx)
            .await
            .map_err(query(
                "schedule only the next actual checkpoint retention deadline",
            ))?;
        removed
    } else {
        0
    };
    tx.commit()
        .await
        .map_err(query("commit private checkpoint owner retention"))?;
    transaction::recycle(connection);
    Ok(removed)
}

#[derive(sqlx::FromRow)]
struct Base {
    id: Uuid7,
    workspace_id: Uuid7,
}

pub(super) async fn bases(tasks: &Tasks) -> Result<u64> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let bases: Vec<Base> = sqlx::query_as(include_str!("../sql/base_orphans.sql"))
        .bind(super::super::ORPHAN_SECONDS)
        .bind(super::super::GC_BATCH)
        .fetch_all(&mut **connection)
        .await
        .map_err(query("read retained bases without checkpoint owners"))?;
    transaction::recycle(connection);
    let mut removed = 0;
    // Each base mutation owns one workspace quota lock; no task lock follows it.
    for base in bases {
        let mut connection = transaction::acquire(tasks.leases.pool()).await?;
        let mut tx = transaction::begin(&mut connection).await?;
        if super::lock(&mut tx, &base.workspace_id).await? {
            removed += sqlx::query(include_str!("../sql/base_delete.sql"))
                .bind(&base.id)
                .bind(&base.workspace_id)
                .bind(super::super::ORPHAN_SECONDS)
                .execute(&mut *tx)
                .await
                .map_err(query("remove only a still-unreferenced retained base"))?
                .rows_affected();
        }
        tx.commit()
            .await
            .map_err(query("commit unreferenced base retention"))?;
        transaction::recycle(connection);
    }
    Ok(removed)
}
