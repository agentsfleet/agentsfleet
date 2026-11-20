//! One `propose_setup` call, read by the shared setup reader and spelled in the
//! attempt protocol's terms.
//!
//! `af_fleet_runtime::setup` decides what the call means: the settled
//! arguments, the resolved anchors and the base the edit must name. This file
//! only carries its verdict into the attempt protocol's `ProposalCall`.

use af_fleet_runtime::{
    Definition,
    definition::{ConfigOp, InstructionEdit},
    setup::{self, Head, Rejection},
};
use af_wire::authoring::{Proposal, ProposalCall, ProposalRejection, RefusalDetail};
use garde::Validate as _;
use rig_core::message::ToolCall;

use super::tools::TOOL;
use crate::{Result, error::ErrorKind, types::Candidate};

/// The model's call as the attempt protocol carries it: a proposal resolved
/// against `candidate`, or a refusal the model may correct.
pub(super) fn proposal(
    item: &ToolCall,
    candidate: Option<&Candidate>,
    said: Option<&str>,
) -> Result<ProposalCall<Definition, ConfigOp, InstructionEdit>> {
    if item.function.name.as_str() != TOOL {
        return Err(ErrorKind::Provider.into());
    }
    // A locally generated SDK id is not a provider acknowledgement.
    let id = item.id.provider().ok_or(ErrorKind::Provider)?.as_str();
    let arguments = serde_json::Value::Object(item.function.arguments.clone());
    let head = candidate.map(|base| Head {
        id: base.id.as_str(),
        revision: &base.revision,
        definition: &base.definition,
    });
    let read = if item.function.invalid_arguments.is_some() {
        Err(setup::Refused::new(
            Rejection::InvalidArguments,
            &setup::settle(arguments),
        ))
    } else {
        setup::read(arguments, head.as_ref(), said)
    };
    let call = match read {
        Ok(proposal) => ProposalCall::Proposed {
            id: id.into(),
            proposal: wire(proposal),
        },
        Err(refused) => ProposalCall::Refused {
            id: id.into(),
            reason: rejection(refused.reason),
            detail: refused
                .detail
                .and_then(|failure| RefusalDetail::new(failure.field(), &failure.told())),
        },
    };
    call.validate().map_err(|_untrusted| ErrorKind::Provider)?;
    Ok(call)
}

/// A resolved proposal in the attempt protocol's spelling.
fn wire(proposal: setup::Native) -> Proposal<Definition, ConfigOp, InstructionEdit> {
    match proposal {
        setup::Proposal::Initialize { definition } => Proposal::Initialize { definition },
        setup::Proposal::Edit {
            base_candidate_id,
            base_candidate_revision,
            config_ops,
            instruction_edits,
        } => Proposal::Edit {
            base_candidate_id,
            base_candidate_revision,
            config_ops,
            instruction_edits,
        },
    }
}

/// A refusal in the attempt protocol's closed vocabulary.
const fn rejection(reason: Rejection) -> ProposalRejection {
    match reason {
        Rejection::InvalidArguments => ProposalRejection::InvalidArguments,
        Rejection::InstructionsMissing => ProposalRejection::InstructionsMissing,
        Rejection::InstructionsAmbiguous => ProposalRejection::InstructionsAmbiguous,
        Rejection::StaleCandidate => ProposalRejection::StaleCandidate,
        Rejection::EditNotRequested => ProposalRejection::EditNotRequested,
    }
}
