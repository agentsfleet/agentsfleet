//! Native source and accepted objective remain complete within the state bound.

use afd_core::id::Uuid7;
use afd_wire::{
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

#[derive(Serialize)]
struct State<'a> {
    source_admission_id: &'a str,
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    assessed_message: Option<&'a str>,
    assessment: &'a afd_wire::task::DecisionAssessment,
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
    input: &DecisionReservationInput,
    source: &str,
    context: &AssessedContext,
    task: Option<&TaskSnapshot>,
    candidates: &[Candidate],
    questions: Vec<NamedQuestion>,
) -> Result<DecisionRequest> {
    let message = afd_wire::event::message_of(source);
    let assessed_message = if matches!(
        input.assessment,
        afd_wire::task::DecisionAssessment::Item { .. }
    ) {
        Some(assessed(&message, &input.assessment)?)
    } else {
        None
    };
    let state = State {
        source_admission_id: &input.source_admission_id,
        message: &message,
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
        state: serde_json::to_value(state).map_err(afd_core::json::Unreadable::from)?,
        questions,
    })
}

fn assessed<'a>(
    message: &'a str,
    assessment: &afd_wire::task::DecisionAssessment,
) -> Result<&'a str> {
    match assessment {
        afd_wire::task::DecisionAssessment::Item {
            source_start,
            source_end,
            ..
        } => {
            let start = usize::try_from(*source_start).map_err(|source| {
                crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                    column: "source_start",
                    source,
                })
            })?;
            let end = usize::try_from(*source_end).map_err(|source| {
                crate::error::Error::from(crate::error::ErrorKind::TaskCounter {
                    column: "source_end",
                    source,
                })
            })?;
            message
                .get(start..end)
                .ok_or_else(|| TaskRefusal::InvalidTarget.into())
        }
        _ => Ok(message),
    }
}
