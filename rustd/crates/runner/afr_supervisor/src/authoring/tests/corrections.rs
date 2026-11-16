//! Invalid model edits receive feedback without acquiring proposal authority.

use super::{ack, json, lease, result, rig, run, run_logged};
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
                    "proposal":{"kind":"initialize","definition":{"schema_version":1,"description":"Answers support questions","instructions_md":"Reply briefly.","config":{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}}}}]),
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

// Value: protects=the checker's named field reaches the model instead of the general rules;
// fails_when=a refusal's detail is dropped and the model is left guessing which key to fix.
#[tokio::test]
async fn a_refusal_naming_a_field_tells_the_model_that_field() {
    const DETAIL: &str = "`budget` belongs under the `x-agentsfleet` block, not at the top level";
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            if request["call_index"] == 1 {
                return result(
                    call,
                    &json!([{"id":"bad-1","reason":"invalid_arguments",
                        "detail":{"field":"budget","text":DETAIL}}]),
                    false,
                );
            }
            let messages = request["request"]["messages"].as_array().unwrap();
            let heard = messages.last().unwrap()["text"].as_str().unwrap();
            assert_eq!(
                heard,
                format!(
                    "No changes were applied. The setup is invalid: {DETAIL}. Fix that, keep only the settings the person asked for, and return a corrected propose_setup call."
                )
            );
            result(
                call,
                &json!([{"id":"corrected",
                    "proposal":{"kind":"initialize","definition":{"schema_version":1,"description":"Answers support questions","instructions_md":"Reply briefly.","config":{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}}}}]),
                false,
            )
        }
        Verb::AuthoringActivity => super::json(&json!({"accepted":true,"replayed":false})),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (_calls, report, logs) = run_logged(&mut rig, lease()).await;
    assert_eq!(report["outcome"], "completed");
    assert_eq!(report["usage"]["calls"], 2);
    // The log names the setting and the kind of refusal; the checker's sentence, which can
    // quote what the person wrote, only the model hears.
    assert!(logs.contains(r#""refusal":"invalid_arguments""#), "{logs}");
    assert!(logs.contains(r#""field":"budget""#), "{logs}");
    assert!(logs.contains(r#""outcome":"accepted""#), "{logs}");
    assert!(!logs.contains("belongs under"), "{logs}");
}

// Value: protects=a first setup the daemon would refuse for its length is told so before it is sent;
// fails_when=the runner submits an oversized first setup, or its refusal reaches the log with the sentence.
#[tokio::test]
async fn an_oversized_first_setup_is_refused_by_the_runner_without_asking_the_daemon() {
    let oversized = "x".repeat(af_fleet_runtime::definition::MAX_INITIAL_INSTRUCTIONS_CHARS + 1);
    let mut rig = rig(move |call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            if request["call_index"] == 1 {
                return result(
                    call,
                    &json!([{"id":"long-1",
                        "proposal":{"kind":"initialize","definition":{"schema_version":1,"description":"Answers support questions","instructions_md":oversized,"config":{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}}}}]),
                    false,
                );
            }
            let messages = request["request"]["messages"].as_array().unwrap();
            let heard = messages.last().unwrap()["text"].as_str().unwrap();
            assert!(
                heard.starts_with("No changes were applied. The setup is invalid: "),
                "{heard}"
            );
            result(
                call,
                &json!([{"id":"short-2",
                    "proposal":{"kind":"initialize","definition":{"schema_version":1,"description":"Answers support questions","instructions_md":"Reply briefly.","config":{"name":"fixture","x-agentsfleet":{"triggers":[],"tools":[],"budget":{"daily_nanos":"1000000000"}}}}}}]),
                false,
            )
        }
        Verb::AuthoringActivity => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            assert_eq!(
                request["event"]["proposal"]["definition"]["instructions_md"], "Reply briefly.",
                "the oversized setup reached the daemon"
            );
            super::json(&json!({"accepted":true,"replayed":false}))
        }
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let (calls, report, logs) = run_logged(&mut rig, lease()).await;
    assert_eq!(report["outcome"], "completed");
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringActivity)
            .count(),
        1
    );
    assert!(
        logs.contains(r#""refusal":"invalid_first_setup""#),
        "{logs}"
    );
    assert!(
        !logs.contains("xxxxxxxx"),
        "the log carries the oversized text"
    );
}

// Value: protects=a refused proposal gets one corrected attempt and its usage is kept;
// fails_when=rejections are retried past that attempt or reported as a completed answer.
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
        2
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_failed");
    assert_eq!(report["usage"]["knowledge"], "known");
    assert_eq!(report["usage"]["calls"], 2);
    assert_eq!(report["usage"]["provider_cost_nanos"], "2");
    assert!(report.get("answer").is_none_or(Value::is_null));
    assert_eq!(rig.prepared.load(Ordering::SeqCst), 0);
    assert_eq!(rig.runs.load(Ordering::SeqCst), 0);
}

// Value: protects=a message that asks for no change is answered in text however many proposals come first;
// fails_when=a refusal that asks for an answer spends the one correction and fails a typed Save.
#[tokio::test]
async fn refusals_that_ask_for_an_answer_do_not_spend_the_one_correction() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            match request["call_index"].as_u64().unwrap() {
                1 => rejected(call, "invalid_arguments"),
                2 | 3 => rejected(call, "edit_not_requested"),
                4 => result(call, &json!([]), false),
                index => panic!("unexpected model call {index}"),
            }
        }
        Verb::AuthoringReport => ack(),
        other => panic!("a refused proposal acquired authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringModel)
            .count(),
        4
    );
    assert_eq!(report["outcome"], "completed");
    assert_eq!(report["answer"], "Private result");
    assert_eq!(report["usage"]["calls"], 4);
}

// Value: protects=the one correction still ends a message that keeps sending bad proposals;
// fails_when=an answer request widens the allowance for the refusals after it.
#[tokio::test]
async fn a_second_format_refusal_ends_the_message_even_after_an_answer_request() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => {
            let request: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            match request["call_index"].as_u64().unwrap() {
                1 => rejected(call, "edit_not_requested"),
                2 | 3 => rejected(call, "invalid_arguments"),
                index => panic!("unexpected model call {index}"),
            }
        }
        Verb::AuthoringReport => ack(),
        other => panic!("a refused proposal acquired authority {other:?}"),
    });
    let (calls, report) = run(&mut rig, lease()).await;
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.verb == Verb::AuthoringModel)
            .count(),
        3
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_failed");
    assert!(report.get("answer").is_none_or(Value::is_null));
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
