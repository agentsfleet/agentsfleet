//! Goal judgments follow completed root action groups and retain native proofs.

use super::{PreparedDurableTask, identity, routing, unavailable, version};
use crate::Result;
use af_wire::{
    decision::{DecisionAnswer, DecisionAssignment},
    durable_task::{
        DecisionAssessment, DecisionReservationInput, DurableTaskDisposition, DurableTaskReport,
    },
};
use afr_tools::RunVerbs;

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
    let answers = routing::answers(&evaluated)?;
    let Some(DecisionAnswer::Choice { choice, .. }) = answers.get("next_action") else {
        return Err(unavailable());
    };
    let (disposition, threshold) = match choice.as_str() {
        "DONE" => (DurableTaskDisposition::Complete, assignment.goal.done),
        "WAIT" => (DurableTaskDisposition::Waiting, assignment.goal.wait),
        "CONTINUE" => (
            DurableTaskDisposition::Continue,
            assignment.goal.continue_action,
        ),
        _ => return Err(unavailable()),
    };
    routing::choice(answers, "next_action", threshold)?;
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
