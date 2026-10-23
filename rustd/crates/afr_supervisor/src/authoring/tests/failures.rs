//! Uncertain physical calls are never retried or described as zero-cost success.

use super::{ack, json, lease, result, rig, run};
use crate::{client::Verb, test_support::Answer};

#[tokio::test]
async fn an_unavailable_model_route_is_called_once_and_spools_unknown_usage() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => Answer::Fail(crate::error::unavailable(call.verb, 503)),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringModel)
            .count(),
        1
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_usage_unknown");
    assert_eq!(report["usage"]["knowledge"], "unknown");
    assert!(report["usage"].get("provider_cost_nanos").is_none());
    assert!(report.get("answer").is_none_or(serde_json::Value::is_null));
}

#[tokio::test]
async fn a_changed_dispatch_identity_cannot_publish_the_returned_private_text() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let Answer::Reply(bytes) = result(call, &json!([]), false) else {
                unreachable!()
            };
            let mut reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            reply["dispatch_id"] = super::ATTEMPT.into();
            super::json(&reply)
        }
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (_, report) = run(&mut rig, lease()).await;
    assert_eq!(report["usage"]["knowledge"], "unknown");
    assert!(report.get("answer").is_none_or(serde_json::Value::is_null));
}

/// A model reply carrying one proposal, as the daemon's model route answers.
fn proposing(call: &crate::client::Call) -> Answer {
    let request: serde_json::Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
    result(
        call,
        &json!([{"outcome":"proposed","id":format!("call-{}",request["call_index"]),
            "proposal":{"kind":"initialize","definition":{}}}]),
        false,
    )
}

/// Runs a turn whose proposal the daemon answers with `activity`, and hands
/// back how many proposals reached it and the report.
async fn proposed(activity: fn() -> Answer) -> (usize, serde_json::Value) {
    let mut rig = rig(move |call| match call.verb {
        Verb::AuthoringModel => proposing(call),
        Verb::AuthoringActivity => activity(),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    let proposals = calls
        .iter()
        .filter(|call| call.verb == Verb::AuthoringActivity)
        .count();
    (proposals, report)
}

#[tokio::test]
async fn a_proposal_answered_but_not_accepted_ends_the_turn_rather_than_repairing() {
    // A 2xx that says "not accepted" is a daemon this runner does not
    // understand: repairing would loop on an answer the model cannot change.
    let (proposals, report) = proposed(|| json(&json!({"accepted":false,"replayed":false}))).await;

    assert_eq!(proposals, 1, "the proposal is not offered again");
    assert_eq!(report["outcome"], "failed");
    assert!(report.get("answer").is_none_or(serde_json::Value::is_null));
}

#[tokio::test]
async fn a_proposal_refused_for_a_reason_the_model_cannot_fix_ends_the_turn() {
    // Only 400 and 422 are the proposal's own fault; anything else is the
    // daemon's, and telling the model to correct its schema would be a lie.
    let (proposals, report) =
        proposed(|| Answer::Fail(crate::error::refused(Verb::AuthoringActivity, 409, None))).await;

    assert_eq!(proposals, 1, "the proposal is not offered again");
    assert_eq!(report["outcome"], "failed");
    assert!(report.get("answer").is_none_or(serde_json::Value::is_null));
}
