//! What each wire sends to have a conversation's repeated prefix read from
//! the provider's cache: Messages marks it, Responses names it, Chat neither;
//! and that a follow-up sends that prefix again byte for byte.

use std::collections::HashMap;

use afd_wire::lease::Turn;
use afr_providers::{
    Chunk, Connect as _, Connector, Message, ProviderSpec, Registry, Request, ToolSpec,
};
use afr_tools::catalog::UPDATE_PLAN;
use futures_util::StreamExt as _;
use serde_json::value::RawValue;
use serde_json::{Value, json};

use super::ANSWER;
use super::support::wires::{CACHED_TOKENS, Wire};
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
/// provider's cache, and how many a turn here writes.
const CACHE_CREATION_INPUT_TOKENS: &str = "cache_creation_input_tokens";
const WRITTEN: u64 = 100;
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
async fn turn(connector: &Connector, provider: &str) -> Vec<afr_providers::Result<Chunk>> {
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
    };
    provider.stream(request).collect().await
}

/// A connector whose one route is a chat route at `fake` speaking
/// `OpenRouter`'s dialect.
fn openrouter(fake: &Fake) -> Connector {
    let spec = ProviderSpec {
        name: OPENROUTER.to_owned(),
        aliases: Vec::new(),
        wire: afr_providers::Wire::Chat,
        base_url: format!("{}/v1", fake.base),
        dialect: Some(OPENROUTER.to_owned()),
    };
    Connector::new(Registry::new([spec]).unwrap()).unwrap()
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

/// A Messages turn that wrote part of its prompt to the provider's cache
/// reports what it wrote apart from what it read.
#[tokio::test]
async fn test_messages_cache_writes_are_reported() {
    let Reply::Stream(mut events) = Wire::Messages.answer(ANSWER) else {
        panic!("an answer streams");
    };
    let (head, data) = events[0].split_once(DATA).unwrap();
    let mut start: Value = serde_json::from_str(data.trim_end()).unwrap();
    start["message"]["usage"][CACHE_CREATION_INPUT_TOKENS] = json!(WRITTEN);
    let writing = format!("{head}{DATA}{start}\n\n");
    events[0] = writing;
    let fake = Fake::serve(vec![Reply::Stream(events)]).await;

    let streamed = turn(&connector(&fake), &Wire::Messages.provider()).await;

    let spent: Vec<(u64, u64)> = streamed
        .into_iter()
        .filter_map(|chunk| match chunk {
            Ok(Chunk::Usage(usage)) => Some((usage.cache_written, usage.cached_input)),
            _ => None,
        })
        .collect();
    assert_eq!(spent, [(WRITTEN, CACHED_TOKENS)], "written, then read");
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
