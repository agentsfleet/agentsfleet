//! Goal judgments follow completed root action groups and retain native proofs.
//!
//! The decision model judges the task's own completion statements
//! (`questions`) and the next action; it neither starts a task nor links a
//! message to one.

use std::collections::BTreeMap;

use super::{PreparedDurableTask, identity, unavailable, version};
use crate::Result;
use af_wire::{
    decision::{DecisionAnswer, DecisionAssignment, DecisionOutcome, DecisionThreshold},
    durable_task::{
        DecisionAssessment, DecisionCallState, DecisionRecordDisposition, DecisionReservationInput,
        DurableTaskDisposition, DurableTaskReport,
    },
};
use afr_tools::RunVerbs;
use afr_tools::verbs::DecisionEvaluation;

pub(crate) async fn assess(
    prepared: &PreparedDurableTask,
    assignment: &DecisionAssignment,
    verbs: &dyn RunVerbs,
    fence: u64,
    evidence: serde_json::Value,
) -> Result<DurableTaskReport> {
    let task = prepared
        .inputs
        .current_durable_task
        .as_ref()
        .ok_or_else(unavailable)?;
    let evaluated = verbs
        .durable_task_decision(DecisionReservationInput {
            operation_id: identity()?,
            fencing_token: fence,
            source_turn_id: prepared.inputs.source_turn_id.clone(),
            assessment: DecisionAssessment::Goal {
                action_group_id: identity()?,
            },
            durable_task: Some(version(task)),
            retry_of: None,
            evidence,
        })
        .await
        .map_err(crate::decision::native)?;
    let answers = answers(&evaluated)?;
    let Some(DecisionAnswer::Choice { choice: next, .. }) = answers.get("next_action") else {
        return Err(unavailable());
    };
    let (disposition, threshold) = match next.as_str() {
        "DONE" => (DurableTaskDisposition::Complete, assignment.goal.done),
        "WAIT" => (DurableTaskDisposition::Waiting, assignment.goal.wait),
        "CONTINUE" => (
            DurableTaskDisposition::Continue,
            assignment.goal.continue_action,
        ),
        _ => return Err(unavailable()),
    };
    choice(answers, "next_action", threshold)?;
    if disposition == DurableTaskDisposition::Complete
        && (task.questions.is_empty()
            || !task.questions.iter().all(|question| {
                matches!(answers.get(&question.name),
            Some(DecisionAnswer::Noul { noul }) if noul.is_finite()
                && *noul >= f64::from(assignment.goal.statement_min_bps) / 10_000.0)
            }))
    {
        return Err(unavailable());
    }
    Ok(DurableTaskReport {
        durable_task: version(task),
        disposition,
        decision_call_id: Some(evaluated.call_id),
        checkpoint_id: None,
        covered_input_sequence: task.covered_input_sequence,
        effect_watermark: task.confirmed_effect_watermark,
    })
}

/// The current, known answers of `evaluation`; a call still unknown, a
/// superseded record, a refusal or an invalid reply answers nothing.
fn answers(evaluation: &DecisionEvaluation) -> Result<&BTreeMap<String, DecisionAnswer>> {
    let recorded = evaluation.recorded.as_ref().ok_or_else(unavailable)?;
    if evaluation.state != DecisionCallState::Known
        || recorded.disposition != DecisionRecordDisposition::Current
    {
        return Err(unavailable());
    }
    match &recorded.result.result {
        DecisionOutcome::Answered { answers } => Ok(answers),
        DecisionOutcome::Refused | DecisionOutcome::Invalid { .. } => Err(unavailable()),
    }
}

/// The choice `name` answered, when its confidence and its own probability
/// both clear `threshold`.
fn choice<'a>(
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
        return Err(unavailable());
    };
    if !confidence.is_finite()
        || *confidence < f64::from(threshold.confidence_bps) / 10_000.0
        || !probabilities.get(choice).is_some_and(|probability| {
            probability.is_finite()
                && *probability >= f64::from(threshold.probability_bps) / 10_000.0
        })
    {
        return Err(unavailable());
    }
    Ok(choice)
}
