//! A reservation's run, task and source, read with no lock: the issue statement
//! (`sql/issue.sql`) locks the rows these reads relied on and re-checks them in one statement.

use af_core::id::Uuid7;
use af_wire::durable_task::DecisionReservationInput;
use sqlx::PgConnection;

use super::{
    context::{AssessedContext, DurableTaskStamp},
    sql,
};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, accept::source,
        guard, read,
    },
    error::{query, run_not_found, stale_fence},
    run::{
        sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE},
        standing::fence_holds,
    },
};

pub(super) struct NativeContext {
    pub current: guard::RunRow,
    pub task: Option<Execution>,
    pub source: source::Source,
}

/// The task the run holds, as read: the issue statement locks it and re-checks its generation.
pub(super) struct Execution {
    pub snapshot: DurableTaskSnapshot,
    pub generation: i64,
    /// The task the run holds: the id the snapshot carries, typed.
    pub durable_task_id: Uuid7,
}

impl NativeContext {
    pub(super) async fn load(
        connection: &mut PgConnection,
        run: DurableTaskRun<'_>,
        input: &DecisionReservationInput,
    ) -> Result<Self> {
        let current = context(connection, run).await?;
        block_profile_switch(connection, &current, input).await?;
        if current.fleet_status != FLEET_STATUS_ACTIVE {
            return Err(DurableTaskRefusal::StaleState.into());
        }
        let scope = DurableTaskScope {
            fleet_id: current.fleet_id.clone(),
            workspace_id: current.workspace_id.clone(),
        };
        let task = execution(connection, &current, &scope, input).await?;
        let source_id = guard::id(&input.source_turn_id, "source_turn_id")?;
        let source = source::fetch(connection, &current, &source_id, sql::SOURCE).await?;
        // A goal check assesses the run's own head turn: the task's source.
        if source_id.as_str() != current.source_turn_id {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        Ok(Self {
            current,
            task,
            source,
        })
    }

    pub(super) fn assessed(
        &self,
        input: &DecisionReservationInput,
        profile_revision: &str,
        questions_digest: String,
    ) -> Result<AssessedContext> {
        Ok(AssessedContext {
            source_turn_id: guard::wire(&input.source_turn_id)?,
            assessment: input.assessment.clone(),
            profile_revision_id: guard::wire(profile_revision)?,
            questions_digest,
            execution: self
                .task
                .as_ref()
                .and_then(|task| DurableTaskStamp::of(&task.snapshot.context)),
        })
    }
}

pub(super) async fn execution(
    connection: &mut PgConnection,
    current: &guard::RunRow,
    scope: &DurableTaskScope,
    input: &DecisionReservationInput,
) -> Result<Option<Execution>> {
    match (&current.durable_task_id, &input.durable_task) {
        (None, None) => Ok(None),
        (Some(id), Some(expected)) if *id == expected.durable_task_id => {
            // A task the run names but no row holds moved under the run: stale, as the
            // locked read answered it, and any other failure keeps its own answer.
            let task = match read::on(connection, scope, id).await {
                Err(error)
                    if matches!(
                        error.kind(),
                        crate::error::ErrorKind::DurableTaskRefused {
                            reason: DurableTaskRefusal::NotFound
                        }
                    ) =>
                {
                    return Err(DurableTaskRefusal::StaleState.into());
                }
                found => found?,
            };
            let generation = i64::try_from(task.context.generation)
                .map_err(|_range| DurableTaskRefusal::StaleState)?;
            if task.context.generation != expected.generation
                || current.durable_task_generation != i64::try_from(task.context.generation).ok()
                || current.durable_task_stop_generation
                    != i64::try_from(task.context.stop_generation).ok()
                || task.context.state != af_wire::durable_task::DurableTaskState::Running
                || task.context.expires_at <= current.checked_at
            {
                return Err(DurableTaskRefusal::StaleState.into());
            }
            Ok(Some(Execution {
                snapshot: task,
                generation,
                durable_task_id: id.clone(),
            }))
        }
        _ => Err(DurableTaskRefusal::StaleState.into()),
    }
}

/// The run as the reservation reads it, with no lock (`sql/context.sql`): what the
/// locked guard answers for a run that is gone, moved or not this runner's.
pub(super) async fn context(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
) -> Result<guard::RunRow> {
    let row: guard::RunRow = sqlx::query_as(sql::CONTEXT)
        .bind(run.run_id)
        .bind(run.runner_id)
        .bind(RUN_STATUS_ACTIVE)
        .bind(af_wire::durable_task::CAPABILITY)
        .bind(afd_turns::Producer::SlackMention.as_str())
        .bind(afd_connector::Provider::Slack.id())
        .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
        .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
        .fetch_optional(connection)
        .await
        .map_err(query("read the reserving run"))?
        .ok_or_else(run_not_found)?;
    if !row.capable || !fence_holds(row.fence, row.live_fence, run.fencing_token) {
        return Err(stale_fence());
    }
    Ok(row)
}

/// A subject whose calls under another profile revision are unresolved takes no
/// call under this one.
async fn block_profile_switch(
    connection: &mut PgConnection,
    current: &guard::RunRow,
    input: &DecisionReservationInput,
) -> Result<()> {
    let assignment = current
        .decision_snapshot
        .as_ref()
        .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
    let subject = super::context::subject(
        &input.assessment,
        current
            .durable_task_id
            .as_ref()
            .map(Uuid7::as_str)
            .zip(input.durable_task.as_ref().map(|task| task.generation)),
    )?;
    let blocked: bool = sqlx::query_scalar(sql::BLOCK_PROFILE_SWITCH)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(format!("{subject}/%"))
        .bind(guard::wire(&assignment.0.revision_id)?)
        .fetch_one(connection)
        .await
        .map_err(query(
            "retain unresolved assessments before changing decision profile",
        ))?;
    if blocked {
        return Err(DurableTaskRefusal::DecisionUnavailable.into());
    }
    Ok(())
}
