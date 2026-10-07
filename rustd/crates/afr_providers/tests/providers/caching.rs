//! What each wire sends to have a conversation's repeated prefix read from
//! the provider's cache: Messages marks it, Responses names it, Chat neither;
//! and that a follow-up sends that prefix again byte for byte.

use std::collections::HashMap;

use afd_wire::lease::Turn;
use afr_providers::{Connect as _, Message, Request, ToolSpec};
use afr_tools::catalog::UPDATE_PLAN;
use futures_util::StreamExt as _;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::ANSWER;
use super::support::wires::Wire;
use super::support::{Fake, connector, engine, lease, run};

/// The model every request here names.
const MODEL: &str = "model-1";
/// The fleet whose conversation every request here belongs to.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
/// The Responses field naming a conversation's prompt cache.
const PROMPT_CACHE_KEY: &str = "prompt_cache_key";
/// The field a Messages block or the request carries its cache marker in.
const CACHE_CONTROL: &str = "cache_control";
/// What the earlier lease asked, and what the follow-up asks after it.
const ASKED: &str = "what is 2+2?";
const FOLLOW_UP: &str = "and doubled?";
/// The wires that cache a conversation's repeated prefix: the fields a
/// follow-up must send unchanged, the cache's marker or key among them, and
/// the field the conversation grows in.
const CACHING: [(Wire, &[&str], &str); 2] = [
    (
        Wire::Messages,
        &["model", "tools", "system", CACHE_CONTROL],
        "messages",
    ),
    (
        Wire::Responses,
        &["model", "tools", "instructions", PROMPT_CACHE_KEY],
        "input",
    ),
];

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

/// A follow-up's first request repeats the earlier lease's on the wire, byte
/// for byte: the same model, tools, system prompt and cache marker or key,
/// and a conversation opening with every message the earlier request sent,
/// up to where the provider's moving breakpoint stood, so its cache serves
/// all of it.
#[tokio::test]
async fn test_history_prefix_is_byte_identical_on_the_wire() {
    for (wire, fixed, grows) in CACHING {
        let earlier = first_request(wire, ASKED, Vec::new()).await;
        let turn = Turn {
            message: ASKED.into(),
            answer: ANSWER.into(),
        };
        let follow_up = first_request(wire, FOLLOW_UP, vec![turn]).await;

        for field in fixed {
            assert_eq!(
                earlier[*field].get(),
                follow_up[*field].get(),
                "{wire:?}: {field}"
            );
        }
        let sent = earlier[grows].get();
        let open = sent.strip_suffix(']').unwrap();
        let repeated = follow_up[grows].get();
        assert!(
            repeated
                .strip_prefix(open)
                .is_some_and(|rest| rest.starts_with(',')),
            "{wire:?}: {repeated} does not open with {sent}"
        );
    }
}

/// The top-level fields of the first request `wire` sends for a lease asking
/// `message` after `history`, each as the bytes it arrived as.
async fn first_request(
    wire: Wire,
    message: &str,
    history: Vec<Turn<'static>>,
) -> HashMap<String, Box<RawValue>> {
    let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
    let mut leased = lease(&wire.provider(), &[UPDATE_PLAN.name()], message);
    leased.history = history;
    let _ran = run(&engine(&fake), &leased).await;
    serde_json::from_slice(&fake.seen().remove(0).raw).unwrap()
}
