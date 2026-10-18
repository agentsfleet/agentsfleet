//! Untrusted output retains billing while refusing executable or oversized content.

use super::{
    super::{attempt::Attempt, definition, fixture::Fixture},
    fixture::{self, Provider},
};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{ENVELOPE_BYTES_MAX, ModelMessage, Proposal, ProposalCall};
use serde_json::{Value, json};
use std::sync::Arc;

const NATIVE_ENVELOPE_HEADROOM: usize = 1024;

fn proposal(id: &str) -> Value {
    let proposal = json!({"kind":"initialize","definition":definition()});
    json!({"type":"function_call","name":"propose_setup","call_id":id,
        "arguments":json!({"proposal_json":proposal.to_string()}).to_string()})
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn unsupported_output_call_bounds_and_native_overflow_keep_known_usage() {
    for output in [
        json!([{"type":"web_search_call","id":"unexpected","status":"completed"}]),
        json!([{"type":"function_call","name":"grant_access","call_id":"wrong-tool","arguments":"{}"}]),
        json!([{"type":"function_call","name":"propose_setup","call_id":"bad-arguments","arguments":"incomplete JSON"}]),
        json!([proposal(&"x".repeat(129))]),
        json!([{"type":"message","content":[{"type":"output_text","text":"x".repeat(ENVELOPE_BYTES_MAX - 190)}]}]),
        json!([
            {"type":"reasoning","id":"rs_large","summary":[],"encrypted_content":"x".repeat(ENVELOPE_BYTES_MAX - NATIVE_ENVELOPE_HEADROOM)},
            proposal("native-large")
        ]),
    ] {
        let f = Fixture::create().await;
        let a = Attempt::start(&f, f.access(), Source::Blank).await;
        let mut stub = Provider::known();
        *stub.response.get_mut("output").expect("output array") = output;
        assert!(
            serde_json::to_vec(&stub.response)
                .expect("provider response")
                .len()
                <= ENVELOPE_BYTES_MAX
        );
        let provider = Arc::new(stub);
        let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
        let request = fixture::request(&a);
        for request in [request.clone(), request] {
            let error = calls
                .call(&f.runner, &a.lease, request)
                .await
                .expect_err("untrusted output cannot execute");
            assert_eq!(error.code(), error_code::AUTHORING_PROVIDER);
        }
        assert_eq!(provider.count(), 1);
        assert_eq!(fixture::receipts(&f, &a).await, (1, 1, false));
        let receipt: (bool, String) = sqlx::query_as("SELECT result IS NULL AND native_reply IS NULL,usage->>'knowledge' FROM authoring.dispatches WHERE attempt_id=$1::uuid")
            .bind(a.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("retained accounting");
        assert_eq!(receipt, (true, "known".into()));
        f.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_provider_cannot_reuse_a_call_identity_already_in_the_conversation() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = Provider::known();
    *stub.response.get_mut("output").expect("output array") = json!([proposal("prior-call")]);
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let mut request = fixture::request(&a);
    request.request.messages.extend([
        ModelMessage::Assistant {
            text: "Prior proposal".into(),
            proposal_calls: vec![ProposalCall {
                id: "prior-call".into(),
                proposal: Proposal::Initialize {
                    definition: Box::new(definition()),
                },
            }],
        },
        ModelMessage::Tool {
            call_id: "prior-call".into(),
            text: "Accepted".into(),
        },
    ]);
    assert_eq!(
        calls
            .call(&f.runner, &a.lease, request)
            .await
            .expect_err("reused proposal identity")
            .code(),
        error_code::AUTHORING_PROVIDER
    );
    assert_eq!(provider.count(), 1);
    assert_eq!(fixture::receipts(&f, &a).await, (1, 1, false));
    f.cleanup().await;
}
