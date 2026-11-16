//! The fixed proposal tool commits through the daemon's native validator.

use crate::{
    Result,
    client::{ControlPlane, retrying},
    error,
};
use af_core::id::Uuid7;
use af_fleet_runtime::Definition;
use af_wire::authoring::{
    ActivityEvent, ActivityRequest, Lease, ModelMessage, Proposal, ProposalCall, ProposalRejection,
};
use serde_json::Value;

pub(super) async fn apply(
    plane: &ControlPlane,
    id: &Uuid7,
    lease: &Lease<Value>,
    sequence: u64,
    call: &ProposalCall,
) -> Result<(ModelMessage, bool)> {
    let (call_id, proposal) = match call {
        ProposalCall::Proposed { id, proposal } => (id, proposal),
        ProposalCall::Refused { id, reason } => {
            return Ok((
                ModelMessage::Tool {
                    call_id: id.clone(),
                    text: feedback(*reason).to_owned(),
                },
                false,
            ));
        }
    };
    if let Some(text) = initial_refusal(proposal) {
        return Ok((
            ModelMessage::Tool {
                call_id: call_id.clone(),
                text,
            },
            false,
        ));
    }
    let request = ActivityRequest {
        fence: lease.fence.clone(),
        sequence: lease.sequence.clone(),
        activity_sequence: sequence.to_string(),
        event: ActivityEvent::Proposal {
            proposal: proposal.clone(),
        },
    };
    let receipt = retrying(|| plane.authoring_activity(id, &request)).await;
    let (text, accepted) = match receipt {
        Ok(receipt) if receipt.accepted => (
            serde_json::to_string(&receipt).map_err(error::encode)?,
            true,
        ),
        Ok(_) => return Err(error::config("the daemon did not accept the proposal")),
        Err(failure) if failure.refusal_code() == Some(af_core::error_code::INVALID_REQUEST) => (
            "The proposal is invalid. Correct it using the fixed native proposal schema."
                .to_owned(),
            false,
        ),
        Err(failure) => return Err(failure),
    };
    Ok((
        ModelMessage::Tool {
            call_id: call_id.clone(),
            text,
        },
        accepted,
    ))
}

/// The most model feedback one refused setup carries.
const REFUSAL_BYTES_MAX: usize = 1_024;

/// Proves a first setup with the daemon's own validator before submitting it.
///
/// The daemon refuses an invalid setup with a fixed sentence, which leaves the
/// model guessing; the same validator here names the field and rule to fix.
/// The daemon still validates: this only decides what the model hears.
pub(super) fn initial_refusal(proposal: &Proposal) -> Option<String> {
    let Proposal::Initialize { definition } = proposal else {
        return None;
    };
    let failure = Definition::parse(&definition.to_string())
        .and_then(|parsed| parsed.validate_initial_generation())
        .err()?;
    let mut text = format!(
        "No changes were applied. The setup is invalid: {failure}. Fix that and return a corrected propose_setup call."
    );
    if text.len() > REFUSAL_BYTES_MAX {
        let mut end = REFUSAL_BYTES_MAX;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    Some(text)
}

const fn feedback(reason: ProposalRejection) -> &'static str {
    match reason {
        ProposalRejection::InvalidArguments => {
            "No changes were applied. Correct the propose_setup arguments using the fixed schema. For initialize, tools, credentials, triggers, network.allow and gates.rules are arrays (empty is []); gates.timeout_ms is an integer and budget.daily_nanos is a decimal string. For edit, base_candidate_revision is a decimal string and instruction edits use kind=append or kind=replace, never byte offsets."
        }
        ProposalRejection::InstructionsMissing => {
            "No changes were applied. The old_text did not match the current instructions_md. Copy a unique exact substring, including whitespace, without adding bullet markers. To add instructions, use kind=append with only the new text. Return a corrected propose_setup call."
        }
        ProposalRejection::InstructionsAmbiguous => {
            "No changes were applied. The old_text matches more than one position in instructions_md. Include enough exact surrounding text to identify one position. Return a corrected propose_setup call."
        }
        ProposalRejection::EditNotRequested => {
            "No changes were applied. The newest message does not ask to change the setup. Answer the question without proposing an edit."
        }
        ProposalRejection::StaleCandidate => {
            "No changes were applied. Use the current candidate identifier and decimal revision from the current candidate or most recent accepted proposal receipt. Return a corrected propose_setup call."
        }
    }
}

#[cfg(test)]
#[path = "proposal_tests.rs"]
mod tests;
