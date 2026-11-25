//! Eligible bounded passes preserve every live reader and writer before erasing private state.

use super::transaction;
use crate::{Result, error::query, run::Runs};
use af_core::id::Uuid7;
use sqlx::Connection as _;

const BATCH: i64 = 100;
const PLANS_PER_OWNER: i64 = 64;

#[derive(sqlx::FromRow)]
struct Candidate {
    #[sqlx(rename = "turn_id")]
    event: Uuid7,
    #[sqlx(rename = "fleet_id")]
    fleet: Option<Uuid7>,
    #[sqlx(rename = "workspace_id")]
    workspace: Option<Uuid7>,
    due: bool,
}

impl Runs {
    pub(crate) async fn maintain_turn_checkpoints(
        &self,
        after: &mut Option<Uuid7>,
    ) -> Result<(u64, u64)> {
        let retention = self.limits().turn_checkpoints.retention_seconds();
        let mut connection = transaction::acquire(self.pool()).await?;
        let candidates: Vec<Candidate> =
            sqlx::query_as(include_str!("sql/maintenance_candidates.sql"))
                .bind(retention)
                .bind(BATCH)
                .bind(after.as_ref())
                .fetch_all(&mut **connection)
                .await
                .map_err(query("page turn checkpoint owners"))?;
        transaction::recycle(connection);
        let wrapped = candidates.len() < usize::try_from(BATCH).unwrap_or(usize::MAX);
        let (mut removed, mut deferred) = (0, 0);
        for candidate in candidates {
            *after = Some(candidate.event.clone());
            if let (Some(fleet), Some(workspace), true) =
                (candidate.fleet, candidate.workspace, candidate.due)
            {
                match retain(self, &candidate.event, &fleet, &workspace).await {
                    Ok(count) => removed += count,
                    Err(failure) => {
                        deferred += 1;
                        let error_code = failure.code();
                        tracing::warn!(event = "turn_checkpoint_retention_deferred",
                            turn_id = %candidate.event, %error_code);
                    }
                }
            }
        }
        if wrapped {
            *after = None;
        }
        Ok((removed, deferred))
    }
}

async fn retain(runs: &Runs, event: &Uuid7, fleet: &Uuid7, workspace: &Uuid7) -> Result<u64> {
    let mut connection = transaction::acquire(runs.pool()).await?;
    let mut tx = connection
        .begin()
        .await
        .map_err(query("begin turn checkpoint retention"))?;
    let parent: Option<Uuid7> = sqlx::query_scalar(crate::durable_task::sql::LOCK_SCOPE)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold turn retention parents"))?;
    let mut removed = 0;
    if parent.is_some() {
        let head: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_lock_head.sql"))
            .bind(event)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("hold turn retention head"))?;
        if head.is_some() {
            removed = collect(
                &mut tx,
                event,
                &super::receipt_scope(workspace, event),
                runs.limits().turn_checkpoints.retention_seconds(),
            )
            .await?;
        }
    }
    tx.commit()
        .await
        .map_err(query("commit bounded turn checkpoint retention"))?;
    transaction::recycle(connection);
    Ok(removed)
}

/// Collects the turn's expired generations, then, once none is left and the
/// turn settled past retention, its receipts a batch at a time. The head clears
/// only when the scope holds no receipt, so the turn stays a candidate until
/// every key is gone and no receipt is left behind without a turn to page it.
async fn collect(
    connection: &mut sqlx::PgConnection,
    event: &Uuid7,
    scope: &str,
    retention: i32,
) -> Result<u64> {
    let plans: Vec<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_lock_plans.sql"))
        .bind(event)
        .bind(PLANS_PER_OWNER)
        .bind(retention)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("hold turn retention generations"))?;
    let _pins: Vec<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_lock_pins.sql"))
        .bind(&plans)
        .fetch_all(&mut *connection)
        .await
        .map_err(query("hold turn retention custody"))?;
    let now: jiff_sqlx::Timestamp = sqlx::query_scalar(include_str!("sql/fresh_clock.sql"))
        .fetch_one(&mut *connection)
        .await
        .map_err(query("read post-lock turn retention clock"))?;
    let removed = sqlx::query(include_str!("sql/maintenance_delete.sql"))
        .bind(event)
        .bind(now)
        .bind(retention)
        .bind(&plans)
        .execute(&mut *connection)
        .await
        .map_err(query("collect only expired unpinned turn generations"))?
        .rows_affected();
    sqlx::query(include_str!("sql/maintenance_operations.sql"))
        .bind(scope)
        .bind(BATCH)
        .bind(event)
        .bind(now)
        .bind(retention)
        .execute(&mut *connection)
        .await
        .map_err(query("collect terminal opaque turn receipts"))?;
    sqlx::query(include_str!("sql/maintenance_head.sql"))
        .bind(event)
        .bind(now)
        .bind(retention)
        .bind(scope)
        .execute(connection)
        .await
        .map_err(query("collect terminal turn recovery head"))?;
    Ok(removed)
}
