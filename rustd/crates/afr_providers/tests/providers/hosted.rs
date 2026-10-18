//! A tool the provider runs itself: offered as the wire's own spec where the
//! wire has one, and refused back to the model where it has none.

use afd_wire::tool_trace::ToolCallStatus;
use afr_tools::catalog::WEB_SEARCH;
use serde_json::json;

use super::support::wires::Wire;
use super::support::{Fake, engine, lease, run};
use super::{ANSWER, CALL_ID};

/// The code the router refuses a hosted tool's call with on a wire that
/// cannot host it.
const HOSTED_REFUSAL: &str = "[hosted_tool_unavailable]";

#[tokio::test]
async fn test_web_search_is_a_hosted_spec() {
    for wire in [Wire::Messages, Wire::Responses] {
        let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
        let leased = lease(&wire.provider(), &[WEB_SEARCH.name()], "search");

        let (output, _frames) = run(&engine(&fake), &leased).await;

        assert_eq!(output.result.content, ANSWER, "{wire:?}");
        let offered = wire.offered(&fake.seen()[0].body);
        let spec = offered
            .iter()
            .find(|tool| tool.starts_with(WEB_SEARCH.name()));
        assert!(spec.is_some(), "{wire:?}: {offered:?}");
    }
}

/// Chat offers no hosted spec, so a stray call is refused by the router. A
/// request that declares no tools carries its calls and results as text
/// (rig's history shaping), so the refusal reaches the model as the user turn
/// after the call, never as a tool message the provider would refuse.
#[tokio::test]
async fn a_hosted_call_on_chat_is_refused_to_the_model_as_text() {
    let wire = Wire::Chat;
    let mut fake = Fake::serve(vec![
        wire.call(CALL_ID, WEB_SEARCH.name(), &json!({"query": "x"})),
        wire.answer(ANSWER),
    ])
    .await;
    let leased = lease(&wire.provider(), &[WEB_SEARCH.name()], "search");

    let (output, _frames) = run(&engine(&fake), &leased).await;

    let seen = fake.seen();
    assert!(
        wire.offered(&seen[0].body).is_empty(),
        "chat has no hosted spec"
    );
    assert!(
        wire.results(&seen[1].body).is_empty(),
        "no tool message without a declared tool: {}",
        seen[1].body
    );
    let refusal = format!("[tool {} result] {HOSTED_REFUSAL}", WEB_SEARCH.name());
    let answered = seen[1].body["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .and_then(|message| message["content"].as_str());
    assert!(
        answered.is_some_and(|text| text.starts_with(&refusal)),
        "{}",
        seen[1].body
    );
    assert_eq!(
        output.trace.unwrap().calls[0].status,
        ToolCallStatus::Failed
    );
}
