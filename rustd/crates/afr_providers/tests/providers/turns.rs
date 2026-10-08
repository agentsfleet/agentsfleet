//! One turn streamed straight from the provider a lease connects to, with no
//! loop above it to stop reading at the first failure: a conversation that
//! cannot be sent, a call whose arguments are not JSON, an output item the
//! wire does not model, and the reasoning a turn hands back.

use afd_wire::activity::StreamTextKind;
use afr_providers::{Call, Chunk, Connect as _, Message, Request};
use afr_tools::catalog::UPDATE_PLAN;
use futures_util::StreamExt as _;
use serde_json::{Value, json};

use super::ends::{SIGNATURE, THOUGHT};
use super::support::wires::{self, Wire};
use super::support::{Fake, Reply, connector, lease};
use super::{ANSWER, CALL_ID};

/// The model every turn here names.
const MODEL: &str = "model-1";
/// What the user asked.
const QUESTION: &str = "what is 2+2?";
/// The fleet whose conversation every request here belongs to.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
/// More items than any turn here yields: a stream that reaches it never ends.
const BOUND: usize = 16;

/// Every item of one turn over `wire` against `fake`, continuing `messages`,
/// read to its end or to [`BOUND`].
async fn streamed(
    fake: &Fake,
    wire: Wire,
    messages: &[Message],
) -> Vec<afr_providers::Result<Chunk>> {
    let leased = lease(&wire.provider(), &[], QUESTION);
    let provider = connector(fake).connect(&leased).unwrap();
    let request = Request {
        model: MODEL,
        instructions: "Read the run.",
        messages,
        tools: &[],
        hosted: &[],
        cache_key: FLEET,
    };
    provider.stream(request).take(BOUND).collect().await
}

/// The chunks of a turn that streamed no failure.
fn chunks(streamed: Vec<afr_providers::Result<Chunk>>) -> Vec<Chunk> {
    streamed.into_iter().map(Result::unwrap).collect()
}

/// A conversation of the question alone.
fn asked() -> [Message; 1] {
    [Message::User(QUESTION.to_owned())]
}

#[tokio::test]
async fn a_conversation_that_cannot_be_sent_fails_the_turn_and_nothing_follows() {
    let mut fake = Fake::serve(vec![Wire::Chat.answer(ANSWER)]).await;
    let orphaned = [
        Message::User(QUESTION.to_owned()),
        Message::ToolResult {
            call_id: CALL_ID.to_owned(),
            output: ANSWER.to_owned(),
            image: None,
        },
    ];

    let streamed = streamed(&fake, Wire::Chat, &orphaned).await;

    let [Err(refused)] = streamed.as_slice() else {
        panic!("one failure and nothing after it: {streamed:?}");
    };
    assert!(refused.detail().contains(CALL_ID), "{}", refused.detail());
    assert!(fake.seen().is_empty(), "nothing was asked");
}

#[tokio::test]
async fn a_call_whose_arguments_are_not_json_goes_out_as_their_text_and_ends_the_turn() {
    let raw = r#"{"expression": "2+"#;
    let fake = Fake::serve(vec![wires::raw_call(CALL_ID, UPDATE_PLAN.name(), raw)]).await;

    let streamed = chunks(streamed(&fake, Wire::Messages, &asked()).await);

    let call = Call {
        id: CALL_ID.to_owned(),
        name: UPDATE_PLAN.name().to_owned(),
        arguments: Value::String(raw.to_owned()),
    };
    assert!(
        matches!(
            streamed.as_slice(),
            [Chunk::Call(sent), Chunk::Usage(_), Chunk::End(_)] if *sent == call
        ),
        "{streamed:?}"
    );
}

// Responses sends a hosted tool's result, such as a web search's, as an output
// item rig does not model and hands on as unknown.
#[tokio::test]
async fn an_output_item_the_wire_does_not_model_shows_nothing() {
    let wire = Wire::Responses;
    let Reply::Stream(mut events) = wire.answer(ANSWER) else {
        panic!("an answer streams");
    };
    let searched = json!({"type": "response.output_item.done", "output_index": 1,
        "sequence_number": 1,
        "item": {"type": "web_search_call", "id": "ws_1", "status": "completed"}});
    events.insert(
        1,
        format!("event: response.output_item.done\ndata: {searched}\n\n"),
    );
    let fake = Fake::serve(vec![Reply::Stream(events)]).await;

    let streamed = chunks(streamed(&fake, wire, &asked()).await);

    let shown = Chunk::Text {
        kind: StreamTextKind::Answer,
        text: ANSWER.to_owned(),
    };
    assert!(
        matches!(
            streamed.as_slice(),
            [text, Chunk::Usage(_), Chunk::End(_)] if *text == shown
        ),
        "{streamed:?}"
    );
}

#[tokio::test]
async fn a_turn_hands_back_its_signed_thinking_and_a_bare_answer_hands_back_nothing() {
    let wire = Wire::Messages;
    let arguments = json!({"expression": "2+2"});
    let fake = Fake::serve(vec![
        wires::thinking_call(THOUGHT, SIGNATURE, CALL_ID, UPDATE_PLAN.name(), &arguments),
        wire.answer(ANSWER),
    ])
    .await;

    let thought = chunks(streamed(&fake, wire, &asked()).await);
    let answered = chunks(streamed(&fake, wire, &asked()).await);

    let nothing_back = |turn: &[Chunk]| match turn.last() {
        Some(Chunk::End(end)) => Some(end.replay.is_empty()),
        _ => None,
    };
    assert_eq!(
        (nothing_back(&thought), nothing_back(&answered)),
        (Some(false), Some(true))
    );
}
