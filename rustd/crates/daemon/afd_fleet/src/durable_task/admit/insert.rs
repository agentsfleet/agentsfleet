//! Typed writes use the transaction whose source and thread are already locked.

use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;
use sqlx::PgConnection;

use super::super::{
    DurableTaskBinding, DurableTaskRefusal, DurableTaskRun, PreparedDurableTask, PreparedItem,
    PreparedPlan, guard::RunRow, sql,
};
use super::source::Source;
use crate::error::{Result, query};

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
    binding: &DurableTaskBinding,
    ordinal: usize,
) -> Result<()> {
    let ordinal = super::super::row::sql_small_counter(ordinal, "ordinal")?;
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
        .bind(binding.durable_task_id.as_ref())
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn task(
    connection: &mut PgConnection,
    current: &RunRow,
    plan: &PreparedPlan,
    item: &PreparedDurableTask,
    binding: &DurableTaskBinding,
    admission: Admission<'_>,
) -> Result<()> {
    let status = if admission.active {
        DurableTaskState::Running
    } else {
        DurableTaskState::Ready
    };
    let questions =
        serde_json::to_string(&item.questions).map_err(af_core::json::Unreadable::from)?;
    sqlx::query_scalar::<_, Uuid7>(sql::INSERT_DURABLE_TASK)
        .bind(
            binding
                .durable_task_id
                .as_ref()
                .ok_or(DurableTaskRefusal::InvalidTarget)?,
        )
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
        .bind(&admission.source.reply_provider)
        .bind(&admission.source.reply_address)
        .bind(f64::from(item.limits.wall_seconds))
        .bind(i64::from(admission.unfinished_max))
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
    Ok(())
}

pub(super) async fn run(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    binding: &DurableTaskBinding,
    slice_seconds: i32,
) -> Result<()> {
    sqlx::query(sql::BIND_RUN)
        .bind(
            binding
                .durable_task_id
                .as_ref()
                .ok_or(DurableTaskRefusal::InvalidTarget)?,
        )
        .bind(run.run_id)
        .bind(run.runner_id)
        .bind(afd_billing::biller::WorkKey::FleetRun(run.run_id.clone()).work_type())
        .bind(af_wire::usage::LineKind::Model.as_str())
        .bind(slice_seconds)
        .bind(1_000_000_000_i64)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

pub(super) async fn bind_turn(
    connection: &mut PgConnection,
    turn: &Uuid7,
    binding: &DurableTaskBinding,
) -> Result<()> {
    sqlx::query(sql::BIND_TURN)
        .bind(
            binding
                .durable_task_id
                .as_ref()
                .ok_or(DurableTaskRefusal::InvalidTarget)?,
        )
        .bind(turn)
        .execute(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}
