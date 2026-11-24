//! Typed writes use the transaction whose source and thread are already locked.

use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;
use sqlx::PgConnection;

use super::super::{
    DurableTaskBinding, DurableTaskRefusal, PreparedDurableTask, PreparedItem, PreparedPlan,
    guard::RunRow, sql,
};
use super::source::Source;
use crate::error::{Result, query};

const CONTEXT: &str = "write durable task binding";

pub(super) struct Acceptance<'a> {
    pub source: &'a Source,
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
    item: &PreparedDurableTask,
    binding: &DurableTaskBinding,
    acceptance: Acceptance<'_>,
) -> Result<()> {
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
        .bind(&item.title)
        .bind(&item.objective)
        .bind(&questions)
        .bind(&item.goal_digest)
        .bind(&item.target_json)
        .bind(DurableTaskState::Ready.as_str())
        .bind(item.limits.model_nanos)
        .bind(i64::from(item.limits.running_seconds) * 1_000_000_000)
        .bind(i64::from(item.limits.runs))
        .bind(i64::from(item.limits.publications))
        .bind(i64::from(item.limits.decision_calls))
        .bind(&acceptance.source.reply_provider)
        .bind(&acceptance.source.reply_address)
        .bind(f64::from(item.limits.wall_seconds))
        .bind(i64::from(acceptance.unfinished_max))
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
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
