//! Reviewed thresholds select meaning; native binding remains the authority.

use super::unavailable;
use crate::Result;
use afd_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionThreshold, IntentDecisionPolicy},
    task::{DecisionCallState, DecisionRecordDisposition, TaskInputAction, TaskProposedItem},
};
use afr_tools::verbs::DecisionEvaluation;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Multiplicity {
    Single,
    Multiple,
}

pub(super) fn answers(
    evaluation: &DecisionEvaluation,
) -> Result<&BTreeMap<String, DecisionAnswer>> {
    let recorded = evaluation.recorded.as_ref().ok_or_else(unavailable)?;
    if evaluation.state != DecisionCallState::Known
        || recorded.disposition != DecisionRecordDisposition::Current
    {
        return Err(unavailable());
    }
    match &recorded.result.result {
        DecisionOutcome::Answered { answers } => Ok(answers),
        _ => Err(unavailable()),
    }
}

pub(super) fn multiplicity(
    answers: &BTreeMap<String, DecisionAnswer>,
    policy: &IntentDecisionPolicy,
) -> Result<Multiplicity> {
    let Some(DecisionAnswer::Noul { noul }) = answers.get("multiple_requests") else {
        return Err(unavailable());
    };
    if !noul.is_finite() || !(0.0..=1.0).contains(noul) {
        return Err(unavailable());
    }
    if *noul <= f64::from(policy.single_request_max_bps) / 10_000.0 {
        Ok(Multiplicity::Single)
    } else if *noul >= f64::from(policy.multiple_request_min_bps) / 10_000.0 {
        Ok(Multiplicity::Multiple)
    } else {
        Err(unavailable())
    }
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

pub(super) fn check(
    evaluation: &DecisionEvaluation,
    item: &TaskProposedItem,
    policy: &IntentDecisionPolicy,
) -> Result<()> {
    let answers = answers(evaluation)?;
    if choice(answers, "intent", policy.intent)? != item.action.as_str() {
        return Err(unavailable());
    }
    if matches!(
        item.action,
        TaskInputAction::Conversation | TaskInputAction::Clarify
    ) {
        return if item.task_id.is_none() && item.target.is_none() {
            Ok(())
        } else {
            Err(unavailable())
        };
    }
    let association = choice(answers, "association", policy.association)?;
    if item.action == TaskInputAction::NewWork {
        return if association == "new_work" && item.task_id.is_none() && item.target.is_some() {
            Ok(())
        } else {
            Err(unavailable())
        };
    }
    // Resolve the alias from the exact native request which was assessed, never
    // the earlier candidate page: another task may have changed between reads.
    let recorded = evaluation.recorded.as_ref().ok_or_else(unavailable)?;
    let candidates = recorded
        .request
        .state
        .get("current_work")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(unavailable)?;
    let candidate = candidates
        .iter()
        .find(|candidate| {
            candidate.get("alias").and_then(serde_json::Value::as_str) == Some(association)
        })
        .ok_or_else(unavailable)?;
    if item.task_id.as_deref() != candidate.get("task_id").and_then(serde_json::Value::as_str)
        || item.target.is_some()
    {
        return Err(unavailable());
    }
    Ok(())
}
