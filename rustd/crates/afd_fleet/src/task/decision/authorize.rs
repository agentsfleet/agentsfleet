//! Accounted current answers still pass reviewed thresholds and native targets.

use super::{
    row::{self, Call},
    semantic,
};
use crate::{
    Result,
    task::{TaskLease, TaskRefusal},
};
use afd_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionThreshold},
    task::{DecisionAssessment, DecisionReceipt, TaskBindingsInput, TaskInputAction},
};
use sqlx::PgConnection;
use std::collections::BTreeMap;

pub(super) struct Answered {
    pub call: Call,
    pub answers: BTreeMap<String, DecisionAnswer>,
}

pub(super) async fn load(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    id: &str,
) -> Result<Answered> {
    let call = row::read_on(connection, id)
        .await?
        .ok_or(TaskRefusal::NotFound)?;
    if !call.owned_by(lease) || !call.accounted || !call.issued {
        return Err(TaskRefusal::StaleState.into());
    }
    let payload = row::payload(connection, id)
        .await?
        .ok_or(TaskRefusal::DecisionUnavailable)?;
    let Some(receipt) = payload.result else {
        return Err(TaskRefusal::DecisionUnavailable.into());
    };
    let DecisionReceipt::Provider { result } = receipt.0 else {
        return Err(TaskRefusal::DecisionUnavailable.into());
    };
    if !semantic::answers_match(&call, &payload.request.0, &result)
        || !semantic::current_on(connection, &call).await?
    {
        return Err(TaskRefusal::StaleState.into());
    }
    let DecisionOutcome::Answered { answers } = result.result else {
        return Err(TaskRefusal::DecisionUnavailable.into());
    };
    Ok(Answered { call, answers })
}

pub(in crate::task) async fn binding(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    input: &TaskBindingsInput,
) -> Result<()> {
    let whole = load(connection, lease, &input.message_decision_call_id).await?;
    source(&whole, input)?;
    if whole.call.context_snapshot.assessment != DecisionAssessment::Message {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    let DecisionAnswer::Noul { noul } = whole
        .answers
        .get("multiple_requests")
        .ok_or(TaskRefusal::InputAmbiguous)?
    else {
        return Err(TaskRefusal::InputAmbiguous.into());
    };
    let policy = &whole.call.profile_snapshot.intent;
    let single = *noul <= f64::from(policy.single_request_max_bps) / 10_000.0;
    let multiple = *noul >= f64::from(policy.multiple_request_min_bps) / 10_000.0;
    if !single && !multiple || single && input.items.len() != 1 || multiple && input.items.len() < 2
    {
        return Err(TaskRefusal::InputAmbiguous.into());
    }
    for (ordinal, item) in input.items.iter().enumerate() {
        if single && item.decision_call_id.is_none() {
            route(&whole, &item.proposed)?;
        } else {
            let id = item
                .decision_call_id
                .as_deref()
                .ok_or(TaskRefusal::InputAmbiguous)?;
            let evaluated = load(connection, lease, id).await?;
            source(&evaluated, input)?;
            let expected = DecisionAssessment::Item {
                ordinal: u8::try_from(ordinal).map_err(|_overflow| TaskRefusal::InputAmbiguous)?,
                interpretation_digest: input.plan_digest.clone(),
                source_start: item.proposed.source_start,
                source_end: item.proposed.source_end,
            };
            if evaluated.call.context_snapshot.assessment != expected {
                return Err(TaskRefusal::ReplayConflict.into());
            }
            route(&evaluated, &item.proposed)?;
        }
    }
    Ok(())
}

fn source(evaluated: &Answered, input: &TaskBindingsInput) -> Result<()> {
    if evaluated.call.source_admission_id != input.source_admission_id
        || evaluated.call.context_snapshot.accepted_revision_id != input.accepted_revision_id
    {
        return Err(TaskRefusal::ReplayConflict.into());
    }
    Ok(())
}

fn route(evaluated: &Answered, item: &afd_wire::task::TaskProposedItem) -> Result<()> {
    let policy = &evaluated.call.profile_snapshot.intent;
    let intent = choice(&evaluated.answers, "intent", policy.intent)?;
    if intent != item.action.as_str() {
        return Err(TaskRefusal::InputAmbiguous.into());
    }
    if matches!(
        item.action,
        TaskInputAction::Conversation | TaskInputAction::Clarify
    ) {
        return if item.task_id.is_none() && item.target.is_none() {
            Ok(())
        } else {
            Err(TaskRefusal::InvalidTarget.into())
        };
    }
    let association = choice(&evaluated.answers, "association", policy.association)?;
    if item.action == TaskInputAction::NewWork {
        return if association == "new_work" && item.task_id.is_none() && item.target.is_some() {
            Ok(())
        } else {
            Err(TaskRefusal::InputAmbiguous.into())
        };
    }
    let ordinal = association
        .strip_prefix('t')
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(TaskRefusal::InputAmbiguous)?;
    let candidate = evaluated
        .call
        .context_snapshot
        .candidates
        .get(ordinal)
        .ok_or(TaskRefusal::InputAmbiguous)?;
    if item.task_id.as_deref() != Some(candidate.task_id.as_str()) || item.target.is_some() {
        return Err(TaskRefusal::InvalidTarget.into());
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
        return Err(TaskRefusal::InputAmbiguous.into());
    };
    if !confidence.is_finite()
        || *confidence < f64::from(threshold.confidence_bps) / 10_000.0
        || !probabilities.get(choice).is_some_and(|probability| {
            probability.is_finite()
                && *probability >= f64::from(threshold.probability_bps) / 10_000.0
        })
    {
        return Err(TaskRefusal::InputAmbiguous.into());
    }
    Ok(choice)
}

/// A native report accepts only its recorded goal call and reviewed thresholds.
pub(in crate::task) async fn goal(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    input: &afd_wire::task::TaskReport,
    task: &afd_wire::task::TaskContext<'_>,
) -> Result<()> {
    use afd_wire::task::TaskDisposition;
    if input.disposition == TaskDisposition::Blocked {
        return Ok(());
    }
    if input.disposition == TaskDisposition::Continue && input.decision_call_id.is_none() {
        return Ok(());
    }
    let evaluated = load(
        connection,
        lease,
        input
            .decision_call_id
            .as_deref()
            .ok_or(TaskRefusal::DecisionUnavailable)?,
    )
    .await?;
    if !matches!(
        evaluated.call.context_snapshot.assessment,
        DecisionAssessment::Goal { .. }
    ) || evaluated.call.context_snapshot.execution.as_ref()
        != Some(&super::context::TaskStamp::of(task))
    {
        return Err(TaskRefusal::StaleState.into());
    }
    let policy = &evaluated.call.profile_snapshot.goal;
    let (action, threshold) = match input.disposition {
        TaskDisposition::Complete => ("DONE", policy.done),
        TaskDisposition::Waiting => ("WAIT", policy.wait),
        TaskDisposition::Continue => ("CONTINUE", policy.continue_action),
        TaskDisposition::Blocked => return Ok(()),
    };
    if choice(&evaluated.answers, "next_action", threshold)? != action {
        return Err(TaskRefusal::InputAmbiguous.into());
    }
    if input.disposition == TaskDisposition::Complete
        && (task.questions.is_empty()
            || !task.questions.iter().all(|question| {
                matches!(evaluated.answers.get(&question.name), Some(DecisionAnswer::Noul { noul })
            if noul.is_finite() && *noul >= f64::from(policy.statement_min_bps) / 10_000.0)
            }))
    {
        return Err(TaskRefusal::InputAmbiguous.into());
    }
    Ok(())
}
