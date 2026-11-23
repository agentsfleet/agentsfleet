//! Accounting accepts original measured costs independently of semantic authority.

use super::sql;
use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;
use afd_billing::Nanos;
use sqlx::PgConnection;

const NANOS_PER_SECOND: i64 = 1_000_000_000;

#[derive(sqlx::FromRow)]
struct Owner {
    #[sqlx(rename = "durable_task_id")]
    task: Uuid7,
    #[sqlx(rename = "fleet_id")]
    fleet: Uuid7,
    #[sqlx(rename = "workspace_id")]
    workspace: Uuid7,
}

pub(crate) async fn record(
    connection: &mut PgConnection,
    run: &Uuid7,
    model_nanos: Nanos,
) -> Result<bool> {
    let owner: Option<Owner> = sqlx::query_as(include_str!("sql/usage_owner.sql"))
        .bind(run)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read original task usage owner"))?;
    let Some(owner) = owner else {
        return Ok(true);
    };
    // Existing metering already owns thread/run. No host or parent lock follows this task lock.
    let _task: Option<String> = sqlx::query_scalar(sql::LOCK_DURABLE_TASK)
        .bind(&owner.task)
        .bind(&owner.fleet)
        .bind(&owner.workspace)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(
            "hold original task while accumulating measured usage",
        ))?;
    let allowed: Option<bool> = sqlx::query_scalar(include_str!("sql/usage_record.sql"))
        .bind(run)
        .bind(model_nanos.as_i64())
        .bind(NANOS_PER_SECOND)
        .bind(DurableTaskState::Exhausted.as_str())
        .bind(DurableTaskState::Running.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query(
            "record original task usage without accepting stale semantics",
        ))?;
    Ok(allowed.unwrap_or(false))
}
