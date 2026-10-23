//! Invalid model edits receive feedback without acquiring proposal authority.

use super::{ack, json, lease, result, rig, run};
use crate::client::{Call, Verb};
use serde_json::Value;
use std::sync::atomic::Ordering;

fn rejected(call: &Call) -> crate::test_support::Answer {
    let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    result(
        call,
        &json!([{"outcome":"refused","id":format!("bad-{}",request["call_index"]),
            "reason":"instructions_missing"}]),
        false,
    )
}

// Value: protects=an invalid anchor is corrected in the same bounded turn;
// fails_when=the daemon's refusal aborts chat or consumes an activity number.
#[tokio::test]
async fn rejected_model_edits_receive_feedback_before_a_corrected_proposal_is_applied() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            match request["call_index"].as_u64().unwrap() {
                1 => rejected(call),
                2 => {
                    let feedback = request["request"]["messages"]
                        .as_array()
                        .unwrap()
                        .last()
                        .unwrap();
                    assert_eq!(feedback["role"], "tool");
                    assert_eq!(feedback["call_id"], "bad-1");
                    assert!(feedback["text"].as_str().unwrap().contains("match"));
                    result(
                        call,
                        &json!([{"outcome":"proposed","id":"corrected",
                        "proposal":{"kind":"initialize","definition":{}}}]),
                        false,
                    )
                }
                index => panic!("unexpected model call {index}"),
            }
        }
        Verb::AuthoringActivity => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            assert_eq!(request["activity_sequence"], "1");
            assert_eq!(request["event"]["proposal"]["kind"], "initialize");
            super::json(&json!({"accepted":true,"replayed":false}))
        }
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringActivity)
            .count(),
        1
    );
    assert_eq!(report["outcome"], "completed");
    assert_eq!(
        report["answer"],
        "Your setup is ready to review. Save it when you’re ready."
    );
    assert_eq!(report["usage"]["calls"], 2);
    assert_eq!(report["usage"]["provider_cost_nanos"], "2");
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}

// Value: protects=correction attempts retain usage and stop at the admitted limit;
// fails_when=rejections are retried forever or reported as a completed answer.
#[tokio::test]
async fn exhausted_corrections_fail_with_known_usage_and_no_activity_or_answer() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => rejected(call),
        Verb::AuthoringReport => ack(),
        other => panic!("a rejected edit acquired authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringModel)
            .count(),
        8
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_failed");
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 8);
    assert_eq!(report["usage"]["provider_cost_nanos"], "8");
    assert!(report.get("answer").is_none_or(Value::is_null));
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}
