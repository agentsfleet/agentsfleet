#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test module: a failed precondition should fail the test loudly"
)]

use afr_tools::catalog::WEB_SEARCH;
use serde_json::json;

use super::{API_VERSION, HEADER_KEY, HEADER_VERSION, MAX_TOKENS, Messages, WEB_SEARCH_TYPE};
use crate::dialect::{Decode as _, Dialect as _};
use crate::provider::{Call, Chunk, Usage};
use crate::test_support::{
    CALL_ID, FOLLOW_UP, INSTRUCTIONS, MODEL, OUTPUT, PREAMBLE, QUESTION, TOOL, body, calculator,
    conversation, decode, parameters, request,
};

#[test]
fn should_send_the_conversation_as_alternating_turns_with_two_cache_breakpoints() {
    let messages = conversation();
    let parameters = parameters();
    let tools = [calculator(&parameters)];

    let sent = body(&Messages, &request(&messages, &tools, &[&WEB_SEARCH]));

    assert_eq!(sent["model"], MODEL);
    assert_eq!(sent["max_tokens"], MAX_TOKENS);
    assert_eq!(sent["stream"], true);
    assert_eq!(
        sent["system"],
        json!([{"type": "text", "text": INSTRUCTIONS, "cache_control": {"type": "ephemeral"}}])
    );
    assert_eq!(
        sent["messages"],
        json!([
            {"role": "user", "content": [{"type": "text", "text": QUESTION}]},
            {"role": "assistant", "content": [
                {"type": "text", "text": PREAMBLE},
                {"type": "tool_use", "id": CALL_ID, "name": TOOL, "input": {"expression": "2+2"}}
            ]},
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": CALL_ID, "content": OUTPUT},
                {"type": "text", "text": FOLLOW_UP, "cache_control": {"type": "ephemeral"}}
            ]}
        ]),
        "the result and the user message after it share one user turn"
    );
    assert_eq!(
        sent["tools"],
        json!([
            {"name": TOOL, "description": TOOL, "input_schema": parameters},
            {"type": WEB_SEARCH_TYPE, "name": WEB_SEARCH.name()}
        ])
    );
}

#[test]
fn should_send_no_system_and_no_tools_when_there_are_none() {
    let messages = conversation();
    let mut turn = request(&messages, &[], &[]);
    turn.instructions = "";

    let sent = body(&Messages, &turn);

    assert!(sent.get("system").is_none() && sent.get("tools").is_none());
}

#[test]
fn should_carry_the_key_and_version_in_their_headers() {
    let client = reqwest::Client::new();

    let built = Messages
        .authorize(client.post("http://localhost/"), "sk-ant-key")
        .build()
        .unwrap();

    assert_eq!(built.headers()[HEADER_KEY], "sk-ant-key");
    assert_eq!(built.headers()[HEADER_VERSION], API_VERSION);
    assert!(
        built
            .headers()
            .get(reqwest::header::AUTHORIZATION)
            .is_none()
    );
}

#[test]
fn should_read_text_reasoning_a_whole_call_and_the_turns_usage() {
    let mut decoder = Messages.decoder();

    let chunks = decode(
        &mut decoder,
        [
            json!({"type": "message_start", "message": {"usage":
                {"input_tokens": 10, "cache_read_input_tokens": 2, "cache_creation_input_tokens": 1}}}),
            json!({"type": "ping"}),
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "hm"}}),
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "text_delta", "text": "Hi"}}),
            json!({"type": "content_block_start", "index": 2, "content_block":
                {"type": "tool_use", "id": CALL_ID, "name": TOOL, "input": {}}}),
            json!({"type": "content_block_delta", "index": 2, "delta":
                {"type": "input_json_delta", "partial_json": "{\"expression\":"}}),
            json!({"type": "content_block_delta", "index": 2, "delta":
                {"type": "input_json_delta", "partial_json": "\"2+2\"}"}}),
            json!({"type": "content_block_stop", "index": 2}),
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 7}}),
        ],
    )
    .unwrap();

    assert!(!decoder.ended(), "no message_stop yet");
    assert_eq!(
        chunks,
        [
            Chunk::Usage(Usage {
                input: 13,
                cached_input: 2,
                output: 0
            }),
            Chunk::reasoning("hm".to_owned()),
            Chunk::answer("Hi".to_owned()),
            Chunk::Call(Call {
                id: CALL_ID.to_owned(),
                name: TOOL.to_owned(),
                arguments: json!({"expression": "2+2"}),
            }),
            Chunk::Usage(Usage {
                output: 7,
                ..Usage::default()
            }),
        ]
    );
    decode(&mut decoder, [json!({"type": "message_stop"})]).unwrap();
    assert!(decoder.ended());
}

#[test]
fn should_leave_a_server_tool_to_the_provider() {
    let mut decoder = Messages.decoder();

    let chunks = decode(
        &mut decoder,
        [
            json!({"type": "content_block_start", "index": 0, "content_block":
                {"type": "server_tool_use", "id": "srv-1", "name": "web_search"}}),
            json!({"type": "content_block_delta", "index": 0, "delta":
                {"type": "input_json_delta", "partial_json": "{\"query\":\"x\"}"}}),
            json!({"type": "content_block_stop", "index": 0}),
        ],
    )
    .unwrap();

    assert!(
        chunks.is_empty(),
        "a server tool's call is not the router's"
    );
}

#[test]
fn should_end_the_turn_with_the_providers_error_name() {
    let failure = decode(
        &mut Messages.decoder(),
        [json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy"}})],
    )
    .unwrap_err();

    assert!(
        failure.detail().ends_with("overloaded_error"),
        "{}",
        failure.detail()
    );
    assert!(
        !failure.detail().contains("busy"),
        "never the provider's message"
    );
}

#[test]
fn should_refuse_an_event_that_is_not_its_json() {
    let failure = decode(&mut Messages.decoder(), [json!({"type": "message_start"})]).unwrap_err();

    assert_eq!(failure.failure_class(), None);
    assert!(failure.detail().contains("could not be written or read"));
}
