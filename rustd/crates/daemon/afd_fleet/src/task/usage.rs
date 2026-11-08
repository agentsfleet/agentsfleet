//! Accounting accepts original measured costs independently of semantic authority.

use super::sql;
use crate::{Result, error::query};
use af_wire::task::TaskState;
use afd_billing::Nanos;
use sqlx::PgConnection;

const NANOS_PER_SECOND: i64 = 1_000_000_000;

#[derive(sqlx::FromRow)]
struct Owner {
    #[sqlx(rename = "task_id")]
    task: String,
    #[sqlx(rename = "fleet_id")]
    fleet: String,
    #[sqlx(rename = "workspace_id")]
    workspace: String,
}

pub(crate) async fn record(
    connection: &mut PgConnection,
    lease: &str,
    model_nanos: Nanos,
) -> Result<bool> {
    let owner: Option<Owner> = sqlx::query_as(include_str!("sql/usage_owner.sql"))
        .bind(lease)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read original task usage owner"))?;
    let Some(owner) = owner else {
        return Ok(true);
    };
    // Existing metering already owns slot/lease. No host or parent lock follows this task lock.
    let _task: Option<String> = sqlx::query_scalar(sql::LOCK_TASK)
        .bind(&owner.task)
        .bind(&owner.fleet)
        .bind(&owner.workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(
            "hold original task while accumulating measured usage",
        ))?;
    let allowed: Option<bool> = sqlx::query_scalar(include_str!("sql/usage_record.sql"))
        .bind(lease)
        .bind(model_nanos.as_i64())
        .bind(NANOS_PER_SECOND)
        .bind(TaskState::Exhausted.as_str())
        .bind(TaskState::Running.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(
            "record original task usage without accepting stale semantics",
        ))?;
    Ok(allowed.unwrap_or(false))
}
