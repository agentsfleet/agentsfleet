//! The issuance: one autocommitted statement that locks what the reads relied on, re-checks
//! it, and inserts the call as issued with its payload (`sql/issue.sql`). The key leaves
//! only in the answer to a statement that inserted.

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_crypto::secret::SecretString;
use af_wire::durable_task::{
    DecisionCallState, DecisionDispatch, DecisionReservationInput, DecisionReservationResult,
};
use jiff_sqlx::ToSqlx as _;
use sqlx::types::Json;
use zeroize::Zeroizing;

use super::{
    ReservedDecision,
    allocate::{Ready, Reservation, replay_matches},
    budget, ownership, replay,
    row::{self, Lookup},
    sql,
};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTasks, guard, transaction},
    error::query,
    run::sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE, RUNNER_STATUS_ACTIVE},
};

/// What one issue statement answered when every check held: the call it inserted, or
/// nothing when the operation's call was already saved.
#[derive(sqlx::FromRow)]
struct Answered {
    id: Option<Uuid7>,
    logical_expires_at: Option<jiff_sqlx::Timestamp>,
}

pub(super) struct Issuance<'a> {
    pub tasks: &'a DurableTasks,
    pub run: DurableTaskRun<'a>,
    pub input: &'a garde::Valid<DecisionReservationInput>,
    pub id: &'a Uuid7,
    pub digest: &'a str,
    pub ready: &'a Ready,
    pub key: &'a SecretString,
}

impl Issuance<'_> {
    /// Runs the issue statement until it answers: a lost usage number runs it again,
    /// which ends because each loss means another call of the run committed.
    pub(super) async fn commit(&self) -> Result<ReservedDecision> {
        let mut attempt = 0_u32;
        loop {
            match self.once().await {
                Ok(Some(Answered {
                    id: Some(id),
                    logical_expires_at: Some(expires_at),
                })) => return self.issued(&id, expires_at.to_jiff()),
                Ok(Some(_saved)) => {
                    let operation = guard::wire(&self.input.operation_id)?;
                    return self
                        .saved(Lookup::Operation {
                            run: self.run.run_id,
                            operation: &operation,
                        })
                        .await;
                }
                Ok(None) => return Err(self.refusal().await),
                Err(Raced::Usage) => {
                    attempt = attempt.saturating_add(1);
                    let (run_id, event) =
                        (self.run.run_id.as_str(), "dt_decision_sequence_retried");
                    tracing::debug!(event, run_id, attempt);
                }
                Err(Raced::Assessment) => {
                    let ready = self.ready;
                    return self
                        .saved(Lookup::Assessment {
                            key: &ready.key,
                            attempt: i16::from(ready.attempt.ordinal()),
                        })
                        .await;
                }
                Err(Raced::Other(error)) => return Err(error),
            }
        }
    }

    /// The issue statement, autocommitted: its locks end with it, whatever becomes of the
    /// daemon afterwards. A request dropped while it waits on a lock can therefore still
    /// commit its call, which that operation's retry then reads as `existing`.
    async fn once(&self) -> Result<Option<Answered>, Raced> {
        let mut connection = transaction::acquire(self.tasks.runs.pool())
            .await
            .map_err(Raced::Other)?;
        let answered = self.statement(&mut connection).await;
        transaction::recycle(connection);
        answered
    }

    async fn statement(
        &self,
        connection: &mut sqlx::PgConnection,
    ) -> Result<Option<Answered>, Raced> {
        let bound = self.bind().map_err(Raced::Other)?;
        bound
            .fetch_optional(connection)
            .await
            .map_err(|error| Raced::of(error, "issue one native decision call"))
    }

    /// A saved call that won the race for this operation or assessment, answered as
    /// its replay: `existing` when it was made from the same request.
    async fn saved(&self, lookup: Lookup<'_>) -> Result<ReservedDecision> {
        let call = row::peek(self.tasks.runs.pool(), lookup)
            .await?
            .ok_or(DurableTaskRefusal::StaleState)?;
        match lookup {
            Lookup::Operation { .. } | Lookup::Id(_) => {
                Reservation {
                    tasks: self.tasks,
                    run: self.run,
                    input: self.input,
                    operation: guard::wire(&self.input.operation_id)?,
                    digest: self.digest.to_owned(),
                }
                .replay(call)
                .await
            }
            Lookup::Assessment { .. } => {
                replay_matches(&call, self.input, self.digest, false)?;
                replay::response(self.tasks, call).await
            }
        }
    }

    /// No row: the run lost its standing, which the run read answers with its own
    /// refusal, or another state the reads relied on moved.
    async fn refusal(&self) -> crate::Error {
        let standing = async {
            let mut connection = transaction::acquire(self.tasks.runs.pool()).await?;
            let standing = ownership::context(&mut connection, self.run).await;
            transaction::recycle(connection);
            standing
        }
        .await;
        standing
            .err()
            .unwrap_or_else(|| DurableTaskRefusal::StaleState.into())
    }

    fn issued(&self, id: &Uuid7, expires_at: jiff::Timestamp) -> Result<ReservedDecision> {
        let ready = self.ready;
        let (decision_call_id, assessment_id, attempt) = (
            id.as_str(),
            ready.attempt.assessment_id().as_str(),
            i16::from(ready.attempt.ordinal()),
        );
        let (profile_revision, purpose) = (
            ready.prepared.assignment.revision_id.as_str(),
            ready.prepared.request.purpose.as_str(),
        );
        {
            let event = &"dt_decision_issued";
            tracing::info!(
                event,
                decision_call_id,
                assessment_id,
                attempt,
                profile_revision,
                purpose
            );
        };
        let packet = DecisionReservationResult::Dispatch {
            call_id: id.clone().into(),
            dispatch: Box::new(DecisionDispatch {
                assessment_id: ready.attempt.assessment_id().clone().into(),
                attempt: ready.attempt.ordinal(),
                assignment: ready.prepared.assignment.clone(),
                request: ready.attempt.request(&ready.prepared.request).clone(),
                api_key: Cow::Borrowed(self.key.expose()),
                expires_at,
            }),
        };
        let body = Zeroizing::new(
            serde_json::to_string(&packet).map_err(af_core::json::Unreadable::from)?,
        );
        Ok(ReservedDecision { body, issued: true })
    }
}

/// Why an issue statement did not answer.
enum Raced {
    /// Another call of the run took the usage number this statement drew.
    Usage,
    /// Another call took this assessment, or this retry of its parent.
    Assessment,
    Other(crate::Error),
}

impl Raced {
    fn of(error: sqlx::Error, context: &'static str) -> Self {
        if afd_db::constraint::violates_unique(&error, "uq_decision_calls_usage") {
            return Self::Usage;
        }
        if afd_db::constraint::violates_unique(&error, "uq_decision_calls_assessment")
            || afd_db::constraint::violates_unique(&error, "uq_decision_calls_retry")
        {
            return Self::Assessment;
        }
        Self::Other(query(context)(error))
    }
}

type Bound<'q> = sqlx::query::QueryAs<'q, sqlx::Postgres, Answered, sqlx::postgres::PgArguments>;

impl Issuance<'_> {
    fn bind(&self) -> Result<Bound<'_>> {
        let (ready, run) = (self.ready, self.run);
        let (prepared, attempt) = (&ready.prepared, &ready.attempt);
        let named = prepared.task.as_ref();
        let parent = attempt.parent();
        Ok(sqlx::query_as(sql::ISSUE)
            .bind(self.id)
            .bind(run.runner_id)
            .bind(run.run_id)
            .bind(&prepared.context.source_turn_id)
            .bind(&prepared.current.fleet_id)
            .bind(&prepared.current.workspace_id)
            .bind(&prepared.current.tenant_id)
            .bind(named.map(|task| &task.durable_task_id))
            .bind(named.map(|task| task.generation))
            .bind(prepared.current.fence)
            .bind(guard::wire(&self.input.operation_id)?)
            .bind(self.digest)
            .bind(attempt.assessment_id())
            .bind(&ready.key)
            .bind(i16::from(attempt.ordinal()))
            .bind(parent.map(|parent| &parent.id))
            .bind(parent.map(|parent| parent.logical_expires_at.to_sqlx()))
            .bind(budget::LOGICAL_SECONDS)
            .bind(prepared.request.purpose.as_str())
            .bind(DecisionCallState::Issued.as_str())
            .bind(&prepared.provider)
            .bind(&prepared.current.fleet_name)
            .bind(Json(&prepared.assignment))
            .bind(Json(attempt.quote(&ready.quote)))
            .bind(Json(parent.map_or(&prepared.context, |parent| {
                &parent.context_snapshot.0
            })))
            .bind(budget::CALL_NANOS)
            .bind(guard::wire(&prepared.assignment.profile.model_id)?)
            .bind(guard::wire(&prepared.assignment.revision_id)?)
            .bind(Json(attempt.request(&prepared.request)))
            .bind(RUN_STATUS_ACTIVE)
            .bind(FLEET_STATUS_ACTIVE)
            .bind(RUNNER_STATUS_ACTIVE)
            .bind(af_wire::durable_task::CAPABILITY)
            .bind(named.map(|task| &task.durable_task_id))
            .bind(named.map(|task| task.generation))
            .bind(af_wire::durable_task::DurableTaskState::Running.as_str())
            .bind(afd_turns::Producer::SlackMention.as_str())
            .bind(afd_connector::Provider::Slack.id())
            .bind(afd_connector::slack::PERSON_ACTOR_PATTERN)
            .bind(afd_connector::slack::PERSON_ACTOR_SEPARATOR)
            .bind(Json(&prepared.assignment))
            .bind(DecisionCallState::Unknown.as_str()))
    }
}
