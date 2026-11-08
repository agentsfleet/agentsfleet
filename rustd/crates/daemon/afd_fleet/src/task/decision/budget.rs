//! Provisional decision credit is cumulative and shares the fleet's budget.

use af_wire::{decision::DecisionPurpose, task::TaskContext};
use afd_billing::{
    Nanos, Spend,
    budget::{self, Verdict},
};
use sqlx::PgConnection;

use super::{context::AssessedContext, sql};
use crate::{
    Result,
    error::query,
    lease::Leases,
    task::{TaskRefusal, TaskSnapshot, guard::LeaseRow},
};

pub(super) const CALL_NANOS: i64 = 20_000_000;
pub(super) const SOURCE_CALLS_MAX: i64 = 10;
pub(super) const LOGICAL_SECONDS: f64 = 30.0;

#[derive(sqlx::FromRow)]
struct FleetSpend {
    day_nanos: i64,
    month_nanos: i64,
}

pub(super) async fn check_on(
    connection: &mut PgConnection,
    current: &LeaseRow,
    context: &AssessedContext,
    task: Option<&TaskSnapshot>,
    allocating: bool,
) -> Result<()> {
    paid_ready(connection, current, context).await?;
    let extra = if allocating { CALL_NANOS } else { 0 };
    if context.assessment.purpose() == DecisionPurpose::Goal {
        let task = &task.ok_or(TaskRefusal::InvalidTarget)?.context;
        goal(task, extra, allocating)?;
    } else if allocating {
        let calls: i64 = sqlx::query_scalar(sql::SOURCE_CALLS)
            .bind(&context.source_admission_id)
            .bind(DecisionPurpose::Intent.as_str())
            .fetch_one(&mut *connection)
            .await
            .map_err(query("check cumulative source decision allowance"))?;
        if calls >= SOURCE_CALLS_MAX {
            return Err(TaskRefusal::LimitExhausted.into());
        }
    } else {
        // A non-goal call that allocates nothing spends no source allowance.
    }
    let config = Leases::stored_config_on(connection, &current.fleet_id, None)
        .await?
        .ok_or(TaskRefusal::StaleState)?;
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
        return Err(TaskRefusal::LimitExhausted.into());
    }
    Ok(())
}

fn goal(task: &TaskContext<'_>, extra: i64, allocating: bool) -> Result<()> {
    let model = task
        .usage
        .model_nanos
        .saturating_add(task.usage.reserved_model_nanos)
        .saturating_add(extra);
    if model > task.limits.model_nanos
        || task.usage.running_nanos >= u64::from(task.limits.running_seconds) * 1_000_000_000
        || (allocating && task.usage.decision_calls >= task.limits.decision_calls)
    {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    Ok(())
}

async fn paid_ready(
    connection: &mut PgConnection,
    current: &LeaseRow,
    context: &AssessedContext,
) -> Result<()> {
    let task = context
        .execution
        .as_ref()
        .filter(|_task| context.assessment.purpose() == DecisionPurpose::Goal);
    let overage: bool = sqlx::query_scalar(sql::OVERAGE)
        .bind(&current.lease_id)
        .bind(&context.source_admission_id)
        .bind(task.map(|task| &task.task_id))
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_one(connection)
        .await
        .map_err(query(
            "check retained original-work and task/source decision overage",
        ))?;
    if overage {
        tracing::warn!(
            event = "dt_decision_blocked",
            reason = "reservation_overage",
            lease_id = current.lease_id.as_str(),
            source_admission_id = context.source_admission_id.as_str()
        );
        return Err(TaskRefusal::DecisionOverage.into());
    }
    Ok(())
}
