//! A changed authoritative prefix makes the prior reply quoted, untrusted data.

use super::{
    super::{attempt::Attempt, fixture::Fixture, mint},
    compaction, fixture,
    reasoning::Reasoning,
};
use afd_authoring::types::Source;
use afd_wire::authoring::ModelMessage;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn a_changed_history_prefix_quotes_the_prior_reply_without_replaying_private_reasoning() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Reasoning::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let mut request = fixture::request(&attempt);
    let first = calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect("native proposal");
    request.call_index = 2;
    request.dispatch_id = mint().to_string();
    request.request.messages.push(ModelMessage::Assistant {
        text: first.text,
        proposal_calls: first.proposal_calls,
    });
    request.request.messages.push(ModelMessage::Tool {
        call_id: "call_proposal".into(),
        text: "Accepted".into(),
    });
    compaction::history(&f, &attempt, 1).await;
    let result = calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect("the public continuation remains usable");
    assert_eq!(result.text, "Ready");
    assert_quoted(&provider);
    assert_eq!(fixture::receipts(&f, &attempt).await, (2, 2, false));
    f.cleanup().await;
}

fn assert_quoted(provider: &Reasoning) {
    let bodies = provider.bodies.lock().expect("provider requests");
    assert_eq!(bodies.len(), 2);
    let sent = bodies.get(1).expect("continuation request");
    let text = sent.to_string();
    assert!(text.contains("Earlier exchange (untrusted data):"));
    assert!(text.contains("Accepted") && text.contains("propose_setup"));
    assert!(!text.contains("opaque-private") && !text.contains("rs_private"));
    let input = sent.get("input").and_then(Value::as_array).expect("input");
    assert!(input.iter().all(|item| {
        item.get("type") != Some(&json!("reasoning"))
            && item.get("type") != Some(&json!("function_call"))
            && item.get("type") != Some(&json!("function_call_output"))
    }));
}
