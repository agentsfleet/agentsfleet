//! Resolve current native ownership before preparing any paid request.

use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionAssignment, DecisionPurpose, DecisionRequest},
    durable_task::{DecisionAssessment, DecisionReservationInput, DurableTaskState},
};
use afd_billing::price::PriceBook;
use serde::Serialize;
use sqlx::{PgConnection, types::Json};

use super::{
    compile,
    context::{AssessedContext, DurableTaskStamp},
    ownership::Execution,
    quote::FrozenQuote,
    sql,
};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, guard,
    },
    error::query,
    run::Runs,
};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub(super) struct Candidate {
    #[sqlx(default)]
    pub alias: String,
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub stamp: DurableTaskStamp,
    title: String,
    target: Json<serde_json::Value>,
}

pub(super) struct Prepared {
    pub current: guard::RunRow,
    pub assignment: DecisionAssignment,
    pub context: AssessedContext,
    pub request: DecisionRequest,
    pub provider: String,
    pub task: Option<Execution>,
}

pub(super) async fn on(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: &Uuid7,
) -> Result<(Prepared, FrozenQuote)> {
    let native = super::ownership::NativeContext::load(connection, run, input).await?;
    native.prepare(connection, input, call).await
}

impl super::ownership::NativeContext {
    async fn prepare(
        self,
        connection: &mut PgConnection,
        input: &garde::Valid<DecisionReservationInput>,
        call: &Uuid7,
    ) -> Result<(Prepared, FrozenQuote)> {
        let assignment = self
            .current
            .decision_snapshot
            .as_ref()
            .ok_or(DurableTaskRefusal::DecisionUnavailable)?
            .0
            .clone();
        let provider = enabled(connection, &assignment).await?;
        let revision = self.revision(input.assessment.purpose())?;
        let config = Runs::stored_config_on(
            connection,
            &self.current.fleet_id,
            Some(&guard::wire(revision)?),
        )
        .await?
        .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
        let policy = config
            .decision_policy()
            .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
        let candidates = candidates(connection, &self.scope, input.assessment.purpose()).await?;
        let questions = questions(
            policy,
            input,
            self.task.as_ref().map(|task| &task.snapshot),
            &candidates,
            self.current.checked_at,
        )?;
        let context = self.assessed(
            input,
            &assignment.revision_id,
            af_core::etag::sha256_hex(
                &serde_json::to_vec(&questions).map_err(af_core::json::Unreadable::from)?,
            ),
            &candidates,
        )?;
        let request = super::request::compile(
            call,
            input,
            &self.source.message()?,
            &context,
            self.task.as_ref().map(|task| &task.snapshot),
            &candidates,
            questions,
        )?;
        // Judged before the request's own validation, so an oversize state
        // reads as too large rather than as a native fault.
        supported(&assignment, &request)?;
        garde::Validate::validate(&request)?;
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
        return Err(DurableTaskRefusal::DecisionUnavailable.into());
    }
    Ok(FrozenQuote { at, book })
}

pub(super) async fn enabled(
    connection: &mut PgConnection,
    assignment: &DecisionAssignment,
) -> Result<String> {
    sqlx::query_scalar(sql::PROFILE)
        .bind(guard::wire(&assignment.profile.model_id)?)
        .bind(guard::wire(&assignment.revision_id)?)
        .fetch_optional(connection)
        .await
        .map_err(query("check frozen decision revision activation"))?
        .ok_or_else(|| DurableTaskRefusal::DecisionUnavailable.into())
}

pub(super) async fn candidates(
    connection: &mut PgConnection,
    scope: &DurableTaskScope,
    purpose: DecisionPurpose,
) -> Result<Vec<Candidate>> {
    if purpose == DecisionPurpose::Goal {
        return Ok(Vec::new());
    }
    let mut rows: Vec<Candidate> = sqlx::query_as(sql::CANDIDATES)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .fetch_all(connection)
        .await
        .map_err(query("read complete native decision candidates"))?;
    for (ordinal, candidate) in rows.iter_mut().enumerate() {
        crate::durable_task::row::state(&candidate.stamp.state)?;
        candidate.alias = format!("t{ordinal}");
    }
    Ok(rows)
}

fn questions(
    policy: &af_fleet_runtime::config::decision::DecisionPolicy,
    input: &DecisionReservationInput,
    task: Option<&DurableTaskSnapshot>,
    candidates: &[Candidate],
    now: jiff::Timestamp,
) -> Result<Vec<af_wire::decision::NamedQuestion>> {
    if !matches!(input.assessment, DecisionAssessment::Goal { .. }) {
        return compile::routing(
            policy,
            candidates,
            matches!(input.assessment, DecisionAssessment::Message),
        );
    }
    let task = &task.ok_or(DurableTaskRefusal::InvalidTarget)?.context;
    if task.state != DurableTaskState::Running || task.expires_at <= now {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let mut questions = compile::completion(policy)?;
    if questions != task.questions {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    questions.push(compile::next_action(policy));
    Ok(questions)
}

/// A question the profile cannot answer is unavailable whatever the evidence.
/// A request the model cannot read whole (state and questions past the
/// profile's reviewed input ceiling) is refused as too large, before any
/// reservation, so the caller may send a shorter description of the same
/// evidence.
fn supported(assignment: &DecisionAssignment, request: &DecisionRequest) -> Result<()> {
    if !request
        .questions
        .iter()
        .all(|named| assignment.profile.capabilities.supports(&named.question))
    {
        return Err(DurableTaskRefusal::DecisionUnavailable.into());
    }
    if !af_validate::serialized_within(
        &(&request.state, &request.questions),
        usize::try_from(assignment.profile.input_max_bytes).unwrap_or(usize::MAX),
    ) {
        return Err(DurableTaskRefusal::DecisionInputTooLarge.into());
    }
    Ok(())
}

#[cfg(test)]
#[path = "prepare_tests.rs"]
mod tests;
