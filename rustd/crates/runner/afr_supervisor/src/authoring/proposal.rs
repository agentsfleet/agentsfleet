//! The fixed proposal tool commits through the daemon's native validator.

use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_fleet_runtime::setup::{self, Rejection};
use af_wire::authoring::{
    ActivityEvent, ActivityRequest, ModelMessage, Proposal, ProposalCall, ProposalRejection,
    RefusalDetail, Run,
};
use serde_json::Value;

pub(super) async fn apply(
    plane: &ControlPlane,
    id: &Uuid7,
    run: &Run<Value>,
    sequence: u64,
    call: &ProposalCall,
) -> Result<(ModelMessage, bool)> {
    let (call_id, proposal) = match call {
        ProposalCall::Proposed { id, proposal } => (id, proposal),
        ProposalCall::Refused { id, reason, detail } => {
            let refusal = rejection(*reason).kind();
            // The field is the checker's own spelling; its sentence may name
            // authored text, so only the model hears that.
            let field = detail.as_ref().and_then(RefusalDetail::field);
            tracing::info!(
                event = "setup_proposal_outcome",
                sequence,
                outcome = "refused",
                refusal,
                field
            );
            let text = detail
                .as_ref()
                .map_or_else(|| rejection(*reason).feedback().to_owned(), invalid_setup);
            return Ok((
                ModelMessage::Tool {
                    call_id: id.clone(),
                    text,
                },
                false,
            ));
        }
    };
    if let Some(reply) = initial_reply(proposal, call_id, sequence) {
        return Ok((reply, false));
    }
    let (text, accepted) = submit(plane, id, run, sequence, proposal).await?;
    Ok((
        ModelMessage::Tool {
            call_id: call_id.clone(),
            text,
        },
        accepted,
    ))
}

fn initial_reply(proposal: &Proposal, call_id: &str, sequence: u64) -> Option<ModelMessage> {
    let detail = initial_refusal(proposal)?;
    tracing::info!(
        event = "setup_proposal_outcome",
        sequence,
        outcome = "refused",
        refusal = "invalid_first_setup",
        field = detail.field()
    );
    Some(ModelMessage::Tool {
        call_id: call_id.to_owned(),
        text: invalid_setup(&detail),
    })
}

async fn submit(
    plane: &ControlPlane,
    id: &Uuid7,
    run: &Run<Value>,
    sequence: u64,
    proposal: &Proposal,
) -> Result<(String, bool)> {
    let request = request(run, sequence, proposal);
    let receipt = plane
        .retrying(|| plane.authoring_activity(id, &request))
        .await;
    Ok(match receipt {
        Ok(receipt) if receipt.accepted => {
            tracing::info!(
                event = "setup_proposal_outcome",
                sequence,
                outcome = "accepted"
            );
            (
                serde_json::to_string(&receipt).map_err(error::encode)?,
                true,
            )
        }
        Ok(_) => return Err(error::config("the daemon did not accept the proposal")),
        Err(failure) if failure.refusal_code() == Some(af_core::error_code::INVALID_REQUEST) => {
            tracing::info!(
                event = "setup_proposal_outcome",
                sequence,
                outcome = "refused",
                refusal = "daemon_invalid"
            );
            (
                "The proposal is invalid. Correct it using the fixed native proposal schema."
                    .to_owned(),
                false,
            )
        }
        Err(failure) => return Err(failure),
    })
}

fn request(run: &Run<Value>, sequence: u64, proposal: &Proposal) -> ActivityRequest {
    ActivityRequest {
        fence: run.fence.clone(),
        sequence: run.sequence.clone(),
        activity_sequence: sequence.to_string(),
        event: ActivityEvent::Proposal {
            proposal: proposal.clone(),
        },
    }
}

/// Proves a first setup with the daemon's own validator before submitting it.
///
/// The daemon refuses an invalid setup with a fixed sentence, which leaves the
/// model guessing; the same validator here names the field and rule to fix.
/// The daemon still validates: this only decides what the model hears.
pub(super) fn initial_refusal(proposal: &Proposal) -> Option<RefusalDetail> {
    let Proposal::Initialize { definition } = proposal else {
        return None;
    };
    let failure = setup::initial_refusal(&definition.to_string())?;
    RefusalDetail::new(failure.field(), &failure.told())
}

/// What the model hears when the checker named the field to fix.
fn invalid_setup(detail: &RefusalDetail) -> String {
    setup::invalid_setup(detail.text())
}

/// The shared refusal a wire reason names.
const fn rejection(reason: ProposalRejection) -> Rejection {
    match reason {
        ProposalRejection::InvalidArguments => Rejection::InvalidArguments,
        ProposalRejection::InstructionsMissing => Rejection::InstructionsMissing,
        ProposalRejection::InstructionsAmbiguous => Rejection::InstructionsAmbiguous,
        ProposalRejection::EditNotRequested => Rejection::EditNotRequested,
        ProposalRejection::StaleCandidate => Rejection::StaleCandidate,
    }
}

#[cfg(test)]
#[path = "proposal_tests.rs"]
mod tests;
