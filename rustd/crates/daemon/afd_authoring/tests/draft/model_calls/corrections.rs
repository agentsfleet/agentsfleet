//! Correctable model arguments retain native continuation and authoritative receipts.

use super::{
    super::{attempt::Attempt, definition, fixture::Fixture, mint},
    fixture, text_edits,
};
use afd_authoring::{
    ModelCallResult, ProviderRequest, ProviderResponse, Transport,
    types::{Candidate, Source},
};
use afd_wire::authoring::{
    ActivityEvent, ModelMessage, ModelUsage, ProposalCall, ProposalRejection,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct Repairing {
    base: Candidate,
    bodies: Mutex<Vec<Value>>,
}

#[async_trait::async_trait]
impl Transport for Repairing {
    async fn send(&self, request: ProviderRequest) -> afd_authoring::Result<ProviderResponse> {
        let mut bodies = self.bodies.lock().expect("sent requests");
        bodies.push(serde_json::from_slice(request.body()).expect("provider request"));
        let anchor = if bodies.len() == 1 {
            "- failures"
        } else {
            "failures"
        };
        let (headers, body) = crate::provider_wire::body(200, serde_json::to_vec(&json!({
            "status":"completed","output":[
                {"type":"reasoning","id":"rs_correction","summary":[],
                    "encrypted_content":"opaque-correction-reasoning"},
                {"type":"function_call","id":"fc_correction",
                    "call_id":format!("correction-{}", bodies.len()),"name":"propose_setup",
                    "arguments":json!({"proposal":{
                        "kind":"edit","base_candidate_id":self.base.id,
                        "base_candidate_revision":self.base.revision,"config_ops":[],
                        "instruction_edits":[{"kind":"replace","old_text":anchor,"text":"evidence"}]
                    }}).to_string()}
            ],"usage":{"input_tokens":1,"output_tokens":1}
        })).expect("provider response"));
        Ok(ProviderResponse {
            status: 200,
            headers,
            body,
        })
    }
}

// Value: protects=bad anchors are replayable refusals, and correction retains original
// reasoning and tool identity before exactly one native edit; seam=physical provider.
#[tokio::test]
#[ignore = "requires live Postgres"]
async fn invalid_anchors_can_be_corrected_without_losing_continuation_or_mutating_early() {
    let f = Fixture::create().await;
    let a = Attempt::start(
        &f,
        f.access(),
        Source::Definition {
            definition: Box::new(definition()),
        },
    )
    .await;
    let provider = Arc::new(Repairing {
        base: a.draft.candidate.as_ref().expect("stored base").clone(),
        bodies: Mutex::new(Vec::new()),
    });
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let mut request = fixture::request(&a);
    let refused = calls
        .call(&f.runner, &a.lease, request.clone())
        .await
        .expect("bounded refusal");
    assert_refused(&refused);
    assert_eq!(a.activity_count(&f).await, 0);
    assert_unchanged(&f, &a).await;
    let replay = calls
        .call(&f.runner, &a.lease, request.clone())
        .await
        .expect("refusal replay");
    assert!(replay.replayed);
    assert_refused(&replay);
    assert_eq!(
        provider
            .bodies
            .lock()
            .expect("recorded provider calls")
            .len(),
        1
    );
    request.call_index = 2;
    request.dispatch_id = mint().to_string();
    request.request.messages.push(ModelMessage::Assistant {
        text: refused.text,
        proposal_calls: refused.proposal_calls,
    });
    request.request.messages.push(ModelMessage::Tool {
        call_id: "correction-1".into(),
        text: "No changes applied. Copy an exact unique match from instructions_md.".into(),
    });
    let corrected = calls
        .call(&f.runner, &a.lease, request)
        .await
        .expect("corrected proposal");
    assert_continuation(&provider);
    apply_once(&f, &a, &corrected).await;
    text_edits::assert_changed(&f, &a).await;
    assert_eq!(fixture::receipts(&f, &a).await, (2, 2, false));
    f.cleanup().await;
}

fn assert_refused(result: &ModelCallResult) {
    assert!(
        matches!(result.proposal_calls.first(), Some(ProposalCall::Refused {
        id, reason: ProposalRejection::InstructionsMissing,
    }) if id == "correction-1")
    );
    assert!(matches!(
        result.usage,
        ModelUsage::Known {
            input_tokens: 1,
            output_tokens: 1,
            ..
        }
    ));
    let public = serde_json::to_string(result).expect("bounded result");
    assert!(!public.contains("opaque-correction") && !public.contains("- failures"));
}

async fn assert_unchanged(f: &Fixture, a: &Attempt) {
    let stored = f
        .drafts
        .read(f.access(), &a.draft.id)
        .await
        .expect("read after refusal");
    let candidate = stored.candidate.expect("original candidate");
    assert_eq!(candidate.revision, "1");
    assert_eq!(
        candidate.definition.instructions_md(),
        "Investigate failures.\n"
    );
}

async fn apply_once(f: &Fixture, a: &Attempt, result: &ModelCallResult) {
    let mut activity = a.initialize(1);
    activity.event = ActivityEvent::Proposal {
        proposal: text_edits::native(result).clone(),
    };
    let accepted = f
        .drafts
        .activity(&f.runner, &a.lease, activity.clone())
        .await
        .expect("corrected native edit");
    assert!(!accepted.replayed);
    let replay = f
        .drafts
        .activity(&f.runner, &a.lease, activity)
        .await
        .expect("activity replay");
    assert!(replay.replayed);
    assert_eq!(a.activity_count(f).await, 1);
}

fn assert_continuation(provider: &Repairing) {
    let bodies = provider.bodies.lock().expect("provider requests");
    assert_eq!(bodies.len(), 2);
    let input = bodies
        .get(1)
        .and_then(|body| body.get("input"))
        .and_then(Value::as_array)
        .expect("continued input");
    assert!(
        input.iter().any(
            |item| item.get("encrypted_content") == Some(&json!("opaque-correction-reasoning"))
        )
    );
    let original = input
        .iter()
        .find(|item| item.get("type") == Some(&json!("function_call")))
        .expect("original tool call");
    let arguments: Value = serde_json::from_str(
        original
            .get("arguments")
            .and_then(Value::as_str)
            .expect("original arguments"),
    )
    .expect("tool JSON");
    assert_eq!(
        arguments.pointer("/proposal/instruction_edits/0/old_text"),
        Some(&json!("- failures"))
    );
    let feedback = input
        .iter()
        .find(|item| item.get("type") == Some(&json!("function_call_output")))
        .expect("correction feedback");
    assert_eq!(feedback.get("call_id"), Some(&json!("correction-1")));
    assert!(
        feedback
            .get("output")
            .and_then(Value::as_str)
            .is_some_and(|text| text.contains("exact unique match"))
    );
}
