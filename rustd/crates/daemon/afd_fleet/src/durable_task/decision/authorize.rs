//! Accounted current answers still pass reviewed thresholds and native targets.

use super::{
    row::{self, Call},
    semantic,
};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun},
};
use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionAnswer, DecisionThreshold, statements_hold},
    durable_task::{DecisionAssessment, DecisionReceipt},
};
use sqlx::{PgConnection, types::Json};
use std::collections::BTreeMap;

pub(super) struct Answered {
    pub call: Call,
    pub answers: BTreeMap<String, DecisionAnswer>,
}

pub(super) async fn load(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    id: &Uuid7,
) -> Result<Answered> {
    let call = row::read_on(connection, id)
        .await?
        .ok_or(DurableTaskRefusal::NotFound)?;
    if !call.owned_by(run) || !call.accounted || !call.issued {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    // Only a provider's answers authorize: an erased payload, or a receipt that
    // proved nothing was sent, leaves the decision unavailable.
    let Some(row::Payload {
        request,
        result: Some(Json(DecisionReceipt::Provider { result })),
    }) = row::payload(connection, id).await?
    else {
        return Err(DurableTaskRefusal::DecisionUnavailable.into());
    };
    let Some(answers) = semantic::answered(&call, &request.0, &result).cloned() else {
        return Err(DurableTaskRefusal::StaleState.into());
    };
    if !semantic::current_on(connection, &call).await? {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok(Answered { call, answers })
}

pub(super) fn choice<'a>(
    answers: &'a BTreeMap<String, DecisionAnswer>,
    name: &str,
    threshold: DecisionThreshold,
) -> Result<&'a str> {
    let Some(DecisionAnswer::Choice {
        choice,
        confidence: Some(confidence),
        probabilities,
    }) = answers.get(name)
    else {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    };
    if !confidence.is_finite()
        || *confidence < f64::from(threshold.confidence_bps) / 10_000.0
        || !probabilities.get(choice).is_some_and(|probability| {
            probability.is_finite()
                && *probability >= f64::from(threshold.probability_bps) / 10_000.0
        })
    {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    Ok(choice)
}

/// A native report accepts only its recorded goal call and reviewed thresholds:
/// a completion by the call's statements alone, a wait or a continuation by its
/// next action.
pub(in crate::durable_task) async fn goal(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &af_wire::durable_task::DurableTaskReport,
    task: &af_wire::durable_task::DurableTaskContext<'_>,
) -> Result<()> {
    use af_wire::{decision::GoalDecisionPolicy, durable_task::DurableTaskDisposition};
    type Threshold = fn(&GoalDecisionPolicy) -> DecisionThreshold;
    let next: Option<(&str, Threshold)> = match input.disposition {
        DurableTaskDisposition::Blocked => return Ok(()),
        DurableTaskDisposition::Continue if input.decision_call_id.is_none() => return Ok(()),
        DurableTaskDisposition::Complete => None,
        DurableTaskDisposition::Waiting => Some(("WAIT", |policy| policy.wait)),
        DurableTaskDisposition::Continue => Some(("CONTINUE", |policy| policy.continue_action)),
    };
    let evaluated = load(
        connection,
        run,
        &crate::durable_task::guard::wire(
            input
                .decision_call_id
                .as_deref()
                .ok_or(DurableTaskRefusal::DecisionUnavailable)?,
        )?,
    )
    .await?;
    if !matches!(
        evaluated.call.context_snapshot.assessment,
        DecisionAssessment::Goal { .. }
    ) || evaluated.call.context_snapshot.execution.as_ref()
        != super::context::DurableTaskStamp::of(task).as_ref()
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let policy = &evaluated.call.profile_snapshot.goal;
    let authorized = match next {
        None => statements_hold(
            &task.questions,
            &evaluated.answers,
            policy.statement_min_bps,
        ),
        Some((action, threshold)) => {
            choice(&evaluated.answers, "next_action", threshold(policy))? == action
        }
    };
    if !authorized {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    Ok(())
}
