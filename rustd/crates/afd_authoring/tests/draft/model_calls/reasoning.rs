//! Native reasoning stays with its authorized dispatch across a tool round trip.

use super::{
    super::{attempt::Attempt, definition, fixture::Fixture, mint},
    fixture,
};
use afd_authoring::{ProviderRequest, ProviderResponse, Transport, types::Source};
use afd_wire::authoring::ModelMessage;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub(super) struct Reasoning {
    pub(super) bodies: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl Transport for Reasoning {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let mut bodies = self.bodies.lock().expect("record provider request");
        let body: Value = serde_json::from_slice(request.body()).expect("provider JSON");
        bodies.push(body);
        let output = if bodies.len() == 1 {
            let proposal = json!({"kind":"initialize","definition":definition()});
            json!([
                {"type":"reasoning","id":"rs_private","summary":[],"encrypted_content":"opaque-private-reasoning"},
                {"type":"function_call","id":"fc_private","call_id":"call_proposal","name":"propose_setup",
                 "arguments":json!({"proposal_json":proposal.to_string()}).to_string()}
            ])
        } else {
            json!([{"type":"message","content":[{"type":"output_text","text":"Ready"}]}])
        };
        Ok(ProviderResponse {
            status: 200,
            body: serde_json::to_vec(&json!({"status":"completed","output":output,
                "usage":{"input_tokens":1,"output_tokens":1}}))
            .expect("response JSON"),
        })
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_tool_continuation_replays_native_reasoning_without_exposing_it_to_the_runner() {
    let f = Fixture::create().await;
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(Reasoning::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let first_request = fixture::request(&attempt);
    let first = calls
        .call(&f.runner, &attempt.lease, first_request.clone())
        .await
        .expect("proposal");
    let public = serde_json::to_string(&first).expect("runner result");
    assert!(!public.contains("opaque-private") && !public.contains("rs_private"));
    let mut next = first_request;
    next.call_index = 2;
    next.dispatch_id = mint().to_string();
    next.request.messages.push(ModelMessage::Assistant {
        text: first.text,
        proposal_calls: first.proposal_calls,
    });
    next.request.messages.push(ModelMessage::Tool {
        call_id: "call_proposal".into(),
        text: "Accepted".into(),
    });
    let second = calls
        .call(&f.runner, &attempt.lease, next)
        .await
        .expect("tool continuation");
    assert_eq!(second.text, "Ready");
    assert_native_replay(&provider);
    f.drafts
        .delete(f.access(), &attempt.draft.id)
        .await
        .expect("private deletion");
    let retained: i64 = sqlx::query_scalar("SELECT count(*) FROM authoring.dispatches WHERE attempt_id=$1::uuid AND native_reply IS NOT NULL")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("purge evidence");
    assert_eq!(retained, 0);
    f.cleanup().await;
}

fn assert_native_replay(provider: &Reasoning) {
    let bodies = provider.bodies.lock().expect("sent requests");
    assert_eq!(bodies.len(), 2);
    for body in bodies.iter() {
        super::preamble::assert_instructions(body, afd_wire::inference::InferenceApi::Responses);
        assert_eq!(body.pointer("/input/0/role"), Some(&json!("user")));
        assert_eq!(
            body.pointer("/input/0/content/0/text"),
            Some(&json!("Investigate the checkout"))
        );
        assert_eq!(body.pointer("/tools/0/type"), Some(&json!("function")));
        assert_eq!(body.pointer("/tools/0/name"), Some(&json!("propose_setup")));
    }
    let items = bodies
        .get(1)
        .and_then(|body| body.get("input"))
        .and_then(Value::as_array)
        .expect("SDK conversation");
    let reasoning = items
        .iter()
        .position(|item| item.get("type") == Some(&json!("reasoning")))
        .expect("reasoning retained");
    assert_eq!(
        items
            .get(reasoning)
            .and_then(|item| item.get("encrypted_content")),
        Some(&json!("opaque-private-reasoning"))
    );
    assert_eq!(
        items.get(reasoning + 1).and_then(|item| item.get("id")),
        Some(&json!("fc_private"))
    );
    assert_eq!(
        items.get(reasoning + 2).and_then(|item| item.get("type")),
        Some(&json!("function_call_output"))
    );
}
