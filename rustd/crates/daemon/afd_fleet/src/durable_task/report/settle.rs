//! Financial settlement survives a refused or stale semantic disposition.

use super::DurableTaskSettlement;
use crate::{
    Result,
    durable_task::{Github, waits},
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskDisposition, DurableTaskEffectState, DurableTaskState};
use sqlx::{Connection as _, PgConnection};

#[derive(sqlx::FromRow)]
struct Current {
    durable_task_id: Uuid7,
    status: String,
    generation: i64,
    stop_generation: i64,
    durable_task_generation: Option<i64>,
    durable_task_stop_generation: Option<i64>,
    accepted_revision_id: Uuid7,
    goal_digest: String,
    covered_input_sequence: i64,
    input_sequence: i64,
    effect_cursor: i64,
    confirmed_effect_watermark: i64,
    checkpoint_id: Option<Uuid7>,
    within_limits: bool,
    effects_resolved: bool,
    can_resume: bool,
}

/// A notice replaces an unverified completion claim in both history and delivery.
pub(crate) async fn apply(
    connection: &mut PgConnection,
    run: &Uuid7,
    runner: &Uuid7,
    settlement: Option<DurableTaskSettlement<'_>>,
) -> Result<Option<&'static str>> {
    let current: Option<Current> = sqlx::query_as(include_str!("row.sql"))
        .bind(run)
        .bind(runner)
        .bind(DurableTaskEffectState::Confirmed.as_str())
        .bind(DurableTaskEffectState::Refused.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read final native task authority after accounting"))?;
    let Some(current) = current else {
        return Ok(None);
    };
    if current.durable_task_generation != Some(current.generation)
        || current.durable_task_stop_generation != Some(current.stop_generation)
        || current.status != DurableTaskState::Running.as_str()
    {
        return Ok(Some(STALE));
    }
    let Some(settlement) = settlement else {
        transition(
            connection,
            &current,
            DurableTaskState::Blocked,
            Some("completion_unverified"),
        )
        .await?;
        return Ok(Some(UNVERIFIED));
    };
    if !current.within_limits {
        transition(
            connection,
            &current,
            DurableTaskState::Exhausted,
            Some("cumulative_limit_reached"),
        )
        .await?;
        return Ok(Some(LIMIT));
    }
    if !current.matches(&settlement) {
        transition(
            connection,
            &current,
            DurableTaskState::Blocked,
            Some("goal_evidence_changed"),
        )
        .await?;
        return Ok(Some(UNVERIFIED));
    }
    if !current.can_resume
        && matches!(
            settlement.plan.input.disposition,
            DurableTaskDisposition::Continue | DurableTaskDisposition::Waiting
        )
    {
        transition(
            connection,
            &current,
            DurableTaskState::Exhausted,
            Some("run_limit_reached"),
        )
        .await?;
        return Ok(Some(LIMIT));
    }
    // A Ready verdict comes only from a disposition that prepared its
    // continuation turn; without one the task is blocked rather than stranded.
    let decided = action(connection, &current, &settlement)
        .await?
        .filter(|(state, _)| *state != DurableTaskState::Ready || settlement.turn.is_some());
    let Some((state, reason)) = decided else {
        transition(
            connection,
            &current,
            DurableTaskState::Blocked,
            Some("recovery_or_completion_unverified"),
        )
        .await?;
        return Ok(Some(UNVERIFIED));
    };
    if state == DurableTaskState::Ready
        && let Some(turn) = settlement.turn
    {
        return continue_ready(connection, &current, settlement.plan, turn, reason).await;
    }
    transition(connection, &current, state, reason).await?;
    if state == DurableTaskState::Waiting {
        let plan = settlement.plan;
        waits::arm(
            &plan.tasks,
            connection,
            &current.durable_task_id,
            &plan.run_id,
            plan.target
                .as_ref()
                .ok_or(crate::durable_task::DurableTaskRefusal::InvalidTarget)?,
            plan.dependency
                .as_ref()
                .ok_or(crate::durable_task::DurableTaskRefusal::InvalidTarget)?,
        )
        .await?;
    }
    Ok(None)
}

impl Current {
    fn matches(&self, settlement: &DurableTaskSettlement<'_>) -> bool {
        let old = &settlement.plan.snapshot.context;
        self.within_limits
            && self.durable_task_id == old.durable_task_id
            && u64::try_from(self.generation).ok() == Some(old.generation)
            && u64::try_from(self.stop_generation).ok() == Some(old.stop_generation)
            && self.accepted_revision_id == old.accepted_revision_id
            && self.goal_digest == old.goal_digest
            && self.effect_cursor.unsigned_abs() == old.effect_cursor
            && self.confirmed_effect_watermark.unsigned_abs() == old.confirmed_effect_watermark
            && self.covered_input_sequence.unsigned_abs() == old.covered_input_sequence
            && self.input_sequence == self.covered_input_sequence
    }
}

async fn action(
    connection: &mut PgConnection,
    current: &Current,
    settlement: &DurableTaskSettlement<'_>,
) -> Result<Option<(DurableTaskState, Option<&'static str>)>> {
    let plan = settlement.plan;
    if let Some(call) = &plan.input.decision_call_id {
        let active: Option<Uuid7> = sqlx::query_scalar(include_str!("profile.sql"))
            .bind(crate::durable_task::guard::wire(call)?)
            .bind(&plan.run_id)
            .bind(&plan.runner_id)
            .bind(crate::run::sql::RUNNER_STATUS_ACTIVE)
            .bind(crate::run::sql::FLEET_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("recheck native goal profile at final report commit"))?;
        if active.is_none() {
            return Ok(None);
        }
    }
    if let Some(target) = &plan.target {
        let Some(config) =
            crate::run::Runs::stored_config_on(connection, &plan.scope.fleet_id, None).await?
        else {
            return Ok(None);
        };
        if target.within(&config).is_err() {
            return Ok(None);
        }
        if let Err(fault) =
            Github::hold_authority(connection, &plan.scope, target.installation_id).await
        {
            if fault.is_datastore_unavailable() {
                return Err(fault);
            }
            return Ok(None);
        }
    }
    if !current.effects_resolved {
        return Ok(None);
    }
    let input = &plan.input;
    if input.covered_input_sequence != current.covered_input_sequence.unsigned_abs()
        || input.effect_watermark != current.confirmed_effect_watermark.unsigned_abs()
    {
        return Ok(None);
    }
    match input.disposition {
        DurableTaskDisposition::Blocked => Ok(Some((
            DurableTaskState::Blocked,
            Some("prerequisite_unavailable"),
        ))),
        DurableTaskDisposition::Complete => Ok((plan.target.is_none()
            || plan
                .dependency
                .as_ref()
                .zip(plan.target.as_ref())
                .is_some_and(|(status, target)| status.green(target)))
        .then_some((DurableTaskState::Done, None))),
        DurableTaskDisposition::Continue | DurableTaskDisposition::Waiting => {
            if input.checkpoint_id.as_deref() != current.checkpoint_id.as_ref().map(Uuid7::as_str) {
                return Ok(None);
            }
            let saved = super::checkpoint::CheckpointGuard {
                checkpoint_id: input.checkpoint_id.as_deref(),
                durable_task_id: &current.durable_task_id,
                run_id: &plan.run_id,
                generation: current.generation,
                stop_generation: current.stop_generation,
                revision_id: &current.accepted_revision_id,
                covered_input_sequence: current.covered_input_sequence,
                effect_watermark: current.confirmed_effect_watermark,
                head: plan.target.as_ref().map(|target| target.head.as_str()),
            }
            .accepts(connection)
            .await?;
            if !saved {
                return Ok(None);
            }
            if input.disposition == DurableTaskDisposition::Continue {
                return Ok(Some((DurableTaskState::Ready, None)));
            }
            Ok(plan
                .dependency
                .as_ref()
                .zip(plan.target.as_ref())
                .filter(|(status, target)| status.open && status.head == target.head)
                .and_then(|(status, target)| {
                    if status.checks_finished(target) {
                        Some((DurableTaskState::Ready, Some("validation_changed")))
                    } else {
                        status
                            .checks
                            .iter()
                            .any(|check| check.status != "completed")
                            .then_some((DurableTaskState::Waiting, Some("validation_pending")))
                    }
                }))
        }
    }
}

async fn transition(
    connection: &mut PgConnection,
    current: &Current,
    state: DurableTaskState,
    reason: Option<&str>,
) -> Result<()> {
    waits::cancel(connection, &current.durable_task_id).await?;
    sqlx::query(include_str!("../waits/sql/transition.sql"))
        .bind(&current.durable_task_id)
        .bind(state.as_str())
        .bind(state.is_terminal())
        .execute(&mut *connection)
        .await
        .map_err(query("commit task state before releasing ordinary run"))?;
    crate::durable_task::continuation::retire(connection, &current.durable_task_id).await?;
    tracing::info!(
        event = "dt_transition",
        durable_task_id = current.durable_task_id.as_str(),
        state = state.as_str(),
        reason
    );
    Ok(())
}

const STALE: &str =
    "This task changed while the run was finishing. Its current state is preserved.";
const UNVERIFIED: &str =
    "Work is blocked because its completion or recovery evidence could not be verified.";

const LIMIT: &str = "Work stopped at the task limit. No further execution was admitted.";

async fn continue_ready(
    connection: &mut PgConnection,
    current: &Current,
    plan: &super::PreparedReport,
    turn: afd_turns::PreparedTurn<'_>,
    reason: Option<&str>,
) -> Result<Option<&'static str>> {
    let mut savepoint = connection
        .begin()
        .await
        .map_err(query("begin task continuation savepoint"))?;
    let admitted = match plan.tasks.turns.stage(&mut savepoint, turn).await {
        Ok(admitted) => admitted,
        Err(fault)
            if fault.is_over_capacity() || fault.code() == af_core::error_code::SETUP_POLICY =>
        {
            savepoint.rollback().await.map_err(query(
                "roll back refused continuation without discarding measured usage",
            ))?;
            tracing::warn!(event = "dt_continuation_refused", durable_task_id = current.durable_task_id.as_str(), error_code = %fault.code());
            transition(
                connection,
                current,
                DurableTaskState::Blocked,
                Some("continuation_unavailable"),
            )
            .await?;
            return Ok(Some(UNVERIFIED));
        }
        Err(fault) => return Err(fault.into()),
    };
    transition(&mut savepoint, current, DurableTaskState::Ready, reason).await?;
    sqlx::query(crate::durable_task::sql::CONTROL_TURN)
        .bind(&current.durable_task_id)
        .bind(&admitted.stored.id)
        .execute(&mut *savepoint)
        .await
        .map_err(query("bind continuation before report releases its thread"))?;
    savepoint
        .commit()
        .await
        .map_err(query("commit staged task continuation"))?;
    Ok(None)
}
