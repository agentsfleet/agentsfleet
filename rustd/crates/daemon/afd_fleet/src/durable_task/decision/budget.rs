//! Provisional decision credit is cumulative and shares the fleet's budget.

use af_wire::{decision::DecisionPurpose, durable_task::DurableTaskContext};
use afd_billing::{
    Nanos, Spend,
    budget::{self, Verdict},
};
use sqlx::PgConnection;

use super::{context::AssessedContext, sql};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskSnapshot, guard::RunRow},
    error::query,
    run::Runs,
};

pub(super) const CALL_NANOS: i64 = 20_000_000;
pub(super) const LOGICAL_SECONDS: f64 = 30.0;

#[derive(sqlx::FromRow)]
struct FleetSpend {
    day_nanos: i64,
    month_nanos: i64,
}

pub(super) async fn check_on(
    connection: &mut PgConnection,
    current: &RunRow,
    context: &AssessedContext,
    task: Option<&DurableTaskSnapshot>,
    allocating: bool,
) -> Result<()> {
    paid_ready(connection, current, context).await?;
    let extra = if allocating { CALL_NANOS } else { 0 };
    if context.assessment.purpose() == DecisionPurpose::Goal {
        let task = &task.ok_or(DurableTaskRefusal::InvalidTarget)?.context;
        goal(task, allocating)?;
    }
    let config = Runs::stored_config_on(connection, &current.fleet_id, None)
        .await?
        .ok_or(DurableTaskRefusal::StaleState)?;
    let totals: FleetSpend = sqlx::query_as(sql::BUDGET)
        .bind(&current.workspace_id)
        .bind(&current.fleet_id)
        .bind(af_core::timing::DAY)
        .fetch_one(connection)
        .await
        .map_err(query(
            "check decision reservations against the current fleet budget",
        ))?;
    let extra = Nanos::from_i64(extra);
    let projected = Spend {
        day: Nanos::from_i64(totals.day_nanos).saturating_add(extra),
        month: Nanos::from_i64(totals.month_nanos).saturating_add(extra),
    };
    if budget::covers_reservation(config.budget(), projected) != Verdict::Admit {
        return Err(DurableTaskRefusal::LimitExhausted.into());
    }
    Ok(())
}

fn goal(task: &DurableTaskContext<'_>, allocating: bool) -> Result<()> {
    if task.usage.model_nanos >= task.limits.model_nanos
        || task.usage.running_nanos >= u64::from(task.limits.running_seconds) * 1_000_000_000
        || (allocating && task.usage.decision_calls >= task.limits.decision_calls)
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
    let task = context
        .execution
        .as_ref()
        .filter(|_task| context.assessment.purpose() == DecisionPurpose::Goal);
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
