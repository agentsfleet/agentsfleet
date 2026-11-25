//! Native source and accepted objective remain complete within the state bound.

use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionRequest, NamedQuestion},
    durable_task::DecisionReservationInput,
};
use serde::Serialize;

use super::context::{AssessedContext, DurableTaskStamp};
use crate::{Result, durable_task::DurableTaskSnapshot};

#[derive(Serialize)]
struct State<'a> {
    source_turn_id: &'a Uuid7,
    message: &'a str,
    assessment: &'a af_wire::durable_task::DecisionAssessment,
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<Objective<'a>>,
    evidence: &'a serde_json::Value,
}

#[derive(Serialize)]
struct Objective<'a> {
    #[serde(flatten)]
    stamp: &'a DurableTaskStamp,
    objective: &'a str,
}

pub(super) fn compile(
    call: &Uuid7,
    input: &garde::Valid<DecisionReservationInput>,
    message: &str,
    context: &AssessedContext,
    task: Option<&DurableTaskSnapshot>,
    questions: Vec<NamedQuestion>,
) -> Result<DecisionRequest> {
    let source_turn_id = crate::durable_task::guard::wire(&input.source_turn_id)?;
    let state = State {
        source_turn_id: &source_turn_id,
        message,
        assessment: &input.assessment,
        task: context
            .execution
            .as_ref()
            .zip(task)
            .map(|(stamp, task)| Objective {
                stamp,
                objective: task.context.objective.as_ref(),
            }),
        evidence: &input.evidence,
    };
    Ok(DecisionRequest {
        call_id: call.to_string(),
        purpose: input.assessment.purpose(),
        state: serde_json::to_value(state).map_err(af_core::json::Unreadable::from)?,
        questions,
    })
}
