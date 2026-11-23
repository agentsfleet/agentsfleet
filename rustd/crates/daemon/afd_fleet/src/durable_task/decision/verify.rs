//! Fresh authority checks reuse saved questions, prices and logical identity.

use af_wire::durable_task::DecisionReservationInput;
use sqlx::PgConnection;

use super::{context::AssessedContext, ownership::NativeContext, prepare, row::Call};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTaskSnapshot, guard},
};

pub(super) struct Verified {
    pub current: guard::RunRow,
    pub context: AssessedContext,
    pub task: Option<DurableTaskSnapshot>,
}

pub(super) async fn on(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
    call: &Call,
) -> Result<Verified> {
    let native = NativeContext::load(connection, run, input).await?;
    let assignment = native
        .current
        .decision_snapshot
        .as_ref()
        .ok_or(DurableTaskRefusal::DecisionUnavailable)?;
    if assignment.0 != call.profile_snapshot.0
        || prepare::enabled(connection, &assignment.0).await? != call.provider
    {
        return Err(DurableTaskRefusal::DecisionUnavailable.into());
    }
    let candidates =
        prepare::candidates(connection, &native.scope, input.assessment.purpose()).await?;
    let context = native.assessed(
        input,
        &assignment.0.revision_id,
        call.context_snapshot.0.questions_digest.clone(),
        &candidates,
    )?;
    if context.key()? != call.assessment_key {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok(Verified {
        current: native.current,
        context,
        task: native.task.map(|task| task.snapshot),
    })
}
