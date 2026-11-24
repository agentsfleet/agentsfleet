//! Native source and accepted objective remain complete within the state bound.

use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionRequest, NamedQuestion},
    durable_task::DecisionReservationInput,
};
use serde::Serialize;

use super::{
    context::{AssessedContext, DurableTaskStamp},
    prepare::Candidate,
};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskSnapshot},
};

const _: () = assert!(usize::BITS >= u32::BITS);

#[derive(Serialize)]
struct State<'a> {
    source_turn_id: &'a Uuid7,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    assessed_message: Option<&'a str>,
    assessment: &'a af_wire::durable_task::DecisionAssessment,
    current_work: &'a [Candidate],
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

/// The part of `message` an item assessment names, or `None` for a whole message or a
/// goal. A range that does not fall on characters of `message` is an invalid target; the
/// reservation judges it before it assigns the source turn, so a malformed range changes
/// nothing.
pub(super) fn assessed<'m>(
    input: &DecisionReservationInput,
    message: &'m str,
) -> Result<Option<&'m str>> {
    match &input.assessment {
        af_wire::durable_task::DecisionAssessment::Item {
            source_start,
            source_end,
            ..
        } => {
            // Validation proves nonnegative offsets; the width assertion preserves every u32.
            let span = usize::try_from(source_start.unsigned_abs()).unwrap_or(usize::MAX)
                ..usize::try_from(source_end.unsigned_abs()).unwrap_or(usize::MAX);
            Ok(Some(
                message.get(span).ok_or(DurableTaskRefusal::InvalidTarget)?,
            ))
        }
        af_wire::durable_task::DecisionAssessment::Message
        | af_wire::durable_task::DecisionAssessment::Goal { .. } => Ok(None),
    }
}

pub(super) fn compile(
    call: &Uuid7,
    input: &garde::Valid<DecisionReservationInput>,
    message: &str,
    context: &AssessedContext,
    task: Option<&DurableTaskSnapshot>,
    candidates: &[Candidate],
    questions: Vec<NamedQuestion>,
) -> Result<DecisionRequest> {
    let assessed_message = assessed(input, message)?;
    let source_turn_id = crate::durable_task::guard::wire(&input.source_turn_id)?;
    let state = State {
        source_turn_id: &source_turn_id,
        message,
        assessed_message,
        assessment: &input.assessment,
        current_work: candidates,
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
