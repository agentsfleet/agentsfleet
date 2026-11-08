//! Invalid model edits receive feedback without acquiring proposal authority.

use super::{ack, json, lease, result, rig, run};
use crate::client::{Call, Verb};
use serde_json::Value;
use std::sync::atomic::Ordering;

fn rejected(call: &Call, reason: &str) -> crate::test_support::Answer {
    let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    result(
        call,
        &json!([{"id":format!("bad-{}",request["call_index"]),
            "reason":reason}]),
        false,
    )
}

fn correcting(call: &Call, reason: &str, feedback_hint: &str) -> crate::test_support::Answer {
    let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    match request["call_index"].as_u64().unwrap() {
        1 => rejected(call, reason),
        2 => {
            let feedback = request["request"]["messages"]
                .as_array()
                .unwrap()
                .last()
                .unwrap();
            assert_eq!(feedback["role"], "tool");
            assert_eq!(feedback["call_id"], "bad-1");
            assert!(feedback["text"].as_str().unwrap().contains(feedback_hint));
            result(
                call,
                &json!([{"id":"corrected",
                    "proposal":{"kind":"initialize","definition":{}}}]),
                false,
            )
        }
        index => panic!("unexpected model call {index}"),
    }
}

// Value: protects=each refused edit receives actionable feedback and can be corrected;
// fails_when=a refusal aborts chat, grants authority or consumes an activity number.
#[tokio::test]
async fn rejected_model_edits_receive_feedback_before_a_corrected_proposal_is_applied() {
    for (reason, feedback_hint) in [
        ("invalid_arguments", "fixed schema"),
        ("instructions_missing", "did not match"),
        ("instructions_ambiguous", "more than one position"),
        ("stale_candidate", "current candidate"),
    ] {
        let mut rig = rig(move |call| match call.verb {
            Verb::AuthoringModel => correcting(call, reason, feedback_hint),
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
        assert_eq!(report["answer"], "Private result");
        assert_eq!(report["usage"]["calls"], 2);
        assert_eq!(report["usage"]["provider_cost_nanos"], "2");
        assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
        assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    }
}

// Value: protects=correction attempts retain usage and stop at the admitted limit;
// fails_when=rejections are retried forever or reported as a completed answer.
#[tokio::test]
async fn exhausted_corrections_fail_with_known_usage_and_no_activity_or_answer() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => rejected(call, "instructions_missing"),
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

// Value: protects=corrections cannot spend beyond the admitted output allowance;
// fails_when=an exhausted turn makes another model call or publishes success.
#[tokio::test]
async fn should_stop_corrections_when_the_output_budget_is_spent() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => rejected(call, "instructions_missing"),
        Verb::AuthoringReport => ack(),
        other => panic!("a rejected edit acquired authority {other:?}"),
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
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_failed");
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 1);
    assert_eq!(report["usage"]["output_tokens"], 2);
    assert_eq!(report["usage"]["provider_cost_nanos"], "1");
    assert!(report.get("answer").is_none_or(Value::is_null));
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}
