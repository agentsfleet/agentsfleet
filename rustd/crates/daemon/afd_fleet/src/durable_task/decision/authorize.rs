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
    decision::{DecisionAnswer, DecisionThreshold},
    durable_task::{
        DecisionAssessment, DecisionReceipt, DurableTaskBindingsInput, DurableTaskInputAction,
    },
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

pub(in crate::durable_task) async fn binding(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &DurableTaskBindingsInput,
) -> Result<()> {
    let whole = load(
        connection,
        run,
        &crate::durable_task::guard::wire(&input.message_decision_call_id)?,
    )
    .await?;
    source(&whole, input)?;
    if whole.call.context_snapshot.assessment != DecisionAssessment::Message {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    let DecisionAnswer::Noul { noul } = whole
        .answers
        .get("multiple_requests")
        .ok_or(DurableTaskRefusal::InputAmbiguous)?
    else {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    };
    let policy = &whole.call.profile_snapshot.intent;
    let single = *noul <= f64::from(policy.single_request_max_bps) / 10_000.0;
    let multiple = *noul >= f64::from(policy.multiple_request_min_bps) / 10_000.0;
    if !single && !multiple || single && input.items.len() != 1 || multiple && input.items.len() < 2
    {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    for (ordinal, item) in input.items.iter().enumerate() {
        if single && item.decision_call_id.is_none() {
            route(&whole, &item.proposed)?;
        } else {
            let id = item
                .decision_call_id
                .as_deref()
                .ok_or(DurableTaskRefusal::InputAmbiguous)?;
            let evaluated = load(connection, run, &crate::durable_task::guard::wire(id)?).await?;
            source(&evaluated, input)?;
            let expected = DecisionAssessment::Item {
                ordinal: u8::try_from(ordinal)
                    .map_err(|_overflow| DurableTaskRefusal::InputAmbiguous)?,
                interpretation_digest: input.plan_digest.clone(),
                source_start: item.proposed.source_start,
                source_end: item.proposed.source_end,
            };
            if evaluated.call.context_snapshot.assessment != expected {
                return Err(DurableTaskRefusal::ReplayConflict.into());
            }
            route(&evaluated, &item.proposed)?;
        }
    }
    Ok(())
}

fn source(evaluated: &Answered, input: &DurableTaskBindingsInput) -> Result<()> {
    if evaluated.call.source_turn_id != input.source_turn_id
        || evaluated.call.context_snapshot.accepted_revision_id != input.accepted_revision_id
    {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    Ok(())
}

fn route(
    evaluated: &Answered,
    item: &af_wire::durable_task::DurableTaskProposedItem,
) -> Result<()> {
    let policy = &evaluated.call.profile_snapshot.intent;
    let intent = choice(&evaluated.answers, "intent", policy.intent)?;
    if intent != item.action.as_str() {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    if matches!(
        item.action,
        DurableTaskInputAction::Conversation | DurableTaskInputAction::Clarify
    ) {
        return if item.durable_task_id.is_none() && item.target.is_none() {
            Ok(())
        } else {
            Err(DurableTaskRefusal::InvalidTarget.into())
        };
    }
    let association = choice(&evaluated.answers, "association", policy.association)?;
    if item.action == DurableTaskInputAction::NewWork {
        return if association == "new_work"
            && item.durable_task_id.is_none()
            && item.target.is_some()
        {
            Ok(())
        } else {
            Err(DurableTaskRefusal::InputAmbiguous.into())
        };
    }
    let ordinal = association
        .strip_prefix('t')
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(DurableTaskRefusal::InputAmbiguous)?;
    let candidate = evaluated
        .call
        .context_snapshot
        .candidates
        .get(ordinal)
        .ok_or(DurableTaskRefusal::InputAmbiguous)?;
    if item.durable_task_id.as_deref() != Some(candidate.durable_task_id.as_str())
        || item.target.is_some()
    {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    Ok(())
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

/// A native report accepts only its recorded goal call and reviewed thresholds.
pub(in crate::durable_task) async fn goal(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &af_wire::durable_task::DurableTaskReport,
    task: &af_wire::durable_task::DurableTaskContext<'_>,
) -> Result<()> {
    use af_wire::{decision::GoalDecisionPolicy, durable_task::DurableTaskDisposition};
    type Threshold = fn(&GoalDecisionPolicy) -> DecisionThreshold;
    let (action, threshold): (&str, Threshold) = match input.disposition {
        DurableTaskDisposition::Blocked => return Ok(()),
        DurableTaskDisposition::Continue if input.decision_call_id.is_none() => return Ok(()),
        DurableTaskDisposition::Complete => ("DONE", |policy| policy.done),
        DurableTaskDisposition::Waiting => ("WAIT", |policy| policy.wait),
        DurableTaskDisposition::Continue => ("CONTINUE", |policy| policy.continue_action),
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
    if choice(&evaluated.answers, "next_action", threshold(policy))? != action {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    if input.disposition == DurableTaskDisposition::Complete
        && (task.questions.is_empty()
            || !task.questions.iter().all(|question| {
                matches!(evaluated.answers.get(&question.name), Some(DecisionAnswer::Noul { noul })
            if noul.is_finite() && *noul >= f64::from(policy.statement_min_bps) / 10_000.0)
            }))
    {
        return Err(DurableTaskRefusal::InputAmbiguous.into());
    }
    Ok(())
}
