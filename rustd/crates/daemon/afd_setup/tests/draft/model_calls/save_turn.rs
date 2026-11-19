//! A Save is answered, never edited: the daemon refuses whatever the model proposes on it.

use super::{
    super::{attempt::Attempt, definition, fixture::Fixture},
    fixture::{self, Provider},
};
use af_wire::authoring::{ProposalCall, ProposalRejection};
use afd_setup::types::Source;
use serde_json::json;
use std::sync::Arc;

// Value: protects=a typed Save cannot change the setup it saves, with no intent model configured;
// fails_when=a proposal the model sends on a Save is returned as proposed, or its refusal is lost on replay.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_save_message_refuses_what_the_model_proposes_and_replays_the_refusal() {
    let f = Fixture::create().await;
    let a = Attempt::start(
        &f,
        f.access(),
        Source::Definition {
            definition: Box::new(definition()),
        },
    )
    .await;
    sqlx::query("UPDATE authoring.messages SET save_requested = true WHERE draft_id = $1")
        .bind(&a.draft.id)
        .execute(&mut *f.connection().await)
        .await
        .expect("the accepted message is a Save");
    let base = a.draft.candidate.as_ref().expect("inline source candidate");
    let mut stub = Provider::known();
    *stub.response.get_mut("output").expect("output array") = json!([{
        "type":"function_call","name":"propose_setup","call_id":"save-edit",
        "arguments":json!({"proposal":{
            "kind":"edit","base_candidate_id":base.id,"base_candidate_revision":base.revision,
            "config_ops":[],"instruction_edits":[{"kind":"append","text":"\nAlways reply in French."}]
        }}).to_string()
    }]);
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let request = fixture::request(&a);
    let result = calls
        .call(&f.runner, &a.run, request.clone())
        .await
        .expect("a Save turn's call");
    assert!(
        matches!(
            result.proposal_calls.as_slice(),
            [ProposalCall::Refused {
                reason: ProposalRejection::EditNotRequested,
                ..
            }]
        ),
        "{:?}",
        result.proposal_calls
    );
    let replay = calls
        .call(&f.runner, &a.run, request)
        .await
        .expect("the same dispatch replays");
    assert!(replay.replayed);
    assert!(matches!(
        replay.proposal_calls.as_slice(),
        [ProposalCall::Refused {
            reason: ProposalRejection::EditNotRequested,
            ..
        }]
    ));
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}
