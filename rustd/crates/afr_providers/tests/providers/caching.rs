//! What each wire sends to have a conversation's repeated prefix read from
//! the provider's cache: Messages marks it, Responses names it, Chat neither;
//! and that a follow-up sends that prefix again byte for byte.

use std::collections::HashMap;

use afd_wire::lease::Turn;
use afr_providers::{Chunk, Connect, Message, Request, ToolSpec};
use afr_tools::catalog::UPDATE_PLAN;
use futures_util::StreamExt as _;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::ANSWER;
use super::support::wires::{CACHED_TOKENS, PROMPT_TOKENS, Wire};
use super::support::{Fake, Reply, connector, engine, lease, run};

/// The model every request here names.
const MODEL: &str = "model-1";
/// The system prompt every request here sends.
const INSTRUCTIONS: &str = "Read the run.";
/// The fleet whose conversation every request here belongs to.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";
/// The Responses field naming a conversation's prompt cache.
const PROMPT_CACHE_KEY: &str = "prompt_cache_key";
/// The field a Messages block or the request carries its cache marker in.
const CACHE_CONTROL: &str = "cache_control";
/// What the earlier lease asked, and what the follow-up asks after it.
const ASKED: &str = "what is 2+2?";
const FOLLOW_UP: &str = "and doubled?";
/// The Messages usage field counting the prompt tokens a turn wrote to the
/// provider's cache, the field Responses and Chat count them in, how many a
/// turn here writes, and how Messages splits them by lifetime.
const CACHE_CREATION_INPUT_TOKENS: &str = "cache_creation_input_tokens";
const CACHE_WRITE_TOKENS: &str = "cache_write_tokens";
const WRITTEN: u64 = 100;
const WRITTEN_5M: u64 = 60;
const WRITTEN_1H: u64 = 40;
/// Where a Server-Sent Event's payload starts.
const DATA: &str = "data: ";
/// The name, and the rig dialect, of a chat route speaking `OpenRouter`'s
/// quirks: the one chat dialect that can mark a cache.
const OPENROUTER: &str = "openrouter";
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
    let _drained = turn(&connector(&fake), &wire.provider()).await;
    fake.seen().remove(0).body
}

/// Every chunk of one turn asking [`ASKED`] and offering one tool, sent to
/// `provider` through `connector`.
async fn turn(connector: &impl Connect, provider: &str) -> Vec<afr_providers::Result<Chunk>> {
    let leased = lease(provider, &[], ASKED);
    let provider = connector.connect(&leased).unwrap();
    let parameters = json!({ "type": "object", "properties": {} });
    let tools = [ToolSpec {
        name: UPDATE_PLAN.name(),
        description: "Record the plan.",
        parameters: &parameters,
    }];
    let messages = [Message::User(ASKED.to_owned())];
    let request = Request {
        model: MODEL,
        instructions: INSTRUCTIONS,
        messages: &messages,
        tools: &tools,
        hosted: &[],
        cache_key: FLEET,
        max_output_tokens: None,
        single_attempt: false,
    };
    provider.stream(request).collect().await
}

/// A connector whose one route is a chat route at `fake` speaking
/// `OpenRouter`'s dialect.
fn openrouter(fake: &Fake) -> super::support::FixtureConnector {
    super::support::FixtureConnector::new(fake, Some(OPENROUTER.to_owned()))
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

/// The Responses key is the lease's fleet, as the loop reads it off the
/// lease: every conversation of one fleet shares a cache, and no other
/// fleet's does.
#[tokio::test]
async fn test_responses_cache_key_is_the_fleet_id() {
    let wire = Wire::Responses;
    let mut fake = Fake::serve(vec![wire.answer(ANSWER)]).await;
    let mut leased = lease(&wire.provider(), &[], ASKED);
    leased.event.fleet_id = FLEET.into();

    let _ran = run(&engine(&fake), &leased).await;

    let body = fake.seen().remove(0).body;
    assert_eq!(body[PROMPT_CACHE_KEY], FLEET, "{body}");
}

/// Chat Completions carries no cache marker and no cache key: not through a
/// plain gateway, and not under `OpenRouter`'s dialect, which could mark one.
#[tokio::test]
async fn test_chat_completions_sends_no_cache_marker() {
    let mut fake = Fake::serve(vec![Wire::Chat.answer(ANSWER); 2]).await;
    let routes = [
        (connector(&fake), Wire::Chat.provider()),
        (openrouter(&fake), OPENROUTER.to_owned()),
    ];

    for (connector, provider) in &routes {
        let _drained = turn(connector, provider).await;
    }

    let bodies: Vec<Value> = fake.seen().into_iter().map(|seen| seen.body).collect();
    assert_eq!(bodies.len(), routes.len(), "one request per route");
    for body in bodies {
        assert_eq!(body["model"], MODEL, "a chat body: {body}");
        let sent = body.to_string();
        assert!(!sent.contains(CACHE_CONTROL), "{sent}");
        assert!(!sent.contains(PROMPT_CACHE_KEY), "{sent}");
    }
}

/// Each wire's turn that wrote part of its prompt to the provider's cache
/// reports what it wrote apart from what it read and from fresh input, by the
/// lifetime the wire names: Messages splits five minutes from one hour,
/// Responses and Chat name none.
#[tokio::test]
async fn test_cache_writes_are_reported_by_lifetime() {
    let fresh = PROMPT_TOKENS - CACHED_TOKENS;
    let wires = [
        (Wire::Messages, (0, WRITTEN_5M, WRITTEN_1H)),
        (Wire::Responses, (WRITTEN, 0, 0)),
        (Wire::Chat, (WRITTEN, 0, 0)),
    ];
    for (wire, lifetimes) in wires {
        let Reply::Stream(events) = wire.answer(ANSWER) else {
            panic!("an answer streams");
        };
        let writing = events.iter().map(|event| writes(wire, event)).collect();
        let fake = Fake::serve(vec![Reply::Stream(writing)]).await;

        let streamed = turn(&connector(&fake), &wire.provider()).await;

        let spent: Vec<_> = streamed
            .into_iter()
            .filter_map(|chunk| match chunk {
                Ok(Chunk::Usage(usage)) => Some(usage),
                _ => None,
            })
            .collect();
        let [usage] = spent.as_slice() else {
            panic!("{wire:?}: one usage chunk, got {spent:?}");
        };
        assert_eq!(
            (usage.input, usage.cached_input),
            (fresh, CACHED_TOKENS),
            "{wire:?}: fresh input, then reads"
        );
        assert_eq!(
            (
                usage.cache_write,
                usage.cache_write_5m,
                usage.cache_write_1h
            ),
            lifetimes,
            "{wire:?}: writes by lifetime"
        );
    }
}

/// `event`, one Server-Sent Event of a `wire` turn, reporting [`WRITTEN`]
/// cache-write tokens where that wire's provider reports them.
fn writes(wire: Wire, event: &str) -> String {
    let Some((head, data)) = event.split_once(DATA) else {
        return event.to_owned();
    };
    let Ok(mut payload) = serde_json::from_str::<Value>(data.trim_end()) else {
        return event.to_owned();
    };
    let usage = match wire {
        Wire::Messages => payload.pointer_mut("/message/usage"),
        Wire::Responses => payload.pointer_mut("/response/usage"),
        Wire::Chat => payload.get_mut("usage"),
    };
    let Some(usage @ Value::Object(_)) = usage else {
        return event.to_owned();
    };
    match wire {
        Wire::Messages => {
            usage[CACHE_CREATION_INPUT_TOKENS] = json!(WRITTEN);
            usage["cache_creation"] = json!({"ephemeral_5m_input_tokens": WRITTEN_5M,
                "ephemeral_1h_input_tokens": WRITTEN_1H});
        }
        Wire::Responses => {
            usage["input_tokens"] = json!(PROMPT_TOKENS + WRITTEN);
            usage["input_tokens_details"][CACHE_WRITE_TOKENS] = json!(WRITTEN);
        }
        Wire::Chat => {
            usage["prompt_tokens"] = json!(PROMPT_TOKENS + WRITTEN);
            usage["prompt_tokens_details"][CACHE_WRITE_TOKENS] = json!(WRITTEN);
        }
    }
    format!("{head}{DATA}{payload}\n\n")
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
