//! How a turn ended, on the socket: the reasoning a provider signed showed
//! live as reasoning and goes back with the turn that made it, a call cut at
//! the output limit never runs, and a reply past the transport's cap ends the
//! run without being asked again.

use afd_wire::activity::{ActivityFrame, StreamTextKind};
use afd_wire::report::ResultOutcome;
use afd_wire::tool_trace::ToolCallStatus;
use afr_providers::REPLY_MAX_BYTES;
use afr_tools::ToolErrorCode;
use afr_tools::catalog::UPDATE_PLAN;
use serde_json::{Value, json};

use super::support::wires::{self, Wire};
use super::support::{Fake, Reply, engine, lease, run};
use super::{ANSWER, CALL_ID};

/// What the model thought before its call.
pub(crate) const THOUGHT: &str = "two and two make four";
/// The signature Messages seals that thought with.
pub(crate) const SIGNATURE: &str = "sig-a1b2";
/// The bytes one padding event spends: a Server-Sent Events comment, which
/// every wire reads past and shows nothing for.
const PAD_BYTES: usize = 64 * 1024;

/// A reply of comments alone that passes [`REPLY_MAX_BYTES`] by one event.
fn padding_past_the_cap() -> Reply {
    let comment = format!(": {}\n\n", "x".repeat(PAD_BYTES - 4));
    Reply::Stream(vec![comment; REPLY_MAX_BYTES / PAD_BYTES + 1])
}

/// The content of the last assistant message a Messages request carries.
fn last_turn(body: &Value) -> Vec<Value> {
    let messages = body["messages"].as_array().unwrap();
    let turn = messages
        .iter()
        .rev()
        .find(|message| message["role"] == "assistant");
    turn.and_then(|message| message["content"].as_array())
        .cloned()
        .unwrap()
}

/// The kind and text of the first chunk of model output a run showed live.
fn first_shown<'a>(frames: &'a [ActivityFrame<'_>]) -> Option<(Option<StreamTextKind>, &'a str)> {
    frames.iter().find_map(|frame| match frame {
        ActivityFrame::FleetResponseChunk(chunk) => Some((chunk.text_kind, chunk.text.as_ref())),
        ActivityFrame::ToolCallStarted(_)
        | ActivityFrame::ToolCallCompleted(_)
        | ActivityFrame::ToolCallProgress(_) => None,
    })
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

    let (output, frames) = run(&engine(&fake), &leased).await;

    assert_eq!(output.result.content, ANSWER);
    assert_eq!(
        first_shown(&frames),
        Some((Some(StreamTextKind::Reasoning), THOUGHT)),
        "the first chunk shown live is the thought, as reasoning"
    );
    let turn = last_turn(&fake.seen()[1].body);
    assert_eq!(turn[0]["type"], "thinking", "{turn:?}");
    assert_eq!(
        (turn[0]["thinking"].as_str(), turn[0]["signature"].as_str()),
        (Some(THOUGHT), Some(SIGNATURE))
    );
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
    assert_eq!(
        output.trace.unwrap().calls[0].status,
        ToolCallStatus::Failed
    );
    let answered = wire.results(&fake.seen()[1].body);
    let refused = format!("[{}] ", ToolErrorCode::OutputLimitReached);
    assert!(answered[0].starts_with(&refused), "{answered:?}");
}

// Nothing showed, so a cut would open the turn again; a reply at the cap is
// not a cut, since the next would be as long.
#[tokio::test]
async fn a_reply_past_the_cap_ends_the_run_as_the_fleets_error_and_is_sent_once() {
    let wire = Wire::Chat;
    let mut fake = Fake::serve(vec![padding_past_the_cap(), wire.answer(ANSWER)]).await;
    let leased = lease(&wire.provider(), &[], "hello");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let ResultOutcome::Failed(failure) = output.result.outcome else {
        panic!("a reply past the cap is no answer");
    };
    assert_eq!(failure.class, None, "the fleet's error");
    assert!(
        failure.detail.contains(&REPLY_MAX_BYTES.to_string()),
        "{}",
        failure.detail
    );
    assert_eq!(fake.seen().len(), 1, "never asked again");
}
