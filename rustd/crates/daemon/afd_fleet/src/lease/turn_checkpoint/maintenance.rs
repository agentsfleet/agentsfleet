//! Eligible bounded passes preserve every live reader and writer before erasing private state.

use super::transaction;
use crate::{Result, error::query, lease::Leases};
use af_core::id::Uuid7;
use sqlx::Connection as _;

const RETENTION_SECONDS: i32 = 7 * 24 * 60 * 60;
const BATCH: i64 = 100;
const PLANS_PER_OWNER: i64 = 64;

#[derive(sqlx::FromRow)]
struct Candidate {
    #[sqlx(rename = "event_id")]
    event: Uuid7,
    #[sqlx(rename = "fleet_id")]
    fleet: Uuid7,
    #[sqlx(rename = "workspace_id")]
    workspace: Uuid7,
}

impl Leases {
    pub(crate) async fn maintain_turn_checkpoints(&self) -> Result<u64> {
        let mut connection = transaction::acquire(self.pool()).await?;
        let candidates: Vec<Candidate> =
            sqlx::query_as(include_str!("sql/maintenance_candidates.sql"))
                .bind(RETENTION_SECONDS)
                .bind(BATCH)
                .fetch_all(&mut **connection)
                .await
                .map_err(query("select actionable turn checkpoint owners"))?;
        transaction::recycle(connection);
        let mut removed = 0;
        for candidate in candidates {
            removed += retain(self, &candidate).await?;
        }
        let mut connection = transaction::acquire(self.pool()).await?;
        sqlx::query(include_str!("sql/maintenance_orphans.sql"))
            .bind(RETENTION_SECONDS)
            .bind(BATCH)
            .execute(&mut **connection)
            .await
            .map_err(query(
                "collect opaque turn operations after source deletion",
            ))?;
        transaction::recycle(connection);
        Ok(removed)
    }
}

async fn retain(leases: &Leases, candidate: &Candidate) -> Result<u64> {
    let mut connection = transaction::acquire(leases.pool()).await?;
    let mut tx = connection
        .begin()
        .await
        .map_err(query("begin turn checkpoint retention"))?;
    let parent: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/lock_scope.sql"))
        .bind(&candidate.fleet)
        .bind(&candidate.workspace)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("hold turn retention parents"))?;
    let mut removed = 0;
    if parent.is_some() {
        let head: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_lock_head.sql"))
            .bind(&candidate.event)
            .fetch_optional(&mut *tx)
            .await
            .map_err(query("hold turn retention head"))?;
        if head.is_some() {
            removed = collect(&mut tx, &candidate.event).await?;
        }
    }
    tx.commit()
        .await
        .map_err(query("commit bounded turn checkpoint retention"))?;
    transaction::recycle(connection);
    Ok(removed)
}

async fn collect(connection: &mut sqlx::PgConnection, event: &Uuid7) -> Result<u64> {
    let plans: Vec<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_lock_plans.sql"))
        .bind(event)
        .bind(PLANS_PER_OWNER)
        .bind(RETENTION_SECONDS)
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
        .bind(RETENTION_SECONDS)
        .bind(&plans)
        .execute(&mut *connection)
        .await
        .map_err(query("collect only expired unpinned turn generations"))?
        .rows_affected();
    let terminal: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/maintenance_head.sql"))
        .bind(event)
        .bind(now)
        .bind(RETENTION_SECONDS)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("collect terminal turn recovery head"))?;
    if terminal.is_some() {
        sqlx::query(include_str!("sql/maintenance_operations.sql"))
            .bind(event)
            .bind(BATCH)
            .execute(connection)
            .await
            .map_err(query("collect terminal opaque turn receipts"))?;
    }
    Ok(removed)
}
