//! A decision reservation's limits: the task's own allowances and the
//! retained overage. The payer's balance is the only other ceiling, and the
//! funding check reads it (`fund.rs`).

use af_wire::durable_task::DurableTaskContext;
use sqlx::PgConnection;

use super::{context::AssessedContext, sql};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskSnapshot, guard::RunRow},
    error::query,
};

pub(super) const CALL_NANOS: i64 = 20_000_000;
pub(super) const LOGICAL_SECONDS: f64 = 30.0;

pub(super) async fn check_on(
    connection: &mut PgConnection,
    current: &RunRow,
    context: &AssessedContext,
    task: Option<&DurableTaskSnapshot>,
) -> Result<()> {
    paid_ready(connection, current, context).await?;
    let task = &task.ok_or(DurableTaskRefusal::InvalidTarget)?.context;
    goal(task)
}

fn goal(task: &DurableTaskContext<'_>) -> Result<()> {
    if task.usage.model_nanos >= task.limits.model_nanos
        || task.usage.running_nanos >= u64::from(task.limits.running_seconds) * 1_000_000_000
        || task.usage.decision_calls >= task.limits.decision_calls
    {
        return Err(DurableTaskRefusal::LimitExhausted.into());
    }
    Ok(())
}

async fn paid_ready(
    connection: &mut PgConnection,
    current: &RunRow,
    context: &AssessedContext,
) -> Result<()> {
    let task = context.execution.as_ref();
    let overage: bool = sqlx::query_scalar(sql::OVERAGE)
        .bind(&current.run_id)
        .bind(&context.source_turn_id)
        .bind(task.map(|task| &task.durable_task_id))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_one(connection)
        .await
        .map_err(query(
            "check retained original-work and task/source decision overage",
        ))?;
    if overage {
        let (run_id, source_turn_id) = (current.run_id.as_str(), context.source_turn_id.as_str());
        {
            let (event, reason) = (&"dt_decision_blocked", &"reservation_overage");
            tracing::warn!(event, reason, run_id, source_turn_id);
        };
        return Err(DurableTaskRefusal::DecisionOverage.into());
    }
    Ok(())
}
