//! Goal judgments follow completed root action groups and retain native proofs.

use super::{PreparedTask, identity, routing, unavailable, version};
use crate::Result;
use af_wire::{
    decision::{DecisionAnswer, DecisionAssignment},
    task::{DecisionAssessment, DecisionReservationInput, TaskDisposition, TaskReport},
};
use afr_tools::LeaseVerbs;

pub(crate) async fn assess(
    prepared: &PreparedTask,
    assignment: &DecisionAssignment,
    verbs: &dyn LeaseVerbs,
    fence: u64,
    evidence: serde_json::Value,
) -> Result<TaskReport> {
    let task = prepared
        .inputs
        .current_task
        .as_ref()
        .ok_or_else(unavailable)?;
    let evaluated = verbs
        .task_decision(DecisionReservationInput {
            operation_id: identity()?,
            fencing_token: fence,
            source_admission_id: prepared.inputs.source_admission_id.clone(),
            assessment: DecisionAssessment::Goal {
                action_group_id: identity()?,
            },
            task: Some(version(task)),
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
        "DONE" => (TaskDisposition::Complete, assignment.goal.done),
        "WAIT" => (TaskDisposition::Waiting, assignment.goal.wait),
        "CONTINUE" => (TaskDisposition::Continue, assignment.goal.continue_action),
        _ => return Err(unavailable()),
    };
    routing::choice(answers, "next_action", threshold)?;
    if disposition == TaskDisposition::Complete
        && (task.questions.is_empty()
            || !task.questions.iter().all(|question| {
                matches!(answers.get(&question.name),
            Some(DecisionAnswer::Noul { noul }) if noul.is_finite()
                && *noul >= f64::from(assignment.goal.statement_min_bps) / 10_000.0)
            }))
    {
        return Err(unavailable());
    }
    Ok(TaskReport {
        task: version(task),
        disposition,
        decision_call_id: Some(evaluated.call_id),
        checkpoint_id: None,
        covered_input_sequence: task.covered_input_sequence,
        effect_watermark: task.confirmed_effect_watermark,
    })
}
