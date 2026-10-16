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
