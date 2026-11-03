//! Typed writes use the transaction whose source and slot are already locked.

use afd_wire::task::{TaskState, UNFINISHED_MAX};
use sqlx::PgConnection;

use super::super::{
    PreparedPlan, PreparedTask, TaskBinding, TaskLease, TaskRefusal, guard::LeaseRow, sql,
};
use super::source::Source;
use crate::error::{Error, ErrorKind, Result, query};

const CONTEXT: &str = "write durable task binding";
const ACTION_NEW: &str = "new_work";

pub(super) async fn item(
    connection: &mut PgConnection,
    current: &LeaseRow,
    plan: &PreparedPlan,
    item: &PreparedTask,
    binding: &TaskBinding,
    ordinal: usize,
) -> Result<()> {
    let ordinal = i32::try_from(ordinal).map_err(|source| {
        Error::from(ErrorKind::TaskCounter {
            column: "ordinal",
            source,
        })
    })?;
    sqlx::query(sql::INSERT_ITEM)
        .bind(binding.item_id.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(plan.source_admission_id.as_str())
        .bind(ordinal)
        .bind(&plan.digest)
        .bind(ACTION_NEW)
        .bind(item.source_span.start)
        .bind(item.source_span.end)
        .bind(binding.task_id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn task(
    connection: &mut PgConnection,
    current: &LeaseRow,
    plan: &PreparedPlan,
    item: &PreparedTask,
    binding: &TaskBinding,
    source: &Source,
    active: bool,
) -> Result<()> {
    let status = if active {
        TaskState::Running
    } else {
        TaskState::Ready
    };
    let questions =
        serde_json::to_string(&item.questions).map_err(afd_core::json::Unreadable::from)?;
    sqlx::query_scalar::<_, String>(sql::INSERT_TASK)
        .bind(binding.task_id.as_str())
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&current.tenant_id)
        .bind(binding.item_id.as_str())
        .bind(plan.accepted_revision_id.as_str())
        .bind(&item.title)
        .bind(&item.objective)
        .bind(&questions)
        .bind(&item.goal_digest)
        .bind(&item.target_json)
        .bind(status.as_str())
        .bind(item.limits.model_nanos)
        .bind(i64::from(item.limits.running_seconds) * 1_000_000_000)
        .bind(i64::from(item.limits.leases))
        .bind(i32::from(active))
        .bind(i64::from(item.limits.publications))
        .bind(i64::from(item.limits.decision_calls))
        .bind(i64::from(item.limits.fallback_reads))
        .bind(&source.reply_provider)
        .bind(&source.reply_address)
        .bind(f64::from(item.limits.wall_seconds))
        .bind(i64::from(UNFINISHED_MAX))
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::LimitExhausted)?;
    Ok(())
}

pub(super) async fn lease(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    binding: &TaskBinding,
) -> Result<()> {
    sqlx::query(sql::BIND_LEASE)
        .bind(binding.task_id.as_str())
        .bind(lease.lease_id.as_str())
        .bind(lease.runner_id.as_str())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn admission(
    connection: &mut PgConnection,
    admission: &str,
    binding: &TaskBinding,
) -> Result<()> {
    sqlx::query(sql::BIND_ADMISSION)
        .bind(binding.task_id.as_str())
        .bind(binding.item_id.as_str())
        .bind(admission)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
