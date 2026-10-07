//! What each wire sends to have a conversation's repeated prefix read from
//! the provider's cache: Messages marks it, Responses names it, Chat neither.

use afr_providers::{Connect as _, Message, Request, ToolSpec};
use futures_util::StreamExt as _;
use serde_json::{Value, json};

use super::ANSWER;
use super::support::wires::Wire;
use super::support::{Fake, connector, lease};

/// The model every request here names.
const MODEL: &str = "model-1";
/// The fleet whose conversation every request here belongs to.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
/// The Responses field naming a conversation's prompt cache.
const PROMPT_CACHE_KEY: &str = "prompt_cache_key";
/// The field a Messages block or the request carries its cache marker in.
const CACHE_CONTROL: &str = "cache_control";

/// The body `wire` sends for one turn offering one tool.
async fn sent(wire: Wire) -> Value {
    let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
    let leased = lease(&wire.provider(), &[], "what is 2+2?");
    let provider = connector(&fake).connect(&leased).unwrap();
    let parameters = json!({ "type": "object", "properties": {} });
    let tools = [ToolSpec {
        name: "update_plan",
        description: "Record the plan.",
        parameters: &parameters,
    }];
    let messages = [Message::User("what is 2+2?".to_owned())];
    let request = Request {
        model: MODEL,
        instructions: "Read the run.",
        messages: &messages,
        tools: &tools,
        hosted: &[],
        cache_key: FLEET,
    };
    let _drained: Vec<_> = provider.stream(request).collect().await;
    fake.seen().remove(0).body
}

/// A Messages request marks its last tool and its system prompt, keeps the
/// top-level breakpoint the provider moves, and names no lifetime.
#[tokio::test]
async fn test_messages_cache_marks_the_static_prefix() {
    let body = sent(Wire::Messages).await;

    let last_tool = body["tools"].as_array().and_then(|tools| tools.last());
    assert!(
        last_tool.is_some_and(|tool| tool.get(CACHE_CONTROL).is_some()),
        "the last tool is marked: {body}"
    );
    let system = body["system"].as_array().and_then(|blocks| blocks.last());
    assert!(
        system.is_some_and(|block| block.get(CACHE_CONTROL).is_some()),
        "the system prompt is marked: {body}"
    );
    assert!(
        body.get(CACHE_CONTROL).is_some(),
        "the moving breakpoint stays: {body}"
    );
    assert!(
        !body.to_string().contains("\"ttl\""),
        "the default lifetime: {body}"
    );
}

/// The fleet's cache key rides the Responses wire, and only that wire.
#[tokio::test]
async fn test_cache_key_rides_responses_only() {
    for wire in Wire::ALL {
        let body = sent(wire).await;
        let key = body.get(PROMPT_CACHE_KEY).and_then(Value::as_str);
        let expected = matches!(wire, Wire::Responses).then_some(FLEET);
        assert_eq!(key, expected, "{wire:?}: {body}");
    }
}
