//! Authoring proposals never acquire fleet tools, memory or sandbox authority.

use super::{ack, json, lease, result, rig, run};
use crate::client::Verb;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn saved_output_allowances_above_eight_thousand_reach_the_daemon() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => result(call, &json!([]), false),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let mut configured = lease();
    configured.limits.output_per_turn = 32_768;
    let (calls, report) = run(&mut rig, configured).await;
    let model = calls
        .iter()
        .find(|call| call.verb == Verb::AuthoringModel)
        .unwrap();
    let request: serde_json::Value = serde_json::from_slice(model.body.as_ref().unwrap()).unwrap();
    assert_eq!(request["request"]["max_output_tokens"], 32_768);
    assert_eq!(report["usage"]["calls"], 1);
}

#[tokio::test]
async fn a_plain_answer_uses_only_the_canonical_scoped_route_and_private_report() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => result(call, &json!([]), false),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].path,
        format!(
            "/v1/runners/me/authoring-leases/{}/model-calls",
            super::LEASE
        )
    );
    assert_eq!(report["answer"], "Private result");
    assert_eq!(report["usage"]["calls"], 1);
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
    // The drainer runs beside this check and removes an entry once its report
    // settles, so an entry can vanish between the listing and the read. A
    // removed entry holds no secret; every entry still on disk is checked.
    let saved = std::fs::read_dir(rig.home.spool().join("authoring")).unwrap();
    for entry in saved.flatten().filter(|entry| entry.path().is_dir()) {
        let accounting = match std::fs::read_to_string(entry.path().join("accounting.json")) {
            Ok(accounting) => accounting,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => panic!("a saved entry could not be read: {error}"),
        };
        assert!(!accounting.contains(&"a".repeat(43)));
    }
}

#[tokio::test]
async fn cut_output_never_submits_a_proposal_even_if_a_reply_contains_one() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => result(
            call,
            &json!([{"id":"cut-call",
            "proposal":{"kind":"initialize","definition":{}}}]),
            true,
        ),
        Verb::AuthoringReport => ack(),
        other => panic!("cut output requested authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(calls.len(), 2);
    assert_eq!(report["outcome"], "completed");
    assert_eq!(report["usage"]["output_tokens"], 2);
}

#[tokio::test]
async fn a_refused_proposal_is_repaired_without_skipping_its_activity_sequence() {
    let activities = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorded = std::sync::Arc::clone(&activities);
    let mut rig = rig(move |call| match call.verb {
        Verb::AuthoringModel => {
            let request: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            let proposals = if request["call_index"] == 3 {
                json!([])
            } else {
                json!([{"id":format!("call-{}",request["call_index"]),
                    "proposal":{"kind":"initialize","definition":{}}}])
            };
            result(call, &proposals, false)
        }
        Verb::AuthoringActivity => {
            let request: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            let mut activities = recorded.lock().unwrap();
            activities.push(request);
            if activities.len() == 1 {
                crate::test_support::Answer::Fail(crate::error::refused(
                    Verb::AuthoringActivity,
                    422,
                    None,
                ))
            } else {
                super::json(&json!({"accepted":true,"replayed":false}))
            }
        }
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (_, report) = run(&mut rig, lease()).await;
    let activities = activities.lock().unwrap();
    assert_eq!(activities.len(), 2);
    assert!(
        activities
            .iter()
            .all(|request| request["activity_sequence"] == "1")
    );
    assert_eq!(report["usage"]["calls"], 3);
    assert_eq!(report["usage"]["provider_cost_nanos"], "3");
}
