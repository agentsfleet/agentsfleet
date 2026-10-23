//! An accepted native receipt is sufficient to finish an edit.

use super::{ack, json, lease, result, rig, run};
use crate::client::Verb;

// Value: protects=accepted setup edits finish without a second provider call;
// fails_when=confirmation replays large private reasoning or guesses whether an edit landed.
#[tokio::test]
async fn an_accepted_edit_finishes_from_its_receipt_without_another_model_call() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            assert_eq!(request["call_index"], 1, "no confirmation dispatch");
            result(
                call,
                &json!([{"id":"edit",
                "proposal":{"kind":"initialize","definition":{}}}]),
                false,
            )
        }
        Verb::AuthoringActivity => json(&json!({"accepted":true,"replayed":false})),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.verb == Verb::AuthoringModel)
            .count(),
        1
    );
    assert_eq!(report["outcome"], "completed");
    assert_eq!(
        report["answer"],
        "Your setup is ready to review. Save it when you’re ready."
    );
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 1);
    assert_eq!(report["usage"]["provider_cost_nanos"], "1");
}
