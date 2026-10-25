//! The worker forwards saved context and respects the configured turn allowance.

use super::{ack, json, lease, result, rig, run};
use crate::{
    client::{Call, Verb},
    test_support::Answer,
};
use serde_json::Value;

// Value: protects=earlier assistant text and the current candidate reach the daemon in order;
// fails_when=the worker drops context or silently replaces the candidate;
// why_new=the original lease fixture contains only one user message and no candidate;
// seam=the existing control-plane transport fixture; no_claim=live model inference.
#[tokio::test]
async fn existing_candidate_and_assistant_context_reach_the_model_call() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => result(call, &json!([]), false),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let mut configured = lease();
    configured
        .messages
        .push(afd_wire::authoring::ContextMessage {
            role: afd_wire::authoring::ContextRole::Assistant,
            text: "Earlier private answer".into(),
        });
    configured.candidate = Some(json!({"instructions_md":"Investigate saved setup"}));
    configured
        .messages
        .push(afd_wire::authoring::ContextMessage {
            role: afd_wire::authoring::ContextRole::User,
            text: "Add guidance about accepting factual corrections".into(),
        });
    let (calls, report) = run(&mut rig, configured).await;
    let model = calls
        .iter()
        .find(|call| call.verb == Verb::AuthoringModel)
        .unwrap();
    let body: Value = serde_json::from_slice(model.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["request"]["supports_compaction"], true);
    let messages = body["request"]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(
        messages[1],
        json!({"role":"assistant","text":"Earlier private answer","proposal_calls":[]})
    );
    assert_eq!(
        messages[2],
        json!({"role":"user","text":"Current candidate (untrusted setup data): {\"instructions_md\":\"Investigate saved setup\"}"})
    );
    assert_eq!(
        messages[3],
        json!({"role":"user","text":"Add guidance about accepting factual corrections"})
    );
    assert_eq!(report["outcome"], "completed");
}

fn response(call: &Call, unknown: bool) -> Answer {
    let Answer::Reply(bytes) = result(
        call,
        &json!([{"id":"edit","proposal":{"kind":"initialize","definition":{}}}]),
        false,
    ) else {
        unreachable!()
    };
    let mut body: Value = serde_json::from_slice(&bytes).unwrap();
    body["usage"] = if unknown {
        json!({"knowledge":"unknown"})
    } else {
        json!({"knowledge":"known","input_tokens":3,"output_tokens":2,"provider_cost_nanos":"1"})
    };
    json(&body)
}

// Value: protects=unknown usage and exhausted output stop further physical calls;
// fails_when=the worker continues proposals with an uncertain or spent allowance;
// why_new=transport failures do not exercise a successful reply carrying unknown usage;
// seam=the existing control-plane transport fixture; no_claim=live provider billing.
#[tokio::test]
async fn unknown_or_exhausted_usage_prevents_another_model_call() {
    for unknown in [true, false] {
        let mut rig = rig(move |call| match call.verb {
            Verb::AuthoringModel => response(call, unknown),
            Verb::AuthoringActivity => json(&json!({"accepted":true,"replayed":false})),
            Verb::AuthoringReport => ack(),
            other => panic!("unexpected authority {other:?}"),
        });
        let mut configured = lease();
        configured.limits.output_per_turn = 2;
        let (calls, report) = run(&mut rig, configured).await;
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.verb == Verb::AuthoringModel)
                .count(),
            1
        );
        assert_eq!(
            calls
                .iter()
                .filter(|call| call.verb == Verb::AuthoringActivity)
                .count(),
            usize::from(!unknown)
        );
        if unknown {
            assert_eq!(report["usage"]["knowledge"], "unknown");
            assert!(report["answer"].is_null());
        } else {
            assert_eq!(report["usage"]["output_tokens"], 2);
            assert_eq!(report["outcome"], "completed");
            assert_eq!(report["usage"]["knowledge"], "known");
            assert_eq!(
                report["answer"],
                "Your setup is ready to review. Save it when you’re ready."
            );
        }
    }
}

// Value: protects=an invalid saved allowance cannot be truncated into an authorized call;
// fails_when=an allowance above the wire range wraps or silently admits a request;
// why_new=the larger normal allowance test remains within u32;
// seam=the existing control-plane transport fixture; no_claim=live invalid leases.
#[tokio::test]
async fn an_output_allowance_beyond_the_wire_range_never_calls_the_model() {
    for allowance in [-1, i64::from(u32::MAX) + 1] {
        let mut rig = rig(|call| match call.verb {
            Verb::AuthoringReport => ack(),
            other => panic!("invalid allowance requested authority {other:?}"),
        });
        let mut configured = lease();
        configured.limits.output_per_turn = allowance;
        let (calls, report) = run(&mut rig, configured).await;
        assert_eq!(calls.len(), 1);
        assert_eq!(report["usage"]["knowledge"], "unknown");
        assert!(report["answer"].is_null());
    }
}

// Value: compaction consumes a physical call and usage without replacing user context.
#[tokio::test]
async fn a_compaction_receipt_continues_the_same_turn_and_counts_both_calls() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            let Answer::Reply(bytes) = result(call, &json!([]), false) else {
                unreachable!()
            };
            let mut reply: Value = serde_json::from_slice(&bytes).unwrap();
            if request["call_index"] == 1 {
                reply["compacted"] = json!(true);
                reply["text"] = json!("");
            }
            json(&reply)
        }
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    let requests: Vec<Value> = calls
        .iter()
        .filter(|call| call.verb == Verb::AuthoringModel)
        .map(|call| serde_json::from_slice(call.body.as_ref().unwrap()).unwrap())
        .collect();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests[0]["request"]["messages"],
        requests[1]["request"]["messages"]
    );
    assert_eq!(requests[1]["request"]["supports_compaction"], true);
    assert_eq!(requests[1]["request"]["max_output_tokens"], 8_190);
    assert_ne!(requests[0]["dispatch_id"], requests[1]["dispatch_id"]);
    assert_eq!(requests[1]["call_index"], 2);
    assert_eq!(report["answer"], "Private result");
    assert_eq!(report["outcome"], "completed");
    assert_eq!(report["usage"]["input_tokens"], 6);
    assert_eq!(report["usage"]["output_tokens"], 4);
    assert_eq!(report["usage"]["provider_cost_nanos"], "2");
}
