//! Financial settlement survives a refused or stale semantic disposition.

use afd_wire::task::{TaskDisposition, TaskEffectState, TaskState};
use sqlx::{PgConnection, Connection as _};
use super::TaskSettlement;
use crate::{Result, error::query, task::waits};

#[derive(sqlx::FromRow)]
struct Current {
    task_id: String, status: String, generation: i64, stop_generation: i64,
    task_generation: Option<i64>, task_stop_generation: Option<i64>, accepted_revision_id: String,
    goal_digest: String, covered_input_sequence: i64, input_sequence: i64,
    effect_cursor: i64, confirmed_effect_watermark: i64, checkpoint_id: Option<String>,
    within_limits: bool, effects_resolved: bool, can_resume: bool,
}

/// A notice replaces an unverified completion claim in both history and delivery.
pub(crate) async fn apply(connection: &mut PgConnection, lease: &afd_core::id::Uuid7,
    runner: &afd_core::id::Uuid7, settlement: Option<TaskSettlement<'_>>) -> Result<Option<&'static str>> {
    let current: Option<Current> = sqlx::query_as(include_str!("row.sql"))
        .bind(lease.as_str()).bind(runner.as_str()).bind(TaskEffectState::Confirmed.as_str()).bind(TaskEffectState::Refused.as_str())
        .fetch_optional(&mut *connection).await.map_err(query("read final native task authority after accounting"))?;
    let Some(current) = current else { return Ok(None); };
    if current.task_generation != Some(current.generation) || current.task_stop_generation != Some(current.stop_generation)
        || current.status != TaskState::Running.as_str() { return Ok(Some(STALE)); }
    let Some(settlement) = settlement else {
        transition(connection, &current, TaskState::Blocked, Some("completion_unverified")).await?;
        return Ok(Some(UNVERIFIED));
    };
    if !current.within_limits {
        transition(connection, &current, TaskState::Exhausted, Some("cumulative_limit_reached")).await?;
        return Ok(Some(LIMIT));
    }
    if !current.matches(&settlement) {
        transition(connection, &current, TaskState::Blocked, Some("goal_evidence_changed")).await?;
        return Ok(Some(UNVERIFIED));
    }
    if !current.can_resume && matches!(settlement.plan.input.disposition, TaskDisposition::Continue | TaskDisposition::Waiting) {
        transition(connection, &current, TaskState::Exhausted, Some("lease_limit_reached")).await?;
        return Ok(Some(LIMIT));
    }
    let Some((state, reason)) = action(connection, &current, &settlement).await? else {
        transition(connection, &current, TaskState::Blocked, Some("recovery_or_completion_unverified")).await?;
        return Ok(Some(UNVERIFIED));
    };
    if state == TaskState::Ready {
        let Some(admission) = settlement.admission else {
            transition(connection, &current, TaskState::Blocked, Some("continuation_unavailable")).await?;
            return Ok(Some(UNVERIFIED));
        };
        let mut savepoint = connection.begin().await.map_err(query("begin task continuation savepoint"))?;
        let admitted = match settlement.plan.tasks.admissions.stage(&mut savepoint, admission).await {
            Ok(admitted) => admitted,
            Err(fault) if fault.is_over_capacity() || fault.code() == afd_core::error_code::AUTHORING_POLICY => {
                savepoint.rollback().await.map_err(query("roll back refused continuation without discarding measured usage"))?;
                tracing::warn!(event = "dt_continuation_refused", task_id = current.task_id.as_str(), error_code = %fault.code());
                transition(connection, &current, TaskState::Blocked, Some("continuation_unavailable")).await?;
                return Ok(Some(UNVERIFIED));
            }
            Err(fault) => return Err(fault.into()),
        };
        transition(&mut savepoint, &current, state, reason).await?;
        sqlx::query(crate::task::sql::CONTROL_ADMISSION).bind(&current.task_id).bind(&admitted.stored.id)
            .execute(&mut *savepoint).await.map_err(query("bind continuation before report releases its slot"))?;
        savepoint.commit().await.map_err(query("commit staged task continuation"))?;
    } else { transition(connection, &current, state, reason).await?; }
    if state == TaskState::Waiting {
        let plan = settlement.plan;
        waits::arm(&plan.tasks, connection, &current.task_id, &plan.lease_id,
            plan.target.as_ref().ok_or(crate::task::TaskRefusal::InvalidTarget)?,
            plan.dependency.as_ref().ok_or(crate::task::TaskRefusal::InvalidTarget)?).await?;
    }
    Ok(None)
}

impl Current {
    fn matches(&self, settlement: &TaskSettlement<'_>) -> bool {
        let old = &settlement.plan.snapshot.context;
        self.within_limits && self.task_id == old.task_id && u64::try_from(self.generation).ok() == Some(old.generation)
            && u64::try_from(self.stop_generation).ok() == Some(old.stop_generation) && self.accepted_revision_id == old.accepted_revision_id
            && self.goal_digest == old.goal_digest && self.effect_cursor.unsigned_abs() == old.effect_cursor
            && self.confirmed_effect_watermark.unsigned_abs() == old.confirmed_effect_watermark
            && self.covered_input_sequence.unsigned_abs() == old.covered_input_sequence
            && self.input_sequence == self.covered_input_sequence
    }
}

async fn action(connection: &mut PgConnection, current: &Current, settlement: &TaskSettlement<'_>) -> Result<Option<(TaskState, Option<&'static str>)>> {
    let plan = settlement.plan;
    if let Some(call) = &plan.input.decision_call_id {
        let active: Option<String> = sqlx::query_scalar(include_str!("profile.sql"))
            .bind(call).bind(plan.lease_id.as_str()).bind(&plan.runner_id)
            .bind(crate::lease::sql::RUNNER_STATUS_ACTIVE).bind(crate::lease::sql::FLEET_STATUS_ACTIVE)
            .fetch_optional(&mut *connection).await.map_err(query("recheck native goal profile at final report commit"))?;
        if active.is_none() { return Ok(None); }
    }
    if let Some(target) = &plan.target {
        let Some(config) = crate::lease::Leases::stored_config_on(connection, plan.scope.fleet_id.as_str(), None).await? else { return Ok(None); };
        if target.within(&config).is_err() { return Ok(None); }
        if let Err(fault) = crate::task::Github::hold_authority(connection, &plan.scope, target.installation_id).await {
            if fault.is_datastore_unavailable() { return Err(fault); }
            return Ok(None);
        }
    }
    if !current.effects_resolved { return Ok(None); }
    let input = &plan.input;
    if input.covered_input_sequence != current.covered_input_sequence.unsigned_abs()
        || input.effect_watermark != current.confirmed_effect_watermark.unsigned_abs() { return Ok(None); }
    match input.disposition {
        TaskDisposition::Blocked => Ok(Some((TaskState::Blocked, Some("prerequisite_unavailable")))),
        TaskDisposition::Complete => Ok((plan.target.is_none() || plan.dependency.as_ref().zip(plan.target.as_ref()).is_some_and(|(status, target)| status.green(target)))
            .then_some((TaskState::Done, None))),
        TaskDisposition::Continue | TaskDisposition::Waiting => {
            if input.checkpoint_id != current.checkpoint_id { return Ok(None); }
            let saved: bool = sqlx::query_scalar(include_str!("checkpoint.sql"))
                .bind(input.checkpoint_id.as_deref()).bind(&current.task_id).bind(plan.lease_id.as_str())
                .bind(current.generation).bind(current.stop_generation).bind(&current.accepted_revision_id)
                .bind(current.covered_input_sequence).bind(current.confirmed_effect_watermark)
                .bind(plan.target.as_ref().map(|target| target.head.as_str()))
                .fetch_one(connection).await.map_err(query("prove complete current handoff before settling execution"))?;
            if !saved { return Ok(None); }
            if input.disposition == TaskDisposition::Continue { return Ok(Some((TaskState::Ready, None))); }
            Ok(plan.dependency.as_ref().zip(plan.target.as_ref()).filter(|(status, target)| status.open && status.head == target.head
                && status.checks.iter().any(|check| check.status != "completed"))
                .map(|_| (TaskState::Waiting, Some("validation_pending"))))
        }
    }
}

async fn transition(connection: &mut PgConnection, current: &Current, state: TaskState, reason: Option<&str>) -> Result<()> {
    waits::cancel(connection, &current.task_id).await?;
    sqlx::query(include_str!("transition.sql")).bind(&current.task_id).bind(state.as_str()).bind(reason).bind(state.is_terminal())
        .execute(connection).await.map_err(query("commit task state before releasing ordinary lease"))?;
    crate::task::continuation::retire(connection, &current.task_id).await?;
    tracing::info!(event = "dt_transition", task_id = current.task_id.as_str(), state = state.as_str(), reason);
    Ok(())
}

const STALE: &str = "This task changed while the run was finishing. Its current state is preserved.";
const UNVERIFIED: &str = "Work is blocked because its completion or recovery evidence could not be verified.";

const LIMIT: &str = "Work stopped at the task limit. No further execution was admitted.";
