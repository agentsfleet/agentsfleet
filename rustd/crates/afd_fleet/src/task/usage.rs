//! Accounting accepts original measured costs independently of semantic authority.

use afd_billing::Nanos;
use afd_wire::task::TaskState;
use sqlx::PgConnection;
use crate::{Result, error::query};
use super::sql;

const NANOS_PER_SECOND: i64 = 1_000_000_000;

#[derive(sqlx::FromRow)]
struct Owner { task_id: String, fleet_id: String, workspace_id: String }

pub(crate) async fn record(connection: &mut PgConnection, lease: &str, model_nanos: Nanos) -> Result<bool> {
    let owner: Option<Owner> = sqlx::query_as(include_str!("sql/usage_owner.sql")).bind(lease)
        .fetch_optional(&mut *connection).await.map_err(query("read original task usage owner"))?;
    let Some(owner) = owner else { return Ok(true); };
    // Existing metering already owns slot/lease. No host or parent lock follows this task lock.
    let _task: Option<String> = sqlx::query_scalar(sql::LOCK_TASK).bind(&owner.task_id).bind(&owner.fleet_id).bind(&owner.workspace_id)
        .fetch_optional(&mut *connection).await.map_err(query("hold original task while accumulating measured usage"))?;
    let allowed: Option<bool> = sqlx::query_scalar(include_str!("sql/usage_record.sql")).bind(lease).bind(model_nanos.as_i64()).bind(NANOS_PER_SECOND)
        .bind(TaskState::Exhausted.as_str()).bind(TaskState::Running.as_str())
        .fetch_optional(connection).await.map_err(query("record original task usage without accepting stale semantics"))?;
    Ok(allowed.unwrap_or(false))
}
