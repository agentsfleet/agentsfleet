//! Resolve source and execution authority once for preparation and issuance.

use af_core::id::Uuid7;
use af_wire::{decision::DecisionPurpose, task::DecisionReservationInput};
use sqlx::PgConnection;

use super::{
    context::{AssessedContext, TaskStamp},
    prepare::Candidate,
    sql,
};
use crate::{
    Result,
    error::query,
    lease::sql::{FLEET_STATUS_ACTIVE, LEASE_STATUS_ACTIVE},
    task::{TaskLease, TaskRefusal, TaskScope, TaskSnapshot, admit::source, guard, read},
};

pub(super) struct NativeContext {
    pub current: guard::LeaseRow,
    pub scope: TaskScope,
    pub task: Option<Execution>,
    pub source: source::Source,
}

/// The locked database generation stays signed until its next native write.
pub(super) struct Execution {
    pub snapshot: TaskSnapshot,
    pub generation: i64,
}

impl NativeContext {
    pub(super) async fn load(
        connection: &mut PgConnection,
        lease: TaskLease<'_>,
        input: &DecisionReservationInput,
    ) -> Result<Self> {
        let current = guard::lease(connection, lease).await?;
        if current.fleet_status != FLEET_STATUS_ACTIVE {
            return Err(TaskRefusal::StaleState.into());
        }
        let scope = TaskScope {
            fleet_id: guard::id(&current.fleet_id, "fleet_id")?,
            workspace_id: guard::id(&current.workspace_id, "workspace_id")?,
        };
        let task = execution(connection, &current, &scope, input).await?;
        let source_id = guard::id(&input.source_admission_id, "source_admission_id")?;
        let source = source::load(connection, &current, &source_id).await?;
        authorize_source(
            connection,
            lease,
            &current,
            &source,
            &source_id,
            input.assessment.purpose(),
        )
        .await?;
        let current = guard::lease(connection, lease).await?;
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
            self.current.revision_id.as_deref()
        }
        .ok_or_else(|| TaskRefusal::StaleState.into())
    }

    pub(super) fn assessed(
        &self,
        input: &DecisionReservationInput,
        profile_revision: &str,
        questions_digest: String,
        candidates: &[Candidate],
    ) -> Result<AssessedContext> {
        Ok(AssessedContext {
            source_admission_id: input.source_admission_id.clone(),
            assessment: input.assessment.clone(),
            accepted_revision_id: self.revision(input.assessment.purpose())?.to_owned(),
            profile_revision_id: profile_revision.to_owned(),
            questions_digest,
            execution: self
                .task
                .as_ref()
                .map(|task| TaskStamp::of(&task.snapshot.context)),
            candidates: candidates
                .iter()
                .map(|candidate| candidate.stamp.clone())
                .collect(),
        })
    }
}

pub(super) async fn execution(
    connection: &mut PgConnection,
    current: &guard::LeaseRow,
    scope: &TaskScope,
    input: &DecisionReservationInput,
) -> Result<Option<Execution>> {
    match (&current.task_id, &input.task) {
        (None, None) => Ok(None),
        (Some(id), Some(expected)) if *id == expected.task_id => {
            let generation = sqlx::query_scalar::<_, i64>(sql::LOCK_TASK)
                .bind(id)
                .bind(&current.fleet_id)
                .bind(&current.workspace_id)
                .fetch_optional(&mut *connection)
                .await
                .map_err(query("lock decision execution stamp"))?
                .ok_or(TaskRefusal::StaleState)?;
            let task = read::on(connection, scope, &guard::id(id, "task_id")?).await?;
            if task.context.generation != expected.generation
                || current.task_generation != i64::try_from(task.context.generation).ok()
                || current.task_stop_generation != i64::try_from(task.context.stop_generation).ok()
                || task.context.state != af_wire::task::TaskState::Running
            {
                return Err(TaskRefusal::StaleState.into());
            }
            Ok(Some(Execution {
                snapshot: task,
                generation,
            }))
        }
        _ => Err(TaskRefusal::StaleState.into()),
    }
}

pub(super) async fn authorize_source(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    current: &guard::LeaseRow,
    input: &source::Source,
    source_id: &Uuid7,
    purpose: DecisionPurpose,
) -> Result<()> {
    if purpose == DecisionPurpose::Goal {
        if source_id.as_str() != current.source_admission_id {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        return Ok(());
    }
    source::check_human(input)?;
    if source_id.as_str() != current.source_admission_id {
        sqlx::query_scalar::<_, String>(sql::CLAIM_SOURCE)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .bind(&current.source_admission_id)
            .bind(afd_admission::Producer::Steer.as_str())
            .bind(afd_events::ACTOR_MACHINE)
            .bind(format!("{}%", afd_events::ACTOR_PREFIX))
            .bind(0_i64)
            .bind(1_i64)
            .bind(lease.lease_id.as_str())
            .bind(current.fence)
            .bind(LEASE_STATUS_ACTIVE)
            .bind(source_id.as_str())
            .bind(false)
            .bind(afd_admission::Producer::SlackMention.as_str())
            .bind(afd_connector::Provider::Slack.id())
            .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
            .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
            .fetch_optional(connection)
            .await
            .map_err(query("claim the next native human decision source"))?
            .ok_or(TaskRefusal::StaleState)?;
    }
    Ok(())
}
