//! How a turn ended, on the socket: the reasoning a provider signed goes back
//! with the turn that made it, and a call cut at the output limit never runs.

use afd_wire::tool_trace::ToolCallStatus;
use afr_tools::ToolErrorCode;
use afr_tools::catalog::UPDATE_PLAN;
use serde_json::{Value, json};

use super::support::wires::{self, Wire};
use super::support::{Fake, engine, lease, run};
use super::{ANSWER, CALL_ID};

/// What the model thought before its call.
const THOUGHT: &str = "two and two make four";
/// The signature Messages seals that thought with.
const SIGNATURE: &str = "sig-a1b2";

/// The content of the last assistant message a Messages request carries.
fn last_turn(body: &Value) -> Vec<Value> {
    let messages = body["messages"].as_array().unwrap();
    let turn = messages.iter().rev().find(|message| message["role"] == "assistant");
    turn.and_then(|message| message["content"].as_array()).cloned().unwrap()
}

#[tokio::test]
async fn a_turns_signed_thinking_goes_back_ahead_of_its_call() {
    let wire = Wire::Messages;
    let arguments = json!({"expression": "2+2"});
    let mut fake = Fake::serve(vec![
        wires::thinking_call(THOUGHT, SIGNATURE, CALL_ID, UPDATE_PLAN.name(), &arguments),
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[UPDATE_PLAN.name()], "what is 2+2?");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    assert_eq!(output.result.content, ANSWER);
    let turn = last_turn(&fake.seen()[1].body);
    assert_eq!(turn[0]["type"], "thinking", "{turn:?}");
    assert_eq!((turn[0]["thinking"].as_str(), turn[0]["signature"].as_str()), (Some(THOUGHT), Some(SIGNATURE)));
    assert_eq!(turn[1]["type"], "tool_use", "{turn:?}");
}

#[tokio::test]
async fn a_call_cut_at_the_output_limit_is_answered_and_never_run() {
    let wire = Wire::Messages;
    let mut fake = Fake::serve(vec![
        wires::cut_call(CALL_ID, UPDATE_PLAN.name(), &json!({"expression": "2+"})),
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[UPDATE_PLAN.name()], "what is 2+2?");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    assert_eq!(output.result.content, ANSWER, "the run went on");
    assert_eq!(output.trace.unwrap().calls[0].status, ToolCallStatus::Failed);
    let answered = wire.results(&fake.seen()[1].body);
    let refused = format!("[{}] ", ToolErrorCode::OutputLimitReached);
    assert!(answered[0].starts_with(&refused), "{answered:?}");
}
