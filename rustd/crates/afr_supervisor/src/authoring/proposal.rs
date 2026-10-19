//! The fixed proposal tool commits through the daemon's native validator.

use crate::{
    Result,
    client::{ControlPlane, retrying},
    error,
};
use afd_core::id::Uuid7;
use afd_wire::authoring::{ActivityEvent, ActivityRequest, Lease, ModelMessage, ProposalCall};
use serde_json::Value;

pub(super) async fn apply(
    plane: &ControlPlane,
    id: &Uuid7,
    lease: &Lease<Value>,
    sequence: u64,
    call: &ProposalCall,
) -> Result<(ModelMessage, bool)> {
    let request = ActivityRequest {
        fence: lease.fence.clone(),
        sequence: lease.sequence.clone(),
        activity_sequence: sequence.to_string(),
        event: ActivityEvent::Proposal {
            proposal: call.proposal.clone(),
        },
    };
    let receipt = retrying(|| plane.authoring_activity(id, &request)).await;
    let (text, accepted) = match receipt {
        Ok(receipt) if receipt.accepted => (
            serde_json::to_string(&receipt).map_err(error::encode)?,
            true,
        ),
        Ok(_) => return Err(error::config("the daemon did not accept the proposal")),
        Err(failure) if matches!(failure.refusal_status(), Some(400 | 422)) => (
            "The proposal is invalid. Correct it using the fixed native proposal schema."
                .to_owned(),
            false,
        ),
        Err(failure) => return Err(failure),
    };
    Ok((
        ModelMessage::Tool {
            call_id: call.id.clone(),
            text,
        },
        accepted,
    ))
}
