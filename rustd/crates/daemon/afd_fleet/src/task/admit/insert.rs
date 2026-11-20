//! Typed writes use the transaction whose source and slot are already locked.

use af_core::id::Uuid7;
use af_wire::task::TaskState;
use sqlx::PgConnection;

use super::super::{
    PreparedItem, PreparedPlan, PreparedTask, TaskBinding, TaskRefusal, TaskRun, guard::RunRow, sql,
};
use super::source::Source;
use crate::error::{Error, ErrorKind, Result, query};

const CONTEXT: &str = "write durable task binding";

pub(super) struct Admission<'a> {
    pub source: &'a Source,
    pub active: bool,
    pub unfinished_max: u32,
}

pub(super) async fn item(
    connection: &mut PgConnection,
    current: &RunRow,
    plan: &PreparedPlan,
    item: &PreparedItem,
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
        .bind(&binding.item_id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&plan.source_turn_id)
        .bind(ordinal)
        .bind(&plan.digest)
        .bind(item.action().as_str())
        .bind(item.span().start)
        .bind(item.span().end)
        .bind(binding.task_id.as_ref())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn task(
    connection: &mut PgConnection,
    current: &RunRow,
    plan: &PreparedPlan,
    item: &PreparedTask,
    binding: &TaskBinding,
    admission: Admission<'_>,
) -> Result<()> {
    let status = if admission.active {
        TaskState::Running
    } else {
        TaskState::Ready
    };
    let questions =
        serde_json::to_string(&item.questions).map_err(af_core::json::Unreadable::from)?;
    sqlx::query_scalar::<_, Uuid7>(sql::INSERT_TASK)
        .bind(binding.task_id.as_ref().ok_or(TaskRefusal::InvalidTarget)?)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(&current.tenant_id)
        .bind(&binding.item_id)
        .bind(&plan.accepted_revision_id)
        .bind(&item.title)
        .bind(&item.objective)
        .bind(&questions)
        .bind(&item.goal_digest)
        .bind(&item.target_json)
        .bind(status.as_str())
        .bind(item.limits.model_nanos)
        .bind(i64::from(item.limits.running_seconds) * 1_000_000_000)
        .bind(i64::from(item.limits.runs))
        .bind(i32::from(admission.active))
        .bind(i64::from(item.limits.publications))
        .bind(i64::from(item.limits.decision_calls))
        .bind(i64::from(item.limits.fallback_reads))
        .bind(&admission.source.reply_provider)
        .bind(&admission.source.reply_address)
        .bind(f64::from(item.limits.wall_seconds))
        .bind(i64::from(admission.unfinished_max))
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::LimitExhausted)?;
    Ok(())
}

pub(super) async fn run(
    connection: &mut PgConnection,
    run: TaskRun<'_>,
    binding: &TaskBinding,
) -> Result<()> {
    sqlx::query(sql::BIND_RUN)
        .bind(binding.task_id.as_ref().ok_or(TaskRefusal::InvalidTarget)?)
        .bind(run.run_id)
        .bind(run.runner_id)
        .bind(afd_billing::biller::WorkKey::FleetRun(run.run_id.clone()).work_type())
        .bind(af_wire::usage::LineKind::Model.as_str())
        .bind(super::super::run::SLICE_SECONDS)
        .bind(1_000_000_000_i64)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn admission(
    connection: &mut PgConnection,
    admission: &Uuid7,
    binding: &TaskBinding,
) -> Result<()> {
    sqlx::query(sql::BIND_TURN)
        .bind(binding.task_id.as_ref().ok_or(TaskRefusal::InvalidTarget)?)
        .bind(&binding.item_id)
        .bind(admission)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
