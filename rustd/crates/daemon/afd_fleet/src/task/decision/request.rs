//! Native source and accepted objective remain complete within the state bound.

use af_core::id::Uuid7;
use af_wire::{
    decision::{DecisionRequest, NamedQuestion},
    task::DecisionReservationInput,
};
use serde::Serialize;

use super::{
    context::{AssessedContext, TaskStamp},
    prepare::Candidate,
};
use crate::{
    Result,
    task::{TaskRefusal, TaskSnapshot},
};

const _: () = assert!(usize::BITS >= u32::BITS);

#[derive(Serialize)]
struct State<'a> {
    source_admission_id: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    assessed_message: Option<&'a str>,
    assessment: &'a af_wire::task::DecisionAssessment,
    current_work: &'a [Candidate],
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<Objective<'a>>,
    evidence: &'a serde_json::Value,
}

#[derive(Serialize)]
struct Objective<'a> {
    #[serde(flatten)]
    stamp: &'a TaskStamp,
    objective: &'a str,
}

pub(super) fn compile(
    call: &Uuid7,
    input: &garde::Valid<DecisionReservationInput>,
    message: &str,
    context: &AssessedContext,
    task: Option<&TaskSnapshot>,
    candidates: &[Candidate],
    questions: Vec<NamedQuestion>,
) -> Result<DecisionRequest> {
    let assessed_message = match &input.assessment {
        af_wire::task::DecisionAssessment::Item {
            source_start,
            source_end,
            ..
        } => {
            // Validation proves nonnegative offsets; the width assertion preserves every u32.
            let span = usize::try_from(source_start.unsigned_abs()).unwrap_or(usize::MAX)
                ..usize::try_from(source_end.unsigned_abs()).unwrap_or(usize::MAX);
            Some(message.get(span).ok_or(TaskRefusal::InvalidTarget)?)
        }
        af_wire::task::DecisionAssessment::Message
        | af_wire::task::DecisionAssessment::Goal { .. } => None,
    };
    let state = State {
        source_admission_id: &input.source_admission_id,
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
