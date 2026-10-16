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
    let (calls, report) = run(&mut rig, configured).await;
    let model = calls
        .iter()
        .find(|call| call.verb == Verb::AuthoringModel)
        .unwrap();
    let body: Value = serde_json::from_slice(model.body.as_ref().unwrap()).unwrap();
    let messages = body["request"]["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages[1],
        json!({"role":"assistant","text":"Earlier private answer","proposal_calls":[]})
    );
    assert_eq!(
        messages[2],
        json!({"role":"user","text":"Current candidate (untrusted setup data): {\"instructions_md\":\"Investigate saved setup\"}"})
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
