//! Accepted edits retain the model's conversational answer.

use super::{ack, json, lease, result, rig, run};
use crate::client::Verb;

fn tool_only(call: &crate::client::Call) -> crate::test_support::Answer {
    let crate::test_support::Answer::Reply(bytes) = result(
        call,
        &json!([{"id":"edit","proposal":{"kind":"initialize","definition":{}}}]),
        false,
    ) else {
        unreachable!()
    };
    let mut reply: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    reply["text"] = json!("");
    json(&reply)
}

// Value: protects=accepted setup edits finish without a second provider call;
// fails_when=an accepted edit replaces the model's answer with a review reminder.
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
    assert_eq!(report["answer"], "Private result");
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 1);
    assert_eq!(report["usage"]["provider_cost_nanos"], "1");
}

// Value: protects=tool-only providers can finish with an answer based on the accepted receipt;
// fails_when=the runner invents an answer or drops the receipt before the follow-up call.
#[tokio::test]
async fn an_accepted_tool_only_edit_gets_a_conversational_follow_up() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            if request["call_index"] == 1 {
                tool_only(call)
            } else {
                assert_eq!(request["call_index"], 2);
                let feedback = request["request"]["messages"]
                    .as_array()
                    .unwrap()
                    .last()
                    .unwrap();
                assert_eq!(feedback["role"], "tool");
                assert_eq!(feedback["call_id"], "edit");
                result(call, &json!([]), false)
            }
        }
        Verb::AuthoringActivity => json(&json!({"accepted":true,"replayed":false})),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (_, report) = run(&mut rig, lease()).await;
    assert_eq!(report["outcome"], "completed");
    assert_eq!(report["answer"], "Private result");
    assert_eq!(report["usage"]["calls"], 2);
}

// Value: protects=tool-only edits cannot bypass the physical-call or output allowance;
// fails_when=an empty accepted edit fabricates completion or authorizes another provider send.
#[tokio::test]
async fn a_tool_only_edit_without_a_follow_up_allowance_does_not_invent_an_answer() {
    for spent_output in [false, true] {
        let mut rig = rig(|call| match call.verb {
            Verb::AuthoringModel => tool_only(call),
            Verb::AuthoringActivity => json(&json!({"accepted":true,"replayed":false})),
            Verb::AuthoringReport => ack(),
            other => panic!("unexpected authority {other:?}"),
        });
        let mut configured = lease();
        if spent_output {
            configured.limits.output_per_turn = 2;
        } else {
            configured.limits.max_calls = 1;
        }
        let (_, report) = run(&mut rig, configured).await;
        assert_eq!(report["outcome"], "failed");
        assert!(report["answer"].is_null());
        assert_eq!(report["usage"]["knowledge"], "known");
        assert_eq!(report["usage"]["calls"], 1);
    }
}
