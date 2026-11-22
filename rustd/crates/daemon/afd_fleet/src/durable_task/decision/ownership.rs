//! Resolve source and execution authority once for preparation and issuance.

use af_core::id::Uuid7;
use af_wire::{decision::DecisionPurpose, durable_task::DecisionReservationInput};
use sqlx::PgConnection;

use super::{
    context::{AssessedContext, DurableTaskStamp},
    prepare::Candidate,
    sql,
};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, admit::source,
        guard, read,
    },
    error::query,
    run::sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE},
};

pub(super) struct NativeContext {
    pub current: guard::RunRow,
    pub scope: DurableTaskScope,
    pub task: Option<Execution>,
    pub source: source::Source,
}

/// The locked database generation stays signed until its next native write.
pub(super) struct Execution {
    pub snapshot: DurableTaskSnapshot,
    pub generation: i64,
    /// The task the run holds, as locked: the id the snapshot carries, typed.
    pub durable_task_id: Uuid7,
}

impl NativeContext {
    pub(super) async fn load(
        connection: &mut PgConnection,
        run: DurableTaskRun<'_>,
        input: &DecisionReservationInput,
    ) -> Result<Self> {
        let current = guard::run(connection, run).await?;
        if current.fleet_status != FLEET_STATUS_ACTIVE {
            return Err(DurableTaskRefusal::StaleState.into());
        }
        let scope = DurableTaskScope {
            fleet_id: current.fleet_id.clone(),
            workspace_id: current.workspace_id.clone(),
        };
        let task = execution(connection, &current, &scope, input).await?;
        let source_id = guard::id(&input.source_turn_id, "source_turn_id")?;
        let source = source::load(connection, &current, &source_id).await?;
        authorize_source(
            connection,
            run,
            &current,
            &source,
            &source_id,
            input.assessment.purpose(),
        )
        .await?;
        let current = guard::run(connection, run).await?;
        Ok(Self {
            current,
            scope,
            task,
            source,
        })
    }

    pub(super) fn revision(&self, purpose: DecisionPurpose) -> Result<&str> {
        if purpose == DecisionPurpose::Goal {
            self.task
                .as_ref()
                .map(|task| task.snapshot.context.accepted_revision_id.as_ref())
        } else {
            self.current.revision_id.as_ref().map(Uuid7::as_str)
        }
        .ok_or_else(|| DurableTaskRefusal::StaleState.into())
    }

    pub(super) fn assessed(
        &self,
        input: &DecisionReservationInput,
        profile_revision: &str,
        questions_digest: String,
        candidates: &[Candidate],
    ) -> Result<AssessedContext> {
        Ok(AssessedContext {
            source_turn_id: guard::wire(&input.source_turn_id)?,
            assessment: input.assessment.clone(),
            accepted_revision_id: guard::wire(self.revision(input.assessment.purpose())?)?,
            profile_revision_id: guard::wire(profile_revision)?,
            questions_digest,
            execution: self
                .task
                .as_ref()
                .and_then(|task| DurableTaskStamp::of(&task.snapshot.context)),
            candidates: candidates
                .iter()
                .map(|candidate| candidate.stamp.clone())
                .collect(),
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
            let generation = sqlx::query_scalar::<_, i64>(sql::LOCK_DURABLE_TASK)
                .bind(id)
                .bind(&current.fleet_id)
                .bind(&current.workspace_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query("lock decision execution stamp"))?
                .ok_or(DurableTaskRefusal::StaleState)?;
            let task = read::on(connection, scope, id).await?;
            if task.context.generation != expected.generation
                || current.durable_task_generation != i64::try_from(task.context.generation).ok()
                || current.durable_task_stop_generation
                    != i64::try_from(task.context.stop_generation).ok()
                || task.context.state != af_wire::durable_task::DurableTaskState::Running
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

pub(super) async fn authorize_source(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    current: &guard::RunRow,
    input: &source::Source,
    source_id: &Uuid7,
    purpose: DecisionPurpose,
) -> Result<()> {
    if purpose == DecisionPurpose::Goal {
        if source_id.as_str() != current.source_turn_id {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        return Ok(());
    }
    source::check_human(input)?;
    if source_id.as_str() != current.source_turn_id {
        sqlx::query_scalar::<_, Uuid7>(sql::ASSIGN_SOURCE)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .bind(&current.source_turn_id)
            .bind(afd_turns::Producer::Steer.as_str())
            .bind(afd_turns::view::ACTOR_MACHINE)
            .bind(format!("{}%", afd_turns::view::ACTOR_PREFIX))
            .bind(0_i64)
            .bind(1_i64)
            .bind(run.run_id)
            .bind(current.fence)
            .bind(RUN_STATUS_ACTIVE)
            .bind(source_id)
            .bind(false)
            .bind(afd_turns::Producer::SlackMention.as_str())
            .bind(afd_connector::Provider::Slack.id())
            .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
            .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
            .fetch_optional(connection)
            .await
            .map_err(query("assign the next native human decision source"))?
            .ok_or(DurableTaskRefusal::StaleState)?;
    }
    Ok(())
}
