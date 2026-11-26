//! Resolve current native ownership before preparing any paid request.

use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionAssignment, DecisionRequest, NamedQuestion},
    durable_task::{DecisionReservationInput, DurableTaskState},
};
use afd_billing::price::PriceBook;
use sqlx::PgConnection;

use super::{compile, context::AssessedContext, ownership::Execution, quote::FrozenQuote, sql};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTaskSnapshot, guard},
    error::query,
};

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
        let questions = questions(
            self.task.as_ref().map(|task| &task.snapshot),
            self.current.checked_at,
        )?;
        let context = self.assessed(
            input,
            &assignment.revision_id,
            af_core::etag::sha256_hex(
                &serde_json::to_vec(&questions).map_err(af_core::json::Unreadable::from)?,
            ),
        )?;
        let request = super::request::compile(
            call,
            input,
            &self.source.message()?,
            &context,
            self.task.as_ref().map(|task| &task.snapshot),
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

/// What a goal check asks: the task's own statements, then the native
/// next-action question.
fn questions(
    task: Option<&DurableTaskSnapshot>,
    now: jiff::Timestamp,
) -> Result<Vec<NamedQuestion>> {
    let task = &task.ok_or(DurableTaskRefusal::InvalidTarget)?.context;
    if task.state != DurableTaskState::Running || task.expires_at <= now {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let mut questions = task.questions.clone();
    questions.push(compile::next_action(&task.target));
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
