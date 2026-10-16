//! Proposal proofs are grouped by replay, validation and authorization.

mod fencing;
mod replay;
mod validation;

use super::{
    attempt::Attempt,
    fixture::{Fixture, now},
    mint,
};
use afd_authoring::{ActivityInput, types::Source};
use afd_wire::authoring::{ActivityEvent, CandidateVersion, Proposal};
use serde_json::json;

fn edit(a: &Attempt, sequence: u32, base: &CandidateVersion, text: &str) -> ActivityInput {
    let mut request = a.initialize(sequence);
    request.event = ActivityEvent::Proposal {
        proposal: Proposal::Edit {
            base_candidate_id: base.id.clone(),
            base_candidate_revision: base.revision.clone(),
            config_ops: vec![],
            instruction_edits: vec![
                serde_json::from_value(json!({
                    "start_byte":0,"end_byte":0,"text":text
                }))
                .expect("native instruction edit"),
            ],
        },
    };
    request
}
