//! Resolve current native ownership before preparing any paid request.

use afd_billing::price::PriceBook;
use afd_core::id::Uuid7;
use afd_wire::{
    decision::{DecisionAssignment, DecisionPurpose, DecisionRequest},
    task::{DecisionAssessment, DecisionReservationInput, TaskState, UNFINISHED_MAX},
};
use serde::Serialize;
use sqlx::{PgConnection, types::Json};

use super::{
    compile,
    context::{AssessedContext, TaskStamp},
    quote::FrozenQuote,
    sql,
};
use crate::{
    Result,
    error::query,
    lease::Leases,
    task::{TaskLease, TaskRefusal, TaskScope, TaskSnapshot, guard},
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(super) struct Candidate {
    #[sqlx(default)]
    pub alias: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub stamp: TaskStamp,
    title: String,
    target: Json<serde_json::Value>,
}

pub(super) struct Prepared {
    pub current: guard::LeaseRow,
    pub assignment: DecisionAssignment,
    pub context: AssessedContext,
    pub request: DecisionRequest,
    pub provider: String,
    pub task: Option<TaskSnapshot>,
}

pub(super) async fn on(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: &Uuid7,
) -> Result<(Prepared, FrozenQuote)> {
    let native = super::ownership::NativeContext::load(connection, lease, input).await?;
    native.prepare(connection, input, call).await
}

impl super::ownership::NativeContext {
    async fn prepare(
        self,
        connection: &mut PgConnection,
        input: &DecisionReservationInput,
        call: &Uuid7,
    ) -> Result<(Prepared, FrozenQuote)> {
        let assignment = self
            .current
            .decision_snapshot
            .as_ref()
            .ok_or(TaskRefusal::DecisionUnavailable)?
            .0
            .clone();
        let provider = enabled(connection, &assignment).await?;
        let revision = self.revision(input.assessment.purpose())?;
        let config = Leases::stored_config_on(connection, &self.current.fleet_id, Some(revision))
            .await?
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        let policy = config
            .decision_policy()
            .ok_or(TaskRefusal::DecisionUnavailable)?;
        let candidates = candidates(connection, &self.scope, input.assessment.purpose()).await?;
        let questions = questions(
            policy,
            input,
            self.task.as_ref(),
            &candidates,
            self.current.checked_at,
        )?;
        let context = self.assessed(
            input,
            &assignment.revision_id,
            afd_core::etag::sha256_hex(
                &serde_json::to_vec(&questions).map_err(afd_core::json::Unreadable::from)?,
            ),
            &candidates,
        )?;
        let request = super::request::compile(
            call,
            input,
            &self.source.request_json,
            &context,
            self.task.as_ref(),
            &candidates,
            questions,
        )?;
        garde::Validate::validate(&request)?;
        supported(&assignment, &request)?;
        let quote = quote(connection, &assignment, self.current.checked_at).await?;
        Ok((
            Prepared {
                current: self.current,
                assignment,
                context,
                request,
                provider,
                task: self.task,
            },
            quote,
        ))
    }
}

async fn quote(
    connection: &mut PgConnection,
    assignment: &DecisionAssignment,
    at: jiff::Timestamp,
) -> Result<FrozenQuote> {
    let model = guard::id(&assignment.profile.model_id, "decision_model_id")?;
    let profile = guard::id(&assignment.revision_id, "decision_revision_id")?;
    let book = PriceBook::load_decision(connection, &model, &profile, at).await?;
    if book.quote(at, 0).is_none() {
        return Err(TaskRefusal::DecisionUnavailable.into());
    }
    Ok(FrozenQuote { at, book })
}

pub(super) async fn enabled(
    connection: &mut PgConnection,
    assignment: &DecisionAssignment,
) -> Result<String> {
    sqlx::query_scalar(sql::PROFILE)
        .bind(&assignment.profile.model_id)
        .bind(&assignment.revision_id)
        .fetch_optional(connection)
        .await
        .map_err(query("check frozen decision revision activation"))?
        .ok_or_else(|| TaskRefusal::DecisionUnavailable.into())
}

pub(super) async fn candidates(
    connection: &mut PgConnection,
    scope: &TaskScope,
    purpose: DecisionPurpose,
) -> Result<Vec<Candidate>> {
    if purpose == DecisionPurpose::Goal {
        return Ok(Vec::new());
    }
    let mut rows: Vec<Candidate> = sqlx::query_as(sql::CANDIDATES)
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .bind(i64::from(UNFINISHED_MAX) + 1)
        .fetch_all(connection)
        .await
        .map_err(query("read complete native decision candidates"))?;
    if rows.len() > UNFINISHED_MAX as usize {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    for (ordinal, candidate) in rows.iter_mut().enumerate() {
        crate::task::row::state(&candidate.stamp.state)?;
        candidate.alias = format!("t{ordinal}");
    }
    Ok(rows)
}

fn questions(
    policy: &afd_fleet_runtime::config::decision::DecisionPolicy,
    input: &DecisionReservationInput,
    task: Option<&TaskSnapshot>,
    candidates: &[Candidate],
    now: jiff::Timestamp,
) -> Result<Vec<afd_wire::decision::NamedQuestion>> {
    if !matches!(input.assessment, DecisionAssessment::Goal { .. }) {
        return compile::routing(
            policy,
            candidates,
            matches!(input.assessment, DecisionAssessment::Message),
        );
    }
    let task = &task.ok_or(TaskRefusal::InvalidTarget)?.context;
    if task.state != TaskState::Running || task.expires_at <= now {
        return Err(TaskRefusal::StaleState.into());
    }
    let mut questions = compile::completion(policy)?;
    if questions != task.questions {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    questions.push(compile::next_action(policy));
    Ok(questions)
}

fn supported(assignment: &DecisionAssignment, request: &DecisionRequest) -> Result<()> {
    if !request
        .questions
        .iter()
        .all(|named| assignment.profile.capabilities.supports(&named.question))
        || !afd_wire::decision::serialized_within(
            &(&request.state, &request.questions),
            assignment.profile.input_max_bytes as usize,
        )
    {
        return Err(TaskRefusal::DecisionUnavailable.into());
    }
    Ok(())
}
