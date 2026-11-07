//! The daemon resolves model text against its stored candidate, then uses native activity.

use super::{
    super::{attempt::Attempt, definition, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::{
    ModelCallResult,
    types::{Candidate, Source},
};
use afd_fleet_runtime::definition::{ConfigOp, Definition, InstructionEdit};
use afd_wire::authoring::{ActivityEvent, Proposal, ProposalCall};
use serde_json::json;
use std::sync::Arc;

type Native = Proposal<Definition, ConfigOp, InstructionEdit>;

pub(super) fn native(result: &ModelCallResult) -> &Native {
    result
        .proposal_calls
        .iter()
        .find_map(|call| match call {
            ProposalCall::Proposed { proposal, .. } => Some(proposal),
            ProposalCall::Refused { .. } => None,
        })
        .expect("one successful native proposal")
}

fn provider(base: &Candidate) -> Arc<Provider> {
    let mut stub = Provider::known();
    *stub.response.get_mut("output").expect("output array") = json!([{
        "type":"function_call","name":"propose_setup","call_id":"text-edit",
        "arguments":json!({"proposal":{
            "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
            "config_ops":[],"instruction_edits":[{"kind":"replace","old_text":"failures","text":"evidence"}]
        }}).to_string()
    }]);
    Arc::new(stub)
}

pub(super) async fn assert_changed(f: &Fixture, a: &Attempt) {
    let stored = f
        .drafts
        .read(f.access(), &a.draft.id)
        .await
        .expect("stored candidate");
    let changed = stored.candidate.as_ref().expect("new candidate");
    assert_eq!(changed.revision, "2");
    assert_eq!(
        changed.definition.instructions_md(),
        "Investigate evidence.\n"
    );
    assert_eq!(
        changed.definition.config(),
        a.draft
            .candidate
            .as_ref()
            .expect("source candidate")
            .definition
            .config()
    );
}

// Value: protects=the actual dispatch reserves a stored base, translates a model anchor,
// commits exactly that native edit and replays its receipt without another paid request;
// fails_when=the decoder has no stored candidate or trusts a model-supplied byte offset.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn model_text_edits_resolve_against_the_stored_revision_and_apply_atomically() {
    let f = Fixture::create().await;
    let a = Attempt::start(
        &f,
        f.access(),
        Source::Definition {
            definition: Box::new(definition()),
        },
    )
    .await;
    let base = a.draft.candidate.as_ref().expect("inline source candidate");
    let provider = provider(base);
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let request = fixture::request(&a);
    let result = calls
        .call(&f.runner, &a.lease, request.clone())
        .await
        .expect("anchored proposal");
    let mut activity = a.initialize(1);
    activity.event = ActivityEvent::Proposal {
        proposal: native(&result).clone(),
    };
    let accepted = f
        .drafts
        .activity(&f.runner, &a.lease, activity)
        .await
        .expect("native activity");
    assert_eq!(
        accepted
            .candidate
            .as_ref()
            .expect("accepted version")
            .revision,
        "2"
    );
    assert_changed(&f, &a).await;
    let replay = calls
        .call(&f.runner, &a.lease, request)
        .await
        .expect("same dispatch replay");
    assert!(replay.replayed);
    assert!(
        matches!(native(&replay), Proposal::Edit { instruction_edits, .. } if instruction_edits.len() == 1)
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &a).await, (1, 1, false));
    f.cleanup().await;
}
